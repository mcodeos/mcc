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
//!   - -32100  IO / FS error
//!   - -32101  workspace conflict / cannot create
//!   - -32102  workspace does not exist
//!   - -32103  archive / decode failed
//!   - -32104  unsupported format
//!   - -32105  entry file not found
//!   - -32106  dependency not loaded
//!   - -32107  Pass1 / Pass2 failed

use super::protocol::{JsonRpcError, RpcResult};
use crate::McURI;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::info;

// C bindings for controlling log output
extern "C" {
    fn mcc_reset(log_flags: libc::c_uchar);
}

const MCC_SYSTEM_ENV: &str = "MCC_SYSTEM_ROOT";

fn mcc_system_root() -> PathBuf {
    if let Ok(val) = std::env::var(MCC_SYSTEM_ENV) {
        let p = PathBuf::from(&val);
        return if p.is_absolute() {
            p
        } else {
            std::env::current_dir().unwrap_or_default().join(p)
        };
    }
    // Prefer local mc/ directory in current working directory
    if let Ok(cwd) = std::env::current_dir() {
        let local = cwd.join("mc");
        if local.exists() {
            return local;
        }
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".mcode")
}

fn projects_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mcc-projects")
}
fn project_dir(id: &str) -> PathBuf {
    projects_dir().join(id)
}
fn project_src_dir(id: &str) -> PathBuf {
    project_dir(id).join("src")
}
fn project_src_dir_from_root(root: &Path, _id: &str) -> PathBuf {
    root.join("src")
}
fn project_manifest(id: &str) -> PathBuf {
    project_dir(id).join("manifest.toml")
}
fn project_manifest_from_root(root: &Path, _id: &str) -> PathBuf {
    root.join("manifest.toml")
}
fn mcode_dir() -> PathBuf {
    mcc_system_root().join("mcode")
}

// ============================================================================
// Existing methods (preserved, behavior unchanged)
// ============================================================================

pub fn handle_project_list(_params: Option<Value>) -> RpcResult {
    let pdir = projects_dir();
    if !pdir.exists() {
        return Ok(json!([]));
    }
    let mut projects = Vec::new();
    for entry in fs::read_dir(&pdir).map_err(io_err)? {
        let entry = entry.map_err(io_err)?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                projects.push(json!({
                    "name": name,
                    "path": path.to_string_lossy(),
                    "has_manifest": project_manifest(name).exists(),
                }));
            }
        }
    }
    Ok(Value::Array(projects))
}

pub fn handle_project_info(params: Option<Value>) -> RpcResult {
    let name = parse_string_param(params, &["name", "project"])?;
    let pdir = project_dir(&name);
    if !pdir.exists() {
        return Err(JsonRpcError::custom(-32102, "project not found"));
    }
    let (active_id, _, _) = crate::workspace_info();
    Ok(json!({
        "name": name,
        "path": pdir.to_string_lossy(),
        "has_manifest": project_manifest(&name).exists(),
        "active": name == active_id,
    }))
}

pub fn handle_library_list(_params: Option<Value>) -> RpcResult {
    let mut libs = Vec::new();
    // Memory-loaded libraries
    let loaded = crate::mcb_loaded_libs();
    for name in &loaded {
        let info = crate::mcb_lib_info(name);
        libs.push(json!({
            "name": name,
            "loaded": true,
            "symbols": info.as_ref().map(|i| i.total_symbols).unwrap_or(0),
            "modules": info.as_ref().map(|i| i.module_count).unwrap_or(0),
            "components": info.as_ref().map(|i| i.component_count).unwrap_or(0),
        }));
    }
    // Disk-installed libraries
    let mut installed = Vec::new();
    if mcode_dir().exists() && !loaded.contains(&"mcode".to_string()) {
        installed.push(json!({"name":"mcode","version":"*","loaded":false}));
    }
    // Skip system directories
    let system_dirs = ["logs", "config", "mclibs", "projects", "unitest", "mcode"];
    if let Ok(entries) = fs::read_dir(mcc_system_root()) {
        for entry in entries.flatten() {
            let fname = entry.file_name().to_string_lossy().to_string();
            if entry.path().is_dir() && !system_dirs.contains(&fname.as_str()) {
                let (name, version) = match fname.find('@') {
                    Some(at) => (fname[..at].to_string(), fname[at + 1..].to_string()),
                    None => (fname.clone(), "0.0.0".to_string()),
                };
                // Skip mcode (already handled above)
                if name == "mcode" || loaded.contains(&name) {
                    continue;
                }
                // Check for valid .mc entry file
                let lib_path = mcc_system_root().join(&fname);
                let entry_file = lib_path.join(format!("{name}.mc"));
                if !entry_file.exists() {
                    continue;
                }
                installed.push(json!({"name":name,"version":version,"loaded":false}));
            }
        }
    }
    Ok(json!({"loaded": libs, "installed": installed}))
}

#[derive(Deserialize)]
struct LibraryShowParams {
    name: String,
}

pub fn handle_library_show(params: Option<Value>) -> RpcResult {
    let p: LibraryShowParams = parse_strict(params)?;
    let name = p.name.as_str();

    // Get library info
    let info = crate::mcb_lib_info(name).ok_or_else(|| {
        JsonRpcError::custom(-32602, format!("Library '{}' not loaded", p.name).as_str())
    })?;

    Ok(json!({
        "name": info.name,
        "root": info.root,
        "modules": info.modules,
        "module_count": info.module_count,
        "components": info.components,
        "component_count": info.component_count,
        "interfaces": info.interfaces,
        "interface_count": info.interface_count,
        "enums": info.enums,
        "enum_count": info.enum_count,
        "total_symbols": info.total_symbols,
        "loaded": true,
    }))
}

pub fn handle_server_info(_params: Option<Value>) -> RpcResult {
    let (active_id, kind, root) = crate::workspace_info();
    Ok(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "status": "running",
        "data_dir": mcc_system_root().to_string_lossy(),
        "active_workspace": {
            "id": active_id,
            "kind": kind,
            "root": root,
        },
        "loaded_libs": crate::mcb_loaded_libs(),
    }))
}

pub fn handle_methods(_params: Option<Value>) -> RpcResult {
    let methods = [
        // discovery
        "server.info",
        "server.methods",
        // lib
        "library.list",
        "library.show",
        "lib.load",
        "lib.unload",
        // trace
        "trace.set",
        "trace.get",
        // build
        "build.full",
        // parse
        "parse",
        // check / extract
        "check",
        "extract",
        // show
        "show.component",
        "show.component.list",
        "show.module",
        "show.module.list",
        "show.interface",
        "show.interface.list",
        "show.net",
        "show.net.list",
    ];
    Ok(json!(methods
        .iter()
        .map(|s| Value::String(s.to_string()))
        .collect::<Vec<_>>()))
}

// ============================================================================
// Lib handlers
// ============================================================================

pub fn handle_lib_load(params: Option<Value>) -> RpcResult {
    let name = parse_string_param(params, &["name", "lib"])?;
    let root = resolve_lib_root(&name)?;
    if !crate::mcb_load_lib(&name, &root) {
        return Err(JsonRpcError::custom(-32107, "lib load failed"));
    }
    let info = crate::mcb_lib_info(&name);
    Ok(json!({
        "name": name,
        "loaded": true,
        "root": root.to_string_lossy(),
        "symbols": info.as_ref().map(|i| i.total_symbols).unwrap_or(0),
        "modules": info.as_ref().map(|i| i.module_count).unwrap_or(0),
        "components": info.as_ref().map(|i| i.component_count).unwrap_or(0),
    }))
}

pub fn handle_lib_unload(params: Option<Value>) -> RpcResult {
    let name = parse_string_param(params, &["name", "lib"])?;
    let ok = crate::mcb_unload_lib(&name);
    Ok(json!({"name": name, "unloaded": ok}))
}

// ============================================================================
// Trace handlers
// ============================================================================

#[derive(Deserialize)]
struct TraceSetParams {
    name: String,
    value: bool,
}

pub fn handle_trace_set(params: Option<Value>) -> RpcResult {
    let p: TraceSetParams = parse_strict(params)?;
    match p.name.as_str() {
        // ── C parser trace flags (token/ast/sem/visit)──
        "trace.enabled" | "enabled" => crate::cli::config::set_trace_enabled(p.value),
        "trace.ast" | "ast" => crate::cli::config::set_trace_ast(p.value),
        "trace.lexer" | "lexer" => crate::cli::config::set_trace_lexer(p.value),
        "trace.parser" | "parser" => crate::cli::config::set_trace_parser(p.value),
        "trace.visit" | "visit" => crate::cli::config::set_trace_visit(p.value),
        // ── Rust log pass flags (real-time effect)──
        "trace.pass1" | "pass1" => crate::cli::config::set_log_pass1(p.value),
        "trace.pass2" | "pass2" => crate::cli::config::set_log_pass2(p.value),
        "trace.server" | "server" => crate::cli::config::set_log_server(p.value),
        _ => {
            return Err(JsonRpcError::custom(
                -32099,
                &format!("unknown trace config: {}", p.name),
            ));
        }
    }
    Ok(json!({"name": p.name, "value": p.value}))
}

pub fn handle_trace_get(params: Option<Value>) -> RpcResult {
    let name = parse_string_param(params, &["name"])?;
    let value = match name.as_str() {
        "trace.enabled" | "enabled" => crate::cli::config::get_trace_enabled(),
        "trace.ast" | "ast" => crate::cli::config::get_trace_ast(),
        "trace.lexer" | "lexer" => crate::cli::config::get_trace_lexer(),
        "trace.parser" | "parser" => crate::cli::config::get_trace_parser(),
        "trace.visit" | "visit" => crate::cli::config::get_trace_visit(),
        "trace.pass1" | "pass1" => crate::cli::config::get_log_pass1(),
        "trace.pass2" | "pass2" => crate::cli::config::get_log_pass2(),
        "trace.server" | "server" => crate::cli::config::get_log_server(),
        _ => {
            return Err(JsonRpcError::custom(
                -32099,
                &format!("unknown trace config: {name}"),
            ));
        }
    };
    Ok(json!({"name": name, "value": value}))
}

// ============================================================================
// Common build.full handlers (based on active workspace)
// ============================================================================

#[derive(Default, Deserialize)]
struct BuildFullParams {
    #[serde(default)]
    entry: Option<String>,
    #[serde(default)]
    top: Option<String>,
    /// Whether to include system library definitions, default true
    #[serde(default = "default_true")]
    include_system: bool,
    /// Whether to output AST visit, default false
    #[serde(default)]
    include_ast: bool,
    #[serde(default)]
    libs: Vec<String>,
}

fn default_true() -> bool {
    true
}

pub fn handle_build_full(params: Option<Value>) -> RpcResult {
    let p: BuildFullParams = parse_or_default(params)?;
    load_libs_rpc(&p.libs);

    let (id, kind, root_str) = crate::workspace_info();
    if kind == "Anonymous" {
        let entry = p.entry.as_deref().ok_or_else(|| {
            JsonRpcError::custom(-32602, "build.full: need <entry> or active workspace")
        })?;
        let cwd = std::env::current_dir().unwrap_or_default();
        let entry_path = PathBuf::from(entry);
        let abs_entry = if entry_path.is_absolute() {
            entry_path
        } else {
            cwd.join(&entry_path)
        };
        return run_full_build(
            &abs_entry,
            p.top.as_deref(),
            "build.full",
            "file",
            &id,
            p.include_system,
        );
    }

    let _root = PathBuf::from(&root_str);
    let entry_path = match kind.as_str() {
        "Project" => resolve_project_entry(&id, p.entry.as_deref())?,
        _ => return Err(JsonRpcError::custom(-32102, "unknown workspace kind")),
    };
    let top = p.top.or_else(read_project_top_from_workspace);
    run_full_build(
        &entry_path,
        top.as_deref(),
        "build.full",
        "project",
        &id,
        p.include_system,
    )
}

// ============================================================================
// Internal: Pass1 / Pass2 execution
// ============================================================================

fn run_pass1(
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
fn run_full_build(
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
            .ok_or_else(|| JsonRpcError::custom(-32107, "no top module found"))?,
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
fn resolve_virtual_entry(
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
        .ok_or_else(|| JsonRpcError::custom(-32105, "no .mc entry found"))
}

/// Load all files from memory and execute Pass1
fn run_pass1_from_memory(
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
fn run_full_build_from_memory(
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
            .ok_or_else(|| JsonRpcError::custom(-32107, "no top module found"))?,
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

fn collect_pass1(_uri: &str, include_system: bool) -> Value {
    let all_modules = collect_definitions(crate::mcb_iter_modules());
    let all_components = collect_definitions(crate::mcb_iter_components());
    let all_interfaces = collect_definitions(crate::mcb_iter_interfaces());
    let all_enums = collect_definitions(crate::mcb_iter_enums());

    // Filter out system modules, components, interfaces, enums if not include_system
    let (modules, components, interfaces, enums) = if include_system {
        (all_modules, all_components, all_interfaces, all_enums)
    } else {
        let filter = |items: Vec<(String, String)>| -> Vec<(String, String)> {
            items
                .into_iter()
                .filter(|(_, uri)| !is_system_uri(uri))
                .collect()
        };
        (
            filter(all_modules),
            filter(all_components),
            filter(all_interfaces),
            filter(all_enums),
        )
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

    json!({
        "loaded_files": loaded_files,
        "definitions": {
            "modules":    refs_json(&modules),
            "components": refs_json(&components),
            "interfaces": refs_json(&interfaces),
            "enums":      refs_json(&enums),
        },
        "diagnostics": []
    })
}

fn collect_pass2(top: &str, inst: &crate::MccProjectTree) -> Value {
    json!({
        "top": top,
        "instances": instance_to_json(inst),
        "nets":       extract_nets(inst),
        "diagnostics": []
    })
}

fn instance_to_json(inst: &crate::MccProjectTree) -> Value {
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

fn extract_nets(_inst: &crate::MccProjectTree) -> Vec<Value> {
    // Placeholder: aggregate inst.connections / inst.nets
    Vec::new()
}

fn iotype_str(io: &crate::IOType) -> &'static str {
    use crate::IOType::*;
    match io {
        In => "in",
        Out => "out",
        InOut => "inout",
        Power => "power",
        Analog => "analog",
        Return => "return",
        NonCon => "noncon",
        None => "none",
    }
}

// ============================================================================
// File entry grouping
// ============================================================================

struct FileEntry {
    uri: String,
    is_system: bool,
    modules: Vec<String>,
    components: Vec<String>,
    interfaces: Vec<String>,
    enums: Vec<String>,
}

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
fn is_system_uri(uri: &str) -> bool {
    uri.contains("/mcode/") || uri.contains("\\mcode\\")
}

fn collect_definitions(items: Vec<(String, String)>) -> Vec<(String, String)> {
    items
}

fn refs_json(items: &[(String, String)]) -> Vec<Value> {
    items
        .iter()
        .map(|(n, u)| json!({"name": n, "uri": u}))
        .collect()
}

fn load_libs_rpc(libs: &[String]) {
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

#[derive(Deserialize, Default)]
struct CheckRpcParams {
    #[serde(default)]
    entry: Option<String>,
    #[serde(default)]
    libs: Vec<String>,
    #[serde(default)]
    strict: bool,
    #[serde(default)]
    errors_only: bool,
}

pub fn handle_check(params: Option<Value>) -> RpcResult {
    let p: CheckRpcParams = parse_or_default(params)?;
    load_libs_rpc(&p.libs);

    let (id, kind, _) = crate::workspace_info();
    if kind == "Anonymous" {
        let entry = p
            .entry
            .as_deref()
            .ok_or_else(|| JsonRpcError::custom(-32602, "check: need to specify <entry>"))?;

        let cwd = std::env::current_dir().unwrap_or_default();
        let entry_path = PathBuf::from(entry);
        let abs_entry = if entry_path.is_absolute() {
            entry_path.clone()
        } else {
            cwd.join(&entry_path)
        };

        let _uri = McURI::from(abs_entry.to_string_lossy().as_ref() as &str);

        return run_full_build(&abs_entry, None, "check", "file", &id, false);
    }

    let bp = json!({ "entry": p.entry, "include_system": false });
    handle_build_full(Some(bp))
}

#[derive(Deserialize, Default)]
struct ExtractRpcParams {
    #[serde(default)]
    entry: Option<String>,
    #[serde(default)]
    target: String,
    #[serde(default)]
    top: Option<String>,
    #[serde(default)]
    libs: Vec<String>,
}

pub fn handle_extract(params: Option<Value>) -> RpcResult {
    let p: ExtractRpcParams = parse_or_default(params)?;
    load_libs_rpc(&p.libs);

    let (id, kind, root_str) = crate::workspace_info();
    if kind == "Anonymous" {
        let entry = p
            .entry
            .as_deref()
            .ok_or_else(|| JsonRpcError::custom(-32602, "extract: need to specify <entry>"))?;
        let cwd = std::env::current_dir().unwrap_or_default();
        let entry_path = PathBuf::from(entry);
        let abs_entry = if entry_path.is_absolute() {
            entry_path.clone()
        } else {
            cwd.join(&entry_path)
        };
        let uri = McURI::from(abs_entry.to_string_lossy().as_ref() as &str);
        crate::mcc_load_project(&uri);
        return extract_from_uri(&abs_entry, p.top.as_deref(), &p.target);
    }

    let _root = PathBuf::from(&root_str);
    let entry_path = match kind.as_str() {
        "Project" => resolve_project_entry(&id, p.entry.as_deref())?,
        _ => {
            return Err(JsonRpcError::custom(
                -32102,
                "extract: only project workspace is supported",
            ))
        }
    };
    extract_from_uri(&entry_path, p.top.as_deref(), &p.target)
}

fn extract_from_uri(entry: &Path, top: Option<&str>, target: &str) -> RpcResult {
    let uri = entry.to_string_lossy().to_string();
    let mc_uri = McURI::from(uri.as_str());

    let top_name = match top {
        Some(t) => t.to_string(),
        None => crate::mcb_get_module_name_by_uri(&mc_uri)
            .or_else(crate::mcb_get_first_module_name)
            .ok_or_else(|| JsonRpcError::custom(-32107, "no top module found"))?,
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
                                crate::McInstance::Bus(b) => ("bus", b.name().to_string()),
                                crate::McInstance::BusRef { component, bus } => {
                                    ("busref", format!("{component}.{bus}"))
                                }
                                crate::McInstance::List(l) => ("list", l.name().to_string()),
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
                Ok(Err(e)) => Err(JsonRpcError::custom(-32107, &format!("build failed: {e}"))),
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

fn parse_strict<T: for<'de> Deserialize<'de>>(params: Option<Value>) -> Result<T, JsonRpcError> {
    let v = params.ok_or_else(JsonRpcError::invalid_params)?;
    serde_json::from_value(v).map_err(|_| JsonRpcError::invalid_params())
}

fn parse_or_default<T: for<'de> Deserialize<'de> + Default>(
    params: Option<Value>,
) -> Result<T, JsonRpcError> {
    match params {
        Some(v) => serde_json::from_value(v).map_err(|_| JsonRpcError::invalid_params()),
        None => Ok(T::default()),
    }
}

fn parse_string_param(params: Option<Value>, keys: &[&str]) -> Result<String, JsonRpcError> {
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

fn io_err(e: std::io::Error) -> JsonRpcError {
    JsonRpcError::custom(-32100, &format!("io error: {e}"))
}

// ============================================================================
// Auxiliary: file / path handling
// ============================================================================

#[derive(Deserialize)]
struct UploadFile {
    path: String,
    content: String,
}

fn write_files(root: &Path, files: &[UploadFile]) -> (Vec<String>, Vec<String>) {
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

fn is_safe_relative(p: &str) -> bool {
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

fn extract_archive(
    format: &str,
    data_b64: &str,
    dest: &Path,
    strip: usize,
) -> Result<Vec<String>, JsonRpcError> {
    use base64::Engine;
    let data = base64::engine::general_purpose::STANDARD
        .decode(data_b64)
        .map_err(|e| JsonRpcError::custom(-32103, &format!("base64 decode: {e}")))?;
    match format {
        "tar.gz" | "tgz" => extract_tar_gz(&data, dest, strip),
        "tar" => extract_tar(&data, dest, strip),
        other => Err(JsonRpcError::custom(
            -32104,
            &format!("unsupported archive format: {other}"),
        )),
    }
}

fn extract_tar_gz(data: &[u8], dest: &Path, strip: usize) -> Result<Vec<String>, JsonRpcError> {
    use flate2::read::GzDecoder;
    use tar::Archive;
    let gz = GzDecoder::new(data);
    let mut archive = Archive::new(gz);
    extract_tar_entries(&mut archive, dest, strip)
}

fn extract_tar(data: &[u8], dest: &Path, strip: usize) -> Result<Vec<String>, JsonRpcError> {
    use tar::Archive;
    let mut archive = Archive::new(data);
    extract_tar_entries(&mut archive, dest, strip)
}

fn extract_tar_entries<R: std::io::Read>(
    archive: &mut tar::Archive<R>,
    dest: &Path,
    strip: usize,
) -> Result<Vec<String>, JsonRpcError> {
    let mut extracted = Vec::new();
    let entries = archive
        .entries()
        .map_err(|e| JsonRpcError::custom(-32103, &format!("tar entries: {e}")))?;
    for entry in entries {
        let mut entry =
            entry.map_err(|e| JsonRpcError::custom(-32103, &format!("tar entry: {e}")))?;
        let entry_path = entry
            .path()
            .map_err(|e| JsonRpcError::custom(-32103, &format!("tar path: {e}")))?
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
            .map_err(|e| JsonRpcError::custom(-32103, &format!("unpack: {e}")))?;
        extracted.push(stripped.to_string_lossy().to_string());
    }
    Ok(extracted)
}

fn resolve_project_entry(_name: &str, entry: Option<&str>) -> Result<PathBuf, JsonRpcError> {
    let (_, _, root_str) = crate::workspace_info();
    let root = PathBuf::from(&root_str);
    let src_root = root.join("src");

    // Prefer src/ directory
    if let Some(rel) = entry {
        if !is_safe_relative(rel) {
            return Err(JsonRpcError::custom(-32105, "unsafe entry path"));
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
    Err(JsonRpcError::custom(-32105, "no .mc entry found in src/"))
}

fn scan_mc_files_recursive(root: &Path, current: &Path, out: &mut Vec<String>) {
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

fn read_manifest_entry(name: &str) -> Option<String> {
    let content = fs::read_to_string(project_manifest(name)).ok()?;
    parse_manifest_field(&content, "entry")
}

fn read_project_entry_from_workspace() -> Option<String> {
    let (_, _, root_str) = crate::workspace_info();
    let project_toml = PathBuf::from(&root_str).join("project.toml");
    let content = fs::read_to_string(&project_toml).ok()?;
    parse_manifest_field(&content, "entry")
}

fn read_manifest_top(name: &str) -> Option<String> {
    let content = fs::read_to_string(project_manifest(name)).ok()?;
    parse_manifest_field(&content, "top_module")
}

fn read_project_top_from_workspace() -> Option<String> {
    let (_, _, root_str) = crate::workspace_info();
    let project_toml = PathBuf::from(&root_str).join("project.toml");
    let content = fs::read_to_string(&project_toml).ok()?;
    parse_manifest_field(&content, "top_module")
}

fn parse_manifest_field(content: &str, key: &str) -> Option<String> {
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

fn activate_workspace(name: &str) -> Result<(), JsonRpcError> {
    let (active, _, _) = crate::workspace_info();
    if active == name {
        return Ok(());
    }
    if !crate::workspace_switch(name) {
        return Err(JsonRpcError::custom(-32102, "workspace not found"));
    }
    Ok(())
}

fn resolve_lib_root(name: &str) -> Result<PathBuf, JsonRpcError> {
    if name == "mcode" {
        let p = mcode_dir();
        if p.exists() {
            return Ok(p);
        }
        return Err(JsonRpcError::custom(-32102, "mcode dir not found"));
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

#[derive(Default, Deserialize)]
struct ParseParams {
    #[serde(default)]
    entry: Option<String>,
    #[serde(default)]
    top: Option<String>,
    /// System libraries to load (e.g. like ["mc/mcode"]);
    #[serde(default)]
    libs: Vec<String>,
    /// Whether to include system library definitions, default: true
    #[serde(default = "default_true")]
    include_system: bool,
}

pub fn handle_parse(params: Option<Value>) -> RpcResult {
    let p: ParseParams = parse_or_default(params)?;
    let (id, kind, root) = crate::workspace_info();

    // S3 fix: load the libs passed via CLI --lib into the mcode global table, otherwise mcb_get_cmie
    // cannot find interfaces like SPI/I2C/DC, and the component pin's 'X::Interface(...)' syntax
    // will fall back to a bare alias (e.g. pin registered as Single("VIN{Vin, GND}"))
    load_libs_rpc(&p.libs);

    // Without workspace, parse the file directly
    if kind == "Anonymous" {
        let entry = p
            .entry
            .as_deref()
            .ok_or_else(|| JsonRpcError::custom(-32602, "parse: need to specify <target> file"))?;

        let cwd = std::env::current_dir().unwrap_or_default();
        let entry_path = PathBuf::from(entry);
        let abs_entry = if entry_path.is_absolute() {
            entry_path.clone()
        } else {
            cwd.join(&entry_path)
        };

        let _uri = McURI::from(abs_entry.to_string_lossy().as_ref() as &str);

        return run_pass1(&abs_entry, "parse", "file", &id, p.include_system);
    }

    let cwd = std::env::current_dir().unwrap_or_default();
    let root_path = cwd.join(&root);

    let entry_str = p.entry.as_deref().map(|s| {
        let entry_path = PathBuf::from(s);
        let abs_entry = if entry_path.is_absolute() {
            entry_path.clone()
        } else {
            cwd.join(&entry_path)
        };
        if abs_entry.starts_with(&root_path) {
            abs_entry
                .strip_prefix(&root_path)
                .unwrap_or(&abs_entry)
                .to_string_lossy()
                .to_string()
        } else {
            s.to_string()
        }
    });

    let entry_path = match kind.as_str() {
        "Project" => resolve_project_entry(&id, entry_str.as_deref())?,
        _ => {
            return Err(JsonRpcError::custom(
                -32102,
                "parse: only project workspace is supported",
            ))
        }
    };
    run_pass1(&entry_path, "parse", "project", &id, p.include_system)
}

// ============================================================================
// Show handlers
// ============================================================================

#[derive(Deserialize, Default)]
struct ShowParams {
    name: Option<String>,
    file: Option<String>,
}

pub fn handle_show_component_list(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_or_default(params)?;

    // If a file is specified, load it
    if let Some(file) = &p.file {
        let uri = McURI::from(file.as_str());
        crate::mcc_load_project(&uri);
    }

    let comps: Vec<(String, String)> = crate::mcb_iter_components();
    let names: Vec<String> = comps.iter().map(|(n, _)| n.clone()).collect();

    Ok(json!({
        "type": "component",
        "count": names.len(),
        "list": names,
    }))
}

pub fn handle_show_module_list(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_or_default(params)?;

    // If a file is specified, load it
    if let Some(file) = &p.file {
        let uri = McURI::from(file.as_str());
        crate::mcc_load_project(&uri);
    }

    let modules: Vec<(String, String)> = crate::mcb_iter_modules();
    let names: Vec<String> = modules.iter().map(|(n, _)| n.clone()).collect();

    Ok(json!({
        "type": "module",
        "count": names.len(),
        "list": names,
    }))
}

pub fn handle_show_interface_list(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_or_default(params)?;

    // If a file is specified, load it
    if let Some(file) = &p.file {
        let uri = McURI::from(file.as_str());
        crate::mcc_load_project(&uri);
    }

    let ifaces: Vec<(String, String)> = crate::mcb_iter_interfaces();
    let names: Vec<String> = ifaces.iter().map(|(n, _)| n.clone()).collect();

    Ok(json!({
        "type": "interface",
        "count": names.len(),
        "list": names,
    }))
}

pub fn handle_show_net_list(_params: Option<Value>) -> RpcResult {
    Ok(json!({
        "type": "net",
        "count": 0,
        "list": [],
        "note": "Nets need to be retrieved when viewing modules via show.module",
    }))
}

pub fn handle_show_component(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_strict(params)?;

    // If a file is specified, load it
    if let Some(file) = &p.file {
        let uri = McURI::from(file.as_str());
        crate::mcc_load_project(&uri);
    }

    let name = p
        .name
        .as_ref()
        .ok_or_else(|| JsonRpcError::custom(-32602, "show.component: need to specify name"))?;
    let comps = crate::mcb_iter_components();
    let name_str = name.as_str();

    let (matched_name, uri) = comps
        .iter()
        .find(|(n, _)| n == name_str)
        .map(|(n, u)| (n.clone(), u.clone()))
        .ok_or_else(|| JsonRpcError::custom(-32003, &format!("component not found: {name}")))?;

    let ident = crate::McIds::from(matched_name.as_str());
    let uri_obj = crate::McURI::from(uri.as_str());

    let cmie = crate::get_def(&ident, &uri_obj)
        .ok_or_else(|| JsonRpcError::custom(-32003, &format!("component not found: {name}")))?;

    match cmie {
        crate::McCMIE::Component(comp) => {
            // Build detailed pin information
            let pins: Vec<serde_json::Value> = comp
                .pins
                .pins
                .iter()
                .map(|(pin_id, pin)| {
                    // Try to extract description from values
                    let mut desc = String::new();
                    for val in pin.values.iter() {
                        if let crate::McAttrVal::AttrLiteral(crate::McLiteral::String(s)) = val {
                            if !desc.is_empty() {
                                desc.push(' ');
                            }
                            desc.push_str(&s.value);
                        }
                    }

                    let mut pin_json = json!({
                        "id": pin_id,
                        "iotype": format!("{:?}", pin.iotype),
                        "names": pin.names,
                    });
                    if !desc.is_empty() {
                        pin_json["description"] = json!(desc);
                    }
                    pin_json
                })
                .collect();

            Ok(json!({
                "name": matched_name,
                "uri": uri,
                "pins": pins,
                "pin_count": comp.pins.pins.len(),
            }))
        }
        _ => Err(JsonRpcError::custom(
            -32002,
            &format!("'{name}' is not a Component"),
        )),
    }
}

pub fn handle_show_module(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_strict(params)?;

    let name = p
        .name
        .as_ref()
        .ok_or_else(|| JsonRpcError::custom(-32602, "show.module: need to specify name"))?;

    let first_module_name = crate::mcb_get_first_module_name()
        .ok_or_else(|| JsonRpcError::custom(-32003, "no module found"))?;

    let uri = crate::McURI::from(first_module_name.as_str());
    let ident = crate::McIds::from(name.as_str());

    let cmie = crate::get_def(&ident, &uri)
        .ok_or_else(|| JsonRpcError::custom(-32003, &format!("module not found: {name}")))?;

    match cmie {
        crate::McCMIE::Module(module) => {
            let insts: Vec<serde_json::Value> = module
                .insts
                .iter()
                .map(|(n, inst)| {
                    let (kind, class) = match inst {
                        crate::McInstance::Component(c) => ("component", c.name.to_string()),
                        crate::McInstance::Module(m) => ("module", m.name.to_string()),
                        crate::McInstance::Label(l) => ("label", l.clone()),
                        crate::McInstance::Interface(i) => ("interface", i.name.to_string()),
                        crate::McInstance::Bus(b) => ("bus", b.name().to_string()),
                        crate::McInstance::BusRef { component, bus } => {
                            ("busref", format!("{component}.{bus}"))
                        }
                        crate::McInstance::List(l) => ("list", l.name().to_string()),
                    };
                    json!({ "name": n.to_string(), "kind": kind, "class": class })
                })
                .collect();

            Ok(json!({
                "name": name,
                "uri": uri,
                "instances": insts,
            }))
        }
        _ => Err(JsonRpcError::custom(
            -32002,
            &format!("'{name}' is not a Module"),
        )),
    }
}

pub fn handle_show_interface(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_strict(params)?;

    let name = p
        .name
        .as_ref()
        .ok_or_else(|| JsonRpcError::custom(-32602, "show.interface: need to specify name"))?;

    let ifaces = crate::mcb_iter_interfaces();
    let name_str = name.as_str();

    let (matched_name, uri) = ifaces
        .iter()
        .find(|(n, _)| n == name_str)
        .map(|(n, u)| (n.clone(), u.clone()))
        .ok_or_else(|| JsonRpcError::custom(-32003, &format!("interface not found: {name}")))?;

    let ident = crate::McIds::from(matched_name.as_str());
    let uri_obj = crate::McURI::from(uri.as_str());

    let cmie = crate::get_def(&ident, &uri_obj)
        .ok_or_else(|| JsonRpcError::custom(-32003, &format!("interface not found: {name}")))?;

    match cmie {
        crate::McCMIE::Interface(_) => Ok(json!({
            "name": matched_name,
            "uri": uri,
        })),
        _ => Err(JsonRpcError::custom(
            -32002,
            &format!("'{name}' is not an Interface"),
        )),
    }
}

pub fn handle_show_net(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_strict(params)?;

    let top_name = crate::mcb_get_first_module_name()
        .ok_or_else(|| JsonRpcError::custom(-32003, "no module found"))?;

    let uri = crate::McURI::from(top_name.as_str());
    let ident = crate::McIds::from(top_name.as_str());

    let inst = crate::mcc_build(&ident, &uri)
        .map_err(|e| JsonRpcError::custom(-32002, &format!("build failed: {e}")))?;

    use std::collections::BTreeMap;
    let mut nets: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for conn in &inst.connections {
        let net = conn
            .net_name
            .clone()
            .unwrap_or_else(|| format!("__net_{}", conn.id));
        if net == "NC" {
            continue;
        }
        let bucket = nets.entry(net).or_default();
        for pt in &conn.points {
            if pt.path == "NC" {
                continue;
            }
            let label = if let Some(ref o) = pt.owner {
                format!(
                    "{}.{}",
                    o,
                    pt.path.split('.').next_back().unwrap_or(&pt.path)
                )
            } else {
                pt.path.clone()
            };
            if !bucket.contains(&label) {
                bucket.push(label);
            }
        }
    }

    if p.name.is_none() {
        let items: Vec<serde_json::Value> = nets
            .iter()
            .map(|(n, points)| json!({ "name": n, "points": points }))
            .collect();
        Ok(json!({ "nets": items }))
    } else {
        let name = p.name.unwrap();
        match nets.get(&name) {
            Some(points) => Ok(json!({ "name": name, "points": points })),
            None => Ok(
                json!({ "name": name, "points": Vec::<String>::new(), "error": "net not found" }),
            ),
        }
    }
}

// ============================================================================
// Semantic data (sem tokens + symbols) for LSP
// ============================================================================

#[derive(Deserialize)]
struct SemParams {
    uri: String,
}

pub fn handle_sem(params: Option<Value>) -> RpcResult {
    let p: SemParams = parse_strict(params)?;
    let raw_uri = &p.uri;

    // Build candidate URIs: exact match + relative path (strip project root from absolute)
    let (_, _, root_str) = crate::workspace_info();
    let cwd = std::env::current_dir().unwrap_or_default();
    let root_path = if Path::new(&root_str).is_absolute() {
        PathBuf::from(&root_str)
    } else {
        cwd.join(&root_str)
    };

    let raw_path = Path::new(raw_uri);
    let mut candidates: Vec<McURI> = vec![McURI::from(raw_uri.as_str())];
    if raw_path.is_absolute() && raw_path.starts_with(&root_path) {
        let rel = raw_path.strip_prefix(&root_path).unwrap_or(raw_path);
        let rel_str = rel.to_string_lossy().to_string();
        if rel_str != *raw_uri {
            candidates.push(McURI::from(rel_str));
        }
    }

    // Try lookup
    let result = try_lookup_sem(&candidates);

    // If not found and workspace is empty, auto-create workspace and load project
    if result.is_none() {
        let workspace_empty = {
            let binding = crate::builder::workspace::WORKSPACE.mcodes.borrow();
            binding.is_empty()
        };
        if workspace_empty && raw_path.is_absolute() {
            auto_load_from_file_path(raw_path);
            return try_lookup_sem(&candidates)
                .ok_or_else(|| JsonRpcError::custom(-32100, "file not found in workspace"));
        }
    }

    result.ok_or_else(|| JsonRpcError::custom(-32100, "file not found in workspace"))
}

/// Detect project root from a file path and load the project
fn auto_load_from_file_path(file_path: &Path) {
    // Walk up from the file to find the project root (directory containing project.toml or .mc files)
    let project_root = find_project_root(file_path);
    info!(target: "mcc::rpc", "auto_load: project_root={}", project_root.display());

    // Create project workspace
    let root_name = project_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".to_string());
    info!(target: "mcc::rpc", "auto_load: creating workspace id={} root={}", root_name, project_root.display());
    crate::workspace_create(
        &root_name,
        crate::WorkspaceKind::Project,
        &project_root,
    );

    // Load all .mc files in the project (not just one entry)
    // This ensures files like c2.ports.mc that aren't imported by other files are also available
    let mut all_files = Vec::new();
    scan_mc_files_recursive(&project_root, &project_root, &mut all_files);
    info!(target: "mcc::rpc", "auto_load: found {} .mc files", all_files.len());
    for rel in &all_files {
        let full = project_root.join(rel);
        let uri = McURI::from(full.to_string_lossy().to_string());
        crate::mcc_add(&uri);
        info!(target: "mcc::rpc", "auto_load: added {}", full.display());
    }
    crate::builder::mcb_parse_all_modules();
}

/// Walk up from a file path to find the project root
/// A project root is a directory containing project.toml or .mc files at top level
fn find_project_root(file_path: &Path) -> PathBuf {
    let mut current = if file_path.is_dir() {
        file_path.to_path_buf()
    } else {
        file_path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."))
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
    file_path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."))
}

/// Try to find semantic data for any of the candidate URIs
fn try_lookup_sem(candidates: &[McURI]) -> Option<Value> {
    let binding = crate::builder::workspace::WORKSPACE.mcodes.borrow();
    for mc_uri in candidates {
        if let Some(mcfile) = binding.get(mc_uri) {
            let tokens: Vec<serde_json::Value> = mcfile
                .tokens
                .lock()
                .map(|t: std::sync::MutexGuard<'_, crate::McSemTokens>| {
                    t.iter()
                        .map(|tok| {
                            json!({
                                "type": tok.type_,
                                "position": tok.position,
                                "length": tok.length,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            let symbols = mcfile
                .symbols
                .lock()
                .map(|s| crate::ast::ast_semantic::symbol_table_to_json(&s, mc_uri))
                .unwrap_or_else(|_| serde_json::json!({}));

            return Some(json!({
                "tokens": tokens,
                "symbols": symbols,
            }));
        }
    }
    None
}
