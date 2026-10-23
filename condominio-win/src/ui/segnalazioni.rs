use super::*;
use crate::segnalazioni::{Ticket, PRIORITA, STATI_TICKET};

pub struct Stato {
    solo_aperti: bool,
    stato: String, // "" = tutti
    n_unita: Option<i64>,
    n_persona: Option<i64>,
    n_titolo: String,
    n_descr: String,
    n_priorita: usize,
    aperto: Option<i64>,
    commento: String,
    autore: String,
}

impl Default for Stato {
    fn default() -> Self {
        Stato {
            solo_aperti: true,
            stato: String::new(),
            n_unita: None,
            n_persona: None,
            n_titolo: String::new(),
            n_descr: String::new(),
            n_priorita: 1,
            aperto: None,
            commento: String::new(),
            autore: "Amministratore".into(),
        }
    }
}

fn colore_priorita(p: i64) -> Color32 {
    match p {
        3 => ROSSO,
        2 => ARANCIO,
        _ => Color32::GRAY,
    }
}

pub fn mostra(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    titolo(ui, "Segnalazioni guasto");

    let unita: Vec<(i64, String)> = c.db.elenco_unita(c.cond).unwrap_or_default().iter().map(|u| (u.id, u.etichetta())).collect();
    let persone: Vec<(i64, String)> = c.db.persone_del_condominio(c.cond).unwrap_or_default().iter().map(|p| (p.id, p.nome_completo())).collect();

    ui.collapsing("Nuova segnalazione", |ui| {
        egui::Grid::new("g_tk_new").num_columns(2).show(ui, |ui| {
            ui.label("Segnalante");
            scegli(ui, "tk_pers", &mut st.n_persona, &persone, "(nessuno)");
            ui.end_row();
            ui.label("Unità");
            scegli(ui, "tk_un", &mut st.n_unita, &unita, "(parti comuni)");
            ui.end_row();
            campo(ui, "Titolo *", &mut st.n_titolo);
            campo_lungo(ui, "Descrizione", &mut st.n_descr, 3);
            ui.label("Priorità");
            egui::ComboBox::from_id_salt("tk_prio").selected_text(PRIORITA[st.n_priorita]).show_ui(ui, |ui| {
                for (i, p) in PRIORITA.iter().enumerate() {
                    ui.selectable_value(&mut st.n_priorita, i, *p);
                }
            });
            ui.end_row();
        });
        if ui.button("Apri segnalazione").clicked() {
            let r = c.db.crea_ticket(c.cond, st.n_unita, st.n_persona, &st.n_titolo, &st.n_descr, st.n_priorita as i64);
            if c.esito(r, "Segnalazione aperta").is_some() {
                st.n_titolo.clear();
                st.n_descr.clear();
            }
        }
    });

    ui.separator();
    ui.horizontal(|ui| {
        ui.checkbox(&mut st.solo_aperti, "Solo aperte");
        ui.label("Stato");
        egui::ComboBox::from_id_salt("tk_filtro").selected_text(if st.stato.is_empty() { "Tutti" } else { st.stato.as_str() }).show_ui(ui, |ui| {
            if ui.selectable_label(st.stato.is_empty(), "Tutti").clicked() {
                st.stato.clear();
            }
            for s in STATI_TICKET {
                if ui.selectable_label(st.stato == s, s).clicked() {
                    st.stato = s.to_string();
                }
            }
        });
    });

    let filtro = if st.stato.is_empty() { None } else { Some(st.stato.as_str()) };
    let tickets: Vec<Ticket> = c.db.elenco_ticket(c.cond, filtro, st.solo_aperti && filtro.is_none()).unwrap_or_default();
    if tickets.is_empty() {
        ui.label("Nessuna segnalazione.");
    }
    for t in tickets {
        let aperto = st.aperto == Some(t.id);
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(PRIORITA.get(t.priorita as usize).copied().unwrap_or("?")).color(colore_priorita(t.priorita)).strong());
                ui.label(RichText::new(&t.titolo).strong());
                ui.label(format!("· {} · {}", t.stato, t.segnalante));
                ui.label(RichText::new(format!("aperta il {}", t.creato_il.get(..10).map(data_it).unwrap_or_default())).small());
                if mini(ui, if aperto { "Chiudi dettaglio" } else { "Dettaglio" }) {
                    st.aperto = if aperto { None } else { Some(t.id) };
                    st.commento.clear();
                }
            });
            if aperto {
                if !t.descrizione.is_empty() {
                    ui.label(&t.descrizione);
                }
                ui.horizontal(|ui| {
                    ui.label("Stato:");
                    for s in STATI_TICKET {
                        if ui.selectable_label(t.stato == s, s).clicked() && t.stato != s {
                            let r = c.db.cambia_stato_ticket(t.id, s, &st.autore);
                            c.esito(r, "Stato aggiornato");
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Priorità:");
                    for (i, p) in PRIORITA.iter().enumerate() {
                        if ui.selectable_label(t.priorita == i as i64, *p).clicked() && t.priorita != i as i64 {
                            let r = c.db.cambia_priorita_ticket(t.id, i as i64);
                            c.esito(r, "Priorità aggiornata");
                        }
                    }
                });
                ui.separator();
                for (quando, autore, testo) in c.db.commenti_ticket(t.id).unwrap_or_default() {
                    ui.label(format!("{} — {}: {}", quando, autore, testo));
                }
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut st.autore).desired_width(120.0));
                    ui.add(egui::TextEdit::singleline(&mut st.commento).hint_text("Aggiungi un commento…").desired_width(320.0));
                    if ui.button("Invia").clicked() && !st.commento.trim().is_empty() {
                        let r = c.db.aggiungi_commento_ticket(t.id, &st.autore, &st.commento);
                        if c.esito(r, "Commento aggiunto").is_some() {
                            st.commento.clear();
                        }
                    }
                });
                if mini(ui, "Elimina segnalazione") {
                    c.chiedi(&format!("Eliminare la segnalazione «{}»?", t.titolo), Azione::EliminaTicket(t.id));
                }
            }
        });
    }
}
