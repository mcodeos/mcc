// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc report` — Structured design summary (M5b).

use crate::cli::{rpc_client::RpcClient, ReportArgs};
use crate::cmds::manifest;
use anyhow::Result;
use mcc::McURI;
use serde_json::json;
use std::path::Path;

pub fn run(args: &ReportArgs) -> Result<()> {
    if let Some(c) = RpcClient::probe() {
        match c.call("report", json!({ "entry": args.target })) {
            Ok(result) => {
                println!("{}", serde_json::to_string_pretty(&result)?);
                return Ok(());
            }
            Err(e) => tracing::debug!(target: "mcc::report", "RPC failed: {}", e),
        }
    }
    run_local(args)
}

fn run_local(args: &ReportArgs) -> Result<()> {
    mcc::mcc_init_no_lib();
    manifest::load_libs(&args.lib);

    if let Some(t) = &args.target {
        let p = Path::new(t);
        let uri = if p.is_absolute() {
            McURI::from(p.to_string_lossy().as_ref())
        } else {
            let cwd = std::env::current_dir().unwrap_or_default();
            McURI::from(cwd.join(p).to_string_lossy().as_ref())
        };
        mcc::mcc_load_project(&uri);
    }

    let comps = mcc::mcb_iter_components();
    let mods = mcc::mcb_iter_modules();
    let ifaces = mcc::mcb_iter_interfaces();
    let enums = mcc::mcb_iter_enums();

    // Aggregate components by prefix (R, C, L, D, X, etc.)
    let mut by_prefix: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for (name, _) in &comps {
        let prefix = name.chars().next().map(|c| c.to_string()).unwrap_or_else(|| "?".into());
        *by_prefix.entry(prefix).or_default() += 1;
    }

    let result = json!({
        "command": "report",
        "summary": {
            "component_count": comps.len(),
            "module_count": mods.len(),
            "interface_count": ifaces.len(),
            "enum_count": enums.len(),
            "total_definitions": comps.len() + mods.len() + ifaces.len() + enums.len(),
        },
        "components_by_prefix": by_prefix,
        "components": comps.iter().take(20).map(|(n, u)| json!({"name": n, "uri": u})).collect::<Vec<_>>(),
        "modules": mods.iter().take(10).map(|(n, u)| json!({"name": n, "uri": u})).collect::<Vec<_>>(),
        "interfaces": ifaces.iter().take(10).map(|(n, u)| json!({"name": n, "uri": u})).collect::<Vec<_>>(),
        "enums": enums.iter().map(|(n, u)| json!({"name": n, "uri": u})).collect::<Vec<_>>(),
    });

    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
