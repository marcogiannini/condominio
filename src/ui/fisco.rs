use super::*;
use crate::fiscale::{RigaCu, RigaSt};

pub struct Stato {
    tab: usize,
    anno: String,
    cache_cu: Cache<Vec<RigaCu>>,
    cache_st: Cache<Vec<RigaSt>>,
}

impl Default for Stato {
    fn default() -> Self {
        Stato { tab: 0, anno: (oggi().format("%Y").to_string().parse::<i32>().unwrap_or(2026) - 1).to_string(), cache_cu: Cache::default(), cache_st: Cache::default() }
    }
}

const MESI: [&str; 12] = ["gennaio", "febbraio", "marzo", "aprile", "maggio", "giugno", "luglio", "agosto", "settembre", "ottobre", "novembre", "dicembre"];

pub fn mostra(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    titolo(ui, "Fisco: ritenute, Certificazione Unica e 770");
    ui.horizontal(|ui| {
        if c.schede {
            ui.selectable_value(&mut st.tab, 0, "Certificazione Unica");
        }
        if c.schede {
            ui.selectable_value(&mut st.tab, 1, "Prospetto 770 (ST)");
        }
        ui.separator();
        ui.label("Anno");
        ui.add(egui::TextEdit::singleline(&mut st.anno).desired_width(60.0));
    });
    ui.label("Criterio di cassa: contano i pagamenti effettuati nell'anno. Le ritenute si registrano e si versano da «Fornitori e Fatture».");
    ui.separator();
    let Ok(anno) = st.anno.trim().parse::<i32>() else {
        ui.colored_label(ROSSO, "Anno non valido");
        return;
    };
    let (db, cond) = (c.db, c.cond);
    if st.tab == 0 {
        let v: Vec<RigaCu> = st.cache_cu.get(*c.rev, &anno.to_string(), || db.riepilogo_cu(cond, anno).unwrap_or_default()).clone();
        ui.horizontal(|ui| {
            if ui.button("Stampa riepilogo").clicked() {
                match c.db.html_cu(c.cond, anno) {
                    Ok(h) => apri_html(c, "cu", &h),
                    Err(e) => c.err(e),
                }
            }
            if ui.button("Esporta CSV").clicked() {
                if let Some(p) = rfd::FileDialog::new().set_file_name(format!("cu_{}.csv", anno)).save_file() {
                    let r = c.db.csv_cu(c.cond, anno).and_then(|s| Ok(std::fs::write(&p, s)?));
                    c.esito(r, "CSV esportato");
                }
            }
        });
        if v.is_empty() {
            ui.label("Nessuna ritenuta operata nell'anno.");
        }
        egui::Grid::new("t_cu").striped(true).num_columns(8).show(ui, |ui| {
            for h in ["Percipiente", "CF/P.IVA", "Regime", "Fatture", "Imponibile", "IVA", "Ritenuta operata", "Ritenuta versata"] {
                ui.strong(h);
            }
            ui.end_row();
            for r in &v {
                ui.label(&r.ragione_sociale);
                ui.label(&r.cf_piva);
                ui.label(&r.regime);
                ui.label(r.n_fatture.to_string());
                ui.label(format_eur(r.imponibile));
                ui.label(format_eur(r.iva));
                ui.label(format_eur(r.ritenuta_operata));
                let col = if r.ritenuta_versata < r.ritenuta_operata { ARANCIO } else { VERDE };
                ui.label(RichText::new(format_eur(r.ritenuta_versata)).color(col));
                ui.end_row();
            }
        });
    } else {
        let v: Vec<RigaSt> = st.cache_st.get(*c.rev, &anno.to_string(), || db.riepilogo_770(cond, anno).unwrap_or_default()).clone();
        if v.is_empty() {
            ui.label("Nessuna ritenuta nell'anno.");
        }
        egui::Grid::new("t_770").striped(true).num_columns(4).show(ui, |ui| {
            for h in ["Mese", "Codice tributo", "Ritenute operate", "Ritenute versate"] {
                ui.strong(h);
            }
            ui.end_row();
            for r in &v {
                ui.label(MESI.get(r.mese.saturating_sub(1) as usize).copied().unwrap_or("?"));
                ui.label(&r.codice_tributo);
                ui.label(format_eur(r.operate));
                ui.label(format_eur(r.versate));
                ui.end_row();
            }
        });
        ui.add_space(6.0);
        ui.label(RichText::new("Prospetto di supporto alla compilazione: la trasmissione telematica di CU e 770 resta a carico del professionista abilitato.").small());
    }
}

#[allow(dead_code)]
impl Stato {
    pub fn imposta_tab(&mut self, t: usize) {
        self.tab = t;
    }
}
