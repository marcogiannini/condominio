//! Riparti flessibili: suddivisione di una voce su più tabelle millesimali e riduzioni/esclusioni per unità.

use super::*;
use crate::riparti::UNO;

pub struct Stato {
    voce: Option<i64>,
    quote: Vec<(i64, String)>, // (tabella, percentuale)
    caricata: Option<i64>,
    nuova_tabella: Option<i64>,
    r_unita: Option<i64>,
    r_perc: String,
    r_motivo: String,
}

impl Default for Stato {
    fn default() -> Self {
        Stato { voce: None, quote: vec![], caricata: None, nuova_tabella: None, r_unita: None, r_perc: "100".into(), r_motivo: String::new() }
    }
}

fn perc(ppm: i64) -> String {
    let v = ppm as f64 / 10_000.0;
    let s = format!("{:.4}", v);
    s.trim_end_matches('0').trim_end_matches('.').replace('.', ",")
}

pub fn mostra(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    let Some(es) = c.esercizio else {
        ui.label("Crea o seleziona un esercizio.");
        return;
    };
    ui.label("Una voce può essere ripartita su più tabelle (es. 60% scale e 40% proprietà) e singole unità possono essere escluse o ridotte (es. negozio al piano terra che non usa l'ascensore). La somma dei riparti si ricalcola da sola sul resto dei condòmini.");
    let voci: Vec<(i64, String)> = c.db.elenco_voci(es).unwrap_or_default().iter().map(|v| (v.id, format!("{} {}", v.codice, v.descrizione))).collect();
    let tabelle: Vec<(i64, String)> = c.db.elenco_tabelle(c.cond).unwrap_or_default().iter().map(|t| (t.id, format!("{} – {}", t.codice, t.nome))).collect();
    ui.horizontal(|ui| {
        ui.label("Voce di spesa");
        scegli(ui, "rip_voce", &mut st.voce, &voci, "");
    });
    let Some(voce) = st.voce else { return };
    if st.caricata != Some(voce) {
        st.quote = c.db.tabelle_voce(voce).unwrap_or_default().into_iter().map(|q| (q.tabella_id, perc(q.quota_ppm))).collect();
        st.caricata = Some(voce);
    }

    ui.separator();
    ui.label(RichText::new("Suddivisione sulle tabelle").strong());
    let mut rimuovi: Option<usize> = None;
    for (i, (t, p)) in st.quote.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label(tabelle.iter().find(|x| x.0 == *t).map(|x| x.1.clone()).unwrap_or_default());
            ui.add(egui::TextEdit::singleline(p).desired_width(70.0));
            ui.label("%");
            if mini(ui, "Rimuovi") {
                rimuovi = Some(i);
            }
        });
    }
    if let Some(i) = rimuovi {
        st.quote.remove(i);
    }
    ui.horizontal(|ui| {
        scegli(ui, "rip_nuova_tab", &mut st.nuova_tabella, &tabelle, "");
        if ui.button("Aggiungi tabella").clicked() {
            if let Some(t) = st.nuova_tabella {
                if !st.quote.iter().any(|q| q.0 == t) {
                    st.quote.push((t, String::new()));
                }
            }
        }
    });
    if ui.button("Salva suddivisione").clicked() {
        let mut v: Vec<(i64, i64)> = vec![];
        let mut ok = true;
        for (t, p) in &st.quote {
            match parse_percentuale_ppm(p) {
                Some(x) => v.push((*t, x)),
                None => ok = false,
            }
        }
        if !ok {
            c.errore("Percentuale non valida");
        } else {
            let r = c.db.imposta_riparto_voce(voce, &v);
            c.esito(r, "Riparto salvato (lista vuota = tabella semplice della voce)");
        }
    }

    ui.separator();
    ui.label(RichText::new("Esclusioni e riduzioni per unità").strong());
    let unita: Vec<(i64, String)> = c.db.elenco_unita(c.cond).unwrap_or_default().iter().map(|u| (u.id, u.etichetta())).collect();
    for r in c.db.elenco_riduzioni(voce).unwrap_or_default() {
        ui.horizontal(|ui| {
            let txt = if r.riduzione_ppm >= UNO { "esclusa".to_string() } else { format!("ridotta del {}%", perc(r.riduzione_ppm)) };
            ui.label(format!("{} – {} {}", r.unita, txt, if r.motivo.is_empty() { String::new() } else { format!("({})", r.motivo) }));
            if mini(ui, "Rimuovi") {
                let x = c.db.rimuovi_riduzione(voce, r.unita_id);
                c.esito(x, "Riduzione rimossa");
            }
        });
    }
    ui.horizontal(|ui| {
        scegli(ui, "rip_unita", &mut st.r_unita, &unita, "");
        ui.label("riduzione %");
        ui.add(egui::TextEdit::singleline(&mut st.r_perc).desired_width(60.0));
        ui.add(egui::TextEdit::singleline(&mut st.r_motivo).hint_text("motivo").desired_width(200.0));
        if ui.button("Applica").clicked() {
            match (st.r_unita, parse_percentuale_ppm(&st.r_perc)) {
                (Some(u), Some(p)) => {
                    let r = c.db.imposta_riduzione(voce, u, p, &st.r_motivo);
                    c.esito(r, "Riduzione applicata (100% = unità esclusa)");
                }
                _ => c.errore("Scegli l'unità e indica una percentuale valida"),
            }
        }
    });
    ui.separator();
    ui.label(RichText::new("Anteprima riparto dell'importo preventivato").strong());
    let importo = c.db.elenco_voci(es).unwrap_or_default().iter().find(|v| v.id == voce).map(|v| v.importo_cents).unwrap_or(0);
    match c.db.riparto_voce(voce, importo) {
        Ok(v) => {
            egui::Grid::new("t_rip_prev").striped(true).num_columns(2).show(ui, |ui| {
                for (u, imp) in v {
                    ui.label(unita.iter().find(|x| x.0 == u).map(|x| x.1.clone()).unwrap_or_default());
                    ui.label(format_eur(imp));
                    ui.end_row();
                }
            });
        }
        Err(e) => {
            ui.colored_label(ARANCIO, e.to_string());
        }
    }
}
