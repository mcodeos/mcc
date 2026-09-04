// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Warning suppression (`diag.ignore_warnings` config key + `-i/--ignore`
//! CLI flag, resolve-gate-design.md §5): the process-wide ignore set filters
//! E3137 (SINGLE_USE_INLINE_NET) from every output channel, errors are never
//! suppressed, and the config / CLI paths both seed the set.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use std::collections::HashSet;

use mcc::errcodes::SINGLE_USE_INLINE_NET;

/// Reset the process-wide ignore set so a test starts from a clean slate.
fn reset_ignored() {
    mcc::set_ignored_warnings(std::iter::empty::<String>());
}

/// Build `src` in a fresh workspace and return the emitted diagnostic codes.
fn build_codes(src: &str) -> HashSet<u32> {
    common::reset();
    let uri = "/mcc/ignore-warnings-test.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build(&mcc::McIds::from("main"), &uri);
    mcc::mcc_diagnose_all().iter().map(|d| d.code).collect()
}

/// A single-use inline ghost-net source that warns E3137.
const SINGLE_USE_SRC: &str =
    "module main {\n    io VDD\n    func main() {\n        uC.ADC.P -> VDD\n    }\n}";

/// A declared-component member miss that errors E3179 (never suppressible).
const ERROR_SRC: &str = "component R {\n    pins = [\n        1 = A\n        2 = B\n    ]\n}\nmodule main {\n    io VDD\n    R r1;\n    func main() {\n        r1.NOPIN -> VDD\n    }\n}";

#[test]
fn sem_ignwarn__single_use_e3137_fires_without_suppression() {
    let _lock = common::lock();
    reset_ignored();
    let codes = build_codes(SINGLE_USE_SRC);
    assert!(
        codes.contains(&SINGLE_USE_INLINE_NET),
        "baseline: E3137 must fire before any suppression; got codes: {codes:?}"
    );
}

#[test]
fn sem_ignwarn__set_ignored_warnings_filters_e3137() {
    let _lock = common::lock();
    reset_ignored();
    mcc::set_ignored_warnings([format!("E{SINGLE_USE_INLINE_NET}")]);
    let codes = build_codes(SINGLE_USE_SRC);
    assert!(
        !codes.contains(&SINGLE_USE_INLINE_NET),
        "E3137 must be filtered by the ignore set; got codes: {codes:?}"
    );
}

#[test]
fn sem_ignwarn__config_key_filters_e3137() {
    let _lock = common::lock();
    reset_ignored();

    // A project manifest carrying `[config.diag] ignore_warnings = ["E3137"]`.
    let dir = std::env::temp_dir().join(format!(
        "mcc-ignore-config-{}-{}",
        std::process::id(),
        SINGLE_USE_INLINE_NET
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("project.toml"),
        "[config.diag]\nignore_warnings = [\"E3137\"]\n",
    )
    .unwrap();

    mcc::load_ignore_warnings(Some(&dir), &[]);
    let codes = build_codes(SINGLE_USE_SRC);
    assert!(
        !codes.contains(&SINGLE_USE_INLINE_NET),
        "config key diag.ignore_warnings must seed the ignore set; got codes: {codes:?}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn sem_ignwarn__cli_codes_merge_over_config() {
    let _lock = common::lock();
    reset_ignored();

    // `-i 3137` (bare numeric form) with no project manifest.
    let dir = std::env::temp_dir().join(format!(
        "mcc-ignore-cli-{}-{}",
        std::process::id(),
        SINGLE_USE_INLINE_NET
    ));
    std::fs::create_dir_all(&dir).unwrap();
    mcc::load_ignore_warnings(Some(&dir), &["3137".to_string()]);
    let codes = build_codes(SINGLE_USE_SRC);
    assert!(
        !codes.contains(&SINGLE_USE_INLINE_NET),
        "CLI -i/--ignore codes must seed the ignore set; got codes: {codes:?}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn sem_ignwarn__never_suppresses_errors() {
    let _lock = common::lock();
    reset_ignored();
    // Add the E3179 code to the ignore set — an Error-level diagnostic must
    // still surface (suppression is Warning-only by design).
    mcc::set_ignored_warnings([format!("E{}", mcc::errcodes::COMPONENT_PIN_NOT_FOUND)]);
    let codes = build_codes(ERROR_SRC);
    assert!(
        codes.contains(&mcc::errcodes::COMPONENT_PIN_NOT_FOUND),
        "an Error must never be suppressed by the ignore set; got codes: {codes:?}"
    );
}
