// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc def` — Go-to-definition for symbols (M6).
//!
//! ```bash
//! mcc def RES --lib mcode        # find component definition
//! mcc def main -F circuit.mc     # find module definition
//! ```

use crate::output::{emit_projection, OutputFormatExt, ProjectionKey};
use anyhow::Result;
use mcc::cli::{rpcclient::RpcClient, DefArgs};
use mcc::{get_def, McCMIE, McIds, McURI};
use serde_json::{json, Value};
use std::path::Path;

pub fn run(args: &DefArgs) -> Result<()> {
    if let Some(c) = RpcClient::probe() {
        let params = json!({ "name": args.name });
        match c.call("def", params) {
            Ok(result) => return emit_def(result),
            Err(e) => tracing::debug!(target: "mcc::def", "RPC failed, using local: {}", e),
        }
    }

    run_local(args)
}

/// Structured face → A-tier envelope (U86 item 7, first slice); text / csv are
/// untouched (`def` was format-blind before the slice — see `emit_report`'s note).
fn emit_def(data: Value) -> Result<()> {
    if mcc::cli::globals().format.is_structured() {
        return emit_projection(
            ProjectionKey::Def,
            data,
            mcc::cli::globals().format,
            mcc::cli::globals().output.as_deref().map(Path::new),
        );
    }
    println!("{}", serde_json::to_string_pretty(&data)?);
    Ok(())
}

fn run_local(args: &DefArgs) -> Result<()> {
    // An omitted target defaults to the current directory when it holds a
    // project manifest.
    let file = crate::cmds::manifest::effective_target(args.file.as_deref());
    crate::cmds::manifest::init_local(file.as_deref(), &mcc::cli::globals().lib);
    if let Some(f) = file.as_deref() {
        if std::path::Path::new(f).is_dir() {
            crate::cmds::common::load_target(
                Some(f),
                mcc::cli::globals().top.as_deref(),
                mcc::cli::globals().entry.as_deref(),
            )?;
        } else {
            let uri = McURI::from(f);
            mcc::mcc_load_project(&uri);
        }
    }

    let name = &args.name;
    let iterators: [(&str, Vec<(String, String)>); 4] = [
        ("component", mcc::mcb_iter_components()),
        ("module", mcc::mcb_iter_modules()),
        ("interface", mcc::mcb_iter_interfaces()),
        ("enum", mcc::mcb_iter_enums()),
    ];

    for (kind, items) in &iterators {
        if let Some((matched, uri)) = items.iter().find(|(n, _)| n == name) {
            let ident = McIds::from(matched.as_str());
            let uri_obj = McURI::from(uri.as_str());

            let detail = match get_def(&ident, &uri_obj) {
                Some(McCMIE::Component(c)) => json!({
                    "kind": "component",
                    "name": matched,
                    "uri": uri,
                    "pin_count": c.pins.pins.len(),
                }),
                Some(McCMIE::Module(m)) => json!({
                    "kind": "module",
                    "name": matched,
                    "uri": uri,
                    "instance_count": m.insts.iter().count(),
                }),
                Some(McCMIE::Interface(i)) => json!({
                    "kind": "interface",
                    "name": matched,
                    "uri": uri,
                    "pin_count": i.pins.pins.len(),
                }),
                Some(McCMIE::Enum(e)) => json!({
                    "kind": "enum",
                    "name": matched,
                    "uri": uri,
                    "value_count": e.values.len(),
                }),
                None => json!({
                    "kind": kind,
                    "name": matched,
                    "uri": uri,
                }),
            };

            return emit_def(detail);
        }
    }

    anyhow::bail!("definition not found: {name}");
}
