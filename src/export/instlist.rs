// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The `inst-list` projection (build-design §3.7): one row per object of the
//! frozen circuit that carries an identity a consumer can join on, spelled the
//! way the two-space contract spells it, instead of re-deriving it from the
//! path string — plus the ledger of the definitions those rows name, which is
//! where the specification face (boundary-design §1.4) is carried.
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
//! # The ledger (`organization-units-design.md` §10.9, ruled 2026-09-19)
//!
//! A row is not self-sufficient: what the specification *is* belongs to the
//! **definition**, and the rows that carry a given def key are not all
//! instances — a pin / port row's class is the def it was flattened into, and
//! the hbl fixture measures 322 rows over 22 distinct keys with 21 keys shared
//! by both blocks. So the specification is written **once per definition**, in
//! a ledger the rows point at with the key they already carry
//! (`class.key`), and a def's entry holds the three reads D0 keeps apart
//! (`present` / `values` / `keys` — `contract-design.md` §3.1). The ledger is
//! the *read* of the definitions the arena already shares behind `Arc`; a copy
//! per row would be the projection §10.4 rejects.
//!
//! - **Coverage** is the reference set: the def keys the rows name, not every
//!   def in the world. A key it names whose def has **no** attribute face (a
//!   module body has no `attrs` table) still gets an entry, with three empty
//!   regions — "this def declares no attribute keys" is a reading, and dropping
//!   the entry would spell it the same as "this def is not in the ledger" (D0).
//! - **Entry order** is the canonical key order `(uri, ident)`, so a build
//!   decides it (discipline 4); **within an entry** the keys keep the source
//!   order of the definition that wrote them, and `keys` is a sub-order of it.
//! - **Depth** is read to the bottom, and the criterion is a value's shape
//!   rather than a nesting depth (§10.9): a table the dictionary opens gives
//!   dotted leaf keys, a record stays one key. `mc_attr_view` is that walk, and
//!   it is the only place the three reads are computed.
//! - The entry names the same def the rows do — `{ def, key }` — and the ledger
//!   key is the canonical key, which is no new identity: it is a definition-
//!   space identity, not a number, not a counter, not persisted, not promoted.
//!
//! ⚠ What the ledger reads is the **definition's** attribute face — the def
//! object itself, after compile-time evaluation (D6). A value that a definition
//! writes as a formal (`spec.resistance = rs`) reads as `ref: rs`; the value a
//! call site binds it to is the *parameter* face and lives on the instance, not
//! here. The flat carries that do substitute (`InstEntry::resistance_ohm`) are
//! untouched and stay where the rules read them.
//!
//! # Faces
//!
//! Rows come in two blocks, in this order: **instances** (`Module` /
//! `Component` — the rows this artifact has always emitted, in their original
//! order) then **points** (`Pin` / `Port`, appended by O16). Labels and bus
//! members are in neither: they own no node, no point and no def, so a row for
//! one would carry no key at all — and this is the artifact downstream joins
//! on.
//!
//! All three faces carry the same two things, because a face that dropped the
//! ledger would silently answer a different question for the same product id
//! (§3.6 contract 1 — one projection, three spellings):
//!
//! - **JSON** — one object `{ items, defs }`.
//! - **text** / **CSV** — the five-column row table, a blank line, then the
//!   ledger as one line per leaf (`uri`, `ident`, `block`, `key`, `value`) in
//!   the same five columns. The `block` column is which read the line answers;
//!   a `present` line carries `-` for the value (D0: the presence read does not
//!   carry one). A cell is `tag` or `tag:text`, split on the first colon.
//!
//! The text and CSV faces print `-` for a value a row does not have — the glyph
//! the readout faces use for the same thing (design §5.3), since an empty field
//! would read as "this row carries the field and it is blank". `spec` /
//! `refdes` are still not projected as flat columns (design §4 open item); the
//! specification is now carried by the ledger instead.

use crate::db::defregistry::{self, DefValue};
use crate::instant::insttab::{InstEntry, InstKind, InstTable};
use crate::semantic::component::mc_attr_view::{self, LeafRead};
use crate::stages::{loc_of, SourceText};
use crate::McSpaceName;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// Build the inst-list payload: the JSON rows plus the text / CSV
/// serialization.
pub fn build_inst_list(table: &InstTable, format: u8) -> (String, Value, usize) {
    let mut sources = SourceText::new();
    // Two passes over one table, concatenated: the instance block keeps the
    // order it has always had, and the point-level rows are appended rather
    // than interleaved, so an existing consumer's row sequence is untouched.
    let instances: Vec<&InstEntry> = table
        .iter()
        .filter(|(_, e)| matches!(e.kind, InstKind::Module | InstKind::Component))
        .map(|(_, e)| e)
        .collect();
    let points: Vec<&InstEntry> = table
        .iter()
        .filter(|(_, e)| matches!(e.kind, InstKind::Pin | InstKind::Port))
        .map(|(_, e)| e)
        .collect();
    let rows: Vec<&InstEntry> = instances.into_iter().chain(points).collect();

    let items: Vec<Value> = rows.iter().map(|e| row(table, e, &mut sources)).collect();
    let defs = ledger(table, &rows);

    let count = items.len();
    let mut lines: Vec<String> = Vec::with_capacity(count);
    for r in items.iter() {
        let node = r["node"]
            .as_u64()
            .map(|n| n.to_string())
            .unwrap_or_else(|| "-".to_string());
        let path = r["path"].as_str().unwrap_or("-");
        let ident = r["class"]["key"]["ident"].as_str().unwrap_or("-");
        let point = r["point"].as_str().unwrap_or("-");
        let loc = crate::stages::loc_cell(&r["loc"]);
        lines.push(row_cells(format, &node, path, ident, point, &loc));
    }
    if !defs.is_empty() {
        // The row table and the ledger are two things in one file, and a blank
        // line is the only thing between them that no row could be mistaken
        // for: every row carries a cell in the first column.
        lines.push(String::new());
        for d in defs.iter() {
            ledger_cells(d, format, &mut lines);
        }
    }

    let raw_text = if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    };

    (
        raw_text,
        json!({ "items": items, "defs": defs.iter().map(entry).collect::<Vec<Value>>() }),
        count,
    )
}

/// One row: the entry's two-space identity plus the source site it is written
/// at.
fn row(table: &InstTable, e: &InstEntry, sources: &mut SourceText) -> Value {
    json!({
        "node": e.node_id.map(|n| n.0),
        "path": e.path,
        "class": class_of(table, e),
        "point": e.point.map(|p| p.to_string()),
        "loc": loc_of(e.anchor_pos(), sources),
    })
}

/// The class a row belongs to, as `{ def, key }`.
///
/// An instance row carries its own `class_def`. A pin / port row has none
/// (§3.7: the field is set for the kinds that name a def, and a pin names a
/// member of one), so the def half of its canonical key is read off the nearest
/// ancestor that declares one — [`InstTable::class_def_of`], the one owner of
/// that walk.
///
/// The row spells this class one step further than the `stage.*` views do: it
/// also names the world-local `DefId`, which is the field the artifact has
/// always carried, and pin rows keep the shape of the rows beside them.
fn class_of(table: &InstTable, e: &InstEntry) -> Value {
    match table.class_def_of(e.id) {
        Some(sn) => class_value(sn),
        None => Value::Null,
    }
}

fn class_value(sn: &McSpaceName) -> Value {
    json!({
        "def": def_id_of(sn),
        "key": {
            "uri": sn.uri.as_uri().to_string(),
            "ident": sn.ident.to_string(),
        },
    })
}

/// The world-local `DefId` a definition identity resolves to, or `null` where
/// the registry holds no live entry for it. The ledger and the rows ask the
/// same question the same way, so the two cannot name different defs.
fn def_id_of(sn: &McSpaceName) -> Value {
    match defregistry::kind_of(sn).and_then(|k| defregistry::def_id(sn, k)) {
        Some(id) => json!(id),
        None => Value::Null,
    }
}

/// One definition of the ledger: the key the rows point at, and the three reads
/// of its attribute face.
struct DefEntry {
    /// The canonical def key `(uri, ident)`, spelled exactly as a row's
    /// `class.key` spells it.
    uri: String,
    ident: String,
    sn: McSpaceName,
    leaves: Vec<LeafRead>,
}

/// The ledger: one entry per canonical def key the rows name, in canonical key
/// order.
///
/// The keys are collected from the rows themselves rather than from the whole
/// definition space — the artifact answers for what it describes (the reference
/// set, §10.9). A `BTreeMap` keyed by `(uri, ident)` is what fixes the order
/// (discipline 4): no hash order and no arrival order reaches the file.
fn ledger(table: &InstTable, rows: &[&InstEntry]) -> Vec<DefEntry> {
    let mut keys: BTreeMap<(String, String), McSpaceName> = BTreeMap::new();
    for e in rows {
        if let Some(sn) = table.class_def_of(e.id) {
            keys.entry((sn.uri.as_uri().to_string(), sn.ident.to_string()))
                .or_insert_with(|| sn.clone());
        }
    }
    keys.into_iter()
        .map(|((uri, ident), sn)| DefEntry {
            uri,
            ident,
            leaves: def_reads(&sn),
            sn,
        })
        .collect()
}

/// The three reads of one definition's attribute face.
///
/// The def object comes from the same registry the rows' `class.def` comes
/// from, under the same identity, so a row and its ledger entry can never name
/// two different definitions. A def kind with no attribute face answers with an
/// empty list, and that is a true reading: a module's body has no `attrs` table
/// (its `@attr` rows are power-intent declarations, which live in `pi`), so its
/// entry holds three empty regions rather than a guess. The module / interface /
/// func half of "what is this instance" is a different read, registered in
/// `organization-units-design.md` §10.5.
fn def_reads(sn: &McSpaceName) -> Vec<LeafRead> {
    let Some(kind) = defregistry::kind_of(sn) else {
        return Vec::new();
    };
    let Some(id) = defregistry::def_id(sn, kind) else {
        return Vec::new();
    };
    match defregistry::live_entry_by_id(id) {
        Some((_, DefValue::Component(comp))) => mc_attr_view::leaf_reads(&comp.attrs),
        _ => Vec::new(),
    }
}

/// A ledger entry on the structured face.
fn entry(d: &DefEntry) -> Value {
    let present: Vec<Value> = d.leaves.iter().map(|l| json!(l.key)).collect();
    let values: Vec<Value> = d
        .leaves
        .iter()
        .map(|l| json!({ "key": l.key, "view": l.view.tag(), "text": l.text }))
        .collect();
    // The key read: `value_kind`'s answer, for the keys the dictionary
    // registers under. A key it does not register has no line here at all —
    // which is how "not registered" stays apart from "registered, no unit".
    let keys: Vec<Value> = d
        .leaves
        .iter()
        .filter_map(|l| {
            mc_attr_view::key_signal(&l.key).map(|s| match s.unit {
                Some(unit) => json!({ "key": l.key, "kind": s.kind, "unit": unit }),
                None => json!({ "key": l.key, "kind": s.kind }),
            })
        })
        .collect();
    json!({
        "class": class_value(&d.sn),
        "present": present,
        "values": values,
        "keys": keys,
    })
}

/// A ledger entry on the text / CSV face, one line per leaf.
///
/// An entry with **no** keys writes one `present` line carrying `-` where a key
/// would go. That is not a key named `-`: it is the glyph the row table already
/// uses for a field a line does not carry (design §5.3), and it is what keeps a
/// definition that has no attribute face from reading the same as a definition
/// that is not in the ledger at all (D0 — two states, two spellings).
fn ledger_cells(d: &DefEntry, format: u8, out: &mut Vec<String>) {
    if d.leaves.is_empty() {
        out.push(row_cells(format, &d.uri, &d.ident, "present", "-", "-"));
        return;
    }
    for leaf in d.leaves.iter() {
        // The presence read carries names and no value (D0).
        out.push(row_cells(
            format, &d.uri, &d.ident, "present", &leaf.key, "-",
        ));
    }
    for leaf in d.leaves.iter() {
        out.push(row_cells(
            format,
            &d.uri,
            &d.ident,
            "values",
            &leaf.key,
            &cell(leaf.view.tag(), &leaf.text),
        ));
    }
    for leaf in d.leaves.iter() {
        if let Some(sig) = mc_attr_view::key_signal(&leaf.key) {
            let value = match sig.unit {
                Some(unit) => cell(sig.kind, &unit),
                None => sig.kind.to_string(),
            };
            out.push(row_cells(
                format, &d.uri, &d.ident, "keys", &leaf.key, &value,
            ));
        }
    }
}

/// One `tag:text` cell. The tag set is closed and holds no colon, so the first
/// colon is the separator — and an empty text (a declaration with no readable
/// value) prints the tag alone rather than a bare colon.
fn cell(tag: &str, text: &str) -> String {
    if text.is_empty() {
        tag.to_string()
    } else {
        format!("{tag}:{text}")
    }
}

/// Five cells of one line: CSV when the format tag asks for it, tab-separated
/// otherwise. Both the row table and the ledger go through here, so a face
/// cannot escape one of the two differently from the other.
fn row_cells(format: u8, a: &str, b: &str, c: &str, d: &str, e: &str) -> String {
    if format == 4 {
        [a, b, c, d, e].map(super::csv_escape).join(",")
    } else {
        format!("{a}\t{b}\t{c}\t{d}\t{e}")
    }
}
