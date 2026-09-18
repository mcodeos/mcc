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
use crate::output::die;
use crate::output::{emit_projection, OutputFormatExt, ProjectionKey};
use anyhow::Result;
use mcc::cli::{ListArgs, ListTarget};
use mcc::McURI;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;

/// ⚠ The CLI no longer delegates to a running server (CIMP §1 U90, ruling (b)).
/// The list kinds mapped 1:1 to the legacy `show.*.list` RPC methods, but the
/// request carried only the raw `file` argument, which the server resolved
/// against *its own* `current_dir()` — so with a daemon running every
/// `list <kind> -F <dir> -f json` answered `component_count 0 / module_count 0`
/// for a design holding 10 components and 7 modules. A reader that returns a
/// smaller answer is not the same question's other reading. The RPC methods
/// stay for direct callers.
pub fn run(args: &ListArgs) -> Result<()> {
    run_local(args)
}

/// Structured face → A-tier envelope (U86 item 7, first slice); text / csv keep
/// the layered / list renderers in [`show::output`], byte for byte.
///
/// One key for all five payload shapes: `list` is the word, and each payload
/// names its own kind (`type: all|component|…|net|port|files`), so the envelope
/// does not need a second discriminator.
fn emit_list(data: Value) -> Result<()> {
    if mcc::cli::globals().format.is_structured() {
        return emit_projection(
            ProjectionKey::List,
            data,
            mcc::cli::globals().format,
            mcc::cli::globals().output.as_deref().map(Path::new),
        );
    }
    output(&data, false)
}

fn run_local(args: &ListArgs) -> Result<()> {
    // One-shot environment setup: init engine, load `--lib` libraries, load
    // the `-F` target file — or the current directory when it holds a project
    // manifest and `-F` is absent.
    let file_opt = crate::cmds::manifest::effective_target(args.file.as_deref());
    let file_opt = file_opt.as_deref();
    crate::cmds::manifest::init_local(file_opt, &mcc::cli::globals().lib);
    if let Some(f) = file_opt {
        if Path::new(f).is_dir() {
            crate::cmds::common::load_target(
                Some(f),
                mcc::cli::globals().top.as_deref(),
                mcc::cli::globals().entry.as_deref(),
            )?;
        } else {
            let uri = McURI::from(resolve_file(f).as_str());
            mcc::mcc_load_project(&uri);
        }
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
    emit_list(data)
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
    emit_list(data)
}

/// All Pass2 nets of the top module (`--top` overrides).
fn list_nets(_args: &ListArgs) -> Result<()> {
    let top = mcc::cli::globals()
        .top
        .clone()
        .or_else(mcc::mcb_get_first_module_name)
        .unwrap_or_else(|| {
            die!(
                "mcc::list",
                1,
                "no modules found\nhint: load a file with -F or use --top"
            );
        });
    let nets = nets_map(&top);
    let items: Vec<Value> = nets
        .iter()
        .map(|(n, points)| json!({ "name": n, "points": points }))
        .collect();
    let data = json!({ "type": "net", "count": items.len(), "nets": items });
    emit_list(data)
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
    emit_list(data)
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
    emit_list(data)
}
