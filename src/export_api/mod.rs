// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! M5 export API — netlist / BOM / SPICE / KiCad builders shared between the CLI
//! (`cmds/export.rs`) and the JSON-RPC handler (`rpc/handlers.rs`).

pub mod bom;
pub mod kicad;
pub mod netlist;
pub mod spice;

use crate::instant::inst_table::InstTable;
use crate::{McIds, McModuleInst, McURI};
use serde_json::Value;
use std::panic;

/// Kind of export.
pub fn kind_from_str(s: &str) -> u8 {
    match s {
        "bom" => 1,
        "spice" => 2,
        "kicad" | "kicad-netlist" => 3,
        _ => 0,
    }
}

pub fn kind_to_str(k: u8) -> &'static str {
    match k {
        1 => "bom",
        2 => "spice",
        3 => "kicad-netlist",
        _ => "netlist",
    }
}

/// Output format. 0=text, 1=json, 2=json-pretty, 3=yaml, 4=csv
pub fn format_from_str(s: &str) -> u8 {
    match s {
        "json" => 1,
        "json-pretty" | "jsonpretty" => 2,
        "yaml" => 3,
        "csv" => 4,
        _ => 0,
    }
}

/// Load project + libs, resolve top module, run Pass2 (with panic guard).
pub fn build_tree(
    file: &str,
    top: Option<&str>,
    libs: &[String],
) -> Result<(McModuleInst, InstTable), String> {
    let _ = libs;
    let _ = mcc::mcc_load_project(&McURI::from(file));

    let top = match top {
        Some(t) => t.to_string(),
        None => match mcc::mcb_get_first_module_name() {
            Some(t) => t,
            None => return Err("no module found in file (use --top)".into()),
        },
    };

    let ident = McIds::from(top.as_str());
    let uri = McURI::from(file);
    let built = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        mcc::mcc_build_flat(&ident, &uri, 0)
    }));
    match built {
        Ok(Ok((tree, table))) => Ok((tree, table)),
        Ok(Err(e)) => Err(format!("build failed: {}", e)),
        Err(_) => Err("build panicked (engine Pass2 bug)".into()),
    }
}

/// Build the export payload for a single kind.
pub fn build_payload(
    tree: &McModuleInst,
    table: &InstTable,
    top: &str,
    kind: u8,
    format: u8,
) -> (String, Value, usize) {
    match kind {
        1 => bom::build_bom(tree, top, format),
        2 => spice::build_spice(tree, table, top),
        3 => kicad::build_kicad_netlist(tree, table, top),
        _ => netlist::build_netlist(tree, top, format),
    }
}

// ============================================================================
// Helpers
// ============================================================================

pub fn attr_value(attrs: &[mcc::McAttribute], name: &str) -> Option<String> {
    let id = McIds::from(name);
    for a in attrs {
        if a.id == id {
            for v in &a.values {
                if let mcc::McAttrVal::AttrLiteral(mcc::McLiteral::String(s)) = v {
                    return Some(s.value.clone());
                }
                if let mcc::McAttrVal::AttrLiteral(mcc::McLiteral::Int(i)) = v {
                    return Some(i.to_string());
                }
                if let mcc::McAttrVal::AttrLiteral(mcc::McLiteral::Uval(u)) = v {
                    return Some(u.value().to_string());
                }
            }
        }
    }
    None
}

pub fn csv_escape(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') || field.contains('\r') {
        let escaped = field.replace('"', "\"\"");
        format!("\"{}\"", escaped)
    } else {
        field.to_string()
    }
}

pub(crate) fn chrono_like_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("epoch={}", secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_escape_plain() {
        assert_eq!(csv_escape("RES"), "RES");
        assert_eq!(csv_escape(""), "");
    }

    #[test]
    fn csv_escape_comma() {
        assert_eq!(csv_escape("a,b"), "\"a,b\"");
    }

    #[test]
    fn csv_escape_quote() {
        assert_eq!(csv_escape("a\"b"), "\"a\"\"b\"");
    }

    #[test]
    fn csv_escape_newline() {
        assert_eq!(csv_escape("a\nb"), "\"a\nb\"");
    }
}
