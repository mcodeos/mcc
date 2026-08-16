// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Infra context: current URI tracking.
//!
//! Originally `builder/current_uri.rs`. Roots and parsing state remain in `db/infra/global.rs`.

use crate::db::infra::mc_use::McUse;
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

// ============================================================================
// Current parsing file's uselist (thread-local stack)
// ============================================================================
//
// When module parsing runs (`McCode::parse_pass1_modules`), the file being
// parsed is typically removed from `mcodes` first (callers take `&mut` for
// the parse, see `mcb_parse_all_modules` in pass1.rs). P4 use-chain
// resolution (`db/resolve/visibility.rs::use_chain_reaches`) then cannot
// read the file's own `uselist` from `mcodes` to start the walk, so we stash
// it here. Mirrors the `LineIndexGuard` pattern above.

thread_local! {
    static CURRENT_PARSING_USES: RefCell<Vec<(McURI, Vec<McUse>)>> =
        const { RefCell::new(Vec::new()) };
}

/// Push a file's `uselist` onto the thread-local stack.
/// Call this before parsing a file that is not present in `mcodes`.
pub(crate) fn push_parsing_uses(uri: McURI, uselist: Vec<McUse>) {
    CURRENT_PARSING_USES.with(|cell| cell.borrow_mut().push((uri, uselist)));
}

/// Pop the most recently pushed `uselist` from the stack.
pub(crate) fn pop_parsing_uses() {
    CURRENT_PARSING_USES.with(|cell| cell.borrow_mut().pop());
}

/// Look up a file's `uselist` on the thread-local stack, searching from
/// most-recently-pushed to oldest. Returns `None` if the file is not stashed.
pub(crate) fn lookup_parsing_uses(uri: &McURI) -> Option<Vec<McUse>> {
    CURRENT_PARSING_USES.with(|cell| {
        let cell = cell.borrow();
        for (stored_uri, uselist) in cell.iter().rev() {
            if stored_uri == uri {
                return Some(uselist.clone());
            }
        }
        None
    })
}

/// RAII guard that pushes a file's `uselist` on construction and pops on
/// drop. Prevents manual push/pop pairing bugs.
pub(crate) struct ParsingUsesGuard;

impl ParsingUsesGuard {
    pub(crate) fn new(uri: McURI, uselist: Vec<McUse>) -> Self {
        push_parsing_uses(uri, uselist);
        Self
    }
}

impl Drop for ParsingUsesGuard {
    fn drop(&mut self) {
        pop_parsing_uses();
    }
}
