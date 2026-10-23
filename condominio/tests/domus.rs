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


fn fornitore(s: &Scenario) -> i64 {
    s.db.salva_fornitore(&Fornitore { condominio_id: s.cond, ragione_sociale: "Pulizie Srl".into(), regime_ritenuta: "nessuna".into(), ..Default::default() }).unwrap()
}

fn fattura(s: &Scenario, forn: i64, num: &str, imponibile: Cents) -> i64 {
    s.db.registra_fattura(&NuovaFattura {
        condominio_id: s.cond,
        esercizio_id: s.esercizio,
        fornitore_id: forn,
        voce_id: Some(s.voce),
        numero: num.into(),
        data: "2026-03-01".into(),
        descrizione: "pulizie".into(),
        imponibile,
        iva: 0,
    })
    .unwrap()
}

fn delibera_rendiconto(s: &Scenario) -> i64 {
    s.db.salva_delibera(&Delibera { condominio_id: s.cond, esercizio_id: Some(s.esercizio), data: "20/04/2027".into(), tipo: "rendiconto".into(), oggetto: "Approvazione rendiconto 2026".into(), riferimento: "Verbale 20/04/2027 p.1".into(), ..Default::default() }).unwrap()
}

#[test]
fn consuntivo_inferiore_al_preventivo_accredita_il_salvadanaio_e_si_annulla() {
    let s = scenario();
    piano_attivo(&s, 2);
    let f = fornitore(&s);
    fattura(&s, f, "1", 90_000);

    let righe = s.db.righe_consuntivo(s.esercizio).unwrap();
    assert_eq!(righe[0].preventivo, 100_001);
    assert_eq!(righe[0].consuntivo, 90_000);
    assert_eq!(righe[0].scostamento(), -10_001);

    let quote = s.db.quote_consuntivo(s.esercizio).unwrap();
    assert_eq!(quote.iter().map(|q| q.consuntivo).sum::<Cents>(), 90_000, "nessun centesimo perso nel riparto");
    assert_eq!(quote.iter().map(|q| q.emesso).sum::<Cents>(), 100_001);
    assert!(quote.iter().all(|q| q.conguaglio() < 0));

    // senza delibera di rendiconto non si emette
    assert!(s.db.emetti_conguaglio(s.esercizio, s.delibera, 1, "2027-05-31", 1).is_err());
    let d = delibera_rendiconto(&s);
    let (deb, cred) = s.db.emetti_conguaglio(s.esercizio, d, 1, "2027-05-31", 1).unwrap();
    assert_eq!((deb, cred), (0, 3));
    let wallet: Cents = s.persone.iter().map(|p| s.db.saldo_salvadanaio(s.cond, *p).unwrap()).sum();
    assert_eq!(wallet, 10_001);
    assert!(s.db.verifica_movimenti(s.cond).unwrap().is_empty(), "movimenti incoerenti con i documenti");
    assert!(s.db.emetti_conguaglio(s.esercizio, d, 1, "2027-05-31", 1).unwrap_err().to_string().contains("già stato emesso"));

    s.db.annulla_conguaglio(s.esercizio).unwrap();
    let wallet: Cents = s.persone.iter().map(|p| s.db.saldo_salvadanaio(s.cond, *p).unwrap()).sum();
    assert_eq!(wallet, 0);
    assert!(s.db.stato_conguaglio(s.esercizio).unwrap().is_none());
    assert!(s.db.verifica_movimenti(s.cond).unwrap().is_empty(), "movimenti incoerenti con i documenti");
}

#[test]
fn consuntivo_superiore_al_preventivo_genera_rate_di_conguaglio() {
    let s = scenario();
    let p = piano_attivo(&s, 1);
    let f = fornitore(&s);
    fattura(&s, f, "1", 120_000);
    // chi ha già pagato tutto il preventivo resta a debito solo per la differenza
    for pers in &s.persone {
        let aperte = s.db.rate_aperte(s.cond, *pers).unwrap();
        let tot: Cents = aperte.iter().map(|r| r.residuo()).sum();
        if tot > 0 {
            s.db.registra_incasso(&incasso(&s, *pers, tot)).unwrap();
        }
    }
    let d = delibera_rendiconto(&s);
    let (deb, cred) = s.db.emetti_conguaglio(s.esercizio, d, 2, "2027-05-31", 1).unwrap();
    assert_eq!((deb, cred), (3, 0));
    let st = s.db.stato_conguaglio(s.esercizio).unwrap().unwrap();
    assert_eq!(st.totale_debito, 120_000 - 100_001);
    assert_eq!(s.db.totale_piano(st.piano_id.unwrap()).unwrap(), 19_999);
    assert!(s.db.verifica_movimenti(s.cond).unwrap().is_empty(), "movimenti incoerenti con i documenti");

    // dopo l'emissione: da_versare = consuntivo - versato = rate di conguaglio
    let quote = s.db.quote_consuntivo(s.esercizio).unwrap();
    assert_eq!(quote.iter().map(|q| q.da_versare()).sum::<Cents>(), 19_999);
    assert_eq!(quote.iter().map(|q| q.conguaglio()).sum::<Cents>(), 19_999, "il piano di conguaglio non conta tra le rate emesse");

    // con incassi sul conguaglio non si può annullare
    let pers = s.persone[2];
    let rate = s.db.rate_aperte(s.cond, pers).unwrap();
    s.db.registra_incasso(&incasso(&s, pers, rate[0].residuo())).unwrap();
    assert!(s.db.annulla_conguaglio(s.esercizio).is_err());
    let _ = p;
}

#[test]
fn consuntivo_senza_tabella_non_si_riparte() {
    let s = scenario();
    let f = fornitore(&s);
    let v2 = s.db.salva_voce(&VoceSpesa { esercizio_id: s.esercizio, codice: "9.99".into(), descrizione: "Varie".into(), importo_cents: 1000, tabella_id: None, ..Default::default() }).unwrap();
    s.db.registra_fattura(&NuovaFattura { condominio_id: s.cond, esercizio_id: s.esercizio, fornitore_id: f, voce_id: Some(v2), numero: "7".into(), data: "2026-03-02".into(), descrizione: "x".into(), imponibile: 500, iva: 0 }).unwrap();
    let e = s.db.quote_consuntivo(s.esercizio).unwrap_err().to_string();
    assert!(e.contains("tabella"), "{e}");
}

fn rata_verdi(s: &Scenario) -> (i64, Cents) {
    let r = s.db.rate_piano(piano_attivo(s, 1)).unwrap();
    let r = r.iter().find(|r| r.persona_id == s.persone[2]).unwrap();
    (r.id, r.importo)
}

fn data(g: &str) -> chrono::NaiveDate {
    parse_data(g).unwrap()
}

#[test]
fn interessi_di_mora_esatti_sul_tasso_legale_dell_anno() {
    let s = scenario();
    let (rata, imp) = rata_verdi(&s);
    // scadenza 31/01/2026, oggi 31/07/2026 = 181 giorni all'1,6%
    let atteso = (imp as f64 * 0.016 * 181.0 / 365.0).round() as Cents;
    assert_eq!(s.db.interessi_rata(rata, data("2026-07-31")).unwrap(), atteso);
    // prima della scadenza nessun interesse
    assert_eq!(s.db.interessi_rata(rata, data("2026-01-31")).unwrap(), 0);
    // con 10 giorni di tolleranza
    s.db.imposta("mora_giorni_tolleranza", "10").unwrap();
    let atteso = (imp as f64 * 0.016 * 171.0 / 365.0).round() as Cents;
    assert_eq!(s.db.interessi_rata(rata, data("2026-07-31")).unwrap(), atteso);
}

#[test]
fn interessi_a_cavallo_d_anno_usano_il_tasso_di_ciascun_anno() {
    let s = scenario();
    let (rata, imp) = rata_verdi(&s);
    s.db.conn.execute("UPDATE rata SET scadenza='2025-12-01' WHERE id=?1", [rata]).unwrap();
    // 30 giorni nel 2025 (2%) + 1 giorno nel 2026 (1,6%)
    let atteso = (imp as f64 * (0.02 * 30.0 / 365.0 + 0.016 * 1.0 / 365.0)).round() as Cents;
    assert_eq!(s.db.interessi_rata(rata, data("2026-01-01")).unwrap(), atteso);
    // modificando il tasso 2026 il calcolo cambia
    s.db.imposta_tasso_legale(2026, 30_000).unwrap();
    let atteso = (imp as f64 * (0.02 * 30.0 / 365.0 + 0.03 * 1.0 / 365.0)).round() as Cents;
    assert_eq!(s.db.interessi_rata(rata, data("2026-01-01")).unwrap(), atteso);
}

#[test]
fn interessi_con_pagamento_parziale_e_storno() {
    let s = scenario();
    let (rata, imp) = rata_verdi(&s);
    let meta = imp / 2;
    let mut n = incasso(&s, s.persone[2], meta);
    n.data = "2026-03-02".into();
    let inc = s.db.registra_incasso(&n).unwrap();
    let giorni1 = (data("2026-03-02") - data("2026-01-31")).num_days() as f64;
    let giorni2 = (data("2026-07-31") - data("2026-03-02")).num_days() as f64;
    let atteso = (imp as f64 * 0.016 * giorni1 / 365.0 + (imp - meta) as f64 * 0.016 * giorni2 / 365.0).round() as Cents;
    assert_eq!(s.db.interessi_rata(rata, data("2026-07-31")).unwrap(), atteso);
    // storno: torna come se non fosse mai stato pagato
    s.db.storna_incasso(inc, "2026-03-05").unwrap();
    let intero = (imp as f64 * 0.016 * 181.0 / 365.0).round() as Cents;
    assert_eq!(s.db.interessi_rata(rata, data("2026-07-31")).unwrap(), intero);
}

#[test]
fn solleciti_raggruppati_per_condomino_con_testo() {
    let s = scenario();
    piano_attivo(&s, 2);
    let oggi = data("2026-06-15");
    let v = s.db.solleciti_da_inviare(s.cond, oggi, 0, 0).unwrap();
    assert_eq!(v.len(), 3, "tre condòmini morosi");
    let rossi = v.iter().find(|x| x.persona.starts_with("Rossi")).unwrap();
    assert_eq!(rossi.rate.len(), 2);
    assert!(rossi.interessi > 0 && rossi.totale() == rossi.capitale + rossi.interessi);
    // soglia minima: nessuno supera 10.000 €
    assert!(s.db.solleciti_da_inviare(s.cond, oggi, 0, 1_000_000).unwrap().is_empty());
    let (ogg, corpo) = s.db.testo_sollecito(s.cond, rossi, oggi, 15).unwrap();
    assert!(ogg.contains("Condominio Aurora"));
    assert!(corpo.contains("art. 63 disp. att. c.c.") && corpo.contains("1,6%"), "{corpo}");
    // accodamento e-mail e storico
    s.db.accoda_email(&rossi.email, &ogg, &corpo).unwrap();
    assert_eq!(s.db.email_in_coda().unwrap(), 1);
    s.db.registra_sollecito(s.cond, rossi, "email", &corpo).unwrap();
    assert_eq!(s.db.storico_solleciti(s.cond).unwrap().len(), 1);
    // atto interruttivo: la prescrizione riparte
    s.db.interrompi_prescrizione_sollecito(rossi, "2026-06-16").unwrap();
    assert!(s.db.accoda_email("senza-chiocciola", "x", "y").is_err());
}

#[test]
fn csv_estratto_conto_formati_diversi() {
    use condominio::banca::parse_estratto_csv;
    // preambolo, punto e virgola, importi italiani, descrizione tra virgolette con ';' dentro
    let t = "Conto: IT00;;\nPeriodo: gennaio\n\nData operazione;Data valuta;Descrizione;Importo\n05/01/2026;05/01/2026;\"BONIFICO DA ROSSI; rata\";1.234,56\n07/01/2026;07/01/2026;Canone banca;-12,50\nxx;1;1;1\n";
    let (v, scart) = parse_estratto_csv(t).unwrap();
    assert_eq!(v.len(), 2);
    assert_eq!(v[0].data, "2026-01-05");
    assert_eq!(v[0].importo, 123_456);
    assert!(v[0].descrizione.contains("ROSSI; rata"));
    assert_eq!(v[1].importo, -1250);
    assert_eq!(scart.len(), 1);
    // colonne entrate/uscite separate, virgola come separatore e date ISO
    let t = "data,causale,entrate,uscite\n2026-02-01,Bonifico,100.00,\n2026-02-02,Spese,,25.50\n";
    let (v, _) = parse_estratto_csv(t).unwrap();
    assert_eq!(v.iter().map(|x| x.importo).collect::<Vec<_>>(), vec![10_000, -2_550]);
    // senza intestazione: data;descrizione;importo
    let (v, _) = parse_estratto_csv("01/03/2026;Prova;5,00\n").unwrap();
    assert_eq!(v[0].importo, 500);
    assert!(parse_estratto_csv("   \n").is_err());
}

#[test]
fn riconciliazione_bancaria_incassi_fatture_e_duplicati() {
    let s = scenario();
    let (_rata, imp) = rata_verdi(&s);
    let forn = fornitore(&s);
    let fid = fattura(&s, forn, "FT-12", 90_000);
    let csv = format!(
        "Data;Descrizione;Importo\n10/02/2026;BONIFICO DA VERDI X RATA;{}\n12/02/2026;Pagamento Pulizie fattura FT-12;-900,00\n13/02/2026;Imposta di bollo;-2,00\n",
        format!("{},{:02}", imp / 100, imp % 100)
    );
    let e = s.db.importa_estratto(s.cond, s.cc, &csv).unwrap();
    assert_eq!((e.nuovi, e.duplicati), (3, 0));
    // reimportare lo stesso file non crea doppioni
    let e = s.db.importa_estratto(s.cond, s.cc, &csv).unwrap();
    assert_eq!((e.nuovi, e.duplicati), (0, 3));

    let mov = s.db.elenco_movimenti_banca(s.cond, Some("da_abbinare")).unwrap();
    let entrata = mov.iter().find(|m| m.importo > 0).unwrap();
    let uscita = mov.iter().find(|m| m.importo == -90_000).unwrap();

    let p = s.db.suggerisci_abbinamenti(entrata.id).unwrap();
    assert_eq!(p[0].tipo, "incasso");
    assert!(p[0].etichetta.contains("Verdi"));
    let p2 = s.db.suggerisci_abbinamenti(uscita.id).unwrap();
    assert_eq!(p2[0].tipo, "fattura");
    assert_eq!(p2[0].riferimento_id, fid);

    // il bonifico copre esattamente la rata aperta di Verdi
    assert_eq!(s.db.rate_aperte(s.cond, s.persone[2]).unwrap().len(), 1);
    let inc = s.db.riconcilia_incasso(entrata.id, p[0].riferimento_id).unwrap();
    assert!(s.db.rate_aperte(s.cond, s.persone[2]).unwrap().is_empty());
    s.db.riconcilia_fattura(uscita.id, fid).unwrap();
    assert!(s.db.riconcilia_fattura(uscita.id, fid).is_err(), "già riconciliato");
    let f = s.db.elenco_fatture(s.cond, None).unwrap();
    assert_eq!(f[0].stato, "pagata");
    assert!(s.db.verifica_movimenti(s.cond).unwrap().is_empty(), "movimenti incoerenti con i documenti");

    // annullo: tutto torna com'era
    s.db.annulla_riconciliazione(entrata.id, "2026-02-20").unwrap();
    s.db.annulla_riconciliazione(uscita.id, "2026-02-20").unwrap();
    assert_eq!(s.db.rate_aperte(s.cond, s.persone[2]).unwrap().len(), 1, "la rata torna aperta");
    assert_eq!(s.db.elenco_fatture(s.cond, None).unwrap()[0].stato, "da_pagare");
    let _ = inc;
    let ign = mov.iter().find(|m| m.importo == -200).unwrap();
    s.db.ignora_movimento(ign.id).unwrap();
    assert_eq!(s.db.elenco_movimenti_banca(s.cond, Some("ignorato")).unwrap().len(), 1);
}

#[test]
fn scadenzario_riunisce_rate_fatture_assemblee_e_agenda() {
    let s = scenario();
    piano_attivo(&s, 2);
    let forn = fornitore(&s);
    fattura(&s, forn, "A1", 50_000);
    s.db.aggiungi_agenda(s.cond, "15/02/2026", "Chiamare l'ascensorista", "").unwrap();
    assert!(s.db.aggiungi_agenda(s.cond, "31/31/2026", "x", "").is_err());
    let v = s.db.scadenzario(s.cond, data("2026-02-01"), data("2026-12-31")).unwrap();
    let tipi: std::collections::HashSet<&str> = v.iter().map(|x| x.tipo).collect();
    for t in ["rata", "fattura", "agenda", "fiscale"] {
        assert!(tipi.contains(t), "manca {t}: {tipi:?}");
    }
    assert!(v.windows(2).all(|w| w[0].data <= w[1].data), "ordinato per data");
    // la prima rata (31/01) è scaduta, la seconda no
    assert!(v.iter().any(|x| x.tipo == "rata" && x.scaduta));
    assert!(v.iter().any(|x| x.tipo == "rata" && !x.scaduta));
    let id = v.iter().find(|x| x.tipo == "agenda").unwrap().agenda_id.unwrap();
    s.db.completa_agenda(id, true).unwrap();
    let v = s.db.scadenzario(s.cond, data("2026-02-01"), data("2026-12-31")).unwrap();
    assert!(!v.iter().any(|x| x.tipo == "agenda"));
}

use condominio::assemblee::*;
use condominio::pratiche::*;

fn assemblea_bozza(s: &Scenario) -> i64 {
    s.db.salva_assemblea(&Assemblea {
        condominio_id: s.cond,
        esercizio_id: Some(s.esercizio),
        tipo: "ordinaria".into(),
        modalita: "presenza".into(),
        luogo: "Sala condominiale".into(),
        data_prima: "10/11/2026".into(),
        ora_prima: "08:00".into(),
        data_seconda: Some("12/11/2026".into()),
        ora_seconda: "18:30".into(),
        tabella_id: Some(s.tabella),
        ..Default::default()
    })
    .unwrap()
}

fn punto(s: &Scenario, ass: i64, titolo: &str, magg: &str, tipo: &str) -> i64 {
    s.db.salva_punto(&Punto { assemblea_id: ass, titolo: titolo.into(), maggioranza: magg.into(), tipo_delibera: tipo.into(), ..Default::default() }).unwrap()
}

#[test]
fn assemblea_date_e_convocazione() {
    let s = scenario();
    // seconda convocazione oltre i 10 giorni, o lo stesso giorno: rifiutata
    let mut a = Assemblea { condominio_id: s.cond, tipo: "ordinaria".into(), modalita: "presenza".into(), data_prima: "10/11/2026".into(), data_seconda: Some("21/11/2026".into()), ..Default::default() };
    assert!(s.db.salva_assemblea(&a).unwrap_err().to_string().contains("dieci giorni"));
    a.data_seconda = Some("10/11/2026".into());
    assert!(s.db.salva_assemblea(&a).is_err());

    let id = assemblea_bozza(&s);
    // senza ordine del giorno non si convoca
    assert!(s.db.convoca_assemblea(id, data("2026-10-20")).unwrap_err().to_string().contains("ordine del giorno"));
    punto(&s, id, "Approvazione rendiconto 2025", "ordinaria", "rendiconto");
    // avviso in ritardo (meno di 5 giorni)
    assert!(s.db.convoca_assemblea(id, data("2026-11-07")).unwrap_err().to_string().contains("5 giorni"));
    let com = s.db.convoca_assemblea(id, data("2026-10-20")).unwrap();
    let c = s.db.elenco_comunicazioni(s.cond).unwrap();
    let c = c.iter().find(|x| x.id == com).unwrap();
    assert_eq!(c.tipo, "assemblea");
    assert!(c.testo.contains("ORDINE DEL GIORNO") && c.testo.contains("Approvazione rendiconto 2025") && c.testo.contains("seconda convocazione"));
    // l'odg non è più modificabile solo dopo lo svolgimento: ora è "convocata"
    assert_eq!(s.db.assemblea(id).unwrap().stato, "convocata");
    assert!(s.db.convoca_assemblea(id, data("2026-10-20")).is_err());
}

#[test]
fn quorum_costitutivo_prima_e_seconda_convocazione() {
    let s = scenario();
    let id = assemblea_bozza(&s);
    punto(&s, id, "Punto", "ordinaria", "altro");
    // presenze solo da convocata
    assert!(s.db.imposta_presenza(id, s.unita[0], "").is_err());
    s.db.convoca_assemblea(id, data("2026-10-20")).unwrap();

    s.db.imposta_presenza(id, s.unita[0], "").unwrap(); // 400‰ (Rossi)
    s.db.imposta_presenza(id, s.unita[2], "Mario Neri").unwrap(); // 250‰ (Verdi, delegato)
    let q1 = s.db.quorum_costitutivo(id, 1).unwrap();
    assert_eq!((q1.teste_presenti, q1.teste_totali), (2, 3));
    assert_eq!(q1.millesimi_presenti, parse_millesimi("650").unwrap());
    assert!(!q1.costituita, "650‰ < 2/3: la prima convocazione non è valida");
    assert!(s.db.apri_assemblea(id, 1, "Rossi", "Verdi").unwrap_err().to_string().contains("non regolarmente costituita"));
    let q2 = s.db.quorum_costitutivo(id, 2).unwrap();
    assert!(q2.costituita);

    // aggiungendo l'unità 2 (350‰) si supera il 2/3 e la maggioranza dei partecipanti
    s.db.imposta_presenza(id, s.unita[1], "").unwrap();
    assert!(s.db.quorum_costitutivo(id, 1).unwrap().costituita);
    // un'unità di un altro condominio non è accettata
    let altro = s.db.salva_condominio(&Condominio { nome: "Altro".into(), ..Default::default() }).unwrap();
    let ua = s.db.salva_unita(&Unita { condominio_id: altro, interno: "1".into(), tipo: "appartamento".into(), ..Default::default() }).unwrap();
    assert!(s.db.imposta_presenza(id, ua, "").is_err());
    // serve presidente e segretario
    assert!(s.db.apri_assemblea(id, 1, "", "").is_err());
    s.db.apri_assemblea(id, 1, "Rossi", "Verdi").unwrap();
    assert_eq!(s.db.assemblea(id).unwrap().stato, "tenuta");
}

#[test]
fn votazioni_maggioranze_delibere_e_verbale() {
    let s = scenario();
    let id = assemblea_bozza(&s);
    let p_ord = punto(&s, id, "Approvazione rendiconto", "ordinaria", "rendiconto");
    let p_qua = punto(&s, id, "Nomina amministratore", "qualificata", "altro");
    let p_inn = punto(&s, id, "Installazione ascensore", "innovazioni", "straordinaria");
    s.db.convoca_assemblea(id, data("2026-10-20")).unwrap();
    // in seconda convocazione: presenti solo Rossi (400‰) e Verdi (250‰)
    s.db.imposta_presenza(id, s.unita[0], "").unwrap();
    s.db.imposta_presenza(id, s.unita[2], "").unwrap();
    // senza assemblea aperta non si vota
    assert!(s.db.registra_voto(p_ord, s.unita[0], "favorevole").is_err());
    s.db.apri_assemblea(id, 2, "Rossi", "Verdi").unwrap();
    // voto di chi non è presente: rifiutato
    assert!(s.db.registra_voto(p_ord, s.unita[1], "favorevole").is_err());

    // ordinaria in 2ª conv.: basta 1/3 del valore (650‰ presenti), maggioranza degli intervenuti
    s.db.registra_voto(p_ord, s.unita[0], "favorevole").unwrap();
    s.db.registra_voto(p_ord, s.unita[2], "favorevole").unwrap();
    let e = s.db.esito_punto(p_ord).unwrap();
    assert!(e.approvato, "{e:?}");
    assert_eq!((e.fav_teste, e.presenti_teste), (2, 2));

    // qualificata (art. 1136 c.4): anche in 2ª conv. servono 500‰ favorevoli -> 400‰ non bastano
    s.db.registra_voto(p_qua, s.unita[0], "favorevole").unwrap();
    s.db.registra_voto(p_qua, s.unita[2], "contrario").unwrap();
    let e = s.db.esito_punto(p_qua).unwrap();
    assert!(!e.approvato);
    assert!(s.db.approva_punto(p_qua).unwrap_err().to_string().contains("non raggiunge"));
    // cambio voto: Verdi diventa favorevole -> 650‰ e maggioranza dei presenti
    s.db.registra_voto(p_qua, s.unita[2], "favorevole").unwrap();
    assert!(s.db.esito_punto(p_qua).unwrap().approvato);

    // innovazioni: 2/3 di 1000 = 666,67‰; 650‰ non bastano
    s.db.voto_a_tutti_i_presenti(p_inn, "favorevole").unwrap();
    assert!(!s.db.esito_punto(p_inn).unwrap().approvato);

    // astenuti contano come intervenuti: 1 favorevole + 1 astenuto su 2 presenti non è maggioranza
    s.db.registra_voto(p_ord, s.unita[2], "astenuto").unwrap();
    assert!(!s.db.esito_punto(p_ord).unwrap().approvato);
    s.db.registra_voto(p_ord, s.unita[2], "favorevole").unwrap();

    // la delibera nasce dal punto approvato, con riferimento al verbale, ed è utilizzabile per i piani
    let d = s.db.approva_punto(p_ord).unwrap();
    let delibere = s.db.elenco_delibere(s.cond).unwrap();
    let dd = delibere.iter().find(|x| x.id == d).unwrap();
    assert_eq!(dd.tipo, "rendiconto");
    assert!(dd.riferimento.contains("12/11/2026") && dd.riferimento.contains("punto 1"), "{}", dd.riferimento);
    assert!(s.db.approva_punto(p_ord).is_err(), "una sola delibera per punto");
    assert!(s.db.registra_voto(p_ord, s.unita[0], "contrario").is_err(), "voto bloccato dopo la delibera");

    let v = s.db.verbale_testo(id).unwrap();
    assert!(v.contains("seconda") || v.contains("2ª convocazione"), "{v}");
    assert!(v.contains("APPROVATO") && v.contains("NON APPROVATO") && v.contains("Presidente: Rossi"), "{v}");
    s.db.verbalizza_assemblea(id).unwrap();
    assert!(s.db.rimuovi_presenza(id, s.unita[0]).is_err());
    assert!(s.db.elimina_assemblea(id).is_err());
}

#[test]
fn pratiche_intervento_da_segnalazione_sinistro_e_legale() {
    let s = scenario();
    let forn = fornitore(&s);
    let t = s.db.crea_ticket(s.cond, Some(s.unita[0]), Some(s.persone[0]), "Perdita acqua", "Dal soffitto", 3).unwrap();
    let p = s.db.intervento_da_ticket(t, Some(forn)).unwrap();
    assert!(s.db.intervento_da_ticket(t, Some(forn)).is_err());
    let pr = s.db.elenco_pratiche(s.cond, Some("intervento"), false).unwrap();
    assert_eq!((pr[0].stato.as_str(), pr[0].ticket_id), ("assegnato", Some(t)));
    let tk = s.db.elenco_ticket(s.cond, Some("in_carico"), false).unwrap();
    assert_eq!(tk.len(), 1);
    assert!(s.db.cambia_stato_pratica(p, "liquidato").is_err(), "stato di un altro tipo");
    s.db.cambia_stato_pratica(p, "in_corso").unwrap();
    s.db.cambia_stato_pratica(p, "chiuso").unwrap();
    let pr = s.db.elenco_pratiche(s.cond, None, true).unwrap();
    assert!(pr.is_empty(), "chiusa: non più tra le aperte");
    assert!(s.db.elenco_pratiche(s.cond, None, false).unwrap()[0].data_chiusura.is_some());
    assert!(s.db.eventi_pratica(p).unwrap().len() >= 4);

    let sin = s.db.salva_pratica(&Pratica { condominio_id: s.cond, tipo: "sinistro".into(), titolo: "Danno da infiltrazione".into(), riferimento: "Sin. 2026/889".into(), importo: 350_000, ..Default::default() }).unwrap();
    s.db.cambia_stato_pratica(sin, "denunciato").unwrap();
    s.db.cambia_stato_pratica(sin, "liquidato").unwrap();
    assert!(s.db.elenco_pratiche(s.cond, Some("sinistro"), true).unwrap().len() == 1, "liquidato non è ancora chiuso");
    let leg = s.db.salva_pratica(&Pratica { condominio_id: s.cond, tipo: "legale".into(), titolo: "Decreto ingiuntivo Verdi".into(), controparte: "Verdi".into(), ..Default::default() }).unwrap();
    s.db.cambia_stato_pratica(leg, "archiviata").unwrap();
    assert!(s.db.salva_pratica(&Pratica { condominio_id: s.cond, tipo: "boh".into(), titolo: "x".into(), ..Default::default() }).is_err());
    s.db.aggiungi_evento_pratica(leg, "Udienza fissata").unwrap();
}

#[test]
fn fiscale_riepilogo_cu_e_770() {
    let s = scenario();
    let forn = s.db.salva_fornitore(&Fornitore { condominio_id: s.cond, ragione_sociale: "Edil Rossi Srl".into(), cf_piva: "01234567890".into(), regime_ritenuta: "appalti4".into(), ..Default::default() }).unwrap();
    let f1 = s.db.registra_fattura(&NuovaFattura { condominio_id: s.cond, esercizio_id: s.esercizio, fornitore_id: forn, voce_id: Some(s.voce), numero: "10".into(), data: "2026-02-01".into(), descrizione: "lavori".into(), imponibile: 100_000, iva: 22_000 }).unwrap();
    let f2 = s.db.registra_fattura(&NuovaFattura { condominio_id: s.cond, esercizio_id: s.esercizio, fornitore_id: forn, voce_id: Some(s.voce), numero: "11".into(), data: "2026-04-01".into(), descrizione: "lavori 2".into(), imponibile: 50_000, iva: 11_000 }).unwrap();
    s.db.paga_fattura(f1, "2026-03-05", s.cc).unwrap();
    s.db.paga_fattura(f2, "2026-05-10", s.cc).unwrap();

    let cu = s.db.riepilogo_cu(s.cond, 2026).unwrap();
    assert_eq!(cu.len(), 1);
    assert_eq!((cu[0].imponibile, cu[0].ritenuta_operata, cu[0].ritenuta_versata, cu[0].n_fatture), (150_000, 6_000, 0, 2));
    assert!(s.db.riepilogo_cu(s.cond, 2025).unwrap().is_empty(), "criterio di cassa: conta la data di pagamento");

    // 40 € + 20 € di ritenute: sotto soglia, F24 semestrale al 30/06
    let g = s.db.ritenute_da_versare(s.cond).unwrap();
    assert_eq!(g.len(), 1);
    assert_eq!((g[0].codice_tributo.as_str(), g[0].importo, g[0].scadenza.as_str()), ("1019", 6_000, "2026-06-30"));
    s.db.versa_ritenute(s.cond, &g[0], "2026-06-30", s.cc).unwrap();
    let cu = s.db.riepilogo_cu(s.cond, 2026).unwrap();
    assert_eq!(cu[0].ritenuta_versata, 6_000);

    let st = s.db.riepilogo_770(s.cond, 2026).unwrap();
    let op: Cents = st.iter().map(|r| r.operate).sum();
    let ve: Cents = st.iter().map(|r| r.versate).sum();
    assert_eq!((op, ve), (6_000, 6_000));
    assert!(st.iter().any(|r| r.mese == 3 && r.operate == 4_000));
    assert!(st.iter().any(|r| r.mese == 6 && r.versate == 6_000 && r.codice_tributo == "1019"));
    let csv = s.db.csv_cu(s.cond, 2026).unwrap();
    assert!(csv.contains("Edil Rossi Srl;01234567890;appalti4;1019;2;1.500,00 €"), "{csv}");
}
