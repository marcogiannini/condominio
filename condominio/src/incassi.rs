//! Incassi & Smart Wallet (Salvadanaio): allocazione dalla rata più vecchia o manuale,
//! compensazione dei crediti pregressi su decisione dell'amministratore, storno "self-healing".
//!
//! Self-healing: nessun contatore "pagato" è memorizzato. Lo stato di ogni rata è sempre derivato
//! dalle allocazioni degli incassi VALIDI, quindi uno storno non può mai lasciare dati sfalsati.

use crate::db::Db;
use crate::util::*;
use anyhow::{bail, Result};
use rusqlite::params;

#[derive(Clone, Debug)]
pub struct RataAperta {
    pub rata_id: i64,
    pub piano: String,
    pub unita_id: i64,
    pub numero: i64,
    pub scadenza: String,
    pub importo: Cents,
    pub pagato: Cents,
}

impl RataAperta {
    pub fn residuo(&self) -> Cents {
        self.importo - self.pagato
    }
}

#[derive(Clone, Debug, Default)]
pub struct NuovoIncasso {
    pub condominio_id: i64,
    pub persona_id: i64,
    pub conto_corrente_id: Option<i64>,
    pub data: String, // ISO
    pub importo: Cents,
    /// Quanto credito del Salvadanaio compensare in questo incasso (decide l'amministratore).
    pub usa_salvadanaio: Cents,
    /// Distribuzione manuale (rata, importo). Se None: dalla rata più vecchia.
    pub distribuzione: Option<Vec<(i64, Cents)>>,
    pub metodo: String,
    pub note: String,
}

#[derive(Clone, Debug)]
pub struct IncassoRiga {
    pub id: i64,
    pub data: String,
    pub persona: String,
    pub importo: Cents,
    pub da_salvadanaio: Cents,
    pub eccedenza: Cents,
    pub allocato: Cents,
    pub metodo: String,
    pub stato: String,
}

#[derive(Clone, Debug)]
pub struct MovEstratto {
    pub data: String,
    pub descrizione: String,
    pub dare: Cents,
    pub avere: Cents,
    pub saldo: Cents,
}

#[derive(Clone, Debug, Default)]
pub struct SituazionePersona {
    pub dovuto: Cents,
    pub versato: Cents,
    pub residuo: Cents,
    pub salvadanaio: Cents,
}

impl Db {
    pub fn rate_aperte(&self, cond: i64, persona: i64) -> Result<Vec<RataAperta>> {
        let mut st = self.conn.prepare(
            "SELECT r.id,p.nome,r.unita_id,r.numero,r.scadenza,r.importo_cents,
                    COALESCE((SELECT SUM(a.importo_cents) FROM allocazione a JOIN incasso i ON i.id=a.incasso_id
                              WHERE a.rata_id=r.id AND i.stato='valido'),0)
             FROM rata r JOIN piano_rate p ON p.id=r.piano_id JOIN esercizio e ON e.id=p.esercizio_id
             WHERE r.persona_id=?1 AND e.condominio_id=?2 AND p.stato='attivo'
             ORDER BY r.scadenza,r.numero,r.id",
        )?;
        let v: Vec<RataAperta> = st
            .query_map(params![persona, cond], |r| {
                Ok(RataAperta {
                    rata_id: r.get(0)?,
                    piano: r.get(1)?,
                    unita_id: r.get(2)?,
                    numero: r.get(3)?,
                    scadenza: r.get(4)?,
                    importo: r.get(5)?,
                    pagato: r.get(6)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(v.into_iter().filter(|r| r.residuo() > 0).collect())
    }

    pub fn saldo_salvadanaio(&self, cond: i64, persona: i64) -> Result<Cents> {
        Ok(self.conn.query_row(
            "SELECT COALESCE(SUM(importo_cents),0) FROM wallet_mov WHERE condominio_id=?1 AND persona_id=?2",
            params![cond, persona],
            |r| r.get(0),
        )?)
    }

    pub fn movimenti_salvadanaio(&self, cond: i64, persona: i64) -> Result<Vec<(String, String, Cents)>> {
        let mut st = self.conn.prepare(
            "SELECT data,causale,importo_cents FROM wallet_mov WHERE condominio_id=?1 AND persona_id=?2 ORDER BY data,id",
        )?;
        let v = st.query_map(params![cond, persona], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<Result<_, _>>()?;
        Ok(v)
    }

    /// Anteprima dell'allocazione automatica (dalla rata più vecchia): (rata, importo).
    pub fn anteprima_allocazione(&self, cond: i64, persona: i64, disponibile: Cents) -> Result<(Vec<(RataAperta, Cents)>, Cents)> {
        let mut resto = disponibile;
        let mut out = vec![];
        for r in self.rate_aperte(cond, persona)? {
            if resto <= 0 {
                break;
            }
            let a = resto.min(r.residuo());
            out.push((r, a));
            resto -= a;
        }
        Ok((out, resto))
    }

    pub fn registra_incasso(&self, n: &NuovoIncasso) -> Result<i64> {
        if n.importo < 0 || n.usa_salvadanaio < 0 {
            bail!("Gli importi non possono essere negativi");
        }
        if n.importo == 0 && n.usa_salvadanaio == 0 {
            bail!("Inserisci un importo da incassare o un credito del Salvadanaio da compensare");
        }
        if parse_data(&n.data).is_none() {
            bail!("Data non valida");
        }
        if n.importo > 0 && n.conto_corrente_id.is_none() {
            bail!("Seleziona il conto corrente su cui è entrato l'incasso");
        }
        let saldo_w = self.saldo_salvadanaio(n.condominio_id, n.persona_id)?;
        if n.usa_salvadanaio > saldo_w {
            bail!("Il Salvadanaio contiene solo {}", format_eur(saldo_w));
        }
        let disponibile = n.importo + n.usa_salvadanaio;

        self.atomico(|db| {
            // 1) distribuzione
            let alloc: Vec<(i64, Cents)> = match &n.distribuzione {
                Some(man) => {
                    let aperte = db.rate_aperte(n.condominio_id, n.persona_id)?;
                    let mut tot = 0;
                    for (rid, imp) in man {
                        if *imp <= 0 {
                            continue;
                        }
                        let Some(r) = aperte.iter().find(|r| r.rata_id == *rid) else {
                            bail!("La rata {} non è aperta per questa persona", rid);
                        };
                        if *imp > r.residuo() {
                            bail!("Importo eccessivo per la rata del {}: residuo {}", data_it(&r.scadenza), format_eur(r.residuo()));
                        }
                        tot += imp;
                    }
                    if tot > disponibile {
                        bail!("Hai distribuito {} ma l'importo disponibile è {}", format_eur(tot), format_eur(disponibile));
                    }
                    man.iter().filter(|x| x.1 > 0).cloned().collect()
                }
                None => {
                    let (v, _) = db.anteprima_allocazione(n.condominio_id, n.persona_id, disponibile)?;
                    v.into_iter().map(|(r, a)| (r.rata_id, a)).collect()
                }
            };
            let allocato: Cents = alloc.iter().map(|x| x.1).sum();
            let eccedenza = disponibile - allocato;
            if eccedenza > n.importo {
                bail!(
                    "Il credito del Salvadanaio compensato ({}) supera quanto serve a coprire le rate",
                    format_eur(n.usa_salvadanaio)
                );
            }

            // 2) registro incasso
            db.conn.execute(
                "INSERT INTO incasso(condominio_id,persona_id,conto_corrente_id,data,importo_cents,da_salvadanaio_cents,eccedenza_cents,metodo,note)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![n.condominio_id, n.persona_id, n.conto_corrente_id, n.data, n.importo, n.usa_salvadanaio, eccedenza, n.metodo, n.note],
            )?;
            let inc = db.conn.last_insert_rowid();
            for (rid, imp) in &alloc {
                db.conn.execute(
                    "INSERT INTO allocazione(incasso_id,rata_id,importo_cents) VALUES(?1,?2,?3)",
                    params![inc, rid, imp],
                )?;
            }
            // 3) Salvadanaio
            if n.usa_salvadanaio > 0 {
                db.conn.execute(
                    "INSERT INTO wallet_mov(condominio_id,persona_id,data,importo_cents,causale,incasso_id) VALUES(?1,?2,?3,?4,?5,?6)",
                    params![n.condominio_id, n.persona_id, n.data, -n.usa_salvadanaio, "Compensazione su rate", inc],
                )?;
            }
            if eccedenza > 0 {
                db.conn.execute(
                    "INSERT INTO wallet_mov(condominio_id,persona_id,data,importo_cents,causale,incasso_id) VALUES(?1,?2,?3,?4,?5,?6)",
                    params![n.condominio_id, n.persona_id, n.data, eccedenza, "Eccedenza incasso (credito)", inc],
                )?;
            }
            // 4) movimento di cassa (solo il versato: la compensazione col Salvadanaio non muove la banca)
            if n.importo > 0 {
                let pers: String =
                    db.conn.query_row("SELECT cognome||' '||nome FROM persona WHERE id=?1", [n.persona_id], |r| r.get(0))?;
                let mov = db.registra_movimento(
                    n.condominio_id,
                    n.conto_corrente_id.unwrap(),
                    &n.data,
                    &format!("Incasso {} – {}", n.metodo, pers.trim()),
                    n.importo,
                    "incasso",
                    Some(inc),
                )?;
                db.conn.execute("UPDATE incasso SET movimento_id=?2 WHERE id=?1", params![inc, mov])?;
            }
            Ok(inc)
        })
    }

    /// Storna un incasso: rate e Salvadanaio tornano come prima, il conto corrente riceve il movimento inverso.
    pub fn storna_incasso(&self, incasso: i64, data: &str) -> Result<()> {
        self.atomico(|db| {
            let (cond, pers, stato, usa, ecc, reg): (i64, i64, String, Cents, Cents, Option<i64>) = db.conn.query_row(
                "SELECT condominio_id,persona_id,stato,da_salvadanaio_cents,eccedenza_cents,movimento_id FROM incasso WHERE id=?1",
                [incasso],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )?;
            if stato != "valido" {
                bail!("Incasso già stornato");
            }
            let saldo = db.saldo_salvadanaio(cond, pers)?;
            if saldo + usa - ecc < 0 {
                bail!("Il credito generato da questo incasso è già stato utilizzato: storna prima gli incassi successivi");
            }
            if usa > 0 {
                db.conn.execute(
                    "INSERT INTO wallet_mov(condominio_id,persona_id,data,importo_cents,causale,incasso_id) VALUES(?1,?2,?3,?4,'Storno: credito restituito',?5)",
                    params![cond, pers, data, usa, incasso],
                )?;
            }
            if ecc > 0 {
                db.conn.execute(
                    "INSERT INTO wallet_mov(condominio_id,persona_id,data,importo_cents,causale,incasso_id) VALUES(?1,?2,?3,?4,'Storno: eccedenza rimossa',?5)",
                    params![cond, pers, data, -ecc, incasso],
                )?;
            }
            let storno_mov = match reg {
                Some(m) => Some(db.storna_movimento(m, data, "storno incasso")?),
                None => None,
            };
            db.conn.execute(
                "UPDATE incasso SET stato='stornato',data_storno=?2,storno_movimento_id=?3 WHERE id=?1",
                params![incasso, data, storno_mov],
            )?;
            Ok(())
        })
    }

    pub fn elenco_incassi(&self, cond: i64, persona: Option<i64>) -> Result<Vec<IncassoRiga>> {
        let mut st = self.conn.prepare(
            "SELECT i.id,i.data,p.cognome||' '||p.nome,i.importo_cents,i.da_salvadanaio_cents,i.eccedenza_cents,
                    COALESCE((SELECT SUM(importo_cents) FROM allocazione WHERE incasso_id=i.id),0),i.metodo,i.stato
             FROM incasso i JOIN persona p ON p.id=i.persona_id
             WHERE i.condominio_id=?1 AND (?2 IS NULL OR i.persona_id=?2) ORDER BY i.data DESC,i.id DESC",
        )?;
        let v = st
            .query_map(params![cond, persona], |r| {
                Ok(IncassoRiga {
                    id: r.get(0)?,
                    data: r.get(1)?,
                    persona: r.get::<_, String>(2)?.trim().to_string(),
                    importo: r.get(3)?,
                    da_salvadanaio: r.get(4)?,
                    eccedenza: r.get(5)?,
                    allocato: r.get(6)?,
                    metodo: r.get(7)?,
                    stato: r.get(8)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(v)
    }

    pub fn estratto_conto(&self, cond: i64, persona: i64) -> Result<Vec<MovEstratto>> {
        let mut righe: Vec<(String, u8, String, Cents, Cents)> = vec![];
        let mut st = self.conn.prepare(
            "SELECT r.scadenza,p.nome,r.numero,p.n_rate,r.importo_cents,u.scala,u.interno
             FROM rata r JOIN piano_rate p ON p.id=r.piano_id JOIN esercizio e ON e.id=p.esercizio_id JOIN unita u ON u.id=r.unita_id
             WHERE r.persona_id=?1 AND e.condominio_id=?2 AND p.stato='attivo'",
        )?;
        let rows = st.query_map(params![persona, cond], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, Cents>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
            ))
        })?;
        for x in rows {
            let (scad, piano, num, tot, imp, scala, interno) = x?;
            righe.push((scad, 0, format!("Rata {}/{} – {} (Sc.{} Int.{})", num, tot, piano, scala, interno), imp, 0));
        }
        drop(st);
        let mut st = self.conn.prepare(
            "SELECT i.data,i.metodo,COALESCE((SELECT SUM(importo_cents) FROM allocazione WHERE incasso_id=i.id),0),i.da_salvadanaio_cents
             FROM incasso i WHERE i.persona_id=?1 AND i.condominio_id=?2 AND i.stato='valido'",
        )?;
        let rows = st.query_map(params![persona, cond], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Cents>(2)?, r.get::<_, Cents>(3)?))
        })?;
        for x in rows {
            let (data, metodo, alloc, usa) = x?;
            if alloc > 0 {
                let extra = if usa > 0 { format!(" (di cui {} da Salvadanaio)", format_eur(usa)) } else { String::new() };
                righe.push((data, 1, format!("Incasso {}{}", metodo, extra), 0, alloc));
            }
        }
        righe.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        let mut saldo = 0;
        Ok(righe
            .into_iter()
            .map(|(data, _, descrizione, dare, avere)| {
                saldo += dare - avere;
                MovEstratto { data, descrizione, dare, avere, saldo }
            })
            .collect())
    }

    /// Dovuto/versato/residuo (sulle rate attive) e saldo Salvadanaio di una persona.
    pub fn situazione_persona(&self, cond: i64, persona: i64) -> Result<SituazionePersona> {
        let (dovuto, versato): (Cents, Cents) = self.conn.query_row(
            "SELECT COALESCE(SUM(r.importo_cents),0),
                    COALESCE(SUM((SELECT COALESCE(SUM(a.importo_cents),0) FROM allocazione a JOIN incasso i ON i.id=a.incasso_id
                                  WHERE a.rata_id=r.id AND i.stato='valido')),0)
             FROM rata r JOIN piano_rate p ON p.id=r.piano_id JOIN esercizio e ON e.id=p.esercizio_id
             WHERE r.persona_id=?1 AND e.condominio_id=?2 AND p.stato='attivo'",
            params![persona, cond],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok(SituazionePersona { dovuto, versato, residuo: dovuto - versato, salvadanaio: self.saldo_salvadanaio(cond, persona)? })
    }
}
