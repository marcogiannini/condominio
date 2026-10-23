use super::*;
use crate::scadenzario::Scadenza;
use chrono::Duration;

pub struct Stato {
    giorni: String,
    data: String,
    titolo: String,
    note: String,
    cache: Cache<Vec<Scadenza>>,
}

impl Default for Stato {
    fn default() -> Self {
        Stato { giorni: "60".into(), data: data_it(&oggi_iso()), titolo: String::new(), note: String::new(), cache: Cache::default() }
    }
}

fn nome_tipo(t: &str) -> &str {
    match t {
        "rata" => "Rata",
        "fattura" => "Fattura da pagare",
        "ritenute" => "Ritenute / F24",
        "prescrizione" => "Prescrizione",
        "assemblea" => "Assemblea",
        "fiscale" => "Adempimento fiscale",
        "agenda" => "Promemoria",
        altro => altro,
    }
}

pub fn mostra(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    titolo(ui, "Agenda e scadenze");
    let giorni: i64 = st.giorni.trim().parse().unwrap_or(60).clamp(1, 730);
    let oggi = oggi();
    let fino = oggi + Duration::days(giorni);

    ui.horizontal(|ui| {
        ui.label("Mostra le scadenze dei prossimi");
        ui.add(egui::TextEdit::singleline(&mut st.giorni).desired_width(50.0));
        ui.label("giorni (le scadute non evase sono sempre incluse)");
        if ui.button("Stampa").clicked() {
            match c.db.html_scadenzario(c.cond, oggi, fino) {
                Ok(h) => apri_html(c, "scadenzario", &h),
                Err(e) => c.err(e),
            }
        }
    });

    ui.collapsing("Nuovo promemoria", |ui| {
        egui::Grid::new("g_agenda").num_columns(2).show(ui, |ui| {
            campo(ui, "Data (GG/MM/AAAA)", &mut st.data);
            campo(ui, "Titolo *", &mut st.titolo);
            campo(ui, "Note", &mut st.note);
        });
        if ui.button("Aggiungi").clicked() {
            let r = c.db.aggiungi_agenda(c.cond, &st.data, &st.titolo, &st.note);
            if c.esito(r, "Promemoria aggiunto").is_some() {
                st.titolo.clear();
                st.note.clear();
            }
        }
    });
    ui.separator();

    let (db, cond) = (c.db, c.cond);
    let chiave = format!("{}|{}", oggi, giorni);
    let lista: Vec<Scadenza> = st.cache.get(*c.rev, &chiave, || db.scadenzario(cond, oggi, fino).unwrap_or_default()).clone();
    if lista.is_empty() {
        ui.colored_label(VERDE, "Nessuna scadenza nel periodo.");
    }
    egui::Grid::new("t_agenda").striped(true).num_columns(6).show(ui, |ui| {
        for h in ["Data", "Tipo", "Descrizione", "Dettaglio", "Importo", ""] {
            ui.strong(h);
        }
        ui.end_row();
        for s in &lista {
            let d = RichText::new(data_it(&s.data));
            ui.label(if s.scaduta { d.color(ROSSO).strong() } else { d });
            ui.label(nome_tipo(s.tipo));
            ui.label(&s.titolo);
            ui.label(&s.dettaglio);
            match s.importo {
                Some(i) => ui.label(format_eur(i)),
                None => ui.label(""),
            };
            if let Some(id) = s.agenda_id {
                ui.horizontal(|ui| {
                    if mini(ui, "Fatto") {
                        let r = c.db.completa_agenda(id, true);
                        c.esito(r, "Promemoria completato");
                    }
                    if mini(ui, "Elimina") {
                        c.chiedi("Eliminare il promemoria?", Azione::EliminaAgenda(id));
                    }
                });
            } else {
                ui.label("");
            }
            ui.end_row();
        }
    });
}
