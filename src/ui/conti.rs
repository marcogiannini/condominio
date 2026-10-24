//! Piano dei conti con preventivo e consuntivo: albero capitoli → voci a sinistra, «Impostazioni» e «Movimenti» a destra.

use super::elenco::*;
use super::*;
use crate::preventivo::*;
use std::collections::HashMap;

pub struct Stato {
    sel: Option<i64>,
    tab: usize,
    form: VoceSpesa,
    importo_txt: String,
    nuova: bool,
    caricata: Option<i64>,
}

impl Default for Stato {
    fn default() -> Self {
        Stato { sel: None, tab: 0, form: VoceSpesa::default(), importo_txt: String::new(), nuova: false, caricata: None }
    }
}

const COLONNE: [Col; 3] = [col("Conti", 0.0), col_dx("Preventivo", 110.0), col_dx("Consuntivo", 110.0)];

fn profondita(v: &VoceSpesa, mappa: &HashMap<i64, &VoceSpesa>) -> usize {
    let mut d = 0;
    let mut cur = v.parent_id;
    while let Some(p) = cur {
        d += 1;
        cur = mappa.get(&p).and_then(|x| x.parent_id);
        if d > 8 {
            break;
        }
    }
    d
}

fn totale_ramo(id: i64, voci: &[VoceSpesa]) -> Cents {
    let figli: Vec<&VoceSpesa> = voci.iter().filter(|v| v.parent_id == Some(id)).collect();
    if figli.is_empty() {
        voci.iter().find(|v| v.id == id).map(|v| v.importo_cents).unwrap_or(0)
    } else {
        figli.iter().map(|f| totale_ramo(f.id, voci)).sum()
    }
}

fn consuntivo_ramo(c: &Ctx, id: i64, voci: &[VoceSpesa]) -> Cents {
    let figli: Vec<&VoceSpesa> = voci.iter().filter(|v| v.parent_id == Some(id)).collect();
    if figli.is_empty() {
        c.db.consuntivo_voce(id).unwrap_or(0)
    } else {
        figli.iter().map(|f| consuntivo_ramo(c, f.id, voci)).sum()
    }
}

/// Le voci in ordine ad albero: ogni capitolo seguito dai suoi figli.
fn ordina_albero(voci: &[VoceSpesa]) -> Vec<&VoceSpesa> {
    fn aggiungi<'a>(voci: &'a [VoceSpesa], padre: Option<i64>, out: &mut Vec<&'a VoceSpesa>, livello: usize) {
        if livello > 8 {
            return;
        }
        for v in voci.iter().filter(|v| v.parent_id == padre) {
            out.push(v);
            aggiungi(voci, Some(v.id), out, livello + 1);
        }
    }
    let mut out = vec![];
    aggiungi(voci, None, &mut out, 0);
    // eventuali voci con padre inesistente non devono sparire
    for v in voci {
        if !out.iter().any(|x| x.id == v.id) {
            out.push(v);
        }
    }
    out
}

pub fn mostra(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    let Some(es) = c.esercizio else {
        ui.add_space(20.0);
        ui.label("Crea o seleziona un esercizio (pulsante Condomìni → Nuovo esercizio).");
        return;
    };
    let lista = c.db.elenco_voci(es).unwrap_or_default();
    let tabelle: Vec<(i64, String)> = c.db.elenco_tabelle(c.cond).unwrap_or_default().iter().map(|t| (t.id, format!("{} – {}", t.codice, t.nome))).collect();
    let mappa: HashMap<i64, &VoceSpesa> = lista.iter().map(|v| (v.id, v)).collect();
    let albero = ordina_albero(&lista);

    let mut righe: Vec<Riga> = vec![];
    let mut tot_prev: Cents = 0;
    let mut tot_cons: Cents = 0;
    for v in &albero {
        let ha_figli = lista.iter().any(|x| x.parent_id == Some(v.id));
        let prev = totale_ramo(v.id, &lista);
        let cons = consuntivo_ramo(c, v.id, &lista);
        if !ha_figli {
            tot_prev += prev;
            tot_cons += cons;
        }
        let d = profondita(v, &mappa);
        let mut riga = Riga::new(vec![
            format!("{} {}", v.codice, v.descrizione).into(),
            if prev == 0 { colorata("0,00", Color32::from_rgb(120, 120, 120)) } else { cella_eur(prev) },
            if cons == 0 {
                colorata("0,00", Color32::from_rgb(120, 120, 120))
            } else if cons > prev && prev > 0 {
                colorata(format_eur(cons), ROSSO)
            } else {
                format_eur(cons).into()
            },
        ])
        .rientrata(d as f32 * 14.0);
        if ha_figli {
            riga = riga.grassetto();
        }
        righe.push(riga);
    }
    let piede = Riga::new(vec![forte("Totale gestione"), forte(format_eur(tot_prev)), forte(format_eur(tot_cons))]);
    let selezionata = st.sel.and_then(|id| albero.iter().position(|v| v.id == id));

    if st.sel.is_none() && !st.nuova {
        st.sel = albero.first().map(|v| v.id);
    }
    if !st.nuova && st.caricata != st.sel {
        match st.sel.and_then(|id| lista.iter().find(|v| v.id == id)) {
            Some(v) => {
                st.form = v.clone();
                st.importo_txt = format_eur(v.importo_cents).trim_end_matches(" €").to_string();
            }
            None => st.form = VoceSpesa::default(),
        }
        st.caricata = st.sel;
    }

    barra_azioni(ui, "az_conti", |ui| {
        if azione(ui, "📄", BLU, "Nuovo") {
            st.nuova = true;
            st.sel = None;
            st.form = VoceSpesa { esercizio_id: es, ..Default::default() };
            st.importo_txt.clear();
            st.tab = 0;
        }
        if azione(ui, "✖", ROSSO_ICONA, "Elimina") {
            if let Some(v) = st.sel.and_then(|id| lista.iter().find(|v| v.id == id)) {
                c.chiedi(&format!("Eliminare la voce {}?", v.codice), Azione::EliminaVoce(v.id));
            }
        }
        if azione(ui, "📊", ARANCIO_ICONA, "Riparti") {
            *c.app = Some(AzioneApp::Vai(Sezione::Preventivo, Some(3)));
        }
        if azione(ui, "🖨", BLU, "Stampa") {
            match c.db.html_rendiconto(es) {
                Ok(h) => apri_html(c, "rendiconto", &h),
                Err(e) => c.err(e),
            }
        }
        if azione(ui, "📗", VERDE_ICONA, "Excel") {
            esporta_csv(c, "piano_dei_conti", &COLONNE, &righe);
        }
    });

    pannello_dettaglio(ui, "dett_conti", 480.0, |ui| {
        schede(ui, &mut st.tab, &["Impostazioni", "Movimenti"]);
        ui.add_space(8.0);
        if st.tab == 0 {
            ui.label(RichText::new(if st.form.id == 0 { "Nuova voce di spesa" } else { "Voce di spesa" }).strong());
            let candidati: Vec<(i64, String)> = lista.iter().filter(|v| v.id != st.form.id).map(|v| (v.id, format!("{} {}", v.codice, v.descrizione))).collect();
            grid_scheda("g_voce").show(ui, |ui| {
                ui.label("Capitolo");
                scegli(ui, "v_parent", &mut st.form.parent_id, &candidati, "— nessuno (è un capitolo) —");
                ui.end_row();
                campo_pieno(ui, "Codice *", &mut st.form.codice);
                campo_pieno(ui, "Descrizione *", &mut st.form.descrizione);
                campo_pieno(ui, "Preventivo (€)", &mut st.importo_txt);
                ui.label("Tabella di ripartizione");
                scegli(ui, "v_tab", &mut st.form.tabella_id, &tabelle, "— nessuna —");
                ui.end_row();
                ui.label("");
                ui.checkbox(&mut st.form.sopravvenienza, "Spesa fuori preventivo (art. 1130-bis c.c.)");
                ui.end_row();
            });
            ui.add_space(4.0);
            if ui.button("Salva voce").clicked() {
                let imp = if st.importo_txt.trim().is_empty() { Some(0) } else { parse_eur(&st.importo_txt) };
                match imp {
                    Some(i) => {
                        st.form.esercizio_id = es;
                        st.form.importo_cents = i;
                        let r = c.db.salva_voce(&st.form);
                        if let Some(id) = c.esito(r, "Voce salvata") {
                            st.sel = Some(id);
                            st.nuova = false;
                            st.caricata = None;
                        }
                    }
                    None => c.errore("Importo non valido"),
                }
            }
            if st.form.id != 0 {
                let in_piano: i64 = c
                    .db
                    .conn
                    .query_row("SELECT COUNT(*) FROM piano_voce pv JOIN piano_rate p ON p.id=pv.piano_id WHERE pv.voce_id=?1 AND p.stato='attivo'", [st.form.id], |r| r.get(0))
                    .unwrap_or(0);
                if in_piano > 0 {
                    ui.add_space(4.0);
                    ui.colored_label(VERDE, "Voce collegata a un piano rate attivo: importo e tabella non sono più modificabili.");
                }
            }
        } else {
            if st.form.id == 0 {
                ui.label("Seleziona una voce per vederne i movimenti.");
                return;
            }
            let mov: Vec<(String, String, Cents)> = (|| -> rusqlite::Result<_> {
                let mut q = c.db.conn.prepare(
                    "SELECT f.data, fo.ragione_sociale || ' – ' || f.descrizione, f.imponibile_cents+f.iva_cents FROM fattura f
                       JOIN fornitore fo ON fo.id=f.fornitore_id WHERE f.voce_id=?1 AND f.stato<>'stornata'
                     UNION ALL
                     SELECT data, descrizione, -importo_cents FROM movimento_gestione WHERE voce_id=?1 AND origine='manuale' AND stornato=0
                     ORDER BY 1",
                )?;
                let v = q.query_map([st.form.id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<Result<Vec<_>, _>>()?;
                Ok(v)
            })()
            .unwrap_or_default();
            if mov.is_empty() {
                ui.label("Nessuna fattura o movimento imputato a questa voce.");
            }
            egui::Grid::new("g_mov_voce").striped(true).num_columns(3).spacing([10.0, 4.0]).show(ui, |ui| {
                for (d, t, i) in mov {
                    ui.label(data_it(&d));
                    ui.label(t);
                    eur_colorato(ui, i);
                    ui.end_row();
                }
            });
        }
    });

    barra_filtri(ui, None, |ui| {
        ui.label(format!("Esercizio {} – spese per capitolo e voce", c.db.elenco_esercizi(c.cond).unwrap_or_default().iter().find(|e| e.id == es).map(|e| e.nome.clone()).unwrap_or_default()));
    });
    let esito = elenco(ui, "t_conti", &COLONNE, &righe, selezionata, Some(&piede));
    if let Some(i) = esito.clic {
        st.sel = Some(albero[i].id);
        st.nuova = false;
    }
}
