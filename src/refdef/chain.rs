// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! §4.3 (Phase 2): structural member-chain resolution — single-container version.
//!
//! Given a member reference text like `MIC.P` or `MIC{P,N}` inside one
//! module/component container, walk the semantic instance table hop by hop and
//! return the precise definition the final segment lands on — instead of relying
//! only on full-name lookup (`lookup_declare_id("MIC.P")`).
//!
//! ## Scope of this version
//!
//! - Bus members (`MIC.P`, `MIC{P,N}`) → `BusMemberDef` with the precise span at
//!   the member declaration text, backed by [`McInstances::bus_def`].
//! - List members (`GPIO[1]`, `GPIO1`) → `LabelDef`.
//! - Whole references (`MIC`, `V3V3`, `GPIO[1:2]`) → the container def.
//! - Parameter declarations (`params`) → terminal `ParamDef` (no member walk).
//!
//! Component/module/interface instance hops (`uC.I2C0`, `uC.ADC{P,N}`) cross
//! into class definitions via `find_inst_with_span` (Phase 3).

use std::ops::Range;

use crate::query::lookup::ContainerRef;
use crate::refdef::types::{ChainSegment, SymbolKind};
use crate::semantic::basic::mc_paramd::McParamDeclares;
use crate::semantic::common::IOType;
use crate::semantic::mc_func::HasFindInst;
use crate::semantic::mc_inst::{McInstance, McInstances};
use crate::semantic::module::McModule;
use crate::semantic::scope as ns_scope;
use crate::McURI;

/// Definition target of a resolved member chain.
#[derive(Debug, Clone)]
pub struct ChainHit {
    /// Canonical name of the final target: `"MIC"` for the whole bus,
    /// `"MIC.P"` for a bus member.
    pub name: String,
    /// Def kind of the final target.
    pub def_kind: SymbolKind,
    /// Precise def span (byte range in `uri`).
    pub span: Range<usize>,
    /// File containing the definition.
    pub uri: McURI,
}

/// Split a member reference into hop segments.
///
/// `{}` / `[]` groups stay attached to their base segment:
///
/// ```text
/// "MIC.P"         → ["MIC", "P"]
/// "MIC{P,N}"      → ["MIC{P,N}"]
/// "uC.I2C0"       → ["uC", "I2C0"]
/// "uC.ADC{P,N}"   → ["uC", "ADC{P,N}"]
/// "GPIO[1:2]"     → ["GPIO[1:2]"]
/// "V3V3"          → ["V3V3"]
/// ```
pub fn split_segments(ref_text: &str) -> Vec<String> {
    let text = ref_text.trim();
    let bytes = text.as_bytes();
    let mut segs: Vec<String> = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'.' => {
                if i > start {
                    segs.push(text[start..i].to_string());
                }
                start = i + 1;
                i += 1;
            }
            b'{' | b'[' => {
                // Consume through the matching close brace/bracket, keeping the
                // group attached to its base segment.
                let open = bytes[i];
                let close = if open == b'{' { b'}' } else { b']' };
                let mut depth = 0usize;
                while i < bytes.len() {
                    if bytes[i] == open {
                        depth += 1;
                    } else if bytes[i] == close {
                        depth = depth.saturating_sub(1);
                        if depth == 0 {
                            i += 1;
                            break;
                        }
                    }
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    if start < bytes.len() {
        segs.push(text[start..].to_string());
    }
    segs
}

/// Base identifier of a segment — everything before the first `{` / `[`.
/// `"MIC{P,N}"` → `"MIC"`, `"ADC{P,N}"` → `"ADC"`, `"V3V3"` → `"V3V3"`.
fn base_of(seg: &str) -> &str {
    seg.split(['{', '[']).next().unwrap_or(seg).trim()
}

/// Members carried by a grouped segment, e.g. `"MIC{P,N}"` → (`"MIC"`, [`"P"`, `"N"`]).
/// `"GPIO[1:2]"` expands the slice → [`"1"`, `"2"`]. Returns `None` when the
/// segment has no `{}` / `[]` group or the group is empty.
fn group_members(seg: &str) -> Option<(String, Vec<String>)> {
    let base = base_of(seg);
    if base.is_empty() || base.len() == seg.len() {
        return None;
    }
    let inner = &seg[base.len()..];
    let is_group = (inner.starts_with('{') && inner.ends_with('}'))
        || (inner.starts_with('[') && inner.ends_with(']'));
    if !is_group {
        return None;
    }
    let content = &inner[1..inner.len() - 1];
    if content.is_empty() {
        return None;
    }
    let mut members = Vec::new();
    for part in content.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((a, b)) = part.split_once(':') {
            if let (Ok(from), Ok(to)) = (a.trim().parse::<i64>(), b.trim().parse::<i64>()) {
                if from <= to {
                    for i in from..=to {
                        members.push(i.to_string());
                    }
                    continue;
                }
            }
        }
        members.push(part.to_string());
    }
    if members.is_empty() {
        None
    } else {
        Some((base.to_string(), members))
    }
}

/// Mid-chain hop state.
enum Hop<'a> {
    /// A resolved instance (whole reference or mid-chain container).
    Inst {
        /// Canonical key in the instance table (e.g. `"MIC"`, `"GPIO[1:2]"`).
        key: String,
        inst: &'a McInstance,
    },
    /// A member of a named curly bus (`MIC.P`).
    BusMember { bus: String, member: String },
    /// A member of a list (`GPIO[1]` → member `"1"` of `"GPIO"`).
    ListMember { list: String, member: String },
    /// A terminal parameter declaration (no member walk in this version).
    Param(String),
    /// ★ Phase 3: a member resolved across a class boundary — e.g. `I2C0`
    /// inside `uC.I2C0` where `uC` is a component/module/interface instance.
    /// The semantic member is owned (a clone of what `find_inst_with_span`
    /// returned); `uri` is the class-definition file and `span` the precise
    /// def position inside it.
    CrossInst {
        /// Canonical full name of the member (e.g. `"uC.I2C0"`).
        key: String,
        inst: McInstance,
        /// Def kind of the member inside the class definition (e.g.
        /// `PinNameDef` for a component pin, `PortDef` for a module port).
        def_kind: SymbolKind,
        /// Def span inside the class-definition file.
        span: Option<Range<usize>>,
        /// Class-definition file that owns the member.
        uri: McURI,
    },
    /// ★ P3-P5 class-chain hit: the base name resolved to a
    /// component/module/interface/enum **class** definition via the unified
    /// [`ns_scope::first_hop`] exit (`BaseResolved::Container`, same-file
    /// CMIE → use-chain → system lib). Not a member axis — a following
    /// member segment ends the chain (the walk has no instance table to
    /// recurse into for a class name).
    Class(ContainerRef),
}

impl Hop<'_> {
    /// Human-readable hop description for `mcc::refdef` debug logs.
    fn desc(&self) -> String {
        match self {
            Hop::Inst { key, inst } => format!("Inst{{{key}: {}}}", inst.type_name()),
            Hop::BusMember { bus, member } => format!("BusMember{{{bus}.{member}}}"),
            Hop::ListMember { list, member } => format!("ListMember{{{list}[{member}]}}"),
            Hop::Param(name) => format!("Param{{{name}}}"),
            Hop::CrossInst {
                key,
                inst,
                span,
                uri,
                ..
            } => format!(
                "CrossInst{{{key}: {}{}{}}}",
                inst.type_name(),
                span.as_ref()
                    .map(|s| format!(" span={}..{}", s.start, s.end))
                    .unwrap_or_default(),
                format!(" uri={}", uri.as_str()),
            ),
            Hop::Class(c) => format!("Class{{{}}}", class_name(c)),
        }
    }
}

/// Display name of a [`ContainerRef`] (used in hop descriptions).
fn class_name(c: &ContainerRef) -> String {
    match c {
        ContainerRef::Component(comp) => comp.name.to_string(),
        ContainerRef::Module(m) => m.name.to_string(),
        ContainerRef::Interface(i) => i.name.to_string(),
        ContainerRef::Enum(e) => e.name.to_string(),
    }
}

/// Resolve the first hop for the initial segment.
///
/// Priority:
/// 1. Exact instance key (`MIC`, `GPIO[1:2]`, `V3V3`).
/// 2. Grouped segment — single-member group (`GPIO[1]`, `MIC{P}`) becomes a
///    member hop; multi-member group (`MIC{P,N}`, `GPIO[1:2]`) is a whole ref.
/// 3. Idx-aware whole-name resolution (`GPIO1` → list member `1`).
/// 4. Terminal parameter declaration.
/// 5. P3-P5 class chain via the unified `scope::first_hop` exit
///    (`BaseResolved::Container`): the base name resolves to a
///    component/module/interface/enum class definition (same-file CMIE →
///    use-chain → system library). Branches 1-4 above always win, so the
///    grouped/idx semantics are unchanged; the class chain only fills the
///    previously-empty miss path for bare class names.
fn first_hop<'a>(
    insts: &'a McInstances,
    params: &McParamDeclares,
    seg: &str,
    uri: &McURI,
) -> Option<Hop<'a>> {
    // 1. Exact key.
    if let Some(inst) = insts.get(seg) {
        mcc_dbg!(
            "refdef::chain",
            "[first_hop] exact key seg=\"{}\" → Inst{{{}}}",
            seg,
            inst.type_name()
        );
        return Some(Hop::Inst {
            key: seg.to_string(),
            inst,
        });
    }
    // 2. Grouped segment.
    if let Some((base, members)) = group_members(seg) {
        mcc_dbg!(
            "refdef::chain",
            "[first_hop] grouped seg=\"{}\" base=\"{}\" members={:?}",
            seg,
            base,
            members
        );
        if let Some((key, inst)) = lookup_base(insts, &base) {
            // Single-member group → member hop (e.g. `GPIO[1]`, `MIC{P}`).
            if members.len() == 1 {
                if let Some(m) = member_of(inst, &members[0]) {
                    mcc_dbg!(
                        "refdef::chain",
                        "[first_hop] single-member group → {}",
                        m.desc()
                    );
                    return Some(m);
                }
                mcc_dbg!(
                    "refdef::chain",
                    "[first_hop] single-member group member \"{}\" NOT in {} — fallthrough",
                    members[0],
                    inst.type_name()
                );
            }
            // Multi-member group → whole reference (e.g. `MIC{P,N}`).
            let hop = Hop::Inst { key, inst };
            mcc_dbg!(
                "refdef::chain",
                "[first_hop] multi-member group → {}",
                hop.desc()
            );
            return Some(hop);
        }
        mcc_dbg!(
            "refdef::chain",
            "[first_hop] grouped base MISS base=\"{}\" — fallthrough",
            base
        );
    }
    // 3. Idx-aware whole-name resolution (e.g. `GPIO1` → `GPIO[1:2]`).
    if let Some(key) = insts.resolve_idx(seg) {
        if let Some(inst) = insts.get(&key) {
            // `GPIO1` (digit form) → member `1` of the `GPIO[1:2]` list.
            if let McInstance::List(l) = inst {
                if let Some(member) = seg.strip_prefix(l.name.as_str()) {
                    if l.member.iter().any(|m| m == member) {
                        let hop = Hop::ListMember {
                            list: l.name.clone(),
                            member: member.to_string(),
                        };
                        mcc_dbg!(
                            "refdef::chain",
                            "[first_hop] idx digit form \"{}\" → {}",
                            seg,
                            hop.desc()
                        );
                        return Some(hop);
                    }
                }
            }
            let hop = Hop::Inst { key, inst };
            mcc_dbg!(
                "refdef::chain",
                "[first_hop] idx-aware \"{}\" → {}",
                seg,
                hop.desc()
            );
            return Some(hop);
        }
    }
    // 4. Terminal parameter declaration.
    let pname = base_of(seg);
    if params.is_defined(pname) {
        let hop = Hop::Param(pname.to_string());
        mcc_dbg!(
            "refdef::chain",
            "[first_hop] param term \"{}\" → {}",
            seg,
            hop.desc()
        );
        return Some(hop);
    }
    // 5. P3-P5 class chain — fills the miss path with class definitions
    //    (e.g. `CAP`, `U_MCU`). Only bare base names reach here: the
    //    grouped/idx branches above handled `MIC{P,N}` / `GPIO1` forms.
    //    `parent = None`: chain.rs resolves against its own insts/params
    //    tables (branches 1-4), so only the class-chain exit applies.
    if let Some(ns_scope::BaseResolved::Container(c)) = ns_scope::first_hop(pname, &[], None, uri) {
        let hop = Hop::Class(c);
        mcc_dbg!(
            "refdef::chain",
            "[first_hop] class-chain \"{}\" → {}",
            seg,
            hop.desc()
        );
        return Some(hop);
    }
    mcc_dbg!(
        "refdef::chain",
        "[first_hop] ALL MISS seg=\"{}\" pname=\"{}\" → None",
        seg,
        pname
    );
    None
}

/// Look up the instance for a base name — direct key, idx-aware forms, or a
/// list whose base name matches (lists store `GPIO[1:2]`, base is `GPIO`).
fn lookup_base<'a>(insts: &'a McInstances, base: &str) -> Option<(String, &'a McInstance)> {
    if let Some(inst) = insts.get(base) {
        mcc_dbg!(
            "refdef::chain",
            "[lookup_base] direct key base=\"{}\" → {}",
            base,
            inst.type_name()
        );
        return Some((base.to_string(), inst));
    }
    if let Some(key) = insts.resolve_idx(base) {
        if let Some(inst) = insts.get(&key) {
            mcc_dbg!(
                "refdef::chain",
                "[lookup_base] idx base=\"{}\" → key=\"{}\" {}",
                base,
                key,
                inst.type_name()
            );
            return Some((key, inst));
        }
    }
    let found = insts
        .insts()
        .iter()
        .find_map(|(key, (_, inst))| match inst {
            McInstance::List(l) if l.name == base => Some((key.clone(), inst)),
            _ => None,
        });
    if let Some((key, _inst)) = &found {
        mcc_dbg!(
            "refdef::chain",
            "[lookup_base] list-name scan base=\"{}\" → key=\"{}\"",
            base,
            key
        );
    } else {
        mcc_dbg!("refdef::chain", "[lookup_base] ALL MISS base=\"{}\"", base);
    }
    found
}

/// Resolve a member name within the current hop's container.
///
/// Local containers (Bus / List) resolve against their own member tables.
/// Component / Module / Interface instances cross into their class definition
/// (Phase 3): `find_inst_with_span` returns the semantic member plus the
/// precise def span in the class-definition file (`base.uri`).
///
/// Returns an owned hop (`'static`) — every returned variant clones the
/// member data, so the result never borrows from `inst`.
fn member_of(inst: &McInstance, member: &str) -> Option<Hop<'static>> {
    match inst {
        McInstance::Bus(b) => {
            let found =
                b.member.iter().any(|m| m == member) || b.full_members.iter().any(|m| m == member);
            mcc_dbg!(
                "refdef::chain",
                "[member_of] Bus \"{}\" member=\"{}\" found={}",
                b.name,
                member,
                found
            );
            if found {
                Some(Hop::BusMember {
                    bus: b.name.clone(),
                    member: member.to_string(),
                })
            } else {
                None
            }
        }
        McInstance::List(l) => {
            let found = l.member.iter().any(|m| m == member);
            mcc_dbg!(
                "refdef::chain",
                "[member_of] List \"{}\" member=\"{}\" found={}",
                l.name,
                member,
                found
            );
            if found {
                Some(Hop::ListMember {
                    list: l.name.clone(),
                    member: member.to_string(),
                })
            } else {
                None
            }
        }
        // ★ Phase 3: cross-container member resolution — the member's def
        // lives in the class-definition file (`base.uri`), lazily resolved
        // via `find_inst_with_span` (matches net-link semantics).
        McInstance::Component(c) => {
            let base = &c.base;
            match base.find_inst_with_span(member) {
                Some((m_inst, span)) => {
                    let kind = cross_def_kind(&m_inst);
                    mcc_dbg!("refdef::chain", "[member_of] Component \"{}\" member=\"{}\" → {} kind={:?} span={:?} uri={}", c.name, member, m_inst.type_name(), kind, span, base.uri.as_str());
                    Some(Hop::CrossInst {
                        key: format!("{}.{}", c.name, member),
                        inst: m_inst,
                        def_kind: kind,
                        span,
                        uri: base.uri.clone(),
                    })
                }
                None => {
                    mcc_dbg!(
                        "refdef::chain",
                        "[member_of] Component \"{}\" member=\"{}\" MISS → None",
                        c.name,
                        member
                    );
                    None
                }
            }
        }
        McInstance::Module(m) => {
            let base = &m.base;
            match base.find_inst_with_span(member) {
                Some((m_inst, span)) => {
                    // Module ports (`in DAC_OUT`) are stored as Label instances
                    // carrying a port-like IOType; `cross_def_kind` alone would
                    // misclassify them as PinNameDef. Resolve through the module
                    // insts table so ports land on PortDef (aligned with
                    // lapper_module_ports / iter_ports_with_span).
                    let kind = module_member_kind(base, member, &m_inst);
                    mcc_dbg!(
                        "refdef::chain",
                        "[member_of] Module \"{}\" member=\"{}\" → {} kind={:?} span={:?} uri={}",
                        m.name,
                        member,
                        m_inst.type_name(),
                        kind,
                        span,
                        base.uri.as_str()
                    );
                    Some(Hop::CrossInst {
                        key: format!("{}.{}", m.name, member),
                        inst: m_inst,
                        def_kind: kind,
                        span,
                        uri: base.uri.clone(),
                    })
                }
                None => {
                    mcc_dbg!(
                        "refdef::chain",
                        "[member_of] Module \"{}\" member=\"{}\" MISS → None",
                        m.name,
                        member
                    );
                    None
                }
            }
        }
        McInstance::Interface(i) => {
            // ★ Local curly members take precedence over the interface class
            // def: `io vin{POWER_SYS, GND}::DC(5V)` declares the member names
            // (POWER_SYS / GND) at the port site, so `vin.GND` resolves to the
            // member name text in THIS file (BusMemberDef via bus_def), not the
            // interface member in the class definition file (dc.mc).
            if let Some((busname, members)) = i.name.as_bus() {
                if members.iter().any(|m| m == member) {
                    mcc_dbg!(
                        "refdef::chain",
                        "[member_of] Interface \"{}\" member=\"{}\" → local BusMember{{{}.{}}}",
                        i.name,
                        member,
                        busname,
                        member
                    );
                    return Some(Hop::BusMember {
                        bus: busname,
                        member: member.to_string(),
                    });
                }
                mcc_dbg!(
                    "refdef::chain",
                    "[member_of] Interface \"{}\" member=\"{}\" NOT local (members={:?}) — cross to class def",
                    i.name,
                    member,
                    members
                );
            }
            let base = &i.base;
            match base.find_inst_with_span(member) {
                Some((m_inst, span)) => {
                    let kind = cross_def_kind(&m_inst);
                    mcc_dbg!("refdef::chain", "[member_of] Interface \"{}\" member=\"{}\" → {} kind={:?} span={:?} uri={}", i.name, member, m_inst.type_name(), kind, span, base.uri.as_str());
                    Some(Hop::CrossInst {
                        key: format!("{}.{}", i.name, member),
                        inst: m_inst,
                        def_kind: kind,
                        span,
                        uri: base.uri.clone(),
                    })
                }
                None => {
                    mcc_dbg!(
                        "refdef::chain",
                        "[member_of] Interface \"{}\" member=\"{}\" MISS → None",
                        i.name,
                        member
                    );
                    None
                }
            }
        }
        other => {
            mcc_dbg!(
                "refdef::chain",
                "[member_of] {} has no member table member=\"{}\" → None",
                inst.type_name(),
                member
            );
            let _ = other;
            None
        }
    }
}

/// Map a semantic member resolved inside a class definition to its lapper
/// def kind (aligned with `lapper_component_defs` / `lapper_module_ports` /
/// `lapper_interfaces`).
fn cross_def_kind(inst: &McInstance) -> SymbolKind {
    match inst {
        McInstance::Label(_) => SymbolKind::PinNameDef,
        McInstance::PinId(_) => SymbolKind::PinIdDef,
        McInstance::Bus(_) => SymbolKind::BusDef,
        McInstance::List(_) => SymbolKind::LabelDef,
        McInstance::Component(_) | McInstance::Module(_) => SymbolKind::InstDef,
        McInstance::Interface(_) => SymbolKind::PinIfaceDef,
        McInstance::Func(_) => SymbolKind::FuncDef,
        McInstance::Attr(_) => SymbolKind::AttrDef,
        McInstance::EnumVal { .. } => SymbolKind::EnumValDef,
        _ => SymbolKind::LabelDef,
    }
}

/// Classify a member resolved inside a module class definition.
///
/// Self-describing variants (Bus / List / Component / ...) are classified by
/// their own type via [`cross_def_kind`]. The only ambiguous case is
/// `McInstance::Label`: module ports (`in DAC_OUT`) and module labels (`GND`)
/// are both stored as `Label` — port-ness lives only in the IOType stored
/// alongside in the insts table (`(IOType, McInstance)`). A `Label` carrying a
/// port-like IOType maps to `PortDef` — the same predicate
/// `iter_ports_with_span` uses for `lapper_module_ports`; everything else
/// falls through to `cross_def_kind`.
fn module_member_kind(base: &McModule, member: &str, inst: &McInstance) -> SymbolKind {
    if let McInstance::Label(_) = inst {
        if let Some((io, _)) = base.insts.get_with_iotype(member) {
            if !matches!(
                io,
                IOType::None | IOType::Return | IOType::NonCon | IOType::Label
            ) {
                return SymbolKind::PortDef;
            }
        }
    }
    cross_def_kind(inst)
}

/// §4.3 (Phase 2): resolve a member-chain reference to its definition target.
///
/// Single-container version — `ref_text` is resolved against `insts` (ports,
/// buses, labels, instances) and `params` of the same module/component.
pub fn resolve_member_chain(
    uri: &McURI,
    ref_text: &str,
    insts: &McInstances,
    params: &McParamDeclares,
) -> Option<ChainHit> {
    let segs = split_segments(ref_text);
    mcc_dbg!(
        "refdef",
        "[chain] resolve_member_chain uri=\"{}\" ref={:?} segs={:?}",
        uri,
        ref_text,
        segs
    );
    let first = match segs.first() {
        Some(f) => f,
        None => {
            mcc_dbg!(
                "refdef::chain",
                "[chain] EMPTY segments for {:?} → None",
                ref_text
            );
            return None;
        }
    };
    let mut hop = match first_hop(insts, params, first, uri) {
        Some(h) => h,
        None => {
            mcc_dbg!(
                "refdef::chain",
                "[chain] first_hop MISS seg0={:?} → None",
                first
            );
            return None;
        }
    };
    mcc_dbg!(
        "refdef::chain",
        "[chain] first_hop HIT seg0={:?} → {}",
        first,
        hop.desc()
    );
    // ★ Longest-match walk: a dotted tail may be a single member name
    // (e.g. pin `IN.N` inside `lpa.IN.N`, where the amp component declares pin
    // `4 = IN.N`). At each position try the longest remaining suffix first
    // (full join → shorter joins → single segment), consuming as many
    // segments as the match covers. Genuine multi-level chains
    // (`uC.I2C0.SCL`) still work: the full suffix misses, then the walk
    // falls back to one segment and continues.
    let mut i = 1;
    while i < segs.len() {
        let inst = match container_inst(&hop) {
            Some(inst) => inst,
            None => {
                mcc_dbg!(
                    "refdef::chain",
                    "[chain] hop {} has no deeper container for segs[{}..]={:?} → None",
                    hop.desc(),
                    i,
                    &segs[i..]
                );
                return None;
            }
        };
        let mut best: Option<Hop<'static>> = None;
        let mut best_end = i + 1;
        for end in ((i + 1)..=segs.len()).rev() {
            let joined_name = segs[i..end].join(".");
            if let Some(h) = resolve_next_member(inst, &joined_name) {
                best = Some(h);
                best_end = end;
                break;
            }
        }
        match best {
            Some(h) => {
                mcc_dbg!(
                    "refdef::chain",
                    "[chain] hop segs[{}..{}]={:?} → {}",
                    i,
                    best_end,
                    &segs[i..best_end],
                    h.desc()
                );
                hop = h;
                i = best_end;
            }
            None => {
                mcc_dbg!(
                    "refdef::chain",
                    "[chain] member MISS segs[{}..]={:?} in hop={} → None",
                    i,
                    &segs[i..],
                    hop.desc()
                );
                return None;
            }
        }
    }
    let hit = finalize_hit(uri, hop, insts, params);
    mcc_dbg!("refdef", "[chain] finalize → {:?}", hit);
    hit
}

/// Convenience wrapper: resolve a chain from pre-split segments.
pub fn resolve_member_chain_from_segments(
    uri: &McURI,
    segs: &[ChainSegment],
    insts: &McInstances,
    params: &McParamDeclares,
) -> Option<ChainHit> {
    // `inst.f(..).member`: an Fcall segment is transparent — the function
    // returns `this` (chaining off a non-`this` return is rejected by
    // check_chain_validity/1316), so `.member` resolves against the receiver
    // instance. Skipping the Fcall avoids rebuilding a broken "uC..I2C0"
    // double-dot text (an empty segment would fail member resolution).
    let ref_text: String = segs
        .iter()
        .filter_map(|s| match s {
            ChainSegment::Ident(name) => Some(name.clone()),
            ChainSegment::Group { base, members } => {
                Some(format!("{}{{{}}}", base, members.join(",")))
            }
            ChainSegment::Fcall(_) => None,
        })
        .collect::<Vec<_>>()
        .join(".");
    resolve_member_chain(uri, &ref_text, insts, params)
}

/// Resolve only the base (first) segment of a member chain to its own def.
///
/// Used to register a separate base ref so hover / F12 on the base identifier
/// (e.g. `spk` in `spk.3`, `USB_VBUS_1` in `USB_VBUS_1.GND`) resolves to the
/// instance / parameter declaration instead of the whole-chain member target.
///
/// The module-parameter declaration is preferred over the inst-table bus entry:
/// curly-bus params (`USB_VBUS_1{VDD_3V, GND}`) record a use-site span in the
/// instance table but the declaration span in the param table.
pub fn resolve_base_hit(
    uri: &McURI,
    base: &str,
    insts: &McInstances,
    params: &McParamDeclares,
) -> Option<ChainHit> {
    if params.is_defined(base) {
        if let Some(hit) = finalize_hit(uri, Hop::Param(base.to_string()), insts, params) {
            return Some(hit);
        }
    }
    let hop = first_hop(insts, params, base, uri)?;
    finalize_hit(uri, hop, insts, params)
}

/// Base identifier text of a chain segment — `Ident("uC")` → `"uC"`,
/// `Group { base: "ADC", .. }` → `"ADC"`, `Fcall("i2c")` → `"i2c"`.
pub fn base_segment_name(seg: &ChainSegment) -> String {
    match seg {
        ChainSegment::Ident(name) => name.clone(),
        ChainSegment::Group { base, .. } => base.clone(),
        ChainSegment::Fcall(name) => name.clone(),
    }
}

/// Convert the final hop into a [`ChainHit`].
fn finalize_hit(
    uri: &McURI,
    hop: Hop<'_>,
    insts: &McInstances,
    params: &McParamDeclares,
) -> Option<ChainHit> {
    match hop {
        Hop::Inst { key, inst } => whole_hit(uri, &key, inst, insts),
        Hop::CrossInst {
            key,
            inst,
            def_kind,
            span,
            uri: def_uri,
        } => cross_hit(&key, &inst, def_kind, span, &def_uri),
        Hop::BusMember { bus, member } => bus_member_hit(uri, &bus, &member, insts),
        Hop::ListMember { list, member } => list_member_hit(uri, &list, &member, insts),
        Hop::Param(name) => param_hit(uri, &name, params),
        Hop::Class(c) => class_hit(&c),
    }
}

/// Class-definition target (`CAP`, `U_MCU`, ...): `ClassDef` at the
/// container's source span in its own definition file.
fn class_hit(c: &ContainerRef) -> Option<ChainHit> {
    let hit = match c {
        ContainerRef::Component(comp) => ChainHit {
            name: comp.name.to_string(),
            def_kind: SymbolKind::ClassDef,
            span: comp.span.start..comp.span.end,
            uri: comp.uri.clone(),
        },
        ContainerRef::Module(m) => ChainHit {
            name: m.name.to_string(),
            def_kind: SymbolKind::ClassDef,
            span: m.span.start..m.span.end,
            uri: m.uri.clone(),
        },
        ContainerRef::Interface(i) => ChainHit {
            name: i.name.to_string(),
            def_kind: SymbolKind::ClassDef,
            span: i.span.start..i.span.end,
            uri: i.uri.clone(),
        },
        ContainerRef::Enum(e) => ChainHit {
            name: e.name.to_string(),
            def_kind: SymbolKind::ClassDef,
            span: e.span[0] as usize..e.span[1] as usize,
            uri: e.uri.clone(),
        },
    };
    mcc_dbg!(
        "refdef::chain",
        "[class_hit] \"{}\" ClassDef span={:?}..{:?} uri={}",
        hit.name,
        hit.span.start,
        hit.span.end,
        hit.uri.as_str()
    );
    Some(hit)
}

/// Extract the container instance from an `Inst` / `CrossInst` hop.
/// Other hop kinds are not containers — `None`.
fn container_inst<'a>(hop: &'a Hop<'a>) -> Option<&'a McInstance> {
    match hop {
        Hop::Inst { inst, .. } => Some(inst),
        Hop::CrossInst { inst, .. } => Some(inst),
        Hop::BusMember { .. } | Hop::ListMember { .. } | Hop::Param(_) | Hop::Class(_) => None,
    }
}

/// Resolve the next chain segment against a container instance.
///
/// Phase 3: a single-member group (`uC.GPIO[1]`) first resolves the base
/// container (`GPIO`), then the specific member (`1`); multi-member groups and
/// dotted members resolve directly.
fn resolve_next_member(inst: &McInstance, seg: &str) -> Option<Hop<'static>> {
    match group_members(seg) {
        Some((base, members)) if members.len() == 1 => {
            let container = member_of(inst, &base)?;
            match container_inst(&container) {
                Some(c) => member_of(c, &members[0]),
                None => {
                    mcc_dbg!("refdef::chain", "[chain] grouped seg={:?} base={:?} — base container is not an instance → None", seg, base);
                    None
                }
            }
        }
        _ => member_of(inst, base_of(seg)),
    }
}

/// Cross-container member target (`uC.I2C0`): def is owned by the
/// class-definition file (`uri`), kind and span come from the semantic member.
fn cross_hit(
    key: &str,
    inst: &McInstance,
    def_kind: SymbolKind,
    span: Option<Range<usize>>,
    uri: &McURI,
) -> Option<ChainHit> {
    let span = span.unwrap_or(0..0);
    mcc_dbg!(
        "refdef::chain",
        "[cross_hit] \"{}\" {} kind={:?} span={:?}..{:?} uri={}",
        key,
        inst.type_name(),
        def_kind,
        span.start,
        span.end,
        uri.as_str()
    );
    Some(ChainHit {
        name: key.to_string(),
        def_kind,
        span,
        uri: uri.clone(),
    })
}

/// Whole-reference target: `MIC` → `BusDef`, `GPIO[1:2]`/`V3V3` → `LabelDef`,
/// instance → `InstDef`. `BusRef` (component.bus form) needs class context and
/// is Phase 3.
fn whole_hit(uri: &McURI, key: &str, inst: &McInstance, insts: &McInstances) -> Option<ChainHit> {
    let kind = match inst {
        McInstance::Bus(_) => SymbolKind::BusDef,
        McInstance::Component(_) | McInstance::Module(_) => SymbolKind::InstDef,
        McInstance::Interface(_) => SymbolKind::PortDef,
        McInstance::BusRef { .. } => {
            mcc_dbg!(
                "refdef::chain",
                "[whole_hit] BusRef key=\"{}\" → Phase 3 needs class context → None",
                key
            );
            return None;
        }
        _ => SymbolKind::LabelDef,
    };
    let span = whole_span(key, inst, insts);
    mcc_dbg!(
        "refdef::chain",
        "[whole_hit] key=\"{}\" kind={:?} span={:?}..{:?}",
        key,
        kind,
        span.start,
        span.end
    );
    Some(ChainHit {
        name: key.to_string(),
        def_kind: kind,
        span,
        uri: uri.clone(),
    })
}

/// Whole-def span: for buses prefer the registered [`crate::semantic::mc_inst::BusDef`]
/// span (covers the base identifier, not the braces); otherwise the first
/// port-span for the key.
fn whole_span(key: &str, inst: &McInstance, insts: &McInstances) -> Range<usize> {
    if let McInstance::Bus(_) = inst {
        if let Some(d) = insts.bus_def(key) {
            return d.span.clone();
        }
    }
    insts
        .port_spans()
        .get(key)
        .and_then(|v| v.first().cloned())
        .unwrap_or(0..0)
}

/// Bus member target: `MIC.P` → `BusMemberDef` with the precise member span.
/// Falls back to `LabelDef` on the whole-bus span when the bus was registered
/// without member spans.
fn bus_member_hit(uri: &McURI, bus: &str, member: &str, insts: &McInstances) -> Option<ChainHit> {
    let name = format!("{bus}.{member}");
    if let Some(d) = insts.bus_def(bus) {
        if let Some(span) = d.member_span(member) {
            mcc_dbg!(
                "refdef::chain",
                "[bus_member_hit] \"{}\" BusMemberDef span={:?}..{:?}",
                name,
                span.start,
                span.end
            );
            return Some(ChainHit {
                name,
                def_kind: SymbolKind::BusMemberDef,
                span,
                uri: uri.clone(),
            });
        }
        mcc_dbg!("refdef::chain", "[bus_member_hit] \"{}\" no member span in bus_def → LabelDef fallback on whole span {:?}..{:?}", name, d.span.start, d.span.end);
        return Some(ChainHit {
            name,
            def_kind: SymbolKind::LabelDef,
            span: d.span.clone(),
            uri: uri.clone(),
        });
    }
    let span = insts
        .port_spans()
        .get(bus)
        .and_then(|v| v.first().cloned())
        .unwrap_or(0..0);
    mcc_dbg!(
        "refdef::chain",
        "[bus_member_hit] \"{}\" no bus_def → LabelDef fallback on port_span {:?}..{:?}",
        name,
        span.start,
        span.end
    );
    Some(ChainHit {
        name,
        def_kind: SymbolKind::LabelDef,
        span,
        uri: uri.clone(),
    })
}

/// List member target: `GPIO[1]` → `LabelDef`. Span prefers a stored span for
/// the bracket form, then the whole-list span.
fn list_member_hit(uri: &McURI, list: &str, member: &str, insts: &McInstances) -> Option<ChainHit> {
    let bracket = format!("{list}[{member}]");
    let span = insts
        .port_spans()
        .get(&bracket)
        .and_then(|v| v.first().cloned())
        .or_else(|| {
            insts
                .port_spans()
                .get(list)
                .and_then(|v| v.first().cloned())
        })
        .unwrap_or(0..0);
    mcc_dbg!(
        "refdef::chain",
        "[list_member_hit] \"{}\" LabelDef span={:?}..{:?}",
        bracket,
        span.start,
        span.end
    );
    Some(ChainHit {
        name: bracket,
        def_kind: SymbolKind::LabelDef,
        span,
        uri: uri.clone(),
    })
}

/// Terminal parameter target: `ParamDef` at the first stored def span.
fn param_hit(uri: &McURI, name: &str, params: &McParamDeclares) -> Option<ChainHit> {
    let span = params
        .iter_defs_with_span()
        .find(|(n, _)| *n == name)
        .map(|(_, s)| s.clone())
        .unwrap_or(0..0);
    mcc_dbg!(
        "refdef::chain",
        "[param_hit] \"{}\" ParamDef span={:?}..{:?}",
        name,
        span.start,
        span.end
    );
    Some(ChainHit {
        name: name.to_string(),
        def_kind: SymbolKind::ParamDef,
        span,
        uri: uri.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::semantic::basic::mc_bus::{McBus, McList};
    use crate::semantic::common::IOType;
    use crate::semantic::component::mc_attr::McAttributes;
    use crate::semantic::component::mc_layout::McLayout;
    use crate::semantic::component::mc_pins::{McPinPort, McPins};
    use crate::semantic::component::{Mc2Component, McComponent};
    use crate::semantic::mc_enum::McEnumDef;
    use crate::semantic::mc_func::McFunctions;
    use crate::semantic::mc_inst::McInstance;
    use crate::semantic::module::McModule;

    use crate::McIds;
    use std::sync::Arc;

    /// Build a container mirroring the declarations in main.mc:
    /// `io MIC{P,N}`, `io GPIO[1:2]`, a plain label `V3V3`, a param `VCC_1V2`.
    fn make_insts() -> (McInstances, McParamDeclares) {
        let mut insts = McInstances::new();
        // io MIC{P,N} — named curly bus with per-member spans.
        insts.register_bus_def(
            "MIC",
            100..103,
            vec![("P".to_string(), 104..105), ("N".to_string(), 106..107)],
        );
        insts.create(
            "MIC",
            IOType::InOut,
            McInstance::Bus(McBus::new_with_members(
                "MIC",
                vec!["P".to_string(), "N".to_string()],
            )),
        );
        insts.store_port_span("MIC", 100..107);
        // io GPIO[1:2] — list.
        insts.create(
            "GPIO[1:2]",
            IOType::InOut,
            McInstance::List(McList::new_with_members(
                "GPIO",
                vec!["1".to_string(), "2".to_string()],
            )),
        );
        insts.store_port_span("GPIO[1:2]", 200..210);
        // Plain label V3V3.
        insts.create("V3V3", IOType::Power, McInstance::Label("V3V3".to_string()));
        insts.store_port_span("V3V3", 300..304);

        let mut params = McParamDeclares::new();
        params.store_def_span("VCC_1V2", 400..407);
        (insts, params)
    }

    #[test]
    fn split_segments_basic() {
        assert_eq!(
            split_segments("MIC.P"),
            vec!["MIC".to_string(), "P".to_string()]
        );
        assert_eq!(split_segments("MIC{P,N}"), vec!["MIC{P,N}".to_string()]);
        assert_eq!(
            split_segments("uC.ADC{P,N}"),
            vec!["uC".to_string(), "ADC{P,N}".to_string()]
        );
        assert_eq!(split_segments("GPIO[1:2]"), vec!["GPIO[1:2]".to_string()]);
        assert_eq!(split_segments("V3V3"), vec!["V3V3".to_string()]);
        assert_eq!(split_segments(""), Vec::<String>::new());
    }

    #[test]
    fn resolve_whole_bus() {
        let (insts, params) = make_insts();
        let hit = resolve_member_chain(&"t.mc".to_string(), "MIC", &insts, &params).unwrap();
        assert_eq!(hit.name, "MIC");
        assert_eq!(hit.def_kind, SymbolKind::BusDef);
        assert_eq!(hit.span, 100..103); // base only, no braces
    }

    #[test]
    fn resolve_whole_bus_curly() {
        let (insts, params) = make_insts();
        let hit = resolve_member_chain(&"t.mc".to_string(), "MIC{P,N}", &insts, &params).unwrap();
        assert_eq!(hit.name, "MIC");
        assert_eq!(hit.def_kind, SymbolKind::BusDef);
        assert_eq!(hit.span, 100..103);
    }

    #[test]
    fn resolve_bus_member() {
        let (insts, params) = make_insts();
        let hit = resolve_member_chain(&"t.mc".to_string(), "MIC.P", &insts, &params).unwrap();
        assert_eq!(hit.name, "MIC.P");
        assert_eq!(hit.def_kind, SymbolKind::BusMemberDef);
        assert_eq!(hit.span, 104..105); // precise member text
    }

    #[test]
    fn resolve_bus_member_curly_single() {
        let (insts, params) = make_insts();
        let hit = resolve_member_chain(&"t.mc".to_string(), "MIC{N}", &insts, &params).unwrap();
        assert_eq!(hit.name, "MIC.N");
        assert_eq!(hit.def_kind, SymbolKind::BusMemberDef);
        assert_eq!(hit.span, 106..107);
    }

    #[test]
    fn resolve_label() {
        let (insts, params) = make_insts();
        let hit = resolve_member_chain(&"t.mc".to_string(), "V3V3", &insts, &params).unwrap();
        assert_eq!(hit.name, "V3V3");
        assert_eq!(hit.def_kind, SymbolKind::LabelDef);
        assert_eq!(hit.span, 300..304);
    }

    #[test]
    fn resolve_list_whole() {
        let (insts, params) = make_insts();
        let hit = resolve_member_chain(&"t.mc".to_string(), "GPIO[1:2]", &insts, &params).unwrap();
        assert_eq!(hit.name, "GPIO[1:2]");
        assert_eq!(hit.def_kind, SymbolKind::LabelDef);
        assert_eq!(hit.span, 200..210);
    }

    #[test]
    fn resolve_list_member() {
        let (insts, params) = make_insts();
        let hit = resolve_member_chain(&"t.mc".to_string(), "GPIO[1]", &insts, &params).unwrap();
        assert_eq!(hit.name, "GPIO[1]");
        assert_eq!(hit.def_kind, SymbolKind::LabelDef);
    }

    #[test]
    fn resolve_list_member_digit_form() {
        let (insts, params) = make_insts();
        // `GPIO1` → idx-aware resolution to the `GPIO[1:2]` list, member `1`.
        let hit = resolve_member_chain(&"t.mc".to_string(), "GPIO1", &insts, &params).unwrap();
        assert_eq!(hit.name, "GPIO[1]");
        assert_eq!(hit.def_kind, SymbolKind::LabelDef);
    }

    #[test]
    fn resolve_param_terminal() {
        let (insts, params) = make_insts();
        let hit = resolve_member_chain(&"t.mc".to_string(), "VCC_1V2", &insts, &params).unwrap();
        assert_eq!(hit.name, "VCC_1V2");
        assert_eq!(hit.def_kind, SymbolKind::ParamDef);
        assert_eq!(hit.span, 400..407);
    }

    #[test]
    fn unknown_returns_none() {
        let (insts, params) = make_insts();
        assert!(resolve_member_chain(&"t.mc".to_string(), "NOPE", &insts, &params).is_none());
        // Member not in the bus.
        assert!(resolve_member_chain(&"t.mc".to_string(), "MIC.X", &insts, &params).is_none());
        // Nameless square group has no whole def.
        assert!(resolve_member_chain(&"t.mc".to_string(), "[VDD,GND]", &insts, &params).is_none());
    }

    /// Build a container with a component instance `uC` whose class definition
    /// (lib.mc) exposes pins `I2C0`, `ADC{P,N}` and `GPIO[1:2]` — the Phase 3
    /// cross-container resolution scenario.
    fn make_cross_insts() -> (McInstances, McParamDeclares) {
        let mut comp = McComponent {
            name: McIds::from("U1"),
            params: McParamDeclares::new(),
            pins: McPins::new(),
            attrs: McAttributes::new(),
            funcs: McFunctions::new(),
            insts: McInstances::new(),
            layout: McLayout {
                left: vec![],
                right: vec![],
                top: vec![],
                bottom: vec![],
            },
            uri: "lib.mc".to_string(),
            cond_pins: vec![],
            cond_attrs: vec![],
            span: 0..4,
        };
        // Pin I2C0 — a plain single pin.
        comp.pins
            .names_to_id
            .insert("I2C0".to_string(), McPinPort::Single("I2C0".to_string()));
        comp.pins
            .pin_name_spans
            .insert("I2C0".to_string(), 100..104);
        // Pin ADC{P,N} — a named bus pin.
        comp.pins.names_to_id.insert(
            "ADC".to_string(),
            McPinPort::Bus(McBus::new_with_members(
                "ADC",
                vec!["P".to_string(), "N".to_string()],
            )),
        );
        comp.pins.pin_name_spans.insert("ADC".to_string(), 200..203);
        // Pin GPIO[1:2] — a list pin.
        comp.pins.names_to_id.insert(
            "GPIO".to_string(),
            McPinPort::List("GPIO".to_string(), vec!["1".to_string(), "2".to_string()]),
        );
        comp.pins
            .pin_name_spans
            .insert("GPIO".to_string(), 300..304);

        let base = Arc::new(comp);
        let mc2 = Mc2Component::new("uC", base);
        let mut insts = McInstances::new();
        insts.create("uC", IOType::None, McInstance::Component(Arc::new(mc2)));
        (insts, McParamDeclares::new())
    }

    #[test]
    fn resolve_cross_component_member() {
        // `uC.I2C0` — member hop into the component class definition.
        let (insts, params) = make_cross_insts();
        let hit = resolve_member_chain(&"main.mc".to_string(), "uC.I2C0", &insts, &params).unwrap();
        assert_eq!(hit.name, "uC.I2C0");
        assert_eq!(hit.def_kind, SymbolKind::PinNameDef);
        assert_eq!(hit.span, 100..104); // precise pin-name span in lib.mc
        assert_eq!(hit.uri.as_str(), "lib.mc"); // cross-file def
    }

    #[test]
    fn resolve_cross_component_bus() {
        // `uC.ADC{P,N}` — whole grouped reference resolves to the bus pin.
        let (insts, params) = make_cross_insts();
        let hit =
            resolve_member_chain(&"main.mc".to_string(), "uC.ADC{P,N}", &insts, &params).unwrap();
        assert_eq!(hit.name, "uC.ADC");
        assert_eq!(hit.def_kind, SymbolKind::BusDef);
        assert_eq!(hit.span, 200..203);
        assert_eq!(hit.uri.as_str(), "lib.mc");
    }

    #[test]
    fn resolve_cross_component_list() {
        // `uC.GPIO[1:2]` — whole grouped reference to the list pin.
        let (insts, params) = make_cross_insts();
        let hit =
            resolve_member_chain(&"main.mc".to_string(), "uC.GPIO[1:2]", &insts, &params).unwrap();
        assert_eq!(hit.name, "uC.GPIO");
        assert_eq!(hit.def_kind, SymbolKind::LabelDef);
        assert_eq!(hit.span, 300..304);
        assert_eq!(hit.uri.as_str(), "lib.mc");
    }

    #[test]
    fn resolve_cross_missing_member() {
        // Unknown member in the component class → None.
        let (insts, params) = make_cross_insts();
        assert!(resolve_member_chain(&"main.mc".to_string(), "uC.NOPE", &insts, &params).is_none());
        // Unknown instance → None.
        assert!(
            resolve_member_chain(&"main.mc".to_string(), "other.I2C0", &insts, &params).is_none()
        );
    }

    // ── class_hit: `ContainerRef` → `ClassDef` mapping (P3-P5 base fallback) ──

    #[test]
    fn class_hit_maps_component_def() {
        let comp = Arc::new(McComponent {
            name: McIds::from("RES"),
            params: McParamDeclares::new(),
            pins: McPins::new(),
            attrs: McAttributes::new(),
            funcs: McFunctions::new(),
            insts: McInstances::new(),
            layout: McLayout {
                left: vec![],
                right: vec![],
                top: vec![],
                bottom: vec![],
            },
            uri: "lib.mc".to_string(),
            cond_pins: vec![],
            cond_attrs: vec![],
            span: 10..13,
        });
        let hit = class_hit(&ContainerRef::Component(comp)).unwrap();
        assert_eq!(hit.name, "RES");
        assert_eq!(hit.def_kind, SymbolKind::ClassDef);
        assert_eq!(hit.span, 10..13);
        assert_eq!(hit.uri.as_str(), "lib.mc");
    }

    #[test]
    fn class_hit_maps_enum_def() {
        let e = Arc::new(McEnumDef {
            name: McIds::from("PKG"),
            span: [20, 23],
            values: vec![],
            uri: "lib.mc".to_string(),
        });
        let hit = class_hit(&ContainerRef::Enum(e)).unwrap();
        assert_eq!(hit.name, "PKG");
        assert_eq!(hit.def_kind, SymbolKind::ClassDef);
        assert_eq!(hit.span, 20..23);
        assert_eq!(hit.uri.as_str(), "lib.mc");
    }

    #[test]
    fn class_hit_maps_module_def() {
        let m = Arc::new(McModule::test_stub("PWR"));
        let hit = class_hit(&ContainerRef::Module(m)).unwrap();
        assert_eq!(hit.name, "PWR");
        assert_eq!(hit.def_kind, SymbolKind::ClassDef);
        // test_stub span covers the bare name (0..len).
        assert_eq!(hit.span, 0..3);
    }
}
