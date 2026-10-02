// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use super::*;

// === handle_lib_load (lines 314-329 in original) ===

pub fn handle_lib_load(params: Option<Value>) -> RpcResult {
    let name = parse_string_param(params, &["name", "lib"])?;
    let root = resolve_lib_root(&name)?;
    if !crate::mcb_load_lib(&name, &root) {
        return Err(JsonRpcError::custom(32107, "lib load failed"));
    }
    let info = crate::mcb_lib_info(&name);
    Ok(json!({
        "name": name,
        "loaded": true,
        "root": root.to_string_lossy(),
        "symbols": info.as_ref().map(|i| i.total_symbols).unwrap_or(0),
        "modules": info.as_ref().map(|i| i.module_count).unwrap_or(0),
        "components": info.as_ref().map(|i| i.component_count).unwrap_or(0),
    }))
}

// === handle_lib_unload (lines 331-335 in original) ===

pub fn handle_lib_unload(params: Option<Value>) -> RpcResult {
    let name = parse_string_param(params, &["name", "lib"])?;
    let ok = crate::mcb_unload_lib(&name);
    Ok(json!({"name": name, "unloaded": ok}))
}

// === handle_lib_install (lines 345-371 in original) ===

pub fn handle_lib_install(params: Option<Value>) -> RpcResult {
    let p: LibInstallParams = parse_strict(params)?;
    // The client resolves the project tier (it owns the cwd); absent → the
    // legacy global data-root behavior.
    let target_root = match &p.target_root {
        Some(root) => PathBuf::from(root),
        None => mcc_system_root(),
    };
    match crate::db::infra::libmgr::install_lib_at(&target_root, &p.name, &p.from, p.version.as_deref())
    {
        Ok((name_ver, target)) => Ok(json!({
            "installed": name_ver,
            "path": target.to_string_lossy(),
        })),
        Err(e) => {
            let msg = format!("{e:#}");
            let code = if msg.contains("already installed") {
                32101
            } else if msg.contains("reserved for mcode") || msg.contains("never vendored") {
                32102
            } else {
                32100
            };
            Err(JsonRpcError::custom(code, &msg))
        }
    }
}

// === handle_lib_uninstall (lines 380-411 in original) ===

pub fn handle_lib_uninstall(params: Option<Value>) -> RpcResult {
    let p: LibUninstallParams = parse_strict(params)?;
    let is_loaded = crate::mcb_loaded_libs().contains(&p.name);
    if is_loaded && !p.force {
        return Err(JsonRpcError::custom(
            32101,
            &format!(
                "lib uninstall: '{}' is loaded; unload first or pass force",
                p.name
            ),
        ));
    }
    if is_loaded && !crate::mcb_unload_lib(&p.name) {
        return Err(JsonRpcError::custom(
            32107,
            &format!("lib uninstall: failed to unload '{}'", p.name),
        ));
    }
    // Client-resolved project-tier copy wins; absent → legacy global scan.
    let lib_dir = match &p.target_dir {
        Some(dir) => PathBuf::from(dir),
        None => resolve_installed_lib_dir(&p.name).ok_or_else(|| {
            JsonRpcError::custom(
                32102,
                &format!("lib uninstall: '{}' is not installed", p.name),
            )
        })?,
    };
    if !lib_dir.exists() {
        return Err(JsonRpcError::custom(
            32102,
            &format!("lib uninstall: '{}' is not installed", p.name),
        ));
    }
    fs::remove_dir_all(&lib_dir).map_err(io_err)?;
    // Refresh index.json so lib.list no longer shows the deleted install.
    let _ = crate::cli::datadir::rebuild_index();
    Ok(json!({
        "uninstalled": p.name,
        "path": lib_dir.to_string_lossy(),
    }))
}

// === handle_lib_search (lines 418-453 in original) ===

pub fn handle_lib_search(params: Option<Value>) -> RpcResult {
    let p: LibSearchParams = parse_strict(params)?;
    let pat = p.pattern.to_lowercase();

    // One scanner for both tiers (datadir::scan_lib_dir): the client-resolved
    // project `libs/` first, then the global root.
    let mut results = Vec::new();
    let mut push_scan = |root: &Path, origin: &str| {
        for lib in crate::cli::datadir::scan_lib_dir(root) {
            let path = lib.path.to_string_lossy().to_string();
            if lib.name.to_lowercase().contains(&pat) || path.to_lowercase().contains(&pat) {
                results.push(json!({
                    "name": lib.name, "version": lib.version,
                    "path": path, "origin": origin,
                }));
            }
        }
    };
    if let Some(proj) = &p.project_root {
        push_scan(
            &crate::cli::datadir::project_libs_dir(Path::new(proj)),
            "project",
        );
    }
    push_scan(&mcc_system_root(), "global");

    Ok(json!({
        "pattern": p.pattern,
        "total": results.len(),
        "results": results,
    }))
}

// === handle_trace_set (lines 699-720 in original) ===
//
// Supports two modes:
//   1. Legacy:  {name: "trace.enabled", value: true}
//   2. New:     {name: "mcc::sem::fcall", level: "debug"}
//               {name: "pass1", level: "trace"}          (alias expansion)
//               {name: "mcc::sem::fcall", level: "off"}  (turn off)

pub fn handle_trace_set(params: Option<Value>) -> RpcResult {
    let p: TraceSetParams = parse_strict(params)?;

    // ── New API: per-target level override ──
    if let Some(ref level) = p.level {
        // Resolve aliases
        let raw = vec![format!("{}={}", p.name, level)];
        let targets = crate::cli::config::resolve_debug_targets(&raw);

        let base = p.base.as_deref().unwrap_or("warn");
        crate::cli::config::set_debug_targets(base, &targets);

        return Ok(
            json!({"name": p.name, "level": level, "targets": targets.iter().map(|(t,l)| format!("{}={}", t, l)).collect::<Vec<_>>()}),
        );
    }

    // ── Legacy API: boolean flags ──
    // Extract bool from value
    let val = p.value.as_ref().and_then(|v| v.as_bool()).unwrap_or(false);

    match p.name.as_str() {
        // ── C parser trace flags (token/ast/sem/visit)──
        "trace.enabled" | "enabled" => crate::cli::config::set_trace_enabled(val),
        "trace.ast" | "ast" => crate::cli::config::set_trace_ast(val),
        "trace.lexer" | "lexer" => crate::cli::config::set_trace_lexer(val),
        "trace.parser" | "parser" => crate::cli::config::set_trace_parser(val),
        "trace.visit" | "visit" => crate::cli::config::set_trace_visit(val),
        // ── Rust log pass flags (real-time effect)──
        "trace.pass1" | "pass1" => crate::cli::config::set_log_pass1(val),
        "trace.pass2" | "pass2" => crate::cli::config::set_log_pass2(val),
        "trace.server" | "server" => crate::cli::config::set_log_server(val),
        // ── New: treat as per-target level via value string ──
        _ => {
            // Try value as a level string ("debug", "off", etc.)
            if let Some(level) = p.value.as_ref().and_then(|v| v.as_str()) {
                let raw = vec![format!("{}={}", p.name, level)];
                let targets = crate::cli::config::resolve_debug_targets(&raw);
                let base = p.base.as_deref().unwrap_or("warn");
                crate::cli::config::set_debug_targets(base, &targets);
                return Ok(
                    json!({"name": p.name, "level": level, "targets": targets.iter().map(|(t,l)| format!("{}={}", t, l)).collect::<Vec<_>>()}),
                );
            }
            return Err(JsonRpcError::custom(
                -32099,
                &format!("unknown trace config: {}", p.name),
            ));
        }
    }
    Ok(json!({"name": p.name, "value": val}))
}

// === handle_trace_get (lines 722-741 in original) ===
//
// Supports two modes:
//   1. Legacy:  {name: "trace.enabled"} → returns {name, value: bool}
//   2. New:     {name: "mcc::sem::fcall"} → returns {name, level: "debug"}
//               {name: "*"} or {} → returns all current target overrides

pub fn handle_trace_get(params: Option<Value>) -> RpcResult {
    // Allow empty params → return all targets
    let name = match params
        .as_ref()
        .and_then(|v| v.get("name"))
        .and_then(|v| v.as_str())
    {
        Some(n) => n.to_string(),
        None => {
            // Return all current target overrides + legacy flags
            let targets = crate::cli::config::get_debug_targets();
            let legacy = json!({
                "trace.enabled": crate::cli::config::get_trace_enabled(),
                "trace.ast": crate::cli::config::get_trace_ast(),
                "trace.lexer": crate::cli::config::get_trace_lexer(),
                "trace.parser": crate::cli::config::get_trace_parser(),
                "trace.visit": crate::cli::config::get_trace_visit(),
                "trace.pass1": crate::cli::config::get_log_pass1(),
                "trace.pass2": crate::cli::config::get_log_pass2(),
                "trace.server": crate::cli::config::get_log_server(),
            });
            return Ok(json!({
                "legacy": legacy,
                "targets": targets,
            }));
        }
    };

    match name.as_str() {
        // ── Legacy flags ──
        "trace.enabled" | "enabled" => {
            return Ok(json!({"name": name, "value": crate::cli::config::get_trace_enabled()}));
        }
        "trace.ast" | "ast" => {
            return Ok(json!({"name": name, "value": crate::cli::config::get_trace_ast()}));
        }
        "trace.lexer" | "lexer" => {
            return Ok(json!({"name": name, "value": crate::cli::config::get_trace_lexer()}));
        }
        "trace.parser" | "parser" => {
            return Ok(json!({"name": name, "value": crate::cli::config::get_trace_parser()}));
        }
        "trace.visit" | "visit" => {
            return Ok(json!({"name": name, "value": crate::cli::config::get_trace_visit()}));
        }
        "trace.pass1" | "pass1" => {
            return Ok(json!({"name": name, "value": crate::cli::config::get_log_pass1()}));
        }
        "trace.pass2" | "pass2" => {
            return Ok(json!({"name": name, "value": crate::cli::config::get_log_pass2()}));
        }
        "trace.server" | "server" => {
            return Ok(json!({"name": name, "value": crate::cli::config::get_log_server()}));
        }
        // ── New: look up per-target level ──
        _ => {
            let targets = crate::cli::config::get_debug_targets();
            if let Some(level) = targets.get(&name) {
                return Ok(json!({"name": name, "level": level}));
            }
            // Not set — return null level
            return Ok(json!({"name": name, "level": serde_json::Value::Null}));
        }
    }
}
