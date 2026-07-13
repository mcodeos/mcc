// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc search <pattern>` — thin CLI wrapper around `mcc::search_api`.
//!
//! The actual walk/filter/matcher logic lives in the library (`src/search_api.rs`)
//! so the `defs.search` RPC handler (`rpc/handlers.rs`) can share the exact
//! same code without reaching into the binary's private `cmds` module.

use crate::cli::{rpc_client::RpcClient, SearchArgs, SearchKind as CliSearchKind};
use crate::cmds::manifest;
use crate::output::{
    self,
    builder::ResultBuilder,
    envelope::{Envelope, SearchData},
    OutputFormatExt,
};
use anyhow::Result;
use serde_json::{json, Value};

pub fn run(args: &SearchArgs) -> Result<()> {
    // Pattern B: probe + rpc_mapping + fallthrough to local.
    if let Some(c) = RpcClient::probe() {
        if let Some((method, params)) = rpc_mapping(args) {
            match c.call(method, params) {
                Ok(result) => {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                    return Ok(());
                }
                Err(e) => tracing::debug!(
                    target: "mcc::search",
                    "RPC failed, falling back to local: {}",
                    e
                ),
            }
        }
    }
    run_local(args)
}

/// Map CLI args → RPC method + params. Returns `None` for now (search is
/// local-only on the CLI; the `defs.search` server method exists for IDE/LSP
/// callers and direct RPC).
fn rpc_mapping(args: &SearchArgs) -> Option<(&'static str, Value)> {
    // CLI search falls through to local; RPC users (LSP/IDE/scripts) call
    // `defs.search` directly. If we later want CLI → RPC, flip this to Some.
    if args.lib.is_empty() && std::env::var("MCC_RPC_SEARCH").is_ok() {
        Some((
            "defs.search",
            json!({
                "pattern": args.pattern,
                "kind": args.kind.map(|k| match k {
                    CliSearchKind::Component => "component",
                    CliSearchKind::Module => "module",
                    CliSearchKind::Interface => "interface",
                    CliSearchKind::Enum => "enum",
                    CliSearchKind::Instance => "instance",
                }),
                "regex": args.regex,
                "fuzzy": args.fuzzy,
                "top": args.top,
                "limit": args.limit,
            }),
        ))
    } else {
        None
    }
}

fn run_local(args: &SearchArgs) -> Result<()> {
    mcc::mcc_init_no_lib();
    if !args.lib.is_empty() {
        manifest::load_libs(&args.lib);
    }
    // Optional target load — required for `--kind instance --top <NAME>` so the
    // named module is in scope for this CLI invocation (workspace is per-process).
    if let Some(target) = &args.target {
        let path = std::path::Path::new(target);
        if path.is_dir() {
            // Walk for *.mc files and load the first module-containing one.
            // For deeper support, `parse <dir>` is the right tool; here we
            // accept a directory and feed it to mcc_load_project which
            // walks internally.
            let _ = mcc::mcc_load_project(&mcc::McURI::from(target.as_str()));
        } else {
            mcc::mcc_load_project(&mcc::McURI::from(target.as_str()));
        }
    }

    let inputs = mcc::search_api::SearchInputs {
        pattern: args.pattern.clone(),
        kind: args.kind.map(cli_to_api_kind),
        regex: args.regex,
        fuzzy: args.fuzzy,
        top: args.top.clone(),
        limit: args.limit,
        libs: args.lib.clone(),
    };
    let hits = mcc::search_api::walk_defs(&inputs)?;

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

    let data = SearchData {
        pattern: inputs.pattern.clone(),
        kind: inputs.kind_str(),
        regex: inputs.regex,
        fuzzy: inputs.fuzzy,
        count,
        items: Value::Array(items.clone()),
    };

    let format = if args.json {
        crate::cli::OutputFormat::Json
    } else {
        args.format
    };

    let mut builder = ResultBuilder::start("mcc search");
    builder.set_search(data);
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

fn cli_to_api_kind(k: CliSearchKind) -> mcc::search_api::SearchKind {
    match k {
        CliSearchKind::Component => mcc::search_api::SearchKind::Component,
        CliSearchKind::Module => mcc::search_api::SearchKind::Module,
        CliSearchKind::Interface => mcc::search_api::SearchKind::Interface,
        CliSearchKind::Enum => mcc::search_api::SearchKind::Enum,
        CliSearchKind::Instance => mcc::search_api::SearchKind::Instance,
    }
}
