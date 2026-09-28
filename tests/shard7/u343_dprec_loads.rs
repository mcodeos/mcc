// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Behavior locks for the two live `%dprec` resolutions in the grammar
//! (mca.y `mc_param` and `mc_net`). Both sites resolve a real ambiguity by
//! precedence; the dropped branch builds a tree no reader accepts, so the
//! locks pin the winning branch's observable behavior instead of the
//! directive itself (U343 structural-risk note, d1' ruling).
//!
//! 1. the call-site named key written `k: v` must bind exactly like `k = v`:
//!    the `%dprec 2` named-key arm produces the `MCAST_ATTRIBUTE` shape; the
//!    dropped `mc_phrase` colon arm produces an `MCAST_OPD_COLON` no argument
//!    reader recognises, so a flip would drop the argument in silence.
//! 2. the iotype net row (`psnk 3 = GND`, `psnk [1,2] = GND`) must keep the
//!    port-elems structure — pinned by the existing `dianlu_core` locks,
//!    cited here so the pairing of lock to directive is findable.
#![allow(non_snake_case)]

use crate::common;

use mcc::{DiagnosticLevel, McIds};

/// One diagnostic, flattened to what the assertions read.
#[derive(Debug)]
struct Diag {
    code: u32,
    level: DiagnosticLevel,
    msg: String,
}

/// The class used by the spelling-equivalence rows: `Vout` is a key whose
/// value is the declared parameter `vout`, so a call-site `Vout` argument
/// binds the parameter (the shared vehicle of `param_call_site_key_binding`).
const VOUT_HEAD: &str = "component C (vout::UV.VOLT = 3.3V) {\n";

const PINS: &str = "    pins = [\n        1 = A\n        2 = B\n    ]\n}\n";

fn probe(src: &str, uri: &str) -> Vec<Diag> {
    let _lock = common::lock();
    common::reset();
    let uri: mcc::McURI = uri.to_string();
    mcc::mcc_load_from_string(&uri, src);
    let (_, _arena, _store, _) =
        mcc::mcc_build_with_arena(&McIds::from("main"), &uri).expect("build");
    let mut ds: Vec<Diag> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| Diag {
            code: d.code,
            level: d.level,
            msg: d.msg.clone(),
        })
        .collect();
    ds.sort_by(|a, b| a.code.cmp(&b.code).then(a.msg.cmp(&b.msg)));
    ds
}

/// Diagnostics minus the unconnected-pin noise the toy class produces.
fn bind_diags(src: &str, uri: &str) -> Vec<Diag> {
    probe(src, uri)
        .into_iter()
        .filter(|d| !matches!(d.code, 4112 | 4116 | 4119))
        .collect()
}

/// The bound attribute value of instance `inst`, as the renderers print it.
fn resolved_attrs(src: &str, uri: &str, inst: &str) -> Vec<String> {
    let _lock = common::lock();
    common::reset();
    let uri: mcc::McURI = uri.to_string();
    mcc::mcc_load_from_string(&uri, src);
    let (tree, arena, store, _) =
        mcc::mcc_build_with_arena(&McIds::from("main"), &uri).expect("build");
    let view = mcc::TreeView::new(&arena, &store);
    let comps: Vec<(String, Vec<String>)> = view
        .components(&tree)
        .map(|c| {
            (
                c.name.clone(),
                c.resolved_attrs.iter().map(|a| a.to_string()).collect(),
            )
        })
        .collect();
    comps
        .into_iter()
        .find(|(name, _)| name == inst)
        .map(|(_, a)| a)
        .unwrap_or_default()
}

/// `k: v` binds the named key: zero orphan/bind diagnostics, same as `k = v`.
#[test]
fn dprec__colon_named_key_binds_clean() {
    let src = format!(
        "{VOUT_HEAD}    spec.Vout = vout\n{PINS}\nmodule main {{\n    C c1( Vout: 2.5V )\n}}\n"
    );
    let ds = bind_diags(&src, "/mcc/u343-colon-key.mc");
    assert!(
        ds.is_empty(),
        "the colon named key must bind cleanly; got {ds:?}"
    );
}

/// The two spellings are one fact (the dprec arm builds the same
/// `MCAST_ATTRIBUTE` shape for both): identical resolved attributes.
#[test]
fn dprec__colon_and_equals_spelling_resolve_alike() {
    let colon = format!(
        "{VOUT_HEAD}    spec.Vout = vout\n{PINS}\nmodule main {{\n    C c1( Vout: 2.5V )\n}}\n"
    );
    let equals = format!(
        "{VOUT_HEAD}    spec.Vout = vout\n{PINS}\nmodule main {{\n    C c1( Vout = 2.5V )\n}}\n"
    );
    let a = resolved_attrs(&colon, "/mcc/u343-colon.mc", "c1");
    let b = resolved_attrs(&equals, "/mcc/u343-equals.mc", "c1");
    assert_eq!(
        a, b,
        "the spellings must resolve alike; colon={a:?} equals={b:?}"
    );
    assert!(
        a.iter().any(|v| v.contains("2.5V")),
        "the bound value must be visible; got {a:?}"
    );
}

/// An unknown name through the colon spelling reports the same orphan fact as
/// the equals spelling — the named-key arm, not the dropped phrase arm, is
/// what produced the row.
#[test]
fn dprec__colon_orphan_reports_like_equals() {
    let colon = format!(
        "{VOUT_HEAD}    spec.Vout = vout\n{PINS}\nmodule main {{\n    C c1( nope: 5 )\n}}\n"
    );
    let equals = format!(
        "{VOUT_HEAD}    spec.Vout = vout\n{PINS}\nmodule main {{\n    C c1( nope = 5 )\n}}\n"
    );
    let a = bind_diags(&colon, "/mcc/u343-colon-orphan.mc");
    let b = bind_diags(&equals, "/mcc/u343-equals-orphan.mc");
    let codes = |ds: &[Diag]| ds.iter().map(|d| d.code).collect::<Vec<_>>();
    assert_eq!(
        codes(&a),
        codes(&b),
        "both spellings must reach the same verdict; colon={a:?} equals={b:?}"
    );
    assert!(
        a.iter().any(|d| d.msg.contains("nope")),
        "the orphan must be named; got {a:?}"
    );
}
