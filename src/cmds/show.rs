// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc show` — Show detailed information for parsed definitions.
//!
//! Two families of queries (structured `verb <what> [<name>]` syntax):
//!   * containers  : `all` / `file` / `component` / `module` / `interface` / `enum` / `net`
//!                   (omit <name> → list; give <name> → detail)
//!   * drill-down  : `pins` / `ports` / `labels` / `instances` / `nets` / `attrs`
//!                   / `funcs` / `params` / `roles` / `values` (<name> = owning entity;
//!                   funcs are referenced dot-qualified as `OWNER.FUNC` for
//!                   `params` and `nets`)

use crate::cmds::filter;
use crate::output::compact;
use anyhow::{Context, Result};
use mcc::cli::{rpcclient::RpcClient, OutputFormat, ShowArgs, ShowTarget};
use mcc::{McIds, McURI};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;
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
        ShowTarget::All => Some(("show.all", json!({}))),
        ShowTarget::File => Some(("show.file", json!({ "file": args.name }))),
        ShowTarget::Files => Some(("show.files", json!({}))),
        ShowTarget::Lapper | ShowTarget::Ast => {
            // local-only: read file, call internal sem, dump lapper / AST tree
            return None;
        }

        // ── container list/detail ──────────────────────────────────────────
        ShowTarget::Component | ShowTarget::Module | ShowTarget::Interface | ShowTarget::Enum => {
            if args.name.is_none() {
                let m = match args.target {
                    ShowTarget::Component => "show.component.list",
                    ShowTarget::Module => "show.module.list",
                    ShowTarget::Interface => "show.interface.list",
                    ShowTarget::Enum => "show.enum.list",
                    _ => unreachable!(),
                };
                Some((m, json!({ "file": args.file })))
            } else {
                let m = match args.target {
                    ShowTarget::Component => "show.component",
                    ShowTarget::Module => "show.module",
                    ShowTarget::Interface => "show.interface",
                    ShowTarget::Enum => "show.enum",
                    _ => unreachable!(),
                };
                Some((m, json!({ "name": args.name, "file": args.file })))
            }
        }
        ShowTarget::Net => {
            if args.name.is_none() {
                Some(("show.net.list", json!({})))
            } else {
                Some(("show.net", json!({ "name": args.name })))
            }
        }

        // ── drill-down ─────────────────────────────────────────────────────
        ShowTarget::Pins => drill_rpc("show.pins", args),
        ShowTarget::Ports => {
            if args.name.is_some() {
                drill_rpc("show.ports", args)
            } else {
                Some(("show.ports.list", json!({})))
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
        ShowTarget::Dump => None, // local-only: compact text rendering
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
    // Suppress C-layer AST tree printing for dump targets (local-only, compact output)
    if matches!(args.target, ShowTarget::Dump) {
        mcc::set_trace_stdout_suppressed(true);
    }
    prepare(args);

    let name = args.name.as_deref();
    match args.target {
        // ── containers ─────────────────────────────────────────────────────
        ShowTarget::All => show_all(args),
        ShowTarget::File => show_file(args),
        ShowTarget::Files => show_files(args),
        ShowTarget::Lapper => show_lapper(args),
        ShowTarget::Ast => show_ast(args),
        ShowTarget::Component => match name {
            None => list_kind(Kind::Component, args),
            Some(n) => show_component(n, args),
        },
        ShowTarget::Module => match name {
            None => list_kind(Kind::Module, args),
            Some(n) => show_module(n, args),
        },
        ShowTarget::Interface => match name {
            None => list_kind(Kind::Interface, args),
            Some(n) => show_interface(n, args),
        },
        ShowTarget::Enum => match name {
            None => list_kind(Kind::Enum, args),
            Some(n) => show_enum(n, args),
        },
        ShowTarget::Net => show_net(name.unwrap_or(""), args),

        // ── drill-down ─────────────────────────────────────────────────────
        ShowTarget::Pins => drill_pins(require_name(args), args),
        ShowTarget::Ports => match name {
            None => list_ports(args),
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
        ShowTarget::Dump => {
            if let Some(n) = name {
                if looks_like_file(n) {
                    show_dump_file(n, args)
                } else {
                    show_dump(n, args)
                }
            } else if let Some(f) = args.file.as_deref() {
                show_dump_file(f, args)
            } else {
                show_dump_all(args)
            }
        }
    }
}

// ============================================================================
// Setup
// ============================================================================

/// One-shot environment setup: init engine, load `--lib` libraries, load the
/// target file. All handlers assume this ran, so none of them re-init.
fn prepare(args: &ShowArgs) {
    // File target keeps the path in <name>; all others use `-F/--file`.
    let file_opt = match args.target {
        ShowTarget::File => args.name.as_deref(),
        // `show dump <file>.mc` accepts the file path as the positional name.
        ShowTarget::Dump => args
            .file
            .as_deref()
            .or_else(|| args.name.as_deref().filter(|n| looks_like_file(n))),
        _ => args.file.as_deref(),
    };
    crate::cmds::manifest::init_local(file_opt, &mcc::cli::globals().lib);

    if let Some(f) = file_opt {
        let actual = resolve_file(f);
        let uri = mcc::McURI::from(actual.as_str());
        mcc::mcc_load_project(&uri);
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

/// Resolve a file path; if it doesn't exist, search by base name in the tree.
fn resolve_file(file: &str) -> String {
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

#[derive(Copy, Clone)]
enum Kind {
    Component,
    Module,
    Interface,
    Enum,
}

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
    let components: Vec<String> = mcc::mcb_iter_components()
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    let modules: Vec<String> = mcc::mcb_iter_modules()
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    let interfaces: Vec<String> = mcc::mcb_iter_interfaces()
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    let enums: Vec<String> = mcc::mcb_iter_enums().into_iter().map(|(n, _)| n).collect();

    let data = json!({
        format!("module_list({})", modules.len()): modules,
        format!("component_list({})", components.len()): components,
        format!("interface_list({})", interfaces.len()): interfaces,
        format!("enum_list({})", enums.len()): enums,
    });
    output(&data, args)
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

fn show_file(args: &ShowArgs) -> Result<()> {
    // File was resolved+loaded in prepare(); report what the load produced.
    let file = args.name.as_deref().unwrap_or("");
    let components: Vec<String> = mcc::mcb_iter_components()
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    let modules: Vec<String> = mcc::mcb_iter_modules()
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    let interfaces: Vec<String> = mcc::mcb_iter_interfaces()
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    let enums: Vec<String> = mcc::mcb_iter_enums().into_iter().map(|(n, _)| n).collect();

    let data = json!({
        "file": file,
        format!("module_list({})", modules.len()): modules,
        format!("component_list({})", components.len()): components,
        format!("interface_list({})", interfaces.len()): interfaces,
        format!("enum_list({})", enums.len()): enums,
    });
    output(&data, args)
}

fn list_kind(kind: Kind, args: &ShowArgs) -> Result<()> {
    let (ty, items) = match kind {
        Kind::Component => ("component", mcc::mcb_iter_components()),
        Kind::Module => ("module", mcc::mcb_iter_modules()),
        Kind::Interface => ("interface", mcc::mcb_iter_interfaces()),
        Kind::Enum => ("enum", mcc::mcb_iter_enums()),
    };
    let names: Vec<String> = items.into_iter().map(|(n, _)| n).collect();
    // `--filter` only accepts `name=` for `--list` targets (single string per row).
    let names = filter::apply_to_names(args.filter.as_deref(), names)?;
    let data = json!({ "type": ty, "count": names.len(), "list": names });
    output(&data, args)
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
    output(&data, args)
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
    output(&data, args)
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
    output(&data, args)
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
    output(&data, args)
}

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

    let data = if name.is_empty() {
        let items: Vec<Value> = nets
            .iter()
            .map(|(n, points)| json!({ "name": n, "points": points }))
            .collect();
        json!({ "type": "net", "count": items.len(), "nets": items })
    } else {
        match nets.get(name) {
            Some(points) => json!({ "name": name, "points": points }),
            None => {
                json!({ "name": name, "points": Vec::<String>::new(), "error": "net not found" })
            }
        }
    };
    output(&data, args)
}

// ============================================================================
// Files overview
// ============================================================================

fn show_files(args: &ShowArgs) -> Result<()> {
    use std::collections::BTreeMap;

    // Aggregate counts per URI across all definition kinds.
    #[derive(Default)]
    struct FileInfo {
        component_count: usize,
        module_count: usize,
        interface_count: usize,
        enum_count: usize,
    }

    let mut files: BTreeMap<String, FileInfo> = BTreeMap::new();

    for (_, uri) in mcc::mcb_iter_components() {
        files.entry(uri).or_default().component_count += 1;
    }
    for (_, uri) in mcc::mcb_iter_modules() {
        files.entry(uri).or_default().module_count += 1;
    }
    for (_, uri) in mcc::mcb_iter_interfaces() {
        files.entry(uri).or_default().interface_count += 1;
    }
    for (_, uri) in mcc::mcb_iter_enums() {
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

    let data = json!({ "type": "files", "count": items.len(), "files": items });
    output(&data, args)
}

// ============================================================================
// Ports list (global)
// ============================================================================

fn list_ports(args: &ShowArgs) -> Result<()> {
    let ports: Vec<Value> = mcc::mcb_iter_ports()
        .into_iter()
        .map(|(name, iotype, module, uri)| {
            json!({ "name": name, "iotype": iotype, "module": module, "uri": uri })
        })
        .collect();
    let data = json!({ "type": "port", "count": ports.len(), "ports": ports });
    output(&data, args)
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
    output(&data, args)
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
    output(&data, args)
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
    output(&data, args)
}

fn drill_instances(name: &str, args: &ShowArgs) -> Result<()> {
    let cmie = def_or_exit(name);
    let insts = match &cmie {
        mcc::McCMIE::Component(c) => &c.insts,
        mcc::McCMIE::Module(m) => &m.insts,
        _ => not_applicable("instances", name),
    };
    let items = instances_json(insts, args.r#type.as_deref());
    let data = json!({ "name": name, "count": items.len(), "instances": items });
    output(&data, args)
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
        return output(&data, args);
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
    output(&data, args)
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
    output(&data, args)
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
        .map(|f| json!({ "name": f.name.to_string(), "params": f.params.names_full() }))
        .collect();
    let data = json!({ "name": name, "count": items.len(), "funcs": items });
    output(&data, args)
}

fn drill_params(name: &str, args: &ShowArgs) -> Result<()> {
    // Func params: `OWNER.FUNC` (dot-qualified; funcs are not top-level defs).
    if let Some(func) = mcc::rpc::handlers::find_func_by_path(name) {
        let items: Vec<Value> = func.params.iter().map(|d| param_json(d)).collect();
        let data = json!({ "name": name, "kind": "func", "count": items.len(), "params": items });
        return output(&data, args);
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
    output(&data, args)
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
    output(&data, args)
}

fn drill_values(name: &str, args: &ShowArgs) -> Result<()> {
    let cmie = def_or_exit(name);
    let mcc::McCMIE::Enum(en) = &cmie else {
        not_applicable("values", name);
    };
    let values: Vec<String> = en.values.iter().map(|v| v.name.to_string()).collect();
    let data = json!({ "name": name, "count": values.len(), "values": values });
    output(&data, args)
}

// ============================================================================
// Dump — full-field dump of any entity for parse debugging
// ============================================================================

/// Dump all entities in scope (when no name given).
fn show_dump_all(args: &ShowArgs) -> Result<()> {
    let mut all = Vec::new();

    for (name, _uri) in mcc::mcb_iter_components() {
        if let Some(cmie) = find_def(&name) {
            if let mcc::McCMIE::Component(comp) = &cmie {
                all.push(dump_component(&name, comp));
            }
        }
    }
    for (name, _uri) in mcc::mcb_iter_modules() {
        if let Some(cmie) = find_def(&name) {
            if let mcc::McCMIE::Module(module) = &cmie {
                all.push(dump_module(&name, module));
            }
        }
    }
    for (name, _uri) in mcc::mcb_iter_interfaces() {
        if let Some(cmie) = find_def(&name) {
            if let mcc::McCMIE::Interface(iface) = &cmie {
                all.push(dump_interface(&name, iface));
            }
        }
    }
    for (name, _uri) in mcc::mcb_iter_enums() {
        if let Some(cmie) = find_def(&name) {
            if let mcc::McCMIE::Enum(en) = &cmie {
                all.push(dump_enum(&name, en));
            }
        }
    }

    // Sort by source position
    all.sort_by_key(|e| e["span"]["start"].as_u64().unwrap_or(u64::MAX));

    let data = json!({
        "type": "dump_all",
        "total": all.len(),
        "entities": all,
    });
    output(&data, args)
}

fn show_dump(name: &str, args: &ShowArgs) -> Result<()> {
    let cmie = def_or_exit(name);
    let data = match &cmie {
        mcc::McCMIE::Component(comp) => dump_component(name, comp),
        mcc::McCMIE::Module(module) => dump_module(name, module),
        mcc::McCMIE::Interface(iface) => dump_interface(name, iface),
        mcc::McCMIE::Enum(en) => dump_enum(name, en),
    };
    output(&data, args)
}

/// Dump every entity defined in a single `.mc` file (file-scoped dump).
///
/// Unlike [`show_dump_all`] (which dumps everything currently in scope,
/// including loaded libraries), this filters to entities whose URI is the
/// target file, so `mcc show dump <file>.mc` shows exactly what the parser
/// produced for that file.
fn show_dump_file(file: &str, args: &ShowArgs) -> Result<()> {
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

    // Sort by source position so the output follows the file layout.
    all.sort_by_key(|e| e["span"]["start"].as_u64().unwrap_or(u64::MAX));

    let data = json!({
        "type": "dump_all",
        "file": resolved,
        "total": all.len(),
        "entities": all,
    });
    output(&data, args)
}

/// True when a `show dump` positional argument is a file path rather than an
/// entity name (a `.mc` suffix or an existing path on disk).
fn looks_like_file(name: &str) -> bool {
    name.ends_with(".mc") || Path::new(name).exists()
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
            let body_lines: Vec<String> = f.lines.iter().map(|l| l.to_string()).collect();
            json!({
                "name": f.name.to_string(),
                "params": f.params.names_full(),
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
    // Params
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

    // Insts (ports + sub-instances)
    let instances = instances_json(&module.insts, None);

    // Lines (connection phrases)
    let lines: Vec<String> = module.lines.iter().map(|l| l.to_string()).collect();

    // Funcs
    let funcs: Vec<Value> = module
        .funcs
        .iter()
        .map(|f| {
            let body_lines: Vec<String> = f.lines.iter().map(|l| l.to_string()).collect();
            json!({
                "name": f.name.to_string(),
                "params": f.params.names_full(),
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
            let mut entry = json!({
                "name": n.to_string(),
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
        mcc::McAttrVal::AttrLiteral(mcc::McLiteral::String(s)) => json!(s.value),
        other => json!(other.to_string()),
    }
}

/// Build the top module and aggregate its connections into a net → points map.
fn nets_map(top: &str) -> BTreeMap<String, Vec<String>> {
    let uri = mcc::mcb_iter_modules()
        .iter()
        .find(|(n, _)| n == top)
        .map(|(_, u)| mcc::McURI::from(u.as_str()))
        .unwrap_or_else(|| mcc::McURI::from(top));
    let ident = mcc::McIds::from(top);

    // Guardrail: a Pass2 panic must not abort the process.
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        mcc::mcc_build(&ident, &uri)
    }));
    let inst = match built {
        Ok(Ok(i)) => i,
        Ok(Err(e)) => {
            error!(target: "mcc::show", "build failed: {}", e);
            std::process::exit(1);
        }
        Err(_) => {
            error!(target: "mcc::show", "build panicked (engine Pass2 bug)");
            std::process::exit(1);
        }
    };

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
                vec![
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
                ]
            })
            .collect();
        render_table(&["name", "kind", "class", "params"], &rows)
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

fn output(data: &Value, args: &ShowArgs) -> Result<()> {
    let rendered = match mcc::cli::globals().format {
        OutputFormat::Json => data.to_string(),
        OutputFormat::JsonPretty => serde_json::to_string_pretty(data)?,
        OutputFormat::Yaml => serde_yaml::to_string(data).unwrap_or_default(),
        OutputFormat::Csv => data.to_string(),
        OutputFormat::Text => {
            // Detect dump output and render in compact .mc-like format.
            // `kind == "func"` drill-downs (show params/nets OWNER.FUNC) are
            // excluded here so they render like the other drill-downs below
            // (list / table) instead of compact's single-line entity form.
            if data
                .get("kind")
                .and_then(|v| v.as_str())
                .is_some_and(|k| k != "func")
            {
                compact::render_entity(data, args.span)
            } else if data.get("type").and_then(|v| v.as_str()) == Some("dump_all") {
                compact::render_all(data, args.span)
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
