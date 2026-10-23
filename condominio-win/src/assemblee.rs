//! Assemblee: convocazione, presenze, quorum costitutivo e deliberativo (art. 1136, 1120 c.c.), voti, verbale.
//!
//! Conteggio delle "teste": ogni unità rappresenta il suo proprietario principale (quota maggiore); una persona
//! che possiede più unità conta una sola volta. I comproprietari di una stessa unità contano come un solo condòmino.

use crate::anagrafica::Unita;
use crate::db::Db;
use crate::preventivo::Delibera;
use crate::util::*;
use anyhow::{bail, Result};
use chrono::{Duration, NaiveDate};
use rusqlite::params;
use std::collections::{BTreeSet, HashMap};

pub const MAGGIORANZE: [(&str, &str); 4] = [
    ("ordinaria", "Ordinaria – art. 1136 c. 2-3 c.c."),
    ("qualificata", "Qualificata – art. 1136 c. 4 c.c. (nomina/revoca amministratore, liti, lavori straordinari di notevole entità)"),
    ("innovazioni", "Innovazioni – art. 1120 c. 1 c.c. (2/3 del valore)"),
    ("unanimita", "Unanimità"),
];

#[derive(Clone, Debug, Default)]
pub struct Assemblea {
    pub id: i64,
    pub condominio_id: i64,
    pub esercizio_id: Option<i64>,
    pub tipo: String,     // ordinaria | straordinaria
    pub stato: String,    // bozza | convocata | tenuta | verbalizzata
    pub modalita: String, // presenza | online | mista
    pub luogo: String,
    pub data_prima: String, // GG/MM/AAAA in input, ISO in lettura
    pub ora_prima: String,
    pub data_seconda: Option<String>,
    pub ora_seconda: String,
    pub tabella_id: Option<i64>,
    pub convocazione_tenuta: Option<i64>,
    pub presidente: String,
    pub segretario: String,
    pub comunicazione_id: Option<i64>,
}

#[derive(Clone, Debug, Default)]
pub struct Punto {
    pub id: i64,
    pub assemblea_id: i64,
    pub numero: i64,
    pub titolo: String,
    pub descrizione: String,
    pub maggioranza: String,
    pub tipo_delibera: String,
    pub delibera_id: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct Presenza {
    pub unita_id: i64,
    pub unita: String,
    pub titolare: String,
    pub rappresentante: String,
    pub millesimi_micro: i64,
}

#[derive(Clone, Debug)]
pub struct Quorum {
    pub convocazione: i64,
    pub millesimi_presenti: i64,
    pub millesimi_totali: i64,
    pub teste_presenti: usize,
    pub teste_totali: usize,
    pub costituita: bool,
    pub spiegazione: String,
}

#[derive(Clone, Debug, Default)]
pub struct EsitoVoto {
    pub fav_mill: i64,
    pub con_mill: i64,
    pub ast_mill: i64,
    pub fav_teste: usize,
    pub con_teste: usize,
    pub ast_teste: usize,
    pub presenti_mill: i64,
    pub presenti_teste: usize,
    pub totali_mill: i64,
    pub totali_teste: usize,
    pub approvato: bool,
    pub regola: String,
}

fn mill(micro: i64) -> String {
    format!("{}‰", format_millesimi(micro))
}

impl Db {
    // ------------------------------------------------------------------ anagrafica assemblea
    pub fn salva_assemblea(&self, a: &Assemblea) -> Result<i64> {
        let Some(prima) = data_iso(&a.data_prima) else { bail!("Data della prima convocazione non valida (usa GG/MM/AAAA)") };
        let seconda = match a.data_seconda.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(s) => Some(data_iso(s).ok_or_else(|| anyhow::anyhow!("Data della seconda convocazione non valida"))?),
            None => None,
        };
        if let Some(s) = &seconda {
            let (p, s) = (parse_data(&prima).unwrap(), parse_data(s).unwrap());
            if s <= p || s > p + Duration::days(10) {
                bail!("La seconda convocazione deve essere in un giorno successivo alla prima e non oltre dieci giorni (art. 1136 c. 3 c.c.)");
            }
        }
        if !["ordinaria", "straordinaria"].contains(&a.tipo.as_str()) || !["presenza", "online", "mista"].contains(&a.modalita.as_str()) {
            bail!("Tipo o modalità non validi");
        }
        if a.id == 0 {
            self.conn.execute(
                "INSERT INTO assemblea(condominio_id,esercizio_id,tipo,modalita,luogo,data_prima,ora_prima,data_seconda,ora_seconda,tabella_id,presidente,segretario,creato_il)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
                params![a.condominio_id, a.esercizio_id, a.tipo, a.modalita, a.luogo, prima, a.ora_prima, seconda, a.ora_seconda, a.tabella_id, a.presidente, a.segretario, oggi_iso()],
            )?;
            Ok(self.conn.last_insert_rowid())
        } else {
            let stato: String = self.conn.query_row("SELECT stato FROM assemblea WHERE id=?1", [a.id], |r| r.get(0))?;
            if stato == "verbalizzata" {
                bail!("L'assemblea è già verbalizzata: non modificabile");
            }
            self.conn.execute(
                "UPDATE assemblea SET esercizio_id=?2,tipo=?3,modalita=?4,luogo=?5,data_prima=?6,ora_prima=?7,data_seconda=?8,ora_seconda=?9,tabella_id=?10,presidente=?11,segretario=?12 WHERE id=?1",
                params![a.id, a.esercizio_id, a.tipo, a.modalita, a.luogo, prima, a.ora_prima, seconda, a.ora_seconda, a.tabella_id, a.presidente, a.segretario],
            )?;
            Ok(a.id)
        }
    }

    pub fn elenco_assemblee(&self, cond: i64) -> Result<Vec<Assemblea>> {
        let mut st = self.conn.prepare(
            "SELECT id,condominio_id,esercizio_id,tipo,stato,modalita,luogo,data_prima,ora_prima,data_seconda,ora_seconda,tabella_id,
                    convocazione_tenuta,presidente,segretario,comunicazione_id FROM assemblea WHERE condominio_id=?1 ORDER BY data_prima DESC,id DESC",
        )?;
        let v = st
            .query_map([cond], |r| {
                Ok(Assemblea {
                    id: r.get(0)?,
                    condominio_id: r.get(1)?,
                    esercizio_id: r.get(2)?,
                    tipo: r.get(3)?,
                    stato: r.get(4)?,
                    modalita: r.get(5)?,
                    luogo: r.get(6)?,
                    data_prima: r.get(7)?,
                    ora_prima: r.get(8)?,
                    data_seconda: r.get(9)?,
                    ora_seconda: r.get(10)?,
                    tabella_id: r.get(11)?,
                    convocazione_tenuta: r.get(12)?,
                    presidente: r.get(13)?,
                    segretario: r.get(14)?,
                    comunicazione_id: r.get(15)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(v)
    }

    pub fn assemblea(&self, id: i64) -> Result<Assemblea> {
        let cond: i64 = self.conn.query_row("SELECT condominio_id FROM assemblea WHERE id=?1", [id], |r| r.get(0))?;
        self.elenco_assemblee(cond)?.into_iter().find(|a| a.id == id).ok_or_else(|| anyhow::anyhow!("Assemblea non trovata"))
    }

    pub fn elimina_assemblea(&self, id: i64) -> Result<()> {
        if self.assemblea(id)?.stato == "verbalizzata" {
            bail!("Un'assemblea verbalizzata non si elimina");
        }
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM assemblea_punto WHERE assemblea_id=?1 AND delibera_id IS NOT NULL",
            [id],
            |r| r.get(0),
        )?;
        if n > 0 {
            bail!("Dall'assemblea sono state generate delle delibere: non è eliminabile");
        }
        self.conn.execute("DELETE FROM assemblea WHERE id=?1", [id])?;
        Ok(())
    }

    // ------------------------------------------------------------------ ordine del giorno
    pub fn salva_punto(&self, p: &Punto) -> Result<i64> {
        if p.titolo.trim().is_empty() {
            bail!("Il titolo del punto è obbligatorio");
        }
        if !MAGGIORANZE.iter().any(|m| m.0 == p.maggioranza) {
            bail!("Maggioranza non valida");
        }
        let stato = self.assemblea(p.assemblea_id)?.stato;
        if stato == "tenuta" || stato == "verbalizzata" {
            bail!("L'ordine del giorno non è modificabile dopo lo svolgimento dell'assemblea");
        }
        if p.id == 0 {
            let numero: i64 = if p.numero > 0 {
                p.numero
            } else {
                self.conn.query_row("SELECT COALESCE(MAX(numero),0)+1 FROM assemblea_punto WHERE assemblea_id=?1", [p.assemblea_id], |r| r.get(0))?
            };
            self.conn.execute(
                "INSERT INTO assemblea_punto(assemblea_id,numero,titolo,descrizione,maggioranza,tipo_delibera) VALUES(?1,?2,?3,?4,?5,?6)",
                params![p.assemblea_id, numero, p.titolo.trim(), p.descrizione, p.maggioranza, p.tipo_delibera],
            )?;
            Ok(self.conn.last_insert_rowid())
        } else {
            self.conn.execute(
                "UPDATE assemblea_punto SET numero=?2,titolo=?3,descrizione=?4,maggioranza=?5,tipo_delibera=?6 WHERE id=?1",
                params![p.id, p.numero, p.titolo.trim(), p.descrizione, p.maggioranza, p.tipo_delibera],
            )?;
            Ok(p.id)
        }
    }

    pub fn elenco_punti(&self, assemblea: i64) -> Result<Vec<Punto>> {
        let mut st = self.conn.prepare(
            "SELECT id,assemblea_id,numero,titolo,descrizione,maggioranza,tipo_delibera,delibera_id FROM assemblea_punto WHERE assemblea_id=?1 ORDER BY numero,id",
        )?;
        let v = st
            .query_map([assemblea], |r| {
                Ok(Punto {
                    id: r.get(0)?,
                    assemblea_id: r.get(1)?,
                    numero: r.get(2)?,
                    titolo: r.get(3)?,
                    descrizione: r.get(4)?,
                    maggioranza: r.get(5)?,
                    tipo_delibera: r.get(6)?,
                    delibera_id: r.get(7)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(v)
    }

    pub fn elimina_punto(&self, id: i64) -> Result<()> {
        let (ass, deliberato): (i64, Option<i64>) =
            self.conn.query_row("SELECT assemblea_id,delibera_id FROM assemblea_punto WHERE id=?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        if deliberato.is_some() {
            bail!("Dal punto è stata generata una delibera");
        }
        let stato = self.assemblea(ass)?.stato;
        if stato == "tenuta" || stato == "verbalizzata" {
            bail!("L'ordine del giorno non è modificabile dopo lo svolgimento dell'assemblea");
        }
        self.conn.execute("DELETE FROM assemblea_punto WHERE id=?1", [id])?;
        Ok(())
    }

    // ------------------------------------------------------------------ convocazione
    pub fn testo_convocazione(&self, id: i64) -> Result<String> {
        let a = self.assemblea(id)?;
        let nome: String = self.conn.query_row("SELECT nome FROM condominio WHERE id=?1", [a.condominio_id], |r| r.get(0))?;
        let studio = self.leggi_imp("studio_nome");
        let mut t = format!("CONVOCAZIONE DI ASSEMBLEA {}\nCondominio «{}»\n\n", a.tipo.to_uppercase(), nome);
        t.push_str(&format!(
            "I signori condòmini sono convocati in assemblea {} presso {}:\n\n  • in prima convocazione il {} alle ore {}\n",
            match a.modalita.as_str() {
                "online" => "in modalità telematica",
                "mista" => "in modalità mista (presenza e collegamento da remoto)",
                _ => "in presenza",
            },
            if a.luogo.trim().is_empty() { "il luogo che sarà indicato" } else { a.luogo.trim() },
            data_it(&a.data_prima),
            if a.ora_prima.is_empty() { "—" } else { &a.ora_prima }
        ));
        if let Some(s) = &a.data_seconda {
            t.push_str(&format!("  • in seconda convocazione il {} alle ore {}\n", data_it(s), if a.ora_seconda.is_empty() { "—" } else { &a.ora_seconda }));
        }
        t.push_str("\nper discutere e deliberare sul seguente\n\nORDINE DEL GIORNO\n");
        for p in self.elenco_punti(id)? {
            t.push_str(&format!("{}. {}\n", p.numero, p.titolo));
            if !p.descrizione.trim().is_empty() {
                t.push_str(&format!("   {}\n", p.descrizione.trim().replace('\n', "\n   ")));
            }
        }
        t.push_str("\nIl condomino impossibilitato a partecipare può farsi rappresentare da un delegato con delega scritta (art. 67 disp. att. c.c.).\n");
        t.push_str(&format!("\n{}\nL'amministratore\n", if studio.trim().is_empty() { String::new() } else { studio }));
        Ok(t)
    }

    /// Passa da bozza a convocata e prepara la comunicazione in bacheca (da pubblicare). L'avviso va comunicato
    /// almeno 5 giorni prima della prima convocazione (art. 66 disp. att. c.c.).
    pub fn convoca_assemblea(&self, id: i64, oggi: NaiveDate) -> Result<i64> {
        let a = self.assemblea(id)?;
        if a.stato != "bozza" {
            bail!("L'assemblea è già {}", a.stato);
        }
        if self.elenco_punti(id)?.is_empty() {
            bail!("Inserisci almeno un punto all'ordine del giorno");
        }
        if a.data_seconda.is_none() {
            bail!("Indica la seconda convocazione (è sempre prevista dall'avviso)");
        }
        let prima = parse_data(&a.data_prima).unwrap();
        if prima < oggi + Duration::days(5) {
            bail!("L'avviso deve essere comunicato almeno 5 giorni prima della prima convocazione (art. 66 disp. att. c.c.)");
        }
        let testo = self.testo_convocazione(id)?;
        self.atomico(|db| {
            let com = db.salva_comunicazione(&crate::bacheca::Comunicazione {
                condominio_id: a.condominio_id,
                tipo: "assemblea".into(),
                titolo: format!("Convocazione assemblea {} del {}", a.tipo, data_it(&a.data_prima)),
                testo,
                data_evento: Some(a.data_prima.clone()),
                ..Default::default()
            })?;
            db.conn.execute("UPDATE assemblea SET stato='convocata',comunicazione_id=?2 WHERE id=?1", params![id, com])?;
            Ok(com)
        })
    }

    // ------------------------------------------------------------------ presenze e quorum
    fn titolare_principale(&self, unita: i64) -> Result<Option<(i64, String)>> {
        let t = self.elenco_titolari(unita)?;
        let mut cand: Vec<_> = t.iter().filter(|x| x.ruolo == "proprietario").collect();
        if cand.is_empty() {
            cand = t.iter().filter(|x| x.ruolo == "usufruttuario").collect();
        }
        cand.sort_by_key(|x| (-x.quota_ppm, x.id));
        Ok(cand.first().map(|x| (x.persona_id, x.persona.clone())))
    }

    fn controllo_tabella(&self, a: &Assemblea) -> Result<i64> {
        let Some(tab) = a.tabella_id else { bail!("Scegli la tabella millesimale di proprietà generale per i quorum") };
        Ok(tab)
    }

    pub fn imposta_presenza(&self, assemblea: i64, unita: i64, rappresentante: &str) -> Result<()> {
        let a = self.assemblea(assemblea)?;
        if a.stato != "convocata" && a.stato != "tenuta" {
            bail!("Le presenze si registrano per un'assemblea convocata o in corso");
        }
        let u: i64 = self.conn.query_row("SELECT condominio_id FROM unita WHERE id=?1", [unita], |r| r.get(0))?;
        if u != a.condominio_id {
            bail!("L'unità appartiene a un altro condominio");
        }
        self.conn.execute(
            "INSERT INTO assemblea_presenza(assemblea_id,unita_id,rappresentante) VALUES(?1,?2,?3)
             ON CONFLICT(assemblea_id,unita_id) DO UPDATE SET rappresentante=?3",
            params![assemblea, unita, rappresentante.trim()],
        )?;
        Ok(())
    }

    pub fn rimuovi_presenza(&self, assemblea: i64, unita: i64) -> Result<()> {
        let a = self.assemblea(assemblea)?;
        if a.stato != "convocata" && a.stato != "tenuta" {
            bail!("Assemblea non modificabile");
        }
        self.conn.execute(
            "DELETE FROM assemblea_voto WHERE unita_id=?2 AND punto_id IN (SELECT id FROM assemblea_punto WHERE assemblea_id=?1)",
            params![assemblea, unita],
        )?;
        self.conn.execute("DELETE FROM assemblea_presenza WHERE assemblea_id=?1 AND unita_id=?2", params![assemblea, unita])?;
        Ok(())
    }

    pub fn elenco_presenze(&self, assemblea: i64) -> Result<Vec<Presenza>> {
        let a = self.assemblea(assemblea)?;
        let mut st = self.conn.prepare("SELECT unita_id,rappresentante FROM assemblea_presenza WHERE assemblea_id=?1")?;
        let righe: Vec<(i64, String)> = st.query_map([assemblea], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
        drop(st);
        let unita: HashMap<i64, Unita> = self.elenco_unita(a.condominio_id)?.into_iter().map(|u| (u.id, u)).collect();
        let mut out = vec![];
        for (u, rapp) in righe {
            let micro = match a.tabella_id {
                Some(t) => self.millesimo(t, u).unwrap_or(0),
                None => 0,
            };
            out.push(Presenza {
                unita_id: u,
                unita: unita.get(&u).map(|x| x.etichetta()).unwrap_or_default(),
                titolare: self.titolare_principale(u)?.map(|x| x.1).unwrap_or_default(),
                rappresentante: rapp,
                millesimi_micro: micro,
            });
        }
        out.sort_by(|a, b| a.unita.cmp(&b.unita));
        Ok(out)
    }

    fn teste_e_millesimi(&self, a: &Assemblea, tab: i64) -> Result<(i64, usize, i64, usize)> {
        let totali_mill = self.somma_millesimi(tab)?;
        let mut totali: BTreeSet<i64> = BTreeSet::new();
        for u in self.elenco_unita(a.condominio_id)? {
            if self.millesimo(tab, u.id).unwrap_or(0) > 0 {
                if let Some((p, _)) = self.titolare_principale(u.id)? {
                    totali.insert(p);
                }
            }
        }
        let mut presenti: BTreeSet<i64> = BTreeSet::new();
        let mut pres_mill = 0;
        for p in self.elenco_presenze(a.id)? {
            pres_mill += p.millesimi_micro;
            if let Some((pers, _)) = self.titolare_principale(p.unita_id)? {
                presenti.insert(pers);
            }
        }
        Ok((totali_mill, totali.len(), pres_mill, presenti.len()))
    }

    /// Verifica la regolare costituzione (art. 1136 c. 1 e 3 c.c.): prima convocazione 2/3 del valore e maggioranza dei
    /// partecipanti; seconda convocazione 1/3 del valore e 1/3 dei partecipanti.
    pub fn quorum_costitutivo(&self, assemblea: i64, convocazione: i64) -> Result<Quorum> {
        let a = self.assemblea(assemblea)?;
        let tab = self.controllo_tabella(&a)?;
        let (tm, tt, pm, pt) = self.teste_e_millesimi(&a, tab)?;
        if tm == 0 || tt == 0 {
            bail!("La tabella millesimale non ha valori assegnati");
        }
        let (ok_v, ok_t, regola) = match convocazione {
            1 => (pm * 3 >= tm * 2, pt * 2 > tt, "1ª convocazione (art. 1136 c. 1): almeno 2/3 del valore dell'edificio e la maggioranza dei partecipanti"),
            2 => (pm * 3 >= tm, pt * 3 >= tt, "2ª convocazione (art. 1136 c. 3): almeno 1/3 del valore dell'edificio e 1/3 dei partecipanti"),
            _ => bail!("Convocazione non valida"),
        };
        Ok(Quorum {
            convocazione,
            millesimi_presenti: pm,
            millesimi_totali: tm,
            teste_presenti: pt,
            teste_totali: tt,
            costituita: ok_v && ok_t,
            spiegazione: format!(
                "{}. Presenti {} su {} e {} condòmini su {}.",
                regola,
                mill(pm),
                mill(tm),
                pt,
                tt
            ),
        })
    }

    /// Apre l'assemblea nella convocazione indicata, solo se regolarmente costituita.
    pub fn apri_assemblea(&self, assemblea: i64, convocazione: i64, presidente: &str, segretario: &str) -> Result<Quorum> {
        let a = self.assemblea(assemblea)?;
        if a.stato != "convocata" {
            bail!("Solo un'assemblea convocata può essere aperta (stato: {})", a.stato);
        }
        if convocazione == 2 && a.data_seconda.is_none() {
            bail!("Nessuna seconda convocazione prevista");
        }
        let q = self.quorum_costitutivo(assemblea, convocazione)?;
        if !q.costituita {
            bail!("Assemblea non regolarmente costituita. {}", q.spiegazione);
        }
        if presidente.trim().is_empty() || segretario.trim().is_empty() {
            bail!("Indica presidente e segretario dell'assemblea");
        }
        self.conn.execute(
            "UPDATE assemblea SET stato='tenuta',convocazione_tenuta=?2,presidente=?3,segretario=?4 WHERE id=?1",
            params![assemblea, convocazione, presidente.trim(), segretario.trim()],
        )?;
        Ok(q)
    }

    // ------------------------------------------------------------------ voti
    pub fn registra_voto(&self, punto: i64, unita: i64, scelta: &str) -> Result<()> {
        if !["favorevole", "contrario", "astenuto"].contains(&scelta) {
            bail!("Voto non valido");
        }
        let (ass, deliberato): (i64, Option<i64>) =
            self.conn.query_row("SELECT assemblea_id,delibera_id FROM assemblea_punto WHERE id=?1", [punto], |r| Ok((r.get(0)?, r.get(1)?)))?;
        if deliberato.is_some() {
            bail!("Su questo punto è già stata generata la delibera");
        }
        if self.assemblea(ass)?.stato != "tenuta" {
            bail!("Si vota solo durante l'assemblea (stato «tenuta»)");
        }
        let presente: i64 =
            self.conn.query_row("SELECT COUNT(*) FROM assemblea_presenza WHERE assemblea_id=?1 AND unita_id=?2", params![ass, unita], |r| r.get(0))?;
        if presente == 0 {
            bail!("L'unità non risulta presente");
        }
        self.conn.execute(
            "INSERT INTO assemblea_voto(punto_id,unita_id,scelta) VALUES(?1,?2,?3) ON CONFLICT(punto_id,unita_id) DO UPDATE SET scelta=?3",
            params![punto, unita, scelta],
        )?;
        Ok(())
    }

    /// Imposta lo stesso voto per tutte le unità presenti che non hanno ancora votato (comodo per le votazioni all'unanimità).
    pub fn voto_a_tutti_i_presenti(&self, punto: i64, scelta: &str) -> Result<usize> {
        let ass: i64 = self.conn.query_row("SELECT assemblea_id FROM assemblea_punto WHERE id=?1", [punto], |r| r.get(0))?;
        let mut n = 0;
        for p in self.elenco_presenze(ass)? {
            if self.voti_punto(punto)?.iter().all(|v| v.0 != p.unita_id) {
                self.registra_voto(punto, p.unita_id, scelta)?;
                n += 1;
            }
        }
        Ok(n)
    }

    pub fn rimuovi_voto(&self, punto: i64, unita: i64) -> Result<()> {
        let deliberato: Option<i64> = self.conn.query_row("SELECT delibera_id FROM assemblea_punto WHERE id=?1", [punto], |r| r.get(0))?;
        if deliberato.is_some() {
            bail!("Su questo punto è già stata generata la delibera");
        }
        self.conn.execute("DELETE FROM assemblea_voto WHERE punto_id=?1 AND unita_id=?2", params![punto, unita])?;
        Ok(())
    }

    /// (unità, scelta)
    pub fn voti_punto(&self, punto: i64) -> Result<Vec<(i64, String)>> {
        let mut st = self.conn.prepare("SELECT unita_id,scelta FROM assemblea_voto WHERE punto_id=?1")?;
        let v = st.query_map([punto], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
        Ok(v)
    }

    pub fn esito_punto(&self, punto: i64) -> Result<EsitoVoto> {
        let (ass_id, magg): (i64, String) =
            self.conn.query_row("SELECT assemblea_id,maggioranza FROM assemblea_punto WHERE id=?1", [punto], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let a = self.assemblea(ass_id)?;
        let tab = self.controllo_tabella(&a)?;
        let conv = a.convocazione_tenuta.unwrap_or(1);
        let (tm, tt, pm, pt) = self.teste_e_millesimi(&a, tab)?;
        let mut e = EsitoVoto { presenti_mill: pm, presenti_teste: pt, totali_mill: tm, totali_teste: tt, ..Default::default() };
        let mut teste: [BTreeSet<i64>; 3] = Default::default();
        for (u, scelta) in self.voti_punto(punto)? {
            let m = self.millesimo(tab, u).unwrap_or(0);
            let idx = match scelta.as_str() {
                "favorevole" => {
                    e.fav_mill += m;
                    0
                }
                "contrario" => {
                    e.con_mill += m;
                    1
                }
                _ => {
                    e.ast_mill += m;
                    2
                }
            };
            if let Some((p, _)) = self.titolare_principale(u)? {
                teste[idx].insert(p);
            }
        }
        e.fav_teste = teste[0].len();
        e.con_teste = teste[1].len();
        e.ast_teste = teste[2].len();
        let maggioranza_intervenuti = e.fav_teste * 2 > e.presenti_teste;
        let (ok_valore, regola) = match (magg.as_str(), conv) {
            ("ordinaria", 1) => (e.fav_mill * 2 >= tm, "art. 1136 c. 2: maggioranza degli intervenuti e almeno 500/1000 del valore dell'edificio"),
            ("ordinaria", _) => (e.fav_mill * 3 >= tm, "art. 1136 c. 3: maggioranza degli intervenuti e almeno 1/3 del valore dell'edificio"),
            ("qualificata", _) => (e.fav_mill * 2 >= tm, "art. 1136 c. 4: maggioranza degli intervenuti e almeno 500/1000 del valore, anche in seconda convocazione"),
            ("innovazioni", _) => (e.fav_mill * 3 >= tm * 2, "art. 1120 c. 1: maggioranza degli intervenuti e almeno 2/3 del valore dell'edificio"),
            _ => (e.fav_mill == tm, "unanimità"),
        };
        e.approvato = if magg == "unanimita" { ok_valore && e.fav_teste == tt } else { maggioranza_intervenuti && ok_valore };
        e.regola = regola.to_string();
        Ok(e)
    }

    /// Se il punto è approvato, registra la delibera (collegata alla tabella delibere e al verbale).
    pub fn approva_punto(&self, punto: i64) -> Result<i64> {
        let p = self.elenco_punti(self.conn.query_row("SELECT assemblea_id FROM assemblea_punto WHERE id=?1", [punto], |r| r.get(0))?)?
            .into_iter()
            .find(|x| x.id == punto)
            .ok_or_else(|| anyhow::anyhow!("Punto non trovato"))?;
        if p.delibera_id.is_some() {
            bail!("Delibera già generata per questo punto");
        }
        let a = self.assemblea(p.assemblea_id)?;
        if a.stato != "tenuta" {
            bail!("L'assemblea deve essere in corso");
        }
        let e = self.esito_punto(punto)?;
        if !e.approvato {
            bail!("Il punto non raggiunge le maggioranze richieste ({})", e.regola);
        }
        let data = if a.convocazione_tenuta == Some(2) { a.data_seconda.clone().unwrap_or(a.data_prima.clone()) } else { a.data_prima.clone() };
        let tipo = if ["preventivo", "rendiconto", "straordinaria", "integrativa"].contains(&p.tipo_delibera.as_str()) { p.tipo_delibera.clone() } else { "altro".into() };
        self.atomico(|db| {
            let id = db.salva_delibera(&Delibera {
                condominio_id: a.condominio_id,
                esercizio_id: a.esercizio_id,
                data: data_it(&data),
                tipo,
                oggetto: p.titolo.clone(),
                riferimento: format!("Verbale assemblea del {}, punto {}", data_it(&data), p.numero),
                maggioranza: e.regola.clone(),
                ..Default::default()
            })?;
            db.conn.execute("UPDATE assemblea_punto SET delibera_id=?2 WHERE id=?1", params![punto, id])?;
            Ok(id)
        })
    }

    // ------------------------------------------------------------------ verbale
    pub fn verbale_testo(&self, id: i64) -> Result<String> {
        let a = self.assemblea(id)?;
        if a.stato != "tenuta" && a.stato != "verbalizzata" {
            bail!("Il verbale si genera dopo lo svolgimento dell'assemblea");
        }
        let nome: String = self.conn.query_row("SELECT nome||' – '||indirizzo FROM condominio WHERE id=?1", [a.condominio_id], |r| r.get(0))?;
        let conv = a.convocazione_tenuta.unwrap_or(1);
        let q = self.quorum_costitutivo(id, conv)?;
        let data = if conv == 2 { a.data_seconda.clone().unwrap_or(a.data_prima.clone()) } else { a.data_prima.clone() };
        let mut t = format!("VERBALE DI ASSEMBLEA {}\nCondominio: {}\n\n", a.tipo.to_uppercase(), nome);
        t.push_str(&format!(
            "Il giorno {} alle ore {} presso {}, in {}ª convocazione, si è riunita l'assemblea del condominio.\n\n",
            data_it(&data),
            if conv == 2 { &a.ora_seconda } else { &a.ora_prima },
            if a.luogo.is_empty() { "—" } else { &a.luogo },
            conv
        ));
        t.push_str(&format!("Presidente: {}\nSegretario: {}\n\n", a.presidente, a.segretario));
        t.push_str("PRESENTI\n");
        for p in self.elenco_presenze(id)? {
            t.push_str(&format!(
                "  • {} – {}{} – {}\n",
                p.unita,
                p.titolare,
                if p.rappresentante.is_empty() { String::new() } else { format!(" (rappresentato da {} per delega)", p.rappresentante) },
                mill(p.millesimi_micro)
            ));
        }
        t.push_str(&format!(
            "\nSono presenti {} condòmini su {} che rappresentano {} su {}. {}L'assemblea è regolarmente costituita.\n\n",
            q.teste_presenti,
            q.teste_totali,
            mill(q.millesimi_presenti),
            mill(q.millesimi_totali),
            if conv == 2 { "La prima convocazione è andata deserta per mancanza del numero legale. " } else { "" }
        ));
        t.push_str("ORDINE DEL GIORNO E DELIBERAZIONI\n");
        for p in self.elenco_punti(id)? {
            let e = self.esito_punto(p.id)?;
            t.push_str(&format!("\n{}. {}\n", p.numero, p.titolo));
            if !p.descrizione.trim().is_empty() {
                t.push_str(&format!("   {}\n", p.descrizione.trim().replace('\n', "\n   ")));
            }
            t.push_str(&format!(
                "   Votazione: favorevoli {} ({} condòmini), contrari {} ({}), astenuti {} ({}).\n   Maggioranza richiesta: {}.\n   ESITO: {}\n",
                mill(e.fav_mill),
                e.fav_teste,
                mill(e.con_mill),
                e.con_teste,
                mill(e.ast_mill),
                e.ast_teste,
                e.regola,
                if e.approvato { "APPROVATO" } else { "NON APPROVATO" }
            ));
        }
        t.push_str(&format!("\nLetto, approvato e sottoscritto.\n\nIl Presidente: {}\nIl Segretario: {}\n", a.presidente, a.segretario));
        Ok(t)
    }

    /// Chiude l'assemblea: dopo la verbalizzazione presenze e voti non sono più modificabili.
    pub fn verbalizza_assemblea(&self, id: i64) -> Result<()> {
        let a = self.assemblea(id)?;
        if a.stato != "tenuta" {
            bail!("L'assemblea non è in corso");
        }
        self.conn.execute("UPDATE assemblea SET stato='verbalizzata' WHERE id=?1", [id])?;
        Ok(())
    }
}
