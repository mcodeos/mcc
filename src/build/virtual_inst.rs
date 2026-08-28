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

/// Prepare the graph of a virtually-instantiated component/interface for
/// rendering (mcd docs-mc 16-export-viz §6):
///
/// - Switch to the device pipeline (`LayerStyle::Device`) so the wrapped unit
///   renders as an IC instead of a block-diagram stub (root-block).
/// - Suppress the fabricated instance name (`u_1`) on the wrapped box, so the
///   view shows only the class name and the pins.
/// - Synthesize one entry point per physical pin (evenly spread on the left /
///   right edges), so the renderer draws a stub + pin number + pin name + io
///   marker per pin instead of an NC cross (a virtual view never wires pins).
///
/// Modules (real top-level `module`s) keep the default block rendering; this
/// helper is only applied to component/interface virtual targets.
pub fn prepare_virtual_graph(
    mut graph: crate::vector::graph::McVecGraph,
    target: &str,
) -> crate::vector::graph::McVecGraph {
    graph.layer_style = crate::vector::graph::LayerStyle::Device;
    let mod_name = synthetic_module_name(target);
    for b in &mut graph.boxes {
        if b.class_name == target || b.name == mod_name || b.class_name == mod_name {
            b.suppress_instance_name = true;
            synthesize_pin_entry_points(b);
        }
    }
    graph
}

/// Give every physical pin of a box its own entry point (stub) so the virtual
/// component view draws pin number + name + io marker on each pin.
///
/// Pin placement (mcd docs-mc 16-export-viz §6):
/// - When the component declares a `layout` attribute, pins are assigned to the
///   edges it specifies (`left`/`right`/`top`/`bottom`), in declaration order.
/// - Otherwise pins are arranged **counterclockwise** around the box on the
///   left/right columns only (pin 1 top-left, left top→bottom, right
///   bottom→top), ordered by **numeric pin number** (not alphabetical).
fn synthesize_pin_entry_points(b: &mut crate::vector::graph::McVecBox) {
    use crate::vector::graph::{EntryPoint, EntrySide};
    let n = b.pins.len();
    if n == 0 {
        return;
    }
    let pin_ids: Vec<String> = b.pins.iter().map(|p| p.pin_id.clone()).collect();

    if let Some(layout) = crate::vector::graph::fromblock::component_pin_layout(&b.class_name) {
        assign_by_layout(b, &layout, &pin_ids);
        return;
    }

    // Arrangement order follows the numeric pin number, never the string sort
    // (1, 2, ..., 9, 10, 11, ...; not 1, 10, 11, ..., 2, 20, ...).
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&i, &j| {
        let na = b.pins[i].pin_id.parse::<u32>().ok();
        let nb = b.pins[j].pin_id.parse::<u32>().ok();
        match (na, nb) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => b.pins[i].pin_id.cmp(&b.pins[j].pin_id),
        }
    });

    // Default: only the left/right columns, counterclockwise (DIP convention):
    // pin 1 at top-left, left side reads top→bottom, right side reads
    // bottom→top. No pins on the top/bottom edges.
    let left_count = (n + 1) / 2;
    let right_count = n - left_count;
    for (i, &pin_i) in order.iter().enumerate() {
        let p = &b.pins[pin_i];
        let (side, rank, count) = if i < left_count {
            (EntrySide::Left, i, left_count)
        } else {
            (EntrySide::Right, i - left_count, right_count)
        };
        let offset = match side {
            EntrySide::Left => (rank as f64 + 1.0) / (count as f64 + 1.0),
            // Right column continues counterclockwise from the bottom-left:
            // the first right pin sits at the bottom, the last at the top.
            EntrySide::Right => 1.0 - (rank as f64 + 1.0) / (count as f64 + 1.0),
            EntrySide::Top | EntrySide::Bottom => unreachable!(),
        };
        b.entry_points.push(EntryPoint {
            pin_id: p.id,
            pin_name: p.description.clone(),
            side,
            offset,
        });
    }
}

/// Assign pins to edges according to the component's `layout` attribute.
/// Pins missing from the layout fall back to the left edge.
fn assign_by_layout(
    b: &mut crate::vector::graph::McVecBox,
    layout: &crate::vector::graph::boxdef::PinLayout,
    pin_ids: &[String],
) {
    use crate::vector::graph::{EntryPoint, EntrySide};
    let mut used = std::collections::HashSet::new();
    let mut push = |side: EntrySide, ids: &[String], acc: &mut Vec<EntryPoint>| {
        let mut rank = 0usize;
        let mut count = 0usize;
        for pid in ids {
            if pin_ids.iter().any(|p| p == pid) {
                count += 1;
            }
        }
        if count == 0 {
            return;
        }
        for pid in ids {
            if let Some(p) = b.pins.iter().find(|p| &p.pin_id == pid) {
                used.insert(p.id);
                let offset = (rank as f64 + 1.0) / (count as f64 + 1.0);
                acc.push(EntryPoint {
                    pin_id: p.id,
                    pin_name: p.description.clone(),
                    side,
                    offset,
                });
                rank += 1;
            }
        }
    };

    let mut eps = Vec::new();
    push(EntrySide::Left, &layout.left, &mut eps);
    push(EntrySide::Right, &layout.right, &mut eps);
    push(EntrySide::Top, &layout.top, &mut eps);
    push(EntrySide::Bottom, &layout.bottom, &mut eps);

    // Unassigned pins: spread on the left edge below the declared ones.
    let mut rank = 0usize;
    let unassigned: Vec<&crate::vector::graph::boxdef::BoxPin> =
        b.pins.iter().filter(|p| !used.contains(&p.id)).collect();
    let count = unassigned.len();
    for p in unassigned {
        let offset = 1.0 - (rank as f64 + 1.0) / (count as f64 + 1.0);
        eps.push(EntryPoint {
            pin_id: p.id,
            pin_name: p.description.clone(),
            side: EntrySide::Left,
            offset,
        });
        rank += 1;
    }

    b.entry_points = eps;
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
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
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
