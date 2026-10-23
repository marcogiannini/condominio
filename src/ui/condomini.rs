use super::*;

pub struct Stato {
    form: Condominio,
    modifica: bool,
}

impl Default for Stato {
    fn default() -> Self {
        Stato { form: Condominio::default(), modifica: false }
    }
}

pub fn mostra(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    titolo(ui, "Condomìni");
    ui.horizontal(|ui| {
        if ui.button("➕ Nuovo condominio").clicked() {
            *c.app = Some(AzioneApp::Nuovo);
        }
        if ui.button("Carica condominio di esempio").clicked() {
            *c.app = Some(AzioneApp::Demo);
        }
    });
    ui.add_space(4.0);

    let elenco = c.db.elenco_condomini().unwrap_or_default();
    egui::Grid::new("t_condomini").striped(true).num_columns(5).min_col_width(80.0).show(ui, |ui| {
        for h in ["Condominio", "Indirizzo", "Città", "Codice fiscale", ""] {
            ui.strong(h);
        }
        ui.end_row();
        for x in &elenco {
            let attuale = x.id == c.cond;
            let n = RichText::new(&x.nome);
            ui.label(if attuale { n.strong().color(ui.visuals().selection.bg_fill) } else { n });
            ui.label(&x.indirizzo);
            ui.label(&x.citta);
            ui.label(&x.codice_fiscale);
            ui.horizontal(|ui| {
                if !attuale && mini(ui, "Apri") {
                    *c.app = Some(AzioneApp::Apri(x.id));
                }
                if mini(ui, "Modifica") {
                    st.form = x.clone();
                    st.modifica = true;
                }
                if mini(ui, "Elimina") {
                    c.chiedi(&format!("Eliminare il condominio «{}» con tutti i suoi dati?", x.nome), Azione::EliminaCondominio(x.id));
                }
            });
            ui.end_row();
        }
    });

    if st.modifica {
        ui.add_space(8.0);
        ui.separator();
        ui.label(RichText::new("Scheda condominio").strong());
        egui::Grid::new("g_cond_scheda").num_columns(2).show(ui, |ui| {
            campo(ui, "Nome *", &mut st.form.nome);
            campo(ui, "Indirizzo", &mut st.form.indirizzo);
            campo(ui, "Città", &mut st.form.citta);
            campo(ui, "Codice fiscale", &mut st.form.codice_fiscale);
            campo_lungo(ui, "Note", &mut st.form.note, 3);
        });
        ui.horizontal(|ui| {
            if ui.button("Salva").clicked() {
                let r = c.db.salva_condominio(&st.form);
                if c.esito(r, "Condominio salvato").is_some() {
                    st.modifica = false;
                }
            }
            if ui.button("Annulla").clicked() {
                st.modifica = false;
            }
        });
    }
}
