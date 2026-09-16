// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc export <KIND> [FILE]` — thin CLI wrapper around `mcc::export`.
//!
//! The actual build pipeline (netlist/BOM/SPICE) lives in
//! `src/export/mod.rs` (lib root) so the JSON-RPC handler in
//! `rpc/handlers.rs` can share the exact same code without reaching into
//! the binary's private `cmds` module.

use crate::cmds::common;
use crate::cmds::manifest;
use crate::output::envelope::ExportData;
use crate::output::{self, builder::ResultBuilder, envelope::Envelope};
use anyhow::Result;
use mcc::cli::{rpcclient::RpcClient, ExportArgs, OutputFormat};
use mcc::export;
use serde_json::{json, Value};
use std::path::Path;

pub fn run(args: &ExportArgs) -> Result<()> {
    // An omitted target defaults to the current directory when it holds a
    // project manifest.
    let target = manifest::effective_target(args.file.as_deref());
    // Pattern B: probe + rpc_mapping + fallthrough to local.
    if let Some(c) = RpcClient::probe() {
        if let Some((method, params)) = rpc_mapping(args, target.as_deref()) {
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
    run_local(args, target.as_deref())
}

/// Map CLI args → RPC method + params. Returns `None` for now (export is
/// local-only on the CLI; `export` server method exists for direct RPC users).
fn rpc_mapping(args: &ExportArgs, target: Option<&str>) -> Option<(&'static str, Value)> {
    if std::env::var("MCC_RPC_EXPORT").is_ok() {
        Some((
            "export",
            json!({
                "kind":   args.kind.name(),
                "entry":  target,
                "top":    mcc::cli::globals().top,
                "format": mcc::cli::globals().format.name(),
                "libs":   mcc::cli::globals().lib,
            }),
        ))
    } else {
        None
    }
}

fn run_local(args: &ExportArgs, target: Option<&str>) -> Result<()> {
    let Some(target) = target else {
        anyhow::bail!("export: <target> not specified");
    };

    // Shared local initialization: engine + libs (global config, --lib, mcode default).
    manifest::init_local(Some(target), &mcc::cli::globals().lib);

    // A directory target resolves to its manifest's entry file; the export
    // pipeline below consumes a single file.
    let (entry_uri, _) = common::load_target(
        Some(target),
        mcc::cli::globals().top.as_deref(),
        mcc::cli::globals().entry.as_deref(),
    )?;

    let format = if args.json {
        OutputFormat::Json
    } else {
        mcc::cli::globals().format
    };

    let (tree, table, arena, inst_store) = match export::build_tree(
        &entry_uri,
        mcc::cli::globals().top.as_deref(),
        &mcc::cli::globals().lib,
    ) {
        Ok(quad) => quad,
        Err(e) => {
            eprintln!("{}", e);
            return Ok(());
        }
    };

    // Resolve top name for header.
    let top = mcc::cli::globals()
        .top
        .clone()
        .unwrap_or_else(|| mcc::mcb_get_first_module_name().unwrap_or_else(|| "?".into()));
    let kind_str = args.kind.name();
    let kind_tag = args.kind.id();
    let format_tag = format.id();
    let (raw_text, items, count) = export::build_payload(
        &tree,
        &table,
        &arena,
        &inst_store,
        &top,
        kind_tag,
        format_tag,
    );

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
        output::emit_envelope(
            &env,
            format,
            mcc::cli::globals().output.as_deref().map(Path::new),
            false,
        )?;
    } else {
        // Raw text/CSV → stdout or file.
        match &mcc::cli::globals().output {
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
