// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The `data-src-uri` display form of a source position (U274; view-model
//! design §10①1).
//!
//! A source position's URI is a machine-absolute path — that is what the
//! loader canonicalizes to. Spelled raw into the drawing, it made the artifact
//! non-comparable across machines and unfit for archiving, so the stamped face
//! rewrites it against the root that owns the file. Two roots own everything
//! the schematic can point at — the project and the system library — and they
//! are the same two roots the loader searches, project first:
//!
//! - `<project-root>/src/hbl.mc`  → `src/hbl.mc`
//! - `<system-root>/mcode/cap.mc` → `mcode/cap.mc` (the `mcode/` segment
//!   survives: it names the library, exactly as the use-path does)
//!
//! A URI under neither root (an out-of-tree file) has no root to name and
//! stays absolute; an unset root rewrites nothing. Resolution mirrors this on
//! the way back: [`resolve`] tries the project root, then the system root, so
//! a stamped relative URI names the same file the absolute one did.

use std::path::{Path, PathBuf};

/// The `data-src-uri` display form of a source URI, roots read from the
/// process globals (the loader set them before any render runs).
pub fn display(uri: &str) -> String {
    let project = crate::db::infra::init::mcb_get_project_root();
    let system = crate::db::infra::init::mcb_get_system_root();
    display_with(uri, &project, &system)
}

/// [`display`] against explicit roots — the testable core.
pub fn display_with(uri: &str, project: &Path, system: &Path) -> String {
    let path = Path::new(uri);
    if !path.is_absolute() {
        // Already relative (a `use`d file recorded before canonicalization) —
        // it carries no machine and needs no rewrite.
        return uri.to_string();
    }
    if project.as_os_str().is_empty() && system.as_os_str().is_empty() {
        // No roots configured: nothing to name the file by, keep the path.
        return uri.to_string();
    }
    // URIs are canonicalized at load, but the roots may arrive through
    // symlinks (the golden fixture does), so both sides get the same
    // normalization before the prefix check.
    let norm = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let uri_n = norm(path);
    for root in [project, system] {
        if root.as_os_str().is_empty() {
            continue;
        }
        if let Some(rel) = strip_prefix_normalized(&uri_n, &norm(root)) {
            return rel;
        }
    }
    uri.to_string()
}

/// `strip_prefix` on already-normalized paths, as a forward-slash string.
fn strip_prefix_normalized(path: &Path, root: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()?
        .to_str()
        .map(|s| s.to_string())
}

/// The file a stamped URI names: absolute as-is; relative first against
/// `project_root`, then against the system root — the loader's own search
/// order, so a project file can shadow a library file of the same shape and
/// a stamped `mcode/…` URI still reaches the library.
pub fn resolve(uri: &str, project_root: &Path) -> PathBuf {
    let path = Path::new(uri);
    if path.is_absolute() {
        return path.to_path_buf();
    }
    let in_project = project_root.join(path);
    if in_project.exists() {
        return in_project;
    }
    crate::db::infra::init::mcb_get_system_root().join(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A project-root member strips to its relative form.
    #[test]
    fn project_member_strips_to_relative() {
        let dir = scratch("srcuri-proj");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let abs = dir.join("src").join("m.mc");
        std::fs::write(&abs, "").unwrap();
        let abs = abs.canonicalize().unwrap();

        let out = display_with(&abs.to_string_lossy(), &dir, Path::new(""));
        assert_eq!(out, "src/m.mc");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The system-library form keeps the `mcode/` segment: it names the
    /// library, exactly as the use-path does.
    #[test]
    fn system_member_keeps_the_library_segment() {
        let out = display_with(
            "/data-root/mcode/cap.mc",
            Path::new("/no/project"),
            Path::new("/data-root"),
        );
        assert_eq!(out, "mcode/cap.mc");
    }

    /// A path outside both roots is none of theirs to name and stays put.
    #[test]
    fn outside_both_roots_stays_absolute() {
        let out = display_with(
            "/etc/elsewhere.mc",
            Path::new("/no/project"),
            Path::new("/data-root"),
        );
        assert_eq!(out, "/etc/elsewhere.mc");
    }

    /// No roots configured rewrites nothing — the no-project paths.
    #[test]
    fn unset_roots_rewrite_nothing() {
        assert_eq!(
            display_with("/abs/m.mc", Path::new(""), Path::new("")),
            "/abs/m.mc"
        );
    }

    /// An already-relative URI passes through untouched.
    #[test]
    fn relative_uri_passes_through() {
        assert_eq!(display_with("rel.mc", Path::new("/p"), Path::new("")), "rel.mc");
    }

    /// Resolution mirrors the loader: project first, then the system root.
    #[test]
    fn resolve_prefers_project_then_system() {
        let dir = scratch("srcuri-res");
        std::fs::create_dir_all(dir.join("mcode")).unwrap();
        std::fs::write(dir.join("mcode").join("cap.mc"), "lib").unwrap();

        // Not under the project root → falls through to the system root side.
        let hit = resolve("mcode/cap.mc", Path::new("/no/such/project"));
        assert!(hit.ends_with("cap.mc"), "{}", hit.display());

        std::fs::remove_dir_all(&dir).ok();
    }
}
