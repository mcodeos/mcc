// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc erc` — Electrical Rule Check.
//!
//! The command is a face, not an engine: it runs Pass2 + flatten and hands the
//! flat instance table to the flat net checks, the same rules `mcc check --nets`
//! and `mcc build` run. Until 2026-09-18 it carried its own root-net engine over
//! the string net table (ERC 6001-6004); that engine is retired
//! (`erc/rules-catalog-design.md` §3.1 E3). `erc` itself is kept — the four
//! checks it used to answer with are all answerable here, by the world's ERC
//! truth, which is the point: two rulers over one design disagreed.
//!
//! The payload comes from `check::nets::erc_payload`, which the RPC `erc`
//! method also renders through, so the two faces cannot drift apart again.
//!
//! ⚠ The CLI no longer delegates to a running server (CIMP §1 U90, ruling (b)).
//! The RPC `erc` method takes no params at all — `handle_erc(_params)` drops
//! them — so the server answered about *its own* world: with a daemon running,
//! `mcc erc <design>` reported `component_count 0 / module_count 0` for a
//! design that has 10 and 7. A reader that returns a *smaller* answer is not
//! the same question's other reading, so the arm is gone. The RPC method stays
//! for direct callers.

use crate::cmds::{common, manifest};
use crate::output::{emit_projection, OutputFormatExt, ProjectionKey};
use anyhow::Result;
use mcc::cli::ErcArgs;
use serde_json::Value;
use std::path::Path;

pub fn run(args: &ErcArgs) -> Result<()> {
    run_local(args)
}

/// Structured face → A-tier envelope (U86 item 7, first slice); text / csv are
/// untouched (`erc` was format-blind before the slice — see `emit_report`'s note).
///
/// ⚠ The envelope's own `summary.errors` does **not** count ERC violations:
/// those live in the payload (`result.erc.summary.violations`). Read the
/// envelope summary as "what this command's passes reported", never as a verdict.
fn emit_erc(data: Value) -> Result<()> {
    if mcc::cli::globals().format.is_structured() {
        return emit_projection(
            ProjectionKey::Erc,
            data,
            mcc::cli::globals().format,
            mcc::cli::globals().output.as_deref().map(Path::new),
        );
    }
    println!("{}", serde_json::to_string_pretty(&data)?);
    Ok(())
}

fn run_local(args: &ErcArgs) -> Result<()> {
    // An omitted target defaults to the current directory when it holds a
    // project manifest.
    let target = manifest::effective_target(args.target.as_deref());
    manifest::init_local(target.as_deref(), &mcc::cli::globals().lib);

    // Unified target loading: directory → project mode (manifest-driven,
    // browse fallback), file → loaded directly. A manifest also declares the
    // design's top module, and that declaration is honored here — `erc` used to
    // ignore it and fall through to the first loaded module, so on a
    // multi-module project `erc` and `build` checked different designs.
    let (entry_uri, manifest_top) = match &target {
        Some(t) => common::load_target(
            Some(t),
            mcc::cli::globals().top.as_deref(),
            mcc::cli::globals().entry.as_deref(),
        )?,
        None => (String::new(), None),
    };

    let top = common::resolve_top_module(&entry_uri, manifest_top)
        .ok_or_else(|| anyhow::anyhow!("erc: no modules found — specify --top"))?;

    // Resolve the module's real URI (modules may live in a different file
    // than the entry), matching `show` / `nets_map`.
    let uri = mcc::mcb_iter_modules()
        .iter()
        .find(|(n, _)| *n == top)
        .map(|(_, u)| u.clone())
        .unwrap_or_else(|| entry_uri.clone());

    let entry = mcc::McSpaceName {
        ident: mcc::McIds::from(top.as_str()),
        uri: mcc::uri_intern(&uri),
    };
    let (_tree, table) = mcc::mcb_pass2_flat(&entry, 1).map_err(|e| anyhow::anyhow!("erc: {e}"))?;

    let results = mcc::check::nets::run_net_checks(&table);
    emit_erc(mcc::check::nets::erc_payload(&top, &results))
}
