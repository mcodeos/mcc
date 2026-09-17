// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! E3112 `PARAM_INLINE_ATTRS_UNSUPPORTED` — the component-instance parameter
//! form `id::Class(k = v)` (A5) parses and is classified, but no engine
//! consumer reads it, so its attribute body would be dropped silently. The
//! diagnostic must fire at **every** header site and must be **class-name
//! independent**: routing it through the `is_enum` fork (a class-registry
//! lookup) would make an enum class report E3112 while any other class fell
//! through to the instance path and reported "unresolved/unloaded class".
//!
//! Family naming `{family}__{essence}` uses a doubled underscore to separate
//! the grep-able family token from the essence (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

const E3112: u32 = 3112;

/// Diagnostic codes produced by building `main` in `src`.
fn codes(uri: &str, src: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let _ = mcc::mcc_build(&McIds::from("main"), &u);
    mcc::mcc_diagnose_all().iter().map(|d| d.code).collect()
}

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|c| **c == code).count()
}

/// The declaration-site form fires E3112 for a **non-enum** class name
/// (`NMOS`) — the case the `is_enum` fork used to misroute.
#[test]
fn pa_attrs__module_header_non_enum_class() {
    let src = "module Child(x::NMOS(partno=\"BC3407\")) { }\nmodule main { }\n";
    let c = codes("/mcc/e3112-module-nmos.mc", src);
    assert_eq!(count(&c, E3112), 1, "codes={c:?}");
    // Must not be reported as a missing/unloaded instance class.
    assert!(!c.contains(&5256), "codes={c:?}");
    assert!(!c.contains(&3157), "codes={c:?}");
}

/// The same form fires E3112 for a class name that **is** in the enum
/// registry — i.e. the verdict does not depend on the name.
#[test]
fn pa_attrs__module_header_enum_like_class() {
    let src = "module Child(x::CAP(partno=\"GRM\")) { }\nmodule main { }\n";
    let c = codes("/mcc/e3112-module-cap.mc", src);
    assert_eq!(count(&c, E3112), 1, "codes={c:?}");
}

/// Several A5 formals in one header yield one diagnostic each.
#[test]
fn pa_attrs__module_header_multiple_formals() {
    let src = "module Child(x::CAP(partno=\"GRM\"), PD::NMOS(partno=\"BC3407\")) { }\n\
               module main { }\n";
    let c = codes("/mcc/e3112-module-multi.mc", src);
    assert_eq!(count(&c, E3112), 2, "codes={c:?}");
}

/// `component` / `interface` headers reach the same verdict through the
/// parameter-declaration path.
#[test]
fn pa_attrs__component_and_interface_headers() {
    let component = "component Child(x::NMOS(partno=\"BC3407\")) { }\nmodule main { }\n";
    let c = codes("/mcc/e3112-component.mc", component);
    assert_eq!(count(&c, E3112), 1, "codes={c:?}");

    let interface = "interface Iface(x::NMOS(partno=\"BC3407\")) { }\nmodule main { }\n";
    let c = codes("/mcc/e3112-interface.mc", interface);
    assert_eq!(count(&c, E3112), 1, "codes={c:?}");
}

/// `func` headers too — the diagnostic is site-uniform (contrast E3055, which
/// is module-only by design).
#[test]
fn pa_attrs__func_header() {
    let src = "module main {\n    func Fn(x::NMOS(partno=\"BC3407\")) { }\n}\n";
    let c = codes("/mcc/e3112-func.mc", src);
    assert_eq!(count(&c, E3112), 1, "codes={c:?}");
}

/// Negative: the **call-site** form `Class(port::Type(k = v))` is supported
/// and must stay silent.
#[test]
fn pa_attrs__call_site_is_silent() {
    let src = "component NMOS { pins = [\n        1 = D\n        2 = G\n    ] }\n\
               component RES { pins = [\n        1 = 1\n        2 = 2\n    ] }\n\
               interface DC(volt)\n{\n    pins = [\n        1 = VOUT\n        2 = GND\n    ]\n}\n\
               func AntiReverseConnection(a, b, c)\n{\n    a - b\n    b - c\n}\n\
               module main {\n    \
               AntiReverseConnection(PD::NMOS(partno=\"BC3407\"), PR::RES(10k), GND)\n}\n";
    let c = codes("/mcc/e3112-callsite.mc", src);
    assert_eq!(count(&c, E3112), 0, "codes={c:?}");
}

/// Negative: an ordinary interface-typed module port is untouched.
#[test]
fn pa_attrs__plain_iface_port_is_silent() {
    let src = "interface DC(volt)\n{\n    pins = [\n        1 = VOUT\n        2 = GND\n    ]\n}\n\
               module main(psnk [VDD_3V3,GND]::DC(3.3V))\n{\n    VDD_3V3 -> GND\n}\n";
    let c = codes("/mcc/e3112-plain-port.mc", src);
    assert_eq!(count(&c, E3112), 0, "codes={c:?}");
}
