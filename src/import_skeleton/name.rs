// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Identifier legalization for generated skeletons (`import_skeleton`).
//!
//! mct net names and refdes come from foreign EDA tools and are routinely
//! *not* mcode idents (`N$1`, `+3.3V`, `SDA-1`). Every rewrite is reported by
//! [`Legalizer::net`] as `(ident, Some(original))` so the renderer can pin a
//! `// was: X` comment on the line — a silent rename would be a correctness
//! hazard, not a cosmetic one.

/// Tokens that would parse as grammar keywords (or shadow the top module) if a
/// net or instance took them as a name. Collisions get an `n_`/`u_` prefix.
const RESERVED: &[&str] = &[
    "module", "component", "interface", "impl", "func", "in", "out", "io", "inout", "pins",
    "partno", "package", "domain", "rail", "psrc", "psnk", "bom", "use", "import", "export",
    "expects", "sim", "check", "param", "type", "enum", "let", "if", "else", "for", "while",
    "return", "true", "false", "main", "self", "as", "when", "assert",
];

/// Allocates unique mcode idents for nets and instances. Rewrites are
/// reported at allocation time (`net`/`instance` return `(ident, Some(orig))`)
/// so the renderer can annotate the line.
#[derive(Default)]
pub struct Legalizer {
    taken: std::collections::BTreeSet<String>,
}

impl Legalizer {
    /// Legalize a net name; the result never collides with a previously
    /// allocated ident (suffix `_2`, `_3`, ... disambiguates).
    pub fn net(&mut self, raw: &str) -> (String, Option<String>) {
        let base = sanitize(raw, "n_", 'V', RESERVED);
        let ident = self.allocate(&base);
        let renamed = (ident != raw).then(|| raw.to_string());
        (ident, renamed)
    }

    /// Legalize an instance designator (component refdes). Instances allocate
    /// first, so nets see them via [`Legalizer::reserve_block`].
    pub fn instance(&mut self, raw: &str) -> (String, Option<String>) {
        let base = sanitize(raw, "u_", 'U', RESERVED);
        let ident = self.allocate(&base);
        let renamed = (ident != raw).then(|| raw.to_string());
        (ident, renamed)
    }

    /// Block a set of idents from future allocation (instances call this once
    /// up front so a net named like a refdes cannot shadow it).
    pub fn reserve_block(&mut self, idents: impl IntoIterator<Item = String>) {
        self.taken.extend(idents);
    }

    fn allocate(&mut self, base: &str) -> String {
        let mut candidate = base.to_string();
        let mut n = 2;
        while self.taken.contains(&candidate) {
            candidate = format!("{base}_{n}");
            n += 1;
        }
        self.taken.insert(candidate.clone());
        candidate
    }
}

/// Fold `raw` into a legal mcode ident: `N$1` -> `N_1`, `5V` -> `V5V` (a
/// leading digit gets `digit_prefix` — nets read as voltages, `V5V` not
/// `N5V`), every non `[A-Za-z0-9_]` collapses to `_`.
fn sanitize(raw: &str, prefix: &str, digit_prefix: char, reserved: &[&str]) -> String {
    let mut s: String = raw
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect();
    // collapse repeats and trim so `SDA--1` -> `SDA_1`, `-X` -> `X`
    let mut folded = String::new();
    for c in s.chars() {
        if c == '_' && folded.ends_with('_') {
            continue;
        }
        folded.push(c);
    }
    s = folded.trim_matches('_').to_string();
    if s.is_empty() {
        s = prefix.trim_end_matches('_').to_string();
    }
    if s.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        // `5V` -> `V5V`: a digit-leading token would parse as a number
        s = format!("{digit_prefix}{s}");
    } else if reserved.contains(&s.to_ascii_lowercase().as_str()) {
        s = format!("{}{}", prefix, s);
    }
    s
}

/// Sanitize a pin name placeholder (`p` + pin text); empty pins fall back to
/// the 1-based position, and duplicates inside one component get a suffix.
pub struct PinNames {
    names: Vec<String>,
}

impl PinNames {
    pub fn build(pins: &[String]) -> PinNames {
        let mut names = Vec::with_capacity(pins.len());
        let mut seen = std::collections::BTreeSet::new();
        for (i, raw) in pins.iter().enumerate() {
            let base = if raw.is_empty() {
                format!("p{}", i + 1)
            } else {
                let mut b = sanitize(raw, "p_", 'P', &[]);
                if b.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                    b = format!("p{b}");
                }
                b
            };
            let mut candidate = base.clone();
            let mut n = 2;
            while candidate.is_empty() || !seen.insert(candidate.clone()) {
                candidate = format!("{base}_{n}");
                n += 1;
            }
            names.push(candidate);
        }
        PinNames { names }
    }

    /// The placeholder for the pin at `i` (`p1`, `p2`, ... shape).
    pub fn get(&self, i: usize) -> &str {
        self.names.get(i).map(String::as_str).unwrap_or("p?")
    }
}

/// Sanitize one class-name segment (keeps `.` segments apart; the caller joins).
pub fn class_segment(raw: &str) -> String {
    sanitize(raw, "c_", 'C', &[])
}
