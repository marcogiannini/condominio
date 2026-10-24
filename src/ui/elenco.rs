//! Componenti grafici in stile Domustudio: elenco a righe gialle con riga selezionata ambra,
//! schede (tab) a bordo, pannello di dettaglio a destra, barra delle azioni in basso.

use super::*;
use egui::{pos2, vec2, Align2, FontId, Rect, Sense, Stroke};

pub const RIGA: Color32 = Color32::from_rgb(255, 252, 226);
pub const SELEZIONE: Color32 = Color32::from_rgb(243, 203, 52);
pub const LINEA: Color32 = Color32::from_rgb(230, 224, 190);
pub const TESTO: Color32 = Color32::from_rgb(30, 30, 30);
pub const ALTEZZA: f32 = 21.0;
const ALTEZZA_TESTATA: f32 = 22.0;

#[derive(Clone)]
pub struct Col {
    pub titolo: std::borrow::Cow<'static, str>,
    /// Larghezza in punti; 0 = occupa lo spazio che resta.
    pub larg: f32,
    pub dx: bool,
}

pub const fn col(titolo: &'static str, larg: f32) -> Col {
    Col { titolo: std::borrow::Cow::Borrowed(titolo), larg, dx: false }
}

/// Colonna numerica con titolo calcolato.
pub fn col_dx_dinamica(titolo: String, larg: f32) -> Col {
    Col { titolo: std::borrow::Cow::Owned(titolo), larg, dx: true }
}

pub const fn col_dx(titolo: &'static str, larg: f32) -> Col {
    Col { titolo: std::borrow::Cow::Borrowed(titolo), larg, dx: true }
}

#[derive(Clone, Default)]
pub struct Cella {
    pub t: String,
    pub colore: Option<Color32>,
    pub forte: bool,
}

impl From<&str> for Cella {
    fn from(t: &str) -> Self {
        Cella { t: t.to_string(), ..Default::default() }
    }
}

impl From<String> for Cella {
    fn from(t: String) -> Self {
        Cella { t, ..Default::default() }
    }
}

impl From<&String> for Cella {
    fn from(t: &String) -> Self {
        Cella { t: t.clone(), ..Default::default() }
    }
}

pub fn colorata(t: impl Into<String>, colore: Color32) -> Cella {
    Cella { t: t.into(), colore: Some(colore), forte: false }
}

pub fn forte(t: impl Into<String>) -> Cella {
    Cella { t: t.into(), colore: None, forte: true }
}

/// Importo in euro, rosso se negativo.
pub fn cella_eur(c: Cents) -> Cella {
    if c < 0 {
        colorata(format_eur(c), ROSSO)
    } else {
        format_eur(c).into()
    }
}

#[derive(Clone, Default)]
pub struct Riga {
    pub celle: Vec<Cella>,
    /// Rientro (in punti) del testo della prima colonna: serve agli elenchi ad albero.
    pub rientro: f32,
    pub forte: bool,
    pub sfondo: Option<Color32>,
}

impl Riga {
    pub fn new(celle: Vec<Cella>) -> Self {
        Riga { celle, ..Default::default() }
    }
    pub fn rientrata(mut self, r: f32) -> Self {
        self.rientro = r;
        self
    }
    pub fn grassetto(mut self) -> Self {
        self.forte = true;
        self
    }
}

#[derive(Default)]
pub struct Esito {
    pub clic: Option<usize>,
    pub doppio: Option<usize>,
}

fn testo(p: &egui::Painter, pos: egui::Pos2, al: Align2, t: &str, colore: Color32, forte: bool) {
    let f = FontId::proportional(13.0);
    p.text(pos, al, t, f.clone(), colore);
    if forte {
        p.text(pos + vec2(0.6, 0.0), al, t, f, colore);
    }
}

/// Elenco a righe con testata, scorrimento verticale (e orizzontale se serve) e riga di totale opzionale.
pub fn elenco(ui: &mut Ui, id: &str, cols: &[Col], righe: &[Riga], sel: Option<usize>, piede: Option<&Riga>) -> Esito {
    let mut esito = Esito::default();
    let larg_disp = ui.available_width();
    let fisse: f32 = cols.iter().map(|c| c.larg).sum();
    let n_flex = cols.iter().filter(|c| c.larg <= 0.0).count();
    let flex_w = if n_flex > 0 { ((larg_disp - fisse - 2.0) / n_flex as f32).max(110.0) } else { 0.0 };
    let larghezze: Vec<f32> = cols.iter().map(|c| if c.larg > 0.0 { c.larg } else { flex_w }).collect();
    let tot_w: f32 = larghezze.iter().sum::<f32>().max(larg_disp - 2.0);
    let altezza_piede = if piede.is_some() { ALTEZZA + 2.0 } else { 0.0 };
    let alt_corpo = (ui.available_height() - ALTEZZA_TESTATA - altezza_piede - 16.0).max(60.0);

    egui::ScrollArea::horizontal().id_salt((id, "h")).auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        // testata
        let (r, _) = ui.allocate_exact_size(vec2(tot_w, ALTEZZA_TESTATA), Sense::hover());
        let p = ui.painter();
        p.rect_filled(r, 0.0, Color32::from_rgb(244, 244, 244));
        p.line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0_f32, Color32::from_rgb(190, 190, 190)));
        let mut x = r.left();
        for (c, w) in cols.iter().zip(&larghezze) {
            let cella = Rect::from_min_size(pos2(x, r.top()), vec2(*w, ALTEZZA_TESTATA));
            let pc = p.with_clip_rect(cella.intersect(ui.clip_rect()));
            if c.dx {
                testo(&pc, pos2(cella.right() - 4.0, cella.center().y), Align2::RIGHT_CENTER, &c.titolo, TESTO, false);
            } else {
                testo(&pc, pos2(cella.left() + 4.0, cella.center().y), Align2::LEFT_CENTER, &c.titolo, TESTO, false);
            }
            p.line_segment([cella.right_top() + vec2(0.0, 3.0), cella.right_bottom() - vec2(0.0, 3.0)], Stroke::new(1.0_f32, Color32::from_rgb(214, 214, 214)));
            x += *w;
        }

        egui::ScrollArea::vertical().id_salt((id, "v")).max_height(alt_corpo).auto_shrink([false, false]).show_rows(ui, ALTEZZA, righe.len(), |ui, range| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            for i in range {
                let riga = &righe[i];
                let (r, risp) = ui.allocate_exact_size(vec2(tot_w, ALTEZZA), Sense::click());
                let sfondo = if sel == Some(i) {
                    SELEZIONE
                } else if let Some(s) = riga.sfondo {
                    s
                } else if risp.hovered() {
                    Color32::from_rgb(255, 245, 190)
                } else {
                    RIGA
                };
                let p = ui.painter();
                p.rect_filled(r, 0.0, sfondo);
                p.line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0_f32, LINEA));
                let mut x = r.left();
                for (k, (c, w)) in cols.iter().zip(&larghezze).enumerate() {
                    let cella = Rect::from_min_size(pos2(x, r.top()), vec2(*w, ALTEZZA));
                    if let Some(cl) = riga.celle.get(k) {
                        let pc = p.with_clip_rect(cella.intersect(ui.clip_rect()));
                        let colore = cl.colore.unwrap_or(TESTO);
                        let f = cl.forte || riga.forte;
                        if c.dx {
                            testo(&pc, pos2(cella.right() - 4.0, cella.center().y), Align2::RIGHT_CENTER, &cl.t, colore, f);
                        } else {
                            let rientro = if k == 0 { riga.rientro } else { 0.0 };
                            testo(&pc, pos2(cella.left() + 4.0 + rientro, cella.center().y), Align2::LEFT_CENTER, &cl.t, colore, f);
                        }
                    }
                    x += *w;
                }
                if risp.clicked() {
                    esito.clic = Some(i);
                }
                if risp.double_clicked() {
                    esito.doppio = Some(i);
                }
            }
        });

        if let Some(pie) = piede {
            let (r, _) = ui.allocate_exact_size(vec2(tot_w, ALTEZZA + 2.0), Sense::hover());
            let p = ui.painter();
            p.rect_filled(r, 0.0, Color32::from_rgb(244, 244, 244));
            p.line_segment([r.left_top(), r.right_top()], Stroke::new(1.0_f32, Color32::from_rgb(190, 190, 190)));
            let mut x = r.left();
            for (k, (c, w)) in cols.iter().zip(&larghezze).enumerate() {
                let cella = Rect::from_min_size(pos2(x, r.top()), vec2(*w, ALTEZZA + 2.0));
                if let Some(cl) = pie.celle.get(k) {
                    let colore = cl.colore.unwrap_or(TESTO);
                    let pc = p.with_clip_rect(cella.intersect(ui.clip_rect()));
                    if c.dx {
                        testo(&pc, pos2(cella.right() - 4.0, cella.center().y), Align2::RIGHT_CENTER, &cl.t, colore, true);
                    } else {
                        testo(&pc, pos2(cella.left() + 4.0, cella.center().y), Align2::LEFT_CENTER, &cl.t, colore, true);
                    }
                }
                x += *w;
            }
        }
    });
    esito
}

/// Schede a bordo, come in Domustudio («Dati generali», «Struttura», …).
pub fn schede(ui: &mut Ui, corrente: &mut usize, nomi: &[&str]) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        for (i, n) in nomi.iter().enumerate() {
            let galley = ui.painter().layout_no_wrap(n.to_string(), FontId::proportional(13.0), TESTO);
            let dim = galley.size() + vec2(16.0, 8.0);
            let (r, risp) = ui.allocate_exact_size(dim, Sense::click());
            let attiva = *corrente == i;
            let p = ui.painter();
            let fondo = if attiva {
                Color32::WHITE
            } else if risp.hovered() {
                Color32::from_rgb(240, 240, 240)
            } else {
                Color32::from_rgb(228, 228, 228)
            };
            p.rect_filled(r, 0.0, fondo);
            p.rect_stroke(r, 0.0, Stroke::new(1.0_f32, Color32::from_rgb(190, 190, 190)));
            if attiva {
                p.line_segment([r.left_bottom() + vec2(1.0, 0.0), r.right_bottom() - vec2(1.0, 0.0)], Stroke::new(2.0_f32, Color32::WHITE));
            }
            p.galley(r.min + vec2(8.0, 4.0), galley, TESTO);
            if risp.clicked() {
                *corrente = i;
            }
        }
    });
}

/// Pulsante piatto della barra delle azioni: icona colorata + testo.
pub fn azione(ui: &mut Ui, icona: &str, colore: Color32, t: &str) -> bool {
    let galley = ui.painter().layout_no_wrap(t.to_string(), FontId::proportional(13.0), TESTO);
    let g_icona = ui.painter().layout_no_wrap(icona.to_string(), FontId::proportional(15.0), colore);
    let dim = vec2(g_icona.size().x + galley.size().x + 18.0, 24.0);
    let (r, risp) = ui.allocate_exact_size(dim, Sense::click());
    let p = ui.painter();
    if risp.hovered() {
        p.rect_filled(r, 3.0, Color32::from_rgb(226, 236, 248));
    }
    let y = r.center().y;
    p.galley(pos2(r.left() + 6.0, y - g_icona.size().y / 2.0), g_icona.clone(), colore);
    p.galley(pos2(r.left() + 10.0 + g_icona.size().x, y - galley.size().y / 2.0), galley, TESTO);
    risp.clicked()
}

/// Barra delle azioni in basso (Nuovo, Elimina, Stampa, Excel…). Va chiamata PRIMA del contenuto.
pub fn barra_azioni(ui: &mut Ui, id: &str, f: impl FnOnce(&mut Ui)) {
    egui::TopBottomPanel::bottom(egui::Id::new(id.to_string()))
        .exact_height(30.0)
        .resizable(false)
        .frame(egui::Frame::none().fill(Color32::from_rgb(250, 250, 250)).stroke(Stroke::new(1.0_f32, Color32::from_rgb(210, 210, 210))).inner_margin(egui::Margin::symmetric(6.0, 2.0)))
        .show_inside(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                f(ui);
            });
        });
}

/// Pannello di dettaglio sulla destra, con le sue schede. Va chiamato PRIMA dell'elenco.
pub fn pannello_dettaglio(ui: &mut Ui, id: &str, larghezza: f32, f: impl FnOnce(&mut Ui)) {
    egui::SidePanel::right(egui::Id::new(id.to_string()))
        .default_width(larghezza)
        .min_width(320.0)
        .resizable(true)
        .frame(egui::Frame::none().fill(Color32::from_rgb(250, 250, 250)).stroke(Stroke::new(1.0_f32, Color32::from_rgb(210, 210, 210))).inner_margin(egui::Margin::same(8.0)))
        .show_inside(ui, |ui| {
            egui::ScrollArea::vertical().id_salt((id, "scroll")).auto_shrink([false, false]).show(ui, f);
        });
}

/// Barra dei filtri (ingranaggio a sinistra, «Cerca» a destra).
pub fn barra_filtri(ui: &mut Ui, cerca: Option<&mut String>, f: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.add_space(2.0);
        ui.label(RichText::new("⚙").size(15.0).color(Color32::from_rgb(40, 110, 190)));
        f(ui);
        if let Some(c) = cerca {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(egui::TextEdit::singleline(c).hint_text("Cerca").desired_width(190.0));
            });
        }
    });
    ui.add_space(2.0);
}

/// Etichetta + campo di testo a tutta larghezza (dentro una Grid a 2 colonne).
pub fn campo_pieno(ui: &mut Ui, etichetta: &str, v: &mut String) {
    ui.label(etichetta);
    let w = ui.available_width().max(120.0);
    ui.add(egui::TextEdit::singleline(v).desired_width(w));
    ui.end_row();
}

/// Etichetta + testo a sola lettura.
pub fn valore(ui: &mut Ui, etichetta: &str, v: &str) {
    ui.label(etichetta);
    ui.label(v);
    ui.end_row();
}

pub fn grid_scheda(id: &str) -> egui::Grid {
    egui::Grid::new(id.to_string()).num_columns(2).spacing([8.0, 6.0]).min_col_width(78.0)
}

/// Salva un elenco come CSV (apribile con Excel).
pub fn esporta_csv(c: &mut Ctx, nome: &str, cols: &[Col], righe: &[Riga]) {
    let Some(p) = rfd::FileDialog::new().set_file_name(format!("{}.csv", nome)).save_file() else { return };
    let campo = |s: &str| {
        if s.contains(';') || s.contains('"') || s.contains('\n') {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s.to_string()
        }
    };
    let mut s = String::from("\u{feff}");
    s.push_str(&cols.iter().map(|x| campo(&x.titolo)).collect::<Vec<_>>().join(";"));
    s.push('\n');
    for r in righe {
        s.push_str(&r.celle.iter().map(|x| campo(&x.t)).collect::<Vec<_>>().join(";"));
        s.push('\n');
    }
    match std::fs::write(&p, s) {
        Ok(_) => c.ok("Elenco esportato in CSV (si apre con Excel)"),
        Err(e) => c.err(e.into()),
    }
}

/// Colori delle icone, per imitare la barra colorata di Domustudio.
pub const BLU: Color32 = Color32::from_rgb(40, 110, 190);
pub const ROSSO_ICONA: Color32 = Color32::from_rgb(210, 50, 40);
pub const VERDE_ICONA: Color32 = Color32::from_rgb(50, 150, 70);
pub const ARANCIO_ICONA: Color32 = Color32::from_rgb(230, 140, 20);
pub const GRIGIO_ICONA: Color32 = Color32::from_rgb(90, 90, 90);
