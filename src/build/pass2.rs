// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::build::pass1::canonicalize_project_uri;
use crate::db::cmie::tables as workspace;
use crate::db::infra::init::uri_equivalent;
use crate::instant::mc_mod::McModuleInst;
use crate::ParserResult;
use crate::{McSpaceName, McURI};
use std::error::Error;

pub type MccProjectTree = McModuleInst;

// === pub fn mcb_query<'a>(uri: &McURI) -> Option<ParserResult> { ===
pub fn mcb_query<'a>(uri: &McURI) -> Option<ParserResult> {
    let binding = &workspace::WORKSPACE.mcodes;
    let canonical_uri = canonicalize_project_uri(uri);

    if let Some(mcfile) = binding.get(&canonical_uri) {
        return Some(ParserResult {
            sem_tokens: mcfile.tokens.clone(),
            sem_symbols: mcfile.symbols.clone(),
        });
    }

    if let Some(mcfile) = binding.get(uri) {
        return Some(ParserResult {
            sem_tokens: mcfile.tokens.clone(),
            sem_symbols: mcfile.symbols.clone(),
        });
    }

    for entry in binding.iter() {
        if uri_equivalent(entry.key(), uri.as_str(), &canonical_uri) {
            return Some(ParserResult {
                sem_tokens: entry.tokens.clone(),
                sem_symbols: entry.symbols.clone(),
            });
        }
    }

    None
}

// === pub(crate) fn mcb_pass2(entry: &McSpaceName) -> Result<MccProjectTree, Box<dyn E ===
/// Pass2: Instantiation entry point
///
/// Find target module definition from global module table, create McModuleInst and execute instantiation.
/// Supports exact match and URI suffix match (solves canonical path vs relative path inconsistency).
pub(crate) fn mcb_pass2(entry: &McSpaceName) -> Result<MccProjectTree, Box<dyn Error>> {
    // FIX: Extract module def from prj_modules and DROP the MutexGuard
    // BEFORE calling inst.instantiate(). instantiate() internally calls
    // mcb_get_cmie() -> prj_modules.borrow() which would deadlock if the
    // lock is still held (std::sync::Mutex is NOT reentrant).
    //
    // We avoid returning DashMap Ref temporaries from block expressions,
    // which would extend their borrow lifetime past the MutexGuard drop.
    let matched_uri;
    let target_module_def;

    {
        let binding = &workspace::WORKSPACE.modules;

        // 1. Exact match
        let exact = binding
            .get(entry)
            .map(|r| (entry.uri.to_string(), r.value().clone()));

        if let Some((uri, def)) = exact {
            matched_uri = uri;
            target_module_def = def;
        } else {
            // 2. Suffix match fallback ("main.mc" vs "/abs/path/to/main.mc")
            let entry_uri = entry.uri.as_uri();
            let canonical_entry = canonicalize_project_uri(&McURI::from(entry_uri.as_ref()));
            let suffix = binding
                .iter()
                .find(|e| {
                    e.key().ident == entry.ident
                        && uri_equivalent(&e.key().uri.as_uri(), &entry_uri, &canonical_entry)
                })
                .map(|e| (e.key().uri.to_string(), e.value().clone()));

            if let Some((uri, def)) = suffix {
                matched_uri = uri;
                target_module_def = def;
            } else {
                let available: Vec<String> = binding
                    .iter()
                    .map(|e| format!("{}@{}", e.key().ident, e.key().uri))
                    .collect();
                return Err(format!(
                    "Target module not found: {} (uri={})\n  Available modules: [{}]",
                    entry.ident,
                    entry.uri,
                    available.join(", ")
                )
                .into());
            }
        }
    } // binding (MutexGuard) dropped here, BEFORE instantiate()

    let mut inst = McModuleInst::new(&entry.ident.to_string(), target_module_def);

    crate::current_uri::set(&matched_uri);

    // ★ Line indices for instantiation: `create_connection` converts the
    // statement byte offset to a real line number via `lookup_line_col`, which
    // searches the thread-local line-index stack. That stack is only populated
    // during parsing (LineIndexGuard in pass1.rs); without it every connection
    // resolved to line 1 during Pass2. Push guards for all loaded files so
    // source_span lines (and the GND group names) are real. Auto-popped on drop.
    let _line_index_guards: Vec<_> = workspace::WORKSPACE
        .mcodes
        .iter()
        .filter_map(|entry| {
            let (uri, mcfile) = (entry.key(), entry.value());
            mcfile
                .line_index
                .as_ref()
                .map(|idx| crate::db::infra::context::LineIndexGuard::new(uri.clone(), idx.clone()))
        })
        .collect();

    inst.instantiate()
        .map_err(|e| -> Box<dyn Error> { Box::new(e) })?;

    Ok(inst)
}

// === pub fn mcb_pass2_flat( ===
/// Pass2 + Flatten: Instantiate and generate flattened instance table (Step 7)
///
/// First execute mcb_pass2 to build McModuleInst tree,
/// then flatten it into InstTable one-dimensional table.
pub fn mcb_pass2_flat(
    entry: &McSpaceName,
    start_id: u32,
) -> Result<(MccProjectTree, crate::instant::insttab::InstTable), Box<dyn Error>> {
    mcb_pass2_flat_with(entry, start_id, None)
}

/// Like [`mcb_pass2_flat`], but marks every entry under `synthetic_prefix` (a
/// virtual-instantiation wrapper module, e.g. `VIRT_XTAL4`) as `synthetic`
/// BEFORE the electrical net checks run. The unwired/pin-count checks skip
/// synthetic instances, so a standalone component/interface file view must not
/// report E4112 "no pins connected" / E4116 "N of M pins connected" — an
/// unwired box is exactly what such a view IS. (`virtual_build_flat` builds the
/// synthetic wrapper module through this entry point.)
pub(crate) fn mcb_pass2_flat_with(
    entry: &McSpaceName,
    start_id: u32,
    synthetic_prefix: Option<&str>,
) -> Result<(MccProjectTree, crate::instant::insttab::InstTable), Box<dyn Error>> {
    let inst = mcb_pass2(entry)?;
    let mut table = crate::instant::insttab::InstTable::from_module_inst(&inst, start_id);
    if let Some(prefix) = synthetic_prefix {
        table.mark_synthetic_by_path_prefix(prefix);
    }
    // ★ Electrical checks after pass2
    let net_results = crate::semantic::validation::nets::run_net_checks(&table);
    let saved_uri = crate::current_uri::try_get();
    for r in &net_results {
        // Switch to the file this diagnostic belongs to
        if !r.uri.is_empty() {
            crate::current_uri::set(&crate::McURI::from(r.uri.as_str()));
        }
        let level = match r.severity {
            "error" => crate::db::diagnostic::diagnostic::DiagnosticLevel::Error,
            "info" => crate::db::diagnostic::diagnostic::DiagnosticLevel::Info,
            _ => crate::db::diagnostic::diagnostic::DiagnosticLevel::Warning,
        };
        crate::db::diagnostic::diagnostic::diagnostic_log(r.code, level, r.pos, 0, &r.message, &[]);
    }
    // Restore previous current_uri
    match saved_uri {
        Some(ref uri) => crate::current_uri::set(uri),
        None => crate::current_uri::reset(),
    }
    Ok((inst, table))
}
