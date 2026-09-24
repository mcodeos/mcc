// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The `--select` / `--exclude` read face on `show stage viz` — the M1 query
//! line of view-model-design.md §6.8: the intent profile compiles into the
//! existing slice select (the net set the family claims), no new selection
//! vocabulary.
//!
//! The fixture is the real `tests/fixtures/hbl` project. Its `stage.viz`
//! reading carries exactly one `intent` row (`family: "power-intent"`, 21
//! member nets — `V1V2.VCC`, `V3V3.VCC`, `VCC_1V2`, `GND`, …), while nets like
//! `DAC_OUT` and `I2C0.SCL` are drawn but claimed by no family. Both sides of
//! every branch below are exercised on it.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

fn hbl_entry() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hbl/src/hbl.mc")
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

/// Run `mcc show stage viz -f json …` against the hbl project.
fn run_stage(extra: &[&str]) -> (String, String, bool) {
    let mut args = vec!["--local", "show", "stage", "viz", "-f", "json"];
    args.extend_from_slice(extra);
    args.push("-F");
    let entry = hbl_entry();
    args.push(entry.to_str().expect("fixture path"));
    run(Path::new("/tmp"), &args)
}

fn stage_of(stdout: &str) -> Value {
    let envelope: Value =
        serde_json::from_str(stdout).unwrap_or_else(|e| panic!("invalid JSON: {e}\n{stdout}"));
    envelope["result"]["stage"].clone()
}

fn slice(extra: &[&str]) -> Value {
    let (stdout, stderr, ok) = run_stage(extra);
    assert!(ok, "show stage viz failed: {stderr}");
    stage_of(&stdout)
}

fn class_counts(view: &Value) -> std::collections::BTreeMap<String, usize> {
    let mut counts = std::collections::BTreeMap::new();
    for item in view["items"].as_array().expect("items") {
        *counts
            .entry(item["class"].as_str().expect("class").to_string())
            .or_insert(0) += 1;
    }
    counts
}

/// The net names the surviving rows address: digest and segment rows carry the
/// raw name (pins carry the `net:` key form, intent rows the member list).
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
fn without_the_flags_the_view_is_byte_identical() {
    // The face must not touch the envelope when it is not used: the golden and
    // mirror locks upstream read this exact serialization.
    let (plain, _, _) = run_stage(&[]);
    let (again, _, _) = run_stage(&[]);
    assert_eq!(plain, again, "two plain runs must agree byte for byte");
    assert!(plain.contains("\"items\""), "expected the json face");
}

#[test]
fn the_intent_selector_keeps_exactly_the_claimed_nets() {
    let full = full_view();
    let intent_row = full["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["class"] == "intent")
        .expect("the fixture must carry an intent row")
        .clone();
    let claimed: Vec<&str> = intent_row["nets"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|n| n["name"].as_str())
        .collect();
    assert!(claimed.contains(&"GND"), "fixture shape drifted");

    let view = slice(&["--select", "intent=power-intent"]);
    let kept = drawn_names(&view);
    assert!(!kept.is_empty(), "nothing survived a real claim");
    for name in &kept {
        assert!(
            claimed.contains(&name.as_str()),
            "net {name} survived but no family claims it"
        );
    }
    // And the intent row itself survived with its full member list.
    let rows = class_counts(&view);
    assert_eq!(rows["intent"], 1);
    // Segments dropped with their nets: strictly fewer than the full reading.
    assert!(rows["segment"] < class_counts(&full)["segment"]);
    // The unclaimed names are gone.
    assert!(!kept.contains("DAC_OUT") && !kept.contains("I2C0.SCL"));
}

#[test]
fn name_selectors_address_nets_by_exact_glob_and_regex() {
    let view = slice(&["--select", "name=VCC_1V2"]);
    let kept = drawn_names(&view);
    assert!(kept.contains("VCC_1V2"));
    assert!(!kept.contains("VDD_3V3"));

    let view = slice(&["--select", "name=V*_3V3"]);
    let kept = drawn_names(&view);
    assert!(kept.contains("VDD_3V3"), "glob missed VDD_3V3: {kept:?}");
    assert!(!kept.contains("VCC_1V2"));

    let view = slice(&["--select", "name~='^V1V2'"]);
    let kept = drawn_names(&view);
    assert!(kept.contains("V1V2.VCC"), "regex missed: {kept:?}");
    assert!(!kept.contains("V3V3.VCC"));
}

#[test]
fn exclude_subtracts_from_the_selection() {
    let both = slice(&["--select", "name=VCC_1V2", "--select", "name=VDD_3V3"]);
    assert!(drawn_names(&both).contains("VCC_1V2"));
    assert!(drawn_names(&both).contains("VDD_3V3"));

    let minus = slice(&[
        "--select",
        "name=VCC_1V2",
        "--select",
        "name=VDD_3V3",
        "--exclude",
        "name=VDD_3V3",
    ]);
    let kept = drawn_names(&minus);
    assert!(kept.contains("VCC_1V2"));
    assert!(!kept.contains("VDD_3V3"));
}

#[test]
fn a_misspelled_selection_dies_loudly() {
    let (stdout, stderr, ok) = run_stage(&["--select", "name=NO_SUCH_NET_HERE"]);
    assert!(!ok, "an empty selection must not read as an empty drawing");
    // The json face reports through the error envelope, not a stage payload.
    assert!(
        stdout.contains("matches no drawn net"),
        "the envelope must carry the miss: {stdout}"
    );
    assert!(
        stderr.contains("matches no drawn net"),
        "the miss must say what failed: {stderr}"
    );
}

#[test]
fn only_the_viz_segment_accepts_the_flags() {
    let (_, stderr, ok) = run(Path::new("/tmp"), &[
        "--local",
        "show",
        "stage",
        "p1",
        "--select",
        "name=GND",
        "-f",
        "json",
        "-F",
        hbl_entry().to_str().expect("fixture path"),
    ]);
    assert!(!ok);
    assert!(
        stderr.contains("only 'viz' accepts them"),
        "wrong refusal text: {stderr}"
    );
}

#[test]
fn a_bad_expression_names_the_flag_that_carried_it() {
    // `kind` is a real query field elsewhere, but net records carry no kind —
    // the slice rejects the vocabulary before evaluating it.
    let (_, stderr, ok) = run_stage(&["--select", "kind=wire"]);
    assert!(!ok);
    assert!(
        stderr.contains("bad --select 'kind=wire'"),
        "the error must name the flag and the expression: {stderr}"
    );
}
