//! Rate: matrice condòmino × rata (come la schermata «Rate» di Domustudio), con totali dovuto, versato, saldo e scaduto.

use super::elenco::*;
use super::*;
use crate::preventivo::RataRiga;
use std::collections::{BTreeMap, HashMap};

#[derive(Default)]
pub struct Stato {
    sel: Option<usize>,
    filtro: String,
    cache: Cache<Vec<(String, Vec<RataRiga>)>>,
}

fn data_corta(iso: &str) -> String {
    let d = data_it(iso);
    if d.len() >= 10 {
        format!("{}/{}/{}", &d[0..2], &d[3..5], &d[8..10])
    } else {
        d
    }
}

pub fn mostra(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    let Some(es) = c.esercizio else {
        ui.add_space(20.0);
        ui.label("Crea o seleziona un esercizio (pulsante Condomìni → Nuovo esercizio).");
        return;
    };
    let (db, rev) = (c.db, *c.rev);
    let piani: Vec<(String, Vec<RataRiga>)> = st
        .cache
        .get(rev, &es.to_string(), || {
            db.elenco_piani(es)
                .unwrap_or_default()
                .into_iter()
                .filter(|p| p.stato == "attivo")
                .map(|p| {
                    let r = db.rate_piano(p.id).unwrap_or_default();
                    (p.nome, r)
                })
                .collect()
        })
        .clone();
    let oggi = oggi_iso();

    let nomi_persone: HashMap<i64, String> = c.db.elenco_persone().unwrap_or_default().iter().map(|p| (p.id, p.nome_completo())).collect();
    let unita: HashMap<i64, String> = c.db.elenco_unita(c.cond).unwrap_or_default().iter().map(|u| (u.id, format!("{}{}", u.scala, u.interno))).collect();

    // colonne: una per ogni (piano, numero), in ordine di scadenza
    let mut chiavi: Vec<(String, usize, i64, String)> = vec![]; // scadenza, indice piano, numero, nome piano
    for (ip, (nome, rate)) in piani.iter().enumerate() {
        let mut viste: BTreeMap<i64, String> = BTreeMap::new();
        for r in rate {
            viste.entry(r.numero).or_insert_with(|| r.scadenza.clone());
        }
        for (n, sc) in viste {
            chiavi.push((sc, ip, n, nome.clone()));
        }
    }
    chiavi.sort();

    // righe: una per persona
    #[derive(Default)]
    struct Acc {
        unita: Vec<String>,
        celle: Vec<Cents>,
        in_ritardo: Vec<bool>,
        dovuto: Cents,
        versato: Cents,
        scaduto: Cents,
    }
    let mut per_persona: BTreeMap<String, (i64, Acc)> = BTreeMap::new();
    for (ip, (_, rate)) in piani.iter().enumerate() {
        for r in rate {
            let nome = nomi_persone.get(&r.persona_id).cloned().unwrap_or_else(|| format!("persona {}", r.persona_id));
            let e = per_persona.entry(nome).or_insert_with(|| (r.persona_id, Acc { celle: vec![0; chiavi.len()], in_ritardo: vec![false; chiavi.len()], ..Default::default() }));
            if let Some(u) = unita.get(&r.unita_id) {
                if !e.1.unita.contains(u) {
                    e.1.unita.push(u.clone());
                }
            }
            if let Some(k) = chiavi.iter().position(|x| x.1 == ip && x.2 == r.numero) {
                e.1.celle[k] += r.importo;
                if r.scadenza < oggi && r.pagato < r.importo {
                    e.1.in_ritardo[k] = true;
                }
            }
            e.1.dovuto += r.importo;
            e.1.versato += r.pagato;
            if r.scadenza < oggi && r.pagato < r.importo {
                e.1.scaduto += r.importo - r.pagato;
            }
        }
    }

    let mut colonne: Vec<Col> = vec![col("Anagrafica", 190.0), col("Unità", 90.0)];
    for (sc, _, n, nome) in &chiavi {
        let abbr: String = nome.chars().take(9).collect();
        colonne.push(col_dx_dinamica(format!("{} {} · {}", abbr, n, data_corta(sc)), 150.0));
    }
    colonne.push(col_dx("Totale dovuto", 105.0));
    colonne.push(col_dx("Totale versato", 105.0));
    colonne.push(col_dx("Saldo", 95.0));
    colonne.push(col_dx("Totale scaduto", 105.0));

    let f = st.filtro.to_lowercase();
    let mut righe: Vec<Riga> = vec![];
    let mut totali = vec![0 as Cents; chiavi.len() + 4];
    for (nome, (_, a)) in per_persona.iter().filter(|(n, _)| f.is_empty() || n.to_lowercase().contains(&f)) {
        let mut celle: Vec<Cella> = vec![nome.clone().into(), a.unita.join(", ").into()];
        for (k, v) in a.celle.iter().enumerate() {
            totali[k] += v;
            celle.push(if a.in_ritardo[k] { colorata(format_eur(*v), ROSSO) } else { format_eur(*v).into() });
        }
        let saldo = a.versato - a.dovuto;
        let n = chiavi.len();
        totali[n] += a.dovuto;
        totali[n + 1] += a.versato;
        totali[n + 2] += saldo;
        totali[n + 3] -= a.scaduto;
        celle.push(format_eur(a.dovuto).into());
        celle.push(format_eur(a.versato).into());
        celle.push(cella_eur(saldo));
        celle.push(cella_eur(-a.scaduto));
        righe.push(Riga::new(celle));
    }
    let mut cp: Vec<Cella> = vec![forte("TOTALE COMPLESSIVO"), "".into()];
    for t in &totali[..chiavi.len()] {
        cp.push(forte(format_eur(*t)));
    }
    let n = chiavi.len();
    cp.push(forte(format_eur(totali[n])));
    cp.push(forte(format_eur(totali[n + 1])));
    cp.push(if totali[n + 2] < 0 { colorata(format_eur(totali[n + 2]), ROSSO) } else { forte(format_eur(totali[n + 2])) });
    cp.push(if totali[n + 3] < 0 { colorata(format_eur(totali[n + 3]), ROSSO) } else { forte(format_eur(totali[n + 3])) });
    let piede = Riga::new(cp);

    barra_azioni(ui, "az_rate", |ui| {
        if azione(ui, "🔀", BLU, "Definizione piano rateale") {
            *c.app = Some(AzioneApp::Vai(Sezione::PianiRate, None));
        }
        if azione(ui, "🖨", BLU, "Stampa") {
            match c.db.html_scadenzario(c.cond, oggi_data(), oggi_data() + chrono::Duration::days(90)) {
                Ok(h) => apri_html(c, "scadenzario", &h),
                Err(e) => c.err(e),
            }
        }
        if azione(ui, "📗", VERDE_ICONA, "Excel") {
            esporta_csv(c, "rate", &colonne, &righe);
        }
        if azione(ui, "💶", ARANCIO_ICONA, "Registra versamenti") {
            *c.app = Some(AzioneApp::Vai(Sezione::Incassi, Some(0)));
        }
        if azione(ui, "％", VERDE_ICONA, "Interessi morosità") {
            *c.app = Some(AzioneApp::Vai(Sezione::Morosita, Some(3)));
        }
        if azione(ui, "❗", BLU, "Rate scadute") {
            *c.app = Some(AzioneApp::Vai(Sezione::Morosita, Some(0)));
        }
    });

    barra_filtri(ui, Some(&mut st.filtro), |ui| {
        if piani.is_empty() {
            ui.label(RichText::new("Nessun piano rate attivo in questo esercizio: crealo con «Definizione piano rateale».").color(ARANCIO));
        } else {
            ui.label(format!("{} piani attivi · le rate in rosso sono scadute e non saldate", piani.len()));
        }
    });
    if st.sel.map(|i| i >= righe.len()).unwrap_or(false) {
        st.sel = None;
    }
    let esito = elenco(ui, "t_rate", &colonne, &righe, st.sel, Some(&piede));
    if let Some(i) = esito.clic {
        st.sel = Some(i);
    }
}

fn oggi_data() -> chrono::NaiveDate {
    oggi()
}
