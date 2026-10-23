//! Scadenzario/agenda: rate, fatture da pagare, ritenute, prescrizioni, assemblee, adempimenti fiscali e promemoria manuali.

use crate::db::Db;
use crate::util::*;
use anyhow::{bail, Result};
use chrono::{Datelike, NaiveDate};
use rusqlite::params;

#[derive(Clone, Debug)]
pub struct Scadenza {
    pub data: String, // ISO
    pub tipo: &'static str,
    pub titolo: String,
    pub dettaglio: String,
    pub importo: Option<Cents>,
    pub scaduta: bool,
    /// Id dell'elemento di agenda (solo tipo "agenda").
    pub agenda_id: Option<i64>,
}

impl Db {
    pub fn aggiungi_agenda(&self, cond: i64, data: &str, titolo: &str, note: &str) -> Result<i64> {
        let Some(d) = data_iso(data) else { bail!("Data non valida (usa GG/MM/AAAA)") };
        if titolo.trim().is_empty() {
            bail!("Il titolo è obbligatorio");
        }
        self.conn.execute("INSERT INTO agenda(condominio_id,data,titolo,note) VALUES(?1,?2,?3,?4)", params![cond, d, titolo.trim(), note])?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn completa_agenda(&self, id: i64, completata: bool) -> Result<()> {
        self.conn.execute("UPDATE agenda SET completata=?2 WHERE id=?1", params![id, completata as i64])?;
        Ok(())
    }

    pub fn elimina_agenda(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM agenda WHERE id=?1", [id])?;
        Ok(())
    }

    /// Giorni dopo la data fattura in cui si presume la scadenza di pagamento (impostazione, default 30).
    pub fn giorni_pagamento_fatture(&self) -> i64 {
        self.leggi_imp("giorni_pagamento_fatture").parse().unwrap_or(30).clamp(0, 365)
    }

    /// Scadenze fino a `fino_a` (incluse quelle già scadute e non evase), in ordine di data.
    pub fn scadenzario(&self, cond: i64, oggi: NaiveDate, fino_a: NaiveDate) -> Result<Vec<Scadenza>> {
        let iso = |d: NaiveDate| d.format("%Y-%m-%d").to_string();
        let limite = iso(fino_a);
        let mut out: Vec<Scadenza> = vec![];
        let scaduta = |d: &str| parse_data(d).map(|x| x < oggi).unwrap_or(false);

        // rate non incassate: una riga per piano e data di scadenza
        let mut st = self.conn.prepare(
            "SELECT r.scadenza,p.nome,COUNT(DISTINCT r.persona_id),
                    SUM(r.importo_cents - COALESCE((SELECT SUM(a.importo_cents) FROM allocazione a JOIN incasso i ON i.id=a.incasso_id
                                                    WHERE a.rata_id=r.id AND i.stato='valido'),0))
             FROM rata r JOIN piano_rate p ON p.id=r.piano_id JOIN esercizio e ON e.id=p.esercizio_id
             WHERE e.condominio_id=?1 AND p.stato='attivo' AND r.scadenza<=?2
             GROUP BY r.scadenza,p.id ORDER BY r.scadenza",
        )?;
        let rate: Vec<(String, String, i64, Cents)> =
            st.query_map(params![cond, limite], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<Result<_, _>>()?;
        drop(st);
        for (d, piano, n, residuo) in rate {
            if residuo > 0 {
                out.push(Scadenza {
                    scaduta: scaduta(&d),
                    data: d,
                    tipo: "rata",
                    titolo: format!("Rate «{}»", piano),
                    dettaglio: format!("{} condòmini con residuo da incassare", n),
                    importo: Some(residuo),
                    agenda_id: None,
                });
            }
        }

        // fatture da pagare
        let giorni = self.giorni_pagamento_fatture();
        let mut st = self.conn.prepare(
            "SELECT f.data,fo.ragione_sociale,f.numero,f.imponibile_cents+f.iva_cents-f.ritenuta_cents
             FROM fattura f JOIN fornitore fo ON fo.id=f.fornitore_id WHERE f.condominio_id=?1 AND f.stato='da_pagare'",
        )?;
        let fatt: Vec<(String, String, String, Cents)> =
            st.query_map([cond], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<Result<_, _>>()?;
        drop(st);
        for (d, forn, num, netto) in fatt {
            let Some(dd) = parse_data(&d) else { continue };
            let scad = iso(dd + chrono::Duration::days(giorni));
            if scad <= limite {
                out.push(Scadenza {
                    scaduta: scaduta(&scad),
                    data: scad,
                    tipo: "fattura",
                    titolo: format!("Pagare {} – fattura {}", forn, num),
                    dettaglio: format!("Fattura del {} (scadenza presunta a {} giorni)", data_it(&d), giorni),
                    importo: Some(netto),
                    agenda_id: None,
                });
            }
        }

        // ritenute da versare
        for g in self.ritenute_da_versare(cond)? {
            if g.scadenza <= limite {
                out.push(Scadenza {
                    scaduta: scaduta(&g.scadenza),
                    data: g.scadenza.clone(),
                    tipo: "ritenute",
                    titolo: format!("F24 ritenute – codice tributo {}", g.codice_tributo),
                    dettaglio: format!("{} fatture", g.fatture.len()),
                    importo: Some(g.importo),
                    agenda_id: None,
                });
            }
        }

        // prescrizioni in arrivo
        for a in self.allerte_prescrizione(cond, oggi)? {
            if a.termine <= limite {
                out.push(Scadenza {
                    scaduta: a.prescritta,
                    data: a.termine.clone(),
                    tipo: "prescrizione",
                    titolo: format!("Prescrizione credito – {}", a.persona),
                    dettaglio: format!("{} (rata del {})", a.piano, data_it(&a.scadenza)),
                    importo: Some(a.residuo),
                    agenda_id: None,
                });
            }
        }

        // assemblee
        let mut st = self.conn.prepare(
            "SELECT data_prima,data_seconda,tipo,luogo FROM assemblea WHERE condominio_id=?1 AND stato IN ('bozza','convocata')",
        )?;
        let ass: Vec<(String, Option<String>, String, String)> =
            st.query_map([cond], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<Result<_, _>>()?;
        drop(st);
        for (p, s, tipo, luogo) in ass {
            for (d, conv) in [(Some(p), "prima"), (s, "seconda")] {
                if let Some(d) = d {
                    if d <= limite {
                        out.push(Scadenza {
                            scaduta: scaduta(&d),
                            data: d,
                            tipo: "assemblea",
                            titolo: format!("Assemblea {} – {} convocazione", tipo, conv),
                            dettaglio: luogo.clone(),
                            importo: None,
                            agenda_id: None,
                        });
                    }
                }
            }
        }

        // adempimenti fiscali ricorrenti (verificare eventuali proroghe)
        for anno in oggi.year()..=fino_a.year() {
            for (m, g, t) in [(3, 16, "Certificazione Unica: consegna ai percipienti e invio telematico"), (10, 31, "Modello 770 semplificato: invio telematico")] {
                if let Some(d) = NaiveDate::from_ymd_opt(anno, m, g) {
                    if d >= oggi && d <= fino_a {
                        out.push(Scadenza { data: iso(d), tipo: "fiscale", titolo: t.into(), dettaglio: "Adempimento del sostituto d'imposta (verifica eventuali proroghe)".into(), importo: None, scaduta: false, agenda_id: None });
                    }
                }
            }
        }

        // promemoria manuali
        let mut st = self.conn.prepare("SELECT id,data,titolo,note FROM agenda WHERE condominio_id=?1 AND completata=0 AND data<=?2")?;
        let ag: Vec<(i64, String, String, String)> =
            st.query_map(params![cond, limite], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<Result<_, _>>()?;
        drop(st);
        for (id, d, t, n) in ag {
            out.push(Scadenza { scaduta: scaduta(&d), data: d, tipo: "agenda", titolo: t, dettaglio: n, importo: None, agenda_id: Some(id) });
        }

        out.sort_by(|a, b| a.data.cmp(&b.data).then(a.tipo.cmp(b.tipo)));
        Ok(out)
    }
}
