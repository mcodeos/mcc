// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Modular name-resolution scopes (design: name-resolution-chain-modular.md §3).
//!
//! The chain definition and the per-category resolution are split apart:
//!
//! - [`ScopeChain`] / [`ResolveScope`] — the **composition mechanism**:
//!   an ordered chain of scope units resolved with first-hit-wins semantics
//!   (`find_map`), which is provably equivalent to every `HasFindInst`
//!   implementation (each category returned `Some(...)` immediately).
//! - Definition-layer scope units (§3.2) — each category (params, pins,
//!   attrs, funcs, ...) is an independent [`ResolveScope`], reading the
//!   semantic tables built from AST directly (no text re-parsing).
//! - Container chain builders (§3.3) — a container is the ordered
//!   composition of its category units.
//! - Two chains (§3.4) — instance chain (P1-P2, returns `McInstance`) and
//!   class chain (P3-P5, returns a container reference) are never mixed;
//!   [`first_hop`] combines them into a [`BaseResolved`] result.
//!
//! ## Behavior guarantee
//!
//! Every scope unit copies the exact hit logic of the category it replaces
//! (including stored spans). The rule set (§1 of the design doc) is
//! unchanged; only the organization of the resolution logic changes.

use std::ops::Range;
use std::sync::Arc;

use crate::db::cmie::tables as workspace;
use crate::db::infra::init::interface_lookup;
use crate::query::lookup::ContainerRef;
use crate::semantic::basic::mc_bus::McBus;
use crate::semantic::basic::mc_ids::McIds;
use crate::semantic::basic::mc_paramd::McParamDeclares;
use crate::semantic::common::IOType;
use crate::semantic::component::mc_attr::McAttributes;
use crate::semantic::component::mc_pins::{McPinPort, McPins};
use crate::semantic::component::{find_scoped_enum_value, port_to_instance, McComponent};
use crate::semantic::mc_enum::{McEnumDef, McEnumValue};
use crate::semantic::mc_func::{HasFindInst, McFunctions};
use crate::semantic::mc_ifs::McInterface;
use crate::semantic::mc_inst::{McInstance, McInstances};
use crate::semantic::module::McModule;
use crate::{McCMIE, McSpaceName, McURI};

// ============================================================================
// Core abstractions (§3.1)
// ============================================================================

/// Resolution result: semantic object + span.
///
/// The span comes from the semantic tables (stored at parse time from the
/// AST); it is `None` only for categories that have no stored definition span
/// (func names, func params — see §3.7).
#[derive(Debug, Clone)]
pub struct Resolved {
    pub inst: McInstance,
    pub span: Option<Range<usize>>,
}

/// A composable scope unit on a name-resolution chain.
///
/// Implementations must be side-effect free on the definition layer (no
/// diagnostics); resolution failure is reported by the caller (§3.7.1).
pub trait ResolveScope<T> {
    /// Resolve `name` within this scope, returning `None` on a miss.
    fn resolve(&self, name: &str) -> Option<T>;
}

/// Ordered scope chain — first hit wins, which is exactly the shadowing
/// semantics of the original hand-written priority chains.
///
/// `T` is generic so the same mechanism serves both layers:
/// definition layer (`T = Resolved`, Pass1 / LSP) and instance layer
/// (`T = NetPoint` / `T = InstEntry`, Pass2).
pub struct ScopeChain<'a, T> {
    scopes: Vec<Box<dyn ResolveScope<T> + 'a>>,
}

impl<'a, T> ScopeChain<'a, T> {
    /// Build a chain from ordered scope units. Earlier units shadow later ones.
    pub fn new(scopes: Vec<Box<dyn ResolveScope<T> + 'a>>) -> Self {
        Self { scopes }
    }

    /// First-hit-wins resolution over the ordered units, with the read-side
    /// canonical fallback (§2.1/§4.1): when the ordered units all miss, a
    /// spelling that denotes exactly one bare member (`res[4]` → `res4`) is
    /// retried under that canonical member name — the storage keys are the
    /// expanded member names, so the retry is the same coordinate.
    pub fn resolve(&self, name: &str) -> Option<T> {
        if let Some(hit) = self.scopes.iter().find_map(|s| s.resolve(name)) {
            return Some(hit);
        }
        // Conservative bound: `canonical_single` returns None for anything
        // but a single bare identifier (dotted/bracketed/curly members), so a
        // miss never turns into a wrong hit.
        let canon = crate::semantic::basic::equivalent::canonical_single(
            &crate::semantic::basic::mc_ids::McIds::from(name),
        );
        match canon {
            Some(c) if c != name => self.scopes.iter().find_map(|s| s.resolve(&c)),
            _ => None,
        }
    }
}

impl<T> ResolveScope<T> for ScopeChain<'_, T> {
    fn resolve(&self, name: &str) -> Option<T> {
        // Delegate to the inherent method so the canonical fallback applies
        // uniformly when the chain is itself a unit of a larger chain.
        ScopeChain::resolve(self, name)
    }
}

// ============================================================================
// Definition-layer scope units (§3.2)
// ============================================================================
// Each unit copies the exact hit logic of one category of the original
// `find_inst_with_span` implementations. Spans come from the stored
// semantic tables; nothing is re-parsed from text.

/// P1 func params / component-interface params: `iter_defs_with_span` match.
pub struct ParamsScope<'a> {
    params: &'a McParamDeclares,
}

impl<'a> ParamsScope<'a> {
    pub fn new(params: &'a McParamDeclares) -> Self {
        Self { params }
    }
}

/// Build the resolved instance for a matched parameter name.
///
/// A structured curly-bracket param def (`dc{VDD_3V3, GND}`) must resolve to
/// its Bus form: a bare `Label(full-string)` would collapse the multi-member
/// port into a 1*1 scalar, so a body reference like `dc{VDD_3V3, GND} ->
/// dcdc{Vin, GND}` fails the §5 row-count check. `McBus` keeps the real
/// member width on both sides of the operator.
///
/// The member split routes through the pipeline's string front-end
/// (`equivalent::member_set_from_str`, §4.2 shared), so `,` and `|` separators
/// (`Q1{S|D}`, `{SPI,MIC|DAC_OUT,SPK_MUTE}`) share one member-set expansion.
/// The def key is the canonical `to_string()` rendering (e.g.
/// `USB_VBUS_1{VDD_3V3, GND}`), which `McIds::from(&str)` wraps as a single
/// `Ida` segment; the front-end recovers the ordered member paths from it.
fn param_name_to_inst(name: &str) -> McInstance {
    let expanded = crate::semantic::basic::equivalent::member_set_from_str(name);
    if let (Some(members), Some(open)) = (&expanded, name.find('{')) {
        // Structured curly form: `base{A,B|C}` expands to `[base.A, base.B, ...]`.
        let base = name[..open].to_string();
        if !base.is_empty() && !members.is_empty() {
            let prefix = format!("{base}.");
            let stripped: Vec<String> = members
                .iter()
                .map(|m| m.strip_prefix(&prefix).unwrap_or(m).to_string())
                .collect();
            if stripped.iter().all(|m| !m.is_empty()) {
                return McInstance::Bus(McBus::new_with_members(&base, stripped));
            }
        }
    }
    McInstance::Label(name.to_string())
}
impl ResolveScope<Resolved> for ParamsScope<'_> {
    fn resolve(&self, name: &str) -> Option<Resolved> {
        self.params
            .iter_defs_with_span()
            .find(|(n, _)| *n == name)
            .map(|(_, span)| Resolved {
                inst: param_name_to_inst(name),
                span: Some(span),
            })
    }
}

/// Module param ports (P1): `iter_ports_with_span` match.
pub struct ParamPortsScope<'a> {
    params: &'a McParamDeclares,
}

impl<'a> ParamPortsScope<'a> {
    pub fn new(params: &'a McParamDeclares) -> Self {
        Self { params }
    }
}

impl ResolveScope<Resolved> for ParamPortsScope<'_> {
    fn resolve(&self, name: &str) -> Option<Resolved> {
        self.params
            .iter_ports_with_span()
            .find(|(n, _)| *n == name)
            .map(|(_, span)| Resolved {
                inst: param_name_to_inst(name),
                span: Some(span),
            })
    }
}

/// Scoped enum value (component P2): `find_scoped_enum_value` match.
///
/// Carries the referencing file URI so the enum class resolves through the
/// unified P1-P5 policy (§5.4) and the value lands on a precise definition
/// (class id + value index) instead of a name-only workspace scan.
pub struct ScopedEnumScope<'a> {
    family_name: &'a McIds,
    uri: &'a McURI,
}

impl<'a> ScopedEnumScope<'a> {
    pub fn new(family_name: &'a McIds, uri: &'a McURI) -> Self {
        Self { family_name, uri }
    }
}

impl ResolveScope<Resolved> for ScopedEnumScope<'_> {
    fn resolve(&self, name: &str) -> Option<Resolved> {
        find_scoped_enum_value(self.uri, self.family_name, name).map(
            |(enum_name, def_uri, class_id, span)| Resolved {
                inst: McInstance::EnumVal {
                    enum_name,
                    value_name: name.to_string(),
                    span: Some(span.clone()),
                    class_id,
                    def_uri: Some(def_uri.to_string()),
                },
                span: Some(span),
            },
        )
    }
}

/// Attributes (component P3): first attr value + key span.
pub struct AttrsScope<'a> {
    attrs: &'a McAttributes,
}

impl<'a> AttrsScope<'a> {
    pub fn new(attrs: &'a McAttributes) -> Self {
        Self { attrs }
    }
}

impl ResolveScope<Resolved> for AttrsScope<'_> {
    fn resolve(&self, name: &str) -> Option<Resolved> {
        let attr_ids = McIds::from(name);
        self.attrs.find(&attr_ids).and_then(|attr| {
            attr.values.first().map(|val| Resolved {
                inst: McInstance::Attr(val.clone()),
                span: attr.key_span.clone(),
            })
        })
    }
}

/// Whole pin names (component P4, includes `pins` transparency):
/// `names_to_id` match → Label carrying the declared pin name.
pub struct PinNamesScope<'a> {
    pins: &'a McPins,
}

impl<'a> PinNamesScope<'a> {
    pub fn new(pins: &'a McPins) -> Self {
        Self { pins }
    }
}

impl ResolveScope<Resolved> for PinNamesScope<'_> {
    fn resolve(&self, name: &str) -> Option<Resolved> {
        let port = self.pins.names_to_id.get(name)?;
        let span = self.pins.pin_name_spans.get(name).cloned();
        // True pins (Single/Multi/MultiGroup) keep the declared name so
        // func-body references display the semantic name (VDD/GPIO[2]/GND)
        // instead of the pin id. Bus/List/Interface pins keep their typed
        // instance so symbol classification (BusDef/ListDef/...) is preserved.
        let inst = match port {
            McPinPort::Single(_) | McPinPort::Multi(_) | McPinPort::MultiGroup(_) => {
                McInstance::Label(name.to_string())
            }
            other => port_to_instance(other),
        };
        Some(Resolved { inst, span })
    }
}

/// Expanded pin names (component P5): any `pin_id_to_names` entry contains
/// the name (first-hit short-circuits like the original loop).
pub struct PinNamesExpandedScope<'a> {
    pins: &'a McPins,
}

impl<'a> PinNamesExpandedScope<'a> {
    pub fn new(pins: &'a McPins) -> Self {
        Self { pins }
    }
}

impl ResolveScope<Resolved> for PinNamesExpandedScope<'_> {
    fn resolve(&self, name: &str) -> Option<Resolved> {
        self.pins
            .pin_id_to_names
            .values()
            .any(|names| names.contains(&name.to_string()))
            .then(|| Resolved {
                inst: McInstance::Label(name.to_string()),
                span: self.pins.pin_name_spans.get(name).cloned(),
            })
    }
}

/// Pin IDs (component P6): `pin_id_to_names` key match.
pub struct PinIdsScope<'a> {
    pins: &'a McPins,
}

impl<'a> PinIdsScope<'a> {
    pub fn new(pins: &'a McPins) -> Self {
        Self { pins }
    }
}

impl ResolveScope<Resolved> for PinIdsScope<'_> {
    fn resolve(&self, name: &str) -> Option<Resolved> {
        self.pins
            .pin_id_to_names
            .contains_key(name)
            .then(|| Resolved {
                inst: McInstance::PinId(name.to_string()),
                span: self.pins.pin_id_spans.get(name).cloned(),
            })
    }
}

/// Direct instance lookup (component P7 / McFunction single level).
pub struct InstsScope<'a> {
    insts: &'a McInstances,
}

impl<'a> InstsScope<'a> {
    pub fn new(insts: &'a McInstances) -> Self {
        Self { insts }
    }
}

impl ResolveScope<Resolved> for InstsScope<'_> {
    fn resolve(&self, name: &str) -> Option<Resolved> {
        let inst = self.insts.get(name)?.clone();
        let span = self.insts.get_port_span(name);
        Some(Resolved { inst, span })
    }
}

/// Module ports (P3): `get_with_iotype` with a concrete IO type.
pub struct PortsScope<'a> {
    insts: &'a McInstances,
}

impl<'a> PortsScope<'a> {
    pub fn new(insts: &'a McInstances) -> Self {
        Self { insts }
    }
}

impl ResolveScope<Resolved> for PortsScope<'_> {
    fn resolve(&self, name: &str) -> Option<Resolved> {
        let (iotype, inst) = self.insts.get_with_iotype(name)?;
        if matches!(iotype, IOType::None) {
            return None;
        }
        let span = self.insts.get_port_span(name);
        Some(Resolved {
            inst: inst.clone(),
            span,
        })
    }
}

/// Module labels (P4): `iter_labels_with_span` match.
pub struct LabelsScope<'a> {
    insts: &'a McInstances,
}

impl<'a> LabelsScope<'a> {
    pub fn new(insts: &'a McInstances) -> Self {
        Self { insts }
    }
}

impl ResolveScope<Resolved> for LabelsScope<'_> {
    fn resolve(&self, name: &str) -> Option<Resolved> {
        self.insts
            .iter_labels_with_span()
            .find(|(n, _, _)| *n == name)
            .map(|(_, _, span)| Resolved {
                inst: McInstance::Label(name.to_string()),
                span: Some(span),
            })
    }
}

/// Non-port, non-label instances (module P5): `get_with_iotype` with no IO
/// type and not a label. Uniformly covers Bus/List/Interface/Component.
pub struct NonPortInstsScope<'a> {
    insts: &'a McInstances,
}

impl<'a> NonPortInstsScope<'a> {
    pub fn new(insts: &'a McInstances) -> Self {
        Self { insts }
    }
}

impl ResolveScope<Resolved> for NonPortInstsScope<'_> {
    fn resolve(&self, name: &str) -> Option<Resolved> {
        let (iotype, inst) = self.insts.get_with_iotype(name)?;
        if !matches!(iotype, IOType::None) || matches!(inst, McInstance::Label(_)) {
            return None;
        }
        let span = self.insts.get_port_span(name);
        Some(Resolved {
            inst: inst.clone(),
            span,
        })
    }
}

/// Funcs (component P8 / module P6): `funcs.find` — span is always `None`
/// (func def spans come from `lapper_func_define_role` at registration time,
/// not from the semantic tables; see §3.7.2).
pub struct FuncsScope<'a> {
    funcs: &'a McFunctions,
}

impl<'a> FuncsScope<'a> {
    pub fn new(funcs: &'a McFunctions) -> Self {
        Self { funcs }
    }
}

impl ResolveScope<Resolved> for FuncsScope<'_> {
    fn resolve(&self, name: &str) -> Option<Resolved> {
        self.funcs.find(name).map(|func| Resolved {
            inst: McInstance::Func(Arc::new(func.clone())),
            span: None,
        })
    }
}

/// Enum values (McEnumDef): value name match within the enum.
pub struct EnumValuesScope<'a> {
    name: &'a McIds,
    values: &'a [McEnumValue],
    uri: &'a McURI,
}

impl<'a> EnumValuesScope<'a> {
    pub fn new(name: &'a McIds, values: &'a [McEnumValue], uri: &'a McURI) -> Self {
        Self { name, values, uri }
    }
}

impl ResolveScope<Resolved> for EnumValuesScope<'_> {
    fn resolve(&self, name: &str) -> Option<Resolved> {
        let enum_name = self.name.to_string();
        self.values
            .iter()
            .find(|v| v.name.to_string() == name)
            .map(|value| {
                let span = value.span[0] as usize..value.span[1] as usize;
                Resolved {
                    inst: McInstance::EnumVal {
                        enum_name,
                        value_name: name.to_string(),
                        span: Some(span.clone()),
                        // The enum container itself is the class here; its class id
                        // lives in whichever file built this chain, which is not
                        // available at scope construction time — leave it for the
                        // consumer to resolve via def_uri + enum name.
                        class_id: None,
                        def_uri: Some(self.uri.to_string()),
                    },
                    span: Some(span),
                }
            })
    }
}

/// Interface pin names (P2): stored `pin_name_spans` match → `Label`.
pub struct InterfacePinNamesScope<'a> {
    pins: &'a McPins,
}

impl<'a> InterfacePinNamesScope<'a> {
    pub fn new(pins: &'a McPins) -> Self {
        Self { pins }
    }
}

impl ResolveScope<Resolved> for InterfacePinNamesScope<'_> {
    fn resolve(&self, name: &str) -> Option<Resolved> {
        self.pins.pin_name_spans.get(name).map(|span| Resolved {
            inst: McInstance::Label(name.to_string()),
            span: Some(span.clone()),
        })
    }
}

/// Func params (P1, only real P1 category — local labels live in the parent
/// container table and resolve via P2): containment check, span always `None`.
pub struct FuncParamsScope<'a> {
    param_names: &'a [String],
}

impl<'a> FuncParamsScope<'a> {
    pub fn new(param_names: &'a [String]) -> Self {
        Self { param_names }
    }
}

impl ResolveScope<Resolved> for FuncParamsScope<'_> {
    fn resolve(&self, name: &str) -> Option<Resolved> {
        self.param_names
            .iter()
            .any(|n| n == name)
            .then(|| Resolved {
                inst: McInstance::Label(name.to_string()),
                span: None,
            })
    }
}

// ============================================================================
// Container chain builders (§3.3) — a container is its ordered category chain
// ============================================================================

/// P2 component category chain (① params → ② scoped enum → ③ attrs →
/// ④ pin names (whole) → ⑤ pin names (expanded) → ⑥ pin IDs →
/// ⑦ insts (IO bus/interface) → ⑧ funcs).
pub fn component_scope<'a>(c: &'a McComponent) -> ScopeChain<'a, Resolved> {
    ScopeChain::new(vec![
        Box::new(ParamsScope::new(&c.params)),
        Box::new(ScopedEnumScope::new(&c.name, &c.uri)),
        Box::new(AttrsScope::new(&c.attrs)),
        Box::new(PinNamesScope::new(&c.pins)),
        Box::new(PinNamesExpandedScope::new(&c.pins)),
        Box::new(PinIdsScope::new(&c.pins)),
        Box::new(InstsScope::new(&c.insts)),
        Box::new(FuncsScope::new(&c.funcs)),
    ])
}

/// P2 module category chain (① param ports → ② param defs → ③ ports →
/// ④ labels → ⑤ non-port insts → ⑥ funcs). No independent buses category:
/// Bus/List/Interface/Component instances are covered by ⑤.
pub fn module_scope<'a>(m: &'a McModule) -> ScopeChain<'a, Resolved> {
    ScopeChain::new(vec![
        Box::new(ParamPortsScope::new(&m.params)),
        Box::new(ParamsScope::new(&m.params)),
        Box::new(PortsScope::new(&m.insts)),
        Box::new(LabelsScope::new(&m.insts)),
        Box::new(NonPortInstsScope::new(&m.insts)),
        Box::new(FuncsScope::new(&m.funcs)),
    ])
}

/// P2 interface category chain (① params → ② pin names).
pub fn interface_scope<'a>(i: &'a McInterface) -> ScopeChain<'a, Resolved> {
    ScopeChain::new(vec![
        Box::new(ParamsScope::new(&i.params)),
        Box::new(InterfacePinNamesScope::new(&i.pins)),
    ])
}

/// P2 enum category chain (① enum values).
pub fn enum_scope<'a>(e: &'a McEnumDef) -> ScopeChain<'a, Resolved> {
    ScopeChain::new(vec![Box::new(EnumValuesScope::new(
        &e.name, &e.values, &e.uri,
    ))])
}

/// Unified container chain — dispatch on the container reference.
pub fn container_scope<'a>(c: &'a ContainerRef) -> ScopeChain<'a, Resolved> {
    match c {
        ContainerRef::Component(c) => component_scope(c),
        ContainerRef::Module(m) => module_scope(m),
        ContainerRef::Interface(i) => interface_scope(i),
        ContainerRef::Enum(e) => enum_scope(e),
    }
}

// ============================================================================
// Two chains (§3.4) — instance chain (P1-P2) and class chain (P3-P5)
// ============================================================================

/// P2 delegation scope — the parent container's own category chain, reached
/// through its `HasFindInst` implementation.
struct DelegatedScope<'a> {
    parent: &'a dyn HasFindInst,
}

impl ResolveScope<Resolved> for DelegatedScope<'_> {
    fn resolve(&self, name: &str) -> Option<Resolved> {
        self.parent
            .find_inst_with_span(name)
            .map(|(inst, span)| Resolved { inst, span })
    }
}

/// Instance chain (Level 1): P1 func params → P2 parent container chain.
pub fn instance_chain<'a>(
    param_names: &'a [String],
    parent: &'a dyn HasFindInst,
) -> ScopeChain<'a, Resolved> {
    ScopeChain::new(vec![
        Box::new(FuncParamsScope::new(param_names)),
        Box::new(DelegatedScope { parent }),
    ])
}

/// P3 — same-file top-level CMIE (component/module/interface/enum/define).
pub struct FileScope<'a> {
    uri: &'a McURI,
}

impl<'a> FileScope<'a> {
    pub fn new(uri: &'a McURI) -> Self {
        Self { uri }
    }
}

impl ResolveScope<ContainerRef> for FileScope<'_> {
    fn resolve(&self, name: &str) -> Option<ContainerRef> {
        let uri = self.uri.as_str();
        for (sn, comp) in crate::definition_space().workspace_components() {
            if sn.uri == uri && sn.ident.to_string() == name {
                return Some(ContainerRef::Component(comp));
            }
        }
        for (sn, module) in crate::definition_space().workspace_modules() {
            if sn.uri == uri && sn.ident.to_string() == name {
                return Some(ContainerRef::Module(module));
            }
        }
        for (sn, iface) in crate::definition_space().workspace_interfaces() {
            if sn.uri == uri && sn.ident.to_string() == name {
                return Some(ContainerRef::Interface(iface));
            }
        }
        for (sn, def) in crate::definition_space().workspace_enums() {
            if sn.uri == uri && sn.ident.to_string() == name {
                return Some(ContainerRef::Enum(def));
            }
        }
        None
    }
}

/// P4 — use-chain imported external classes. Mirrors the RefDefMap name-index
/// path of `mcb_get_cmie` (§5): local name → def file → CMIE by kind.
pub struct UseChainScope<'a> {
    uri: &'a McURI,
}

impl<'a> UseChainScope<'a> {
    pub fn new(uri: &'a McURI) -> Self {
        Self { uri }
    }
}

impl ResolveScope<ContainerRef> for UseChainScope<'_> {
    fn resolve(&self, name: &str) -> Option<ContainerRef> {
        let mcfile = workspace::WORKSPACE.mcodes.get(self.uri)?;
        let sym = mcfile.symbols.lock().ok()?;
        let map = sym.ref_def_map.as_ref()?;
        let entry = map
            .name_index
            .get(&(self.uri.as_str().to_string(), name.to_string()))?;
        let def_uri = crate::semantic::common::uri_of_file_id(entry.def_loc.file_id);
        let space_name = McSpaceName::new(&McIds::from(name), McURI::from(def_uri.as_ref()));
        cmie_by_kind(entry.cmie_kind, &space_name).and_then(cmie_to_container_ref)
    }
}

/// P5 — mcode system library (CAP/RES/DC..., visible by default).
pub struct SystemLibScope;

impl SystemLibScope {
    pub fn new() -> Self {
        Self
    }
}

impl ResolveScope<ContainerRef> for SystemLibScope {
    fn resolve(&self, name: &str) -> Option<ContainerRef> {
        // P5 read: the system-library tables alone, through the DefinitionSpace
        // system-only view (defspace.rs). A definition in a *different project
        // file* must reach here via the use chain (P4), not answer P5 — the
        // merged view would wrongly admit those.
        let ds = crate::definition_space();
        for (sn, def) in ds.system_components() {
            if sn.ident.to_string() == name {
                return Some(ContainerRef::Component(def));
            }
        }
        for (sn, def) in ds.system_modules() {
            if sn.ident.to_string() == name {
                return Some(ContainerRef::Module(def));
            }
        }
        for (sn, def) in ds.system_interfaces() {
            if sn.ident.to_string() == name {
                return Some(ContainerRef::Interface(def));
            }
        }
        for (sn, def) in ds.system_enums() {
            if sn.ident.to_string() == name {
                return Some(ContainerRef::Enum(def));
            }
        }
        None
    }
}

/// Class chain (Level 2): P3 same-file → P4 use chain → P5 system library.
/// Never mixed with the instance chain — the two return different types.
pub fn class_chain<'a>(uri: &'a McURI) -> ScopeChain<'a, ContainerRef> {
    ScopeChain::new(vec![
        Box::new(FileScope::new(uri)),
        Box::new(UseChainScope::new(uri)),
        Box::new(SystemLibScope::new()),
    ])
}

/// Unified base-name resolution result — the exit of the two chains.
pub enum BaseResolved {
    /// P1-P2 instance chain hit (`McInstance` + span). Produced by
    /// [`first_hop`] but only the `Container` exit is consumed by current
    /// callers; the payload is retained for the chain contract.
    #[allow(dead_code)]
    Inst(Resolved),
    /// P3-P5 class chain hit (container reference).
    Container(ContainerRef),
}

/// Resolve the base segment of a name: Level 1 instance chain first, then
/// Level 2 class chain (never both — different return types, §1.2).
///
/// `Inst` → member hops go through `member_of`; `Container` → member access
/// enters the container's own category chain via [`container_scope`].
///
/// `parent` is `None` in table-only LSP contexts (chain.rs) where only the
/// class chain is relevant — the instance chain is skipped entirely.
pub fn first_hop(
    name: &str,
    param_names: &[String],
    parent: Option<&dyn HasFindInst>,
    uri: &McURI,
) -> Option<BaseResolved> {
    let inst = parent
        .and_then(|p| instance_chain(param_names, p).resolve(name))
        .map(BaseResolved::Inst);
    inst.or_else(|| class_chain(uri).resolve(name).map(BaseResolved::Container))
}

// ============================================================================
// Helpers
// ============================================================================

/// Map a CMIE kind byte to a concrete `McCMIE` via the space name.
/// All single-identity lookups route through the
/// [`DefinitionSpace`](crate::DefinitionSpace) unified view (workspace first,
/// then the system-lib tables; design §12.4 rule 1).
fn cmie_by_kind(kind: u8, space_name: &McSpaceName) -> Option<McCMIE> {
    let ds = crate::definition_space();
    match kind {
        0 => ds.get_component(space_name).map(McCMIE::Component),
        1 => ds.get_module(space_name).map(McCMIE::Module),
        2 => interface_lookup(space_name).map(McCMIE::Interface),
        3 => ds.get_enum(space_name).map(McCMIE::Enum),
        _ => None,
    }
}

/// Convert a `McCMIE` into a `ContainerRef`.
fn cmie_to_container_ref(c: McCMIE) -> Option<ContainerRef> {
    match c {
        McCMIE::Component(c) => Some(ContainerRef::Component(c)),
        McCMIE::Module(m) => Some(ContainerRef::Module(m)),
        McCMIE::Interface(i) => Some(ContainerRef::Interface(i)),
        McCMIE::Enum(e) => Some(ContainerRef::Enum(e)),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::basic::mc_literal::{McInt, McLiteral};
    use crate::semantic::component::mc_attr::McAttribute;
    use crate::semantic::component::mc_pins::McPinPort;
    use crate::semantic::mc_enum::McEnumValue;
    use crate::McAttrVal;

    /// Minimal probe scope used to assert chain ordering semantics.
    struct ProbeScope(&'static str, Option<&'static str>);

    impl ResolveScope<&'static str> for ProbeScope {
        fn resolve(&self, name: &str) -> Option<&'static str> {
            self.1.filter(|hit| *hit == name).map(|_| self.0) // tag of the hitting scope
        }
    }

    /// §3.1/§6.4: first-hit wins — a later scope with the same name is never
    /// consulted (no `try_next` penetration); a full miss returns `None`.
    #[test]
    fn chain_first_hit_wins_no_penetration() {
        let chain = ScopeChain::new(vec![
            Box::new(ProbeScope("P1", Some("VDD"))),
            Box::new(ProbeScope("P2", Some("VDD"))), // same name, must lose
            Box::new(ProbeScope("P5", Some("VDD"))), // same name, must lose
        ]);
        assert_eq!(chain.resolve("VDD"), Some("P1"));
        assert_eq!(chain.resolve("GND"), None);
    }

    /// §6.4: P3 shadows P5 — when two levels would both hit, the earlier one
    /// (file-level) wins over the later one (system library).
    #[test]
    fn chain_ordering_shadows_lower_priority() {
        let chain = ScopeChain::new(vec![
            Box::new(ProbeScope("P3-file", Some("CAP"))),
            Box::new(ProbeScope("P5-system", Some("CAP"))),
        ]);
        assert_eq!(chain.resolve("CAP"), Some("P3-file"));
    }

    /// §2.1/§4.1 read-side canonical fallback: a spelling that denotes a single
    /// bare member (`res[4]` → `res4`) is retried under the canonical member
    /// name when the ordered units all miss — the storage keys are the
    /// expanded member names. Conservative bound: dotted/curly members do not
    /// canonicalize, so a genuine miss stays a miss rather than guessing.
    #[test]
    fn chain_canonical_single_fallback_resolves_member_spelling() {
        let chain = ScopeChain::new(vec![Box::new(ProbeScope("P1", Some("res4")))]);
        // Structured spelling of the same member resolves through the fallback.
        assert_eq!(chain.resolve("res[4]"), Some("P1"));
        // The bare member name hits directly (no fallback); a different member
        // misses on both the direct and the canonical forms.
        assert_eq!(chain.resolve("res4"), Some("P1"));
        assert_eq!(chain.resolve("res[5]"), None);
        // Dotted / curly forms never canonicalize — `A{m}` is not `A.m`.
        let dotted = ScopeChain::new(vec![Box::new(ProbeScope("P1", Some("A.m")))]);
        assert_eq!(dotted.resolve("A.m"), Some("P1"));
        assert_eq!(dotted.resolve("A{m}"), None);
    }

    /// P1 func-params scope: containment match, span is always `None`.
    #[test]
    fn func_params_scope_matches_and_misses() {
        let params = vec!["a".to_string(), "b".to_string()];
        let scope = FuncParamsScope::new(&params);
        let hit = scope.resolve("a").unwrap();
        assert!(matches!(hit.inst, McInstance::Label(ref n) if n == "a"));
        assert_eq!(hit.span, None);
        assert!(scope.resolve("c").is_none());
    }

    /// Instances scope reads the semantic table directly (no text re-parsing):
    /// the stored port span is returned unchanged.
    #[test]
    fn insts_scope_reads_semantic_table() {
        let mut insts = McInstances::new();
        insts.create_inst("VDD", McInstance::Label("VDD".to_string()));
        insts.store_port_span("VDD", 10..14);
        let scope = InstsScope::new(&insts);
        let hit = scope.resolve("VDD").unwrap();
        assert!(matches!(hit.inst, McInstance::Label(_)));
        assert_eq!(hit.span, Some(10..14));
        assert!(scope.resolve("GND").is_none());
    }

    fn probe_enum_def() -> McEnumDef {
        McEnumDef {
            name: McIds::from("PKG"),
            span: [0, 3],
            values: vec![McEnumValue {
                name: McIds::from("A"),
                span: [4, 5],
            }],
            uri: McURI::default(),
        }
    }

    /// Enum category chain resolves an enum value from the semantic table.
    #[test]
    fn enum_scope_resolves_value() {
        let def = probe_enum_def();
        let hit = enum_scope(&def).resolve("A").unwrap();
        assert!(matches!(hit.inst, McInstance::EnumVal { .. }));
        assert_eq!(hit.span, Some(4..5));
        assert!(enum_scope(&def).resolve("B").is_none());
    }

    /// §6.4 P1 shadows P2: a func param with the same name as a parent member
    /// wins through the instance chain (`McEnumDef` plays the parent role).
    #[test]
    fn instance_chain_param_shadows_parent() {
        let def = probe_enum_def();
        let params = vec!["A".to_string()];

        // No matching param → parent (P2) resolves the enum value.
        let no_params: Vec<String> = Vec::new();
        let hit = instance_chain(&no_params, &def).resolve("A").unwrap();
        assert!(matches!(hit.inst, McInstance::EnumVal { .. }));

        // Matching param (P1) shadows the parent's enum value.
        let hit = instance_chain(&params, &def).resolve("A").unwrap();
        assert!(matches!(hit.inst, McInstance::Label(ref n) if n == "A"));
        assert_eq!(hit.span, None);
    }

    // ── §6.4: one independent test per definition-layer category unit ──

    fn param_declares_with(name: &str, span: std::ops::Range<usize>) -> McParamDeclares {
        let mut p = McParamDeclares::new();
        p.store_def_span(name, span);
        p
    }

    fn pins_with(
        names_to_id: Vec<(&str, McPinPort)>,
        pin_names: Vec<(&str, std::ops::Range<usize>)>,
        pin_ids: Vec<(&str, Vec<&str>)>,
        id_spans: Vec<(&str, std::ops::Range<usize>)>,
    ) -> McPins {
        let mut pins = McPins::new();
        for (n, p) in names_to_id {
            pins.names_to_id.insert(n.to_string(), p);
        }
        for (n, s) in pin_names {
            pins.pin_name_spans.insert(n.to_string(), s);
        }
        for (i, names) in pin_ids {
            pins.pin_id_to_names
                .insert(i.to_string(), names.iter().map(|s| s.to_string()).collect());
        }
        for (i, s) in id_spans {
            pins.pin_id_spans.insert(i.to_string(), s);
        }
        pins
    }

    /// ParamsScope: component params resolve from the semantic table.
    #[test]
    fn params_scope_resolves_defs() {
        let params = param_declares_with("VDD", 5..8);
        let hit = ParamsScope::new(&params).resolve("VDD").unwrap();
        assert!(matches!(hit.inst, McInstance::Label(ref n) if n == "VDD"));
        assert_eq!(hit.span, Some(5..8));
        assert!(ParamsScope::new(&params).resolve("GND").is_none());
    }

    /// ParamPortsScope: module param ports resolve with their port spans.
    #[test]
    fn param_ports_scope_resolves_ports() {
        let params = param_declares_with("vin", 3..6);
        let hit = ParamPortsScope::new(&params).resolve("vin").unwrap();
        assert!(matches!(hit.inst, McInstance::Label(ref n) if n == "vin"));
        assert_eq!(hit.span, Some(3..6));
        assert!(ParamPortsScope::new(&params).resolve("vout").is_none());
    }

    /// AttrsScope: first attribute value + key span.
    #[test]
    fn attrs_scope_resolves_attr_value() {
        let mut attrs = McAttributes::new();
        attrs.push(McAttribute {
            no: 0,
            id: McIds::from("partno"),
            values: vec![McAttrVal::AttrLiteral(McLiteral::Int(McInt { value: 10 }))],
            key_span: Some(7..13),
        });
        let hit = AttrsScope::new(&attrs).resolve("partno").unwrap();
        assert!(matches!(hit.inst, McInstance::Attr(_)));
        assert_eq!(hit.span, Some(7..13));
        assert!(AttrsScope::new(&attrs).resolve("package").is_none());
    }

    /// PinNamesScope: whole pin names → single/multi pins carry the declared
    /// name; bus/list/interface pins keep their typed instance.
    #[test]
    fn pin_names_scope_resolves_pin() {
        let pins = pins_with(
            vec![("VDD", McPinPort::Single("1".to_string()))],
            vec![("VDD", 10..13)],
            vec![],
            vec![],
        );
        let hit = PinNamesScope::new(&pins).resolve("VDD").unwrap();
        assert!(matches!(hit.inst, McInstance::Label(ref n) if n == "VDD"));
        assert_eq!(hit.span, Some(10..13));
        assert!(PinNamesScope::new(&pins).resolve("GND").is_none());
    }

    /// PinNamesExpandedScope: any expanded pin name hits.
    #[test]
    fn pin_names_expanded_scope_resolves_alias() {
        let pins = pins_with(
            vec![],
            vec![("VDD", 10..13)],
            vec![("1", vec!["VDD"])],
            vec![],
        );
        let hit = PinNamesExpandedScope::new(&pins).resolve("VDD").unwrap();
        assert!(matches!(hit.inst, McInstance::Label(ref n) if n == "VDD"));
        assert_eq!(hit.span, Some(10..13));
        assert!(PinNamesExpandedScope::new(&pins).resolve("VCC").is_none());
    }

    /// PinIdsScope: pin ID key match → PinId + stored ID span.
    #[test]
    fn pin_ids_scope_resolves_pin_id() {
        let pins = pins_with(vec![], vec![], vec![("1", vec!["VDD"])], vec![("1", 0..1)]);
        let hit = PinIdsScope::new(&pins).resolve("1").unwrap();
        assert!(matches!(hit.inst, McInstance::PinId(ref n) if n == "1"));
        assert_eq!(hit.span, Some(0..1));
        assert!(PinIdsScope::new(&pins).resolve("2").is_none());
    }

    /// PortsScope: concrete-IO-type instances resolve; plain labels do not.
    #[test]
    fn ports_scope_resolves_typed_ports_only() {
        let mut insts = McInstances::new();
        insts.create("VDD", IOType::Power, McInstance::Label("VDD".to_string()));
        insts.store_port_span("VDD", 10..13);
        insts.create("lbl", IOType::None, McInstance::Label("lbl".to_string()));
        insts.store_port_span("lbl", 14..17);
        let scope = PortsScope::new(&insts);
        assert!(scope.resolve("VDD").is_some());
        assert!(scope.resolve("lbl").is_none());
    }

    /// LabelsScope: module labels resolve with their stored spans.
    #[test]
    fn labels_scope_resolves_label() {
        let mut insts = McInstances::new();
        insts.create("sig", IOType::None, McInstance::Label("sig".to_string()));
        insts.store_port_span("sig", 5..8);
        let hit = LabelsScope::new(&insts).resolve("sig").unwrap();
        assert!(matches!(hit.inst, McInstance::Label(ref n) if n == "sig"));
        assert_eq!(hit.span, Some(5..8));
        assert!(LabelsScope::new(&insts).resolve("no").is_none());
    }

    /// NonPortInstsScope: non-port, non-label instances resolve; ports and
    /// labels do not (module P5 uniformly covers Bus/List/Interface/Component).
    #[test]
    fn non_port_insts_scope_resolves_bus_like() {
        let mut insts = McInstances::new();
        insts.create(
            "U1",
            IOType::None,
            McInstance::Unresolved {
                class_name: "U".to_string(),
            },
        );
        insts.create("VDD", IOType::Power, McInstance::Label("VDD".to_string()));
        insts.create("lbl", IOType::None, McInstance::Label("lbl".to_string()));
        let scope = NonPortInstsScope::new(&insts);
        assert!(scope.resolve("U1").is_some());
        assert!(scope.resolve("VDD").is_none()); // port
        assert!(scope.resolve("lbl").is_none()); // label
    }

    /// InterfacePinNamesScope: interface pin names resolve with stored spans.
    #[test]
    fn interface_pin_names_scope_resolves_pin() {
        let pins = pins_with(vec![], vec![("SCLK", 3..7)], vec![], vec![]);
        let hit = InterfacePinNamesScope::new(&pins).resolve("SCLK").unwrap();
        assert!(matches!(hit.inst, McInstance::Label(ref n) if n == "SCLK"));
        assert_eq!(hit.span, Some(3..7));
        assert!(InterfacePinNamesScope::new(&pins).resolve("MOSI").is_none());
    }

    /// DelegatedScope: forwards the miss/hit to the parent container's chain.
    #[test]
    fn delegated_scope_forwards_to_parent() {
        let def = probe_enum_def();
        let scope = DelegatedScope { parent: &def };
        let hit = scope.resolve("A").unwrap();
        assert!(matches!(hit.inst, McInstance::EnumVal { .. }));
        assert!(scope.resolve("B").is_none());
    }

    /// container_scope: dispatches on the container kind (enum branch).
    #[test]
    fn container_scope_dispatches_by_kind() {
        let def = Arc::new(probe_enum_def());
        let hit = container_scope(&ContainerRef::Enum(def.clone()))
            .resolve("A")
            .unwrap();
        assert!(matches!(hit.inst, McInstance::EnumVal { .. }));
        assert!(container_scope(&ContainerRef::Enum(def))
            .resolve("B")
            .is_none());
    }

    /// param_name_to_inst routes through the shared member-set front-end
    /// (§4.2 shared): `,` and `|` separators expand to the bus member list, a
    /// single-member curly group keeps its Bus form, and non-curly names stay
    /// labels (no behavior change for bare/dotted params).
    #[test]
    fn param_name_to_inst_curly_members() {
        fn bus_members(inst: &McInstance) -> Option<(&str, &[String])> {
            match inst {
                McInstance::Bus(b) => Some((&b.name, &b.member[..])),
                _ => None,
            }
        }

        fn expect_bus(inst: &McInstance, name: &str, members: &[&str]) {
            let (got_name, got_members) = bus_members(inst).expect("expected Bus");
            assert_eq!(got_name, name);
            let want: Vec<String> = members.iter().map(|s| s.to_string()).collect();
            assert_eq!(got_members, want.as_slice());
        }

        // Comma-separated group (pre-existing form).
        expect_bus(
            &param_name_to_inst("dc{VDD_3V3, GND}"),
            "dc",
            &["VDD_3V3", "GND"],
        );
        // Pipe-separated group — the new `|` support.
        expect_bus(&param_name_to_inst("Q1{S|D}"), "Q1", &["S", "D"]);
        // Mixed `,` + `|` separators.
        expect_bus(
            &param_name_to_inst("X{SPI,MIC|DAC_OUT}"),
            "X",
            &["SPI", "MIC", "DAC_OUT"],
        );
        // Single-member curly group keeps its Bus width (port stays 1*1 but
        // the name is structured, matching the pre-existing behavior).
        expect_bus(&param_name_to_inst("dc{VDD}"), "dc", &["VDD"]);
        // Non-curly names resolve to plain labels.
        assert!(matches!(param_name_to_inst("GND"), McInstance::Label(_)));
        // Empty braces are not a member set — falls back to the label.
        assert!(matches!(param_name_to_inst("dc{}"), McInstance::Label(_)));
    }
}
