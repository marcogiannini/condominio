//! Condomìni: albero condominio → esercizi a sinistra, scheda con le schede «Dati generali»,
//! «Struttura», «Risorse e fondi», «Altri dati» a destra (come in Domustudio).

use super::elenco::*;
use super::*;
use crate::preventivo::Esercizio;
use std::collections::HashSet;

pub struct Stato {
    aperti: HashSet<i64>,
    tab: usize,
    form: Condominio,
    nuovo_es: Option<Esercizio>,
    filtro: String,
    ultimo: Option<(i64, Option<i64>)>,
}

impl Default for Stato {
    fn default() -> Self {
        Stato { aperti: HashSet::new(), tab: 0, form: Condominio::default(), nuovo_es: None, filtro: String::new(), ultimo: None }
    }
}

const COLONNE: [Col; 6] = [col("Condominio/Esercizio", 0.0), col("Cod. cond.", 80.0), col("Cod. fiscale", 120.0), col("Tipo es.", 90.0), col("Apertura", 90.0), col("Chiusura", 90.0)];

pub fn mostra(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    let condomini = c.db.elenco_condomini().unwrap_or_default();
    st.aperti.insert(c.cond);

    // righe dell'albero
    let f = st.filtro.to_lowercase();
    let mut righe: Vec<Riga> = vec![];
    let mut mappa: Vec<(i64, Option<i64>)> = vec![];
    let mut selezionata = None;
    for k in condomini.iter().filter(|k| f.is_empty() || k.nome.to_lowercase().contains(&f) || k.codice_fiscale.contains(&f)) {
        let aperto = st.aperti.contains(&k.id);
        let sel_cond = k.id == c.cond;
        let freccia = if aperto { "▾ " } else { "▸ " };
        if sel_cond && c.esercizio.is_none() {
            selezionata = Some(righe.len());
        }
        righe.push(Riga::new(vec![forte(format!("{}{}", freccia, k.nome)), "".into(), k.codice_fiscale.clone().into()]));
        mappa.push((k.id, None));
        if aperto {
            for e in c.db.elenco_esercizi(k.id).unwrap_or_default() {
                if sel_cond && c.esercizio == Some(e.id) {
                    selezionata = Some(righe.len());
                }
                let col_testo = if e.stato == "aperto" { VERDE } else { Color32::from_rgb(110, 110, 110) };
                righe.push(
                    Riga::new(vec![e.nome.clone().into(), "".into(), "".into(), "Ordinario".into(), colorata(data_it(&e.dal), col_testo), colorata(data_it(&e.al), col_testo)])
                        .rientrata(18.0),
                );
                mappa.push((k.id, Some(e.id)));
            }
        }
    }

    // la scheda segue la selezione
    if st.ultimo != Some((c.cond, c.esercizio)) {
        if let Some(k) = condomini.iter().find(|k| k.id == c.cond) {
            st.form = k.clone();
        }
        st.ultimo = Some((c.cond, c.esercizio));
    }

    // ---- barra azioni
    barra_azioni(ui, "az_cond", |ui| {
        if azione(ui, "📄", BLU, "Nuovo") {
            *c.app = Some(AzioneApp::Nuovo);
        }
        if azione(ui, "➕", VERDE_ICONA, "Nuovo esercizio") {
            let anno = c.db.elenco_esercizi(c.cond).unwrap_or_default().iter().filter_map(|e| e.al.get(0..4).and_then(|a| a.parse::<i32>().ok())).max().map(|a| a + 1).unwrap_or_else(|| oggi_iso()[0..4].parse().unwrap_or(2026));
            st.nuovo_es = Some(Esercizio { condominio_id: c.cond, nome: anno.to_string(), dal: format!("01/01/{}", anno), al: format!("31/12/{}", anno), ..Default::default() });
        }
        if azione(ui, "✖", ROSSO_ICONA, "Elimina") {
            if let Some(k) = condomini.iter().find(|k| k.id == c.cond) {
                c.chiedi(&format!("Eliminare il condominio «{}» con tutti i suoi dati?", k.nome), Azione::EliminaCondominio(k.id));
            }
        }
        if azione(ui, "📗", VERDE_ICONA, "Excel") {
            esporta_csv(c, "condomini", &COLONNE, &righe);
        }
        if azione(ui, "🧪", GRIGIO_ICONA, "Carica esempio") {
            *c.app = Some(AzioneApp::Demo);
        }
    });

    // ---- scheda a destra
    pannello_dettaglio(ui, "dett_cond", 460.0, |ui| scheda(c, st, ui));

    // ---- finestra «nuovo esercizio»
    let mut crea = false;
    let mut chiudi = false;
    if let Some(e) = &mut st.nuovo_es {
        egui::Window::new("Nuovo esercizio").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ui.ctx(), |ui| {
            grid_scheda("g_nuovo_es").show(ui, |ui| {
                campo_pieno(ui, "Descrizione *", &mut e.nome);
                campo_pieno(ui, "Dal (GG/MM/AAAA) *", &mut e.dal);
                campo_pieno(ui, "Al (GG/MM/AAAA) *", &mut e.al);
            });
            ui.horizontal(|ui| {
                if ui.button("Crea esercizio").clicked() {
                    crea = true;
                }
                if ui.button("Annulla").clicked() {
                    chiudi = true;
                }
            });
        });
        if crea {
            match c.db.salva_esercizio(e) {
                Ok(id) => {
                    c.ok("Esercizio creato");
                    *c.app = Some(AzioneApp::Contesto(c.cond, Some(id)));
                    chiudi = true;
                }
                Err(err) => c.err(err),
            }
        }
    }
    if chiudi {
        st.nuovo_es = None;
    }

    // ---- elenco
    barra_filtri(ui, Some(&mut st.filtro), |ui| {
        ui.label(format!("Condomìni gestiti ({})", condomini.len()));
    });
    let esito = elenco(ui, "t_condomini", &COLONNE, &righe, selezionata, None);
    if let Some(i) = esito.clic {
        let (cond, es) = mappa[i];
        match es {
            Some(e) => *c.app = Some(AzioneApp::Contesto(cond, Some(e))),
            None => {
                if cond == c.cond && st.aperti.contains(&cond) && selezionata == Some(i) {
                    st.aperti.remove(&cond);
                } else {
                    st.aperti.insert(cond);
                    *c.app = Some(AzioneApp::Contesto(cond, None));
                }
            }
        }
    }
}

fn scheda(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    schede(ui, &mut st.tab, &["Dati generali", "Struttura", "Risorse e fondi", "Altri dati"]);
    ui.add_space(8.0);
    match st.tab {
        0 => {
            grid_scheda("g_cond_dati").show(ui, |ui| {
                campo_pieno(ui, "Intestazione", &mut st.form.nome);
                campo_pieno(ui, "Indirizzo", &mut st.form.indirizzo);
                campo_pieno(ui, "Città", &mut st.form.citta);
                campo_pieno(ui, "Cod. fiscale", &mut st.form.codice_fiscale);
            });
            ui.add_space(8.0);
            if let Some(es) = c.esercizio.and_then(|e| c.db.elenco_esercizi(c.cond).unwrap_or_default().into_iter().find(|x| x.id == e)) {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Esercizio ordinario").strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            for (nome, stato) in [("Chiuso", "chiuso"), ("Aperto", "aperto")] {
                                if ui.selectable_label(es.stato == stato, nome).clicked() && es.stato != stato {
                                    let r = c.db.imposta_stato_esercizio(es.id, stato);
                                    c.esito(r, "Stato dell'esercizio aggiornato");
                                }
                            }
                        });
                    });
                    grid_scheda("g_es_dati").show(ui, |ui| {
                        valore(ui, "Descrizione", &es.nome);
                        let giorni = match (chrono::NaiveDate::parse_from_str(&es.dal, "%Y-%m-%d"), chrono::NaiveDate::parse_from_str(&es.al, "%Y-%m-%d")) {
                            (Ok(a), Ok(b)) => (b - a).num_days() + 1,
                            _ => 0,
                        };
                        valore(ui, "Periodo", &format!("Dal {} - al {}  ({} giorni)", data_it(&es.dal), data_it(&es.al), giorni));
                    });
                });
                ui.add_space(8.0);
            }
            if ui.button("Salva modifiche").clicked() {
                let r = c.db.salva_condominio(&st.form);
                c.esito(r, "Condominio salvato");
            }
        }
        1 => {
            let unita = c.db.elenco_unita(c.cond).unwrap_or_default().len();
            let persone = c.db.persone_del_condominio(c.cond).unwrap_or_default().len();
            let tabelle = c.db.elenco_tabelle(c.cond).unwrap_or_default().len();
            let voci = c.esercizio.map(|e| c.db.elenco_voci(e).unwrap_or_default().len()).unwrap_or(0);
            let piani = c.esercizio.map(|e| c.db.elenco_piani(e).unwrap_or_default().len()).unwrap_or(0);
            grid_scheda("g_struttura").show(ui, |ui| {
                valore(ui, "Unità", &unita.to_string());
                valore(ui, "Persone", &persone.to_string());
                valore(ui, "Tabelle millesimali", &tabelle.to_string());
                valore(ui, "Voci di spesa (esercizio)", &voci.to_string());
                valore(ui, "Piani rate (esercizio)", &piani.to_string());
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Vai alle unità").clicked() {
                    *c.app = Some(AzioneApp::Vai(Sezione::Unita, None));
                }
                if ui.button("Vai alle tabelle").clicked() {
                    *c.app = Some(AzioneApp::Vai(Sezione::Millesimi, None));
                }
            });
        }
        2 => {
            let conti = c.db.elenco_conti_correnti(c.cond).unwrap_or_default();
            if conti.is_empty() {
                ui.label("Nessun conto corrente: aggiungilo da «Condomìni → Conti correnti».");
            }
            grid_scheda("g_risorse").show(ui, |ui| {
                for (id, nome, iban) in &conti {
                    ui.label(nome);
                    ui.vertical(|ui| {
                        eur_colorato(ui, c.db.saldo_conto_corrente(*id).unwrap_or(0));
                        if !iban.is_empty() {
                            ui.label(RichText::new(iban).small().weak());
                        }
                    });
                    ui.end_row();
                }
            });
            ui.add_space(6.0);
            if ui.button("Gestisci conti correnti…").clicked() {
                *c.app = Some(AzioneApp::Vai(Sezione::Contabilita, Some(0)));
            }
        }
        _ => {
            ui.label("Note");
            ui.add(egui::TextEdit::multiline(&mut st.form.note).desired_rows(12).desired_width(f32::INFINITY));
            if ui.button("Salva note").clicked() {
                let r = c.db.salva_condominio(&st.form);
                c.esito(r, "Note salvate");
            }
        }
    }
}
