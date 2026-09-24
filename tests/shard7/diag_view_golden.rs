// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Golden baseline for the `diagnostics` projection (CIMP §1 U280, second
//! half first slice).
//!
//! The view's serialized payload is an exact-byte snapshot stored at
//! `tests/golden/diagnostics_view.expected.json`. Any change that adds,
//! removes, re-sites or re-orders an item — or touches the envelope's stable
//! fields — fails the test on purpose, so the change is a reviewed contract
//! change rather than a silent drift.
//!
//! The three identity fields that move for reasons outside the contract are
//! normalized before comparison: `world_ver` / `top_ver` (a hashing change
//! must not masquerade as a payload change) and `mcc_version` (otherwise
//! every BUILDNr bump would rewrite the golden).
//!
//! Regenerate (review the diff — a new item means a deliberate contract
//! change): `DIAG_VIEW_UPDATE=1 cargo test --test shard7 diag_view_golden`.

use crate::common;

use mcc::stages::diagview::{self, DiagItem, DiagLoc, Level};
use mcc::stages::payload::StageViewData;

struct Case {
    name: &'static str,
    src: &'static str,
}

const CASES: &[Case] = &[
    Case {
        name: "clean",
        // Everything resolves: no diagnostics at all — the empty reading is
        // itself part of the contract (the counts still name every level).
        src: "component R {\n    pins = [\n        1 = A\n    ]\n}\nmodule main {\n    io VDD\n    R r1;\n    func main() {\n        r1 -> VDD\n    }\n}",
    },
    Case {
        name: "broken",
        // `r1.NOPIN` — a loud parse-time finding, plus whatever the flat
        // pass2 run logs for the surviving statements: several items across
        // more than one level, in the view's stated order.
        src: "component R {\n    pins = [\n        1 = A\n        2 = B\n    ]\n}\nmodule main {\n    io VDD\n    R r1;\n    func main() {\n        r1.NOPIN -> VDD\n    }\n}",
    },
];

/// Build one case in a fresh workspace and return the normalized payload.
/// Entry resolution mirrors `mcc check` (`check_one_world`): the module named
/// by the URI, else the first module in the file, else "main"; one tolerated
/// flat pass2 run so the net/ERC findings reach the store like a real check.
fn build_case(c: &Case) -> StageViewData {
    common::reset();
    let uri = format!("/mcc/diag-view-{}.mc", c.name);
    mcc::mcc_load_from_string(&uri, c.src);
    let mod_name = mcc::mcb_get_module_name_by_uri(&uri)
        .or_else(mcc::mcb_get_first_module_name)
        .unwrap_or_else(|| "main".to_string());
    let entry = mcc::McSpaceName {
        ident: mcc::McIds::from(mod_name.as_str()),
        uri: mcc::uri_intern(&uri),
    };
    let _ = mcc::mcb_pass2_flat(&entry, 1);
    let diags = mcc::mcc_diagnose_all();
    let view = diagview::diagnostics_view(&mod_name, &diags);

    let mut payload = StageViewData::from(&view);
    payload.world_ver = None;
    payload.top_ver = None;
    payload.mcc_version = "0.0.0-test".to_string();
    payload
}

fn golden_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/diagnostics_view.expected.json")
}

#[test]
fn diagnostics_view__golden_matches_baseline() {
    let _g = common::lock();
    let update = std::env::var("DIAG_VIEW_UPDATE").is_ok();

    let mut actual = serde_json::Map::new();
    for c in CASES {
        let payload = build_case(c);
        let v = serde_json::to_value(&payload).expect("StageViewData serializes");
        actual.insert(c.name.to_string(), v);
    }
    let actual = serde_json::Value::Object(actual);

    let path = golden_path();
    if update {
        let mut s = serde_json::to_string_pretty(&actual).expect("serialize golden");
        s.push('\n');
        std::fs::write(&path, s).expect("write golden");
        return;
    }
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing golden — run `DIAG_VIEW_UPDATE=1 cargo test --test shard7 diag_view_golden` \
             to (re)generate: {e}"
        )
    });
    let expected: serde_json::Value = serde_json::from_str(&raw).expect("parse golden");
    assert_eq!(
        actual, expected,
        "the diagnostics view's bytes drifted — inspect tests/golden/diagnostics_view.expected.json; \
         if the change is deliberate, regenerate with DIAG_VIEW_UPDATE=1"
    );
}

/// The branches the engine cannot reach today still have to be pinned: the
/// three reserved optional members serialize only when present, all four
/// level words spell as the CDDL spells them, and a missing extent serializes
/// `span` as `null` rather than being invented.
#[test]
fn diagnostics_view__reserved_members_and_level_words_serialize_as_contracted() {
    let _g = common::lock();
    let full = DiagItem {
        code: "E4057".to_string(),
        level: Level::Advisory,
        msg: "open lead".to_string(),
        loc: DiagLoc {
            uri: "/mcc/x.mc".to_string(),
            line: 7,
            span: Some(3),
        },
        fix_hint: Some("connect the lead".to_string()),
        net: Some("N45".to_string()),
        pin: Some("1".to_string()),
    };
    assert_eq!(
        serde_json::json!({
            "code": "E4057",
            "level": "advisory",
            "msg": "open lead",
            "loc": { "uri": "/mcc/x.mc", "line": 7, "span": 3 },
            "fix_hint": "connect the lead",
            "net": "N45",
            "pin": "1",
        }),
        serde_json::to_value(&full).expect("DiagItem serializes"),
    );

    let minimal = DiagItem {
        code: "E1000".to_string(),
        level: Level::Error,
        msg: "boom".to_string(),
        loc: DiagLoc {
            uri: "/mcc/x.mc".to_string(),
            line: 1,
            span: None,
        },
        fix_hint: None,
        net: None,
        pin: None,
    };
    assert_eq!(
        serde_json::json!({
            "code": "E1000",
            "level": "error",
            "msg": "boom",
            "loc": { "uri": "/mcc/x.mc", "line": 1, "span": null },
        }),
        serde_json::to_value(&minimal).expect("DiagItem serializes"),
    );

    for (level, word) in [
        (Level::Error, "error"),
        (Level::Warning, "warning"),
        (Level::Advisory, "advisory"),
        (Level::Info, "info"),
    ] {
        let item = DiagItem {
            code: "E0000".to_string(),
            level,
            msg: "m".to_string(),
            loc: DiagLoc {
                uri: "/mcc/x.mc".to_string(),
                line: 1,
                span: None,
            },
            fix_hint: None,
            net: None,
            pin: None,
        };
        let v = serde_json::to_value(&item).expect("DiagItem serializes");
        assert_eq!(v["level"], word, "level word drift for {level:?}");
    }
}
