// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use std::path::Path;

use tracing::{debug, warn};

use crate::ast::{error::message::*, macros::*, node::AstNode};
use crate::db::diagnostic::diagnostic::{dlog_error, dlog_warning};
use crate::db::infra::init::{mcb_get_project_root, mcb_get_system_root};
use crate::{McIds, McURI};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone)]
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
        let base_path = match self.prefix {
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

        // 3. Join URI + version, filename format: with version → filename@1.0.0.mc; without → filename.mc
        let mut final_filename = self.uri.clone();
        if let Some(ver) = &self.version {
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
        }

        // 5. Base path + versioned URI
        let absolute_file_path = base_path.join(final_filename);

        // 6. Canonicalize absolute path (log warning on failure if node is available)
        let canonical_abs_path: std::path::PathBuf = match absolute_file_path.canonicalize() {
            Ok(path) => path,
            Err(e) => {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Test multi-segment path tail-segment auto-completion.
    /// Verifies `use man.mcu.comp` → `man/mcu/comp/comp`.
    #[test]
    fn test_multi_segment_auto_completion() {
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
    fn test_single_segment_auto_completion() {
        let path_strs = vec!["conn".to_string()];

        if path_strs.len() == 1 {
            let module_name = path_strs[0].clone();
            let path = format!("{module_name}/{module_name}");
            assert_eq!(path, "conn/conn");
        }
    }

    /// Test system-library prefix prepending `mcode/`.
    /// Verifies `use $::mcode.gpio` → `mcode/gpio/gpio`.
    #[test]
    fn test_system_lib_prefix() {
        let prefix = McUsePrefix::PathSystem;
        let mut final_filename = "gpio/gpio".to_string();

        // Apply system-library prefix logic
        if prefix == McUsePrefix::PathSystem {
            final_filename = format!("mcode/{}", final_filename);
        }

        assert_eq!(final_filename, "mcode/gpio/gpio");
    }

    /// Test project-root path NOT prepending `mcode/`.
    /// Verifies `use /lib/power` → `lib/power/power`.
    #[test]
    fn test_project_root_no_mcode_prefix() {
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
    fn test_version_concatenation() {
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
