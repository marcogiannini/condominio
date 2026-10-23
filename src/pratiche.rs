//! Pratiche: interventi dei fornitori (dalla richiesta alla chiusura), sinistri e pratiche legali.

use crate::db::Db;
use crate::util::*;
use anyhow::{bail, Result};
use rusqlite::params;

pub const TIPI_PRATICA: [&str; 3] = ["intervento", "sinistro", "legale"];

pub fn stati_pratica(tipo: &str) -> &'static [&'static str] {
    match tipo {
        "intervento" => &["richiesto", "assegnato", "in_corso", "completato", "chiuso"],
        "sinistro" => &["aperto", "denunciato", "in_istruttoria", "liquidato", "respinto", "chiuso"],
        _ => &["aperta", "in_corso", "sospesa", "conclusa", "archiviata"],
    }
}

pub fn stato_finale(tipo: &str, stato: &str) -> bool {
    matches!((tipo, stato), ("intervento", "chiuso") | ("sinistro", "chiuso") | ("sinistro", "respinto") | ("legale", "conclusa") | ("legale", "archiviata"))
}

#[derive(Clone, Debug, Default)]
pub struct Pratica {
    pub id: i64,
    pub condominio_id: i64,
    pub tipo: String,
    pub titolo: String,
    pub descrizione: String,
    pub stato: String,
    pub fornitore_id: Option<i64>,
    pub unita_id: Option<i64>,
    pub ticket_id: Option<i64>,
    pub riferimento: String,
    pub controparte: String,
    pub importo: Cents,
    pub data_apertura: String,
    pub data_chiusura: Option<String>,
}

impl Db {
    pub fn salva_pratica(&self, p: &Pratica) -> Result<i64> {
        if !TIPI_PRATICA.contains(&p.tipo.as_str()) {
            bail!("Tipo di pratica non valido");
        }
        if p.titolo.trim().is_empty() {
            bail!("Il titolo è obbligatorio");
        }
        let apertura = if p.data_apertura.trim().is_empty() { oggi_iso() } else { data_iso(&p.data_apertura).ok_or_else(|| anyhow::anyhow!("Data di apertura non valida"))? };
        if p.importo < 0 {
            bail!("L'importo non può essere negativo");
        }
        if p.id == 0 {
            let stato = stati_pratica(&p.tipo)[0];
            self.conn.execute(
                "INSERT INTO pratica(condominio_id,tipo,titolo,descrizione,stato,fornitore_id,unita_id,ticket_id,riferimento,controparte,importo_cents,data_apertura,creato_il)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
                params![p.condominio_id, p.tipo, p.titolo.trim(), p.descrizione, stato, p.fornitore_id, p.unita_id, p.ticket_id, p.riferimento, p.controparte, p.importo, apertura, oggi_iso()],
            )?;
            let id = self.conn.last_insert_rowid();
            self.aggiungi_evento_pratica(id, "Pratica aperta")?;
            Ok(id)
        } else {
            self.conn.execute(
                "UPDATE pratica SET titolo=?2,descrizione=?3,fornitore_id=?4,unita_id=?5,riferimento=?6,controparte=?7,importo_cents=?8,data_apertura=?9 WHERE id=?1",
                params![p.id, p.titolo.trim(), p.descrizione, p.fornitore_id, p.unita_id, p.riferimento, p.controparte, p.importo, apertura],
            )?;
            Ok(p.id)
        }
    }

    pub fn elenco_pratiche(&self, cond: i64, tipo: Option<&str>, solo_aperte: bool) -> Result<Vec<Pratica>> {
        let mut st = self.conn.prepare(
            "SELECT id,condominio_id,tipo,titolo,descrizione,stato,fornitore_id,unita_id,ticket_id,riferimento,controparte,importo_cents,data_apertura,data_chiusura
             FROM pratica WHERE condominio_id=?1 AND (?2 IS NULL OR tipo=?2) AND (?3=0 OR data_chiusura IS NULL) ORDER BY data_apertura DESC,id DESC",
        )?;
        let v = st
            .query_map(params![cond, tipo, solo_aperte as i64], |r| {
                Ok(Pratica {
                    id: r.get(0)?,
                    condominio_id: r.get(1)?,
                    tipo: r.get(2)?,
                    titolo: r.get(3)?,
                    descrizione: r.get(4)?,
                    stato: r.get(5)?,
                    fornitore_id: r.get(6)?,
                    unita_id: r.get(7)?,
                    ticket_id: r.get(8)?,
                    riferimento: r.get(9)?,
                    controparte: r.get(10)?,
                    importo: r.get(11)?,
                    data_apertura: r.get(12)?,
                    data_chiusura: r.get(13)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(v)
    }

    pub fn cambia_stato_pratica(&self, id: i64, stato: &str) -> Result<()> {
        let (cond, tipo, titolo, vecchio): (i64, String, String, String) =
            self.conn.query_row("SELECT condominio_id,tipo,titolo,stato FROM pratica WHERE id=?1", [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        if !stati_pratica(&tipo).contains(&stato) {
            bail!("Stato non valido per una pratica di tipo {}", tipo);
        }
        if vecchio == stato {
            return Ok(());
        }
        self.atomico(|db| {
            let chiusura = if stato_finale(&tipo, stato) { Some(oggi_iso()) } else { None };
            db.conn.execute("UPDATE pratica SET stato=?2,data_chiusura=?3 WHERE id=?1", params![id, stato, chiusura])?;
            db.aggiungi_evento_pratica(id, &format!("Stato: {} → {}", vecchio.replace('_', " "), stato.replace('_', " ")))?;
            db.crea_notifica(cond, "pratica", &format!("{} «{}»: {}", tipo, titolo, stato.replace('_', " ")))?;
            Ok(())
        })
    }

    pub fn aggiungi_evento_pratica(&self, id: i64, testo: &str) -> Result<()> {
        if testo.trim().is_empty() {
            bail!("Testo vuoto");
        }
        self.conn.execute("INSERT INTO pratica_evento(pratica_id,data,testo) VALUES(?1,?2,?3)", params![id, oggi_iso(), testo.trim()])?;
        Ok(())
    }

    pub fn eventi_pratica(&self, id: i64) -> Result<Vec<(String, String)>> {
        let mut st = self.conn.prepare("SELECT data,testo FROM pratica_evento WHERE pratica_id=?1 ORDER BY id")?;
        let v = st.query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
        Ok(v)
    }

    pub fn elimina_pratica(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM pratica WHERE id=?1", [id])?;
        Ok(())
    }

    /// Trasforma una segnalazione guasto in un intervento affidato a un fornitore; la segnalazione passa «in carico».
    pub fn intervento_da_ticket(&self, ticket: i64, fornitore: Option<i64>) -> Result<i64> {
        let (cond, unita, titolo, descr): (i64, Option<i64>, String, String) = self.conn.query_row(
            "SELECT condominio_id,unita_id,titolo,descrizione FROM ticket WHERE id=?1",
            [ticket],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        let esiste: i64 = self.conn.query_row("SELECT COUNT(*) FROM pratica WHERE ticket_id=?1", [ticket], |r| r.get(0))?;
        if esiste > 0 {
            bail!("Per questa segnalazione esiste già un intervento");
        }
        self.atomico(|db| {
            let id = db.salva_pratica(&Pratica {
                condominio_id: cond,
                tipo: "intervento".into(),
                titolo,
                descrizione: descr,
                fornitore_id: fornitore,
                unita_id: unita,
                ticket_id: Some(ticket),
                ..Default::default()
            })?;
            if fornitore.is_some() {
                db.cambia_stato_pratica(id, "assegnato")?;
            }
            db.cambia_stato_ticket(ticket, "in_carico", "Amministratore")?;
            Ok(id)
        })
    }
}
