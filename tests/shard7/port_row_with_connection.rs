// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Lock for PORT_ROW_WITH_CONNECTION (4023) — the "port row carries a
// connection" judgment in the MCAST_NET_PORTS reader
// (`semantic::mc_inst::InstTable::parse`).
//
// Background (design: `nc-design.md` §4.3; CIMP §1 U50): `nc N1 -> d1.A`
// vanishes without a word — 0 connections, 0 diagnostics, `N1` never entering
// the symbol table. The grammar accepts the row (mca.y
// `mc_net: mc_iotype mc_phrase mc_tattrs_opt` produces MCAST_NET_PORTS), so the
// silence is a *missing consumer*, not a missing production: the reader matches
// MCAST_DECLARE / MCAST_OPD / MCAST_OPD_SQUARE_VEC and drops everything else
// through a wildcard arm.
//
// The judgment is structural — "an iotype-prefixed row carrying a connection
// phrase" — never a name check, which is why the `in` twin below must fire the
// same code. Every branch of the new arm is exercised here: the two that must
// report, and the three legal row shapes that must stay silent.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use mcc::McIds;

const CODE: u32 = 4023;

/// A minimal class the rows can connect to.
const D1: &str = r#"
component D1
{
    pins = [
        1 = A;
        2 = B;
    ]
}
"#;

fn diags_for(body: &str) -> Vec<mcc::McDiagnostic> {
    let _lock = common::lock();
    common::reset();
    let source = format!("{D1}\nmodule main\n{{\n{body}\n}}\n");
    let uri: mcc::McURI = "/mcc/port-row-conn.mc".to_string();
    mcc::mcc_load_from_string(&uri, &source);
    let result = mcc::mcc_build(&McIds::from("main"), &uri);
    result.expect("build failed");
    mcc::mcc_diagnose_all()
        .into_iter()
        .filter(|d| d.code == CODE)
        .collect()
}

#[test]
fn sem_portrowconn__nc_head_reports() {
    // The ruling's case: a connection line headed by the direction word `nc`.
    let hits = diags_for("    nc N1 -> D1.A;");
    assert_eq!(
        hits.len(),
        1,
        "`nc N1 -> D1.A` must report E{CODE} exactly once, got: {hits:?}"
    );
    assert!(
        hits[0].msg.contains("N1"),
        "diagnostic must name the offending phrase, got: {}",
        hits[0].msg
    );
}

#[test]
fn sem_portrowconn__in_head_reports() {
    // The twin hole: every other iotype word lands in the same production and
    // is read the same way. The judgment keys off the *shape*, not the word,
    // so this must fire the same code.
    let hits = diags_for("    in N1 -> D1.A;");
    assert_eq!(
        hits.len(),
        1,
        "`in N1 -> D1.A` must report E{CODE} exactly once, got: {hits:?}"
    );
}

#[test]
fn sem_portrowconn__plain_port_rows_are_silent() {
    // Three legal port-row shapes, one per operand kind the reader consumes:
    // a bare iotype word, the `nc` direction word, and a power terminal.
    let hits = diags_for("    io N1;\n    nc N2;\n    psnk dc24v;");
    assert!(
        hits.is_empty(),
        "legal port rows must stay silent, got: {hits:?}"
    );
}

#[test]
fn sem_portrowconn__trailing_attr_row_is_silent() {
    // `mc_tattrs_opt` is the third child of MCAST_NET_PORTS; identity words
    // (`@class` / `@return` / `@bind_role`) are re-captured by `parse_port`
    // downstream and must not be mistaken for a connection phrase.
    let hits = diags_for("    out N4 @class(analog);\n    io MIC{P,N} @return(GNDA);");
    assert!(
        hits.is_empty(),
        "trailing identity words must stay silent, got: {hits:?}"
    );
}

#[test]
fn sem_portrowconn__plain_connection_stays_silent() {
    // The control: an arrow-led line is an ordinary connection (MCAST_NET, a
    // different clause) and must never be caught by this judgment.
    let hits = diags_for("    N2 -> D1.A;\n    N3 -> D1.B;");
    assert!(
        hits.is_empty(),
        "ordinary connections must stay silent, got: {hits:?}"
    );
}
