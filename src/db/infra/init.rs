// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::build::pass1::canonicalize_project_uri;
use crate::db::cmie::tables as workspace;
use crate::db::infra::global;
use crate::semantic::mc_ifs::McInterface;
use crate::{McSpaceName, McURI};
use std::path::{Path, PathBuf};
use std::sync::Arc;

// === pub(crate) fn mcb_test_parse_lock() -> std::sync::MutexGuard<'static, ()> { ===
/// Serialize tests that drive the C parser / workspace tables. The parser
/// keeps token/error state in process-global buffers, so it is not
/// re-entrant across threads — every test that calls `mcc_load_from_string`,
/// `mcc_load_project`, or clears the workspace must hold this same lock.
/// [`crate::db::infra::mc_code::tests`]'s `PARSE_LOCK` and the
/// `rpc::handlers::buildcmd::tests`' local lock both funnel through here.
#[cfg(test)]
pub(crate) static MCC_TEST_PARSE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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

// === pub fn uri_equivalent(key_uri: &str, uri: &str, canonical_uri: &str) -> bool { ===
/// Unified URI equivalence test (consistency-convergence.md §2.1).
///
/// `key_uri` (a workspace key, canonical or raw) is compared against `uri`
/// (a caller-supplied path) with: exact match, then bidirectional suffix
/// match, then the same bidirectional test against `canonical_uri` (the
/// pre-canonicalized form of `uri`). This is the single URI-comparison
/// entry point; it replaces the scattered hand-written `ends_with` chains in
/// loader / pass2 / lookup / iterators that each re-derived the canonical
/// path locally.
pub fn uri_equivalent(key_uri: &str, uri: &str, canonical_uri: &str) -> bool {
    key_uri == uri
        || key_uri.ends_with(uri)
        || uri.ends_with(key_uri)
        || key_uri.ends_with(canonical_uri)
        || canonical_uri.ends_with(key_uri)
}

// === pub fn interface_lookup(space: &McSpaceName) -> Option<Arc<McInterface>> { ===
/// Look up an interface across the workspace and global (system library)
/// tables (consistency-convergence.md §2.2). This is the single access point;
/// it replaces the scattered
/// `WORKSPACE.interfaces.get(..).or_else(|| global::mcc_interfaces.get(..))`
/// chains that each hand-merged the two tables.
pub fn interface_lookup(space: &McSpaceName) -> Option<Arc<McInterface>> {
    workspace::WORKSPACE
        .interfaces
        .get(space)
        .map(|r| r.clone())
        .or_else(|| global::mcc_interfaces.get(space).map(|r| r.clone()))
}

// === pub fn iter_interfaces() -> Vec<(McSpaceName, Arc<McInterface>)> { ===
/// Iterate all interfaces from the workspace and global (system library)
/// tables (consistency-convergence.md §2.2). Replaces the hand-written
/// `WORKSPACE.interfaces.iter().chain(global::mcc_interfaces.iter())` merges.
pub fn iter_interfaces() -> Vec<(McSpaceName, Arc<McInterface>)> {
    workspace::WORKSPACE
        .interfaces
        .iter()
        .chain(global::mcc_interfaces.iter())
        .map(|entry| (entry.key().clone(), entry.value().clone()))
        .collect()
}

// === pub fn mcb_init() { ===
pub fn mcb_init() {
    crate::db::infra::libmgr::clear_state(crate::db::infra::libmgr::ClearScope::Full, None);
    // System library loading is uniformly handled by mcb_init_system_lib()
}

// === pub fn mcb_workspace_clear() { ===
pub fn mcb_workspace_clear() {
    crate::db::infra::libmgr::clear_state(crate::db::infra::libmgr::ClearScope::Active, None);
}
