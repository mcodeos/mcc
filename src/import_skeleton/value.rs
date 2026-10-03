// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Conservative `comment` -> builtin-arg parser for `import_skeleton`.
//!
//! Inline passives (`::CAP/::RES/::IND`) are emitted **only** when every token
//! in the comment parses into a known slot (value / tolerance / dielectric /
//! voltage) or is an ignorable package code. Anything else — a token we cannot
//! classify, an empty comment, a split part number — returns `None` and the
//! renderer falls back to a component block plus a `TODO(import)` stub.
//! Rather-missing-than-wrong: a wrong nominal value poisons every downstream
//! check, a TODO poisons nothing.

/// One parsed inline builtin: the class (`CAP`/`RES`/`IND`) and the pre-joined
/// argument list (already in verified argument order).
pub struct Inline {
    pub class: &'static str,
    pub args: String,
}

/// Class from the mct `libref` (display text). Whitelist only — `CONN`, `CN`,
/// `TVS` and friends must not sniff their way into an inline builtin.
fn class_of(libref: &str) -> Option<&'static str> {
    let seg = libref.split('.').next().unwrap_or("").trim().to_ascii_uppercase();
    // strip library suffixes like CAPACITOR_0 / CAP NP -> first token
    let head: String = seg
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    match head.as_str() {
        "CAP" | "CAPACITOR" => Some("CAP"),
        "RES" | "RESISTOR" => Some("RES"),
        "IND" | "INDUCTOR" | "COIL" => Some("IND"),
        _ => None,
    }
}

/// Parse `comment` into inline builtin args, or `None` (→ block + TODO).
pub fn parse(libref: &str, comment: &str) -> Option<Inline> {
    let class = class_of(libref)?;
    let comment = comment.trim();
    if comment.is_empty() {
        return None;
    }

    let mut value: Option<String> = None;
    let mut tol: Option<String> = None;
    let mut diel: Option<String> = None;
    let mut volt: Option<String> = None;

    for tok in comment.split(|c: char| c == ',' || c == '/' || c.is_whitespace()) {
        let tok = tok.trim();
        if tok.is_empty() {
            continue;
        }
        let up = tok.to_ascii_uppercase();
        if let Some(t) = tolerance(&up) {
            tol = Some(t);
        } else if let Some(v) = voltage(&up) {
            volt = Some(v);
        } else if let Some(d) = dielectric(&up) {
            diel = Some(d);
        } else if ignore(&up) {
            continue;
        } else if value.is_none() {
            let parsed = match class {
                "RES" => resistance(&up),
                "CAP" => capacitance(&up),
                _ => inductance(&up),
            };
            // first unparsable value-ish token → the whole comment is
            // beyond high-confidence; do not guess around it
            value = Some(parsed?);
        } else {
            return None;
        }
    }

    let value = value?;
    let mut args = vec![value];
    // argument order mirrors verified usage: `RES(22Ω, ±1%)`,
    // `CAP(100nF, ±10%, CAP.X5R, 25V)`, `CAP(330uF, 25V)` — never invent a
    // middle slot
    if class == "CAP" {
        if let Some(t) = &tol {
            args.push(format!("±{t}%"));
        }
        if let Some(d) = &diel {
            args.push(format!("CAP.{d}"));
        }
    }
    if let Some(v) = &volt {
        args.push(format!("{v}V"));
    } else if class != "CAP" {
        if let Some(t) = &tol {
            args.push(format!("±{t}%"));
        }
    }
    Some(Inline { class, args: args.join(", ") })
}

/// `±1%` / `1%` -> `1`
fn tolerance(tok: &str) -> Option<String> {
    tok.strip_prefix('±')
        .unwrap_or(tok)
        .strip_suffix('%')
        .filter(|t| t.chars().all(|c| c.is_ascii_digit() || c == '.'))
        .map(str::to_string)
}

/// `25V` / `6.3V` -> `6.3` (kept as its own slot; renderer re-appends `V`)
fn voltage(tok: &str) -> Option<String> {
    tok.strip_suffix('V').filter(|v| {
        !v.is_empty() && v.chars().all(|c| c.is_ascii_digit() || c == '.')
    })?.parse::<f64>().ok().map(|_| tok[..tok.len() - 1].to_string())
}

/// `X7R` / `X5R` / `C0G` / `NP0` / `Y5V` / `Z5U`
fn dielectric(tok: &str) -> Option<String> {
    match tok {
        "X7R" | "X5R" | "X6S" | "X7S" | "C0G" | "NP0" | "Y5V" | "Z5U" | "X8R" => {
            Some(tok.replace("NP0", "C0G"))
        }
        _ => None,
    }
}

/// Package / mounting codes that carry no electrical value.
fn ignore(tok: &str) -> bool {
    matches!(tok, "0201" | "0402" | "0603" | "0805" | "1206" | "1210" | "2010" | "2512" | "01005")
        || tok == "SMD"
        || tok == "SMT"
}

/// Resistance: `10K` `10kΩ` `22R` `4K7` `0R5` `1M` `100` `5mΩ`. Returns the
/// normalized `{n}{unit}` form with Ω units.
fn resistance(tok: &str) -> Option<String> {
    let tok = tok.strip_suffix("OHM").unwrap_or(tok);
    let tok = tok.strip_suffix('Ω').unwrap_or(tok);
    if tok.is_empty() {
        return None;
    }
    // letter position decides the shape: trailing = multiplier (`10K`),
    // middle = decimal point (`4K7`, `0R5`)
    let letter_idx = tok
        .char_indices()
        .find_map(|(i, c)| matches!(c, 'K' | 'k' | 'M' | 'm' | 'R').then_some((i, c)));
    let int_part: String;
    let frac: Option<String>;
    let mult: f64;
    let unit: &str;
    match letter_idx {
        Some((i, c)) if i + 1 == tok.len() => {
            int_part = tok[..i].to_string();
            frac = None;
            match c {
                'K' | 'k' => {
                    mult = 1e3;
                    unit = "kΩ";
                }
                'M' => {
                    mult = 1e6;
                    unit = "MΩ";
                }
                'm' => {
                    mult = 1e-3;
                    unit = "mΩ";
                }
                _ => {
                    mult = 1.0;
                    unit = "Ω";
                }
            }
        }
        Some((i, c)) if c != 'm' && tok.len() > 1 => {
            // `4K7` / `0R5` infix decimal point (a prefix `m` is milliohms,
            // never a decimal point)
            int_part = tok[..i].to_string();
            let f = tok[i + 1..].to_string();
            if f.is_empty() || !f.chars().all(|ch: char| ch.is_ascii_digit()) {
                return None;
            }
            frac = Some(f);
            match c {
                'K' | 'k' => {
                    mult = 1e3;
                    unit = "kΩ";
                }
                'M' => {
                    mult = 1e6;
                    unit = "MΩ";
                }
                _ => {
                    mult = 1.0;
                    unit = "Ω";
                }
            }
        }
        Some(_) => return None,
        None => {
            int_part = tok.to_string();
            frac = None;
            mult = 1.0;
            unit = "Ω";
        }
    }
    if !int_part.chars().all(|ch: char| ch.is_ascii_digit() || ch == '.') || int_part.is_empty() {
        return None;
    }
    if let Some(f) = &frac {
        if !f.chars().all(|ch: char| ch.is_ascii_digit()) || f.is_empty() {
            return None;
        }
    }
    let v = format!("{int_part}{}", frac.map(|f| format!(".{f}")).unwrap_or_default())
        .parse::<f64>()
        .ok()?;
    let _ = unit; // display re-normalizes from the ohm value; multipliers only multiply
    let ohms = v * mult;
    Some(if ohms >= 1e6 {
        format!("{}MΩ", trim_num(ohms / 1e6))
    } else if ohms >= 1e3 {
        format!("{}kΩ", trim_num(ohms / 1e3))
    } else if ohms < 1.0 {
        format!("{}mΩ", trim_num(ohms * 1e3))
    } else {
        format!("{}Ω", trim_num(ohms))
    })
}

/// Capacitance: `100nF` `10uF` `4u7` `104` (three-digit code, pF base).
fn capacitance(tok: &str) -> Option<String> {
    let lower = tok.to_ascii_lowercase();
    if let Some(v) = with_unit(&lower, "pf", 1e-12)
        .or_else(|| with_unit(&lower, "nf", 1e-9))
        .or_else(|| with_unit(&lower, "uf", 1e-6))
        .or_else(|| with_unit(&lower, "mf", 1e-3))
    {
        return Some(fmt_farad(v));
    }
    // `4u7` / `4n7` letter-as-decimal-point forms
    let b: Vec<char> = lower.chars().collect();
    if b.len() >= 2 && b[1] == 'u' {
        return parse_two_part(&b, 1e-6).map(fmt_farad);
    }
    if b.len() >= 2 && b[1] == 'n' {
        return parse_two_part(&b, 1e-9).map(fmt_farad);
    }
    if b.len() >= 2 && b[1] == 'p' {
        return parse_two_part(&b, 1e-12).map(fmt_farad);
    }
    // three-digit code: `104` = 10×10⁴ pF = 100nF
    if b.len() == 3 && b.iter().all(|c| c.is_ascii_digit()) {
        let sig: f64 = format!("{}{}", b[0], b[1]).parse().ok()?;
        let exp: i32 = (b[2] as u8 - b'0') as i32;
        return Some(fmt_farad(sig * 10f64.powi(exp) * 1e-12));
    }
    None
}

/// Inductance: `10uH` `4R7` `2.2mH` `10UH`.
fn inductance(tok: &str) -> Option<String> {
    let lower = tok.to_ascii_lowercase();
    if let Some(v) = with_unit(&lower, "nh", 1e-9)
        .or_else(|| with_unit(&lower, "uh", 1e-6))
        .or_else(|| with_unit(&lower, "mh", 1e-3))
        .or_else(|| with_unit(&lower, "h", 1.0))
    {
        return Some(fmt_henry(v));
    }
    let b: Vec<char> = lower.chars().collect();
    if b.len() >= 2 && (b[1] == 'u' || b[1] == 'r') {
        return parse_two_part(&b, 1e-6).map(fmt_henry);
    }
    None
}

/// `4u7` shape: digit, letter at `split`, optional trailing digits.
fn parse_two_part(chars: &[char], mult: f64) -> Option<f64> {
    let int_part: String = chars[..1].iter().collect();
    let frac: String = chars[2..].iter().collect();
    let v = format!("{int_part}{}", if frac.is_empty() { String::new() } else { format!(".{frac}") })
        .parse::<f64>()
        .ok()?;
    Some(v * mult)
}

fn with_unit(tok: &str, unit: &str, mult: f64) -> Option<f64> {
    let num = tok.strip_suffix(unit)?;
    if num.is_empty() || !num.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    num.parse::<f64>().ok().map(|v| v * mult)
}

/// Farad formatting: pick nF/uF so real-world comments round-trip readably.
fn fmt_farad(v: f64) -> String {
    if v >= 1e-6 {
        format!("{}uF", trim_num(v / 1e-6))
    } else if v >= 1e-9 {
        format!("{}nF", trim_num(v / 1e-9))
    } else {
        format!("{}pF", trim_num(v / 1e-12))
    }
}

fn fmt_henry(v: f64) -> String {
    if v >= 1e-6 {
        format!("{}uH", trim_num(v / 1e-6))
    } else if v >= 1e-3 {
        format!("{}mH", trim_num(v / 1e-3))
    } else {
        format!("{}nH", trim_num(v / 1e-9))
    }
}

/// `10.0` -> `10`, `4.70` -> `4.7` (keep user-visible digits honest).
fn trim_num(v: f64) -> String {
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn g18_comments_parse() {
        for (libref, comment, want) in [
            ("Cap 0402", "1uF", "CAP(1uF)"),
            ("Cap 0402", "100nF", "CAP(100nF)"),
            ("Cap 0402", "4.7uF", "CAP(4.7uF)"),
            ("Cap 0402", "15pF 5%", "CAP(15pF, ±5%)"),
            ("RES 0402", "1k", "RES(1kΩ)"),
            ("RES 0402", "300R", "RES(300Ω)"),
            ("RES 0402", "100k 1%", "RES(100kΩ, ±1%)"),
            ("RES 0402", "4K7", "RES(4.7kΩ)"),
        ] {
            let got = parse(libref, comment);
            assert!(got.is_some(), "{libref} '{comment}' should parse");
            let g = got.unwrap();
            assert_eq!(format!("{}({})", g.class, g.args), want, "{libref} '{comment}'");
        }
        // Conservative face: a non-whitelisted / unclassifiable token is None
        assert!(parse("8P4R", "8P4R104").is_none());
        assert!(parse("Cap 0402", "NC").is_none());
        assert!(parse("Cap 0402", "").is_none());
        assert!(parse("XT1861", "XT1861").is_none());
    }
}
