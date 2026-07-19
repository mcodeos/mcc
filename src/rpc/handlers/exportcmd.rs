// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use super::*;

// === handle_export (lines 595-648 in original) ===

pub fn handle_export(params: Option<Value>) -> RpcResult {
    let p: ExportRpcParams = parse_or_default(params)?;
    let args = crate::cli::ExportArgs {
        kind: match p.kind.as_str() {
            "bom" => crate::cli::ExportKind::Bom,
            "spice" => crate::cli::ExportKind::Spice,
            "kicad" | "kicad-netlist" => crate::cli::ExportKind::KiCad,
            _ => crate::cli::ExportKind::Netlist,
        },
        file: p.entry,
        top: p.top,
        lib: p.libs,
        format: match p.format.as_deref() {
            Some("json") => crate::cli::OutputFormat::Json,
            Some("json-pretty") => crate::cli::OutputFormat::JsonPretty,
            Some("yaml") => crate::cli::OutputFormat::Yaml,
            Some("csv") => crate::cli::OutputFormat::Csv,
            _ => crate::cli::OutputFormat::Text,
        },
        json: p.format.as_deref() == Some("json"),
        output: None,
    };
    let (tree, table) = crate::export::build_tree(&args.file, args.top.as_deref(), &args.lib)
        .map_err(|e| JsonRpcError::custom(-32603, &format!("export: {}", e)))?;
    let top = args.top.clone().unwrap_or_else(|| "?".to_string());
    // Convert local cli enums → u8 tags for export.
    let kind_tag = match args.kind {
        crate::cli::ExportKind::Netlist => 0u8,
        crate::cli::ExportKind::Bom => 1u8,
        crate::cli::ExportKind::KiCad => 3u8,
        crate::cli::ExportKind::Spice => 2u8,
    };
    let format_tag = match args.format {
        crate::cli::OutputFormat::Text => 0u8,
        crate::cli::OutputFormat::Json => 1u8,
        crate::cli::OutputFormat::JsonPretty => 2u8,
        crate::cli::OutputFormat::Yaml => 3u8,
        crate::cli::OutputFormat::Csv => 4u8,
    };
    let (raw_text, items, count) =
        crate::export::build_payload(&tree, &table, &top, kind_tag, format_tag);
    let kind_str = match kind_tag {
        1 => "bom",
        2 => "spice",
        _ => "netlist",
    };
    let _ = raw_text; // raw artifact; for RPC we return structured items
    Ok(json!({
        "kind": kind_str,
        "format": p.format.unwrap_or_else(|| "text".into()),
        "count": count,
        "items": items,
    }))
}
