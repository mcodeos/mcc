// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! project.toml parsing + `mcc build` integration — PR-4b
//!
//! ## Manifest format
//!
//! ```toml
//! [project]
//! name = "example"
//! version = "1.0.0"
//! entry = "src/main.mc"       # Entry file (relative to project root)
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

    /// Find the project manifest (`project.toml`) from the project root.
    /// Delegates to the shared lib-layer helper so CLI, RPC and MCP agree on
    /// the manifest name.
    pub fn find_in(root: &Path) -> Option<PathBuf> {
        mcc::cli::datadir::find_manifest_in(root)
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
    // Project-local resources, including SVG symbols, resolve from this root.
    mcc::mcc_set_project_root(project_root);

    // 1. Try reading manifest
    let manifest = Manifest::find_in(project_root).and_then(|p| Manifest::load(&p).ok());

    let (entry, top) = if let Some(ref m) = manifest {
        let override_entry = cli_entry.is_some();
        let entry = cli_entry
            .map(|s| project_root.join(s))
            .unwrap_or_else(|| m.entry_path(project_root));
        // A CLI --entry replaces the manifest entry, so the manifest's
        // top_module no longer applies; the entry file's module (or --top)
        // wins instead.
        let top = if override_entry {
            cli_top.map(|s| s.to_string())
        } else {
            m.top_module_or(cli_top)
        };
        (entry, top)
    } else {
        let entry = cli_entry
            .map(|s| project_root.join(s))
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

    // 4. Determine top module.
    //    Priority (mcd docs-mc 16-export-viz §6): explicit top → targets in the
    //    entry file (all modules → components → interfaces, virtually
    //    instantiated) → first module anywhere in the workspace.
    let top_name = top
        .map(|s| s.to_string())
        .or_else(|| {
            mcc::mcc_virtual_resolve_targets(&entry_uri, None)
                .ok()
                .and_then(|t| t.into_iter().next())
        })
        .or_else(|| mcc::mcb_get_first_module_name())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "build: cannot find top-level module. Set top_module in manifest or use --top"
            )
        })?;

    Ok((entry_uri, top_name))
}

/// Browse-mode entry selection for a directory that has no manifest
/// (§19.5 rule 3 of use-design.md).
///
/// Priority:
/// 1. Explicit `--entry`: resolved against `root`.
/// 2. The unique `.mc` file under `root` that declares `module main`.
/// 3. An error prompting `--entry` when zero or several candidates exist.
pub fn select_browse_entry(root: &Path, cli_entry: Option<&str>) -> Result<PathBuf> {
    if let Some(entry) = cli_entry {
        let p = root.join(entry);
        if !p.is_file() {
            anyhow::bail!("browse: entry file not found: {}", p.display());
        }
        return Ok(p);
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    scan_entries_with_module_main(root, &mut candidates);
    candidates.sort();

    match candidates.len() {
        0 => anyhow::bail!(
            "browse: no `.mc` file declaring `module main` under {}; use --entry to select an entry file",
            root.display()
        ),
        1 => Ok(candidates.remove(0)),
        n => {
            let names: Vec<String> = candidates.iter().map(|p| p.display().to_string()).collect();
            anyhow::bail!(
                "browse: {} `.mc` files declare `module main` under {} ({}); use --entry to select one",
                n,
                root.display(),
                names.join(", ")
            );
        }
    }
}

/// Recursively collect `.mc` files under `current` that declare `module main`.
/// Hidden directories (leading `.`) are skipped.
fn scan_entries_with_module_main(current: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(current) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            if !p
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
            {
                scan_entries_with_module_main(&p, out);
            }
        } else if p.extension().is_some_and(|ext| ext == "mc") && file_declares_module_main(&p) {
            out.push(p);
        }
    }
}

/// True when `path` contains a top-level `module main` declaration
/// (comments and `module main2`-style identifiers do not count).
fn file_declares_module_main(path: &Path) -> bool {
    let Ok(content) = std::fs::read_to_string(path) else {
        return false;
    };
    for line in content.lines() {
        let line = line.trim_start();
        if line.is_empty() || line.starts_with("//") || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("pub ").unwrap_or(line);
        if let Some(rest) = line.strip_prefix("module main") {
            if rest.is_empty() || rest.starts_with('{') || rest.starts_with(char::is_whitespace) {
                return true;
            }
        }
    }
    false
}

/// Collect library names from all config sources, with deduplication.
///
/// Sources (in order):
/// 1. Global user config (~/.mcode/config/mcc.yaml)  → [libs].load
/// 2. Project project.toml                            → [config.libs].load (legacy)
/// 3. Project project.toml                            → [dependencies]       (manifest)
/// 4. CLI --lib
pub fn collect_libs(project_root: Option<&Path>, cli_libs: &[String]) -> Vec<String> {
    let mut libs = mcc::get_libs_load_list(project_root);
    if let Some(root) = project_root {
        if let Some(path) = Manifest::find_in(root) {
            if let Ok(manifest) = Manifest::load(&path) {
                for dep in manifest.dependencies.keys() {
                    if !libs.contains(dep) {
                        libs.push(dep.clone());
                    }
                }
            }
        }
    }
    for l in cli_libs {
        if !libs.contains(l) {
            libs.push(l.clone());
        }
    }
    // mcode standard library auto-loads by default unless disabled
    // (see LibsConfig::should_load_mcode / libs.disable_mcode).
    if mcc::should_load_mcode(project_root) && !libs.iter().any(|l| l.to_lowercase() == "mcode") {
        libs.push("mcode".to_string());
    }
    libs
}

/// Load exactly the given library names. No automatic global config loading.
pub fn load_libs(lib_names: &[String]) {
    for lib_name in lib_names {
        mcc::mcb_load_lib_by_name(lib_name);
    }
}

/// Walk up from `target` (a file or directory path) to find the project root:
/// the nearest ancestor directory containing `project.toml` (see
/// [`Manifest::find_in`]). Falls back to the target's own directory (or its
/// parent for a file) when nothing is found. Relative targets are resolved
/// against the current directory first, so the returned root is always
/// absolute.
pub fn find_project_root(target: Option<&str>) -> Option<PathBuf> {
    let t = target?;
    let raw = Path::new(t);
    let p = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(raw)
    };
    let mut current: Option<&Path> = if p.is_dir() { Some(&p) } else { p.parent() };
    while let Some(dir) = current {
        if Manifest::find_in(dir).is_some() {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }
    // Fallback: use the original heuristic (dir or parent of file).
    if p.is_dir() {
        Some(p)
    } else {
        p.parent().map(|p| p.to_path_buf())
    }
}

/// Shared initialization for all single-process local commands.
///
/// Initializes the engine without the config-gated system library, sets the
/// system root to the auto-discovered base, walks up from `target` to find the
/// project root (a directory containing project.toml, see [`find_project_root`]),
/// then loads libraries from global
/// config, project config, manifest, CLI --lib, plus the mcode default
/// (unless disabled by libs.disable_mcode).
///
/// Returns the resolved project root if any.
pub fn init_local(target: Option<&str>, cli_libs: &[String]) -> Option<PathBuf> {
    mcc::mcc_init_no_lib();
    // Empty path → system root is auto-discovered from cwd (env or cwd/mc/ or ~/.mcode/).
    mcc::mcc_set_system_root(Path::new(""));
    let project_root = find_project_root(target);
    if let Some(root) = project_root.as_deref() {
        mcc::mcc_set_project_root(root);
    }
    load_libs(&collect_libs(project_root.as_deref(), cli_libs));
    project_root
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_manifest__parse_project_toml() {
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
    fn cli_manifest__generate_default_manifest() {
        let s = Manifest::generate_default("test_proj", "src/main.mc");
        assert!(s.contains("name = \"test_proj\""));
        assert!(s.contains("entry = \"src/main.mc\""));
        assert!(s.contains("mcode = \"*\""));
    }

    // ── browse-mode entry selection (§19.5 rule 3) ──

    fn temp_browse_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mcc-browse-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn cli_manifest__select_browse_entry_unique() {
        let root = temp_browse_root("unique");
        std::fs::write(root.join("main.mc"), "module main {}\n").unwrap();
        std::fs::write(root.join("lib.mc"), "component A(rs::UV.OHM) {}\n").unwrap();

        let entry = select_browse_entry(&root, None).unwrap();
        assert_eq!(entry, root.join("main.mc"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn cli_manifest__select_browse_entry_ambiguous() {
        let root = temp_browse_root("ambiguous");
        std::fs::write(root.join("a.mc"), "module main {}\n").unwrap();
        std::fs::write(root.join("b.mc"), "module main {}\n").unwrap();

        let err = select_browse_entry(&root, None).unwrap_err().to_string();
        assert!(err.contains("2 `.mc` files"), "unexpected error: {err}");
        assert!(err.contains("--entry"), "should prompt --entry: {err}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn cli_manifest__select_browse_entry_no_module_main() {
        let root = temp_browse_root("none");
        std::fs::write(root.join("lib.mc"), "component A(rs::UV.OHM) {}\n").unwrap();

        let err = select_browse_entry(&root, None).unwrap_err().to_string();
        assert!(
            err.contains("no `.mc` file declaring `module main`"),
            "unexpected error: {err}"
        );
        assert!(err.contains("--entry"), "should prompt --entry: {err}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn cli_manifest__select_browse_entry_explicit_entry() {
        let root = temp_browse_root("explicit");
        std::fs::write(root.join("main.mc"), "module main {}\n").unwrap();
        std::fs::write(root.join("other.mc"), "module main {}\n").unwrap();

        let entry = select_browse_entry(&root, Some("other.mc")).unwrap();
        assert_eq!(entry, root.join("other.mc"));

        let err = select_browse_entry(&root, Some("missing.mc"))
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("entry file not found"),
            "unexpected error: {err}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn cli_manifest__file_declares_module_main_edge_cases() {
        let root = temp_browse_root("edge");
        let cases = [
            (
                "comment.mc",
                "// module main comment\ncomponent A {}\n",
                false,
            ),
            ("main2.mc", "module main2 {}\n", false),
            ("pub_main.mc", "pub module main {\n}\n", true),
            ("braced.mc", "module main {\n}\n", true),
            ("main_only.mc", "module main\n", true),
        ];
        for (name, content, expect) in cases {
            let p = root.join(name);
            std::fs::write(&p, content).unwrap();
            assert_eq!(
                file_declares_module_main(&p),
                expect,
                "unexpected result for {name}"
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}
