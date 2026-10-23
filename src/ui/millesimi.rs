use super::*;
use crate::anagrafica::*;
use std::collections::HashMap;

#[derive(Default)]
pub struct Stato {
    tabella: Tabella,
    sel: Option<i64>,
    valori: HashMap<i64, String>,
    caricata: Option<i64>,
    totale_txt: String,
}

pub fn mostra(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    titolo(ui, "Tabelle millesimali");
    ui.group(|ui| {
        ui.label(RichText::new(if st.tabella.id == 0 { "Nuova tabella" } else { "Modifica tabella" }).strong());
        if st.totale_txt.is_empty() {
            st.totale_txt = "1000".into();
        }
        egui::Grid::new("g_tab").num_columns(2).show(ui, |ui| {
            campo(ui, "Codice * (es. A)", &mut st.tabella.codice);
            campo(ui, "Nome * (es. Proprietà generale)", &mut st.tabella.nome);
            campo(ui, "Descrizione", &mut st.tabella.descrizione);
            campo(ui, "Totale atteso (millesimi)", &mut st.totale_txt);
        });
        ui.horizontal(|ui| {
            if ui.button("Salva tabella").clicked() {
                match parse_millesimi(&st.totale_txt) {
                    Some(t) => {
                        st.tabella.condominio_id = c.cond;
                        st.tabella.totale_micro = t;
                        let r = c.db.salva_tabella(&st.tabella);
                        if let Some(id) = c.esito(r, "Tabella salvata") {
                            st.sel = Some(id);
                            st.caricata = None;
                            st.tabella = Tabella::default();
                            st.totale_txt = "1000".into();
                        }
                    }
                    None => c.errore("Totale non valido"),
                }
            }
            if st.tabella.id != 0 && ui.button("Annulla modifica").clicked() {
                st.tabella = Tabella::default();
            }
        });
    });

    let tabelle = c.db.elenco_tabelle(c.cond).unwrap_or_default();
    ui.add_space(6.0);
    egui::Grid::new("t_tabelle").striped(true).num_columns(5).show(ui, |ui| {
        for h in ["Codice", "Nome", "Somma / Totale", "", ""] {
            ui.strong(h);
        }
        ui.end_row();
        for t in &tabelle {
            if ui.selectable_label(st.sel == Some(t.id), &t.codice).clicked() {
                st.sel = Some(t.id);
                st.caricata = None;
            }
            ui.label(&t.nome);
            let somma = c.db.somma_millesimi(t.id).unwrap_or(0);
            let txt = format!("{} / {}", format_millesimi(somma), format_millesimi(t.totale_micro));
            if somma == t.totale_micro {
                ui.colored_label(VERDE, txt);
            } else {
                ui.colored_label(ARANCIO, txt);
            }
            if mini(ui, "Modifica") {
                st.tabella = t.clone();
                st.totale_txt = format_millesimi(t.totale_micro);
            }
            if mini(ui, "Elimina") {
                c.chiedi(&format!("Eliminare la tabella {}?", t.codice), Azione::EliminaTabella(t.id));
            }
            ui.end_row();
        }
    });

    let Some(sel) = st.sel.and_then(|s| tabelle.iter().find(|t| t.id == s)) else { return };
    ui.separator();
    ui.label(RichText::new(format!("Millesimi – tabella {} ({})", sel.codice, sel.nome)).strong());
    let unita = c.db.elenco_unita(c.cond).unwrap_or_default();
    if st.caricata != Some(sel.id) {
        st.valori.clear();
        for u in &unita {
            let v = c.db.millesimo(sel.id, u.id).unwrap_or(0);
            st.valori.insert(u.id, if v == 0 { String::new() } else { format_millesimi(v) });
        }
        st.caricata = Some(sel.id);
    }
    egui::Grid::new("g_mill").striped(true).num_columns(3).show(ui, |ui| {
        ui.strong("Unità");
        ui.strong("Millesimi");
        ui.strong("");
        ui.end_row();
        for u in &unita {
            ui.label(u.etichetta());
            let campo = st.valori.entry(u.id).or_default();
            ui.add(egui::TextEdit::singleline(campo).desired_width(100.0).hint_text("0,000"));
            ui.end_row();
        }
    });
    let somma_inserita: i64 = st.valori.values().filter_map(|v| parse_millesimi(v)).sum();
    ui.label(format!("Somma inserita: {} su {}", format_millesimi(somma_inserita), format_millesimi(sel.totale_micro)));
    if ui.button("Salva millesimi").clicked() {
        let mut errore = None;
        for u in &unita {
            let txt = st.valori.get(&u.id).cloned().unwrap_or_default();
            let micro = if txt.trim().is_empty() { Some(0) } else { parse_millesimi(&txt) };
            match micro {
                Some(m) => {
                    if let Err(e) = c.db.imposta_millesimo(sel.id, u.id, m) {
                        errore = Some(e);
                        break;
                    }
                }
                None => {
                    errore = Some(anyhow::anyhow!("Valore non valido per {}: «{}» (max 3 decimali)", u.etichetta(), txt));
                    break;
                }
            }
        }
        match errore {
            Some(e) => c.err(e),
            None => c.ok("Millesimi salvati"),
        }
    }
}
