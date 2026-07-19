// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Go-to-definition — resolve a symbol name to its definition location.
//!
//! Extracted from `rpc/handlers/defs.rs` (handle_def).

use crate::builder::{
    mcb_iter_components, mcb_iter_enums, mcb_iter_interfaces, mcb_iter_modules,
};
use crate::{McCMIE, McIds, McURI};
use serde_json::{json, Value};

/// Resolve a symbol name to its definition, returning structured JSON.
/// Looks across components, modules, interfaces, and enums.
pub fn resolve(name: &str) -> Option<Value> {
    let iterators: [(&str, Vec<(String, String)>); 4] = [
        ("component", mcb_iter_components()),
        ("module", mcb_iter_modules()),
        ("interface", mcb_iter_interfaces()),
        ("enum", mcb_iter_enums()),
    ];

    for (kind, items) in &iterators {
        if let Some((matched, uri)) = items.iter().find(|(n, _)| n == name) {
            let ident = McIds::from(matched.as_str());
            let uri_obj = McURI::from(uri.as_str());

            return match crate::get_def(&ident, &uri_obj) {
                Some(McCMIE::Component(c)) => Some(json!({
                    "kind": "component", "name": matched, "uri": uri,
                    "pin_count": c.pins.pins.len(),
                })),
                Some(McCMIE::Module(m)) => Some(json!({
                    "kind": "module", "name": matched, "uri": uri,
                    "instance_count": m.insts.iter().count(),
                })),
                Some(McCMIE::Interface(i)) => Some(json!({
                    "kind": "interface", "name": matched, "uri": uri,
                    "pin_count": i.pins.pins.len(),
                })),
                Some(McCMIE::Enum(e)) => Some(json!({
                    "kind": "enum", "name": matched, "uri": uri,
                    "value_count": e.values.len(),
                })),
                None => Some(json!({ "kind": kind, "name": matched, "uri": uri })),
            };
        }
    }
    None
}
