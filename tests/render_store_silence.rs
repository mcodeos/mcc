// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Stage B (world-core refactor): render must not write the Problems store.
//!
//! Root cause of the reported defect: the render / export surfaces flattened a
//! circuit (for drawing) and unconditionally logged the flat net-check
//! diagnostics into the shared workspace store they never consume — so merely
//! opening a file (auto viz) flooded every source file with the whole
//! circuit's electrical warnings. Phase A contract: `flatten` RETURNS the
//! diagnostics and the owning surface decides logging. Validation surfaces
//! (`mcb_pass2_flat` / `mcc_build_flat`) log; render / synthetic / export do
//! not. These tests lock that split.

#![allow(non_snake_case)]

mod common;

use mcc::McIds;

/// Two-input/one-output buffer used by the driver-conflict fixture.
const BUF: &str = "component BUF {\n    pins = [\n        in 1 = A\n        out 2 = Y\n    ]\n}\n";

/// `b1.Y -> b2.Y` merges two `Out` pins onto one net → a 4101 driver conflict
/// (plus floating-input / module-port / partial-wiring companions).
const CONFLICT_SRC: &str = "module main {\n    BUF b1\n    BUF b2\n    b1.Y -> b2.Y\n}";

/// The flat net-check code family (run_net_checks). Nothing outside this set
/// may be produced by a flatten of the conflict fixture.
fn is_net_check_code(code: u32) -> bool {
    (4101..=4119).contains(&code) || code == 4056 || code == 6005
}

fn store_net_codes() -> Vec<u32> {
    mcc::mcc_diagnose_all()
        .iter()
        .map(|d| d.code)
        .filter(|c| is_net_check_code(*c))
        .collect()
}

/// The validation wrapper (`mcb_pass2_flat`, via `mcc_build_flat`) is the
/// owning surface: its flatten diagnostics reach the store.
#[test]
fn render_silence__validation_wrapper_logs_net_diags() {
    let _lock = common::lock();
    common::reset();
    let uri = "/mcc/render-silence.mc".to_string();
    common::load_string(&uri, &format!("{BUF}{CONFLICT_SRC}"));
    mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");
    assert!(
        store_net_codes().contains(&4101),
        "validation wrapper must log the 4101; got {:?}",
        store_net_codes()
    );
}

/// The render entry (`mcc_build_flat_with_arena`, what viz flattens through)
/// must NOT write net diagnostics to the store — flatten returns them and the
/// render caller does not own the Problems surface.
#[test]
fn render_silence__render_entry_leaves_store_clean() {
    let _lock = common::lock();
    common::reset();
    let uri = "/mcc/render-silence.mc".to_string();
    common::load_string(&uri, &format!("{BUF}{CONFLICT_SRC}"));
    mcc::mcc_build_flat_with_arena(&McIds::from("main"), &uri, 1000).expect("flat build");
    assert!(
        store_net_codes().is_empty(),
        "render entry must not write net diagnostics; got {:?}",
        store_net_codes()
    );
}
