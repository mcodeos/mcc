// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::db::cmie::tables as workspace;
use crate::db::infra::global;
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
    workspace::WORKSPACE
        .modules
        .iter()
        .find(|entry| entry.key().uri.ends_with(uri) || uri.ends_with(&entry.key().uri))
        .map(|entry| entry.key().ident.to_string())
}

// === pub fn mcb_component_count() -> usize { ===
/// Get the number of loaded components
pub fn mcb_component_count() -> usize {
    workspace::WORKSPACE.components.len()
}

// === pub fn mcb_get_modules_in_file(uri: &McURI) -> Vec<String> { ===
/// Get all module names in a specific file (by URI)
pub fn mcb_get_modules_in_file(uri: &McURI) -> Vec<String> {
    workspace::WORKSPACE
        .modules
        .iter()
        .filter(|entry| entry.key().uri == *uri)
        .map(|entry| entry.key().ident.to_string())
        .collect()
}

// === pub fn mcb_interface_count() -> usize { ===
pub fn mcb_interface_count() -> usize {
    workspace::WORKSPACE.interfaces.len() + global::mcc_interfaces.len()
}

// === pub fn mcb_iter_modules() -> Vec<(String, String)> { ===
/// Iterate all registered project module definitions, return (name, uri) pairs.
pub fn mcb_iter_modules() -> Vec<(String, String)> {
    workspace::WORKSPACE
        .modules
        .iter()
        .map(|entry| (entry.key().ident.to_string(), entry.key().uri.clone()))
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
                entry.key().uri.clone(),
                [span.start, span.end],
            )
        })
        .collect()
}

// === pub fn mcb_iter_components() -> Vec<(String, String)> { ===
/// Iterate all registered component definitions (including project and system lib).
pub fn mcb_iter_components() -> Vec<(String, String)> {
    let mut items: Vec<(String, String)> = workspace::WORKSPACE
        .components
        .iter()
        .chain(global::mcc_components.iter())
        .map(|entry| (entry.key().ident.to_string(), entry.key().uri.clone()))
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    items
}

// === pub fn mcb_iter_components_with_span() -> Vec<(String, String, [usize; 2])> { ===
/// Like `mcb_iter_components` but includes source span for LSP goto-def.
pub fn mcb_iter_components_with_span() -> Vec<(String, String, [usize; 2])> {
    let mut items: Vec<_> = workspace::WORKSPACE
        .components
        .iter()
        .chain(global::mcc_components.iter())
        .map(|entry| {
            let span = &entry.value().span;
            (
                entry.key().ident.to_string(),
                entry.key().uri.clone(),
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
    let mut items: Vec<(String, String)> = workspace::WORKSPACE
        .interfaces
        .iter()
        .chain(global::mcc_interfaces.iter())
        .map(|entry| (entry.key().ident.to_string(), entry.key().uri.clone()))
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    items
}

// === pub fn mcb_iter_interfaces_with_span() -> Vec<(String, String, [usize; 2])> { ===
/// Like `mcb_iter_interfaces` but includes source span for LSP goto-def.
pub fn mcb_iter_interfaces_with_span() -> Vec<(String, String, [usize; 2])> {
    let mut items: Vec<_> = workspace::WORKSPACE
        .interfaces
        .iter()
        .chain(global::mcc_interfaces.iter())
        .map(|entry| {
            let span = &entry.value().span;
            (
                entry.key().ident.to_string(),
                entry.key().uri.clone(),
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
    let mut items: Vec<(String, String)> = Vec::new();

    // Workspace enums (project files)
    for entry in workspace::WORKSPACE.enums.iter() {
        items.push((entry.key().ident.to_string(), entry.key().uri.clone()));
    }

    // System library enums (e.g. enum PKG in mcode/package.mc)
    for entry in global::mcc_enums.iter() {
        items.push((entry.key().ident.to_string(), entry.key().uri.clone()));
    }

    items.sort_by(|a, b| a.0.cmp(&b.0));
    items
}

// === pub fn mcb_iter_enums_with_span() -> Vec<(String, String, [usize; 2])> { ===
/// Same as `mcb_iter_enums`, but also returns the class span
/// `[start, end)` of the `enum PKG { ... }` head — needed by LSP
/// gotodef to know where to land when jumping to the class itself.
/// Includes both workspace and system library enums.
pub fn mcb_iter_enums_with_span() -> Vec<(String, String, [usize; 2])> {
    let mut items: Vec<(String, String, [usize; 2])> = Vec::new();

    // Workspace enums (project files)
    let enums_guard = &workspace::WORKSPACE.enums;
    for entry in enums_guard.iter() {
        let s = entry.value().span;
        items.push((
            entry.key().ident.to_string(),
            entry.key().uri.clone(),
            [s[0] as usize, s[1] as usize],
        ));
    }

    // System library enums (e.g. enum PKG in mcode/package.mc)
    let sys_enums_guard = &global::mcc_enums;
    for entry in sys_enums_guard.iter() {
        let s = entry.value().span;
        items.push((
            entry.key().ident.to_string(),
            entry.key().uri.clone(),
            [s[0] as usize, s[1] as usize],
        ));
    }

    items.sort_by(|a, b| a.0.cmp(&b.0));
    items
}

// === pub fn mcb_iter_enum_values() -> Vec<(String, String, String, [u32; 2])> { ===
/// Iterate all enum value rows project-wide (both workspace and system library).
/// Returns `Vec<(class, value, uri, [u32;2])>` sorted by class then value.
pub fn mcb_iter_enum_values() -> Vec<(String, String, String, [u32; 2])> {
    let mut items: Vec<(String, String, String, [u32; 2])> = Vec::new();

    // Iterate workspace enums (project files)
    let enums_guard = &workspace::WORKSPACE.enums;
    for entry in enums_guard.iter() {
        let class = entry.key().ident.to_string();
        let uri = entry.key().uri.clone();
        let enum_def = entry.value();
        for v in enum_def.values.iter() {
            let value_name = v.name.to_string();
            items.push((class.clone(), value_name, uri.clone(), v.span));
        }
    }

    // Iterate system library enums (e.g. enum PKG in mcode/package.mc)
    let sys_enums_guard = &global::mcc_enums;
    for entry in sys_enums_guard.iter() {
        let class = entry.key().ident.to_string();
        let uri = entry.key().uri.clone();
        let enum_def = entry.value();
        for v in enum_def.values.iter() {
            let value_name = v.name.to_string();
            items.push((class.clone(), value_name, uri.clone(), v.span));
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
        let uri = entry.key().uri.clone();
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
