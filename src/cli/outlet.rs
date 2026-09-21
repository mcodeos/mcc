// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! One law for every file mcc generates without being told where to put it.
//!
//! The **outlet** of a generated intermediate is `<project-root>/build/<name>`
//! — never the source tree beside an entry, never loose in the directory the
//! command ran in, never the data root (`datadir` owns that: index, logs,
//! config and pid live with the *installation*, not the *project*). An
//! explicit `-o/--output` overrides wholesale; an env override (e.g.
//! `MC_RENDER_GOLDEN`) sits in front of the one default arm it owns.
//!
//! The project root is resolved the one way every writer shares: the nearest
//! ancestor of `start` carrying `project.toml`, `start` itself when there is
//! none anywhere up the chain. Writers hand in `start` — the source file's
//! directory when one is known, the cwd otherwise — instead of re-implementing
//! the walk.
//!
//! Every generated file goes through [`write`] (or [`ensure_parent`] plus a
//! write): the parent directory materializes on first write, so a fresh
//! checkout needs no mkdir step. Repo-checkout fixtures that must land at a
//! fixed path inside the mcc checkout itself (the `tests/golden/` guard) are
//! infra, not intermediates, and stay where they are by design.

use std::io;
use std::path::{Path, PathBuf};

/// The nearest ancestor of `start` carrying `project.toml`; `start` itself
/// when there is none anywhere up the chain.
pub fn project_root(start: &Path) -> PathBuf {
    let start = start.to_path_buf();
    let mut dir = start.clone();
    loop {
        if crate::cli::datadir::find_manifest_in(&dir).is_some() {
            return dir;
        }
        match dir.parent() {
            Some(parent) if parent != dir => dir = parent.to_path_buf(),
            _ => return start,
        }
    }
}

/// The default outlet of a generated intermediate:
/// `<project-root>/build/<name>`. `name` may carry subdirectories
/// (`baseline/render_golden.toml`).
pub fn intermediate(start: &Path, name: &str) -> PathBuf {
    project_root(start).join("build").join(name)
}

/// Create the parent directory of an outlet, so `build/…` materializes on
/// first write and an `-o` into a fresh directory works too.
pub fn ensure_parent(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    Ok(())
}

/// Create the parent, then write. Returns the path written, for reporting.
pub fn write(path: &Path, bytes: &[u8]) -> io::Result<PathBuf> {
    ensure_parent(path)?;
    std::fs::write(path, bytes)?;
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mcc-outlet-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    /// The walk stops at the nearest manifest: a file nested below a project
    /// resolves to that project, not to some directory further up.
    #[test]
    fn nearest_manifest_wins() {
        let root = scratch("nearest");
        std::fs::create_dir_all(root.join("outer/src/deep")).unwrap();
        // An outer project, so "none anywhere" is not the case under test.
        std::fs::write(root.join("outer/project.toml"), "[project]\n").unwrap();
        let deep = root.join("outer/src/deep");
        assert_eq!(project_root(&deep), root.join("outer"));
    }

    /// Without a manifest the start directory plays root — the outlet still
    /// lands under a `build/`, never beside the source.
    #[test]
    fn no_manifest_keeps_the_start_dir() {
        let root = scratch("bare");
        std::fs::create_dir_all(root.join("src")).unwrap();
        assert_eq!(project_root(&root.join("src")), root.join("src"));
        assert_eq!(
            intermediate(&root.join("src"), "circuit.html"),
            root.join("src").join("build").join("circuit.html")
        );
    }

    /// `write` materializes a multi-level default outlet on first write.
    #[test]
    fn write_creates_the_parent() {
        let root = scratch("mkdir");
        let path = intermediate(&root, "baseline/render_projection.md");
        write(&path, b"audit").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"audit");
    }
}
