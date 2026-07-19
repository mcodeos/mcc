// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! System library management API -- PR-4a
//!
//! Generalize `mcb_init_system_lib()` hardcoded mcode logic into "load any library by name".
//!
//! ## Core API
//!
//! - [`mcb_load_lib`]: load a system library into `mcc_blibs`
//! - [`mcb_unload_lib`]: unload from memory (no disk deletion)
//! - [`mcb_loaded_libs`]: list currently loaded system libraries
//! - [`mcb_lib_info`]: query definitions contained in a library
//!
//! ## Compatibility with old API
//!
//! `mcb_init_system_lib()` preserved, internally changed to call `mcb_load_lib("mcode", mcode_dir)`.

use crate::db::cmie::tables as workspace;
use crate::db::infra::global;
use crate::db::infra::mc_code::McCode;
use crate::{McIds, McSpaceName};
use dashmap::DashMap;
use lazy_static::lazy_static;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info, warn};

// ── System library source cache ──
lazy_static! {
    pub(crate) static ref mcc_blibs: DashMap<String, McCode> = DashMap::new();
}

/// System library basic info (snapshot from mcc_blibs).
#[derive(Debug, Clone)]
pub struct LibInfo {
    pub name: String,
    pub root: String,
    pub module_count: usize,
    pub component_count: usize,
    pub interface_count: usize,
    pub enum_count: usize,
    pub total_symbols: usize,
    pub modules: Vec<String>,
    pub components: Vec<String>,
    pub interfaces: Vec<String>,
    pub enums: Vec<String>,
}

/// Load a system library into memory.
///
/// `name`: library name (e.g., "mcode", "infineon")
/// `root`: library root directory, should contain `<name>.mc` as entry file
///
/// Process:
/// 1. Find `<root>/<name>.mc` entry file
/// 2. Pre-insert empty blib entry (avoid circular lookup issues)
/// 3. `mcb_add_recursive` load entry and all dependencies (is_system=true)
/// 4. Collect all definitions belonging to this library from workspace tables, register to blib's spacenames
///
/// Returns `true` if load succeeded.
pub fn mcb_load_lib(name: &str, root: &Path) -> bool {
    let t0 = std::time::Instant::now();
    info!(
        target: "mcc::lib",
        name = name,
        root = ?root,
        "load: start"
    );
    let entry_basename = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let entry_file = root.join(format!("{entry_basename}.mc"));
    if !entry_file.exists() {
        warn!(
            target: "mcc::lib",
            name = name,
            path = ?entry_file,
            "entry file not found"
        );
        return false;
    }

    // If already loaded, check if it has interfaces
    if mcc_blibs.contains_key(name) {
        // Check if it has interfaces spacenames
        if let Some(blib) = mcc_blibs.get(name) {
            let has_interfaces = blib.spacenames.keys().any(|ids| {
                let name = format!("{}", ids);
                name.contains("SPI")
                    || name.contains("I2C")
                    || name.contains("UART")
                    || name.contains("GPIO")
                    || name.contains("ADC")
                    || name.contains("DAC")
            });
            if has_interfaces {
                info!(target: "mcc::lib", name = name, "load: already has interfaces, skip");
                return true;
            }
        }
        // No interfaces found, need to reload
        info!(target: "mcc::lib", name = name, "load: no interfaces found, will reload");
    }

    // Pre-insert empty blib entry (to avoid circular lookup issues)
    mcc_blibs.insert(name.to_string(), McCode::new_empty());

    // Set system lib loading flag
    crate::cli::config::set_system_lib_loading(true);

    // Reset AST visit flag, to avoid visit conflict with user files
    super::mc_code::mcb_reset_ast_visit_flag();

    // Recursively load all dependencies (is_system=true)
    let uri = entry_file.to_string_lossy().to_string();
    let mut loaded = HashSet::new();
    crate::build::loader::mcb_add_recursive(&uri, &mut loaded, true);

    // Clear system lib loading flag
    crate::cli::config::set_system_lib_loading(false);

    debug!(
        target: "mcc::lib",
        name = name,
        files_loaded = loaded.len(),
        "recursive load complete"
    );

    // Collect all definitions belonging to this library from workspace tables, register to blib's spacenames
    let root_str = root.to_string_lossy().to_string();
    let mut lib_entry = McCode::new_empty();

    tracing::trace!(target: "mcc::lib", name = name, root_str = %root_str, "collecting spacenames with prefix");

    // Collect all definitions belonging to this library from workspace tables, register to blib's spacenames
    collect_spacenames_by_prefix(&workspace::WORKSPACE.components, &root_str, &mut lib_entry);
    collect_spacenames_by_prefix(&workspace::WORKSPACE.modules, &root_str, &mut lib_entry);
    collect_spacenames_by_prefix(&workspace::WORKSPACE.interfaces, &root_str, &mut lib_entry);
    collect_spacenames_by_prefix(&workspace::WORKSPACE.enums, &root_str, &mut lib_entry);

    // Collect all definitions belonging to this library from system tables, register to blib's spacenames
    collect_spacenames_by_prefix_global(&global::mcc_components, &root_str, &mut lib_entry);
    collect_spacenames_by_prefix_global(&global::mcc_modules, &root_str, &mut lib_entry);
    collect_spacenames_by_prefix_global(&global::mcc_interfaces, &root_str, &mut lib_entry);
    collect_spacenames_by_prefix_global(&global::mcc_enums, &root_str, &mut lib_entry);

    let symbol_count = lib_entry.spacenames.len();

    // Replace blib with new one
    mcc_blibs.insert(name.to_string(), lib_entry);

    info!(
        target: "mcc::lib",
        name = name,
        symbols = symbol_count,
        files_loaded = loaded.len(),
        elapsed_ms = t0.elapsed().as_millis() as u64,
        "loaded"
    );
    true
}

/// Unload system library from memory. Do not delete disk files.
///
/// 1. Remove entry from `mcc_blibs`
/// 2. Remove definitions from `mcc_*` system tables with uri containing library path
/// 3. Remove definitions from workspace tables with uri containing library path
pub fn mcb_unload_lib(name: &str) -> bool {
    let blib = match mcc_blibs.remove(name) {
        Some((_, blib)) => blib,
        None => return false,
    };

    // Collect all uri prefixes in this library
    let uris: HashSet<String> = blib.spacenames.values().map(|sn| sn.uri.clone()).collect();

    // Remove all definitions with this uri prefixes in system tables and workspace tables
    remove_by_uris(&global::mcc_components, &uris);
    remove_by_uris(&global::mcc_modules, &uris);
    remove_by_uris(&global::mcc_interfaces, &uris);
    remove_by_uris(&global::mcc_enums, &uris);
    remove_by_uris(&workspace::WORKSPACE.components, &uris);
    remove_by_uris(&workspace::WORKSPACE.modules, &uris);
    remove_by_uris(&workspace::WORKSPACE.interfaces, &uris);
    remove_by_uris(&workspace::WORKSPACE.enums, &uris);

    info!(target: "mcc::lib", name = name, "unloaded");
    true
}

/// List all loaded system libraries in memory.
pub fn mcb_loaded_libs() -> Vec<String> {
    mcc_blibs.iter().map(|e| e.key().clone()).collect()
}

fn format_mc_ids(ids: &McIds) -> String {
    format!("{ids}")
}

/// Get system library information by name.
pub fn mcb_lib_info(name: &str) -> Option<LibInfo> {
    let blib = mcc_blibs.get(name)?;
    let sn = &blib.spacenames;

    let mut module_count = 0usize;
    let mut component_count = 0usize;
    let mut interface_count = 0usize;
    let mut enum_count = 0usize;

    let mut modules_list = Vec::new();
    let mut components_list = Vec::new();
    let mut interfaces_list = Vec::new();
    let mut enums_list = Vec::new();

    for (_, space_name) in sn.iter() {
        if workspace::WORKSPACE.modules.contains_key(space_name)
            || global::mcc_modules.contains_key(space_name)
        {
            module_count += 1;
            modules_list.push(format_mc_ids(&space_name.ident));
        } else if workspace::WORKSPACE.components.contains_key(space_name)
            || global::mcc_components.contains_key(space_name)
        {
            component_count += 1;
            components_list.push(format_mc_ids(&space_name.ident));
        } else if workspace::WORKSPACE.interfaces.contains_key(space_name)
            || global::mcc_interfaces.contains_key(space_name)
        {
            interface_count += 1;
            interfaces_list.push(format_mc_ids(&space_name.ident));
        } else if workspace::WORKSPACE.enums.contains_key(space_name)
            || global::mcc_enums.contains_key(space_name)
        {
            enum_count += 1;
            enums_list.push(format_mc_ids(&space_name.ident));
        }
    }

    modules_list.sort();
    components_list.sort();
    interfaces_list.sort();
    enums_list.sort();

    Some(LibInfo {
        name: name.to_string(),
        root: String::new(),
        module_count,
        component_count,
        interface_count,
        enum_count,
        total_symbols: sn.len(),
        modules: modules_list,
        components: components_list,
        interfaces: interfaces_list,
        enums: enums_list,
    })
}

// ============================================================================
// Internal helper functions
// ============================================================================

fn collect_spacenames_by_prefix<T>(
    table: &DashMap<McSpaceName, Arc<T>>,
    prefix: &str,
    lib_entry: &mut McCode,
) {
    for entry in table.iter() {
        if entry.key().uri.contains(prefix) {
            lib_entry
                .spacenames
                .insert(entry.key().ident.clone(), entry.key().clone());
        }
    }
}

fn collect_spacenames_by_prefix_global<T>(
    table: &DashMap<McSpaceName, Arc<T>>,
    prefix: &str,
    lib_entry: &mut McCode,
) {
    for entry in table.iter() {
        let uri = &entry.key().uri;
        if uri.contains(prefix) {
            lib_entry
                .spacenames
                .insert(entry.key().ident.clone(), entry.key().clone());
        }
    }
}

fn remove_by_uris<T>(table: &DashMap<McSpaceName, Arc<T>>, uris: &HashSet<String>) {
    let to_remove: Vec<McSpaceName> = table
        .iter()
        .filter(|e| uris.contains(&e.key().uri))
        .map(|e| e.key().clone())
        .collect();
    for key in to_remove {
        table.remove(&key);
    }
}
