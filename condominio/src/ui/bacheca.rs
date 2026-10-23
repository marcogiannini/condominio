use super::*;
use crate::bacheca::{Comunicazione, TIPI_COMUNICAZIONE};

pub struct Stato {
    tab: usize,
    form: Comunicazione,
    data_evento: String,
    invia_email: bool,
}

impl Default for Stato {
    fn default() -> Self {
        Stato { tab: 0, form: Comunicazione { tipo: "avviso".into(), ..Default::default() }, data_evento: String::new(), invia_email: true }
    }
}

pub fn mostra(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    titolo(ui, "Bacheca e Comunicazioni");
    ui.horizontal(|ui| {
        if c.schede {
            ui.selectable_value(&mut st.tab, 0, "Comunicazioni");
        }
        if c.schede {
            ui.selectable_value(&mut st.tab, 1, "Registro e-mail");
        }
    });
    ui.separator();
    if st.tab == 1 {
        email(c, ui);
        return;
    }

    ui.collapsing(if st.form.id == 0 { "Nuova comunicazione" } else { "Modifica comunicazione" }, |ui| {
        egui::Grid::new("g_com").num_columns(2).show(ui, |ui| {
            ui.label("Tipo");
            scegli_str(ui, "com_tipo", &mut st.form.tipo, &TIPI_COMUNICAZIONE);
            ui.end_row();
            campo(ui, "Titolo *", &mut st.form.titolo);
            campo(ui, "Data evento (GG/MM/AAAA)", &mut st.data_evento);
            campo_lungo(ui, "Testo", &mut st.form.testo, 6);
        });
        ui.horizontal(|ui| {
            if ui.button("Salva bozza").clicked() {
                let evento = if st.data_evento.trim().is_empty() {
                    Some(None)
                } else {
                    data_iso(&st.data_evento).map(Some)
                };
                match evento {
                    None => c.errore("Data evento non valida (usa GG/MM/AAAA)"),
                    Some(ev) => {
                        st.form.condominio_id = c.cond;
                        st.form.data_evento = ev;
                        let r = c.db.salva_comunicazione(&st.form);
                        if c.esito(r, "Comunicazione salvata").is_some() {
                            st.form = Comunicazione { tipo: "avviso".into(), ..Default::default() };
                            st.data_evento.clear();
                        }
                    }
                }
            }
            if st.form.id != 0 && ui.button("Annulla modifica").clicked() {
                st.form = Comunicazione { tipo: "avviso".into(), ..Default::default() };
                st.data_evento.clear();
            }
        });
    });

    ui.separator();
    ui.checkbox(&mut st.invia_email, "Alla pubblicazione accoda anche l'e-mail ai condòmini con indirizzo");
    let elenco = c.db.elenco_comunicazioni(c.cond).unwrap_or_default();
    if elenco.is_empty() {
        ui.label("Nessuna comunicazione.");
    }
    for x in elenco {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("[{}]", x.tipo)).small());
                ui.label(RichText::new(&x.titolo).strong());
                if x.pubblicata {
                    ui.colored_label(VERDE, "pubblicata");
                } else {
                    ui.colored_label(ARANCIO, "bozza");
                }
                if let Some(e) = &x.data_evento {
                    ui.label(format!("· evento: {}", data_it(e)));
                }
            });
            ui.label(&x.testo);
            ui.horizontal(|ui| {
                if !x.pubblicata && mini(ui, "Pubblica") {
                    let r = c.db.pubblica_comunicazione(x.id, st.invia_email);
                    if let Some(n) = r.as_ref().ok().copied() {
                        c.ok(&format!("Pubblicata ({} e-mail accodate)", n));
                    } else if let Err(e) = r {
                        c.err(e);
                    }
                }
                if !x.pubblicata && mini(ui, "Modifica") {
                    st.data_evento = x.data_evento.as_deref().map(data_it).unwrap_or_default();
                    st.form = x.clone();
                }
                if mini(ui, "Elimina") {
                    c.chiedi(&format!("Eliminare «{}»?", x.titolo), Azione::EliminaComunicazione(x.id));
                }
            });
        });
    }
}

fn email(c: &mut Ctx, ui: &mut Ui) {
    let coda = c.db.email_in_coda().unwrap_or(0);
    ui.horizontal(|ui| {
        ui.label(format!("E-mail in coda: {}", coda));
        if ui.button("Invia la coda").clicked() {
            match c.db.invia_coda_email() {
                Ok((ok, ko)) => {
                    if ko == 0 {
                        c.ok(&format!("{} e-mail inviate", ok));
                    } else {
                        c.errore(&format!("{} inviate, {} in errore (vedi registro)", ok, ko));
                        *c.rev += 1;
                    }
                }
                Err(e) => c.err(e),
            }
        }
        if ui.button("Riaccoda gli errori").clicked() {
            let r = c.db.riaccoda_errori_email();
            c.esito(r, "Errori rimessi in coda");
        }
    });
    egui::Grid::new("t_email").striped(true).num_columns(5).show(ui, |ui| {
        for h in ["Creata", "Destinatario", "Oggetto", "Stato", "Dettaglio"] {
            ui.strong(h);
        }
        ui.end_row();
        for e in c.db.elenco_email_log(200).unwrap_or_default() {
            ui.label(&e.creato_il);
            ui.label(&e.destinatario);
            ui.label(&e.oggetto);
            let col = match e.stato.as_str() {
                "inviata" => VERDE,
                "errore" => ROSSO,
                _ => ARANCIO,
            };
            ui.colored_label(col, &e.stato);
            ui.label(match (&e.inviato_il, e.errore.is_empty()) {
                (_, false) => e.errore.clone(),
                (Some(t), _) => format!("inviata {}", t),
                _ => String::new(),
            });
            ui.end_row();
        }
    });
}

#[allow(dead_code)]
impl Stato {
    pub fn imposta_tab(&mut self, t: usize) {
        self.tab = t;
    }
}
