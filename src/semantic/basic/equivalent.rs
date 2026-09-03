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

use super::mc_ids::{parse_display, McIds};

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
/// The string port has no AST; it goes through the shared text entry
/// [`parse_display`] (mc_ids.rs), which builds the same segment tree the AST
/// front end produces — squares/escapes/dots via `McIda`, curly groups
/// (`{A|B}`, `{A,B|C}`, numeric slices `{1:3}`) as structural `Curly`
/// segments — and then expands it with `member_set`. This is the P3 port of
/// the pipeline's single `parse_display` entry; callers never split the
/// string themselves.
pub(crate) fn member_set_from_str(display: &str) -> Option<Vec<String>> {
    let ids = parse_display(display);
    let expanded = member_set(&ids)?;
    if expanded.is_empty() {
        None
    } else {
        Some(expanded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sem_equiv__member_set_single_index_vector() {
        assert_eq!(
            member_set(&McIds::from("c[1:2]")),
            Some(vec!["c1".into(), "c2".into()])
        );
    }

    #[test]
    fn sem_equiv__member_set_single_member_is_set_of_one() {
        assert_eq!(
            member_set(&McIds::from("res[4]")),
            Some(vec!["res4".into()])
        );
    }

    #[test]
    fn sem_equiv__member_set_embedded_member_slice() {
        // `XTAL.X[1:2]` — member after a dotted prefix.
        assert_eq!(
            member_set(&McIds::from("XTAL.X[1:2]")),
            Some(vec!["XTAL.X1".into(), "XTAL.X2".into()])
        );
    }

    #[test]
    fn sem_equiv__member_set_matrix_row_major() {
        // Nested combination: outer index first (row-major, §11.2).
        assert_eq!(
            member_set(&McIds::from("S[1:2][L,R]")),
            Some(vec!["S1L".into(), "S1R".into(), "S2L".into(), "S2R".into()])
        );
    }

    #[test]
    fn sem_equiv__member_set_plain_name_is_singleton() {
        assert_eq!(member_set(&McIds::from("gnd")), Some(vec!["gnd".into()]));
    }

    #[test]
    fn sem_equiv__member_set_from_str_curly_pipe() {
        assert_eq!(
            member_set_from_str("Q1{S|D}"),
            Some(vec!["Q1.S".into(), "Q1.D".into()])
        );
    }

    #[test]
    fn sem_equiv__member_set_from_str_curly_mixed_separators() {
        assert_eq!(
            member_set_from_str("X{SPI,MIC|DAC_OUT}"),
            Some(vec!["X.SPI".into(), "X.MIC".into(), "X.DAC_OUT".into()])
        );
    }

    #[test]
    fn sem_equiv__member_set_from_str_escaped_member() {
        assert_eq!(
            member_set_from_str("usbsock.USB.D\\+"),
            Some(vec!["usbsock.USB.D+".into()])
        );
    }

    #[test]
    fn sem_equiv__member_set_from_str_curly_slice_expands_range() {
        // R12: `IO0{0:7}` (real corpus: pca9555.mc) expands to the 8 members,
        // matching the AST Curly+Slice branch — not a literal "0:7" member.
        assert_eq!(
            member_set_from_str("IO0{0:7}"),
            Some(vec![
                "IO0.0".into(),
                "IO0.1".into(),
                "IO0.2".into(),
                "IO0.3".into(),
                "IO0.4".into(),
                "IO0.5".into(),
                "IO0.6".into(),
                "IO0.7".into(),
            ])
        );
    }

    #[test]
    fn sem_equiv__member_set_from_str_curly_slice_descending() {
        // Declaration direction is authoritative: `4:1` yields [4,3,2,1].
        assert_eq!(
            member_set_from_str("X{4:1}"),
            Some(vec!["X.4".into(), "X.3".into(), "X.2".into(), "X.1".into(),])
        );
    }

    #[test]
    fn sem_equiv__member_set_from_str_curly_slice_mixed_with_enum() {
        // Slices and enumerated members mix in one group, in writing order.
        assert_eq!(
            member_set_from_str("P{1,3:5}"),
            Some(vec!["P.1".into(), "P.3".into(), "P.4".into(), "P.5".into(),])
        );
    }

    #[test]
    fn sem_equiv__parse_display_curly_is_structural() {
        // The string front-end now parses `Q1{S|D}` into a base `Ida` run
        // plus a `Curly` member group — the same tree shape the AST front
        // end builds — instead of a single flat Ida that a text post-pass
        // re-splits. Ports can read base and members from the tree.
        let ids = parse_display("Q1{S|D}");
        assert_eq!(ids.segments.len(), 2);
        assert!(matches!(
            &ids.segments[0],
            crate::semantic::basic::mc_ids::IdsSegment::Ida(_)
        ));
        match &ids.segments[1] {
            crate::semantic::basic::mc_ids::IdsSegment::Curly(members) => {
                assert_eq!(members.len(), 2);
            }
            other => panic!("expected a Curly member group, got {other:?}"),
        }
        // Member set is unchanged from the pre-parse_display front-end.
        assert_eq!(
            member_set_from_str("Q1{S|D}"),
            Some(vec!["Q1.S".into(), "Q1.D".into()])
        );
    }

    #[test]
    fn sem_equiv__parse_display_curly_slice_is_slice_segment() {
        // R12 numeric slice `{0:7}` is a structural `Slice` inside the curly
        // group; expand later yields the interval in declaration order.
        let ids = parse_display("IO0{0:7}");
        match &ids.segments[1] {
            crate::semantic::basic::mc_ids::IdsSegment::Curly(members) => {
                assert!(matches!(
                    &members[0],
                    crate::semantic::basic::mc_ids::IdsSegment::Slice { .. }
                ));
            }
            other => panic!("expected a Curly member group, got {other:?}"),
        }
        assert_eq!(
            member_set_from_str("IO0{0:7}"),
            Some(vec![
                "IO0.0".into(),
                "IO0.1".into(),
                "IO0.2".into(),
                "IO0.3".into(),
                "IO0.4".into(),
                "IO0.5".into(),
                "IO0.6".into(),
                "IO0.7".into(),
            ])
        );
    }

    #[test]
    fn sem_equiv__parse_display_empty_curly_yields_no_members() {
        // An empty curly body removes the whole name (keeps the `dc{}`
        // param-name fallback in scope.rs a plain Label).
        assert_eq!(member_set_from_str("dc{}"), None);
    }

    #[test]
    fn sem_equiv__parse_display_escaped_brace_stays_literal() {
        // An escaped brace is a literal character, not a curly group — the
        // AST would never see a group there, and neither should the string
        // port (the old text pass split it only because escapes were already
        // consumed by expansion time).
        assert_eq!(
            member_set_from_str("A\\{X}"),
            Some(vec!["A{X}".to_string()])
        );
    }

    #[test]
    fn sem_equiv__canonical_single_bare_only() {
        assert_eq!(canonical_single(&McIds::from("gnd")), Some("gnd".into()));
        // Dotted member is not a bare identifier.
        assert_eq!(canonical_single(&McIds::from("A.m")), None);
        // Multi-member is not canonical-single.
        assert_eq!(canonical_single(&McIds::from("c[1:2]")), None);
    }

    #[test]
    fn sem_equiv__are_equivalent_compares_member_sets() {
        assert!(are_equivalent(&McIds::from("c[1:2]"), &McIds::from("c1,c2")) == false);
        assert!(are_equivalent(&McIds::from("res[4]"), &McIds::from("res4")));
    }
}
