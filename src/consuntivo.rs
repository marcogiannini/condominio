//! Consuntivo (rendiconto), riparto per condòmino e conguaglio di fine esercizio.
//!
//! Il consuntivo di una voce è la somma delle fatture imputate (imponibile + IVA, non stornate).
//! Il riparto usa la tabella millesimale della voce, come per i piani rate.
//! Conguaglio = quota consuntivo − rate emesse: se positivo genera un piano integrativo di rate
//! (delibera obbligatoria), se negativo accredita il Salvadanaio del condòmino (da usare quando decidi tu).

use crate::db::Db;
use crate::util::*;
use anyhow::{bail, Result};
use rusqlite::params;
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Debug)]
pub struct RigaConsuntivo {
    pub voce_id: i64,
    pub parent_id: Option<i64>,
    pub codice: String,
    pub descrizione: String,
    pub foglia: bool,
    pub sopravvenienza: bool,
    pub tabella_id: Option<i64>,
    pub preventivo: Cents,
    pub consuntivo: Cents,
}

impl RigaConsuntivo {
    pub fn scostamento(&self) -> Cents {
        self.consuntivo - self.preventivo
    }
}

#[derive(Clone, Debug, Default)]
pub struct QuotaPersona {
    pub persona_id: i64,
    pub persona: String,
    pub unita: String,
    /// Quota di spesa effettiva (consuntivo) a carico del condòmino.
    pub consuntivo: Cents,
    /// Rate emesse per l'esercizio (esclusi i conguagli).
    pub emesso: Cents,
    /// Incassato sulle rate dell'esercizio (conguaglio compreso).
    pub versato: Cents,
}

impl QuotaPersona {
    /// >0: da versare in più; <0: credito.
    pub fn conguaglio(&self) -> Cents {
        self.consuntivo - self.emesso
    }
    /// Quanto manca rispetto alla spesa effettiva, al netto di quanto già versato.
    pub fn da_versare(&self) -> Cents {
        self.consuntivo - self.versato
    }
}

#[derive(Clone, Debug)]
pub struct StatoConguaglio {
    pub data: String,
    pub delibera_id: i64,
    pub piano_id: Option<i64>,
    pub totale_debito: Cents,
    pub totale_credito: Cents,
}

impl Db {
    /// Voci dell'esercizio con preventivo e consuntivo. Per le voci con sottovoci i totali sono la somma dei discendenti.
    pub fn righe_consuntivo(&self, esercizio: i64) -> Result<Vec<RigaConsuntivo>> {
        let voci = self.elenco_voci(esercizio)?;
        let con_figli: std::collections::HashSet<i64> = voci.iter().filter_map(|v| v.parent_id).collect();
        let mut righe: Vec<RigaConsuntivo> = vec![];
        for v in &voci {
            let foglia = !con_figli.contains(&v.id);
            righe.push(RigaConsuntivo {
                voce_id: v.id,
                parent_id: v.parent_id,
                codice: v.codice.clone(),
                descrizione: v.descrizione.clone(),
                foglia,
                sopravvenienza: v.sopravvenienza,
                tabella_id: v.tabella_id,
                preventivo: if foglia { v.importo_cents } else { 0 },
                consuntivo: self.consuntivo_voce(v.id)?,
            });
        }
        // Le voci con figli sommano i discendenti (le fatture imputate direttamente al capitolo restano in più).
        let idx: HashMap<i64, usize> = righe.iter().enumerate().map(|(i, r)| (r.voce_id, i)).collect();
        let foglie: Vec<(i64, Cents, Cents)> = righe.iter().filter(|r| r.foglia).map(|r| (r.voce_id, r.preventivo, r.consuntivo)).collect();
        for (id, prev, cons) in foglie {
            let mut cur = righe[idx[&id]].parent_id;
            let mut guardia = 0;
            while let Some(p) = cur {
                let i = idx[&p];
                righe[i].preventivo += prev;
                righe[i].consuntivo += cons;
                cur = righe[i].parent_id;
                guardia += 1;
                if guardia > 50 {
                    break;
                }
            }
        }
        Ok(righe)
    }

    /// Fatture dell'esercizio non imputate ad alcuna voce di spesa (non entrano nel riparto).
    pub fn fatture_non_imputate(&self, esercizio: i64) -> Result<Cents> {
        Ok(self.conn.query_row(
            "SELECT COALESCE(SUM(imponibile_cents+iva_cents),0) FROM fattura WHERE esercizio_id=?1 AND voce_id IS NULL AND stato<>'stornata'",
            [esercizio],
            |r| r.get(0),
        )?)
    }

    /// Quote di consuntivo per condòmino (pro-quota tra i co-intestatari), con rate emesse e incassi dell'esercizio.
    pub fn quote_consuntivo(&self, esercizio: i64) -> Result<Vec<QuotaPersona>> {
        let cond: i64 = self.conn.query_row("SELECT condominio_id FROM esercizio WHERE id=?1", [esercizio], |r| r.get(0))?;
        let righe = self.righe_consuntivo(esercizio)?;
        let mut per_unita: BTreeMap<i64, Cents> = BTreeMap::new();
        for r in righe.iter().filter(|r| r.foglia && r.consuntivo > 0) {
            if !self.voce_ripartibile(r.voce_id)? {
                bail!("La voce «{} {}» ha spese registrate ma nessuna tabella millesimale: impossibile ripartire", r.codice, r.descrizione);
            }
            for (u, c) in self.riparto_voce(r.voce_id, r.consuntivo)? {
                *per_unita.entry(u).or_insert(0) += c;
            }
        }
        // direttamente imputate a un capitolo che ha sottovoci: non ripartibili
        for r in righe.iter().filter(|r| !r.foglia) {
            let diretto = self.consuntivo_voce(r.voce_id)?;
            if diretto > 0 {
                bail!("Il capitolo «{} {}» ha fatture imputate direttamente: sposta le fatture su una sottovoce", r.codice, r.descrizione);
            }
        }
        let mut quote: BTreeMap<i64, QuotaPersona> = BTreeMap::new();
        let nome = |db: &Db, p: i64| -> Result<String> {
            Ok(db.conn.query_row("SELECT trim(cognome||' '||nome) FROM persona WHERE id=?1", [p], |r| r.get(0))?)
        };
        for (unita, totale) in per_unita {
            let pagatori = self.pagatori_unita(unita)?;
            if pagatori.is_empty() {
                bail!("L'unità {} non ha un proprietario/pagante: impossibile ripartire il consuntivo", unita);
            }
            let etichetta = self.elenco_unita(cond)?.into_iter().find(|u| u.id == unita).map(|u| u.etichetta()).unwrap_or_default();
            let pesi: Vec<i64> = pagatori.iter().map(|x| x.1).collect();
            for ((pers, _), parte) in pagatori.iter().zip(ripartisci(totale, &pesi)) {
                let q = quote.entry(*pers).or_insert_with(|| QuotaPersona { persona_id: *pers, ..Default::default() });
                if q.persona.is_empty() {
                    q.persona = nome(self, *pers)?;
                }
                q.consuntivo += parte;
                if !q.unita.is_empty() {
                    q.unita.push_str(", ");
                }
                q.unita.push_str(&etichetta);
            }
        }
        // rate emesse (escluso il piano di conguaglio) e incassi
        let piano_cong: Option<i64> = self
            .conn
            .query_row("SELECT piano_id FROM conguaglio WHERE esercizio_id=?1", [esercizio], |r| r.get::<_, Option<i64>>(0))
            .unwrap_or(None);
        let mut st = self.conn.prepare(
            "SELECT r.persona_id,p.id,r.importo_cents,
                    COALESCE((SELECT SUM(a.importo_cents) FROM allocazione a JOIN incasso i ON i.id=a.incasso_id
                              WHERE a.rata_id=r.id AND i.stato='valido'),0)
             FROM rata r JOIN piano_rate p ON p.id=r.piano_id WHERE p.esercizio_id=?1 AND p.stato='attivo'",
        )?;
        let rate: Vec<(i64, i64, Cents, Cents)> =
            st.query_map([esercizio], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<Result<_, _>>()?;
        drop(st);
        for (pers, piano, imp, pag) in rate {
            let q = quote.entry(pers).or_insert_with(|| QuotaPersona { persona_id: pers, ..Default::default() });
            if q.persona.is_empty() {
                q.persona = nome(self, pers)?;
            }
            if Some(piano) != piano_cong {
                q.emesso += imp;
            }
            q.versato += pag;
        }
        let mut v: Vec<QuotaPersona> = quote.into_values().collect();
        v.sort_by(|a, b| a.persona.to_lowercase().cmp(&b.persona.to_lowercase()));
        Ok(v)
    }

    pub fn stato_conguaglio(&self, esercizio: i64) -> Result<Option<StatoConguaglio>> {
        let r = self.conn.query_row(
            "SELECT data,delibera_id,piano_id FROM conguaglio WHERE esercizio_id=?1",
            [esercizio],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, Option<i64>>(2)?)),
        );
        let Ok((data, delibera_id, piano_id)) = r else { return Ok(None) };
        let totale_debito: Cents = match piano_id {
            Some(p) => self.conn.query_row("SELECT COALESCE(SUM(importo_cents),0) FROM rata WHERE piano_id=?1", [p], |r| r.get(0))?,
            None => 0,
        };
        let totale_credito: Cents =
            self.conn.query_row("SELECT COALESCE(SUM(importo_cents),0) FROM conguaglio_credito WHERE esercizio_id=?1", [esercizio], |r| r.get(0))?;
        Ok(Some(StatoConguaglio { data, delibera_id, piano_id, totale_debito, totale_credito }))
    }

    /// Emette il conguaglio: rate integrative per chi deve versare, credito nel Salvadanaio per chi ha versato/emesso in più.
    /// Richiede la delibera di approvazione del rendiconto. `prima_scadenza` in formato ISO.
    pub fn emetti_conguaglio(&self, esercizio: i64, delibera: i64, n_rate: i64, prima_scadenza: &str, cadenza_mesi: i64) -> Result<(usize, usize)> {
        if self.stato_conguaglio(esercizio)?.is_some() {
            bail!("Il conguaglio di questo esercizio è già stato emesso: annullalo prima per rifarlo");
        }
        if !(1..=60).contains(&n_rate) || !(1..=12).contains(&cadenza_mesi) {
            bail!("Numero di rate o cadenza non validi");
        }
        let Some(prima) = parse_data(prima_scadenza) else { bail!("Data della prima scadenza non valida") };
        let (cond, nome_es): (i64, String) =
            self.conn.query_row("SELECT condominio_id,nome FROM esercizio WHERE id=?1", [esercizio], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let (dcond, dtipo): (i64, String) = self
            .conn
            .query_row("SELECT condominio_id,tipo FROM delibera WHERE id=?1", [delibera], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|_| anyhow::anyhow!("Delibera non trovata"))?;
        if dcond != cond {
            bail!("La delibera appartiene a un altro condominio");
        }
        if dtipo != "rendiconto" && dtipo != "integrativa" {
            bail!("Serve una delibera di approvazione del rendiconto (o integrativa), non «{}»", dtipo);
        }
        let quote = self.quote_consuntivo(esercizio)?;
        let debiti: Vec<&QuotaPersona> = quote.iter().filter(|q| q.conguaglio() > 0).collect();
        let crediti: Vec<&QuotaPersona> = quote.iter().filter(|q| q.conguaglio() < 0).collect();
        if debiti.is_empty() && crediti.is_empty() {
            bail!("Nessun conguaglio da emettere: consuntivo e rate emesse coincidono");
        }
        let oggi = oggi_iso();
        self.atomico(|db| {
            let mut piano_id = None;
            if !debiti.is_empty() {
                db.conn.execute(
                    "INSERT INTO piano_rate(esercizio_id,nome,tipo,stato,delibera_id,n_rate,prima_scadenza,cadenza_mesi,creato_il)
                     VALUES(?1,?2,'integrativo','attivo',?3,?4,?5,?6,?7)",
                    params![esercizio, format!("Conguaglio {}", nome_es), delibera, n_rate, prima_scadenza, cadenza_mesi, oggi],
                )?;
                let pid = db.conn.last_insert_rowid();
                for q in &debiti {
                    // la rata va intestata a una unità del pagante
                    let unita: i64 = db.conn.query_row(
                        "SELECT t.unita_id FROM titolarita t JOIN unita u ON u.id=t.unita_id
                         WHERE t.persona_id=?1 AND u.condominio_id=?2 ORDER BY t.paga_spese DESC, t.id LIMIT 1",
                        params![q.persona_id, cond],
                        |r| r.get(0),
                    )?;
                    let parti = ripartisci(q.conguaglio(), &vec![1; n_rate as usize]);
                    for (k, imp) in parti.iter().enumerate() {
                        let scad = aggiungi_mesi(prima, (k as i64 * cadenza_mesi) as u32);
                        db.conn.execute(
                            "INSERT INTO rata(piano_id,unita_id,persona_id,numero,scadenza,importo_cents) VALUES(?1,?2,?3,?4,?5,?6)",
                            params![pid, unita, q.persona_id, k as i64 + 1, scad.format("%Y-%m-%d").to_string(), imp],
                        )?;
                    }
                }
                piano_id = Some(pid);
            }
            db.conn.execute(
                "INSERT INTO conguaglio(esercizio_id,data,delibera_id,piano_id) VALUES(?1,?2,?3,?4)",
                params![esercizio, oggi, delibera, piano_id],
            )?;
            if !crediti.is_empty() {
                for q in &crediti {
                    let c = -q.conguaglio();
                    db.conn.execute(
                        "INSERT INTO conguaglio_credito(esercizio_id,persona_id,importo_cents) VALUES(?1,?2,?3)",
                        params![esercizio, q.persona_id, c],
                    )?;
                    db.conn.execute(
                        "INSERT INTO wallet_mov(condominio_id,persona_id,data,importo_cents,causale) VALUES(?1,?2,?3,?4,?5)",
                        params![cond, q.persona_id, oggi, c, format!("Conguaglio a credito {}", nome_es)],
                    )?;
                }
            }
            Ok((debiti.len(), crediti.len()))
        })
    }

    /// Annulla il conguaglio: possibile solo se nessuna rata di conguaglio è stata incassata e i crediti non sono stati usati.
    pub fn annulla_conguaglio(&self, esercizio: i64) -> Result<()> {
        let Some(c) = self.stato_conguaglio(esercizio)? else { bail!("Nessun conguaglio emesso per questo esercizio") };
        let cond: i64 = self.conn.query_row("SELECT condominio_id FROM esercizio WHERE id=?1", [esercizio], |r| r.get(0))?;
        if let Some(p) = c.piano_id {
            let inc: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM allocazione a JOIN rata r ON r.id=a.rata_id JOIN incasso i ON i.id=a.incasso_id
                 WHERE r.piano_id=?1 AND i.stato='valido'",
                [p],
                |r| r.get(0),
            )?;
            if inc > 0 {
                bail!("Esistono incassi sulle rate di conguaglio: stornali prima");
            }
        }
        let mut st = self.conn.prepare("SELECT persona_id,importo_cents FROM conguaglio_credito WHERE esercizio_id=?1")?;
        let crediti: Vec<(i64, Cents)> = st.query_map([esercizio], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
        drop(st);
        for (pers, imp) in &crediti {
            if self.saldo_salvadanaio(cond, *pers)? < *imp {
                bail!("Il credito di conguaglio di un condòmino è già stato utilizzato: storna prima quell'incasso");
            }
        }
        let oggi = oggi_iso();
        self.atomico(|db| {
            for (pers, imp) in &crediti {
                db.conn.execute(
                    "INSERT INTO wallet_mov(condominio_id,persona_id,data,importo_cents,causale) VALUES(?1,?2,?3,?4,'Storno conguaglio a credito')",
                    params![cond, pers, oggi, -imp],
                )?;
            }
            if let Some(p) = c.piano_id {
                db.conn.execute("UPDATE piano_rate SET stato='annullato' WHERE id=?1", [p])?;
            }
            db.conn.execute("DELETE FROM conguaglio WHERE esercizio_id=?1", [esercizio])?;
            Ok(())
        })
    }

    /// Dettaglio individuale: quota di consuntivo di ogni voce a carico di un condòmino (pro-quota sulle sue unità).
    pub fn dettaglio_persona_consuntivo(&self, esercizio: i64, persona: i64) -> Result<Vec<(String, String, Cents)>> {
        let mut out = vec![];
        for r in self.righe_consuntivo(esercizio)?.into_iter().filter(|r| r.foglia && r.consuntivo > 0) {
            let Some(tab) = r.tabella_id else { continue };
            let mut quota = 0;
            for (u, c) in self.riparto_per_tabella(tab, r.consuntivo)? {
                let pag = self.pagatori_unita(u)?;
                if pag.is_empty() {
                    continue;
                }
                let pesi: Vec<i64> = pag.iter().map(|x| x.1).collect();
                for ((p, _), parte) in pag.iter().zip(ripartisci(c, &pesi)) {
                    if *p == persona {
                        quota += parte;
                    }
                }
            }
            if quota > 0 {
                out.push((r.codice.clone(), r.descrizione.clone(), quota));
            }
        }
        Ok(out)
    }
}
