//! Importazione dell'anagrafica da CSV (unità, titolari e millesimi), per il passaggio da un altro gestionale.
//!
//! Intestazioni riconosciute (maiuscole/minuscole indifferenti, ordine libero):
//! `scala, piano, interno, tipo_unita, cognome, nome, codice_fiscale, email, telefono, ruolo, quota, paga_spese`
//! e una colonna `mill_<CODICE>` per ogni tabella millesimale (valore in millesimi, es. `mill_A` = 123,45).
//! Più righe con la stessa scala+interno sono i titolari della stessa unità.

use crate::anagrafica::{Persona, Tabella, Unita, RUOLI, TIPI_UNITA};
use crate::banca::campi;
use crate::db::Db;
use crate::util::*;
use anyhow::{bail, Result};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Debug, Default)]
pub struct EsitoImportAnagrafica {
    pub unita_create: usize,
    pub persone_create: usize,
    pub titolarita: usize,
    pub tabelle_create: usize,
    pub avvisi: Vec<String>,
}

#[derive(Default, Clone)]
struct Riga {
    n: usize,
    scala: String,
    piano: String,
    interno: String,
    tipo: String,
    cognome: String,
    nome: String,
    cf: String,
    email: String,
    telefono: String,
    ruolo: String,
    quota: Option<i64>,
    paga: bool,
    mill: Vec<(String, i64)>,
}

impl Db {
    pub fn importa_anagrafica_csv(&self, cond: i64, testo: &str) -> Result<EsitoImportAnagrafica> {
        let testo = testo.trim_start_matches('\u{feff}');
        let mut linee = testo.lines().filter(|l| !l.trim().is_empty());
        let Some(intest) = linee.next() else { bail!("Il file è vuoto") };
        let sep = [';', '\t', ','].into_iter().max_by_key(|c| intest.matches(*c).count()).unwrap();
        let head: Vec<String> = campi(intest, sep).iter().map(|s| s.trim().to_lowercase().replace(' ', "_")).collect();
        let col = |nome: &str| head.iter().position(|h| h == nome);
        if col("interno").is_none() || col("cognome").is_none() {
            bail!("Intestazione non riconosciuta: servono almeno le colonne «interno» e «cognome»");
        }
        let mut righe: Vec<Riga> = vec![];
        let mut esito = EsitoImportAnagrafica::default();
        for (i, l) in linee.enumerate() {
            let c = campi(l, sep);
            let g = |nome: &str| col(nome).and_then(|k| c.get(k)).map(|s| s.trim().to_string()).unwrap_or_default();
            let mut r = Riga { n: i + 2, scala: g("scala"), piano: g("piano"), interno: g("interno"), tipo: g("tipo_unita").to_lowercase(), cognome: g("cognome"), nome: g("nome"), cf: g("codice_fiscale").to_uppercase(), email: g("email"), telefono: g("telefono"), ruolo: g("ruolo").to_lowercase(), ..Default::default() };
            if r.interno.is_empty() || r.cognome.is_empty() {
                esito.avvisi.push(format!("riga {}: interno o cognome mancante, saltata", r.n));
                continue;
            }
            if r.ruolo.is_empty() {
                r.ruolo = "proprietario".into();
            }
            if !RUOLI.contains(&r.ruolo.as_str()) {
                esito.avvisi.push(format!("riga {}: ruolo «{}» non valido, saltata", r.n, r.ruolo));
                continue;
            }
            if !TIPI_UNITA.contains(&r.tipo.as_str()) {
                r.tipo = "appartamento".into();
            }
            let q = g("quota");
            if !q.is_empty() {
                match parse_percentuale_ppm(&q) {
                    Some(v) if v > 0 && v <= 1_000_000 => r.quota = Some(v),
                    _ => {
                        esito.avvisi.push(format!("riga {}: quota «{}» non valida, saltata", r.n, q));
                        continue;
                    }
                }
            }
            r.paga = matches!(g("paga_spese").to_lowercase().as_str(), "si" | "sì" | "s" | "1" | "true" | "x");
            for (k, h) in head.iter().enumerate() {
                if let Some(codice) = h.strip_prefix("mill_") {
                    let v = c.get(k).map(|s| s.trim()).unwrap_or("");
                    if v.is_empty() {
                        continue;
                    }
                    match parse_millesimi(v) {
                        Some(m) => r.mill.push((codice.to_uppercase(), m)),
                        None => esito.avvisi.push(format!("riga {}: millesimi «{}» non validi per {}", r.n, v, codice)),
                    }
                }
            }
            righe.push(r);
        }
        if righe.is_empty() {
            bail!("Nessuna riga valida da importare");
        }

        self.atomico(|db| {
            let mut unita_note: HashMap<(String, String), i64> = db.elenco_unita(cond)?.into_iter().map(|u| ((u.scala.to_lowercase(), u.interno.to_lowercase()), u.id)).collect();
            let persone = db.elenco_persone()?;
            let mut tabelle: HashMap<String, i64> = db.elenco_tabelle(cond)?.into_iter().map(|t| (t.codice.to_uppercase(), t.id)).collect();
            let mut per_unita: BTreeMap<(String, String), Vec<Riga>> = BTreeMap::new();
            for r in righe {
                per_unita.entry((r.scala.to_lowercase(), r.interno.to_lowercase())).or_default().push(r);
            }
            for (chiave, rr) in per_unita {
                let prima = &rr[0];
                let uid = match unita_note.get(&chiave) {
                    Some(id) => *id,
                    None => {
                        let id = db.salva_unita(&Unita { condominio_id: cond, scala: prima.scala.clone(), piano: prima.piano.clone(), interno: prima.interno.clone(), tipo: prima.tipo.clone(), ..Default::default() })?;
                        unita_note.insert(chiave.clone(), id);
                        esito.unita_create += 1;
                        id
                    }
                };
                // quote: mancanti = ripartizione uguale fra i proprietari senza quota
                let senza: Vec<usize> = rr.iter().enumerate().filter(|(_, r)| r.quota.is_none() && r.ruolo == "proprietario").map(|(i, _)| i).collect();
                let usate: i64 = rr.iter().filter(|r| r.ruolo == "proprietario").filter_map(|r| r.quota).sum();
                let resto = (1_000_000 - usate).max(0);
                let parti = if senza.is_empty() { vec![] } else { ripartisci(resto, &vec![1; senza.len()]) };
                for (i, r) in rr.iter().enumerate() {
                    let pid = match persone.iter().find(|p| (!r.cf.is_empty() && p.codice_fiscale.eq_ignore_ascii_case(&r.cf)) || (r.cf.is_empty() && p.cognome.eq_ignore_ascii_case(&r.cognome) && p.nome.eq_ignore_ascii_case(&r.nome))) {
                        Some(p) => p.id,
                        None => {
                            // potrebbe essere stata creata da una riga precedente di questo stesso file
                            let gia = db.elenco_persone()?.into_iter().find(|p| (!r.cf.is_empty() && p.codice_fiscale.eq_ignore_ascii_case(&r.cf)) || (r.cf.is_empty() && p.cognome.eq_ignore_ascii_case(&r.cognome) && p.nome.eq_ignore_ascii_case(&r.nome)));
                            match gia {
                                Some(p) => p.id,
                                None => {
                                    esito.persone_create += 1;
                                    db.salva_persona(&Persona { tipo: "fisica".into(), nome: r.nome.clone(), cognome: r.cognome.clone(), codice_fiscale: r.cf.clone(), email: r.email.clone(), telefono: r.telefono.clone(), ..Default::default() })?
                                }
                            }
                        }
                    };
                    let quota = match r.quota {
                        Some(q) => q,
                        None if r.ruolo == "proprietario" => senza.iter().position(|x| *x == i).map(|k| parti[k]).unwrap_or(1_000_000).max(1),
                        None => 1_000_000,
                    };
                    let esiste = db.elenco_titolari(uid)?.iter().any(|t| t.persona_id == pid && t.ruolo == r.ruolo);
                    if esiste {
                        esito.avvisi.push(format!("riga {}: titolarità già presente, saltata", r.n));
                    } else {
                        db.aggiungi_titolarita(uid, pid, &r.ruolo, quota, r.paga)?;
                        esito.titolarita += 1;
                    }
                    for (codice, m) in &r.mill {
                        let tid = match tabelle.get(codice) {
                            Some(t) => *t,
                            None => {
                                let t = db.salva_tabella(&Tabella { condominio_id: cond, codice: codice.clone(), nome: format!("Tabella {}", codice), totale_micro: 1_000_000, ..Default::default() })?;
                                tabelle.insert(codice.clone(), t);
                                esito.tabelle_create += 1;
                                t
                            }
                        };
                        db.imposta_millesimo(tid, uid, *m)?;
                    }
                }
            }
            Ok(())
        })?;
        Ok(esito)
    }
}
