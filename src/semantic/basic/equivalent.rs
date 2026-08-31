// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Pipeline ① — member_set geometry (§11.3, architecture doc).
//!
//! A name's *member set* is the ordered list of concrete member names the
//! spelling denotes, in strict declaration (writing) order — never sorted.
//! All of `[k]` / `{m}` / `.m` spellings of the same physical member set must
//! expand to the same ordered `Vec<String>`; that set is the currency the
//! parsing/resolution layers already use as their string keys.
//!
//! The vector guard (`expanded.len() >= 2`, contract E) is NOT applied here —
//! `member_set` returns the full set (a single member is a set of one); callers
//! decide vector-ness. `canonical_single` is the read-side fallback that only
//! accepts exactly one bare identifier.

use super::mc_ids::McIds;

/// Expand the segment tree of `ids` to its ordered member set.
///
/// Backed by `McIds::expand` / `McIda::expand`, which already handle single
/// indices (`c[1:2]` -> `["c1","c2"]`), embedded member slices
/// (`XTAL.X[1:2]` -> `["XTAL.X1","XTAL.X2"]`), nested combination
/// (`S[1:4][L,R]` -> 8 leaves, Cartesian row-major), comma curly groups and
/// escapes. Returns `None` only for an empty expansion.
pub(crate) fn member_set(ids: &McIds) -> Option<Vec<String>> {
    let expanded = ids.expand();
    if expanded.is_empty() {
        return None;
    }
    Some(expanded)
}

/// Read-side canonical fallback (§2.1): the single bare-identifier member, if
/// the member set has exactly one member AND that member is a bare identifier.
/// Dotted / bracketed / curly members return `None` (conservative bound).
pub(crate) fn canonical_single(ids: &McIds) -> Option<String> {
    let set = member_set(ids)?;
    if set.len() != 1 {
        return None;
    }
    let s = &set[0];
    if s.contains(['.', '[', ']', '{', '}', '|', ',']) {
        return None;
    }
    Some(s.clone())
}

/// Two spellings are equivalent iff they denote the same ordered member set.
pub(crate) fn are_equivalent(a: &McIds, b: &McIds) -> bool {
    match (member_set(a), member_set(b)) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

/// String front-end: parse `display` to its ordered member set.
///
/// The string port has no AST; it parses the same spelling grammar that the
/// segment tree captures (squares, escapes, dotted chains) and additionally
/// handles curly groups (`{A|B}`, `{A,B|C}`) with `,` and `|` separators that
/// the segment tree only sees structurally from the AST. This is the member-set
/// counterpart of the deferred `parse_display`/`segmentize` (§8.1) — it shares
/// the square/escape parsing core (`McIda::parse`) but is a standalone helper,
/// not a parallel parser of the McIds shape.
pub(crate) fn member_set_from_str(display: &str) -> Option<Vec<String>> {
    let ids = McIds::from(display);
    let mut expanded = member_set(&ids)?;

    // Curly groups the single-Ida string form could not structurally parse
    // (`Q1{S|D}` arrives as one Ida "Q1{S|D}" because `McIda::parse` only
    // handles squares): split residual `{...}` groups on `,` and `|`.
    expanded = split_curly_groups(expanded);

    if expanded.is_empty() {
        None
    } else {
        Some(expanded)
    }
}

/// Split any residual `{...}` curly group in expanded member names on `,` and
/// `|` separators, expanding each member with the prefix in declaration order.
fn split_curly_groups(expanded: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in expanded {
        if let Some(open) = name.find('{') {
            if let Some(close) = name.rfind('}') {
                if close > open {
                    let prefix = &name[..open];
                    let body = &name[open + 1..close];
                    let suffix = &name[close + 1..];
                    let members: Vec<&str> = body
                        .split([',', '|'])
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .collect();
                    for m in &members {
                        out.push(format!("{prefix}.{m}{suffix}"));
                    }
                    // A curly group consumed the whole name — nothing else to do.
                    continue;
                }
            }
        }
        out.push(name);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn member_set_single_index_vector() {
        assert_eq!(
            member_set(&McIds::from("c[1:2]")),
            Some(vec!["c1".into(), "c2".into()])
        );
    }

    #[test]
    fn member_set_single_member_is_set_of_one() {
        assert_eq!(
            member_set(&McIds::from("res[4]")),
            Some(vec!["res4".into()])
        );
    }

    #[test]
    fn member_set_embedded_member_slice() {
        // `XTAL.X[1:2]` — member after a dotted prefix.
        assert_eq!(
            member_set(&McIds::from("XTAL.X[1:2]")),
            Some(vec!["XTAL.X1".into(), "XTAL.X2".into()])
        );
    }

    #[test]
    fn member_set_matrix_row_major() {
        // Nested combination: outer index first (row-major, §11.2).
        assert_eq!(
            member_set(&McIds::from("S[1:2][L,R]")),
            Some(vec!["S1L".into(), "S1R".into(), "S2L".into(), "S2R".into()])
        );
    }

    #[test]
    fn member_set_plain_name_is_singleton() {
        assert_eq!(member_set(&McIds::from("gnd")), Some(vec!["gnd".into()]));
    }

    #[test]
    fn member_set_from_str_curly_pipe() {
        assert_eq!(
            member_set_from_str("Q1{S|D}"),
            Some(vec!["Q1.S".into(), "Q1.D".into()])
        );
    }

    #[test]
    fn member_set_from_str_curly_mixed_separators() {
        assert_eq!(
            member_set_from_str("X{SPI,MIC|DAC_OUT}"),
            Some(vec!["X.SPI".into(), "X.MIC".into(), "X.DAC_OUT".into()])
        );
    }

    #[test]
    fn member_set_from_str_escaped_member() {
        assert_eq!(
            member_set_from_str("usbsock.USB.D\\+"),
            Some(vec!["usbsock.USB.D+".into()])
        );
    }

    #[test]
    fn canonical_single_bare_only() {
        assert_eq!(canonical_single(&McIds::from("gnd")), Some("gnd".into()));
        // Dotted member is not a bare identifier.
        assert_eq!(canonical_single(&McIds::from("A.m")), None);
        // Multi-member is not canonical-single.
        assert_eq!(canonical_single(&McIds::from("c[1:2]")), None);
    }

    #[test]
    fn are_equivalent_compares_member_sets() {
        assert!(are_equivalent(&McIds::from("c[1:2]"), &McIds::from("c1,c2")) == false);
        assert!(are_equivalent(&McIds::from("res[4]"), &McIds::from("res4")));
    }
}
