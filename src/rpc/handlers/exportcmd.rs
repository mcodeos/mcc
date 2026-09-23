// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use super::*;

// === handle_export (lines 595-648 in original) ===

pub fn handle_export(params: Option<Value>) -> RpcResult {
    let p: ExportRpcParams = parse_or_default(params)?;
    let kind = crate::cli::ExportKind::from_name(&p.kind);
    let format = crate::cli::OutputFormat::from_name(p.format.as_deref().unwrap_or("text"));
    let (tree, table, arena, inst_store) =
        crate::export::build_tree(&p.entry, p.top.as_deref(), &p.libs)
            .map_err(|e| JsonRpcError::custom(-32603, &format!("export: {}", e)))?;
    let top = p.top.clone().unwrap_or_else(|| "?".to_string());
    let kind_tag = kind.id();
    // Both JSON spellings read the same structured payload (U277): the pretty
    // spelling is a serialization choice at emit time, not a different export
    // format — the exporters fill `items` for the JSON tag only. Mirrors the
    // CLI's own split, so both entry points answer with one payload.
    let format_tag = if format.is_jsonish() {
        crate::cli::OutputFormat::Json.id()
    } else {
        format.id()
    };
    let (raw_text, items, count) = crate::export::build_payload(
        &tree,
        &table,
        &arena,
        &inst_store,
        &top,
        kind_tag,
        format_tag,
    );
    let _ = raw_text; // raw artifact; for RPC we return structured items
    Ok(json!({
        "kind": kind.name(),
        "format": format.name(),
        "count": count,
        "items": items,
    }))
}
