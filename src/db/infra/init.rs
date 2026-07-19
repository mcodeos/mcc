// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::build::pass1::canonicalize_project_uri;
use crate::db::cmie::tables as workspace;
use crate::db::infra::global;
use crate::McURI;
use std::path::{Path, PathBuf};

// === pub fn mcb_set_system_root(path: &Path) { ===
pub fn mcb_set_system_root(path: &Path) {
    *global::mcc_system_root.lock().unwrap() = path.to_path_buf();
}

// === pub fn mcb_set_project_root(path: &Path) { ===
pub fn mcb_set_project_root(path: &Path) {
    *global::mcc_project_root.lock().unwrap() = path.to_path_buf();
}

// === pub fn mcb_get_system_root() -> PathBuf { ===
pub fn mcb_get_system_root() -> PathBuf {
    global::mcc_system_root.lock().unwrap().clone()
}

// === pub fn mcb_get_project_root() -> PathBuf { ===
pub fn mcb_get_project_root() -> PathBuf {
    global::mcc_project_root.lock().unwrap().clone()
}

// === pub fn mcb_canonicalize_uri(uri: &McURI) -> String { ===
pub fn mcb_canonicalize_uri(uri: &McURI) -> String {
    canonicalize_project_uri(uri)
}

// === pub fn mcb_init() { ===
pub fn mcb_init() {
    crate::db::infra::libmgr::mcc_blibs.clear();
    global::mcc_components.clear();
    global::mcc_modules.clear();
    global::mcc_interfaces.clear();
    global::mcc_enums.clear();

    workspace::WORKSPACE.clear_active();
    // System library loading is uniformly handled by mcb_init_system_lib()
}

// === pub fn mcb_workspace_clear() { ===
pub fn mcb_workspace_clear() {
    workspace::WORKSPACE.clear_active();
}
