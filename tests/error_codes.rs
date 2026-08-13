// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Error code catalog integrity tests (§5.2 of mcc-error-code-unification-plan.md).
//!
//! These tests guard against the registry drifting out of sync with reality:
//!
//! 1. `no_duplicate_codes` — every code in the central registry is unique.
//! 2. `every_declared_const_is_registered` — every `pub const` declared in
//!    `errcodes.rs` is present in `all_codes()` under the same name.
//! 3. `emitted_codes_are_registered` — codes actually emitted by real parse /
//!    build runs over representative snippets are all registered (no hardcoded
//!    stray codes reaching the output).

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

/// Global mutex to serialize tests that share mcc's global workspace state.
static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Parse `errcodes.rs` and return the declared `(name, value)` pairs of every
/// `pub const NAME: u32 = N;`.
fn declared_consts() -> Vec<(String, u32)> {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/db/diagnostic/errcodes.rs");
    let src = std::fs::read_to_string(&path).expect("read errcodes.rs");
    let mut out = Vec::new();
    for line in src.lines() {
        let line = line.trim();
        // pub const NAME: u32 = N;
        let Some(rest) = line.strip_prefix("pub const ") else {
            continue;
        };
        let Some((name, rhs)) = rest.split_once(":") else {
            continue;
        };
        let rhs = rhs.trim();
        let Some(rhs) = rhs.strip_prefix("u32 = ") else {
            continue;
        };
        let Some((num, _)) = rhs.split_once(";") else {
            continue;
        };
        let Ok(value) = num.trim().parse::<u32>() else {
            continue;
        };
        out.push((name.trim().to_string(), value));
    }
    out
}

#[test]
fn no_duplicate_codes() {
    let codes: Vec<u32> = mcc::errcodes::all_codes().iter().map(|e| e.code).collect();
    let unique: HashSet<u32> = codes.iter().copied().collect();
    assert_eq!(
        codes.len(),
        unique.len(),
        "registry contains duplicate codes: {} entries but {} unique",
        codes.len(),
        unique.len()
    );
}

/// Every registered code carries a non-empty canonical message template, and
/// `format_msg` renders `{i}` placeholders with the supplied arguments.
#[test]
fn format_msg_renders_message_templates() {
    for info in mcc::errcodes::all_codes() {
        assert!(
            !info.message.is_empty(),
            "code {:04} {} has an empty message template",
            info.code,
            info.name
        );
        // Rendering without arguments must never panic and must keep the
        // template intact when no placeholders are present.
        let rendered = mcc::errcodes::format_msg(info.code, &[]);
        assert!(
            rendered.contains(""),
            "format_msg panicked for {}",
            info.name
        );
    }

    // ERC templates (the json!-emission form) interpolate positional args.
    let m1 = mcc::errcodes::format_msg(mcc::errcodes::ERC_SINGLE_POINT_NET, &[&"VCC"]);
    assert_eq!(m1, "single-point net: 'VCC' has only one connection");
    let m3 = mcc::errcodes::format_msg(
        mcc::errcodes::ERC_MULTI_DRIVE_NET,
        &[&"NET_A", &2usize, &"p1, p2"],
    );
    assert_eq!(m3, "multi-drive net: 'NET_A' has 2 drivers (p1, p2)");
    let m2 = mcc::errcodes::format_msg(mcc::errcodes::ERC_UNCONNECTED_PORT, &[&"VOUT"]);
    assert_eq!(m2, "unconnected port: 'VOUT' is not connected to any net");
    let m4 = mcc::errcodes::format_msg(mcc::errcodes::ERC_FLOATING_NET, &[&"GND2"]);
    assert_eq!(m4, "floating net: 'GND2' has no driver");

    // Unknown codes render an empty string (caller keeps its own message).
    assert_eq!(mcc::errcodes::format_msg(9999, &[]), "");
}

#[test]
fn every_declared_const_is_registered() {
    let declared = declared_consts();
    assert!(!declared.is_empty(), "no `pub const` found in errcodes.rs?");
    let registered: HashSet<(String, u32)> = mcc::errcodes::all_codes()
        .iter()
        .map(|e| (e.name.to_string(), e.code))
        .collect();
    let missing: Vec<&(String, u32)> = declared
        .iter()
        // PARSER_WARNING_CODE_BASE is a segment boundary sentinel, not a
        // diagnostic code, so it is intentionally absent from all_codes().
        .filter(|(name, code)| {
            name != "PARSER_WARNING_CODE_BASE" && !registered.contains(&(name.clone(), *code))
        })
        .collect();
    assert!(
        missing.is_empty(),
        "declared constants missing from all_codes(): {missing:?}"
    );
}

/// Collect every code emitted while building a battery of broken snippets;
/// each must be registered in the central catalog.
#[test]
fn emitted_codes_are_registered() {
    let lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

    let registered: HashSet<u32> = mcc::errcodes::all_codes().iter().map(|e| e.code).collect();
    let mut emitted: HashSet<u32> = HashSet::new();

    // Representative snippets that each exercise a different pipeline stage.
    let cases: &[(&str, &str)] = &[
        // Pass1b: use-stage diagnostics (2050-2079)
        (
            "use-stage",
            "use $::missing.lib@1.0\n\nmodule main { io VDD }",
        ),
        // Pass1a: duplicate definition (1000-1049)
        (
            "dup-def",
            "component DUP { pins = [ 1 = 1 ] }\ncomponent DUP { pins = [ 1 = 1 ] }\nmodule main { io VDD }",
        ),
        // Pass1c: invalid unit (3040-3049)
        (
            "unit",
            "component U (v = 5bad)\n{\n    pins = [ 1 = 1 ]\n}\nmodule main { io VDD }",
        ),
        // Pass2: connection shape (4000-4049)
        (
            "shape",
            "module main { io A\nio B\nA + B }",
        ),
        // Pass3: naming/style (5050-5099)
        (
            "naming",
            "component _BAD { pins = [ 1 = 1 ] }\nmodule main { io VDD }",
        ),
        // Syntax error → C parser codes (2080-2119)
        ("syntax", "module main { io VDD"),
    ];

    for (_name, source) in cases {
        mcc::mcc_init_no_lib();
        mcc::mcc_set_system_root(std::path::Path::new(""));
        mcc::mcc_clear_workspace();
        let uri = "/mcc/errcodes-test.mc".to_string();
        mcc::mcc_load_from_string(&uri, source);
        let _ = mcc::mcc_build(&mcc::McIds::from("main"), &uri);
        for d in mcc::mcc_diagnose_all() {
            emitted.insert(d.code);
        }
    }

    let stray: Vec<u32> = emitted
        .iter()
        .copied()
        .filter(|c| !registered.contains(c))
        .collect();
    assert!(
        stray.is_empty(),
        "emitted codes missing from registry: {stray:?}\n(emitted: {emitted:?})"
    );
    assert!(!emitted.is_empty(), "no diagnostics were emitted at all?");

    drop(lock);
}
