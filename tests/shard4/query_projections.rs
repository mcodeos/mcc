// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy). Family:
// `queryproj` — extract→query merge projections (`query --kind net`,
// `--kind instance` inst_kind fidelity, `-f csv` raw rows; extract shim parity).
#![allow(non_snake_case)]

use serde_json::Value;
use std::fs;
use std::process::Command;

fn fixture_path() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mcc-queryproj-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("create fixture directory");
    let path = dir.join("probe.mc");
    fs::write(
        &path,
        r#"
component RES_10K
{
    pins = [
        1 = A
        2 = B
    ]
}

component CAP_1U
{
    pins = [
        1 = P
        2 = N
    ]
}

module main
{
    RES_10K R1
    RES_10K R2
    CAP_1U C1

    R1.A -> VCC
    R1.B -> MID
    R2.A -> MID
    R2.B -> GND
    C1.P -> VCC
    C1.N -> GND
}
"#,
    )
    .expect("write fixture");
    path
}

fn run(path: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_mcc"))
        .current_dir(path.parent().expect("fixture parent"))
        .arg("--local")
        .args(args)
        .output()
        .expect("run mcc")
}

fn run_json(path: &std::path::Path, args: &[&str]) -> Value {
    let mut args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    args.extend(["-f".into(), "json".into()]);
    let out = run(path, &args.iter().map(|s| s.as_str()).collect::<Vec<_>>());
    assert!(
        out.status.success(),
        "mcc {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("parse JSON output")
}

/// `query '' <file> --kind net -f json` returns envelope query rows
/// `{name, points}` for the top module's Pass2 nets.
#[test]
fn cli_queryproj__net_json_rows_are_name_points() {
    let path = fixture_path();
    let v = run_json(
        &path,
        &["query", "", path.to_str().unwrap(), "--kind", "net"],
    );
    let q = &v["result"]["query"];
    assert_eq!(q["count"], 3);
    let items = q["items"].as_array().expect("net items");
    let by_name: Vec<&Value> = items.iter().collect();
    for it in &by_name {
        let obj = it.as_object().expect("net row object");
        assert_eq!(
            obj.len(),
            2,
            "net row must be exactly {{name, points}}: {it}"
        );
        assert!(obj.contains_key("name"));
        assert!(obj.contains_key("points"));
    }
    let gnd = items
        .iter()
        .find(|it| it["name"] == "GND")
        .expect("GND net present");
    assert_eq!(gnd["points"], serde_json::json!(["R2.2", "GND", "C1.2"]));
}

/// Same fold as `extract nets`: the shim and `query --kind net` must project
/// byte-identical item graphs (no second engine).
#[test]
fn cli_queryproj__net_matches_extract_nets_items() {
    let path = fixture_path();
    let q = run_json(
        &path,
        &["query", "", path.to_str().unwrap(), "--kind", "net"],
    );
    let e = run_json(&path, &["extract", "nets", path.to_str().unwrap()]);
    assert_eq!(e["result"]["extract"]["target"], "nets");
    assert_eq!(
        q["result"]["query"]["items"],
        e["result"]["extract"]["items"]
    );
}

/// `-f csv` under `--kind net` is a real CSV projection (explicit columns,
/// `;`-joined points), not the envelope's text fall-through.
#[test]
fn cli_queryproj__net_csv_is_raw_projection() {
    let path = fixture_path();
    let out = run(
        &path,
        &[
            "query",
            "",
            path.to_str().unwrap(),
            "--kind",
            "net",
            "-f",
            "csv",
        ],
    );
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).expect("csv stdout");
    assert_eq!(stdout.lines().next(), Some("name,points"));
    assert_eq!(stdout.lines().count(), 4, "header + 3 nets");
    assert!(stdout.contains("\nGND,R2.2;GND;C1.2\n"));
    assert!(stdout.contains("\nMID,R1.2;MID;R2.1\n"));
    assert!(stdout.contains("\nVCC,R1.1;VCC;C1.1\n"));
    assert!(String::from_utf8(out.stderr)
        .expect("stderr")
        .contains("(3 items)"));
}

/// `--kind instance` (with `--top`) rows carry the per-instance kind tag in
/// `inst_kind`; every row is a drill row (kind="instance").
#[test]
fn cli_queryproj__instance_rows_carry_inst_kind() {
    let path = fixture_path();
    let v = run_json(
        &path,
        &[
            "query",
            "",
            path.to_str().unwrap(),
            "--top",
            "main",
            "--kind",
            "instance",
        ],
    );
    let items = v["result"]["query"]["items"]
        .as_array()
        .expect("instance items");
    assert_eq!(items.len(), 6, "R1 R2 C1 + labels VCC MID GND");
    for it in items {
        assert_eq!(it["kind"], "instance");
        assert!(
            it.get("inst_kind").is_some(),
            "drill row must tag inst_kind: {it}"
        );
        assert!(it.get("uri").is_some());
    }
    let by_name: std::collections::HashMap<&str, &Value> = items
        .iter()
        .map(|it| (it["name"].as_str().expect("name"), it))
        .collect();
    assert_eq!(by_name["R1"]["inst_kind"], "component");
    assert_eq!(by_name["R1"]["class"], "RES_10K");
    assert_eq!(by_name["C1"]["class"], "CAP_1U");
    assert_eq!(by_name["GND"]["inst_kind"], "label");
    assert_eq!(by_name["VCC"]["inst_kind"], "label");
}

/// Definition-kind hits (component/interface/...) never carry `inst_kind`.
#[test]
fn cli_queryproj__def_rows_have_no_inst_kind() {
    let path = fixture_path();
    // DSL pinned to the fixture's own def keeps the assertion free of any
    // installed system-library coupling.
    let v = run_json(
        &path,
        &[
            "query",
            "kind=component AND name=RES_10K",
            path.to_str().unwrap(),
        ],
    );
    let items = v["result"]["query"]["items"].as_array().expect("def items");
    assert_eq!(items.len(), 1);
    let it = &items[0];
    assert_eq!(it["kind"], "component");
    assert_eq!(it["name"], "RES_10K");
    assert_eq!(
        it.as_object().expect("def row object").len(),
        3,
        "def row must be exactly {{kind,name,uri}}: {it}"
    );
    assert!(it.get("inst_kind").is_none());
    assert!(it.get("class").is_none());
}

/// A DSL-shaped expression under `--kind net` is an error (nets are not defs;
/// never silently degraded to a substring search).
#[test]
fn cli_queryproj__dsl_expr_under_kind_net_errors() {
    let path = fixture_path();
    let out = run(
        &path,
        &[
            "query",
            "kind=component",
            path.to_str().unwrap(),
            "--kind",
            "net",
            "-f",
            "json",
        ],
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("DSL expressions do not apply to nets"),
        "stderr: {err}"
    );
}

/// Def/instance CSV uses explicit stable columns `kind,name,uri,class,inst_kind`
/// (not a key union — serde_json maps sort keys alphabetically).
#[test]
fn cli_queryproj__def_csv_columns_explicit() {
    let path = fixture_path();
    let out = run(
        &path,
        &[
            "query",
            "",
            path.to_str().unwrap(),
            "--kind",
            "component",
            "-f",
            "csv",
        ],
    );
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).expect("csv stdout");
    assert_eq!(stdout.lines().next(), Some("kind,name,uri,class,inst_kind"));
    // Every data line: two trailing empty cells for class/inst_kind.
    for line in stdout.lines().skip(1) {
        assert!(
            line.ends_with(",,"),
            "def csv line must end with empty class,inst_kind cells: {line}"
        );
        assert!(line.starts_with("component,"));
    }
}
