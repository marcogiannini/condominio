//! Import delle fatture elettroniche passive (FatturaPA, XML SDI) con registrazione automatica.
//!
//! Legge fornitore, numero, data, imponibile, IVA, ritenuta e IBAN; crea il fornitore se manca (riconosciuto da P.IVA o
//! codice fiscale) e registra la fattura. Le note di credito non vengono importate (vanno gestite a mano).
//! I file firmati `.p7m` sono supportati estraendo l'XML incorporato.

use crate::db::Db;
use crate::fornitori::{Fornitore, NuovaFattura};
use crate::util::*;
use anyhow::{bail, Result};
use quick_xml::events::Event;
use quick_xml::Reader;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FatturaXml {
    pub fornitore: String,
    pub piva: String,
    pub codice_fiscale: String,
    pub iban: String,
    pub tipo_documento: String,
    pub numero: String,
    pub data: String, // ISO
    pub descrizione: String,
    pub imponibile: Cents,
    pub iva: Cents,
    pub totale: Cents,
    pub ritenuta: Cents,
    pub aliquota_ritenuta_ppm: i64, // 20% = 200_000 (ppm = percentuale x 10.000)
    pub scadenza: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct EsitoXml {
    pub importate: Vec<String>,
    pub saltate: Vec<String>,
    pub avvisi: Vec<String>,
    pub nuovi_fornitori: usize,
}

/// Se il file è firmato (CAdES .p7m) recupera l'XML incorporato; altrimenti lo restituisce com'è.
pub fn estrai_xml(bytes: &[u8]) -> Result<String> {
    let testo = String::from_utf8_lossy(bytes).to_string();
    let t = testo.trim_start_matches('\u{feff}').trim_start();
    if t.starts_with('<') {
        return Ok(t.to_string());
    }
    let inizio = testo.find("<?xml").or_else(|| testo.find("<p:FatturaElettronica")).or_else(|| testo.find("<FatturaElettronica"));
    let Some(i) = inizio else { bail!("Il file non contiene una fattura elettronica XML riconoscibile") };
    let Some(f) = testo.rfind("FatturaElettronica>") else { bail!("Fine del documento XML non trovata") };
    Ok(testo[i..f + "FatturaElettronica>".len()].to_string())
}

/// Importo decimale XML ("1234.56") in centesimi.
pub fn importo_xml(s: &str) -> Option<Cents> {
    let s = s.trim().replace(',', ".");
    let (neg, s) = match s.strip_prefix('-') {
        Some(r) => (true, r.to_string()),
        None => (false, s),
    };
    let (int, dec) = s.split_once('.').unwrap_or((&s, ""));
    let int: i64 = if int.is_empty() { 0 } else { int.parse().ok()? };
    let mut d = dec.chars().take(2).collect::<String>();
    while d.len() < 2 {
        d.push('0');
    }
    let dec: i64 = d.parse().ok()?;
    let v = int.checked_mul(100)?.checked_add(dec)?;
    Some(if neg { -v } else { v })
}

struct Stato {
    cedente: FatturaXml,
    corrente: Option<FatturaXml>,
    nome: String,
    cognome: String,
    prima_desc: bool,
}

/// Assegna il testo di un elemento (percorso `p`) al campo corrispondente.
fn applica(st: &mut Stato, p: &str, txt: String) {
    let ends = |s: &str| p.ends_with(s);
    if p.contains("CedentePrestatore") && st.corrente.is_none() {
        if ends("IdFiscaleIVA/IdCodice") {
            st.cedente.piva = txt;
        } else if ends("DatiAnagrafici/CodiceFiscale") {
            st.cedente.codice_fiscale = txt;
        } else if ends("Anagrafica/Denominazione") {
            st.cedente.fornitore = txt;
        } else if ends("Anagrafica/Nome") {
            st.nome = txt;
        } else if ends("Anagrafica/Cognome") {
            st.cognome = txt;
        }
    } else if let Some(f) = st.corrente.as_mut() {
        if ends("DatiGeneraliDocumento/TipoDocumento") {
            f.tipo_documento = txt;
        } else if ends("DatiGeneraliDocumento/Data") {
            f.data = txt;
        } else if ends("DatiGeneraliDocumento/Numero") {
            f.numero = txt;
        } else if ends("DatiGeneraliDocumento/ImportoTotaleDocumento") {
            f.totale = importo_xml(&txt).unwrap_or(0);
        } else if ends("DatiGeneraliDocumento/Causale") && f.descrizione.is_empty() {
            f.descrizione = txt;
        } else if ends("DatiRitenuta/ImportoRitenuta") {
            f.ritenuta += importo_xml(&txt).unwrap_or(0);
        } else if ends("DatiRitenuta/AliquotaRitenuta") {
            f.aliquota_ritenuta_ppm = (txt.replace(',', ".").parse::<f64>().unwrap_or(0.0) * 10_000.0).round() as i64;
        } else if ends("DettaglioLinee/Descrizione") && st.prima_desc {
            if f.descrizione.is_empty() {
                f.descrizione = txt;
            }
            st.prima_desc = false;
        } else if ends("DatiRiepilogo/ImponibileImporto") {
            f.imponibile += importo_xml(&txt).unwrap_or(0);
        } else if ends("DatiRiepilogo/Imposta") {
            f.iva += importo_xml(&txt).unwrap_or(0);
        } else if ends("DettaglioPagamento/IBAN") && f.iban.is_empty() {
            f.iban = txt;
        } else if ends("DettaglioPagamento/DataScadenzaPagamento") && f.scadenza.is_none() {
            f.scadenza = Some(txt);
        }
    }
}

/// Risolve un riferimento a entità (`&amp;`, `&#232;`...) nel carattere corrispondente.
fn entita(nome: &str) -> String {
    match nome {
        "amp" => "&".into(),
        "lt" => "<".into(),
        "gt" => ">".into(),
        "quot" => "\"".into(),
        "apos" => "'".into(),
        n if n.starts_with("#x") => u32::from_str_radix(&n[2..], 16).ok().and_then(char::from_u32).map(|c| c.to_string()).unwrap_or_default(),
        n if n.starts_with('#') => n[1..].parse::<u32>().ok().and_then(char::from_u32).map(|c| c.to_string()).unwrap_or_default(),
        _ => String::new(),
    }
}

/// Legge tutte le fatture (corpi) contenute nel file.
pub fn leggi_fatture_xml(xml: &str) -> Result<Vec<FatturaXml>> {
    let mut rd = Reader::from_str(xml);
    let mut path: Vec<String> = vec![];
    let mut corpi: Vec<FatturaXml> = vec![];
    let mut st = Stato { cedente: FatturaXml::default(), corrente: None, nome: String::new(), cognome: String::new(), prima_desc: true };
    let mut buf = String::new(); // il testo di un elemento può arrivare in più eventi (es. attorno a "&amp;")
    let mut vide_fattura = false;
    loop {
        match rd.read_event() {
            Err(e) => bail!("XML non valido: {}", e),
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => {
                let n = e.local_name().as_ref().to_string();
                buf.clear();
                if n == "FatturaElettronica" {
                    vide_fattura = true;
                }
                if n == "FatturaElettronicaBody" {
                    let mut f = st.cedente.clone();
                    f.imponibile = 0;
                    f.iva = 0;
                    st.corrente = Some(f);
                    st.prima_desc = true;
                }
                path.push(n);
            }
            Ok(Event::End(e)) => {
                let n = e.local_name().as_ref().to_string();
                let txt = buf.trim().to_string();
                buf.clear();
                if !txt.is_empty() {
                    applica(&mut st, &path.join("/"), txt);
                }
                if n == "FatturaElettronicaBody" {
                    if let Some(f) = st.corrente.take() {
                        corpi.push(f);
                    }
                }
                if n == "CedentePrestatore" && st.cedente.fornitore.is_empty() {
                    st.cedente.fornitore = format!("{} {}", st.cognome, st.nome).trim().to_string();
                }
                path.pop();
            }
            Ok(Event::Text(t)) => buf.push_str(&t.xml10_content()),
            Ok(Event::GeneralRef(r)) => buf.push_str(&entita(&r)),
            Ok(Event::CData(c)) => buf.push_str(&c),
            _ => {}
        }
    }
    if !vide_fattura {
        bail!("Non è una fattura elettronica (manca l'elemento FatturaElettronica)");
    }
    if corpi.is_empty() {
        bail!("Nessuna fattura trovata nel file");
    }
    Ok(corpi)
}

impl Db {
    fn fornitore_per_xml(&self, cond: i64, f: &FatturaXml) -> Result<(i64, bool)> {
        let trovato: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM fornitore WHERE condominio_id=?1
                 AND ((?2<>'' AND replace(upper(cf_piva),'IT','')=?2) OR (?3<>'' AND upper(cf_piva)=?3))",
                rusqlite::params![cond, f.piva.to_uppercase().replace("IT", ""), f.codice_fiscale.to_uppercase()],
                |r| r.get(0),
            )
            .ok();
        if let Some(id) = trovato {
            return Ok((id, false));
        }
        let regime = if f.ritenuta > 0 {
            match f.aliquota_ritenuta_ppm {
                x if x >= 150_000 => "professionista20",
                _ => "appalti4",
            }
        } else {
            "nessuna"
        };
        let id = self.salva_fornitore(&Fornitore {
            condominio_id: cond,
            ragione_sociale: if f.fornitore.is_empty() { format!("Fornitore {}", f.piva) } else { f.fornitore.clone() },
            cf_piva: if f.piva.is_empty() { f.codice_fiscale.clone() } else { f.piva.clone() },
            iban: f.iban.clone(),
            regime_ritenuta: regime.into(),
            ..Default::default()
        })?;
        Ok((id, true))
    }

    /// Importa un file XML (anche più fatture). Un errore su una fattura non blocca le altre.
    pub fn importa_fatture_xml(&self, cond: i64, esercizio: i64, bytes: &[u8], voce: Option<i64>, etichetta: &str) -> Result<EsitoXml> {
        let xml = estrai_xml(bytes)?;
        let fatture = leggi_fatture_xml(&xml)?;
        let mut esito = EsitoXml::default();
        for f in fatture {
            let nome = format!("{} n. {} ({})", if f.fornitore.is_empty() { etichetta } else { &f.fornitore }, f.numero, data_it(&f.data));
            if f.tipo_documento == "TD04" || f.tipo_documento == "TD05" {
                esito.saltate.push(format!("{}: nota di credito/debito, registrala a mano", nome));
                continue;
            }
            if f.numero.is_empty() || f.imponibile <= 0 || parse_data(&f.data).is_none() {
                esito.saltate.push(format!("{}: dati incompleti (numero, data o imponibile)", nome));
                continue;
            }
            let r = self.atomico(|db| {
                let (forn, nuovo) = db.fornitore_per_xml(cond, &f)?;
                let id = db.registra_fattura(&NuovaFattura {
                    condominio_id: cond,
                    esercizio_id: esercizio,
                    fornitore_id: forn,
                    voce_id: voce,
                    numero: f.numero.clone(),
                    data: f.data.clone(),
                    descrizione: f.descrizione.clone(),
                    imponibile: f.imponibile,
                    iva: f.iva,
                })?;
                Ok((id, nuovo))
            });
            match r {
                Ok((id, nuovo)) => {
                    if nuovo {
                        esito.nuovi_fornitori += 1;
                    }
                    let calcolata: Cents = db_ritenuta(self, id)?;
                    if calcolata != f.ritenuta {
                        esito.avvisi.push(format!(
                            "{}: ritenuta nel file {} diversa da quella calcolata {} (controlla il regime del fornitore)",
                            nome,
                            format_eur(f.ritenuta),
                            format_eur(calcolata)
                        ));
                    }
                    if f.totale > 0 && f.totale != f.imponibile + f.iva {
                        esito.avvisi.push(format!("{}: totale documento {} diverso da imponibile + IVA {}", nome, format_eur(f.totale), format_eur(f.imponibile + f.iva)));
                    }
                    esito.importate.push(nome);
                }
                Err(e) => esito.saltate.push(format!("{}: {}", nome, e)),
            }
        }
        Ok(esito)
    }
}

fn db_ritenuta(db: &Db, fattura: i64) -> Result<Cents> {
    Ok(db.conn.query_row("SELECT ritenuta_cents FROM fattura WHERE id=?1", [fattura], |r| r.get(0))?)
}
