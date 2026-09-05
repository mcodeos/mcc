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
                // Deepened descriptor for catalog rules (design §8): the
                // explain view carries the same projection the `mcc rules`
                // detail view renders, plus the §8-5 allow syntax.
                if let Some(meta) = mcc::rules::find_rule(code) {
                    let desc = mcc::override_store::rule_descriptor_json(meta);
                    println!(
                        "  scope: {} / domain: {} / gate: {} / plane: {} / acceptance: {} / cadence: {} / fix: {}",
                        desc["scope"].as_str().unwrap_or("?"),
                        desc["domain"].as_str().unwrap_or("?"),
                        desc["gate"].as_str().unwrap_or("?"),
                        desc["plane"].as_str().unwrap_or("?"),
                        desc["acceptance"].as_str().unwrap_or("?"),
                        desc["cadence"].as_str().unwrap_or("?"),
                        desc["fix"].as_str().unwrap_or("?"),
                    );
                    if let Some(fam) = desc["family"].as_str() {
                        println!("  family: {fam}");
                    }
                    println!(
                        "  overridable: {}",
                        if desc["overridable"].as_bool().unwrap_or(false) {
                            "yes"
                        } else {
                            "no"
                        }
                    );
                    println!("  lock: {}", meta.lock);
                    println!("  doc: {}", meta.doc);
                    println!(
                        "  allow: `mcc rules set-severity {key} <hint|info|warning|error>` / `mcc rules allow {key} --path 'boards/**/*.mc' --reason ...`",
                        key = mcc::override_store::rule_key(code)
                    );
                }
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
