// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::McURI;
use rust_lapper::Lapper;
use std::ops::Range;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

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

    pub fn from_span(span: &Span) -> Self {
        SourceLocation {
            file_id: 0,
            container_id: 0,
            func_id: 0,
            byte_start: span.start as u32,
            byte_end: span.end as u32,
        }
    }
}

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

#[derive(Clone, Debug)]
pub struct McSemSymbols {
    pub global_table: Arc<Mutex<GlobalSymbolTable>>,
    pub local_table: LocalSymbolTable,
    pub symbol_lapper: SymbolRangeLapper,
    /// ★ Unified ref→def map — built once at pass1 completion
    pub ref_def_map: Option<RefDefMap>,
    /// ★ A3: Pre-populated def_map — (def_kind, decl_id) → SourceLocation.
    /// Built during register_def, consumed by fill_refdef_layer2.
    pub def_map: HashMap<(SymbolKind, u32), SourceLocation>,
    /// ★ A3: Pre-collected ref entries — (ref_kind, decl_id, start, stop).
    /// Populated during lapper ref registration, consumed by fill_refdef_layer2
    /// (eliminates lapper scan for ref→def matching).
    pub ref_entries: Vec<(SymbolKind, u32, usize, usize)>,
    /// ★ SourceLocation tables: intern file/container/func names to u32 IDs.
    pub file_table: Vec<String>,
    pub container_table: Vec<String>,
    pub func_table: Vec<String>,
    // (my_components field removed — dead code)
}
impl Default for McSemSymbols {
    fn default() -> Self {
        Self::new()
    }
}

impl McSemSymbols {
    pub fn new() -> Self {
        McSemSymbols {
            global_table: Arc::new(Mutex::new(GlobalSymbolTable::new())),
            local_table: LocalSymbolTable::new(),
            symbol_lapper: SymbolRangeLapper::new(vec![]),
            ref_def_map: None,
            def_map: HashMap::new(),
            ref_entries: Vec::new(),
            file_table: Vec::new(),
            container_table: Vec::new(),
            func_table: Vec::new(),
            // (my_components removed)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolType {
    ClassDefinition(DeclareId), // component/module/interface/enum head
    DeclareClass(ReferenceId),
    DeclareInstance(DeclareId),
    InstanceRef(DeclareId),         // Reference to instance
    PortDefinition(DeclareId),      // ★ Module port definition (ps/io/in/out)
    EnumValueDefinition(DeclareId), // `SOP8,` — body row; id packed as (class<<16 | idx)
    EnumValueRef(DeclareId),        // `SOP8` in `PKG.SOP8`
    // ── M6 gaps: language constructs not previously tracked ──
    FunctionDefinition(DeclareId), // `func i2c()` — func name definition
    FunctionRef(DeclareId),        // function/method call reference
    ClassRef(DeclareId),           // standalone class ref: `RES(10k)` (not in declare)
    PinNameDefinition(DeclareId),  // pin name in component body: `1 = _CS`
    PinNameRef(DeclareId),         // pin name reference: `Pullup(_CS, V3V3)`
    DefineDefinition(DeclareId),   // `define name body`
    RoleDefinition(DeclareId),     // `role id { ... }`
    // ── Label support (scope design, step 7) ──
    LabelDefinition(DeclareId), // `label A` or inline label def
    LabelRef(DeclareId),        // label reference in a net phrase
    // ── RefDefMap gap fill (§3.2.2, §4.1) ──
    PortRef(DeclareId),            // port reference in net phrase
    PinIdDefinition(DeclareId),    // pin ID definition: `1` in `1 = _CS`
    PinIdRef(DeclareId),           // pin ID reference
    PinIfaceDefinition(DeclareId), // pin interface definition: `UART.TTL`
    PinIfaceRef(DeclareId),        // pin interface reference
    EnumDefinition(DeclareId),     // enum class definition (`enum PKG { ... }`)
    EnumRef(DeclareId),            // enum class reference
    ParamDefinition(DeclareId),    // parameter definition: `(cap::UV.CAP)`
    AttrDefinition(DeclareId),     // attribute definition: `capacitance = cap`
}
pub type SymbolRangeLapper = Lapper<usize, SymbolType>;

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
        }
    }
}

/// CMIE table kind for direct lookup — tells which WORKSPACE DashMap to query.
/// Mirrors the 4 CMIE tables. 255 = unknown (not a CMIE entry).
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

/// Unified symbol resolution table — built once at pass1 completion.
#[derive(Clone, Debug, Default)]
pub struct RefDefMap {
    /// (ref_kind, ref_id) → entry. Single-layer O(1) ID-based lookup.
    pub entries: HashMap<(SymbolKind, u32), RefDefEntry>,
    pub files: Vec<String>,
    pub containers: Vec<String>,
    /// ★ Use table: (file_uri, class_name) → entry for name-based P3/P4/P5 lookup.
    pub name_index: HashMap<(String, String), RefDefEntry>,
}

impl RefDefMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, kind: SymbolKind, ref_id: u32, mut entry: RefDefEntry) {
        entry.ref_kind = kind;
        entry.ref_id = ref_id;
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
        self.entries.insert((kind, ref_id), entry.clone());
        self.name_index
            .insert((lookup_file_uri.to_string(), class_name.to_string()), entry);
    }

    pub fn get(&self, kind: SymbolKind, ref_id: u32) -> Option<&RefDefEntry> {
        self.entries.get(&(kind, ref_id))
    }

    /// Add a name-index entry for a class definition.
    pub fn add_name_alias(&mut self, file_uri: &McURI, class_name: &str, entry: RefDefEntry) {
        self.name_index
            .insert((file_uri.to_string(), class_name.to_string()), entry);
    }

    /// Look up by (file_uri, class_name) — Use table query.
    pub fn get_by_name(&self, file_uri: &McURI, class_name: &str) -> Option<&RefDefEntry> {
        self.name_index
            .get(&(file_uri.to_string(), class_name.to_string()))
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn intern_file(&mut self, uri: &McURI) -> u32 {
        let s = uri.to_string();
        if let Some(pos) = self.files.iter().position(|f| f == &s) {
            pos as u32
        } else {
            let id = self.files.len() as u32;
            self.files.push(s);
            id
        }
    }

    pub fn intern_container(&mut self, name: &str) -> u32 {
        if name.is_empty() {
            return u32::MAX;
        }
        if let Some(pos) = self.containers.iter().position(|c| c == name) {
            pos as u32
        } else {
            let id = self.containers.len() as u32;
            self.containers.push(name.to_string());
            id
        }
    }
}

//---------------------------
pub type Span = Range<usize>;

oxc_index::define_index_type! {
    #[derive(Default)]
    pub struct DeclareId = u32;
    IMPL_RAW_CONVERSIONS = true;
}
oxc_index::define_index_type! {
    #[derive(Default)]
    pub struct ReferenceId = u32;
    IMPL_RAW_CONVERSIONS = true;
}

// Modification strategy: upon file modification, the entire table is cleared and re-added
#[derive(Default, Clone, Debug)]
pub struct LocalSymbolTable {
    declare_inst_id_counter: DeclareId,
    inst_id_counter: ReferenceId,

    /// (uri, scope, name) → (declare_id, source_location).
    /// SourceLocation carries file_id/container_id/func_id/span — replaces
    /// declare_inst_to_span + symbol_scope + scope string parsing.
    pub name_to_declare_id: HashMap<(McURI, String, String), (DeclareId, SourceLocation)>,

    pub inst_id_to_span: HashMap<ReferenceId, Span>,
    pub inst_id_to_declare_inst: HashMap<ReferenceId, DeclareId>,
    //.. pub class_id_reference_list : Vec<((McURI, String), Span)>,
}

impl LocalSymbolTable {
    pub fn new() -> Self {
        LocalSymbolTable {
            declare_inst_id_counter: DeclareId { _raw: 0 },
            inst_id_counter: ReferenceId { _raw: 0 },
            name_to_declare_id: HashMap::new(), // ★ LSP
            inst_id_to_span: HashMap::new(),
            inst_id_to_declare_inst: HashMap::new(),
        }
    }
    pub fn assign_declare_id(&mut self) -> DeclareId {
        let did = self.declare_inst_id_counter;
        self.declare_inst_id_counter += 1;
        did
    }
    /// ★ 14.2: Deterministic DeclareId via hash — stable across runs.
    pub fn assign_declare_id_stable(uri: &McURI, scope: &str, name: &str) -> DeclareId {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        uri.as_str().hash(&mut h);
        scope.hash(&mut h);
        name.hash(&mut h);
        DeclareId { _raw: h.finish() as u32 }
    }
    pub fn assign_inst_id(&mut self) -> ReferenceId {
        let rid = self.inst_id_counter;
        self.inst_id_counter += 1;
        rid
    }

    pub fn add_declare_with_name(
        &mut self,
        uri: &McURI,
        loc: SourceLocation,
        name: Option<String>,
        scope: Option<&str>,
    ) -> DeclareId {
        let scope_key = scope.unwrap_or("");
        let declare_id = if let Some(ref n) = name {
            Self::assign_declare_id_stable(uri, scope_key, n)
        } else {
            self.assign_declare_id()
        };
        if let Some(n) = name {
            self.name_to_declare_id
                .insert((uri.clone(), scope_key.to_string(), n), (declare_id, loc));
        }
        declare_id
    }

    pub fn add_inst(&mut self, span: Span, declr_id: DeclareId) {
        let inst_id = self.assign_inst_id();
        self.inst_id_to_span.insert(inst_id, span.clone());
        self.inst_id_to_declare_inst.insert(inst_id, declr_id);
    }
}

// Storage strategy: store declare + inst pairs, where declare is class definition and inst is instance reference
#[derive(Default, Clone, Debug)]
pub struct GlobalSymbolTable {
    class_id_counter: DeclareId,           // Global class ID counter
    declare_class_id_counter: ReferenceId, // Global reference ID counter

    pub class_name_to_id: HashMap<(McURI, String), DeclareId>, // id
    pub class_id_to_span: HashMap<DeclareId, (McURI, Span)>,   // Find class position in source code

    pub declare_class_id_to_span: HashMap<ReferenceId, (McURI, Span)>, // Find reference ID position in source code
    pub span_to_declare_class_id: HashMap<(McURI, Span), ReferenceId>, //
    pub declare_id_to_class_id: HashMap<ReferenceId, DeclareId>,       //

    // ★ LSP: enum global storage
    // (uri, class_name) -> class_id
    pub enum_class_name_to_id: HashMap<(McURI, String), DeclareId>,
    // class_id -> (uri, span) — span of the `enum PKG { ... }` head
    pub enum_class_id_to_span: HashMap<DeclareId, (McURI, Span)>,
    // value_id (packed: class_id << 16 | value_idx) -> (uri, span) of the value row
    pub enum_value_id_to_span: HashMap<DeclareId, (McURI, Span)>,
}

impl GlobalSymbolTable {
    pub fn new() -> Self {
        GlobalSymbolTable {
            class_id_counter: DeclareId { _raw: 0 },
            declare_class_id_counter: ReferenceId { _raw: 0 },

            class_name_to_id: HashMap::new(),
            class_id_to_span: HashMap::new(),
            declare_class_id_to_span: HashMap::new(),
            span_to_declare_class_id: HashMap::new(),
            declare_id_to_class_id: HashMap::new(),

            enum_class_name_to_id: HashMap::new(),
            enum_class_id_to_span: HashMap::new(),
            enum_value_id_to_span: HashMap::new(),
        }
    }
    pub fn assign_class_id(&mut self) -> DeclareId {
        let rid = self.class_id_counter;
        self.class_id_counter += 1;
        rid
    }
    pub fn assign_declare_class_id(&mut self) -> ReferenceId {
        let rid = self.declare_class_id_counter;
        self.declare_class_id_counter += 1;
        rid
    }

    // ★ LSP: enum id helpers
    /// Pack `(class_id, value_idx)` into a single DeclareId. Top 16 bits
    /// carry the class id; bottom 16 bits carry the value position in the
    /// body. This means a class can have up to 65536 values before
    /// collisions.
    pub fn pack_enum_value_id(class_id: DeclareId, value_idx: u32) -> DeclareId {
        let c = class_id._raw;
        let v = value_idx & 0xFFFF;
        DeclareId {
            _raw: ((c & 0xFFFF) << 16) | v,
        }
    }

    /// Register an enum class definition (`enum PKG { ... }`).
    pub fn add_enum_class(&mut self, uri: &McURI, class_name: &str, span: Span) -> DeclareId {
        // Reuse class_id_counter so enum class ids do not collide with
        // component / interface / module ids used elsewhere.
        let cls_id = self.assign_class_id();
        self.enum_class_name_to_id
            .insert((uri.clone(), class_name.to_string()), cls_id);
        self.enum_class_id_to_span
            .insert(cls_id, (uri.clone(), span));
        cls_id
    }

    /// Register an enum value row (`SOP8,` inside `enum PKG { ... }`).
    /// `value_idx` is the position inside the class body (0-based).
    pub fn add_enum_value(
        &mut self,
        uri: &McURI,
        class_id: DeclareId,
        value_idx: u32,
        span: Span,
    ) -> DeclareId {
        let value_id = Self::pack_enum_value_id(class_id, value_idx);
        self.enum_value_id_to_span
            .insert(value_id, (uri.clone(), span));
        value_id
    }

    /// Look up enum class id by (uri, class_name). Returns None if absent.
    pub fn lookup_enum_class(&self, uri: &McURI, class_name: &str) -> Option<DeclareId> {
        self.enum_class_name_to_id
            .get(&(uri.clone(), class_name.to_string()))
            .copied()
    }

    /// Look up enum class span by class_id.
    pub fn enum_class_span(&self, class_id: DeclareId) -> Option<&(McURI, Span)> {
        self.enum_class_id_to_span.get(&class_id)
    }

    /// Look up enum value span by value_id.
    pub fn enum_value_span(&self, value_id: DeclareId) -> Option<&(McURI, Span)> {
        self.enum_value_id_to_span.get(&value_id)
    }

    pub fn add_class(&mut self, uri: &McURI, class_name: &String, span: Span) -> DeclareId {
        let cls_id = self.assign_class_id();
        self.class_name_to_id
            .insert((uri.clone(), class_name.clone()), cls_id);
        self.class_id_to_span
            .insert(cls_id, (uri.clone(), span.clone()));
        cls_id
    }

    pub fn add_declare_class(
        &mut self,
        uri: &McURI,
        span: Span,
        class_id: DeclareId,
    ) -> ReferenceId {
        let reference_id = self.assign_declare_class_id();
        //1. Register reference_id
        self.declare_class_id_to_span
            .insert(reference_id, (uri.clone(), span.clone()));
        //2. Record reference_id position
        self.span_to_declare_class_id
            .insert((uri.clone(), span.clone()), reference_id);
        //3. reference_id -> class_id
        self.declare_id_to_class_id.insert(reference_id, class_id);
        reference_id
    }

    pub fn clear_by_uri(&mut self, target_uri: &McURI) {
        // 1. Remove declare_class_id for target file, then re-add
        let dcls_id_to_remove: Vec<ReferenceId> = self
            .span_to_declare_class_id
            .iter()
            .filter(|((uri, _span), _dcls_id)| uri == target_uri)
            .map(|(_key, ref_id)| *ref_id)
            .collect();

        let _ = dcls_id_to_remove.iter().map(|id| {
            self.declare_class_id_to_span.remove(id);
            self.declare_id_to_class_id.remove(id);
        });
        self.span_to_declare_class_id
            .retain(|(uri, _), _| uri != target_uri);

        // 2. Remove class_id for target file, then re-add
        let class_id_to_remove: Vec<DeclareId> = self
            .class_name_to_id
            .iter()
            .filter(|((uri, _name), _cls_id)| uri == target_uri)
            .map(|(_key, cls_id)| *cls_id)
            .collect();

        let _ = class_id_to_remove.iter().map(|clsid| {
            self.class_id_to_span.remove(clsid);
        });
        self.class_name_to_id
            .retain(|(uri, _), _| uri != target_uri);
    }

    pub fn clear(&mut self) {
        *self = GlobalSymbolTable {
            class_id_counter: DeclareId { _raw: 0 },
            declare_class_id_counter: ReferenceId { _raw: 0 },

            class_name_to_id: HashMap::new(),
            class_id_to_span: HashMap::new(),
            declare_class_id_to_span: HashMap::new(),
            span_to_declare_class_id: HashMap::new(),
            declare_id_to_class_id: HashMap::new(),

            enum_class_name_to_id: HashMap::new(),
            enum_class_id_to_span: HashMap::new(),
            enum_value_id_to_span: HashMap::new(),
        };
    }

    // (global_inst methods removed — dead code)
}

/// Convert McSemSymbols to JSON for RPC transfer to LSP
pub fn symbol_table_to_json(symbols: &McSemSymbols, uri: &McURI) -> serde_json::Value {
    use serde_json::json;

    // Get local table data
    let local = &symbols.local_table;
    let local_declares: Vec<serde_json::Value> = local
        .name_to_declare_id
        .iter()
        .filter(|((u, _, _), _)| u == uri)
        .map(|((_, scope, name), (id, loc))| {
            json!({
                "kind": "declare",
                "id": id._raw,
                "span": [loc.byte_start, loc.byte_end],
                "scope": scope,
                "name": name,
            })
        })
        .collect();

    let local_references: Vec<serde_json::Value> = local
        .inst_id_to_span
        .iter()
        .map(|(id, span)| {
            let declare_id = local.inst_id_to_declare_inst.get(id).map(|d| d._raw);
            json!({
                "kind": "reference",
                "id": id._raw,
                "span": [span.start, span.end],
                "declare_id": declare_id,
            })
        })
        .collect();

    // Get lapper ranges (local symbol positions)
    let lapper_ranges: Vec<serde_json::Value> = symbols
        .symbol_lapper
        .iter()
        .map(|interval| {
            let (kind, id) = match interval.val {
                SymbolType::ClassDefinition(id) => (SymbolKind::ClassDef as u8, id._raw),
                SymbolType::DeclareClass(id) => (SymbolKind::ClassRef as u8, id._raw),
                SymbolType::ClassRef(id) => (SymbolKind::ClassRef as u8, id._raw),
                SymbolType::DeclareInstance(id) => (SymbolKind::InstDef as u8, id._raw),
                SymbolType::InstanceRef(id) => (SymbolKind::InstRef as u8, id._raw),
                SymbolType::EnumValueDefinition(id) => (SymbolKind::EnumValDef as u8, id._raw),
                SymbolType::EnumValueRef(id) => (SymbolKind::EnumValRef as u8, id._raw),
                SymbolType::DefineDefinition(id) => (SymbolKind::DefineDef as u8, id._raw),
                SymbolType::FunctionDefinition(id) => (SymbolKind::FuncDef as u8, id._raw),
                SymbolType::FunctionRef(id) => (SymbolKind::FuncRef as u8, id._raw),
                SymbolType::PortDefinition(id) => (SymbolKind::PortDef as u8, id._raw),
                SymbolType::PinNameDefinition(id) => (SymbolKind::PinNameDef as u8, id._raw),
                SymbolType::PinNameRef(id) => (SymbolKind::PinNameRef as u8, id._raw),
                SymbolType::RoleDefinition(id) => (SymbolKind::RoleDef as u8, id._raw),
                SymbolType::LabelDefinition(id) => (SymbolKind::LabelDef as u8, id._raw),
                SymbolType::LabelRef(id) => (SymbolKind::LabelRef as u8, id._raw),
                SymbolType::PortRef(id) => (SymbolKind::PortRef as u8, id._raw),
                SymbolType::PinIdDefinition(id) => (SymbolKind::PinIdDef as u8, id._raw),
                SymbolType::PinIdRef(id) => (SymbolKind::PinIdRef as u8, id._raw),
                SymbolType::PinIfaceDefinition(id) => (SymbolKind::PinIfaceDef as u8, id._raw),
                SymbolType::PinIfaceRef(id) => (SymbolKind::PinIfaceRef as u8, id._raw),
                SymbolType::EnumDefinition(id) => (SymbolKind::EnumDef as u8, id._raw),
                SymbolType::EnumRef(id) => (SymbolKind::EnumRef as u8, id._raw),
                SymbolType::ParamDefinition(id) => (SymbolKind::ParamDef as u8, id._raw),
                SymbolType::AttrDefinition(id) => (SymbolKind::AttrDef as u8, id._raw),
            };
            let scope = symbols
                .local_table
                .name_to_declare_id
                .iter()
                .find(|(_, (_, s))| s.byte_start as usize == interval.start && s.byte_end as usize == interval.stop)
                .map(|((_, scope, _), _)| scope.clone())
                .unwrap_or_default();
            json!({
                "kind": kind,
                "start": interval.start,
                "stop": interval.stop,
                "id": id,
                "scope": scope,
            })
        })
        .collect();

    // Get global table data for this URI
    let gtable = symbols.global_table.lock().ok();
    let uri_str = uri.as_str();
    let global_declares: Vec<serde_json::Value> = gtable
        .as_ref()
        .map(|g| {
            g.class_id_to_span
                .iter()
                .filter(|(_id, (file_uri, _))| file_uri.as_str() == uri_str)
                .map(|(id, (file_uri, span))| {
                    json!({
                        "id": id._raw,
                        "uri": file_uri,
                        "span": [span.start, span.end],
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let global_references: Vec<serde_json::Value> = gtable
        .as_ref()
        .map(|g| {
            g.declare_class_id_to_span
                .iter()
                .filter(|(_id, (file_uri, _))| file_uri.as_str() == uri_str)
                .map(|(id, (file_uri, span))| {
                    json!({
                        "id": id._raw,
                        "uri": file_uri,
                        "span": [span.start, span.end],
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    // ★ cross_file_targets: deprecated — replaced by ref_def_map.
    // Kept as empty array for backward compatibility with older mcext.

    // ★ §7.6: Build ref_def_map JSON with result_id hash for mcext dedup.
    let ref_def_map_json = symbols.ref_def_map.as_ref().map(|m| {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        m.entries.len().hash(&mut hasher);
        m.files.len().hash(&mut hasher);
        m.containers.len().hash(&mut hasher);
        m.name_index.len().hash(&mut hasher);
        if let Some((_, e)) = m.entries.iter().next() {
            e.ref_kind.hash(&mut hasher);
            e.def_loc.file_id.hash(&mut hasher);
            e.def_loc.byte_start.hash(&mut hasher);
            e.def_loc.byte_end.hash(&mut hasher);
        }
        let result_id = hasher.finish();

        json!({
            "entries": m.entries.iter().map(|((_kind, _id), e)| {
                json!({
                    "ref_kind": e.ref_kind as u8,
                    "ref_id": e.ref_id,
                    "file_id": e.def_loc.file_id,
                    "def_span": [e.def_loc.byte_start, e.def_loc.byte_end],
                    "def_kind": e.def_kind as u8,
                    "container_id": e.def_loc.container_id,
                    "cmie_kind": e.cmie_kind,
                })
            }).collect::<Vec<_>>(),
            "files": &m.files,
            "containers": &m.containers,
            "kind_names": (0u8..=23).map(|i| {
                let kind: crate::ast::ast_semantic::SymbolKind = unsafe { std::mem::transmute(i) };
                kind.kind_name()
            }).collect::<Vec<_>>(),
            "result_id": result_id,
        })
    });

    json!({
        "local": {
            "declares": local_declares,
            "references": local_references,
        },
        "lapper": lapper_ranges,
        "global": {
            "declares": global_declares,
            "references": global_references,
            "cross_file_targets": [],
        },
        "ref_def_map": ref_def_map_json,
    })
}
