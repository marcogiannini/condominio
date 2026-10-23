//! Documenti & Archivio: verbali, contratti, planimetrie, polizze. Filtri per tipo e ricerca full-text (SQLite FTS5).

use crate::db::Db;
use anyhow::{bail, Result};
use rusqlite::params;
use std::path::Path;

pub const TIPI_DOCUMENTO: [&str; 7] = ["verbale", "contratto", "planimetria", "polizza", "fattura", "preventivo", "altro"];

/// Estensioni da cui viene estratto il testo per la ricerca full-text.
const ESTENSIONI_TESTO: [&str; 8] = ["txt", "md", "csv", "html", "htm", "xml", "json", "log"];

#[derive(Clone, Debug)]
pub struct Documento {
    pub id: i64,
    pub condominio_id: i64,
    pub tipo: String,
    pub titolo: String,
    pub descrizione: String,
    pub nome_file: String,
    pub percorso: String,
    pub data: String,
    pub dimensione: i64,
}

fn nome_sicuro(n: &str) -> String {
    n.chars().map(|c| if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' }).collect()
}

/// Trasforma l'input utente in una query FTS5 sicura (prefisso su ogni parola).
pub fn query_fts(q: &str) -> Option<String> {
    let termini: Vec<String> = q
        .split_whitespace()
        .map(|t| t.chars().filter(|c| c.is_alphanumeric()).collect::<String>())
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{}\"*", t))
        .collect();
    if termini.is_empty() {
        None
    } else {
        Some(termini.join(" "))
    }
}

impl Db {
    pub fn archivia_documento(
        &self,
        cond: i64,
        tipo: &str,
        titolo: &str,
        descrizione: &str,
        sorgente: &Path,
        data_iso: &str,
    ) -> Result<i64> {
        if titolo.trim().is_empty() {
            bail!("Il titolo del documento è obbligatorio");
        }
        if !sorgente.is_file() {
            bail!("File non trovato: {}", sorgente.display());
        }
        let nome = sorgente.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "documento".into());
        let cartella = self.archivio_dir.join(format!("condominio_{}", cond));
        std::fs::create_dir_all(&cartella)?;
        let stamp = chrono::Local::now().format("%Y%m%d%H%M%S%3f");
        let dest = cartella.join(format!("{}_{}", stamp, nome_sicuro(&nome)));
        std::fs::copy(sorgente, &dest)?;
        let dim = std::fs::metadata(&dest)?.len() as i64;

        let ext = sorgente.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let mut contenuto = nome.clone();
        if ESTENSIONI_TESTO.contains(&ext.as_str()) && dim <= 4 * 1024 * 1024 {
            if let Ok(b) = std::fs::read(&dest) {
                contenuto.push(' ');
                contenuto.push_str(&String::from_utf8_lossy(&b));
            }
        }
        let r = self.atomico(|db| {
            db.conn.execute(
                "INSERT INTO documento(condominio_id,tipo,titolo,descrizione,nome_file,percorso,data,dimensione,creato_il)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![cond, tipo, titolo.trim(), descrizione, nome, dest.to_string_lossy(), data_iso, dim, crate::util::oggi_iso()],
            )?;
            let id = db.conn.last_insert_rowid();
            db.conn.execute(
                "INSERT INTO documento_fts(rowid,titolo,descrizione,contenuto) VALUES(?1,?2,?3,?4)",
                params![id, titolo.trim(), descrizione, contenuto],
            )?;
            Ok(id)
        });
        if r.is_err() {
            let _ = std::fs::remove_file(&dest);
        }
        r
    }

    pub fn elenco_documenti(&self, cond: i64, tipo: Option<&str>, ricerca: &str) -> Result<Vec<Documento>> {
        let q = query_fts(ricerca);
        let mut st = self.conn.prepare(
            "SELECT id,condominio_id,tipo,titolo,descrizione,nome_file,percorso,data,dimensione FROM documento
             WHERE condominio_id=?1 AND (?2 IS NULL OR tipo=?2)
               AND (?3 IS NULL OR id IN (SELECT rowid FROM documento_fts WHERE documento_fts MATCH ?3))
             ORDER BY data DESC,id DESC",
        )?;
        let v = st
            .query_map(params![cond, tipo, q], |r| {
                Ok(Documento {
                    id: r.get(0)?,
                    condominio_id: r.get(1)?,
                    tipo: r.get(2)?,
                    titolo: r.get(3)?,
                    descrizione: r.get(4)?,
                    nome_file: r.get(5)?,
                    percorso: r.get(6)?,
                    data: r.get(7)?,
                    dimensione: r.get(8)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(v)
    }

    pub fn aggiorna_documento(&self, id: i64, tipo: &str, titolo: &str, descrizione: &str) -> Result<()> {
        if titolo.trim().is_empty() {
            bail!("Il titolo del documento è obbligatorio");
        }
        self.atomico(|db| {
            let contenuto: String =
                db.conn.query_row("SELECT contenuto FROM documento_fts WHERE rowid=?1", [id], |r| r.get(0)).unwrap_or_default();
            db.conn.execute("UPDATE documento SET tipo=?2,titolo=?3,descrizione=?4 WHERE id=?1", params![id, tipo, titolo.trim(), descrizione])?;
            db.conn.execute("DELETE FROM documento_fts WHERE rowid=?1", [id])?;
            db.conn.execute(
                "INSERT INTO documento_fts(rowid,titolo,descrizione,contenuto) VALUES(?1,?2,?3,?4)",
                params![id, titolo.trim(), descrizione, contenuto],
            )?;
            Ok(())
        })
    }

    pub fn elimina_documento(&self, id: i64) -> Result<()> {
        let percorso: String = self.conn.query_row("SELECT percorso FROM documento WHERE id=?1", [id], |r| r.get(0))?;
        self.atomico(|db| {
            db.conn.execute("DELETE FROM documento_fts WHERE rowid=?1", [id])?;
            db.conn.execute("DELETE FROM documento WHERE id=?1", [id])?;
            Ok(())
        })?;
        let _ = std::fs::remove_file(percorso);
        Ok(())
    }

    pub fn esporta_documento(&self, id: i64, destinazione: &Path) -> Result<()> {
        let percorso: String = self.conn.query_row("SELECT percorso FROM documento WHERE id=?1", [id], |r| r.get(0))?;
        std::fs::copy(percorso, destinazione)?;
        Ok(())
    }
}
