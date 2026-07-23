use serde_json::Value;
use std::fs;
use std::process::Command;

fn fixture_path() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mcc-net-report-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("create fixture directory");
    let path = dir.join("shared-pin.mc");
    fs::write(
        &path,
        r#"
component ONE_PIN
{
    pins = [
        io 1 = NODE
    ]
}

module main
{
    ONE_PIN U_LEFT
    ONE_PIN U_JOIN
    ONE_PIN U_RIGHT

    U_LEFT.NODE -> U_JOIN.NODE
    U_JOIN.NODE -> U_RIGHT.NODE
}
"#,
    )
    .expect("write fixture");
    path
}

#[test]
fn text_and_json_report_the_union_find_merged_net() {
    let path = fixture_path();
    let workdir = path.parent().expect("fixture parent");

    let json_output = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .current_dir(workdir)
        .args([
            "parse",
            path.to_str().expect("fixture path"),
            "--pass1",
            "--pass2",
            "-f",
            "json",
        ])
        .output()
        .expect("run JSON parse");
    assert!(json_output.status.success());

    let envelope: Value = serde_json::from_slice(&json_output.stdout).expect("parse JSON output");
    let summary = &envelope["result"]["summary"];
    let nets = envelope["result"]["pass2"]["nets"]
        .as_array()
        .expect("Pass 2 nets");
    assert_eq!(summary["net_count"], 1);
    assert_eq!(nets.len(), 1);
    assert_eq!(nets[0]["points"].as_array().expect("net points").len(), 3);

    let text_output = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .current_dir(workdir)
        .args([
            "parse",
            path.to_str().expect("fixture path"),
            "--pass1",
            "--pass2",
        ])
        .output()
        .expect("run text parse");
    assert!(text_output.status.success());
    let text = String::from_utf8(text_output.stdout).expect("UTF-8 text output");
    assert!(text.contains("Module: main (1 nets: 1 connected, 0 stub)"));
    assert!(text.contains("U_LEFT.1 ~ U_JOIN.1 ~ U_RIGHT.1"));

    fs::remove_dir_all(workdir).expect("remove fixture directory");
}
