// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc extract` — structured data extraction (envelope version)

use crate::cli::{rpc_client::RpcClient, ExtractArgs, ExtractTarget};
use crate::cmds::filter;
use crate::cmds::manifest;
use crate::cmds::proj::resolve_workspace_ref;
use crate::output::{
    self,
    builder::ResultBuilder,
    envelope::{Envelope, ExtractData, RpcError},
    OutputFormatExt,
};
use anyhow::{Context, Result};
use mcc::{McCMIE, McIds, McInstance, McURI};
use serde_json::{json, Value};
use std::path::Path;

pub fn run(args: &ExtractArgs) -> Result<()> {
    if let Some(client) = RpcClient::probe() {
        let result = client.call(
            "extract",
            json!({
                "entry":  args.file.clone(),
                "target": format!("{:?}", args.target).to_lowercase(),
                "top":    args.top.clone(),
                "libs":   args.lib.clone(),
            }),
        )?;
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    // Initialize (do not load system libraries)
    mcc::mcc_init_no_lib();

    // Local mode also loads --lib (the other half of Fix 2)
    manifest::load_libs(&args.lib);

    // components / interfaces operate on already-loaded libraries; no file required
    match args.target {
        ExtractTarget::Components => return extract_components(args),
        ExtractTarget::Interfaces => return extract_interfaces(args),
        _ => {}
    }

    // Note: --filter is applied per-target below; for components/interfaces
    // we apply it after building the items vec, before emit_extract.

    // instances / nets require an entry
    let uri = if let Some(file) = &args.file {
        let uri = McURI::from(file.as_str());
        mcc::mcc_load_project(&uri);
        uri
    } else {
        return emit_err(
            args,
            RpcError::invalid_params("extract instances/nets: target file must be specified"),
        );
    };

    let top_name = args
        .top
        .clone()
        .or_else(|| mcc::mcb_get_module_name_by_uri(&uri))
        .or_else(|| mcc::mcb_get_first_module_name());
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
        ExtractTarget::Nets => extract_nets(&uri, &top_name, &ident, args),
        _ => unreachable!(),
    }
}

fn extract_instances(uri: &McURI, top_name: &str, ident: &McIds, args: &ExtractArgs) -> Result<()> {
    let cmie = mcc::get_def(ident, uri)
        .with_context(|| format!("extract: definition '{}' not found", top_name))?;
    let module_def = match cmie {
        McCMIE::Module(m) => m,
        _ => {
            return emit_err(
                args,
                RpcError::invalid_params(format!("'{}' is not a Module", top_name)),
            )
        }
    };

    let mut items: Vec<serde_json::Value> = module_def
        .insts
        .iter()
        .map(|(name, inst)| {
            let (kind, class) = match inst {
                McInstance::Component(c) => ("component", c.name.to_string()),
                McInstance::Module(m) => ("module", m.name.to_string()),
                McInstance::Label(l) => ("label", l.clone()),
                McInstance::Interface(i) => ("interface", i.name.to_string()),
                McInstance::Bus(b) => ("bus", b.name().to_string()),
                McInstance::BusRef { component, bus } => {
                    ("busref", format!("{}.{}", component, bus))
                }
                McInstance::List(l) => ("list", l.name().to_string()),
            };
            json!({ "name": name.to_string(), "kind": kind, "class": class })
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

    emit_extract(args, "instances", Value::Array(items))
}

fn extract_nets(uri: &McURI, _top_name: &str, ident: &McIds, args: &ExtractArgs) -> Result<()> {
    let inst = mcc::mcc_build(ident, uri).map_err(|e| anyhow::anyhow!("build failed: {}", e))?;

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

    let items: Vec<serde_json::Value> = nets
        .into_iter()
        .map(|(name, points)| json!({ "name": name, "points": points }))
        .collect();

    let items = filter::apply_to_values(args.filter.as_deref(), Value::Array(items), &["name"])?;
    emit_extract(args, "nets", items)
}

fn extract_components(args: &ExtractArgs) -> Result<()> {
    let items: Vec<serde_json::Value> = mcc::mcb_iter_components()
        .into_iter()
        .map(|(name, uri)| json!({ "name": name, "uri": uri }))
        .collect();
    // components emit `name` + `uri` only — `class=` / `kind=` keys are rejected.
    let items = filter::apply_to_values(args.filter.as_deref(), Value::Array(items), &["name"])?;
    emit_extract(args, "components", items)
}

fn extract_interfaces(args: &ExtractArgs) -> Result<()> {
    let items: Vec<serde_json::Value> = mcc::mcb_iter_interfaces()
        .into_iter()
        .map(|(name, uri)| json!({ "name": name, "uri": uri }))
        .collect();
    let items = filter::apply_to_values(args.filter.as_deref(), Value::Array(items), &["name"])?;
    emit_extract(args, "interfaces", items)
}

// ── helpers ──

fn emit_extract(args: &ExtractArgs, target: &str, items: serde_json::Value) -> Result<()> {
    let mut builder =
        ResultBuilder::start(format!("mcc extract {}", target)).workspace(resolve_workspace_ref());
    builder.set_extract(ExtractData {
        target: target.into(),
        items: items.clone(),
    });
    let env = Envelope::ok(builder.finish());
    output::emit_envelope(
        &env,
        args.format,
        args.output.as_deref().map(Path::new),
        false,
    )?;

    // Text mode: details → stdout, count → stderr (Fix 3)
    if !args.format.is_structured() {
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

fn emit_err(args: &ExtractArgs, err: RpcError) -> Result<()> {
    if args.format.is_structured() {
        output::emit_envelope(&Envelope::err(err), args.format, None, false)?;
        Ok(())
    } else {
        Err(anyhow::anyhow!(err.message))
    }
}
