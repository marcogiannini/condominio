use super::*;
use crate::incassi::*;
use std::collections::HashMap;

pub struct Stato {
    persona: Option<i64>,
    data: String,
    importo: String,
    usa: String,
    conto: Option<i64>,
    metodo: String,
    note: String,
    manuale: bool,
    per_rata: HashMap<i64, String>,
    tab: usize,
    cache_estratto: Cache<Vec<MovEstratto>>,
}

impl Default for Stato {
    fn default() -> Self {
        Stato {
            persona: None,
            data: data_it(&oggi_iso()),
            importo: String::new(),
            usa: String::new(),
            conto: None,
            metodo: "bonifico".into(),
            note: String::new(),
            manuale: false,
            per_rata: HashMap::new(),
            tab: 0,
            cache_estratto: Cache::default(),
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
    titolo(ui, "Incassi e Salvadanaio");
    let persone: Vec<(i64, String)> = c.db.persone_del_condominio(c.cond).unwrap_or_default().iter().map(|p| (p.id, p.nome_completo())).collect();
    let conti: Vec<(i64, String)> = c.db.elenco_conti_correnti(c.cond).unwrap_or_default().into_iter().map(|(i, n, _)| (i, n)).collect();
    if st.conto.is_none() {
        st.conto = conti.first().map(|x| x.0);
    }
    ui.horizontal(|ui| {
        ui.label(RichText::new("Condòmino").strong());
        let prima = st.persona;
        scegli(ui, "inc_p", &mut st.persona, &persone, "");
        if prima != st.persona {
            st.per_rata.clear();
            st.importo.clear();
            st.usa.clear();
        }
    });
    let Some(pers) = st.persona else {
        ui.label("Scegli un condòmino per registrare incassi e vedere l'estratto conto.");
        return;
    };
    let nome = persone.iter().find(|x| x.0 == pers).map(|x| x.1.clone()).unwrap_or_default();
    let sit = c.db.situazione_persona(c.cond, pers).unwrap_or_default();
    ui.horizontal_wrapped(|ui| {
        ui.label(format!("Dovuto: {}", format_eur(sit.dovuto)));
        ui.separator();
        ui.label(format!("Versato: {}", format_eur(sit.versato)));
        ui.separator();
        ui.label(RichText::new(format!("Residuo: {}", format_eur(sit.residuo))).strong().color(if sit.residuo > 0 { ARANCIO } else { VERDE }));
        ui.separator();
        ui.label(RichText::new(format!("Salvadanaio: {}", format_eur(sit.salvadanaio))).strong());
    });
    ui.separator();
    ui.horizontal(|ui| {
        if c.schede {
            ui.selectable_value(&mut st.tab, 0, "Registra incasso");
        }
        if c.schede {
            ui.selectable_value(&mut st.tab, 1, "Storico incassi");
        }
        if c.schede {
            ui.selectable_value(&mut st.tab, 2, "Estratto conto");
        }
        if c.schede {
            ui.selectable_value(&mut st.tab, 3, "Movimenti Salvadanaio");
        }
    });
    ui.separator();
    match st.tab {
        0 => registra(c, st, ui, pers, &conti, sit.salvadanaio),
        1 => storico(c, ui, pers),
        2 => estratto(c, st, ui, pers, &nome),
        _ => {
            egui::Grid::new("t_wallet").striped(true).num_columns(3).show(ui, |ui| {
                for h in ["Data", "Causale", "Importo"] {
                    ui.strong(h);
                }
                ui.end_row();
                for (d, causale, imp) in c.db.movimenti_salvadanaio(c.cond, pers).unwrap_or_default() {
                    ui.label(data_it(&d));
                    ui.label(causale);
                    eur_colorato(ui, imp);
                    ui.end_row();
                }
            });
        }
    }
}

fn registra(c: &mut Ctx, st: &mut Stato, ui: &mut Ui, pers: i64, conti: &[(i64, String)], wallet: Cents) {
    if conti.is_empty() {
        ui.colored_label(ARANCIO, "Aggiungi prima un conto corrente in «Conti e Movimenti».");
    }
    egui::Grid::new("g_inc").num_columns(2).show(ui, |ui| {
        campo(ui, "Data incasso (GG/MM/AAAA)", &mut st.data);
        campo(ui, "Importo versato (€)", &mut st.importo);
        ui.label("Conto corrente");
        scegli(ui, "inc_cc", &mut st.conto, conti, "");
        ui.end_row();
        ui.label("Metodo");
        scegli_str(ui, "inc_met", &mut st.metodo, &["bonifico", "contanti", "assegno", "bollettino", "pagoPA", "altro"]);
        ui.end_row();
        campo(ui, "Note", &mut st.note);
    });
    ui.add_space(4.0);
    ui.group(|ui| {
        ui.label(RichText::new(format!("Salvadanaio disponibile: {}", format_eur(wallet))).strong());
        ui.horizontal(|ui| {
            ui.label("Credito da compensare con questo incasso (€):");
            ui.add(egui::TextEdit::singleline(&mut st.usa).desired_width(100.0).hint_text("0,00"));
            if wallet > 0 && ui.button("Usa tutto").clicked() {
                st.usa = format_eur(wallet).trim_end_matches(" €").to_string();
            }
            if ui.button("Non usare").clicked() {
                st.usa.clear();
            }
        });
        ui.label("Il credito viene usato solo se lo decidi tu, indicando l'importo qui sopra.");
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.radio_value(&mut st.manuale, false, "Distribuzione automatica (dalla rata più vecchia)");
        ui.radio_value(&mut st.manuale, true, "Distribuzione manuale");
    });

    let importo = if st.importo.trim().is_empty() { Some(0) } else { parse_eur(&st.importo) };
    let usa = if st.usa.trim().is_empty() { Some(0) } else { parse_eur(&st.usa) };
    let (Some(importo), Some(usa)) = (importo, usa) else {
        ui.colored_label(ROSSO, "Importo non valido");
        return;
    };
    let disponibile = importo + usa;
    let aperte = c.db.rate_aperte(c.cond, pers).unwrap_or_default();

    let mut distribuzione: Option<Vec<(i64, Cents)>> = None;
    if st.manuale {
        let mut tot: Cents = 0;
        egui::Grid::new("g_man").striped(true).num_columns(5).show(ui, |ui| {
            for h in ["Piano", "Rata", "Scadenza", "Residuo", "Importo da imputare"] {
                ui.strong(h);
            }
            ui.end_row();
            for r in &aperte {
                ui.label(&r.piano);
                ui.label(r.numero.to_string());
                ui.label(data_it(&r.scadenza));
                ui.label(format_eur(r.residuo()));
                let e = st.per_rata.entry(r.rata_id).or_default();
                ui.add(egui::TextEdit::singleline(e).desired_width(100.0).hint_text("0,00"));
                ui.end_row();
            }
        });
        let mut v = vec![];
        for r in &aperte {
            let txt = st.per_rata.get(&r.rata_id).cloned().unwrap_or_default();
            if txt.trim().is_empty() {
                continue;
            }
            match parse_eur(&txt) {
                Some(x) if x > 0 => {
                    tot += x;
                    v.push((r.rata_id, x));
                }
                _ => {}
            }
        }
        let diff = disponibile - tot;
        let col = if diff < 0 { ROSSO } else { ui.visuals().text_color() };
        ui.label(RichText::new(format!("Disponibile {} – distribuito {} – eccedenza al Salvadanaio {}", format_eur(disponibile), format_eur(tot), format_eur(diff.max(0)))).color(col));
        distribuzione = Some(v);
    } else {
        let (ant, resto) = c.db.anteprima_allocazione(c.cond, pers, disponibile).unwrap_or((vec![], 0));
        if disponibile > 0 {
            ui.label(RichText::new("Anteprima imputazione").strong());
            for (r, a) in &ant {
                ui.label(format!("  {} rata {} del {}: {} (residuo {})", r.piano, r.numero, data_it(&r.scadenza), format_eur(*a), format_eur(r.residuo() - a)));
            }
            if resto > 0 {
                ui.colored_label(ARANCIO, format!("  Eccedenza {} → va nel Salvadanaio come credito", format_eur(resto)));
            }
        }
    }

    ui.add_space(6.0);
    if ui.button(RichText::new("Registra incasso").strong()).clicked() {
        match data_iso(&st.data) {
            None => c.errore("Data non valida (GG/MM/AAAA)"),
            Some(d) => {
                let n = NuovoIncasso {
                    condominio_id: c.cond,
                    persona_id: pers,
                    conto_corrente_id: st.conto,
                    data: d,
                    importo,
                    usa_salvadanaio: usa,
                    distribuzione,
                    metodo: st.metodo.clone(),
                    note: st.note.clone(),
                };
                let r = c.db.registra_incasso(&n);
                if c.esito(r, "Incasso registrato").is_some() {
                    st.importo.clear();
                    st.usa.clear();
                    st.note.clear();
                    st.per_rata.clear();
                }
            }
        }
    }
}

fn storico(c: &mut Ctx, ui: &mut Ui, pers: i64) {
    egui::Grid::new("t_storico").striped(true).num_columns(8).show(ui, |ui| {
        for h in ["Data", "Metodo", "Versato", "Da Salvadanaio", "Imputato", "Eccedenza", "Stato", ""] {
            ui.strong(h);
        }
        ui.end_row();
        for i in c.db.elenco_incassi(c.cond, Some(pers)).unwrap_or_default() {
            ui.label(data_it(&i.data));
            ui.label(&i.metodo);
            ui.label(format_eur(i.importo));
            ui.label(format_eur(i.da_salvadanaio));
            ui.label(format_eur(i.allocato));
            ui.label(format_eur(i.eccedenza));
            ui.colored_label(if i.stato == "valido" { VERDE } else { ROSSO }, &i.stato);
            if i.stato == "valido" && mini(ui, "Storna") {
                c.chiedi("Stornare l'incasso? Le rate tornano aperte, il Salvadanaio viene ripristinato e la contabilità riceve la scrittura inversa.", Azione::StornaIncasso(i.id));
            }
            ui.end_row();
        }
    });
}

fn estratto(c: &mut Ctx, st: &mut Stato, ui: &mut Ui, pers: i64, nome: &str) {
    let db = c.db;
    let cond = c.cond;
    let righe = st.cache_estratto.get(*c.rev, &pers.to_string(), || db.estratto_conto(cond, pers).unwrap_or_default()).clone();
    if ui.button("Esporta in CSV").clicked() {
        if let Some(p) = rfd::FileDialog::new().set_file_name("estratto_conto.csv").save_file() {
            let mut out = String::from("Data;Descrizione;Dare;Avere;Saldo\n");
            for r in &righe {
                out.push_str(&format!("{};{};{};{};{}\n", data_it(&r.data), csv_campo(&r.descrizione), format_eur(r.dare), format_eur(r.avere), format_eur(r.saldo)));
            }
            match std::fs::write(&p, out) {
                Ok(_) => c.ok(&format!("Estratto conto di {} esportato", nome)),
                Err(e) => c.err(e.into()),
            }
        }
    }
    egui::Grid::new("t_estratto").striped(true).num_columns(5).show(ui, |ui| {
        for h in ["Data", "Descrizione", "Dare", "Avere", "Saldo"] {
            ui.strong(h);
        }
        ui.end_row();
        for r in &righe {
            ui.label(data_it(&r.data));
            ui.label(&r.descrizione);
            ui.label(if r.dare != 0 { format_eur(r.dare) } else { String::new() });
            ui.label(if r.avere != 0 { format_eur(r.avere) } else { String::new() });
            eur_colorato(ui, r.saldo);
            ui.end_row();
        }
    });
}

#[allow(dead_code)]
impl Stato {
    pub fn imposta_tab(&mut self, t: usize) {
        self.tab = t;
    }
    pub fn persona_prova(&mut self, id: i64) {
        self.persona = Some(id);
    }
}
