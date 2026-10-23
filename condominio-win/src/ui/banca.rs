use super::*;
use crate::banca::{MovBanca, Proposta};

pub struct Stato {
    conto: Option<i64>,
    filtro: String, // "" = tutti
    aperto: Option<i64>,
    proposte: Vec<Proposta>,
    cache: Cache<Vec<MovBanca>>,
}

impl Default for Stato {
    fn default() -> Self {
        Stato { conto: None, filtro: "da_abbinare".into(), aperto: None, proposte: vec![], cache: Cache::default() }
    }
}

pub fn mostra(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    titolo(ui, "Banca e riconciliazione");
    let conti: Vec<(i64, String)> = c.db.elenco_conti_correnti(c.cond).unwrap_or_default().into_iter().map(|(i, n, _)| (i, n)).collect();
    if st.conto.is_none() {
        st.conto = conti.first().map(|x| x.0);
    }
    if conti.is_empty() {
        ui.colored_label(ARANCIO, "Aggiungi prima un conto corrente in «Conti e Movimenti».");
        return;
    }

    ui.group(|ui| {
        ui.label(RichText::new("Importa estratto conto (CSV)").strong());
        ui.label("Il file deve avere data, descrizione e importo (o colonne dare/avere). Separatore «;» o «,», date GG/MM/AAAA o AAAA-MM-GG. I movimenti già importati non vengono duplicati.");
        ui.horizontal(|ui| {
            ui.label("Conto");
            scegli(ui, "banca_cc", &mut st.conto, &conti, "");
            if ui.button("Scegli file e importa…").clicked() {
                if let (Some(cc), Some(p)) = (st.conto, rfd::FileDialog::new().add_filter("CSV / testo", &["csv", "txt"]).pick_file()) {
                    match std::fs::read(&p) {
                        Ok(b) => match c.db.importa_estratto(c.cond, cc, &String::from_utf8_lossy(&b)) {
                            Ok(e) => {
                                c.ok(&format!("Importati {} movimenti ({} già presenti, {} righe scartate)", e.nuovi, e.duplicati, e.scartate.len()));
                                if !e.scartate.is_empty() {
                                    c.errore(&format!("Righe scartate: {}", e.scartate.iter().take(3).cloned().collect::<Vec<_>>().join(" | ")));
                                }
                            }
                            Err(e) => c.err(e),
                        },
                        Err(e) => c.err(e.into()),
                    }
                }
            }
        });
    });

    ui.horizontal(|ui| {
        ui.label("Mostra");
        for (v, n) in [("da_abbinare", "Da abbinare"), ("riconciliato", "Riconciliati"), ("ignorato", "Ignorati"), ("", "Tutti")] {
            if ui.selectable_label(st.filtro == v, n).clicked() {
                st.filtro = v.to_string();
                st.aperto = None;
            }
        }
    });
    let (db, cond) = (c.db, c.cond);
    let filtro = st.filtro.clone();
    let lista: Vec<MovBanca> = st.cache.get(*c.rev, &filtro, || db.elenco_movimenti_banca(cond, if filtro.is_empty() { None } else { Some(filtro.as_str()) }).unwrap_or_default()).clone();
    if lista.is_empty() {
        ui.label("Nessun movimento.");
    }
    for m in lista {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(data_it(&m.data));
                eur_colorato(ui, m.importo);
                ui.label(&m.descrizione);
                let (col, t) = match m.stato.as_str() {
                    "riconciliato" => (VERDE, "riconciliato"),
                    "ignorato" => (egui::Color32::GRAY, "ignorato"),
                    _ => (ARANCIO, "da abbinare"),
                };
                ui.colored_label(col, t);
            });
            match m.stato.as_str() {
                "da_abbinare" => {
                    ui.horizontal(|ui| {
                        if mini(ui, "Suggerisci abbinamenti") {
                            match c.db.suggerisci_abbinamenti(m.id) {
                                Ok(p) => {
                                    if p.is_empty() {
                                        c.errore("Nessun abbinamento automatico trovato");
                                    }
                                    st.proposte = p;
                                    st.aperto = Some(m.id);
                                }
                                Err(e) => c.err(e),
                            }
                        }
                        if mini(ui, "Ignora") {
                            let r = c.db.ignora_movimento(m.id);
                            c.esito(r, "Movimento ignorato");
                        }
                    });
                    if st.aperto == Some(m.id) {
                        for p in st.proposte.clone() {
                            ui.horizontal(|ui| {
                                ui.label(format!("{}  (affinità {})", p.etichetta, p.punteggio));
                                if mini(ui, "Conferma") {
                                    let r = match p.tipo {
                                        "incasso" => c.db.riconcilia_incasso(m.id, p.riferimento_id).map(|_| ()),
                                        "fattura" => c.db.riconcilia_fattura(m.id, p.riferimento_id),
                                        _ => c.db.riconcilia_f24(m.id, p.codice_tributo.as_deref().unwrap_or("")),
                                    };
                                    if c.esito(r, "Movimento riconciliato").is_some() {
                                        st.aperto = None;
                                    }
                                }
                            });
                        }
                    }
                }
                "riconciliato" => {
                    if mini(ui, "Annulla riconciliazione") {
                        c.chiedi("Annullare la riconciliazione? L'incasso/pagamento collegato verrà stornato.", Azione::AnnullaRiconciliazione(m.id));
                    }
                }
                _ => {}
            }
        });
    }
}
