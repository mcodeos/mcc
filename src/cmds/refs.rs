// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc refs` — Find all references to a symbol.
//!
//! Requires the engine to have collected reference data during Pass1/Pass2.
//!
//! `--circuit` answers the same question in the other space: the name is looked
//! up in the built circuit's reverse index (design
//! `doc/arch/space/organization-units-design.md` §9.6) and the answer is the
//! rows it lands on, not the source spans the definition space holds.

use crate::cmds::common;
use crate::output::{emit_projection, OutputFormatExt, ProjectionKey};
use anyhow::Result;
use mcc::cli::RefsArgs;
use serde_json::{json, Value};
use std::path::Path;

pub fn run(args: &RefsArgs) -> Result<()> {
    // No server arm — ruled 2026-09-18 (mcd/CIMP-OPEN.md §1 U90): carry the context
    // or don't delegate. This one delegated cleanly, envelope and all: measured
    // `mcc refs <sym> -f json` was 284 B in-process vs 283 B over RPC, the sole
    // differing byte being the summary's `interface_count` (57 vs 0). But that
    // one byte is the whole point — the daemon has to answer from the workspace
    // it was *started* on, and the request carries no cwd, so the reference set
    // it reports on is not necessarily the one the caller is standing in.
    // Run in-process.
    run_local(args)
}

/// Structured face → A-tier envelope (U86 item 7, first slice); text / csv are
/// untouched (`refs` was format-blind before the slice — see `emit_report`'s
/// note).
fn emit_refs(data: Value) -> Result<()> {
    if mcc::cli::globals().format.is_structured() {
        return emit_projection(
            ProjectionKey::Refs,
            data,
            mcc::cli::globals().format,
            mcc::cli::globals().output.as_deref().map(Path::new),
        );
    }
    println!("{}", serde_json::to_string_pretty(&data)?);
    Ok(())
}

fn run_local(args: &RefsArgs) -> Result<()> {
    // An omitted target defaults to the current directory when it holds a
    // project manifest.
    let file = crate::cmds::manifest::effective_target(args.file.as_deref());
    crate::cmds::manifest::init_local(file.as_deref(), &mcc::cli::globals().lib);

    // `--circuit` needs a built top, which the target load below produces; it
    // branches before the definition-space reference lookup so that a name is
    // never answered twice, once per space.
    if args.circuit {
        return run_circuit(args, file.as_deref());
    }

    if let Some(f) = file.as_deref() {
        if std::path::Path::new(f).is_dir() {
            crate::cmds::common::load_target(
                Some(f),
                mcc::cli::globals().top.as_deref(),
                mcc::cli::globals().entry.as_deref(),
            )?;
        } else {
            let uri = mcc::McURI::from(f);
            mcc::mcc_load_project(&uri);
        }
    }

    let refs = mcc::mcb_get_refs(&args.name);

    let items: Vec<_> = refs
        .iter()
        .map(|(uri, scope, span)| {
            json!({
                "uri": uri,
                "scope": scope,
                "pos": span.start,
                "end": span.end,
            })
        })
        .collect();

    emit_refs(json!({
        "name": args.name,
        "count": items.len(),
        "refs": items,
    }))
}

/// `--circuit`: the reverse index of the built top (design §9.6).
///
/// Two readings, told apart by `mode` in the payload:
///
/// - `hits` — a non-empty NAME: the circuit rows of every key it matches, with
///   the matched keys named beside them. The match is the engine's usual one
///   for a typed name (case-insensitive substring, `query::search`), over the
///   **keys** rather than the rows — an editor's filter types a prefix, not a
///   whole name, and this is the read that stays cheap under it (§9.4).
/// - `keys` — an empty NAME: every key the index holds, with what each one
///   holds. This is the reading that answers "what can I type at all".
///
/// A NAME that matches no key answers with zero rows and an explanation rather
/// than with a near miss: the caller's `--top` / target decides *which* build
/// is asked, and "this build has no such name" is a fact about that build.
fn run_circuit(args: &RefsArgs, file: Option<&str>) -> Result<()> {
    // Same top/uri resolution chain as `query --kind net`: the target's
    // manifest top, else the global --top, else the module the entry uri names,
    // else the first module.
    let (entry_uri, manifest_top) = match file {
        Some(f) => common::load_target(
            Some(f),
            mcc::cli::globals().top.as_deref(),
            mcc::cli::globals().entry.as_deref(),
        )?,
        None => (String::new(), None),
    };
    let top = match manifest_top {
        Some(t) => t,
        None => common::resolve_top_module(&entry_uri, None).ok_or_else(|| {
            anyhow::anyhow!(
                "refs --circuit: no top module found (pass a target directory, or set --top <NAME>)"
            )
        })?,
    };
    let uri = if entry_uri.is_empty() {
        mcc::mcb_iter_modules()
            .iter()
            .find(|(n, _)| *n == top)
            .map(|(_, u)| mcc::McURI::from(u.as_str()))
            .unwrap_or_else(|| mcc::McURI::from(top.as_str()))
    } else {
        mcc::McURI::from(entry_uri.as_str())
    };

    // One instantiation, projected once — the same `DianLu` entry the render
    // path uses, so the index read here is the index of the world being shown.
    let ident = mcc::McIds::from(top.as_str());
    let mut dl = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        mcc::mcc_build_dianlu(&ident, &uri, 1000)
    }))
    .map_err(|_| anyhow::anyhow!("build panicked (engine Pass2 bug)"))?
    .map_err(|e| anyhow::anyhow!("build failed: {e}"))?;
    dl.flatten();
    let index = dl
        .reverse()
        .ok_or_else(|| anyhow::anyhow!("refs --circuit: the projection produced no index"))?;

    let mut sources = mcc::stages::SourceText::new();
    let listing = args.name.is_empty();
    let (mode, keys, items) = if listing {
        ("keys", Vec::new(), mcc::key_rows(index))
    } else {
        let (keys, rows) = mcc::matched_rows(index, &args.name, &mut sources);
        if keys.is_empty() {
            // A pattern matching no key is a real answer; say which question
            // was asked and how large the index is, so the caller can tell
            // "no such name" from "nothing built".
            eprintln!(
                "refs --circuit: '{}' matches none of this build's {} keys",
                args.name,
                index.len()
            );
        }
        ("hits", keys, rows)
    };

    let mut payload = json!({
        "name": args.name,
        "space": "circuit",
        "top": top,
        "mode": mode,
        "count": items.len(),
        "items": items,
    });
    if !listing {
        // The keys the pattern reached, named explicitly: a row knows the key
        // it was found by, and the list still answers when no row was.
        payload["keys"] = json!(keys);
    }
    emit_refs(payload)
}
