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

        let vec_block = mcc::build_mc_vec(&inst, &table.1);
        let graph = mcc::build_mc_vec_graph(&vec_block, &table.1);

        let opts = mcc::viz::api::RenderOpts::default();
        let doc = mcc::viz::api::render_with(graph, opts);
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
