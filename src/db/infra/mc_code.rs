// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::ast::ast_node::McValueFFI;

// ═══════════════════════════════════════════════════════════════════════════
// DedupLapper — wraps SymbolRangeLapper, deduplicates by (kind, start, stop)
// on insert, rejecting entries with the same kind and span regardless of id.
// ═══════════════════════════════════════════════════════════════════════════
struct DedupLapper {
    inner: SymbolRangeLapper,
    seen: HashSet<(u8, usize, usize)>,
}

impl DedupLapper {
    fn new() -> Self {
        Self {
            inner: SymbolRangeLapper::new(vec![]),
            seen: HashSet::new(),
        }
    }

    fn insert(&mut self, interval: Interval<usize, SymbolType>) {
        let key = (interval.val.kind, interval.start, interval.stop);
        if self.seen.insert(key) {
            self.inner.insert(interval);
        }
    }

    fn into_inner(self) -> SymbolRangeLapper {
        self.inner
    }
}
use crate::ast::ast_semantic::{
    DeclareId, McSemSymbols, SourceLocation, Span, SymbolKind, SymbolRangeLapper, SymbolType,
};
use crate::ast::ast_token::McSemTokens;
use crate::ast::error::message::MISSING_SUBNODE;
use crate::db::cmie::tables as workspace;
use crate::db::diagnostic::diagnostic::{dlog_error, dlog_error_at, dlog_warning_at};
use crate::db::infra::global;
use crate::db::infra::mc_use::{McUse, McUsePrefix};
use crate::semantic::mc_enum::McEnumDef;
use crate::semantic::mc_ifs::McInterface;
use crate::{ast::ast_node::AstNode, ast::c_macros::*, semantic::common::McCMIE};
use crate::{current_uri, mcb_loaded_libs, McComponent, McIds, McModule, McSpaceName, McURI};
use core::panic;
use line_index::LineIndex;
use rust_lapper::Interval;
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Global deduplication flag: each parse cycle outputs AST visit only once
/// Reset at the mcc_load_project entry point (mcb_reset_ast_visit_flag)
pub static AST_VISIT_DONE: AtomicBool = AtomicBool::new(false);

pub fn mcb_reset_ast_visit_flag() {
    AST_VISIT_DONE.store(false, Ordering::SeqCst);
}

#[derive(Debug, Clone)]
pub struct McCode {
    pub(crate) mcbase: bool,
    pub(crate) uri: McURI,
    /// Canonical (symlink-resolved) path for reliable file comparison.
    pub(crate) canonical_uri: String,
    pub(crate) ast: AstNode,
    pub(crate) tokens: Arc<Mutex<McSemTokens>>,
    pub(crate) symbols: Arc<Mutex<McSemSymbols>>,
    pub(crate) uselist: Vec<McUse>,
    pub(crate) spacenames: BTreeMap<McIds, McSpaceName>,
    pub(crate) line_index: Option<LineIndex>,
    pub(crate) pass1_complete: bool,
    pub(crate) modules_parsed: bool,
    /// ★ §7.6: Use table needs refresh because a `use`d file changed.
    pub(crate) use_table_dirty: bool,
    /// ★ Cross-file class ref targets cached from create_lapper() for consolidate_ref_def_map().
    /// Replaces GlobalSymbolTable.declare_id_to_target_span (§8.2 removal).
    cross_file_targets: Vec<(
        crate::ast::ast_semantic::DeclareId,
        McURI,
        std::ops::Range<usize>,
    )>,
}

////////////////////////////////
impl McCode {
    pub(crate) fn collect_direct_uses(&self, current_path: &Path) -> Vec<McUse> {
        let mut uses = Vec::new();
        self.ast
            .iter()
            .filter(|x| x.is_type(MCAST_USE) || x.is_type(MCAST_USE_PUB))
            .for_each(|node| {
                if let Some(mc_use) = McUse::new(&node, current_path) {
                    uses.push(mc_use);
                }
            });
        uses
    }

    /// Convert character position to line number and column number
    /// Returns (line, column) where both are 1-based
    pub fn pos_to_line_col(&self, pos: u32) -> (u32, u32) {
        if let Some(line_index) = &self.line_index {
            let max_pos: u32 = line_index.len().into();
            if pos > max_pos {
                return (1, 1);
            }
            let line_col = line_index.line_col(line_index::TextSize::new(pos));
            // Convert from zero-based to one-based
            (line_col.line + 1, line_col.col + 1)
        } else {
            // If we don't have line index, return (1, 1) as fallback
            (1, 1)
        }
    }

    pub fn new(uri: &McURI, base: bool) -> Option<Self> {
        //case1: use (abs / relative) + current path
        //case2: mcode abs
        //case3: mcb_add  abs
        //case4: cmie (name -> abs path)
        if fs::metadata(Path::new(&uri)).is_err() {
            tracing::debug!(target: "mcc::code", uri = %uri, "file not found");
            return None;
        }

        let canonical_uri = std::fs::canonicalize(Path::new(uri))
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| uri.clone());
        Some(McCode {
            mcbase: base,
            uri: uri.clone(),
            canonical_uri,
            ast: AstNode::new(null_mut()),
            tokens: Arc::new(Mutex::new(McSemTokens::new())),
            symbols: Arc::new(Mutex::new(McSemSymbols::new())),
            spacenames: BTreeMap::new(),
            uselist: Vec::new(),
            line_index: None,
            pass1_complete: false,
            modules_parsed: false,
            use_table_dirty: false,
            cross_file_targets: Vec::new(),
        })
    }

    pub fn new_empty() -> Self {
        Self {
            mcbase: false,
            uri: String::new(),
            canonical_uri: String::new(),
            ast: AstNode::new(null_mut()),
            tokens: Arc::new(Mutex::new(McSemTokens::new())),
            symbols: Arc::new(Mutex::new(McSemSymbols::new())),
            spacenames: BTreeMap::new(),
            uselist: Vec::new(),
            line_index: None,
            pass1_complete: false,
            modules_parsed: false,
            use_table_dirty: false,
            cross_file_targets: Vec::new(),
        }
    }

    /// Create McCode from an in-memory string (no disk file dependency)
    pub fn new_from_string(uri: &McURI, content: &str) -> Option<Self> {
        let canonical_uri = std::fs::canonicalize(Path::new(uri))
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| uri.clone());
        Some(McCode {
            mcbase: false,
            uri: uri.clone(),
            canonical_uri,
            ast: AstNode::new(null_mut()),
            tokens: Arc::new(Mutex::new(McSemTokens::new())),
            symbols: Arc::new(Mutex::new(McSemSymbols::new())),
            spacenames: BTreeMap::new(),
            uselist: Vec::new(),
            line_index: Some(LineIndex::new(content)),
            pass1_complete: false,
            modules_parsed: false,
            use_table_dirty: false,
            cross_file_targets: Vec::new(),
        })
    }
    pub fn free(&mut self) {
        if !self.ast.is_null() {
            unsafe {
                crate::ast::c_bindings::mcc_free(self.ast.get_ptr());
            }
        }
        self.ast.set_ptr(null_mut() as *mut McValueFFI);
    }

    pub fn parse_ast(&mut self) {
        current_uri::set(&self.uri);
        crate::db::diagnostic::diagnostic::dlog_clear_file(&self.uri);

        // eprintln!("parse: {:#?}", self.uri);
        let binding = self.uri.clone();
        let fname = Path::new(&binding);

        // First reset, enable trace based on config (must be done before mcc_load)
        let project_root = {
            let meta = workspace::WORKSPACE.active_meta();
            if !meta.id.is_empty() {
                Some(meta.root.clone())
            } else {
                None
            }
        };
        let trace_flag = crate::cli::config::get_trace_flag(project_root.as_deref());
        // Exclude visit bit (0x08) to avoid mcc_parse() internally re-outputting the AST tree
        // visit output is controlled uniformly by Rust side explicitly calling mcc_visit_tree_color()
        let parse_flag = trace_flag & !0x08u8;
        unsafe {
            crate::ast::c_bindings::mcc_reset(parse_flag);
        }

        // Use C mcc_load instead of Rust read_to_string
        // Must use CString to ensure null-terminated string for C
        let c_path = std::ffi::CString::new(binding.clone()).expect("Failed to create CString");
        let fcontent_ptr = unsafe { crate::ast::c_bindings::mcc_load(c_path.as_ptr() as *mut i8) };
        if fcontent_ptr.is_null() {
            tracing::warn!(target: "mcc::code", file = ?fname, "mcc_load failed");
            return;
        }

        // Create line index from the loaded content
        unsafe {
            let fcontent_cstr = std::ffi::CStr::from_ptr(fcontent_ptr as *mut i8);
            if let Ok(fcontent) = fcontent_cstr.to_str() {
                self.line_index = Some(LineIndex::new(fcontent));
            }
        }

        self.free();

        unsafe {
            // Call mcc_reset to ensure complete state cleanup (exclude visit bit, avoid duplicate output)
            crate::ast::c_bindings::mcc_reset(parse_flag);

            // Clear tokens and symbols, ensure no residual data
            if let Ok(mut t) = self.tokens.lock() {
                *t = McSemTokens::new();
            }
            if let Ok(mut s) = self.symbols.lock() {
                *s = McSemSymbols::new();
            }

            // P2-7-XTAL: set file name for lexer debug
            let fname_cstr =
                std::ffi::CString::new(fname.to_string_lossy().as_bytes()).unwrap_or_default();
            crate::ast::c_bindings::mcc_set_lex_file(fname_cstr.as_ptr());
            crate::ast::c_bindings::mcc_lex(fcontent_ptr);

            let ast = AstNode::new(crate::ast::c_bindings::mcc_parse());
            if ast.is_null() {
                tracing::warn!(target: "mcc::code", file = ?fname, "AST parse returned null");
            } else {
                // Output AST visit (if trace.visit is enabled), once per cycle
                // Skip during system library loading, to prevent mcode loading from preempting user file visit quota
                if crate::cli::config::get_trace_visit() == Some(true)
                    && !crate::cli::config::is_system_lib_loading()
                    && !crate::cli::config::is_trace_stdout_suppressed()
                    && !AST_VISIT_DONE.swap(true, Ordering::SeqCst)
                {
                    crate::ast::c_bindings::mcc_visit_tree_color(ast.get_ptr() as *mut McValueFFI);
                }
                self.ast = ast;
            }

            // ★ Fix (Defect 15): Push line_index onto thread-local stack so
            // Location::new can resolve (line, col) for E1000 errors.
            // At this point the file has not yet been inserted into
            // WORKSPACE.mcodes (that happens in loader.rs step 5.5).
            if let Some(ref line_index) = self.line_index {
                crate::db::infra::context::push_line_index(self.uri.clone(), line_index.clone());
            }

            // Collect error tokens from parser and create diagnostics
            {
                let mut err_ptr = crate::ast::c_bindings::mcc_get_error_tokens();
                while !err_ptr.is_null() {
                    let err = &*err_ptr;
                    let pos = err.pos as u32;
                    let len = err.len as u32;
                    let location = crate::db::diagnostic::diagnostic::Location::new(
                        self.uri.clone(),
                        pos,
                        len,
                    );
                    let diagnostic = crate::db::diagnostic::diagnostic::Diagnostic::new(
                        1000, // E1000: parse error
                        crate::db::diagnostic::diagnostic::DiagnosticLevel::Error,
                        location,
                        "syntax error".to_string(),
                    );
                    workspace::WORKSPACE
                        .diagnostics
                        .lock()
                        .unwrap()
                        .add_diagnostic(diagnostic);
                    err_ptr = err.next;
                }
            }

            if self.line_index.is_some() {
                crate::db::infra::context::pop_line_index();
            }

            // Collect structured diagnostics from parser (mc_dlog_add)
            {
                // Gather all entries, resolve messages, dedup by position
                let mut raw: Vec<(u32, i32, u32, u32, String)> = Vec::new();
                let mut dlog_ptr = crate::ast::c_bindings::mcc_get_dlog_entries();
                while !dlog_ptr.is_null() {
                    let entry = &*dlog_ptr;
                    let msg = if entry.msg.is_null() {
                        Self::dlog_parser_message(entry.code).to_string()
                    } else {
                        std::ffi::CStr::from_ptr(entry.msg)
                            .to_string_lossy()
                            .to_string()
                    };
                    raw.push((entry.code, entry.level, entry.pos, entry.len, msg));
                    dlog_ptr = entry.next;
                }
                // Dedup: at overlapping positions, keep the highest code (most specific)
                raw.sort_by_key(|e| (e.2, e.3)); // sort by pos, then len
                let mut last_end: u32 = 0;
                for (code, level, pos, len, msg) in &raw {
                    if *pos < last_end && *code < 1100 {
                        continue; // skip less-specific error at same position
                    }
                    last_end = pos.saturating_add(*len);
                    match level {
                        2 => crate::db::diagnostic::diagnostic::dlog_warning_at(
                            *code, *pos, *len, &msg,
                        ),
                        _ => crate::db::diagnostic::diagnostic::dlog_error_at(
                            *code, *pos, *len, &msg,
                        ),
                    }
                }
            }

            // Free the loaded content
            libc::free(fcontent_ptr as *mut libc::c_void);

            match self.tokens.lock() {
                Ok(mut t) => {
                    // Clear tokens first, then parse new tokens
                    *t = McSemTokens::new();
                    t.parse(crate::ast::c_bindings::mcc_get_sem_tokens())
                }
                Err(e) => {
                    tracing::error!(target: "mcc::code", error = %e, "tokens mutex poisoned");
                }
            }
        }
    }

    pub fn parse_ast_quiet(&mut self) {
        current_uri::set(&self.uri);
        crate::db::diagnostic::diagnostic::dlog_clear_file(&self.uri);

        let binding = self.uri.clone();
        let fname = Path::new(&binding);

        unsafe {
            crate::ast::c_bindings::mcc_reset(0);
        }

        let c_path = std::ffi::CString::new(binding.clone()).expect("Failed to create CString");
        let fcontent_ptr = unsafe { crate::ast::c_bindings::mcc_load(c_path.as_ptr() as *mut i8) };
        if fcontent_ptr.is_null() {
            tracing::warn!(target: "mcc::code", file = ?fname, "mcc_load failed");
            return;
        }

        unsafe {
            crate::ast::c_bindings::mcc_reset(0);

            if let Ok(mut t) = self.tokens.lock() {
                *t = McSemTokens::new();
            }
            if let Ok(mut s) = self.symbols.lock() {
                *s = McSemSymbols::new();
            }

            // P2-7-XTAL: set file name for lexer debug
            let fname_cstr2 =
                std::ffi::CString::new(fname.to_string_lossy().as_bytes()).unwrap_or_default();
            crate::ast::c_bindings::mcc_set_lex_file(fname_cstr2.as_ptr());
            crate::ast::c_bindings::mcc_lex(fcontent_ptr);
            let ast = AstNode::new(crate::ast::c_bindings::mcc_parse());
            if !ast.is_null() {
                self.ast = ast;
            }

            // Collect error tokens from parser and create diagnostics
            {
                let mut err_ptr = crate::ast::c_bindings::mcc_get_error_tokens();
                while !err_ptr.is_null() {
                    let err = &*err_ptr;
                    let pos = err.pos as u32;
                    let len = err.len as u32;
                    let location = crate::db::diagnostic::diagnostic::Location::new(
                        self.uri.clone(),
                        pos,
                        len,
                    );
                    let diagnostic = crate::db::diagnostic::diagnostic::Diagnostic::new(
                        1000, // E1000: parse error
                        crate::db::diagnostic::diagnostic::DiagnosticLevel::Error,
                        location,
                        "syntax error".to_string(),
                    );
                    workspace::WORKSPACE
                        .diagnostics
                        .lock()
                        .unwrap()
                        .add_diagnostic(diagnostic);
                    err_ptr = err.next;
                }
            }

            // Collect structured diagnostics from parser (mc_dlog_add)
            {
                let mut raw: Vec<(u32, i32, u32, u32, String)> = Vec::new();
                let mut dlog_ptr = crate::ast::c_bindings::mcc_get_dlog_entries();
                while !dlog_ptr.is_null() {
                    let entry = &*dlog_ptr;
                    let msg = if entry.msg.is_null() {
                        Self::dlog_parser_message(entry.code).to_string()
                    } else {
                        std::ffi::CStr::from_ptr(entry.msg)
                            .to_string_lossy()
                            .to_string()
                    };
                    raw.push((entry.code, entry.level, entry.pos, entry.len, msg));
                    dlog_ptr = entry.next;
                }
                // Dedup: at overlapping positions, keep the highest code (most specific)
                raw.sort_by_key(|e| (e.2, e.3));
                let mut last_end: u32 = 0;
                for (code, level, pos, len, msg) in &raw {
                    if *pos < last_end && *code < 1100 {
                        continue;
                    }
                    last_end = pos.saturating_add(*len);
                    match level {
                        2 => crate::db::diagnostic::diagnostic::dlog_warning_at(
                            *code, *pos, *len, &msg,
                        ),
                        _ => crate::db::diagnostic::diagnostic::dlog_error_at(
                            *code, *pos, *len, &msg,
                        ),
                    }
                }
            }

            libc::free(fcontent_ptr as *mut libc::c_void);
        }
    }

    /// Extract inline comments from sem tokens that were consumed by ELC
    /// prefix/suffix in the lexer. The lexer's ELC definition includes
    /// SINGLELINE_COMMENT, so comments between operators (e.g. `// comment`)
    /// get consumed as part of the operator token. This function scans each
    /// token's source text for `//` or `#` comment markers, splits off the
    /// comment portion into a separate MCC_TK_COMMENT (type=16) token, and
    /// adjusts the original token's span.
    fn extract_inline_comments(tokens: &mut Vec<crate::ast::ast_token::McSemToken>, content: &str) {
        let content_bytes = content.as_bytes();
        let content_len = content.len();
        let mut new_tokens: Vec<crate::ast::ast_token::McSemToken> = Vec::new();

        for token in tokens.iter() {
            let pos = token.position as usize;
            let len = token.length as usize;

            // Clamp to content boundary
            if pos >= content_len {
                continue;
            }

            // Clamp to char boundary
            let remaining_len = content_len - pos;
            let safe_len = if len <= remaining_len {
                // Check if pos + len is on a char boundary
                let end_pos = pos + len;
                if end_pos <= content_len && !content.is_char_boundary(end_pos) {
                    // Back up to the previous char boundary
                    content[..end_pos]
                        .char_indices()
                        .last()
                        .map(|(i, _)| i)
                        .unwrap_or(len)
                } else {
                    len
                }
            } else {
                remaining_len
            };

            let text = &content[pos..pos + safe_len];
            let text_bytes = text.as_bytes();

            // Find comment markers in the text: // or #
            if let Some(comment_start) = Self::find_comment_start(text, text_bytes) {
                // Check if content BEFORE comment is meaningful (non-whitespace)
                let before_comment = text[..comment_start].trim_end();

                if before_comment.is_empty() {
                    // PREFIX comment: `// comment\n    ->` — meaningful token is AFTER the comment
                    let comment_body = &text[comment_start..];
                    let nl_pos = comment_body.find('\n');
                    if let Some(nl) = nl_pos {
                        // Comment: from // to after newline
                        new_tokens.push(crate::ast::ast_token::McSemToken {
                            type_: 16,
                            position: (pos + comment_start) as i32,
                            length: (nl + 1) as i32,
                        });
                        // Remaining token after the comment's newline
                        let rest_start = pos + comment_start + nl + 1;
                        let rest = &content_bytes[rest_start..pos + len];
                        let trimmed = rest.iter().position(|&b| b != b' ' && b != b'\t');
                        if let Some(ts) = trimmed {
                            // Check if rest after trimming still has content
                            let rest_content = &content[rest_start + ts..pos + len];
                            let actual_len = rest_content.trim_end().len();
                            if actual_len > 0 {
                                new_tokens.push(crate::ast::ast_token::McSemToken {
                                    type_: token.type_,
                                    position: (rest_start + ts) as i32,
                                    length: actual_len as i32,
                                });
                            }
                        }
                    } else {
                        // Entire token is just the comment
                        new_tokens.push(crate::ast::ast_token::McSemToken {
                            type_: 16,
                            position: token.position,
                            length: token.length,
                        });
                    }
                } else {
                    // SUFFIX comment: `,     // inline2` — meaningful token is BEFORE the comment
                    new_tokens.push(crate::ast::ast_token::McSemToken {
                        type_: token.type_,
                        position: token.position,
                        length: before_comment.len() as i32,
                    });
                    // Comment: from // to end of line
                    let comment_src = &text[comment_start..];
                    let comment_end = comment_src.find('\n').map_or(comment_src.len(), |i| i + 1);
                    if comment_end > 0 {
                        new_tokens.push(crate::ast::ast_token::McSemToken {
                            type_: 16,
                            position: (pos + comment_start) as i32,
                            length: comment_end as i32,
                        });
                    }
                }
            } else {
                new_tokens.push(token.clone());
            }
        }

        *tokens = new_tokens;
    }

    /// Find the start of a comment in token text. Returns the byte offset within the
    /// token where `//` or `#` starts, or None if no comment is found.
    fn find_comment_start(text: &str, text_bytes: &[u8]) -> Option<usize> {
        for i in 0..text.len().saturating_sub(1) {
            if text_bytes[i] == b'/' && text_bytes[i + 1] == b'/' {
                // Skip // that is part of a URL (://)
                if i > 0 && text_bytes[i - 1] == b':' {
                    continue;
                }
                return Some(i);
            }
            if text_bytes[i] == b'#' {
                return Some(i);
            }
        }
        None
    }

    /// Parse AST from an in-memory string (no disk file dependency)
    /// Note: the caller must set log flags via `mcc_reset()` before calling
    /// Parse AST from in-memory string.
    /// Mirrors `parse_ast()` exactly, but reads content from memory instead of disk.
    pub fn parse_ast_from_string(&mut self, content: &str) {
        current_uri::set(&self.uri);
        crate::db::diagnostic::diagnostic::dlog_clear_file(&self.uri);

        // ★ mcc_reset BEFORE loading — mirrors parse_ast()'s first reset.
        //   Clears any residual C parser state (g_token_head, etc.) from
        //   a previous parse that ran on the same OS thread.
        unsafe {
            crate::ast::c_bindings::mcc_reset(0);
        }

        // Create line index from the content (mirrors parse_ast)
        self.line_index = Some(LineIndex::new(content));

        let c_content = std::ffi::CString::new(content).expect("Failed to create CString");
        let fcontent_ptr = unsafe {
            crate::ast::c_bindings::mcc_load_from_string(
                c_content.as_ptr() as *const i8,
                content.len(),
            )
        };
        if fcontent_ptr.is_null() {
            tracing::warn!(target: "mcc::code", uri = %self.uri, "mcc_load_from_string failed");
            return;
        }

        self.free();

        unsafe {
            // ★ mcc_reset AFTER loading — mirrors parse_ast()'s second reset
            crate::ast::c_bindings::mcc_reset(0);

            // Clear tokens and symbols, ensure no residual data
            if let Ok(mut t) = self.tokens.lock() {
                *t = McSemTokens::new();
            }
            if let Ok(mut s) = self.symbols.lock() {
                *s = McSemSymbols::new();
            }

            // P2-7-XTAL: set file name for lexer debug
            let uri_cstr = std::ffi::CString::new(self.uri.as_bytes()).unwrap_or_default();
            crate::ast::c_bindings::mcc_set_lex_file(uri_cstr.as_ptr());
            crate::ast::c_bindings::mcc_lex(fcontent_ptr);

            let ast = AstNode::new(crate::ast::c_bindings::mcc_parse());
            if ast.is_null() {
                tracing::warn!(target: "mcc::code", uri = %self.uri, "AST parse returned null");
            } else {
                if crate::cli::config::get_trace_visit() == Some(true)
                    && !crate::cli::config::is_system_lib_loading()
                    && !crate::cli::config::is_trace_stdout_suppressed()
                    && !AST_VISIT_DONE.swap(true, Ordering::SeqCst)
                {
                    crate::ast::c_bindings::mcc_visit_tree_color(ast.get_ptr() as *mut McValueFFI);
                }
                self.ast = ast;
            }

            // Collect error tokens from parser and create diagnostics
            {
                let mut err_ptr = crate::ast::c_bindings::mcc_get_error_tokens();
                while !err_ptr.is_null() {
                    let err = &*err_ptr;
                    let pos = err.pos as u32;
                    let len = err.len as u32;
                    let location = crate::db::diagnostic::diagnostic::Location::new(
                        self.uri.clone(),
                        pos,
                        len,
                    );
                    let diagnostic = crate::db::diagnostic::diagnostic::Diagnostic::new(
                        1000,
                        crate::db::diagnostic::diagnostic::DiagnosticLevel::Error,
                        location,
                        "syntax error".to_string(),
                    );
                    workspace::WORKSPACE
                        .diagnostics
                        .lock()
                        .unwrap()
                        .add_diagnostic(diagnostic);
                    err_ptr = err.next;
                }
            }

            // Collect structured diagnostics from parser (mc_dlog_add)
            {
                let mut raw: Vec<(u32, i32, u32, u32, String)> = Vec::new();
                let mut dlog_ptr = crate::ast::c_bindings::mcc_get_dlog_entries();
                while !dlog_ptr.is_null() {
                    let entry = &*dlog_ptr;
                    let msg = if entry.msg.is_null() {
                        Self::dlog_parser_message(entry.code).to_string()
                    } else {
                        std::ffi::CStr::from_ptr(entry.msg)
                            .to_string_lossy()
                            .to_string()
                    };
                    raw.push((entry.code, entry.level, entry.pos, entry.len, msg));
                    dlog_ptr = entry.next;
                }
                raw.sort_by_key(|e| (e.2, e.3));
                let mut last_end: u32 = 0;
                for (code, level, pos, len, msg) in &raw {
                    if *pos < last_end && *code < 1100 {
                        continue;
                    }
                    last_end = pos.saturating_add(*len);
                    match level {
                        2 => crate::db::diagnostic::diagnostic::dlog_warning_at(
                            *code, *pos, *len, &msg,
                        ),
                        _ => crate::db::diagnostic::diagnostic::dlog_error_at(
                            *code, *pos, *len, &msg,
                        ),
                    }
                }
            }

            libc::free(fcontent_ptr as *mut libc::c_void);

            match self.tokens.lock() {
                Ok(mut t) => {
                    *t = McSemTokens::new();
                    t.parse(crate::ast::c_bindings::mcc_get_sem_tokens());
                    Self::extract_inline_comments(&mut t.tokens, content);
                }
                Err(e) => {
                    tracing::error!(target: "mcc::code", error = %e, "tokens mutex poisoned");
                }
            }
        }
    }

    pub fn parse_nsp(&mut self) {
        // Check whether prj_mcodes already has the file's built spacenames
        // If yes, reuse existing spacenames and uselist to avoid rebuilding
        let canonical_uri = {
            let path_buf = PathBuf::from(self.uri.clone());
            path_buf
                .canonicalize()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| self.uri.clone())
        };

        // Check whether prj_mcodes already has the file and spacenames are built
        if let Some(existing) = workspace::WORKSPACE.mcodes.get(&canonical_uri) {
            if !existing.spacenames.is_empty() {
                // Reuse existing spacenames and uselist to avoid re-traversing
                // the use graph. Do NOT set pass1_complete here — the current
                // file's types haven't been registered yet; setting this flag
                // would prevent mcb_add_recursive from calling parse_pass1_types,
                // breaking all ClassRef→ClassDef goto-def mappings.
                self.spacenames.clone_from(&existing.spacenames);
                self.uselist.clone_from(&existing.uselist);
                return;
            }
        }

        self.uselist.clear();
        self.spacenames.clear();

        let path_buf = PathBuf::from(self.uri.clone());
        let Some(current_path) = path_buf.parent() else {
            tracing::warn!(target: "mcc::code", uri = %self.uri, "cannot get parent path");
            return;
        };

        //1. uses to use list
        self.uselist = self.collect_direct_uses(current_path);

        //2. load spacenames from use targets
        let mut uses_stack = Vec::<McUse>::new();
        let mut visited_uses = HashSet::<String>::new();
        // §14: track (module_name, (orig_uri, exported_symbols)) for conflict detection
        let mut seen_modules: std::collections::HashMap<String, (String, HashSet<McIds>)> =
            std::collections::HashMap::new();
        self.uselist
            .iter()
            .for_each(|mu| uses_stack.push(mu.clone()));

        while let Some(mcuse) = uses_stack.pop() {
            // ★ Fix: use the same path normalization logic as mcb_add_recursive
            // Relative paths should be resolved relative to the current file's directory, not CWD
            let use_path = current_path.join(&mcuse.uri);
            let canonical_use_uri = use_path
                .canonicalize()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| mcuse.uri.clone());
            if !visited_uses.insert(canonical_use_uri.clone()) {
                continue;
            }

            // §11: check that unprefixed (system/third-party) use targets
            // are declared in project.toml [dependencies] or loaded via global config.
            if mcuse.prefix == McUsePrefix::PathSystem {
                let lib_name = mcuse.orig_uri.split('/').next().unwrap_or("");
                if !lib_name.is_empty() && !mcb_loaded_libs().contains(&lib_name.to_string()) {
                    dlog_warning_at(
                        800,
                        mcuse.pos,
                        mcuse.len,
                        &format!(
                            "use of undeclared dependency '{}': add it to project.toml [dependencies] or load via --lib",
                            lib_name
                        ),
                    );
                }
            }

            // (1). load ast
            // ★ Fix: always parse AST (because AstNode pointer cannot be reused across contexts)
            // but can reuse existing spacenames and uselist
            let has_existing: bool;
            {
                has_existing = workspace::WORKSPACE
                    .mcodes
                    .get(&canonical_use_uri)
                    .map(|e| !e.spacenames.is_empty() && !e.uselist.is_empty())
                    .unwrap_or(false);
            }
            let mut mcfile = match McCode::new(&mcuse.uri, self.mcbase) {
                Some(mcfile) => mcfile,
                None => {
                    tracing::debug!(target: "mcc::code", uri = %mcuse.uri, "use file not found");
                    continue;
                }
            };
            if self.mcbase {
                mcfile.parse_ast_quiet();
            } else {
                mcfile.parse_ast();
            }

            // (2). load idx from current file
            let mut cmie_list = mcfile.parse_cmie_names();
            // Defect 8: Sort alphabetically so that `use x as y` alias
            // binding to cmie_list[0] is deterministic regardless of
            // declaration order in the source file.
            cmie_list.sort_by(|a, b| a.to_string().cmp(&b.to_string()));

            // §14: detect symbol conflicts when two unprefixed uses share the same module name
            if mcuse.prefix == McUsePrefix::PathSystem {
                let module_name = mcuse.orig_uri.split('/').last().unwrap_or("");
                if !module_name.is_empty() {
                    if let Some((prev_uri, prev_symbols)) = seen_modules.get(module_name) {
                        if prev_uri != &mcuse.orig_uri {
                            let current_symbols: HashSet<McIds> =
                                cmie_list.iter().cloned().collect();
                            let overlap: Vec<_> =
                                prev_symbols.intersection(&current_symbols).collect();
                            if !overlap.is_empty() {
                                let names: Vec<String> =
                                    overlap.iter().map(|s| s.to_string()).collect();
                                dlog_error_at(
                                    801,
                                    mcuse.pos,
                                    mcuse.len,
                                    &format!(
                                        "symbol conflict in module '{}': {} collides with previous use from '{}'. Use 'as' alias to disambiguate",
                                        module_name, names.join(", "), prev_uri
                                    ),
                                );
                            }
                        }
                    } else {
                        seen_modules.insert(
                            module_name.to_string(),
                            (mcuse.orig_uri.clone(), cmie_list.iter().cloned().collect()),
                        );
                    }
                }
            }

            // (2.5) ★ Step 3: ensure CMIE definitions are registered to the global table
            // If spacenames and uselist already exist, reuse them directly
            if has_existing {
                // Reuse existing spacenames (clone only when actually needed)
                // Only cascade for default imports without aliases;
                // named imports and aliased imports must not leak symbols (§6.2 / §6.3).
                if mcuse.impt_ids.is_none() && mcuse.as_id.is_none() {
                    if let Some(existing) = workspace::WORKSPACE.mcodes.get(&canonical_use_uri) {
                        for (key, value) in existing.spacenames.iter() {
                            if !self.spacenames.contains_key(key) {
                                self.spacenames.insert(key.clone(), value.clone());
                            }
                        }
                    }
                }
            } else {
                // Need to build spacenames and uselist
                mcfile.uri = canonical_use_uri.clone();
                for (_, space_name) in mcfile.spacenames.iter_mut() {
                    space_name.uri = canonical_use_uri.clone();
                }
                // Do NOT call parse_pass1_types/parse_pass1_modules here.
                // mcb_add_recursive handles CMIE registration in dependency order.
                // Calling it here causes duplicate registration when mcb_add_recursive
                // later processes the same file.
                mcfile.parse_nsp();
                // ★ FIX: Do NOT insert mcfile into workspace here.
                // Previously, this inserted a McCode with a SEPARATE symbols Arc.
                // Later, mcb_add_recursive() creates ANOTHER McCode (with a DIFFERENT symbols Arc)
                // for the same file and inserts it, OVERWRITING this entry.
                // The overwritten entry had the correct symbol table, but the replacement
                // (created via McCode::new()) has an EMPTY symbol table.
                // Solution: let mcb_add_recursive() handle all workspace insertion.
                // Only copy spacenames to self for use resolution.
                // Only cascade for default imports without aliases (§6.2 / §6.3).
                if mcuse.impt_ids.is_none() && mcuse.as_id.is_none() {
                    for (key, value) in &mcfile.spacenames {
                        if !self.spacenames.contains_key(key) {
                            self.spacenames.insert(key.clone(), value.clone());
                        }
                    }
                }
                // ★ Fix: Do NOT mark pass1_complete here. The current file's own
                // components/modules haven't been registered via parse_pass1_types yet
                // — only the dependency's components are loaded. Setting this flag
                // early prevents mcb_add_recursive from calling parse_pass1_types on
                // this file, which means its classes never enter gt.class_name_to_id
                // and all ClassRef→ClassDef goto-def mappings break.
            }

            let is_default_import = mcuse.impt_ids.is_none();
            let has_alias = mcuse.as_id.is_some();

            // §6.3: when `as <alias>` is present, the first CMIE is registered
            // under the alias name; remaining CMIEs keep their original names.
            // The McSpaceName stores the original ident so that member resolution
            // can find the correct CMIE definition in the component/module tables.
            match mcuse.impt_ids {
                None => {
                    for (i, cmie) in cmie_list.iter().enumerate() {
                        let key = if has_alias && i == 0 {
                            // Safety: has_alias guarantees as_id is Some
                            McIds::from(mcuse.as_id.as_ref().unwrap().as_str())
                        } else {
                            cmie.clone()
                        };
                        self.spacenames
                            .insert(key, McSpaceName::new(cmie, mcuse.uri.clone()));
                    }
                }
                Some(classes) => {
                    for (i, class) in classes.iter().enumerate() {
                        if cmie_list.contains(class) {
                            let key = if has_alias && i == 0 {
                                // Safety: has_alias guarantees as_id is Some
                                McIds::from(mcuse.as_id.as_ref().unwrap().as_str())
                            } else {
                                class.clone()
                            };
                            self.spacenames
                                .insert(key, McSpaceName::new(class, mcuse.uri.clone()));
                        } else {
                            dlog_warning_at(
                                804,
                                mcuse.pos,
                                mcuse.len,
                                &format!(
                                    "imported symbol '{}' not found in '{}'",
                                    class, mcuse.orig_uri
                                ),
                            );
                        }
                    }
                }
            }

            // Cascade the dependency's spacenames — only for default imports
            // without aliases (§6.2 / §6.3). Aliased imports and named imports
            // must not leak symbols under their original names.
            if is_default_import && !has_alias {
                for (key, value) in &mcfile.spacenames {
                    if !self.spacenames.contains_key(key) {
                        self.spacenames.insert(key.clone(), value.clone());
                    }
                }
            }

            let dep_path_buf = PathBuf::from(mcfile.uri.clone());
            let dep_current_path = dep_path_buf.parent().unwrap_or(current_path);
            for mc_use in mcfile.collect_direct_uses(dep_current_path) {
                if mc_use.public {
                    uses_stack.push(mc_use);
                }
            }

            // Only insert into workspace if the existing entry hasn't been fully
            // parsed yet. mcb_add_recursive may have already parsed this file and
            // set modules_parsed=true; overwriting it with a fresh McCode would
            // cause duplicate module registrations when mcb_parse_all_modules runs.
            let should_insert = workspace::WORKSPACE
                .mcodes
                .get(&canonical_use_uri)
                .map(|e| !e.modules_parsed)
                .unwrap_or(true);
            if should_insert {
                if let dashmap::Entry::Occupied(mut entry) =
                    workspace::WORKSPACE.mcodes.entry(canonical_use_uri.clone())
                {
                    entry.insert(mcfile);
                }
            }
        }

        //3. self file cmie definitions
        self.parse_cmie_names();
    }

    /// Compute spacenames from already-resolved dependencies in the workspace.
    ///
    /// Unlike `parse_nsp`, this method does NOT recursively traverse the use
    /// graph. It assumes all dependencies have already been processed by
    /// `mcb_add_recursive` and their spacenames are available in the workspace.
    /// This eliminates the double traversal of the use dependency graph
    /// (Defect 12).
    ///
    /// Prerequisites (caller must ensure):
    /// 1. `self.uselist` is already populated via `collect_direct_uses`
    /// 2. All dependencies in `self.uselist` are already in the workspace
    ///    with their spacenames computed
    pub fn parse_nsp_from_deps(&mut self) {
        // Early exit: check if spacenames already computed in workspace
        let canonical_uri = {
            let path_buf = PathBuf::from(self.uri.clone());
            path_buf
                .canonicalize()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| self.uri.clone())
        };

        if let Some(existing) = workspace::WORKSPACE.mcodes.get(&canonical_uri) {
            if !existing.spacenames.is_empty() {
                self.spacenames.clone_from(&existing.spacenames);
                self.uselist.clone_from(&existing.uselist);
                return;
            }
        }

        self.spacenames.clear();

        let path_buf = PathBuf::from(self.uri.clone());
        let Some(current_path) = path_buf.parent() else {
            tracing::warn!(target: "mcc::code", uri = %self.uri, "cannot get parent path");
            return;
        };

        // uselist already populated by caller (mcb_add_recursive)
        let mut uses_stack: Vec<McUse> = self.uselist.iter().cloned().collect();
        let mut visited_uses = HashSet::new();
        // §14: track (module_name, (orig_uri, exported_symbols)) for conflict detection
        let mut seen_modules: std::collections::HashMap<String, (String, HashSet<McIds>)> =
            std::collections::HashMap::new();

        while let Some(mcuse) = uses_stack.pop() {
            let use_path = current_path.join(&mcuse.uri);
            let canonical_use_uri = use_path
                .canonicalize()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| mcuse.uri.clone());
            if !visited_uses.insert(canonical_use_uri.clone()) {
                continue;
            }

            // §11: check that unprefixed (system/third-party) use targets
            // are declared in project.toml [dependencies] or loaded via global config.
            if mcuse.prefix == McUsePrefix::PathSystem {
                let lib_name = mcuse.orig_uri.split('/').next().unwrap_or("");
                if !lib_name.is_empty() && !mcb_loaded_libs().contains(&lib_name.to_string()) {
                    dlog_warning_at(
                        800,
                        mcuse.pos,
                        mcuse.len,
                        &format!(
                            "use of undeclared dependency '{}': add it to project.toml [dependencies] or load via --lib",
                            lib_name
                        ),
                    );
                }
            }

            // Look up dependency's spacenames from workspace.
            // All dependencies should already be in the workspace with
            // spacenames computed, because mcb_add_recursive processes
            // them in dependency order.
            let dep_sn = match workspace::WORKSPACE.mcodes.get(&canonical_use_uri) {
                Some(dep) => dep.spacenames.clone(),
                None => {
                    tracing::debug!(
                        target: "mcc::code",
                        uri = %canonical_use_uri,
                        "use target not in workspace"
                    );
                    continue;
                }
            };

            // Extract CMIE names: entries whose URI matches the dependency's own file
            let mut cmie_list: Vec<McIds> = dep_sn
                .iter()
                .filter(|(_, v)| v.uri == canonical_use_uri)
                .map(|(k, _)| k.clone())
                .collect();
            cmie_list.sort_by(|a, b| a.to_string().cmp(&b.to_string()));

            // §14: detect symbol conflicts when two unprefixed uses share the same module name
            if mcuse.prefix == McUsePrefix::PathSystem {
                let module_name = mcuse.orig_uri.split('/').last().unwrap_or("");
                if !module_name.is_empty() {
                    if let Some((prev_uri, prev_symbols)) = seen_modules.get(module_name) {
                        if prev_uri != &mcuse.orig_uri {
                            let current_symbols: HashSet<McIds> =
                                cmie_list.iter().cloned().collect();
                            let overlap: Vec<_> =
                                prev_symbols.intersection(&current_symbols).collect();
                            if !overlap.is_empty() {
                                let names: Vec<String> =
                                    overlap.iter().map(|s| s.to_string()).collect();
                                dlog_error_at(
                                    801,
                                    mcuse.pos,
                                    mcuse.len,
                                    &format!(
                                        "symbol conflict in module '{}': {} collides with previous use from '{}'. Use 'as' alias to disambiguate",
                                        module_name, names.join(", "), prev_uri
                                    ),
                                );
                            }
                        }
                    } else {
                        seen_modules.insert(
                            module_name.to_string(),
                            (mcuse.orig_uri.clone(), cmie_list.iter().cloned().collect()),
                        );
                    }
                }
            }

            let is_default_import = mcuse.impt_ids.is_none();
            let has_alias = mcuse.as_id.is_some();

            // Register imports according to the six import forms
            match &mcuse.impt_ids {
                None => {
                    for (i, cmie) in cmie_list.iter().enumerate() {
                        let key = if has_alias && i == 0 {
                            // Safety: has_alias guarantees as_id is Some
                            McIds::from(mcuse.as_id.as_ref().unwrap().as_str())
                        } else {
                            cmie.clone()
                        };
                        self.spacenames
                            .insert(key, McSpaceName::new(cmie, mcuse.uri.clone()));
                    }
                }
                Some(classes) => {
                    for (i, class) in classes.iter().enumerate() {
                        if cmie_list.contains(class) {
                            let key = if has_alias && i == 0 {
                                McIds::from(mcuse.as_id.as_ref().unwrap().as_str())
                            } else {
                                class.clone()
                            };
                            self.spacenames
                                .insert(key, McSpaceName::new(class, mcuse.uri.clone()));
                        } else {
                            dlog_warning_at(
                                804,
                                mcuse.pos,
                                mcuse.len,
                                &format!(
                                    "imported symbol '{}' not found in '{}'",
                                    class, mcuse.orig_uri
                                ),
                            );
                        }
                    }
                }
            }

            // Cascade the dependency's spacenames — only for default imports
            // without aliases.
            if is_default_import && !has_alias {
                for (key, value) in &dep_sn {
                    if !self.spacenames.contains_key(key) {
                        self.spacenames.insert(key.clone(), value.clone());
                    }
                }
            }

            // Pub use propagation: push dependency's public uses onto stack.
            // The dependency's uselist is already populated in the workspace.
            if let Some(dep) = workspace::WORKSPACE.mcodes.get(&canonical_use_uri) {
                for mc_use in &dep.uselist {
                    if mc_use.public {
                        uses_stack.push(mc_use.clone());
                    }
                }
            }
        }

        // Register self's CMIE names
        self.parse_cmie_names();
    }

    /// List of class names defined in this file
    pub fn parse_cmie_names(&mut self) -> Vec<McIds> {
        let mut cmies: Vec<McIds> = Vec::<McIds>::new();
        for node in self.ast.iter() {
            if node.is_type(MCAST_INTERFACE)
                || node.is_type(MCAST_COMPONENT)
                || node.is_type(MCAST_MODULE)
                || node.is_type(MCAST_ENUM)
                || node.is_type(MCAST_DEFINE)
            {
                let subnodes = node.get_sub_node().expect(MISSING_SUBNODE);
                if let Some(class_name) = McIds::new(
                    &subnodes
                        .iter()
                        .find(|x| x.is_type(MCAST_NAME))
                        .expect(MISSING_SUBNODE)
                        .get_sub_node() // ids
                        .expect(MISSING_SUBNODE),
                ) {
                    if cmies.contains(&class_name) {
                        dlog_error(501, &node, "Definition already exists");
                    } else {
                        self.spacenames.insert(
                            class_name.clone(),
                            McSpaceName::new(&class_name, self.uri.clone()),
                        );
                        cmies.push(class_name);
                    }
                }
            }
        }
        cmies
    }

    /// Load a single CMIE from mcode base lib and add to global tables
    pub fn parse_cmie_single(&mut self, ident: &McIds) -> Option<McCMIE> {
        for node in self.ast.iter() {
            if node.is_type(MCAST_INTERFACE)
                || node.is_type(MCAST_COMPONENT)
                || node.is_type(MCAST_MODULE)
                || node.is_type(MCAST_ENUM)
                || node.is_type(MCAST_DEFINE)
            {
                let subnodes = node.get_sub_node().expect(MISSING_SUBNODE);
                if let Some(name) = McIds::new(
                    &subnodes
                        .iter()
                        .find(|x| x.is_type(MCAST_NAME))
                        .expect(MISSING_SUBNODE)
                        .get_sub_node() // ids
                        .expect(MISSING_SUBNODE),
                ) {
                    if ident == &name {
                        match node.get_type() {
                            MCAST_COMPONENT => {
                                if let Some(comp) = McComponent::new(&node, &self.uri) {
                                    let components_guard = &global::mcc_components;
                                    let result = components_guard
                                        .entry(McSpaceName {
                                            ident: comp.name.clone(),
                                            uri: self.uri.clone(),
                                        })
                                        .and_modify(|_| {
                                            dlog_error(1002, &node, "Duplicate component");
                                        })
                                        .or_insert(Arc::new(comp));
                                    return Some(McCMIE::Component(result.value().clone()));
                                };
                            }

                            MCAST_MODULE => {
                                // Phase 3: pre-parse function bodies before Arc wrapping
                                if let Some(mdl) = McModule::new(&node, &self.uri) {
                                    let modules_guard = &global::mcc_modules;
                                    let result = modules_guard
                                        .entry(McSpaceName {
                                            ident: mdl.name.clone(),
                                            uri: self.uri.clone(),
                                        })
                                        .and_modify(|_| {
                                            dlog_error(1503, &node, "Duplicate module");
                                        })
                                        .or_insert(Arc::new(mdl));
                                    return Some(McCMIE::Module(result.value().clone()));
                                }
                            }
                            MCAST_INTERFACE => {
                                if let Some(ifs) = McInterface::new(&node, &self.uri) {
                                    let ifs_guard = &global::mcc_interfaces;
                                    let result = ifs_guard
                                        .entry(McSpaceName {
                                            ident: ifs.name.clone(),
                                            uri: self.uri.clone(),
                                        })
                                        .and_modify(|_| {
                                            dlog_error(1001, &node, "Duplicate interface");
                                        })
                                        .or_insert(Arc::new(ifs));
                                    return Some(McCMIE::Interface(result.value().clone()));
                                }
                            }
                            MCAST_ENUM => {
                                if let Some(enum_def) = McEnumDef::new(&node, &self.uri) {
                                    // ★ LSP: register class + values in global table before
                                    //   moving enum_def into Arc, so the value spans remain
                                    //   accessible here. Clone out everything we need first
                                    //   because add_* methods take &mut self.
                                    let self_uri = self.uri.clone();
                                    let class_name_ids = McIds::from(enum_def.name.clone());
                                    let class_span =
                                        enum_def.span[0] as usize..enum_def.span[1] as usize;
                                    let value_spans: Vec<(usize, usize)> = enum_def
                                        .values
                                        .iter()
                                        .map(|v| (v.span[0] as usize, v.span[1] as usize))
                                        .collect();
                                    if let Some(class_id) = self.add_enum_class(
                                        &self_uri,
                                        &class_name_ids,
                                        class_span.clone(),
                                    ) {
                                        for (idx, (vs, ve)) in value_spans.iter().enumerate() {
                                            self.add_enum_value(
                                                &self_uri,
                                                class_id,
                                                idx as u32,
                                                *vs..*ve,
                                            );
                                        }
                                    }

                                    let space_name = McSpaceName {
                                        ident: enum_def.name.clone(),
                                        uri: self.uri.clone(),
                                    };
                                    let arc_enum = Arc::new(enum_def);
                                    if self.mcbase {
                                        let enums_guard = &global::mcc_enums;
                                        enums_guard
                                            .entry(space_name.clone())
                                            .and_modify(|_| {
                                                dlog_error(1004, &node, "Duplicate enum");
                                            })
                                            .or_insert(arc_enum.clone());
                                    } else {
                                        let enums_guard = &workspace::WORKSPACE.enums;
                                        enums_guard
                                            .entry(space_name.clone())
                                            .and_modify(|_| {
                                                dlog_error(1004, &node, "Duplicate enum");
                                            })
                                            .or_insert(arc_enum.clone());
                                    }
                                    return Some(McCMIE::Enum(arc_enum));
                                }
                            }
                            _ => panic!(),
                        }
                    }
                }
            }
        }

        None
    }

    /// Parse current file, add all definitions to project tables (parse_cmie_to_project)
    /// Phase 1a: only register component/interface/enum definitions to the global table
    /// This step does not parse module body, ensuring cross-file type definitions are ready first
    pub fn parse_pass1_types(&mut self) {
        for node in self.ast.iter() {
            match node.get_type() {
                MCAST_INTERFACE => {
                    if let Some(ifs) = McInterface::new(&node, &self.uri) {
                        let ifs_name_ids = McIds::from(ifs.name.clone());
                        let ifs_span = ifs.span.clone();
                        let self_uri = self.uri.clone();
                        let space_name = McSpaceName {
                            ident: ifs.name.clone(),
                            uri: self.uri.clone(),
                        };
                        if self.mcbase {
                            global::mcc_interfaces
                                .entry(space_name)
                                .and_modify(|_| {
                                    dlog_error(1001, &node, "Duplicate interface");
                                })
                                .or_insert(Arc::new(ifs));
                        } else {
                            workspace::WORKSPACE
                                .interfaces
                                .entry(space_name)
                                .and_modify(|_| {
                                    dlog_error(1001, &node, "Duplicate interface");
                                })
                                .or_insert(Arc::new(ifs));
                        }
                        // ★ Fix: register interface in class_name_to_id so
                        // lapper_global_classes can create ClassDef intervals.
                        self.add_global_class(
                            &self_uri,
                            &ifs_name_ids,
                            ifs_span,
                            crate::ContainerKind::Interface,
                        );
                    }
                }
                MCAST_COMPONENT => {
                    if let Some(comp) = McComponent::new(&node, &self.uri) {
                        // ★ First clone the needed data (name + uri) for global_table,
                        // then move comp into the Arc table
                        let comp_name_str = comp.name.to_string();
                        let comp_name_ids = McIds::from(comp.name.clone());
                        // Compute the correct span from the component node's subtree.
                        // Direct node.get_pos() returns 0 for MCAST_COMPONENT top-level nodes
                        // (a parser limitation). Instead, find the MCAST_NAME child and
                        // extract its MCAST_IDS grandchild which has the correct position.
                        let comp_span: Span = node
                            .get_sub_node()
                            .and_then(|sub| sub.iter().find(|x| x.is_type(MCAST_NAME)))
                            .and_then(|name_node| name_node.get_sub_node())
                            .map(|ids_node| {
                                (ids_node.get_pos() as usize)
                                    ..((ids_node.get_pos() + ids_node.get_len()) as usize)
                            })
                            .unwrap_or_else(|| {
                                // Fallback: use node position (may be 0)
                                (node.get_pos() as usize)
                                    ..((node.get_pos() + node.get_len()) as usize)
                            });
                        let self_uri = self.uri.clone();
                        tracing::info!(target: "mcc::lsp", "  parse_pass1_types: component '{}' in '{}' node_pos={} node_len={} span={:?}",
                            comp_name_str, self_uri, node.get_pos(), node.get_len(), comp_span);

                        let space_name = McSpaceName {
                            ident: comp.name.clone(),
                            uri: self.uri.clone(),
                        };
                        {
                            if self.mcbase {
                                global::mcc_components
                                    .entry(space_name)
                                    .and_modify(|_| {
                                        dlog_error(1002, &node, "Duplicate component");
                                    })
                                    .or_insert(Arc::new(comp));
                            } else {
                                workspace::WORKSPACE
                                    .components
                                    .entry(space_name)
                                    .and_modify(|_| {
                                        dlog_error(1002, &node, "Duplicate component");
                                    })
                                    .or_insert(Arc::new(comp));
                            }
                        } // borrow is dropped at end of block

                        // ★ Fix: also register to global_table.class_id_to_span,
                        // letting create_lapper() find the component's span.
                        // Previously only inserted into workspace.components without filling class_id_to_span,
                        // causing LSP goto_definition's symbol_lapper to always be empty.
                        self.add_global_class(
                            &self_uri,
                            &comp_name_ids,
                            comp_span,
                            crate::ContainerKind::Component,
                        );
                    }
                }
                MCAST_ENUM => {
                    if let Some(enum_def) = McEnumDef::new(&node, &self.uri) {
                        // ★ LSP: register class + values in global table before the move.
                        let self_uri = self.uri.clone();
                        let class_name_ids = McIds::from(enum_def.name.clone());
                        let class_span = enum_def.span[0] as usize..enum_def.span[1] as usize;
                        let value_spans: Vec<(usize, usize)> = enum_def
                            .values
                            .iter()
                            .map(|v| (v.span[0] as usize, v.span[1] as usize))
                            .collect();
                        if let Some(class_id) =
                            self.add_enum_class(&self_uri, &class_name_ids, class_span.clone())
                        {
                            for (idx, (vs, ve)) in value_spans.iter().enumerate() {
                                self.add_enum_value(&self_uri, class_id, idx as u32, *vs..*ve);
                            }
                        }

                        let space_name = McSpaceName {
                            ident: enum_def.name.clone(),
                            uri: self.uri.clone(),
                        };
                        if self.mcbase {
                            global::mcc_enums
                                .entry(space_name)
                                .and_modify(|_| {
                                    dlog_error(1004, &node, "Duplicate enum");
                                })
                                .or_insert(Arc::new(enum_def));
                        } else {
                            workspace::WORKSPACE
                                .enums
                                .entry(space_name)
                                .and_modify(|_| {
                                    dlog_error(1004, &node, "Duplicate enum");
                                })
                                .or_insert(Arc::new(enum_def));
                        }
                    }
                }
                MCAST_DEFINE => {
                    if let Some(def) =
                        crate::semantic::mc_define::McDefineDef::new(&node, &self.uri)
                    {
                        let space_name = McSpaceName {
                            ident: def.name.clone(),
                            uri: self.uri.clone(),
                        };
                        if self.mcbase {
                            global::mcc_defines
                                .entry(space_name)
                                .and_modify(|_| {
                                    dlog_error(1505, &node, "Duplicate define");
                                })
                                .or_insert(Arc::new(def));
                        } else {
                            workspace::WORKSPACE
                                .defines
                                .entry(space_name)
                                .and_modify(|_| {
                                    dlog_error(1505, &node, "Duplicate define");
                                })
                                .or_insert(Arc::new(def));
                        }
                    }
                }
                _ => {} // MCAST_MODULE handled in the second phase
            }
        }

        // Build spacenames from all definitions found in this file
        for node in self.ast.iter() {
            if node.is_type(MCAST_INTERFACE)
                || node.is_type(MCAST_COMPONENT)
                || node.is_type(MCAST_MODULE)
                || node.is_type(MCAST_ENUM)
                || node.is_type(MCAST_DEFINE)
            {
                if let Some(subnodes) = node.get_sub_node() {
                    if let Some(name_node) = subnodes.iter().find(|x| x.is_type(MCAST_NAME)) {
                        if let Some(ids_node) = name_node.get_sub_node() {
                            if let Some(class_name) = McIds::new(&ids_node) {
                                let class_name_clone = class_name.clone();
                                if !self.spacenames.contains_key(&class_name) {
                                    self.spacenames.insert(
                                        class_name_clone,
                                        McSpaceName::new(&class_name, self.uri.clone()),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        // Mark Pass1 parse as complete
        self.pass1_complete = true;
    }

    /// Phase 1b: parse all module definitions (at this point all component/interface/enum are already registered)
    /// Extract (name, span) pairs from MCAST_PARAMS node for function parameter
    /// definitions. Handles MCAST_PARAM wrappers and direct ID/IDS nodes.
    fn extract_func_param_spans(params_node: &AstNode) -> Vec<(String, std::ops::Range<usize>)> {
        let mut result = Vec::new();
        if let Some(sub) = params_node.get_sub_node() {
            for param in sub.iter() {
                let inner = if param.get_type() == MCAST_PARAM {
                    param.get_sub_node().unwrap_or(param)
                } else {
                    param.clone()
                };
                if let Some(ids) = McIds::new(&inner) {
                    let span =
                        (inner.get_pos() as usize)..((inner.get_pos() + inner.get_len()) as usize);
                    result.push((ids.to_string(), span));
                }
            }
        }
        result
    }

    fn extract_pin_name_spans(comp: &McComponent) -> Vec<(String, std::ops::Range<usize>)> {
        comp.pins
            .pin_name_spans
            .iter()
            .map(|(n, s)| (n.clone(), s.clone()))
            .collect()
    }

    /// §3.2.2: Extract (pin_id, span) for pin ID definitions.
    fn extract_pin_id_spans(comp: &McComponent) -> Vec<(String, std::ops::Range<usize>)> {
        comp.pins
            .pin_id_spans
            .iter()
            .map(|(n, s)| (n.clone(), s.clone()))
            .collect()
    }

    /// §3.2.2: Extract (iface_name, span) for pin interface definitions.
    fn extract_pin_iface_spans(comp: &McComponent) -> Vec<(String, std::ops::Range<usize>)> {
        comp.pins
            .pin_iface_spans
            .iter()
            .map(|(n, s)| (n.clone(), s.clone()))
            .collect()
    }

    /// Extract (key_name, span) for spec-like attribute keys.
    fn extract_spec_key_spans(comp: &McComponent) -> Vec<(String, std::ops::Range<usize>)> {
        comp.attrs
            .iter()
            .filter_map(|a| a.key_span.clone().map(|s| (a.id.to_string(), s)))
            .collect()
    }

    pub fn parse_pass1_modules(&mut self) {
        if self.modules_parsed && !self.use_table_dirty {
            return;
        }
        // ★ §7.6: Use table dirty — only rebuild RefDefMap/name_index,
        // no need to re-parse modules.
        if self.modules_parsed && self.use_table_dirty {
            self.create_lapper(); // includes inline Layer 2 + consolidate (Layer 1 + name_index)
            self.use_table_dirty = false;
            return;
        }
        self.modules_parsed = true;

        for (_i, node) in self.ast.iter().enumerate() {
            let node_type = node.get_type();
            if node_type == MCAST_MODULE {
                if let Some(module) = McModule::new(&node, &self.uri) {
                    let module_name = module.name.clone();
                    let module_name_ids = McIds::from(module_name.clone());
                    let module_span = module.span.clone();
                    let self_uri = self.uri.clone();
                    let key = McSpaceName {
                        ident: module_name.clone(),
                        uri: self.uri.clone(),
                    };
                    // ★ Fix (Defect 30): Register module in class_name_to_id so
                    // lapper_global_classes can create ClassDef intervals for goto-def.
                    // Previously only component and interface were registered, leaving
                    // module names without ClassDef entries in the lapper.
                    self.add_global_class(
                        &self_uri,
                        &module_name_ids,
                        module_span,
                        crate::ContainerKind::Module,
                    );
                    // Replace any previously registered shallow copy with fully-parsed module
                    workspace::WORKSPACE
                        .modules
                        .entry(key)
                        .and_modify(|_| {
                            dlog_error(1503, &node, "Duplicate module");
                        })
                        .or_insert(Arc::new(module));
                }
            }
        }
        // ★ Fix: Build the lapper after processing all modules.
        // mcb_parse_all_modules() does remove+insert on the McCode, creating a new McCode instance.
        // This new instance has the same Arc<Mutex<McSemSymbols>> (shared symbol data),
        // but create_lapper() was NOT called on it, so symbol_lapper was empty.
        // Call create_lapper here to ensure the lapper is built for the current file.
        // ★ Fix: Build the lapper after processing all modules.
        self.create_lapper(); // includes inline Layer 2 + consolidate_ref_def_map (Layer 1 + name_index)
        self.use_table_dirty = false;

        // ★ §7.6: Mark dependent files dirty — their Use table P4 entries
        // may need refreshing because this file's CMIE defs changed.
        let canonical_self = crate::build::pass1::canonicalize_project_uri(&self.uri);
        if let Some(deps) = workspace::WORKSPACE.reverse_deps.get(&canonical_self) {
            for dep_uri in deps.value().iter() {
                if let Some(mut dep_file) = workspace::WORKSPACE.mcodes.get_mut(dep_uri) {
                    dep_file.use_table_dirty = true;
                }
            }
        }
    }

    /// Backward-compatible interface: parse all definitions sequentially (single-file scenario or system library)
    pub fn parse_pass1(&mut self) {
        self.parse_pass1_types();
        self.parse_pass1_modules();
    }

    // ========================================================================
    // Phase 3: Pre-parse function bodies
    // ========================================================================

    /// Pre-parse function bodies for all functions in the module.
    pub fn add_global_class(
        &mut self,
        uri: &McURI,
        class_name: &McIds,
        span: Span,
        kind: crate::ContainerKind,
    ) -> Option<DeclareId> {
        let result = match self.symbols.lock() {
            Ok(sem) => match sem.global_table.lock() {
                Ok(mut gt) => {
                    let gt: &mut crate::ast::ast_semantic::GlobalSymbolTable = &mut gt;
                    Some(gt.add_class(uri, class_name, span.clone()))
                }
                Err(e) => {
                    tracing::error!(target: "mcc::code", error = %e, "global_table mutex poisoned (add_global_class)");
                    None
                }
            },
            Err(e) => {
                tracing::error!(target: "mcc::code", error = %e, "symbols mutex poisoned (add_global_class)");
                None
            }
        };
        // ★ LSP: Also register in workspace lsp.class_table for cross-context lookup
        if let Some(class_id) = result {
            tracing::info!(target: "mcc::lsp", "  add_global_class: registered '{}' ({}) in '{}' -> class_id={:?}", class_name, kind.as_str(), uri, class_id);
            let mut table = workspace::WORKSPACE.lsp.class_table.lock().unwrap();
            // class_table is a plain-name index (queried externally by &str),
            // so flatten the McIds here at the boundary.
            table.insert(
                (uri.to_string(), kind, class_name.to_string()),
                (class_id, span),
            );
        }
        result
    }
    pub fn add_declare_class(&mut self, uri: &McURI, span: Span, class_id: DeclareId) {
        match self.symbols.lock() {
            Ok(sem) => match sem.global_table.lock() {
                Ok(mut gt) => {
                    let gt: &mut crate::ast::ast_semantic::GlobalSymbolTable = &mut gt;
                    let _refid = gt.add_declare_class(uri, span, class_id);
                }
                Err(e) => {
                    tracing::error!(target: "mcc::code", error = %e, "global_table mutex poisoned (add_declare_class)")
                }
            },
            Err(e) => {
                tracing::error!(target: "mcc::code", error = %e, "symbols mutex poisoned (add_declare_class)")
            }
        }
    }

    /// Register an enum class definition (`enum PKG { ... }`) in the global
    /// table so `enum_class_def` lapper entries can resolve cross-file.
    /// Returns the assigned DeclareId, or None on lock failure.
    pub fn add_enum_class(
        &mut self,
        uri: &McURI,
        class_name: &McIds,
        span: Span,
    ) -> Option<DeclareId> {
        let result = match self.symbols.lock() {
            Ok(sem) => match sem.global_table.lock() {
                Ok(mut gt) => Some(gt.add_enum_class(uri, class_name, span.clone())),
                Err(e) => {
                    tracing::error!(target: "mcc::code", error = %e, "global_table mutex poisoned (add_enum_class)");
                    None
                }
            },
            Err(e) => {
                tracing::error!(target: "mcc::code", error = %e, "symbols mutex poisoned (add_enum_class)");
                None
            }
        };
        if let Some(class_id) = result {
            tracing::info!(target: "mcc::lsp", "  add_enum_class: registered '{}' in '{}' -> class_id={:?} span={:?}", class_name, uri, class_id, span);
        }
        result
    }

    /// Register an enum value row (`SOP8,` inside `enum PKG { ... }`) in the
    /// global table. `value_idx` is the position inside the body (0-based).
    /// Returns the packed value_id (class_id << 16 | value_idx), or None.
    pub fn add_enum_value(
        &mut self,
        uri: &McURI,
        class_id: DeclareId,
        value_idx: u32,
        span: Span,
    ) -> Option<DeclareId> {
        match self.symbols.lock() {
            Ok(sem) => match sem.global_table.lock() {
                Ok(mut gt) => Some(gt.add_enum_value(uri, class_id, value_idx, span)),
                Err(e) => {
                    tracing::error!(target: "mcc::code", error = %e, "global_table mutex poisoned (add_enum_value)");
                    None
                }
            },
            Err(e) => {
                tracing::error!(target: "mcc::code", error = %e, "symbols mutex poisoned (add_enum_value)");
                None
            }
        }
    }

    /// Public wrapper for RPC handlers.
    pub fn scope_path_from_scope_str_public(uri: &McURI, scope: &str) -> crate::ScopePath {
        crate::refdef::register::scope_path_from_scope_str(uri, scope)
    }

    fn param_def_kind(
        param: Option<&crate::semantic::basic::mc_paramd::McParamDeclare>,
    ) -> SymbolKind {
        match param {
            Some(p)
                if p.param_type.kind
                    == crate::semantic::basic::mc_param_type::McParamTypeKind::Unknown =>
            {
                SymbolKind::UnknownDef
            }
            _ => SymbolKind::ParamDef,
        }
    }

    /// ★ §4.3 #22-#26: Resolve correct ref kind for a port ref, dispatching
    /// inst.member patterns (e.g. uC.PA1 → PinNameRef, cap4.1 → PinIdRef).
    fn resolve_net_ref_kind(
        port_name: &str,
        insts: &crate::semantic::mc_inst::McInstances,
    ) -> SymbolKind {
        // Check for inst.member pattern (dot-separated)
        if let Some(dot_pos) = port_name.find('.') {
            let base = &port_name[..dot_pos];
            let member = &port_name[dot_pos + 1..];

            // Look up the base instance
            if let Some((_iotype, inst)) = insts.insts().get(base) {
                match inst {
                    crate::semantic::mc_inst::McInstance::Component(comp) => {
                        // ★ Priority: pin id > pin name > sub-component (§4.3)
                        // Pure numeric members are pin ids, not names
                        if member.chars().all(|c| c.is_ascii_digit()) {
                            return SymbolKind::PinIdRef;
                        }
                        if comp.base.pins.names_to_id.contains_key(member) {
                            return SymbolKind::PinNameRef;
                        }
                        // Sub-component or sub-instance declared inside the component
                        if comp.base.insts.contains(member) {
                            return SymbolKind::InstRef;
                        }
                    }
                    crate::semantic::mc_inst::McInstance::Module(_m) => {
                        return SymbolKind::InstRef;
                    }
                    crate::semantic::mc_inst::McInstance::Bus(_) => {
                        // ★ §3.4.3 (rev): `MIC.P` member refs resolve to the
                        // member def (precise span), not the whole bus.
                        return SymbolKind::BusMemberRef;
                    }
                    crate::semantic::mc_inst::McInstance::Interface(iface) => {
                        if iface.base.pins.names_to_id.contains_key(member) {
                            return SymbolKind::PinNameRef;
                        }
                    }
                    _ => {}
                }
            }
        }
        // Plain name (no dot): check if it's a Component/Module instance
        // before defaulting to PortRef. This ensures instance references like
        // `X6` in `X6.setup(...)` are classified as InstRef, not PortRef,
        // so the tooltip shows "instance" instead of "label".
        if let Some((_iotype, inst)) = insts.insts().get(port_name) {
            if matches!(
                inst,
                crate::semantic::mc_inst::McInstance::Component(_)
                    | crate::semantic::mc_inst::McInstance::Module(_)
            ) {
                return SymbolKind::InstRef;
            }
        }
        // Default: plain port ref
        SymbolKind::PortRef
    }

    /// ★ Phase 3 (cross-container member chain): ref kind for a member hit
    /// resolved by `refdef::chain`. The chain knows the exact def kind, so we
    /// use its "Ref" counterpart; `FuncParamRef` is the catch-all fallback
    /// whose candidate def list in `fill_refdef_layer2` covers every kind.
    fn chain_ref_kind(def_kind: SymbolKind) -> SymbolKind {
        match def_kind {
            SymbolKind::PinNameDef => SymbolKind::PinNameRef,
            SymbolKind::PinIdDef => SymbolKind::PinIdRef,
            SymbolKind::PinIfaceDef => SymbolKind::PinIfaceRef,
            SymbolKind::BusDef => SymbolKind::BusRef,
            SymbolKind::BusMemberDef => SymbolKind::BusMemberRef,
            SymbolKind::LabelDef => SymbolKind::LabelRef,
            SymbolKind::PortDef | SymbolKind::ParamDef => SymbolKind::PortRef,
            SymbolKind::InstDef => SymbolKind::InstRef,
            SymbolKind::FuncDef => SymbolKind::FuncRef,
            SymbolKind::EnumValDef => SymbolKind::EnumValRef,
            _ => SymbolKind::FuncParamRef,
        }
    }

    /// Build RefDefMap from semantic tables.
    /// Runs after parse_pass1_modules() registers all symbols, before create_lapper().
    fn consolidate_ref_def_map(&mut self) {
        use crate::ast::ast_semantic::{GlobalSymbolTable, RefDefEntry, RefDefMap, SymbolKind};

        let mut map = RefDefMap::new();

        // Scope the lock to release before writing
        {
            let sem = match self.symbols.lock() {
                Ok(s) => s,
                Err(_) => return,
            };
            let gt = match sem.global_table.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            let lt = &sem.local_table;
            let _uri = &self.uri;

            // ── DeclareId → scope map (reserved for future cross-file Layer 1d) ──
            let _decl_id_to_scope: std::collections::HashMap<u32, String> = lt
                .name_to_declare_id
                .iter()
                .map(|((_fid, cid, fnid, _n), (did, _))| {
                    let scope = crate::ast::ast_semantic::scope_from_ids(
                        &sem.container_table,
                        &sem.func_table,
                        *cid,
                        *fnid,
                    );
                    (u32::from(*did), scope)
                })
                .collect();

            // ── Layer 1: ID chain ──

            // 1a. class_ref (ReferenceId) → class_def
            // ★ Fix: iterate span_to_declare_class_id filtered by current URI,
            // then resolve ref_id → class_id → class_span. Previously iterated
            // ALL declare_id_to_class_id entries (from ALL loaded files), which
            // leaked stale entries (e.g. library CAP refs with class_id=0) into
            // the current file's MAP, causing incorrect goto-def jumps.
            for ((loop_uri, _span), ref_id) in gt.span_to_declare_class_id.iter() {
                if loop_uri != _uri {
                    continue;
                }
                if let Some(class_id) = gt.declare_id_to_class_id.get(ref_id) {
                    if let Some((def_uri, span)) = gt.class_id_to_span.get(class_id) {
                        let fid = map.intern_file(def_uri);
                        let cid = map.intern_container("");
                        // ★ Fix: use class_id (DeclareId) as key, matching
                        // what the lapper and ref_entries use. Previously
                        // used ref_id (ReferenceId) which is a different
                        // counter — causing goto_def to resolve wrong classes.
                        map.insert(
                            SymbolKind::ClassRef,
                            u32::from(*class_id),
                            RefDefEntry {
                                ref_kind: SymbolKind::ClassDef,
                                ref_id: 0,
                                def_loc: SourceLocation {
                                    file_id: fid,
                                    container_id: cid,
                                    func_id: 0,
                                    byte_start: span.start as u32,
                                    byte_end: span.end as u32,
                                },
                                def_kind: SymbolKind::ClassDef,
                                cmie_kind: crate::ast::ast_semantic::CmieKind::UNKNOWN,
                            },
                        );
                    }
                }
            }

            // 1b (REMOVED): class_id → def mapping. ClassRef in lapper uses
            // ReferenceId (from span_to_declare_class_id), not DeclareId
            // (class_id). This section created ClassRef entries keyed by
            // class_id, which collides with ReferenceId space (both start at 0)
            // and produces incorrect goto-def jumps. Layer 1a above correctly
            // resolves ClassRef (ReferenceId) → class_id → def_span.

            // 1c. cross-file class ref targets (cached from create_lapper, §8.2)
            // ★ Fix: cross_file_targets now stores DeclareId (class_id) instead
            // of ReferenceId, matching the ID space used by the lapper.
            for (class_id, def_uri, span) in &self.cross_file_targets {
                let fid = map.intern_file(def_uri);
                let cid = map.intern_container("");
                map.insert(
                    SymbolKind::ClassRef,
                    u32::from(*class_id),
                    RefDefEntry {
                        ref_kind: SymbolKind::ClassDef,
                        ref_id: 0,
                        def_loc: SourceLocation {
                            file_id: fid,
                            container_id: cid,
                            func_id: 0,
                            byte_start: span.start as u32,
                            byte_end: span.end as u32,
                        },
                        def_kind: SymbolKind::ClassDef,
                        cmie_kind: crate::ast::ast_semantic::CmieKind::UNKNOWN,
                    },
                );
            }

            // 1d (REMOVED): instance_ref → def (via inst_id_to_declare_inst).
            // Layer 1d mixed ReferenceId and DeclareId namespaces, producing
            // wrong def positions. Same-file InstRef resolution is now handled
            // by Layer 2 (fill_refdef_layer2) with proper kind-specific matching.
            // Cross-file InstRef will be re-added with a clean implementation.

            // 1e. enum_value_ref → def (§1.3: P3 > P4 > P5).
            // Collect all entries, then insert in priority order (P3 first).
            // Lower-priority entries are skipped if key already exists.
            let mut enum_val_entries: Vec<(u32, String, usize, usize)> = Vec::new();
            let mut collect_ev = |gt: &GlobalSymbolTable| {
                for (value_id, (def_uri, span)) in &gt.enum_value_id_to_span {
                    enum_val_entries.push((
                        u32::from(*value_id),
                        def_uri.clone(),
                        span.start,
                        span.end,
                    ));
                }
            };
            // Gather from all sources
            collect_ev(&gt); // P3: current file
            for entry in workspace::WORKSPACE.mcodes.iter() {
                if entry.key() == &self.uri {
                    continue;
                }
                if let Ok(ws_sym) = entry.value().symbols.lock() {
                    if let Ok(ws_gt) = ws_sym.global_table.lock() {
                        collect_ev(&ws_gt);
                    }
                }
            }
            for entry in crate::db::infra::libmgr::mcc_blibs.iter() {
                if let Ok(ws_sym) = entry.value().symbols.lock() {
                    if let Ok(ws_gt) = ws_sym.global_table.lock() {
                        collect_ev(&ws_gt);
                    }
                }
            }
            // Insert in collection order: P3 first (wins), then P4, then P5.
            // Collection order is already P3→P4→P5; "already exists → skip"
            // ensures higher-priority entries are kept.
            for (value_id, def_uri, start, end) in &enum_val_entries {
                if map
                    .entries
                    .contains_key(&(SymbolKind::EnumValRef, *value_id))
                {
                    continue; // already resolved by higher priority
                }
                let fid = map.intern_file(def_uri);
                let cid = map.intern_container("");
                map.insert(
                    SymbolKind::EnumValRef,
                    *value_id,
                    RefDefEntry {
                        ref_kind: SymbolKind::ClassDef,
                        ref_id: 0,
                        def_loc: SourceLocation {
                            file_id: fid,
                            container_id: cid,
                            func_id: 0,
                            byte_start: *start as u32,
                            byte_end: *end as u32,
                        },
                        def_kind: SymbolKind::EnumValDef,
                        cmie_kind: crate::ast::ast_semantic::CmieKind::UNKNOWN,
                    },
                );
            }

            // 1f. enum class ref → enum class def (§1.3: P3 > P4 > P5).
            // Like Layer 1b: use DeclareId (class_id) as key, matching lapper EnumRef.
            let mut enum_cls_entries: Vec<(u32, String, usize, usize)> = Vec::new();
            let mut collect_ec = |gt: &GlobalSymbolTable| {
                for (class_id, (def_uri, span)) in &gt.enum_class_id_to_span {
                    enum_cls_entries.push((
                        u32::from(*class_id),
                        def_uri.clone(),
                        span.start,
                        span.end,
                    ));
                }
            };
            // Gather P3→P4→P5, insert in collection order (P3 wins).
            collect_ec(&gt);
            for entry in workspace::WORKSPACE.mcodes.iter() {
                if entry.key() == &self.uri {
                    continue;
                }
                if let Ok(ws_sym) = entry.value().symbols.lock() {
                    if let Ok(ws_gt) = ws_sym.global_table.lock() {
                        collect_ec(&ws_gt);
                    }
                }
            }
            for entry in crate::db::infra::libmgr::mcc_blibs.iter() {
                if let Ok(ws_sym) = entry.value().symbols.lock() {
                    if let Ok(ws_gt) = ws_sym.global_table.lock() {
                        collect_ec(&ws_gt);
                    }
                }
            }
            // Insert in collection order: P3 first (wins), then P4, then P5.
            for (ref_id, def_uri, start, end) in &enum_cls_entries {
                if map.entries.contains_key(&(SymbolKind::EnumRef, *ref_id)) {
                    continue;
                }
                let fid = map.intern_file(def_uri);
                let cid = map.intern_container("");
                map.insert(
                    SymbolKind::EnumRef,
                    *ref_id,
                    RefDefEntry {
                        ref_kind: SymbolKind::ClassDef,
                        ref_id: 0,
                        def_loc: SourceLocation {
                            file_id: fid,
                            container_id: cid,
                            func_id: 0,
                            byte_start: *start as u32,
                            byte_end: *end as u32,
                        },
                        def_kind: SymbolKind::EnumDef,
                        cmie_kind: crate::ast::ast_semantic::CmieKind::UNKNOWN,
                    },
                );
            }
        } // lock released here

        // Layer 2 (shared DeclareId matching) now built inline at end of create_lapper.

        // ── Name index (Use table §5): P5 → P4 → P3 order ──
        // Later insertions overwrite earlier ones, so lowest priority first.
        {
            // P5: mcode system library — register from global tables
            use crate::ast::ast_semantic::CmieKind;
            let mut add_p5 = |name: &str,
                              uri_str: &str,
                              span_start: usize,
                              span_end: usize,
                              def_kind: SymbolKind,
                              cmie_kind: u8| {
                let uri: McURI = uri_str.to_string();
                let fid = map.intern_file(&uri);
                let cid = map.intern_container("");
                let entry = RefDefEntry {
                    ref_kind: SymbolKind::ClassDef,
                    ref_id: 0,
                    def_loc: SourceLocation {
                        file_id: fid,
                        container_id: cid,
                        func_id: 0,
                        byte_start: span_start as u32,
                        byte_end: span_end as u32,
                    },
                    def_kind,
                    cmie_kind,
                };
                map.name_index
                    .insert((self.uri.to_string(), name.to_string()), entry);
            };
            for entry in crate::db::infra::global::mcc_components.iter() {
                let c = entry.value();
                let name = entry.key().ident.to_string();
                let uri = entry.key().uri.to_string();
                add_p5(
                    &name,
                    &uri,
                    c.span.start,
                    c.span.end,
                    SymbolKind::ClassDef,
                    CmieKind::Component as u8,
                );
            }
            for entry in crate::db::infra::global::mcc_modules.iter() {
                let m = entry.value();
                let name = entry.key().ident.to_string();
                let uri = entry.key().uri.to_string();
                add_p5(
                    &name,
                    &uri,
                    m.span.start,
                    m.span.end,
                    SymbolKind::ClassDef,
                    CmieKind::Module as u8,
                );
            }
            for entry in crate::db::infra::global::mcc_interfaces.iter() {
                let i = entry.value();
                let name = entry.key().ident.to_string();
                let uri = entry.key().uri.to_string();
                add_p5(
                    &name,
                    &uri,
                    i.span.start,
                    i.span.end,
                    SymbolKind::ClassDef,
                    CmieKind::Interface as u8,
                );
            }
            for entry in crate::db::infra::global::mcc_enums.iter() {
                let e = entry.value();
                let name = entry.key().ident.to_string();
                let uri = entry.key().uri.to_string();
                add_p5(
                    &name,
                    &uri,
                    e.span[0] as usize,
                    e.span[1] as usize,
                    SymbolKind::EnumDef,
                    CmieKind::Enum as u8,
                );
            }

            // P4: use chain (medium priority, overwrites P5)
            // ★ Fix: target_map entry indices point into target_map.entries,
            // not self.entries. We must copy the entry data (re-interning file/container)
            // and register the new index in self's name_index.
            for mc_use in &self.uselist {
                let target_uri = crate::build::pass1::canonicalize_project_uri(&mc_use.uri);
                // ★ §7.6: Register reverse dependency — "self uses target"
                let mut deps = workspace::WORKSPACE
                    .reverse_deps
                    .entry(target_uri.clone())
                    .or_default();
                if !deps.contains(&self.uri) {
                    deps.push(self.uri.clone());
                }
                if let Some(target_file) = workspace::WORKSPACE.mcodes.get(&target_uri) {
                    if let Ok(target_sym) = target_file.symbols.lock() {
                        if let Some(ref target_map) = target_sym.ref_def_map {
                            for ((_target_uri, name), src_entry) in &target_map.name_index {
                                let src_file_uri =
                                    target_map.files.get(src_entry.def_loc.file_id as usize);
                                let src_container = if src_entry.def_loc.container_id != u32::MAX {
                                    target_map
                                        .containers
                                        .get(src_entry.def_loc.container_id as usize)
                                        .map(|c| c.as_str())
                                        .unwrap_or("")
                                } else {
                                    ""
                                };
                                let new_fid = if let Some(furi) = src_file_uri {
                                    map.intern_file(&McURI::from(furi.as_str()))
                                } else {
                                    map.intern_file(&self.uri)
                                };
                                let new_cid = map.intern_container(src_container);
                                let entry = RefDefEntry {
                                    ref_kind: src_entry.ref_kind,
                                    ref_id: src_entry.ref_id,
                                    def_loc: SourceLocation {
                                        file_id: new_fid,
                                        container_id: new_cid,
                                        func_id: 0,
                                        byte_start: src_entry.def_loc.byte_start,
                                        byte_end: src_entry.def_loc.byte_end,
                                    },
                                    def_kind: src_entry.def_kind,
                                    cmie_kind: src_entry.cmie_kind,
                                };
                                // Register original name (P4)
                                map.name_index.insert(
                                    (self.uri.to_string(), name.to_string()),
                                    entry.clone(),
                                );
                                // ★ §5.1 use as alias: e.g. `use ./helper as h`
                                if let Some(ref alias) = mc_use.as_id {
                                    let aliased = format!("{alias}.{name}");
                                    map.name_index
                                        .insert((self.uri.to_string(), aliased), entry);
                                }
                            }
                        }
                    }
                }
            }

            // P3: own file CMIE defs (highest priority, overwrites P4+P5)
            // Need to re-acquire GlobalSymbolTable lock to read class defs
            if let Ok(sem) = self.symbols.lock() {
                if let Ok(gt) = sem.global_table.lock() {
                    for ((def_uri, class_name), class_id) in &gt.class_name_to_id {
                        if let Some((_u, span)) = gt.class_id_to_span.get(class_id) {
                            let fid = map.intern_file(def_uri);
                            let cid = map.intern_container("");
                            let entry = RefDefEntry {
                                ref_kind: SymbolKind::ClassDef,
                                ref_id: u32::from(*class_id),
                                def_loc: SourceLocation {
                                    file_id: fid,
                                    container_id: cid,
                                    func_id: 0,
                                    byte_start: span.start as u32,
                                    byte_end: span.end as u32,
                                },
                                def_kind: SymbolKind::ClassDef,
                                cmie_kind: crate::ast::ast_semantic::CmieKind::UNKNOWN,
                            };
                            map.add_name_alias(&self.uri, &class_name.to_string(), entry);
                        }
                    }
                    for ((def_uri, class_name), class_id) in &gt.enum_class_name_to_id {
                        if let Some((_u, span)) = gt.enum_class_id_to_span.get(class_id) {
                            let fid = map.intern_file(def_uri);
                            // Use a distinct container so same-named enum+component coexist
                            // in name_to_declare_id (e.g. enum CAP + component CAP).
                            let cid = map.intern_container("@enum");
                            let entry = RefDefEntry {
                                ref_kind: SymbolKind::ClassDef,
                                ref_id: u32::from(*class_id),
                                def_loc: SourceLocation {
                                    file_id: fid,
                                    container_id: cid,
                                    func_id: 0,
                                    byte_start: span.start as u32,
                                    byte_end: span.end as u32,
                                },
                                def_kind: SymbolKind::EnumDef,
                                cmie_kind: CmieKind::Enum as u8,
                            };
                            map.add_name_alias(&self.uri, &class_name.to_string(), entry);
                        }
                    }
                }
            }
        }

        tracing::info!(
            target: "mcc::lsp",
            "consolidate_ref_def_map: uri={} entries={} files={} containers={} names={}",
            self.uri, map.entries.len(), map.files.len(), map.containers.len(),
            map.name_index.len()
        );

        // Write back to symbols
        if let Ok(mut sem) = self.symbols.lock() {
            sem.ref_def_map = Some(map);
        }
    }

    pub fn create_lapper(&mut self) {
        tracing::info!(target: "mcc::lsp", "[LAPPER_DEBUG] create_lapper START uri={}", self.uri);
        self.cross_file_targets.clear();
        // Clear stale name_to_declare_id entries from previous lapper builds.
        // mcb_parse_all_modules rebuilds the lapper but name_to_declare_id is
        // shared via Arc, so old DeclareIds would pollute FuncRef scope searches.
        if let Ok(mut sem) = self.symbols.lock() {
            let file_id = crate::ast::ast_semantic::intern(&mut sem.file_table, self.uri.as_str());
            let _ = sem.local_table.name_to_declare_id.len();
            sem.local_table
                .name_to_declare_id
                .retain(|(fid, _, _, _), _| *fid != file_id);
            sem.local_table
                .scope_index
                .retain(|_, (fid, _, _)| *fid != file_id);
            // Cleanup complete — stale entries removed
        }
        match self.symbols.lock() {
            Ok(mut sem) => {
                // ★ Dedup-aware lapper: rejects duplicate (kind, start, stop) on insert.
                let mut symbol_lapper = DedupLapper::new();
                // ★ Clear ref_entries to avoid duplicates when create_lapper is called
                // multiple times (e.g., once from parse_pass1_modules and again from
                // mcb_parse_all_modules as a robustness fallback).
                sem.ref_entries.clear();

                Self::lapper_global_classes(
                    &self.uri,
                    &mut self.cross_file_targets,
                    &mut sem,
                    &mut symbol_lapper,
                );
                Self::lapper_instance_decls_and_refs(&self.uri, &mut sem, &mut symbol_lapper);
                Self::lapper_interfaces(&self.uri, &mut sem, &mut symbol_lapper);
                Self::lapper_module_ports(&self.uri, &mut sem, &mut symbol_lapper);

                let decl_count_file_id =
                    crate::ast::ast_semantic::intern(&mut sem.file_table, self.uri.as_str());
                let decl_count = sem
                    .local_table
                    .name_to_declare_id
                    .iter()
                    .filter(|((fid, _, _, _), _)| *fid == decl_count_file_id)
                    .count();
                let local_ref_count = sem.local_table.inst_id_to_span.len();
                tracing::info!(target: "mcc::lsp", "create_lapper: {} decls, {} local_refs, lapper len={}", decl_count, local_ref_count, symbol_lapper.inner.len());

                Self::lapper_func_define_role(&self.uri, &self.ast, &mut sem, &mut symbol_lapper);
                Self::lapper_function_params(&self.uri, &mut sem, &mut symbol_lapper);
                Self::lapper_component_defs(&self.uri, &mut sem, &mut symbol_lapper);
                Self::lapper_enum_refs(&self.uri, &self.ast, &mut sem, &mut symbol_lapper);
                Self::lapper_scoped_enum_bare_refs(
                    &self.uri,
                    &self.ast,
                    &mut sem,
                    &mut symbol_lapper,
                );

                // ★ InstRef from inst_id_to_span (was in lapper_second_pass_and_dedup)
                // Collect first to avoid borrowing sem immutably + mutably.
                {
                    let insts: Vec<_> = sem
                        .local_table
                        .inst_id_to_span
                        .iter()
                        .map(|(inst_id, span)| {
                            let decl_id = sem
                                .local_table
                                .inst_id_to_declare_inst
                                .get(inst_id)
                                .copied()
                                .unwrap_or(DeclareId::default());
                            (*inst_id, span.clone(), decl_id)
                        })
                        .collect();
                    for (_inst_id, span, decl_id) in insts {
                        symbol_lapper.insert(Interval {
                            start: span.start,
                            stop: span.end,
                            val: SymbolType::new(SymbolKind::InstRef, u32::from(decl_id)),
                        });
                        sem.ref_entries.push((
                            SymbolKind::InstRef,
                            u32::from(decl_id),
                            span.start,
                            span.end,
                        ));
                    }
                }

                // ★ DedupLapper already deduplicates on insert by (kind, start, stop).
                sem.symbol_lapper = symbol_lapper.into_inner();
                // ★ Dedup ref_entries: multiple push sites or data-level duplicates
                // can produce identical (kind, decl_id, start, end) tuples. Sort and
                // dedup to ensure each entry appears exactly once.
                sem.ref_entries
                    .sort_unstable_by_key(|(k, d, s, e)| (*k as u8, *d, *s, *e));
                sem.ref_entries.dedup();
            }
            Err(e) => {
                tracing::error!(target: "mcc::code", error = %e, "symbols mutex poisoned (create_lapper)")
            }
        }
        // ★ Layer 1 + name index — build after lapper is complete and lock released.
        self.consolidate_ref_def_map();

        // ★ Layer 2 — merge after Layer 1 so entries aren't overwritten.
        let (scope_snapshot, def_map_snapshot, ref_entries_snapshot) = self
            .symbols
            .lock()
            .ok()
            .map(|s| {
                let scope_map: std::collections::HashMap<(usize, usize), String> = s
                    .local_table
                    .name_to_declare_id
                    .iter()
                    .map(|((_fid, cid, fnid, _n), (_, loc))| {
                        let scope = crate::ast::ast_semantic::scope_from_ids(
                            &s.container_table,
                            &s.func_table,
                            *cid,
                            *fnid,
                        );
                        ((loc.byte_start as usize, loc.byte_end as usize), scope)
                    })
                    .collect();
                (scope_map, s.def_map.clone(), s.ref_entries.clone())
            })
            .unwrap_or_else(|| {
                (
                    std::collections::HashMap::new(),
                    std::collections::HashMap::new(),
                    Vec::new(),
                )
            });
        if let Ok(mut sem) = self.symbols.lock() {
            let file_table = sem.file_table.clone(); // clone before mutable borrow
            if let Some(ref mut map) = sem.ref_def_map {
                crate::refdef::matching::fill_refdef_layer2(
                    map,
                    &scope_snapshot,
                    &def_map_snapshot,
                    &ref_entries_snapshot,
                    &self.uri,
                    &file_table,
                );
                // ★ §3.5.4: Upgrade UnknownDef → inferred type
                Self::upgrade_unknown_defs(map, &self.uri);
            }
        }
    }

    /// ★ §3.5.4: Upgrade UnknownDef entries based on param type inference.
    fn upgrade_unknown_defs(map: &mut crate::ast::ast_semantic::RefDefMap, uri: &McURI) {
        use crate::refdef::types::SymbolKind;
        use crate::semantic::basic::mc_param_type::McParamTypeKind;

        // Collect (name → McParamTypeKind) from all containers for this URI
        let mut param_types: std::collections::HashMap<String, McParamTypeKind> =
            std::collections::HashMap::new();

        let collect =
            |params: &crate::semantic::basic::mc_paramd::McParamDeclares,
             acc: &mut std::collections::HashMap<String, McParamTypeKind>| {
                for (name, _span) in params.iter_defs_with_span() {
                    if let Some(decl) = params.find(name) {
                        acc.insert(name.to_string(), decl.param_type.kind.clone());
                    }
                }
            };

        // Modules
        for entry in crate::db::cmie::tables::WORKSPACE.modules.iter() {
            if entry.key().uri.as_str() == uri.as_str() {
                collect(&entry.value().params, &mut param_types);
            }
        }
        // Components
        for entry in crate::db::cmie::tables::WORKSPACE.components.iter() {
            if entry.key().uri.as_str() == uri.as_str() {
                collect(&entry.value().params, &mut param_types);
                for func in entry.value().funcs.iter() {
                    collect(&func.params, &mut param_types);
                }
            }
        }
        // Interfaces
        for entry in crate::db::cmie::tables::WORKSPACE.interfaces.iter() {
            if entry.key().uri.as_str() == uri.as_str() {
                collect(&entry.value().params, &mut param_types);
            }
        }
        // Func params (nested inside modules)
        for entry in crate::db::cmie::tables::WORKSPACE.modules.iter() {
            if entry.key().uri.as_str() == uri.as_str() {
                for func in entry.value().funcs.iter() {
                    collect(&func.params, &mut param_types);
                }
            }
        }

        if param_types.is_empty() {
            return;
        }

        // McParamTypeKind → SymbolKind mapping
        let kind_map = |k: &McParamTypeKind| -> SymbolKind {
            match k {
                McParamTypeKind::Label | McParamTypeKind::Idx => SymbolKind::LabelDef,
                McParamTypeKind::Interface { .. }
                | McParamTypeKind::InterfaceWithRole { .. }
                | McParamTypeKind::ComponentInstance { .. } => SymbolKind::PortDef,
                McParamTypeKind::Unknown => SymbolKind::UnknownDef,
                _ => SymbolKind::ParamDef,
            }
        };

        // Find and upgrade UnknownDef entries
        let mut upgrades: Vec<((SymbolKind, u32), SymbolKind)> = Vec::new();
        for ((kind, ref_id), entry) in &map.entries {
            if entry.def_kind != SymbolKind::UnknownDef {
                continue;
            }
            for (name, pt_kind) in &param_types {
                let new_kind = kind_map(pt_kind);
                if new_kind == SymbolKind::UnknownDef {
                    continue;
                }
                // Check if this param's def_loc matches the entry's def_loc
                if let Some(name_entry) = map.name_index.get(&(uri.to_string(), name.to_string())) {
                    if name_entry.def_loc.byte_start == entry.def_loc.byte_start
                        && name_entry.def_loc.byte_end == entry.def_loc.byte_end
                    {
                        upgrades.push(((*kind, *ref_id), new_kind));
                        break;
                    }
                }
            }
        }

        for ((old_kind, ref_id), new_kind) in upgrades {
            if let Some(mut entry) = map.entries.remove(&(old_kind, ref_id)) {
                entry.def_kind = new_kind;
                map.insert(new_kind, ref_id, entry);
            }
        }
    }

    pub fn pass2(&mut self) {}

    /// Re-resolve a class reference at a given span in the source file.
    /// Used when the class couldn't be resolved during parsing (sentinel entry
    /// with class_id=0) — at lapper creation time all dependency files have
    /// been parsed, so we can search workspace + global library tables.
    /// If found, ensures the class is registered in `gt` and returns the real
    /// (class_id, target_uri, target_span). Returns None if still unresolved.
    ///
    /// `class_name` is the AST-derived `McIds` captured at registration time —
    /// no disk re-read and no flattened-string rebuild needed (the latter
    /// would collapse dotted names such as `MCU.US513_20_F` from `[Ida, DotIda]`
    /// into a single `Ida`, breaking the structural `Eq` used by the tables).
    fn resolve_class_ref_at_span(
        ref_uri: &McURI,
        class_name: &McIds,
        gt: &mut crate::ast::ast_semantic::GlobalSymbolTable,
        _sem: &McSemSymbols,
    ) -> Option<(DeclareId, McURI, std::ops::Range<usize>)> {
        if class_name.segments.is_empty() {
            return None;
        }

        // ★ Use mcb_get_cmie with five-layer priority (P1–P5) instead of
        // manual table-by-table searches. mcb_get_cmie already implements:
        //   P1: RefDefMap ID-based lookup (all scopes)
        //   P2: Name-based Use table lookup (P3→P4→P5 priority)
        //   P3: Single DashMap.get via cmie_kind
        //   P4: Re-entry guard → name-only fallback
        //   P5: find_by_name_in_project_tables (final fallback)
        if let Some(cmie) = crate::db::cmie::cmie::mcb_get_cmie(class_name, ref_uri) {
            let (def_uri, def_span) = match &cmie {
                crate::semantic::common::McCMIE::Component(c) => (c.uri.clone(), c.span.clone()),
                crate::semantic::common::McCMIE::Module(m) => (m.uri.clone(), m.span.clone()),
                crate::semantic::common::McCMIE::Interface(i) => (i.uri.clone(), i.span.clone()),
                crate::semantic::common::McCMIE::Enum(e) => {
                    let s = e.span;
                    (e.uri.clone(), s[0] as usize..s[1] as usize)
                }
            };
            // Check if already registered; if so return existing id,
            // otherwise register now. Key is (McURI, McIds): O(1) lookup via
            // normalized McIds Eq/Hash (DotIda/Curly equivalence).
            let cid = gt
                .class_name_to_id
                .get(&(def_uri.clone(), class_name.clone()))
                .copied()
                .unwrap_or_else(|| gt.add_class(&def_uri, class_name, def_span.clone()));
            return Some((cid, def_uri, def_span));
        }

        None
    }

    fn lapper_global_classes(
        uri: &McURI,
        cross_file_targets: &mut Vec<(
            crate::ast::ast_semantic::DeclareId,
            McURI,
            std::ops::Range<usize>,
        )>,
        sem: &mut McSemSymbols,
        symbol_lapper: &mut DedupLapper,
    ) {
        match sem.global_table.lock() {
            Ok(mut gt) => {
                let clsids: Vec<_> = gt
                    .class_name_to_id
                    .iter()
                    .filter(|((u, _clsname), _clsid)| u == uri)
                    .map(|(_key, clsid)| *clsid)
                    .collect();

                for clsid in &clsids {
                    if let Some((_uri, span)) = gt.class_id_to_span.get(clsid) {
                        let id = u32::from(*clsid);
                        symbol_lapper.insert(Interval {
                            start: span.start,
                            stop: span.end,
                            val: SymbolType::new(SymbolKind::ClassDef, id),
                        });
                        // ★ Fix: register ClassDef in def_map so fill_refdef_layer2
                        // can resolve ClassRef → ClassDef lookups. Without this,
                        // ClassRef entries in ref_entries never find their def.
                        let file_id =
                            crate::ast::ast_semantic::intern(&mut sem.file_table, uri.as_str());
                        sem.def_map.insert(
                            (SymbolKind::ClassDef, id),
                            crate::ast::ast_semantic::SourceLocation::new(
                                file_id,
                                0,
                                span.start as u32,
                                span.end as u32,
                            ),
                        );
                    }
                }

                {
                    let mut decl_refs = crate::db::cmie::tables::WORKSPACE
                        .lsp
                        .declare_class_refs
                        .lock()
                        .unwrap();
                    tracing::info!(target: "mcc::lsp", "  create_lapper: lsp.declare_class_refs for '{}' = {} entries", uri, decl_refs.get(uri).map(|v| v.len()).unwrap_or(0));
                    if let Some(refs) = decl_refs.remove(uri) {
                        for (decl_span, _class_id, target_uri, target_span, class_name) in refs {
                            // ★ Fix (unified): Register each class ref with a
                            // locally-unique DeclareId.
                            //
                            // _class_id from declare_class_refs is the class_id
                            // from the DEFINING file's per-file table. Using it
                            // directly in the current file's class_id_to_span
                            // lookup collides with local classes that happen to
                            // share the same id.
                            //
                            // Instead we extract the class name from the source
                            // and register it in the local GlobalSymbolTable,
                            // which assigns a unique DeclareId. target_uri and
                            // target_span from declare_class_refs are already
                            // correct (set by mcb_register_declare_class) —
                            // no need to re-resolve via mcb_get_cmie.
                            //
                            // Sentinel (target_uri=""): class was NOT found
                            // during registration. Fall back to late resolution
                            // via resolve_class_ref_at_span.
                            //
                            // ★ class_name is captured at registration time
                            // (mcb_register_declare_class) instead of being
                            // re-read from disk here — disk reads fail for
                            // in-memory buffers / virtual URIs (LSP didOpen).

                            if class_name.segments.is_empty() {
                                continue;
                            }

                            let (local_class_id, ref_target_uri, ref_target_span) = if target_uri
                                .is_empty()
                            {
                                // Sentinel: unresolved during registration
                                if let Some(resolved) =
                                    Self::resolve_class_ref_at_span(uri, &class_name, &mut gt, &sem)
                                {
                                    (resolved.0, resolved.1, resolved.2)
                                } else {
                                    continue;
                                }
                            } else {
                                // Normal case: target_uri/target_span are
                                // already correct. Register in local table
                                // to get a locally-unique DeclareId.
                                let cid = {
                                    let mut found = None;
                                    for ((u, name), &existing_cid) in gt.class_name_to_id.iter() {
                                        if name == &class_name && u == &target_uri {
                                            found = Some(existing_cid);
                                            break;
                                        }
                                    }
                                    found.unwrap_or_else(|| {
                                        gt.add_class(&target_uri, &class_name, target_span.clone())
                                    })
                                };
                                (cid, target_uri, target_span)
                            };

                            let _refid =
                                gt.add_declare_class(&uri, decl_span.clone(), local_class_id);
                            // ★ Fix: push class_id (DeclareId) instead of refid
                            // (ReferenceId) so Layer 1c uses the same ID space as
                            // the lapper and ref_entries.
                            cross_file_targets.push((
                                local_class_id,
                                ref_target_uri,
                                ref_target_span,
                            ));
                        }
                    }
                }

                for ((loop_uri, span), refid) in gt.span_to_declare_class_id.iter() {
                    if loop_uri == uri {
                        // ★ Fix: use class_id (DeclareId) in BOTH lapper and ref_entries.
                        // Previously lapper used refid while ref_entries used class_id.
                        // This mismatch meant F12's map.lookup(ClassRef, refid) could
                        // never find the RefDefMap entry keyed by (ClassRef, class_id).
                        // fill_refdef_layer2 uses class_id as the key, so lapper must
                        // also use class_id for the lookup to match.
                        let class_id = gt
                            .declare_id_to_class_id
                            .get(refid)
                            .copied()
                            .unwrap_or(DeclareId::new(0));
                        symbol_lapper.insert(Interval {
                            start: span.start,
                            stop: span.end,
                            val: SymbolType::new(SymbolKind::ClassRef, u32::from(class_id)),
                        });
                        sem.ref_entries.push((
                            SymbolKind::ClassRef,
                            u32::from(class_id),
                            span.start,
                            span.end,
                        ));
                    }
                }

                for ((loop_uri, _name), class_id) in gt.enum_class_name_to_id.iter() {
                    if loop_uri != uri {
                        continue;
                    }
                    if let Some((_u, span)) = gt.enum_class_id_to_span.get(class_id) {
                        let id = u32::from(*class_id);
                        symbol_lapper.insert(Interval {
                            start: span.start,
                            stop: span.end,
                            val: SymbolType::new(SymbolKind::ClassDef, id),
                        });
                        // ★ Fix: register enum ClassDef in def_map
                        let file_id =
                            crate::ast::ast_semantic::intern(&mut sem.file_table, uri.as_str());
                        sem.def_map.insert(
                            (SymbolKind::ClassDef, id),
                            crate::ast::ast_semantic::SourceLocation::new(
                                file_id,
                                0,
                                span.start as u32,
                                span.end as u32,
                            ),
                        );
                    }
                }
                for (value_id, (loop_uri, span)) in gt.enum_value_id_to_span.iter() {
                    if loop_uri == uri {
                        symbol_lapper.insert(Interval {
                            start: span.start,
                            stop: span.end,
                            val: SymbolType::new(SymbolKind::EnumValDef, u32::from(*value_id)),
                        });
                    }
                }
            }
            Err(e) => {
                tracing::error!(target: "mcc::code", error = %e, "global_table mutex poisoned (create_lapper)")
            }
        }
    }

    /// Register module-level `declare_instance` declarations as InstDef,
    /// and `inst_id_to_span` references as InstRef.
    fn lapper_instance_decls_and_refs(
        uri: &McURI,
        sem: &mut McSemSymbols,
        symbol_lapper: &mut DedupLapper,
    ) {
        // ── InstDef: module declare_instance (e.g. `MCU.US513_20_F uC`) ──
        let modules = &crate::db::cmie::tables::WORKSPACE.modules;
        for entry in modules.iter() {
            if entry.key().uri.as_str() != uri.as_str() {
                continue;
            }
            let m = entry.value();
            let mod_ident = entry.key().ident.to_string();
            for (inst_name, (_iotype, inst)) in m.insts.insts() {
                match inst {
                    crate::semantic::mc_inst::McInstance::Component(_)
                    | crate::semantic::mc_inst::McInstance::Module(_) => {
                        if let Some(spans) = m.insts.port_spans().get(inst_name) {
                            for span in spans {
                                let (d, _) = crate::refdef::register::register_def(
                                    sem,
                                    uri,
                                    &mod_ident,
                                    None,
                                    inst_name,
                                    span.clone(),
                                    SymbolKind::InstDef,
                                );
                                symbol_lapper.insert(Interval {
                                    start: span.start,
                                    stop: span.end,
                                    val: SymbolType::new(SymbolKind::InstDef, u32::from(d)),
                                });
                                tracing::info!(target: "mcc::lsp::audit",
                                    "[AUDIT-InstDef] name={inst_name} span={span:?} decl_id={d:?}");
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // ── InstRef: references from inst_id_to_span ──
        for (inst_id, span) in sem.local_table.inst_id_to_span.iter() {
            let decl_id = sem
                .local_table
                .inst_id_to_declare_inst
                .get(inst_id)
                .copied()
                .unwrap_or(DeclareId::default());
            symbol_lapper.insert(Interval {
                start: span.start,
                stop: span.end,
                val: SymbolType::new(SymbolKind::InstRef, u32::from(decl_id)),
            });
            tracing::info!(target: "mcc::lsp::audit",
                "[AUDIT-InstRef] inst_id={inst_id:?} span={span:?} decl_id={decl_id:?}");
            sem.ref_entries.push((
                SymbolKind::InstRef,
                u32::from(decl_id),
                span.start,
                span.end,
            ));
        }
    }

    fn lapper_interfaces(uri: &McURI, sem: &mut McSemSymbols, symbol_lapper: &mut DedupLapper) {
        let uri_str = uri.as_str();

        // Note: ClassDef intervals for interfaces are now created by
        // lapper_global_classes via gt.class_name_to_id (populated by
        // add_global_class in parse_pass1_types).  This function only
        // handles interface-internal symbols: params, attrs, net refs.

        let interfaces = &crate::db::cmie::tables::WORKSPACE.interfaces;
        for entry in interfaces.iter() {
            let iface = entry.value();
            if iface.uri.as_str() == uri_str {
                let mut param_decl_ids: std::collections::HashMap<String, DeclareId> =
                    std::collections::HashMap::new();
                let iface_ident = iface.name.to_string();
                for (name, span) in iface.params.iter_defs_with_span() {
                    let def_kind = Self::param_def_kind(iface.params.find(name));
                    let (d, _) = crate::refdef::register::register_def(
                        &mut *sem,
                        &uri,
                        &iface_ident,
                        None,
                        name,
                        span.clone(),
                        def_kind,
                    );
                    param_decl_ids.insert(name.to_string(), d);
                    symbol_lapper.insert(Interval {
                        start: span.start,
                        stop: span.end,
                        val: SymbolType::new(def_kind, u32::from(d)),
                    });
                }
                for attr in iface.attrs.iter() {
                    for val in &attr.values {
                        if let crate::semantic::component::mc_attr::McAttrVal::AttrVariable(
                            opd,
                            Some(span),
                        ) = val
                        {
                            let var_name = opd.to_string();
                            let decl_id = param_decl_ids
                                .get(&var_name)
                                .copied()
                                .unwrap_or(DeclareId::new(0));
                            sem.local_table.add_inst(span.clone(), decl_id);
                        }
                    }
                }
                for (span, port_name, scope) in iface.params.iter_net_refs() {
                    let sp = crate::refdef::register::scope_path_from_scope_str(&uri, scope);
                    let decl_id = crate::refdef::register::lookup_declare_id(
                        &sem.local_table,
                        port_name,
                        &sp,
                    );
                    if let Some(decl_id) = decl_id {
                        symbol_lapper.insert(Interval {
                            start: span.start,
                            stop: span.end,
                            val: SymbolType::new(SymbolKind::PortRef, u32::from(decl_id)),
                        });
                        sem.ref_entries.push((
                            SymbolKind::PortRef,
                            u32::from(decl_id),
                            span.start,
                            span.end,
                        ));
                    }
                }
            }
        }
        let global_interfaces = &crate::db::infra::global::mcc_interfaces;
        for entry in global_interfaces.iter() {
            let iface = entry.value();
            if iface.uri.as_str() == uri_str {
                let iface_name_g = iface.name.to_string();
                let mut param_decl_ids: std::collections::HashMap<String, DeclareId> =
                    std::collections::HashMap::new();
                for (name, span) in iface.params.iter_defs_with_span() {
                    let def_kind = Self::param_def_kind(iface.params.find(name));
                    let (d, _) = crate::refdef::register::register_def(
                        &mut *sem,
                        &uri,
                        &iface_name_g,
                        None,
                        name,
                        span.clone(),
                        def_kind,
                    );
                    param_decl_ids.insert(name.to_string(), d);
                    symbol_lapper.insert(Interval {
                        start: span.start,
                        stop: span.end,
                        val: SymbolType::new(def_kind, u32::from(d)),
                    });
                }
                for attr in iface.attrs.iter() {
                    for val in &attr.values {
                        if let crate::semantic::component::mc_attr::McAttrVal::AttrVariable(
                            opd,
                            Some(span),
                        ) = val
                        {
                            let var_name = opd.to_string();
                            let decl_id = param_decl_ids
                                .get(&var_name)
                                .copied()
                                .unwrap_or(DeclareId::new(0));
                            sem.local_table.add_inst(span.clone(), decl_id);
                        }
                    }
                }
                for (span, port_name, scope) in iface.params.iter_net_refs() {
                    let sp = crate::refdef::register::scope_path_from_scope_str(&uri, scope);
                    let decl_id = crate::refdef::register::lookup_declare_id(
                        &sem.local_table,
                        port_name,
                        &sp,
                    );
                    if let Some(decl_id) = decl_id {
                        symbol_lapper.insert(Interval {
                            start: span.start,
                            stop: span.end,
                            val: SymbolType::new(SymbolKind::InstRef, u32::from(decl_id)),
                        });
                        sem.ref_entries.push((
                            SymbolKind::InstRef,
                            u32::from(decl_id),
                            span.start,
                            span.end,
                        ));
                    }
                }
            }
        }
    }

    fn lapper_module_ports(uri: &McURI, sem: &mut McSemSymbols, symbol_lapper: &mut DedupLapper) {
        let modules = &crate::db::cmie::tables::WORKSPACE.modules;
        for entry in modules.iter() {
            let m = entry.value();
            if entry.key().uri.as_str() != uri.as_str() {
                continue;
            }

            tracing::debug!(
                target: "mcc::lsp",
                "[LAPPER_DEBUG] Processing module params: {}",
                entry.key().ident
            );
            let param_def_count = m.params.iter_defs_with_span().count();
            tracing::debug!(
                target: "mcc::lsp",
                "[LAPPER_DEBUG] module={}, param_def_count={}",
                entry.key().ident,
                param_def_count
            );
            let mod_ident = entry.key().ident.to_string();
            for (name, span) in m.params.iter_defs_with_span() {
                // ★ Rule 6: untyped params → UnknownDef, typed → ParamDef.
                // Square-vec members (e.g. VDD_3V3 inside `[VDD_3V3,GND]::DC(3.3V)`)
                // register as LabelDef instead (§3.4.3 full registration).
                let def_kind = if m.params.is_square_member(name) {
                    SymbolKind::LabelDef
                } else {
                    Self::param_def_kind(m.params.find(name))
                };
                let (d, _) = crate::refdef::register::register_def(
                    &mut *sem,
                    &uri,
                    &mod_ident,
                    None,
                    name,
                    span.clone(),
                    def_kind,
                );
                symbol_lapper.insert(Interval {
                    start: span.start,
                    stop: span.end,
                    val: SymbolType::new(def_kind, u32::from(d)),
                });
                tracing::info!(target: "mcc::lsp::audit",
                    "[AUDIT-ParamDef] name={name} span={span:?} decl_id={d:?} kind={def_kind:?}");
            }

            let mod_ident2 = entry.key().ident.to_string();
            for (name, _iotype, span) in m.insts.iter_ports_with_span() {
                let (d, _) = crate::refdef::register::register_def(
                    &mut *sem,
                    &uri,
                    &mod_ident2,
                    None,
                    name,
                    span.clone(),
                    SymbolKind::PortDef,
                );
                symbol_lapper.insert(Interval {
                    start: span.start,
                    stop: span.end,
                    val: SymbolType::new(SymbolKind::PortDef, u32::from(d)),
                });
                tracing::info!(target: "mcc::lsp::audit",
                    "[AUDIT-PortDef] name={name} span={span:?} decl_id={d:?}");
            }
            // ★ §3.4.3 (rev): named curly-bus member defs — expand each
            // registered BusDef into per-member BusMemberDef lapper entries.
            // Lookup key is the full member name `MIC.P`; span points at the
            // member text so `MIC.P` resolves directly to `P` in `{P,N}`.
            for bus in m.insts.iter_bus_defs() {
                for (member_name, mspan) in &bus.members {
                    let full = format!("{}.{}", bus.name, member_name);
                    let (d, _) = crate::refdef::register::register_def(
                        sem,
                        &uri,
                        &mod_ident2,
                        None,
                        &full,
                        mspan.clone(),
                        SymbolKind::BusMemberDef,
                    );
                    symbol_lapper.insert(Interval {
                        start: mspan.start,
                        stop: mspan.end,
                        val: SymbolType::new(SymbolKind::BusMemberDef, u32::from(d)),
                    });
                    tracing::info!(target: "mcc::lsp::audit",
                        "[AUDIT-BusMemberDef] name={full} span={mspan:?} decl_id={d:?}");
                }
            }
            // ★ Square-vec member defs — register port_spans entries
            // not covered by iter_ports_with_span (IOType::None members).
            // Skip Component/Module instances — they are already registered
            // as InstDef by lapper_instance_decls_and_refs above.
            for (name, spans) in m.insts.port_spans() {
                // Skip if this name is a Component/Module instance
                if let Some((_io, inst)) = m.insts.insts().get(name) {
                    if matches!(
                        inst,
                        crate::semantic::mc_inst::McInstance::Component(_)
                            | crate::semantic::mc_inst::McInstance::Module(_)
                    ) {
                        tracing::info!(target: "mcc::lsp::audit",
                            "[AUDIT-LabelDef-SKIP] name={name} (is Component/Module instance)");
                        continue;
                    }
                }
                for span in spans {
                    let (d, _) = crate::refdef::register::register_def(
                        sem,
                        uri,
                        &mod_ident2,
                        None,
                        name,
                        span.clone(),
                        SymbolKind::LabelDef,
                    );
                    symbol_lapper.insert(Interval {
                        start: span.start,
                        stop: span.end,
                        val: SymbolType::new(SymbolKind::LabelDef, u32::from(d)),
                    });
                    tracing::info!(target: "mcc::lsp::audit",
                        "[AUDIT-LabelDef] name={name} span={span:?} decl_id={d:?}");
                }
            }
            for (span, port_name, scope) in m.insts.iter_net_refs() {
                let sp = crate::refdef::register::scope_path_from_scope_str(&uri, scope);
                let decl_id =
                    crate::refdef::register::lookup_declare_id(&sem.local_table, port_name, &sp);
                tracing::info!(target: "mcc::lsp::audit",
                    "[AUDIT-NetRef] name={port_name} span={span:?} scope={scope} decl_id={decl_id:?}");
                // ★ Phase 3 (cross-container member chain): full member names
                // like `U_MCU.I2C0.SCL` are absent from the local table when
                // their def lives in the class-definition file (e.g. an I2C
                // interface pin in the mcode library). Fall back to structural
                // member-chain resolution and register the member def with its
                // cross-file SourceLocation so goto-def jumps to that file.
                let mut ref_kind = Self::resolve_net_ref_kind(port_name, &m.insts);
                let mut use_decl_id = decl_id;
                if decl_id.is_none() {
                    if let Some(hit) = crate::refdef::chain::resolve_member_chain(
                        &uri, port_name, &m.insts, &m.params,
                    ) {
                        let (d, _) = crate::refdef::register::register_def(
                            sem,
                            &hit.uri,
                            scope,
                            None,
                            &hit.name,
                            hit.span.clone(),
                            hit.def_kind,
                        );
                        ref_kind = Self::chain_ref_kind(hit.def_kind);
                        use_decl_id = Some(d);
                        tracing::info!(target: "mcc::lsp::audit",
                            "[AUDIT-NetRef-Chain] name={port_name} → {} kind={ref_kind:?} def_uri={} def_span={:?} decl_id={d:?}",
                            hit.name, hit.uri, hit.span);
                    }
                }
                if let Some(decl_id) = use_decl_id {
                    // ★ §4.3 #22-#26: Dispatch inst.member refs to correct type
                    tracing::info!(target: "mcc::lsp::audit",
                        "[AUDIT-NetRef-Kind] name={port_name} ref_kind={ref_kind:?}");
                    symbol_lapper.insert(Interval {
                        start: span.start,
                        stop: span.end,
                        val: SymbolType::new(ref_kind, u32::from(decl_id)),
                    });
                    sem.ref_entries
                        .push((ref_kind, u32::from(decl_id), span.start, span.end));
                }
            }
            // ★ Chain references: AST-structured segments for cross-container
            // member resolution (e.g. `uC.ADC{P,N}`, `uC.19`, `uC.i2c(0x36).I2C0`).
            // Recorded by try_record_chain_ref from the AST; resolved here so
            // goto-def / hover land on the member text.
            for (span, segments, scope) in m.insts.iter_chain_refs() {
                if let Some(hit) = crate::refdef::chain::resolve_member_chain_from_segments(
                    &uri, segments, &m.insts, &m.params,
                ) {
                    let ref_kind = Self::chain_ref_kind(hit.def_kind);
                    let (d, _) = crate::refdef::register::register_def(
                        sem,
                        &hit.uri,
                        scope,
                        None,
                        &hit.name,
                        hit.span.clone(),
                        hit.def_kind,
                    );
                    symbol_lapper.insert(Interval {
                        start: span.start,
                        stop: span.end,
                        val: SymbolType::new(ref_kind, u32::from(d)),
                    });
                    sem.ref_entries
                        .push((ref_kind, u32::from(d), span.start, span.end));
                    tracing::info!(target: "mcc::lsp::audit",
                        "[AUDIT-ChainRef] segments={segments:?} → {} kind={ref_kind:?} def_uri={} def_span={:?} decl_id={d:?}",
                        hit.name, hit.uri, hit.span);
                }
            }
            for (span, port_name, scope) in m.params.iter_net_refs() {
                let sp = crate::refdef::register::scope_path_from_scope_str(&uri, scope);
                let decl_id =
                    crate::refdef::register::lookup_declare_id(&sem.local_table, port_name, &sp);
                if let Some(decl_id) = decl_id {
                    symbol_lapper.insert(Interval {
                        start: span.start,
                        stop: span.end,
                        val: SymbolType::new(SymbolKind::InstRef, u32::from(decl_id)),
                    });
                    sem.ref_entries.push((
                        SymbolKind::InstRef,
                        u32::from(decl_id),
                        span.start,
                        span.end,
                    ));
                }
            }
            let mod_ident_label = entry.key().ident.to_string();
            for (name, _label_kind, span) in m.insts.iter_labels_with_span() {
                let (d, _) = crate::refdef::register::register_def(
                    sem,
                    uri,
                    &mod_ident_label,
                    None,
                    name,
                    span.clone(),
                    SymbolKind::LabelDef,
                );
                symbol_lapper.insert(Interval {
                    start: span.start,
                    stop: span.end,
                    val: SymbolType::new(SymbolKind::LabelDef, u32::from(d)),
                });
            }
            // ★ §3.2.4 #5: Register bus definitions (e.g. power{VCC,GND}, MIC{P,N})
            for (inst_name, (_iotype, inst)) in m.insts.insts() {
                if let crate::semantic::mc_inst::McInstance::Bus(_) = inst {
                    if let Some(spans) = m.insts.port_spans().get(inst_name) {
                        for span in spans {
                            let (d, _) = crate::refdef::register::register_def(
                                sem,
                                uri,
                                &mod_ident_label,
                                None,
                                inst_name,
                                span.clone(),
                                SymbolKind::BusDef,
                            );
                            symbol_lapper.insert(Interval {
                                start: span.start,
                                stop: span.end,
                                val: SymbolType::new(SymbolKind::BusDef, u32::from(d)),
                            });
                        }
                    }
                }
            }
        }
    }

    fn lapper_function_params(
        uri: &McURI,
        sem: &mut McSemSymbols,
        symbol_lapper: &mut DedupLapper,
    ) {
        let modules = &crate::db::cmie::tables::WORKSPACE.modules;
        for entry in modules.iter() {
            let m = entry.value();
            if entry.key().uri.as_str() != uri.as_str() {
                continue;
            }
            for func in m.funcs.iter() {
                let fscope = func.name.to_string();
                for (span, port_name, scope) in func.params.iter_net_refs() {
                    let sp = crate::refdef::register::scope_path_from_scope_str(&uri, scope);
                    let decl_id = crate::refdef::register::lookup_declare_id(
                        &sem.local_table,
                        port_name,
                        &sp,
                    );
                    if let Some(decl_id) = decl_id {
                        symbol_lapper.insert(Interval {
                            start: span.start,
                            stop: span.end,
                            val: SymbolType::new(SymbolKind::LabelRef, u32::from(decl_id)),
                        });
                        sem.ref_entries.push((
                            SymbolKind::LabelRef,
                            u32::from(decl_id),
                            span.start,
                            span.end,
                        ));
                    }
                }
                for (span, port_name, scope) in func.insts.iter_net_refs() {
                    let sp = crate::refdef::register::scope_path_from_scope_str(&uri, scope);
                    let decl_id = crate::refdef::register::lookup_declare_id(
                        &sem.local_table,
                        port_name,
                        &sp,
                    );
                    if let Some(decl_id) = decl_id {
                        // ★ Use module insts for resolve_net_ref_kind because
                        // function bodies can reference module-level instances
                        // (e.g. uC) that are not in func.insts.
                        let ref_kind = Self::resolve_net_ref_kind(port_name, &m.insts);
                        symbol_lapper.insert(Interval {
                            start: span.start,
                            stop: span.end,
                            val: SymbolType::new(ref_kind, u32::from(decl_id)),
                        });
                        sem.ref_entries
                            .push((ref_kind, u32::from(decl_id), span.start, span.end));
                    }
                }
                // ★ Chain references inside func bodies (e.g. `spi + uC.SPI`
                // in us513.mc loadFlash). Recorded into `func.insts` by
                // try_record_chain_ref; resolve against module insts because
                // func bodies reference module-level instances (e.g. uC).
                for (span, segments, scope) in func.insts.iter_chain_refs() {
                    if let Some(hit) = crate::refdef::chain::resolve_member_chain_from_segments(
                        &uri, segments, &m.insts, &m.params,
                    ) {
                        let ref_kind = Self::chain_ref_kind(hit.def_kind);
                        let (d, _) = crate::refdef::register::register_def(
                            sem,
                            &hit.uri,
                            scope,
                            None,
                            &hit.name,
                            hit.span.clone(),
                            hit.def_kind,
                        );
                        symbol_lapper.insert(Interval {
                            start: span.start,
                            stop: span.end,
                            val: SymbolType::new(ref_kind, u32::from(d)),
                        });
                        sem.ref_entries
                            .push((ref_kind, u32::from(d), span.start, span.end));
                    }
                }
                let func_scope = func.insts.scope.clone().unwrap_or_else(|| fscope.clone());
                for (name, _label_kind, span) in func.insts.iter_labels_with_span() {
                    let (d, _) = crate::refdef::register::register_def(
                        sem,
                        uri,
                        &func_scope,
                        None,
                        name,
                        span.clone(),
                        SymbolKind::LabelDef,
                    );
                    symbol_lapper.insert(Interval {
                        start: span.start,
                        stop: span.end,
                        val: SymbolType::new(SymbolKind::LabelDef, u32::from(d)),
                    });
                }
            }
        }
    }

    fn lapper_component_defs(uri: &McURI, sem: &mut McSemSymbols, symbol_lapper: &mut DedupLapper) {
        let all_comps: Vec<(String, Arc<McComponent>, String)> = workspace::WORKSPACE
            .components
            .iter()
            .map(|e| {
                (
                    e.key().ident.to_string(),
                    e.value().clone(),
                    e.key().uri.to_string(),
                )
            })
            .chain(global::mcc_components.iter().map(|e| {
                (
                    e.key().ident.to_string(),
                    e.value().clone(),
                    e.key().uri.to_string(),
                )
            }))
            .filter(|(_, _, comp_uri)| comp_uri == uri.as_str())
            .collect();
        for (comp_ident, comp, _comp_uri) in &all_comps {
            for (name, span) in comp.params.iter_defs_with_span() {
                let def_kind = Self::param_def_kind(comp.params.find(name));
                let (d, _) = crate::refdef::register::register_def(
                    &mut *sem,
                    &uri,
                    comp_ident,
                    None,
                    name,
                    span.clone(),
                    def_kind,
                );
                symbol_lapper.insert(Interval {
                    start: span.start,
                    stop: span.end,
                    val: SymbolType::new(def_kind, u32::from(d)),
                });
            }
            for (pin_name, mut pin_span) in Self::extract_pin_name_spans(comp) {
                // ★ Fix: AST span may exclude leading/trailing delimiters
                // (parser tokens).  Extend span to cover them so PinNameDef
                // names are complete.
                if let Ok(content) = std::fs::read_to_string(std::path::Path::new(uri.as_str())) {
                    // Trailing ) or } — e.g. "I2C(Master)" not "I2C(Master"
                    if let Some(&ch) = content.as_bytes().get(pin_span.end) {
                        if ch == b')' || ch == b'}' {
                            pin_span.end += 1;
                        }
                    }
                    // Leading [ or { — e.g. "[VDD, GND]" not "VDD, GND]"
                    if pin_span.start > 0 {
                        if let Some(&ch) = content.as_bytes().get(pin_span.start - 1) {
                            if ch == b'[' || ch == b'{' {
                                pin_span.start -= 1;
                            }
                        }
                    }
                }
                let (d, _) = crate::refdef::register::register_def(
                    &mut *sem,
                    &uri,
                    comp_ident,
                    None,
                    &pin_name,
                    pin_span.clone(),
                    SymbolKind::PinNameDef,
                );
                symbol_lapper.insert(Interval {
                    start: pin_span.start,
                    stop: pin_span.end,
                    val: SymbolType::new(SymbolKind::PinNameDef, u32::from(d)),
                });
            }
            for (pin_id, id_span) in Self::extract_pin_id_spans(comp) {
                let (d, _) = crate::refdef::register::register_def(
                    &mut *sem,
                    &uri,
                    comp_ident,
                    None,
                    &pin_id,
                    id_span.clone(),
                    SymbolKind::PinIdDef,
                );
                symbol_lapper.insert(Interval {
                    start: id_span.start,
                    stop: id_span.end,
                    val: SymbolType::new(SymbolKind::PinIdDef, u32::from(d)),
                });
            }
            for (iface, if_span) in Self::extract_pin_iface_spans(comp) {
                let (d, _) = crate::refdef::register::register_def(
                    &mut *sem,
                    &uri,
                    comp_ident,
                    None,
                    &iface,
                    if_span.clone(),
                    SymbolKind::PinIfaceDef,
                );
                symbol_lapper.insert(Interval {
                    start: if_span.start,
                    stop: if_span.end,
                    val: SymbolType::new(SymbolKind::PinIfaceDef, u32::from(d)),
                });
            }
            for (key_name, key_span) in Self::extract_spec_key_spans(comp) {
                let sdecl_id = sem.local_table.add_declare_with_name(
                    &uri,
                    SourceLocation::from_span(&key_span),
                    Some(key_name.clone()),
                    Some(comp_ident),
                );
                symbol_lapper.insert(Interval {
                    start: key_span.start,
                    stop: key_span.end,
                    val: SymbolType::new(SymbolKind::AttrDef, u32::from(sdecl_id)),
                });
            }
            for (span, port_name, scope) in comp.params.iter_net_refs() {
                let sp = crate::refdef::register::scope_path_from_scope_str(&uri, scope);
                let decl_id =
                    crate::refdef::register::lookup_declare_id(&sem.local_table, port_name, &sp);
                if let Some(decl_id) = decl_id {
                    symbol_lapper.insert(Interval {
                        start: span.start,
                        stop: span.end,
                        val: SymbolType::new(SymbolKind::InstRef, u32::from(decl_id)),
                    });
                    sem.ref_entries.push((
                        SymbolKind::InstRef,
                        u32::from(decl_id),
                        span.start,
                        span.end,
                    ));
                }
            }
            let comp_ident_label = comp_ident.clone();
            for (name, _label_kind, span) in comp.insts.iter_labels_with_span() {
                let decl_id = sem.local_table.add_declare_with_name(
                    &uri,
                    SourceLocation::from_span(&span),
                    Some(name.to_string()),
                    Some(&comp_ident_label),
                );
                symbol_lapper.insert(Interval {
                    start: span.start,
                    stop: span.end,
                    val: SymbolType::new(SymbolKind::LabelDef, u32::from(decl_id)),
                });
            }
            // ★ §15.1: Generate PinRef entries from component body references
            let pin_names: std::collections::HashSet<String> = Self::extract_pin_name_spans(comp)
                .into_iter()
                .map(|(n, _)| n)
                .collect();
            let pin_ids: std::collections::HashSet<String> = Self::extract_pin_id_spans(comp)
                .into_iter()
                .map(|(n, _)| n)
                .collect();
            let pin_ifaces: std::collections::HashSet<String> = Self::extract_pin_iface_spans(comp)
                .into_iter()
                .map(|(n, _)| n)
                .collect();
            for (span, port_name, _scope) in comp.insts.iter_net_refs() {
                if pin_names.contains(port_name) {
                    if let Some(decl_id) =
                        sem.local_table.lookup_by_scope_name(&comp_ident, port_name)
                    {
                        symbol_lapper.insert(Interval {
                            start: span.start,
                            stop: span.end,
                            val: SymbolType::new(SymbolKind::PinNameRef, u32::from(decl_id.0)),
                        });
                        sem.ref_entries.push((
                            SymbolKind::PinNameRef,
                            u32::from(decl_id.0),
                            span.start,
                            span.end,
                        ));
                    }
                } else if pin_ids.contains(port_name) {
                    if let Some(decl_id) =
                        sem.local_table.lookup_by_scope_name(&comp_ident, port_name)
                    {
                        symbol_lapper.insert(Interval {
                            start: span.start,
                            stop: span.end,
                            val: SymbolType::new(SymbolKind::PinIdRef, u32::from(decl_id.0)),
                        });
                        sem.ref_entries.push((
                            SymbolKind::PinIdRef,
                            u32::from(decl_id.0),
                            span.start,
                            span.end,
                        ));
                    }
                } else if pin_ifaces.contains(port_name) {
                    if let Some(decl_id) =
                        sem.local_table.lookup_by_scope_name(&comp_ident, port_name)
                    {
                        symbol_lapper.insert(Interval {
                            start: span.start,
                            stop: span.end,
                            val: SymbolType::new(SymbolKind::PinIfaceRef, u32::from(decl_id.0)),
                        });
                        sem.ref_entries.push((
                            SymbolKind::PinIfaceRef,
                            u32::from(decl_id.0),
                            span.start,
                            span.end,
                        ));
                    }
                }
            }
        }
    }

    /// Search cross-file global tables for an enum class by name.
    /// Returns `(def_uri, def_span)` from the defining file's table.
    /// Priority: P3 (current file) → P4 (other workspace files) → P5 (system libs).
    fn find_enum_class_cross_file(
        uri: &McURI,
        sem: &McSemSymbols,
        base_name: &str,
    ) -> Option<(McURI, crate::ast::ast_semantic::Span)> {
        // P3: current file's global table
        if let Ok(gt) = sem.global_table.lock() {
            for ((def_uri, name), class_id) in gt.enum_class_name_to_id.iter() {
                if name == &McIds::from(base_name) {
                    if let Some((_u, span)) = gt.enum_class_id_to_span.get(class_id) {
                        return Some((def_uri.clone(), span.clone()));
                    }
                }
            }
        }

        // P4: other workspace files
        for entry in workspace::WORKSPACE.mcodes.iter() {
            if entry.key() == uri {
                continue;
            }
            if let Ok(ws_sym) = entry.value().symbols.lock() {
                if let Ok(ws_gt) = ws_sym.global_table.lock() {
                    for ((def_uri, name), class_id) in ws_gt.enum_class_name_to_id.iter() {
                        if name == &McIds::from(base_name) {
                            if let Some((_u, span)) = ws_gt.enum_class_id_to_span.get(class_id) {
                                return Some((def_uri.clone(), span.clone()));
                            }
                        }
                    }
                }
            }
        }

        // P5: system libraries
        for entry in crate::db::infra::libmgr::mcc_blibs.iter() {
            if let Ok(ws_sym) = entry.value().symbols.lock() {
                if let Ok(ws_gt) = ws_sym.global_table.lock() {
                    for ((def_uri, name), class_id) in ws_gt.enum_class_name_to_id.iter() {
                        if name == &McIds::from(base_name) {
                            if let Some((_u, span)) = ws_gt.enum_class_id_to_span.get(class_id) {
                                return Some((def_uri.clone(), span.clone()));
                            }
                        }
                    }
                }
            }
        }

        None
    }

    fn lapper_enum_refs(
        uri: &McURI,
        ast: &AstNode,
        sem: &mut McSemSymbols,
        symbol_lapper: &mut DedupLapper,
    ) {
        use crate::ast::ast_semantic::GlobalSymbolTable;
        use rust_lapper::Interval;

        let all_ast_nodes: Vec<AstNode> = {
            let mut acc: Vec<AstNode> = Vec::new();
            let mut stack: Vec<AstNode> = ast.iter().collect();
            while let Some(node) = stack.pop() {
                if let Some(sub) = node.get_sub_node() {
                    for child in sub.iter() {
                        stack.push(child);
                    }
                }
                acc.push(node);
            }
            acc
        };
        'outer: for attr_node in all_ast_nodes.iter().cloned() {
            if !attr_node.is_type(MCAST_ATTRIBUTE) {
                continue;
            }
            // Process all attribute keys for DOT-pattern enum refs (e.g., CAP.X7R)
            let att_id = match attr_node.get_sub_node() {
                Some(s) => s,
                None => continue,
            };
            let values_node = match att_id.get_next() {
                Some(v) => v,
                None => continue,
            };
            if !values_node.is_type(MCAST_ATT_VALUES) {
                continue;
            }
            let values_sub = match values_node.get_sub_node() {
                Some(s) => s,
                None => continue,
            };
            for opd_node in values_sub.iter() {
                let parsed = match extract_dot_pair(&opd_node) {
                    Some(p) => p,
                    None => continue,
                };
                let (base_name, member_name, base_start, base_end, member_start, member_end) =
                    parsed;

                let (class_id, value_idx) = {
                    // Look up enum class_id: local table first, then cross-file.
                    let local_id = match sem.global_table.lock() {
                        Ok(gt) => gt
                            .lookup_enum_class(&uri, &McIds::from(&base_name))
                            .or_else(|| {
                                gt.enum_class_name_to_id
                                    .iter()
                                    .find_map(|((_uri, name), cid)| {
                                        (name == &McIds::from(&base_name)).then_some(*cid)
                                    })
                            }),
                        Err(_) => continue 'outer,
                    };
                    let class_id = if local_id.map_or(false, |id| u32::from(id) != 0) {
                        local_id.unwrap()
                    } else {
                        // Cross-file search: register enum class in local table
                        // to get a locally-unique DeclareId (mirrors how
                        // lapper_global_classes handles cross-file ClassRef).
                        match (
                            Self::find_enum_class_cross_file(uri, sem, &base_name),
                            sem.global_table.lock(),
                        ) {
                            (Some((def_uri, def_span)), Ok(mut gt)) => {
                                gt.add_enum_class(&def_uri, &McIds::from(&base_name), def_span)
                            }
                            _ => DeclareId::default(),
                        }
                    };

                    let mut idx = None;
                    {
                        let enums_guard = &crate::db::cmie::tables::WORKSPACE.enums;
                        for entry in enums_guard.iter() {
                            if entry.key().ident.to_string() != base_name {
                                continue;
                            }
                            for (i, v) in entry.value().values.iter().enumerate() {
                                if v.name.to_string() == member_name {
                                    idx = Some(i as u32);
                                    break;
                                }
                            }
                            break;
                        }
                    }
                    if idx.is_none() {
                        let sys_enums_guard = &crate::db::infra::global::mcc_enums;
                        for entry in sys_enums_guard.iter() {
                            if entry.key().ident.to_string() != base_name {
                                continue;
                            }
                            for (i, v) in entry.value().values.iter().enumerate() {
                                if v.name.to_string() == member_name {
                                    idx = Some(i as u32);
                                    break;
                                }
                            }
                            break;
                        }
                    }

                    match idx {
                        Some(i) => (class_id, i),
                        None => continue,
                    }
                };
                let value_id = GlobalSymbolTable::pack_enum_value_id(class_id, value_idx);

                symbol_lapper.insert(Interval {
                    start: base_start as usize,
                    stop: base_end as usize,
                    val: SymbolType::new(SymbolKind::EnumRef, u32::from(class_id)),
                });
                sem.ref_entries.push((
                    SymbolKind::EnumRef,
                    u32::from(class_id),
                    base_start as usize,
                    base_end as usize,
                ));
                symbol_lapper.insert(Interval {
                    start: member_start as usize,
                    stop: member_end as usize,
                    val: SymbolType::new(SymbolKind::EnumValRef, u32::from(value_id)),
                });
                sem.ref_entries.push((
                    SymbolKind::EnumValRef,
                    u32::from(value_id),
                    member_start as usize,
                    member_end as usize,
                ));
                tracing::debug!(target: "mcc::enum_ref",
                    "pushed enum_class_ref+enum_value_ref for {base_name}.{member_name} (class_id={class_id:?}, value_id={value_id:?})");
            }
        }
    }

    /// Register EnumValRef for bare identifiers that match scoped enum values.
    ///
    /// When inside `component CAP` or `component CAP.CER`, a bare identifier
    /// like `X7R` that matches a value in `enum CAP` gets an `EnumValRef`
    /// lapper entry pointing to the enum value definition.
    fn lapper_scoped_enum_bare_refs(
        uri: &McURI,
        ast: &AstNode,
        sem: &mut McSemSymbols,
        symbol_lapper: &mut DedupLapper,
    ) {
        use rust_lapper::Interval;

        // Collect all AST nodes via BFS
        let all_nodes: Vec<AstNode> = {
            let mut acc: Vec<AstNode> = Vec::new();
            let mut stack: Vec<AstNode> = ast.iter().collect();
            while let Some(node) = stack.pop() {
                if let Some(sub) = node.get_sub_node() {
                    for child in sub.iter() {
                        stack.push(child);
                    }
                }
                acc.push(node);
            }
            acc
        };

        // Build container stack: track which component encloses each position
        let mut container_stack: Vec<(String, usize)> = Vec::new();
        let mut pos_to_container: Vec<(usize, String)> = Vec::new();
        for node in &all_nodes {
            let ntype = node.get_type();
            let node_start = node.get_pos() as usize;
            let node_end = node_start + node.get_len() as usize;
            while let Some((_, end)) = container_stack.last() {
                if node_start >= *end {
                    container_stack.pop();
                } else {
                    break;
                }
            }
            if ntype == MCAST_COMPONENT {
                if let Some(sub) = node.get_sub_node() {
                    if let Some(name_node) = sub.iter().find(|x| x.is_type(MCAST_NAME)) {
                        if let Some(ids_node) = name_node.get_sub_node() {
                            if let Some(ids) = McIds::new(&ids_node) {
                                container_stack.push((ids.to_string(), node_end));
                            }
                        }
                    }
                }
            }
            if let Some((name, _)) = container_stack.last() {
                pos_to_container.push((node_start, name.clone()));
            }
        }
        pos_to_container.sort_by_key(|(pos, _)| *pos);
        let find_container = move |pos: usize| -> Option<String> {
            pos_to_container
                .iter()
                .take_while(|(p, _)| *p <= pos)
                .last()
                .map(|(_, name)| name.clone())
        };

        // Scan bare identifiers inside component scopes
        for node in &all_nodes {
            let ntype = node.get_type();
            // Only handle bare identifiers — MCAST_ID (single ID) or direct IDA
            if ntype != MCAST_ID && ntype != MCAST_IDA {
                continue;
            }

            let pos = node.get_pos() as usize;
            let comp_name_str = match find_container(pos) {
                Some(name) => name,
                None => continue,
            };

            let bare_name = match McIds::new(node) {
                Some(ids) => ids.to_string(),
                None => continue,
            };
            if bare_name.is_empty() {
                continue;
            }

            let comp_ids = McIds::from(comp_name_str.as_str());
            match crate::db::cmie::cmie::lookup_scoped_enum_value(&bare_name, &comp_ids, uri) {
                Some((_def_uri, _span, value_idx)) => {
                    // Get the enum class to build value_id
                    let family_name = comp_ids.root_name().unwrap_or_default();
                    let class_id = {
                        let local_id = {
                            let gt = match sem.global_table.lock() {
                                Ok(gt) => gt,
                                Err(_) => continue,
                            };
                            gt.lookup_enum_class(uri, &McIds::from(&family_name))
                                .or_else(|| {
                                    gt.enum_class_name_to_id.iter().find_map(
                                        |((_uri, name), cid)| {
                                            (name == &McIds::from(&family_name)).then_some(*cid)
                                        },
                                    )
                                })
                        };
                        if local_id.map_or(false, |id| u32::from(id) != 0) {
                            local_id.unwrap()
                        } else {
                            // Cross-file search + local registration
                            match (
                                Self::find_enum_class_cross_file(uri, sem, &family_name),
                                sem.global_table.lock(),
                            ) {
                                (Some((def_uri, def_span)), Ok(mut gt)) => gt.add_enum_class(
                                    &def_uri,
                                    &McIds::from(&family_name),
                                    def_span,
                                ),
                                _ => DeclareId::default(),
                            }
                        }
                    };
                    if u32::from(class_id) == 0 {
                        continue;
                    }
                    let value_id = crate::ast::ast_semantic::GlobalSymbolTable::pack_enum_value_id(
                        class_id, value_idx,
                    );

                    let end = pos + node.get_len() as usize;
                    symbol_lapper.insert(Interval {
                        start: pos,
                        stop: end,
                        val: SymbolType::new(SymbolKind::EnumValRef, u32::from(value_id)),
                    });
                    sem.ref_entries
                        .push((SymbolKind::EnumValRef, u32::from(value_id), pos, end));
                    tracing::debug!(target: "mcc::enum_ref",
                        "pushed scoped enum bare ref '{}' -> {}.{} (value_id={:?})",
                        bare_name, family_name, bare_name, value_id);
                }
                None => {}
            }
        }
    }

    fn lapper_func_define_role(
        uri: &McURI,
        ast: &AstNode,
        sem: &mut McSemSymbols,
        symbol_lapper: &mut DedupLapper,
    ) {
        let all_nodes: Vec<AstNode> = {
            let mut acc = Vec::new();
            let mut stack: Vec<AstNode> = ast.iter().collect();
            while let Some(node) = stack.pop() {
                if let Some(sub) = node.get_sub_node() {
                    for child in sub.iter() {
                        stack.push(child);
                    }
                }
                acc.push(node);
            }
            acc
        };
        let mut container_names: Vec<String> = Vec::new();
        {
            let uri_str = uri.as_str();
            let modules = &workspace::WORKSPACE.modules;
            for entry in modules.iter() {
                let key_uri = entry.key().uri.as_str();
                if key_uri == uri_str || key_uri.ends_with(uri_str) || uri_str.ends_with(key_uri) {
                    container_names.push(entry.key().ident.to_string());
                }
            }
            let comps = &workspace::WORKSPACE.components;
            for entry in comps.iter() {
                let key_uri = entry.key().uri.as_str();
                if key_uri == uri_str || key_uri.ends_with(uri_str) || uri_str.ends_with(key_uri) {
                    container_names.push(entry.key().ident.to_string());
                }
            }
            for entry in global::mcc_modules.iter() {
                let key_uri = entry.key().uri.as_str();
                if key_uri == uri_str || key_uri.ends_with(uri_str) || uri_str.ends_with(key_uri) {
                    container_names.push(entry.key().ident.to_string());
                }
            }
            for entry in global::mcc_components.iter() {
                let key_uri = entry.key().uri.as_str();
                if key_uri == uri_str || key_uri.ends_with(uri_str) || uri_str.ends_with(key_uri) {
                    container_names.push(entry.key().ident.to_string());
                }
            }
            tracing::info!(target: "mcc::lsp",
                "create_lapper scope: uri={uri_str}, found {} containers: {:?}",
                container_names.len(), container_names);
        }
        let mut container_stack: Vec<(String, usize)> = Vec::new();
        let mut pos_to_container: Vec<(usize, String)> = Vec::new();
        for node in &all_nodes {
            let ntype = node.get_type();
            let node_start = node.get_pos() as usize;
            let node_end = node_start + node.get_len() as usize;
            while let Some((_, end)) = container_stack.last() {
                if node_start >= *end {
                    container_stack.pop();
                } else {
                    break;
                }
            }
            if ntype == MCAST_MODULE || ntype == MCAST_COMPONENT {
                if let Some(sub) = node.get_sub_node() {
                    if let Some(name_node) = sub.iter().find(|x| x.is_type(MCAST_NAME)) {
                        if let Some(ids_node) = name_node.get_sub_node() {
                            if let Some(ids) = McIds::new(&ids_node) {
                                container_stack.push((ids.to_string(), node_end));
                            }
                        }
                    }
                }
            }
            if let Some((name, _)) = container_stack.last() {
                pos_to_container.push((node_start, name.clone()));
            }
        }
        pos_to_container.sort_by_key(|(pos, _)| *pos);
        let find_container = move |pos: usize| -> Option<String> {
            pos_to_container
                .iter()
                .take_while(|(p, _)| *p <= pos)
                .last()
                .map(|(_, name)| name.clone())
        };

        for node in &all_nodes {
            if node.get_type() == MCAST_FUNCTION {
                let ids_node = node.get_sub_node().and_then(|n| n.get_sub_node());
                let span = if let Some(ref ids) = ids_node {
                    (
                        ids.get_pos() as usize,
                        (ids.get_pos() + ids.get_len()) as usize,
                    )
                } else if let Some(name_node) = node.get_sub_node() {
                    (
                        name_node.get_pos() as usize,
                        (name_node.get_pos() + name_node.get_len()) as usize,
                    )
                } else {
                    continue;
                };
                if let Some(name_node) = node.get_sub_node() {
                    let enclosing = find_container(span.0);
                    let func_name = ids_node
                        .and_then(|n| crate::semantic::basic::mc_ids::McIds::new(&n))
                        .map(|ids| ids.to_string());
                    let scope = match (&enclosing, &func_name) {
                        (Some(m), Some(f)) => Some(format!("{m}.{f}")),
                        _ => func_name.clone(),
                    };
                    let (d, _) = crate::refdef::register::register_def(
                        &mut *sem,
                        &uri,
                        enclosing.as_deref().unwrap_or(""),
                        func_name.as_deref(),
                        func_name.as_deref().unwrap_or("?"),
                        span.0..span.1,
                        SymbolKind::FuncDef,
                    );
                    symbol_lapper.insert(Interval {
                        start: span.0,
                        stop: span.1,
                        val: SymbolType::new(SymbolKind::FuncDef, u32::from(d)),
                    });
                    if let Some(params_node) = node
                        .get_sub_node()
                        .and_then(|s| s.iter().find(|n| n.is_type(MCAST_PARAMS)))
                    {
                        let _func_scope = scope.clone().unwrap_or_else(|| {
                            crate::semantic::basic::mc_ids::McIds::new(&name_node)
                                .map(|ids| ids.to_string())
                                .unwrap_or_default()
                        });
                        for (pname, pspan) in Self::extract_func_param_spans(&params_node) {
                            // Func params default to LabelDef (func body labels).
                            // Previously UnknownDef was used but the upgrade pass
                            // (upgrade_unknown_defs) could not resolve them because
                            // name_index only contains class-level names, not func params.
                            let (d, _) = crate::refdef::register::register_def(
                                &mut *sem,
                                &uri,
                                enclosing.as_deref().unwrap_or(""),
                                func_name.as_deref(),
                                &pname,
                                pspan.clone(),
                                SymbolKind::LabelDef,
                            );
                            symbol_lapper.insert(Interval {
                                start: pspan.start,
                                stop: pspan.end,
                                val: SymbolType::new(SymbolKind::LabelDef, u32::from(d)),
                            });
                        }
                    }
                }
            }
        }
        for node in all_nodes.iter().rev() {
            let ntype = node.get_type();
            if ntype == MCAST_FUNCTION {
                continue;
            }
            if ntype == MCAST_DEFINE {
                if let Some(name_node) = node.get_sub_node() {
                    let span = (
                        name_node.get_pos() as usize,
                        (name_node.get_pos() + name_node.get_len()) as usize,
                    );
                    let enclosing = find_container(span.0).unwrap_or_default();
                    let (d, _) = crate::refdef::register::register_def(
                        sem,
                        uri,
                        &enclosing,
                        None,
                        "",
                        span.0..span.1,
                        SymbolKind::DefineDef,
                    );
                    symbol_lapper.insert(Interval {
                        start: span.0,
                        stop: span.1,
                        val: SymbolType::new(SymbolKind::DefineDef, u32::from(d)),
                    });
                }
            } else if ntype == MCAST_ROLE {
                if let Some(name_node) = node.get_sub_node() {
                    let span = (
                        name_node.get_pos() as usize,
                        (name_node.get_pos() + name_node.get_len()) as usize,
                    );
                    let enclosing = find_container(span.0).unwrap_or_default();
                    let (d, _) = crate::refdef::register::register_def(
                        sem,
                        uri,
                        &enclosing,
                        None,
                        "",
                        span.0..span.1,
                        SymbolKind::RoleDef,
                    );
                    symbol_lapper.insert(Interval {
                        start: span.0,
                        stop: span.1,
                        val: SymbolType::new(SymbolKind::RoleDef, u32::from(d)),
                    });
                }
            } else if ntype == MCAST_OPD_FCALL {
                let sub = node.get_sub_node();
                let name_node = if let Some(s) = &sub {
                    match s.get_type() {
                        MCAST_INSTANCE => s.get_next(),
                        _ => Some(s.clone()),
                    }
                } else {
                    None
                };
                if let Some(nn) = name_node {
                    let ids_node = if nn.get_type() == MCAST_IDS {
                        nn.clone()
                    } else {
                        nn.get_sub_node().unwrap_or_else(|| nn.clone())
                    };
                    let span = (
                        ids_node.get_pos() as usize,
                        (ids_node.get_pos() + ids_node.get_len()) as usize,
                    );
                    let has_instance = sub
                        .as_ref()
                        .map(|s| s.get_type() == MCAST_INSTANCE)
                        .unwrap_or(false);
                    let func_name = crate::semantic::basic::mc_ids::McIds::new(&ids_node)
                        .map(|ids| ids.to_string());
                    if has_instance {
                        // ★ Fix: only create FuncRef if the function is found in
                        // name_to_declare_id. Don't fall back to add_declare_with_name
                        // (which produces a random ID that never matches RefDefMap,
                        // causing P6 self-locate with no navigation).
                        if let Some(resolved_id) = func_name.as_ref().and_then(|n| {
                            let filt_file_id =
                                crate::ast::ast_semantic::intern(&mut sem.file_table, uri.as_str());
                            let candidates: Vec<_> = sem
                                .local_table
                                .name_to_declare_id
                                .iter()
                                .filter(|((fid, _, _, name), _id)| {
                                    *fid == filt_file_id && name.as_str() == n.as_str()
                                })
                                .collect();
                            if candidates.is_empty() {
                                None
                            } else {
                                Some(candidates[0].1 .0)
                            }
                        }) {
                            // P1: local scope FuncRef
                            symbol_lapper.insert(Interval {
                                start: span.0,
                                stop: span.1,
                                val: SymbolType::new(SymbolKind::FuncRef, u32::from(resolved_id)),
                            });
                            sem.ref_entries.push((
                                SymbolKind::FuncRef,
                                u32::from(resolved_id),
                                span.0,
                                span.1,
                            ));
                        } else {
                            if let Some(class_name) = Self::extract_class_name(&sub) {
                                // P3-P5: CMIE member lookup via mcb_get_cmie
                                if let Some(method_name) =
                                    func_name.as_ref().map(|s| s.as_str().to_string())
                                {
                                    if let Some((def_uri, def_span, ref_kind)) =
                                        crate::db::cmie::cmie::resolve_cmie_member(
                                            &class_name,
                                            &method_name,
                                            uri,
                                        )
                                    {
                                        let enclosing = find_container(span.0).unwrap_or_default();
                                        let (decl_id, _loc) = crate::refdef::register::register_def(
                                            sem,
                                            &def_uri,
                                            &enclosing,
                                            None,
                                            &method_name,
                                            def_span,
                                            SymbolKind::FuncDef,
                                        );
                                        symbol_lapper.insert(Interval {
                                            start: span.0,
                                            stop: span.1,
                                            val: SymbolType::new(ref_kind, u32::from(decl_id)),
                                        });
                                        sem.ref_entries.push((
                                            ref_kind,
                                            u32::from(decl_id),
                                            span.0,
                                            span.1,
                                        ));
                                    } else {
                                        dlog_error(
                                            1501,
                                            node,
                                            &format!(
                                                "function '{}' not found in class '{}'",
                                                method_name, class_name
                                            ),
                                        );
                                    }
                                }
                            }
                        }
                    } else {
                        // ★ Fix: Do NOT create a duplicate ClassRef here.
                        // lapper_global_classes already creates the correct ClassRef
                        // entry (keyed by ReferenceId from gt.add_declare_class).
                        // Creating another ClassRef with decl_id from
                        // add_declare_with_name produces a different ID space that
                        // shadows the correct entry and causes RefDefMap MISS → P6
                        // self-locate (no navigation). See §8.2.
                    }
                    if let Some(enclosing) = find_container(span.0) {
                        let refs = crate::refdef::collect::collect_funccall_arg_refs(
                            node,
                            &sem.local_table,
                            &uri,
                            &enclosing,
                        );
                        for (span, did) in refs {
                            // ★ §4.3: Dispatch ref kind based on def type (not catch-all FuncParamRef)
                            let ref_kind =
                                crate::refdef::collect::resolve_arg_ref_kind(&sem.def_map, did);
                            symbol_lapper.insert(Interval {
                                start: span.start,
                                stop: span.end,
                                val: SymbolType::new(ref_kind, u32::from(did)),
                            });
                            sem.ref_entries
                                .push((ref_kind, u32::from(did), span.start, span.end));
                        }
                    }
                }
            }
        }
    }

    /// Extract the class name from an MCAST_INSTANCE inside an MCAST_OPD_FCALL.
    ///
    /// RES(10kΩ).Pullup(uC.7, uC.VDD)
    ///   sub = [INSTANCE, NAME("Pullup"), PARAMS]
    ///   → INSTANCE.get_sub_node() = inner FCall for RES(10kΩ)
    ///   → inner.children = [NAME("RES"), PARAMS("10kΩ")]
    ///   → returns Some("RES")
    fn extract_class_name(sub: &Option<AstNode>) -> Option<String> {
        let s = sub.as_ref()?;
        if s.get_type() != MCAST_INSTANCE {
            return None;
        }
        // MCAST_INSTANCE wraps the inner FCall (e.g. RES(100kΩ)).
        let inner_fcall = s.get_sub_node()?;
        // Walk children to find MCAST_NAME — the first child may be
        // MCAST_PARAMS_PRE (from `pre => Class(...)` patterns) rather
        // than MCAST_NAME.
        let name_node = if inner_fcall.get_type() == MCAST_NAME {
            inner_fcall
        } else {
            // Search children for MCAST_NAME in the linked list
            let first_child = inner_fcall.get_sub_node()?;
            if first_child.get_type() == MCAST_NAME {
                first_child
            } else {
                first_child.iter().find(|n| n.get_type() == MCAST_NAME)?
            }
        };
        let ids_node = name_node.get_sub_node()?;
        let ids = McIds::new(&ids_node)?;
        Some(ids.to_string())
    }

    /// Look up the human-readable message for a parser diagnostic code.
    fn dlog_parser_message(code: u32) -> &'static str {
        match code {
            // Errors (E1002–E1031)
            1002 => "Invalid top-level declaration",
            1003 => "Invalid clause in body",
            1004 => "Invalid pin declaration",
            1005 => "Pin ID must be a constant integer, not an expression",
            1006 => "Pin name must be a constant identifier, not an expression",
            1007 => "Net endpoint must be a port/label, not a literal",
            1008 => "Invalid net/connection expression",
            1009 => "Invalid if/else condition block",
            1010 => "Invalid role block",
            1011 => "Invalid function definition",
            1012 => "Invalid pins declaration",
            1013 => "Invalid import statement",
            1014 => "Invalid condition body",
            1015 => "Invalid instance declaration (:: syntax)",
            1016 => "Invalid body",
            1017 => "Invalid condition expression",
            1018 => "Invalid parameter declaration",
            1019 => "Invalid import path",
            1020 => "Invalid expression list",
            1021 => "Invalid operand list",
            1022 => "Invalid parameter list",
            1023 => "Invalid parameter declaration list",
            1024 => "Invalid attribute value list",
            1025 => "Invalid attribute line list",
            1026 => "Invalid pin name list",
            1027 => "Invalid instance list",
            1028 => "Invalid else-if chain",
            1029 => "Invalid identifier list",
            1030 => "Invalid path in import",
            1031 => "Invalid expression",
            // Warnings (W1101–W1106)
            1101 => "Single '|' as binary operator outside pin context",
            1102 => "'±' as binary operator outside tolerance context",
            1103 => "Transpose (') on a literal has no effect",
            1104 => "Caret (^) on a literal has no effect",
            1105 => "Empty body — no clauses defined",
            1106 => "Empty pins declaration",
            _ => "Syntax error",
        }
    }
}

fn attr_key_name(attr_node: &AstNode) -> Option<String> {
    let sub = attr_node.get_sub_node()?;
    let ids_node = sub.get_sub_node()?;
    crate::semantic::basic::mc_ids::McIds::new(&ids_node).map(|ids| ids.to_string())
}

fn extract_dot_pair(value_node: &AstNode) -> Option<(String, String, u32, u32, u32, u32)> {
    use crate::ast::c_macros::{MCAST_ID, MCAST_IDS, MCAST_OPD_DOT};
    let ids_node = if value_node.is_type(crate::ast::c_macros::MCAST_OPD) {
        value_node.get_sub_node()?
    } else if value_node.is_type(MCAST_IDS) {
        value_node.clone()
    } else {
        return None;
    };
    if !ids_node.is_type(MCAST_IDS) {
        return None;
    }
    let mut children = ids_node.get_sub_node()?.iter();
    let first_id = children.next()?;
    if !first_id.is_type(MCAST_ID) && first_id.get_type() != 7 {
        return None;
    }
    let dot_node = children.next()?;
    if !dot_node.is_type(MCAST_OPD_DOT) {
        return None;
    }
    let member_node = dot_node.get_sub_node()?;
    let (base_name, member_name) = {
        let base = crate::semantic::basic::mc_ids::McIds::new(&first_id).map(|i| i.to_string())?;
        let mem =
            crate::semantic::basic::mc_ids::McIds::new(&member_node).map(|i| i.to_string())?;
        (base, mem)
    };
    let base_start = first_id.get_pos();
    let base_end = base_start + base_name.len() as u32;
    let member_start = member_node.get_pos();
    let member_end = member_start + member_node.get_len();
    Some((
        base_name,
        member_name,
        base_start,
        base_end,
        member_start,
        member_end,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The C parser (`mcc_load_from_string` → `mcc_lex`/`mcc_parse`) keeps
    /// token/error state in process-global buffers, so it is not re-entrant.
    /// Tests that drive it must be serialized to avoid cross-test corruption.
    static PARSE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Regression: declareb instances inside net expressions must be registered
    /// as LSP declarations (`res[1:2]::RES(0Ω)` → res1/res2, `C4::CAP()` → C4).
    ///
    /// The twopin early-return in `McPhrase::new` (MCAST_DECLARE) bypasses
    /// `context.parse_declare()`, so instance names were never registered —
    /// only the class ref was. This test asserts the declaration is present in
    /// `name_to_declare_id` and that `lapper_module_ports` produced a LabelDef
    /// interval at the instance-name span (what mcext goto-def consumes).
    #[test]
    fn declareb_inline_inst_registers_lsp_declaration() {
        let _guard = PARSE_LOCK.lock().expect("test parse lock");
        crate::mcc_init_no_lib();
        crate::mcc_set_system_root(std::path::Path::new(""));
        crate::mcc_clear_workspace();

        let uri: crate::McURI = "/mcc/declareb-inst.mc".to_string();
        let source = r#"
module main
{
    UART0 - res[1:2]::RES(0Ω) - UART1
    MIC{P,N} -> [C4::CAP(),C5::CAP()] -> ADC{P,N}
}
"#;
        crate::mcc_load_from_string(&uri, source);

        let mcode = workspace::WORKSPACE.mcodes.get(&uri).expect("file loaded");
        let sem = mcode.symbols.lock().expect("symbols lock");
        let lt = &sem.local_table;

        // 1. Declarations present in name_to_declare_id.
        let file_id = crate::ast::ast_semantic::intern(&mut sem.file_table.clone(), uri.as_str());
        let scope_id = lt.scope_index.get("main").copied();
        let (cid, fnid) = scope_id
            .map(|(_, c, f)| (c, f))
            .unwrap_or((u32::MAX, u32::MAX));
        for name in ["res1", "res2", "C4", "C5"] {
            let key = (file_id, cid, fnid, name.to_string());
            assert!(
                lt.name_to_declare_id.contains_key(&key),
                "declareb instance '{name}' must have a declaration in name_to_declare_id"
            );
        }

        // 2. Lapper has a def interval covering each instance-name span.
        // res1/res2 share the `res[1:2]` span; C4/C5 have their own spans.
        let expected: Vec<(&str, std::ops::Range<usize>)> = vec![
            (
                "res1",
                source.find("res[1:2]").unwrap()..source.find("res[1:2]").unwrap() + 8,
            ),
            (
                "res2",
                source.find("res[1:2]").unwrap()..source.find("res[1:2]").unwrap() + 8,
            ),
            (
                "C4",
                source.find("C4::CAP").unwrap()..source.find("C4::CAP").unwrap() + 2,
            ),
            (
                "C5",
                source.find("C5::CAP").unwrap()..source.find("C5::CAP").unwrap() + 2,
            ),
        ];
        for (name, span) in expected {
            let found = sem.symbol_lapper.iter().any(|iv| {
                iv.val.kind == SymbolKind::LabelDef as u8
                    && iv.start == span.start
                    && iv.stop == span.end
            });
            assert!(
                found,
                "lapper must contain a LabelDef interval for declareb instance '{name}' at {span:?}"
            );
        }
    }

    /// Regression: member chains in net expressions (`uC.ADC{P,N}`, `uC.19`)
    /// must produce lapper REF intervals resolved to the class-definition pin.
    ///
    /// `try_record_chain_ref` stores these in `chain_ref_spans`, but the
    /// consumption loop in `lapper_module_ports` / `lapper_function_params`
    /// was dropped in the a520f3a merge — so goto-def on the member text
    /// found no interval. This test asserts each chain ref is present in the
    /// lapper and maps (via ref_def_map) to the pin in the component.
    #[test]
    fn module_member_chain_refs_resolve_in_lapper() {
        let _guard = PARSE_LOCK.lock().expect("test parse lock");
        crate::mcc_init_no_lib();
        crate::mcc_set_system_root(std::path::Path::new(""));
        crate::mcc_clear_workspace();

        let uri: crate::McURI = "/mcc/chainref.mc".to_string();
        let source = r#"
component MCU.X
{
    pins = [
        io [6,7] = UART0::UART.TTL(DCE)
        io [16,17] = ADC::ADC.DIFF(Receiver)
        io [18,19] = GPIO[0,1]
    ]
}

module main
{
    io UART0, MIC{P,N}
    out SPK
    MCU.X uC
    UART0 - uC.UART0
    MIC{P,N} -> uC.ADC{P,N}
    uC.19 -> SPK
}
"#;
        crate::mcc_load_from_string(&uri, source);

        let mcode = workspace::WORKSPACE.mcodes.get(&uri).expect("file loaded");
        let sem = mcode.symbols.lock().expect("symbols lock");

        // Expected byte spans (source is pure ASCII here).
        let adc_ref = source.find("uC.ADC{P,N}").unwrap()
            ..source.find("uC.ADC{P,N}").unwrap() + "uC.ADC{P,N}".len();
        let p19_ref = source.find("uC.19").unwrap()..source.find("uC.19").unwrap() + "uC.19".len();
        let uart0_ref =
            source.find("uC.UART0").unwrap()..source.find("uC.UART0").unwrap() + "uC.UART0".len();
        let adc_def = source.find("ADC").unwrap()..source.find("ADC").unwrap() + 3;
        let pin19_def = source.find("18,19").unwrap()..source.find("18,19").unwrap() + 5;
        let uart0_def = source.find("UART0").unwrap()..source.find("UART0").unwrap() + 5;

        // 1. Each member chain must have a REF interval at its text span.
        let mut ref_ids: Vec<(
            std::ops::Range<usize>,
            SymbolKind,
            u32,
            std::ops::Range<usize>,
        )> = Vec::new();
        for (ref_span, def_span) in [
            (adc_ref.clone(), adc_def.clone()),
            (p19_ref.clone(), pin19_def.clone()),
            (uart0_ref.clone(), uart0_def.clone()),
        ] {
            let mut found: Option<(SymbolKind, u32)> = None;
            for iv in sem.symbol_lapper.iter() {
                if iv.start == ref_span.start && iv.stop == ref_span.end {
                    let kind: SymbolKind = unsafe { std::mem::transmute(iv.val.kind) };
                    if kind.is_ref() {
                        found = Some((kind, iv.val.id));
                    }
                }
            }
            assert!(
                found.is_some(),
                "lapper must contain a REF interval for member chain at {ref_span:?}"
            );
            ref_ids.push((ref_span, found.unwrap().0, found.unwrap().1, def_span));
        }

        // 2. Each chain ref must map (via ref_def_map) to the pin def span.
        let rdm = sem.ref_def_map.as_ref().expect("ref_def_map is built");
        for (ref_span, kind, id, def_span) in &ref_ids {
            let entry = rdm
                .entries
                .get(&(*kind, *id))
                .expect("chain ref must have a ref_def_map entry");
            let loc = &entry.def_loc;
            assert!(
                loc.byte_start as usize == def_span.start && loc.byte_end as usize == def_span.end,
                "chain ref at {ref_span:?} (kind={kind:?} id={id}) must resolve to pin def {def_span:?}, got {}..{}",
                loc.byte_start,
                loc.byte_end
            );
        }
    }
}
