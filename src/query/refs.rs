// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::ast::ast_semantic::{DeclareId, Span};
use crate::db::cmie::tables as workspace;
use crate::db::infra::global;
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
/// If already registered, returns the existing id; otherwise calls `add_class`.
fn register_lib_class_in_global_table(
    def_uri: &str,
    class_name: &str,
    def_span: &std::ops::Range<usize>,
) -> DeclareId {
    // Try to find it in any loaded file's global table first
    let binding = &workspace::WORKSPACE.mcodes;
    for entry in binding.iter() {
        if let Ok(sem) = entry.value().symbols.lock() {
            if let Ok(gt) = sem.global_table.lock() {
                // Check if already registered by (uri, name)
                let mc_uri = McURI::from(def_uri);
                if let Some(&cid) = gt.class_name_to_id.get(&(mc_uri, class_name.to_string())) {
                    return cid;
                }
            }
        }
    }
    // Not found — register it in the first available file's global table
    for entry in binding.iter() {
        if let Ok(sem) = entry.value().symbols.lock() {
            if let Ok(mut gt) = sem.global_table.lock() {
                let mc_uri = McURI::from(def_uri);
                return gt.add_class(&mc_uri, &class_name.to_string(), def_span.clone());
            }
        }
    }
    // Fallback: return default (shouldn't happen if workspace has at least one file)
    DeclareId::default()
}

// === pub fn mcb_register_declare_class(uri: &McURI, class_name: &str, span: Span) { ===
/// 🆕 Register a class reference for goto-definition
///
/// Called when a class name is used in a declare statement (e.g., `MCU.US513_20_F uC`).
/// Registers the class reference so LSP can jump from the reference to the class definition.
pub fn mcb_register_declare_class(uri: &McURI, class_name: &str, span: Span) {
    // Step 1: Find (class_id, target_uri, target_span) — try lsp.class_table first
    // Priority: same URI as reference > other URIs (for duplicate class definitions)
    let uri_str = uri.to_string();
    let found = {
        let class_table = workspace::WORKSPACE.lsp.class_table.lock().unwrap();
        tracing::debug!(target: "crate::lsp", "  register_declare_class: lsp.class_table size={}", class_table.len());

        // First try: exact URI match (same file as reference)
        let same_uri_result = class_table.iter().find_map(
            |((target_uri, _kind, name), &(class_id, ref target_span))| {
                if name == class_name && target_uri == &uri_str {
                    Some((class_id, target_uri.clone(), target_span.clone()))
                } else {
                    None
                }
            },
        );

        // Second try: different URI (fallback for cross-file references)
        let other_uri_result = if same_uri_result.is_none() {
            class_table.iter().find_map(
                |((target_uri, _kind, name), &(class_id, ref target_span))| {
                    if name == class_name && target_uri != &uri_str {
                        Some((class_id, target_uri.clone(), target_span.clone()))
                    } else {
                        None
                    }
                },
            )
        } else {
            None
        };

        let result = same_uri_result.or(other_uri_result);
        if result.is_none() {
            tracing::debug!(target: "crate::lsp", "  register_declare_class: lsp.class_table miss for '{}'", class_name);
        } else {
            tracing::info!(target: "crate::lsp", "  register_declare_class: lsp.class_table hit for '{}'", class_name);
        }
        result
    };

    // Step 2: Try workspace files' global tables if not found above
    let from_mcodes: Option<(DeclareId, String, Span)> = if found.is_none() {
        let binding = &workspace::WORKSPACE.mcodes;
        let mut result = None;
        for entry in binding.iter() {
            if let Ok(sem) = entry.value().symbols.lock() {
                if let Ok(gt) = sem.global_table.lock() {
                    for ((file_uri, name), &cid) in gt.class_name_to_id.iter() {
                        if name == class_name {
                            if let Some((_, tspan)) = gt.class_id_to_span.get(&cid) {
                                result = Some((cid, file_uri.clone(), tspan.clone()));
                                break;
                            }
                        }
                    }
                }
            }
            if result.is_some() {
                break;
            }
        }
        result
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
    let from_syslibs: Option<(DeclareId, String, Span)> = if class_info.is_none() {
        let name_str = class_name.to_string();
        let mut result = None;

        // Helper macro to reduce repetition
        macro_rules! try_register {
            ($def_uri:expr, $def_span:expr) => {
                let def_uri_str = ($def_uri).to_string();
                let def_span_range = ($def_span);
                let class_id = register_lib_class_in_global_table(
                    &def_uri_str, &name_str, &def_span_range,
                );
                result = Some((class_id, def_uri_str, def_span_range));
            };
        }

        // 2.5a: Search workspace tables first (project-level definitions from `use` directives)
        for entry in workspace::WORKSPACE.components.iter() {
            if entry.key().ident.to_string() == name_str {
                try_register!(entry.key().uri, entry.value().span.clone());
                break;
            }
        }
        if result.is_none() {
            for entry in workspace::WORKSPACE.modules.iter() {
                if entry.key().ident.to_string() == name_str {
                    try_register!(entry.key().uri, entry.value().span.clone());
                    break;
                }
            }
        }
        if result.is_none() {
            for entry in workspace::WORKSPACE.interfaces.iter() {
                if entry.key().ident.to_string() == name_str {
                    try_register!(entry.key().uri, entry.value().span.clone());
                    break;
                }
            }
        }
        if result.is_none() {
            for entry in workspace::WORKSPACE.enums.iter() {
                if entry.key().ident.to_string() == name_str {
                    let s = entry.value().span;
                    try_register!(entry.key().uri, s[0] as usize..s[1] as usize);
                    break;
                }
            }
        }

        // 2.5b: Search system library tables (global::mcc_*) — classes from loaded libraries
        if result.is_none() {
            for entry in global::mcc_components.iter() {
                if entry.key().ident.to_string() == name_str {
                    try_register!(entry.key().uri, entry.value().span.clone());
                    break;
                }
            }
        }
        if result.is_none() {
            for entry in global::mcc_modules.iter() {
                if entry.key().ident.to_string() == name_str {
                    try_register!(entry.key().uri, entry.value().span.clone());
                    break;
                }
            }
        }
        if result.is_none() {
            for entry in global::mcc_interfaces.iter() {
                if entry.key().ident.to_string() == name_str {
                    try_register!(entry.key().uri, entry.value().span.clone());
                    break;
                }
            }
        }
        if result.is_none() {
            for entry in global::mcc_enums.iter() {
                if entry.key().ident.to_string() == name_str {
                    let s = entry.value().span;
                    try_register!(entry.key().uri, s[0] as usize..s[1] as usize);
                    break;
                }
            }
        }
        result
    } else {
        None
    };
    let class_info = class_info.or(from_syslibs);

    // Step 3: Store in workspace-level table
    if let Some((class_id, target_uri, target_span)) = class_info {
        let span_clone = span.clone();
        let uri_str = uri.to_string();
        tracing::info!(target: "crate::lsp", "  register_declare_class: storing ref decl_span={:?} -> class_id={:?} target={}", span_clone, class_id, target_uri);
        let mut refs = workspace::WORKSPACE.lsp.declare_class_refs.lock().unwrap();
        refs.entry(uri_str)
            .or_default()
            .push((span, class_id, target_uri, target_span));
        tracing::info!(target: "crate::lsp", "Registered declare_class: {} at {:?} -> class_id={:?}", class_name, span_clone, class_id);
    } else {
        // ★ Diagnostic: class definition not found — emit warning for IDE.
        crate::db::diagnostic::diagnostic::diagnostic_log(
            1601,
            crate::db::diagnostic::diagnostic::DiagnosticLevel::Error,
            span.start as u32,
            (span.end - span.start) as u32,
            &format!("class '{}' not found", class_name),
            &[],
        );
        // ★ LSP: Even without cross-file resolution, register the class-name
        // span as a declare_class entry in the lapper.  This lets mcext's
        // F12 handler pick it up and resolve via project index.
        tracing::info!(target: "crate::lsp", "register_declare_class: {} not resolved cross-file, registering local span {:?} for lapper", class_name, span);
        let uri_str = uri.to_string();
        // Use a synthetic sentinel: target_uri="" and target_span=[0,0].
        // create_lapper will emit DeclareClass for this span; mcext's
        // project-index fallback will resolve the actual definition.
        let mut refs = workspace::WORKSPACE.lsp.declare_class_refs.lock().unwrap();
        refs.entry(uri_str)
            .or_default()
            .push((span, DeclareId::default(), "".to_string(), 0..0));
    }
}
