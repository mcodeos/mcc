// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The `diagnostics` RPC contract (the channel mcext consumes): it carries the
//! fields the `caps` handshake advertises under `features.diagnostics` --
//! `end_line`/`end_column` (1-based, computed from `pos + len` through mcc's
//! line index, so a span may cross lines) and the `suggestions`/`related`
//! arrays (empty until a producer fills `Diagnostic.other`).
//!
//! Before the contract landed, the channel dropped the end position and the
//! extension clamped every range to the start line, truncating real
//! multi-line diagnostics (mcext gap-audit §1.1/§1.2).

use crate::common;
use serde_json::Value;
use std::sync::Arc;
use std::sync::OnceLock;

use mcc::rpc::RpcServerBuilder;

/// One process-wide registry: `register_all` is the single source of truth the
/// live server wires, so a renamed or re-signed handler fails here first.
fn registry() -> Arc<mcc::rpc::protocol::RpcMethodRegistry> {
    static REG: OnceLock<Arc<mcc::rpc::protocol::RpcMethodRegistry>> = OnceLock::new();
    REG.get_or_init(|| {
        mcc::rpc::handlers::register_all(RpcServerBuilder::new())
            .build()
            .registry()
    })
    .clone()
}

/// Build `src` and return the raw `diagnostics` RPC rows.
fn rpc_diagnostics(src: &str) -> Vec<Value> {
    common::reset();
    let uri = mcc::McURI::from("/mcc/diag-rpc-contract.mc");
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build_with_nets(&mcc::McIds::from("main"), &uri);
    let result = registry().call("diagnostics", Some(serde_json::json!({ "uri": uri })));
    let value = result.expect("diagnostics RPC call");
    value["diagnostics"]
        .as_array()
        .expect("diagnostics array")
        .clone()
}

/// Every row carries the advertised fields, and the 1-based end never sits
/// before the 1-based start.
#[test]
fn diag_rpc__every_row_carries_the_advertised_fields() {
    let _lock = common::lock();
    let rows = rpc_diagnostics(
        "component F {\n    pins = [\n        1 = 1\n    ]\n}\nmodule main {\n    F f1\n    func M() {\n        ghost_net -> f1.1\n    }\n}\n",
    );
    assert!(
        !rows.is_empty(),
        "the fixture must produce at least one diagnostic"
    );
    for d in &rows {
        let line = d["location"]["line"].as_u64().expect("1-based line");
        let column = d["location"]["column"].as_u64().expect("1-based column");
        let end_line = d["end_line"].as_u64().expect("end_line on the channel");
        let end_column = d["end_column"]
            .as_u64()
            .expect("end_column on the channel");
        assert!(
            end_line > line || (end_line == line && end_column >= column),
            "1-based end must not precede the start: {d}"
        );
        assert!(
            d["suggestions"].is_array() && d["related"].is_array(),
            "suggestions/related ride the channel even when empty: {d}"
        );
    }
}

/// The end position the channel carries is the true end of the span: for an
/// anchor that crosses a line boundary, `Location::end_row` (serialized as
/// `end_line`) lands on the next line. Today's producers anchor tokens and
/// points, so no live diagnostic spans lines yet -- this locks the value the
/// unclamped channel now forwards, at the constructor every diagnostic goes
/// through (the layer's own contract, u124-style).
#[test]
fn diag_rpc__multiline_span_end_crosses_lines() {
    let _lock = common::lock();
    let src = "module main {\n    func M() {\n        alpha ->\n            beta\n    }\n}\n";
    let uri = mcc::McURI::from("/mcc/diag-rpc-contract-multi.mc");
    common::reset();
    mcc::mcc_load_from_string(&uri, src);

    // From the start of `alpha` to the end of `beta`: two lines.
    let start = src.find("alpha").expect("alpha in fixture") as u32;
    let end = src.find("beta").expect("beta in fixture") as u32 + 4;
    let loc = mcc::McLocation::new(uri, start, end - start);
    assert!(
        loc.end_row > loc.row,
        "a two-line anchor must end on a later line: start {}/{} end {}/{}",
        loc.row,
        loc.col,
        loc.end_row,
        loc.end_col
    );
}
