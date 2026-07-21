// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::ast::ast_node::McValueFFI;
use crate::ast::ast_semantic::{
    DeclareId, McSemSymbols, ReferenceId, Span, SymbolRangeLapper, SymbolType,
};
use crate::ast::ast_token::McSemTokens;
use crate::ast::error::message::MISSING_SUBNODE;
use crate::db::cmie::tables as workspace;
use crate::db::diagnostic::diagnostic::dlog_error;
use crate::db::infra::global;
use crate::db::infra::mc_use::McUse;
use crate::semantic::mc_enum::McEnumDef;
use crate::semantic::mc_ifs::McInterface;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};

/// Global deduplication flag: each parse cycle outputs AST visit only once
/// Reset at the mcc_load_project entry point (mcb_reset_ast_visit_flag)
pub static AST_VISIT_DONE: AtomicBool = AtomicBool::new(false);

/// Re-entrancy guard for parse_pass1_types: prevents mcb_get_cmie's
/// on-demand parsing from re-entering parse_pass1_types for a file
/// that is already being parsed higher up the call stack.
thread_local! {
    static PARSING_PASS1: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

pub fn mcb_reset_ast_visit_flag() {
    AST_VISIT_DONE.store(false, Ordering::SeqCst);
}
use crate::{ast::ast_node::AstNode, ast::c_macros::*, semantic::common::McCMIE};
use crate::{current_uri, McComponent, McIds, McModule, McSpaceName, McURI};
use core::panic;
use line_index::LineIndex;
use rust_lapper::Interval;
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct McCode {
    pub(crate) mcbase: bool,
    pub(crate) uri: McURI,
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
        crate::ast::ast_semantic::ReferenceId,
        McURI,
        std::ops::Range<usize>,
    )>,
}

////////////////////////////////
impl McCode {
    fn collect_direct_uses(&self, current_path: &Path) -> Vec<McUse> {
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

        Some(McCode {
            mcbase: base,
            uri: uri.clone(),
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
        Some(McCode {
            mcbase: false,
            uri: uri.clone(),
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
    pub fn parse_ast_from_string(&mut self, content: &str) {
        current_uri::set(&self.uri);
        crate::db::diagnostic::diagnostic::dlog_clear_file(&self.uri);

        // Clear C-level error tokens before parsing to prevent accumulation
        unsafe { crate::ast::c_bindings::mcc_clear_error_tokens() };

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
            if let Ok(mut t) = self.tokens.lock() {
                *t = McSemTokens::new();
            }
            if let Ok(mut s) = self.symbols.lock() {
                *s = McSemSymbols::new();
            }

            crate::ast::c_bindings::mc_sem_token_free();
            crate::ast::c_bindings::mcc_lex(fcontent_ptr);

            let ast = AstNode::new(crate::ast::c_bindings::mcc_parse());
            if ast.is_null() {
                tracing::warn!(target: "mcc::code", uri = %self.uri, "AST parse returned null");
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

            libc::free(fcontent_ptr as *mut libc::c_void);

            match self.tokens.lock() {
                Ok(mut t) => {
                    *t = McSemTokens::new();
                    t.parse(crate::ast::c_bindings::mcc_get_sem_tokens());
                    // Extract inline comments consumed by ELC prefix/suffix
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
                // Reuse existing spacenames and uselist
                self.spacenames.clone_from(&existing.spacenames);
                self.uselist.clone_from(&existing.uselist);
                // Mark pass1 as complete (dependency file has been processed)
                self.pass1_complete = true;
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
            let cmie_list = mcfile.parse_cmie_names();

            // (2.5) ★ Step 3: ensure CMIE definitions are registered to the global table
            // If spacenames and uselist already exist, reuse them directly
            if has_existing {
                // Reuse existing spacenames (clone only when actually needed)
                if let Some(existing) = workspace::WORKSPACE.mcodes.get(&canonical_use_uri) {
                    for (key, value) in existing.spacenames.iter() {
                        if !self.spacenames.contains_key(key) {
                            self.spacenames.insert(key.clone(), value.clone());
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
                for (key, value) in &mcfile.spacenames {
                    if !self.spacenames.contains_key(key) {
                        self.spacenames.insert(key.clone(), value.clone());
                    }
                }
                // Mark the current file's pass1 as complete (dependent components are already registered in parse_pass1_types above)
                self.pass1_complete = true;
            }

            match mcuse.impt_ids {
                None => {
                    for cmie in cmie_list {
                        self.spacenames
                            .insert(cmie.clone(), McSpaceName::new(&cmie, mcuse.uri.clone()));
                    }
                }
                Some(classes) => {
                    for class in classes {
                        if cmie_list.contains(&class) {
                            self.spacenames
                                .insert(class.clone(), McSpaceName::new(&class, mcuse.uri.clone()));
                        } else {
                            tracing::warn!(
                                target: "mcc::code",
                                definition = %class,
                                uri = %mcuse.uri,
                                "use'd definition does not exist in target file"
                            );
                        }
                    }
                }
            }

            for (key, value) in &mcfile.spacenames {
                if !self.spacenames.contains_key(key) {
                    self.spacenames.insert(key.clone(), value.clone());
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

    /// List of class names defined in this file
    pub fn parse_cmie_names(&mut self) -> Vec<McIds> {
        let mut cmies: Vec<McIds> = Vec::<McIds>::new();
        for node in self.ast.iter() {
            if node.is_type(MCAST_INTERFACE)
                || node.is_type(MCAST_COMPONENT)
                || node.is_type(MCAST_MODULE)
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
            } //TODO enum
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
                                            dlog_error(1004, &node, "Duplicate module");
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
                                            dlog_error(1501, &node, "Duplicate interface");
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
                                    let class_name_str = enum_def.name.to_string();
                                    let class_span =
                                        enum_def.span[0] as usize..enum_def.span[1] as usize;
                                    let value_spans: Vec<(usize, usize)> = enum_def
                                        .values
                                        .iter()
                                        .map(|v| (v.span[0] as usize, v.span[1] as usize))
                                        .collect();
                                    if let Some(class_id) = self.add_enum_class(
                                        &self_uri,
                                        &class_name_str,
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
                                                dlog_error(1504, &node, "Duplicate enum");
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
        // Re-entrancy guard: if this file is already being parsed up the call
        // stack (triggered from mcb_get_cmie's on-demand parsing), skip.
        let already_parsing = PARSING_PASS1.with(|s| !s.borrow_mut().insert(self.uri.clone()));
        if already_parsing {
            return;
        }
        for node in self.ast.iter() {
            match node.get_type() {
                MCAST_INTERFACE => {
                    if let Some(ifs) = McInterface::new(&node, &self.uri) {
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
                    }
                }
                MCAST_COMPONENT => {
                    if let Some(comp) = McComponent::new(&node, &self.uri) {
                        // ★ First clone the needed data (name + uri) for global_table,
                        // then move comp into the Arc table
                        let comp_name_str = comp.name.to_string();
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
                            &comp_name_str,
                            comp_span,
                            crate::ContainerKind::Component,
                        );
                    }
                }
                MCAST_ENUM => {
                    if let Some(enum_def) = McEnumDef::new(&node, &self.uri) {
                        // ★ LSP: register class + values in global table before the move.
                        let self_uri = self.uri.clone();
                        let class_name_str = enum_def.name.to_string();
                        let class_span = enum_def.span[0] as usize..enum_def.span[1] as usize;
                        let value_spans: Vec<(usize, usize)> = enum_def
                            .values
                            .iter()
                            .map(|v| (v.span[0] as usize, v.span[1] as usize))
                            .collect();
                        if let Some(class_id) =
                            self.add_enum_class(&self_uri, &class_name_str, class_span.clone())
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

        // Mark Pass1 parse as complete
        self.pass1_complete = true;

        self.parse_pass1_modules();
        PARSING_PASS1.with(|s| s.borrow_mut().remove(&self.uri));
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
        // Re-entrancy guard: same as parse_pass1_types — mcb_get_cmie's
        // on-demand parsing can trigger parse_pass1_modules for a file
        // that is already being parsed higher up the call stack.
        let already_parsing = PARSING_PASS1.with(|s| !s.borrow_mut().insert(self.uri.clone()));
        if already_parsing {
            return;
        }
        if self.modules_parsed && !self.use_table_dirty {
            PARSING_PASS1.with(|s| s.borrow_mut().remove(&self.uri));
            return;
        }
        // ★ §7.6: Use table dirty — only rebuild RefDefMap/name_index,
        // no need to re-parse modules.
        if self.modules_parsed && self.use_table_dirty {
            self.create_lapper(); // includes inline Layer 2 + consolidate (Layer 1 + name_index)
            self.use_table_dirty = false;
            PARSING_PASS1.with(|s| s.borrow_mut().remove(&self.uri));
            return;
        }
        self.modules_parsed = true;

        for (_i, node) in self.ast.iter().enumerate() {
            let node_type = node.get_type();
            if node_type == MCAST_MODULE {
                if let Some(module) = McModule::new(&node, &self.uri) {
                    let module_name = module.name.clone();
                    let key = McSpaceName {
                        ident: module_name.clone(),
                        uri: self.uri.clone(),
                    };
                    // Replace any previously registered shallow copy with fully-parsed module
                    workspace::WORKSPACE.modules.insert(key, Arc::new(module));
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
        // Keep URI in PARSING_PASS1 so mcb_parse_all_modules' second pass
        // skips rebuild (which would clear name_to_declare_id and create
        // new DeclareIds, breaking FuncRef→FuncDef matching).

        // ★ §7.6: Mark dependent files dirty — their Use table P4 entries
        // may need refreshing because this file's CMIE defs changed.
        if let Some(deps) = workspace::WORKSPACE.reverse_deps.get(&self.uri) {
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
        // parse_pass1_modules is already called at the end of parse_pass1_types
    }

    // ========================================================================
    // Phase 3: Pre-parse function bodies
    // ========================================================================

    /// Pre-parse function bodies for all functions in the module.
    pub fn add_global_class(
        &mut self,
        uri: &McURI,
        class_name: &String,
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
            table.insert(
                (uri.to_string(), kind, class_name.clone()),
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
        class_name: &str,
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
    pub fn add_declare_inst(&self, span: Span) -> Option<DeclareId> {
        self.add_declare_inst_with_name(span, None)
    }

    pub fn add_declare_inst_with_name(
        &self,
        span: Span,
        name: Option<String>,
    ) -> Option<DeclareId> {
        match self.symbols.lock() {
            Ok(mut symbols) => {
                // Register to local table
                let local_id = symbols.local_table.add_declare_with_name(
                    &self.uri,
                    span.clone(),
                    name.clone(),
                    None,
                );
                // ★ Also register to global table for cross-file lookup
                if let Some(ref n) = name {
                    if let Ok(mut gtable) = symbols.global_table.lock() {
                        let global_id = gtable.add_global_inst(&self.uri, n, span.clone());
                        tracing::debug!(target: "mcc::lsp", "Registered inst {} at {:?} -> global_id={:?}", n, span, global_id);
                    }
                }
                Some(local_id)
            }
            Err(e) => {
                tracing::error!(target: "mcc::code", error = %e, "symbols mutex poisoned (add_declare_inst)");
                None
            }
        }
    }

    pub fn get_declare_id_by_name(&self, name: &str) -> Option<DeclareId> {
        match self.symbols.lock() {
            Ok(symbols) => {
                // First check global table (cross-file)
                if let Ok(gtable) = symbols.global_table.lock() {
                    if let Some(id) = gtable.get_global_inst(&self.uri, name) {
                        return Some(id);
                    }
                }
                // Then check local table — unscoped lookup within this file
                symbols
                    .local_table
                    .name_to_declare_id
                    .get(&(self.uri.clone(), String::new(), name.to_string()))
                    .copied()
            }
            Err(e) => {
                tracing::error!(target: "mcc::code", error = %e, "symbols mutex poisoned (get_declare_id_by_name)");
                None
            }
        }
    }

    pub fn add_inst_reference(&self, span: Span, declr_id: DeclareId) {
        if let Ok(mut symbols) = self.symbols.lock() {
            symbols.local_table.add_inst(span, declr_id);
        }
    }

    /// Look up a DeclareId in the workspace global instance table.
    fn lookup_global_inst(
        uri: &str,
        name: &str,
        scope: Option<&str>,
    ) -> Option<crate::ast::ast_semantic::DeclareId> {
        crate::db::cmie::tables::WORKSPACE
            .lsp
            .inst_table
            .lock()
            .ok()
            .and_then(|t| t.get(uri, scope, name))
    }

    /// Public wrapper for RPC handlers.
    pub fn scope_path_from_scope_str_public(uri: &McURI, scope: &str) -> crate::ScopePath {
        Self::scope_path_from_scope_str(uri, scope)
    }

    /// Build a ScopePath from a scope string and file URI.
    /// "US513" → module,  "US513.i2c" → func-in-module,  "" → file-level.
    fn scope_path_from_scope_str(uri: &McURI, scope: &str) -> crate::ScopePath {
        if scope.is_empty() {
            crate::ScopePath::file_level(uri)
        } else if let Some(dot_pos) = scope.rfind('.') {
            let container = &scope[..dot_pos];
            let func = &scope[dot_pos + 1..];
            crate::ScopePath::func_in_module(uri, container, func)
        } else {
            crate::ScopePath::module(uri, scope)
        }
    }

    /// Priority-based declare lookup.
    ///
    /// Search order (from innermost to outermost):
    ///   1. Exact scope match:  name_to_declare_id with ref's scope_path
    ///   2. Same container:     name_to_declare_id with container scope
    ///   3. File-level:         local_inst_map (DeclareInstances in this file)
    ///   4. Global inst table:  scoped then unscoped
    /// Resolve a name to its DeclareId using the visibility scope model
    /// from the design doc (§1.3).
    ///
    /// ## Lookup priority (higher shadows lower):
    ///   P1: current func scope — func params, func body labels
    ///   P2: current container  — module/component/interface/enum internal defs
    ///
    /// Internal defs (ports, instances, labels, funcs) are container-scoped
    /// and do NOT leak to file-level or cross-file visibility (§3.2.2).
    /// There is intentionally NO P3/P4/P5 fallback — those levels are for
    /// CMIE class names (component/module/interface/enum/define) resolved
    /// via `mcb_get_cmie`, not for port/instance refs.
    fn lookup_declare_id(
        local: &crate::ast::ast_semantic::LocalSymbolTable,
        _uri: &str,
        name: &str,
        scope_path: &crate::ScopePath,
    ) -> Option<crate::ast::ast_semantic::DeclareId> {
        let ref_scope = scope_path.scope_key();
        let file_uri = &scope_path.uri;

        // P1: exact scope match — scope identified by (file_uri, scope_string)
        //   func body ref → scope = "US513.i2c"
        //   module body ref → scope = "US513"
        let exact_key = (file_uri.clone(), ref_scope.clone(), name.to_string());
        if let Some(id) = local.name_to_declare_id.get(&exact_key).copied() {
            return Some(id);
        }

        // P2: container-level match — when inside a func, fall back to
        //   the parent container (module/component) scope
        if scope_path.func.is_some() {
            let container_key = (
                file_uri.clone(),
                scope_path.container.name.clone(),
                name.to_string(),
            );
            if let Some(id) = local.name_to_declare_id.get(&container_key).copied() {
                return Some(id);
            }
        }

        None
    }

    /// Recursively scan funcall argument nodes for port refs (SQUARE_VEC members,
    /// IDs, etc.) and return (span, DeclareId) pairs for InstanceRef lapper entries.
    fn collect_funccall_arg_refs(
        arg_node: &AstNode,
        local_table: &crate::ast::ast_semantic::LocalSymbolTable,
        file_uri: &McURI,
        enclosing: &str,
    ) -> Vec<(std::ops::Range<usize>, DeclareId)> {
        use crate::ast::c_macros::{
            MCAST_ID, MCAST_IDA, MCAST_IDS, MCAST_OPD_SQUARE_VEC, MCAST_SQUARE_VEC,
        };
        let mut result = Vec::new();
        let ntype = arg_node.get_type();
        match ntype {
            MCAST_SQUARE_VEC | MCAST_OPD_SQUARE_VEC => {
                // Iterate members: [VDD_3V3, GND] → VDD_3V3, GND
                let mut cur = arg_node.get_sub_node();
                while let Some(member) = cur {
                    let ids_node = member.get_sub_node().unwrap_or_else(|| member.clone());
                    if let Some(ids) = crate::semantic::basic::mc_ids::McIds::new(&ids_node) {
                        let name = ids.to_string();
                        let span = (ids_node.get_pos() as usize)
                            ..((ids_node.get_pos() + ids_node.get_len()) as usize);
                        let sp = Self::scope_path_from_scope_str(file_uri, enclosing);
                        let decl_id = Self::lookup_declare_id(local_table, "", &name, &sp);
                        tracing::info!(target: "mcc::lsp",
                            "FCALL_ARG_REF: member='{name}' span=[{},{}] enclosing='{enclosing}' decl_id={}",
                            span.start, span.end,
                            decl_id.map(|d| u32::from(d) as i64).unwrap_or(-1)
                        );
                        if let Some(did) = decl_id {
                            result.push((span, did));
                        }
                    }
                    cur = member.get_next();
                }
            }
            MCAST_ID | MCAST_IDA | MCAST_IDS => {
                if let Some(ids) = crate::semantic::basic::mc_ids::McIds::new(arg_node) {
                    let name = ids.to_string();
                    let span = (arg_node.get_pos() as usize)
                        ..((arg_node.get_pos() + arg_node.get_len()) as usize);
                    let sp = Self::scope_path_from_scope_str(file_uri, enclosing);
                    let decl_id = Self::lookup_declare_id(local_table, "", &name, &sp);
                    if let Some(did) = decl_id {
                        result.push((span, did));
                    }
                }
            }
            _ => {
                // Recurse into children
                if let Some(sub) = arg_node.get_sub_node() {
                    let mut cur = Some(sub);
                    while let Some(child) = cur {
                        let mut child_refs = Self::collect_funccall_arg_refs(
                            &child,
                            local_table,
                            file_uri,
                            enclosing,
                        );
                        result.append(&mut child_refs);
                        cur = child.get_next();
                    }
                }
            }
        }
        result
    }

    /// Build RefDefMap Layer 2 inline from the freshly-built lapper.
    /// Matches InstanceRef/LabelRef/FunctionRef/etc. to their defs via shared DeclareId.
    /// Called at end of create_lapper() — no separate lapper re-scan.
    fn fill_refdef_layer2(
        map: &mut crate::ast::ast_semantic::RefDefMap,
        lapper: &SymbolRangeLapper,
        scope_map: &std::collections::HashMap<(usize, usize), String>,
        file_uri: &McURI,
    ) {
        use crate::ast::ast_semantic::{RefDefEntry, SymbolKind};

        // Build def_map: decl_id → (start, stop, kind) from lapper defs
        let mut def_map: std::collections::HashMap<u32, (usize, usize, SymbolKind)> =
            std::collections::HashMap::new();
        for iv in lapper.iter() {
            let (kind, id) = match &iv.val {
                SymbolType::PortDefinition(d) => (SymbolKind::PortDef, u32::from(*d)),
                SymbolType::DeclareInstance(d) => (SymbolKind::InstDef, u32::from(*d)),
                SymbolType::LabelDefinition(d) => (SymbolKind::LabelDef, u32::from(*d)),
                SymbolType::FunctionDefinition(d) => (SymbolKind::FuncDef, u32::from(*d)),
                SymbolType::PinNameDefinition(d) => (SymbolKind::PinNameDef, u32::from(*d)),
                SymbolType::PinIdDefinition(d) => (SymbolKind::PinIdDef, u32::from(*d)),
                SymbolType::PinIfaceDefinition(d) => (SymbolKind::PinIfaceDef, u32::from(*d)),
                SymbolType::EnumValueDefinition(d) => (SymbolKind::EnumValDef, u32::from(*d)),
                SymbolType::EnumDefinition(d) => (SymbolKind::EnumDef, u32::from(*d)),
                SymbolType::ClassDefinition(d) => (SymbolKind::ClassDef, u32::from(*d)),
                SymbolType::DefineDefinition(d) => (SymbolKind::DefineDef, u32::from(*d)),
                SymbolType::RoleDefinition(d) => (SymbolKind::RoleDef, u32::from(*d)),
                SymbolType::ParamDefinition(d) => (SymbolKind::ParamDef, u32::from(*d)),
                SymbolType::AttrDefinition(d) => (SymbolKind::AttrDef, u32::from(*d)),
                _ => continue,
            };
            def_map.entry(id).or_insert((iv.start, iv.stop, kind));
        }

        // For each ref, match to def via shared DeclareId
        for iv in lapper.iter() {
            let (ref_kind, decl_id) = match &iv.val {
                SymbolType::InstanceRef(d) => (SymbolKind::InstRef, u32::from(*d)),
                SymbolType::PortRef(d) => (SymbolKind::PortRef, u32::from(*d)),
                SymbolType::LabelRef(d) => (SymbolKind::LabelRef, u32::from(*d)),
                SymbolType::FunctionRef(d) => (SymbolKind::FuncRef, u32::from(*d)),
                SymbolType::PinNameRef(d) => (SymbolKind::PinNameRef, u32::from(*d)),
                SymbolType::PinIdRef(d) => (SymbolKind::PinIdRef, u32::from(*d)),
                SymbolType::PinIfaceRef(d) => (SymbolKind::PinIfaceRef, u32::from(*d)),
                SymbolType::EnumValueRef(d) => (SymbolKind::EnumValRef, u32::from(*d)),
                SymbolType::EnumRef(d) => (SymbolKind::EnumRef, u32::from(*d)),
                SymbolType::ClassRef(d) => (SymbolKind::ClassRef, u32::from(*d)),
                _ => continue,
            };
            if map.index.contains_key(&(ref_kind, decl_id)) {
                continue;
            }
            // Verify def_kind matches ref_kind (e.g. FuncRef→FuncDef, InstRef→InstDef)
            let expected_def = match ref_kind {
                SymbolKind::InstRef => Some(SymbolKind::InstDef),
                SymbolKind::FuncRef => Some(SymbolKind::FuncDef),
                SymbolKind::PortRef => Some(SymbolKind::PortDef),
                SymbolKind::LabelRef => Some(SymbolKind::LabelDef),
                SymbolKind::PinNameRef => Some(SymbolKind::PinNameDef),
                SymbolKind::PinIdRef => Some(SymbolKind::PinIdDef),
                SymbolKind::PinIfaceRef => Some(SymbolKind::PinIfaceDef),
                SymbolKind::EnumValRef => Some(SymbolKind::EnumValDef),
                SymbolKind::EnumRef => Some(SymbolKind::EnumDef),
                SymbolKind::ClassRef => Some(SymbolKind::ClassDef),
                _ => None,
            };
            if let Some(&(def_start, def_stop, def_kind)) = def_map.get(&decl_id) {
                if let Some(expected) = expected_def {
                    if def_kind != expected {
                        continue;
                    }
                }
                if def_start == iv.start && def_stop == iv.stop {
                    continue;
                }
                let fid = map.intern_file(file_uri);
                let scope = scope_map
                    .get(&(iv.start, iv.stop))
                    .cloned()
                    .unwrap_or_default();
                let cid = map.intern_container(&scope);
                map.insert(
                    ref_kind,
                    decl_id,
                    RefDefEntry {
                        ref_kind: SymbolKind::ClassDef,
                        ref_id: 0,
                        file_id: fid,
                        def_span_start: def_start as u32,
                        def_span_end: def_stop as u32,
                        def_kind,
                        container_id: cid,
                        cmie_kind: crate::ast::ast_semantic::CmieKind::UNKNOWN,
                    },
                );
            }
        }
    }

    /// Build RefDefMap from semantic tables.
    /// Runs after parse_pass1_modules() registers all symbols, before create_lapper().
    fn consolidate_ref_def_map(&mut self) {
        use crate::ast::ast_semantic::{RefDefEntry, RefDefMap, SymbolKind};

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
            let uri = &self.uri;

            // ── Build reverse DeclareId → scope map for container_id population ──
            let decl_id_to_scope: std::collections::HashMap<u32, String> = lt
                .name_to_declare_id
                .iter()
                .map(|((_u, scope, _n), &did)| (u32::from(did), scope.clone()))
                .collect();

            // ── Layer 1: ID chain ──

            // 1a. class_ref (ReferenceId) → class_def
            for (ref_id, class_id) in &gt.declare_id_to_class_id {
                if let Some((def_uri, span)) = gt.class_id_to_span.get(class_id) {
                    let fid = map.intern_file(def_uri);
                    let cid = map.intern_container("");
                    map.insert(
                        SymbolKind::ClassRef,
                        u32::from(*ref_id),
                        RefDefEntry {
                            ref_kind: SymbolKind::ClassDef,
                            ref_id: 0,
                            file_id: fid,
                            def_span_start: span.start as u32,
                            def_span_end: span.end as u32,
                            def_kind: SymbolKind::ClassDef,
                            container_id: cid,
                            cmie_kind: crate::ast::ast_semantic::CmieKind::UNKNOWN,
                        },
                    );
                }
            }

            // 1b. class_ref (DeclareId / ClassRef variant) → class_def
            for (class_id, (def_uri, span)) in &gt.class_id_to_span {
                let fid = map.intern_file(def_uri);
                let cid = map.intern_container("");
                map.insert(
                    SymbolKind::ClassRef,
                    u32::from(*class_id),
                    RefDefEntry {
                        ref_kind: SymbolKind::ClassDef,
                        ref_id: 0,
                        file_id: fid,
                        def_span_start: span.start as u32,
                        def_span_end: span.end as u32,
                        def_kind: SymbolKind::ClassDef,
                        container_id: cid,
                        cmie_kind: crate::ast::ast_semantic::CmieKind::UNKNOWN,
                    },
                );
            }

            // 1c. cross-file class ref targets (cached from create_lapper, §8.2)
            for (ref_id, def_uri, span) in &self.cross_file_targets {
                let fid = map.intern_file(def_uri);
                let cid = map.intern_container("");
                map.insert(
                    SymbolKind::ClassRef,
                    u32::from(*ref_id),
                    RefDefEntry {
                        ref_kind: SymbolKind::ClassDef,
                        ref_id: 0,
                        file_id: fid,
                        def_span_start: span.start as u32,
                        def_span_end: span.end as u32,
                        def_kind: SymbolKind::ClassDef,
                        container_id: cid,
                        cmie_kind: crate::ast::ast_semantic::CmieKind::UNKNOWN,
                    },
                );
            }

            // 1d. instance_ref → def (via inst_id_to_declare_inst)
            for (inst_id, decl_id) in &lt.inst_id_to_declare_inst {
                if let Some(span) = lt.declare_inst_to_span.get(decl_id) {
                    let fid = map.intern_file(uri);
                    let scope = decl_id_to_scope
                        .get(&u32::from(*decl_id))
                        .map(|s| s.as_str())
                        .unwrap_or("");
                    let cid = map.intern_container(scope);
                    map.insert(
                        SymbolKind::InstRef,
                        u32::from(*inst_id),
                        RefDefEntry {
                            ref_kind: SymbolKind::ClassDef,
                            ref_id: 0,
                            file_id: fid,
                            def_span_start: span.start as u32,
                            def_span_end: span.end as u32,
                            def_kind: SymbolKind::InstDef,
                            container_id: cid,
                            cmie_kind: crate::ast::ast_semantic::CmieKind::UNKNOWN,
                        },
                    );
                }
            }

            // 1e. enum_value_ref → def
            for (value_id, (def_uri, span)) in &gt.enum_value_id_to_span {
                let fid = map.intern_file(def_uri);
                let cid = map.intern_container("");
                map.insert(
                    SymbolKind::EnumValRef,
                    u32::from(*value_id),
                    RefDefEntry {
                        ref_kind: SymbolKind::ClassDef,
                        ref_id: 0,
                        file_id: fid,
                        def_span_start: span.start as u32,
                        def_span_end: span.end as u32,
                        def_kind: SymbolKind::EnumValDef,
                        container_id: cid,
                        cmie_kind: crate::ast::ast_semantic::CmieKind::UNKNOWN,
                    },
                );
            }

            // 1f. enum class ref → enum class def
            // Enum classes use a separate span storage (enum_class_id_to_span),
            // so DeclareClass refs to enums need this extra lookup.
            for (ref_id, class_id) in &gt.declare_id_to_class_id {
                // Skip if already covered by 1a (class_id_to_span)
                if gt.class_id_to_span.contains_key(class_id) {
                    continue;
                }
                if let Some((def_uri, span)) = gt.enum_class_id_to_span.get(class_id) {
                    let fid = map.intern_file(def_uri);
                    let cid = map.intern_container("");
                    map.insert(
                        SymbolKind::EnumRef,
                        u32::from(*ref_id),
                        RefDefEntry {
                            ref_kind: SymbolKind::ClassDef,
                            ref_id: 0,
                            file_id: fid,
                            def_span_start: span.start as u32,
                            def_span_end: span.end as u32,
                            def_kind: SymbolKind::EnumDef,
                            container_id: cid,
                            cmie_kind: crate::ast::ast_semantic::CmieKind::UNKNOWN,
                        },
                    );
                }
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
                let idx = map.entries.len();
                map.entries.push(RefDefEntry {
                    ref_kind: SymbolKind::ClassDef,
                    ref_id: 0,
                    file_id: fid,
                    def_span_start: span_start as u32,
                    def_span_end: span_end as u32,
                    def_kind,
                    container_id: cid,
                    cmie_kind,
                });
                map.name_index
                    .insert((self.uri.to_string(), name.to_string()), idx);
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
                            for ((_target_uri, name), &entry_idx) in &target_map.name_index {
                                // Copy the entry from target_map, re-interning
                                // file_id/container_id into self's map.
                                if let Some(src_entry) = target_map.entries.get(entry_idx) {
                                    let src_file_uri =
                                        target_map.files.get(src_entry.file_id as usize);
                                    let src_container = if src_entry.container_id != u32::MAX {
                                        target_map
                                            .containers
                                            .get(src_entry.container_id as usize)
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
                                    let new_idx = map.entries.len();
                                    map.entries.push(RefDefEntry {
                                        ref_kind: src_entry.ref_kind,
                                        ref_id: src_entry.ref_id,
                                        file_id: new_fid,
                                        def_span_start: src_entry.def_span_start,
                                        def_span_end: src_entry.def_span_end,
                                        def_kind: src_entry.def_kind,
                                        container_id: new_cid,
                                        cmie_kind: src_entry.cmie_kind,
                                    });
                                    // Register original name (P4)
                                    map.name_index
                                        .insert((self.uri.to_string(), name.to_string()), new_idx);
                                    // ★ §5.1 use as alias: e.g. `use ./helper as h`
                                    if let Some(ref alias) = mc_use.as_id {
                                        let aliased = format!("{alias}.{name}");
                                        map.name_index
                                            .insert((self.uri.to_string(), aliased), new_idx);
                                    }
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
                            let idx = map.entries.len();
                            map.entries.push(RefDefEntry {
                                ref_kind: SymbolKind::ClassDef,
                                ref_id: 0,
                                file_id: fid,
                                def_span_start: span.start as u32,
                                def_span_end: span.end as u32,
                                def_kind: SymbolKind::ClassDef,
                                container_id: cid,
                                cmie_kind: crate::ast::ast_semantic::CmieKind::UNKNOWN,
                            });
                            map.add_name_alias(&self.uri, class_name, idx);
                        }
                    }
                    for ((def_uri, class_name), class_id) in &gt.enum_class_name_to_id {
                        if let Some((_u, span)) = gt.enum_class_id_to_span.get(class_id) {
                            let fid = map.intern_file(def_uri);
                            let cid = map.intern_container("");
                            let idx = map.entries.len();
                            map.entries.push(RefDefEntry {
                                ref_kind: SymbolKind::ClassDef,
                                ref_id: 0,
                                file_id: fid,
                                def_span_start: span.start as u32,
                                def_span_end: span.end as u32,
                                def_kind: SymbolKind::EnumDef,
                                container_id: cid,
                                cmie_kind: CmieKind::Enum as u8,
                            });
                            map.add_name_alias(&self.uri, class_name, idx);
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
            let before = sem.local_table.name_to_declare_id.len();
            sem.local_table
                .name_to_declare_id
                .retain(|(uri, _, _), _| uri != &self.uri);
            // Cleanup complete — stale entries removed
        }
        match self.symbols.lock() {
            Ok(mut sem) => {
                let mut symbol_lapper = SymbolRangeLapper::new(vec![]);
                let uri_str = self.uri.as_str();

                match sem.global_table.lock() {
                    Ok(mut gt) => {
                        let clsids: Vec<_> = gt
                            .class_name_to_id
                            .iter()
                            .filter(|((uri, _clsname), _clsid)| uri == &self.uri)
                            .map(|(_key, clsid)| *clsid)
                            .collect();

                        for clsid in &clsids {
                            if let Some((_uri, span)) = gt.class_id_to_span.get(clsid) {
                                symbol_lapper.insert(Interval {
                                    start: span.start,
                                    stop: span.end,
                                    val: SymbolType::ClassDefinition(*clsid),
                                });
                            }
                        }

                        // ★ LSP: Read declare class refs from workspace table FIRST,
                        // then populate span_to_declare_class_id via add_declare_class(),
                        // THEN iterate span_to_declare_class_id to insert into symbol_lapper.
                        {
                            let mut decl_refs = crate::db::cmie::tables::WORKSPACE
                                .lsp
                                .declare_class_refs
                                .lock()
                                .unwrap();
                            tracing::info!(target: "mcc::lsp", "  create_lapper: lsp.declare_class_refs for '{}' = {} entries", self.uri, decl_refs.get(&self.uri).map(|v| v.len()).unwrap_or(0));
                            if let Some(refs) = decl_refs.remove(&self.uri) {
                                for (decl_span, _class_id, target_uri, target_span) in refs {
                                    let refid = gt.add_declare_class(
                                        &self.uri,
                                        decl_span.clone(),
                                        _class_id,
                                    );
                                    // ★ Cache cross-file targets locally for consolidate_ref_def_map,
                                    // bypassing GlobalSymbolTable.declare_id_to_target_span (§8.2).
                                    self.cross_file_targets
                                        .push((refid, target_uri, target_span));
                                }
                            }
                        }

                        // Now iterate span_to_declare_class_id (which now includes entries
                        // from lsp.declare_class_refs above) and insert into symbol_lapper
                        for ((uri, span), refid) in gt.span_to_declare_class_id.iter() {
                            if uri == &self.uri {
                                symbol_lapper.insert(Interval {
                                    start: span.start,
                                    stop: span.end,
                                    val: SymbolType::DeclareClass(*refid),
                                });
                            }
                        }

                        // ★ LSP: enum class + value defs — emit to lapper so cursor on the
                        //   `enum PKG {` line or the `SOP8,` row self-locates (matches the
                        //   existing `class_definition` / `port_definition` self-resolution
                        //   pattern in gotodef).
                        for ((uri, _name), class_id) in gt.enum_class_name_to_id.iter() {
                            if uri != &self.uri {
                                continue;
                            }
                            if let Some((_u, span)) = gt.enum_class_id_to_span.get(class_id) {
                                symbol_lapper.insert(Interval {
                                    start: span.start,
                                    stop: span.end,
                                    val: SymbolType::ClassDefinition(*class_id),
                                });
                            }
                        }
                        for (value_id, (uri, span)) in gt.enum_value_id_to_span.iter() {
                            if uri == &self.uri {
                                symbol_lapper.insert(Interval {
                                    start: span.start,
                                    stop: span.end,
                                    val: SymbolType::EnumValueDefinition(*value_id),
                                });
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!(target: "mcc::code", error = %e, "global_table mutex poisoned (create_lapper)")
                    }
                }

                // ★ LSP: Get instance declarations from workspace global table (cross-file)
                // and also add to local_table so LSP handler can find them
                {
                    let inst_table = crate::db::cmie::tables::WORKSPACE
                        .lsp
                        .inst_table
                        .lock()
                        .unwrap();

                    tracing::info!(target: "mcc::lsp", "  create_lapper: inst_table.len = {}", inst_table.len());
                    for (decl_id, scope, span) in inst_table.get_decls_for_uri(uri_str) {
                        // Add to symbol_lapper for LSP lookup
                        symbol_lapper.insert(Interval {
                            start: span.start,
                            stop: span.end,
                            val: SymbolType::DeclareInstance(decl_id),
                        });
                        // ★ Store scope for LSP goto-def
                        if !scope.is_empty() {
                            sem.symbol_scope.insert((span.start, span.end), scope);
                        }
                        // ★ Also add to local_table so LSP handler can find the span
                        sem.local_table
                            .declare_inst_to_span
                            .insert(decl_id, span.clone());
                    }
                }

                //local (keep for backward compatibility)
                let decl_count = sem.local_table.declare_inst_to_span.len();
                for (dcl_id, span) in sem.local_table.declare_inst_to_span.iter() {
                    symbol_lapper.insert(Interval {
                        start: span.start,
                        stop: span.end,
                        val: SymbolType::DeclareInstance(*dcl_id),
                    });
                }
                // First pass: register known refs as InstanceRef(DeclareId)
                // instead of InstanceReference(ReferenceId), using the
                // declare_inst_to_inst_ids reverse mapping.
                let mut inst_to_decl: std::collections::HashMap<ReferenceId, DeclareId> =
                    std::collections::HashMap::new();
                for (decl_id, inst_ids) in sem.local_table.declare_inst_to_inst_ids.iter() {
                    for iid in inst_ids {
                        inst_to_decl.insert(*iid, *decl_id);
                    }
                }
                for (inst_id, span) in sem.local_table.inst_id_to_span.iter() {
                    let decl_id = inst_to_decl
                        .get(inst_id)
                        .copied()
                        .unwrap_or(DeclareId::default());
                    symbol_lapper.insert(Interval {
                        start: span.start,
                        stop: span.end,
                        val: SymbolType::InstanceRef(decl_id),
                    });
                }

                // ★ LSP: Also add global instance references
                let global_ref_count = {
                    let inst_table = crate::db::cmie::tables::WORKSPACE
                        .lsp
                        .inst_table
                        .lock()
                        .unwrap();
                    let refs = inst_table.get_all_refs_for_uri(uri_str);
                    let count = refs.len();
                    for (decl_id, scope, span) in refs {
                        symbol_lapper.insert(Interval {
                            start: span.start,
                            stop: span.end,
                            val: SymbolType::InstanceRef(decl_id),
                        });
                        // ★ Store scope for LSP goto-def
                        if !scope.is_empty() {
                            sem.symbol_scope.insert((span.start, span.end), scope);
                        }
                    }
                    count
                };

                // ★ LSP: Add interface definitions + param port_definitions
                {
                    let interfaces = &crate::db::cmie::tables::WORKSPACE.interfaces;
                    for entry in interfaces.iter() {
                        let iface = entry.value();
                        if iface.uri.as_str() == uri_str {
                            symbol_lapper.insert(Interval {
                                start: iface.span.start,
                                stop: iface.span.end,
                                val: SymbolType::ClassDefinition(DeclareId::new(0)),
                            });
                            // Collect param decl_ids for attr reference linking
                            let mut param_decl_ids: std::collections::HashMap<String, DeclareId> =
                                std::collections::HashMap::new();
                            let iface_ident = iface.name.to_string();
                            for (name, span) in iface.params.iter_defs_with_span() {
                                let decl_id = sem.local_table.add_declare_with_name(
                                    &self.uri,
                                    span.clone(),
                                    Some(name.to_string()),
                                    Some(&iface_ident),
                                );
                                param_decl_ids.insert(name.to_string(), decl_id);
                                symbol_lapper.insert(Interval {
                                    start: span.start,
                                    stop: span.end,
                                    val: SymbolType::PortDefinition(decl_id),
                                });
                                sem.symbol_scope
                                    .insert((span.start, span.end), iface_ident.clone());
                            }
                            // Register attribute variable references linked to param decls
                            for attr in iface.attrs.iter() {
                                for val in &attr.values {
                                    if let crate::semantic::component::mc_attr::McAttrVal::AttrVariable(opd, Some(span)) = val {
                                        let var_name = opd.to_string();
                                        let decl_id = param_decl_ids.get(&var_name).copied().unwrap_or(DeclareId::new(0));
                                        sem.local_table.add_inst(span.clone(), decl_id);
                                    }
                                }
                            }
                            // ★ Register interface param refs from body expressions
                            // (e.g. `spec.voltage = volt` where volt is a param)
                            for (span, port_name, scope) in iface.params.iter_port_refs() {
                                let sp = Self::scope_path_from_scope_str(&self.uri, scope);
                                let decl_id = Self::lookup_declare_id(
                                    &sem.local_table,
                                    self.uri.as_str(),
                                    port_name,
                                    &sp,
                                );
                                if let Some(decl_id) = decl_id {
                                    symbol_lapper.insert(Interval {
                                        start: span.start,
                                        stop: span.end,
                                        val: SymbolType::InstanceRef(decl_id),
                                    });
                                    sem.symbol_scope
                                        .insert((span.start, span.end), scope.clone());
                                }
                            }
                        }
                    }
                    let global_interfaces = &crate::db::infra::global::mcc_interfaces;
                    for entry in global_interfaces.iter() {
                        let iface = entry.value();
                        if iface.uri.as_str() == uri_str {
                            symbol_lapper.insert(Interval {
                                start: iface.span.start,
                                stop: iface.span.end,
                                val: SymbolType::ClassDefinition(DeclareId::new(0)),
                            });
                            let iface_name_g = iface.name.to_string();
                            let mut param_decl_ids: std::collections::HashMap<String, DeclareId> =
                                std::collections::HashMap::new();
                            for (name, span) in iface.params.iter_defs_with_span() {
                                let decl_id = sem.local_table.add_declare_with_name(
                                    &self.uri,
                                    span.clone(),
                                    Some(name.to_string()),
                                    Some(&iface_name_g),
                                );
                                param_decl_ids.insert(name.to_string(), decl_id);
                                symbol_lapper.insert(Interval {
                                    start: span.start,
                                    stop: span.end,
                                    val: SymbolType::PortDefinition(decl_id),
                                });
                                sem.symbol_scope
                                    .insert((span.start, span.end), iface_name_g.clone());
                            }
                            for attr in iface.attrs.iter() {
                                for val in &attr.values {
                                    if let crate::semantic::component::mc_attr::McAttrVal::AttrVariable(opd, Some(span)) = val {
                                        let var_name = opd.to_string();
                                        let decl_id = param_decl_ids.get(&var_name).copied().unwrap_or(DeclareId::new(0));
                                        sem.local_table.add_inst(span.clone(), decl_id);
                                    }
                                }
                            }
                            // ★ Register interface param refs from body expressions
                            for (span, port_name, scope) in iface.params.iter_port_refs() {
                                let sp = Self::scope_path_from_scope_str(&self.uri, scope);
                                let decl_id = Self::lookup_declare_id(
                                    &sem.local_table,
                                    self.uri.as_str(),
                                    port_name,
                                    &sp,
                                );
                                if let Some(decl_id) = decl_id {
                                    symbol_lapper.insert(Interval {
                                        start: span.start,
                                        stop: span.end,
                                        val: SymbolType::InstanceRef(decl_id),
                                    });
                                    sem.symbol_scope
                                        .insert((span.start, span.end), scope.clone());
                                }
                            }
                        }
                    }
                }

                // ★ LSP: Add module port definitions to symbol_lapper and local_table
                {
                    let modules = &crate::db::cmie::tables::WORKSPACE.modules;
                    for entry in modules.iter() {
                        let m = entry.value();
                        // Only process modules that belong to the current file
                        if entry.key().uri.as_str() != self.uri.as_str() {
                            continue;
                        }

                        // Ports from params (e.g. `module m(dc24v, GPIO[1:2])`)
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
                            let span_clone = span.clone();
                            let decl_id = sem.local_table.add_declare_with_name(
                                &self.uri,
                                span_clone,
                                Some(name.to_string()),
                                Some(&mod_ident),
                            );
                            tracing::debug!(
                                target: "mcc::lsp",
                                "[LAPPER_DEBUG]   param port: name={}, span=[{},{}], decl_id={:?}",
                                name, span.start, span.end, decl_id
                            );
                            symbol_lapper.insert(Interval {
                                start: span.start,
                                stop: span.end,
                                val: SymbolType::PortDefinition(decl_id),
                            });
                            sem.symbol_scope
                                .insert((span.start, span.end), mod_ident.clone());
                        }

                        // Ports from body declarations (e.g. `ps dc24v`, `io GPIO[1:2]`)
                        let mod_ident2 = entry.key().ident.to_string();
                        for (name, _iotype, span) in m.insts.iter_ports_with_span() {
                            let span_clone = span.clone();
                            let decl_id = sem.local_table.add_declare_with_name(
                                &self.uri,
                                span_clone,
                                Some(name.to_string()),
                                Some(&mod_ident2),
                            );
                            tracing::debug!(
                                target: "mcc::lsp",
                                "[LAPPER_DEBUG]   inst port: name={}, span=[{},{}], decl_id={:?}",
                                name, span.start, span.end, decl_id
                            );
                            symbol_lapper.insert(Interval {
                                start: span.start,
                                stop: span.end,
                                val: SymbolType::PortDefinition(decl_id),
                            });
                            sem.symbol_scope
                                .insert((span.start, span.end), mod_ident2.clone());
                        }
                        // Register port references from net lines (e.g. GPIO1 - A references port GPIO1)
                        for (span, port_name, scope) in m.insts.iter_port_refs() {
                            let sp = Self::scope_path_from_scope_str(&self.uri, scope);
                            let decl_id = Self::lookup_declare_id(
                                &sem.local_table,
                                self.uri.as_str(),
                                port_name,
                                &sp,
                            );
                            if let Some(decl_id) = decl_id {
                                symbol_lapper.insert(Interval {
                                    start: span.start,
                                    stop: span.end,
                                    val: SymbolType::InstanceRef(decl_id),
                                });
                                sem.symbol_scope
                                    .insert((span.start, span.end), scope.clone());
                            }
                        }
                        // Register param port references from net lines
                        tracing::info!(
                            "LAPPER_PARAM_REFS: module={} param_port_refs_count={}",
                            entry.key().ident,
                            m.params.iter_port_refs().count()
                        );
                        for (span, port_name, scope) in m.params.iter_port_refs() {
                            let sp = Self::scope_path_from_scope_str(&self.uri, scope);
                            let decl_id = Self::lookup_declare_id(
                                &sem.local_table,
                                self.uri.as_str(),
                                port_name,
                                &sp,
                            );
                            tracing::info!(
                                "LAPPER_PARAM_REF: port_name='{port_name}' span=[{},{}] scope='{scope}' decl_id={}",
                                span.start, span.end,
                                decl_id.map(|d| u32::from(d) as i64).unwrap_or(-1)
                            );
                            if let Some(decl_id) = decl_id {
                                symbol_lapper.insert(Interval {
                                    start: span.start,
                                    stop: span.end,
                                    val: SymbolType::InstanceRef(decl_id),
                                });
                                sem.symbol_scope
                                    .insert((span.start, span.end), scope.clone());
                            }
                        }
                        // ★ Label definitions (explicit + inline) for LSP goto-def
                        let mod_ident_label = entry.key().ident.to_string();
                        for (name, _label_kind, span) in m.insts.iter_labels_with_span() {
                            let decl_id = sem.local_table.add_declare_with_name(
                                &self.uri,
                                span.clone(),
                                Some(name.to_string()),
                                Some(&mod_ident_label),
                            );
                            symbol_lapper.insert(Interval {
                                start: span.start,
                                stop: span.end,
                                val: SymbolType::LabelDefinition(decl_id),
                            });
                            sem.symbol_scope
                                .insert((span.start, span.end), mod_ident_label.clone());
                            // ★ Register in global instance table for cross-file lookup
                            if let Ok(mut ginst) =
                                crate::db::cmie::tables::WORKSPACE.lsp.inst_table.lock()
                            {
                                ginst.add(
                                    self.uri.as_str(),
                                    Some(&mod_ident_label),
                                    name,
                                    span.clone(),
                                );
                            }
                        }
                    }
                }

                let local_ref_count = sem.local_table.inst_id_to_span.len();
                tracing::info!(target: "mcc::lsp", "create_lapper: {} decls, {} local_refs, {} global_refs, lapper len={}", decl_count, local_ref_count, global_ref_count, symbol_lapper.len());

                // ★ G2: Register function parameter refs from module functions
                {
                    let modules = &crate::db::cmie::tables::WORKSPACE.modules;
                    for entry in modules.iter() {
                        let m = entry.value();
                        if entry.key().uri.as_str() != self.uri.as_str() {
                            continue;
                        }
                        for func in m.funcs.iter() {
                            let fscope = func.name.to_string();
                            for (span, port_name, scope) in func.params.iter_port_refs() {
                                let sp = Self::scope_path_from_scope_str(&self.uri, scope);
                                let decl_id = Self::lookup_declare_id(
                                    &sem.local_table,
                                    self.uri.as_str(),
                                    port_name,
                                    &sp,
                                );
                                if let Some(decl_id) = decl_id {
                                    symbol_lapper.insert(Interval {
                                        start: span.start,
                                        stop: span.end,
                                        val: SymbolType::InstanceRef(decl_id),
                                    });
                                    sem.symbol_scope
                                        .insert((span.start, span.end), scope.clone());
                                }
                            }
                            // ★ Label definitions within function body
                            let func_scope =
                                func.insts.scope.clone().unwrap_or_else(|| fscope.clone());
                            for (name, _label_kind, span) in func.insts.iter_labels_with_span() {
                                let decl_id = sem.local_table.add_declare_with_name(
                                    &self.uri,
                                    span.clone(),
                                    Some(name.to_string()),
                                    Some(&func_scope),
                                );
                                symbol_lapper.insert(Interval {
                                    start: span.start,
                                    stop: span.end,
                                    val: SymbolType::LabelDefinition(decl_id),
                                });
                                sem.symbol_scope
                                    .insert((span.start, span.end), func_scope.clone());
                                // ★ Register in global instance table for cross-file lookup
                                if let Ok(mut ginst) =
                                    crate::db::cmie::tables::WORKSPACE.lsp.inst_table.lock()
                                {
                                    ginst.add(
                                        self.uri.as_str(),
                                        Some(&func_scope),
                                        name,
                                        span.clone(),
                                    );
                                }
                            }
                        }
                    }
                }

                // ★ LSP: Add component parameter definitions to symbol_lapper
                //   (e.g. `component RESA(rs, volt)` -> rs, volt are PortDefinition)
                {
                    let components = &crate::db::cmie::tables::WORKSPACE.components;
                    for entry in components.iter() {
                        let comp = entry.value();
                        if entry.key().uri.as_str() != self.uri.as_str() {
                            continue;
                        }
                        // Component params (e.g. `component RESA(rs, volt)`)
                        let comp_ident = entry.key().ident.to_string();
                        for (name, span) in comp.params.iter_defs_with_span() {
                            let span_clone = span.clone();
                            let decl_id = sem.local_table.add_declare_with_name(
                                &self.uri,
                                span_clone,
                                Some(name.to_string()),
                                Some(&comp_ident),
                            );
                            symbol_lapper.insert(Interval {
                                start: span.start,
                                stop: span.end,
                                val: SymbolType::PortDefinition(decl_id),
                            });
                            sem.symbol_scope
                                .insert((span.start, span.end), comp_ident.clone());
                        }
                        // ★ G5: Pin name definitions from component pins
                        for (pin_name, pin_span) in Self::extract_pin_name_spans(comp) {
                            let pdecl_id = sem.local_table.add_declare_with_name(
                                &self.uri,
                                pin_span.clone(),
                                Some(pin_name.clone()),
                                Some(&comp_ident),
                            );
                            symbol_lapper.insert(Interval {
                                start: pin_span.start,
                                stop: pin_span.end,
                                val: SymbolType::PinNameDefinition(pdecl_id),
                            });
                            sem.symbol_scope
                                .insert((pin_span.start, pin_span.end), comp_ident.clone());
                        }
                        // §3.2.2: Pin ID definitions (e.g. "1", "2" in "1 = VDD")
                        for (pin_id, id_span) in Self::extract_pin_id_spans(comp) {
                            let decl_id = sem.local_table.add_declare_with_name(
                                &self.uri,
                                id_span.clone(),
                                Some(pin_id.clone()),
                                Some(&comp_ident),
                            );
                            symbol_lapper.insert(Interval {
                                start: id_span.start,
                                stop: id_span.end,
                                val: SymbolType::PinNameDefinition(decl_id),
                            });
                            sem.symbol_scope
                                .insert((id_span.start, id_span.end), comp_ident.clone());
                        }
                        // §3.2.2: Pin interface definitions (e.g. "UART.TTL" in "1::UART.TTL")
                        for (iface, if_span) in Self::extract_pin_iface_spans(comp) {
                            let decl_id = sem.local_table.add_declare_with_name(
                                &self.uri,
                                if_span.clone(),
                                Some(iface.clone()),
                                Some(&comp_ident),
                            );
                            symbol_lapper.insert(Interval {
                                start: if_span.start,
                                stop: if_span.end,
                                val: SymbolType::PinNameDefinition(decl_id),
                            });
                            sem.symbol_scope
                                .insert((if_span.start, if_span.end), comp_ident.clone());
                        }
                        // ★ G8: Spec key definitions from component attrs
                        for (key_name, key_span) in Self::extract_spec_key_spans(comp) {
                            let sdecl_id = sem.local_table.add_declare_with_name(
                                &self.uri,
                                key_span.clone(),
                                Some(key_name.clone()),
                                Some(&comp_ident),
                            );
                            symbol_lapper.insert(Interval {
                                start: key_span.start,
                                stop: key_span.end,
                                val: SymbolType::PinNameDefinition(sdecl_id),
                            });
                            sem.symbol_scope
                                .insert((key_span.start, key_span.end), comp_ident.clone());
                        }
                        // Component param references from body expressions
                        // (e.g. `spec.value = rs` where rs is a param)
                        for (span, port_name, scope) in comp.params.iter_port_refs() {
                            let sp = Self::scope_path_from_scope_str(&self.uri, scope);
                            let decl_id = Self::lookup_declare_id(
                                &sem.local_table,
                                self.uri.as_str(),
                                port_name,
                                &sp,
                            );
                            if let Some(decl_id) = decl_id {
                                symbol_lapper.insert(Interval {
                                    start: span.start,
                                    stop: span.end,
                                    val: SymbolType::InstanceRef(decl_id),
                                });
                                sem.symbol_scope
                                    .insert((span.start, span.end), scope.clone());
                            }
                        }
                        // ★ Label definitions for components
                        let comp_ident_label = entry.key().ident.to_string();
                        for (name, _label_kind, span) in comp.insts.iter_labels_with_span() {
                            let decl_id = sem.local_table.add_declare_with_name(
                                &self.uri,
                                span.clone(),
                                Some(name.to_string()),
                                Some(&comp_ident_label),
                            );
                            symbol_lapper.insert(Interval {
                                start: span.start,
                                stop: span.end,
                                val: SymbolType::LabelDefinition(decl_id),
                            });
                            sem.symbol_scope
                                .insert((span.start, span.end), comp_ident_label.clone());
                            // ★ Register in global instance table for cross-file lookup
                            if let Ok(mut ginst) =
                                crate::db::cmie::tables::WORKSPACE.lsp.inst_table.lock()
                            {
                                ginst.add(
                                    self.uri.as_str(),
                                    Some(&comp_ident_label),
                                    name,
                                    span.clone(),
                                );
                            }
                        }
                    }
                }

                // ★ LSP: emit enum reference entries for qualified refs like
                //   `package = PKG.SOP8`. We walk the AST for `MCAST_ATTRIBUTE`
                //   whose `key` is `package` (or `pkg`), and inspect each value
                //   OPD. When a value has the form `A.B` and `A` matches a known
                //   enum class while `B` matches one of its values, we push:
                //     - `enum_class_ref` at A's span, target = class_id
                //     - `enum_value_ref` at B's span, target = packed_value_id
                {
                    use crate::ast::ast_semantic::GlobalSymbolTable;
                    use rust_lapper::Interval;

                    fn attr_key_name(attr_node: &AstNode) -> Option<String> {
                        let sub = attr_node.get_sub_node()?;
                        let ids_node = sub.get_sub_node()?;
                        crate::semantic::basic::mc_ids::McIds::new(&ids_node)
                            .map(|ids| ids.to_string())
                    }

                    fn extract_dot_pair(
                        value_node: &AstNode,
                    ) -> Option<(
                        String,
                        String,
                        u32, /*base_start*/
                        u32, /*base_end*/
                        u32, /*member_start*/
                        u32, /*member_end*/
                    )> {
                        use crate::ast::c_macros::{MCAST_ID, MCAST_IDS, MCAST_OPD_DOT};
                        // Get the inner MCAST_IDS, unwrapping MCAST_OPD if present.
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
                        // MCAST_IDS children: MCAST_ID("PKG"), MCAST_OPD_DOT->MCAST_ID("QFN20")
                        let mut children = ids_node.get_sub_node()?.iter();
                        let first_id = children.next()?;
                        if !first_id.is_type(MCAST_ID) && first_id.get_type() != 7 {
                            // 7 = MCAST_IDA
                            return None;
                        }
                        let dot_node = children.next()?;
                        if !dot_node.is_type(MCAST_OPD_DOT) {
                            return None;
                        }
                        let member_node = dot_node.get_sub_node()?;
                        let (base_name, member_name) = {
                            let base = crate::semantic::basic::mc_ids::McIds::new(&first_id)
                                .map(|i| i.to_string())?;
                            let mem = crate::semantic::basic::mc_ids::McIds::new(&member_node)
                                .map(|i| i.to_string())?;
                            (base, mem)
                        };
                        let base_start = first_id.get_pos();
                        // The parser's first_id.get_len() sometimes spans the whole
                        // dotted path (`PKG.QFN20`), which would make the class-ref
                        // interval overlap the value-ref interval. Clamp it to just
                        // the base identifier so `PKG` and `QFN20` map to disjoint
                        // lapper ranges.
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

                    fn single_id_text(node: &AstNode) -> Option<String> {
                        crate::semantic::basic::mc_ids::McIds::new(node).map(|ids| ids.to_string())
                    }

                    // `AstNode::iter()` only walks sibling nodes (via `.next`), it does
                    // NOT descend into children (`.sub`). Attributes like
                    // `package = PKG.QFN20` are nested inside `component { ... }` /
                    // `class { ... }` bodies, so a flat top-level scan misses them.
                    // Collect every reachable node via an explicit DFS first.
                    let all_ast_nodes: Vec<AstNode> = {
                        let mut acc: Vec<AstNode> = Vec::new();
                        let mut stack: Vec<AstNode> = self.ast.iter().collect();
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
                        let key_name = match attr_key_name(&attr_node) {
                            Some(k) => k,
                            None => continue,
                        };
                        if key_name != "package" && key_name != "pkg" {
                            continue;
                        }
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
                            let (
                                base_name,
                                member_name,
                                base_start,
                                base_end,
                                member_start,
                                member_end,
                            ) = parsed;

                            // Resolve the enum class + value index. The enum may be:
                            //   (a) defined in a project/workspace file — registered in
                            //       this file's semantic `global_table`, or
                            //   (b) a system-library enum (e.g. `enum PKG` in mcode) —
                            //       only present in `global::mcc_enums`; the per-file
                            //       semantic table does NOT know about it.
                            // We therefore look up the class id best-effort and search
                            // both the workspace and the system-library enum stores for
                            // the value. mcext resolves `enum_value_ref` by (class,value)
                            // name via the project index, so the packed id is only used
                            // for display, not navigation — a system enum still gets a
                            // usable lapper entry even without a per-file class id.
                            let (class_id, value_idx) = {
                                // Best-effort class id from the per-file semantic table.
                                let class_id = match sem.global_table.lock() {
                                    Ok(gt) => gt
                                        .lookup_enum_class(&self.uri, &base_name)
                                        .or_else(|| {
                                            gt.enum_class_name_to_id.iter().find_map(
                                                |((_uri, name), cid)| {
                                                    (name == &base_name).then_some(*cid)
                                                },
                                            )
                                        })
                                        .unwrap_or_default(),
                                    Err(_) => continue 'outer,
                                };

                                // Search enum values in workspace enums first, then the
                                // system-library enum store.
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

                                // Not a known enum value (base_name isn't an enum class,
                                // or member isn't one of its values) — skip silently.
                                match idx {
                                    Some(i) => (class_id, i),
                                    None => continue,
                                }
                            };
                            let value_id =
                                GlobalSymbolTable::pack_enum_value_id(class_id, value_idx);

                            symbol_lapper.insert(Interval {
                                start: base_start as usize,
                                stop: base_end as usize,
                                val: SymbolType::ClassRef(class_id),
                            });
                            symbol_lapper.insert(Interval {
                                start: member_start as usize,
                                stop: member_end as usize,
                                val: SymbolType::EnumValueRef(value_id),
                            });
                            tracing::debug!(target: "mcc::enum_ref",
                                "pushed enum_class_ref+enum_value_ref for {base_name}.{member_name} (class_id={class_id:?}, value_id={value_id:?})");
                        }
                    }
                }

                // ── M6: func / define / role definitions ──
                {
                    let all_nodes: Vec<AstNode> = {
                        let mut acc = Vec::new();
                        let mut stack: Vec<AstNode> = self.ast.iter().collect();
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
                    // Collect enclosing module/component names for the current file.
                    // Module spans cover only the name, not the full body, so we
                    // just collect all containers in this file.
                    let _uri_str = self.uri.as_str();
                    // Collect enclosing module/component names for the current file.
                    let mut container_names: Vec<String> = Vec::new();
                    {
                        let uri_str = self.uri.as_str();
                        let modules = &workspace::WORKSPACE.modules;
                        for entry in modules.iter() {
                            let key_uri = entry.key().uri.as_str();
                            if key_uri == uri_str
                                || key_uri.ends_with(uri_str)
                                || uri_str.ends_with(key_uri)
                            {
                                container_names.push(entry.key().ident.to_string());
                            }
                        }
                        let comps = &workspace::WORKSPACE.components;
                        for entry in comps.iter() {
                            let key_uri = entry.key().uri.as_str();
                            if key_uri == uri_str
                                || key_uri.ends_with(uri_str)
                                || uri_str.ends_with(key_uri)
                            {
                                container_names.push(entry.key().ident.to_string());
                            }
                        }
                        // Also check global tables
                        for entry in global::mcc_modules.iter() {
                            let key_uri = entry.key().uri.as_str();
                            if key_uri == uri_str
                                || key_uri.ends_with(uri_str)
                                || uri_str.ends_with(key_uri)
                            {
                                container_names.push(entry.key().ident.to_string());
                            }
                        }
                        for entry in global::mcc_components.iter() {
                            let key_uri = entry.key().uri.as_str();
                            if key_uri == uri_str
                                || key_uri.ends_with(uri_str)
                                || uri_str.ends_with(key_uri)
                            {
                                container_names.push(entry.key().ident.to_string());
                            }
                        }
                        tracing::info!(target: "mcc::lsp",
                            "create_lapper scope: uri={uri_str}, found {} containers: {:?}",
                            container_names.len(), container_names);
                    }
                    // Build enclosing container lookup from AST node positions.
                    // Stored spans (McModule::span / McComponent::span) may be
                    // truncated (get_pos() returns 0 for MCAST_COMPONENT), so we
                    // use the AST nodes directly from all_nodes to determine
                    // which container a position falls within.
                    // Use AST node extents to determine enclosing container.
                    // Nodes are in DFS order; child nodes appear after parent.
                    // Track a stack of (name, end_pos) — when we pass a node's
                    // end position, pop it from the stack.
                    let mut container_stack: Vec<(String, usize)> = Vec::new();
                    let mut pos_to_container: Vec<(usize, String)> = Vec::new();
                    for node in &all_nodes {
                        let ntype = node.get_type();
                        let node_start = node.get_pos() as usize;
                        let node_end = node_start + node.get_len() as usize;
                        // Pop containers we've moved past
                        while let Some((_, end)) = container_stack.last() {
                            if node_start >= *end {
                                container_stack.pop();
                            } else {
                                break;
                            }
                        }
                        if ntype == MCAST_MODULE || ntype == MCAST_COMPONENT {
                            if let Some(sub) = node.get_sub_node() {
                                if let Some(name_node) = sub.iter().find(|x| x.is_type(MCAST_NAME))
                                {
                                    if let Some(ids_node) = name_node.get_sub_node() {
                                        if let Some(ids) = McIds::new(&ids_node) {
                                            container_stack.push((ids.to_string(), node_end));
                                        }
                                    }
                                }
                            }
                        }
                        // Record current container for this position
                        if let Some((name, _)) = container_stack.last() {
                            pos_to_container.push((node_start, name.clone()));
                        }
                    }
                    // Build a sorted lookup by position
                    pos_to_container.sort_by_key(|(pos, _)| *pos);
                    let find_container = move |pos: usize| -> Option<String> {
                        pos_to_container
                            .iter()
                            .take_while(|(p, _)| *p <= pos)
                            .last()
                            .map(|(_, name)| name.clone())
                    };

                    // Process in forward file order: FuncDefs before FuncRefs
                    for node in all_nodes.iter().rev() {
                        let ntype = node.get_type();
                        if ntype == MCAST_FUNCTION {
                            // ★ Fix: use MCAST_IDS (the actual name) for span, not
                            // MCAST_NAME which may cover the entire func body and
                            // shadow instance_ref entries inside func bodies.
                            let ids_node = node.get_sub_node().and_then(|n| n.get_sub_node()); // MCAST_NAME -> MCAST_IDS
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
                                // FuncDef scope: {enclosing}.{func_name}
                                let decl_id = sem.local_table.add_declare_with_name(
                                    &self.uri,
                                    span.0..span.1,
                                    func_name.clone(),
                                    scope.as_deref(),
                                );
                                // FuncDef registered in name_to_declare_id
                                symbol_lapper.insert(Interval {
                                    start: span.0,
                                    stop: span.1,
                                    val: SymbolType::FunctionDefinition(decl_id),
                                });
                                // ★ G2: function parameter definitions
                                if let Some(params_node) = node
                                    .get_sub_node()
                                    .and_then(|s| s.iter().find(|n| n.is_type(MCAST_PARAMS)))
                                {
                                    // Use the same scope as FunctionDefinition (enclosing.funcName)
                                    let func_scope = scope.clone().unwrap_or_else(|| {
                                        crate::semantic::basic::mc_ids::McIds::new(&name_node)
                                            .map(|ids| ids.to_string())
                                            .unwrap_or_default()
                                    });
                                    for (pname, pspan) in
                                        Self::extract_func_param_spans(&params_node)
                                    {
                                        let pdecl_id = sem.local_table.add_declare_with_name(
                                            &self.uri,
                                            pspan.clone(),
                                            Some(pname.clone()),
                                            Some(&func_scope),
                                        );
                                        symbol_lapper.insert(Interval {
                                            start: pspan.start,
                                            stop: pspan.end,
                                            val: SymbolType::PortDefinition(pdecl_id),
                                        });
                                        sem.symbol_scope
                                            .insert((pspan.start, pspan.end), func_scope.clone());
                                        // ★ Fix: also register in GlobalInstTable so
                                        // McPhrase::new can find func params and register
                                        // instance_ref entries. Use parent scope (module name)
                                        // so that LSP refs can be resolved by Level 3 name match.
                                        let module_scope = enclosing.clone();
                                        crate::query::refs::mcb_register_instance_decl(
                                            &self.uri,
                                            pspan.clone(),
                                            Some(pname.clone()),
                                            module_scope.as_deref(),
                                        );
                                    }
                                }
                            }
                        } else if ntype == MCAST_DEFINE {
                            if let Some(name_node) = node.get_sub_node() {
                                let span = (
                                    name_node.get_pos() as usize,
                                    (name_node.get_pos() + name_node.get_len()) as usize,
                                );
                                let enclosing = find_container(span.0);
                                let decl_id = sem.local_table.add_declare_with_name(
                                    &self.uri,
                                    span.0..span.1,
                                    None,
                                    enclosing.as_deref(),
                                );
                                symbol_lapper.insert(Interval {
                                    start: span.0,
                                    stop: span.1,
                                    val: SymbolType::DefineDefinition(decl_id),
                                });
                            }
                        } else if ntype == MCAST_ROLE {
                            if let Some(name_node) = node.get_sub_node() {
                                let span = (
                                    name_node.get_pos() as usize,
                                    (name_node.get_pos() + name_node.get_len()) as usize,
                                );
                                let enclosing = find_container(span.0);
                                let decl_id = sem.local_table.add_declare_with_name(
                                    &self.uri,
                                    span.0..span.1,
                                    None,
                                    enclosing.as_deref(),
                                );
                                symbol_lapper.insert(Interval {
                                    start: span.0,
                                    stop: span.1,
                                    val: SymbolType::RoleDefinition(decl_id),
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
                                // Use the tightest IDS sub-node for the span,
                                // not the parent which may cover constructor args.
                                // Extract the innermost MCAST_IDS for accurate span and name.
                                // nn may be MCAST_NAME (wrapper) → need child MCAST_IDS.
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
                                // Use ids_node (MCAST_IDS) for name parsing
                                let func_name =
                                    crate::semantic::basic::mc_ids::McIds::new(&ids_node)
                                        .map(|ids| ids.to_string());
                                if has_instance {
                                    // For method calls, reuse the FunctionDefinition's DeclareId.
                                    // FuncDef is registered with scope "{enclosing}.{func_name}".
                                    // Strategy (§4.2): search same-file for matching function_def.
                                    let resolved_id = func_name
                                        .as_ref()
                                        .and_then(|n| {
                                            // Search all scopes in the same file for this func name.
                                            // FuncDef may be registered under e.g. "US513_20_F.power".
                                            let candidates: Vec<_> = sem
                                                .local_table
                                                .name_to_declare_id
                                                .iter()
                                                .filter(|((u, _s, name), _id)| {
                                                    u == &self.uri && name.as_str() == n.as_str()
                                                })
                                                .collect();
                                            if candidates.is_empty() {
                                                None
                                            } else {
                                                // Use first match (same file, same name)
                                                Some(*candidates[0].1)
                                            }
                                        })
                                        .unwrap_or_else(|| {
                                            let fname = func_name.as_ref().map(|s| s.as_str()).unwrap_or("?");
                                            dlog_error(
                                                1501,
                                                &node,
                                                &format!(
                                                    "function '{}' not found in file '{}'",
                                                    fname,
                                                    self.uri
                                                ),
                                            );
                                            sem.local_table.add_declare_with_name(
                                                &self.uri,
                                                span.0..span.1,
                                                func_name.clone(),
                                                None,
                                            )
                                        });
                                    // ★ Register in inst_id_to_declare_inst for RefDefMap Layer 1d
                                    sem.local_table.add_inst(span.0..span.1, resolved_id);
                                    symbol_lapper.insert(Interval {
                                        start: span.0,
                                        stop: span.1,
                                        val: SymbolType::FunctionRef(resolved_id),
                                    });
                                } else {
                                    let decl_id = sem.local_table.add_declare_with_name(
                                        &self.uri,
                                        span.0..span.1,
                                        func_name,
                                        None,
                                    );
                                    // ★ Register in inst_id_to_declare_inst for RefDefMap Layer 1d
                                    sem.local_table.add_inst(span.0..span.1, decl_id);
                                    symbol_lapper.insert(Interval {
                                        start: span.0,
                                        stop: span.1,
                                        val: SymbolType::ClassRef(decl_id),
                                    });
                                }
                                // ★ Scan funcall arguments for port refs
                                // (e.g. uC.power([VDD_3V3,GND]) → VDD_3V3 refs)
                                // Walk ALL descendants of the funcall node — not just
                                // next siblings (which may be at wrong nesting level).
                                if let Some(enclosing) = find_container(span.0) {
                                    let refs = Self::collect_funccall_arg_refs(
                                        node,
                                        &sem.local_table,
                                        &self.uri,
                                        &enclosing,
                                    );
                                    for (span, did) in refs {
                                        symbol_lapper.insert(Interval {
                                            start: span.start,
                                            stop: span.end,
                                            val: SymbolType::InstanceRef(did),
                                        });
                                        sem.symbol_scope
                                            .insert((span.start, span.end), enclosing.clone());
                                    }
                                }
                            }
                        }
                    }
                }

                // Second pass A: pick up refs registered after the first pass
                // (e.g. interface attr variable references)
                // Also uses InstanceRef(DeclareId) via reverse mapping.
                let mut inst_to_decl2: std::collections::HashMap<ReferenceId, DeclareId> =
                    std::collections::HashMap::new();
                for (decl_id, inst_ids) in sem.local_table.declare_inst_to_inst_ids.iter() {
                    for iid in inst_ids {
                        inst_to_decl2.insert(*iid, *decl_id);
                    }
                }
                for (inst_id, span) in sem.local_table.inst_id_to_span.iter() {
                    let decl_id = inst_to_decl2
                        .get(inst_id)
                        .copied()
                        .unwrap_or(DeclareId::default());
                    symbol_lapper.insert(Interval {
                        start: span.start,
                        stop: span.end,
                        val: SymbolType::InstanceRef(decl_id),
                    });
                }

                // ★ RefDefMap Layer 2 is built after consolidate_ref_def_map
                // below — moved outside this lock scope so consolidate doesn't overwrite.

                sem.symbol_lapper = symbol_lapper;
            }
            Err(e) => {
                tracing::error!(target: "mcc::code", error = %e, "symbols mutex poisoned (create_lapper)")
            }
        }
        // ★ Layer 1 + name index — build after lapper is complete and lock released.
        self.consolidate_ref_def_map();

        // ★ Layer 2 — merge after Layer 1 so entries aren't overwritten.
        let (lapper_snapshot, scope_snapshot) = self
            .symbols
            .lock()
            .ok()
            .map(|s| (s.symbol_lapper.clone(), s.symbol_scope.clone()))
            .unwrap_or_else(|| {
                (
                    SymbolRangeLapper::new(vec![]),
                    std::collections::HashMap::new(),
                )
            });
        if let Ok(mut sem) = self.symbols.lock() {
            if let Some(ref mut map) = sem.ref_def_map {
                Self::fill_refdef_layer2(map, &lapper_snapshot, &scope_snapshot, &self.uri);
            }
        }
    }

    pub fn pass2(&mut self) {}
}
