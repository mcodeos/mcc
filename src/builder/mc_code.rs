// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::ast::ast_node::McValueFFI;
use crate::ast::ast_semantic::{
    DeclareId, McSemSymbols, ReferenceId, Span, SymbolRangeLapper, SymbolType,
};
use crate::ast::ast_token::McSemTokens;
use crate::ast::error::message::MISSING_SUBNODE;
use crate::builder::diagnostic::dlog_error;
use crate::builder::global;
use crate::builder::mc_use::McUse;
use crate::builder::workspace;
use crate::core::mc_enum::McEnumDef;
use crate::core::mc_ifs::McInterface;
use std::sync::atomic::{AtomicBool, Ordering};

/// Global deduplication flag: each parse cycle outputs AST visit only once
/// Reset at the mcc_load_project entry point (mcb_reset_ast_visit_flag)
pub static AST_VISIT_DONE: AtomicBool = AtomicBool::new(false);

pub fn mcb_reset_ast_visit_flag() {
    AST_VISIT_DONE.store(false, Ordering::SeqCst);
}
use crate::{ast::ast_node::AstNode, ast::c_macros::*, core::common::McCMIE};
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
    pub(crate) mcbase: bool,                             //is mcode base lib
    pub(crate) uri: McURI,                               //absolute path+filename of this .mc file
    pub(crate) ast: AstNode,                             //head pointer to the ast
    pub(crate) tokens: Arc<Mutex<McSemTokens>>,          //tokens parsed during ast parse
    pub(crate) symbols: Arc<Mutex<McSemSymbols>>,        //semantic symbols defined in this file
    pub(crate) uselist: Vec<McUse>,                      //
    pub(crate) spacenames: BTreeMap<McIds, McSpaceName>, //
    pub(crate) line_index: Option<LineIndex>, //line index for position to line/column conversion
    pub(crate) pass1_complete: bool,          // tracks whether parse_pass1_types() has been called
    pub(crate) modules_parsed: bool, // tracks whether parse_pass1_modules() has been called
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
        crate::builder::diagnostic::dlog_clear_file(&self.uri);

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
                    let location =
                        crate::builder::diagnostic::Location::new(self.uri.clone(), pos, len);
                    let diagnostic = crate::builder::diagnostic::Diagnostic::new(
                        1000, // E1000: parse error
                        crate::builder::diagnostic::DiagnosticLevel::Error,
                        location,
                        "syntax error".to_string(),
                    );
                    workspace::WORKSPACE
                        .diagnostics
                        .borrow_mut()
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
        crate::builder::diagnostic::dlog_clear_file(&self.uri);

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
                    let location =
                        crate::builder::diagnostic::Location::new(self.uri.clone(), pos, len);
                    let diagnostic = crate::builder::diagnostic::Diagnostic::new(
                        1000, // E1000: parse error
                        crate::builder::diagnostic::DiagnosticLevel::Error,
                        location,
                        "syntax error".to_string(),
                    );
                    workspace::WORKSPACE
                        .diagnostics
                        .borrow_mut()
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
        crate::builder::diagnostic::dlog_clear_file(&self.uri);

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
                    let location =
                        crate::builder::diagnostic::Location::new(self.uri.clone(), pos, len);
                    let diagnostic = crate::builder::diagnostic::Diagnostic::new(
                        1000, // E1000: parse error
                        crate::builder::diagnostic::DiagnosticLevel::Error,
                        location,
                        "syntax error".to_string(),
                    );
                    workspace::WORKSPACE
                        .diagnostics
                        .borrow_mut()
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
        if let Some(existing) = workspace::WORKSPACE.mcodes.borrow().get(&canonical_uri) {
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
            let existing_spacenames: BTreeMap<McIds, McSpaceName>;
            let existing_uselist: Vec<McUse>;
            {
                if let Some(existing) = workspace::WORKSPACE.mcodes.borrow().get(&canonical_use_uri)
                {
                    existing_spacenames = existing.spacenames.clone();
                    existing_uselist = existing.uselist.clone();
                } else {
                    existing_spacenames = BTreeMap::new();
                    existing_uselist = Vec::new();
                }
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
            if !existing_spacenames.is_empty() && !existing_uselist.is_empty() {
                // Reuse existing spacenames
                for (key, value) in &existing_spacenames {
                    if !self.spacenames.contains_key(key) {
                        self.spacenames.insert(key.clone(), value.clone());
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
                .borrow()
                .get(&canonical_use_uri)
                .map(|e| !e.modules_parsed)
                .unwrap_or(true);
            if should_insert {
                if let dashmap::Entry::Occupied(mut entry) = workspace::WORKSPACE
                    .mcodes
                    .borrow()
                    .entry(canonical_use_uri.clone())
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
                                    let components_guard = global::mcc_components.borrow_mut();
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
                                    let modules_guard = global::mcc_modules.borrow();
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
                                    let ifs_guard = global::mcc_interfaces.borrow_mut();
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
                                        let enums_guard = global::mcc_enums.borrow_mut();
                                        enums_guard
                                            .entry(space_name.clone())
                                            .and_modify(|_| {
                                                dlog_error(1504, &node, "Duplicate enum");
                                            })
                                            .or_insert(arc_enum.clone());
                                    } else {
                                        let enums_guard = workspace::WORKSPACE.enums.borrow_mut();
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
                        let space_name = McSpaceName {
                            ident: ifs.name.clone(),
                            uri: self.uri.clone(),
                        };
                        if self.mcbase {
                            global::mcc_interfaces
                                .borrow_mut()
                                .entry(space_name)
                                .and_modify(|_| {
                                    dlog_error(1001, &node, "Duplicate interface");
                                })
                                .or_insert(Arc::new(ifs));
                        } else {
                            workspace::WORKSPACE
                                .interfaces
                                .borrow_mut()
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
                                    .borrow_mut()
                                    .entry(space_name)
                                    .and_modify(|_| {
                                        dlog_error(1002, &node, "Duplicate component");
                                    })
                                    .or_insert(Arc::new(comp));
                            } else {
                                workspace::WORKSPACE
                                    .components
                                    .borrow()
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
                                .borrow()
                                .entry(space_name)
                                .and_modify(|_| {
                                    dlog_error(1004, &node, "Duplicate enum");
                                })
                                .or_insert(Arc::new(enum_def));
                        } else {
                            workspace::WORKSPACE
                                .enums
                                .borrow()
                                .entry(space_name)
                                .and_modify(|_| {
                                    dlog_error(1004, &node, "Duplicate enum");
                                })
                                .or_insert(Arc::new(enum_def));
                        }
                    }
                }
                MCAST_DEFINE => {
                    if let Some(def) = crate::core::mc_define::McDefineDef::new(&node, &self.uri) {
                        let space_name = McSpaceName {
                            ident: def.name.clone(),
                            uri: self.uri.clone(),
                        };
                        if self.mcbase {
                            global::mcc_defines
                                .borrow()
                                .entry(space_name)
                                .and_modify(|_| {
                                    dlog_error(1505, &node, "Duplicate define");
                                })
                                .or_insert(Arc::new(def));
                        } else {
                            workspace::WORKSPACE
                                .defines
                                .borrow()
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

        self.parse_pass1_modules();
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

    /// Extract (key_name, span) for spec-like attribute keys.
    fn extract_spec_key_spans(comp: &McComponent) -> Vec<(String, std::ops::Range<usize>)> {
        comp.attrs
            .iter()
            .filter_map(|a| a.key_span.clone().map(|s| (a.id.to_string(), s)))
            .collect()
    }

    pub fn parse_pass1_modules(&mut self) {
        if self.modules_parsed {
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
                    workspace::WORKSPACE
                        .modules
                        .borrow()
                        .insert(key, Arc::new(module));
                }
            }
        }
        // ★ Fix: Build the lapper after processing all modules.
        // mcb_parse_all_modules() does remove+insert on the McCode, creating a new McCode instance.
        // This new instance has the same Arc<Mutex<McSemSymbols>> (shared symbol data),
        // but create_lapper() was NOT called on it, so symbol_lapper was empty.
        // Call create_lapper here to ensure the lapper is built for the current file.
        self.create_lapper();
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
        // ★ LSP: Also register in workspace global_class_table for cross-context lookup
        if let Some(class_id) = result {
            tracing::info!(target: "mcc::lsp", "  add_global_class: registered '{}' ({}) in '{}' -> class_id={:?}", class_name, kind.as_str(), uri, class_id);
            let mut table = workspace::WORKSPACE.global_class_table.lock().unwrap();
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
                let local_id =
                    symbols
                        .local_table
                        .add_declare_with_name(span.clone(), name.clone(), None);
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
                // Then check local table
                symbols
                    .local_table
                    .name_to_declare_id
                    .get(&(McURI::new(), String::new(), name.to_string()))
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
        crate::builder::workspace::WORKSPACE
            .global_inst_table
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
    fn lookup_declare_id(
        local: &crate::ast::ast_semantic::LocalSymbolTable,
        uri: &str,
        name: &str,
        scope_path: &crate::ScopePath,
        local_inst_map: &std::collections::HashMap<String, crate::ast::ast_semantic::DeclareId>,
    ) -> Option<crate::ast::ast_semantic::DeclareId> {
        let ref_scope = scope_path.scope_key();

        // P1: exact scope match (e.g. "US513.i2c" for func body ref)
        let exact_key = (McURI::new(), ref_scope.clone(), name.to_string());
        if let Some(id) = local.name_to_declare_id.get(&exact_key).copied() {
            return Some(id);
        }

        // P2: container-level match (e.g. "US513" for module-level ref)
        if scope_path.func.is_some() {
            // If inside a func, also try the parent container scope
            let container_key = (
                McURI::new(),
                scope_path.container.name.clone(),
                name.to_string(),
            );
            if let Some(id) = local.name_to_declare_id.get(&container_key).copied() {
                return Some(id);
            }
        }

        // P3: file-level: check local_inst_map (DeclareInstances in same file)
        if let Some(id) = local_inst_map.get(name).copied() {
            return Some(id);
        }

        // P4: global inst table (cross-file)
        if let Some(id) = Self::lookup_global_inst(uri, name, Some(&ref_scope)) {
            return Some(id);
        }
        if let Some(id) = Self::lookup_global_inst(uri, name, None) {
            return Some(id);
        }

        // P5: unscoped fallback in local table
        let unscoped_key = (McURI::new(), String::new(), name.to_string());
        local.name_to_declare_id.get(&unscoped_key).copied()
    }

    pub fn create_lapper(&mut self) {
        tracing::info!(target: "mcc::lsp", "[LAPPER_DEBUG] create_lapper START uri={}", self.uri);
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
                            let mut decl_refs = crate::builder::workspace::WORKSPACE
                                .global_declare_class_refs
                                .lock()
                                .unwrap();
                            tracing::info!(target: "mcc::lsp", "  create_lapper: global_declare_class_refs for '{}' = {} entries", self.uri, decl_refs.get(&self.uri).map(|v| v.len()).unwrap_or(0));
                            if let Some(refs) = decl_refs.remove(&self.uri) {
                                for (decl_span, _class_id, target_uri, target_span) in refs {
                                    tracing::info!(target: "mcc::lsp", "  create_lapper: register decl_span={:?} -> class_id={:?} target={}:{:?}", decl_span, _class_id, target_uri, target_span);
                                    let refid = gt.add_declare_class(
                                        &self.uri,
                                        decl_span.clone(),
                                        _class_id,
                                    );
                                    // Store target span so goto-def can resolve cross-file
                                    gt.declare_id_to_target_span
                                        .insert(refid, (target_uri.clone(), target_span.clone()));
                                }
                            }
                        }

                        // Now iterate span_to_declare_class_id (which now includes entries
                        // from global_declare_class_refs above) and insert into symbol_lapper
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
                    let inst_table = crate::builder::workspace::WORKSPACE
                        .global_inst_table
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
                    let inst_table = crate::builder::workspace::WORKSPACE
                        .global_inst_table
                        .lock()
                        .unwrap();
                    let refs = inst_table.get_all_refs_for_uri(uri_str);
                    let count = refs.len();
                    for (decl_id, _ref_scope, ref_span) in &refs {
                    }
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

                // ★ Build name→DeclareId map from DeclareInstance entries in the lapper.
                // Local instances (e.g. `MCU.US513_20_F uC`) are NOT in name_to_declare_id
                // or global_inst_table — they only exist as lapper entries.
                let mut local_inst_map: std::collections::HashMap<String, DeclareId> =
                    std::collections::HashMap::new();
                if let Ok(src) = std::fs::read_to_string(self.uri.as_str()) {
                    for entry in symbol_lapper.iter() {
                        if let SymbolType::DeclareInstance(did) = entry.val {
                            if let Some(n) = src.get(entry.start..entry.stop) {
                                let bare = n
                                    .trim_end_matches(|c: char| {
                                        c == '(' || c == '{' || c == ')' || c == '}'
                                    })
                                    .split(|c: char| c == ',' || c.is_whitespace())
                                    .next()
                                    .unwrap_or("");
                                if !bare.is_empty() {
                                    local_inst_map.entry(bare.to_string()).or_insert(did);
                                }
                            }
                        }
                    }
                }

                // ★ LSP: Add interface definitions + param port_definitions
                {
                    let interfaces = crate::builder::workspace::WORKSPACE.interfaces.borrow();
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
                                    if let crate::core::component::mc_attr::McAttrVal::AttrVariable(opd, Some(span)) = val {
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
                                    &local_inst_map,
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
                    drop(interfaces);

                    let global_interfaces = crate::builder::global::mcc_interfaces.borrow();
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
                                    if let crate::core::component::mc_attr::McAttrVal::AttrVariable(opd, Some(span)) = val {
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
                                    &local_inst_map,
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
                    let modules = crate::builder::workspace::WORKSPACE.modules.borrow();
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
                                &local_inst_map,
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
                        for (span, port_name, scope) in m.params.iter_port_refs() {
                            let sp = Self::scope_path_from_scope_str(&self.uri, scope);
                            let decl_id = Self::lookup_declare_id(
                                &sem.local_table,
                                self.uri.as_str(),
                                port_name,
                                &sp,
                                &local_inst_map,
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
                            if let Ok(mut ginst) = crate::builder::workspace::WORKSPACE
                                .global_inst_table
                                .lock()
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
                    let modules = crate::builder::workspace::WORKSPACE.modules.borrow();
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
                                    &local_inst_map,
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
                                if let Ok(mut ginst) = crate::builder::workspace::WORKSPACE
                                    .global_inst_table
                                    .lock()
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
                    let components = crate::builder::workspace::WORKSPACE.components.borrow();
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
                        // ★ G8: Spec key definitions from component attrs
                        for (key_name, key_span) in Self::extract_spec_key_spans(comp) {
                            let sdecl_id = sem.local_table.add_declare_with_name(
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
                                &local_inst_map,
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
                            if let Ok(mut ginst) = crate::builder::workspace::WORKSPACE
                                .global_inst_table
                                .lock()
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
                        crate::core::basic::mc_ids::McIds::new(&ids_node).map(|ids| ids.to_string())
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
                            let base = crate::core::basic::mc_ids::McIds::new(&first_id)
                                .map(|i| i.to_string())?;
                            let mem = crate::core::basic::mc_ids::McIds::new(&member_node)
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
                        crate::core::basic::mc_ids::McIds::new(node).map(|ids| ids.to_string())
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
                                    let enums_guard =
                                        crate::builder::workspace::WORKSPACE.enums.borrow();
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
                                    let sys_enums_guard =
                                        crate::builder::global::mcc_enums.borrow();
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
                        let modules = workspace::WORKSPACE.modules.borrow();
                        for entry in modules.iter() {
                            let key_uri = entry.key().uri.as_str();
                            if key_uri == uri_str
                                || key_uri.ends_with(uri_str)
                                || uri_str.ends_with(key_uri)
                            {
                                container_names.push(entry.key().ident.to_string());
                            }
                        }
                        let comps = workspace::WORKSPACE.components.borrow();
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
                        for entry in global::mcc_modules.borrow().iter() {
                            let key_uri = entry.key().uri.as_str();
                            if key_uri == uri_str
                                || key_uri.ends_with(uri_str)
                                || uri_str.ends_with(key_uri)
                            {
                                container_names.push(entry.key().ident.to_string());
                            }
                        }
                        for entry in global::mcc_components.borrow().iter() {
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
                    let default_container = container_names.first().cloned();

                    for node in &all_nodes {
                        let ntype = node.get_type();
                        if ntype == MCAST_FUNCTION {
                            // ★ Fix: use MCAST_IDS (the actual name) for span, not
                            // MCAST_NAME which may cover the entire func body and
                            // shadow instance_ref entries inside func bodies.
                            let ids_node = node.get_sub_node()
                                .and_then(|n| n.get_sub_node()); // MCAST_NAME -> MCAST_IDS
                            let span = if let Some(ref ids) = ids_node {
                                (ids.get_pos() as usize, (ids.get_pos() + ids.get_len()) as usize)
                            } else if let Some(name_node) = node.get_sub_node() {
                                (name_node.get_pos() as usize, (name_node.get_pos() + name_node.get_len()) as usize)
                            } else {
                                continue;
                            };
                            if let Some(name_node) = node.get_sub_node() {
                                let enclosing = default_container.clone();
                                let func_name = ids_node
                                    .and_then(|n| crate::core::basic::mc_ids::McIds::new(&n))
                                    .map(|ids| ids.to_string());
                                let scope = match (&enclosing, &func_name) {
                                    (Some(m), Some(f)) => Some(format!("{m}.{f}")),
                                    _ => func_name.clone(),
                                };
                                let decl_id = sem.local_table.add_declare_with_name(
                                    span.0..span.1,
                                    func_name,
                                    scope.as_deref(),
                                );
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
                                        crate::core::basic::mc_ids::McIds::new(&name_node)
                                            .map(|ids| ids.to_string())
                                            .unwrap_or_default()
                                    });
                                    for (pname, pspan) in
                                        Self::extract_func_param_spans(&params_node)
                                    {
                                        let pdecl_id = sem.local_table.add_declare_with_name(
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
                                        crate::builder::mcb_register_instance_decl(
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
                                let enclosing = default_container.clone();
                                let decl_id = sem.local_table.add_declare_with_name(
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
                                let enclosing = default_container.clone();
                                let decl_id = sem.local_table.add_declare_with_name(
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
                                let id_node = nn.get_sub_node().unwrap_or_else(|| nn.clone());
                                let span = (
                                    id_node.get_pos() as usize,
                                    (id_node.get_pos() + id_node.get_len()) as usize,
                                );
                                let has_instance = sub
                                    .as_ref()
                                    .map(|s| s.get_type() == MCAST_INSTANCE)
                                    .unwrap_or(false);
                                let func_name = crate::core::basic::mc_ids::McIds::new(&nn)
                                    .map(|ids| ids.to_string());
                                if has_instance {
                                    // For method calls, try to reuse the FunctionDefinition's
                                    // ID so gotodef can resolve local same-file jumps.
                                    let resolved_id = func_name
                                        .as_ref()
                                        .and_then(|n| {
                                            sem.local_table.name_to_declare_id.get(&(
                                                McURI::new(),
                                                String::new(),
                                                n.clone(),
                                            ))
                                        })
                                        .copied()
                                        .unwrap_or_else(|| {
                                            sem.local_table.add_declare_with_name(
                                                span.0..span.1,
                                                func_name.clone(),
                                                None,
                                            )
                                        });
                                    symbol_lapper.insert(Interval {
                                        start: span.0,
                                        stop: span.1,
                                        val: SymbolType::FunctionRef(resolved_id),
                                    });
                                } else {
                                    let decl_id = sem.local_table.add_declare_with_name(
                                        span.0..span.1,
                                        func_name,
                                        None,
                                    );
                                    symbol_lapper.insert(Interval {
                                        start: span.0,
                                        stop: span.1,
                                        val: SymbolType::ClassRef(decl_id),
                                    });
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

                // Second pass B: fix up method_ref IDs to match their
                // function_definition so gotodef can resolve same-file jumps.
                // Read source once, build function name→ID map, then patch.
                if let Ok(source) = std::fs::read_to_string(self.uri.as_str()) {
                    let mut func_name_to_id: std::collections::HashMap<String, DeclareId> =
                        std::collections::HashMap::new();
                    for entry in symbol_lapper.iter() {
                        if let SymbolType::FunctionDefinition(did) = entry.val {
                            if let Some(sig) = source.get(entry.start..entry.stop) {
                                if let Some(name) = sig
                                    .split(|c: char| c == '(' || c == '{' || c.is_whitespace())
                                    .next()
                                {
                                    func_name_to_id.entry(name.to_string()).or_insert(did);
                                }
                            }
                        }
                    }
                    let mut patches: Vec<(usize, usize, DeclareId)> = Vec::new();
                    for entry in symbol_lapper.iter() {
                        if let SymbolType::FunctionRef(_mid) = entry.val {
                            if let Some(ref_name) = source.get(entry.start..entry.stop) {
                                let ref_name =
                                    ref_name.trim_end_matches(|c: char| c == '(' || c == '{');
                                if let Some(&fd_id) = func_name_to_id.get(ref_name) {
                                    patches.push((entry.start, entry.stop, fd_id));
                                }
                            }
                        }
                    }
                    for (s, e, fd_id) in patches {
                        symbol_lapper.insert(Interval {
                            start: s,
                            stop: e,
                            val: SymbolType::FunctionRef(fd_id),
                        });
                    }

                    // Second pass C: generate declare_instance cross_file_targets.
                    // For each declare_instance with non-empty scope (usage-side),
                    // find the matching definition (declare_instance with empty scope+same name).
                    //
                    // NOTE: PortDefinition-based sub-element linking has been removed.
                    // Sub-element (port/param/pin) lookup now goes through Phase 2
                    // `lookup_sub_def()` instead of direct cross_file_targets entries.
                    {
                        // Build name→def_span from declare_instance with empty scope (top-level defs)
                        let mut def_map: std::collections::HashMap<
                            (String, String),
                            (usize, usize),
                        > = std::collections::HashMap::new();
                        for entry in symbol_lapper.iter() {
                            if let SymbolType::DeclareInstance(_) = entry.val {
                                let scope = sem
                                    .symbol_scope
                                    .get(&(entry.start, entry.stop))
                                    .cloned()
                                    .unwrap_or_default();
                                // ★ Fix: also include non-empty-scope definitions so
                                // module-level instances (scope="US513") can be found
                                // by cross_file_targets for references inside func bodies.
                                if let Some(name) = source.get(entry.start..entry.stop) {
                                    let bare = name.trim_end_matches(|c: char| {
                                        c == '(' || c == '{' || c == ')' || c == '}'
                                    });
                                    let name = bare
                                        .split(|c: char| c == ',' || c.is_whitespace())
                                        .next()
                                        .unwrap_or("");
                                    if !name.is_empty() {
                                        def_map
                                            .entry((scope.clone(), name.to_string()))
                                            .or_insert((entry.start, entry.stop));
                                    }
                                }
                            }
                        }
                        // Link usage-side declare_instance to definitions via cross_file_targets
                        if let Ok(mut gtable) = sem.global_table.lock() {
                            for entry in symbol_lapper.iter() {
                                if let SymbolType::DeclareInstance(did) = entry.val {
                                    let scope = sem
                                        .symbol_scope
                                        .get(&(entry.start, entry.stop))
                                        .cloned()
                                        .unwrap_or_default();
                                    // ★ Fix: don't skip empty-scope entries;
                                    // module-level defs have non-empty scope.
                                    if let Some(name) = source.get(entry.start..entry.stop) {
                                        let bare = name.trim_end_matches(|c: char| {
                                            c == '(' || c == '{' || c == ')' || c == '}'
                                        });
                                        let name = bare
                                            .split(|c: char| c == ',' || c.is_whitespace())
                                            .next()
                                            .unwrap_or("");
                                        // Try scoped match first, then unscoped fallback
                                        let scoped_key = (scope.clone(), name.to_string());
                                        let empty_key = ("".to_string(), name.to_string());
                                        if let Some(&(def_s, def_e)) = def_map.get(&scoped_key)
                                            .or_else(|| def_map.get(&empty_key))
                                        {
                                            gtable
                                                .declare_inst_to_target_span
                                                .entry(did)
                                                .or_insert((self.uri.clone(), def_s..def_e));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                sem.symbol_lapper = symbol_lapper;
            }
            Err(e) => {
                tracing::error!(target: "mcc::code", error = %e, "symbols mutex poisoned (create_lapper)")
            }
        }
    }

    pub fn pass2(&mut self) {}
}
