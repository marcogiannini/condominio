use super::*;
use crate::preventivo::*;
use std::collections::HashMap;

#[derive(Default)]
pub struct Stato {
    tab: usize,
    voce: VoceSpesa,
    importo_txt: String,
    parent: Option<i64>,
    tabella: Option<i64>,
    esercizio: Esercizio,
    delibera: Delibera,
    delibera_data: String,
    riparti: super::riparti::Stato,
}

pub fn mostra(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    titolo(ui, "Preventivo e Piano dei Conti");
    ui.horizontal(|ui| {
        if c.schede {
            ui.selectable_value(&mut st.tab, 0, "Piano dei conti (voci di spesa)");
        }
        if c.schede {
            ui.selectable_value(&mut st.tab, 1, "Delibere assembleari");
        }
        if c.schede {
            ui.selectable_value(&mut st.tab, 3, "Riparti e riduzioni");
        }
        if c.schede {
            ui.selectable_value(&mut st.tab, 2, "Esercizi");
        }
    });
    ui.separator();
    match st.tab {
        0 => voci(c, st, ui),
        1 => delibere(c, st, ui),
        3 => super::riparti::mostra(c, &mut st.riparti, ui),
        _ => esercizi(c, st, ui),
    }
}

fn profondita(v: &VoceSpesa, mappa: &HashMap<i64, &VoceSpesa>) -> usize {
    let mut d = 0;
    let mut cur = v.parent_id;
    while let Some(p) = cur {
        d += 1;
        cur = mappa.get(&p).and_then(|x| x.parent_id);
        if d > 8 {
            break;
        }
    }
    d
}

fn totale_ramo(id: i64, voci: &[VoceSpesa]) -> Cents {
    let figli: Vec<&VoceSpesa> = voci.iter().filter(|v| v.parent_id == Some(id)).collect();
    if figli.is_empty() {
        voci.iter().find(|v| v.id == id).map(|v| v.importo_cents).unwrap_or(0)
    } else {
        figli.iter().map(|f| totale_ramo(f.id, voci)).sum()
    }
}

fn voci(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    let Some(es) = c.esercizio else {
        ui.label("Crea o seleziona un esercizio (scheda «Esercizi»).");
        return;
    };
    let lista = c.db.elenco_voci(es).unwrap_or_default();
    let tabelle: Vec<(i64, String)> = c.db.elenco_tabelle(c.cond).unwrap_or_default().iter().map(|t| (t.id, format!("{} – {}", t.codice, t.nome))).collect();
    let candidati: Vec<(i64, String)> = lista.iter().map(|v| (v.id, format!("{} {}", v.codice, v.descrizione))).collect();

    ui.group(|ui| {
        ui.label(RichText::new(if st.voce.id == 0 { "Nuova voce" } else { "Modifica voce" }).strong());
        egui::Grid::new("g_voce").num_columns(2).show(ui, |ui| {
            campo(ui, "Codice * (es. 1.01)", &mut st.voce.codice);
            campo(ui, "Descrizione *", &mut st.voce.descrizione);
            campo(ui, "Importo preventivato (€)", &mut st.importo_txt);
            ui.label("Capitolo padre");
            scegli(ui, "v_parent", &mut st.parent, &candidati, "— nessuno (capitolo) —");
            ui.end_row();
            ui.label("Tabella millesimale");
            scegli(ui, "v_tab", &mut st.tabella, &tabelle, "— nessuna —");
            ui.end_row();
            ui.label("");
            ui.checkbox(&mut st.voce.sopravvenienza, "Sopravvenienza / spesa fuori preventivo (Art. 1130-bis c.c.)");
            ui.end_row();
        });
        ui.horizontal(|ui| {
            if ui.button("Salva voce").clicked() {
                let imp = if st.importo_txt.trim().is_empty() { Some(0) } else { parse_eur(&st.importo_txt) };
                match imp {
                    Some(i) => {
                        st.voce.esercizio_id = es;
                        st.voce.importo_cents = i;
                        st.voce.parent_id = st.parent;
                        st.voce.tabella_id = st.tabella;
                        let r = c.db.salva_voce(&st.voce);
                        if c.esito(r, "Voce salvata").is_some() {
                            st.voce = VoceSpesa::default();
                            st.importo_txt.clear();
                            st.parent = None;
                            st.tabella = None;
                        }
                    }
                    None => c.errore("Importo non valido"),
                }
            }
            if st.voce.id != 0 && ui.button("Annulla modifica").clicked() {
                st.voce = VoceSpesa::default();
                st.importo_txt.clear();
                st.parent = None;
                st.tabella = None;
            }
        });
    });
    ui.add_space(6.0);

    let mappa: HashMap<i64, &VoceSpesa> = lista.iter().map(|v| (v.id, v)).collect();
    egui::Grid::new("t_voci").striped(true).num_columns(7).show(ui, |ui| {
        for h in ["Voce", "Preventivo", "Consuntivo", "Tabella", "Note", "", ""] {
            ui.strong(h);
        }
        ui.end_row();
        for v in &lista {
            let d = profondita(v, &mappa);
            let ha_figli = lista.iter().any(|x| x.parent_id == Some(v.id));
            let testo = format!("{}{} {}", "    ".repeat(d), v.codice, v.descrizione);
            if ha_figli {
                ui.label(RichText::new(testo).strong());
            } else {
                ui.label(testo);
            }
            let tot = totale_ramo(v.id, &lista);
            ui.label(if ha_figli { RichText::new(format_eur(tot)).strong() } else { RichText::new(format_eur(tot)) });
            let cons = c.db.consuntivo_voce(v.id).unwrap_or(0);
            if cons > 0 && !ha_figli {
                let col = if cons > tot { ROSSO } else { ui.visuals().text_color() };
                ui.label(RichText::new(format_eur(cons)).color(col));
            } else {
                ui.label("");
            }
            let tab = v.tabella_id.and_then(|t| tabelle.iter().find(|x| x.0 == t)).map(|x| x.1.clone()).unwrap_or_default();
            ui.label(tab);
            ui.horizontal(|ui| {
                if v.sopravvenienza {
                    ui.label(RichText::new(" Art. 1130-bis ").background_color(ARANCIO).color(Color32::BLACK).small());
                }
                let ancorata: i64 = c
                    .db
                    .conn
                    .query_row(
                        "SELECT COUNT(*) FROM piano_voce pv JOIN piano_rate p ON p.id=pv.piano_id WHERE pv.voce_id=?1 AND p.stato='attivo'",
                        [v.id],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                if ancorata > 0 {
                    ui.label(RichText::new(" in piano attivo ").background_color(VERDE).color(Color32::WHITE).small());
                }
            });
            if mini(ui, "Modifica") {
                st.voce = v.clone();
                st.importo_txt = format_eur(v.importo_cents).trim_end_matches(" €").to_string();
                st.parent = v.parent_id;
                st.tabella = v.tabella_id;
            }
            if mini(ui, "Elimina") {
                c.chiedi(&format!("Eliminare la voce {}?", v.codice), Azione::EliminaVoce(v.id));
            }
            ui.end_row();
        }
    });
    let totale = c.db.totale_preventivo(es).unwrap_or(0);
    ui.add_space(6.0);
    ui.label(RichText::new(format!("Totale preventivo: {}", format_eur(totale))).strong());
}

fn delibere(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    ui.group(|ui| {
        ui.label(RichText::new(if st.delibera.id == 0 { "Nuova delibera" } else { "Modifica delibera" }).strong());
        if st.delibera.tipo.is_empty() {
            st.delibera.tipo = "preventivo".into();
        }
        egui::Grid::new("g_del").num_columns(2).show(ui, |ui| {
            campo(ui, "Data assemblea * (GG/MM/AAAA)", &mut st.delibera_data);
            ui.label("Tipo");
            scegli_str(ui, "d_tipo", &mut st.delibera.tipo, &TIPI_DELIBERA);
            ui.end_row();
            campo(ui, "Oggetto *", &mut st.delibera.oggetto);
            campo(ui, "Riferimento al verbale *", &mut st.delibera.riferimento);
            campo(ui, "Maggioranza (es. Art. 1136 c.2)", &mut st.delibera.maggioranza);
        });
        ui.horizontal(|ui| {
            if ui.button("Salva delibera").clicked() {
                st.delibera.condominio_id = c.cond;
                st.delibera.esercizio_id = c.esercizio;
                st.delibera.data = st.delibera_data.clone();
                let r = c.db.salva_delibera(&st.delibera);
                if c.esito(r, "Delibera registrata").is_some() {
                    st.delibera = Delibera::default();
                    st.delibera_data.clear();
                }
            }
            if st.delibera.id != 0 && ui.button("Annulla modifica").clicked() {
                st.delibera = Delibera::default();
                st.delibera_data.clear();
            }
        });
    });
    ui.add_space(6.0);
    egui::Grid::new("t_del").striped(true).num_columns(5).show(ui, |ui| {
        for h in ["Data", "Tipo", "Oggetto", "Verbale", ""] {
            ui.strong(h);
        }
        ui.end_row();
        for d in c.db.elenco_delibere(c.cond).unwrap_or_default() {
            ui.label(data_it(&d.data));
            ui.label(&d.tipo);
            ui.label(&d.oggetto);
            ui.label(&d.riferimento);
            if mini(ui, "Modifica") {
                st.delibera_data = data_it(&d.data);
                st.delibera = d.clone();
            }
            ui.end_row();
        }
    });
}

fn esercizi(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    ui.group(|ui| {
        ui.label(RichText::new("Nuovo esercizio").strong());
        egui::Grid::new("g_es").num_columns(2).show(ui, |ui| {
            campo(ui, "Nome * (es. 2026)", &mut st.esercizio.nome);
            campo(ui, "Dal * (GG/MM/AAAA)", &mut st.esercizio.dal);
            campo(ui, "Al * (GG/MM/AAAA)", &mut st.esercizio.al);
        });
        if ui.button("Crea esercizio").clicked() {
            st.esercizio.condominio_id = c.cond;
            let r = c.db.salva_esercizio(&st.esercizio);
            if c.esito(r, "Esercizio creato").is_some() {
                st.esercizio = Esercizio::default();
            }
        }
    });
    ui.add_space(6.0);
    egui::Grid::new("t_es").striped(true).num_columns(5).show(ui, |ui| {
        for h in ["Esercizio", "Dal", "Al", "Stato", ""] {
            ui.strong(h);
        }
        ui.end_row();
        for e in c.db.elenco_esercizi(c.cond).unwrap_or_default() {
            ui.label(&e.nome);
            ui.label(data_it(&e.dal));
            ui.label(data_it(&e.al));
            ui.label(&e.stato);
            let (testo, nuovo) = if e.stato == "aperto" { ("Chiudi esercizio", "chiuso") } else { ("Riapri", "aperto") };
            if mini(ui, testo) {
                let r = c.db.imposta_stato_esercizio(e.id, nuovo);
                c.esito(r, "Stato dell'esercizio aggiornato");
            }
            ui.end_row();
        }
    });
}

impl Stato {
    pub fn imposta_tab(&mut self, t: usize) {
        self.tab = t;
    }
}
