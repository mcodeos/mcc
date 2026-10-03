// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc lib` — system library management
//!
//! - `mcc lib list` — list loaded + installed libraries (project and global)
//! - `mcc lib install <name> --from <path>` — vendor into `<project>/libs/`
//!   (mcode installs into the global data root as `mcode@<version>`)
//! - `mcc lib install mcode --from <path>` — global official-library install
//! - `mcc lib load <name>` — load into memory
//! - `mcc lib unload <name>` — unload from memory
//! - `mcc lib show <name>` — show library details
//! - `mcc lib search <pat>` — search installed libraries
//! - `mcc lib uninstall <name>` — remove an installed library (project copy
//!   first; `--global` removes the data-root copy)
//!
//! Layout model (cargo-like): third-party libraries are vendored into the
//! project's `libs/` directory — self-contained, git-committable,
//! environment-independent. The global data root is official-library
//! territory only (mcode, multi-version `mcode@<MAJOR.MINOR>`). Version
//! pins in `project.toml [dependencies]` select which installed copy a
//! project loads; resolution is project tier first, then global.

use crate::output;
use anyhow::{Context, Result};
use mcc::cli::{datadir, manifest::Manifest, packfile, LibAction, OutputFormat};
use serde::Serialize;
use serde_json::Value;
use std::fmt;
use std::path::{Path, PathBuf};

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
    /// Where this copy lives: `"project"` (`<root>/libs`) or `"global"`
    /// (the data root).
    pub origin: String,
    /// One-line device description, quoted from the pack's own `pack.toml`
    /// (`description` — the authority face lives with the library, never in
    /// project.toml). Absent for legacy installs not yet repacked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// The pack's one-line `description`, read from an installed copy's
/// pack.toml. Missing file/field is normal (legacy copies) → None.
fn pack_description(lib_dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(lib_dir.join("pack.toml")).ok()?;
    let pack: packfile::PackToml = toml::from_str(&text).ok()?;
    pack.package.description.filter(|d| !d.is_empty())
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
                writeln!(
                    f,
                    "  {:20} {:6} {:8} {}",
                    format!("{}@{}", lib.name, lib.version),
                    "",
                    lib.origin,
                    lib.path
                )?;
                // One-line device note, quoted from the pack face.
                if let Some(d) = &lib.description {
                    writeln!(f, "  {:20} {:6} {:8} {}", "", "", "", d)?;
                }
            }
        }
        // Same name in both tiers: resolution picks the project copy — say so.
        let mut names: Vec<&str> = self.installed.iter().map(|l| l.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        for name in names {
            let copies: Vec<&InstalledLib> = self
                .installed
                .iter()
                .filter(|l| l.name == name)
                .collect();
            if copies.iter().any(|l| l.origin == "project")
                && copies.iter().any(|l| l.origin == "global")
            {
                writeln!(
                    f,
                    "warning: '{name}' exists in both <project>/libs and the system root; \
                     the project copy wins"
                )?;
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
    // Project awareness for the in-process faces (load/show/search resolve
    // through mcc::resolve_lib_root's project tier): the lib commands do not
    // run init_local, so seed the project root from the cwd manifest walk.
    if let Some(root) = client_project_root() {
        mcc::mcc_set_project_root(&root);
    }

    let client = mcc::cli::rpcclient::RpcClient::probe();

    match action {
        LibAction::List => match &client {
            Some(c) => {
                let mut params = serde_json::json!({});
                if let Some(root) = client_project_root() {
                    params["project_root"] = serde_json::json!(root.to_string_lossy());
                }
                call_and_emit(c, "lib.list", params, format)
            }
            None => cmd_list(format),
        },
        LibAction::Install {
            name,
            from,
            version,
            global,
        } => {
            // Resolve the install root client-side: the daemon cannot see
            // this cwd, so the project tier must travel in the params.
            // .mcl archives keep the in-process face (pack/inspect precedent,
            // U90): their coordinates come from pack.toml, read locally.
            let is_mcl = from.ends_with(".mcl") && Path::new(from).is_file();
            match resolve_install_target(name.as_deref(), *global) {
                Err(e) => Err(e),
                Ok(target_root) => match (&client, is_mcl) {
                    (Some(c), false) => call_and_emit(
                        c,
                        "lib.install",
                        serde_json::json!({
                            "name": name, "from": from, "version": version,
                            "target_root": target_root.to_string_lossy(),
                        }),
                        format,
                    ),
                    _ => cmd_install(name.as_deref(), from, version.as_deref(), &target_root),
                },
            }
        }
        // pack/inspect are offline tools with no daemon face (the U90
        // precedent) — always in-process.
        LibAction::Pack { dir, out } => crate::cmds::pack::cmd_pack(dir, out.as_deref(), format),
        LibAction::Inspect { file } => crate::cmds::pack::cmd_inspect(file, format),
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
            Some(c) => {
                let mut params = serde_json::json!({ "pattern": pattern });
                if let Some(root) = client_project_root() {
                    params["project_root"] = serde_json::json!(root.to_string_lossy());
                }
                call_and_emit(c, "lib.search", params, format)
            }
            None => cmd_search(pattern, format),
        },
        LibAction::Uninstall { name, force, global } => {
            // Project-tier copy resolved client-side (cwd owner); mcode and
            // --global always target the data root.
            let project_copy = if *global || name == "mcode" {
                None
            } else {
                client_project_root().and_then(|root| {
                    find_installed_copy(&datadir::project_libs_dir(&root), name)
                })
            };
            match &client {
                Some(c) => {
                    let mut params =
                        serde_json::json!({ "name": name, "force": force });
                    if let Some(dir) = &project_copy {
                        params["target_dir"] = serde_json::json!(dir.to_string_lossy());
                    }
                    call_and_emit(c, "lib.uninstall", params, format)
                }
                None => cmd_uninstall(name, *force, project_copy, format),
            }
        }
    }
}

/// The client-side project root for the project-aware lib faces.
fn client_project_root() -> Option<PathBuf> {
    std::env::current_dir()
        .ok()
        .and_then(|cwd| Manifest::nearest_root(&cwd))
}

/// First `<name>@<ver>` directory under `root`, then a bare `<name>`.
fn find_installed_copy(root: &Path, name: &str) -> Option<PathBuf> {
    let prefix = format!("{name}@");
    if let Ok(entries) = std::fs::read_dir(root) {
        for e in entries.flatten() {
            let fname = e.file_name().to_string_lossy().to_string();
            if fname.starts_with(&prefix) && e.path().is_dir() {
                return Some(e.path());
            }
        }
    }
    let bare = root.join(name);
    bare.is_dir().then_some(bare)
}

/// Resolve the install root: mcode (and `--global`) → the data root;
/// everything else vendors into `<project>/libs` and needs a project.
fn resolve_install_target(name: Option<&str>, global: bool) -> Result<PathBuf> {
    if name == Some("mcode") {
        // The official library installs into the global data root, as a
        // versioned mcode@<MAJOR.MINOR> copy.
        return Ok(datadir::data_root());
    }
    if global {
        anyhow::bail!(
            "lib install: --global is reserved for mcode; the global data root is the \
             official library store. Third-party libraries vendor into <project>/libs — \
             run inside a project"
        );
    }
    let cwd = std::env::current_dir().context("lib install: cannot read the current directory")?;
    let root = Manifest::nearest_root(&cwd).ok_or_else(|| {
        anyhow::anyhow!(
            "lib install: no project.toml found above the current directory; run inside a \
             project (third-party libraries vendor into <project>/libs)"
        )
    })?;
    Ok(datadir::project_libs_dir(&root))
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

    // Scan libraries installed on disk: project tier first, then global.
    let installed = scan_installed_merged();

    let report = LibListReport { loaded, installed };
    output::emit(&report, format, None)
}

/// Merged install scan: `<project>/libs` (origin "project") first, then the
/// data root (origin "global"). One scanner for both tiers
/// ([`datadir::scan_lib_dir`]) so the skip rules cannot drift.
fn scan_installed_merged() -> Vec<InstalledLib> {
    let mut result = Vec::new();
    if let Some(root) = client_project_root() {
        for lib in datadir::scan_lib_dir(&datadir::project_libs_dir(&root)) {
            let description = pack_description(&lib.path);
            result.push(InstalledLib {
                name: lib.name,
                version: lib.version,
                path: lib.path.to_string_lossy().to_string(),
                origin: "project".into(),
                description,
            });
        }
    }
    for lib in datadir::scan_lib_dir(&datadir::data_root()) {
        let description = pack_description(&lib.path);
        result.push(InstalledLib {
            name: lib.name,
            version: lib.version,
            path: lib.path.to_string_lossy().to_string(),
            origin: "global".into(),
            description,
        });
    }
    result
}

// install

fn cmd_install(
    name: Option<&str>,
    from: &str,
    version: Option<&str>,
    target_root: &Path,
) -> Result<()> {
    let (lib_name_ver, target) = if from.ends_with(".mcl") && Path::new(from).is_file() {
        // An .mcl archive: identify the content by its zstd frame
        // fingerprint; the coordinates/version come from pack.toml.
        crate::cmds::pack::install_mcl_at(Path::new(from), name, target_root)?
    } else {
        let name = name.ok_or_else(|| {
            anyhow::anyhow!(
                "lib install: bare directory source requires <name> (.mcl archives carry their own)"
            )
        })?;
        do_install_at(target_root, name, from, version)?
    };
    eprintln!("✓ installed {} → {}", lib_name_ver, target.display());
    Ok(())
}

/// Pure bare-directory install: copy library dir into `target_root` as
/// `<name>@<version>`. Returns (name@version, target path). Thin delegate to
/// the lib-crate core ([`mcc::db::infra::libmgr::install_lib_at`]) so the
/// RPC `lib.install` handler and this CLI face enforce one law; .mcl
/// archives go through `cmds::pack::install_mcl_at`.
pub fn do_install_at(
    target_root: &Path,
    name: &str,
    from: &str,
    version: Option<&str>,
) -> Result<(String, PathBuf)> {
    mcc::install_lib_at(target_root, name, from, version)
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

/// Scan libraries installed on disk (search/list faces).
fn scan_installed_libs() -> Vec<InstalledLib> {
    scan_installed_merged()
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

fn cmd_uninstall(
    name: &str,
    force: bool,
    project_copy: Option<PathBuf>,
    _format: OutputFormat,
) -> Result<()> {
    let lib_dir = do_uninstall(name, force, project_copy)?;
    eprintln!("✓ uninstalled '{}' (deleted {})", name, lib_dir.display());
    Ok(())
}

/// Pure uninstall: unload if loaded (force), then delete the install dir.
/// `project_copy` (the client-resolved project-tier directory) wins when
/// present; otherwise the global data root is scanned — legacy global
/// installs and mcode live there. Returns the deleted path. Shared by CLI
/// (`mcc lib uninstall`) and RPC (`lib.uninstall`) for local/server parity.
pub fn do_uninstall(name: &str, force: bool, project_copy: Option<PathBuf>) -> Result<PathBuf> {
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

    // Resolve the library directory: project tier first, then the data root.
    let lib_dir = match project_copy {
        Some(dir) => dir,
        None => resolve_lib_uninstall_dir(name)?,
    };

    if !lib_dir.exists() {
        anyhow::bail!("lib uninstall: '{}' is not installed", name);
    }

    // Delete the directory
    std::fs::remove_dir_all(&lib_dir)
        .with_context(|| format!("lib uninstall: failed to delete {}", lib_dir.display()))?;

    // Refresh index.json (global-root face) so `lib list` no longer shows
    // the deleted install; project tiers list by scan.
    if lib_dir.starts_with(datadir::data_root()) {
        let _ = datadir::rebuild_index();
    }

    Ok(lib_dir)
}

/// Resolve the uninstall directory in the global data root: the first
/// `<name>@<ver>` entry, then the bare directory, then the legacy mcode dir.
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
