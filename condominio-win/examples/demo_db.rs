//! Crea un database con il condominio di esempio: `cargo run --example demo_db -- percorso/condominio.db`
fn main() {
    let p = std::env::args().nth(1).expect("percorso del database");
    let p = std::path::PathBuf::from(p);
    let arch = p.with_file_name("archivio");
    let db = condominio::Db::apri(&p, &arch).unwrap();
    db.carica_demo().unwrap();
}
