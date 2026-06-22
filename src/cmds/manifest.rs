// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! manifest.toml parsing + `mcc build` integration — PR-4b
//!
//! ## Manifest format
//!
//! ```toml
//! [project]
//! name = "hbl"
//! version = "1.0.0"
//! entry = "src/hbl.mc"       # Entry file (relative to project root)
//! top_module = "main"        # Default top-level module
//!
//! [dependencies]
//! mcode = "*"                # Base library, always required
//! infineon = "2.1.0"         # Third-party library
//! ```
//!
//! ## `mcc build` flow
//!
//! 1. Read manifest → parse entry / top / dependencies
//! 2. Auto `lib load` all dependencies
//! 3. `mcc_load_project(entry)` → `mcc_build(top)`
//! 4. Output envelope

use crate::cli::data_dir;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

// ============================================================================
// Manifest struct
// ============================================================================

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

    /// Find manifest from project root.
    /// Prefers `manifest.toml`, then `mcc.toml`.
    pub fn find_in(root: &Path) -> Option<PathBuf> {
        let candidates = ["manifest.toml", "project.toml", "mcc.toml"];
        for name in &candidates {
            let p = root.join(name);
            if p.exists() {
                return Some(p);
            }
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

// ============================================================================
// Build flow
// ============================================================================

/// Core logic for `mcc build`.
///
/// 1. Read manifest (if present)
/// 2. Load dependency libraries
/// 3. Load project entry
/// 4. Build
///
/// Returns (entry_uri, top_module_name) for caller to build envelope.
pub fn build_from_manifest(
    project_root: &Path,
    cli_top: Option<&str>,
    cli_entry: Option<&str>,
) -> Result<(String, String)> {
    // 1. Try reading manifest
    let manifest = Manifest::find_in(project_root).and_then(|p| Manifest::load(&p).ok());

    let (entry, top) = if let Some(ref m) = manifest {
        let entry = cli_entry
            .map(|s| project_root.join(s))
            .unwrap_or_else(|| m.entry_path(project_root));
        let top = m.top_module_or(cli_top);
        (entry, top)
    } else {
        let entry = cli_entry
            .map(|s| PathBuf::from(s))
            .ok_or_else(|| anyhow::anyhow!("build: no manifest and no entry file specified"))?;
        let top = cli_top.map(|s| s.to_string());
        (entry, top)
    };

    // 2. Load unloaded dependency libraries
    if let Some(ref m) = manifest {
        let system_root = mcc::mcb_get_system_root();
        for (lib_name, _version) in &m.dependencies {
            if !mcc::mcb_loaded_libs().contains(lib_name) {
                let lib_root = system_root.join(lib_name);
                if lib_root.exists() {
                    tracing::info!(target: "mcc::build",
                        lib = lib_name,
                        path = ?lib_root,
                        "loading dependency");
                    mcc::mcb_load_lib(lib_name, &lib_root);
                } else {
                    tracing::warn!(target: "mcc::build",
                        lib = lib_name,
                        "dependency not found in system root");
                }
            }
        }
    }

    // 3. Load project
    let entry_uri = entry.to_string_lossy().to_string();
    mcc::mcc_load_project(&entry_uri);

    // 4. Determine top module
    let top_name = top
        .or_else(|| mcc::mcb_get_module_name_by_uri(&entry_uri))
        .or_else(|| mcc::mcb_get_first_module_name())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "build: cannot find top-level module. Set top_module in manifest or use --top"
            )
        })?;

    Ok((entry_uri, top_name))
}

/// Load libraries by CLI --lib specified list.
pub fn load_libs(lib_names: &[String]) {
    for lib_name in lib_names {
        let system_root = mcc::mcb_get_system_root();
        let lib_root = system_root.join(lib_name);
        // Check if library truly loaded interfaces (built-in components don't count, need interfaces to count as truly loaded)
        let lib_info = mcc::mcb_lib_info(lib_name);
        let interface_count = lib_info.as_ref().map(|i| i.interface_count).unwrap_or(0);
        if lib_root.exists() && (!mcc::mcb_loaded_libs().contains(lib_name) || interface_count == 0)
        {
            tracing::info!(target: "mcc::lib",
                lib = lib_name,
                path = ?lib_root,
                "loading library");
            mcc::mcb_load_lib(lib_name, &lib_root);
        } else if !lib_root.exists() {
            tracing::warn!(target: "mcc::lib",
                lib = lib_name,
                "library not found in system root");
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_manifest_toml() {
        let toml = r#"
[project]
name = "hbl"
version = "1.0.0"
entry = "src/hbl.mc"
top_module = "main"

[dependencies]
mcode = "*"
infineon = "2.1.0"
"#;
        let m: Manifest = toml::from_str(toml).unwrap();
        assert_eq!(m.project.name, "hbl");
        assert_eq!(m.project.entry, "src/hbl.mc");
        assert_eq!(m.project.top_module, Some("main".into()));
        assert_eq!(m.dependencies.len(), 2);
        assert_eq!(m.dependencies["infineon"], "2.1.0");
    }

    #[test]
    fn generate_default_manifest() {
        let s = Manifest::generate_default("test_proj", "src/main.mc");
        assert!(s.contains("name = \"test_proj\""));
        assert!(s.contains("entry = \"src/main.mc\""));
        assert!(s.contains("mcode = \"*\""));
    }
}
