// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! CIMP U299 locks, both grammar-alive / semantics-dead halves.
//!
//! ① `&ID` single-reference formal — the grammar's declare 2' arm wraps the
//!    ids in MCAST_OPD exactly as `&[a, b]` wraps its members, and mcc models
//!    no ref/copy difference, so the declare face reads it as a plain Single
//!    formal. Locked A/B against the bare-ids control: both spellings must
//!    register the formal (the never-used warning names it) and stay free
//!    of E3103.
//!
//! ② `uv@uv` scalar attribute — `rate = 1Mbps@0.5m` folds to
//!    AttrExpr::UnitValueAt, the shape the list form already carries.
//!    E3022 (node_type=118) is retired at this site; the lock reads the
//!    parse view, where the attribute must carry its written value, not an
//!    empty slot. This re-homes the U300 corpus lock I5 (formerly
//!    shard3/u300_corpus_locks.rs), which asserted the old E3022 firing
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

// ① `&sense` must land as the same formal as bare `sense`: identical parse
// view params, no E3103, no errors.
#[test]
fn lock_u299__amp_ref_formal_matches_bare_control() {
    let bare = r#"component CUT(sense)
{
    p = 1
}
"#;
    let amp = r#"component CUT(&sense)
{
    p = 1
}
"#;
    for (tag, src) in [("bare", bare), ("amp", amp)] {
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
            !codes.contains(&3103),
            "{tag}: E3103 must stay retired at the declare site; codes: {codes:?}"
        );
        assert_eq!(
            result["result"]["summary"]["errors"].as_u64(),
            Some(0),
            "{tag}: snippet must parse without errors; diagnostics: {}",
            result["result"]["pass0"]["diagnostics"]
        );
        assert!(
            codes.contains(&5641),
            "{tag}: the formal must register (never-used names `sense`); codes: {codes:?}"
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
