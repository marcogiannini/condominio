//! Agenda: pagina iniziale in stile Domustudio (intestazione dello studio + elenco delle scadenze).

use super::elenco::*;
use super::*;
use crate::scadenzario::Scadenza;
use chrono::Duration;

pub struct Stato {
    giorni: String,
    filtro: String,
    sel: Option<usize>,
    nuovo: Option<(String, String, String)>,
    cache: Cache<Vec<Scadenza>>,
}

impl Default for Stato {
    fn default() -> Self {
        Stato { giorni: "60".into(), filtro: String::new(), sel: None, nuovo: None, cache: Cache::default() }
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

const COLONNE: [Col; 4] = [col("Data", 90.0), col("Tipo", 140.0), col("Descrizione", 0.0), col_dx("Importo", 100.0)];

pub fn mostra(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    let giorni: i64 = st.giorni.trim().parse().unwrap_or(60).clamp(1, 730);
    let oggi = oggi();
    let fino = oggi + Duration::days(giorni);
    let (db, cond) = (c.db, c.cond);
    let chiave = format!("{}|{}", oggi, giorni);
    let lista: Vec<Scadenza> = st.cache.get(*c.rev, &chiave, || db.scadenzario(cond, oggi, fino).unwrap_or_default()).clone();
    let f = st.filtro.to_lowercase();
    let visibili: Vec<&Scadenza> = lista.iter().filter(|s| f.is_empty() || s.titolo.to_lowercase().contains(&f) || s.dettaglio.to_lowercase().contains(&f) || nome_tipo(s.tipo).to_lowercase().contains(&f)).collect();
    let righe: Vec<Riga> = visibili
        .iter()
        .map(|s| {
            let data = if s.scaduta { colorata(data_it(&s.data), ROSSO) } else { data_it(&s.data).into() };
            let descr = if s.dettaglio.is_empty() { s.titolo.clone() } else { format!("{} – {}", s.titolo, s.dettaglio) };
            Riga::new(vec![data, nome_tipo(s.tipo).into(), descr.into(), s.importo.map(format_eur).unwrap_or_default().into()])
        })
        .collect();
    if st.sel.map(|i| i >= righe.len()).unwrap_or(false) {
        st.sel = None;
    }
    let scelta: Option<&Scadenza> = st.sel.and_then(|i| visibili.get(i).copied());

    // ---- barra azioni
    barra_azioni(ui, "az_agenda", |ui| {
        if azione(ui, "📝", ARANCIO_ICONA, "Nuovo memo") {
            st.nuovo = Some((data_it(&oggi_iso()), String::new(), String::new()));
        }
        if azione(ui, "✔", VERDE_ICONA, "Fatto") {
            match scelta.and_then(|s| s.agenda_id) {
                Some(id) => {
                    let r = c.db.completa_agenda(id, true);
                    c.esito(r, "Promemoria completato");
                }
                None => c.errore("Seleziona un promemoria (le altre scadenze si chiudono da sole quando l'operazione viene registrata)"),
            }
        }
        if azione(ui, "✖", ROSSO_ICONA, "Elimina") {
            match scelta.and_then(|s| s.agenda_id) {
                Some(id) => c.chiedi("Eliminare il promemoria?", Azione::EliminaAgenda(id)),
                None => c.errore("Si possono eliminare solo i promemoria inseriti a mano"),
            }
        }
        if azione(ui, "🖨", BLU, "Stampa") {
            match c.db.html_scadenzario(c.cond, oggi, fino) {
                Ok(h) => apri_html(c, "scadenzario", &h),
                Err(e) => c.err(e),
            }
        }
        if azione(ui, "📗", VERDE_ICONA, "Excel") {
            esporta_csv(c, "agenda", &COLONNE, &righe);
        }
    });

    // ---- finestra nuovo memo
    let mut chiudi = false;
    if let Some((data, titolo_m, note)) = &mut st.nuovo {
        egui::Window::new("Nuovo memo").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ui.ctx(), |ui| {
            grid_scheda("g_memo").show(ui, |ui| {
                campo_pieno(ui, "Data", data);
                campo_pieno(ui, "Titolo *", titolo_m);
                campo_pieno(ui, "Note", note);
            });
            ui.horizontal(|ui| {
                if ui.button("Aggiungi").clicked() {
                    let r = c.db.aggiungi_agenda(c.cond, data, titolo_m, note);
                    if c.esito(r, "Promemoria aggiunto").is_some() {
                        chiudi = true;
                    }
                }
                if ui.button("Annulla").clicked() {
                    chiudi = true;
                }
            });
        });
    }
    if chiudi {
        st.nuovo = None;
    }

    // ---- intestazione
    let studio = c.db.leggi_imp("studio");
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(14.0);
        ui.label(RichText::new("🏢").size(54.0).color(BLU));
        ui.add_space(8.0);
        ui.vertical(|ui| {
            ui.label(RichText::new(if studio.trim().is_empty() { "Gestione Condominio" } else { studio.trim() }).size(26.0).strong());
            ui.label(RichText::new("Amministrazioni condominiali").size(17.0).color(Color32::from_rgb(110, 110, 110)));
            ui.horizontal(|ui| {
                if ui.link("Hai bisogno di aiuto?").clicked() {
                    *c.app = Some(AzioneApp::Guida);
                }
                let n = c.db.notifiche_non_lette(c.cond).unwrap_or(0);
                if n > 0 {
                    ui.add_space(12.0);
                    if ui.link(RichText::new(format!("{} notifiche da leggere", n)).color(ARANCIO)).clicked() {
                        *c.app = Some(AzioneApp::Vai(Sezione::Dashboard, None));
                    }
                }
            });
        });
    });
    ui.add_space(8.0);
    ui.separator();

    // ---- filtri + elenco
    barra_filtri(ui, Some(&mut st.filtro), |ui| {
        ui.label(RichText::new("Agenda").strong().size(16.0).color(Color32::from_rgb(120, 120, 120)));
        ui.add_space(6.0);
        ui.label("Prossimi");
        ui.add(egui::TextEdit::singleline(&mut st.giorni).desired_width(40.0));
        ui.label("giorni (le scadute non evase restano sempre in elenco)");
    });
    if lista.is_empty() {
        ui.colored_label(VERDE, "Nessuna scadenza nel periodo.");
    }
    let esito = elenco(ui, "t_agenda", &COLONNE, &righe, st.sel, None);
    if let Some(i) = esito.clic {
        st.sel = Some(i);
    }
    if let Some(i) = esito.doppio {
        let dest = match visibili[i].tipo {
            "rata" => Some((Sezione::Rate, None)),
            "fattura" => Some((Sezione::Fornitori, Some(1))),
            "ritenute" => Some((Sezione::Fornitori, Some(2))),
            "prescrizione" => Some((Sezione::Conformita, Some(1))),
            "assemblea" => Some((Sezione::Assemblee, None)),
            "fiscale" => Some((Sezione::Fisco, Some(0))),
            _ => None,
        };
        if let Some((s, t)) = dest {
            *c.app = Some(AzioneApp::Vai(s, t));
        }
    }
}
