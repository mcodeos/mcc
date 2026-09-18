// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc join <A> <B>` — two adjacent segments of the chain, matched by key.
//!
//! Design: `mcd/doc/pipeline/stage-readout-design.md` §5.3 ②. The value domain
//! is `src` plus the three segment names, and only **adjacent** pairs are
//! allowed: a hop would otherwise skip the segment in between and lose its
//! `merge` criterion. So this is a small closed set of pairs, not an N×N matrix.
//!
//! Two faces from one `items` (§5.3 ruling ③): `-f text` prints the fixed-width
//! readout and every machine format prints the envelope. The text face obeys the
//! four prohibitions — no ANSI, no box drawing, no tab-delimited columns, a
//! missing value printed as `-` — and it is a **readout**: the diagnostic count
//! is a number in the header and never a gate, so the exit code stays 0 (law C).

use crate::output::die;
use anyhow::Result;

use mcc::cli::OutputFormat;

/// Render `mcc join <a> <b>` for a supported pair.
pub fn run(args: &mcc::cli::JoinArgs) -> Result<()> {
    let (a, b) = (args.a.as_str(), args.b.as_str());
    match (a, b) {
        ("src", "p2") | ("p2", "vec") | ("vec", "viz") => {}
        _ => error_pair(a, b),
    }

    // The file: `-F` wins, else the cwd manifest that `prepare` already loaded
    // through, exactly as `show stage` resolves it.
    let target = crate::cmds::manifest::effective_target(args.file.as_deref());
    // The library must be loaded before the project, as it is for `show`: the
    // standard components (`CAP`, `RES`, `DIO`, ...) are defs like any other, and
    // without them every statement that uses one builds a different world than
    // `show stage p2` reports on — a smaller one, whose statements all read as
    // `drop`. Two readouts of the same hop have to be two readouts of the *same*
    // world, so this is not optional.
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
            die!(
                "mcc::join",
                1,
                "no modules found\nhint: load a file with -F or use --top"
            );
        });

    let (tree, table, arena, store, diags) = match mcc::export::build_tree_diags(
        &entry_uri,
        Some(top.as_str()),
        &mcc::cli::globals().lib,
    ) {
        Ok(quint) => quint,
        Err(e) => die!("mcc::join", 1, "join: {e}"),
    };

    // Which hop, and therefore which two views. The two inner hops need the
    // vector graph; the block it is built from is the same one `show stage vec`
    // and `show stage viz` build, so a `join` reading and a `show` reading of one
    // segment are two readings of one build (§5.3 ruling ③).
    let mut view = match (a, b) {
        ("src", "p2") => mcc::stages::join::build_join_src_p2(&table, &top, diags.len()),
        ("p2", "vec") => {
            let block = mcc::build_mc_vec_with_arena(&tree, &table, &arena, &store);
            let (graph, log) = mcc::vector::graph::build_mc_vec_graph_with_log(&block, &table);
            mcc::stages::join::build_join_p2_vec(&graph, &log, &table, &top, diags.len())
        }
        _ => {
            let block = mcc::build_mc_vec_with_arena(&tree, &table, &arena, &store);
            let (graph, log) = mcc::vector::graph::build_mc_vec_graph_with_log(&block, &table);
            mcc::stages::join::build_join_vec_viz(graph, &log, &table, &top, diags.len())
        }
    };

    // `--only` filters the *same* items the unfiltered readout builds, and only
    // the rows: the counts keep describing the whole hop, so a filtered readout
    // cannot be mistaken for a world with nothing else in it.
    if let Some(only) = args.only.as_deref() {
        if !mcc::stages::join::is_class_word(only) {
            let words: Vec<&str> = mcc::stages::join::SIX_WORDS
                .iter()
                .chain(mcc::stages::join::DIAG_WORDS)
                .copied()
                .collect();
            die!(
                "mcc::join",
                2,
                "unknown class '{only}'\nexpected one of: {}",
                words.join(" | ")
            );
        }
        view.items.retain(|i| i["class"] == only);
    }

    if matches!(
        mcc::cli::globals().format,
        OutputFormat::Text | OutputFormat::Csv
    ) {
        // CSV falls back to the text face on purpose, as `show stage` does: a
        // fixed-width readout is not CSV-safe (a source line may contain a
        // comma), so a real CSV face is a separate decision, not a fake one.
        let rendered = mcc::stages::join::render_join_text(&view);
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
    let mut builder = crate::output::builder::ResultBuilder::start("mcc join");
    builder.set_stage(crate::output::envelope::StageViewData::from(view));
    let env = crate::output::envelope::Envelope::ok(builder.finish());
    crate::output::emit_envelope(&env, mcc::cli::globals().format, None, true)
}

/// An argument that cannot be honoured is not a judged readout, so it may fail
/// loudly — the same split `show stage` makes for an unknown segment.
fn error_pair(a: &str, b: &str) -> ! {
    die!(
        "mcc::join",
        2,
        "cannot join '{a}' with '{b}'\n\
         only adjacent segments join, and only in chain order:\n  \
         join src p2 | join p2 vec | join vec viz"
    );
}
