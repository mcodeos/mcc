// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! M5 -- the difference between two `stage.viz` readings.
//!
//! `show stage viz` answers "what does this world look like". This answers "what
//! changed between these two worlds", in the projection family's `change` shape
//! (`{type, kind, id, delta}`), so a review -- human or automatic -- can state
//! what it expects and have the difference checked against that.
//!
//! # The alignment law is per class, not one rule
//!
//! The obvious rule is "align on `canon_key`, and an item without one cannot be
//! aligned". That rule is wrong, and measurably so: on `hbl`, 97 `metrics` items
//! and 10 `segment` items all publish `canon_key: null`, yet they are perfectly
//! alignable. A metric's identity is `(family, field, layer)`; an edge segment's
//! is the pair of endpoint paths. So each class states its own key below, and
//! "unaligned" means "this class's key function produced nothing" -- which, in
//! the current emission, only a `pin` can reach.
//!
//! The key is the canonical **path**, with the def compared as content rather
//! than folded into the key. Folding it in would read a module replacement as a
//! delete plus an add, when `module-replace` is a thing the projection family
//! names; and `def.uri` is a source path, so two builds of one source tree from
//! different directories would otherwise differ in every single item.
//!
//! # What is not an identity
//!
//! `key` (`D39`), `point` (`PointId`), `nid` and `index` are build-local
//! ordinals -- `D{}` is literally the `InstTable` row number, so inserting one
//! instance shifts every one of them. They are never keys here, and they are
//! excluded from content comparison, because comparing whole items would make a
//! single insertion read as "every box changed".
//!
//! # Segments have no id, and that has a consequence
//!
//! Segments are deliberately not issued a number (ruling O9), so a segment that
//! moved cannot be told apart from one deleted plus one added. "Rerouted" is
//! therefore read at the **layer** level -- the layer's segment set changed --
//! and not per segment; [`VizDiff::stability`] carries that count. A segment
//! whose key stayed put can still carry a content change (`lanes`, `length`),
//! which is reported as an ordinary `modify`.
//!
//! # Nets are references, not items
//!
//! There is no net item to add or remove, so a net's comings and goings are read
//! off the pins that reference it. Only nets with a source-authored name have a
//! key (`net:<name>`); a segment-minted (`~`) or anonymous (`_net<k>`) net
//! publishes `net: null` and only a build-local `nid`, so it **cannot** be
//! aligned across builds -- those are counted, never listed, because keying on
//! `nid` would be treating an in-build ordinal as an identity.
//!
//! # Scope
//!
//! This module produces data. The command face (`mcc diff <A> <B> --view
//! stage.*`) and the two-token envelope belong to ledger item U84 (4); nothing
//! here builds a `StageView`, because a difference belongs to two worlds and a
//! `StageView` carries one `world_ver`.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use crate::viz::stability::report::StabilityReport;

/// Separator between key components. `\u{1}` cannot appear in a canonical path
/// or a def ident, so the encoding stays injective.
const SEP: char = '\u{1}';

/// Separator between the members of one end's path list.
const LIST: char = '\u{2}';

/// How many unchanged boxes a reading needs before the share of them that moved
/// is worth reading at all: below this, "most of them moved" is one or two
/// boxes and says nothing.
pub const LOCALITY_MIN_SAMPLE: usize = 4;

/// The difference between two `stage.viz` readings.
#[derive(Debug, Clone, Default)]
pub struct VizDiff {
    /// The `change` items, sorted by `(kind, id)`.
    pub changes: Vec<Value>,
    /// Items whose class key function produced nothing, plus any key collision.
    /// Reported rather than guessed at: falling back to `key` or `name` is the
    /// name-as-identity move the view model forbids.
    pub unaligned: Vec<Value>,
    /// Pins referencing a net that has no cross-build key, on each side. A count
    /// and not a list, deliberately: listing them would claim an identity they
    /// do not have.
    pub nameless_net_pins: (usize, usize),
    /// The M12 stability summary. This is its first producer.
    pub stability: StabilityReport,
}

/// Compare two `stage.viz` item sets.
pub fn diff_stage_viz(a: &[Value], b: &[Value]) -> VizDiff {
    let mut unaligned = Vec::new();
    let left = index(a, "a", &mut unaligned);
    let right = index(b, "b", &mut unaligned);

    let mut changes = Vec::new();
    let mut routed_layers: BTreeSet<String> = BTreeSet::new();
    let mut stability = StabilityReport::default();

    for (key, va) in &left {
        match right.get(key) {
            None => {
                note_route(&mut routed_layers, va);
                changes.push(json!({
                    "type": "remove",
                    "kind": kind_of(va),
                    "id": id_of(va),
                }));
            }
            Some(vb) => {
                let fields = content_fields(va);
                if let Some(delta) = delta_of(va, vb, fields) {
                    note_route(&mut routed_layers, va);
                    changes.push(json!({
                        "type": "modify",
                        "kind": kind_of(va),
                        "id": id_of(va),
                        "delta": delta,
                    }));
                }
                if class(va) == "box" {
                    let content = delta_of(va, vb, BOX_CONTENT).is_none();
                    if content {
                        stability.unchanged_boxes_total += 1;
                        let at = delta_of(va, vb, &["at"]);
                        if at.is_some() {
                            stability.unchanged_boxes_moved += 1;
                            let d = shift(va, vb);
                            if d > stability.max_unchanged_box_delta {
                                stability.max_unchanged_box_delta = d;
                            }
                        }
                    }
                }
            }
        }
    }
    for (key, vb) in &right {
        if !left.contains_key(key) {
            note_route(&mut routed_layers, vb);
            changes.push(json!({
                "type": "add",
                "kind": kind_of(vb),
                "id": id_of(vb),
            }));
        }
    }

    // Nets, read off the pins that reference them.
    let (an, a_nameless) = net_refs(a);
    let (bn, b_nameless) = net_refs(b);
    for n in an.difference(&bn) {
        changes.push(json!({ "type": "remove", "kind": "net", "id": n }));
    }
    for n in bn.difference(&an) {
        changes.push(json!({ "type": "add", "kind": "net", "id": n }));
    }

    stability.route_hashes_changed = routed_layers.len();
    // Declarative, and the constants are named rather than folded into the
    // arithmetic: this is a judgement about locality, not a measurement.
    stability.locality_warning = stability.unchanged_boxes_total >= LOCALITY_MIN_SAMPLE
        && stability.unchanged_boxes_moved * 2 > stability.unchanged_boxes_total;

    changes.sort_by(|x, y| {
        let kx = (kind_of(x), x["id"].as_str().unwrap_or(""));
        let ky = (kind_of(y), y["id"].as_str().unwrap_or(""));
        kx.cmp(&ky)
    });

    VizDiff {
        changes,
        unaligned,
        nameless_net_pins: (a_nameless, b_nameless),
        stability,
    }
}

/// Note the layer a changed segment belongs to, for the layer-level reroute
/// count. A segment that appeared or vanished counts as much as one that
/// changed: all three mean the layer's routing is not what it was.
fn note_route(layers: &mut BTreeSet<String>, item: &Value) {
    if kind_of(item) != "segment" {
        return;
    }
    if let Some(l) = item.get("layer").and_then(Value::as_str) {
        layers.insert(l.to_string());
    }
}

/// Content fields of a box, excluding its position -- position is what
/// `unchanged_boxes_moved` is about, so a box that only moved is still
/// "unchanged" as far as this list is concerned.
const BOX_CONTENT: &[&str] = &[
    "def",
    "name",
    "class_name",
    "kind",
    "size",
    "pins",
    "anchors",
    "layer",
];

fn index(items: &[Value], side: &str, unaligned: &mut Vec<Value>) -> BTreeMap<String, Value> {
    let mut map: BTreeMap<String, Value> = BTreeMap::new();
    for it in items {
        if !is_known_class(it) {
            continue;
        }
        let Some(k) = key_of(it) else {
            unaligned.push(json!({
                "side": side,
                "class": class(it),
                "kind": it.get("kind").cloned().unwrap_or(Value::Null),
                "reason": "no-key",
                "item": it,
            }));
            continue;
        };
        if map.contains_key(&k) {
            // Two items claiming one key is a real shape (two routes between the
            // same pins), and O9 says mark it rather than let first-arrival win.
            unaligned.push(json!({
                "side": side,
                "class": class(it),
                "reason": "duplicate-key",
                "id": id_of(it),
            }));
            continue;
        }
        map.insert(k, it.clone());
    }
    map
}

fn class(item: &Value) -> &str {
    item.get("class").and_then(Value::as_str).unwrap_or("")
}

fn kind_of(item: &Value) -> &str {
    match class(item) {
        "metrics" => "metrics",
        c => c,
    }
}

fn is_known_class(item: &Value) -> bool {
    matches!(class(item), "box" | "layer" | "pin" | "segment" | "metrics")
}

/// The class key function. `None` means "this item cannot be aligned across
/// builds", never "fall back to something weaker".
fn key_of(item: &Value) -> Option<String> {
    let item_class = class(item);
    match item_class {
        // A box, layer or pin with a canonical instance path. `def` may be null
        // (the row exists but no class def was found) -- the path is still the
        // canonical path, so the key is still usable.
        "box" | "layer" | "pin" => canon_key_of(item).map(|k| format!("{item_class}{SEP}{k}")),
        // `canon_key` is always null for a metric, but `(family, field, layer)`
        // is its identity. `layer` is null throughout the current emission; it
        // is in the key anyway so a future per-layer reading cannot collide.
        "metrics" => {
            let f = item.get("family").and_then(Value::as_str);
            let field = item.get("field").and_then(Value::as_str);
            match (f, field) {
                (Some(f), Some(field)) => Some(format!(
                    "metrics{SEP}{f}{SEP}{field}{SEP}{}",
                    item.get("layer").and_then(Value::as_str).unwrap_or("")
                )),
                // The whole-family placeholder has `field: null`, which is still
                // an identity -- it is the family itself.
                (Some(f), None) => Some(format!("metrics{SEP}{f}{SEP}{SEP}")),
                _ => None,
            }
        }
        "segment" => segment_key(item).map(|k| format!("segment{SEP}{k}")),
        _ => None,
    }
}

/// Ruling O9: a segment is not issued a number, so its key is the ordered pair
/// of its endpoints' canonical paths.
///
/// An edge has ends and no coordinates; a wire has coordinates and no ends. That
/// is why the two take different keys rather than one: for a wire, geometry *is*
/// the identity, since there is nothing else to key on.
///
/// The endpoint paths arrive already sorted by `(path, point)` from `edge_end`;
/// the sort here is what keeps the difference depending on that, because a key
/// built from an unsorted list would read one route with its lanes in another
/// order as a delete plus an add.
fn segment_key(item: &Value) -> Option<String> {
    match item.get("kind").and_then(Value::as_str) {
        Some("edge") => {
            let from = end_paths(item.get("from")?)?;
            let to = end_paths(item.get("to")?)?;
            Some(format!(
                "edge{SEP}{}{SEP}{}",
                from.join(&LIST.to_string()),
                to.join(&LIST.to_string())
            ))
        }
        // A wire segment is keyed on where it runs, because it has no ends.
        Some("wire") => {
            let from = coord(item.get("from_at")?)?;
            let to = coord(item.get("to_at")?)?;
            Some(format!(
                "wire{SEP}{}{SEP}{from}{SEP}{to}",
                item.get("net").and_then(Value::as_str).unwrap_or("")
            ))
        }
        _ => None,
    }
}

/// The canonical **path** of an instance item, which is the alignment key.
///
/// The path alone, deliberately, and not with the def folded in. Two reasons,
/// both concrete:
///
/// * The path is already unique within a reading (measured: 63 boxes, 175 pins,
///   7 layers, no repeats), so the def adds nothing to alignment.
/// * Folding the def in would make a **module replacement** read as a delete plus
///   an add, when the projection family has a name for it (`module-replace`)
///   that only a `modify` can carry. The def is compared as content instead --
///   see [`field_value`].
///
/// `def.uri` is also a *source path*, so it differs between two builds of the
/// same source tree in different directories. That would be a total false
/// churn, and it is why the def is compared by `ident` and not by uri.
fn canon_key_of(item: &Value) -> Option<String> {
    let ck = item.get("canon_key")?;
    if ck.is_null() {
        return None;
    }
    let path = ck.get("path").and_then(Value::as_str)?;
    if path.is_empty() {
        return None;
    }
    Some(path.to_string())
}

/// One end's canonical paths, sorted. `None` when any member has no path -- an
/// end the table could not resolve, which O9 marks rather than joins.
fn end_paths(v: &Value) -> Option<Vec<String>> {
    let arr = v.as_array()?;
    if arr.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(arr.len());
    for e in arr {
        let p = e.get("path").and_then(Value::as_str)?;
        if p.is_empty() {
            return None;
        }
        out.push(p.to_string());
    }
    out.sort();
    Some(out)
}

fn coord(v: &Value) -> Option<String> {
    let a = v.as_array()?;
    if a.len() != 2 {
        return None;
    }
    Some(format!("{},{}", a[0].as_f64()?, a[1].as_f64()?))
}

/// The human-readable handle for a change row. This is for reading, not for
/// matching -- matching already happened on [`key_of`].
fn id_of(item: &Value) -> Value {
    match class(item) {
        "box" | "layer" | "pin" => match item.get("canon_key") {
            Some(ck) if !ck.is_null() => ck.get("path").cloned().unwrap_or(Value::Null),
            _ => Value::Null,
        },
        "metrics" => item.get("path").cloned().unwrap_or(Value::Null),
        "segment" => match item.get("kind").and_then(Value::as_str) {
            Some("edge") => {
                let f = end_paths(item.get("from").unwrap_or(&Value::Null)).unwrap_or_default();
                let t = end_paths(item.get("to").unwrap_or(&Value::Null)).unwrap_or_default();
                Value::String(format!("{} -> {}", f.join(","), t.join(",")))
            }
            _ => {
                let f = coord(item.get("from_at").unwrap_or(&Value::Null)).unwrap_or_default();
                let t = coord(item.get("to_at").unwrap_or(&Value::Null)).unwrap_or_default();
                Value::String(format!("{f} -> {t}"))
            }
        },
        _ => Value::Null,
    }
}

/// The fields compared for a matched pair, by class. Everything not listed here
/// is either the key, or a build-local ordinal.
///
/// `loc` is excluded on purpose: it carries the **source path**, so two builds of
/// an identical source from different directories would differ in every item.
/// This difference is about the drawing, and `loc` is the way back to the source,
/// not part of what is being compared.
///
/// `"def"` is a virtual field -- see [`field_value`].
fn content_fields(item: &Value) -> &'static [&'static str] {
    match (class(item), item.get("kind").and_then(Value::as_str)) {
        ("box", _) => &[
            "def",
            "name",
            "class_name",
            "kind",
            "size",
            "pins",
            "anchors",
            "at",
            "layer",
        ],
        ("pin", _) => &[
            "def", "num", "name", "io", "side", "offset", "at", "anchors", "box", "layer", "net",
        ],
        ("layer", _) => &[
            "def", "name", "style", "parent", "boxes", "nets", "edges", "segments", "canvas",
            "audited", "reports",
        ],
        ("metrics", _) => &["value"],
        // An edge has ends but no coordinates; a wire the reverse. The ends are
        // compared as **sorted paths**, exactly as the key builds them -- the
        // emitter sorts them, and comparing the raw arrays would read one route
        // with its lanes in another order as a changed route under an unchanged
        // key.
        ("segment", Some("edge")) => &[
            "edge_kind",
            "lanes",
            "trunk",
            "ret",
            "from_paths",
            "to_paths",
            "net",
        ],
        ("segment", _) => &["net", "from_at", "to_at", "length"],
        _ => &[],
    }
}

/// Read one compared field.
///
/// Three names are virtual:
///
/// * `"def"` resolves to the def **ident** rather than the whole `def` object, so
///   a build from a different directory (a different `def.uri`) does not read as
///   a module replacement.
/// * `"from_paths"` / `"to_paths"` resolve to a segment's sorted endpoint paths,
///   so the comparison agrees with the key about what a lane reorder is.
fn field_value(item: &Value, f: &str) -> Value {
    match f {
        "def" => item
            .get("canon_key")
            .and_then(|c| c.get("def"))
            .and_then(|d| d.get("ident"))
            .cloned()
            .unwrap_or(Value::Null),
        "from_paths" => sorted_paths(item, "from"),
        "to_paths" => sorted_paths(item, "to"),
        _ => item.get(f).cloned().unwrap_or(Value::Null),
    }
}

fn sorted_paths(item: &Value, side: &str) -> Value {
    match item.get(side).and_then(end_paths) {
        Some(ps) => Value::Array(ps.into_iter().map(Value::String).collect()),
        None => Value::Null,
    }
}

/// `Some(delta)` when any listed field differs, `None` when none does.
fn delta_of(a: &Value, b: &Value, fields: &[&str]) -> Option<Value> {
    let mut m = serde_json::Map::new();
    for f in fields {
        let va = field_value(a, f);
        let vb = field_value(b, f);
        if va != vb {
            m.insert((*f).to_string(), json!({ "from": va, "to": vb }));
        }
    }
    if m.is_empty() {
        None
    } else {
        Some(Value::Object(m))
    }
}

/// How far a box moved, in drawing units.
fn shift(a: &Value, b: &Value) -> f64 {
    let p = |v: &Value| -> Option<(f64, f64)> {
        let arr = v.get("at")?.as_array()?;
        Some((arr.first()?.as_f64()?, arr.get(1)?.as_f64()?))
    };
    match (p(a), p(b)) {
        (Some((x0, y0)), Some((x1, y1))) => ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt(),
        _ => 0.0,
    }
}

/// The nets referenced by pins, split into the ones that have a cross-build key
/// and a count of the pins on the ones that do not.
fn net_refs(items: &[Value]) -> (BTreeSet<String>, usize) {
    let mut named = BTreeSet::new();
    let mut nameless = 0usize;
    for it in items {
        if class(it) != "pin" {
            continue;
        }
        match it.get("net").and_then(Value::as_str) {
            Some(n) if !n.is_empty() => {
                named.insert(n.to_string());
            }
            // `net: null` with a `nid` present is a segment-minted or anonymous
            // net: it exists, but it has no name to be keyed on.
            _ => {
                if it.get("nid").map(|v| !v.is_null()).unwrap_or(false) {
                    nameless += 1;
                }
            }
        }
    }
    (named, nameless)
}
