// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use super::*;

// === handle_sem (lines 3554-3609 in original) ===

pub fn handle_sem(params: Option<Value>) -> RpcResult {
    let p: SemParams = parse_strict(params)?;
    let raw_uri = &p.uri;

    // If content is provided, parse from memory (for unsaved editor content)
    if let Some(ref content) = p.content {
        // ★ Fix: Ensure library context is loaded before parsing
        let mc_uri = McURI::from(raw_uri.as_str());
        ensure_library_loaded(&mc_uri);
        crate::build::loader::mcb_add_from_string(&mc_uri, content);
        crate::build::pass1::mcb_parse_all_modules();
        // ★ Fix: Use canonicalized URI for lookup (same as what mcb_add_from_string uses)
        let canonical_uri = crate::build::pass1::canonicalize_project_uri(&mc_uri);
        let result = try_lookup_sem(&[McURI::from(&canonical_uri)]);
        // ★ Fix: DON'T remove the entry - mcc_query needs it for goto_definition
        // crate::db::cmie::tables::WORKSPACE.mcodes.remove(&McURI::from(&canonical_uri));
        return result.ok_or_else(|| JsonRpcError::custom(32100, "parse from string failed"));
    }

    // Build candidate URIs: exact match + relative path (strip project root from absolute)
    let (_, _, root_str) = crate::workspace_info();
    let cwd = std::env::current_dir().unwrap_or_default();
    let root_path = if Path::new(&root_str).is_absolute() {
        PathBuf::from(&root_str)
    } else {
        cwd.join(&root_str)
    };

    let raw_path = Path::new(raw_uri);
    let mut candidates: Vec<McURI> = vec![McURI::from(raw_uri.as_str())];
    if raw_path.is_absolute() && raw_path.starts_with(&root_path) {
        let rel = raw_path.strip_prefix(&root_path).unwrap_or(raw_path);
        let rel_str = rel.to_string_lossy().to_string();
        if rel_str != *raw_uri {
            candidates.push(McURI::from(rel_str));
        }
    }

    // Try lookup
    let result = try_lookup_sem(&candidates);

    // If not found and workspace is empty, auto-create workspace and load project
    if result.is_none() {
        let workspace_empty = {
            let binding = &crate::db::cmie::tables::WORKSPACE.mcodes;
            binding.is_empty()
        };
        if workspace_empty && raw_path.is_absolute() {
            auto_load_from_file_path(raw_path);
            return try_lookup_sem(&candidates)
                .ok_or_else(|| JsonRpcError::custom(32100, "file not found in workspace"));
        }
    }

    result.ok_or_else(|| JsonRpcError::custom(32100, "file not found in workspace"))
}

// === handle_diagnostics (lines 4239-4277 in original) ===
pub fn handle_diagnostics(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize)]
    struct DiagnosticsParams {
        uri: String,
    }

    let p: DiagnosticsParams = parse_strict(params)?;
    let raw_uri = McURI::from(p.uri.as_str());
    // Canonicalize URI to match the keys used when storing diagnostics
    // (mcb_add_from_string and all diagnostic_log calls use canonical URIs)
    let mc_uri = McURI::from(crate::build::pass1::canonicalize_project_uri(&raw_uri));

    tracing::info!(target: "mcc::rpc", "handle_diagnostics: raw={} canonical={}", raw_uri, mc_uri);

    let diags = crate::lsp::diagnostics::collect(&mc_uri);
    Ok(serde_json::json!({ "diagnostics": diags }))
}

// === handle_project_symbols (lines 4280-4333 in original) ===
pub fn handle_project_symbols(_params: Option<Value>) -> RpcResult {
    use crate::query::iterators::{
        mcb_iter_components_with_span, mcb_iter_enum_values, mcb_iter_enums_with_span,
        mcb_iter_interfaces_with_span, mcb_iter_modules_with_span,
    };

    let components: Vec<serde_json::Value> = mcb_iter_components_with_span()
        .into_iter()
        .map(|(name, uri, span)| serde_json::json!({ "name": name, "uri": uri, "span": span }))
        .collect();

    let interfaces: Vec<serde_json::Value> = mcb_iter_interfaces_with_span()
        .into_iter()
        .map(|(name, uri, span)| serde_json::json!({ "name": name, "uri": uri, "span": span }))
        .collect();

    let enums: Vec<serde_json::Value> = mcb_iter_enums_with_span()
        .into_iter()
        .map(|(name, uri, span)| {
            serde_json::json!({
                "name": name,
                "uri": uri,
                "span": [span[0], span[1]],
            })
        })
        .collect();

    let modules: Vec<serde_json::Value> = mcb_iter_modules_with_span()
        .into_iter()
        .map(|(name, uri, span)| serde_json::json!({ "name": name, "uri": uri, "span": span }))
        .collect();

    // Per-value rows so the extension can do (class, value) -> uri+span lookup
    // for F12 on the value half of `PKG.SOP8`.
    let enum_values: Vec<serde_json::Value> = mcb_iter_enum_values()
        .into_iter()
        .map(|(class, name, uri, span)| {
            serde_json::json!({
                "class": class,
                "name": name,
                "uri": uri,
                "span": [span[0], span[1]],
            })
        })
        .collect();

    Ok(serde_json::json!({
        "components": components,
        "interfaces": interfaces,
        "enums": enums,
        "modules": modules,
        "enum_values": enum_values,
    }))
}

// === handle_init (lines 4360-4368 in original) ===
pub fn handle_init(_params: Option<Value>) -> RpcResult {
    // Use mcc_init() (not mcc_init_no_lib) so that configured system libraries
    // (e.g. `mcode`, providing `enum PKG`) are loaded. The LSP client (mcext)
    // calls `init` on startup; using the no-lib variant here previously wiped
    // the mcode library that was loaded at server startup, which broke enum
    // reference resolution (e.g. goto-definition on `PKG.QFN20`).
    crate::mcc_init();
    Ok(serde_json::json!({ "ok": true }))
}

// === handle_add_file (lines 4384-4393 in original) ===
pub fn handle_add_file(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize)]
    struct AddFileParams {
        uri: String,
    }

    let p: AddFileParams = parse_strict(params)?;
    crate::mcc_add(&McURI::from(p.uri.as_str()));
    Ok(serde_json::json!({ "ok": true }))
}

// === handle_remove_file (lines 4396-4405 in original) ===
pub fn handle_remove_file(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize)]
    struct RemoveFileParams {
        uri: String,
    }

    let p: RemoveFileParams = parse_strict(params)?;
    crate::mcc_remove(&McURI::from(p.uri.as_str()));
    Ok(serde_json::json!({ "ok": true }))
}

// === handle_completion ===
pub fn handle_completion(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize)]
    struct CompletionParams {
        uri: String,
        #[serde(default)]
        prefix: Option<String>,
        #[serde(default)]
        scope: Option<String>,
    }

    let p: CompletionParams = parse_strict(params)?;
    let items = crate::lsp::completion::complete(&p.uri, p.prefix.as_deref(), p.scope.as_deref());
    Ok(serde_json::json!({ "items": items }))
}

// === handle_hover ===
pub fn handle_hover(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize)]
    struct HoverParams {
        name: String,
        uri: String,
    }

    let p: HoverParams = parse_strict(params)?;
    let result = crate::lsp::hover::hover(&p.name, &p.uri);
    Ok(serde_json::json!({ "result": result }))
}
