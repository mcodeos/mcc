// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc query <EXPR>` — thin CLI wrapper over `mcc::query_api`.
//!
//! Mirrors `cmds/search.rs` structure: probe RPC, fall through to local.
//! For v1 the CLI is local-only (no server-side eval yet); the `defs.query`
//! RPC handler in `rpc/handlers.rs` provides the same capability to IDE/LSP.

use crate::cli::{rpc_client::RpcClient, QueryArgs};
use crate::cmds::manifest;
use crate::output::{
    self,
    builder::ResultBuilder,
    envelope::{Envelope, QueryData},
    OutputFormatExt,
};
use anyhow::Result;
use serde_json::{json, Value};
use std::path::Path;

pub fn run(args: &QueryArgs) -> Result<()> {
    // Pattern B: probe + rpc_mapping + fallthrough to local.
    if let Some(c) = RpcClient::probe() {
        if let Some((method, params)) = rpc_mapping(args) {
            match c.call(method, params) {
                Ok(result) => {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                    return Ok(());
                }
                Err(e) => tracing::debug!(
                    target: "mcc::query",
                    "RPC failed, falling back to local: {}",
                    e
                ),
            }
        }
    }
    run_local(args)
}

/// Map CLI args → RPC method + params. Returns `None` for now (query is
/// local-only on the CLI; `defs.query` exists for direct RPC users).
fn rpc_mapping(args: &QueryArgs) -> Option<(&'static str, Value)> {
    if std::env::var("MCC_RPC_QUERY").is_ok() {
        Some((
            "defs.query",
            json!({
                "expr": args.expr,
                "limit": args.limit,
            }),
        ))
    } else {
        None
    }
}

fn run_local(args: &QueryArgs) -> Result<()> {
    mcc::mcc_init_no_lib();
    if !args.lib.is_empty() {
        manifest::load_libs(&args.lib);
    }
    if let Some(target) = &args.target {
        let path = Path::new(target);
        if path.is_dir() {
            let _ = mcc::mcc_load_project(&mcc::McURI::from(target.as_str()));
        } else {
            mcc::mcc_load_project(&mcc::McURI::from(target.as_str()));
        }
    }

    // Compile the query expression once.
    let query = mcc::query_api::compile(&args.expr)?;

    // Build all-defs SearchInputs (empty pattern, kind=None → all kinds).
    let inputs = mcc::search_api::SearchInputs {
        pattern: String::new(),
        kind: None,
        regex: false,
        fuzzy: false,
        top: None,
        limit: args.limit,
        libs: args.lib.clone(),
    };
    let hits = mcc::search_api::walk_defs(&inputs, Some(&query))?;

    let items: Vec<Value> = hits
        .iter()
        .map(|h| {
            let mut v = json!({
                "kind": h.kind,
                "name": h.name,
                "uri": h.uri,
            });
            if let Some(c) = &h.class {
                v["class"] = json!(c);
            }
            v
        })
        .collect();
    let count = items.len();

    let data = QueryData {
        expr: args.expr.clone(),
        count,
        items: Value::Array(items.clone()),
    };

    let format = if args.json {
        crate::cli::OutputFormat::Json
    } else {
        args.format
    };

    let mut builder = ResultBuilder::start("mcc query");
    builder.set_query(data);
    let env = Envelope::ok(builder.finish());
    output::emit_envelope(
        &env,
        format,
        args.output.as_deref().map(std::path::Path::new),
        false,
    )?;

    // Text mode convention: one hit per line on stdout, count to stderr.
    if !format.is_structured() {
        for it in &items {
            let kind = it.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
            let name = it.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let class = it.get("class").and_then(|v| v.as_str());
            match class {
                Some(c) => println!("{}\t{}\t{}", kind, name, c),
                None => println!("{}\t{}", kind, name),
            }
        }
        eprintln!("({} items)", count);
    }
    Ok(())
}
