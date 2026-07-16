// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc check` — diagnostic output (envelope version)
//!
//! PR-2 Step 6 refactor: go through the envelope path, using
//! `output::diagnostic::from_mcc()` to replace `guess_severity()`.

use crate::cli::{rpc_client::RpcClient, CheckArgs};
use crate::cmds::manifest;
use crate::cmds::proj::resolve_workspace_ref;
use crate::output::{
    self,
    builder::ResultBuilder,
    diagnostic::{self, count_severity},
    envelope::{Envelope, Pass0Report, Phase, RpcError},
    OutputFormatExt,
};
use anyhow::Result;
use mcc::McURI;
use serde_json::json;
use std::path::{Path, PathBuf};

/// Controls the returned exit code: 0 = OK, 1 = has errors (or warnings under --strict)
pub struct CheckOutcome {
    pub exit_code: i32,
}

pub fn run(args: &CheckArgs) -> Result<CheckOutcome> {
    if let Some(client) = RpcClient::probe() {
        let result = client.call(
            "check",
            json!({
                "entry": args.target.clone(),
                "libs":  args.lib.clone(),
                "strict": args.strict,
                "errors_only": args.errors_only,
            }),
        )?;
        println!("{}", serde_json::to_string_pretty(&result)?);
        let code = result
            .get("summary")
            .and_then(|s| s.get("errors"))
            .and_then(|v| v.as_i64())
            .map(|n| if n > 0 { 1 } else { 0 })
            .unwrap_or(0);
        return Ok(CheckOutcome {
            exit_code: code as i32,
        });
    }

    mcc::mcc_init_no_lib();
    manifest::load_libs(&args.lib);

    let _uri: McURI = if let Some(t) = &args.target {
        let p = Path::new(t);
        if p.is_dir() {
            match manifest::build_from_manifest(p, None, None) {
                Ok((entry_uri, _)) => McURI::from(entry_uri.as_str()),
                Err(e) => {
                    if args.format.is_structured() {
                        let env = Envelope::err(RpcError::invalid_params(format!("{:#}", e)));
                        output::emit_envelope(&env, args.format, None, false)?;
                        return Ok(CheckOutcome { exit_code: 2 });
                    }
                    anyhow::bail!("check: {}", e);
                }
            }
        } else {
            let entry_path = Path::new(t);
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let mut search_dir: PathBuf = cwd.join(entry_path.parent().unwrap_or(entry_path));
            let project_root = loop {
                if search_dir.join("manifest.toml").exists() {
                    break search_dir;
                }
                match search_dir.parent() {
                    Some(parent) => {
                        if parent == search_dir {
                            break cwd.clone();
                        }
                        search_dir = parent.to_path_buf();
                    }
                    None => break cwd,
                }
            };

            let (entry_uri, _) = match manifest::build_from_manifest(&project_root, None, Some(t)) {
                Ok(r) => r,
                Err(e) => {
                    if args.format.is_structured() {
                        let env = Envelope::err(RpcError::invalid_params(format!("{:#}", e)));
                        output::emit_envelope(&env, args.format, None, false)?;
                        return Ok(CheckOutcome { exit_code: 2 });
                    }
                    anyhow::bail!("check: {}", e);
                }
            };

            McURI::from(entry_uri.as_str())
        }
    } else {
        if args.format.is_structured() {
            let env = Envelope::err(RpcError::invalid_params("check: <target> not specified"));
            output::emit_envelope(&env, args.format, None, false)?;
            return Ok(CheckOutcome { exit_code: 2 });
        }
        anyhow::bail!("check: <target> not specified");
    };

    // ── Nets flag: run pass2 and collect electrical checks ──
    if args.nets {
        let entry = mcc::McSpaceName {
            ident: mcc::McIds::from("main"),
            uri: _uri.clone(),
        };
        let mut errors = 0usize;
        if let Ok((_tree, table)) = mcc::mcb_pass2_flat(&entry, 1) {
            let net_results = mcc::check::nets::run_net_checks(&table);
            errors = net_results.iter().filter(|r| r.severity == "error").count();
            if !net_results.is_empty() {
                eprintln!(
                    "=== Electrical Net Checks ({} issues) ===",
                    net_results.len()
                );
                for r in &net_results {
                    eprintln!("  [{}] {}: {}", r.severity, r.check, r.message);
                }
            }
        }
        return Ok(CheckOutcome {
            exit_code: if errors > 0 { 1 } else { 0 },
        });
    }

    // ── Collect diagnostics (use the real from_mcc instead of guess_severity) ──
    // `check` is a diagnostic overview; there's no pass1/pass2 distinction,
    // so everything is attributed to Pass0.
    let raw = mcc::mcc_diagnose_all();
    let all_diags: Vec<_> = raw
        .iter()
        .map(|d| diagnostic::from_mcc(d, Phase::Pass0))
        .collect();

    // --errors-only filter
    let filtered: Vec<_> = if args.errors_only {
        all_diags
            .into_iter()
            .filter(|d| d.severity == crate::output::envelope::Severity::Error)
            .collect()
    } else {
        all_diags
    };

    let (error_count, warning_count) = count_severity(&filtered);

    // ── Build envelope ──
    let mut builder = ResultBuilder::start("mcc check").workspace(resolve_workspace_ref());

    builder.set_pass0(Pass0Report {
        loaded_files: vec![],
        diagnostics: filtered,
    });

    let env = Envelope::ok(builder.finish());
    output::emit_envelope(&env, args.format, None, false)?;

    // ── Text mode: print extra summary ──
    if !args.format.is_structured() {
        if error_count == 0 && warning_count == 0 {
            eprintln!("✓ check: no diagnostics");
        } else {
            eprintln!("check: {} errors, {} warnings", error_count, warning_count);
        }
    }

    let exit_code = if error_count > 0 || (args.strict && warning_count > 0) {
        1
    } else {
        0
    };
    Ok(CheckOutcome { exit_code })
}
