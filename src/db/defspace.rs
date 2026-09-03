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
use crate::db::defregistry::{
    peel_capabilities, peel_components, peel_defines, peel_enums, peel_interfaces, peel_modules,
    DefKind, DomainFilter,
};
use crate::db::infra::mc_code::McCode;
use crate::semantic::capability::McCapability;
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
    //
    // T3 (bounded close-out): every definition read below goes through the
    // registry of the world this `DefinitionSpace` was built over — never the
    // process-global one — so an isolated workspace's view cannot leak into
    // another world (regression: `isolated_worlds_do_not_leak_definitions`).

    /// Look up a component by its `McSpaceName`. Precedence is
    /// registry-internal (design §12.4 rule 1): the workspace def shadows a
    /// same-key system-lib def (workspace-first, P0.1 — the mcode lib loads
    /// first, then a project file re-declares the identity and wins).
    pub fn get_component(&self, sn: &McSpaceName) -> Option<Arc<McComponent>> {
        self.ws.registry().get_component(sn)
    }

    /// Look up a module by its `McSpaceName`.
    pub fn get_module(&self, sn: &McSpaceName) -> Option<Arc<McModule>> {
        self.ws.registry().get_module(sn)
    }

    /// Look up an interface by its `McSpaceName`.
    pub fn get_interface(&self, sn: &McSpaceName) -> Option<Arc<McInterface>> {
        self.ws.registry().get_interface(sn)
    }

    /// Look up an enum by its `McSpaceName`.
    pub fn get_enum(&self, sn: &McSpaceName) -> Option<Arc<McEnumDef>> {
        self.ws.registry().get_enum(sn)
    }

    /// Look up a define by its `McSpaceName`.
    pub fn get_define(&self, sn: &McSpaceName) -> Option<Arc<McDefineDef>> {
        self.ws.registry().get_define(sn)
    }

    /// Look up a capability by its `McSpaceName`. Capability is not a class
    /// kind (never in the system name index); this typed registry read is the
    /// link-time resolution path (`::` adoption, P2).
    pub fn get_capability(&self, sn: &McSpaceName) -> Option<Arc<McCapability>> {
        self.ws.registry().get_capability(sn)
    }

    // ── Unified definition view: whole-table enumeration ──

    /// Enumerate every live component definition (any domain). The single
    /// registry holds one identity per `(uri, ident)`, so no dedup is needed.
    pub fn all_components(&self) -> Vec<(McSpaceName, Arc<McComponent>)> {
        peel_components(
            self.ws
                .registry()
                .enumerate(DefKind::Component, DomainFilter::Any),
        )
    }

    /// Enumerate every live module definition (any domain).
    pub fn all_modules(&self) -> Vec<(McSpaceName, Arc<McModule>)> {
        peel_modules(
            self.ws
                .registry()
                .enumerate(DefKind::Module, DomainFilter::Any),
        )
    }

    /// Enumerate every live interface definition (any domain).
    pub fn all_interfaces(&self) -> Vec<(McSpaceName, Arc<McInterface>)> {
        peel_interfaces(
            self.ws
                .registry()
                .enumerate(DefKind::Interface, DomainFilter::Any),
        )
    }

    /// Enumerate every live enum definition (any domain).
    pub fn all_enums(&self) -> Vec<(McSpaceName, Arc<McEnumDef>)> {
        peel_enums(
            self.ws
                .registry()
                .enumerate(DefKind::Enum, DomainFilter::Any),
        )
    }

    /// Enumerate every live define definition (any domain).
    pub fn all_defines(&self) -> Vec<(McSpaceName, Arc<McDefineDef>)> {
        peel_defines(
            self.ws
                .registry()
                .enumerate(DefKind::Define, DomainFilter::Any),
        )
    }

    /// Enumerate every live capability definition (any domain).
    pub fn all_capabilities(&self) -> Vec<(McSpaceName, Arc<McCapability>)> {
        peel_capabilities(
            self.ws
                .registry()
                .enumerate(DefKind::Capability, DomainFilter::Any),
        )
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
        self.ws.registry().get_workspace_component(sn)
    }

    /// Look up a module by its `McSpaceName` in the project domain only.
    pub fn get_workspace_module(&self, sn: &McSpaceName) -> Option<Arc<McModule>> {
        self.ws.registry().get_workspace_module(sn)
    }

    /// Look up an interface by its `McSpaceName` in the project domain only.
    pub fn get_workspace_interface(&self, sn: &McSpaceName) -> Option<Arc<McInterface>> {
        self.ws.registry().get_workspace_interface(sn)
    }

    /// Look up an enum by its `McSpaceName` in the project domain only.
    pub fn get_workspace_enum(&self, sn: &McSpaceName) -> Option<Arc<McEnumDef>> {
        self.ws.registry().get_workspace_enum(sn)
    }

    /// Look up a define by its `McSpaceName` in the project domain only.
    pub fn get_workspace_define(&self, sn: &McSpaceName) -> Option<Arc<McDefineDef>> {
        self.ws.registry().get_workspace_define(sn)
    }

    /// Look up a capability by its `McSpaceName` in the project domain only.
    pub fn get_workspace_capability(&self, sn: &McSpaceName) -> Option<Arc<McCapability>> {
        self.ws.registry().get_workspace_capability(sn)
    }

    /// Enumerate every project (workspace) component definition.
    pub fn workspace_components(&self) -> Vec<(McSpaceName, Arc<McComponent>)> {
        peel_components(
            self.ws
                .registry()
                .enumerate(DefKind::Component, DomainFilter::Project),
        )
    }

    /// Enumerate every project (workspace) module definition.
    pub fn workspace_modules(&self) -> Vec<(McSpaceName, Arc<McModule>)> {
        peel_modules(
            self.ws
                .registry()
                .enumerate(DefKind::Module, DomainFilter::Project),
        )
    }

    /// Enumerate every project (workspace) interface definition.
    pub fn workspace_interfaces(&self) -> Vec<(McSpaceName, Arc<McInterface>)> {
        peel_interfaces(
            self.ws
                .registry()
                .enumerate(DefKind::Interface, DomainFilter::Project),
        )
    }

    /// Enumerate every project (workspace) enum definition.
    pub fn workspace_enums(&self) -> Vec<(McSpaceName, Arc<McEnumDef>)> {
        peel_enums(
            self.ws
                .registry()
                .enumerate(DefKind::Enum, DomainFilter::Project),
        )
    }

    /// Enumerate every project (workspace) define definition.
    pub fn workspace_defines(&self) -> Vec<(McSpaceName, Arc<McDefineDef>)> {
        peel_defines(
            self.ws
                .registry()
                .enumerate(DefKind::Define, DomainFilter::Project),
        )
    }

    /// Enumerate every project (workspace) capability definition.
    pub fn workspace_capabilities(&self) -> Vec<(McSpaceName, Arc<McCapability>)> {
        peel_capabilities(
            self.ws
                .registry()
                .enumerate(DefKind::Capability, DomainFilter::Project),
        )
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
        self.ws.registry().system_contains(sn)
    }

    /// Enumerate every *system-library* component definition (P5).
    pub fn system_components(&self) -> Vec<(McSpaceName, Arc<McComponent>)> {
        peel_components(
            self.ws
                .registry()
                .enumerate(DefKind::Component, DomainFilter::System),
        )
    }

    /// Enumerate every *system-library* module definition (P5).
    pub fn system_modules(&self) -> Vec<(McSpaceName, Arc<McModule>)> {
        peel_modules(
            self.ws
                .registry()
                .enumerate(DefKind::Module, DomainFilter::System),
        )
    }

    /// Enumerate every *system-library* interface definition (P5).
    pub fn system_interfaces(&self) -> Vec<(McSpaceName, Arc<McInterface>)> {
        peel_interfaces(
            self.ws
                .registry()
                .enumerate(DefKind::Interface, DomainFilter::System),
        )
    }

    /// Enumerate every *system-library* enum definition (P5).
    pub fn system_enums(&self) -> Vec<(McSpaceName, Arc<McEnumDef>)> {
        peel_enums(
            self.ws
                .registry()
                .enumerate(DefKind::Enum, DomainFilter::System),
        )
    }

    /// Enumerate every *system-library* capability definition (P5).
    pub fn system_capabilities(&self) -> Vec<(McSpaceName, Arc<McCapability>)> {
        peel_capabilities(
            self.ws
                .registry()
                .enumerate(DefKind::Capability, DomainFilter::System),
        )
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
            is_abstract: false,
            variant_base: None,
            adopts: Vec::new(),
        })
    }

    /// The end-to-end wiring (loader records the manifest into the global
    /// workspace) lives in `tests/defspace_wiring.rs`, a separate test binary —
    /// the lib unit tests here must NOT mutate the process-global `WORKSPACE`
    /// (the parallel mc_code/buildcmd tests share it). These tests construct an
    /// isolated [`WorkspaceManager`] instead — each owns its own registry
    /// (`workspace.registry()`), and reads through the view never touch the
    /// process-global one. The one deliberate exception is
    /// `unified_get_component_is_workspace_first_then_global` below, which pins
    /// the active-world (production) path: it runs under `MCC_TEST_PARSE_LOCK`
    /// and removes its dedicated key on drop.
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
    /// unified `get_*` lookup resolves a key's project layer first, falling
    /// back to the intact same-key system-lib layer only on a project miss —
    /// the workspace-first precedence the Phase 3 single-table merge must
    /// preserve. This pins the *active-world* (production) path on purpose:
    /// the free defregistry write/remove entries serve the process-global
    /// `WORKSPACE`, so the reads bind to that same world through
    /// `definition_space()` — the one call surface the T3 bounded close-out
    /// leaves process-global by design. Reads over a constructed world go to
    /// that world's own registry (`isolated_worlds_do_not_leak_definitions`).
    /// The key is dedicated and removed on drop: the process-global state is
    /// shared with the parallel mc_code / buildcmd tests, so no residue may
    /// remain.
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
        // project file re-declares the identity (Project). Under T8 (M2)
        // layered coexist the project def does NOT destroy the system def —
        // both layers stay live under one key, and reads resolve the project
        // layer first (workspace-first precedence), falling back to the
        // intact system layer when the project file goes away.
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
            "project def layers over the same-key system-lib def (workspace-first read)"
        );

        // Bind the reads to the process-global world the free write/remove
        // entries above serve — a fresh constructed world owns an empty
        // registry and would see nothing.
        let ds = crate::definition_space();
        let hit = ds.get_component(&sn).expect("identity resolves");
        assert_eq!(
            hit.pins.pins.len(),
            2,
            "the project (workspace) layer wins the unified read"
        );
        assert_eq!(
            ds.get_workspace_component(&sn).map(|c| c.pins.pins.len()),
            Some(2),
            "the project view sees the project layer"
        );
        assert!(
            ds.system_components().iter().any(|(k, _)| k == &sn),
            "T8: the system layer survives the project shadow (layered, not overwritten)"
        );

        // A same-key re-insert cannot displace the project layer: same-domain
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
            "a project re-insert is a duplicate (first project layer stays)"
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
            "a system lib cannot displace a live project layer"
        );

        // The precedence is per-key, not per-kind: other kinds stay a miss.
        assert!(ds.get_module(&sn).is_none());
        assert!(ds.get_interface(&sn).is_none());
        assert!(ds.get_enum(&sn).is_none());
        assert!(ds.get_define(&sn).is_none());

        // Removing the project file's layer alone falls back to the intact
        // system def — the T8 fallback read, no mcode reload required.
        crate::db::defregistry::remove_project_by_uri("/mcc/prio.mc");
        let fallback = ds
            .get_component(&sn)
            .expect("system layer survives the shadow");
        assert_eq!(
            fallback.pins.pins.len(),
            1,
            "removing the project layer falls back to the system def"
        );
        assert!(
            ds.get_workspace_component(&sn).is_none(),
            "the project view is empty once the project layer is gone"
        );

        // Full cleanup removes the remaining system layer too.
        drop(_cleanup);
        assert!(
            ds.get_component(&sn).is_none(),
            "the identity is gone after removing the last layer"
        );
        assert!(
            !ds.system_components().iter().any(|(k, _)| k == &sn),
            "the system-only view is empty after the full cleanup"
        );
    }

    /// T3 (bounded close-out) regression: a `DefinitionSpace` view reads the
    /// registry of the world it was built over — never the process-global
    /// one. Two constructed worlds register the *same identity* with
    /// different content and in different domains; each world's view must
    /// return exactly its own def, keep its own domain segmentation, and
    /// never see the other world's defs. Before the close-out the view
    /// methods fell through to the free defregistry API (the process-global
    /// active world), which makes these asserts fail deterministically: the
    /// worlds hold unique keys the process-global registry never contains.
    #[test]
    fn isolated_worlds_do_not_leak_definitions() {
        let wm_a = WorkspaceManager::new();
        let wm_b = WorkspaceManager::new();

        // The same identity X in both worlds — different content (pin count)
        // and different domains: project in A, system lib in B.
        let sn_x = McSpaceName::new(&McIds::from("ISO_LEAK_X"), uri("/iso/x.mc"));
        // Identities registered in exactly one world.
        let sn_a_only = McSpaceName::new(&McIds::from("ISO_LEAK_A_ONLY"), uri("/iso/a.mc"));
        let sn_b_only = McSpaceName::new(&McIds::from("ISO_LEAK_B_ONLY"), uri("/iso/b.mc"));

        use crate::db::defregistry::{DefValue, InsertOutcome, LoadDomain};
        assert_eq!(
            wm_a.registry().insert(
                &sn_x,
                &LoadDomain::Project,
                &DefValue::Component(gold_component("ISO_LEAK_X", "/iso/x.mc", 2)),
            ),
            InsertOutcome::Inserted
        );
        assert_eq!(
            wm_a.registry().insert(
                &sn_a_only,
                &LoadDomain::Project,
                &DefValue::Component(gold_component("ISO_LEAK_A_ONLY", "/iso/a.mc", 1)),
            ),
            InsertOutcome::Inserted
        );
        assert_eq!(
            wm_b.registry().insert(
                &sn_x,
                &LoadDomain::SystemLib("acme".to_string()),
                &DefValue::Component(gold_component("ISO_LEAK_X", "/iso/x.mc", 7)),
            ),
            InsertOutcome::Inserted
        );
        assert_eq!(
            wm_b.registry().insert(
                &sn_b_only,
                &LoadDomain::SystemLib("acme".to_string()),
                &DefValue::Component(gold_component("ISO_LEAK_B_ONLY", "/iso/b.mc", 1)),
            ),
            InsertOutcome::Inserted
        );

        let ds_a = DefinitionSpace::of(&wm_a);
        let ds_b = DefinitionSpace::of(&wm_b);

        // Same key, per-world content: each view resolves its own world's def.
        assert_eq!(
            ds_a.get_component(&sn_x).map(|c| c.pins.pins.len()),
            Some(2),
            "world A's view resolves A's own project def of X"
        );
        assert_eq!(
            ds_b.get_component(&sn_x).map(|c| c.pins.pins.len()),
            Some(7),
            "world B's view resolves B's own system def of X — the shared key never crosses worlds"
        );

        // A def registered in one world is invisible in the other, through
        // every view (typed lookup, whole-table enumeration).
        assert!(ds_a.get_component(&sn_b_only).is_none());
        assert!(ds_b.get_component(&sn_a_only).is_none());
        assert!(
            !ds_a.all_components().iter().any(|(k, _)| k == &sn_b_only),
            "A's enumeration never sees B's def"
        );
        assert!(
            !ds_b.all_components().iter().any(|(k, _)| k == &sn_a_only),
            "B's enumeration never sees A's def"
        );
        assert_eq!(
            ds_a.all_components().len(),
            2,
            "A holds exactly its two defs"
        );
        assert_eq!(
            ds_b.all_components().len(),
            2,
            "B holds exactly its two defs"
        );

        // Domain segmentation is per world: X is a project def in A and a
        // system-lib def in B; each world's domain views agree with its own
        // registration only.
        assert_eq!(
            ds_a.workspace_components()
                .iter()
                .filter(|(k, _)| k == &sn_x)
                .count(),
            1,
            "A's project view holds its project layer of X"
        );
        assert!(
            ds_b.workspace_components().iter().all(|(k, _)| k != &sn_x),
            "B registered X as a system def — B's project view is empty for X"
        );
        assert!(
            ds_a.system_components().iter().all(|(k, _)| k != &sn_x),
            "A registered X as a project def — A's system view is empty for X"
        );
        assert_eq!(
            ds_b.system_components()
                .iter()
                .filter(|(k, _)| k == &sn_x)
                .count(),
            1,
            "B's system view holds its system layer of X"
        );
        assert!(!ds_a.system_contains(&sn_x), "A's registry has no system X");
        assert!(
            ds_b.system_contains(&sn_x),
            "B's registry owns the system X"
        );
    }
}
