// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Capability adoption (`:: Cap`) name resolution (abstract-variant-capability
//! plan §4.1/§2.1).
//!
//! The `adopts` list on a component header names capability defs. Each name is
//! resolved the same way a class reference in the same header resolves (P3 own
//! file → P4 use chain → P5 system), *minus* the class-specific machinery: a
//! capability is never an `McCMIE`/`add_global_class` symbol, so the layer walk
//! here is a single visibility-table hit (`WORKSPACE.visibility`, which already
//! mirrors every declared symbol a file can see — own decls shadow imports) plus
//! a system-lib capability-name fallback.
//!
//! Pure analysis, no diagnostics, no mutation: the registry link seam
//! ([`crate::db::defregistry::RegistryState::sync_derivation_edges`]) and the
//! re-derived-file validation check both call this and each turn the verdicts
//! into their own side of the contract (silent ledger fill vs. the
//! `ADOPTS_NON_CAPABILITY` / `CAPABILITY_SIGNAL_MISSING` /
//! `ADOPTED_FUNC_AMBIGUOUS` diagnostics).

use crate::db::cmie::tables as workspace;
use crate::db::defregistry::{def_id, live_entry_by_id, DefId, DefKind, DefValue};
use crate::semantic::common::McSpaceName;
use crate::semantic::component::mc_attr::McAttributes;
use crate::semantic::component::McComponent;
use crate::{McIds, McURI};

/// What a `::` adopt name resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdoptTarget {
    /// The capability def that adopts into the host, by registry id.
    Capability(DefId),
    /// Visible name resolved to a def that is *not* a capability (component,
    /// module, …) — `ADOPTS_NON_CAPABILITY`. The visible target's identity is
    /// carried for the message.
    NonCapability(McSpaceName),
    /// Nothing visible or system-defined under this name.
    Unresolved(String),
}

/// Resolve one capability-adoption name as seen from `from_uri` (the adopting
/// file). Layered exactly like class resolution: the file's own visibility
/// table first (P3 own-file decl shadows imports, P4 use chain already merged
/// by [`sync_visibility`](crate::db::infra::mc_code::McCode::sync_visibility)),
/// then the per-world system-lib capability table by name (P5). A name that
/// resolves to a live def of any other kind is `NonCapability`, never
/// `Unresolved`, so the wrong-operator hint (use `:` for an abstract component)
/// fires on the exact shadowed symbol.
pub fn resolve_capability_name(from_uri: &McURI, name: &McIds) -> AdoptTarget {
    let canonical = crate::build::pass1::canonicalize_project_uri(from_uri);
    let name_str = name.to_string();

    // P3/P4 — the visibility table the file's own decls and imports derive.
    if let Some(sn) = workspace::WORKSPACE
        .visibility
        .get(&(canonical, name_str.clone()))
    {
        return classify_visible(&sn);
    }

    // P5 — system-lib capability by name only (capabilities are excluded from
    // `system_name_index`, so this is the one scan `resolve_class`'s system
    // lookup cannot serve).
    let ds = crate::definition_space();
    for (csn, cap) in ds.system_capabilities() {
        if cap.name.to_string() == name_str {
            return AdoptTarget::Capability(match def_id(&csn, DefKind::Capability) {
                Some(id) => id,
                None => {
                    // Def is live in the peel but its id vanished (tombstoned
                    // mid-iteration) — treat as unresolved this round.
                    return AdoptTarget::Unresolved(name_str);
                }
            });
        }
    }

    AdoptTarget::Unresolved(name_str)
}

/// Classify a visibility-table hit: a live capability, a live non-capability
/// (wrong operator), or unresolved (hit row for a def that is no longer live).
fn classify_visible(sn: &McSpaceName) -> AdoptTarget {
    if let Some(id) = def_id(sn, DefKind::Capability) {
        return AdoptTarget::Capability(id);
    }
    let is_other_kind = [
        DefKind::Component,
        DefKind::Module,
        DefKind::Interface,
        DefKind::Enum,
        DefKind::Define,
    ]
    .iter()
    .any(|&k| def_id(sn, k).is_some());
    if is_other_kind {
        return AdoptTarget::NonCapability(sn.clone());
    }
    AdoptTarget::Unresolved(sn.ident.to_string())
}

// ============================================================================
// P4 — variant base (`: Base`) resolution & materialization (§7)
// ============================================================================
//
// `component Y : X` shares X's *data* surface (pins/params/funcs/spec), only
// differing by the child's own attributes (partno/vendor/…). Resolution and
// the merge below live here next to the `::` resolver so the registry link
// seam (materialization) and the validation check
// (`VARIANT_BASE_NON_ABSTRACT`) both read the same verdicts. Materialization
// is a pure clone-and-overlay of the [`McComponent`]; the registry only
// replaces the child's row with the result.

/// What a `: Base` variant-base reference resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VariantBaseTarget {
    /// The name resolved to a live abstract component def (by registry id).
    AbstractComponent(DefId),
    /// The name resolved to a live def that is not an abstract component —
    /// a concrete component, module, … (`VARIANT_BASE_NON_ABSTRACT`).
    NonAbstract(McSpaceName),
    /// Nothing visible or system-defined under this name.
    Unresolved(String),
}

/// Resolve one variant-base name as seen from `from_uri` (the variant's own
/// file), mirroring [`resolve_capability_name`]'s layer walk but against class
/// symbols: the file's own visibility table first, then the per-world
/// system-lib component table by name. A name that resolves to a live def of
/// any other shape is `NonAbstract`, never `Unresolved`, so the
/// `VARIANT_BASE_NON_ABSTRACT` diagnostic fires on the exact shadowed symbol.
pub fn resolve_variant_base(from_uri: &McURI, name: &McIds) -> VariantBaseTarget {
    let canonical = crate::build::pass1::canonicalize_project_uri(from_uri);
    let name_str = name.to_string();

    // P3/P4 — the visibility table the file's own decls and imports derive.
    if let Some(sn) = workspace::WORKSPACE
        .visibility
        .get(&(canonical, name_str.clone()))
    {
        return classify_variant_base(&sn);
    }

    // P5 — system-lib component by name (the `resolve_class` system fallback,
    // served from the registry's live component enumeration).
    let ds = crate::definition_space();
    for (csn, comp) in ds.system_components() {
        if comp.name.to_string() == name_str {
            let id = match def_id(&csn, DefKind::Component) {
                Some(id) => id,
                None => return VariantBaseTarget::Unresolved(name_str),
            };
            if comp.is_abstract {
                return VariantBaseTarget::AbstractComponent(id);
            }
            return VariantBaseTarget::NonAbstract(csn);
        }
    }
    VariantBaseTarget::Unresolved(name_str)
}

/// Classify a visibility-table hit for the `:` operator: a live abstract
/// component is a valid base; anything else the name resolves to is not.
fn classify_variant_base(sn: &McSpaceName) -> VariantBaseTarget {
    if let Some(id) = def_id(sn, DefKind::Component) {
        if let Some((_, DefValue::Component(c))) = live_entry_by_id(id) {
            if c.is_abstract {
                return VariantBaseTarget::AbstractComponent(id);
            }
        }
        return VariantBaseTarget::NonAbstract(sn.clone());
    }
    VariantBaseTarget::NonAbstract(sn.clone())
}

/// §7.2/§4.2 materialization: the abstract base def cloned and overlaid with
/// the variant child's own *data* surface. The result is a concrete,
/// self-sufficient component def — same pins/params/funcs/spec *values* and
/// `::` adoption as the base, with the child's top-level attribute overrides
/// (`partno`/`vendor`/`spec.*` — the "data-only" diff) applied on top.
///
/// The child's own pins/params/funcs are never merged: the parse-time data
/// lock (`VARIANT_REDECLARES_PINS_PARAMS_FUNCS`) already made writing them an
/// error, and the base's are authoritative (§7.2). `variant_base` stays the
/// child's own declared clause (provenance — the base, being abstract, never
/// has one), so a later re-derivation round and the validation check still
/// see the declared base on the materialized def.
pub fn materialize_variant(base: &McComponent, child: &McComponent) -> McComponent {
    let mut mat = base.clone();
    mat.name = child.name.clone();
    mat.uri = child.uri.clone();
    mat.span = child.span.clone();
    mat.is_abstract = false;
    mat.variant_base = child.variant_base.clone();
    // adopts ride the base clone (inherited §7.2); a child self-listing `::`
    // is already the parse-time VARIANT_ADOPTS data lock.
    apply_attr_overrides(&mut mat.attrs, &child.attrs);
    mat
}

/// Overlay child top-level attributes onto `base` by full dotted id: a child
/// attr whose id already exists replaces that base attr (values + span);
/// otherwise it appends. `spec.HBM = ±0kV` parses as one dotted id, so the
/// per-leaf spec-item override (§7.2, per-leaf spec overlay) falls out of the same
/// rule — an untouched base `spec.*` leaf simply stays.
fn apply_attr_overrides(base: &mut McAttributes, child: &McAttributes) {
    for attr in child.iter() {
        match base.find_mut(&attr.id) {
            Some(slot) => {
                slot.no = attr.no;
                slot.values.clone_from(&attr.values);
                slot.key_span.clone_from(&attr.key_span);
            }
            None => base.push(attr.clone()),
        }
    }
}

// ============================================================================
// §4.2 / §5 host analysis — capability-adoption consistency & func collisions
// ============================================================================
//
// Both consumers of the resolver (the silent link seam and the re-derived-file
// validation check) need the *consequences* of adoption on a host def, so the
// pure analysis lives here next to the resolver and the check just maps the
// verdicts to diagnostics. The §4.2 matcher is the first-cut implementation
// the plan marks as pending golden empirical tuning: presence is exact-name membership on the
// adopter's declared member surface (`McPins.names_to_id`), and direction
// compatibility uses the provisional table — a capability `ps` signal matches
// an adopter `in`/`io` rail (power rails parse as `in [..]::DC()`, per the
// golden), an `in` signal never matches an adopter `out`, and so on. Grouping
// is NOT flexible here (declared label/member names must line up); cross-group
// name flexibility is tracked separately (Open D2) and out of P2 scope.

use crate::semantic::capability::McCapability;
use crate::semantic::common::IOType;
use crate::semantic::component::mc_pins::McPinPort;
use crate::semantic::mc_inst::McInstance;

/// One capability-declared signal that the adopter does not realize.
#[derive(Debug, Clone)]
pub struct MissingSignal {
    /// The referenceable form the adopter lacks (e.g. `uart`, `uart.RO`, `VCC`).
    pub form: String,
    /// Concrete fix hint for the diagnostic message (names the declaring
    /// capability so a multi-capability adopter stays unambiguous).
    pub hint: String,
}

/// Everything the §4.2/§5 checks need to know about one adopting host.
#[derive(Debug, Default)]
pub struct HostAdoptionFindings {
    /// `::` target names that resolved to a non-capability (`ADOPTS_NON_CAPABILITY`).
    pub non_capabilities: Vec<String>,
    /// Func names two adopted capabilities share and the host does not override
    /// (`ADOPTED_FUNC_AMBIGUOUS`).
    pub ambiguous_funcs: Vec<String>,
    /// Capability-declared signals missing on the adopter (`CAPABILITY_SIGNAL_MISSING`).
    pub missing_signals: Vec<MissingSignal>,
}

/// §4.2/§5 host analysis. `comp` is a live component def whose `adopts` list is
/// non-empty; every adopt name is resolved exactly as the link seam resolves it
/// (same resolver, same verdicts), so the reported set is the set the ledgers
/// actually excluded.
pub fn analyze_host_adoption(comp: &McComponent) -> HostAdoptionFindings {
    let mut out = HostAdoptionFindings::default();
    if comp.adopts.is_empty() {
        return out;
    }
    let from_uri = comp.uri.clone();

    // Resolve each adopt name once; collect capability defs for §4.2 and §5.
    let mut caps: Vec<(String, std::sync::Arc<McCapability>)> = Vec::new();
    for name in &comp.adopts {
        match resolve_capability_name(&from_uri, name) {
            AdoptTarget::Capability(id) => {
                if let Some((_, crate::db::defregistry::DefValue::Capability(cap))) =
                    crate::db::defregistry::live_entry_by_id(id)
                {
                    caps.push((name.to_string(), cap));
                }
            }
            AdoptTarget::NonCapability(_) => out.non_capabilities.push(name.to_string()),
            AdoptTarget::Unresolved(_) => {} // existing unresolved-name path (plan §2.1)
        }
    }

    // §5 — adopted-func ambiguity: a name two capabilities share is ambiguous
    // unless the host overrides it with its own func.
    if caps.len() >= 2 {
        let own: std::collections::HashSet<String> =
            comp.funcs.iter().map(|f| f.name.to_string()).collect();
        let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for (_, cap) in &caps {
            for f in cap.funcs.iter() {
                *counts.entry(f.name.to_string()).or_insert(0) += 1;
            }
        }
        for (nm, n) in counts {
            if n >= 2 && !own.contains(&nm) {
                out.ambiguous_funcs.push(nm);
            }
        }
        out.ambiguous_funcs.sort();
    }

    // §4.2 — every declared signal of every adopted capability must be
    // realizable on the adopter's declared member surface.
    for (_, cap) in &caps {
        for (form, cap_dir) in capability_signal_forms(&cap) {
            match comp.pins.names_to_id.get(&form) {
                Some(port) => {
                    // Present — direction must be compatible when both sides
                    // carry a concrete direction.
                    if let Some(adopt_dir) = pin_dir(port, &comp.pins.pins) {
                        if !dir_compatible(&cap_dir, &adopt_dir) {
                            out.missing_signals.push(MissingSignal {
                                form: form.clone(),
                                hint: dir_conflict_hint(
                                    &cap.name.to_string(),
                                    &cap_dir,
                                    &adopt_dir,
                                ),
                            });
                        }
                    }
                }
                None => {
                    out.missing_signals.push(MissingSignal {
                        form: form.clone(),
                        hint: format!(
                            "capability '{}' declares it — {}",
                            cap.name,
                            missing_form_hint(&form)
                        ),
                    });
                }
            }
        }
    }
    out
}

/// The declared member/referenceable forms of a capability's signal table:
/// scalars as their name, curly buses as the bus label plus one dotted form
/// per member (`uart`, `uart.RO`, `uart.DI`). Square-only vectors register
/// under an anonymous `@N` key and are skipped (nothing referenceable).
fn capability_signal_forms(cap: &McCapability) -> Vec<(String, IOType)> {
    let mut forms = Vec::new();
    for (key, (io, inst)) in cap.signals.insts() {
        if key.starts_with('@') {
            continue;
        }
        match inst {
            McInstance::Bus(b) => {
                forms.push((key.clone(), io.clone()));
                for m in &b.member {
                    forms.push((format!("{}.{}", b.name, m), io.clone()));
                }
            }
            McInstance::List(l) => {
                // Named square list (`io gpio[1,2]`) — the label is referenceable.
                if !l.name.starts_with('@') {
                    forms.push((key.clone(), io.clone()));
                }
            }
            _ => {
                // Label (incl. unlabeled-interface member expansion) and any
                // other keyed instance: require the name itself.
                forms.push((key.clone(), io.clone()));
            }
        }
    }
    forms
}

/// The [`IOType`] of an adopter member by its [`McPinPort`] — the backing pin
/// when the name maps to a single/multi pin id, otherwise `None` (bus/interface
/// instances without a resolved pin keep the direction unchecked).
fn pin_dir(
    port: &McPinPort,
    pins: &std::collections::BTreeMap<String, crate::semantic::component::mc_pins::McPin>,
) -> Option<IOType> {
    let id = match port {
        McPinPort::Single(id) => id.clone(),
        McPinPort::Multi(ids) => ids.first().cloned()?,
        _ => return None,
    };
    pins.get(&id).map(|p| p.iotype.clone())
}

/// Provisional §4.2 direction-compatibility table (plan §4.2 — tuned by golden).
/// A capability `ps` signal is satisfied by an adopter `in`/`io` rail: library
/// power pins are declared `in [..]::DC()` (probe-confirmed), and the doc's
/// row `ps ↔ in/io*` marks exactly that case. Every other pair is strict.
fn dir_compatible(cap: &IOType, adopt: &IOType) -> bool {
    use IOType::*;
    match (cap, adopt) {
        (In, In | InOut) => true,
        (Out, Out | InOut) => true,
        (InOut, In | Out | InOut) => true,
        (Power, Power | In | InOut) => true,
        _ => false,
    }
}

fn io_label(io: &IOType) -> &'static str {
    match io {
        IOType::In => "in",
        IOType::Out => "out",
        IOType::InOut => "io",
        IOType::Power => "ps",
        IOType::Analog => "analog",
        IOType::Return => "return",
        IOType::NonCon => "nc",
        IOType::Label => "label",
        IOType::None => "unspecified",
    }
}

fn dir_conflict_hint(cap_name: &str, cap_dir: &IOType, adopt_dir: &IOType) -> String {
    format!(
        "capability '{}' declares it {} but the component member is {}; \
         declare a compatible direction (in/out/io, or ps for a power rail)",
        cap_name,
        io_label(cap_dir),
        io_label(adopt_dir),
    )
}

fn missing_form_hint(form: &str) -> String {
    if let Some((label, member)) = form.split_once('.') {
        format!(
            "declare a group '{label}' whose members include '{member}' \
             (e.g. `io {label}{{{member}, ...}}`) or expose '{form}'"
        )
    } else {
        format!(
            "declare a member/pin named '{form}' on the component \
             (add it to the pins table, e.g. `in {form}`)"
        )
    }
}
