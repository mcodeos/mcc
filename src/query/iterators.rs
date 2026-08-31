// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::db::cmie::tables as workspace;
use crate::db::infra::init::iter_interfaces;
use crate::db::infra::init::mcb_canonicalize_uri;
use crate::db::infra::init::uri_equivalent;
use crate::McURI;

// === pub fn mcb_module_count() -> usize { ===
/// Get the number of all modules (for debugging)
pub fn mcb_module_count() -> usize {
    workspace::WORKSPACE.modules.len()
}

// === pub fn mcb_get_first_module_name() -> Option<String> { ===
/// Get the name of the first module (for auto-detecting the top-level module)
pub fn mcb_get_first_module_name() -> Option<String> {
    workspace::WORKSPACE
        .modules
        .iter()
        .next()
        .map(|entry| entry.key().ident.to_string())
}

// === pub fn mcb_get_module_name_by_uri(uri: &McURI) -> Option<String> { ===
/// Get module name by matching URI suffix
pub fn mcb_get_module_name_by_uri(uri: &McURI) -> Option<String> {
    let canonical = mcb_canonicalize_uri(uri);
    workspace::WORKSPACE
        .modules
        .iter()
        .find(|entry| uri_equivalent(&entry.key().uri.as_uri(), uri.as_str(), &canonical))
        .map(|entry| entry.key().ident.to_string())
}

// === pub fn mcb_component_count() -> usize { ===
/// Get the number of loaded components
pub fn mcb_component_count() -> usize {
    workspace::WORKSPACE.components.len()
}

// === pub fn mcb_get_modules_in_file(uri: &McURI) -> Vec<String> { ===
/// Get all module names in a specific file (by URI)
///
/// Key comparison uses [`uri_equivalent`] (not raw `==`): workspace keys are
/// canonicalized (resolving `/tmp`→`/private/tmp` etc.), so a caller's raw
/// path must be tested bidirectionally or real modules under a symlinked path
/// would be reported as absent — misclassifying them as virtual targets.
pub fn mcb_get_modules_in_file(uri: &McURI) -> Vec<String> {
    let canonical = mcb_canonicalize_uri(uri);
    workspace::WORKSPACE
        .modules
        .iter()
        .filter(|entry| uri_equivalent(&entry.key().uri.as_uri(), uri.as_str(), &canonical))
        .map(|entry| entry.key().ident.to_string())
        .collect()
}

// === pub fn mcb_interface_count() -> usize { ===
/// Number of distinct interface definitions across workspace and system lib
/// (deduplicated by identity — a def in both tables counts once).
pub fn mcb_interface_count() -> usize {
    crate::definition_space().all_interfaces().len()
}

// === pub fn mcb_iter_modules() -> Vec<(String, String)> { ===
/// Iterate all registered project module definitions, return (name, uri) pairs.
pub fn mcb_iter_modules() -> Vec<(String, String)> {
    workspace::WORKSPACE
        .modules
        .iter()
        .map(|entry| (entry.key().ident.to_string(), entry.key().uri.to_string()))
        .collect()
}

// === pub fn mcb_iter_modules_with_span() -> Vec<(String, String, [usize; 2])> { ===
/// Like `mcb_iter_modules` but includes source span for LSP goto-def.
pub fn mcb_iter_modules_with_span() -> Vec<(String, String, [usize; 2])> {
    workspace::WORKSPACE
        .modules
        .iter()
        .map(|entry| {
            let span = &entry.value().span;
            (
                entry.key().ident.to_string(),
                entry.key().uri.to_string(),
                [span.start, span.end],
            )
        })
        .collect()
}

// === pub fn mcb_iter_components() -> Vec<(String, String)> { ===
/// Iterate all registered component definitions (including project and system lib).
pub fn mcb_iter_components() -> Vec<(String, String)> {
    let mut items: Vec<(String, String)> = crate::definition_space()
        .all_components()
        .into_iter()
        .map(|(sn, _)| (sn.ident.to_string(), sn.uri.to_string()))
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    items
}

// === pub fn mcb_iter_components_with_span() -> Vec<(String, String, [usize; 2])> { ===
/// Like `mcb_iter_components` but includes source span for LSP goto-def.
pub fn mcb_iter_components_with_span() -> Vec<(String, String, [usize; 2])> {
    let mut items: Vec<_> = crate::definition_space()
        .all_components()
        .into_iter()
        .map(|(sn, comp)| {
            let span = &comp.span;
            (
                sn.ident.to_string(),
                sn.uri.to_string(),
                [span.start, span.end],
            )
        })
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    items
}

// === pub fn mcb_iter_interfaces() -> Vec<(String, String)> { ===
/// Iterate all registered project interface definitions.
pub fn mcb_iter_interfaces() -> Vec<(String, String)> {
    let mut items: Vec<(String, String)> = iter_interfaces()
        .iter()
        .map(|(space, _)| (space.ident.to_string(), space.uri.to_string()))
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    items
}

// === pub fn mcb_iter_interfaces_with_span() -> Vec<(String, String, [usize; 2])> { ===
/// Like `mcb_iter_interfaces` but includes source span for LSP goto-def.
pub fn mcb_iter_interfaces_with_span() -> Vec<(String, String, [usize; 2])> {
    let mut items: Vec<_> = iter_interfaces()
        .iter()
        .map(|(space, iface)| {
            let span = &iface.span;
            (
                space.ident.to_string(),
                space.uri.to_string(),
                [span.start, span.end],
            )
        })
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    items
}

// === pub fn mcb_iter_enums() -> Vec<(String, String)> { ===
/// Iterate all registered enum definitions (both workspace and system library).
pub fn mcb_iter_enums() -> Vec<(String, String)> {
    let mut items: Vec<(String, String)> = crate::definition_space()
        .all_enums()
        .into_iter()
        .map(|(sn, _)| (sn.ident.to_string(), sn.uri.to_string()))
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    items
}

// === pub fn mcb_iter_enums_with_span() -> Vec<(String, String, [usize; 2])> { ===
/// Same as `mcb_iter_enums`, but also returns the class span
/// `[start, end)` of the `enum PKG { ... }` head — needed by LSP
/// gotodef to know where to land when jumping to the class itself.
/// Includes both workspace and system library enums (deduped, §12.4 rule 1).
pub fn mcb_iter_enums_with_span() -> Vec<(String, String, [usize; 2])> {
    let mut items: Vec<(String, String, [usize; 2])> = crate::definition_space()
        .all_enums()
        .into_iter()
        .map(|(sn, def)| {
            let s = def.span;
            (
                sn.ident.to_string(),
                sn.uri.to_string(),
                [s[0] as usize, s[1] as usize],
            )
        })
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    items
}

// === pub fn mcb_iter_enum_values() -> Vec<(String, String, String, [u32; 2])> { ===
/// Iterate all enum value rows project-wide (both workspace and system library).
/// Returns `Vec<(class, value, uri, [u32;2])>` sorted by class then value.
pub fn mcb_iter_enum_values() -> Vec<(String, String, String, [u32; 2])> {
    let mut items: Vec<(String, String, String, [u32; 2])> = Vec::new();

    for (sn, def) in crate::definition_space().all_enums() {
        let class = sn.ident.to_string();
        let uri = sn.uri.to_string();
        for v in def.values.iter() {
            items.push((class.clone(), v.name.to_string(), uri.clone(), v.span));
        }
    }

    items.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    items
}

// === pub fn mcb_iter_ports() -> Vec<(String, String, String, String)> { ===
/// Iterate all module port definitions (ps/io/in/out).
/// Returns Vec of (port_name, iotype, module_name, uri).
pub fn mcb_iter_ports() -> Vec<(String, String, String, String)> {
    use crate::semantic::common::IOType;

    let mut ports: Vec<(String, String, String, String)> = Vec::new();

    for entry in workspace::WORKSPACE.modules.iter() {
        let module_name = entry.key().ident.to_string();
        let uri = entry.key().uri.to_string();
        let module = entry.value();

        for (name, iotype) in module.insts.iter_ports() {
            let io_name = match iotype {
                IOType::Power => "power".to_string(),
                IOType::In => "input".to_string(),
                IOType::Out => "output".to_string(),
                IOType::InOut => "inout".to_string(),
                IOType::Analog => "analog".to_string(),
                IOType::Label => "label".to_string(),
                IOType::Return | IOType::NonCon | IOType::None => continue, // Skip non-port declarations
            };
            ports.push((name.to_string(), io_name, module_name.clone(), uri.clone()));
        }
    }

    ports.sort_by(|a, b| a.0.cmp(&b.0));
    ports
}
