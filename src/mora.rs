//! Insoluti, interessi di mora (tasso legale per anno, calcolo esatto sulle date degli incassi) e solleciti.

use crate::db::Db;
use crate::util::*;
use anyhow::{bail, Result};
use chrono::{Datelike, Duration, NaiveDate};
use rusqlite::params;

#[derive(Clone, Debug)]
pub struct RataScaduta {
    pub rata_id: i64,
    pub persona_id: i64,
    pub persona: String,
    pub piano: String,
    pub numero: i64,
    pub scadenza: String,
    pub importo: Cents,
    pub residuo: Cents,
    pub giorni_ritardo: i64,
    pub interessi: Cents,
}

#[derive(Clone, Debug)]
pub struct Sollecito {
    pub persona_id: i64,
    pub persona: String,
    pub email: String,
    pub rate: Vec<RataScaduta>,
    pub capitale: Cents,
    pub interessi: Cents,
    pub giorni_max: i64,
}

impl Sollecito {
    pub fn totale(&self) -> Cents {
        self.capitale + self.interessi
    }
}

fn giorni_anno(anno: i32) -> i64 {
    if NaiveDate::from_ymd_opt(anno, 2, 29).is_some() {
        366
    } else {
        365
    }
}

impl Db {
    /// Tasso legale (ppm) in vigore nell'anno; se manca, l'ultimo noto precedente.
    pub fn tasso_legale(&self, anno: i32) -> i64 {
        self.conn
            .query_row("SELECT ppm FROM tasso_legale WHERE anno<=?1 ORDER BY anno DESC LIMIT 1", [anno], |r| r.get(0))
            .unwrap_or(0)
    }

    pub fn elenco_tassi(&self) -> Result<Vec<(i32, i64)>> {
        let mut st = self.conn.prepare("SELECT anno,ppm FROM tasso_legale ORDER BY anno DESC")?;
        let v = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
        Ok(v)
    }

    pub fn imposta_tasso_legale(&self, anno: i32, ppm: i64) -> Result<()> {
        if !(1900..=2200).contains(&anno) || !(0..=1_000_000).contains(&ppm) {
            bail!("Anno o tasso non validi");
        }
        self.conn.execute("INSERT INTO tasso_legale(anno,ppm) VALUES(?1,?2) ON CONFLICT(anno) DO UPDATE SET ppm=?2", params![anno, ppm])?;
        Ok(())
    }

    /// Giorni di tolleranza dopo la scadenza prima che decorrano gli interessi (impostazione, default 0).
    pub fn giorni_tolleranza_mora(&self) -> i64 {
        self.leggi_imp("mora_giorni_tolleranza").parse().unwrap_or(0).clamp(0, 365)
    }

    /// Interessi semplici su `capitale` dal giorno `dal` (escluso) al giorno `al` (incluso), anno per anno.
    /// Restituisce sotto-unità di 1e-6 centesimi, per sommare senza perdere frazioni.
    fn interessi_micro(&self, capitale: Cents, dal: NaiveDate, al: NaiveDate) -> i128 {
        let mut tot: i128 = 0;
        let mut cur = dal;
        while cur < al {
            // gli interessi maturano dal giorno successivo a `cur`: l'anno di riferimento è quello di quel giorno
            let anno = (cur + Duration::days(1)).year();
            let fine = al.min(NaiveDate::from_ymd_opt(anno, 12, 31).unwrap());
            let giorni = (fine - cur).num_days() as i128;
            tot += capitale as i128 * self.tasso_legale(anno) as i128 * giorni / giorni_anno(anno) as i128;
            cur = fine;
        }
        tot
    }

    /// Interesse di mora di una singola rata a `oggi`: tiene conto di ogni incasso (data e importo).
    pub fn interessi_rata(&self, rata: i64, oggi: NaiveDate) -> Result<Cents> {
        let (scad, importo): (String, Cents) =
            self.conn.query_row("SELECT scadenza,importo_cents FROM rata WHERE id=?1", [rata], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let Some(scad) = parse_data(&scad) else { bail!("Scadenza non valida") };
        let inizio = scad + Duration::days(self.giorni_tolleranza_mora());
        let mut st = self.conn.prepare(
            "SELECT i.data,a.importo_cents FROM allocazione a JOIN incasso i ON i.id=a.incasso_id
             WHERE a.rata_id=?1 AND i.stato='valido' ORDER BY i.data,a.id",
        )?;
        let pagamenti: Vec<(String, Cents)> = st.query_map([rata], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
        let mut residuo = importo;
        let mut cursore = inizio;
        let mut micro: i128 = 0;
        for (d, x) in pagamenti {
            let Some(d) = parse_data(&d) else { continue };
            if d > cursore {
                micro += self.interessi_micro(residuo, cursore, d.min(oggi));
                cursore = d;
            }
            residuo -= x;
        }
        if oggi > cursore && residuo > 0 {
            micro += self.interessi_micro(residuo, cursore, oggi);
        }
        Ok(((micro + 500_000) / 1_000_000) as Cents)
    }

    /// Rate scadute e non (del tutto) pagate, con giorni di ritardo e interessi maturati.
    pub fn rate_scadute(&self, cond: i64, oggi: NaiveDate, persona: Option<i64>) -> Result<Vec<RataScaduta>> {
        let mut st = self.conn.prepare(
            "SELECT r.id,r.persona_id,trim(pe.cognome||' '||pe.nome),p.nome,r.numero,r.scadenza,r.importo_cents,
                    r.importo_cents - COALESCE((SELECT SUM(a.importo_cents) FROM allocazione a JOIN incasso i ON i.id=a.incasso_id
                                                WHERE a.rata_id=r.id AND i.stato='valido'),0)
             FROM rata r JOIN piano_rate p ON p.id=r.piano_id JOIN esercizio e ON e.id=p.esercizio_id
             JOIN persona pe ON pe.id=r.persona_id
             WHERE e.condominio_id=?1 AND p.stato='attivo' AND r.scadenza<?2 AND (?3 IS NULL OR r.persona_id=?3)
             ORDER BY pe.cognome,pe.nome,r.scadenza,r.numero",
        )?;
        let grezze: Vec<(i64, i64, String, String, i64, String, Cents, Cents)> = st
            .query_map(params![cond, oggi.format("%Y-%m-%d").to_string(), persona], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?))
            })?
            .collect::<Result<_, _>>()?;
        drop(st);
        let mut out = vec![];
        for (rata_id, persona_id, nome, piano, numero, scadenza, importo, residuo) in grezze {
            if residuo <= 0 {
                continue;
            }
            let giorni = parse_data(&scadenza).map(|d| (oggi - d).num_days()).unwrap_or(0);
            out.push(RataScaduta {
                rata_id,
                persona_id,
                persona: nome,
                piano,
                numero,
                scadenza,
                importo,
                residuo,
                giorni_ritardo: giorni,
                interessi: self.interessi_rata(rata_id, oggi)?,
            });
        }
        Ok(out)
    }

    /// Un elemento per condòmino moroso, filtrato per ritardo e importo minimi.
    pub fn solleciti_da_inviare(&self, cond: i64, oggi: NaiveDate, giorni_minimi: i64, soglia: Cents) -> Result<Vec<Sollecito>> {
        let rate = self.rate_scadute(cond, oggi, None)?;
        let persone = self.persone_del_condominio(cond)?;
        let mut out: Vec<Sollecito> = vec![];
        for r in rate {
            if r.giorni_ritardo < giorni_minimi {
                continue;
            }
            match out.iter_mut().find(|s| s.persona_id == r.persona_id) {
                Some(s) => {
                    s.capitale += r.residuo;
                    s.interessi += r.interessi;
                    s.giorni_max = s.giorni_max.max(r.giorni_ritardo);
                    s.rate.push(r);
                }
                None => {
                    let email = persone.iter().find(|p| p.id == r.persona_id).map(|p| p.email.clone()).unwrap_or_default();
                    out.push(Sollecito {
                        persona_id: r.persona_id,
                        persona: r.persona.clone(),
                        email,
                        capitale: r.residuo,
                        interessi: r.interessi,
                        giorni_max: r.giorni_ritardo,
                        rate: vec![r],
                    });
                }
            }
        }
        out.retain(|s| s.capitale >= soglia);
        Ok(out)
    }

    /// Testo del sollecito (oggetto, corpo).
    pub fn testo_sollecito(&self, cond: i64, s: &Sollecito, oggi: NaiveDate, termine_giorni: i64) -> Result<(String, String)> {
        let nome_cond: String = self.conn.query_row("SELECT nome FROM condominio WHERE id=?1", [cond], |r| r.get(0))?;
        let studio = self.leggi_imp("studio_nome");
        let firma = if studio.trim().is_empty() { format!("Amministrazione {}", nome_cond) } else { studio };
        let tasso = self.tasso_legale(oggi.year());
        let mut c = format!("Gentile {},\n\n", s.persona);
        c.push_str(&format!(
            "dalle verifiche contabili del condominio «{}» alla data del {} risultano non versate le seguenti rate:\n\n",
            nome_cond,
            data_it(&oggi.format("%Y-%m-%d").to_string())
        ));
        for r in &s.rate {
            c.push_str(&format!(
                "  • {} – rata n. {} scaduta il {}: residuo {} ({} giorni di ritardo, interessi {})\n",
                r.piano,
                r.numero,
                data_it(&r.scadenza),
                format_eur(r.residuo),
                r.giorni_ritardo,
                format_eur(r.interessi)
            ));
        }
        c.push_str(&format!(
            "\nCapitale da versare: {}\nInteressi di mora maturati (tasso legale {}% annuo): {}\nTOTALE: {}\n",
            format_eur(s.capitale),
            format_fixed_ppm(tasso),
            format_eur(s.interessi),
            format_eur(s.totale())
        ));
        let wallet = self.saldo_salvadanaio(cond, s.persona_id)?;
        if wallet > 0 {
            c.push_str(&format!("\nRisulta a Suo favore un credito di {} (Salvadanaio) che, se lo desidera, può essere compensato.\n", format_eur(wallet)));
        }
        c.push_str(&format!(
            "\nLa invitiamo a regolarizzare la posizione entro {} giorni dalla ricezione della presente. In mancanza, saremo tenuti ad avviare il recupero del credito ai sensi dell'art. 63 disp. att. c.c.\n\nSe ha già provveduto al pagamento, La preghiamo di ignorare questo avviso e di inviarci copia della ricevuta.\n\nCordiali saluti\n{}\n",
            termine_giorni, firma
        ));
        Ok((format!("[{}] Sollecito di pagamento rate condominiali", nome_cond), c))
    }

    pub fn registra_sollecito(&self, cond: i64, s: &Sollecito, canale: &str, testo: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sollecito(condominio_id,persona_id,data,canale,capitale_cents,interessi_cents,testo) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![cond, s.persona_id, oggi_iso(), canale, s.capitale, s.interessi, testo],
        )?;
        Ok(())
    }

    /// Storico solleciti: (data, persona, canale, capitale, interessi).
    pub fn storico_solleciti(&self, cond: i64) -> Result<Vec<(String, String, String, Cents, Cents)>> {
        let mut st = self.conn.prepare(
            "SELECT s.data,trim(p.cognome||' '||p.nome),s.canale,s.capitale_cents,s.interessi_cents FROM sollecito s
             JOIN persona p ON p.id=s.persona_id WHERE s.condominio_id=?1 ORDER BY s.id DESC LIMIT 300",
        )?;
        let v = st.query_map([cond], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?.collect::<Result<_, _>>()?;
        Ok(v)
    }

    /// Atto interruttivo della prescrizione su tutte le rate del sollecito (es. raccomandata / PEC).
    pub fn interrompi_prescrizione_sollecito(&self, s: &Sollecito, data: &str) -> Result<()> {
        self.atomico(|db| {
            for r in &s.rate {
                db.registra_interruzione(r.rata_id, data)?;
            }
            Ok(())
        })
    }
}

/// Formatta un tasso in ppm come percentuale italiana (16000 → "1,6").
pub fn format_fixed_ppm(ppm: i64) -> String {
    let s = format!("{:.4}", ppm as f64 / 10_000.0);
    let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    s.replace('.', ",")
}
