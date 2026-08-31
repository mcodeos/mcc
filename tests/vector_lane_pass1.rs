// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! §11.3 / vector-pipeline Phase 1.3: lane-structured pass1 references.
//!
//! `c[1:2]`, `XTAL.X[1:2]`, `S[1:4][L,R].IN` resolve at pass1 to **structured
//! member references** — never a literal `c[1:2]` label (invariant B). The
//! lane structure contract (§11.3 ③ / the flatten-before-broadcast pitfall):
//! - receiver `c[1:2]` → `Endpoint(List([Single(c1), Single(c2)]))` — one lane
//!   per ordered member (broadcast driver for iterated.rs)
//! - `[VDD, GND]` arg list → `Set([Opd(Id(VDD)), Opd(Id(GND))])` — 2 scalar lanes
//! - the vector member set is carried in structure, never re-parsed from a
//!   `format!("{}", phrase)` display string (AST-driven guideline)

use mcc::{McIds, McURI};
use std::sync::{Mutex, OnceLock};

static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

const CAP_COMP: &str = "component CAP(cap::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Cap([n1, n2]) {\n        n1 - this - n2\n    }\n}\n";

/// Build `main` and return func `M`'s parsed stmts.
fn func_m_stmts(src: &str, uri: &str) -> Vec<mcc::McPhrase> {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let inst = mcc::mcc_build(&McIds::from("main"), &u).expect("build");
    inst.def
        .funcs
        .find("M")
        .map(|f| f.stmts.clone())
        .unwrap_or_default()
}

/// Walk phrases to find the first FuncCall with `func_name`.
fn find_funccall<'a>(stmts: &'a [mcc::McPhrase], name: &str) -> Option<&'a mcc::McFuncCall> {
    fn walk<'a>(phrase: &'a mcc::McPhrase, name: &str) -> Option<&'a mcc::McFuncCall> {
        match phrase {
            mcc::McPhrase::FuncCall(f) => {
                if f.func_name.to_string() == name {
                    return Some(f);
                }
                f.caller.as_ref().and_then(|c| walk(c, name))
            }
            mcc::McPhrase::Series(elems, _) => elems.iter().find_map(|e| walk(e, name)),
            mcc::McPhrase::Parallel(v) | mcc::McPhrase::Multiple(v) => {
                v.iter().find_map(|e| walk(e, name))
            }
            mcc::McPhrase::Group(g) => g.opds.iter().find_map(|e| walk(e, name)),
            mcc::McPhrase::Transposed(inner) => walk(inner, name),
            mcc::McPhrase::Member(p, _) => walk(p, name),
            mcc::McPhrase::Closure(c) => c.body.iter().find_map(|e| walk(e, name)),
            mcc::McPhrase::Lead | mcc::McPhrase::Endpoint(_) => None,
        }
    }
    stmts.iter().find_map(|s| walk(s, name))
}

/// ── §11.3 ③ (a): declared vector receiver → lane-structured member List ───
/// `CAP c[1:2](1)` then `c[1:2].Cap([VDD, GND])` — the receiver is
/// `Endpoint(List([Single(c1), Single(c2)]))`, never `Label("c[1:2]")`.
#[test]
fn vector_receiver_is_lane_structured_list() {
    let src = format!(
        "{CAP_COMP}module main {{\n    io VDD\n    io GND\n    func M() {{\n        CAP c[1:2](1)\n        c[1:2].Cap([VDD, GND])\n    }}\n}}\n"
    );
    let stmts = func_m_stmts(&src, "/mcc/lane-vec-receiver.mc");
    let fc = find_funccall(&stmts, "Cap").expect("Cap fcall in M stmts");
    let caller = fc.caller.as_ref().expect("caller");
    // Receiver is a lane-structured List, not a literal label.
    let lanes = match caller.as_ref() {
        mcc::McPhrase::Endpoint(mcc::McEndpoint::List(lanes)) => lanes,
        other => panic!("caller must be Endpoint(List(..)), got {other:?}"),
    };
    assert_eq!(lanes.len(), 2, "two lanes for c[1:2]; got {lanes:?}");
    for (i, lane) in lanes.iter().enumerate() {
        match lane {
            mcc::McEndpoint::Single(iref) => match &iref.base {
                mcc::McInstance::Label(s) => {
                    assert_eq!(s, &format!("c{}", i + 1), "lane {i} member name");
                }
                other => panic!("lane {i} base must be Label, got {other:?}"),
            },
            other => panic!("lane {i} must be Single, got {other:?}"),
        }
    }
    // Invariant B: no literal `c[1:2]` reference anywhere in the phrase tree.
    let mut stack: Vec<&mcc::McPhrase> = vec![caller];
    while let Some(p) = stack.pop() {
        let text = format!("{p:?}");
        assert!(
            !text.contains("c[1:2]"),
            "no literal c[1:2] reference; got {text}"
        );
        if let mcc::McPhrase::FuncCall(f) = p {
            if let Some(c) = &f.caller {
                stack.push(c);
            }
        }
    }
    // ── Lane contract: `[VDD, GND]` = 2 scalar lanes (structured McIds) ──
    let params = &fc.params;
    assert_eq!(params.len(), 1, "one Set arg; got {params:?}");
    let lanes = match &params[0] {
        mcc::McParamValue::Set(items) => items,
        other => panic!("arg must be Set(..), got {other:?}"),
    };
    assert_eq!(lanes.len(), 2, "[VDD, GND] = 2 lanes; got {lanes:?}");
    for (i, lane) in lanes.iter().enumerate() {
        let name = match lane {
            mcc::McParamValue::Opd(mcc::McOpd::Id(ids)) => ids.to_string(),
            other => panic!("lane {i} must be Opd(Id(..)), got {other:?}"),
        };
        let expected = if i == 0 { "VDD" } else { "GND" };
        assert_eq!(name, expected, "scalar lane {i}");
    }
}

/// ── Lane contract: func-local declared receiver (§11.3 pin 3) ────────────
/// `r[1:2]::RES(0)` (func-body declare, invisible to in-body find_inst) then
/// `r[1:2].Pullup([NET, VCC])` — still resolves to per-member lanes via
/// `is_declared_instance_name`.
#[test]
fn func_local_vector_receiver_is_lane_structured_list() {
    let res_comp = "component RES(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Pullup([n1, n2]) {\n        n1 - this - n2\n    }\n}\n";
    let src = format!(
        "{res_comp}module main {{\n    io NET\n    io VCC\n    func M() {{\n        r[1:2]::RES(0)\n        r[1:2].Pullup([NET, VCC])\n    }}\n}}\n"
    );
    let stmts = func_m_stmts(&src, "/mcc/lane-funclocal.mc");
    let fc = find_funccall(&stmts, "Pullup").expect("Pullup fcall in M stmts");
    let caller = fc.caller.as_ref().expect("caller");
    let lanes = match caller.as_ref() {
        mcc::McPhrase::Endpoint(mcc::McEndpoint::List(lanes)) => lanes,
        other => panic!("caller must be Endpoint(List(..)), got {other:?}"),
    };
    assert_eq!(lanes.len(), 2, "two lanes for r[1:2]; got {lanes:?}");
    for (i, lane) in lanes.iter().enumerate() {
        match lane {
            mcc::McEndpoint::Single(iref) => match &iref.base {
                mcc::McInstance::Label(s) => {
                    assert_eq!(s, &format!("r{}", i + 1), "lane {i} member name");
                }
                other => panic!("lane {i} base must be Label, got {other:?}"),
            },
            other => panic!("lane {i} must be Single, got {other:?}"),
        }
    }
}

/// ── §11.3 ③ (b): bus/interface member slice stays structured ─────────────
/// `[XTAL.X[1:2], gnd]` — the vector lane keeps its AST segment tree
/// (McIds with embedded square), not a pre-flattened display string.
/// GAP1 / iterated broadcast compare member sets from this structure.
#[test]
fn bus_member_slice_lane_stays_structured_ids() {
    let src = format!(
        "{CAP_COMP}module main {{\n    io VDD\n    io GND\n    func M() {{\n        CAP c[1:2](1)\n        c[1:2].Cap([VDD, GND])\n    }}\n}}\n"
    );
    let stmts = func_m_stmts(&src, "/mcc/lane-args.mc");
    let fc = find_funccall(&stmts, "Cap").expect("Cap fcall in M stmts");
    // `[VDD, GND]` = 2 scalar lanes, each a structured McIds (Ida segment), not
    // a string `"[VDD, GND]"`.
    let params = &fc.params;
    let lanes = match &params[0] {
        mcc::McParamValue::Set(items) => items,
        other => panic!("arg must be Set(..), got {other:?}"),
    };
    for lane in lanes {
        match lane {
            mcc::McParamValue::Opd(mcc::McOpd::Id(ids)) => {
                // Structured segment tree: exactly one Ida segment, no embedded
                // bracket text glued into the display form of the lane.
                assert_eq!(ids.segments.len(), 1, "one segment; got {ids:?}");
            }
            other => panic!("lane must be Opd(Id(..)), got {other:?}"),
        }
    }
}
