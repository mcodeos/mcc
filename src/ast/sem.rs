// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::McIds;
use crate::McURI;
use rust_lapper::Lapper;
use std::ops::Range;
use std::{
    collections::HashMap,
    sync::atomic::{AtomicU32, Ordering},
    sync::{Arc, LazyLock, Mutex},
};

// ★ Re-exported from refdef module (single source of truth, §16)
pub use crate::refdef::{intern_uri, SourceLocation};

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
    /// ★ Def names by (def_kind, decl_id) — captured at def registration from
    /// the AST node (class name, enum value name, ...). Emitted into the
    /// RefDefMap RPC payload so mcext hover can show the exact def name
    /// (e.g. `RES`) without text slicing of the def line.
    pub def_names: HashMap<(SymbolKind, u32), String>,
    /// ★ SourceLocation tables: intern container/func names to u32 IDs.
    /// File identity is the process-global `UriId` (`refdef::types::intern_uri`),
    /// not a per-file table (CIMP U81 ①).
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
            def_names: HashMap::new(),
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
    inst_id_counter: ReferenceId,

    /// ★ P3: (uri_id, kind, scope, name) → (declare_id, source_location) — the
    /// canonical key (CIMP U81 ②, kind dimension added by CIMP U269), same
    /// shape as `McSpaceName`. `uri_id` is the process-global `UriId` of the
    /// owning file (0 = none, e.g. an inline port registered with a fileless
    /// `SourceLocation`).
    pub name_to_declare_id: HashMap<DeclareKey, (DeclareId, SourceLocation)>,

    /// ★ P0: reverse name index — name → (uri_id, kind, scope) keys in
    /// registration order. Turns the linear `name_to_declare_id.iter().find(..)`
    /// class-ref lookup in `Resolver::resolve_class_locked` into an O(1) index
    /// hit.
    pub name_to_declare_ids: HashMap<String, Vec<(u32, u8, String)>>,

    /// ★ Parallel index: scope string → the `UriId` its first registration came
    /// from. Turns a scope-string lookup into the canonical key without a scan.
    pub scope_index: HashMap<String, u32>,

    pub inst_id_to_span: HashMap<ReferenceId, Span>,
    pub inst_id_to_declare_inst: HashMap<ReferenceId, DeclareId>,
    //.. pub class_id_reference_list : Vec<((McURI, String), Span)>,
}

/// The canonical declaration key: `(uri_id, kind, scope, name)`. The kind
/// dimension (CIMP U269) keeps two defs of one name in one scope — a component
/// parameter and a pin, say — in two ids with two positions, instead of the
/// later registration overwriting the earlier one's loc.
pub type DeclareKey = (u32, u8, String, String);

/// Canonical key → `DeclareId` — the single allocator of
/// the definition-entry id space (CIMP U81 ③, build-design §3.7 discipline 0
/// ②a: the id is the key's encoding, so one key has one id and a repeat
/// registration reuses it instead of consuming a number whose value depends on
/// how much was parsed before it in the process). The key's shape is the same
/// as `LocalSymbolTable::name_to_declare_id`, which stays a per-file index.
static DECLARE_ID_BY_KEY: LazyLock<Mutex<HashMap<DeclareKey, u32>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Orders first registrations only; ids are monotonic and never recycled.
static NEXT_DECLARE_ID: AtomicU32 = AtomicU32::new(1);

/// Intern the canonical key to its `DeclareId`. Shared by the declaration
/// table and the global class table so one key has one id.
pub(crate) fn intern_declare_id(
    uri_id: u32,
    scope: &str,
    name: &str,
    kind: SymbolKind,
) -> DeclareId {
    let mut table = DECLARE_ID_BY_KEY.lock().unwrap();
    let key: DeclareKey = (uri_id, kind as u8, scope.to_string(), name.to_string());
    if let Some(raw) = table.get(&key) {
        return DeclareId { _raw: *raw };
    }
    let raw = NEXT_DECLARE_ID.fetch_add(1, Ordering::Relaxed);
    table.insert(key, raw);
    DeclareId { _raw: raw }
}

/// Drop every interned key and restart the sequence. A full state reset
/// (`clear_state(ClearScope::Full)`) rebuilds every table that holds these ids,
/// so the ledger goes with it: otherwise a second clean load inside one process
/// keeps numbering where the first ended, and a file's `id=` values depend on
/// how much was parsed before it.
pub(crate) fn reset_declare_id_space() {
    DECLARE_ID_BY_KEY.lock().unwrap().clear();
    NEXT_DECLARE_ID.store(1, Ordering::Relaxed);
}

impl LocalSymbolTable {
    pub fn new() -> Self {
        LocalSymbolTable {
            inst_id_counter: ReferenceId { _raw: 0 },
            name_to_declare_id: HashMap::new(), // ★ LSP
            name_to_declare_ids: HashMap::new(),
            scope_index: HashMap::new(),
            inst_id_to_span: HashMap::new(),
            inst_id_to_declare_inst: HashMap::new(),
        }
    }
    pub fn assign_inst_id(&mut self) -> ReferenceId {
        let rid = self.inst_id_counter;
        self.inst_id_counter += 1;
        rid
    }

    /// Register the declaration of `name` in `scope` for the file owning `loc`.
    /// The `DeclareId` is derived from the canonical key, so two registrations
    /// of one key — wherever they happen — yield one id (a `PortRef` looked up
    /// before its `PortDef` is registered and the `PortDef` itself must agree).
    /// Two defs of one name in one scope but of different kinds (a parameter
    /// and a pin, say) are two keys and two ids (CIMP U269).
    pub fn add_declare_with_name(
        &mut self,
        loc: SourceLocation,
        name: &str,
        scope: &str,
        kind: SymbolKind,
    ) -> DeclareId {
        let key: DeclareKey = (
            loc.file_id,
            kind as u8,
            scope.to_string(),
            name.to_string(),
        );
        let declare_id = intern_declare_id(loc.file_id, scope, name, kind);
        let existed = self.name_to_declare_id.contains_key(&key);
        self.name_to_declare_id.insert(key, (declare_id, loc));
        // ★ P0: keep the reverse name index in sync (first registration only).
        if !existed {
            self.name_to_declare_ids
                .entry(name.to_string())
                .or_default()
                .push((loc.file_id, kind as u8, scope.to_string()));
        }
        // Populate scope_index for scope-based lookups
        if !scope.is_empty() {
            self.scope_index
                .entry(scope.to_string())
                .or_insert(loc.file_id);
        }
        declare_id
    }

    pub fn add_inst(&mut self, span: Span, declr_id: DeclareId) {
        let inst_id = self.assign_inst_id();
        self.inst_id_to_span.insert(inst_id, span.clone());
        self.inst_id_to_declare_inst.insert(inst_id, declr_id);
    }

    /// Look up a declare by scope string + name + exact kind, using scope_index
    /// to reach the owning file's canonical key.
    pub fn lookup_by_scope_name(
        &self,
        scope_str: &str,
        name: &str,
        kind: SymbolKind,
    ) -> Option<(DeclareId, SourceLocation)> {
        let uri_id = *self.scope_index.get(scope_str)?;
        self.name_to_declare_id
            .get(&(uri_id, kind as u8, scope_str.to_string(), name.to_string()))
            .copied()
    }

    /// Kind-agnostic lookup: the lowest registered kind wins. Deterministic
    /// (unlike a HashMap iteration), and for a name registered once it is the
    /// same answer `lookup_by_scope_name` gives. Callers that genuinely do not
    /// know the def kind of the name they seek use this; callers that know use
    /// `lookup_by_scope_name` (CIMP U269).
    pub fn lookup_any_by_scope_name(
        &self,
        scope_str: &str,
        name: &str,
    ) -> Option<(DeclareId, SourceLocation)> {
        let uri_id = *self.scope_index.get(scope_str)?;
        (0..=u8::MAX)
            .find_map(|k| {
                self.name_to_declare_id
                    .get(&(uri_id, k, scope_str.to_string(), name.to_string()))
                    .copied()
            })
    }
}

// Storage strategy: store declare + inst pairs, where declare is class definition and inst is
// instance reference
#[derive(Default, Clone, Debug)]
pub struct GlobalSymbolTable {
    class_id_counter: DeclareId,           // Global class ID counter
    declare_class_id_counter: ReferenceId, // Global reference ID counter

    pub class_name_to_id: HashMap<(McURI, McIds), DeclareId>, // id
    pub class_id_to_span: HashMap<DeclareId, (McURI, Span)>,  // Find class position in source code

    pub declare_class_id_to_span: HashMap<ReferenceId, (McURI, Span)>, // Find reference ID position in source code
    pub span_to_declare_class_id: HashMap<(McURI, Span), ReferenceId>, //
    pub declare_id_to_class_id: HashMap<ReferenceId, DeclareId>,       //

    // ★ LSP: enum global storage
    // (uri, class_name) -> class_id
    pub enum_class_name_to_id: HashMap<(McURI, McIds), DeclareId>,
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
            _raw: ((c & 0xFFFF) << 16) | (v as u32),
        }
    }

    /// Register an enum class definition (`enum PKG { ... }`).
    pub fn add_enum_class(&mut self, uri: &McURI, class_name: &McIds, span: Span) -> DeclareId {
        // Reuse class_id_counter so enum class ids do not collide with
        // component / interface / module ids used elsewhere.
        let cls_id = self.assign_class_id();
        self.enum_class_name_to_id
            .insert((uri.clone(), class_name.clone()), cls_id);
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
    pub fn lookup_enum_class(&self, uri: &McURI, class_name: &McIds) -> Option<DeclareId> {
        self.enum_class_name_to_id
            .get(&(uri.clone(), class_name.clone()))
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

    pub fn add_class(&mut self, uri: &McURI, class_name: &McIds, span: Span) -> DeclareId {
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

        for id in &dcls_id_to_remove {
            self.declare_class_id_to_span.remove(id);
            self.declare_id_to_class_id.remove(id);
        }
        self.span_to_declare_class_id
            .retain(|(uri, _), _| uri != target_uri);

        // 2. Remove class_id for target file, then re-add
        let class_id_to_remove: Vec<DeclareId> = self
            .class_name_to_id
            .iter()
            .filter(|((uri, _name), _cls_id)| uri == target_uri)
            .map(|(_key, cls_id)| *cls_id)
            .collect();

        for clsid in &class_id_to_remove {
            self.class_id_to_span.remove(clsid);
        }
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

/// Sort rows by a key read off the source (a span, or a `(kind, id)` pair) and
/// drop the key, leaving the emitted rows.
///
/// Every table this module emits is a `HashMap`, so without this the product's
/// order would be redrawn per process (build-design §3.7 discipline 4).
fn sorted_rows<K: Ord>(rows: Vec<(K, serde_json::Value)>) -> Vec<serde_json::Value> {
    let mut rows = rows;
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    rows.into_iter().map(|(_, v)| v).collect()
}

/// Content fingerprint of the payload(s) a dedup id stands for.
///
/// A dedup id is read by its consumer as "the data is unchanged, skip the
/// recompute", so it is sound only if it moves whenever that data moves. A
/// hand-picked subset of fields cannot promise that: the id then stays put
/// while a field outside the subset changes, and the consumer keeps the stale
/// payload without an error (CIMP §1 U94 — the token-stream id missed every
/// rename, and the ref-map id missed a rename that left the first entry alone).
/// Hashing the payloads themselves makes the promise structural: the caller
/// names *what* the id covers, and every field of those values is covered
/// because none is enumerated.
///
/// Requires the values to be equal when the data is equal — `serde_json`
/// objects are `BTreeMap`s here (no `preserve_order`), so the serialization
/// this hashes is key-sorted and not a property of the insertion order.
pub fn payload_fingerprint(parts: &[&serde_json::Value]) -> u64 {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for part in parts {
        // `Hash` for `str` appends its own terminator, so two parts cannot be
        // read as one longer part.
        part.to_string().hash(&mut hasher);
    }
    hasher.finish()
}

/// Convert McSemSymbols to JSON for RPC transfer to LSP
pub fn symbol_table_to_json(symbols: &McSemSymbols, uri: &McURI) -> serde_json::Value {
    use serde_json::json;

    // Get local table data
    //
    // §3.7 discipline 4: a product's order must be determined by its input
    // alone. The four tables below are `HashMap`s — their iteration order is
    // redrawn from a per-process seed — and this function *is* a product (the
    // `show lapper` payload and the RPC reply to the extension share it), so
    // that order became the JSON array order: two processes reading one file
    // got two different payloads. Each array is sorted by **span**, which is
    // read off the source, so the order is the file's own reading order and
    // not a property of the draw. The maps themselves stay as they are: they
    // are hot O(1) lookup paths, and only the emitted order is at issue.
    let local = &symbols.local_table;
    let file_id = intern_uri(uri.as_str());
    let local_declares = sorted_rows(
        local
            .name_to_declare_id
            .iter()
            .filter(|((fid, _, _, _), _)| *fid == file_id)
            .map(|((_fid, _k, scope, name), (id, loc))| {
                (
                    (loc.byte_start, loc.byte_end, name.clone()),
                    json!({
                        "kind": "declare",
                        "id": id._raw,
                        "span": [loc.byte_start, loc.byte_end],
                        "scope": scope,
                        "name": name,
                    }),
                )
            })
            .collect::<Vec<_>>(),
    );

    let local_references = sorted_rows(
        local
            .inst_id_to_span
            .iter()
            .map(|(id, span)| {
                (
                    (span.start, span.end),
                    json!({
                        "kind": "reference",
                        "id": id._raw,
                        "span": [span.start, span.end],
                        "declare_id": local.inst_id_to_declare_inst.get(id).map(|d| d._raw),
                    }),
                )
            })
            .collect::<Vec<_>>(),
    );

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
                .map(|((_fid, _k, scope, _name), _)| scope.clone())
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

    // Get global table data for this URI — sorted by span for the same reason
    // the local tables above are (§3.7 discipline 4).
    let gtable = symbols.global_table.lock().ok();
    let uri_str = uri.as_str();
    let global_declares: Vec<serde_json::Value> = sorted_rows(
        gtable
            .as_ref()
            .map(|g| {
                g.class_id_to_span
                    .iter()
                    .filter(|(_id, (file_uri, _))| file_uri.as_str() == uri_str)
                    .map(|(id, (file_uri, span))| {
                        (
                            (span.start, span.end),
                            json!({
                                "id": id._raw,
                                "uri": file_uri,
                                "span": [span.start, span.end],
                            }),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default(),
    );

    let global_references: Vec<serde_json::Value> = sorted_rows(
        gtable
            .as_ref()
            .map(|g| {
                g.declare_class_id_to_span
                    .iter()
                    .filter(|(_id, (file_uri, _))| file_uri.as_str() == uri_str)
                    .map(|(id, (file_uri, span))| {
                        (
                            (span.start, span.end),
                            json!({
                                "id": id._raw,
                                "uri": file_uri,
                                "span": [span.start, span.end],
                            }),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default(),
    );

    // ★ cross_file_targets: deprecated — replaced by ref_def_map.
    // Kept as empty array for backward compatibility with older mcext.

    // ★ §7.6: Build ref_def_map JSON with result_id hash for mcext dedup.
    let ref_def_map_json = symbols.ref_def_map.as_ref().map(|m| {
        // §3.7 discipline 4, the same defect as the tables above: `entries` and
        // `def_to_refs` are `HashMap`s, so both the emitted array order *and*
        // `result_id` — which hashes one entry picked by `.next()`, i.e. an
        // arbitrary one before this sort — were redrawn per process. Sorting by
        // the map key, `(kind, id)`, makes both a function of the input: the id
        // comes from the id registry, so the key does not move with the hash
        // seed.
        let mut entries: Vec<((SymbolKind, u32), &RefDefEntry)> =
            m.entries.iter().map(|(k, e)| (*k, e)).collect();
        entries.sort_by_key(|((kind, id), _)| (*kind as u8, *id));

        // files: index == interned file_id → uri. The legacy per-map `files`
        // array is replaced by the process-global UriTable (§5.5), so rebuild
        // the array up to the highest file_id referenced by this map.
        let max_file_id = m
            .entries
            .values()
            .map(|e| e.def_loc.file_id)
            .chain(m.def_to_refs.keys().map(|(_, fid, _, _)| *fid))
            .max()
            .unwrap_or(0);
        let files: Vec<String> = (0..=max_file_id)
            .map(|fid| crate::semantic::common::uri_of_file_id(fid).to_string())
            .collect();

        // ★ §15.2: Reverse index for find-all-references. The inner list is
        // sorted too: it is pushed in registration order, which this module
        // does not otherwise depend on.
        let mut def_to_refs: Vec<((SymbolKind, u32, u32, u32), &Vec<(SymbolKind, u32)>)> =
            m.def_to_refs.iter().map(|(k, v)| (*k, v)).collect();
        def_to_refs.sort_by_key(|((kind, fid, start, end), _)| (*kind as u8, *fid, *start, *end));

        let mut payload = json!({
            "entries": entries.iter().map(|(_, e)| {
                json!({
                    "ref_kind": e.ref_kind as u8,
                    "ref_id": e.ref_id,
                    "file_id": e.def_loc.file_id,
                    "def_span": [e.def_loc.byte_start, e.def_loc.byte_end],
                    "def_kind": e.def_kind as u8,
                    "container_id": e.def_loc.container_id,
                    "cmie_kind": e.cmie_kind,
                    "def_name": e.def_name,
                })
            }).collect::<Vec<_>>(),
            "files": files,
            "containers": &m.containers,
            "kind_names": (0u8..=29).map(|i| {
                let kind: crate::ast::sem::SymbolKind = unsafe { std::mem::transmute(i) };
                kind.kind_name()
            }).collect::<Vec<_>>(),
            "def_to_refs": def_to_refs.iter().map(|((dk, fid, bs, be), refs)| {
                let mut refs: Vec<(u8, u32)> =
                    refs.iter().map(|(rk, rid)| (*rk as u8, *rid)).collect();
                refs.sort_unstable();
                json!({
                    "def_kind": *dk as u8,
                    "file_id": *fid,
                    "byte_start": *bs,
                    "byte_end": *be,
                    "refs": refs.iter().map(|(rk, rid)| json!([rk, rid])).collect::<Vec<_>>(),
                })
            }).collect::<Vec<_>>(),
        });

        // ★ §7.6 / U94: the id is the fingerprint of the map it is sent with,
        // so every field above is covered by construction. It used to hash
        // `entries.len()` / `containers.len()` / `name_index.len()` plus four
        // fields of the first entry, which left `def_name`, `ref_id`,
        // `def_kind`, `container_id`, `cmie_kind` and every entry after the
        // first outside the id: renaming a symbol could leave it fixed.
        payload["result_id"] = json!(payload_fingerprint(&[&payload]));
        payload
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
