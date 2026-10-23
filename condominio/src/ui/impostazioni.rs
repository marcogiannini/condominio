use super::*;
use crate::bacheca::ConfigSmtp;

#[derive(Default)]
pub struct Stato {
    smtp: Option<ConfigSmtp>,
    porta: String,
    conferma_elimina: bool,
}


pub fn mostra(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    titolo(ui, "Impostazioni e Backup");

    ui.label(RichText::new("Dati").strong());
    if let Some(p) = &c.db.percorso {
        ui.label(format!("Database: {}", p.display()));
    }
    ui.label(format!("Archivio documenti: {}", c.db.archivio_dir.display()));
    ui.horizontal(|ui| {
        if ui.button("Crea backup del database…").clicked() {
            let nome = format!("condominio_backup_{}.db", oggi_iso());
            if let Some(dest) = rfd::FileDialog::new().set_file_name(&nome).save_file() {
                let r = c.db.backup(&dest);
                c.esito(r, "Backup creato");
            }
        }
    });
    ui.label(RichText::new("Per un backup completo copia anche la cartella dell'archivio documenti.").small());

    ui.separator();
    ui.label(RichText::new("Server e-mail (SMTP) per le comunicazioni").strong());
    if st.smtp.is_none() {
        let cfg = c.db.carica_smtp();
        st.porta = cfg.porta.to_string();
        st.smtp = Some(cfg);
    }
    if let Some(cfg) = &mut st.smtp {
        egui::Grid::new("g_smtp").num_columns(2).show(ui, |ui| {
            campo(ui, "Server (host)", &mut cfg.host);
            campo(ui, "Porta", &mut st.porta);
            campo(ui, "Utente", &mut cfg.utente);
            ui.label("Password");
            ui.add(egui::TextEdit::singleline(&mut cfg.password).password(true).desired_width(240.0));
            ui.end_row();
            campo(ui, "Mittente (es. Nome <a@b.it>)", &mut cfg.mittente);
            ui.label("Connessione");
            ui.checkbox(&mut cfg.starttls, "STARTTLS (porta 587); disattiva per TLS diretto (porta 465)");
            ui.end_row();
        });
        if ui.button("Salva impostazioni e-mail").clicked() {
            match st.porta.trim().parse::<u16>() {
                Ok(p) => {
                    cfg.porta = p;
                    let r = c.db.salva_smtp(cfg);
                    c.esito(r, "Impostazioni e-mail salvate");
                }
                Err(_) => c.errore("Porta non valida"),
            }
        }
        ui.label(RichText::new("La password è memorizzata nel database locale: proteggi il file e i backup.").small());
    }

    ui.separator();
    ui.label(RichText::new("Zona pericolosa").strong().color(ROSSO));
    ui.checkbox(&mut st.conferma_elimina, "Voglio eliminare il condominio selezionato con tutti i suoi dati");
    ui.add_enabled_ui(st.conferma_elimina, |ui| {
        if ui.button(RichText::new(format!("Elimina «{}»", c.cond_nome)).color(ROSSO)).clicked() {
            c.chiedi(&format!("Eliminare definitivamente il condominio «{}» e tutti i suoi dati? Fai prima un backup.", c.cond_nome), Azione::EliminaCondominio(c.cond));
            st.conferma_elimina = false;
        }
    });
}
