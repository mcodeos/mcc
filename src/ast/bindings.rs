// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::ast::node::McValueFFI;
use crate::ast::token::{McLexTokenFFI, McSemTokenFFI};
use std::sync::{Mutex, MutexGuard};

// FFI binding for C struct mc_dlog_entry
#[repr(C)]
pub struct McDlogEntryFFI {
    pub code: u32,
    pub level: i32,
    pub pos: u32,
    pub len: u32,
    pub msg: *const libc::c_char,
    pub next: *mut McDlogEntryFFI,
}

/// The C parser ABI, declared in full.
///
/// The full C parser ABI is kept intact even where Rust doesn't call every
/// entry today (visit/error-token/dlog helpers); removing declarations would
/// silently break the mcast ABI contract.
///
/// Nothing outside this module may call the entries that touch the frontend's
/// process-wide state (see [`Frontend`]); they are reachable only as methods
/// there, which is what stops a new call site from forgetting the lock.
#[allow(dead_code)]
mod raw {
    use super::{McDlogEntryFFI, McLexTokenFFI, McSemTokenFFI, McValueFFI};

    extern "C" {
        pub fn mcc_reset(log_flags: libc::c_uchar);
        pub fn mcc_load(file: *mut i8) -> *mut i8;
        pub fn mcc_load_from_string(content: *const i8, len: usize) -> *mut i8;
        pub fn mcc_lex(data: *mut i8);
        pub fn mcc_set_lex_file(fname: *const libc::c_char);
        pub fn mcc_parse() -> *mut McValueFFI;
        pub fn mcc_free(ast: *mut McValueFFI);
        pub fn mcc_visit(ast: *mut McValueFFI);
        pub fn mcc_visit_tree(ast: *mut McValueFFI);
        pub fn mcc_visit_tree_color(ast: *mut McValueFFI);
        pub fn mcc_visit_set_mode(mode: libc::c_int);
        pub fn mcc_visit_get_mode() -> libc::c_int;
        pub fn mcc_get_sem_tokens() -> *mut McSemTokenFFI;
        pub fn mcc_get_tokens() -> *mut McLexTokenFFI;
        pub fn mc_sem_token_free();
        pub fn mcc_get_error_tokens() -> *mut McSemTokenFFI;
        pub fn mcc_clear_error_tokens();
        pub fn mcc_get_dlog_entries() -> *mut McDlogEntryFFI;
        pub fn mcc_clear_dlog_entries();
        pub fn mc_log_init(log_file: *const libc::c_char);
        pub fn mc_log_close();
    }
}

// Entries with no frontend state of their own: they read the source into a
// fresh buffer, or free a tree that the caller owns. Safe to call at any time.
pub use raw::{mc_log_close, mc_log_init, mcc_free, mcc_load, mcc_load_from_string};

/// The one lock the C frontend is entitled to.
///
/// Not a re-entrant lock, and never taken twice on one thread: every session
/// opens with `reset`, and the methods below are called in a straight line
/// within one function.
static FRONTEND: Mutex<()> = Mutex::new(());

/// One turn at the C frontend.
///
/// The lexer and parser keep their working state in process-wide variables in
/// `mcast` -- `g_token_head` (the list the lexer builds), `g_current_token`
/// (the cursor the parser walks), `g_last_token` (the token an error message
/// names), `g_lex_file`, and the parser's own `mca_debug` flag. They are not
/// per-call state: `mcc_lex` fills them, `mcc_parse` walks them, and the
/// `mcc_get_*` readers read them back. Two sessions that overlap therefore
/// corrupt each other -- the second one walks a cursor the first has already
/// freed, which is a wild pointer dereference, not a wrong answer.
///
/// Holding a `Frontend` is what makes such a call legal, so the lock travels
/// with the right to call rather than being a rule someone has to remember.
///
/// Cost is one serialized parse per file. That is the price of the C side's
/// design, and it is the same cost the single-threaded CLI already pays.
pub struct Frontend {
    /// Released when the session ends. A session that panics drops it too,
    /// which is why the lock is never left held.
    _guard: MutexGuard<'static, ()>,
}

impl Frontend {
    pub fn acquire() -> Self {
        // A poisoned lock only means some earlier session panicked mid-parse.
        // There is no Rust-side invariant to restore: every session opens with
        // `reset`, which frees the token list and clears the cursor. Taking the
        // lock back is what keeps one panicking test from wedging the binary.
        let guard = match FRONTEND.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        Frontend { _guard: guard }
    }

    /// Drop the previous session's tokens and dlog entries.
    pub fn reset(&self, log_flags: libc::c_uchar) {
        unsafe { raw::mcc_reset(log_flags) }
    }

    /// Name the file the lexer is about to read, for its debug output.
    pub fn set_lex_file(&self, fname: *const libc::c_char) {
        unsafe { raw::mcc_set_lex_file(fname) }
    }

    /// Build the token list for `data`, which `load`/`load_from_string` gave.
    ///
    /// # Safety
    /// `data` must be the null-terminated buffer those two returned, still
    /// owned by the caller.
    pub unsafe fn lex(&self, data: *mut i8) {
        raw::mcc_lex(data)
    }

    /// Walk the token list `lex` built and return the AST it describes.
    pub fn parse(&self) -> *mut McValueFFI {
        unsafe { raw::mcc_parse() }
    }

    /// # Safety
    /// `ast` must be the tree `parse` returned and not yet freed.
    pub unsafe fn visit_tree_color(&self, ast: *mut McValueFFI) {
        raw::mcc_visit_tree_color(ast)
    }

    pub fn get_error_tokens(&self) -> *mut McSemTokenFFI {
        unsafe { raw::mcc_get_error_tokens() }
    }

    pub fn get_dlog_entries(&self) -> *mut McDlogEntryFFI {
        unsafe { raw::mcc_get_dlog_entries() }
    }

    pub fn get_tokens(&self) -> *mut McLexTokenFFI {
        unsafe { raw::mcc_get_tokens() }
    }

    pub fn get_sem_tokens(&self) -> *mut McSemTokenFFI {
        unsafe { raw::mcc_get_sem_tokens() }
    }
}
