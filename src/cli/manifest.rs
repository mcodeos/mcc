// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `project.toml` — what names a project's entry file.
//!
//! ## Format
//!
//! ```toml
//! [project]
//! name = "example"
//! version = "1.0.0"
//! entry = "src/main.mc"       # Entry file (relative to project root)
//! top_module = "main"         # Default top-level module
//!
//! [dependencies]
//! mcode = "*"                 # Base library, always required
//! infineon = "2.1.0"          # Third-party library
//! ```
//!
//! Lives in the lib (not the binary's `cmds/`) because the batch entry resolver
//! ([`crate::build::loader::discover_entries`]) runs on the RPC side too. One
//! reader for one file: a second, hand-rolled scanner would drift from this one
//! on anything it does not happen to handle (single-quoted values, key order,
//! inline tables).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub project: ProjectSection,
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSection {
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    /// Entry .mc file (relative to project root)
    pub entry: String,
    /// Default top-level module name
    #[serde(default)]
    pub top_module: Option<String>,
}

fn default_version() -> String {
    "0.1.0".into()
}

impl Manifest {
    /// Parse from toml file.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read manifest: {}", path.display()))?;
        let manifest: Manifest = toml::from_str(&content)
            .with_context(|| format!("Failed to parse manifest: {}", path.display()))?;
        Ok(manifest)
    }

    /// Parse from already-read manifest content.
    pub fn parse(content: &str) -> Result<Self> {
        toml::from_str(content).context("Failed to parse manifest")
    }

    /// The manifest in `root`, if it has one and it parses.
    ///
    /// A manifest that exists but cannot be read is treated as absent: the
    /// caller is a directory walk deciding which definition space a file
    /// belongs to, and "the manifest is broken" is a diagnostic for the project
    /// itself to report, not a reason to invent a world for it.
    pub fn find_and_load(root: &Path) -> Option<Self> {
        Self::find_in(root).and_then(|p| Self::load(&p).ok())
    }

    /// Find the project manifest (`project.toml`) from the project root.
    /// Delegates to the shared data-dir helper so CLI, RPC and MCP agree on
    /// the manifest name.
    pub fn find_in(root: &Path) -> Option<PathBuf> {
        super::datadir::find_manifest_in(root)
    }

    /// The nearest directory at or above `dir` that holds a manifest.
    ///
    /// A named directory is usually *inside* the project it belongs to
    /// (`mcc check proj/src`), so a resolver that only looked downwards would
    /// read that project as a folder of unrelated files. This is the one place
    /// that walks up.
    pub fn nearest_root(dir: &Path) -> Option<PathBuf> {
        let mut current = Some(dir);
        while let Some(d) = current {
            if Self::find_in(d).is_some() {
                return Some(d.to_path_buf());
            }
            current = d.parent();
        }
        None
    }

    /// Generate default manifest content.
    pub fn generate_default(name: &str, entry: &str) -> String {
        format!(
            r#"[project]
name = "{}"
version = "0.1.0"
entry = "{}"
# top_module = "main"

[dependencies]
mcode = "*"
"#,
            name, entry
        )
    }

    /// Resolve entry absolute path (relative to project root).
    pub fn entry_path(&self, project_root: &Path) -> PathBuf {
        project_root.join(&self.project.entry)
    }

    /// Get top_module name (prefers manifest, overridable by CLI --top).
    pub fn top_module_or(&self, cli_override: Option<&str>) -> Option<String> {
        cli_override
            .map(|s| s.to_string())
            .or_else(|| self.project.top_module.clone())
    }
}
