// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The clock family's intent profile row on `show stage viz` (U112 ③,
//! clock-intent-design.md §5): the second intent family publishes into the
//! same row shape the power family already uses — one item per family, the
//! member nets inside it, nothing when no net claims.
//!
//! What claims a net is the flat lane carry's declared shape, never a name:
//! a role declaring `exclusive = true` with a peer (the pairing shape, the
//! E4122 / 6054 anchor) or a role whose pins all declare one direction (the
//! unidirectional shape, the 6060 anchor). A mixed role (`out` beside `in`)
//! and a roleless adoption carry no shape, so their net stays unclaimed —
//! and a family with no members publishes no row at all (§5.2).
//!
//! The fixture is the purpose-built `tests/fixtures/clockline` project: two
//! pairing nets (crystal to MCU), one unidirectional net (clock source to
//! sink), one mixed/roleless net — and no power claim anywhere, so the power
//! row's absence is itself exercised.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

fn clockline_entry() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/clockline/src/clockline.mc")
}

fn run(cwd: &Path, args: &[&str]) -> (String, String, bool) {
    let out = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("run mcc");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

/// Run `mcc show stage viz -f json …` against the clockline project.
fn run_stage(extra: &[&str]) -> (String, String, bool) {
    let mut args = vec!["--local", "show", "stage", "viz", "-f", "json"];
    args.extend_from_slice(extra);
    args.push("-F");
    let entry = clockline_entry();
    args.push(entry.to_str().expect("fixture path"));
    run(Path::new("/tmp"), &args)
}

fn slice(extra: &[&str]) -> Value {
    let (stdout, stderr, ok) = run_stage(extra);
    assert!(ok, "show stage viz failed: {stderr}");
    let envelope: Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("invalid JSON: {e}\n{stdout}"));
    envelope["result"]["stage"].clone()
}

fn intent_rows(view: &Value) -> Vec<&Value> {
    view["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter(|i| i["class"] == "intent")
        .collect()
}

/// The net names the surviving rows address: digest and segment rows carry
/// the raw name (intent rows the member list).
fn drawn_names(view: &Value) -> std::collections::BTreeSet<String> {
    view["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter(|i| i["class"] == "digest" || i["class"] == "segment")
        .filter_map(|i| i["net"].as_str())
        .map(|s| s.to_string())
        .collect()
}

/// The whole fixture reading, once, for the tests that compare against it.
fn full_view() -> Value {
    slice(&[])
}

#[test]
fn the_clock_row_names_exactly_the_shaped_nets() {
    let view = full_view();
    let rows = intent_rows(&view);
    assert_eq!(rows.len(), 1, "one family claims here, so one row: {rows:?}");
    let row = rows[0];
    assert_eq!(row["family"], "clock-intent");
    let nets = row["nets"].as_array().expect("nets");
    assert_eq!(row["count"], 3, "two pairing nets, one unidirectional");
    let mut attrs: Vec<&str> = nets.iter().filter_map(|n| n["attr"].as_str()).collect();
    attrs.sort_unstable();
    assert_eq!(
        attrs,
        vec!["pairing", "pairing", "unidirectional"],
        "the attr is the declared shape, never a name: {attrs:?}"
    );
    // Every member carries the same record fields the power row uses.
    for n in nets {
        assert!(n["nid"].is_u64(), "member without a nid: {n}");
        assert!(n["layer"].is_string(), "member without a layer: {n}");
    }
}

#[test]
fn no_power_claim_publishes_no_power_row() {
    // §5.2: a family with no member nets publishes no item — the clockline
    // fixture declares no supply face anywhere, so the power row is absent
    // and the row list holds only the clock family.
    let view = full_view();
    let rows = intent_rows(&view);
    assert!(
        rows.iter().all(|r| r["family"] != "power-intent"),
        "no supply face is declared, so no power row may appear: {rows:?}"
    );
}

#[test]
fn the_clock_selector_keeps_exactly_the_claimed_nets() {
    let full = full_view();
    let row = intent_rows(&full)
        .into_iter()
        .next()
        .expect("the fixture must carry the clock row");
    let claimed: std::collections::BTreeSet<String> = row["nets"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|n| n["name"].as_str())
        .map(|s| s.to_string())
        .collect();

    // The mixed/roleless net is drawn but claimed by nobody: exactly one.
    let drawn = drawn_names(&full);
    let unclaimed: Vec<_> = drawn.difference(&claimed).collect();
    assert_eq!(
        unclaimed.len(),
        1,
        "exactly one net must sit outside every family: {unclaimed:?}"
    );

    let view = slice(&["--select", "intent=clock-intent"]);
    let kept = drawn_names(&view);
    assert_eq!(kept, claimed, "the selector keeps precisely the members");
    assert!(intent_rows(&view).len() == 1, "the row itself survives");
}

#[test]
fn the_power_selector_finds_no_member_here_and_dies_loudly() {
    // The other family's selector is a real selector, not a silent empty:
    // with no supply face declared, it must miss loudly, not draw nothing.
    let (stdout, stderr, ok) = run_stage(&["--select", "intent=power-intent"]);
    assert!(!ok, "an empty selection must not read as an empty drawing");
    assert!(
        stdout.contains("matches no drawn net") && stderr.contains("matches no drawn net"),
        "the envelope must carry the miss: {stdout} | {stderr}"
    );
}
