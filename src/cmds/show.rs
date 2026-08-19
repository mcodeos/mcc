// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc show` — Show detailed content of parsed definitions.
//!
//! Targets:
//!   * overview : `all` (layered by origin, `--scope`; -F anchors the file layer)
//!   * entity   : `component` / `module` / `interface` / `enum` (<name> required)
//!   * drill    : `pins` / `ports` / `labels` / `instances` / `nets` / `attrs`
//!                / `funcs` / `params` / `roles` / `values` / `net` (<name> = owning
//!                entity; funcs are referenced dot-qualified as `OWNER.FUNC` for
//!                `params` and `nets`)
//!   * debug    : `dump` / `lapper` / `ast`
//!
//! Top-level name lists live in `mcc list` (see cmds/list.rs).

use crate::output::compact;
use anyhow::{Context, Result};
use mcc::cli::{rpcclient::RpcClient, OutputFormat, ShowArgs, ShowScope, ShowTarget};
use mcc::{McIds, McURI};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tracing::error;

pub fn run(args: &ShowArgs) -> Result<()> {
    // Server path: only legacy container targets have RPC methods today
    // (server/local parity for the rest is tracked by roadmap M3). Everything
    // else falls through to local execution.
    if let Some(c) = RpcClient::probe() {
        if let Some((method, params)) = rpc_mapping(args) {
            match c.call(method, params) {
                Ok(result) => {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                    return Ok(());
                }
                Err(e) => {
                    tracing::debug!(target: "mcc::show", "RPC failed, using local mode: {}", e);
                }
            }
        }
    }

    run_local(args)
}

/// Map show targets to their RPC method + params. Returns `None` when the
/// command must fall through to local execution:
///   * `--filter` is set — RPC list methods don't apply filters (parity deferred).
///   * output format is `text` — RPC handlers only return JSON, so the aligned
///     tables / .mc-like dumps are rendered locally. This also makes the
///     default `-f text` output stable whether or not a server is running.
fn rpc_mapping(args: &ShowArgs) -> Option<(&'static str, Value)> {
    if mcc::cli::globals().format == OutputFormat::Text || args.filter.is_some() {
        return None;
    }
    match args.target {
        // ── overview ───────────────────────────────────────────────────────
        // show.all is local-only: the RPC handler has no --scope / -F concept
        // and would bypass the layered (file/use/system) filtering.
        ShowTarget::All => None,
        ShowTarget::Lapper | ShowTarget::Ast => {
            // local-only: read file, call internal sem, dump lapper / AST tree
            return None;
        }

        // ── entity detail (name required) ──────────────────────────────────
        ShowTarget::Component | ShowTarget::Module | ShowTarget::Interface | ShowTarget::Enum => {
            if args.name.is_none() {
                // name lists moved to `mcc list <kind>`; run_local prints the hint
                return None;
            }
            let m = match args.target {
                ShowTarget::Component => "show.component",
                ShowTarget::Module => "show.module",
                ShowTarget::Interface => "show.interface",
                ShowTarget::Enum => "show.enum",
                _ => unreachable!(),
            };
            Some((m, json!({ "name": args.name, "file": args.file })))
        }
        ShowTarget::Net => {
            if args.name.is_none() {
                // net list moved to `mcc list nets`; run_local prints the hint
                return None;
            }
            Some(("show.net", json!({ "name": args.name })))
        }
        ShowTarget::Dianlu => {
            // local-only: walks the Pass2 McModuleInst tree (sections render
            // from live object data, no RPC method exists)
            return None;
        }

        // ── drill-down ─────────────────────────────────────────────────────
        ShowTarget::Pins => drill_rpc("show.pins", args),
        ShowTarget::Ports => {
            if args.name.is_some() {
                drill_rpc("show.ports", args)
            } else {
                // port list moved to `mcc list ports`; run_local prints the hint
                None
            }
        }
        ShowTarget::Labels => drill_rpc("show.labels", args),
        ShowTarget::Instances => drill_rpc("show.instances", args),
        ShowTarget::Nets => drill_rpc("show.nets", args),
        ShowTarget::Attrs => drill_rpc("show.attrs", args),
        ShowTarget::Funcs => drill_rpc("show.funcs", args),
        ShowTarget::Params => drill_rpc("show.params", args),
        ShowTarget::Roles => drill_rpc("show.roles", args),
        ShowTarget::Values => drill_rpc("show.values", args),
    }
}

/// Build an RPC call for a drill-down target. All drill-down targets require
/// `name`; `--type` and `--top` are passed through when present.
fn drill_rpc(method: &'static str, args: &ShowArgs) -> Option<(&'static str, Value)> {
    let name = args.name.as_ref()?;
    let mut params = json!({ "name": name });
    if let Some(t) = &args.r#type {
        params["type"] = json!(t);
    }
    if let Some(t) = &mcc::cli::globals().top {
        params["top"] = json!(t);
    }
    Some((method, params))
}

fn run_local(args: &ShowArgs) -> Result<()> {
    prepare(args);

    let name = args.name.as_deref();
    match args.target {
        // ── overview / debug ───────────────────────────────────────────────
        ShowTarget::All => show_all(args),
        ShowTarget::Lapper => show_lapper(args),
        ShowTarget::Ast => show_ast(args),

        // ── entity detail (name required; lists moved to `mcc list`) ───────
        ShowTarget::Component => match name {
            None => need_list_hint(args, "component"),
            Some(n) => show_component(n, args),
        },
        ShowTarget::Module => match name {
            None => need_list_hint(args, "module"),
            Some(n) => show_module(n, args),
        },
        ShowTarget::Interface => match name {
            None => need_list_hint(args, "interface"),
            Some(n) => show_interface(n, args),
        },
        ShowTarget::Enum => match name {
            None => need_list_hint(args, "enum"),
            Some(n) => show_enum(n, args),
        },
        ShowTarget::Net => match name {
            None => need_list_hint(args, "nets"),
            Some(n) => show_net(n, args),
        },
        ShowTarget::Dianlu => show_dianlu(args),

        // ── drill-down ─────────────────────────────────────────────────────
        ShowTarget::Pins => drill_pins(require_name(args), args),
        ShowTarget::Ports => match name {
            None => need_list_hint(args, "ports"),
            Some(n) => drill_ports(n, args),
        },
        ShowTarget::Labels => drill_labels(require_name(args), args),
        ShowTarget::Instances => drill_instances(require_name(args), args),
        ShowTarget::Nets => drill_nets(require_name(args), args),
        ShowTarget::Attrs => drill_attrs(require_name(args), args),
        ShowTarget::Funcs => drill_funcs(require_name(args), args),
        ShowTarget::Params => drill_params(require_name(args), args),
        ShowTarget::Roles => drill_roles(require_name(args), args),
        ShowTarget::Values => drill_values(require_name(args), args),
    }
}

// ============================================================================
// Setup
// ============================================================================

/// One-shot environment setup: init engine, load `--lib` libraries, load the
/// target file. All handlers assume this ran, so none of them re-init.
///
/// A directory target is treated as project mode (mirrors `parse <dir>`):
/// `project.toml` provides the entry file and dependency libraries, or browse
/// mode selects the unique `.mc` file declaring `module main` when no
/// manifest exists.
///
/// The target path comes from `-F`, or from the positional argument for
/// targets that take no entity name ([`target_path`]).
fn prepare(args: &ShowArgs) {
    let file_opt = target_path(args);
    crate::cmds::manifest::init_local(file_opt, &mcc::cli::globals().lib);

    if let Some(f) = file_opt {
        if Path::new(f).is_dir() {
            // Directory target: unified project/browse-mode loading.
            if let Err(e) = crate::cmds::common::load_target(
                Some(f),
                mcc::cli::globals().top.as_deref(),
                mcc::cli::globals().entry.as_deref(),
            ) {
                error!(target: "mcc::show", "directory target: {:#}", e);
                std::process::exit(1);
            }
        } else {
            let actual = resolve_file(f);
            // Absolutize so the engine does not join a relative path onto the
            // project root (which would double the directory components).
            let path = if Path::new(&actual).is_absolute() {
                actual
            } else {
                std::env::current_dir()
                    .map(|c| c.join(&actual).to_string_lossy().to_string())
                    .unwrap_or(actual)
            };
            let uri = mcc::McURI::from(path.as_str());
            mcc::mcc_load_project(&uri);
        }
    }
}

/// Effective target path for file-based targets: `-F` wins; otherwise the
/// positional argument is the target for targets that take no entity name
/// (`show all` / `show dianlu`), mirroring `show ast` / `show lapper`.
fn target_path(args: &ShowArgs) -> Option<&str> {
    if args.file.is_some() {
        return args.file.as_deref();
    }
    match args.target {
        ShowTarget::All | ShowTarget::Dianlu => args.name.as_deref(),
        _ => None,
    }
}

fn require_name<'a>(args: &'a ShowArgs) -> &'a str {
    match args.name.as_deref() {
        Some(n) => n,
        None => {
            error!(target: "mcc::show", "'show {:?}' requires an entity name", args.target);
            std::process::exit(2);
        }
    }
}

/// Name lists moved to `mcc list`; a bare `show <target>` without a name
/// prints a hint instead of silently listing.
fn need_list_hint(args: &ShowArgs, list_kind: &str) -> Result<()> {
    error!(
        target: "mcc::show",
        "'show {:?}' requires an entity name\nto list {} names, use `mcc list {}`",
        args.target, list_kind, list_kind
    );
    std::process::exit(2);
}

/// Resolve a file path; if it doesn't exist, search by base name in the tree.
pub(crate) fn resolve_file(file: &str) -> String {
    if Path::new(file).exists() {
        return file.to_string();
    }
    let matches = find_files_with_name(file);
    match matches.len() {
        0 => {
            error!(target: "mcc::show", "file not found: {}", file);
            std::process::exit(1);
        }
        1 => matches[0].clone(),
        _ => {
            let list: Vec<String> = matches
                .iter()
                .enumerate()
                .map(|(i, p)| format!("  {}: {}", i + 1, p))
                .collect();
            error!(target: "mcc::show", "multiple files named '{}':\n{}", file, list.join("\n"));
            std::process::exit(1);
        }
    }
}

/// Search for files with the same name, recursively in common directories.
fn find_files_with_name(name: &str) -> Vec<String> {
    use std::fs;

    let file_name = Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(name);

    let mut matches = Vec::new();

    fn search_dir(dir: &Path, file_name: &str, matches: &mut Vec<String>, depth: usize) {
        if depth > 5 {
            return;
        }
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(fname) = path.file_name().and_then(|n| n.to_str()) {
                        if !fname.starts_with('.') && fname != "target" && fname != "node_modules" {
                            search_dir(&path, file_name, matches, depth + 1);
                        }
                    }
                } else if path.is_file() {
                    if let Some(fname) = path.file_name().and_then(|n| n.to_str()) {
                        if fname == file_name && fname.ends_with(".mc") {
                            if let Ok(canonical) = path.canonicalize() {
                                if let Some(p) = canonical.to_str() {
                                    matches.push(p.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    search_dir(Path::new("."), file_name, &mut matches, 0);
    matches
}

// ============================================================================
// Definition lookup
// ============================================================================

/// Find a definition by name across all kinds; returns its CMIE.
fn find_def(name: &str) -> Option<mcc::McCMIE> {
    let lists = [
        mcc::mcb_iter_components(),
        mcc::mcb_iter_modules(),
        mcc::mcb_iter_interfaces(),
        mcc::mcb_iter_enums(),
    ];
    for list in &lists {
        if let Some((n, u)) = list.iter().find(|(n, _)| n == name) {
            if let Some(cmie) =
                mcc::get_def(&mcc::McIds::from(n.as_str()), &mcc::McURI::from(u.as_str()))
            {
                return Some(cmie);
            }
        }
    }
    None
}

/// Find a component definition by name, bypassing the RefDefMap ambiguity
/// that arises when a component and an enum share the same name+URI
/// (e.g. `enum CAP` + `component CAP` in mcode/cap.mc, P0-3).
fn find_component_def(name: &str) -> Option<mcc::McCMIE> {
    for (n, u) in mcc::mcb_iter_components() {
        if n == name {
            if let Some(c) =
                mcc::get_component_def(&mcc::McIds::from(n.as_str()), &mcc::McURI::from(u.as_str()))
            {
                return Some(c);
            }
        }
    }
    None
}

fn def_or_exit(name: &str) -> mcc::McCMIE {
    match find_def(name) {
        Some(c) => c,
        None => {
            error!(target: "mcc::show", "definition not found: {}\nhint: load a file with -F, a library with --lib, or start a server", name);
            std::process::exit(1);
        }
    }
}

fn component_def_or_exit(name: &str) -> mcc::McCMIE {
    match find_component_def(name) {
        Some(c) => c,
        None => {
            error!(target: "mcc::show", "definition not found: {}\nhint: load a file with -F, a library with --lib, or start a server", name);
            std::process::exit(1);
        }
    }
}

/// Report that `<what>` is not applicable to the kind of `<name>`, then exit.
fn not_applicable(what: &str, name: &str) -> ! {
    error!(target: "mcc::show", "'{}' is not available for '{}'", what, name);
    std::process::exit(1);
}

// ============================================================================
// Containers: overview / list / detail
// ============================================================================

fn show_all(args: &ShowArgs) -> Result<()> {
    let target = target_path(args).map(resolve_file);
    let scopes = resolve_scopes(args.scope, target.is_some());

    let mut data = serde_json::Map::new();
    for s in &scopes {
        data.insert(
            scope_name(*s).to_string(),
            collect_scope(*s, target.as_deref()),
        );
    }
    // The target file path is kept under its own key so it does not collide
    // with the "file" layer collection above.
    if let Some(t) = &target {
        data.insert("target_file".to_string(), json!(t));
    }
    data.insert("type".to_string(), json!("layered_all"));
    output(&json!(data), args.span)
}

/// Resolve the `--scope` default policy shared by `show all` and `list all`:
///   * default is the `file` layer when a target file is present
///   * without a target file the `file` layer has nothing to match against,
///     so fall back to every loaded layer (keeps the overview role)
pub(crate) fn resolve_scopes(scope: Option<ShowScope>, has_target: bool) -> Vec<ShowScope> {
    let scope = scope.unwrap_or(ShowScope::File);
    match (scope, has_target) {
        (ShowScope::All, _) | (ShowScope::File, false) => {
            vec![ShowScope::File, ShowScope::Use, ShowScope::System]
        }
        (s, _) => vec![s],
    }
}

/// JSON tag used by [`render_layered_text`] to detect `show all` output.
const LAYERED_ALL_TYPE: &str = "layered_all";

fn scope_name(scope: ShowScope) -> &'static str {
    match scope {
        ShowScope::File => "file",
        ShowScope::Use => "use",
        ShowScope::System => "system",
        ShowScope::All => "all",
    }
}

/// Collect the definitions of one layer (file / use / system) from the loaded
/// tables, grouped into the same module/component/interface/enum lists that
/// the flat `show all` used to print.
fn collect_scope(scope: ShowScope, target: Option<&str>) -> Value {
    let mut components = Vec::new();
    let mut modules = Vec::new();
    let mut interfaces = Vec::new();
    let mut enums = Vec::new();
    for (n, u) in mcc::mcb_iter_components() {
        if classify_def_scope(&u, target) == scope {
            components.push(n);
        }
    }
    for (n, u) in mcc::mcb_iter_modules() {
        if classify_def_scope(&u, target) == scope {
            modules.push(n);
        }
    }
    for (n, u) in mcc::mcb_iter_interfaces() {
        if classify_def_scope(&u, target) == scope {
            interfaces.push(n);
        }
    }
    for (n, u) in mcc::mcb_iter_enums() {
        if classify_def_scope(&u, target) == scope {
            enums.push(n);
        }
    }
    json!({
        format!("module_list({})", modules.len()): modules,
        format!("component_list({})", components.len()): components,
        format!("interface_list({})", interfaces.len()): interfaces,
        format!("enum_list({})", enums.len()): enums,
    })
}

/// Classify a definition URI into a layer:
///   * `File`   — the definition lives in the -F target file
///   * `System` — the definition lives inside a loaded system library
///   * `Use`    — everything else (use-imported / project libraries)
pub(crate) fn classify_def_scope(uri: &str, target: Option<&str>) -> ShowScope {
    if let Some(t) = target {
        if uri_matches(uri, t) {
            return ShowScope::File;
        }
    }
    if is_system_uri(uri) {
        return ShowScope::System;
    }
    ShowScope::Use
}

/// True when `uri` belongs to a loaded system library (mcode or an installed
/// library resolved under the data root).
fn is_system_uri(uri: &str) -> bool {
    let path = std::path::Path::new(uri);
    mcc::mcb_loaded_libs()
        .iter()
        .any(|name| mcc::resolve_lib_root(name).is_some_and(|root| path.starts_with(&root)))
}

fn show_ast(args: &ShowArgs) -> Result<()> {
    let file_path = require_name(args);
    let path = Path::new(file_path);
    if !path.exists() {
        anyhow::bail!("file not found: {}", file_path);
    }
    let uri_str = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string();
    let mc_uri = McURI::from(uri_str.as_str());
    // Enable AST tree output (MCC_LOG_VISIT) from C engine
    if let Ok(mut trace) = mcc::get_runtime_trace().write() {
        trace.visit = Some(true);
    }
    mcc::set_trace_stdout_suppressed(false);
    // Reset AST visit flag so tree is printed for this invocation
    mcc::mcb_reset_ast_visit_flag();
    mcc::mcc_load_project(&mc_uri);
    Ok(())
}

fn show_lapper(args: &ShowArgs) -> Result<()> {
    let file_path = require_name(args);
    let path = Path::new(file_path);
    if !path.exists() {
        anyhow::bail!("file not found: {}", file_path);
    }
    let uri_str = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string();
    let mc_uri = McURI::from(uri_str.as_str());

    // Suppress AST tree printing during parsing
    mcc::set_trace_stdout_suppressed(true);

    // prepare() already called mcc_load_project. If the file is already loaded,
    // dump symbols directly. Otherwise, load and parse first.
    let is_text = matches!(mcc::cli::globals().format, OutputFormat::Text);
    if is_text {
        if let Some(text) = mcc::dump_symbols_f12_text(&mc_uri) {
            print!("{text}");
            return Ok(());
        }
    } else {
        if let Some(json_val) = mcc::dump_symbols_json(&mc_uri) {
            println!("{}", serde_json::to_string_pretty(&json_val)?);
            return Ok(());
        }
    }

    // Not loaded yet — load project and try again.
    // Unified with the shared init: the nearest project manifest
    // (project.toml / manifest.toml / mcc.toml) drives the dependency set,
    // and the project root is set so project-relative `use` paths resolve
    // correctly.
    let project_root = super::manifest::find_project_root(Some(file_path))
        .unwrap_or_else(|| path.parent().map(|p| p.to_path_buf()).unwrap_or_default());
    if !project_root.as_os_str().is_empty() {
        mcc::mcc_set_project_root(&project_root);
    }
    let libs = super::manifest::collect_libs(Some(&project_root), &[]);
    super::manifest::load_libs(&libs);
    mcc::mcc_load_project(&mc_uri);
    if is_text {
        if let Some(text) = mcc::dump_symbols_f12_text(&mc_uri) {
            print!("{text}");
            return Ok(());
        }
    } else {
        if let Some(json_val) = mcc::dump_symbols_json(&mc_uri) {
            println!("{}", serde_json::to_string_pretty(&json_val)?);
            return Ok(());
        }
    }

    // Fallback: send to RPC server
    let content =
        std::fs::read_to_string(path).with_context(|| format!("failed to read {}", file_path))?;
    let c = RpcClient::probe().context("no mcc server running and file not in local workspace")?;
    let result = c.call("sem", json!({"uri": uri_str, "content": content}))?;
    let symbols = &result["symbols"];

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "file": uri_str,
            "lapper": symbols["lapper"],
            "local": symbols["local"],
            "ref_def_map": symbols["ref_def_map"],
            "cross_file_targets": symbols["global"]["cross_file_targets"],
        }))?
    );
    Ok(())
}

fn show_component(name: &str, args: &ShowArgs) -> Result<()> {
    let cmie = component_def_or_exit(name);
    let mcc::McCMIE::Component(comp) = cmie else {
        error!(target: "mcc::show", "'{}' is not a Component", name);
        std::process::exit(1);
    };
    let mut data = pins_json(&comp.pins);
    data["name"] = json!(name);
    data["uri"] = json!(comp.uri.to_string());
    output(&data, args.span)
}

fn show_module(name: &str, args: &ShowArgs) -> Result<()> {
    let cmie = def_or_exit(name);
    let mcc::McCMIE::Module(module) = cmie else {
        error!(target: "mcc::show", "'{}' is not a Module", name);
        std::process::exit(1);
    };
    let data = json!({
        "name": name,
        "uri": module.uri.to_string(),
        "instances": instances_json(&module.insts, None),
    });
    output(&data, args.span)
}

fn show_interface(name: &str, args: &ShowArgs) -> Result<()> {
    let cmie = def_or_exit(name);
    let mcc::McCMIE::Interface(iface) = cmie else {
        error!(target: "mcc::show", "'{}' is not an Interface", name);
        std::process::exit(1);
    };
    let roles: Vec<String> = iface.roles.iter().map(|r| r.name.to_string()).collect();
    let data = json!({
        "name": name,
        "uri": iface.uri.to_string(),
        "pin_count": iface.pins.pins.len(),
        "role_count": roles.len(),
        "roles": roles,
        "params": iface.params.names_full(),
    });
    output(&data, args.span)
}

fn show_enum(name: &str, args: &ShowArgs) -> Result<()> {
    let cmie = def_or_exit(name);
    let mcc::McCMIE::Enum(en) = cmie else {
        error!(target: "mcc::show", "'{}' is not an Enum", name);
        std::process::exit(1);
    };
    let values: Vec<String> = en.values.iter().map(|v| v.name.to_string()).collect();
    let data = json!({
        "name": name,
        "uri": en.uri.to_string(),
        "value_count": values.len(),
        "values": values,
    });
    output(&data, args.span)
}

/// Points of one Pass2 net (net list moved to `mcc list nets`).
fn show_net(name: &str, args: &ShowArgs) -> Result<()> {
    let top = mcc::cli::globals()
        .top
        .clone()
        .or_else(mcc::mcb_get_first_module_name)
        .unwrap_or_else(|| {
            error!(target: "mcc::show", "no modules found\nhint: load a file with -F or use --top");
            std::process::exit(1);
        });
    let nets = nets_map(&top);

    let data = match nets.get(name) {
        Some(points) => json!({ "name": name, "points": points }),
        None => json!({ "name": name, "points": Vec::<String>::new(), "error": "net not found" }),
    };
    output(&data, args.span)
}

// ============================================================================
// show dianlu — whole circuit tree after instantiation (Pass2)
// ============================================================================

/// `show dianlu`: instantiate the top module (--top or first module) and walk
/// the resulting `McModuleInst` tree. Output is organized as one section per
/// module in source order: same-level instances (components, sub-modules,
/// labels, buses) and connections first, then each sub-module in its own
/// nested section. Interface-typed buses are annotated with their interface
/// class (e.g. `uC.UART0{TX, RX} :: UART.TTL(DCE)`).
fn show_dianlu(args: &ShowArgs) -> Result<()> {
    // Top module resolution mirrors `parse`: a directory target (project
    // mode) resolves the top through the manifest / browse entry; a
    // single-file target uses --top, else the module defined in that file,
    // else the first loaded module.
    let (entry_uri, top) = if let Some(f) = target_path(args) {
        let p = Path::new(f);
        if p.is_dir() {
            crate::cmds::common::load_target(
                Some(f),
                mcc::cli::globals().top.as_deref(),
                mcc::cli::globals().entry.as_deref(),
            )
            .unwrap_or_else(|e| {
                error!(target: "mcc::show", "directory target: {:#}", e);
                std::process::exit(1);
            })
        } else {
            let path = if p.is_absolute() {
                p.to_path_buf()
            } else {
                std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join(p)
            };
            (path.to_string_lossy().to_string(), None)
        }
    } else {
        (String::new(), None)
    };
    let top = crate::cmds::common::resolve_top_module(&entry_uri, top).unwrap_or_else(|| {
        error!(target: "mcc::show", "no modules found\nhint: load a file with -F or use --top");
        std::process::exit(1);
    });
    let uri = mcc::mcb_iter_modules()
        .iter()
        .find(|(n, _)| *n == top)
        .map(|(_, u)| mcc::McURI::from(u.as_str()))
        .unwrap_or_else(|| mcc::McURI::from(top.clone()));

    // Guardrail: a Pass2 panic must not abort the process.
    let inst = crate::cmds::common::build_pass2(&top, &uri).unwrap_or_else(|e| {
        error!(target: "mcc::show", "{e}");
        std::process::exit(1);
    });

    // Global module-nesting overview first (shared with `mcc verify`): every
    // module in source order with its declared / declareb / funcall-generated
    // instances, so the whole instance structure is visible before the
    // per-module sections.
    let hierarchy =
        mcc::hierarchy::build_hierarchy(&mcc::hierarchy::collect_module_nodes(&inst, &top));

    // Text mode: hand-rendered sections (aligned with the user-facing
    // circuit view; the generic key: value fallback would bury the tree).
    if mcc::cli::globals().format == OutputFormat::Text {
        let mut lines = Vec::new();
        lines.push(format!("===== Hierarchy: {top} ====="));
        let mut htext = String::new();
        mcc::hierarchy::render_hierarchy_text(&mut htext, &hierarchy);
        lines.push(htext.trim_end().to_string());
        lines.push(String::new());
        render_dianlu_section(&inst, &top, &mut lines);
        let rendered = lines.join("\n");
        if let Some(path) = &mcc::cli::globals().output {
            std::fs::write(path, rendered)?;
        } else {
            println!("{rendered}");
        }
        return Ok(());
    }

    let data = json!({
        "type": "dianlu",
        "top": top,
        "hierarchy": hierarchy,
        "sections": dianlu_sections(&inst, &top),
    });
    output(&data, args.span)
}

/// Render one module section (text): instances then connections, recursing
/// into sub-modules as their own sections below.
fn render_dianlu_section(inst: &mcc::McModuleInst, path: &str, lines: &mut Vec<String>) {
    lines.push(format!("===== Section: {path} (module) ====="));
    lines.push("Instances:".to_string());

    for comp in &inst.components {
        lines.push(format!(
            "  [C] {}: {} [pins: {}]",
            comp.name,
            comp.def.name,
            comp_pin_labels(comp).join(", ")
        ));
    }
    for sub in &inst.sub_modules {
        lines.push(format!("  [M] {}: {}", sub.name, sub.def.name));
    }

    let mut labels: Vec<&String> = inst.get_labels().keys().collect();
    labels.sort();
    for label in labels {
        lines.push(format!("  [L] {label}"));
    }

    let mut buses: Vec<&mcc::McBusInst> = inst.get_buses().values().collect();
    buses.sort_by(|a, b| a.name.cmp(&b.name));
    for bus in buses {
        let mut line = format!("  [B] {}{{{}}}", bus.name, bus.members.join(", "));
        if let Some(ty) = bus_interface_type(inst, bus) {
            line.push_str(&format!(" :: {ty}"));
        }
        lines.push(line);
    }
    // Interface buses projected by component instances (Pass2 keeps their
    // members as physical pins, so they are surfaced here synthetically).
    for (name, members, ty) in comp_interface_buses(inst) {
        lines.push(format!("  [B] {name}{{{}}} :: {ty}", members.join(", ")));
    }

    lines.push("Connections:".to_string());
    for conn in &inst.connections {
        let net = conn.effective_net_name();
        if net == "NC" {
            continue;
        }
        let points: Vec<&str> = conn
            .points
            .iter()
            .filter(|p| p.path != "NC")
            .map(|p| p.path.as_str())
            .collect();
        if points.is_empty() {
            continue;
        }
        lines.push(format!("  {net} : {}", points.join(" - ")));
    }

    for sub in &inst.sub_modules {
        lines.push(String::new());
        render_dianlu_section(sub, &format!("{path}.{}", sub.name), lines);
    }
}

/// Build the structured (JSON/YAML) representation: one section object per
/// module, in the same order as the text renderer.
fn dianlu_sections(inst: &mcc::McModuleInst, path: &str) -> Vec<Value> {
    let mut section = json!({
        "module": path,
        "uri": inst.def_uri.to_string(),
        "components": inst.components.iter().map(comp_json).collect::<Vec<_>>(),
        "sub_modules": inst.sub_modules.iter().map(|s| json!({
            "name": s.name,
            "class": s.def.name.to_string(),
        })).collect::<Vec<_>>(),
        "connections": Vec::<Value>::new(),
    });

    let mut labels: Vec<&String> = inst.get_labels().keys().collect();
    labels.sort();
    section["labels"] = json!(labels);

    let mut buses: Vec<&mcc::McBusInst> = inst.get_buses().values().collect();
    buses.sort_by(|a, b| a.name.cmp(&b.name));
    section["buses"] = json!(buses
        .iter()
        .map(|bus| {
            json!({
                "name": bus.name,
                "members": bus.members,
                "interface": bus_interface_type(inst, bus),
            })
        })
        .chain(
            comp_interface_buses(inst)
                .into_iter()
                .map(|(name, members, ty)| {
                    json!({
                        "name": name,
                        "members": members,
                        "interface": Some(ty),
                    })
                })
        )
        .collect::<Vec<_>>());

    let conns: Vec<Value> = inst
        .connections
        .iter()
        .filter_map(|conn| {
            let net = conn.effective_net_name();
            if net == "NC" {
                return None;
            }
            let points: Vec<&str> = conn
                .points
                .iter()
                .filter(|p| p.path != "NC")
                .map(|p| p.path.as_str())
                .collect();
            if points.is_empty() {
                return None;
            }
            Some(json!({ "net": net, "points": points }))
        })
        .collect();
    section["connections"] = json!(conns);

    let mut sections = vec![section];
    for sub in &inst.sub_modules {
        sections.extend(dianlu_sections(sub, &format!("{path}.{}", sub.name)));
    }
    sections
}

/// Component instance as JSON: name, class, and sorted pin labels.
fn comp_json(comp: &mcc::McComponentInst) -> Value {
    json!({
        "name": comp.name,
        "class": comp.def.name.to_string(),
        "pins": comp_pin_labels(comp),
    })
}

/// Preferred user-facing label per connected pin (physical id, longest alias
/// in parentheses when a readable name exists), sorted by pin id.
fn comp_pin_labels(comp: &mcc::McComponentInst) -> Vec<String> {
    let mut pins: Vec<String> = comp
        .pins
        .keys()
        .map(|pid| {
            let alias = comp
                .cond_pin_names
                .get(pid)
                .and_then(|names| names.iter().max_by_key(|n| n.len()).cloned())
                .or_else(|| {
                    comp.def
                        .pins
                        .pin_id_to_names
                        .get(pid)
                        .and_then(|names| names.iter().max_by_key(|n| n.len()).cloned())
                });
            match alias {
                Some(n) if n.as_str() != pid.as_str() => format!("{pid}({n})"),
                _ => pid.clone(),
            }
        })
        .collect();
    pins.sort_by_key(|p| {
        p.split('(')
            .next()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(i64::MAX)
    });
    pins
}

/// Resolve an interface class annotation for a bus that projects a component
/// interface (e.g. bus `uC.UART0` → `UART.TTL(DCE)`). Plain buses return None.
fn bus_interface_type(inst: &mcc::McModuleInst, bus: &mcc::McBusInst) -> Option<String> {
    let (comp_name, member) = bus.name.split_once('.')?;
    let comp = inst.components.iter().find(|c| c.name == comp_name)?;
    let mcc::McPinPort::Interface(iface) = comp.def.pins.names_to_id.get(member)? else {
        return None;
    };
    Some(iface_type_string(iface))
}

/// Synthetic interface buses projected by component instances: Pass2 keeps
/// interface members as physical pins, so a component binding such as
/// `io [1:2] = UART0::UART.TTL(DCE)` never registers a bus. Surface it as
/// `inst.IFACE{members}` with its interface class annotation. A bus that
/// already exists (prefixed form registered by a function body) is skipped.
fn comp_interface_buses(inst: &mcc::McModuleInst) -> Vec<(String, Vec<String>, String)> {
    let mut out = Vec::new();
    for comp in &inst.components {
        for (iface_name, port) in &comp.def.pins.names_to_id {
            let mcc::McPinPort::Interface(iface) = port else {
                continue;
            };
            // Anonymous interfaces (`[VCC, GND]::DC(...)`) carry their member
            // names inside the bracket key; surface the bus under the
            // component name alone (e.g. `PWR{VCC, GND}`).
            let name = if iface_name.starts_with('[') {
                comp.name.clone()
            } else {
                format!("{}.{}", comp.name, iface_name)
            };
            if inst.get_buses().contains_key(&name) {
                continue;
            }
            out.push((name, iface_member_names(iface), iface_type_string(iface)));
        }
    }
    out.sort();
    out
}

/// Interface member names in declaration order: bracket-keyed anonymous
/// interfaces (`[VCC, GND]::DC(...)`) carry their member names inside the
/// interface name; otherwise use the declared pin name mapping, then the
/// interface instance members, then the interface definition's pins, then
/// the registered chip pin IDs.
fn iface_member_names(iface: &mcc::Mc2Interface) -> Vec<String> {
    let name = iface.name.to_string();
    if let Some(inner) = name.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let members: Vec<String> = inner.split(',').map(|m| m.trim().to_string()).collect();
        if !members.is_empty() {
            return members;
        }
    }
    if !iface.pin_name_mapping.is_empty() {
        return iface.pin_name_mapping.clone();
    }
    let insts: Vec<String> = iface.insts.iter().map(|m| m.id.to_string()).collect();
    if !insts.is_empty() {
        return insts;
    }
    let names: Vec<String> = iface.base.pins.names_to_id.keys().cloned().collect();
    if !names.is_empty() {
        return names;
    }
    iface.registered_pins.clone()
}

/// Interface class string with params, e.g. `UART.TTL(DCE)` / `I2C(Master)`.
fn iface_type_string(iface: &mcc::Mc2Interface) -> String {
    let base = iface.base_name();
    let params: Vec<String> = iface.params.iter().map(|p| p.to_string()).collect();
    if params.is_empty() {
        base
    } else {
        format!("{base}({})", params.join(", "))
    }
}

// ============================================================================
// Drill-down handlers
// ============================================================================

fn drill_pins(name: &str, args: &ShowArgs) -> Result<()> {
    let cmie = def_or_exit(name);
    let pins = match &cmie {
        mcc::McCMIE::Component(c) => &c.pins,
        mcc::McCMIE::Interface(i) => &i.pins,
        _ => not_applicable("pins", name),
    };
    let mut data = pins_json(pins);
    data["name"] = json!(name);
    output(&data, args.span)
}

fn drill_ports(name: &str, args: &ShowArgs) -> Result<()> {
    let cmie = def_or_exit(name);
    let mcc::McCMIE::Module(module) = &cmie else {
        not_applicable("ports", name);
    };
    let ports: Vec<Value> = module
        .insts
        .insts()
        .iter()
        .filter(|(_, (io_type, _))| {
            !matches!(
                io_type,
                mcc::IOType::None | mcc::IOType::Return | mcc::IOType::NonCon | mcc::IOType::Label
            )
        })
        .map(|(pname, (io_type, inst))| {
            let (ptype, members) = port_type_members(inst);
            json!({
                "name": pname,
                "iotype": format!("{:?}", io_type),
                "type": ptype,
                "members": members,
            })
        })
        .collect();
    let data = json!({ "name": name, "port_count": ports.len(), "ports": ports });
    output(&data, args.span)
}

/// Extract a port's type and sub-members from its instance:
/// - Interface ports: type = interface class name with params (e.g. `I2C(Master)`),
///   members = registered chip pin IDs when available.
/// - List/Bus ports: type = `list` / `bus`, members = declared member names
///   (e.g. `MIC{P,N}` → `P, N`).
/// - Component/Module ports: type = the class name.
/// - Bare ports: type = `pin`.
fn port_type_members(inst: &mcc::McInstance) -> (String, Vec<String>) {
    match inst {
        mcc::McInstance::Interface(i) => {
            let base = i.base_name();
            let params: Vec<String> = i.params.iter().map(|p| p.to_string()).collect();
            let ty = if params.is_empty() {
                base
            } else {
                format!("{base}({})", params.join(", "))
            };
            let members = if i.registered_pins.is_empty() {
                i.insts.iter().map(|m| m.id.to_string()).collect()
            } else {
                i.registered_pins.clone()
            };
            (ty, members)
        }
        mcc::McInstance::List(_) => ("list".to_string(), inst.members()),
        mcc::McInstance::Bus(_) => ("bus".to_string(), inst.members()),
        mcc::McInstance::Component(c) => (c.name.to_string(), Vec::new()),
        mcc::McInstance::Module(m) => (m.name.to_string(), Vec::new()),
        _ => ("pin".to_string(), Vec::new()),
    }
}

fn drill_labels(name: &str, args: &ShowArgs) -> Result<()> {
    let cmie = def_or_exit(name);
    let mcc::McCMIE::Module(module) = &cmie else {
        not_applicable("labels", name);
    };
    let labels: Vec<String> = module
        .insts
        .iter()
        .filter(|(_, inst)| matches!(inst, mcc::McInstance::Label(_)))
        .map(|(n, _)| n.to_string())
        .collect();
    let data = json!({ "name": name, "label_count": labels.len(), "labels": labels });
    output(&data, args.span)
}

fn drill_instances(name: &str, args: &ShowArgs) -> Result<()> {
    let cmie = def_or_exit(name);
    match &cmie {
        mcc::McCMIE::Component(c) => {
            let items = instances_json(&c.insts, args.r#type.as_deref());
            let data = json!({ "name": name, "count": items.len(), "instances": items });
            output(&data, args.span)
        }
        mcc::McCMIE::Module(_) => {
            // Source annotations (stage 5, design §4.5): build the module so
            // every instance carries its origin (src / decl / gen), the
            // declaration / call-site / func-body line, and the caller chain.
            let top = mcc::cli::globals()
                .top
                .clone()
                .unwrap_or_else(|| name.to_string());
            let uri = mcc::mcb_iter_modules()
                .iter()
                .find(|(n, _)| n == &top)
                .map(|(_, u)| mcc::McURI::from(u.as_str()))
                .unwrap_or_else(|| mcc::McURI::from(top.as_str()));
            let inst = crate::cmds::common::build_pass2(&top, &uri).map_err(anyhow::Error::msg)?;
            let content = std::fs::read_to_string(&inst.def_uri.to_string()).ok();
            let fam = mcc::hierarchy::extract_instance_families(&inst, &content);
            let mut items: Vec<Value> = Vec::new();
            for (n, k, l, cl) in fam.source {
                if args
                    .r#type
                    .as_deref()
                    .is_none_or(|t| k.eq_ignore_ascii_case(t))
                {
                    items.push(json!({
                        "name": n, "kind": k, "class": cl,
                        "origin": "src", "line": l,
                    }));
                }
            }
            for (n, l, cl) in fam.declareb {
                items.push(json!({
                    "name": n, "kind": "declareb", "class": cl,
                    "origin": "decl", "line": l,
                }));
            }
            for (n, l, cl, caller) in fam.generated {
                items.push(json!({
                    "name": n, "kind": "component", "class": cl,
                    "origin": "gen", "line": l, "caller": caller,
                }));
            }
            items.sort_by_key(|e| e["line"].as_u64().unwrap_or(u64::MAX));
            let data = json!({ "name": name, "count": items.len(), "instances": items });
            output(&data, args.span)
        }
        _ => not_applicable("instances", name),
    }
}

fn drill_nets(name: &str, args: &ShowArgs) -> Result<()> {
    // Func body nets: `OWNER.FUNC` — connection-line-level nets (no Pass2,
    // funcs depend on parameters and a calling context).
    if let Some(func) = mcc::rpc::handlers::find_func_by_path(name) {
        let nets = mcc::rpc::handlers::func_nets_map(&func);
        let items: Vec<Value> = nets
            .iter()
            .map(|(n, points)| json!({ "name": n, "points": points }))
            .collect();
        let data = json!({ "name": name, "kind": "func", "count": items.len(), "nets": items });
        return output(&data, args.span);
    }

    // `nets <module>` uses the entity as the top module.
    let top = mcc::cli::globals()
        .top
        .clone()
        .unwrap_or_else(|| name.to_string());
    let nets = nets_map(&top);
    let items: Vec<Value> = nets
        .iter()
        .map(|(n, points)| json!({ "name": n, "points": points }))
        .collect();
    let data = json!({ "name": name, "count": items.len(), "nets": items });
    output(&data, args.span)
}

fn drill_attrs(name: &str, args: &ShowArgs) -> Result<()> {
    let cmie = def_or_exit(name);
    let attrs = match &cmie {
        mcc::McCMIE::Component(c) => &c.attrs,
        mcc::McCMIE::Interface(i) => &i.attrs,
        _ => not_applicable("attrs", name),
    };
    let items: Vec<Value> = attrs
        .iter()
        .map(|a| {
            let values: Vec<Value> = a.values.iter().map(attrval_json).collect();
            json!({ "no": a.no, "name": a.id.to_string(), "values": values })
        })
        .collect();
    let data = json!({ "name": name, "count": items.len(), "attrs": items });
    output(&data, args.span)
}

fn drill_funcs(name: &str, args: &ShowArgs) -> Result<()> {
    let cmie = def_or_exit(name);
    let funcs = match &cmie {
        mcc::McCMIE::Component(c) => &c.funcs,
        mcc::McCMIE::Module(m) => &m.funcs,
        _ => not_applicable("funcs", name),
    };
    let items: Vec<Value> = funcs
        .iter()
        .map(|f| json!({ "name": f.name.to_string(), "params": f.params.names_full_annotated() }))
        .collect();
    let data = json!({ "name": name, "count": items.len(), "funcs": items });
    output(&data, args.span)
}

fn drill_params(name: &str, args: &ShowArgs) -> Result<()> {
    // Func params: `OWNER.FUNC` (dot-qualified; funcs are not top-level defs).
    if let Some(func) = mcc::rpc::handlers::find_func_by_path(name) {
        let items: Vec<Value> = func.params.iter().map(|d| param_json(d)).collect();
        let data = json!({ "name": name, "kind": "func", "count": items.len(), "params": items });
        return output(&data, args.span);
    }
    let cmie = def_or_exit(name);
    let params = match &cmie {
        mcc::McCMIE::Component(c) => &c.params,
        mcc::McCMIE::Module(m) => &m.params,
        mcc::McCMIE::Interface(i) => &i.params,
        _ => not_applicable("params", name),
    };
    let items: Vec<Value> = params.iter().map(param_json).collect();
    let arity = params.arity();
    let data = json!({
        "name": name,
        "count": items.len(),
        "required": arity.required,
        "optional": arity.optional,
        "params": items
    });
    output(&data, args.span)
}

/// One parameter declaration as JSON, mirroring the RPC `show.params` shape.
/// `name` uses the display form so compound params render as `[VDD_3V3, GND]`.
fn param_json(d: &mcc::McParamDeclare) -> Value {
    let mut j = mcc::rpc::handlers::param_declare_to_json(d);
    j["name"] = json!(d.display_name());
    j
}

fn drill_roles(name: &str, args: &ShowArgs) -> Result<()> {
    let cmie = def_or_exit(name);
    let mcc::McCMIE::Interface(iface) = &cmie else {
        not_applicable("roles", name);
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
    let data = json!({ "name": name, "count": items.len(), "roles": items });
    output(&data, args.span)
}

fn drill_values(name: &str, args: &ShowArgs) -> Result<()> {
    let cmie = def_or_exit(name);
    let mcc::McCMIE::Enum(en) = &cmie else {
        not_applicable("values", name);
    };
    let values: Vec<String> = en.values.iter().map(|v| v.name.to_string()).collect();
    let data = json!({ "name": name, "count": values.len(), "values": values });
    output(&data, args.span)
}

// ============================================================================
// Entity detail collection (used by `show all` file-layer text details)
// ============================================================================

/// Collect every entity defined in a single `.mc` file as full-field detail
/// values, sorted by source position so the output follows the file layout.
fn collect_dump_file(file: &str) -> Vec<Value> {
    let resolved = resolve_file(file);
    let file_uri = resolved.as_str();
    let mut all: Vec<Value> = Vec::new();

    // mcb_iter_* chains workspace + global tables, so dedup within each
    // category (a component and an enum can share a name+URI, e.g. mcode's
    // `component CAP` + `enum CAP` in cap.mc, so the set must not be shared).
    let mut seen_comp: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    let mut seen_mod: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    let mut seen_iface: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    let mut seen_enum: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();

    for (name, cmie_uri) in mcc::mcb_iter_components() {
        if !seen_comp.insert((name.clone(), cmie_uri.clone())) || !uri_matches(&cmie_uri, file_uri)
        {
            continue;
        }
        let ident = McIds::from(name.as_str());
        let cmie = mcc::get_component_def(&ident, &McURI::from(cmie_uri.as_str()))
            .or_else(|| mcc::get_def(&ident, &McURI::from(cmie_uri.as_str())));
        if let Some(mcc::McCMIE::Component(comp)) = cmie {
            all.push(dump_component(&name, &comp));
        }
    }
    for (name, cmie_uri) in mcc::mcb_iter_modules() {
        if !seen_mod.insert((name.clone(), cmie_uri.clone())) || !uri_matches(&cmie_uri, file_uri) {
            continue;
        }
        let ident = McIds::from(name.as_str());
        if let Some(mcc::McCMIE::Module(module)) =
            mcc::get_def(&ident, &McURI::from(cmie_uri.as_str()))
        {
            all.push(dump_module(&name, &module));
        }
    }
    for (name, cmie_uri) in mcc::mcb_iter_interfaces() {
        if !seen_iface.insert((name.clone(), cmie_uri.clone())) || !uri_matches(&cmie_uri, file_uri)
        {
            continue;
        }
        let ident = McIds::from(name.as_str());
        if let Some(mcc::McCMIE::Interface(iface)) =
            mcc::get_def(&ident, &McURI::from(cmie_uri.as_str()))
        {
            all.push(dump_interface(&name, &iface));
        }
    }
    for (name, cmie_uri) in mcc::mcb_iter_enums() {
        if !seen_enum.insert((name.clone(), cmie_uri.clone())) || !uri_matches(&cmie_uri, file_uri)
        {
            continue;
        }
        let ident = McIds::from(name.as_str());
        if let Some(mcc::McCMIE::Enum(en)) = mcc::get_def(&ident, &McURI::from(cmie_uri.as_str())) {
            all.push(dump_enum(&name, &en));
        }
    }

    all.sort_by_key(|e| e["span"]["start"].as_u64().unwrap_or(u64::MAX));
    all
}

/// True when `cmie_uri` and `file_uri` refer to the same file. The workspace
/// may register a URI in a canonical form different from the caller-provided
/// path, so either string may be a prefix/suffix of the other.
fn uri_matches(cmie_uri: &str, file_uri: &str) -> bool {
    cmie_uri == file_uri || cmie_uri.ends_with(file_uri) || file_uri.ends_with(cmie_uri)
}

fn dump_component(name: &str, comp: &mcc::McComponent) -> Value {
    // Params
    let params: Vec<Value> = comp.params.names_full().iter().map(|n| json!(n)).collect();
    let params_with_defaults: Vec<Value> = comp
        .params
        .get_params_with_defaults()
        .iter()
        .map(|(id, default)| json!({"name": id.to_string(), "default": default}))
        .collect();

    // Attrs
    let attrs: Vec<Value> = comp
        .attrs
        .iter()
        .map(|a| {
            let values: Vec<Value> = a.values.iter().map(attrval_json).collect();
            json!({"no": a.no, "name": a.id.to_string(), "values": values})
        })
        .collect();

    // Funcs (with body lines)
    let funcs: Vec<Value> = comp
        .funcs
        .iter()
        .map(|f| {
            let body_lines: Vec<String> = f.body_lines_display();
            json!({
                "name": f.name.to_string(),
                "params": f.params.names_full_annotated(),
                "returns": f.returns.kind_str(),
                "called_time": f.called_time,
                "body_lines": body_lines,
            })
        })
        .collect();

    // Insts (sub-instances)
    let instances = instances_json(&comp.insts, None);

    // Layout
    let layout = json!({
        "left": comp.layout.left,
        "right": comp.layout.right,
        "top": comp.layout.top,
        "bottom": comp.layout.bottom,
    });

    // CondPins / CondAttrs (debug representation)
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
    data["uri"] = json!(comp.uri.to_string());
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

fn dump_module(name: &str, module: &mcc::McModule) -> Value {
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

    // Insts (ports + sub-instances)
    let instances = instances_json(&module.insts, None);

    // Lines (connection phrases)
    let lines: Vec<String> = module.lines.iter().map(|l| l.to_string()).collect();

    // Funcs
    let funcs: Vec<Value> = module
        .funcs
        .iter()
        .map(|f| {
            let body_lines: Vec<String> = f.body_lines_display();
            json!({
                "name": f.name.to_string(),
                "params": f.params.names_full_annotated(),
                "returns": f.returns.kind_str(),
                "called_time": f.called_time,
                "body_lines": body_lines,
            })
        })
        .collect();

    // LSP goto-def data: param/port definition positions
    let defs: Vec<Value> = module
        .params
        .iter_defs_with_span()
        .map(|(name, span)| json!({"name": name, "span": {"start": span.start, "end": span.end}}))
        .collect();
    // LSP goto-def data: port reference positions in net lines
    let refs: Vec<Value> = module
        .params
        .iter_net_refs()
        .map(|(span, name, scope)| json!({"name": name, "scope": scope, "span": {"start": span.start, "end": span.end}}))
        .collect();

    json!({
        "name": name,
        "kind": "module",
        "uri": module.uri.to_string(),
        "span": {"start": module.span.start, "end": module.span.end},
        "params": params,
        "params_with_defaults": params_with_defaults,
        "instances": instances,
        "lines_count": module.lines.len(),
        "lines": lines,
        "funcs": funcs,
        "defs": defs,
        "refs": refs,
    })
}

fn dump_interface(name: &str, iface: &mcc::McInterface) -> Value {
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
        .map(|r| {
            json!({
                "name": r.name.to_string(),
                "pins": pins_json(&r.pins),
            })
        })
        .collect();

    let mut data = pins_json(&iface.pins);
    data["name"] = json!(name);
    data["kind"] = json!("interface");
    data["uri"] = json!(iface.uri.to_string());
    data["params"] = json!(params);
    data["params_with_defaults"] = json!(params_with_defaults);
    data["attrs"] = json!(attrs);
    data["roles"] = json!(roles);
    data["span"] = json!({"start": iface.span.start, "end": iface.span.end});
    data
}

fn dump_enum(name: &str, en: &mcc::McEnumDef) -> Value {
    let values: Vec<Value> = en
        .values
        .iter()
        .map(|v| {
            json!({
                "name": v.name.to_string(),
                "span": [v.span[0], v.span[1]],
            })
        })
        .collect();

    json!({
        "name": name,
        "kind": "enum",
        "uri": en.uri.to_string(),
        "span": [en.span[0], en.span[1]],
        "value_count": values.len(),
        "values": values,
    })
}

// ============================================================================
// Rendering helpers
// ============================================================================

/// Build the JSON view of a `McPins` (pins + interfaces + name/id mappings).
/// Single implementation lives in `rpc::handlers` so the CLI and the server
/// stay in parity.
fn pins_json(pins: &mcc::McPins) -> Value {
    mcc::rpc::handlers::pins_json(pins)
}

fn inst_kind_class(inst: &mcc::McInstance) -> (&'static str, String) {
    match inst {
        mcc::McInstance::Component(c) => ("component", c.base.name.to_string()),
        mcc::McInstance::Module(m) => ("module", m.base.name.to_string()),
        mcc::McInstance::Label(l) => ("label", l.clone()),
        mcc::McInstance::Interface(i) => ("interface", i.base_name()),
        mcc::McInstance::Bus(b) => ("bus", b.to_string()),
        mcc::McInstance::BusRef { component, bus } => ("busref", format!("{}.{}", component, bus)),
        mcc::McInstance::List(l) => {
            let name = l.name().to_string();
            // Show debug form (includes members) for lists with members
            let class = format!("{:?}", l);
            if class != name {
                ("list", class)
            } else {
                ("list", name)
            }
        }
        mcc::McInstance::Unresolved { class_name } => ("unresolved", class_name.clone()),
        mcc::McInstance::Pins => ("pins", "pins".into()),
        mcc::McInstance::PinId(id) => ("pinid", id.clone()),
        mcc::McInstance::Attr(a) => ("attr", a.to_string()),
        mcc::McInstance::Func(f) => ("func", f.name.to_string()),
        mcc::McInstance::EnumVal {
            enum_name,
            value_name,
            ..
        } => ("enumval", format!("{}.{}", enum_name, value_name)),
    }
}

fn instances_json(insts: &mcc::McInstances, type_filter: Option<&str>) -> Vec<Value> {
    let port_spans = insts.port_spans();
    insts
        .iter()
        .filter_map(|(n, inst)| {
            let (kind, class) = inst_kind_class(inst);
            let kind = if kind == "label" {
                match insts.get_label_kind(n) {
                    mcc::LabelKind::Inline => "ilabel",
                    mcc::LabelKind::Explicit => "label",
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
                Some((mcc::IOType::InOut, _)) => "io",
                Some((mcc::IOType::Out, _)) => "out",
                Some((mcc::IOType::In, _)) => "in",
                _ => "",
            };
            let mut entry = json!({
                "name": n.to_string(),
                "io": io,
                "kind": kind,
                "class": class,
                "params": inst_params(inst),
            });
            if let Some(s) = span {
                entry["span"] = s;
            }
            Some(entry)
        })
        .collect()
}

/// Render the construction parameters of an instance as strings
/// (component `params`, module `args`, interface `params`).
fn inst_params(inst: &mcc::McInstance) -> Vec<String> {
    match inst {
        mcc::McInstance::Component(c) => c.params.iter().map(|p| p.to_string()).collect(),
        mcc::McInstance::Module(m) => m.args.iter().map(|p| p.to_string()).collect(),
        mcc::McInstance::Interface(i) => i.params.iter().map(|p| p.to_string()).collect(),
        _ => Vec::new(),
    }
}

fn attrval_json(v: &mcc::McAttrVal) -> Value {
    match v {
        // Keep string literals quoted so the dump shows the source form.
        mcc::McAttrVal::AttrLiteral(mcc::McLiteral::String(s)) => {
            json!(format!("\"{}\"", s.value))
        }
        other => json!(other.to_string()),
    }
}

/// Build the Pass2 netlist for a module: net name -> ordered point labels.
pub(crate) fn nets_map(top: &str) -> BTreeMap<String, Vec<String>> {
    let uri = mcc::mcb_iter_modules()
        .iter()
        .find(|(n, _)| n == top)
        .map(|(_, u)| mcc::McURI::from(u.as_str()))
        .unwrap_or_else(|| mcc::McURI::from(top));

    // Guardrail: a Pass2 panic must not abort the process.
    let inst = crate::cmds::common::build_pass2(top, &uri).unwrap_or_else(|e| {
        error!(target: "mcc::show", "{e}");
        std::process::exit(1);
    });

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

// ============================================================================
// Output
// ============================================================================

/// Render `show all` layered output (tagged `type: "layered_all"`) in text
/// mode: one section per layer, separated by `------`. Sections follow a fixed
/// order (system -> use -> file) instead of the JSON map's alphabetical order.
/// The `file` layer renders per-entity details (full-field compact text);
/// the `use`/`system` layers keep the name-list overview. Returns `None` for
/// every other data shape.
fn render_layered_text(data: &Value, span: bool) -> Option<String> {
    if data.get("type")?.as_str()? != LAYERED_ALL_TYPE {
        return None;
    }
    let obj = data.as_object()?;
    let file = obj.get("target_file").and_then(|v| v.as_str());
    let mut lines = Vec::new();
    for layer in ["system", "use", "file"] {
        let Some(section) = obj.get(layer) else {
            continue;
        };
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(format!("------ {layer} ------"));
        if layer == "file" && file.is_some() {
            for entity in collect_dump_file(file.unwrap()) {
                lines.push(compact::render_entity(&entity, span));
            }
        } else if let Some(sec) = section.as_object() {
            for (k, v) in sec {
                lines.push(format!("{k}: {v}"));
            }
        }
    }
    Some(lines.join("\n"))
}

/// `list all` text renderer: `{type:"all", count, list:[{name, kind, uri}]}`
/// → a `count:` header followed by one `kind: name` line per definition.
fn render_all_list_text(data: &Value) -> Option<String> {
    if data.get("type").and_then(|v| v.as_str()) != Some("all") {
        return None;
    }
    let count = data.get("count")?.as_u64()?;
    let items = data.get("list")?.as_array()?;
    let mut out = format!("count: {count}\n");
    for item in items {
        let name = item.get("name")?.as_str()?;
        let kind = item.get("kind")?.as_str()?;
        out.push_str(&format!("{kind}: {name}\n"));
    }
    Some(out.trim_end().to_string())
}

/// `list <kind>` text renderer: `{type, count, list:[names]}` → a `count:`
/// header followed by one name per line.
fn render_kind_list_text(data: &Value) -> Option<String> {
    let t = data.get("type")?.as_str()?;
    if !matches!(t, "component" | "module" | "interface" | "enum") {
        return None;
    }
    let names: Vec<&str> = data
        .get("list")?
        .as_array()?
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    let mut out = format!("count: {}\n", names.len());
    for n in names {
        out.push_str(n);
        out.push('\n');
    }
    Some(out.trim_end().to_string())
}

/// `list nets` text renderer: `{type:"net", count, nets:[{name, points}]}`
/// → a `count:` header followed by one `name: point, point` line per net.
fn render_nets_list_text(data: &Value) -> Option<String> {
    if data.get("type").and_then(|v| v.as_str()) != Some("net") {
        return None;
    }
    let count = data.get("count")?.as_u64()?;
    let nets = data.get("nets")?.as_array()?;
    let mut out = format!("count: {count}\n");
    for net in nets {
        let name = net.get("name")?.as_str()?;
        let points: Vec<&str> = net
            .get("points")?
            .as_array()?
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        out.push_str(&format!("{name}: {}\n", points.join(", ")));
    }
    Some(out.trim_end().to_string())
}

/// `list ports` text renderer: `{type:"port", count, ports:[{name, iotype, module}]}`
/// → a `count:` header followed by one `name: iotype (module)` line per port.
fn render_ports_list_text(data: &Value) -> Option<String> {
    if data.get("type").and_then(|v| v.as_str()) != Some("port") {
        return None;
    }
    let count = data.get("count")?.as_u64()?;
    let ports = data.get("ports")?.as_array()?;
    let mut out = format!("count: {count}\n");
    for port in ports {
        let name = port.get("name")?.as_str()?;
        let iotype = port.get("iotype")?.as_str()?;
        let module = port.get("module")?.as_str()?;
        out.push_str(&format!("{name}: {iotype} ({module})\n"));
    }
    Some(out.trim_end().to_string())
}

/// `list files` text renderer: `{type:"files", count, files:[{uri, *_count}]}`
/// → a `count:` header followed by one `uri: comp=N mod=N iface=N enum=N` line.
fn render_files_list_text(data: &Value) -> Option<String> {
    if data.get("type").and_then(|v| v.as_str()) != Some("files") {
        return None;
    }
    let count = data.get("count")?.as_u64()?;
    let files = data.get("files")?.as_array()?;
    let mut out = format!("count: {count}\n");
    for f in files {
        let uri = f.get("uri")?.as_str()?;
        let comp = f
            .get("component_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let mod_ = f.get("module_count").and_then(|v| v.as_u64()).unwrap_or(0);
        let iface = f
            .get("interface_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let en = f.get("enum_count").and_then(|v| v.as_u64()).unwrap_or(0);
        out.push_str(&format!(
            "{uri}: comp={comp} mod={mod_} iface={iface} enum={en}\n"
        ));
    }
    Some(out.trim_end().to_string())
}

/// Render a component/pins data object (`name`, `uri`, `pin_count`, `pins`)
/// as an aligned text table. Returns `None` when the data has no `pins` array.
fn render_pins_text(data: &Value) -> Option<String> {
    let pins = data.get("pins")?;
    if pins.as_array().is_none() {
        return None;
    }

    let mut out = String::new();
    if let Some(name) = data.get("name").and_then(|v| v.as_str()) {
        out.push_str(&format!("component: {name}\n"));
    }
    if let Some(uri) = data.get("uri").and_then(|v| v.as_str()) {
        out.push_str(&format!("uri: {uri}\n"));
    }
    if let Some(n) = data.get("pin_count").and_then(|v| v.as_u64()) {
        out.push_str(&format!("pin_count: {n}\n"));
    }
    out.push('\n');
    out.push_str(&render_pins_table(pins));
    Some(out)
}

/// Render an attrs drill-down as an aligned table (text format only).
fn render_attrs_text(data: &Value) -> Option<String> {
    let attrs = data.get("attrs")?.as_array()?;
    let mut rows: Vec<(String, String)> = Vec::new();
    for a in attrs {
        let name = a.get("name")?.as_str()?.to_string();
        let values: Vec<String> = a
            .get("values")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|v| match v.as_str() {
                        Some(s) => s.to_string(),
                        None => v.to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        rows.push((name, values.join(", ")));
    }

    let name_w = rows.iter().map(|r| r.0.len()).max().unwrap_or(10).max(10);
    let val_w = rows.iter().map(|r| r.1.len()).max().unwrap_or(30).max(30);

    let mut out = String::new();
    if let Some(name) = data.get("name").and_then(|v| v.as_str()) {
        out.push_str(&format!("{name}\n"));
    }
    if let Some(n) = data.get("count").and_then(|v| v.as_u64()) {
        out.push_str(&format!("attr_count: {n}\n"));
    }
    out.push('\n');
    out.push_str(&format!("  {:<name_w$}  {}\n", "name", "values"));
    out.push_str(&format!(
        "  {}  {}\n",
        "-".repeat(name_w),
        "-".repeat(val_w.min(80)),
    ));
    for (name, values) in &rows {
        out.push_str(&format!("  {:<name_w$}  {}\n", name, values));
    }
    Some(out)
}

/// Render a pin list (as produced by `pins_json`) as an aligned table.
/// Header/divider/rows only — callers add the entity title lines.
fn render_pins_table(pins: &Value) -> String {
    let Some(pins) = pins.as_array() else {
        return String::new();
    };
    let mut rows: Vec<(String, String, String, String)> = Vec::new();
    for p in pins {
        let id = p
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let io = p
            .get("iotype")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let names: Vec<String> = p
            .get("names")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|n| n.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let ifaces: Vec<String> = p
            .get("interfaces")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(iface_display).collect())
            .unwrap_or_default();
        rows.push((id, io, names.join(", "), ifaces.join(" | ")));
    }

    let id_w = rows.iter().map(|r| r.0.len()).max().unwrap_or(4).max(4);
    let io_w = rows.iter().map(|r| r.1.len()).max().unwrap_or(8).max(8);
    let names_w = rows.iter().map(|r| r.2.len()).max().unwrap_or(40).max(40);
    let name_w = rows.iter().map(|r| r.3.len()).max().unwrap_or(12).max(12);

    let mut out = String::new();
    out.push_str(&format!(
        "  {:<id_w$}  {:<io_w$}  {:<names_w$}  {}\n",
        "id", "io", "names", "interfaces"
    ));
    out.push_str(&format!(
        "  {}  {}  {}  {}\n",
        "-".repeat(id_w),
        "-".repeat(io_w),
        "-".repeat(names_w),
        "-".repeat(name_w.min(60)),
    ));
    for (id, io, names, ifaces) in &rows {
        out.push_str(&format!(
            "  {:<id_w$}  {:<io_w$}  {:<names_w$}  {}\n",
            id, io, names, ifaces
        ));
    }
    out
}

/// Render an aligned text table from string rows. Column widths adapt to the
/// widest header/cell. Used by the `show` drill-downs in text format.
fn render_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let cols = headers.len();
    let widths: Vec<usize> = (0..cols)
        .map(|c| {
            let h = headers[c].len();
            rows.iter().map(|r| r[c].len()).max().unwrap_or(h).max(h)
        })
        .collect();
    let mut out = String::new();
    let hdr: Vec<String> = (0..cols)
        .map(|c| format!("{:<w$}", headers[c], w = widths[c]))
        .collect();
    out.push_str(&format!("  {}\n", hdr.join("  ")));
    out.push_str(&format!(
        "  {}\n",
        widths
            .iter()
            .map(|w| "-".repeat(*w))
            .collect::<Vec<_>>()
            .join("  ")
    ));
    for r in rows {
        let cells: Vec<String> = (0..cols)
            .map(|c| format!("{:<w$}", r[c], w = widths[c]))
            .collect();
        out.push_str(&format!("  {}\n", cells.join("  ")));
    }
    out
}

/// Render any `show` drill-down as readable text: an aligned table (object
/// arrays) or a per-line list (string arrays), headed by the entity name and a
/// count line. JSON output is unaffected — this fires only for text format.
fn render_drill_text(data: &Value) -> Option<String> {
    let name = data.get("name")?.as_str()?;

    // Count line: drill-downs use port_count / label_count / count.
    let count_key = ["port_count", "label_count", "count"]
        .iter()
        .find(|k| data.get(**k).is_some())?;
    let count = data.get(count_key)?.as_u64()?;

    let body: String = if let Some(arr) = data.get("ports").and_then(|v| v.as_array()) {
        let rows: Vec<Vec<String>> = arr
            .iter()
            .map(|p| {
                let members = p
                    .get("members")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|m| m.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                vec![
                    p.get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    p.get("iotype")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    p.get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    members,
                ]
            })
            .collect();
        render_table(&["name", "iotype", "type", "members"], &rows)
    } else if let Some(arr) = data.get("instances").and_then(|v| v.as_array()) {
        let has_origin = arr.iter().any(|i| i.get("origin").is_some());
        let has_caller = arr.iter().any(|i| {
            i.get("caller")
                .and_then(|v| v.as_str())
                .is_some_and(|c| !c.is_empty())
        });
        let rows: Vec<Vec<String>> = arr
            .iter()
            .map(|i| {
                let params = i
                    .get("params")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|p| p.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                let mut row = vec![
                    i.get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    i.get("kind")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    i.get("class")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    params,
                ];
                if has_origin {
                    let origin = i
                        .get("origin")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let line = i
                        .get("line")
                        .and_then(|v| v.as_u64())
                        .map(|l| format!("L{l}"))
                        .unwrap_or_default();
                    row.push(if line.is_empty() {
                        origin
                    } else {
                        format!("{origin}@{line}")
                    });
                }
                if has_caller {
                    row.push(
                        i.get("caller")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                    );
                }
                row
            })
            .collect();
        let mut headers = vec!["name", "kind", "class", "params"];
        if has_origin {
            headers.push("origin");
        }
        if has_caller {
            headers.push("caller");
        }
        render_table(&headers, &rows)
    } else if let Some(arr) = data.get("funcs").and_then(|v| v.as_array()) {
        let rows: Vec<Vec<String>> = arr
            .iter()
            .map(|f| {
                let params = f
                    .get("params")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|p| p.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                vec![
                    f.get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    params,
                ]
            })
            .collect();
        render_table(&["name", "params"], &rows)
    } else if let Some(arr) = data.get("nets").and_then(|v| v.as_array()) {
        let rows: Vec<Vec<String>> = arr
            .iter()
            .map(|n| {
                let points = n
                    .get("points")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|p| p.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                vec![
                    n.get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    points,
                ]
            })
            .collect();
        render_table(&["name", "points"], &rows)
    } else if let Some(arr) = data.get("roles").and_then(|v| v.as_array()) {
        // Each role: heading followed by its pins table.
        let mut body = String::new();
        for r in arr {
            let rname = r
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            body.push_str(&format!("role: {rname}\n"));
            if let Some(pins) = r.get("pins").and_then(|p| p.get("pins")) {
                body.push_str(&render_pins_table(pins));
            }
            body.push('\n');
        }
        body
    } else if let Some(arr) = data.get("params").and_then(|v| v.as_array()) {
        // Parameter declarations: name / type / default table. The type column
        // shows the concrete interface class with its constructor params
        // (e.g. `DC(3.3V)`) and falls back to the semantic category
        // (e.g. `A1-Label`) when no class is bound.
        let rows: Vec<Vec<String>> = arr
            .iter()
            .map(|p| {
                let class = p.get("class").and_then(|v| v.as_str()).unwrap_or("");
                let params = p
                    .get("params")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                let ty = if class.is_empty() {
                    p.get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string()
                } else if params.is_empty() {
                    class.to_string()
                } else {
                    format!("{class}({params})")
                };
                vec![
                    p.get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    ty,
                    p.get("default")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                ]
            })
            .collect();
        render_table(&["name", "type", "default"], &rows)
    } else if let Some(arr) = data
        .get("labels")
        .or_else(|| data.get("values"))
        .and_then(|v| v.as_array())
    {
        // Simple string lists: one entry per line.
        arr.iter()
            .filter_map(|v| v.as_str())
            .map(|s| format!("  {s}"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    } else {
        return None;
    };

    let mut out = format!("{name}\n{count_key}: {count}\n\n");
    out.push_str(&body);
    Some(out)
}

/// Format one interface entry of a pin. Interfaces render as
/// `Name::Base(param1, param2)`, buses as `Name{CLK, DATA}`, and List groups
/// as `Name[CLK, DATA]` to mirror the `.mc` source notation.
fn iface_display(v: &Value) -> Option<String> {
    let kind = v
        .get("kind")
        .and_then(|x| x.as_str())
        .unwrap_or("Interface");
    let inst = v.get("name").and_then(|x| x.as_str())?;
    let members: Vec<String> = v
        .get("members")
        .and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(|x| x.to_string()))
                .collect()
        })
        .unwrap_or_default();
    match kind {
        "Bus" => Some(format!("{}{{{}}}", inst, members.join(", "))),
        "List" => Some(format!("{}[{}]", inst, members.join(", "))),
        _ => {
            let base = v.get("base").and_then(|x| x.as_str()).unwrap_or(inst);
            let params: Vec<String> = v
                .get("params")
                .and_then(|x| x.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|s| s.as_str().map(|x| x.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            if params.is_empty() {
                Some(format!("{inst}::{base}"))
            } else {
                Some(format!("{inst}::{base}({})", params.join(", ")))
            }
        }
    }
}

pub(crate) fn output(data: &Value, span: bool) -> Result<()> {
    let rendered = match mcc::cli::globals().format {
        OutputFormat::Json => data.to_string(),
        OutputFormat::JsonPretty => serde_json::to_string_pretty(data)?,
        OutputFormat::Yaml => serde_yaml::to_string(data).unwrap_or_default(),
        OutputFormat::Csv => data.to_string(),
        OutputFormat::Text => {
            // Entity dump values (kind == "func") render like the other
            // drill-downs below (list / table); everything else falls through
            // the layered / list renderers.
            if let Some(t) = render_layered_text(data, span) {
                // show all: per-layer sections; the file layer renders details
                t
            } else if let Some(t) = render_all_list_text(data) {
                // list all: `kind: name` per line, count header
                t
            } else if let Some(t) = render_kind_list_text(data) {
                // list component/module/interface/enum: one name per line
                t
            } else if let Some(t) = render_nets_list_text(data) {
                // list nets: `name: point, point` per line
                t
            } else if let Some(t) = render_ports_list_text(data) {
                // list ports: `name: iotype (module)` per line
                t
            } else if let Some(t) = render_files_list_text(data) {
                // list files: `uri: comp=N mod=N iface=N enum=N` per line
                t
            } else if let Some(t) = render_pins_text(data) {
                // component / pins drill-down: aligned pin table
                t
            } else if let Some(t) = render_attrs_text(data) {
                // attrs drill-down: aligned name/values table
                t
            } else if let Some(t) = render_drill_text(data) {
                // other drill-downs (ports/labels/instances/nets/funcs/params/
                // roles/values): aligned tables or per-line lists
                t
            } else if let Some(obj) = data.as_object() {
                obj.iter()
                    .map(|(k, v)| format!("{}: {}", k, v))
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                data.to_string()
            }
        }
    };

    if let Some(path) = &mcc::cli::globals().output {
        std::fs::write(path, rendered)?;
    } else {
        println!("{}", rendered);
    }
    Ok(())
}
