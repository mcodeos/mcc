// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U355: a chain end routes by its DECLARATION, never by the class the
//! declaration failed to resolve to.
//!
//! `net B` is not a legal declaration — `net` is no iotype keyword and no
//! class, so pass1 kept an `Unresolved` instance whose `get_name` used to
//! answer with the CLASS name. The chain end `B - ldo.1` then built the net
//! `net` with the point `net`, the instance name lost entirely. The variant
//! now carries `inst_name` next to `class_name`, the chain end lands on the
//! declared name, and the check face rules the declaration an unknown-class
//! error once the system library is loaded.

use crate::common;

use mcc::check::{extra::ExtraCheck, CheckAccumulator, ValidationCheck};
use mcc::{McIds, McURI};

const PRELUDE: &str = r#"
component LDO2(r) {
    pins = [
        1 = VIN, "input"
        2 = VOUT, "output"
        3 = GND, "ground"
    ]
}
module top {
"#;

fn netlines(src: &str) -> (Vec<String>, Vec<u32>) {
    let _lock = common::lock();
    common::reset();
    let uri = McURI::from("/mcc/u355-net-decl-chain.mc");
    mcc::mcc_load_from_string(&uri, src);
    let (_, table) = mcc::mcc_build_flat(&McIds::from("top"), &uri, 1000).expect("flat build");
    let mut lines: Vec<String> = Vec::new();
    for net in table.get_nets() {
        let mut pts: Vec<String> = Vec::new();
        for pid in net.points.iter() {
            if let Some(e) = table.get_entry(*pid) {
                pts.push(e.path.clone());
            }
        }
        pts.sort();
        lines.push(format!("{} <= [{}]", net.name, pts.join(", ")));
    }
    lines.sort();
    let codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    (lines, codes)
}

/// The chain end routes by the declaration: the net and its endpoint carry
/// the declared instance name `B`, never the unresolved class name `net`.
#[test]
fn u355__chain_end_names_the_declared_instance_not_the_class() {
    let (lines, codes) = netlines(&format!(
        "{PRELUDE}    net B\n    B - ldo.1\n    LDO2 ldo\n}}"
    ));
    assert_eq!(
        lines,
        vec!["B <= [top.B, top.ldo.1]".to_string()],
        "codes: {codes:?}"
    );
    assert!(
        codes.contains(&3157),
        "the unresolved class must still be flagged: codes: {codes:?}"
    );
}

/// With the system library loaded, the check face rules `net B` an
/// unknown-class ERROR; with nothing loaded the not-loaded reading stays a
/// warning. The chain-end misdirection never returns.
#[test]
fn u355__unknown_class_is_an_error_once_the_library_is_loaded() {
    let _lock = common::lock();
    let src = format!("{PRELUDE}    net B\n    B - ldo.1\n    LDO2 ldo\n}}");
    let severity_of = |code: u32| -> Vec<(String, String)> {
        let mut acc = CheckAccumulator::new();
        ExtraCheck.run_post_parse(&mut acc);
        acc.results
            .iter()
            .filter(|r| r.code == code)
            .map(|r| (format!("{:?}", r.severity), r.message.clone()))
            .collect()
    };

    // No system library: the honest reading stays the not-loaded warning.
    common::reset();
    let uri = McURI::from("/mcc/u355-unknown-class-nolib.mc");
    mcc::mcc_load_from_string(&uri, &src);
    let hits = severity_of(5256);
    assert!(
        hits.iter()
            .any(|(s, m)| s.contains("Warning") && m.contains("references class 'net'")),
        "no-library state must keep the warning: {hits:?}"
    );

    // System library loaded: an unknown class is an authoring error.
    mcc::mcc_init();
    let uri = McURI::from("/mcc/u355-unknown-class-lib.mc");
    mcc::mcc_load_from_string(&uri, &src);
    let hits = severity_of(5256);
    assert!(
        hits.iter()
            .any(|(s, m)| s.contains("Error") && m.contains("unknown class")),
        "loaded-library state must report the unknown-class error: {hits:?}"
    );
    common::reset();
}

/// An undeclared bare chain end keeps the existing E3136 machine and names
/// its point after the written name — the honest floating-label treatment.
#[test]
fn u355__undeclared_bare_chain_end_stays_the_e3136_floating_label() {
    let (lines, codes) = netlines(&format!(
        "{PRELUDE}    VIN - ldo.2\n    LDO2 ldo\n}}"
    ));
    assert_eq!(
        lines,
        vec!["VIN <= [top.VIN, top.ldo.2]".to_string()],
        "codes: {codes:?}"
    );
    assert!(
        codes.contains(&3136),
        "the bare undeclared name must keep its floating-label report: codes: {codes:?}"
    );
}

/// A healthy declared-port chain end is untouched: the net names the port and
/// neither the class nor the floating-label machine fires.
#[test]
fn u355__healthy_declared_chain_end_is_untouched() {
    let (lines, codes) = netlines(&format!(
        "{PRELUDE}    io A\n    A - ldo.1\n    LDO2 ldo\n}}"
    ));
    assert_eq!(
        lines,
        vec!["A <= [top.A, top.ldo.1]".to_string()],
        "codes: {codes:?}"
    );
    assert!(!codes.contains(&3136), "codes: {codes:?}");
    assert!(!codes.contains(&3157), "codes: {codes:?}");
    assert!(!codes.contains(&5256), "codes: {codes:?}");
}
