// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc export <KIND> <FILE>` — thin CLI wrapper around `mcc::export`.
//!
//! The actual build pipeline (netlist/BOM/SPICE) lives in
//! `src/export/mod.rs` (lib root) so the JSON-RPC handler in
//! `rpc/handlers.rs` can share the exact same code without reaching into
//! the binary's private `cmds` module.

use crate::output::envelope::ExportData;
use crate::output::{self, builder::ResultBuilder, envelope::Envelope};
use anyhow::Result;
use mcc::cli::{rpcclient::RpcClient, ExportArgs, ExportKind, OutputFormat};
use mcc::export;
use serde_json::{json, Value};
use std::path::Path;

pub fn run(args: &ExportArgs) -> Result<()> {
    // Pattern B: probe + rpc_mapping + fallthrough to local.
    if let Some(c) = RpcClient::probe() {
        if let Some((method, params)) = rpc_mapping(args) {
            match c.call(method, params) {
                Ok(result) => {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                    return Ok(());
                }
                Err(e) => tracing::debug!(
                    target: "mcc::export",
                    "RPC failed, falling back to local: {}",
                    e
                ),
            }
        }
    }
    run_local(args)
}

/// Map CLI args → RPC method + params. Returns `None` for now (export is
/// local-only on the CLI; `export` server method exists for direct RPC users).
fn rpc_mapping(args: &ExportArgs) -> Option<(&'static str, Value)> {
    if std::env::var("MCC_RPC_EXPORT").is_ok() {
        Some((
            "export",
            json!({
                "kind":   match args.kind {
                    ExportKind::Netlist => "netlist",
                    ExportKind::Bom => "bom",
                    ExportKind::Spice => "spice",
                    ExportKind::KiCad => "kicad-netlist",
                },
                "entry":  args.file,
                "top":    args.top,
                "format": match args.format {
                    OutputFormat::Text => "text",
                    OutputFormat::Json => "json",
                    OutputFormat::JsonPretty => "json-pretty",
                    OutputFormat::Yaml => "yaml",
                    OutputFormat::Csv => "csv",
                },
                "libs":   args.lib,
            }),
        ))
    } else {
        None
    }
}

fn run_local(args: &ExportArgs) -> Result<()> {
    let format = if args.json {
        OutputFormat::Json
    } else {
        args.format
    };

    let (tree, table) = match export::build_tree(&args.file, args.top.as_deref(), &args.lib) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{}", e);
            return Ok(());
        }
    };

    // Resolve top name for header.
    let top = args
        .top
        .clone()
        .unwrap_or_else(|| mcc::mcb_get_first_module_name().unwrap_or_else(|| "?".into()));
    let kind_str = match args.kind {
        ExportKind::Netlist => "netlist",
        ExportKind::Bom => "bom",
        ExportKind::Spice => "spice",
        ExportKind::KiCad => "kicad-netlist",
    };
    let kind_tag = match args.kind {
        ExportKind::Netlist => 0u8,
        ExportKind::Bom => 1u8,
        ExportKind::Spice => 2u8,
        ExportKind::KiCad => 3u8,
    };
    let format_tag = match format {
        OutputFormat::Text => 0u8,
        OutputFormat::Json => 1u8,
        OutputFormat::JsonPretty => 2u8,
        OutputFormat::Yaml => 3u8,
        OutputFormat::Csv => 4u8,
    };
    let (raw_text, items, count) =
        export::build_payload(&tree, &table, &top, kind_tag, format_tag);

    if format == OutputFormat::Json {
        let data = ExportData {
            kind: kind_str.to_string(),
            format: "json".to_string(),
            count,
            items,
        };
        let mut builder = ResultBuilder::start(format!("mcc export {}", kind_str));
        builder.set_export(data);
        let env = Envelope::ok(builder.finish());
        output::emit_envelope(&env, format, args.output.as_deref().map(Path::new), false)?;
    } else {
        // Raw text/CSV → stdout or file.
        match &args.output {
            Some(p) => std::fs::write(p, raw_text.as_bytes().to_vec())?,
            None => {
                print!("{}", raw_text);
                if !raw_text.ends_with('\n') {
                    println!();
                }
                eprintln!("({} items)", count);
            }
        }
    }
    Ok(())
}
