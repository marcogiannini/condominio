//! Anagrafica & Millesimi: condomìni, persone, unità, titolarità (multi-proprietario), tabelle millesimali.

use crate::db::Db;
use crate::util::*;
use anyhow::{bail, Result};
use rusqlite::{params, OptionalExtension};

#[derive(Clone, Debug, Default)]
pub struct Condominio {
    pub id: i64,
    pub nome: String,
    pub indirizzo: String,
    pub citta: String,
    pub codice_fiscale: String,
    pub note: String,
}

#[derive(Clone, Debug, Default)]
pub struct Persona {
    pub id: i64,
    pub tipo: String,
    pub nome: String,
    pub cognome: String,
    pub codice_fiscale: String,
    pub email: String,
    pub telefono: String,
    pub indirizzo: String,
    pub note: String,
}

impl Persona {
    pub fn nome_completo(&self) -> String {
        format!("{} {}", self.cognome, self.nome).trim().to_string()
    }
}

#[derive(Clone, Debug, Default)]
pub struct Unita {
    pub id: i64,
    pub condominio_id: i64,
    pub scala: String,
    pub piano: String,
    pub interno: String,
    pub tipo: String,
    pub dati_catastali: String,
    pub note: String,
}

impl Unita {
    pub fn etichetta(&self) -> String {
        let mut p = vec![];
        if !self.scala.is_empty() {
            p.push(format!("Sc. {}", self.scala));
        }
        if !self.piano.is_empty() {
            p.push(format!("P. {}", self.piano));
        }
        p.push(format!("Int. {}", if self.interno.is_empty() { "-" } else { &self.interno }));
        format!("{} ({})", p.join(" "), self.tipo)
    }
}

#[derive(Clone, Debug)]
pub struct Titolarita {
    pub id: i64,
    pub unita_id: i64,
    pub persona_id: i64,
    pub persona: String,
    pub ruolo: String,
    pub quota_ppm: i64,
    pub paga_spese: bool,
    pub dal: Option<String>,
    pub al: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Tabella {
    pub id: i64,
    pub condominio_id: i64,
    pub codice: String,
    pub nome: String,
    pub descrizione: String,
    pub totale_micro: i64,
}

pub const RUOLI: [&str; 3] = ["proprietario", "inquilino", "usufruttuario"];
pub const TIPI_UNITA: [&str; 7] = ["appartamento", "box", "cantina", "negozio", "ufficio", "posto auto", "altro"];

impl Db {
    // ------------------------------------------------------------ condominio
    pub fn salva_condominio(&self, c: &Condominio) -> Result<i64> {
        if c.nome.trim().is_empty() {
            bail!("Il nome del condominio è obbligatorio");
        }
        if c.id == 0 {
            self.conn.execute(
                "INSERT INTO condominio(nome,indirizzo,citta,codice_fiscale,note) VALUES(?1,?2,?3,?4,?5)",
                params![c.nome.trim(), c.indirizzo, c.citta, c.codice_fiscale, c.note],
            )?;
            Ok(self.conn.last_insert_rowid())
        } else {
            self.conn.execute(
                "UPDATE condominio SET nome=?2,indirizzo=?3,citta=?4,codice_fiscale=?5,note=?6 WHERE id=?1",
                params![c.id, c.nome.trim(), c.indirizzo, c.citta, c.codice_fiscale, c.note],
            )?;
            Ok(c.id)
        }
    }

    pub fn elenco_condomini(&self) -> Result<Vec<Condominio>> {
        let mut st =
            self.conn.prepare("SELECT id,nome,indirizzo,citta,codice_fiscale,note FROM condominio ORDER BY nome")?;
        let v = st
            .query_map([], |r| {
                Ok(Condominio {
                    id: r.get(0)?,
                    nome: r.get(1)?,
                    indirizzo: r.get(2)?,
                    citta: r.get(3)?,
                    codice_fiscale: r.get(4)?,
                    note: r.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    }

    pub fn elimina_condominio(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM condominio WHERE id=?1", [id])?;
        Ok(())
    }

    // --------------------------------------------------------------- persone
    pub fn salva_persona(&self, p: &Persona) -> Result<i64> {
        if p.cognome.trim().is_empty() {
            bail!("Cognome / ragione sociale obbligatorio");
        }
        if p.id == 0 {
            self.conn.execute(
                "INSERT INTO persona(tipo,nome,cognome,codice_fiscale,email,telefono,indirizzo,note) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                params![p.tipo, p.nome.trim(), p.cognome.trim(), p.codice_fiscale, p.email, p.telefono, p.indirizzo, p.note],
            )?;
            Ok(self.conn.last_insert_rowid())
        } else {
            self.conn.execute(
                "UPDATE persona SET tipo=?2,nome=?3,cognome=?4,codice_fiscale=?5,email=?6,telefono=?7,indirizzo=?8,note=?9 WHERE id=?1",
                params![p.id, p.tipo, p.nome.trim(), p.cognome.trim(), p.codice_fiscale, p.email, p.telefono, p.indirizzo, p.note],
            )?;
            Ok(p.id)
        }
    }

    pub fn elenco_persone(&self) -> Result<Vec<Persona>> {
        let mut st = self.conn.prepare(
            "SELECT id,tipo,nome,cognome,codice_fiscale,email,telefono,indirizzo,note FROM persona ORDER BY cognome,nome",
        )?;
        let v = st
            .query_map([], |r| {
                Ok(Persona {
                    id: r.get(0)?,
                    tipo: r.get(1)?,
                    nome: r.get(2)?,
                    cognome: r.get(3)?,
                    codice_fiscale: r.get(4)?,
                    email: r.get(5)?,
                    telefono: r.get(6)?,
                    indirizzo: r.get(7)?,
                    note: r.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    }

    pub fn elimina_persona(&self, id: i64) -> Result<()> {
        let n: i64 = self.conn.query_row("SELECT COUNT(*) FROM titolarita WHERE persona_id=?1", [id], |r| r.get(0))?;
        if n > 0 {
            bail!("La persona è collegata a delle unità: rimuovi prima le titolarità");
        }
        self.conn
            .execute("DELETE FROM persona WHERE id=?1", [id])
            .map_err(|_| anyhow::anyhow!("La persona ha movimenti contabili e non può essere eliminata"))?;
        Ok(())
    }

    // ----------------------------------------------------------------- unità
    pub fn salva_unita(&self, u: &Unita) -> Result<i64> {
        if u.id == 0 {
            self.conn.execute(
                "INSERT INTO unita(condominio_id,scala,piano,interno,tipo,dati_catastali,note) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![u.condominio_id, u.scala, u.piano, u.interno, u.tipo, u.dati_catastali, u.note],
            )?;
            Ok(self.conn.last_insert_rowid())
        } else {
            self.conn.execute(
                "UPDATE unita SET scala=?2,piano=?3,interno=?4,tipo=?5,dati_catastali=?6,note=?7 WHERE id=?1",
                params![u.id, u.scala, u.piano, u.interno, u.tipo, u.dati_catastali, u.note],
            )?;
            Ok(u.id)
        }
    }

    pub fn elenco_unita(&self, cond: i64) -> Result<Vec<Unita>> {
        let mut st = self.conn.prepare(
            "SELECT id,condominio_id,scala,piano,interno,tipo,dati_catastali,note FROM unita WHERE condominio_id=?1 ORDER BY scala,interno,id",
        )?;
        let v = st
            .query_map([cond], |r| {
                Ok(Unita {
                    id: r.get(0)?,
                    condominio_id: r.get(1)?,
                    scala: r.get(2)?,
                    piano: r.get(3)?,
                    interno: r.get(4)?,
                    tipo: r.get(5)?,
                    dati_catastali: r.get(6)?,
                    note: r.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    }

    pub fn elimina_unita(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM unita WHERE id=?1", [id])
            .map_err(|_| anyhow::anyhow!("L'unità ha rate o movimenti collegati e non può essere eliminata"))?;
        Ok(())
    }

    // ------------------------------------------------------------ titolarità
    pub fn aggiungi_titolarita(
        &self,
        unita: i64,
        persona: i64,
        ruolo: &str,
        quota_ppm: i64,
        paga_spese: bool,
    ) -> Result<i64> {
        if !RUOLI.contains(&ruolo) {
            bail!("Ruolo non valido");
        }
        if quota_ppm <= 0 || quota_ppm > 1_000_000 {
            bail!("La quota deve essere compresa tra 0 e 100%");
        }
        self.conn.execute(
            "INSERT INTO titolarita(unita_id,persona_id,ruolo,quota_ppm,paga_spese) VALUES(?1,?2,?3,?4,?5)",
            params![unita, persona, ruolo, quota_ppm, paga_spese as i64],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn rimuovi_titolarita(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM titolarita WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn elenco_titolari(&self, unita: i64) -> Result<Vec<Titolarita>> {
        let mut st = self.conn.prepare(
            "SELECT t.id,t.unita_id,t.persona_id,p.cognome||' '||p.nome,t.ruolo,t.quota_ppm,t.paga_spese,t.dal,t.al
             FROM titolarita t JOIN persona p ON p.id=t.persona_id WHERE t.unita_id=?1 ORDER BY t.ruolo,t.id",
        )?;
        let v = st
            .query_map([unita], |r| {
                Ok(Titolarita {
                    id: r.get(0)?,
                    unita_id: r.get(1)?,
                    persona_id: r.get(2)?,
                    persona: r.get::<_, String>(3)?.trim().to_string(),
                    ruolo: r.get(4)?,
                    quota_ppm: r.get(5)?,
                    paga_spese: r.get::<_, i64>(6)? != 0,
                    dal: r.get(7)?,
                    al: r.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    }

    /// Soggetti che pagano le spese dell'unità e relativa quota pro-quota (ppm).
    /// Se qualcuno ha `paga_spese` (es. inquilino) paga lui; altrimenti i proprietari.
    pub fn pagatori_unita(&self, unita: i64) -> Result<Vec<(i64, i64)>> {
        let t = self.elenco_titolari(unita)?;
        let mut paganti: Vec<&Titolarita> = t.iter().filter(|x| x.paga_spese).collect();
        if paganti.is_empty() {
            paganti = t.iter().filter(|x| x.ruolo == "proprietario").collect();
        }
        Ok(paganti.iter().map(|x| (x.persona_id, x.quota_ppm)).collect())
    }

    /// Persone (titolari) di un condominio, senza duplicati.
    pub fn persone_del_condominio(&self, cond: i64) -> Result<Vec<Persona>> {
        let mut st = self.conn.prepare(
            "SELECT DISTINCT p.id,p.tipo,p.nome,p.cognome,p.codice_fiscale,p.email,p.telefono,p.indirizzo,p.note
             FROM persona p JOIN titolarita t ON t.persona_id=p.id JOIN unita u ON u.id=t.unita_id
             WHERE u.condominio_id=?1 ORDER BY p.cognome,p.nome",
        )?;
        let v = st
            .query_map([cond], |r| {
                Ok(Persona {
                    id: r.get(0)?,
                    tipo: r.get(1)?,
                    nome: r.get(2)?,
                    cognome: r.get(3)?,
                    codice_fiscale: r.get(4)?,
                    email: r.get(5)?,
                    telefono: r.get(6)?,
                    indirizzo: r.get(7)?,
                    note: r.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    }

    // ------------------------------------------------------ tabelle millesimali
    pub fn salva_tabella(&self, t: &Tabella) -> Result<i64> {
        if t.codice.trim().is_empty() || t.nome.trim().is_empty() {
            bail!("Codice e nome della tabella sono obbligatori");
        }
        if t.id == 0 {
            self.conn
                .execute(
                    "INSERT INTO tabella_millesimale(condominio_id,codice,nome,descrizione,totale_micro) VALUES(?1,?2,?3,?4,?5)",
                    params![t.condominio_id, t.codice.trim(), t.nome.trim(), t.descrizione, t.totale_micro],
                )
                .map_err(|_| anyhow::anyhow!("Esiste già una tabella con questo codice"))?;
            Ok(self.conn.last_insert_rowid())
        } else {
            self.conn.execute(
                "UPDATE tabella_millesimale SET codice=?2,nome=?3,descrizione=?4,totale_micro=?5 WHERE id=?1",
                params![t.id, t.codice.trim(), t.nome.trim(), t.descrizione, t.totale_micro],
            )?;
            Ok(t.id)
        }
    }

    pub fn elenco_tabelle(&self, cond: i64) -> Result<Vec<Tabella>> {
        let mut st = self.conn.prepare(
            "SELECT id,condominio_id,codice,nome,descrizione,totale_micro FROM tabella_millesimale WHERE condominio_id=?1 ORDER BY codice",
        )?;
        let v = st
            .query_map([cond], |r| {
                Ok(Tabella {
                    id: r.get(0)?,
                    condominio_id: r.get(1)?,
                    codice: r.get(2)?,
                    nome: r.get(3)?,
                    descrizione: r.get(4)?,
                    totale_micro: r.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    }

    pub fn elimina_tabella(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM tabella_millesimale WHERE id=?1", [id])
            .map_err(|_| anyhow::anyhow!("La tabella è usata da voci di spesa e non può essere eliminata"))?;
        Ok(())
    }

    pub fn imposta_millesimo(&self, tabella: i64, unita: i64, micro: i64) -> Result<()> {
        if micro < 0 {
            bail!("Il millesimo non può essere negativo");
        }
        if micro == 0 {
            self.conn.execute("DELETE FROM millesimo WHERE tabella_id=?1 AND unita_id=?2", params![tabella, unita])?;
        } else {
            self.conn.execute(
                "INSERT INTO millesimo(tabella_id,unita_id,valore_micro) VALUES(?1,?2,?3)
                 ON CONFLICT(tabella_id,unita_id) DO UPDATE SET valore_micro=?3",
                params![tabella, unita, micro],
            )?;
        }
        Ok(())
    }

    pub fn millesimo(&self, tabella: i64, unita: i64) -> Result<i64> {
        Ok(self
            .conn
            .query_row(
                "SELECT valore_micro FROM millesimo WHERE tabella_id=?1 AND unita_id=?2",
                params![tabella, unita],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }

    pub fn somma_millesimi(&self, tabella: i64) -> Result<i64> {
        Ok(self.conn.query_row(
            "SELECT COALESCE(SUM(valore_micro),0) FROM millesimo WHERE tabella_id=?1",
            [tabella],
            |r| r.get(0),
        )?)
    }

    /// Ripartisce `importo` tra le unità secondo la tabella (penny-perfect).
    pub fn riparto_per_tabella(&self, tabella: i64, importo: Cents) -> Result<Vec<(i64, Cents)>> {
        let mut st = self
            .conn
            .prepare("SELECT unita_id,valore_micro FROM millesimo WHERE tabella_id=?1 AND valore_micro>0 ORDER BY unita_id")?;
        let righe: Vec<(i64, i64)> = st.query_map([tabella], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
        if righe.is_empty() {
            bail!("La tabella millesimale non ha valori assegnati");
        }
        let pesi: Vec<i64> = righe.iter().map(|x| x.1).collect();
        let parti = ripartisci(importo, &pesi);
        Ok(righe.iter().zip(parti).map(|(r, c)| (r.0, c)).collect())
    }
}
