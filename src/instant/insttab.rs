// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Flattened Instance Table
//!
//! Flattens the McModuleInst tree structure into a one-dimensional table,
//! where every instance (module, component, pin, port, bus, label) has a
//! unique ID and a complete hierarchical path.
//!
//! ## Usage
//! ```ignore
//! let table = InstTable::from_module_inst(&module_inst, 1000);
//! table.dump();
//! ```

use super::arena::NodeArena;
use super::identity::NodeId;
use super::inststore::{InstanceStore, TreeView};
use super::mc_bus::McBusInst;
use super::mc_mod::McModuleInst;
use super::mc_net::NetPoint;
use crate::instant::nettab::NetTableStore;
use crate::semantic::basic::attr_keys::{self, ElementClass};
use crate::semantic::basic::mc_uval::McUnit;
use crate::semantic::common::{IOType, McSpaceName};
use crate::semantic::component::mc_pins::PwrDir;
use crate::semantic::module::pi::McPowerDecls;
use crate::semantic::pwrid::{self, DeclaredMember, Face};
use crate::vector::model::DiffFace;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;

// InstKind - Instance entry type

/// Instance entry type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstKind {
    /// Module instance (including top-level)
    Module,
    /// Component instance (resistor, capacitor, IC, etc.)
    Component,
    /// Component pin
    Pin,
    /// Module port (in/out/inout)
    Port,
    /// Bus (e.g. power{VCC, GND})
    Bus,
    /// Label (standalone label / bus member)
    Label,
}

impl std::fmt::Display for InstKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstKind::Module => write!(f, "Module"),
            InstKind::Component => write!(f, "Component"),
            InstKind::Pin => write!(f, "Pin"),
            InstKind::Port => write!(f, "Port"),
            InstKind::Bus => write!(f, "Bus"),
            InstKind::Label => write!(f, "Label"),
        }
    }
}

impl InstKind {
    /// The lowercase tag this kind carries on the outward faces (JSON rows,
    /// query filters). Spelled per variant rather than lowercased from the
    /// `Display` form, so the two spellings can be changed apart.
    ///
    /// `Display` stays capitalized: it is what diagnostics print, and those
    /// strings are frozen in test expectations.
    pub fn word(&self) -> &'static str {
        match self {
            InstKind::Module => "module",
            InstKind::Component => "component",
            InstKind::Pin => "pin",
            InstKind::Port => "port",
            InstKind::Bus => "bus",
            InstKind::Label => "label",
        }
    }

    /// Registration priority — used to arbitrate when two different kinds
    /// compete for the same path.
    ///
    /// Structural entities (`Module` / `Component` / `Pin`) are real physical
    /// hierarchy nodes in the circuit, with priority over "net-side projections"
    /// (`Port` / `Bus` / `Label`). The latter are often just aliases/endpoints
    /// of some structural entity in the net namespace; when they collide with
    /// a structural entity on path, the structural entity should win.
    ///
    /// See the dedup arbitration logic in `InstTable::register`.
    fn registration_priority(&self) -> u8 {
        match self {
            InstKind::Module | InstKind::Component | InstKind::Pin => 2,
            InstKind::Port | InstKind::Bus | InstKind::Label => 1,
        }
    }
}

// MemberRole / MemberInfo — pin role for net merging and rail checks

/// Electrical role of a pin / interface member
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemberRole {
    Power,
    Ground,
    Signal,
}

impl std::fmt::Display for MemberRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemberRole::Power => write!(f, "Power"),
            MemberRole::Ground => write!(f, "Ground"),
            MemberRole::Signal => write!(f, "Signal"),
        }
    }
}

/// Voltage value extracted from interface params (e.g. `DC(3.3V)` → 3.3 V)
#[derive(Debug, Clone, PartialEq)]
pub struct Volt {
    pub value: f64,
}

impl std::fmt::Display for Volt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.1}V", self.value)
    }
}

/// Role + optional voltage for a pin / interface member
#[derive(Debug, Clone)]
pub struct MemberInfo {
    pub role: MemberRole,
    pub voltage: Option<Volt>,
    /// ★ P3 (ret lineage): for a member of a declared connection-point DC pair,
    /// the paired face's leaf name — the return member on a supply face, the
    /// supply member on the return. Set at flatten from `port.dc_pair`;
    /// viz/project.rs mirrors it onto the nets born from the pair so a draw-time
    /// opt-in can anchor a return lane. `None` for any member the pair does not
    /// name (component DC pins, inferred roles).
    pub pair: Option<String>,
    /// The differential-pair leg this member is, when its port's interface
    /// declares a `@pair(group)` group naming this member's row. Set at
    /// flatten from `port.diff_pair`; viz/project.rs mirrors it onto the
    /// nets born from the pair. `None` for every member the declaration does
    /// not name — including every member of an interface that declares none.
    pub diff: Option<DiffFace>,
}

impl MemberInfo {
    pub fn new(role: MemberRole, voltage: Option<Volt>) -> Self {
        Self {
            role,
            voltage,
            pair: None,
            diff: None,
        }
    }
}

// VectorMemberInfo — vector group projection (design §11.1)

/// Vector member projection (design §11.1): which declared vector group
/// (`c[1:2]`) a flattened component entry belongs to, and its position within
/// the ordered member set.
///
/// Projection-only — the flat path stays `main.c1` (invariant B); consumers
/// (LSP, export, GAP1) use this field to reverse-query the vector structure.
/// Distinct from [`MemberInfo`] (interface member role/voltage), which carries
/// unrelated semantics and is consumed by interface / power-pin checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorMemberInfo {
    /// Vector base name — `"c"` for `c[1:2]` (declaration scope).
    pub vector_base: String,
    /// Member name within the group, e.g. `"c2"` (member_set product).
    pub member: String,
    /// Zero-based index in the ordered member set (c1 → 0, c2 → 1).
    pub index: usize,
}

impl VectorMemberInfo {
    pub fn new(vector_base: String, member: String, index: usize) -> Self {
        Self {
            vector_base,
            member,
            index,
        }
    }
}

/// Infer MemberRole from IOType and owning-module declared identity.
///
/// Model-A rule (classification-retirement batch2 R1): a member gets an
/// electrical role only from an explicit qualifier or the owning module's OWN
/// power-intent declarations — never from a hardcoded name table. The old
/// `is_ground`/`is_supply` name-fallback arm is gone: a bare name with no
/// declaration is `Signal` (classification-retirement-design §3 ruling ① —
/// undeclared: never judged, never guessed). The two predicates are the module's declared
/// ground-side
/// coppers (conduit names ∪ DC rail `ret` members) and declared supply faces
/// (DC rail `hot` members) — the identity set the flat per-statement ground
/// partition exempted from fragmentation before its retirement
/// (split-ground-copper-design v0.2 §6).
/// Callers enrich the predicates with connection-point DC pairs (port
/// `dc_pair` at site 1, the component's own `McPwrPin` rows at the pin sites).
///
/// Returns `(role, inferred)`; `inferred` is kept for signature stability but
/// no arm sets it now (there is no heuristic arm left).
pub fn infer_member_role(
    leaf_name: &str,
    io_type: &IOType,
    is_declared_ground: impl Fn(&str) -> bool,
    is_declared_power: impl Fn(&str) -> bool,
) -> (MemberRole, bool) {
    // (a) declared identity first: the member names a declared return/
    //     reference copper or DC-pair `ret` (ground side) → Ground, or a
    //     declared DC rail / DC-pair `hot` member (supply side) → Power.
    //     Outranks the generic `IOType::Power` qualifier so a `::DC` pair's
    //     ret member (also `IOType::Power`) reads Ground, not Power
    //     (classification-retirement-design §4 C full capture — positional,
    //     name-independent).
    if is_declared_ground(leaf_name) {
        return (MemberRole::Ground, false);
    }
    if is_declared_power(leaf_name) {
        return (MemberRole::Power, false);
    }
    // (b) explicit qualifier fallback: psnk / ::DC power-direction → Power.
    if matches!(io_type, IOType::Power) {
        return (MemberRole::Power, false);
    }
    // (c) no explicit qualifier, no declaration → Signal (never guessed).
    (MemberRole::Signal, false)
}

/// The component `pins.pwr` contract row that owns the flat pin registered
/// under `pin_id` (the `comp.pins` table key — a pin number for `pin [5,2]`,
/// or a name for a plain `in 3 = CE`).
///
/// The row's `hot` stores the member spelling **verbatim**, dotted for a named
/// group (`psrc [5,2] = VOUT{Vout, GND}` → `VOUT.Vout`) and bare for an
/// anonymous pair. That spelling is what `McPin::names` holds, so the match is
/// made against the pin's declared names — the same equality the canonical
/// [`crate::semantic::validation::nets::source_contract_for`] /
/// `sink_contract_for` matchers use. Never split or re-spell the token.
pub(crate) fn pwr_row_for_pin<'a>(
    comp: &'a crate::instant::mc_comp::McComponentInst,
    pin_id: &str,
) -> Option<&'a crate::semantic::component::mc_pins::McPwrPin> {
    let names: Vec<&str> = comp
        .def
        .pins
        .pins
        .get(pin_id)
        .map(|p| p.names.iter().map(|n| n.as_str()).collect())
        .unwrap_or_default();
    comp.def
        .pins
        .pwr
        .iter()
        .find(|c| names.iter().any(|n| *n == c.hot))
}

/// The instance-bound nominal of a flat power pin's contract row (U266 ②):
/// the row's positional `::DC(NAME)` nominal spells a declared constructor
/// formal, resolved here against the instance's bound argument. `text` is the
/// argument as bound (the written default stands in where the call site bound
/// nothing — the same substitution conditions get, U54); `v` carries its DC
/// volts when the argument decodes as one. A window or still-symbolic
/// argument is an honest undecided value, not an error — the value layer
/// keeps window forms undecoded by design (doc/eval), so `v` stays `None`
/// while `text` speaks with the instance's voice.
#[derive(Debug, Clone)]
pub(crate) struct DeclaredNominal {
    pub text: String,
    pub v: Option<f64>,
}

/// Resolve the contract row that owns this flat pin against the instance's
/// constructor arguments. `None` unless the row's positional nominal is
/// exactly a declared formal's name — a literal nominal decodes from the
/// declaration alone and carries nothing instance-bound.
pub(crate) fn pwr_nominal_for_pin(
    comp: &crate::instant::mc_comp::McComponentInst,
    pin_id: &str,
) -> Option<DeclaredNominal> {
    use crate::semantic::basic::mc_ids::McIds;
    use crate::semantic::basic::mc_uval::McUnit;
    let row = pwr_row_for_pin(comp, pin_id)?;
    let text = row.params.iter().find(|p| p.key.is_none())?.text.clone();
    let arg = comp
        .params
        .to_params_for_eval()
        .into_iter()
        .find(|(n, _)| *n == McIds::from(text.as_str()))?
        .1;
    let v = crate::eval::quantity_in(&arg, &McUnit::Volt);
    Some(DeclaredNominal { text: arg, v })
}

/// The class's own protection declaration, decoded for the flat carry
/// ([`InstEntry::protection`]) — exposed-protection-design.md §4, PWR-5.
///
/// The key is read off the instance's *resolved* attribute list, so whatever
/// lands there is what the flat entry records: the definition-body value, or —
/// on a func call site — the key assignment that rewrites it (contract-design
/// §2.4). The value is matched whole and case-sensitively against the two words
/// the registry holds for the key; an absent key — or a value that is neither —
/// leaves the entry unmarked, and PWR-5 then says nothing about the device.
/// Nothing here reads a class name or a pin shape: the declaration is the only
/// witness (world-axioms §1 A1).
pub(crate) fn protection_of(
    comp: &crate::instant::mc_comp::McComponentInst,
) -> Option<ProtectionKind> {
    use crate::semantic::basic::attr_keys;
    let attr = comp
        .resolved_attrs
        .iter()
        .find(|a| a.id.to_string() == attr_keys::KEY_PROTECT)?;
    match crate::semantic::component::mc_attr::attr_values_text(attr.values.iter()).as_deref() {
        Some(attr_keys::WORD_SHUNT) => Some(ProtectionKind::Shunt),
        Some(attr_keys::WORD_SERIES) => Some(ProtectionKind::Series),
        _ => None,
    }
}

/// The declared transient boundary of one component pin, read from the pin's
/// own declaration row (`io 3 = D+ @exposed(esd_contact)` → `["esd_contact"]`).
/// See [`InstEntry::exposed`] — the pin-level spelling of the same word the
/// module port row spells, decoded by the same key reader as the L1 port
/// projection so the two hosts cannot drift apart.
///
/// A row that carries no trailing `@attr` leaves its pins' attrs empty, and a
/// pin id the def's table does not hold (a materialized dynamic bank pin) has
/// no row here at all — both answer "undeclared", and PWR-6 then says nothing
/// about that pin.
pub(crate) fn exposed_of_pin(
    comp: &crate::instant::mc_comp::McComponentInst,
    pin_name: &str,
) -> Vec<String> {
    comp.attrs_of_pin(pin_name)
        .map(|attrs| {
            crate::semantic::module::pi::attr_texts(
                attrs,
                crate::semantic::basic::attr_keys::KEY_EXPOSED,
            )
        })
        .unwrap_or_default()
}

/// The pin-row expectation words on the two axes (pin-expectation v0.3 §4):
/// `@role(...)` for the return-identity axis, `@class(...)` for the
/// signal-class axis. Read through the instance's materialized pins
/// ([`McComponentInst::attrs_of_pin`]), so a conditional-branch row — a
/// parametric component's selected variant, e.g. a sensor's adopted
/// `ADC.SINGLE` / `I2C` / `SPI` interface — carries its words the same as a
/// direct row, including the member defaults the adoption unioned onto the
/// row at parse time (interface-inventory-design.md §7 D3).
pub(crate) fn expectations_of_pin(
    comp: &crate::instant::mc_comp::McComponentInst,
    pin_name: &str,
) -> (Vec<String>, Vec<String>) {
    match comp.attrs_of_pin(pin_name) {
        Some(attrs) => (
            crate::semantic::module::pi::attr_texts(
                attrs,
                crate::semantic::basic::attr_keys::KEY_ROLE,
            ),
            crate::semantic::module::pi::attr_texts(
                attrs,
                crate::semantic::basic::attr_keys::KEY_CLASS,
            ),
        ),
        None => (Vec::new(), Vec::new()),
    }
}

/// The interface-adoption face of one flat component pin (U201 ①②, the
/// role-anchored exclusive-peer gate): which interface family the pin's
/// adoption row names, which declared role the instantiation selected, which
/// role that role names as its `peer`, whether the role declares the adoption
/// lane exclusive to one peer instance, and the lane itself — the `NAME::` of
/// the adoption row, the declared **body identity**.
///
/// A declaration-face carry, like [`expectations_of_pin`]: decoded once at
/// flatten time through the same def-side reading the connection-time judge
/// uses (`stmt.rs` `iface_endpoint_of_point`: pin id → port name through
/// `pin_id_to_names`, port name → the `Mc2Interface` through `names_to_id`),
/// so the flat rule never re-derives adoption from a pin shape or a name and
/// the two readers cannot drift. `None` = the pin adopts no interface — a
/// real answer, not a missing one: the gate says nothing about the pin.
#[derive(Debug, Clone)]
pub struct IfaceLane {
    /// Interface family name — the definition's own name (`XTAL`, not the
    /// adoption row's instance spelling).
    pub family: String,
    /// The role the instantiation selected (`::XTAL(Resonator)` →
    /// `Resonator`). `None` for a roleless adoption, which is never
    /// exclusive.
    pub role: Option<String>,
    /// The selected role's declared `peer` role, if it names one.
    pub peer_role: Option<String>,
    /// The role's `exclusive = true` declaration (U201 ①②): the lane's
    /// terminals pair with exactly **one** peer instance — a resonator body
    /// meets one oscillator body, not two. Roles that declare nothing pair
    /// unrestricted (a multi-input receiver lane is a legal shape).
    pub exclusive: bool,
    /// The adoption lane: the `NAME::` of the adoption row. One lane is one
    /// declared body — the author groups one body's terminals on one row, so
    /// two crystals on one part are two lanes (`XTAL_A::` / `XTAL_B::`).
    pub lane: String,
}

/// U217: which positional member of an AC mains face this endpoint is. The
/// positional law is the DC pair's ([`PortInst::dc_pair`]): member order is
/// declaration order — the first member is the supply face, the second the
/// declared return. The spelling (`feed{L, N}` → `L`/`N`) rides along only
/// for the diagnostic's message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcFaceMember {
    Hot,
    Ret,
}

/// U217: the AC mains face an endpoint belongs to, decoded once at flatten
/// time (module rows from the port's own declaration, component rows from the
/// pin's resolved interface binding). `face` is the row's written port name —
/// with the entry's owner id, the identity both members of one face share;
/// `member` is `None` for a component-row carry, which states the nominal but
/// no positional face.
#[derive(Debug, Clone)]
pub struct AcFaceCarry {
    pub face: String,
    /// The member's written spelling (`L`), for the message only — the
    /// positional [`AcFaceMember`] is the semantic half.
    pub label: String,
    pub member: Option<AcFaceMember>,
    pub volts: Option<f64>,
    pub hz: Option<f64>,
}

/// One role attribute's declared value set, read the way the connection-time
/// judge reads it (`stmt.rs` `iface_attr_value_set`): a `Set` expression
/// flattens to its items, anything else reads as written, quotes stripped.
pub(crate) fn iface_role_attr_values(
    values: &[crate::McAttrVal],
) -> Vec<String> {
    let mut out = Vec::new();
    for val in values {
        if let crate::McAttrVal::AttrExpr(crate::semantic::basic::mc_expr::McExpression::Set(
            items,
        )) = val
        {
            out.extend(
                items
                    .iter()
                    .map(|e| {
                        crate::semantic::basic::mc_literal::strip_string_quotes(
                            e.to_string().trim(),
                        )
                        .trim()
                        .to_string()
                    })
                    .filter(|s| !s.is_empty()),
            );
        } else {
            let s = crate::semantic::basic::mc_literal::strip_string_quotes(
                format!("{}", val).trim(),
            )
            .trim()
            .to_string();
            if !s.is_empty() {
                out.push(s);
            }
        }
    }
    out
}

/// Decode one component pin's interface-adoption face ([`IfaceLane`]) from
/// the instance's materialized definition — the flatten-time twin of the
/// connection-time endpoint resolver, kept to the same three lookups
/// (`pin_id_to_names` → port name, `names_to_id` → `Mc2Interface`, params →
/// role) so neither side can read a different adoption.
pub(crate) fn iface_lane_of_pin(
    comp: &crate::instant::mc_comp::McComponentInst,
    pin_name: &str,
) -> Option<IfaceLane> {
    let names = comp.def.pins.pin_id_to_names.get(pin_name)?;
    let port_name = names.first()?.split('.').next()?;
    if port_name.is_empty() {
        return None;
    }
    let port = comp.def.pins.names_to_id.get(port_name)?;
    let crate::semantic::component::mc_pins::McPinPort::Interface(iface) = port else {
        return None;
    };
    let role = iface.params.iter().find_map(|p| match p {
        crate::semantic::basic::mc_param::McParamValue::Ids(ids) => {
            let name = ids.to_string();
            iface
                .base
                .roles
                .iter()
                .map(|r| r.name.to_string())
                .find(|rn| *rn == name)
        }
        _ => None,
    });
    let role_def = role.as_ref().and_then(|rn| {
        iface
            .base
            .roles
            .iter()
            .find(|r| &r.name.to_string() == rn)
    });
    let first_values = |id: &str| -> Vec<String> {
        role_def
            .map(|r| {
                r.attrs
                    .iter()
                    .filter(|a| a.id.to_string() == id)
                    .flat_map(|a| iface_role_attr_values(&a.values))
                    .collect()
            })
            .unwrap_or_default()
    };
    let peer_role = first_values("peer").into_iter().next();
    let exclusive = first_values("exclusive").iter().any(|v| v == "true");
    Some(IfaceLane {
        family: iface.base.name.to_string(),
        role,
        peer_role,
        exclusive,
        lane: port_name.to_string(),
    })
}

/// U217: the AC mains face a component pin's adoption row declares
/// ([`AcFaceCarry`]) — the resolved `::AC.*` binding's region nominal, decoded
/// by the same per-axis rule the module port row applies
/// (`phases.rs::declared_ac_face_of_params`). A component row states the
/// nominal but no positional face (member `None`): the return gate judges
/// direction-word module rows, where the pair and its positions are the row's
/// own declaration. `None` = the pin adopts no `AC`-family interface — a real
/// answer, not a missing one.
pub(crate) fn ac_face_carry_of_pin(
    comp: &crate::instant::mc_comp::McComponentInst,
    pin_name: &str,
) -> Option<AcFaceCarry> {
    let names = comp.def.pins.pin_id_to_names.get(pin_name)?;
    let port_name = names.first()?.split('.').next()?;
    if port_name.is_empty() {
        return None;
    }
    let port = comp.def.pins.names_to_id.get(port_name)?;
    let crate::semantic::component::mc_pins::McPinPort::Interface(iface) = port else {
        return None;
    };
    if !is_ac_family(&iface.base_name()) {
        return None;
    }
    let AcFaceCarryVolts { volts, hz } = declared_ac_face_of_params(&iface.params);
    Some(AcFaceCarry {
        face: String::new(),
        label: port_name.to_string(),
        member: None,
        volts,
        hz,
    })
}

/// U217: the `AC` interface family test — the pre-dot segment of the
/// interface's own name (`AC.1P` → `AC`), the same one-family law the DC
/// axis's `base_name() == "DC"` gate applies. The one decode both flatten
/// hosts share: the module port row (`phases.rs::instantiate_interface`)
/// and the component pin row ([`ac_face_carry_of_pin`]) read the family gate
/// and the per-axis nominal decode from here, so the two hosts cannot drift.
pub(crate) fn is_ac_family(base_name: &str) -> bool {
    base_name.split('.').next() == Some("AC")
}

/// U217: the declared region nominal of an `::AC.*` row's arguments — the
/// sole scalar `Volt` argument and the sole scalar `Hz` argument (a range or
/// `±` literal is not a value; two of one axis cancel to "do not pick"). The
/// one decode both flatten hosts share.
pub(crate) fn declared_ac_face_of_params(
    params: &[crate::semantic::basic::mc_param::McParamValue],
) -> AcFaceCarryVolts {
    let mut one = |unit: &'static crate::semantic::basic::mc_uval::McUnit| -> Option<f64> {
        let mut found: Option<f64> = None;
        for p in params {
            let crate::semantic::basic::mc_param::McParamValue::UValue(uv) = p else {
                continue;
            };
            if uv.unit() != unit || uv.is_range_or_plusminus() {
                continue;
            }
            if found.is_some() {
                return None;
            }
            found = Some(uv.value());
        }
        found
    };
    AcFaceCarryVolts {
        volts: one(&crate::semantic::basic::mc_uval::McUnit::Volt),
        hz: one(&crate::semantic::basic::mc_uval::McUnit::Hz),
    }
}

/// The (volts, hz) pair [`declared_ac_face_of_params`] decodes, before it is
/// dressed as a carry (the module host adds the positional face, the
/// component host adds none).
pub(crate) struct AcFaceCarryVolts {
    pub volts: Option<f64>,
    pub hz: Option<f64>,
}

/// What this component **is**, as its own `spec` table declares it — the flat
/// carry [`InstEntry::element_class`] and the power-quality axis's answer to
/// "a decoupling capacitor? a filter? a series pass?" (power-quality-design.md
/// §1.2).
///
/// Read from the **definition's** spec table, unlike [`protection_of`], which
/// reads the instance's resolved attributes: at an instance the table is one
/// key whose value is the whole table rendered as text (parameter names not
/// substituted), so the key *set* — the only thing asked here — survives at
/// definition space alone. That is also the scope the answer belongs to: what
/// an element is belongs to the class, and a call site binds values under those
/// keys, it does not restate the class.
///
/// Both spellings of the table are one fact (G2) and give the same read — the
/// table form `spec = [capacitance = cap]` and the dotted form
/// `spec.capacitance = cap` — and the namespace that opens it is asked of the
/// dictionary, so no key string is compared here.
///
/// The key→class column lives in that dictionary
/// ([`crate::semantic::basic::attr_keys::ElementClass`]): the keys that merely
/// accompany an element (`spec.esr`, `spec.dcr`) carry no class, so they can
/// neither add one nor clash. Several marked keys agreeing on one class
/// (`spec.inductance` and `spec.impedance` on a choke) read as that class; two
/// keys naming different classes say the part is not one element; no marked key
/// at all says the spec table does not classify it. The last two both leave the
/// carry `None`, and the rules reading it then say nothing: the axis judges what
/// a declaration states, and silence is not a defect.
pub(crate) fn element_class_of(
    comp: &crate::instant::mc_comp::McComponentInst,
) -> Option<ElementClass> {
    let mut found: Option<ElementClass> = None;
    for attr in comp.def.attrs.iter() {
        let segs = &attr.id.segments;
        if segs.is_empty() || !attr_keys::is_table_namespace(&segs[0].to_string()) {
            continue;
        }
        // A dotted key keeps its sub-key in the remaining segments; the table
        // form keeps it in the id of each inner attribute.
        let subs: Vec<String> = if let Some(sub) = attr.id.sub_path() {
            vec![sub]
        } else {
            attr.values
                .iter()
                .filter_map(|v| match v {
                    crate::semantic::component::mc_attr::McAttrVal::Attributes(inner) => Some(
                        inner
                            .iter()
                            .map(|row| row.id.to_string())
                            .collect::<Vec<_>>(),
                    ),
                    _ => None,
                })
                .flatten()
                .collect()
        };
        for sub in subs {
            let Some(class) = attr_keys::element_of_spec_key(&sub) else {
                continue;
            };
            match found {
                None => found = Some(class),
                Some(prev) if prev == class => {}
                Some(_) => return None,
            }
        }
    }
    found
}

/// Read one `spec` sub-key's **quantity** off the instance's *resolved*
/// attribute list — the flat carry [`InstEntry::resistance_ohm`] /
/// [`InstEntry::power_rated_w`] (package-thermal-design.md §6).
///
/// Unlike [`element_class_of`], which reads the definition's spec key *set*, a
/// quantity only exists after parameter substitution: at a definition
/// `spec.power_rated = prated` names a formal, and the bound literal lives on
/// the instance's `resolved_attrs` (contract-design.md §2.4 — the same source
/// [`protection_of`] reads). `spec_key` is the dotted key as registered
/// (`spec.power_rated`); both spellings of the table are one fact (G2) and give
/// the same read — the table form `spec = [power_rated = prated]` and the
/// dotted form `spec.power_rated = prated`. A value that does not decode in
/// `family` (`_`, a window, a foreign unit) leaves the carry `None`: a quantity
/// the author did not write is never guessed.
pub(crate) fn spec_quantity_of(
    comp: &crate::instant::mc_comp::McComponentInst,
    spec_key: &str,
    family: &crate::semantic::basic::mc_uval::McUnit,
) -> Option<f64> {
    use crate::semantic::component::mc_attr::{attr_values_text, McAttrVal};
    let (ns, sub) = spec_key.split_once('.')?;
    for attr in comp.resolved_attrs.iter() {
        let segs = &attr.id.segments;
        let text = if segs.len() == 1 && segs[0].to_string() == ns {
            attr.values
                .iter()
                .filter_map(|v| match v {
                    McAttrVal::Attributes(inner) => inner.iter().find(|r| r.id.to_string() == sub),
                    _ => None,
                })
                .next()
                .and_then(|row| attr_values_text(row.values.iter()))
        } else if segs.len() > 1
            && segs[0].to_string() == ns
            && attr.id.sub_path().as_deref() == Some(sub)
        {
            attr_values_text(attr.values.iter())
        } else {
            continue;
        };
        if let Some(v) = text.and_then(|t| crate::eval::quantity_in(&t, family)) {
            return Some(v);
        }
    }
    None
}

/// The declared power face of one pin of a component: the `::DC(hot, ret)`
/// contract of the row that owns it when the row writes one, else the face its
/// declared role names (a bare `psrc/psnk/psbi` row declares a face without
/// naming a pair), else `None` for a pin nothing declares a power face for.
///
/// `role` is the caller's [`infer_member_role`] answer, itself grounded on the
/// component's own `pins.pwr` rows and the owning module's declarations.
pub(crate) fn declared_member_of_pin(
    comp: &crate::instant::mc_comp::McComponentInst,
    pin: &crate::semantic::component::mc_pins::McPin,
    info: Option<&MemberInfo>,
) -> Option<DeclaredMember> {
    let names: Vec<&str> = pin.names.iter().map(|n| n.as_str()).collect();
    if let Some(member) = pwrid::member_of_names(&comp.def.pins, &names) {
        return Some(member);
    }
    let spelling = names.first().copied().unwrap_or_default();
    declared_member_of_role(spelling, info.map(|i| i.role.clone()))
}

/// [`declared_member_of_pin`] for a caller that holds only the declared spelling
/// and the resolved role (a module port member, a module-scope net label).
pub(crate) fn declared_member_of_role(
    spelling: &str,
    role: Option<MemberRole>,
) -> Option<DeclaredMember> {
    if spelling.is_empty() {
        return None;
    }
    match role {
        Some(MemberRole::Ground) => Some(pwrid::member_from_face(spelling, Face::Ret)),
        Some(MemberRole::Power) => Some(pwrid::member_from_face(spelling, Face::Hot)),
        _ => None,
    }
}

// InstOrigin

/// ★ M0-B-E: instance origin —— whether the device came from a declaration or a funcall
#[derive(Debug, Clone)]
pub enum InstOrigin {
    /// Instance produced by a declaration (`RES R1(10kΩ)` etc.)
    Declared,
    /// Instance generated by a function call (`.Cap()` / `.Pullup()` / `.ESD()` etc.)
    FuncCall {
        fn_name: String,
        /// Byte offset of the construction site in the owning file
        /// (decision A, §7.1; 0 = unknown). Convert to a line for display.
        line: u32,
        /// Back-link to the expansion record that produced this instance
        /// (§7.4). Not part of `PartialEq` — provenance only, does not
        /// change semantic comparison (verify / golden depend on that).
        expansion_id: Option<usize>,
    },
}

impl PartialEq for InstOrigin {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (InstOrigin::Declared, InstOrigin::Declared) => true,
            (
                InstOrigin::FuncCall {
                    fn_name: a,
                    line: la,
                    ..
                },
                InstOrigin::FuncCall {
                    fn_name: b,
                    line: lb,
                    ..
                },
            ) => a == b && la == lb,
            _ => false,
        }
    }
}

impl Eq for InstOrigin {}

impl Default for InstOrigin {
    fn default() -> Self {
        InstOrigin::Declared
    }
}

// InstEntry - Single instance record

/// How a class declares itself a protection device (`protect = shunt|series`
/// in the definition body — exposed-protection-design.md §4, PWR-5).
///
/// A fuse/PTC and a TVS/MOV are both two-terminal pass elements in the
/// netlist, and neither carries a DC row, so nothing structural tells a
/// protection device from an ordinary copper pass: the *declaration* is the
/// only witness, and this is its decoded value. An unmarked class (the
/// default) is an ordinary pass element and PWR-5 says nothing about it —
/// no name table, no inferred classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtectionKind {
    /// The device dumps to a reference: it must reach a protective/earth one.
    Shunt,
    /// The device sits in series: it must be a two-terminal element on a
    /// supply path, never a bypass.
    Series,
}

/// Single instance record
#[derive(Debug, Clone)]
pub struct InstEntry {
    /// Globally unique ID
    pub id: u32,
    /// Full hierarchical path: "main.submod1.res102.1"
    pub path: String,
    /// Instance type
    pub kind: InstKind,
    /// Parent instance ID (None for top-level module)
    pub parent_id: Option<u32>,
    /// Definition class name: "Res", "comp.sub", "power_domain" (empty string for pin/port/label)
    pub class_name: String,
    /// IO type (only meaningful for Port/Pin, otherwise IOType::None)
    pub io_type: IOType,
    /// ★ Structural: real resolved pin count of a component instance, recorded at
    /// flatten time (= `comp.pins.len()`: static + materialized dynamic pins).
    /// Non-component entries stay 0. Lets vector consumers classify two-pin vs
    /// multi-pin from the class's pin data instead of class-name heuristics.
    pub pin_count: usize,
    /// **Every** wiring site that reached this entity, in walk order (from
    /// NetPoint / AST). One endpoint can be wired by more than one statement
    /// (`u1.P.A -> u2.P.A` and `u1.P.A -> u3.P.A` both name `u1.P.A`), so this
    /// is a set, not a single winner: collapsing it to one value silently
    /// dropped the second site from every readout.
    ///
    /// Cardinality 0 = never wired. See [`SourcePosSet`] for the order contract.
    pub src_pos: crate::semantic::common::SourcePosSet,
    /// Coarse fallback position for diagnostics: where the entity was *declared*
    /// (e.g. a component pin's pin-id in the component body).
    ///
    /// Filled **unconditionally**, in parallel with `src_pos` — the two are not
    /// ranked. Which of them anchors an output row is a readout decision, which
    /// the `via` key states; it is not a priority between these two fields.
    pub fallback_pos: Option<crate::semantic::common::SourcePos>,
    /// URI of the file where this instance was defined
    pub def_uri: String,
    /// ★ Member role and voltage (for interface members / power pins)
    pub member_info: Option<MemberInfo>,
    /// ★ Typed direction carry for a **component** power pin (`InstKind::Pin`):
    /// the `PwrDir` of the `McPwrPin` row whose `hot` member this pin's function
    /// name matches, recorded at flatten time from the component def's own
    /// `pins.pwr` rows (same equality test [`infer_member_role`] uses to recover
    /// the role — no name heuristic, no io-type guess).
    ///
    /// Motivation: a component's power face loses its direction on the flat
    /// `io_type` (`IOType::Power` for every member), so a viz/rule consumer
    /// cannot tell `psrc` from `psnk` from the flat entry alone. The module-level
    /// equivalent already exists (`power_decls[].pwr_ports[].dir`); this is the
    /// leaf-component counterpart. `None` for every other kind (Port/Pin of a
    /// module, Label, Bus, Module, Component, …) and for a power pin whose
    /// function name matches no `hot` row.
    pub pwr_dir: Option<PwrDir>,
    /// ★ Instance-bound nominal of this endpoint's power contract (U266 ②):
    /// the owning row's positional `::DC(NAME)` nominal spells a declared
    /// constructor formal, resolved at flatten time against the instance's
    /// bound argument — see [`DeclaredNominal`]. Recorded only on the row's
    /// hot terminal, the same endpoint [`Self::pwr_dir`] lands on. `None`
    /// for a literal nominal (the declaration decodes it alone) and for
    /// every non-power endpoint.
    pub pwr_nom: Option<DeclaredNominal>,
    /// ★ Declared power-face identity of this endpoint: which side of a written
    /// `[hot, ret]` contract it is, spelled as the declaration spelled it, under
    /// the declaration that owns it (see [`crate::semantic::pwrid`]).
    ///
    /// Carried here so the rules never have to *guess* a face from the shape of
    /// a name — the flat table is the one place that still holds the
    /// declaration at hand, so the answer is computed once, at flatten time, and
    /// read thereafter. Sources, strongest first: the `::DC(hot, ret)` contract
    /// of the owning `psrc/psnk/psbi` row / declared connection-point pair, the
    /// owning module's own rail / power-port / `conduit` declarations for a
    /// module-scope label, and finally the endpoint's declared direction word
    /// (a `psrc/psnk/psbi` row with no `::` tail still declares a face).
    ///
    /// `None` means the author declared no power face for this endpoint — which
    /// is a real answer, not a missing one: an undeclared label carries no
    /// promise that it is a rail, however it is spelled.
    pub pwr_member: Option<crate::semantic::pwrid::DeclaredMember>,
    /// ★ §11.1: vector member projection — the declared vector group this
    /// flattened component entry belongs to (None for scalar / non-vector).
    /// Populated during `flatten_module` from the modeling-layer `vectors`
    /// groups; consumers reverse-query the vector structure via
    /// [`InstTable::vector_member_paths`].
    pub vector_info: Option<VectorMemberInfo>,
    /// ★ M0-B-D: not-fitted marker (from McComponentInst.nc)
    pub not_fitted: bool,
    /// ★ U48: this individual pin/port is explicitly marked not-connected at
    /// the instance site (`CHIP d1 @ncpin(1,3)`) — the instance-level
    /// counterpart of the class-level `nc` direction word.
    ///
    /// Pure suppression marker: it never touches the netlist, the connections,
    /// the BOM or the viz. Its only consumers are the "unconnected" diagnostic
    /// family (`is_nc_entry`, `check_unused_pins`, the E4116 denominator).
    /// A pin that is already class-level NC never carries it, so a marked entry
    /// and a `NonCon` entry never overlap and no count subtracts twice.
    pub nc_marked: bool,
    /// ★ abstract-variant plan §6.1/§3.2: unselected marker — set when the
    /// instance's def is an `abstract component` placed without a materialized
    /// variant (`component Y : X`), so a BOM tool must still pick a part.
    /// Pure def-marker (`McComponent.is_abstract`); never inferred from a
    /// `partno` sentinel (abstract defs may legally carry a reference partno).
    pub unselected: bool,
    /// ★ PWR-5: protection classification carried from the class's own
    /// definition-body declaration (`protect = shunt|series`,
    /// exposed-protection-design.md §4). Def-marker, exactly like
    /// [`Self::unselected`]: decoded once at flatten time from the definition's
    /// resolved attributes, never inferred from a class name or pin shape.
    /// `None` = ordinary pass element (the unmarked default), which no PWR-5
    /// half adjudicates.
    pub protection: Option<ProtectionKind>,
    /// ★ PWR-6 (exposed-protection-design.md §8.4): the declared transient
    /// boundary of this endpoint — the `@exposed(<level>)` levels its own
    /// declaration row carries. Non-empty on exactly two kinds of entry: a
    /// module port row (`io USB_DP @exposed(esd_contact)`, decoded by the rule
    /// from the L1 port projection) and a component pin row (`io 3 = D+
    /// @exposed(esd_contact)`, carried here — the pin-level spelling, whose
    /// only home before this field was the definition table, where no rule can
    /// see it).
    ///
    /// A declaration-face carry, like [`Self::pwr_member`]: the words are the
    /// author's, copied at flatten time and never inferred from a pin shape or
    /// a class name. Empty means the author declared no boundary — which is a
    /// real answer, not a missing one.
    pub exposed: Vec<String>,
    /// ★ pin-expectation v0.3 (§4): the pin row's own expectation words on
    /// the two axes — `exp_role` the `@role(...)` words (return identity,
    /// `quiet` being the one with a net-side reading), `exp_class` the
    /// `@class(...)` words (analog/digital signal class). A declaration-face
    /// carry like [`Self::exposed`]: decoded once at flatten time through
    /// [`expectations_of_pin`], from the instance's materialized pins — so a
    /// conditional-branch row (the selected variant of a parametric
    /// component) and an adoption row (whose interface member defaults were
    /// unioned onto it at parse time, D3) carry their words exactly like a
    /// direct row. Empty means the author declared no expectation on that
    /// axis — a real answer, not a missing one.
    pub exp_role: Vec<String>,
    pub exp_class: Vec<String>,
    /// ★ U201 ①②: the pin's interface-adoption face — family, selected
    /// role, that role's `peer`, the role's `exclusive` declaration, and the
    /// adoption lane (the `NAME::` of the row). A declaration-face carry like
    /// [`Self::exp_role`]: decoded once at flatten time by
    /// [`iface_lane_of_pin`] from the instance's materialized definition —
    /// the same three lookups the connection-time endpoint resolver makes —
    /// so the flat exclusive-peer gate never re-derives adoption. `None` =
    /// the pin adopts no interface, a real answer the gate stays silent on.
    pub iface_lane: Option<IfaceLane>,
    /// ★ U217 (ac-interface-design.md §7): the AC mains face this endpoint
    /// belongs to — the `::AC.*` contract the owning row declares, with the
    /// region nominal it states and, for a direction-word module row, which
    /// positional member (first = the supply face, second = the declared
    /// return) this endpoint is. A declaration-face carry like
    /// [`Self::iface_lane`]: decoded once at flatten time, from the port's own
    /// declaration (module rows) or the pin's resolved interface binding
    /// (component rows) — never from a name or a shape. `None` = the endpoint
    /// declares no AC face, a real answer the AC gates stay silent on.
    pub ac_face: Option<AcFaceCarry>,
    /// ★ PI axis (power-quality-design.md §1.2): the element class this
    /// component's own `spec` table declares it to be — decoupling capacitor,
    /// filter magnetics, or a dissipating pass — so a rule can ask "is this a
    /// capacitor?" of the declaration instead of a class name or a pin shape.
    ///
    /// Def-marker, exactly like [`Self::protection`]: decoded once at flatten
    /// time by [`element_class_of`] from the definition's spec key set, and
    /// `None` both for a part whose table marks no class and for one whose
    /// marks disagree — neither is a defect, and a rule reading the carry stays
    /// silent on both.
    pub element_class: Option<ElementClass>,
    /// ★ PWR-4b (package-thermal-design.md §6): the element's declared
    /// resistance in ohms, decoded once at flatten time from the instance's
    /// resolved `spec.resistance`. Instance-level, unlike
    /// [`Self::element_class`] — a quantity exists only after parameter
    /// substitution, so the flat carry records the *bound* value the wiring
    /// actually uses. `None` when the class declares no resistance, or declares
    /// one that does not decode as ohms (`_`, a window, a foreign unit).
    pub resistance_ohm: Option<f64>,
    /// ★ PWR-4b (package-thermal-design.md §6): the element's declared package
    /// dissipation rating in watts, decoded the same way from the instance's
    /// resolved `spec.power_rated`. `None` for a class that rates nothing (a
    /// PTC/NTC writes the key as `_`) — an unrated device is never judged.
    pub power_rated_w: Option<f64>,
    /// ★ M0-B-E: instance origin (declaration vs funcall)
    pub origin: InstOrigin,
    /// ★ virtual: true when this entry belongs to a synthetic wrapper module
    /// generated by virtual instantiation (`module VIRT_<T> { T u_1 }`).
    /// Set from the generation site (build/vinst), never inferred from the
    /// `VIRT_`/`u_1` names, so build / diagnostic layers can distinguish
    /// synthetic wrappers from real user modules and instances.
    pub synthetic: bool,
    /// ★ A′ (2026-09-04): physical-member id this spelling collapses onto.
    ///
    /// Aggregate port / slash-lane / bare-member alias entries are *not*
    /// separate conductors — they are non-physical spellings of one module
    /// boundary point (`lane.rs` §boundary-transparent: one physical point,
    /// one id, both sides of the module boundary). When `Some(target)`, this
    /// entry keeps its structural id (rendering / parent chain / path lookup)
    /// but carries `io_type == IOType::None` and never participates in net
    /// endpoints or E41xx judgments: `resolve_single_path` folds any hit onto
    /// `target` first. A real bus member (only slash spelling, no dotted
    /// member port) stays `None` and remains electrical.
    pub alias_of: Option<u32>,
    /// §3.7 two-space id contract: the frozen circuit's arena node id for this
    /// entry (Phase C [`NodeId`]), threaded at flatten time from the
    /// modelling-layer instance. `None` for entries that own no arena node
    /// (pins, labels, bus members, synthetic points). Products join on this
    /// instead of re-deriving identity from the path string.
    pub node_id: Option<NodeId>,
    /// §3.7: canonical def key `(uri, ident)` of the class this entry
    /// instantiates. `def_uri` records the *usage* file, so a cross-file class
    /// keeps its own declaring uri here. `None` for entries that name no def
    /// (pins, ports, labels, nets).
    pub class_def: Option<McSpaceName>,
    /// ★ Stage-readout §2.1: the endpoint's **stage-comparable key** — the
    /// `PointId` (arena node + stable def-member ordinal) the net layer itself
    /// derives, recorded here so the pin-level bridge reaches the flat table.
    ///
    /// `node_id` above bridges *instances*; this bridges *pins*, and it is the
    /// key every downstream segment aligns on (`stage-readout-design.md` §2).
    /// `Some` for the two kinds that name a physical point — a component pin
    /// (`InstKind::Pin`) and a module port (`InstKind::Port`). `None` for
    /// entries that own no point: labels, bus members, and the synthetic
    /// endpoints the router invents (they have no pin to name).
    ///
    /// The whole `PointId` is stored rather than a bare member ordinal beside
    /// `node_id` on purpose: world axiom A6 forbids a second authority, so the
    /// node half and the member half must be written by one call and never
    /// maintained separately. Consumers read the two halves back together, as
    /// one value, and never re-derive either from the path string.
    pub point: Option<crate::instant::lane::PointId>,
}

impl InstEntry {
    /// The declared power face this endpoint carries, or `None` when the author
    /// declared none. Never derived from the shape of the path.
    pub fn power_face(&self) -> Option<crate::semantic::pwrid::Face> {
        self.pwr_member.as_ref().map(|m| m.face)
    }

    /// The **declared** spelling of this endpoint's face, verbatim — what a
    /// diagnostic should print, and what the rail identity is built from.
    /// `None` for an endpoint nothing declares.
    pub fn power_spelling(&self) -> Option<&str> {
        self.pwr_member.as_ref().map(|m| m.member.as_str())
    }

    /// Declaration-side rail identity (see
    /// [`crate::semantic::pwrid::DeclaredMember::identity`]): two endpoints
    /// share it exactly when one declaration names them the same way. `None`
    /// for an endpoint nothing declares.
    pub fn rail_identity(&self) -> Option<String> {
        self.pwr_member.as_ref().map(|m| m.identity())
    }

    /// The first wiring site in walk order, or `None` when nothing ever wired
    /// this entity. All wiring sites are on [`Self::src_pos`].
    pub fn wired_at(&self) -> Option<&crate::semantic::common::SourcePos> {
        self.src_pos.first()
    }

    /// True when no statement ever wired this entity. Says nothing about the
    /// declaration site — that one is filled either way.
    pub fn unwired(&self) -> bool {
        self.src_pos.is_empty()
    }

    /// The single position an output row anchors on: the first wiring site if
    /// there is one, else the declaration site. The precedence lives here and
    /// only here, so every readout states the same rule once; which of the two
    /// actually anchored a row is published as `via`.
    pub fn anchor_pos(&self) -> Option<&crate::semantic::common::SourcePos> {
        self.wired_at().or(self.fallback_pos.as_ref())
    }
}

// NetEntry - Network record

/// Network record, representing an electrical network after flattening
///
/// Each network connects several `InstEntry`s (pins, ports, labels, etc.),
/// referenced by their IDs in `points`.
///
/// ## Example
/// ```text
/// net "VCC" (#5001): [#1003(main.VCC), #1007(main.R1.1), #1012(main.R2.1)]
/// net "GND" (#5002): [#1004(main.GND), #1008(main.R1.2)]
/// ```
#[derive(Debug, Clone)]
pub struct NetEntry {
    /// Network unique ID
    pub id: u32,
    /// Network name (port name > label name > anonymous `_net{N}`)
    pub name: String,
    /// ★ Net-island attribution L1 (island-attribution-design.md §5): the
    /// module entry id whose `flatten_nets` produced this record (the scope its
    /// endpoints live under). Every flat net is created from exactly one
    /// module's frozen string net table, so this is always `Some` at flatten
    /// time; `None` is reserved for a net with no locatable scope. Consumers
    /// that need per-module identity (island/copper attribution, S-set, the
    /// per-scope 6011/6019/6021 refinement) key on this instead of re-deriving
    /// scope from point-path prefixes.
    pub module: Option<u32>,
    /// InstEntry IDs of all endpoints belonging to this network
    pub points: Vec<u32>,
}

// InstTable - Flattened instance table

/// Flattened instance table
///
/// Flattens the nested McModuleInst tree into a one-dimensional ID → entry
/// mapping, while maintaining a path → ID index for fast lookup.
/// Contains network information and can be directly consumed by the drawing side.
#[derive(Debug)]
pub struct InstTable {
    /// Next available ID
    next_id: u32,
    /// id -> entry (ordered by ID)
    entries: BTreeMap<u32, InstEntry>,
    /// path -> id (fast lookup)
    path_index: HashMap<String, u32>,

    /// Network ID counter
    net_id_counter: u32,
    /// net_id -> NetEntry (ordered by ID)
    nets: BTreeMap<u32, NetEntry>,
    /// point_id -> net_ids (reverse index from endpoint to every net segment it
    /// belongs to). 1:N since A′ (2026-09-04): a module-boundary point id is a
    /// *junction* shared by the child module's internal net segment and the
    /// parent net segment (they are different NetEntries — no global fusion).
    point_to_net: HashMap<u32, Vec<u32>>,

    /// ★ M11.3: full paths of bridge passive components (Transposed 2-pin devices)
    bridge_passive_paths: HashSet<String>,

    /// Phase D: the circuit-wide frozen string net-table store, keyed by
    /// canonical module path. `flatten_nets` sources each module's table from
    /// here (`McModuleInst` never carries `NetPoint`); consumers that need the
    /// tree-level string nets (export netlist, viz ground override) read
    /// through [`Self::net_table`].
    net_table: Rc<RefCell<NetTableStore>>,

    /// Power-intent declarations of every module instance in the flat tree,
    /// keyed by the module entry id registered in [`Self::flatten_module`].
    /// Each entry is the *owning def's* capture (`ref` roles / `@star` and the
    /// connection-net relation edges — design §3), cloned at flatten time so a
    /// FlatErc rule can read the declaration set whose nets live at that
    /// instance's prefixed path. Store-only threading: the L1 checks (PWR-2 /
    /// PWR-7) consume this; the edges never merge L0 copper.
    power_decls: BTreeMap<u32, McPowerDecls>,

    /// ★ U168: display-only partition table per module entry id — the owning
    /// def's `block` groupings ([`BlockPartitions`]), cloned at flatten time
    /// beside `power_decls`. Consumed by the viz block-frame projection; no
    /// semantic rule reads it and no id is issued for a partition.
    block_parts: BTreeMap<u32, crate::semantic::common::BlockPartitions>,

    /// Module-scope net origin offsets, keyed by module entry id: net name →
    /// byte offset of that net's **defining token** in the module's file.
    /// "Defining" = declaration (conduit `ref` decl / declared port bus-member
    /// decl) when the net is declared, else its earliest net-name reference
    /// (first use). Threaded at flatten time from the owning def's power-intent
    /// decls + LSP net-ref spans so the GHOST_PORT(4055) anchor for a
    /// module-scope Label/Bus pseudo endpoint can point at the net's own token
    /// instead of an unrelated statement head. Store-only: no semantics.
    net_origin: BTreeMap<u32, std::collections::HashMap<String, u32>>,

    /// ★ Root declaration span: byte range of the built module's own
    /// `module <name>` header in its def file, captured at flatten time from the
    /// root `McModuleInst.def.span`. Gives design-scope FlatErc summaries (4118
    /// power-net count) a real anchor at the module header instead of
    /// `file:1:1` — the net checks themselves only see the flat table.
    root_span: Option<crate::semantic::common::SourcePos>,

    /// Declared semantics of every component power pin, keyed by the **member
    /// spelling** a net point uses (`main.ldo33.VOUT.Vout`), captured at the
    /// component flatten site. A wiring that names a pin by its group member
    /// (`ldo33{VOUT}`) registers an on-the-fly endpoint entry under that
    /// spelling, which carries no io / role / direction of its own; this map
    /// lets [`Self::flatten_nets`] hand it the declared contract instead of
    /// manufacturing a semantics-less pin.
    member_pin_sem: HashMap<
        String,
        (
            IOType,
            Option<MemberInfo>,
            Option<PwrDir>,
            Option<DeclaredMember>,
        ),
    >,

    /// ★ Member-spelling → physical pin id. A wiring that names a component
    /// power pin by its *declared member name* — the `{L|R}` through face
    /// `ldo{VIN | VOUT}`, or the plain dotted form `ldo.VIN.Vin` — spells the
    /// pin as its flat member identity (`VIN.Vin`), while the entry is
    /// registered under the pin-table key (`main.LDO.ldo.1`). Keyed by the
    /// full flat spelling (`main.LDO.ldo.VIN.Vin`). Consulted by the path
    /// resolvers (`InstTable::resolve_single_path`, `vector::builder::resolve::
    /// try_resolve_path`) so the reference lands on the declared pin instead of
    /// materialising a phantom pin under the member spelling — a phantom left
    /// the real pin unconnected (drawn as an X) and added a duplicate pin to
    /// the symbol, and made the two pipelines disagree (the block builder's
    /// owner-fallback attached the net to the *component* instead).
    member_pin_alias: HashMap<String, u32>,
}

impl InstTable {
    /// Create a new instance table, specifying the starting ID
    pub fn new(start_id: u32) -> Self {
        Self {
            next_id: start_id,
            entries: BTreeMap::new(),
            path_index: HashMap::new(),
            net_id_counter: start_id + 100_000, // Network ID and instance ID use separate number spaces
            nets: BTreeMap::new(),
            point_to_net: HashMap::new(),
            bridge_passive_paths: HashSet::new(),
            net_table: Rc::new(RefCell::new(NetTableStore::new())),
            power_decls: BTreeMap::new(),
            block_parts: BTreeMap::new(),
            net_origin: BTreeMap::new(),
            root_span: None,
            member_pin_sem: HashMap::new(),
            member_pin_alias: HashMap::new(),
        }
    }

    /// The flattened root module's own declaration span (its `module <name>`
    /// header). `None` when the table has no root span recorded (defensive —
    /// every flatten starts from a real module inst, so this is normally `Some`).
    pub fn root_span(&self) -> Option<&crate::semantic::common::SourcePos> {
        self.root_span.as_ref()
    }

    /// The circuit-wide frozen string net-table store (Phase D). Tree-level
    /// string-net consumers that hold a flat table read per-module tables
    /// here, keyed by canonical module path.
    pub fn net_table(&self) -> Rc<RefCell<NetTableStore>> {
        self.net_table.clone()
    }

    /// Power-intent declaration map threaded at flatten time: module entry id →
    /// owning def's [`McPowerDecls`]. FlatErc L1 rules read the per-instance
    /// declaration sets (ref roles / `@star` / connection-net relation edges)
    /// through this — the map is keyed by the same ids [`Self::get_entry`]
    /// resolves, so a check can recover the module's `path`/`def_uri`.
    pub(crate) fn power_decls(&self) -> &BTreeMap<u32, McPowerDecls> {
        &self.power_decls
    }

    /// ★ U168: the display-only partition table of one module entry (see
    /// [`Self::block_parts`]). `None` when the entry is not a module with a
    /// parsed body (components, pins, synthetic wrappers).
    pub fn block_parts_of(
        &self,
        id: u32,
    ) -> Option<&crate::semantic::common::BlockPartitions> {
        self.block_parts.get(&id)
    }

    /// Module-scope net origin offsets (see [`Self::net_origin`]).
    pub(crate) fn net_origin(&self) -> &BTreeMap<u32, std::collections::HashMap<String, u32>> {
        &self.net_origin
    }

    /// The declared pin a *member spelling* names (see [`Self::member_pin_alias`]).
    /// `path` is the flat spelling without module prefix handling — callers pass
    /// the same `module_path.path` / bare `path` candidates they try against the
    /// path index. Returns `None` when the spelling names no declared pin.
    pub(crate) fn member_pin_of(&self, path: &str) -> Option<u32> {
        self.member_pin_alias.get(path).copied()
    }

    /// Recursively generate flattened instance table from McModuleInst tree.
    ///
    /// `view` (Phase C S3: arena edges + instance store) drives the traversal:
    /// sub-module order follows the arena `children` edges and the content
    /// resolves from the store (design §4 — the tree is a view over arena
    /// edges) instead of the tree's (now-removed) recursive `sub_modules` Vec.
    /// Callers hold the owning build's arena + store and construct the view
    /// (`mcc build` net-check path, `DianLu` flatten projection via
    /// [`Self::from_module_inst_with_arena`]).
    ///
    /// `net_store` (Phase D) carries the circuit-wide frozen string net tables
    /// produced during construction — the tree no longer stores `NetPoint`, so
    /// the projection sources each module's table from here.
    pub fn from_module_inst(
        inst: &McModuleInst,
        start_id: u32,
        net_store: Rc<RefCell<NetTableStore>>,
        view: &TreeView,
    ) -> Self {
        let mut table = InstTable::new(start_id);
        table.net_table = net_store;
        table.flatten_module(inst, "", None, view);
        // NOTE: no global ground merge here (strict DC rail identity). Ground
        // nets stay exactly as wired: each DC rail keeps its own ground
        // (`V5V.GND` != `V3V3.GND`) and grounds merge only through real wiring
        // ties (shared component ground pins, explicit `X.GND -> GND`).
        table
    }

    /// Recursively generate flattened instance table, with the Phase C arena
    /// + instance store driving the traversal. Thin wrapper over
    /// [`Self::from_module_inst`] that builds the view from the owning build's
    /// arena + store.
    pub(crate) fn from_module_inst_with_arena(
        inst: &McModuleInst,
        start_id: u32,
        arena: &NodeArena,
        store: &InstanceStore,
        net_store: Rc<RefCell<NetTableStore>>,
    ) -> Self {
        let view = TreeView::new(arena, store);
        Self::from_module_inst(inst, start_id, net_store, &view)
    }

    /// Register an instance, return the allocated ID
    ///
    /// If the path is already registered:
    /// - New and old kinds are the same → directly reuse the existing ID
    ///   (normal dedup, silent).
    /// - New and old kinds differ → arbitrate per [`InstKind::registration_priority`]:
    ///   * New kind priority is **higher** (structural entity seizes a path
    ///     held by the net side)
    ///     → **in-place upgrade** the entry (replace kind / parent_id /
    ///     class_name / io_type; the ID remains unchanged, and the established
    ///     path_index and parent references remain valid).
    ///   * Otherwise keep the old entry and discard this registration.
    ///
    /// When BOTH the existing and the new registration are structural
    /// (Module/Component/Pin) with different declaration classes, the collision
    /// is reported as GAP3 (E4062 PIN_OCCUPIED_BY_DECLARATION) — two different
    /// declarations materialized to the same physical pin id (see the check
    /// body below for the domain split vs E5151 / 4051 / 4053).
    pub fn register(
        &mut self,
        path: String,
        kind: InstKind,
        parent_id: Option<u32>,
        class_name: String,
        io_type: IOType,
        src_pos: impl Into<crate::semantic::common::SourcePosSet>,
        def_uri: String,
    ) -> u32 {
        let src_pos: crate::semantic::common::SourcePosSet = src_pos.into();
        // Prevent duplicate registration
        if let Some(&existing_id) = self.path_index.get(&path) {
            let existing_kind = self.entries.get(&existing_id).map(|e| e.kind.clone());

            if let Some(existing_kind) = existing_kind {
                // GAP3 (E4062 PIN_OCCUPIED_BY_DECLARATION)
                // "Two different declarations materialize to the same physical
                // pin id" (design §9.3.3 / vector-pipeline §2.3). Fires only when
                // BOTH registrations are structural entities (Module/Component/
                // Pin) AND their declaration classes differ — a flat-layer
                // physical-position preemption the declaration layer cannot see.
                // Every valid-syntax trigger is absorbed by the pass1
                // declaration layer (E5151 same-scope instance names, `insts`
                // name-keyed dedup), so this is dormant-by-construction for
                // well-formed MCode — it converts the silent merge into an
                // error should a collision ever reach flatten. The domain split:
                // GAP3 = pin DECLARATION occupancy (here), E5151 = same-scope
                // instance names (pass1), 4051 = per-connection net merge
                // (build side, visit.rs), 4053 = bus pin-group monotonicity
                // (pass1, instref.rs) — mutually exclusive, no double-report.
                let existing_class = self
                    .entries
                    .get(&existing_id)
                    .map(|e| e.class_name.clone())
                    .unwrap_or_default();
                if kind.registration_priority() == 2
                    && existing_kind.registration_priority() == 2
                    && !existing_class.is_empty()
                    && existing_class != class_name
                {
                    crate::db::diagnostic::diagnostic::diagnostic_log(
                        crate::errcodes::PIN_OCCUPIED_BY_DECLARATION,
                        crate::db::diagnostic::diagnostic::DiagnosticLevel::Error,
                        src_pos.first().map(|s| s.offset).unwrap_or(0),
                        1,
                        &crate::errcodes::format_msg(
                            crate::errcodes::PIN_OCCUPIED_BY_DECLARATION,
                            &[
                                &path as &dyn std::fmt::Display,
                                &existing_class as &dyn std::fmt::Display,
                                &class_name as &dyn std::fmt::Display,
                            ],
                        ),
                        &[],
                    );
                }
                if existing_kind != kind {
                    let new_pri = kind.registration_priority();
                    let old_pri = existing_kind.registration_priority();

                    if new_pri > old_pri {
                        // Structural entity (Component/Module/Pin) reclaims a
                        // path held by net side (Port/Bus/Label)
                        // —— in-place upgrade.
                        if let Some(entry) = self.entries.get_mut(&existing_id) {
                            entry.kind = kind;
                            entry.parent_id = parent_id;
                            entry.class_name = class_name;
                            entry.io_type = io_type;
                            // The structural entity replaces the net-side entry
                            // wholesale -- it is another object, so its wiring
                            // sites are a fresh set, not a union.
                            if !src_pos.is_empty() {
                                entry.src_pos = src_pos.clone();
                            }
                            if !def_uri.is_empty() {
                                entry.def_uri = def_uri;
                            }
                        }
                    } else {
                        // Old kind priority >= new kind —— keep the old entry.
                    }
                } else {
                    // Same kind: update io_type if the new one is more specific.
                    // This handles cases like: first registered as Bus with io_type=None,
                    // later registered as Bus with io_type=InOut (from port declaration).
                    if let Some(entry) = self.entries.get_mut(&existing_id) {
                        let needs_update = match (&entry.io_type, &io_type) {
                            // Update if current is None/Unknown and new is more specific
                            (IOType::None, _) if !matches!(io_type, IOType::None) => true,
                            // Update parent_id if current is None
                            (IOType::None, _)
                                if entry.parent_id.is_none() && parent_id.is_some() =>
                            {
                                true
                            }
                            _ => false,
                        };
                        if needs_update {
                            entry.io_type = io_type;
                            if entry.parent_id.is_none() {
                                entry.parent_id = parent_id;
                            }
                        }
                        // Same kind: the two registrations name the same object,
                        // so their wiring sites union in walk order.
                        entry.src_pos.extend_from(&src_pos);
                        if !def_uri.is_empty() && entry.def_uri.is_empty() {
                            entry.def_uri = def_uri;
                        }
                    }
                }
            }
            return existing_id;
        }

        let id = self.next_id;
        self.next_id += 1;

        let entry = InstEntry {
            id,
            path: path.clone(),
            kind,
            parent_id,
            class_name,
            io_type,
            pin_count: 0,
            src_pos,
            fallback_pos: None,
            def_uri,
            member_info: None,
            pwr_dir: None,
            pwr_nom: None,
            pwr_member: None,
            vector_info: None,
            not_fitted: false,
            nc_marked: false,
            unselected: false,
            protection: None,
            exposed: Vec::new(),
            exp_role: Vec::new(),
            exp_class: Vec::new(),
            iface_lane: None,
            ac_face: None,
            element_class: None,
            resistance_ohm: None,
            power_rated_w: None,
            origin: InstOrigin::Declared,
            synthetic: false,
            alias_of: None,
            node_id: None,
            class_def: None,
            point: None,
        };

        self.entries.insert(id, entry);
        self.path_index.insert(path, id);
        id
    }

    /// Set member_info for an entry by ID.
    pub fn set_member_info(&mut self, id: u32, member_info: MemberInfo) {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.member_info = Some(member_info);
        }
    }

    /// Thread the §3.7 two-space identity onto an entry: the frozen circuit's
    /// arena node id plus the canonical def key of its class. The flatten sites
    /// call this right after registering a module / component / port node.
    /// `None` arguments never clear a value already recorded.
    pub fn set_identity(
        &mut self,
        id: u32,
        node_id: Option<NodeId>,
        class_def: Option<McSpaceName>,
    ) {
        if let Some(entry) = self.entries.get_mut(&id) {
            if node_id.is_some() {
                entry.node_id = node_id;
            }
            if class_def.is_some() {
                entry.class_def = class_def;
            }
        }
    }

    /// Thread the stage-comparable pin key onto an entry (see
    /// [`InstEntry::point`]). The pin sites call this immediately after
    /// registering the entry, so `pin_id` (the entry's own row number, a
    /// segment-local index) and `point` (the key) are born in the same step
    /// and can never drift apart.
    ///
    /// `None` never clears a value already recorded — a later, better-informed
    /// site (the back-fill pass in `flatten_nets`) may supply what an earlier
    /// one could not, but no site unsays one.
    pub fn set_point(&mut self, id: u32, point: Option<crate::instant::lane::PointId>) {
        if let (Some(entry), Some(point)) = (self.entries.get_mut(&id), point) {
            entry.point = Some(point);
        }
    }

    /// Set the typed direction carry for a component power pin by ID (see
    /// [`InstEntry::pwr_dir`]). Component flatten sites only.
    pub fn set_pwr_dir(&mut self, id: u32, dir: PwrDir) {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.pwr_dir = Some(dir);
        }
    }

    /// Set the instance-bound nominal carry for a component power pin by ID
    /// (see [`InstEntry::pwr_nom`]). Component flatten sites only.
    pub fn set_pwr_nom(&mut self, id: u32, nom: DeclaredNominal) {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.pwr_nom = Some(nom);
        }
    }

    /// Set the declared transient-boundary levels of a component pin by ID
    /// (see [`InstEntry::exposed`]). Component flatten sites only.
    pub fn set_exposed(&mut self, id: u32, levels: Vec<String>) {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.exposed = levels;
        }
    }

    /// Set the declared expectation words of a component pin by ID (see
    /// [`InstEntry::exp_role`] / [`InstEntry::exp_class`]). Component flatten
    /// sites only.
    pub fn set_expectations(&mut self, id: u32, role: Vec<String>, class: Vec<String>) {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.exp_role = role;
            entry.exp_class = class;
        }
    }

    /// Set the declared interface-adoption face of a component pin by ID
    /// (see [`InstEntry::iface_lane`]). Component flatten sites only.
    pub fn set_iface_lane(&mut self, id: u32, lane: IfaceLane) {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.iface_lane = Some(lane);
        }
    }

    /// Set the declared AC mains face of an endpoint by ID
    /// ([`InstEntry::ac_face`]). Flatten sites only — the module port member
    /// loop and the component pin loops.
    pub fn set_ac_face(&mut self, id: u32, face: AcFaceCarry) {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.ac_face = Some(face);
        }
    }

    /// Set the declared power face of an entry by ID
    /// ([`InstEntry::pwr_member`]). Only the declaration side calls this.
    pub fn set_pwr_member(&mut self, id: u32, member: crate::semantic::pwrid::DeclaredMember) {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.pwr_member = Some(member);
        }
    }

    /// Record the declared semantics of one flat component pin under every
    /// member spelling a net point may use for it (see
    /// [`Self::member_pin_sem`]). `comp_path` is the component's flat path;
    /// `pin_name` is the `comp.pins` key.
    #[allow(clippy::too_many_arguments)]
    fn record_member_pin_sem(
        &mut self,
        comp: &crate::instant::mc_comp::McComponentInst,
        pin_name: &str,
        comp_path: &str,
        io: IOType,
        info: Option<MemberInfo>,
        dir: Option<PwrDir>,
    ) {
        let Some(pin) = comp.def.pins.pins.get(pin_name) else {
            return;
        };
        let member = declared_member_of_pin(comp, pin, info.as_ref());
        let target = self.get_id_by_path(&format!("{comp_path}.{pin_name}"));
        if let (Some(target), Some(member)) = (target, member.clone()) {
            self.set_pwr_member(target, member);
        }
        for name in &pin.names {
            let spelling = format!("{comp_path}.{name}");
            self.member_pin_sem.insert(
                spelling.clone(),
                (io.clone(), info.clone(), dir, member.clone()),
            );
            if let Some(target) = target {
                self.member_pin_alias.insert(spelling, target);
            }
        }
    }

    /// Record the real resolved pin count for a component entry (from
    /// `comp.pins.len()` at flatten time). Non-component entries never set it.
    pub fn set_pin_count(&mut self, id: u32, pin_count: usize) {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.pin_count = pin_count;
        }
    }

    /// ★ U48: flag one entry as explicitly not-connected at the instance site.
    ///
    /// Idempotence by construction: an entry that is *already* NC at class
    /// level (the `nc` direction word — the only definition-site spelling) is
    /// left untouched, so [`InstEntry::nc_marked`] and the class-level marker
    /// never overlap and no downstream count subtracts the same pin twice. The
    /// predicate is the same arm `is_nc_entry` reads.
    fn mark_nc(&mut self, id: u32) {
        if let Some(entry) = self.entries.get_mut(&id) {
            if !matches!(entry.io_type, IOType::NonCon) {
                entry.nc_marked = true;
            }
        }
    }

    /// §11.1: attach the vector-group projection to a flattened component
    /// entry. Called from `flatten_module` when the entry is a member of a
    /// declared vector group.
    pub fn set_vector_info(&mut self, id: u32, vector_info: VectorMemberInfo) {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.vector_info = Some(vector_info);
        }
    }

    /// ★ A′ (2026-09-04): record that `alias_id` is a non-physical spelling of the
    /// physical member `target_id` (aggregate / slash-lane / bare-member alias
    /// entry). The alias keeps its structural id (rendering, parent chain, path
    /// lookup) but loses any electrical direction it carried — aliases never
    /// participate in E41xx unconnected-output judgments.
    fn mark_alias(&mut self, alias_id: u32, target_id: u32) {
        if alias_id == target_id {
            return;
        }
        if let Some(entry) = self.entries.get_mut(&alias_id) {
            entry.alias_of = Some(target_id);
            entry.io_type = IOType::None;
        }
    }

    /// ★ CIMP §1 U97 (the bare-member-spelling defect): the declared member
    /// `Port` of `owner` that a bare member spelling names, if exactly one
    /// exists.
    ///
    /// A wiring may name a bundle member by its **bare** key (`MCU513.8`) while
    /// the declared member is registered under its qualified path
    /// (`MCU513.SPI.8`). Both name one conductor, and the declared member is the
    /// physical one, so the net point folds onto it instead of minting a second
    /// identity for it (`flatten_nets`'s on-the-fly boundary pin carries no
    /// direction, no member semantics, and leaves the declared Port in no net).
    ///
    /// Structural, never a name heuristic: the candidates are `owner`'s **own
    /// direct children** of kind `Port` whose last path segment **equals** the
    /// member key. A tie — two ports of one owner declaring the same member key,
    /// e.g. `port1.A` / `port2.A` — is deliberately **not** resolved: an
    /// ambiguous spelling keeps the caller's default rather than picking one.
    ///
    /// `path` is the net point's path, relative to the same module the owner is
    /// relative to (`MCU513.8` under module `main`), or the bare member key
    /// alone when the owner is carried separately (`8` with owner `MCU513`).
    ///
    /// Two callers, one rule: `flatten_nets`'s on-the-fly boundary fallback and
    /// `vector::builder::resolve`'s owner fallback. The second one used to
    /// attach the whole sub-module box, which loses the member identity —
    /// measured on hbl's root layer, four nets then shared one endpoint, the
    /// bus trunk collapsed and the SPI edge lost its label.
    pub(crate) fn declared_member_port_of(
        &self,
        owner_id: u32,
        path: &str,
        owner_name: &str,
    ) -> Option<u32> {
        let member_key = path
            .strip_prefix(owner_name)
            .and_then(|rest| rest.strip_prefix('.'))
            .unwrap_or(path);
        if member_key.is_empty() {
            return None;
        }
        let suffix = format!(".{member_key}");
        let mut found: Option<u32> = None;
        for (id, e) in self.entries.iter() {
            if e.parent_id != Some(owner_id) || !matches!(e.kind, InstKind::Port) {
                continue;
            }
            if e.path.len() <= suffix.len() || !e.path.ends_with(&suffix) {
                continue;
            }
            if found.is_some() {
                return None;
            }
            found = Some(*id);
        }
        found
    }

    /// ★ A′: fold a path-spelling hit along its alias chain onto the physical
    /// member id. Non-alias entries return unchanged. Bounded against a corrupt
    /// alias cycle (never expected — targets are always real member ids).
    fn fold_alias(&self, mut id: u32) -> u32 {
        let mut guard = 0usize;
        while guard < self.entries.len() {
            let next = self.entries.get(&id).and_then(|e| e.alias_of);
            match next {
                Some(t) if t != id => {
                    id = t;
                    guard += 1;
                }
                _ => break,
            }
        }
        id
    }

    /// §11.1: reverse-query — all flattened paths whose entries belong to the
    /// declared vector group `base`, in member order. The forward projection
    /// (vector_info) is built at flatten time; the reverse index is a
    /// low-frequency O(n) scan here (vector member counts are small, and
    /// consumers such as LSP query `c[1:2]` only on demand).
    pub fn vector_member_paths(&self, base: &str) -> Vec<String> {
        let mut items: Vec<(usize, &str)> = self
            .entries
            .values()
            .filter_map(|e| {
                e.vector_info
                    .as_ref()
                    .filter(|vi| vi.vector_base == base)
                    .map(|vi| (vi.index, e.path.as_str()))
            })
            .collect();
        items.sort_by_key(|(idx, _)| *idx);
        items.into_iter().map(|(_, p)| p.to_string()).collect()
    }

    /// Mark every entry whose path is exactly `prefix` or starts with
    /// `prefix + "."` as a synthetic virtual-instantiation wrapper.
    ///
    /// Called by `virtual_build_flat` with the generated wrapper module name,
    /// so the synthetic marker is attached at the generation site and carried
    /// into the build (and from there into the viz graph), never inferred by
    /// matching the `VIRT_` / `u_1` names downstream.
    pub fn mark_synthetic_by_path_prefix(&mut self, prefix: &str) {
        let scope = format!("{prefix}.");
        for (_id, entry) in self.entries.iter_mut() {
            if entry.path == prefix || entry.path.starts_with(&scope) {
                entry.synthetic = true;
            }
        }
    }

    /// Convenience wrapper for tests — calls `register` with empty source info.
    #[cfg(test)]
    pub fn register_simple(
        &mut self,
        path: String,
        kind: InstKind,
        parent_id: Option<u32>,
        class_name: String,
        io_type: IOType,
    ) -> u32 {
        self.register(
            path,
            kind,
            parent_id,
            class_name,
            io_type,
            None,
            String::new(),
        )
    }

    // Query methods

    /// Find ID by path
    pub fn get_id_by_path(&self, path: &str) -> Option<u32> {
        self.path_index.get(path).copied()
    }

    /// Find entry by ID
    pub fn get_entry(&self, id: u32) -> Option<&InstEntry> {
        self.entries.get(&id)
    }

    /// The definition this entry belongs to: its own `class_def`, or the
    /// nearest ancestor's.
    ///
    /// §3.7: `class_def` is set for the kinds that name a def — a module, a
    /// component — while a pin or a port names a *member* of one and carries
    /// none, so its def half is recovered by walking `parent_id` up to the
    /// first entry that declares one. `parent_id` strictly decreases towards
    /// the root, so the walk terminates.
    ///
    /// **One owner for the walk.** The rows of `export::instlist`, the
    /// two-space key of `stages::def_of` and the keys of the reverse index
    /// (`instant::reverse`) all read it here, so no two faces of the same
    /// entry can name different defs.
    pub fn class_def_of(&self, id: u32) -> Option<&McSpaceName> {
        let mut cur = Some(id);
        while let Some(cid) = cur {
            let e = self.entries.get(&cid)?;
            if let Some(sn) = &e.class_def {
                return Some(sn);
            }
            cur = e.parent_id;
        }
        None
    }

    /// Get all direct child instances under a given parent node
    pub fn children_of(&self, parent_id: u32) -> Vec<&InstEntry> {
        self.entries
            .values()
            .filter(|e| e.parent_id == Some(parent_id))
            .collect()
    }

    /// Iterate all entries (ordered by ID)
    pub fn iter(&self) -> impl Iterator<Item = (&u32, &InstEntry)> {
        self.entries.iter()
    }

    /// Return total entry count
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the table is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// ★ M11.3: check whether a component path is a bridge passive (Transposed 2-pin device)
    pub fn is_bridge_passive(&self, path: &str) -> bool {
        self.bridge_passive_paths.contains(path)
    }

    // Network query methods

    /// Get all networks
    pub fn get_nets(&self) -> Vec<&NetEntry> {
        self.nets.values().collect()
    }

    /// Find network by ID
    pub fn get_net(&self, net_id: u32) -> Option<&NetEntry> {
        self.nets.get(&net_id)
    }

    /// Find the network a given endpoint belongs to.
    ///
    /// A boundary point may belong to several net segments (junction); this
    /// returns the lowest-id segment deterministically. Callers that need *all*
    /// segments (short detection, rail binding) use [`Self::nets_of`].
    pub fn get_net_of(&self, point_id: u32) -> Option<&NetEntry> {
        self.nets_of(point_id)
            .first()
            .and_then(|&net_id| self.nets.get(&net_id))
    }

    /// ★ A′ (2026-09-04): every net segment this point id belongs to. Net ids
    /// ascend in insertion order, so `.first()` is the lowest-id segment.
    pub fn nets_of(&self, point_id: u32) -> &[u32] {
        self.point_to_net
            .get(&point_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Get all component entries
    pub fn get_components(&self) -> Vec<&InstEntry> {
        self.entries
            .values()
            .filter(|e| e.kind == InstKind::Component)
            .collect()
    }

    /// Every flat entry, in id order (U217: the AC face gates group entries by
    /// declaration face, so they walk the whole table rather than the nets —
    /// a dangling face member is exactly the endpoint no net names). Id order
    /// is the registration order ([P0-DET]), so consumers see a stable
    /// sequence.
    pub(crate) fn iter_entries(&self) -> impl Iterator<Item = &InstEntry> {
        let mut ids: Vec<u32> = self.entries.keys().copied().collect();
        ids.sort_unstable();
        ids.into_iter().filter_map(move |id| self.entries.get(&id))
    }

    /// Get all module entries
    pub fn get_modules(&self) -> Vec<&InstEntry> {
        self.entries
            .values()
            .filter(|e| e.kind == InstKind::Module)
            .collect()
    }

    /// Get all pins of a component
    pub fn get_pins_of(&self, comp_id: u32) -> Vec<&InstEntry> {
        self.entries
            .values()
            .filter(|e| e.parent_id == Some(comp_id) && e.kind == InstKind::Pin)
            .collect()
    }

    /// Get all ports of a module
    pub fn get_ports_of(&self, mod_id: u32) -> Vec<&InstEntry> {
        self.entries
            .values()
            .filter(|e| e.parent_id == Some(mod_id) && e.kind == InstKind::Port)
            .collect()
    }

    /// Return total network count
    pub fn net_count(&self) -> usize {
        self.nets.len()
    }

    // Ground net merging — REMOVED (strict DC rail identity)
    //
    // `merge_ground_nets` (global merge of every `MemberRole::Ground` net into a
    // single "GND" net) has been deleted. Under strict DC rail identity, a
    // module's different DC rails may carry different grounds: `va.GND` and
    // `vb.GND` are distinct nets, and each stays traceable to its rail. Grounds
    // merge only through real wiring ties (shared component ground pins,
    // explicit `X.GND -> GND` connections), which the endpoint union-find in
    // mc_net / visit handles naturally.
    //
    // `MemberInfo`/`MemberRole` are still set on ports/pins below: the viz
    // projection layer (project.rs) and the graph builder (fromblock.rs) use
    // the role to classify rail vs signal endpoints and to extract voltages.
    // Only the global ground merge is gone.

    // flatten traversal (Step 5)

    /// Declaration-site fallback for unconnected ports (AGENTS.md: "the module
    /// span for ports"). A port that is never wired (e.g. E4117 floating
    /// bidirectional) has no net point to back-fill a wiring site into
    /// `src_pos`, so net checks would anchor at offset 0 → file:1:1; anchor
    /// them at the port's declaration span in the module body instead.
    /// Recorded in parallel with `src_pos`, never instead of it.
    fn backfill_port_decl_pos(&mut self, id: u32, def_uri: &str, span: Option<Range<usize>>) {
        if let Some(entry) = self.entries.get_mut(&id) {
            if entry.fallback_pos.is_none() {
                if let Some(span) = span {
                    entry.fallback_pos = Some(crate::semantic::common::SourcePos::new(
                        def_uri.to_string(),
                        span.start as u32,
                    ));
                }
            }
        }
    }

    /// Port declaration span for a flattened port/bus name. Body instances
    /// (`insts.port_spans`) first, then signature interface params
    /// (`def.params.def_spans`) — the latter covers bracket-form params such
    /// as `[VDD_3V3, GND]::DC(3.3V)`, whose whole-name span lives in
    /// `def.params` and is dropped from `port_spans` by `filter_port_spans`.
    fn port_decl_span_of(inst: &McModuleInst, name: &str) -> Option<Range<usize>> {
        inst.def.port_decl_span(name)
    }

    /// Recursively flatten a module instance
    ///
    /// Traversal order: module itself → ports → components + pins →
    /// bus + members → standalone labels → sub-modules (recursive)
    fn flatten_module(
        &mut self,
        inst: &McModuleInst,
        prefix: &str,
        parent_id: Option<u32>,
        view: &TreeView,
    ) {
        // 1. Register the module itself
        let my_path = if prefix.is_empty() {
            inst.name.clone()
        } else {
            format!("{}.{}", prefix, inst.name)
        };
        let my_id = self.register(
            my_path.clone(),
            InstKind::Module,
            parent_id,
            inst.def.name.to_string(),
            IOType::None,
            None,
            inst.def_uri.to_string(),
        );
        self.set_identity(
            my_id,
            inst.node_id,
            Some(McSpaceName::new(&inst.def.name, inst.def.uri.clone())),
        );
        // ★ Root header anchor: record the built module's own `module <name>`
        // declaration span so design-scope net-check summaries (4118 power-net
        // count) can anchor at the module header instead of file:1:1. Only the
        // outermost flatten call carries parent_id == None; sub-module
        // registrations do not overwrite it.
        if parent_id.is_none() {
            self.root_span = Some(crate::semantic::common::SourcePos::new(
                inst.def.uri.clone(),
                inst.def.span.start as u32,
            ));
        }
        // Power-intent L1 threading: capture the owning def's declarations so
        // FlatErc can run the per-module relation-edge checks against this
        // instance. Cloned per instance (defs are shared across instances);
        // the L1 checks dedupe by def below.
        self.power_decls.insert(my_id, inst.def.pi.clone());
        // ★ U168: same threading shape — the def's partition table rides to
        // every instance so the viz layer can attribute boxes by source region.
        self.block_parts.insert(
            my_id,
            crate::semantic::common::BlockPartitions {
                uri: inst.def.uri.clone(),
                roots: inst.def.blocks.roots.clone(),
            },
        );

        // Module-scope net origin map (decl-or-first-ref), store-only
        // threading for GHOST_PORT(4055) anchoring. Definition wins over
        // first use: conduit `ref` declarations, then declared port
        // bus-members (io MIC{P,N} → members anchor on the io row), then the
        // earliest net-name reference from the LSP net-ref spans (usage-born
        // nets such as a bare `[VBUS_RAW, GND]` label keep their own token).
        let mut net_origin_map: std::collections::HashMap<String, u32> =
            std::collections::HashMap::new();
        for r in &inst.def.pi.refs {
            net_origin_map.insert(r.name.clone(), r.span.start as u32);
        }
        for port in &inst.ports {
            if port.bus_members.is_empty() {
                continue;
            }
            if let Some(span) = Self::port_decl_span_of(inst, &port.name) {
                let base = span.start as u32;
                for m in &port.bus_members {
                    net_origin_map.insert(m.clone(), base);
                    net_origin_map.insert(format!("{}.{}", port.name, m), base);
                }
            }
        }
        for (span, name, _scope) in inst.def.insts.iter_net_refs() {
            net_origin_map
                .entry(name.clone())
                .or_insert(span.start as u32);
        }
        self.net_origin.insert(my_id, net_origin_map);

        // Model-A declared member identity (classification-retirement batch2
        // R1): a port member / pin func name gets an electrical role only when
        // it names this module's OWN declared copper — ground-side = every
        // `conduit` name + every declared DC rail's `ret` member (the same set
        // the flat per-statement ground partition exempted from fragmentation,
        // retired per split-ground-copper-design v0.2 §6); supply-side = every
        // declared DC rail's
        // `hot` member. Shared by all three `infer_member_role` call sites in
        // this method (module port members, net-connected pins). Legacy
        // modules with no power-intent declaration → empty sets → Signal.
        let declared_ground_coppers: std::collections::HashSet<String> = {
            let pi = &inst.def.pi;
            pi.l1_refs()
                .iter()
                .map(|r| r.name.clone())
                .chain(pi.l1_rails().iter().map(|r| r.ret.clone()))
                .collect()
        };
        let declared_power_members: std::collections::HashSet<String> = {
            let pi = &inst.def.pi;
            pi.l1_rails().iter().map(|r| r.hot.clone()).collect()
        };
        let is_declared_ground = |m: &str| declared_ground_coppers.contains(m);
        let is_declared_power = |m: &str| declared_power_members.contains(m);

        // 2. Register ports
        for port in &inst.ports {
            let suffixes = port.path_suffixes();
            let port_path = format!("{}.{}", my_path, suffixes.header);
            let port_id = self.register(
                port_path,
                InstKind::Port,
                Some(my_id),
                String::new(),
                port.iotype.clone(),
                None,
                inst.def_uri.to_string(),
            );
            self.set_identity(port_id, port.node_id, None);
            // ★ Stage-readout §2.1: the port's own point — the module node plus
            // the port's id in the module def's registry-owned port ledger,
            // resolved by the net layer's single authority so both segments
            // name the port the same way. The root module's ports are the
            // circuit boundary, where a rename is a pure label change
            // (world-equivalence §10.3), so they stay positionally anchored.
            self.set_point(
                port_id,
                crate::instant::lane::resolve_port_ordinal(inst, &port.name, parent_id.is_none()),
            );
            // Signature interface params (e.g. `[VDD_3V3, GND]::DC(3.3V)`)
            // are declared in `def.params`, not `def.insts` — when the body
            // instance lookup misses, fall back to the param declaration span
            // so unconnected-port diagnostics anchor at the declaration
            // instead of file:1:1.
            let port_decl_span = Self::port_decl_span_of(inst, &port.name);
            self.backfill_port_decl_pos(port_id, &inst.def_uri, port_decl_span.clone());
            // ★ U48: the instance-site NC marker carries the port's registered
            // suffixes (resolved in `mc_mod::phases`, never re-derived here), so
            // this is a plain lookup — no parsing, no name heuristics.
            if inst.nc_ports.contains(&suffixes.header) {
                self.mark_nc(port_id);
            }

            // ★ A′ (2026-09-04): a port that carries members is a *grouping
            // header*, not a leaf conductor — the members registered below are
            // the physical, direction-bearing points. Track every header path
            // for this port so it can be de-electrified after the member loop;
            // a scalar (member-less) port keeps its io and stays a real point.
            let mut aggregate_headers = vec![port_id];

            // ── Phase-D support: register a bracketed path for ports with bus_members ──
            // Only create bracketed path for List ports (e.g., [A,B] or GPIO[1:2]),
            // NOT for Bus ports (e.g., rs485{A,B}) because Bus ports can be accessed
            // via the dot syntax (rs485.A, rs485.B).
            // Check if the port name contains '[' to identify List-style ports.
            if let Some(bracket_name) = suffixes.bracket.clone() {
                let bracket_path = format!("{my_path}.{bracket_name}");
                let bracket_id = self.register(
                    bracket_path,
                    InstKind::Port,
                    Some(my_id),
                    String::new(),
                    port.iotype.clone(),
                    None,
                    inst.def_uri.to_string(),
                );
                self.backfill_port_decl_pos(bracket_id, &inst.def_uri, port_decl_span.clone());
                // ★ U48: same lookup as the header — the bracket alias is part
                // of the port's registered name surface.
                if inst.nc_ports.contains(&bracket_name) {
                    self.mark_nc(bracket_id);
                }
                aggregate_headers.push(bracket_id);
            }

            // ── P2-4: register individual bus member ports ──
            // Net points use plain member names (e.g. "VDD_3V3"), not bracket form.
            // Without individual registration, resolve_netpoint_path can't find them
            // and the port points are silently dropped from the InstTable nets.
            // This causes port-only nets (e.g. V5V.VCC, MIC.P, DAC) to be invisible
            // to netdiff and downstream consumers.
            //
            // ── P2-4: include port name prefix for non-bracket ports ──
            // Bracket ports (e.g. [VDD_3V3, GND]) use flat member paths:
            //   main.dcdc.VDD_3V3
            // Named ports (e.g. vin, vout, USB_VBUS_1) include port name:
            //   main.ldo.vin.VCC
            // This preserves the port→member relationship so netdiff can match
            // golden references like "vin.VCC" and "USB_VBUS_1.VDD_3V".
            for (mi, member) in port.bus_members.iter().enumerate() {
                let member_path = format!("{my_path}.{}", suffixes.members[mi]);
                // diff-pair-design.md (ruled 2026-09-23, was U61): a member
                // one of the interface's `@pair` tuples names is a leg of one
                // declared pair. Both legs carry the port's own path as their
                // group — that shared value is the whole pairing rule, so a
                // pair is recognized whatever its nets are called. The tuple
                // order is member order: the first member is the derived
                // leg A. Recorded whatever the inferred role is, since a
                // differential pair is a signal by nature.
                let diff = port.diff_pair.iter().find_map(|(pos, neg)| {
                    let positive = if member == pos {
                        true
                    } else if member == neg {
                        false
                    } else {
                        return None;
                    };
                    let group = member_path
                        .rsplit_once('.')
                        .map_or(member_path.as_str(), |(g, _)| g);
                    Some(DiffFace::new(group, positive))
                });
                let member_id = self.register(
                    member_path,
                    InstKind::Port,
                    Some(my_id),
                    String::new(),
                    port.iotype.clone(),
                    None,
                    inst.def_uri.to_string(),
                );
                self.backfill_port_decl_pos(member_id, &inst.def_uri, port_decl_span.clone());
                // ★ U48: same lookup — naming the member suppresses exactly
                // that member; the resolver is what widens a header into its
                // members (see `McModuleInst::nc_ports`).
                if inst.nc_ports.contains(&suffixes.members[mi]) {
                    self.mark_nc(member_id);
                }

                // Set member_info role (Ground/Power) — consumed by the viz
                // projection layer for rail classification, not for net merging.
                // A connection-point DC pair declared on this port is decoded
                // positionally and outranks everything: the 2nd member is the
                // declared return (ground side), the 1st is the supply face
                // (classification-retirement-design §4, C). The pair is carried
                // by the declaration, so a scalar `x::DC(v)` row claims the
                // same two faces as a written `[hot, ret]` row does.
                let (role, _inferred) = if let Some((hot, ret)) = &port.dc_pair {
                    if member == ret {
                        (MemberRole::Ground, false)
                    } else if member == hot {
                        (MemberRole::Power, false)
                    } else {
                        infer_member_role(
                            member,
                            &port.iotype,
                            &is_declared_ground,
                            &is_declared_power,
                        )
                    }
                } else {
                    infer_member_role(
                        member,
                        &port.iotype,
                        &is_declared_ground,
                        &is_declared_power,
                    )
                };
                if !matches!(role, MemberRole::Signal) || diff.is_some() {
                    // P3 (ret lineage): a member the pair names carries the other
                    // face — the ret on the hot member, the hot on the ret member.
                    let mut info = MemberInfo::new(role.clone(), None);
                    if let Some((hot, ret)) = &port.dc_pair {
                        if member == ret {
                            info.pair = Some(hot.clone());
                        } else if member == hot {
                            info.pair = Some(ret.clone());
                        }
                    }
                    info.diff = diff;
                    self.set_member_info(member_id, info);
                }
                // Declared power face: the connection-point DC pair names both
                // faces verbatim (`[VDD_3V3, GND]`), so it outranks the role —
                // the pair IS the declaration. Without a pair the member's own
                // declared role still names a face.
                let member_decl = port
                    .dc_pair
                    .as_ref()
                    .and_then(|(hot, ret)| {
                        if member == ret {
                            Some(pwrid::member_from_declared_face(ret, Face::Ret))
                        } else if member == hot {
                            Some(pwrid::member_from_declared_face(hot, Face::Hot))
                        } else {
                            None
                        }
                    })
                    .or_else(|| declared_member_of_role(member.as_str(), Some(role.clone())));
                if let Some(member_decl) = member_decl {
                    self.set_pwr_member(member_id, member_decl);
                }

                // ★ U217 (ac-interface-design.md §7): an `::AC.*` row's face
                // rides the flat member entries, positional like the DC pair —
                // the roles come from the variant's registry group (member i
                // takes the group's position i; the written names are never
                // read). A registered multi-member family (`AC.3P`) carries
                // every member; an unregistered two-member row keeps the pair
                // reading (first the supply face, second the declared
                // return). The carry holds the port path (the identity all
                // members share), the member's spelling, and the row's
                // declared region nominal.
                if let Some(ac) = &port.ac_face {
                    let words = ac.member_words.clone().or_else(|| {
                        (port.bus_members.len() == 2)
                            .then(|| vec![AcFaceMember::Hot, AcFaceMember::Ret])
                    });
                    if let Some(words) = words {
                        if port.bus_members.len() == words.len() {
                            if let Some(&member_word) = words.get(mi) {
                                self.set_ac_face(
                                    member_id,
                                    AcFaceCarry {
                                        face: port.name.clone(),
                                        label: member.clone(),
                                        member: Some(member_word),
                                        volts: ac.volts,
                                        hz: ac.hz,
                                    },
                                );
                            }
                        }
                    }
                }
            }

            // ★ A′: de-electrify the aggregate header(s). Members are physical;
            // the header must not read as an unconnected Out/InOut (E4110/E4117).
            // This also makes flatten step-4 `bus_io` read None for port-derived
            // buses, so their slash lanes inherit no electrical direction.
            if !port.bus_members.is_empty() {
                for hid in aggregate_headers {
                    if let Some(e) = self.entries.get_mut(&hid) {
                        e.io_type = IOType::None;
                    }
                }
            }
        }

        // ── ★ P8-1: build func-to-owner map for correcting parent_id of
        // func-created instances. When a component defines a func (e.g. FLASH.GD25Q32E
        // defines func GD25Q32E), instances created by that func belong to the
        // component instance, not the calling module.
        //
        // ★ P9-A1 fix: only include Declared components. Func-created instances
        // (CAP, RES etc.) are themselves being reparented and cannot be owners.
        let mut func_to_owner: HashMap<String, String> = HashMap::new();
        let comps: Vec<_> = view.components(inst).collect();
        for comp in comps {
            if !matches!(comp.origin, InstOrigin::Declared) {
                continue;
            }
            for func in comp.def.funcs.iter() {
                func_to_owner.insert(func.name.to_string(), comp.name.clone());
            }
        }

        // ★ §11.1: build the vector-member projection map (member name →
        // group info) from the modeling-layer `vectors` groups before the
        // component loop. `member_ids` are the physical instance coordinates
        // within this module (`c1`, `c2`, …), which equal the flat `comp.name`.
        // The map is looked up once per component — vector groups are sparse,
        // so a HashMap build is cheaper than scanning `inst.vectors` per comp.
        let mut vector_member_map: HashMap<String, VectorMemberInfo> = HashMap::new();
        for v in &inst.vectors {
            for (idx, mid) in v.member_ids.iter().enumerate() {
                let member = v
                    .member_names
                    .get(idx)
                    .cloned()
                    .unwrap_or_else(|| mid.clone());
                vector_member_map
                    .entry(mid.clone())
                    .or_insert_with(|| VectorMemberInfo::new(v.base.clone(), member, idx));
            }
        }

        // 3. Register components + pins (two-pass: non-func-created first,
        //    then func-created with corrected parent_id)
        let comps: Vec<_> = view.components(inst).collect();
        for comp in comps {
            if matches!(comp.origin, InstOrigin::FuncCall { .. }) {
                continue; // handled in second pass
            }
            // Model-A connection-point DC pair (classification-retirement-design
            // §4, C full capture): a component pin IS the component's own
            // external connection point — every `McPwrPin` row this def WRITES
            // declares a DC pair at its pins (ret member = declared return /
            // ground side, hot member = declared supply face). Enrich the
            // module-scope declared identity with this component's own rows so
            // a DC return pin (US513.21 — ret of `psnk [5,21]=[VDD,GND]::DC`)
            // reads Ground instead of the io==Power default. Positional:
            // ret/hot come from the ::DC [hot,ret] write, never from names.
            let comp_ground_coppers: Vec<String> = comp
                .def
                .pins
                .pwr
                .iter()
                .filter_map(|p| p.ret.clone())
                .collect();
            let comp_power_members: Vec<String> =
                comp.def.pins.pwr.iter().map(|p| p.hot.clone()).collect();
            let is_comp_ground =
                |m: &str| is_declared_ground(m) || comp_ground_coppers.iter().any(|r| r == m);
            let is_comp_power =
                |m: &str| is_declared_power(m) || comp_power_members.iter().any(|p| p == m);
            let comp_path = format!("{}.{}", my_path, comp.name);
            let comp_id = self.register(
                comp_path.clone(),
                InstKind::Component,
                Some(my_id),
                comp.def.name.to_string(),
                IOType::None,
                None,
                inst.def_uri.to_string(),
            );
            self.set_identity(
                comp_id,
                comp.node_id,
                Some(McSpaceName::new(&comp.def.name, comp.def.uri.clone())),
            );
            // ★ Structural pin count: comp.pins (static + resolved dynamic) is
            // exactly the set registered as Pin children below.
            self.set_pin_count(comp_id, comp.pins.len());

            // ★ Declaration site, recorded for every instance.
            // A fully-unconnected component never appears in a net, so no net
            // point back-fills a wiring site into `src_pos` — net diagnostics
            // (E4116 pin-count, E4112, …) would anchor at offset 0 → file:1:1.
            // The module's instance table records the declaration span of
            // `RES r1` (parse_declare → store_port_span).
            // Filled even when the entry already has wiring sites: the two
            // states are recorded in parallel, and which one anchors a report
            // is decided by the reader (see `via`), not here.
            if let Some(span) = inst.def.insts.get_port_span(&comp.name) {
                if let Some(entry) = self.entries.get_mut(&comp_id) {
                    if entry.fallback_pos.is_none() {
                        entry.fallback_pos = Some(crate::semantic::common::SourcePos::new(
                            inst.def_uri.clone(),
                            span.start as u32,
                        ));
                    }
                }
            }

            // ★ M0-B-D: pass through the nc marker
            if comp.nc {
                if let Some(entry) = self.entries.get_mut(&comp_id) {
                    entry.not_fitted = true;
                }
            }
            // ★ abstract-variant §6.1/§3.2: an instance of an abstract def is
            // unselected — no variant materialized it, so a BOM tool must pick
            // a part. Marker is the def's `is_abstract` only (no partno rule).
            if comp.def.is_abstract {
                if let Some(entry) = self.entries.get_mut(&comp_id) {
                    entry.unselected = true;
                }
            }
            // ★ PWR-5: the class's protection declaration (`protect =
            // shunt|series` in its body) rides the flat entry the same way the
            // def-markers above do — decoded here, consumed by the rule.
            if let Some(kind) = protection_of(comp) {
                if let Some(entry) = self.entries.get_mut(&comp_id) {
                    entry.protection = Some(kind);
                }
            }
            // ★ PI: the element class the class's own `spec` table declares,
            // carried the same way (a def-marker, not an instance property).
            if let Some(class) = element_class_of(comp) {
                if let Some(entry) = self.entries.get_mut(&comp_id) {
                    entry.element_class = Some(class);
                }
            }
            // ★ PWR-4b: the two quantities the dissipation verdict compares,
            // read from the instance's resolved spec values (see
            // `spec_quantity_of` — the definition holds formal names only).
            let resistance = spec_quantity_of(comp, "spec.resistance", &McUnit::Ohm);
            let power_rated = spec_quantity_of(comp, "spec.power_rated", &McUnit::Wat);
            if resistance.is_some() || power_rated.is_some() {
                if let Some(entry) = self.entries.get_mut(&comp_id) {
                    entry.resistance_ohm = resistance;
                    entry.power_rated_w = power_rated;
                }
            }
            // ★ M0-B-E: pass through origin
            if let Some(entry) = self.entries.get_mut(&comp_id) {
                entry.origin = comp.origin.clone();
            }
            // ★ §11.1: attach the vector-group projection for vector members
            if let Some(info) = vector_member_map.get(&comp.name) {
                self.set_vector_info(comp_id, info.clone());
            }

            // ★ M11.3: record bridge passive full paths
            if inst.bridge_passive_names.contains(&comp.name) {
                self.bridge_passive_paths.insert(comp_path.clone());
            }

            // Each pin as an independent entry
            // ★ U152 ⑥: canonical pin order (`pin_id_cmp` — numeric ids
            // numerically, the rest naturally), not dictionary order. The sort
            // exists for determinism; the canonical comparator is the one
            // definition every pin listing uses, so the registration (and with
            // it the entry-id allocation order) follows it too.
            let mut pin_names: Vec<&String> = comp.pins.keys().collect();
            pin_names.sort_by(|a, b| crate::instant::mc_comp::pin_id_cmp(a, b));
            for pin_name in pin_names {
                if let Some(net_point) = comp.pins.get(pin_name) {
                    let pin_path = format!("{comp_path}.{pin_name}");
                    let pin_func_name = comp
                        .cond_pin_names
                        .get(pin_name)
                        .and_then(|names| names.first())
                        .or_else(|| {
                            comp.def
                                .pins
                                .pin_id_to_names
                                .get(pin_name)
                                .and_then(|names| names.first())
                        })
                        .cloned()
                        .unwrap_or_default();
                    let pin_id = self.register(
                        pin_path,
                        InstKind::Pin,
                        Some(comp_id),
                        pin_func_name.clone(),
                        net_point.iotype.clone(),
                        net_point.src_pos.clone(),
                        inst.def_uri.to_string(),
                    );
                    // ★ Stage-readout §2.1: the pin's stage-comparable key.
                    // Resolved by the net layer's single authority — the very
                    // call that gives the lane its `PointId` — so this segment
                    // and the net layer name the pin identically by
                    // construction, not by two agreeing implementations.
                    let point = crate::instant::lane::point_of_comp_pin(comp, pin_name);
                    self.set_point(pin_id, point);

                    // ★ U48: the per-pin NC marker of the declaration line
                    // (`CHIP d1 @ncpin(1,3)`), already resolved to pin ids at
                    // instantiation — `pin_name` here *is* the pin id.
                    if comp.nc_pins.contains(pin_name) {
                        self.mark_nc(pin_id);
                    }

                    // ★ PWR-6: the pin row's own `@exposed` words ride the flat
                    // pin entry — the second host of a declared transient
                    // boundary (exposed-protection-design.md §2/§8.4).
                    let exposed = exposed_of_pin(comp, pin_name);
                    if !exposed.is_empty() {
                        self.set_exposed(pin_id, exposed);
                    }

                    // ★ pin-expectation v0.3: the row's `@role`/`@class` words
                    // ride the flat pin entry the same way, so the ERC gate
                    // reads the instance's materialized row — a
                    // conditional-branch variant and an adoption row (D3
                    // member defaults) included (§4.1).
                    let (exp_role, exp_class) = expectations_of_pin(comp, pin_name);
                    if !exp_role.is_empty() || !exp_class.is_empty() {
                        self.set_expectations(pin_id, exp_role, exp_class);
                    }

                    // ★ U201 ①②: the pin's interface-adoption face rides the
                    // flat entry the same way, so the exclusive-peer gate
                    // reads the instance's materialized adoption row — the
                    // flatten-time twin of the connection-time endpoint
                    // resolver, not a second opinion.
                    if let Some(lane) = iface_lane_of_pin(comp, pin_name) {
                        self.set_iface_lane(pin_id, lane);
                    }

                    // ★ U217: the pin's AC mains face rides the flat entry the
                    // same way (nominal only — a component row states no
                    // positional face).
                    if let Some(carry) = ac_face_carry_of_pin(comp, pin_name) {
                        self.set_ac_face(pin_id, carry);
                    }

                    // ── Declaration position for pins ──
                    // An unconnected pin never appears in a net, so `flatten_nets`
                    // can't back-fill a wiring site into `src_pos`. Anchor the
                    // pin's diagnostics at its declaration instead: the pin-id
                    // span in the component body (`io [12,13] = UART1...`).
                    // Recorded even when wiring sites exist — the states are
                    // parallel, and the reader picks (see `via`).
                    if let Some(entry) = self.entries.get_mut(&pin_id) {
                        if entry.fallback_pos.is_none() {
                            if let Some(r) = comp.def.pins.pin_id_spans.get(pin_name) {
                                entry.fallback_pos = Some(crate::semantic::common::SourcePos::new(
                                    comp.def.uri.clone(),
                                    r.start as u32,
                                ));
                            }
                        }
                    }

                    // Set member_info role (Ground/Power) — consumed by the viz
                    // projection layer for rail classification, not for net merging.
                    let (role, _inferred) = infer_member_role(
                        &pin_func_name,
                        &net_point.iotype,
                        &is_comp_ground,
                        &is_comp_power,
                    );
                    let info =
                        (!matches!(role, MemberRole::Signal)).then(|| MemberInfo::new(role, None));
                    if let Some(info) = &info {
                        self.set_member_info(pin_id, info.clone());
                    }
                    // ★ Typed direction carry: the contract row that owns this
                    // pin carries its `psrc`/`psnk`/`psbi` energy direction —
                    // which the flat `io_type` (always `IOType::Power`) cannot
                    // express. The row's `hot` is the member verbatim (dotted
                    // for a named group `= VOUT{Vout, GND}`, bare for an
                    // anonymous pair), so it matches either the pin table key
                    // (`comp.pins`, the wiring spelling) or the physical
                    // function name.
                    let dir = pwr_row_for_pin(comp, pin_name).map(|row| row.dir);
                    if let Some(dir) = dir {
                        self.set_pwr_dir(pin_id, dir);
                    }
                    // U266 ②: instance-bound nominal — a `::DC(formal)` row
                    // speaks with the instance's argument, not the bare name.
                    if let Some(nom) = pwr_nominal_for_pin(comp, pin_name) {
                        self.set_pwr_nom(pin_id, nom);
                    }
                    // Record the declared contract under every member spelling
                    // the net table may use for this pin (`ldo33{VOUT}` →
                    // `ldo33.VOUT.Vout`), so `flatten_nets` folds that spelling
                    // onto the declaration instead of manufacturing a
                    // semantics-less on-the-fly pin.
                    self.record_member_pin_sem(
                        comp,
                        pin_name,
                        &comp_path,
                        net_point.iotype.clone(),
                        info,
                        dir,
                    );
                }
            }
        }

        // ★ P8-1 pass 2: func-created components — re-parent to the component
        // that defines the func, not the calling module.
        let comps: Vec<_> = view.components(inst).collect();
        for comp in comps {
            if !matches!(comp.origin, InstOrigin::FuncCall { .. }) {
                continue;
            }
            // Per-comp connection-point DC enrichment — same rule as pass 1
            // (a func-created instance's own def rarely carries pwr rows, so
            // this usually degenerates to the module-scope identity).
            let comp_ground_coppers: Vec<String> = comp
                .def
                .pins
                .pwr
                .iter()
                .filter_map(|p| p.ret.clone())
                .collect();
            let comp_power_members: Vec<String> =
                comp.def.pins.pwr.iter().map(|p| p.hot.clone()).collect();
            let is_comp_ground =
                |m: &str| is_declared_ground(m) || comp_ground_coppers.iter().any(|r| r == m);
            let is_comp_power =
                |m: &str| is_declared_power(m) || comp_power_members.iter().any(|p| p == m);
            let fn_name = match &comp.origin {
                InstOrigin::FuncCall { fn_name, .. } => fn_name.clone(),
                _ => continue,
            };

            let (comp_path, comp_parent_id) = match func_to_owner.get(&fn_name) {
                Some(owner_name) => {
                    let owner_path = format!("{my_path}.{owner_name}");
                    if let Some(owner_id) = self.get_id_by_path(&owner_path) {
                        let path = format!("{owner_path}.{}", comp.name);
                        (path, Some(owner_id))
                    } else {
                        // Owner not found (should not happen), fall back to module parent
                        (format!("{my_path}.{}", comp.name), Some(my_id))
                    }
                }
                None => {
                    // Func owner not in this module (e.g., builtin func), fall back
                    (format!("{my_path}.{}", comp.name), Some(my_id))
                }
            };

            let comp_id = self.register(
                comp_path.clone(),
                InstKind::Component,
                comp_parent_id,
                comp.def.name.to_string(),
                IOType::None,
                None,
                inst.def_uri.to_string(),
            );
            self.set_identity(
                comp_id,
                comp.node_id,
                Some(McSpaceName::new(&comp.def.name, comp.def.uri.clone())),
            );
            // ★ Structural pin count (same as pass-1): pins registered below.
            self.set_pin_count(comp_id, comp.pins.len());

            // ★ Declaration site (same rationale as pass-1): anchor net
            // diagnostics for a never-wired func-created instance at the
            // caller's declaration rather than offset 0 → file:1:1. Recorded
            // in parallel with any wiring site, not in place of one.
            if let Some(span) = inst.def.insts.get_port_span(&comp.name) {
                if let Some(entry) = self.entries.get_mut(&comp_id) {
                    if entry.fallback_pos.is_none() {
                        entry.fallback_pos = Some(crate::semantic::common::SourcePos::new(
                            inst.def_uri.clone(),
                            span.start as u32,
                        ));
                    }
                }
            }

            if comp.nc {
                if let Some(entry) = self.entries.get_mut(&comp_id) {
                    entry.not_fitted = true;
                }
            }
            // ★ abstract-variant §6.1/§3.2: unselected marker (same as pass-1)
            if comp.def.is_abstract {
                if let Some(entry) = self.entries.get_mut(&comp_id) {
                    entry.unselected = true;
                }
            }
            // ★ PWR-5: protection declaration (same as pass-1)
            if let Some(kind) = protection_of(comp) {
                if let Some(entry) = self.entries.get_mut(&comp_id) {
                    entry.protection = Some(kind);
                }
            }
            // ★ PI: the element class the class's own `spec` table declares,
            // carried the same way (a def-marker, not an instance property).
            if let Some(class) = element_class_of(comp) {
                if let Some(entry) = self.entries.get_mut(&comp_id) {
                    entry.element_class = Some(class);
                }
            }
            // ★ PWR-4b: the two decoded quantities (same as pass-1)
            let resistance = spec_quantity_of(comp, "spec.resistance", &McUnit::Ohm);
            let power_rated = spec_quantity_of(comp, "spec.power_rated", &McUnit::Wat);
            if resistance.is_some() || power_rated.is_some() {
                if let Some(entry) = self.entries.get_mut(&comp_id) {
                    entry.resistance_ohm = resistance;
                    entry.power_rated_w = power_rated;
                }
            }
            if let Some(entry) = self.entries.get_mut(&comp_id) {
                entry.origin = comp.origin.clone();
            }

            if inst.bridge_passive_names.contains(&comp.name) {
                self.bridge_passive_paths.insert(comp_path.clone());
            }

            // U152 ⑥: canonical pin order, see the twin site above.
            let mut pin_names: Vec<&String> = comp.pins.keys().collect();
            pin_names.sort_by(|a, b| crate::instant::mc_comp::pin_id_cmp(a, b));
            for pin_name in pin_names {
                if let Some(net_point) = comp.pins.get(pin_name) {
                    let pin_path = format!("{comp_path}.{pin_name}");
                    let pin_func_name = comp
                        .cond_pin_names
                        .get(pin_name)
                        .and_then(|names| names.first())
                        .or_else(|| {
                            comp.def
                                .pins
                                .pin_id_to_names
                                .get(pin_name)
                                .and_then(|names| names.first())
                        })
                        .cloned()
                        .unwrap_or_default();
                    let pin_id = self.register(
                        pin_path,
                        InstKind::Pin,
                        Some(comp_id),
                        pin_func_name.clone(),
                        net_point.iotype.clone(),
                        net_point.src_pos.clone(),
                        inst.def_uri.to_string(),
                    );
                    // ★ Stage-readout §2.1: the pin's stage-comparable key.
                    // Resolved by the net layer's single authority — the very
                    // call that gives the lane its `PointId` — so this segment
                    // and the net layer name the pin identically by
                    // construction, not by two agreeing implementations.
                    let point = crate::instant::lane::point_of_comp_pin(comp, pin_name);
                    self.set_point(pin_id, point);

                    // ★ U48: per-pin NC marker (same lookup as pass 1).
                    if comp.nc_pins.contains(pin_name) {
                        self.mark_nc(pin_id);
                    }

                    // ★ PWR-6: pin-row `@exposed` carry (same as pass 1).
                    let exposed = exposed_of_pin(comp, pin_name);
                    if !exposed.is_empty() {
                        self.set_exposed(pin_id, exposed);
                    }

                    // ★ pin-expectation v0.3: `@role`/`@class` carry
                    // (same rule as pass 1).
                    let (exp_role, exp_class) = expectations_of_pin(comp, pin_name);
                    if !exp_role.is_empty() || !exp_class.is_empty() {
                        self.set_expectations(pin_id, exp_role, exp_class);
                    }

                    // ★ U201 ①②: the pin's interface-adoption face rides the
                    // flat entry the same way, so the exclusive-peer gate
                    // reads the instance's materialized adoption row — the
                    // flatten-time twin of the connection-time endpoint
                    // resolver, not a second opinion.
                    if let Some(lane) = iface_lane_of_pin(comp, pin_name) {
                        self.set_iface_lane(pin_id, lane);
                    }

                    // ★ U217: the pin's AC mains face rides the flat entry the
                    // same way (nominal only — a component row states no
                    // positional face).
                    if let Some(carry) = ac_face_carry_of_pin(comp, pin_name) {
                        self.set_ac_face(pin_id, carry);
                    }

                    let (role, _inferred) = infer_member_role(
                        &pin_func_name,
                        &net_point.iotype,
                        &is_comp_ground,
                        &is_comp_power,
                    );
                    let info =
                        (!matches!(role, MemberRole::Signal)).then(|| MemberInfo::new(role, None));
                    if let Some(info) = &info {
                        self.set_member_info(pin_id, info.clone());
                    }
                    // ★ Typed direction carry (same rule as pass 1).
                    let dir = pwr_row_for_pin(comp, pin_name).map(|row| row.dir);
                    if let Some(dir) = dir {
                        self.set_pwr_dir(pin_id, dir);
                    }
                    // U266 ②: instance-bound nominal — a `::DC(formal)` row
                    // speaks with the instance's argument, not the bare name.
                    if let Some(nom) = pwr_nominal_for_pin(comp, pin_name) {
                        self.set_pwr_nom(pin_id, nom);
                    }
                    // Record the declared contract under every member spelling
                    // the net table may use for this pin (`ldo33{VOUT}` →
                    // `ldo33.VOUT.Vout`), so `flatten_nets` folds that spelling
                    // onto the declaration instead of manufacturing a
                    // semantics-less on-the-fly pin.
                    self.record_member_pin_sem(
                        comp,
                        pin_name,
                        &comp_path,
                        net_point.iotype.clone(),
                        info,
                        dir,
                    );
                }
            }
        }

        // 4. Register bus + bus members (Step 6: bus member expansion)
        //    Bus paths use `.` separator: main.power
        //    Bus member paths use `/` separator: main.power/VCC
        // [P0-DET] iterate buses in sorted name order: `register` allocates ids by
        // call order, so HashMap iteration order would leak into entry/pin ids.
        //
        // Phase E: labels/buses come from the module's frozen overlay fragment
        // in the store (keyed by `my_path`) — `McModuleInst` no longer carries
        // them. The fragment is cloned out first so the store borrow ends
        // before the `&mut self` `register` calls below.
        let (labels, buses) = {
            let store = self.net_table.borrow();
            let labels: HashMap<String, NetPoint> = store.labels_of(&my_path).clone();
            let buses: HashMap<String, McBusInst> = store.buses_of(&my_path).clone();
            (labels, buses)
        };
        let mut bus_names: Vec<&String> = buses.keys().collect();
        bus_names.sort();
        for bus_name in bus_names {
            let bus_inst = &buses[bus_name];
            let bus_path = format!("{my_path}.{bus_name}");

            // Bug ② defense
            // `inst.buses` theoretically only contains real buses, but if the
            // upstream (points.rs's ensure_bus) mistakenly collects some
            // component/sub-module instance name as a bus, here it would expand
            // component pins into `<comp>/<pid>` form Labels. Step 3 has already
            // registered components/sub-modules with `.` as Component/Module;
            // if bus_path hits either of these two kinds, skip the whole bus.
            if let Some(existing_id) = self.get_id_by_path(&bus_path) {
                if let Some(e) = self.get_entry(existing_id) {
                    if matches!(e.kind, InstKind::Component | InstKind::Module) {
                        continue;
                    }
                }
            }

            // ── Fix: inherit IO type from Port if this bus is a port declaration ──
            // Bus ports like `rs485{A,B}` have IO type InOut, but their members
            // were registered with IOType::None, causing them to be misidentified
            // as power labels in viz rendering.
            let bus_io = self
                .get_id_by_path(&bus_path)
                .and_then(|id| self.get_entry(id))
                .map(|e| e.io_type.clone())
                .unwrap_or(IOType::None);

            let bus_id = self.register(
                bus_path.clone(),
                InstKind::Bus,
                Some(my_id),
                String::new(),
                bus_io.clone(),
                None,
                inst.def_uri.to_string(),
            );

            // Declared power face of a module-scope bus — same rule the label loop
            // below applies: this module's own def space answers for its own name.
            if let Some(pi) = self.power_decls.get(&my_id) {
                if let Some(member) = pwrid::member_of_module(pi, bus_name) {
                    self.set_pwr_member(bus_id, member);
                }
            }

            // Expand bus members with the inherited IO type
            let member_decl_span = Self::port_decl_span_of(inst, bus_name);
            for member in &bus_inst.members {
                let member_path = format!("{bus_path}/{member}");
                let member_id = self.register(
                    member_path,
                    InstKind::Label,
                    Some(bus_id),
                    String::new(),
                    bus_io.clone(),
                    None,
                    inst.def_uri.to_string(),
                );
                // Unconnected bus members (E4117) have no wiring site; anchor
                // them at the owning port/bus declaration span instead of
                // file:1:1 (same fallback as the port loop above).
                self.backfill_port_decl_pos(member_id, &inst.def_uri, member_decl_span.clone());

                // ★ A′: when this `/` lane is the slash spelling of a member that
                // was also registered as a dotted member *Port* (port-derived bus,
                // e.g. `MIC{P,N}` → `main.MIC.MIC.P`, or `dc{VDD}` → `main.dc.VDD`),
                // fold the lane onto that Port — same physical conductor, one id.
                // A real bus whose members have no dotted Port (only `/` lanes) is
                // untouched and stays electrical.
                let dotted = format!("{bus_path}.{member}");
                if let Some(target_id) = self.get_id_by_path(&dotted) {
                    self.mark_alias(member_id, target_id);
                }
            }
        }

        // 5. Register standalone labels (avoid duplication with ports/buses)
        // [P0-DET] sorted name order: `register` allocates ids by call order.
        // Port buses also inject bare member labels (e.g. `io MIC{P,N}` yields
        // plain `P` / `N` labels) that carry no wiring site; map each member
        // back to its owning port's declaration span so E4117 anchors there.
        let mut member_to_port_span: HashMap<&str, Option<Range<usize>>> = HashMap::new();
        for port in &inst.ports {
            let span = Self::port_decl_span_of(inst, &port.name);
            for member in &port.bus_members {
                member_to_port_span
                    .entry(member.as_str())
                    .or_insert_with(|| span.clone());
            }
        }
        // ★ A′: map a bare member name (`P` from `MIC{P,N}`) to the dotted member
        // Port path registered in step 2, using the *same* member-path rules so
        // the two always agree. Bracket members register flat under `{my_path}`,
        // which equals the bare-label path — those are skipped by the
        // `get_id_by_path` guard below and never reach aliasing here.
        let mut member_to_dotted: HashMap<&str, String> = HashMap::new();
        for port in &inst.ports {
            for member in &port.bus_members {
                let dotted = if port.name.contains('[') {
                    format!("{}.{}", my_path, member)
                } else if port.name.contains('{') {
                    let base = port.name.split('{').next().unwrap_or("");
                    if base.is_empty() {
                        format!("{}.{}", my_path, member)
                    } else {
                        format!("{}.{}.{}", my_path, base, member)
                    }
                } else {
                    format!("{}.{}.{}", my_path, port.name, member)
                };
                member_to_dotted.entry(member.as_str()).or_insert(dotted);
            }
        }
        let mut label_names: Vec<&String> = labels.keys().collect();
        label_names.sort();
        for label_name in label_names {
            let net_point = &labels[label_name];
            let label_path = format!("{my_path}.{label_name}");
            if self.get_id_by_path(&label_path).is_none() {
                let label_id = self.register(
                    label_path,
                    InstKind::Label,
                    Some(my_id),
                    String::new(),
                    net_point.iotype.clone(),
                    net_point.src_pos.clone(),
                    inst.def_uri.to_string(),
                );
                self.backfill_port_decl_pos(
                    label_id,
                    &inst.def_uri,
                    member_to_port_span
                        .get(label_name.as_str())
                        .cloned()
                        .flatten(),
                );

                // Declared power face of a module-scope net: only this module's
                // own declarations count, so a bare `GND` is a return here
                // exactly when *this* scope declared it one. An undeclared
                // label keeps `None` — it carries no promise, whatever it is
                // spelled.
                if let Some(pi) = self.power_decls.get(&my_id) {
                    if let Some(member) = pwrid::member_of_module(pi, label_name) {
                        self.set_pwr_member(label_id, member);
                    }
                }

                // ★ A′: a bare member label that carries an electrical direction
                // *and* whose owning port member was registered as a dotted member
                // Port is a non-physical alias of that Port (lane.rs — bare member
                // labels are not physical points). De-electrify and fold it onto
                // the member. A bare label with no dotted member (scalar body rail)
                // or with no direction stays untouched and remains electrical.
                let alias = member_to_dotted
                    .get(label_name.as_str())
                    .and_then(|d| self.get_id_by_path(d))
                    .filter(|_| !matches!(net_point.iotype, IOType::None));
                if let Some(target_id) = alias {
                    self.mark_alias(label_id, target_id);
                }
            }
        }

        // 6. Recursively process sub-modules. Phase C S3: the view's arena
        //    `children` edges drive the traversal order (design §4 — the tree
        //    is a view over arena edges); the aligned tree node supplies the
        //    sub-module data from the store.
        for sub in view.sub_modules(inst) {
            self.flatten_module(sub, &my_path, Some(my_id), view);
        }

        // 7. Register network information (module's frozen string net table).
        //    Pass `my_id` so every NetEntry records the module scope that owns
        //    it (net-island attribution L1).
        self.flatten_nets(inst, &my_path, my_id);
    }

    /// Flatten the module instance's net table into NetEntry records
    ///
    /// Traverse the module's frozen string net table (Phase D — sourced from
    /// the circuit-wide store, never from the tree), add the module prefix to
    /// each `NetPoint.path` and map to the registered `InstEntry.id`.
    ///
    /// ## Path resolution — three-level fallback + bracket expansion
    ///
    /// Maintains **exactly the same** behavior as
    /// `crate::vector::mc_vec_builder::McVecBuilder::resolve_netpoint`,
    /// avoiding the two pipelines (flatten_nets and mc_vec_builder) giving
    /// different resolution results for the same `NetPoint.path`.
    ///
    /// In the past, `flatten_nets` only tried two candidates: `module_path.path`
    /// and `path`, causing all points in the "bus member" form (e.g. `mic.MIC.P`
    /// needing to hit `main.mic.MIC/P`) to be silently lost here, making
    /// `InstTable.nets` have far fewer points than `McVecBlock.nets`, which in
    /// turn caused the layer to see fewer top-level edges (root cause 2).
    ///
    /// See `resolve_netpoint_path` comment for details.
    fn flatten_nets(&mut self, _inst: &McModuleInst, module_path: &str, module_id: u32) {
        // [P0-DET] sorted net-name order: `net_id_counter` is allocated by iteration
        // order, so HashMap order would leak into net ids (and downstream pin ids).
        // Each module's table is a Vec (a module may hold multiple nets all named
        // "GND"); it is pre-sorted deterministically by build_net_table, so just
        // iterate in order.
        let net_entries = self
            .net_table
            .borrow()
            .get(module_path)
            .map(|t| t.to_vec())
            .unwrap_or_default();

        for (net_name, net_points) in net_entries {
            let mut point_ids: Vec<u32> = Vec::new();

            for np in &net_points {
                let mut ids = self.resolve_netpoint_path(&np.path, module_path);
                // ── P2-2: register boundary connection pins on the fly ──
                // When a boundary connection creates a pin like mcu.10, it's not
                // registered as a port or component pin in the InstTable. Register it
                // as a Pin entry under the owner submodule so flatten_nets can resolve it.
                //
                // Only reached when the spelling names **no** entry of its own: a
                // bare member key that does answer to a declared member Port of
                // the same owner folds onto that Port instead (below), and the
                // minted entry is a last resort — it carries no member semantics
                // and no direction.
                if ids.is_empty() && np.owner.is_some() {
                    if let Some(owner_name) = &np.owner {
                        let full_path = format!("{module_path}.{}", np.path);
                        let owner_full = format!("{module_path}.{owner_name}");
                        if let Some(parent_id) = self.get_id_by_path(&owner_full) {
                            // ★ CIMP §1 U97 — try the declared member first. A
                            // bare member spelling (`MCU513.8`) and the declared
                            // member Port it names (`MCU513.SPI.8`) are one
                            // conductor; folding the point onto the declared
                            // member keeps one identity per conductor and lets
                            // the declared Port read as wired. Minting here
                            // instead is what made 4 of hbl's 11 E4114 rows
                            // false positives (see
                            // `log/9.18.bundle-port-readout.md` §4).
                            if let Some(member_id) =
                                self.declared_member_port_of(parent_id, &np.path, owner_name)
                            {
                                ids.push(member_id);
                            } else {
                                // A wiring may name a component power pin by its
                                // group member (`ldo33{VOUT}` → `ldo33.VOUT.Vout`)
                                // while the pin itself is registered under its
                                // declaration key. Inherit the contract recorded at
                                // the component flatten site so the endpoint keeps
                                // its io / role / `psrc|psnk|psbi` direction rather
                                // than reading as an anonymous pin.
                                let carried = self.member_pin_sem.get(&full_path).cloned();
                                let io = carried
                                    .as_ref()
                                    .map(|c| c.0.clone())
                                    .unwrap_or_else(|| np.iotype.clone());
                                let pin_id = self.register(
                                    full_path,
                                    InstKind::Pin,
                                    Some(parent_id),
                                    String::new(),
                                    io,
                                    np.src_pos.clone(),
                                    String::new(),
                                );
                                if let Some((_, info, dir, member)) = carried {
                                    if let Some(info) = info {
                                        self.set_member_info(pin_id, info);
                                    }
                                    if let Some(dir) = dir {
                                        self.set_pwr_dir(pin_id, dir);
                                    }
                                    if let Some(member) = member {
                                        self.set_pwr_member(pin_id, member);
                                    }
                                }
                                ids.push(pin_id);
                            }
                        }
                    }
                }
                for id in ids {
                    // ★ Copy the net point's wiring sites into the entry, so
                    // net-level diagnostics (E4103 undriven-net, driver-conflict,
                    // voltage-mismatch, …) resolve to the wiring site instead of
                    // offset 0 → file:1:1.
                    //
                    // No file filter: a set has no winner to pick, so there is
                    // nothing for one to choose between. Over-inclusion is
                    // bounded — the loop only runs over the ids this one net
                    // point resolved to, so an entry never inherits a position
                    // from a point it is not part of.
                    if let Some(entry) = self.entries.get_mut(&id) {
                        entry.src_pos.extend_from(&np.src_pos);
                    }
                    point_ids.push(id);
                }
            }

            // ★ A′: after alias folding, several spellings of one conductor can
            // resolve to the same physical member id within one net statement
            // (e.g. a body writes both `dc.VDD` and `VDD`). Dedupe so each id
            // appears once per NetEntry, preserving first-seen order.
            let mut seen: HashSet<u32> = HashSet::new();
            point_ids.retain(|&pid| seen.insert(pid));

            // At least 2 endpoints are needed to constitute a meaningful net
            if point_ids.len() >= 2 {
                let net_id = self.net_id_counter;
                self.net_id_counter += 1;

                // Build reverse index (1 point → many net segments: a boundary
                // junction id lands on both the child internal net and the
                // parent net — A′ 3).
                for &pid in &point_ids {
                    self.point_to_net.entry(pid).or_default().push(net_id);
                }

                self.nets.insert(
                    net_id,
                    NetEntry {
                        id: net_id,
                        name: net_name.clone(),
                        module: Some(module_id),
                        points: point_ids,
                    },
                );
            } else if point_ids.is_empty() && !net_points.is_empty() {
                // §11.4 GAP2: net statement materialized 0 physical pins
                // Every NetPoint of this module-level net failed to resolve to a
                // registered physical entry (component pin / port). The whole
                // statement produced no connection at all — its endpoints are
                // unresolved structured ghosts (bases declared nowhere) or paths
                // no entity registered. This is the global half of E4057
                // (NET_DROPPED_STATEMENT): the local NAME[k] alias site in
                // mc_phrase.rs catches the indexed-alias shape at pass1; here the
                // materialization fact — 0 pins from a live net — is checked on
                // the flattened table, once per dropped net, at the first
                // point's wiring site. A net that kept ≥1 physical point is a
                // stub, not 0-pin, and stays quiet here (the ghost reference is
                // E3137's pass1 domain; the orphaned pin is the 41xx unconnected
                // checks' domain) — the domains do not double-report.
                let src_pos = net_points.iter().find_map(|np| np.src_pos.first().cloned());
                let paths: Vec<String> = net_points.iter().map(|np| np.path.clone()).collect();
                let msg = crate::errcodes::format_msg(
                    crate::errcodes::NET_DROPPED_STATEMENT,
                    &[
                        &net_name.clone(),
                        &format!(
                            "materialized no physical pins (endpoints [{}] all failed to resolve)",
                            paths.join(", ")
                        ),
                    ],
                );
                match &src_pos {
                    Some(sp) => crate::db::diagnostic::diagnostic::diagnostic_log_at(
                        crate::errcodes::NET_DROPPED_STATEMENT,
                        crate::db::diagnostic::diagnostic::DiagnosticLevel::Error,
                        sp.uri.clone(),
                        sp.offset,
                        0,
                        &msg,
                        &[],
                    ),
                    None => crate::db::diagnostic::diagnostic::diagnostic_log(
                        crate::errcodes::NET_DROPPED_STATEMENT,
                        crate::db::diagnostic::diagnostic::DiagnosticLevel::Error,
                        0,
                        0,
                        &msg,
                        &[],
                    ),
                }
            }
        }
    }

    /// Resolve a single NetPoint path to zero or more registered InstEntry IDs
    ///
    /// This method is the "single source of truth" shared by `flatten_nets` and
    /// `mc_vec_builder::resolve_netpoint` — both must produce the same ID set
    /// for the same `NetPoint.path`, otherwise the downstream (drawing layer)
    /// will see edges inconsistent with McVecBlock.
    ///
    /// ## Input forms
    /// - Plain single path: `R1.1`, `VCC`, `power.VCC`, `sub1.clk` → 0 or 1 ID
    /// - List form (bracket): `sub.[A, B, C]` → up to 3 IDs (resolve each after expansion)
    ///
    /// ## Resolution failure
    /// Single paths for which all fallbacks fail are silently discarded
    /// (return empty Vec). No warning is printed and no counter is incremented
    /// here; diagnosis is handled on the `mc_vec_builder` side;
    /// the flatten_nets side only cares about "connect what can be connected,
    /// skip what cannot".
    pub(crate) fn resolve_netpoint_path(&self, path: &str, module_path: &str) -> Vec<u32> {
        // ── (A) Bracket list expansion: `sub.[A, B]` → ["sub.A", "sub.B"] ──
        if let Some(expanded) = expand_bracket_list(path) {
            return expanded
                .iter()
                .filter_map(|p| self.resolve_single_path(p, module_path))
                .collect();
        }

        // ── (B) Plain single path: three-level fallback ──
        self.resolve_single_path(path, module_path)
            .into_iter()
            .collect()
    }

    /// Single path three-level fallback resolution (internal helper)
    ///
    /// Lookup order:
    /// 1. `module_path.path`        (most common: sub-module internal component pin/port)
    /// 2. `path`                    (top-level port direct reference)
    /// 3. Replace the trailing `.` with `/` and try (★ key: bus member, e.g. `power.VCC` →
    /// `power/VCC`)
    ///
    /// ## Why (3) is needed: heterogeneous path separators
    /// InstTable registration rules:
    /// - Component pin / module port / sub-module — joined by `.` (e.g. `main.mcu.uC.XTAL`)
    /// - Bus member — joined by `/` (e.g. `main.power/VCC`, see flatten_module step 4)
    ///
    /// And `NetPoint.path` is **always assembled with `.`** in the phrase parsing stage,
    /// so all points accessed via "bus member" (`bus.member` syntax) will miss
    /// in steps (1) and (2). Step (3) replaces the trailing separator with `/`
    /// and tries once more to hit them.
    ///
    /// Only replacing the **last** `.` is intentional — to avoid multiple
    /// ambiguous interpretations of `a.b.c` (`a.b/c` vs `a/b.c`), and consistent
    /// with the current single-level bus expansion semantic boundary.
    fn resolve_single_path(&self, path: &str, module_path: &str) -> Option<u32> {
        // Handle the edge case where `module_path` is the empty string
        // (current callers guarantee non-empty, defensive handling here)
        let full_path = if module_path.is_empty() {
            path.to_string()
        } else {
            format!("{module_path}.{path}")
        };

        // (1) Module prefix + path, most common
        if let Some(&id) = self.path_index.get(&full_path) {
            return Some(self.fold_alias(id));
        }
        // (2) Direct path lookup (top-level port/label)
        if let Some(&id) = self.path_index.get(path) {
            return Some(self.fold_alias(id));
        }
        // (3) ★ Replace last `.` with `/` — bus member fallback
        //     Example: main.power.VCC → main.power/VCC
        //              power.VCC      → power/VCC (if top-level is a bus)
        for candidate in [full_path.as_str(), path] {
            if let Some(pos) = candidate.rfind('.') {
                let bus_style = format!("{}/{}", &candidate[..pos], &candidate[pos + 1..]);
                if let Some(&id) = self.path_index.get(&bus_style) {
                    return Some(self.fold_alias(id));
                }
            }
        }
        // (4) ★ Member spelling of a declared component pin (`ldo{VIN | VOUT}` /
        //     `ldo.VIN.Vin` → the pin registered under its pin-table key). Tried
        //     last: a real entry at either candidate path always wins. Without
        //     this, `flatten_nets` manufactures a semantics-less on-the-fly pin
        //     for the spelling and the renderer draws it beside the real one.
        for candidate in [full_path.as_str(), path] {
            if let Some(id) = self.member_pin_alias.get(candidate) {
                return Some(self.fold_alias(*id));
            }
        }
        // ★ A′: every lookup path (dotted member, slash lane, bare member label,
        // aggregate) collapses onto the same physical member id via `alias_of`;
        // no fallback below may leak an un-folded alias spelling.
        None
    }

    // dump output (Step 8)

    /// Print the table (for debugging)
    pub fn dump(&self) {
        mcc_dbg!(
            "inst::table",
            "  {:<6} {:<40} {:<12} {:<16} IO",
            "ID",
            "Path",
            "Kind",
            "Class"
        );
        mcc_dbg!(
            "inst::table",
            "  {:<6} {:<40} {:<12} {:<16} ────",
            "──────",
            "────────────────────────────────────────",
            "────────────",
            "────────────────"
        );
        for entry in self.entries.values() {
            let io_str = match &entry.io_type {
                IOType::In => "in",
                IOType::Out => "out",
                IOType::InOut => "io",
                IOType::Power => "power",
                IOType::Return => "return",
                IOType::NonCon => "nc",
                IOType::None => "-",
            };
            let class_display = if entry.class_name.is_empty() {
                "-"
            } else {
                &entry.class_name
            };
            mcc_dbg!(
                "inst::table",
                "  {:<6} {:<40} {:<12} {:<16} {}",
                entry.id,
                entry.path,
                entry.kind,
                class_display,
                io_str
            );
        }
        mcc_dbg!(
            "inst::table",
            "  ── Total: {} entries ──",
            self.entries.len()
        );

        // Output network information
        if !self.nets.is_empty() {
            mcc_dbg!("inst::table", "");
            mcc_dbg!("inst::table", "  {:<8} {:<24} Points", "NetID", "Name");
            mcc_dbg!(
                "inst::table",
                "  {:<8} {:<24} ──────────────────────────────",
                "────────",
                "────────────────────────"
            );
            for net in self.nets.values() {
                let point_strs: Vec<String> = net
                    .points
                    .iter()
                    .map(|pid| {
                        self.entries
                            .get(pid)
                            .map(|e| format!("#{} ({})", pid, e.path))
                            .unwrap_or_else(|| format!("#{pid}"))
                    })
                    .collect();
                mcc_dbg!(
                    "inst::table",
                    "  {:<8} {:<24} [{}]",
                    net.id,
                    net.name,
                    point_strs.join(", ")
                );
            }
            mcc_dbg!("inst::table", "  ── Total: {} nets ──", self.nets.len());
        }
    }

    /// Collect all failed component records from the module tree and write to known_missing.md.
    ///
    /// Phase C S3-D: the tree walk resolves sub-modules through `view` (arena
    /// edges + store — the tree's Vec fields are gone).
    pub fn write_known_missing(inst: &McModuleInst, output_path: &str, view: &TreeView) {
        let mut all_records: Vec<&crate::instant::mc_mod::FailedRecord> = Vec::new();
        Self::collect_failed_records(inst, view, &mut all_records);

        if all_records.is_empty() {
            return;
        }

        let mut content = String::from("# Known Missing Components (G4 Baseline)\n\n");
        content.push_str(
            "Components that failed instantiation and were excluded from the netlist.\n\n",
        );
        content.push_str("| Module | Component | Class | Src Line | Reason |\n");
        content.push_str("|--------|-----------|-------|----------|--------|\n");

        for r in &all_records {
            let line = r
                .src_line
                .map(|l| l.to_string())
                .unwrap_or_else(|| "?".to_string());
            content.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                r.module, r.component_name, r.class_name, line, r.reason
            ));
        }

        content.push_str(&format!(
            "\nTotal: {} failed instantiation(s)\n",
            all_records.len()
        ));

        if let Some(parent) = std::path::Path::new(output_path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(output_path, &content) {
            mcc_dbg!(
                "inst::table",
                "[G4] Failed to write known_missing.md: {}",
                e
            );
        } else {
            mcc_dbg!(
                "inst::table",
                "[G4] known_missing.md written with {} entries",
                all_records.len()
            );
        }
    }

    fn collect_failed_records<'a>(
        inst: &'a McModuleInst,
        view: &'a TreeView<'a>,
        out: &mut Vec<&'a crate::instant::mc_mod::FailedRecord>,
    ) {
        for r in &inst.failed_records {
            out.push(r);
        }
        for sub in view.sub_modules(inst) {
            Self::collect_failed_records(sub, view, out);
        }
    }
}

// Helper: bracket list expansion (Iter 1 extension)

/// Try to split a path of the form `<prefix>.[<m1>, <m2>, ...]` into a list of
/// independent paths
///
/// Maintains **consistent behavior** with
/// `crate::vector::mc_vec_builder::McVecBuilder::expand_bracket_list`
/// (both sides share the same set of rules, avoiding drift).
///
/// - Returns `Some(vec!["<prefix>.<m1>", "<prefix>.<m2>", ...])`
/// - Non-match, malformed form (empty prefix / empty list / `]` not at the end),
///   or empty members all return `None`, and the caller treats it as a normal
///   single path
///
/// ## Design decisions
/// - Use `.[` as the only trigger identifier
/// - `]` must be at the end of the string; otherwise degrade to normal single
///   path processing (safer than accidental splitting)
/// - Members split by `,` and `trim`; empty members are filtered (tolerate `a.[X, ,Y]`)
/// - Nesting is not supported (`a.[X.[Y, Z], W]`)
fn expand_bracket_list(path: &str) -> Option<Vec<String>> {
    let open = path.find(".[")?;
    if !path.ends_with(']') {
        return None;
    }
    let close = path.len() - 1;
    // Defend against zero-length body like `prefix.[]` (close - (open + 2) < 1)
    if close <= open + 2 {
        return None;
    }
    let prefix = &path[..open];
    if prefix.is_empty() {
        return None;
    }
    let body = &path[open + 2..close];
    let members: Vec<String> = body
        .split(',')
        .map(|m| m.trim())
        .filter(|m| !m.is_empty())
        .map(|m| format!("{prefix}.{m}"))
        .collect();
    if members.is_empty() {
        None
    } else {
        Some(members)
    }
}

// Unit tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mat_insttab__register_and_lookup() {
        let mut table = InstTable::new(1000);
        let id = table.register_simple(
            "main".into(),
            InstKind::Module,
            None,
            "main".into(),
            IOType::None,
        );
        assert_eq!(id, 1000);
        assert_eq!(table.get_id_by_path("main"), Some(1000));
        assert!(table.get_entry(1000).is_some());
    }

    #[test]
    fn mat_insttab__no_duplicate_registration() {
        let mut table = InstTable::new(1000);
        let id1 = table.register_simple(
            "main".into(),
            InstKind::Module,
            None,
            "main".into(),
            IOType::None,
        );
        let id2 = table.register_simple(
            "main".into(),
            InstKind::Module,
            None,
            "main".into(),
            IOType::None,
        );
        assert_eq!(id1, id2);
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn mat_insttab__children_of() {
        let mut table = InstTable::new(1000);
        let parent = table.register_simple(
            "main".into(),
            InstKind::Module,
            None,
            "main".into(),
            IOType::None,
        );
        table.register_simple(
            "main.VCC".into(),
            InstKind::Port,
            Some(parent),
            String::new(),
            IOType::In,
        );
        table.register_simple(
            "main.GND".into(),
            InstKind::Port,
            Some(parent),
            String::new(),
            IOType::In,
        );
        table.register_simple(
            "main.R1".into(),
            InstKind::Component,
            Some(parent),
            "Res".into(),
            IOType::None,
        );

        let children = table.children_of(parent);
        assert_eq!(children.len(), 3);
    }

    #[test]
    fn mat_insttab__id_uniqueness() {
        let mut table = InstTable::new(1000);
        table.register_simple("a".into(), InstKind::Module, None, "A".into(), IOType::None);
        table.register_simple("b".into(), InstKind::Module, None, "B".into(), IOType::None);
        table.register_simple("c".into(), InstKind::Module, None, "C".into(), IOType::None);

        let ids: Vec<u32> = table.iter().map(|(id, _)| *id).collect();
        let unique: std::collections::HashSet<u32> = ids.iter().cloned().collect();
        assert_eq!(ids.len(), unique.len());
    }

    // Iter 1: Path resolution's three-level fallback + bracket expansion

    /// Bus member fallback: `power.VCC` should hit `main.power/VCC` already
    /// registered with `/`
    ///
    /// This is the most direct reproduction of root cause 2 — the original
    /// `flatten_nets` only tried `main.power.VCC` and `power.VCC`, both miss,
    /// causing the point to be silently lost in the flat netlist.
    #[test]
    fn mat_insttab__resolve_bus_member_path_fallback() {
        let mut table = InstTable::new(1000);
        let m = table.register_simple(
            "main".into(),
            InstKind::Module,
            None,
            String::new(),
            IOType::None,
        );
        let bus = table.register_simple(
            "main.power".into(),
            InstKind::Bus,
            Some(m),
            String::new(),
            IOType::None,
        );
        let vcc = table.register_simple(
            "main.power/VCC".into(),
            InstKind::Label,
            Some(bus),
            String::new(),
            IOType::None,
        );

        let ids = table.resolve_netpoint_path("power.VCC", "main");
        assert_eq!(
            ids,
            vec![vcc],
            "bus-member path should resolve via `/` fallback"
        );
    }

    /// Plain component pin path still hits from step (1), fallback does not
    /// change existing behavior
    #[test]
    fn mat_insttab__resolve_plain_dot_path_still_works() {
        let mut table = InstTable::new(1000);
        let m = table.register_simple(
            "main".into(),
            InstKind::Module,
            None,
            String::new(),
            IOType::None,
        );
        let comp = table.register_simple(
            "main.R1".into(),
            InstKind::Component,
            Some(m),
            String::new(),
            IOType::None,
        );
        let pin = table.register_simple(
            "main.R1.1".into(),
            InstKind::Pin,
            Some(comp),
            String::new(),
            IOType::None,
        );

        let ids = table.resolve_netpoint_path("R1.1", "main");
        assert_eq!(ids, vec![pin]);
    }

    /// Top-level port `VCC` (without prefix) should be hit by step (2)
    #[test]
    fn mat_insttab__resolve_top_level_port_no_prefix() {
        let mut table = InstTable::new(1000);
        let m = table.register_simple(
            "main".into(),
            InstKind::Module,
            None,
            String::new(),
            IOType::None,
        );
        let vcc = table.register_simple(
            "main.VCC".into(),
            InstKind::Port,
            Some(m),
            String::new(),
            IOType::None,
        );
        // Use "main.VCC" directly, go through step (1)
        let ids = table.resolve_netpoint_path("VCC", "main");
        assert_eq!(ids, vec![vcc]);
    }

    /// Bracket expansion: `sub.[A, B]` should be resolved into two independent IDs
    #[test]
    fn mat_insttab__resolve_bracket_list_expands() {
        let mut table = InstTable::new(1000);
        let m = table.register_simple(
            "main".into(),
            InstKind::Module,
            None,
            String::new(),
            IOType::None,
        );
        let sub = table.register_simple(
            "main.moddcdc".into(),
            InstKind::Module,
            Some(m),
            String::new(),
            IOType::None,
        );
        let a = table.register_simple(
            "main.moddcdc.VDD_3V3".into(),
            InstKind::Port,
            Some(sub),
            String::new(),
            IOType::None,
        );
        let b = table.register_simple(
            "main.moddcdc.GND".into(),
            InstKind::Port,
            Some(sub),
            String::new(),
            IOType::None,
        );

        let ids = table.resolve_netpoint_path("moddcdc.[VDD_3V3, GND]", "main");
        assert_eq!(ids, vec![a, b]);
    }

    /// Bracket partial hit: missed members are silently skipped, hit members
    /// retain original order
    #[test]
    fn mat_insttab__resolve_bracket_partial_miss() {
        let mut table = InstTable::new(1000);
        let m = table.register_simple(
            "main".into(),
            InstKind::Module,
            None,
            String::new(),
            IOType::None,
        );
        let sub = table.register_simple(
            "main.moddcdc".into(),
            InstKind::Module,
            Some(m),
            String::new(),
            IOType::None,
        );
        let a = table.register_simple(
            "main.moddcdc.VDD_3V3".into(),
            InstKind::Port,
            Some(sub),
            String::new(),
            IOType::None,
        );
        // GHOST deliberately not registered

        let ids = table.resolve_netpoint_path("moddcdc.[VDD_3V3, GHOST]", "main");
        assert_eq!(ids, vec![a]);
    }

    /// Unregistered path returns empty Vec (no panic, no polluting reverse index)
    #[test]
    fn mat_insttab__resolve_missing_path_returns_empty() {
        let mut table = InstTable::new(1000);
        table.register_simple(
            "main".into(),
            InstKind::Module,
            None,
            String::new(),
            IOType::None,
        );
        let ids = table.resolve_netpoint_path("ghost.signal", "main");
        assert!(ids.is_empty());
    }

    /// Syntax test cases for expand_bracket_list (kept in sync with mc_vec_builder side)
    #[test]
    fn mat_insttab__expand_bracket_list_syntax() {
        assert_eq!(
            expand_bracket_list("moddcdc.[VDD_3V3, GND]"),
            Some(vec!["moddcdc.VDD_3V3".into(), "moddcdc.GND".into()])
        );
        assert_eq!(
            expand_bracket_list("sub.[ A , B ]"),
            Some(vec!["sub.A".into(), "sub.B".into()])
        );
        assert_eq!(expand_bracket_list("sub.[X]"), Some(vec!["sub.X".into()]));
        // No match: no `.[`
        assert_eq!(expand_bracket_list("foo.bar"), None);
        // No match: `]` not at the end
        assert_eq!(expand_bracket_list("foo.[A, B].suffix"), None);
        // Malformed: empty body
        assert_eq!(expand_bracket_list("foo.[]"), None);
        // Malformed: empty prefix
        assert_eq!(expand_bracket_list(".[A, B]"), None);
        // Tolerate: extra commas between members
        assert_eq!(
            expand_bracket_list("foo.[A, , B]"),
            Some(vec!["foo.A".into(), "foo.B".into()])
        );
    }

    /// ★ PI carrier (`power-quality-design.md` §1.2): the element class a
    /// component's own definition declares is decoded once at flatten time and
    /// carried on the flat entry. The read is definition-space and key-driven —
    /// a class name or a pin shape never answers it (world-axioms §1 A1).
    #[test]
    fn mat_insttab__element_class_comes_from_the_definition_spec_table() {
        use crate::db::infra::init::MCC_TEST_PARSE_LOCK;
        const SRC: &str = r#"component PLAIN_R {
    pins = [ io [1:2] = [P, N] ]
}
component CAP_DECOUP {
    pins = [ io [1:2] = [P, N] ]
    spec = [
        capacitance = 1uF
    ]
}
component CAP_WITH_ESR {
    pins = [ io [1:2] = [P, N] ]
    spec = [
        capacitance = 1uF
        esr = 10mΩ
    ]
}
component ESR_ONLY {
    pins = [ io [1:2] = [P, N] ]
    spec = [
        esr = 10mΩ
    ]
}
component FB_FILTER {
    pins = [ io [1:2] = [P, N] ]
    spec = [
        impedance = 600Ω
    ]
}
component MAG_TWO_KEYS {
    pins = [ io [1:2] = [P, N] ]
    spec = [
        inductance = 1uH
        impedance = 600Ω
    ]
}
component CLASS_CLASH {
    pins = [ io [1:2] = [P, N] ]
    spec = [
        capacitance = 1uF
        resistance = 10k
    ]
}
component CAP_DOTTED {
    pins = [ io [1:2] = [P, N] ]
    spec.capacitance = 1uF
}
component OFF_SHELF {
    pins = [ io [1:2] = [P, N] ]
    name = "Off shelf"
}
module main {
    PLAIN_R r1
    CAP_DECOUP c1
    CAP_WITH_ESR c2
    ESR_ONLY e1
    FB_FILTER fb1
    MAG_TWO_KEYS m1
    CLASS_CLASH x1
    CAP_DOTTED c3
    OFF_SHELF off1
}
"#;
        let _guard = MCC_TEST_PARSE_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::mcc_init_no_lib();
        crate::mcc_set_system_root(std::path::Path::new(""));
        crate::mcc_clear_workspace();
        let uri: crate::McURI = "/mcc/element-class-lock.mc".to_string();
        crate::mcc_load_from_string(&uri, SRC);
        let (_tree, table) = crate::mcc_build_flat(&crate::McIds::from("main"), &uri, 1)
            .expect("element-class lock: flat build");
        let mut all: Vec<String> = table
            .iter()
            .map(|(_, e)| format!("{}={:?}", e.path, e.element_class))
            .collect();
        all.sort();
        let class_of = |path: &str| {
            table
                .iter()
                .find(|(_, e)| e.path == path)
                .unwrap_or_else(|| panic!("no flat entry `{path}` — table: {all:?}"))
                .1
                .element_class
        };
        // The declared class, once: the `spec` key that is the quantity.
        assert_eq!(
            class_of("main.c1"),
            Some(ElementClass::Capacitive),
            "{all:?}"
        );
        assert_eq!(
            class_of("main.fb1"),
            Some(ElementClass::Magnetic),
            "{all:?}"
        );
        // No spec table ⇒ no certificate ⇒ no class (silence, not a guess) —
        // whether the body declares nothing else at all or declares other keys
        // (`partno`/`package` on a bought-in part are not a classification).
        assert_eq!(class_of("main.r1"), None, "{all:?}");
        assert_eq!(class_of("main.off1"), None, "{all:?}");
        // A quantity that merely accompanies the element does not decide it.
        assert_eq!(
            class_of("main.c2"),
            Some(ElementClass::Capacitive),
            "{all:?}"
        );
        assert_eq!(class_of("main.e1"), None, "{all:?}");
        // Two keys of one class are one answer; two classes are no answer.
        assert_eq!(class_of("main.m1"), Some(ElementClass::Magnetic), "{all:?}");
        assert_eq!(class_of("main.x1"), None, "{all:?}");
        // Both spellings of a spec key are one fact (G2): `spec.capacitance`
        // is the same declaration as a `capacitance` row of the table.
        assert_eq!(
            class_of("main.c3"),
            Some(ElementClass::Capacitive),
            "{all:?}"
        );
    }
}
