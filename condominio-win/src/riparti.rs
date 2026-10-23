//! Riparti flessibili (come in Domustudio): una voce di spesa può essere suddivisa su più tabelle millesimali
//! e ogni unità può essere esclusa o ridotta (es. il piano terra non usa l'ascensore).
//!
//! Il riparto è unico per piani rate e consuntivo: stessa regola, stessi importi.

use crate::db::Db;
use crate::util::*;
use anyhow::{bail, Result};
use rusqlite::params;
use std::collections::BTreeMap;

pub const UNO: i64 = 1_000_000;

#[derive(Clone, Debug)]
pub struct QuotaTabella {
    pub tabella_id: i64,
    pub tabella: String,
    pub quota_ppm: i64,
}

#[derive(Clone, Debug)]
pub struct Riduzione {
    pub unita_id: i64,
    pub unita: String,
    pub riduzione_ppm: i64,
    pub motivo: String,
}

impl Db {
    fn voce_ancorata(&self, voce: i64) -> Result<bool> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM piano_voce pv JOIN piano_rate p ON p.id=pv.piano_id WHERE pv.voce_id=?1 AND p.stato='attivo'",
            [voce],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    /// Tabelle su cui è ripartita la voce. Senza suddivisione esplicita vale la tabella semplice della voce al 100%.
    pub fn tabelle_voce(&self, voce: i64) -> Result<Vec<QuotaTabella>> {
        let mut st = self.conn.prepare(
            "SELECT vt.tabella_id,t.codice||' – '||t.nome,vt.quota_ppm FROM voce_tabella vt JOIN tabella_millesimale t ON t.id=vt.tabella_id
             WHERE vt.voce_id=?1 ORDER BY vt.tabella_id",
        )?;
        let v: Vec<QuotaTabella> = st
            .query_map([voce], |r| Ok(QuotaTabella { tabella_id: r.get(0)?, tabella: r.get(1)?, quota_ppm: r.get(2)? }))?
            .collect::<Result<_, _>>()?;
        if !v.is_empty() {
            return Ok(v);
        }
        let semplice: Option<(i64, String)> = self
            .conn
            .query_row(
                "SELECT t.id,t.codice||' – '||t.nome FROM voce_spesa v JOIN tabella_millesimale t ON t.id=v.tabella_id WHERE v.id=?1",
                [voce],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();
        Ok(semplice.map(|(t, n)| vec![QuotaTabella { tabella_id: t, tabella: n, quota_ppm: UNO }]).unwrap_or_default())
    }

    /// True se per la voce è definito un riparto (tabella semplice o suddivisione).
    pub fn voce_ripartibile(&self, voce: i64) -> Result<bool> {
        Ok(!self.tabelle_voce(voce)?.is_empty())
    }

    /// Imposta la suddivisione su più tabelle. `quote` = (tabella, quota_ppm); la somma deve fare 100%.
    /// Una lista vuota riporta la voce alla sola tabella semplice.
    pub fn imposta_riparto_voce(&self, voce: i64, quote: &[(i64, i64)]) -> Result<()> {
        if self.voce_ancorata(voce)? {
            bail!("La voce è ancorata a un piano rate attivo: il riparto non è modificabile");
        }
        if !quote.is_empty() {
            let somma: i64 = quote.iter().map(|q| q.1).sum();
            if somma != UNO {
                bail!("Le quote delle tabelle devono sommare al 100% (ora {}%)", format_fixed_ppm_pct(somma));
            }
            if quote.iter().any(|q| q.1 <= 0) {
                bail!("Ogni quota deve essere maggiore di zero");
            }
            let mut viste = std::collections::HashSet::new();
            if !quote.iter().all(|q| viste.insert(q.0)) {
                bail!("Una tabella compare più volte");
            }
            let cond_voce: i64 = self.conn.query_row(
                "SELECT e.condominio_id FROM voce_spesa v JOIN esercizio e ON e.id=v.esercizio_id WHERE v.id=?1",
                [voce],
                |r| r.get(0),
            )?;
            for (t, _) in quote {
                let c: i64 = self.conn.query_row("SELECT condominio_id FROM tabella_millesimale WHERE id=?1", [t], |r| r.get(0))?;
                if c != cond_voce {
                    bail!("Una tabella appartiene a un altro condominio");
                }
            }
        }
        self.atomico(|db| {
            db.conn.execute("DELETE FROM voce_tabella WHERE voce_id=?1", [voce])?;
            for (t, q) in quote {
                db.conn.execute("INSERT INTO voce_tabella(voce_id,tabella_id,quota_ppm) VALUES(?1,?2,?3)", params![voce, t, q])?;
            }
            Ok(())
        })
    }

    pub fn elenco_riduzioni(&self, voce: i64) -> Result<Vec<Riduzione>> {
        let mut st = self.conn.prepare(
            "SELECT r.unita_id,u.scala,u.piano,u.interno,r.riduzione_ppm,r.motivo FROM voce_riduzione r JOIN unita u ON u.id=r.unita_id
             WHERE r.voce_id=?1 ORDER BY u.scala,u.interno,u.id",
        )?;
        let v = st
            .query_map([voce], |r| {
                let (scala, piano, interno): (String, String, String) = (r.get(1)?, r.get(2)?, r.get(3)?);
                let etichetta = [format!("Sc. {}", scala), format!("P. {}", piano), format!("Int. {}", interno)]
                    .into_iter()
                    .filter(|x| !x.ends_with(' '))
                    .collect::<Vec<_>>()
                    .join(" ");
                Ok(Riduzione { unita_id: r.get(0)?, unita: etichetta, riduzione_ppm: r.get(4)?, motivo: r.get(5)? })
            })?
            .collect::<Result<_, _>>()?;
        Ok(v)
    }

    /// Riduce (o esclude, con 100%) una unità dalla spesa. `riduzione_ppm` in 1..=1_000_000.
    pub fn imposta_riduzione(&self, voce: i64, unita: i64, riduzione_ppm: i64, motivo: &str) -> Result<()> {
        if !(1..=UNO).contains(&riduzione_ppm) {
            bail!("La riduzione deve essere tra lo 0,0001% e il 100%");
        }
        if self.voce_ancorata(voce)? {
            bail!("La voce è ancorata a un piano rate attivo: il riparto non è modificabile");
        }
        let ok: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM voce_spesa v JOIN esercizio e ON e.id=v.esercizio_id JOIN unita u ON u.condominio_id=e.condominio_id
             WHERE v.id=?1 AND u.id=?2",
            params![voce, unita],
            |r| r.get(0),
        )?;
        if ok == 0 {
            bail!("L'unità non appartiene al condominio della voce");
        }
        self.conn.execute(
            "INSERT INTO voce_riduzione(voce_id,unita_id,riduzione_ppm,motivo) VALUES(?1,?2,?3,?4)
             ON CONFLICT(voce_id,unita_id) DO UPDATE SET riduzione_ppm=?3,motivo=?4",
            params![voce, unita, riduzione_ppm, motivo.trim()],
        )?;
        Ok(())
    }

    pub fn rimuovi_riduzione(&self, voce: i64, unita: i64) -> Result<()> {
        if self.voce_ancorata(voce)? {
            bail!("La voce è ancorata a un piano rate attivo: il riparto non è modificabile");
        }
        self.conn.execute("DELETE FROM voce_riduzione WHERE voce_id=?1 AND unita_id=?2", params![voce, unita])?;
        Ok(())
    }

    /// Riparto di `importo` della voce tra le unità (penny-perfect): suddivide fra le tabelle, poi pesa i millesimi
    /// di ciascuna unità per (1 − riduzione). Le unità escluse non pagano nulla e la loro quota va alle altre.
    pub fn riparto_voce(&self, voce: i64, importo: Cents) -> Result<Vec<(i64, Cents)>> {
        let tabelle = self.tabelle_voce(voce)?;
        if tabelle.is_empty() {
            bail!("La voce non ha una tabella millesimale associata");
        }
        let riduzioni: BTreeMap<i64, i64> = self.elenco_riduzioni(voce)?.into_iter().map(|r| (r.unita_id, r.riduzione_ppm)).collect();
        let pesi_tab: Vec<i64> = tabelle.iter().map(|t| t.quota_ppm).collect();
        let quote_tab = ripartisci(importo, &pesi_tab);
        let mut per_unita: BTreeMap<i64, Cents> = BTreeMap::new();
        for (t, quota) in tabelle.iter().zip(quote_tab) {
            if quota == 0 {
                continue;
            }
            let mut st = self
                .conn
                .prepare("SELECT unita_id,valore_micro FROM millesimo WHERE tabella_id=?1 AND valore_micro>0 ORDER BY unita_id")?;
            let righe: Vec<(i64, i64)> = st.query_map([t.tabella_id], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
            let pesate: Vec<(i64, i64)> = righe
                .into_iter()
                .map(|(u, v)| {
                    let peso = match riduzioni.get(&u) {
                        Some(rid) => ((v as i128) * ((UNO - rid) as i128) / UNO as i128) as i64,
                        None => v,
                    };
                    (u, peso)
                })
                .filter(|x| x.1 > 0)
                .collect();
            if pesate.is_empty() {
                bail!("Nessuna unità partecipa alla spesa nella tabella «{}» (valori assenti o tutte escluse)", t.tabella);
            }
            let pesi: Vec<i64> = pesate.iter().map(|x| x.1).collect();
            for ((u, _), c) in pesate.iter().zip(ripartisci(quota, &pesi)) {
                *per_unita.entry(*u).or_insert(0) += c;
            }
        }
        Ok(per_unita.into_iter().collect())
    }
}

fn format_fixed_ppm_pct(ppm: i64) -> String {
    format!("{:.4}", ppm as f64 / 10_000.0).trim_end_matches('0').trim_end_matches('.').replace('.', ",")
}
