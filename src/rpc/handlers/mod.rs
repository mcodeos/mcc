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

use super::protocol::{JsonRpcError, RpcResult};
use crate::builder::mc_code::McCode;
use crate::builder::workspace;
use crate::search_api::{walk_defs, SearchInputs, SearchKind};
use crate::McURI;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::info;

// LSP semantic token/symbol assembly (extracted to lsp/sem.rs)
pub(crate) use params::*;
pub(crate) mod params;
pub use crate::lsp::sem::{classify_token_by_symbol, try_lookup_sem};

// C bindings for controlling log output
extern "C" {
    fn mcc_reset(log_flags: libc::c_uchar);
}

pub(crate) const MCC_SYSTEM_ENV: &str = "MCC_SYSTEM_ROOT";

pub(crate) fn mcc_system_root() -> PathBuf {
    // Single source of truth: delegate to data_dir::data_root() (which honors
    // $MCC_SYSTEM_ROOT). The cwd/mc/ probe and the `~/.mcode` fallback live
    // there now.
    crate::cli::data_dir::data_root()
}

pub(crate) fn projects_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mcc-projects")
}
pub(crate) fn project_dir(id: &str) -> PathBuf {
    projects_dir().join(id)
}
pub(crate) fn project_src_dir(id: &str) -> PathBuf {
    project_dir(id).join("src")
}
pub(crate) fn project_src_dir_from_root(root: &Path, _id: &str) -> PathBuf {
    root.join("src")
}
pub(crate) fn project_manifest(id: &str) -> PathBuf {
    project_dir(id).join("manifest.toml")
}
pub(crate) fn project_manifest_from_root(root: &Path, _id: &str) -> PathBuf {
    root.join("manifest.toml")
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
    info!(target: "mcc::pass1", "----------------------------------------");
    info!(target: "mcc::pass1", "[Pass 1] Loading project from: {}", uri);
    info!(target: "mcc::pass1", "----------------------------------------");

    crate::mcc_load_project(&mc_uri);
    let pass1 = collect_pass1(&mc_uri, include_system);

    let module_count = crate::mcb_module_count();
    let component_count = crate::mcb_component_count();
    let interface_count = crate::mcb_interface_count();

    // Output definition statistics to server log
    info!(target: "mcc::pass1", "Total definitions loaded:");
    info!(target: "mcc::pass1", "  - Modules: {}", module_count);
    info!(target: "mcc::pass1", "  - Components: {}", component_count);
    info!(target: "mcc::pass1", "  - Interfaces: {}", interface_count);

    // Output each module details to server log
    for (name, module_uri) in crate::mcb_iter_modules() {
        let ident = crate::McIds::from(name.as_str());
        let module_mc_uri = McURI::from(module_uri.as_str());
        if let Some(cmie) = crate::get_def(&ident, &module_mc_uri) {
            if let crate::McCMIE::Module(module_def) = cmie {
                info!(target: "mcc::pass1", ">> Found module definition: {}", name);
                info!(target: "mcc::pass1", "------------------------------------------------------------------");
                info!(target: "mcc::pass1", "| Ports ");
                info!(target: "mcc::pass1", "|-----------------------------------------------------------------");
                info!(target: "mcc::pass1", "|   inputs:  {:?}",
                    module_def.insts.inputs_with_name().iter()
                        .map(|(n, _)| *n).collect::<Vec<_>>()
                );
                info!(target: "mcc::pass1", "|   outputs: {:?}",
                    module_def.insts.outputs_with_name().iter()
                        .map(|(n, _)| *n).collect::<Vec<_>>()
                );
                info!(target: "mcc::pass1", "|   bidirs:  {:?}",
                    module_def.insts.bidirs_with_name().iter()
                        .map(|(n, _)| *n).collect::<Vec<_>>()
                );
                info!(target: "mcc::pass1", "|   powers:  {:?}",
                    module_def.insts.powers_with_name().iter()
                        .map(|(n, _)| *n).collect::<Vec<_>>()
                );
                info!(target: "mcc::pass1", "|");
                info!(target: "mcc::pass1", "| Symbols ({} entries)", module_def.insts.iter().count());
                info!(target: "mcc::pass1", "|-----------------------------------------------------------------");
                for (key, ident) in module_def.insts.iter() {
                    let type_name = ident.type_name();
                    info!(target: "mcc::pass1", "|  {:<15} {}", type_name, key);
                }
                info!(target: "mcc::pass1", "|");
                info!(target: "mcc::pass1", "| Lines ({} connections)", module_def.lines.len());
                info!(target: "mcc::pass1", "|-----------------------------------------------------------------");
                if module_def.lines.is_empty() {
                    info!(target: "mcc::pass1", "|   (no connections)");
                } else {
                    for (i, _line) in module_def.lines.iter().enumerate() {
                        info!(target: "mcc::pass1", "|");
                        info!(target: "mcc::pass1", "|   +--- Series[{}] ----------", i);
                    }
                    info!(target: "mcc::pass1", "|   +--------------------------------------------------");
                }
                info!(target: "mcc::pass1", "------------------------------------------------------------------");
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

    // Decide top module
    let top_name = match top {
        Some(t) => t.to_string(),
        None => crate::mcb_get_module_name_by_uri(&mc_uri)
            .or_else(crate::mcb_get_first_module_name)
            .ok_or_else(|| JsonRpcError::custom(32107, "no top module found"))?,
    };

    // Pass2
    let ident = crate::McIds::from(top_name.as_str());
    if crate::get_def(&ident, &mc_uri).is_none() {
        return Err(JsonRpcError::custom(
            -32107,
            &format!("top module '{top_name}' not defined"),
        ));
    }
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::mcc_build(&ident, &mc_uri)
    }));
    let pass2 = match built {
        Ok(Ok(inst)) => {
            info!(target: "mcc::pass2", "----------------------------------------");
            info!(target: "mcc::pass2", "[Pass 2] Instantiating top module: {}", top_name);
            info!(target: "mcc::pass2", "----------------------------------------");
            info!(target: "mcc::pass2", ">> Instance: {} (class {})",
                inst.name.to_string(), inst.def.name.to_string());
            info!(target: "mcc::pass2", "|   ports:       {}", inst.ports.len());
            info!(target: "mcc::pass2", "|   components:  {}", inst.components.len());
            info!(target: "mcc::pass2", "|   sub_modules: {}", inst.sub_modules.len());
            info!(target: "mcc::pass2", "|   connections: {}", inst.connections.len());
            for sub in inst.sub_modules.iter() {
                info!(target: "mcc::pass2", "|     - {} (class {})",
                    sub.name.to_string(), sub.def.name.to_string());
            }
            collect_pass2(&top_name, &inst)
        }
        Ok(Err(e)) => {
            return Err(JsonRpcError::custom(
                -32107,
                &format!("instantiation failed: {e}"),
            ));
        }
        Err(_) => {
            return Err(JsonRpcError::custom(
                -32108,
                "build.full: Pass2 build panicked (engine bug); request aborted, server kept alive",
            ));
        }
    };

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

// ============================================================================
// Internal: Memory load version (no disk file dependency)
// ============================================================================

/// Parse entry filename from memory store
pub(crate) fn resolve_virtual_entry(
    store: &BTreeMap<String, String>,
    entry: Option<&str>,
) -> Result<String, JsonRpcError> {
    if let Some(rel) = entry {
        if store.contains_key(rel) {
            return Ok(rel.to_string());
        }
        return Err(JsonRpcError::custom(
            -32105,
            &format!("entry not found: {rel}"),
        ));
    }
    // Auto select: main.mc > first .mc file
    for cand in &["main.mc"] {
        if store.contains_key(*cand) {
            return Ok(cand.to_string());
        }
    }
    store
        .keys()
        .find(|k| k.ends_with(".mc"))
        .cloned()
        .ok_or_else(|| JsonRpcError::custom(32105, "no .mc entry found"))
}

/// Load all files from memory and execute Pass1
pub(crate) fn run_pass1_from_memory(
    vdir: &str,
    entry_name: &str,
    store: &BTreeMap<String, String>,
    command: &str,
    ws_kind: &str,
    ws_name: &str,
    include_system: bool,
) -> RpcResult {
    let entry_uri = format!("{vdir}/{entry_name}");
    info!(target: "mcc::pass1", "----------------------------------------");
    info!(target: "mcc::pass1", "[Pass 1] Loading from memory: {}", entry_uri);
    info!(target: "mcc::pass1", "----------------------------------------");

    // Load all files to builder
    for (fname, content) in store.iter() {
        let file_uri = format!("{vdir}/{fname}");
        crate::mcc_load_from_string(&file_uri, content);
    }

    let _mc_uri = McURI::from(entry_uri.as_str());
    let pass1 = collect_pass1(&entry_uri, include_system);

    let module_count = crate::mcb_module_count();
    let component_count = crate::mcb_component_count();
    let interface_count = crate::mcb_interface_count();

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

/// Execute Pass1 + Pass2 from memory
pub(crate) fn run_full_build_from_memory(
    vdir: &str,
    entry_name: &str,
    store: &BTreeMap<String, String>,
    top: Option<&str>,
    command: &str,
    ws_kind: &str,
    ws_name: &str,
    include_system: bool,
    include_ast: bool,
) -> RpcResult {
    // Set AST visit output flag
    // MCC_LOG_VISIT = 1 << 3 = 8
    // MCC_LOG_ALL = 0xFF
    if include_ast {
        unsafe {
            mcc_reset(0xFF);
        } // Enable all logs
    } else {
        unsafe {
            mcc_reset(0);
        } // Disable all logs
    }

    let entry_uri = format!("{vdir}/{entry_name}");
    info!(target: "mcc::pass1", "----------------------------------------");
    info!(target: "mcc::pass1", "[Pass 1] Loading from memory: {}", entry_uri);
    info!(target: "mcc::pass1", "----------------------------------------");

    // Load all files to builder
    for (fname, content) in store.iter() {
        let file_uri = format!("{vdir}/{fname}");
        crate::mcc_load_from_string(&file_uri, content);
    }

    let mc_uri = McURI::from(entry_uri.as_str());
    let pass1 = collect_pass1(&entry_uri, include_system);

    // Decide top module
    let top_name = match top {
        Some(t) => t.to_string(),
        None => crate::mcb_get_module_name_by_uri(&mc_uri)
            .or_else(crate::mcb_get_first_module_name)
            .ok_or_else(|| JsonRpcError::custom(32107, "no top module found"))?,
    };

    // Pass2
    let ident = crate::McIds::from(top_name.as_str());
    if crate::get_def(&ident, &mc_uri).is_none() {
        return Err(JsonRpcError::custom(
            -32107,
            &format!("top module '{top_name}' not defined"),
        ));
    }
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::mcc_build(&ident, &mc_uri)
    }));
    let pass2 = match built {
        Ok(Ok(inst)) => {
            info!(target: "mcc::pass2", "----------------------------------------");
            info!(target: "mcc::pass2", "[Pass 2] Instantiating top module: {}", top_name);
            info!(target: "mcc::pass2", "----------------------------------------");
            info!(target: "mcc::pass2", ">> Instance: {} (class {})",
                inst.name.to_string(), inst.def.name.to_string());
            info!(target: "mcc::pass2", "|   ports:       {}", inst.ports.len());
            info!(target: "mcc::pass2", "|   components:  {}", inst.components.len());
            info!(target: "mcc::pass2", "|   sub_modules: {}", inst.sub_modules.len());
            info!(target: "mcc::pass2", "|   connections: {}", inst.connections.len());
            for sub in inst.sub_modules.iter() {
                info!(target: "mcc::pass2", "|     - {} (class {})",
                    sub.name.to_string(), sub.def.name.to_string());
            }
            collect_pass2(&top_name, &inst)
        }
        Ok(Err(e)) => {
            return Err(JsonRpcError::custom(
                -32107,
                &format!("instantiation failed: {e}"),
            ));
        }
        Err(_) => {
            return Err(JsonRpcError::custom(
                -32108,
                "build: Pass2 build panicked (engine bug); request aborted, server kept alive",
            ));
        }
    };

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
    walk_connections(inst, &mut out);
    out
}

pub(crate) fn walk_connections(inst: &crate::MccProjectTree, out: &mut Vec<Value>) {
    for conn in &inst.connections {
        out.push(json!({
            "id": conn.id,
            "net_name": conn.net_name,
            "points": conn.points.iter().map(|p| p.path.clone()).collect::<Vec<_>>(),
        }));
    }
    for sub in &inst.sub_modules {
        walk_connections(sub, out);
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
                    // Get pin name from component definition
                    let pin_name = c
                        .def
                        .pins
                        .pins
                        .get(pin_id)
                        .and_then(|p| p.names.first())
                        .cloned()
                        .unwrap_or_else(|| pin_id.clone());
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
    use std::collections::BTreeMap;
    let mut by_name: BTreeMap<String, Vec<String>> = BTreeMap::new();
    walk_nets(inst, &mut by_name);
    by_name
        .into_iter()
        .map(|(name, points)| json!({ "name": name, "points": points }))
        .collect()
}

pub(crate) fn walk_nets(inst: &crate::MccProjectTree, by_name: &mut BTreeMap<String, Vec<String>>) {
    for conn in &inst.connections {
        let key = conn.net_name.clone().unwrap_or_default();
        let entry = by_name.entry(key).or_default();
        for p in &conn.points {
            if !entry.contains(&p.path) {
                entry.push(p.path.clone());
            }
        }
    }
    for sub in &inst.sub_modules {
        walk_nets(sub, by_name);
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

pub(crate) fn collect_definitions(
    items: Vec<(String, String, [usize; 2])>,
) -> Vec<(String, String, [usize; 2])> {
    items
}

pub(crate) fn refs_json(items: &[(String, String, [usize; 2])]) -> Vec<Value> {
    items
        .iter()
        .map(|(n, u, s)| json!({"name": n, "uri": u, "span": s}))
        .collect()
}

pub(crate) fn load_libs_rpc(libs: &[String]) {
    if libs.is_empty() {
        return;
    }
    let system_root = crate::mcb_get_system_root();
    let loaded = crate::mcb_loaded_libs();
    for name in libs {
        if loaded.contains(name) {
            continue;
        }
        let root = system_root.join(name);
        if root.exists() {
            crate::mcb_load_lib(name, &root);
        }
    }
}

/// Overlay URI used when `content` is provided — virtual file, never touches disk.
/// Phase 8.1: uses per-request unique URIs to prevent concurrent AI clients from
/// stepping on each other's workspace data.
pub(crate) const CHECK_OVERLAY_URI: &str = "/mcc/check.mc";

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
    crate::builder::mcb_remove(uri);
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
        .ok_or_else(|| JsonRpcError::custom(-32003, "semantic: no modules found"))?;

    let uri = crate::McURI::from(top.as_str());
    let ident = crate::McIds::from(top.as_str());

    let inst = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::mcc_build(&ident, &uri)
    }))
    .map_err(|_| JsonRpcError::custom(-32002, "semantic: build panicked"))?
    .map_err(|e| JsonRpcError::custom(-32002, &format!("semantic: build failed: {e}")))?;

    let mut diags: Vec<Value> = Vec::new();

    // ── Single-point nets ──
    let single_point: Vec<&String> = inst
        .nets
        .iter()
        .filter(|(name, points)| {
            !name.starts_with("__net_") && points.len() <= 1 && name.as_str() != "NC"
        })
        .map(|(name, _)| name)
        .collect();

    for net in &single_point {
        diags.push(json!({
            "code": 5001,
            "severity": "warning",
            "message": format!("single-point net: '{net}' has only one connection — may be unconnected"),
            "check": "single_point_net",
        }));
    }

    // ── Unconnected ports ──
    let all_net_paths: std::collections::HashSet<&str> = inst
        .nets
        .values()
        .flat_map(|pts| pts.iter())
        .map(|p| p.path.as_str())
        .collect();

    for port in &inst.ports {
        if !all_net_paths.contains(port.name.as_str()) {
            diags.push(json!({
                "code": 5002,
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
        if name.starts_with("__net_") || name.as_str() == "NC" {
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
            diags.push(json!({
                "code": 5003,
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
            diags.push(json!({
                "code": 5004,
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
                        let net = conn.net_name.clone().unwrap_or_else(|| format!("__net_{}", conn.id));
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

pub(crate) fn write_files(root: &Path, files: &[UploadFile]) -> (Vec<String>, Vec<String>) {
    let mut uploaded = Vec::new();
    let mut skipped = Vec::new();
    for f in files {
        if !is_safe_relative(&f.path) {
            skipped.push(format!("{} (unsafe path)", f.path));
            continue;
        }
        let target = root.join(&f.path);
        if let Some(parent) = target.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                skipped.push(format!("{} (mkdir: {})", f.path, e));
                continue;
            }
        }
        match fs::write(&target, &f.content) {
            Ok(_) => uploaded.push(f.path.clone()),
            Err(e) => skipped.push(format!("{} ({})", f.path, e)),
        }
    }
    (uploaded, skipped)
}

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

pub(crate) fn extract_archive(
    format: &str,
    data_b64: &str,
    dest: &Path,
    strip: usize,
) -> Result<Vec<String>, JsonRpcError> {
    use base64::Engine;
    let data = base64::engine::general_purpose::STANDARD
        .decode(data_b64)
        .map_err(|e| JsonRpcError::custom(32103, &format!("base64 decode: {e}")))?;
    match format {
        "tar.gz" | "tgz" => extract_tar_gz(&data, dest, strip),
        "tar" => extract_tar(&data, dest, strip),
        other => Err(JsonRpcError::custom(
            -32104,
            &format!("unsupported archive format: {other}"),
        )),
    }
}

pub(crate) fn extract_tar_gz(
    data: &[u8],
    dest: &Path,
    strip: usize,
) -> Result<Vec<String>, JsonRpcError> {
    use flate2::read::GzDecoder;
    use tar::Archive;
    let gz = GzDecoder::new(data);
    let mut archive = Archive::new(gz);
    extract_tar_entries(&mut archive, dest, strip)
}

pub(crate) fn extract_tar(
    data: &[u8],
    dest: &Path,
    strip: usize,
) -> Result<Vec<String>, JsonRpcError> {
    use tar::Archive;
    let mut archive = Archive::new(data);
    extract_tar_entries(&mut archive, dest, strip)
}

pub(crate) fn extract_tar_entries<R: std::io::Read>(
    archive: &mut tar::Archive<R>,
    dest: &Path,
    strip: usize,
) -> Result<Vec<String>, JsonRpcError> {
    let mut extracted = Vec::new();
    let entries = archive
        .entries()
        .map_err(|e| JsonRpcError::custom(32103, &format!("tar entries: {e}")))?;
    for entry in entries {
        let mut entry =
            entry.map_err(|e| JsonRpcError::custom(32103, &format!("tar entry: {e}")))?;
        let entry_path = entry
            .path()
            .map_err(|e| JsonRpcError::custom(32103, &format!("tar path: {e}")))?
            .to_path_buf();
        let stripped: PathBuf = entry_path.components().skip(strip).collect();
        if stripped.as_os_str().is_empty() {
            continue;
        }
        if stripped.is_absolute()
            || stripped
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            continue;
        }
        let target = dest.join(&stripped);
        if entry.header().entry_type().is_dir() {
            let _ = fs::create_dir_all(&target);
            continue;
        }
        if let Some(parent) = target.parent() {
            let _ = fs::create_dir_all(parent);
        }
        entry
            .unpack(&target)
            .map_err(|e| JsonRpcError::custom(32103, &format!("unpack: {e}")))?;
        extracted.push(stripped.to_string_lossy().to_string());
    }
    Ok(extracted)
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

pub(crate) fn read_manifest_entry(name: &str) -> Option<String> {
    let content = fs::read_to_string(project_manifest(name)).ok()?;
    parse_manifest_field(&content, "entry")
}

pub(crate) fn read_project_entry_from_workspace() -> Option<String> {
    let (_, _, root_str) = crate::workspace_info();
    let project_toml = PathBuf::from(&root_str).join("project.toml");
    let content = fs::read_to_string(&project_toml).ok()?;
    parse_manifest_field(&content, "entry")
}

pub(crate) fn read_manifest_top(name: &str) -> Option<String> {
    let content = fs::read_to_string(project_manifest(name)).ok()?;
    parse_manifest_field(&content, "top_module")
}

pub(crate) fn read_project_top_from_workspace() -> Option<String> {
    let (_, _, root_str) = crate::workspace_info();
    let project_toml = PathBuf::from(&root_str).join("project.toml");
    let content = fs::read_to_string(&project_toml).ok()?;
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

pub(crate) fn activate_workspace(name: &str) -> Result<(), JsonRpcError> {
    let (active, _, _) = crate::workspace_info();
    if active == name {
        return Ok(());
    }
    if !crate::workspace_switch(name) {
        return Err(JsonRpcError::custom(32102, "workspace not found"));
    }
    Ok(())
}

pub(crate) fn resolve_lib_root(name: &str) -> Result<PathBuf, JsonRpcError> {
    if name == "mcode" {
        // Try default mcode_dir first
        let p = mcode_dir();
        if p.exists() {
            return Ok(p);
        }
        // Fallback: try sibling directory (mcc_system_root/../mcode)
        let sibling = mcc_system_root().join("..").join("mcode");
        if sibling.exists() {
            return Ok(sibling);
        }
        return Err(JsonRpcError::custom(32102, "mcode dir not found"));
    }
    let tp = mcc_system_root();
    if tp.exists() {
        let prefix = format!("{name}@");
        if let Ok(entries) = fs::read_dir(&tp) {
            for e in entries.flatten() {
                let fname = e.file_name().to_string_lossy().to_string();
                if fname.starts_with(&prefix) && e.path().is_dir() {
                    return Ok(e.path());
                }
            }
        }
        let bare = tp.join(name);
        if bare.exists() {
            return Ok(bare);
        }
    }
    Err(JsonRpcError::custom(
        -32102,
        &format!("library '{name}' not installed"),
    ))
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
    let iterators: [(&str, Vec<(String, String)>); 4] = [
        ("component", crate::mcb_iter_components()),
        ("module", crate::mcb_iter_modules()),
        ("interface", crate::mcb_iter_interfaces()),
        ("enum", crate::mcb_iter_enums()),
    ];
    for (_, items) in &iterators {
        if let Some((matched, uri)) = items.iter().find(|(n, _)| n == name) {
            let ident = crate::McIds::from(matched.as_str());
            let uri_obj = crate::McURI::from(uri.as_str());
            if let Some(cmie) = crate::get_def(&ident, &uri_obj) {
                return Some((cmie, uri.clone()));
            }
        }
    }
    None
}

/// Build a pin JSON object (mirrors pins_json in show.rs).
pub(crate) fn pins_json(pins: &crate::McPins) -> Value {
    let pin_list: Vec<Value> = pins
        .pins
        .iter()
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
            let mut j = json!({
                "id": pin_id,
                "iotype": format!("{:?}", pin.iotype),
                "names": pin.names,
            });
            if !desc.is_empty() {
                j["description"] = json!(desc);
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
        crate::McInstance::Component(c) => ("component", c.name.to_string()),
        crate::McInstance::Module(m) => ("module", m.name.to_string()),
        crate::McInstance::Label(l) => ("label", l.clone()),
        crate::McInstance::Interface(i) => ("interface", i.name.to_string()),
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
    }
}

pub(crate) fn attrval_json(v: &crate::McAttrVal) -> Value {
    match v {
        crate::McAttrVal::AttrLiteral(crate::McLiteral::String(s)) => json!(s.value),
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
pub(crate) fn param_declare_to_json(d: &mcc::semantic::basic::mc_paramd::McParamDeclare) -> Value {
    let name = d.get_primary_name().unwrap_or_default();
    let is_port = d.is_port();
    let has_default = d.has_default_value();
    let default_val = d.param_type.default_value().map(|s| s.to_string());
    let class_name = d.get_class_name();
    json!({
        "name": name,
        "type": d.param_type.category_name(),
        "is_port": is_port,
        "has_default": has_default,
        "default": default_val,
        "class": class_name,
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
            let body_lines: Vec<String> = f.lines.iter().map(|l| l.to_string()).collect();
            json!({
                "name": f.name.to_string(),
                "params": f.params.names(),
                "returns": f.returns.kind_str(),
                "called_time": f.called_time,
                "body_lines": body_lines,
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
    let params: Vec<Value> = module
        .params
        .names_full()
        .iter()
        .map(|n| json!(n))
        .collect();
    let params_with_defaults: Vec<Value> = module
        .params
        .get_params_with_defaults()
        .iter()
        .map(|(id, default)| json!({"name": id.to_string(), "default": default}))
        .collect();
    let instances: Vec<Value> = instances_json(&module.insts, None);
    let lines: Vec<String> = module.lines.iter().map(|l| l.to_string()).collect();
    let funcs: Vec<Value> = module
        .funcs
        .iter()
        .map(|f| {
            let body_lines: Vec<String> = f.lines.iter().map(|l| l.to_string()).collect();
            json!({
                "name": f.name.to_string(),
                "params": f.params.names(),
                "returns": f.returns.kind_str(),
                "called_time": f.called_time,
                "body_lines": body_lines,
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
        "lines_count": module.lines.len(),
        "lines": lines,
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
            let mut entry = json!({"name": n.to_string(), "kind": kind, "class": class});
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

/// Detect project root from a file path and load the project
pub(crate) fn auto_load_from_file_path(file_path: &Path) {
    // Walk up from the file to find the project root (directory containing project.toml or .mc files)
    let project_root = find_project_root(file_path);
    info!(target: "mcc::rpc", "auto_load: project_root={}", project_root.display());

    // Create project workspace
    let root_name = project_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".to_string());
    info!(target: "mcc::rpc", "auto_load: creating workspace id={} root={}", root_name, project_root.display());
    crate::workspace_create(&root_name, crate::WorkspaceKind::Project, &project_root);

    // 1. Load entry file with mcc_load_project (triggers parse_pass1_types -> create_lapper)
    let mut all_files = Vec::new();
    scan_mc_files_recursive(&project_root, &project_root, &mut all_files);
    info!(target: "mcc::rpc", "auto_load: found {} .mc files", all_files.len());

    if let Some(entry_path) = all_files.first() {
        let full = project_root.join(entry_path);
        let uri = McURI::from(full.to_string_lossy().to_string());
        info!(target: "mcc::rpc", "auto_load: mcc_load_project({})", uri);
        crate::mcc_load_project(&uri);
    }

    // 2. Add any remaining independent files that weren't loaded as dependencies
    // (call parse_pass1_types directly to trigger create_lapper)
    let loaded_uris: Vec<String> = workspace::WORKSPACE
        .mcodes
        .borrow()
        .iter()
        .map(|e| e.key().clone())
        .collect();
    for rel in &all_files {
        let full = project_root.join(rel);
        let uri_str = full.to_string_lossy().to_string();
        let is_loaded = loaded_uris.iter().any(|u| u == &uri_str);
        if !is_loaded {
            if let Some(mut mcfile) = McCode::new(&uri_str, false) {
                mcfile.parse_ast();
                mcfile.parse_nsp();
                mcfile.parse_pass1_types(); // triggers create_lapper
                workspace::WORKSPACE
                    .mcodes
                    .borrow()
                    .insert(uri_str.clone(), mcfile);
                info!(target: "mcc::rpc", "auto_load: added independent {}", uri_str);
            }
        }
    }
}

/// Walk up from a file path to find the project root
/// A project root is a directory containing project.toml or .mc files at top level
pub(crate) fn find_project_root(file_path: &Path) -> PathBuf {
    let mut current = if file_path.is_dir() {
        file_path.to_path_buf()
    } else {
        file_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    };

    loop {
        // Check for project.toml
        if current.join("project.toml").exists() {
            return current;
        }
        // Check for .mc files at this level
        if let Ok(entries) = std::fs::read_dir(&current) {
            for entry in entries.flatten() {
                if entry.path().extension().is_some_and(|ext| ext == "mc") {
                    return current;
                }
            }
        }
        // Go up one level
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
    // Check if libraries are already loaded
    let libs = crate::builder::mcb_loaded_libs();
    eprintln!(
        "[ensure_library_loaded] uri={} libs_already_loaded={:?}",
        file_uri, libs
    );

    if !libs.is_empty() {
        eprintln!("[ensure_library_loaded] skip, libs already loaded");
        return;
    }

    // Find project root from file
    let path = Path::new(file_uri.as_str());
    let project_root = find_project_root(path);

    eprintln!(
        "[ensure_library_loaded] project_root={}",
        project_root.display()
    );

    // Try to load project.toml dependencies
    let project_toml = project_root.join("project.toml");
    eprintln!(
        "[ensure_library_loaded] project_toml={} exists={}",
        project_toml.display(),
        project_toml.exists()
    );

    if project_toml.exists() {
        if let Ok(contents) = std::fs::read_to_string(&project_toml) {
            if let Some(deps) = extract_lib_dependencies(&contents) {
                eprintln!("[ensure_library_loaded] deps={:?}", deps);
                for lib_name in deps {
                    eprintln!("[ensure_library_loaded] loading lib={}", lib_name);
                    match resolve_lib_root(&lib_name) {
                        Ok(root) => {
                            eprintln!("[ensure_library_loaded] resolved root={}", root.display());
                            crate::builder::mcb_load_lib(&lib_name, &root);
                        }
                        Err(e) => {
                            eprintln!("[ensure_library_loaded] resolve_lib_root failed: {:?}", e);
                        }
                    }
                }
            } else {
                eprintln!("[ensure_library_loaded] no deps extracted");
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
mod ai_contract;
mod build_cmd;
mod defs;
mod export_cmd;
mod lib_cmd;
mod lsp;
mod show;

pub use admin::*;
pub use ai_contract::*;
pub use build_cmd::*;
pub use defs::*;
pub use export_cmd::*;
pub use lib_cmd::*;
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
    builder
}
