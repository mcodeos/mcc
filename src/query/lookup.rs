// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::ast::ast_semantic::Span;
use crate::db::cmie::cmie::mcb_get_cmie_with_uri;
use crate::db::cmie::tables as workspace;
use crate::db::infra::global;
use crate::semantic::mc_func::HasFindInst;
use crate::semantic::module::McModule;
use crate::{McCMIE, McIds, McSpaceName, McURI};
use std::ops::Range;
use std::sync::Arc;

use crate::build::pass1::canonicalize_project_uri;
use crate::db::cmie::cmie::mcb_get_cmie;
// === pub fn unified_lookup(class_name: &str, from_uri: &McURI) -> Option<(McURI, Span ===
/// Unified lookup for pass1/pass2 and F12 — returns (uri, span) for goto-def.
/// Reuses Tier 1–4 resolution from mcb_get_cmie.
pub fn unified_lookup(class_name: &str, from_uri: &McURI) -> Option<(McURI, Span)> {
    let ids = McIds::from(class_name);
    let (cmie, source_uri) = mcb_get_cmie_with_uri(&ids, from_uri)?;
    let span = match &cmie {
        McCMIE::Component(c) => c.span.clone(),
        McCMIE::Module(m) => m.span.clone(),
        McCMIE::Interface(i) => i.span.clone(),
        McCMIE::Enum(e) => e.span[0] as usize..e.span[1] as usize,
    };
    Some((source_uri, span))
}

// === pub fn unified_lookup_with_scope( ===
/// Priority-based lookup using ScopePath.
///
/// Searches in 5 levels for a class-level (component/module/interface/enum/function)
/// definition matching `name`. Returns (uri, span, container_kind).
///
/// Priority: P1 (exact scope) → P2 (same container) → P3 (same file) →
///           P4 (use chain)   → P5 (project/libs).
pub fn unified_lookup_with_scope(
    name: &str,
    scope_path: &crate::ScopePath,
) -> Option<(McURI, Range<usize>, crate::ContainerKind)> {
    // P1-P2: search within current scope (container-aware)
    let ids = McIds::from(name);
    let (cmie, source_uri) = mcb_get_cmie_with_uri(&ids, &scope_path.uri)?;
    let span = match &cmie {
        McCMIE::Component(c) => c.span.clone(),
        McCMIE::Module(m) => m.span.clone(),
        McCMIE::Interface(i) => i.span.clone(),
        McCMIE::Enum(e) => e.span[0] as usize..e.span[1] as usize,
    };
    let kind = match &cmie {
        McCMIE::Component(_) => crate::ContainerKind::Component,
        McCMIE::Module(_) => crate::ContainerKind::Module,
        McCMIE::Interface(_) => crate::ContainerKind::Interface,
        McCMIE::Enum(_) => crate::ContainerKind::Enum,
    };
    Some((source_uri, span, kind))
}

// === pub fn lookup_with_sub( ===
/// Extended lookup: find a class definition, then optionally look up a sub-element
/// within it. Combines Phase 1 (parent container) and Phase 2 (sub-element) for
/// compound identifiers like `uC.PA1`.
pub fn lookup_with_sub(
    class_name: &str,
    sub_name: Option<&str>,
    sub_kind: Option<crate::SubElementKind>,
    from_uri: &McURI,
) -> Option<(McURI, Range<usize>)> {
    let (parent_uri, parent_span) = unified_lookup(class_name, from_uri)?;
    match (sub_name, sub_kind) {
        (Some(sub), Some(kind)) => {
            lookup_sub_def(&parent_uri, None, kind, sub).map(|span| (parent_uri, span))
        }
        _ => Some((parent_uri, parent_span)),
    }
}

// === pub fn unified_lookup_all( ===
/// Enumerate all visible symbols at a given ScopePath.
///
/// Searches in priority order (innermost → outermost):
///   1. Current function (params, labels)
///   2. Current container (ports, instances, functions)
///   3. Current file (modules, components, interfaces, enums)
///   4. Project files + use chain
///   5. System library (mcode)
///   6. Third-party libs
///
/// Returns up to `filter.limit` results, optionally filtered by kind and prefix.
pub fn unified_lookup_all(
    scope_path: &crate::ScopePath,
    filter: &crate::ScopeFilter,
) -> Vec<crate::LookupResult> {
    let max = filter.limit.unwrap_or(100);
    let mut results: Vec<crate::LookupResult> = Vec::new();

    // P1-P3: collect from workspace containers at this file
    collect_from_file(scope_path, filter, &mut results, max);

    // P4: project index (via mcb_get_cmie with all class names)
    collect_from_project(filter, &mut results, max);

    // P5-P6: system library + third-party (deferred to future enhancement)

    results.truncate(max);
    results
}

// === fn collect_from_file( ===
/// Collect symbols from the current file's containers.
pub(crate) fn collect_from_file(
    scope_path: &crate::ScopePath,
    filter: &crate::ScopeFilter,
    results: &mut Vec<crate::LookupResult>,
    max: usize,
) {
    let uri = &scope_path.uri;
    let uri_str = uri.as_str();

    // Scan modules
    if filter
        .kind
        .map_or(true, |k| k == crate::ContainerKind::Module)
    {
        for entry in workspace::WORKSPACE.modules.iter() {
            if entry.key().uri.as_str() != uri_str {
                continue;
            }
            let m = entry.value();
            add_result(
                results,
                max,
                crate::LookupResult {
                    uri: uri.clone(),
                    span: m.span.start..m.span.end,
                    kind: crate::LookupSymbolKind::Module,
                    container: Some(crate::ContainerInfo::new(
                        crate::ContainerKind::Module,
                        &m.name.to_string(),
                    )),
                    scope: m.name.to_string(),
                    name: m.name.to_string(),
                },
            );
            // Collect module ports and labels
            collect_module_symbols(m, scope_path, filter, results, max);
        }
    }

    // Scan components
    if filter
        .kind
        .map_or(true, |k| k == crate::ContainerKind::Component)
    {
        for entry in workspace::WORKSPACE.components.iter() {
            if entry.key().uri.as_str() != uri_str {
                continue;
            }
            let c = entry.value();
            add_result(
                results,
                max,
                crate::LookupResult {
                    uri: uri.clone(),
                    span: c.span.start..c.span.end,
                    kind: crate::LookupSymbolKind::Component,
                    container: Some(crate::ContainerInfo::new(
                        crate::ContainerKind::Component,
                        &c.name.to_string(),
                    )),
                    scope: c.name.to_string(),
                    name: c.name.to_string(),
                },
            );
            // Collect component params, pins, funcs
            collect_component_symbols(c, scope_path, filter, results, max);
        }
    }
}

// === fn collect_module_symbols( ===
/// Collect ports, labels, instances from a module's insts.
pub(crate) fn collect_module_symbols(
    m: &crate::McModule,
    scope_path: &crate::ScopePath,
    _filter: &crate::ScopeFilter,
    results: &mut Vec<crate::LookupResult>,
    max: usize,
) {
    for (name, span) in m.insts.port_spans().iter() {
        if let Some(spans) = span.first() {
            let kind = if m.insts.get_label_kind(name) == crate::LabelKind::Explicit {
                crate::LookupSymbolKind::Label
            } else {
                crate::LookupSymbolKind::Port
            };
            add_result(
                results,
                max,
                crate::LookupResult {
                    uri: scope_path.uri.clone(),
                    span: spans.clone(),
                    kind,
                    container: Some(scope_path.container.clone()),
                    scope: scope_path.scope_key(),
                    name: name.clone(),
                },
            );
        }
    }
    // Module funcs
    for func in m.funcs.iter() {
        add_result(
            results,
            max,
            crate::LookupResult {
                uri: scope_path.uri.clone(),
                span: 0..0, // funcs don't have individual spans
                kind: crate::LookupSymbolKind::Function,
                container: Some(scope_path.container.clone()),
                scope: format!("{}.{}", scope_path.container.name, func.name),
                name: func.name.to_string(),
            },
        );
    }
}

// === fn collect_component_symbols( ===
/// Collect params, pins, funcs from a component.
pub(crate) fn collect_component_symbols(
    c: &crate::McComponent,
    scope_path: &crate::ScopePath,
    _filter: &crate::ScopeFilter,
    results: &mut Vec<crate::LookupResult>,
    max: usize,
) {
    let scope = scope_path.scope_key();
    // Component params
    for (name, span) in c.params.iter_defs_with_span() {
        add_result(
            results,
            max,
            crate::LookupResult {
                uri: scope_path.uri.clone(),
                span,
                kind: crate::LookupSymbolKind::Param,
                container: Some(scope_path.container.clone()),
                scope: scope.clone(),
                name: name.to_string(),
            },
        );
    }
    // Component pins
    for (name, span) in &c.pins.pin_name_spans {
        add_result(
            results,
            max,
            crate::LookupResult {
                uri: scope_path.uri.clone(),
                span: span.clone(),
                kind: crate::LookupSymbolKind::Pin,
                container: Some(scope_path.container.clone()),
                scope: scope.clone(),
                name: name.clone(),
            },
        );
    }
    // Component funcs
    for func in c.funcs.iter() {
        add_result(
            results,
            max,
            crate::LookupResult {
                uri: scope_path.uri.clone(),
                span: 0..0,
                kind: crate::LookupSymbolKind::Function,
                container: Some(scope_path.container.clone()),
                scope: format!("{}.{}", scope, func.name),
                name: func.name.to_string(),
            },
        );
    }
}

// === fn collect_from_project( ===
/// Collect symbols from the project index (cross-file).
pub(crate) fn collect_from_project(
    _filter: &crate::ScopeFilter,
    results: &mut Vec<crate::LookupResult>,
    max: usize,
) {
    // Component classes
    for entry in workspace::WORKSPACE.components.iter() {
        let name = entry.key().ident.to_string();
        let uri = entry.key().uri.clone();
        if !results.iter().any(|r: &crate::LookupResult| r.name == name) {
            add_result(
                results,
                max,
                crate::LookupResult {
                    uri,
                    span: entry.value().span.start..entry.value().span.end,
                    kind: crate::LookupSymbolKind::Component,
                    container: None,
                    scope: String::new(),
                    name,
                },
            );
        }
    }
    // Module classes
    for entry in workspace::WORKSPACE.modules.iter() {
        let name = entry.key().ident.to_string();
        let uri = entry.key().uri.clone();
        if !results.iter().any(|r: &crate::LookupResult| r.name == name) {
            add_result(
                results,
                max,
                crate::LookupResult {
                    uri,
                    span: entry.value().span.start..entry.value().span.end,
                    kind: crate::LookupSymbolKind::Module,
                    container: None,
                    scope: String::new(),
                    name,
                },
            );
        }
    }
    // Interfaces
    for entry in workspace::WORKSPACE.interfaces.iter() {
        let name = entry.key().ident.to_string();
        add_result(
            results,
            max,
            crate::LookupResult {
                uri: entry.key().uri.clone(),
                span: entry.value().span.start..entry.value().span.end,
                kind: crate::LookupSymbolKind::Interface,
                container: None,
                scope: String::new(),
                name,
            },
        );
    }
    // Enums
    for entry in workspace::WORKSPACE.enums.iter() {
        let name = entry.key().ident.to_string();
        add_result(
            results,
            max,
            crate::LookupResult {
                uri: entry.key().uri.clone(),
                span: entry.value().span[0] as usize..entry.value().span[1] as usize,
                kind: crate::LookupSymbolKind::Enum,
                container: None,
                scope: String::new(),
                name,
            },
        );
    }
}

// === fn add_result(results: &mut Vec<crate::LookupResult>, max: usize, result: crate: ===
/// Add result if prefix matches and limit not reached.
pub(crate) fn add_result(
    results: &mut Vec<crate::LookupResult>,
    max: usize,
    result: crate::LookupResult,
) {
    if results.len() >= max {
        return;
    }
    results.push(result);
}

// ============================================================================
// ContainerRef + CmieKind — cross-library container discovery (Phase 4.5/5)
// ============================================================================

/// Kind of CMIE container — used to narrow the search scope in [`find_container`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmieKind {
    Component,
    Module,
    Interface,
    Enum,
    /// Search all container types (no narrowing).
    Any,
}

impl CmieKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Component => "component",
            Self::Module => "module",
            Self::Interface => "interface",
            Self::Enum => "enum",
            Self::Any => "any",
        }
    }
}

/// Reference to a resolved CMIE container — result of [`find_container`].
///
/// Holds an owned [`Arc`] to the container, avoiding lifetime coupling with
/// the DashMap iterator.
///
/// After obtaining a `ContainerRef`, callers delegate to
/// [`HasFindInst::find_inst_with_span`] for member name resolution
/// inside the container's internal namespace.
pub enum ContainerRef {
    Component(Arc<crate::semantic::component::McComponent>),
    Module(Arc<crate::semantic::module::McModule>),
    Interface(Arc<crate::semantic::mc_ifs::McInterface>),
    Enum(Arc<crate::semantic::mc_enum::McEnumDef>),
}

impl ContainerRef {
    /// Delegate to the container's [`HasFindInst::find_inst_with_span`].
    pub fn find_inst_with_span(
        &self,
        name: &str,
    ) -> Option<(crate::McInstance, Option<Range<usize>>)> {
        match self {
            ContainerRef::Component(c) => c.find_inst_with_span(name),
            ContainerRef::Module(m) => m.find_inst_with_span(name),
            ContainerRef::Interface(i) => i.find_inst_with_span(name),
            ContainerRef::Enum(e) => e.find_inst_with_span(name),
        }
    }
}

/// Cross-library container discovery — the bridging function between global
/// and CMIE-internal namespaces.
///
/// Search order:
///   1. `workspace.*` CMIE tables (project definitions)
///   2. `global::mcc_*` CMIE tables (system library definitions)
///
/// `kind_hint` narrows which DashMaps to search. Pass [`CmieKind::Any`] to
/// search all four container types.
pub fn find_container(name: &McIds, uri: &McURI, kind_hint: CmieKind) -> Option<ContainerRef> {
    let uri_str = uri.as_str();
    let cn = name.to_string();

    // ── Layer 1: workspace tables ──
    if matches!(kind_hint, CmieKind::Component | CmieKind::Any) {
        for entry in workspace::WORKSPACE.components.iter() {
            let key = entry.key();
            if key.uri.as_str() == uri_str && key.ident.to_string() == cn {
                return Some(ContainerRef::Component(entry.value().clone()));
            }
        }
    }
    if matches!(kind_hint, CmieKind::Module | CmieKind::Any) {
        for entry in workspace::WORKSPACE.modules.iter() {
            let key = entry.key();
            if key.uri.as_str() == uri_str && key.ident.to_string() == cn {
                return Some(ContainerRef::Module(entry.value().clone()));
            }
        }
    }
    if matches!(kind_hint, CmieKind::Interface | CmieKind::Any) {
        for entry in workspace::WORKSPACE.interfaces.iter() {
            let key = entry.key();
            if key.uri.as_str() == uri_str && key.ident.to_string() == cn {
                return Some(ContainerRef::Interface(entry.value().clone()));
            }
        }
    }
    if matches!(kind_hint, CmieKind::Enum | CmieKind::Any) {
        for entry in workspace::WORKSPACE.enums.iter() {
            let key = entry.key();
            if key.uri.as_str() == uri_str && key.ident.to_string() == cn {
                return Some(ContainerRef::Enum(entry.value().clone()));
            }
        }
    }

    // ── Layer 2: global (mcode system library) tables ──
    if matches!(kind_hint, CmieKind::Component | CmieKind::Any) {
        for entry in global::mcc_components.iter() {
            let key = entry.key();
            if key.uri.as_str() == uri_str && key.ident.to_string() == cn {
                return Some(ContainerRef::Component(entry.value().clone()));
            }
        }
    }
    if matches!(kind_hint, CmieKind::Module | CmieKind::Any) {
        for entry in global::mcc_modules.iter() {
            let key = entry.key();
            if key.uri.as_str() == uri_str && key.ident.to_string() == cn {
                return Some(ContainerRef::Module(entry.value().clone()));
            }
        }
    }
    if matches!(kind_hint, CmieKind::Interface | CmieKind::Any) {
        for entry in global::mcc_interfaces.iter() {
            let key = entry.key();
            if key.uri.as_str() == uri_str && key.ident.to_string() == cn {
                return Some(ContainerRef::Interface(entry.value().clone()));
            }
        }
    }
    if matches!(kind_hint, CmieKind::Enum | CmieKind::Any) {
        for entry in global::mcc_enums.iter() {
            let key = entry.key();
            if key.uri.as_str() == uri_str && key.ident.to_string() == cn {
                return Some(ContainerRef::Enum(entry.value().clone()));
            }
        }
    }

    None
}

/// Map [`SubElementKind`] to [`CmieKind`] for narrowing container search.
fn sub_kind_to_cmie_kind(kind: SubElementKind) -> CmieKind {
    match kind {
        SubElementKind::Pin => CmieKind::Component,
        SubElementKind::Port => CmieKind::Module,
        SubElementKind::Label => CmieKind::Module,
        SubElementKind::Param => CmieKind::Any,
        SubElementKind::Func => CmieKind::Any,
        SubElementKind::EnumValue => CmieKind::Enum,
    }
}

// === enum SubElementKind + impl ===
/// Kinds of sub-elements that can be looked up within a parent container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubElementKind {
    /// Component pin (e.g. `PA1` within `MCU.US513_20_F`)
    Pin,
    /// Module/component port in instances (e.g. `io VDD` within module)
    Port,
    /// Parameter declared in params section
    Param,
    /// Enum value within an enum definition
    EnumValue,
    /// Function defined within a module/component
    Func,
    /// Label (explicit or inline) within a module/component/function
    Label,
}

impl SubElementKind {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pin" => Some(Self::Pin),
            "port" => Some(Self::Port),
            "param" => Some(Self::Param),
            "enum_value" => Some(Self::EnumValue),
            "func" => Some(Self::Func),
            "label" => Some(Self::Label),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pin => "pin",
            Self::Port => "port",
            Self::Param => "param",
            Self::EnumValue => "enum_value",
            Self::Func => "func",
            Self::Label => "label",
        }
    }
}

// === pub fn lookup_sub_def( ===
/// Phase 2 lookup: find a sub-element (pin, port, param, enum value, func, label)
/// within a parent container identified by its definition URI and optional name.
///
/// Uses [`find_container`] for cross-library container discovery, then delegates
/// to [`HasFindInst::find_inst_with_span`] for member name resolution.
///
/// When `container_name` is [`None`], searches all containers of the matching
/// kind at `parent_uri`.
///
/// Returns the byte range of the sub-element within the container's source file.
pub fn lookup_sub_def(
    parent_uri: &McURI,
    container_name: Option<&str>,
    kind: SubElementKind,
    name: &str,
) -> Option<Range<usize>> {
    let cmie_kind = sub_kind_to_cmie_kind(kind);

    // Try a single container resolution when the name is known
    if let Some(cn) = container_name {
        let ids = McIds::from(cn);
        let container = find_container(&ids, parent_uri, cmie_kind)?;
        let (inst, span) = container.find_inst_with_span(name)?;
        return kind_matches_instance(kind, &inst).then_some(span)?;
    }

    // ── container_name is None: search all matching containers at this URI ──
    let uri_str = parent_uri.as_str();
    let try_container = |container: &ContainerRef| {
        let (inst, span) = container.find_inst_with_span(name)?;
        kind_matches_instance(kind, &inst).then_some(span)?
    };

    if matches!(cmie_kind, CmieKind::Component | CmieKind::Any) {
        for entry in workspace::WORKSPACE.components.iter() {
            if entry.key().uri.as_str() == uri_str {
                if let Some(span) = try_container(&ContainerRef::Component(entry.value().clone())) {
                    return Some(span);
                }
            }
        }
        for entry in global::mcc_components.iter() {
            if entry.key().uri.as_str() == uri_str {
                if let Some(span) = try_container(&ContainerRef::Component(entry.value().clone())) {
                    return Some(span);
                }
            }
        }
    }
    if matches!(cmie_kind, CmieKind::Module | CmieKind::Any) {
        for entry in workspace::WORKSPACE.modules.iter() {
            if entry.key().uri.as_str() == uri_str {
                if let Some(span) = try_container(&ContainerRef::Module(entry.value().clone())) {
                    return Some(span);
                }
            }
        }
        for entry in global::mcc_modules.iter() {
            if entry.key().uri.as_str() == uri_str {
                if let Some(span) = try_container(&ContainerRef::Module(entry.value().clone())) {
                    return Some(span);
                }
            }
        }
    }
    if matches!(cmie_kind, CmieKind::Interface | CmieKind::Any) {
        for entry in workspace::WORKSPACE.interfaces.iter() {
            if entry.key().uri.as_str() == uri_str {
                if let Some(span) = try_container(&ContainerRef::Interface(entry.value().clone())) {
                    return Some(span);
                }
            }
        }
        for entry in global::mcc_interfaces.iter() {
            if entry.key().uri.as_str() == uri_str {
                if let Some(span) = try_container(&ContainerRef::Interface(entry.value().clone())) {
                    return Some(span);
                }
            }
        }
    }
    if matches!(cmie_kind, CmieKind::Enum | CmieKind::Any) {
        for entry in workspace::WORKSPACE.enums.iter() {
            if entry.key().uri.as_str() == uri_str {
                if let Some(span) = try_container(&ContainerRef::Enum(entry.value().clone())) {
                    return Some(span);
                }
            }
        }
        for entry in global::mcc_enums.iter() {
            if entry.key().uri.as_str() == uri_str {
                if let Some(span) = try_container(&ContainerRef::Enum(entry.value().clone())) {
                    return Some(span);
                }
            }
        }
    }

    None
}

// === fn find_param_def_span( ===
/// Helper: find a param def span by name using the public iterator.
pub(crate) fn find_param_def_span(
    params: &crate::semantic::basic::mc_param::McParamDeclares,
    name: &str,
) -> Option<Range<usize>> {
    for (n, span) in params.iter_defs_with_span() {
        if n == name {
            return Some(span);
        }
    }
    None
}

// === fn find_param_port_span( ===
/// Helper: find a param port span by name using the public iterator.
pub(crate) fn find_param_port_span(
    params: &crate::semantic::basic::mc_param::McParamDeclares,
    name: &str,
) -> Option<Range<usize>> {
    for (n, span) in params.iter_ports_with_span() {
        if n == name {
            return Some(span);
        }
    }
    None
}

/// Validate that a resolved [`McInstance`] variant matches the expected [`SubElementKind`].
fn kind_matches_instance(kind: SubElementKind, inst: &crate::McInstance) -> bool {
    match kind {
        SubElementKind::Pin => matches!(inst, crate::McInstance::Label(_)),
        SubElementKind::Port => matches!(inst, crate::McInstance::Label(_)),
        SubElementKind::Label => matches!(inst, crate::McInstance::Label(_)),
        SubElementKind::Param => matches!(inst, crate::McInstance::Label(_)),
        SubElementKind::Func => matches!(inst, crate::McInstance::Func(_)),
        SubElementKind::EnumValue => matches!(inst, crate::McInstance::EnumVal { .. }),
    }
}

// === fn find_component_uri(class_name: &McIds) -> Option<McURI> { ===
/// Find source URI of component definition
pub(crate) fn find_component_uri(class_name: &McIds) -> Option<McURI> {
    let name_str = class_name.to_string();
    for entry in workspace::WORKSPACE.components.iter() {
        let ident_str = entry.key().ident.to_string();
        if ident_str == name_str {
            return Some(entry.key().uri.clone());
        }
    }
    None
}

// === fn find_in_project_tables(space_name: &McSpaceName) -> Option<McCMIE> { ===
/// Look up CMIE in project global table (via McSpaceName)
pub(crate) fn find_in_project_tables(space_name: &McSpaceName) -> Option<McCMIE> {
    let canonical_uri = canonicalize_project_uri(&space_name.uri);
    let canonical_space_name = McSpaceName {
        ident: space_name.ident.clone(),
        uri: canonical_uri,
    };
    // eprintln!(
    //     "[DIAG find_in_project_tables] searching ident='{}', uri='{}' -> canonical='{}'",
    //     space_name.ident.to_string(),
    //     space_name.uri,
    //     canonical_space_name.uri
    // );
    if let Some(comp) = workspace::WORKSPACE.components.get(&canonical_space_name) {
        return Some(McCMIE::Component(comp.clone()));
    }
    if let Some(comp) = global::mcc_components.get(&canonical_space_name) {
        return Some(McCMIE::Component(comp.clone()));
    }
    if let Some(module) = workspace::WORKSPACE.modules.get(&canonical_space_name) {
        return Some(McCMIE::Module(module.clone()));
    }
    if let Some(module) = global::mcc_modules.get(&canonical_space_name) {
        return Some(McCMIE::Module(module.clone()));
    }
    if let Some(ifs) = workspace::WORKSPACE.interfaces.get(&canonical_space_name) {
        return Some(McCMIE::Interface(ifs.clone()));
    }
    if let Some(ifs) = global::mcc_interfaces.get(&canonical_space_name) {
        return Some(McCMIE::Interface(ifs.clone()));
    }
    if let Some(enum_def) = global::mcc_enums.get(&canonical_space_name) {
        return Some(McCMIE::Enum(enum_def.clone()));
    }
    if let Some(enum_def) = workspace::WORKSPACE.enums.get(&canonical_space_name) {
        return Some(McCMIE::Enum(enum_def.clone()));
    }
    None
}

// === fn find_by_name_in_project_tables(class_name: &McIds) -> Option<McCMIE> { ===
/// Look up directly in the global table by name (ignoring URI)
pub(crate) fn find_by_name_in_project_tables(class_name: &McIds) -> Option<McCMIE> {
    let name_str = class_name.to_string();
    // Check components (exact match)
    for entry in workspace::WORKSPACE.components.iter() {
        let ident_str = entry.key().ident.to_string();
        if ident_str == name_str {
            return Some(McCMIE::Component(entry.value().clone()));
        }
    }
    for entry in global::mcc_components.iter() {
        let ident_str = entry.key().ident.to_string();
        if name_str == "DIO.ESD" {
            mcc_dbg!(
                "lsp::query",
                "[CMIE-LOOKUP] global component ident={ident_str} name={name_str}"
            );
        }
        if ident_str == name_str {
            return Some(McCMIE::Component(entry.value().clone()));
        }
    }

    // Check modules (exact match)
    for entry in workspace::WORKSPACE.modules.iter() {
        let ident_str = entry.key().ident.to_string();
        if ident_str == name_str {
            return Some(McCMIE::Module(entry.value().clone()));
        }
    }
    for entry in global::mcc_modules.iter() {
        let ident_str = entry.key().ident.to_string();
        if ident_str == name_str {
            return Some(McCMIE::Module(entry.value().clone()));
        }
    }

    // Check interfaces
    for entry in workspace::WORKSPACE.interfaces.iter() {
        let ident_str = entry.key().ident.to_string();
        if ident_str == name_str {
            return Some(McCMIE::Interface(entry.value().clone()));
        }
    }
    for entry in global::mcc_interfaces.iter() {
        let ident_str = entry.key().ident.to_string();
        if ident_str == name_str {
            return Some(McCMIE::Interface(entry.value().clone()));
        }
    }

    // Check enums
    for entry in global::mcc_enums.iter() {
        let ident_str = entry.key().ident.to_string();
        if ident_str == name_str {
            return Some(McCMIE::Enum(entry.value().clone()));
        }
    }
    for entry in workspace::WORKSPACE.enums.iter() {
        let ident_str = entry.key().ident.to_string();
        if ident_str == name_str {
            return Some(McCMIE::Enum(entry.value().clone()));
        }
    }

    None
}

// === pub(crate) fn mcb_find_module_uri(class_name: &McIds) -> Option<McURI> { ===
/// Find the source URI of a module definition (for setting current_uri context in Pass2)
///
/// Look up by name in prj_modules, return the URI of the file containing the module definition.
/// This is critical for cross-file module instantiation: symbol resolution inside submodules
/// must occur in the context of their defining file.
pub(crate) fn mcb_find_module_uri(class_name: &McIds) -> Option<McURI> {
    let name_str = class_name.to_string();
    for entry in workspace::WORKSPACE.modules.iter() {
        let ident_str = entry.key().ident.to_string();
        if ident_str == name_str {
            return Some(entry.key().uri.clone());
        }
    }
    None
}

// === pub fn mcb_get_module_def_by_name(class_name: &McIds) -> Option<Arc<McModule>> { ===
/// 🆕 New API: directly look up module by name from prj_modules (bypasses mcb_get_cmie's URI matching issue)
///
/// This is the most reliable way to get a module definition, accessing the global table directly.
/// When mcb_get_cmie fails due to URI mismatch, use this function as a fallback.
pub fn mcb_get_module_def_by_name(class_name: &McIds) -> Option<Arc<McModule>> {
    let name_str = class_name.to_string();

    // Exact match
    for entry in workspace::WORKSPACE.modules.iter() {
        let ident_str = entry.key().ident.to_string();
        if ident_str == name_str {
            return Some(entry.value().clone());
        }
    }

    None
}

// === pub fn mcb_get_module_with_diagnostics( ===
/// 🆕 New API: get module definition with diagnostic information
///
/// Returns (module, diagnostics) tuple
/// diagnostics contains all information during the lookup process for easier troubleshooting
pub fn mcb_get_module_with_diagnostics(
    class_name: &McIds,
    uri: &McURI,
) -> (Option<Arc<McModule>>, Vec<String>) {
    let mut diags = Vec::new();
    let name_str = class_name.to_string();

    // 1. First try the standard path
    if let Some(McCMIE::Module(module)) = mcb_get_cmie(class_name, uri) {
        if module.lines.is_empty() && module.insts.iter().count() == 0 {
            diags.push(
                "⚠️  mcb_get_cmie returned an empty module (lines=0, symbols=0), trying fallback"
                    .to_string(),
            );
            // Standard path returned an empty module, go to fallback
        } else {
            diags.push(format!(
                "✅ mcb_get_cmie success: lines={}, symbols={}",
                module.lines.len(),
                module.insts.iter().count()
            ));
            return (Some(module), diags);
        }
    } else {
        diags.push("❌ mcb_get_cmie returned None".to_string());
    }

    // 2. Fallback: look up directly by name
    if let Some(module) = mcb_get_module_def_by_name(class_name) {
        diags.push(format!(
            "✅ fallback mcb_get_module_def_by_name success: lines={}, symbols={}",
            module.lines.len(),
            module.insts.iter().count()
        ));
        return (Some(module), diags);
    }

    diags.push(format!("❌ fallback also did not find module '{name_str}'"));

    // 3. List all known modules for reference
    let modules = &workspace::WORKSPACE.modules;
    diags.push(format!("Registered modules ({}):", modules.len()));
    for entry in modules.iter() {
        diags.push(format!(
            "  - {} @ {} (lines={}, symbols={})",
            entry.key().ident,
            entry.key().uri,
            entry.value().lines.len(),
            entry.value().insts.iter().count()
        ));
    }
    (None, diags)
}
