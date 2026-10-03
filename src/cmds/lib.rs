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
            here,
        } => {
            let Some(from) = from else {
                // The registry form (registry-design.md §4.5): solve
                // `[name][@ver][#partno]` against [registry] url and place the
                // pack — `--here` into <project>/deps/, otherwise the data
                // root. In-process always: it writes the local machine state
                // the daemon cannot see this cwd own (the .mcl install
                // precedent, U90), and the lock is never its to rewrite.
                if *global {
                    anyhow::bail!("lib install: --global is reserved for mcode; a registry install places the pack in the data root or, with --here, in <project>/deps");
                }
                let spec = name.as_deref().ok_or_else(|| {
                    anyhow::anyhow!(
                        "lib install: a registry install needs [name][@ver][#partno] \
                         (or --from PATH to vendor a directory/.mcl archive)"
                    )
                })?;
                let root = client_project_root().ok_or_else(|| {
                    anyhow::anyhow!(
                        "lib install: no project.toml found above the current directory; \
                         run inside a project"
                    )
                })?;
                return crate::cmds::lib::cmd_install_registry(spec, *here, &root);
            };
            // Resolve the install root client-side: the daemon cannot see
            // this cwd, so the project tier must travel in the params.
            // .mcl archives keep the in-process face (pack/inspect precedent,
            // U90): their coordinates come from pack.toml, read locally.
            if *here {
                anyhow::bail!("lib install: --here belongs to the registry form (drop --from)");
            }
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
                    _ => cmd_install(name.as_deref(), &from, version.as_deref(), &target_root),
                },
            }
        }
        // update/fetch-docs are in-process always: the lock write authority
        // and the fetched attachments are this cwd's local machine state the
        // daemon cannot own (the .mcl install precedent, U90).
        LibAction::Update { name } => {
            let root = client_project_root().ok_or_else(|| {
                anyhow::anyhow!(
                    "lib update: no project.toml found above the current directory; \
                     run inside a project"
                )
            })?;
            cmd_update(name.as_deref(), &root)
        }
        LibAction::FetchDocs { spec } => {
            let root = client_project_root().ok_or_else(|| {
                anyhow::anyhow!(
                    "lib fetch-docs: no project.toml found above the current directory; \
                     run inside a project"
                )
            })?;
            cmd_fetch_docs(spec, &root)
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
        LibAction::Search { pattern, remote, limit } => {
            if *remote {
                // The registry face runs in-process always: the daemon serves
                // this cwd's installs, not a remote catalog (the install/U90
                // precedent — local machine state must be the caller's own).
                return cmd_search_remote(pattern, *limit);
            }
            match &client {
                Some(c) => {
                    let mut params = serde_json::json!({ "pattern": pattern });
                    if let Some(root) = client_project_root() {
                        params["project_root"] = serde_json::json!(root.to_string_lossy());
                    }
                    call_and_emit(c, "lib.search", params, format)
                }
                None => cmd_search(pattern, format),
            }
        }
        LibAction::Keygen { path } => cmd_keygen(path),
        LibAction::Publish { source, go } => crate::cmds::publish::cmd_publish(source, *go),
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
pub(crate) fn client_project_root() -> Option<PathBuf> {
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

/// The registry install form (registry-design.md §4.5): solve the one
/// declaration against the configured registry and place the pack — `--here`
/// into `<project>/deps/`, otherwise the data root (law amendment 1 admits a solved
/// pack there). The lock is never this command's to rewrite; the placement
/// is recorded nowhere but the disk (U387⑩) and `deps/` lands in .gitignore.
pub fn cmd_install_registry(spec: &str, here: bool, project_root: &Path) -> Result<()> {
    use mcc::{ensure_deps_gitignore, solve_and_install, RegistrySource, SolveDecl};

    let (name, ver, partno) = parse_install_spec(spec)?;
    let url = mcc::cli::config::get_registry_url(Some(project_root)).ok_or_else(|| {
        anyhow::anyhow!(
            "lib install: no registry configured — set [registry] url in mcc.yaml or \
             [config.registry] url in project.toml"
        )
    })?;
    let src = RegistrySource::from_url(&url)
        .map_err(|e| anyhow::anyhow!("lib install: {e}"))?;
    let decl = SolveDecl {
        key: name.clone(),
        req: mcc::parse_version_req(ver.as_deref().unwrap_or("*")),
        partno,
        local: here,
    };
    let decls = std::iter::once((name.clone(), decl)).collect();
    let deps_dir = here.then(|| project_root.join("deps"));
    let (out, installed) = solve_and_install(
        &src,
        deps_dir.as_deref(),
        &mcc::cli::datadir::data_root(),
        &decls,
        None,
    )
    .map_err(|e| anyhow::anyhow!("lib install: {}", crate::cmds::manifest::solve_error_msg(&e)))?;
    if here {
        ensure_deps_gitignore(project_root)?;
    }
    for face in &installed {
        eprintln!("↓ downloaded {face}");
    }
    for p in out.packs.values() {
        let from = match p.origin {
            mcc::SolveOrigin::Project => "<project>/deps",
            mcc::SolveOrigin::DataRoot => "data root",
            mcc::SolveOrigin::Registry => "registry",
        };
        let part = p.partno.as_deref().map(|v| format!("#{v}")).unwrap_or_default();
        eprintln!("✓ {} {}@{}{part} (from {from})", p.key, p.package, p.version);
    }
    Ok(())
}

/// `[name][@ver][#partno]` — the registry install token. Punctuation splits
/// only; a partno is never guessed from the name shape.
fn parse_install_spec(spec: &str) -> Result<(String, Option<String>, Option<String>)> {
    let (body, partno) = match spec.split_once('#') {
        Some((b, p)) => (b, Some(p.to_string())),
        None => (spec, None),
    };
    let (name, ver) = match body.split_once('@') {
        Some((n, v)) => (n, Some(v.to_string())),
        None => (body, None),
    };
    if name.is_empty() {
        anyhow::bail!("lib install: empty name in `{spec}` (expected [name][@ver][#partno])");
    }
    Ok((name.to_string(), ver, partno))
}

// update

/// Re-solve `[dependencies]` and rewrite `mcode.lock` (registry-design.md
/// §4.3): the ONLY lock-rewrite authority — build writes once when absent
/// and install never writes. Without `name` the lock is dropped entirely
/// (every key re-selects fresh, yanked skipped); with one, that key's entry
/// is dropped from the lock (its subtree re-selects) and the rest stay
/// locked — the reproduction credential survives for untouched keys.
pub fn cmd_update(name: Option<&str>, project_root: &Path) -> Result<()> {
    use mcc::{solve_and_install, LockFile, RegistrySource};

    let manifest_path = mcc::cli::manifest::Manifest::find_in(project_root).ok_or_else(|| {
        anyhow::anyhow!("lib update: no project.toml in {}", project_root.display())
    })?;
    let m = mcc::cli::manifest::Manifest::load(&manifest_path)?;
    let decls = m.solve_decls();
    if let Some(key) = name {
        if !decls.contains_key(key) {
            anyhow::bail!(
                "lib update: `{key}` is not declared in [dependencies] ({})",
                decls
                    .keys()
                    .map(|k| k.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
    let url = mcc::cli::config::get_registry_url(Some(project_root)).ok_or_else(|| {
        anyhow::anyhow!(
            "lib update: no registry configured — set [registry] url in mcc.yaml or \
             [config.registry] url in project.toml"
        )
    })?;
    let src = RegistrySource::from_url(&url).map_err(|e| anyhow::anyhow!("lib update: {e}"))?;

    // The refresh face: no name → solve with no lock at all; one name → the
    // stored lock minus that key (everything else stays pinned).
    let stored = LockFile::load(project_root);
    let effective = match (name, stored) {
        (Some(key), Some(mut lock)) => {
            lock.deps.remove(key);
            Some(lock)
        }
        (Some(_), None) => None,
        (None, lock) => None,
    };
    let any_local = decls.values().any(|d| d.local);
    let (out, installed) = solve_and_install(
        &src,
        any_local.then(|| project_root.join("deps")).as_deref(),
        &mcc::cli::datadir::data_root(),
        &decls,
        effective.as_ref(),
    )
    .map_err(|e| anyhow::anyhow!("lib update: {}", crate::cmds::manifest::solve_error_msg(&e)))?;
    for face in &installed {
        eprintln!("↓ downloaded {face}");
    }

    // The rewrite: the solve's own lock face records every declared key
    // (locked entries included), so this is the whole credential, refreshed.
    let body = mcc::lock_with_mcode_rev(&out.lock);
    let path = body.store(project_root)?;
    eprintln!("✓ updated {} ({} entries)", path.display(), body.deps.len());
    Ok(())
}

// fetch-docs

/// Pull a pack's documentation attachments (registry-design.md §1.3/§7⑪).
/// Bundled attachments missing from the installed thin pack are extracted
/// from the full-tier artifact (sha256-verified per file against the
/// attachment table); linked attachments print their URL — the pointer is
/// the delivery, the content never enters the reproducibility face.
pub fn cmd_fetch_docs(spec: &str, project_root: &Path) -> Result<()> {
    use mcc::RegistrySource;

    let (name, ver) = match spec.split_once('@') {
        Some((n, v)) => (n, Some(v.to_string())),
        None => (spec, None),
    };
    if name.is_empty() {
        anyhow::bail!("lib fetch-docs: empty name in `{spec}` (expected [name][@ver])");
    }
    let url = mcc::cli::config::get_registry_url(Some(project_root)).ok_or_else(|| {
        anyhow::anyhow!(
            "lib fetch-docs: no registry configured — set [registry] url in mcc.yaml or \
             [config.registry] url in project.toml"
        )
    })?;
    let src = RegistrySource::from_url(&url).map_err(|e| anyhow::anyhow!("lib fetch-docs: {e}"))?;
    let data_root = mcc::cli::datadir::data_root();

    // Locate the installed copy (project deps/ first, then the data root);
    // absent → install the thin tier first (the solver's single fetch path).
    let deps_dir = project_root.join("deps");
    let (dir, ver) = match find_installed_pack(&deps_dir, &data_root, name, ver.as_deref()) {
        Some((dir, ver)) => (dir, ver),
        None => {
            let Some(ver) = ver else {
                anyhow::bail!(
                    "lib fetch-docs: `{name}` is not installed — pass `@<ver>` to \
                     install and fetch in one step"
                );
            };
            let decl = std::iter::once((
                name.to_string(),
                mcc::SolveDecl {
                    key: name.to_string(),
                    req: mcc::parse_version_req("*"),
                    partno: None,
                    local: false,
                },
            ))
            .collect();
            let (_, _) = mcc::solve_and_install(&src, None, &data_root, &decl, None)
                .map_err(|e| anyhow::anyhow!("lib fetch-docs: {}", crate::cmds::manifest::solve_error_msg(&e)))?;
            let dir = data_root.join(format!("{name}@{ver}"));
            if !dir.is_dir() {
                anyhow::bail!(
                    "lib fetch-docs: installed `{name}` but {} is missing",
                    dir.display()
                );
            }
            (dir, ver)
        }
    };

    // The attachment table is the material ledger: bundled rows name in-pack
    // paths, linked rows name URLs.
    let pack = mcc::cli::packfile::load(&dir)?;
    let mut linked = 0;
    for att in &pack.attachments {
        if let Some(url) = &att.url {
            linked += 1;
            let rev = att.rev.as_deref().unwrap_or("-");
            eprintln!(" linked {} {} (rev {rev}): {url}", att.kind, att.name.as_deref().unwrap_or("-"));
        }
    }

    let missing: Vec<&mcc::cli::packfile::AttachmentEntry> = pack
        .attachments
        .iter()
        .filter(|a| a.is_bundled())
        .filter(|a| !dir.join(a.path.as_deref().unwrap_or_default()).is_file())
        .collect();
    if missing.is_empty() {
        eprintln!(
            "✓ {name}@{ver}: all bundled attachments present ({} linked pointer{})",
            linked,
            if linked == 1 { "" } else { "s" }
        );
        return Ok(());
    }

    // The full tier is the attachment carrier: verify the artifact against
    // the metadata checksum, then extract only the missing rows.
    let meta = src
        .meta_json_cached(name)
        .map_err(|e| anyhow::anyhow!("lib fetch-docs: {e}"))?
        .ok_or_else(|| anyhow::anyhow!("lib fetch-docs: `{name}` is not in the registry"))?;
    let vmeta = meta.versions.get(&ver).ok_or_else(|| {
        anyhow::anyhow!("lib fetch-docs: `{name}@{ver}` is not in the registry")
    })?;
    let want = vmeta.checksum.as_deref().ok_or_else(|| {
        anyhow::anyhow!(
            "lib fetch-docs: registry metadata for `{name}@{ver}` carries no full-tier checksum"
        )
    })?;
    let archive_path = src.artifact_path(&meta.category, name, &ver, mcc::Tier::Full);
    let got = crate::cmds::pack::sha256_hex_file(&archive_path)
        .map_err(|e| anyhow::anyhow!("lib fetch-docs: {e}"))?;
    let want_hex = want.strip_prefix("sha256:").unwrap_or(want);
    // Hex compares case-normalized, then exact (the name-case law).
    if want_hex.to_ascii_lowercase() != got {
        anyhow::bail!(
            "lib fetch-docs: checksum mismatch for `{name}@{ver}`: metadata {want}, read {got}"
        );
    }
    let archive = crate::cmds::pack::read_archive(&archive_path)?;
    for att in &missing {
        let path = att.path.as_deref().unwrap_or_default();
        let bytes = archive.get(path).ok_or_else(|| {
            anyhow::anyhow!(
                "lib fetch-docs: full-tier archive of {name}@{ver} lacks `{path}` (the pack and the registry tree disagree)"
            )
        })?;
        // The attachment table's own checksum audits the extracted file.
        if let Some(sum) = &att.checksum {
            let digest = {
                use sha2::{Digest, Sha256};
                let mut h = Sha256::new();
                h.update(bytes);
                format!("sha256:{:x}", h.finalize())
            };
            if sum.to_ascii_lowercase() != digest {
                anyhow::bail!(
                    "lib fetch-docs: attachment `{path}` checksum mismatch: pack.toml {sum}, extracted {digest}"
                );
            }
        }
        let target = dir.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, bytes)?;
        eprintln!("↓ fetched {path}");
    }
    eprintln!(
        "✓ {name}@{ver}: {} attachment(s) into {}",
        missing.len(),
        dir.display()
    );
    Ok(())
}

/// The installed `<name>@<ver>` dir: `deps/` first (the declaration face
/// wins), then the data root. `ver` absent → the highest installed version
/// (the directory scan's version ordering).
fn find_installed_pack(
    deps_dir: &Path,
    data_root: &Path,
    name: &str,
    ver: Option<&str>,
) -> Option<(PathBuf, String)> {
    let pick = |root: &Path| -> Option<(PathBuf, String)> {
        match ver {
            Some(v) => {
                let d = root.join(format!("{name}@{v}"));
                d.is_dir().then(|| (d, v.to_string()))
            }
            None => {
                let mut hits: Vec<(String, PathBuf)> = std::fs::read_dir(root)
                    .ok()?
                    .filter_map(|e| e.ok())
                    .filter_map(|e| {
                        let f = e.file_name().into_string().ok()?;
                        let rest = f.strip_prefix(&format!("{name}@"))?;
                        Some((rest.to_string(), e.path()))
                    })
                    .collect();
                hits.sort_by(|a, b| mcc::version_key(&a.0).cmp(&mcc::version_key(&b.0)));
                hits.pop().map(|(v, p)| (p, v))
            }
        }
    };
    pick(deps_dir).or_else(|| pick(data_root))
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

/// `mcc lib search --remote` (registry-p3-protocol.md §4.1): read the
/// registry's `/search.json`, filter client-side on name/description
/// substring, classify each hit through its metadata signature (verified /
/// community / unclassified when the metadata is unreachable). In-process by
/// law — the daemon has no business with the network.
fn cmd_search_remote(pattern: &str, limit: usize) -> Result<()> {
    use mcc::RegistrySource;
    let project_root = client_project_root();
    let url = mcc::cli::config::get_registry_url(project_root.as_deref()).ok_or_else(|| {
        anyhow::anyhow!(
            "lib search --remote: no registry configured — set [registry] url in mcc.yaml \
             or [config.registry] url in project.toml"
        )
    })?;
    let src = RegistrySource::from_url(&url).map_err(|e| anyhow::anyhow!("lib search: {e}"))?;
    let index = src.search_index().map_err(|e| anyhow::anyhow!("lib search: {e}"))?;
    let needle = pattern.to_lowercase();
    let hits: Vec<&mcc::SearchEntry> = index
        .iter()
        .filter(|e| {
            e.name.to_lowercase().contains(&needle)
                || e.description
                    .as_deref()
                    .is_some_and(|d| d.to_lowercase().contains(&needle))
        })
        .take(limit)
        .collect();

    if hits.is_empty() {
        eprintln!("no registry matches for '{pattern}'");
        return Ok(());
    }
    let keys = mcc::trust_keys().unwrap_or_default();
    for e in &hits {
        let trust = e
            .latest
            .as_deref()
            .and_then(|_| src.meta_json_cached(&e.name).ok().flatten())
            .map(|m| {
                mcc::classify(&m, &m.versions.keys().last().cloned().unwrap_or_default(), &keys)
                    .label()
                    .to_string()
            })
            .unwrap_or_else(|| "unclassified".to_string());
        match (&e.latest, &e.description) {
            (Some(v), Some(d)) => eprintln!("{}@{} [{}] — {}", e.name, v, trust, d),
            (Some(v), None) => eprintln!("{}@{} [{}]", e.name, v, trust),
            (None, d) => eprintln!(
                "{} [{}]{}",
                e.name,
                trust,
                d.as_deref().map(|d| format!(" — {d}")).unwrap_or_default()
            ),
        }
    }
    Ok(())
}

/// `mcc lib keygen` (registry-p3-protocol.md §3): mint an Ed25519 signing key
/// (create-new — an existing seed refuses), print the keyid and the
/// trust.toml snippet the consumers paste.
fn cmd_keygen(path: &str) -> Result<()> {
    let (pubkey, keyid) =
        mcc::keygen(std::path::Path::new(path))
            .map_err(|e| anyhow::anyhow!("lib keygen: {e}"))?;
    eprintln!("✓ signing key written to {path}");
    eprintln!("  keyid: {keyid}");
    eprintln!();
    eprintln!("  # consumers add to ~/.mcode/config/trust.toml:");
    eprintln!("  [[keys]]");
    eprintln!("  keyid = \"{keyid}\"");
    eprintln!("  public = \"{pubkey}\"");
    Ok(())
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
