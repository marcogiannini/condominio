use super::*;
use crate::legale::{AllertaPrescrizione, Controllo, Gravita, GIORNI_PREAVVISO_PRESCRIZIONE};

#[derive(Default)]
pub struct Stato {
    tab: usize,
    interruzione: Option<(i64, String)>,
    cache_controlli: Cache<Vec<Controllo>>,
    cache_prescr: Cache<Vec<AllertaPrescrizione>>,
}


pub fn mostra(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    titolo(ui, "Conformità legale italiana");
    ui.horizontal(|ui| {
        if c.schede {
            ui.selectable_value(&mut st.tab, 0, "Controlli (Art. 1130, 1130-bis, 1135 c.c.)");
        }
        if c.schede {
            ui.selectable_value(&mut st.tab, 1, "Prescrizione crediti (Art. 2948 c.c.)");
        }
    });
    ui.separator();
    let (db, cond) = (c.db, c.cond);
    if st.tab == 0 {
        let v: Vec<Controllo> = st.cache_controlli.get(*c.rev, "", || db.controlli_conformita(cond).unwrap_or_default()).clone();
        if v.iter().all(|x| x.gravita == Gravita::Ok) {
            ui.colored_label(VERDE, "Nessuna anomalia rilevata.");
        }
        for x in &v {
            let (col, tag) = match x.gravita {
                Gravita::Errore => (ROSSO, "ERRORE"),
                Gravita::Avviso => (ARANCIO, "AVVISO"),
                Gravita::Ok => (VERDE, "OK"),
            };
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(tag).color(col).strong());
                ui.label(RichText::new(x.riferimento).strong());
                ui.label(&x.messaggio);
            });
        }
        ui.add_space(8.0);
        ui.label(
            RichText::new("I controlli sono un ausilio e non sostituiscono la verifica di un professionista abilitato.").small(),
        );
        return;
    }

    ui.label(format!(
        "Le rate non riscosse si prescrivono in 5 anni dalla scadenza. Si segnalano i crediti in scadenza entro {} giorni. Un atto interruttivo (es. raccomandata di messa in mora) fa ripartire il termine.",
        GIORNI_PREAVVISO_PRESCRIZIONE
    ));
    let oggi = oggi();
    let v: Vec<AllertaPrescrizione> = st.cache_prescr.get(*c.rev, &oggi.to_string(), || db.allerte_prescrizione(cond, oggi).unwrap_or_default()).clone();
    if v.is_empty() {
        ui.colored_label(VERDE, "Nessun credito prossimo alla prescrizione.");
    }
    egui::Grid::new("t_prescr").striped(true).num_columns(7).show(ui, |ui| {
        for h in ["Condòmino", "Piano", "Scadenza rata", "Residuo", "Termine prescrizione", "Stato", ""] {
            ui.strong(h);
        }
        ui.end_row();
        for a in &v {
            ui.label(&a.persona);
            ui.label(&a.piano);
            ui.label(data_it(&a.scadenza));
            ui.label(format_eur(a.residuo));
            ui.label(data_it(&a.termine));
            if a.prescritta {
                ui.colored_label(ROSSO, "PRESCRITTO");
            } else {
                ui.colored_label(ARANCIO, format!("tra {} giorni{}", a.giorni_mancanti, if a.interrotta { " (interrotta)" } else { "" }));
            }
            if !a.prescritta && mini(ui, "Registra interruzione") {
                st.interruzione = Some((a.rata_id, data_it(&oggi_iso())));
            }
            ui.end_row();
        }
    });

    let mut salva = false;
    let mut chiudi = false;
    if let Some((_, data)) = &mut st.interruzione {
        egui::Window::new("Atto interruttivo").collapsible(false).show(ui.ctx(), |ui| {
            ui.label("Data dell'atto (GG/MM/AAAA)");
            ui.text_edit_singleline(data);
            ui.horizontal(|ui| {
                salva = ui.button("Registra").clicked();
                chiudi = ui.button("Annulla").clicked();
            });
        });
    }
    if salva {
        if let Some((rata, data)) = st.interruzione.clone() {
            match data_iso(&data) {
                Some(d) => {
                    let r = c.db.registra_interruzione(rata, &d);
                    if c.esito(r, "Interruzione registrata: il termine riparte dalla data indicata").is_some() {
                        st.interruzione = None;
                    }
                }
                None => c.errore("Data non valida"),
            }
        }
    }
    if chiudi {
        st.interruzione = None;
    }
}

impl Stato {
    pub fn imposta_tab(&mut self, t: usize) {
        self.tab = t;
    }
}
