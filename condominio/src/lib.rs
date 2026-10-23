//! Gestionale condominiale: nucleo logico (database, contabilità, regole di legge).

pub mod anagrafica;
pub mod assemblee;
pub mod banca;
pub mod bacheca;
pub mod consuntivo;
pub mod db;
pub mod demo;
pub mod documenti;
pub mod fatturapa;
pub mod fiscale;
pub mod fornitori;
pub mod importa;
pub mod incassi;
pub mod legale;
pub mod mora;
pub mod movimenti;
pub mod pratiche;
pub mod preventivo;
pub mod riparti;
pub mod scadenzario;
pub mod segnalazioni;
pub mod stampe;
pub mod ui;
pub mod util;

pub use db::Db;
