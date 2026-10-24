//! Persone (proprietari, inquilini…): elenco a sinistra, scheda a destra.

use super::elenco::*;
use super::*;
use crate::anagrafica::Persona;

pub struct Stato {
    sel: Option<i64>,
    tab: usize,
    form: Persona,
    nuova: bool,
    solo_condominio: bool,
    filtro: String,
}

impl Default for Stato {
    fn default() -> Self {
        Stato { sel: None, tab: 0, form: Persona::default(), nuova: false, solo_condominio: true, filtro: String::new() }
    }
}

const COLONNE: [Col; 4] = [col("Denominazione", 0.0), col("Telefono 1", 130.0), col("Email", 230.0), col("Cod. fiscale", 150.0)];

pub fn mostra(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    let tutte = if st.solo_condominio { c.db.persone_del_condominio(c.cond).unwrap_or_default() } else { c.db.elenco_persone().unwrap_or_default() };
    let f = st.filtro.to_lowercase();
    let lista: Vec<&Persona> = tutte
        .iter()
        .filter(|p| f.is_empty() || p.nome_completo().to_lowercase().contains(&f) || p.codice_fiscale.to_lowercase().contains(&f) || p.email.to_lowercase().contains(&f))
        .collect();
    let righe: Vec<Riga> = lista.iter().map(|p| Riga::new(vec![p.nome_completo().into(), p.telefono.clone().into(), p.email.clone().into(), p.codice_fiscale.clone().into()])).collect();
    let selezionata = st.sel.and_then(|id| lista.iter().position(|p| p.id == id));

    if st.sel.is_none() && !st.nuova {
        if let Some(p) = lista.first() {
            st.sel = Some(p.id);
        }
    }
    if !st.nuova && st.sel.unwrap_or(0) != st.form.id {
        st.form = st.sel.and_then(|id| tutte.iter().find(|p| p.id == id)).cloned().unwrap_or_default();
    }

    barra_azioni(ui, "az_persone", |ui| {
        if azione(ui, "📄", BLU, "Nuovo") {
            st.nuova = true;
            st.sel = None;
            st.form = Persona { tipo: "fisica".into(), ..Default::default() };
            st.tab = 0;
        }
        if azione(ui, "✖", ROSSO_ICONA, "Elimina") {
            if let Some(p) = st.sel.and_then(|id| tutte.iter().find(|p| p.id == id)) {
                c.chiedi(&format!("Eliminare {}?", p.nome_completo()), Azione::EliminaPersona(p.id));
            }
        }
        if azione(ui, "📗", VERDE_ICONA, "Excel") {
            esporta_csv(c, "persone", &COLONNE, &righe);
        }
        if azione(ui, "📥", ARANCIO_ICONA, "Importa CSV") {
            super::unita::importa_csv(c);
        }
        if azione(ui, "🏢", BLU, "Pos. nei condom.") {
            st.tab = 2;
        }
        if azione(ui, "💶", VERDE_ICONA, "Rate") {
            *c.app = Some(AzioneApp::Vai(Sezione::Rate, None));
        }
    });

    pannello_dettaglio(ui, "dett_persone", 440.0, |ui| scheda(c, st, ui));

    barra_filtri(ui, Some(&mut st.filtro), |ui| {
        ui.selectable_value(&mut st.solo_condominio, true, "Persone del condominio");
        ui.selectable_value(&mut st.solo_condominio, false, "Tutte le persone");
        ui.label(format!("({})", lista.len()));
    });
    let esito = elenco(ui, "t_persone", &COLONNE, &righe, selezionata, None);
    if let Some(i) = esito.clic {
        st.sel = Some(lista[i].id);
        st.nuova = false;
    }
}

fn scheda(c: &mut Ctx, st: &mut Stato, ui: &mut Ui) {
    schede(ui, &mut st.tab, &["Dati generali", "Altri dati", "Posizione nei condomìni"]);
    ui.add_space(8.0);
    match st.tab {
        0 => {
            ui.label(RichText::new(if st.form.id == 0 { "Nuova persona" } else { "Persona" }).strong());
            grid_scheda("g_persona").show(ui, |ui| {
                campo_pieno(ui, "Cognome / Rag. sociale *", &mut st.form.cognome);
                campo_pieno(ui, "Nome", &mut st.form.nome);
                ui.label("Tipo");
                if st.form.tipo.is_empty() {
                    st.form.tipo = "fisica".into();
                }
                scegli_str(ui, "p_tipo", &mut st.form.tipo, &["fisica", "giuridica"]);
                ui.end_row();
                campo_pieno(ui, "Indirizzo", &mut st.form.indirizzo);
                campo_pieno(ui, "Cod. fiscale / P.IVA", &mut st.form.codice_fiscale);
                campo_pieno(ui, "Telefono", &mut st.form.telefono);
                campo_pieno(ui, "E-mail", &mut st.form.email);
            });
            ui.add_space(4.0);
            if ui.button("Salva persona").clicked() {
                let r = c.db.salva_persona(&st.form);
                if let Some(id) = c.esito(r, "Persona salvata") {
                    st.sel = Some(id);
                    st.nuova = false;
                }
            }
        }
        1 => {
            ui.label("Note");
            ui.add(egui::TextEdit::multiline(&mut st.form.note).desired_rows(14).desired_width(f32::INFINITY));
            if ui.button("Salva note").clicked() {
                let r = c.db.salva_persona(&st.form);
                c.esito(r, "Note salvate");
            }
        }
        _ => {
            if st.form.id == 0 {
                ui.label("Salva la persona per vedere le sue unità.");
                return;
            }
            let righe: Vec<(String, String, String, i64)> = (|| -> rusqlite::Result<_> {
                let mut q = c.db.conn.prepare(
                    "SELECT k.nome, u.scala, u.interno, t.ruolo, t.quota_ppm FROM titolarita t
                     JOIN unita u ON u.id=t.unita_id JOIN condominio k ON k.id=u.condominio_id
                     WHERE t.persona_id=?1 ORDER BY k.nome, u.scala, u.interno",
                )?;
                let v = q
                    .query_map([st.form.id], |r| {
                        let scala: String = r.get(1)?;
                        let interno: String = r.get(2)?;
                        Ok((r.get::<_, String>(0)?, format!("{} {}", scala, interno).trim().to_string(), r.get::<_, String>(3)?, r.get::<_, i64>(4)?))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(v)
            })()
            .unwrap_or_default();
            if righe.is_empty() {
                ui.label("Questa persona non è titolare di nessuna unità.");
            }
            grid_scheda("g_pos").show(ui, |ui| {
                for (k, u, ruolo, quota) in righe {
                    ui.label(RichText::new(k).strong());
                    ui.label(format!("unità {} – {} ({:.2}%)", u, ruolo, quota as f64 / 10_000.0));
                    ui.end_row();
                }
            });
        }
    }
}
