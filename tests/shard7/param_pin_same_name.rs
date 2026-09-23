// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! CIMP U269: a component parameter and a pin sharing one name in one
//! component scope are two defs -- two canonical keys, two DeclareIds, two
//! spans. CIMP U281: the net-line `u1.P` member face is terminal-first (G10)
//! -- a member ref anchors at the pin's span, never at the parameter's, while
//! a param with no same-name pin keeps the parameter anchor.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use mcc::McURI;

const SOURCE: &str = r#"
component PP(P)
{
    pins = [
        1 = P
    ]
}

module main
{
    PP u1
    PP u2

    u1.P -> u2.P
}
"#;

/// A second fixture without the pin/param name collision: `Q` is a parameter
/// only, so the member face falls through the terminal face to the parameter.
const PARAM_ONLY: &str = r#"
component PQ(Q)
{
    pins = [
        1 = X
    ]
}

module main
{
    PQ q1

    q1.Q -> q1.Q
}
"#;

/// Parse `id=  N` from a F12_DIAG dump line.
fn extract_id(line: &str) -> Option<u32> {
    let s = line.find("id=")?;
    let rest = line[s + 3..].trim_start();
    let end = rest.find(|c: char| !c.is_ascii_digit())?;
    rest[..end].parse().ok()
}

/// Parse `span=[  123,  456]` from a F12_DIAG dump line.
fn extract_span(line: &str) -> Option<(usize, usize)> {
    let s = line.find("span=[")?;
    let rest = &line[s + 6..];
    let comma = rest.find(',')?;
    let close = rest.find(']')?;
    let a: usize = rest[..comma].trim().parse().ok()?;
    let b: usize = rest[comma + 1..close].trim().parse().ok()?;
    Some((a, b))
}

#[test]
fn svc_parpin__same_name_param_and_pin_keep_two_ids_and_two_spans() {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = "/mcc/param-pin-same-name.mc".to_string();
    mcc::mcc_load_from_string(&uri, SOURCE);

    let dump = mcc::dump_symbols_f12_text(&uri).expect("f12 dump");

    let param_span = (
        SOURCE.find("(P)").expect("param P in source") + 1,
        SOURCE.find("(P)").unwrap() + 2,
    );
    let pin_span = (
        SOURCE.find("= P").expect("pin P in source") + 2,
        SOURCE.find("= P").unwrap() + 3,
    );

    // Two DECLARE rows for name='P' scope='PP' with distinct ids and each
    // kind's own position: the parameter at the header text, the pin inside
    // the pins list. Before U269 the second registration overwrote the first,
    // leaving one row (and one shared id).
    let rows: Vec<&str> = dump
        .lines()
        .filter(|l| {
            l.contains("F12_DIAG DECLARE:") && l.contains("scope='PP'") && l.contains("name='P'")
        })
        .collect();
    assert_eq!(
        rows.len(),
        2,
        "param P and pin P must both be declared; dump:\n{}",
        dump.lines()
            .filter(|l| l.contains("DECLARE"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let ids: Vec<u32> = rows.iter().filter_map(|l| extract_id(l)).collect();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1], "the two defs must not share one id");
    let spans: Vec<(usize, usize)> = rows.iter().filter_map(|l| extract_span(l)).collect();
    assert!(
        spans.contains(&param_span),
        "the parameter keeps its own span {param_span:?}; got {spans:?}"
    );
    assert!(
        spans.contains(&pin_span),
        "the pin keeps its own span {pin_span:?}; got {spans:?}"
    );
}

#[test]
fn svc_parpin__ref_side_keeps_the_two_defs_apart_in_the_def_map() {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = "/mcc/param-pin-same-name-ref.mc".to_string();
    mcc::mcc_load_from_string(&uri, SOURCE);

    let dump = mcc::dump_symbols_f12_text(&uri).expect("f12 dump");

    let param_span = (
        SOURCE.find("(P)").expect("param P in source") + 1,
        SOURCE.find("(P)").unwrap() + 2,
    );
    let pin_span = (
        SOURCE.find("= P").expect("pin P in source") + 2,
        SOURCE.find("= P").unwrap() + 3,
    );

    // The u1.P / u2.P net-line usages are pin usages: their PinNameRef rows
    // must map to a PinNameDef, never to the parameter.
    let pin_ref_maps = dump
        .lines()
        .filter(|l| l.contains("Ref(PinNameRef") && l.contains("=> Def(PinNameDef"))
        .count();
    assert!(
        pin_ref_maps > 0,
        "the u1.P/u2.P usages must produce PinNameRef → PinNameDef mappings; dump:\n{}",
        dump.lines()
            .filter(|l| l.contains("PinNameRef"))
            .collect::<Vec<_>>()
            .join("\n")
    );

    // In the def map the two defs must be two rows with distinct ids at their
    // own spans: the parameter at the header text, the pin inside the pins
    // list. Before U269 one raw id carried both kinds and the pin row's id
    // was the parameter's.
    let def_rows: Vec<&str> = dump
        .lines()
        .filter(|l| l.contains("F12_DIAG DEF_MAP:"))
        .collect();
    let param_rows: Vec<u32> = def_rows
        .iter()
        .filter(|l| l.contains("kind=ParamDef"))
        .filter_map(|l| extract_id(l))
        .collect();
    let pin_def_rows: Vec<(u32, (usize, usize))> = def_rows
        .iter()
        .filter(|l| l.contains("kind=PinNameDef"))
        .filter_map(|l| extract_id(l).map(|id| (id, extract_span(l).unwrap_or((0, 0)))))
        .collect();
    assert!(
        def_rows
            .iter()
            .any(|l| l.contains("kind=ParamDef")
                && extract_span(l).is_some_and(|s| s == param_span)),
        "the param def keeps the header span {param_span:?}; rows:\n{}",
        def_rows.join("\n")
    );
    assert!(
        pin_def_rows.iter().any(|(_, s)| *s == pin_span),
        "the pin def keeps its own span {pin_span:?}; rows:\n{}",
        def_rows.join("\n")
    );
    for (pid, _) in &pin_def_rows {
        assert!(
            !param_rows.contains(pid),
            "the pin def id {pid} must not be the parameter's id; rows:\n{}",
            def_rows.join("\n")
        );
    }
}

/// Def span of the F12 MAP row whose ref name equals `ref_name`
/// (`Ref(PinNameRef/13, id=N, name='u1.P') => Def(..., span=[a,b], ...)`).
fn map_def_span(dump: &str, ref_name: &str) -> Option<(usize, usize)> {
    dump.lines()
        .find(|l| l.contains("F12_DIAG MAP:") && l.contains(&format!("name='{ref_name}'")))?
        .split("span=[")
        .nth(1)
        .and_then(|rest| {
            let comma = rest.find(',')?;
            let close = rest.find(']')?;
            Some((
                rest[..comma].trim().parse().ok()?,
                rest[comma + 1..close].trim().parse().ok()?,
            ))
        })
}

/// CIMP U281: the `u1.P` / `u2.P` net-line member face is terminal-first --
/// each member ref anchors at the pin's span inside the pins list, never at
/// the parameter header, even though `P` is declared as both.
#[test]
fn svc_parpin__member_ref_anchors_at_the_pin_not_the_parameter() {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = "/mcc/param-pin-same-name-member.mc".to_string();
    mcc::mcc_load_from_string(&uri, SOURCE);

    let dump = mcc::dump_symbols_f12_text(&uri).expect("f12 dump");

    let pin_span = (
        SOURCE.find("= P").expect("pin P in source") + 2,
        SOURCE.find("= P").unwrap() + 3,
    );

    for ref_name in ["u1.P", "u2.P"] {
        assert_eq!(
            map_def_span(&dump, ref_name),
            Some(pin_span),
            "the {ref_name} member ref must anchor at the pin span {pin_span:?}, not the parameter header; dump:\n{}",
            dump.lines()
                .filter(|l| l.contains("F12_DIAG MAP:"))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

/// The terminal face only re-ranks the member face: with no pin sharing the
/// name, the `q1.Q` member ref still falls through to the parameter anchor.
#[test]
fn svc_parpin__param_only_member_ref_keeps_the_parameter_anchor() {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = "/mcc/param-only-member-anchor.mc".to_string();
    mcc::mcc_load_from_string(&uri, PARAM_ONLY);

    let dump = mcc::dump_symbols_f12_text(&uri).expect("f12 dump");

    let param_span = (
        PARAM_ONLY.find("(Q)").expect("param Q in source") + 1,
        PARAM_ONLY.find("(Q)").unwrap() + 2,
    );

    assert_eq!(
        map_def_span(&dump, "q1.Q"),
        Some(param_span),
        "the q1.Q member ref must keep the parameter anchor {param_span:?}; dump:\n{}",
        dump.lines()
            .filter(|l| l.contains("F12_DIAG MAP:"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
