// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Lock for the E4101 vantage behavior fix (rule-registry design §4-a):
//! `check_driver_conflict` counts physical drivers per flat net, where a
//! module-boundary `out` port is the *exit* of the net segment inside its own
//! module instance. When that net already carries an `Out` point rooted inside
//! the port's module (the internal driver it forwards), the port is not a
//! second driver — otherwise the legal pattern
//! `SUB { out Y; BUF b; b.Y -> Y }` instanced as `s1` reports its internal net
//! `Y` (`s1.Y` + `s1.b.2`) as a 2-driver short.
//!
//! A port whose driver lives in a *different* module still counts: two out
//! ports shorted on a parent net (`s1.Y -> s2.Y`) are two real drivers and
//! keep firing. Same for two sibling out ports of one instance and for two
//! buffers shorted inside a single module that feeds one out port.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

/// Build `main` and flatten to the InstTable; return only the E4101
/// driver-conflict messages (in ordered form).
fn driver_conflict_msgs(src: &str) -> Vec<String> {
    let _lock = common::lock();
    common::reset();
    let uri = "/mcc/vantage.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build_flat(&mcc::McIds::from("main"), &uri, 1000).expect("flat build");
    mcc::mcc_diagnose_all()
        .iter()
        .filter(|d| d.code == mcc::errcodes::NET_MULTI_DRIVE)
        .map(|d| d.msg.clone())
        .collect()
}

/// Two-input/one-output buffer used by the driver-conflict fixtures.
const BUF: &str = "component BUF {\n    pins = [\n        in 1 = A\n        out 2 = Y\n    ]\n}\n";

/// §4-a: an interior driver feeding a module's own `out` port is one driver —
/// the port is its exit, so a single-buffer fan-out must NOT fire E4101.
#[test]
fn dlv_drvconf__own_out_port_is_not_second_driver() {
    let src = format!(
        "{BUF}module SUB {{\n    out Y\n    BUF b1\n    b1.Y -> Y\n}}\nmodule main {{\n    SUB s1\n    BUF b2\n    s1.Y -> b2.A\n}}"
    );
    assert!(
        driver_conflict_msgs(&src).is_empty(),
        "no false-positive E4101"
    );
}

/// Two different modules' out ports shorted on a parent net are two real
/// drivers — E4101 must keep firing (message names both ports).
#[test]
fn dlv_drvconf__two_submodule_outs_short_still_fires() {
    let src = format!(
        "{BUF}module SUB {{\n    out Y\n    BUF b1\n    b1.Y -> Y\n}}\nmodule main {{\n    SUB s1\n    SUB s2\n    s1.Y -> s2.Y\n}}"
    );
    assert_eq!(
        driver_conflict_msgs(&src),
        ["Net '_net0' has 2 drivers: main.s1.Y, main.s2.Y. Possible short circuit."]
    );
}

/// Two sibling out ports of one instance shorted at the parent: flatten does
/// not materialize a parent net between two ports of the *same* instance —
/// each internal net keeps its single driver plus its own exit port, so no net
/// carries two drivers and E4101 stays silent (the short of the two interior
/// buffers is a cross-net join the flat model does not fuse).
#[test]
fn dlv_drvconf__two_sibling_out_ports_join_is_not_one_net() {
    let src = format!(
        "{BUF}module SUB {{\n    out Y1\n    out Y2\n    BUF a\n    BUF b\n    a.Y -> Y1\n    b.Y -> Y2\n}}\nmodule main {{\n    SUB s1\n    s1.Y1 -> s1.Y2\n}}"
    );
    assert!(
        driver_conflict_msgs(&src).is_empty(),
        "each sibling internal net carries one driver + its exit port"
    );
}

/// Two buffers shorted *inside* one module still count as two drivers — the
/// out port is deduped but the interior pins are not.
#[test]
fn dlv_drvconf__interior_two_buffer_short_still_fires() {
    let src = format!(
        "{BUF}module SUB {{\n    out Y\n    BUF a\n    BUF b\n    a.Y -> Y\n    b.Y -> Y\n}}\nmodule main {{\n    SUB s1\n}}"
    );
    assert_eq!(
        driver_conflict_msgs(&src),
        ["Net 'Y' has 2 drivers: main.s1.a.2, main.s1.b.2. Possible short circuit."]
    );
}

/// A parent-net short between a module's out port and a sibling instance pin
/// is two real drivers — the port counts because the pin is not inside it.
#[test]
fn dlv_drvconf__module_out_shorts_sibling_pin_fires() {
    let src = format!(
        "{BUF}module SUB {{\n    out Y\n    BUF b1\n    b1.Y -> Y\n}}\nmodule main {{\n    SUB s1\n    BUF b0\n    s1.Y -> b0.Y\n}}"
    );
    assert_eq!(
        driver_conflict_msgs(&src),
        ["Net '_net0' has 2 drivers: main.s1.Y, main.b0.2. Possible short circuit."]
    );
}
