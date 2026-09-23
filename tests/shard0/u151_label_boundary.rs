// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U151: the module boundary ticket is the direction word. A member without
//! one (an inline body name; historically also explicit `label` declaration
//! rows) is module-internal, and an access through an instance dot-path from
//! the parent body is E3184 (label-boundary-gate-design.md).
//!
//! Two faces are locked, because a single-point gate provably misses one:
//! a width-incompatible access dies in Pass1 (the shape gate rejects the
//! statement before instantiation), while a width-compatible one survives
//! into Pass2 and is judged at the point-mint/validation face. Controls pin
//! every branch that must stay clean: direction-worded ports, interface
//! members, func-call seams, and members of an aggregate direction-worded
//! port (whose own def-store entry carries no direction word — the owning
//! port's direction is the member's ticket).
//!
//! Same in-process harness as `u152_body_face_decl_order`.

#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI, errcodes};

/// E3184 diagnostics for one in-process build of `source`.
fn e3184_count(source: &str) -> usize {
    let _lock = common::lock();
    let system_root = mcc::cli::datadir::data_root();
    mcc::mcc_clear_workspace();
    mcc::mcc_set_system_root(&system_root);
    mcc::mcc_init();

    let uri: McURI = "/mcc/u151-label-boundary.mc".to_string();
    mcc::mcc_load_from_string(&uri, source);
    mcc::mcc_build_with_arena(&McIds::from("main"), &uri).ok();
    mcc::mcc_diagnose(&uri)
        .iter()
        .filter(|d| d.code == errcodes::LABEL_NOT_EXPORTABLE)
        .count()
}

const SUB: &str = r#"
module Sub(
    io P1
)
{
    L1 -> P1
    inner -> P1
}
"#;

/// Pass1 face: the label access rides a chain the shape gate rejects on
/// width (a 2-wide aggregate output vs a 1-wide label), so the statement
/// never reaches instantiation — E3184 must still fire, anchored at the
/// operator (report-only; the shape verdict stands).
#[test]
fn label_access_reports_on_the_pass1_shape_face() {
    let wide = r#"
module Wide()
{
    out [X1, X2]::DC(3.3V)
}
"#;
    let n = e3184_count(&format!(
        "{SUB}\n{wide}\nmodule main() {{\n    Sub s1\n    Wide w\n    w -> s1.L1\n}}"
    ));
    assert_eq!(n, 1, "expected exactly one E3184 for the header-label leak");
}

/// Pass2 face: a width-compatible access (1↔1) passes the shape gate, the
/// statement instantiates, and the point face judges the member — E3184.
#[test]
fn label_access_reports_on_the_pass2_point_face() {
    let n = e3184_count(&format!(
        "{SUB}\nmodule main() {{\n    Sub s1\n    net_a -> s1.L1\n}}"
    ));
    assert_eq!(n, 1, "expected exactly one E3184 for the header-label leak");
}

/// A direction-less member is internal the same way — judged, not exempted.
#[test]
fn directionless_member_access_reports() {
    let n = e3184_count(&format!(
        "{SUB}\nmodule main() {{\n    Sub s1\n    net_a -> s1.inner\n}}"
    ));
    assert_eq!(n, 1, "expected exactly one E3184 for the direction-less leak");
}

/// Control: a direction-worded port is on the boundary — clean.
#[test]
fn direction_worded_port_stays_clean() {
    let n = e3184_count(&format!(
        "{SUB}\nmodule main() {{\n    Sub s1\n    net_a -> s1.P1\n}}"
    ));
    assert_eq!(n, 0, "io port access must not report E3184");
}

/// Control: the func-call seam is a designed boundary crossing — clean.
#[test]
fn func_call_seam_stays_clean() {
    let sub = r#"
module Sub2(
    io P1
)
{
    func setup(x)
    {
        x -> P1
    }
}
"#;
    let n = e3184_count(&format!(
        "{sub}\nmodule main() {{\n    Sub2 s1\n    s1.setup(net_a)\n}}"
    ));
    assert_eq!(n, 0, "func call must not report E3184");
}

/// Control: a member of an aggregate direction-worded port inherits the
/// owning port's ticket. Its own def-store entry is an implicit label, so a
/// gate that read the direct entry would false-positive here.
#[test]
fn aggregate_port_member_stays_clean() {
    let sub = r#"
module Sub3()
{
    in [VA, GB]::DC(1.2V)
    out [W1, W2]::DC(3.3V)
}
"#;
    let n = e3184_count(&format!(
        "{sub}\nmodule main() {{\n    Sub3 s2\n    s2.W1 -> s2.VA\n}}"
    ));
    assert_eq!(n, 0, "aggregate-port member access must not report E3184");
}
