// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.


use crate::builder::*;
use crate::McURI;
use crate::db::infra::global;
use crate::db::cmie::tables as workspace;
use std::path::{Path, PathBuf};
use std::fs;
use tracing::debug;

// === pub fn mcb_set_system_root(path: &Path) { ===
pub fn mcb_set_system_root(path: &Path) {
    *global::mcc_system_root.borrow_mut() = path.to_path_buf();
}

// === pub fn mcb_set_project_root(path: &Path) { ===
pub fn mcb_set_project_root(path: &Path) {
    *global::mcc_project_root.borrow_mut() = path.to_path_buf();
}

// === pub fn mcb_get_system_root() -> PathBuf { ===
pub fn mcb_get_system_root() -> PathBuf {
    global::mcc_system_root.borrow().clone()
}

// === pub fn mcb_get_project_root() -> PathBuf { ===
pub fn mcb_get_project_root() -> PathBuf {
    global::mcc_project_root.borrow().clone()
}

// === pub fn mcb_canonicalize_uri(uri: &McURI) -> String { ===
pub fn mcb_canonicalize_uri(uri: &McURI) -> String {
    canonicalize_project_uri(uri)
}

// === pub fn mcb_init() { ===
pub fn mcb_init() {
    global::mcc_blibs.borrow().clear();
    global::mcc_components.borrow().clear();
    global::mcc_modules.borrow().clear();
    global::mcc_interfaces.borrow().clear();
    global::mcc_enums.borrow().clear();

    workspace::WORKSPACE.clear_active();
    // System library loading is uniformly handled by mcb_init_system_lib()
}

// === pub fn mcb_workspace_clear() { ===
pub fn mcb_workspace_clear() {
    workspace::WORKSPACE.clear_active();
}
