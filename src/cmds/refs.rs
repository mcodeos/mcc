// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc refs` — Find all references to a symbol (M6).
//!
//! Requires the engine to have collected reference data during Pass1/Pass2.

use crate::output::{emit_projection, OutputFormatExt, ProjectionKey};
use anyhow::Result;
use mcc::cli::{rpcclient::RpcClient, RefsArgs};
use serde_json::{json, Value};
use std::path::Path;

pub fn run(args: &RefsArgs) -> Result<()> {
    if let Some(c) = RpcClient::probe() {
        let params = json!({ "name": args.name });
        match c.call("refs", params) {
            Ok(result) => return emit_refs(result),
            Err(e) => tracing::debug!(target: "mcc::refs", "RPC failed, using local: {}", e),
        }
    }

    run_local(args)
}

/// Structured face → A-tier envelope (U86 item 7, first slice); text / csv are
/// untouched (`refs` was format-blind before the slice — see `emit_report`'s
/// note).
fn emit_refs(data: Value) -> Result<()> {
    if mcc::cli::globals().format.is_structured() {
        return emit_projection(
            ProjectionKey::Refs,
            data,
            mcc::cli::globals().format,
            mcc::cli::globals().output.as_deref().map(Path::new),
        );
    }
    println!("{}", serde_json::to_string_pretty(&data)?);
    Ok(())
}

fn run_local(args: &RefsArgs) -> Result<()> {
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
            let uri = mcc::McURI::from(f);
            mcc::mcc_load_project(&uri);
        }
    }

    let refs = mcc::mcb_get_refs(&args.name);

    let items: Vec<_> = refs
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

    emit_refs(json!({
        "name": args.name,
        "count": items.len(),
        "refs": items,
    }))
}
