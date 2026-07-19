// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc explain` — Look up error code descriptions (M6).
//!
//! ```bash
//! mcc explain 1001    # single code
//! mcc explain         # list all known codes
//! ```

use anyhow::Result;
use mcc::cli::{rpcclient::RpcClient, ExplainArgs};
use mcc::errcodes;
use serde_json::{json, Value};

pub fn run(args: &ExplainArgs) -> Result<()> {
    if let Some(c) = RpcClient::probe() {
        let params = json!({ "code": args.code });
        match c.call("explain", params) {
            Ok(result) => {
                println!("{}", serde_json::to_string_pretty(&result)?);
                return Ok(());
            }
            Err(e) => tracing::debug!(target: "mcc::explain", "RPC failed, using local: {}", e),
        }
    }

    run_local(args)
}

fn run_local(args: &ExplainArgs) -> Result<()> {
    match args.code {
        Some(code) => match errcodes::describe(code) {
            Some(info) => {
                println!("Error {}: {}", info.code, info.name);
                println!("  {}", info.description);
            }
            None => {
                eprintln!("Unknown error code: {code}");
                eprintln!("Run `mcc explain` to see all known codes.");
                std::process::exit(1);
            }
        },
        None => {
            let all = errcodes::all_codes();
            let items: Vec<Value> = all
                .iter()
                .map(|e| {
                    json!({
                        "code": e.code,
                        "name": e.name,
                        "description": e.description,
                    })
                })
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({ "codes": items }))?
            );
        }
    }
    Ok(())
}
