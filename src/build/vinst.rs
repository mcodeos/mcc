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
use crate::{McIds, McURI};
use std::collections::HashSet;
use std::error::Error;
use std::path::Path;
use std::sync::{OnceLock, RwLock};

/// Single source of the fabricated instance name inside a synthetic wrapper
/// module (`module VIRT_<T> { <T> <INSTANCE> }`). Generation
/// ([`synthetic_module_text`]) and identification ([`prepare_virtual_graph`])
/// both read this constant — nothing downstream matches on the literal
/// elsewhere, and the name is only ever treated as synthetic when it sits
/// inside the wrapper module's own scope (an exact path under
/// `VIRT_<T>.<INSTANCE>`), never globally.
pub const SYNTHETIC_INSTANCE: &str = "u_1";

/// Synthetic wrapper module names installed by [`install_synthetic_view`] /
/// [`install_synthetic_views`].
///
/// These are fabricated `module VIRT_<T>` wrappers, not real user modules.
/// [`resolve_targets`] must never hand one back as a build target: on a hot
/// server the wrappers stay registered after the first `build.viz`, so a second
/// resolution that reads the file's modules verbatim would re-target the
/// wrappers themselves and render block-diagram stubs (`u_1` visible) instead
/// of the wrapped unit as a device IC. Tracking the names we ourselves
/// installed (not matching on the `VIRT_` prefix) keeps a genuine user module
/// that happens to be named `VIRT_*` buildable.
static SYNTHETIC_MODULES: OnceLock<RwLock<HashSet<String>>> = OnceLock::new();

fn synthetic_modules() -> &'static RwLock<HashSet<String>> {
    SYNTHETIC_MODULES.get_or_init(|| RwLock::new(HashSet::new()))
}

fn record_synthetic_module(name: &str) {
    synthetic_modules()
        .write()
        .unwrap()
        .insert(name.to_string());
}

/// Is `name` a synthetic wrapper module fabricated by this process?
pub fn is_synthetic_module(name: &str) -> bool {
    synthetic_modules().read().unwrap().contains(name)
}

// Whether the current thread is reloading a synthetic view
// ([`install_synthetic_views`] / [`install_synthetic_view`]).
//
// The reload runs the C parser + PostParse validation synchronously, and the
// parser flags the fabricated interface wrapper's empty body with
// `MCD_W1105_EMPTY_BODY` (2115). An empty body is that wrapper's normal,
// intended shape — its `io` ports render boundary pins and there is nothing to
// declare inside — so the parser diagnostic is suppressed while this is set
// (see `db/infra/mc_code.rs`). Mirrors the MODULE_STUB carve-out for synthetic
// wrappers in `semantic/validation/conds.rs`.
//
// Thread-local, not a process global: a load on another thread (a concurrent
// RPC request) must never inherit the suppression.
std::thread_local! {
    static LOADING_SYNTHETIC_VIEW: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Is the current thread reloading a synthetic view?
pub(crate) fn is_loading_synthetic_view() -> bool {
    LOADING_SYNTHETIC_VIEW.with(|f| f.get())
}

/// RAII guard that marks the current thread as reloading a synthetic view for
/// the duration of the reload. Dropped when the reload's scope ends, so the
/// flag cannot leak if `mcc_load_from_string` panics.
pub(crate) struct SyntheticViewLoadGuard;

impl SyntheticViewLoadGuard {
    pub(crate) fn new() -> Self {
        LOADING_SYNTHETIC_VIEW.with(|f| f.set(true));
        SyntheticViewLoadGuard
    }
}

impl Drop for SyntheticViewLoadGuard {
    fn drop(&mut self) {
        LOADING_SYNTHETIC_VIEW.with(|f| f.set(false));
    }
}

/// Re-apply synthetic markers to a freshly-flattened instance table.
///
/// `InstTable::from_module_inst` builds a new table without the synthetic flags
/// that `mcb_pass2_flat_with` attached during the virtual build, so a standalone
/// component/interface view's unwired ports would be re-flagged by the
/// electrical net checks (floating-bidirectional, etc.). Mark every entry under
/// each recorded wrapper module as synthetic again, mirroring
/// `mark_synthetic_by_path_prefix` at the generation site.
pub fn mark_synthetic_flat_entries(table: &mut crate::instant::insttab::InstTable) {
    for name in synthetic_modules().read().unwrap().iter() {
        table.mark_synthetic_by_path_prefix(name);
    }
}

/// Canonical form of `uri` for workspace-table key comparisons (the loader
/// stores definitions under `canonicalize_project_uri`, so a raw path like
/// `/var/folders/...` must be normalized to match `/private/var/folders/...`).
fn canonical(uri: &McURI) -> String {
    canonicalize_project_uri(uri)
}

/// Modules declared in `uri`, in source-declaration order.
///
/// The registry views are sorted by (uri, ident), not source order, so a raw
/// read hands targets back in definition order only after a span sort. Sort
/// by the definition's declaration span instead.
pub fn modules_in_file(uri: &McURI) -> Vec<String> {
    let c = canonical(uri);
    let mut defs: Vec<(usize, String)> = crate::definition_space()
        .workspace_modules()
        .into_iter()
        .filter(|(sn, _)| sn.uri.as_uri().as_ref() == c)
        .map(|(sn, m)| (m.span.start, sn.ident.to_string()))
        .collect();
    defs.sort_by_key(|(pos, _)| *pos);
    defs.into_iter().map(|(_, name)| name).collect()
}

/// Components declared in `uri`, in source-declaration order.
pub fn components_in_file(uri: &McURI) -> Vec<String> {
    let c = canonical(uri);
    let mut defs: Vec<(usize, String)> = crate::definition_space()
        .workspace_components()
        .into_iter()
        .filter(|(sn, _)| sn.uri.as_uri().as_ref() == c)
        .map(|(sn, comp)| (comp.span.start, sn.ident.to_string()))
        .collect();
    defs.sort_by_key(|(pos, _)| *pos);
    defs.into_iter().map(|(_, name)| name).collect()
}

/// Interfaces declared in `uri`, in source-declaration order.
pub fn interfaces_in_file(uri: &McURI) -> Vec<String> {
    let c = canonical(uri);
    let mut defs: Vec<(usize, String)> = crate::definition_space()
        .workspace_interfaces()
        .into_iter()
        .filter(|(sn, _)| sn.uri.as_uri().as_ref() == c)
        .map(|(sn, iface)| (iface.span.start, sn.ident.to_string()))
        .collect();
    defs.sort_by_key(|(pos, _)| *pos);
    defs.into_iter().map(|(_, name)| name).collect()
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
    // Exclude the synthetic wrapper modules (`module VIRT_<T>`) installed by a
    // previous build.viz: they are fabrication, not user modules, and must not
    // shadow the real components/interfaces they wrap. See [`SYNTHETIC_MODULES`].
    let mods: Vec<String> = modules_in_file(uri)
        .into_iter()
        .filter(|m| !is_synthetic_module(m))
        .collect();
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
    let mod_name = ensure_synthetic_view(target, uri)?;
    crate::mcc_build(&McIds::from(mod_name.as_str()), uri)
}

/// Like [`virtual_build`], but also returns the Phase C companion arena +
/// instance store and the Phase D frozen string net-table store
/// (`McModuleInst` never carries `NetPoint`, so consumers of the tree-level
/// string nets — e.g. the `mcc build` net-check path that re-flattens the
/// tree — read the per-module tables from the store). The arena + store let
/// the caller build a [`TreeView`](crate::instant::inststore::TreeView)
/// for store-backed children traversal (Phase C S3-D: the tree's Vec fields
/// are gone).
pub fn virtual_build_with_nets(
    target: &str,
    uri: &McURI,
) -> Result<
    (
        crate::build::pass2::MccProjectTree,
        crate::instant::arena::NodeArena,
        crate::instant::inststore::InstanceStore,
        crate::instant::nettab::NetTableStore,
    ),
    Box<dyn Error>,
> {
    if is_module_in_file(target, uri) {
        return crate::mcc_build_with_arena(&McIds::from(target), uri);
    }
    let mod_name = ensure_synthetic_view(target, uri)?;
    crate::mcc_build_with_arena(&McIds::from(mod_name.as_str()), uri)
}

/// Like [`virtual_build`] but returns the flattened instance table plus the
/// Phase C companion arena + instance store.
///
/// When `target` is a component/interface wrapped in a synthetic module, the
/// returned table's wrapper-module and wrapped-instance entries are marked
/// `synthetic` (via the module name this function itself generated), so
/// downstream build/diagnostic layers can distinguish the fabricated wrapper
/// from real user modules and instances without matching on the `VIRT_`/`u_1`
/// names. Phase C S3-D: the caller needs the arena + store to build a
/// [`TreeView`](crate::instant::inststore::TreeView) for any children walk
/// (the tree's Vec fields are gone).
pub fn virtual_build_flat(
    target: &str,
    uri: &McURI,
    start_id: u32,
) -> Result<
    (
        crate::build::pass2::MccProjectTree,
        crate::instant::insttab::InstTable,
        crate::instant::arena::NodeArena,
        crate::instant::inststore::InstanceStore,
    ),
    Box<dyn Error>,
> {
    if is_module_in_file(target, uri) {
        return crate::mcc_build_flat_with_arena(&McIds::from(target), uri, start_id);
    }
    let mod_name = ensure_synthetic_view(target, uri)?;
    // Build through `mcb_pass2_flat_with` so the wrapper module is flagged
    // `synthetic` BEFORE the electrical net checks run — otherwise the checks
    // report E4112/E4116 on the unwired standalone component/interface view.
    let canonical_uri: crate::McURI = crate::db::infra::init::mcb_canonicalize_uri(uri).into();
    let ident = crate::McIds::from(mod_name.as_str());
    let (tree, table, arena, store) = crate::build::pass2::mcb_pass2_flat_with(
        &crate::McSpaceName::new(&ident, canonical_uri),
        start_id,
        Some(&mod_name),
    )?;
    Ok((tree, table, arena, store))
}

/// Return the synthetic module name that wraps `target` (a component/interface),
/// installing it only if it is not already present.
///
/// Installs re-parse the whole file, so when a previous call (e.g. a batch
/// [`install_synthetic_views`]) already appended the wrapper, reuse it instead
/// of reloading the source again.
fn ensure_synthetic_view(target: &str, uri: &McURI) -> Result<String, Box<dyn Error>> {
    let mod_name = synthetic_module_name(target);
    if is_module_in_file(&mod_name, uri) {
        return Ok(mod_name);
    }
    install_synthetic_view(target, uri)
}

/// Append a synthetic wrapper module for every component/interface target at
/// once and reload the file a single time.
///
/// The per-target fallback ([`install_synthetic_view`]) reads the whole file
/// from disk and re-parses it for each target, so a component-only library
/// (e.g. `mclibs/digital/74ahc.mc`, 42 parts) was re-parsed N times — O(n²) on
/// the number of parts. Installing all wrappers in one reload makes build.viz /
/// `build --viz` over a component library O(n) instead.
///
/// Real module targets are skipped (they build directly). Returns the number of
/// wrappers installed (0 when every target already had one).
pub fn install_synthetic_views(targets: &[String], uri: &McURI) -> Result<usize, Box<dyn Error>> {
    let missing: Vec<String> = targets
        .iter()
        .filter(|t| {
            !is_module_in_file(t, uri) && !is_module_in_file(&synthetic_module_name(t), uri)
        })
        .cloned()
        .collect();
    if missing.is_empty() {
        return Ok(0);
    }
    let original = std::fs::read_to_string(Path::new(uri))
        .map_err(|e| format!("virtual instantiation: cannot read '{}': {e}", uri))?;
    let mut combined = original;
    for t in &missing {
        combined.push_str(&synthetic_module_text(t, uri)?);
        record_synthetic_module(&synthetic_module_name(t));
    }
    // Guard the reload so parser diagnostics that can only come from the
    // fabricated wrappers (e.g. 2115 empty body) are suppressed.
    let _guard = SyntheticViewLoadGuard::new();
    crate::mcc_load_from_string(uri, &combined);
    Ok(missing.len())
}

/// Prepare the graph of a virtually-instantiated component/interface for
/// rendering (mcd docs-mc 16-export-viz §6):
///
/// - Switch to the device pipeline (`LayerStyle::Device`) so the wrapped unit
///   renders as an IC instead of a block-diagram stub (root-block).
/// - Suppress the fabricated instance name (`u_1`) on the wrapped box, so the
///   view shows only the class name and the pins.
/// - Rename the root layer to the real class name: the view's title and
///   breadcrumb come from `graph.name` (viz api), which would otherwise show the
///   synthetic wrapper name (`VIRT_<T>`) — the wrapper exists only to build,
///   never to display.
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
    graph.name = target.to_string();
    let mod_name = synthetic_module_name(target);
    let mod_scope = format!("{mod_name}.");
    for b in &mut graph.boxes {
        // Identify the fabricated wrapper by its own scope — the module box
        // itself, plus the single generated instance inside it — never by
        // matching the VIRT_/u_1 names against arbitrary boxes. A user's `u_1`
        // outside this module, or a user box coincidentally named VIRT_*, is
        // left untouched. (An interface wrapper has no instance box: its
        // boundary ports render via the Device layer style set above.)
        let is_module_box = b.inst_path == mod_name;
        let is_wrapped_unit = b.inst_path.starts_with(&mod_scope) && b.class_name == target;
        if is_module_box || is_wrapped_unit {
            b.synthetic = true;
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
    let synthetic = synthetic_module_text(target, uri)?;
    let combined = format!("{original}\n{synthetic}");
    // Record the fabricated module before reloading: mcc_load_from_string runs
    // the PostParse validation synchronously, so the marker must be in place
    // for checks that must not flag synthetic wrappers (e.g. MODULE_STUB).
    let mod_name = synthetic_module_name(target);
    record_synthetic_module(&mod_name);
    // Guard the reload so parser diagnostics that can only come from the
    // fabricated wrapper (e.g. 2115 empty body) are suppressed.
    let _guard = SyntheticViewLoadGuard::new();
    crate::mcc_load_from_string(uri, &combined);
    Ok(mod_name)
}

/// Source text of the synthetic module that wraps `target` (component →
/// `module VIRT_<Name> { <target> u_1 }`, interface → boundary `io` ports).
fn synthetic_module_text(target: &str, uri: &McURI) -> Result<String, Box<dyn Error>> {
    let mod_name = synthetic_module_name(target);
    if interfaces_in_file(uri).iter().any(|i| i == target) {
        synthesize_interface_module(target, uri)
    } else {
        Ok(format!(
            "\nmodule {mod_name}\n{{\n    {target} {SYNTHETIC_INSTANCE}\n}}\n"
        ))
    }
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
