// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::ast::ast_semantic::{DeclareId, Span};
use crate::db::cmie::tables as workspace;
use crate::refdef::types::CmieKind;
use crate::semantic::common::McSpaceName;
use crate::McIds;
use crate::McURI;

// === pub fn mcb_lookup_instance_decl(uri: &McURI, name: &str, scope: Option<&str>) -> ===
/// 🆕 Look up declare_id by instance name
///
/// Returns the DeclareId for a given instance name, if registered.
pub fn mcb_lookup_instance_decl(uri: &McURI, name: &str, scope: Option<&str>) -> Option<DeclareId> {
    let scope_str = scope.unwrap_or("");
    // First try exact URI match
    if let Some(mcode) = workspace::WORKSPACE.mcodes.get(uri) {
        if let Ok(sem) = mcode.symbols.lock() {
            // Use scope_index for precise scope-based lookup
            if let Some((id, _)) = sem.local_table.lookup_by_scope_name(scope_str, name) {
                return Some(id);
            }
            // Fallback: iterate and match by name only (cross-scope within same file)
            for ((_fid, _cid, _fnid, n), (id, _)) in sem.local_table.name_to_declare_id.iter() {
                if n == name {
                    return Some(*id);
                }
            }
        }
    }
    // Cross-file fallback
    for entry in workspace::WORKSPACE.mcodes.iter() {
        if let Ok(sem) = entry.value().symbols.lock() {
            if let Some((id, _)) = sem.local_table.lookup_by_scope_name(scope_str, name) {
                return Some(id);
            }
        }
    }
    None
}

// === pub fn mcb_register_instance_ref(uri: &McURI, span: Span, decl_id: DeclareId, sc ===
/// 🆕 Register an instance reference in the global symbol table
///
/// Called when an instance name is used elsewhere in the module (e.g., `uC.i2c()`).
/// The reference is linked to the declaration via decl_id.
pub fn mcb_register_instance_ref(
    uri: &McURI,
    span: Span,
    decl_id: DeclareId,
    _scope: Option<&str>,
) {
    if let Some(mcode) = workspace::WORKSPACE.mcodes.get(uri) {
        if let Ok(mut sem) = mcode.symbols.lock() {
            sem.local_table.add_inst(span, decl_id);
        }
    }
}

// === pub fn mcb_get_refs(name: &str) -> Vec<(String, String, Span)> { ===
/// M6: Get all references for a named declaration.
/// Returns Vec<(uri, scope, span)>.
pub fn mcb_get_refs(name: &str) -> Vec<(String, String, Span)> {
    let mut results = Vec::new();
    for entry in workspace::WORKSPACE.mcodes.iter() {
        if let Ok(sem) = entry.value().symbols.lock() {
            // Find decl_ids matching name
            let mut decl_ids: Vec<DeclareId> = Vec::new();
            for ((_fid, _cid, _fnid, n), (id, _)) in sem.local_table.name_to_declare_id.iter() {
                if n == name {
                    decl_ids.push(*id);
                }
            }
            // Find refs for those decl_ids
            for (inst_id, decl_id) in sem.local_table.inst_id_to_declare_inst.iter() {
                if decl_ids.contains(decl_id) {
                    if let Some(span) = sem.local_table.inst_id_to_span.get(inst_id) {
                        results.push((entry.key().to_string(), String::new(), span.clone()));
                    }
                }
            }
        }
    }
    results
}

/// Register a system library class in the global table, returning its DeclareId.
/// If already registered, returns the existing id; otherwise computes a stable
/// (hash-based) DeclareId and stores it in an available file's global table.
///
/// ★ Previously used `gt.add_class()` which assigns a sequential
/// id from whatever file's per-file counter happened to be first in DashMap
/// iteration order — non-deterministic and meaningless to the referencing file.
/// Now uses `assign_declare_id_stable` for a deterministic id based on (uri, name).
fn register_lib_class_in_global_table(
    def_uri: &str,
    class_name: &str,
    def_span: &std::ops::Range<usize>,
) -> DeclareId {
    let mc_uri = McURI::from(def_uri);
    // Try to find it in any loaded file's global table first
    let binding = &workspace::WORKSPACE.mcodes;
    for entry in binding.iter() {
        if let Ok(sem) = entry.value().symbols.lock() {
            if let Ok(gt) = sem.global_table.lock() {
                if let Some(&cid) = gt
                    .class_name_to_id
                    .get(&(mc_uri.clone(), McIds::from(class_name)))
                {
                    return cid;
                }
            }
        }
    }
    // Not found — compute a stable (deterministic) DeclareId and register
    // in the first available file's global table. Using a hash-based id
    // avoids the non-determinism of per-file sequential counters (Defect 73).
    let cid = crate::ast::ast_semantic::LocalSymbolTable::assign_declare_id_stable(
        &mc_uri, "", class_name,
    );
    for entry in binding.iter() {
        if let Ok(sem) = entry.value().symbols.lock() {
            if let Ok(mut gt) = sem.global_table.lock() {
                // Only insert if not already present (avoid overwriting)
                gt.class_name_to_id
                    .entry((mc_uri.clone(), McIds::from(class_name)))
                    .or_insert(cid);
                gt.class_id_to_span
                    .entry(cid)
                    .or_insert((mc_uri.clone(), def_span.clone()));
                return cid;
            }
        }
    }
    // Fallback: return default (shouldn't happen if workspace has at least one file)
    DeclareId::default()
}

// === pub fn mcb_register_declare_class(uri: &McURI, class_name: McIds, span: Span) { ===
/// 🆕 Register a class reference for goto-definition
///
/// Called when a class name is used in a declare statement (e.g., `comp.sub uC`).
/// Registers the class reference so LSP can jump from the reference to the class definition.
///
/// §5.4 visibility gate: cross-file candidates are only accepted when the
/// referencing file can actually see them — P3 (same file), P4 (use chain),
/// or P5 (mcode). This stops goto-def from jumping into files the
/// referencing file never `use`d (the `net1.basic.mc` → `c3.defs.mc` `DC`
/// regression). Unresolved refs fall back to the sentinel path, which
/// re-resolves later through the visibility-aware `Resolver`.
fn cross_file_class_visible(uri: &McURI, target_uri: &str, name_str: &str) -> bool {
    if target_uri == uri.as_str() {
        return true; // P3: same file
    }
    if crate::db::resolve::use_chain_reaches(uri, target_uri) {
        return true; // P4: reachable through the use chain
    }
    // P5: mcode system library. System-lib-only by construction — a definition
    // in a *different project file* must reach the referrer through the use
    // chain (P4) instead, so the unified DefinitionSpace views would wrongly
    // admit those (see the system-view doc in defspace.rs).
    let space = McSpaceName::new(&McIds::from(name_str), McURI::from(target_uri));
    crate::definition_space().system_contains(&space)
}

pub fn mcb_register_declare_class(uri: &McURI, class_name: &McIds, raw_span: Span) {
    // ★ Fix: mc_value_link (C-side) extends MCAST_IDS node `len` to include
    // linked MCAST_PARAMS, so raw_span may cover "RES(10kΩ)" instead of "RES".
    // class_name from McIds is already correctly parsed without params, so
    // reconstruct the span from the flattened name's length.
    let name_str = class_name.to_string();
    let span = raw_span.start..(raw_span.start + name_str.len());

    // Step 1: Find (class_id, target_uri, target_span) — try lsp.class_table first
    // Priority: same URI as reference > other URIs (for duplicate class definitions).
    //
    // Candidates are collected inside the class_table lock, but the §5.4
    // visibility gate for cross-URI candidates runs AFTER the lock is
    // released: the gate walks WORKSPACE.mcodes (DashMap), and running it
    // while holding class_table would invert the mcodes -> class_table lock
    // order used by other paths, deadlocking under parallel parsing.
    let uri_str = uri.to_string();
    let found = {
        let mut same_uri_result: Option<(DeclareId, String, Span, u8)> = None;
        let mut other_candidates: Vec<(DeclareId, String, Span, u8)> = Vec::new();
        {
            let class_table = workspace::WORKSPACE.lsp.class_table.lock().unwrap();
            tracing::debug!(target: "crate::lsp", "  register_declare_class: lsp.class_table size={}", class_table.len());
            for ((target_uri, kind, name), &(class_id, ref target_span)) in class_table.iter() {
                if name != &name_str {
                    continue;
                }
                let info = (
                    class_id,
                    target_uri.clone(),
                    target_span.clone(),
                    container_kind_cmie(kind),
                );
                if target_uri == &uri_str {
                    // First try: exact URI match (same file as reference).
                    if same_uri_result.is_none() {
                        same_uri_result = Some(info);
                    }
                } else {
                    // Second try: different URI (fallback for cross-file references).
                    other_candidates.push(info);
                }
            }
        }

        same_uri_result.or_else(|| {
            other_candidates.into_iter().find(|(_, target_uri, _, _)| {
                // §5.4 gate: only accept a candidate whose file is visible from
                // the referencing file (P3 same file / P4 use chain / P5 mcode).
                cross_file_class_visible(uri, target_uri, &name_str)
            })
        })
    };
    if found.is_none() {
        tracing::debug!(target: "crate::lsp", "  register_declare_class: lsp.class_table miss for '{}'", class_name);
    } else {
        tracing::info!(target: "crate::lsp", "  register_declare_class: lsp.class_table hit for '{}'", class_name);
    }

    // Step 2: Try workspace files' global tables if not found above.
    //
    // Same lock discipline as Step 1: candidates are collected while holding
    // the file's symbols/global_table locks, but the §5.4 gate runs AFTER all
    // locks are released. The gate walks WORKSPACE.mcodes (DashMap), and
    // running it while holding an mcodes entry's symbols lock would invert the
    // mcodes -> symbols lock order used by create_lapper, deadlocking under
    // parallel parsing.
    let from_mcodes: Option<(DeclareId, String, Span, u8)> = if found.is_none() {
        let binding = &workspace::WORKSPACE.mcodes;
        let mut candidates: Vec<(DeclareId, String, Span)> = Vec::new();
        for entry in binding.iter() {
            if let Ok(sem) = entry.value().symbols.lock() {
                if let Ok(gt) = sem.global_table.lock() {
                    for ((file_uri, name), &cid) in gt.class_name_to_id.iter() {
                        if name == &McIds::from(name_str.as_str()) {
                            if let Some((_, tspan)) = gt.class_id_to_span.get(&cid) {
                                candidates.push((cid, file_uri.clone(), tspan.clone()));
                            }
                        }
                    }
                }
            }
        }
        candidates
            .into_iter()
            .find(|(_, file_uri, _)| {
                // §5.4 gate: only accept a candidate whose file is visible from
                // the referencing file (P3 same file / P4 use chain / P5 mcode).
                cross_file_class_visible(uri, file_uri, &name_str)
            })
            .map(|(cid, file_uri, tspan)| {
                (
                    cid,
                    file_uri.clone(),
                    tspan,
                    cmie_kind_for(&file_uri, &name_str),
                )
            })
    } else {
        None
    };

    let class_info = if let Some(info) = found {
        Some(info)
    } else {
        from_mcodes
    };

    // Step 2.5: Search workspace tables (project-level) and system library tables
    // for classes that may not be in the global table yet (e.g. because the
    // defining file hasn't been parsed when this reference is encountered).
    // ★ Fix: Register found classes in the global table to get a real DeclareId
    // instead of using DeclareId::default(). Without this, all library class
    // refs map to class_id=0 with invalid def spans in Layer 1.
    //
    // Candidates are collected first — the §5.4 gate (which walks
    // WORKSPACE.mcodes) is never invoked while iterating a workspace/global
    // DashMap or while holding any file lock, preserving the same lock
    // discipline as Steps 1/2.
    let from_syslibs: Option<(DeclareId, String, Span, u8)> = if class_info.is_none() {
        let mut result = None;

        // 2.5a: workspace tables first (project-level definitions from `use` directives),
        // gated by §5.4 visibility (P3/P4/P5) — a workspace file's symbols are
        // importable only via `use`, never by bare name.
        let mut ws_candidates: Vec<(String, std::ops::Range<usize>, CmieKind)> = Vec::new();
        for (sn, comp) in crate::definition_space().workspace_components() {
            if sn.ident.to_string() == name_str {
                ws_candidates.push((sn.uri.to_string(), comp.span.clone(), CmieKind::Component));
            }
        }
        for (sn, module) in crate::definition_space().workspace_modules() {
            if sn.ident.to_string() == name_str {
                ws_candidates.push((sn.uri.to_string(), module.span.clone(), CmieKind::Module));
            }
        }
        for (sn, iface) in crate::definition_space().workspace_interfaces() {
            if sn.ident.to_string() == name_str {
                ws_candidates.push((sn.uri.to_string(), iface.span.clone(), CmieKind::Interface));
            }
        }
        for (sn, def) in crate::definition_space().workspace_enums() {
            if sn.ident.to_string() == name_str {
                let s = def.span;
                ws_candidates.push((
                    sn.uri.to_string(),
                    s[0] as usize..s[1] as usize,
                    CmieKind::Enum,
                ));
            }
        }
        for (def_uri, def_span, kind) in ws_candidates {
            if cross_file_class_visible(uri, &def_uri, &name_str) {
                let class_id = register_lib_class_in_global_table(&def_uri, &name_str, &def_span);
                result = Some((class_id, def_uri, def_span, kind as u8));
                break;
            }
        }

        // 2.5b: system library tables — classes from loaded libraries are
        // always visible (P5), so no gate is applied. Read through the
        // DefinitionSpace system-only view (defspace.rs): the unified `all_*`
        // enumerations mix in workspace definitions, which a cross-file
        // referrer may not `use`, so they must not back this scan.
        if result.is_none() {
            let ds = crate::definition_space();
            let mut sys_candidates: Vec<(String, std::ops::Range<usize>, CmieKind)> = Vec::new();
            for (sn, def) in ds.system_components() {
                if sn.ident.to_string() == name_str {
                    sys_candidates.push((
                        sn.uri.to_string(),
                        def.span.clone(),
                        CmieKind::Component,
                    ));
                    break;
                }
            }
            if sys_candidates.is_empty() {
                for (sn, def) in ds.system_modules() {
                    if sn.ident.to_string() == name_str {
                        sys_candidates.push((
                            sn.uri.to_string(),
                            def.span.clone(),
                            CmieKind::Module,
                        ));
                        break;
                    }
                }
            }
            if sys_candidates.is_empty() {
                for (sn, def) in ds.system_interfaces() {
                    if sn.ident.to_string() == name_str {
                        sys_candidates.push((
                            sn.uri.to_string(),
                            def.span.clone(),
                            CmieKind::Interface,
                        ));
                        break;
                    }
                }
            }
            if sys_candidates.is_empty() {
                for (sn, def) in ds.system_enums() {
                    if sn.ident.to_string() == name_str {
                        let s = def.span;
                        sys_candidates.push((
                            sn.uri.to_string(),
                            s[0] as usize..s[1] as usize,
                            CmieKind::Enum,
                        ));
                        break;
                    }
                }
            }
            for (def_uri, def_span, kind) in sys_candidates {
                let class_id = register_lib_class_in_global_table(&def_uri, &name_str, &def_span);
                result = Some((class_id, def_uri, def_span, kind as u8));
                break;
            }
        }
        result
    } else {
        None
    };
    let class_info = class_info.or(from_syslibs);

    // Step 3: Store in workspace-level table
    if let Some((class_id, target_uri, target_span, cmie_kind)) = class_info {
        let span_clone = span.clone();
        let uri_str = uri.to_string();
        tracing::info!(target: "crate::lsp", "  register_declare_class: storing ref decl_span={:?} -> class_id={:?} target={}", span_clone, class_id, target_uri);
        tracing::info!(target: "crate::lsp", "Registered declare_class: {} at {:?} -> class_id={:?}", class_name, span_clone, class_id);
        let mut refs = workspace::WORKSPACE.lsp.declare_class_refs.lock().unwrap();
        refs.entry(uri_str).or_default().push((
            span,
            class_id,
            target_uri,
            target_span,
            class_name.clone(),
            cmie_kind,
        ));
    } else {
        // ★ Do NOT emit E1601 here during P4, because WORKSPACE.modules
        // is empty at that point (modules are registered in P5). The class ref is stored
        // below with DeclareId::default() sentinel; resolve_class_ref_at_span in
        // create_lapper will re-resolve it correctly after all modules are parsed.
        // Emitting an Error here would leave a permanent false positive in the diagnostic
        // table that is never retracted.
        tracing::info!(target: "crate::lsp", "register_declare_class: {} not resolved cross-file (P4 — deferring to create_lapper)", class_name);
        // ★ LSP: Even without cross-file resolution, register the class-name
        // span as a declare_class entry in the lapper.  This lets mcext's
        // F12 handler pick it up and resolve via project index.
        tracing::info!(target: "crate::lsp", "register_declare_class: {} not resolved cross-file, registering local span {:?} for lapper", class_name, span);
        let uri_str = uri.to_string();
        // Use a synthetic sentinel: target_uri="" and target_span=[0,0].
        // create_lapper will emit DeclareClass for this span; mcext's
        // project-index fallback will resolve the actual definition.
        let mut refs = workspace::WORKSPACE.lsp.declare_class_refs.lock().unwrap();
        refs.entry(uri_str).or_default().push((
            span,
            DeclareId::default(),
            "".to_string(),
            0..0,
            class_name.clone(),
            CmieKind::UNKNOWN,
        ));
    }
}

/// Register interface class refs found in a `func` header parameter list
/// (e.g. `func GD25Q32E([V3V3, GND]::DC(3.3V))` → class ref `DC`), mirroring
/// the module-port registration in module/mod.rs. Without this, class refs in
/// func headers are never entered into the lapper / RefDefMap, so goto-def
/// and hover cannot resolve them (unlike module ports and component pin
/// bindings).
pub(crate) fn register_func_header_iface_refs(
    func_node: &crate::ast::ast_node::AstNode,
    uri: &McURI,
) {
    use crate::ast::c_macros::{MCAST_DECLARE, MCAST_PARAM, MCAST_PARAMS};
    use crate::semantic::basic::mc_param_type::{McParamType, McParamTypeKind};
    // MCAST_FUNCTION → MCAST_PARAMS → (MCAST_PARAM → MCAST_DECLARE)*
    let Some(params_node) = func_node
        .get_sub_node()
        .and_then(|sub| sub.iter().find(|n| n.get_type() == MCAST_PARAMS))
    else {
        return;
    };
    let Some(first_param) = params_node.get_sub_node() else {
        return;
    };
    for param_node in first_param.iter() {
        // Unwrap MCAST_PARAM wrappers (mc_pard rules can nest them).
        let mut declare_node = param_node.get_sub_node();
        while let Some(n) = &declare_node {
            if n.get_type() != MCAST_PARAM {
                break;
            }
            declare_node = n.get_sub_node();
        }
        let Some(declare_node) = declare_node else {
            continue;
        };
        if declare_node.get_type() != MCAST_DECLARE {
            continue;
        }
        // Only interface-typed declares (e.g. `::DC(3.3V)`), same detection
        // as module ports — enum/plain declares are handled elsewhere.
        let pt = McParamType::from_ast(&declare_node);
        let is_interface = matches!(
            pt.kind,
            McParamTypeKind::Interface { .. } | McParamTypeKind::InterfaceWithRole { .. }
        );
        if !is_interface {
            continue;
        }
        if let Some((class_name, class_span)) =
            crate::semantic::module::McModule::extract_declare_class_span(&declare_node)
        {
            mcb_register_declare_class(uri, &class_name, class_span);
        }
    }
}

/// Map a `ContainerKind` (class_table key) to the CMIE kind ordinal carried by
/// RefDefEntry. Function/File kinds are not classes — they map to UNKNOWN.
fn container_kind_cmie(kind: &crate::ContainerKind) -> u8 {
    use crate::ContainerKind;
    match kind {
        ContainerKind::Component => CmieKind::Component as u8,
        ContainerKind::Module => CmieKind::Module as u8,
        ContainerKind::Interface => CmieKind::Interface as u8,
        ContainerKind::Enum => CmieKind::Enum as u8,
        _ => CmieKind::UNKNOWN,
    }
}

/// Resolve a class's CMIE kind by name + defining uri from the workspace and
/// system library tables (name-based — never id-guessing across files).
/// Used by class-ref registration (refs.rs) and by Layer 1a in mc_code.rs,
/// so a class ref to an `interface` hovers as `→ interface`.
pub(crate) fn cmie_kind_for(def_uri: &str, name: &str) -> u8 {
    let space = McSpaceName {
        ident: McIds::from(name),
        uri: crate::semantic::common::uri_intern(def_uri),
    };
    // Unified workspace-then-system-lib lookups (design §12.4 rule 1): a class
    // is its identity regardless of which table system holds it.
    let ds = crate::definition_space();
    if ds.get_component(&space).is_some() {
        return CmieKind::Component as u8;
    }
    if ds.get_module(&space).is_some() {
        return CmieKind::Module as u8;
    }
    if ds.get_interface(&space).is_some() {
        return CmieKind::Interface as u8;
    }
    if ds.get_enum(&space).is_some() {
        return CmieKind::Enum as u8;
    }
    CmieKind::UNKNOWN
}
