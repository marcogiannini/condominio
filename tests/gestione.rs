//! Modello "gestione" (movimenti di cassa), migrazione dalla partita doppia e riparti su più tabelle.

use condominio::anagrafica::*;
use condominio::fornitori::*;
use condominio::incassi::NuovoIncasso;
use condominio::preventivo::*;
use condominio::util::*;
use condominio::Db;

struct S {
    db: Db,
    cond: i64,
    es: i64,
    cc: i64,
    unita: Vec<i64>,
    persone: Vec<i64>,
    tab_a: i64,
    voce: i64,
    delibera: i64,
}

fn scenario() -> S {
    let db = Db::in_memoria().unwrap();
    let cond = db.salva_condominio(&Condominio { nome: "Aurora".into(), ..Default::default() }).unwrap();
    let cc = db.crea_conto_corrente(cond, "Principale", "").unwrap();
    let mut persone = vec![];
    let mut unita = vec![];
    for (i, cogn) in ["Rossi", "Bianchi", "Verdi"].iter().enumerate() {
        let p = db.salva_persona(&Persona { tipo: "fisica".into(), nome: "X".into(), cognome: cogn.to_string(), ..Default::default() }).unwrap();
        let u = db.salva_unita(&Unita { condominio_id: cond, scala: "A".into(), interno: (i + 1).to_string(), tipo: "appartamento".into(), ..Default::default() }).unwrap();
        db.aggiungi_titolarita(u, p, "proprietario", 1_000_000, false).unwrap();
        persone.push(p);
        unita.push(u);
    }
    let tab_a = db.salva_tabella(&Tabella { condominio_id: cond, codice: "A".into(), nome: "Generale".into(), totale_micro: 1_000_000, ..Default::default() }).unwrap();
    for (u, m) in unita.iter().zip(["400", "350", "250"]) {
        db.imposta_millesimo(tab_a, *u, parse_millesimi(m).unwrap()).unwrap();
    }
    let es = db.salva_esercizio(&Esercizio { condominio_id: cond, nome: "2026".into(), dal: "01/01/2026".into(), al: "31/12/2026".into(), ..Default::default() }).unwrap();
    let voce = db.salva_voce(&VoceSpesa { esercizio_id: es, codice: "1".into(), descrizione: "Ascensore".into(), importo_cents: 100_001, tabella_id: Some(tab_a), ..Default::default() }).unwrap();
    let delibera = db
        .salva_delibera(&Delibera { condominio_id: cond, esercizio_id: Some(es), data: "15/01/2026".into(), tipo: "preventivo".into(), oggetto: "Preventivo".into(), riferimento: "Verbale".into(), ..Default::default() })
        .unwrap();
    S { db, cond, es, cc, unita, persone, tab_a, voce, delibera }
}

fn tabella_b(s: &S) -> i64 {
    let t = s.db.salva_tabella(&Tabella { condominio_id: s.cond, codice: "B".into(), nome: "Scala".into(), totale_micro: 1_000_000, ..Default::default() }).unwrap();
    s.db.imposta_millesimo(t, s.unita[0], parse_millesimi("500").unwrap()).unwrap();
    s.db.imposta_millesimo(t, s.unita[1], parse_millesimi("500").unwrap()).unwrap();
    t
}

fn fattura(s: &S, forn: i64, num: &str, imponibile: Cents) -> i64 {
    s.db.registra_fattura(&NuovaFattura {
        condominio_id: s.cond,
        esercizio_id: s.es,
        fornitore_id: forn,
        voce_id: Some(s.voce),
        numero: num.into(),
        data: "2026-03-01".into(),
        descrizione: "x".into(),
        imponibile,
        iva: 0,
    })
    .unwrap()
}

fn quota(r: &[(i64, Cents)], u: i64) -> Cents {
    r.iter().find(|x| x.0 == u).map(|x| x.1).unwrap_or(0)
}

// ------------------------------------------------------------------ riparti

#[test]
fn voce_su_due_tabelle_somma_esattamente() {
    let s = scenario();
    let b = tabella_b(&s);
    // senza suddivisione: solo la tabella semplice
    assert_eq!(s.db.tabelle_voce(s.voce).unwrap().len(), 1);
    s.db.imposta_riparto_voce(s.voce, &[(s.tab_a, 500_000), (b, 500_000)]).unwrap();
    let r = s.db.riparto_voce(s.voce, 100_000).unwrap();
    assert_eq!(r.iter().map(|x| x.1).sum::<Cents>(), 100_000);
    assert_eq!(quota(&r, s.unita[0]), 20_000 + 25_000);
    assert_eq!(quota(&r, s.unita[1]), 17_500 + 25_000);
    assert_eq!(quota(&r, s.unita[2]), 12_500);
    // le quote devono fare 100%
    assert!(s.db.imposta_riparto_voce(s.voce, &[(s.tab_a, 600_000), (b, 500_000)]).is_err());
    // tornando a nessuna suddivisione vale di nuovo la tabella semplice
    s.db.imposta_riparto_voce(s.voce, &[]).unwrap();
    assert_eq!(quota(&s.db.riparto_voce(s.voce, 100_000).unwrap(), s.unita[0]), 40_000);
}

#[test]
fn esclusione_e_riduzione_di_unita() {
    let s = scenario();
    let b = tabella_b(&s);
    s.db.imposta_riparto_voce(s.voce, &[(s.tab_a, 500_000), (b, 500_000)]).unwrap();
    // l'unità 1 non usa il servizio: esclusa al 100%
    s.db.imposta_riduzione(s.voce, s.unita[0], 1_000_000, "non usa l'ascensore").unwrap();
    let r = s.db.riparto_voce(s.voce, 100_000).unwrap();
    assert_eq!(quota(&r, s.unita[0]), 0);
    assert_eq!(r.iter().map(|x| x.1).sum::<Cents>(), 100_000, "la quota esclusa è ripartita sugli altri");
    // riduzione al 50%: paga meno di prima ma non zero
    s.db.imposta_riduzione(s.voce, s.unita[0], 500_000, "").unwrap();
    let r2 = s.db.riparto_voce(s.voce, 100_000).unwrap();
    assert!(quota(&r2, s.unita[0]) > 0 && quota(&r2, s.unita[0]) < 45_000);
    assert_eq!(r2.iter().map(|x| x.1).sum::<Cents>(), 100_000);
    assert_eq!(s.db.elenco_riduzioni(s.voce).unwrap().len(), 1);
    s.db.rimuovi_riduzione(s.voce, s.unita[0]).unwrap();
    assert!(s.db.elenco_riduzioni(s.voce).unwrap().is_empty());
    // un'unità inesistente per il condominio è rifiutata
    assert!(s.db.imposta_riduzione(s.voce, 9999, 100_000, "").is_err());
}

#[test]
fn il_piano_rate_usa_il_riparto_multi_tabella_e_poi_lo_blocca() {
    let s = scenario();
    let b = tabella_b(&s);
    s.db.imposta_riparto_voce(s.voce, &[(s.tab_a, 500_000), (b, 500_000)]).unwrap();
    s.db.imposta_riduzione(s.voce, s.unita[2], 1_000_000, "").unwrap();
    let p = s
        .db
        .crea_piano(&Piano { esercizio_id: s.es, nome: "Ord".into(), tipo: "ordinario".into(), delibera_id: Some(s.delibera), n_rate: 1, prima_scadenza: "31/01/2026".into(), cadenza_mesi: 1, ..Default::default() })
        .unwrap();
    s.db.imposta_voce_piano(p, s.voce, 100_001).unwrap();
    s.db.attiva_piano(p).unwrap();
    let rate = s.db.rate_piano(p).unwrap();
    assert_eq!(rate.iter().map(|r| r.importo).sum::<Cents>(), 100_001);
    assert!(rate.iter().all(|r| r.persona_id != s.persone[2]), "l'unità esclusa non riceve rate");
    // voce ancorata a un piano attivo: riparto non modificabile
    assert!(s.db.imposta_riparto_voce(s.voce, &[]).is_err());
    assert!(s.db.imposta_riduzione(s.voce, s.unita[0], 100_000, "").is_err());
    assert!(s.db.rimuovi_riduzione(s.voce, s.unita[2]).is_err());
}

#[test]
fn il_consuntivo_usa_lo_stesso_riparto() {
    let s = scenario();
    let b = tabella_b(&s);
    s.db.imposta_riparto_voce(s.voce, &[(s.tab_a, 500_000), (b, 500_000)]).unwrap();
    let forn = s.db.salva_fornitore(&Fornitore { condominio_id: s.cond, ragione_sociale: "Otis".into(), regime_ritenuta: "nessuna".into(), ..Default::default() }).unwrap();
    fattura(&s, forn, "1", 100_000);
    let q = s.db.quote_consuntivo(s.es).unwrap();
    assert_eq!(q.iter().map(|x| x.consuntivo).sum::<Cents>(), 100_000);
    let rossi = q.iter().find(|x| x.persona_id == s.persone[0]).unwrap();
    assert_eq!(rossi.consuntivo, 45_000);
}

// ---------------------------------------------------------------- movimenti

#[test]
fn movimento_manuale_entra_nel_consuntivo_e_si_storna() {
    let s = scenario();
    let forn = s.db.salva_fornitore(&Fornitore { condominio_id: s.cond, ragione_sociale: "Otis".into(), regime_ritenuta: "nessuna".into(), ..Default::default() }).unwrap();
    fattura(&s, forn, "1", 100_000);
    assert_eq!(s.db.consuntivo_voce(s.voce).unwrap(), 100_000);
    let m = s.db.registra_movimento_manuale(s.cond, s.cc, "2026-04-01", "Spese tenuta conto", -500, Some(s.voce)).unwrap();
    assert_eq!(s.db.consuntivo_voce(s.voce).unwrap(), 100_500);
    assert_eq!(s.db.saldo_conto_corrente(s.cc).unwrap(), -500);
    s.db.storna_movimento(m, "2026-04-02", "errore").unwrap();
    assert_eq!(s.db.consuntivo_voce(s.voce).unwrap(), 100_000);
    assert_eq!(s.db.saldo_conto_corrente(s.cc).unwrap(), 0, "lo storno annulla l'effetto sul saldo");
    // nulla si cancella: movimento originale + storno
    assert_eq!(s.db.elenco_movimenti(s.cond, None, None, None).unwrap().len(), 2);
    assert!(s.db.storna_movimento(m, "2026-04-03", "bis").is_err(), "già stornato");
    // movimento a importo zero e data errata rifiutati
    assert!(s.db.registra_movimento_manuale(s.cond, s.cc, "2026-04-01", "x", 0, None).is_err());
    assert!(s.db.registra_movimento_manuale(s.cond, s.cc, "ieri", "x", 100, None).is_err());
    // riepilogo: lo storno non conta come uscita
    assert_eq!(s.db.riepilogo_cassa(s.cond, None, None).unwrap(), (0, 0));
}

#[test]
fn giroconto_si_storna_su_entrambi_i_conti() {
    let s = scenario();
    let cc2 = s.db.crea_conto_corrente(s.cond, "Fondo lavori", "").unwrap();
    s.db.registra_movimento_manuale(s.cond, s.cc, "2026-01-02", "Saldo iniziale", 50_000, None).unwrap();
    let g = s.db.giroconto(s.cond, s.cc, cc2, 20_000, "2026-02-01", "accantonamento").unwrap();
    assert_eq!(s.db.saldo_conto_corrente(s.cc).unwrap(), 30_000);
    assert_eq!(s.db.saldo_conto_corrente(cc2).unwrap(), 20_000);
    assert!(s.db.giroconto(s.cond, s.cc, s.cc, 1, "2026-02-01", "").is_err());
    s.db.storna_movimento(g, "2026-02-02", "annullo").unwrap();
    assert_eq!(s.db.saldo_conto_corrente(s.cc).unwrap(), 50_000);
    assert_eq!(s.db.saldo_conto_corrente(cc2).unwrap(), 0);
    // il giroconto non è un'entrata/uscita di gestione
    assert_eq!(s.db.riepilogo_cassa(s.cond, None, None).unwrap(), (50_000, 0));
}

#[test]
fn situazione_della_gestione_segue_incassi_fatture_e_ritenute() {
    let s = scenario();
    let p = s
        .db
        .crea_piano(&Piano { esercizio_id: s.es, nome: "Ord".into(), tipo: "ordinario".into(), delibera_id: Some(s.delibera), n_rate: 2, prima_scadenza: "31/01/2026".into(), cadenza_mesi: 1, ..Default::default() })
        .unwrap();
    s.db.imposta_voce_piano(p, s.voce, 100_001).unwrap();
    s.db.attiva_piano(p).unwrap();
    assert_eq!(s.db.situazione_gestione(s.cond).unwrap().crediti_condomini, 100_001);
    s.db.registra_incasso(&NuovoIncasso { condominio_id: s.cond, persona_id: s.persone[0], conto_corrente_id: Some(s.cc), data: "2026-02-10".into(), importo: 10_000, metodo: "bonifico".into(), ..Default::default() }).unwrap();
    let sit = s.db.situazione_gestione(s.cond).unwrap();
    assert_eq!(sit.liquidita, 10_000);
    assert_eq!(sit.crediti_condomini, 90_001);

    // fattura con ritenuta 4%: da pagare -> pagata -> ritenuta da versare -> F24
    let forn = s.db.salva_fornitore(&Fornitore { condominio_id: s.cond, ragione_sociale: "Edil".into(), regime_ritenuta: "appalti4".into(), ..Default::default() }).unwrap();
    let f = fattura(&s, forn, "7", 100_000);
    assert_eq!(s.db.situazione_gestione(s.cond).unwrap().debiti_fornitori, 96_000);
    s.db.registra_movimento_manuale(s.cond, s.cc, "2026-03-01", "Saldo iniziale", 200_000, None).unwrap();
    s.db.paga_fattura(f, "2026-03-10", s.cc).unwrap();
    let sit = s.db.situazione_gestione(s.cond).unwrap();
    assert_eq!((sit.debiti_fornitori, sit.ritenute_da_versare), (0, 4_000));
    assert_eq!(sit.liquidita, 10_000 + 200_000 - 96_000);

    let g = s.db.ritenute_da_versare(s.cond).unwrap();
    let vid = s.db.versa_ritenute(s.cond, &g[0], "2026-04-10", s.cc).unwrap();
    let sit = s.db.situazione_gestione(s.cond).unwrap();
    assert_eq!(sit.ritenute_da_versare, 0);
    assert_eq!(sit.liquidita, 10_000 + 200_000 - 100_000);
    assert!(s.db.verifica_movimenti(s.cond).unwrap().is_empty());

    // storno F24: ritenuta di nuovo da versare, banca ripristinata, pagamento ancora non stornabile finché versata
    s.db.storna_versamento_ritenute(vid, "2026-04-11").unwrap();
    let sit = s.db.situazione_gestione(s.cond).unwrap();
    assert_eq!(sit.ritenute_da_versare, 4_000);
    assert_eq!(sit.liquidita, 10_000 + 200_000 - 96_000);
    assert!(s.db.verifica_movimenti(s.cond).unwrap().is_empty());
    assert_eq!(s.db.ritenute_da_versare(s.cond).unwrap().len(), 1);
    // ora il pagamento è di nuovo stornabile
    s.db.storna_pagamento_fattura(f, "2026-04-12").unwrap();
    assert_eq!(s.db.situazione_gestione(s.cond).unwrap().debiti_fornitori, 96_000);
    assert!(s.db.verifica_movimenti(s.cond).unwrap().is_empty());
}

#[test]
fn eccedenza_nel_salvadanaio_e_compensazione_senza_credito_rifiutata() {
    let s = scenario();
    let p = s
        .db
        .crea_piano(&Piano { esercizio_id: s.es, nome: "Ord".into(), tipo: "ordinario".into(), delibera_id: Some(s.delibera), n_rate: 1, prima_scadenza: "31/01/2026".into(), cadenza_mesi: 1, ..Default::default() })
        .unwrap();
    s.db.imposta_voce_piano(p, s.voce, 100_001).unwrap();
    s.db.attiva_piano(p).unwrap();
    // eccedenza -> Salvadanaio
    let r = s.db.rate_aperte(s.cond, s.persone[0]).unwrap();
    let dovuto: Cents = r.iter().map(|x| x.residuo()).sum();
    s.db.registra_incasso(&NuovoIncasso { condominio_id: s.cond, persona_id: s.persone[0], conto_corrente_id: Some(s.cc), data: "2026-02-10".into(), importo: dovuto + 5_000, metodo: "bonifico".into(), ..Default::default() }).unwrap();
    assert_eq!(s.db.saldo_conto_corrente(s.cc).unwrap(), dovuto + 5_000);
    // Bianchi paga usando solo il credito? non ne ha: verifica che il saldo banca non cambi con importo 0 e Salvadanaio vuoto
    assert!(s.db.registra_incasso(&NuovoIncasso { condominio_id: s.cond, persona_id: s.persone[1], conto_corrente_id: Some(s.cc), data: "2026-02-11".into(), importo: 0, usa_salvadanaio: 100, ..Default::default() }).is_err());
    assert!(s.db.verifica_movimenti(s.cond).unwrap().is_empty());
}

// ---------------------------------------------------------------- migrazione

#[test]
fn migrazione_dalla_partita_doppia_conserva_conti_e_movimenti() {
    let db = Db::in_memoria_a(2).unwrap();
    db.conn
        .execute_batch(
            "INSERT INTO condominio(id,nome) VALUES(1,'C');
             INSERT INTO conto_contabile(id,condominio_id,codice,nome,tipo) VALUES
               (1,1,'100','Banca A','attivo'),(2,1,'101','Banca B','attivo'),(3,1,'110','Crediti','attivo'),(4,1,'200','Debiti','passivo');
             INSERT INTO conto_corrente(id,condominio_id,nome,iban,conto_contabile_id) VALUES(1,1,'A','IT1',1),(2,1,'B','',2);
             INSERT INTO persona(id,cognome) VALUES(1,'Rossi');
             INSERT INTO esercizio(id,condominio_id,nome,dal,al) VALUES(1,1,'2026','2026-01-01','2026-12-31');
             INSERT INTO fornitore(id,condominio_id,ragione_sociale) VALUES(1,1,'F');
             INSERT INTO registrazione(id,condominio_id,data,descrizione,protocollo,origine,origine_id) VALUES
               (1,1,'2026-02-01','Incasso A',1,'incasso',1),
               (2,1,'2026-02-02','Incasso stornato',2,'incasso',2),
               (3,1,'2026-02-03','Pagamento fattura',3,'pagamento_fattura',1),
               (4,1,'2026-02-04','STORNO',4,'storno',2),
               (5,1,'2026-02-05','Giroconto',5,'giroconto',NULL);
             UPDATE registrazione SET storno_di=4 WHERE id=2;
             INSERT INTO riga_reg(registrazione_id,conto_id,dare_cents,avere_cents) VALUES
               (1,1,10000,0),(1,3,0,10000),
               (2,1,5000,0),(2,3,0,5000),
               (3,4,3000,0),(3,1,0,3000),
               (4,3,5000,0),(4,1,0,5000),
               (5,2,2000,0),(5,1,0,2000);
             INSERT INTO incasso(id,condominio_id,persona_id,conto_corrente_id,data,importo_cents,stato,registrazione_id,storno_registrazione_id) VALUES
               (1,1,1,1,'2026-02-01',10000,'valido',1,NULL),
               (2,1,1,1,'2026-02-02',5000,'stornato',2,4);
             INSERT INTO fattura(id,condominio_id,esercizio_id,fornitore_id,numero,data,imponibile_cents,stato,data_pagamento,conto_corrente_id,registrazione_pag_id)
               VALUES(1,1,1,1,'1','2026-02-03',3000,'pagata','2026-02-03',1,3);",
        )
        .unwrap();
    db.migra().unwrap();

    let v: i64 = db.conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(v, 3);
    let tabelle: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM sqlite_master WHERE name IN ('conto_contabile','registrazione','riga_reg')", [], |r| r.get(0))
        .unwrap();
    assert_eq!(tabelle, 0, "le tabelle della partita doppia sono state rimosse");
    // saldi dei conti correnti identici a quelli della contabilità
    assert_eq!(db.saldo_conto_corrente(1).unwrap(), 10_000 + 5_000 - 3_000 - 5_000 - 2_000);
    assert_eq!(db.saldo_conto_corrente(2).unwrap(), 2_000);
    assert_eq!(db.elenco_conti_correnti(1).unwrap().len(), 2);
    // storno collegato: il movimento originale risulta stornato
    let movs = db.elenco_movimenti(1, None, None, None).unwrap();
    let orig = movs.iter().find(|m| m.descrizione == "Incasso stornato").unwrap();
    let storno = movs.iter().find(|m| m.origine == "storno").unwrap();
    assert!(orig.stornato);
    assert_eq!(storno.storno_di, Some(orig.id));
    // i documenti puntano ai nuovi movimenti
    let (mi, ms): (Option<i64>, Option<i64>) =
        db.conn.query_row("SELECT movimento_id,storno_movimento_id FROM incasso WHERE id=2", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!((mi, ms), (Some(orig.id), Some(storno.id)));
    let pag: Option<i64> = db.conn.query_row("SELECT movimento_pag_id FROM fattura WHERE id=1", [], |r| r.get(0)).unwrap();
    assert!(pag.is_some());
    assert!(db.verifica_movimenti(1).unwrap().is_empty(), "{:?}", db.verifica_movimenti(1).unwrap());
    // il database migrato è pienamente operativo
    let g = db.giroconto(1, 1, 2, 1_000, "2026-03-01", "test").unwrap();
    assert!(g > 0);
    assert_eq!(db.saldo_conto_corrente(2).unwrap(), 3_000);
}

#[test]
fn migrazione_di_un_database_vuoto_e_idempotente() {
    let db = Db::in_memoria().unwrap();
    db.migra().unwrap();
    db.migra().unwrap();
    let v: i64 = db.conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(v, 3);
}

// ------------------------------------------------------------ fatture XML

const XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<p:FatturaElettronica versione="FPR12" xmlns:p="http://ivaservizi.agenziaentrate.gov.it/docs/xsd/fatture/v1.2">
  <FatturaElettronicaHeader>
    <CedentePrestatore>
      <DatiAnagrafici>
        <IdFiscaleIVA><IdPaese>IT</IdPaese><IdCodice>01234567890</IdCodice></IdFiscaleIVA>
        <CodiceFiscale>01234567890</CodiceFiscale>
        <Anagrafica><Denominazione>Edil &amp; Costruzioni Srl</Denominazione></Anagrafica>
      </DatiAnagrafici>
    </CedentePrestatore>
  </FatturaElettronicaHeader>
  <FatturaElettronicaBody>
    <DatiGenerali>
      <DatiGeneraliDocumento>
        <TipoDocumento>TD01</TipoDocumento>
        <Data>2026-05-12</Data>
        <Numero>FT-45/2026</Numero>
        <DatiRitenuta><TipoRitenuta>RT02</TipoRitenuta><ImportoRitenuta>40.00</ImportoRitenuta><AliquotaRitenuta>4.00</AliquotaRitenuta><CausalePagamento>A</CausalePagamento></DatiRitenuta>
        <ImportoTotaleDocumento>1220.00</ImportoTotaleDocumento>
        <Causale>Rifacimento intonaco androne</Causale>
      </DatiGeneraliDocumento>
    </DatiGenerali>
    <DatiBeniServizi>
      <DettaglioLinee><NumeroLinea>1</NumeroLinea><Descrizione>Intonaco</Descrizione><PrezzoTotale>1000.00</PrezzoTotale></DettaglioLinee>
      <DatiRiepilogo><AliquotaIVA>22.00</AliquotaIVA><ImponibileImporto>1000.00</ImponibileImporto><Imposta>220.00</Imposta></DatiRiepilogo>
    </DatiBeniServizi>
    <DatiPagamento><DettaglioPagamento><IBAN>IT60X0542811101000000123456</IBAN><DataScadenzaPagamento>2026-06-11</DataScadenzaPagamento></DettaglioPagamento></DatiPagamento>
  </FatturaElettronicaBody>
  <FatturaElettronicaBody>
    <DatiGenerali><DatiGeneraliDocumento><TipoDocumento>TD04</TipoDocumento><Data>2026-05-20</Data><Numero>NC-1</Numero><ImportoTotaleDocumento>122.00</ImportoTotaleDocumento></DatiGeneraliDocumento></DatiGenerali>
    <DatiBeniServizi><DatiRiepilogo><ImponibileImporto>100.00</ImponibileImporto><Imposta>22.00</Imposta></DatiRiepilogo></DatiBeniServizi>
  </FatturaElettronicaBody>
</p:FatturaElettronica>"#;

#[test]
fn lettura_fattura_elettronica() {
    use condominio::fatturapa::*;
    let v = leggi_fatture_xml(XML).unwrap();
    assert_eq!(v.len(), 2);
    let f = &v[0];
    assert_eq!(f.fornitore, "Edil & Costruzioni Srl");
    assert_eq!(f.piva, "01234567890");
    assert_eq!((f.numero.as_str(), f.data.as_str()), ("FT-45/2026", "2026-05-12"));
    assert_eq!((f.imponibile, f.iva, f.totale, f.ritenuta), (100_000, 22_000, 122_000, 4_000));
    assert_eq!(f.aliquota_ritenuta_ppm, 40_000);
    assert_eq!(f.iban, "IT60X0542811101000000123456");
    assert_eq!(f.descrizione, "Rifacimento intonaco androne");
    assert_eq!(v[1].tipo_documento, "TD04");
    assert_eq!(importo_xml("12.5"), Some(1250));
    assert_eq!(importo_xml("-0.07"), Some(-7));
    assert!(leggi_fatture_xml("<altro/>").is_err());
    assert!(leggi_fatture_xml("non è xml <<<").is_err());
}

#[test]
fn import_fattura_xml_crea_fornitore_registra_e_salta_note_di_credito() {
    use condominio::fatturapa::*;
    let s = scenario();
    let e = s.db.importa_fatture_xml(s.cond, s.es, XML.as_bytes(), Some(s.voce), "file1.xml").unwrap();
    assert_eq!(e.importate.len(), 1);
    assert_eq!(e.saltate.len(), 1, "la nota di credito non si importa");
    assert_eq!(e.nuovi_fornitori, 1);
    assert!(e.avvisi.is_empty(), "{:?}", e.avvisi);
    let forn = s.db.elenco_fornitori(s.cond).unwrap();
    assert_eq!(forn.len(), 1);
    assert_eq!(forn[0].regime_ritenuta, "appalti4", "dedotto dall'aliquota 4% del file");
    let fatt = s.db.elenco_fatture(s.cond, Some(s.es)).unwrap();
    assert_eq!(fatt.len(), 1);
    assert_eq!((fatt[0].imponibile, fatt[0].iva, fatt[0].ritenuta), (100_000, 22_000, 4_000));
    assert_eq!(s.db.consuntivo_voce(s.voce).unwrap(), 122_000);
    // seconda importazione: stesso fornitore, fattura duplicata non registrata
    let e2 = s.db.importa_fatture_xml(s.cond, s.es, XML.as_bytes(), None, "file1.xml").unwrap();
    assert_eq!(e2.importate.len(), 0);
    assert_eq!(e2.nuovi_fornitori, 0);
    assert!(e2.saltate.iter().any(|x| x.contains("già registrata")), "{:?}", e2.saltate);
    assert_eq!(s.db.elenco_fornitori(s.cond).unwrap().len(), 1);
    // file firmato .p7m: l'XML è incorporato dopo un'intestazione binaria
    let mut p7m = vec![0x30, 0x82, 0x0a, 0x1b, 0x06, 0x09, 0x2a, 0x86, 0x48];
    p7m.extend_from_slice(XML.as_bytes());
    p7m.extend_from_slice(&[0x00, 0x01, 0x02]);
    assert!(estrai_xml(&p7m).unwrap().ends_with("FatturaElettronica>"));
    assert!(estrai_xml(b"\x00\x01 niente").is_err());
}
