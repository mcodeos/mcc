// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The view vocabulary lock (CIMP U280, first half).
//!
//! The vocabulary ruling (b3907) makes the six words of
//! `schema/projection.cddl`'s `view-name` the canonical read projections —
//! the word list is the ruling, so this file reads the words **from the CDDL**,
//! not from a second hand-copied list, and golden-locks the set. The read
//! faces publish stage and join vocabularies of their own (design §3: a
//! pipeline stage names a pipeline stage, not a read-side projection of one
//! frozen world), and the law for them is exact-name disjointness: until a
//! canonical word's serde payload group lands, publishing that word would be
//! impersonating a projection that does not exist. The lock holds three
//! things:
//!
//! 1. the CDDL's `view-name` set is exactly the six ruled words;
//! 2. a published `view` value equals a canonical word only once that word's
//!    serde payload group has landed (the carried set below);
//! 3. the published registry actually covers what the producers stamp, so it
//!    cannot rot behind the constants it lists.
//!
//! A fourth lock guards the first carried group itself: the serialized member
//! set of the `diag` item equals the CDDL group's member set — no more, no
//! less — which is the CDDL↔serde consistency guard the schema family calls
//! for, per view as each one lands.

use mcc::stages::{self, StageSeg};

const CDDL: &str = include_str!("../../schema/projection.cddl");

/// The quoted words of the `view-name` rule: the text from `view-name` to the
/// next blank line is the whole rule in the schema file, and its string
/// literals are the word list.
fn canonical_view_names() -> Vec<&'static str> {
    let start = CDDL
        .find("view-name")
        .expect("schema/projection.cddl has a view-name rule");
    let rule = CDDL[start..]
        .split("\n\n")
        .next()
        .expect("the view-name rule is a non-empty block");
    let mut words: Vec<&'static str> = Vec::new();
    let mut inside = false;
    for tok in rule.split('"') {
        if inside {
            words.push(tok);
        }
        inside = !inside;
    }
    words.sort_unstable();
    words
}

/// The six ruled words (b3907), locked so an edit to the CDDL word list is a
/// conscious re-pin here, not a silent vocabulary change.
const RULED_WORDS: &[&str] = &[
    "core-erc",
    "diagnostics",
    "diff",
    "expectation",
    "netlist",
    "project-model",
];

#[test]
fn cddl_view_name_rule_is_exactly_the_six_ruled_words() {
    assert_eq!(canonical_view_names(), RULED_WORDS);
}

/// The canonical words that HAVE landed a serde payload group. Each entry
/// names a real carrier — `diagnostics` → `mcc::stages::diagview::DiagItem`
/// — kept honest by the member-set guard below and the golden byte lock in
/// `diag_view_golden.rs`. Publishing a second canonical word means landing
/// its group first, then adding it here.
const CARRIED_CANONICAL_VIEWS: &[&str] = &["diagnostics"];

#[test]
fn no_published_view_impersonates_an_uncarried_canonical_word() {
    let canonical = canonical_view_names();
    let mut seen = std::collections::BTreeSet::new();
    for view in stages::published_views() {
        assert!(
            !canonical.contains(&view) || CARRIED_CANONICAL_VIEWS.contains(&view),
            "'{view}' is a canonical projection word with no serde payload behind it; \
             land the payload group first, then add the word to CARRIED_CANONICAL_VIEWS (design §3)"
        );
        assert!(seen.insert(view), "'{view}' is registered twice");
    }
}

#[test]
fn registry_covers_every_producer_constant() {
    let mut published = stages::published_views();
    published.sort_unstable();

    let mut stamped: Vec<&str> = Vec::new();
    for seg in [StageSeg::P1, StageSeg::P2, StageSeg::Vec, StageSeg::Viz] {
        stamped.push(seg.view_name());
    }
    stamped.push(stages::join::SRC_P2_VIEW);
    stamped.push(stages::join::P2_VEC_VIEW);
    stamped.push(stages::join::VEC_VIZ_VIEW);
    stamped.push(stages::trace::TRACE_VIEW);
    stamped.push(stages::ORG_UNITS_VIEW);
    stamped.push(stages::diagview::DIAGNOSTICS_VIEW);
    stamped.push(stages::stage_diff::DIFF_P2_VIEW);
    stamped.push(stages::stage_diff::DIFF_VEC_VIEW);
    stamped.push(stages::stage_diff::DIFF_VIZ_VIEW);
    stamped.sort_unstable();

    assert_eq!(published, stamped, "the registry and the producers drifted");
}

/// The member identifiers of the CDDL `diag` group, read from the schema file:
/// the text from `diag =` to its closing `}` is the whole group; each member
/// is the identifier before the `:` on a non-comment line, with the optional
/// marker `?` stripped.
fn cddl_diag_members() -> Vec<String> {
    let start = CDDL
        .find("diag =")
        .expect("schema/projection.cddl has a diag group");
    let block = &CDDL[start..];
    let end = block
        .find('}')
        .expect("the diag group is closed on the same rule");
    let mut members: Vec<String> = Vec::new();
    for line in block[..=end].lines() {
        let line = match line.split(';').next() {
            Some(code) => code.trim(),
            None => continue,
        };
        if line.is_empty() || line.starts_with("diag") {
            continue;
        }
        let name = line.trim_start_matches('?');
        let Some((member, _)) = name.split_once(':') else {
            continue;
        };
        members.push(member.trim().to_string());
    }
    members.sort();
    members
}

/// The CDDL↔serde guard for the first carried group: the serialized key set
/// of a **fully populated** `DiagItem` is exactly the CDDL member set, and the
/// key set of a minimally populated one is exactly the required-member set —
/// a field added on one side and not the other fails here, which is the drift
/// the schema family exists to kill.
#[test]
fn cddl_diag_group_members_are_exactly_the_serialized_fields() {
    use mcc::stages::diagview::{DiagItem, DiagLoc, Level};

    let required = |item: &DiagItem| {
        let v = serde_json::to_value(item).expect("DiagItem serializes");
        let mut keys: Vec<String> = v
            .as_object()
            .expect("DiagItem serializes to an object")
            .keys()
            .cloned()
            .collect();
        keys.sort();
        keys
    };

    let full = DiagItem {
        code: "E4057".to_string(),
        level: Level::Advisory,
        msg: "msg".to_string(),
        loc: DiagLoc {
            uri: "file://x.mc".to_string(),
            line: 3,
            span: Some(2),
        },
        fix_hint: Some("hint".to_string()),
        net: Some("N1".to_string()),
        pin: Some("1".to_string()),
    };
    assert_eq!(
        required(&full),
        cddl_diag_members(),
        "the serialized `diag` item and the CDDL group drifted"
    );

    let minimal = DiagItem {
        code: "E1000".to_string(),
        level: Level::Error,
        msg: "msg".to_string(),
        loc: DiagLoc {
            uri: "file://x.mc".to_string(),
            line: 1,
            span: None,
        },
        fix_hint: None,
        net: None,
        pin: None,
    };
    let keys = required(&minimal);
    assert!(
        !keys.contains(&"fix_hint".to_string())
            && !keys.contains(&"net".to_string())
            && !keys.contains(&"pin".to_string()),
        "absent optional members must be omitted, not serialized as null: {keys:?}"
    );
    assert_eq!(keys.len(), 4, "the required member set is code/level/msg/loc");
}
