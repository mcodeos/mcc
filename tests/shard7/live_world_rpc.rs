// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The live-world residency faces over RPC (U389 P1,
//! `live-world-residency-design.md`): the `caps` handshake verdicts (§4.3)
//! and the `if_version` conditional read on the six projection methods
//! (§3.3). The conditional read is the correctness seam of ruling 5 — the
//! tokens hash the source text, so a hit proves the disk the client cached
//! against is still the disk being read, with no watcher involved.

use crate::common;
use serde_json::{json, Value};
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

/// The identity triple this very binary was built with — the values a real
/// client would read from its own `mcc --version`.
fn self_triple() -> Value {
    json!({
        "protocol": mcc::buildinfo::RPC_PROTOCOL,
        "mcc_version": mcc::buildinfo::VERSION,
        "build": mcc::buildinfo::number(),
    })
}

/// Seed a tiny project and return the full `show.org-units` payload (no
/// pass2 on this face), with the live token pair a client would cache.
fn seed_and_read_full() -> (Value, String, String) {
    common::reset();
    let uri = mcc::McURI::from("/mcc/live-world-rpc.mc");
    mcc::mcc_load_from_string(
        &uri,
        "module main {\n    func M() {\n        a -> b\n    }\n}\n",
    );
    let payload = registry()
        .call("show.org-units", None)
        .expect("show.org-units call");
    let world = payload["world_ver"]
        .as_str()
        .expect("world_ver on the full payload")
        .to_string();
    let top = payload["top_ver"]
        .as_str()
        .expect("top_ver on the full payload")
        .to_string();
    (payload, world, top)
}

/// The legacy no-params call stays the plain capability sheet: no verdict is
/// invented for a client that never identified itself.
#[test]
fn live_world__caps_without_params_stays_the_plain_sheet() {
    let _lock = common::lock();
    common::reset();
    let caps = registry().call("caps", None).expect("caps call");
    assert!(
        caps.get("handshake").is_none(),
        "no-params caps must stay the legacy sheet: {caps}"
    );
    assert_eq!(caps["protocol"], mcc::buildinfo::RPC_PROTOCOL);
}

/// A matching triple reads `ok`; each drifted field gets its own verdict, and
/// every refusal carries the restart hint. Same version but a different build
/// refuses (`stale_build`) — revision tokens fold BUILD, so caches must not
/// straddle builds.
#[test]
fn live_world__caps_handshake_verdicts() {
    let _lock = common::lock();
    common::reset();

    let call = |client: Value| {
        registry()
            .call("caps", Some(json!({ "client": client })))
            .expect("caps call")["handshake"]
            .clone()
    };

    let ok = call(self_triple());
    assert_eq!(ok["verdict"], "ok");
    assert!(ok.get("restart_hint").is_none(), "ok carries no hint: {ok}");

    let mut wrong_protocol = self_triple();
    wrong_protocol["protocol"] = json!("mcc-rpc/0");
    let hs = call(wrong_protocol);
    assert_eq!(hs["verdict"], "protocol_mismatch");
    assert_eq!(hs["restart_hint"], "mcc restart");

    let mut wrong_version = self_triple();
    wrong_version["mcc_version"] = json!("0.0.1");
    let hs = call(wrong_version);
    assert_eq!(hs["verdict"], "version_mismatch");
    assert_eq!(hs["restart_hint"], "mcc restart");

    let mut stale_build = self_triple();
    stale_build["build"] = json!(mcc::buildinfo::number() + 1);
    let hs = call(stale_build);
    assert_eq!(hs["verdict"], "stale_build");
    assert_eq!(hs["restart_hint"], "mcc restart");

    let unverified = call(json!({}));
    assert_eq!(unverified["verdict"], "unverified");
}

/// A hit under either live token — the whole-world `world_ver` or the
/// per-top `top_ver` — returns the cheap `unchanged` answer with no
/// projection items, and an unknown token falls through to the full payload.
#[test]
fn live_world__if_version_hits_under_either_token() {
    let _lock = common::lock();
    let (full, world, top) = seed_and_read_full();
    assert!(
        full["items"].is_array(),
        "the full payload carries the projection: {full}"
    );

    for token in [world, top] {
        let hit = registry()
            .call(
                "show.org-units",
                Some(json!({ "if_version": token })),
            )
            .expect("conditional call");
        assert_eq!(hit["unchanged"], true, "token {token} must hit: {hit}");
        assert!(
            hit.get("items").is_none(),
            "a hit runs no projection: {hit}"
        );
        assert_eq!(hit["view"], "org-units");
        assert_eq!(hit["top"], "main");
        // The hit echoes the live pair, so the client can refresh its cache
        // key without a second full read.
        assert_eq!(hit["world_ver"].as_str(), Some(full["world_ver"].as_str().unwrap()));
        assert_eq!(hit["top_ver"].as_str(), Some(full["top_ver"].as_str().unwrap()));
    }

    let miss = registry()
        .call("show.org-units", Some(json!({ "if_version": "w_0000000000000000" })))
        .expect("conditional call");
    assert!(
        miss.get("unchanged").is_none(),
        "an unknown token must fall through to the full read: {miss}"
    );
    assert!(miss["items"].is_array());
}

/// The second face of ruling 5: the source text is part of the token, so an
/// edit to the seeded module invalidates the client's cached token and the
/// next conditional read returns the full payload again.
#[test]
fn live_world__if_version_misses_after_source_change() {
    let _lock = common::lock();
    let (_full, world, _top) = seed_and_read_full();

    // Rewrite the module body: same URIs, different content, new token.
    let uri = mcc::McURI::from("/mcc/live-world-rpc.mc");
    mcc::mcc_load_from_string(
        &uri,
        "module main {\n    func M() {\n        a -> b -> c\n    }\n}\n",
    );
    let hit = registry()
        .call("show.org-units", Some(json!({ "if_version": world })))
        .expect("conditional call after edit");
    assert!(
        hit.get("unchanged").is_none(),
        "the edit must invalidate the stale token: {hit}"
    );
    assert!(hit["items"].is_array());
    let new_world = hit["world_ver"].as_str().expect("world_ver after edit");
    assert_ne!(new_world, world, "the token must move when the source moves");
}
