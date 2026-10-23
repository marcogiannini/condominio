//! Struttura di navigazione, ispirata a Danea Domustudio:
//! barra dei menu, barra dei pulsanti-sezione e, per ogni sezione, le viste (schede) disponibili.

use super::Sezione;

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
    pub voci: &'static [Voce],
}

use Sezione as S;

/// Pulsanti della barra delle sezioni (Agenda, Condomìni, Unità, Tabelle, Conti, Rate, Movimenti, …).
pub static GRUPPI: &[Gruppo] = &[
    Gruppo { nome: "Agenda", icona: "📅", voci: &[v(S::Dashboard, None, "Cruscotto"), v(S::Agenda, None, "Scadenze e promemoria")] },
    Gruppo {
        nome: "Condomìni",
        icona: "🏢",
        voci: &[
            v(S::Condomini, None, "Condomìni"),
            v(S::Preventivo, Some(2), "Esercizi"),
            v(S::Preventivo, Some(1), "Delibere"),
            v(S::Contabilita, Some(0), "Conti correnti"),
            v(S::Impostazioni, None, "Opzioni e backup"),
        ],
    },
    Gruppo { nome: "Unità", icona: "🚪", voci: &[v(S::Anagrafica, Some(1), "Unità e titolarità")] },
    Gruppo { nome: "Tabelle", icona: "📊", voci: &[v(S::Millesimi, None, "Tabelle millesimali"), v(S::Preventivo, Some(3), "Riparti e riduzioni")] },
    Gruppo { nome: "Conti", icona: "📒", voci: &[v(S::Preventivo, Some(0), "Piano dei conti e preventivo")] },
    Gruppo {
        nome: "Rate",
        icona: "💶",
        voci: &[
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
        voci: &[
            v(S::Contabilita, Some(1), "Gestione"),
            v(S::Fornitori, Some(1), "Fatture"),
            v(S::Incassi, Some(0), "Versamento rate"),
            v(S::Incassi, Some(2), "Personali"),
            v(S::Contabilita, Some(0), "Giroconti"),
            v(S::Incassi, Some(3), "Operazioni su fondi"),
            v(S::Banca, None, "Banca"),
        ],
    },
    Gruppo { nome: "Attività", icona: "🛠", voci: &[v(S::Segnalazioni, None, "Segnalazioni guasto"), v(S::Pratiche, None, "Interventi, sinistri, legali")] },
    Gruppo { nome: "Archivio", icona: "🗂", voci: &[v(S::Documenti, None, "Documenti")] },
    Gruppo { nome: "Assemblee", icona: "👥", voci: &[v(S::Assemblee, None, "Assemblee e verbali")] },
    Gruppo { nome: "Persone", icona: "👤", voci: &[v(S::Anagrafica, Some(0), "Persone")] },
    Gruppo {
        nome: "Fornitori",
        icona: "🚚",
        voci: &[v(S::Fornitori, Some(0), "Fornitori"), v(S::Fornitori, Some(2), "Ritenute da versare")],
    },
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Cmd {
    Vai(Sezione, Option<usize>),
    StampaRendiconto,
    NuovoCondominio,
    Demo,
    Backup,
    Seleziona,
    Guida,
    Tema,
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
            m("Piano dei conti e preventivo", Vai(S::Preventivo, Some(0)), ""),
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
            m("Importa anagrafica da CSV…", Vai(S::Anagrafica, Some(0)), ""),
            m("Riconciliazione bancaria", Vai(S::Banca, None), ""),
            m("Opzioni e backup", Vai(S::Impostazioni, None), ""),
            m("Tema chiaro / scuro", Cmd::Tema, ""),
        ],
    ),
    ("?", &[m("Guida rapida e scorciatoie", Cmd::Guida, "F1")]),
];
