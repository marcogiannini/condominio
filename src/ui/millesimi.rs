//! Tabelle millesimali: elenco delle tabelle a sinistra, coefficienti per unità a destra.

use super::elenco::*;
use super::*;
use crate::anagrafica::*;
use std::collections::HashMap;

pub struct Stato {
    sel: Option<i64>,
    tab: usize,
    form: Tabella,
    totale_txt: String,
    valori: HashMap<i64, String>,
    caricata: Option<i64>,
    nuova: bool,
}

impl Default for Stato {
    fn default() -> Self {
        Stato { sel: None, tab: 0, form: Tabella::default(), totale_txt: "1000".into(), valori: HashMap::new(), caricata: None, nuova: false }
    }
}

const COLONNE: [Col; 3] = [col("Tabelle", 0.0), col("Tipo", 90.0), col_dx("Totale", 100.0)];

fn sigla(u: &Unita) -> String {
    let mut p = vec![];
    if !u.scala.is_empty() {
        p.push(format!("Sc.{}", u.scala));
    }
    p.push(format!("Int.{}", if u.interno.is_empty() { "-" } else { &u.interno }));
    p.join(" ")
}

pub fn mostra(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    let tabelle = c.db.elenco_tabelle(c.cond).unwrap_or_default();
    if st.sel.is_none() && !st.nuova {
        st.sel = tabelle.first().map(|t| t.id);
    }
    let righe: Vec<Riga> = tabelle
        .iter()
        .map(|t| {
            let somma = c.db.somma_millesimi(t.id).unwrap_or(0);
            let tot = if somma == t.totale_micro { format_millesimi(somma).into() } else { colorata(format!("{} / {}", format_millesimi(somma), format_millesimi(t.totale_micro)), ARANCIO) };
            Riga::new(vec![format!("Tab. {} - {}", t.codice, t.nome).into(), "standard".into(), tot])
        })
        .collect();
    let selezionata = st.sel.and_then(|id| tabelle.iter().position(|t| t.id == id));
    let unita = c.db.elenco_unita(c.cond).unwrap_or_default();

    // allinea scheda e coefficienti alla selezione
    if !st.nuova && st.caricata != st.sel {
        match st.sel.and_then(|id| tabelle.iter().find(|t| t.id == id)) {
            Some(t) => {
                st.form = t.clone();
                st.totale_txt = format_millesimi(t.totale_micro);
                st.valori.clear();
                for u in &unita {
                    let v = c.db.millesimo(t.id, u.id).unwrap_or(0);
                    st.valori.insert(u.id, if v == 0 { String::new() } else { format_millesimi(v) });
                }
            }
            None => st.form = Tabella::default(),
        }
        st.caricata = st.sel;
    }

    let colonne_coeff = [col("Unità", 90.0), col("Proprietario", 0.0), col_dx("Coeff.", 90.0)];
    let righe_coeff: Vec<Riga> = unita
        .iter()
        .map(|u| {
            let prop = c.db.elenco_titolari(u.id).unwrap_or_default().iter().filter(|t| t.ruolo == "proprietario").map(|t| t.persona.clone()).collect::<Vec<_>>().join(" - ");
            Riga::new(vec![sigla(u).into(), prop.into(), st.valori.get(&u.id).cloned().unwrap_or_default().into()])
        })
        .collect();

    barra_azioni(ui, "az_tabelle", |ui| {
        if azione(ui, "📄", BLU, "Nuovo") {
            st.nuova = true;
            st.sel = None;
            st.caricata = None;
            st.form = Tabella::default();
            st.totale_txt = "1000".into();
            st.tab = 1;
        }
        if azione(ui, "✖", ROSSO_ICONA, "Elimina") {
            if let Some(t) = st.sel.and_then(|id| tabelle.iter().find(|t| t.id == id)) {
                c.chiedi(&format!("Eliminare la tabella {}?", t.codice), Azione::EliminaTabella(t.id));
            }
        }
        if azione(ui, "📗", VERDE_ICONA, "Excel") {
            esporta_csv(c, "coefficienti", &colonne_coeff, &righe_coeff);
        }
        if azione(ui, "🚪", BLU, "Unità") {
            *c.app = Some(AzioneApp::Vai(Sezione::Unita, None));
        }
        if azione(ui, "📊", ARANCIO_ICONA, "Riparti") {
            *c.app = Some(AzioneApp::Vai(Sezione::Preventivo, Some(3)));
        }
    });

    pannello_dettaglio(ui, "dett_tabelle", 480.0, |ui| {
        schede(ui, &mut st.tab, &["Generale", "Impostazioni"]);
        ui.add_space(8.0);
        if st.tab == 0 {
            let Some(t) = st.sel.and_then(|id| tabelle.iter().find(|t| t.id == id)) else {
                ui.label("Seleziona una tabella oppure creane una nuova (pulsante Nuovo).");
                return;
            };
            ui.label(RichText::new(format!("Tab. {} - {}", t.codice, t.nome)).strong().size(14.0));
            ui.add_space(4.0);
            egui::Grid::new("g_mill").striped(true).num_columns(3).spacing([10.0, 4.0]).show(ui, |ui| {
                ui.strong("Unità");
                ui.strong("Proprietario");
                ui.strong("Coeff.");
                ui.end_row();
                for (u, r) in unita.iter().zip(&righe_coeff) {
                    ui.label(sigla(u));
                    ui.label(&r.celle[1].t);
                    let campo = st.valori.entry(u.id).or_default();
                    ui.add(egui::TextEdit::singleline(campo).desired_width(90.0).hint_text("0,000").horizontal_align(egui::Align::RIGHT));
                    ui.end_row();
                }
            });
            let somma_inserita: i64 = st.valori.values().filter_map(|v| parse_millesimi(v)).sum();
            ui.add_space(4.0);
            let col_somma = if somma_inserita == t.totale_micro { VERDE } else { ARANCIO };
            ui.colored_label(col_somma, format!("Somma inserita: {} su {}", format_millesimi(somma_inserita), format_millesimi(t.totale_micro)));
            if ui.button("Salva coefficienti").clicked() {
                let mut errore = None;
                for u in &unita {
                    let txt = st.valori.get(&u.id).cloned().unwrap_or_default();
                    let micro = if txt.trim().is_empty() { Some(0) } else { parse_millesimi(&txt) };
                    match micro {
                        Some(m) => {
                            if let Err(e) = c.db.imposta_millesimo(t.id, u.id, m) {
                                errore = Some(e);
                                break;
                            }
                        }
                        None => {
                            errore = Some(anyhow::anyhow!("Valore non valido per {}: «{}» (max 3 decimali)", sigla(u), txt));
                            break;
                        }
                    }
                }
                match errore {
                    Some(e) => c.err(e),
                    None => c.ok("Coefficienti salvati"),
                }
            }
        } else {
            ui.label(RichText::new(if st.form.id == 0 { "Nuova tabella" } else { "Tabella" }).strong());
            grid_scheda("g_tab").show(ui, |ui| {
                campo_pieno(ui, "Codice * (es. A)", &mut st.form.codice);
                campo_pieno(ui, "Nome * (es. Proprietà generale)", &mut st.form.nome);
                campo_pieno(ui, "Descrizione", &mut st.form.descrizione);
                campo_pieno(ui, "Totale atteso", &mut st.totale_txt);
            });
            if ui.button("Salva tabella").clicked() {
                match parse_millesimi(&st.totale_txt) {
                    Some(t) => {
                        st.form.condominio_id = c.cond;
                        st.form.totale_micro = t;
                        let r = c.db.salva_tabella(&st.form);
                        if let Some(id) = c.esito(r, "Tabella salvata") {
                            st.sel = Some(id);
                            st.nuova = false;
                            st.caricata = None;
                            st.tab = 0;
                        }
                    }
                    None => c.errore("Totale non valido"),
                }
            }
        }
    });

    barra_filtri(ui, None, |ui| {
        ui.label(format!("Tabelle millesimali ({})", tabelle.len()));
    });
    let esito = elenco(ui, "t_tabelle", &COLONNE, &righe, selezionata, None);
    if let Some(i) = esito.clic {
        st.sel = Some(tabelle[i].id);
        st.nuova = false;
    }
}
