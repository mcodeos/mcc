// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! §12.2: DianLu — the core circuit object: one instantiation of one entry
//! module.
//!
//! The physical model is built by the pipeline (member_set → classify →
//! resolve_reference → materialize) into an instance tree (`McModuleInst`,
//! which carries the vector grouping nodes `McVectorInst` and the lane /
//! connectivity structures). A `DianLu` is the owning object for ONE such
//! instantiation: it holds the tree plus the lazily derived flat projection
//! view (`InstTable`). `flatten()` is the single one-way projection exit
//! (invariant B) — it derives the flat view from the already-built tree and
//! never re-enters instantiation, and it runs the flat electrical net checks
//! (§11.4) once.
//!
//! This replaces the previous shape where the flat build entry
//! (`mcb_pass2_flat`) re-ran the whole instantiation just to flatten — the
//! structural cause of double-instantiation (and of the GAP2 double-report
//! that diagnostic dedup then papered over). One instantiation = one DianLu;
//! tree-only consumers read [`Self::tree`], flat consumers call
//! [`Self::flatten`].

use super::insttab::InstTable;
use super::mc_mod::McModuleInst;
use crate::db::diagnostic::diagnostic::Diagnostic;
use crate::instant::identity::{CircuitKey, IdentityRegistry};

/// One instantiation of one entry module — the physical model plus its flat
/// projection view (design §12.2, code name `DianLu`).
pub struct DianLu {
    /// Instance tree (modelling layer): instances, vector grouping nodes
    /// (`McVectorInst`), lanes (`ConnectionInst`) and net connectivity.
    tree: McModuleInst,
    /// Starting id for the flat table (typically 1000).
    start_id: u32,
    /// Lazily built flat projection view; `None` until the first `flatten()`.
    table: Option<InstTable>,
    /// Flat electrical net-check diagnostics (§11.4), produced once by
    /// `flatten()` and returned to the caller for logging (Phase A: DianLu
    /// never writes to the workspace diagnostic manager itself).
    net_diags: Vec<Diagnostic>,
    /// Phase C1: per-build identity registry (canonical path ↔ `NodeId`),
    /// rebuilt from the frozen tree's companion node ids on construction.
    /// Consumers resolve `node_id` by path (or vice versa) through here.
    identity: IdentityRegistry,
}

impl DianLu {
    /// Wrap an already-instantiated tree. The model is authoritative; the flat
    /// projection is derived lazily via [`Self::flatten`]. The per-build
    /// identity registry is rebuilt from the tree's companion node ids.
    pub fn new(tree: McModuleInst, start_id: u32) -> Self {
        let identity = build_identity_registry(&tree);
        DianLu {
            tree,
            start_id,
            table: None,
            net_diags: Vec::new(),
            identity,
        }
    }

    /// The per-build identity registry (canonical path ↔ node id).
    pub fn identity(&self) -> &IdentityRegistry {
        &self.identity
    }

    /// The instance tree (modelling layer), not the flat projection.
    pub fn tree(&self) -> &McModuleInst {
        &self.tree
    }

    /// Consume the object, discarding any flat projection.
    pub fn into_tree(self) -> McModuleInst {
        self.tree
    }

    /// Consume the object into (tree, table). Panics if `flatten` has not been
    /// called — the projection is derived exactly once, by [`Self::flatten`],
    /// so callers must project before taking the parts.
    pub fn into_parts(self) -> (McModuleInst, InstTable) {
        let table = self
            .table
            .expect("DianLu::into_parts: flatten() must run first");
        (self.tree, table)
    }

    /// The flat projection view, if `flatten` has been called.
    pub fn table(&self) -> Option<&InstTable> {
        self.table.as_ref()
    }

    /// One-way projection (invariant B): derive the flat `InstTable` from the
    /// already-built tree — never re-instantiate. Cached; subsequent calls are
    /// no-ops. Runs the flat electrical net checks (§11.4) once and returns
    /// their diagnostics to the caller, who owns logging (Phase A: DianLu
    /// performs no global writes; the `current_uri` context is the caller's).
    pub fn flatten(&mut self) -> Vec<Diagnostic> {
        self.flatten_with_prefix(None)
    }

    /// Like [`Self::flatten`], but marks every entry under `synthetic_prefix`
    /// (a virtual-instantiation wrapper module, e.g. `VIRT_XTAL4`) as synthetic
    /// during the projection, so the unwired/pin-count checks skip synthetic
    /// instances in a standalone component/interface view.
    pub fn flatten_with_prefix(&mut self, synthetic_prefix: Option<&str>) -> Vec<Diagnostic> {
        if self.table.is_none() {
            let mut table = InstTable::from_module_inst(&self.tree, self.start_id);
            if let Some(prefix) = synthetic_prefix {
                table.mark_synthetic_by_path_prefix(prefix);
            }
            // Flat electrical checks run once during the projection (§11.4
            // flat entry); their diagnostics are returned, never logged here.
            let results = crate::semantic::validation::nets::run_net_checks(&table);
            self.net_diags =
                crate::semantic::validation::nets::net_results_to_diagnostics(&results);
            self.table = Some(table);
        }
        self.net_diags.clone()
    }
}

/// Rebuild the per-build identity registry from a frozen tree's companion
/// node ids (Phase C1): walk the modelling tree in the same order as the
/// flat projection and resume every `(canonical path, node id)` pair. The
/// result is identical to the construction-time registry — same path, same
/// id (per-build determinism).
fn build_identity_registry(tree: &McModuleInst) -> IdentityRegistry {
    let mut reg = IdentityRegistry::new(CircuitKey::new(&tree.def_uri.to_string(), &tree.name));
    resume_module(&mut reg, &tree.name, tree);
    reg
}

/// Recursive resume over one module's scope: the module node itself, its
/// components (leaf nodes), and its sub-modules (recursive).
fn resume_module(reg: &mut IdentityRegistry, path: &str, module: &McModuleInst) {
    if let Some(id) = module.node_id {
        reg.resume(path, id);
    }
    for comp in &module.components {
        if let Some(id) = comp.node_id {
            reg.resume(&format!("{path}.{}", comp.name), id);
        }
    }
    for sub in &module.sub_modules {
        let sub_path = format!("{path}.{}", sub.name);
        if let Some(id) = sub.node_id {
            reg.resume(&sub_path, id);
        }
        resume_module(reg, &sub_path, sub);
    }
}
