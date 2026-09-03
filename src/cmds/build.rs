// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc build` — one-shot build (manifest-driven)
//!
//! Read manifest → auto-load dependencies → Pass1 + Pass2 → envelope.
//!
//! ## Output funnel
//!
//! Both the local and the RPC (server-forwarded) path emit the same envelope
//! via [`output::emit_envelope`]: text mode renders the detailed report
//! (definitions / instance tree / connections / nets / net summary), and
//! `--format json/yaml` serializes the envelope. The text renderer is
//! data-driven (reads only the envelope), so `-f text` output is identical
//! local ↔ server. The design contract (manual-v2) is that command and
//! output are fully identical, so the RPC result is realigned to the local
//! command/workspace and emitted with the identical renderer.
//!
//! ## Exit code
//!
//! `run` returns `(Result<()>, usize)`: success/failure + error count.
//! `dispatch` sets `exit_code` based on this (aligned with `check`). RPC mode
//! now propagates the summary's error count instead of hard-coding 0.

use crate::cmds::manifest;
use crate::cmds::proj::resolve_workspace_ref;
use crate::output::{
    self, builder::ResultBuilder, diagnostic::PhaseTracker, envelope::*, OutputFormatExt,
};
use anyhow::{Context, Result};
use mcc::cli::{rpcclient::RpcClient, BuildArgs, OutputFormat};
use mcc::mcc_dbg;
use mcc::viz::layout::FlowLayouter;
use serde_json::json;
use std::path::{Path, PathBuf};

/// Build command return result (includes exit_code; aligned with `check`)
pub struct BuildOutcome {
    pub exit_code: i32,
}

/// Resolve the entry file given on the CLI: the positional `FILE` wins over
/// the global `--entry` flag; both override the manifest entry. A directory
/// positional is a project-root target, not an entry file, so it is excluded
/// here and handled by `resolve_project_root`.
fn cli_entry(args: &BuildArgs) -> Option<String> {
    let positional = args.file.as_deref().filter(|f| !Path::new(f).is_dir());
    positional
        .map(|s| s.to_string())
        .or_else(|| mcc::cli::globals().entry.clone())
}

pub fn run(args: &BuildArgs) -> Result<BuildOutcome> {
    match RpcClient::probe() {
        Some(c) => run_rpc(&c, args),
        None => run_local(args),
    }
}

fn resolve_project_root(args: &BuildArgs) -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    // A directory positional is the project root itself (manifest-driven or
    // browse fallback), like `parse <dir>` / `show -F <dir>`.
    if let Some(f) = args.file.as_deref() {
        let p = Path::new(f);
        if p.is_dir() {
            return if p.is_absolute() {
                p.to_path_buf()
            } else {
                cwd.join(p)
            };
        }
    }
    if let Some(entry) = cli_entry(args) {
        let entry_path = Path::new(&entry);
        let mut search_dir = cwd.join(entry_path.parent().unwrap_or(entry_path));
        loop {
            if manifest::Manifest::find_in(&search_dir).is_some() {
                return search_dir;
            }
            match search_dir.parent() {
                Some(parent) => {
                    if parent == search_dir {
                        return cwd;
                    }
                    search_dir = parent.to_path_buf();
                }
                None => return cwd,
            }
        }
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }
}

fn run_rpc(c: &RpcClient, args: &BuildArgs) -> Result<BuildOutcome> {
    let project_root = resolve_project_root(args);
    let manifest =
        manifest::Manifest::find_in(&project_root).and_then(|p| manifest::Manifest::load(&p).ok());
    let entry_abs = if let Some(e) = cli_entry(args) {
        project_root.join(e)
    } else if let Some(ref m) = manifest {
        m.entry_path(&project_root)
    } else {
        project_root.clone()
    };
    let libs: Vec<String> = manifest
        .as_ref()
        .map(|m| m.dependencies.keys().cloned().collect())
        .unwrap_or_default();

    match c.call(
        "build.full",
        json!({
            "entry": entry_abs.to_string_lossy(),
            "top": mcc::cli::globals().top,
            "libs": libs,
            "include_system": args.include_system,
        }),
    ) {
        Ok(result) => Ok(BuildOutcome {
            exit_code: emit_build_result(result)?,
        }),
        Err(e) => {
            tracing::debug!(target: "mcc::build", "RPC failed, using local: {}", e);
            run_local(args)
        }
    }
}

/// Render an RPC `build.full` result exactly like the local path, per `--format`.
///
/// The server returns the [`CommandResult`] body; `command` / `workspace` are
/// process-local truth here, so they are realigned to what a local build would
/// emit (matching the design contract that output matches the local path). Exit code follows
/// the summary's error count (previously RPC mode hard-coded 0).
fn emit_build_result(result: serde_json::Value) -> Result<i32> {
    let format = mcc::cli::globals().format;
    let target = mcc::cli::globals().output.as_deref().map(Path::new);

    let mut r = result;
    r["command"] = json!("mcc build");
    r["workspace"] = serde_json::to_value(resolve_workspace_ref())?;

    let cmd: CommandResult = serde_json::from_value(r)?;
    let env = Envelope::ok(cmd);
    let errors = env.result.as_ref().map(|x| x.summary.errors).unwrap_or(0);
    output::emit_envelope(&env, format, target, false)?;
    Ok(if errors > 0 { 1 } else { 0 })
}

fn run_local(args: &BuildArgs) -> Result<BuildOutcome> {
    // Reset R05 counter before each build run
    mcc::instant::reset_r05_counter();

    let mut builder = ResultBuilder::start("mcc build").workspace(resolve_workspace_ref());
    let mut tracker = PhaseTracker::new();
    tracker.skip();

    // ── 0. Initialize system root (same as parse command) ──
    // Reset the engine first: when mcc is embedded as a library (or a prior
    // init left tables dirty), mcode may already be loaded from the *unset*
    // system root (~/.mcode). Clearing the tables ensures mcode is reloaded
    // from the true system root below, otherwise a local `mcode/` or
    // $MCC_SYSTEM_ROOT would be shadowed by the stale ~/.mcode copy.
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    let project_root: PathBuf = resolve_project_root(args);
    // Defect 9: set project root before loading libraries so that
    // path resolution (e.g. mcc_relative_path) works during lib load.
    mcc::mcc_set_project_root(&project_root);
    manifest::load_libs(&manifest::collect_libs(
        Some(&project_root),
        &mcc::cli::globals().lib,
    ));

    // ── 0.5. Pass 0 snapshot: lib load + manifest + project load phase diagnostics ──
    builder.set_pass0(crate::cmds::parse::public_collect_pass0());

    // ── 1. manifest parsing ──
    tracing::debug!(target: "mcc::build", project_root = ?project_root, "resolved project root");

    let (entry_uri, top_name) = match manifest::build_from_manifest(
        &project_root,
        mcc::cli::globals().top.as_deref(),
        cli_entry(args).as_deref(),
    ) {
        Ok(r) => r,
        Err(_) => {
            // No project manifest → directory batch mode (use-design.md §19.5
            // rule 3): parse every `.mc` under the root recursively and build
            // each file's default top. Unified with `build.full`'s directory
            // branch and the extension's Build Project on a toml-less folder.
            return build_browse_dir(&project_root, args, builder, tracker);
        }
    };

    // Targets for the envelope and the viz (mcd docs-mc 16-export-viz §6):
    // explicit --top → all modules in the entry file → all components → all
    // interfaces. Components/interfaces are "virtually instantiated". Must be
    // resolved BEFORE any virtual build replaces the file with its synthetic
    // module (which would otherwise pollute the module list).
    let targets =
        match mcc::mcc_virtual_resolve_targets(&entry_uri, mcc::cli::globals().top.as_deref()) {
            Ok(t) => t,
            Err(e) => {
                let err = RpcError::invalid_params(e);
                emit_err(&mcc::cli::globals().format, err)?;
                return Ok(BuildOutcome { exit_code: 1 });
            }
        };

    // ── 2. Pass1 ──
    builder.set_pass1(crate::cmds::parse::public_collect_pass1(
        &entry_uri,
        &mut tracker,
    ));

    // ── 2.5. Batch-install synthetic wrappers for every virtual target ──
    // Done after the Pass1 report (so the envelope still shows the real
    // component count, not the VIRT_ wrappers) and before any per-target build.
    // Each per-target install re-reads + re-parses the whole file, so without
    // this a component library with N parts would be re-parsed N times.
    if let Err(e) = mcc::mcc_virtual_install_synthetic_views(&targets, &entry_uri) {
        let err = RpcError::invalid_params(format!("{:#}", e));
        emit_err(&mcc::cli::globals().format, err)?;
        return Ok(BuildOutcome { exit_code: 1 });
    }

    // ── 3. Pass2 ──
    // The envelope carries the first target's Pass 2 tree (matching
    // `--format json`); viz still renders all targets. The build also returns
    // the Phase D frozen string net-table store (the tree never carries
    // `NetPoint`); it feeds the flat net checks below.
    let (inst, arena, store, net_store) =
        match mcc::mcc_virtual_build_with_nets(&top_name, &entry_uri) {
            Ok((i, a, s, ns)) => {
                // Phase C S3-D: the tree tally resolves children through the
                // view (the tree's Vec fields are gone).
                let view = mcc::TreeView::new(&a, &s);
                builder.set_pass2(crate::cmds::parse::public_collect_pass2(
                    &top_name,
                    &i,
                    &view,
                    &ns,
                    &mut tracker,
                ));
                (i, a, s, ns)
            }
            Err(e) => {
                let err = RpcError::build_error(format!("{}", e));
                emit_err(&mcc::cli::globals().format, err)?;
                return Ok(BuildOutcome { exit_code: 1 });
            }
        };

    // ── G4: Write known_missing.md baseline ──
    // Phase C S3-D: the failed-record tree walk resolves sub-modules through
    // the view (the tree's Vec fields are gone).
    let view = mcc::TreeView::new(&arena, &store);
    mcc::InstTable::write_known_missing(&inst, "baseline/known_missing.md", &view);

    // ── 4. Viz generation ──
    if args.viz {
        if targets.len() > 1 {
            // Render all targets (peer modules, or several components /
            // interfaces in one file): build viz for each and combine them.
            let mut svgs: Vec<(Option<String>, String)> = Vec::new();
            let mut total_boxes = 0;
            let mut total_edges = 0;
            let mut netcheck_errors = 0usize;

            for target in &targets {
                let mod_uri = entry_uri.clone();

                match mcc::mcc_virtual_build_flat(target, &mod_uri, 1000) {
                    Ok((mod_inst, mod_table, mod_arena, mod_store)) => {
                        // ★ netcheck Tier 0: netlist health check (hard gate;
                        // a target failing it skips viz entirely)
                        let nc_report = mcc::instant::netcheck::run(&mod_table);
                        nc_report.print();
                        if !nc_report.is_clean() {
                            netcheck_errors += 1;
                            mcc_dbg!(
                                "build",
                                "[gate] NETCHECK Tier 0 not clean for '{}' -> skip viz.",
                                target
                            );
                            continue;
                        }

                        mcc::vector::builder::reset_np_warn_count();
                        let (vec_block, report) = mcc::build_mc_vec_with_report(
                            &mod_inst, &mod_table, &mod_arena, &mod_store,
                        );
                        // ★ P5.3: NetShape coverage to CLI — N/M nets have shape info
                        let ss = &report.shape_stats;
                        eprintln!(
                            "shape info: {}/{} nets have shape info ({:.0}%)",
                            ss.from_source,
                            ss.total_nets,
                            ss.coverage() * 100.0
                        );
                        // Virtual (component/interface) targets render in the
                        // device pipeline with the fabricated instance name
                        // hidden so the physical pins (id + name) show.
                        let is_virtual = !mcc::mcc_get_modules_in_file(&mod_uri)
                            .iter()
                            .any(|m| m == target);
                        let graph = if is_virtual {
                            mcc::mcc_virtual_prepare_graph(
                                mcc::build_mc_vec_graph(&vec_block, &mod_table),
                                target,
                            )
                        } else {
                            mcc::build_mc_vec_graph(&vec_block, &mod_table)
                        };

                        total_boxes += graph.boxes.len();
                        total_edges += graph.edges.len();

                        let opts = build_viz_opts(args.layouter.as_deref());
                        let doc = mcc::viz::api::render_with(graph, opts);

                        if let Some(root_layer) = doc.root_layer() {
                            // Virtual (component/interface) targets get no
                            // heading in the combined view — see combine_svgs.
                            let label = if is_virtual {
                                None
                            } else {
                                Some(target.clone())
                            };
                            svgs.push((label, root_layer.svg.clone()));
                        }
                    }
                    Err(e) => {
                        mcc_dbg!(
                            "build",
                            "[viz] skip target '{}': mcc_virtual_build_flat failed: {}",
                            target,
                            e
                        );
                    }
                }
            }

            if svgs.is_empty() {
                if netcheck_errors > 0 {
                    return Ok(BuildOutcome { exit_code: 1 });
                }
                return Err(anyhow::anyhow!("viz: no targets rendered"));
            }

            // Combine all SVGs into one big SVG, stacked vertically
            let combined_svg = mcc::viz::template::combine_svgs(&svgs);

            // Build a single-layer VizDocument with the combined SVG. The root
            // layer carries the entry file's base name so the title / breadcrumb
            // show the file, not the generic "all_targets".
            let root_name = mcc::viz::template::combined_view_name(&entry_uri);
            let mut doc = mcc::viz::doc::VizDocument::new(1000, root_name.clone());
            let mut layer = mcc::viz::layer::VizLayer::new(1000, root_name, None);
            layer.svg = combined_svg;
            doc.add_layer(layer);

            let html = mcc::viz::template::wrap_document(&doc);

            let output_path = mcc::cli::globals()
                .output
                .as_deref()
                .unwrap_or("circuit.html");
            std::fs::write(output_path, &html)
                .with_context(|| format!("failed to write file: {}", output_path))?;
            eprintln!("viz: {} bytes written to {}", html.len(), output_path);

            mcc_dbg!(
                "build",
                "[viz] rendered {} targets: {} boxes, {} edges",
                svgs.len(),
                total_boxes,
                total_edges
            );
        } else {
            // Single target render (explicit --top or only one target)
            let (_tinst, table, _tarena, _tstore) =
                mcc::mcc_virtual_build_flat(&top_name, &entry_uri, 1000)
                    .map_err(|e| anyhow::anyhow!("mcc_virtual_build_flat failed: {}", e))?;

            // Pipeline diagnostics gated behind MC_VIZ_DUMP (silent by default).
            if mcc::viz::log::enabled() {
                mcc_dbg!("build", "[DUMP] ====== InstTable contents ======");
                table.dump();
                mcc_dbg!("build", "[DUMP] ==============================");
            }

            // ★ netcheck Tier 0: netlist health check (hard gate; fails the build)
            let nc_report = mcc::instant::netcheck::run(&table);
            nc_report.print();

            // ★ M1-4: alignment metrics (self-test variant)
            let align_report = mcc::viz::metrics::align::AlignMetricsReport::compute(&table);
            align_report.print();

            if !nc_report.is_clean() {
                let gate_on = std::env::var("MCC_NETCHECK_GATE")
                    .map(|v| v.trim() != "0" && !v.eq_ignore_ascii_case("false"))
                    .unwrap_or(true);
                if gate_on {
                    mcc_dbg!("build", "[gate] NETCHECK Tier 0 not clean -> build failed.");
                    return Ok(BuildOutcome { exit_code: 1 });
                }
                mcc_dbg!(
                    "build",
                    "[gate] NETCHECK Tier 0 not clean (MCC_NETCHECK_GATE=0, continuing)"
                );
            }

            let (vec_block, build_report) =
                mcc::build_mc_vec_with_report(&inst, &table, &arena, &store);
            // Virtual (component/interface) targets render in the device
            // pipeline with the fabricated instance name hidden so the
            // physical pins (id + name) show.
            let is_virtual = !mcc::mcc_get_modules_in_file(&entry_uri)
                .iter()
                .any(|m| *m == top_name);
            let graph = if is_virtual {
                mcc::mcc_virtual_prepare_graph(
                    mcc::build_mc_vec_graph(&vec_block, &table),
                    &top_name,
                )
            } else {
                mcc::build_mc_vec_graph(&vec_block, &table)
            };

            let opts = build_viz_opts(args.layouter.as_deref());
            let (doc, metrics) = mcc::viz::api::render_with_metrics(graph, opts);
            let quality = metrics.finish_quality(Some(&build_report));
            // Metrics summary: always shown (this is the acceptance yardstick).
            for line in quality.report_lines() {
                mcc_dbg!("build", "{line}");
            }

            // [P0-DET] CLI golden guard: compare against baseline when MCC_GOLDEN_CHECK is set
            if std::env::var("MCC_GOLDEN_CHECK").is_ok() {
                let sig = doc.to_json();
                let gp = std::path::PathBuf::from("tests/golden/hbl.golden.json");
                if std::env::var("UPDATE_GOLDEN").is_ok() || !gp.exists() {
                    std::fs::create_dir_all(gp.parent().unwrap()).ok();
                    std::fs::write(&gp, &sig).ok();
                    mcc_dbg!("build", "[golden] wrote {}", gp.display());
                } else if sig != std::fs::read_to_string(&gp).unwrap_or_default() {
                    mcc_dbg!(
                        "build",
                        "[golden] MISMATCH vs {} (UPDATE_GOLDEN=1 to refresh)",
                        gp.display()
                    );
                    return Ok(BuildOutcome { exit_code: 1 });
                }
            }

            let html = mcc::viz::template::wrap_document(&doc);

            let output_path = mcc::cli::globals()
                .output
                .as_deref()
                .unwrap_or("circuit.html");
            std::fs::write(output_path, &html)
                .with_context(|| format!("failed to write file: {}", output_path))?;
            eprintln!("viz: {} bytes written to {}", html.len(), output_path);

            // [P0/A2] Electrical-fidelity hard gate: a non-perfect fidelity report means
            // the drawing is electrically wrong (dropped/partial nets, unrendered pins,
            // box/wire collisions). Fail the build so it can't pass silently.
            if !quality.is_perfect() {
                mcc_dbg!(
                    "build",
                    "[gate] FIDELITY not perfect -> build failed. See report above. \
                     (set MCC_FIDELITY_GATE=0 to downgrade to warning)"
                );
                let gate_on = std::env::var("MCC_FIDELITY_GATE")
                    .map(|v| v.trim() != "0" && !v.eq_ignore_ascii_case("false"))
                    .unwrap_or(true);
                if gate_on {
                    return Ok(BuildOutcome { exit_code: 1 });
                }
            }
        }
    }

    // ── 4.5. Electrical net checks (Pass2, incl. D7 PULLUP_DEGENERATE) ──
    // Surface the same findings as `mcc check --nets`. The pass-2 tree was
    // built above by `mcc_build`; flatten it once and run the net checks, so
    // D7 PULLUP_DEGENERATE and friends appear in `mcc build` output (the Pass 1
    // diagnostics block renders before Pass 2, so DB-only logging would never
    // be visible). Findings are printed, not gated: the Tier-0 netcheck and the
    // exit-code error count below own the failure semantics.
    // Phase C S3-D: the net-check re-flatten resolves children through the
    // view (the owning build's arena + store) — the tree's Vec fields are gone.
    let view = mcc::TreeView::new(&arena, &store);
    let mut flat_table =
        mcc::InstTable::from_module_inst(&inst, 1000, net_store.clone().into_shared(), &view);
    // from_module_inst rebuilds a fresh table that drops the synthetic markers
    // pass2 attached to the virtual wrapper's entries; re-apply them so the net
    // checks don't re-flag the unwired interface/component view's ports.
    mcc::mcc_virtual_mark_synthetic_flat_entries(&mut flat_table);
    let net_results = mcc::check::nets::run_net_checks(&flat_table);
    if !net_results.is_empty() {
        eprintln!(
            "=== Electrical Net Checks ({} issues) ===",
            net_results.len()
        );
        for r in &net_results {
            eprintln!("  [{}] {}: {}", r.severity, r.check, r.message);
        }
    }

    // ── 5. Exit code: based on error count ──
    let errors = builder.error_count();

    // ── 6. Emit envelope ──
    let env = Envelope::ok(builder.finish());
    let envelope_target = if args.viz && mcc::cli::globals().output.is_some() {
        None
    } else {
        mcc::cli::globals().output.as_deref().map(Path::new)
    };
    output::emit_envelope(&env, mcc::cli::globals().format, envelope_target, false)?;
    Ok(BuildOutcome {
        exit_code: if errors > 0 { 1 } else { 0 },
    })
}

/// A Pass2 build-failure diagnostic for the report (same shape the envelope
/// diagnostics carry).
fn build_failure_diag(uri: &str, code: u32, msg: String) -> Diagnostic {
    Diagnostic {
        phase: Phase::Pass2,
        severity: Severity::Error,
        code,
        message: msg,
        location: Some(DiagLocation {
            file: uri.to_string(),
            line: 0,
            column: 0,
            end_line: None,
            end_column: None,
            pos: 0,
            len: 0,
        }),
        suggestions: vec![],
        related: vec![],
    }
}

/// `mcc build <dir>` fallback when the directory has no project manifest
/// (use-design.md §19.5 rule 3 — directory batch mode).
///
/// Parses every `.mc` file under `root` recursively (hidden dirs skipped) and
/// builds each file's default top (module directly, component / interface
/// virtually instantiated). Pass1 covers the whole folder; Pass2 aggregates
/// per-file diagnostics; the envelope carries the first successfully-built
/// tree. A file whose build fails is recorded as a Pass2 error and skipped —
/// one bad file never aborts the folder report. An explicit `--top` builds
/// that target from the first file that declares it. `--viz` renders each
/// built file's top and combines the views.
fn build_browse_dir(
    root: &Path,
    args: &BuildArgs,
    mut builder: ResultBuilder,
    mut tracker: PhaseTracker,
) -> Result<BuildOutcome> {
    let files = mcc::mcc_load_directory_all(root);
    if files.is_empty() {
        let err = RpcError::invalid_params(format!(
            "build: no `.mc` files found under {}",
            root.display()
        ));
        emit_err(&mcc::cli::globals().format, err)?;
        return Ok(BuildOutcome { exit_code: 1 });
    }

    // ── 2. Pass1: workspace-wide — every file + its use closure (all files). ──
    let first_uri = files[0].to_string_lossy().into_owned();
    builder.set_pass1(crate::cmds::parse::public_collect_pass1(
        &first_uri,
        &mut tracker,
    ));

    // ── 3. Pass2: per-file default top build, aggregated ──
    let explicit_top = mcc::cli::globals().top.as_deref();
    let mut top_name = String::new();
    // Phase C S3-D: the arena + store ride alongside the tree so the
    // net-check re-flatten can build a store-backed view (the tree's Vec
    // fields are gone).
    let mut first_inst: Option<(
        mcc::MccProjectTree,
        mcc::NodeArena,
        mcc::InstanceStore,
        mcc::NetTableStore,
    )> = None;
    let mut failures: Vec<Diagnostic> = Vec::new();
    let mut built: Vec<(String, PathBuf)> = Vec::new(); // (target, file) for viz

    // Build `target` from `file`; record failures (non-fatal) so one bad file
    // doesn't abort the folder report.
    let build_one = |target: &str,
                     file: &Path,
                     failures: &mut Vec<Diagnostic>|
     -> Option<(
        mcc::MccProjectTree,
        mcc::NodeArena,
        mcc::InstanceStore,
        mcc::NetTableStore,
    )> {
        let uri = file.to_string_lossy().to_string();
        let mc_uri = mcc::McURI::from(uri.as_str());
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            mcc::mcc_virtual_build_with_nets(target, &mc_uri)
        })) {
            Ok(Ok(pair)) => Some(pair),
            Ok(Err(e)) => {
                failures.push(build_failure_diag(
                    &uri,
                    32107,
                    format!("build failed: {e}"),
                ));
                None
            }
            Err(_) => {
                failures.push(build_failure_diag(
                    &uri,
                    32108,
                    "Pass2 build panicked (engine bug); skipped".into(),
                ));
                None
            }
        }
    };

    if let Some(t) = explicit_top {
        for f in &files {
            let uri = f.to_string_lossy().to_string();
            let mc_uri = mcc::McURI::from(uri.as_str());
            let declares = mcc::mcc_get_modules_in_file(&mc_uri)
                .iter()
                .chain(mcc::mcc_get_components_in_file(&mc_uri).iter())
                .chain(mcc::mcc_get_interfaces_in_file(&mc_uri).iter())
                .any(|n| n == t);
            if !declares {
                continue;
            }
            if let Some(pair) = build_one(t, f, &mut failures) {
                top_name = t.to_string();
                first_inst = Some(pair);
                built.push((t.to_string(), f.clone()));
            }
            break;
        }
    } else {
        for f in &files {
            let uri = f.to_string_lossy().to_string();
            let mc_uri = mcc::McURI::from(uri.as_str());
            let targets = match mcc::mcc_virtual_resolve_targets(&mc_uri, None) {
                Ok(t) => t,
                Err(_) => continue, // pass1 already reports the file's problems
            };
            let Some(tgt) = targets.into_iter().next() else {
                continue;
            };
            if let Some(pair) = build_one(&tgt, f, &mut failures) {
                built.push((tgt.clone(), f.clone()));
                if first_inst.is_none() {
                    top_name = tgt;
                    first_inst = Some(pair);
                }
            }
        }
    }

    // ── Pass2 report: aggregated diagnostics, first tree in the envelope ──
    match &first_inst {
        Some((inst, arena, store, net_store)) => {
            // Phase C S3-D: the tree tally resolves children through the view
            // (the tree's Vec fields are gone).
            let view = mcc::TreeView::new(arena, store);
            let mut report = crate::cmds::parse::public_collect_pass2(
                &top_name,
                inst,
                &view,
                net_store,
                &mut tracker,
            );
            report.diagnostics.extend(failures);
            builder.set_pass2(report);
        }
        None => {
            let mut report = Pass2Report {
                top: top_name.clone(),
                instances: None,
                nets: vec![],
                connections: vec![],
                diagnostics: tracker.collect(Phase::Pass2),
            };
            report.diagnostics.extend(failures);
            builder.set_pass2(report);
        }
    }

    // ── G4: baseline from the first successful tree (if any) ──
    if let Some((inst, arena, store, _)) = &first_inst {
        // Phase C S3-D: the failed-record tree walk resolves sub-modules
        // through the view (the tree's Vec fields are gone).
        let view = mcc::TreeView::new(arena, store);
        mcc::InstTable::write_known_missing(inst, "baseline/known_missing.md", &view);
    }

    // ── 4. Viz: each built file's top, combined ──
    if args.viz {
        let mut svgs: Vec<(Option<String>, String)> = Vec::new();
        let mut total_boxes = 0usize;
        let mut total_edges = 0usize;
        let mut netcheck_errors = 0usize;
        for (target, file) in &built {
            let uri = file.to_string_lossy().to_string();
            let mc_uri = mcc::McURI::from(uri.as_str());
            match mcc::mcc_virtual_build_flat(target, &mc_uri, 1000) {
                Ok((mod_inst, mod_table, mod_arena, mod_store)) => {
                    let nc_report = mcc::instant::netcheck::run(&mod_table);
                    nc_report.print();
                    if !nc_report.is_clean() {
                        netcheck_errors += 1;
                        mcc_dbg!(
                            "build",
                            "[gate] NETCHECK Tier 0 not clean for '{target}' -> skip viz."
                        );
                        continue;
                    }
                    mcc::vector::builder::reset_np_warn_count();
                    let (vec_block, report) = mcc::build_mc_vec_with_report(
                        &mod_inst, &mod_table, &mod_arena, &mod_store,
                    );
                    let ss = &report.shape_stats;
                    eprintln!(
                        "shape info: {}/{} nets have shape info ({:.0}%)",
                        ss.from_source,
                        ss.total_nets,
                        ss.coverage() * 100.0
                    );
                    let is_virtual = !mcc::mcc_get_modules_in_file(&mc_uri)
                        .iter()
                        .any(|m| m == target);
                    let graph = if is_virtual {
                        mcc::mcc_virtual_prepare_graph(
                            mcc::build_mc_vec_graph(&vec_block, &mod_table),
                            target,
                        )
                    } else {
                        mcc::build_mc_vec_graph(&vec_block, &mod_table)
                    };
                    total_boxes += graph.boxes.len();
                    total_edges += graph.edges.len();
                    let opts = build_viz_opts(args.layouter.as_deref());
                    let doc = mcc::viz::api::render_with(graph, opts);
                    if let Some(root_layer) = doc.root_layer() {
                        let label = if is_virtual {
                            None
                        } else {
                            Some(target.clone())
                        };
                        svgs.push((label, root_layer.svg.clone()));
                    }
                }
                Err(e) => {
                    mcc_dbg!(
                        "build",
                        "[viz] skip target '{target}': mcc_virtual_build_flat failed: {e}"
                    );
                }
            }
        }
        if svgs.is_empty() {
            if netcheck_errors > 0 {
                return Ok(BuildOutcome { exit_code: 1 });
            }
            return Err(anyhow::anyhow!("viz: no targets rendered"));
        }
        let combined_svg = mcc::viz::template::combine_svgs(&svgs);
        let root_name = mcc::viz::template::combined_view_name(&root.to_string_lossy());
        let mut doc = mcc::viz::doc::VizDocument::new(1000, root_name.clone());
        let mut layer = mcc::viz::layer::VizLayer::new(1000, root_name, None);
        layer.svg = combined_svg;
        doc.add_layer(layer);
        let html = mcc::viz::template::wrap_document(&doc);
        let output_path = mcc::cli::globals()
            .output
            .as_deref()
            .unwrap_or("circuit.html");
        std::fs::write(output_path, &html)
            .with_context(|| format!("failed to write file: {}", output_path))?;
        eprintln!("viz: {} bytes written to {}", html.len(), output_path);
        mcc_dbg!(
            "build",
            "[viz] rendered {} targets: {} boxes, {} edges",
            svgs.len(),
            total_boxes,
            total_edges
        );
    }

    // ── 4.5. Electrical net checks (Pass2) on the first successful tree ──
    if let Some((inst, arena, store, net_store)) = &first_inst {
        // Phase C S3-D: re-flatten resolves children through the view (the
        // owning build's arena + store) — the tree's Vec fields are gone.
        let view = mcc::TreeView::new(arena, store);
        let mut flat_table =
            mcc::InstTable::from_module_inst(inst, 1000, net_store.clone().into_shared(), &view);
        mcc::mcc_virtual_mark_synthetic_flat_entries(&mut flat_table);
        let net_results = mcc::check::nets::run_net_checks(&flat_table);
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

    // ── 5/6. Exit code + envelope ──
    let errors = builder.error_count();
    let env = Envelope::ok(builder.finish());
    let envelope_target = mcc::cli::globals().output.as_deref().map(Path::new);
    output::emit_envelope(&env, mcc::cli::globals().format, envelope_target, false)?;
    Ok(BuildOutcome {
        exit_code: if errors > 0 { 1 } else { 0 },
    })
}

fn emit_err(fmt: &OutputFormat, err: RpcError) -> Result<()> {
    if fmt.is_structured() {
        output::emit_envelope(&Envelope::err(err), *fmt, None, false)?;
        Ok(())
    } else {
        Err(anyhow::anyhow!(err.message))
    }
}

fn build_viz_opts(layouter_name: Option<&str>) -> mcc::viz::api::RenderOpts {
    let mut opts = mcc::viz::api::RenderOpts::default();
    if let Some(name) = layouter_name {
        match name {
            "flow" => {
                opts.top_layouter = Box::new(FlowLayouter::default());
                opts.sub_layouter = Box::new(FlowLayouter::sub());
                opts.top_candidates = vec![Box::new(FlowLayouter::default())];
                opts.sub_candidates = vec![Box::new(FlowLayouter::sub())];
                mcc_dbg!("build", "[viz] locked layouter: top=flow sub=flow");
            }
            other => {
                mcc_dbg!(
                    "build",
                    "[viz] unknown layouter '{}', using default (flow). Only 'flow' is supported.",
                    other
                );
            }
        }
    }
    opts
}

// ─────────────────────────────────────────────────────────────────────────────
// Multi-target SVG combination now lives in `viz::template::combine_svgs`
// (shared with the RPC `build.viz` path, mcd docs-mc 16-export-viz §6).
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod phase0_golden {
    use super::build_viz_opts;
    use crate::cmds::manifest;
    use mcc::mcc_dbg;
    use mcc::McIds;
    use std::path::{Path, PathBuf};

    /// hbl fixture driven by env vars; skipped if unset (CI without fixture passes).
    ///   MCC_GOLDEN_PROJECT=<hbl project root>  [MCC_GOLDEN_ENTRY=<entry>] [MCC_GOLDEN_TOP=<top name>]
    fn hbl_project() -> Option<(PathBuf, Option<String>, Option<String>)> {
        let root = std::env::var("MCC_GOLDEN_PROJECT").ok()?;
        Some((
            PathBuf::from(root),
            std::env::var("MCC_GOLDEN_ENTRY").ok(),
            std::env::var("MCC_GOLDEN_TOP").ok(),
        ))
    }

    /// Replicate the real `--viz` sequence from build.rs, stopping before render.
    fn build_graph(
        root: &Path,
        entry: Option<&str>,
        top: Option<&str>,
    ) -> mcc::vector::graph::McVecGraph {
        mcc::mcc_init_no_lib();
        let (entry_uri, top_name) =
            manifest::build_from_manifest(root, top, entry).expect("build_from_manifest");
        let ident = McIds::from(top_name.as_str());
        // One instantiation → one DianLu (§12.2); the flat projection and net
        // checks run once inside `flatten`, never a second instantiation.
        let mut dl = mcc::mcc_build_dianlu(&ident, &entry_uri, 1000).expect("mcc_build_dianlu");
        let _ = dl.flatten(); // viz path does not consume net-check diagnostics
        let arena = dl.arena().clone();
        let store = dl.store().clone();
        let (inst, table) = dl.into_parts();
        let vec_block = mcc::build_mc_vec_with_arena(&inst, &table, &arena, &store);
        mcc::build_mc_vec_graph(&vec_block, &table)
    }

    /// Fingerprint = VizDocument::to_json() (structure + per-layer SVG).
    fn render_signature(graph: mcc::vector::graph::McVecGraph) -> String {
        let opts = build_viz_opts(None); // default = FlowLayouter
        mcc::viz::api::render_with(graph, opts).to_json()
    }

    /// Core guard: same input rendered twice must produce byte-identical fingerprints.
    /// Isolates layout+route determinism; any HashMap-order leak (including in flow.rs)
    /// will surface here.
    #[test]
    fn cli_build__determinism_render_twice() {
        let Some((root, entry, top)) = hbl_project() else {
            mcc_dbg!(
                "build",
                "[phase0] set MCC_GOLDEN_PROJECT to enable; skipping"
            );
            return;
        };
        let graph = build_graph(&root, entry.as_deref(), top.as_deref());
        let a = render_signature(graph.clone());
        let b = render_signature(graph);
        assert_eq!(
            a, b,
            "render_with nondeterministic on identical input graph"
        );
    }

    /// Secondary guard: two independent build+render cycles should also match
    /// (covers build-phase determinism).
    #[test]
    fn cli_build__determinism_two_builds() {
        let Some((root, entry, top)) = hbl_project() else {
            return;
        };
        let a = render_signature(build_graph(&root, entry.as_deref(), top.as_deref()));
        let b = render_signature(build_graph(&root, entry.as_deref(), top.as_deref()));
        assert_eq!(
            a, b,
            "two independent builds differ (global-state or HashMap leak)"
        );
    }

    /// Golden regression: first run (or UPDATE_GOLDEN=1) writes baseline;
    /// subsequent runs compare byte-for-byte.
    #[test]
    fn cli_build__golden_roundtrip_hbl() {
        let Some((root, entry, top)) = hbl_project() else {
            return;
        };
        let sig = render_signature(build_graph(&root, entry.as_deref(), top.as_deref()));
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/hbl.golden.json");
        if std::env::var("UPDATE_GOLDEN").is_ok() || !path.exists() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &sig).unwrap();
            mcc_dbg!("build", "[golden] wrote baseline -> {}", path.display());
            return;
        }
        let golden = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            sig, golden,
            "hbl render changed vs golden. If intended: UPDATE_GOLDEN=1 cargo test cli_build__golden_roundtrip_hbl"
        );
    }

    /// Smoke test: metrics accumulation on hbl produces sensible counts.
    #[test]
    fn cli_build__metrics_hbl_smoke() {
        let Some((root, entry, top)) = hbl_project() else {
            return;
        };
        let graph = build_graph(&root, entry.as_deref(), top.as_deref());
        let (_, metrics) =
            mcc::viz::api::render_with_metrics(graph, mcc::viz::api::RenderOpts::default());
        let (fid, read) = metrics.finish(None); // self-consistent even without build report
        mcc_dbg!("build", "{}", fid.report_line());
        mcc_dbg!("build", "{}", read.report_line());
        assert!(fid.pins_rendered <= fid.pins_total);
        assert!(fid.nets_rendered <= fid.nets_total);
        assert!(read.total_wirelength >= 0.0 && read.weighted() >= 0.0);
    }
}

// ============================================================================
// D1–D8 detector tests
// ============================================================================
// Each test creates a small .mc fixture that triggers a specific detector,
// builds it, and asserts that the expected diagnostic code was emitted.
// Tests use a global lock because mcc global state (workspace) is not thread-safe.
#[cfg(test)]
mod d_detectors {
    use mcc::McDiagnostic;
    use mcc::McIds;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Helper: build a fixture string and return diagnostics produced.
    /// Returns (diagnostics, build_error) — build_error is Some(msg) if mcc_build failed.
    fn build_fixture(content: &str) -> (Vec<McDiagnostic>, Option<String>) {
        mcc::mcc_init_no_lib();
        mcc::mcc_set_system_root(std::path::Path::new(""));
        let uri = "/mcc/snippet.mc".to_string();
        mcc::mcc_clear_workspace();
        mcc::vector::builder::resolve::reset_np_warn_count();
        mcc::mcc_load_from_string(&uri, content);
        let ident = McIds::from("top");
        let build_result = mcc::mcc_build_dianlu(&ident, &uri, 1000);
        let build_err = build_result.as_ref().err().map(|e| e.to_string());
        let mut diags = mcc::mcc_diagnose_all();
        if let Ok(mut dl) = build_result {
            // Phase A: flatten returns the net-check diagnostics — the caller
            // owns them. Append instead of logging into the workspace.
            diags.extend(dl.flatten());
        }
        (diags, build_err)
    }

    /// Like build_fixture but panics on build failure.
    fn build_fixture_or_panic(content: &str) -> Vec<McDiagnostic> {
        let (diags, err) = build_fixture(content);
        if let Some(e) = err {
            panic!(
                "mcc_build failed: {e}. Diags: {:?}",
                diags.iter().map(|d| (d.code, &d.msg)).collect::<Vec<_>>()
            );
        }
        diags
    }
    /// Helper: build fixture + vector graph, return diagnostics.
    fn build_fixture_with_graph(content: &str) -> Vec<McDiagnostic> {
        mcc::mcc_init_no_lib();
        mcc::mcc_set_system_root(std::path::Path::new(""));
        let uri = "/mcc/snippet.mc".to_string();
        mcc::mcc_clear_workspace();
        mcc::vector::builder::resolve::reset_np_warn_count();
        mcc::mcc_load_from_string(&uri, content);
        let ident = McIds::from("top");
        let mut dl = mcc::mcc_build_dianlu(&ident, &uri, 1000).expect("mcc_build_dianlu");
        let flat_diags = dl.flatten(); // Phase A: caller owns net-check diagnostics
        let arena = dl.arena().clone();
        let store = dl.store().clone();
        let (inst, table) = dl.into_parts();
        let vec_block = mcc::build_mc_vec_with_arena(&inst, &table, &arena, &store);
        let _graph = mcc::build_mc_vec_graph(&vec_block, &table);
        let mut diags = mcc::mcc_diagnose_all();
        diags.extend(flat_diags);
        diags
    }

    fn has_code(diags: &[McDiagnostic], code: u32) -> bool {
        diags.iter().any(|d| d.code == code)
    }

    // ── D1 SORT_HAZARD ─────────────────────────────────────────────────

    #[test]
    fn cli_build__d1_sort_hazard_non_monotonic_pins() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // D1 fires when bus pin numbers are non-monotonic.
        // [5,2] = BUS{CLK, DATA} → pin order differs from member order.
        let fixture = r#"
component MyChip {
    pins = [
        io [5,2] = BUS{CLK, DATA}
    ]
}
module top {
    io CLK, DATA
    MyChip chip
    chip{CLK, DATA} -> (CLK, DATA)
}
"#;
        let (diags, build_err) = build_fixture(fixture);
        assert!(
            build_err.is_none(),
            "D1 build should succeed. Build err: {:?}",
            build_err
        );
        assert!(
            has_code(&diags, mcc::errcodes::SORT_HAZARD),
            "D1 SORT_HAZARD should fire for non-monotonic pins [5,2]. Diags: {:?}",
            diags.iter().map(|d| (d.code, &d.msg)).collect::<Vec<_>>()
        );
    }

    // ── D2 FLOATING_PLACEHOLDER ─────────────────────────────────────────

    #[test]
    fn cli_build__d2_floating_placeholder_unbound_lead() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let fixture = r#"
module top {
    _ -> _
}
"#;
        let diags = build_fixture_with_graph(fixture);
        assert!(
            has_code(&diags, mcc::errcodes::FLOATING_PLACEHOLDER),
            "D2 FLOATING_PLACEHOLDER should fire for unbound '_'. Diags: {:?}",
            diags.iter().map(|d| (d.code, &d.msg)).collect::<Vec<_>>()
        );
    }

    // ── D3 MERGED_SHORT ─────────────────────────────────────────────────

    #[test]
    fn cli_build__d3_merged_short_same_physical_pin() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // D3 fires when two points in the same net resolve to the same id.
        // The bracket expansion [A, A] creates two points both resolving to
        // the same port A, which is a merged short. The right side must be
        // 2-wide too: `[A, A] -> GND` (2x1 vs 1x1) is intentionally rejected
        // by the strict opcheck as a single-point broadcast (no carve-out).
        let fixture = r#"
module top {
    io A
    [A, A] -> [GND, GND]
}
"#;
        let diags = build_fixture_with_graph(fixture);
        assert!(
            has_code(&diags, mcc::errcodes::NET_MERGED_SHORT),
            "D3 MERGED_SHORT should fire for duplicate bracket entries. Diags: {:?}",
            diags.iter().map(|d| (d.code, &d.msg)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn cli_build__d3_no_fire_for_legit_fanout() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // Fan-out `[P1, P2] -> [G, G]` produces the distinct pairs (P1, G) and
        // (P2, G) — multiple pins merging onto one net is legitimate and must
        // NOT be flagged as a merged short (regression: hbl periph.mc:51
        // `-> [dc.GND, dc.GND]` used to fire E2003).
        let fixture = r#"
component RES {
    pins = [
        1 = P
        2 = G
    ]
}
module top {
    io A, B
    RES r1, r2
    [r1.G, r2.G] -> [GND, GND]
    A -> r1.P
    B -> r2.P
}
"#;
        let (diags, build_err) = build_fixture(fixture);
        assert!(
            build_err.is_none(),
            "fan-out build should succeed. Build err: {:?}",
            build_err
        );
        assert!(
            !has_code(&diags, mcc::errcodes::NET_MERGED_SHORT),
            "D3 MERGED_SHORT should NOT fire for legit fan-out. Diags: {:?}",
            diags.iter().map(|d| (d.code, &d.msg)).collect::<Vec<_>>()
        );
    }

    // ── §5 same-name group: NET_DUPLICATE_REF / NET_SHORT_REF ──────────
    // same-name-pin-group.md §5: a same-name multi-pin group (`3 = GND; 4 = GND`)
    // is one logical net; referencing it in two slots (`spk{GND, GND}`) is
    // either redundant (all slots pair to the same peer net) or shorts the
    // peer nets together — both are non-blocking warnings.

    #[test]
    fn cli_build__d5_same_name_group_redundant_ref_warns() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // `spk{GND, GND}` references the same logical net twice and both slots
        // pair to the same peer net (cap.GND) — redundant wiring, warning only.
        // Both sides are 2-pad same-name groups so Pass2 physical rows match
        // (4 vs 4) and the statement reaches create_connection.
        let fixture = r#"
component SPK {
    pins = [
        3 = GND
        4 = GND
    ]
}
component CAP {
    pins = [
        1 = GND
        2 = GND
    ]
}
module top {
    SPK spk
    CAP cap
    spk{GND, GND} -> cap{GND, GND}
}
"#;
        let diags = build_fixture_or_panic(fixture);
        assert!(
            has_code(&diags, mcc::errcodes::NET_DUPLICATE_REF),
            "NET_DUPLICATE_REF should fire for a redundant same-net reference. Diags: {:?}",
            diags.iter().map(|d| (d.code, &d.msg)).collect::<Vec<_>>()
        );
        assert!(
            !has_code(&diags, mcc::errcodes::NET_MERGED_SHORT),
            "D3 MERGED_SHORT must NOT fire for the redundant same-name group case — it stays warning-level. Diags: {:?}",
            diags.iter().map(|d| (d.code, &d.msg)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn cli_build__d5_same_name_group_short_ref_warns() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // `spk{GND, GND}` references the same logical net twice but the two
        // slots pair to different peer nets (cap.A1 / cap.A2) — they are
        // shorted together through the group's pads.
        let fixture = r#"
component SPK {
    pins = [
        3 = GND
        4 = GND
    ]
}
component CAP {
    pins = [
        1 = A1
        2 = A1
        3 = A2
        4 = A2
    ]
}
module top {
    SPK spk
    CAP cap
    spk{GND, GND} -> cap{A1, A2}
}
"#;
        let diags = build_fixture_or_panic(fixture);
        assert!(
            has_code(&diags, mcc::errcodes::NET_SHORT_REF),
            "NET_SHORT_REF should fire when a same-name group pairs to different nets. Diags: {:?}",
            diags.iter().map(|d| (d.code, &d.msg)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn cli_build__d5_same_name_group_single_ref_no_warn() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // `spk{GND}` references the 2-pad group once — legal fan-in, no warning.
        let fixture = r#"
component SPK {
    pins = [
        3 = GND
        4 = GND
    ]
}
component CAP {
    pins = [
        1 = GND
        2 = GND
    ]
}
module top {
    SPK spk
    CAP cap
    spk{GND} -> cap{GND}
}
"#;
        let diags = build_fixture_or_panic(fixture);
        assert!(
            !has_code(&diags, mcc::errcodes::NET_DUPLICATE_REF)
                && !has_code(&diags, mcc::errcodes::NET_SHORT_REF),
            "single same-name group reference should not warn. Diags: {:?}",
            diags.iter().map(|d| (d.code, &d.msg)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn cli_build__d5_same_name_group_single_side_fan_in_connects_pads() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // `spk{GND} -> GND`: one same-name group (`3 = GND; 4 = GND`)
        // referenced once against a 1×1 scalar. Point resolution must see a
        // single logical slot (1×1) so §5.2 passes, and the connection must fan
        // BOTH physical pads onto the peer net (same-name-pin-group.md §6.3).
        // Pre-fix behavior: Pass2 resolved 2 physical points vs 1 scalar,
        // dropped the statement and left spk.3/spk.4 dangling.
        let fixture = r#"
component SPK {
    pins = [
        1 = P
        2 = N
        3 = GND
        4 = GND
    ]
}
module top {
    SPK spk
    spk{GND} -> GND
}
"#;
        mcc::mcc_init_no_lib();
        mcc::mcc_set_system_root(std::path::Path::new(""));
        let uri = "/mcc/snippet.mc".to_string();
        mcc::mcc_clear_workspace();
        mcc::vector::builder::resolve::reset_np_warn_count();
        mcc::mcc_load_from_string(&uri, fixture);
        let ident = McIds::from("top");
        let (tree, _table) = mcc::mcc_build_flat(&ident, &uri, 1000).expect("mcc_build_flat");
        let diags = mcc::mcc_diagnose_all();
        assert!(
            !has_code(&diags, mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH),
            "E4007 must not fire for a single same-name group slot vs a scalar. Diags: {:?}",
            diags.iter().map(|d| (d.code, &d.msg)).collect::<Vec<_>>()
        );
        let pads_connected = tree.connections.iter().any(|conn| {
            let paths: Vec<&str> = conn.points.iter().map(|p| p.path.as_str()).collect();
            paths.contains(&"spk.3") && paths.contains(&"spk.4")
        });
        assert!(
            pads_connected,
            "fan-in must connect both pads spk.3 and spk.4 onto the peer net. Connections: {:?}",
            tree.connections
                .iter()
                .map(|c| c.points.iter().map(|p| p.path.clone()).collect::<Vec<_>>())
                .collect::<Vec<_>>()
        );
    }

    // ── D5 BUS_ORDER_MISMATCH ───────────────────────────────────────────

    #[test]
    fn cli_build__d5_bus_order_mismatch_all_pairs() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // Two distinct components: PORT_A{A, B} connects to PORT_B{X, Y} with
        // no overlapping member names, so all pairs mismatch positionally.
        let fixture = r#"
component MyChip {
    pins = [
        io [1,2] = PORT_A{A, B}
        io [1,2] = PORT_B{X, Y}
    ]
}
module top {
    MyChip chipA
    MyChip chipB
    chipA{PORT_A} -> chipB{PORT_B}
}
"#;
        let (diags, build_err) = build_fixture(fixture);
        let mismatched = mcc::mcc_bus_bits_mismatched();
        assert!(
            build_err.is_none(),
            "D5 build should succeed. Build err: {:?}",
            build_err
        );
        assert!(
            has_code(&diags, mcc::errcodes::NET_BUS_ORDER_MISMATCH) || mismatched > 0,
            "D5 BUS_ORDER_MISMATCH should fire for A↔X, B↔Y. mismatched={} diags: {:?}",
            mismatched,
            diags.iter().map(|d| (d.code, &d.msg)).collect::<Vec<_>>()
        );
    }

    // ── Arity-0 gate (stmt.rs instance-method dispatch) ────────────────
    // A no-arg instance method called with arguments must NOT be dispatched:
    // dispatching it would silently drop the caller's args and wrongly expand
    // the no-arg body (e.g. `A -> GND_PIN` would short the pin to ground).
    // NOTE: use a *component* method here — module-level arity-0 funcs are
    // auto-invoked during instantiate (P2-8), which would mask the gate.

    #[test]
    fn cli_build__arity_gate_noarg_method_with_args_not_dispatched() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let fixture = r#"
component CMP {
    pins = [
        1 = A
        2 = GND_PIN
    ]
    func noarg() {
        A -> GND_PIN
    }
}
module top {
    CMP c1
    VCC -> c1.A
    c1.noarg(VCC)
}
"#;
        mcc::mcc_init_no_lib();
        mcc::mcc_set_system_root(std::path::Path::new(""));
        let uri = "/mcc/snippet.mc".to_string();
        mcc::mcc_clear_workspace();
        mcc::mcc_load_from_string(&uri, fixture);
        let ident = McIds::from("top");
        let (_inst, _, _, net_store) = mcc::mcc_build_with_nets(&ident, &uri).expect("mcc_build");
        let root_nets = net_store.get("top").map(|t| t.to_vec()).unwrap_or_default();

        let diags = mcc::mcc_diagnose_all();

        // Debug dump of every net so a regression shows exactly what got wired.
        let net_dump: Vec<String> = root_nets
            .iter()
            .map(|(name, pts)| {
                format!(
                    "{name}=[{}]",
                    pts.iter()
                        .map(|p| format!("{}.{}", p.path, p.member_name.as_deref().unwrap_or("")))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
            .collect();
        eprintln!("TOP nets: {net_dump:?}");

        // The arity-0 gate must intercept: the no-arg body `A -> GND_PIN` must
        // NOT be expanded, so no single net may contain both c1.A and GND_PIN.
        let shorted = root_nets.iter().any(|(_, pts)| {
            pts.iter().any(|p| p.path == "c1.A") && pts.iter().any(|p| p.path.ends_with("GND_PIN"))
        });
        assert!(
            !shorted,
            "arity-0 method called with args was dispatched — body `A -> GND_PIN` expanded, pin shorted: {net_dump:?}. diags: {:?}",
            diags.iter().map(|d| (d.code, &d.msg)).collect::<Vec<_>>()
        );
    }

    // ── D6 DROPPED_STATEMENT ────────────────────────────────────────────

    #[test]
    fn cli_build__d6_dropped_statement_indexed_alias() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // D6 fires when a single-element square bracket expands to an
        // unknown name that is not a known instance.
        let fixture = r#"
module top {
    io A
    [Unknown] -> A
}
"#;
        let (diags, build_err) = build_fixture(fixture);
        assert!(
            has_code(&diags, mcc::errcodes::NET_DROPPED_STATEMENT),
            "D6 DROPPED_STATEMENT should fire for indexed alias. Build err: {:?}. Diags: {:?}",
            build_err,
            diags.iter().map(|d| (d.code, &d.msg)).collect::<Vec<_>>()
        );
    }

    // ── D7 PULLUP_DEGENERATE ────────────────────────────────────────────

    #[test]
    fn cli_build__d7_pullup_degenerate_signal_bridge() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let fixture = r#"
component RES(rs::UV.OHM) {
    pins = [
        io [1,2] = NODE{P, N}
    ]
    func Pullup(net, vcc) {
        net - this{1}
        this{2} - vcc
        return net
    }
}
module top {
    io SCL, SDA
    RES(10k).Pullup(SCL, SDA)
}
"#;
        let (diags, build_err) = build_fixture(fixture);
        assert!(
            has_code(&diags, mcc::errcodes::PULLUP_DEGENERATE),
            "D7 PULLUP_DEGENERATE should fire for signal-signal bridge. Build err: {:?}. Diags: {:?}",
            build_err,
            diags.iter().map(|d| (d.code, &d.msg)).collect::<Vec<_>>()
        );
    }

    // ── D8 ARRAY-INSTANCE BRACKET REFERENCE (problem A) ──────────────────
    // A plain Series statement referencing a declared array of instances by
    // its bracket form (`cap[4:5] -> PWR{VCC, GND}`) must re-link to the
    // already-declared instances cap4/cap5 instead of being quarantined as
    // `@_phantom_N` (0 connections, silent drop).

    #[test]
    fn cli_build__d8_array_instance_bracket_reference_relinks() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let fixture = r#"
component CAP {
    pins = [
        io [1,2] = NODE{P, N}
    ]
}
module top {
    io PWR{VCC, GND};
    cap[4:5]::CAP();
    cap[4:5] -> PWR{VCC, GND};
}
"#;
        let (diags, build_err) = build_fixture(fixture);
        assert!(
            build_err.is_none(),
            "D8 build should succeed. Build err: {:?}",
            build_err
        );
        assert!(
            !has_code(&diags, mcc::errcodes::NET_DROPPED_STATEMENT),
            "D8 DROPPED_STATEMENT should NOT fire for an existing array-instance bracket ref. Diags: {:?}",
            diags.iter().map(|d| (d.code, &d.msg)).collect::<Vec<_>>()
        );

        // The bracket-form reference must resolve the array members to real
        // row-aligned connections instead of a phantom drop: the 2-row array
        // node column (cap4.2, cap5.2) zips against the 2-member rail column
        // (PWR.VCC, PWR.GND) per vec-dianlu §5.2 — cap4.2 on PWR.VCC and
        // cap5.2 on PWR.GND. (The old per-member re-link landed BOTH members
        // on PWR.VCC — the abolished 1:N single-point broadcast; §5.3.1.)
        mcc::mcc_init_no_lib();
        mcc::mcc_set_system_root(std::path::Path::new(""));
        let uri = "/mcc/snippet.mc".to_string();
        mcc::mcc_clear_workspace();
        mcc::mcc_load_from_string(&uri, fixture);
        let ident = McIds::from("top");
        let (_inst, _, _, net_store) = mcc::mcc_build_with_nets(&ident, &uri).expect("mcc_build");
        let root_nets = net_store.get("top").map(|t| t.to_vec()).unwrap_or_default();
        let net_paths = |name: &str| -> Vec<&str> {
            root_nets
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, pts)| pts.iter().map(|p| p.path.as_str()).collect())
                .unwrap_or_default()
        };
        let vcc = net_paths("PWR.VCC");
        let gnd = net_paths("PWR.GND");
        assert!(
            vcc.contains(&"cap4.2") && gnd.contains(&"cap5.2"),
            "D8 row zip must place cap4.2 on PWR.VCC and cap5.2 on PWR.GND. \
             PWR.VCC: {vcc:?}  PWR.GND: {gnd:?}"
        );
        assert!(
            !vcc.contains(&"cap5.2"),
            "D8 broadcast abolished (§5.3.1): cap5.2 must NOT land on PWR.VCC. \
             PWR.VCC: {vcc:?}"
        );
    }
}
