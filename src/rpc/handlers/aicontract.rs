// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use super::*;

// === handle_check (lines 1462-1528 in original) ===

pub fn handle_check(params: Option<Value>) -> RpcResult {
    let p: CheckRpcParams = parse_or_default(params)?;
    load_libs_rpc(&p.libs);

    // ── Mode A: inline content (AI dry-run) ──
    if let Some(content) = &p.content {
        // Phase 8.1: one shared overlay slot for every dry-run; the dispatch
        // lock in protocol.rs serializes handlers, so no per-request URI.
        let uri = super::make_overlay_uri();
        crate::mcc_load_from_string(&uri, content);

        // Phase 8.2: unified diagnostic serialization via lsp/diagnostics,
        // scoped to the overlay candidate file only. `collect_all_full` would
        // also surface every other file in the workspace — during an agent
        // edit the on-disk project is frequently mid-edit (broken), and that
        // noise must not fail a dry-run of the candidate content.
        let diags: Vec<Value> = crate::mcc_diagnose(&uri)
            .iter()
            .map(crate::lsp::diagnostics::diagnostic_to_json_full)
            .collect();
        // Compiler fault tolerance: diagnostics located in system-library
        // files are external library noise, not the user's circuit. A broken
        // third-party library must not fail the user's check — count them
        // separately, keep the summary over the user's own files only, and
        // drop the library entries from the returned list.
        let (user, lib): (Vec<Value>, Vec<Value>) = diags
            .into_iter()
            .partition(|d| !super::diag_in_system_lib(d));
        let errors = user.iter().filter(|d| d["severity"] == "error").count();
        let warnings = user.iter().filter(|d| d["severity"] == "warning").count();
        let lib_errors = lib.iter().filter(|d| d["severity"] == "error").count();
        let lib_warnings = lib.iter().filter(|d| d["severity"] == "warning").count();

        // Phase 8.1: the slot is shared, so it must be released before the
        // next request sees the candidate as a workspace file.
        super::remove_overlay(&uri);

        return Ok(json!({
            "summary": { "errors": errors, "warnings": warnings },
            "diagnostics": user,
            "library": { "errors": lib_errors, "warnings": lib_warnings },
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

    let bp = json!({
        "entry": p.entry,
        "include_system": false,
        "ledger": p.ledger,
    });
    handle_build_full(Some(bp))
}

// === handle_caps (Phase 8.3: auto-generated from METHOD registry) ===
//
// Without params this stays the plain capability sheet (the legacy face).
// A `client` triple turns it into the handshake of
// live-world-residency-design.md §4.3: the verdict rides alongside the sheet,
// mismatches carry a restart hint. Same version but a different build refuses
// (`stale_build`) — world_ver folds BUILD into the digest, so revision tokens
// from different builds are systematically incomparable and an if_version
// cache must never straddle them.
pub fn handle_caps(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize, Default)]
    struct ClientTriple {
        protocol: Option<String>,
        mcc_version: Option<String>,
        build: Option<u64>,
    }

    #[derive(Deserialize, Default)]
    struct CapsParams {
        client: Option<ClientTriple>,
    }

    let p: CapsParams = parse_or_default(params)?;
    let mut caps = super::caps_json();
    let Some(client) = p.client else {
        return Ok(caps);
    };

    // A `client` object with no field set identifies nothing — the same
    // unverified ground as the legacy no-params call.
    let verified = client.protocol.is_some()
        || client.mcc_version.is_some()
        || client.build.is_some();
    if !verified {
        let handshake = json!({
            "verdict": "unverified",
            "detail": "client sent no identifying fields",
        });
        if let Some(obj) = caps.as_object_mut() {
            obj.insert("handshake".into(), handshake);
        }
        return Ok(caps);
    }

    let server_build = crate::buildinfo::number();
    let (verdict, detail) =
        if client.protocol.as_deref() != Some(crate::buildinfo::RPC_PROTOCOL) {
            (
                "protocol_mismatch",
                format!(
                    "client protocol {:?} != server {:?}",
                    client.protocol.as_deref().unwrap_or("<none>"),
                    crate::buildinfo::RPC_PROTOCOL
                ),
            )
        } else if client.mcc_version.as_deref() != Some(crate::buildinfo::VERSION) {
            (
                "version_mismatch",
                format!(
                    "client mcc {} != server {}",
                    client.mcc_version.as_deref().unwrap_or("<none>"),
                    crate::buildinfo::VERSION
                ),
            )
        } else if client.build != Some(server_build) {
            (
                "stale_build",
                format!(
                    "client build {} != server {}; revision tokens are build-scoped, restart the daemon",
                    client.build.map(|b| b.to_string()).unwrap_or_else(|| "<none>".into()),
                    server_build
                ),
            )
        } else {
            (
                "ok",
                "client triple matches this daemon".to_string(),
            )
        };

    let handshake = if verdict == "ok" {
        json!({
            "verdict": verdict,
            "detail": detail,
        })
    } else {
        json!({
            "verdict": verdict,
            "detail": detail,
            "restart_hint": "mcc restart",
        })
    };
    if let Some(obj) = caps.as_object_mut() {
        obj.insert("handshake".into(), handshake);
    }
    Ok(caps)
}

// === handle_explain (lines 4201-4236 in original) ===
pub fn handle_explain(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize, Default)]
    struct ExplainParams {
        code: Option<u32>,
    }

    let p: ExplainParams = parse_or_default(params)?;

    match p.code {
        Some(code) => match crate::errcodes::describe(code) {
            Some(info) => {
                // Deepen to the full rule descriptor when the code is a
                // registered catalog rule (rule-registry design §8): the
                // explain view carries owner/lock/plane/acceptance/fix and
                // the allow syntax of the §8-5 write face.
                let mut base = json!({
                    "code": info.code,
                    "name": info.name,
                    "description": info.description,
                });
                if let Some(meta) = crate::rules::find_rule(code) {
                    if let Some(obj) = base.as_object_mut() {
                        obj.insert(
                            "rule".into(),
                            crate::override_store::rule_descriptor_json(meta),
                        );
                        obj.insert(
                            "allow_syntax".into(),
                            serde_json::json!({
                                "severity.set": format!(
                                    "mcc rules set-severity {} <hint|info|warning|error> [--write]",
                                    crate::override_store::rule_key(code)
                                ),
                                "allow.add": format!(
                                    "mcc rules allow {} --path 'boards/**/*.mc' --reason '...' [--write]",
                                    crate::override_store::rule_key(code)
                                ),
                                "accept": format!(
                                    "mcc rules accept {} --path boards/dev/main.mc --since 2026-09-05 [--write]",
                                    crate::override_store::rule_key(code)
                                ),
                            }),
                        );
                    }
                }
                Ok(base)
            }
            None => Err(JsonRpcError::custom(
                32112,
                &format!("unknown error code: {code}"),
            )),
        },
        None => {
            let all = crate::errcodes::all_codes();
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
