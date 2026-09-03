// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Shared bootstrap for mcc integration tests (`tests/*.rs`).
//!
//! Every `tests/*.rs` is its own test binary/process, so a `static` mutex here
//! only serializes tests within one file — it never couples files. This module
//! centralizes what the files used to re-implement individually:
//!
//! 1. **lock** — the file-internal serialization mutex (poison-tolerant).
//! 2. **init family** — the workspace bootstrap variants:
//!    [`init_no_lib`] (`mcc_init_no_lib`, no system library), [`init`] (full),
//!    [`clear`] (workspace state wipe between builds) and [`reset`] (the former
//!    `reset_workspace()` triple).
//! 3. **load** — [`load_string`] loads `.mc` from a virtual URI string.
//!
//! Per-file "build → probe depth" helpers (`codes`, `net_store`, `DianLu`,
//! flat `InstTable`, ...) stay in the owning test file; they only delegate
//! their bootstrap lines here. Files that intentionally keep global state
//! across tests (reset-hostile) or set a custom system root keep that logic in
//! place and adopt only [`lock`].

#![allow(dead_code)]

use std::sync::{Mutex, MutexGuard, OnceLock};

static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Acquire the file-internal serialization lock.
///
/// Poison-tolerant: if a previous test in this file panicked while holding the
/// lock, later tests must not deadlock, so a poisoned lock is recovered via
/// `into_inner`. (Behavior relaxation applies only after a same-file panic.)
pub fn lock() -> MutexGuard<'static, ()> {
    TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Initialize without loading the system library and resolve the system root
/// once (`mcc_init_no_lib` + `mcc_set_system_root("")`).
pub fn init_no_lib() {
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
}

/// Full initialize: load the system library and resolve the system root once
/// (`mcc_init` + `mcc_set_system_root("")`).
pub fn init() {
    mcc::mcc_init();
    mcc::mcc_set_system_root(std::path::Path::new(""));
}

/// Clear the active workspace state (test isolation between builds).
pub fn clear() {
    mcc::mcc_clear_workspace();
}

/// The standard fresh-workspace reset: init without the system library, then
/// clear. Equivalent to the `reset_workspace()` triple the files used to
/// define. The caller must hold the [`lock`].
pub fn reset() {
    init_no_lib();
    clear();
}

/// Load `.mc` source under a virtual URI string into the active workspace and
/// parse all modules. The caller must hold the [`lock`].
pub fn load_string(uri: &str, src: &str) {
    mcc::mcc_load_from_string(&uri.to_string(), src);
}
