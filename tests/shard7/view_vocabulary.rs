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
const CARRIED_CANONICAL_VIEWS: &[&str] = &["diagnostics", "netlist", "project-model"];

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
    stamped.push(stages::netlistview::NETLIST_VIEW);
    stamped.push(stages::projmodel::PROJECT_MODEL_VIEW);
    stamped.push(stages::stage_diff::DIFF_P2_VIEW);
    stamped.push(stages::stage_diff::DIFF_VEC_VIEW);
    stamped.push(stages::stage_diff::DIFF_VIZ_VIEW);
    stamped.sort_unstable();

    assert_eq!(published, stamped, "the registry and the producers drifted");
}

/// The member identifiers of one CDDL group, read from the schema file: the
/// text from `marker` (e.g. `"diag ="`) to the group's closing `}` is the
/// whole group; each member is the identifier before the `:` on a
/// non-comment line, with the optional marker `?` stripped. Brace depth is
/// tracked so a group with single-line nested groups (`circuit-node`'s
/// `params` map, `ports` array) parses whole.
fn cddl_group_members(marker: &str) -> Vec<String> {
    let start = CDDL
        .find(marker)
        .unwrap_or_else(|| panic!("schema/projection.cddl has a `{marker}` group"));
    let mut members: Vec<String> = Vec::new();
    let mut depth = 0usize;
    for line in CDDL[start..].lines() {
        let code = line.split(';').next().unwrap_or("").trim();
        if depth == 0 {
            // The marker line itself opens the group.
            depth = 1;
            continue;
        }
        let name = code.trim_start_matches('?');
        if let Some((member, _)) = name.split_once(':') {
            members.push(member.trim().to_string());
        }
        depth += code.matches('{').count() + code.matches('[').count();
        depth -= code.matches('}').count() + code.matches(']').count();
        if depth == 0 {
            break;
        }
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
        cddl_group_members("diag ="),
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

/// The same guard for the second carried group: the serialized key set of
/// `NetItem` is exactly the CDDL `net` group's member set.
#[test]
fn cddl_net_group_members_are_exactly_the_serialized_fields() {
    use mcc::stages::netlistview::NetItem;

    let item = NetItem {
        name: "VDD".to_string(),
        points: vec!["r1.1".to_string(), "r2.2".to_string()],
    };
    let v = serde_json::to_value(&item).expect("NetItem serializes");
    let mut keys: Vec<String> = v
        .as_object()
        .expect("NetItem serializes to an object")
        .keys()
        .cloned()
        .collect();
    keys.sort();
    assert_eq!(
        keys,
        cddl_group_members("net ="),
        "the serialized `net` item and the CDDL group drifted"
    );
}

/// The same guard for the third carried group: the serialized key set of a
/// fully-filled `CircuitNode` (children present) is exactly the CDDL
/// `circuit-node` group's member set; a leaf omits only the optional
/// `children`.
#[test]
fn cddl_circuit_node_group_members_are_exactly_the_serialized_fields() {
    use mcc::stages::projmodel::{CircuitNode, DefSite, PortItem, TypedValue};

    let def = || DefSite {
        kind: "component".to_string(),
        name: "R".to_string(),
        uri: "m.".to_string(),
        span: 4,
    };
    let leaf = |name: &str| CircuitNode {
        id: name.to_string(),
        def: def(),
        params: {
            let mut m = std::collections::BTreeMap::new();
            m.insert("res".to_string(), TypedValue::Int(10));
            m
        },
        ports: vec![PortItem {
            name: "1".to_string(),
            dir: "in",
            net: Some("VDD".to_string()),
        }],
        children: None,
    };
    let full = CircuitNode {
        children: Some(vec![leaf("main.r1")]),
        ..leaf("main")
    };

    let keys = |v: &serde_json::Value| -> Vec<String> {
        let mut keys: Vec<String> = v
            .as_object()
            .expect("CircuitNode serializes to an object")
            .keys()
            .cloned()
            .collect();
        keys.sort();
        keys
    };

    let full_v = serde_json::to_value(&full).expect("full node serializes");
    assert_eq!(
        keys(&full_v),
        cddl_group_members("circuit-node ="),
        "the serialized `circuit-node` item and the CDDL group drifted"
    );

    let leaf_v = serde_json::to_value(leaf("main.r1")).expect("leaf serializes");
    let mut required = cddl_group_members("circuit-node =");
    required.retain(|m| m != "children");
    assert_eq!(
        keys(&leaf_v),
        required,
        "the leaf must omit only the optional member"
    );
}
