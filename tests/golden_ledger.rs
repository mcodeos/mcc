// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Golden ledger baselines (resolve-gate-design.md §7.3, §7.5 item 12).
//!
//! Each corpus case's ledger rows are an exact-set snapshot stored at
//! `tests/golden/<case>.ledger.expected.json`. Any resolution change that
//! adds, removes, renames or re-sites a row fails the test — a silent
//! fallback with a never-seen-before `form×site` breaks CI on purpose, so the
//! new row is a reviewed, deliberate change rather than a silent regression.
//!
//! Sites are normalized (leading `<path>:<line> ` stripped) so the baseline
//! survives code-line drift; the stable part is the semantic descriptor
//! (e.g. `add_bus ghost-bus`, `eval_port_elems right shape_mismatch`).
//! Non-location sites (component names, `net-point`) pass through unchanged.
//!
//! The ledger is observation-only — these tests never change resolution
//! semantics.
//!
//! Regenerate (review the diff — a new row means a deliberate resolution
//! change): `LEDGER_UPDATE=1 cargo test --test golden_ledger`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use mcc::ledger::{self, LedgerMode};

/// Global mutex to serialize tests that share mcc's global workspace state
/// (the ledger is process-global too — each case clears it before building).
static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Acquire the shared test lock, recovering from a prior test's panic (a
/// panicked assert poisons the mutex while unwinding).
fn lock() -> std::sync::MutexGuard<'static, ()> {
    TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// One self-contained corpus case. Each source has no library or project
/// dependency, so rows are deterministic and stable.
struct Case {
    name: &'static str,
    src: &'static str,
}

const CASES: &[Case] = &[
    Case {
        name: "ghost_bus",
        // `MISSING.PIN` — two-segment dot on an undeclared base → ghost bus.
        src: "module main {\n    io VDD\n    func main() {\n        MISSING.PIN -> VDD\n    }\n}",
    },
    Case {
        name: "curly_ghost_bus",
        // `dc{...}` — curly member list on an undeclared base → curly ghost bus.
        src: "module main {\n    io VDD\n    func main() {\n        dc{VDD_3V3, GND} -> VDD\n    }\n}",
    },
    Case {
        name: "this_multi",
        // `this.y.2` — D9 multi-segment `this` access → tail dropped, literal label.
        src: "component T {\n    pins = [ 1 = A ]\n    func main() {\n        this.y.2 -> A\n    }\n}\nmodule main { io VDD }",
    },
    Case {
        name: "group_shape_mismatch",
        // `([GND, X], r1)` — unequal branch widths → `<error:shape_mismatch>`.
        src: "component R {\n    pins = [\n        1 = A\n        2 = B\n    ]\n}\nmodule main {\n    io GND\n    R r1;\n    func main() {\n        ([GND, X], r1) -> GND\n    }\n}",
    },
    Case {
        name: "bare_miss_wire",
        // `pwr -> DC` — a bare identifier referenced exactly once → floating net label.
        src: "component FLT(pwr) {\n    func F(pwr) {\n        pwr -> DC\n    }\n}\nmodule main { io VDD }",
    },
    Case {
        name: "phantom_port",
        // `module MIC_SIP(dc{VDD_3V3, GND}::DC(3.3V))` with a curly `out` — the
        // curly port/out names reach the net layer and are quarantined. Entry
        // resolves to MIC_SIP (first module in the file).
        src: "component DC { pins = [ 1 = A ] }\nmodule MIC_SIP(dc{VDD_3V3, GND}::DC(3.3V))\n{\n    out MIC{P, N}::ADC.DIFF()\n}",
    },
    Case {
        name: "clean_declared",
        // Everything resolves to something declared: no non-clean parse, no rows.
        src: "component R {\n    pins = [\n        1 = A\n    ]\n}\nmodule main {\n    io VDD\n    R r1;\n    func main() {\n        r1 -> VDD\n    }\n}",
    },
    Case {
        name: "unresolved_pin",
        // `r1.NOPIN` — a two-segment dot on a declared component whose member is
        // not a pin → loud E3179 at parse time, statement dropped. UnresolvedRef
        // (action=error), base hit / member fails (§1.2③).
        src: "component R {\n    pins = [\n        1 = A\n        2 = B\n    ]\n}\nmodule main {\n    io VDD\n    R r1;\n    func main() {\n        r1.NOPIN -> VDD\n    }\n}",
    },
    Case {
        name: "unresolved_curly_iface",
        // `MISSING.IF{A, B}` — a curly interface-member access with an undeclared
        // base → loud IFACE_CURLY_MEMBER_INVALID, statement dropped. UnresolvedRef
        // (action=error).
        src: "module main {\n    io VDD\n    func main() {\n        MISSING.IF{A, B} -> VDD\n    }\n}",
    },
];

/// Build one case in a fresh workspace, leaving the ledger populated for the
/// caller to snapshot. Entry resolution mirrors `mcc check` (§7.2): the
/// module named by the URI, else the first module in the file, else "main".
fn build_case(c: &Case) -> ledger::LedgerReport {
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();
    ledger::clear();
    let uri = format!("/mcc/golden-{}.mc", c.name);
    mcc::mcc_load_from_string(&uri, c.src);
    let mod_name = mcc::mcb_get_module_name_by_uri(&uri)
        .or_else(mcc::mcb_get_first_module_name)
        .unwrap_or_else(|| "main".to_string());
    let _ = mcc::mcc_build(&mcc::McIds::from(mod_name.as_str()), &uri);
    ledger::build_report(LedgerMode::Audit)
}

/// Strip the leading `<path>:<line> ` token from a site string so the golden
/// is stable across code-line drift; the descriptor (e.g. `add_bus ghost-bus`)
/// is the stable semantic identity. Sites without a location prefix — a
/// component name (Wire), `net-point` (Phantom) — pass through unchanged.
fn normalize_site(site: &str) -> String {
    if let Some((_, rest)) = site.split_once(':') {
        if rest.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            if let Some((_, desc)) = rest.split_once(' ') {
                return desc.to_string();
            }
        }
    }
    site.to_string()
}

/// One detail row, normalized for golden comparison (sorted for determinism).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
struct GoldenRow {
    kind: String,
    form: String,
    site: String,
}

/// Full per-case snapshot: summary + sorted detail rows.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct GoldenCase {
    total: usize,
    survived: usize,
    resolved_late: usize,
    by_kind_form: BTreeMap<String, BTreeMap<String, usize>>,
    rows: Vec<GoldenRow>,
}

impl GoldenCase {
    fn from_report(r: &ledger::LedgerReport) -> Self {
        let mut rows: Vec<GoldenRow> = r
            .detail
            .iter()
            .map(|row| GoldenRow {
                kind: row.kind.clone(),
                form: row.form.clone(),
                site: normalize_site(&row.site),
            })
            .collect();
        rows.sort();
        let by_kind_form: BTreeMap<String, BTreeMap<String, usize>> = r
            .by_kind_form
            .iter()
            .map(|(kind, forms)| {
                let f: BTreeMap<String, usize> = forms
                    .iter()
                    .filter(|(_, count)| **count > 0)
                    .map(|(form, count)| (form.clone(), *count))
                    .collect();
                (kind.clone(), f)
            })
            .collect();
        GoldenCase {
            total: r.total,
            survived: r.survived,
            resolved_late: r.resolved_late,
            by_kind_form,
            rows,
        }
    }
}

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

#[test]
fn golden_corpus_matches_baseline() {
    let _g = lock();
    let update = std::env::var("LEDGER_UPDATE").is_ok();
    let dir = golden_dir();
    for c in CASES {
        let report = build_case(c);
        let actual = GoldenCase::from_report(&report);
        let path = dir.join(format!("{}.ledger.expected.json", c.name));
        if update {
            std::fs::create_dir_all(&dir).expect("create golden dir");
            let mut s = serde_json::to_string_pretty(&actual).expect("serialize golden");
            s.push('\n');
            std::fs::write(&path, s).expect("write golden");
            continue;
        }
        let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "missing golden for `{}` — run `LEDGER_UPDATE=1 cargo test --test golden_ledger` to (re)generate: {e}",
                c.name
            )
        });
        let expected: GoldenCase = serde_json::from_str(&raw).expect("parse golden");
        assert_eq!(
            actual, expected,
            "golden baseline drifted for `{}` — a resolution change added/removed/renamed/re-sited \
             a ledger row. Inspect tests/golden/{}.ledger.expected.json; if the change is deliberate, \
             regenerate with LEDGER_UPDATE=1.",
            c.name, c.name
        );
    }
}

#[test]
fn corpus_exercises_all_live_kinds() {
    let _g = lock();
    let mut kinds = BTreeSet::new();
    for c in CASES {
        let report = build_case(c);
        for row in &report.detail {
            kinds.insert(row.kind.clone());
        }
    }
    for k in ["wire", "phantom", "fallback", "unresolved_ref"] {
        assert!(
            kinds.contains(k),
            "corpus must exercise the live kind `{k}` — a corpus case regressed or the recording \
             site changed"
        );
    }
}
