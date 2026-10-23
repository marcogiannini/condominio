//! Utilità: importi in centesimi, ripartizione "penny-perfect", date italiane.

use chrono::{Datelike, Local, Months, NaiveDate};

/// Importi sempre in centesimi interi (mai f64 per i soldi).
pub type Cents = i64;

/// 1000 millesimi = 1_000_000 micro-millesimi (3 decimali di precisione).
pub const MICRO: i64 = 1000;

pub fn format_eur(c: Cents) -> String {
    let neg = c < 0;
    let a = c.unsigned_abs();
    let int = a / 100;
    let dec = a % 100;
    let s = int.to_string();
    let mut grouped = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            grouped.push('.');
        }
        grouped.push(ch);
    }
    format!("{}{},{:02} €", if neg { "-" } else { "" }, grouped, dec)
}

/// Interpreta "1.234,56", "1234,56", "1234.56", "12" ecc. in centesimi.
pub fn parse_eur(s: &str) -> Option<Cents> {
    let mut t: String = s.trim().replace(['€', ' '], "");
    if t.is_empty() {
        return None;
    }
    let neg = t.starts_with('-');
    if neg {
        t.remove(0);
    }
    let norm = if t.contains(',') {
        t.replace('.', "").replace(',', ".")
    } else if t.matches('.').count() == 1 {
        let after = t.rsplit('.').next().unwrap_or("");
        if after.len() <= 2 {
            t.clone()
        } else {
            t.replace('.', "")
        }
    } else {
        t.replace('.', "")
    };
    let mut parts = norm.splitn(2, '.');
    let int_s = parts.next()?;
    let frac_s = parts.next().unwrap_or("");
    if int_s.is_empty() && frac_s.is_empty() {
        return None;
    }
    if !int_s.chars().all(|c| c.is_ascii_digit()) || !frac_s.chars().all(|c| c.is_ascii_digit()) || frac_s.len() > 2 {
        return None;
    }
    let int: i64 = if int_s.is_empty() { 0 } else { int_s.parse().ok()? };
    let mut frac: i64 = if frac_s.is_empty() { 0 } else { frac_s.parse().ok()? };
    if frac_s.len() == 1 {
        frac *= 10;
    }
    let v = int.checked_mul(100)?.checked_add(frac)?;
    Some(if neg { -v } else { v })
}

/// Numero decimale con `dec` cifre (es. dec=3: "12,345" -> 12345).
pub fn parse_fixed(s: &str, dec: usize) -> Option<i64> {
    let t = s.trim().replace(',', ".");
    if t.is_empty() {
        return None;
    }
    let mut parts = t.splitn(2, '.');
    let int_s = parts.next()?;
    let frac_s = parts.next().unwrap_or("");
    if !int_s.chars().all(|c| c.is_ascii_digit()) || !frac_s.chars().all(|c| c.is_ascii_digit()) || frac_s.len() > dec {
        return None;
    }
    if int_s.is_empty() && frac_s.is_empty() {
        return None;
    }
    let int: i64 = if int_s.is_empty() { 0 } else { int_s.parse().ok()? };
    let mut frac: i64 = if frac_s.is_empty() { 0 } else { frac_s.parse().ok()? };
    for _ in frac_s.len()..dec {
        frac *= 10;
    }
    int.checked_mul(10i64.pow(dec as u32))?.checked_add(frac)
}

/// Millesimi ("12,345") -> micro (12345).
pub fn parse_millesimi(s: &str) -> Option<i64> {
    parse_fixed(s, 3)
}

/// Percentuale ("60" o "33,33") -> parti per milione (1_000_000 = 100%).
pub fn parse_percentuale_ppm(s: &str) -> Option<i64> {
    parse_fixed(s, 4)
}

pub fn format_millesimi(micro: i64) -> String {
    format!("{},{:03}", micro / MICRO, (micro % MICRO).abs())
}

/// Ripartisce `totale` in proporzione ai `pesi` col metodo del resto maggiore:
/// la somma delle parti è SEMPRE esattamente `totale` (nessun centesimo perso).
pub fn ripartisci(totale: Cents, pesi: &[i64]) -> Vec<Cents> {
    let n = pesi.len();
    if n == 0 {
        return vec![];
    }
    let sign: i64 = if totale < 0 { -1 } else { 1 };
    let t = totale.unsigned_abs() as i128;
    let w: Vec<i128> = pesi.iter().map(|&p| p.max(0) as i128).collect();
    let mut sum: i128 = w.iter().sum();
    let w = if sum == 0 {
        sum = n as i128;
        vec![1i128; n]
    } else {
        w
    };
    let mut out = vec![0i128; n];
    let mut resti: Vec<(i128, usize)> = Vec::with_capacity(n);
    let mut assegnato: i128 = 0;
    for i in 0..n {
        let prod = t * w[i];
        out[i] = prod / sum;
        assegnato += out[i];
        resti.push((prod % sum, i));
    }
    let mancanti = (t - assegnato) as usize;
    resti.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    for k in 0..mancanti {
        out[resti[k].1] += 1;
    }
    out.into_iter().map(|v| (v as i64) * sign).collect()
}

/// Percentuale (in punti base, 400 = 4%) con arrotondamento commerciale.
pub fn percentuale(importo: Cents, punti_base: i64) -> Cents {
    let v = importo as i128 * punti_base as i128;
    let r = if v >= 0 { (v + 5000) / 10000 } else { (v - 5000) / 10000 };
    r as i64
}

// ------------------------------------------------------------------ date

pub fn oggi() -> NaiveDate {
    Local::now().date_naive()
}

pub fn oggi_iso() -> String {
    oggi().format("%Y-%m-%d").to_string()
}

pub fn parse_data(s: &str) -> Option<NaiveDate> {
    let t = s.trim();
    NaiveDate::parse_from_str(t, "%d/%m/%Y")
        .or_else(|_| NaiveDate::parse_from_str(t, "%Y-%m-%d"))
        .ok()
}

/// Da testo utente ("GG/MM/AAAA") a ISO "AAAA-MM-GG".
pub fn data_iso(s: &str) -> Option<String> {
    parse_data(s).map(|d| d.format("%Y-%m-%d").to_string())
}

/// Da ISO a "GG/MM/AAAA" (se non parsabile restituisce l'input).
pub fn data_it(iso: &str) -> String {
    match NaiveDate::parse_from_str(iso, "%Y-%m-%d") {
        Ok(d) => d.format("%d/%m/%Y").to_string(),
        Err(_) => iso.to_string(),
    }
}

pub fn aggiungi_mesi(d: NaiveDate, mesi: u32) -> NaiveDate {
    d.checked_add_months(Months::new(mesi)).unwrap_or(d)
}

pub fn aggiungi_anni(d: NaiveDate, anni: u32) -> NaiveDate {
    aggiungi_mesi(d, anni * 12)
}

/// Il giorno `giorno` del mese successivo a `d`.
pub fn giorno_mese_successivo(d: NaiveDate, giorno: u32) -> NaiveDate {
    let primo = NaiveDate::from_ymd_opt(d.year(), d.month(), 1).unwrap_or(d);
    let succ = aggiungi_mesi(primo, 1);
    NaiveDate::from_ymd_opt(succ.year(), succ.month(), giorno).unwrap_or(succ)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formato_importi() {
        assert_eq!(format_eur(123456), "1.234,56 €");
        assert_eq!(format_eur(-5), "-0,05 €");
        assert_eq!(format_eur(100000000), "1.000.000,00 €");
    }

    #[test]
    fn parsing_importi() {
        assert_eq!(parse_eur("1.234,56"), Some(123456));
        assert_eq!(parse_eur("1234,5"), Some(123450));
        assert_eq!(parse_eur("1234.56"), Some(123456));
        assert_eq!(parse_eur("1.234"), Some(123400));
        assert_eq!(parse_eur("12"), Some(1200));
        assert_eq!(parse_eur("-3,10"), Some(-310));
        assert_eq!(parse_eur("abc"), None);
        assert_eq!(parse_eur("1,234"), None);
    }

    #[test]
    fn millesimi() {
        assert_eq!(parse_millesimi("12,345"), Some(12345));
        assert_eq!(parse_millesimi("100"), Some(100000));
        assert_eq!(format_millesimi(12345), "12,345");
    }

    #[test]
    fn ripartizione_penny_perfect() {
        let v = ripartisci(10000, &[333333, 333333, 333334]);
        assert_eq!(v.iter().sum::<i64>(), 10000);
        let v = ripartisci(1, &[1, 1, 1]);
        assert_eq!(v, vec![1, 0, 0]);
        let v = ripartisci(-100, &[1, 1, 1]);
        assert_eq!(v.iter().sum::<i64>(), -100);
        let v = ripartisci(500, &[0, 0]);
        assert_eq!(v, vec![250, 250]);
    }

    #[test]
    fn percentuali() {
        assert_eq!(percentuale(100000, 400), 4000);
        assert_eq!(percentuale(12345, 400), 494); // 493,8 -> 494
    }
}
