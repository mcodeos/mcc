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

// C bindings for controlling log output
extern "C" {
    fn mcc_reset(log_flags: libc::c_uchar);
}

const MCC_SYSTEM_ENV: &str = "MCC_SYSTEM_ROOT";

fn mcc_system_root() -> PathBuf {
    // Single source of truth: delegate to data_dir::data_root() (which honors
    // $MCC_SYSTEM_ROOT). The cwd/mc/ probe and the `~/.mcode` fallback live
    // there now.
    crate::cli::data_dir::data_root()
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
        return Err(JsonRpcError::custom(32102, "project not found"));
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
    // Disk-installed libraries: prefer reading the v1 layout's index.json.
    // Falls back to filesystem scan if index is missing or stale.
    let mut installed = Vec::new();
    if let Some(index) = crate::cli::data_dir::read_index_if_present() {
        // v1 index path: enumerate system + 3rdparty from JSON.
        for entry in index.system {
            let name = entry
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if name == "mcode" && !loaded.contains(&"mcode".to_string()) {
                installed.push(json!({"name":"mcode","version":entry.get("version").and_then(|v| v.as_str()).unwrap_or("0.0.0"),"loaded":false}));
            }
        }
        for entry in index.thirdparty {
            let name = entry
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if name.is_empty() || loaded.contains(&name) {
                continue;
            }
            installed.push(json!({
                "name": name,
                "version": entry.get("version").and_then(|v| v.as_str()).unwrap_or("0.0.0"),
                "loaded": false,
            }));
        }
    } else {
        // Fallback: scan mcc_system_root(), system/, and 3rdparty/ (legacy).
        if mcode_dir().exists() && !loaded.contains(&"mcode".to_string()) {
            installed.push(json!({"name":"mcode","version":"*","loaded":false}));
        }
        let system_dirs = ["logs", "config", "mclibs", "projects", "unitest"];
        if let Ok(entries) = fs::read_dir(mcc_system_root()) {
            for entry in entries.flatten() {
                let fname = entry.file_name().to_string_lossy().to_string();
                if entry.path().is_dir() && !system_dirs.contains(&fname.as_str()) {
                    let (name, version) = match fname.find('@') {
                        Some(at) => (fname[..at].to_string(), fname[at + 1..].to_string()),
                        None => (fname.clone(), "0.0.0".to_string()),
                    };
                    if name == "mcode" || loaded.contains(&name) {
                        continue;
                    }
                    let lib_path = mcc_system_root().join(&fname);
                    let entry_file = lib_path.join(format!("{name}.mc"));
                    if !entry_file.exists() {
                        continue;
                    }
                    installed.push(json!({"name":name,"version":version,"loaded":false}));
                }
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
        "lib.list",
        "lib.info",
        "lib.load",
        "lib.unload",
        "lib.install",
        "lib.uninstall",
        "lib.search",
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
        // show — missing containers (M5 drill-down)
        "show.all",
        "show.file",
        "show.files",
        "show.enum",
        "show.enum.list",
        // show — drill-down (M5)
        "show.pins",
        "show.ports",
        "show.ports.list",
        "show.labels",
        "show.instances",
        "show.nets",
        "show.attrs",
        "show.funcs",
        "show.params",
        "show.roles",
        "show.values",
        // search (M5)
        "defs.search",
        "defs.query",
        // export (M5)
        "export",
        // explain (M6)
        "explain",
        "caps",
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
        return Err(JsonRpcError::custom(32107, "lib load failed"));
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

#[derive(Deserialize)]
struct LibInstallParams {
    name: String,
    from: String,
    #[serde(default)]
    version: Option<String>,
}

pub fn handle_lib_install(params: Option<Value>) -> RpcResult {
    let p: LibInstallParams = parse_strict(params)?;
    let src = PathBuf::from(&p.from);
    if !src.exists() {
        return Err(JsonRpcError::custom(
            32100,
            &format!("lib install: source path does not exist '{}'", p.from),
        ));
    }
    let ver = p.version.as_deref().unwrap_or("0.0.0");
    let name_ver = format!("{}@{}", p.name, ver);
    // Flat layout: install into <root>/<name>@<ver>
    let target = crate::cli::data_dir::data_root().join(&name_ver);
    if target.exists() {
        return Err(JsonRpcError::custom(
            32101,
            &format!("lib install: {} is already installed", name_ver),
        ));
    }
    copy_dir_recursive(&src, &target).map_err(io_err)?;
    // Refresh index.json so lib.list sees the new install.
    let _ = crate::cli::data_dir::rebuild_index();
    Ok(json!({
        "installed": name_ver,
        "path": target.to_string_lossy(),
    }))
}

#[derive(Deserialize)]
struct LibUninstallParams {
    name: String,
    #[serde(default)]
    force: bool,
}

pub fn handle_lib_uninstall(params: Option<Value>) -> RpcResult {
    let p: LibUninstallParams = parse_strict(params)?;
    let is_loaded = crate::mcb_loaded_libs().contains(&p.name);
    if is_loaded && !p.force {
        return Err(JsonRpcError::custom(
            32101,
            &format!(
                "lib uninstall: '{}' is loaded; unload first or pass force",
                p.name
            ),
        ));
    }
    if is_loaded && !crate::mcb_unload_lib(&p.name) {
        return Err(JsonRpcError::custom(
            32107,
            &format!("lib uninstall: failed to unload '{}'", p.name),
        ));
    }
    let lib_dir = resolve_installed_lib_dir(&p.name).ok_or_else(|| {
        JsonRpcError::custom(
            32102,
            &format!("lib uninstall: '{}' is not installed", p.name),
        )
    })?;
    fs::remove_dir_all(&lib_dir).map_err(io_err)?;
    // Refresh index.json so lib.list no longer shows the deleted install.
    let _ = crate::cli::data_dir::rebuild_index();
    Ok(json!({
        "uninstalled": p.name,
        "path": lib_dir.to_string_lossy(),
    }))
}

#[derive(Deserialize)]
struct LibSearchParams {
    pattern: String,
}

pub fn handle_lib_search(params: Option<Value>) -> RpcResult {
    let p: LibSearchParams = parse_strict(params)?;
    let pat = p.pattern.to_lowercase();
    let mut results = Vec::new();
    if mcode_dir().exists() && ("mcode".contains(&pat) || pat.is_empty()) {
        results.push(json!({
            "name": "mcode", "version": "*",
            "path": mcode_dir().to_string_lossy(),
        }));
    }

    // Scan flat root directory for installed libs.
    let system_dirs = ["logs", "config", "mclibs", "projects", "unitest"];

    if let Ok(entries) = fs::read_dir(mcc_system_root()) {
        for entry in entries.flatten() {
            let fname = entry.file_name().to_string_lossy().to_string();
            if !entry.path().is_dir() || system_dirs.contains(&fname.as_str()) {
                continue;
            }
            let (name, version) = match fname.find('@') {
                Some(at) => (fname[..at].to_string(), fname[at + 1..].to_string()),
                None => (fname.clone(), "0.0.0".to_string()),
            };
            let path = entry.path().to_string_lossy().to_string();
            if name.to_lowercase().contains(&pat) || path.to_lowercase().contains(&pat) {
                results.push(json!({"name": name, "version": version, "path": path}));
            }
        }
    }
    Ok(json!({
        "pattern": p.pattern,
        "total": results.len(),
        "results": results,
    }))
}

// ============================================================================
// defs.search (M5) — text/regex/fuzzy search across loaded definitions
// ============================================================================

#[derive(Deserialize, Default)]
struct DefsSearchParams {
    pattern: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    regex: bool,
    #[serde(default)]
    fuzzy: bool,
    #[serde(default)]
    top: Option<String>,
    #[serde(default)]
    limit: usize,
}

pub fn handle_defs_search(params: Option<Value>) -> RpcResult {
    let p: DefsSearchParams = parse_or_default(params)?;
    let kind = match p.kind.as_deref() {
        None => None,
        Some("component") => Some(SearchKind::Component),
        Some("module") => Some(SearchKind::Module),
        Some("interface") => Some(SearchKind::Interface),
        Some("enum") => Some(SearchKind::Enum),
        Some("instance") => Some(SearchKind::Instance),
        Some(other) => {
            return Err(JsonRpcError::custom(
                -32602,
                &format!(
                    "defs.search: unknown kind '{}', expected one of component|module|interface|enum|instance",
                    other
                ),
            ));
        }
    };
    let inputs = SearchInputs {
        pattern: p.pattern,
        kind,
        regex: p.regex,
        fuzzy: p.fuzzy,
        top: p.top,
        limit: p.limit,
        libs: Vec::new(),
    };
    let hits = walk_defs(&inputs, None)
        .map_err(|e| JsonRpcError::custom(-32603, &format!("defs.search: {}", e)))?;
    let count = hits.len();
    let results: Vec<Value> = hits
        .into_iter()
        .map(|h| {
            let mut v = json!({
                "kind": h.kind,
                "name": h.name,
                "uri": h.uri,
            });
            if let Some(c) = h.class {
                v["class"] = json!(c);
            }
            v
        })
        .collect();
    Ok(json!({
        "pattern": inputs.pattern,
        "kind": inputs.kind.map(|k| format!("{:?}", k).to_lowercase()),
        "regex": inputs.regex,
        "fuzzy": inputs.fuzzy,
        "count": count,
        "results": results,
    }))
}

// ============================================================================
// defs.query (M5 PR#2) — structured DSL query
// ============================================================================

#[derive(Deserialize, Default)]
struct DefsQueryParams {
    expr: String,
    #[serde(default)]
    limit: usize,
}

pub fn handle_defs_query(params: Option<Value>) -> RpcResult {
    let p: DefsQueryParams = parse_or_default(params)?;
    let query = crate::query_api::compile(&p.expr)
        .map_err(|e| JsonRpcError::custom(-32602, &format!("defs.query: {}", e)))?;
    let inputs = SearchInputs {
        pattern: String::new(),
        kind: None,
        regex: false,
        fuzzy: false,
        top: None,
        limit: p.limit,
        libs: Vec::new(),
    };
    let hits = walk_defs(&inputs, Some(&query))
        .map_err(|e| JsonRpcError::custom(-32603, &format!("defs.query: {}", e)))?;
    let count = hits.len();
    let results: Vec<Value> = hits
        .into_iter()
        .map(|h| {
            let mut v = json!({"kind": h.kind, "name": h.name, "uri": h.uri});
            if let Some(c) = h.class {
                v["class"] = json!(c);
            }
            v
        })
        .collect();
    Ok(json!({
        "expr": p.expr,
        "count": count,
        "results": results,
    }))
}

// ============================================================================
// export (M5 PR#3) — text/JSON/CSV netlist, BOM, SPICE
// ============================================================================

#[derive(Deserialize, Default)]
struct ExportRpcParams {
    /// "netlist" | "bom" | "spice"
    #[serde(default)]
    kind: String,
    /// Source .mc file path
    entry: String,
    /// Top module name (optional; defaults to first module)
    #[serde(default)]
    top: Option<String>,
    /// "text" | "json" | "csv" — defaults to "text"
    #[serde(default)]
    format: Option<String>,
    /// Library names to load
    #[serde(default)]
    libs: Vec<String>,
}

pub fn handle_export(params: Option<Value>) -> RpcResult {
    let p: ExportRpcParams = parse_or_default(params)?;
    let args = crate::cli::ExportArgs {
        kind: match p.kind.as_str() {
            "bom" => crate::cli::ExportKind::Bom,
            "spice" => crate::cli::ExportKind::Spice,
            "kicad" | "kicad-netlist" => crate::cli::ExportKind::KiCad,
            _ => crate::cli::ExportKind::Netlist,
        },
        file: p.entry,
        top: p.top,
        lib: p.libs,
        format: match p.format.as_deref() {
            Some("json") => crate::cli::OutputFormat::Json,
            Some("json-pretty") => crate::cli::OutputFormat::JsonPretty,
            Some("yaml") => crate::cli::OutputFormat::Yaml,
            Some("csv") => crate::cli::OutputFormat::Csv,
            _ => crate::cli::OutputFormat::Text,
        },
        json: p.format.as_deref() == Some("json"),
        output: None,
    };
    let (tree, table) = crate::export_api::build_tree(&args.file, args.top.as_deref(), &args.lib)
        .map_err(|e| JsonRpcError::custom(-32603, &format!("export: {}", e)))?;
    let top = args.top.clone().unwrap_or_else(|| "?".to_string());
    // Convert local cli enums → u8 tags for export_api.
    let kind_tag = match args.kind {
        crate::cli::ExportKind::Netlist => 0u8,
        crate::cli::ExportKind::Bom => 1u8,
        crate::cli::ExportKind::KiCad => 3u8,
        crate::cli::ExportKind::Spice => 2u8,
    };
    let format_tag = match args.format {
        crate::cli::OutputFormat::Text => 0u8,
        crate::cli::OutputFormat::Json => 1u8,
        crate::cli::OutputFormat::JsonPretty => 2u8,
        crate::cli::OutputFormat::Yaml => 3u8,
        crate::cli::OutputFormat::Csv => 4u8,
    };
    let (raw_text, items, count) =
        crate::export_api::build_payload(&tree, &table, &top, kind_tag, format_tag);
    let kind_str = match kind_tag {
        1 => "bom",
        2 => "spice",
        _ => "netlist",
    };
    let _ = raw_text; // raw artifact; for RPC we return structured items
    Ok(json!({
        "kind": kind_str,
        "format": p.format.unwrap_or_else(|| "text".into()),
        "count": count,
        "items": items,
    }))
}

/// Resolve an installed library directory under the system root.
/// Flat layout: checks `<root>/<name>` (built-in) and `<root>/<name>@<version>` (3rd-party).
fn resolve_installed_lib_dir(name: &str) -> Option<PathBuf> {
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

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
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
        _ => return Err(JsonRpcError::custom(32102, "unknown workspace kind")),
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
        .ok_or_else(|| JsonRpcError::custom(32105, "no .mc entry found"))
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

fn collect_pass1(_uri: &str, include_system: bool) -> Value {
    let all_modules = collect_definitions(crate::mcb_iter_modules());
    let all_components = collect_definitions(crate::mcb_iter_components());
    let all_interfaces = collect_definitions(crate::mcb_iter_interfaces());
    let all_enums = collect_definitions(crate::mcb_iter_enums());
    let all_ports = crate::mcb_iter_ports();

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
    /// Inline source content (M6). When set, loaded from memory — no disk I/O.
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    libs: Vec<String>,
    #[serde(default)]
    strict: bool,
    #[serde(default)]
    errors_only: bool,
}

/// Overlay URI used when `content` is provided — virtual file, never touches disk.
const CHECK_OVERLAY_URI: &str = "/mcc/check.mc";

pub fn handle_check(params: Option<Value>) -> RpcResult {
    let p: CheckRpcParams = parse_or_default(params)?;
    load_libs_rpc(&p.libs);

    // ── Mode A: inline content (AI dry-run) (M6) ──
    if let Some(content) = &p.content {
        let uri = McURI::from(CHECK_OVERLAY_URI);
        crate::mcc_load_from_string(&uri, content);

        let raw = crate::mcc_diagnose_all();
        let diags: Vec<Value> = raw
            .iter()
            .map(|d| {
                let sev = match d.level {
                    crate::DiagnosticLevel::Error => "error",
                    crate::DiagnosticLevel::Warning => "warning",
                    crate::DiagnosticLevel::Info => "info",
                    crate::DiagnosticLevel::Hint => "hint",
                };
                json!({
                    "code": d.code,
                    "severity": sev,
                    "message": d.msg,
                    "location": {
                        "file": d.loc.uri,
                        "line": d.loc.row,
                        "column": d.loc.col,
                        "pos": d.loc.pos,
                        "len": d.loc.len,
                    }
                })
            })
            .collect();

        let errors = diags.iter().filter(|d| d["severity"] == "error").count();
        let warnings = diags.iter().filter(|d| d["severity"] == "warning").count();

        return Ok(json!({
            "summary": { "errors": errors, "warnings": warnings },
            "diagnostics": diags,
        }));
    }

    // ── Mode B/C/D: disk file / project / workspace ──
    let (id, kind, _) = crate::workspace_info();
    if kind == "Anonymous" {
        let entry = p
            .entry
            .as_deref()
            .ok_or_else(|| JsonRpcError::custom(-32602, "check: need <entry> or <content>"))?;

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

// ============================================================================
// Refs (M6)
// ============================================================================

pub fn handle_refs(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize)]
    struct RefsParams {
        name: String,
    }

    let p: RefsParams = parse_strict(params)?;
    let refs = crate::mcb_get_refs(&p.name);

    let items: Vec<Value> = refs
        .iter()
        .map(|(uri, scope, span)| {
            json!({
                "uri": uri,
                "scope": scope,
                "pos": span.start,
                "end": span.end,
            })
        })
        .collect();

    Ok(json!({ "name": p.name, "count": items.len(), "refs": items }))
}

// ============================================================================
// ERC — Electrical Rule Check (M6)
// ============================================================================

pub fn handle_erc(_params: Option<Value>) -> RpcResult {
    run_erc()
}

/// Run Pass2 ERC: single-point nets, unconnected ports, net stats.
fn run_erc() -> RpcResult {
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
                    crate::core::common::IOType::Out
                        | crate::core::common::IOType::InOut
                        | crate::core::common::IOType::Power
                        | crate::core::common::IOType::Analog
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
    JsonRpcError::custom(32100, &format!("io error: {e}"))
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

fn resolve_project_entry(_name: &str, entry: Option<&str>) -> Result<PathBuf, JsonRpcError> {
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
        return Err(JsonRpcError::custom(32102, "workspace not found"));
    }
    Ok(())
}

fn resolve_lib_root(name: &str) -> Result<PathBuf, JsonRpcError> {
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
    #[serde(rename = "type")]
    type_filter: Option<String>,
    top: Option<String>,
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
// Show helpers (shared across drill-down handlers)
// ============================================================================

/// Find a definition by name across all four kinds.
fn find_def_by_name(name: &str) -> Option<(crate::McCMIE, String)> {
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
fn pins_json(pins: &crate::McPins) -> Value {
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

fn pinport_json(v: &crate::McPinPort) -> Value {
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

fn inst_kind_class(inst: &crate::McInstance) -> (&'static str, String) {
    match inst {
        crate::McInstance::Component(c) => ("component", c.name.to_string()),
        crate::McInstance::Module(m) => ("module", m.name.to_string()),
        crate::McInstance::Label(l) => ("label", l.clone()),
        crate::McInstance::Interface(i) => ("interface", i.name.to_string()),
        crate::McInstance::Bus(b) => ("bus", b.name().to_string()),
        crate::McInstance::BusRef { component, bus } => ("busref", format!("{component}.{bus}")),
        crate::McInstance::List(l) => ("list", l.name().to_string()),
    }
}

fn attrval_json(v: &crate::McAttrVal) -> Value {
    match v {
        crate::McAttrVal::AttrLiteral(crate::McLiteral::String(s)) => json!(s.value),
        other => json!(format!("{:?}", other)),
    }
}

// ============================================================================
// Show — missing container handlers
// ============================================================================

pub fn handle_show_all(_params: Option<Value>) -> RpcResult {
    let comps = crate::mcb_iter_components();
    let mods = crate::mcb_iter_modules();
    let ifaces = crate::mcb_iter_interfaces();
    let enums = crate::mcb_iter_enums();

    Ok(json!({
        "type": "all",
        "component_count": comps.len(),
        "component_list": comps.iter().map(|(n,_)| n).collect::<Vec<_>>(),
        "module_count": mods.len(),
        "module_list": mods.iter().map(|(n,_)| n).collect::<Vec<_>>(),
        "interface_count": ifaces.len(),
        "interface_list": ifaces.iter().map(|(n,_)| n).collect::<Vec<_>>(),
        "enum_count": enums.len(),
        "enum_list": enums.iter().map(|(n,_)| n).collect::<Vec<_>>(),
    }))
}

pub fn handle_show_file(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_or_default(params)?;
    let file = p.name.unwrap_or_default();

    // Load the file if provided
    if !file.is_empty() {
        let uri = McURI::from(file.as_str());
        crate::mcc_load_project(&uri);
    }

    let comps = crate::mcb_iter_components();
    let mods = crate::mcb_iter_modules();
    let ifaces = crate::mcb_iter_interfaces();
    let enums = crate::mcb_iter_enums();

    Ok(json!({
        "type": "file",
        "file": file,
        "component_count": comps.len(),
        "component_list": comps.iter().map(|(n,_)| n).collect::<Vec<_>>(),
        "module_count": mods.len(),
        "module_list": mods.iter().map(|(n,_)| n).collect::<Vec<_>>(),
        "interface_count": ifaces.len(),
        "interface_list": ifaces.iter().map(|(n,_)| n).collect::<Vec<_>>(),
        "enum_count": enums.len(),
        "enum_list": enums.iter().map(|(n,_)| n).collect::<Vec<_>>(),
    }))
}

pub fn handle_show_files(_params: Option<Value>) -> RpcResult {
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct FileInfo {
        component_count: usize,
        module_count: usize,
        interface_count: usize,
        enum_count: usize,
    }

    let mut files: BTreeMap<String, FileInfo> = BTreeMap::new();
    for (_, uri) in crate::mcb_iter_components() {
        files.entry(uri).or_default().component_count += 1;
    }
    for (_, uri) in crate::mcb_iter_modules() {
        files.entry(uri).or_default().module_count += 1;
    }
    for (_, uri) in crate::mcb_iter_interfaces() {
        files.entry(uri).or_default().interface_count += 1;
    }
    for (_, uri) in crate::mcb_iter_enums() {
        files.entry(uri).or_default().enum_count += 1;
    }

    let items: Vec<Value> = files
        .into_iter()
        .map(|(uri, info)| {
            json!({
                "uri": uri,
                "component_count": info.component_count,
                "module_count": info.module_count,
                "interface_count": info.interface_count,
                "enum_count": info.enum_count,
            })
        })
        .collect();

    Ok(json!({ "type": "files", "count": items.len(), "files": items }))
}

pub fn handle_show_enum_list(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_or_default(params)?;
    if let Some(file) = &p.file {
        let uri = McURI::from(file.as_str());
        crate::mcc_load_project(&uri);
    }
    let enums = crate::mcb_iter_enums();
    let names: Vec<String> = enums.iter().map(|(n, _)| n.clone()).collect();
    Ok(json!({ "type": "enum", "count": names.len(), "list": names }))
}

pub fn handle_show_enum(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_strict(params)?;
    let name = p
        .name
        .as_ref()
        .ok_or_else(|| JsonRpcError::custom(-32602, "show.enum: need to specify name"))?;

    let (cmie, uri) = find_def_by_name(name)
        .ok_or_else(|| JsonRpcError::custom(-32003, &format!("enum not found: {name}")))?;

    match cmie {
        crate::McCMIE::Enum(en) => {
            let values: Vec<String> = en.values.iter().map(|v| v.name.to_string()).collect();
            Ok(json!({
                "name": name,
                "uri": uri,
                "value_count": values.len(),
                "values": values,
            }))
        }
        _ => Err(JsonRpcError::custom(
            -32002,
            &format!("'{name}' is not an Enum"),
        )),
    }
}

// ============================================================================
// Show — drill-down handlers
// ============================================================================

pub fn handle_show_pins(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_strict(params)?;
    let name = p
        .name
        .as_ref()
        .ok_or_else(|| JsonRpcError::custom(-32602, "show.pins: need to specify name"))?;

    let (cmie, _) = find_def_by_name(name)
        .ok_or_else(|| JsonRpcError::custom(-32003, &format!("entity not found: {name}")))?;

    let pins = match &cmie {
        crate::McCMIE::Component(c) => &c.pins,
        crate::McCMIE::Interface(i) => &i.pins,
        _ => {
            return Err(JsonRpcError::custom(
                -32002,
                &format!("'{name}' does not have pins (only components and interfaces do)"),
            ))
        }
    };
    let mut data = pins_json(pins);
    data["name"] = json!(name);
    Ok(data)
}

pub fn handle_show_ports(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_strict(params)?;
    let name = p
        .name
        .as_ref()
        .ok_or_else(|| JsonRpcError::custom(-32602, "show.ports: need to specify name"))?;

    let (cmie, _) = find_def_by_name(name)
        .ok_or_else(|| JsonRpcError::custom(-32003, &format!("entity not found: {name}")))?;

    let module = match &cmie {
        crate::McCMIE::Module(m) => m,
        _ => {
            return Err(JsonRpcError::custom(
                -32002,
                &format!("'{name}' is not a Module"),
            ))
        }
    };
    let ports: Vec<Value> = module
        .insts
        .iter_ports()
        .map(|(pname, io)| json!({ "name": pname, "iotype": format!("{:?}", io) }))
        .collect();
    Ok(json!({ "name": name, "port_count": ports.len(), "ports": ports }))
}

pub fn handle_show_ports_list(_params: Option<Value>) -> RpcResult {
    let ports: Vec<Value> = crate::mcb_iter_ports()
        .into_iter()
        .map(|(name, iotype, module, uri)| {
            json!({ "name": name, "iotype": iotype, "module": module, "uri": uri })
        })
        .collect();
    Ok(json!({ "type": "port", "count": ports.len(), "ports": ports }))
}

pub fn handle_show_labels(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_strict(params)?;
    let name = p
        .name
        .as_ref()
        .ok_or_else(|| JsonRpcError::custom(-32602, "show.labels: need to specify name"))?;

    let (cmie, _) = find_def_by_name(name)
        .ok_or_else(|| JsonRpcError::custom(-32003, &format!("entity not found: {name}")))?;

    let module = match &cmie {
        crate::McCMIE::Module(m) => m,
        _ => {
            return Err(JsonRpcError::custom(
                -32002,
                &format!("'{name}' is not a Module"),
            ))
        }
    };
    let labels: Vec<String> = module
        .insts
        .iter()
        .filter(|(_, inst)| matches!(inst, crate::McInstance::Label(_)))
        .map(|(n, _)| n.to_string())
        .collect();
    Ok(json!({ "name": name, "label_count": labels.len(), "labels": labels }))
}

pub fn handle_show_instances(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_strict(params)?;
    let name = p
        .name
        .as_ref()
        .ok_or_else(|| JsonRpcError::custom(-32602, "show.instances: need to specify name"))?;

    let (cmie, _) = find_def_by_name(name)
        .ok_or_else(|| JsonRpcError::custom(-32003, &format!("entity not found: {name}")))?;

    let insts = match &cmie {
        crate::McCMIE::Component(c) => &c.insts,
        crate::McCMIE::Module(m) => &m.insts,
        _ => {
            return Err(JsonRpcError::custom(
                -32002,
                &format!("'{name}' does not have instances (only components and modules do)"),
            ))
        }
    };
    let items: Vec<Value> = insts
        .iter()
        .filter_map(|(n, inst)| {
            let (kind, class) = inst_kind_class(inst);
            if let Some(ref t) = p.type_filter {
                if !kind.eq_ignore_ascii_case(t) {
                    return None;
                }
            }
            Some(json!({ "name": n.to_string(), "kind": kind, "class": class }))
        })
        .collect();
    Ok(json!({ "name": name, "count": items.len(), "instances": items }))
}

pub fn handle_show_nets(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_strict(params)?;
    let name = p
        .name
        .as_ref()
        .ok_or_else(|| JsonRpcError::custom(-32602, "show.nets: need to specify name"))?;

    let top = p.top.as_ref().unwrap_or(name);
    let top_uri = crate::mcb_iter_modules()
        .iter()
        .find(|(n, _)| n == top)
        .map(|(_, u)| crate::McURI::from(u.as_str()))
        .unwrap_or_else(|| crate::McURI::from(top));
    let ident = crate::McIds::from(top.as_str());

    let inst = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::mcc_build(&ident, &top_uri)
    }))
    .map_err(|_| JsonRpcError::custom(-32002, "build panicked (engine Pass2 bug)"))?
    .map_err(|e| JsonRpcError::custom(-32002, &format!("build failed: {e}")))?;

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

    let items: Vec<Value> = nets
        .iter()
        .map(|(n, points)| json!({ "name": n, "points": points }))
        .collect();
    Ok(json!({ "name": name, "count": items.len(), "nets": items }))
}

pub fn handle_show_attrs(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_strict(params)?;
    let name = p
        .name
        .as_ref()
        .ok_or_else(|| JsonRpcError::custom(-32602, "show.attrs: need to specify name"))?;

    let (cmie, _) = find_def_by_name(name)
        .ok_or_else(|| JsonRpcError::custom(-32003, &format!("entity not found: {name}")))?;

    let attrs = match &cmie {
        crate::McCMIE::Component(c) => &c.attrs,
        crate::McCMIE::Interface(i) => &i.attrs,
        _ => {
            return Err(JsonRpcError::custom(
                -32002,
                &format!("'{name}' does not have attributes (only components and interfaces do)"),
            ))
        }
    };
    let items: Vec<Value> = attrs
        .iter()
        .map(|a| {
            let values: Vec<Value> = a.values.iter().map(attrval_json).collect();
            json!({ "no": a.no, "name": a.id.to_string(), "values": values })
        })
        .collect();
    Ok(json!({ "name": name, "count": items.len(), "attrs": items }))
}

pub fn handle_show_funcs(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_strict(params)?;
    let name = p
        .name
        .as_ref()
        .ok_or_else(|| JsonRpcError::custom(-32602, "show.funcs: need to specify name"))?;

    let (cmie, _) = find_def_by_name(name)
        .ok_or_else(|| JsonRpcError::custom(-32003, &format!("entity not found: {name}")))?;

    let funcs = match &cmie {
        crate::McCMIE::Component(c) => &c.funcs,
        crate::McCMIE::Module(m) => &m.funcs,
        _ => {
            return Err(JsonRpcError::custom(
                -32002,
                &format!("'{name}' does not have functions (only components and modules do)"),
            ))
        }
    };
    let items: Vec<Value> = funcs
        .iter()
        .map(|f| json!({ "name": f.name.to_string(), "params": f.params.names() }))
        .collect();
    Ok(json!({ "name": name, "count": items.len(), "funcs": items }))
}

pub fn handle_show_params(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_strict(params)?;
    let name = p
        .name
        .as_ref()
        .ok_or_else(|| JsonRpcError::custom(-32602, "show.params: need to specify name"))?;

    let (cmie, _) = find_def_by_name(name)
        .ok_or_else(|| JsonRpcError::custom(-32003, &format!("entity not found: {name}")))?;

    let param_names = match &cmie {
        crate::McCMIE::Component(c) => c.params.names(),
        crate::McCMIE::Module(m) => m.params.names(),
        crate::McCMIE::Interface(i) => i.params.names(),
        _ => {
            return Err(JsonRpcError::custom(
                -32002,
                &format!("'{name}' does not have params"),
            ))
        }
    };
    Ok(json!({ "name": name, "count": param_names.len(), "params": param_names }))
}

pub fn handle_show_roles(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_strict(params)?;
    let name = p
        .name
        .as_ref()
        .ok_or_else(|| JsonRpcError::custom(-32602, "show.roles: need to specify name"))?;

    let (cmie, _) = find_def_by_name(name)
        .ok_or_else(|| JsonRpcError::custom(-32003, &format!("entity not found: {name}")))?;

    let iface = match &cmie {
        crate::McCMIE::Interface(i) => i,
        _ => {
            return Err(JsonRpcError::custom(
                -32002,
                &format!("'{name}' is not an Interface"),
            ))
        }
    };
    let items: Vec<Value> = iface
        .roles
        .iter()
        .map(|r| {
            json!({
                "name": r.name.to_string(),
                "pins": pins_json(&r.pins),
            })
        })
        .collect();
    Ok(json!({ "name": name, "count": items.len(), "roles": items }))
}

pub fn handle_show_values(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_strict(params)?;
    let name = p
        .name
        .as_ref()
        .ok_or_else(|| JsonRpcError::custom(-32602, "show.values: need to specify name"))?;

    let (cmie, _) = find_def_by_name(name)
        .ok_or_else(|| JsonRpcError::custom(-32003, &format!("entity not found: {name}")))?;

    let en = match &cmie {
        crate::McCMIE::Enum(e) => e,
        _ => {
            return Err(JsonRpcError::custom(
                -32002,
                &format!("'{name}' is not an Enum"),
            ))
        }
    };
    let values: Vec<String> = en.values.iter().map(|v| v.name.to_string()).collect();
    Ok(json!({ "name": name, "count": values.len(), "values": values }))
}

// ============================================================================
// Semantic data (sem tokens + symbols) for LSP
// ============================================================================

#[derive(Deserialize)]
struct SemParams {
    uri: String,
    content: Option<String>,
}

pub fn handle_sem(params: Option<Value>) -> RpcResult {
    let p: SemParams = parse_strict(params)?;
    let raw_uri = &p.uri;

    // If content is provided, parse from memory (for unsaved editor content)
    if let Some(ref content) = p.content {
        // ★ Fix: Ensure library context is loaded before parsing
        let mc_uri = McURI::from(raw_uri.as_str());
        ensure_library_loaded(&mc_uri);
        crate::builder::mcb_add_from_string(&mc_uri, content);
        crate::builder::mcb_parse_all_modules();
        // ★ Fix: Use canonicalized URI for lookup (same as what mcb_add_from_string uses)
        let canonical_uri = crate::builder::canonicalize_project_uri(&mc_uri);
        let result = try_lookup_sem(&[McURI::from(&canonical_uri)]);
        // ★ Fix: DON'T remove the entry - mcc_query needs it for goto_definition
        // crate::builder::workspace::WORKSPACE.mcodes.borrow().remove(&McURI::from(&canonical_uri));
        return result.ok_or_else(|| JsonRpcError::custom(32100, "parse from string failed"));
    }

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
                .ok_or_else(|| JsonRpcError::custom(32100, "file not found in workspace"));
        }
    }

    result.ok_or_else(|| JsonRpcError::custom(32100, "file not found in workspace"))
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
fn find_project_root(file_path: &Path) -> PathBuf {
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
fn ensure_library_loaded(file_uri: &McURI) {
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
fn extract_lib_dependencies(contents: &str) -> Option<Vec<String>> {
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
fn classify_token_by_symbol(
    lex_type: i16,
    position: usize,
    length: usize,
    lapper: &crate::ast::ast_semantic::SymbolRangeLapper,
) -> i16 {
    // Only re-classify identifiers (lexer marks them as KEYWORD=13 or NONE=255)
    if lex_type != 13 && lex_type != 255 {
        return lex_type;
    }

    let token_end = position + length;
    let token_start = position;

    // Try symbol lapper
    if lapper.len() > 0 {
        for interval in lapper.iter() {
            let sym_start = interval.start;
            let sym_stop = interval.stop;
            if token_start < sym_stop && token_end > sym_start {
                use crate::ast::ast_semantic::SymbolType;
                if matches!(&interval.val, SymbolType::ClassDefinition(_)) {
                    return 3; // CLASS
                }
                if matches!(&interval.val, SymbolType::DeclareClass(_)) {
                    return 2; // TYPE
                }
                if matches!(&interval.val, SymbolType::DeclareInstance(_)) {
                    return 4; // FUNCTION
                }
                if matches!(&interval.val, SymbolType::InstanceReference(_)) {
                    return 9; // VARIABLE
                }
            }
        }
        return lex_type;
    }

    // Fallback: language keywords stay as KEYWORD, all other identifiers become VARIABLE
    // The actual keyword check will be done in mcext with the live document content
    lex_type
}

/// Try to find semantic data for any of the candidate URIs
fn try_lookup_sem(candidates: &[McURI]) -> Option<Value> {
    let binding = crate::builder::workspace::WORKSPACE.mcodes.borrow();
    for mc_uri in candidates {
        if let Some(mcfile) = binding.get(mc_uri) {
            // Get raw tokens and symbol lapper for semantic re-classification
            let raw_tokens: Vec<(i16, i32, i32)> = mcfile
                .tokens
                .lock()
                .map(|t: std::sync::MutexGuard<'_, crate::McSemTokens>| {
                    t.iter()
                        .map(|tok| (tok.type_, tok.position, tok.length))
                        .collect()
                })
                .unwrap_or_default();

            let symbols = mcfile
                .symbols
                .lock()
                .ok()
                .map(|s| s.symbol_lapper.clone())
                .unwrap_or_else(|| crate::ast::ast_semantic::SymbolRangeLapper::new(vec![]));

            // Re-classify tokens using symbol lapper
            let tokens: Vec<serde_json::Value> = raw_tokens
                .iter()
                .map(|(lex_type, position, length)| {
                    let sem_type = classify_token_by_symbol(
                        *lex_type,
                        *position as usize,
                        *length as usize,
                        &symbols,
                    );
                    json!({
                        "type": sem_type,
                        "position": position,
                        "length": length,
                    })
                })
                .collect();

            // Compute stable result_id: hash of (token_count, first_pos, last_pos)
            let result_id = if tokens.is_empty() {
                None
            } else {
                let count = tokens.len();
                let first_pos = tokens[0]
                    .get("position")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let last_pos = tokens
                    .last()
                    .and_then(|v| v.get("position").and_then(|v| v.as_i64()))
                    .unwrap_or(0);
                Some(format!("{}-{}-{}", count, first_pos, last_pos))
            };

            let symbols = mcfile
                .symbols
                .lock()
                .map(|s| crate::ast::ast_semantic::symbol_table_to_json(&s, mc_uri))
                .unwrap_or_else(|_| serde_json::json!({}));

            return Some(json!({
                "tokens": tokens,
                "symbols": symbols,
                "result_id": result_id,
            }));
        }
    }
    None
}

// ============================================================================
// Report (M5b)
// ============================================================================

pub fn handle_report(_params: Option<Value>) -> RpcResult {
    let comps = crate::mcb_iter_components();
    let mods = crate::mcb_iter_modules();
    let ifaces = crate::mcb_iter_interfaces();
    let enums = crate::mcb_iter_enums();

    let mut by_prefix: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    for (name, _) in &comps {
        let prefix = name
            .chars()
            .next()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "?".into());
        *by_prefix.entry(prefix).or_default() += 1;
    }

    Ok(json!({
        "summary": {
            "component_count": comps.len(),
            "module_count": mods.len(),
            "interface_count": ifaces.len(),
            "enum_count": enums.len(),
        },
        "components_by_prefix": by_prefix,
        "components": comps.iter().take(20).map(|(n, u)| json!({"name": n, "uri": u})).collect::<Vec<_>>(),
        "modules": mods.iter().take(10).map(|(n, u)| json!({"name": n, "uri": u})).collect::<Vec<_>>(),
        "interfaces": ifaces.iter().take(10).map(|(n, u)| json!({"name": n, "uri": u})).collect::<Vec<_>>(),
        "enums": enums.iter().map(|(n, u)| json!({"name": n, "uri": u})).collect::<Vec<_>>(),
    }))
}

// ============================================================================
// Convert (M5b)
// ============================================================================

pub fn handle_convert(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize)]
    struct ConvertParams {
        entry: String,
        #[serde(default)]
        format: Option<String>,
    }
    let p: ConvertParams = parse_strict(params)?;
    // Delegate to parse — convert is a thin wrapper
    let bp = json!({ "entry": p.entry, "format": p.format.unwrap_or_else(|| "json".into()), "include_system": false });
    handle_parse(Some(bp))
}

// ============================================================================
// Def (M6)
// ============================================================================

/// Handle def RPC — go-to-definition for a symbol.
pub fn handle_def(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize)]
    struct DefParams {
        name: String,
    }

    let p: DefParams = parse_strict(params)?;
    let name = &p.name;

    let iterators: [(&str, Vec<(String, String)>); 4] = [
        ("component", crate::mcb_iter_components()),
        ("module", crate::mcb_iter_modules()),
        ("interface", crate::mcb_iter_interfaces()),
        ("enum", crate::mcb_iter_enums()),
    ];

    for (kind, items) in &iterators {
        if let Some((matched, uri)) = items.iter().find(|(n, _)| n == name) {
            let ident = crate::McIds::from(matched.as_str());
            let uri_obj = crate::McURI::from(uri.as_str());

            return match crate::get_def(&ident, &uri_obj) {
                Some(crate::McCMIE::Component(c)) => Ok(json!({
                    "kind": "component", "name": matched, "uri": uri,
                    "pin_count": c.pins.pins.len(),
                })),
                Some(crate::McCMIE::Module(m)) => Ok(json!({
                    "kind": "module", "name": matched, "uri": uri,
                    "instance_count": m.insts.iter().count(),
                })),
                Some(crate::McCMIE::Interface(i)) => Ok(json!({
                    "kind": "interface", "name": matched, "uri": uri,
                    "pin_count": i.pins.pins.len(),
                })),
                Some(crate::McCMIE::Enum(e)) => Ok(json!({
                    "kind": "enum", "name": matched, "uri": uri,
                    "value_count": e.values.len(),
                })),
                None => Ok(json!({ "kind": kind, "name": matched, "uri": uri })),
            };
        }
    }

    Err(JsonRpcError::custom(
        -32003,
        &format!("definition not found: {name}"),
    ))
}

// ============================================================================
// Capabilities (M6)
// ============================================================================

/// Handle capabilities RPC — self-describing API for AI discovery.
pub fn handle_caps(_params: Option<Value>) -> RpcResult {
    Ok(json!({
        "server": "mcc",
        "version": env!("CARGO_PKG_VERSION"),
        "schema_version": 1,
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
            "show_drilldown": true,
            "show_global_ports": true,
            "show_files": true,
            "parse_code": true,
            "parse_directory": true,
            "overlay_dry_run": true,
            "simulation": false,
            "pcb_export": false,
            "erc": true,
            "semantic_lint": false
        },
        "rpc_methods": [
            "server.info", "server.methods",
            "parse", "check", "build.full", "extract",
            "show.all", "show.file", "show.files",
            "show.component", "show.component.list",
            "show.module", "show.module.list",
            "show.interface", "show.interface.list",
            "show.enum", "show.enum.list",
            "show.net", "show.net.list",
            "show.pins", "show.ports", "show.ports.list",
            "show.labels", "show.instances", "show.nets",
            "show.attrs", "show.funcs", "show.params",
            "show.roles", "show.values",
            "lib.list", "lib.info", "lib.load", "lib.unload",
            "lib.install", "lib.uninstall", "lib.search",
            "defs.search", "defs.query",
            "export", "explain", "def", "erc", "refs", "caps",
            "trace.set", "trace.get",
            "sem", "diagnostics",
            "project_symbols", "set_project_root", "set_system_root",
            "init", "load_project", "add_file", "remove_file"
        ]
    }))
}

// ============================================================================
// Explain (M6)
// ============================================================================

/// Handle explain RPC — look up error code descriptions.
pub fn handle_explain(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize, Default)]
    struct ExplainParams {
        code: Option<u32>,
    }

    let p: ExplainParams = parse_or_default(params)?;

    match p.code {
        Some(code) => match crate::error_codes::describe(code) {
            Some(info) => Ok(json!({
                "code": info.code,
                "name": info.name,
                "description": info.description,
            })),
            None => Err(JsonRpcError::custom(
                -32003,
                &format!("unknown error code: {code}"),
            )),
        },
        None => {
            let all = crate::error_codes::all_codes();
            let items: Vec<Value> = all
                .iter()
                .map(|e| {
                    json!({
                        "code": e.code,
                        "name": e.name,
                        "description": e.description,
                    })
                })
                .collect();
            Ok(json!({ "codes": items }))
        }
    }
}

/// Handle diagnostics RPC - return parse/semantic diagnostics for a file
pub fn handle_diagnostics(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize)]
    struct DiagnosticsParams {
        uri: String,
    }

    let p: DiagnosticsParams = parse_strict(params)?;
    let raw_uri = McURI::from(p.uri.as_str());
    // Canonicalize URI to match the keys used when storing diagnostics
    // (mcb_add_from_string and all diagnostic_log calls use canonical URIs)
    let mc_uri = McURI::from(crate::builder::canonicalize_project_uri(&raw_uri));

    tracing::info!(target: "mcc::rpc", "handle_diagnostics: raw={} canonical={}", raw_uri, mc_uri);

    // Get all diagnostics for this file
    let diagnostics = crate::mcc_diagnose(&mc_uri);

    tracing::info!(target: "mcc::rpc", "handle_diagnostics: found {} diagnostics", diagnostics.len());

    // Convert to JSON
    let diags: Vec<serde_json::Value> = diagnostics
        .iter()
        .map(|d| {
            serde_json::json!({
                "code": d.code,
                "level": format!("{:?}", d.level).to_lowercase(),
                "message": d.msg,
                "location": {
                    "pos": d.loc.pos,
                    "len": d.loc.len,
                    "line": d.loc.row,
                    "column": d.loc.col,
                }
            })
        })
        .collect();

    Ok(serde_json::json!({ "diagnostics": diags }))
}

/// Handle project_symbols RPC - return project-wide symbols (components, interfaces, enums, modules, enum_values)
pub fn handle_project_symbols(_params: Option<Value>) -> RpcResult {
    use crate::builder::main::{
        mcb_iter_components, mcb_iter_enum_values, mcb_iter_enums_with_span, mcb_iter_interfaces,
        mcb_iter_modules,
    };

    let components: Vec<serde_json::Value> = mcb_iter_components()
        .into_iter()
        .map(|(name, uri)| serde_json::json!({ "name": name, "uri": uri }))
        .collect();

    let interfaces: Vec<serde_json::Value> = mcb_iter_interfaces()
        .into_iter()
        .map(|(name, uri)| serde_json::json!({ "name": name, "uri": uri }))
        .collect();

    let enums: Vec<serde_json::Value> = mcb_iter_enums_with_span()
        .into_iter()
        .map(|(name, uri, span)| {
            serde_json::json!({
                "name": name,
                "uri": uri,
                "span": [span[0], span[1]],
            })
        })
        .collect();

    let modules: Vec<serde_json::Value> = mcb_iter_modules()
        .into_iter()
        .map(|(name, uri)| serde_json::json!({ "name": name, "uri": uri }))
        .collect();

    // Per-value rows so the extension can do (class, value) -> uri+span lookup
    // for F12 on the value half of `PKG.SOP8`.
    let enum_values: Vec<serde_json::Value> = mcb_iter_enum_values()
        .into_iter()
        .map(|(class, name, uri, span)| {
            serde_json::json!({
                "class": class,
                "name": name,
                "uri": uri,
                "span": [span[0], span[1]],
            })
        })
        .collect();

    Ok(serde_json::json!({
        "components": components,
        "interfaces": interfaces,
        "enums": enums,
        "modules": modules,
        "enum_values": enum_values,
    }))
}

/// Handle set_project_root RPC - set project root path
pub fn handle_set_project_root(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize)]
    struct SetProjectRootParams {
        path: String,
    }

    let p: SetProjectRootParams = parse_strict(params)?;
    crate::mcc_set_project_root(std::path::Path::new(&p.path));
    Ok(serde_json::json!({ "ok": true }))
}

/// Handle set_system_root RPC - set system root path (for library resolution)
pub fn handle_set_system_root(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize)]
    struct SetSystemRootParams {
        path: String,
    }

    let p: SetSystemRootParams = parse_strict(params)?;
    crate::mcc_set_system_root(std::path::Path::new(&p.path));
    Ok(serde_json::json!({ "ok": true }))
}

/// Handle init RPC - initialize mcc system
pub fn handle_init(_params: Option<Value>) -> RpcResult {
    // Use mcc_init() (not mcc_init_no_lib) so that configured system libraries
    // (e.g. `mcode`, providing `enum PKG`) are loaded. The LSP client (mcext)
    // calls `init` on startup; using the no-lib variant here previously wiped
    // the mcode library that was loaded at server startup, which broke enum
    // reference resolution (e.g. goto-definition on `PKG.QFN20`).
    crate::mcc_init();
    Ok(serde_json::json!({ "ok": true }))
}

/// Handle load_project RPC - load entire project
pub fn handle_load_project(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize)]
    struct LoadProjectParams {
        entry: String,
    }

    let p: LoadProjectParams = parse_strict(params)?;
    let mc_uri = McURI::from(p.entry.as_str());
    crate::mcc_load_project(&mc_uri);
    Ok(serde_json::json!({ "ok": true }))
}

/// Handle add_file RPC - add a single file to project
pub fn handle_add_file(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize)]
    struct AddFileParams {
        uri: String,
    }

    let p: AddFileParams = parse_strict(params)?;
    crate::mcc_add(&McURI::from(p.uri.as_str()));
    Ok(serde_json::json!({ "ok": true }))
}

/// Handle remove_file RPC - remove a file from project
pub fn handle_remove_file(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize)]
    struct RemoveFileParams {
        uri: String,
    }

    let p: RemoveFileParams = parse_strict(params)?;
    crate::mcc_remove(&McURI::from(p.uri.as_str()));
    Ok(serde_json::json!({ "ok": true }))
}
