// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The console section for the flat electrical net checks.
//!
//! One renderer for every surface that shows them: a local `mcc build` (single
//! file and folder), a delegated `build.full` payload read back by the CLI, and
//! `mcc check --nets`. The rows come from the envelope's `pass2.net_checks`
//! whenever there is an envelope — so what a build prints is what it carries,
//! on both faces — and from `mcc check`'s own pass-2 run otherwise.
//!
//! Findings are reported, never gated: the Tier-0 netcheck and the exit code's
//! error count own the failure semantics.

use mcc::check::nets::NetCheckRow;

/// Print `=== Electrical Net Checks (N issues) ===` and one line per row.
///
/// Prints nothing at all when there are no rows — an empty section would be
/// noise on every clean build.
pub fn render_section(rows: &[NetCheckRow]) {
    if rows.is_empty() {
        return;
    }
    eprintln!("=== Electrical Net Checks ({} issues) ===", rows.len());
    for r in rows {
        eprintln!("  [{}] {}: {}", r.severity, r.check, r.message);
    }
}
