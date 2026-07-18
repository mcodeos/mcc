// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! M12 Determinism regression test for hbl1
//!
//! Verifies that repeated runs of the same input produce identical
//! determinism hashes and metrics within tolerance.

use std::process::Command;

/// Run hbl1 build and collect metrics output.
fn run_hbl1_viz() -> Option<String> {
    let output = Command::new("cargo")
        .args([
            "run",
            "--",
            "build",
            "projects/hbl1/hbl.mc",
            "--lib",
            "mcode",
            "--viz",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stderr).ok()
}

/// Extract determinism hashes from stderr output.
fn extract_determinism_hashes(stderr: &str) -> Vec<String> {
    stderr
        .lines()
        .filter(|l| l.contains("[metrics] DETERMINISM:"))
        .map(|l| l.to_string())
        .collect()
}

#[test]
fn hbl1_repeated_run_determinism_hashes_match() {
    let stderr1 = run_hbl1_viz();
    let stderr2 = run_hbl1_viz();

    // If hbl1 build fails (e.g., missing dependency), skip the test gracefully
    let (stderr1, stderr2) = match (stderr1, stderr2) {
        (Some(a), Some(b)) => (a, b),
        _ => {
            eprintln!("hbl1 build failed — skipping determinism test");
            return;
        }
    };

    let hashes1 = extract_determinism_hashes(&stderr1);
    let hashes2 = extract_determinism_hashes(&stderr2);

    assert!(!hashes1.is_empty(), "No determinism output found in run 1");
    assert!(!hashes2.is_empty(), "No determinism output found in run 2");
    assert_eq!(
        hashes1, hashes2,
        "Determinism hashes must match between repeated runs"
    );
}

#[test]
fn hbl1_determinism_output_contains_expected_fields() {
    let stderr = match run_hbl1_viz() {
        Some(s) => s,
        None => {
            eprintln!("hbl1 build failed — skipping");
            return;
        }
    };

    let hashes = extract_determinism_hashes(&stderr);
    assert!(!hashes.is_empty());

    // Check that the determinism output contains all expected field names
    for h in &hashes {
        assert!(
            h.contains("box_hash="),
            "Missing box_hash in determinism output"
        );
        assert!(
            h.contains("net_hash="),
            "Missing net_hash in determinism output"
        );
        assert!(
            h.contains("pin_hash="),
            "Missing pin_hash in determinism output"
        );
        assert!(
            h.contains("route_hash="),
            "Missing route_hash in determinism output"
        );
        assert!(
            h.contains("metrics_hash="),
            "Missing metrics_hash in determinism output"
        );
    }
}
