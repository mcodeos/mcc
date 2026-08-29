// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::db::cmie::tables as workspace;
use crate::db::infra::libmgr;
use crate::db::infra::mc_code::McCode;
use crate::McURI;
use std::path::Path;
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
///
/// Incremental: a file is re-derived only when it needs it —
/// - `modules_parsed == false` (freshly parsed THIS round, or never module-
///   parsed): full re-derive via parse_pass1_modules_full. Such files were
///   cleared by parse_ast/parse_ast_from_string and carry fresh parser +
///   use-stage diagnostics that the module parse does NOT re-emit — keep them.
/// - `modules_parsed && use_table_dirty`: its dependency graph changed since
///   the lapper was built (create_lapper marks reverse-dependents dirty), so
///   re-derive. Sweep its stale diagnostics first — everything it carries is
///   re-emitted below.
/// - `modules_parsed && !use_table_dirty` (clean): nothing changed, skip
///   entirely — the file and its diagnostics stay as-is. This is what keeps
///   repeated load_project/sem calls cheap instead of re-deriving every file.
///
/// The dirty flag is set DURING the loop (a re-derived dependency marks its
/// reverse-dependents dirty), so the clean/dirty decision must be made per
/// file at loop time, in topo order (deps first), not pre-computed.
pub fn mcb_parse_all_modules() {
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
    //
    // Clean files (modules_parsed && !use_table_dirty) are skipped: nothing in
    // their dependency graph changed, so their modules, symbol lapper and
    // diagnostics all remain valid from the last round. Only fresh or dirty
    // files are re-derived.
    let mut re_derived: Vec<String> = Vec::new();
    for uri in sorted_uris {
        let mcfile_opt = workspace::WORKSPACE.mcodes.remove(&uri).map(|(_k, v)| v);

        if let Some(mut mcfile) = mcfile_opt {
            let is_clean = mcfile.modules_parsed && !mcfile.use_table_dirty;
            if is_clean {
                // Keep the file untouched; re-insert in place.
                workspace::WORKSPACE.mcodes.insert(uri, mcfile);
                continue;
            }
            if mcfile.modules_parsed {
                // Dirty: its previous round's diagnostics are accumulation —
                // everything the re-derive emits below replaces them.
                workspace::WORKSPACE
                    .diagnostics
                    .lock()
                    .unwrap()
                    .clear_file(&McURI::from(uri.as_str()));
            }
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
            // Fully re-derive: module parse (re-emits module diagnostics such
            // as E5642) plus lapper rebuild. parse_pass1_modules_full is
            // idempotent across rounds — module registration replaces this
            // file's prior entry instead of firing a spurious DUP_MODULE.
            mcfile.parse_pass1_modules_full();
            // _guard drops here, automatically pops line_index
            re_derived.push(uri.clone());
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
    //
    // diagnostic_log appends (no dedup), so a validator result may only be
    // emitted for a file that was re-derived this round (its diagnostics were
    // just swept). Emitting for a clean file would append a duplicate of what
    // that file already carries. Every validator attributes its results to a
    // real workspace URI (file the definition lives in), so membership in
    // re_derived is the exact filter; unattributable results (uri: None) are
    // dropped rather than misattributed.
    {
        use crate::db::diagnostic::diagnostic::{diagnostic_log, DiagnosticLevel};
        use crate::semantic::validation::CheckRegistry;
        let re_derived_set: std::collections::HashSet<String> = re_derived.into_iter().collect();
        // Nothing changed → nothing to (re)validate: every clean file already
        // carries the results of its last re-derive round. Skip the full
        // workspace validator pass on this all-clean hot path.
        if re_derived_set.is_empty() {
            return;
        }
        let registry = CheckRegistry::with_defaults();
        let saved_uri = crate::current_uri::try_get();
        for r in registry.run_post_parse() {
            let Some(ref uri) = r.uri else {
                continue;
            };
            if !re_derived_set.contains(uri) {
                continue;
            }
            // Switch current_uri to the file this diagnostic belongs to
            crate::current_uri::set(&McURI::from(uri.as_str()));
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

    // Single resolved system root: the explicitly-set root, else the data root
    // resolved from config (`MCC_SYSTEM_ROOT` env or `~/.mcode` default). Never
    // a hardcoded `~/.mcode`.
    let system_root = mcb_get_system_root();
    let mcode_root = if system_root.as_os_str().is_empty() {
        crate::cli::datadir::data_root().join("mcode")
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
