// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc list` — List top-level definition names.
//!
//! Targets: `all` / `component` / `module` / `interface` / `enum` / `nets` /
//! `ports` / `files`. Detailed content of one entity is the `mcc show`
//! command (see cmds/show.rs); the two replace the former dual-mode `show`.

use crate::cmds::filter;
use crate::cmds::show::{classify_def_scope, nets_map, output, resolve_file, resolve_scopes};
use anyhow::Result;
use mcc::cli::{rpcclient::RpcClient, ListArgs, ListTarget, OutputFormat};
use mcc::McURI;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use tracing::error;

pub fn run(args: &ListArgs) -> Result<()> {
    // Server path: the list kinds map 1:1 to the legacy `show.*.list` RPC
    // methods. Everything else falls through to local execution.
    if let Some(c) = RpcClient::probe() {
        if let Some((method, params)) = rpc_mapping(args) {
            match c.call(method, params) {
                Ok(result) => {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                    return Ok(());
                }
                Err(e) => {
                    tracing::debug!(target: "mcc::list", "RPC failed, using local mode: {}", e);
                }
            }
        }
    }

    run_local(args)
}

/// Map list targets to their RPC method + params. Returns `None` when the
/// command must fall through to local execution: text format (RPC handlers
/// only return JSON), a `--filter` (RPC list methods don't apply filters), or
/// `list all` (local-only flat aggregation).
fn rpc_mapping(args: &ListArgs) -> Option<(&'static str, Value)> {
    if mcc::cli::globals().format == OutputFormat::Text || args.filter.is_some() {
        return None;
    }
    let m = match args.target {
        ListTarget::All => return None,
        ListTarget::Component => "show.component.list",
        ListTarget::Module => "show.module.list",
        ListTarget::Interface => "show.interface.list",
        ListTarget::Enum => "show.enum.list",
        ListTarget::Nets => "show.net.list",
        ListTarget::Ports => "show.ports.list",
        ListTarget::Files => "show.files",
    };
    Some((m, json!({ "file": args.file })))
}

fn run_local(args: &ListArgs) -> Result<()> {
    // One-shot environment setup: init engine, load `--lib` libraries, load
    // the `-F` target file.
    let file_opt = args.file.as_deref();
    crate::cmds::manifest::init_local(file_opt, &mcc::cli::globals().lib);
    if let Some(f) = file_opt {
        let uri = McURI::from(resolve_file(f).as_str());
        mcc::mcc_load_project(&uri);
    }

    match args.target {
        ListTarget::All => list_all(args),
        ListTarget::Component => list_kind(ListTarget::Component, args),
        ListTarget::Module => list_kind(ListTarget::Module, args),
        ListTarget::Interface => list_kind(ListTarget::Interface, args),
        ListTarget::Enum => list_kind(ListTarget::Enum, args),
        ListTarget::Nets => list_nets(args),
        ListTarget::Ports => list_ports(args),
        ListTarget::Files => list_files(args),
    }
}

/// Flat aggregate of every definition in scope, kind-tagged.
///
/// Scope follows the same default policy as `show all`: with `-F` the `file`
/// layer is the default, `--scope` selects use/system/all; without `-F` every
/// loaded layer is included (overview role).
fn list_all(args: &ListArgs) -> Result<()> {
    let target = args.file.as_deref().map(resolve_file);
    let scopes = resolve_scopes(args.scope, args.file.is_some());
    let in_scope = |uri: &str| scopes.contains(&classify_def_scope(uri, target.as_deref()));

    let mut items: Vec<Value> = Vec::new();
    for (n, u) in mcc::mcb_iter_components() {
        if in_scope(&u) {
            items.push(json!({ "name": n, "kind": "component", "uri": u }));
        }
    }
    for (n, u) in mcc::mcb_iter_modules() {
        if in_scope(&u) {
            items.push(json!({ "name": n, "kind": "module", "uri": u }));
        }
    }
    for (n, u) in mcc::mcb_iter_interfaces() {
        if in_scope(&u) {
            items.push(json!({ "name": n, "kind": "interface", "uri": u }));
        }
    }
    for (n, u) in mcc::mcb_iter_enums() {
        if in_scope(&u) {
            items.push(json!({ "name": n, "kind": "enum", "uri": u }));
        }
    }
    if let Some(filter) = args.filter.as_deref() {
        let names: Vec<String> = items
            .iter()
            .filter_map(|i| i.get("name").and_then(|v| v.as_str()).map(String::from))
            .collect();
        let kept = filter::apply_to_names(Some(filter), names)?;
        let kept: std::collections::HashSet<String> = kept.into_iter().collect();
        items.retain(|i| {
            i.get("name")
                .and_then(|v| v.as_str())
                .is_some_and(|n| kept.contains(n))
        });
    }
    let data = json!({ "type": "all", "count": items.len(), "list": items });
    output(&data, false)
}

/// Flat name list for one kind.
fn list_kind(target: ListTarget, args: &ListArgs) -> Result<()> {
    let (ty, items) = match target {
        ListTarget::Component => ("component", mcc::mcb_iter_components()),
        ListTarget::Module => ("module", mcc::mcb_iter_modules()),
        ListTarget::Interface => ("interface", mcc::mcb_iter_interfaces()),
        ListTarget::Enum => ("enum", mcc::mcb_iter_enums()),
        _ => unreachable!("list_kind only handles the four definition kinds"),
    };
    let names: Vec<String> = items.into_iter().map(|(n, _)| n).collect();
    // `--filter` only accepts `name=` for name lists (single string per row).
    let names = filter::apply_to_names(args.filter.as_deref(), names)?;
    let data = json!({ "type": ty, "count": names.len(), "list": names });
    output(&data, false)
}

/// All Pass2 nets of the top module (`--top` overrides).
fn list_nets(_args: &ListArgs) -> Result<()> {
    let top = mcc::cli::globals()
        .top
        .clone()
        .or_else(mcc::mcb_get_first_module_name)
        .unwrap_or_else(|| {
            error!(target: "mcc::list", "no modules found\nhint: load a file with -F or use --top");
            std::process::exit(1);
        });
    let nets = nets_map(&top);
    let items: Vec<Value> = nets
        .iter()
        .map(|(n, points)| json!({ "name": n, "points": points }))
        .collect();
    let data = json!({ "type": "net", "count": items.len(), "nets": items });
    output(&data, false)
}

/// All module ports (name, iotype, module, uri).
fn list_ports(_args: &ListArgs) -> Result<()> {
    let ports: Vec<Value> = mcc::mcb_iter_ports()
        .into_iter()
        .map(|(name, iotype, module, uri)| {
            json!({ "name": name, "iotype": iotype, "module": module, "uri": uri })
        })
        .collect();
    let data = json!({ "type": "port", "count": ports.len(), "ports": ports });
    output(&data, false)
}

/// Every loaded file with per-file definition counts.
fn list_files(_args: &ListArgs) -> Result<()> {
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
    output(&data, false)
}
