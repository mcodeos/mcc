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
/// Without this, DashMap iteration is unordered, so hbl.mc modules could be parsed
/// before power.mc modules are registered, causing "definition not found" errors.
pub fn mcb_parse_all_modules() {
    // 1. Collect all URIs and their dependencies
    let mut uri_deps: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
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
    let all_uris: Vec<String> = uri_deps.keys().cloned().collect();

    fn topo_visit(
        uri: &str,
        uri_deps: &std::collections::HashMap<String, Vec<String>>,
        visited: &mut std::collections::HashSet<String>,
        sorted: &mut Vec<String>,
    ) {
        if visited.contains(uri) {
            return;
        }
        visited.insert(uri.to_string());
        if let Some(deps) = uri_deps.get(uri) {
            for dep in deps {
                topo_visit(dep, uri_deps, visited, sorted);
            }
        }
        sorted.push(uri.to_string());
    }

    for uri in &all_uris {
        topo_visit(uri, &uri_deps, &mut visited, &mut sorted_uris);
    }

    // 3. Parse modules in dependency order
    // Use remove+insert instead of clone+insert to avoid AstNode ownership issues.
    // Clone creates a shallow AstNode copy (owned=false) that dangles when the
    // original (owned=true) is dropped during insert replacement.
    for uri in sorted_uris {
        let mcfile_opt = workspace::WORKSPACE.mcodes.remove(&uri).map(|(_k, v)| v);

        if let Some(mut mcfile) = mcfile_opt {
            crate::current_uri::set(&uri);
            mcfile.parse_pass1_modules();
            workspace::WORKSPACE.mcodes.insert(uri, mcfile);
        }
    }

    // ★ Validation: run PostParse checks after all modules parsed (once)
    {
        use crate::db::diagnostic::diagnostic::{diagnostic_log, DiagnosticLevel};
        use crate::semantic::validation::{CheckRegistry, PostParseContext};
        use std::sync::LazyLock;
        static POST_PARSE_RUN: LazyLock<std::sync::Mutex<bool>> =
            LazyLock::new(|| std::sync::Mutex::new(false));
        let mut flag = POST_PARSE_RUN.lock().unwrap_or_else(|e| e.into_inner());
        if !*flag {
            *flag = true;
            let ctx = PostParseContext::new();
            let registry = CheckRegistry::with_defaults();
            let saved_uri = crate::current_uri::try_get();
            for r in registry.run_post_parse(&ctx) {
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
///   - Check `libs.load` config (mcc.yaml or project.toml)
///   - If empty: do not load mcode by default
///   - If contains "mcode": load mcode library
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
