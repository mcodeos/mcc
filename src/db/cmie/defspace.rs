// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! §12.1 DefinitionSpace — the definition space object (design §12).
//!
//! The definition space is the **loading context + definition view**: which
//! source files and system libraries are visible and where the boundary is,
//! plus a unified read view over the definition tables (workspace + system
//! lib) under one `McSpaceName` identity. Construction = loading (design
//! §12.2): files and libraries enter through the loader chain and are recorded
//! in the source manifest; the definition tables they populate stay where the
//! loader writes them (`WorkspaceManager` + `global::mcc_*`), and this object
//! is the single typed view over both.
//!
//! This is the two-space counterpart of the circuit object [`DianLu`](crate::DianLu)
//! (design §12.2): the definition space is loaded then read-only (③ type
//! resolution, instantiation reads definitions from here); the circuit is
//! instantiated then projected. Relationship is one-way:
//! `DefinitionSpace → (instantiation rules) → DianLu`.

use super::tables::WorkspaceManager;
use crate::db::infra::global;
use crate::semantic::component::McComponent;
use crate::semantic::mc_define::McDefineDef;
use crate::semantic::mc_enum::McEnumDef;
use crate::semantic::mc_ifs::McInterface;
use crate::semantic::module::McModule;
use crate::{McSpaceName, McURI};
use std::path::PathBuf;
use std::sync::Arc;

/// Load domain of one source file in the definition space.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceDomain {
    /// Loaded into the active project workspace (`mcb_add` /
    /// `mcb_add_from_string`).
    Project,
    /// Loaded as part of a system library (`mcb_load_lib`); names the library.
    SystemLib(String),
}

/// Boundary of one loaded system library: its name, on-disk root, and the
/// URIs of the files it brought into the definition space.
#[derive(Debug, Clone)]
pub struct LibBoundary {
    pub name: String,
    pub root: PathBuf,
    pub uris: Vec<McURI>,
}

/// §12.1 — the definition space object: loading context (source manifest +
/// library boundary) plus a unified definition view over the workspace and
/// system-lib tables under one `McSpaceName` identity.
///
/// The manifest fields live on [`WorkspaceManager`] so they follow the
/// per-workspace lifecycle (snapshot / switch / clear); this type is the
/// constructible, typed view over the active workspace.
#[derive(Clone, Copy)]
pub struct DefinitionSpace<'a> {
    ws: &'a WorkspaceManager,
}

impl<'a> DefinitionSpace<'a> {
    /// Wrap a workspace as a definition space.
    pub(crate) fn of(ws: &'a WorkspaceManager) -> Self {
        DefinitionSpace { ws }
    }

    // ── Loading context: source manifest ──

    /// Every loaded source file and the domain it was loaded into.
    pub fn sources(&self) -> impl Iterator<Item = (McURI, SourceDomain)> + '_ {
        self.ws
            .sources
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
    }

    /// The load domain of one source file, if it is loaded.
    pub fn source_of(&self, uri: &McURI) -> Option<SourceDomain> {
        self.ws.sources.get(uri).map(|e| e.value().clone())
    }

    /// Is this source file part of the active project (not a system lib)?
    pub fn is_project_source(&self, uri: &McURI) -> bool {
        matches!(self.source_of(uri), Some(SourceDomain::Project))
    }

    // ── Loading context: library boundary ──

    /// Every loaded system library and its boundary (root + member uris).
    pub fn libs(&self) -> impl Iterator<Item = (String, LibBoundary)> + '_ {
        self.ws
            .libs
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
    }

    /// The boundary of one loaded system library, if any.
    pub fn lib(&self, name: &str) -> Option<LibBoundary> {
        self.ws.libs.get(name).map(|e| e.value().clone())
    }

    // ── Unified definition view (workspace, then system lib) ──

    /// Look up a component by its `McSpaceName` — workspace first, then the
    /// system-lib tables (one identity, two table systems; design §12.4 rule 1).
    pub fn get_component(&self, sn: &McSpaceName) -> Option<Arc<McComponent>> {
        self.ws
            .components
            .get(sn)
            .map(|e| e.value().clone())
            .or_else(|| global::mcc_components.get(sn).map(|e| e.value().clone()))
    }

    /// Look up a module by its `McSpaceName` — workspace first, then the
    /// system-lib tables.
    pub fn get_module(&self, sn: &McSpaceName) -> Option<Arc<McModule>> {
        self.ws
            .modules
            .get(sn)
            .map(|e| e.value().clone())
            .or_else(|| global::mcc_modules.get(sn).map(|e| e.value().clone()))
    }

    /// Look up an interface by its `McSpaceName` — workspace first, then the
    /// system-lib tables.
    pub fn get_interface(&self, sn: &McSpaceName) -> Option<Arc<McInterface>> {
        self.ws
            .interfaces
            .get(sn)
            .map(|e| e.value().clone())
            .or_else(|| global::mcc_interfaces.get(sn).map(|e| e.value().clone()))
    }

    /// Look up an enum by its `McSpaceName` — workspace first, then the
    /// system-lib tables.
    pub fn get_enum(&self, sn: &McSpaceName) -> Option<Arc<McEnumDef>> {
        self.ws
            .enums
            .get(sn)
            .map(|e| e.value().clone())
            .or_else(|| global::mcc_enums.get(sn).map(|e| e.value().clone()))
    }

    /// Look up a define by its `McSpaceName` — workspace first, then the
    /// system-lib tables.
    pub fn get_define(&self, sn: &McSpaceName) -> Option<Arc<McDefineDef>> {
        self.ws
            .defines
            .get(sn)
            .map(|e| e.value().clone())
            .or_else(|| global::mcc_defines.get(sn).map(|e| e.value().clone()))
    }
}

/// The active definition space — the current workspace seen as a definition
/// space (design §12.2: one active per workspace, more can coexist saved).
pub fn definition_space() -> DefinitionSpace<'static> {
    DefinitionSpace::of(&super::tables::WORKSPACE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::cmie::tables::WorkspaceManager;
    use crate::McIds;

    fn uri(s: &str) -> McURI {
        McURI::from(s)
    }

    /// The end-to-end wiring (loader records the manifest into the global
    /// workspace) lives in `tests/defspace_wiring.rs`, a separate test binary —
    /// the lib unit tests here must NOT mutate the process-global `WORKSPACE`
    /// (the parallel mc_code/buildcmd tests share it). These tests construct an
    /// isolated [`WorkspaceManager`] instead.
    #[test]
    fn manifest_accessors_read_a_definition_space() {
        let wm = WorkspaceManager::new();
        wm.sources.insert(uri("/mcc/proj.mc"), SourceDomain::Project);
        wm.sources
            .insert(uri("/mcc/lib.mc"), SourceDomain::SystemLib("acme".into()));
        wm.libs.insert(
            "acme".into(),
            LibBoundary {
                name: "acme".into(),
                root: PathBuf::from("/libs/acme"),
                uris: vec![uri("/mcc/lib.mc")],
            },
        );

        let ds = DefinitionSpace::of(&wm);
        assert_eq!(
            ds.source_of(&uri("/mcc/proj.mc")),
            Some(SourceDomain::Project)
        );
        assert!(ds.is_project_source(&uri("/mcc/proj.mc")));
        assert!(
            !ds.is_project_source(&uri("/mcc/lib.mc")),
            "a system-lib source is not a project source"
        );
        assert_eq!(ds.sources().count(), 2);

        let boundary = ds.lib("acme").expect("loaded lib has a boundary");
        assert_eq!(boundary.name, "acme");
        assert_eq!(boundary.uris, vec![uri("/mcc/lib.mc")]);
        assert!(ds.lib("nope").is_none());
        assert_eq!(ds.libs().count(), 1);
    }

    /// The unified definition view is empty over an empty workspace + empty
    /// system tables. (Resolution of real defs through the view is exercised
    /// end to end in `tests/defspace_wiring.rs`, where the loader populates a
    /// workspace — the def constructors need a parsed `AstNode`, which has no
    /// place in these global-free unit tests.)
    #[test]
    fn unified_lookup_is_empty_over_an_empty_workspace() {
        let wm = WorkspaceManager::new();
        let ds = DefinitionSpace::of(&wm);
        let sn = McSpaceName::new(&McIds::from("main"), uri("/mcc/proj.mc"));

        assert!(ds.get_component(&sn).is_none());
        assert!(ds.get_module(&sn).is_none());
        assert!(ds.get_interface(&sn).is_none());
        assert!(ds.get_enum(&sn).is_none());
        assert!(ds.get_define(&sn).is_none());
    }
}
