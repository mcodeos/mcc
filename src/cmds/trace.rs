// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc trace <KEY>` — one object, followed along the chain.
//!
//! Design: `mcd/doc/pipeline/stage-readout-design.md` §5.3 ③. The key's form is
//! read off the key itself (four forms, no `--kind`), and the walk goes over the
//! hop readouts `join` already builds, so the class this prints for an object is
//! the class `join` prints for it (§5.3 ruling ③).
//!
//! Two faces from one `items`, and a **readout**, not a verdict: the exit code
//! is 0 whenever a key was traced at all. An argument that cannot be honoured —
//! a key of no form, or one that resolves to nothing in this build — is not a
//! judged readout, and fails loudly, the same split `join` makes.

use anyhow::Result;
use serde_json::Value;
use tracing::error;

use mcc::cli::OutputFormat;

/// Render `mcc trace <KEY>`.
pub fn run(args: &mcc::cli::TraceArgs) -> Result<()> {
    // The file: `-F` wins, else the cwd manifest that `prepare` already loaded
    // through, exactly as `join` and `show stage` resolve it.
    let target = crate::cmds::manifest::effective_target(args.file.as_deref());
    // The library is loaded before the project, as it is for `join`: the
    // standard components are defs like any other, and a world built without
    // them would resolve a canonical path or a def key to nothing.
    crate::cmds::manifest::init_local(target.as_deref(), &mcc::cli::globals().lib);
    let (mut entry_uri, resolved_top) = crate::cmds::common::load_target(
        target.as_deref(),
        mcc::cli::globals().top.as_deref(),
        mcc::cli::globals().entry.as_deref(),
    )?;
    if entry_uri.is_empty() {
        entry_uri = mcc::mcb_iter_modules()
            .iter()
            .find(|(n, _)| Some(n.clone()) == resolved_top.clone())
            .map(|(_, u)| u.to_string())
            .or_else(|| mcc::mcb_iter_modules().first().map(|(_, u)| u.to_string()))
            .unwrap_or_default();
    }
    let top = resolved_top
        .or_else(|| {
            crate::cmds::common::resolve_top_module(&entry_uri, mcc::cli::globals().top.clone())
        })
        .unwrap_or_else(|| {
            error!("no modules found\nhint: load a file with -F or use --top");
            std::process::exit(1);
        });

    let (tree, table, arena, store, diags) = match mcc::export::build_tree_diags(
        &entry_uri,
        Some(top.as_str()),
        &mcc::cli::globals().lib,
    ) {
        Ok(quint) => quint,
        Err(e) => {
            error!("trace: {e}");
            std::process::exit(1);
        }
    };

    // All three hops of one build: the source hop is generated from the table,
    // and the two circuit hops from one vector graph — cloned rather than built
    // twice, because the second hop's builder consumes the graph it renders.
    let src_p2 = mcc::stages::join::build_join_src_p2(&table, &top, diags.len());
    let block = mcc::build_mc_vec_with_arena(&tree, &table, &arena, &store);
    let (graph, log) = mcc::vector::graph::build_mc_vec_graph_with_log(&block, &table);
    let p2_vec =
        mcc::stages::join::build_join_p2_vec_with_sides(&graph, &log, &table, &top, diags.len());
    let vec_viz =
        mcc::stages::join::build_join_vec_viz_with_sides(graph, &log, &table, &top, diags.len());

    let view = match mcc::stages::trace::build_trace(&args.key, &src_p2, &p2_vec, &vec_viz, &top) {
        Ok(view) => view,
        Err(e) => {
            error!("{e}");
            std::process::exit(2);
        }
    };

    if matches!(
        mcc::cli::globals().format,
        OutputFormat::Text | OutputFormat::Csv
    ) {
        // CSV falls back to the text face on purpose, as `join` does: a
        // fixed-width readout is not CSV-safe, so a real CSV face is a separate
        // decision rather than a fake one.
        let rendered = mcc::stages::trace::render_trace_text(&view);
        return write_text(&rendered);
    }
    emit_envelope(&view)
}

/// Write the text face to `--output` or stdout.
fn write_text(rendered: &str) -> Result<()> {
    if let Some(path) = &mcc::cli::globals().output {
        std::fs::write(path, format!("{rendered}\n"))?;
    } else {
        println!("{rendered}");
    }
    Ok(())
}

/// Emit the view through the standard envelope channel — a new `view` value on
/// the existing envelope, not a second envelope format (law B).
fn emit_envelope(view: &mcc::stages::StageView) -> Result<()> {
    let mut builder = crate::output::builder::ResultBuilder::start("mcc trace");
    builder.set_stage(crate::output::envelope::StageViewData {
        schema_version: view.schema_version.to_string(),
        world_ver: view.world_ver.clone(),
        mcc_version: view.mcc_version.clone(),
        view: view.view.to_string(),
        top: view.top.clone(),
        items: Value::Array(view.items.clone()),
        counts: view.counts.clone(),
    });
    let env = crate::output::envelope::Envelope::ok(builder.finish());
    crate::output::emit_envelope(&env, mcc::cli::globals().format, None, true)
}
