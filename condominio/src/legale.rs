//! Conformità legale: prescrizione quinquennale delle rate (Art. 2948 n.4 c.c.),
//! controlli su delibere (Art. 1135), sopravvenienze (Art. 1130-bis) e coerenza contabile.

use crate::db::Db;
use crate::util::*;
use anyhow::Result;
use chrono::NaiveDate;
use rusqlite::params;

pub const GIORNI_PREAVVISO_PRESCRIZIONE: i64 = 180;

#[derive(Clone, Debug)]
pub struct AllertaPrescrizione {
    pub rata_id: i64,
    pub persona: String,
    pub piano: String,
    pub scadenza: String,
    pub residuo: Cents,
    pub termine: String,
    pub giorni_mancanti: i64,
    pub prescritta: bool,
    pub interrotta: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gravita {
    Ok,
    Avviso,
    Errore,
}

#[derive(Clone, Debug)]
pub struct Controllo {
    pub gravita: Gravita,
    pub riferimento: &'static str,
    pub messaggio: String,
}

impl Db {
    /// Rate non saldate prescritte o prossime alla prescrizione. Il termine di 5 anni decorre dalla
    /// scadenza della rata, oppure dall'ultimo atto interruttivo (es. messa in mora) registrato.
    pub fn allerte_prescrizione(&self, cond: i64, oggi: NaiveDate) -> Result<Vec<AllertaPrescrizione>> {
        let mut st = self.conn.prepare(
            "SELECT r.id,pe.cognome||' '||pe.nome,p.nome,r.scadenza,r.interruzione_prescrizione,
                    r.importo_cents-COALESCE((SELECT SUM(a.importo_cents) FROM allocazione a JOIN incasso i ON i.id=a.incasso_id
                                              WHERE a.rata_id=r.id AND i.stato='valido'),0)
             FROM rata r JOIN piano_rate p ON p.id=r.piano_id JOIN esercizio e ON e.id=p.esercizio_id JOIN persona pe ON pe.id=r.persona_id
             WHERE e.condominio_id=?1 AND p.stato='attivo'",
        )?;
        let righe: Vec<(i64, String, String, String, Option<String>, Cents)> = st
            .query_map([cond], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?
            .collect::<Result<_, _>>()?;
        let mut out = vec![];
        for (id, pers, piano, scad, intr, residuo) in righe {
            if residuo <= 0 {
                continue;
            }
            let base_s = intr.clone().unwrap_or_else(|| scad.clone());
            let Some(base) = parse_data(&base_s) else { continue };
            let termine = aggiungi_anni(base, 5);
            let giorni = (termine - oggi).num_days();
            if giorni <= GIORNI_PREAVVISO_PRESCRIZIONE {
                out.push(AllertaPrescrizione {
                    rata_id: id,
                    persona: pers.trim().to_string(),
                    piano,
                    scadenza: scad,
                    residuo,
                    termine: termine.format("%Y-%m-%d").to_string(),
                    giorni_mancanti: giorni,
                    prescritta: giorni < 0,
                    interrotta: intr.is_some(),
                });
            }
        }
        out.sort_by_key(|a| a.giorni_mancanti);
        Ok(out)
    }

    /// Registra un atto interruttivo (es. raccomandata di messa in mora): il termine riparte da quella data.
    pub fn registra_interruzione(&self, rata: i64, data: &str) -> Result<()> {
        if parse_data(data).is_none() {
            anyhow::bail!("Data non valida");
        }
        self.conn.execute("UPDATE rata SET interruzione_prescrizione=?2 WHERE id=?1", params![rata, data])?;
        Ok(())
    }

    /// Verifiche automatiche di conformità e coerenza sul condominio.
    pub fn controlli_conformita(&self, cond: i64) -> Result<Vec<Controllo>> {
        let mut out: Vec<Controllo> = vec![];
        let n = |sql: &str| -> Result<i64> { Ok(self.conn.query_row(sql, [cond], |r| r.get(0))?) };

        // Art. 1135: ogni piano attivo deve poggiare su una delibera
        let senza = n("SELECT COUNT(*) FROM piano_rate p JOIN esercizio e ON e.id=p.esercizio_id
                       WHERE e.condominio_id=?1 AND p.stato='attivo' AND p.delibera_id IS NULL")?;
        if senza > 0 {
            out.push(Controllo { gravita: Gravita::Errore, riferimento: "Art. 1135 c.c.", messaggio: format!("{} piani attivi senza delibera assembleare", senza) });
        }
        // Art. 1135 n.2: preventivo approvato dall'assemblea
        let mut st = self.conn.prepare(
            "SELECT e.nome FROM esercizio e WHERE e.condominio_id=?1 AND e.stato='aperto'
             AND EXISTS(SELECT 1 FROM voce_spesa v WHERE v.esercizio_id=e.id)
             AND NOT EXISTS(SELECT 1 FROM delibera d WHERE d.esercizio_id=e.id AND d.tipo='preventivo')",
        )?;
        let nomi: Vec<String> = st.query_map([cond], |r| r.get(0))?.collect::<Result<_, _>>()?;
        for nome in nomi {
            out.push(Controllo { gravita: Gravita::Avviso, riferimento: "Art. 1135 n.2 c.c.", messaggio: format!("Esercizio «{}»: nessuna delibera di approvazione del preventivo registrata", nome) });
        }
        // Art. 1130 n.10 / 1130-bis: rendiconto per gli esercizi chiusi
        let mut st = self.conn.prepare(
            "SELECT e.nome FROM esercizio e WHERE e.condominio_id=?1 AND e.stato='chiuso'
             AND NOT EXISTS(SELECT 1 FROM delibera d WHERE d.esercizio_id=e.id AND d.tipo='rendiconto')",
        )?;
        let nomi: Vec<String> = st.query_map([cond], |r| r.get(0))?.collect::<Result<_, _>>()?;
        for nome in nomi {
            out.push(Controllo { gravita: Gravita::Avviso, riferimento: "Art. 1130 n.10 c.c.", messaggio: format!("Esercizio chiuso «{}» senza delibera di approvazione del rendiconto", nome) });
        }
        // Art. 1130-bis: sopravvenienze non ancora ripartite in alcun piano
        let sopr = n("SELECT COUNT(*) FROM voce_spesa v JOIN esercizio e ON e.id=v.esercizio_id
                      WHERE e.condominio_id=?1 AND v.sopravvenienza=1
                      AND NOT EXISTS(SELECT 1 FROM piano_voce pv JOIN piano_rate p ON p.id=pv.piano_id WHERE pv.voce_id=v.id AND p.stato<>'annullato')")?;
        if sopr > 0 {
            out.push(Controllo { gravita: Gravita::Avviso, riferimento: "Art. 1130-bis c.c.", messaggio: format!("{} sopravvenienze non ancora ripartite in un piano rate (es. integrativo)", sopr) });
        }
        // Tabelle millesimali
        for t in self.elenco_tabelle(cond)? {
            let s = self.somma_millesimi(t.id)?;
            if s != t.totale_micro {
                out.push(Controllo {
                    gravita: Gravita::Avviso,
                    riferimento: "Tabelle millesimali",
                    messaggio: format!("Tabella {}: somma {} diverso da totale {}", t.codice, format_millesimi(s), format_millesimi(t.totale_micro)),
                });
            }
        }
        // Unità senza pagante
        for u in self.elenco_unita(cond)? {
            if self.pagatori_unita(u.id)?.is_empty() {
                out.push(Controllo { gravita: Gravita::Errore, riferimento: "Anagrafica", messaggio: format!("{}: nessun proprietario/pagante assegnato", u.etichetta()) });
            } else {
                let tot: i64 = self.pagatori_unita(u.id)?.iter().map(|x| x.1).sum();
                if tot != 1_000_000 {
                    out.push(Controllo {
                        gravita: Gravita::Avviso,
                        riferimento: "Anagrafica",
                        messaggio: format!("{}: le quote dei paganti sommano {:.2}% (dovrebbero fare 100%)", u.etichetta(), tot as f64 / 10_000.0),
                    });
                }
            }
        }
        // Ritenute scadute
        let oggi_s = oggi_iso();
        for g in self.ritenute_da_versare(cond)? {
            if g.scadenza < oggi_s {
                out.push(Controllo {
                    gravita: Gravita::Errore,
                    riferimento: "DPR 600/73 art. 25-ter",
                    messaggio: format!("Ritenute cod. {} di {} da versare entro il {} (scaduto)", g.codice_tributo, format_eur(g.importo), data_it(&g.scadenza)),
                });
            }
        }
        // Prescrizione
        let pre = self.allerte_prescrizione(cond, oggi())?;
        let prescritte = pre.iter().filter(|a| a.prescritta).count();
        let a_rischio = pre.len() - prescritte;
        if prescritte > 0 {
            out.push(Controllo { gravita: Gravita::Errore, riferimento: "Art. 2948 n.4 c.c.", messaggio: format!("{} rate non incassate risultano prescritte", prescritte) });
        }
        if a_rischio > 0 {
            out.push(Controllo { gravita: Gravita::Avviso, riferimento: "Art. 2948 n.4 c.c.", messaggio: format!("{} rate in scadenza di prescrizione entro {} giorni: valuta una messa in mora", a_rischio, GIORNI_PREAVVISO_PRESCRIZIONE) });
        }
        // Coerenza tra movimenti bancari e documenti (incassi, pagamenti fornitori, F24)
        for m in self.verifica_movimenti(cond)? {
            out.push(Controllo { gravita: Gravita::Errore, riferimento: "Audit movimenti", messaggio: m });
        }
        if out.is_empty() {
            out.push(Controllo { gravita: Gravita::Ok, riferimento: "", messaggio: "Nessuna anomalia rilevata".into() });
        }
        Ok(out)
    }
}
