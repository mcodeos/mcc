// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use super::*;

// === handle_check (lines 1462-1528 in original) ===

pub fn handle_check(params: Option<Value>) -> RpcResult {
    let p: CheckRpcParams = parse_or_default(params)?;
    load_libs_rpc(&p.libs);

    // ── Mode A: inline content (AI dry-run) (M6) ──
    if let Some(content) = &p.content {
        // Phase 8.1: unique overlay URI per request → no concurrent cross-contamination
        let uri = super::make_overlay_uri();
        crate::mcc_load_from_string(&uri, content);

        // Phase 8.2: unified diagnostic serialization via lsp/diagnostics
        let diags = crate::lsp::diagnostics::collect_all_full();
        let errors = diags.iter().filter(|d| d["severity"] == "error").count();
        let warnings = diags.iter().filter(|d| d["severity"] == "warning").count();

        // Phase 8.1: clean up overlay so it doesn't accumulate in workspace
        super::remove_overlay(&uri);

        return Ok(json!({
            "summary": { "errors": errors, "warnings": warnings },
            "diagnostics": diags,
        }));
    }

    // ── Mode B/C/D: disk file / project / workspace ──
    let (id, kind, _) = crate::workspace_info();
    if kind == "Anonymous" {
        let entry = p
            .entry
            .as_deref()
            .ok_or_else(|| JsonRpcError::custom(-32602, "check: need <entry> or <content>"))?;

        let cwd = std::env::current_dir().unwrap_or_default();
        let entry_path = PathBuf::from(entry);
        let abs_entry = if entry_path.is_absolute() {
            entry_path.clone()
        } else {
            cwd.join(&entry_path)
        };

        let _uri = McURI::from(abs_entry.to_string_lossy().as_ref() as &str);

        return run_full_build(&abs_entry, None, "check", "file", &id, false);
    }

    let bp = json!({ "entry": p.entry, "include_system": false });
    handle_build_full(Some(bp))
}

// === handle_caps (Phase 8.3: auto-generated from METHOD registry) ===
pub fn handle_caps(_params: Option<Value>) -> RpcResult {
    Ok(super::caps_json())
}

// === handle_explain (lines 4201-4236 in original) ===
pub fn handle_explain(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize, Default)]
    struct ExplainParams {
        code: Option<u32>,
    }

    let p: ExplainParams = parse_or_default(params)?;

    match p.code {
        Some(code) => match crate::error_codes::describe(code) {
            Some(info) => Ok(json!({
                "code": info.code,
                "name": info.name,
                "description": info.description,
            })),
            None => Err(JsonRpcError::custom(
                -32003,
                &format!("unknown error code: {code}"),
            )),
        },
        None => {
            let all = crate::error_codes::all_codes();
            let items: Vec<Value> = all
                .iter()
                .map(|e| {
                    json!({
                        "code": e.code,
                        "name": e.name,
                        "description": e.description,
                    })
                })
                .collect();
            Ok(json!({ "codes": items }))
        }
    }
}
