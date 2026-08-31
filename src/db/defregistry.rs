// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Definition-layer write side (design defspace §9 Phase A / §4).
//!
//! [`insert`] / [`remove_by_uri`] / [`remove_by_uris`] are the single write
//! entry for the definition tables. Phase 2 keeps the physical two-table
//! layout where it is (workspace + process-global) — only this module knows
//! both, and callers no longer branch on `mcbase` themselves. The
//! [`LoadDomain`] tag is the finalized domain semantics
//! (`Project | SystemLib(name)`) that Phase 3's single table and Phase 5's
//! per-world loading build on.
//!
//! Routing is faithful to the pre-refactor behavior:
//! - Module defs always land in the workspace module table — module parsing
//!   runs over `WORKSPACE.mcodes` regardless of the source domain, and nothing
//!   ever inserts into the process-global module table.
//! - Component / Interface / Enum / Define defs land in the workspace table
//!   for `Project` and in the process-global table for `SystemLib(_)`.

use crate::db::cmie::tables as workspace;
use crate::db::infra::global;
use crate::semantic::component::McComponent;
use crate::semantic::mc_define::McDefineDef;
use crate::semantic::mc_enum::McEnumDef;
use crate::semantic::mc_ifs::McInterface;
use crate::semantic::module::McModule;
use crate::McSpaceName;
use dashmap::DashMap;
use std::collections::HashSet;
use std::hash::Hash;
use std::sync::Arc;

/// Which world a definition belongs to (design §4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadDomain {
    /// Project / user files.
    Project,
    /// A system library, named (`mcode` today).
    SystemLib(String),
}

/// Tagged definition value: one [`insert`] writes any of the five tables.
pub enum DefValue {
    Component(Arc<McComponent>),
    Module(Arc<McModule>),
    Interface(Arc<McInterface>),
    Enum(Arc<McEnumDef>),
    Define(Arc<McDefineDef>),
}

/// Outcome of an [`insert`]: the caller turns a duplicate into the matching
/// duplicate diagnostic at the declaration node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertOutcome {
    Inserted,
    Duplicate,
}

/// Insert one definition. CMIE kinds (component/interface/enum/define) treat
/// an occupied key as a duplicate (the previous value stays); the module kind
/// **overwrites** — module parsing runs as a re-derive across parse rounds and
/// replaces this file's prior entry instead of firing a spurious DUP_MODULE
/// (the file-local duplicate check lives in `parse_pass1_modules`).
pub fn insert(sn: &McSpaceName, domain: LoadDomain, def: DefValue) -> InsertOutcome {
    match def {
        DefValue::Component(def) => insert_routed(
            &workspace::WORKSPACE.components,
            &global::mcc_components,
            sn,
            &domain,
            def,
        ),
        DefValue::Module(def) => {
            workspace::WORKSPACE.modules.insert(sn.clone(), def);
            InsertOutcome::Inserted
        }
        DefValue::Interface(def) => insert_routed(
            &workspace::WORKSPACE.interfaces,
            &global::mcc_interfaces,
            sn,
            &domain,
            def,
        ),
        DefValue::Enum(def) => insert_routed(
            &workspace::WORKSPACE.enums,
            &global::mcc_enums,
            sn,
            &domain,
            def,
        ),
        DefValue::Define(def) => insert_routed(
            &workspace::WORKSPACE.defines,
            &global::mcc_defines,
            sn,
            &domain,
            def,
        ),
    }
}

/// Remove every definition of any kind whose defining file matches `uri`,
/// from both physical tables (mirrors the old `remove_defines` sweep).
pub fn remove_by_uri(uri: &str) {
    remove_by_uri_from(&workspace::WORKSPACE.components, uri);
    remove_by_uri_from(&workspace::WORKSPACE.modules, uri);
    remove_by_uri_from(&workspace::WORKSPACE.interfaces, uri);
    remove_by_uri_from(&workspace::WORKSPACE.enums, uri);
    remove_by_uri_from(&workspace::WORKSPACE.defines, uri);
    remove_by_uri_from(&global::mcc_components, uri);
    remove_by_uri_from(&global::mcc_modules, uri);
    remove_by_uri_from(&global::mcc_interfaces, uri);
    remove_by_uri_from(&global::mcc_enums, uri);
    remove_by_uri_from(&global::mcc_defines, uri);
}

/// Remove every definition whose defining file is one of `uris`, from both
/// physical tables (third-party-lib unload sweep).
pub fn remove_by_uris(uris: &HashSet<String>) {
    remove_by_uris_from(&workspace::WORKSPACE.components, uris);
    remove_by_uris_from(&workspace::WORKSPACE.modules, uris);
    remove_by_uris_from(&workspace::WORKSPACE.interfaces, uris);
    remove_by_uris_from(&workspace::WORKSPACE.enums, uris);
    remove_by_uris_from(&workspace::WORKSPACE.defines, uris);
    remove_by_uris_from(&global::mcc_components, uris);
    remove_by_uris_from(&global::mcc_modules, uris);
    remove_by_uris_from(&global::mcc_interfaces, uris);
    remove_by_uris_from(&global::mcc_enums, uris);
    remove_by_uris_from(&global::mcc_defines, uris);
}

/// Route a non-module kind by domain: project → workspace table, system lib →
/// process-global table.
fn insert_routed<T>(
    ws: &DashMap<McSpaceName, Arc<T>>,
    global_table: &DashMap<McSpaceName, Arc<T>>,
    sn: &McSpaceName,
    domain: &LoadDomain,
    def: Arc<T>,
) -> InsertOutcome {
    match domain {
        LoadDomain::Project => insert_one(ws, sn.clone(), def),
        LoadDomain::SystemLib(_) => insert_one(global_table, sn.clone(), def),
    }
}

fn insert_one<K, V>(table: &DashMap<K, V>, key: K, value: V) -> InsertOutcome
where
    K: Eq + Hash + Clone,
{
    match table.entry(key) {
        dashmap::Entry::Occupied(_) => InsertOutcome::Duplicate,
        dashmap::Entry::Vacant(vacant) => {
            vacant.insert(value);
            InsertOutcome::Inserted
        }
    }
}

fn remove_by_uri_from<T>(table: &DashMap<McSpaceName, Arc<T>>, uri: &str) {
    let to_remove: Vec<McSpaceName> = table
        .iter()
        .filter(|e| e.key().uri == uri)
        .map(|e| e.key().clone())
        .collect();
    for key in to_remove {
        table.remove(&key);
    }
}

fn remove_by_uris_from<T>(table: &DashMap<McSpaceName, Arc<T>>, uris: &HashSet<String>) {
    let to_remove: Vec<McSpaceName> = table
        .iter()
        .filter(|e| uris.contains(e.key().uri.as_uri().as_ref()))
        .map(|e| e.key().clone())
        .collect();
    for key in to_remove {
        table.remove(&key);
    }
}
