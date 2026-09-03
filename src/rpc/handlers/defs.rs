// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use super::*;

// === handle_defs_search (lines 474-527 in original) ===

pub fn handle_defs_search(params: Option<Value>) -> RpcResult {
    let p: DefsSearchParams = parse_or_default(params)?;
    let kind = match p.kind.as_deref() {
        None => None,
        Some("component") => Some(SearchKind::Component),
        Some("module") => Some(SearchKind::Module),
        Some("interface") => Some(SearchKind::Interface),
        Some("enum") => Some(SearchKind::Enum),
        Some("instance") => Some(SearchKind::Instance),
        Some(other) => {
            return Err(JsonRpcError::custom(
                -32602,
                &format!(
                    "defs.search: unknown kind '{}', expected one of component|module|interface|enum|instance",
                    other
                ),
            ));
        }
    };
    let inputs = SearchInputs {
        pattern: p.pattern,
        kind,
        regex: p.regex,
        fuzzy: p.fuzzy,
        top: p.top,
        limit: p.limit,
        libs: Vec::new(),
    };
    let hits = walk_defs(&inputs, None)
        .map_err(|e| JsonRpcError::custom(-32603, &format!("defs.search: {}", e)))?;
    let count = hits.len();
    let results: Vec<Value> = hits
        .into_iter()
        .map(|h| {
            let mut v = json!({
                "kind": h.kind,
                "name": h.name,
                "uri": h.uri,
            });
            if let Some(c) = h.class {
                v["class"] = json!(c);
            }
            v
        })
        .collect();
    Ok(json!({
        "pattern": inputs.pattern,
        "kind": inputs.kind.map(|k| format!("{:?}", k).to_lowercase()),
        "regex": inputs.regex,
        "fuzzy": inputs.fuzzy,
        "count": count,
        "results": results,
    }))
}

// === handle_defs_query (lines 540-571 in original) ===

pub fn handle_defs_query(params: Option<Value>) -> RpcResult {
    let p: DefsQueryParams = parse_or_default(params)?;
    let query = crate::query_api::compile(&p.expr)
        .map_err(|e| JsonRpcError::custom(-32602, &format!("defs.query: {}", e)))?;
    let inputs = SearchInputs {
        pattern: String::new(),
        kind: None,
        regex: false,
        fuzzy: false,
        top: None,
        limit: p.limit,
        libs: Vec::new(),
    };
    let hits = walk_defs(&inputs, Some(&query))
        .map_err(|e| JsonRpcError::custom(-32603, &format!("defs.query: {}", e)))?;
    let count = hits.len();
    let results: Vec<Value> = hits
        .into_iter()
        .map(|h| {
            let mut v = json!({"kind": h.kind, "name": h.name, "uri": h.uri});
            if let Some(c) = h.class {
                v["class"] = json!(c);
            }
            v
        })
        .collect();
    Ok(json!({
        "expr": p.expr,
        "count": count,
        "results": results,
    }))
}

// === handle_refs (lines 1534-1556 in original) ===

pub fn handle_refs(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize)]
    struct RefsParams {
        name: String,
    }

    let p: RefsParams = parse_strict(params)?;
    let items = crate::lsp::references::find(&p.name);
    Ok(json!({ "name": p.name, "count": items.len(), "refs": items }))
}

// === handle_erc (lines 1562-1564 in original) ===

pub fn handle_erc(_params: Option<Value>) -> RpcResult {
    run_erc()
}

// === handle_def (lines 3978-4025 in original) ===
pub fn handle_def(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize)]
    struct DefParams {
        name: String,
        /// Optional cursor byte offset (with `uri`). When present, goto-def
        /// resolves via the strict position-aware lapper + RefDefMap exact
        /// path so same-name defs (e.g. `enum CAP` vs `component CAP`) stay
        /// distinct. A miss is reported as not-found — never a name-based
        /// fallback (which would misattribute the def).
        #[serde(default)]
        uri: Option<String>,
        #[serde(default)]
        position: Option<usize>,
    }

    let p: DefParams = parse_strict(params)?;
    if let (Some(uri), Some(position)) = (&p.uri, p.position) {
        return match crate::lsp::gotodef::resolve_at_pos(uri, position) {
            Some(result) => Ok(result),
            None => Err(JsonRpcError::custom(
                32112,
                &format!("definition not found: {}", p.name),
            )),
        };
    }
    // Legacy name-based path (callers without a cursor position). When the
    // request carries the cursor file (`uri`), resolution is restricted to
    // that file's visibility set V(F) (§5.4) — never a workspace-wide scan.
    let result = match &p.uri {
        Some(uri) => crate::lsp::gotodef::resolve_in_file(&p.name, uri),
        None => crate::lsp::gotodef::resolve(&p.name),
    };
    match result {
        Some(result) => Ok(result),
        None => Err(JsonRpcError::custom(
            32112,
            &format!("definition not found: {}", p.name),
        )),
    }
}

// === handle_lookup (lines 4091-4101 in original) ===

pub fn handle_lookup(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize)]
    struct LookupParams {
        name: String,
    }
    let p: LookupParams = parse_strict(params)?;
    match crate::unified_lookup(&p.name, &McURI::new()) {
        Some((uri, span)) => Ok(json!({"uri": uri, "span": [span.start, span.end]})),
        None => Ok(json!({"uri": null, "span": null})),
    }
}

// === handle_lookup_sub (lines 4104-4128 in original) ===
pub fn handle_lookup_sub(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize)]
    struct LookupSubParams {
        #[serde(rename = "parentUri")]
        parent_uri: String,
        #[serde(rename = "containerName")]
        container_name: Option<String>,
        kind: String,
        name: String,
    }
    let p: LookupSubParams = parse_strict(params)?;
    let parent_uri = McURI::from(p.parent_uri.as_str());
    let kind = match crate::SubElementKind::from_str(&p.kind) {
        Some(k) => k,
        None => {
            let msg = format!("Unknown kind: {}", p.kind);
            return Err(JsonRpcError::custom(32104, &msg));
        }
    };
    match crate::lookup_sub_def(&parent_uri, p.container_name.as_deref(), kind, &p.name) {
        Some(span) => Ok(json!({"uri": parent_uri, "span": [span.start, span.end]})),
        None => Ok(json!({"uri": null, "span": null})),
    }
}

// === handle_lookup_with_sub (lines 4132-4154 in original) ===
pub fn handle_lookup_with_sub(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize)]
    struct LwsParams {
        #[serde(rename = "className")]
        class_name: String,
        #[serde(rename = "subName")]
        sub_name: Option<String>,
        #[serde(rename = "subKind")]
        sub_kind: Option<String>,
        #[serde(rename = "fromUri")]
        from_uri: Option<String>,
    }
    let p: LwsParams = parse_strict(params)?;
    let from = p.from_uri.as_deref().map(McURI::from).unwrap_or_default();
    let sub_kind = p
        .sub_kind
        .as_deref()
        .and_then(crate::SubElementKind::from_str);
    match crate::lookup_with_sub(&p.class_name, p.sub_name.as_deref(), sub_kind, &from) {
        Some((uri, span)) => Ok(json!({"uri": uri, "span": [span.start, span.end]})),
        None => Ok(json!({"uri": null, "span": null})),
    }
}

// === handle_lookup_all (lines 4157-4194 in original) ===
pub fn handle_lookup_all(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize, Default)]
    struct LookupAllParams {
        uri: Option<String>,
        scope: Option<String>,
        prefix: Option<String>,
        #[serde(default)]
        limit: usize,
    }
    let p: LookupAllParams = parse_or_default(params)?;
    let uri = p.uri.map(|s| McURI::from(s.as_str())).unwrap_or_default();
    let scope_path = if let Some(ref s) = p.scope {
        crate::db::infra::mc_code::McCode::scope_path_from_scope_str_public(&uri, s)
    } else {
        crate::ScopePath::file_level(&uri)
    };
    let mut filter = crate::ScopeFilter::new();
    if let Some(pref) = &p.prefix {
        filter = filter.with_prefix(pref);
    }
    let limit = if p.limit > 0 { p.limit } else { 100 };
    filter = filter.with_limit(limit);

    let results = crate::unified_lookup_all(&scope_path, &filter);
    let items: Vec<serde_json::Value> = results
        .iter()
        .map(|r| {
            json!({
                "name": r.name,
                "uri": r.uri,
                "span": [r.span.start, r.span.end],
                "kind": r.kind.as_str(),
                "scope": r.scope,
            })
        })
        .collect();
    Ok(json!({ "items": items }))
}

// === defs.checkpoint / defs.diff (design §10; T6-② daemon/RPC exit) ===

/// Capture the current definition-space version — the daemon/RPC baseline a
/// consumer diffs against with `defs.diff`. The capture is unconditional
/// (always a fresh version handle), and the payload is the serialized
/// registry identity set: DefId + canonical key + domain + liveness +
/// content fingerprint per entry ([`RegistryEntrySnapshot`]), never the def
/// payloads. Re-baseline with this method when `defs.diff` reports that the
/// journal window expired.
pub fn handle_defs_checkpoint(_params: Option<Value>) -> RpcResult {
    let cp = crate::db::defregistry::checkpoint();
    Ok(serde_json::to_value(cp).expect("checkpoint serialization cannot fail"))
}

/// Diff the definition space since a previously captured version
/// (`defs.checkpoint`) against the live state. Every changed def appears as
/// `{id, kind: added|removed|modified, before, after}` where `before`/`after`
/// are registry-entry snapshots (null when the side does not exist); `files`
/// lists the touched uris. The query does not stamp a new version. When the
/// requested baseline fell out of the sliding journal window the call fails
/// with 32113 — re-baseline with `defs.checkpoint`.
pub fn handle_defs_diff(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize)]
    struct DefsDiffParams {
        #[serde(rename = "fromVersion")]
        from_version: u64,
    }
    let p: DefsDiffParams = parse_strict(params)?;
    let changes = crate::db::defregistry::diff_since(p.from_version).map_err(|e| {
        JsonRpcError::custom(
            32113,
            &format!("defs.diff: {e}; re-baseline with defs.checkpoint"),
        )
    })?;
    let files = crate::db::defregistry::changed_files(&changes);
    let items: Vec<Value> = changes
        .into_iter()
        .map(|c| {
            let kind = match c.kind {
                crate::db::defregistry::DefChangeKind::Added => "added",
                crate::db::defregistry::DefChangeKind::Removed => "removed",
                crate::db::defregistry::DefChangeKind::Modified => "modified",
            };
            json!({
                "id": c.id,
                "kind": kind,
                "before": c.before,
                "after": c.after,
            })
        })
        .collect();
    Ok(json!({
        "fromVersion": p.from_version,
        "count": items.len(),
        "files": files,
        "changes": items,
    }))
}

// === defs.dependents (design D14; T5 who-uses over the DefRefGraph) ===

/// Def-scoped who-uses: which ref-points `(referenced-name, referencing-file)`
/// resolved to the named definition. The answer comes from the per-world
/// DefRefGraph rev side in one hop (`def_id_of` + `dependents_of`) — consumers
/// hold the def id, never a text-keyed registry round trip (D15.3). Position
/// granularity stays in the symbol layer (RefDefMap); this is the def-level
/// dependents question ("what references this def"), the invalidation-domain
/// primitive. An unknown / tombstoned def answers an empty list, never an
/// error.
pub fn handle_defs_dependents(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize)]
    struct DefsDependentsParams {
        name: String,
        uri: String,
    }
    let p: DefsDependentsParams = parse_strict(params)?;
    let graph = &crate::db::cmie::tables::WORKSPACE.refgraph;
    let sn = crate::McSpaceName::new(
        &crate::McIds::from(p.name.as_str()),
        crate::McURI::from(p.uri.as_str()),
    );
    let def_id = graph.def_id_of(&sn);
    let dependents: Vec<Value> = match def_id {
        Some(id) => graph
            .dependents_of(id)
            .into_iter()
            .map(|r| {
                json!({
                    "name": r.ident.to_string(),
                    "uri": r.uri_string().to_string(),
                })
            })
            .collect(),
        None => Vec::new(),
    };
    let has_dependents = def_id.is_some_and(|id| graph.has_dependents_of(id));
    Ok(json!({
        "name": p.name,
        "uri": p.uri,
        "defId": def_id,
        "hasDependents": has_dependents,
        "count": dependents.len(),
        "dependents": dependents,
    }))
}
