// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Locks the module-body `expects` clause declaration face: a malformed row
//! fires E3082, and a module whose rows are all in the designed forms stays
//! silent. Storage face (rows on `McModule.expects`) is locked by the unit
//! tests in `src/semantic/module/expects.rs`.

use serde_json::Value;
use std::process::Command;

/// Run `mcc parse --code <source> --local --pass1 --pass2 --top main -f json`
/// and return the parsed JSON result.
fn parse(source: &str) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .args([
            "parse", "--code", source, "--local", "--pass1", "--pass2", "--top", "main", "-f",
            "json",
        ])
        .output()
        .expect("run mcc parse");
    assert!(
        output.status.success(),
        "mcc parse failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("parse mcc JSON output")
}

fn has_code(value: &Value, code: u64) -> bool {
    value["result"]["pass0"]["diagnostics"]
        .as_array()
        .expect("Pass 0 diagnostics")
        .iter()
        .any(|diagnostic| diagnostic["code"].as_u64() == Some(code))
}

// E3082 EXPECTS_ROW_MALFORMED: a row in none of the designed forms (here a
// bare number where a class/role word, `driven`, a window, or a range must
// stand) is reported and skipped.
#[test]
fn lock_module_expects__malformed_row_3082_fires() {
    let source = r#"module main
{
    expects = [
        bad = 42
    ]
}
"#;
    let result = parse(source);
    assert!(
        has_code(&result, 3082),
        "expected E3082 to fire; diagnostics: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

// A module whose expects rows are all in the designed forms emits no 3082.
#[test]
fn lock_module_expects__valid_rows_stay_silent() {
    let source = r#"module main
{
    expects = [
        u2 = REG.ADJ
        v1v2 = driven
        vout = [low:3.2V, high:3.4V]
        vout2 = 3.2V ~ 3.4V
    ]
}
"#;
    let result = parse(source);
    assert!(
        !has_code(&result, 3082),
        "E3082 fired on a valid expects clause; diagnostics: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}
