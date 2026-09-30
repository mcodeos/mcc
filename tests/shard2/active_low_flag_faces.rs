// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The §2.8 active-low flag on the readout faces (U365). A pin whose name
//! carries the `_` prefix sets `McPin.active_low` at registration; the flag
//! must surface wherever a consumer reads pins, but only where it is set —
//! a bare-spelled twin pin keeps the output byte-identical to the pre-U365
//! shape. Both faces here ride the `show pins` command: JSON (the field
//! appears only when true, the same conditional idiom as `attrs`) and text
//! (an `(active-low)` marker appended to the names cell).

use std::process::Command;

const SRC: &str = "component FLASH\n{\n    pins = [ in 1 = _CS\n             in 2 = WP ]\n}\n\nmodule main\n{\n    FLASH f\n}\n";

fn show_pins(format: &str) -> String {
    let path = std::env::temp_dir().join("mcc-active-low-show-pins.mc");
    std::fs::write(&path, SRC).expect("write fixture");
    let output = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .args([
            "show",
            "pins",
            "FLASH",
            "-F",
            path.to_str().expect("utf-8 path"),
            "-f",
            format,
        ])
        .output()
        .expect("run mcc show pins");
    assert!(
        output.status.success(),
        "mcc show pins exited {:?}; stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf-8 stdout")
}

#[test]
fn alow__json_face_sets_the_flag_only_where_prefixed() {
    let value: serde_json::Value = serde_json::from_str(&show_pins("json"))
        .expect("show pins JSON output");
    let pins = value["result"]["show"]["pins"].as_array().expect("pins");
    let cs: Vec<&serde_json::Value> = pins.iter().filter(|p| p["id"] == "1").collect();
    let wp: Vec<&serde_json::Value> = pins.iter().filter(|p| p["id"] == "2").collect();
    assert_eq!(cs.len(), 1, "pin 1 must be in the dump: {value}");
    assert_eq!(wp.len(), 1, "pin 2 must be in the dump: {value}");
    assert_eq!(
        cs[0]["active_low"].as_bool(),
        Some(true),
        "the `_CS` row must carry the flag: {}",
        cs[0]
    );
    assert!(
        wp[0].get("active_low").is_none(),
        "the bare-spelled row must not gain the field: {}",
        wp[0]
    );
}

#[test]
fn alow__text_face_marks_the_names_cell() {
    let text = show_pins("text");
    let line = text
        .lines()
        .find(|l| l.contains("_CS"))
        .expect("_CS row in text table");
    assert!(
        line.contains("_CS (active-low)"),
        "marker must ride the names cell: {line}"
    );
    assert!(
        !text.lines().filter(|l| l.contains("WP")).any(|l| l.contains("active-low")),
        "the bare-spelled row stays unmarked: {text}"
    );
}
