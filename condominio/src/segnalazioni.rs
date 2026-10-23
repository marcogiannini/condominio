//! Segnalazioni guasto: ticketing interno con priorità, stati, commenti e notifiche.

use crate::db::Db;
use anyhow::{bail, Result};
use rusqlite::params;

pub const PRIORITA: [&str; 4] = ["Bassa", "Normale", "Alta", "Urgente"];
pub const STATI_TICKET: [&str; 5] = ["aperta", "in_carico", "in_attesa", "risolta", "chiusa"];

fn adesso() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M").to_string()
}

#[derive(Clone, Debug)]
pub struct Ticket {
    pub id: i64,
    pub unita_id: Option<i64>,
    pub segnalante: String,
    pub titolo: String,
    pub descrizione: String,
    pub priorita: i64,
    pub stato: String,
    pub creato_il: String,
    pub aggiornato_il: String,
}

#[derive(Clone, Debug)]
pub struct Notifica {
    pub id: i64,
    pub creato_il: String,
    pub tipo: String,
    pub messaggio: String,
    pub letta: bool,
}

impl Db {
    pub fn crea_notifica(&self, cond: i64, tipo: &str, messaggio: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO notifica(condominio_id,creato_il,tipo,messaggio) VALUES(?1,?2,?3,?4)",
            params![cond, adesso(), tipo, messaggio],
        )?;
        Ok(())
    }

    pub fn elenco_notifiche(&self, cond: i64, limite: i64) -> Result<Vec<Notifica>> {
        let mut st = self.conn.prepare(
            "SELECT id,creato_il,tipo,messaggio,letta FROM notifica WHERE condominio_id=?1 ORDER BY id DESC LIMIT ?2",
        )?;
        let v = st
            .query_map(params![cond, limite], |r| {
                Ok(Notifica { id: r.get(0)?, creato_il: r.get(1)?, tipo: r.get(2)?, messaggio: r.get(3)?, letta: r.get::<_, i64>(4)? != 0 })
            })?
            .collect::<Result<_, _>>()?;
        Ok(v)
    }

    pub fn notifiche_non_lette(&self, cond: i64) -> Result<i64> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM notifica WHERE condominio_id=?1 AND letta=0", [cond], |r| r.get(0))?)
    }

    pub fn segna_notifiche_lette(&self, cond: i64) -> Result<()> {
        self.conn.execute("UPDATE notifica SET letta=1 WHERE condominio_id=?1", [cond])?;
        Ok(())
    }

    pub fn crea_ticket(&self, cond: i64, unita: Option<i64>, persona: Option<i64>, titolo: &str, descrizione: &str, priorita: i64) -> Result<i64> {
        if titolo.trim().is_empty() {
            bail!("Il titolo della segnalazione è obbligatorio");
        }
        if !(0..=3).contains(&priorita) {
            bail!("Priorità non valida");
        }
        self.atomico(|db| {
            let t = adesso();
            db.conn.execute(
                "INSERT INTO ticket(condominio_id,unita_id,persona_id,titolo,descrizione,priorita,creato_il,aggiornato_il) VALUES(?1,?2,?3,?4,?5,?6,?7,?7)",
                params![cond, unita, persona, titolo.trim(), descrizione, priorita, t],
            )?;
            let id = db.conn.last_insert_rowid();
            db.crea_notifica(cond, "segnalazione", &format!("Nuova segnalazione [{}]: {}", PRIORITA[priorita as usize], titolo.trim()))?;
            Ok(id)
        })
    }

    pub fn elenco_ticket(&self, cond: i64, stato: Option<&str>, solo_aperti: bool) -> Result<Vec<Ticket>> {
        let mut st = self.conn.prepare(
            "SELECT t.id,t.unita_id,COALESCE(p.cognome||' '||p.nome,'—'),t.titolo,t.descrizione,t.priorita,t.stato,t.creato_il,t.aggiornato_il
             FROM ticket t LEFT JOIN persona p ON p.id=t.persona_id
             WHERE t.condominio_id=?1 AND (?2 IS NULL OR t.stato=?2) AND (?3=0 OR t.stato NOT IN ('risolta','chiusa'))
             ORDER BY t.priorita DESC,t.aggiornato_il DESC",
        )?;
        let v = st
            .query_map(params![cond, stato, solo_aperti as i64], |r| {
                Ok(Ticket {
                    id: r.get(0)?,
                    unita_id: r.get(1)?,
                    segnalante: r.get::<_, String>(2)?.trim().to_string(),
                    titolo: r.get(3)?,
                    descrizione: r.get(4)?,
                    priorita: r.get(5)?,
                    stato: r.get(6)?,
                    creato_il: r.get(7)?,
                    aggiornato_il: r.get(8)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(v)
    }

    pub fn cambia_stato_ticket(&self, id: i64, stato: &str, autore: &str) -> Result<()> {
        if !STATI_TICKET.contains(&stato) {
            bail!("Stato non valido");
        }
        self.atomico(|db| {
            let (cond, titolo, vecchio): (i64, String, String) =
                db.conn.query_row("SELECT condominio_id,titolo,stato FROM ticket WHERE id=?1", [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            if vecchio == stato {
                return Ok(());
            }
            db.conn.execute("UPDATE ticket SET stato=?2,aggiornato_il=?3 WHERE id=?1", params![id, stato, adesso()])?;
            db.conn.execute(
                "INSERT INTO ticket_commento(ticket_id,autore,testo,creato_il) VALUES(?1,?2,?3,?4)",
                params![id, autore, format!("Stato: {} -> {}", vecchio, stato), adesso()],
            )?;
            db.crea_notifica(cond, "segnalazione", &format!("Segnalazione «{}»: stato {}", titolo, stato))?;
            Ok(())
        })
    }

    pub fn cambia_priorita_ticket(&self, id: i64, priorita: i64) -> Result<()> {
        if !(0..=3).contains(&priorita) {
            bail!("Priorità non valida");
        }
        self.conn.execute("UPDATE ticket SET priorita=?2,aggiornato_il=?3 WHERE id=?1", params![id, priorita, adesso()])?;
        Ok(())
    }

    pub fn aggiungi_commento_ticket(&self, id: i64, autore: &str, testo: &str) -> Result<()> {
        if testo.trim().is_empty() {
            bail!("Il commento è vuoto");
        }
        self.conn.execute(
            "INSERT INTO ticket_commento(ticket_id,autore,testo,creato_il) VALUES(?1,?2,?3,?4)",
            params![id, autore, testo.trim(), adesso()],
        )?;
        self.conn.execute("UPDATE ticket SET aggiornato_il=?2 WHERE id=?1", params![id, adesso()])?;
        Ok(())
    }

    pub fn commenti_ticket(&self, id: i64) -> Result<Vec<(String, String, String)>> {
        let mut st = self.conn.prepare("SELECT creato_il,autore,testo FROM ticket_commento WHERE ticket_id=?1 ORDER BY id")?;
        let v = st.query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<Result<_, _>>()?;
        Ok(v)
    }

    pub fn elimina_ticket(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM ticket WHERE id=?1", [id])?;
        Ok(())
    }
}
