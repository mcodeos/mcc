// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! RPC parameter structs — extracted from mod.rs.

use serde::Deserialize;

pub(crate) fn default_true() -> bool {
    true
}

#[derive(Deserialize)]
pub(crate) struct LibraryShowParams {
    pub(crate) name: String,
}

#[derive(Deserialize)]
pub(crate) struct LibInstallParams {
    pub(crate) name: String,
    pub(crate) from: String,
    #[serde(default)]
    pub(crate) version: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct LibUninstallParams {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) force: bool,
}

#[derive(Deserialize)]
pub(crate) struct LibSearchParams {
    pub(crate) pattern: String,
}

#[derive(Deserialize, Default)]
pub(crate) struct DefsSearchParams {
    pub(crate) pattern: String,
    #[serde(default)]
    pub(crate) kind: Option<String>,
    #[serde(default)]
    pub(crate) regex: bool,
    #[serde(default)]
    pub(crate) fuzzy: bool,
    #[serde(default)]
    pub(crate) top: Option<String>,
    #[serde(default)]
    pub(crate) limit: usize,
}

#[derive(Deserialize, Default)]
pub(crate) struct DefsQueryParams {
    pub(crate) expr: String,
    #[serde(default)]
    pub(crate) limit: usize,
}

#[derive(Deserialize, Default)]
pub(crate) struct ExportRpcParams {
    /// "netlist" | "bom" | "spice"
    #[serde(default)]
    pub(crate) kind: String,
    /// Source .mc file path
    pub(crate) entry: String,
    /// Top module name (optional; defaults to first module)
    #[serde(default)]
    pub(crate) top: Option<String>,
    /// "text" | "json" | "csv" — defaults to "text"
    #[serde(default)]
    pub(crate) format: Option<String>,
    /// Library names to load
    #[serde(default)]
    pub(crate) libs: Vec<String>,
}

#[derive(Deserialize)]
pub(crate) struct TraceSetParams {
    pub(crate) name: String,
    pub(crate) value: bool,
}

#[derive(Default, Deserialize)]
pub(crate) struct BuildFullParams {
    #[serde(default)]
    pub(crate) entry: Option<String>,
    #[serde(default)]
    pub(crate) top: Option<String>,
    /// Whether to include system library definitions, default true
    #[serde(default = "default_true")]
    pub(crate) include_system: bool,
    /// Whether to output AST visit, default false
    #[serde(default)]
    pub(crate) include_ast: bool,
    #[serde(default)]
    pub(crate) libs: Vec<String>,
}

pub(crate) struct FileEntry {
    pub(crate) uri: String,
    pub(crate) is_system: bool,
    pub(crate) modules: Vec<String>,
    pub(crate) components: Vec<String>,
    pub(crate) interfaces: Vec<String>,
    pub(crate) enums: Vec<String>,
}

#[derive(Deserialize, Default)]
pub(crate) struct CheckRpcParams {
    #[serde(default)]
    pub(crate) entry: Option<String>,
    /// Inline source content (M6). When set, loaded from memory — no disk I/O.
    #[serde(default)]
    pub(crate) content: Option<String>,
    #[serde(default)]
    pub(crate) libs: Vec<String>,
    #[serde(default)]
    pub(crate) strict: bool,
    #[serde(default)]
    pub(crate) errors_only: bool,
}

#[derive(Deserialize, Default)]
pub(crate) struct ExtractRpcParams {
    #[serde(default)]
    pub(crate) entry: Option<String>,
    #[serde(default)]
    pub(crate) target: String,
    #[serde(default)]
    pub(crate) top: Option<String>,
    #[serde(default)]
    pub(crate) libs: Vec<String>,
}

#[derive(Deserialize)]
pub(crate) struct UploadFile {
    pub(crate) path: String,
    pub(crate) content: String,
}

#[derive(Default, Deserialize)]
pub(crate) struct ParseParams {
    #[serde(default)]
    pub(crate) entry: Option<String>,
    #[serde(default)]
    pub(crate) top: Option<String>,
    /// System libraries to load (e.g. like ["mc/mcode"]);
    #[serde(default)]
    pub(crate) libs: Vec<String>,
    /// Whether to include system library definitions, default: true
    #[serde(default = "default_true")]
    pub(crate) include_system: bool,
}

#[derive(Deserialize, Default)]
pub(crate) struct ShowParams {
    pub(crate) name: Option<String>,
    pub(crate) file: Option<String>,
    #[serde(rename = "type")]
    pub(crate) type_filter: Option<String>,
    pub(crate) top: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct SemParams {
    pub(crate) uri: String,
    pub(crate) content: Option<String>,
}
