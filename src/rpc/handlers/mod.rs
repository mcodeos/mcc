// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! RPC API Handlers — Iteration B
//!
//! ### Project (Project Mode)
//!   - `project.create`         Create project workspace
//!   - `project.use`            Switch active project
//!   - `project.upload`         Upload file to project src/
//!   - `project.upload_archive` Upload entire project directory
//!   - `project.parse`          Pass1
//!   - `project.build`          Pass1 + Pass2
//!   - `project.delete`         Delete project
//!
//! ### Lib (Library Management)
//!   - `lib.load`               Load library by name into memory
//!   - `lib.unload`             Unload library from memory (if loaded)
//!
//! ### Common Pass
//!   - `build.full`             Run Pass1 + Pass2 based on the active workspace    
//!
//! ## Error codes (extended JSON-RPC standard)
//!   - 32100  IO / FS error
//!   - 32101  workspace conflict / cannot create
//!   - 32102  workspace does not exist
//!   - 32103  archive / decode failed
//!   - 32104  unsupported format
//!   - 32105  entry file not found
//!   - 32106  dependency not loaded
//!   - 32107  Pass1 / Pass2 failed
//!   - 32110  Pass1 (parse) failed
//!   - 32111  Pass2 (build) failed
//!   - 32112  component / module / entity / file not found

use super::protocol::{JsonRpcError, RpcResult};
use crate::db::cmie::tables as workspace;
use crate::search_api::{walk_defs, SearchInputs, SearchKind};
use crate::McURI;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::info;

// LSP semantic token/symbol assembly (extracted to lsp/sem.rs)
pub(crate) use params::*;
pub(crate) mod params;
pub use crate::lsp::sem::{classify_token_by_symbol, try_lookup_sem};

pub(crate) fn mcc_system_root() -> PathBuf {
    // Single source of truth: delegate to datadir::data_root() (which honors
    // $MCC_SYSTEM_ROOT). The cwd/mc/ probe and the `~/.mcode` fallback live
    // there now.
    crate::cli::datadir::data_root()
}

pub(crate) fn projects_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mcc-projects")
}
pub(crate) fn project_dir(id: &str) -> PathBuf {
    projects_dir().join(id)
}
pub(crate) fn project_manifest(id: &str) -> PathBuf {
    project_dir(id).join("manifest.toml")
}
pub(crate) fn mcode_dir() -> PathBuf {
    mcc_system_root().join("mcode")
}

// ============================================================================
// Existing methods (preserved, behavior unchanged)
// ============================================================================

// ============================================================================
// Lib handlers
// ============================================================================

// ============================================================================
// defs.search (M5) — text/regex/fuzzy search across loaded definitions
// ============================================================================

// ============================================================================
// defs.query (M5 PR#2) — structured DSL query
// ============================================================================

// ============================================================================
// export (M5 PR#3) — text/JSON/CSV netlist, BOM, SPICE
// ============================================================================

/// Resolve an installed library directory under the system root.
/// Flat layout: checks `<root>/<name>` (built-in) and `<root>/<name>@<version>` (3rd-party).
pub(crate) fn resolve_installed_lib_dir(name: &str) -> Option<PathBuf> {
    let root = mcc_system_root();

    // Built-in: <root>/<name> (e.g. mcode)
    let bare = root.join(name);
    if bare.exists() {
        return Some(bare);
    }

    // 3rd-party: <root>/<name>@<version>
    if let Ok(entries) = fs::read_dir(&root) {
        let prefix = format!("{name}@");
        for entry in entries.flatten() {
            let fname = entry.file_name().to_string_lossy().to_string();
            if fname.starts_with(&prefix) && entry.path().is_dir() {
                return Some(entry.path());
            }
        }
    }
    None
}

pub(crate) fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

// ============================================================================
// Trace handlers
// ============================================================================

// ============================================================================
// Common build.full handlers (based on active workspace)
// ============================================================================

// ============================================================================
// Internal: Pass1 / Pass2 execution
// ============================================================================

pub(crate) fn run_pass1(
    entry: &Path,
    command: &str,
    ws_kind: &str,
    ws_name: &str,
    include_system: bool,
) -> RpcResult {
    let uri = entry.to_string_lossy().to_string();
    let mc_uri = McURI::from(uri.as_str());

    // Output Pass 1 trace to server log
    info!(target: "crate::pass1", "----------------------------------------");
    info!(target: "crate::pass1", "[Pass 1] Loading project from: {}", uri);
    info!(target: "crate::pass1", "----------------------------------------");

    crate::mcc_load_project(&mc_uri);
    let pass1 = collect_pass1(&mc_uri, include_system);

    let module_count = crate::mcb_module_count();
    let component_count = crate::mcb_component_count();
    let interface_count = crate::mcb_interface_count();

    // Output definition statistics to server log
    info!(target: "crate::pass1", "Total definitions loaded:");
    info!(target: "crate::pass1", "  - Modules: {}", module_count);
    info!(target: "crate::pass1", "  - Components: {}", component_count);
    info!(target: "crate::pass1", "  - Interfaces: {}", interface_count);

    // Output each module details to server log
    for (name, module_uri) in crate::mcb_iter_modules() {
        let ident = crate::McIds::from(name.as_str());
        let module_mc_uri = McURI::from(module_uri.as_str());
        if let Some(cmie) = crate::get_def(&ident, &module_mc_uri) {
            if let crate::McCMIE::Module(module_def) = cmie {
                info!(target: "crate::pass1", ">> Found module definition: {}", name);
                info!(target: "crate::pass1", "------------------------------------------------------------------");
                info!(target: "crate::pass1", "| Ports ");
                info!(target: "crate::pass1", "|-----------------------------------------------------------------");
                info!(target: "crate::pass1", "|   inputs:  {:?}",
                    module_def.insts.inputs_with_name().iter()
                        .map(|(n, _)| *n).collect::<Vec<_>>()
                );
                info!(target: "crate::pass1", "|   outputs: {:?}",
                    module_def.insts.outputs_with_name().iter()
                        .map(|(n, _)| *n).collect::<Vec<_>>()
                );
                info!(target: "crate::pass1", "|   bidirs:  {:?}",
                    module_def.insts.bidirs_with_name().iter()
                        .map(|(n, _)| *n).collect::<Vec<_>>()
                );
                info!(target: "crate::pass1", "|   powers:  {:?}",
                    module_def.insts.powers_with_name().iter()
                        .map(|(n, _)| *n).collect::<Vec<_>>()
                );
                info!(target: "crate::pass1", "|");
                info!(target: "crate::pass1", "| Symbols ({} entries)", module_def.insts.iter().count());
                info!(target: "crate::pass1", "|-----------------------------------------------------------------");
                for (key, ident) in module_def.insts.iter() {
                    let type_name = ident.type_name();
                    info!(target: "crate::pass1", "|  {:<15} {}", type_name, key);
                }
                info!(target: "crate::pass1", "|");
                info!(target: "crate::pass1", "| Stmts ({} connections)", module_def.stmts.len());
                info!(target: "crate::pass1", "|-----------------------------------------------------------------");
                if module_def.stmts.is_empty() {
                    info!(target: "crate::pass1", "|   (no connections)");
                } else {
                    for (i, _stmt) in module_def.stmts.iter().enumerate() {
                        info!(target: "crate::pass1", "|");
                        info!(target: "crate::pass1", "|   +--- Series[{}] ----------", i);
                    }
                    info!(target: "crate::pass1", "|   +--------------------------------------------------");
                }
                info!(target: "crate::pass1", "------------------------------------------------------------------");
            }
        }
    }

    Ok(json!({
        "command": command,
        "workspace": {"kind": ws_kind, "name": ws_name},
        "pass1": pass1,
        "summary": {
            "module_count": module_count,
            "component_count": component_count,
            "interface_count": interface_count,
        }
    }))
}

/// Execute Pass2: resolve top module, run instantiation with panic guard,
/// collect results. Returns (top_name, pass2_json).
fn execute_pass2(mc_uri: &McURI, top: Option<&str>) -> Result<(String, Value), JsonRpcError> {
    let top_name = match top {
        Some(t) => t.to_string(),
        None => crate::mcb_get_module_name_by_uri(mc_uri)
            .ok_or_else(|| JsonRpcError::custom(32107, "no top module found"))?,
    };

    let ident = crate::McIds::from(top_name.as_str());
    if crate::get_def(&ident, mc_uri).is_none() {
        return Err(JsonRpcError::custom(
            32107,
            &format!("top module '{top_name}' not defined"),
        ));
    }

    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::mcc_build(&ident, mc_uri)
    }));

    match built {
        Ok(Ok(inst)) => {
            info!(target: "crate::pass2", "----------------------------------------");
            info!(target: "crate::pass2", "[Pass 2] Instantiating top module: {}", top_name);
            info!(target: "crate::pass2", "----------------------------------------");
            info!(target: "crate::pass2", ">> Instance: {} (class {})",
                inst.name.to_string(), inst.def.name.to_string());
            info!(target: "crate::pass2", "|   ports:       {}", inst.ports.len());
            info!(target: "crate::pass2", "|   components:  {}", inst.components.len());
            info!(target: "crate::pass2", "|   sub_modules: {}", inst.sub_modules.len());
            info!(target: "crate::pass2", "|   connections: {}", inst.connections.len());
            for sub in inst.sub_modules.iter() {
                info!(target: "crate::pass2", "|     - {} (class {})",
                    sub.name.to_string(), sub.def.name.to_string());
            }
            let pass2 = collect_pass2(&top_name, &inst);
            Ok((top_name, pass2))
        }
        Ok(Err(e)) => Err(JsonRpcError::custom(
            32107,
            &format!("instantiation failed: {e}"),
        )),
        Err(_) => Err(JsonRpcError::custom(
            32108,
            "Pass2 build panicked (engine bug); request aborted, server kept alive",
        )),
    }
}

/// Execute Pass1 + Pass2 from file
pub(crate) fn run_full_build(
    entry: &Path,
    top: Option<&str>,
    command: &str,
    ws_kind: &str,
    ws_name: &str,
    include_system: bool,
) -> RpcResult {
    let uri = entry.to_string_lossy().to_string();
    let mc_uri = McURI::from(uri.as_str());
    crate::mcc_load_project(&mc_uri);
    let pass1 = collect_pass1(&mc_uri, include_system);

    let (top_name, pass2) = execute_pass2(&mc_uri, top)?;

    Ok(json!({
        "command": command,
        "workspace": {"kind": ws_kind, "name": ws_name},
        "pass1": pass1,
        "pass2": pass2,
        "summary": {
            "module_count": crate::mcb_module_count(),
            "component_count": crate::mcb_component_count(),
            "interface_count": crate::mcb_interface_count(),
            "top": top_name,
        }
    }))
}

/// Envelope-shaped Pass1+Pass2 result, matching `mcc build`'s local `CommandResult`.
///
/// The CLI's RPC mode deserializes this payload into its own (binary-crate)
/// `CommandResult` and renders it through the same `emit_envelope` funnel as
/// the local path — so the shape must carry every field the client's types
/// require, and the summary must aggregate the same counts
/// `ResultBuilder::finish()` computes locally (design contract
/// local ↔ server). The lib crate cannot reach the
/// binary's `output::*` / `cmds::parse::*` modules, so the envelope is
/// assembled here from the lib-side collectors (`collect_pass1` /
/// `collect_pass2`) plus a cursor-based diagnostic snapshot that mirrors
/// [`crate::...PhaseTracker`]'s phase batching. Kept separate from
/// [`run_full_build`] so the legacy shape used by other RPC methods (e.g.
/// aicontract `check`) is untouched.
pub(crate) fn run_full_build_envelope(
    entry: &Path,
    top: Option<&str>,
    command: &str,
    ws_kind: &str,
    ws_name: &str,
    _include_system: bool,
) -> RpcResult {
    let t0 = std::time::Instant::now();
    let uri = entry.to_string_lossy().to_string();
    let mc_uri = McURI::from(uri.as_str());

    // ── Diagnostic cursor: phase batching mirrors the local build. `handle_build_full`
    //    has already run `load_libs_rpc` — the diagnostics present *now* are lib-load
    //    diagnostics → Pass0. Loading/parsing the project below emits the project's
    //    Pass1 diagnostics. Each snapshot advances the cursor. ──
    let mut cursor = 0usize;
    let mut take_diags = |phase: &str| -> Vec<Value> {
        let all = crate::mcc_diagnose_all();
        let slice = if cursor <= all.len() {
            &all[cursor..]
        } else {
            &[]
        };
        let out: Vec<Value> = slice.iter().map(|d| mcc_diag_to_json(d, phase)).collect();
        cursor = all.len();
        out
    };

    let pass0 = json!({ "loaded_files": [], "diagnostics": take_diags("pass0") });

    crate::mcc_load_project(&mc_uri);

    // Include system files unconditionally: the local path's `public_collect_pass1`
    // never filters them, so the RPC payload must carry the same definitions for
    // byte-identical output (design contract: output identical local ↔ server).
    let mut pass1 = collect_pass1(&uri, true);
    pass1["diagnostics"] = Value::Array(take_diags("pass1"));
    // Local's `public_collect_pass1` never populates `definitions.ports`, so
    // the payload must carry the same (empty) list for byte-identical output.
    pass1["definitions"]["ports"] = Value::Array(vec![]);

    // Top selection (mcd docs-mc 16-export-viz §6): explicit top → all modules
    // in the file → all components → all interfaces. Components and interfaces
    // are "virtually instantiated" via a synthetic module so a component-only
    // file (e.g. a connector library part) builds instead of failing with
    // "no top module found". When several targets share the file, the envelope
    // carries the first target's Pass 2 tree (mirrors the CLI build).
    let targets = crate::mcc_virtual_resolve_targets(&mc_uri, top)
        .map_err(|e| JsonRpcError::custom(32107, &e))?;
    let top_name = targets.first().cloned().unwrap_or_else(|| "".to_string());

    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::mcc_virtual_build(&top_name, &mc_uri)
    }));
    let inst = match built {
        Ok(Ok(inst)) => inst,
        Ok(Err(e)) => {
            return Err(JsonRpcError::custom(
                32107,
                &format!("instantiation failed: {e}"),
            ))
        }
        Err(_) => {
            return Err(JsonRpcError::custom(
                32108,
                "Pass2 build panicked (engine bug); request aborted, server kept alive",
            ))
        }
    };

    let mut pass2 = collect_pass2(&top_name, &inst);
    pass2["diagnostics"] = Value::Array(take_diags("pass2"));

    // ── Summary, mirroring ResultBuilder::finish() ──
    let all_diags: Vec<&Value> = pass0["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(pass1["diagnostics"].as_array().into_iter().flatten())
        .chain(pass2["diagnostics"].as_array().into_iter().flatten())
        .collect();
    let errors = all_diags
        .iter()
        .filter(|d| d["severity"] == "error")
        .count();
    let warnings = all_diags
        .iter()
        .filter(|d| d["severity"] == "warning")
        .count();
    let module_count = pass1["definitions"]["modules"]
        .as_array()
        .map(|v| v.len())
        .unwrap_or(0);
    let component_count = pass1["definitions"]["components"]
        .as_array()
        .map(|v| v.len())
        .unwrap_or(0);
    let interface_count = pass1["definitions"]["interfaces"]
        .as_array()
        .map(|v| v.len())
        .unwrap_or(0);
    let instance_count = count_instance_json(pass2.get("instances"));
    let net_count = pass2["nets"].as_array().map(|v| v.len()).unwrap_or(0);

    // ── Categorized statistics, mirroring `output/mod.rs` `render_envelope_text`
    //    Summary block: namespace classes split system/project, the classes
    //    actually instantiated (used) split system/project, and the instance
    //    breakdown by kind. A class counts as *system* when it is defined only
    //    in the system space — a same-named project definition shadows it. ──
    let (ns_mod_sys, ns_mod_proj) = def_class_split(pass1["definitions"].get("modules"));
    let (ns_comp_sys, ns_comp_proj) = def_class_split(pass1["definitions"].get("components"));
    let (ns_iface_sys, ns_iface_proj) = def_class_split(pass1["definitions"].get("interfaces"));

    let mut sys_mods = std::collections::HashSet::new();
    let mut proj_mods = std::collections::HashSet::new();
    let mut sys_comps = std::collections::HashSet::new();
    let mut proj_comps = std::collections::HashSet::new();
    for d in pass1["definitions"]["modules"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let name = d["name"].as_str().unwrap_or_default().to_string();
        if d["uri"].as_str().map(is_system_uri).unwrap_or(false) {
            sys_mods.insert(name);
        } else {
            proj_mods.insert(name);
        }
    }
    for d in pass1["definitions"]["components"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let name = d["name"].as_str().unwrap_or_default().to_string();
        if d["uri"].as_str().map(is_system_uri).unwrap_or(false) {
            sys_comps.insert(name);
        } else {
            proj_comps.insert(name);
        }
    }
    let mut used_modules = std::collections::BTreeSet::new();
    let mut used_components = std::collections::BTreeSet::new();
    let mut module_insts = 0usize;
    let mut component_insts = 0usize;
    tally_build_stats(
        &inst,
        &mut used_modules,
        &mut used_components,
        &mut module_insts,
        &mut component_insts,
    );
    let is_system_class = |name: &str,
                           system: &std::collections::HashSet<String>,
                           project: &std::collections::HashSet<String>| {
        system.contains(name) && !project.contains(name)
    };
    let used_modules_system = used_modules
        .iter()
        .filter(|n| is_system_class(n, &sys_mods, &proj_mods))
        .count();
    let used_components_system = used_components
        .iter()
        .filter(|n| is_system_class(n, &sys_comps, &proj_comps))
        .count();

    let summary = json!({
        "module_count": module_count,
        "component_count": component_count,
        "interface_count": interface_count,
        "instance_count": instance_count,
        "net_count": net_count,
        "errors": errors,
        "warnings": warnings,
        "elapsed_ms": t0.elapsed().as_millis(),
        "stats": {
            "ns_modules_system": ns_mod_sys,
            "ns_modules_project": ns_mod_proj,
            "ns_components_system": ns_comp_sys,
            "ns_components_project": ns_comp_proj,
            "ns_interfaces_system": ns_iface_sys,
            "ns_interfaces_project": ns_iface_proj,
            "used_modules_system": used_modules_system,
            "used_modules_project": used_modules.len() - used_modules_system,
            "used_components_system": used_components_system,
            "used_components_project": used_components.len() - used_components_system,
            "module_insts": module_insts,
            "component_insts": component_insts,
        },
    });

    Ok(json!({
        "command": command,
        "workspace": { "kind": ws_kind, "name": ws_name },
        "pass0": pass0,
        "pass1": pass1,
        "pass2": pass2,
        "summary": summary,
    }))
}

/// Envelope `Diagnostic` JSON from an `mcc::Diagnostic`, mirroring the binary
/// crate's `output::diagnostic::from_mcc` field mapping so the payload
/// deserializes into the client's `Diagnostic` byte-identically.
fn mcc_diag_to_json(d: &crate::McDiagnostic, phase: &str) -> Value {
    let related: Vec<Value> = d
        .other
        .iter()
        .map(|ri| {
            json!({
                "message": ri.get_formatted_message(),
                "location": {
                    "file": ri.location.uri.as_str(),
                    "line": ri.location.row,
                    "column": ri.location.col,
                    "pos": ri.location.pos,
                    "len": ri.location.len,
                },
            })
        })
        .collect();
    let severity = match d.level {
        crate::DiagnosticLevel::Error => "error",
        crate::DiagnosticLevel::Warning => "warning",
        crate::DiagnosticLevel::Info => "info",
        crate::DiagnosticLevel::Hint => "hint",
    };
    json!({
        "phase": phase,
        "severity": severity,
        "code": d.code,
        "message": d.msg,
        "location": {
            "file": d.loc.uri.as_str(),
            "line": d.loc.row,
            "column": d.loc.col,
            "pos": d.loc.pos,
            "len": d.loc.len,
        },
        "suggestions": [],
        "related": related,
    })
}

/// Recursively count total instances in the Pass2 instance tree (root +
/// components + submodules), mirroring `ResultBuilder`'s `count_instances`.
fn count_instance_json(node: Option<&Value>) -> usize {
    let Some(n) = node else { return 0 };
    let mut total = 1;
    if let Some(cs) = n["components"].as_array() {
        total += cs.len();
    }
    for sub in n["sub_modules"].as_array().into_iter().flatten() {
        total += count_instance_json(Some(sub));
    }
    total
}

/// Count how many of a pass1 definition array live in the system space
/// (`/mcode/`) vs the project. Returns `(system, project)`.
fn def_class_split(arr: Option<&Value>) -> (usize, usize) {
    let Some(arr) = arr.and_then(|v| v.as_array()) else {
        return (0, 0);
    };
    let sys = arr
        .iter()
        .filter(|d| d["uri"].as_str().map(is_system_uri).unwrap_or(false))
        .count();
    (sys, arr.len() - sys)
}

/// Walk the instance tree, collecting per-kind instance counts and the distinct
/// set of classes actually instantiated (mirrors `output/mod.rs` `tally_tree`).
fn tally_build_stats(
    node: &crate::MccProjectTree,
    used_modules: &mut std::collections::BTreeSet<String>,
    used_components: &mut std::collections::BTreeSet<String>,
    module_insts: &mut usize,
    component_insts: &mut usize,
) {
    *module_insts += 1;
    *component_insts += node.components.len();
    used_modules.insert(node.def.name.to_string());
    for c in &node.components {
        used_components.insert(c.def.name.to_string());
    }
    for sub in &node.sub_modules {
        tally_build_stats(
            sub,
            used_modules,
            used_components,
            module_insts,
            component_insts,
        );
    }
}

pub(crate) fn collect_pass1(_uri: &str, include_system: bool) -> Value {
    let all_modules = crate::mcb_iter_modules_with_span();
    let all_components = crate::mcb_iter_components_with_span();
    let all_interfaces = crate::mcb_iter_interfaces_with_span();
    let all_enums = crate::mcb_iter_enums_with_span();
    let all_ports = crate::mcb_iter_ports();

    // Filter out system modules, components, interfaces, enums if not include_system
    let (modules, components, interfaces, enums) = if include_system {
        (all_modules, all_components, all_interfaces, all_enums)
    } else {
        let filter =
            |items: Vec<(String, String, [usize; 2])>| -> Vec<(String, String, [usize; 2])> {
                items
                    .into_iter()
                    .filter(|(_, uri, _)| !is_system_uri(uri))
                    .collect()
            };
        (
            filter(all_modules),
            filter(all_components),
            filter(all_interfaces),
            filter(all_enums),
        )
    };

    // Filter ports - only include ports from non-system modules
    let ports: Vec<_> = if include_system {
        all_ports
    } else {
        all_ports
            .into_iter()
            .filter(|(_, _, _, uri)| !is_system_uri(uri))
            .collect()
    };

    let mut by_uri: BTreeMap<String, FileEntry> = BTreeMap::new();
    for m in &modules {
        let uri = m.1.clone();
        let e = by_uri
            .entry(uri.clone())
            .or_insert_with(|| FileEntry::new(&uri));
        e.modules.push(m.0.clone());
    }
    for c in &components {
        let uri = c.1.clone();
        let e = by_uri
            .entry(uri.clone())
            .or_insert_with(|| FileEntry::new(&uri));
        e.components.push(c.0.clone());
    }
    for i in &interfaces {
        let uri = i.1.clone();
        let e = by_uri
            .entry(uri.clone())
            .or_insert_with(|| FileEntry::new(&uri));
        e.interfaces.push(i.0.clone());
    }
    for en in &enums {
        let uri = en.1.clone();
        let e = by_uri
            .entry(uri.clone())
            .or_insert_with(|| FileEntry::new(&uri));
        e.enums.push(en.0.clone());
    }

    let loaded_files: Vec<Value> = by_uri.into_values().map(|f| f.into_json()).collect();

    // Convert ports to PortRef format
    let ports_json: Vec<serde_json::Value> = ports
        .iter()
        .map(|(name, iotype, module, uri)| {
            serde_json::json!({
                "name": name,
                "iotype": iotype,
                "module": module,
                "uri": uri
            })
        })
        .collect();

    json!({
        "loaded_files": loaded_files,
        "definitions": {
            "modules":    refs_json(&modules),
            "components": refs_json(&components),
            "interfaces": refs_json(&interfaces),
            "enums":      refs_json(&enums),
            "ports":      ports_json,
        },
        "diagnostics": []
    })
}

pub(crate) fn collect_pass2(top: &str, inst: &crate::MccProjectTree) -> Value {
    json!({
        "top": top,
        "instances": instance_to_json(inst),
        "connections": extract_connections(inst),
        "nets":       extract_nets(inst),
        "diagnostics": []
    })
}

pub(crate) fn extract_connections(inst: &crate::MccProjectTree) -> Vec<Value> {
    let mut out = Vec::new();
    walk_connections(inst, "", &mut out);
    out
}

/// Flatten the instance tree's connections into envelope rows. Each row carries
/// its module scope (e.g. `main.speaker`) because connection ids and instance
/// names repeat across modules — the CLI's `ConnectionEntry` requires it. The
/// net name is resolved against this module's net table so every connection
/// gets the same surviving name as the matching nets row (mirrors
/// `cmds::parse::walk_connections`); the statement label is only a fallback.
pub(crate) fn walk_connections(inst: &crate::MccProjectTree, scope: &str, out: &mut Vec<Value>) {
    let my_scope = if scope.is_empty() {
        inst.name.clone()
    } else {
        format!("{}.{}", scope, inst.name)
    };
    let mut point_to_net: HashMap<&str, &str> = HashMap::new();
    for (net_name, points) in &inst.nets {
        for p in points {
            point_to_net.entry(p.path.as_str()).or_insert(net_name);
        }
    }
    for conn in &inst.connections {
        let net_name = conn
            .points
            .iter()
            .find_map(|p| point_to_net.get(p.path.as_str()).copied())
            .map(str::to_string)
            .or_else(|| conn.net_name.clone());
        out.push(json!({
            "id": conn.id,
            "module": my_scope,
            "net_name": net_name,
            "points": conn.points.iter().map(|p| p.path.clone()).collect::<Vec<_>>(),
        }));
    }
    for sub in &inst.sub_modules {
        walk_connections(sub, &my_scope, out);
    }
}

pub(crate) fn instance_to_json(inst: &crate::MccProjectTree) -> Value {
    use crate::IOType;
    let ports: Vec<Value> = inst
        .ports
        .iter()
        .filter(|p| !matches!(p.iotype, IOType::None | IOType::NonCon | IOType::Return))
        .map(|p| {
            json!({
                "name":   p.name.to_string(),
                "iotype": iotype_str(&p.iotype),
            })
        })
        .collect();
    let components: Vec<Value> = inst
        .components
        .iter()
        .map(|c| {
            let pins: Vec<Value> = c
                .pins
                .keys()
                .map(|pin_id| {
                    let pin_name = c.pin_name(pin_id).unwrap_or_else(|| pin_id.clone());
                    json!({
                        "id":   pin_id.clone(),
                        "name": pin_name,
                    })
                })
                .collect();
            json!({
                "name":       c.name.to_string(),
                "class_name": c.def.name.to_string(),
                "pins":       pins,
                "nc":         c.nc,
            })
        })
        .collect();
    let sub_modules: Vec<Value> = inst.sub_modules.iter().map(instance_to_json).collect();
    json!({
        "name":        inst.name.to_string(),
        "kind":        "module",
        "class_name":  inst.def.name.to_string(),
        "ports":       ports,
        "components":  components,
        "sub_modules": sub_modules,
    })
}

pub(crate) fn extract_nets(inst: &crate::MccProjectTree) -> Vec<Value> {
    let mut nets = Vec::new();
    walk_nets(inst, "", &mut nets);
    nets
}

/// Flatten the instance tree's nets into envelope rows, each tagged with its
/// module scope (`main.speaker`) as the CLI's `NetEntry` requires.
pub(crate) fn walk_nets(inst: &crate::MccProjectTree, scope: &str, out: &mut Vec<Value>) {
    let my_scope = if scope.is_empty() {
        inst.name.clone()
    } else {
        format!("{}.{}", scope, inst.name)
    };
    for (name, points) in inst.sorted_nets() {
        let points: Vec<String> = points.iter().map(|point| point.path.clone()).collect();
        out.push(json!({ "module": my_scope, "name": name, "points": points }));
    }
    for sub in &inst.sub_modules {
        walk_nets(sub, &my_scope, out);
    }
}

pub(crate) fn iotype_str(io: &crate::IOType) -> &'static str {
    use crate::IOType::*;
    match io {
        In => "in",
        Out => "out",
        InOut => "inout",
        Power => "power",
        Analog => "analog",
        Return => "return",
        NonCon => "noncon",
        Label => "label",
        None => "none",
    }
}

// ============================================================================
// File entry grouping
// ============================================================================

impl FileEntry {
    fn new(uri: &str) -> Self {
        Self {
            uri: uri.to_string(),
            is_system: is_system_uri(uri),
            modules: vec![],
            components: vec![],
            interfaces: vec![],
            enums: vec![],
        }
    }
    fn into_json(self) -> Value {
        json!({
            "uri":        self.uri,
            "is_system":  self.is_system,
            "modules":    self.modules,
            "components": self.components,
            "interfaces": self.interfaces,
            "enums":      self.enums,
        })
    }
}

/// Check if URI is a system library
pub(crate) fn is_system_uri(uri: &str) -> bool {
    uri.contains("/mcode/") || uri.contains("\\mcode\\")
}

pub(crate) fn refs_json(items: &[(String, String, [usize; 2])]) -> Vec<Value> {
    items
        .iter()
        .map(|(n, u, s)| json!({"name": n, "uri": u, "span": s}))
        .collect()
}

pub(crate) fn load_libs_rpc(libs: &[String]) {
    // Non-project mode: when the caller supplies no explicit library list,
    // fall back to the global mcc.yaml [libs].load configuration so custom
    // path libraries are still loaded (mcext-folder-parse-design.md §5.2).
    let libs: Vec<String> = if libs.is_empty() {
        crate::cli::config::get_libs_load_list(None).to_vec()
    } else {
        libs.to_vec()
    };
    if libs.is_empty() {
        return;
    }
    for name in &libs {
        // Reuse the shared library loader: it supports absolute paths and .mc
        // file forms, and skips libraries that are already loaded.
        crate::mcb_load_lib_by_name(name);
    }
}

use std::sync::atomic::{AtomicU64, Ordering};
static OVERLAY_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a unique overlay URI for this request.
/// Concurrent AI clients each get their own URI → no cross-contamination.
pub(crate) fn make_overlay_uri() -> McURI {
    let n = OVERLAY_COUNTER.fetch_add(1, Ordering::Relaxed);
    let s = format!("/mcc/check_{}.mc", n);
    McURI::from(s.as_str())
}

/// Remove a previously loaded overlay from the workspace.
/// Called after the AI check completes to prevent accumulation.
pub(crate) fn remove_overlay(uri: &McURI) {
    crate::build::loader::mcb_remove(uri);
}

// ============================================================================
// Refs (M6)
// ============================================================================

// ============================================================================
// ERC — Electrical Rule Check (M6)
// ============================================================================

/// Run Pass2 ERC: single-point nets, unconnected ports, net stats.
pub(crate) fn run_erc() -> RpcResult {
    let top = crate::mcb_get_first_module_name()
        .ok_or_else(|| JsonRpcError::custom(32112, "semantic: no modules found"))?;

    // Resolve the top module to its defining file URI; using the bare module
    // name as a URI makes mcc_build fail with "Target module not found".
    let uri = crate::mcb_iter_modules()
        .into_iter()
        .find(|(name, _)| name == &top)
        .map(|(_, u)| crate::McURI::from(u.as_str()))
        .unwrap_or_else(|| crate::McURI::from(top.as_str()));
    let ident = crate::McIds::from(top.as_str());

    let inst = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::mcc_build(&ident, &uri)
    }))
    .map_err(|_| JsonRpcError::custom(32111, "semantic: build panicked"))?
    .map_err(|e| JsonRpcError::custom(32111, &format!("semantic: build failed: {e}")))?;

    let mut diags: Vec<Value> = Vec::new();

    // ── Single-point nets ──
    let single_point: Vec<&String> = inst
        .nets
        .iter()
        .filter(|(name, points)| {
            !crate::instant::mc_net::is_anon_net_name(name)
                && points.len() <= 1
                && name.as_str() != "NC"
        })
        .map(|(name, _)| name)
        .collect();

    for net in &single_point {
        let code = crate::errcodes::ERC_SINGLE_POINT_NET;
        diags.push(json!({
            "code": code,
            "severity": "warning",
            "message": format!("single-point net: '{net}' has only one connection — may be unconnected"),
            "check": "single_point_net",
        }));
    }

    // ── Unconnected ports ──
    let all_net_paths: std::collections::HashSet<&str> = inst
        .nets
        .iter()
        .flat_map(|(_, pts)| pts.iter())
        .map(|p| p.path.as_str())
        .collect();

    for port in &inst.ports {
        if !all_net_paths.contains(port.name.as_str()) {
            let code = crate::errcodes::ERC_UNCONNECTED_PORT;
            diags.push(json!({
                "code": code,
                "severity": "warning",
                "message": format!("unconnected port: '{}' is not connected to any net", port.name),
                "check": "unconnected_port",
            }));
        }
    }

    // ── Multi-drive / floating net detection ──
    let mut multi_drive = 0u32;
    let mut floating = 0u32;

    for (name, points) in &inst.nets {
        if crate::instant::mc_net::is_anon_net_name(name) || name.as_str() == "NC" {
            continue;
        }
        // Classify points: is_driver (Out, InOut, Power, Analog) vs is_load (In, ...)
        let drivers: Vec<_> = points
            .iter()
            .filter(|p| {
                matches!(
                    p.iotype,
                    crate::semantic::common::IOType::Out
                        | crate::semantic::common::IOType::InOut
                        | crate::semantic::common::IOType::Power
                        | crate::semantic::common::IOType::Analog
                )
            })
            .collect();

        if drivers.len() > 1 {
            multi_drive += 1;
            let names: Vec<_> = drivers.iter().map(|d| d.path.as_str()).collect();
            let code = crate::errcodes::ERC_MULTI_DRIVE_NET;
            diags.push(json!({
                "code": code,
                "severity": "error",
                "check": "multi_drive",
                "message": format!(
                    "multi-drive net: '{}' has {} drivers ({}) — short circuit risk",
                    name, drivers.len(),
                    names.join(", ")
                ),
            }));
        } else if drivers.is_empty() && points.len() > 1 {
            floating += 1;
            let code = crate::errcodes::ERC_FLOATING_NET;
            diags.push(json!({
                "code": code,
                "severity": "warning",
                "check": "floating_net",
                "message": format!(
                    "floating net: '{}' has no driver (no Out/InOut/Power/Analog pin)",
                    name
                ),
            }));
        }
    }

    Ok(json!({
        "summary": {
            "errors": diags.iter().filter(|d| d["severity"] == "error").count(),
            "warnings": diags.iter().filter(|d| d["severity"] == "warning").count(),
            "erc": {
                "net_count": inst.nets.len(),
                "connection_count": inst.connections.len(),
                "component_count": inst.components.len(),
                "port_count": inst.ports.len(),
                "single_point_nets": single_point.len(),
                "unconnected_ports": diags.iter().filter(|d| d["check"] == "unconnected_port").count(),
                "multi_drive_nets": multi_drive,
                "floating_nets": floating,
            }
        },
        "diagnostics": diags,
    }))
}

pub(crate) fn extract_from_uri(entry: &Path, top: Option<&str>, target: &str) -> RpcResult {
    let uri = entry.to_string_lossy().to_string();
    let mc_uri = McURI::from(uri.as_str());

    let top_name = match top {
        Some(t) => t.to_string(),
        None => crate::mcb_get_module_name_by_uri(&mc_uri)
            .or_else(crate::mcb_get_first_module_name)
            .ok_or_else(|| JsonRpcError::custom(32107, "no top module found"))?,
    };

    match target {
        "instances" | "\"instances\"" => {
            let ident = crate::McIds::from(top_name.as_str());
            if let Some(cmie) = crate::get_def(&ident, &mc_uri) {
                if let crate::McCMIE::Module(module_def) = cmie {
                    let items: Vec<Value> = module_def
                        .insts
                        .iter()
                        .map(|(name, inst)| {
                            let (kind, class) = match inst {
                                crate::McInstance::Component(c) => {
                                    ("component", c.name.to_string())
                                }
                                crate::McInstance::Module(m) => ("module", m.name.to_string()),
                                crate::McInstance::Label(l) => ("label", l.clone()),
                                crate::McInstance::Interface(i) => {
                                    ("interface", i.name.to_string())
                                }
                                crate::McInstance::Bus(b) => ("bus", b.to_string()),
                                crate::McInstance::BusRef { component, bus } => {
                                    ("busref", format!("{component}.{bus}"))
                                }
                                crate::McInstance::List(l) => {
                                    let name = l.name().to_string();
                                    let class = format!("{:?}", l);
                                    if class != name {
                                        ("list", class)
                                    } else {
                                        ("list", name)
                                    }
                                }
                                crate::McInstance::Unresolved { class_name } => {
                                    ("unresolved", class_name.clone())
                                }
                                crate::McInstance::Pins => ("pins", "pins".into()),
                                crate::McInstance::PinId(id) => ("pinid", id.clone()),
                                crate::McInstance::Attr(a) => ("attr", a.to_string()),
                                crate::McInstance::Func(f) => ("func", f.name.to_string()),
                                crate::McInstance::EnumVal {
                                    enum_name,
                                    value_name,
                                    ..
                                } => ("enumval", format!("{}.{}", enum_name, value_name)),
                            };
                            json!({ "name": name.to_string(), "kind": kind, "class": class })
                        })
                        .collect();
                    Ok(json!({ "target": "instances", "items": items }))
                } else {
                    Err(JsonRpcError::custom(
                        -32107,
                        &format!("'{top_name}' is not a Module"),
                    ))
                }
            } else {
                Err(JsonRpcError::custom(
                    -32107,
                    &format!("Definition '{top_name}' not found"),
                ))
            }
        }
        "nets" | "\"nets\"" => {
            let ident = crate::McIds::from(top_name.as_str());
            let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::mcc_build(&ident, &mc_uri)
            }));
            match built {
                Ok(Ok(inst)) => {
                    use std::collections::BTreeMap;
                    let mut nets: BTreeMap<String, Vec<String>> = BTreeMap::new();
                    for conn in &inst.connections {
                        let net = conn.effective_net_name();
                        if net == "NC" { continue; }
                        let bucket = nets.entry(net).or_default();
                        for p in &conn.points {
                            if p.path == "NC" { continue; }
                            let label = if let Some(ref o) = p.owner {
                                format!("{}.{}", o, p.path.split('.').next_back().unwrap_or(&p.path))
                            } else { p.path.clone() };
                            if !bucket.contains(&label) { bucket.push(label); }
                        }
                    }
                    let items: Vec<Value> = nets
                        .into_iter()
                        .map(|(name, points)| json!({ "name": name, "points": points }))
                        .collect();
                    Ok(json!({ "target": "nets", "items": items }))
                }
                Ok(Err(e)) => Err(JsonRpcError::custom(32107, &format!("build failed: {e}"))),
                Err(_) => Err(JsonRpcError::custom(
                    -32108,
                    "extract nets: Pass2 build panicked (engine bug); request aborted, server kept alive",
                )),
            }
        }
        "components" | "\"components\"" => {
            let items: Vec<Value> = crate::mcb_iter_components()
                .into_iter()
                .map(|(name, uri)| json!({ "name": name, "uri": uri }))
                .collect();
            Ok(json!({ "target": "components", "items": items }))
        }
        "interfaces" | "\"interfaces\"" => {
            let items: Vec<Value> = crate::mcb_iter_interfaces()
                .into_iter()
                .map(|(name, uri)| json!({ "name": name, "uri": uri }))
                .collect();
            Ok(json!({ "target": "interfaces", "items": items }))
        }
        other => Err(JsonRpcError::custom(
            -32602,
            &format!("unknown extract target: {other}"),
        )),
    }
}

// ============================================================================
// Auxiliary: parameter parsing / error handling
// ============================================================================

pub(crate) fn parse_strict<T: for<'de> Deserialize<'de>>(
    params: Option<Value>,
) -> Result<T, JsonRpcError> {
    let v = params.ok_or_else(JsonRpcError::invalid_params)?;
    serde_json::from_value(v).map_err(|_| JsonRpcError::invalid_params())
}

pub(crate) fn parse_or_default<T: for<'de> Deserialize<'de> + Default>(
    params: Option<Value>,
) -> Result<T, JsonRpcError> {
    match params {
        Some(v) => serde_json::from_value(v).map_err(|_| JsonRpcError::invalid_params()),
        None => Ok(T::default()),
    }
}

pub(crate) fn parse_string_param(
    params: Option<Value>,
    keys: &[&str],
) -> Result<String, JsonRpcError> {
    match params {
        Some(Value::String(s)) => Ok(s),
        Some(Value::Object(mut m)) => {
            for k in keys {
                if let Some(Value::String(s)) = m.remove(*k) {
                    return Ok(s);
                }
            }
            Err(JsonRpcError::invalid_params())
        }
        _ => Err(JsonRpcError::invalid_params()),
    }
}

pub(crate) fn io_err(e: std::io::Error) -> JsonRpcError {
    JsonRpcError::custom(32100, &format!("io error: {e}"))
}

// ============================================================================
// Auxiliary: file / path handling
// ============================================================================

pub(crate) fn is_safe_relative(p: &str) -> bool {
    use std::path::Component;
    let path = Path::new(p);
    if path.is_absolute() {
        return false;
    }
    for c in path.components() {
        match c {
            Component::ParentDir | Component::Prefix(_) | Component::RootDir => return false,
            _ => {}
        }
    }
    true
}

pub(crate) fn resolve_project_entry(
    _name: &str,
    entry: Option<&str>,
) -> Result<PathBuf, JsonRpcError> {
    let (_, _, root_str) = crate::workspace_info();
    let root = PathBuf::from(&root_str);
    let src_root = root.join("src");

    // Prefer src/ directory
    if let Some(rel) = entry {
        // Handle absolute paths directly
        let abs_path = PathBuf::from(rel);
        if abs_path.is_absolute() {
            if abs_path.exists() {
                return Ok(abs_path);
            } else {
                return Err(JsonRpcError::custom(
                    32105,
                    &format!("entry not found: {rel}"),
                ));
            }
        }

        // Relative path: check safety
        if !is_safe_relative(rel) {
            return Err(JsonRpcError::custom(
                32105,
                &format!("unsafe entry path: {rel}"),
            ));
        }
        // Search in src/
        let p = src_root.join(rel);
        if p.exists() {
            return Ok(p);
        }
        // Then in root
        let p = root.join(rel);
        if !p.exists() {
            return Err(JsonRpcError::custom(
                -32105,
                &format!("entry not found: {rel}"),
            ));
        }
        return Ok(p);
    }

    // Read entry from project.toml
    if let Some(rel) = read_project_entry_from_workspace() {
        // Search in src/
        let p = src_root.join(&rel);
        if p.exists() {
            return Ok(p);
        }
        // Then in root
        let p = root.join(&rel);
        if p.exists() {
            return Ok(p);
        }
    }

    // fallback: scan src/ for first .mc file
    let mut found = Vec::new();
    scan_mc_files_recursive(&src_root, &src_root, &mut found);
    if let Some(rel) = found.first() {
        return Ok(src_root.join(rel));
    }
    Err(JsonRpcError::custom(32105, "no .mc entry found in src/"))
}

pub(crate) fn scan_mc_files_recursive(root: &Path, current: &Path, out: &mut Vec<String>) {
    if let Ok(entries) = fs::read_dir(current) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                scan_mc_files_recursive(root, &p, out);
            } else if p.extension().is_some_and(|ext| ext == "mc") {
                if let Ok(rel) = p.strip_prefix(root) {
                    out.push(rel.to_string_lossy().to_string());
                }
            }
        }
    }
}

pub(crate) fn read_project_entry_from_workspace() -> Option<String> {
    let (_, _, root_str) = crate::workspace_info();
    let root = PathBuf::from(&root_str);
    let manifest = crate::cli::datadir::find_manifest_in(&root)?;
    let content = fs::read_to_string(&manifest).ok()?;
    parse_manifest_field(&content, "entry")
}

pub(crate) fn read_project_top_from_workspace() -> Option<String> {
    let (_, _, root_str) = crate::workspace_info();
    let root = PathBuf::from(&root_str);
    let manifest = crate::cli::datadir::find_manifest_in(&root)?;
    let content = fs::read_to_string(&manifest).ok()?;
    parse_manifest_field(&content, "top_module")
}

pub(crate) fn parse_manifest_field(content: &str, key: &str) -> Option<String> {
    // Simple TOML parser: support [project] section
    let mut in_project_section = false;

    for line in content.lines() {
        let line = line.trim();

        // Detect section
        if line.starts_with('[') && line.ends_with(']') {
            in_project_section = line.contains("project");
            continue;
        }

        // Search in project section
        if in_project_section && line.starts_with(key) {
            if let Some(eq) = line.find('=') {
                let v = line[eq + 1..].trim().trim_matches('"').trim();
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

pub(crate) fn resolve_lib_root(name: &str) -> Result<PathBuf, JsonRpcError> {
    // Delegate to the single library-root resolver so the RPC/IDE path and the
    // CLI path agree: system root first (always the data root: MCC_SYSTEM_ROOT
    // env, then ~/.mcode), then data_root fallback. mcode resolves under
    // each root with a sibling fallback; other libraries match versioned
    // `<name>@<version>` then bare `<name>` (use-design §19.5 rule 2).
    crate::db::infra::libmgr::resolve_lib_root(name)
        .ok_or_else(|| JsonRpcError::custom(-32102, &format!("library '{name}' not installed")))
}

// ============================================================================
// Load handlers
// ============================================================================

// ============================================================================
// Parse handlers
// ============================================================================

// ============================================================================
// Show handlers
// ============================================================================

/// Resolve a file path to an absolute URI string for filtering.
pub(crate) fn resolve_to_abs_uri(file: &str) -> String {
    let path = std::path::Path::new(file);
    if let Ok(canonical) = path.canonicalize() {
        canonical.to_string_lossy().to_string()
    } else if path.is_absolute() {
        file.to_string()
    } else if let Ok(cwd) = std::env::current_dir() {
        cwd.join(path).to_string_lossy().to_string()
    } else {
        file.to_string()
    }
}

/// Filter (name, uri) pairs to only those that belong to the same project as
/// `file`. An item belongs if its URI equals the resolved file path, or is
/// under the same directory as the file (transitive `$include` files).
pub(crate) fn filter_items_by_file<T: Clone>(items: &[(T, String)], file: &str) -> Vec<T> {
    let target = resolve_to_abs_uri(file);
    let parent_dir = std::path::Path::new(&target)
        .parent()
        .map(|p| p.to_string_lossy().to_string());

    items
        .iter()
        .filter(|(_, uri)| {
            if uri == &target {
                return true;
            }
            if let Some(ref dir) = parent_dir {
                if uri.starts_with(dir) {
                    return true;
                }
            }
            false
        })
        .map(|(n, _)| n.clone())
        .collect()
}

// ============================================================================
// Show helpers (shared across drill-down handlers)
// ============================================================================

/// Find a definition by name across all four kinds.
pub(crate) fn find_def_by_name(name: &str) -> Option<(crate::McCMIE, String)> {
    crate::lsp::gotodef::find_def_by_name_raw(name)
}

/// Split a dot-qualified entity path into `(owner, member)`.
///
/// Used to reference funcs, which are nested inside a module/component and
/// therefore not top-level definitions: `main.setup` → `("main", "setup")`.
///
/// The owner is everything before the **last** dot, so dotted class names work
/// too: `comp.sub.i2c` → `("comp.sub", "i2c")`.
pub fn split_owner_member(name: &str) -> Option<(&str, &str)> {
    name.rsplit_once('.')
}

/// Resolve a dot-qualified function path `OWNER.FUNC` where OWNER is a loaded
/// Module or Component. Returns a clone of the function, or `None` when the
/// name is not a (owner, func) pair, the owner is not found, or the owner has
/// no such function.
pub fn find_func_by_path(name: &str) -> Option<crate::semantic::mc_func::McFunction> {
    let (owner, member) = split_owner_member(name)?;
    let (cmie, _) = find_def_by_name(owner)?;
    match &cmie {
        crate::McCMIE::Component(c) => c.funcs.find(member).cloned(),
        crate::McCMIE::Module(m) => m.funcs.find(member).cloned(),
        _ => None,
    }
}

/// Build a phrase-level net map from a function body (no Pass2 — funcs depend
/// on parameters and a calling context that are not available standalone).
///
/// Each connection stmt in `func.stmts` becomes one net entry named `stmt_N`
/// (1-based), whose points are the endpoint names referenced by that stmt.
pub fn func_nets_map(func: &crate::semantic::mc_func::McFunction) -> BTreeMap<String, Vec<String>> {
    let mut nets: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (i, stmt) in func.stmts.iter().enumerate() {
        let mut names: std::collections::HashSet<String> = std::collections::HashSet::new();
        crate::semantic::validation::body::collect_referenced_names(stmt, &mut names);
        let mut points: Vec<String> = names.into_iter().collect();
        points.sort();
        nets.insert(format!("stmt_{}", i + 1), points);
    }
    nets
}

/// Natural ordering for pin IDs in the `pins` view of `show dump` / `show
/// pins`.
///
/// Rule:
///   * pure-numeric pin IDs sort first, numerically — `1, 2, ..., 9, 10,
///     11` instead of lexicographic `1, 10, 11, 2, ...`;
///   * non-numeric pin IDs sort after them, "naturally": letter runs
///     compare lexically while embedded digit runs compare numerically, so
///     `A9 < A10` and `B1 > A9`.
fn pin_id_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let a_numeric = !a.is_empty() && a.bytes().all(|c| c.is_ascii_digit());
    let b_numeric = !b.is_empty() && b.bytes().all(|c| c.is_ascii_digit());
    // Numeric IDs first, then natural comparison within each group.
    a_numeric
        .cmp(&b_numeric)
        .reverse()
        .then_with(|| natural_cmp(a, b))
}

/// Compare two strings run-by-run: digit runs numerically, every other run
/// lexically (natural sort). `A9 < A10`, `A2 < A10`, `PA0 < PB0`.
fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let (ba, bb) = (a.as_bytes(), b.as_bytes());
    let (mut ia, mut ib) = (0usize, 0usize);
    loop {
        if ia == ba.len() && ib == bb.len() {
            return std::cmp::Ordering::Equal;
        }
        if ia == ba.len() {
            return std::cmp::Ordering::Less;
        }
        if ib == bb.len() {
            return std::cmp::Ordering::Greater;
        }
        if ba[ia].is_ascii_digit() && bb[ib].is_ascii_digit() {
            // Both runs are digits: compare them numerically.
            let (sa, sb) = (ia, ib);
            while ia < ba.len() && ba[ia].is_ascii_digit() {
                ia += 1;
            }
            while ib < bb.len() && bb[ib].is_ascii_digit() {
                ib += 1;
            }
            match numeric_str_cmp(&a[sa..ia], &b[sb..ib]) {
                std::cmp::Ordering::Equal => {}
                ord => return ord,
            }
        } else {
            match ba[ia].cmp(&bb[ib]) {
                std::cmp::Ordering::Equal => {
                    ia += 1;
                    ib += 1;
                }
                ord => return ord,
            }
        }
    }
}

/// Compare two digit-run strings numerically (`"9" < "10"`). Leading zeros
/// are ignored so `"01"` ties with `"1"`.
fn numeric_str_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let ta = a.trim_start_matches('0');
    let tb = b.trim_start_matches('0');
    match ta.len().cmp(&tb.len()) {
        std::cmp::Ordering::Equal => ta.cmp(tb),
        ord => ord,
    }
}

/// Build the pin JSON view (pins + interfaces + name/id mappings). Single
/// implementation shared by the RPC handlers and the local CLI `show`
/// commands so both stay in parity.
pub fn pins_json(pins: &crate::McPins) -> Value {
    // Interface/group instances registered on this component, keyed by
    // instance name (e.g. "I2C0" → I2C0::I2C(Master)) or bare group name
    // ("PBus" → Bus). Built once and shared by the per-pin `interfaces` field
    // and the top-level `interfaces` summary.
    let mut ifaces: Vec<Value> = pins
        .names_to_id
        .iter()
        .filter_map(|(name, port)| match port {
            crate::McPinPort::Interface(iface) => {
                let mut param_strs: Vec<String> =
                    iface.params.iter().map(|p| p.to_string()).collect();
                let params: Value = if param_strs.is_empty() {
                    Value::Null
                } else {
                    // Drop placeholder "_" arguments so `I2C0::I2C(Master)`
                    // renders without trailing noise.
                    if param_strs.iter().all(|s| s == "_") {
                        Value::Null
                    } else {
                        param_strs.retain(|s| s != "_");
                        json!(param_strs)
                    }
                };
                // Pin names belonging to this interface (e.g. I2C0.SCL),
                // derived from the Single entries whose pin is registered.
                // A name whose prefix is another interface instance name
                // (e.g. I2C0.SCL on a GPIO-registered pin) is excluded.
                let iface_keys: std::collections::BTreeSet<String> = pins
                    .names_to_id
                    .iter()
                    .filter_map(|(n, port)| match port {
                        crate::McPinPort::Interface(_) => Some(n.clone()),
                        _ => None,
                    })
                    .collect();
                let mut pin_names: Vec<String> = pins
                    .names_to_id
                    .iter()
                    .filter_map(|(n, port)| match port {
                        crate::McPinPort::Single(pid) => {
                            let prefix = n.split('.').next();
                            let owned_by_other = prefix
                                .map(|p| iface_keys.contains(p) && p != name.as_str())
                                .unwrap_or(false);
                            if !owned_by_other && iface.registered_pins.iter().any(|p| p == pid) {
                                Some(n.clone())
                            } else {
                                None
                            }
                        }
                        _ => None,
                    })
                    .collect();
                pin_names.sort();
                Some(json!({
                    "name": name,
                    "inst_name": iface.name.to_string(),
                    "base_name": iface.base_name(),
                    "params": params,
                    "pins": iface.registered_pins,
                    "pin_names": pin_names,
                    "kind": "Interface",
                }))
            }
            crate::McPinPort::Bus(bus) => {
                // `PBus{CLK, DATA}`: a bus instance. Individual pins register
                // dot-qualified names (PBus.CLK), and the bare bus name
                // (PBus) is registered as the Bus port itself.
                let members: Vec<String> = bus.full_members.clone();
                let mut pin_names: Vec<String> = pins
                    .names_to_id
                    .iter()
                    .filter_map(|(n, port)| match port {
                        crate::McPinPort::Single(_) => {
                            if n.starts_with(&format!("{name}.")) {
                                Some(n.clone())
                            } else {
                                None
                            }
                        }
                        _ => None,
                    })
                    .collect();
                pin_names.sort();
                Some(json!({
                    "name": name,
                    "inst_name": name,
                    "base_name": name,
                    "params": Value::Null,
                    "pins": Value::Null,
                    "members": members,
                    "pin_names": pin_names,
                    "kind": "Bus",
                }))
            }
            _ => None,
        })
        .collect();

    // List groups (e.g. `PDM[CLK, DATA]`) are NOT registered in
    // `names_to_id` (§2.1 bare-prefix rule), so they are appended from the
    // display-only table recorded during pins parsing.
    for (list_name, members, pids) in &pins.list_groups {
        let mut pin_names: Vec<String> = pins
            .names_to_id
            .iter()
            .filter_map(|(n, port)| match port {
                crate::McPinPort::Single(pid) => {
                    if pids.contains(pid) && n.starts_with(list_name) {
                        Some(n.clone())
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect();
        pin_names.sort();
        ifaces.push(json!({
            "name": list_name,
            "inst_name": list_name,
            "base_name": list_name,
            "params": Value::Null,
            "pins": Value::Null,
            "members": members,
            "pin_names": pin_names,
            "kind": "List",
        }));
    }

    // For each physical pin, the interface instances that occupy it.
    // Pins are listed in natural order (see `pin_id_cmp`) so numeric pin
    // IDs appear in numeric sequence instead of lexicographic order.
    let mut pin_entries: Vec<_> = pins.pins.iter().collect();
    pin_entries.sort_by(|(a, _), (b, _)| pin_id_cmp(a, b));
    let pin_list: Vec<Value> = pin_entries
        .into_iter()
        .map(|(pin_id, pin)| {
            let mut desc = String::new();
            for val in pin.values.iter() {
                if let crate::McAttrVal::AttrLiteral(crate::McLiteral::String(s)) = val {
                    if !desc.is_empty() {
                        desc.push(' ');
                    }
                    desc.push_str(&s.value);
                }
            }
            // Non-string pin values (KVS like `volt:[...]`, ranges, ...),
            // rendered in full so dumps show everything the parser produced.
            let extra_vals: Vec<String> = pin
                .values
                .iter()
                .filter(|val| {
                    !matches!(
                        val,
                        crate::McAttrVal::AttrLiteral(crate::McLiteral::String(_))
                    )
                })
                .map(|val| val.to_string())
                .collect();
            // An interface/group belongs to this pin when it registers the
            // pin (e.g. I2C0 → [1,2]) or when the pin's name matches the
            // group's naming scheme: dot-qualified for interfaces and buses
            // (`I2C1.SCL` → I2C1, `PBus.CLK` → PBus), concatenated for List
            // groups (`PDMCLK` → PDM). The prefix fallback covers repeated
            // interface instance names where `registered_pins` only keeps
            // the last occurrence.
            let mut pin_ifaces: Vec<Value> = ifaces
                .iter()
                .filter(|i| {
                    let kind = i
                        .get("kind")
                        .and_then(|k| k.as_str())
                        .unwrap_or("Interface");
                    let key = i.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    match kind {
                        "List" => pin
                            .names
                            .iter()
                            .any(|n| n.starts_with(key) && n.len() > key.len()),
                        "Bus" => pin.names.iter().any(|n| n.split('.').next() == Some(key)),
                        _ => {
                            let via_reg = i
                                .get("pins")
                                .and_then(|p| p.as_array())
                                .map(|arr| arr.iter().any(|p| p == pin_id))
                                .unwrap_or(false);
                            let via_prefix =
                                pin.names.iter().any(|n| n.split('.').next() == Some(key));
                            via_reg || via_prefix
                        }
                    }
                })
                .map(|i| {
                    json!({
                        "name": i["inst_name"],
                        "base": i["base_name"],
                        "params": i["params"],
                        "kind": i["kind"],
                        "members": i["members"],
                    })
                })
                .collect();
            // Reorder `|` alternates to match the source declaration order
            // recorded in `pin_iface_order` (instead of the alphabetical
            // `names_to_id` order); entries without a recorded position keep
            // their relative order at the end.
            if let Some(order) = pins.pin_iface_order.get(pin_id) {
                pin_ifaces.sort_by_key(|i| {
                    let name = i.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    order.iter().position(|k| k == name).unwrap_or(usize::MAX)
                });
            }
            let mut j = json!({
                "id": pin_id,
                "iotype": format!("{:?}", pin.iotype),
                "names": pin.names,
                "interfaces": pin_ifaces,
            });
            if !desc.is_empty() {
                j["description"] = json!(desc);
            }
            if !extra_vals.is_empty() {
                j["values"] = json!(extra_vals);
            }
            j
        })
        .collect();

    let mut names_to_id = serde_json::Map::new();
    for (k, v) in &pins.names_to_id {
        names_to_id.insert(k.clone(), pinport_json(v));
    }
    let mut pin_id_to_names = serde_json::Map::new();
    for (k, v) in &pins.pin_id_to_names {
        pin_id_to_names.insert(k.clone(), json!(v));
    }

    json!({
        "pin_count": pins.pins.len(),
        "pins": pin_list,
        "interfaces": ifaces,
        "names_to_id": Value::Object(names_to_id),
        "pin_id_to_names": Value::Object(pin_id_to_names),
    })
}

pub(crate) fn pinport_json(v: &crate::McPinPort) -> Value {
    match v {
        crate::McPinPort::Single(pid) => json!({ "kind": "Single", "pin": pid }),
        crate::McPinPort::Multi(pids) => json!({ "kind": "Multi", "pins": pids }),
        crate::McPinPort::MultiGroup(groups) => {
            json!({ "kind": "MultiGroup", "groups": groups })
        }
        crate::McPinPort::List(name, items) => {
            json!({ "kind": "List", "name": name, "items": items })
        }
        crate::McPinPort::Bus(bus) => json!({ "kind": "Bus", "debug": format!("{:?}", bus) }),
        crate::McPinPort::Interface(iface) => json!({
            "kind": "Interface",
            "inst_name": iface.name.to_string(),
            "base_name": iface.base_name(),
            "registered_pins": iface.registered_pins,
        }),
        crate::McPinPort::NC => json!({ "kind": "NC" }),
    }
}

pub(crate) fn inst_kind_class(inst: &crate::McInstance) -> (&'static str, String) {
    match inst {
        crate::McInstance::Component(c) => ("component", c.base.name.to_string()),
        crate::McInstance::Module(m) => ("module", m.base.name.to_string()),
        crate::McInstance::Label(l) => ("label", l.clone()),
        crate::McInstance::Interface(i) => ("interface", i.base_name()),
        crate::McInstance::Bus(b) => ("bus", b.to_string()),
        crate::McInstance::BusRef { component, bus } => ("busref", format!("{component}.{bus}")),
        crate::McInstance::List(l) => {
            let name = l.name().to_string();
            let class = format!("{:?}", l);
            if class != name {
                ("list", class)
            } else {
                ("list", name)
            }
        }
        crate::McInstance::Unresolved { class_name } => ("unresolved", class_name.clone()),
        crate::McInstance::Pins => ("pins", "pins".into()),
        crate::McInstance::PinId(id) => ("pinid", id.clone()),
        crate::McInstance::Attr(a) => ("attr", a.to_string()),
        crate::McInstance::Func(f) => ("func", f.name.to_string()),
        crate::McInstance::EnumVal {
            enum_name,
            value_name,
            ..
        } => ("enumval", format!("{}.{}", enum_name, value_name)),
    }
}

pub(crate) fn attrval_json(v: &crate::McAttrVal) -> Value {
    match v {
        // Keep string literals quoted so the dump shows the source form.
        crate::McAttrVal::AttrLiteral(crate::McLiteral::String(s)) => {
            json!(format!("\"{}\"", s.value))
        }
        other => json!(other.to_string()),
    }
}

// ============================================================================
// Show — missing container handlers
// ============================================================================

// ============================================================================
// Show — drill-down handlers
// ============================================================================

/// Convert a McParamDeclare to a JSON object with smart parameter metadata.
pub fn param_declare_to_json(d: &crate::semantic::basic::mc_paramd::McParamDeclare) -> Value {
    let name = d.get_primary_name().unwrap_or_default();
    let is_port = d.is_port();
    let has_default = d.has_default_value();
    let default_val = d.param_type.default_value().map(|s| s.to_string());
    let class_name = d.get_class_name();
    let iface_params: Vec<String> = d.param_type.interface_params().to_vec();
    json!({
        "name": name,
        "type": d.param_type.category_name(),
        "is_port": is_port,
        "has_default": has_default,
        "default": default_val,
        "class": class_name,
        "params": iface_params,
    })
}

// JSON builders for each entity kind (used by handle_show_dump and handle_show_dump_all)
pub(crate) fn dump_component_json(name: &str, comp: &crate::McComponent, uri: &str) -> Value {
    let params: Vec<Value> = comp.params.names_full().iter().map(|n| json!(n)).collect();
    let params_with_defaults: Vec<Value> = comp
        .params
        .get_params_with_defaults()
        .iter()
        .map(|(id, default)| json!({"name": id.to_string(), "default": default}))
        .collect();
    let attrs: Vec<Value> = comp
        .attrs
        .iter()
        .map(|a| {
            let values: Vec<Value> = a.values.iter().map(attrval_json).collect();
            json!({"no": a.no, "name": a.id.to_string(), "values": values})
        })
        .collect();
    let funcs: Vec<Value> = comp
        .funcs
        .iter()
        .map(|f| {
            let body_stmts: Vec<String> = f.body_stmts_display();
            json!({
                "name": f.name.to_string(),
                "params": f.params.names_full_annotated(),
                "returns": f.returns.kind_str(),
                "called_time": f.called_time,
                "body_stmts": body_stmts,
            })
        })
        .collect();
    let instances: Vec<Value> = instances_json(&comp.insts, None);
    let layout = json!({
        "left": comp.layout.left,
        "right": comp.layout.right,
        "top": comp.layout.top,
        "bottom": comp.layout.bottom,
    });
    let cond_pins: Vec<String> = comp
        .cond_pins
        .iter()
        .map(|cp| format!("{:?}", cp))
        .collect();
    let cond_attrs: Vec<String> = comp
        .cond_attrs
        .iter()
        .map(|ca| format!("{:?}", ca))
        .collect();

    let mut data = pins_json(&comp.pins);
    data["name"] = json!(name);
    data["kind"] = json!("component");
    data["uri"] = json!(uri);
    data["span"] = json!({"start": comp.span.start, "end": comp.span.end});
    data["params"] = json!(params);
    data["params_with_defaults"] = json!(params_with_defaults);
    data["attrs"] = json!(attrs);
    data["funcs"] = json!(funcs);
    data["instances"] = json!(instances);
    data["layout"] = layout;
    data["cond_pins_count"] = json!(comp.cond_pins.len());
    data["cond_pins"] = json!(cond_pins);
    data["cond_attrs_count"] = json!(comp.cond_attrs.len());
    data["cond_attrs"] = json!(cond_attrs);
    data
}

pub(crate) fn dump_module_json(name: &str, module: &crate::McModule, uri: &str) -> Value {
    // Params. Interface-bound params keep their binding: `[VDD,GND]::DC(3.3V)`
    // → `{"name":"[VDD, GND]","iface":"DC","iface_params":["3.3V"]}`.
    let params: Vec<Value> = module
        .params
        .iter()
        .map(|d| {
            let display = json!(d.display_name());
            match d.interface_annotation() {
                Some((class, p)) => json!({
                    "name": d.display_name(),
                    "iface": class,
                    "iface_params": p,
                }),
                None => display,
            }
        })
        .collect();
    let params_with_defaults: Vec<Value> = module
        .params
        .get_params_with_defaults()
        .iter()
        .map(|(id, default)| json!({"name": id.to_string(), "default": default}))
        .collect();
    let instances: Vec<Value> = instances_json(&module.insts, None);
    let stmts: Vec<String> = module.stmts.iter().map(|l| l.to_string()).collect();
    let funcs: Vec<Value> = module
        .funcs
        .iter()
        .map(|f| {
            let body_stmts: Vec<String> = f.body_stmts_display();
            json!({
                "name": f.name.to_string(),
                "params": f.params.names_full_annotated(),
                "returns": f.returns.kind_str(),
                "called_time": f.called_time,
                "body_stmts": body_stmts,
            })
        })
        .collect();
    json!({
        "name": name,
        "kind": "module",
        "uri": uri,
        "span": {"start": module.span.start, "end": module.span.end},
        "params": params,
        "params_with_defaults": params_with_defaults,
        "instances": instances,
        "stmts_count": module.stmts.len(),
        "stmts": stmts,
        "funcs": funcs,
    })
}

pub(crate) fn dump_interface_json(name: &str, iface: &crate::McInterface, uri: &str) -> Value {
    let params: Vec<Value> = iface.params.names_full().iter().map(|n| json!(n)).collect();
    let params_with_defaults: Vec<Value> = iface
        .params
        .get_params_with_defaults()
        .iter()
        .map(|(id, default)| json!({"name": id.to_string(), "default": default}))
        .collect();
    let attrs: Vec<Value> = iface
        .attrs
        .iter()
        .map(|a| {
            let values: Vec<Value> = a.values.iter().map(attrval_json).collect();
            json!({"no": a.no, "name": a.id.to_string(), "values": values})
        })
        .collect();
    let roles: Vec<Value> = iface
        .roles
        .iter()
        .map(|r| json!({"name": r.name.to_string(), "pins": pins_json(&r.pins)}))
        .collect();

    let mut data = pins_json(&iface.pins);
    data["name"] = json!(name);
    data["kind"] = json!("interface");
    data["uri"] = json!(uri);
    data["params"] = json!(params);
    data["params_with_defaults"] = json!(params_with_defaults);
    data["attrs"] = json!(attrs);
    data["roles"] = json!(roles);
    data["span"] = json!({"start": iface.span.start, "end": iface.span.end});
    data
}

pub(crate) fn dump_enum_json(name: &str, en: &crate::McEnumDef, uri: &str) -> Value {
    let values: Vec<Value> = en
        .values
        .iter()
        .map(|v| json!({"name": v.name.to_string(), "span": [v.span[0], v.span[1]]}))
        .collect();
    json!({
        "name": name,
        "kind": "enum",
        "uri": uri,
        "span": [en.span[0], en.span[1]],
        "value_count": values.len(),
        "values": values,
    })
}

// Helper: serialize instances (mirrors instances_json in show.rs)
pub(crate) fn instances_json(insts: &crate::McInstances, type_filter: Option<&str>) -> Vec<Value> {
    let port_spans = insts.port_spans();
    insts
        .iter()
        .filter_map(|(n, inst)| {
            let (kind, class) = inst_kind_class(inst);
            let kind = if kind == "label" {
                match insts.get_label_kind(n) {
                    crate::LabelKind::Inline => "ilabel",
                    crate::LabelKind::Explicit => "label",
                }
            } else {
                kind
            };
            if let Some(t) = type_filter {
                if !kind.eq_ignore_ascii_case(t) {
                    return None;
                }
            }
            let span = port_spans
                .get(n)
                .and_then(|v| v.first())
                .map(|r| json!({"start": r.start, "end": r.end}));
            // Module port direction (`io`/`out`/`in`), empty for non-port
            // instances (components, modules, inline net labels).
            let io = match insts.insts().get(n) {
                Some((crate::IOType::InOut, _)) => "io",
                Some((crate::IOType::Out, _)) => "out",
                Some((crate::IOType::In, _)) => "in",
                _ => "",
            };
            let mut entry = json!({"name": n.to_string(), "io": io, "kind": kind, "class": class});
            if let Some(s) = span {
                entry["span"] = s;
            }
            Some(entry)
        })
        .collect()
}

// ============================================================================
// Semantic data (sem tokens + symbols) for LSP
// ============================================================================

/// Load the project for a file that is not yet in the active workspace.
///
/// Non-project mode: the workspace root is the configured project root (the
/// folder opened in the editor). Only the opened file plus its `use` closure
/// is loaded; sibling files are intentionally NOT added, so each file is
/// parsed in its own semantic scope without bare-name pollution from
/// unrelated definitions.
pub(crate) fn auto_load_from_file_path(file_path: &Path) {
    let project_root = find_project_root(file_path);
    info!(target: "crate::rpc", "auto_load: project_root={}", project_root.display());

    // Reuse the active workspace when its root already matches; only create a
    // new workspace when the root differs. This avoids snapshot/clear churn
    // (workspace hopping) when files are opened one after another.
    if workspace::WORKSPACE.active_root() != project_root {
        let root_name = project_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "project".to_string());
        info!(target: "crate::rpc", "auto_load: creating workspace id={} root={}", root_name, project_root.display());
        crate::workspace_create(&root_name, crate::WorkspaceKind::Project, &project_root);
    } else {
        info!(target: "crate::rpc", "auto_load: reusing active workspace root={}", project_root.display());
    }

    // Load library dependencies from project.toml before parsing
    let file_uri = McURI::from(file_path.to_string_lossy().to_string());
    ensure_library_loaded(&file_uri);

    // Force-load mcode: mcb_init_system_lib may have registered an empty placeholder.
    // Reload from the real mcode directory to ensure library components are
    // available. Respect libs.disable_mcode so the switch applies here too.
    if crate::cli::config::should_load_mcode(Some(&project_root)) {
        if let Ok(mcode_root) = resolve_lib_root("mcode") {
            crate::db::infra::libmgr::mcb_load_lib("mcode", &mcode_root);
        }
    }

    // Load only the entry file itself (plus its use closure via mcc_load_project).
    // The entry is the opened file, not the first file of a directory scan: the
    // former sibling loop is removed, so a directory scan would be wrong here.
    let uri = McURI::from(file_path.to_string_lossy().to_string());
    info!(target: "crate::rpc", "auto_load: mcc_load_project({})", uri);
    crate::mcc_load_project(&uri);
}

/// Walk up from a file path to find the project root
/// A project root is a directory containing a project manifest
/// (project.toml / manifest.toml / mcc.toml) or .mc files at top level
pub(crate) fn find_project_root(file_path: &Path) -> PathBuf {
    // Priority 1: the configured project root (the folder opened in the editor,
    // set via mcext set_project_root). In non-project mode every .mc file under
    // the opened folder is a peer, so the workspace root is always the folder
    // itself. No upward search for a nested manifest: sub-projects are
    // handled as plain files (see design doc mcext-folder-parse-design.md §2.6).
    let configured = crate::db::infra::init::mcb_get_project_root();
    if configured.is_absolute() && !configured.as_os_str().is_empty() {
        return configured;
    }

    let mut current = if file_path.is_dir() {
        file_path.to_path_buf()
    } else {
        file_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    };

    // First pass: walk up looking for a project manifest (3 names, same
    // priority as CLI project-root discovery).
    let mut probe = current.clone();
    let mut toml_dir: Option<PathBuf> = None;
    loop {
        if crate::cli::datadir::find_manifest_in(&probe).is_some() {
            toml_dir = Some(probe.clone());
            break;
        }
        if let Some(parent) = probe.parent() {
            probe = parent.to_path_buf();
        } else {
            break;
        }
    }
    // If a manifest is found, use that directory
    if let Some(dir) = toml_dir {
        return dir;
    }
    // Fallback: first directory with .mc files
    loop {
        if let Ok(entries) = std::fs::read_dir(&current) {
            for entry in entries.flatten() {
                if entry.path().extension().is_some_and(|ext| ext == "mc") {
                    return current;
                }
            }
        }
        if let Some(parent) = current.parent() {
            current = parent.to_path_buf();
        } else {
            break;
        }
    }
    // Fallback: use the file's parent directory
    file_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Ensure library dependencies are loaded for a file.
/// This is called when parsing files with content from LSP to ensure
/// the library context is available for type lookups.
pub(crate) fn ensure_library_loaded(file_uri: &McURI) {
    let libs = crate::db::infra::libmgr::mcb_loaded_libs();
    tracing::debug!(
        target: "mcc::lib",
        uri = %file_uri,
        loaded = ?libs,
        "ensure_library_loaded: start"
    );

    if !libs.is_empty() {
        return;
    }

    let path = Path::new(file_uri.as_str());
    let project_root = find_project_root(path);

    // Try to load project manifest dependencies (3 names, same as CLI)
    let project_manifest = crate::cli::datadir::find_manifest_in(&project_root);
    if let Some(manifest_path) = project_manifest {
        if let Ok(contents) = std::fs::read_to_string(&manifest_path) {
            if let Some(deps) = extract_lib_dependencies(&contents) {
                tracing::debug!(target: "mcc::lib", deps = ?deps, "loading dependencies");
                for lib_name in deps {
                    match resolve_lib_root(&lib_name) {
                        Ok(root) => {
                            tracing::info!(target: "mcc::lib", name = %lib_name, root = %root.display(), "auto-loading lib");
                            crate::db::infra::libmgr::mcb_load_lib(&lib_name, &root);
                        }
                        Err(e) => {
                            tracing::warn!(target: "mcc::lib", name = %lib_name, error = ?e, "resolve_lib_root failed");
                        }
                    }
                }
            }
        }
    }
}

/// Extract library dependencies from project.toml contents
pub(crate) fn extract_lib_dependencies(contents: &str) -> Option<Vec<String>> {
    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with("dependencies") || line.starts_with("lib_deps") {
            // Parse the dependencies section
            let mut deps = Vec::new();
            let mut in_deps = false;
            for dep_line in contents.lines() {
                let dep_line = dep_line.trim();
                if dep_line.starts_with("dependencies") || dep_line.starts_with("lib_deps") {
                    in_deps = true;
                    continue;
                }
                if in_deps {
                    if dep_line.is_empty() || dep_line.starts_with('#') {
                        continue;
                    }
                    if dep_line.starts_with('[') || dep_line.starts_with("lib_") {
                        break;
                    }
                    // Extract lib name (format: "name" = "version" or just "name")
                    let name = if let Some(eq_pos) = dep_line.find('=') {
                        let left = dep_line[..eq_pos].trim();
                        left.trim_matches('"').trim_matches('\'').to_string()
                    } else {
                        dep_line
                            .trim_matches(',')
                            .trim_matches('"')
                            .trim_matches('\'')
                            .to_string()
                    };
                    if !name.is_empty() {
                        deps.push(name);
                    }
                }
            }
            return Some(deps);
        }
    }
    None
}

/// Classify a token using the symbol table.
/// Overrides lexer type for identifiers that have semantic classification.

/// Try to find semantic data for any of the candidate URIs

// ============================================================================
// Report (M5b)
// ============================================================================

// ============================================================================
// Convert (M5b)
// ============================================================================

// ============================================================================
// Def (M6)
// ============================================================================

/// Handle def RPC — go-to-definition for a symbol.

// ============================================================================
// Capabilities (M6)
// ============================================================================

/// Handle capabilities RPC — self-describing API for AI discovery.

// ============================================================================
// Unified Lookup (F12/pass1-pass2)
// ============================================================================

/// Lookup a sub-element (pin, port, param, label) within a parent container.

/// Combined lookup: find class + optionally look up sub-element.
/// Supports compound identifiers like `uC.PA1` — finds `uC` then `PA1` within it.

/// Enumerate all visible symbols at a given scope.

// ============================================================================
// Explain (M6)
// ============================================================================

/// Handle explain RPC — look up error code descriptions.

/// Handle diagnostics RPC - return parse/semantic diagnostics for a file

/// Handle project_symbols RPC - return project-wide symbols (components, interfaces, enums, modules, enum_values)

/// Handle set_project_root RPC - set project root path

/// Handle set_system_root RPC - set system root path (for library resolution)

/// Handle init RPC - initialize mcc system

/// Handle load_project RPC - load entire project

/// Handle add_file RPC - add a single file to project

/// Handle remove_file RPC - remove a file from project
// ── Sub-module declarations ──
mod admin;
mod aicontract;
mod buildcmd;
mod defs;
mod exportcmd;
mod libcmd;
mod lsp;
mod show;

pub use admin::*;
pub use aicontract::*;
pub use buildcmd::*;
pub use defs::*;
pub use exportcmd::*;
pub use libcmd::*;
pub use lsp::*;
pub use show::*;

// ── Phase 8.3: Method registry (single source of truth for caps) ──

/// Metadata for one RPC method.
pub struct MethodMeta {
    pub name: &'static str,
    pub consumer: &'static str, // "lsp" | "ai" | "cli" | "admin"
}

/// Registry of all RPC methods. Single source of truth for caps + register_all.
pub static METHODS: &[MethodMeta] = &[
    MethodMeta {
        name: "server.info",
        consumer: "admin",
    },
    MethodMeta {
        name: "server.methods",
        consumer: "admin",
    },
    MethodMeta {
        name: "lib.list",
        consumer: "admin",
    },
    MethodMeta {
        name: "lib.info",
        consumer: "admin",
    },
    MethodMeta {
        name: "lib.load",
        consumer: "admin",
    },
    MethodMeta {
        name: "lib.unload",
        consumer: "admin",
    },
    MethodMeta {
        name: "lib.install",
        consumer: "admin",
    },
    MethodMeta {
        name: "lib.uninstall",
        consumer: "admin",
    },
    MethodMeta {
        name: "lib.search",
        consumer: "admin",
    },
    MethodMeta {
        name: "trace.set",
        consumer: "admin",
    },
    MethodMeta {
        name: "trace.get",
        consumer: "admin",
    },
    MethodMeta {
        name: "build.full",
        consumer: "cli",
    },
    MethodMeta {
        name: "build.viz",
        consumer: "cli",
    },
    MethodMeta {
        name: "parse",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.component",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.module",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.interface",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.net",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.all",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.file",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.files",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.enum",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.pins",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.ports",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.labels",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.instances",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.nets",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.attrs",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.funcs",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.params",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.roles",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.values",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.dump",
        consumer: "cli",
    },
    MethodMeta {
        name: "check",
        consumer: "ai",
    },
    MethodMeta {
        name: "extract",
        consumer: "cli",
    },
    MethodMeta {
        name: "defs.search",
        consumer: "cli",
    },
    MethodMeta {
        name: "defs.query",
        consumer: "cli",
    },
    MethodMeta {
        name: "export",
        consumer: "cli",
    },
    MethodMeta {
        name: "sem",
        consumer: "lsp",
    },
    MethodMeta {
        name: "explain",
        consumer: "ai",
    },
    MethodMeta {
        name: "def",
        consumer: "cli",
    },
    MethodMeta {
        name: "erc",
        consumer: "cli",
    },
    MethodMeta {
        name: "refs",
        consumer: "cli",
    },
    MethodMeta {
        name: "lookup",
        consumer: "cli",
    },
    MethodMeta {
        name: "lookup_sub",
        consumer: "cli",
    },
    MethodMeta {
        name: "lookup_with_sub",
        consumer: "cli",
    },
    MethodMeta {
        name: "lookup_all",
        consumer: "cli",
    },
    MethodMeta {
        name: "convert",
        consumer: "cli",
    },
    MethodMeta {
        name: "report",
        consumer: "cli",
    },
    MethodMeta {
        name: "caps",
        consumer: "ai",
    },
    MethodMeta {
        name: "diagnostics",
        consumer: "lsp",
    },
    MethodMeta {
        name: "project_symbols",
        consumer: "lsp",
    },
    MethodMeta {
        name: "set_project_root",
        consumer: "admin",
    },
    MethodMeta {
        name: "set_system_root",
        consumer: "admin",
    },
    MethodMeta {
        name: "init",
        consumer: "lsp",
    },
    MethodMeta {
        name: "load_project",
        consumer: "admin",
    },
    MethodMeta {
        name: "add_file",
        consumer: "lsp",
    },
    MethodMeta {
        name: "remove_file",
        consumer: "lsp",
    },
    MethodMeta {
        name: "show.component.list",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.module.list",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.interface.list",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.net.list",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.enum.list",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.ports.list",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.dump.all",
        consumer: "cli",
    },
    MethodMeta {
        name: "completion",
        consumer: "lsp",
    },
    MethodMeta {
        name: "hover",
        consumer: "lsp",
    },
];

/// Generate caps JSON from the method registry.
pub fn caps_json() -> serde_json::Value {
    use serde_json::json;

    let names: Vec<&str> = METHODS.iter().map(|m| m.name).collect();
    let ai_methods: Vec<&str> = METHODS
        .iter()
        .filter(|m| m.consumer == "ai")
        .map(|m| m.name)
        .collect();

    json!({
        "server": "mcc",
        "version": env!("CARGO_PKG_VERSION"),
        "schema_version": 1,
        "methods": names,
        "features": {
            "diagnostics": {
                "byte_range": false,
                "end_line": true,
                "end_column": true,
                "suggestions": true,
                "related": true
            },
            "explain": true,
            "search": true,
            "query": true,
            "export": ["netlist", "bom", "spice", "kicad"],
            "ai": {
                "methods": ai_methods,
                "overlay_dry_run": true,
            },
            "trace": {
                "targets": crate::cli::config::get_known_debug_targets(),
                "aliases": crate::cli::config::get_debug_aliases().into_iter().map(|(name, targets)| {
                    json!({"name": name, "targets": targets})
                }).collect::<Vec<_>>()
            }
        }
    })
}

/// Register all handlers on a server builder (single source of truth).
/// Called from `cmds/server.rs`.
pub fn register_all(
    mut builder: crate::rpc::server::RpcServerBuilder,
) -> crate::rpc::server::RpcServerBuilder {
    // Admin
    builder = builder.register_method("server.info", handle_server_info);
    builder = builder.register_method("server.methods", handle_methods);
    // Lib
    builder = builder.register_method("lib.list", handle_library_list);
    builder = builder.register_method("lib.info", handle_library_show);
    builder = builder.register_method("lib.load", handle_lib_load);
    builder = builder.register_method("lib.unload", handle_lib_unload);
    builder = builder.register_method("lib.install", handle_lib_install);
    builder = builder.register_method("lib.uninstall", handle_lib_uninstall);
    builder = builder.register_method("lib.search", handle_lib_search);
    builder = builder.register_method("trace.set", handle_trace_set);
    builder = builder.register_method("trace.get", handle_trace_get);
    // Build
    builder = builder.register_method("build.full", handle_build_full);
    builder = builder.register_method("build.viz", handle_build_viz);
    builder = builder.register_method("parse", handle_parse);
    // Show — lists
    builder = builder.register_method("show.component", handle_show_component);
    builder = builder.register_method("show.component.list", handle_show_component_list);
    builder = builder.register_method("show.module", handle_show_module);
    builder = builder.register_method("show.module.list", handle_show_module_list);
    builder = builder.register_method("show.interface", handle_show_interface);
    builder = builder.register_method("show.interface.list", handle_show_interface_list);
    builder = builder.register_method("show.net", handle_show_net);
    builder = builder.register_method("show.net.list", handle_show_net_list);
    builder = builder.register_method("show.all", handle_show_all);
    builder = builder.register_method("show.file", handle_show_file);
    builder = builder.register_method("show.files", handle_show_files);
    builder = builder.register_method("show.enum", handle_show_enum);
    builder = builder.register_method("show.enum.list", handle_show_enum_list);
    // Show — drill-down
    builder = builder.register_method("show.pins", handle_show_pins);
    builder = builder.register_method("show.ports", handle_show_ports);
    builder = builder.register_method("show.ports.list", handle_show_ports_list);
    builder = builder.register_method("show.labels", handle_show_labels);
    builder = builder.register_method("show.instances", handle_show_instances);
    builder = builder.register_method("show.nets", handle_show_nets);
    builder = builder.register_method("show.attrs", handle_show_attrs);
    builder = builder.register_method("show.funcs", handle_show_funcs);
    builder = builder.register_method("show.params", handle_show_params);
    builder = builder.register_method("show.roles", handle_show_roles);
    builder = builder.register_method("show.values", handle_show_values);
    builder = builder.register_method("show.dump", handle_show_dump);
    builder = builder.register_method("show.dump.all", handle_show_dump_all);
    // AI
    builder = builder.register_method("check", handle_check);
    builder = builder.register_method("extract", handle_extract);
    // Defs
    builder = builder.register_method("defs.search", handle_defs_search);
    builder = builder.register_method("defs.query", handle_defs_query);
    builder = builder.register_method("export", handle_export);
    // LSP
    builder = builder.register_method("sem", handle_sem);
    builder = builder.register_method("explain", handle_explain);
    builder = builder.register_method("def", handle_def);
    builder = builder.register_method("erc", handle_erc);
    builder = builder.register_method("refs", handle_refs);
    builder = builder.register_method("lookup", handle_lookup);
    builder = builder.register_method("lookup_sub", handle_lookup_sub);
    builder = builder.register_method("lookup_with_sub", handle_lookup_with_sub);
    builder = builder.register_method("lookup_all", handle_lookup_all);
    builder = builder.register_method("convert", handle_convert);
    builder = builder.register_method("report", handle_report);
    builder = builder.register_method("caps", handle_caps);
    builder = builder.register_method("diagnostics", handle_diagnostics);
    builder = builder.register_method("project_symbols", handle_project_symbols);
    builder = builder.register_method("set_project_root", handle_set_project_root);
    builder = builder.register_method("set_system_root", handle_set_system_root);
    builder = builder.register_method("init", handle_init);
    builder = builder.register_method("load_project", handle_load_project);
    builder = builder.register_method("add_file", handle_add_file);
    builder = builder.register_method("remove_file", handle_remove_file);
    builder = builder.register_method("completion", handle_completion);
    builder = builder.register_method("hover", handle_hover);
    builder
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn find_project_root_prefers_configured_root() {
        // Save and restore the global project root so this test does not leak
        // state into other tests running in the same process.
        let saved = crate::db::infra::init::mcb_get_project_root();

        let tmp = std::env::temp_dir().join(format!("mcc-root-test-{}", std::process::id()));
        let sub = tmp.join("a").join("b");
        fs::create_dir_all(&sub).unwrap();
        let file = sub.join("x.mc");
        fs::write(&file, "").unwrap();

        // Configured root wins regardless of the file location: the opened
        // folder is the single workspace root in non-project mode.
        crate::db::infra::init::mcb_set_project_root(&tmp);
        assert_eq!(find_project_root(&file), tmp);

        // Empty configured root falls back to the original walk-up logic:
        // the first directory containing .mc files (here: the file's parent).
        crate::db::infra::init::mcb_set_project_root(std::path::Path::new(""));
        assert_eq!(find_project_root(&file), sub);

        fs::remove_dir_all(&tmp).unwrap();
        crate::db::infra::init::mcb_set_project_root(&saved);
    }

    #[test]
    fn find_project_root_detects_any_manifest_name() {
        let saved = crate::db::infra::init::mcb_get_project_root();
        crate::db::infra::init::mcb_set_project_root(std::path::Path::new(""));

        let tmp = std::env::temp_dir().join(format!("mcc-root-mf-{}", std::process::id()));
        let sub = tmp.join("src");
        fs::create_dir_all(&sub).unwrap();
        let file = sub.join("x.mc");
        fs::write(&file, "").unwrap();

        // manifest.toml project
        fs::write(tmp.join("manifest.toml"), "[project]\nname = \"m\"\n").unwrap();
        assert_eq!(find_project_root(&file), tmp);

        // mcc.toml project (no manifest.toml / project.toml present)
        fs::remove_file(tmp.join("manifest.toml")).unwrap();
        fs::write(tmp.join("mcc.toml"), "[project]\nname = \"m\"\n").unwrap();
        assert_eq!(find_project_root(&file), tmp);

        // project.toml project (the original single-name behavior)
        fs::remove_file(tmp.join("mcc.toml")).unwrap();
        fs::write(tmp.join("project.toml"), "[project]\nname = \"m\"\n").unwrap();
        assert_eq!(find_project_root(&file), tmp);

        fs::remove_dir_all(&tmp).unwrap();
        crate::db::infra::init::mcb_set_project_root(&saved);
    }

    /// The `pins` view orders pin IDs naturally (see `pin_id_cmp`): numeric
    /// IDs first in numeric order, then non-numeric IDs with embedded digit
    /// runs compared numerically.
    #[test]
    fn pin_id_cmp_orders_numeric_then_natural() {
        let mut ids = vec![
            "10", "2", "1", "12", "11", "3", "4", "9", "5", "6", "7", "8",
        ];
        ids.sort_by(|a, b| pin_id_cmp(a, b));
        assert_eq!(
            ids,
            vec!["1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12"]
        );

        // Non-numeric IDs follow: letter runs lexically, digit runs numerically.
        let mut mixed = vec!["PA10", "A2", "B1", "PA0", "A10", "AB", "A1"];
        mixed.sort_by(|a, b| pin_id_cmp(a, b));
        assert_eq!(mixed, vec!["A1", "A2", "A10", "AB", "B1", "PA0", "PA10"]);

        // Numeric pins come before letter pins.
        let mut mixed2 = vec!["B", "2", "A", "1"];
        mixed2.sort_by(|a, b| pin_id_cmp(a, b));
        assert_eq!(mixed2, vec!["1", "2", "A", "B"]);
    }
}
