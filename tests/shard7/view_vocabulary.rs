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
//! 2. no published `view` value equals a canonical word;
//! 3. the published registry actually covers what the producers stamp, so it
//!    cannot rot behind the constants it lists.

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

#[test]
fn no_published_view_impersonates_a_canonical_word() {
    let canonical = canonical_view_names();
    let mut seen = std::collections::BTreeSet::new();
    for view in stages::published_views() {
        assert!(
            !canonical.contains(&view),
            "'{view}' is a canonical projection word with no serde payload behind it; \
             name the face after what it reads instead (design §3)"
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
    stamped.push(stages::stage_diff::DIFF_P2_VIEW);
    stamped.push(stages::stage_diff::DIFF_VEC_VIEW);
    stamped.push(stages::stage_diff::DIFF_VIZ_VIEW);
    stamped.sort_unstable();

    assert_eq!(published, stamped, "the registry and the producers drifted");
}
