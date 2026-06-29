// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc build` — one-shot build (manifest-driven)
//!
//! Read manifest → auto-load dependencies → Pass1 + Pass2 → envelope.
//!
//! ## Output funnel
//!
//! Iteration B: hand off the direct `eprintln!("[Pass 1] ...")` /
//! `eprintln!("[Pass 2] ...")` calls entirely to the `Renderer` trait. So:
//!   - JSON mode → `SilentRenderer`, stdout is clean and only emits the envelope
//!   - Text mode → `TextRenderer`, visually consistent with `mcc parse`
//!
//! ## Exit code
//!
//! `run` returns `(Result<()>, usize)`: success/failure + error count.
//! `dispatch` sets `exit_code` based on this (aligned with `check`).

use crate::cli::{rpc_client::RpcClient, BuildArgs, OutputFormat};
use crate::cmds::manifest;
use crate::cmds::proj::resolve_workspace_ref;
use crate::output::{
    self, builder::ResultBuilder, diagnostic::PhaseTracker, envelope::*, renderer, OutputFormatExt,
};
use anyhow::{Context, Result};
use mcc::McIds;
use serde_json::json;
use std::path::{Path, PathBuf};

/// Build command return result (includes exit_code; aligned with `check`)
pub struct BuildOutcome {
    pub exit_code: i32,
}

pub fn run(args: &BuildArgs) -> Result<BuildOutcome> {
    match RpcClient::probe() {
        Some(c) => run_rpc(&c, args),
        None => run_local(args),
    }
}

fn resolve_project_root(args: &BuildArgs) -> PathBuf {
    if let Some(ref entry) = args.entry {
        let entry_path = Path::new(entry);
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
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
    let entry_abs = if let Some(ref e) = args.entry {
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

    let result = c.call(
        "build.full",
        json!({
            "entry": entry_abs.to_string_lossy(),
            "top": args.top,
            "libs": libs,
            "include_system": args.include_system,
        }),
    )?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    // RPC mode conservatively returns 0 (the server side has its own logs)
    Ok(BuildOutcome { exit_code: 0 })
}

fn run_local(args: &BuildArgs) -> Result<BuildOutcome> {
    let renderer = renderer::for_format(args.format);
    let mut builder = ResultBuilder::start("mcc build").workspace(resolve_workspace_ref());
    let mut tracker = PhaseTracker::new();
    tracker.skip();

    // ── 0.5. Pass 0 snapshot: lib load + manifest + project load phase diagnostics ──
    builder.set_pass0(crate::cmds::parse::public_collect_pass0());

    // ── 1. manifest parsing ──
    let project_root: PathBuf = resolve_project_root(args);
    tracing::debug!(target: "mcc::build", project_root = ?project_root, "resolved project root");

    let (entry_uri, top_name) = match manifest::build_from_manifest(
        &project_root,
        args.top.as_deref(),
        args.entry.as_deref(),
    ) {
        Ok(r) => r,
        Err(e) => {
            let err = RpcError::invalid_params(format!("{:#}", e));
            emit_err(&args.format, err)?;
            return Ok(BuildOutcome { exit_code: 1 });
        }
    };

    // ── 2. Pass1 ──
    renderer.pass1_header(&entry_uri);
    renderer.pass1_definitions(
        mcc::mcb_module_count(),
        mcc::mcb_component_count(),
        mcc::mcb_interface_count(),
    );
    builder.set_pass1(crate::cmds::parse::public_collect_pass1(
        &entry_uri,
        &mut tracker,
    ));

    // ── 3. Pass2 ──
    let ident = McIds::from(top_name.as_str());
    if mcc::get_def(&ident, &entry_uri).is_none() {
        let err = RpcError::invalid_params(format!("'{}' not found", top_name));
        emit_err(&args.format, err)?;
        return Ok(BuildOutcome { exit_code: 1 });
    }

    renderer.pass2_header(&top_name);
    let inst = match mcc::mcc_build(&ident, &entry_uri) {
        Ok(i) => {
            renderer.instances(&i, 0);
            renderer.nets(&i, 0);
            renderer.net_summary(&i);
            builder.set_pass2(crate::cmds::parse::public_collect_pass2(
                &top_name,
                &i,
                &mut tracker,
            ));
            i
        }
        Err(e) => {
            renderer.pass2_failed(&format!("{}", e));
            let err = RpcError::build_error(format!("{}", e));
            emit_err(&args.format, err)?;
            return Ok(BuildOutcome { exit_code: 1 });
        }
    };

    // ── 4. Viz generation ──
    if args.viz {
        let table = mcc::mcc_build_flat(&ident, &entry_uri, 1000)
            .map_err(|e| anyhow::anyhow!("mcc_build_flat failed: {}", e))?;

        // ── DEBUG: check whether mcu513 sub-module's components is empty ──
        let sub = inst.sub_modules.iter().find(|s| s.name == "mcu513");
        eprintln!(
            "[CHK] inst-side mcu513.components = {}",
            sub.map(|s| s.components.len()).unwrap_or(0)
        );

        // ── DEBUG: dump the full InstTable to diagnose net drop points ──
        eprintln!("[DUMP] ====== InstTable contents ======");
        table.1.dump();
        eprintln!("[DUMP] ==============================");

        let (vec_block, build_report) = mcc::build_mc_vec_with_report(&inst, &table.1);
        let graph = mcc::build_mc_vec_graph(&vec_block, &table.1);

        let opts = mcc::viz::api::RenderOpts::default();
        let (doc, metrics) = mcc::viz::api::render_with_metrics(graph, opts);
        let (fidelity, readability) = metrics.finish(Some(&build_report));
        eprintln!("{}", fidelity.report_line());
        eprintln!("{}", readability.report_line());

        // [P0-DET] CLI golden guard: compare against baseline when MCC_GOLDEN_CHECK is set
        if std::env::var("MCC_GOLDEN_CHECK").is_ok() {
            let sig = doc.to_json();
            let gp = std::path::PathBuf::from("tests/golden/hbl.golden.json");
            if std::env::var("UPDATE_GOLDEN").is_ok() || !gp.exists() {
                std::fs::create_dir_all(gp.parent().unwrap()).ok();
                std::fs::write(&gp, &sig).ok();
                eprintln!("[golden] wrote {}", gp.display());
            } else if sig != std::fs::read_to_string(&gp).unwrap_or_default() {
                eprintln!(
                    "[golden] MISMATCH vs {} (UPDATE_GOLDEN=1 to refresh)",
                    gp.display()
                );
                return Ok(BuildOutcome { exit_code: 1 });
            }
        }

        let html = mcc::viz::template::wrap_document(&doc);

        let output_path = args.output.as_deref().unwrap_or("circuit.html");
        std::fs::write(output_path, &html)
            .with_context(|| format!("failed to write file: {}", output_path))?;
        renderer.viz_written(output_path, html.len());
    }

    // ── 5. Exit code: based on error count ──
    let errors = builder.error_count();

    // ── 6. Emit envelope ──
    let env = Envelope::ok(builder.finish());
    let envelope_target = if args.viz && args.output.is_some() {
        None
    } else {
        args.output.as_deref().map(Path::new)
    };
    output::emit_envelope(&env, args.format, envelope_target, false)?;
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

#[cfg(test)]
mod phase0_golden {
    use crate::cmds::manifest;
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
        let inst = mcc::mcc_build(&ident, &entry_uri).expect("mcc_build");
        let table = mcc::mcc_build_flat(&ident, &entry_uri, 1000).expect("mcc_build_flat");
        let vec_block = mcc::build_mc_vec(&inst, &table.1);
        mcc::build_mc_vec_graph(&vec_block, &table.1)
    }

    /// Fingerprint = VizDocument::to_json() (structure + per-layer SVG).
    fn render_signature(graph: mcc::vector::graph::McVecGraph) -> String {
        let opts = mcc::viz::api::RenderOpts::default(); // default = FlowLayouter
        mcc::viz::api::render_with(graph, opts).to_json()
    }

    /// Core guard: same input rendered twice must produce byte-identical fingerprints.
    /// Isolates layout+route determinism; any HashMap-order leak (including in flow.rs)
    /// will surface here.
    #[test]
    fn determinism_render_twice() {
        let Some((root, entry, top)) = hbl_project() else {
            eprintln!("[phase0] set MCC_GOLDEN_PROJECT to enable; skipping");
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
    fn determinism_two_builds() {
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
    fn golden_roundtrip_hbl() {
        let Some((root, entry, top)) = hbl_project() else {
            return;
        };
        let sig = render_signature(build_graph(&root, entry.as_deref(), top.as_deref()));
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/hbl.golden.json");
        if std::env::var("UPDATE_GOLDEN").is_ok() || !path.exists() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &sig).unwrap();
            eprintln!("[golden] wrote baseline -> {}", path.display());
            return;
        }
        let golden = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            sig, golden,
            "hbl render changed vs golden. If intended: UPDATE_GOLDEN=1 cargo test golden_roundtrip_hbl"
        );
    }

    /// Smoke test: metrics accumulation on hbl produces sensible counts.
    #[test]
    fn metrics_hbl_smoke() {
        let Some((root, entry, top)) = hbl_project() else {
            return;
        };
        let graph = build_graph(&root, entry.as_deref(), top.as_deref());
        let (_, metrics) = mcc::viz::api::render_with_metrics(
            graph,
            mcc::viz::api::RenderOpts::default(),
        );
        let (fid, read) = metrics.finish(None); // self-consistent even without build report
        eprintln!("{}", fid.report_line());
        eprintln!("{}", read.report_line());
        assert!(fid.pins_rendered <= fid.pins_total);
        assert!(fid.nets_rendered <= fid.nets_total);
        assert!(read.total_wirelength >= 0.0 && read.weighted() >= 0.0);
    }
}
