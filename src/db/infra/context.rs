// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Infra context: current URI tracking.
//!
//! Originally `builder/current_uri.rs`. Roots and parsing state remain in `db/infra/global.rs`.

use crate::McURI;
use std::cell::RefCell;

// ============================================================================
// Current URI (thread-local)
// ============================================================================

thread_local! {
    static CURRENT_URI: RefCell<Option<McURI>> = const { RefCell::new(None) };
}

pub(crate) fn get() -> McURI {
    CURRENT_URI.with(|cell| cell.borrow().clone().expect("Current URI is empty."))
}

/// Safe accessor that returns None instead of panicking
/// when current_uri has not been set yet.
pub(crate) fn try_get() -> Option<McURI> {
    CURRENT_URI.with(|cell| cell.borrow().clone())
}

pub(crate) fn set(uri: &McURI) {
    CURRENT_URI.with(|cell| *cell.borrow_mut() = Some(uri.clone()));
}

pub(crate) fn reset() {
    CURRENT_URI.with(|cell| *cell.borrow_mut() = None);
}
