// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Integration test: an `enum` and a `component` sharing the same base name in
// one file must coexist without DEF_ALREADY_EXISTS (1051, formerly E0501)
// (P0-3).
//
// Regression: `parse_cmie_names` collected all declaration names into a single
// list without tracking their types, so `enum CAP` + `component CAP` (as in
// mcode/cap.mc) triggered the duplicate-name error even though the design doc
// (same-name-enum-component.md §2.3) allows enum+component namespace merging.

use mcc::{McIds, McURI};
use std::sync::{Mutex, OnceLock};

/// Global mutex to serialize tests that share mcc's global workspace state.
static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Extract the `(start, end)` span from an `F12_DIAG` line.
fn extract_span(line: &str) -> Option<(usize, usize)> {
    let rest = line.split("span=[").nth(1)?;
    let comma = rest.find(',')?;
    let close = rest.find(']')?;
    let a: usize = rest[..comma].trim().parse().ok()?;
    let b: usize = rest[comma + 1..close].trim().parse().ok()?;
    Some((a, b))
}

/// The `id` of the LAPPER_REF interval whose kind tag and span match.
fn ref_interval(dump: &str, kind: &str, span: (usize, usize)) -> Option<u32> {
    dump.lines()
        .filter(|l| l.contains("F12_DIAG LAPPER_REF:"))
        .filter(|l| l.contains(&format!("kind={kind}")))
        .filter(|l| extract_span(l) == Some(span))
        .filter_map(|l| {
            l.find("id=")
                .and_then(|i| l[i + 3..].split_whitespace().next())
                .and_then(|s| s.parse().ok())
        })
        .next()
}

/// The def span from the MAP line `Ref(<kind>/<ku>, id=<ref_id>, ...) => Def(...)`.
fn map_def_span(dump: &str, kind: &str, ref_id: u32) -> Option<(usize, usize)> {
    dump.lines()
        .filter(|l| l.contains("F12_DIAG MAP:"))
        .filter(|l| l.contains(&format!("Ref({kind}/")) && l.contains(&format!("id={ref_id:5}")))
        .filter_map(|l| {
            let idx = l.find("=> Def")?;
            extract_span(&l[idx..])
        })
        .next()
}

/// The def name from the MAP line `... => Def(..., def_name='<name>', ...)`.
fn map_def_name(dump: &str, kind: &str, ref_id: u32) -> Option<String> {
    dump.lines()
        .filter(|l| l.contains("F12_DIAG MAP:"))
        .filter(|l| l.contains(&format!("Ref({kind}/")) && l.contains(&format!("id={ref_id:5}")))
        .filter_map(|l| {
            let idx = l.find("def_name='")?;
            let rest = &l[idx + "def_name='".len()..];
            let close = rest.find('\'')?;
            Some(rest[..close].to_string())
        })
        .next()
}

const SOURCE: &str = r#"
enum CAP { X7R, MLCC, C0G }

component CAP (diel = X7R)
{
    pins = [
        1 = 1
        2 = 2
    ]
}

module main
{
    io VDD
}
"#;

#[test]
fn enum_and_component_same_name_coexist() {
    let lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();

    let uri: McURI = "/mcc/same-name-cap.mc".to_string();
    mcc::mcc_load_from_string(&uri, SOURCE);
    let result = mcc::mcc_build(&McIds::from("main"), &uri);
    result.expect("build failed");

    // Both the enum and the component must be registered (no E0501 drop).
    let enum_cmie = mcc::get_def(&McIds::from("CAP"), &uri).expect("CAP definition missing");
    let comp = mcc::get_component_def(&McIds::from("CAP"), &uri)
        .expect("CAP component definition missing (E0501 suppressed both)");
    assert!(
        matches!(enum_cmie, mcc::McCMIE::Enum(_)),
        "CAP should resolve to the enum first"
    );
    assert!(
        matches!(comp, mcc::McCMIE::Component(_)),
        "get_component_def must return the component"
    );

    drop(lock);
}

#[test]
fn component_component_same_name_still_errors() {
    let lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();

    let uri: McURI = "/mcc/dup-component.mc".to_string();
    // Two components with the same name must still trigger DEF_ALREADY_EXISTS (1051).
    let source = r#"
component DUP { pins = [ 1 = 1 ] }
component DUP { pins = [ 1 = 1 ] }

module main
{
    io VDD
}
"#;
    mcc::mcc_load_from_string(&uri, source);
    let _ = mcc::mcc_build(&McIds::from("main"), &uri);

    let has_501 = mcc::mcc_diagnose_all().iter().any(|d| d.code == 1051);
    assert!(
        has_501,
        "duplicate component definitions must be reported as DEF_ALREADY_EXISTS"
    );

    drop(lock);
}

/// `diel = X7R` (bare enum value inside the component attr) must register an
/// EnumValRef whose packed value id (class id << 16 | value index) maps to the
/// exact `X7R` row inside `enum CAP` — the class id, not a name-only scan,
/// locates the value definition.
#[test]
fn scoped_enum_value_ref_lands_on_enum_value_def() {
    let lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();

    let uri: McURI = "/mcc/same-name-cap.mc".to_string();
    mcc::mcc_load_from_string(&uri, SOURCE);
    let dump = mcc::dump_symbols_f12_text(&uri).expect("f12 dump");

    // The `X7R` operand of `diel = X7R`.
    let ref_span = SOURCE
        .find("diel = X7R")
        .map(|p| (p + "diel = ".len(), p + "diel = X7R".len()))
        .expect("diel = X7R in source");
    let ref_id =
        ref_interval(&dump, "EnumValRef", ref_span).expect("EnumValRef interval for diel = X7R");

    // The ref must map to the enum value def named X7R, and the def must start
    // exactly at the `X7R` row inside `enum CAP` (the value row's end span is
    // a parser quirk on the first list element, not part of this resolution).
    let def_span = SOURCE
        .find("enum CAP { X7R")
        .map(|p| p + "enum CAP { ".len())
        .expect("enum CAP { X7R in source");

    let mapped = map_def_span(&dump, "EnumValRef", ref_id).expect("EnumValRef must map to a def");
    assert_eq!(
        mapped.0, def_span,
        "scoped enum value ref must land at the start of the enum value row"
    );
    assert_eq!(
        map_def_name(&dump, "EnumValRef", ref_id).as_deref(),
        Some("X7R"),
        "class-id-based value id must resolve to the X7R value, not a same-named row elsewhere"
    );

    drop(lock);
}
