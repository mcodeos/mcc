// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Infra context: current URI tracking.
//!
//! Originally `builder/current_uri.rs`. Roots and parsing state remain in `db/infra/global.rs`.

use crate::McURI;
use line_index::LineIndex;
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

// ============================================================================
// Current parsing LineIndex (thread-local stack)
// ============================================================================
//
// When `mcb_parse_all_modules` removes a file from `mcodes` to parse it
// (see pass1.rs), diagnostic emission (e.g., E2008) may fire during parsing
// and need to convert byte positions to (line, col). Since the file is
// temporarily absent from `mcodes`, we store its `LineIndex` here so
// `Location::new` can fall back to it.
//
// Uses a stack to support nested on-demand parsing.

thread_local! {
    static CURRENT_LINE_INDEX: RefCell<Vec<(McURI, LineIndex)>> = const { RefCell::new(Vec::new()) };
}

/// Push a file's `LineIndex` onto the thread-local stack.
/// Call this before parsing a file that has been removed from `mcodes`.
pub(crate) fn push_line_index(uri: McURI, index: LineIndex) {
    CURRENT_LINE_INDEX.with(|cell| cell.borrow_mut().push((uri, index)));
}

/// Pop the most recently pushed `LineIndex` from the stack.
/// Call this after re-inserting the file into `mcodes`.
pub(crate) fn pop_line_index() {
    CURRENT_LINE_INDEX.with(|cell| cell.borrow_mut().pop());
}

/// RAII guard that pushes a `LineIndex` on construction and pops on drop.
/// Prevents manual push/pop pairing bugs.
pub(crate) struct LineIndexGuard;

impl LineIndexGuard {
    pub(crate) fn new(uri: McURI, index: LineIndex) -> Self {
        push_line_index(uri, index);
        Self
    }
}

impl Drop for LineIndexGuard {
    fn drop(&mut self) {
        pop_line_index();
    }
}

/// Look up (line, col) for `pos` in the thread-local line index stack.
/// Searches from most-recently-pushed to oldest. Returns `None` if no
/// matching URI is found.
pub(crate) fn lookup_line_col(uri: &McURI, pos: u32) -> Option<(u32, u32)> {
    CURRENT_LINE_INDEX.with(|cell| {
        let cell = cell.borrow();
        for (stored_uri, index) in cell.iter().rev() {
            if stored_uri == uri {
                let max_pos: u32 = index.len().into();
                if pos > max_pos {
                    return Some((1, 1));
                }
                let line_col = index.line_col(line_index::TextSize::new(pos));
                return Some((line_col.line + 1, line_col.col + 1));
            }
        }
        None
    })
}
