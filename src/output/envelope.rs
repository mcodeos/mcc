// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! [`McVecGraph`] —— Graph container
//!
//! Holds a layer's boxes / edges (old, deprecated) / nets / sub-graphs.
//!
//! ## ★ P03 (S1) Changes
//! - `edges` field **is still present but not populated**:
//!   - `from_block.rs::build_mc_vec_graph` no longer writes to `graph.edges`
//!   - `components.rs::build_adjacency` now reads from `graph.nets` only
//!   - `entry_points.rs::collect_pins_per_box` same
//!   - `wire.rs::render_edge` has been removed
//! - `nets: Vec<VizNet>` is the **only network representation**
//! - `total_edges()` / `total_wires()` still compile but always return 0
//!   "result":  { ... }   // Success
//!   // Or error
//!   "error":   { ... }   // Error
//! }
//! ```
//!
//! [`CommandResult`] puts `pass1` / `pass2` / `extract` / `view` / `viz` as top-level
//! sibling keys (instead of nested). This allows consumers to fetch `pass2.nets` in one line
//! using `jq '.result.pass2.nets'`, without walking nested paths or using case branches.
//!
//! ## Design considerations
//!
//! - All `Option` fields add `skip_serializing_if = "Option::is_none"`,
//!   keeping JSON clean (passes that didn't run don't appear in output, rather than `null`).
//! - All enums use `#[serde(rename_all = "snake_case")]`, outputting lowercase without ambiguity.
//! - All Diagnostics carry the `phase` field, ensuring the semantic level can be traced back to a specific pass.

use serde::{Deserialize, Serialize};

// ============================================================================
// Top-level envelope
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
pub struct Envelope {
    pub jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<CommandResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl Envelope {
    pub fn ok(result: CommandResult) -> Self {
        Self {
            jsonrpc: "2.0",
            id: None,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(error: RpcError) -> Self {
        Self {
            jsonrpc: "2.0",
            id: None,
            result: None,
            error: Some(error),
        }
    }

    pub fn with_id(mut self, id: u64) -> Self {
        self.id = Some(id);
        self
    }
}

// ============================================================================
// RpcError - Standard JSON-RPC error format
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl RpcError {
    /// -32001: Pass1 (parse) error
    pub fn parse_error(msg: impl Into<String>) -> Self {
        Self {
            code: -32001,
            message: msg.into(),
            data: None,
        }
    }

    /// -32002: Pass2 (build/instantiate) phase error
    pub fn build_error(msg: impl Into<String>) -> Self {
        Self {
            code: -32002,
            message: msg.into(),
            data: None,
        }
    }

    /// -32003: User input file / path not found
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self {
            code: -32003,
            message: msg.into(),
            data: None,
        }
    }

    /// -32602: Caller passed invalid params (no top module found)
    pub fn invalid_params(msg: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: msg.into(),
            data: None,
        }
    }

    /// -32603: Internal error (panic, unreachable, IO failed)
    pub fn internal_error(msg: impl Into<String>) -> Self {
        Self {
            code: -32603,
            message: msg.into(),
            data: None,
        }
    }

    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }
}

// ============================================================================
// CommandResult - result body
// ============================================================================

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct CommandResult {
    /// Command string, e.g. "mcc parse" / "mcc build" / "mcc extract instances"
    pub command: String,

    /// Current workspace reference (PR-3 uses onsite/project, PR-2 defaults to anonymous)
    pub workspace: WorkspaceRef,

    /// Pass 0 = lib load + project load (C parser) phase diagnostics.
    /// Snapshot once in [`crate::cmds::parse::public_collect_pass0`].
    /// No definitions (at that point), loaded_files left for upper layer to fill (usually empty).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pass0: Option<Pass0Report>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub pass1: Option<Pass1Report>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub pass2: Option<Pass2Report>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub extract: Option<ExtractData>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub view: Option<ViewData>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub viz: Option<VizData>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<SearchData>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<QueryData>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub export: Option<ExportData>,

    pub summary: Summary,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct WorkspaceRef {
    pub kind: WorkspaceKind,
    pub name: String,
}

impl WorkspaceRef {
    pub fn project(name: impl Into<String>) -> Self {
        Self {
            kind: WorkspaceKind::Project,
            name: name.into(),
        }
    }
    pub fn sandbox(id: impl Into<String>) -> Self {
        Self {
            kind: WorkspaceKind::Sandbox,
            name: id.into(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceKind {
    #[default]
    Project,
    #[serde(rename = "sandbox")]
    Sandbox,
}

// ============================================================================
// Pass0 report — load + lib phase diagnostics
// ============================================================================

/// Pass 0 = lib load + `mcc_load_project` phase.
/// Snapshot once in [`crate::cmds::parse::public_collect_pass0`].
/// No definitions (none built yet at that point), loaded_files is left for upper layer to fill explicitly (usually empty).
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Pass0Report {
    pub loaded_files: Vec<LoadedFile>,
    pub diagnostics: Vec<Diagnostic>,
}

// ============================================================================
// Pass1 report
// ============================================================================

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Pass1Report {
    pub loaded_files: Vec<LoadedFile>,
    pub definitions: DefinitionsIndex,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoadedFile {
    pub uri: String,
    pub is_system: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub modules: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub interfaces: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub enums: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct DefinitionsIndex {
    pub modules: Vec<DefinitionRef>,
    pub components: Vec<DefinitionRef>,
    pub interfaces: Vec<DefinitionRef>,
    pub enums: Vec<DefinitionRef>,
    /// Module port definitions (ps/io/in/out)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<PortRef>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct PortRef {
    pub name: String,
    pub iotype: String,
    pub module: String,
    pub uri: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DefinitionRef {
    pub name: String,
    pub uri: String,
}

// ============================================================================
// Pass2 report
// ============================================================================

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Pass2Report {
    pub top: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instances: Option<InstanceNode>,
    pub nets: Vec<NetEntry>,
    pub connections: Vec<ConnectionEntry>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InstanceNode {
    pub name: String,
    /// "module" | "component"
    pub kind: String,
    pub class_name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<PortInfo>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<ComponentInfo>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sub_modules: Vec<InstanceNode>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PortInfo {
    pub name: String,
    /// "in" | "out" | "inout" | "power" | "analog"
    pub iotype: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ComponentInfo {
    pub name: String,
    pub class_name: String,
    /// Pin list: each element contains pin_id and pin_name
    pub pins: Vec<PinInfo>,
    pub nc: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PinInfo {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NetEntry {
    pub name: String,
    pub points: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectionEntry {
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_name: Option<String>,
    pub points: Vec<String>,
}

// ============================================================================
// Auxiliary result types: extract / view / viz
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
pub struct ExtractData {
    /// "instances" | "nets" | "components" | "interfaces" | ...
    pub target: String,
    pub items: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ViewData {
    /// "ast" | "tree" | "hierarchy" | "schematic"
    pub target: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VizData {
    /// "json" | "html" | "svg"
    pub format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub written_to: Option<String>,
    pub bytes: usize,
    pub layers: usize,
    pub boxes: usize,
    pub edges: usize,
}

// ============================================================================
// Search result (M5)
// ============================================================================

/// Search hits — used by `mcc search` and `defs.search` RPC.
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchData {
    /// The pattern the user searched for (as-typed)
    pub pattern: String,
    /// Optional kind restriction: "component" | "module" | "interface" | "enum" | "instance" | null
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Whether pattern was treated as a regular expression
    pub regex: bool,
    /// Whether pattern was fuzzy-matched (Levenshtein ≤ 2)
    pub fuzzy: bool,
    /// Number of items in `items`
    pub count: usize,
    /// Vec<{kind, name, uri}> serialized as JSON array
    pub items: serde_json::Value,
}

/// Query result — used by `mcc query` and `defs.query` RPC.
///
/// Distinct from `SearchData` (no pattern mode, regex flag, fuzzy flag, or kind
/// option) so the contract can evolve independently.
#[derive(Debug, Serialize, Deserialize)]
pub struct QueryData {
    /// The expression the user queried (as-typed)
    pub expr: String,
    /// Number of items in `items`
    pub count: usize,
    /// Vec<{kind, name, uri}> serialized as JSON array
    pub items: serde_json::Value,
}

/// Export result — used by `mcc export` and `export` RPC.
///
/// When format is `text` or `csv` (raw stdout), `items` is null and the
/// artifact was emitted directly to stdout/file.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExportData {
    /// "netlist" | "bom" | "spice"
    pub kind: String,
    /// Output format actually used: "text" | "json" | "csv"
    pub format: String,
    /// For bom: row count; for netlist: net count; for spice: instance count
    pub count: usize,
    /// Structured payload for JSON; null for text/csv (raw artifact on stdout).
    pub items: serde_json::Value,
}

// ============================================================================
// Summary
// ============================================================================

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Summary {
    pub module_count: usize,
    pub component_count: usize,
    pub interface_count: usize,
    pub instance_count: usize,
    pub net_count: usize,
    pub errors: usize,
    pub warnings: usize,
    pub elapsed_ms: u128,
}

// ============================================================================
// Diagnostic (unified format, converted from mcc::Diagnostic by diagnostic.rs adapter)
// ============================================================================

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Diagnostic {
    pub phase: Phase,
    pub severity: Severity,
    pub code: u32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<DiagLocation>,
    /// Quick-fix suggestions (M6).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub suggestions: Vec<DiagnosticSuggestion>,
    /// Related locations / context (M6).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub related: Vec<DiagnosticRelated>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Pass0,
    Pass1,
    Pass2,
    Viz,
    Other,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DiagLocation {
    pub file: String,
    pub line: u32,
    pub column: u32,
    /// End line (M6). Computed from pos+len when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    /// End column (M6). Computed from pos+len when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_column: Option<u32>,
    pub pos: u32,
    pub len: u32,
}

/// A quick-fix suggestion attached to a diagnostic (M6).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DiagnosticSuggestion {
    /// Human-readable label for this fix.
    pub message: String,
    /// Replacement text.
    pub replacement: String,
    /// Span to replace.
    pub location: DiagLocation,
}

/// A related location / context note attached to a diagnostic (M6).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DiagnosticRelated {
    pub message: String,
    pub location: DiagLocation,
}

// ============================================================================
// Tests: schema round-trip does not lose fields
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_ok_minimal_serializes_clean() {
        let env = Envelope::ok(CommandResult {
            command: "mcc load".into(),
            workspace: WorkspaceRef::project("test"),
            ..Default::default()
        });
        let json = serde_json::to_string(&env).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"command\":\"mcc load\""));
        // pass0/pass1/pass2 not set, should not appear
        assert!(!json.contains("\"pass0\""));
        assert!(!json.contains("\"pass1\""));
        assert!(!json.contains("\"pass2\""));
        // error not set, should not appear
        assert!(!json.contains("\"error\""));
    }

    #[test]
    fn envelope_err_serializes_with_code() {
        let env = Envelope::err(RpcError::parse_error("bad token"));
        let json = serde_json::to_string(&env).unwrap();
        assert!(json.contains("\"code\":-32001"));
        assert!(json.contains("\"message\":\"bad token\""));
        assert!(!json.contains("\"result\""));
    }

    #[test]
    fn pass1_pass2_are_sibling_keys() {
        let res = CommandResult {
            command: "mcc build".into(),
            workspace: WorkspaceRef::project("test"),
            pass1: Some(Pass1Report::default()),
            pass2: Some(Pass2Report::default()),
            ..Default::default()
        };
        let v = serde_json::to_value(&res).unwrap();
        // Sibling keys, not nested
        assert!(v.get("pass1").is_some());
        assert!(v.get("pass2").is_some());
        assert!(v["pass1"].get("pass2").is_none());
    }
}
