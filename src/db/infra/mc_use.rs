// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use std::path::Path;

use tracing::{debug, warn};

use crate::ast::{ast_node::AstNode, c_macros::*, error::message::*};
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
    pub(crate) fn new(node: &AstNode, current_path: &Path) -> Option<McUse> {
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
                return None;
            }
        };

        //2. uri module / file
        let module_file_node = pre_fix_node.get_next().expect(MISSING_SUBNODE);
        let uri_path = match module_file_node.get_type() {
            MCAST_URI_MODULE => {
                if let Some(path_strs) = module_file_node.subs_to_string_vec() {
                    // For single-path modules like `use conn`, complete as `conn/conn` (same-name file under module directory)
                    if path_strs.len() == 1 {
                        let module_name = path_strs[0].clone();
                        format!("{module_name}/{module_name}")
                    } else {
                        path_strs.join("/")
                    }
                } else {
                    String::new()
                }
            }
            MCAST_URI_FILE => {
                // ★ Fix: handle C parser potentially splitting "power.mc" into two child nodes
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
                dlog_error(402, &module_file_node, "Invalid path in USE");
                return None;
            }
        };

        // 3. Process the next 3 nodes
        let node1 = module_file_node.get_next();
        let node2 = node1.as_ref().and_then(|n| n.get_next());
        let node3 = node2.as_ref().and_then(|n| n.get_next());

        let (uri_version, uri_asid, uri_import_ids) =
            match (node1.as_ref(), node2.as_ref(), node3.as_ref()) {
                // ---------------- Scenario 1: 3 nodes (node1 + node2 + node3) ----------------
                (Some(n1), Some(n2), Some(n3)) => {
                    let t1 = n1.get_type();
                    let t2 = n2.get_type();
                    let t3 = n3.get_type();
                    match (t1, t2, t3) {
                        (MCAST_URI_VERSION, MCAST_URI_ASID, MCAST_URI_IMPORT_IDS) => {
                            (n1.to_string(), n2.to_string(), n3.subs_to_mcids_vec())
                        }
                        _ => (None, None, None),
                    }
                }

                // ---------------- Scenario 2: 2 nodes (node1 + node2) ----------------
                (Some(n1), Some(n2), None) => {
                    let t1 = n1.get_type();
                    let t2 = n2.get_type();
                    match (t1, t2) {
                        (MCAST_URI_VERSION, MCAST_URI_ASID) => {
                            (n1.to_string(), n2.to_string(), None)
                        }
                        (MCAST_URI_ASID, MCAST_URI_IMPORT_IDS) => {
                            (None, n1.to_string(), n2.subs_to_mcids_vec())
                        }
                        (MCAST_URI_VERSION, MCAST_URI_IMPORT_IDS) => {
                            (n1.to_string(), None, n2.subs_to_mcids_vec())
                        }
                        _ => (None, None, None),
                    }
                }

                // ---------------- Scenario 3: 1 node (node1 only) ----------------
                (Some(n1), None, None) => {
                    let t1 = n1.get_type();
                    match t1 {
                        MCAST_URI_VERSION => (n1.to_string(), None, None),
                        MCAST_URI_ASID => (None, n1.to_string(), None),
                        MCAST_URI_IMPORT_IDS => (None, None, n1.subs_to_mcids_vec()),
                        _ => (None, None, None),
                    }
                }

                // ---------------- Scenario 4: 0 nodes (fallback) ----------------
                (None, None, None) => (None, None, None),

                _ => (None, None, None),
            };

        let mut mc_use = Self {
            public: node.is_type(MCAST_USE_PUB),
            prefix: uri_prefix,
            uri: uri_path,
            version: uri_version,
            as_id: uri_asid,
            impt_ids: uri_import_ids,
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

        // 4. For system libraries, prepend "mcode/" prefix
        if matches!(
            self.prefix,
            McUsePrefix::PathSystem | McUsePrefix::PathProject
        ) {
            final_filename = format!("mcode/{final_filename}");
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
                    dlog_warning(403, fnode, &format!("use target not found: {file_display}"));
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

    // pub fn from_uri(uri: &String) -> McUse {
    //     // Self {
    //     //     uri: McURI::from(uri.as_str()),
    //     //     class: None,
    //     //     public: false,
    //     // }
    //     Self {
    //         public: node.is_type(MCAST_USE_PUB),
    //         prefix: uri_prefix,
    //         path: uri_path,
    //         version: uri_version,
    //         as_id: uri_asid,
    //         impt_ids: uri_import_ids,
    //     }
    // }
}
