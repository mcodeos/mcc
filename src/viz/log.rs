// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Gated pipeline logging.
//!
//! The viz layout/route/render pipeline used to print a large amount of per-box /
//! per-net / per-layer progress to stderr on every render. That noise is only
//! useful while debugging the pipeline, so it is now gated behind `MC_VIZ_DUMP`
//! (the same flag as [`crate::viz::debug`]).
//!
//! Use [`vlog!`] exactly like `eprintln!`; output is suppressed unless
//! `MC_VIZ_DUMP` is set to a non-empty, non-`0`/`false` value.

use std::sync::OnceLock;

static VLOG_ENABLED: OnceLock<bool> = OnceLock::new();

/// Whether gated viz pipeline logging is enabled (`MC_VIZ_DUMP`).
pub fn enabled() -> bool {
    *VLOG_ENABLED.get_or_init(|| match std::env::var("MC_VIZ_DUMP") {
        Ok(v) => {
            let t = v.trim();
            !(t.is_empty() || t == "0" || t == "false" || t == "False" || t == "FALSE")
        }
        Err(_) => false,
    })
}

/// `eprintln!`-compatible macro that only prints when `MC_VIZ_DUMP` is enabled.
#[macro_export]
macro_rules! vlog {
    ($($arg:tt)*) => {
        if $crate::viz::log::enabled() {
            eprintln!($($arg)*);
        }
    };
}
