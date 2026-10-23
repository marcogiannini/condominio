//! Bacheca & Comunicazioni: avvisi, assemblee, circolari; notifiche e-mail con registro degli invii.

use crate::db::Db;
use crate::util::oggi_iso;
use anyhow::{bail, Result};
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use rusqlite::params;

pub const TIPI_COMUNICAZIONE: [&str; 3] = ["avviso", "assemblea", "circolare"];

#[derive(Clone, Debug, Default)]
pub struct Comunicazione {
    pub id: i64,
    pub condominio_id: i64,
    pub tipo: String,
    pub titolo: String,
    pub testo: String,
    pub data_evento: Option<String>,
    pub creato_il: String,
    pub pubblicata: bool,
}

#[derive(Clone, Debug)]
pub struct EmailRiga {
    pub id: i64,
    pub destinatario: String,
    pub oggetto: String,
    pub stato: String,
    pub errore: String,
    pub creato_il: String,
    pub inviato_il: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ConfigSmtp {
    pub host: String,
    pub porta: u16,
    pub utente: String,
    pub password: String,
    pub mittente: String,
    /// true = STARTTLS (porta 587), false = TLS implicito (porta 465)
    pub starttls: bool,
}

impl Db {
    pub fn carica_smtp(&self) -> ConfigSmtp {
        ConfigSmtp {
            host: self.leggi_imp("smtp_host"),
            porta: self.leggi_imp("smtp_porta").parse().unwrap_or(587),
            utente: self.leggi_imp("smtp_utente"),
            password: self.leggi_imp("smtp_password"),
            mittente: self.leggi_imp("smtp_mittente"),
            starttls: self.leggi_imp("smtp_starttls") != "0",
        }
    }

    pub fn salva_smtp(&self, c: &ConfigSmtp) -> Result<()> {
        self.imposta("smtp_host", c.host.trim())?;
        self.imposta("smtp_porta", &c.porta.to_string())?;
        self.imposta("smtp_utente", c.utente.trim())?;
        self.imposta("smtp_password", &c.password)?;
        self.imposta("smtp_mittente", c.mittente.trim())?;
        self.imposta("smtp_starttls", if c.starttls { "1" } else { "0" })?;
        Ok(())
    }

    pub fn salva_comunicazione(&self, c: &Comunicazione) -> Result<i64> {
        if c.titolo.trim().is_empty() {
            bail!("Il titolo è obbligatorio");
        }
        if !TIPI_COMUNICAZIONE.contains(&c.tipo.as_str()) {
            bail!("Tipo non valido");
        }
        if c.id == 0 {
            self.conn.execute(
                "INSERT INTO comunicazione(condominio_id,tipo,titolo,testo,data_evento,creato_il) VALUES(?1,?2,?3,?4,?5,?6)",
                params![c.condominio_id, c.tipo, c.titolo.trim(), c.testo, c.data_evento, oggi_iso()],
            )?;
            Ok(self.conn.last_insert_rowid())
        } else {
            self.conn.execute(
                "UPDATE comunicazione SET tipo=?2,titolo=?3,testo=?4,data_evento=?5 WHERE id=?1",
                params![c.id, c.tipo, c.titolo.trim(), c.testo, c.data_evento],
            )?;
            Ok(c.id)
        }
    }

    pub fn elenco_comunicazioni(&self, cond: i64) -> Result<Vec<Comunicazione>> {
        let mut st = self.conn.prepare(
            "SELECT id,condominio_id,tipo,titolo,testo,data_evento,creato_il,pubblicata FROM comunicazione WHERE condominio_id=?1 ORDER BY id DESC",
        )?;
        let v = st
            .query_map([cond], |r| {
                Ok(Comunicazione {
                    id: r.get(0)?,
                    condominio_id: r.get(1)?,
                    tipo: r.get(2)?,
                    titolo: r.get(3)?,
                    testo: r.get(4)?,
                    data_evento: r.get(5)?,
                    creato_il: r.get(6)?,
                    pubblicata: r.get::<_, i64>(7)? != 0,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(v)
    }

    pub fn elimina_comunicazione(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM comunicazione WHERE id=?1", [id])?;
        Ok(())
    }

    /// Pubblica in bacheca e, se richiesto, accoda un'e-mail per ogni condòmino con indirizzo.
    /// Restituisce il numero di e-mail accodate.
    pub fn pubblica_comunicazione(&self, id: i64, invia_email: bool) -> Result<usize> {
        self.atomico(|db| {
            let (cond, tipo, titolo, testo, evento): (i64, String, String, String, Option<String>) = db.conn.query_row(
                "SELECT condominio_id,tipo,titolo,testo,data_evento FROM comunicazione WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )?;
            db.conn.execute("UPDATE comunicazione SET pubblicata=1 WHERE id=?1", [id])?;
            db.crea_notifica(cond, "bacheca", &format!("Pubblicato ({}): {}", tipo, titolo))?;
            let mut n = 0;
            if invia_email {
                let cn: String = db.conn.query_row("SELECT nome FROM condominio WHERE id=?1", [cond], |r| r.get(0))?;
                let mut corpo = String::new();
                if let Some(e) = &evento {
                    corpo.push_str(&format!("Data: {}\n\n", crate::util::data_it(e)));
                }
                corpo.push_str(&testo);
                corpo.push_str(&format!("\n\n— Amministrazione {}", cn));
                for p in db.persone_del_condominio(cond)? {
                    if p.email.contains('@') {
                        db.conn.execute(
                            "INSERT INTO email_log(comunicazione_id,destinatario,oggetto,corpo,creato_il) VALUES(?1,?2,?3,?4,?5)",
                            params![id, p.email.trim(), format!("[{}] {}", cn, titolo), corpo, chrono::Local::now().format("%Y-%m-%d %H:%M").to_string()],
                        )?;
                        n += 1;
                    }
                }
            }
            Ok(n)
        })
    }

    pub fn elenco_email_log(&self, limite: i64) -> Result<Vec<EmailRiga>> {
        let mut st = self.conn.prepare(
            "SELECT id,destinatario,oggetto,stato,errore,creato_il,inviato_il FROM email_log ORDER BY id DESC LIMIT ?1",
        )?;
        let v = st
            .query_map([limite], |r| {
                Ok(EmailRiga { id: r.get(0)?, destinatario: r.get(1)?, oggetto: r.get(2)?, stato: r.get(3)?, errore: r.get(4)?, creato_il: r.get(5)?, inviato_il: r.get(6)? })
            })?
            .collect::<Result<_, _>>()?;
        Ok(v)
    }

    pub fn email_in_coda(&self) -> Result<i64> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM email_log WHERE stato='in_coda'", [], |r| r.get(0))?)
    }

    /// Accoda una singola e-mail (es. sollecito) nel registro di invio.
    pub fn accoda_email(&self, destinatario: &str, oggetto: &str, corpo: &str) -> Result<()> {
        if !destinatario.contains('@') {
            bail!("Indirizzo e-mail mancante o non valido");
        }
        self.conn.execute(
            "INSERT INTO email_log(comunicazione_id,destinatario,oggetto,corpo,creato_il) VALUES(NULL,?1,?2,?3,?4)",
            params![destinatario.trim(), oggetto, corpo, chrono::Local::now().format("%Y-%m-%d %H:%M").to_string()],
        )?;
        Ok(())
    }

    pub fn riaccoda_errori_email(&self) -> Result<()> {
        self.conn.execute("UPDATE email_log SET stato='in_coda',errore='' WHERE stato='errore'", [])?;
        Ok(())
    }

    /// Invia le e-mail in coda via SMTP. Ogni esito viene registrato nel log. Restituisce (inviate, errori).
    pub fn invia_coda_email(&self) -> Result<(usize, usize)> {
        let cfg = self.carica_smtp();
        if cfg.host.trim().is_empty() || cfg.mittente.trim().is_empty() {
            bail!("Configura prima il server SMTP (host e mittente) nelle Impostazioni");
        }
        let mittente: Mailbox = cfg.mittente.parse().map_err(|_| anyhow::anyhow!("Indirizzo mittente non valido"))?;
        let builder = if cfg.starttls { SmtpTransport::starttls_relay(&cfg.host) } else { SmtpTransport::relay(&cfg.host) }
            .map_err(|e| anyhow::anyhow!("Configurazione SMTP non valida: {}", e))?;
        let mut builder = builder.port(cfg.porta);
        if !cfg.utente.is_empty() {
            builder = builder.credentials(Credentials::new(cfg.utente.clone(), cfg.password.clone()));
        }
        let mailer = builder.build();

        let mut st = self.conn.prepare("SELECT id,destinatario,oggetto,corpo FROM email_log WHERE stato='in_coda' ORDER BY id")?;
        let coda: Vec<(i64, String, String, String)> =
            st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<Result<_, _>>()?;
        drop(st);
        let (mut ok, mut ko) = (0, 0);
        for (id, dest, oggetto, corpo) in coda {
            let esito: Result<()> = (|| {
                let to: Mailbox = dest.parse().map_err(|_| anyhow::anyhow!("indirizzo destinatario non valido"))?;
                let msg = Message::builder().from(mittente.clone()).to(to).subject(oggetto).body(corpo)?;
                mailer.send(&msg)?;
                Ok(())
            })();
            let ora = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
            match esito {
                Ok(_) => {
                    ok += 1;
                    self.conn.execute("UPDATE email_log SET stato='inviata',inviato_il=?2,errore='' WHERE id=?1", params![id, ora])?;
                }
                Err(e) => {
                    ko += 1;
                    self.conn.execute("UPDATE email_log SET stato='errore',errore=?2 WHERE id=?1", params![id, e.to_string()])?;
                }
            }
        }
        Ok((ok, ko))
    }
}
