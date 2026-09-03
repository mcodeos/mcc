// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! LSP goto-def regression: a module-port interface binding such as
//! `module US513([VDD_3V3,GND]::DC(3.3V))` must register the interface class
//! name (`DC`) as a ClassRef so goto-def lands on the `interface DC` definition.
//!
//! NOTE: These tests share global mcc state, so a mutex serializes them.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use mcc::{McIds, McURI};

const SOURCE: &str = r#"
interface DC(volt)
{
    pins = [
        1 = VOUT, "DC power positive"
        2 = GND, "DC power ground"
    ]
}

module main([VDD_3V3,GND]::DC(3.3V))
{
    VDD_3V3 -> GND
}
"#;

#[test]
fn svc_portiface__binding_registers_class_ref() {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = "/mcc/module-port-interface-ref.mc".to_string();
    mcc::mcc_load_from_string(&uri, SOURCE);

    let dump = mcc::dump_symbols_f12_text(&uri).expect("f12 dump");

    // Byte span of the interface class name `DC` inside
    // `module main([VDD_3V3,GND]::DC(3.3V))` (audit log: 144..146).
    let dc_ref_span = SOURCE.find("::DC").map(|p| p + 2).expect("::DC in source");

    // 1. The module-port `::DC` must be a ClassRef interval at that span.
    let ref_line = dump
        .lines()
        .find(|l| l.contains("LAPPER_REF") && l.contains("kind=ClassRef"))
        .map(|l| extract_span(l).is_some_and(|(a, b)| a == dc_ref_span && b == dc_ref_span + 2));
    assert!(
        ref_line.unwrap_or(false),
        "expected a ClassRef interval at span {dc_ref_span}..{} for '::DC'; dump:\n{}",
        dc_ref_span + 2,
        dump.lines()
            .filter(|l| l.contains("ClassRef"))
            .collect::<Vec<_>>()
            .join("\n")
    );

    // 2. The ClassRef must map to the `interface DC` ClassDef (span 11..13) in the same file.
    let mapped = dump.lines().any(|l| {
        if !(l.contains("Ref(ClassRef") && l.contains("=> Def(ClassDef")) {
            return false;
        }
        let Some((a, b)) = l.find("=> Def").and_then(|i| extract_span(&l[i..])) else {
            return false;
        };
        a == 11 && b == 13
    });
    assert!(
        mapped,
        "ClassRef 'DC' must map to the interface DC ClassDef at 11..13; dump:\n{}",
        dump.lines()
            .filter(|l| l.contains("MAP"))
            .collect::<Vec<_>>()
            .join("\n")
    );
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
fn svc_portiface__named_square_iface_port_is_single_instance() {
    let _lock = common::lock();
    common::reset();

    // A named square-vec instance binding (`PWR_[VDD2, GND2]::DC(5V)`) is ONE
    // interface port named `PWR_`; it must not be treated as an array to
    // expand per member (which previously registered the same key twice and
    // reported INST_DECLARED_MULTIPLE / duplicate LSP symbols).
    let uri: McURI = "/mcc/named-square-iface-port.mc".to_string();
    let source = r#"
interface DC(volt)
{
    pins = [
        1 = VOUT, "DC power positive"
        2 = GND, "DC power ground"
    ]
}

module main
{
    in PWR_[VDD2, GND2]::DC(5V)
}
"#;
    mcc::mcc_load_from_string(&uri, source);
    mcc::mcc_build(&McIds::from("main"), &uri).expect("build failed");

    let diags = mcc::mcc_diagnose_all();
    let pwr_5151: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 5151 && d.msg.contains("PWR_"))
        .collect();
    assert!(
        pwr_5151.is_empty(),
        "PWR_ must be a single instance, got: {:?}",
        pwr_5151
    );
}
