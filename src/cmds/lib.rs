// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc lib` — system library management
//!
//! - `mcc lib list` — list loaded system libraries
//! - `mcc lib install <name> --from <path>` — install to data_dir/system/public/
//! - `mcc lib load <name>` — load into memory
//! - `mcc lib unload <name>` — unload from memory
//! - `mcc lib info <name>` — show library details

use crate::cli::{data_dir, LibAction, OutputFormat};
use crate::output;
use anyhow::{Context, Result};
use serde::Serialize;
use std::fmt;
use std::path::PathBuf;

// ============================================================================
// Report types
// ============================================================================

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

// ============================================================================
// Dispatch
// ============================================================================

pub fn run(action: &LibAction, format: OutputFormat) -> Result<()> {
    let client = crate::cli::rpc_client::RpcClient::probe();

    match action {
        LibAction::List => match &client {
            Some(c) => {
                let result = c.call("library.list", serde_json::json!({}))?;
                println!("{}", serde_json::to_string_pretty(&result)?);
                Ok(())
            }
            None => cmd_list(format),
        },
        LibAction::Install {
            name,
            from,
            version,
        } => cmd_install(name, from, version.as_deref(), format),
        LibAction::Load { name } => match &client {
            Some(c) => {
                let result = c.call("lib.load", serde_json::json!({ "name": name }))?;
                println!("{}", serde_json::to_string_pretty(&result)?);
                Ok(())
            }
            None => cmd_load(name, format),
        },
        LibAction::Unload { name } => match &client {
            Some(c) => {
                let result = c.call("lib.unload", serde_json::json!({ "name": name }))?;
                println!("{}", serde_json::to_string_pretty(&result)?);
                Ok(())
            }
            None => cmd_unload(name, format),
        },
        LibAction::Show { name } => match &client {
            Some(c) => {
                let result = c.call("library.show", serde_json::json!({ "name": name }))?;
                println!("{}", serde_json::to_string_pretty(&result)?);
                Ok(())
            }
            None => cmd_show(name, format),
        },
        LibAction::Search { pattern } => cmd_search(pattern, format),
        LibAction::Uninstall { name, force } => cmd_uninstall(name, *force, format),
    }
}

// ============================================================================
// list
// ============================================================================

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

// ============================================================================
// install
// ============================================================================

fn cmd_install(name: &str, from: &str, version: Option<&str>, _format: OutputFormat) -> Result<()> {
    let src = PathBuf::from(from);
    if !src.exists() {
        anyhow::bail!("lib install: source path does not exist '{}'", from);
    }

    let ver = version.unwrap_or("0.0.0");
    let lib_name_ver = format!("{}@{}", name, ver);
    let target = data_dir::data_root().join(&lib_name_ver);

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

    eprintln!("✓ installed {} → {}", lib_name_ver, target.display());
    Ok(())
}

// ============================================================================
// load
// ============================================================================

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

// ============================================================================
// unload
// ============================================================================

fn cmd_unload(name: &str, _format: OutputFormat) -> Result<()> {
    let ok = mcc::mcb_unload_lib(name);
    if !ok {
        anyhow::bail!("lib unload: '{}' is not loaded", name);
    }
    eprintln!("✓ unloaded '{}'", name);
    Ok(())
}

// ============================================================================
// info
// ============================================================================

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

// ============================================================================
// Helpers
// ============================================================================

/// Resolve the on-disk root directory for a library.
/// Priority: 1. mcode → system_dir/mcode  2. public → thirdparty_dir/<name>@*
fn resolve_lib_root(name: &str) -> Result<PathBuf> {
    // Built-in mcode
    if name == "mcode" {
        // Prefer the local mc/mcode/ directory (consistent with the parser)
        let local_mc = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("mc/mcode");
        if local_mc.exists() {
            return Ok(local_mc);
        }
        // Fallback: data_dir::mcode_dir()
        let p = data_dir::mcode_dir();
        if p.exists() {
            return Ok(p);
        }
        anyhow::bail!("lib load: mcode directory does not exist");
    }

    // Public: look under data_root for <name>@<any_version>
    let tp = data_dir::data_root();
    if tp.exists() {
        if let Ok(entries) = std::fs::read_dir(&tp) {
            let prefix = format!("{}@", name);
            for entry in entries.flatten() {
                let fname = entry.file_name().to_string_lossy().to_string();
                if fname.starts_with(&prefix) && entry.path().is_dir() {
                    return Ok(entry.path());
                }
            }
        }
    }

    // Also check the bare directory without @version
    let bare = tp.join(name);
    if bare.exists() {
        return Ok(bare);
    }

    anyhow::bail!(
        "lib load: install directory for '{}' not found. Run `mcc lib install {} --from <path>` first",
        name,
        name
    );
}

/// Scan third-party libraries installed on disk.
fn scan_installed_libs() -> Vec<InstalledLib> {
    let tp = data_dir::data_root();
    let mut result = Vec::new();

    // mcode (always present if system dir exists)
    if data_dir::mcode_dir().exists() {
        result.push(InstalledLib {
            name: "mcode".into(),
            version: "*".into(),
            path: data_dir::mcode_dir().to_string_lossy().to_string(),
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

// ============================================================================
// search
// ============================================================================

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
    let report = LibSearchReport {
        pattern: pattern.to_string(),
        results,
        total,
    };
    output::emit(&report, format, None)
}

// ============================================================================
// uninstall
// ============================================================================

fn cmd_uninstall(name: &str, force: bool, _format: OutputFormat) -> Result<()> {
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
    if is_loaded {
        eprintln!("Unloading '{}' from memory...", name);
        if !mcc::mcb_unload_lib(name) {
            anyhow::bail!("lib uninstall: failed to unload '{}' from memory", name);
        }
    }

    // Resolve the library directory
    let lib_dir = resolve_lib_uninstall_dir(name)?;

    if !lib_dir.exists() {
        anyhow::bail!("lib uninstall: '{}' is not installed", name);
    }

    // Delete the directory
    std::fs::remove_dir_all(&lib_dir)
        .with_context(|| format!("lib uninstall: failed to delete {}", lib_dir.display()))?;

    eprintln!("✓ uninstalled '{}' (deleted {})", name, lib_dir.display());
    Ok(())
}

fn resolve_lib_uninstall_dir(name: &str) -> Result<PathBuf> {
    let tp = data_dir::data_root();

    // Look for directories in the form <name>@<version>
    if let Ok(entries) = std::fs::read_dir(&tp) {
        let prefix = format!("{}@", name);
        for entry in entries.flatten() {
            let fname = entry.file_name().to_string_lossy().to_string();
            if fname.starts_with(&prefix) && entry.path().is_dir() {
                return Ok(entry.path());
            }
        }
    }

    // Also check the bare directory without @version
    let bare = tp.join(name);
    if bare.exists() {
        return Ok(bare);
    }

    // Check the special mcode directory
    if name == "mcode" {
        let mcode_dir = data_dir::mcode_dir();
        if mcode_dir.exists() {
            return Ok(mcode_dir);
        }
    }

    anyhow::bail!("lib uninstall: install directory for '{}' not found", name);
}
