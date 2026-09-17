// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! M5 export API — netlist / BOM / SPICE / KiCad builders shared between the CLI
//! (`cmds/export.rs`) and the JSON-RPC handler (`rpc/handlers.rs`).

pub mod bom;
pub mod instlist;
pub mod kicad;
pub mod netlist;
pub mod spice;

use crate::instant::arena::NodeArena;
use crate::instant::inststore::{InstanceStore, TreeView};
use crate::instant::insttab::InstTable;
use crate::{McIds, McModuleInst, McURI};
use serde_json::Value;
use std::panic;

// === pub fn for_each_module(inst: &McModuleInst, f: &mut impl FnMut(&McModuleInst)) { ===
/// Depth-first pre-order walk of a module instance tree
/// (consistency-convergence.md §2.5).
///
/// Visits the module itself first, then recurses into each sub-module. The
/// exporters (bom / kicad / netlist) share this one walker instead of each
/// writing their own `for sub in &inst.sub_modules { recurse }`:
/// get_components`) keep that view — it carries per-instance class names that
/// the module tree does not store.
///
/// Phase C: store-backed depth-first pre-order walk — sub-module order
/// sourced through the [`TreeView`] (design §4: the tree is a view over arena
/// edges, and the tree's `sub_modules` Vec is gone).
pub fn for_each_module_with_arena(
    inst: &McModuleInst,
    arena: &NodeArena,
    store: &InstanceStore,
    f: &mut impl FnMut(&McModuleInst),
) {
    let view = TreeView::new(arena, store);
    for_each_module_impl(inst, &view, f);
}

fn for_each_module_impl(inst: &McModuleInst, view: &TreeView, f: &mut impl FnMut(&McModuleInst)) {
    f(inst);
    for sub in view.sub_modules(inst) {
        for_each_module_impl(sub, view, f);
    }
}

/// Load project + libs, resolve top module, run Pass2 (with panic guard).
///
/// Returns the tree, the flat projection, and the Phase C companion arena +
/// Phase C S3 instance store (design §4: the arena's `children` edges drive
/// the exporter walks, the store supplies the instance content).
pub fn build_tree(
    file: &str,
    top: Option<&str>,
    libs: &[String],
) -> Result<(McModuleInst, InstTable, NodeArena, InstanceStore), String> {
    build_tree_diags(file, top, libs)
        .map(|(tree, table, arena, store, _diags)| (tree, table, arena, store))
}

/// [`build_tree`], plus the net-check diagnostics the flatten returns.
///
/// A file-generation surface (netlist / BOM / drawing) has no use for the
/// diagnostics and calls [`build_tree`]; a **readout** does, because its header
/// reports how many the segment produced. Returning them here rather than
/// re-running the check keeps both consumers on one computation.
///
/// The diagnostics are a **count in a header line**, never a gate: the caller
/// decides whether they mean anything, and for a stage view they do not
/// (law C — a readout does not judge).
pub fn build_tree_diags(
    file: &str,
    top: Option<&str>,
    libs: &[String],
) -> Result<
    (
        McModuleInst,
        InstTable,
        NodeArena,
        InstanceStore,
        Vec<crate::db::diagnostic::diagnostic::Diagnostic>,
    ),
    String,
> {
    let _ = libs;
    let _ = crate::mcc_load_project(&McURI::from(file));

    let top = match top {
        Some(t) => t.to_string(),
        None => match crate::mcb_get_first_module_name() {
            Some(t) => t,
            None => return Err("no module found in file (use --top)".into()),
        },
    };

    let ident = McIds::from(top.as_str());
    let uri = McURI::from(file);
    let built = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        // One instantiation via the core circuit object, then the single
        // one-way flatten projection (Phase A: flatten returns the net-check
        // diagnostics; the owning surface decides logging). Export is a
        // file-generation surface (netlist / BOM / drawing) — it never writes
        // the shared Problems store. The arena + store ride along for the
        // exporters.
        let mut dl = crate::mcc_build_dianlu(&ident, &uri, 0)?;
        let diags = dl.flatten_with_prefix(None);
        let arena = dl.arena().clone();
        let store = dl.store().clone();
        let (tree, table) = dl.into_parts();
        Ok::<_, Box<dyn std::error::Error>>((tree, table, arena, store, diags))
    }));
    match built {
        Ok(Ok(quint)) => Ok(quint),
        Ok(Err(e)) => Err(format!("build failed: {}", e)),
        Err(_) => Err("build panicked (engine Pass2 bug)".into()),
    }
}

/// Build the export payload for a single kind. `arena` + `inst_store` drive
/// the module walks of the bom / netlist / kicad exporters (Phase C); spice
/// reads the flat table and does not traverse the tree.
pub fn build_payload(
    tree: &McModuleInst,
    table: &InstTable,
    arena: &NodeArena,
    inst_store: &InstanceStore,
    top: &str,
    kind: u8,
    format: u8,
) -> (String, Value, usize) {
    match kind {
        1 => bom::build_bom(tree, arena, inst_store, top, format),
        2 => spice::build_spice(tree, table, arena, inst_store, top),
        3 => kicad::build_kicad_netlist(tree, table, arena, inst_store, top),
        4 => instlist::build_inst_list(table, format),
        _ => {
            // Phase D: the tree never stores NetPoint — the netlist export
            // reads the frozen string net tables from the flat table's store.
            let store = table.net_table();
            let store_ref = store.borrow();
            netlist::build_netlist(tree, arena, inst_store, top, format, &store_ref)
        }
    }
}

// Helpers

pub fn attr_value(attrs: &[crate::McAttribute], name: &str) -> Option<String> {
    let id = McIds::from(name);
    for a in attrs {
        if a.id == id {
            for v in &a.values {
                if let crate::McAttrVal::AttrLiteral(crate::McLiteral::String(s)) = v {
                    return Some(s.value.clone());
                }
                if let crate::McAttrVal::AttrLiteral(crate::McLiteral::Int(i)) = v {
                    return Some(i.to_string());
                }
                if let crate::McAttrVal::AttrLiteral(crate::McLiteral::Uval(u)) = v {
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
    fn cli_export__csv_escape_plain() {
        assert_eq!(csv_escape("RES"), "RES");
        assert_eq!(csv_escape(""), "");
    }

    #[test]
    fn cli_export__csv_escape_comma() {
        assert_eq!(csv_escape("a,b"), "\"a,b\"");
    }

    #[test]
    fn cli_export__csv_escape_quote() {
        assert_eq!(csv_escape("a\"b"), "\"a\"\"b\"");
    }

    #[test]
    fn cli_export__csv_escape_newline() {
        assert_eq!(csv_escape("a\nb"), "\"a\nb\"");
    }
}
