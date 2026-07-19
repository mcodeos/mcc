// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::ast::ast_node::AstNode;
use crate::builder::workspace;
use crate::McURI;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
pub type Position = u32;

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
    pub row: u32, // 1-based line number
    pub col: u32, // 1-based column number
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
    pub const INVALID_IO_TYPE: &str = "Invalid IO type '{0}'";
    pub const CANNOT_TRANSPOSE: &str = "Cannot transpose";
    pub const NOT_SUPPORT: &str = "Not support";
    pub const SHAPE_MISMATCH: &str = "Cannot connect. Shape mismatch. Left: {0}, right: {1}";

    pub const RANGE_UNIT_MISMATCH: &str =
        "Unit mismatch. Lower bound has unit {0} while upper bound has unit {1}";

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
        let (line, column) = workspace::WORKSPACE
            .mcodes
            .borrow()
            .get(&file)
            .map(|mcfile| mcfile.pos_to_line_col(pos))
            .unwrap_or((1, 1));

        Self {
            uri: file,
            pos,
            len,
            row: line,
            col: column,
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

    /*
        pub fn to_lsp_diagnostic(&self) -> lsp_types::Diagnostic {
            let mut diagnostic = lsp_types::Diagnostic {
                range: self.location.to_lsp_range(),
                severity: Some(self.level.as_lsp_severity()),
                code: self
                    .code
                    .clone()
                    .map(|code| lsp_types::NumberOrString::String(code)),
                source: Some("rust-compiler".to_string()),
                message: self.get_formatted_message(),
                related_information: None,
                tags: None,
                data: None,
            };

            if !self.related_information.is_empty() {
                let related = self
                    .related_information
                    .iter()
                    .map(|info| lsp_types::DiagnosticRelatedInformation {
                        location: lsp_types::Location {
                            uri: lsp_types::Url::parse(&format!("file://{}", info.location.file_path))
                                .unwrap(),
                            range: info.location.to_lsp_range(),
                        },
                        message: info.get_formatted_message(),
                    })
                    .collect();
                mc_diagnostic.related_information = Some(related);
            }

            mc_diagnostic
        }
    */
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
        self.diagnostics.push(diagnostic.clone());

        self.file_to_diagnostics
            .entry(diagnostic.loc.uri.clone())
            .or_default()
            .push(index);
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

    // pub fn clear_file(&mut self, file: &McURI) {
    //     if let Some(indices) = self.file_to_diagnostics.remove(file) {
    //         for &index in &indices {
    //             self.diagnostics[index] = Diagnostic::new(
    //                 DiagnosticLevel::Error,
    //                 SourceLocation::new(McURI::from(""), 0, 0),
    //                 "".to_string(),
    //                 &[],
    //             );
    //         }
    //         self.diagnostics.retain(|d| !d.message_template.is_empty());
    //     }
    // }
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
    // DEBUG: log pos=0 diagnostics to track down the source
    if pos == 0 && len > 100 {
        tracing::info!(target: "mcc::diagnostic", 
            "pos=0 diag: code={} len={} msg={}", 
            code, len, msg);
    }
    let new_diagnostic = Diagnostic::new(
        code,
        level,
        Location::new(crate::current_uri::get().clone(), pos, len),
        message_templates::format(msg, args),
    );

    workspace::WORKSPACE
        .diagnostics
        .borrow_mut()
        .add_diagnostic(new_diagnostic);
}

pub fn dlog_trace(code: u32, msg: &str) {
    diagnostic_log(code, DiagnosticLevel::Info, 0, 0, msg, &[]);
}
pub fn dlog_error(code: u32, node: &AstNode, msg: &str) {
    let full_msg = format!("node={} {}", node.get_type(), msg);
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
    let full_msg = format!("node={} {}", node.get_type(), msg);
    // Print to stderr so LSP server can capture it
    eprintln!(
        "[dlog_warning] code={} node_type={} node_pos={} node_len={} msg={}",
        code,
        node.get_type(),
        node.get_pos(),
        node.get_len(),
        full_msg
    );
    // Print chain of sub-nodes for debugging
    let mut cur = node.get_sub_node();
    let mut depth = 0;
    while let Some(n) = cur {
        eprintln!(
            "  [dlog_warning] sub[{}] type={} pos={} len={}",
            depth,
            n.get_type(),
            n.get_pos(),
            n.get_len()
        );
        cur = n.get_next();
        depth += 1;
        if depth > 10 {
            eprintln!("  [dlog_warning] ... (truncated)");
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
pub fn dlog_info(code: u32, node: &AstNode, msg: &str) {
    let full_msg = format!("node={} {}", node.get_type(), msg);
    diagnostic_log(
        code,
        DiagnosticLevel::Info,
        node.get_pos(),
        node.get_len(),
        &full_msg,
        &[],
    );
}
pub fn dlog_hint(code: u32, node: &AstNode, msg: &str) {
    let full_msg = format!("node={} {}", node.get_type(), msg);
    diagnostic_log(
        code,
        DiagnosticLevel::Hint,
        node.get_pos(),
        node.get_len(),
        &full_msg,
        &[],
    );
}

pub fn dlog_clear_file(uri: &McURI) {
    workspace::WORKSPACE
        .diagnostics
        .borrow_mut()
        .clear_file(uri);
}
