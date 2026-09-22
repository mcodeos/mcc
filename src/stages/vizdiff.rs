// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `viz_diff` — structural diff of two `stage.viz` JSON faces.
//!
//! The AI edit loop's verifier: an agent changes the source, the pipeline
//! re-renders, and this diff says what the drawing *did* — which parts
//! appeared, vanished, or moved, and which nets changed their endpoint sets.
//! Identity follows the stage keys: a box is `(layer, inst_path)`, a net is
//! `(layer, net)`; geometry compares with a tolerance because a redraw can
//! wiggle the last decimal without moving anything an engineer would call
//! moved.

use serde_json::{json, Value};

/// Two positions farther apart than this count as "moved", not "redrawn".
const MOVE_EPS: f64 = 0.5;

/// Diff two stage item arrays into a change report.
pub fn diff_items(a: &[Value], b: &[Value]) -> Value {
    let boxes_a = boxes_by_key(a);
    let boxes_b = boxes_by_key(b);
    let mut added: Vec<Value> = Vec::new();
    let mut removed: Vec<Value> = Vec::new();
    let mut moved: Vec<Value> = Vec::new();
    for (k, (layer, path, at_a)) in &boxes_a {
        match boxes_b.get(k) {
            None => removed.push(json!({ "layer": layer, "path": path })),
            Some((_, _, at_b)) => {
                if (at_a.0 - at_b.0).abs() > MOVE_EPS || (at_a.1 - at_b.1).abs() > MOVE_EPS {
                    moved.push(json!({
                        "layer": layer,
                        "path": path,
                        "from": [at_a.0, at_a.1],
                        "to": [at_b.0, at_b.1],
                    }));
                }
            }
        }
    }
    for (k, (layer, path, _)) in &boxes_b {
        if !boxes_a.contains_key(k) {
            added.push(json!({ "layer": layer, "path": path }));
        }
    }

    let nets_a = nets_by_key(a);
    let nets_b = nets_by_key(b);
    let mut nets_added: Vec<Value> = Vec::new();
    let mut nets_removed: Vec<Value> = Vec::new();
    let mut nets_changed: Vec<Value> = Vec::new();
    for (k, (layer, net, ends_a)) in &nets_a {
        match nets_b.get(k) {
            None => nets_removed.push(json!({ "layer": layer, "net": net })),
            Some((_, _, ends_b)) => {
                if ends_a != ends_b {
                    nets_changed.push(json!({
                        "layer": layer,
                        "net": net,
                        "from": ends_a,
                        "to": ends_b,
                    }));
                }
            }
        }
    }
    for (k, (layer, net, _)) in &nets_b {
        if !nets_a.contains_key(k) {
            nets_added.push(json!({ "layer": layer, "net": net }));
        }
    }

    json!({
        "boxes": {
            "added": added,
            "removed": removed,
            "moved": moved,
        },
        "nets": {
            "added": nets_added,
            "removed": nets_removed,
            "changed": nets_changed,
        },
    })
}

/// Diff two complete stage envelopes (the `{"items": [...]}` faces `mcc show
/// stage viz --format json` writes).
pub fn diff_stages(a: &Value, b: &Value) -> Value {
    let empty = Vec::new();
    let ia = a.get("items").and_then(|v| v.as_array()).unwrap_or(&empty);
    let ib = b.get("items").and_then(|v| v.as_array()).unwrap_or(&empty);
    diff_items(ia, ib)
}

type At = (f64, f64);

/// `(layer, inst_path) -> (layer, path, at)` for every box item.
fn boxes_by_key(items: &[Value]) -> std::collections::BTreeMap<String, (String, String, At)> {
    let mut out = std::collections::BTreeMap::new();
    for it in items {
        if it.get("class").and_then(|c| c.as_str()) != Some("box") {
            continue;
        }
        let (Some(layer), Some(path)) = (str_of(it, "layer"), str_of(it, "path")) else {
            continue;
        };
        let at = it
            .get("at")
            .and_then(|v| v.as_array())
            .map(|a| (num(a.first()), num(a.get(1))))
            .unwrap_or((0.0, 0.0));
        out.insert(format!("{layer}\u{1}{path}"), (layer, path, at));
    }
    out
}

/// `(layer, net) -> (layer, net, sorted endpoints)` from digest rows.
fn nets_by_key(items: &[Value]) -> std::collections::BTreeMap<String, (String, String, Vec<String>)> {
    let mut out = std::collections::BTreeMap::new();
    for it in items {
        if it.get("class").and_then(|c| c.as_str()) != Some("digest") {
            continue;
        }
        let (Some(layer), Some(net)) = (str_of(it, "layer"), str_of(it, "net")) else {
            continue;
        };
        let ends: Vec<String> = it
            .get("endpoints")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|e| e.as_str().map(String::from)).collect())
            .unwrap_or_default();
        out.insert(format!("{layer}\u{1}{net}"), (layer, net, ends));
    }
    out
}

fn str_of(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).map(String::from)
}

fn num(v: Option<&Value>) -> f64 {
    v.and_then(|x| x.as_f64()).unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn stage(items: Value) -> Value {
        json!({ "items": items })
    }

    #[test]
    fn identical_stages_diff_to_nothing() {
        let items = json!([
            { "class": "box", "layer": "main", "path": "main.R1", "at": [10.0, 20.0] },
            { "class": "digest", "layer": "main", "net": "V3V3", "endpoints": ["main.R1.1"] }
        ]);
        let d = diff_stages(&stage(items.clone()), &stage(items));
        assert_eq!(d["boxes"]["added"].as_array().unwrap().len(), 0);
        assert_eq!(d["nets"]["changed"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn a_moved_box_reports_from_and_to() {
        let a = stage(json!([
            { "class": "box", "layer": "main", "path": "main.R1", "at": [10.0, 20.0] }
        ]));
        let b = stage(json!([
            { "class": "box", "layer": "main", "path": "main.R1", "at": [50.0, 20.0] }
        ]));
        let d = diff_stages(&a, &b);
        let moved = d["boxes"]["moved"].as_array().unwrap();
        assert_eq!(moved.len(), 1);
        assert_eq!(moved[0]["from"][0], json!(10.0));
        assert_eq!(moved[0]["to"][0], json!(50.0));
    }

    #[test]
    fn net_endpoint_changes_are_named() {
        let a = stage(json!([
            { "class": "digest", "layer": "main", "net": "MID", "endpoints": ["main.R1.2"] }
        ]));
        let b = stage(json!([
            { "class": "digest", "layer": "main", "net": "MID", "endpoints": ["main.R1.2", "main.C1.1"] }
        ]));
        let d = diff_stages(&a, &b);
        let changed = d["nets"]["changed"].as_array().unwrap();
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0]["to"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn sub_pixel_wiggle_is_not_a_move() {
        let a = stage(json!([
            { "class": "box", "layer": "main", "path": "main.R1", "at": [10.0, 20.0] }
        ]));
        let b = stage(json!([
            { "class": "box", "layer": "main", "path": "main.R1", "at": [10.1, 20.0] }
        ]));
        let d = diff_stages(&a, &b);
        assert_eq!(d["boxes"]["moved"].as_array().unwrap().len(), 0);
    }
}
