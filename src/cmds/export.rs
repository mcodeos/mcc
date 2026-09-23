// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc export <KIND> [FILE]` — thin CLI wrapper around `mcc::export`.
//!
//! The actual build pipeline (netlist/BOM/SPICE) lives in
//! `src/export/mod.rs` (lib root) so the JSON-RPC handler in
//! `rpc/handlers.rs` can share the exact same code without reaching into
//! the binary's private `cmds` module.

use crate::cmds::common;
use crate::cmds::manifest;
use crate::output::envelope::ExportData;
use crate::output;
use anyhow::Result;
use mcc::cli::{rpcclient::RpcClient, ExportArgs, ExportKind, OutputFormat};
use mcc::export;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub fn run(args: &ExportArgs) -> Result<()> {
    // An omitted target defaults to the current directory when it holds a
    // project manifest.
    let target = manifest::effective_target(args.file.as_deref());
    // Pattern B: probe + rpc_mapping + fallthrough to local.
    if let Some(c) = RpcClient::probe() {
        if let Some((method, params)) = rpc_mapping(args, target.as_deref()) {
            // The RPC result deserializes into the same `ExportData` the local
            // arm builds, and both emit through [`emit_json`] — one face, one
            // byte stream, whichever entry point answered (U277).
            match c.call(method, params).and_then(|result| {
                serde_json::from_value::<ExportData>(result)
                    .map_err(|e| anyhow::anyhow!("export payload: {e}"))
            }) {
                Ok(data) => {
                    emit_json(&data, effective_format(args))?;
                    return Ok(());
                }
                Err(e) => tracing::debug!(
                    target: "mcc::export",
                    "RPC failed, falling back to local: {}",
                    e
                ),
            }
        }
    }
    run_local(args, target.as_deref())
}

/// The format the export face answers on: `--json` pins JSON, otherwise the
/// global `--format`. The RPC params and both emitters read it here, so the
/// two entry points always speak the same format.
fn effective_format(args: &ExportArgs) -> OutputFormat {
    if args.json {
        OutputFormat::Json
    } else {
        mcc::cli::globals().format
    }
}

/// The one JSON face both entry points emit through (U277: two entries, one
/// byte stream). The bare payload is the whole answer — no envelope — and `-o`
/// is honored exactly as on the raw-artifact faces.
fn emit_json(data: &ExportData, format: OutputFormat) -> Result<()> {
    let target = mcc::cli::globals().output.clone();
    output::emit_payload_json(data, format, target.as_deref().map(Path::new))
}

/// Map CLI args → RPC method + params. Gated behind `MCC_RPC_EXPORT`; without
/// it export is local-only on the CLI (the `export` server method still exists
/// for direct RPC users).
///
/// Only the payload face maps: the server drops the raw artifact (`raw_text`
/// never crosses RPC), so a text/csv/yaml request would come back with
/// `items: null` — those faces answer locally, where the artifact exists.
fn rpc_mapping(args: &ExportArgs, target: Option<&str>) -> Option<(&'static str, Value)> {
    // The graphical face writes one file per sheet and only the CLI can take
    // that dispatch — the server's own payload for it is a redirect notice.
    if args.kind == ExportKind::KiCadSch || !effective_format(args).is_jsonish() {
        return None;
    }
    if std::env::var("MCC_RPC_EXPORT").is_ok() {
        Some((
            "export",
            json!({
                "kind":   args.kind.name(),
                "entry":  target,
                "top":    mcc::cli::globals().top,
                "format": effective_format(args).name(),
                "libs":   mcc::cli::globals().lib,
            }),
        ))
    } else {
        None
    }
}

fn run_local(args: &ExportArgs, target: Option<&str>) -> Result<()> {
    let Some(target) = target else {
        anyhow::bail!("export: <target> not specified");
    };

    // Shared local initialization: engine + libs (global config, --lib, mcode default).
    manifest::init_local(Some(target), &mcc::cli::globals().lib);

    // A directory target resolves to its manifest's entry file; the export
    // pipeline below consumes a single file.
    let (entry_uri, _) = common::load_target(
        Some(target),
        mcc::cli::globals().top.as_deref(),
        mcc::cli::globals().entry.as_deref(),
    )?;

    let format = effective_format(args);

    let (tree, table, arena, inst_store) = match export::build_tree(
        &entry_uri,
        mcc::cli::globals().top.as_deref(),
        &mcc::cli::globals().lib,
    ) {
        Ok(quad) => quad,
        Err(e) => {
            eprintln!("{}", e);
            return Ok(());
        }
    };

    // Resolve top name for header.
    let top = mcc::cli::globals()
        .top
        .clone()
        .unwrap_or_else(|| mcc::mcb_get_first_module_name().unwrap_or_else(|| "?".into()));

    // The graphical KiCad export writes one .kicad_sch per sheet, so it does
    // not fit the single-payload `build_payload` faces; it writes its files
    // directly and reports them on stderr.
    if args.kind == ExportKind::KiCadSch {
        return write_kicad_sch(&tree, &table, &arena, &inst_store, &top, args.flat);
    }

    let kind_str = args.kind.name();
    let kind_tag = args.kind.id();
    // Both JSON spellings read the same structured payload: the pretty
    // spelling is a serialization choice made at emit time, not a different
    // export format — the exporters fill `items` for the JSON tag only.
    let format_tag = if format.is_jsonish() {
        OutputFormat::Json.id()
    } else {
        format.id()
    };
    let (raw_text, items, count) = export::build_payload(
        &tree,
        &table,
        &arena,
        &inst_store,
        &top,
        kind_tag,
        format_tag,
    );

    if format == OutputFormat::Json || format == OutputFormat::JsonPretty {
        // The JSON face is the bare payload (U277): the envelope around it is
        // retired, and `json-pretty` joins the same face so both JSON
        // spellings serialize the payload, not the raw artifact.
        let data = ExportData {
            kind: kind_str.to_string(),
            format: format.name().to_string(),
            count,
            items,
        };
        emit_json(&data, format)?;
    } else {
        // Raw text/CSV → stdout or file.
        match &mcc::cli::globals().output {
            Some(p) => std::fs::write(p, raw_text.as_bytes().to_vec())?,
            None => {
                print!("{}", raw_text);
                if !raw_text.ends_with('\n') {
                    println!();
                }
                eprintln!("({} items)", count);
            }
        }
    }
    Ok(())
}

/// Write the hierarchical `.kicad_sch` set: the root sheet at `-o` (or
/// `<top>.kicad_sch` in the current directory) and every child sheet beside it.
fn write_kicad_sch(
    tree: &mcc::McModuleInst,
    table: &mcc::InstTable,
    arena: &mcc::NodeArena,
    inst_store: &mcc::InstanceStore,
    top: &str,
    flat: bool,
) -> Result<()> {
    let files = export::kicad_sch::build_kicad_sch_project(tree, table, arena, inst_store, top, flat);
    if files.is_empty() {
        anyhow::bail!("kicad-sch: nothing rendered for top '{top}'");
    }
    let out = mcc::cli::globals().output.clone();
    let (dir, root_name) = match out.as_deref() {
        Some(p) if p.ends_with(".kicad_sch") => {
            let path = Path::new(p);
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| format!("{top}.kicad_sch"));
            (path.parent().unwrap_or(Path::new(".")).to_path_buf(), name)
        }
        Some(p) => (
            PathBuf::from(p),
            format!("{}.kicad_sch", sanitize_stem(top)),
        ),
        None => (
            PathBuf::from("."),
            format!("{}.kicad_sch", sanitize_stem(top)),
        ),
    };
    std::fs::create_dir_all(&dir)?;
    let mut written: Vec<String> = Vec::new();
    for (i, f) in files.iter().enumerate() {
        let name = if i == 0 {
            root_name.clone()
        } else {
            f.name.clone()
        };
        let path = dir.join(&name);
        std::fs::write(&path, f.content.as_bytes())?;
        written.push(path.display().to_string());
    }
    eprintln!("(kicad-sch: {} sheets -> {})", written.len(), dir.display());
    for w in &written {
        eprintln!("  {w}");
    }
    Ok(())
}

fn sanitize_stem(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}
