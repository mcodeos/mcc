// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::ast::ast_node::McValueFFI;
use crate::ast::ast_token::McSemTokenFFI;

// #[link(name = "mcast", kind = "static")]
extern "C" {
    pub fn mcc_reset(log_flags: libc::c_uchar);
    pub fn mcc_load(file: *mut i8) -> *mut i8;
    pub fn mcc_load_from_string(content: *const i8, len: usize) -> *mut i8;
    pub fn mcc_lex(data: *mut i8);
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
    pub fn mc_log_init(log_file: *const libc::c_char);
    pub fn mc_log_close();
}
