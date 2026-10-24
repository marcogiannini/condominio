//! Struttura di navigazione, ispirata a Danea Domustudio:
//! barra dei menu, barra dei pulsanti-sezione e, per ogni sezione, le viste (schede) disponibili.

use super::Sezione;
use eframe::egui::Color32;

#[derive(Clone, Copy)]
pub struct Voce {
    pub sez: Sezione,
    /// Scheda interna da mostrare (None = la schermata mostra le proprie schede).
    pub tab: Option<usize>,
    pub nome: &'static str,
}

const fn v(sez: Sezione, tab: Option<usize>, nome: &'static str) -> Voce {
    Voce { sez, tab, nome }
}

pub struct Gruppo {
    pub nome: &'static str,
    pub icona: &'static str,
    pub colore: Color32,
    pub voci: &'static [Voce],
}

use Sezione as S;

const BLU: Color32 = Color32::from_rgb(40, 110, 190);
const ROSSO: Color32 = Color32::from_rgb(210, 60, 40);
const VERDE: Color32 = Color32::from_rgb(50, 150, 70);
const ARANCIO: Color32 = Color32::from_rgb(230, 140, 20);
const GRIGIO: Color32 = Color32::from_rgb(90, 90, 100);

/// Pulsanti della barra principale, nello stesso ordine di Domustudio:
/// Agenda … Movimenti | Attività … Fornitori | Calc., Pubblica, Aiuto.
pub static GRUPPI: &[Gruppo] = &[
    Gruppo { nome: "Agenda", icona: "📅", colore: ROSSO, voci: &[v(S::Agenda, None, "Agenda"), v(S::Dashboard, None, "Cruscotto e notifiche")] },
    Gruppo {
        nome: "Condomìni",
        icona: "🏢",
        colore: BLU,
        voci: &[
            v(S::Condomini, None, "Condomìni ed esercizi"),
            v(S::Preventivo, Some(1), "Delibere"),
            v(S::Preventivo, Some(2), "Esercizi (elenco)"),
            v(S::Contabilita, Some(0), "Conti correnti"),
            v(S::Impostazioni, None, "Opzioni e backup"),
        ],
    },
    Gruppo { nome: "Unità", icona: "🚪", colore: BLU, voci: &[v(S::Unita, None, "Unità e titolarità")] },
    Gruppo { nome: "Tabelle", icona: "📊", colore: ARANCIO, voci: &[v(S::Millesimi, None, "Tabelle millesimali"), v(S::Preventivo, Some(3), "Riparti e riduzioni")] },
    Gruppo { nome: "Conti", icona: "📒", colore: GRIGIO, voci: &[v(S::Conti, None, "Piano dei conti e preventivo")] },
    Gruppo {
        nome: "Rate",
        icona: "💶",
        colore: BLU,
        voci: &[
            v(S::Rate, None, "Rate per condòmino"),
            v(S::PianiRate, None, "Piani rate"),
            v(S::Morosita, Some(0), "Rate scadute"),
            v(S::Morosita, Some(1), "Solleciti"),
            v(S::Morosita, Some(2), "Storico solleciti"),
            v(S::Morosita, Some(3), "Interessi di mora"),
        ],
    },
    Gruppo {
        nome: "Movimenti",
        icona: "🔁",
        colore: ARANCIO,
        voci: &[
            v(S::Contabilita, Some(1), "Movimenti di gestione"),
            v(S::Fornitori, Some(1), "Fatture"),
            v(S::Incassi, Some(2), "Movimenti personali"),
            v(S::Incassi, Some(0), "Versamenti rate"),
            v(S::Contabilita, Some(0), "Giroconti e conti"),
            v(S::Incassi, Some(3), "Operazioni su fondi"),
            v(S::Banca, None, "Banca"),
        ],
    },
    Gruppo { nome: "Attività", icona: "🛠", colore: BLU, voci: &[v(S::Segnalazioni, None, "Segnalazioni guasto"), v(S::Pratiche, None, "Interventi, sinistri, legali")] },
    Gruppo { nome: "Parti comuni", icona: "🏠", colore: VERDE, voci: &[v(S::Documenti, None, "Documenti e archivio")] },
    Gruppo { nome: "Assemblee", icona: "👥", colore: ARANCIO, voci: &[v(S::Assemblee, None, "Assemblee e verbali")] },
    Gruppo { nome: "Persone", icona: "👤", colore: ARANCIO, voci: &[v(S::Persone, None, "Persone")] },
    Gruppo {
        nome: "Fornitori",
        icona: "🚚",
        colore: ARANCIO,
        voci: &[v(S::Fornitori, Some(0), "Fornitori"), v(S::Fornitori, Some(2), "Ritenute da versare")],
    },
    Gruppo { nome: "Pubblica", icona: "🌐", colore: VERDE, voci: &[v(S::Bacheca, Some(0), "Bacheca e circolari"), v(S::Bacheca, Some(1), "Registro e-mail")] },
];

/// Primo indice di gruppo di ogni blocco della barra (Agenda…Movimenti | Attività…Fornitori | Calc., Pubblica, Aiuto).
pub const INIZIO_BLOCCO_2: usize = 7;
pub const INIZIO_BLOCCO_3: usize = 12;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Cmd {
    Vai(Sezione, Option<usize>),
    StampaRendiconto,
    NuovoCondominio,
    Demo,
    Backup,
    Seleziona,
    Guida,
    Calcolatrice,
    Esci,
}

pub struct VoceMenu {
    pub nome: &'static str,
    pub cmd: Cmd,
    pub tasto: &'static str,
}

const fn m(nome: &'static str, cmd: Cmd, tasto: &'static str) -> VoceMenu {
    VoceMenu { nome, cmd, tasto }
}

use Cmd::Vai;

/// Barra dei menu: File, Preventivo, Consuntivo, Comunicazioni, Fisco, Strumenti, ?
pub static MENU: &[(&str, &[VoceMenu])] = &[
    (
        "File",
        &[
            m("Nuovo condominio…", Cmd::NuovoCondominio, ""),
            m("Cambia condominio o esercizio…", Cmd::Seleziona, "F2"),
            m("Carica condominio di esempio", Cmd::Demo, ""),
            m("Crea backup del database…", Cmd::Backup, ""),
            m("Esci", Cmd::Esci, ""),
        ],
    ),
    (
        "Preventivo",
        &[
            m("Piano dei conti e preventivo", Vai(S::Conti, None), ""),
            m("Riparti e riduzioni", Vai(S::Preventivo, Some(3)), ""),
            m("Piani rate", Vai(S::PianiRate, None), ""),
            m("Delibere", Vai(S::Preventivo, Some(1)), ""),
        ],
    ),
    (
        "Consuntivo",
        &[
            m("Preventivo e consuntivo", Vai(S::Bilanci, Some(0)), ""),
            m("Riparto per condòmino e bilancio individuale", Vai(S::Bilanci, Some(1)), ""),
            m("Conguaglio", Vai(S::Bilanci, Some(2)), ""),
            m("Stampa rendiconto", Cmd::StampaRendiconto, ""),
        ],
    ),
    (
        "Comunicazioni",
        &[
            m("Bacheca e circolari", Vai(S::Bacheca, Some(0)), ""),
            m("Registro e-mail", Vai(S::Bacheca, Some(1)), ""),
            m("Solleciti di pagamento", Vai(S::Morosita, Some(1)), ""),
            m("Convocazione assemblee", Vai(S::Assemblee, None), ""),
        ],
    ),
    (
        "Fisco",
        &[
            m("Ritenute d'acconto – versamento F24", Vai(S::Fornitori, Some(2)), ""),
            m("Certificazioni a fornitori e CU", Vai(S::Fisco, Some(0)), ""),
            m("Modello 770", Vai(S::Fisco, Some(1)), ""),
            m("Conformità legale", Vai(S::Conformita, Some(0)), ""),
            m("Prescrizione dei crediti", Vai(S::Conformita, Some(1)), ""),
        ],
    ),
    (
        "Strumenti",
        &[
            m("Importa fatture elettroniche…", Vai(S::Fornitori, Some(1)), ""),
            m("Importa anagrafica da CSV…", Vai(S::Persone, None), ""),
            m("Riconciliazione bancaria", Vai(S::Banca, None), ""),
            m("Opzioni e backup", Vai(S::Impostazioni, None), ""),
            m("Calcolatrice", Cmd::Calcolatrice, ""),
        ],
    ),
    ("?", &[m("Guida rapida e scorciatoie", Cmd::Guida, "F1")]),
];
