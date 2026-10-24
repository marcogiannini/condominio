use super::*;
use crate::preventivo::*;

#[derive(Default)]
pub struct Stato {
    tab: usize,
    esercizio: Esercizio,
    delibera: Delibera,
    delibera_data: String,
    riparti: super::riparti::Stato,
}

pub fn mostra(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    if c.schede {
        titolo(ui, "Delibere, Esercizi, Riparti");
    }
    ui.horizontal(|ui| {
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
        1 => delibere(c, st, ui),
        3 => super::riparti::mostra(c, &mut st.riparti, ui),
        _ => esercizi(c, st, ui),
    }
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
