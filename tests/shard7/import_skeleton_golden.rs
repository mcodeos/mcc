// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Golden baseline for `import --skeleton` (mct.netlist/1 -> project skeleton).
//!
//! The generator's output is pinned byte-exact. The self-check gate (0 errors,
//! both diagnostic layers) runs inside `plan` — a generator regression that
//! produces non-compiling code fails **before** the snapshot diff. The fixture
//! carries the shapes each rule exists for: a `N$1` rename, a digit-leading
//! net, a two-pin inline cap/res with parsed values, a multi-pin block, a
//! domain-candidate label, and a designator a net must avoid.
//!
//! The generated header carries a generation date, so the date line is
//! normalized out (like `world_ver` in the other goldens).
//!
//! Regenerate (review the diff — a changed skeleton is a contract change):
//! `IMPORT_SKELETON_UPDATE=1 cargo test --test shard7 import_skeleton_golden`.

use crate::common;

const FIXTURE: &str = r#"{
  "meta": {
    "schema_version": "mct.netlist/1",
    "tool": "schdoc",
    "tool_tag": "schdoc geometric reconstruction",
    "source": "demo.SchDoc",
    "title": "demo",
    "attach_rate": 0.934
  },
  "components": [
    { "designator": "C1", "libref": "Cap 0402", "comment": "100nF 25V X7R" },
    { "designator": "R1", "libref": "RES 0402", "comment": "10k 1%" },
    { "designator": "U1", "libref": "TP4054", "comment": "TP4054" },
    { "designator": "5V1", "libref": "Cap 0603", "comment": "10uF" },
    { "designator": "U2", "libref": "LDO", "comment": "XC6206" }
  ],
  "nets": [
    { "name": null, "pins": [{"designator": "U1", "pin": "5"}, {"designator": "U2", "pin": "3"}], "power": [], "labels": [] },
    { "name": null, "pins": [{"designator": "C1", "pin": "1"}, {"designator": "U2", "pin": "2"}], "power": [], "labels": ["VCC"] },
    { "name": null, "pins": [{"designator": "C1", "pin": "2"}, {"designator": "5V1", "pin": "1"}, {"designator": "R1", "pin": "1"}], "power": ["GND"], "labels": [] },
    { "name": null, "pins": [{"designator": "5V1", "pin": "2"}, {"designator": "R1", "pin": "2"}], "power": [], "labels": ["5V"] },
    { "name": "N$7", "pins": [{"designator": "U1", "pin": "2"}], "power": [], "labels": [] }
  ],
  "dangle": ["[p1:demo] U1.1"]
}"#;

/// Build the plan and normalize the volatile date line out of main.mc.
/// The caller holds [`common::lock`]; the reset gives the in-engine selfcheck
/// a fresh workspace so no earlier test's `main.*` defs leak into the build.
fn build() -> mcc::import_skeleton::SkeletonPlan {
    common::reset();
    let plan = mcc::import_skeleton::plan(
        FIXTURE,
        &mcc::import_skeleton::Opts { name: Some("demo") },
    )
    .expect("plan succeeds (self-check 0 errors baked in)");
    plan
}

fn strip_volatile(mc: &str) -> String {
    mc.lines()
        .filter(|l| !l.starts_with("# generated: "))
        .collect::<Vec<_>>()
        .join("\n")
}

fn golden_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/import_skeleton.expected.mc")
}

#[test]
fn import_skeleton__golden_matches_baseline() {
    let _g = common::lock();
    let plan = build();
    let actual = strip_volatile(plan.files.get("main.mc").expect("main.mc"));

    let path = golden_path();
    if std::env::var("IMPORT_SKELETON_UPDATE").is_ok() {
        std::fs::write(&path, &actual).expect("write golden");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing golden — run `IMPORT_SKELETON_UPDATE=1 cargo test --test shard7 \
             import_skeleton_golden` to (re)generate: {e}"
        )
    });
    assert_eq!(
        actual, expected,
        "the generated skeleton drifted — inspect tests/golden/import_skeleton.expected.mc; \
         if deliberate, regenerate with IMPORT_SKELETON_UPDATE=1"
    );
}

/// The RPC face returns the same files the engine planned (no disk writes, no
/// drift between the two faces).
#[test]
fn import_skeleton__rpc_face_matches_engine() {
    let _g = common::lock();
    let plan = build();
    // RPC reads from disk — write the fixture to a scratch file first.
    let dir = std::env::temp_dir().join(format!("mcc-skel-test-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("fixture.netlist.json");
    std::fs::write(&path, FIXTURE).expect("write fixture");
    common::reset();
    let resp = mcc::rpc::handlers::handle_import_skeleton(Some(serde_json::json!({
        "file": path.to_string_lossy(),
        "name": "demo",
    })));
    let _ = std::fs::remove_dir_all(&dir);
    let v = resp.expect("rpc ok");
    let files = v.get("files").and_then(|f| f.as_object()).expect("files object");
    for (name, content) in &plan.files {
        assert_eq!(
            files.get(name).and_then(|c| c.as_str()),
            Some(content.as_str()),
            "RPC file '{name}' differs from the engine's plan"
        );
    }
}

/// net_ident rules, table-driven (T5): every rewrite leaves a trace,
/// collisions get a suffix, instances claim their names first.
#[test]
fn import_skeleton__net_ident_rules() {
    use mcc::import_skeleton as sk;
    let _g = common::lock();
    // direct engine-level expectations via a plan (fresh workspace per build —
    // the selfcheck build shares the global module tables with every other
    // lock-taking test in this binary)
    let mk = |json: &str| {
        common::reset();
        sk::plan(json, &sk::Opts { name: Some("t") }).expect("plan")
    };
    let fixture = |nets: &str, comps: &str| {
        format!(
            r#"{{"meta":{{"schema_version":"mct.netlist/1","tool":"schdoc","title":"t"}},
                "components":{comps},"nets":{nets}}}"#
        )
    };
    let comps = r#"[{"designator":"R1","libref":"RES","comment":"10k"}]"#;
    let nets = r#"[
        {"name":null,"pins":[{"designator":"R1","pin":"1"}],"power":[],"labels":["N$1"]},
        {"name":null,"pins":[{"designator":"R1","pin":"2"}],"power":[],"labels":["5V"]}
    ]"#;
    let plan = mk(&fixture(nets, comps));
    let mc = plan.files.get("main.mc").unwrap();
    assert!(mc.contains("in N_1 // was: N$1"), "N$1 -> N_1 rename leaves a trace");
    assert!(mc.contains("in V5V // was: 5V"), "digit-leading name gains the V prefix, with a trace");
    assert!(mc.contains("RES(10kΩ"), "10k parses into 10kΩ: {mc}");

    // A net name colliding with an instance designator: the net yields
    // (gains a prefix), the instance keeps its name.
    let nets2 = r#"[
        {"name":null,"pins":[{"designator":"R1","pin":"1"}],"power":[],"labels":["R1"]}
    ]"#;
    let plan2 = mk(&fixture(nets2, comps));
    let mc2 = plan2.files.get("main.mc").unwrap();
    assert!(
        !mc2.contains("in R1\n"),
        "a net name must not equal an instance designator: {mc2}"
    );
}

/// plan() bails on an unknown schema version — the contract gate.
#[test]
fn import_skeleton__schema_version_gate() {
    let bad = r#"{"meta":{"schema_version":"mct.netlist/9","tool":"schdoc"},"components":[],"nets":[]}"#;
    let err = mcc::import_skeleton::plan(bad, &mcc::import_skeleton::Opts::default())
        .expect_err("unknown schema_version must bail");
    assert!(err.contains("schema_version"), "{err}");
}
