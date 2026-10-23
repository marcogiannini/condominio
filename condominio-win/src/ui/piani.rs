use super::*;
use crate::preventivo::*;

#[derive(Default)]
pub struct Stato {
    nome: String,
    tipo: String,
    n_rate: String,
    prima: String,
    cadenza: String,
    delibera: Option<i64>,
    sel: Option<i64>,
    voce: Option<i64>,
    voce_importo: String,
    anteprima: bool,
    cache_anteprima: Cache<Result<Vec<RataCalc>, String>>,
    cache_rate: Cache<Vec<RataRiga>>,
}

pub fn mostra(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    titolo(ui, "Piani rate e contabilità");
    let Some(es) = c.esercizio else {
        ui.label("Crea o seleziona un esercizio in «Preventivo e Piano dei Conti».");
        return;
    };
    let delibere: Vec<(i64, String)> =
        c.db.elenco_delibere(c.cond).unwrap_or_default().iter().map(|d| (d.id, format!("{} – {} ({})", data_it(&d.data), d.oggetto, d.tipo))).collect();

    if st.tipo.is_empty() {
        st.tipo = "ordinario".into();
        st.n_rate = "4".into();
        st.cadenza = "3".into();
    }
    ui.group(|ui| {
        ui.label(RichText::new("Nuovo piano rate").strong());
        egui::Grid::new("g_piano").num_columns(2).show(ui, |ui| {
            campo(ui, "Nome *", &mut st.nome);
            ui.label("Tipo");
            scegli_str(ui, "p_tipo", &mut st.tipo, &TIPI_PIANO);
            ui.end_row();
            campo(ui, "Numero di rate", &mut st.n_rate);
            campo(ui, "Prima scadenza * (GG/MM/AAAA)", &mut st.prima);
            campo(ui, "Cadenza (mesi)", &mut st.cadenza);
            ui.label("Delibera assembleare *");
            scegli(ui, "p_del", &mut st.delibera, &delibere, "— da collegare prima dell'attivazione —");
            ui.end_row();
        });
        if st.tipo == "straordinario" {
            ui.label("Art. 1135 c.c.: i lavori straordinari richiedono la delibera dell'assemblea che li approva e la costituzione di un fondo speciale.");
        }
        if ui.button("Crea piano (bozza)").clicked() {
            match (st.n_rate.trim().parse::<i64>(), st.cadenza.trim().parse::<i64>()) {
                (Ok(n), Ok(cad)) => {
                    let p = Piano { esercizio_id: es, nome: st.nome.clone(), tipo: st.tipo.clone(), delibera_id: st.delibera, n_rate: n, prima_scadenza: st.prima.clone(), cadenza_mesi: cad, ..Default::default() };
                    let r = c.db.crea_piano(&p);
                    if let Some(id) = c.esito(r, "Piano creato in bozza") {
                        st.sel = Some(id);
                        st.nome.clear();
                    }
                }
                _ => c.errore("Numero di rate e cadenza devono essere numeri interi"),
            }
        }
    });
    ui.add_space(6.0);

    let piani = c.db.elenco_piani(es).unwrap_or_default();
    egui::Grid::new("t_piani").striped(true).num_columns(5).show(ui, |ui| {
        for h in ["Piano", "Tipo", "Stato", "Totale", "Rate"] {
            ui.strong(h);
        }
        ui.end_row();
        for p in &piani {
            if ui.selectable_label(st.sel == Some(p.id), &p.nome).clicked() {
                st.sel = Some(p.id);
                st.anteprima = false;
            }
            ui.label(&p.tipo);
            let col = match p.stato.as_str() {
                "attivo" => VERDE,
                "annullato" => ROSSO,
                _ => ARANCIO,
            };
            ui.colored_label(col, &p.stato);
            ui.label(format_eur(c.db.totale_piano(p.id).unwrap_or(0)));
            ui.label(format!("{} ogni {} mesi dal {}", p.n_rate, p.cadenza_mesi, data_it(&p.prima_scadenza)));
            ui.end_row();
        }
    });

    let Some(p) = st.sel.and_then(|s| piani.iter().find(|p| p.id == s)).cloned() else { return };
    ui.separator();
    ui.label(RichText::new(format!("Piano «{}» – {}", p.nome, p.stato)).size(16.0).strong());

    // delibera collegata
    let del_txt = p.delibera_id.and_then(|d| delibere.iter().find(|x| x.0 == d)).map(|x| x.1.clone());
    match (&del_txt, p.stato.as_str()) {
        (Some(t), _) => {
            ui.label(format!("Delibera: {}", t));
        }
        (None, _) => {
            ui.colored_label(ROSSO, "Nessuna delibera collegata: l'attivazione è bloccata (delibera assembleare obbligatoria).");
        }
    }
    if p.stato == "bozza" {
        ui.horizontal(|ui| {
            ui.label("Collega delibera:");
            let mut d = p.delibera_id;
            scegli(ui, "p_del_sel", &mut d, &delibere, "— nessuna —");
            if d != p.delibera_id {
                let r = c.db.imposta_delibera_piano(p.id, d);
                c.esito(r, "Delibera collegata");
            }
        });

        // voci
        let voci_es = c.db.elenco_voci(es).unwrap_or_default();
        let foglie: Vec<(i64, String)> = voci_es
            .iter()
            .filter(|v| !voci_es.iter().any(|f| f.parent_id == Some(v.id)))
            .map(|v| (v.id, format!("{} {} ({})", v.codice, v.descrizione, format_eur(v.importo_cents))))
            .collect();
        ui.label(RichText::new("Voci di spesa del piano").strong());
        ui.horizontal(|ui| {
            let prima = st.voce;
            scegli(ui, "pv_voce", &mut st.voce, &foglie, "");
            if prima != st.voce {
                if let Some(v) = st.voce.and_then(|id| voci_es.iter().find(|x| x.id == id)) {
                    st.voce_importo = format_eur(v.importo_cents).trim_end_matches(" €").to_string();
                }
            }
            ui.add(egui::TextEdit::singleline(&mut st.voce_importo).desired_width(100.0).hint_text("importo"));
            if ui.button("Aggiungi / aggiorna").clicked() {
                match (st.voce, parse_eur(&st.voce_importo)) {
                    (Some(v), Some(i)) => {
                        let r = c.db.imposta_voce_piano(p.id, v, i);
                        c.esito(r, "Voce aggiornata nel piano");
                    }
                    _ => c.errore("Scegli una voce e un importo valido"),
                }
            }
        });
        for (vid, nome, imp) in c.db.voci_piano(p.id).unwrap_or_default() {
            ui.horizontal(|ui| {
                ui.label(format!("{}  –  {}", nome, format_eur(imp)));
                if mini(ui, "Rimuovi") {
                    let r = c.db.imposta_voce_piano(p.id, vid, 0);
                    c.esito(r, "Voce rimossa dal piano");
                }
            });
        }
        ui.label(RichText::new(format!("Totale piano: {}", format_eur(c.db.totale_piano(p.id).unwrap_or(0)))).strong());

        ui.horizontal(|ui| {
            if ui.button("Mostra anteprima del riparto").clicked() {
                st.anteprima = true;
            }
            if ui.button(RichText::new("Attiva piano ed emetti le rate").strong()).clicked() {
                let r = c.db.attiva_piano(p.id);
                if let Ok(n) = r.as_ref() {
                    let n = *n;
                    c.ok(&format!("Piano attivato: {} rate emesse e registrate in partita doppia", n));
                } else if let Err(e) = r {
                    c.err(e);
                }
            }
            if ui.button("Elimina bozza").clicked() {
                c.chiedi("Eliminare il piano in bozza?", Azione::EliminaPianoBozza(p.id));
            }
        });
        if st.anteprima {
            let db = c.db;
            let pid = p.id;
            let calc = st.cache_anteprima.get(*c.rev, &pid.to_string(), || db.calcola_rate(pid).map_err(|e| e.to_string()));
            match calc {
                Err(e) => {
                    ui.colored_label(ROSSO, e);
                }
                Ok(rate) => {
                    let totale: Cents = rate.iter().map(|r| r.importo).sum();
                    ui.label(format!("{} rate, totale {}", rate.len(), format_eur(totale)));
                    egui::Grid::new("t_antep").striped(true).num_columns(5).show(ui, |ui| {
                        for h in ["Unità", "Intestatario", "Rata", "Scadenza", "Importo"] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for r in rate.iter() {
                            let u = c.db.elenco_unita(c.cond).unwrap_or_default().into_iter().find(|u| u.id == r.unita_id).map(|u| u.etichetta()).unwrap_or_default();
                            let n: String = c.db.conn.query_row("SELECT cognome||' '||nome FROM persona WHERE id=?1", [r.persona_id], |x| x.get(0)).unwrap_or_default();
                            ui.label(u);
                            ui.label(n.trim());
                            ui.label(r.numero.to_string());
                            ui.label(data_it(&r.scadenza));
                            ui.label(format_eur(r.importo));
                            ui.end_row();
                        }
                    });
                }
            }
        }
    } else {
        if p.stato == "attivo" && ui.button("Annulla piano").clicked() {
            c.chiedi("Annullare il piano? Verrà stornata la scrittura contabile di emissione (solo se non ci sono incassi).", Azione::AnnullaPiano(p.id));
        }
        let db = c.db;
        let pid = p.id;
        let rate = st.cache_rate.get(*c.rev, &pid.to_string(), || db.rate_piano(pid).unwrap_or_default());
        let unita = c.db.elenco_unita(c.cond).unwrap_or_default();
        let totale: Cents = rate.iter().map(|r| r.importo).sum();
        let pagato: Cents = rate.iter().map(|r| r.pagato).sum();
        ui.label(format!("Emesso {} – incassato {} – residuo {}", format_eur(totale), format_eur(pagato), format_eur(totale - pagato)));
        egui::Grid::new("t_rate").striped(true).num_columns(6).show(ui, |ui| {
            for h in ["Unità", "Intestatario", "Rata", "Scadenza", "Importo", "Pagato"] {
                ui.strong(h);
            }
            ui.end_row();
            for r in rate.iter() {
                ui.label(unita.iter().find(|u| u.id == r.unita_id).map(|u| u.etichetta()).unwrap_or_default());
                let n: String = c.db.conn.query_row("SELECT cognome||' '||nome FROM persona WHERE id=?1", [r.persona_id], |x| x.get(0)).unwrap_or_default();
                ui.label(n.trim());
                ui.label(r.numero.to_string());
                ui.label(data_it(&r.scadenza));
                ui.label(format_eur(r.importo));
                let col = if r.pagato >= r.importo { VERDE } else if r.pagato > 0 { ARANCIO } else { ui.visuals().text_color() };
                ui.colored_label(col, format_eur(r.pagato));
                ui.end_row();
            }
        });
    }
}
