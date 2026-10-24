//! Unità immobiliari: elenco a sinistra, scheda («Dati generali», «Saldi», «Varie») a destra.

use super::elenco::*;
use super::*;
use crate::anagrafica::*;

pub struct Stato {
    sel: Option<i64>,
    tab: usize,
    form: Unita,
    nuova: bool,
    tit_persona: Option<i64>,
    tit_ruolo: String,
    tit_quota: String,
    tit_paga: bool,
    filtro: String,
}

impl Default for Stato {
    fn default() -> Self {
        Stato {
            sel: None,
            tab: 0,
            form: Unita::default(),
            nuova: false,
            tit_persona: None,
            tit_ruolo: "proprietario".into(),
            tit_quota: "100".into(),
            tit_paga: false,
            filtro: String::new(),
        }
    }
}

fn sigla(u: &Unita) -> String {
    let mut p = vec![];
    if !u.scala.is_empty() {
        p.push(format!("Sc.{}", u.scala));
    }
    p.push(format!("Int.{}", if u.interno.is_empty() { "-" } else { &u.interno }));
    p.join(" ")
}

const COLONNE: [Col; 8] = [
    col("Unità", 90.0),
    col("Tipo", 110.0),
    col("Proprietario", 0.0),
    col("Conduttore", 170.0),
    col("Usufruttuario", 140.0),
    col("Interno", 60.0),
    col("Piano", 55.0),
    col("Dati catastali", 190.0),
];

fn nomi_ruolo(tit: &[Titolarita], ruolo: &str) -> String {
    tit.iter().filter(|t| t.ruolo == ruolo).map(|t| t.persona.clone()).collect::<Vec<_>>().join(" - ")
}

pub fn mostra(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    let lista = c.db.elenco_unita(c.cond).unwrap_or_default();
    let titolari: Vec<Vec<Titolarita>> = lista.iter().map(|u| c.db.elenco_titolari(u.id).unwrap_or_default()).collect();
    let f = st.filtro.to_lowercase();
    let visibili: Vec<usize> = (0..lista.len())
        .filter(|&i| {
            f.is_empty()
                || sigla(&lista[i]).to_lowercase().contains(&f)
                || lista[i].tipo.contains(&f)
                || titolari[i].iter().any(|t| t.persona.to_lowercase().contains(&f))
        })
        .collect();
    let righe: Vec<Riga> = visibili
        .iter()
        .map(|&i| {
            let u = &lista[i];
            let t = &titolari[i];
            Riga::new(vec![
                sigla(u).into(),
                u.tipo.clone().into(),
                nomi_ruolo(t, "proprietario").into(),
                nomi_ruolo(t, "inquilino").into(),
                nomi_ruolo(t, "usufruttuario").into(),
                u.interno.clone().into(),
                u.piano.clone().into(),
                if u.dati_catastali.is_empty() { "Unità catastalmente non censita".to_string() } else { u.dati_catastali.clone() }.into(),
            ])
        })
        .collect();
    let selezionata = st.sel.and_then(|id| visibili.iter().position(|&i| lista[i].id == id));

    // la scheda segue la selezione
    if !st.nuova && st.sel.unwrap_or(0) != st.form.id {
        st.form = st.sel.and_then(|id| lista.iter().find(|u| u.id == id)).cloned().unwrap_or_default();
    }
    if st.sel.is_none() && !st.nuova {
        if let Some(u) = lista.first() {
            st.sel = Some(u.id);
            st.form = u.clone();
        }
    }

    barra_azioni(ui, "az_unita", |ui| {
        if azione(ui, "📄", BLU, "Nuovo") {
            st.nuova = true;
            st.sel = None;
            st.form = Unita { tipo: "appartamento".into(), ..Default::default() };
            st.tab = 0;
        }
        if azione(ui, "✖", ROSSO_ICONA, "Elimina") {
            if let Some(u) = st.sel.and_then(|id| lista.iter().find(|u| u.id == id)) {
                c.chiedi(&format!("Eliminare l'unità {}?", sigla(u)), Azione::EliminaUnita(u.id));
            }
        }
        if azione(ui, "📗", VERDE_ICONA, "Excel") {
            esporta_csv(c, "unita", &COLONNE, &righe);
        }
        if azione(ui, "📥", ARANCIO_ICONA, "Importa CSV") {
            importa_csv(c);
        }
    });

    pannello_dettaglio(ui, "dett_unita", 430.0, |ui| scheda(c, st, ui, &lista));

    barra_filtri(ui, Some(&mut st.filtro), |ui| {
        ui.label(format!("Unità ({})", lista.len()));
    });
    let esito = elenco(ui, "t_unita", &COLONNE, &righe, selezionata, None);
    if let Some(i) = esito.clic {
        st.sel = Some(lista[visibili[i]].id);
        st.nuova = false;
    }
}

pub fn importa_csv(c: &mut Ctx) {
    if let Some(p) = rfd::FileDialog::new().add_filter("CSV / testo", &["csv", "txt"]).pick_file() {
        match std::fs::read(&p).map_err(anyhow::Error::from).and_then(|b| c.db.importa_anagrafica_csv(c.cond, &String::from_utf8_lossy(&b))) {
            Ok(e) => {
                c.ok(&format!("Importate {} unità, {} persone, {} titolarità, {} tabelle ({} avvisi)", e.unita_create, e.persone_create, e.titolarita, e.tabelle_create, e.avvisi.len()));
                if !e.avvisi.is_empty() {
                    c.errore(&format!("Avvisi: {}", e.avvisi.iter().take(4).cloned().collect::<Vec<_>>().join(" | ")));
                }
            }
            Err(e) => c.err(e),
        }
    }
}

fn scheda(c: &mut Ctx, st: &mut Stato, ui: &mut Ui, lista: &[Unita]) {
    schede(ui, &mut st.tab, &["Dati generali", "Saldi", "Varie"]);
    ui.add_space(8.0);
    match st.tab {
        0 => {
            ui.label(RichText::new(if st.form.id == 0 { "Nuova unità immobiliare" } else { "Unità" }).strong());
            grid_scheda("g_unita").show(ui, |ui| {
                campo_pieno(ui, "Scala", &mut st.form.scala);
                campo_pieno(ui, "Piano", &mut st.form.piano);
                campo_pieno(ui, "Interno", &mut st.form.interno);
                ui.label("Tipo");
                if st.form.tipo.is_empty() {
                    st.form.tipo = "appartamento".into();
                }
                scegli_str(ui, "u_tipo", &mut st.form.tipo, &TIPI_UNITA);
                ui.end_row();
                campo_pieno(ui, "Dati catastali", &mut st.form.dati_catastali);
            });
            if ui.button("Salva unità").clicked() {
                st.form.condominio_id = c.cond;
                let r = c.db.salva_unita(&st.form);
                if let Some(id) = c.esito(r, "Unità salvata") {
                    st.sel = Some(id);
                    st.nuova = false;
                }
            }
            let Some(u) = st.sel.and_then(|id| lista.iter().find(|u| u.id == id)) else { return };
            ui.add_space(10.0);
            ui.label(RichText::new("Anagrafiche dell'unità").strong());
            let tit = c.db.elenco_titolari(u.id).unwrap_or_default();
            if tit.is_empty() {
                ui.label(RichText::new("Nessun titolare: aggiungi un soggetto qui sotto.").weak());
            }
            grid_scheda("t_tit").show(ui, |ui| {
                for t in &tit {
                    ui.label(RichText::new(&t.ruolo).color(Color32::from_rgb(20, 60, 160)).strong());
                    ui.horizontal(|ui| {
                        ui.label(format!("{}  ({:.2}%){}", t.persona, t.quota_ppm as f64 / 10_000.0, if t.paga_spese { "" } else { " – non paga" }));
                        if mini(ui, "Rimuovi") {
                            c.chiedi(&format!("Rimuovere {} dall'unità?", t.persona), Azione::RimuoviTitolarita(t.id));
                        }
                    });
                    ui.end_row();
                }
            });
            let paganti = c.db.pagatori_unita(u.id).unwrap_or_default();
            let somma: i64 = paganti.iter().map(|x| x.1).sum();
            if !paganti.is_empty() && somma != 1_000_000 {
                ui.colored_label(ARANCIO, format!("Le quote dei paganti sommano {:.2}% (dovrebbero fare 100%)", somma as f64 / 10_000.0));
            }
            ui.add_space(6.0);
            ui.collapsing("Aggiungi soggetto", |ui| {
                let persone: Vec<(i64, String)> = c.db.elenco_persone().unwrap_or_default().iter().map(|p| (p.id, etichetta_persona(p))).collect();
                grid_scheda("g_addtit").show(ui, |ui| {
                    ui.label("Persona");
                    scegli(ui, "tit_p", &mut st.tit_persona, &persone, "");
                    ui.end_row();
                    ui.label("Ruolo");
                    scegli_str(ui, "tit_r", &mut st.tit_ruolo, &RUOLI);
                    ui.end_row();
                    campo_pieno(ui, "Quota %", &mut st.tit_quota);
                    ui.label("");
                    ui.checkbox(&mut st.tit_paga, "Paga lui le spese (es. inquilino)");
                    ui.end_row();
                });
                if ui.button("Aggiungi").clicked() {
                    match (st.tit_persona, parse_percentuale_ppm(&st.tit_quota)) {
                        (Some(p), Some(q)) => {
                            let r = c.db.aggiungi_titolarita(u.id, p, &st.tit_ruolo, q, st.tit_paga);
                            c.esito(r, "Titolare aggiunto");
                        }
                        (None, _) => c.errore("Scegli una persona"),
                        (_, None) => c.errore("Quota non valida (es. 50 oppure 33,33)"),
                    }
                }
            });
        }
        1 => {
            let Some(u) = st.sel.and_then(|id| lista.iter().find(|u| u.id == id)) else {
                ui.label("Seleziona un'unità.");
                return;
            };
            let paganti = c.db.pagatori_unita(u.id).unwrap_or_default();
            let tit = c.db.elenco_titolari(u.id).unwrap_or_default();
            if paganti.is_empty() {
                ui.label("Nessun soggetto paga le spese di questa unità.");
            }
            for (pid, _) in paganti {
                let nome = tit.iter().find(|t| t.persona_id == pid).map(|t| t.persona.clone()).unwrap_or_default();
                if let Ok(s) = c.db.situazione_persona(c.cond, pid) {
                    ui.label(RichText::new(nome).strong());
                    grid_scheda(&format!("g_saldo_{}", pid)).show(ui, |ui| {
                        valore(ui, "Dovuto", &format_eur(s.dovuto));
                        valore(ui, "Versato", &format_eur(s.versato));
                        ui.label("Residuo");
                        eur_colorato(ui, -s.residuo);
                        ui.end_row();
                        valore(ui, "Salvadanaio", &format_eur(s.salvadanaio));
                    });
                    ui.label(RichText::new("Rate dei piani attivi, su tutte le unità della persona.").small().weak());
                    ui.add_space(6.0);
                }
            }
        }
        _ => {
            ui.label("Note");
            ui.add(egui::TextEdit::multiline(&mut st.form.note).desired_rows(12).desired_width(f32::INFINITY));
            if st.form.id != 0 && ui.button("Salva note").clicked() {
                st.form.condominio_id = c.cond;
                let r = c.db.salva_unita(&st.form);
                c.esito(r, "Note salvate");
            }
        }
    }
}
