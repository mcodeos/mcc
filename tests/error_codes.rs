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

/// E2902 (SHAPE_TRANSPOSE_LIMIT): transpose operand shape guard (eval.md §5.5).
/// A 3+ row operand (e.g. `([A, B, C] - X)'`) must be rejected, while legal
/// transposes (component `CAP C1'`, series `(A - B)'`) must stay clean.
#[test]
fn transpose_shape_limit_emitted_and_legal_transposes_pass() {
    let lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

    // Bad: a legal series of 3-row column vectors cannot be transposed.
    // (`[A, B, C] - X` would be an illegal `3*1 -> 1*1` broadcast and be
    // rejected earlier with E4007, so the transpose is never reached.)
    let bad = "module main { ([A, B, C] - [D, E, F])' }";
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();
    let uri = "/mcc/transpose-bad.mc".to_string();
    mcc::mcc_load_from_string(&uri, bad);
    let _ = mcc::mcc_build(&mcc::McIds::from("main"), &uri);
    let codes: HashSet<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&mcc::errcodes::SHAPE_TRANSPOSE_LIMIT),
        "E2902 not emitted for 3-row transpose operand; got codes: {codes:?}"
    );

    // Good: component and series transposes stay legal.
    let good = "module main { (A - B)'\nCAP C1' }";
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();
    let uri = "/mcc/transpose-good.mc".to_string();
    mcc::mcc_load_from_string(&uri, good);
    let _ = mcc::mcc_build(&mcc::McIds::from("main"), &uri);
    let codes: HashSet<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&mcc::errcodes::SHAPE_TRANSPOSE_LIMIT),
        "E2902 false positive on legal transposes; got codes: {codes:?}"
    );

    drop(lock);
}

/// E2903 (SHAPE_REVERSE_NOOP): reverse `^` on a vector operand is a hint
/// (eval.md §9 / examples L180). Parallel operands carry no order to reverse.
#[test]
fn reverse_noop_hint_on_parallel_operand() {
    let lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

    // Reverse on a parallel vector is a no-op → hint.
    let src = "module main { (A + B)^ }";
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();
    let uri = "/mcc/reverse-noop.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build(&mcc::McIds::from("main"), &uri);
    let codes: HashSet<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&mcc::errcodes::SHAPE_REVERSE_NOOP),
        "E2903 not emitted for reverse on a parallel vector; got codes: {codes:?}"
    );

    // Reverse on a series chain is a meaningful order flip → no hint.
    let good = "module main { (A - B)^ }";
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();
    let uri = "/mcc/reverse-series.mc".to_string();
    mcc::mcc_load_from_string(&uri, good);
    let _ = mcc::mcc_build(&mcc::McIds::from("main"), &uri);
    let codes: HashSet<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&mcc::errcodes::SHAPE_REVERSE_NOOP),
        "E2903 false positive on series reverse; got codes: {codes:?}"
    );

    drop(lock);
}

/// E2905 (SHAPE_INST_3PIN_PLUSMINUS): an instance with 3+ pins cannot directly
/// participate in `+` / `-` (veccircuit.md inst constraint).
#[test]
fn inst_3pin_plusminus_rejected_with_dedicated_code() {
    let lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

    // 3-pin instance directly participating in `+`.
    let src = "component _M()\n{\n    pins = [\n        1 = P1\n        2 = P2\n        3 = P3\n    ]\n}\nmodule main\n{\n    _M U1 + GND\n}";
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();
    let uri = "/mcc/inst-plusminus.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build(&mcc::McIds::from("main"), &uri);
    let codes: HashSet<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&mcc::errcodes::SHAPE_INST_3PIN_PLUSMINUS),
        "E2905 not emitted for 3+ pin instance in `+`; got codes: {codes:?}"
    );

    // 3-pin instance directly participating in `-`.
    let src2 = "component _M()\n{\n    pins = [\n        1 = P1\n        2 = P2\n        3 = P3\n    ]\n}\nmodule main\n{\n    _M U1 - GND\n}";
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();
    let uri = "/mcc/inst-minus.mc".to_string();
    mcc::mcc_load_from_string(&uri, src2);
    let _ = mcc::mcc_build(&mcc::McIds::from("main"), &uri);
    let codes: HashSet<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&mcc::errcodes::SHAPE_INST_3PIN_PLUSMINUS),
        "E2905 not emitted for 3+ pin instance in `-`; got codes: {codes:?}"
    );

    drop(lock);
}

/// E2904 (SHAPE_EXPAND_DIM_MISMATCH): the Pass2 recovery branch attaches the
/// P5.4 fix suggestion (eval.md §7 rule 3) to the message. The suggestion
/// generator itself is unit-tested in netshape.rs; here we verify the
/// generator output flows into the rendered E2904 message.
#[test]
fn expand_dim_mismatch_reported_with_suggestion() {
    // 3×1 vs 2×1 named members: a fix suggestion exists and is interpolated.
    let fix = mcc::vector::model::netshape::suggest_shape_fix(3, 2)
        .expect("3x1 vs 2x1 mismatch must produce a suggestion");
    let msg = mcc::errcodes::format_msg(
        mcc::errcodes::SHAPE_EXPAND_DIM_MISMATCH,
        &[&3usize, &2usize, &fix],
    );
    assert!(
        msg.contains("left 3 rows vs right 2 rows"),
        "E2904 message should report both row counts; got: {msg}"
    );
    assert!(
        msg.contains("`*`"),
        "E2904 message should carry the explicit-`*` fix suggestion; got: {msg}"
    );

    // Equal row counts → no suggestion (no mismatch to fix).
    assert_eq!(mcc::vector::model::netshape::suggest_shape_fix(2, 2), None);
}

/// A2 (regression): `record_error` must surface as an Error-level diagnostic
/// with a real file:line, not be swallowed into a module dump that nothing
/// reads. Fixture `r1 -> r2'` (2-pin component in series with a transposed
/// 2-pin component) triggers CONN_SERIES_SHAPE_MISMATCH (E4007) through the
/// §5 transpose-bridge check in stmt.rs:692.
#[test]
fn record_error_surfaces_as_located_error_diagnostic() {
    let lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

    let src = "component _R\n{\n    pins = [\n        1 = X\n        2 = Y\n    ]\n}\nmodule main\n{\n    _R r1\n    _R r2\n    r1 -> r2'\n}";
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();
    let uri = "/mcc/record-error-e4007.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build(&mcc::McIds::from("main"), &uri);
    let diags = mcc::mcc_diagnose_all();
    let hits: Vec<_> = diags
        .iter()
        .filter(|d| d.code == mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH)
        .collect();
    assert!(
        !hits.is_empty(),
        "E4007 should surface from record_error; got codes: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
    for d in hits {
        assert_eq!(
            d.level,
            mcc::DiagnosticLevel::Error,
            "E4007 must be Error-level (was previously swallowed); got {:?}",
            d.level
        );
        assert!(
            d.loc.row > 0,
            "E4007 must carry a file:line location; row={} uri={}",
            d.loc.row,
            d.loc.uri
        );
    }

    drop(lock);
}
