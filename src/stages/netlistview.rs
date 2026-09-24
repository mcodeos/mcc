// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The `netlist` projection — the flattening's connectivity, read as data.
//!
//! The second carried canonical `view-name` word (`schema/projection.cddl`).
//! The item shape is the export/netlist face expressed as envelope items
//! (§2.2 / §4 of `projection-schema-design.md`): one item per **copper
//! island** — the flat table's per-scope net segments folded by shared point
//! ids ([`crate::export::netlist::island_nets`], U158) — named by its best
//! member, carrying the member pin paths. The JSON export face and this view
//! build their items from **one** builder ([`net_items`]), so the two faces
//! cannot spell the connectivity two ways.
//!
//! Boundary ports and bus lanes arrive as ordinary member points here; a
//! spelling of their own is a later decision, not a v1 gap. A readout, not a
//! verdict (law C): the item count never flips an exit code.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::StageView;
use crate::export::netlist::{island_nets, is_excluded, PointNaming};
use crate::instant::insttab::InstTable;

/// The canonical word this face publishes (CDDL `view-name`).
pub const NETLIST_VIEW: &str = "netlist";

/// CDDL `net = { name, points }` — one copper island.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetItem {
    /// A named member's name, or the engine `_netN` spelling for copper no
    /// statement named; same-spelling islands disambiguate `#2`, `#3`, …
    pub name: String,
    /// Member pin paths, ascending net id, deduplicated in encounter order.
    pub points: Vec<String>,
}

/// The items, in the island map's own order (the `BTreeMap` keys islands by
/// name, so the order is a function of the world). The deliberate no-connect
/// bucket and the parse-error marker are not copper — the same exclusion the
/// export face applies.
pub fn net_items(table: &InstTable) -> Vec<Value> {
    island_nets(table, PointNaming::Local)
        .into_iter()
        .filter(|(name, _)| !is_excluded(name))
        .map(|(name, points)| {
            serde_json::to_value(NetItem { name, points }).unwrap_or(Value::Null)
        })
        .collect()
}

/// Per-face counts: the islands and their members. Every word is printed even
/// when zero — an absent line reads as "not implemented" rather than "none".
pub fn net_counts(items: &[Value]) -> Value {
    let points: usize = items
        .iter()
        .map(|i| i["points"].as_array().map(|a| a.len()).unwrap_or(0))
        .sum();
    json!({
        "nets": items.len(),
        "points": points,
    })
}

/// Assemble the projection. Goes through [`StageView::with_view`] — this view
/// publishes its own vocabulary and its own count words, not a pipeline
/// segment's.
pub fn netlist_view(top: &str, table: &InstTable) -> StageView {
    let items = net_items(table);
    let counts = net_counts(&items);
    StageView::with_view(NETLIST_VIEW, top, items, counts)
}

/// The text face, rendered from the **same** items the envelope carries:
/// header, the counts, then one row per island — `name` and its points joined
/// by spaces, the export face's own row shape.
pub fn render_netlist_text(view: &StageView) -> String {
    let mut lines = vec![view.header_line()];
    let words: Vec<String> = view
        .counts
        .as_object()
        .map(|m| {
            m.iter()
                .map(|(k, v)| format!("{k} {}", v.as_u64().unwrap_or(0)))
                .collect()
        })
        .unwrap_or_default();
    lines.push(format!("# {}", words.join("  ")));
    for it in &view.items {
        let name = it["name"].as_str().unwrap_or("-");
        let points = it["points"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|p| p.as_str().unwrap_or("-"))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();
        lines.push(format!("{name}: {points}"));
    }
    lines.join("\n")
}
