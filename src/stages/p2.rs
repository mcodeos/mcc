// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `stage.p2` — the flat instance table as a stage view.
//!
//! Pass2 is the first segment that issues the chain's identity key: `N<node>:<member>`
//! ([`crate::instant::lane::PointId`]) exists from here on, which is why the
//! chain starts at `p2` and not at `src` (design §5.2 ①: Pass1's AST carries
//! none of the chain's keys).
//!
//! ## Which objects, and which key each one gets
//!
//! Taken from the design's §2.4 table — **not every object is a point**, which
//! was v0.3's error:
//!
//! | object | key | cross-build? |
//! |---|---|---|
//! | instance / box | in-domain `NodeId`; canonical **instance path** | both forms |
//! | pin / port | `PointId = (NodeId, PinOrd)` | in-domain only |
//! | labeled net | `NetId` | ⚠ the interning registry is rebuilt per build on the viz path, so the label is what a reader can actually compare |
//! | anonymous net | **none** — derived from its member set | ✗ |
//! | label / bus member | **none** — it owns no physical point | ✗ |
//!
//! Every item therefore carries **both** `key` (the in-domain handle, which is
//! the fast channel *within* one run) and `canon_key` (the only form that
//! survives a rebuild). The design is explicit that a view carrying only
//! `point` becomes worthless the moment the compiler changes (§3 ⚠), and the
//! reason is measurable: a `NodeId`'s integer half is the ordinal of first
//! interning, so inserting one instance ahead of another pushes every later
//! number up.
//!
//! ## Where an item's def comes from
//!
//! A pin row and a port row both have `class_def == None`: the port call site
//! passes `None` to `set_identity`, and a pin row never calls it at all (it
//! only calls `set_point`). So the def half of a point's canonical key is
//! recovered from the **owning instance** by walking `parent_id` up to the
//! first ancestor that has one. That is a read of a value the build already
//! computed, not a second identity system.

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::instant::insttab::{InstKind, InstTable};

use super::{def_of, loc_cell, loc_of, render_table, SourceText, StageSeg, StageView};

/// Build the `stage.p2` view over a flattened instance table.
///
/// `diagnostics` is the count of net-check diagnostics the flatten produced
/// (the caller owns them: `flatten_with_prefix` returns them and the build
/// deliberately does not log them). It is a *count in a header line*, never a
/// gate — law C.
pub fn build_p2(table: &InstTable, top: &str, diagnostics: usize) -> StageView {
    let mut sources = SourceText::new();
    // entry id -> owning net name, built in one pass so point rows do not scan
    // every net (and so two lookups cannot disagree).
    let net_of = net_names(table);

    let mut items: Vec<Value> = Vec::new();

    for (id, entry) in table.iter() {
        let class = match entry.kind {
            InstKind::Module | InstKind::Component => "instance",
            InstKind::Pin | InstKind::Port => "point",
            InstKind::Bus => "bus",
            InstKind::Label => "label",
        };

        // The def half of the canonical key: a component/module row carries its
        // own; a pin/port row has none, so it is read off the owning instance.
        let canon_def = def_of(table, *id);
        let canon = match canon_def {
            Some(def) => json!({ "path": entry.path, "def": def }),
            None => json!({ "path": entry.path, "def": Value::Null }),
        };

        let point = entry.point;
        let (key, net, members) = match entry.kind {
            InstKind::Module | InstKind::Component | InstKind::Bus | InstKind::Label => {
                // A label / bus member owns no physical point (design §2.4), so
                // it has no key at all rather than a synthesised one.
                let key = if matches!(entry.kind, InstKind::Module | InstKind::Component) {
                    Value::String(format!("D{}", entry.id))
                } else {
                    Value::Null
                };
                (key, Value::Null, Value::Null)
            }
            InstKind::Pin | InstKind::Port => (
                point
                    .map(|p| Value::String(p.to_string()))
                    .unwrap_or(Value::Null),
                net_of
                    .get(&id)
                    .map(|n| Value::String(n.clone()))
                    .unwrap_or(Value::Null),
                Value::Null,
            ),
        };

        // Both position states are published, in parallel — neither is a
        // fallback for the other. `loc` stays the single position the text face
        // prints (§5.3 ① pins that column to one position, and a variable-length
        // list would break the column grammar); the whole wiring set is
        // `loc_all`, and `via` says which state anchored the row.
        let loc = loc_of(entry.anchor_pos(), &mut sources);
        let loc_all = Value::Array(
            entry
                .src_pos
                .iter()
                .map(|p| loc_of(Some(p), &mut sources))
                .collect(),
        );
        let decl_loc = entry
            .fallback_pos
            .as_ref()
            .map(|p| loc_of(Some(p), &mut sources))
            .unwrap_or(Value::Null);
        let via = if !entry.unwired() {
            "wired"
        } else if decl_loc.is_null() {
            "none"
        } else {
            "declared"
        };

        items.push(json!({
            "class": class,
            "key": key,
            "point": point.map(|p| p.to_string()),
            "path": entry.path,
            "canon_key": canon,
            "class_name": if entry.class_name.is_empty() {
                Value::Null
            } else {
                Value::String(entry.class_name.clone())
            },
            "net": net,
            "members": members,
            "loc": loc,
            "loc_all": loc_all,
            "decl_loc": decl_loc,
            "via": via,
        }));
    }

    // Net rows. A net is its own object kind: a labeled net keys on its label,
    // an anonymous one has no key and is matched by member-set overlap, so its
    // member list is the thing a consumer compares (design §2.4).
    for net in table.get_nets() {
        let mut paths: Vec<String> = net
            .points
            .iter()
            .filter_map(|p| table.get_entry(*p).map(|e| e.path.clone()))
            .collect();
        paths.sort();
        let labeled = !crate::instant::mc_net::is_anon_net_name(&net.name);
        items.push(json!({
            "class": "net",
            "key": if labeled {
                Value::String(format!("net:{}", net.name))
            } else {
                Value::Null
            },
            "point": Value::Null,
            "path": Value::Null,
            "canon_key": Value::Null,
            "class_name": Value::Null,
            "net": net.name,
            "members": paths,
            "loc": Value::Null,
            "loc_all": Value::Array(vec![]),
            "decl_loc": Value::Null,
            "via": "none",
        }));
    }

    StageView::new(StageSeg::P2, top, items, diagnostics)
}

/// Render the `stage.p2` text face from the *same* items the JSON face uses
/// (design §5.3 ruling ③). Columns: key, path, class-or-net, loc — first column
/// always the key, last always `loc`, per §5.3 ①.
pub fn render_p2_text(view: &StageView) -> String {
    // Rendered in `items` order, which [`StageView::new`] already sorted: the
    // two faces must present the same sequence, not merely the same set.
    let rows: Vec<Vec<String>> = view
        .items
        .iter()
        .map(|item| {
            let key = item["key"].as_str().unwrap_or("-").to_string();
            let second = match item["class"].as_str().unwrap_or("") {
                "net" => item["net"].as_str().unwrap_or("-").to_string(),
                _ => item["path"].as_str().unwrap_or("-").to_string(),
            };
            let third = match item["class"].as_str().unwrap_or("") {
                // `members=N`, not `N members`: a cell must not contain a
                // space, or the two-space column separator becomes ambiguous
                // and the row stops being readable by splitting on whitespace
                // (§5.3's "no tabs, use fixed width or double space" only
                // works if no cell can be mistaken for a separator).
                "net" => match item["members"].as_array() {
                    Some(m) => format!("members={}", m.len()),
                    None => "-".to_string(),
                },
                "point" => item["net"].as_str().unwrap_or("-").to_string(),
                _ => item["class_name"].as_str().unwrap_or("-").to_string(),
            };
            vec![key, second, third, loc_cell(&item["loc"]).to_string()]
        })
        .collect();

    let mut out = vec![view.header_line(), view.counts_line(StageSeg::P2)];
    render_table(&rows, &mut out);
    out.join("\n")
}

/// entry id -> name of the net that owns it. Built once, in one pass.
fn net_names(table: &InstTable) -> HashMap<u32, String> {
    let mut out = HashMap::new();
    for net in table.get_nets() {
        for p in &net.points {
            out.insert(*p, net.name.clone());
        }
    }
    out
}
