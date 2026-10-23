//! Interfaccia grafica nativa (egui/eframe).

use crate::anagrafica::Condominio;
use crate::util::*;
use crate::Db;
use eframe::egui::{self, Color32, RichText, Ui};

mod agenda;
mod anagrafica;
mod assemblee;
mod bacheca;
mod banca;
mod bilanci;
mod condomini;
mod conformita;
mod contabilita;
mod dashboard;
mod documenti;
mod fisco;
mod fornitori;
mod impostazioni;
mod incassi;
mod millesimi;
mod morosita;
mod piani;
mod pratiche;
mod preventivo;
mod riparti;
mod segnalazioni;
mod struttura;

use struttura::{Cmd, GRUPPI, MENU};

// ------------------------------------------------------------------ messaggi

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Livello {
    Ok,
    Err,
}

pub struct Msg {
    pub livello: Livello,
    pub testo: String,
}

/// Azioni distruttive: richiedono sempre una conferma esplicita.
#[derive(Clone, Debug)]
pub enum Azione {
    EliminaPersona(i64),
    EliminaUnita(i64),
    RimuoviTitolarita(i64),
    EliminaTabella(i64),
    EliminaVoce(i64),
    StornaIncasso(i64),
    StornaMovimento(i64),
    StornaFattura(i64),
    StornaPagamentoFattura(i64),
    EliminaDocumento(i64),
    EliminaTicket(i64),
    EliminaComunicazione(i64),
    EliminaPianoBozza(i64),
    AnnullaPiano(i64),
    EliminaCondominio(i64),
    EliminaAssemblea(i64),
    EliminaPratica(i64),
    EliminaAgenda(i64),
    AnnullaConguaglio(i64),
    AnnullaRiconciliazione(i64),
}

/// Richieste delle schermate che riguardano l'applicazione intera (cambio condominio, nuovo condominio…).
#[derive(Clone, Copy, Debug)]
pub enum AzioneApp {
    Apri(i64),
    Nuovo,
    Demo,
}

/// Contesto condiviso dalle schermate durante un frame.
pub struct Ctx<'a> {
    pub db: &'a Db,
    pub cond: i64,
    pub cond_nome: &'a str,
    pub esercizio: Option<i64>,
    pub rev: &'a mut u64,
    pub msg: &'a mut Option<Msg>,
    pub richiesta: &'a mut Option<(String, Azione)>,
    pub naviga: &'a mut Option<Sezione>,
    pub app: &'a mut Option<AzioneApp>,
    /// True se la schermata deve mostrare le proprie schede interne (altrimenti le governa la barra delle viste).
    pub schede: bool,
}

impl<'a> Ctx<'a> {
    pub fn ok(&mut self, t: &str) {
        *self.rev += 1;
        *self.msg = Some(Msg { livello: Livello::Ok, testo: t.to_string() });
    }
    pub fn err(&mut self, e: anyhow::Error) {
        *self.msg = Some(Msg { livello: Livello::Err, testo: e.to_string() });
    }
    pub fn errore(&mut self, t: &str) {
        *self.msg = Some(Msg { livello: Livello::Err, testo: t.to_string() });
    }
    /// Se `r` è Ok mostra `ok` e restituisce il valore, altrimenti mostra l'errore.
    pub fn esito<T>(&mut self, r: anyhow::Result<T>, ok: &str) -> Option<T> {
        match r {
            Ok(v) => {
                self.ok(ok);
                Some(v)
            }
            Err(e) => {
                self.err(e);
                None
            }
        }
    }
    pub fn chiedi(&mut self, testo: &str, a: Azione) {
        *self.richiesta = Some((testo.to_string(), a));
    }
}

/// Cache invalidata quando cambia il contatore di revisione (dopo ogni modifica) o la chiave.
pub struct Cache<T> {
    rev: u64,
    key: String,
    val: Option<T>,
}

impl<T> Default for Cache<T> {
    fn default() -> Self {
        Cache { rev: 0, key: String::new(), val: None }
    }
}

impl<T> Cache<T> {
    pub fn get(&mut self, rev: u64, key: &str, f: impl FnOnce() -> T) -> &T {
        if self.val.is_none() || self.rev != rev || self.key != key {
            self.val = Some(f());
            self.rev = rev;
            self.key = key.to_string();
        }
        self.val.as_ref().unwrap()
    }
}

// ------------------------------------------------------------------- helper UI

/// Salva una pagina HTML (A4, pronta per la stampa o per «Salva come PDF») e la apre nel browser predefinito.
pub fn apri_html_file(nome: &str, html: &str) -> anyhow::Result<std::path::PathBuf> {
    let dir = std::env::temp_dir().join("gestione-condominio-stampe");
    std::fs::create_dir_all(&dir)?;
    let pulito: String = nome.chars().map(|ch| if ch.is_alphanumeric() { ch } else { '_' }).collect();
    let file = dir.join(format!("{}_{}.html", pulito, chrono::Local::now().format("%Y%m%d_%H%M%S")));
    std::fs::write(&file, html)?;
    opener::open(&file).map_err(|e| anyhow::anyhow!("impossibile aprire il browser: {}", e))?;
    Ok(file)
}

pub fn apri_html(c: &mut Ctx, nome: &str, html: &str) {
    match apri_html_file(nome, html) {
        Ok(_) => c.ok("Stampa aperta nel browser: usa Stampa (Ctrl+P) o «Salva come PDF»"),
        Err(e) => c.err(e),
    }
}

pub const VERDE: Color32 = Color32::from_rgb(46, 140, 80);
pub const ROSSO: Color32 = Color32::from_rgb(200, 60, 60);
pub const ARANCIO: Color32 = Color32::from_rgb(210, 140, 30);

pub fn campo(ui: &mut Ui, etichetta: &str, v: &mut String) {
    ui.label(etichetta);
    ui.add(egui::TextEdit::singleline(v).desired_width(240.0));
    ui.end_row();
}

pub fn campo_lungo(ui: &mut Ui, etichetta: &str, v: &mut String, righe: usize) {
    ui.label(etichetta);
    ui.add(egui::TextEdit::multiline(v).desired_rows(righe).desired_width(380.0));
    ui.end_row();
}

pub fn scegli(ui: &mut Ui, id: &str, corrente: &mut Option<i64>, voci: &[(i64, String)], vuoto: &str) {
    let testo = corrente
        .and_then(|c| voci.iter().find(|v| v.0 == c))
        .map(|v| v.1.clone())
        .unwrap_or_else(|| if vuoto.is_empty() { "— scegli —".to_string() } else { vuoto.to_string() });
    egui::ComboBox::from_id_salt(id).selected_text(testo).width(260.0).show_ui(ui, |ui| {
        if !vuoto.is_empty() && ui.selectable_label(corrente.is_none(), vuoto).clicked() {
            *corrente = None;
        }
        for (i, n) in voci {
            if ui.selectable_label(*corrente == Some(*i), n).clicked() {
                *corrente = Some(*i);
            }
        }
    });
}

pub fn scegli_str(ui: &mut Ui, id: &str, corrente: &mut String, opzioni: &[&str]) {
    egui::ComboBox::from_id_salt(id).selected_text(corrente.clone()).width(160.0).show_ui(ui, |ui| {
        for o in opzioni {
            if ui.selectable_label(corrente == o, *o).clicked() {
                *corrente = o.to_string();
            }
        }
    });
}

pub fn titolo(ui: &mut Ui, t: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(t).size(20.0).strong());
    ui.add_space(4.0);
}

pub fn eur_colorato(ui: &mut Ui, c: Cents) {
    let col = if c < 0 { ROSSO } else { ui.visuals().text_color() };
    ui.label(RichText::new(format_eur(c)).color(col));
}

/// Pulsante "piccolo" per le azioni di riga.
pub fn mini(ui: &mut Ui, t: &str) -> bool {
    ui.small_button(t).clicked()
}

pub fn etichetta_persona(p: &crate::anagrafica::Persona) -> String {
    let n = p.nome_completo();
    if p.codice_fiscale.is_empty() {
        n
    } else {
        format!("{} ({})", n, p.codice_fiscale)
    }
}

// ------------------------------------------------------------------ sezioni

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sezione {
    Dashboard,
    Condomini,
    Agenda,
    Anagrafica,
    Millesimi,
    Preventivo,
    PianiRate,
    Bilanci,
    Incassi,
    Morosita,
    Fornitori,
    Contabilita,
    Banca,
    Fisco,
    Assemblee,
    Segnalazioni,
    Pratiche,
    Bacheca,
    Documenti,
    Conformita,
    Impostazioni,
}


#[derive(Default)]
struct Stati {
    condomini: condomini::Stato,
    agenda: agenda::Stato,
    assemblee: assemblee::Stato,
    banca: banca::Stato,
    bilanci: bilanci::Stato,
    fisco: fisco::Stato,
    morosita: morosita::Stato,
    pratiche: pratiche::Stato,
    dashboard: dashboard::Stato,
    anagrafica: anagrafica::Stato,
    millesimi: millesimi::Stato,
    preventivo: preventivo::Stato,
    piani: piani::Stato,
    incassi: incassi::Stato,
    fornitori: fornitori::Stato,
    contabilita: contabilita::Stato,
    documenti: documenti::Stato,
    segnalazioni: segnalazioni::Stato,
    bacheca: bacheca::Stato,
    conformita: conformita::Stato,
    impostazioni: impostazioni::Stato,
}

impl Stati {
    fn imposta_tab(&mut self, s: Sezione, t: usize) {
        match s {
            Sezione::Anagrafica => self.anagrafica.imposta_tab(t),
            Sezione::Bacheca => self.bacheca.imposta_tab(t),
            Sezione::Bilanci => self.bilanci.imposta_tab(t),
            Sezione::Conformita => self.conformita.imposta_tab(t),
            Sezione::Contabilita => self.contabilita.imposta_tab(t),
            Sezione::Fisco => self.fisco.imposta_tab(t),
            Sezione::Fornitori => self.fornitori.imposta_tab(t),
            Sezione::Incassi => self.incassi.imposta_tab(t),
            Sezione::Morosita => self.morosita.imposta_tab(t),
            Sezione::Preventivo => self.preventivo.imposta_tab(t),
            _ => {}
        }
    }
}

pub struct App {
    db: Db,
    condomini: Vec<Condominio>,
    cond: Option<i64>,
    esercizio: Option<i64>,
    sezione: Sezione,
    /// Gruppo (pulsante della barra delle sezioni) attivo e scheda scelta dalla barra delle viste.
    gruppo: usize,
    tab: Option<usize>,
    scuro: bool,
    selettore: bool,
    guida: bool,
    rev_condomini: u64,
    msg: Option<Msg>,
    richiesta: Option<(String, Azione)>,
    naviga: Option<Sezione>,
    rev: u64,
    st: Stati,
    nuovo_cond: Option<Condominio>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, db: Db) -> Self {
        applica_tema(&cc.egui_ctx, false);
        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        cc.egui_ctx.set_style(style);
        let condomini = db.elenco_condomini().unwrap_or_default();
        let cond = condomini.first().map(|c| c.id);
        let mut app = App {
            db,
            condomini,
            cond,
            esercizio: None,
            sezione: Sezione::Dashboard,
            gruppo: 0,
            tab: None,
            scuro: false,
            selettore: false,
            guida: false,
            rev_condomini: 1,
            msg: None,
            richiesta: None,
            naviga: None,
            rev: 1,
            st: Stati::default(),
            nuovo_cond: None,
        };
        app.scegli_esercizio_predefinito();
        app
    }

    fn scegli_esercizio_predefinito(&mut self) {
        self.esercizio = None;
        if let Some(c) = self.cond {
            if let Ok(es) = self.db.elenco_esercizi(c) {
                self.esercizio = es.iter().find(|e| e.stato == "aperto").or(es.first()).map(|e| e.id);
            }
        }
    }

    fn ricarica_condomini(&mut self) {
        self.condomini = self.db.elenco_condomini().unwrap_or_default();
        if self.cond.map(|c| !self.condomini.iter().any(|x| x.id == c)).unwrap_or(true) {
            self.cond = self.condomini.first().map(|c| c.id);
            self.scegli_esercizio_predefinito();
        }
    }

    fn esegui(&mut self, a: Azione) {
        use Azione::*;
        let db = &self.db;
        let oggi = oggi_iso();
        let r: anyhow::Result<&str> = match a {
            EliminaPersona(id) => db.elimina_persona(id).map(|_| "Persona eliminata"),
            EliminaUnita(id) => db.elimina_unita(id).map(|_| "Unità eliminata"),
            RimuoviTitolarita(id) => db.rimuovi_titolarita(id).map(|_| "Titolarità rimossa"),
            EliminaTabella(id) => db.elimina_tabella(id).map(|_| "Tabella eliminata"),
            EliminaVoce(id) => db.elimina_voce(id).map(|_| "Voce eliminata"),
            StornaMovimento(id) => db.storna_movimento(id, &oggi, "storno manuale").map(|_| "Movimento stornato"),
            StornaIncasso(id) => db.storna_incasso(id, &oggi).map(|_| "Incasso stornato: rate e Salvadanaio ripristinati"),
            StornaFattura(id) => db.storna_fattura(id, &oggi).map(|_| "Fattura stornata"),
            StornaPagamentoFattura(id) => db.storna_pagamento_fattura(id, &oggi).map(|_| "Pagamento stornato"),
            EliminaDocumento(id) => db.elimina_documento(id).map(|_| "Documento eliminato"),
            EliminaTicket(id) => db.elimina_ticket(id).map(|_| "Segnalazione eliminata"),
            EliminaComunicazione(id) => db.elimina_comunicazione(id).map(|_| "Comunicazione eliminata"),
            EliminaPianoBozza(id) => db.elimina_piano_bozza(id).map(|_| "Piano eliminato"),
            AnnullaPiano(id) => db.annulla_piano(id).map(|_| "Piano annullato e scrittura contabile stornata"),
            EliminaCondominio(id) => db.elimina_condominio(id).map(|_| "Condominio eliminato"),
            EliminaAssemblea(id) => db.elimina_assemblea(id).map(|_| "Assemblea eliminata"),
            EliminaPratica(id) => db.elimina_pratica(id).map(|_| "Pratica eliminata"),
            EliminaAgenda(id) => db.elimina_agenda(id).map(|_| "Promemoria eliminato"),
            AnnullaConguaglio(es) => db.annulla_conguaglio(es).map(|_| "Conguaglio annullato"),
            AnnullaRiconciliazione(id) => db.annulla_riconciliazione(id, &oggi).map(|_| "Riconciliazione annullata: operazione collegata stornata"),
        };
        let era_condominio = matches!(a, EliminaCondominio(_));
        match r {
            Ok(t) => {
                self.rev += 1;
                self.msg = Some(Msg { livello: Livello::Ok, testo: t.to_string() });
            }
            Err(e) => self.msg = Some(Msg { livello: Livello::Err, testo: e.to_string() }),
        }
        if era_condominio {
            self.ricarica_condomini();
        }
    }

    fn barra_alta(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("alto").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Condominio (F2)").strong());
                let voci: Vec<(i64, String)> = self.condomini.iter().map(|c| (c.id, c.nome.clone())).collect();
                let prima = self.cond;
                scegli(ui, "sel_cond", &mut self.cond, &voci, "");
                if self.cond != prima {
                    self.scegli_esercizio_predefinito();
                    self.rev += 1;
                }
                if ui.button("+ Nuovo").clicked() {
                    self.nuovo_cond = Some(Condominio::default());
                }
                ui.separator();
                if let Some(c) = self.cond {
                    ui.label(RichText::new("Esercizio").strong());
                    let es = self.db.elenco_esercizi(c).unwrap_or_default();
                    let voci: Vec<(i64, String)> =
                        es.iter().map(|e| (e.id, format!("{} ({})", e.nome, if e.stato == "aperto" { "aperto" } else { "chiuso" }))).collect();
                    let prima = self.esercizio;
                    scegli(ui, "sel_es", &mut self.esercizio, &voci, "");
                    if prima != self.esercizio {
                        self.rev += 1;
                    }
                    let n = self.db.notifiche_non_lette(c).unwrap_or(0);
                    if n > 0 {
                        ui.separator();
                        if ui.button(RichText::new(format!("{} notifiche", n)).color(ARANCIO)).clicked() {
                            self.vai(Sezione::Dashboard, None);
                        }
                    }
                }
            });
            if let Some(m) = &self.msg {
                let col = if m.livello == Livello::Ok { VERDE } else { ROSSO };
                let mut chiudi = false;
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&m.testo).color(col).strong());
                    if ui.small_button("x").clicked() {
                        chiudi = true;
                    }
                });
                if chiudi {
                    self.msg = None;
                }
            }
        });
    }

    fn finestre(&mut self, ctx: &egui::Context) {
        // nuovo condominio
        let mut crea: Option<Condominio> = None;
        let mut demo = false;
        let mut chiudi = false;
        if let Some(c) = &mut self.nuovo_cond {
            egui::Window::new("Nuovo condominio").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                egui::Grid::new("g_nuovo_cond").num_columns(2).show(ui, |ui| {
                    campo(ui, "Nome *", &mut c.nome);
                    campo(ui, "Indirizzo", &mut c.indirizzo);
                    campo(ui, "Città", &mut c.citta);
                    campo(ui, "Codice fiscale", &mut c.codice_fiscale);
                });
                ui.horizontal(|ui| {
                    if ui.button("Crea").clicked() {
                        crea = Some(c.clone());
                    }
                    if ui.button("Annulla").clicked() {
                        chiudi = true;
                    }
                    if ui.button("Carica esempio").clicked() {
                        demo = true;
                    }
                });
            });
        }
        if demo {
            self.nuovo_cond = None;
            self.crea_demo();
        }
        if let Some(c) = crea {
            match self.db.salva_condominio(&c) {
                Ok(id) => {
                    self.ricarica_condomini();
                    self.cond = Some(id);
                    self.scegli_esercizio_predefinito();
                    self.nuovo_cond = None;
                    self.rev += 1;
                    self.msg = Some(Msg { livello: Livello::Ok, testo: "Condominio creato".into() });
                }
                Err(e) => self.msg = Some(Msg { livello: Livello::Err, testo: e.to_string() }),
            }
        }
        if chiudi {
            self.nuovo_cond = None;
        }

        // conferma azioni distruttive
        let mut esegui: Option<Azione> = None;
        let mut annulla = false;
        if let Some((testo, a)) = &self.richiesta {
            egui::Window::new("Conferma").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.label(testo);
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("Conferma").color(ROSSO)).clicked() {
                        esegui = Some(a.clone());
                    }
                    if ui.button("Annulla").clicked() {
                        annulla = true;
                    }
                });
            });
        }
        if let Some(a) = esegui {
            self.richiesta = None;
            self.esegui(a);
        } else if annulla {
            self.richiesta = None;
        }
    }

    fn crea_demo(&mut self) {
        match self.db.carica_demo() {
            Ok(id) => {
                self.ricarica_condomini();
                self.cond = Some(id);
                self.scegli_esercizio_predefinito();
                self.rev += 1;
                self.msg = Some(Msg { livello: Livello::Ok, testo: "Condominio di esempio caricato: puoi eliminarlo da Impostazioni".into() });
            }
            Err(e) => self.msg = Some(Msg { livello: Livello::Err, testo: e.to_string() }),
        }
    }

    fn benvenuto(&mut self, ui: &mut Ui) {
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("Benvenuto nel gestionale condominiale").size(26.0).strong());
            ui.add_space(8.0);
            ui.label("Crea il primo condominio per iniziare. I dati restano sul tuo computer.");
            ui.add_space(12.0);
            if ui.button(RichText::new("Crea il primo condominio").size(16.0)).clicked() {
                self.nuovo_cond = Some(Condominio::default());
            }
            ui.add_space(6.0);
            if ui.button("Oppure carica un condominio di esempio").clicked() {
                self.crea_demo();
            }
        });
    }
}

impl App {
    /// Cambia vista: sezione + scheda interna; aggiorna il pulsante attivo della barra delle sezioni.
    fn vai(&mut self, s: Sezione, tab: Option<usize>) {
        self.sezione = s;
        self.tab = tab;
        if let Some(t) = tab {
            self.st.imposta_tab(s, t);
        }
        let in_gruppo = |g: &struttura::Gruppo| g.voci.iter().any(|v| v.sez == s && (tab.is_none() || v.tab == tab));
        if !in_gruppo(&GRUPPI[self.gruppo]) {
            if let Some(i) = GRUPPI.iter().position(in_gruppo) {
                self.gruppo = i;
            }
        }
    }

    fn notifica(&mut self, livello: Livello, testo: impl Into<String>) {
        self.msg = Some(Msg { livello, testo: testo.into() });
    }

    fn esegui_cmd(&mut self, ctx: &egui::Context, cmd: Cmd) {
        match cmd {
            Cmd::Vai(s, t) => self.vai(s, t),
            Cmd::NuovoCondominio => self.nuovo_cond = Some(Condominio::default()),
            Cmd::Demo => self.crea_demo(),
            Cmd::Seleziona => self.selettore = true,
            Cmd::Guida => self.guida = true,
            Cmd::Esci => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Cmd::Tema => {
                self.scuro = !self.scuro;
                applica_tema(ctx, self.scuro);
            }
            Cmd::StampaRendiconto => match self.esercizio {
                None => self.notifica(Livello::Err, "Scegli prima un esercizio"),
                Some(es) => match self.db.html_rendiconto(es).and_then(|h| apri_html_file("rendiconto", &h)) {
                    Ok(_) => self.notifica(Livello::Ok, "Rendiconto aperto nel browser: usa Stampa o «Salva come PDF»"),
                    Err(e) => self.notifica(Livello::Err, e.to_string()),
                },
            },
            Cmd::Backup => {
                let nome = format!("condominio_backup_{}.db", oggi_iso());
                if let Some(dest) = rfd::FileDialog::new().set_file_name(&nome).save_file() {
                    match self.db.backup(&dest) {
                        Ok(_) => self.notifica(Livello::Ok, "Backup creato (copia anche la cartella dell'archivio documenti)"),
                        Err(e) => self.notifica(Livello::Err, e.to_string()),
                    }
                }
            }
        }
    }

    /// Barra dei menu: File, Preventivo, Consuntivo, Comunicazioni, Fisco, Strumenti, ?
    fn barra_menu(&mut self, ctx: &egui::Context) -> Option<Cmd> {
        let mut cmd = None;
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                for (nome, voci) in MENU {
                    ui.menu_button(*nome, |ui| {
                        for v in voci.iter() {
                            let mut b = egui::Button::new(v.nome);
                            if !v.tasto.is_empty() {
                                b = b.shortcut_text(v.tasto);
                            }
                            if ui.add(b).clicked() {
                                cmd = Some(v.cmd);
                                ui.close_menu();
                            }
                        }
                    });
                }
            });
        });
        cmd
    }

    /// Barra dei pulsanti-sezione: Agenda, Condomìni, Unità, Tabelle, Conti, Rate, Movimenti, …
    fn barra_sezioni(&mut self, ctx: &egui::Context) {
        let mut scelto = None;
        let fondo = ctx.style().visuals.faint_bg_color;
        egui::TopBottomPanel::top("sezioni").frame(egui::Frame::none().fill(fondo).inner_margin(egui::Margin::symmetric(8.0, 4.0))).show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (i, g) in GRUPPI.iter().enumerate() {
                    let attivo = self.gruppo == i;
                    let (rect, risposta) = ui.allocate_exact_size(egui::vec2(84.0, 56.0), egui::Sense::click());
                    let v = ui.visuals();
                    let (sfondo, testo) = if attivo {
                        (v.selection.bg_fill, Color32::WHITE)
                    } else if risposta.hovered() {
                        (v.widgets.hovered.bg_fill, v.strong_text_color())
                    } else {
                        (Color32::TRANSPARENT, v.text_color())
                    };
                    let p = ui.painter();
                    p.rect_filled(rect, 4.0, sfondo);
                    p.text(rect.center_top() + egui::vec2(0.0, 19.0), egui::Align2::CENTER_CENTER, g.icona, egui::FontId::proportional(22.0), testo);
                    p.text(rect.center_bottom() - egui::vec2(0.0, 11.0), egui::Align2::CENTER_CENTER, g.nome, egui::FontId::proportional(12.5), testo);
                    if risposta.clicked() {
                        scelto = Some(i);
                    }
                }
            });
        });
        if let Some(i) = scelto {
            let v = GRUPPI[i].voci[0];
            self.vai(v.sez, v.tab);
            self.gruppo = i;
        }
    }

    fn barra_stato(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("stato").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let cond = self.condomini.iter().find(|c| Some(c.id) == self.cond).map(|c| c.nome.clone());
                ui.label(RichText::new(format!("Condominio: {}", cond.as_deref().unwrap_or("—"))).strong());
                ui.separator();
                let es = self.cond.and_then(|c| self.db.elenco_esercizi(c).ok()).and_then(|v| v.into_iter().find(|e| Some(e.id) == self.esercizio));
                match es {
                    Some(e) => ui.label(format!("Esercizio {} ({} – {}, {})", e.nome, data_it(&e.dal), data_it(&e.al), e.stato)),
                    None => ui.label("Nessun esercizio"),
                };
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("F1 Guida   ·   F2 Cambia condominio / esercizio").small());
                });
            });
        });
    }

    /// Finestre di servizio: selettore condominio/esercizio (F2) e guida rapida (F1).
    fn finestre_extra(&mut self, ctx: &egui::Context) {
        if self.selettore {
            let mut aperta = true;
            let mut cambia_cond: Option<i64> = None;
            let mut cambia_es: Option<i64> = None;
            egui::Window::new("Condominio ed esercizio di lavoro").open(&mut aperta).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Condomìni").strong());
                        for c in &self.condomini {
                            if ui.selectable_label(self.cond == Some(c.id), &c.nome).clicked() {
                                cambia_cond = Some(c.id);
                            }
                        }
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Esercizi").strong());
                        if let Some(c) = self.cond {
                            for e in self.db.elenco_esercizi(c).unwrap_or_default() {
                                if ui.selectable_label(self.esercizio == Some(e.id), format!("{} ({})", e.nome, e.stato)).clicked() {
                                    cambia_es = Some(e.id);
                                }
                            }
                        }
                    });
                });
            });
            if let Some(c) = cambia_cond {
                if self.cond != Some(c) {
                    self.cond = Some(c);
                    self.scegli_esercizio_predefinito();
                    self.rev += 1;
                }
            }
            if let Some(e) = cambia_es {
                self.esercizio = Some(e);
                self.rev += 1;
            }
            if !aperta || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.selettore = false;
            }
        }
        if self.guida {
            let mut aperta = true;
            egui::Window::new("Guida rapida").open(&mut aperta).collapsible(false).default_width(520.0).show(ctx, |ui| {
                ui.label(RichText::new("Come ci si muove").strong());
                ui.label("• La barra dei pulsanti in alto sceglie l'area di lavoro (Agenda, Condomìni, Unità, Tabelle, Conti, Rate, Movimenti, Attività, Archivio, Assemblee, Persone, Fornitori); sotto compaiono le viste di quell'area.");
                ui.label("• I menu File, Preventivo, Consuntivo, Comunicazioni, Fisco e Strumenti raggiungono le stesse schermate e le stampe.");
                ui.label("• Condominio ed esercizio scelti sono il contesto di tutte le schermate: F2 per cambiarli.");
                ui.add_space(6.0);
                ui.label(RichText::new("Ciclo di lavoro tipico").strong());
                ui.label("1. Condomìni → Unità → Persone → Tabelle millesimali.\n2. Conti: preventivo per voce, riparti sulle tabelle.\n3. Rate: piano con delibera, poi Movimenti → Versamento rate.\n4. Movimenti → Fatture / Gestione; Banca per la riconciliazione.\n5. Consuntivo: bilancio, riparto individuale, conguaglio.\n6. Fisco: F24, CU e 770. Assemblee e verbali dal pulsante Assemblee.");
                ui.add_space(6.0);
                ui.label(RichText::new("Scorciatoie").strong());
                ui.label("F1 questa guida · F2 cambia condominio/esercizio · Esc chiude le finestre di servizio");
            });
            if !aperta || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.guida = false;
            }
        }
    }
}

fn applica_tema(ctx: &egui::Context, scuro: bool) {
    let mut v = if scuro { egui::Visuals::dark() } else { egui::Visuals::light() };
    if !scuro {
        v.panel_fill = Color32::from_rgb(243, 245, 248);
        v.window_fill = Color32::WHITE;
        v.extreme_bg_color = Color32::WHITE;
        v.faint_bg_color = Color32::from_rgb(228, 233, 241);
        v.selection.bg_fill = Color32::from_rgb(38, 99, 168);
        v.selection.stroke.color = Color32::WHITE;
        v.hyperlink_color = Color32::from_rgb(38, 99, 168);
    }
    ctx.set_visuals(v);
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.rev != self.rev_condomini {
            self.ricarica_condomini();
            self.rev_condomini = self.rev;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F2)) {
            self.selettore = true;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F1)) {
            self.guida = true;
        }
        if let Some(cmd) = self.barra_menu(ctx) {
            self.esegui_cmd(ctx, cmd);
        }
        self.barra_sezioni(ctx);
        self.barra_alta(ctx);
        self.barra_stato(ctx);

        let mut prossima: Option<(Sezione, Option<usize>)> = None;
        let mut azione_app: Option<AzioneApp> = None;
        egui::CentralPanel::default().show(ctx, |ui| {
            let Some(cond) = self.cond else {
                self.benvenuto(ui);
                return;
            };
            // barra delle viste del gruppo attivo
            let gruppo = &GRUPPI[self.gruppo];
            if gruppo.voci.len() > 1 {
                ui.horizontal_wrapped(|ui| {
                    for v in gruppo.voci {
                        let attiva = v.sez == self.sezione && v.tab == self.tab;
                        if ui.selectable_label(attiva, RichText::new(v.nome).size(14.0)).clicked() {
                            prossima = Some((v.sez, v.tab));
                        }
                    }
                });
                ui.separator();
            }
            let nome = self.condomini.iter().find(|c| c.id == cond).map(|c| c.nome.clone()).unwrap_or_default();
            let sezione = self.sezione;
            let schede = self.tab.is_none();
            let App { db, esercizio, rev, msg, richiesta, naviga, st, .. } = self;
            let mut c = Ctx { db, cond, cond_nome: &nome, esercizio: *esercizio, rev, msg, richiesta, naviga, app: &mut azione_app, schede };
            egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| match sezione {
                Sezione::Dashboard => dashboard::mostra(&mut c, &mut st.dashboard, ui),
                Sezione::Condomini => condomini::mostra(&mut c, &mut st.condomini, ui),
                Sezione::Agenda => agenda::mostra(&mut c, &mut st.agenda, ui),
                Sezione::Bilanci => bilanci::mostra(&mut c, &mut st.bilanci, ui),
                Sezione::Morosita => morosita::mostra(&mut c, &mut st.morosita, ui),
                Sezione::Banca => banca::mostra(&mut c, &mut st.banca, ui),
                Sezione::Fisco => fisco::mostra(&mut c, &mut st.fisco, ui),
                Sezione::Pratiche => pratiche::mostra(&mut c, &mut st.pratiche, ui),
                Sezione::Anagrafica => anagrafica::mostra(&mut c, &mut st.anagrafica, ui),
                Sezione::Millesimi => millesimi::mostra(&mut c, &mut st.millesimi, ui),
                Sezione::Preventivo => preventivo::mostra(&mut c, &mut st.preventivo, ui),
                Sezione::PianiRate => piani::mostra(&mut c, &mut st.piani, ui),
                Sezione::Incassi => incassi::mostra(&mut c, &mut st.incassi, ui),
                Sezione::Fornitori => fornitori::mostra(&mut c, &mut st.fornitori, ui),
                Sezione::Contabilita => contabilita::mostra(&mut c, &mut st.contabilita, ui),
                Sezione::Documenti => documenti::mostra(&mut c, &mut st.documenti, ui),
                Sezione::Segnalazioni => segnalazioni::mostra(&mut c, &mut st.segnalazioni, ui),
                Sezione::Assemblee => assemblee::mostra(&mut c, &mut st.assemblee, ui),
                Sezione::Bacheca => bacheca::mostra(&mut c, &mut st.bacheca, ui),
                Sezione::Conformita => conformita::mostra(&mut c, &mut st.conformita, ui),
                Sezione::Impostazioni => impostazioni::mostra(&mut c, &mut st.impostazioni, ui),
            });
            if let Some(s) = self.naviga.take() {
                prossima = Some((s, None));
            }
        });

        if let Some((s, t)) = prossima {
            self.vai(s, t);
        }
        match azione_app {
            Some(AzioneApp::Nuovo) => self.nuovo_cond = Some(Condominio::default()),
            Some(AzioneApp::Demo) => self.crea_demo(),
            Some(AzioneApp::Apri(id)) => {
                self.cond = Some(id);
                self.scegli_esercizio_predefinito();
                self.rev += 1;
            }
            None => {}
        }
        self.finestre_extra(ctx);
        self.finestre(ctx);
    }
}

/// Avvia l'applicazione nativa.
pub fn avvia(db: Db) -> eframe::Result<()> {
    let opzioni = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1360.0, 860.0]).with_min_inner_size([980.0, 600.0]).with_title("Gestione Condominio"),
        ..Default::default()
    };
    eframe::run_native("Gestione Condominio", opzioni, Box::new(|cc| Ok(Box::new(App::new(cc, db)))))
}

#[cfg(test)]
mod prova {
    use super::*;

    /// Disegna ogni schermata (con i dati di esempio) per alcuni frame: intercetta panic, unwrap e conti sballati.
    #[test]
    fn tutte_le_schermate_si_disegnano_con_i_dati_di_esempio() {
        let db = Db::in_memoria().unwrap();
        let cond = db.carica_demo().unwrap();
        let esercizio = db.elenco_esercizi(cond).unwrap().first().map(|e| e.id);
        let mut st = Stati::default();
        let (mut rev, mut msg, mut richiesta, mut naviga, mut app) = (1u64, None, None, None, None);
        let sezioni = [
            Sezione::Dashboard, Sezione::Condomini, Sezione::Agenda, Sezione::Anagrafica, Sezione::Millesimi, Sezione::Preventivo, Sezione::PianiRate,
            Sezione::Bilanci, Sezione::Incassi, Sezione::Morosita, Sezione::Fornitori, Sezione::Contabilita, Sezione::Banca,
            Sezione::Fisco, Sezione::Assemblee, Sezione::Segnalazioni, Sezione::Pratiche, Sezione::Bacheca, Sezione::Documenti,
            Sezione::Conformita, Sezione::Impostazioni,
        ];
        let ctx = egui::Context::default();
        let ass = db.elenco_assemblee(cond).unwrap()[0].id;
        let persona = db.persone_del_condominio(cond).unwrap()[1].id;
        st.assemblee.sel_prova(ass);
        st.incassi.persona_prova(persona);
        st.bilanci.persona_prova(persona);
        for s in sezioni {
            for tab in 0..6 {
                st.assemblee.tab_prova(tab);
                st.bilanci.imposta_tab(tab % 3);
                st.morosita.imposta_tab(tab % 4);
                st.fornitori.imposta_tab(tab % 3);
                st.fisco.imposta_tab(tab % 2);
                st.contabilita.imposta_tab(tab % 3);
                st.incassi.imposta_tab(tab % 4);
                st.bacheca.imposta_tab(tab % 2);
                st.anagrafica.imposta_tab(tab % 2);
                let _ = ctx.run(egui::RawInput::default(), |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let mut c = Ctx { db: &db, cond, cond_nome: "Esempio", esercizio, rev: &mut rev, msg: &mut msg, richiesta: &mut richiesta, naviga: &mut naviga, app: &mut app, schede: true };
                        match s {
                            Sezione::Dashboard => dashboard::mostra(&mut c, &mut st.dashboard, ui),
                            Sezione::Condomini => condomini::mostra(&mut c, &mut st.condomini, ui),
                            Sezione::Agenda => agenda::mostra(&mut c, &mut st.agenda, ui),
                            Sezione::Anagrafica => anagrafica::mostra(&mut c, &mut st.anagrafica, ui),
                            Sezione::Millesimi => millesimi::mostra(&mut c, &mut st.millesimi, ui),
                            Sezione::Preventivo => {
                                for tab in 0..4 {
                                    st.preventivo.imposta_tab(tab);
                                    preventivo::mostra(&mut c, &mut st.preventivo, ui);
                                }
                            }
                            Sezione::PianiRate => piani::mostra(&mut c, &mut st.piani, ui),
                            Sezione::Bilanci => bilanci::mostra(&mut c, &mut st.bilanci, ui),
                            Sezione::Incassi => incassi::mostra(&mut c, &mut st.incassi, ui),
                            Sezione::Morosita => morosita::mostra(&mut c, &mut st.morosita, ui),
                            Sezione::Fornitori => fornitori::mostra(&mut c, &mut st.fornitori, ui),
                            Sezione::Contabilita => contabilita::mostra(&mut c, &mut st.contabilita, ui),
                            Sezione::Banca => banca::mostra(&mut c, &mut st.banca, ui),
                            Sezione::Fisco => fisco::mostra(&mut c, &mut st.fisco, ui),
                            Sezione::Assemblee => assemblee::mostra(&mut c, &mut st.assemblee, ui),
                            Sezione::Segnalazioni => segnalazioni::mostra(&mut c, &mut st.segnalazioni, ui),
                            Sezione::Pratiche => pratiche::mostra(&mut c, &mut st.pratiche, ui),
                            Sezione::Bacheca => bacheca::mostra(&mut c, &mut st.bacheca, ui),
                            Sezione::Documenti => documenti::mostra(&mut c, &mut st.documenti, ui),
                            Sezione::Conformita => conformita::mostra(&mut c, &mut st.conformita, ui),
                            Sezione::Impostazioni => impostazioni::mostra(&mut c, &mut st.impostazioni, ui),
                        }
                    });
                });
            }
        }
    }
}
