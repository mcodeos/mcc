// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc query <EXPR | name>` — unified F4 read engine over loaded definitions.
//!
//! A positional that compiles as a DSL expression is evaluated structurally
//! (`kind=component AND name=RES*`); anything else is treated as a def-name
//! search (case-insensitive substring by default; `--regex`/`--fuzzy` select
//! other matchers, `--substring` forces the substring flavor). `mcc search`
//! is a clap alias of this command and equals a bare-name substring query —
//! no second engine. The CLI runs local; the `defs.query`/`defs.search` RPC
//! handlers provide the same capability to IDE/LSP/direct RPC callers.

use crate::cmds::manifest;
use crate::output::{
    self,
    builder::ResultBuilder,
    envelope::{Envelope, QueryData},
    OutputFormatExt,
};
use anyhow::Result;
use mcc::cli::{QueryArgs, SearchKind as CliSearchKind};
use mcc::search_api::{self, SearchHit, SearchInputs};
use serde_json::{json, Value};

pub fn run(args: &QueryArgs) -> Result<()> {
    run_local(args)
}

fn run_local(args: &QueryArgs) -> Result<()> {
    manifest::init_local(args.target.as_deref(), &mcc::cli::globals().lib);
    // Unified target loading: directory → project mode (manifest-driven,
    // browse fallback), file → loaded directly.
    if let Some(target) = &args.target {
        crate::cmds::common::load_target(
            Some(target),
            mcc::cli::globals().top.as_deref(),
            mcc::cli::globals().entry.as_deref(),
        )?;
    }

    // ── Mode selection ─────────────────────────────────────────────────
    // Any matcher flag forces NAME mode. Without one, try the DSL: success →
    // DSL mode (historical `mcc query`); a failure that "looks like DSL" is a
    // real compile error; otherwise the value is a bare name → substring mode.
    let hits = if args.regex || args.fuzzy || args.substring {
        name_hits(args, &args.expr, args.regex, args.fuzzy)?
    } else {
        match mcc::query_api::compile(&args.expr) {
            Ok(q) => {
                let inputs = base_inputs(args, String::new(), None, false, false, None);
                mcc::search_api::walk_defs(&inputs, Some(&q))?
            }
            Err(e) if expr_looks_like_dsl(&args.expr) => return Err(e),
            Err(_) => name_hits(args, &args.expr, false, false)?,
        }
    };

    let items: Vec<Value> = hits
        .iter()
        .map(|h| {
            let mut v = json!({
                "kind": h.kind,
                "name": h.name,
                "uri": h.uri,
            });
            if let Some(c) = &h.class {
                v["class"] = json!(c);
            }
            v
        })
        .collect();
    let count = items.len();

    let data = QueryData {
        expr: args.expr.clone(),
        count,
        items: Value::Array(items.clone()),
    };

    let format = if args.json {
        mcc::cli::OutputFormat::Json
    } else {
        mcc::cli::globals().format
    };

    let mut builder = ResultBuilder::start("mcc query");
    builder.set_query(data);
    let env = Envelope::ok(builder.finish());
    output::emit_envelope(
        &env,
        format,
        mcc::cli::globals()
            .output
            .as_deref()
            .map(std::path::Path::new),
        false,
    )?;

    // Text mode convention: one hit per line on stdout, count to stderr.
    if !format.is_structured() {
        for it in &items {
            let kind = it.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
            let name = it.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let class = it.get("class").and_then(|v| v.as_str());
            match class {
                Some(c) => println!("{}\t{}\t{}", kind, name, c),
                None => println!("{}\t{}", kind, name),
            }
        }
        eprintln!("({} items)", count);
    }
    Ok(())
}

/// Build search inputs shared by both modes. `top` only matters for
/// `--kind instance` (the named top module must be in scope for this run).
fn base_inputs(
    args: &QueryArgs,
    pattern: String,
    kind: Option<search_api::SearchKind>,
    regex: bool,
    fuzzy: bool,
    top: Option<String>,
) -> SearchInputs {
    SearchInputs {
        pattern,
        kind,
        regex,
        fuzzy,
        top,
        limit: args.limit,
        libs: mcc::cli::globals().lib.clone(),
    }
}

/// NAME mode: match `pattern` against def names with the given matcher flavor.
/// Substring is the engine default when `regex == fuzzy == false`.
fn name_hits(args: &QueryArgs, pattern: &str, regex: bool, fuzzy: bool) -> Result<Vec<SearchHit>> {
    let inputs = base_inputs(
        args,
        pattern.to_string(),
        args.kind.map(cli_to_api_kind),
        regex,
        fuzzy,
        mcc::cli::globals().top.clone(),
    );
    mcc::search_api::walk_defs(&inputs, None)
}

fn cli_to_api_kind(k: CliSearchKind) -> search_api::SearchKind {
    match k {
        CliSearchKind::Component => search_api::SearchKind::Component,
        CliSearchKind::Module => search_api::SearchKind::Module,
        CliSearchKind::Interface => search_api::SearchKind::Interface,
        CliSearchKind::Enum => search_api::SearchKind::Enum,
        CliSearchKind::Instance => search_api::SearchKind::Instance,
    }
}

/// True when `s` carries enough DSL structure that a `compile()` failure must
/// be surfaced rather than silently degraded to a bare-name substring search.
/// Consulted only on the error path — any expression that compiles is untouched.
fn expr_looks_like_dsl(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() {
        return false;
    }
    // (A) Comparison-operator characters (= != ~= >= <= and the malformed bare
    //     `~` whose `use '~=' for regex` hint must not be swallowed).
    if t.contains('=') || t.contains('~') || t.contains('<') || t.contains('>') {
        return true;
    }
    // (B) Grouping / attr(...) parens.
    if t.contains('(') || t.contains(')') {
        return true;
    }
    // (C) Internal whitespace → multiple tokens. Def names never contain
    //     spaces, so a multi-token value is an intended DSL phrase (e.g. the
    //     missing-`=` typo `name RES`).
    if t.chars().any(|c| c.is_whitespace()) {
        return true;
    }
    // (D) A single bare reserved/field keyword — the parser rejects these at
    //     parse_field (`name|kind|class|attr` → "unknown field") or treats them
    //     as reserved connectors. Whole-token match only, so real def names
    //     like `and_gate` / `name_X` are unaffected.
    matches!(
        t.to_ascii_lowercase().as_str(),
        "name" | "kind" | "class" | "attr" | "and" | "or" | "not"
    )
}
