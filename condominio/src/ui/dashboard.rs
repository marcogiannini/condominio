use super::*;
use crate::legale::{Controllo, Gravita};

#[derive(Default)]
pub struct Stato {
    controlli: Cache<Vec<Controllo>>,
}

fn riquadro(ui: &mut Ui, titolo: &str, valore: String, colore: Option<Color32>) {
    ui.group(|ui| {
        ui.set_min_width(190.0);
        ui.vertical(|ui| {
            ui.label(RichText::new(titolo).small());
            let mut t = RichText::new(valore).size(20.0).strong();
            if let Some(c) = colore {
                t = t.color(c);
            }
            ui.label(t);
        });
    });
}

pub fn mostra(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    titolo(ui, &format!("Cruscotto – {}", c.cond_nome));
    let larghezza = (ui.ctx().screen_rect().width() - 290.0).max(480.0);
    ui.set_max_width(larghezza);
    let sit = c.db.situazione_gestione(c.cond).unwrap_or_default();
    let conti = c.db.elenco_conti_correnti(c.cond).unwrap_or_default();
    let aperti = c.db.elenco_ticket(c.cond, None, true).map(|v| v.len()).unwrap_or(0);

    ui.horizontal_wrapped(|ui| {
        riquadro(ui, "Liquidità (conti correnti)", format_eur(sit.liquidita), None);
        riquadro(ui, "Crediti verso condòmini", format_eur(sit.crediti_condomini), if sit.crediti_condomini > 0 { Some(ARANCIO) } else { None });
        riquadro(ui, "Salvadanaio condòmini", format_eur(sit.salvadanaio), None);
        riquadro(ui, "Fatture da pagare", format_eur(sit.debiti_fornitori), None);
        riquadro(ui, "Ritenute da versare", format_eur(sit.ritenute_da_versare), None);
        riquadro(ui, "Segnalazioni aperte", aperti.to_string(), if aperti > 0 { Some(ARANCIO) } else { None });
    });

    if conti.is_empty() {
        ui.add_space(8.0);
        ui.colored_label(ARANCIO, "Nessun conto corrente: aggiungine uno in «Conti e Movimenti» per registrare incassi e pagamenti.");
    }

    ui.add_space(10.0);
    ui.separator();
    ui.label(RichText::new("Controlli di conformità").strong());
    let db = c.db;
    let cond = c.cond;
    let controlli = st.controlli.get(*c.rev, "", || db.controlli_conformita(cond).unwrap_or_default());
    let problemi = controlli.iter().filter(|x| x.gravita != Gravita::Ok).count();
    if problemi == 0 {
        ui.colored_label(VERDE, "Nessuna anomalia rilevata");
    } else {
        for x in controlli.iter().filter(|x| x.gravita != Gravita::Ok).take(5) {
            let col = if x.gravita == Gravita::Errore { ROSSO } else { ARANCIO };
            ui.colored_label(col, format!("[{}] {}", x.riferimento, x.messaggio));
        }
        if problemi > 5 {
            ui.label(format!("… e altre {} segnalazioni", problemi - 5));
        }
    }
    if ui.button("Apri Conformità legale").clicked() {
        *c.naviga = Some(Sezione::Conformita);
    }

    ui.add_space(10.0);
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(RichText::new("Notifiche recenti").strong());
        if ui.button("Segna tutte come lette").clicked() {
            let r = c.db.segna_notifiche_lette(c.cond);
            c.esito(r, "Notifiche segnate come lette");
        }
    });
    let n = c.db.elenco_notifiche(c.cond, 15).unwrap_or_default();
    if n.is_empty() {
        ui.label("Nessuna notifica.");
    }
    for x in n {
        let t = format!("{}  {}", x.creato_il, x.messaggio);
        if x.letta {
            ui.label(t);
        } else {
            ui.label(RichText::new(t).strong());
        }
    }
}
