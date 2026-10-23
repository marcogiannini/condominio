//! Fornitori, fatture passive, pagamenti e ritenute d'acconto (versamento F24).
//!
//! Regole implementate (da far verificare al commercialista, sono parametri normativi):
//! - appalti/servizi d'impresa: ritenuta 4% sull'imponibile, F24 codice 1019 (IRPEF) o 1020 (IRES);
//! - professionisti: ritenuta 20% sull'imponibile (semplificazione: senza rivalsa cassa), F24 codice 1040;
//! - 4%: se le ritenute operate raggiungono 500 € -> versamento entro il 16 del mese successivo;
//!   sotto soglia -> 30 giugno / 20 dicembre cumulando gli importi.

use crate::db::Db;
use crate::util::*;
use anyhow::{bail, Result};
use chrono::{Datelike, NaiveDate};
use rusqlite::params;
use std::collections::BTreeMap;

pub const SOGLIA_RITENUTE_CENTS: Cents = 50_000;
pub const REGIMI: [&str; 3] = ["nessuna", "appalti4", "professionista20"];

#[derive(Clone, Debug, Default)]
pub struct Fornitore {
    pub id: i64,
    pub condominio_id: i64,
    pub ragione_sociale: String,
    pub cf_piva: String,
    pub email: String,
    pub iban: String,
    pub regime_ritenuta: String,
    pub soggetto_ires: bool,
}

#[derive(Clone, Debug, Default)]
pub struct NuovaFattura {
    pub condominio_id: i64,
    pub esercizio_id: i64,
    pub fornitore_id: i64,
    pub voce_id: Option<i64>,
    pub numero: String,
    pub data: String, // ISO
    pub descrizione: String,
    pub imponibile: Cents,
    pub iva: Cents,
}

#[derive(Clone, Debug)]
pub struct FatturaRiga {
    pub id: i64,
    pub fornitore: String,
    pub numero: String,
    pub data: String,
    pub descrizione: String,
    pub imponibile: Cents,
    pub iva: Cents,
    pub ritenuta: Cents,
    pub stato: String,
    pub data_pagamento: Option<String>,
    pub versata: bool,
}

impl FatturaRiga {
    pub fn totale(&self) -> Cents {
        self.imponibile + self.iva
    }
    pub fn netto_a_pagare(&self) -> Cents {
        self.totale() - self.ritenuta
    }
}

#[derive(Clone, Debug)]
pub struct GruppoRitenute {
    pub codice_tributo: String,
    pub scadenza: String, // ISO
    pub importo: Cents,
    pub fatture: Vec<i64>,
}

pub fn calcola_ritenuta(regime: &str, imponibile: Cents) -> Cents {
    match regime {
        "appalti4" => percentuale(imponibile, 400),
        "professionista20" => percentuale(imponibile, 2000),
        _ => 0,
    }
}

fn scadenza_semestrale(d: NaiveDate) -> NaiveDate {
    if d.month() <= 6 {
        NaiveDate::from_ymd_opt(d.year(), 6, 30).unwrap()
    } else {
        NaiveDate::from_ymd_opt(d.year(), 12, 20).unwrap()
    }
}

/// Elementi: (fattura_id, data pagamento ISO, ritenuta, regime, soggetto_ires).
pub fn raggruppa_ritenute(mut items: Vec<(i64, String, Cents, String, bool)>) -> Vec<GruppoRitenute> {
    items.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
    let iso = |d: NaiveDate| d.format("%Y-%m-%d").to_string();
    let mut out: Vec<GruppoRitenute> = vec![];

    // 4% — soglia dei 500 € cumulativa per codice tributo
    for ires in [false, true] {
        let codice = if ires { "1020" } else { "1019" };
        let mut cum: Cents = 0;
        let mut pendenti: Vec<&(i64, String, Cents, String, bool)> = vec![];
        for it in items.iter().filter(|x| x.3 == "appalti4" && x.4 == ires && x.2 > 0) {
            cum += it.2;
            pendenti.push(it);
            if cum >= SOGLIA_RITENUTE_CENTS {
                let d = parse_data(&it.1).unwrap_or_else(oggi);
                out.push(GruppoRitenute {
                    codice_tributo: codice.into(),
                    scadenza: iso(giorno_mese_successivo(d, 16)),
                    importo: cum,
                    fatture: pendenti.iter().map(|x| x.0).collect(),
                });
                cum = 0;
                pendenti.clear();
            }
        }
        // sotto soglia: 30/06 e 20/12
        let mut per_scad: BTreeMap<String, (Cents, Vec<i64>)> = BTreeMap::new();
        for it in pendenti {
            let d = parse_data(&it.1).unwrap_or_else(oggi);
            let e = per_scad.entry(iso(scadenza_semestrale(d))).or_insert((0, vec![]));
            e.0 += it.2;
            e.1.push(it.0);
        }
        for (scad, (imp, ids)) in per_scad {
            out.push(GruppoRitenute { codice_tributo: codice.into(), scadenza: scad, importo: imp, fatture: ids });
        }
    }

    // professionisti: 16 del mese successivo
    let mut prof: BTreeMap<String, (Cents, Vec<i64>)> = BTreeMap::new();
    for it in items.iter().filter(|x| x.3 == "professionista20" && x.2 > 0) {
        let d = parse_data(&it.1).unwrap_or_else(oggi);
        let e = prof.entry(iso(giorno_mese_successivo(d, 16))).or_insert((0, vec![]));
        e.0 += it.2;
        e.1.push(it.0);
    }
    for (scad, (imp, ids)) in prof {
        out.push(GruppoRitenute { codice_tributo: "1040".into(), scadenza: scad, importo: imp, fatture: ids });
    }
    out.sort_by(|a, b| a.scadenza.cmp(&b.scadenza).then(a.codice_tributo.cmp(&b.codice_tributo)));
    out
}

impl Db {
    pub fn salva_fornitore(&self, f: &Fornitore) -> Result<i64> {
        if f.ragione_sociale.trim().is_empty() {
            bail!("La ragione sociale è obbligatoria");
        }
        if !REGIMI.contains(&f.regime_ritenuta.as_str()) {
            bail!("Regime di ritenuta non valido");
        }
        if f.id == 0 {
            self.conn.execute(
                "INSERT INTO fornitore(condominio_id,ragione_sociale,cf_piva,email,iban,regime_ritenuta,soggetto_ires) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![f.condominio_id, f.ragione_sociale.trim(), f.cf_piva, f.email, f.iban, f.regime_ritenuta, f.soggetto_ires as i64],
            )?;
            Ok(self.conn.last_insert_rowid())
        } else {
            self.conn.execute(
                "UPDATE fornitore SET ragione_sociale=?2,cf_piva=?3,email=?4,iban=?5,regime_ritenuta=?6,soggetto_ires=?7 WHERE id=?1",
                params![f.id, f.ragione_sociale.trim(), f.cf_piva, f.email, f.iban, f.regime_ritenuta, f.soggetto_ires as i64],
            )?;
            Ok(f.id)
        }
    }

    pub fn elenco_fornitori(&self, cond: i64) -> Result<Vec<Fornitore>> {
        let mut st = self.conn.prepare(
            "SELECT id,condominio_id,ragione_sociale,cf_piva,email,iban,regime_ritenuta,soggetto_ires FROM fornitore WHERE condominio_id=?1 ORDER BY ragione_sociale",
        )?;
        let v = st
            .query_map([cond], |r| {
                Ok(Fornitore {
                    id: r.get(0)?,
                    condominio_id: r.get(1)?,
                    ragione_sociale: r.get(2)?,
                    cf_piva: r.get(3)?,
                    email: r.get(4)?,
                    iban: r.get(5)?,
                    regime_ritenuta: r.get(6)?,
                    soggetto_ires: r.get::<_, i64>(7)? != 0,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(v)
    }

    pub fn registra_fattura(&self, n: &NuovaFattura) -> Result<i64> {
        if n.numero.trim().is_empty() {
            bail!("Numero fattura obbligatorio");
        }
        if n.imponibile <= 0 {
            bail!("L'imponibile deve essere positivo");
        }
        if n.iva < 0 {
            bail!("IVA non valida");
        }
        if parse_data(&n.data).is_none() {
            bail!("Data fattura non valida");
        }
        let regime: String =
            self.conn.query_row("SELECT regime_ritenuta FROM fornitore WHERE id=?1", [n.fornitore_id], |r| r.get(0))?;
        let dup: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM fattura WHERE fornitore_id=?1 AND numero=?2 AND substr(data,1,4)=substr(?3,1,4) AND stato<>'stornata'",
            params![n.fornitore_id, n.numero.trim(), n.data],
            |r| r.get(0),
        )?;
        if dup > 0 {
            bail!("Fattura già registrata per questo fornitore (stesso numero e anno)");
        }
        let rit = calcola_ritenuta(&regime, n.imponibile);
        self.atomico(|db| {
            db.conn.execute(
                "INSERT INTO fattura(condominio_id,esercizio_id,fornitore_id,voce_id,numero,data,descrizione,imponibile_cents,iva_cents,ritenuta_cents)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![n.condominio_id, n.esercizio_id, n.fornitore_id, n.voce_id, n.numero.trim(), n.data, n.descrizione, n.imponibile, n.iva, rit],
            )?;
            let id = db.conn.last_insert_rowid();
            Ok(id)
        })
    }

    pub fn paga_fattura(&self, fattura: i64, data: &str, conto_corrente: i64) -> Result<()> {
        if parse_data(data).is_none() {
            bail!("Data di pagamento non valida");
        }
        self.atomico(|db| {
            let (cond, stato, tot, rit, num): (i64, String, Cents, Cents, String) = db.conn.query_row(
                "SELECT condominio_id,stato,imponibile_cents+iva_cents,ritenuta_cents,numero FROM fattura WHERE id=?1",
                [fattura],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )?;
            if stato != "da_pagare" {
                bail!("La fattura non è da pagare (stato: {})", stato);
            }
            let netto = tot - rit;
            let forn: String = db.conn.query_row(
                "SELECT fo.ragione_sociale FROM fattura f JOIN fornitore fo ON fo.id=f.fornitore_id WHERE f.id=?1",
                [fattura],
                |r| r.get(0),
            )?;
            let mov = db.registra_movimento(
                cond,
                conto_corrente,
                data,
                &format!("Pagamento fattura {} – {}", num, forn),
                -netto,
                "pagamento_fattura",
                Some(fattura),
            )?;
            db.conn.execute(
                "UPDATE fattura SET stato='pagata',data_pagamento=?2,conto_corrente_id=?3,movimento_pag_id=?4 WHERE id=?1",
                params![fattura, data, conto_corrente, mov],
            )?;
            Ok(())
        })
    }

    pub fn storna_pagamento_fattura(&self, fattura: i64, data: &str) -> Result<()> {
        self.atomico(|db| {
            let (stato, reg, vers): (String, Option<i64>, Option<i64>) = db.conn.query_row(
                "SELECT stato,movimento_pag_id,versamento_id FROM fattura WHERE id=?1",
                [fattura],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?;
            if stato != "pagata" {
                bail!("La fattura non risulta pagata");
            }
            if vers.is_some() {
                bail!("La ritenuta di questa fattura è già stata versata con F24");
            }
            if let Some(r) = reg {
                db.storna_movimento(r, data, "storno pagamento")?;
            }
            db.conn.execute(
                "UPDATE fattura SET stato='da_pagare',data_pagamento=NULL,conto_corrente_id=NULL,movimento_pag_id=NULL WHERE id=?1",
                [fattura],
            )?;
            Ok(())
        })
    }

    pub fn storna_fattura(&self, fattura: i64, _data: &str) -> Result<()> {
        let stato: String = self.conn.query_row("SELECT stato FROM fattura WHERE id=?1", [fattura], |r| r.get(0))?;
        if stato == "pagata" {
            bail!("Fattura pagata: storna prima il pagamento");
        }
        if stato == "stornata" {
            bail!("Fattura già stornata");
        }
        self.conn.execute("UPDATE fattura SET stato='stornata' WHERE id=?1", [fattura])?;
        Ok(())
    }

    pub fn elenco_fatture(&self, cond: i64, esercizio: Option<i64>) -> Result<Vec<FatturaRiga>> {
        let mut st = self.conn.prepare(
            "SELECT f.id,fo.ragione_sociale,f.numero,f.data,f.descrizione,f.imponibile_cents,f.iva_cents,f.ritenuta_cents,f.stato,f.data_pagamento,f.versamento_id
             FROM fattura f JOIN fornitore fo ON fo.id=f.fornitore_id
             WHERE f.condominio_id=?1 AND (?2 IS NULL OR f.esercizio_id=?2) ORDER BY f.data DESC,f.id DESC",
        )?;
        let v = st
            .query_map(params![cond, esercizio], |r| {
                Ok(FatturaRiga {
                    id: r.get(0)?,
                    fornitore: r.get(1)?,
                    numero: r.get(2)?,
                    data: r.get(3)?,
                    descrizione: r.get(4)?,
                    imponibile: r.get(5)?,
                    iva: r.get(6)?,
                    ritenuta: r.get(7)?,
                    stato: r.get(8)?,
                    data_pagamento: r.get(9)?,
                    versata: r.get::<_, Option<i64>>(10)?.is_some(),
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(v)
    }

    /// Ritenute operate (fatture pagate) non ancora versate, raggruppate per F24.
    pub fn ritenute_da_versare(&self, cond: i64) -> Result<Vec<GruppoRitenute>> {
        let mut st = self.conn.prepare(
            "SELECT f.id,f.data_pagamento,f.ritenuta_cents,fo.regime_ritenuta,fo.soggetto_ires
             FROM fattura f JOIN fornitore fo ON fo.id=f.fornitore_id
             WHERE f.condominio_id=?1 AND f.stato='pagata' AND f.ritenuta_cents>0 AND f.versamento_id IS NULL",
        )?;
        let items: Vec<(i64, String, Cents, String, bool)> = st
            .query_map([cond], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get::<_, i64>(4)? != 0)))?
            .collect::<Result<_, _>>()?;
        Ok(raggruppa_ritenute(items))
    }

    /// Versa un gruppo di ritenute con F24: Dare Erario c/ritenute / Avere Banca.
    pub fn versa_ritenute(&self, cond: i64, g: &GruppoRitenute, data: &str, conto_corrente: i64) -> Result<i64> {
        if parse_data(data).is_none() {
            bail!("Data di versamento non valida");
        }
        self.atomico(|db| {
            db.conn.execute(
                "INSERT INTO versamento_ritenute(condominio_id,data,codice_tributo,importo_cents,conto_corrente_id) VALUES(?1,?2,?3,?4,?5)",
                params![cond, data, g.codice_tributo, g.importo, conto_corrente],
            )?;
            let vid = db.conn.last_insert_rowid();
            let mov = db.registra_movimento(
                cond,
                conto_corrente,
                data,
                &format!("Versamento F24 ritenute – cod. {}", g.codice_tributo),
                -g.importo,
                "f24",
                Some(vid),
            )?;
            db.conn.execute("UPDATE versamento_ritenute SET movimento_id=?2 WHERE id=?1", params![vid, mov])?;
            for f in &g.fatture {
                db.conn.execute("UPDATE fattura SET versamento_id=?2 WHERE id=?1", params![f, vid])?;
            }
            Ok(vid)
        })
    }

    /// Annulla un versamento F24: storna il movimento bancario e rende di nuovo "da versare" le ritenute collegate.
    pub fn storna_versamento_ritenute(&self, versamento: i64, data: &str) -> Result<()> {
        self.atomico(|db| {
            let mov: Option<i64> =
                db.conn.query_row("SELECT movimento_id FROM versamento_ritenute WHERE id=?1", [versamento], |r| r.get(0))
                    .map_err(|_| anyhow::anyhow!("Versamento non trovato"))?;
            if let Some(m) = mov {
                db.storna_movimento(m, data, "storno versamento F24")?;
            }
            db.conn.execute("UPDATE fattura SET versamento_id=NULL WHERE versamento_id=?1", [versamento])?;
            db.conn.execute("UPDATE movimento_banca SET versamento_id=NULL WHERE versamento_id=?1", [versamento])?;
            db.conn.execute("DELETE FROM versamento_ritenute WHERE id=?1", [versamento])?;
            Ok(())
        })
    }
}
