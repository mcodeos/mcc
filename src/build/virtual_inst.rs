// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Virtual instantiation for non-project single-file views.
//!
//! Strategy (mcd docs-mc 16-export-viz §6):
//! - A file opened outside a project (no project.toml/manifest) that declares
//!   one or more `module`s: each module is instantiated on its own (existing
//!   behaviour).
//! - A file with no module but one or more `component`s / `interface`s: each
//!   unit is "virtually instantiated" by wrapping it in a synthetic module, so
//!   the standard Pass2 + viz pipeline can render the unit standalone.

use crate::build::pass1::canonicalize_project_uri;
use crate::db::cmie::tables as workspace;
use crate::{McIds, McURI};
use std::error::Error;
use std::path::Path;

/// Canonical form of `uri` for workspace-table key comparisons (the loader
/// stores definitions under `canonicalize_project_uri`, so a raw path like
/// `/var/folders/...` must be normalized to match `/private/var/folders/...`).
fn canonical(uri: &McURI) -> String {
    canonicalize_project_uri(uri)
}

/// Modules declared in `uri`, in registration order.
pub fn modules_in_file(uri: &McURI) -> Vec<String> {
    let c = canonical(uri);
    workspace::WORKSPACE
        .modules
        .iter()
        .filter(|e| e.key().uri == c)
        .map(|e| e.key().ident.to_string())
        .collect()
}

/// Components declared in `uri`, in registration order.
pub fn components_in_file(uri: &McURI) -> Vec<String> {
    let c = canonical(uri);
    workspace::WORKSPACE
        .components
        .iter()
        .filter(|e| e.key().uri == c)
        .map(|e| e.key().ident.to_string())
        .collect()
}

/// Interfaces declared in `uri`, in registration order.
pub fn interfaces_in_file(uri: &McURI) -> Vec<String> {
    let c = canonical(uri);
    workspace::WORKSPACE
        .interfaces
        .iter()
        .filter(|e| e.key().uri == c)
        .map(|e| e.key().ident.to_string())
        .collect()
}

/// Resolve the build/viz targets for a file opened outside a project.
///
/// Priority: explicit `top` → all modules in the file → all components in the
/// file → all interfaces in the file.
pub fn resolve_targets(uri: &McURI, top: Option<&str>) -> Result<Vec<String>, String> {
    if let Some(t) = top {
        if !t.trim().is_empty() {
            return Ok(vec![t.to_string()]);
        }
    }
    let mods = modules_in_file(uri);
    if !mods.is_empty() {
        return Ok(mods);
    }
    let comps = components_in_file(uri);
    if !comps.is_empty() {
        return Ok(comps);
    }
    let ifs = interfaces_in_file(uri);
    if !ifs.is_empty() {
        return Ok(ifs);
    }
    Err(format!(
        "no module, component, or interface found in '{}'",
        uri
    ))
}

/// Is `target` a module declared in `uri` (i.e. buildable without synthesis)?
pub fn is_module_in_file(target: &str, uri: &McURI) -> bool {
    modules_in_file(uri).iter().any(|m| m == target)
}

/// Build `target` to a module instance tree. Modules build directly; a
/// component or interface is wrapped in a synthetic module first.
pub fn virtual_build(
    target: &str,
    uri: &McURI,
) -> Result<crate::build::pass2::MccProjectTree, Box<dyn Error>> {
    if is_module_in_file(target, uri) {
        return crate::mcc_build(&McIds::from(target), uri);
    }
    let mod_name = install_synthetic_view(target, uri)?;
    crate::mcc_build(&McIds::from(mod_name.as_str()), uri)
}

/// Like [`virtual_build`] but returns the flattened instance table too.
pub fn virtual_build_flat(
    target: &str,
    uri: &McURI,
    start_id: u32,
) -> Result<
    (
        crate::build::pass2::MccProjectTree,
        crate::instant::insttab::InstTable,
    ),
    Box<dyn Error>,
> {
    if is_module_in_file(target, uri) {
        return crate::mcc_build_flat(&McIds::from(target), uri, start_id);
    }
    let mod_name = install_synthetic_view(target, uri)?;
    crate::mcc_build_flat(&McIds::from(mod_name.as_str()), uri, start_id)
}

/// Install a synthetic module that wraps `target` (a component or interface)
/// and return the synthetic module name.
///
/// The synthetic module is appended to the file's own content and the combined
/// source is reloaded under the same URI, so the wrapped unit stays visible
/// (same-file P3 resolution) and no cross-file duplicate (E5001) is reported.
fn install_synthetic_view(target: &str, uri: &McURI) -> Result<String, Box<dyn Error>> {
    let original = std::fs::read_to_string(Path::new(uri))
        .map_err(|e| format!("virtual instantiation: cannot read '{}': {e}", uri))?;
    let mod_name = synthetic_module_name(target);
    let synthetic = if interfaces_in_file(uri).iter().any(|i| i == target) {
        synthesize_interface_module(target, uri)?
    } else {
        format!("\nmodule {mod_name}\n{{\n    {target} u_1\n}}\n")
    };
    let combined = format!("{original}\n{synthetic}");
    crate::mcc_load_from_string(uri, &combined);
    Ok(mod_name)
}

fn synthetic_module_name(target: &str) -> String {
    // `VIRT_` + alnum/underscore only, so the generated name is always a valid
    // identifier even when the source class has dots (e.g. USB.MINIB).
    let clean: String = target
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect();
    format!("VIRT_{clean}")
}

/// Synthetic module that views an interface alone: the interface members
/// become module `io` ports, so viz draws the interface's boundary ports.
fn synthesize_interface_module(target: &str, uri: &McURI) -> Result<String, Box<dyn Error>> {
    let member_names: Vec<String> = crate::get_kind_def(2, &McIds::from(target), uri)
        .and_then(|cmie| match cmie {
            crate::McCMIE::Interface(iface) => Some(iface.pins.member_names()),
            _ => None,
        })
        .unwrap_or_default();
    let ports: Vec<String> = member_names
        .iter()
        .map(|m| {
            let clean: String = m
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '_' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            format!("io {clean}")
        })
        .collect();
    let mod_name = synthetic_module_name(target);
    let port_list = ports.join(", ");
    if port_list.is_empty() {
        Ok(format!("\nmodule {mod_name}\n{{\n}}\n"))
    } else {
        Ok(format!("\nmodule {mod_name}({port_list})\n{{\n}}\n"))
    }
}
