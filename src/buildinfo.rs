// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Crate version + per-build counter.
//!
//! [`crate`](crate) `build.rs` reads the gitignored `.buildnr`, writes back
//! n+1 and feeds `MCC_BUILD_NR` to rustc, so every `cargo build`/`check`/`test`
//! invocation stamps this build with a fresh monotonic number. `mcc --version`
//! and the RPC `caps` / `admin.server_info` surfaces report it. The counter is
//! a build convenience (per-machine), not a source-level fact — nothing
//! semantic depends on it.

/// Semver from `Cargo.toml` (e.g. `0.9.0`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Monotonic per-build counter set by `build.rs`
/// (`cargo:rustc-env=MCC_BUILD_NR`).
pub const BUILD: &str = env!("MCC_BUILD_NR");

/// `"0.9.0.b42"` display form (`mcc --version`).
pub fn display() -> String {
    format!("{VERSION}.b{BUILD}")
}

/// Counter as an integer, for the JSON `"build"` fields of RPC surfaces.
pub fn number() -> u64 {
    BUILD.parse().unwrap_or(0)
}
