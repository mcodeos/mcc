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

use super::cmie::tables::WorkspaceManager;
use crate::db::infra::global;
use crate::db::infra::mc_code::McCode;
use crate::semantic::component::McComponent;
use crate::semantic::mc_define::McDefineDef;
use crate::semantic::mc_enum::McEnumDef;
use crate::semantic::mc_ifs::McInterface;
use crate::semantic::module::McModule;
use crate::{McSpaceName, McURI};
use dashmap::DashMap;
use std::collections::HashSet;
use std::hash::Hash;
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

    // ── Loading context: loaded source content (pass1 semantic tables) ──
    //
    // The `mcodes` table holds each loaded file's pass1 record — AST, tokens,
    // symbols, ref-def map (the `McCode`). It is part of the loading context
    // ("which files are loaded, with what content"), so LSP hover / goto-def /
    // completion / semantic-token reads reach file content through this view,
    // not the table directly (design §12.4 rule 1).

    /// The loaded source file's pass1 record (AST, tokens, symbols, ref-def
    /// map), if the file is loaded. `None` when the file is not in the
    /// definition space.
    pub fn source_file(
        &self,
        uri: &McURI,
    ) -> Option<dashmap::mapref::one::Ref<'_, McURI, McCode>> {
        self.ws.mcodes.get(uri)
    }

    /// Every loaded source file's pass1 record, in arbitrary (DashMap) order.
    pub fn source_files(
        &self,
    ) -> impl Iterator<Item = dashmap::mapref::multiple::RefMulti<'_, McURI, McCode>> + '_ {
        self.ws.mcodes.iter()
    }

    // ── Loading context: file dependency (reverse deps) ──

    /// Files that `use` this one — "who uses me" (§7.6). When file B's CMIE
    /// defs change, iterate `reverse_deps[B]` to find the affected files whose
    /// Use table needs rebuilding.
    pub fn reverse_deps(&self, uri: &McURI) -> Option<Vec<McURI>> {
        self.ws.reverse_deps.get(uri).map(|e| e.value().clone())
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

    // ── Unified definition view: whole-table enumeration ──

    /// Enumerate every component definition: workspace entries first, then
    /// system-lib entries whose identity is not already present. A file loaded
    /// both as a project file and as a system lib appears once, workspace-first
    /// (same shadowing rule as the single-identity lookups).
    pub fn all_components(&self) -> Vec<(McSpaceName, Arc<McComponent>)> {
        chain_dedup(&self.ws.components, &global::mcc_components)
    }

    /// Enumerate every module definition (workspace-then-system-lib, deduped).
    pub fn all_modules(&self) -> Vec<(McSpaceName, Arc<McModule>)> {
        chain_dedup(&self.ws.modules, &global::mcc_modules)
    }

    /// Enumerate every interface definition (workspace-then-system-lib, deduped).
    pub fn all_interfaces(&self) -> Vec<(McSpaceName, Arc<McInterface>)> {
        chain_dedup(&self.ws.interfaces, &global::mcc_interfaces)
    }

    /// Enumerate every enum definition (workspace-then-system-lib, deduped).
    pub fn all_enums(&self) -> Vec<(McSpaceName, Arc<McEnumDef>)> {
        chain_dedup(&self.ws.enums, &global::mcc_enums)
    }

    /// Enumerate every define definition (workspace-then-system-lib, deduped).
    pub fn all_defines(&self) -> Vec<(McSpaceName, Arc<McDefineDef>)> {
        chain_dedup(&self.ws.defines, &global::mcc_defines)
    }

    // ── System-library-only view (P5 visibility) ──
    //
    // The unified `get_*` / `all_*` views mix the workspace in. P5 — "mcode
    // system library is always visible" (refs.rs §5.4 gate) — is the *opposite*
    // read: a cross-file reference to a definition in a *different project file*
    // must be justified by the use chain, not by mere table existence (the
    // net1.basic.mc → c3.defs.mc regression). Callers with that semantic read
    // the system tables alone, through this view.

    /// Does the loaded system library (not the workspace) define this identity,
    /// as any class kind?
    pub fn system_contains(&self, sn: &McSpaceName) -> bool {
        global::mcc_components.contains_key(sn)
            || global::mcc_modules.contains_key(sn)
            || global::mcc_interfaces.contains_key(sn)
            || global::mcc_enums.contains_key(sn)
    }

    /// Enumerate every *system-library* component definition (P5).
    pub fn system_components(&self) -> Vec<(McSpaceName, Arc<McComponent>)> {
        system_dump(&global::mcc_components)
    }

    /// Enumerate every *system-library* module definition (P5).
    pub fn system_modules(&self) -> Vec<(McSpaceName, Arc<McModule>)> {
        system_dump(&global::mcc_modules)
    }

    /// Enumerate every *system-library* interface definition (P5).
    pub fn system_interfaces(&self) -> Vec<(McSpaceName, Arc<McInterface>)> {
        system_dump(&global::mcc_interfaces)
    }

    /// Enumerate every *system-library* enum definition (P5).
    pub fn system_enums(&self) -> Vec<(McSpaceName, Arc<McEnumDef>)> {
        system_dump(&global::mcc_enums)
    }
}

/// System-lib-only enumeration: every entry of one global table, in arbitrary
/// (DashMap) order. No dedup — the global tables are a single system.
fn system_dump<K, V>(global: &DashMap<K, V>) -> Vec<(K, V)>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    global
        .iter()
        .map(|e| (e.key().clone(), e.value().clone()))
        .collect()
}

/// Workspace-then-system-lib enumeration, deduplicated by exact table identity
/// (the same key semantics the tables themselves use — an identity loaded into
/// both tables is the same definition and must be enumerated once).
fn chain_dedup<K, V>(ws: &DashMap<K, V>, global: &DashMap<K, V>) -> Vec<(K, V)>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    let mut out: Vec<(K, V)> = ws
        .iter()
        .map(|e| (e.key().clone(), e.value().clone()))
        .collect();
    let mut seen: HashSet<K> = out.iter().map(|(k, _)| k.clone()).collect();
    for e in global.iter() {
        if seen.insert(e.key().clone()) {
            out.push((e.key().clone(), e.value().clone()));
        }
    }
    out
}

/// The active definition space — the current workspace seen as a definition
/// space (design §12.2: one active per workspace, more can coexist saved).
pub fn definition_space() -> DefinitionSpace<'static> {
    DefinitionSpace::of(&super::cmie::tables::WORKSPACE)
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
        wm.sources
            .insert(uri("/mcc/proj.mc"), SourceDomain::Project);
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

    /// The loading context also exposes each loaded file's pass1 record and
    /// the reverse-dependency index — LSP hover / goto-def / completion /
    /// semantic-token reads reach file content through the definition space,
    /// not the `mcodes` / `reverse_deps` tables directly.
    #[test]
    fn source_content_and_reverse_deps_read_through_the_view() {
        let wm = WorkspaceManager::new();
        wm.mcodes
            .insert(uri("/mcc/a.mc"), McCode::new_empty());
        wm.reverse_deps
            .insert(uri("/mcc/b.mc"), vec![uri("/mcc/a.mc")]);

        let ds = DefinitionSpace::of(&wm);
        assert!(
            ds.source_file(&uri("/mcc/a.mc")).is_some(),
            "a loaded file's pass1 record is reachable"
        );
        assert!(ds.source_file(&uri("/mcc/nope.mc")).is_none());
        assert_eq!(ds.source_files().count(), 1);
        assert_eq!(
            ds.reverse_deps(&uri("/mcc/b.mc")),
            Some(vec![uri("/mcc/a.mc")])
        );
        assert!(ds.reverse_deps(&uri("/mcc/a.mc")).is_none());
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

    /// Whole-table enumeration is workspace-first and deduplicated by identity:
    /// a key present in both tables (the same file loaded as project and as
    /// system lib) appears once, keeping the workspace entry. Exercised on
    /// plain local `DashMap`s — the real `all_*` wrappers feed the process-global
    /// system tables, which these tests must not touch.
    #[test]
    fn chain_dedup_enumerates_workspace_first_and_skips_duplicate_identity() {
        let ws: DashMap<String, i32> = DashMap::new();
        let global: DashMap<String, i32> = DashMap::new();
        ws.insert("a".into(), 1);
        ws.insert("b".into(), 2);
        global.insert("a".into(), 99); // duplicate identity -> skipped (workspace wins)
        global.insert("c".into(), 3);

        let out = chain_dedup(&ws, &global);
        // DashMap iteration order is hash-based, not insertion order — sort by
        // key for the set-level assertion.
        let mut pairs: Vec<_> = out.into_iter().collect();
        pairs.sort();
        assert_eq!(
            pairs,
            vec![
                ("a".to_string(), 1),
                ("b".to_string(), 2),
                ("c".to_string(), 3),
            ],
            "duplicate identity 'a' resolves to the workspace value (1, not 99); \
             global-only 'c' is appended"
        );
    }
}
