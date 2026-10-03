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
            // The --from face's error family (install_lib_at) has no
            // SolveError enum behind it — the 321xx solve codes (D15) do not
            // apply here; these three legacy codes stay.
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

// === handle_lib_resolve (registry-design.md §7, P4) ===
//
// The editor face of the dependency solver: solve the project's
// `[dependencies]` against the configured registry, install what is missing
// (unless `offline`), and report per pack — resolved faces, what this run
// downloaded, the attachment list (§7⑪ the document-card face) and per-key
// diagnostics. Loading is NOT here: the library load stays `lib.load`.

/// RPC error codes for the solve faces (D15): one per `SolveError` arm —
/// enum-mapped, never string-matched.
const RPC_SOLVE_FAILED: i32 = 32120;
const RPC_REGISTRY_UNREACHABLE: i32 = 32121;
const RPC_PACK_NOT_FOUND: i32 = 32122;
const RPC_CHECKSUM_MISMATCH: i32 = 32123;
const RPC_PARTNO_UNAVAILABLE: i32 = 32124;
const RPC_LOCK_STALE: i32 = 32125;

fn solve_rpc_code(e: &crate::SolveError) -> i32 {
    use crate::SolveError;
    match e {
        SolveError::Unresolved { .. } => RPC_PACK_NOT_FOUND,
        SolveError::Conflict { .. } | SolveError::Cycle { .. } => RPC_SOLVE_FAILED,
        SolveError::PartnoUnavailable { .. } => RPC_PARTNO_UNAVAILABLE,
        SolveError::Checksum { .. } => RPC_CHECKSUM_MISMATCH,
        // Transport/parse failures — including the offline
        // "not installed and downloads are off" face.
        SolveError::Registry(_) => RPC_REGISTRY_UNREACHABLE,
    }
}

pub fn handle_lib_resolve(params: Option<Value>) -> RpcResult {
    let p: LibResolveParams = parse_strict(params)?;
    let root = PathBuf::from(&p.project_root);
    if !root.is_dir() {
        return Err(JsonRpcError::custom(
            RPC_SOLVE_FAILED,
            &format!(
                "lib.resolve: project root is not a directory: {}",
                root.display()
            ),
        ));
    }

    let decls: std::collections::BTreeMap<String, crate::SolveDecl> = match &p.deps {
        Some(map) => map
            .iter()
            .map(|(k, d)| {
                (
                    k.clone(),
                    crate::SolveDecl {
                        key: k.clone(),
                        req: crate::parse_version_req(d.version.as_deref().unwrap_or("*")),
                        partno: d.partno.clone(),
                        local: d.local,
                    },
                )
            })
            .collect(),
        None => {
            let path = crate::cli::manifest::Manifest::find_in(&root).ok_or_else(|| {
                JsonRpcError::custom(
                    RPC_SOLVE_FAILED,
                    &format!("lib.resolve: no project.toml in {}", root.display()),
                )
            })?;
            let m = crate::cli::manifest::Manifest::load(&path).map_err(|e| {
                JsonRpcError::custom(RPC_SOLVE_FAILED, &format!("lib.resolve: {e}"))
            })?;
            m.solve_decls()
        }
    };

    let url = crate::cli::config::get_registry_url(Some(&root)).ok_or_else(|| {
        JsonRpcError::custom(
            RPC_SOLVE_FAILED,
            "lib.resolve: no registry configured — set [registry] url in mcc.yaml or \
             [config.registry] url in project.toml",
        )
    })?;
    let src = crate::RegistrySource::from_url(&url)
        .map_err(|e| JsonRpcError::custom(RPC_REGISTRY_UNREACHABLE, &format!("lib.resolve: {e}")))?;

    let any_local = decls.values().any(|d| d.local);
    let deps_dir = any_local.then(|| root.join("deps"));
    let data_root = mcc_system_root();
    let stored = crate::LockFile::load(&root);

    let solved = if p.offline {
        crate::solve(
            &decls,
            stored.as_ref(),
            &crate::DiskSource {
                src: &src,
                deps_dir: deps_dir.clone(),
                data_root: data_root.clone(),
                no_install: true,
            },
        )
        .map(|o| (o, Vec::new()))
    } else {
        crate::solve_and_install(
            &src,
            deps_dir.as_deref(),
            &data_root,
            &decls,
            stored.as_ref(),
        )
    };

    match solved {
        Ok((out, installed)) => {
            // Lock write authority = build's: write once when fresh, never
            // rewrite; a stale lock is a diagnostic, not an error.
            let lock_written = if out.fresh {
                crate::lock_with_mcode_rev(&out.lock)
                    .store(&root)
                    .is_ok()
            } else {
                false
            };
            let mut diagnostics = Vec::new();
            if !out.fresh {
                if let Some(st) = &stored {
                    if let Some(key) = crate::first_stale_key(st, &out.lock) {
                        diagnostics.push(json!({
                            "code": RPC_LOCK_STALE,
                            "key": key,
                            "message": format!(
                                "mcode.lock does not cover `{key}` as solved — run `mcc lib update`"
                            ),
                        }));
                    }
                }
            }
            Ok(json!({
                "resolved": out.packs.values().map(|pk| {
                    let dir = match (pk.local, &deps_dir) {
                        (true, Some(d)) => d.join(format!("{}@{}", pk.package, pk.version)),
                        _ => data_root.join(format!("{}@{}", pk.package, pk.version)),
                    };
                    json!({
                        "key": pk.key,
                        "package": pk.package,
                        "version": pk.version,
                        "partno": pk.partno,
                        "checksum": pk.checksum,
                        "origin": match pk.origin {
                            crate::SolveOrigin::Project => "project",
                            crate::SolveOrigin::DataRoot => "data-root",
                            crate::SolveOrigin::Registry => "registry",
                        },
                        "attachments": pack_attachments(&dir),
                    })
                }).collect::<Vec<_>>(),
                "installed": installed,
                "lock_present": stored.is_some(),
                "lock_written": lock_written,
                "diagnostics": diagnostics,
            }))
        }
        Err(primary) => {
            // The whole-solve failed; re-run per declaration (selection only,
            // no installs) so the editor gets one diagnostic row per failing
            // key plus whatever did resolve. Every key failing = a hard
            // error carrying the primary's code.
            let disk = crate::DiskSource {
                src: &src,
                deps_dir: deps_dir.clone(),
                data_root: data_root.clone(),
                no_install: true,
            };
            let mut resolved = Vec::new();
            let mut diagnostics = Vec::new();
            for (key, decl) in &decls {
                let one = std::iter::once((key.clone(), decl.clone())).collect();
                match crate::solve(&one, stored.as_ref(), &disk) {
                    Ok(out) => {
                        for pk in out.packs.values() {
                            let dir = match (pk.local, &deps_dir) {
                                (true, Some(d)) => {
                                    d.join(format!("{}@{}", pk.package, pk.version))
                                }
                                _ => data_root.join(format!("{}@{}", pk.package, pk.version)),
                            };
                            resolved.push(json!({
                                "key": pk.key,
                                "package": pk.package,
                                "version": pk.version,
                                "partno": pk.partno,
                                "checksum": pk.checksum,
                                "origin": match pk.origin {
                                    crate::SolveOrigin::Project => "project",
                                    crate::SolveOrigin::DataRoot => "data-root",
                                    crate::SolveOrigin::Registry => "registry",
                                },
                                "attachments": pack_attachments(&dir),
                            }));
                        }
                    }
                    Err(e) => diagnostics.push(json!({
                        "code": solve_rpc_code(&e),
                        "key": key,
                        "message": e.to_string(),
                    })),
                }
            }
            if resolved.is_empty() {
                return Err(JsonRpcError::custom(
                    solve_rpc_code(&primary),
                    &format!("lib.resolve: {primary}"),
                ));
            }
            Ok(json!({
                "resolved": resolved,
                "installed": [],
                "lock_present": stored.is_some(),
                "lock_written": false,
                "diagnostics": diagnostics,
            }))
        }
    }
}

/// The §7⑪ attachment face of an installed pack: the pack.toml
/// `[[attachments]]` table as the document card reads it — bundled rows
/// (in-pack paths the thin tier may omit; `lib fetch-docs` fills them) and
/// linked rows (URL pointers).
fn pack_attachments(dir: &Path) -> Value {
    let pack = match crate::cli::packfile::load(dir) {
        Ok(p) => p,
        Err(_) => return json!([]),
    };
    json!(pack
        .attachments
        .iter()
        .map(|a| {
            json!({
                "form": if a.is_bundled() { "bundled" } else { "linked" },
                "kind": a.kind,
                "path": a.path,
                "name": a.name,
                "url": a.url,
                "rev": a.rev,
                "license": a.license,
            })
        })
        .collect::<Vec<_>>())
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
                // One-line device description, quoted from the pack face.
                let description = std::fs::read_to_string(lib.path.join("pack.toml"))
                    .ok()
                    .and_then(|t| toml::from_str::<crate::cli::packfile::PackToml>(&t).ok())
                    .and_then(|p| p.package.description)
                    .filter(|d| !d.is_empty());
                results.push(json!({
                    "name": lib.name, "version": lib.version,
                    "path": path, "origin": origin,
                    "description": description,
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
