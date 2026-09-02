// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Phase F (implementation plan §9 F, design §12.6): circuit → def dependency
//! edges.
//!
//! One instantiation performs a set of definition-space resolutions (the
//! entry module, every component / module / interface / enum class it
//! constructs). Each resolution is recorded into a per-build collector — the
//! resolution bridge (`mcb_get_cmie` / `Resolver::resolve_system`) is the only
//! channel touching the definition space during instantiation, so the edge
//! set is complete by construction and no separate pass is needed.
//!
//! The collector is a build-scoped scratch, installed by `mcb_instantiate`
//! for the duration of one instantiation and frozen into the `DianLu` as
//! `circuit_deps`. The def→circuits reverse index (invalidation domain,
//! design §12.6) is NOT held here — it is the CircuitWorld's (Phase G); this
//! module only produces the per-circuit out side.

use crate::instant::inststore::TreeView;
use crate::McCMIE;
use crate::McSpaceName;
use std::cell::RefCell;
use std::rc::Rc;

thread_local! {
    /// The active circuit-build dependency collector. `None` outside an
    /// instantiation window, so pass1 / LSP / query resolutions never record.
    static ACTIVE: RefCell<Option<Rc<RefCell<Vec<McSpaceName>>>>> = const { RefCell::new(None) };
}

/// RAII guard for one instantiation's dependency window: installs a fresh
/// collector and pops the active slot on drop (an error path between
/// `install` and `finish` still uninstalls cleanly).
pub struct DepCollectorGuard(Rc<RefCell<Vec<McSpaceName>>>);

impl DepCollectorGuard {
    /// Install a fresh collector for one instantiation window.
    pub fn install() -> Self {
        let collector = Rc::new(RefCell::new(Vec::new()));
        ACTIVE.with(|a| *a.borrow_mut() = Some(collector.clone()));
        DepCollectorGuard(collector)
    }

    /// Pop the active slot and take the defs resolved during the window.
    /// The guard itself implements `Drop`, so it must not be consumed —
    /// `Drop` uninstalls the (already cleared) active slot again, idempotent.
    pub fn finish(&mut self) -> Vec<McSpaceName> {
        ACTIVE.with(|a| *a.borrow_mut() = None);
        std::mem::take(&mut *self.0.borrow_mut())
    }
}

impl Drop for DepCollectorGuard {
    fn drop(&mut self) {
        ACTIVE.with(|a| *a.borrow_mut() = None);
    }
}

/// Record one resolved definition into the active collector. No-op outside a
/// circuit-build window. `def` is the canonical `(def-name, defining-file)`.
pub fn record_def(def: &McSpaceName) {
    ACTIVE.with(|a| {
        if let Some(collector) = a.borrow().as_ref() {
            let mut deps = collector.borrow_mut();
            if !deps.contains(def) {
                deps.push(def.clone());
            }
        }
    });
}

/// Record a resolved class definition (`McCMIE`) into the active collector.
pub fn record_cmie(cmie: &McCMIE) {
    let Some(uri) = crate::db::resolve::cmie_uri(cmie) else {
        return;
    };
    record_def(&McSpaceName::new(
        &crate::db::cmie::cmie::cmie_ident(cmie),
        uri,
    ));
}

/// Record the defs the frozen tree actually materialized — every component's
/// class and every sub-module's class, recursively. Declared instances carry
/// their def resolved by pass1 (the lapper), so they never pass through the
/// `mcb_get_cmie` bridge at instantiation time; the tree sweep is what makes
/// the circuit→def edge set complete by construction (plan §9 F).
pub fn record_tree_defs(tree: &crate::instant::mc_mod::McModuleInst, view: &TreeView) {
    for comp in view.components(tree) {
        let uri = comp.def.uri.clone();
        if !uri.is_empty() {
            record_def(&McSpaceName::new(&comp.def.name, uri));
        }
    }
    for sub in view.sub_modules(tree) {
        if !sub.def_uri.is_empty() {
            record_def(&McSpaceName::new(&sub.def.name, sub.def_uri.clone()));
        }
        record_tree_defs(sub, view);
    }
}
