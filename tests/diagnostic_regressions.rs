use serde_json::Value;
use std::fs;
use std::process::Command;

fn fixture_path() -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("mcc-diagnostic-regression-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("create fixture directory");
    let path = dir.join("power-net.mc");
    fs::write(
        &path,
        r#"
component SOURCE
{
    name = "Source"
    voltage = 5V
    pins = [
        ps 1 = VCC
        ps 2 = GND
    ]
}

module main
{
    SOURCE PWR
    PWR.1 -> V5V
    PWR.2 -> GND
}
"#,
    )
    .expect("write fixture");
    path
}

#[test]
fn valid_power_net_has_no_false_instance_diagnostics() {
    let path = fixture_path();
    let workdir = path.parent().expect("fixture parent");
    let output = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .current_dir(workdir)
        .args([
            "parse",
            path.to_str().expect("fixture path"),
            "--pass1",
            "--pass2",
            "--top",
            "main",
            "-f",
            "json",
        ])
        .output()
        .expect("run JSON parse");
    assert!(output.status.success());

    let envelope: Value = serde_json::from_slice(&output.stdout).expect("parse JSON output");
    let result = &envelope["result"];
    assert_eq!(result["summary"]["errors"], 0);
    assert_eq!(result["summary"]["warnings"], 0);
    let diagnostics = result["pass0"]["diagnostics"]
        .as_array()
        .expect("Pass 0 diagnostics");
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| { !matches!(diagnostic["code"].as_u64(), Some(2202 | 2403 | 2606)) }),
        "unexpected instance diagnostics: {}",
        result["pass0"]["diagnostics"]
    );

    fs::remove_dir_all(workdir).expect("remove fixture directory");
}

#[test]
fn unresolved_instance_still_reports_missing_class() {
    let source = r#"
module main
{
    MISSING_PART U_MISSING
    U_MISSING.1 -> OUT
}
"#;
    let output = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .args([
            "parse", "--code", source, "--pass1", "--pass2", "--top", "main", "-f", "json",
        ])
        .output()
        .expect("run unresolved-instance parse");
    assert!(output.status.success());

    let envelope: Value = serde_json::from_slice(&output.stdout).expect("parse JSON output");
    let diagnostics = envelope["result"]["pass0"]["diagnostics"]
        .as_array()
        .expect("Pass 0 diagnostics");
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic["code"] == 2606
                && diagnostic["message"]
                    .as_str()
                    .is_some_and(|message| message.contains("MISSING_PART"))
        }),
        "missing unresolved-class diagnostic: {}",
        envelope["result"]["pass0"]["diagnostics"]
    );
}
