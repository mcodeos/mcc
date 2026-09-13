// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Crate version + shared build number.
//!
//! [`crate`](crate) `build.rs` reads `mcd/BUILDNr` — the ledger repo's batch
//! counter, committed once per landed batch — and feeds it to rustc via
//! `cargo:rustc-env`. Every author building the same batch therefore bakes the
//! same number, unlike a per-machine counter that nobody else can reproduce.
//! `mcc --version` and the RPC `caps` / `admin.server_info` surfaces report it;
//! the ledger (`CIMP.md` §2) maps a number back to the mcc/mcd commits.

/// Semver from `Cargo.toml` (e.g. `0.9.0`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Shared batch number set by `build.rs` (`cargo:rustc-env=MCC_BUILD_NR`);
/// `0` when mcc was built without the ledger repo in reach.
pub const BUILD: &str = env!("MCC_BUILD_NR");

/// Batch number as an integer, for the JSON `"build"` fields of RPC surfaces.
pub fn number() -> u64 {
    BUILD.parse().unwrap_or(0)
}
