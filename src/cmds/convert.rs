// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc convert` — Format conversion: mc → json / yaml (M5b).

use crate::cmds::common;
use crate::cmds::manifest;
use anyhow::Result;
use mcc::cli::{rpcclient::RpcClient, ConvertArgs};
use serde_json::Value;

pub fn run(args: &ConvertArgs) -> Result<()> {
    // An omitted target defaults to the current directory when it holds a
    // project manifest.
    let target = manifest::effective_target(args.file.as_deref());
    if let Some(c) = RpcClient::probe() {
        let params = serde_json::json!({
            "entry": target,
            "format": args.to,
        });
        match c.call("convert", params) {
            Ok(result) => {
                println!("{}", serde_json::to_string_pretty(&result)?);
                return Ok(());
            }
            Err(e) => tracing::debug!(target: "mcc::convert", "RPC failed, using local: {}", e),
        }
    }
    run_local(args, target.as_deref())
}

fn run_local(args: &ConvertArgs, target: Option<&str>) -> Result<()> {
    let Some(target) = target else {
        anyhow::bail!("convert: <target> not specified");
    };

    manifest::init_local(Some(target), &mcc::cli::globals().lib);

    // A directory target resolves to its manifest's entry file, which is then
    // loaded like an explicit file target.
    let (source, _) = common::load_target(
        Some(target),
        mcc::cli::globals().top.as_deref(),
        mcc::cli::globals().entry.as_deref(),
    )?;

    // Collect parsed definitions
    let components: Vec<Value> = mcc::mcb_iter_components()
        .into_iter()
        .map(|(n, u)| serde_json::json!({"name": n, "uri": u, "kind": "component"}))
        .collect();
    let modules: Vec<Value> = mcc::mcb_iter_modules()
        .into_iter()
        .map(|(n, u)| serde_json::json!({"name": n, "uri": u, "kind": "module"}))
        .collect();
    let interfaces: Vec<Value> = mcc::mcb_iter_interfaces()
        .into_iter()
        .map(|(n, u)| serde_json::json!({"name": n, "uri": u, "kind": "interface"}))
        .collect();
    let enums: Vec<Value> = mcc::mcb_iter_enums()
        .into_iter()
        .map(|(n, u)| serde_json::json!({"name": n, "uri": u, "kind": "enum"}))
        .collect();

    let result = serde_json::json!({
        "source": source,
        "definitions": {
            "components": components,
            "modules": modules,
            "interfaces": interfaces,
            "enums": enums,
        }
    });

    let output = match args.to.as_str() {
        "yaml" => serde_yaml::to_string(&result)?,
        _ => serde_json::to_string_pretty(&result)?,
    };

    if let Some(out_path) = &mcc::cli::globals().output {
        std::fs::write(out_path, output)?;
    } else {
        println!("{}", output);
    }
    Ok(())
}
