//! Condominio di esempio: dati fittizi per provare il programma senza inserire nulla a mano.

use crate::anagrafica::*;
use crate::assemblee::{Assemblea, Punto};
use crate::fornitori::*;
use crate::incassi::NuovoIncasso;
use crate::pratiche::Pratica;
use crate::preventivo::*;
use crate::util::*;
use crate::Db;
use anyhow::Result;
use chrono::{Datelike, Duration};

impl Db {
    /// Crea «Condominio Esempio» con anagrafica, millesimi, preventivo, piano rate, incassi, fatture,
    /// estratto conto da riconciliare, assemblea convocata, segnalazione e pratica. Restituisce l'id del condominio.
    pub fn carica_demo(&self) -> Result<i64> {
        self.atomico(|db| {
            let oggi = oggi();
            let anno = oggi.year();
            let it = |d: chrono::NaiveDate| d.format("%d/%m/%Y").to_string();
            let cond = db.salva_condominio(&Condominio {
                nome: "Condominio Esempio".into(),
                indirizzo: "Via Roma 10".into(),
                citta: "Milano".into(),
                codice_fiscale: "90000000000".into(),
                ..Default::default()
            })?;
            let cc = db.crea_conto_corrente(cond, "Conto corrente principale", "IT60X0542811101000000123456")?;

            // ---- persone e unità
            let pers = |cognome: &str, nome: &str, email: &str, tipo: &str| -> Result<i64> {
                db.salva_persona(&Persona { tipo: tipo.into(), nome: nome.into(), cognome: cognome.into(), email: email.into(), ..Default::default() })
            };
            let rossi = pers("Rossi", "Mario", "mario.rossi@example.com", "fisica")?;
            let bianchi = pers("Bianchi", "Laura", "laura.bianchi@example.com", "fisica")?;
            let neri = pers("Neri", "Paolo", "", "fisica")?;
            let verdi = pers("Verdi", "Giulia", "giulia.verdi@example.com", "fisica")?;
            let gialli = pers("Gialli", "Marco", "", "fisica")?;
            let russo = pers("Russo", "Anna", "anna.russo@example.com", "fisica")?;
            let esposito = pers("Esposito", "Luca", "", "fisica")?;
            let ferrari = pers("Ferrari Abbigliamento Srl", "", "info@ferrari.example.com", "giuridica")?;
            let mut unita = vec![];
            let dati = [("1", "1", "appartamento"), ("2", "1", "appartamento"), ("3", "2", "appartamento"), ("4", "2", "appartamento"), ("5", "3", "appartamento"), ("6", "0", "negozio")];
            for (interno, piano, tipo) in dati {
                unita.push(db.salva_unita(&Unita { condominio_id: cond, scala: "A".into(), piano: piano.into(), interno: interno.into(), tipo: tipo.into(), ..Default::default() })?);
            }
            db.aggiungi_titolarita(unita[0], rossi, "proprietario", 1_000_000, false)?;
            db.aggiungi_titolarita(unita[1], bianchi, "proprietario", 1_000_000, false)?;
            db.aggiungi_titolarita(unita[1], neri, "inquilino", 1_000_000, false)?;
            db.aggiungi_titolarita(unita[2], verdi, "proprietario", 500_000, false)?;
            db.aggiungi_titolarita(unita[2], gialli, "proprietario", 500_000, false)?;
            db.aggiungi_titolarita(unita[3], russo, "proprietario", 1_000_000, false)?;
            db.aggiungi_titolarita(unita[4], esposito, "proprietario", 1_000_000, false)?;
            db.aggiungi_titolarita(unita[5], ferrari, "proprietario", 1_000_000, false)?;

            // ---- tabelle millesimali
            let tab_a = db.salva_tabella(&Tabella { condominio_id: cond, codice: "A".into(), nome: "Proprietà generale".into(), totale_micro: 1_000_000, ..Default::default() })?;
            let tab_b = db.salva_tabella(&Tabella { condominio_id: cond, codice: "B".into(), nome: "Scala e ascensore".into(), totale_micro: 1_000_000, ..Default::default() })?;
            for (u, m) in unita.iter().zip(["180", "170", "160", "170", "150", "170"]) {
                db.imposta_millesimo(tab_a, *u, parse_millesimi(m).unwrap())?;
            }
            for u in &unita[..5] {
                db.imposta_millesimo(tab_b, *u, parse_millesimi("200").unwrap())?;
            }

            // ---- esercizio, preventivo, delibera
            let es = db.salva_esercizio(&Esercizio { condominio_id: cond, nome: anno.to_string(), dal: format!("01/01/{}", anno), al: format!("31/12/{}", anno), ..Default::default() })?;
            let voce = |parent: Option<i64>, cod: &str, descr: &str, imp: Cents, tab: Option<i64>| -> Result<i64> {
                db.salva_voce(&VoceSpesa { esercizio_id: es, parent_id: parent, codice: cod.into(), descrizione: descr.into(), importo_cents: imp, tabella_id: tab, ..Default::default() })
            };
            let c1 = voce(None, "1", "Spese generali", 0, None)?;
            let v_pulizia = voce(Some(c1), "1.01", "Pulizia scale", 240_000, Some(tab_b))?;
            let v_luce = voce(Some(c1), "1.02", "Illuminazione parti comuni", 90_000, Some(tab_a))?;
            let v_amm = voce(Some(c1), "1.03", "Compenso amministratore", 180_000, Some(tab_a))?;
            let c2 = voce(None, "2", "Ascensore", 0, None)?;
            let v_asc = voce(Some(c2), "2.01", "Manutenzione e revisione ascensore", 150_000, Some(tab_b))?;
            let v_ass = voce(None, "3", "Assicurazione fabbricato", 120_000, Some(tab_a))?;
            // l'ascensore si ripartisce 70% sulla tabella B e 30% sulla generale; il negozio non lo usa
            db.imposta_riparto_voce(v_asc, &[(tab_b, 700_000), (tab_a, 300_000)])?;
            db.imposta_riduzione(v_asc, unita[5], 1_000_000, "Il negozio non usa l'ascensore")?;

            let delibera = db.salva_delibera(&Delibera {
                condominio_id: cond,
                esercizio_id: Some(es),
                data: format!("20/01/{}", anno),
                tipo: "preventivo".into(),
                oggetto: format!("Approvazione preventivo {}", anno),
                riferimento: format!("Verbale assemblea del 20/01/{}", anno),
                maggioranza: "art. 1136 c. 2 c.c.".into(),
                ..Default::default()
            })?;

            // ---- piano rate ordinario (4 rate trimestrali)
            let piano = db.crea_piano(&Piano {
                esercizio_id: es,
                nome: format!("Gestione ordinaria {}", anno),
                tipo: "ordinario".into(),
                delibera_id: Some(delibera),
                n_rate: 4,
                prima_scadenza: format!("31/01/{}", anno),
                cadenza_mesi: 3,
                ..Default::default()
            })?;
            for (v, i) in [(v_pulizia, 240_000), (v_luce, 90_000), (v_amm, 180_000), (v_asc, 150_000), (v_ass, 120_000)] {
                db.imposta_voce_piano(piano, v, i)?;
            }
            db.attiva_piano(piano)?;

            // ---- incassi: Rossi in regola, Bianchi parziale con eccedenza su una rata, gli altri morosi
            let paga = |persona: i64, solo_scadute: bool, extra: Cents| -> Result<()> {
                let aperte = db.rate_aperte(cond, persona)?;
                let totale: Cents = aperte.iter().filter(|r| !solo_scadute || r.scadenza.as_str() <= oggi.format("%Y-%m-%d").to_string().as_str()).map(|r| r.residuo()).sum();
                if totale + extra > 0 {
                    db.registra_incasso(&NuovoIncasso {
                        condominio_id: cond,
                        persona_id: persona,
                        conto_corrente_id: Some(cc),
                        data: oggi_iso(),
                        importo: totale + extra,
                        metodo: "bonifico".into(),
                        ..Default::default()
                    })?;
                }
                Ok(())
            };
            paga(rossi, true, 0)?;
            paga(verdi, true, 0)?;
            if let Some(r) = db.rate_aperte(cond, bianchi)?.first() {
                db.registra_incasso(&NuovoIncasso {
                    condominio_id: cond,
                    persona_id: bianchi,
                    conto_corrente_id: Some(cc),
                    data: oggi_iso(),
                    importo: r.residuo() / 2,
                    metodo: "bonifico".into(),
                    note: "Acconto".into(),
                    ..Default::default()
                })?;
            }
            paga(russo, true, 5_000)?; // versa 50 € in più: finiscono nel Salvadanaio

            // ---- fornitori e fatture
            let forn = |nome: &str, regime: &str| -> Result<i64> {
                db.salva_fornitore(&Fornitore { condominio_id: cond, ragione_sociale: nome.into(), cf_piva: "01234567890".into(), regime_ritenuta: regime.into(), ..Default::default() })
            };
            let f_pul = forn("Pulizie Lucente Srl", "nessuna")?;
            let f_asc = forn("Ascensori Rapido SpA", "appalti4")?;
            let f_tec = forn("Studio Tecnico Bassi", "professionista20")?;
            let f_ele = forn("Elettricista Fulmine", "nessuna")?;
            let fatt = |f: i64, num: &str, v: i64, imp: Cents, iva: Cents, giorni: i64| -> Result<i64> {
                db.registra_fattura(&NuovaFattura {
                    condominio_id: cond,
                    esercizio_id: es,
                    fornitore_id: f,
                    voce_id: Some(v),
                    numero: num.into(),
                    data: (oggi - Duration::days(giorni)).format("%Y-%m-%d").to_string(),
                    descrizione: String::new(),
                    imponibile: imp,
                    iva,
                })
            };
            let a = fatt(f_pul, "12", v_pulizia, 60_000, 13_200, 60)?;
            let b = fatt(f_asc, "88", v_asc, 75_000, 16_500, 45)?;
            let t = fatt(f_tec, "5", v_amm, 50_000, 0, 30)?;
            let _ = fatt(f_ele, "112", v_luce, 30_000, 6_600, 10)?;
            let pagato = (oggi - Duration::days(5)).format("%Y-%m-%d").to_string();
            for f in [a, b, t] {
                db.paga_fattura(f, &pagato, cc)?;
            }

            // ---- estratto conto da riconciliare
            let riga = |giorni: i64, d: &str, i: &str| format!("{};{};{}\n", it(oggi - Duration::days(giorni)), d, i);
            let csv = format!(
                "data;descrizione;importo\n{}{}{}",
                riga(2, "BONIFICO BIANCHI LAURA ACCONTO RATE", "250,00"),
                riga(3, "PAGAMENTO FATT 112 ELETTRICISTA FULMINE", "-366,00"),
                riga(9, "COMMISSIONI E CANONE CONTO", "-12,50")
            );
            db.importa_estratto(cond, cc, &csv)?;

            // ---- assemblea convocata tra 20 giorni
            let ass = db.salva_assemblea(&Assemblea {
                condominio_id: cond,
                esercizio_id: Some(es),
                tipo: "ordinaria".into(),
                modalita: "presenza".into(),
                luogo: "Sala condominiale, Via Roma 10".into(),
                data_prima: it(oggi + Duration::days(20)),
                ora_prima: "17:00".into(),
                data_seconda: Some(it(oggi + Duration::days(21))),
                ora_seconda: "18:00".into(),
                tabella_id: Some(tab_a),
                ..Default::default()
            })?;
            db.salva_punto(&Punto { assemblea_id: ass, titolo: format!("Approvazione rendiconto {}", anno - 1), maggioranza: "ordinaria".into(), tipo_delibera: "rendiconto".into(), ..Default::default() })?;
            db.salva_punto(&Punto {
                assemblea_id: ass,
                titolo: "Sostituzione impianto citofonico".into(),
                descrizione: "Preventivi a confronto e relativa ripartizione.".into(),
                maggioranza: "ordinaria".into(),
                tipo_delibera: "straordinaria".into(),
                ..Default::default()
            })?;
            db.convoca_assemblea(ass, oggi)?;

            // ---- segnalazione, intervento, sinistro, agenda
            let tk = db.crea_ticket(cond, Some(unita[2]), Some(verdi), "Luce delle scale non funzionante al 2° piano", "La plafoniera lampeggia da tre giorni.", 2)?;
            db.intervento_da_ticket(tk, Some(f_ele))?;
            db.salva_pratica(&Pratica {
                condominio_id: cond,
                tipo: "sinistro".into(),
                titolo: "Infiltrazione dal lastrico solare".into(),
                descrizione: "Danni alla tinteggiatura del vano scale.".into(),
                riferimento: "Polizza 123456".into(),
                controparte: "Assicurazioni Alfa".into(),
                ..Default::default()
            })?;
            db.aggiungi_agenda(cond, &it(oggi + Duration::days(12)), "Rinnovo contratto di manutenzione ascensore", "Chiedere due preventivi alternativi.")?;
            Ok(cond)
        })
    }
}
