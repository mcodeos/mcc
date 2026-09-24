// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::db::cmie::tables as workspace;
use crate::db::defspace::SourceDomain;
use crate::db::infra::mc_code::McCode;
use crate::McURI;
use dashmap;
use std::collections::HashSet;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use tracing::{trace, warn};

use crate::build::pass1::canonicalize_project_uri;
use crate::db::infra::init::*;
// === pub fn mcb_add(uri: &McURI) { ===
/// Load project file (single file, not recursive)
pub fn mcb_add(uri: &McURI) {
    let canonical_uri = canonicalize_project_uri(uri);

    let file_to_add = if Path::new(&canonical_uri).is_absolute() {
        canonical_uri.clone()
    } else {
        mcb_get_project_root()
            .join(&canonical_uri)
            .to_string_lossy()
            .to_string()
    };

    if let Some(mut mcfile) = McCode::new(&file_to_add, false) {
        let already_exists = workspace::WORKSPACE.mcodes.contains_key(&canonical_uri);
        if already_exists {
            // U234 tier ③: capture the old export signature before the def
            // sweep — the module parse's mark step diffs this against the
            // re-derived state, so an edit that moves no def name-span
            // marks no dependents.
            crate::db::infra::mc_code::stash_export_snapshot(&canonical_uri);
            // Re-add: drop this file's previous-generation defs BEFORE the
            // re-parse (the from_string arm's order). The old position —
            // after parse_pass1 — swept the freshly registered defs along
            // with the stale ones.
            remove_project_defs(&canonical_uri);
        }
        // U234 tier ②: purge BEFORE the re-parse. parse_pass1 below already
        // re-records this file's resolution edges (the RefDefMap insert
        // chokepoint fires during it), so a purge after the parse would wipe
        // freshly recorded edges that nothing re-records until the next full
        // rebuild. Purging first keeps the usual drop-then-rerecord order.
        workspace::WORKSPACE
            .refgraph
            .purge_file(canonical_uri.as_str());

        mcfile.parse_ast(); // step 1
        mcfile.parse_nsp(); // step 2
        mcfile.parse_pass1(); // step 3

        let binding = &workspace::WORKSPACE.mcodes;
        let entry: dashmap::Entry<'_, _, McCode> = binding.entry(canonical_uri.clone());
        match entry {
            dashmap::Entry::Occupied(mut occupied_entry) => {
                // update pass
                occupied_entry.insert(mcfile);
            }
            dashmap::Entry::Vacant(vacant_entry) => {
                vacant_entry.insert(mcfile);
            }
        }
        // §12.1 DefinitionSpace manifest: a single project file load.
        workspace::WORKSPACE
            .sources
            .insert(canonical_uri.clone(), SourceDomain::Project);
    }
    // T6-②: a single disk-file load round is self-contained (parse_pass1
    // derives modules too) — stamp one journal version when it changed the
    // definition space.
    workspace::WORKSPACE.registry().checkpoint_if_changed();
}

// === pub fn mcb_add_from_string(uri: &McURI, content: &str) { ===
/// Load file from memory string (no disk dependency)
/// uri is virtual path (e.g., /mcc/s01/file.mc), content is .mc file content
/// Note: caller must set log flags via `mcc_reset()` before calling
pub fn mcb_add_from_string(uri: &McURI, content: &str) {
    let canonical_uri = canonicalize_project_uri(uri);
    tracing::info!(target: "mcc::lsp", "mcb_add_from_string: uri={:?} -> canonical={:?}", uri, canonical_uri);

    if let Some(mut mcfile) = McCode::new_from_string(&canonical_uri, content) {
        let already_exists = {
            let binding = &workspace::WORKSPACE.mcodes;
            binding.contains_key(&canonical_uri)
        };
        tracing::info!(target: "mcc::lsp", "mcb_add_from_string: already_exists={}", already_exists);
        if already_exists {
            // U234 tier ③: capture the old export signature before the def
            // sweep — the module parse's mark step diffs this against the
            // re-derived state, so an edit that moves no def name-span marks
            // no dependents. Insert-if-absent: the driver loop's own stash
            // for this uri will not clobber it.
            crate::db::infra::mc_code::stash_export_snapshot(&canonical_uri);
            remove_project_defs(&canonical_uri);
            // U234: the re-parse invalidates this file's resolution edges —
            // drop them before the new pass re-records.
            workspace::WORKSPACE
                .refgraph
                .purge_file(canonical_uri.as_str());
            // Also clear diagnostics for this file
            workspace::WORKSPACE
                .diagnostics
                .lock()
                .unwrap()
                .clear_file(&canonical_uri);
            tracing::info!(target: "mcc::lsp", "mcb_add_from_string: cleared diagnostics for {}", canonical_uri);
        }

        mcfile.parse_ast_from_string(content);
        mcfile.parse_nsp();
        // Recursively load the on-disk `use` dependencies so definitions from
        // other files (components, modules, enums) are present in the
        // definition space — without this, LSP cross-file goto-def /
        // references can never resolve a class in a sibling file (mirrors
        // mcb_add_recursive's deps-first contract, so this file's pass1 below
        // can look up dependency definitions). The current file is registered
        // first so dependency cycles terminate; re-loads skip files whose
        // pass1 is already complete.
        {
            let binding = &workspace::WORKSPACE.mcodes;
            binding.insert(canonical_uri.clone(), mcfile.clone());
            let mut loaded = HashSet::new();
            loaded.insert(canonical_uri.clone());
            let deps: Vec<McURI> = mcfile
                .uselist
                .iter()
                .map(|u| canonicalize_project_uri(&u.uri))
                .collect();
            for dep_uri in deps {
                mcb_add_recursive(&dep_uri, &mut loaded, false);
            }
        }
        mcfile.parse_pass1_types();
        // Module parsing is owned by mcb_parse_all_modules() (which every
        // caller of mcb_add_from_string invokes right after). Keeping the
        // module parse out of this file-level pass preserves the invariant
        // `modules_parsed == false` for files freshly parsed in this round,
        // so mcb_parse_all_modules' stale-diagnostic sweep does not wipe this
        // file's fresh parser/use-stage diagnostics before the topo loop
        // re-derives its modules and lapper.

        let binding = &workspace::WORKSPACE.mcodes;
        if already_exists {
            binding.insert(canonical_uri.clone(), mcfile);
        } else {
            binding.insert(canonical_uri.clone(), mcfile);
        }
        // §12.1 DefinitionSpace manifest: an in-memory project source load.
        workspace::WORKSPACE
            .sources
            .insert(canonical_uri.clone(), SourceDomain::Project);
        tracing::info!(target: "mcc::lsp", "mcb_add_from_string: added to workspace, keys count={}, all_keys={:?}",
            binding.len(), binding.iter().map(|e| e.key().clone()).collect::<Vec<_>>());
    } else {
        tracing::warn!(target: "mcc::lsp", "mcb_add_from_string: McCode::new_from_string returned None");
    }
}

// === System-library loading progress (interactive terminals only) ===

/// Library name currently being loaded (set by `mcb_load_lib`).
pub(crate) static CURRENT_LIB_NAME: Mutex<Option<String>> = Mutex::new(None);

/// Number of files parsed so far in the current system-library load.
static LIB_FILES_PARSED: AtomicUsize = AtomicUsize::new(0);

/// Characters written by the last progress line, for precise line clearing.
static LAST_PROGRESS_LEN: AtomicUsize = AtomicUsize::new(0);

/// Set (or clear) the library name reported by the loading progress line.
pub(crate) fn set_current_lib(name: Option<String>) {
    *CURRENT_LIB_NAME.lock().unwrap() = name;
    LIB_FILES_PARSED.store(0, Ordering::Relaxed);
    LAST_PROGRESS_LEN.store(0, Ordering::Relaxed);
}

/// Print a single self-overwriting progress line to stderr while a system
/// library is parsed file by file. A carriage return keeps everything on one
/// line; each new file overwrites the previous one. Active only on
/// interactive terminals so piped / CI / JSON-RPC output stays clean.
pub(crate) fn print_lib_progress(path: &str) {
    if !std::io::stderr().is_terminal() {
        return;
    }
    let n = LIB_FILES_PARSED.fetch_add(1, Ordering::Relaxed) + 1;
    let name = CURRENT_LIB_NAME
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(|| "lib".to_string());
    // Keep the line readable: show the path tail (ASCII-safe char slicing).
    let shown = if path.chars().count() > 60 {
        let tail: String = path.chars().skip(path.chars().count() - 60).collect();
        format!("...{tail}")
    } else {
        path.to_string()
    };
    let line = format!("loading lib {name}: {n} {shown} ...");
    let len = line.chars().count();
    eprint!("\r{line}");
    let _ = std::io::stderr().flush();
    LAST_PROGRESS_LEN.store(len, Ordering::Relaxed);
}

/// Clear the progress line (overwrite with spaces) once the library load ends.
pub(crate) fn clear_lib_progress() {
    if !std::io::stderr().is_terminal() {
        return;
    }
    let len = LAST_PROGRESS_LEN.load(Ordering::Relaxed);
    if len > 0 {
        eprint!("\r{}\r", " ".repeat(len));
        let _ = std::io::stderr().flush();
    }
}

// === pub fn mcb_add_recursive(uri: &McURI, loaded: &mut HashSet<String>, is_system_li ===
/// Recursively load project files and all their dependencies
///
/// Starting from entry file, parse use statements, recursively load all dependency files,
/// ensure dependency files complete pass1 parsing before being referenced.
///
/// # Parameters
/// - `uri`: Entry file URI (relative to project root)
///
/// # Example
/// ```ignore
/// let mut loaded = HashSet::new();
/// mcb_add_recursive(&"main.mc".to_string(), &mut loaded);
/// ```
pub fn mcb_add_recursive(uri: &McURI, loaded: &mut HashSet<String>, is_system_lib: bool) {
    // 1. Normalize path, avoid duplicate loading
    let canonical_uri = canonicalize_project_uri(uri);
    trace!(target: "mcc::builder", uri = %uri, canonical = %canonical_uri, is_system_lib, "load: enter");

    if loaded.contains(&canonical_uri) {
        trace!(target: "mcc::builder", canonical = %canonical_uri, "load: skip (already loaded)");
        return;
    }

    // Optimization: a file whose types are already registered in the workspace
    // (pass1_complete) needs no disk re-read — unless the file changed on disk
    // since it was read. An entry with a disk mtime is compared against the
    // current mtime: unchanged keeps the fast path (repeated server-side
    // load_project calls stay cheap and synthetic VIRT_* modules survive);
    // changed falls through to the disk re-read below so external edits (an
    // agent writing files directly, or the user in an editor) are picked up
    // instead of serving stale symbol tables. An entry with no disk mtime came
    // from mcb_add_from_string: its in-memory content is authoritative (the
    // LSP push flow), so disk never wins for it. Re-reading replaces the
    // in-memory entry with fresh disk state, wiping any synthetic virtual
    // modules (VIRT_*) installed since load and resetting modules_parsed —
    // which forces mcb_parse_all_modules to re-derive the whole workspace.
    // That cost is only paid for files that actually changed. A workspace
    // entry with pass1_complete == false means an earlier load aborted
    // mid-parse — fall through and re-load from disk either way.
    if let Some(entry) = workspace::WORKSPACE.mcodes.get(&canonical_uri) {
        if entry.pass1_complete {
            match entry.disk_mtime {
                None => {
                    trace!(target: "mcc::builder", canonical = %canonical_uri, "load: skip (in-memory entry is authoritative)");
                    return;
                }
                Some(stamped) => {
                    let unchanged = std::fs::metadata(&canonical_uri)
                        .and_then(|m| m.modified())
                        .map(|t| t == stamped)
                        .unwrap_or(false);
                    if unchanged {
                        trace!(target: "mcc::builder", canonical = %canonical_uri, "load: skip (already in workspace, disk unchanged)");
                        return;
                    }
                    trace!(target: "mcc::builder", canonical = %canonical_uri, "load: disk mtime moved, re-reading");
                }
            }
        }
    }

    // 2. Construct full file path
    let file_path = if Path::new(&canonical_uri).is_absolute() {
        PathBuf::from(&canonical_uri)
    } else {
        mcb_get_project_root().join(&canonical_uri)
    };

    let file_str = match file_path.to_str() {
        Some(s) => s.to_string(),
        None => {
            warn!(target: "mcc::builder", path = ?file_path, "load: non-utf8 path, skip");
            return;
        }
    };

    // Single-line progress on interactive terminals while a system library
    // (e.g. mcode) is parsed file by file.
    if is_system_lib {
        print_lib_progress(&file_str);
    }

    // 3. Create and parse file
    let mut mcfile = match McCode::new(&file_str, is_system_lib) {
        Some(f) => f,
        None => {
            warn!(target: "mcc::builder", file = %file_str, "load: McCode::new failed");
            return;
        }
    };

    // 4. Parse AST
    trace!(target: "mcc::builder", file = %file_str, "load: parse_ast");
    mcfile.parse_ast();

    // 5. Collect direct uses (cheap scan, no recursive traversal).
    //    This populates uselist so we know which dependencies to recurse into.
    let current_path = match PathBuf::from(&file_str).parent() {
        Some(p) => p.to_path_buf(),
        None => {
            warn!(target: "mcc::builder", file = %file_str, "load: cannot get parent path");
            return;
        }
    };
    mcfile.uselist = mcfile.collect_direct_uses(&current_path);

    // 5.5. First insert file into prj_mcodes (so dependency cycle detection works,
    //      and when parse_pass1_types() calls mcb_get_cmie to lookup Interface,
    //      it can find current file's spacenames in prj_mcodes).
    //      Note: spacenames are empty at this point — they will be computed
    //      in step 8 after all dependencies are loaded.
    workspace::WORKSPACE
        .mcodes
        .insert(canonical_uri.clone(), mcfile.clone());

    // §12.1 DefinitionSpace manifest: record which domain this source was
    // loaded into — project, or a system library (named by the loader's
    // current-lib context, set by `mcb_load_lib`).
    let domain = if is_system_lib {
        SourceDomain::SystemLib(
            CURRENT_LIB_NAME
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| "mcode".to_string()),
        )
    } else {
        SourceDomain::Project
    };
    workspace::WORKSPACE
        .sources
        .insert(canonical_uri.clone(), domain);

    // 6. Mark as loaded (before recursion to prevent circular dependencies)
    loaded.insert(canonical_uri.clone());

    // 7. Recursively load all dependencies FIRST.
    //    This ensures dependencies' spacenames are computed before we
    //    compute the current file's spacenames.
    let deps: Vec<McURI> = mcfile.uselist.iter().map(|u| u.uri.clone()).collect();
    if !deps.is_empty() {
        trace!(target: "mcc::builder", file = %file_str, deps = deps.len(), "load: recurse into deps");
    }

    for dep_uri in deps {
        mcb_add_recursive(&dep_uri, loaded, is_system_lib);
    }

    // 8. After all dependencies are loaded, compute this file's spacenames
    //    using the dependencies' already-resolved spacenames from the workspace.
    //    This is a non-recursive lookup — unlike the old parse_nsp() which
    //    re-traversed the entire use graph independently (Defect 12).
    trace!(target: "mcc::builder", file = %file_str, "load: parse_nsp_from_deps");
    mcfile.parse_nsp_from_deps();

    // 9. After all dependencies are loaded, parse this file's CMIE definitions
    // Check pass1_complete flag to determine if parsing is needed
    let need_parse = !mcfile.pass1_complete;
    if need_parse {
        trace!(target: "mcc::builder", file = %file_str, "load: parse_pass1_types");
        crate::current_uri::set(&canonical_uri);
        remove_project_defs(&canonical_uri);
        mcfile.parse_pass1_types();
        // Update spacenames in prj_mcodes
        workspace::WORKSPACE
            .mcodes
            .entry(canonical_uri.clone())
            .and_modify(|entry| entry.spacenames.clone_from(&mcfile.spacenames));
    }
    // Note: the symbol lapper is not built here — parse_pass1_types only
    // registers CMIE definitions. Module parsing and create_lapper happen in
    // mcb_parse_all_modules(), which every project-load entry point calls.
    trace!(
        target: "mcc::builder",
        file = %file_str,
        "load: done"
    );

    // 10. Update project file table (replace pre-inserted empty file with parsed file)
    if let dashmap::Entry::Occupied(mut occupied_entry) =
        workspace::WORKSPACE.mcodes.entry(canonical_uri.clone())
    {
        occupied_entry.insert(mcfile);
    }
}

// === pub struct BuildEntry / pub fn discover_entries ===

/// One definition space in a directory batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildEntry {
    /// Identity of the definition space — the value
    /// [`world_key`](crate::db::cmie::tables) keys a world by.
    ///
    /// For a project it is the directory the manifest names. For a loose `.mc`
    /// file it is the file itself: two unrelated files in one folder must be
    /// two worlds, and a world keyed by their shared parent would make the
    /// second file's world "already active" and silently merge the two.
    pub world: PathBuf,
    /// The base this entry's relative paths resolve against, handed to
    /// `mcc_set_project_root`. Always a **directory**.
    ///
    /// Never the entry file: a file there breaks `./`-relative `use` resolution
    /// (`canonicalize_project_uri`, `McUsePrefix::PathProject`) and the
    /// `symbols/manifest.toml` lookup, both of which join onto the project root
    /// and would silently find nothing.
    pub scope: PathBuf,
    /// The `.mc` file this world is built from.
    pub entry: PathBuf,
}

/// Resolve a directory into the definition spaces beneath it.
///
/// A directory is a **container**, not a definition space. What it holds is
/// *entries*: a subdirectory with a manifest names one, and — anywhere no
/// manifest names one — each `.mc` file is one. A directory's name or depth
/// decides nothing, so a folder of unrelated examples and a folder of nested
/// projects are both read correctly, and no two entries ever share tables.
///
/// The named root is resolved *upwards* first ([`Manifest::nearest_root`]), so
/// `mcc check <project>/src` is the project it sits in rather than a folder of
/// loose files, and `mcc check <project>` is unchanged.
///
/// `entry_override` (the CLI's `--entry`) names exactly one entry and replaces
/// the walk — it is a deliberate restriction, not an extra entry.
pub fn discover_entries(root: &Path, entry_override: Option<&str>) -> Vec<BuildEntry> {
    let root = absolute(root);

    if let Some(rel) = entry_override {
        let entry = absolute(&root.join(rel));
        let (world, scope) = match crate::cli::manifest::Manifest::nearest_root(&root) {
            Some(project) => (project.clone(), project),
            None => (entry.clone(), root),
        };
        return vec![BuildEntry {
            world,
            scope,
            entry,
        }];
    }

    // Inside a project, the whole named subtree is that one project.
    if let Some(project) = crate::cli::manifest::Manifest::nearest_root(&root) {
        let manifest = crate::cli::manifest::Manifest::find_and_load(&project);
        if let Some(entry) = manifest.map(|m| m.entry_path(&project)) {
            return vec![BuildEntry {
                world: project.clone(),
                scope: project,
                entry,
            }];
        }
    }

    let mut out = Vec::new();
    walk_entries(&root, &root, &mut out);
    // Deterministic order across the whole batch, not just within a directory:
    // which entry's tree rides into the report, and which world is left active,
    // both follow this order.
    out.sort_by(|a, b| a.world.cmp(&b.world));
    out
}

/// Collect the entries under `dir`. `scope` is the directory relative paths
/// resolve against — the walk's own root, unless a nested manifest overrides it
/// with itself.
fn walk_entries(dir: &Path, scope: &Path, out: &mut Vec<BuildEntry>) {
    // A manifest names this directory's entry — and names only one: the
    // project's own `.mc` files belong to that entry's `use` closure, so the
    // walk stops here rather than making each of them a world of its own.
    if let Some(m) = crate::cli::manifest::Manifest::find_and_load(dir) {
        out.push(BuildEntry {
            world: dir.to_path_buf(),
            scope: dir.to_path_buf(),
            entry: m.entry_path(dir),
        });
        return;
    }

    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut children: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    children.sort();
    for path in children {
        if path.is_dir() {
            if path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
            {
                continue;
            }
            walk_entries(&path, scope, out);
        } else if path.extension().is_some_and(|ext| ext == "mc") {
            out.push(BuildEntry {
                world: path.clone(),
                scope: scope.to_path_buf(),
                entry: path,
            });
        }
    }
}

/// `path` made absolute against the process cwd, then canonicalized where it
/// exists.
///
/// The resolver's outputs are joined onto other paths by code that assumes they
/// are absolute — a relative one resolves against the *project root* instead of
/// the cwd, which is how `mcc check .` would look for `./x/y.mc` under
/// `./x/y.mc/./x/y.mc`. Canonicalizing also gives one spelling per directory, so
/// `/var/x` and `/private/var/x` are one world and not two.
fn absolute(path: &Path) -> PathBuf {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    abs.canonicalize().unwrap_or(abs)
}

/// Recursively collect every `.mc` file under `root`, skipping hidden
/// directories (leading `.`). Sorted for deterministic order.
pub fn collect_mc_files(root: &Path) -> Vec<PathBuf> {
    fn walk(current: &Path, out: &mut Vec<PathBuf>) {
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
                    walk(&p, out);
                }
            } else if p.extension().is_some_and(|ext| ext == "mc") {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out.sort();
    out
}

// === pub fn mcb_loaded_file_count() -> usize { ===
/// Get number of loaded files
pub fn mcb_loaded_file_count() -> usize {
    workspace::WORKSPACE.mcodes.len()
}

// === pub fn mcb_print_loaded_files() { ===
/// Print list of loaded files
pub fn mcb_print_loaded_files() {
    for _entry in workspace::WORKSPACE.mcodes.iter() {}
}

// === pub fn mcb_remove(uri: &McURI) { ===
/// Unload project file
pub fn mcb_remove(uri: &McURI) {
    let canonical_uri = canonicalize_project_uri(uri);

    remove_project_defs(uri);
    if canonical_uri != *uri {
        remove_project_defs(&canonical_uri);
    }
    // U234: the removal drops the files' resolution edges from the graph
    // (both uri spellings, mirroring the double registry removal).
    workspace::WORKSPACE.refgraph.purge_file(uri.as_str());
    if canonical_uri != *uri {
        workspace::WORKSPACE.refgraph.purge_file(canonical_uri.as_str());
    }

    let binding = &workspace::WORKSPACE.mcodes;
    binding.remove(uri);
    if canonical_uri != *uri {
        binding.remove(&canonical_uri);
    }

    // §12.1 DefinitionSpace manifest: drop the source entry.
    workspace::WORKSPACE.sources.remove(uri);
    if canonical_uri != *uri {
        workspace::WORKSPACE.sources.remove(&canonical_uri);
    }

    let extra_keys: Vec<String> = binding
        .iter()
        .filter(|entry| uri_equivalent(entry.key(), uri.as_str(), &canonical_uri))
        .map(|entry| entry.key().clone())
        .collect();
    for key in extra_keys {
        binding.remove(&key);
    }
    // T6-②: file-remove round end — stamp one journal version when the
    // removal tombstoned any definition (design §10: each load/change).
    workspace::WORKSPACE.registry().checkpoint_if_changed();
}

// === fn remove_project_defs(uri: &McURI) { ===
/// Remove every definition this project file registered, from both physical
/// tables and the registry's project layer — delegated to the single write
/// entry (defregistry.rs). T8 (M2): the project layer is tombstoned while a
/// live same-key system-lib def the file was shadowing survives as the read
/// fallback (deleting a project source file never destroys mcode data).
/// (Named for its old define-only scope; since b3953 the `define` keyword is
/// gone and this removes all project def kinds.)
pub(crate) fn remove_project_defs(uri: &McURI) {
    workspace::WORKSPACE.remove_project_defs_by_uri(uri.as_str());
}
