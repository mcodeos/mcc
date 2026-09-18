// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `world_ver` — the loaded world's source set as a deterministic hash, and the
//! hash machine its per-top sibling [`super::top_ver`] reuses.
//!
//! `world_ver` is the root token of the projection envelope
//! (`mcd/doc/world/projection-schema-design.md` §1): a hash over the **source
//! file set + version + library load set**, so a consumer can ask "is this the
//! same world I read last time, and did anything change?" without re-sending
//! the whole projection. `stage.*` views carry it because a persisted readout
//! that cannot say *which* world it describes is not comparable to anything.
//!
//! The schema (§1.1) gives a second token, `top_ver`, to the same machine over a
//! subset of the same material — the top closure's sub-hash is *the same hash
//! machine as the root* and *sits inside the root's material*, in the schema's
//! own words. So this module owns the material ([`source_pairs`])
//! and the machine ([`digest`]); `world_ver` is that machine over all of it and
//! `top_ver` is the same machine over the closure's share of it. Two tokens,
//! **one scan** — deriving them separately would read every source twice.
//!
//! Two properties are required, and each one dictates how this is written:
//!
//! - **Reproducible across machines and Rust releases.** The hash is a
//!   hand-written FNV-1a 64 rather than [`std::collections::hash_map::DefaultHasher`],
//!   whose output is explicitly not guaranteed stable between Rust versions. A
//!   token whose value changes when the compiler does is not a token.
//! - **Independent of loader order.** The source set lives in a `DashMap`,
//!   whose iteration order is unspecified, so the pairs are collected and
//!   sorted by URI *before* hashing. Hashing in place would make `world_ver`
//!   depend on hash-table layout — exactly the class of non-determinism these
//!   views exist to remove (`build-design.md` §3.7 discipline 0: allocation
//!   order must be decided by the input alone).

use crate::db::cmie::tables::WORKSPACE;

/// FNV-1a 64 offset basis (the standard constant).
pub(crate) const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a 64 prime (the standard constant).
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Fold one byte string into the running FNV-1a 64 state.
pub(crate) fn fold(mut h: u64, bytes: &[u8]) -> u64 {
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

/// The loaded world as `(uri, content)` pairs, **sorted by URI**.
///
/// Returns `None` when the world cannot be fingerprinted — an empty source set,
/// or a source whose text is unreadable. That is deliberate: an empty world
/// still hashes to *something*, and publishing that value would make "the world
/// is genuinely empty" and "the sources went missing" the same token. The
/// caller prints `-` for `None` (design §5.3: a value that is absent prints as
/// `-`) rather than a fingerprint that means nothing.
///
/// Sorting happens **here**, once, so both tokens inherit it and neither can
/// forget: the source set lives in a `DashMap`, whose iteration order is
/// unspecified, and hashing in place would make both tokens depend on
/// hash-table layout (build-design §3.7 discipline 0).
///
/// ## Where the source text comes from
///
/// `McCode.content` already holds the text of an in-memory load
/// ([`crate::db::infra::mc_code`]), so a source loaded from a string is hashed
/// from what was actually parsed. It is documented there as *empty for files
/// whose content was never parsed* — which is the normal case for a project
/// loaded from disk — so the filesystem read is the **main path, not a
/// fallback optimisation**.
pub(crate) fn source_pairs() -> Option<Vec<(String, String)>> {
    let mut pairs: Vec<(String, String)> = Vec::with_capacity(WORKSPACE.mcodes.len());

    for entry in WORKSPACE.mcodes.iter() {
        let uri = entry.key().to_string();
        let in_memory = entry.value().content.clone();
        let content = if in_memory.is_empty() {
            std::fs::read_to_string(&uri).ok()?
        } else {
            in_memory
        };
        pairs.push((uri, content));
    }

    if pairs.is_empty() {
        return None;
    }

    pairs.sort();
    Some(pairs)
}

/// The hash machine, run over an already-collected material set.
///
/// `pairs` must already be sorted (see [`source_pairs`]); this function does
/// not sort, so that it stays the same machine whatever subset it is handed —
/// which is what lets [`super::top_ver`] hash a *sub-sequence* of the same
/// material and land on the same digest when the subset is the whole world.
///
/// The version is part of the material on purpose — the schema doc defines the
/// token over "source set + version + library set", and a readout produced by a
/// different compiler is not the same world even when the sources are
/// byte-identical. The library set needs no separate term: the libraries are
/// loaded sources, so they are already in `pairs`. That is also why `top_ver`
/// needs no "library load set ∩ closure" term of its own — the intersection is
/// just which of these pairs the closure selects.
pub(crate) fn digest(pairs: &[(String, String)]) -> u64 {
    let mut h = FNV_OFFSET_BASIS;
    h = fold(h, crate::buildinfo::VERSION.as_bytes());
    h = fold(h, &[0]);
    h = fold(h, crate::buildinfo::BUILD.as_bytes());
    h = fold(h, &[0]);
    for (uri, content) in pairs {
        h = fold(h, uri.as_bytes());
        h = fold(h, &[0]);
        h = fold(h, content.as_bytes());
        h = fold(h, &[0]);
    }
    h
}

/// [`world_ver`] over material the caller has already collected.
///
/// The sibling of [`super::top_ver::top_ver_from`], and the reason both exist:
/// one call to [`source_pairs`] feeds both tokens — the schema asks for the two
/// to be derived together in one pass, with no second scan (§1.1). Reading the
/// sources twice to spell the same numbers would be that second scan, for
/// nothing.
pub(crate) fn root_from(pairs: &[(String, String)]) -> String {
    format!("w_{:016x}", digest(pairs))
}

/// The loaded world's source set + version, as `w_<16 hex>`.
pub fn world_ver() -> Option<String> {
    source_pairs().as_deref().map(root_from)
}
