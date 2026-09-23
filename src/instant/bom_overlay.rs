// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! BOM overlay: engineering-level part selection over abstract slots.
//!
//! `bom.overlay.toml` sits at a project root. Keys are instance paths
//! relative to the top module (`"LDO.ldo"`); values are component class
//! names resolved against the live defs at bind time. A binding replaces the
//! instance's class identity only — never its shape. See
//! param-authoring-design.md section 4.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, RwLock};

pub(crate) const OVERLAY_FILE: &str = "bom.overlay.toml";

/// Outcome of one bind attempt, recorded per overlay key for the ERC checks.
#[derive(Debug, Clone)]
pub(crate) enum BindOutcome {
    /// Slot bound to the variant def.
    Bound,
    /// Value name resolved to no live component def.
    ValueUnresolved,
    /// Value def exists but is not a `:` descendant of the declared slot class.
    ValueNotDescendant,
    /// The key's instance declares a concrete class, not an abstract slot.
    KeyNotSlot,
}

#[derive(Debug, Default)]
struct BomOverlayState {
    root: Option<PathBuf>,
    /// Canonical entry uri of the build in flight (set by [`begin_build`]).
    entry: Option<String>,
    /// overlay key (top-relative instance path) -> variant class name
    entries: BTreeMap<String, String>,
    /// overlay key -> (canonical instance path, bind outcome); per build run
    binds: BTreeMap<String, (String, BindOutcome)>,
}

static BOM_OVERLAY: LazyLock<RwLock<BomOverlayState>> =
    LazyLock::new(|| RwLock::new(BomOverlayState::default()));

/// Load the overlay that owns `dir` (the nearest ancestor carrying an
/// `OVERLAY_FILE`), replacing whatever the registry held. No ancestor carries
/// one, or the file does not parse, clears the registry — a build whose board
/// has no overlay has no slots to bind.
fn load_for_dir(dir: &Path) {
    let mut hit: Option<(PathBuf, String)> = None;
    let mut cur = Some(dir.to_path_buf());
    while let Some(d) = cur {
        let candidate = d.join(OVERLAY_FILE);
        if let Ok(t) = fs::read_to_string(&candidate) {
            hit = Some((d, t));
            break;
        }
        cur = d.parent().map(|p| p.to_path_buf());
    }
    let Some((root, text)) = hit else {
        let mut state = BOM_OVERLAY.write().expect("bom overlay lock");
        state.root = None;
        state.entries.clear();
        return;
    };
    let value: toml::Value = match toml::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("mcc: {OVERLAY_FILE}: {e}");
            let mut state = BOM_OVERLAY.write().expect("bom overlay lock");
            state.root = None;
            state.entries.clear();
            return;
        }
    };
    let mut entries = BTreeMap::new();
    if let Some(table) = value.as_table() {
        for (key, val) in table {
            match val.as_str() {
                Some(class) => {
                    entries.insert(key.clone(), class.to_string());
                }
                None => {
                    eprintln!("mcc: {OVERLAY_FILE}: key '{key}' must map to a class name string")
                }
            }
        }
    }
    let mut state = BOM_OVERLAY.write().expect("bom overlay lock");
    state.root = Some(root);
    state.entries = entries;
}

/// Open a build run: clear the bind ledger and record the build's canonical
/// entry uri (call from the instantiation root once the entry module is
/// resolved). The overlay is rediscovered per build from the entry file's
/// nearest ancestor carrying one, so the overlay binds exactly the boards in
/// the directory tree that owns it — never a registry left over from another
/// project or a synthetic build over virtual sources.
pub(crate) fn begin_build(entry_uri: &str) {
    let entry = crate::build::pass1::canonicalize_project_uri(&crate::McURI::from(entry_uri));
    let parent = Path::new(&entry).parent().map(|p| p.to_path_buf());
    if let Some(dir) = parent {
        load_for_dir(&dir);
    }
    let mut state = BOM_OVERLAY.write().expect("bom overlay lock");
    state.binds.clear();
    state.entry = Some(entry);
}

/// The consumer gate: an overlay was discovered for the build in flight, and
/// that build enters through a file under the discovered root (true by
/// construction for a discovered overlay, false once discovery cleared the
/// registry).
fn overlay_active(state: &BomOverlayState) -> bool {
    let Some(root) = &state.root else {
        return false;
    };
    state
        .entry
        .as_deref()
        .is_some_and(|e| Path::new(e).starts_with(root))
}

/// The overlay key for a component instance declared inside the module being
/// built at `current_path` (`"main.modldo"` + `ldo` -> `"modldo.ldo"`): the
/// canonical path minus the leading top-module segment.
pub(crate) fn overlay_key(current_path: &str, inst: &str) -> String {
    match current_path.split_once('.') {
        Some((_top, rest)) => format!("{rest}.{inst}"),
        None => inst.to_string(),
    }
}

/// Attempt to bind the declared class of one component instance. Called from
/// the Pass2 declaration expansion for every declared component; records the
/// outcome and returns the replacement def when the bind succeeds.
///
/// Resolution is anchored at `uri` (the instantiating module's file), so the
/// value name reads the same visibility world as the module face author.
pub(crate) fn apply_binding(
    current_path: &str,
    inst: &str,
    declared: &std::sync::Arc<crate::semantic::component::McComponent>,
    uri: &crate::McURI,
) -> Option<std::sync::Arc<crate::semantic::component::McComponent>> {
    let key = overlay_key(current_path, inst);
    let value = {
        let state = BOM_OVERLAY.read().expect("bom overlay lock");
        if !overlay_active(&state) {
            None
        } else {
            state.entries.get(&key).cloned()
        }
    }?;
    let inst_path = format!("{current_path}.{inst}");
    if !declared.is_abstract {
        record(key, inst_path, BindOutcome::KeyNotSlot);
        return None;
    }
    let Some((value_def, value_id)) = resolve_value(&value, uri) else {
        record(key, inst_path, BindOutcome::ValueUnresolved);
        return None;
    };
    let Some(declared_id) = def_id_of(declared) else {
        record(key, inst_path, BindOutcome::ValueNotDescendant);
        return None;
    };
    let descendant = value_id
        .and_then(|id| crate::db::defregistry::variant_base_of(id))
        .is_some_and(|base| base == declared_id);
    if !descendant {
        record(key, inst_path, BindOutcome::ValueNotDescendant);
        return None;
    }
    record(key, inst_path, BindOutcome::Bound);
    Some(value_def)
}

fn record(key: String, path: String, outcome: BindOutcome) {
    BOM_OVERLAY
        .write()
        .expect("bom overlay lock")
        .binds
        .insert(key, (path, outcome));
}

/// Resolve a value name against the live component defs (any domain — a
/// value may name a library part), returning (def, def_id). The design sends
/// the overlay through the DefRegistry, not the visibility tables, so this is
/// an exact ident-string match; two live rows with one name resolve to
/// nothing (the exact-name law leaves no second guess).
fn resolve_value(
    name: &str,
    _uri: &crate::McURI,
) -> Option<(
    std::sync::Arc<crate::semantic::component::McComponent>,
    Option<crate::DefId>,
)> {
    let ds = crate::definition_space();
    let hits: Vec<_> = ds
        .all_components()
        .into_iter()
        .filter(|(sn, _)| sn.ident.to_string() == name)
        .collect();
    if hits.len() != 1 {
        return None;
    }
    let (sn, comp) = hits.into_iter().next()?;
    let id = crate::db::defregistry::def_id(&sn, crate::DefKind::Component);
    Some((comp, id))
}

fn def_id_of(comp: &crate::semantic::component::McComponent) -> Option<crate::DefId> {
    let sn = crate::McSpaceName::new(&comp.name, comp.uri.clone());
    crate::db::defregistry::def_id(&sn, crate::DefKind::Component)
}

/// The bind ledger read face for the ERC checks: (key, canonical instance
/// path, outcome) triples, in key order.
pub(crate) fn bind_outcomes() -> Vec<(String, String, BindOutcome)> {
    let state = BOM_OVERLAY.read().expect("bom overlay lock");
    if !overlay_active(&state) {
        return Vec::new();
    }
    state
        .binds
        .iter()
        .map(|(k, (p, o))| (k.clone(), p.clone(), o.clone()))
        .collect()
}

/// Overlay keys that no bind attempt ever consumed (dangling paths).
pub(crate) fn dangling_keys() -> Vec<String> {
    let state = BOM_OVERLAY.read().expect("bom overlay lock");
    if !overlay_active(&state) {
        return Vec::new();
    }
    state
        .entries
        .keys()
        .filter(|k| !state.binds.contains_key(k.as_str()))
        .cloned()
        .collect()
}

/// The class name a key maps to (for check messages).
pub(crate) fn overlay_value(key: &str) -> Option<String> {
    BOM_OVERLAY
        .read()
        .expect("bom overlay lock")
        .entries
        .get(key)
        .cloned()
}
