//! Riconciliazione bancaria: import dell'estratto conto (CSV) e abbinamento a incassi, pagamenti fornitori e F24.

use crate::db::Db;
use crate::fornitori::GruppoRitenute;
use crate::incassi::NuovoIncasso;
use crate::util::*;
use anyhow::{bail, Result};
use chrono::NaiveDate;
use rusqlite::params;

#[derive(Clone, Debug, PartialEq)]
pub struct RigaCsv {
    pub data: String, // ISO
    pub descrizione: String,
    pub importo: Cents,
}

#[derive(Clone, Debug)]
pub struct MovBanca {
    pub id: i64,
    pub conto_corrente_id: i64,
    pub data: String,
    pub descrizione: String,
    pub importo: Cents,
    pub stato: String,
    pub incasso_id: Option<i64>,
    pub fattura_id: Option<i64>,
    pub versamento_id: Option<i64>,
}

#[derive(Clone, Debug, Default)]
pub struct EsitoImport {
    pub nuovi: usize,
    pub duplicati: usize,
    pub scartate: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Proposta {
    pub tipo: &'static str, // "incasso" | "fattura" | "f24"
    pub riferimento_id: i64,
    pub codice_tributo: Option<String>,
    pub etichetta: String,
    pub punteggio: i64,
}

fn data_banca(s: &str) -> Option<NaiveDate> {
    let t = s.trim().trim_matches('"');
    for f in ["%d/%m/%Y", "%Y-%m-%d", "%d-%m-%Y", "%d/%m/%y", "%d.%m.%Y", "%Y/%m/%d"] {
        if let Ok(d) = NaiveDate::parse_from_str(t, f) {
            return Some(d);
        }
    }
    None
}

pub(crate) fn campi(riga: &str, sep: char) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut tra_apici = false;
    let mut it = riga.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '"' if tra_apici && it.peek() == Some(&'"') => {
                cur.push('"');
                it.next();
            }
            '"' => tra_apici = !tra_apici,
            c if c == sep && !tra_apici => out.push(std::mem::take(&mut cur)),
            c => cur.push(c),
        }
    }
    out.push(cur);
    out.into_iter().map(|s| s.trim().to_string()).collect()
}

fn importo_banca(s: &str) -> Option<Cents> {
    let t = s.trim().replace(['+', '€'], "");
    let t = t.trim();
    if t.is_empty() {
        return None;
    }
    // "(12,34)" = negativo; "12,34-" = negativo
    let (neg, t) = if t.starts_with('(') && t.ends_with(')') {
        (true, &t[1..t.len() - 1])
    } else if let Some(x) = t.strip_suffix('-') {
        (true, x)
    } else {
        (false, t)
    };
    let v = parse_eur(t)?;
    Some(if neg { -v.abs() } else { v })
}

/// Legge un estratto conto CSV. Riconosce il separatore, salta il preambolo e individua le colonne
/// per nome (data / descrizione / importo oppure entrate-uscite / dare-avere); senza intestazione usa data;descrizione;importo.
pub fn parse_estratto_csv(testo: &str) -> Result<(Vec<RigaCsv>, Vec<String>)> {
    let testo = testo.trim_start_matches('\u{feff}');
    let righe: Vec<&str> = testo.lines().filter(|l| !l.trim().is_empty()).collect();
    if righe.is_empty() {
        bail!("Il file è vuoto");
    }
    let prima = righe.iter().find(|l| l.matches(';').count() + l.matches('\t').count() + l.matches(',').count() > 0).copied().unwrap_or(righe[0]);
    let sep = [(';', prima.matches(';').count()), ('\t', prima.matches('\t').count()), (',', prima.matches(',').count())]
        .into_iter()
        .max_by_key(|x| x.1)
        .map(|x| x.0)
        .unwrap_or(';');

    let norm = |s: &str| s.to_lowercase();
    let mut idx_header = None;
    let (mut c_data, mut c_desc, mut c_imp, mut c_in, mut c_out) = (0usize, 1usize, 2usize, None, None);
    for (i, r) in righe.iter().enumerate().take(40) {
        let cc = campi(r, sep);
        let find = |chiavi: &[&str]| cc.iter().position(|c| chiavi.iter().any(|k| norm(c).contains(k)));
        let d = find(&["data"]);
        let imp = find(&["importo", "amount"]);
        let ent = find(&["entrate", "accredit", "avere", "credit"]);
        let usc = find(&["uscite", "addebit", "dare", "debit"]);
        if d.is_some() && (imp.is_some() || (ent.is_some() && usc.is_some())) {
            c_data = cc.iter().position(|c| norm(c).contains("data") && !norm(c).contains("valuta")).or(d).unwrap();
            // priorità alle intestazioni più specifiche; mai una colonna di data
            c_desc = ["descrizione", "causale", "dettagli", "description", "operazione", "movimento"]
                .iter()
                .find_map(|k| cc.iter().position(|c| norm(c).contains(k) && !norm(c).contains("data")))
                .filter(|x| *x != c_data)
                .unwrap_or(if c_data == 0 { 1 } else { 0 });
            if let Some(i2) = imp {
                c_imp = i2;
            } else {
                c_in = ent;
                c_out = usc;
            }
            idx_header = Some(i);
            break;
        }
    }
    let inizio = idx_header.map(|i| i + 1).unwrap_or(0);
    let mut out = vec![];
    let mut scartate = vec![];
    for (n, r) in righe.iter().enumerate().skip(inizio) {
        let cc = campi(r, sep);
        let get = |i: usize| cc.get(i).map(|s| s.as_str()).unwrap_or("");
        let Some(d) = data_banca(get(c_data)) else {
            scartate.push(format!("riga {}: data non riconosciuta", n + 1));
            continue;
        };
        let importo = match (c_in, c_out) {
            (Some(i), Some(o)) => {
                let a = importo_banca(get(i)).unwrap_or(0).abs();
                let b = importo_banca(get(o)).unwrap_or(0).abs();
                if a == 0 && b == 0 { None } else { Some(a - b) }
            }
            _ => importo_banca(get(c_imp)),
        };
        let Some(importo) = importo else {
            scartate.push(format!("riga {}: importo non riconosciuto", n + 1));
            continue;
        };
        if importo == 0 {
            continue;
        }
        out.push(RigaCsv { data: d.format("%Y-%m-%d").to_string(), descrizione: get(c_desc).to_string(), importo });
    }
    if out.is_empty() && scartate.is_empty() {
        bail!("Nessun movimento trovato nel file");
    }
    Ok((out, scartate))
}

fn minuscolo(s: &str) -> String {
    s.to_lowercase().chars().map(|c| match c {
        'à' | 'á' => 'a',
        'è' | 'é' => 'e',
        'ì' | 'í' => 'i',
        'ò' | 'ó' => 'o',
        'ù' | 'ú' => 'u',
        c => c,
    }).collect()
}

impl Db {
    pub fn importa_estratto(&self, cond: i64, conto_corrente: i64, testo: &str) -> Result<EsitoImport> {
        let (righe, scartate) = parse_estratto_csv(testo)?;
        let mut esito = EsitoImport { scartate, ..Default::default() };
        self.atomico(|db| {
            let mut visti: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
            for r in righe {
                // due movimenti identici nello stesso file sono distinti dal numero d'ordine
                let base = format!("{}|{}|{}", r.data, r.importo, r.descrizione.trim().to_lowercase());
                let n = visti.entry(base.clone()).or_insert(0);
                *n += 1;
                let impronta = format!("{}|{}", base, n);
                let k = db.conn.execute(
                    "INSERT OR IGNORE INTO movimento_banca(condominio_id,conto_corrente_id,data,descrizione,importo_cents,impronta)
                     VALUES(?1,?2,?3,?4,?5,?6)",
                    params![cond, conto_corrente, r.data, r.descrizione, r.importo, impronta],
                )?;
                if k == 1 {
                    esito.nuovi += 1;
                } else {
                    esito.duplicati += 1;
                }
            }
            Ok(())
        })?;
        Ok(esito)
    }

    pub fn elenco_movimenti_banca(&self, cond: i64, stato: Option<&str>) -> Result<Vec<MovBanca>> {
        let mut st = self.conn.prepare(
            "SELECT id,conto_corrente_id,data,descrizione,importo_cents,stato,incasso_id,fattura_id,versamento_id
             FROM movimento_banca WHERE condominio_id=?1 AND (?2 IS NULL OR stato=?2) ORDER BY data DESC,id DESC LIMIT 1000",
        )?;
        let v = st
            .query_map(params![cond, stato], |r| {
                Ok(MovBanca {
                    id: r.get(0)?,
                    conto_corrente_id: r.get(1)?,
                    data: r.get(2)?,
                    descrizione: r.get(3)?,
                    importo: r.get(4)?,
                    stato: r.get(5)?,
                    incasso_id: r.get(6)?,
                    fattura_id: r.get(7)?,
                    versamento_id: r.get(8)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(v)
    }

    fn movimento_banca(&self, id: i64) -> Result<(i64, MovBanca)> {
        let cond: i64 = self.conn.query_row("SELECT condominio_id FROM movimento_banca WHERE id=?1", [id], |r| r.get(0))?;
        let m = self.elenco_movimenti_banca(cond, None)?.into_iter().find(|m| m.id == id).ok_or_else(|| anyhow::anyhow!("Movimento non trovato"))?;
        Ok((cond, m))
    }

    /// Abbinamenti suggeriti, dal più al meno probabile.
    pub fn suggerisci_abbinamenti(&self, mov: i64) -> Result<Vec<Proposta>> {
        let (cond, m) = self.movimento_banca(mov)?;
        let desc = minuscolo(&m.descrizione);
        let mut out: Vec<Proposta> = vec![];
        if m.importo > 0 {
            for p in self.persone_del_condominio(cond)? {
                let cogn = minuscolo(p.cognome.trim());
                if cogn.chars().count() < 3 || !desc.contains(&cogn) {
                    continue;
                }
                let mut punti = 3;
                let nome = minuscolo(p.nome.trim());
                if nome.chars().count() >= 3 && desc.contains(&nome) {
                    punti += 2;
                }
                let residuo: Cents = self.rate_aperte(cond, p.id)?.iter().map(|r| r.residuo()).sum();
                if residuo == m.importo {
                    punti += 4;
                }
                out.push(Proposta {
                    tipo: "incasso",
                    riferimento_id: p.id,
                    codice_tributo: None,
                    etichetta: format!("Incasso da {} (residuo rate {})", p.nome_completo(), format_eur(residuo)),
                    punteggio: punti,
                });
            }
        } else {
            let importo = -m.importo;
            for f in self.elenco_fatture(cond, None)?.into_iter().filter(|f| f.stato == "da_pagare") {
                let mut punti = 0;
                if f.netto_a_pagare() == importo {
                    punti += 5;
                }
                let forn = minuscolo(&f.fornitore);
                if forn.split_whitespace().any(|w| w.chars().count() >= 4 && desc.contains(w)) {
                    punti += 3;
                }
                if desc.contains(&minuscolo(&f.numero)) && f.numero.chars().count() >= 2 {
                    punti += 1;
                }
                if punti >= 3 {
                    out.push(Proposta {
                        tipo: "fattura",
                        riferimento_id: f.id,
                        codice_tributo: None,
                        etichetta: format!("Pagamento fattura {} di {} (netto {})", f.numero, f.fornitore, format_eur(f.netto_a_pagare())),
                        punteggio: punti,
                    });
                }
            }
            for g in self.ritenute_da_versare(cond)? {
                if g.importo == importo {
                    out.push(Proposta {
                        tipo: "f24",
                        riferimento_id: 0,
                        codice_tributo: Some(g.codice_tributo.clone()),
                        etichetta: format!("Versamento F24 ritenute, codice {} ({})", g.codice_tributo, format_eur(g.importo)),
                        punteggio: 5 + if desc.contains("f24") || desc.contains("erario") || desc.contains("agenzia") { 2 } else { 0 },
                    });
                }
            }
        }
        out.sort_by(|a, b| b.punteggio.cmp(&a.punteggio));
        Ok(out)
    }

    fn verifica_da_abbinare(&self, mov: i64) -> Result<(i64, MovBanca)> {
        let (cond, m) = self.movimento_banca(mov)?;
        if m.stato != "da_abbinare" {
            bail!("Il movimento è già {}", m.stato.replace('_', " "));
        }
        Ok((cond, m))
    }

    /// Registra come incasso del condòmino (allocazione dalla rata più vecchia, modificabile poi con storno e reinserimento).
    pub fn riconcilia_incasso(&self, mov: i64, persona: i64) -> Result<i64> {
        let (cond, m) = self.verifica_da_abbinare(mov)?;
        if m.importo <= 0 {
            bail!("Un'uscita non può essere un incasso");
        }
        self.atomico(|db| {
            let inc = db.registra_incasso(&NuovoIncasso {
                condominio_id: cond,
                persona_id: persona,
                conto_corrente_id: Some(m.conto_corrente_id),
                data: m.data.clone(),
                importo: m.importo,
                usa_salvadanaio: 0,
                distribuzione: None,
                metodo: "bonifico".into(),
                note: format!("Da estratto conto: {}", m.descrizione),
            })?;
            db.conn.execute("UPDATE movimento_banca SET stato='riconciliato',incasso_id=?2 WHERE id=?1", params![mov, inc])?;
            Ok(inc)
        })
    }

    pub fn riconcilia_fattura(&self, mov: i64, fattura: i64) -> Result<()> {
        let (cond, m) = self.verifica_da_abbinare(mov)?;
        if m.importo >= 0 {
            bail!("Un'entrata non può pagare una fattura");
        }
        let f = self.elenco_fatture(cond, None)?.into_iter().find(|f| f.id == fattura).ok_or_else(|| anyhow::anyhow!("Fattura non trovata"))?;
        if f.netto_a_pagare() != -m.importo {
            bail!("L'importo del movimento ({}) non coincide con il netto da pagare ({})", format_eur(-m.importo), format_eur(f.netto_a_pagare()));
        }
        self.atomico(|db| {
            db.paga_fattura(fattura, &m.data, m.conto_corrente_id)?;
            db.conn.execute("UPDATE movimento_banca SET stato='riconciliato',fattura_id=?2 WHERE id=?1", params![mov, fattura])?;
            Ok(())
        })
    }

    pub fn riconcilia_f24(&self, mov: i64, codice_tributo: &str) -> Result<()> {
        let (cond, m) = self.verifica_da_abbinare(mov)?;
        let g: GruppoRitenute = self
            .ritenute_da_versare(cond)?
            .into_iter()
            .find(|g| g.codice_tributo == codice_tributo && g.importo == -m.importo)
            .ok_or_else(|| anyhow::anyhow!("Nessun gruppo di ritenute con codice {} e importo {}", codice_tributo, format_eur(-m.importo)))?;
        self.atomico(|db| {
            let v = db.versa_ritenute(cond, &g, &m.data, m.conto_corrente_id)?;
            db.conn.execute("UPDATE movimento_banca SET stato='riconciliato',versamento_id=?2 WHERE id=?1", params![mov, v])?;
            Ok(())
        })
    }

    pub fn ignora_movimento(&self, mov: i64) -> Result<()> {
        self.verifica_da_abbinare(mov)?;
        self.conn.execute("UPDATE movimento_banca SET stato='ignorato' WHERE id=?1", [mov])?;
        Ok(())
    }

    /// Annulla riconciliazione/ignora: per incassi e fatture storna l'operazione collegata.
    pub fn annulla_riconciliazione(&self, mov: i64, data: &str) -> Result<()> {
        let (_, m) = self.movimento_banca(mov)?;
        if m.stato == "da_abbinare" {
            return Ok(());
        }
        self.atomico(|db| {
            if let Some(v) = m.versamento_id {
                db.storna_versamento_ritenute(v, data)?;
            }
            if let Some(i) = m.incasso_id {
                db.storna_incasso(i, data)?;
            }
            if let Some(f) = m.fattura_id {
                db.storna_pagamento_fattura(f, data)?;
            }
            db.conn.execute("UPDATE movimento_banca SET stato='da_abbinare',incasso_id=NULL,fattura_id=NULL WHERE id=?1", [mov])?;
            Ok(())
        })
    }
}
