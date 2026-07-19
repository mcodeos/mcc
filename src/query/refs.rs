// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::ast::ast_semantic::{DeclareId, Span};
use crate::builder::*;
use crate::db::cmie::tables as workspace;
use crate::McURI;

// === pub fn mcb_register_instance_decl( ===
/// 🆕 Register an instance declaration (definition) in the global symbol table
///
/// Called when parsing `TypeName instanceName` in module body.
/// Returns the declare_id which can be used to register references later.
pub fn mcb_register_instance_decl(
    uri: &McURI,
    span: Span,
    name: Option<String>,
    scope: Option<&str>,
) -> Option<DeclareId> {
    let uri_str = uri.as_str();
    let span_clone = span.clone();
    if let Some(n) = name {
        let mut table = workspace::WORKSPACE.global_inst_table.lock().unwrap();
        let id = table.add(uri_str, scope, &n, span_clone);
        tracing::debug!(target: "mcc::lsp", "Registered inst decl: {} scope={:?} at {:?} -> id={:?}", n, scope, span, id);
        Some(id)
    } else {
        None
    }
}

// === pub fn mcb_lookup_instance_decl(uri: &McURI, name: &str, scope: Option<&str>) -> ===
/// 🆕 Look up declare_id by instance name
///
/// Returns the DeclareId for a given instance name, if registered.
pub fn mcb_lookup_instance_decl(uri: &McURI, name: &str, scope: Option<&str>) -> Option<DeclareId> {
    let uri_str = uri.as_str();
    let table = workspace::WORKSPACE.global_inst_table.lock().unwrap();
    table.get(uri_str, scope, name)
}

// === pub fn mcb_register_instance_ref(uri: &McURI, span: Span, decl_id: DeclareId, sc ===
/// 🆕 Register an instance reference in the global symbol table
///
/// Called when an instance name is used elsewhere in the module (e.g., `uC.i2c()`).
/// The reference is linked to the declaration via decl_id.
pub fn mcb_register_instance_ref(uri: &McURI, span: Span, decl_id: DeclareId, scope: Option<&str>) {
    let uri_str = uri.as_str();
    let span_clone = span.clone();
    let mut table = workspace::WORKSPACE.global_inst_table.lock().unwrap();
    table.add_ref(decl_id, uri_str, scope, span);
    tracing::info!(target: "mcc::lsp", "Registered inst ref: decl_id={:?} scope={:?} at {:?}", decl_id, scope, span_clone);
}

// === pub fn mcb_get_refs(name: &str) -> Vec<(String, String, Span)> { ===
/// M6: Get all references for a named declaration.
/// Returns Vec<(uri, scope, span)>.
pub fn mcb_get_refs(name: &str) -> Vec<(String, String, Span)> {
    let table = workspace::WORKSPACE.global_inst_table.lock().unwrap();
    let decl_ids = table.find_decls_by_name(name);
    let mut results = Vec::new();
    for decl_id in &decl_ids {
        results.extend(table.get_refs(*decl_id));
    }
    results
}

// === pub fn mcb_register_declare_class(uri: &McURI, class_name: &str, span: Span) { ===
/// 🆕 Register a class reference for goto-definition
///
/// Called when a class name is used in a declare statement (e.g., `MCU.US513_20_F uC`).
/// Registers the class reference so LSP can jump from the reference to the class definition.
pub fn mcb_register_declare_class(uri: &McURI, class_name: &str, span: Span) {
    // Step 1: Find (class_id, target_uri, target_span) — try global_class_table first
    // Priority: same URI as reference > other URIs (for duplicate class definitions)
    let uri_str = uri.to_string();
    let found = {
        let class_table = workspace::WORKSPACE.global_class_table.lock().unwrap();
        tracing::debug!(target: "mcc::lsp", "  register_declare_class: global_class_table size={}", class_table.len());

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
            tracing::debug!(target: "mcc::lsp", "  register_declare_class: global_class_table miss for '{}'", class_name);
        } else {
            tracing::info!(target: "mcc::lsp", "  register_declare_class: global_class_table hit for '{}'", class_name);
        }
        result
    };

    // Step 2: Try workspace files' global tables if not found above
    let from_mcodes: Option<(DeclareId, String, Span)> = if found.is_none() {
        let binding = workspace::WORKSPACE.mcodes.borrow();
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

    // Step 2.5: Search system library tables for mcode classes (CAP, RES, etc.)
    let from_syslibs: Option<(DeclareId, String, Span)> = if class_info.is_none() {
        let name_str = class_name.to_string();
        let mut result = None;
        for entry in global::mcc_components.borrow().iter() {
            if entry.key().ident.to_string() == name_str {
                result = Some((
                    DeclareId::default(),
                    entry.key().uri.clone(),
                    entry.value().span.clone(),
                ));
                break;
            }
        }
        if result.is_none() {
            for entry in global::mcc_modules.borrow().iter() {
                if entry.key().ident.to_string() == name_str {
                    result = Some((
                        DeclareId::default(),
                        entry.key().uri.clone(),
                        entry.value().span.clone(),
                    ));
                    break;
                }
            }
        }
        if result.is_none() {
            for entry in global::mcc_interfaces.borrow().iter() {
                if entry.key().ident.to_string() == name_str {
                    result = Some((
                        DeclareId::default(),
                        entry.key().uri.clone(),
                        entry.value().span.clone(),
                    ));
                    break;
                }
            }
        }
        if result.is_none() {
            for entry in global::mcc_enums.borrow().iter() {
                if entry.key().ident.to_string() == name_str {
                    let s = entry.value().span;
                    result = Some((
                        DeclareId::default(),
                        entry.key().uri.clone(),
                        s[0] as usize..s[1] as usize,
                    ));
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
        tracing::info!(target: "mcc::lsp", "  register_declare_class: storing ref decl_span={:?} -> class_id={:?} target={}", span_clone, class_id, target_uri);
        let mut refs = workspace::WORKSPACE
            .global_declare_class_refs
            .lock()
            .unwrap();
        refs.entry(uri_str)
            .or_default()
            .push((span, class_id, target_uri, target_span));
        tracing::info!(target: "mcc::lsp", "Registered declare_class: {} at {:?} -> class_id={:?}", class_name, span_clone, class_id);
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
        tracing::info!(target: "mcc::lsp", "register_declare_class: {} not resolved cross-file, registering local span {:?} for lapper", class_name, span);
        let uri_str = uri.to_string();
        // Use a synthetic sentinel: target_uri="" and target_span=[0,0].
        // create_lapper will emit DeclareClass for this span; mcext's
        // project-index fallback will resolve the actual definition.
        let mut refs = workspace::WORKSPACE
            .global_declare_class_refs
            .lock()
            .unwrap();
        refs.entry(uri_str)
            .or_default()
            .push((span, DeclareId::default(), "".to_string(), 0..0));
    }
}
