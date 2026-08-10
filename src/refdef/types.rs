// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Unified ref/def type definitions — SymbolKind, RefDefMap, RefDefEntry, SourceLocation.
//!
//! Extracted from `ast/ast_semantic.rs` as the single source of truth for
//! symbol resolution types (see design doc §16).

use crate::McURI;
use std::collections::HashMap;

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
    /// ★ Bus member reference — e.g. `MIC.P`, `power.VCC` (see §3.2.3).
    BusRef = 26,
    /// ★ Rule 6: untyped param provisional type (see §3.2.3, §3.5.1).
    UnknownDef = 27,
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
            "unknown_def" => Some(Self::UnknownDef),
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
}

// ── RefDefMap ──

/// Unified symbol resolution table — built once at pass1 completion.
#[derive(Clone, Debug, Default)]
pub struct RefDefMap {
    /// (ref_kind, ref_id) → entry. Single-layer O(1) ID-based lookup.
    pub entries: HashMap<(SymbolKind, u32), RefDefEntry>,
    pub files: Vec<String>,
    pub containers: Vec<String>,
    /// ★ Use table: (file_uri, class_name) → entry for name-based P3/P4/P5 lookup.
    pub name_index: HashMap<(String, String), RefDefEntry>,
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

    /// Insert with name-based index for Use-table lookup.
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
        self.name_index
            .insert((lookup_file_uri.to_string(), class_name.to_string()), entry);
    }

    pub fn get(&self, kind: SymbolKind, ref_id: u32) -> Option<&RefDefEntry> {
        self.entries.get(&(kind, ref_id))
    }

    /// Add a name-index entry for a class definition under an alias.
    pub fn add_name_alias(&mut self, file_uri: &McURI, class_name: &str, entry: RefDefEntry) {
        self.name_index
            .insert((file_uri.to_string(), class_name.to_string()), entry);
    }

    /// Lookup by name in the Use table (P3/P4/P5).
    pub fn get_by_name(&self, file_uri: &str, class_name: &str) -> Option<&RefDefEntry> {
        self.name_index
            .get(&(file_uri.to_string(), class_name.to_string()))
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

    /// Intern a file URI into the file table, returning its u32 id.
    pub fn intern_file(&mut self, uri: &McURI) -> u32 {
        let s = uri.as_str().to_string();
        if let Some(pos) = self.files.iter().position(|x| x == &s) {
            pos as u32
        } else {
            let id = self.files.len() as u32;
            self.files.push(s);
            id
        }
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
