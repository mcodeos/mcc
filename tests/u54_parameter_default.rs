// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Integration test: the default value a formal parameter is written with (CIMP U54).
//
// A declaration's default is one fact with three consequences, and all three
// are pinned here. It belongs to the DECLARATION, so `sel = FAST` without a
// type annotation records it exactly as `partno::STRING = "WIDE"` does — the
// type is not where the value lives. It is a real value, so an instance whose
// call site writes no argument evaluates its conditional blocks with it. And
// it never outranks the call site, so the instance that passes its own value
// selects its own branch.
//
// The fourth case is the other half of the same coin: a formal left unbound
// says nothing about which pins exist, so a pin the component's conditional
// block adds is found even when the binding failed. A definition-side and an
// instance-side answer to "is this pin here?" must not disagree.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use std::collections::BTreeSet;

use mcc::{McIds, McURI};

/// A pin the component instance does not have — the code that reports a
/// definition-side and an instance-side answer disagreeing.
const PIN_NOT_FOUND: u32 = 3179;

/// A two-branch component: the branch is chosen by `sel`, whose written default
/// is the whole point. `FAST` resolves as no enum member anywhere, so the type
/// stays unknown and only the declaration can carry the value.
///
/// The branch's pin is `2 = Q_FAST` — pin id `2`, named `Q_FAST` — and the net
/// table addresses a pin by its ID, so the assertions below read `a.2` for
/// "instance `a` took the `sel == FAST` branch".
const SEL: &str = r#"
component SEL(sel = FAST)
{
    pins = [1 = P]
    if (sel == FAST) { pins += [2 = Q_FAST] }
    else { pins += [3 = Q_SLOW] }
}
"#;

/// A component with two genuinely required parameters and a condition over
/// literals alone. Nothing here carries a default, so the block is deferred to
/// every instance and every instance leaves both formals unbound. Its pin is
/// `2 = Q_TRUE`, ids being what the net table addresses.
const REQ: &str = r#"
component REQ(a::INT, b::STRING)
{
    pins = [1 = P]
    if (1 == 1) { pins += [2 = Q_TRUE] }
}
"#;

fn source(body: &str) -> String {
    format!(
        r#"{SEL}{REQ}
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

    let uri: McURI = format!("/mcc/u54-{tag}.mc");
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

/// The definition the instance `inst` was built from — the surface a
/// declaration's own facts live on.
fn def_of(tag: &str, body: &str, inst: &str) -> std::sync::Arc<mcc::McComponent> {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = format!("/mcc/u54-{tag}.mc");
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

/// Every `(code, message)` the build reports, in no particular order.
fn diags_of(tag: &str, body: &str) -> Vec<(u32, String)> {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = format!("/mcc/u54-{tag}.mc");
    mcc::mcc_load_from_string(&uri, &source(body));
    let _ = mcc::mcc_build(&McIds::from("main"), &uri);
    mcc::mcc_diagnose_all()
        .iter()
        .map(|d| (d.code, d.msg.clone()))
        .collect()
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

/// The net holding each point, for the failure messages: a wrong branch is only
/// readable when the nets are on screen.
fn dump(nets: &[BTreeSet<String>]) -> String {
    nets.iter()
        .map(|n| n.iter().cloned().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join(" | ")
}

/// The declaration records the default it was written with, verbatim, and a
/// default is what makes a formal optional. `FAST` names no enum member, so no
/// type could have carried it: the value survives on the declaration alone.
#[test]
fn u54__a_written_default_is_recorded_on_the_declaration() {
    let def = def_of("recorded", "    SEL a\n    a.P -> VDD\n", "a");
    let defaults = def.params.get_params_with_defaults();
    assert_eq!(
        defaults
            .iter()
            .map(|(n, v)| (n.to_string(), v.as_str()))
            .collect::<Vec<_>>(),
        vec![("sel".to_string(), "FAST")],
        "the written default is the declaration's fact; got {defaults:?}"
    );

    let sel = def.params.find("sel").expect("`sel` is declared");
    assert!(
        sel.has_default_value(),
        "a formal with a default is optional at every call site"
    );
}

/// The instance that writes no argument evaluates its conditional blocks with
/// the default, so the branch the default selects is the one it gets.
#[test]
fn u54__a_bare_default_reaches_the_instance() {
    let n = nets("bare-default", "    SEL a\n    a.Q_FAST -> VDD\n");

    assert!(
        shares_a_net(&n, "a.2", "VDD"),
        "the default selects the branch, so the instance carries its pin: {}",
        dump(&n)
    );
    assert!(
        !shares_a_net(&n, "a.3", "VDD"),
        "and the branch it does not select adds no pin: {}",
        dump(&n)
    );
}

/// The call site outranks the default: the instance that passes its own value
/// selects its own branch, and the definition is not decided for it.
#[test]
fn u54__an_argument_outranks_the_default() {
    let n = nets(
        "arg-outranks",
        "    SEL a\n    SEL(SLOW) b\n    a.Q_FAST -> VDD\n    b.Q_SLOW -> VDD\n",
    );

    assert!(
        shares_a_net(&n, "b.3", "VDD"),
        "the argument selects the branch: {}",
        dump(&n)
    );
    assert!(
        shares_a_net(&n, "a.2", "VDD"),
        "and the default still holds for the instance that writes nothing: {}",
        dump(&n)
    );
    assert!(
        !shares_a_net(&n, "b.2", "VDD"),
        "the default never reaches the instance that overrode it: {}",
        dump(&n)
    );
}

/// An unbound formal is not evidence about which pins exist. Both formals here
/// are genuinely required and neither is bound, so the binding fails — and the
/// pin the literal condition adds is still found. The block stays on the
/// definition, which is what makes this the instance-side answer under test
/// rather than a branch the definition already folded in.
#[test]
fn u54__an_unbound_formal_does_not_hide_a_pin() {
    let body = "    REQ x\n    x.Q_TRUE -> VDD\n";
    let def = def_of("unbound", body, "x");
    assert!(
        !def.cond_pins.is_empty(),
        "the condition is deferred to the instance, not decided on the definition"
    );

    let n = nets("unbound", body);
    assert!(
        shares_a_net(&n, "x.2", "VDD"),
        "a pin the deferred block adds is found even though the binding failed: {}",
        dump(&n)
    );

    let missed: Vec<String> = diags_of("unbound", body)
        .into_iter()
        .filter(|(code, msg)| *code == PIN_NOT_FOUND && msg.contains("Q_TRUE"))
        .map(|(_, msg)| msg)
        .collect();
    assert!(
        missed.is_empty(),
        "the pin exists, so reporting it missing is the disagreement this locks out; got {missed:?}"
    );
}
