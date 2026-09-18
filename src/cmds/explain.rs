// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc explain` — Look up error code descriptions (M6).
//!
//! ```bash
//! mcc explain 1001    # single code
//! mcc explain         # list all known codes
//! ```

use crate::output::{emit_failure_envelope, emit_projection, OutputFormatExt, ProjectionKey};
use anyhow::Result;
use mcc::cli::{rpcclient::RpcClient, ExplainArgs};
use mcc::errcodes;
use serde_json::{json, Value};
use std::path::Path;

pub fn run(args: &ExplainArgs) -> Result<()> {
    if let Some(c) = RpcClient::probe() {
        let params = json!({ "code": args.code });
        match c.call("explain", params) {
            Ok(result) => return emit_explain(result),
            Err(e) => tracing::debug!(target: "mcc::explain", "RPC failed, using local: {}", e),
        }
    }

    run_local(args)
}

/// Structured face → A-tier envelope (U86 item 7, first slice); text / csv are
/// untouched.
///
/// ⚠ Two things this word does **not** fix, both recorded rather than papered
/// over (mcd/log/9.18.cli-output-face-inventory.md §2):
///
/// 1. `explain` was format-blind in *both* directions — the no-argument branch
///    printed pretty JSON in every format, the single-code branch printed text
///    in every format. Only the machine face of the first is wrapped here.
/// 2. `mcc explain <CODE> -f json` therefore still prints **text**: that branch
///    has no JSON payload to wrap, and inventing one means choosing its shape
///    against the RPC handler's richer one (`aicontract.rs`) — a payload-design
///    step, not a wrapping step. Left for the follow-up slice.
fn emit_explain(data: Value) -> Result<()> {
    if mcc::cli::globals().format.is_structured() {
        return emit_projection(
            ProjectionKey::Explain,
            data,
            mcc::cli::globals().format,
            mcc::cli::globals().output.as_deref().map(Path::new),
        );
    }
    println!("{}", serde_json::to_string_pretty(&data)?);
    Ok(())
}

fn run_local(args: &ExplainArgs) -> Result<()> {
    match args.code {
        // Single-code face: text in every format (see `emit_explain`).
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
                // Raw stderr with no tracing behind it, so `die!` cannot be used
                // here: it would add a log line. Emit the envelope, then leave
                // this face's bytes exactly as they were.
                let msg = format!(
                    "Unknown error code: {code}\nRun `mcc explain` to see all known codes."
                );
                if let Err(e) = emit_failure_envelope(&msg) {
                    eprintln!("warning: failed to emit the failure envelope: {e}");
                }
                eprintln!("{msg}");
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
            emit_explain(json!({ "codes": items }))?;
        }
    }
    Ok(())
}
