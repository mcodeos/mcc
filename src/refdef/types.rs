// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Unified ref/def type definitions — SymbolKind, RefDefMap, RefDefEntry, SourceLocation.
//!
//! Extracted from `ast/sem.rs` as the single source of truth for
//! symbol resolution types (see design doc §16).

use crate::McURI;
use std::collections::HashMap;

// ── ChainSegment ──

/// A segment in a member-chain reference, extracted from the AST.
///
/// Unlike text-based `split_segments` / `base_of`, this carries the parsed
/// structure directly — the AST already knows whether a segment is a plain
/// identifier, a bracketed group, or a function call, so chain resolution
/// must not re-parse brackets from raw text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainSegment {
    /// A plain identifier segment (e.g., `uC`, `I2C0`, `MIC`).
    Ident(String),
    /// A bracketed group attached to a base segment, e.g. `ADC{P,N}` →
    /// `Group { base: "ADC", members: ["P", "N"] }`, `GPIO[1:2]` →
    /// `Group { base: "GPIO", members: ["1", "2"] }`. Members are extracted
    /// from the AST node, so no string re-parsing is needed downstream.
    Group { base: String, members: Vec<String> },
    /// A function-call segment (e.g., `i2c(0x36)`). The name is the function
    /// identifier; args are omitted because the func returns `this` and the
    /// next segment resolves against the same container.
    Fcall(String),
}

// ── SourceLocation ──

/// ★ SourceLocation carries file_id/container_id/func_id/byte_start/byte_end
/// Replaces bare Span for precise location tracking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceLocation {
    pub file_id: u32,
    pub container_id: u32,
    pub func_id: u32,
    pub byte_start: u32,
    pub byte_end: u32,
}

impl SourceLocation {
    pub const NONE: SourceLocation = SourceLocation {
        file_id: 0,
        container_id: 0,
        func_id: 0,
        byte_start: 0,
        byte_end: 0,
    };

    pub fn new(file_id: u32, container_id: u32, byte_start: u32, byte_end: u32) -> Self {
        SourceLocation {
            file_id,
            container_id,
            func_id: 0,
            byte_start,
            byte_end,
        }
    }

    pub fn from_span(span: &std::ops::Range<usize>) -> Self {
        SourceLocation {
            file_id: 0,
            container_id: 0,
            func_id: 0,
            byte_start: span.start as u32,
            byte_end: span.end as u32,
        }
    }
}

// ── String interning ──

/// Intern `s` into `table`, returning its u32 id. Empty strings get id 0.
pub fn intern(table: &mut Vec<String>, s: &str) -> u32 {
    if s.is_empty() {
        return 0;
    }
    if let Some(pos) = table.iter().position(|x| x == s) {
        pos as u32
    } else {
        let id = table.len() as u32;
        table.push(s.to_string());
        id
    }
}

// ── SymbolType ──

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SymbolType {
    /// SymbolKind ordinal (u8). Maps to kind_names[] for serialization.
    pub kind: u8,
    /// DeclareId or ReferenceId as raw u32.
    pub id: u32,
}

impl SymbolType {
    pub fn new(kind: SymbolKind, id: u32) -> Self {
        SymbolType {
            kind: kind as u8,
            id,
        }
    }
}

// ── Compact SymbolKind for RefDefMap (replaces lapper kind strings) ──

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SymbolKind {
    ClassDef = 0,
    ClassRef = 1,
    InstDef = 2,
    InstRef = 3,
    PortDef = 4,
    PortRef = 5,
    LabelDef = 6,
    LabelRef = 7,
    FuncDef = 8,
    FuncRef = 9,
    PinIdDef = 10,
    PinIdRef = 11,
    PinNameDef = 12,
    PinNameRef = 13,
    PinIfaceDef = 14,
    PinIfaceRef = 15,
    EnumDef = 16,
    EnumRef = 17,
    EnumValDef = 18,
    EnumValRef = 19,
    RoleDef = 20,
    ParamDef = 21,
    DefineDef = 22,
    AttrDef = 23,
    /// ★ §15.1: Function parameter reference at call site.
    FuncParamRef = 24,
    /// ★ Bus definition — named group of nets (see §3.2.3).
    BusDef = 25,
    /// ★ Whole-bus reference — e.g. `MIC{P,N}` in a net (see §3.2.3).
    BusRef = 26,
    /// ★ Rule 6: untyped param provisional type (see §3.2.3, §3.5.1).
    UnknownDef = 27,
    /// ★ §3.4.3 (rev): bus member definition — one member of a named curly
    /// bus, registered with a precise span at its declaration text
    /// (e.g. `P` inside `io MIC{P,N}`). Lookup key is the full name `MIC.P`.
    BusMemberDef = 28,
    /// ★ §3.4.3 (rev): bus member reference — e.g. the `P` segment of `MIC.P`.
    /// Resolves to the member def (precise span), not the whole bus.
    BusMemberRef = 29,
}

impl SymbolKind {
    pub fn from_lapper_kind(kind: &str) -> Option<Self> {
        match kind {
            "class_def" | "class_definition" => Some(Self::ClassDef),
            "class_ref" | "declare_class" => Some(Self::ClassRef),
            "instance_def" | "declare_instance" => Some(Self::InstDef),
            "instance_ref" => Some(Self::InstRef),
            "port_def" => Some(Self::PortDef),
            "port_ref" => Some(Self::PortRef),
            "label_def" => Some(Self::LabelDef),
            "label_ref" => Some(Self::LabelRef),
            "function_def" => Some(Self::FuncDef),
            "function_ref" => Some(Self::FuncRef),
            "pin_id_def" => Some(Self::PinIdDef),
            "pin_id_ref" => Some(Self::PinIdRef),
            "pin_name_def" => Some(Self::PinNameDef),
            "pin_name_ref" => Some(Self::PinNameRef),
            "pin_iface_def" => Some(Self::PinIfaceDef),
            "pin_iface_ref" => Some(Self::PinIfaceRef),
            "enum_def" | "enum_class_def" => Some(Self::EnumDef),
            "enum_ref" | "enum_class_ref" => Some(Self::EnumRef),
            "enum_value_def" => Some(Self::EnumValDef),
            "enum_value_ref" => Some(Self::EnumValRef),
            "role_def" => Some(Self::RoleDef),
            "param_def" => Some(Self::ParamDef),
            "define_def" => Some(Self::DefineDef),
            "attr_def" => Some(Self::AttrDef),
            "func_param_ref" => Some(Self::FuncParamRef),
            "bus_def" => Some(Self::BusDef),
            "bus_ref" => Some(Self::BusRef),
            "bus_member_def" => Some(Self::BusMemberDef),
            "bus_member_ref" => Some(Self::BusMemberRef),
            "unknown_def" => Some(Self::UnknownDef),
            _ => None,
        }
    }

    /// Inverse of `SymbolType.kind` — reconstruct a SymbolKind from its
    /// u8 ordinal (as stored in the lapper). Used by position-aware LSP
    /// queries (hover) that read the lapper and need the exact def kind.
    pub fn from_raw(kind: u8) -> Option<Self> {
        match kind {
            0 => Some(Self::ClassDef),
            1 => Some(Self::ClassRef),
            2 => Some(Self::InstDef),
            3 => Some(Self::InstRef),
            4 => Some(Self::PortDef),
            5 => Some(Self::PortRef),
            6 => Some(Self::LabelDef),
            7 => Some(Self::LabelRef),
            8 => Some(Self::FuncDef),
            9 => Some(Self::FuncRef),
            10 => Some(Self::PinIdDef),
            11 => Some(Self::PinIdRef),
            12 => Some(Self::PinNameDef),
            13 => Some(Self::PinNameRef),
            14 => Some(Self::PinIfaceDef),
            15 => Some(Self::PinIfaceRef),
            16 => Some(Self::EnumDef),
            17 => Some(Self::EnumRef),
            18 => Some(Self::EnumValDef),
            19 => Some(Self::EnumValRef),
            20 => Some(Self::RoleDef),
            21 => Some(Self::ParamDef),
            22 => Some(Self::DefineDef),
            23 => Some(Self::AttrDef),
            24 => Some(Self::FuncParamRef),
            25 => Some(Self::BusDef),
            26 => Some(Self::BusRef),
            27 => Some(Self::UnknownDef),
            28 => Some(Self::BusMemberDef),
            29 => Some(Self::BusMemberRef),
            _ => None,
        }
    }

    pub fn is_ref(&self) -> bool {
        matches!(
            self,
            Self::ClassRef
                | Self::InstRef
                | Self::PortRef
                | Self::LabelRef
                | Self::FuncRef
                | Self::PinIdRef
                | Self::PinNameRef
                | Self::PinIfaceRef
                | Self::EnumRef
                | Self::EnumValRef
                | Self::FuncParamRef
                | Self::BusRef
                | Self::BusMemberRef
        )
    }

    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::ClassDef => "ClassDef",
            Self::ClassRef => "ClassRef",
            Self::InstDef => "InstDef",
            Self::InstRef => "InstRef",
            Self::PortDef => "PortDef",
            Self::PortRef => "PortRef",
            Self::LabelDef => "LabelDef",
            Self::LabelRef => "LabelRef",
            Self::FuncDef => "FuncDef",
            Self::FuncRef => "FuncRef",
            Self::PinIdDef => "PinIdDef",
            Self::PinIdRef => "PinIdRef",
            Self::PinNameDef => "PinNameDef",
            Self::PinNameRef => "PinNameRef",
            Self::PinIfaceDef => "PinIfaceDef",
            Self::PinIfaceRef => "PinIfaceRef",
            Self::EnumDef => "EnumDef",
            Self::EnumRef => "EnumRef",
            Self::EnumValDef => "EnumValDef",
            Self::EnumValRef => "EnumValRef",
            Self::RoleDef => "RoleDef",
            Self::ParamDef => "ParamDef",
            Self::DefineDef => "DefineDef",
            Self::AttrDef => "AttrDef",
            Self::FuncParamRef => "FuncParamRef",
            Self::BusDef => "BusDef",
            Self::BusRef => "BusRef",
            Self::UnknownDef => "UnknownDef",
            Self::BusMemberDef => "BusMemberDef",
            Self::BusMemberRef => "BusMemberRef",
        }
    }
}

// ── CMIE Kind ──

/// CMIE table kind for O(1) direct DashMap lookup.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CmieKind {
    Component = 0,
    Module = 1,
    Interface = 2,
    Enum = 3,
}

impl CmieKind {
    pub const UNKNOWN: u8 = 255;
}

// ── RefDefEntry ──

/// One entry in the unified ref→def map.
#[derive(Clone, Debug)]
pub struct RefDefEntry {
    pub ref_kind: SymbolKind,
    pub ref_id: u32,
    pub def_loc: SourceLocation,
    pub def_kind: SymbolKind,
    /// CMIE table kind for O(1) direct DashMap lookup (0=Comp,1=Mod,2=Ifs,3=Enum,255=unknown)
    pub cmie_kind: u8,
    /// ★ Exact def name captured at registration from the AST node
    /// (e.g. `RES`, `QFN20`). Emitted into the RPC payload so hover can show
    /// the def name without text-slicing the def line.
    pub def_name: String,
}

// ── Name-index candidates ──

/// The visibility layer through which a name became visible from a lookup
/// file (resolve-unification policy P3/P4/P5, design §5.4).
///
/// Each same-name candidate in [`RefDefMap::name_index`] keeps its layer so a
/// bucket can order itself deterministically: P3 (defined in the lookup file)
/// beats P4 (reached through its `use` chain), which beats P5 (a loaded
/// system library) — the precedence the old "later write overwrites" rule
/// implemented, but independent of map iteration order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameLayer {
    /// P3 — the def is declared in the lookup file itself.
    Own,
    /// P4 — the def is visible through the lookup file's `use` chain.
    Use,
    /// P5 — the def lives in a loaded system library.
    System,
}

impl NameLayer {
    /// Smaller rank wins the same-name resolution (Own < Use < System).
    fn rank(self) -> u8 {
        match self {
            NameLayer::Own => 0,
            NameLayer::Use => 1,
            NameLayer::System => 2,
        }
    }
}

/// One visible candidate for a `(lookup file, name)` key in
/// [`RefDefMap::name_index`]: the def entry plus the layer that made the name
/// visible. A bucket keeps every same-name def — resolution never drops a
/// candidate to a single overwrite; reads pick the deterministic winner
/// ([`RefDefMap::get_by_name`]) or scan the whole bucket
/// ([`RefDefMap::name_candidates`]).
#[derive(Debug, Clone)]
pub struct NameIndexCandidate {
    pub layer: NameLayer,
    pub entry: RefDefEntry,
}

impl NameIndexCandidate {
    /// Deterministic policy key — smaller wins. Layer first (P3 > P4 > P5),
    /// then the kind-family preference, then def-location tiebreaks. Never
    /// registration-order or map-iteration dependent.
    ///
    /// The family preference reproduces the historical winner of the old
    /// single-slot name_index: `consolidate_ref_def_map` registered class
    /// rows (components/modules/interfaces) and enum rows under the same
    /// `(lookup file, name)` key, enums after classes, so a same-named enum
    /// ended up owning the slot (last write wins). Preserving that choice
    /// keeps kind-blind consumers (`get_def`) returning the enum for a
    /// coexisting `component CAP` + `enum CAP`, which the coexistence
    /// regression (`tests/enum_component_same_name.rs`) pins down.
    fn policy_key(&self) -> (u8, u8, u32, u32, u32) {
        let family = match self.entry.def_kind {
            SymbolKind::EnumDef | SymbolKind::EnumRef => 0,
            SymbolKind::ClassDef | SymbolKind::ClassRef => 1,
            _ => 2,
        };
        (
            self.layer.rank(),
            family,
            self.entry.def_loc.file_id,
            self.entry.def_loc.byte_start,
            self.entry.def_loc.byte_end,
        )
    }

    /// True when this candidate is the def at `(file_id, span)`.
    fn is_def_at(&self, file_id: u32, byte_start: u32, byte_end: u32) -> bool {
        self.entry.def_loc.file_id == file_id
            && self.entry.def_loc.byte_start == byte_start
            && self.entry.def_loc.byte_end == byte_end
    }
}

// ── RefDefMap ──

/// Unified symbol resolution table — built once at pass1 completion.
#[derive(Clone, Debug, Default)]
pub struct RefDefMap {
    /// (ref_kind, ref_id) → entry. Single-layer O(1) ID-based lookup.
    pub entries: HashMap<(SymbolKind, u32), RefDefEntry>,
    pub containers: Vec<String>,
    /// ★ Use table (T10 / N2): (lookup_file_uri, class_name) → every def with
    /// that name visible from the lookup file, each tagged with its visibility
    /// layer (P3/P4/P5). A bucket holds all same-name candidates instead of
    /// one overwrite, so a name is never silently dropped and resolution never
    /// depends on map iteration or registration order — reads pick the
    /// deterministic winner ([`Self::get_by_name`]) or scan the bucket
    /// ([`Self::name_candidates`]).
    pub name_index: HashMap<(String, String), Vec<NameIndexCandidate>>,
    /// ★ §15.2: Reverse index — (def_kind, file_id, byte_start, byte_end) → [(ref_kind, ref_id)].
    /// Built alongside entries for O(1) find-all-references and rename.
    pub def_to_refs: HashMap<(SymbolKind, u32, u32, u32), Vec<(SymbolKind, u32)>>,
}

impl RefDefMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, kind: SymbolKind, ref_id: u32, mut entry: RefDefEntry) {
        entry.ref_kind = kind;
        entry.ref_id = ref_id;
        // ★ §15.2: Populate reverse index (def→refs)
        let def_key = (
            entry.def_kind,
            entry.def_loc.file_id,
            entry.def_loc.byte_start,
            entry.def_loc.byte_end,
        );
        self.def_to_refs
            .entry(def_key)
            .or_default()
            .push((kind, ref_id));
        self.entries.insert((kind, ref_id), entry);
    }

    /// Insert with name-based index for Use-table lookup. Legacy API kept for
    /// callers that register a defining file's own name (P3).
    pub fn insert_with_name(
        &mut self,
        kind: SymbolKind,
        ref_id: u32,
        lookup_file_uri: &McURI,
        class_name: &str,
        mut entry: RefDefEntry,
    ) {
        entry.ref_kind = kind;
        entry.ref_id = ref_id;
        // ★ §15.2: Populate reverse index
        let def_key = (
            entry.def_kind,
            entry.def_loc.file_id,
            entry.def_loc.byte_start,
            entry.def_loc.byte_end,
        );
        self.def_to_refs
            .entry(def_key)
            .or_default()
            .push((kind, ref_id));
        self.entries.insert((kind, ref_id), entry.clone());
        self.add_name_candidate(lookup_file_uri, class_name, NameLayer::Own, entry);
    }

    pub fn get(&self, kind: SymbolKind, ref_id: u32) -> Option<&RefDefEntry> {
        self.entries.get(&(kind, ref_id))
    }

    /// Record one visible def name from the lookup file's viewpoint (T10).
    /// Same-name candidates accumulate — never overwrite — and an exact
    /// duplicate (same def file + span, any layer) is skipped, so a system
    /// name re-exported through a `use` chain does not duplicate its direct
    /// P5 copy. Buckets are intentionally unordered; reads go through the
    /// deterministic policy ([`Self::name_winner`]) or the bucket scan
    /// ([`Self::name_candidates`]).
    pub fn add_name_candidate(
        &mut self,
        lookup_file_uri: &McURI,
        class_name: &str,
        layer: NameLayer,
        entry: RefDefEntry,
    ) {
        let bucket = self
            .name_index
            .entry((lookup_file_uri.to_string(), class_name.to_string()))
            .or_default();
        let dup = bucket.iter().any(|c| {
            c.is_def_at(
                entry.def_loc.file_id,
                entry.def_loc.byte_start,
                entry.def_loc.byte_end,
            )
        });
        if !dup {
            bucket.push(NameIndexCandidate { layer, entry });
        }
    }

    /// Deterministic name-policy winner among the candidates visible under
    /// `(file_uri, class_name)` — P3 > P4 > P5, then the enum-family
    /// preference, then def-location tiebreaks. Never registration-order or
    /// map-iteration dependent. `None` when the name is not visible.
    pub fn name_winner(&self, file_uri: &McURI, class_name: &str) -> Option<&RefDefEntry> {
        self.name_index
            .get(&(file_uri.to_string(), class_name.to_string()))
            .and_then(|bucket| {
                bucket
                    .iter()
                    .min_by_key(|c| c.policy_key())
                    .map(|c| &c.entry)
            })
    }

    /// Lookup by name in the Use table (P3/P4/P5): the deterministic winner
    /// among all visible candidates (see [`Self::name_winner`]).
    pub fn get_by_name(&self, file_uri: &str, class_name: &str) -> Option<&RefDefEntry> {
        self.name_winner(&McURI::from(file_uri), class_name)
    }

    /// Every visible candidate under `(file_uri, class_name)` — the full
    /// bucket, unordered. Visibility of a specific def must be checked against
    /// the whole bucket, because the deterministic winner may be a different
    /// same-named def (e.g. a class whose same-named enum outranks it).
    pub fn name_candidates(&self, file_uri: &McURI, class_name: &str) -> &[NameIndexCandidate] {
        self.name_index
            .get(&(file_uri.to_string(), class_name.to_string()))
            .map(|bucket| bucket.as_slice())
            .unwrap_or(&[])
    }

    /// ★ §15.2: Look up all refs for a given def.
    pub fn get_refs_for_def(
        &self,
        def_kind: SymbolKind,
        file_id: u32,
        byte_start: u32,
        byte_end: u32,
    ) -> &[(SymbolKind, u32)] {
        self.def_to_refs
            .get(&(def_kind, file_id, byte_start, byte_end))
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Intern a file URI into the global UriTable, returning its stable u32
    /// id (design: name-space-global.md §5.5 — single uri-id source shared
    /// with McSpaceName keys; ids are append-only and never recycled).
    pub fn intern_file(&mut self, uri: &McURI) -> u32 {
        crate::semantic::common::uri_intern(uri.as_str()).0
    }

    /// Intern a container name into the container table, returning its u32 id.
    pub fn intern_container(&mut self, name: &str) -> u32 {
        if name.is_empty() {
            return 0;
        }
        if let Some(pos) = self.containers.iter().position(|x| x == name) {
            pos as u32
        } else {
            let id = self.containers.len() as u32;
            self.containers.push(name.to_string());
            id
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(def_kind: SymbolKind, file_id: u32, start: u32, end: u32) -> RefDefEntry {
        RefDefEntry {
            ref_kind: SymbolKind::ClassDef,
            ref_id: 0,
            def_loc: SourceLocation {
                file_id,
                container_id: 0,
                func_id: 0,
                byte_start: start,
                byte_end: end,
            },
            def_kind,
            cmie_kind: if def_kind == SymbolKind::EnumDef {
                3
            } else {
                0
            },
            def_name: String::new(),
        }
    }

    fn uri(s: &str) -> McURI {
        McURI::from(s)
    }

    /// T10 (N2): the name_index bucket keeps every same-name candidate — a
    /// name is never silently dropped by a later write — and the winner is a
    /// deterministic policy (P3 > P4 > P5, enum family before class,
    /// def-location tiebreaks), never registration order.
    #[test]
    fn svc_types__name_index_keeps_all_candidates_and_winner_is_deterministic() {
        let mut map = RefDefMap::new();
        let f = uri("/mcc/t10.mc");

        // Component CAP (own file) and enum CAP (own file) with the same name.
        map.add_name_candidate(
            &f,
            "CAP",
            NameLayer::Own,
            entry(SymbolKind::ClassDef, 1, 100, 120),
        );
        map.add_name_candidate(
            &f,
            "CAP",
            NameLayer::Own,
            entry(SymbolKind::EnumDef, 1, 10, 30),
        );

        // Both survive; the bare-name winner is the enum, not the class —
        // reproducing the historical single-slot result (consolidate listed
        // class rows before enum rows, so the enum owned the slot) and
        // matching the coexistence regression
        // (tests/enum_component_same_name.rs) — regardless of write order.
        let bucket = map.name_candidates(&f, "CAP");
        assert_eq!(bucket.len(), 2, "both same-name defs stay visible");
        let winner = map.get_by_name(f.as_str(), "CAP").expect("winner");
        assert_eq!(
            winner.def_kind,
            SymbolKind::EnumDef,
            "bare-name resolution prefers the enum-kind def"
        );
        assert_eq!(winner.def_loc.byte_start, 10);

        // A same-name system candidate never displaces the own-file one: the
        // own-file (P3) candidate outranks the system (P5) candidate.
        map.add_name_candidate(
            &f,
            "CAP",
            NameLayer::System,
            entry(SymbolKind::ClassDef, 50, 500, 520),
        );
        let bucket = map.name_candidates(&f, "CAP");
        assert_eq!(
            bucket.len(),
            3,
            "the system candidate is an extra layer, not a drop"
        );
        let winner = map.get_by_name(f.as_str(), "CAP").expect("winner");
        assert_eq!(
            winner.def_loc.file_id, 1,
            "P3 outranks P5 for the same name"
        );

        // An exact duplicate (same def file + span) re-added through a use
        // chain is skipped — a system name re-exported by an import does not
        // double-list.
        map.add_name_candidate(
            &f,
            "CAP",
            NameLayer::Use,
            entry(SymbolKind::ClassDef, 50, 500, 520),
        );
        assert_eq!(
            map.name_candidates(&f, "CAP").len(),
            3,
            "re-export of the same system def dedupes"
        );

        // Two genuinely different P4 imports of the same name stay two
        // candidates and resolve by the deterministic def-location tiebreak.
        map.add_name_candidate(
            &f,
            "CAP",
            NameLayer::Use,
            entry(SymbolKind::ClassDef, 60, 700, 720),
        );
        let bucket = map.name_candidates(&f, "CAP");
        assert_eq!(bucket.len(), 4);
        assert_eq!(
            map.name_winner(&f, "CAP").unwrap().def_loc.file_id,
            1,
            "P3 winner is unaffected by imported candidates"
        );
        // Winner recomputed from the P4-only bucket (no own-file def) is
        // deterministic across the two imports.
        let mut p4_only = RefDefMap::new();
        p4_only.add_name_candidate(
            &f,
            "CAP",
            NameLayer::Use,
            entry(SymbolKind::ClassDef, 60, 700, 720),
        );
        p4_only.add_name_candidate(
            &f,
            "CAP",
            NameLayer::Use,
            entry(SymbolKind::ClassDef, 55, 600, 620),
        );
        let w1 = p4_only
            .get_by_name(f.as_str(), "CAP")
            .unwrap()
            .def_loc
            .file_id;
        // And re-adding in the reverse order gives the same winner.
        let mut p4_rev = RefDefMap::new();
        p4_rev.add_name_candidate(
            &f,
            "CAP",
            NameLayer::Use,
            entry(SymbolKind::ClassDef, 55, 600, 620),
        );
        p4_rev.add_name_candidate(
            &f,
            "CAP",
            NameLayer::Use,
            entry(SymbolKind::ClassDef, 60, 700, 720),
        );
        assert_eq!(
            p4_rev
                .get_by_name(f.as_str(), "CAP")
                .unwrap()
                .def_loc
                .file_id,
            w1,
            "winner is insertion-order independent"
        );
        assert!(
            p4_rev
                .get_by_name(f.as_str(), "CAP")
                .unwrap()
                .def_loc
                .file_id
                == 55,
            "def-location tiebreak picks the lexicographically first def file"
        );
    }
}
