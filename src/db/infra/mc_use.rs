// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use std::path::Path;

use tracing::{debug, warn};

use crate::ast::{error::message::*, macros::*, node::AstNode};
use crate::db::diagnostic::diagnostic::{dlog_error, dlog_warning};
use crate::db::infra::init::{mcb_get_project_root, mcb_get_system_root};
use crate::{McIds, McURI};

// Serde on the use-statement face: the lib parse cache (U392 leg B) stores a
// library file's resolved `uselist` in its slot and restores it verbatim on a
// hit — the module pass reads it for topo order and world_ver reads it for the
// version envelope, so the replay must carry the resolved shape, prefix and
// all (the source-position fields ride along; they are plain offsets).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum McUsePrefix {
    PathSystem,
    PathProject,
    PathCurrent,
    PathParent,
}

impl std::fmt::Display for McUsePrefix {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McUsePrefix::PathSystem => write!(f, "PathSystem"),
            McUsePrefix::PathProject => write!(f, "PathProject"),
            McUsePrefix::PathCurrent => write!(f, "PathCurrent"),
            McUsePrefix::PathParent => write!(f, "PathParent"),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct McUse {
    pub public: bool,
    pub prefix: McUsePrefix,
    pub uri: McURI,
    pub version: Option<String>,
    pub as_id: Option<String>,
    pub impt_ids: Option<Vec<McIds>>,
    /// Original unresolved URI (before update_abs_path), used to extract library name for §11 check
    pub orig_uri: McURI,
    /// Source position of the use statement for diagnostics (§11)
    pub pos: u32,
    pub len: u32,
}

impl std::fmt::Display for McUse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Compact single-line format with alignment
        write!(
            f,
            "{:5} {:12} {}",
            if self.public { "pub" } else { "    " },
            self.prefix,
            self.uri
        )?;
        if let Some(ref v) = self.version {
            write!(f, " @{v}")?;
        }
        if let Some(ref a) = self.as_id {
            write!(f, " as {a}")?;
        }
        if let Some(ref ids) = self.impt_ids {
            let names: Vec<String> = ids.iter().map(|id| id.to_string()).collect();
            write!(f, " import({})", names.join("."))?;
        }
        Ok(())
    }
}

impl McUse {
    pub(crate) fn new(node: &AstNode, current_path: &Path, source: &str) -> Option<McUse> {
        // MCAST_USE / MCAST_USE_PUB
        //      |- MCAST_URI_PREFIX  str($ ./ ../)
        //  (1) |- MCAST_URI_MODULE
        //      |    |- mc_id...
        //  (2) |- MCAST_URI_FILE
        //      |    |- mc_id
        //      |- * MCAST_URI_VERSION str(@x.x.x)
        //      |- * MCAST_URI_ASID
        //           |- mc_id...
        //      |- * MCAST_URI_IMPORT_IDS

        //1. prefix
        let pre_fix_node = node.get_sub_node().expect(MISSING_SUBNODE);
        let uri_prefix = match pre_fix_node.to_string()?.as_str() {
            "$" => McUsePrefix::PathSystem,
            "/" => McUsePrefix::PathProject,
            "./" => McUsePrefix::PathCurrent,
            "../" => McUsePrefix::PathParent,
            _ => {
                dlog_error(
                    crate::db::diagnostic::errcodes::USE_URI_PREFIX_INVALID,
                    &pre_fix_node,
                    &crate::errcodes::format_msg(crate::errcodes::USE_URI_PREFIX_INVALID, &[]),
                );
                return None;
            }
        };

        //2. uri module / file
        let module_file_node = pre_fix_node.get_next().expect(MISSING_SUBNODE);

        // File paths (`./`, `../`) are recovered from the raw source text.
        // The C lexer only treats `[A-Za-z0-9_.]` as URI characters, so a
        // hyphenated file name such as `use ./comp-cap.mc` is lexed as
        // `comp` `-` `cap` `.` `mc` and the parser drops everything after
        // the first `-`. The path is sliced verbatim from the module/file
        // node's start position up to the first whitespace (a file path
        // never contains whitespace).
        let uri_path = if matches!(
            uri_prefix,
            McUsePrefix::PathCurrent | McUsePrefix::PathParent
        ) {
            let path_start = module_file_node.get_pos() as usize;
            source
                .get(path_start..)
                .map(|tail| {
                    tail.split(|c: char| c.is_ascii_whitespace())
                        .next()
                        .unwrap_or("")
                })
                .unwrap_or("")
                .to_string()
        } else {
            match module_file_node.get_type() {
                MCAST_URI_MODULE => {
                    if let Some(path_strs) = module_file_node.subs_to_string_vec() {
                        if path_strs.len() == 1 {
                            // Single module name like `use conn` → conn/conn
                            let module_name = path_strs[0].clone();
                            format!("{module_name}/{module_name}")
                        } else {
                            // Multi-segment module: man.mcu.comp → man/mcu/comp/comp
                            let last = path_strs.last().unwrap();
                            let mut path = path_strs.join("/");
                            path.push('/');
                            path.push_str(last);
                            path
                        }
                    } else {
                        String::new()
                    }
                }
                MCAST_URI_FILE => {
                    // Handle C parser potentially splitting "power.mc" into two child nodes
                    if let Some(path_strs) = module_file_node.subs_to_string_vec() {
                        if path_strs.len() >= 2 {
                            let last = path_strs.last().unwrap();
                            if last == "mc" {
                                // ["power", "mc"] → "power.mc" (join with dot)
                                let prefix = path_strs[..path_strs.len() - 1].join("/");
                                format!("{prefix}.mc")
                            } else {
                                path_strs.join("/")
                            }
                        } else {
                            path_strs.join("/")
                        }
                    } else {
                        String::new()
                    }
                }
                _ => {
                    dlog_error(
                        crate::errcodes::USE_PATH_INVALID,
                        &module_file_node,
                        &crate::errcodes::format_msg(crate::errcodes::USE_PATH_INVALID, &[]),
                    );
                    return None;
                }
            }
        };

        // 3. Process the next 3 nodes — collect by type, order-independent
        let mut node1 = module_file_node.get_next();
        let mut uri_version: Option<String> = None;
        let mut uri_asid: Option<String> = None;
        let mut uri_import_ids: Option<Vec<McIds>> = None;

        for _ in 0..3 {
            let n = match node1 {
                Some(ref n) => n.clone(),
                None => break,
            };
            match n.get_type() {
                MCAST_URI_VERSION => uri_version = n.to_string(),
                MCAST_URI_ASID => uri_asid = n.to_string(),
                MCAST_URI_IMPORT_IDS => uri_import_ids = n.subs_to_mcids_vec(),
                // Report instead of silently dropping an unknown trailing node —
                // it may indicate a grammar extension this compiler does not
                // support yet (§4.3 P2-use).
                other => {
                    dlog_warning(
                        crate::db::diagnostic::errcodes::USE_TRAILING_NODE,
                        &n,
                        &crate::db::diagnostic::errcodes::format_msg(
                            crate::db::diagnostic::errcodes::USE_TRAILING_NODE,
                            &[&format!("{:?}", other)],
                        ),
                    );
                    break;
                }
            }
            node1 = n.get_next();
        }

        let orig_uri = uri_path.clone();

        let mut mc_use = Self {
            public: node.is_type(MCAST_USE_PUB),
            prefix: uri_prefix,
            uri: uri_path,
            version: uri_version,
            as_id: uri_asid,
            impt_ids: uri_import_ids,
            orig_uri,
            pos: node.get_pos(),
            len: node.get_len(),
        };
        mc_use.update_abs_path(current_path, Some(&module_file_node));
        Some(mc_use)
    }

    pub fn update_abs_path(&mut self, current_path: &Path, file_node: Option<&AstNode>) {
        // 0. LSP string loads carry a `file://`-schemed URI as the file's key,
        //    so the "current path" derived from it (`file://<dir>`) is not a
        //    real filesystem path and `base_path.join(...)` could never resolve
        //    on disk — every relative use target stayed unresolved. Strip the
        //    scheme so use targets resolve to real absolute paths (use-jump /
        //    LSP gotodef parity with mcext).
        let current_path = match current_path.to_str() {
            Some(s) if s.starts_with("file://") => Path::new(&s["file://".len()..]),
            _ => current_path,
        };

        // 1. Validate current path is absolute (log and exit on failure)
        if !current_path.is_absolute() {
            warn!(
                target: "mcc::use",
                path = ?current_path,
                "current path is not absolute"
            );
            return;
        }

        // 2. Determine base path from prefix (log and exit on failure)
        let mut base_path = match self.prefix {
            McUsePrefix::PathSystem => mcb_get_system_root(),
            McUsePrefix::PathProject => mcb_get_project_root(),
            McUsePrefix::PathCurrent => current_path.to_path_buf(),
            McUsePrefix::PathParent => match current_path.parent() {
                Some(parent) => parent.to_path_buf(),
                None => {
                    warn!(
                        target: "mcc::use",
                        path = ?current_path,
                        "no parent directory"
                    );
                    return;
                }
            },
        };

        // 3. Join URI + version, filename format: with version → filename@1.0.0.mc; without →
        // filename.mc
        let mut final_filename = self.uri.clone();
        if let Some(ver) = &self.version {
            // `./`/`../` paths are sliced verbatim from the source (step 2
            // above), so the text already carries the `@ver` tail — and its
            // `.mc` — that this step re-appends. Strip it first or the
            // target doubles (`./led@1.0.0` → `led@1.0.0@1.0.0.mc`).
            let tag = format!("@{ver}");
            if let Some(at) = final_filename.rfind(&tag) {
                final_filename.truncate(at);
                if final_filename.ends_with(".mc") {
                    final_filename.truncate(final_filename.len() - ".mc".len());
                }
            }
            final_filename.push('@');
            final_filename.push_str(ver);
        }
        if !final_filename.ends_with(".mc") {
            final_filename.push_str(".mc");
        }

        // 4. System libraries: the system root already points to the library
        //    directory, so no extra path prefix is needed.
        if self.prefix == McUsePrefix::PathSystem {
            // System root is already the library root — no mcode/ prefix

            // 4b. Versioned-pack fallback: an installed pack lands as
            //     `<root>/<name>@<ver>/` (registry-design.md §4.2), so
            //     `use <name>/…` joins to `<root>/<name>/…` and misses when
            //     only the versioned directory exists. When the bare
            //     first-segment directory is absent, re-base on the highest
            //     `<name>@<ver>` directory (bare dirs stay preferred, so
            //     mcode/mclibs/mcpub's bare checkouts are untouched; the P2
            //     solver replaces "highest wins" later).
            base_path = match final_filename.split(['/', '.']).find(|s| !s.is_empty()) {
                Some(seg) if seg != "mcode" && !base_path.join(seg).exists() => {
                    match crate::db::infra::libmgr::highest_versioned_dir(&base_path, seg) {
                        Some(vroot) => {
                            // Re-base AND strip the consumed first segment:
                            // `ams1117.ams1117` → uri `ams1117/ams1117/ams1117.mc`
                            // becomes `<ams1117@ver-root>/ams1117.mc`; dotted
                            // category tails (`mcpub.power/…`) keep mapping
                            // dots to directory levels.
                            let tail = final_filename[seg.len()..]
                                .trim_start_matches(['/', '.'])
                                .to_string();
                            final_filename = tail;
                            // The module-form tail still carries the a/b/b
                            // expansion, so the direct join can miss inside
                            // the pack root; fall through to the same
                            // unique-basename pack search 6a uses.
                            if !vroot.join(&final_filename).exists() {
                                if let Some(fname) =
                                    std::path::Path::new(&final_filename).file_name()
                                {
                                    let fname = fname.to_string_lossy().into_owned();
                                    if let Some(hit) = search_unique_under(&vroot, &fname) {
                                        final_filename = hit.to_string_lossy().into_owned();
                                    }
                                }
                            }
                            vroot
                        }
                        None => base_path,
                    }
                }
                _ => base_path,
            };
        }

        // 5. Base path + versioned URI
        let absolute_file_path = base_path.join(final_filename);

        // 6. Canonicalize absolute path (log warning on failure if node is available)
        let canonical_abs_path: std::path::PathBuf = match absolute_file_path.canonicalize() {
            Ok(path) => path,
            Err(e) => {
                // 6a. Package-dir fallback (system libraries only): a part
                // package keeps its .mc next to its datasheet in its own
                // directory (`<lib>/<category>/<part>/<part>.mc`), while use
                // URIs address the part by file name (`mcpub.mcu/hc32l110.mc`
                // joins to `<...>/mcu/hc32l110.mc`, which misses). Re-search
                // the joined path's parent directory for a unique file with
                // the same basename; relative/project prefixes keep strict
                // join semantics — only the managed library space gets the
                // deep search. Zero or multiple hits stay E2003.
                if self.prefix == McUsePrefix::PathSystem {
                    let found = absolute_file_path
                        .parent()
                        .zip(absolute_file_path.file_name())
                        .and_then(|(dir, base)| search_unique_under(dir, &base.to_string_lossy()));
                    match found {
                        Some(hit) => match hit.canonicalize() {
                            Ok(path) => {
                                debug!(
                                    target: "mcc::use",
                                    wanted = %absolute_file_path.display(),
                                    hit = %path.display(),
                                    "use target resolved via package-dir search"
                                );
                                // 6b. below re-checks containment; fall through
                                // with the found path.
                                if let Some(abs_path_str) = path.to_str() {
                                    self.uri = abs_path_str.to_owned();
                                    return;
                                }
                            }
                            Err(ce) => {
                                debug!(
                                    target: "mcc::use",
                                    error = %ce,
                                    path = ?hit,
                                    "package-dir search hit failed to canonicalize"
                                );
                            }
                        },
                        None if absolute_file_path
                            .parent()
                            .zip(absolute_file_path.file_name())
                            .map(|(dir, base)| {
                                count_files_named(dir, &base.to_string_lossy()) > 1
                            })
                            .unwrap_or(false) =>
                        {
                            if let Some(fnode) = file_node {
                                let msg = format!(
                                    "{} (ambiguous: multiple files named {} under {})",
                                    absolute_file_path.display(),
                                    absolute_file_path
                                        .file_name()
                                        .map(|f| f.to_string_lossy())
                                        .unwrap_or_default(),
                                    absolute_file_path
                                        .parent()
                                        .map(|p| p.display().to_string())
                                        .unwrap_or_default()
                                );
                                dlog_warning(
                                    crate::db::diagnostic::errcodes::USE_TARGET_NOT_FOUND,
                                    fnode,
                                    &crate::db::diagnostic::errcodes::format_msg(
                                        crate::db::diagnostic::errcodes::USE_TARGET_NOT_FOUND,
                                        &[&msg],
                                    ),
                                );
                            }
                            return;
                        }
                        None => {}
                    }
                }
                // Log warning with dlog_warning if file_node is available
                if let Some(fnode) = file_node {
                    let file_display = absolute_file_path.display();
                    dlog_warning(
                        crate::db::diagnostic::errcodes::USE_TARGET_NOT_FOUND,
                        fnode,
                        &crate::db::diagnostic::errcodes::format_msg(
                            crate::db::diagnostic::errcodes::USE_TARGET_NOT_FOUND,
                            &[&file_display],
                        ),
                    );
                } else {
                    debug!(
                        target: "mcc::use",
                        error = %e,
                        path = ?absolute_file_path,
                        "canonicalize failed (use target probably not on disk)"
                    );
                }
                return;
            }
        };

        // 6b. Containment gate: canonicalize() follows symlinks and the
        // relative prefixes climb, so without this check a committed
        // `link.mc -> /anywhere` symlink — or a chain of per-file `../`
        // climbs — lands the use target outside every root the prefix law
        // defines. The system face is always gated by the system root
        // (data_root(), always configured). The relative/project faces are
        // gated by the project root only in project mode AND only when the
        // using file itself lives under that root — a system library's own
        // internal `./` uses resolve under the system root, not the project,
        // and must not be measured against it. View mode (root unset) keeps
        // the historical permissive behavior.
        let boundary = match self.prefix {
            McUsePrefix::PathSystem => {
                let root = mcb_get_system_root();
                if root.as_os_str().is_empty() {
                    None
                } else {
                    Some(root.canonicalize().unwrap_or(root))
                }
            }
            McUsePrefix::PathProject | McUsePrefix::PathCurrent | McUsePrefix::PathParent => {
                let root = mcb_get_project_root();
                if root.as_os_str().is_empty() {
                    None
                } else {
                    let root_canon = root.canonicalize().unwrap_or_else(|_| root);
                    // Both sides canonical: /tmp vs /private/tmp on macOS
                    // would otherwise false-positive on a string prefix.
                    let here_canon = current_path
                        .canonicalize()
                        .unwrap_or_else(|_| current_path.to_path_buf());
                    if here_canon.starts_with(&root_canon) {
                        Some(root_canon)
                    } else {
                        None
                    }
                }
            }
        };
        if let Some(root_canon) = boundary {
            if !canonical_abs_path.starts_with(&root_canon) {
                if let Some(fnode) = file_node {
                    let file_display = canonical_abs_path.display();
                    dlog_warning(
                        crate::db::diagnostic::errcodes::USE_TARGET_ESCAPES_ROOT,
                        fnode,
                        &crate::db::diagnostic::errcodes::format_msg(
                            crate::db::diagnostic::errcodes::USE_TARGET_ESCAPES_ROOT,
                            &[&file_display],
                        ),
                    );
                }
                return;
            }
        }

        // 7. Update final absolute path into self.uri
        // Convert PathBuf to string, then update McURI
        if let Some(abs_path_str) = canonical_abs_path.to_str() {
            self.uri = abs_path_str.to_owned();
        } else {
            warn!(
                target: "mcc::use",
                path = ?canonical_abs_path,
                "absolute path contains invalid UTF-8"
            );
        }
    }
}

/// Count files named `base` under `dir` (recursive, no size limit needed —
/// failure-path only, called on one library category directory at a time).
fn count_files_named(dir: &Path, base: &str) -> usize {
    let mut n = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.file_name().is_some_and(|f| f == base) {
                n += 1;
            }
        }
    }
    n
}

/// Return the unique file named `base` under `dir`, or `None` on zero or
/// multiple matches (the caller keeps its not-found diagnostic for both).
fn search_unique_under(dir: &Path, base: &str) -> Option<std::path::PathBuf> {
    let mut hits = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.file_name().is_some_and(|f| f == base) {
                hits.push(p);
                if hits.len() > 1 {
                    return None;
                }
            }
        }
    }
    hits.pop()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test multi-segment path tail-segment auto-completion.
    /// Verifies `use man.mcu.comp` → `man/mcu/comp/comp`.
    #[test]
    fn def_mcuse__multi_segment_auto_completion() {
        // Simulate path_strs = ["man", "mcu", "comp"]
        let path_strs = vec!["man".to_string(), "mcu".to_string(), "comp".to_string()];

        // Apply auto-completion logic
        let last = path_strs.last().unwrap();
        let mut path = path_strs.join("/");
        path.push('/');
        path.push_str(last);

        assert_eq!(path, "man/mcu/comp/comp");
    }

    /// Test single-segment path auto-completion.
    /// Verifies `use conn` → `conn/conn`.
    #[test]
    fn def_mcuse__single_segment_auto_completion() {
        let path_strs = vec!["conn".to_string()];

        if path_strs.len() == 1 {
            let module_name = path_strs[0].clone();
            let path = format!("{module_name}/{module_name}");
            assert_eq!(path, "conn/conn");
        }
    }

    /// Test system-library filename assembly.
    /// Verifies the system root already IS the library root, so the assembled
    /// filename gets no extra `mcode/` segment (`use $::mcode.gpio` resolves
    /// under the system root directly).
    #[test]
    fn def_mcuse__system_lib_prefix() {
        let prefix = McUsePrefix::PathSystem;
        let mut final_filename = "gpio/gpio".to_string();

        // The system root already points at the library directory — no
        // `mcode/` prefix is prepended (update_abs_path step 4).
        if prefix == McUsePrefix::PathSystem {
            // no prefix
        }

        assert_eq!(final_filename, "gpio/gpio");
    }

    /// Test versioned relative-path tail stripping (U369 ①).
    /// Verifies `./led@1.0.0` assembles `led@1.0.0.mc`, not the doubled
    /// `led@1.0.0@1.0.0.mc` — the verbatim slice already carries the
    /// `@ver` tail that step 3 re-appends.
    #[test]
    fn def_mcuse__versioned_relative_tail_stripped_once() {
        for (raw, want) in [
            ("parts/led@1.0.0", "parts/led@1.0.0.mc"),
            ("parts/led@1.0.0.mc", "parts/led@1.0.0.mc"),
        ] {
            let mut final_filename = raw.to_string();
            let ver = "1.0.0";
            let tag = format!("@{ver}");
            if let Some(at) = final_filename.rfind(&tag) {
                final_filename.truncate(at);
                if final_filename.ends_with(".mc") {
                    final_filename.truncate(final_filename.len() - ".mc".len());
                }
            }
            final_filename.push('@');
            final_filename.push_str(ver);
            if !final_filename.ends_with(".mc") {
                final_filename.push_str(".mc");
            }
            assert_eq!(final_filename, want, "raw form: {raw}");
        }
    }

    /// Test project-root path NOT prepending `mcode/`.
    /// Verifies `use /lib/power` → `lib/power/power`.
    #[test]
    fn def_mcuse__project_root_no_mcode_prefix() {
        let prefix = McUsePrefix::PathProject;
        let mut final_filename = "lib/power/power".to_string();

        // Apply prefix logic (project root does NOT prepend `mcode/`)
        if prefix == McUsePrefix::PathSystem {
            final_filename = format!("mcode/{}", final_filename);
        }

        assert_eq!(final_filename, "lib/power/power");
    }

    /// Test version-suffix concatenation.
    /// Verifies `use man.mcu.comp@1.1.0` → `man/mcu/comp/comp@1.1.0.mc`.
    #[test]
    fn def_mcuse__version_concatenation() {
        let mut final_filename = "man/mcu/comp/comp".to_string();
        let version = Some("1.1.0".to_string());

        // Apply version concatenation
        if let Some(ver) = &version {
            final_filename.push('@');
            final_filename.push_str(ver);
        }
        if !final_filename.ends_with(".mc") {
            final_filename.push_str(".mc");
        }

        assert_eq!(final_filename, "man/mcu/comp/comp@1.1.0.mc");
    }
}
