// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `top_ver` — one top's dependency closure, as a deterministic hash.
//!
//! The projection envelope carries two revision tokens
//! (`mcd/doc/world/projection-schema-design.md` §1 / §1.1). The root one,
//! [`super::world_ver`], answers "did the whole world change?"; this one answers
//! "did *this top* change?", so that editing file A does not invalidate a client
//! that is watching top B.
//!
//! It is **the same machine over less material**, not a second machine. That is
//! the schema's own reading — the top closure's sub-hash is *the same hash
//! machine as the root* and *sits inside the root's material* — and it is what
//! makes the two tokens
//! comparable: when a top's material happens to cover the whole loaded world,
//! the two digests are the same number and only the prefix differs. A second
//! machine would make that agreement unobservable, and with it the one property
//! that says this token is the root token's sibling rather than an unrelated
//! number.
//!
//! ## What the material is, exactly
//!
//! The root token's material is the whole loaded world. This one's is that
//! material minus **the project's own files that the top does not reach** —
//! everything else stays in, including the loaded libraries.
//!
//! The libraries are the interesting part, because they are *preloaded*: the
//! system library is documented as needing no `use` statement, "all definitions
//! are globally available … similar to Python's builtins", so a `use`-edge walk
//! never reaches it and a strictly closure-shaped material would leave it out of
//! every top. That would break the one promise this token makes. The schema
//! names the term itself — sorted source URIs + their versions + the **library
//! load set ∩ closure** — and
//! the only reading under which it is not vacuous is this one: the library load
//! set is a dependency of *every* top, so it belongs to every top's material.
//! Editing a library invalidates a watcher of any top, which is what a reader
//! would expect and what the root token already says.
//!
//! Consistently, a source the definition space does not know is kept as well.
//! The exclusion is stated as "known to be a project source *and* outside the
//! closure" rather than "inside the closure", so that a file this code cannot
//! classify makes the token move when it might have needed to, never the other
//! way round — for a revision token, over-reporting change is the safe error.

use std::collections::HashSet;

use super::world_ver::{digest, source_pairs};
use crate::db::cmie::tables::WORKSPACE;
use crate::McURI;

/// The token for `top`, as `t_<16 hex>`.
///
/// `None` when it cannot be derived: no loaded world, an unreadable source, a
/// `top` name that names no project module, or one that names more than one.
/// The caller prints `-` (design §5.3) — a token standing for "I could not tell
/// which top you mean" would be worse than no token at all.
pub fn top_ver(top: &str) -> Option<String> {
    let pairs = source_pairs()?;
    top_ver_from(&pairs, top)
}

/// [`top_ver`] over material the caller has already collected.
///
/// Exists so that one call to [`source_pairs`] can feed both tokens — schema
/// §1.1: both tokens from one pass, no second scan. Deriving them
/// independently would read every source off disk twice.
pub(crate) fn top_ver_from(pairs: &[(String, String)], top: &str) -> Option<String> {
    let closure = closure_uris(top)?;
    // A sub-sequence of `pairs`, so it is still sorted; [`digest`] requires
    // that, and filtering a sorted vector is the one place it comes for free.
    let material: Vec<(String, String)> = pairs
        .iter()
        .filter(|(uri, _)| {
            closure.contains(uri) || !crate::definition_space().is_project_source(&McURI::from(uri.as_str()))
        })
        .cloned()
        .collect();
    if material.is_empty() {
        return None;
    }
    Some(format!("t_{:016x}", digest(&material)))
}

/// Every source `top` depends on, **including the file that defines it**.
///
/// The closure is the *declared* one: `use` / `import` edges walked over source
/// files. Not the set of files instantiation happens to reach — the schema asks
/// for the dependency closure, and a watcher deciding whether to re-read a top
/// needs the answer without having to build that top first.
fn closure_uris(top: &str) -> Option<HashSet<String>> {
    let start = defining_uri(top)?;
    let mut seen: HashSet<String> = HashSet::new();
    let mut stack = vec![start.clone()];
    seen.insert(start);

    while let Some(uri) = stack.pop() {
        let Some(code) = WORKSPACE.mcodes.get(&uri) else {
            // A use target that was never loaded contributes nothing; it is not
            // an error here, because the closure is read off the declarations
            // and a missing target is Pass1's business, not this token's.
            continue;
        };
        for u in &code.uselist {
            let target = crate::build::pass1::canonicalize_project_uri(&u.uri);
            if seen.insert(target.clone()) {
                stack.push(target);
            }
        }
    }
    Some(seen)
}

/// The file that defines the project module named `top`.
///
/// Unique match or nothing: two project modules may share a name (they are
/// distinct definitions in distinct files), and picking one of them would make
/// the token silently describe the wrong file. Same discipline as the viz
/// view's determinism-layer lookup — an ambiguous answer is reported as no
/// answer, never as a guess.
fn defining_uri(top: &str) -> Option<String> {
    let mut hit: Option<String> = None;
    for (sn, _module) in crate::definition_space().workspace_modules() {
        if sn.ident.to_string() != top {
            continue;
        }
        if hit.is_some() {
            return None;
        }
        hit = Some(crate::build::pass1::canonicalize_project_uri(
            &sn.uri.as_uri().to_string(),
        ));
    }
    hit
}
