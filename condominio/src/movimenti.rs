//! Movimenti di gestione (modello Domustudio): ogni entrata e uscita sui conti correnti del condominio.
//!
//! Non c'è partita doppia: la liquidità è la somma dei movimenti, i crediti verso i condòmini derivano dalle rate,
//! il Salvadanaio dai suoi movimenti e i debiti verso i fornitori dalle fatture da pagare.
//! Nulla si cancella: uno storno aggiunge il movimento inverso e marca l'originale come stornato.

use crate::db::Db;
use crate::util::*;
use anyhow::{bail, Result};
use rusqlite::params;

#[derive(Clone, Debug)]
pub struct Movimento {
    pub id: i64,
    pub data: String,
    pub descrizione: String,
    pub importo: Cents, // + entrata, - uscita
    pub conto_id: i64,
    pub conto: String,
    pub origine: String,
    pub origine_id: Option<i64>,
    pub voce_id: Option<i64>,
    pub storno_di: Option<i64>,
    pub stornato: bool,
}

/// Fotografia della gestione: ciò che il condominio ha, deve incassare e deve pagare.
#[derive(Clone, Debug, Default)]
pub struct Situazione {
    pub liquidita: Cents,
    pub crediti_condomini: Cents,
    pub salvadanaio: Cents,
    pub debiti_fornitori: Cents,
    pub ritenute_da_versare: Cents,
}

pub const ORIGINI: [(&str, &str); 7] = [
    ("incasso", "Incasso rate"),
    ("pagamento_fattura", "Pagamento fattura"),
    ("f24", "Versamento F24"),
    ("giroconto", "Giroconto"),
    ("manuale", "Movimento manuale"),
    ("storno", "Storno"),
    ("altro", "Altro"),
];

pub fn nome_origine(o: &str) -> &str {
    ORIGINI.iter().find(|x| x.0 == o).map(|x| x.1).unwrap_or(o)
}

impl Db {
    // ------------------------------------------------------------ conti correnti
    pub fn crea_conto_corrente(&self, cond: i64, nome: &str, iban: &str) -> Result<i64> {
        if nome.trim().is_empty() {
            bail!("Il nome del conto è obbligatorio");
        }
        self.conn.execute(
            "INSERT INTO conto_corrente(condominio_id,nome,iban) VALUES(?1,?2,?3)",
            params![cond, nome.trim(), iban.trim()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn elenco_conti_correnti(&self, cond: i64) -> Result<Vec<(i64, String, String)>> {
        let mut st = self.conn.prepare("SELECT id,nome,iban FROM conto_corrente WHERE condominio_id=?1 ORDER BY id")?;
        let v = st.query_map([cond], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<Result<_, _>>()?;
        Ok(v)
    }

    pub fn saldo_conto_corrente(&self, conto_corrente: i64) -> Result<Cents> {
        Ok(self.conn.query_row(
            "SELECT COALESCE(SUM(importo_cents),0) FROM movimento_gestione WHERE conto_corrente_id=?1",
            [conto_corrente],
            |r| r.get(0),
        )?)
    }

    // ------------------------------------------------------------ movimenti
    /// Registra un movimento (importo positivo = entrata, negativo = uscita).
    #[allow(clippy::too_many_arguments)]
    pub fn registra_movimento(
        &self,
        cond: i64,
        cc: i64,
        data: &str,
        descrizione: &str,
        importo: Cents,
        origine: &str,
        origine_id: Option<i64>,
    ) -> Result<i64> {
        if importo == 0 {
            bail!("L'importo del movimento non può essere zero");
        }
        if parse_data(data).is_none() {
            bail!("Data del movimento non valida");
        }
        let ok: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM conto_corrente WHERE id=?1 AND condominio_id=?2",
            params![cc, cond],
            |r| r.get(0),
        )?;
        if ok == 0 {
            bail!("Il conto corrente non appartiene a questo condominio");
        }
        self.conn.execute(
            "INSERT INTO movimento_gestione(condominio_id,conto_corrente_id,data,descrizione,importo_cents,origine,origine_id)
             VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![cond, cc, data, descrizione, importo, origine, origine_id],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Movimento libero (spese e interessi bancari, saldo iniziale, rimborsi...). Con `voce` entra nel consuntivo di quella voce.
    pub fn registra_movimento_manuale(&self, cond: i64, cc: i64, data: &str, descrizione: &str, importo: Cents, voce: Option<i64>) -> Result<i64> {
        if descrizione.trim().is_empty() {
            bail!("La descrizione è obbligatoria");
        }
        self.atomico(|db| {
            let id = db.registra_movimento(cond, cc, data, descrizione.trim(), importo, "manuale", None)?;
            if let Some(v) = voce {
                let ok: i64 = db.conn.query_row(
                    "SELECT COUNT(*) FROM voce_spesa v JOIN esercizio e ON e.id=v.esercizio_id WHERE v.id=?1 AND e.condominio_id=?2",
                    params![v, cond],
                    |r| r.get(0),
                )?;
                if ok == 0 {
                    bail!("La voce di spesa non appartiene a questo condominio");
                }
                db.conn.execute("UPDATE movimento_gestione SET voce_id=?2 WHERE id=?1", params![id, v])?;
            }
            Ok(id)
        })
    }

    /// Storno "write-then-reverse": aggiunge il movimento inverso. Un giroconto viene stornato su entrambi i conti.
    pub fn storna_movimento(&self, id: i64, data: &str, motivo: &str) -> Result<i64> {
        if parse_data(data).is_none() {
            bail!("Data dello storno non valida");
        }
        self.atomico(|db| {
            let (origine, origine_id, storno_di, stornato): (String, Option<i64>, Option<i64>, i64) = db.conn.query_row(
                "SELECT origine,origine_id,storno_di,stornato FROM movimento_gestione WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )?;
            if storno_di.is_some() {
                bail!("Questo movimento è già uno storno");
            }
            if stornato != 0 {
                bail!("Il movimento è già stato stornato");
            }
            let mut da_stornare = vec![id];
            if origine == "giroconto" {
                let partner: Option<i64> = match origine_id {
                    Some(p) => Some(p),
                    None => db
                        .conn
                        .query_row("SELECT id FROM movimento_gestione WHERE origine='giroconto' AND origine_id=?1", [id], |r| r.get(0))
                        .ok(),
                };
                if let Some(p) = partner {
                    da_stornare.push(p);
                }
            }
            let mut primo = 0;
            for m in da_stornare {
                let (cond, cc, d, descr, imp, gia): (i64, i64, String, String, Cents, i64) = db.conn.query_row(
                    "SELECT condominio_id,conto_corrente_id,data,descrizione,importo_cents,stornato FROM movimento_gestione WHERE id=?1",
                    [m],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
                )?;
                if gia != 0 {
                    continue;
                }
                db.conn.execute(
                    "INSERT INTO movimento_gestione(condominio_id,conto_corrente_id,data,descrizione,importo_cents,origine,origine_id,storno_di)
                     VALUES(?1,?2,?3,?4,?5,'storno',?6,?6)",
                    params![cond, cc, data, format!("STORNO del {} – {} ({})", data_it(&d), descr, motivo), -imp, m],
                )?;
                if primo == 0 {
                    primo = db.conn.last_insert_rowid();
                }
                db.conn.execute("UPDATE movimento_gestione SET stornato=1 WHERE id=?1", [m])?;
            }
            Ok(primo)
        })
    }

    /// Trasferimento tra due conti correnti del condominio.
    pub fn giroconto(&self, cond: i64, da: i64, a: i64, importo: Cents, data: &str, descr: &str) -> Result<i64> {
        if da == a {
            bail!("Scegli due conti diversi");
        }
        if importo <= 0 {
            bail!("Importo non valido");
        }
        self.atomico(|db| {
            let testo = format!("Giroconto – {}", descr.trim());
            let primo = db.registra_movimento(cond, da, data, &testo, -importo, "giroconto", None)?;
            db.registra_movimento(cond, a, data, &testo, importo, "giroconto", Some(primo))?;
            Ok(primo)
        })
    }

    pub fn elenco_movimenti(&self, cond: i64, cc: Option<i64>, dal: Option<&str>, al: Option<&str>) -> Result<Vec<Movimento>> {
        let mut st = self.conn.prepare(
            "SELECT m.id,m.data,m.descrizione,m.importo_cents,m.conto_corrente_id,c.nome,m.origine,m.origine_id,m.voce_id,m.storno_di,m.stornato
             FROM movimento_gestione m JOIN conto_corrente c ON c.id=m.conto_corrente_id
             WHERE m.condominio_id=?1 AND (?2 IS NULL OR m.conto_corrente_id=?2) AND (?3 IS NULL OR m.data>=?3) AND (?4 IS NULL OR m.data<=?4)
             ORDER BY m.data,m.id",
        )?;
        let v = st
            .query_map(params![cond, cc, dal, al], |r| {
                Ok(Movimento {
                    id: r.get(0)?,
                    data: r.get(1)?,
                    descrizione: r.get(2)?,
                    importo: r.get(3)?,
                    conto_id: r.get(4)?,
                    conto: r.get(5)?,
                    origine: r.get(6)?,
                    origine_id: r.get(7)?,
                    voce_id: r.get(8)?,
                    storno_di: r.get(9)?,
                    stornato: r.get::<_, i64>(10)? != 0,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(v)
    }

    /// (entrate, uscite) nel periodo, esclusi i giroconti e i movimenti stornati con i loro storni.
    pub fn riepilogo_cassa(&self, cond: i64, dal: Option<&str>, al: Option<&str>) -> Result<(Cents, Cents)> {
        let v = self.elenco_movimenti(cond, None, dal, al)?;
        let (mut e, mut u) = (0, 0);
        for m in v.iter().filter(|m| m.origine != "giroconto" && !m.stornato && m.storno_di.is_none()) {
            if m.importo > 0 {
                e += m.importo;
            } else {
                u += -m.importo;
            }
        }
        Ok((e, u))
    }

    // ------------------------------------------------------------ situazione e verifiche
    pub fn situazione_gestione(&self, cond: i64) -> Result<Situazione> {
        let q = |sql: &str| -> Result<Cents> { Ok(self.conn.query_row(sql, [cond], |r| r.get(0))?) };
        Ok(Situazione {
            liquidita: q("SELECT COALESCE(SUM(importo_cents),0) FROM movimento_gestione WHERE condominio_id=?1")?,
            crediti_condomini: q(
                "SELECT COALESCE(SUM(r.importo_cents-COALESCE((SELECT SUM(a.importo_cents) FROM allocazione a JOIN incasso i ON i.id=a.incasso_id
                                                                 WHERE a.rata_id=r.id AND i.stato='valido'),0)),0)
                 FROM rata r JOIN piano_rate p ON p.id=r.piano_id JOIN esercizio e ON e.id=p.esercizio_id
                 WHERE e.condominio_id=?1 AND p.stato='attivo'",
            )?,
            salvadanaio: q("SELECT COALESCE(SUM(importo_cents),0) FROM wallet_mov WHERE condominio_id=?1")?,
            debiti_fornitori: q(
                "SELECT COALESCE(SUM(imponibile_cents+iva_cents-ritenuta_cents),0) FROM fattura WHERE condominio_id=?1 AND stato='da_pagare'",
            )?,
            ritenute_da_versare: q(
                "SELECT COALESCE(SUM(ritenuta_cents),0) FROM fattura WHERE condominio_id=?1 AND stato='pagata' AND versamento_id IS NULL",
            )?,
        })
    }

    /// Controlli di coerenza tra movimenti e documenti (incassi, pagamenti, F24). Vuoto = tutto coerente.
    pub fn verifica_movimenti(&self, cond: i64) -> Result<Vec<String>> {
        let q = |sql: &str| -> Result<Cents> { Ok(self.conn.query_row(sql, [cond], |r| r.get(0))?) };
        let mut out = vec![];
        let mut conf = |nome: &str, attesi: Cents, trovati: Cents| {
            if attesi != trovati {
                out.push(format!("{}: documenti {} ma movimenti bancari {}", nome, format_eur(attesi), format_eur(trovati)));
            }
        };
        conf(
            "Incassi",
            q("SELECT COALESCE(SUM(importo_cents),0) FROM incasso WHERE condominio_id=?1 AND stato='valido'")?,
            q("SELECT COALESCE(SUM(importo_cents),0) FROM movimento_gestione WHERE condominio_id=?1 AND origine='incasso' AND stornato=0")?,
        );
        conf(
            "Pagamenti fornitori",
            q("SELECT COALESCE(SUM(imponibile_cents+iva_cents-ritenuta_cents),0) FROM fattura WHERE condominio_id=?1 AND stato='pagata'")?,
            q("SELECT COALESCE(-SUM(importo_cents),0) FROM movimento_gestione WHERE condominio_id=?1 AND origine='pagamento_fattura' AND stornato=0")?,
        );
        conf(
            "Versamenti F24",
            q("SELECT COALESCE(SUM(importo_cents),0) FROM versamento_ritenute WHERE condominio_id=?1")?,
            q("SELECT COALESCE(-SUM(importo_cents),0) FROM movimento_gestione WHERE condominio_id=?1 AND origine='f24' AND stornato=0")?,
        );
        Ok(out)
    }
}
