// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc lib` — system library management
//!
//! - `mcc lib list` — list loaded system libraries
//! - `mcc lib install <name> --from <path>` — install to data_dir/system/public/
//! - `mcc lib load <name>` — load into memory
//! - `mcc lib unload <name>` — unload from memory
//! - `mcc lib show <name>` — show library details
//! - `mcc lib search <pat>` — search installed libraries
//! - `mcc lib uninstall <name>` — remove an installed library

use crate::output;
use anyhow::{Context, Result};
use mcc::cli::{datadir, LibAction, OutputFormat};
use serde::Serialize;
use serde_json::Value;
use std::fmt;
use std::path::PathBuf;

// Report types

#[derive(Serialize)]
pub struct LibListReport {
    pub loaded: Vec<LibListEntry>,
    pub installed: Vec<InstalledLib>,
}

#[derive(Serialize)]
pub struct LibListEntry {
    pub name: String,
    pub symbols: usize,
    pub in_memory: bool,
}

#[derive(Serialize)]
pub struct InstalledLib {
    pub name: String,
    pub version: String,
    pub path: String,
}

impl fmt::Display for LibListReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Loaded libraries:")?;
        if self.loaded.is_empty() {
            writeln!(f, "  (none)")?;
        } else {
            for lib in &self.loaded {
                writeln!(f, "  {:20} {} symbols", lib.name, lib.symbols)?;
            }
        }
        if !self.installed.is_empty() {
            writeln!(f, "\nInstalled (disk):")?;
            for lib in &self.installed {
                writeln!(f, "  {}@{} → {}", lib.name, lib.version, lib.path)?;
            }
        }
        Ok(())
    }
}

#[derive(Serialize)]
pub struct LibInfoReport {
    pub name: String,
    pub modules: usize,
    pub components: usize,
    pub interfaces: usize,
    pub enums: usize,
    pub total_symbols: usize,
}

impl fmt::Display for LibInfoReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Library: {}", self.name)?;
        writeln!(f, "  modules:    {}", self.modules)?;
        writeln!(f, "  components: {}", self.components)?;
        writeln!(f, "  interfaces: {}", self.interfaces)?;
        writeln!(f, "  enums:      {}", self.enums)?;
        writeln!(f, "  total:      {}", self.total_symbols)?;
        Ok(())
    }
}

// Dispatch

/// One daemon arm: make the call, then print the payload in the requested
/// format.
///
/// These seven arms used to print pretty JSON whatever `-f` said, so `-f json`
/// and `-f yaml` were byte-identical and the same argv had one shape with a
/// daemon up and another without one (CIMP §1 U90, the face half). The payload
/// is not the local report — its `loaded` / `modules` fields mean something
/// else — so it is rendered as it is, in the format asked for.
fn call_and_emit(
    client: &mcc::cli::rpcclient::RpcClient,
    method: &str,
    params: Value,
    format: OutputFormat,
) -> Result<()> {
    let result = client.call(method, params)?;
    output::emit_payload(&result, format, None)
}

pub fn run(action: &LibAction, format: OutputFormat) -> Result<()> {
    let client = mcc::cli::rpcclient::RpcClient::probe();

    match action {
        LibAction::List => match &client {
            Some(c) => call_and_emit(c, "lib.list", serde_json::json!({}), format),
            None => cmd_list(format),
        },
        LibAction::Install {
            name,
            from,
            version,
        } => match &client {
            Some(c) => call_and_emit(
                c,
                "lib.install",
                serde_json::json!({ "name": name, "from": from, "version": version }),
                format,
            ),
            None => cmd_install(name, from, version.as_deref(), format),
        },
        LibAction::Load { name } => match &client {
            Some(c) => call_and_emit(c, "lib.load", serde_json::json!({ "name": name }), format),
            None => cmd_load(name, format),
        },
        LibAction::Unload { name } => match &client {
            Some(c) => call_and_emit(c, "lib.unload", serde_json::json!({ "name": name }), format),
            None => cmd_unload(name, format),
        },
        LibAction::Show { name } => match &client {
            Some(c) => call_and_emit(c, "lib.info", serde_json::json!({ "name": name }), format),
            None => cmd_show(name, format),
        },
        LibAction::Search { pattern } => match &client {
            Some(c) => call_and_emit(
                c,
                "lib.search",
                serde_json::json!({ "pattern": pattern }),
                format,
            ),
            None => cmd_search(pattern, format),
        },
        LibAction::Uninstall { name, force } => match &client {
            Some(c) => call_and_emit(
                c,
                "lib.uninstall",
                serde_json::json!({ "name": name, "force": force }),
                format,
            ),
            None => cmd_uninstall(name, *force, format),
        },
    }
}

// list

fn cmd_list(format: OutputFormat) -> Result<()> {
    let loaded_names = mcc::mcb_loaded_libs();
    let loaded: Vec<LibListEntry> = loaded_names
        .iter()
        .map(|name| {
            let info = mcc::mcb_lib_info(name);
            LibListEntry {
                name: name.clone(),
                symbols: info.map(|i| i.total_symbols).unwrap_or(0),
                in_memory: true,
            }
        })
        .collect();

    // Scan libraries installed on disk
    let installed = scan_installed_libs();

    let report = LibListReport { loaded, installed };
    output::emit(&report, format, None)
}

// install

fn cmd_install(name: &str, from: &str, version: Option<&str>, _format: OutputFormat) -> Result<()> {
    let (lib_name_ver, target) = do_install(name, from, version)?;
    eprintln!("✓ installed {} → {}", lib_name_ver, target.display());
    Ok(())
}

/// Pure install: copy library dir into data_root. Returns (name@version, target path).
/// Shared by CLI (`mcc lib install`) and RPC (`lib.install`) for local/server parity.
pub fn do_install(name: &str, from: &str, version: Option<&str>) -> Result<(String, PathBuf)> {
    let src = PathBuf::from(from);
    if !src.exists() {
        anyhow::bail!("lib install: source path does not exist '{}'", from);
    }

    let ver = version.unwrap_or("0.0.0");
    let lib_name_ver = format!("{}@{}", name, ver);
    // Flat layout: install into <root>/<name>@<ver>
    let target = datadir::data_root().join(&lib_name_ver);

    if target.exists() {
        anyhow::bail!(
            "lib install: {} is already installed ({}). Run `uninstall` first to reinstall.",
            lib_name_ver,
            target.display()
        );
    }

    // Copy directory
    copy_dir_recursive(&src, &target).with_context(|| {
        format!(
            "lib install: failed to copy {} → {}",
            from,
            target.display()
        )
    })?;

    // Refresh index.json so `lib list` sees the new install.
    let _ = datadir::rebuild_index();

    Ok((lib_name_ver, target))
}

// load

fn cmd_load(name: &str, _format: OutputFormat) -> Result<()> {
    // First check whether it has already been loaded
    if let Some(info) = mcc::mcb_lib_info(name) {
        eprintln!(
            "✓ '{}' already loaded ({} symbols: {} mod, {} comp, {} ifs, {} enum)",
            name,
            info.total_symbols,
            info.module_count,
            info.component_count,
            info.interface_count,
            info.enum_count
        );
        return Ok(());
    }

    // Not yet loaded, perform the load flow
    let root = resolve_lib_root(name)?;

    let ok = mcc::mcb_load_lib(name, &root);
    if !ok {
        anyhow::bail!(
            "lib load: failed to load '{}' (entry file {}/{}.mc missing?)",
            name,
            root.display(),
            name
        );
    }

    if let Some(info) = mcc::mcb_lib_info(name) {
        eprintln!(
            "✓ loaded '{}' ({} symbols: {} mod, {} comp, {} ifs, {} enum)",
            name,
            info.total_symbols,
            info.module_count,
            info.component_count,
            info.interface_count,
            info.enum_count
        );
    }
    Ok(())
}

// unload

fn cmd_unload(name: &str, _format: OutputFormat) -> Result<()> {
    let ok = mcc::mcb_unload_lib(name);
    if !ok {
        anyhow::bail!("lib unload: '{}' is not loaded", name);
    }
    eprintln!("✓ unloaded '{}'", name);
    Ok(())
}

// info

fn cmd_show(name: &str, format: OutputFormat) -> Result<()> {
    let info = mcc::mcb_lib_info(name).with_context(|| {
        format!(
            "lib show: '{}' is not loaded. Run `mcc lib load {}` first",
            name, name
        )
    })?;

    let report = LibInfoReport {
        name: info.name,
        modules: info.module_count,
        components: info.component_count,
        interfaces: info.interface_count,
        enums: info.enum_count,
        total_symbols: info.total_symbols,
    };
    output::emit(&report, format, None)
}

// Helpers

/// Resolve the on-disk root directory for a library, delegating to the single
/// library-root resolver shared with the RPC/IDE path (system root first, then
/// data_root fallback — use-design §19.5 rule 2). No per-command project
/// directory probing: the system root is always the data root.
fn resolve_lib_root(name: &str) -> Result<PathBuf> {
    mcc::resolve_lib_root(name).ok_or_else(|| {
        anyhow::anyhow!(
            "lib load: install directory for '{}' not found. Run `mcc lib install {} --from <path>` first",
            name,
            name
        )
    })
}

/// Scan third-party libraries installed on disk.
fn scan_installed_libs() -> Vec<InstalledLib> {
    let tp = datadir::data_root();
    let mut result = Vec::new();

    // mcode (always present if system dir exists)
    if datadir::mcode_dir().exists() {
        result.push(InstalledLib {
            name: "mcode".into(),
            version: "*".into(),
            path: datadir::mcode_dir().to_string_lossy().to_string(),
        });
    }

    // Skip system directories
    let system_dirs = ["logs", "config"];

    if let Ok(entries) = std::fs::read_dir(&tp) {
        for entry in entries.flatten() {
            let fname = entry.file_name().to_string_lossy().to_string();
            if entry.path().is_dir() && !system_dirs.contains(&fname.as_str()) {
                let (name, version) = if let Some(at_pos) = fname.find('@') {
                    (fname[..at_pos].to_string(), fname[at_pos + 1..].to_string())
                } else {
                    (fname, "0.0.0".into())
                };
                // Skip mcode (handled separately above)
                if name == "mcode" {
                    continue;
                }
                result.push(InstalledLib {
                    name,
                    version,
                    path: entry.path().to_string_lossy().to_string(),
                });
            }
        }
    }
    result
}

fn copy_dir_recursive(src: &PathBuf, dst: &PathBuf) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

// search

#[derive(Serialize)]
pub struct LibSearchReport {
    pub pattern: String,
    pub results: Vec<InstalledLib>,
    pub total: usize,
}

impl fmt::Display for LibSearchReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Search results for '{}':", self.pattern)?;
        if self.results.is_empty() {
            writeln!(f, "  (no matches found)")?;
        } else {
            for lib in &self.results {
                writeln!(f, "  {}@{} → {}", lib.name, lib.version, lib.path)?;
            }
        }
        writeln!(f, "\nTotal: {} result(s)", self.total)?;
        Ok(())
    }
}

fn cmd_search(pattern: &str, format: OutputFormat) -> Result<()> {
    let report = do_search(pattern);
    output::emit(&report, format, None)
}

/// Pure search: filter installed libs by name/path substring.
/// Shared by CLI (`mcc lib search`) and RPC (`lib.search`) for local/server parity.
pub fn do_search(pattern: &str) -> LibSearchReport {
    let installed = scan_installed_libs();

    let pattern_lower = pattern.to_lowercase();
    let results: Vec<InstalledLib> = installed
        .into_iter()
        .filter(|lib| {
            lib.name.to_lowercase().contains(&pattern_lower)
                || lib.path.to_lowercase().contains(&pattern_lower)
        })
        .collect();

    let total = results.len();
    LibSearchReport {
        pattern: pattern.to_string(),
        results,
        total,
    }
}

// uninstall

fn cmd_uninstall(name: &str, force: bool, _format: OutputFormat) -> Result<()> {
    let lib_dir = do_uninstall(name, force)?;
    eprintln!("✓ uninstalled '{}' (deleted {})", name, lib_dir.display());
    Ok(())
}

/// Pure uninstall: unload if loaded (force), then delete the install dir. Returns deleted path.
/// Shared by CLI (`mcc lib uninstall`) and RPC (`lib.uninstall`) for local/server parity.
pub fn do_uninstall(name: &str, force: bool) -> Result<PathBuf> {
    // First check whether it has already been loaded into memory
    let loaded = mcc::mcb_loaded_libs();
    let is_loaded = loaded.contains(&name.to_string());

    if is_loaded && !force {
        anyhow::bail!(
            "lib uninstall: '{}' is loaded in memory. Run 'mcc lib unload {}' first, or use --force to force uninstall.",
            name,
            name
        );
    }

    // If already loaded, force unload
    if is_loaded && !mcc::mcb_unload_lib(name) {
        anyhow::bail!("lib uninstall: failed to unload '{}' from memory", name);
    }

    // Resolve the library directory
    let lib_dir = resolve_lib_uninstall_dir(name)?;

    if !lib_dir.exists() {
        anyhow::bail!("lib uninstall: '{}' is not installed", name);
    }

    // Delete the directory
    std::fs::remove_dir_all(&lib_dir)
        .with_context(|| format!("lib uninstall: failed to delete {}", lib_dir.display()))?;

    // Refresh index.json so `lib list` no longer shows the deleted install.
    let _ = datadir::rebuild_index();

    Ok(lib_dir)
}

fn resolve_lib_uninstall_dir(name: &str) -> Result<PathBuf> {
    // Flat layout: scan data_root for <name>@<ver> entries.
    let root = datadir::data_root();
    if let Ok(entries) = std::fs::read_dir(&root) {
        let prefix = format!("{}@", name);
        for entry in entries.flatten() {
            let fname = entry.file_name().to_string_lossy().to_string();
            if fname.starts_with(&prefix) && entry.path().is_dir() {
                return Ok(entry.path());
            }
        }
    }

    // Also check the bare directory without @version
    let bare = root.join(name);
    if bare.exists() {
        return Ok(bare);
    }

    // Check the special mcode directory
    if name == "mcode" {
        let mcode_dir = datadir::mcode_dir();
        if mcode_dir.exists() {
            return Ok(mcode_dir);
        }
    }

    anyhow::bail!("lib uninstall: install directory for '{}' not found", name);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every daemon arm renders through one dispatch, so `-f` is read.
    ///
    /// Before the funnel each of the seven arms printed the payload as pretty
    /// JSON on its own: `-f json` and `-f yaml` came out byte-identical, and
    /// the same argv had one shape with a daemon up and another without
    /// (CIMP §1 U90, the face half). This locks the dispatch, not the payload —
    /// the payload keeps its own schema and is deliberately not translated into
    /// the local report.
    #[test]
    fn cmds_lib__daemon_payload_is_rendered_in_the_requested_format() {
        let payload = serde_json::json!({
            "loaded": [{"name": "mcode", "symbols": 3}],
            "installed": [],
        });

        let compact = output::render_payload(&payload, OutputFormat::Json).unwrap();
        let pretty = output::render_payload(&payload, OutputFormat::JsonPretty).unwrap();
        let yaml = output::render_payload(&payload, OutputFormat::Yaml).unwrap();

        assert_eq!(
            compact, r#"{"installed":[],"loaded":[{"name":"mcode","symbols":3}]}"#,
            "`-f json` must be the compact form the local arm prints"
        );
        assert_ne!(
            compact, yaml,
            "`-f yaml` answered with JSON — the format is being ignored again"
        );
        assert!(!yaml.starts_with('{'), "`-f yaml` produced JSON: {yaml}");
        assert!(
            yaml.contains("loaded:"),
            "`-f yaml` did not produce YAML: {yaml}"
        );
        assert_eq!(
            pretty,
            serde_json::to_string_pretty(&payload).unwrap(),
            "the pretty face is the one these arms printed before the funnel"
        );
    }
}
