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
        }
    }
}

fn csv_campo(s: &str) -> String {
    if s.contains(';') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub fn mostra(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    titolo(ui, "Conti e Movimenti di gestione");
    ui.horizontal(|ui| {
        if c.schede {
            ui.selectable_value(&mut st.tab, 0, "Conti correnti");
        }
        if c.schede {
            ui.selectable_value(&mut st.tab, 1, "Movimenti");
        }
    });
    ui.separator();
    match st.tab {
        0 => conti(c, st, ui),
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

fn movimenti(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    let conti: Vec<(i64, String)> = c.db.elenco_conti_correnti(c.cond).unwrap_or_default().into_iter().map(|(i, n, _)| (i, n)).collect();
    if conti.is_empty() {
        ui.colored_label(ARANCIO, "Crea prima un conto corrente nella scheda «Conti correnti».");
        return;
    }
    if st.m_conto.is_none() {
        st.m_conto = conti.first().map(|x| x.0);
    }
    let voci: Vec<(i64, String)> = match c.esercizio {
        Some(es) => c.db.elenco_voci(es).unwrap_or_default().iter().map(|v| (v.id, format!("{} {}", v.codice, v.descrizione))).collect(),
        None => vec![],
    };

    ui.collapsing("Nuovo movimento manuale (spese bancarie, interessi, saldo iniziale…)", |ui| {
        egui::Grid::new("g_mov").num_columns(2).show(ui, |ui| {
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
                    }
                }
                _ => c.errore("Controlla conto, data e importo (positivo)"),
            }
        }
    });

    ui.separator();
    ui.horizontal(|ui| {
        ui.label("Conto");
        scegli(ui, "mov_f_cc", &mut st.f_conto, &conti, "Tutti");
        ui.label("Dal");
        ui.add(egui::TextEdit::singleline(&mut st.dal).hint_text("GG/MM/AAAA").desired_width(100.0));
        ui.label("Al");
        ui.add(egui::TextEdit::singleline(&mut st.al).hint_text("GG/MM/AAAA").desired_width(100.0));
    });
    let dal = data_iso(&st.dal);
    let al = data_iso(&st.al);
    let (db, cond, fc) = (c.db, c.cond, st.f_conto);
    let chiave = format!("{:?}|{:?}|{:?}", fc, dal, al);
    let movs: Vec<Movimento> = st.cache.get(*c.rev, &chiave, || db.elenco_movimenti(cond, fc, dal.as_deref(), al.as_deref()).unwrap_or_default()).clone();
    let (entrate, uscite) = c.db.riepilogo_cassa(c.cond, dal.as_deref(), al.as_deref()).unwrap_or((0, 0));

    ui.horizontal_wrapped(|ui| {
        ui.label(format!("Entrate: {}", format_eur(entrate)));
        ui.separator();
        ui.label(format!("Uscite: {}", format_eur(uscite)));
        ui.separator();
        ui.label(RichText::new(format!("Saldo periodo: {}", format_eur(entrate - uscite))).strong());
        if ui.button("Esporta CSV").clicked() {
            if let Some(p) = rfd::FileDialog::new().set_file_name("movimenti.csv").save_file() {
                let mut s = String::from("data;conto;tipo;descrizione;entrata;uscita\n");
                for m in &movs {
                    s.push_str(&format!(
                        "{};{};{};{};{};{}\n",
                        data_it(&m.data),
                        csv_campo(&m.conto),
                        nome_origine(&m.origine),
                        csv_campo(&m.descrizione),
                        if m.importo > 0 { format_eur(m.importo) } else { String::new() },
                        if m.importo < 0 { format_eur(-m.importo) } else { String::new() }
                    ));
                }
                match std::fs::write(&p, s) {
                    Ok(_) => c.ok("Movimenti esportati"),
                    Err(e) => c.err(e.into()),
                }
            }
        }
    });
    match c.db.verifica_movimenti(c.cond) {
        Ok(v) if v.is_empty() => {
            ui.colored_label(VERDE, "Movimenti coerenti con incassi, pagamenti e F24");
        }
        Ok(v) => {
            for m in v {
                ui.colored_label(ROSSO, m);
            }
        }
        Err(e) => {
            ui.colored_label(ROSSO, e.to_string());
        }
    }
    ui.separator();
    egui::Grid::new("t_mov").striped(true).num_columns(6).show(ui, |ui| {
        for h in ["Data", "Conto", "Tipo", "Descrizione", "Importo", ""] {
            ui.strong(h);
        }
        ui.end_row();
        for m in &movs {
            let t = |s: String| if m.stornato || m.storno_di.is_some() { RichText::new(s).weak() } else { RichText::new(s) };
            ui.label(t(data_it(&m.data)));
            ui.label(t(m.conto.clone()));
            ui.label(t(nome_origine(&m.origine).to_string()));
            ui.label(t(if m.stornato { format!("{} (stornato)", m.descrizione) } else { m.descrizione.clone() }));
            eur_colorato(ui, m.importo);
            if !m.stornato && m.storno_di.is_none() && mini(ui, "Storna") {
                c.chiedi(&format!("Stornare il movimento «{}» di {}?", m.descrizione, format_eur(m.importo)), Azione::StornaMovimento(m.id));
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
