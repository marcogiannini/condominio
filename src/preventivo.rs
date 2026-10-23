//! Preventivo & Piano dei Conti, Delibere, Piani Rate (ordinari, straordinari Art. 1135, integrativi).

use crate::db::Db;
use crate::util::*;
use anyhow::{bail, Result};
use rusqlite::params;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default)]
pub struct Esercizio {
    pub id: i64,
    pub condominio_id: i64,
    pub nome: String,
    pub dal: String,
    pub al: String,
    pub stato: String,
}

#[derive(Clone, Debug, Default)]
pub struct VoceSpesa {
    pub id: i64,
    pub esercizio_id: i64,
    pub parent_id: Option<i64>,
    pub codice: String,
    pub descrizione: String,
    pub importo_cents: Cents,
    pub tabella_id: Option<i64>,
    pub sopravvenienza: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Delibera {
    pub id: i64,
    pub condominio_id: i64,
    pub esercizio_id: Option<i64>,
    pub data: String,
    pub tipo: String,
    pub oggetto: String,
    pub riferimento: String,
    pub maggioranza: String,
}

#[derive(Clone, Debug, Default)]
pub struct Piano {
    pub id: i64,
    pub esercizio_id: i64,
    pub nome: String,
    pub tipo: String,
    pub stato: String,
    pub delibera_id: Option<i64>,
    pub n_rate: i64,
    pub prima_scadenza: String,
    pub cadenza_mesi: i64,
}

#[derive(Clone, Debug)]
pub struct RataCalc {
    pub unita_id: i64,
    pub persona_id: i64,
    pub numero: i64,
    pub scadenza: String,
    pub importo: Cents,
}

#[derive(Clone, Debug)]
pub struct RataRiga {
    pub id: i64,
    pub unita_id: i64,
    pub persona_id: i64,
    pub numero: i64,
    pub scadenza: String,
    pub importo: Cents,
    pub pagato: Cents,
}

pub const TIPI_PIANO: [&str; 3] = ["ordinario", "straordinario", "integrativo"];
pub const TIPI_DELIBERA: [&str; 5] = ["preventivo", "rendiconto", "straordinaria", "integrativa", "altro"];

impl Db {
    // ------------------------------------------------------------- esercizi
    pub fn salva_esercizio(&self, e: &Esercizio) -> Result<i64> {
        if e.nome.trim().is_empty() {
            bail!("Il nome dell'esercizio è obbligatorio");
        }
        let (Some(dal), Some(al)) = (data_iso(&e.dal), data_iso(&e.al)) else {
            bail!("Date dell'esercizio non valide (usa GG/MM/AAAA)");
        };
        if dal > al {
            bail!("La data di fine precede quella di inizio");
        }
        if e.id == 0 {
            self.conn.execute(
                "INSERT INTO esercizio(condominio_id,nome,dal,al) VALUES(?1,?2,?3,?4)",
                params![e.condominio_id, e.nome.trim(), dal, al],
            )?;
            Ok(self.conn.last_insert_rowid())
        } else {
            self.conn.execute(
                "UPDATE esercizio SET nome=?2,dal=?3,al=?4,stato=?5 WHERE id=?1",
                params![e.id, e.nome.trim(), dal, al, e.stato],
            )?;
            Ok(e.id)
        }
    }

    pub fn elenco_esercizi(&self, cond: i64) -> Result<Vec<Esercizio>> {
        let mut st = self
            .conn
            .prepare("SELECT id,condominio_id,nome,dal,al,stato FROM esercizio WHERE condominio_id=?1 ORDER BY dal DESC")?;
        let v = st
            .query_map([cond], |r| {
                Ok(Esercizio {
                    id: r.get(0)?,
                    condominio_id: r.get(1)?,
                    nome: r.get(2)?,
                    dal: r.get(3)?,
                    al: r.get(4)?,
                    stato: r.get(5)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(v)
    }

    pub fn imposta_stato_esercizio(&self, id: i64, stato: &str) -> Result<()> {
        self.conn.execute("UPDATE esercizio SET stato=?2 WHERE id=?1", params![id, stato])?;
        Ok(())
    }

    // ---------------------------------------------------------------- delibere
    pub fn salva_delibera(&self, d: &Delibera) -> Result<i64> {
        let Some(data) = data_iso(&d.data) else {
            bail!("Data della delibera non valida (usa GG/MM/AAAA)");
        };
        if d.oggetto.trim().is_empty() {
            bail!("L'oggetto della delibera è obbligatorio");
        }
        if d.riferimento.trim().is_empty() {
            bail!("Indica il riferimento al verbale assembleare");
        }
        if d.id == 0 {
            self.conn.execute(
                "INSERT INTO delibera(condominio_id,esercizio_id,data,tipo,oggetto,riferimento,maggioranza) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![d.condominio_id, d.esercizio_id, data, d.tipo, d.oggetto.trim(), d.riferimento.trim(), d.maggioranza],
            )?;
            Ok(self.conn.last_insert_rowid())
        } else {
            self.conn.execute(
                "UPDATE delibera SET esercizio_id=?2,data=?3,tipo=?4,oggetto=?5,riferimento=?6,maggioranza=?7 WHERE id=?1",
                params![d.id, d.esercizio_id, data, d.tipo, d.oggetto.trim(), d.riferimento.trim(), d.maggioranza],
            )?;
            Ok(d.id)
        }
    }

    pub fn elenco_delibere(&self, cond: i64) -> Result<Vec<Delibera>> {
        let mut st = self.conn.prepare(
            "SELECT id,condominio_id,esercizio_id,data,tipo,oggetto,riferimento,maggioranza FROM delibera WHERE condominio_id=?1 ORDER BY data DESC",
        )?;
        let v = st
            .query_map([cond], |r| {
                Ok(Delibera {
                    id: r.get(0)?,
                    condominio_id: r.get(1)?,
                    esercizio_id: r.get(2)?,
                    data: r.get(3)?,
                    tipo: r.get(4)?,
                    oggetto: r.get(5)?,
                    riferimento: r.get(6)?,
                    maggioranza: r.get(7)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(v)
    }

    // ------------------------------------------------- piano dei conti (voci)
    pub fn salva_voce(&self, v: &VoceSpesa) -> Result<i64> {
        if v.codice.trim().is_empty() || v.descrizione.trim().is_empty() {
            bail!("Codice e descrizione sono obbligatori");
        }
        if v.importo_cents < 0 {
            bail!("L'importo preventivato non può essere negativo");
        }
        if v.id != 0 {
            if v.parent_id == Some(v.id) {
                bail!("Una voce non può essere figlia di sé stessa");
            }
            // una voce ancorata a un piano attivo non può cambiare importo: i riparti sarebbero incoerenti
            let attivi: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM piano_voce pv JOIN piano_rate p ON p.id=pv.piano_id WHERE pv.voce_id=?1 AND p.stato='attivo'",
                [v.id],
                |r| r.get(0),
            )?;
            let (old_imp, old_tab): (Cents, Option<i64>) = self.conn.query_row(
                "SELECT importo_cents,tabella_id FROM voce_spesa WHERE id=?1",
                [v.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            if attivi > 0 && (old_imp != v.importo_cents || old_tab != v.tabella_id) {
                bail!("La voce è ancorata a un piano rate attivo: importo e tabella non sono modificabili");
            }
            self.conn.execute(
                "UPDATE voce_spesa SET parent_id=?2,codice=?3,descrizione=?4,importo_cents=?5,tabella_id=?6,sopravvenienza=?7 WHERE id=?1",
                params![v.id, v.parent_id, v.codice.trim(), v.descrizione.trim(), v.importo_cents, v.tabella_id, v.sopravvenienza as i64],
            )?;
            Ok(v.id)
        } else {
            self.conn.execute(
                "INSERT INTO voce_spesa(esercizio_id,parent_id,codice,descrizione,importo_cents,tabella_id,sopravvenienza) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![v.esercizio_id, v.parent_id, v.codice.trim(), v.descrizione.trim(), v.importo_cents, v.tabella_id, v.sopravvenienza as i64],
            )?;
            Ok(self.conn.last_insert_rowid())
        }
    }

    pub fn elenco_voci(&self, esercizio: i64) -> Result<Vec<VoceSpesa>> {
        let mut st = self.conn.prepare(
            "SELECT id,esercizio_id,parent_id,codice,descrizione,importo_cents,tabella_id,sopravvenienza
             FROM voce_spesa WHERE esercizio_id=?1 ORDER BY codice",
        )?;
        let v = st
            .query_map([esercizio], |r| {
                Ok(VoceSpesa {
                    id: r.get(0)?,
                    esercizio_id: r.get(1)?,
                    parent_id: r.get(2)?,
                    codice: r.get(3)?,
                    descrizione: r.get(4)?,
                    importo_cents: r.get(5)?,
                    tabella_id: r.get(6)?,
                    sopravvenienza: r.get::<_, i64>(7)? != 0,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(v)
    }

    /// Blocco di cancellazione: non si elimina una voce con figli, con fatture o ancorata a piani attivi.
    pub fn elimina_voce(&self, id: i64) -> Result<()> {
        let figli: i64 = self.conn.query_row("SELECT COUNT(*) FROM voce_spesa WHERE parent_id=?1", [id], |r| r.get(0))?;
        if figli > 0 {
            bail!("La voce ha delle sottovoci: eliminale prima");
        }
        let attivi: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM piano_voce pv JOIN piano_rate p ON p.id=pv.piano_id WHERE pv.voce_id=?1 AND p.stato='attivo'",
            [id],
            |r| r.get(0),
        )?;
        if attivi > 0 {
            bail!("Voce ancorata a un piano rate ATTIVO: annulla prima il piano per poterla eliminare");
        }
        let fatt: i64 = self.conn.query_row("SELECT COUNT(*) FROM fattura WHERE voce_id=?1", [id], |r| r.get(0))?;
        if fatt > 0 {
            bail!("Alla voce sono collegate delle fatture");
        }
        self.conn.execute("DELETE FROM piano_voce WHERE voce_id=?1", [id])?;
        self.conn.execute("DELETE FROM voce_spesa WHERE id=?1", [id])?;
        Ok(())
    }

    /// Totale preventivato (somma delle sole foglie: le voci che non hanno figli).
    pub fn totale_preventivo(&self, esercizio: i64) -> Result<Cents> {
        Ok(self.conn.query_row(
            "SELECT COALESCE(SUM(importo_cents),0) FROM voce_spesa v
             WHERE esercizio_id=?1 AND NOT EXISTS(SELECT 1 FROM voce_spesa f WHERE f.parent_id=v.id)",
            [esercizio],
            |r| r.get(0),
        )?)
    }

    /// Consuntivo di una voce: fatture registrate (non stornate) imputate alla voce.
    pub fn consuntivo_voce(&self, voce: i64) -> Result<Cents> {
        Ok(self.conn.query_row(
            "SELECT COALESCE((SELECT SUM(imponibile_cents+iva_cents) FROM fattura WHERE voce_id=?1 AND stato<>'stornata'),0)
                  + COALESCE((SELECT -SUM(importo_cents) FROM movimento_gestione
                              WHERE voce_id=?1 AND origine='manuale' AND stornato=0),0)",
            [voce],
            |r| r.get(0),
        )?)
    }

    // --------------------------------------------------------------- piani rate
    pub fn crea_piano(&self, p: &Piano) -> Result<i64> {
        if p.nome.trim().is_empty() {
            bail!("Il nome del piano è obbligatorio");
        }
        if !TIPI_PIANO.contains(&p.tipo.as_str()) {
            bail!("Tipo di piano non valido");
        }
        let Some(prima) = data_iso(&p.prima_scadenza) else {
            bail!("Data della prima scadenza non valida (usa GG/MM/AAAA)");
        };
        if p.n_rate < 1 || p.n_rate > 60 {
            bail!("Il numero di rate deve essere tra 1 e 60");
        }
        if p.cadenza_mesi < 1 || p.cadenza_mesi > 12 {
            bail!("La cadenza deve essere tra 1 e 12 mesi");
        }
        self.conn.execute(
            "INSERT INTO piano_rate(esercizio_id,nome,tipo,delibera_id,n_rate,prima_scadenza,cadenza_mesi,creato_il) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![p.esercizio_id, p.nome.trim(), p.tipo, p.delibera_id, p.n_rate, prima, p.cadenza_mesi, oggi_iso()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn elenco_piani(&self, esercizio: i64) -> Result<Vec<Piano>> {
        let mut st = self.conn.prepare(
            "SELECT id,esercizio_id,nome,tipo,stato,delibera_id,n_rate,prima_scadenza,cadenza_mesi FROM piano_rate WHERE esercizio_id=?1 ORDER BY id",
        )?;
        let v = st
            .query_map([esercizio], |r| {
                Ok(Piano {
                    id: r.get(0)?,
                    esercizio_id: r.get(1)?,
                    nome: r.get(2)?,
                    tipo: r.get(3)?,
                    stato: r.get(4)?,
                    delibera_id: r.get(5)?,
                    n_rate: r.get(6)?,
                    prima_scadenza: r.get(7)?,
                    cadenza_mesi: r.get(8)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(v)
    }

    fn piano(&self, id: i64) -> Result<Piano> {
        let mut st = self.conn.prepare(
            "SELECT id,esercizio_id,nome,tipo,stato,delibera_id,n_rate,prima_scadenza,cadenza_mesi FROM piano_rate WHERE id=?1",
        )?;
        let p = st.query_row([id], |r| {
            Ok(Piano {
                id: r.get(0)?,
                esercizio_id: r.get(1)?,
                nome: r.get(2)?,
                tipo: r.get(3)?,
                stato: r.get(4)?,
                delibera_id: r.get(5)?,
                n_rate: r.get(6)?,
                prima_scadenza: r.get(7)?,
                cadenza_mesi: r.get(8)?,
            })
        })?;
        Ok(p)
    }

    pub fn imposta_delibera_piano(&self, piano: i64, delibera: Option<i64>) -> Result<()> {
        if self.piano(piano)?.stato != "bozza" {
            bail!("Il piano non è più in bozza");
        }
        self.conn.execute("UPDATE piano_rate SET delibera_id=?2 WHERE id=?1", params![piano, delibera])?;
        Ok(())
    }

    /// Aggiunge/aggiorna una voce nel piano (solo in bozza). Importo di default = preventivo della voce.
    pub fn imposta_voce_piano(&self, piano: i64, voce: i64, importo: Cents) -> Result<()> {
        if self.piano(piano)?.stato != "bozza" {
            bail!("Il piano non è in bozza: voci non modificabili");
        }
        if importo <= 0 {
            self.conn.execute("DELETE FROM piano_voce WHERE piano_id=?1 AND voce_id=?2", params![piano, voce])?;
        } else {
            self.conn.execute(
                "INSERT INTO piano_voce(piano_id,voce_id,importo_cents) VALUES(?1,?2,?3)
                 ON CONFLICT(piano_id,voce_id) DO UPDATE SET importo_cents=?3",
                params![piano, voce, importo],
            )?;
        }
        Ok(())
    }

    pub fn voci_piano(&self, piano: i64) -> Result<Vec<(i64, String, Cents)>> {
        let mut st = self.conn.prepare(
            "SELECT v.id,v.codice||' '||v.descrizione,pv.importo_cents FROM piano_voce pv JOIN voce_spesa v ON v.id=pv.voce_id
             WHERE pv.piano_id=?1 ORDER BY v.codice",
        )?;
        let v = st.query_map([piano], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<Result<_, _>>()?;
        Ok(v)
    }

    pub fn totale_piano(&self, piano: i64) -> Result<Cents> {
        // Bozza: somma delle voci. Piano emesso: somma delle rate (copre anche i piani di conguaglio, senza voci).
        let (stato, nrate): (String, i64) = self.conn.query_row(
            "SELECT stato,(SELECT COUNT(*) FROM rata WHERE piano_id=piano_rate.id) FROM piano_rate WHERE id=?1",
            [piano],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if stato != "bozza" && nrate > 0 {
            return Ok(self.conn.query_row("SELECT COALESCE(SUM(importo_cents),0) FROM rata WHERE piano_id=?1", [piano], |r| r.get(0))?);
        }
        Ok(self.conn.query_row("SELECT COALESCE(SUM(importo_cents),0) FROM piano_voce WHERE piano_id=?1", [piano], |r| r.get(0))?)
    }

    pub fn elimina_piano_bozza(&self, piano: i64) -> Result<()> {
        if self.piano(piano)?.stato != "bozza" {
            bail!("Solo i piani in bozza possono essere eliminati (gli altri si annullano)");
        }
        self.conn.execute("DELETE FROM piano_rate WHERE id=?1", [piano])?;
        Ok(())
    }

    /// Calcolo del riparto e delle rate (senza scrivere nulla): usato per anteprima e attivazione.
    pub fn calcola_rate(&self, piano: i64) -> Result<Vec<RataCalc>> {
        let p = self.piano(piano)?;
        let voci = self.voci_piano(piano)?;
        if voci.is_empty() {
            bail!("Il piano non contiene voci di spesa");
        }
        let mut per_unita: BTreeMap<i64, Cents> = BTreeMap::new();
        for (voce_id, nome, importo) in voci {
            if !self.voce_ripartibile(voce_id)? {
                bail!("La voce «{}» non ha una tabella millesimale associata", nome);
            }
            for (u, c) in self.riparto_voce(voce_id, importo)? {
                *per_unita.entry(u).or_insert(0) += c;
            }
        }
        let prima = parse_data(&p.prima_scadenza).ok_or_else(|| anyhow::anyhow!("Data di prima scadenza non valida"))?;
        let n = p.n_rate as usize;
        let mut out = vec![];
        for (unita, totale) in per_unita {
            if totale == 0 {
                continue;
            }
            let pagatori = self.pagatori_unita(unita)?;
            if pagatori.is_empty() {
                bail!("L'unità {} non ha un proprietario/pagante: impossibile emettere le rate", unita);
            }
            let pesi: Vec<i64> = pagatori.iter().map(|x| x.1).collect();
            let quote = ripartisci(totale, &pesi); // pro-quota tra i co-intestatari
            for (i, (persona, _)) in pagatori.iter().enumerate() {
                let rate = ripartisci(quote[i], &vec![1; n]);
                for (k, imp) in rate.iter().enumerate() {
                    if *imp == 0 {
                        continue;
                    }
                    let scad = aggiungi_mesi(prima, (k as i64 * p.cadenza_mesi) as u32);
                    out.push(RataCalc {
                        unita_id: unita,
                        persona_id: *persona,
                        numero: k as i64 + 1,
                        scadenza: scad.format("%Y-%m-%d").to_string(),
                        importo: *imp,
                    });
                }
            }
        }
        Ok(out)
    }

    /// Attiva il piano: richiede la delibera assembleare, genera le rate (i crediti verso i condòmini derivano dalle rate).
    pub fn attiva_piano(&self, piano: i64) -> Result<usize> {
        let p = self.piano(piano)?;
        if p.stato != "bozza" {
            bail!("Il piano è già {}", p.stato);
        }
        if p.delibera_id.is_none() {
            bail!("Delibera assembleare obbligatoria: collega al piano la delibera che lo approva");
        }
        let rate = self.calcola_rate(piano)?;
        if rate.is_empty() {
            bail!("Nessuna rata da emettere");
        }
        self.atomico(|db| {
            for r in &rate {
                db.conn.execute(
                    "INSERT INTO rata(piano_id,unita_id,persona_id,numero,scadenza,importo_cents) VALUES(?1,?2,?3,?4,?5,?6)",
                    params![piano, r.unita_id, r.persona_id, r.numero, r.scadenza, r.importo],
                )?;
            }
            db.conn.execute("UPDATE piano_rate SET stato='attivo' WHERE id=?1", [piano])?;
            Ok(rate.len())
        })
    }

    /// Annulla un piano attivo (solo se nessuna rata ha incassi validi).
    pub fn annulla_piano(&self, piano: i64) -> Result<()> {
        let p = self.piano(piano)?;
        if p.stato != "attivo" {
            bail!("Solo i piani attivi possono essere annullati");
        }
        let incassi: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM allocazione a JOIN rata r ON r.id=a.rata_id JOIN incasso i ON i.id=a.incasso_id
             WHERE r.piano_id=?1 AND i.stato='valido'",
            [piano],
            |r| r.get(0),
        )?;
        if incassi > 0 {
            bail!("Esistono incassi su queste rate: stornali prima di annullare il piano");
        }
        self.atomico(|db| {
            db.conn.execute("UPDATE piano_rate SET stato='annullato' WHERE id=?1", [piano])?;
            Ok(())
        })
    }

    pub fn rate_piano(&self, piano: i64) -> Result<Vec<RataRiga>> {
        let mut st = self.conn.prepare(
            "SELECT r.id,r.unita_id,r.persona_id,r.numero,r.scadenza,r.importo_cents,
                    COALESCE((SELECT SUM(a.importo_cents) FROM allocazione a JOIN incasso i ON i.id=a.incasso_id
                              WHERE a.rata_id=r.id AND i.stato='valido'),0)
             FROM rata r WHERE r.piano_id=?1 ORDER BY r.scadenza,r.unita_id,r.persona_id",
        )?;
        let v = st
            .query_map([piano], |r| {
                Ok(RataRiga {
                    id: r.get(0)?,
                    unita_id: r.get(1)?,
                    persona_id: r.get(2)?,
                    numero: r.get(3)?,
                    scadenza: r.get(4)?,
                    importo: r.get(5)?,
                    pagato: r.get(6)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(v)
    }
}
