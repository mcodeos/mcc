// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Integration test: a literal default value on an UNTYPED formal (CIMP U66).
//
// The declaration side and the read side spell one value in two ways, and they
// have to agree on the TEXT face: the recorded default is the bare text, so
// `get_params_with_defaults` answers `FAST` for both `sel = FAST` and
// `sel = "FAST"`. Before this rule the declaration side could not write the
// quoted spelling at all: `sel = "FAST"` failed to parse, and the whole file
// was reported invalid. The rule `mc_ids MCOP_EQUAL mc_literal` is what lets
// the two sides meet.
//
// The quotes are the lexical FAMILY of the value (U144, ruling of 2026-09-20):
// a default meets only the condition written in its own family — `sel = FAST`
// with `if (sel == "FAST")` never matches, and the mirror never matches
// either. The matrix lock in `cond_family_matrix.rs` holds the diagnostic
// face; this file holds the branch-selection face.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use std::collections::BTreeSet;

use mcc::{McIds, McURI};

/// A literal default, compared against the bare spelling of the same value.
const QS: &str = r#"
component QS(sel = "FAST")
{
    pins = [1 = P]
    if (sel == FAST) { pins += [2 = Q_FAST] }
    else { pins += [3 = Q_SLOW] }
}
"#;

/// The bare default, compared against the quoted spelling — the mirror image.
const QI: &str = r#"
component QI(sel = FAST)
{
    pins = [1 = P]
    if (sel == "FAST") { pins += [2 = Q_FAST] }
    else { pins += [3 = Q_SLOW] }
}
"#;

/// A numeric literal default. The branch's pin is `2 = Q_FIVE`.
const NUM: &str = r#"
component NUM(count = 5)
{
    pins = [1 = P]
    if (count == 5) { pins += [2 = Q_FIVE] }
    else { pins += [3 = Q_OTHER] }
}
"#;

/// The quoted default against the quoted condition — the same-family pair.
const QQ: &str = r#"
component QQ(sel = "FAST")
{
    pins = [1 = P]
    if (sel == "FAST") { pins += [2 = Q_FAST] }
    else { pins += [3 = Q_SLOW] }
}
"#;

/// The bare default against the bare condition — the mirror same-family pair.
const II: &str = r#"
component II(sel = FAST)
{
    pins = [1 = P]
    if (sel == FAST) { pins += [2 = Q_FAST] }
    else { pins += [3 = Q_SLOW] }
}
"#;

fn source(body: &str) -> String {
    format!(
        r#"{QS}{QI}{NUM}{QQ}{II}
module main
{{
    io VDD
{body}
}}
"#
    )
}

/// Build `source` and return the top module's nets, each as the set of point
/// paths it holds.
fn nets(tag: &str, body: &str) -> Vec<BTreeSet<String>> {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = format!("/mcc/u66-{tag}.mc");
    mcc::mcc_load_from_string(&uri, &source(body));
    let (_, _, _, store) = mcc::mcc_build_with_nets(&McIds::from("main"), &uri)
        .unwrap_or_else(|_| panic!("build failed for {tag}"));

    let mut out: Vec<BTreeSet<String>> = Vec::new();
    if let Some(table) = store.get("main") {
        for (_, pts) in table.iter() {
            let mut net: BTreeSet<String> = BTreeSet::new();
            for p in pts.iter() {
                net.insert(p.path.clone());
            }
            if !net.is_empty() {
                out.push(net);
            }
        }
    }
    out
}

/// The definition the instance `inst` was built from.
fn def_of(tag: &str, body: &str, inst: &str) -> std::sync::Arc<mcc::McComponent> {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = format!("/mcc/u66-{tag}.mc");
    mcc::mcc_load_from_string(&uri, &source(body));
    let (instance, arena, store, _) = mcc::mcc_build_with_arena(&McIds::from("main"), &uri)
        .unwrap_or_else(|_| panic!("build failed for {tag}"));
    let view = mcc::TreeView::new(&arena, &store);
    let def = view
        .components(&instance)
        .find(|c| c.name == inst)
        .unwrap_or_else(|| panic!("no instance named {inst}"))
        .def
        .clone();
    def
}

/// Is `path` the point named `name` — the name itself, or a dotted path whose
/// last segment it is? Comparing whole segments keeps `a.2` from matching on
/// the `2` inside `b.2`.
fn is_point(path: &str, name: &str) -> bool {
    path == name || path.ends_with(&format!(".{name}"))
}

/// Do `pin` and `other` share a net?
fn shares_a_net(nets: &[BTreeSet<String>], pin: &str, other: &str) -> bool {
    nets.iter()
        .any(|n| n.iter().any(|p| is_point(p, pin)) && n.iter().any(|p| is_point(p, other)))
}

/// The net holding each point, for the failure messages.
fn dump(nets: &[BTreeSet<String>]) -> String {
    nets.iter()
        .map(|n| n.iter().cloned().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join(" | ")
}

/// The declaration records the value, not the delimiters that spelled it —
/// otherwise the quoted default would never equal the bare condition.
#[test]
fn u66__a_literal_default_is_recorded_without_its_delimiters() {
    let body = "    QS a\n    a.P -> VDD\n    NUM c\n    c.P -> VDD\n";

    let quoted = def_of("recorded", body, "a");
    assert_eq!(
        quoted
            .params
            .get_params_with_defaults()
            .iter()
            .map(|(n, v)| (n.to_string(), v.as_str()))
            .collect::<Vec<_>>(),
        vec![("sel".to_string(), "FAST")],
        "the recorded default is the value the condition side compares against"
    );

    let numeric = def_of("recorded", body, "c");
    assert_eq!(
        numeric
            .params
            .get_params_with_defaults()
            .iter()
            .map(|(n, v)| (n.to_string(), v.as_str()))
            .collect::<Vec<_>>(),
        vec![("count".to_string(), "5")],
        "a numeric literal default is recorded as its own text"
    );
    assert!(
        numeric
            .params
            .find("count")
            .expect("`count` is declared")
            .has_default_value(),
        "a formal with a literal default is optional at every call site"
    );
}

/// A default meets only the condition written in its own family (U144,
/// ruling of 2026-09-20): the cross-family pairs fall to the else branch, and
/// the same-family pairs — either spelling — select the then branch.
#[test]
fn u66__a_default_meets_only_the_condition_in_its_own_family() {
    let n = nets(
        "spellings",
        "    QS a\n    QI b\n    QQ q\n    II i\n    a.Q_SLOW -> VDD\n    b.Q_SLOW -> VDD\n    q.Q_FAST -> VDD\n    i.Q_FAST -> VDD\n",
    );

    assert!(
        !shares_a_net(&n, "a.2", "VDD"),
        "`sel = \"FAST\"` never meets `sel == FAST`: {}",
        dump(&n)
    );
    assert!(
        shares_a_net(&n, "a.3", "VDD"),
        "so the else branch is selected: {}",
        dump(&n)
    );
    assert!(
        !shares_a_net(&n, "b.2", "VDD"),
        "`sel = FAST` never meets `sel == \"FAST\"`: {}",
        dump(&n)
    );
    assert!(
        shares_a_net(&n, "b.3", "VDD"),
        "so the else branch is selected: {}",
        dump(&n)
    );
    assert!(
        shares_a_net(&n, "q.2", "VDD") && !shares_a_net(&n, "q.3", "VDD"),
        "the quoted pair agrees on the then branch: {}",
        dump(&n)
    );
    assert!(
        shares_a_net(&n, "i.2", "VDD") && !shares_a_net(&n, "i.3", "VDD"),
        "the bare pair agrees on the then branch: {}",
        dump(&n)
    );
}

/// A numeric literal default reaches the instance the same way.
#[test]
fn u66__a_numeric_default_reaches_the_instance() {
    let n = nets("numeric", "    NUM c\n    c.Q_FIVE -> VDD\n");

    assert!(
        shares_a_net(&n, "c.2", "VDD"),
        "`count = 5` selects the `count == 5` branch: {}",
        dump(&n)
    );
    assert!(
        !shares_a_net(&n, "c.3", "VDD"),
        "and not the other branch: {}",
        dump(&n)
    );
}
