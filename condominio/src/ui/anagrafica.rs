use super::*;
use crate::anagrafica::*;

#[derive(Default)]
pub struct Stato {
    tab: usize,
    persona: Persona,
    unita: Unita,
    unita_sel: Option<i64>,
    tit_persona: Option<i64>,
    tit_ruolo: String,
    tit_quota: String,
    tit_paga: bool,
    filtro: String,
}

pub fn mostra(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    titolo(ui, "Anagrafica");
    ui.horizontal(|ui| {
        if c.schede {
            ui.selectable_value(&mut st.tab, 0, "Persone (proprietari, inquilini)");
        }
        if c.schede {
            ui.selectable_value(&mut st.tab, 1, "Unità immobiliari e titolarità");
        }
    });
    ui.collapsing("Importa anagrafica da CSV (passaggio da un altro gestionale)", |ui| {
        ui.label("Colonne riconosciute: scala, piano, interno, tipo_unita, cognome, nome, codice_fiscale, email, telefono, ruolo, quota, paga_spese e una colonna mill_<CODICE> per ogni tabella millesimale. Più righe con stessa scala+interno = più titolari della stessa unità.");
        if ui.button("Scegli file CSV…").clicked() {
            if let Some(p) = rfd::FileDialog::new().add_filter("CSV / testo", &["csv", "txt"]).pick_file() {
                match std::fs::read(&p).map_err(anyhow::Error::from).and_then(|b| c.db.importa_anagrafica_csv(c.cond, &String::from_utf8_lossy(&b))) {
                    Ok(e) => {
                        c.ok(&format!("Importate {} unità, {} persone, {} titolarità, {} tabelle ({} avvisi)", e.unita_create, e.persone_create, e.titolarita, e.tabelle_create, e.avvisi.len()));
                        if !e.avvisi.is_empty() {
                            c.errore(&format!("Avvisi: {}", e.avvisi.iter().take(4).cloned().collect::<Vec<_>>().join(" | ")));
                        }
                    }
                    Err(e) => c.err(e),
                }
            }
        }
    });
    ui.separator();
    if st.tab == 0 {
        persone(c, st, ui);
    } else {
        unita(c, st, ui);
    }
}

fn persone(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    ui.group(|ui| {
        ui.label(RichText::new(if st.persona.id == 0 { "Nuova persona" } else { "Modifica persona" }).strong());
        egui::Grid::new("g_persona").num_columns(4).show(ui, |ui| {
            ui.label("Tipo");
            scegli_str(ui, "p_tipo", &mut st.persona.tipo, &["fisica", "giuridica"]);
            ui.label("Cognome / Rag. sociale *");
            ui.add(egui::TextEdit::singleline(&mut st.persona.cognome).desired_width(200.0));
            ui.end_row();
            ui.label("Nome");
            ui.add(egui::TextEdit::singleline(&mut st.persona.nome).desired_width(200.0));
            ui.label("Codice fiscale / P.IVA");
            ui.add(egui::TextEdit::singleline(&mut st.persona.codice_fiscale).desired_width(200.0));
            ui.end_row();
            ui.label("E-mail");
            ui.add(egui::TextEdit::singleline(&mut st.persona.email).desired_width(200.0));
            ui.label("Telefono");
            ui.add(egui::TextEdit::singleline(&mut st.persona.telefono).desired_width(200.0));
            ui.end_row();
            ui.label("Indirizzo");
            ui.add(egui::TextEdit::singleline(&mut st.persona.indirizzo).desired_width(200.0));
            ui.label("Note");
            ui.add(egui::TextEdit::singleline(&mut st.persona.note).desired_width(200.0));
            ui.end_row();
        });
        ui.horizontal(|ui| {
            if ui.button("Salva").clicked() {
                if st.persona.tipo.is_empty() {
                    st.persona.tipo = "fisica".into();
                }
                let r = c.db.salva_persona(&st.persona);
                if c.esito(r, "Persona salvata").is_some() {
                    st.persona = Persona::default();
                }
            }
            if st.persona.id != 0 && ui.button("Annulla modifica").clicked() {
                st.persona = Persona::default();
            }
        });
    });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label("Cerca:");
        ui.text_edit_singleline(&mut st.filtro);
    });
    let f = st.filtro.to_lowercase();
    let lista = c.db.elenco_persone().unwrap_or_default();
    egui::Grid::new("t_persone").striped(true).num_columns(6).show(ui, |ui| {
        for h in ["Nominativo", "Cod. fiscale", "E-mail", "Telefono", "Unità", ""] {
            ui.strong(h);
        }
        ui.end_row();
        for p in lista.iter().filter(|p| f.is_empty() || p.nome_completo().to_lowercase().contains(&f) || p.codice_fiscale.to_lowercase().contains(&f)) {
            ui.label(p.nome_completo());
            ui.label(&p.codice_fiscale);
            ui.label(&p.email);
            ui.label(&p.telefono);
            let n: i64 = c.db.conn.query_row("SELECT COUNT(*) FROM titolarita WHERE persona_id=?1", [p.id], |r| r.get(0)).unwrap_or(0);
            ui.label(n.to_string());
            ui.horizontal(|ui| {
                if mini(ui, "Modifica") {
                    st.persona = p.clone();
                }
                if mini(ui, "Elimina") {
                    c.chiedi(&format!("Eliminare {}?", p.nome_completo()), Azione::EliminaPersona(p.id));
                }
            });
            ui.end_row();
        }
    });
}

fn unita(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    ui.group(|ui| {
        ui.label(RichText::new(if st.unita.id == 0 { "Nuova unità immobiliare" } else { "Modifica unità" }).strong());
        egui::Grid::new("g_unita").num_columns(6).show(ui, |ui| {
            ui.label("Scala");
            ui.add(egui::TextEdit::singleline(&mut st.unita.scala).desired_width(60.0));
            ui.label("Piano");
            ui.add(egui::TextEdit::singleline(&mut st.unita.piano).desired_width(60.0));
            ui.label("Interno");
            ui.add(egui::TextEdit::singleline(&mut st.unita.interno).desired_width(60.0));
            ui.end_row();
            ui.label("Tipo");
            if st.unita.tipo.is_empty() {
                st.unita.tipo = "appartamento".into();
            }
            scegli_str(ui, "u_tipo", &mut st.unita.tipo, &TIPI_UNITA);
            ui.label("Dati catastali");
            ui.add(egui::TextEdit::singleline(&mut st.unita.dati_catastali).desired_width(160.0));
            ui.end_row();
        });
        ui.horizontal(|ui| {
            if ui.button("Salva unità").clicked() {
                st.unita.condominio_id = c.cond;
                let r = c.db.salva_unita(&st.unita);
                if c.esito(r, "Unità salvata").is_some() {
                    st.unita = Unita::default();
                }
            }
            if st.unita.id != 0 && ui.button("Annulla modifica").clicked() {
                st.unita = Unita::default();
            }
        });
    });
    ui.add_space(6.0);

    let lista = c.db.elenco_unita(c.cond).unwrap_or_default();
    ui.columns(2, |cols| {
        // elenco unità
        let ui = &mut cols[0];
        ui.label(RichText::new("Unità").strong());
        egui::Grid::new("t_unita").striped(true).num_columns(4).show(ui, |ui| {
            for h in ["Unità", "Titolari", "", ""] {
                ui.strong(h);
            }
            ui.end_row();
            for u in &lista {
                if ui.selectable_label(st.unita_sel == Some(u.id), u.etichetta()).clicked() {
                    st.unita_sel = Some(u.id);
                }
                let t = c.db.elenco_titolari(u.id).unwrap_or_default();
                let nomi: Vec<String> = t.iter().map(|x| x.persona.clone()).collect();
                ui.label(if nomi.is_empty() { "— nessuno —".to_string() } else { nomi.join(", ") });
                if mini(ui, "Modifica") {
                    st.unita = u.clone();
                }
                if mini(ui, "Elimina") {
                    c.chiedi(&format!("Eliminare l'unità {}?", u.etichetta()), Azione::EliminaUnita(u.id));
                }
                ui.end_row();
            }
        });

        // titolarità dell'unità selezionata
        let ui = &mut cols[1];
        let Some(sel) = st.unita_sel.and_then(|s| lista.iter().find(|u| u.id == s)) else {
            ui.label("Seleziona un'unità per gestirne i titolari.");
            return;
        };
        ui.label(RichText::new(format!("Titolari – {}", sel.etichetta())).strong());
        let tit = c.db.elenco_titolari(sel.id).unwrap_or_default();
        egui::Grid::new("t_tit").striped(true).num_columns(5).show(ui, |ui| {
            for h in ["Persona", "Ruolo", "Quota", "Paga spese", ""] {
                ui.strong(h);
            }
            ui.end_row();
            for t in &tit {
                ui.label(&t.persona);
                ui.label(&t.ruolo);
                ui.label(format!("{:.2}%", t.quota_ppm as f64 / 10_000.0));
                ui.label(if t.paga_spese { "sì" } else { "" });
                if mini(ui, "Rimuovi") {
                    c.chiedi(&format!("Rimuovere {} dall'unità?", t.persona), Azione::RimuoviTitolarita(t.id));
                }
                ui.end_row();
            }
        });
        let paganti = c.db.pagatori_unita(sel.id).unwrap_or_default();
        let somma: i64 = paganti.iter().map(|x| x.1).sum();
        if !paganti.is_empty() && somma != 1_000_000 {
            ui.colored_label(ARANCIO, format!("Le quote dei paganti sommano {:.2}% (dovrebbero fare 100%)", somma as f64 / 10_000.0));
        }
        ui.separator();
        ui.label("Aggiungi titolare (multi-proprietario: ripeti con quote pro-quota):");
        let persone: Vec<(i64, String)> = c.db.elenco_persone().unwrap_or_default().iter().map(|p| (p.id, etichetta_persona(p))).collect();
        if st.tit_ruolo.is_empty() {
            st.tit_ruolo = "proprietario".into();
        }
        if st.tit_quota.is_empty() {
            st.tit_quota = "100".into();
        }
        egui::Grid::new("g_addtit").num_columns(2).show(ui, |ui| {
            ui.label("Persona");
            scegli(ui, "tit_p", &mut st.tit_persona, &persone, "");
            ui.end_row();
            ui.label("Ruolo");
            scegli_str(ui, "tit_r", &mut st.tit_ruolo, &RUOLI);
            ui.end_row();
            ui.label("Quota %");
            ui.add(egui::TextEdit::singleline(&mut st.tit_quota).desired_width(80.0));
            ui.end_row();
            ui.label("");
            ui.checkbox(&mut st.tit_paga, "Paga lui le spese (es. inquilino)");
            ui.end_row();
        });
        if ui.button("Aggiungi titolare").clicked() {
            match (st.tit_persona, parse_percentuale_ppm(&st.tit_quota)) {
                (Some(p), Some(q)) => {
                    let r = c.db.aggiungi_titolarita(sel.id, p, &st.tit_ruolo, q, st.tit_paga);
                    c.esito(r, "Titolare aggiunto");
                }
                (None, _) => c.errore("Scegli una persona"),
                (_, None) => c.errore("Quota non valida (es. 50 oppure 33,33)"),
            }
        }
    });
}

#[allow(dead_code)]
impl Stato {
    pub fn imposta_tab(&mut self, t: usize) {
        self.tab = t;
    }
}
