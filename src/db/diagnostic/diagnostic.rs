// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::ast::node::AstNode;
use crate::db::cmie::tables as workspace;
use crate::McURI;
use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Formatter};
use std::sync::{Mutex, OnceLock};
pub type Position = u32;

// ============================================================================
// Warning-code suppression (`diag.ignore_warnings` config + `-i/--ignore`
// CLI flag). Warning severity only — errors are never suppressed by this path.
// ============================================================================

static IGNORED_WARNINGS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn ignored_warnings() -> &'static Mutex<HashSet<String>> {
    IGNORED_WARNINGS.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Set the set of warning diagnostic codes to suppress (replaces any previous
/// list). Codes may be written as `E3137` or `3137`. Callers merge config +
/// CLI values before calling.
pub fn set_ignored_warnings(codes: impl IntoIterator<Item = String>) {
    let mut guard = ignored_warnings().lock().unwrap();
    guard.clear();
    guard.extend(codes);
}

/// Is `d` suppressed by the ignore set? Only `Warning` level diagnostics are
/// considered — an Error is never filtered by this mechanism.
pub fn is_diagnostic_ignored(d: &Diagnostic) -> bool {
    if d.level != DiagnosticLevel::Warning {
        return false;
    }
    let guard = ignored_warnings().lock().unwrap();
    guard.contains(&format!("E{}", d.code)) || guard.contains(&d.code.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticLevel {
    Error = 1,
    Warning = 2,
    Info = 3,
    Hint = 4,
}

#[derive(Debug, Clone)]
pub struct Location {
    pub uri: McURI,
    pub pos: Position,
    pub len: u32,
    pub row: u32,     // 1-based start line number
    pub col: u32,     // 1-based start column number
    pub end_row: u32, // 1-based end line number (computed from pos + len)
    pub end_col: u32, // 1-based end column number (computed from pos + len)
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub code: u32,
    pub level: DiagnosticLevel,
    pub loc: Location,
    pub msg: String,
    pub other: Vec<RelatedInformation>,
}

#[derive(Debug, Clone)]
pub struct RelatedInformation {
    pub location: Location,
    pub message_template: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DiagnosticManager {
    diagnostics: Vec<Diagnostic>,
    file_to_diagnostics: HashMap<McURI, Vec<usize>>, // File URI -> Diagnostics indices
}

pub mod message_templates {
    pub(super) fn format(msg: &str, args: &[&dyn std::fmt::Display]) -> String {
        let mut message = msg.to_string();
        for (i, arg) in args.iter().enumerate() {
            let placeholder = format!("{{{i}}}");
            message = message.replace(&placeholder, &arg.to_string());
        }
        message
    }
}

impl DiagnosticLevel {
    pub fn as_lsp_severity(&self) -> i32 {
        match self {
            DiagnosticLevel::Error => 1,
            DiagnosticLevel::Warning => 2,
            DiagnosticLevel::Info => 3,
            DiagnosticLevel::Hint => 4,
        }
    }
}

impl Location {
    pub fn new(file: McURI, pos: Position, len: u32) -> Self {
        // Try to get line and column from the file's line index
        let (line, column) = match workspace::WORKSPACE.mcodes.get(&file) {
            Some(mcfile) => mcfile.pos_to_line_col(pos),
            None => {
                // Fallback: the file may have been temporarily removed from
                // `mcodes` during parsing (see pass1.rs `mcb_parse_all_modules`).
                // Check the thread-local line index stack.
                crate::db::infra::context::lookup_line_col(&file, pos).unwrap_or((1, 1))
            }
        };

        // Compute end position (pos + len) for proper span highlighting
        let (end_line, end_column) = if len > 0 {
            workspace::WORKSPACE
                .mcodes
                .get(&file)
                .map(|mcfile| mcfile.pos_to_line_col(pos + len))
                .unwrap_or((line, column + len))
        } else {
            (line, column)
        };

        Self {
            uri: file,
            pos,
            len,
            row: line,
            col: column,
            end_row: end_line,
            end_col: end_column,
        }
    }
}

impl Display for Location {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}:{} (pos={})",
            self.uri, self.row, self.col, self.pos
        )
    }
}

impl Diagnostic {
    pub fn new(code: u32, level: DiagnosticLevel, location: Location, message: String) -> Self {
        Self {
            code,
            level,
            loc: location,
            msg: message,
            other: Vec::new(),
        }
    }

    pub fn with_related(mut self, related: RelatedInformation) -> Self {
        self.other.push(related);
        self
    }
}

impl Display for Diagnostic {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Diagnostic {{ code: {}, level: {:?}, location: {}, message: {}",
            self.code, self.level, self.loc, self.msg
        )
    }
}

impl RelatedInformation {
    pub fn new(location: Location, message_template: String, args: &[&str]) -> Self {
        Self {
            location,
            message_template,
            args: args.iter().map(|&s| s.to_string()).collect(),
        }
    }

    pub fn get_formatted_message(&self) -> String {
        let mut message = self.message_template.clone();
        for (i, arg) in self.args.iter().enumerate() {
            let placeholder = format!("{{{i}}}");
            message = message.replace(&placeholder, arg);
        }
        message
    }
}

impl DiagnosticManager {
    pub fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
            file_to_diagnostics: HashMap::new(),
        }
    }

    pub fn add_diagnostic(&mut self, diagnostic: Diagnostic) {
        let index = self.diagnostics.len();
        let uri = diagnostic.loc.uri.clone();
        self.diagnostics.push(diagnostic);

        self.file_to_diagnostics.entry(uri).or_default().push(index);
    }

    pub fn get_diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn get_diagnostics_for_file(&self, file: &McURI) -> Vec<&Diagnostic> {
        self.file_to_diagnostics
            .get(file)
            .map(|indices| {
                indices
                    .iter()
                    .map(|&index| &self.diagnostics[index])
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Has a diagnostic with `code` already been logged at exactly this
    /// `(uri, pos)`? Keeps a single source-level fact idempotent when the same
    /// fact is re-derived by repeated runs — e.g. `mcc_build` and
    /// `mcc_build_flat` both execute pass2 instantiation (the latter delegates
    /// to the former, pass2.rs), so a phantom-only connection's GAP2 E4057
    /// would otherwise fire once per instantiation run.
    pub fn has_code_at(&self, code: u32, uri: &McURI, pos: u32) -> bool {
        self.file_to_diagnostics
            .get(uri)
            .map(|indices| {
                indices.iter().any(|&i| {
                    self.diagnostics[i].code == code && self.diagnostics[i].loc.pos == pos
                })
            })
            .unwrap_or(false)
    }

    pub fn clear(&mut self) {
        self.diagnostics.clear();
        self.file_to_diagnostics.clear();
    }

    pub fn clear_file(&mut self, file: &McURI) {
        let Some(remove_indices) = self.file_to_diagnostics.remove(file) else {
            return;
        };

        let remove_set: std::collections::HashSet<usize> = remove_indices.into_iter().collect();

        if remove_set.is_empty() {
            return;
        }

        let mut new_diagnostics = Vec::with_capacity(self.diagnostics.len() - remove_set.len());
        let mut old_to_new = vec![usize::MAX; self.diagnostics.len()];

        for (old_idx, diag) in self.diagnostics.iter().enumerate() {
            if remove_set.contains(&old_idx) {
                continue;
            }
            let new_idx = new_diagnostics.len();
            old_to_new[old_idx] = new_idx;
            new_diagnostics.push(diag.clone());
        }

        self.diagnostics = new_diagnostics;

        self.file_to_diagnostics.retain(|_, indices| {
            indices.retain(|old_idx| {
                *old_idx < old_to_new.len() && old_to_new[*old_idx] != usize::MAX
            });
            for idx in indices.iter_mut() {
                *idx = old_to_new[*idx];
            }
            !indices.is_empty()
        });
    }
}

/// Report diagnostic information to the global diagnostic manager
///
/// ## Parameters
/// - `$level`: Diagnostic level (Error, Warning, Info, Hint)
/// - `$pos`: Error position
/// - `$len`: Error length
/// - `$msg`: Message template string
/// - `$args`: Template parameter array
pub fn diagnostic_log(
    code: u32,
    level: DiagnosticLevel,
    pos: Position,
    len: u32,
    msg: &str,
    args: &[&dyn std::fmt::Display],
) {
    let new_diagnostic = Diagnostic::new(
        code,
        level,
        Location::new(crate::current_uri::get().clone(), pos, len),
        message_templates::format(msg, args),
    );

    workspace::WORKSPACE
        .diagnostics
        .lock()
        .unwrap()
        .add_diagnostic(new_diagnostic);
}

/// Report a diagnostic at an explicit file URI + byte offset, bypassing the
/// thread-local `current_uri`. Used when the diagnostic's real source file
/// differs from the file currently being processed (e.g. a component method
/// body expanded from a library file) — otherwise a pos 0 fallback renders
/// as `file:1:1` and cross-file offsets would be interpreted in the wrong
/// file.
pub fn diagnostic_log_at(
    code: u32,
    level: DiagnosticLevel,
    uri: McURI,
    pos: u32,
    len: u32,
    msg: &str,
    args: &[&dyn std::fmt::Display],
) {
    let new_diagnostic = Diagnostic::new(
        code,
        level,
        Location::new(uri, pos, len),
        message_templates::format(msg, args),
    );

    workspace::WORKSPACE
        .diagnostics
        .lock()
        .unwrap()
        .add_diagnostic(new_diagnostic);
}

/// Has a diagnostic with `code` already been logged at exactly this
/// `(uri, pos)`? See [`DiagnosticManager::has_code_at`] — used before logging
/// a fact that repeated instantiation runs would otherwise duplicate.
pub fn has_code_at(code: u32, uri: &McURI, pos: u32) -> bool {
    workspace::WORKSPACE
        .diagnostics
        .lock()
        .unwrap()
        .has_code_at(code, uri, pos)
}

pub fn dlog_trace(code: u32, msg: &str) {
    diagnostic_log(code, DiagnosticLevel::Info, 0, 0, msg, &[]);
}
pub fn dlog_error(code: u32, node: &AstNode, msg: &str) {
    let full_msg = msg.to_string();
    let uri = crate::current_uri::get();
    tracing::debug!(
        target: "mcc::diagnostic",
        code = code,
        node_type = node.get_type(),
        node_pos = node.get_pos(),
        node_len = node.get_len(),
        file = uri.as_str(),
        "{full_msg}"
    );
    diagnostic_log(
        code,
        DiagnosticLevel::Error,
        node.get_pos(),
        node.get_len(),
        &full_msg,
        &[],
    );
}
pub fn dlog_warning(code: u32, node: &AstNode, msg: &str) {
    let full_msg = msg.to_string();
    // Log sub-node chain via tracing for debugging (gated by log level)
    let uri = crate::current_uri::get();
    tracing::debug!(
        target: "mcc::diagnostic",
        code = code,
        node_type = node.get_type(),
        node_pos = node.get_pos(),
        node_len = node.get_len(),
        file = uri.as_str(),
        "{full_msg}"
    );
    let mut cur = node.get_sub_node();
    let mut depth = 0;
    while let Some(n) = cur {
        tracing::trace!(
            target: "mcc::diagnostic",
            depth = depth,
            node_type = n.get_type(),
            node_pos = n.get_pos(),
            node_len = n.get_len(),
            "sub-node"
        );
        cur = n.get_next();
        depth += 1;
        if depth > 10 {
            break;
        }
    }
    diagnostic_log(
        code,
        DiagnosticLevel::Warning,
        node.get_pos(),
        node.get_len(),
        &full_msg,
        &[],
    );
}
/// Emit a warning diagnostic using raw position/length (no AstNode).
/// Used when the AST node is unavailable (e.g., deferred validation in parse_nsp §11).
pub fn dlog_warning_at(code: u32, pos: Position, len: u32, msg: &str) {
    let uri = crate::current_uri::get();
    tracing::debug!(
        target: "mcc::diagnostic",
        code = code,
        pos = pos,
        len = len,
        file = uri.as_str(),
        "{msg}"
    );
    diagnostic_log(code, DiagnosticLevel::Warning, pos, len, msg, &[]);
}
/// Emit an error diagnostic using raw position/length (no AstNode).
/// Used for deferred validation (e.g., symbol conflict detection in parse_nsp §14).
pub fn dlog_error_at(code: u32, pos: Position, len: u32, msg: &str) {
    let uri = crate::current_uri::get();
    tracing::debug!(
        target: "mcc::diagnostic",
        code = code,
        pos = pos,
        len = len,
        file = uri.as_str(),
        "{msg}"
    );
    diagnostic_log(code, DiagnosticLevel::Error, pos, len, msg, &[]);
}

pub fn dlog_clear_file(uri: &McURI) {
    workspace::WORKSPACE
        .diagnostics
        .lock()
        .unwrap()
        .clear_file(uri);
}
