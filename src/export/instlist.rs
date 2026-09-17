// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The `inst-list` projection (build-design §3.7): one row per object of the
//! frozen circuit that carries an identity a consumer can join on, spelled the
//! way the two-space contract spells it, instead of re-deriving it from the
//! path string.
//!
//! Row shape (§3.7, extended to the point level by O16):
//!
//! ```text
//! { node, path, class: { def, key }, point, loc }
//! ```
//!
//! - `node` — the arena `NodeId`, the build-internal join handle. `null` on a
//!   pin row: a pin owns no arena node (its identity is the `PointId`).
//! - `path` — the canonical instance path. On a point row it already ends in
//!   the member name, so the row names its own site.
//! - `class` — `class.def` the world-local `DefId`, `class.key` the canonical
//!   def key `(uri, ident)` that survives across worlds and versions. A pin /
//!   port row names no def of its own (§3.7), so its class is the one it was
//!   flattened into — the same def half the `stage.*` views read for a point.
//! - `point` — the `PointId` (`node:member`) of a pin / port, `null` on an
//!   instance row. In-domain: valid within this build, and it shifts when an
//!   instance is inserted ahead of it, so it is a fast path beside
//!   `class.key`, never a replacement for it. **Not every pin / port row has
//!   one**: a port's aggregate and bus-member spellings are `Port` entries that
//!   name no physical point (§2.4 — an endpoint is not necessarily a point), so
//!   they print `null`. A row that names no point says so; none invents one.
//! - `loc` — the source site the flatten recorded for the object: the
//!   statement that wires a point, the declaration of an instance. A point with
//!   no wiring site falls back to its declaration, and an object with neither
//!   prints `null` (the text face prints `-`) rather than a plausible-looking
//!   line. `span` stays `null` — a flat position carries an offset and no
//!   extent, and a made-up end would be invented data.
//!
//! Rows come in two blocks, in this order: **instances** (`Module` /
//! `Component` — the rows this artifact has always emitted, in their original
//! order) then **points** (`Pin` / `Port`, appended by O16). Labels and bus
//! members are in neither: they own no node, no point and no def, so a row for
//! one would carry no key at all — and this is the artifact downstream joins
//! on. `spec` / `refdes` are still not projected (design §4 open item).
//!
//! The text and CSV faces carry the five fields in that order and print `-` for
//! a value a row does not have — the glyph the readout faces use for the same
//! thing (design §5.3), since an empty field would read as "this row carries
//! the field and it is blank". Before O16 those faces carried the first three
//! (`node` / `path` / `ident`) and printed an absent value as an empty field.

use crate::instant::insttab::{InstEntry, InstKind, InstTable};
use crate::stages::{loc_of, SourceText};
use crate::McSpaceName;
use serde_json::{json, Value};

/// Build the inst-list payload: the JSON rows plus the text / CSV serialization.
pub fn build_inst_list(table: &InstTable, format: u8) -> (String, Value, usize) {
    let mut sources = SourceText::new();
    // Two passes over one table, concatenated: the instance block keeps the
    // order it has always had, and the point-level rows are appended rather
    // than interleaved, so an existing consumer's row sequence is untouched.
    let instances = table
        .iter()
        .filter(|(_, e)| matches!(e.kind, InstKind::Module | InstKind::Component));
    let points = table
        .iter()
        .filter(|(_, e)| matches!(e.kind, InstKind::Pin | InstKind::Port));

    let items: Vec<Value> = instances
        .chain(points)
        .map(|(_, e)| row(table, e, &mut sources))
        .collect();

    let count = items.len();
    let lines: Vec<String> = items
        .iter()
        .map(|row| {
            let node = row["node"]
                .as_u64()
                .map(|n| n.to_string())
                .unwrap_or_else(|| "-".to_string());
            let path = row["path"].as_str().unwrap_or("-").to_string();
            let ident = row["class"]["key"]["ident"]
                .as_str()
                .unwrap_or("-")
                .to_string();
            let point = row["point"].as_str().unwrap_or("-").to_string();
            let loc = crate::stages::loc_cell(&row["loc"]);
            if format == 4 {
                // CSV: node,path,ident,point,loc
                format!(
                    "{},{},{},{},{}",
                    super::csv_escape(&node),
                    super::csv_escape(&path),
                    super::csv_escape(&ident),
                    super::csv_escape(&point),
                    super::csv_escape(&loc)
                )
            } else {
                format!("{}\t{}\t{}\t{}\t{}", node, path, ident, point, loc)
            }
        })
        .collect();
    let raw_text = if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    };

    (raw_text, Value::Array(items), count)
}

/// One row: the entry's two-space identity plus the source site it is written
/// at.
fn row(table: &InstTable, e: &InstEntry, sources: &mut SourceText) -> Value {
    json!({
        "node": e.node_id.map(|n| n.0),
        "path": e.path,
        "class": class_of(table, e),
        "point": e.point.map(|p| p.to_string()),
        "loc": loc_of(e.src_pos.as_ref().or(e.fallback_pos.as_ref()), sources),
    })
}

/// The class a row belongs to, as `{ def, key }`.
///
/// An instance row carries its own `class_def`. A pin / port row has none
/// (§3.7: the field is set for the kinds that name a def, and a pin names a
/// member of one), so the def half of its canonical key is read off the nearest
/// ancestor that declares one — the same walk, for the same reason, the
/// `stage.*` views do it. `parent_id` strictly decreases towards the root, so
/// the walk terminates.
///
/// Unlike the views' `def_of`, the row also spells the world-local `DefId`, so
/// this walk cannot be shared with it; that `DefId` is the field the artifact
/// has always carried, and pin rows keep the shape of the rows beside them.
fn class_of(table: &InstTable, e: &InstEntry) -> Value {
    if let Some(sn) = &e.class_def {
        return class_value(sn);
    }
    let mut cur = e.parent_id;
    while let Some(id) = cur {
        let Some(entry) = table.get_entry(id) else {
            break;
        };
        if let Some(sn) = &entry.class_def {
            return class_value(sn);
        }
        cur = entry.parent_id;
    }
    Value::Null
}

fn class_value(sn: &McSpaceName) -> Value {
    let def =
        crate::db::defregistry::kind_of(sn).and_then(|k| crate::db::defregistry::def_id(sn, k));
    json!({
        "def": def,
        "key": {
            "uri": sn.uri.as_uri().to_string(),
            "ident": sn.ident.to_string(),
        },
    })
}
