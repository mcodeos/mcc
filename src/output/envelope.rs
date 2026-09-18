// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Output envelope — the JSON-RPC response shape wrapping every command result.
//!
//! ```json
//! {
//!   "jsonrpc": "2.0",
//!   "result":  { ... }   // Success
//!   // Or error
//!   "error":   { ... }   // Error
//! }
//! ```
//!
//! [`CommandResult`] puts `pass1` / `pass2` / `view` / `viz` as top-level
//! sibling keys (instead of nested). This allows consumers to fetch `pass2.nets` in one line
//! using `jq '.result.pass2.nets'`, without walking nested paths or using case branches.
//!
//! ## Design considerations
//!
//! - All `Option` fields add `skip_serializing_if = "Option::is_none"`,
//!   keeping JSON clean (passes that didn't run don't appear in output, rather than `null`).
//! - All enums use `#[serde(rename_all = "snake_case")]`, outputting lowercase without ambiguity.
//! - All Diagnostics carry the `phase` field, ensuring the semantic level can be traced back to a
//! specific pass.

use mcc::ledger::LedgerReport;
use serde::{Deserialize, Serialize};

// Top-level envelope

#[derive(Debug, Serialize, Deserialize)]
pub struct Envelope {
    pub jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
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
}

// RpcError - Standard JSON-RPC error format

#[derive(Debug, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl RpcError {
    /// 32110: Pass1 (parse) error. No production caller; kept for the
    /// envelope serialization tests.
    #[allow(dead_code)]
    pub fn parse_error(msg: impl Into<String>) -> Self {
        Self {
            code: 32110,
            message: msg.into(),
            data: None,
        }
    }

    /// 32111: Pass2 (build/instantiate) phase error
    pub fn build_error(msg: impl Into<String>) -> Self {
        Self {
            code: 32111,
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
}

// CommandResult - result body

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct CommandResult {
    /// Command string, e.g. "mcc parse" / "mcc build" / "mcc show pins"
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
    pub view: Option<ViewData>,

    /// Stage readout (`mcc show stage`). Its own sibling key rather than a
    /// reuse of `view`: a stage view is a pipeline segment, not one of the
    /// read-side projections `ViewData` carries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<StageViewData>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub viz: Option<VizData>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<QueryData>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub export: Option<ExportData>,

    /// Failure ledger (resolve-gate-design.md §7.1-2): cross-pass record of
    /// non-clean parses — silent fallbacks, phantoms, floating wires. Summary
    /// counts always; `detail` rows only under `--ledger` / `--ledger=audit`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ledger: Option<LedgerReport>,

    // ── A-tier read projections (design §2.5, U86 item 7) ──
    //
    // Read-side commands whose `-f json` stdout was the *bare* payload — the
    // same reading, minus the envelope. Each now hangs under its own key (the
    // ruling for this slice: command envelope + projection key). Carried
    // **verbatim**: wrapping must not reshape it, so an existing consumer moves
    // exactly one level down (`points` → `result.list.points`).
    //
    // Built through [`crate::output::emit_projection`], the single place that
    // knows the key ↔ `mcc <command>` pairing; the payload types stay
    // `serde_json::Value` because those commands build them as `json!` inline
    // (typed homes are a separate change, not a wrapping one).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub erc: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub rules: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub def: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub refs: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub explain: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<serde_json::Value>,

    // `show`'s 21 sub-faces share this one key (the second slice of the same
    // ruling). The sub-face is named by the envelope's `command` — `mcc show
    // pins` — not by a second key or an injected field, because 16 of the
    // payloads carry no `type` discriminator and wrapping must not reshape
    // them. Consumers dispatch on `command`, or on the payload's own fields
    // where it has them (`type` for all/defs/dianlu/pwr/pwrflow).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show: Option<serde_json::Value>,

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
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceKind {
    #[default]
    Project,
    #[serde(rename = "sandbox")]
    Sandbox,
}

// Pass0 report — load + lib phase diagnostics

/// Pass 0 = lib load + `mcc_load_project` phase.
/// Snapshot once in [`crate::cmds::parse::public_collect_pass0`].
/// No definitions (none built yet at that point), loaded_files is left for upper layer to fill
/// explicitly (usually empty).
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Pass0Report {
    pub loaded_files: Vec<LoadedFile>,
    pub diagnostics: Vec<Diagnostic>,
}

// Pass1 report

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Pass1Report {
    pub loaded_files: Vec<LoadedFile>,
    pub definitions: DefinitionsIndex,
    pub diagnostics: Vec<Diagnostic>,
}

impl Pass1Report {
    /// Fold `other`'s Pass 1 into this one, keeping the first sighting of each
    /// definition, file and diagnostic.
    ///
    /// A directory is a container of definition spaces, each parsed in its own
    /// world, and a file reached by two entries' `use` closures is parsed once
    /// per world — so a folder's entries overlap, and joining them as they come
    /// would list a shared file, and mcode with it, once per entry that reaches
    /// it. The report is about the folder: a definition or a problem in a file
    /// is one entry however many entries include that file.
    pub fn merge(&mut self, other: Pass1Report) {
        for f in other.loaded_files {
            if !self.loaded_files.iter().any(|e| e.uri == f.uri) {
                self.loaded_files.push(f);
            }
        }
        let Pass1Report {
            definitions,
            diagnostics,
            ..
        } = other;
        self.definitions.merge(definitions);
        for d in diagnostics {
            if !self.diagnostics.iter().any(|e| same_diagnostic(e, &d)) {
                self.diagnostics.push(d);
            }
        }
    }
}

/// Two diagnostics are the same problem when they say the same thing about the
/// same place.
pub fn same_diagnostic(a: &Diagnostic, b: &Diagnostic) -> bool {
    a.code == b.code
        && a.message == b.message
        && a.location.as_ref().map(|l| (&l.file, l.pos))
            == b.location.as_ref().map(|l| (&l.file, l.pos))
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
    /// Module port definitions (psrc/psnk/psbi/io/in/out)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<PortRef>,
}

impl DefinitionsIndex {
    /// Fold `other` in, keyed by `(name, uri)` — see [`Pass1Report::merge`].
    pub fn merge(&mut self, other: DefinitionsIndex) {
        let DefinitionsIndex {
            modules,
            components,
            interfaces,
            enums,
            ports,
        } = other;
        for (into, from) in [
            (&mut self.modules, modules),
            (&mut self.components, components),
            (&mut self.interfaces, interfaces),
            (&mut self.enums, enums),
        ] {
            for r in from {
                if !into.iter().any(|e| e.name == r.name && e.uri == r.uri) {
                    into.push(r);
                }
            }
        }
        for p in ports {
            if !self
                .ports
                .iter()
                .any(|e| e.name == p.name && e.uri == p.uri && e.module == p.module)
            {
                self.ports.push(p);
            }
        }
    }
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

// Pass2 report

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
    /// True when this node is a synthetic `VIRT_<T>` wrapper fabricated so a
    /// standalone component/interface file can be built. Set at tree→node
    /// conversion time (in the building process) and carried in the envelope,
    /// so instance counts can exclude wrappers even after an RPC round-trip
    /// (the client process has no synthetic-name registry).
    #[serde(default)]
    pub synthetic: bool,
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
    /// Full module-scope path the net belongs to (e.g. `main.speaker`). Instance
    /// names collide across scopes, so entries are ambiguous without it.
    pub module: String,
    pub name: String,
    pub points: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectionEntry {
    pub id: u32,
    /// Full module-scope path the connection belongs to (e.g. `main.speaker`).
    /// The engine's connection ids are per-module counters, so `id` alone does
    /// not disambiguate across scopes — this does.
    pub module: String,
    /// Net this connection belongs to. Always resolved against the module's net
    /// table (surviving name after union-find merges), so it matches the name
    /// in the matching Nets table row. Falls back to the statement label when
    /// the points are absent from the table (e.g. NC).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_name: Option<String>,
    pub points: Vec<String>,
}

// Auxiliary result types: view / viz

#[derive(Debug, Serialize, Deserialize)]
pub struct ViewData {
    /// "ast" | "tree" | "hierarchy" | "schematic"
    pub target: String,
    pub data: serde_json::Value,
}

/// Stage readout — `mcc show stage <p1|p2|vec|viz>`
/// (design `mcd/doc/pipeline/stage-readout-design.md` §3 / §5.3 ①).
///
/// This is the **minimal projection envelope**: the six fields of
/// `projection-schema-design.md` §1, carried as one more `view` value on the
/// existing envelope rather than as a second envelope format (law B). `view` is
/// `stage.<seg>` and names a *pipeline stage*, which is why it does not
/// impersonate one of the six read-side projections of the frozen world.
#[derive(Debug, Serialize, Deserialize)]
pub struct StageViewData {
    /// Envelope schema version (`proj.1.0`).
    pub schema_version: String,
    /// Root token: the loaded world's source set as a deterministic hash, or
    /// null when it cannot be fingerprinted. See `mcc::stages::world_ver`.
    pub world_ver: Option<String>,
    /// The compiler that produced this view.
    pub mcc_version: String,
    /// `stage.p1` | `stage.p2` | `stage.vec` | `stage.viz`.
    pub view: String,
    /// The resolved top module this view is scoped to.
    pub top: String,
    /// Sorted by `(class, key)`. Every item carries its run-local key
    /// (`point`) *and* its cross-build `canon_key` — a view carrying only the
    /// former is invalidated by the next compiler change (design §3).
    pub items: serde_json::Value,
    /// Per-class item counts plus the diagnostic base. A count in the header
    /// line, never a gate (law C).
    pub counts: serde_json::Value,
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
}

/// Query result — used by `mcc query` (DSL and name modes; `mcc search` is a
/// query alias) and `defs.query` RPC.
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

// Summary

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

// Diagnostic (unified format, converted from mcc::Diagnostic by diagnostic.rs adapter)

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

// Tests: schema round-trip does not lose fields

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_envelope__envelope_ok_minimal_serializes_clean() {
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
    fn cli_envelope__envelope_err_serializes_with_code() {
        let env = Envelope::err(RpcError::parse_error("bad token"));
        let json = serde_json::to_string(&env).unwrap();
        assert!(json.contains("\"code\":32110"));
        assert!(json.contains("\"message\":\"bad token\""));
        assert!(!json.contains("\"result\""));
    }

    #[test]
    fn cli_envelope__pass1_pass2_are_sibling_keys() {
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

    /// One file, one row — however many entries of a directory batch reach it.
    ///
    /// The batch parses each entry in its own world, so a file pulled in by two
    /// `use` closures is parsed twice and diagnosed twice. The report is about
    /// the *folder*, which is the caller's unit; folding per-world reports is
    /// therefore a keyed union and not a concatenation, or every shared file and
    /// every library file would appear once per entry.
    #[test]
    fn cli_envelope__merge_folds_a_shared_file_into_one_row() {
        fn world(uri: &str) -> Pass1Report {
            let diag = Diagnostic {
                phase: Phase::Pass1,
                severity: Severity::Warning,
                code: mcc::errcodes::INST_CLASS_UNRESOLVED,
                message: "no such class".into(),
                location: Some(DiagLocation {
                    file: uri.into(),
                    line: 4,
                    column: 5,
                    end_line: None,
                    end_column: None,
                    pos: 42,
                    len: 7,
                }),
                suggestions: vec![],
                related: vec![],
            };
            Pass1Report {
                loaded_files: vec![LoadedFile {
                    uri: uri.into(),
                    is_system: false,
                    modules: vec!["SHARED".into()],
                    components: vec![],
                    interfaces: vec![],
                    enums: vec![],
                }],
                definitions: DefinitionsIndex {
                    modules: vec![DefinitionRef {
                        name: "SHARED".into(),
                        uri: uri.into(),
                    }],
                    ..Default::default()
                },
                diagnostics: vec![diag],
            }
        }

        let mut merged = world("/p/shared.mc");
        merged.merge(world("/p/shared.mc"));
        assert_eq!(
            merged.loaded_files.len(),
            1,
            "a file two entries reached is one file"
        );
        assert_eq!(
            merged.definitions.modules.len(),
            1,
            "and its definitions are its own, listed once"
        );
        assert_eq!(
            merged.diagnostics.len(),
            1,
            "and its problem is one problem, not one per entry that reached it"
        );

        // A genuinely different file is a different row.
        merged.merge(world("/p/other.mc"));
        assert_eq!(merged.loaded_files.len(), 2);
        assert_eq!(merged.definitions.modules.len(), 2);
        assert_eq!(merged.diagnostics.len(), 2);
    }
}
