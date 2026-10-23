use super::*;
use crate::documenti::{Documento, TIPI_DOCUMENTO};
use std::path::PathBuf;

pub struct Stato {
    filtro_tipo: String, // "" = tutti
    ricerca: String,
    nuovo_tipo: String,
    nuovo_titolo: String,
    nuovo_descr: String,
    nuovo_data: String,
    nuovo_file: Option<PathBuf>,
    modifica: Option<(i64, String, String, String)>,
    cache: Cache<Vec<Documento>>,
}

impl Default for Stato {
    fn default() -> Self {
        Stato {
            filtro_tipo: String::new(),
            ricerca: String::new(),
            nuovo_tipo: "verbale".into(),
            nuovo_titolo: String::new(),
            nuovo_descr: String::new(),
            nuovo_data: data_it(&oggi_iso()),
            nuovo_file: None,
            modifica: None,
            cache: Cache::default(),
        }
    }
}

fn dimensione(b: i64) -> String {
    if b >= 1024 * 1024 {
        format!("{:.1} MB", b as f64 / 1048576.0)
    } else {
        format!("{:.0} KB", (b as f64 / 1024.0).max(1.0))
    }
}

pub fn mostra(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    titolo(ui, "Documenti e Archivio");

    ui.collapsing("Archivia un nuovo documento", |ui| {
        egui::Grid::new("g_doc_nuovo").num_columns(2).show(ui, |ui| {
            ui.label("Tipo");
            scegli_str(ui, "doc_tipo", &mut st.nuovo_tipo, &TIPI_DOCUMENTO);
            ui.end_row();
            campo(ui, "Titolo *", &mut st.nuovo_titolo);
            campo(ui, "Data (GG/MM/AAAA)", &mut st.nuovo_data);
            campo_lungo(ui, "Descrizione", &mut st.nuovo_descr, 2);
            ui.label("File");
            ui.horizontal(|ui| {
                if ui.button("Scegli file…").clicked() {
                    if let Some(p) = rfd::FileDialog::new().pick_file() {
                        if st.nuovo_titolo.trim().is_empty() {
                            st.nuovo_titolo = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                        }
                        st.nuovo_file = Some(p);
                    }
                }
                match &st.nuovo_file {
                    Some(p) => ui.label(p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()),
                    None => ui.label("nessun file"),
                };
            });
            ui.end_row();
        });
        if ui.button("Archivia").clicked() {
            let Some(data) = data_iso(&st.nuovo_data) else {
                c.errore("Data non valida (usa GG/MM/AAAA)");
                return;
            };
            let Some(file) = st.nuovo_file.clone() else {
                c.errore("Scegli il file da archiviare");
                return;
            };
            let r = c.db.archivia_documento(c.cond, &st.nuovo_tipo, &st.nuovo_titolo, &st.nuovo_descr, &file, &data);
            if c.esito(r, "Documento archiviato").is_some() {
                st.nuovo_titolo.clear();
                st.nuovo_descr.clear();
                st.nuovo_file = None;
            }
        }
    });

    ui.separator();
    ui.horizontal(|ui| {
        ui.label("Tipo");
        egui::ComboBox::from_id_salt("doc_filtro").selected_text(if st.filtro_tipo.is_empty() { "Tutti".to_string() } else { st.filtro_tipo.clone() }).show_ui(ui, |ui| {
            if ui.selectable_label(st.filtro_tipo.is_empty(), "Tutti").clicked() {
                st.filtro_tipo.clear();
            }
            for t in TIPI_DOCUMENTO {
                if ui.selectable_label(st.filtro_tipo == t, t).clicked() {
                    st.filtro_tipo = t.to_string();
                }
            }
        });
        ui.label("Cerca (titolo, descrizione, contenuto)");
        ui.add(egui::TextEdit::singleline(&mut st.ricerca).desired_width(260.0));
    });

    let (db, cond) = (c.db, c.cond);
    let chiave = format!("{}|{}", st.filtro_tipo, st.ricerca);
    let tipo = if st.filtro_tipo.is_empty() { None } else { Some(st.filtro_tipo.clone()) };
    let ricerca = st.ricerca.clone();
    let docs: Vec<Documento> = st.cache.get(*c.rev, &chiave, || db.elenco_documenti(cond, tipo.as_deref(), &ricerca).unwrap_or_default()).clone();

    if docs.is_empty() {
        ui.label("Nessun documento trovato.");
    }
    egui::Grid::new("t_doc").striped(true).num_columns(6).show(ui, |ui| {
        for h in ["Data", "Tipo", "Titolo", "File", "Dim.", "Azioni"] {
            ui.strong(h);
        }
        ui.end_row();
        for d in &docs {
            ui.label(data_it(&d.data));
            ui.label(&d.tipo);
            ui.vertical(|ui| {
                ui.label(RichText::new(&d.titolo).strong());
                if !d.descrizione.is_empty() {
                    ui.label(RichText::new(&d.descrizione).small());
                }
            });
            ui.label(&d.nome_file);
            ui.label(dimensione(d.dimensione));
            ui.horizontal(|ui| {
                if mini(ui, "Esporta") {
                    if let Some(dest) = rfd::FileDialog::new().set_file_name(&d.nome_file).save_file() {
                        let r = c.db.esporta_documento(d.id, &dest);
                        c.esito(r, "Documento esportato");
                    }
                }
                if mini(ui, "Modifica") {
                    st.modifica = Some((d.id, d.tipo.clone(), d.titolo.clone(), d.descrizione.clone()));
                }
                if mini(ui, "Elimina") {
                    c.chiedi(&format!("Eliminare il documento «{}»?", d.titolo), Azione::EliminaDocumento(d.id));
                }
            });
            ui.end_row();
        }
    });

    let mut salva = false;
    let mut chiudi = false;
    if let Some((_, tipo, tit, descr)) = &mut st.modifica {
        egui::Window::new("Modifica documento").collapsible(false).show(ui.ctx(), |ui| {
            egui::Grid::new("g_doc_mod").num_columns(2).show(ui, |ui| {
                ui.label("Tipo");
                scegli_str(ui, "doc_tipo_mod", tipo, &TIPI_DOCUMENTO);
                ui.end_row();
                campo(ui, "Titolo *", tit);
                campo_lungo(ui, "Descrizione", descr, 3);
            });
            ui.horizontal(|ui| {
                salva = ui.button("Salva").clicked();
                chiudi = ui.button("Annulla").clicked();
            });
        });
    }
    if salva {
        if let Some((id, tipo, tit, descr)) = st.modifica.clone() {
            let r = c.db.aggiorna_documento(id, &tipo, &tit, &descr);
            if c.esito(r, "Documento aggiornato").is_some() {
                st.modifica = None;
            }
        }
    }
    if chiudi {
        st.modifica = None;
    }
}
