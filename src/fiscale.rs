//! Adempimenti fiscali del sostituto d'imposta: riepilogo per la Certificazione Unica e per il 770 semplificato.

use crate::db::Db;
use crate::util::*;
use anyhow::Result;
use rusqlite::params;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct RigaCu {
    pub fornitore_id: i64,
    pub ragione_sociale: String,
    pub cf_piva: String,
    pub regime: String,
    pub n_fatture: i64,
    /// Ammontare corrisposto nell'anno, al netto di IVA (base della ritenuta).
    pub imponibile: Cents,
    pub iva: Cents,
    pub ritenuta_operata: Cents,
    pub ritenuta_versata: Cents,
}

#[derive(Clone, Debug)]
pub struct RigaSt {
    pub mese: u32,
    pub codice_tributo: String,
    pub operate: Cents,
    pub versate: Cents,
}

pub fn codice_tributo(regime: &str, ires: bool) -> &'static str {
    match (regime, ires) {
        ("appalti4", false) => "1019",
        ("appalti4", true) => "1020",
        ("professionista20", _) => "1040",
        _ => "",
    }
}

fn csv(s: &str) -> String {
    if s.contains(';') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

impl Db {
    /// Per ogni percipiente con ritenuta operata nell'anno (criterio di cassa: data di pagamento).
    pub fn riepilogo_cu(&self, cond: i64, anno: i32) -> Result<Vec<RigaCu>> {
        let mut st = self.conn.prepare(
            "SELECT fo.id,fo.ragione_sociale,fo.cf_piva,fo.regime_ritenuta,COUNT(*),SUM(f.imponibile_cents),SUM(f.iva_cents),
                    SUM(f.ritenuta_cents),SUM(CASE WHEN f.versamento_id IS NOT NULL THEN f.ritenuta_cents ELSE 0 END)
             FROM fattura f JOIN fornitore fo ON fo.id=f.fornitore_id
             WHERE f.condominio_id=?1 AND f.stato='pagata' AND f.ritenuta_cents>0 AND substr(f.data_pagamento,1,4)=?2
             GROUP BY fo.id ORDER BY fo.ragione_sociale",
        )?;
        let v = st
            .query_map(params![cond, format!("{:04}", anno)], |r| {
                Ok(RigaCu {
                    fornitore_id: r.get(0)?,
                    ragione_sociale: r.get(1)?,
                    cf_piva: r.get(2)?,
                    regime: r.get(3)?,
                    n_fatture: r.get(4)?,
                    imponibile: r.get(5)?,
                    iva: r.get(6)?,
                    ritenuta_operata: r.get(7)?,
                    ritenuta_versata: r.get(8)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(v)
    }

    /// Prospetto per mese e codice tributo: ritenute operate (mese del pagamento) e versate (mese del versamento F24).
    pub fn riepilogo_770(&self, cond: i64, anno: i32) -> Result<Vec<RigaSt>> {
        let mut mappa: BTreeMap<(u32, String), (Cents, Cents)> = BTreeMap::new();
        let mut st = self.conn.prepare(
            "SELECT f.data_pagamento,f.ritenuta_cents,fo.regime_ritenuta,fo.soggetto_ires FROM fattura f JOIN fornitore fo ON fo.id=f.fornitore_id
             WHERE f.condominio_id=?1 AND f.stato='pagata' AND f.ritenuta_cents>0 AND substr(f.data_pagamento,1,4)=?2",
        )?;
        let operate: Vec<(String, Cents, String, i64)> =
            st.query_map(params![cond, format!("{:04}", anno)], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<Result<_, _>>()?;
        drop(st);
        for (d, rit, regime, ires) in operate {
            let mese = parse_data(&d).map(|x| chrono::Datelike::month(&x)).unwrap_or(1);
            mappa.entry((mese, codice_tributo(&regime, ires != 0).to_string())).or_default().0 += rit;
        }
        let mut st = self.conn.prepare(
            "SELECT data,codice_tributo,importo_cents FROM versamento_ritenute WHERE condominio_id=?1 AND substr(data,1,4)=?2",
        )?;
        let versati: Vec<(String, String, Cents)> =
            st.query_map(params![cond, format!("{:04}", anno)], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<Result<_, _>>()?;
        drop(st);
        for (d, cod, imp) in versati {
            let mese = parse_data(&d).map(|x| chrono::Datelike::month(&x)).unwrap_or(1);
            mappa.entry((mese, cod)).or_default().1 += imp;
        }
        Ok(mappa.into_iter().map(|((mese, codice_tributo), (operate, versate))| RigaSt { mese, codice_tributo, operate, versate }).collect())
    }

    pub fn csv_cu(&self, cond: i64, anno: i32) -> Result<String> {
        let mut s = String::from("percipiente;cf_piva;regime;codice_tributo;n_fatture;ammontare_corrisposto;iva;ritenuta_operata;ritenuta_versata\n");
        for r in self.riepilogo_cu(cond, anno)? {
            let ires: i64 = self.conn.query_row("SELECT soggetto_ires FROM fornitore WHERE id=?1", [r.fornitore_id], |x| x.get(0))?;
            s.push_str(&format!(
                "{};{};{};{};{};{};{};{};{}\n",
                csv(&r.ragione_sociale),
                csv(&r.cf_piva),
                r.regime,
                codice_tributo(&r.regime, ires != 0),
                r.n_fatture,
                format_eur(r.imponibile),
                format_eur(r.iva),
                format_eur(r.ritenuta_operata),
                format_eur(r.ritenuta_versata)
            ));
        }
        Ok(s)
    }
}
