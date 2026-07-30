// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Schema validation for golden TOML files.
//!
//! Validates that each golden TOML file:
//! - Has all required fields
//! - Points referenced in nets exist in the comp table
//! - Pin numbers don't exceed the component's pin count
//! - Series annotations reference existing components

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, serde::Deserialize)]
struct GoldenMeta {
    module: String,
    nets: usize,
    components: usize,
}

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
struct GoldenComp {
    id: String,
    class: String,
    value: String,
    pins: usize,
    origin: String,
}

#[derive(Debug, serde::Deserialize)]
struct GoldenNet {
    name: String,
    points: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
struct GoldenUnconnected {
    pin: String,
    reason: String,
}

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
struct GoldenSeries {
    comp: String,
    is_series: bool,
}

#[derive(Debug, serde::Deserialize)]
struct GoldenModule {
    meta: GoldenMeta,
    #[serde(default)]
    comp: Vec<GoldenComp>,
    #[serde(default)]
    net: Vec<GoldenNet>,
    #[serde(default)]
    unconnected: Vec<GoldenUnconnected>,
    #[serde(default)]
    series: Vec<GoldenSeries>,
}

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/hbl")
}

fn load_golden(path: &std::path::Path) -> GoldenModule {
    let content = fs::read_to_string(path).expect("Failed to read golden file");
    toml::from_str(&content).expect("Failed to parse golden TOML")
}

/// Parse a point reference like "usbsock.1" or "vin.GND" into (comp_id, pin).
/// Returns None for port references (no dot before the pin number).
fn parse_point(point: &str) -> Option<(&str, &str)> {
    // Find the last dot that precedes a numeric pin
    // Port references like "vin.GND" don't have numeric pins
    if let Some(dot) = point.rfind('.') {
        let after = &point[dot + 1..];
        if after.chars().all(|c| c.is_ascii_digit()) {
            let before = &point[..dot];
            return Some((before, after));
        }
    }
    None
}

#[test]
fn test_golden_schema_all_files_present() {
    let dir = golden_dir();
    let expected = [
        "POWER_USB.golden.toml",
        "POWER_LDO.golden.toml",
        "POWER_DCDC.golden.toml",
        "US513.golden.toml",
        "MIC_SIP.golden.toml",
        "SPEAKER_M.golden.toml",
        "main.golden.toml",
    ];
    for name in &expected {
        let path = dir.join(name);
        assert!(path.exists(), "Missing golden file: {name}");
    }
}

#[test]
fn test_golden_schema_required_fields() {
    let dir = golden_dir();
    let mut total_nets = 0;
    let mut total_comps = 0;

    for entry in fs::read_dir(&dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if !path.extension().map_or(false, |e| e == "toml") {
            continue;
        }

        let m = load_golden(&path);

        // Verify meta fields
        assert!(
            !m.meta.module.is_empty(),
            "missing module name in {:?}",
            path
        );
        assert!(m.meta.nets > 0, "nets must be > 0 in {:?}", path);
        assert!(
            m.meta.components > 0,
            "components must be > 0 in {:?}",
            path
        );

        // Verify comp list matches meta count
        assert_eq!(
            m.comp.len(),
            m.meta.components,
            "comp count mismatch in {:?}: meta says {} but found {}",
            path,
            m.meta.components,
            m.comp.len()
        );

        // Verify net list matches meta count
        assert_eq!(
            m.net.len(),
            m.meta.nets,
            "net count mismatch in {:?}: meta says {} but found {}",
            path,
            m.meta.nets,
            m.net.len()
        );

        // Verify each comp has required fields
        for c in &m.comp {
            assert!(!c.id.is_empty(), "comp missing id in {:?}", path);
            assert!(
                !c.class.is_empty(),
                "comp {} missing class in {:?}",
                c.id,
                path
            );
            assert!(
                !c.origin.is_empty(),
                "comp {} missing origin in {:?}",
                c.id,
                path
            );
            assert!(c.pins > 0, "comp {} pins must be > 0 in {:?}", c.id, path);
        }

        // Verify no duplicate comp ids
        let mut seen_ids = BTreeMap::new();
        for c in &m.comp {
            assert!(
                seen_ids.insert(&c.id, &path).is_none(),
                "duplicate comp id '{}' in {:?}",
                c.id,
                path
            );
        }

        // Verify each net has required fields
        for n in &m.net {
            assert!(!n.name.is_empty(), "net missing name in {:?}", path);
            assert!(
                !n.points.is_empty(),
                "net '{}' has no points in {:?}",
                n.name,
                path
            );
        }

        total_nets += m.meta.nets;
        total_comps += m.meta.components;
    }

    // Verify total counts
    assert_eq!(total_nets, 60, "total nets should be 60, got {total_nets}");
    assert_eq!(
        total_comps, 61,
        "total components should be 61, got {total_comps}"
    );
}

#[test]
fn test_golden_schema_point_references() {
    let dir = golden_dir();

    for entry in fs::read_dir(&dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if !path.extension().map_or(false, |e| e == "toml") {
            continue;
        }

        let m = load_golden(&path);

        // Build a map of comp_id -> max_pins
        let mut comp_pins: BTreeMap<&str, usize> = BTreeMap::new();
        for c in &m.comp {
            comp_pins.insert(&c.id, c.pins);
        }

        // Verify all points in nets reference valid comps and valid pin numbers
        for n in &m.net {
            for point in &n.points {
                if let Some((comp_id, pin_str)) = parse_point(point) {
                    // If comp_id is not in the comp table, it's a submodule reference
                    // (e.g., "mcu513.10" in main module). Skip pin validation for these.
                    if let Some(max_pins) = comp_pins.get(comp_id) {
                        // Check pin number is valid
                        let pin_num: usize = pin_str.parse().unwrap_or_else(|_| {
                            panic!("invalid pin number '{}' in {:?}", point, path)
                        });
                        assert!(
                            pin_num >= 1 && pin_num <= *max_pins,
                            "point '{}' has pin {} but comp '{}' only has {} pins in {:?}",
                            point,
                            pin_num,
                            comp_id,
                            max_pins,
                            path
                        );
                    }
                    // Submodule references (comp_id not in comp table) are valid
                }
                // Port references (like "vin.GND") don't need to be in comp table
            }
        }
    }
}

#[test]
fn test_golden_schema_series_references() {
    let dir = golden_dir();

    for entry in fs::read_dir(&dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if !path.extension().map_or(false, |e| e == "toml") {
            continue;
        }

        let m = load_golden(&path);

        let comp_ids: Vec<&str> = m.comp.iter().map(|c| c.id.as_str()).collect();

        for s in &m.series {
            assert!(
                comp_ids.contains(&s.comp.as_str()),
                "series references unknown comp '{}' in {:?}",
                s.comp,
                path
            );
        }
    }
}

#[test]
fn test_golden_schema_unconnected_pin_format() {
    let dir = golden_dir();

    for entry in fs::read_dir(&dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if !path.extension().map_or(false, |e| e == "toml") {
            continue;
        }

        let m = load_golden(&path);

        let comp_ids: Vec<&str> = m.comp.iter().map(|c| c.id.as_str()).collect();

        for u in &m.unconnected {
            assert!(
                !u.pin.is_empty(),
                "unconnected pin missing pin field in {:?}",
                path
            );
            assert!(
                !u.reason.is_empty(),
                "unconnected pin missing reason field in {:?}",
                path
            );

            // Check the pin's comp exists
            if let Some((comp_id, _)) = parse_point(&u.pin) {
                assert!(
                    comp_ids.contains(&comp_id),
                    "unconnected pin '{}' references unknown comp '{}' in {:?}",
                    u.pin,
                    comp_id,
                    path
                );
            }
        }
    }
}
