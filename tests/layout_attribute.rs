// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Parse-level lock for the `layout = [ ... ]` attribute on `component` and
//! `module` declarations (reserved interface ①, see mcd
//! doc/viz/pin-layout-design.md).
//!
//! Before the semantic rewrite the parser never recognized the real grammar
//! shape, so every `comp.layout` stayed empty and the feature was inert. These
//! tests lock the *semantic* outcome end-to-end from source text: each edge
//! becomes the exact list of member strings — numbers, `[a:b]` ranges
//! (ascending and descending), and function/port names — with `up`/`down`
//! accepted as aliases for `top`/`bottom`. A module body accepts `layout`
//! without E3081; a non-layout edge name is a warning, not a hard error.

mod common;

use std::sync::{Mutex, OnceLock};

static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Find a workspace component by its declared class name.
fn component(name: &str) -> mcc::McComponent {
    mcc::definition_space()
        .workspace_components()
        .into_iter()
        .find(|(sn, _)| sn.ident.to_string() == name)
        .map(|(_, c)| (*c).clone())
        .unwrap_or_else(|| panic!("component {name} not registered in workspace"))
}

/// Find a workspace module by its declared name.
fn module(name: &str) -> mcc::McModule {
    mcc::definition_space()
        .workspace_modules()
        .into_iter()
        .find(|(sn, _)| sn.ident.to_string() == name)
        .map(|(_, m)| (*m).clone())
        .unwrap_or_else(|| panic!("module {name} not registered in workspace"))
}

/// Load `src` under a fresh uri and return the diagnostics emitted by the
/// semantic pass. The caller must hold the file lock.
fn load_codes(src: &str, tag: &str) -> Vec<(u32, String)> {
    let uri = format!("/mcc/layout-attr-{tag}.mc");
    mcc::mcc_load_from_string(&uri, src);
    mcc::mcc_diagnose_all()
        .iter()
        .map(|d| (d.code, d.msg.clone()))
        .collect()
}

/// Component: numbers, ascending & descending ranges, function names, and an
/// empty edge all parse into the matching `McLayout` edge lists.
#[test]
fn comp_layout_edges_ranges_and_names() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    common::reset();
    load_codes(
        r#"
component CHIP9
{
    pins = [ 1 = A ]
    layout = [
        left = [1:3]
        right = [D, GND]
        top = [9, 7]
        bottom = [8:6]
    ]
}
"#,
        "comp",
    );
    let l = component("CHIP9").layout;
    assert_eq!(l.left, vec!["1", "2", "3"]);
    assert_eq!(l.right, vec!["D", "GND"]);
    assert_eq!(l.top, vec!["9", "7"]);
    // Descending range [8:6] stays descending in the semantic list.
    assert_eq!(l.bottom, vec!["8", "7", "6"]);
}

/// Module: a `layout` clause is accepted in the module body (no E3081) and the
/// edge aliases `up` / `down` map to `top` / `bottom`.
#[test]
fn mod_layout_accepted_with_up_down_aliases() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    common::reset();
    let diags = load_codes(
        r#"
module PMOD
{
    layout = [
        left = [EN]
        up = [LED1, LED2]
        right = [3:1]
        down = [VCC, GND]
    ]
}
"#,
        "mod",
    );
    assert!(
        !diags
            .iter()
            .any(|(c, _)| *c == mcc::errcodes::UNEXPECTED_CLAUSE_TYPE),
        "module layout must not raise E3081, got {diags:?}"
    );
    let l = module("PMOD").layout;
    assert_eq!(l.left, vec!["EN"]);
    assert_eq!(l.top, vec!["LED1", "LED2"], "`up` aliases to `top`");
    assert_eq!(l.right, vec!["3", "2", "1"]);
    assert_eq!(l.bottom, vec!["VCC", "GND"], "`down` aliases to `bottom`");
}

/// Real-content shape: hbl USB.MINI_B uses `right = [4, 3, 2, 5, 1]` and
/// `bottom = [6:9]`; the members must survive into the semantic layout.
#[test]
fn comp_layout_real_content_shape() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    common::reset();
    load_codes(
        r#"
component USB_MINI_B
{
    pins = [ 1 = VBUS ]
    layout = [
        right = [4, 3, 2, 5, 1]
        bottom = [6:9]
    ]
}
"#,
        "usb",
    );
    let l = component("USB_MINI_B").layout;
    assert_eq!(l.right, vec!["4", "3", "2", "5", "1"]);
    assert_eq!(l.bottom, vec!["6", "7", "8", "9"]);
    assert!(l.left.is_empty() && l.top.is_empty());
}

/// An unrecognized edge name is a warning (LAYOUT_EDGE_INVALID); the rest of
/// the layout still parses and the declaration is not rejected.
#[test]
fn comp_layout_unknown_edge_warns_only() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    common::reset();
    let diags = load_codes(
        r#"
component CHIPB
{
    pins = [ 1 = A ]
    layout = [
        left = [1]
        middle = [2]
        bottom = [2]
    ]
}
"#,
        "warn",
    );
    assert!(
        diags
            .iter()
            .any(|(c, _)| *c == mcc::errcodes::LAYOUT_EDGE_INVALID),
        "unknown edge must warn LAYOUT_EDGE_INVALID, got {diags:?}"
    );
    let l = component("CHIPB").layout;
    assert_eq!(l.left, vec!["1"]);
    assert_eq!(
        l.bottom,
        vec!["2"],
        "edges after the unknown one still parse"
    );
}
