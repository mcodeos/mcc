// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::build::pass1::canonicalize_project_uri;
use crate::db::cmie::tables as workspace;
use crate::db::infra::init::uri_equivalent;
use crate::instant::dianlu::DianLu;
use crate::instant::insttab::InstTable;
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

// === mcb_pass2 (tree-only) ===
/// Pass2: Instantiation entry point (tree only — no flat projection).
///
/// Find target module definition from global module table, create McModuleInst
/// and execute instantiation. Supports exact match and URI suffix match
/// (solves canonical path vs relative path inconsistency).
pub(crate) fn mcb_pass2(entry: &McSpaceName) -> Result<MccProjectTree, Box<dyn Error>> {
    Ok(mcb_instantiate(entry, 0)?.into_tree())
}

// === mcb_instantiate (one instantiation = one DianLu, §12.2) ===
/// Pass2 instantiation — the single construction of the core circuit object.
///
/// One instantiation = one [`DianLu`]: the module lookup + `instantiate` body
/// lives here, and both entry points ([`mcb_pass2`] / [`mcb_pass2_flat`]) are
/// thin wrappers over it. Previously `mcb_pass2_flat` re-ran the whole
/// instantiation just to flatten — the structural cause of double-instantiation
/// (and of the GAP2 double-report that diagnostic dedup then papered over).
/// `start_id` seeds the flat projection (tree-only callers pass 0 — the table
/// is never built).
pub(crate) fn mcb_instantiate(
    entry: &McSpaceName,
    start_id: u32,
) -> Result<DianLu, Box<dyn Error>> {
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
        let modules = crate::definition_space().workspace_modules();

        // 1. Exact match
        let exact = crate::definition_space()
            .get_workspace_module(entry)
            .map(|def| (entry.uri.to_string(), def));

        if let Some((uri, def)) = exact {
            matched_uri = uri;
            target_module_def = def;
        } else {
            // 2. Suffix match fallback ("main.mc" vs "/abs/path/to/main.mc")
            let entry_uri = entry.uri.as_uri();
            let canonical_entry = canonicalize_project_uri(&McURI::from(entry_uri.as_ref()));
            let suffix = modules
                .iter()
                .find(|(sn, _)| {
                    sn.ident == entry.ident
                        && uri_equivalent(&sn.uri.as_uri(), &entry_uri, &canonical_entry)
                })
                .map(|(sn, def)| (sn.uri.to_string(), (*def).clone()));

            if let Some((uri, def)) = suffix {
                matched_uri = uri;
                target_module_def = def;
            } else {
                let available: Vec<String> = modules
                    .iter()
                    .map(|(sn, _)| format!("{}@{}", sn.ident, sn.uri))
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

    Ok(DianLu::new(inst, start_id))
}

// === pub fn mcb_pass2_flat( ===
/// Pass2 + Flatten: Instantiate and generate flattened instance table (Step 7)
///
/// One instantiation via [`mcb_instantiate`], then a single one-way projection
/// into the flat `InstTable` (plus the flat electrical net checks) inside
/// [`DianLu::flatten`]. Never re-instantiates.
pub fn mcb_pass2_flat(
    entry: &McSpaceName,
    start_id: u32,
) -> Result<(MccProjectTree, InstTable), Box<dyn Error>> {
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
) -> Result<(MccProjectTree, InstTable), Box<dyn Error>> {
    let mut dl = mcb_instantiate(entry, start_id)?;
    // Project once (this also runs the flat electrical net checks), then take
    // both parts out of the object — no second instantiation, no clone.
    // Phase A: flatten returns the net-check diagnostics; the build layer owns
    // logging them into the workspace (the current_uri context is ours).
    let diags = dl.flatten_with_prefix(synthetic_prefix);
    crate::semantic::validation::nets::log_net_check_diagnostics(&diags);
    Ok(dl.into_parts())
}
