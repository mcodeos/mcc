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
    }

    let p: DefParams = parse_strict(params)?;
    match crate::lsp::goto_def::resolve(&p.name) {
        Some(result) => Ok(result),
        None => Err(JsonRpcError::custom(
            -32003,
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
            return Ok(
                json!({"uri": null, "span": null, "error": format!("Unknown kind: {}", p.kind)}),
            )
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
