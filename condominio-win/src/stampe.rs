//! Stampe: pagine HTML (A4) da aprire nel browser e stampare o salvare in PDF.

use crate::db::Db;
use crate::util::*;
use anyhow::Result;

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

const CSS: &str = r#"
@page { size: A4; margin: 16mm 14mm; }
* { box-sizing: border-box; }
body { font: 11pt/1.4 "Segoe UI", "DejaVu Sans", Arial, sans-serif; color: #1b1b1b; margin: 0 auto; max-width: 190mm; padding: 8mm; }
h1 { font-size: 17pt; margin: 0 0 2mm; }
h2 { font-size: 12.5pt; margin: 7mm 0 2mm; border-bottom: 1px solid #888; padding-bottom: 1mm; }
.testata { color: #444; margin-bottom: 5mm; font-size: 10pt; }
table { width: 100%; border-collapse: collapse; margin: 2mm 0; font-size: 10pt; }
th { text-align: left; background: #eee; }
th, td { border-bottom: 1px solid #ccc; padding: 1.2mm 2mm; vertical-align: top; }
td.n, th.n { text-align: right; white-space: nowrap; font-variant-numeric: tabular-nums; }
tr.tot td { font-weight: 700; border-top: 2px solid #555; }
.neg { color: #a02020; }
pre.testo { white-space: pre-wrap; font: inherit; }
.nota { color: #555; font-size: 9pt; margin-top: 6mm; }
button { font-size: 11pt; padding: 2mm 5mm; margin-bottom: 4mm; }
@media print { button { display: none; } body { padding: 0; } }
"#;

fn n(c: Cents) -> String {
    let t = format_eur(c);
    if c < 0 {
        format!("<span class=\"neg\">{}</span>", esc(&t))
    } else {
        esc(&t)
    }
}

impl Db {
    pub fn pagina_html(&self, cond: i64, titolo: &str, corpo: &str) -> Result<String> {
        let (nome, ind, cf): (String, String, String) =
            self.conn.query_row("SELECT nome,indirizzo||' '||citta,codice_fiscale FROM condominio WHERE id=?1", [cond], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        let studio = self.leggi_imp("studio_nome");
        let studio_ind = self.leggi_imp("studio_indirizzo");
        Ok(format!(
            "<!doctype html><html lang=\"it\"><head><meta charset=\"utf-8\"><title>{t}</title><style>{css}</style></head><body>\
             <button onclick=\"window.print()\">Stampa / Salva in PDF</button>\
             <h1>{t}</h1><div class=\"testata\"><b>Condominio {c}</b>{i}{f}<br>{s}{si}Stampato il {d}</div>{b}</body></html>",
            t = esc(titolo),
            css = CSS,
            c = esc(&nome),
            i = if ind.trim().is_empty() { String::new() } else { format!(" – {}", esc(ind.trim())) },
            f = if cf.is_empty() { String::new() } else { format!(" – C.F. {}", esc(&cf)) },
            s = if studio.trim().is_empty() { String::new() } else { format!("Amministratore: {} ", esc(&studio)) },
            si = if studio_ind.trim().is_empty() { String::new() } else { format!("({}) – ", esc(&studio_ind)) },
            d = data_it(&oggi_iso()),
            b = corpo
        ))
    }

    /// Documento di testo semplice (convocazione, verbale, sollecito…).
    pub fn html_testo(&self, cond: i64, titolo: &str, testo: &str) -> Result<String> {
        self.pagina_html(cond, titolo, &format!("<pre class=\"testo\">{}</pre>", esc(testo)))
    }

    /// Rendiconto: preventivo / consuntivo per voce e riparto per condòmino con conguaglio.
    pub fn html_rendiconto(&self, esercizio: i64) -> Result<String> {
        let (cond, nome_es, dal, al): (i64, String, String, String) =
            self.conn.query_row("SELECT condominio_id,nome,dal,al FROM esercizio WHERE id=?1", [esercizio], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        let righe = self.righe_consuntivo(esercizio)?;
        let mut b = format!("<p>Esercizio <b>{}</b> dal {} al {}</p><h2>Spese: preventivo e consuntivo</h2>", esc(&nome_es), data_it(&dal), data_it(&al));
        b.push_str("<table><tr><th>Cod.</th><th>Voce</th><th class=\"n\">Preventivo</th><th class=\"n\">Consuntivo</th><th class=\"n\">Scostamento</th></tr>");
        let (mut tp, mut tc) = (0, 0);
        for r in &righe {
            let capitolo = !r.foglia;
            b.push_str(&format!(
                "<tr{}><td>{}</td><td>{}{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td></tr>",
                if capitolo { " style=\"font-weight:700\"" } else { "" },
                esc(&r.codice),
                esc(&r.descrizione),
                if r.sopravvenienza { " <i>(sopravvenienza, art. 1130-bis)</i>" } else { "" },
                n(r.preventivo),
                n(r.consuntivo),
                n(r.scostamento())
            ));
            if r.foglia {
                tp += r.preventivo;
                tc += r.consuntivo;
            }
        }
        b.push_str(&format!("<tr class=\"tot\"><td></td><td>Totale</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td></tr></table>", n(tp), n(tc), n(tc - tp)));
        let non_imp = self.fatture_non_imputate(esercizio)?;
        if non_imp > 0 {
            b.push_str(&format!("<p class=\"nota\">Attenzione: fatture per {} non imputate ad alcuna voce di spesa (escluse dal riparto).</p>", n(non_imp)));
        }
        match self.quote_consuntivo(esercizio) {
            Ok(quote) => {
                b.push_str("<h2>Riparto per condòmino</h2><table><tr><th>Condòmino</th><th>Unità</th><th class=\"n\">Spesa effettiva</th><th class=\"n\">Rate emesse</th><th class=\"n\">Conguaglio</th><th class=\"n\">Versato</th><th class=\"n\">Ancora da versare</th></tr>");
                let (mut a, mut e, mut c, mut v, mut d) = (0, 0, 0, 0, 0);
                for q in &quote {
                    b.push_str(&format!(
                        "<tr><td>{}</td><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td></tr>",
                        esc(&q.persona), esc(&q.unita), n(q.consuntivo), n(q.emesso), n(q.conguaglio()), n(q.versato), n(q.da_versare())
                    ));
                    a += q.consuntivo;
                    e += q.emesso;
                    c += q.conguaglio();
                    v += q.versato;
                    d += q.da_versare();
                }
                b.push_str(&format!("<tr class=\"tot\"><td colspan=\"2\">Totale</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td></tr></table>", n(a), n(e), n(c), n(v), n(d)));
                b.push_str("<p class=\"nota\">Conguaglio positivo = da versare in più; negativo = a credito del condòmino. «Ancora da versare» = spesa effettiva meno quanto già versato.</p>");
            }
            Err(e) => b.push_str(&format!("<p class=\"neg\">Riparto non disponibile: {}</p>", esc(&e.to_string()))),
        }
        self.pagina_html(cond, &format!("Rendiconto – esercizio {}", nome_es), &b)
    }

    /// Rendiconto individuale di un condòmino.
    pub fn html_individuale(&self, esercizio: i64, persona: i64) -> Result<String> {
        let (cond, nome_es): (i64, String) = self.conn.query_row("SELECT condominio_id,nome FROM esercizio WHERE id=?1", [esercizio], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let p: String = self.conn.query_row("SELECT trim(cognome||' '||nome) FROM persona WHERE id=?1", [persona], |r| r.get(0))?;
        let quote = self.quote_consuntivo(esercizio)?;
        let q = quote.iter().find(|q| q.persona_id == persona);
        let mut b = format!("<p>Condòmino: <b>{}</b>{}</p>", esc(&p), q.map(|q| format!(" – unità: {}", esc(&q.unita))).unwrap_or_default());
        b.push_str("<h2>Spese di competenza</h2><table><tr><th>Cod.</th><th>Voce</th><th class=\"n\">Quota a carico</th></tr>");
        let mut tot = 0;
        for (cod, descr, quota) in self.dettaglio_persona_consuntivo(esercizio, persona)? {
            b.push_str(&format!("<tr><td>{}</td><td>{}</td><td class=\"n\">{}</td></tr>", esc(&cod), esc(&descr), n(quota)));
            tot += quota;
        }
        b.push_str(&format!("<tr class=\"tot\"><td></td><td>Totale spesa effettiva</td><td class=\"n\">{}</td></tr></table>", n(tot)));
        if let Some(q) = q {
            b.push_str(&format!(
                "<h2>Situazione</h2><table><tr><td>Rate emesse</td><td class=\"n\">{}</td></tr><tr><td>Conguaglio ({})</td><td class=\"n\">{}</td></tr><tr><td>Versato</td><td class=\"n\">{}</td></tr><tr class=\"tot\"><td>Ancora da versare</td><td class=\"n\">{}</td></tr></table>",
                n(q.emesso),
                if q.conguaglio() >= 0 { "a debito" } else { "a credito" },
                n(q.conguaglio()),
                n(q.versato),
                n(q.da_versare())
            ));
        }
        self.pagina_html(cond, &format!("Rendiconto individuale – esercizio {}", nome_es), &b)
    }

    pub fn html_estratto_conto(&self, cond: i64, persona: i64) -> Result<String> {
        let p: String = self.conn.query_row("SELECT trim(cognome||' '||nome) FROM persona WHERE id=?1", [persona], |r| r.get(0))?;
        let mut b = format!("<p>Condòmino: <b>{}</b></p><table><tr><th>Data</th><th>Descrizione</th><th class=\"n\">Dare</th><th class=\"n\">Avere</th><th class=\"n\">Saldo</th></tr>", esc(&p));
        for m in self.estratto_conto(cond, persona)? {
            b.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td></tr>",
                data_it(&m.data),
                esc(&m.descrizione),
                if m.dare != 0 { n(m.dare) } else { String::new() },
                if m.avere != 0 { n(m.avere) } else { String::new() },
                n(m.saldo)
            ));
        }
        b.push_str("</table>");
        let sit = self.situazione_persona(cond, persona)?;
        b.push_str(&format!("<p>Salvadanaio (credito disponibile): <b>{}</b></p>", n(sit.salvadanaio)));
        self.pagina_html(cond, "Estratto conto condòmino", &b)
    }

    pub fn html_scadenzario(&self, cond: i64, oggi: chrono::NaiveDate, fino_a: chrono::NaiveDate) -> Result<String> {
        let mut b = format!("<p>Dal {} al {}</p><table><tr><th>Data</th><th>Tipo</th><th>Scadenza</th><th>Dettaglio</th><th class=\"n\">Importo</th></tr>", data_it(&oggi.format("%Y-%m-%d").to_string()), data_it(&fino_a.format("%Y-%m-%d").to_string()));
        for s in self.scadenzario(cond, oggi, fino_a)? {
            b.push_str(&format!(
                "<tr><td>{}{}</td><td>{}</td><td>{}</td><td>{}</td><td class=\"n\">{}</td></tr>",
                data_it(&s.data),
                if s.scaduta { " <b class=\"neg\">(scaduta)</b>" } else { "" },
                s.tipo,
                esc(&s.titolo),
                esc(&s.dettaglio),
                s.importo.map(n).unwrap_or_default()
            ));
        }
        b.push_str("</table>");
        self.pagina_html(cond, "Scadenzario", &b)
    }

    pub fn html_morosita(&self, cond: i64, oggi: chrono::NaiveDate) -> Result<String> {
        let mut b = String::from("<table><tr><th>Condòmino</th><th>Piano</th><th>Scadenza</th><th class=\"n\">Ritardo (gg)</th><th class=\"n\">Residuo</th><th class=\"n\">Interessi</th></tr>");
        let (mut tr, mut ti) = (0, 0);
        for r in self.rate_scadute(cond, oggi, None)? {
            b.push_str(&format!(
                "<tr><td>{}</td><td>{} n.{}</td><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td></tr>",
                esc(&r.persona), esc(&r.piano), r.numero, data_it(&r.scadenza), r.giorni_ritardo, n(r.residuo), n(r.interessi)
            ));
            tr += r.residuo;
            ti += r.interessi;
        }
        b.push_str(&format!("<tr class=\"tot\"><td colspan=\"4\">Totale</td><td class=\"n\">{}</td><td class=\"n\">{}</td></tr></table>", n(tr), n(ti)));
        b.push_str(&format!("<p class=\"nota\">Situazione al {}. Interessi calcolati al tasso legale annuo per anno, sulle date effettive degli incassi.</p>", data_it(&oggi.format("%Y-%m-%d").to_string())));
        self.pagina_html(cond, "Situazione morosità", &b)
    }

    pub fn html_cu(&self, cond: i64, anno: i32) -> Result<String> {
        let mut b = format!("<p>Anno d'imposta <b>{}</b> – criterio di cassa (data di pagamento)</p><table><tr><th>Percipiente</th><th>CF / P.IVA</th><th>Cod. tributo</th><th class=\"n\">Corrisposto</th><th class=\"n\">Ritenuta operata</th><th class=\"n\">Ritenuta versata</th></tr>", anno);
        for r in self.riepilogo_cu(cond, anno)? {
            let ires: i64 = self.conn.query_row("SELECT soggetto_ires FROM fornitore WHERE id=?1", [r.fornitore_id], |x| x.get(0))?;
            b.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td></tr>",
                esc(&r.ragione_sociale), esc(&r.cf_piva), crate::fiscale::codice_tributo(&r.regime, ires != 0), n(r.imponibile), n(r.ritenuta_operata), n(r.ritenuta_versata)
            ));
        }
        b.push_str("</table><h2>Prospetto per mese (770 semplificato)</h2><table><tr><th>Mese</th><th>Cod. tributo</th><th class=\"n\">Operate</th><th class=\"n\">Versate</th></tr>");
        for r in self.riepilogo_770(cond, anno)? {
            b.push_str(&format!("<tr><td>{:02}/{}</td><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td></tr>", r.mese, anno, r.codice_tributo, n(r.operate), n(r.versate)));
        }
        b.push_str("</table><p class=\"nota\">Riepilogo di supporto alla compilazione: la CU e il 770 vanno predisposti e trasmessi con i software e i canali dell'Agenzia delle Entrate.</p>");
        self.pagina_html(cond, &format!("Ritenute d'acconto {} – riepilogo CU / 770", anno), &b)
    }
}
