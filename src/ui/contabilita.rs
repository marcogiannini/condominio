use super::elenco::*;
use super::*;
use crate::movimenti::{nome_origine, Movimento};

pub struct Stato {
    tab: usize,
    cc_nome: String,
    cc_iban: String,
    g_da: Option<i64>,
    g_a: Option<i64>,
    g_importo: String,
    g_data: String,
    g_descr: String,
    // filtri movimenti
    f_conto: Option<i64>,
    dal: String,
    al: String,
    // movimento manuale
    m_conto: Option<i64>,
    m_data: String,
    m_descr: String,
    m_importo: String,
    m_entrata: bool,
    m_voce: Option<i64>,
    cache: Cache<Vec<Movimento>>,
    sel: Option<usize>,
    nuovo: bool,
    cerca: String,
}

impl Default for Stato {
    fn default() -> Self {
        Stato {
            tab: 0,
            cc_nome: String::new(),
            cc_iban: String::new(),
            g_da: None,
            g_a: None,
            g_importo: String::new(),
            g_data: data_it(&oggi_iso()),
            g_descr: String::new(),
            f_conto: None,
            dal: String::new(),
            al: String::new(),
            m_conto: None,
            m_data: data_it(&oggi_iso()),
            m_descr: String::new(),
            m_importo: String::new(),
            m_entrata: true,
            m_voce: None,
            cache: Cache::default(),
            sel: None,
            nuovo: false,
            cerca: String::new(),
        }
    }
}


pub fn mostra(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    if c.schede {
        titolo(ui, "Conti e Movimenti di gestione");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut st.tab, 0, "Conti correnti");
            ui.selectable_value(&mut st.tab, 1, "Movimenti");
        });
        ui.separator();
    }
    match st.tab {
        0 => {
            if !c.schede {
                ui.add_space(4.0);
            }
            conti(c, st, ui)
        }
        _ => movimenti(c, st, ui),
    }
}

fn conti(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    egui::Grid::new("g_cc").num_columns(2).show(ui, |ui| {
        campo(ui, "Nome conto *", &mut st.cc_nome);
        campo(ui, "IBAN", &mut st.cc_iban);
    });
    if ui.button("Aggiungi conto corrente").clicked() {
        let r = c.db.crea_conto_corrente(c.cond, &st.cc_nome, &st.cc_iban);
        if c.esito(r, "Conto corrente creato").is_some() {
            st.cc_nome.clear();
            st.cc_iban.clear();
        }
    }
    ui.separator();
    let elenco = c.db.elenco_conti_correnti(c.cond).unwrap_or_default();
    egui::Grid::new("t_cc").striped(true).num_columns(3).show(ui, |ui| {
        for h in ["Conto", "IBAN", "Saldo"] {
            ui.strong(h);
        }
        ui.end_row();
        for (id, nome, iban) in &elenco {
            ui.label(nome);
            ui.label(iban);
            eur_colorato(ui, c.db.saldo_conto_corrente(*id).unwrap_or(0));
            ui.end_row();
        }
    });
    ui.label(RichText::new("Per caricare il saldo di partenza registra un movimento manuale di entrata («Saldo iniziale»).").small());
    if elenco.len() >= 2 {
        ui.separator();
        ui.label(RichText::new("Giroconto tra conti").strong());
        let voci: Vec<(i64, String)> = elenco.iter().map(|(i, n, _)| (*i, n.clone())).collect();
        egui::Grid::new("g_giro").num_columns(2).show(ui, |ui| {
            ui.label("Da");
            scegli(ui, "giro_da", &mut st.g_da, &voci, "");
            ui.end_row();
            ui.label("A");
            scegli(ui, "giro_a", &mut st.g_a, &voci, "");
            ui.end_row();
            campo(ui, "Importo (€)", &mut st.g_importo);
            campo(ui, "Data", &mut st.g_data);
            campo(ui, "Causale", &mut st.g_descr);
        });
        if ui.button("Esegui giroconto").clicked() {
            match (st.g_da, st.g_a, parse_eur(&st.g_importo), data_iso(&st.g_data)) {
                (Some(da), Some(a), Some(imp), Some(d)) => {
                    let r = c.db.giroconto(c.cond, da, a, imp, &d, &st.g_descr);
                    if c.esito(r, "Giroconto registrato").is_some() {
                        st.g_importo.clear();
                    }
                }
                _ => c.errore("Controlla conti, importo e data"),
            }
        }
    }
}

const COLONNE_MOV: [Col; 5] = [col("Data", 90.0), col("Descrizione", 0.0), col_dx("Importo", 110.0), col("Tipo", 150.0), col("Conto", 170.0)];

fn movimenti(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    let conti: Vec<(i64, String)> = c.db.elenco_conti_correnti(c.cond).unwrap_or_default().into_iter().map(|(i, n, _)| (i, n)).collect();
    if conti.is_empty() {
        ui.add_space(10.0);
        ui.colored_label(ARANCIO, "Crea prima un conto corrente (Condomìni → Conti correnti).");
        return;
    }
    if st.m_conto.is_none() {
        st.m_conto = conti.first().map(|x| x.0);
    }
    let voci: Vec<(i64, String)> = match c.esercizio {
        Some(es) => c.db.elenco_voci(es).unwrap_or_default().iter().map(|v| (v.id, format!("{} {}", v.codice, v.descrizione))).collect(),
        None => vec![],
    };
    let dal = data_iso(&st.dal);
    let al = data_iso(&st.al);
    let (db, cond, fc) = (c.db, c.cond, st.f_conto);
    let chiave = format!("{:?}|{:?}|{:?}", fc, dal, al);
    let tutti: Vec<Movimento> = st.cache.get(*c.rev, &chiave, || db.elenco_movimenti(cond, fc, dal.as_deref(), al.as_deref()).unwrap_or_default()).clone();
    let f = st.cerca.to_lowercase();
    let movs: Vec<&Movimento> = tutti.iter().filter(|m| f.is_empty() || m.descrizione.to_lowercase().contains(&f) || m.conto.to_lowercase().contains(&f)).collect();
    let (entrate, uscite) = c.db.riepilogo_cassa(c.cond, dal.as_deref(), al.as_deref()).unwrap_or((0, 0));
    let grigio = Color32::from_rgb(130, 130, 130);
    let righe: Vec<Riga> = movs
        .iter()
        .map(|m| {
            let spento = m.stornato || m.storno_di.is_some();
            let t = |s: String| -> Cella {
                if spento {
                    colorata(s, grigio)
                } else {
                    s.into()
                }
            };
            let descr = if m.stornato { format!("{} (stornato)", m.descrizione) } else { m.descrizione.clone() };
            let imp = if spento { colorata(format_eur(m.importo), grigio) } else { cella_eur(m.importo) };
            Riga::new(vec![t(data_it(&m.data)), t(descr), imp, t(nome_origine(&m.origine).to_string()), t(m.conto.clone())])
        })
        .collect();
    let piede = Riga::new(vec![
        forte(""),
        forte(format!("Totale ({} movimenti)   Entrate {} · Uscite {}", movs.len(), format_eur(entrate), format_eur(uscite))),
        if entrate - uscite < 0 { colorata(format_eur(entrate - uscite), ROSSO) } else { forte(format_eur(entrate - uscite)) },
    ]);
    if st.sel.map(|i| i >= righe.len()).unwrap_or(false) {
        st.sel = None;
    }
    let scelto: Option<&Movimento> = st.sel.and_then(|i| movs.get(i).copied());

    barra_azioni(ui, "az_mov", |ui| {
        if azione(ui, "📄", BLU, "Nuovo") {
            st.nuovo = true;
        }
        if azione(ui, "↩", ROSSO_ICONA, "Storna") {
            match scelto {
                Some(m) if !m.stornato && m.storno_di.is_none() => c.chiedi(&format!("Stornare il movimento «{}» di {}?", m.descrizione, format_eur(m.importo)), Azione::StornaMovimento(m.id)),
                Some(_) => c.errore("Il movimento è già uno storno o è già stato stornato"),
                None => c.errore("Seleziona un movimento"),
            }
        }
        if azione(ui, "📗", VERDE_ICONA, "Excel") {
            esporta_csv(c, "movimenti", &COLONNE_MOV, &righe);
        }
        if azione(ui, "📥", ARANCIO_ICONA, "Importa fatture") {
            *c.app = Some(AzioneApp::Vai(Sezione::Fornitori, Some(1)));
        }
        if azione(ui, "🔔", ARANCIO_ICONA, "Scadenze fornitori") {
            *c.app = Some(AzioneApp::Vai(Sezione::Agenda, None));
        }
    });

    // finestra «nuovo movimento»
    if st.nuovo {
        let mut aperta = true;
        egui::Window::new("Nuovo movimento manuale").open(&mut aperta).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ui.ctx(), |ui| {
            ui.label(RichText::new("Spese bancarie, interessi, saldo iniziale…").small().weak());
            egui::Grid::new("g_mov").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                ui.label("Conto");
                scegli(ui, "mov_cc", &mut st.m_conto, &conti, "");
                ui.end_row();
                ui.label("Tipo");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut st.m_entrata, true, "Entrata");
                    ui.selectable_value(&mut st.m_entrata, false, "Uscita");
                });
                ui.end_row();
                campo(ui, "Data (GG/MM/AAAA)", &mut st.m_data);
                campo(ui, "Descrizione *", &mut st.m_descr);
                campo(ui, "Importo (€)", &mut st.m_importo);
                ui.label("Imputa a voce di spesa");
                scegli(ui, "mov_voce", &mut st.m_voce, &voci, "(nessuna)");
                ui.end_row();
            });
            ui.label(RichText::new("Con una voce di spesa il movimento entra nel consuntivo (uscita = costo, entrata = recupero).").small());
            if ui.button("Registra movimento").clicked() {
                match (st.m_conto, parse_eur(&st.m_importo), data_iso(&st.m_data)) {
                    (Some(cc), Some(imp), Some(d)) if imp > 0 => {
                        let r = c.db.registra_movimento_manuale(c.cond, cc, &d, &st.m_descr, if st.m_entrata { imp } else { -imp }, st.m_voce);
                        if c.esito(r, "Movimento registrato").is_some() {
                            st.m_descr.clear();
                            st.m_importo.clear();
                            st.nuovo = false;
                        }
                    }
                    _ => c.errore("Controlla conto, data e importo (positivo)"),
                }
            }
        });
        if !aperta {
            st.nuovo = false;
        }
    }

    barra_filtri(ui, Some(&mut st.cerca), |ui| {
        ui.label("Conto");
        scegli(ui, "mov_f_cc", &mut st.f_conto, &conti, "Tutti");
        ui.label("Dal");
        ui.add(egui::TextEdit::singleline(&mut st.dal).hint_text("GG/MM/AAAA").desired_width(90.0));
        ui.label("Al");
        ui.add(egui::TextEdit::singleline(&mut st.al).hint_text("GG/MM/AAAA").desired_width(90.0));
    });
    match c.db.verifica_movimenti(c.cond) {
        Ok(v) if v.is_empty() => {}
        Ok(v) => {
            for m in v {
                ui.colored_label(ROSSO, m);
            }
        }
        Err(e) => {
            ui.colored_label(ROSSO, e.to_string());
        }
    }
    let esito = elenco(ui, "t_mov", &COLONNE_MOV, &righe, st.sel, Some(&piede));
    if let Some(i) = esito.clic {
        st.sel = Some(i);
    }
}

#[allow(dead_code)]
impl Stato {
    pub fn imposta_tab(&mut self, t: usize) {
        self.tab = t;
    }
}
