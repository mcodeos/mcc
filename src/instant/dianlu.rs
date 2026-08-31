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
}

impl DianLu {
    /// Wrap an already-instantiated tree. The model is authoritative; the flat
    /// projection is derived lazily via [`Self::flatten`].
    pub fn new(tree: McModuleInst, start_id: u32) -> Self {
        DianLu {
            tree,
            start_id,
            table: None,
        }
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
    /// no-ops. Runs the flat electrical net checks (§11.4) once, logging their
    /// diagnostics under the owning file's current_uri.
    pub fn flatten(&mut self) -> &InstTable {
        self.flatten_with_prefix(None)
    }

    /// Like [`Self::flatten`], but marks every entry under `synthetic_prefix`
    /// (a virtual-instantiation wrapper module, e.g. `VIRT_XTAL4`) as synthetic
    /// during the projection, so the unwired/pin-count checks skip synthetic
    /// instances in a standalone component/interface view.
    pub fn flatten_with_prefix(&mut self, synthetic_prefix: Option<&str>) -> &InstTable {
        if self.table.is_none() {
            let mut table = InstTable::from_module_inst(&self.tree, self.start_id);
            if let Some(prefix) = synthetic_prefix {
                table.mark_synthetic_by_path_prefix(prefix);
            }
            // Flat electrical checks after the projection (§11.4 flat entry).
            // Log each result into the workspace diagnostics under the owning
            // file's current_uri, restoring the previous uri afterwards.
            let net_results = crate::semantic::validation::nets::run_net_checks(&table);
            let saved_uri = crate::current_uri::try_get();
            for r in &net_results {
                if !r.uri.is_empty() {
                    crate::current_uri::set(&crate::McURI::from(r.uri.as_str()));
                }
                let level = match r.severity {
                    "error" => crate::db::diagnostic::diagnostic::DiagnosticLevel::Error,
                    "info" => crate::db::diagnostic::diagnostic::DiagnosticLevel::Info,
                    _ => crate::db::diagnostic::diagnostic::DiagnosticLevel::Warning,
                };
                crate::db::diagnostic::diagnostic::diagnostic_log(
                    r.code,
                    level,
                    r.pos,
                    0,
                    &r.message,
                    &[],
                );
            }
            match saved_uri {
                Some(ref uri) => crate::current_uri::set(uri),
                None => crate::current_uri::reset(),
            }
            self.table = Some(table);
        }
        self.table.as_ref().expect("just set")
    }
}
