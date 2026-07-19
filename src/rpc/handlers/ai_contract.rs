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
        let uri = McURI::from(CHECK_OVERLAY_URI);
        crate::mcc_load_from_string(&uri, content);

        let raw = crate::mcc_diagnose_all();
        let diags: Vec<Value> = raw
            .iter()
            .map(|d| {
                let sev = match d.level {
                    crate::DiagnosticLevel::Error => "error",
                    crate::DiagnosticLevel::Warning => "warning",
                    crate::DiagnosticLevel::Info => "info",
                    crate::DiagnosticLevel::Hint => "hint",
                };
                json!({
                    "code": d.code,
                    "severity": sev,
                    "message": d.msg,
                    "location": {
                        "file": d.loc.uri,
                        "line": d.loc.row,
                        "column": d.loc.col,
                        "pos": d.loc.pos,
                        "len": d.loc.len,
                    }
                })
            })
            .collect();

        let errors = diags.iter().filter(|d| d["severity"] == "error").count();
        let warnings = diags.iter().filter(|d| d["severity"] == "warning").count();

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

// === handle_caps (lines 4032-4085 in original) ===
pub fn handle_caps(_params: Option<Value>) -> RpcResult {
    Ok(json!({
        "server": "mcc",
        "version": env!("CARGO_PKG_VERSION"),
        "schema_version": 1,
        "features": {
            "diagnostics": {
                "byte_range": false,
                "end_line": true,
                "end_column": true,
                "suggestions": true,
                "related": true
            },
            "explain": true,
            "search": true,
            "query": true,
            "export": ["netlist", "bom", "spice", "kicad"],
            "show_drilldown": true,
            "show_global_ports": true,
            "show_files": true,
            "parse_code": true,
            "parse_directory": true,
            "overlay_dry_run": true,
            "simulation": false,
            "pcb_export": false,
            "erc": true,
            "semantic_lint": false
        },
        "rpc_methods": [
            "server.info", "server.methods",
            "parse", "check", "build.full", "extract",
            "show.all", "show.file", "show.files",
            "show.component", "show.component.list",
            "show.module", "show.module.list",
            "show.interface", "show.interface.list",
            "show.enum", "show.enum.list",
            "show.net", "show.net.list",
            "show.pins", "show.ports", "show.ports.list",
            "show.labels", "show.instances", "show.nets",
            "show.attrs", "show.funcs", "show.params",
            "show.roles", "show.values",
            "show.dump", "show.dump.all",
            "lib.list", "lib.info", "lib.load", "lib.unload",
            "lib.install", "lib.uninstall", "lib.search",
            "defs.search", "defs.query",
            "export", "explain", "def", "erc", "refs", "caps",
            "lookup", "lookup_sub", "lookup_with_sub", "lookup_all",
            "trace.set", "trace.get",
            "sem", "diagnostics",
            "project_symbols", "set_project_root", "set_system_root",
            "init", "load_project", "add_file", "remove_file"
        ]
    }))
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
