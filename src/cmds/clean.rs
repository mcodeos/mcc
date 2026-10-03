// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc clean` (registry-design.md §4.6): cache sweeping. This batch
//! implements `--cache` only — the registry metadata cache
//! (`<data_root>/cache/meta/`, per-source shards). The bare form (project
//! build products, `deps/`) is a separate pending face; the U392 library
//! parse cache (`cache/libparse/`) has its own invalidation law and is
//! deliberately out of scope here, as are installed packs (§4.6: that is
//! `lib uninstall`'s territory).

use anyhow::{bail, Context, Result};

pub fn run(cache: bool) -> Result<()> {
    if !cache {
        bail!(
            "mcc clean: nothing selected — only --cache is implemented (the registry \
             metadata cache, registry-design.md §4.6); project build products are a \
             separate pending face"
        );
    }
    let dir = mcc::meta_cache_dir();
    if dir.is_dir() {
        std::fs::remove_dir_all(&dir)
            .with_context(|| format!("cannot clear {}", dir.display()))?;
        eprintln!("✓ cleared {}", dir.display());
    } else {
        eprintln!("= nothing to clear ({})", dir.display());
    }
    eprintln!(
        "  the next solve refetches (an ETag miss is a full fetch); cache/libparse and \
         installed packs are untouched"
    );
    Ok(())
}
