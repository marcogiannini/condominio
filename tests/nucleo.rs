use condominio::anagrafica::*;
use condominio::fornitori::*;
use condominio::incassi::NuovoIncasso;
use condominio::legale::Gravita;
use condominio::preventivo::*;
use condominio::util::*;
use condominio::Db;

struct Scenario {
    db: Db,
    cond: i64,
    esercizio: i64,
    cc: i64,
    persone: Vec<i64>, // [rossi, bianchi, verdi]
    unita: Vec<i64>,
    tabella: i64,
    voce: i64,
    delibera: i64,
}

fn persona(db: &Db, cognome: &str, email: &str) -> i64 {
    db.salva_persona(&Persona { tipo: "fisica".into(), nome: "X".into(), cognome: cognome.into(), email: email.into(), ..Default::default() }).unwrap()
}

fn scenario() -> Scenario {
    let db = Db::in_memoria().unwrap();
    let cond = db.salva_condominio(&Condominio { nome: "Condominio Aurora".into(), ..Default::default() }).unwrap();
    let cc = db.crea_conto_corrente(cond, "Conto principale", "IT00X0000000000000000000000").unwrap();
    let rossi = persona(&db, "Rossi", "rossi@example.it");
    let bianchi = persona(&db, "Bianchi", "bianchi@example.it");
    let verdi = persona(&db, "Verdi", "");
    let mut unita = vec![];
    for i in ["1", "2", "3"] {
        unita.push(db.salva_unita(&Unita { condominio_id: cond, scala: "A".into(), interno: i.into(), tipo: "appartamento".into(), ..Default::default() }).unwrap());
    }
    // unità 1: multi-proprietario Rossi 60% / Bianchi 40%
    db.aggiungi_titolarita(unita[0], rossi, "proprietario", 600_000, false).unwrap();
    db.aggiungi_titolarita(unita[0], bianchi, "proprietario", 400_000, false).unwrap();
    db.aggiungi_titolarita(unita[1], bianchi, "proprietario", 1_000_000, false).unwrap();
    db.aggiungi_titolarita(unita[2], verdi, "proprietario", 1_000_000, false).unwrap();
    let tab = db.salva_tabella(&Tabella { condominio_id: cond, codice: "A".into(), nome: "Proprietà generale".into(), totale_micro: 1_000_000, ..Default::default() }).unwrap();
    db.imposta_millesimo(tab, unita[0], parse_millesimi("400").unwrap()).unwrap();
    db.imposta_millesimo(tab, unita[1], parse_millesimi("350").unwrap()).unwrap();
    db.imposta_millesimo(tab, unita[2], parse_millesimi("250").unwrap()).unwrap();
    let es = db.salva_esercizio(&Esercizio { condominio_id: cond, nome: "2026".into(), dal: "01/01/2026".into(), al: "31/12/2026".into(), ..Default::default() }).unwrap();
    // importo con centesimi "scomodi"
    let voce = db
        .salva_voce(&VoceSpesa { esercizio_id: es, codice: "1.01".into(), descrizione: "Pulizie".into(), importo_cents: 100_001, tabella_id: Some(tab), ..Default::default() })
        .unwrap();
    let delibera = db
        .salva_delibera(&Delibera { condominio_id: cond, esercizio_id: Some(es), data: "15/01/2026".into(), tipo: "preventivo".into(), oggetto: "Approvazione preventivo 2026".into(), riferimento: "Verbale 15/01/2026 p.2".into(), ..Default::default() })
        .unwrap();
    Scenario { db, cond, esercizio: es, cc, persone: vec![rossi, bianchi, verdi], unita, tabella: tab, voce, delibera }
}

fn piano_attivo(s: &Scenario, n_rate: i64) -> i64 {
    let p = s
        .db
        .crea_piano(&Piano { esercizio_id: s.esercizio, nome: "Ordinario 2026".into(), tipo: "ordinario".into(), delibera_id: Some(s.delibera), n_rate, prima_scadenza: "31/01/2026".into(), cadenza_mesi: 1, ..Default::default() })
        .unwrap();
    s.db.imposta_voce_piano(p, s.voce, 100_001).unwrap();
    s.db.attiva_piano(p).unwrap();
    p
}

fn incasso(s: &Scenario, persona: i64, importo: Cents) -> NuovoIncasso {
    NuovoIncasso { condominio_id: s.cond, persona_id: persona, conto_corrente_id: Some(s.cc), data: "2026-02-10".into(), importo, metodo: "bonifico".into(), ..Default::default() }
}

#[test]
fn riparto_rate_somma_esattamente_al_piano() {
    let s = scenario();
    let p = piano_attivo(&s, 3);
    let rate = s.db.rate_piano(p).unwrap();
    let tot: Cents = rate.iter().map(|r| r.importo).sum();
    assert_eq!(tot, 100_001, "nessun centesimo perso");
    // l'unità 1 è divisa tra due proprietari: devono comparire entrambi
    assert!(rate.iter().any(|r| r.persona_id == s.persone[0] && r.unita_id == s.unita[0]));
    assert!(rate.iter().any(|r| r.persona_id == s.persone[1] && r.unita_id == s.unita[0]));
    // gestione: i crediti verso i condòmini sono pari al totale emesso
    assert_eq!(s.db.situazione_gestione(s.cond).unwrap().crediti_condomini, 100_001);
}

#[test]
fn piano_richiede_delibera_e_blocca_voci() {
    let s = scenario();
    let p = s
        .db
        .crea_piano(&Piano { esercizio_id: s.esercizio, nome: "Senza delibera".into(), tipo: "straordinario".into(), n_rate: 2, prima_scadenza: "01/03/2026".into(), cadenza_mesi: 1, ..Default::default() })
        .unwrap();
    s.db.imposta_voce_piano(p, s.voce, 50_000).unwrap();
    let e = s.db.attiva_piano(p).unwrap_err().to_string();
    assert!(e.contains("Delibera"), "{e}");
    s.db.imposta_delibera_piano(p, Some(s.delibera)).unwrap();
    s.db.attiva_piano(p).unwrap();
    // voce ancorata a piano attivo: non eliminabile né modificabile nell'importo
    assert!(s.db.elimina_voce(s.voce).unwrap_err().to_string().contains("ATTIVO"));
    let mut v = s.db.elenco_voci(s.esercizio).unwrap().remove(0);
    v.importo_cents += 1;
    assert!(s.db.salva_voce(&v).is_err());
    // annullando il piano si sblocca
    s.db.annulla_piano(p).unwrap();
    s.db.elimina_voce(s.voce).unwrap();
}

#[test]
fn incasso_parte_dalla_rata_piu_vecchia_e_l_eccedenza_va_nel_salvadanaio() {
    let s = scenario();
    piano_attivo(&s, 3);
    let verdi = s.persone[2];
    let aperte = s.db.rate_aperte(s.cond, verdi).unwrap();
    assert_eq!(aperte.len(), 3);
    let prima = aperte[0].residuo();
    // pago una rata e mezza
    let importo = prima + prima / 2;
    s.db.registra_incasso(&incasso(&s, verdi, importo)).unwrap();
    let dopo = s.db.rate_aperte(s.cond, verdi).unwrap();
    assert_eq!(dopo.len(), 2, "la prima rata è chiusa");
    assert_eq!(dopo[0].pagato, importo - prima);
    // pago molto più del dovuto: il resto finisce nel Salvadanaio
    let residuo: Cents = dopo.iter().map(|r| r.residuo()).sum();
    s.db.registra_incasso(&incasso(&s, verdi, residuo + 5_000)).unwrap();
    assert!(s.db.rate_aperte(s.cond, verdi).unwrap().is_empty());
    assert_eq!(s.db.saldo_salvadanaio(s.cond, verdi).unwrap(), 5_000);
    let sit = s.db.situazione_persona(s.cond, verdi).unwrap();
    assert_eq!(sit.residuo, 0);
}

#[test]
fn distribuzione_manuale_e_compensazione_salvadanaio_su_scelta() {
    let s = scenario();
    piano_attivo(&s, 3);
    let verdi = s.persone[2];
    // credito pregresso: pago in anticipo oltre il dovuto di un'altra persona? Creo credito con eccedenza.
    let tutte: Cents = s.db.rate_aperte(s.cond, verdi).unwrap().iter().map(|r| r.residuo()).sum();
    let inc0 = s.db.registra_incasso(&incasso(&s, verdi, tutte + 10_000)).unwrap();
    assert_eq!(s.db.saldo_salvadanaio(s.cond, verdi).unwrap(), 10_000);
    // storno: l'eccedenza sparisce, le rate tornano aperte
    s.db.storna_incasso(inc0, "2026-02-11").unwrap();
    assert_eq!(s.db.saldo_salvadanaio(s.cond, verdi).unwrap(), 0);
    assert_eq!(s.db.rate_aperte(s.cond, verdi).unwrap().len(), 3);
    // ricreo credito e lo uso solo quando decido io, con distribuzione manuale sull'ultima rata
    s.db.registra_incasso(&incasso(&s, verdi, tutte + 10_000)).unwrap();
    // (tutte pagate, credito 10_000) -> nuovo piano per avere rate aperte
    let p2 = s
        .db
        .crea_piano(&Piano { esercizio_id: s.esercizio, nome: "Integrativo".into(), tipo: "integrativo".into(), delibera_id: Some(s.delibera), n_rate: 2, prima_scadenza: "01/06/2026".into(), cadenza_mesi: 1, ..Default::default() })
        .unwrap();
    s.db.imposta_voce_piano(p2, s.voce, 60_000).unwrap();
    s.db.attiva_piano(p2).unwrap();
    let aperte = s.db.rate_aperte(s.cond, verdi).unwrap();
    assert_eq!(aperte.len(), 2);
    let ultima = aperte[1].rata_id;
    let mut n = incasso(&s, verdi, 0);
    n.usa_salvadanaio = 4_000;
    n.distribuzione = Some(vec![(ultima, 4_000)]);
    s.db.registra_incasso(&n).unwrap();
    assert_eq!(s.db.saldo_salvadanaio(s.cond, verdi).unwrap(), 6_000);
    let aperte = s.db.rate_aperte(s.cond, verdi).unwrap();
    assert_eq!(aperte[1].pagato, 4_000);
    assert_eq!(aperte[0].pagato, 0, "distribuzione manuale rispettata");
    // non si può compensare più del credito disponibile
    let mut n = incasso(&s, verdi, 0);
    n.usa_salvadanaio = 6_001;
    assert!(s.db.registra_incasso(&n).is_err());
}

#[test]
fn storno_bloccato_se_il_credito_e_gia_stato_usato() {
    let s = scenario();
    let p = piano_attivo(&s, 1);
    let verdi = s.persone[2];
    let dovuto: Cents = s.db.rate_aperte(s.cond, verdi).unwrap().iter().map(|r| r.residuo()).sum();
    let inc = s.db.registra_incasso(&incasso(&s, verdi, dovuto + 3_000)).unwrap();
    // secondo piano e uso del credito
    let p2 = s
        .db
        .crea_piano(&Piano { esercizio_id: s.esercizio, nome: "Extra".into(), tipo: "integrativo".into(), delibera_id: Some(s.delibera), n_rate: 1, prima_scadenza: "01/07/2026".into(), cadenza_mesi: 1, ..Default::default() })
        .unwrap();
    s.db.imposta_voce_piano(p2, s.voce, 30_000).unwrap();
    s.db.attiva_piano(p2).unwrap();
    let mut n = incasso(&s, verdi, 0);
    n.usa_salvadanaio = 3_000;
    s.db.registra_incasso(&n).unwrap();
    let e = s.db.storna_incasso(inc, "2026-03-01").unwrap_err().to_string();
    assert!(e.contains("già stato utilizzato"), "{e}");
    // nulla è cambiato (atomicità)
    assert_eq!(s.db.saldo_salvadanaio(s.cond, verdi).unwrap(), 0);
    let _ = p;
}

#[test]
fn la_contabilita_resta_coerente_dopo_incassi_e_storni() {
    let s = scenario();
    piano_attivo(&s, 4);
    for (i, pers) in s.persone.iter().enumerate() {
        let dovuto: Cents = s.db.rate_aperte(s.cond, *pers).unwrap().iter().map(|r| r.residuo()).sum();
        let id = s.db.registra_incasso(&incasso(&s, *pers, dovuto / 3 + i as i64)).unwrap();
        if i == 1 {
            s.db.storna_incasso(id, "2026-02-20").unwrap();
        }
    }
    let c = s.db.controlli_conformita(s.cond).unwrap();
    assert!(!c.iter().any(|x| x.gravita == Gravita::Errore), "{c:#?}");
    assert!(s.db.verifica_movimenti(s.cond).unwrap().is_empty(), "movimenti incoerenti con i documenti");
    // saldo banca = incassi validi
    let banca = s.db.saldo_conto_corrente(s.cc).unwrap();
    let validi: Cents = s.db.elenco_incassi(s.cond, None).unwrap().iter().filter(|i| i.stato == "valido").map(|i| i.importo).sum();
    assert_eq!(banca, validi);
}

#[test]
fn estratto_conto_ha_saldo_progressivo_coerente() {
    let s = scenario();
    piano_attivo(&s, 2);
    let bianchi = s.persone[1];
    s.db.registra_incasso(&incasso(&s, bianchi, 10_000)).unwrap();
    let est = s.db.estratto_conto(s.cond, bianchi).unwrap();
    let sit = s.db.situazione_persona(s.cond, bianchi).unwrap();
    assert_eq!(est.last().unwrap().saldo, sit.residuo);
}

#[test]
fn ritenute_calcolo_e_soglia_500() {
    let s = scenario();
    let f4 = s.db.salva_fornitore(&Fornitore { condominio_id: s.cond, ragione_sociale: "Impresa Pulizie srl".into(), regime_ritenuta: "appalti4".into(), ..Default::default() }).unwrap();
    let prof = s.db.salva_fornitore(&Fornitore { condominio_id: s.cond, ragione_sociale: "Ing. Neri".into(), regime_ritenuta: "professionista20".into(), ..Default::default() }).unwrap();
    assert_eq!(calcola_ritenuta("appalti4", 100_000), 4_000);
    assert_eq!(calcola_ritenuta("professionista20", 100_000), 20_000);

    let mut ids = vec![];
    for (i, imp) in [600_000i64, 300_000].iter().enumerate() {
        let id = s
            .db
            .registra_fattura(&NuovaFattura { condominio_id: s.cond, esercizio_id: s.esercizio, fornitore_id: f4, voce_id: Some(s.voce), numero: format!("F{}", i + 1), data: format!("2026-03-{:02}", 10 + i), descrizione: "Pulizie".into(), imponibile: *imp, iva: imp / 5 })
            .unwrap();
        s.db.paga_fattura(id, &format!("2026-03-{:02}", 12 + i), s.cc).unwrap();
        ids.push(id);
    }
    // duplicato rifiutato
    assert!(s.db.registra_fattura(&NuovaFattura { condominio_id: s.cond, esercizio_id: s.esercizio, fornitore_id: f4, numero: "F1".into(), data: "2026-04-01".into(), imponibile: 1000, ..Default::default() }).is_err());

    let g = s.db.ritenute_da_versare(s.cond).unwrap();
    // 4% di 6.000 = 240 + 4% di 3.000 = 120 -> 360 € < 500 € => 30 giugno
    assert_eq!(g.len(), 1);
    assert_eq!(g[0].importo, 36_000);
    assert_eq!(g[0].codice_tributo, "1019");
    assert_eq!(g[0].scadenza, "2026-06-30");

    // una fattura grande supera i 500 € cumulati: scadenza il 16 del mese dopo
    let id3 = s
        .db
        .registra_fattura(&NuovaFattura { condominio_id: s.cond, esercizio_id: s.esercizio, fornitore_id: f4, numero: "F3".into(), data: "2026-04-01".into(), imponibile: 500_000, ..Default::default() })
        .unwrap();
    s.db.paga_fattura(id3, "2026-04-05", s.cc).unwrap();
    let g = s.db.ritenute_da_versare(s.cond).unwrap();
    assert_eq!(g.len(), 1);
    assert_eq!(g[0].importo, 36_000 + 20_000);
    assert_eq!(g[0].scadenza, "2026-05-16");

    // professionista: 20%, F24 1040, 16 del mese successivo
    let fp = s
        .db
        .registra_fattura(&NuovaFattura { condominio_id: s.cond, esercizio_id: s.esercizio, fornitore_id: prof, numero: "P1".into(), data: "2026-05-02".into(), imponibile: 100_000, ..Default::default() })
        .unwrap();
    s.db.paga_fattura(fp, "2026-05-20", s.cc).unwrap();
    let g = s.db.ritenute_da_versare(s.cond).unwrap();
    let gp = g.iter().find(|x| x.codice_tributo == "1040").unwrap();
    assert_eq!(gp.importo, 20_000);
    assert_eq!(gp.scadenza, "2026-06-16");

    // versamento F24: l'erario torna a zero
    for gr in &g {
        s.db.versa_ritenute(s.cond, gr, "2026-06-15", s.cc).unwrap();
    }
    assert!(s.db.ritenute_da_versare(s.cond).unwrap().is_empty());
    assert_eq!(s.db.situazione_gestione(s.cond).unwrap().ritenute_da_versare, 0);
    assert!(s.db.verifica_movimenti(s.cond).unwrap().is_empty(), "movimenti incoerenti con i documenti");
    // fattura pagata con ritenuta versata: pagamento non stornabile
    assert!(s.db.storna_pagamento_fattura(ids[0], "2026-07-01").is_err());
}

#[test]
fn prescrizione_quinquennale_con_interruzione() {
    let s = scenario();
    let p = s
        .db
        .crea_piano(&Piano { esercizio_id: s.esercizio, nome: "Vecchio".into(), tipo: "ordinario".into(), delibera_id: Some(s.delibera), n_rate: 1, prima_scadenza: "01/03/2020".into(), cadenza_mesi: 1, ..Default::default() })
        .unwrap();
    s.db.imposta_voce_piano(p, s.voce, 10_000).unwrap();
    s.db.attiva_piano(p).unwrap();
    let oggi = parse_data("01/09/2025").unwrap(); // 5 anni dal 01/03/2020 = 01/03/2025: già prescritte
    let al = s.db.allerte_prescrizione(s.cond, oggi).unwrap();
    assert!(!al.is_empty() && al.iter().all(|a| a.prescritta));
    // atto interruttivo il 01/06/2025: il termine riparte, nessuna prescritta
    for a in &al {
        s.db.registra_interruzione(a.rata_id, "2025-06-01").unwrap();
    }
    let al = s.db.allerte_prescrizione(s.cond, parse_data("01/05/2025").unwrap()).unwrap();
    assert!(al.iter().all(|a| !a.prescritta && a.interrotta));
}

#[test]
fn archivio_documenti_con_ricerca_full_text() {
    let s = scenario();
    let dir = std::env::temp_dir().join(format!("cond-doc-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("verbale.txt");
    std::fs::write(&f, "Assemblea ordinaria: sostituzione dell'ascensore e rifacimento della facciata").unwrap();
    let mut db = s.db;
    db.archivio_dir = dir.join("archivio");
    let id = db.archivia_documento(s.cond, "verbale", "Verbale assemblea 2026", "Approvazione lavori", &f, "2026-01-15").unwrap();
    db.archivia_documento(s.cond, "polizza", "Polizza globale fabbricati", "Assicurazione", &f, "2026-02-01").unwrap();
    assert_eq!(db.elenco_documenti(s.cond, None, "").unwrap().len(), 2);
    assert_eq!(db.elenco_documenti(s.cond, Some("polizza"), "").unwrap().len(), 1);
    // ricerca nel testo del file, anche per prefisso e senza accenti/maiuscole
    assert_eq!(db.elenco_documenti(s.cond, None, "ASCENSORE").unwrap().len(), 2);
    assert_eq!(db.elenco_documenti(s.cond, None, "fac").unwrap().len(), 2);
    assert_eq!(db.elenco_documenti(s.cond, None, "assicurazione").unwrap().len(), 1);
    assert_eq!(db.elenco_documenti(s.cond, None, "inesistente").unwrap().len(), 0);
    // input ostile non rompe la query
    assert!(db.elenco_documenti(s.cond, None, "\"* OR (").is_ok());
    db.elimina_documento(id).unwrap();
    assert_eq!(db.elenco_documenti(s.cond, None, "ascensore").unwrap().len(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn segnalazioni_e_bacheca() {
    let s = scenario();
    let t = s.db.crea_ticket(s.cond, Some(s.unita[0]), Some(s.persone[0]), "Ascensore bloccato", "Fermo al 3° piano", 3).unwrap();
    s.db.crea_ticket(s.cond, None, None, "Lampadina scala B", "", 0).unwrap();
    let v = s.db.elenco_ticket(s.cond, None, true).unwrap();
    assert_eq!(v[0].id, t, "priorità più alta per prima");
    s.db.cambia_stato_ticket(t, "in_carico", "Amministratore").unwrap();
    s.db.aggiungi_commento_ticket(t, "Amministratore", "Tecnico chiamato").unwrap();
    assert_eq!(s.db.commenti_ticket(t).unwrap().len(), 2);
    assert!(s.db.notifiche_non_lette(s.cond).unwrap() >= 3);
    s.db.segna_notifiche_lette(s.cond).unwrap();
    assert_eq!(s.db.notifiche_non_lette(s.cond).unwrap(), 0);

    let c = s.db.salva_comunicazione(&condominio::bacheca::Comunicazione { condominio_id: s.cond, tipo: "assemblea".into(), titolo: "Convocazione".into(), testo: "Ordine del giorno...".into(), data_evento: Some("2026-03-20".into()), ..Default::default() }).unwrap();
    let n = s.db.pubblica_comunicazione(c, true).unwrap();
    assert_eq!(n, 2, "solo i condòmini con e-mail (Verdi non ha indirizzo)");
    assert_eq!(s.db.email_in_coda().unwrap(), 2);
    // senza SMTP configurato l'invio è rifiutato con messaggio chiaro e le mail restano in coda
    assert!(s.db.invia_coda_email().is_err());
    assert_eq!(s.db.email_in_coda().unwrap(), 2);
}

#[test]
fn tabella_millesimale_e_quote() {
    let s = scenario();
    assert_eq!(s.db.somma_millesimi(s.tabella).unwrap(), 1_000_000);
    let r = s.db.riparto_per_tabella(s.tabella, 100_001).unwrap();
    assert_eq!(r.iter().map(|x| x.1).sum::<i64>(), 100_001);
    assert_eq!(s.db.pagatori_unita(s.unita[0]).unwrap().len(), 2);
    // l'inquilino che paga le spese sostituisce i proprietari
    let inq = persona(&s.db, "Inquilino", "");
    s.db.aggiungi_titolarita(s.unita[2], inq, "inquilino", 1_000_000, true).unwrap();
    assert_eq!(s.db.pagatori_unita(s.unita[2]).unwrap(), vec![(inq, 1_000_000)]);
    assert!(s.db.elimina_persona(inq).is_err(), "persona con titolarità non eliminabile");
}

#[test]
fn backup_consistente() {
    let s = scenario();
    let f = std::env::temp_dir().join(format!("cond-backup-{}.db", std::process::id()));
    s.db.backup(&f).unwrap();
    assert!(std::fs::metadata(&f).unwrap().len() > 0);
    let _ = std::fs::remove_file(f);
}
