use super::*;
use crate::consuntivo::{QuotaPersona, RigaConsuntivo};

pub struct Stato {
    tab: usize,
    forma: usize, // 0 sintetico, 1 dettagliato, 2 misto (solo capitoli con importi + voci scostate)
    persona: Option<i64>,
    delibera: Option<i64>,
    n_rate: String,
    prima: String,
    cadenza: String,
    cache_righe: Cache<Vec<RigaConsuntivo>>,
    cache_quote: Cache<Result<Vec<QuotaPersona>, String>>,
}

impl Default for Stato {
    fn default() -> Self {
        Stato {
            tab: 0,
            forma: 1,
            persona: None,
            delibera: None,
            n_rate: "1".into(),
            prima: data_it(&oggi_iso()),
            cadenza: "1".into(),
            cache_righe: Cache::default(),
            cache_quote: Cache::default(),
        }
    }
}

pub fn mostra(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    titolo(ui, "Consuntivo e Bilanci");
    let Some(es) = c.esercizio else {
        ui.label("Crea o seleziona un esercizio (Preventivo e Piano dei Conti → Esercizi).");
        return;
    };
    ui.horizontal(|ui| {
        if c.schede {
            ui.selectable_value(&mut st.tab, 0, "Preventivo e consuntivo");
        }
        if c.schede {
            ui.selectable_value(&mut st.tab, 1, "Riparto per condòmino");
        }
        if c.schede {
            ui.selectable_value(&mut st.tab, 2, "Conguaglio");
        }
        ui.separator();
        if ui.button("Stampa rendiconto").clicked() {
            match c.db.html_rendiconto(es) {
                Ok(h) => apri_html(c, "rendiconto", &h),
                Err(e) => c.err(e),
            }
        }
    });
    ui.separator();
    match st.tab {
        0 => confronto(c, st, ui, es),
        1 => riparto(c, st, ui, es),
        _ => conguaglio(c, st, ui, es),
    }
}

fn confronto(c: &mut Ctx, st: &mut Stato, ui: &mut Ui, es: i64) {
    ui.horizontal(|ui| {
        ui.label("Forma:");
        ui.selectable_value(&mut st.forma, 0, "Sintetica (capitoli)");
        ui.selectable_value(&mut st.forma, 1, "Dettagliata (tutte le voci)");
        ui.selectable_value(&mut st.forma, 2, "Mista (capitoli + voci con scostamento)");
    });
    let db = c.db;
    let righe: Vec<RigaConsuntivo> = st.cache_righe.get(*c.rev, &es.to_string(), || db.righe_consuntivo(es).unwrap_or_default()).clone();
    let nascosta = |r: &RigaConsuntivo| match st.forma {
        0 => r.parent_id.is_some(),
        2 => r.parent_id.is_some() && r.scostamento() == 0,
        _ => false,
    };
    egui::Grid::new("t_cons").striped(true).num_columns(5).show(ui, |ui| {
        for h in ["Voce", "Preventivo", "Consuntivo", "Scostamento", ""] {
            ui.strong(h);
        }
        ui.end_row();
        for r in righe.iter().filter(|r| !nascosta(r)) {
            let t = format!("{}{} {}", if r.parent_id.is_some() { "    " } else { "" }, r.codice, r.descrizione);
            ui.label(if r.parent_id.is_none() { RichText::new(t).strong() } else { RichText::new(t) });
            ui.label(format_eur(r.preventivo));
            ui.label(format_eur(r.consuntivo));
            let s = r.scostamento();
            ui.label(RichText::new(format_eur(s)).color(if s > 0 { ROSSO } else if s < 0 { VERDE } else { ui.visuals().text_color() }));
            ui.label(if r.sopravvenienza { "sopravvenienza (art. 1130-bis)" } else { "" });
            ui.end_row();
        }
        let (p, k): (Cents, Cents) = righe.iter().filter(|r| r.foglia).fold((0, 0), |a, r| (a.0 + r.preventivo, a.1 + r.consuntivo));
        ui.strong("TOTALE");
        ui.strong(format_eur(p));
        ui.strong(format_eur(k));
        ui.strong(format_eur(k - p));
        ui.end_row();
    });
    let non_imp = c.db.fatture_non_imputate(es).unwrap_or(0);
    if non_imp > 0 {
        ui.colored_label(ARANCIO, format!("Attenzione: fatture per {} non sono imputate a nessuna voce e non rientrano nel consuntivo.", format_eur(non_imp)));
    }
}

fn riparto(c: &mut Ctx, st: &mut Stato, ui: &mut Ui, es: i64) {
    let db = c.db;
    let quote = match st.cache_quote.get(*c.rev, &es.to_string(), || db.quote_consuntivo(es).map_err(|e| e.to_string())) {
        Ok(q) => q.clone(),
        Err(e) => {
            ui.colored_label(ROSSO, e.clone());
            ui.label("Assegna una tabella millesimale a tutte le voci con spesa (Preventivo e Piano dei Conti).");
            return;
        }
    };
    egui::Grid::new("t_quote").striped(true).num_columns(7).show(ui, |ui| {
        for h in ["Condòmino", "Unità", "Spesa effettiva", "Rate emesse", "Conguaglio", "Versato", "Ancora da versare"] {
            ui.strong(h);
        }
        ui.end_row();
        for q in &quote {
            if ui.link(&q.persona).clicked() {
                st.persona = Some(q.persona_id);
            }
            ui.label(&q.unita);
            ui.label(format_eur(q.consuntivo));
            ui.label(format_eur(q.emesso));
            ui.label(RichText::new(format_eur(q.conguaglio())).color(if q.conguaglio() > 0 { ARANCIO } else { VERDE }));
            ui.label(format_eur(q.versato));
            eur_colorato(ui, q.da_versare());
            ui.end_row();
        }
    });
    if let Some(p) = st.persona {
        ui.separator();
        let nome = quote.iter().find(|q| q.persona_id == p).map(|q| q.persona.clone()).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Bilancio individuale – {}", nome)).strong());
            if ui.button("Stampa").clicked() {
                match c.db.html_individuale(es, p) {
                    Ok(h) => apri_html(c, "bilancio_individuale", &h),
                    Err(e) => c.err(e),
                }
            }
            if ui.button("Chiudi").clicked() {
                st.persona = None;
            }
        });
        egui::Grid::new("t_ind").striped(true).num_columns(3).show(ui, |ui| {
            for (voce, tabella, imp) in c.db.dettaglio_persona_consuntivo(es, p).unwrap_or_default() {
                ui.label(voce);
                ui.label(tabella);
                ui.label(format_eur(imp));
                ui.end_row();
            }
        });
    }
}

fn conguaglio(c: &mut Ctx, st: &mut Stato, ui: &mut Ui, es: i64) {
    match c.db.stato_conguaglio(es) {
        Ok(Some(s)) => {
            ui.colored_label(VERDE, format!("Conguaglio emesso il {}", data_it(&s.data)));
            ui.label(format!("Da versare dai condòmini: {}  ·  accreditato nel Salvadanaio: {}", format_eur(s.totale_debito), format_eur(s.totale_credito)));
            if ui.button("Annulla conguaglio").clicked() {
                c.chiedi("Annullare il conguaglio? Le rate di conguaglio verranno annullate e i crediti tolti dal Salvadanaio (se non già usati).", Azione::AnnullaConguaglio(es));
            }
            return;
        }
        Ok(None) => {}
        Err(e) => {
            ui.colored_label(ROSSO, e.to_string());
            return;
        }
    }
    ui.label("Il conguaglio è la differenza tra la spesa effettiva e le rate emesse. Chi deve versare in più riceve un piano integrativo di rate; chi ha pagato troppo viene accreditato nel Salvadanaio (lo compensi tu quando vuoi). Serve la delibera di approvazione del rendiconto (art. 1130 n. 10 c.c.).");
    let delibere: Vec<(i64, String)> = c
        .db
        .elenco_delibere(c.cond)
        .unwrap_or_default()
        .iter()
        .map(|d| (d.id, format!("{} – {} ({})", data_it(&d.data), d.oggetto, d.tipo)))
        .collect();
    egui::Grid::new("g_cong").num_columns(2).show(ui, |ui| {
        ui.label("Delibera di approvazione *");
        scegli(ui, "cong_del", &mut st.delibera, &delibere, "");
        ui.end_row();
        campo(ui, "Numero di rate", &mut st.n_rate);
        campo(ui, "Prima scadenza", &mut st.prima);
        campo(ui, "Cadenza (mesi)", &mut st.cadenza);
    });
    if ui.button("Emetti conguaglio").clicked() {
        let (Some(d), Ok(n), Some(p), Ok(cad)) = (st.delibera, st.n_rate.trim().parse::<i64>(), data_iso(&st.prima), st.cadenza.trim().parse::<i64>()) else {
            c.errore("Controlla delibera, numero rate, data e cadenza");
            return;
        };
        match c.db.emetti_conguaglio(es, d, n, &p, cad) {
            Ok((debitori, creditori)) => c.ok(&format!("Conguaglio emesso: {} condòmini a debito, {} accreditati nel Salvadanaio", debitori, creditori)),
            Err(e) => c.err(e),
        }
    }
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
