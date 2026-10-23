use super::*;
use crate::mora::{format_fixed_ppm, RataScaduta, Sollecito};

pub struct Stato {
    tab: usize,
    giorni_minimi: String,
    soglia: String,
    termine: String,
    anteprima: Option<(usize, String, String)>, // (indice, oggetto, corpo)
    data_atto: String,
    anno_tasso: String,
    tasso: String,
    cache_scadute: Cache<Vec<RataScaduta>>,
    cache_solleciti: Cache<Vec<Sollecito>>,
}

impl Default for Stato {
    fn default() -> Self {
        Stato {
            tab: 0,
            giorni_minimi: "15".into(),
            soglia: "0,00".into(),
            termine: "15".into(),
            anteprima: None,
            data_atto: data_it(&oggi_iso()),
            anno_tasso: oggi().format("%Y").to_string(),
            tasso: String::new(),
            cache_scadute: Cache::default(),
            cache_solleciti: Cache::default(),
        }
    }
}

pub fn mostra(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    titolo(ui, "Morosità, interessi di mora e solleciti");
    ui.horizontal(|ui| {
        if c.schede {
            ui.selectable_value(&mut st.tab, 0, "Rate scadute");
        }
        if c.schede {
            ui.selectable_value(&mut st.tab, 1, "Solleciti da inviare");
        }
        if c.schede {
            ui.selectable_value(&mut st.tab, 2, "Storico solleciti");
        }
        if c.schede {
            ui.selectable_value(&mut st.tab, 3, "Tasso legale");
        }
    });
    ui.separator();
    match st.tab {
        0 => scadute(c, st, ui),
        1 => solleciti(c, st, ui),
        2 => storico(c, ui),
        _ => tassi(c, st, ui),
    }
}

fn scadute(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    let oggi = oggi();
    let (db, cond) = (c.db, c.cond);
    let v: Vec<RataScaduta> = st.cache_scadute.get(*c.rev, &oggi.to_string(), || db.rate_scadute(cond, oggi, None).unwrap_or_default()).clone();
    ui.horizontal(|ui| {
        let cap: Cents = v.iter().map(|r| r.residuo).sum();
        let int: Cents = v.iter().map(|r| r.interessi).sum();
        ui.label(RichText::new(format!("Capitale scaduto: {}  ·  interessi di mora: {}  ·  totale: {}", format_eur(cap), format_eur(int), format_eur(cap + int))).strong());
        if ui.button("Stampa").clicked() {
            match c.db.html_morosita(c.cond, oggi) {
                Ok(h) => apri_html(c, "morosita", &h),
                Err(e) => c.err(e),
            }
        }
    });
    if v.is_empty() {
        ui.colored_label(VERDE, "Nessuna rata scaduta e non pagata.");
        return;
    }
    egui::Grid::new("t_scadute").striped(true).num_columns(7).show(ui, |ui| {
        for h in ["Condòmino", "Piano", "Rata", "Scadenza", "Residuo", "Giorni", "Interessi"] {
            ui.strong(h);
        }
        ui.end_row();
        for r in &v {
            ui.label(&r.persona);
            ui.label(&r.piano);
            ui.label(r.numero.to_string());
            ui.label(data_it(&r.scadenza));
            ui.label(format_eur(r.residuo));
            ui.label(RichText::new(r.giorni_ritardo.to_string()).color(if r.giorni_ritardo > 60 { ROSSO } else { ARANCIO }));
            ui.label(format_eur(r.interessi));
            ui.end_row();
        }
    });
}

fn solleciti(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label("Ritardo minimo (giorni)");
        ui.add(egui::TextEdit::singleline(&mut st.giorni_minimi).desired_width(50.0));
        ui.label("Importo minimo (€)");
        ui.add(egui::TextEdit::singleline(&mut st.soglia).desired_width(70.0));
        ui.label("Termine concesso (giorni)");
        ui.add(egui::TextEdit::singleline(&mut st.termine).desired_width(50.0));
    });
    let gm: i64 = st.giorni_minimi.trim().parse().unwrap_or(0).max(0);
    let soglia = parse_eur(&st.soglia).unwrap_or(0).max(0);
    let termine: i64 = st.termine.trim().parse().unwrap_or(15).clamp(1, 90);
    let oggi = oggi();
    let (db, cond) = (c.db, c.cond);
    let chiave = format!("{}|{}|{}", oggi, gm, soglia);
    let lista: Vec<Sollecito> = st.cache_solleciti.get(*c.rev, &chiave, || db.solleciti_da_inviare(cond, oggi, gm, soglia).unwrap_or_default()).clone();
    if lista.is_empty() {
        ui.colored_label(VERDE, "Nessun condòmino da sollecitare con questi criteri.");
        return;
    }
    ui.add_space(4.0);
    ui.label("Per ciascun condòmino puoi generare il testo, accodare l'e-mail, stamparlo come lettera o registrare l'invio di una raccomandata/PEC (che interrompe la prescrizione).");
    ui.horizontal(|ui| {
        ui.label("Data dell'atto interruttivo");
        ui.add(egui::TextEdit::singleline(&mut st.data_atto).desired_width(100.0));
    });
    for (i, s) in lista.iter().enumerate() {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&s.persona).strong());
                ui.label(format!("capitale {} + interessi {} = {}  ·  ritardo massimo {} gg  ·  {} rate", format_eur(s.capitale), format_eur(s.interessi), format_eur(s.totale()), s.giorni_max, s.rate.len()));
            });
            ui.horizontal(|ui| {
                if mini(ui, "Anteprima testo") {
                    match c.db.testo_sollecito(c.cond, s, oggi, termine) {
                        Ok((o, t)) => st.anteprima = Some((i, o, t)),
                        Err(e) => c.err(e),
                    }
                }
                if mini(ui, "Accoda e-mail") {
                    let r = c.db.testo_sollecito(c.cond, s, oggi, termine).and_then(|(o, t)| {
                        c.db.accoda_email(&s.email, &o, &t)?;
                        c.db.registra_sollecito(c.cond, s, "email", &t)
                    });
                    c.esito(r, "Sollecito accodato: invialo da «Bacheca e Comunicazioni → Registro e-mail»");
                }
                if mini(ui, "Stampa lettera") {
                    let r = c.db.testo_sollecito(c.cond, s, oggi, termine).and_then(|(o, t)| {
                        let h = c.db.html_testo(c.cond, &o, &t)?;
                        c.db.registra_sollecito(c.cond, s, "lettera", &t)?;
                        Ok(h)
                    });
                    match r {
                        Ok(h) => apri_html(c, "sollecito", &h),
                        Err(e) => c.err(e),
                    }
                }
                if mini(ui, "Registra raccomandata/PEC (interrompe prescrizione)") {
                    match data_iso(&st.data_atto) {
                        Some(d) => {
                            let r = c.db.testo_sollecito(c.cond, s, oggi, termine).and_then(|(_, t)| {
                                c.db.registra_sollecito(c.cond, s, "raccomandata/PEC", &t)?;
                                c.db.interrompi_prescrizione_sollecito(s, &d)
                            });
                            c.esito(r, "Atto interruttivo registrato su tutte le rate del sollecito");
                        }
                        None => c.errore("Data dell'atto non valida"),
                    }
                }
            });
            if s.email.is_empty() {
                ui.label(RichText::new("Nessun indirizzo e-mail in anagrafica: l'accodamento e-mail non è possibile.").small().color(ARANCIO));
            }
        });
    }
    if let Some((_, oggetto, corpo)) = &st.anteprima {
        ui.separator();
        ui.label(RichText::new(oggetto).strong());
        ui.add(egui::TextEdit::multiline(&mut corpo.clone()).desired_rows(16).desired_width(f32::INFINITY));
        if ui.button("Chiudi anteprima").clicked() {
            st.anteprima = None;
        }
    }
}

fn storico(c: &mut Ctx, ui: &mut Ui) {
    egui::Grid::new("t_storico").striped(true).num_columns(5).show(ui, |ui| {
        for h in ["Data", "Condòmino", "Canale", "Capitale", "Interessi"] {
            ui.strong(h);
        }
        ui.end_row();
        for (d, p, canale, cap, int) in c.db.storico_solleciti(c.cond).unwrap_or_default() {
            ui.label(data_it(&d));
            ui.label(p);
            ui.label(canale);
            ui.label(format_eur(cap));
            ui.label(format_eur(int));
            ui.end_row();
        }
    });
}

fn tassi(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    ui.label("Gli interessi di mora si calcolano giorno per giorno al tasso legale dell'anno (art. 1284 c.c.), sul residuo di ciascuna rata e a partire dal giorno successivo alla scadenza. Verifica i tassi: sono aggiornati ogni anno con decreto ministeriale.");
    egui::Grid::new("g_tasso").num_columns(2).show(ui, |ui| {
        campo(ui, "Anno", &mut st.anno_tasso);
        campo(ui, "Tasso annuo (%)", &mut st.tasso);
    });
    if ui.button("Salva tasso").clicked() {
        match (st.anno_tasso.trim().parse::<i32>(), parse_percentuale_ppm(&st.tasso)) {
            (Ok(a), Some(p)) => {
                let r = c.db.imposta_tasso_legale(a, p);
                c.esito(r, "Tasso salvato");
            }
            _ => c.errore("Anno o percentuale non validi"),
        }
    }
    ui.separator();
    for (anno, ppm) in c.db.elenco_tassi().unwrap_or_default() {
        ui.label(format!("{}: {}%", anno, format_fixed_ppm(ppm)));
    }
}

#[allow(dead_code)]
impl Stato {
    pub fn imposta_tab(&mut self, t: usize) {
        self.tab = t;
    }
}
