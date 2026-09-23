// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! CIMP U281 interface face: the same terminal-first law the component member
//! face obeys -- an interface instance's member ref anchors at the pin's span
//! even when an interface parameter shares the name, while a param with no
//! same-name pin keeps the parameter anchor.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use mcc::McURI;

const SOURCE: &str = r#"
interface II(P)
{
    pins = [
        1 = P
    ]
}

module main
{
    II i1

    i1.P -> i1.P
}
"#;

/// Def span of the F12 MAP row whose ref name equals `ref_name`.
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

/// The `i1.P` member ref anchors at the pin's span inside the interface's
/// pins list, never at the same-named parameter header.
#[test]
fn svc_ifparpin__member_ref_anchors_at_the_pin_not_the_parameter() {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = "/mcc/iface-param-pin-same-name.mc".to_string();
    mcc::mcc_load_from_string(&uri, SOURCE);

    let dump = mcc::dump_symbols_f12_text(&uri).expect("f12 dump");

    let pin_span = (
        SOURCE.find("= P").expect("pin P in source") + 2,
        SOURCE.find("= P").unwrap() + 3,
    );

    assert_eq!(
        map_def_span(&dump, "i1.P"),
        Some(pin_span),
        "the i1.P member ref must anchor at the pin span {pin_span:?}, not the parameter header; dump:\n{}",
        dump.lines()
            .filter(|l| l.contains("F12_DIAG MAP:"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
