// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::db::cmie::tables as workspace;
use crate::db::infra::libmgr;
use crate::db::infra::mc_code::McCode;
use crate::McURI;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use tracing::{debug, trace};

use crate::db::infra::init::*;

// === pub fn mcb_parse_all_modules() { ===
/// Phase 1b: all component/interface/enum are registered, now parse all modules
///
/// To avoid Mutex deadlock (parse_pass1_modules -> mcb_get_cmie -> prj_mcodes.borrow),
/// we extract files from the map, parse outside the lock, then re-insert.
///
/// ★ Fix: Parse modules in dependency order (topological sort based on uselist).
/// Without this, DashMap iteration is unordered, so main.mc modules could be parsed
/// before power.mc modules are registered, causing "definition not found" errors.
pub fn mcb_parse_all_modules() {
    // ★ Clear stale diagnostics before this round re-emits them.
    //
    // Every call rebuilds the symbol lapper for ALL workspace files (the
    // parse_pass1_modules_full() loop below) and re-runs every PostParse
    // validator (including ImportsCheck, which re-emits USE_* 2xxx) over ALL
    // files. Neither step clears what a previous round emitted, so diagnostics
    // accumulate across load_project/sem calls — observed as E5508 per file
    // doubling 5 -> 10 -> 15 — and stale resolution errors (E3157/E3071)
    // emitted during a round where the mcode library was not yet loaded
    // survive forever.
    //
    // Invariant: a workspace file has `modules_parsed == false` iff it was
    // freshly parsed in THIS round (parse_ast/parse_ast_from_string clear the
    // file's diagnostics, and file-level parsing never runs the module parse).
    // Such files carry fresh parser and use-stage (parse_nsp) diagnostics that
    // are NOT re-emitted by the topo loop — keep them. Files from earlier
    // rounds (modules_parsed == true) are fully re-derived below, so their old
    // entries are pure accumulation and must go.
    let stale_uris: Vec<McURI> = workspace::WORKSPACE
        .mcodes
        .iter()
        .filter(|e| e.value().modules_parsed)
        .map(|e| McURI::from(e.key().as_str()))
        .collect();
    for uri in stale_uris {
        workspace::WORKSPACE
            .diagnostics
            .lock()
            .unwrap()
            .clear_file(&uri);
    }

    // 1. Collect all URIs and their dependencies
    let mut uri_deps: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for entry in workspace::WORKSPACE.mcodes.iter() {
        let uri = entry.key().clone();
        // ★ Fix: Canonicalize dependency URIs so they match the map keys.
        // Without this, raw URIs like "./power.mc" won't match canonicalized
        // keys like "/abs/path/power.mc", causing topo sort to treat all files
        // as having zero deps → random parse order → "definition not found" errors.
        let deps: Vec<String> = entry
            .value()
            .uselist
            .iter()
            .map(|u| canonicalize_project_uri(&u.uri))
            .collect();
        uri_deps.insert(uri, deps);
    }

    // 2. Topological sort: dependencies first
    let mut sorted_uris = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let mut in_stack = std::collections::HashSet::new();
    let all_uris: Vec<String> = uri_deps.keys().cloned().collect();

    fn topo_visit(
        uri: &str,
        uri_deps: &std::collections::BTreeMap<String, Vec<String>>,
        visited: &mut std::collections::HashSet<String>,
        in_stack: &mut std::collections::HashSet<String>,
        sorted: &mut Vec<String>,
    ) {
        if visited.contains(uri) {
            return;
        }
        if in_stack.contains(uri) {
            // Circular dependency detected — log warning but continue
            tracing::warn!(
                target: "mcc::pass1",
                uri = %uri,
                "circular dependency detected in use graph; dependency order may be incorrect"
            );
            return;
        }
        in_stack.insert(uri.to_string());
        visited.insert(uri.to_string());
        if let Some(deps) = uri_deps.get(uri) {
            for dep in deps {
                topo_visit(dep, uri_deps, visited, in_stack, sorted);
            }
        }
        in_stack.remove(uri);
        sorted.push(uri.to_string());
    }

    for uri in all_uris.iter() {
        topo_visit(
            uri,
            &uri_deps,
            &mut visited,
            &mut in_stack,
            &mut sorted_uris,
        );
    }

    // 3. Parse modules in dependency order
    // Use remove+insert instead of clone+insert to avoid AstNode ownership issues.
    // Clone creates a shallow AstNode copy (owned=false) that dangles when the
    // original (owned=true) is dropped during insert replacement.
    for uri in sorted_uris {
        let mcfile_opt = workspace::WORKSPACE.mcodes.remove(&uri).map(|(_k, v)| v);

        if let Some(mut mcfile) = mcfile_opt {
            crate::current_uri::set(&uri);
            // ★ The file was removed from `mcodes` during parsing, so diagnostic
            //   emission (e.g., E2008) cannot look up its `LineIndex` there.
            //   Push the line index onto the thread-local stack as a fallback.
            let _guard = mcfile.line_index.as_ref().map(|idx| {
                crate::db::infra::context::LineIndexGuard::new(
                    crate::McURI::from(uri.as_str()),
                    idx.clone(),
                )
            });
            // Always fully re-derive: module parse (re-emits module
            // diagnostics such as E5642 that the stale sweep above may have
            // wiped) plus lapper rebuild. parse_pass1_modules_full is
            // idempotent across rounds — module registration replaces this
            // file's prior entry instead of firing a spurious DUP_MODULE — so
            // every round yields the same single set of diagnostics per file.
            mcfile.parse_pass1_modules_full();
            // _guard drops here, automatically pops line_index
            workspace::WORKSPACE.mcodes.insert(uri, mcfile);
        } else {
            // File was in uri_deps but not in workspace — log as dlog
            tracing::warn!(
                target: "mcc::pass1",
                uri = %uri,
                "mcb_parse_all_modules: file in dependency graph but not found in workspace"
            );
        }
    }

    // ★ Validation: run PostParse checks after all modules parsed.
    {
        use crate::db::diagnostic::diagnostic::{diagnostic_log, DiagnosticLevel};
        use crate::semantic::validation::CheckRegistry;
        let registry = CheckRegistry::with_defaults();
        let saved_uri = crate::current_uri::try_get();
        for r in registry.run_post_parse() {
            // Switch current_uri to the file this diagnostic belongs to
            if let Some(ref uri) = r.uri {
                crate::current_uri::set(&McURI::from(uri.as_str()));
            }
            let level = match r.severity {
                crate::semantic::validation::CheckSeverity::Error => DiagnosticLevel::Error,
                crate::semantic::validation::CheckSeverity::Warning => DiagnosticLevel::Warning,
                crate::semantic::validation::CheckSeverity::Info => DiagnosticLevel::Info,
                crate::semantic::validation::CheckSeverity::Hint => DiagnosticLevel::Hint,
            };
            let (pos, len) = r
                .span
                .as_ref()
                .map(|s| (s.start as u32, (s.end - s.start) as u32))
                .unwrap_or((0, 0));
            diagnostic_log(r.code, level, pos, len, &r.message, &[]);
        }
        // Restore previous current_uri (or reset)
        match saved_uri {
            Some(ref uri) => crate::current_uri::set(uri),
            None => crate::current_uri::reset(),
        }
    }
}

// === fn topo_visit( ===

// === pub(crate) fn canonicalize_project_uri(uri: &McURI) -> String { ===
/// Normalize project file URI
///
/// Handle relative and absolute paths, return canonical path in unified format
pub(crate) fn canonicalize_project_uri(uri: &McURI) -> String {
    let path = Path::new(uri);

    // If absolute path, try to normalize
    if path.is_absolute() {
        return path
            .canonicalize()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| uri.clone());
    }

    // Relative path, join project root and normalize
    let full_path = mcb_get_project_root().join(path);
    full_path
        .canonicalize()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| uri.clone())
}

// === fn scan_mc_files(dir: &Path) -> Vec<PathBuf> { ===
/// Recursively scan all .mc files in the directory
pub(crate) fn scan_mc_files(dir: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return result;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Skip doc directory (documentation is not .mc definitions)
            if path.file_name().is_some_and(|n| n == "doc") {
                continue;
            }
            result.extend(scan_mc_files(&path));
        } else if path.extension().is_some_and(|ext| ext == "mc") {
            result.push(path);
        }
    }
    result
}

// === pub fn mcb_init_system_lib() { ===
/// Initialize system library: automatically scan all .mc files in the mcode/ directory
///
/// The system library does not require use statements; all definitions are globally available.
/// Similar to Python's builtins or C's standard header preloading.
///
/// system_root convention:
///   - MCC_SYSTEM_ROOT points to the data root directory
///   - System library is under system_root/mcode/
///   - If the environment variable is not set, defaults to ~/.mcode/mcode
///
/// Config-based loading:
///   - mcode loads by default in every mode
///   - Only `libs.disable_mcode: true` disables it (see `should_load_mcode`)
pub fn mcb_init_system_lib() {
    use crate::cli::config::should_load_mcode;

    debug!(target: "mcc::sysinit", "start");

    // Check config to decide whether to load mcode
    let project_root = mcb_get_project_root();
    let project_root_ref: Option<&std::path::Path> = if project_root.as_os_str().is_empty() {
        None
    } else {
        Some(&project_root)
    };

    if !should_load_mcode(project_root_ref) {
        debug!(target: "mcc::sysinit", "mcode not in libs.load config, skipping");
        if !crate::db::infra::libmgr::mcc_blibs.contains_key("mcode") {
            crate::db::infra::libmgr::mcc_blibs.insert("mcode".to_string(), McCode::new_empty());
        }
        debug!(target: "mcc::sysinit", "system lib init done (skipped)");
        return;
    }

    let system_root = mcb_get_system_root();
    let mcode_root = if system_root.as_os_str().is_empty() {
        dirs::home_dir()
            .map(|h| h.join(".mcode").join("mcode"))
            .unwrap_or_else(|| PathBuf::from(".mcode/mcode"))
    } else {
        system_root.join("mcode")
    };
    trace!(target: "mcc::sysinit", root = ?mcode_root, "got mcode root");

    if mcode_root.exists() {
        libmgr::mcb_load_lib("mcode", &mcode_root);
        debug!(target: "mcc::sysinit", "system lib loaded");
    } else {
        debug!(target: "mcc::sysinit", "mcode directory not found, registering builtins only");
        if !crate::db::infra::libmgr::mcc_blibs.contains_key("mcode") {
            crate::db::infra::libmgr::mcc_blibs.insert("mcode".to_string(), McCode::new_empty());
        }
    }

    debug!(target: "mcc::sysinit", "system lib init done");
}
