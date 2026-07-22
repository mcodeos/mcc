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

// ★ Re-exported from refdef module (single source of truth, §16)
pub use crate::refdef::{intern, SourceLocation};

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
            file_table: vec![String::new()],
            container_table: vec![String::new()],
            func_table: vec![String::new()],
            // (my_components removed)
        }
    }
}

// ★ SymbolType re-exported from refdef
pub use crate::refdef::SymbolType;
impl SymbolType {
    /// Convenience: extract DeclareId from SymbolType's raw id.
    pub fn decl_id(&self) -> DeclareId {
        DeclareId { _raw: self.id }
    }
}
pub type SymbolRangeLapper = Lapper<usize, SymbolType>;

// ★ SymbolKind re-exported from refdef (single source of truth, §16)
pub use crate::refdef::SymbolKind;

// ★ Re-exported from refdef module (single source of truth, §16)
pub use crate::refdef::{CmieKind, RefDefEntry, RefDefMap};

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

    /// ★ P3: (file_id, container_id, func_id, name) → (declare_id, source_location).
    /// file_id/container_id/func_id from SourceLocation intern tables.
    /// Replaces (McURI, scope_str, name) triple with ID-based key.
    pub name_to_declare_id: HashMap<(u32, u32, u32, String), (DeclareId, SourceLocation)>,

    /// ★ Parallel index: scope string → (file_id, container_id, func_id).
    /// For scope-based lookups (e.g. "US513.i2c" → IDs) without parsing scope strings.
    pub scope_index: HashMap<String, (u32, u32, u32)>,

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
            scope_index: HashMap::new(),
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
        DeclareId {
            _raw: h.finish() as u32,
        }
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
            self.name_to_declare_id.insert(
                (loc.file_id, loc.container_id, loc.func_id, n),
                (declare_id, loc),
            );
        }
        // Populate scope_index for scope-based lookups
        if !scope_key.is_empty() {
            self.scope_index.entry(scope_key.to_string()).or_insert((
                loc.file_id,
                loc.container_id,
                loc.func_id,
            ));
        }
        declare_id
    }

    pub fn add_inst(&mut self, span: Span, declr_id: DeclareId) {
        let inst_id = self.assign_inst_id();
        self.inst_id_to_span.insert(inst_id, span.clone());
        self.inst_id_to_declare_inst.insert(inst_id, declr_id);
    }

    /// Look up a declare by scope string + name, using scope_index.
    pub fn lookup_by_scope_name(
        &self,
        scope_str: &str,
        name: &str,
    ) -> Option<(DeclareId, SourceLocation)> {
        let (fid, cid, fnid) = self.scope_index.get(scope_str)?;
        self.name_to_declare_id
            .get(&(*fid, *cid, *fnid, name.to_string()))
            .copied()
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

/// Helper: look up a file_id from the file_table (read-only, no interning).
fn resolve_file_id(file_table: &[String], uri: &McURI) -> u32 {
    file_table
        .iter()
        .position(|x| x == uri.as_str())
        .map(|i| i as u32)
        .unwrap_or(u32::MAX)
}

/// Helper: reconstruct a scope string from container_id and func_id.
pub fn scope_from_ids(
    container_table: &[String],
    func_table: &[String],
    cid: u32,
    fnid: u32,
) -> String {
    let container = if cid > 0 {
        container_table
            .get(cid as usize)
            .cloned()
            .unwrap_or_default()
    } else {
        String::new()
    };
    let func = if fnid > 0 {
        func_table.get(fnid as usize).cloned().unwrap_or_default()
    } else {
        String::new()
    };
    match (container.is_empty(), func.is_empty()) {
        (true, _) => func,
        (false, true) => container,
        (false, false) => format!("{container}.{func}"),
    }
}

/// Convert McSemSymbols to JSON for RPC transfer to LSP
pub fn symbol_table_to_json(symbols: &McSemSymbols, uri: &McURI) -> serde_json::Value {
    use serde_json::json;

    // Get local table data
    let local = &symbols.local_table;
    let file_id = resolve_file_id(&symbols.file_table, uri);
    let local_declares: Vec<serde_json::Value> = local
        .name_to_declare_id
        .iter()
        .filter(|((fid, _, _, _), _)| *fid == file_id)
        .map(|((_fid, cid, fnid, name), (id, loc))| {
            let scope = scope_from_ids(&symbols.container_table, &symbols.func_table, *cid, *fnid);
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
            let kind = interval.val.kind;
            let id = interval.val.id;
            let scope = symbols
                .local_table
                .name_to_declare_id
                .iter()
                .find(|(_, (_, s))| {
                    s.byte_start as usize == interval.start && s.byte_end as usize == interval.stop
                })
                .map(|((_fid, cid, fnid, _name), _)| {
                    scope_from_ids(&symbols.container_table, &symbols.func_table, *cid, *fnid)
                })
                .unwrap_or_default();
            json!({
                "kind": kind,
                "start": interval.start,
                "stop": interval.stop,
                "id": id,
                "scope": scope,
                "file": uri.as_str(),
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
            "kind_names": (0u8..=27).map(|i| {
                let kind: crate::ast::ast_semantic::SymbolKind = unsafe { std::mem::transmute(i) };
                kind.kind_name()
            }).collect::<Vec<_>>(),
            "result_id": result_id,
            // ★ §15.2: Reverse index for find-all-references
            "def_to_refs": m.def_to_refs.iter().map(|((dk, fid, bs, be), refs)| {
                json!({
                    "def_kind": *dk as u8,
                    "file_id": *fid,
                    "byte_start": *bs,
                    "byte_end": *be,
                    "refs": refs.iter().map(|(rk, rid)| json!([*rk as u8, *rid])).collect::<Vec<_>>(),
                })
            }).collect::<Vec<_>>(),
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
