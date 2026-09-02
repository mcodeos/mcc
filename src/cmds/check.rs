// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc check` — diagnostic output (envelope version)
//!
//! PR-2 Step 6 refactor: go through the envelope path, using
//! `output::diagnostic::from_mcc()` to replace `guess_severity()`.

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
use mcc::cli::{rpcclient::RpcClient, CheckArgs};
use mcc::ledger;
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
                "libs":  mcc::cli::globals().lib.clone(),
                "strict": mcc::cli::globals().strict,
                "errors_only": args.errors_only,
                "ledger": args.ledger.clone(),
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

    // Fresh ledger per invocation: a long-lived server must not accumulate
    // stale rows across requests, and repeated CLI runs must be reproducible.
    ledger::clear();
    manifest::init_local(args.target.as_deref(), &mcc::cli::globals().lib);

    // Resolve the target into an entry URI.
    //   - Directory: manifest-driven project mode; falls back to browse mode
    //     (§19.5 rule 3 of use-design.md) when the directory has no manifest,
    //     using the unique `module main` file or --entry.
    //   - File: nearest project root (a directory with project.toml) is
    //     resolved by walking up, then the manifest (if any) drives the load.
    let _uri: McURI = if let Some(t) = &args.target {
        let p = Path::new(t);
        if p.is_dir() {
            let fail = |e: anyhow::Error| -> Result<CheckOutcome> {
                if mcc::cli::globals().format.is_structured() {
                    let env = Envelope::err(RpcError::invalid_params(format!("{:#}", e)));
                    output::emit_envelope(&env, mcc::cli::globals().format, None, false)?;
                    Ok(CheckOutcome { exit_code: 2 })
                } else {
                    anyhow::bail!("check: {}", e);
                }
            };
            match crate::cmds::common::load_target(
                Some(t),
                mcc::cli::globals().top.as_deref(),
                mcc::cli::globals().entry.as_deref(),
            ) {
                Ok((entry_uri, _)) => McURI::from(entry_uri.as_str()),
                Err(e) => return fail(e),
            }
        } else {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let abs_t = if p.is_absolute() {
                p.to_path_buf()
            } else {
                cwd.join(p)
            };
            let abs_t_str = abs_t.to_string_lossy().to_string();
            let project_root = match manifest::find_project_root(Some(abs_t_str.as_str())) {
                Some(root) => root,
                None => {
                    if mcc::cli::globals().format.is_structured() {
                        let env = Envelope::err(RpcError::invalid_params(format!(
                            "check: cannot resolve project root for {}",
                            t
                        )));
                        output::emit_envelope(&env, mcc::cli::globals().format, None, false)?;
                        return Ok(CheckOutcome { exit_code: 2 });
                    }
                    anyhow::bail!("check: cannot resolve project root for {}", t);
                }
            };

            let (entry_uri, _) = match manifest::build_from_manifest(
                &project_root,
                None,
                Some(abs_t_str.as_str()),
            ) {
                Ok(r) => r,
                Err(e) => {
                    if mcc::cli::globals().format.is_structured() {
                        let env = Envelope::err(RpcError::invalid_params(format!("{:#}", e)));
                        output::emit_envelope(&env, mcc::cli::globals().format, None, false)?;
                        return Ok(CheckOutcome { exit_code: 2 });
                    }
                    anyhow::bail!("check: {}", e);
                }
            };

            McURI::from(entry_uri.as_str())
        }
    } else {
        if mcc::cli::globals().format.is_structured() {
            let env = Envelope::err(RpcError::invalid_params("check: <target> not specified"));
            output::emit_envelope(&env, mcc::cli::globals().format, None, false)?;
            return Ok(CheckOutcome { exit_code: 2 });
        }
        anyhow::bail!("check: <target> not specified");
    };

    // ── Nets flag: run pass2 and collect electrical checks ──
    if args.nets {
        let mod_name = mcc::mcb_get_module_name_by_uri(&_uri)
            .or_else(|| mcc::mcb_get_first_module_name())
            .unwrap_or_else(|| "main".to_string());
        let entry = mcc::McSpaceName {
            ident: mcc::McIds::from(mod_name.as_str()),
            uri: mcc::uri_intern(&_uri),
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

    // ── Pins flag: run pin usage checks ──
    if args.pins {
        let mod_name = mcc::mcb_get_module_name_by_uri(&_uri)
            .or_else(|| mcc::mcb_get_first_module_name())
            .unwrap_or_else(|| "main".to_string());
        let entry = mcc::McSpaceName {
            ident: mcc::McIds::from(mod_name.as_str()),
            uri: mcc::uri_intern(&_uri),
        };
        let mut errors = 0usize;
        match mcc::mcb_pass2_flat(&entry, 1) {
            Ok((_tree, table)) => {
                let pin_results = mcc::check::pins::run_pin_checks(&table);
                errors = pin_results.iter().filter(|r| r.severity == "error").count();
                if !pin_results.is_empty() {
                    eprintln!("=== Pin Usage Checks ({} issues) ===", pin_results.len());
                    for r in &pin_results {
                        eprintln!("  [{}] {}: {}", r.severity, r.check, r.message);
                    }
                }
            }
            Err(e) => {
                eprintln!("Pin checks skipped: pass2 failed: {e}");
            }
        }
        return Ok(CheckOutcome {
            exit_code: if errors > 0 { 1 } else { 0 },
        });
    }

    // ── Run pass2 instantiation so pass2-stage diagnostics (e.g. the func
    // method arity check E4176 — net-endpoint arguments must match the
    // formal count exactly) surface in the check overview too. pass2 errors
    // are recorded in the global store via diagnostic_log; a failed flat run
    // is tolerated so the overview still reports whatever pass1 collected.
    if let Some(mod_name) = mcc::mcb_get_module_name_by_uri(&_uri)
        .or_else(|| mcc::mcb_get_first_module_name())
        .or_else(|| Some("main".to_string()))
    {
        let entry = mcc::McSpaceName {
            ident: mcc::McIds::from(mod_name.as_str()),
            uri: mcc::uri_intern(&_uri),
        };
        let _ = mcc::mcb_pass2_flat(&entry, 1);
    }

    // ── Collect diagnostics (use the real from_mcc instead of guess_severity) ──
    // `check` is a diagnostic overview; pass2 diagnostics are attributed to
    // Pass0 in the report.
    let raw = mcc::mcc_diagnose_all();

    // --dlog: raw one-line diagnostics only (no envelope / summary). Decoupled
    // from execution mode — pair with --local when an RPC server is running.
    if args.dlog {
        diagnostic::print_dlog_lines(args.errors_only);
        let errs = raw
            .iter()
            .filter(|d| matches!(d.level, mcc::DiagnosticLevel::Error))
            .count();
        let warns = if args.errors_only {
            0
        } else {
            raw.iter()
                .filter(|d| matches!(d.level, mcc::DiagnosticLevel::Warning))
                .count()
        };
        let exit_code = if errs > 0 || (mcc::cli::globals().strict && warns > 0) {
            1
        } else {
            0
        };
        return Ok(CheckOutcome { exit_code });
    }

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

    // ── Failure ledger snapshot (resolve-gate §7.1): every recording point has
    // fired by now — Wire during parse (component-finish recheck), Phantom
    // during the pass2 flat run above. Observation-only; never affects exit
    // code or diagnostics. `--ledger` opens per-row detail (excluding the
    // Deferred/ResolvedMany audit rows); `--ledger=audit` includes them. ──
    let ledger_mode = ledger::LedgerMode::from_flag(args.ledger.as_deref());
    let ledger_report = ledger::build_report(ledger_mode);

    // ── Build envelope ──
    let mut builder = ResultBuilder::start("mcc check").workspace(resolve_workspace_ref());

    builder.set_pass0(Pass0Report {
        loaded_files: vec![],
        diagnostics: filtered,
    });
    builder.set_ledger(ledger_report.clone());

    let env = Envelope::ok(builder.finish());
    output::emit_envelope(&env, mcc::cli::globals().format, None, false)?;

    // ── Text mode: print extra summary ──
    if !mcc::cli::globals().format.is_structured() {
        if error_count == 0 && warning_count == 0 {
            eprintln!("✓ check: no diagnostics");
        } else {
            eprintln!("check: {} errors, {} warnings", error_count, warning_count);
        }
        if args.ledger.is_some() {
            print_ledger(&ledger_report);
        }
    }

    let exit_code = if error_count > 0 || (mcc::cli::globals().strict && warning_count > 0) {
        1
    } else {
        0
    };
    Ok(CheckOutcome { exit_code })
}

/// Text-mode rendering of the failure ledger (resolve-gate-design.md §7.1-2):
/// summary counts (kind×form) always; per-row detail only when the row list was
/// requested (`--ledger`). Rows that had no source node show `-` for location.
fn print_ledger(report: &ledger::LedgerReport) {
    eprintln!("── Failure Ledger (resolve-gate §1) ─────────────────────────────");
    eprintln!(
        "  total: {} (survived: {}, deferred resolved late: {})",
        report.total, report.survived, report.resolved_late
    );
    for (kind, forms) in &report.by_kind_form {
        if forms.is_empty() {
            continue;
        }
        let count: usize = forms.values().sum();
        let forms = forms
            .iter()
            .map(|(f, c)| format!("{f}×{c}"))
            .collect::<Vec<_>>()
            .join(", ");
        eprintln!("  {kind:<14} {count:<5} [{forms}]");
    }
    if !report.detail.is_empty() {
        eprintln!("── detail ─────────────────────────────────────────────────────");
        for row in &report.detail {
            let loc = match (&row.file, row.line, row.column) {
                (Some(f), Some(l), Some(c)) => format!("{f}:{l}:{c}"),
                _ => "-".to_string(),
            };
            eprintln!(
                "  {:<14} {:<24} {:<24} {:<8} {}",
                row.kind, row.form, row.site, row.action, loc
            );
        }
    }
}
