// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc query <EXPR | name> [<target>]` — unified F4 read engine over loaded
//! definitions and (with `--kind net`) the top module's Pass2 net table.
//!
//! A positional that compiles as a DSL expression is evaluated structurally
//! (`kind=component AND name=RES*`); anything else is treated as a def-name
//! search (case-insensitive substring by default; `--regex`/`--fuzzy` select
//! other matchers, `--substring` forces the substring flavor). `mcc search`
//! is a clap alias of this command and equals a bare-name substring query —
//! no second engine. The CLI runs local; the `defs.query`/`defs.search` RPC
//! handlers provide the same def-search to IDE/LSP/direct RPC callers.
//!
//! `--kind net` projects the top module's net table (`{name, points}` through
//! `cmds/nets.rs` — the same fold `extract nets`, `list nets` and `show` use).
//! `net` is *not* a definition kind, so it never reaches the lib search
//! engine; this command guards `Net` and routes it to `run_nets`. `extract`
//! keeps its own top-level verb during the migration window but shares this
//! projection (extract-merge plan).

use crate::cmds::common;
use crate::cmds::manifest;
use crate::cmds::nets;
use crate::output::{
    self,
    builder::ResultBuilder,
    envelope::{Envelope, QueryData},
    OutputFormatExt,
};
use anyhow::Result;
use mcc::cli::{OutputFormat, QueryArgs, SearchKind as CliSearchKind};
use mcc::export;
use mcc::search_api::{self, SearchHit, SearchInputs};
use mcc::McURI;
use serde_json::{json, Value};
use std::path::Path;

/// Text/csv row flavor of a query result set.
#[derive(Copy, Clone)]
enum RowStyle {
    /// Definition/instance search hits — text `kind\tname[\tclass]`, csv
    /// `kind,name,uri,class,inst_kind`.
    Hit,
    /// Net projection rows (`{name, points}`) — text `name` per line, csv
    /// `name,points`.
    Name,
}

pub fn run(args: &QueryArgs) -> Result<()> {
    run_local(args)
}

fn run_local(args: &QueryArgs) -> Result<()> {
    manifest::init_local(args.target.as_deref(), &mcc::cli::globals().lib);
    // Unified target loading: directory → project mode (manifest-driven,
    // browse fallback), file → loaded directly. The returned entry uri + top
    // drive `--kind net`'s Pass2 build.
    let (entry_uri, manifest_top) = match &args.target {
        Some(target) => common::load_target(
            Some(target.as_str()),
            mcc::cli::globals().top.as_deref(),
            mcc::cli::globals().entry.as_deref(),
        )?,
        None => (String::new(), None),
    };

    // `--kind net` is a raw-table projection, not a def search: branch before
    // the DSL/name mode logic so `Net` never reaches `cli_to_api_kind`.
    if args.kind == Some(CliSearchKind::Net) {
        return run_nets(args, &entry_uri, manifest_top.as_deref());
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
            // Instance hits carry the per-instance kind tag (component |
            // module | label | ...); definition-kind hits leave it absent.
            if let Some(k) = &h.inst_kind {
                v["inst_kind"] = json!(k);
            }
            v
        })
        .collect();

    emit_query(args.expr.clone(), items, RowStyle::Hit, args.json)
}

/// `--kind net`: project the top module's Pass2 net table `{name, points}`.
/// `<EXPR>` (when given and not DSL-shaped) filters net names through the same
/// matcher flags; an empty expression lists every net.
fn run_nets(args: &QueryArgs, entry_uri: &str, manifest_top: Option<&str>) -> Result<()> {
    // Net names are not defs — a DSL phrase has no meaning against this table.
    // Surface the mistake instead of silently reinterpreting it as a name.
    if expr_looks_like_dsl(&args.expr) {
        return Err(anyhow::anyhow!(
            "--kind net: <EXPR> must be a bare net-name pattern (empty = all); \
             DSL expressions do not apply to nets: {:?}",
            args.expr
        ));
    }

    // Top-module resolution: manifest top (directory/project target) → the
    // shared chain (global --top → module by entry uri → first module).
    let top = match manifest_top {
        Some(t) => t.to_string(),
        None => match common::resolve_top_module(entry_uri, None) {
            Some(t) => t,
            None => {
                return Err(anyhow::anyhow!(
                    "--kind net: no top module found (pass a target file, or set --top <NAME>)"
                ))
            }
        },
    };

    // A file/dir target pins the Pass2 entry; without one, resolve by registry
    // name like `list nets` / `show` do.
    let uri = if entry_uri.is_empty() {
        None
    } else {
        Some(McURI::from(entry_uri))
    };

    let matcher = search_api::build_matcher(&args.expr, args.regex, args.fuzzy)?;
    let nets = nets::top_nets(&top, uri.as_ref()).map_err(anyhow::Error::msg)?;

    let mut items: Vec<Value> = Vec::new();
    for (name, points) in nets {
        if matcher(&name) {
            items.push(json!({ "name": name, "points": points }));
        }
    }
    if args.limit > 0 && items.len() > args.limit {
        items.truncate(args.limit);
    }

    emit_query(args.expr.clone(), items, RowStyle::Name, args.json)
}

/// Serialize a projection result set: envelope for structured formats, raw CSV
/// for `-f csv`, tab/name detail lines after the envelope text report otherwise.
fn emit_query(expr: String, items: Vec<Value>, style: RowStyle, json_flag: bool) -> Result<()> {
    let count = items.len();
    let format = if json_flag {
        OutputFormat::Json
    } else {
        mcc::cli::globals().format
    };
    let output = mcc::cli::globals().output.as_deref().map(Path::new);

    // `csv` is a raw projection artifact (like `export`), not an envelope:
    // intercept before `emit_envelope`, whose Csv arm text-falls-through.
    if format == OutputFormat::Csv {
        let raw = csv_for(&style, &items);
        match output {
            Some(p) => std::fs::write(p, raw)?,
            None => {
                print!("{}", raw);
                if !raw.ends_with('\n') {
                    println!();
                }
                eprintln!("({} items)", count);
            }
        }
        return Ok(());
    }

    let data = QueryData {
        expr,
        count,
        items: Value::Array(items.clone()),
    };
    let mut builder = ResultBuilder::start("mcc query");
    builder.set_query(data);
    let env = Envelope::ok(builder.finish());
    output::emit_envelope(&env, format, output, false)?;

    // Text-mode convention: details → stdout, count → stderr.
    if !format.is_structured() {
        for it in &items {
            match style {
                RowStyle::Hit => {
                    let kind = it.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
                    let name = it.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                    let class = it.get("class").and_then(|v| v.as_str());
                    match class {
                        Some(c) => println!("{}\t{}\t{}", kind, name, c),
                        None => println!("{}\t{}", kind, name),
                    }
                }
                RowStyle::Name => {
                    let name = it.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                    println!("{}", name);
                }
            }
        }
        eprintln!("({} items)", count);
    }
    Ok(())
}

/// Stable-column CSV for a projection. serde_json maps sort their keys, so the
/// columns are declared explicitly here rather than derived from a key union.
fn csv_for(style: &RowStyle, items: &[Value]) -> String {
    let mut out = String::new();
    match style {
        RowStyle::Hit => {
            out.push_str("kind,name,uri,class,inst_kind\n");
            for it in items {
                let cell =
                    |k: &str| export::csv_escape(it.get(k).and_then(|v| v.as_str()).unwrap_or(""));
                out.push_str(&format!(
                    "{},{},{},{},{}\n",
                    cell("kind"),
                    cell("name"),
                    cell("uri"),
                    cell("class"),
                    cell("inst_kind"),
                ));
            }
        }
        RowStyle::Name => {
            out.push_str("name,points\n");
            for it in items {
                let name = it.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let points = it
                    .get("points")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|p| p.as_str())
                            .collect::<Vec<_>>()
                            .join(";")
                    })
                    .unwrap_or_default();
                out.push_str(&format!(
                    "{},{}\n",
                    export::csv_escape(name),
                    export::csv_escape(&points)
                ));
            }
        }
    }
    out
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
        // `net` is not a definition kind; run_local branches to run_nets before
        // any kind→API mapping, so this arm is unreachable.
        CliSearchKind::Net => unreachable!("--kind net is handled in run_local"),
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
