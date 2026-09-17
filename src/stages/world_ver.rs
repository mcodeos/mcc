// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `world_ver` — the loaded world's source set as a deterministic hash.
//!
//! `world_ver` is the root token of the projection envelope
//! (`mcd/doc/world/projection-schema-design.md` §1): a hash over the **source
//! file set + version + library load set**, so a consumer can ask "is this the
//! same world I read last time, and did anything change?" without re-sending
//! the whole projection. `stage.*` views carry it because a persisted readout
//! that cannot say *which* world it describes is not comparable to anything.
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
const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a 64 prime (the standard constant).
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Fold one byte string into the running FNV-1a 64 state.
fn fold(mut h: u64, bytes: &[u8]) -> u64 {
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

/// The loaded world's source set + version, as `w_<16 hex>`.
///
/// Returns `None` when the world cannot be fingerprinted — an empty source set,
/// or a source whose text is unreadable. That is deliberate: an empty world
/// still hashes to *something*, and publishing that value would make "the world
/// is genuinely empty" and "the sources went missing" the same token. The
/// caller prints `-` for `None` (design §5.3: a value that is absent prints as
/// `-`) rather than a fingerprint that means nothing.
///
/// ## Where the source text comes from
///
/// `McCode.content` already holds the text of an in-memory load
/// ([`crate::db::infra::mc_code`]), so a source loaded from a string is hashed
/// from what was actually parsed. It is documented there as *empty for files
/// whose content was never parsed* — which is the normal case for a project
/// loaded from disk — so the filesystem read is the **main path, not a
/// fallback optimisation**.
pub fn world_ver() -> Option<String> {
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

    // Sorted *before* hashing: the map's iteration order is unspecified.
    pairs.sort();

    let mut h = FNV_OFFSET_BASIS;
    // The version is part of the material on purpose — the schema doc defines
    // the token over "source set + version + library set", and a readout
    // produced by a different compiler is not the same world even when the
    // sources are byte-identical. The library set needs no separate term: the
    // libraries are loaded sources, so they are already in `pairs`.
    h = fold(h, crate::buildinfo::VERSION.as_bytes());
    h = fold(h, &[0]);
    h = fold(h, crate::buildinfo::BUILD.as_bytes());
    h = fold(h, &[0]);
    for (uri, content) in &pairs {
        h = fold(h, uri.as_bytes());
        h = fold(h, &[0]);
        h = fold(h, content.as_bytes());
        h = fold(h, &[0]);
    }

    Some(format!("w_{h:016x}"))
}
