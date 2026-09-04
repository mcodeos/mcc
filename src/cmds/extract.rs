// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc extract` — structured data extraction (envelope version).
//!
//! Migration shim (extract-merge plan): the four raw-table projections now live
//! under `mcc query --kind`; this command is retained during the migration
//! window as a thin wrapper that emits the legacy `ExtractData{target, items}`
//! envelope byte-for-byte (local path), so downstream scripts that consume
//! `mcc extract` output keep working. RPC delegation was removed — the command
//! always runs local, like `query`. The RPC `extract` method / `handle_extract`
//! / `extract_from_uri` server-side tables are untouched.

use crate::cmds::common;
use crate::cmds::filter;
use crate::cmds::manifest;
use crate::cmds::nets;
use crate::cmds::proj::resolve_workspace_ref;
use crate::output::{
    self,
    builder::ResultBuilder,
    envelope::{Envelope, ExtractData, RpcError},
    OutputFormatExt,
};
use anyhow::{Context, Result};
use mcc::cli::{ExtractArgs, ExtractTarget};
use mcc::{McCMIE, McIds, McInstance, McURI};
use serde_json::{json, Value};
use std::path::Path;

pub fn run(args: &ExtractArgs) -> Result<()> {
    // Shared local initialization: engine + libs (global config, --lib, mcode
    // default). Local-only since the merge — no RPC delegation.
    manifest::init_local(args.file.as_deref(), &mcc::cli::globals().lib);

    // components / interfaces operate on already-loaded libraries; no file required
    match args.target {
        ExtractTarget::Components => return extract_components(args),
        ExtractTarget::Interfaces => return extract_interfaces(args),
        _ => {}
    }

    // instances / nets require an entry
    let file = match &args.file {
        Some(f) => f,
        None => {
            return emit_err(RpcError::invalid_params(
                "extract instances/nets: target file must be specified",
            ))
        }
    };
    let uri = McURI::from(file.as_str());
    mcc::mcc_load_project(&uri);

    // Top-module resolution: global --top → module declared in the entry →
    // first loaded module (identical chain to the old inline code, centralized
    // in cmds/common.rs).
    let top_name = common::resolve_top_module(file, None);
    let top_name = match top_name {
        Some(n) => n,
        None => {
            eprintln!("no module in file.");
            return Ok(());
        }
    };

    let ident = McIds::from(top_name.as_str());
    match args.target {
        ExtractTarget::Instances => extract_instances(&uri, &top_name, &ident, args),
        ExtractTarget::Nets => extract_nets(&uri, &top_name, args),
        _ => unreachable!(),
    }
}

fn extract_instances(uri: &McURI, top_name: &str, ident: &McIds, args: &ExtractArgs) -> Result<()> {
    let cmie = mcc::get_def(ident, uri)
        .with_context(|| format!("extract: definition '{}' not found", top_name))?;
    let module_def = match cmie {
        McCMIE::Module(m) => m,
        _ => {
            return emit_err(RpcError::invalid_params(format!(
                "'{}' is not a Module",
                top_name
            )))
        }
    };

    let mut items: Vec<serde_json::Value> = module_def
        .insts
        .iter()
        .map(|(name, inst)| {
            // Class column: legacy semantics preserved byte-for-byte for the
            // shim (extract-merge plan, class-divergence policy) — resolved
            // component/module/interface instances report their own symbol
            // name here, not the base class name. The *kind* column is
            // single-sourced from `search_api::instance_kind_tag`, shared with
            // `query --kind instance` (`inst_kind`).
            let class = match inst {
                McInstance::Component(c) => c.name.to_string(),
                McInstance::Module(m) => m.name.to_string(),
                McInstance::Label(l) => l.clone(),
                McInstance::Interface(i) => i.name.to_string(),
                McInstance::Bus(b) => b.name().to_string(),
                McInstance::BusRef { component, bus } => {
                    format!("{}.{}", component, bus)
                }
                McInstance::List(l) => l.name().to_string(),
                McInstance::Unresolved { class_name } => class_name.clone(),
                McInstance::Pins => "pins".into(),
                McInstance::PinId(id) => id.clone(),
                McInstance::Attr(a) => a.to_string(),
                McInstance::Func(f) => f.name.to_string(),
                McInstance::EnumVal {
                    enum_name,
                    value_name,
                    ..
                } => format!("{}.{}", enum_name, value_name),
            };
            json!({
                "name": name.to_string(),
                "kind": mcc::search_api::instance_kind_tag(inst),
                "class": class
            })
        })
        .collect();

    items = filter::apply_to_values(
        args.filter.as_deref(),
        Value::Array(items),
        &["name", "kind", "class"],
    )?
    .as_array()
    .cloned()
    .unwrap_or_default();

    emit_extract("instances", Value::Array(items))
}

fn extract_nets(uri: &McURI, top_name: &str, args: &ExtractArgs) -> Result<()> {
    // Shared net-table projection (same fold as `query --kind net` /
    // `list nets` / `show net|nets`).
    let nets = nets::top_nets(top_name, Some(uri)).map_err(anyhow::Error::msg)?;
    let items: Vec<serde_json::Value> = nets
        .into_iter()
        .map(|(name, points)| json!({ "name": name, "points": points }))
        .collect();

    let items = filter::apply_to_values(args.filter.as_deref(), Value::Array(items), &["name"])?;
    emit_extract("nets", items)
}

fn extract_components(args: &ExtractArgs) -> Result<()> {
    let items: Vec<serde_json::Value> = mcc::mcb_iter_components()
        .into_iter()
        .map(|(name, uri)| json!({ "name": name, "uri": uri }))
        .collect();
    // components emit `name` + `uri`; `attr(...)` works because matches_json_record
    // resolves attrs from get_def (via mcc::query_api::attrs_for_def).
    let items = filter::apply_to_values(
        args.filter.as_deref(),
        Value::Array(items),
        &["name", "attr"],
    )?;
    emit_extract("components", items)
}

fn extract_interfaces(args: &ExtractArgs) -> Result<()> {
    let items: Vec<serde_json::Value> = mcc::mcb_iter_interfaces()
        .into_iter()
        .map(|(name, uri)| json!({ "name": name, "uri": uri }))
        .collect();
    let items = filter::apply_to_values(
        args.filter.as_deref(),
        Value::Array(items),
        &["name", "attr"],
    )?;
    emit_extract("interfaces", items)
}

// ── helpers ──

fn emit_extract(target: &str, items: serde_json::Value) -> Result<()> {
    let mut builder =
        ResultBuilder::start(format!("mcc extract {}", target)).workspace(resolve_workspace_ref());
    builder.set_extract(ExtractData {
        target: target.into(),
        items: items.clone(),
    });
    let env = Envelope::ok(builder.finish());
    output::emit_envelope(
        &env,
        mcc::cli::globals().format,
        mcc::cli::globals().output.as_deref().map(Path::new),
        false,
    )?;

    // Text mode: details → stdout, count → stderr (Fix 3)
    if !mcc::cli::globals().format.is_structured() {
        if let Some(arr) = items.as_array() {
            for it in arr {
                match it.get("name").and_then(|v| v.as_str()) {
                    Some(n) => println!("{}", n),
                    None => println!("{}", it),
                }
            }
            eprintln!("({} items)", arr.len());
        }
    }
    Ok(())
}

fn emit_err(err: RpcError) -> Result<()> {
    if mcc::cli::globals().format.is_structured() {
        output::emit_envelope(&Envelope::err(err), mcc::cli::globals().format, None, false)?;
        Ok(())
    } else {
        Err(anyhow::anyhow!(err.message))
    }
}
