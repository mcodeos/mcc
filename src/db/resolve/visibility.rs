// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Per-file visibility filter (§5.4) for LSP consumers (goto-def / hover /
//! find-references): a definition is visible from a file F when it is
//! P3 (defined in F), P4 (reachable through F's use chain), or P5 (mcode
//! system library).

use crate::db::cmie::tables as workspace;
use crate::{McSpaceName, McURI};
use std::collections::HashSet;

/// Is `target_uri` reachable from `from_uri` through the transitive `use`
/// graph of loaded files (P4)?
///
/// Unlike `is_visible` — which reads the already-consolidated RefDefMap
/// `name_index` — this walks the raw `uselist`s, so it is usable at
/// class-ref *registration* time, before the referencing file's name_index
/// has been merged (consolidate_ref_def_map runs inside create_lapper, after
/// the class refs are collected).
pub(crate) fn use_chain_reaches(from_uri: &McURI, target_uri: &str) -> bool {
    use crate::build::pass1::canonicalize_project_uri;

    let target = canonicalize_project_uri(&McURI::from(target_uri));
    let mut visited: HashSet<String> = HashSet::new();
    let mut stack = vec![from_uri.clone()];
    while let Some(cur) = stack.pop() {
        let cur_c = canonicalize_project_uri(&cur);
        if !visited.insert(cur_c.clone()) {
            continue;
        }
        let uses: Vec<McURI> = match workspace::WORKSPACE.mcodes.get(&cur_c) {
            Some(mcfile) => mcfile.uselist.iter().map(|u| u.uri.clone()).collect(),
            // The file is mid-parse and temporarily absent from `mcodes`; its
            // uselist is stashed on the thread-local stack (context.rs).
            None => crate::db::infra::context::lookup_parsing_uses(&McURI::from(cur_c.as_str()))
                .map(|list| list.iter().map(|u| u.uri.clone()).collect())
                .unwrap_or_default(),
        };
        for u in uses {
            let t = canonicalize_project_uri(&u);
            if t == target {
                return true;
            }
            stack.push(McURI::from(t));
        }
    }
    false
}

/// Is `def` visible from the file `from_uri`?
///
///   P3 — def.uri == from_uri → visible.
///   P4 — from_uri's RefDefMap name_index maps the ident to def.uri → visible.
///   P5 — def exists in the global (mcode) tables → visible.
///   otherwise → not visible.
pub fn is_visible(from_uri: &McURI, def: &McSpaceName) -> bool {
    // P3: defined in the same file.
    if def.uri == *from_uri {
        return true;
    }

    // P3/P4: the file's own symbols or use-chain symbols (RefDefMap name_index).
    if let Some(mcfile) = workspace::WORKSPACE.mcodes.get(from_uri) {
        if let Ok(sym) = mcfile.symbols.lock() {
            if let Some(ref map) = sym.ref_def_map {
                if let Some(entry) = map.get_by_name(from_uri, &def.ident.to_string()) {
                    let entry_uri = crate::semantic::common::uri_of_file_id(entry.def_loc.file_id);
                    if entry_uri.as_ref() == def.uri.as_uri().as_ref() {
                        return true;
                    }
                }
            }
        }
    }

    // P5: system library (mcode or another loaded lib). Phase 5 reads the
    // registry's per-world system segment — a system def is visible exactly
    // when the current world loaded its library.
    crate::db::defregistry::system_contains(def)
}
