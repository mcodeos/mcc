// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use super::*;

// === rules.list / rule.detail / severity.set / allow.add / accept (§8) ===
//
// Read handlers render the shared projection built in `crate::override_store`
// (the same bytes `mcc rules list/detail` and the MCP tools emit). Write
// handlers go through the same store API the CLI uses and land in the session
// layer — RPC never persists (only the CLI `--write` merges into the project
// config, design §8-5 persistence discipline).

fn rule_code_param(params: Option<Value>) -> Result<u32, JsonRpcError> {
    match params {
        Some(Value::Number(n)) => n
            .as_u64()
            .and_then(|u| u32::try_from(u).ok())
            .ok_or_else(JsonRpcError::invalid_params),
        Some(Value::String(s)) => {
            crate::override_store::parse_rule_code(&s).ok_or_else(JsonRpcError::invalid_params)
        }
        Some(Value::Object(mut m)) => {
            for k in ["code", "key"] {
                match m.remove(k) {
                    Some(Value::Number(n)) => {
                        return n
                            .as_u64()
                            .and_then(|u| u32::try_from(u).ok())
                            .ok_or_else(JsonRpcError::invalid_params)
                    }
                    Some(Value::String(s)) => {
                        return crate::override_store::parse_rule_code(&s)
                            .ok_or_else(JsonRpcError::invalid_params)
                    }
                    _ => {}
                }
            }
            Err(JsonRpcError::invalid_params())
        }
        _ => Err(JsonRpcError::invalid_params()),
    }
}

/// Rules.list — enumerate the numeric-code catalog filtered by the §2.3/§2.5
/// axes (`scope`/`domain`/`severity`/`plane`/`gate`/`overridable`/`fix`).
pub fn handle_rules_list(params: Option<Value>) -> RpcResult {
    let filter = crate::override_store::filter_from_value(params.as_ref())
        .map_err(|e| JsonRpcError::custom(-32602, &e))?;
    Ok(crate::override_store::rules_list_json(&filter))
}

/// Rule.detail — one rule's descriptor plus its §8-5 override audit.
pub fn handle_rule_detail(params: Option<Value>) -> RpcResult {
    let code = rule_code_param(params)?;
    crate::override_store::rule_detail_json(code).map_err(|e| JsonRpcError::custom(32112, &e))
}

/// Severity.set — session-layer severity override (§8-5 write entry). Never
/// persists; only the CLI `--write` merges into the project config.
pub fn handle_severity_set(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize, Default)]
    struct Params {
        code: Option<String>,
        key: Option<String>,
        severity: Option<String>,
    }
    let p: Params = parse_or_default(params)?;
    let raw = p.code.as_deref().or(p.key.as_deref()).unwrap_or("");
    let code = crate::override_store::parse_rule_code(raw)
        .ok_or_else(|| JsonRpcError::custom(-32602, "severity.set: need a numeric rule code"))?;
    let sev = p
        .severity
        .as_deref()
        .and_then(|s| crate::check::CheckSeverity::from_str(s.trim()))
        .ok_or_else(|| {
            JsonRpcError::custom(
                -32602,
                "severity.set: unknown severity (hint|info|warning|error)",
            )
        })?;
    crate::override_store::session_set_severity(code, sev)
        .map_err(|e| JsonRpcError::custom(32113, &e))?;
    Ok(json!({
        "key": crate::override_store::rule_key(code),
        "severity": sev.as_str(),
        "layer": "session",
        "persisted": false,
    }))
}

/// Allow.add — session-layer suppression row (§8-5). `path` omitted or empty
/// is the project global; accepts a file, directory prefix or glob.
pub fn handle_allow_add(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize, Default)]
    struct Params {
        code: Option<String>,
        key: Option<String>,
        path: Option<String>,
        reason: Option<String>,
    }
    let p: Params = parse_or_default(params)?;
    let raw = p.code.as_deref().or(p.key.as_deref()).unwrap_or("");
    let code = crate::override_store::parse_rule_code(raw)
        .ok_or_else(|| JsonRpcError::custom(-32602, "allow.add: need a numeric rule code"))?;
    let scope = crate::override_store::parse_path_scope(p.path.as_deref().unwrap_or(""));
    crate::override_store::session_add_allow(code, scope.clone(), p.reason.clone())
        .map_err(|e| JsonRpcError::custom(32113, &e))?;
    Ok(json!({
        "key": crate::override_store::rule_key(code),
        "path": crate::override_store::path_display(&scope),
        "layer": "session",
        "persisted": false,
    }))
}

/// Accept — session-layer waiver row (§8-5). `path` omitted or empty is the
/// project global; `since` records when the waiver started.
pub fn handle_accept(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize, Default)]
    struct Params {
        code: Option<String>,
        key: Option<String>,
        path: Option<String>,
        since: Option<String>,
    }
    let p: Params = parse_or_default(params)?;
    let raw = p.code.as_deref().or(p.key.as_deref()).unwrap_or("");
    let code = crate::override_store::parse_rule_code(raw)
        .ok_or_else(|| JsonRpcError::custom(-32602, "accept: need a numeric rule code"))?;
    let scope = crate::override_store::parse_path_scope(p.path.as_deref().unwrap_or(""));
    crate::override_store::session_add_accept(code, scope.clone(), p.since.clone())
        .map_err(|e| JsonRpcError::custom(32113, &e))?;
    Ok(json!({
        "key": crate::override_store::rule_key(code),
        "path": crate::override_store::path_display(&scope),
        "layer": "session",
        "persisted": false,
    }))
}
