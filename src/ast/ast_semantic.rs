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

#[derive(Clone, Debug)]
pub struct McSemSymbols {
    pub global_table: Arc<Mutex<GlobalSymbolTable>>,
    pub local_table: LocalSymbolTable,
    pub symbol_lapper: SymbolRangeLapper,
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
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolType {
    ClassDefinition(DeclareId),
    DeclareClass(ReferenceId),
    DeclareInstance(DeclareId),
    InstanceReference(ReferenceId),
}
pub type SymbolRangeLapper = Lapper<usize, SymbolType>;

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

    pub declare_inst_to_span: HashMap<DeclareId, Span>,
    pub span_to_declare_inst: HashMap<Span, DeclareId>,
    pub declare_inst_to_inst_ids: HashMap<DeclareId, Vec<ReferenceId>>,

    pub inst_id_to_span: HashMap<ReferenceId, Span>,
    pub span_to_inst_id: HashMap<Span, ReferenceId>,
    pub inst_id_to_declare_inst: HashMap<ReferenceId, DeclareId>,
    //.. pub class_id_reference_list : Vec<((McURI, String), Span)>,
}

impl LocalSymbolTable {
    pub fn new() -> Self {
        LocalSymbolTable {
            declare_inst_id_counter: DeclareId { _raw: 0 },
            inst_id_counter: ReferenceId { _raw: 0 },
            declare_inst_to_span: HashMap::new(),
            span_to_declare_inst: HashMap::new(),
            declare_inst_to_inst_ids: HashMap::new(),
            inst_id_to_span: HashMap::new(),
            span_to_inst_id: HashMap::new(),
            inst_id_to_declare_inst: HashMap::new(),
        }
    }
    pub fn assign_declare_id(&mut self) -> DeclareId {
        let did = self.declare_inst_id_counter;
        self.declare_inst_id_counter += 1;
        did
    }
    pub fn assign_inst_id(&mut self) -> ReferenceId {
        let rid = self.inst_id_counter;
        self.inst_id_counter += 1;
        rid
    }

    pub fn add_declare(&mut self, span: Span) -> DeclareId {
        let declare_id = self.assign_declare_id();
        self.declare_inst_to_span.insert(declare_id, span.clone());
        self.span_to_declare_inst.insert(span, declare_id);
        declare_id
    }

    pub fn add_inst(&mut self, span: Span, declr_id: DeclareId) {
        let inst_id = self.assign_inst_id();
        //1. Register inst_id
        self.inst_id_to_span.insert(inst_id, span.clone());
        //2. Record inst_id position
        self.span_to_inst_id.insert(span, inst_id);
        //3. Add inst_id -> declare_id record
        self.declare_inst_to_inst_ids
            .entry(declr_id)
            .or_default()
            .push(inst_id);
        //4. Add inst_id -> declare_id record
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
    pub span_to_class_id: HashMap<(McURI, Span), DeclareId>, // Find class ID from position in source code
    pub class_id_to_reference_ids: HashMap<DeclareId, Vec<ReferenceId>>, // Find reference IDs for a class ID

    pub declare_class_id_to_span: HashMap<ReferenceId, (McURI, Span)>, // Find reference ID position in source code
    pub span_to_declare_class_id: HashMap<(McURI, Span), ReferenceId>, //
    pub declare_id_to_class_id: HashMap<ReferenceId, DeclareId>,       //
}

impl GlobalSymbolTable {
    pub fn new() -> Self {
        GlobalSymbolTable {
            class_id_counter: DeclareId { _raw: 0 },
            declare_class_id_counter: ReferenceId { _raw: 0 },

            class_name_to_id: HashMap::new(),
            class_id_to_span: HashMap::new(),
            span_to_class_id: HashMap::new(),
            class_id_to_reference_ids: HashMap::new(),
            declare_class_id_to_span: HashMap::new(),
            span_to_declare_class_id: HashMap::new(),
            declare_id_to_class_id: HashMap::new(),
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

    pub fn add_class(&mut self, uri: &McURI, class_name: &String, span: Span) -> DeclareId {
        let cls_id = self.assign_class_id();
        self.class_name_to_id
            .insert((uri.clone(), class_name.clone()), cls_id);
        self.class_id_to_span
            .insert(cls_id, (uri.clone(), span.clone()));
        self.span_to_class_id
            .insert((uri.clone(), span.clone()), cls_id);
        cls_id
    }

    pub fn add_declare_class(&mut self, uri: &McURI, span: Span, class_id: DeclareId) {
        let reference_id = self.assign_declare_class_id();
        //1. Register reference_id
        self.declare_class_id_to_span
            .insert(reference_id, (uri.clone(), span.clone()));
        //2. Record reference_id position
        self.span_to_declare_class_id
            .insert((uri.clone(), span.clone()), reference_id);
        //3. Add reference_id record
        self.class_id_to_reference_ids
            .entry(class_id)
            .or_default()
            .push(reference_id);
        //4. reference_id -> class_id
        self.declare_id_to_class_id.insert(reference_id, class_id);
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
            self.class_id_to_reference_ids.remove(clsid);
            // Vec<ReferenceId> in class_id_to_reference_ids is auto-removed
        });
        self.span_to_class_id
            .retain(|(uri, _), _| uri != target_uri);
        self.class_name_to_id
            .retain(|(uri, _), _| uri != target_uri);
    }

    pub fn clear(&mut self) {
        *self = GlobalSymbolTable {
            class_id_counter: DeclareId { _raw: 0 },
            declare_class_id_counter: ReferenceId { _raw: 0 },

            class_name_to_id: HashMap::new(),
            class_id_to_span: HashMap::new(),
            span_to_class_id: HashMap::new(),
            class_id_to_reference_ids: HashMap::new(),
            declare_class_id_to_span: HashMap::new(),
            span_to_declare_class_id: HashMap::new(),
            declare_id_to_class_id: HashMap::new(),
        };
    }
}
