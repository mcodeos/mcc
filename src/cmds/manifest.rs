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

use anyhow::Result;
use std::path::{Path, PathBuf};

// The manifest itself lives in the lib (`mcc::cli::manifest`) because the batch
// entry resolver reads it from the RPC side too; re-exported here so the
// `cmds::manifest::Manifest` spelling the CLI uses stays put.
pub use mcc::cli::manifest::Manifest;

// Build flow

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

    // 2. Load unloaded dependency libraries — through the shared loading
    //    loop (use-design §19.10 D6; same shape as the RPC
    //    ensure_library_loaded handler). Registry face first
    //    (registry-design.md §4.2 — the solver is an automated lib install):
    //    when a registry is configured, the declarations are solved and any
    //    missing pack installed, and the loader pins the exact solved
    //    versions. No [registry] url → the legacy data-root loading,
    //    byte for byte.
    if let Some(ref m) = manifest {
        let pins = match mcc::cli::config::get_registry_url(Some(project_root)) {
            Some(url) => solve_deps_for_build(project_root, m, &url)?,
            None => m.dep_pins(),
        };
        let ctx = mcc::cli::loadctx::LoadContext {
            deps: pins.keys().cloned().collect(),
            pins,
            ..mcc::cli::loadctx::LoadContext::default()
        };
        for warn in mcc::cli::loadctx::load_all(&ctx) {
            eprintln!("warning: {warn}");
        }
    }

    // 3. Load project
    let entry_uri = entry.to_string_lossy().to_string();
    mcc::mcc_load_project(&entry_uri);

    // 4. Determine top module.
    //    Priority (mcd spec/16-export-viz §6): explicit top → targets in the
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

/// The build-side dependency solve (registry-design.md §4.2): solve the
/// `[dependencies]` against the registry + lock, install what is missing,
/// and return the loader pins as exact solved faces. Lock write authority:
/// written once when absent (mcode records its stdlib-rail rev — the mcc
/// batch); an existing lock is never rewritten here — a stale one warns
/// E2056 and points at `mcc lib update`.
fn solve_deps_for_build(
    project_root: &Path,
    m: &Manifest,
    url: &str,
) -> Result<std::collections::BTreeMap<String, String>> {
    use mcc::{ensure_deps_gitignore, solve_and_install, LockEntry, LockFile, RegistrySource};

    let src = RegistrySource::from_url(url).map_err(|e| anyhow::anyhow!("build: {e}"))?;
    let decls = m.solve_decls();
    let any_local = decls.values().any(|d| d.local);
    let lock = LockFile::load(project_root);
    let (out, installed) = solve_and_install(
        &src,
        // The explicit-placement law: `deps/` joins the search only when a
        // declaration opted in.
        any_local
            .then(|| project_root.join("deps"))
            .as_deref(),
        &mcc::cli::datadir::data_root(),
        &decls,
        lock.as_ref(),
    )
    .map_err(|e| anyhow::anyhow!("build: {}", solve_error_msg(&e)))?;
    if any_local {
        ensure_deps_gitignore(project_root)?;
    }
    if !installed.is_empty() {
        for face in &installed {
            eprintln!("installed {face}");
        }
    }

    // Lock write authority (§4.2): only a fresh solve writes, exactly once.
    if out.fresh {
        mcc::lock_with_mcode_rev(&out.lock).store(project_root)?;
    } else if let Some(stored) = &lock {
        if let Some(key) = mcc::first_stale_key(stored, &out.lock) {
            let msg = mcc::errcodes::format_msg(mcc::errcodes::USE_DEP_LOCK_STALE, &[&key]);
            eprintln!("warning: E{}: {msg} — run `mcc lib update`", mcc::errcodes::USE_DEP_LOCK_STALE);
        }
    }

    // The loader pins the solved faces: package → exact version (a partno
    // alias key dissolved into its package during the solve); mcode keeps
    // its manifest req on the stdlib rail.
    let mut pins: std::collections::BTreeMap<String, String> = out
        .packs
        .values()
        .map(|p| (p.package.clone(), format!("={}", p.version)))
        .collect();
    if let Some(req) = m.dep_pins().get("mcode") {
        pins.insert("mcode".to_string(), req.clone());
    }
    Ok(pins)
}

/// Solve failures surface their diagnostic code face (E2053-E2055); registry
/// transport and checksum failures have no 2xxx producer — they carry the
/// plain face and name the registry.
pub(crate) fn solve_error_msg(e: &mcc::SolveError) -> String {
    use mcc::SolveError;
    match e {
        SolveError::Unresolved { name, req } => format!(
            "E{}: {}",
            mcc::errcodes::USE_DEP_UNRESOLVED,
            mcc::errcodes::format_msg(mcc::errcodes::USE_DEP_UNRESOLVED, &[name, req])
        ),
        SolveError::Cycle { path } => {
            let p = path.join(" -> ");
            format!(
                "E{}: {}",
                mcc::errcodes::USE_DEP_CYCLE,
                mcc::errcodes::format_msg(mcc::errcodes::USE_DEP_CYCLE, &[&p])
            )
        }
        SolveError::PartnoUnavailable { partno, package, selected, since, available } => {
            let av = available.join(", ");
            format!(
                "E{}: {}",
                mcc::errcodes::USE_DEP_PARTNO_UNAVAILABLE,
                mcc::errcodes::format_msg(mcc::errcodes::USE_DEP_PARTNO_UNAVAILABLE, &[
                    partno, package, selected, since, &av
                ])
            )
        }
        SolveError::Conflict { .. }
        | SolveError::Checksum { .. }
        | SolveError::Registry(_) => e.to_string(),
    }
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
///
/// The resolution itself lives in [`mcc::cli::loadctx::resolve_load_context`]
/// (use-design §19.10 D6 phase 1); this wrapper keeps the CLI call sites
/// spelled as before.
pub fn collect_libs(project_root: Option<&Path>, cli_libs: &[String]) -> Vec<String> {
    mcc::cli::loadctx::resolve_load_context(project_root, cli_libs).lib_names()
}

/// Load exactly the given library names. No automatic global config loading.
pub fn load_libs(lib_names: &[String]) {
    mcc::cli::loadctx::load_all(&mcc::cli::loadctx::LoadContext::from_resolved(
        lib_names.to_vec(),
    ));
}

/// Walk up from `target` (a file or directory path) to find the project root:
/// the nearest ancestor directory containing `project.toml` (see
/// [`Manifest::find_in`]). Falls back to the target's own directory (or its
/// parent for a file) when nothing is found. Relative targets are resolved
/// against the current directory first, so the returned root is always
/// absolute. The walk itself is shared with the MCP server
/// ([`mcc::cli::loadctx::find_manifest_root`], use-design §19.10 D6 phase 2).
pub fn find_project_root(target: Option<&str>) -> Option<PathBuf> {
    let t = target?;
    let raw = Path::new(t);
    let p = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(raw)
    };
    mcc::cli::loadctx::find_manifest_root(&p)
}

/// Effective target of a command run without one: the current directory when
/// it holds a project manifest (`project.toml`), else `None`. Only the current
/// directory counts, never an ancestor.
pub fn effective_target(target: Option<&str>) -> Option<String> {
    if let Some(t) = target {
        return Some(t.to_string());
    }
    let cwd = std::env::current_dir().ok()?;
    Manifest::find_in(&cwd).map(|_| cwd.to_string_lossy().to_string())
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
    // The D6 shape: resolve one context, load it once
    // (use-design §19.10 convergence table, init_local row).
    let ctx = mcc::cli::loadctx::resolve_load_context(project_root.as_deref(), cli_libs);
    for warn in mcc::cli::loadctx::load_all(&ctx) {
        eprintln!("warning: {warn}");
    }
    project_root
}

// Tests

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
        assert_eq!(m.dependencies["infineon"].req(), "2.1.0");
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
