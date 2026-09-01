// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Frozen per-module string net tables (dianlu-tree refactor Phase D,
//! verification item: `McModuleInst.nets` field disappears).
//!
//! The `NetPoint` tables produced by construction-time `build_net_table`
//! (union-find merged nets, ground re-partition) are the projection layer's
//! source data — they feed `InstTable::flatten_nets` and the string-net
//! consumers (ERC / tree JSON / print / export / viz). They never live on
//! `McModuleInst` (the modelling tree); a build freezes them here, keyed by
//! canonical module path (`main`, `main.ldo`, ...), and the projection plus
//! the flat consumers read them from the frozen store. Invariant B: the
//! projection output is byte-identical to the pre-refactor form.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::mc_net::NetPoint;

/// The frozen per-module net tables of one instantiation.
///
/// Key: canonical module path — identical to the path scheme of the flat
/// projection (`flatten_module`'s `my_path`) and the arena-walk consumers.
/// Value: the module's net table (label -> points), already deterministically
/// sorted by `build_net_table`.
#[derive(Debug, Default, Clone)]
pub struct NetTableStore {
    tables: HashMap<String, Vec<(String, Vec<NetPoint>)>>,
}

impl NetTableStore {
    /// Create an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or replace) one module's net table.
    pub fn insert(&mut self, path: impl Into<String>, table: Vec<(String, Vec<NetPoint>)>) {
        self.tables.insert(path.into(), table);
    }

    /// The net table of the module at `path`, if that module was built.
    pub fn get(&self, path: &str) -> Option<&[(String, Vec<NetPoint>)]> {
        self.tables.get(path).map(Vec::as_slice)
    }

    /// Name-sorted snapshot of the module's net table (mirrors the pre-refactor
    /// `McModuleInst::sorted_nets` — consumers that sorted by name keep their
    /// exact order).
    pub fn sorted(&self, path: &str) -> Vec<(&str, &[NetPoint])> {
        let mut nets: Vec<(&str, &[NetPoint])> = self
            .get(path)
            .map(|t| {
                t.iter()
                    .map(|(name, points)| (name.as_str(), points.as_slice()))
                    .collect()
            })
            .unwrap_or_default();
        nets.sort_by(|a, b| a.0.cmp(b.0));
        nets
    }

    /// Whether the store holds no module tables.
    pub fn is_empty(&self) -> bool {
        self.tables.is_empty()
    }

    /// Number of module tables frozen in the store.
    pub fn len(&self) -> usize {
        self.tables.len()
    }

    /// Per-module iteration: (canonical path, net table).
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Vec<(String, Vec<NetPoint>)>)> {
        self.tables.iter()
    }

    /// Move the store into the shared-cell form used during construction
    /// (`InstantiationBuilder` / `DianLu` / `InstTable` hand the same store
    /// down through `Rc<RefCell<...>>`; consumers that built the store
    /// themselves feed it into [`super::insttab::InstTable::from_module_inst`]
    /// via this adapter).
    pub fn into_shared(self) -> Rc<RefCell<NetTableStore>> {
        Rc::new(RefCell::new(self))
    }
}
