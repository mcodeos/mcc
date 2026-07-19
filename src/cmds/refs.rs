// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc refs` — Find all references to a symbol (M6).
//!
//! Requires the engine to have collected reference data during Pass1/Pass2.

use anyhow::Result;
use mcc::cli::{rpc_client::RpcClient, RefsArgs};
use serde_json::json;

pub fn run(args: &RefsArgs) -> Result<()> {
    if let Some(c) = RpcClient::probe() {
        let params = json!({ "name": args.name });
        match c.call("refs", params) {
            Ok(result) => {
                println!("{}", serde_json::to_string_pretty(&result)?);
                return Ok(());
            }
            Err(e) => tracing::debug!(target: "mcc::refs", "RPC failed, using local: {}", e),
        }
    }

    run_local(args)
}

fn run_local(args: &RefsArgs) -> Result<()> {
    mcc::mcc_init_no_lib();
    if !args.lib.is_empty() {
        crate::cmds::manifest::load_libs(&args.lib);
    }
    if let Some(f) = &args.file {
        let uri = mcc::McURI::from(f.as_str());
        mcc::mcc_load_project(&uri);
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

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "name": args.name,
            "count": items.len(),
            "refs": items,
        }))?
    );
    Ok(())
}
