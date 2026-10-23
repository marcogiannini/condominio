use super::*;
use crate::pratiche::{stati_pratica, Pratica, TIPI_PRATICA};

pub struct Stato {
    filtro: String, // "" = tutti
    solo_aperte: bool,
    form: Pratica,
    importo: String,
    aperta: Option<i64>,
    evento: String,
    ticket: Option<i64>,
    ticket_fornitore: Option<i64>,
}

impl Default for Stato {
    fn default() -> Self {
        Stato {
            filtro: String::new(),
            solo_aperte: true,
            form: Pratica { tipo: "intervento".into(), ..Default::default() },
            importo: String::new(),
            aperta: None,
            evento: String::new(),
            ticket: None,
            ticket_fornitore: None,
        }
    }
}

fn nome_tipo(t: &str) -> &str {
    match t {
        "intervento" => "Intervento fornitore",
        "sinistro" => "Sinistro assicurativo",
        "legale" => "Pratica legale",
        a => a,
    }
}

pub fn mostra(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    titolo(ui, "Interventi, sinistri e pratiche legali");
    let fornitori: Vec<(i64, String)> = c.db.elenco_fornitori(c.cond).unwrap_or_default().iter().map(|f| (f.id, f.ragione_sociale.clone())).collect();
    let unita: Vec<(i64, String)> = c.db.elenco_unita(c.cond).unwrap_or_default().iter().map(|u| (u.id, u.etichetta())).collect();

    ui.collapsing("Nuova pratica", |ui| {
        egui::Grid::new("g_prat").num_columns(2).show(ui, |ui| {
            ui.label("Tipo");
            egui::ComboBox::from_id_salt("pr_tipo").selected_text(nome_tipo(&st.form.tipo)).show_ui(ui, |ui| {
                for t in TIPI_PRATICA {
                    ui.selectable_value(&mut st.form.tipo, t.to_string(), nome_tipo(t));
                }
            });
            ui.end_row();
            campo(ui, "Titolo *", &mut st.form.titolo);
            campo_lungo(ui, "Descrizione", &mut st.form.descrizione, 3);
            ui.label("Fornitore");
            scegli(ui, "pr_forn", &mut st.form.fornitore_id, &fornitori, "(nessuno)");
            ui.end_row();
            ui.label("Unità interessata");
            scegli(ui, "pr_unita", &mut st.form.unita_id, &unita, "(parti comuni)");
            ui.end_row();
            campo(ui, "N. polizza / sinistro / causa", &mut st.form.riferimento);
            campo(ui, "Controparte / compagnia", &mut st.form.controparte);
            campo(ui, "Importo (€)", &mut st.importo);
        });
        if ui.button("Apri pratica").clicked() {
            st.form.condominio_id = c.cond;
            st.form.importo = parse_eur(&st.importo).unwrap_or(0);
            let r = c.db.salva_pratica(&st.form);
            if c.esito(r, "Pratica aperta").is_some() {
                st.form = Pratica { tipo: st.form.tipo.clone(), ..Default::default() };
                st.importo.clear();
            }
        }
    });

    ui.collapsing("Trasforma una segnalazione guasto in intervento", |ui| {
        let tickets: Vec<(i64, String)> = c.db.elenco_ticket(c.cond, None, true).unwrap_or_default().iter().map(|t| (t.id, format!("{} ({})", t.titolo, t.stato))).collect();
        ui.horizontal(|ui| {
            scegli(ui, "pr_ticket", &mut st.ticket, &tickets, "");
            scegli(ui, "pr_tk_forn", &mut st.ticket_fornitore, &fornitori, "(fornitore da assegnare)");
            if ui.button("Crea intervento").clicked() {
                match st.ticket {
                    Some(t) => {
                        let r = c.db.intervento_da_ticket(t, st.ticket_fornitore);
                        c.esito(r, "Intervento creato dalla segnalazione");
                    }
                    None => c.errore("Scegli la segnalazione"),
                }
            }
        });
    });

    ui.separator();
    ui.horizontal(|ui| {
        for (v, n) in [("", "Tutte"), ("intervento", "Interventi"), ("sinistro", "Sinistri"), ("legale", "Legali")] {
            if ui.selectable_label(st.filtro == v, n).clicked() {
                st.filtro = v.to_string();
            }
        }
        ui.checkbox(&mut st.solo_aperte, "Solo aperte");
    });
    let tipo = if st.filtro.is_empty() { None } else { Some(st.filtro.as_str()) };
    let lista = c.db.elenco_pratiche(c.cond, tipo, st.solo_aperte).unwrap_or_default();
    if lista.is_empty() {
        ui.label("Nessuna pratica.");
    }
    for p in lista {
        let aperta = st.aperta == Some(p.id);
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("[{}]", nome_tipo(&p.tipo))).small());
                ui.label(RichText::new(&p.titolo).strong());
                ui.label(format!("· {}", p.stato.replace('_', " ")));
                if p.importo != 0 {
                    ui.label(format!("· {}", format_eur(p.importo)));
                }
                ui.label(RichText::new(format!("aperta il {}", data_it(&p.data_apertura))).small());
                if mini(ui, if aperta { "Chiudi dettaglio" } else { "Dettaglio" }) {
                    st.aperta = if aperta { None } else { Some(p.id) };
                    st.evento.clear();
                }
            });
            if aperta {
                if !p.descrizione.is_empty() {
                    ui.label(&p.descrizione);
                }
                if !p.riferimento.is_empty() || !p.controparte.is_empty() {
                    ui.label(format!("Rif.: {}  ·  Controparte: {}", p.riferimento, p.controparte));
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label("Stato:");
                    for s in stati_pratica(&p.tipo) {
                        if ui.selectable_label(p.stato == *s, s.replace('_', " ")).clicked() && p.stato != *s {
                            let r = c.db.cambia_stato_pratica(p.id, s);
                            c.esito(r, "Stato aggiornato");
                        }
                    }
                });
                ui.separator();
                for (quando, testo) in c.db.eventi_pratica(p.id).unwrap_or_default() {
                    ui.label(format!("{} — {}", quando, testo));
                }
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut st.evento).hint_text("Annota un evento (es. sopralluogo, denuncia, udienza)…").desired_width(380.0));
                    if ui.button("Aggiungi").clicked() && !st.evento.trim().is_empty() {
                        let r = c.db.aggiungi_evento_pratica(p.id, &st.evento);
                        if c.esito(r, "Evento registrato").is_some() {
                            st.evento.clear();
                        }
                    }
                });
                if mini(ui, "Elimina pratica") {
                    c.chiedi(&format!("Eliminare la pratica «{}»?", p.titolo), Azione::EliminaPratica(p.id));
                }
            }
        });
    }
}
