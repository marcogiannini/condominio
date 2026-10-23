use super::*;
use crate::assemblee::{Assemblea, Punto, MAGGIORANZE};
use crate::preventivo::TIPI_DELIBERA;
use std::collections::HashMap;

pub struct Stato {
    sel: Option<i64>,
    tab: usize,
    form: Assemblea,
    modifica: bool,
    punto: Punto,
    rapp: HashMap<i64, String>,
    conv: i64,
    presidente: String,
    segretario: String,
}

impl Default for Stato {
    fn default() -> Self {
        Stato {
            sel: None,
            tab: 0,
            form: nuova_vuota(),
            modifica: false,
            punto: nuovo_punto(),
            rapp: HashMap::new(),
            conv: 1,
            presidente: String::new(),
            segretario: String::new(),
        }
    }
}

fn nuova_vuota() -> Assemblea {
    Assemblea { tipo: "ordinaria".into(), modalita: "presenza".into(), ora_prima: "18:00".into(), ora_seconda: "18:00".into(), ..Default::default() }
}

fn nuovo_punto() -> Punto {
    Punto { maggioranza: "ordinaria".into(), tipo_delibera: "altro".into(), ..Default::default() }
}

fn nome_magg(m: &str) -> &str {
    MAGGIORANZE.iter().find(|x| x.0 == m).map(|x| x.1.split(" – ").next().unwrap_or(x.0)).unwrap_or(m)
}

fn colore_stato(s: &str) -> Color32 {
    match s {
        "bozza" => Color32::GRAY,
        "convocata" => ARANCIO,
        _ => VERDE,
    }
}

pub fn mostra(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    titolo(ui, "Assemblee e verbali");

    // ---------------------------------------------------------------- elenco
    ui.horizontal(|ui| {
        if ui.button("+ Nuova assemblea").clicked() {
            st.form = nuova_vuota();
            st.form.condominio_id = c.cond;
            st.form.esercizio_id = c.esercizio;
            st.modifica = true;
            st.sel = None;
        }
    });
    let elenco = c.db.elenco_assemblee(c.cond).unwrap_or_default();
    egui::Grid::new("t_ass").striped(true).num_columns(5).show(ui, |ui| {
        for h in ["Prima convocazione", "Tipo", "Luogo", "Stato", ""] {
            ui.strong(h);
        }
        ui.end_row();
        for a in &elenco {
            ui.label(format!("{} {}", data_it(&a.data_prima), a.ora_prima));
            ui.label(&a.tipo);
            ui.label(&a.luogo);
            ui.colored_label(colore_stato(&a.stato), &a.stato);
            ui.horizontal(|ui| {
                if mini(ui, "Apri") {
                    st.sel = Some(a.id);
                    st.modifica = false;
                    st.tab = 0;
                    st.presidente = a.presidente.clone();
                    st.segretario = a.segretario.clone();
                    st.rapp.clear();
                }
                if a.stato != "verbalizzata" && mini(ui, "Elimina") {
                    c.chiedi("Eliminare l'assemblea con ordine del giorno, presenze e voti?", Azione::EliminaAssemblea(a.id));
                }
            });
            ui.end_row();
        }
    });
    ui.separator();

    if st.modifica {
        modulo(c, st, ui);
        return;
    }
    let Some(id) = st.sel else {
        ui.label("Scegli un'assemblea o creane una nuova.");
        return;
    };
    let Ok(a) = c.db.assemblea(id) else {
        st.sel = None;
        return;
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("Assemblea {} del {}", a.tipo, data_it(&a.data_prima))).size(17.0).strong());
        ui.colored_label(colore_stato(&a.stato), &a.stato);
    });
    ui.horizontal(|ui| {
        for (i, n) in ["Dati", "Ordine del giorno", "Convocazione", "Presenze e quorum", "Votazioni", "Verbale"].iter().enumerate() {
            ui.selectable_value(&mut st.tab, i, *n);
        }
    });
    ui.separator();
    match st.tab {
        0 => dati(c, st, ui, &a),
        1 => odg(c, st, ui, &a),
        2 => convocazione(c, ui, &a),
        3 => presenze(c, st, ui, &a),
        4 => votazioni(c, ui, &a),
        _ => verbale(c, ui, &a),
    }
}

fn modulo(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    let tabelle: Vec<(i64, String)> = c.db.elenco_tabelle(c.cond).unwrap_or_default().iter().map(|t| (t.id, format!("{} – {}", t.codice, t.nome))).collect();
    let esercizi: Vec<(i64, String)> = c.db.elenco_esercizi(c.cond).unwrap_or_default().iter().map(|e| (e.id, e.nome.clone())).collect();
    ui.label(RichText::new(if st.form.id == 0 { "Nuova assemblea" } else { "Modifica assemblea" }).strong());
    egui::Grid::new("g_ass").num_columns(2).show(ui, |ui| {
        ui.label("Tipo");
        scegli_str(ui, "ass_tipo", &mut st.form.tipo, &["ordinaria", "straordinaria"]);
        ui.end_row();
        ui.label("Modalità");
        scegli_str(ui, "ass_mod", &mut st.form.modalita, &["presenza", "online", "mista"]);
        ui.end_row();
        campo(ui, "Luogo / link collegamento", &mut st.form.luogo);
        campo(ui, "1ª convocazione – data (GG/MM/AAAA)", &mut st.form.data_prima);
        campo(ui, "1ª convocazione – ora", &mut st.form.ora_prima);
        let mut seconda = st.form.data_seconda.clone().unwrap_or_default();
        campo(ui, "2ª convocazione – data (GG/MM/AAAA)", &mut seconda);
        st.form.data_seconda = if seconda.trim().is_empty() { None } else { Some(seconda) };
        campo(ui, "2ª convocazione – ora", &mut st.form.ora_seconda);
        ui.label("Tabella per i quorum (proprietà generale)");
        scegli(ui, "ass_tab", &mut st.form.tabella_id, &tabelle, "");
        ui.end_row();
        ui.label("Esercizio collegato");
        scegli(ui, "ass_es", &mut st.form.esercizio_id, &esercizi, "(nessuno)");
        ui.end_row();
    });
    ui.horizontal(|ui| {
        if ui.button("Salva").clicked() {
            st.form.condominio_id = c.cond;
            let r = c.db.salva_assemblea(&st.form);
            if let Some(id) = c.esito(r, "Assemblea salvata") {
                st.sel = Some(id);
                st.modifica = false;
                st.tab = 1;
            }
        }
        if ui.button("Annulla").clicked() {
            st.modifica = false;
        }
    });
}

fn dati(_c: &mut Ctx, st: &mut Stato, ui: &mut Ui, a: &Assemblea) {
    egui::Grid::new("g_ass_dati").num_columns(2).show(ui, |ui| {
        ui.label("Modalità");
        ui.label(&a.modalita);
        ui.end_row();
        ui.label("Luogo");
        ui.label(&a.luogo);
        ui.end_row();
        ui.label("1ª convocazione");
        ui.label(format!("{} ore {}", data_it(&a.data_prima), a.ora_prima));
        ui.end_row();
        ui.label("2ª convocazione");
        ui.label(a.data_seconda.as_deref().map(|d| format!("{} ore {}", data_it(d), a.ora_seconda)).unwrap_or_else(|| "—".into()));
        ui.end_row();
        if a.convocazione_tenuta.is_some() {
            ui.label("Tenuta in");
            ui.label(format!("{}ª convocazione – presidente {}, segretario {}", a.convocazione_tenuta.unwrap_or(1), a.presidente, a.segretario));
            ui.end_row();
        }
    });
    if a.stato != "verbalizzata" && ui.button("Modifica dati").clicked() {
        st.form = Assemblea { data_prima: data_it(&a.data_prima), data_seconda: a.data_seconda.as_deref().map(data_it), ..a.clone() };
        st.modifica = true;
    }
}

fn odg(c: &mut Ctx, st: &mut Stato, ui: &mut Ui, a: &Assemblea) {
    let bloccato = a.stato == "tenuta" || a.stato == "verbalizzata";
    for p in c.db.elenco_punti(a.id).unwrap_or_default() {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{}. {}", p.numero, p.titolo)).strong());
                ui.label(format!("· maggioranza {}", nome_magg(&p.maggioranza)));
                if p.delibera_id.is_some() {
                    ui.colored_label(VERDE, "deliberato");
                }
                if !bloccato {
                    if mini(ui, "Modifica") {
                        st.punto = p.clone();
                    }
                    if mini(ui, "Elimina") {
                        let r = c.db.elimina_punto(p.id);
                        c.esito(r, "Punto eliminato");
                    }
                }
            });
            if !p.descrizione.is_empty() {
                ui.label(&p.descrizione);
            }
        });
    }
    if bloccato {
        ui.label("L'ordine del giorno non è più modificabile.");
        return;
    }
    ui.separator();
    ui.label(RichText::new(if st.punto.id == 0 { "Nuovo punto" } else { "Modifica punto" }).strong());
    egui::Grid::new("g_punto").num_columns(2).show(ui, |ui| {
        campo(ui, "Titolo *", &mut st.punto.titolo);
        campo_lungo(ui, "Descrizione", &mut st.punto.descrizione, 3);
        ui.label("Maggioranza richiesta");
        egui::ComboBox::from_id_salt("pt_magg").selected_text(nome_magg(&st.punto.maggioranza)).width(300.0).show_ui(ui, |ui| {
            for (k, d) in MAGGIORANZE {
                ui.selectable_value(&mut st.punto.maggioranza, k.to_string(), d);
            }
        });
        ui.end_row();
        ui.label("Tipo di delibera");
        scegli_str(ui, "pt_tipo", &mut st.punto.tipo_delibera, &TIPI_DELIBERA);
        ui.end_row();
    });
    ui.horizontal(|ui| {
        if ui.button(if st.punto.id == 0 { "Aggiungi punto" } else { "Salva punto" }).clicked() {
            st.punto.assemblea_id = a.id;
            let r = c.db.salva_punto(&st.punto);
            if c.esito(r, "Punto salvato").is_some() {
                st.punto = nuovo_punto();
            }
        }
        if st.punto.id != 0 && ui.button("Annulla").clicked() {
            st.punto = nuovo_punto();
        }
    });
}

fn convocazione(c: &mut Ctx, ui: &mut Ui, a: &Assemblea) {
    match c.db.testo_convocazione(a.id) {
        Ok(t) => {
            ui.add(egui::TextEdit::multiline(&mut t.clone()).desired_rows(18).desired_width(f32::INFINITY));
            ui.horizontal(|ui| {
                if ui.button("Stampa avviso").clicked() {
                    match c.db.html_testo(c.cond, "Convocazione di assemblea", &t) {
                        Ok(h) => apri_html(c, "convocazione", &h),
                        Err(e) => c.err(e),
                    }
                }
                if a.stato == "bozza" && ui.button("Convoca e prepara avviso in bacheca").clicked() {
                    let r = c.db.convoca_assemblea(a.id, oggi());
                    c.esito(r, "Assemblea convocata: pubblica l'avviso da «Bacheca e Comunicazioni» (con invio e-mail)");
                }
            });
            if a.stato == "bozza" {
                ui.label(RichText::new("L'avviso va comunicato almeno 5 giorni prima della prima convocazione (art. 66 disp. att. c.c.).").small());
            }
        }
        Err(e) => {
            ui.colored_label(ROSSO, e.to_string());
        }
    }
}

fn presenze(c: &mut Ctx, st: &mut Stato, ui: &mut Ui, a: &Assemblea) {
    if a.stato == "bozza" {
        ui.label("Le presenze si registrano dopo la convocazione.");
        return;
    }
    if a.tabella_id.is_none() {
        ui.colored_label(ARANCIO, "Scegli la tabella per i quorum nei dati dell'assemblea.");
        return;
    }
    let modificabile = a.stato == "convocata" || a.stato == "tenuta";
    let presenti: HashMap<i64, crate::assemblee::Presenza> = c.db.elenco_presenze(a.id).unwrap_or_default().into_iter().map(|p| (p.unita_id, p)).collect();
    for conv in [1, 2] {
        if conv == 2 && a.data_seconda.is_none() {
            continue;
        }
        match c.db.quorum_costitutivo(a.id, conv) {
            Ok(q) => {
                let col = if q.costituita { VERDE } else { ARANCIO };
                ui.colored_label(col, format!("{} – {}", if q.costituita { "Costituita" } else { "Non costituita" }, q.spiegazione));
            }
            Err(e) => {
                ui.colored_label(ROSSO, e.to_string());
            }
        }
    }
    ui.separator();
    let tab = a.tabella_id.unwrap_or(0);
    egui::Grid::new("t_pres").striped(true).num_columns(5).show(ui, |ui| {
        for h in ["Presente", "Unità", "Proprietario", "Millesimi", "Rappresentante / delegato"] {
            ui.strong(h);
        }
        ui.end_row();
        for u in c.db.elenco_unita(c.cond).unwrap_or_default() {
            let p = presenti.get(&u.id);
            let mut presente = p.is_some();
            let prima = presente;
            ui.add_enabled_ui(modificabile, |ui| {
                ui.checkbox(&mut presente, "");
            });
            ui.label(u.etichetta());
            let titolari = c.db.elenco_titolari(u.id).unwrap_or_default();
            ui.label(titolari.iter().filter(|t| t.ruolo != "inquilino").map(|t| t.persona.trim().to_string()).collect::<Vec<_>>().join(", "));
            ui.label(format!("{}‰", format_millesimi(c.db.millesimo(tab, u.id).unwrap_or(0))));
            let buf = st.rapp.entry(u.id).or_insert_with(|| p.map(|x| x.rappresentante.clone()).unwrap_or_default());
            ui.add_enabled(modificabile, egui::TextEdit::singleline(buf).hint_text("(di persona)").desired_width(200.0));
            ui.end_row();
            if presente != prima {
                let r = if presente { c.db.imposta_presenza(a.id, u.id, buf) } else { c.db.rimuovi_presenza(a.id, u.id) };
                if let Err(e) = r {
                    c.err(e);
                } else {
                    *c.rev += 1;
                }
            }
        }
    });
    if a.stato == "convocata" {
        ui.separator();
        ui.label(RichText::new("Apri l'assemblea").strong());
        ui.horizontal(|ui| {
            ui.label("Convocazione");
            ui.selectable_value(&mut st.conv, 1, "1ª");
            if a.data_seconda.is_some() {
                ui.selectable_value(&mut st.conv, 2, "2ª");
            }
            ui.label("Presidente");
            ui.add(egui::TextEdit::singleline(&mut st.presidente).desired_width(160.0));
            ui.label("Segretario");
            ui.add(egui::TextEdit::singleline(&mut st.segretario).desired_width(160.0));
            if ui.button("Dichiara aperta l'assemblea").clicked() {
                match c.db.apri_assemblea(a.id, st.conv, &st.presidente, &st.segretario) {
                    Ok(q) => c.ok(&format!("Assemblea aperta. {}", q.spiegazione)),
                    Err(e) => c.err(e),
                }
            }
        });
    }
}

fn votazioni(c: &mut Ctx, ui: &mut Ui, a: &Assemblea) {
    if a.stato != "tenuta" && a.stato != "verbalizzata" {
        ui.label("Si vota dopo l'apertura dell'assemblea (scheda «Presenze e quorum»).");
        return;
    }
    let presenti = c.db.elenco_presenze(a.id).unwrap_or_default();
    for p in c.db.elenco_punti(a.id).unwrap_or_default() {
        let votabile = a.stato == "tenuta" && p.delibera_id.is_none();
        egui::CollapsingHeader::new(format!("{}. {}  ·  {}", p.numero, p.titolo, nome_magg(&p.maggioranza))).id_salt(("punto", p.id)).show(ui, |ui| {
            match c.db.esito_punto(p.id) {
                Ok(e) => {
                    ui.label(format!(
                        "Favorevoli {}‰ ({} condòmini) · Contrari {}‰ ({}) · Astenuti {}‰ ({})  —  presenti {}‰ su {}‰",
                        format_millesimi(e.fav_mill),
                        e.fav_teste,
                        format_millesimi(e.con_mill),
                        e.con_teste,
                        format_millesimi(e.ast_mill),
                        e.ast_teste,
                        format_millesimi(e.presenti_mill),
                        format_millesimi(e.totali_mill)
                    ));
                    ui.colored_label(if e.approvato { VERDE } else { ARANCIO }, format!("{} ({})", if e.approvato { "Maggioranza raggiunta" } else { "Maggioranza non raggiunta" }, e.regola));
                }
                Err(e) => {
                    ui.colored_label(ROSSO, e.to_string());
                }
            }
            let voti: HashMap<i64, String> = c.db.voti_punto(p.id).unwrap_or_default().into_iter().collect();
            if votabile {
                ui.horizontal(|ui| {
                    if mini(ui, "Tutti i presenti favorevoli") {
                        let r = c.db.voto_a_tutti_i_presenti(p.id, "favorevole");
                        c.esito(r, "Voti registrati");
                    }
                    if mini(ui, "Genera delibera") {
                        let r = c.db.approva_punto(p.id);
                        c.esito(r, "Delibera registrata (collegata al verbale)");
                    }
                });
            } else if let Some(d) = p.delibera_id {
                ui.colored_label(VERDE, format!("Delibera n. {} registrata", d));
            }
            for pr in &presenti {
                ui.horizontal(|ui| {
                    ui.label(format!("{} – {}", pr.unita, pr.titolare));
                    let attuale = voti.get(&pr.unita_id).cloned();
                    for (v, n) in [("favorevole", "Favorevole"), ("contrario", "Contrario"), ("astenuto", "Astenuto")] {
                        if ui.add_enabled(votabile, egui::SelectableLabel::new(attuale.as_deref() == Some(v), n)).clicked() {
                            let r = c.db.registra_voto(p.id, pr.unita_id, v);
                            c.esito(r, "Voto registrato");
                        }
                    }
                    if attuale.is_some() && votabile && mini(ui, "Togli") {
                        let r = c.db.rimuovi_voto(p.id, pr.unita_id);
                        c.esito(r, "Voto rimosso");
                    }
                });
            }
        });
    }
}

fn verbale(c: &mut Ctx, ui: &mut Ui, a: &Assemblea) {
    match c.db.verbale_testo(a.id) {
        Ok(t) => {
            ui.add(egui::TextEdit::multiline(&mut t.clone()).desired_rows(24).desired_width(f32::INFINITY));
            ui.horizontal(|ui| {
                if ui.button("Stampa verbale").clicked() {
                    match c.db.html_testo(c.cond, "Verbale di assemblea", &t) {
                        Ok(h) => apri_html(c, "verbale", &h),
                        Err(e) => c.err(e),
                    }
                }
                if a.stato == "tenuta" && ui.button("Verbalizza e chiudi (non più modificabile)").clicked() {
                    let r = c.db.verbalizza_assemblea(a.id);
                    c.esito(r, "Assemblea verbalizzata");
                }
            });
        }
        Err(e) => {
            ui.label(e.to_string());
        }
    }
}

#[cfg(test)]
impl Stato {
    pub fn tab_prova(&mut self, t: usize) {
        self.tab = t;
    }
    pub fn sel_prova(&mut self, id: i64) {
        self.sel = Some(id);
    }
}
