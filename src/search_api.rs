// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Public search API (M5) — shared between `mcc search` (in the binary's
//! `cmds/search.rs`) and `defs.search` (in `rpc/handlers.rs`).
//!
//! This module is `pub mod`'d from `lib.rs` so RPC handlers (which live in
//! the library crate) can call into the search logic without reaching into
//! the binary's private `cmds` module. The CLI side reuses the same code via
//! `use crate::search_api::*` — wait, no: it cannot because the binary's
//! `cmds` is private to main.rs and cannot reach into lib.rs at compile time
//! of the binary.
//!
//! Architecture:
//! - This file (`src/search_api.rs`) is the **single source of truth** for
//!   `walk_defs`, `SearchInputs`, `SearchKind`, `SearchHit`.
//! - `cmds/search.rs` (binary-private) `use`'s the types from
//!   `crate::search_api::*` via `mcc::search_api::*` (the binary crate's
//!   extern alias for the library).
//! - `rpc/handlers.rs` (library) `use`'s them via `crate::search_api::*`.
//!
//! Re: the `extern crate self as mcc;` shim in `lib.rs` — that lets internal
//! modules write `mcc::foo` instead of `crate::foo`. The binary does NOT
//! need that shim.

use mcc::{McCMIE, McIds, McInstance, McURI};
use regex::Regex;
use serde::{Deserialize, Serialize};

/// One search hit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub kind: String, // "component" | "module" | "interface" | "enum" | "instance"
    pub name: String,
    pub uri: String,
    /// For instances: the component/module class the instance is of.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
}

/// Inputs that drive a single search pass — shared between CLI + RPC.
#[derive(Debug, Clone)]
pub struct SearchInputs {
    pub pattern: String,
    pub kind: Option<SearchKind>,
    pub regex: bool,
    pub fuzzy: bool,
    pub top: Option<String>,
    pub limit: usize,
    /// Pre-loaded libs the caller has already wired in. For local mode we
    /// re-load via `manifest::load_libs`; for RPC the server already has
    /// these in memory, so this is informational only (kept for symmetry).
    pub libs: Vec<String>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SearchKind {
    Component,
    Module,
    Interface,
    Enum,
    Instance,
}

impl SearchInputs {
    /// Convert a `SearchKind` to the lowercase kind tag string used in
    /// JSON envelopes.
    pub fn kind_str(&self) -> Option<String> {
        self.kind.map(|k| match k {
            SearchKind::Component => "component".to_string(),
            SearchKind::Module => "module".to_string(),
            SearchKind::Interface => "interface".to_string(),
            SearchKind::Enum => "enum".to_string(),
            SearchKind::Instance => "instance".to_string(),
        })
    }
}

/// Walk loaded definitions (and optionally a module's instances) and return
/// matching hits. **Single source of truth** for both the CLI path and the
/// `defs.search` / `defs.query` RPC handlers.
///
/// `filter_expr` is an optional compiled query (from `mcc::query_api`) — when
/// present, hits are retained only if they satisfy the query. Filter is
/// applied BEFORE the limit so the limit counts matching results.
pub fn walk_defs(
    inputs: &SearchInputs,
    filter_expr: Option<&mcc::query_api::Query>,
) -> anyhow::Result<Vec<SearchHit>> {
    let matcher = build_matcher(&inputs.pattern, inputs.regex, inputs.fuzzy)?;

    let mut hits = Vec::new();

    let kind = inputs.kind;
    let want = |k: SearchKind| match kind {
        None => true,
        Some(ref want) => *want == k,
    };

    // Top-level defs
    if want(SearchKind::Component) {
        for (name, uri) in mcc::mcb_iter_components() {
            if matcher(&name) {
                hits.push(SearchHit {
                    kind: "component".into(),
                    name,
                    uri,
                    class: None,
                });
            }
        }
    }
    if want(SearchKind::Module) {
        for (name, uri) in mcc::mcb_iter_modules() {
            if matcher(&name) {
                hits.push(SearchHit {
                    kind: "module".into(),
                    name,
                    uri,
                    class: None,
                });
            }
        }
    }
    if want(SearchKind::Interface) {
        for (name, uri) in mcc::mcb_iter_interfaces() {
            if matcher(&name) {
                hits.push(SearchHit {
                    kind: "interface".into(),
                    name,
                    uri,
                    class: None,
                });
            }
        }
    }
    if want(SearchKind::Enum) {
        for (name, uri) in mcc::mcb_iter_enums() {
            if matcher(&name) {
                hits.push(SearchHit {
                    kind: "enum".into(),
                    name,
                    uri,
                    class: None,
                });
            }
        }
    }

    // Optional instance drill: walk the named module's declared instances
    // (definition-side, no Pass2 needed). Only opt-in: when `kind` is None,
    // user didn't ask for instance drill — don't emit a warning just because
    // `top` is unset.
    if matches!(inputs.kind, Some(SearchKind::Instance)) {
        if let Some(top_name) = &inputs.top {
            hits.extend(walk_instances(top_name, &matcher)?);
        } else {
            tracing::warn!(
                target: "mcc::search",
                "--kind instance requires --top <NAME>; skipping instance drill"
            );
        }
    }

    // Apply query filter before sort + limit.
    if let Some(q) = filter_expr {
        let needs_attrs = mcc::query_api::needs_attrs(q);
        hits.retain(|h| {
            if needs_attrs {
                let attrs = mcc::query_api::attrs_for_def(&h.name, &h.uri, |n, u| {
                    mcc::get_def(&mcc::McIds::from(n), &mcc::McURI::from(u))
                });
                mcc::query_api::matches_definition_with_attrs(
                    q,
                    Some(&h.kind),
                    Some(&h.name),
                    h.class.as_deref(),
                    Some(&h.uri),
                    &attrs,
                )
            } else {
                mcc::query_api::matches_definition(
                    q,
                    Some(&h.kind),
                    Some(&h.name),
                    h.class.as_deref(),
                    Some(&h.uri),
                )
            }
        });
    }

    // Stable order so output is reproducible across runs.
    hits.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.name.cmp(&b.name)));

    if inputs.limit > 0 && hits.len() > inputs.limit {
        hits.truncate(inputs.limit);
    }

    Ok(hits)
}

fn walk_instances(
    top_name: &str,
    matcher: &dyn Fn(&str) -> bool,
) -> anyhow::Result<Vec<SearchHit>> {
    let Some((_, uri_str)) = mcc::mcb_iter_modules()
        .into_iter()
        .find(|(n, _)| n == top_name)
    else {
        tracing::warn!(target: "mcc::search", "top module '{}' not found", top_name);
        return Ok(Vec::new());
    };
    let uri = McURI::from(uri_str.as_str());
    let ident = McIds::from(top_name);

    let cmie = match mcc::get_def(&ident, &uri) {
        Some(c) => c,
        None => {
            tracing::warn!(target: "mcc::search", "def not found for '{}'", top_name);
            return Ok(Vec::new());
        }
    };
    let module_def = match cmie {
        McCMIE::Module(m) => m,
        _ => {
            tracing::warn!(
                target: "mcc::search",
                "'{}' is not a Module (got {})",
                top_name,
                cmie_kind_name(&cmie)
            );
            return Ok(Vec::new());
        }
    };

    let mut out = Vec::new();
    for (name, inst) in module_def.insts.iter() {
        let (_kind_tag, class) = inst_kind_class(inst);
        let name_s = name.to_string();
        if matcher(&name_s) || matcher(&class) {
            out.push(SearchHit {
                kind: "instance".into(),
                name: name_s,
                uri: uri_str.clone(),
                class: if class.is_empty() { None } else { Some(class) },
            });
        }
    }
    Ok(out)
}

fn cmie_kind_name(cmie: &McCMIE) -> &'static str {
    match cmie {
        McCMIE::Component(_) => "Component",
        McCMIE::Module(_) => "Module",
        McCMIE::Interface(_) => "Interface",
        McCMIE::Enum(_) => "Enum",
    }
}

fn inst_kind_class(inst: &McInstance) -> (String, String) {
    match inst {
        McInstance::Component(c) => ("component".into(), c.name.to_string()),
        McInstance::Module(m) => ("module".into(), m.name.to_string()),
        McInstance::Label(l) => ("label".into(), l.clone()),
        McInstance::Interface(i) => ("interface".into(), i.name.to_string()),
        McInstance::Bus(b) => ("bus".into(), b.name().to_string()),
        McInstance::BusRef { component, bus } => {
            ("busref".into(), format!("{}.{}", component, bus))
        }
        McInstance::List(l) => ("list".into(), l.name().to_string()),
    }
}

fn build_matcher(
    pattern: &str,
    regex: bool,
    fuzzy: bool,
) -> anyhow::Result<Box<dyn Fn(&str) -> bool>> {
    if regex {
        let re = Regex::new(pattern)?;
        Ok(Box::new(move |s| re.is_match(s)))
    } else if fuzzy {
        let pat = pattern.to_lowercase();
        Ok(Box::new(move |s| lev_le_2(&s.to_lowercase(), &pat)))
    } else {
        let pat = pattern.to_lowercase();
        Ok(Box::new(move |s| s.to_lowercase().contains(&pat)))
    }
}

/// Wagner–Fischer Levenshtein with a 3-row rolling buffer; early-exits when
/// any cell exceeds the bound. Returns `true` iff distance ≤ 2.
pub fn lev_le_2(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let (la, lb) = (a.len(), b.len());
    if la.abs_diff(lb) > 2 {
        return false;
    }
    let a = a.as_bytes();
    let b = b.as_bytes();

    let mut prev: Vec<u8> = (0..=lb as u8).collect();
    let mut curr = vec![0u8; lb + 1];
    let mut next = vec![0u8; lb + 1];

    for i in 1..=la {
        curr[0] = i as u8;
        let mut row_min = curr[0];
        for j in 1..=lb {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = std::cmp::min(
                std::cmp::min(prev[j] + 1, curr[j - 1] + 1),
                prev[j - 1] + cost,
            );
            if curr[j] < row_min {
                row_min = curr[j];
            }
        }
        if row_min > 2 {
            return false;
        }
        std::mem::swap(&mut prev, &mut curr);
        std::mem::swap(&mut curr, &mut next);
        let _ = curr;
    }
    prev[lb] <= 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lev_le_2_basic() {
        assert!(lev_le_2("RES", "RES"));
        assert!(lev_le_2("RES", "RES1")); // 1 insertion
        assert!(lev_le_2("RES", "RESS")); // 1 insertion
        assert!(lev_le_2("RES", "RESSX")); // 2 insertions — still ≤2
        assert!(!lev_le_2("RES", "RESSXX")); // 3 insertions — over
        assert!(!lev_le_2("MCU_F4", "RES10K")); // far apart
    }

    #[test]
    fn walk_defs_substring_no_libs() {
        let inputs = SearchInputs {
            pattern: "zz_no_such_thing_zz".into(),
            kind: None,
            regex: false,
            fuzzy: false,
            top: None,
            limit: 0,
            libs: vec![],
        };
        let hits = walk_defs(&inputs, None).unwrap();
        assert!(hits.is_empty());
    }
}
