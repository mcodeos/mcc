// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Integration tests for the mcode standard library auto-load policy
//! (§19.5 rule 1 of use-design.md).
//!
//! mcode loads automatically in every mode unless `libs.disable_mcode: true`
//! is set in the project or global config.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

/// Run `mcc parse` on a project directory and return the `result` envelope.
fn parse_project(dir: &PathBuf) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .args([
            "--local",
            "parse",
            dir.to_str().expect("dir path"),
            "--pass1",
            "-f",
            "json",
        ])
        .output()
        .expect("run JSON parse on project dir");
    assert!(
        output.status.success(),
        "mcc parse failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let envelope: Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("invalid JSON: {e}\n{stdout}"));
    envelope["result"].clone()
}

fn temp_project(name: &str, with_disable_mcode: bool) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mcc-autoload-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp project dir");
    let config = if with_disable_mcode {
        "\n[config.libs]\ndisable_mcode = true\n"
    } else {
        ""
    };
    std::fs::write(
        dir.join("project.toml"),
        format!(
            "[project]\nname = \"autoload\"\nversion = \"1.0.0\"\nentry = \"main.mc\"\ntop_module = \"main\"\n{config}"
        ),
    )
    .expect("write project.toml");
    std::fs::write(
        dir.join("main.mc"),
        "module main {\n    VIN -> RES(10kOhm) -> GND\n}\n",
    )
    .expect("write main.mc");
    dir
}

fn component_names(result: &Value) -> Vec<String> {
    result["pass1"]["definitions"]["components"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c["name"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn has_mcode_loaded_files(result: &Value) -> bool {
    result["pass1"]["loaded_files"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .any(|f| f["uri"].as_str().is_some_and(|u| u.contains("/mcode/")))
        })
        .unwrap_or(false)
}

/// Default: mcode auto-loads, so `RES` resolves and mcode files are loaded.
#[test]
fn cli_mcode__loads_by_default() {
    let dir = temp_project("default", false);
    let result = parse_project(&dir);

    let comps = component_names(&result);
    assert!(
        comps.iter().any(|n| n == "RES"),
        "RES from mcode should be registered by default, got {} components: {comps:?}",
        comps.len()
    );
    assert!(
        has_mcode_loaded_files(&result),
        "mcode library files should be listed in loaded_files"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// `libs.disable_mcode: true` is the only opt-out: mcode is not loaded,
/// so `RES` is unresolved and no mcode files appear.
#[test]
fn cli_mcode__disable_skips_mcode() {
    let dir = temp_project("disabled", true);
    let result = parse_project(&dir);

    let comps = component_names(&result);
    assert!(
        !comps.iter().any(|n| n == "RES"),
        "RES must not resolve when mcode is disabled, got {} components: {comps:?}",
        comps.len()
    );
    assert!(
        !has_mcode_loaded_files(&result),
        "no mcode library files should be loaded when disabled"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
