// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::ast::node::McValueFFI;
use crate::ast::token::McSemTokenFFI;

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

// #[link(name = "mcast", kind = "static")]
// The full C parser ABI is kept intact even where Rust doesn't call every
// entry today (visit/error-token/dlog helpers); removing declarations would
// silently break the mcast ABI contract.
#[allow(dead_code)]
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
    pub fn mc_sem_token_free();
    pub fn mcc_get_error_tokens() -> *mut McSemTokenFFI;
    pub fn mcc_clear_error_tokens();
    pub fn mcc_get_dlog_entries() -> *mut McDlogEntryFFI;
    pub fn mcc_clear_dlog_entries();
    pub fn mc_log_init(log_file: *const libc::c_char);
    pub fn mc_log_close();
}
