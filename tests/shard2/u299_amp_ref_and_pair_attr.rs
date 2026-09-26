// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! CIMP U299 lock ② and the U310 retirement lock.
//!
//! ① (U310) `&ID` / `&[a, b]` param prefix — retired at the grammar level:
//!    the mc_pard amp productions are gone (U299 had already collapsed every
//!    ref/copy distinction), so both spellings are syntax errors that
//!    register no formal. Locked A/B against the bare-ids control, which
//!    must still parse clean and register the formal.
//!
//! ② `uv@uv` scalar attribute — `rate = 1Mbps@0.5m` folds to
//!    AttrExpr::UnitValueAt, the shape the list form already carries.
//!    E3022 (node_type=118) is retired at this site; the lock reads the
//!    parse view, where the attribute must carry its written value, not an
//!    empty slot. This re-homes the U300 corpus lock I5 that lived in
//!    shard3/u300_corpus_locks.rs, which asserted the old E3022 firing
//!    here and flips to absence under U299②.

#![allow(non_snake_case)]

use serde_json::Value;
use std::process::Command;

fn parse_args(args: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .args(args)
        .output()
        .expect("run mcc");
    assert!(
        output.status.success(),
        "mcc exited {:?}; stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("mcc JSON output")
}

fn pass0_codes(value: &Value) -> Vec<u64> {
    value["result"]["pass0"]["diagnostics"]
        .as_array()
        .expect("pass0 diagnostics")
        .iter()
        .filter_map(|d| d["code"].as_u64())
        .collect()
}

// ① `&sense` / `&[a, b]` must be syntax errors that register no formal;
// the bare spellings stay the alive control.
#[test]
fn lock_u310__amp_param_prefix_is_a_syntax_error() {
    let bare = r#"component CUT(sense)
{
    p = 1
}
"#;
    let amp_id = r#"component CUT(&sense)
{
    p = 1
}
"#;
    let amp_vec = r#"component CUT(&[a, b])
{
    p = 1
}
"#;
    let result = parse_args(&[
        "parse",
        "--code",
        bare,
        "--local",
        "--pass1",
        "--pass2",
        "--top",
        "main",
        "-f",
        "json",
    ]);
    let codes = pass0_codes(&result);
    assert!(
        !codes.contains(&3103),
        "bare control: E3103 must stay retired at the declare site; codes: {codes:?}"
    );
    assert_eq!(
        result["result"]["summary"]["errors"].as_u64(),
        Some(0),
        "bare control: snippet must parse without errors; diagnostics: {}",
        result["result"]["pass0"]["diagnostics"]
    );
    assert!(
        codes.contains(&5641),
        "bare control: the formal must register (never-used names `sense`); codes: {codes:?}"
    );

    for (tag, src) in [("amp-id", amp_id), ("amp-vec", amp_vec)] {
        let result = parse_args(&[
            "parse",
            "--code",
            src,
            "--local",
            "--pass1",
            "--pass2",
            "--top",
            "main",
            "-f",
            "json",
        ]);
        let codes = pass0_codes(&result);
        assert!(
            result["result"]["summary"]["errors"].as_u64().unwrap_or(0) >= 1,
            "{tag}: the retired `&` param prefix must be a syntax error; codes: {codes:?}"
        );
        assert!(
            !codes.contains(&5641),
            "{tag}: no formal may register for the retired spelling; codes: {codes:?}"
        );
    }
}

// ② The scalar `uv@uv` attribute carries its written value on the parse
// view — the paired-value form the list face already folds.
#[test]
fn lock_u299__scalar_pair_attr_carries_value() {
    let dir = std::env::temp_dir().join(format!("u299-{}-pair-attr.mc", std::process::id()));
    std::fs::write(&dir, "component DEV\n{\n    rate = 1Mbps@0.5m\n}\n").expect("write fixture");
    let path = dir.to_string_lossy().into_owned();
    let result = parse_args(&["parse", &path, "--local", "--ast", "-f", "json"]);
    let codes = pass0_codes(&result);
    assert!(
        !codes.contains(&3022),
        "E3022 must stay retired at the scalar attr site; codes: {codes:?}"
    );
    let data = result["result"]["view"]["data"]
        .as_array()
        .expect("ast view data");
    let dev = data
        .iter()
        .find(|c| c["name"] == "DEV")
        .expect("component DEV on the view");
    let attrs = dev["attrs"].as_array().expect("attrs");
    let rate = attrs
        .iter()
        .find(|a| a["key"] == "rate")
        .expect("attr `rate` present");
    assert_eq!(
        rate["value"].as_str(),
        Some("1Mbps@0.5m"),
        "scalar `uv@uv` must survive into the attribute value"
    );
    let _ = std::fs::remove_file(&dir);
}
