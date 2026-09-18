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
use mcc::cli::CheckArgs;
use mcc::ledger;
use mcc::McURI;
use std::path::{Path, PathBuf};

/// Controls the returned exit code: 0 = OK, 1 = has errors (or warnings under --strict)
pub struct CheckOutcome {
    pub exit_code: i32,
}

/// What the whole run collected. A directory target checks several definition
/// spaces, and the report is about the folder, so each world's findings are
/// merged before anything is printed.
#[derive(Default)]
struct CheckBatch {
    diags: Vec<mcc::McDiagnostic>,
    net_errors: usize,
    pin_errors: usize,
}

/// Ask one definition space everything `check` asks, and keep the answer.
///
/// Called once for a file target and once per entry for a directory target —
/// while that entry's world is active, which is the only time it can be asked.
fn check_one_world(uri: &McURI, args: &CheckArgs, batch: &mut CheckBatch) {
    let mod_name = mcc::mcb_get_module_name_by_uri(uri)
        .or_else(|| mcc::mcb_get_first_module_name())
        .unwrap_or_else(|| "main".to_string());
    let entry = mcc::McSpaceName {
        ident: mcc::McIds::from(mod_name.as_str()),
        uri: mcc::uri_intern(uri),
    };

    // ── Nets flag: run pass2 and collect electrical checks ──
    if args.nets {
        if let Ok((_tree, table)) = mcc::mcb_pass2_flat(&entry, 1) {
            let net_results = mcc::check::nets::run_net_checks(&table);
            batch.net_errors += net_results.iter().filter(|r| r.severity == "error").count();
            // Same renderer and same row shape as `mcc build`'s section
            // (`output::net_check`) — one rule for the section's text.
            crate::output::net_check::render_section(&mcc::check::nets::net_check_rows(
                &net_results,
            ));
        }
        return;
    }

    // ── Pins flag: run pin usage checks ──
    if args.pins {
        match mcc::mcb_pass2_flat(&entry, 1) {
            Ok((_tree, table)) => {
                let pin_results = mcc::check::pins::run_pin_checks(&table);
                batch.pin_errors += pin_results.iter().filter(|r| r.severity == "error").count();
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
        return;
    }

    // ── Run pass2 instantiation so pass2-stage diagnostics (e.g. the func
    // method arity check E4176 — net-endpoint arguments must match the
    // formal count exactly) surface in the check overview too. pass2 errors
    // are recorded in the global store via diagnostic_log; a failed flat run
    // is tolerated so the overview still reports whatever pass1 collected.
    let _ = mcc::mcb_pass2_flat(&entry, 1);

    batch.diags.extend(mcc::mcc_diagnose_all());
}

/// Drop diagnostics a later entry reported again — a file reached by two
/// entries' `use` closures is parsed once per world, so its diagnostics arrive
/// once per entry that reaches it, and the report is about the folder.
fn dedup_mcc_diags(diags: Vec<mcc::McDiagnostic>) -> Vec<mcc::McDiagnostic> {
    let mut out: Vec<mcc::McDiagnostic> = Vec::new();
    for d in diags {
        let seen = out.iter().any(|e| {
            e.code == d.code && e.msg == d.msg && e.loc.uri == d.loc.uri && e.loc.pos == d.loc.pos
        });
        if !seen {
            out.push(d);
        }
    }
    out
}

pub fn run(args: &CheckArgs) -> Result<CheckOutcome> {
    // An omitted target defaults to the current directory when it holds a
    // project manifest.
    let target = manifest::effective_target(args.target.as_deref());

    // No server arm — ruled 2026-09-18 (mcd/CIMP.md §1 U90): carry the context
    // or don't delegate. The request carries `entry`/`libs`, but not the
    // caller's cwd, and `handle_check` resolves the entry against the daemon's
    // own `current_dir()` and reports on the daemon's own workspace — so the
    // same argv answered about two different worlds depending on whether a
    // daemon happened to be running.
    //
    // Worse, the project branch of `handle_check` forwards the request verbatim
    // to `handle_build_full`, so the server ran *build* and returned build's
    // payload: `strict` and `errors_only` were silently dropped, and the exit
    // code below came from build's summary rather than check's. Measured
    // `mcc check <fixture> -f json` on the hbl fixture: rc 2 in-process vs 1
    // over RPC, envelope -32602 vs -32603. Run in-process.

    // Fresh ledger per invocation: a long-lived server must not accumulate
    // stale rows across requests, and repeated CLI runs must be reproducible.
    ledger::clear();
    manifest::init_local(target.as_deref(), &mcc::cli::globals().lib);

    // Resolve the target.
    //   - Directory: a container of definition spaces (§19.5 rule 3 of
    //     use-design.md) — each entry is checked in its own world. Not a
    //     single-entry browse mode: a folder of unrelated `.mc` files must not
    //     be read as one project.
    //   - File: nearest project root (a directory with project.toml) is
    //     resolved by walking up, then the manifest (if any) drives the load.
    let dir_root: Option<PathBuf> = target
        .as_deref()
        .map(Path::new)
        .filter(|p| p.is_dir())
        .map(Path::to_path_buf);

    let _uri: McURI = if dir_root.is_some() {
        McURI::from("")
    } else if let Some(t) = &target {
        let p = Path::new(t);
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

        let (entry_uri, _) =
            match manifest::build_from_manifest(&project_root, None, Some(abs_t_str.as_str())) {
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
    } else {
        if mcc::cli::globals().format.is_structured() {
            let env = Envelope::err(RpcError::invalid_params("check: <target> not specified"));
            output::emit_envelope(&env, mcc::cli::globals().format, None, false)?;
            return Ok(CheckOutcome { exit_code: 2 });
        }
        anyhow::bail!("check: <target> not specified");
    };

    // ── Check each definition space. A directory target runs once per entry,
    //    and everything a world can be asked has to be asked inside it: the
    //    next entry drops it, diagnostics and all. ──
    let mut batch = CheckBatch::default();
    match &dir_root {
        Some(root) => {
            if mcc::discover_entries(root, mcc::cli::globals().entry.as_deref()).is_empty() {
                if mcc::cli::globals().format.is_structured() {
                    let env = Envelope::err(RpcError::invalid_params(format!(
                        "check: no `.mc` files found under {}",
                        root.display()
                    )));
                    output::emit_envelope(&env, mcc::cli::globals().format, None, false)?;
                    return Ok(CheckOutcome { exit_code: 2 });
                }
                anyhow::bail!("check: no `.mc` files found under {}", root.display());
            }
            let lib = mcc::cli::globals().lib.clone();
            let entry_override = mcc::cli::globals().entry.clone();
            mcc::mcc_for_each_entry(
                root,
                entry_override.as_deref(),
                &|e| manifest::collect_libs(Some(&e.scope), &lib),
                |e| {
                    check_one_world(
                        &McURI::from(e.entry.to_string_lossy().as_ref()),
                        args,
                        &mut batch,
                    )
                },
            );
        }
        None => check_one_world(&_uri, args, &mut batch),
    }

    // ── Nets flag: pass2 already ran per entry; report the aggregate. ──
    if args.nets {
        return Ok(CheckOutcome {
            exit_code: if batch.net_errors > 0 { 1 } else { 0 },
        });
    }

    // ── Pins flag: likewise. ──
    if args.pins {
        return Ok(CheckOutcome {
            exit_code: if batch.pin_errors > 0 { 1 } else { 0 },
        });
    }

    // ── Collect diagnostics (use the real from_mcc instead of guess_severity) ──
    // `check` is a diagnostic overview; pass2 diagnostics are attributed to
    // Pass0 in the report.
    let raw = dedup_mcc_diags(batch.diags);

    // --dlog: raw one-line diagnostics only (no envelope / summary). Decoupled
    // from execution mode — pair with --local when an RPC server is running.
    if args.dlog {
        diagnostic::print_dlog_of(&raw, args.errors_only);
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
