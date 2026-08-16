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
use crate::db::diagnostic::errcodes;
use crate::db::infra::global;
use crate::db::infra::mc_use::{McUse, McUsePrefix};
use crate::semantic::mc_enum::McEnumDef;
use crate::semantic::mc_ifs::McInterface;
use crate::{ast::ast_node::AstNode, ast::c_macros::*, semantic::common::McCMIE};
use crate::{current_uri, mcb_loaded_libs, McComponent, McIds, McModule, McSpaceName, McURI};
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
    /// The trailing u8 is the class's CMIE kind (Component/Module/Interface/Enum
    /// or 255 UNKNOWN), captured at registration so Layer 1c entries carry it.
    cross_file_targets: Vec<(
        crate::ast::ast_semantic::DeclareId,
        McURI,
        std::ops::Range<usize>,
        u8,
    )>,
}

/// §11/§19: validate an unprefixed (system/third-party) `use` target against
/// the loaded-library set.
///
/// Non-project context (no project.toml reachable from the current file):
/// lazily load the library from disk; E2051 fires only when it does not
/// exist. Project context: strict check — the library must be declared in
/// project.toml [dependencies] (or loaded via --lib / global config);
/// otherwise E2051 "undeclared dependency" (use-design §19.5 rule 2).
fn check_system_use_lib(mcuse: &McUse, current_path: &Path) {
    // `orig_uri` is the module path (e.g. "acme/res/res"); the library name is
    // its first segment. Strip any defensive `@version` suffix.
    let lib_name = mcuse
        .orig_uri
        .split('/')
        .next()
        .unwrap_or("")
        .split('@')
        .next()
        .unwrap_or("");
    if lib_name.is_empty() {
        return;
    }
    if mcb_loaded_libs().contains(&lib_name.to_string()) {
        return;
    }
    if !manifest_reachable_from(current_path) {
        // Non-project context: lazy load when the library exists on disk.
        if let Some(root) = crate::db::infra::libmgr::resolve_lib_root(lib_name) {
            if crate::db::infra::libmgr::mcb_load_lib(lib_name, &root) {
                return; // loaded — no diagnostic
            }
        }
        // The library is truly absent: report "not found" instead of the
        // project-mode "undeclared dependency" message.
        dlog_warning_at(
            crate::errcodes::USE_DEP_NOT_DECLARED,
            mcuse.pos,
            mcuse.len,
            &format!(
                "library '{lib_name}' not found in the system root; install it with `mcc lib install` or load it with --lib"
            ),
        );
        return;
    }
    // Project context: strict declaration check.
    dlog_warning_at(
        crate::errcodes::USE_DEP_NOT_DECLARED,
        mcuse.pos,
        mcuse.len,
        &crate::errcodes::format_msg(crate::errcodes::USE_DEP_NOT_DECLARED, &[&lib_name]),
    );
}

/// Walk up from `start` looking for a project manifest. Accepts the same
/// three candidate names as the unified project-root discovery
/// (`manifest.toml` / `project.toml` / `mcc.toml`), so use validation and
/// project-root resolution agree on what counts as a project.
fn manifest_reachable_from(start: &Path) -> bool {
    let mut current = Some(start);
    while let Some(dir) = current {
        for name in ["manifest.toml", "project.toml", "mcc.toml"] {
            if dir.join(name).exists() {
                return true;
            }
        }
        current = dir.parent();
    }
    false
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

            // ★ Push line_index onto thread-local stack so
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
                // Parser errors are below PARSER_WARNING_CODE_BASE; warnings are more specific.
                raw.sort_by_key(|e| (e.2, e.3)); // sort by pos, then len
                let mut last_end: u32 = 0;
                for (code, level, pos, len, msg) in &raw {
                    if *pos < last_end && *code < errcodes::PARSER_WARNING_CODE_BASE {
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
                    if *pos < last_end && *code < errcodes::PARSER_WARNING_CODE_BASE {
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
                    if *pos < last_end && *code < errcodes::PARSER_WARNING_CODE_BASE {
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

            // §11/§19: check that unprefixed (system/third-party) use targets
            // are declared (project context) or lazily loaded (non-project).
            if mcuse.prefix == McUsePrefix::PathSystem {
                check_system_use_lib(&mcuse, current_path);
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
                                    crate::errcodes::USE_SYMBOL_CONFLICT,
                                    mcuse.pos,
                                    mcuse.len,
                                    &crate::errcodes::format_msg(
                                        crate::errcodes::USE_SYMBOL_CONFLICT,
                                        &[&module_name, &names.join(", "), &prev_uri],
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
                    space_name.uri = crate::semantic::common::uri_intern(&canonical_use_uri);
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
                                crate::errcodes::USE_IMPORTED_NOT_FOUND,
                                mcuse.pos,
                                mcuse.len,
                                &crate::errcodes::format_msg(
                                    crate::errcodes::USE_IMPORTED_NOT_FOUND,
                                    &[&class, &mcuse.orig_uri],
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

            // §11/§19: check that unprefixed (system/third-party) use targets
            // are declared (project context) or lazily loaded (non-project).
            if mcuse.prefix == McUsePrefix::PathSystem {
                check_system_use_lib(&mcuse, current_path);
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
                                    crate::errcodes::USE_SYMBOL_CONFLICT,
                                    mcuse.pos,
                                    mcuse.len,
                                    &crate::errcodes::format_msg(
                                        crate::errcodes::USE_SYMBOL_CONFLICT,
                                        &[&module_name, &names.join(", "), &prev_uri],
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
                                crate::errcodes::USE_IMPORTED_NOT_FOUND,
                                mcuse.pos,
                                mcuse.len,
                                &crate::errcodes::format_msg(
                                    crate::errcodes::USE_IMPORTED_NOT_FOUND,
                                    &[&class, &mcuse.orig_uri],
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
        // (name, declaration node type) — needed to exempt legal
        // enum + component/interface same-name coexistence (§2.3 of
        // same-name-enum-component.md: enum+component namespaces merge).
        let mut cmie_types: Vec<(McIds, u16)> = Vec::new();
        for node in self.ast.iter() {
            if node.is_type(MCAST_INTERFACE)
                || node.is_type(MCAST_COMPONENT)
                || node.is_type(MCAST_MODULE)
                || node.is_type(MCAST_ENUM)
                || node.is_type(MCAST_DEFINE)
            {
                let decl_type = node.get_type();
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
                        // P0-3: enum + component/interface sharing a name is
                        // legal (namespace merge). Other collisions still error.
                        let exempt = cmie_types.iter().any(|(existing, t)| {
                            *existing == class_name
                                && (Self::is_enum_decl(*t) != Self::is_enum_decl(decl_type))
                        });
                        if !exempt {
                            dlog_error(
                                crate::errcodes::DEF_ALREADY_EXISTS,
                                &node,
                                &crate::errcodes::format_msg(
                                    crate::errcodes::DEF_ALREADY_EXISTS,
                                    &[],
                                ),
                            );
                        }
                    } else {
                        self.spacenames.insert(
                            class_name.clone(),
                            McSpaceName::new(&class_name, self.uri.clone()),
                        );
                        cmies.push(class_name.clone());
                    }
                    cmie_types.push((class_name, decl_type));
                }
            }
        }
        cmies
    }

    /// True when the declaration AST type is an `enum`.
    ///
    /// Used by [`parse_cmie_names`] to exempt legal enum + component/interface
    /// same-name coexistence (§2.3 of same-name-enum-component.md).
    fn is_enum_decl(decl_type: u16) -> bool {
        decl_type == MCAST_ENUM
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
                                            uri: crate::semantic::common::uri_intern(&self.uri),
                                        })
                                        .and_modify(|_| {
                                            dlog_error(
                                                crate::errcodes::DUP_COMPONENT,
                                                &node,
                                                &crate::errcodes::format_msg(
                                                    crate::errcodes::DUP_COMPONENT,
                                                    &[],
                                                ),
                                            );
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
                                            uri: crate::semantic::common::uri_intern(&self.uri),
                                        })
                                        .and_modify(|_| {
                                            dlog_error(
                                                crate::errcodes::DUP_MODULE,
                                                &node,
                                                &crate::errcodes::format_msg(
                                                    crate::errcodes::DUP_MODULE,
                                                    &[],
                                                ),
                                            );
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
                                            uri: crate::semantic::common::uri_intern(&self.uri),
                                        })
                                        .and_modify(|_| {
                                            dlog_error(
                                                crate::errcodes::DUP_INTERFACE,
                                                &node,
                                                &crate::errcodes::format_msg(
                                                    crate::errcodes::DUP_INTERFACE,
                                                    &[],
                                                ),
                                            );
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
                                        uri: crate::semantic::common::uri_intern(&self.uri),
                                    };
                                    let arc_enum = Arc::new(enum_def);
                                    if self.mcbase {
                                        let enums_guard = &global::mcc_enums;
                                        enums_guard
                                            .entry(space_name.clone())
                                            .and_modify(|_| {
                                                dlog_error(
                                                    crate::errcodes::DUP_ENUM,
                                                    &node,
                                                    &crate::errcodes::format_msg(
                                                        crate::errcodes::DUP_ENUM,
                                                        &[],
                                                    ),
                                                );
                                            })
                                            .or_insert(arc_enum.clone());
                                    } else {
                                        let enums_guard = &workspace::WORKSPACE.enums;
                                        enums_guard
                                            .entry(space_name.clone())
                                            .and_modify(|_| {
                                                dlog_error(
                                                    crate::errcodes::DUP_ENUM,
                                                    &node,
                                                    &crate::errcodes::format_msg(
                                                        crate::errcodes::DUP_ENUM,
                                                        &[],
                                                    ),
                                                );
                                            })
                                            .or_insert(arc_enum.clone());
                                    }
                                    return Some(McCMIE::Enum(arc_enum));
                                }
                            }
                            MCAST_DEFINE => {
                                // P1-10: a define cannot be represented as an
                                // McCMIE (component/module/interface/enum only).
                                // Report the mismatch instead of panicking.
                                dlog_error(
                                    crate::errcodes::CMIE_IS_DEFINE,
                                    &node,
                                    &crate::errcodes::format_msg(
                                        crate::errcodes::CMIE_IS_DEFINE,
                                        &[&name],
                                    ),
                                );
                                return None;
                            }
                            // Defensive fallback: the outer scan guard only admits
                            // the five declaration types above, so this arm is
                            // unreachable in practice. Keep it diagnostic-based
                            // rather than panicking (P1-10).
                            _ => {
                                dlog_error(
                                    crate::errcodes::CMIE_LOAD_REJECTED,
                                    &node,
                                    &crate::errcodes::format_msg(
                                        crate::errcodes::CMIE_LOAD_REJECTED,
                                        &[&node.get_type() as &dyn std::fmt::Display],
                                    ),
                                );
                                return None;
                            }
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
                            uri: crate::semantic::common::uri_intern(&self.uri),
                        };
                        if self.mcbase {
                            global::mcc_interfaces
                                .entry(space_name)
                                .and_modify(|_| {
                                    dlog_error(
                                        crate::errcodes::DUP_INTERFACE,
                                        &node,
                                        &crate::errcodes::format_msg(
                                            crate::errcodes::DUP_INTERFACE,
                                            &[],
                                        ),
                                    );
                                })
                                .or_insert(Arc::new(ifs));
                        } else {
                            workspace::WORKSPACE
                                .interfaces
                                .entry(space_name)
                                .and_modify(|_| {
                                    dlog_error(
                                        crate::errcodes::DUP_INTERFACE,
                                        &node,
                                        &crate::errcodes::format_msg(
                                            crate::errcodes::DUP_INTERFACE,
                                            &[],
                                        ),
                                    );
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
                            uri: crate::semantic::common::uri_intern(&self.uri),
                        };
                        {
                            if self.mcbase {
                                global::mcc_components
                                    .entry(space_name)
                                    .and_modify(|_| {
                                        dlog_error(
                                            crate::errcodes::DUP_COMPONENT,
                                            &node,
                                            &crate::errcodes::format_msg(
                                                crate::errcodes::DUP_COMPONENT,
                                                &[],
                                            ),
                                        );
                                    })
                                    .or_insert(Arc::new(comp));
                            } else {
                                workspace::WORKSPACE
                                    .components
                                    .entry(space_name)
                                    .and_modify(|_| {
                                        dlog_error(
                                            crate::errcodes::DUP_COMPONENT,
                                            &node,
                                            &crate::errcodes::format_msg(
                                                crate::errcodes::DUP_COMPONENT,
                                                &[],
                                            ),
                                        );
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
                            uri: crate::semantic::common::uri_intern(&self.uri),
                        };
                        if self.mcbase {
                            global::mcc_enums
                                .entry(space_name)
                                .and_modify(|_| {
                                    dlog_error(
                                        crate::errcodes::DUP_ENUM,
                                        &node,
                                        &crate::errcodes::format_msg(
                                            crate::errcodes::DUP_ENUM,
                                            &[],
                                        ),
                                    );
                                })
                                .or_insert(Arc::new(enum_def));
                        } else {
                            workspace::WORKSPACE
                                .enums
                                .entry(space_name)
                                .and_modify(|_| {
                                    dlog_error(
                                        crate::errcodes::DUP_ENUM,
                                        &node,
                                        &crate::errcodes::format_msg(
                                            crate::errcodes::DUP_ENUM,
                                            &[],
                                        ),
                                    );
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
                            uri: crate::semantic::common::uri_intern(&self.uri),
                        };
                        if self.mcbase {
                            global::mcc_defines
                                .entry(space_name)
                                .and_modify(|_| {
                                    dlog_error(
                                        crate::errcodes::DUP_DEFINE,
                                        &node,
                                        &crate::errcodes::format_msg(
                                            crate::errcodes::DUP_DEFINE,
                                            &[],
                                        ),
                                    );
                                })
                                .or_insert(Arc::new(def));
                        } else {
                            workspace::WORKSPACE
                                .defines
                                .entry(space_name)
                                .and_modify(|_| {
                                    dlog_error(
                                        crate::errcodes::DUP_DEFINE,
                                        &node,
                                        &crate::errcodes::format_msg(
                                            crate::errcodes::DUP_DEFINE,
                                            &[],
                                        ),
                                    );
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
                // Simple param (e.g. `spi`): McIds parses the node directly.
                if let Some(ids) = McIds::new(&inner) {
                    let span =
                        (inner.get_pos() as usize)..((inner.get_pos() + inner.get_len()) as usize);
                    result.push((ids.to_string(), span));
                    continue;
                }
                // Typed power param (e.g. `[V3V3, GND]::DC(3.3V)`): walk the
                // subtree for square-vec nodes and register each member with
                // its precise span, so func body refs resolve to the members.
                let before = result.len();
                Self::collect_square_vec_member_spans(&inner, &mut result);
                if result.len() == before {
                    // Single-ID typed param (e.g. `V3V3::DC(3.3V)`): the parser
                    // emits DECLARE(class=TYPE, instance=NAME) here. Extract the
                    // instance identifier so func body refs resolve to this
                    // param def (not to another func's same-named param).
                    Self::collect_declare_instance_spans(&inner, &mut result);
                }
            }
        }
        result
    }

    /// Walk a param subtree for the single-ID typed form `NAME::TYPE(...)`,
    /// which the parser emits as DECLARE(class=TYPE, instance=NAME). Extract
    /// the instance identifier (`NAME`) with its precise span.
    fn collect_declare_instance_spans(
        node: &AstNode,
        out: &mut Vec<(String, std::ops::Range<usize>)>,
    ) {
        if node.get_type() == MCAST_INSTANCE {
            if let Some(sub) = node.get_sub_node() {
                // instance → opd → ids (or direct ids)
                let ids_node = if sub.get_type() == MCAST_OPD {
                    sub.get_sub_node().unwrap_or_else(|| sub.clone())
                } else {
                    sub.clone()
                };
                if let Some(ids) = McIds::new(&ids_node) {
                    let span = (ids_node.get_pos() as usize)
                        ..((ids_node.get_pos() + ids_node.get_len()) as usize);
                    out.push((ids.to_string(), span));
                }
            }
            return; // instance children already captured
        }
        if let Some(sub) = node.get_sub_node() {
            let mut cur = sub;
            loop {
                Self::collect_declare_instance_spans(&cur, out);
                match cur.get_next() {
                    Some(nx) => cur = nx,
                    None => break,
                }
            }
        }
    }

    /// Walk a param subtree for square-vec nodes (`[A, B]::DC(...)`) and push
    /// each member with its precise span.
    fn collect_square_vec_member_spans(
        node: &AstNode,
        out: &mut Vec<(String, std::ops::Range<usize>)>,
    ) {
        if matches!(node.get_type(), MCAST_SQUARE_VEC | MCAST_OPD_SQUARE_VEC) {
            let mut current = node.get_sub_node();
            while let Some(phrase_node) = current {
                let ids_node = phrase_node
                    .get_sub_node()
                    .unwrap_or_else(|| phrase_node.clone());
                if let Some(ids) = McIds::new(&ids_node) {
                    let span = (ids_node.get_pos() as usize)
                        ..((ids_node.get_pos() + ids_node.get_len()) as usize);
                    out.push((ids.to_string(), span));
                }
                current = phrase_node.get_next();
            }
            return; // do not descend into members again
        }
        if let Some(sub) = node.get_sub_node() {
            let mut cur = sub;
            loop {
                Self::collect_square_vec_member_spans(&cur, out);
                match cur.get_next() {
                    Some(nx) => cur = nx,
                    None => break,
                }
            }
        }
    }

    /// Walk a whole subtree (e.g. a component `func` node) and collect every
    /// square-vec member (name, span) found inside. Used to register refs such
    /// as `[VCC, VSS]` in component func bodies that name the component's own
    /// pins.
    fn collect_square_vec_members_in_subtree(
        node: &AstNode,
        out: &mut Vec<(String, std::ops::Range<usize>)>,
    ) {
        if matches!(node.get_type(), MCAST_SQUARE_VEC | MCAST_OPD_SQUARE_VEC) {
            Self::collect_square_vec_member_spans(node, out);
            return; // members already extracted; do not descend again
        }
        if let Some(sub) = node.get_sub_node() {
            let mut cur = sub;
            loop {
                Self::collect_square_vec_members_in_subtree(&cur, out);
                match cur.get_next() {
                    Some(nx) => cur = nx,
                    None => break,
                }
            }
        }
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

    /// Parse all modules and (re)build the symbol lapper.
    ///
    /// Returns `true` when this call (re)built the lapper (full parse or
    /// use-table-dirty rebuild), `false` when it was a no-op because the
    /// lapper is already up to date (`modules_parsed` set, table not dirty).
    /// Callers that need to guarantee a fresh lapper must rebuild when this
    /// returns `false` — this lets `mcb_parse_all_modules` avoid the second
    /// redundant `create_lapper()` for files this call just built.
    pub fn parse_pass1_modules(&mut self) -> bool {
        if self.modules_parsed && !self.use_table_dirty {
            return false;
        }
        // ★ §7.6: Use table dirty — only rebuild RefDefMap/name_index,
        // no need to re-parse modules.
        if self.modules_parsed && self.use_table_dirty {
            self.create_lapper(); // includes inline Layer 2 + consolidate (Layer 1 + name_index)
            self.use_table_dirty = false;
            return true;
        }
        self.modules_parsed = true;

        // ★ Module parsing resolves instance classes through the P4 use chain
        //   (`db/resolve/visibility.rs::use_chain_reaches`), which starts from
        //   this file's own `uselist`. Callers remove the file from `mcodes`
        //   while parsing takes `&mut self` (see `mcb_parse_all_modules`), so
        //   the walk cannot read the uselist from `mcodes` — stash it on the
        //   thread-local stack for the duration of the parse.
        let _parsing_uses_guard = crate::db::infra::context::ParsingUsesGuard::new(
            self.uri.clone(),
            self.uselist.clone(),
        );

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
                        uri: crate::semantic::common::uri_intern(&self.uri),
                    };
                    // ★ Register module in class_name_to_id so
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
                            dlog_error(
                                crate::errcodes::DUP_MODULE,
                                &node,
                                &crate::errcodes::format_msg(crate::errcodes::DUP_MODULE, &[]),
                            );
                        })
                        .or_insert(Arc::new(module));
                }
            }
        }
        // Build the lapper after processing all modules so that
        // module-level symbols are registered before ref resolution.
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
        true
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
                    crate::semantic::mc_inst::McInstance::BusRef { .. } => {
                        // `comp.bus.member` — the base resolved to a bus ref,
                        // so the member is a bus member, not a port.
                        return SymbolKind::BusMemberRef;
                    }
                    _ => {}
                }
            }
        }
        // Plain name (no dot): check if it's a Component/Module instance
        // before defaulting to PortRef. This ensures instance references like
        // `X6` in `X6.setup(...)` are classified as InstRef, not PortRef,
        // so the tooltip shows "instance" instead of "label".
        // ★ Declareb inference (`idx::CLASS(...)`): 2-pin declareb names
        // (`C4::CAP()`) bypass parse_declare, so they are absent from insts;
        // the parse-time hint classifies them as instance refs.
        if let Some((kind, _)) = insts.declareb_def(port_name) {
            if kind == SymbolKind::InstDef {
                return SymbolKind::InstRef;
            }
        }
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
            // Class-chain fallback (bare class names like `RES` / `CAP`): the
            // hit is a ClassDef, so the ref must be a ClassRef — not the
            // FuncParamRef catch-all (which would duplicate the ClassRef that
            // lapper_global_classes already registers at the same span).
            SymbolKind::ClassDef => SymbolKind::ClassRef,
            _ => SymbolKind::FuncParamRef,
        }
    }

    /// Fallback for func-body chain refs whose root is one of the component's
    /// own pins (e.g. `VIN.Vin` inside `func enable` — `VIN` is a pin defined
    /// by `pins = [...]`, not a func-local instance, so `resolve_member_chain`
    /// misses it). Resolves the chain root against the component pin declares
    /// and returns the ref kind for the pin family.
    fn resolve_func_chain_own_pin(
        uri: &McURI,
        segments: &[crate::refdef::types::ChainSegment],
        comp_ident: &str,
        comp: &McComponent,
        sem: &mut McSemSymbols,
    ) -> Option<(DeclareId, SymbolKind)> {
        // Rebuild the dotted text (`VIN.Vin`) and extract the chain root.
        let full_name: String = segments
            .iter()
            .filter_map(|s| match s {
                crate::refdef::types::ChainSegment::Ident(name) => Some(name.clone()),
                crate::refdef::types::ChainSegment::Group { base, members } => {
                    Some(format!("{}{{{}}}", base, members.join(",")))
                }
                crate::refdef::types::ChainSegment::Fcall(_) => None,
            })
            .collect::<Vec<_>>()
            .join(".");
        let root = match segments.first() {
            Some(crate::refdef::types::ChainSegment::Ident(name)) => name.as_str(),
            Some(crate::refdef::types::ChainSegment::Group { base, .. }) => base.as_str(),
            _ => return None,
        };
        // The root must be one of the component's own pins.
        let pin_names: std::collections::HashSet<String> = Self::extract_pin_name_spans(comp)
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        if !pin_names.contains(root) {
            return None;
        }
        let file_id = crate::ast::ast_semantic::intern(&mut sem.file_table, uri.as_str());
        let comp_id = crate::ast::ast_semantic::intern(&mut sem.container_table, comp_ident);
        // Prefer the exact dotted form (`VIN.Vin`), then the root (`VIN`).
        let mut target: &str = &full_name;
        loop {
            if let Some((id, _)) =
                sem.local_table
                    .name_to_declare_id
                    .get(&(file_id, comp_id, 0, target.to_string()))
            {
                let kind = if Self::extract_pin_iface_spans(comp)
                    .iter()
                    .any(|(n, _)| n == root)
                {
                    SymbolKind::PinIfaceRef
                } else {
                    SymbolKind::PinNameRef
                };
                return Some((*id, kind));
            }
            if target == root {
                return None;
            }
            target = root;
        }
    }

    /// Resolve the AST-driven def name for `(def_kind, decl_id)` at `def_uri`.
    ///
    /// Chains the reliable sources in order:
    /// 1. the caller's local `def_names` table (same-file defs — Layer 2 ids
    ///    are minted locally by `register_def`, so every Layer 2 def hits here);
    /// 2. reverse lookup in the current global table (classes/enums minted here
    ///    when a cross-file ref resolved via `add_class`/`add_enum_class`).
    ///
    /// The def file's own tables are NEVER probed with a caller-side id — its
    /// ids live in a different id space, and a same-id class there would alias
    /// the wrong name (`CAP` id=9 here vs `CAP.SAFETY` id=9 in cap.mc). The one
    /// exception is EnumValDef: a packed enum value_id carries the *def file's*
    /// class_id (`(class_id << 16) | value_idx`), so it is looked up in the def
    /// file's gt (`with_def_file_gt`). A miss returns "" — never a guess.
    fn def_name_for(
        gt: &crate::ast::ast_semantic::GlobalSymbolTable,
        local_names: &std::collections::HashMap<
            (crate::ast::ast_semantic::SymbolKind, u32),
            String,
        >,
        def_uri: &str,
        def_kind: crate::ast::ast_semantic::SymbolKind,
        decl_id: u32,
    ) -> String {
        let n = crate::refdef::matching::resolve_def_name(local_names, def_uri, def_kind, decl_id);
        if !n.is_empty() {
            return n;
        }
        if def_kind == crate::ast::ast_semantic::SymbolKind::EnumValDef {
            return Self::enum_value_def_name(gt, def_uri, decl_id);
        }
        Self::class_def_name(gt, def_uri, def_kind, decl_id)
    }

    /// Run `f` against the def file's own GlobalSymbolTable — the workspace file
    /// or the loaded system library that owns `def_uri`.
    ///
    /// Cross-file ids are cast in the *def file's* id space (its gt's class_id
    /// counter), so a reverse lookup by `(def_uri, id)` may only hit there, not in
    /// the current file's gt. Uses `try_lock` to avoid deadlock while another pass
    /// is mid-parse on that file.
    fn with_def_file_gt<R>(
        def_uri: &str,
        f: impl Fn(&crate::ast::ast_semantic::GlobalSymbolTable) -> Option<R>,
    ) -> Option<R> {
        let mut result = None;
        if let Some(code) = crate::db::cmie::tables::WORKSPACE.mcodes.get(def_uri) {
            if let Ok(sem) = code.symbols.try_lock() {
                if let Ok(g) = sem.global_table.try_lock() {
                    result = f(&g);
                }
            }
        }
        if result.is_none() {
            for b in crate::db::infra::libmgr::mcc_blibs.iter() {
                if b.uri == def_uri {
                    if let Ok(sem) = b.symbols.try_lock() {
                        if let Ok(g) = sem.global_table.try_lock() {
                            result = f(&g);
                        }
                    }
                    break;
                }
            }
        }
        result
    }

    /// Reverse-lookup an AST-driven class/enum name by `(def_uri, decl_id)`.
    ///
    /// The id was registered in this table when the class/enum was parsed
    /// (same-file) or when a class ref resolved cross-file (`add_class`), so the
    /// name is the real AST name (e.g. `RES` in the mcode library) — never a text
    /// slice of the def line. Only the current file's gt is consulted: its ids
    /// are minted by `add_class`/`add_enum_class` for every class this file
    /// references, so a ClassRef that resolved here always has its (def_uri, id)
    /// entry in this table. Probing the def file's gt with a caller-side id
    /// would alias a different same-id class in the def file's own id space
    /// (e.g. `CAP` id=9 here vs `CAP.SAFETY` id=9 in cap.mc) — never guess.
    fn class_def_name(
        gt: &crate::ast::ast_semantic::GlobalSymbolTable,
        def_uri: &str,
        def_kind: crate::ast::ast_semantic::SymbolKind,
        decl_id: u32,
    ) -> String {
        let table = if def_kind == crate::ast::ast_semantic::SymbolKind::EnumDef {
            &gt.enum_class_name_to_id
        } else {
            &gt.class_name_to_id
        };
        table
            .iter()
            .find(|((u, _n), c)| u == def_uri && u32::from(**c) == decl_id)
            .map(|((_u, n), _c)| n.to_string())
            .unwrap_or_default()
    }

    /// Resolve an enum value def name from the AST-driven workspace/global enum
    /// tables. `value_id` packs (class_id << 16) | value_index (§refdef).
    fn enum_value_def_name(
        gt: &crate::ast::ast_semantic::GlobalSymbolTable,
        def_uri: &str,
        value_id: u32,
    ) -> String {
        let class_id = value_id >> 16;
        let idx = (value_id & 0xFFFF) as usize;
        let lookup_ident = |g: &crate::ast::ast_semantic::GlobalSymbolTable| {
            g.enum_class_name_to_id
                .iter()
                .find(|((u, _n), c)| u == def_uri && u32::from(**c) == class_id)
                .map(|((_u, n), _c)| n.clone())
        };
        let enum_ident = lookup_ident(gt).or_else(|| Self::with_def_file_gt(def_uri, lookup_ident));
        let Some(ident) = enum_ident else {
            return String::new();
        };
        let space = crate::semantic::common::McSpaceName {
            ident,
            uri: crate::semantic::common::uri_intern(&def_uri),
        };
        if let Some(e) = crate::db::cmie::tables::WORKSPACE.enums.get(&space) {
            if let Some(v) = e.values.get(idx) {
                return v.name.to_string();
            }
        }
        if let Some(e) = crate::db::infra::global::mcc_enums.get(&space) {
            if let Some(v) = e.values.get(idx) {
                return v.name.to_string();
            }
        }
        String::new()
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
                        let def_name = Self::def_name_for(
                            &gt,
                            &sem.def_names,
                            def_uri,
                            SymbolKind::ClassDef,
                            u32::from(*class_id),
                        );
                        // ★ Resolve the class's real CMIE kind (Component/
                        // Module/Interface/Enum) by name + def uri so a same-file
                        // ref to an `interface`/`enum` hovers as `→ interface` /
                        // `→ enum` instead of a generic `→ class`.
                        let cmie_kind = crate::query::refs::cmie_kind_for(def_uri, &def_name);
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
                                cmie_kind,
                                def_name,
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
            // of ReferenceId, matching the ID space used by the lapper. The
            // trailing u8 is the class's CMIE kind, captured at registration so
            // a cross-file ref to an `interface` hovers as `→ interface`.
            for (class_id, def_uri, span, kind) in &self.cross_file_targets {
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
                        cmie_kind: *kind,
                        def_name: Self::def_name_for(
                            &gt,
                            &sem.def_names,
                            def_uri,
                            SymbolKind::ClassDef,
                            u32::from(*class_id),
                        ),
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
                        def_name: Self::def_name_for(
                            &gt,
                            &sem.def_names,
                            def_uri,
                            SymbolKind::EnumValDef,
                            *value_id,
                        ),
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
                        def_name: Self::def_name_for(
                            &gt,
                            &sem.def_names,
                            def_uri,
                            SymbolKind::EnumDef,
                            *ref_id,
                        ),
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
                    def_name: name.to_string(),
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
                                let src_uri = crate::semantic::common::uri_of_file_id(
                                    src_entry.def_loc.file_id,
                                );
                                let src_container = if src_entry.def_loc.container_id != u32::MAX {
                                    target_map
                                        .containers
                                        .get(src_entry.def_loc.container_id as usize)
                                        .map(|c| c.as_str())
                                        .unwrap_or("")
                                } else {
                                    ""
                                };
                                let new_fid = if src_uri.is_empty() {
                                    map.intern_file(&self.uri)
                                } else {
                                    map.intern_file(&McURI::from(src_uri.as_ref()))
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
                                    def_name: src_entry.def_name.clone(),
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
                                def_name: class_name.to_string(),
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
                                def_name: class_name.to_string(),
                            };
                            map.add_name_alias(&self.uri, &class_name.to_string(), entry);
                        }
                    }
                }
            }
        }

        tracing::info!(
            target: "mcc::lsp",
            "consolidate_ref_def_map: uri={} entries={} containers={} names={}",
            self.uri, map.entries.len(), map.containers.len(),
            map.name_index.len()
        );

        // Write back to symbols
        if let Ok(mut sem) = self.symbols.lock() {
            sem.ref_def_map = Some(map);
        }
    }

    /// True when two URIs refer to the same file. Prefers canonicalized
    /// (symlink-resolved) comparison; falls back to a normalized string
    /// comparison so relative/absolute/`./`-prefixed spellings of the same
    /// path match without substring false positives.
    fn uris_same_file(a: &str, b: &str) -> bool {
        if a == b {
            return true;
        }
        let norm = |s: &str| -> String {
            let mut out = std::path::PathBuf::new();
            for c in std::path::Path::new(s).components() {
                match c {
                    std::path::Component::CurDir => {}
                    std::path::Component::ParentDir => {
                        out.push("..");
                    }
                    other => out.push(other.as_os_str()),
                }
            }
            out.to_string_lossy().to_string()
        };
        if norm(a) == norm(b) {
            return true;
        }
        match (
            std::fs::canonicalize(std::path::Path::new(a)),
            std::fs::canonicalize(std::path::Path::new(b)),
        ) {
            (Ok(ca), Ok(cb)) => ca == cb,
            _ => false,
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
            // Drop def_map entries for this file too. They were registered
            // during the previous lapper build; the name_to_declare_id keys
            // that carried their ids are gone (retain above), so a rebuild
            // allocates fresh DeclareIds. Without this cleanup the old
            // generation stays behind as ghost (def_kind, decl_id) entries
            // that no ref references, doubling def_map and polluting the
            // span-based LabelRef/BusRef synthesis in fill_refdef_layer2.
            // Cross-file entries (loc.file_id != this file) are kept: their
            // name_to_declare_id keys survive the retain, so their ids are
            // reused and they never duplicate.
            sem.def_map.retain(|_, loc| loc.file_id != file_id);
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
                // ★ Component defs must be registered before func-role resolution:
                // funcall-arg refs (e.g. `CAP(...).Cap([AVDD09_CAP, GND])`) rely on
                // the container-level P2 fallback finding the component's own pins.
                Self::lapper_component_defs_register(&self.uri, &mut sem, &mut symbol_lapper);

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
                Self::lapper_component_func_pin_refs(
                    &self.uri,
                    &self.ast,
                    &mut sem,
                    &mut symbol_lapper,
                );
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
        let (scope_snapshot, def_map_snapshot, ref_entries_snapshot, def_names_snapshot) = self
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
                (
                    scope_map,
                    s.def_map.clone(),
                    s.ref_entries.clone(),
                    s.def_names.clone(),
                )
            })
            .unwrap_or_else(|| {
                (
                    std::collections::HashMap::new(),
                    std::collections::HashMap::new(),
                    Vec::new(),
                    std::collections::HashMap::new(),
                )
            });
        if let Ok(mut sem) = self.symbols.lock() {
            let file_table = sem.file_table.clone(); // clone before mutable borrow
            if let Some(ref mut map) = sem.ref_def_map {
                crate::refdef::matching::fill_refdef_layer2(
                    map,
                    &scope_snapshot,
                    &def_map_snapshot,
                    &def_names_snapshot,
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

        // collect (name, param_type, def_span) directly from the
        // param tables. The old code looked up map.name_index — which only
        // contains class-level names, never params — so the upgrade loop was
        // dead (no entry ever matched). Matching on the param def span bytes
        // works because entries carry the def loc produced by register_def.
        let mut param_defs: Vec<(String, McParamTypeKind, std::ops::Range<usize>)> = Vec::new();

        let collect =
            |params: &crate::semantic::basic::mc_paramd::McParamDeclares,
             acc: &mut Vec<(String, McParamTypeKind, std::ops::Range<usize>)>| {
                for (name, span) in params.iter_defs_with_span() {
                    if let Some(decl) = params.find(name) {
                        acc.push((name.to_string(), decl.param_type.kind.clone(), span.clone()));
                    }
                }
            };

        // Modules
        for entry in crate::db::cmie::tables::WORKSPACE.modules.iter() {
            if entry.key().uri == uri.as_str() {
                collect(&entry.value().params, &mut param_defs);
            }
        }
        // Components
        for entry in crate::db::cmie::tables::WORKSPACE.components.iter() {
            if entry.key().uri == uri.as_str() {
                collect(&entry.value().params, &mut param_defs);
                for func in entry.value().funcs.iter() {
                    collect(&func.params, &mut param_defs);
                }
            }
        }
        // Interfaces
        for entry in crate::db::cmie::tables::WORKSPACE.interfaces.iter() {
            if entry.key().uri == uri.as_str() {
                collect(&entry.value().params, &mut param_defs);
            }
        }
        // Func params (nested inside modules)
        for entry in crate::db::cmie::tables::WORKSPACE.modules.iter() {
            if entry.key().uri == uri.as_str() {
                for func in entry.value().funcs.iter() {
                    collect(&func.params, &mut param_defs);
                }
            }
        }

        if param_defs.is_empty() {
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

        // Find and upgrade UnknownDef entries — match the entry def span
        // against the param def span directly.
        let mut upgrades: Vec<((SymbolKind, u32), SymbolKind)> = Vec::new();
        for ((kind, ref_id), entry) in &map.entries {
            if entry.def_kind != SymbolKind::UnknownDef {
                continue;
            }
            for (_name, pt_kind, def_span) in &param_defs {
                let new_kind = kind_map(pt_kind);
                if new_kind == SymbolKind::UnknownDef {
                    continue;
                }
                if entry.def_loc.byte_start == def_span.start as u32
                    && entry.def_loc.byte_end == def_span.end as u32
                {
                    upgrades.push(((*kind, *ref_id), new_kind));
                    break;
                }
            }
        }

        for ((ref_kind, ref_id), new_kind) in upgrades {
            // keep the (ref_kind, ref_id) key untouched — only the
            // def_kind is upgraded. The old code re-inserted under the def
            // kind, which rewrote the key and broke every consumer querying
            // entries by (ref_kind, id). Sync the def_to_refs reverse index
            // so find-all-references still sees the ref under the new kind.
            if let Some(entry) = map.entries.get_mut(&(ref_kind, ref_id)) {
                let old_kind = entry.def_kind;
                if old_kind == new_kind {
                    continue;
                }
                entry.def_kind = new_kind;
                let old_key = (
                    old_kind,
                    entry.def_loc.file_id,
                    entry.def_loc.byte_start,
                    entry.def_loc.byte_end,
                );
                if let Some(vec) = map.def_to_refs.get_mut(&old_key) {
                    vec.retain(|r| *r != (ref_kind, ref_id));
                }
                let new_key = (
                    new_kind,
                    entry.def_loc.file_id,
                    entry.def_loc.byte_start,
                    entry.def_loc.byte_end,
                );
                map.def_to_refs
                    .entry(new_key)
                    .or_default()
                    .push((ref_kind, ref_id));
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
    /// would collapse dotted names such as `comp.sub` from `[Ida, DotIda]`
    /// into a single `Ida`, breaking the structural `Eq` used by the tables).
    fn resolve_class_ref_at_span(
        ref_uri: &McURI,
        class_name: &McIds,
        gt: &mut crate::ast::ast_semantic::GlobalSymbolTable,
        sem: &McSemSymbols,
    ) -> Option<(DeclareId, McURI, std::ops::Range<usize>, u8)> {
        if class_name.segments.is_empty() {
            return None;
        }

        // ★ Use the unified resolution policy (P3→P4→P5) instead of manual
        // table-by-table searches. Runs through resolve_class_locked: the
        // caller (create_lapper) already holds this file's symbols lock, and
        // re-locking it through mcb_get_cmie would self-deadlock (std Mutex is
        // not reentrant).
        if let Some(cmie) =
            crate::db::resolve::Resolver::resolve_class_locked(ref_uri, class_name, sem)
        {
            let (def_uri, def_span, cmie_kind) = match &cmie {
                crate::semantic::common::McCMIE::Component(c) => (
                    c.uri.clone(),
                    c.span.clone(),
                    crate::refdef::types::CmieKind::Component as u8,
                ),
                crate::semantic::common::McCMIE::Module(m) => (
                    m.uri.clone(),
                    m.span.clone(),
                    crate::refdef::types::CmieKind::Module as u8,
                ),
                crate::semantic::common::McCMIE::Interface(i) => (
                    i.uri.clone(),
                    i.span.clone(),
                    crate::refdef::types::CmieKind::Interface as u8,
                ),
                crate::semantic::common::McCMIE::Enum(e) => {
                    let s = e.span;
                    (
                        e.uri.clone(),
                        s[0] as usize..s[1] as usize,
                        crate::refdef::types::CmieKind::Enum as u8,
                    )
                }
            };
            // Check if already registered; if so return existing id,
            // otherwise register now. Key is (McURI, McIds): O(1) lookup via
            // normalized McIds Eq/Hash (DotIda/Curly equivalence).
            // enums live in enum_class_name_to_id, never in
            // class_name_to_id — add_class would mint a second, unrelated id.
            // Route enum classes to the enum tables so ClassDef/EnumRef stay
            // in a single ID space.
            let cid = match &cmie {
                crate::semantic::common::McCMIE::Enum(_) => gt
                    .enum_class_name_to_id
                    .get(&(def_uri.clone(), class_name.clone()))
                    .copied()
                    .unwrap_or_else(|| gt.add_enum_class(&def_uri, class_name, def_span.clone())),
                _ => gt
                    .class_name_to_id
                    .get(&(def_uri.clone(), class_name.clone()))
                    .copied()
                    .unwrap_or_else(|| gt.add_class(&def_uri, class_name, def_span.clone())),
            };
            return Some((cid, def_uri, def_span, cmie_kind));
        }

        None
    }

    fn lapper_global_classes(
        uri: &McURI,
        cross_file_targets: &mut Vec<(
            crate::ast::ast_semantic::DeclareId,
            McURI,
            std::ops::Range<usize>,
            u8,
        )>,
        sem: &mut McSemSymbols,
        symbol_lapper: &mut DedupLapper,
    ) {
        match sem.global_table.lock() {
            Ok(mut gt) => {
                let classes: Vec<(McURI, McIds, crate::ast::ast_semantic::DeclareId)> = gt
                    .class_name_to_id
                    .iter()
                    .filter(|((u, _clsname), _clsid)| u == uri)
                    .map(|((u, n), c)| (u.clone(), n.clone(), *c))
                    .collect();

                for (_class_uri, class_name, clsid) in &classes {
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
                        // ★ Capture the class def name from the global table key
                        // (AST-driven) so RefDefMap RPC payloads carry it.
                        sem.def_names
                            .insert((SymbolKind::ClassDef, id), class_name.to_string());
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
                        for (
                            decl_span,
                            _class_id,
                            target_uri,
                            target_span,
                            class_name,
                            cmie_kind,
                        ) in refs
                        {
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

                            let (local_class_id, ref_target_uri, ref_target_span, resolved_kind) =
                                if target_uri.is_empty() {
                                    // Sentinel: unresolved during registration
                                    if let Some(resolved) = Self::resolve_class_ref_at_span(
                                        uri,
                                        &class_name,
                                        &mut gt,
                                        &sem,
                                    ) {
                                        (resolved.0, resolved.1, resolved.2, resolved.3)
                                    } else {
                                        // The entry was already removed above; put
                                        // it back so a later create_lapper pass
                                        // (after libraries load) can retry, instead
                                        // of dropping the class ref permanently and
                                        // leaving the span without any LSP data.
                                        decl_refs
                                            .entry(uri.as_str().to_string())
                                            .or_default()
                                            .push((
                                                decl_span,
                                                _class_id,
                                                String::new(),
                                                0..0,
                                                class_name.clone(),
                                                cmie_kind,
                                            ));
                                        continue;
                                    }
                                } else {
                                    // Normal case: target_uri/target_span are
                                    // already correct. Register in local table
                                    // to get a locally-unique DeclareId.
                                    let cid = {
                                        let mut found = None;
                                        for ((u, name), &existing_cid) in gt.class_name_to_id.iter()
                                        {
                                            if name == &class_name && u == &target_uri {
                                                found = Some(existing_cid);
                                                break;
                                            }
                                        }
                                        // enums are keyed in
                                        // enum_class_name_to_id, never class_name_to_id.
                                        // Check the enum table before add_class so a
                                        // class ref to an enum does not mint a second,
                                        // unrelated id via add_class.
                                        if found.is_none() {
                                            if let Some(&eid) = gt
                                                .enum_class_name_to_id
                                                .get(&(target_uri.clone(), class_name.clone()))
                                            {
                                                found = Some(eid);
                                            }
                                        }
                                        found.unwrap_or_else(|| {
                                            gt.add_class(
                                                &target_uri,
                                                &class_name,
                                                target_span.clone(),
                                            )
                                        })
                                    };
                                    (cid, target_uri, target_span, cmie_kind)
                                };

                            let _refid =
                                gt.add_declare_class(&uri, decl_span.clone(), local_class_id);
                            // ★ Fix: push class_id (DeclareId) instead of refid
                            // (ReferenceId) so Layer 1c uses the same ID space as
                            // the lapper and ref_entries. The trailing u8 is the
                            // class's CMIE kind (Component/Module/Interface/Enum
                            // or 255 UNKNOWN), captured so Layer 1c entries carry
                            // it into the RefDefMap for hover labels.
                            cross_file_targets.push((
                                local_class_id,
                                ref_target_uri,
                                ref_target_span,
                                resolved_kind,
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
                        // class_id=0 is a valid first-class id (the
                        // per-file counter starts at 0), so it cannot be used as
                        // a "not found" sentinel. Skip the ref when the mapping is
                        // missing instead of emitting a bogus id=0 ClassRef that
                        // would match the file's first class in Layer 2.
                        if let Some(class_id) = gt.declare_id_to_class_id.get(refid).copied() {
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
                }

                for ((loop_uri, name), class_id) in gt.enum_class_name_to_id.iter() {
                    if loop_uri != uri {
                        continue;
                    }
                    if let Some((_u, span)) = gt.enum_class_id_to_span.get(class_id) {
                        let id = u32::from(*class_id);
                        symbol_lapper.insert(Interval {
                            start: span.start,
                            stop: span.end,
                            val: SymbolType::new(SymbolKind::EnumDef, id),
                        });
                        // ★ Fix: register enum head as EnumDef in def_map so
                        // fill_refdef_layer2's EnumRef → [EnumDef] lookup
                        // (`def_map.get(&(EnumDef, class_id))`) hits. Same-name
                        // enum + component heads (e.g. `enum CAP` + `component
                        // CAP` in one file) stay distinct in the dump.
                        let file_id =
                            crate::ast::ast_semantic::intern(&mut sem.file_table, uri.as_str());
                        sem.def_map.insert(
                            (SymbolKind::EnumDef, id),
                            crate::ast::ast_semantic::SourceLocation::new(
                                file_id,
                                0,
                                span.start as u32,
                                span.end as u32,
                            ),
                        );
                        // ★ Capture the enum head name from the global table key
                        // (AST-driven) so RefDefMap RPC payloads carry it.
                        sem.def_names
                            .insert((SymbolKind::EnumDef, id), name.to_string());
                    }
                }
                for (value_id, (loop_uri, span)) in gt.enum_value_id_to_span.iter() {
                    if loop_uri == uri {
                        // EnumValDef must also enter def_map so
                        // fill_refdef_layer2's EnumValRef → [EnumValDef] lookup
                        // (`def_map.get(&(EnumValDef, value_id))`) can hit. It
                        // only inserted into the lapper, so hover/find-refs on
                        // the value def site had no map entry (Layer 1e masked
                        // the miss by building its own table).
                        let file_id =
                            crate::ast::ast_semantic::intern(&mut sem.file_table, uri.as_str());
                        symbol_lapper.insert(Interval {
                            start: span.start,
                            stop: span.end,
                            val: SymbolType::new(SymbolKind::EnumValDef, u32::from(*value_id)),
                        });
                        sem.def_map.insert(
                            (SymbolKind::EnumValDef, u32::from(*value_id)),
                            crate::ast::ast_semantic::SourceLocation::new(
                                file_id,
                                0,
                                span.start as u32,
                                span.end as u32,
                            ),
                        );
                        // ★ Capture the enum value name from the AST node
                        // (unpack value_id → class + idx → McEnumDef.values)
                        // so RefDefMap RPC payloads carry it.
                        let value_id_raw = u32::from(*value_id);
                        let class_id = value_id_raw >> 16;
                        let idx = (value_id_raw & 0xFFFF) as usize;
                        // Look up the enum value name from the AST-derived
                        // workspace table (unpack value_id → class + idx).
                        let value_name = (|| {
                            let (class_uri, n) = gt
                                .enum_class_name_to_id
                                .iter()
                                .find(|((u, _n), c)| u == loop_uri && u32::from(**c) == class_id)
                                .map(|((u, n), _c)| (u.clone(), n.clone()))?;
                            let space = crate::semantic::common::McSpaceName {
                                ident: n,
                                uri: crate::semantic::common::uri_intern(&class_uri),
                            };
                            let e = crate::db::cmie::tables::WORKSPACE.enums.get(&space)?;
                            e.values.get(idx).map(|v| v.name.to_string())
                        })();
                        if let Some(vn) = value_name {
                            sem.def_names
                                .insert((SymbolKind::EnumValDef, value_id_raw), vn);
                        }
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
        // ── InstDef: declare_instance declarations as InstDef ──
        // Modules: `comp.sub uC` inside `mod.sub { ... }`.
        let modules = &crate::db::cmie::tables::WORKSPACE.modules;
        for entry in modules.iter() {
            if entry.key().uri != uri.as_str() {
                continue;
            }
            let m = entry.value();
            let mod_ident = entry.key().ident.to_string();
            for (inst_name, (_iotype, inst)) in m.insts.insts() {
                // ★ Declareb inference: the def kind follows the declared
                // class. Component/module instances (`C4::CAP()`) are InstDef;
                // interface instances (`vin::DC(5V)`, `[VDD,GND]::DC(3.3V)`)
                // are label/bus semantics and fall through to the module
                // port_spans loop, which registers them as LabelDef.
                match inst {
                    crate::semantic::mc_inst::McInstance::Component(_)
                    | crate::semantic::mc_inst::McInstance::Module(_) => {
                        if let Some(spans) = m.insts.port_spans().get(inst_name) {
                            // Only the first span is the declaration site
                            // (store_port_span is called first from
                            // parse_declare); later spans are use sites. Loop
                            // over every span polluted the lapper with InstDef
                            // intervals at reference positions and let the last
                            // span overwrite the def_map entry.
                            if let Some(span) = spans.first() {
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
        // Components: sub-instances declared inside a component body
        // (e.g. `U_MCU` in a `component` block) never got an InstDef because
        let comps = &crate::db::cmie::tables::WORKSPACE.components;
        for entry in comps.iter() {
            if entry.key().uri != uri.as_str() {
                continue;
            }
            let comp = entry.value();
            let comp_ident = entry.key().ident.to_string();
            for (inst_name, (_iotype, inst)) in comp.insts.insts() {
                // ★ Declareb inference: the def kind follows the declared
                // class. Component/module instances (`C4::CAP()`) are InstDef;
                // interface instances (`vin::DC(5V)`) are label/bus semantics,
                // so they register as LabelDef (a component body has no
                // port_spans loop to fall back on).
                let def_kind = match inst {
                    crate::semantic::mc_inst::McInstance::Component(_)
                    | crate::semantic::mc_inst::McInstance::Module(_) => Some(SymbolKind::InstDef),
                    crate::semantic::mc_inst::McInstance::Interface(_) => {
                        Some(SymbolKind::LabelDef)
                    }
                    _ => None,
                };
                if let Some(def_kind) = def_kind {
                    if let Some(spans) = comp.insts.port_spans().get(inst_name) {
                        if let Some(span) = spans.first() {
                            let (d, _) = crate::refdef::register::register_def(
                                sem,
                                uri,
                                &comp_ident,
                                None,
                                inst_name,
                                span.clone(),
                                def_kind,
                            );
                            symbol_lapper.insert(Interval {
                                start: span.start,
                                stop: span.end,
                                val: SymbolType::new(def_kind, u32::from(d)),
                            });
                            tracing::info!(target: "mcc::lsp::audit",
                                "[AUDIT-InstDef] name={inst_name} span={span:?} decl_id={d:?} kind={def_kind:?}");
                        }
                    }
                }
            }
            // ★ Declareb inference: 2-pin declareb inside a component body
            // (`C4::CAP()`) bypasses parse_declare, so the name never enters
            // insts. Register InstDef at the hint's declaration span so
            // component-body instances resolve like module-body ones.
            for (inst_name, (kind, span)) in comp.insts.iter_declareb_defs() {
                if *kind != SymbolKind::InstDef {
                    continue;
                }
                let (d, _) = crate::refdef::register::register_def(
                    sem,
                    uri,
                    &comp_ident,
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
                    "[AUDIT-InstDef] name={inst_name} span={span:?} decl_id={d:?} (declareb hint)");
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
            if Self::uris_same_file(iface.uri.as_str(), uri_str) {
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
                            // AttrVariable refs target the interface
                            // param def, so register as PortRef directly instead of
                            // add_inst (which later becomes InstRef and never matches
                            // ParamDef). Skip variables that are not params — do not
                            // fall back to the id=0 sentinel.
                            let var_name = opd.to_string();
                            if let Some(decl_id) = param_decl_ids.get(&var_name).copied() {
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
            if Self::uris_same_file(iface.uri.as_str(), uri_str) {
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
                            // AttrVariable refs target the interface
                            // param def, so register as PortRef directly instead of
                            // add_inst (which later becomes InstRef and never matches
                            // ParamDef). Skip variables that are not params — do not
                            // fall back to the id=0 sentinel.
                            let var_name = opd.to_string();
                            if let Some(decl_id) = param_decl_ids.get(&var_name).copied() {
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
                            // global-lib segment must use PortRef too —
                            // InstRef only maps to InstDef and never matched the
                            // ParamDef/UnknownDef defs registered above.
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
    }

    fn lapper_module_ports(uri: &McURI, sem: &mut McSemSymbols, symbol_lapper: &mut DedupLapper) {
        let modules = &crate::db::cmie::tables::WORKSPACE.modules;
        for entry in modules.iter() {
            let m = entry.value();
            if entry.key().uri != uri.as_str() {
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
                // ★ Declareb inference (`idx::CLASS(...)`): 2-pin declareb
                // names (`C4::CAP()`) carry a parse-time hint with their
                // inferred def kind (InstDef for component/module classes) and
                // declaration span. Register the def at the declaration span
                // only. Other spans are bare uses of the typed name (e.g. a
                // bare `C4` before `C4::CAP()`): the typed declaration is
                // authoritative, so bare uses are ref-only and must not mint a
                // second def (a stray LabelDef overlapping the InstRef).
                let hint = m.insts.declareb_def(name);
                for span in spans {
                    let def_kind = match &hint {
                        Some((kind, declareb_span)) if declareb_span == span => *kind,
                        Some(_) => continue,
                        _ => SymbolKind::LabelDef,
                    };
                    let (d, _) = crate::refdef::register::register_def(
                        sem,
                        uri,
                        &mod_ident2,
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
                        "[AUDIT-LabelDef] name={name} span={span:?} decl_id={d:?} kind={def_kind:?}");
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
                        // ★ Class-chain hits (bare class names like `CAP`/`RES`
                        // resolved via the P3-P5 base fallback) are already
                        // covered by lapper_global_classes, which emits the
                        // canonical ClassRef at every class-name span.
                        // Registering the class def again here mints a second
                        // DeclareId for the same class (e.g. RES id=13 vs
                        // id=199), so skip those hits.
                        if hit.def_kind != SymbolKind::ClassDef {
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
                        } else {
                            tracing::info!(target: "mcc::lsp::audit",
                                "[AUDIT-NetRef-Chain-SKIP] name={port_name} → {} (class hit handled by lapper_global_classes)",
                                hit.name);
                        }
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
                // ★ Base-instance ref: the base identifier of a dotted member
                // chain gets its own InstRef so hover / F12 on `spk` in
                // `spk.3` (or `USB_VBUS_1` in `USB_VBUS_1.GND`) resolves to
                // the base's own def instead of the whole-chain member target.
                let segs = crate::refdef::chain::split_segments(port_name);
                if segs.len() > 1 {
                    // Only register the base ref when the recorded span covers
                    // the whole chain (i.e. the span starts at the base).
                    // record_scoped_net_ref records dotted member refs with a
                    // member-only span — e.g. `GND` inside `[dc.GND, dc.GND]`
                    // is stored as name `dc.GND` at span [member start, member
                    // end]. register_chain_base_ref assumes span.start is the
                    // base position, so without this guard it would emit a
                    // PortRef at the member text (`GN` of `GND`), overlapping
                    // the BusMemberRef. The base ref for those member-only
                    // spans already exists (params net-ref on the base name).
                    if span.end - span.start >= port_name.len() {
                        Self::register_chain_base_ref(
                            sem,
                            symbol_lapper,
                            &uri,
                            &segs[0],
                            &span,
                            scope,
                            &m.insts,
                            &m.params,
                        );
                    }
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
                    // ★ Class-chain hits (a chain ending on a bare class name)
                    // are already covered by lapper_global_classes, which emits
                    // the canonical ClassRef at every class-name span —
                    // re-registering the class def here mints a second
                    // DeclareId for the same class. Skip those hits.
                    if hit.def_kind != SymbolKind::ClassDef {
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
                    // ★ Base-instance ref for the chain (see net-refs above).
                    if let Some(base) = segments
                        .first()
                        .map(crate::refdef::chain::base_segment_name)
                    {
                        Self::register_chain_base_ref(
                            sem,
                            symbol_lapper,
                            &uri,
                            &base,
                            &span,
                            scope,
                            &m.insts,
                            &m.params,
                        );
                    }
                }
            }
            // Module-param net refs (`m.params.iter_net_refs()`) reference the
            // module's own param/port declarations (e.g. the curly bus param
            // `USB_VBUS_1` in `module M(USB_VBUS_1{VDD_3V, GND}::DC(3.3V))` used
            // at `USB_VBUS_1 {VDD_3V, GND} -> ...`). Those defs are ParamDef /
            // PortDef, so the ref kind must be PortRef: fill_refdef_layer2 maps
            // InstRef only to InstDef, which would drop the entry and make F12
            // self-locate instead of jumping to the param declaration.
            for (span, port_name, scope) in m.params.iter_net_refs() {
                let sp = crate::refdef::register::scope_path_from_scope_str(&uri, scope);
                let decl_id =
                    crate::refdef::register::lookup_declare_id(&sem.local_table, port_name, &sp);
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
            let mod_ident_label = entry.key().ident.to_string();
            for (name, _label_kind, span) in m.insts.iter_labels_with_span() {
                // ★ Declareb inference: hint names are instances (`C4::CAP()`),
                // registered as InstDef by the port_spans loop — never label defs.
                if m.insts.declareb_def(name).is_some() {
                    continue;
                }
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

    /// Register an InstRef for the base identifier of a dotted member chain,
    /// covering only the base segment (the first `base.len()` bytes of `span`)
    /// and resolving to the base's own definition.
    ///
    /// Without this, the whole-chain ref (`spk.3`, `USB_VBUS_1.GND`) is the
    /// only interval covering the base text, so hover / F12 on the base
    /// identifier resolves to the member target (pin `3`, bus member `GND`).
    fn register_chain_base_ref(
        sem: &mut McSemSymbols,
        symbol_lapper: &mut DedupLapper,
        uri: &McURI,
        base: &str,
        span: &std::ops::Range<usize>,
        scope: &str,
        insts: &crate::semantic::mc_inst::McInstances,
        params: &crate::semantic::basic::mc_paramd::McParamDeclares,
    ) {
        if base.is_empty() {
            return;
        }
        let base_len = base.len();
        if base_len >= span.end - span.start {
            return;
        }
        let base_span = span.start..(span.start + base_len);
        let Some(hit) = crate::refdef::chain::resolve_base_hit(uri, base, insts, params) else {
            return;
        };
        let (d, _) = crate::refdef::register::register_def(
            sem,
            &hit.uri,
            scope,
            None,
            &hit.name,
            hit.span.clone(),
            hit.def_kind,
        );
        // ★ A param/port base (module param declaration) must resolve through
        // PortRef so fill_refdef_layer2 (PortRef → [PortDef, ParamDef]) matches
        // the ParamDef in def_map. InstRef only matches InstDef, which would
        // leave the base ref unresolved (RefDefMap miss → self-locate / hover
        // fallback) instead of jumping to the param declaration.
        let ref_kind = match hit.def_kind {
            SymbolKind::ParamDef | SymbolKind::PortDef => SymbolKind::PortRef,
            _ => SymbolKind::InstRef,
        };
        symbol_lapper.insert(Interval {
            start: base_span.start,
            stop: base_span.end,
            val: SymbolType::new(ref_kind, u32::from(d)),
        });
        sem.ref_entries
            .push((ref_kind, u32::from(d), base_span.start, base_span.end));
        tracing::info!(target: "mcc::lsp::audit",
            "[AUDIT-BaseRef] base={base} chain_span={span:?} → {} kind={:?} ref_kind={ref_kind:?} def_uri={} def_span={:?} decl_id={d:?}",
            hit.name, hit.def_kind, hit.uri, hit.span);
    }

    fn lapper_function_params(
        uri: &McURI,
        sem: &mut McSemSymbols,
        symbol_lapper: &mut DedupLapper,
    ) {
        let modules = &crate::db::cmie::tables::WORKSPACE.modules;
        for entry in modules.iter() {
            let m = entry.value();
            if entry.key().uri != uri.as_str() {
                continue;
            }
            let mod_ident = entry.key().ident.to_string();
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
                // in a module func like do_flash). Recorded into `func.insts` by
                // try_record_chain_ref; resolve against module insts because
                // func bodies reference module-level instances (e.g. uC).
                for (span, segments, scope) in func.insts.iter_chain_refs() {
                    if let Some(hit) = crate::refdef::chain::resolve_member_chain_from_segments(
                        &uri, segments, &m.insts, &m.params,
                    ) {
                        // ★ Class-chain hits are already covered by
                        // lapper_global_classes (canonical ClassRef at every
                        // class-name span) — skip to avoid registering the
                        // class def twice.
                        if hit.def_kind == SymbolKind::ClassDef {
                            continue;
                        }
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
                let func_scope = func.insts.scope.clone().unwrap_or_else(|| fscope.clone());
                // Split the func scope into (container, func) — a dotted scope
                // like "mod.do_flash" must register labels under
                // container="mod", func="do_flash" so the key matches the
                // param/label defs registered by lapper_func_define_role.
                // Passing the whole dotted string as a single container
                // produced a different (container_id) key, so func-body labels
                // were invisible to lookup_declare_id.
                let (label_container, label_func) = match func_scope.rfind('.') {
                    Some(dot) => (
                        func_scope[..dot].to_string(),
                        Some(func_scope[dot + 1..].to_string()),
                    ),
                    None => (mod_ident.clone(), None),
                };
                for (name, _label_kind, span) in func.insts.iter_labels_with_span() {
                    // ★ Declareb inference: hint names are instances, never labels.
                    if func.insts.declareb_def(name).is_some() {
                        continue;
                    }
                    let (d, _) = crate::refdef::register::register_def(
                        sem,
                        uri,
                        &label_container,
                        label_func.as_deref(),
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

    /// Register component-level defs (params, pins, pin ids, ifaces, spec keys)
    /// into the local symbol table.
    ///
    /// Kept separate from `lapper_component_defs` so it runs BEFORE
    /// `lapper_func_define_role`: funcall-arg refs (e.g.
    /// `CAP(...).Cap([AVDD09_CAP, GND])`) are resolved there, and the
    /// container-level P2 fallback must already be able to find the
    /// component's own pins. Previously pins were only registered inside
    /// `lapper_component_defs` (which runs after func-role resolution), so such
    /// args missed P2 and fell through to the P3 name-only scan — random
    /// cross-container hits (e.g. GND → another component's func param).
    fn lapper_component_defs_register(
        uri: &McURI,
        sem: &mut McSemSymbols,
        symbol_lapper: &mut DedupLapper,
    ) {
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
                // Rule 6: untyped params -> UnknownDef, typed -> ParamDef.
                // Square-vec members (e.g. VDD1 inside `[VDD1,GND1]::DC(3.3V)`)
                // register as LabelDef instead, matching the module path (F0.3).
                let def_kind = if comp.params.is_square_member(name) {
                    SymbolKind::LabelDef
                } else {
                    Self::param_def_kind(comp.params.find(name))
                };
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
                // AST span may exclude leading/trailing delimiters (parser
                // tokens). Extend the span to cover them so PinNameDef names
                // are complete. Read only the boundary bytes — a full-file
                // read per build is wasteful, and virtual URIs (LSP didOpen
                // buffers) have no on-disk file, so the extension is skipped
                // there (File::open fails and we fall back to the raw span).
                use std::io::{Read as _, Seek as _, SeekFrom};
                if let Ok(mut file) = std::fs::File::open(std::path::Path::new(uri.as_str())) {
                    // Trailing ) or } — e.g. "I2C(Master)" not "I2C(Master"
                    if file.seek(SeekFrom::Start(pin_span.end as u64)).is_ok() {
                        let mut buf = [0u8; 1];
                        if file.read_exact(&mut buf).is_ok() && (buf[0] == b')' || buf[0] == b'}') {
                            pin_span.end += 1;
                        }
                    }
                    // Leading [ or { — e.g. "[VDD, GND]" not "VDD, GND]"
                    if pin_span.start > 0
                        && file
                            .seek(SeekFrom::Start(pin_span.start as u64 - 1))
                            .is_ok()
                    {
                        let mut buf = [0u8; 1];
                        if file.read_exact(&mut buf).is_ok() && (buf[0] == b'[' || buf[0] == b'{') {
                            pin_span.start -= 1;
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
                // register via register_def (real scope + def_map).
                // The old add_declare_with_name(SourceLocation::from_span) used
                // the shared (0,0,0) namespace — def_map had no (AttrDef, id)
                // entry, so FuncParamRef's AttrDef candidate could never match,
                // and the (0,0,0) key made same-named spec keys across components
                // share one DeclareId.
                let (d, _) = crate::refdef::register::register_def(
                    &mut *sem,
                    &uri,
                    comp_ident,
                    None,
                    &key_name,
                    key_span.clone(),
                    SymbolKind::AttrDef,
                );
                symbol_lapper.insert(Interval {
                    start: key_span.start,
                    stop: key_span.end,
                    val: SymbolType::new(SymbolKind::AttrDef, u32::from(d)),
                });
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
            for (span, port_name, scope) in comp.params.iter_net_refs() {
                let sp = crate::refdef::register::scope_path_from_scope_str(&uri, scope);
                let decl_id =
                    crate::refdef::register::lookup_declare_id(&sem.local_table, port_name, &sp);
                if let Some(decl_id) = decl_id {
                    symbol_lapper.insert(Interval {
                        start: span.start,
                        stop: span.end,
                        // param refs target ParamDef/LabelDef, not
                        // InstDef — InstRef never matched in fill_refdef_layer2.
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
            let comp_ident_label = comp_ident.clone();
            for (name, _label_kind, span) in comp.insts.iter_labels_with_span() {
                // ★ Declareb inference: hint names are instances, never labels.
                if comp.insts.declareb_def(name).is_some() {
                    continue;
                }
                // register via register_def so the LabelDef lands in
                // the real (file_id, comp_id, 0) scope and in def_map. The old
                // add_declare_with_name(SourceLocation::from_span) used the shared
                // (0,0,0) namespace — same-name labels in different components
                // shared one DeclareId and never entered def_map, so LabelRef
                // never matched in Layer 2.
                let (d, _) = crate::refdef::register::register_def(
                    sem,
                    uri,
                    &comp_ident_label,
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
            // ★ Component funcs: register func body refs (mirrors
            // lapper_function_params for module funcs). Param defs for both
            // module and component funcs are registered by
            // lapper_func_define_role (extract_func_param_spans).
            for func in comp.funcs.iter() {
                // func.insts.scope may be a bare func name (e.g. "do_flash")
                // when the component is re-parsed in some library-loading
                // passes; prefer the full "<comp>.<func>" scope so that
                // lookup_declare_id's P1 (exact func-scope match) hits the
                // param defs registered by lapper_func_define_role instead of
                // falling back to a name-only match on an unrelated GND.
                let func_scope = match func.insts.scope.as_deref() {
                    Some(s) if s.contains('.') => s.to_string(),
                    _ => format!("{}.{}", comp_ident, func.name),
                };
                // Func param net refs (e.g. `[V3V3, GND]` on the func's power
                // param) → LabelRef to the param member defs.
                for (span, port_name, scope) in func.params.iter_net_refs() {
                    let scope = if scope.contains('.') {
                        scope.clone()
                    } else {
                        func_scope.clone()
                    };
                    let sp = crate::refdef::register::scope_path_from_scope_str(&uri, &scope);
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
                // Func body net refs — prefer the component's own pins, then
                // fall back to generic dispatch against component insts.
                for (span, port_name, scope) in func.insts.iter_net_refs() {
                    let scope = if scope.contains('.') {
                        scope.clone()
                    } else {
                        func_scope.clone()
                    };
                    let sp = crate::refdef::register::scope_path_from_scope_str(&uri, &scope);
                    let decl_id = crate::refdef::register::lookup_declare_id(
                        &sem.local_table,
                        port_name,
                        &sp,
                    );
                    let Some(decl_id) = decl_id else {
                        continue;
                    };
                    let ref_kind = if pin_names.contains(port_name) {
                        SymbolKind::PinNameRef
                    } else if pin_ids.contains(port_name) {
                        SymbolKind::PinIdRef
                    } else if pin_ifaces.contains(port_name) {
                        SymbolKind::PinIfaceRef
                    } else {
                        Self::resolve_net_ref_kind(port_name, &comp.insts)
                    };
                    symbol_lapper.insert(Interval {
                        start: span.start,
                        stop: span.end,
                        val: SymbolType::new(ref_kind, u32::from(decl_id)),
                    });
                    sem.ref_entries
                        .push((ref_kind, u32::from(decl_id), span.start, span.end));
                }
                // Chain references inside func bodies (e.g. `this.MIC.P`).
                for (span, segments, scope) in func.insts.iter_chain_refs() {
                    if let Some(hit) = crate::refdef::chain::resolve_member_chain_from_segments(
                        &uri,
                        segments,
                        &comp.insts,
                        &comp.params,
                    ) {
                        // ★ Class-chain hits are already covered by
                        // lapper_global_classes (canonical ClassRef at every
                        // class-name span) — skip to avoid registering the
                        // class def twice.
                        if hit.def_kind == SymbolKind::ClassDef {
                            continue;
                        }
                        let ref_kind = Self::chain_ref_kind(hit.def_kind);
                        // ★ Cross-file chain defs (e.g. `uC.I2C0` resolving into the
                        // MCU component file) must not hijack this file's func-scope
                        // scope_index entry. Only attach the local scope when the def
                        // lives in the current file.
                        let container: &str = if hit.uri == *uri { scope.as_str() } else { "" };
                        let (d, _) = crate::refdef::register::register_def(
                            sem,
                            &hit.uri,
                            container,
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
                    } else if let Some((d, kind)) =
                        Self::resolve_func_chain_own_pin(&uri, segments, comp_ident, comp, sem)
                    {
                        // ★ Fallback: the chain root is one of the component's
                        // own pins (e.g. `VIN.Vin` inside `func enable` — `VIN`
                        // is a pin, not a func-local instance, so the chain
                        // resolver misses it). Resolve against the pin declare.
                        symbol_lapper.insert(Interval {
                            start: span.start,
                            stop: span.end,
                            val: SymbolType::new(kind, u32::from(d)),
                        });
                        sem.ref_entries
                            .push((kind, u32::from(d), span.start, span.end));
                    }
                }
                // Func body labels → LabelDef. Split the dotted func_scope
                // ("COMP.FUNC") into container + func so the DeclareId key
                // matches the defs registered by lapper_func_define_role (same
                // rationale as the module-side func label registration).
                let (label_container, label_func) = match func_scope.rfind('.') {
                    Some(dot) => (
                        func_scope[..dot].to_string(),
                        Some(func_scope[dot + 1..].to_string()),
                    ),
                    None => (comp_ident.to_string(), None),
                };
                for (name, _label_kind, span) in func.insts.iter_labels_with_span() {
                    let (d, _) = crate::refdef::register::register_def(
                        sem,
                        &uri,
                        &label_container,
                        label_func.as_deref(),
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

    /// Register square-vec refs inside component func bodies that name the
    /// component's own pins (e.g. `[VCC, VSS]`). `collect_net_refs_in_node`
    /// skips such members (neither func params nor func-local instances), so
    /// they are extracted here from the AST — after `lapper_component_defs`
    /// has registered the component pin defs as PinNameDef.
    fn lapper_component_func_pin_refs(
        uri: &McURI,
        ast: &AstNode,
        sem: &mut McSemSymbols,
        symbol_lapper: &mut DedupLapper,
    ) {
        // Component pin names in this file.
        let mut comp_pins: std::collections::HashMap<String, std::collections::HashSet<String>> =
            std::collections::HashMap::new();
        {
            let uri_str = uri.as_str();
            for entry in workspace::WORKSPACE.components.iter() {
                let key_uri = entry.key().uri.as_uri();
                if Self::uris_same_file(key_uri.as_ref(), uri_str) {
                    let names = Self::extract_pin_name_spans(entry.value())
                        .into_iter()
                        .map(|(n, _)| n)
                        .collect();
                    comp_pins.insert(entry.key().ident.to_string(), names);
                }
            }
            for entry in global::mcc_components.iter() {
                let key_uri = entry.key().uri.as_uri();
                if Self::uris_same_file(key_uri.as_ref(), uri_str) {
                    let names = Self::extract_pin_name_spans(entry.value())
                        .into_iter()
                        .map(|(n, _)| n)
                        .collect();
                    comp_pins.insert(entry.key().ident.to_string(), names);
                }
            }
        }

        // Container positioning — mirrors lapper_func_define_role.
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

        for node in &all_nodes {
            if node.get_type() != MCAST_FUNCTION {
                continue;
            }
            let ids_node = node.get_sub_node().and_then(|n| n.get_sub_node());
            let Some(ref ids) = ids_node else { continue };
            let pos = ids.get_pos() as usize;
            let comp_name = pos_to_container
                .iter()
                .take_while(|(p, _)| *p <= pos)
                .last()
                .map(|(_, name)| name.clone());
            let Some(comp_name) = comp_name else { continue };
            let Some(pins) = comp_pins.get(&comp_name) else {
                continue;
            };
            let mut members = Vec::new();
            Self::collect_square_vec_members_in_subtree(node, &mut members);
            for (mname, mspan) in members {
                if !pins.contains(&mname) {
                    continue;
                }
                // ★ scope_index may hold stale entries for "<comp>" scopes
                // registered from another file id (cross-file/duplicate
                // loads), so resolve directly against this file's pin defs:
                // (file_id, container_id=comp, func_id=0, name).
                let file_id = crate::ast::ast_semantic::intern(&mut sem.file_table, uri.as_str());
                let comp_id =
                    crate::ast::ast_semantic::intern(&mut sem.container_table, &comp_name);
                let got = sem
                    .local_table
                    .name_to_declare_id
                    .get(&(file_id, comp_id, 0, mname.to_string()))
                    .map(|(id, loc)| (*id, loc.clone()));
                if let Some((d, _)) = got {
                    symbol_lapper.insert(Interval {
                        start: mspan.start,
                        stop: mspan.end,
                        val: SymbolType::new(SymbolKind::PinNameRef, u32::from(d)),
                    });
                    sem.ref_entries.push((
                        SymbolKind::PinNameRef,
                        u32::from(d),
                        mspan.start,
                        mspan.end,
                    ));
                }
            }
        }
    }

    /// Search cross-file global tables for an enum class by name.
    /// Returns `(def_uri, def_span)` from the defining file's table.
    /// Priority: P3 (current file) → P4 (other workspace files) → P5 (system libs).
    /// `ref_span` is the byte span of the referencing `base.member` text, used
    /// to place the §5.4.6 D2 ambiguity diagnostic when multiple use-reachable
    /// files define the same enum class.
    fn find_enum_class_cross_file(
        uri: &McURI,
        sem: &McSemSymbols,
        base_name: &str,
        ref_span: Option<(u32, u32)>,
    ) -> Option<(McURI, crate::ast::ast_semantic::Span)> {
        // P3: current file's own definition — exact key only. A name-only walk
        // of `enum_class_name_to_id` could hit a same-named class registered
        // from another file (§5.4.6 D1).
        if let Ok(gt) = sem.global_table.lock() {
            if let Some(class_id) = gt.lookup_enum_class(uri, &McIds::from(base_name)) {
                if let Some((_u, span)) = gt.enum_class_id_to_span.get(&class_id) {
                    return Some((uri.clone(), span.clone()));
                }
            }
        }

        // P4: other workspace files — read the enums table directly instead
        // of locking each file's symbols + global_table. Locking other files
        // while the caller already holds this file's symbols lock can deadlock
        // when two files create their lapper concurrently (A locks B while B
        // locks A). McEnumDef carries uri + span, so no per-file lock is needed.
        // §5.4: a workspace enum is visible only when its defining file is
        // reachable through this file's `use` chain — never by bare name.
        // Multiple reachable definitions of the same name are ambiguous and
        // must be reported (§5.4.6 D2), not silently resolved to the first.
        let mut reachable: Vec<(McURI, crate::ast::ast_semantic::Span)> = Vec::new();
        for entry in workspace::WORKSPACE.enums.iter() {
            if entry.key().uri == uri.as_str() {
                continue;
            }
            if entry.key().ident.to_string() == base_name
                && crate::db::resolve::use_chain_reaches(uri, entry.key().uri.as_uri().as_ref())
            {
                reachable.push((
                    entry.key().uri.to_string(),
                    (entry.value().span[0] as usize)..(entry.value().span[1] as usize),
                ));
            }
        }
        if reachable.len() > 1 {
            if let Some((start, end)) = ref_span {
                dlog_error_at(
                    crate::errcodes::USE_SYMBOL_CONFLICT,
                    start,
                    end.saturating_sub(start),
                    &format!(
                        "enum class '{base_name}' resolves through multiple use-reachable files; use 'as' alias to disambiguate"
                    ),
                );
            }
            // Best-effort: still resolve to the first reachable definition.
            return reachable.into_iter().next();
        }
        if let Some(hit) = reachable.into_iter().next() {
            return Some(hit);
        }

        // P5: system libraries. Name-unique by construction (§5.4.6 "P5
        // uniqueness guarantee"): same-file duplicates error at load
        // (DUP_ENUM), cross-file duplicates are a library-side build rule.
        for entry in crate::db::infra::global::mcc_enums.iter() {
            if entry.key().uri == uri.as_str() {
                continue;
            }
            if entry.key().ident.to_string() == base_name {
                return Some((
                    entry.key().uri.to_string(),
                    (entry.value().span[0] as usize)..(entry.value().span[1] as usize),
                ));
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

                let (class_id, value_idx, _cross_file_uri) = {
                    // Look up enum class_id: local table (exact key) first,
                    // then the §5.4.3 cross-file chain (P3 → P4 → P5) via
                    // find_enum_class_cross_file. Both branches capture the
                    // defining uri so the value span can always be registered
                    // below. A class that resolves nowhere visible is an error
                    // (§5.4.6 C2), never a default-id def. The local table
                    // guard is dropped before the cross-file search — the
                    // cross-file path re-locks the same Mutex, and re-locking
                    // std::sync::Mutex on the same thread would deadlock.
                    let local_id = match sem.global_table.lock() {
                        Ok(gt) => gt.lookup_enum_class(&uri, &McIds::from(&base_name)),
                        Err(_) => continue 'outer,
                    };
                    let (cls, xuri) = match local_id {
                        Some(cid) => (cid, Some(uri.clone())),
                        None => match (
                            Self::find_enum_class_cross_file(
                                uri,
                                sem,
                                &base_name,
                                Some((base_start, base_end)),
                            ),
                            sem.global_table.lock(),
                        ) {
                            (Some((def_uri, def_span)), Ok(mut gt)) => (
                                gt.add_enum_class(&def_uri, &McIds::from(&base_name), def_span),
                                Some(def_uri),
                            ),
                            _ => {
                                dlog_error(
                                    crate::errcodes::INST_CLASS_UNRESOLVED,
                                    &opd_node,
                                    &crate::errcodes::format_msg(
                                        crate::errcodes::INST_CLASS_UNRESOLVED,
                                        &[],
                                    ),
                                );
                                continue;
                            }
                        },
                    };

                    // Locate the value by exact key (def_uri + name), never by
                    // a name-only walk — the class's defining file is already
                    // known (`xuri`), and a bare-name scan could hit a same-named
                    // enum in an unrelated file (§5.4.5).
                    let find_value = |def_uri: &McURI| {
                        let space =
                            McSpaceName::new(&McIds::from(base_name.as_str()), def_uri.clone());
                        let enum_def = workspace::WORKSPACE
                            .enums
                            .get(&space)
                            .or_else(|| crate::db::infra::global::mcc_enums.get(&space));
                        if let Some(entry) = enum_def {
                            for (i, v) in entry.value().values.iter().enumerate() {
                                if v.name.to_string() == member_name {
                                    return Some((i as u32, v.span));
                                }
                            }
                        }
                        None
                    };
                    let mut idx = None;
                    let mut value_span: Option<[u32; 2]> = None;
                    if let Some(xuri) = &xuri {
                        if let Some((i, s)) = find_value(xuri) {
                            idx = Some(i);
                            value_span = Some(s);
                        }
                    }
                    if idx.is_none() {
                        // §5.4.6 C3: no fallback to the current file — the
                        // class's defining file (xuri) is the only place its
                        // values can live. A member missing there is a real
                        // reference error, not a reason to guess at a
                        // same-named class in the current file.
                        dlog_error(
                            crate::errcodes::SYMBOL_NOT_FOUND,
                            &opd_node,
                            &crate::errcodes::format_msg(crate::errcodes::SYMBOL_NOT_FOUND, &[]),
                        );
                        continue;
                    }

                    match idx {
                        Some(i) => {
                            tracing::info!(target: "mcc::lsp::audit",
                                "[AUDIT-EnumVal] base={base_name} member={member_name} cls={:?} xuri={:?} idx={} value_span={:?}",
                                cls, xuri, i, value_span);
                            // ★ Cross-file enum classes: register the value
                            // spans under the locally-assigned class_id so the
                            // RefDefMap 1e layer can map the packed EnumValRef
                            // id (class_id<<16|idx) to the value def in the
                            // defining file (e.g. `PKG.SOP8` → package.mc).
                            if let (Some(def_uri), Some(span)) = (&xuri, value_span) {
                                if let Ok(mut gt) = sem.global_table.lock() {
                                    let span: Span = (span[0] as usize)..(span[1] as usize);
                                    gt.add_enum_value(def_uri, cls, i, span);
                                }
                            }
                            (cls, i, xuri)
                        }
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

        // Collect all AST nodes via BFS (reverse order; sort below so the
        // container-stack pop condition holds for a monotonic scan).
        let mut all_nodes: Vec<AstNode> = {
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
        all_nodes.sort_by_key(|n| n.get_pos());

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
                Some((def_uri, value_span, value_idx)) => {
                    // Get the enum class to build value_id
                    let family_name = comp_ids.root_name().unwrap_or_default();
                    let class_id = {
                        let local_id = {
                            let gt = match sem.global_table.lock() {
                                Ok(gt) => gt,
                                Err(_) => continue,
                            };
                            gt.lookup_enum_class(uri, &McIds::from(&family_name))
                        };
                        // presence of the mapping — not a nonzero
                        // value — decides whether the class is registered;
                        // class_id=0 is a legitimate id.
                        match local_id {
                            Some(id) => Some(id),
                            None => {
                                // Cross-file search + local registration
                                match (
                                    Self::find_enum_class_cross_file(
                                        uri,
                                        sem,
                                        &family_name,
                                        Some((pos as u32, (pos + node.get_len() as usize) as u32)),
                                    ),
                                    sem.global_table.lock(),
                                ) {
                                    (Some((xuri, def_span)), Ok(mut gt)) => {
                                        Some(gt.add_enum_class(
                                            &xuri,
                                            &McIds::from(&family_name),
                                            def_span,
                                        ))
                                    }
                                    _ => None,
                                }
                            }
                        }
                    };
                    let Some(class_id) = class_id else {
                        continue;
                    };
                    // for a class defined in another file, register
                    // the value span under the local class_id so the packed
                    // EnumValRef resolves in the defining file (mirrors
                    // lapper_enum_refs). Without this the RefDefMap 1e layer
                    // has no entry and goto-def comes up empty.
                    if def_uri.as_str() != uri.as_str() {
                        if let Ok(mut gt) = sem.global_table.lock() {
                            gt.add_enum_value(&def_uri, class_id, value_idx, value_span);
                        }
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
        let mut all_nodes: Vec<AstNode> = {
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
        // The stack-based walk above visits nodes in reverse order. The
        // container/func pop condition (`node_start >= end`) only holds for a
        // position-monotonic scan, so sort by position first — otherwise a
        // node after a nested container (or a non-func node between two
        // siblings) is assigned to the wrong scope.
        all_nodes.sort_by_key(|n| n.get_pos());
        let mut container_names: Vec<String> = Vec::new();
        {
            let uri_str = uri.as_str();
            let modules = &workspace::WORKSPACE.modules;
            for entry in modules.iter() {
                let key_uri = entry.key().uri.as_uri();
                if Self::uris_same_file(key_uri.as_ref(), uri_str) {
                    container_names.push(entry.key().ident.to_string());
                }
            }
            let comps = &workspace::WORKSPACE.components;
            for entry in comps.iter() {
                let key_uri = entry.key().uri.as_uri();
                if Self::uris_same_file(key_uri.as_ref(), uri_str) {
                    container_names.push(entry.key().ident.to_string());
                }
            }
            for entry in global::mcc_modules.iter() {
                let key_uri = entry.key().uri.as_uri();
                if Self::uris_same_file(key_uri.as_ref(), uri_str) {
                    container_names.push(entry.key().ident.to_string());
                }
            }
            for entry in global::mcc_components.iter() {
                let key_uri = entry.key().uri.as_uri();
                if Self::uris_same_file(key_uri.as_ref(), uri_str) {
                    container_names.push(entry.key().ident.to_string());
                }
            }
            tracing::info!(target: "mcc::lsp",
                "create_lapper scope: uri={uri_str}, found {} containers: {:?}",
                container_names.len(), container_names);
        }
        let mut container_stack: Vec<(String, usize)> = Vec::new();
        let mut func_stack: Vec<(String, usize)> = Vec::new();
        let mut pos_to_container: Vec<(usize, String)> = Vec::new();
        let mut pos_to_func: Vec<(usize, String)> = Vec::new();
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
            while let Some((_, end)) = func_stack.last() {
                if node_start >= *end {
                    func_stack.pop();
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
            if ntype == MCAST_FUNCTION {
                if let Some(ids_node) = node.get_sub_node().and_then(|n| n.get_sub_node()) {
                    if let Some(ids) = McIds::new(&ids_node) {
                        func_stack.push((ids.to_string(), node_end));
                    }
                }
            }
            if let Some((name, _)) = container_stack.last() {
                pos_to_container.push((node_start, name.clone()));
            }
            if let Some((fname, _)) = func_stack.last() {
                pos_to_func.push((node_start, fname.clone()));
            }
        }
        pos_to_container.sort_by_key(|(pos, _)| *pos);
        pos_to_func.sort_by_key(|(pos, _)| *pos);
        let find_container = move |pos: usize| -> Option<String> {
            pos_to_container
                .iter()
                .take_while(|(p, _)| *p <= pos)
                .last()
                .map(|(_, name)| name.clone())
        };
        // Full scope for a position: "container.func" when inside a func body,
        // otherwise just "container". Enables the lookup priority
        // "func params/labels first, then parent container defs" (§3.2.2).
        let find_container_for_scope = find_container.clone();
        let find_scope = move |pos: usize| -> Option<String> {
            let container = find_container_for_scope(pos)?;
            let func = pos_to_func
                .iter()
                .take_while(|(p, _)| *p <= pos)
                .last()
                .map(|(_, f)| f.clone());
            Some(match func {
                Some(f) if !f.is_empty() => format!("{container}.{f}"),
                _ => container,
            })
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
                        // Only create FuncRef if the function is found in the
                        // enclosing scope. Don't fall back to
                        // add_declare_with_name (which produces a random ID
                        // that never matches RefDefMap, causing P6 self-locate
                        // with no navigation), and don't scan name_to_declare_id
                        // by name only — that HashMap iteration is
                        // non-deterministic and picks a random def when several
                        // containers declare the same func name. P1/P2 scope
                        // lookup resolves the enclosing container/func instead.
                        if let Some(resolved_id) = func_name.as_ref().and_then(|n| {
                            let scope = find_scope(node.get_pos() as usize).unwrap_or_default();
                            let sp =
                                crate::refdef::register::scope_path_from_scope_str(&uri, &scope);
                            crate::refdef::register::lookup_declare_id(&sem.local_table, n, &sp)
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
                            // ★ Chain fix: `mcu.i2c().do_flash(flash.SPI)` must
                            // resolve `do_flash` against the base instance's class
                            // (mod.sub — chain funcs return `this`, see the function
                            // call chain design), NOT against the intermediate
                            // method name `i2c` that extract_class_name returns.
                            // The base instance is preferred; the old last-name
                            // extraction stays as a fallback (e.g. `RES(...).Pullup`).
                            let class_name = Self::extract_chain_base_instance(&sub)
                                .and_then(|inst| Self::find_instance_class_name(&inst, uri))
                                .or_else(|| Self::extract_class_name(&sub));
                            if let (Some(class_name), Some(method_name)) = (
                                class_name,
                                func_name.as_ref().map(|s| s.as_str().to_string()),
                            ) {
                                if let Some((def_uri, def_span, ref_kind)) =
                                    crate::db::resolve::member::resolve_cmie_member_locked(
                                        &class_name,
                                        &method_name,
                                        uri,
                                        sem,
                                    )
                                {
                                    // ★ Cross-file member defs (e.g. `CAP(...).Cap(_)`
                                    // where the Cap method lives in cap.mc) must NOT be
                                    // registered under the current file's container name.
                                    // Doing so writes scope_index[container] with the def
                                    // file's id, which hijacks the P2 container fallback
                                    // for same-file defs (component pins / module ports).
                                    // Register under the member's own class scope instead.
                                    //
                                    // When the func is declared in the current file,
                                    // lapper_func_define_role already registered it under
                                    // the func scope ("{class}.{func}"). Reuse that
                                    // DeclareId so the same symbol is not registered twice
                                    // (e.g. `func power` under both "mod.power" and "mod").
                                    let func_scope = format!("{class_name}.{method_name}");
                                    let sp = crate::refdef::register::scope_path_from_scope_str(
                                        &def_uri,
                                        &func_scope,
                                    );
                                    let decl_id = match crate::refdef::register::lookup_declare_id(
                                        &sem.local_table,
                                        &method_name,
                                        &sp,
                                    ) {
                                        Some(existing) => existing,
                                        None => {
                                            crate::refdef::register::register_def(
                                                sem,
                                                &def_uri,
                                                &class_name,
                                                None,
                                                &method_name,
                                                def_span,
                                                SymbolKind::FuncDef,
                                            )
                                            .0
                                        }
                                    };
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
                                        crate::errcodes::MODULE_METHOD_NOT_FOUND,
                                        node,
                                        &crate::errcodes::format_msg(
                                            crate::errcodes::MODULE_METHOD_NOT_FOUND,
                                            &[
                                                &method_name as &dyn std::fmt::Display,
                                                &class_name as &dyn std::fmt::Display,
                                            ],
                                        ),
                                    );
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
                        // ★ Lookup priority inside func bodies: func-scoped defs
                        // (params/labels) first, then the parent container's defs
                        // (component pins / module ports etc). Pass the full
                        // "container.func" scope so lookup_declare_id's P1 (func
                        // scope) and P2 (container fallback) both apply.
                        let scope = find_scope(span.0).unwrap_or_else(|| enclosing.clone());
                        let refs = crate::refdef::collect::collect_funccall_arg_refs(
                            node,
                            &sem.local_table,
                            &uri,
                            &scope,
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

    /// Walk an `MCAST_INSTANCE` receiver chain down to the base instance name.
    ///
    /// `mcu.i2c().do_flash(...)` — the outer receiver's MCAST_INSTANCE
    /// wraps the inner fcall `mcu.i2c()`, whose own MCAST_INSTANCE wraps
    /// the base `mcu`. Chain funcs bind to the base object (funcs return
    /// `this`), so member resolution must use the base instance's class, not
    /// the intermediate method name. Returns the base instance name, or None
    /// when the receiver is a plain class call (e.g. `RES(...).Pullup(...)`).
    fn extract_chain_base_instance(sub: &Option<AstNode>) -> Option<String> {
        let mut current = sub.clone()?;
        loop {
            if current.get_type() != MCAST_INSTANCE {
                return None;
            }
            let Some(inner) = current.get_sub_node() else {
                return None;
            };
            if inner.get_type() == MCAST_OPD_FCALL {
                let receiver = inner
                    .get_sub_node()
                    .and_then(|s2| s2.iter().find(|c| c.get_type() == MCAST_INSTANCE));
                let Some(receiver) = receiver else {
                    return None;
                };
                current = receiver;
                continue;
            }
            // Base receiver — MCAST_OPD (or similar) wrapping the identifiers.
            let ids = inner.get_sub_node()?;
            if inner.get_type() == MCAST_DECLARE {
                // Same-line instance declare + method call
                // (`DCDC.LP3220AB5F lp322dcdc.enable()`): the FCALL's INSTANCE
                // receiver wraps the DECLARE node. The instance name is
                // either a direct MCAST_IDS child or wrapped in an
                // MCAST_INSTANCE child that follows the MCAST_CLASS child.
                let mut name_ids = None;
                let mut cur = Some(ids);
                while let Some(n) = cur {
                    if n.get_type() == MCAST_CLASS {
                        cur = n.get_next();
                        continue;
                    }
                    if n.get_type() == MCAST_INSTANCE {
                        name_ids = n.get_sub_node();
                        break;
                    }
                    if n.get_type() == MCAST_IDS {
                        name_ids = Some(n);
                        break;
                    }
                    cur = n.get_next();
                }
                return name_ids.and_then(|n| McIds::new(&n)).map(|i| i.to_string());
            }
            return McIds::new(&ids).map(|i| i.to_string());
        }
    }

    /// Find the class name of an instance declared in the current file's
    /// modules (or library modules). `mod.sub mcu(...)` → "mod.sub".
    fn find_instance_class_name(inst_name: &str, uri: &McURI) -> Option<String> {
        let uri_str = uri.as_str();
        for table in [&workspace::WORKSPACE.modules, &global::mcc_modules] {
            for entry in table.iter() {
                let key_uri = entry.key().uri.as_uri();
                if !Self::uris_same_file(key_uri.as_ref(), uri_str) {
                    continue;
                }
                let m = entry.value();
                if let Some(inst) = m.insts.get(inst_name) {
                    match inst {
                        crate::McInstance::Module(m2) => {
                            return Some(m2.base.name.to_string());
                        }
                        crate::McInstance::Component(c2) => {
                            return Some(c2.base.name.to_string());
                        }
                        _ => {}
                    }
                }
            }
        }
        None
    }

    /// Look up the human-readable message for a parser diagnostic code.
    /// Codes follow the unified numbering (Pass1b parser cluster 2080-2116);
    /// keep in sync with `errcodes.rs` descriptions.
    fn dlog_parser_message(code: u32) -> &'static str {
        match code {
            // Errors (2081–2110)
            2081 => "Invalid top-level declaration",
            2082 => "Invalid clause in a body",
            2083 => "Invalid pin declaration",
            2084 => "Pin ID must be a constant integer, not an expression",
            2085 => "Pin name must be a constant identifier, not an expression",
            2086 => "Net endpoint must be a port/label, not a literal",
            2087 => "Invalid net/connection expression",
            2088 => "Invalid if/else condition block",
            2089 => "Invalid role block",
            2090 => "Invalid function definition",
            2091 => "Invalid pins declaration",
            2092 => "Invalid import statement",
            2093 => "Invalid condition body",
            2094 => "Invalid instance declaration (:: syntax)",
            2095 => "Invalid body",
            2096 => "Invalid condition expression",
            2097 => "Invalid parameter declaration",
            2098 => "Invalid import path",
            2099 => "Invalid expression list",
            2100 => "Invalid operand list",
            2101 => "Invalid parameter list",
            2102 => "Invalid parameter declaration list",
            2103 => "Invalid attribute value list",
            2104 => "Invalid attribute line list",
            2105 => "Invalid pin name list",
            2106 => "Invalid instance list",
            2107 => "Invalid else-if chain",
            2108 => "Invalid identifier list",
            2109 => "Invalid path in import",
            2110 => "Invalid expression",
            // Warnings (2111–2116)
            2111 => "Single '|' used as a binary operator outside a pin context",
            2112 => "'±' used as a binary operator outside a tolerance context",
            2113 => "Transpose (') on a literal has no effect",
            2114 => "Caret (^) on a literal has no effect",
            2115 => "Empty body — no clauses defined",
            2116 => "Empty pins declaration",
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
                iv.val.kind == SymbolKind::InstDef as u8
                    && iv.start == span.start
                    && iv.stop == span.end
            });
            assert!(
                found,
                "lapper must contain an InstDef interval for declareb instance '{name}' at {span:?}"
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

    /// Regression: a chain whose receiver is a method call returning `this`
    /// (`uC.i2c(0x36).I2C0` → the uC instance's I2C0 pin) must be recorded as
    /// a chain ref and resolve to the class-definition pin, not the local
    /// module port.
    ///
    /// The chain root is MCAST_OPD_DOT (sub=FCALL, next=member), which
    /// `try_record_chain_ref` previously did not handle — only MCAST_OPD
    /// roots were recorded, so `uC.i2c(0x36).I2C0` fell through to plain
    /// scoped net refs and `.I2C0` resolved to the module port I2C0.
    #[test]
    fn fcall_chain_member_resolves_to_instance_pin() {
        let _guard = PARSE_LOCK.lock().expect("test parse lock");
        crate::mcc_init_no_lib();
        crate::mcc_set_system_root(std::path::Path::new(""));
        crate::mcc_clear_workspace();

        let uri: crate::McURI = "/mcc/fcallchain.mc".to_string();
        let source = r#"
interface I2C.SMBus(role)
{
    pins = [
        1 = SDA
        2 = SCL
    ]
    role Target { name = "I2C.SMBus Target" }
    role Controller { name = "I2C.SMBus Controller" }
}

component MCU.X
{
    pins = [
        io [1,2] = I2C0::I2C.SMBus(Target)
        io [3,4] = XTAL
    ]
}

module main
{
    io I2C0
    MCU.X uC
    uC.i2c(0x36).I2C0 -> I2C0
}
"#;
        crate::mcc_load_from_string(&uri, source);

        let mcode = workspace::WORKSPACE.mcodes.get(&uri).expect("file loaded");
        let sem = mcode.symbols.lock().expect("symbols lock");

        // Expected byte spans (source is pure ASCII here).
        let chain_ref = source.find("uC.i2c(0x36).I2C0").unwrap()
            ..source.find("uC.i2c(0x36).I2C0").unwrap() + "uC.i2c(0x36).I2C0".len();
        let pin_def = source.find("I2C0::I2C.SMBus").unwrap()
            ..source.find("I2C0::I2C.SMBus").unwrap() + "I2C0".len();
        let module_port_def =
            source.find("io I2C0").unwrap()..source.find("io I2C0").unwrap() + "I2C0".len();

        // The chain ref span must cover the whole `uC.i2c(0x36).I2C0` text.
        let chain_iv = sem
            .symbol_lapper
            .iter()
            .find(|iv| iv.start == chain_ref.start && iv.stop == chain_ref.end)
            .unwrap_or_else(|| {
                let existing: Vec<_> = sem.symbol_lapper.iter().collect();
                std::panic!(
                    "lapper must contain a chain interval at {chain_ref:?}; got {existing:?}"
                );
            });
        // `I2C0` is bound to the `I2C.SMBus` interface, so the chain member
        // must surface as a PinIfaceRef (and its def as PinIfaceDef).
        assert_eq!(
            chain_iv.val.kind,
            SymbolKind::PinIfaceRef as u8,
            "chain interval must be a PinIfaceRef"
        );

        // The member text must map (via ref_def_map) to the I2C0 pin def in
        // the MCU.X component, not to the module's own I2C0 port.
        let rdm = sem.ref_def_map.as_ref().expect("ref_def_map built");
        let entry = rdm
            .entries
            .get(&(SymbolKind::PinIfaceRef, chain_iv.val.id))
            .expect("chain ref must have a ref_def_map entry");
        assert_eq!(
            entry.def_loc.byte_start as usize, pin_def.start,
            "uC.i2c(0x36).I2C0 member must resolve to the MCU.X I2C0 pin def"
        );
        assert_ne!(
            entry.def_loc.byte_start as usize, module_port_def.start,
            "must NOT resolve to the module's own I2C0 port"
        );
    }

    /// Regression: position-aware hover must resolve same-name defs precisely
    /// (`enum CAP` + `component CAP` in one file). A position at a component
    /// class reference resolves to the component head via the lapper +
    /// RefDefMap exact path; a position with no registered interval returns
    /// None — never a name-based guess (which would misattribute the def).
    #[test]
    fn position_hover_resolves_same_name_enum_and_component() {
        let _guard = PARSE_LOCK.lock().expect("test parse lock");
        crate::mcc_init_no_lib();
        crate::mcc_set_system_root(std::path::Path::new(""));
        crate::mcc_clear_workspace();

        let source = r#"
enum CAP { X7R, MLCC, C0G }

component CAP (diel = CAP.X7R)
{
    pins = [
        1 = 1
        2 = 2
    ]
}

module main
{
    CAP C1
    C1.1 -> V5V
}
"#;
        let uri: crate::McURI = "/mcc/hover-cap.mc".to_string();
        crate::mcc_load_from_string(&uri, source);
        crate::mcc_build(&McIds::from("main"), &uri).expect("build failed");

        let comp_head = source.find("component CAP").unwrap() + "component ".len();

        // Component construction site: `CAP` in `CAP C1` is a class reference
        // to the component — must resolve to the component head.
        let comp_ref = source.find("CAP C1").unwrap();
        let h =
            crate::lsp::hover::hover("CAP", &uri, Some(comp_ref)).expect("hover at component ref");
        assert_eq!(
            h["kind"], "ClassDef",
            "constructor must hit component head: {h}"
        );
        assert_eq!(
            h["byte_start"].as_u64(),
            Some(comp_head as u64),
            "constructor span must point at component head: {h}"
        );

        // Enum reference site: `CAP` in `diel = CAP.X7R` has no dedicated
        // lapper interval here (the parameter span covers the whole phrase),
        // so the position-aware path must return None — never a wrong
        // name-based guess at the same-named component.
        let enum_ref = source.find("CAP.X7R").unwrap();
        let h = crate::lsp::hover::hover("CAP", &uri, Some(enum_ref));
        assert!(h.is_none(), "unresolved position must not fall back: {h:?}");

        // Name-based path (no position) is inherently ambiguous for same-name
        // defs — its result depends on name_index registration order. It
        // exists only for legacy callers; the position-aware path is the
        // authoritative one.
        let h = crate::lsp::hover::hover("CAP", &uri, None).expect("hover without position");
        assert!(
            h["kind"] == "component" || h["kind"] == "enum",
            "name-based must return a same-name kind: {h}"
        );
    }

    /// Regression: strict position-aware goto-def shares the hover path
    /// (`refdef::query::resolve_at`). A component class reference resolves to
    /// the component head; a position with no registered interval returns None
    /// — never a name-based guess at the same-named enum/component.
    #[test]
    fn position_goto_def_resolves_same_name_enum_and_component() {
        let _guard = PARSE_LOCK.lock().expect("test parse lock");
        crate::mcc_init_no_lib();
        crate::mcc_set_system_root(std::path::Path::new(""));
        crate::mcc_clear_workspace();

        let source = r#"
enum CAP { X7R, MLCC, C0G }

component CAP (diel = CAP.X7R)
{
    pins = [
        1 = 1
        2 = 2
    ]
}

module main
{
    CAP C1
    C1.1 -> V5V
}
"#;
        let uri: crate::McURI = "/mcc/gotodef-cap.mc".to_string();
        crate::mcc_load_from_string(&uri, source);
        crate::mcc_build(&McIds::from("main"), &uri).expect("build failed");

        let comp_head = source.find("component CAP").unwrap() + "component ".len();

        // `CAP` in `CAP C1` is a class reference → the component head.
        let comp_ref = source.find("CAP C1").unwrap();
        let d =
            crate::lsp::gotodef::resolve_at_pos(&uri, comp_ref).expect("goto-def at component ref");
        assert_eq!(
            d["kind"], "ClassDef",
            "goto-def must hit component head: {d}"
        );
        assert_eq!(
            d["byte_start"].as_u64(),
            Some(comp_head as u64),
            "goto-def span must point at component head: {d}"
        );

        // Unregistered position (inside the param phrase) → strict None.
        let enum_ref = source.find("CAP.X7R").unwrap();
        let d = crate::lsp::gotodef::resolve_at_pos(&uri, enum_ref);
        assert!(
            d.is_none(),
            "unresolved position must not fall back for goto-def: {d:?}"
        );
    }

    /// Regression: completion must keep same-name candidates of different
    /// kinds (`enum CAP` + `component CAP` in the project) instead of letting
    /// the project scan swallow one of them by name-only dedup.
    #[test]
    fn completion_keeps_same_name_enum_and_component_candidates() {
        let _guard = PARSE_LOCK.lock().expect("test parse lock");
        crate::mcc_init_no_lib();
        crate::mcc_set_system_root(std::path::Path::new(""));
        crate::mcc_clear_workspace();

        let cap_src = r#"
enum CAP { X7R, MLCC, C0G }

component CAP (diel = CAP.X7R)
{
    pins = [
        1 = 1
        2 = 2
    ]
}

module cap_mod
{
}
"#;
        let cap_uri: crate::McURI = "/mcc/comp-cap.mc".to_string();
        crate::mcc_load_from_string(&cap_uri, cap_src);

        let main_src = r#"
use cap_mod

module main
{
    CAP C1
    C1.1 -> V5V
}
"#;
        let main_uri: crate::McURI = "/mcc/comp-main.mc".to_string();
        crate::mcc_load_from_string(&main_uri, main_src);
        crate::mcc_build(&McIds::from("main"), &main_uri).expect("build failed");

        let items = crate::lsp::completion::complete(&main_uri, Some("CAP"), None);
        let mut has_component = false;
        let mut has_enum = false;
        for item in &items {
            let kind = item["kind"].as_str().unwrap_or_default();
            if kind == "component" {
                has_component = true;
            }
            if kind == "enum" {
                has_enum = true;
            }
        }
        assert!(
            has_component && has_enum,
            "completion must offer both component CAP and enum CAP, got: {items:?}"
        );
    }

    /// Regression: RefDefMap entries must carry an AST-driven `def_name` for
    /// same-file and cross-file defs (class, enum, enum value). The name is
    /// captured at registration from the AST node and reverse-looked-up in the
    /// current file's symbol table when the id space differs across files —
    /// never produced by text-slicing the def line, and never by probing the
    /// def file's own table with a caller-side id (which can alias a different
    /// same-id class, e.g. `CAP` id=9 here vs `CAP.SAFETY` id=9 in cap.mc).
    ///
    /// Uses the real `mcode` library so the cross-file refs (`RES` from
    /// res.mc, `PKG.SOT_23_5` from package.mc) resolve through the library
    /// loading path — virtual cross-file `use` of in-memory files cannot be
    /// loaded from disk.
    #[test]
    fn ref_def_map_entries_carry_ast_def_names() {
        let _guard = PARSE_LOCK.lock().expect("test parse lock");
        crate::mcc_init_no_lib();
        let mcode_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mcode");
        crate::mcc_set_system_root(mcode_dir.as_path());
        crate::mcc_clear_workspace();
        crate::mcb_load_lib("mcode", mcode_dir.as_path());

        let main_src = r#"
component TEST_PKG
{
    package = PKG.SOT_23_5
}

module TEST_IFS([VDD, GND]::DC(3.3V))
{
}

module main
{
    RES R1
    R1.1 -> V5V

    // ★ func-header interface binding — must register the `DC` class ref
    // just like a module port (regression guard for func-param refs).
    func pwr([A, B]::DC(3.3V))
    {
    }
}
"#;
        let main_uri: crate::McURI = "/mcc/defs-main.mc".to_string();
        crate::mcc_load_from_string(&main_uri, main_src);
        crate::mcc_build(&McIds::from("main"), &main_uri).expect("build failed");

        let mcode = workspace::WORKSPACE
            .mcodes
            .get(&main_uri)
            .expect("file loaded");
        let sem = mcode.symbols.lock().expect("symbols lock");
        let rdm = sem.ref_def_map.as_ref().expect("ref_def_map is built");

        let mut class_names: Vec<&str> = Vec::new();
        let mut enum_val_names: Vec<&str> = Vec::new();
        for ((kind, _id), entry) in rdm.entries.iter() {
            match kind {
                SymbolKind::ClassRef => class_names.push(entry.def_name.as_str()),
                SymbolKind::EnumValRef => enum_val_names.push(entry.def_name.as_str()),
                _ => {}
            }
        }
        assert!(
            class_names.contains(&"RES"),
            "cross-file ClassRef def_name must be 'RES', got: {class_names:?}"
        );
        assert!(
            enum_val_names.contains(&"SOT_23_5"),
            "cross-file EnumValRef def_name must be 'SOT_23_5', got: {enum_val_names:?}"
        );

        // ★ The ref's CMIE kind must reflect the real def kind, so hover can
        // label a class ref to an `interface` as `→ interface` (not `→ class`):
        // RES is a component (0), DC is an interface (2).
        let mut res_kinds: Vec<u8> = Vec::new();
        let mut dc_kinds: Vec<u8> = Vec::new();
        for ((kind, _id), entry) in rdm.entries.iter() {
            if *kind == SymbolKind::ClassRef {
                match entry.def_name.as_str() {
                    "RES" => res_kinds.push(entry.cmie_kind),
                    "DC" => dc_kinds.push(entry.cmie_kind),
                    _ => {}
                }
            }
        }
        assert!(
            res_kinds.contains(&(crate::refdef::types::CmieKind::Component as u8)),
            "ClassRef RES must carry Component cmie_kind, got: {res_kinds:?}"
        );
        assert!(
            dc_kinds.contains(&(crate::refdef::types::CmieKind::Interface as u8)),
            "ClassRef DC must carry Interface cmie_kind, got: {dc_kinds:?}"
        );
        // The RefDefMap entry is deduped by (kind, decl_id): every DC class ref
        // resolves to the same interface and shares one map entry. Verify the
        // func-header binding reached the lapper/ref_entries by counting the
        // ClassRef spans for the DC class id — must cover BOTH the module port
        // and the func-header declare.
        let dc_class_id = rdm
            .entries
            .iter()
            .find(|((k, _), e)| *k == SymbolKind::ClassRef && e.def_name.as_str() == "DC")
            .map(|((_, id), _)| *id);
        let dc_span_count = sem
            .ref_entries
            .iter()
            .filter(|(k, id, _s, _e)| *k == SymbolKind::ClassRef && Some(*id) == dc_class_id)
            .count();
        assert!(
            dc_span_count >= 2,
            "DC ClassRef spans must include both the module port AND the \
             func-header binding (got {dc_span_count})"
        );
    }
}
