use super::elenco::*;
use super::*;
use crate::fornitori::*;

pub struct Stato {
    tab: usize,
    form: Fornitore,
    f_fornitore: Option<i64>,
    f_voce: Option<i64>,
    f_numero: String,
    f_data: String,
    f_descr: String,
    f_imponibile: String,
    f_iva: String,
    pag_data: String,
    pag_conto: Option<i64>,
    sel: Option<i64>,
    tab_f: usize,
    nuovo: bool,
    cerca: String,
}

impl Default for Stato {
    fn default() -> Self {
        Stato {
            tab: 0,
            form: Fornitore { regime_ritenuta: "nessuna".into(), ..Default::default() },
            f_fornitore: None,
            f_voce: None,
            f_numero: String::new(),
            f_data: data_it(&oggi_iso()),
            f_descr: String::new(),
            f_imponibile: String::new(),
            f_iva: "0,00".into(),
            pag_data: data_it(&oggi_iso()),
            pag_conto: None,
            sel: None,
            tab_f: 0,
            nuovo: false,
            cerca: String::new(),
        }
    }
}

fn nome_regime(r: &str) -> &'static str {
    match r {
        "appalti4" => "Ritenuta 4% (appalti/servizi)",
        "professionista20" => "Ritenuta 20% (professionisti)",
        _ => "Nessuna ritenuta",
    }
}

pub fn mostra(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    if c.schede {
        titolo(ui, "Fornitori, Fatture, Ritenute d'acconto");
    }
    ui.horizontal(|ui| {
        if c.schede {
            ui.selectable_value(&mut st.tab, 0, "Fornitori");
        }
        if c.schede {
            ui.selectable_value(&mut st.tab, 1, "Fatture");
        }
        if c.schede {
            ui.selectable_value(&mut st.tab, 2, "Ritenute da versare (F24)");
        }
    });
    if c.schede {
        ui.separator();
    }
    let conti: Vec<(i64, String)> = c.db.elenco_conti_correnti(c.cond).unwrap_or_default().into_iter().map(|(i, n, _)| (i, n)).collect();
    if st.pag_conto.is_none() {
        st.pag_conto = conti.first().map(|x| x.0);
    }
    match st.tab {
        0 => fornitori(c, st, ui),
        1 => fatture(c, st, ui, &conti),
        _ => ritenute(c, st, ui, &conti),
    }
}

const COLONNE_F: [Col; 5] = [col("Denominazione", 0.0), col("CF / P.IVA", 150.0), col("E-mail", 220.0), col("Fisco - Codice tributo", 160.0), col("IBAN", 240.0)];

fn codice_tributo(f: &Fornitore) -> String {
    match f.regime_ritenuta.as_str() {
        "appalti4" => "1019 (4%)".into(),
        "professionista20" => if f.soggetto_ires { "1250 (20%)".into() } else { "1040 (20%)".into() },
        _ => String::new(),
    }
}

fn fornitori(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    let tutti = c.db.elenco_fornitori(c.cond).unwrap_or_default();
    let f = st.cerca.to_lowercase();
    let lista: Vec<&Fornitore> = tutti.iter().filter(|x| f.is_empty() || x.ragione_sociale.to_lowercase().contains(&f) || x.cf_piva.to_lowercase().contains(&f) || x.email.to_lowercase().contains(&f)).collect();
    let righe: Vec<Riga> = lista.iter().map(|x| Riga::new(vec![x.ragione_sociale.clone().into(), x.cf_piva.clone().into(), x.email.clone().into(), codice_tributo(x).into(), x.iban.clone().into()])).collect();
    let selezionata = st.sel.and_then(|id| lista.iter().position(|x| x.id == id));
    if st.sel.is_none() && !st.nuovo {
        st.sel = lista.first().map(|x| x.id);
    }
    if !st.nuovo && st.sel.unwrap_or(0) != st.form.id {
        st.form = st.sel.and_then(|id| tutti.iter().find(|x| x.id == id)).cloned().unwrap_or(Fornitore { regime_ritenuta: "nessuna".into(), ..Default::default() });
    }

    barra_azioni(ui, "az_forn", |ui| {
        if azione(ui, "📄", BLU, "Nuovo") {
            st.nuovo = true;
            st.sel = None;
            st.form = Fornitore { regime_ritenuta: "nessuna".into(), ..Default::default() };
            st.tab_f = 0;
        }
        if azione(ui, "📗", VERDE_ICONA, "Excel") {
            esporta_csv(c, "fornitori", &COLONNE_F, &righe);
        }
        if azione(ui, "🧾", ARANCIO_ICONA, "Fatture") {
            *c.app = Some(AzioneApp::Vai(Sezione::Fornitori, Some(1)));
        }
        if azione(ui, "💰", ROSSO_ICONA, "Ritenute da versare") {
            *c.app = Some(AzioneApp::Vai(Sezione::Fornitori, Some(2)));
        }
    });

    pannello_dettaglio(ui, "dett_forn", 440.0, |ui| {
        schede(ui, &mut st.tab_f, &["Dati generali", "Movimenti"]);
        ui.add_space(8.0);
        if st.tab_f == 0 {
            grid_scheda("g_forn").show(ui, |ui| {
                campo_pieno(ui, "Rag. Soc. *", &mut st.form.ragione_sociale);
                campo_pieno(ui, "Cod. fiscale / P.IVA", &mut st.form.cf_piva);
                campo_pieno(ui, "E-mail", &mut st.form.email);
                campo_pieno(ui, "IBAN", &mut st.form.iban);
                ui.label("Ritenuta");
                egui::ComboBox::from_id_salt("forn_reg").selected_text(nome_regime(&st.form.regime_ritenuta)).width(260.0).show_ui(ui, |ui| {
                    for r in REGIMI {
                        ui.selectable_value(&mut st.form.regime_ritenuta, r.to_string(), nome_regime(r));
                    }
                });
                ui.end_row();
                ui.label("");
                ui.checkbox(&mut st.form.soggetto_ires, "Soggetto a IRES (codice tributo 1250 invece di 1040)");
                ui.end_row();
            });
            ui.add_space(4.0);
            if ui.button(if st.form.id == 0 { "Aggiungi fornitore" } else { "Salva modifiche" }).clicked() {
                st.form.condominio_id = c.cond;
                let r = c.db.salva_fornitore(&st.form);
                if let Some(id) = c.esito(r, "Fornitore salvato") {
                    st.sel = Some(id);
                    st.nuovo = false;
                }
            }
        } else if st.form.id == 0 {
            ui.label("Salva il fornitore per vederne i movimenti.");
        } else {
            let fatture: Vec<FatturaRiga> = c.db.elenco_fatture(c.cond, c.esercizio).unwrap_or_default().into_iter().filter(|x| x.fornitore == st.form.ragione_sociale).collect();
            if fatture.is_empty() {
                ui.label("Nessuna fattura in questo esercizio.");
            }
            egui::Grid::new("g_forn_mov").striped(true).num_columns(4).spacing([10.0, 4.0]).show(ui, |ui| {
                for x in &fatture {
                    ui.label(data_it(&x.data));
                    ui.label(format!("{} {}", x.numero, x.descrizione));
                    eur_colorato(ui, -x.totale());
                    ui.label(&x.stato);
                    ui.end_row();
                }
            });
        }
    });

    barra_filtri(ui, Some(&mut st.cerca), |ui| {
        ui.label(format!("Fornitori ({})", lista.len()));
    });
    let esito = elenco(ui, "t_forn", &COLONNE_F, &righe, selezionata, None);
    if let Some(i) = esito.clic {
        st.sel = Some(lista[i].id);
        st.nuovo = false;
    }
}

fn fatture(c: &mut Ctx, st: &mut Stato, ui: &mut Ui, conti: &[(i64, String)]) {
    let Some(es) = c.esercizio else {
        ui.label("Crea o scegli un esercizio in «Preventivo e Piano dei Conti».");
        return;
    };
    let forn = c.db.elenco_fornitori(c.cond).unwrap_or_default();
    let voci_f: Vec<(i64, String)> = forn.iter().map(|f| (f.id, f.ragione_sociale.clone())).collect();
    let voci_s: Vec<(i64, String)> = c.db.elenco_voci(es).unwrap_or_default().iter().map(|v| (v.id, format!("{} {}", v.codice, v.descrizione))).collect();

    ui.collapsing("Importa fatture elettroniche (XML FatturaPA / .p7m)", |ui| {
        ui.label("Crea in automatico i fornitori mancanti (con ritenuta e IBAN, se presenti nel file) e registra le fatture. Le note di credito vanno registrate a mano. Se scegli una voce di spesa, le fatture vi vengono imputate.");
        ui.horizontal(|ui| {
            ui.label("Voce di spesa");
            scegli(ui, "ft_xml_voce", &mut st.f_voce, &voci_s, "(da imputare dopo)");
            if ui.button("Scegli file…").clicked() {
                if let Some(files) = rfd::FileDialog::new().add_filter("Fattura elettronica", &["xml", "p7m"]).pick_files() {
                    let (mut ok, mut skip, mut nuovi) = (0, 0, 0);
                    let mut msg: Vec<String> = vec![];
                    for f in files {
                        let nome = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        match std::fs::read(&f).map_err(anyhow::Error::from).and_then(|b| c.db.importa_fatture_xml(c.cond, es, &b, st.f_voce, &nome)) {
                            Ok(e) => {
                                ok += e.importate.len();
                                skip += e.saltate.len();
                                nuovi += e.nuovi_fornitori;
                                msg.extend(e.saltate.into_iter().chain(e.avvisi));
                            }
                            Err(e) => msg.push(format!("{}: {}", nome, e)),
                        }
                    }
                    *c.rev += 1;
                    c.ok(&format!("Importate {} fatture ({} nuovi fornitori, {} saltate)", ok, nuovi, skip));
                    if !msg.is_empty() {
                        c.errore(&format!("{} fatture non importate o con avvisi: {}", msg.len(), msg.iter().take(3).cloned().collect::<Vec<_>>().join(" | ")));
                    }
                }
            }
        });
    });
    ui.collapsing("Registra fattura passiva", |ui| {
        egui::Grid::new("g_fatt").num_columns(2).show(ui, |ui| {
            ui.label("Fornitore *");
            scegli(ui, "ft_forn", &mut st.f_fornitore, &voci_f, "");
            ui.end_row();
            ui.label("Voce di spesa");
            scegli(ui, "ft_voce", &mut st.f_voce, &voci_s, "(nessuna)");
            ui.end_row();
            campo(ui, "Numero *", &mut st.f_numero);
            campo(ui, "Data (GG/MM/AAAA)", &mut st.f_data);
            campo(ui, "Descrizione", &mut st.f_descr);
            campo(ui, "Imponibile (€)", &mut st.f_imponibile);
            campo(ui, "IVA (€)", &mut st.f_iva);
        });
        if let (Some(fid), Some(imp)) = (st.f_fornitore, parse_eur(&st.f_imponibile)) {
            if let Some(f) = forn.iter().find(|f| f.id == fid) {
                let r = calcola_ritenuta(&f.regime_ritenuta, imp);
                if r > 0 {
                    ui.label(format!("Ritenuta d'acconto: {} · netto da pagare al fornitore: {}", format_eur(r), format_eur(imp + parse_eur(&st.f_iva).unwrap_or(0) - r)));
                }
            }
        }
        if ui.button("Registra fattura").clicked() {
            let (Some(fid), Some(data), Some(imp), Some(iva)) = (st.f_fornitore, data_iso(&st.f_data), parse_eur(&st.f_imponibile), parse_eur(&st.f_iva)) else {
                c.errore("Controlla fornitore, data (GG/MM/AAAA) e importi");
                return;
            };
            let n = NuovaFattura {
                condominio_id: c.cond,
                esercizio_id: es,
                fornitore_id: fid,
                voce_id: st.f_voce,
                numero: st.f_numero.clone(),
                data,
                descrizione: st.f_descr.clone(),
                imponibile: imp,
                iva,
            };
            let r = c.db.registra_fattura(&n);
            if c.esito(r, "Fattura registrata").is_some() {
                st.f_numero.clear();
                st.f_descr.clear();
                st.f_imponibile.clear();
            }
        }
    });

    ui.separator();
    ui.horizontal(|ui| {
        ui.label("Data pagamento");
        ui.add(egui::TextEdit::singleline(&mut st.pag_data).desired_width(100.0));
        ui.label("Conto");
        scegli(ui, "ft_pag_cc", &mut st.pag_conto, conti, "");
    });
    egui::Grid::new("t_fatt").striped(true).num_columns(9).show(ui, |ui| {
        for h in ["Data", "Fornitore", "N.", "Imponibile", "IVA", "Ritenuta", "Netto", "Stato", "Azioni"] {
            ui.strong(h);
        }
        ui.end_row();
        for f in c.db.elenco_fatture(c.cond, Some(es)).unwrap_or_default() {
            ui.label(data_it(&f.data));
            ui.label(&f.fornitore);
            ui.label(&f.numero);
            ui.label(format_eur(f.imponibile));
            ui.label(format_eur(f.iva));
            ui.label(format_eur(f.ritenuta));
            ui.label(format_eur(f.netto_a_pagare()));
            let col = match f.stato.as_str() {
                "pagata" => VERDE,
                "stornata" => Color32::GRAY,
                _ => ARANCIO,
            };
            ui.colored_label(col, if f.versata { "pagata, ritenuta versata".to_string() } else { f.stato.replace('_', " ") });
            ui.horizontal(|ui| match f.stato.as_str() {
                "da_pagare" => {
                    if mini(ui, "Paga") {
                        match (data_iso(&st.pag_data), st.pag_conto) {
                            (Some(d), Some(cc)) => {
                                let r = c.db.paga_fattura(f.id, &d, cc);
                                c.esito(r, "Fattura pagata");
                            }
                            _ => c.errore("Indica data valida e conto corrente"),
                        }
                    }
                    if mini(ui, "Storna") {
                        c.chiedi(&format!("Stornare la fattura {} di {}?", f.numero, f.fornitore), Azione::StornaFattura(f.id));
                    }
                }
                "pagata"
                    if mini(ui, "Storna pagamento") => {
                        c.chiedi(&format!("Stornare il pagamento della fattura {}?", f.numero), Azione::StornaPagamentoFattura(f.id));
                    }
                _ => {}
            });
            ui.end_row();
        }
    });
}

fn ritenute(c: &mut Ctx, st: &mut Stato, ui: &mut Ui, conti: &[(i64, String)]) {
    ui.label(format!(
        "Le ritenute vanno versate con F24 entro il 16 del mese successivo al pagamento; qui sono raggruppate per codice tributo. Soglia indicativa di rilevanza: {}.",
        format_eur(SOGLIA_RITENUTE_CENTS)
    ));
    ui.horizontal(|ui| {
        ui.label("Data versamento");
        ui.add(egui::TextEdit::singleline(&mut st.pag_data).desired_width(100.0));
        ui.label("Conto");
        scegli(ui, "rit_cc", &mut st.pag_conto, conti, "");
    });
    let gruppi = c.db.ritenute_da_versare(c.cond).unwrap_or_default();
    if gruppi.is_empty() {
        ui.label("Nessuna ritenuta da versare.");
    }
    egui::Grid::new("t_rit").striped(true).num_columns(5).show(ui, |ui| {
        for h in ["Codice tributo", "Scadenza", "Importo", "Fatture", ""] {
            ui.strong(h);
        }
        ui.end_row();
        for g in &gruppi {
            ui.label(&g.codice_tributo);
            let scaduta = parse_data(&g.scadenza).map(|d| d < oggi()).unwrap_or(false);
            let t = RichText::new(data_it(&g.scadenza));
            ui.label(if scaduta { t.color(ROSSO) } else { t });
            ui.label(format_eur(g.importo));
            ui.label(g.fatture.len().to_string());
            if mini(ui, "Registra versamento F24") {
                match (data_iso(&st.pag_data), st.pag_conto) {
                    (Some(d), Some(cc)) => {
                        let r = c.db.versa_ritenute(c.cond, g, &d, cc);
                        c.esito(r, "Versamento registrato");
                    }
                    _ => c.errore("Indica data valida e conto corrente"),
                }
            }
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
