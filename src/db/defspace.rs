// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! §12.1 DefinitionSpace — the definition space object (design §12).
//!
//! The definition space is the **loading context + definition view**: which
//! source files and system libraries are visible and where the boundary is,
//! plus a unified read view over the definitions under one `McSpaceName`
//! identity. Construction = loading (design §12.2): files and libraries
//! enter through the loader chain and are recorded in the source manifest.
//! Definitions live in the single-table definition registry
//! (`db::defregistry`, design §9 Phase B — DefId + arena + tombstone); this
//! object is the typed view over the registry plus the workspace manifest.
//!
//! This is the two-space counterpart of the circuit object [`DianLu`](crate::DianLu)
//! (design §12.2): the definition space is loaded then read-only (③ type
//! resolution, instantiation reads definitions from here); the circuit is
//! instantiated then projected. Relationship is one-way:
//! `DefinitionSpace → (instantiation rules) → DianLu`.

use super::cmie::tables::WorkspaceManager;
use crate::db::infra::mc_code::McCode;
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
    pub fn source_file(&self, uri: &McURI) -> Option<dashmap::mapref::one::Ref<'_, McURI, McCode>> {
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

    // ── Unified definition view (any domain, one registry) ──

    /// Look up a component by its `McSpaceName`. Precedence is
    /// registry-internal (design §12.4 rule 1): the workspace def shadows a
    /// same-key system-lib def (workspace-first, P0.1 — the mcode lib loads
    /// first, then a project file re-declares the identity and wins).
    pub fn get_component(&self, sn: &McSpaceName) -> Option<Arc<McComponent>> {
        crate::db::defregistry::get_component(sn)
    }

    /// Look up a module by its `McSpaceName`.
    pub fn get_module(&self, sn: &McSpaceName) -> Option<Arc<McModule>> {
        crate::db::defregistry::get_module(sn)
    }

    /// Look up an interface by its `McSpaceName`.
    pub fn get_interface(&self, sn: &McSpaceName) -> Option<Arc<McInterface>> {
        crate::db::defregistry::get_interface(sn)
    }

    /// Look up an enum by its `McSpaceName`.
    pub fn get_enum(&self, sn: &McSpaceName) -> Option<Arc<McEnumDef>> {
        crate::db::defregistry::get_enum(sn)
    }

    /// Look up a define by its `McSpaceName`.
    pub fn get_define(&self, sn: &McSpaceName) -> Option<Arc<McDefineDef>> {
        crate::db::defregistry::get_define(sn)
    }

    // ── Unified definition view: whole-table enumeration ──

    /// Enumerate every live component definition (any domain). The single
    /// registry holds one identity per `(uri, ident)`, so no dedup is needed.
    pub fn all_components(&self) -> Vec<(McSpaceName, Arc<McComponent>)> {
        crate::db::defregistry::all_components()
    }

    /// Enumerate every live module definition (any domain).
    pub fn all_modules(&self) -> Vec<(McSpaceName, Arc<McModule>)> {
        crate::db::defregistry::all_modules()
    }

    /// Enumerate every live interface definition (any domain).
    pub fn all_interfaces(&self) -> Vec<(McSpaceName, Arc<McInterface>)> {
        crate::db::defregistry::all_interfaces()
    }

    /// Enumerate every live enum definition (any domain).
    pub fn all_enums(&self) -> Vec<(McSpaceName, Arc<McEnumDef>)> {
        crate::db::defregistry::all_enums()
    }

    /// Enumerate every live define definition (any domain).
    pub fn all_defines(&self) -> Vec<(McSpaceName, Arc<McDefineDef>)> {
        crate::db::defregistry::all_defines()
    }

    // ── Workspace-only definition view ──
    //
    // Several consumers read the project definitions deliberately WITHOUT the
    // system-lib fallback: project-level checks (name collisions, component
    // stubs), the P4 "collect from project" lookup layer, and the P1 local
    // file scope all operate on project definitions only. The unified get_*
    // / all_* views above mix the system-lib definitions in, which would
    // change their behavior; these workspace-only reads keep the exact
    // semantics (registry entries with `LoadDomain::Project`).

    /// Look up a component by its `McSpaceName` in the project domain only.
    pub fn get_workspace_component(&self, sn: &McSpaceName) -> Option<Arc<McComponent>> {
        crate::db::defregistry::get_workspace_component(sn)
    }

    /// Look up a module by its `McSpaceName` in the project domain only.
    pub fn get_workspace_module(&self, sn: &McSpaceName) -> Option<Arc<McModule>> {
        crate::db::defregistry::get_workspace_module(sn)
    }

    /// Look up an interface by its `McSpaceName` in the project domain only.
    pub fn get_workspace_interface(&self, sn: &McSpaceName) -> Option<Arc<McInterface>> {
        crate::db::defregistry::get_workspace_interface(sn)
    }

    /// Look up an enum by its `McSpaceName` in the project domain only.
    pub fn get_workspace_enum(&self, sn: &McSpaceName) -> Option<Arc<McEnumDef>> {
        crate::db::defregistry::get_workspace_enum(sn)
    }

    /// Look up a define by its `McSpaceName` in the project domain only.
    pub fn get_workspace_define(&self, sn: &McSpaceName) -> Option<Arc<McDefineDef>> {
        crate::db::defregistry::get_workspace_define(sn)
    }

    /// Enumerate every project (workspace) component definition.
    pub fn workspace_components(&self) -> Vec<(McSpaceName, Arc<McComponent>)> {
        crate::db::defregistry::workspace_components()
    }

    /// Enumerate every project (workspace) module definition.
    pub fn workspace_modules(&self) -> Vec<(McSpaceName, Arc<McModule>)> {
        crate::db::defregistry::workspace_modules()
    }

    /// Enumerate every project (workspace) interface definition.
    pub fn workspace_interfaces(&self) -> Vec<(McSpaceName, Arc<McInterface>)> {
        crate::db::defregistry::workspace_interfaces()
    }

    /// Enumerate every project (workspace) enum definition.
    pub fn workspace_enums(&self) -> Vec<(McSpaceName, Arc<McEnumDef>)> {
        crate::db::defregistry::workspace_enums()
    }

    /// Enumerate every project (workspace) define definition.
    pub fn workspace_defines(&self) -> Vec<(McSpaceName, Arc<McDefineDef>)> {
        crate::db::defregistry::workspace_defines()
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
        crate::db::defregistry::system_contains(sn)
    }

    /// Enumerate every *system-library* component definition (P5).
    pub fn system_components(&self) -> Vec<(McSpaceName, Arc<McComponent>)> {
        crate::db::defregistry::system_components()
    }

    /// Enumerate every *system-library* module definition (P5).
    pub fn system_modules(&self) -> Vec<(McSpaceName, Arc<McModule>)> {
        crate::db::defregistry::system_modules()
    }

    /// Enumerate every *system-library* interface definition (P5).
    pub fn system_interfaces(&self) -> Vec<(McSpaceName, Arc<McInterface>)> {
        crate::db::defregistry::system_interfaces()
    }

    /// Enumerate every *system-library* enum definition (P5).
    pub fn system_enums(&self) -> Vec<(McSpaceName, Arc<McEnumDef>)> {
        crate::db::defregistry::system_enums()
    }
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
    use crate::semantic::basic::mc_paramd::McParamDeclares;
    use crate::semantic::common::IOType;
    use crate::semantic::component::mc_attr::McAttributes;
    use crate::semantic::component::mc_layout::McLayout;
    use crate::semantic::component::mc_pins::McPin;
    use crate::semantic::component::mc_pins::McPins;
    use crate::semantic::component::McComponent;
    use crate::semantic::mc_func::McFunctions;
    use crate::semantic::mc_inst::McInstances;
    use crate::McIds;
    use std::sync::Arc;

    fn uri(s: &str) -> McURI {
        McURI::from(s)
    }

    /// Minimal component value for precedence tests: `pin_count` pins tell two
    /// same-identity defs apart.
    fn gold_component(name: &str, component_uri: &str, pin_count: usize) -> Arc<McComponent> {
        let mut pins = McPins::new();
        for i in 0..pin_count {
            pins.pins.insert(
                (i + 1).to_string(),
                McPin {
                    iotype: IOType::In,
                    id: (i + 1).to_string(),
                    names: vec![format!("P{}", i + 1)],
                    values: Arc::new(vec![]),
                    active_low: false,
                    is_nc: false,
                },
            );
        }
        Arc::new(McComponent {
            name: McIds::from(name),
            params: McParamDeclares::new(),
            pins,
            attrs: McAttributes::new(),
            funcs: McFunctions::new(),
            insts: McInstances::new(),
            layout: McLayout {
                left: vec![],
                right: vec![],
                top: vec![],
                bottom: vec![],
            },
            uri: component_uri.into(),
            cond_pins: vec![],
            cond_attrs: vec![],
            span: 0..0,
            anon_counter: 0,
        })
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
        wm.mcodes.insert(uri("/mcc/a.mc"), McCode::new_empty());
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

    /// P0.1 golden sample (defspace-refactor-implementation.md Phase 0): the
    /// unified `get_*` lookup reads the WORKSPACE table first, falling back to
    /// the system-lib (global) tables only on a miss — the precedence the
    /// Phase 3 single-table merge must preserve. The registry is the
    /// process-global single table, so this test writes through the single
    /// write entry (defregistry.rs) on a dedicated key and removes it on drop
    /// (the process-global state is shared with the parallel mc_code /
    /// buildcmd tests, so no residue may remain).
    #[test]
    fn unified_get_component_is_workspace_first_then_global() {
        use crate::db::infra::init::MCC_TEST_PARSE_LOCK;
        let _guard = MCC_TEST_PARSE_LOCK.lock().expect("test parse lock");

        struct Cleanup(McSpaceName);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                crate::db::defregistry::remove_by_uri(self.0.uri.as_uri().as_ref());
            }
        }

        let ident = McIds::from("GOLD_PRIO");
        let sn = McSpaceName::new(&ident, uri("/mcc/prio.mc"));
        let _cleanup = Cleanup(sn.clone());

        // Production order: the mcode lib loads first (SystemLib), then the
        // project file re-declares the identity (Project). The project def
        // must shadow the system def — workspace-first precedence.
        assert_eq!(
            crate::db::defregistry::insert(
                &sn,
                crate::db::defregistry::LoadDomain::SystemLib("mcode".to_string()),
                crate::db::defregistry::DefValue::Component(gold_component(
                    "GOLD_PRIO",
                    "/mcc/prio.mc",
                    1
                )),
            ),
            crate::db::defregistry::InsertOutcome::Inserted,
            "system-lib def registers first"
        );
        assert_eq!(
            crate::db::defregistry::insert(
                &sn,
                crate::db::defregistry::LoadDomain::Project,
                crate::db::defregistry::DefValue::Component(gold_component(
                    "GOLD_PRIO",
                    "/mcc/prio.mc",
                    2
                )),
            ),
            crate::db::defregistry::InsertOutcome::Inserted,
            "project def takes over the same-key system-lib def (workspace-first)"
        );

        let wm = WorkspaceManager::new();
        let ds = DefinitionSpace::of(&wm);
        let hit = ds.get_component(&sn).expect("identity resolves");
        assert_eq!(
            hit.pins.pins.len(),
            2,
            "workspace (project) def wins over the global (system-lib) def"
        );
        assert_eq!(
            ds.get_workspace_component(&sn).map(|c| c.pins.pins.len()),
            Some(2),
            "the project view sees the project def"
        );
        assert!(
            !ds.system_components().iter().any(|(k, _)| k == &sn),
            "the identity is no longer a system def after the takeover"
        );

        // A same-key re-insert cannot displace the project def: same-domain
        // (project) and reverse-domain (system lib) re-inserts are duplicates.
        assert_eq!(
            crate::db::defregistry::insert(
                &sn,
                crate::db::defregistry::LoadDomain::Project,
                crate::db::defregistry::DefValue::Component(gold_component(
                    "GOLD_PRIO",
                    "/mcc/prio.mc",
                    3
                )),
            ),
            crate::db::defregistry::InsertOutcome::Duplicate,
            "a project re-insert is a duplicate (first project def stays)"
        );
        assert_eq!(
            crate::db::defregistry::insert(
                &sn,
                crate::db::defregistry::LoadDomain::SystemLib("mcode".to_string()),
                crate::db::defregistry::DefValue::Component(gold_component(
                    "GOLD_PRIO",
                    "/mcc/prio.mc",
                    4
                )),
            ),
            crate::db::defregistry::InsertOutcome::Duplicate,
            "a system lib cannot displace a project def"
        );

        // The precedence is per-key, not per-kind: other kinds stay a miss.
        assert!(ds.get_module(&sn).is_none());
        assert!(ds.get_interface(&sn).is_none());
        assert!(ds.get_enum(&sn).is_none());
        assert!(ds.get_define(&sn).is_none());

        // Dropping the entry removes it entirely: the identity is gone.
        drop(_cleanup);
        assert!(
            ds.get_component(&sn).is_none(),
            "the identity is removed with the cleanup"
        );
    }
}
