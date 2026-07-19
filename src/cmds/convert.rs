// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc convert` — Format conversion: mc → json / yaml (M5b).

use crate::cmds::manifest;
use anyhow::Result;
use mcc::cli::{rpcclient::RpcClient, ConvertArgs};
use mcc::McURI;
use serde_json::Value;
use std::path::Path;

pub fn run(args: &ConvertArgs) -> Result<()> {
    if let Some(c) = RpcClient::probe() {
        let params = serde_json::json!({
            "entry": args.file,
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
    run_local(args)
}

fn run_local(args: &ConvertArgs) -> Result<()> {
    mcc::mcc_init_no_lib();
    manifest::load_libs(&args.lib);

    let path = Path::new(&args.file);
    let uri = if path.is_absolute() {
        McURI::from(path.to_string_lossy().as_ref())
    } else {
        let cwd = std::env::current_dir().unwrap_or_default();
        McURI::from(cwd.join(path).to_string_lossy().as_ref())
    };
    mcc::mcc_load_project(&uri);

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
        "source": args.file,
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

    if let Some(out_path) = &args.output {
        std::fs::write(out_path, output)?;
    } else {
        println!("{}", output);
    }
    Ok(())
}
