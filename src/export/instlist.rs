// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The `inst-list` projection (build-design §3.7): one row per module /
//! component instance of the frozen circuit, carrying the two-space identity
//! downstream consumers join on instead of re-deriving it from the path
//! string.
//!
//! Row shape (design §3.7): `{ node, path, class: { def, key } }` — `node` is
//! the arena `NodeId` (build-internal join handle), `path` the canonical
//! instance path, `class.def` the world-local `DefId`, and `class.key` the
//! canonical def key `(uri, ident)` that survives across worlds and versions.
//! `spec` / `refdes` are not projected yet (see the design's §4 open item).

use crate::instant::insttab::{InstKind, InstTable};
use serde_json::{json, Value};

/// Build the inst-list payload: the JSON rows plus the text / CSV serialization.
pub fn build_inst_list(table: &InstTable, format: u8) -> (String, Value, usize) {
    let items: Vec<Value> = table
        .iter()
        .filter(|(_, e)| matches!(e.kind, InstKind::Module | InstKind::Component))
        .map(|(_, e)| {
            let class = match &e.class_def {
                Some(sn) => {
                    let def = crate::db::defregistry::kind_of(sn)
                        .and_then(|k| crate::db::defregistry::def_id(sn, k));
                    json!({
                        "def": def,
                        "key": {
                            "uri": sn.uri.as_uri().to_string(),
                            "ident": sn.ident.to_string(),
                        },
                    })
                }
                None => Value::Null,
            };
            json!({
                "node": e.node_id.map(|n| n.0),
                "path": e.path,
                "class": class,
            })
        })
        .collect();

    let count = items.len();
    let lines: Vec<String> = items
        .iter()
        .map(|row| {
            let node = row["node"]
                .as_u64()
                .map(|n| n.to_string())
                .unwrap_or_default();
            let path = row["path"].as_str().unwrap_or_default();
            let ident = row["class"]["key"]["ident"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            if format == 4 {
                // CSV: node,path,ident
                format!(
                    "{},{},{}",
                    super::csv_escape(&node),
                    super::csv_escape(path),
                    super::csv_escape(&ident)
                )
            } else {
                format!("{}\t{}\t{}", node, path, ident)
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
