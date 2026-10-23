#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

use condominio::Db;
use std::path::PathBuf;

/// Modalità portatile: se accanto all'eseguibile c'è il file `portable.txt`
/// (o la cartella `dati`), database e archivio vivono lì, es. su una chiavetta USB.
fn cartella_dati() -> PathBuf {
    if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
        if dir.join("portable.txt").exists() || dir.join("dati").is_dir() {
            return dir.join("dati");
        }
    }
    directories::ProjectDirs::from("it", "gestionale", "condominio")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("dati"))
}

fn main() {
    let base = cartella_dati();
    let db = match Db::apri(&base.join("condominio.db"), &base.join("archivio")) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Impossibile aprire il database: {e:#}");
            std::process::exit(1);
        }
    };
    if let Err(e) = condominio::ui::avvia(db) {
        eprintln!("Errore interfaccia grafica: {e}");
        std::process::exit(1);
    }
}
