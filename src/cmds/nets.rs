// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Shared Pass2 net-table projection (`{name, points}`) — the single
//! `inst.connections` fold used by `query --kind net`, `extract nets`,
//! `list nets` and `show net|nets`.
//!
//! The fold rules are kept verbatim from the historical extract.rs / show.rs
//! loops so all consumers stay byte-identical (extract-merge plan §Slice C).

use crate::cmds::common;
use mcc::McModuleInst;
use mcc::McURI;
use std::collections::BTreeMap;

/// Fold one built module instance's connections into a net → ordered point
/// labels map. Legacy net-table semantics shared by the read verbs:
///
/// - nets named `"NC"` and points whose path is `"NC"` are skipped;
/// - a point label is `owner + "." + last path segment` when the point has an
///   owner, else the raw path;
/// - points are deduplicated per net in encounter order.
pub fn fold_connections(inst: &McModuleInst) -> BTreeMap<String, Vec<String>> {
    let mut nets: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for conn in &inst.connections {
        let net = conn.effective_net_name();
        if net == "NC" {
            continue;
        }
        let bucket = nets.entry(net).or_default();
        for p in &conn.points {
            if p.path == "NC" {
                continue;
            }
            let label = if let Some(ref o) = p.owner {
                format!("{}.{}", o, p.path.split('.').last().unwrap_or(&p.path))
            } else {
                p.path.clone()
            };
            if !bucket.contains(&label) {
                bucket.push(label);
            }
        }
    }
    nets
}

/// Pass2 nets of `top`. When `uri` is `Some`, build that entry — the
/// `extract nets <file>` and `query … <target> --kind net` path. When `None`,
/// resolve the module's uri by registry name exactly like `show`/`list` do
/// (falling back to `top` as a bare uri).
pub fn top_nets(top: &str, uri: Option<&McURI>) -> Result<BTreeMap<String, Vec<String>>, String> {
    let uri = match uri {
        Some(u) => u.clone(),
        None => mcc::mcb_iter_modules()
            .iter()
            .find(|(n, _)| n == top)
            .map(|(_, u)| McURI::from(u.as_str()))
            .unwrap_or_else(|| McURI::from(top)),
    };
    let inst = common::build_pass2(top, uri.as_str())?;
    Ok(fold_connections(&inst))
}
