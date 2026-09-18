// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! build-design §3.7 **discipline 4** over every product the CLI emits.
//!
//! A product must have an order that its input alone determines. Discipline 0 ②
//! says this of the **id issuance** side; §3.7's note adds the second, independent
//! source of nondeterminism the products carry — the **traversal order of the
//! containers they walk** — and records that nobody had scanned for it. This file
//! is that scan's lock.
//!
//! The scan found two offenders, in two different layers, which is why this file
//! covers readouts as well as exports:
//!
//! * `spice` — `build_spice` accumulated `inst_nodes` in a `HashMap` and then
//!   **iterated it to emit the `X<name> <net> <net>` lines**, so a `HashMap`'s
//!   per-process order became the file's line order. Every other exporter in the
//!   module already keys on a `BTreeMap` (`netmap` above, and all of `bom` /
//!   `kicad`), so the fix moved it onto the module's existing convention.
//! * `show lapper` — `symbol_table_to_json` emits four arrays built from
//!   `HashMap`s (`local.declares`, `ref_def_map.entries`, `ref_def_map.def_to_refs`,
//!   and the `result_id` that hashes one entry picked by `.next()`). Two of those
//!   arrays have hundreds of members, so the whole payload was redrawn per
//!   process. A readout is a product too, and is held to the same rule.
//!
//! ⚠ **What this does and does not assert.** Some of these products stamp the
//! moment they were written (`# Generated: epoch=…` / `* Generated: …` /
//! `(date "epoch=…")`). A clock is not a traversal order and does not violate
//! discipline 4, which asks for a *total order*, not for byte-identity — but it
//! does defeat byte-reconciliation on those files, and whether an export should
//! carry its generation time is a separate, open question (CIMP §1 U92). So the
//! clock line is **masked, not asserted away**: this file locks the ordering
//! property it is about, and does not quietly bless the clock by demanding it.

use std::path::{Path, PathBuf};
use std::process::Command;

fn hbl_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hbl")
}

/// A fresh, **empty** directory to run a CLI invocation in, so nothing the
/// command reads can depend on where the test happens to be.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mcc-prodorder-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// Run `mcc --local <args…>` from `cwd`; returns `(stdout, stderr, exit_ok)`.
///
/// `--local` on every call: without it a running `mcc start` service answers
/// instead, and the daemon holds its own world (an old library, an old binary),
/// which is exactly the second opinion a determinism test must not admit.
fn run_mcc(cwd: &Path, args: &[&str]) -> (String, String, bool) {
    let mut full = vec!["--local"];
    full.extend_from_slice(args);
    let out = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .current_dir(cwd)
        .args(&full)
        .output()
        .expect("run mcc");
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
        out.status.success(),
    )
}

/// Drop the generation-time line, so the comparison is about order and content.
///
/// Every form the four stamping products use is listed: a missing one would not
/// fail the test loudly, it would make the test pass for the wrong reason.
fn without_clock(s: &str) -> String {
    s.lines()
        .filter(|l| !(l.contains("Generated: epoch=") || l.contains("(date \"epoch=")))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every export kind, both faces where a product has two.
const PRODUCTS: &[&[&str]] = &[
    &["export", "netlist"],
    &["export", "netlist", "--format", "csv"],
    &["export", "netlist", "--json"],
    &["export", "bom"],
    &["export", "bom", "--json"],
    &["export", "spice"],
    &["export", "kicad"],
    &["export", "inst-list"],
    &["export", "inst-list", "--json"],
];

/// The same design exported twice is the same file.
///
/// Two **processes**, not two calls: a `HashMap`'s iteration order is drawn from
/// a per-process seed, so a same-process comparison cannot see the defect this
/// locks. Reading it back out — the assertion is over the whole product, not a
/// count, because a stable count with shuffled lines is exactly the failure mode.
#[test]
fn every_export_product_is_the_same_file_twice() {
    let cwd = scratch("export-products");
    let target = hbl_dir();
    let t = target.to_str().expect("fixture path");

    for args in PRODUCTS {
        let mut argv: Vec<&str> = args.to_vec();
        argv.push(t);

        let (first, err1, ok1) = run_mcc(&cwd, &argv);
        let (second, err2, ok2) = run_mcc(&cwd, &argv);
        assert!(ok1 && ok2, "`mcc {}` failed: {err1}{err2}", argv.join(" "));

        // A product with nothing in it would make the comparison below vacuous.
        assert!(
            first.len() > 200,
            "`mcc {}` produced {} bytes — too small to be the product under test",
            argv.join(" "),
            first.len()
        );

        assert_eq!(
            without_clock(&first),
            without_clock(&second),
            "`mcc {}` gave two different products for one input — its order is not \
             determined by the input (build-design §3.7 discipline 4)",
            argv.join(" ")
        );
    }
}

/// The `spice` netlist — the product that actually regressed — held to the same
/// rule over more runs than the sweep spends on it.
///
/// Shuffling is the *only* degree of freedom a `HashMap` iteration has, and the
/// fixture's netlist is long enough that two runs agreeing is not something
/// chance explains: agreement over `RUNS` runs of a product with `MIN_LINES`
/// independently-ordered lines is agreement about the rule, not about the draw.
/// Both numbers are asserted rather than assumed, so a fixture that shrank would
/// weaken this lock loudly instead of silently.
#[test]
fn the_spice_netlist_order_survives_repeated_runs() {
    const RUNS: usize = 3;
    const MIN_LINES: usize = 20;

    let cwd = scratch("spice-order");
    let target = hbl_dir();
    let t = target.to_str().expect("fixture path");

    let mut products = Vec::new();
    for _ in 0..RUNS {
        let (out, err, ok) = run_mcc(&cwd, &["export", "spice", t]);
        assert!(ok, "`export spice` failed: {err}");
        products.push(without_clock(&out));
    }

    // The emitter's lines are `X<instance> <net> <net>`; `.SUBCKT` / `.END` are
    // the file's frame, not instances.
    let lines = products[0]
        .lines()
        .filter(|l| l.starts_with('X') && l.split(' ').count() == 3)
        .count();
    assert!(
        lines >= MIN_LINES,
        "the fixture's spice netlist has {lines} instance lines, below the {MIN_LINES} \
         this lock's reasoning needs"
    );

    for (i, p) in products.iter().enumerate().skip(1) {
        assert_eq!(
            &products[0], p,
            "`export spice` run 0 and run {i} disagree — the netlist's order comes \
             from a container whose order the input does not fix"
        );
    }
}

/// `show lapper` — a **readout**, and held to the same rule.
///
/// Two properties, because stability alone would also be satisfied by a
/// constant-but-arbitrary order: the payload is the same across processes
/// **and** the declaration list reads in source order. The second is what makes
/// the first mean "determined by the input" rather than "frozen by accident".
#[test]
fn the_lapper_readout_is_the_same_file_twice() {
    const MIN_DECLARES: usize = 2;

    let cwd = scratch("lapper-order");
    // The entry file, not the project directory: `show lapper` takes a file.
    let target = hbl_dir().join("src/hbl.mc");
    let t = target.to_str().expect("fixture path");

    let mut runs = Vec::new();
    for _ in 0..3 {
        let (out, err, ok) = run_mcc(&cwd, &["show", "lapper", "-f", "json", t]);
        assert!(ok, "`show lapper` failed: {err}");
        runs.push(out);
    }
    for (i, r) in runs.iter().enumerate().skip(1) {
        assert_eq!(
            &runs[0], r,
            "`show lapper` run 0 and run {i} disagree — its tables are built from \
             `HashMap`s, so their iteration order became the payload's order"
        );
    }

    let payload: serde_json::Value =
        serde_json::from_str(&runs[0]).expect("lapper payload is JSON");
    let declares = payload["result"]["show"]["local"]["declares"]
        .as_array()
        .expect("`local.declares` is an array");
    assert!(
        declares.len() >= MIN_DECLARES,
        "the fixture declares {} names — too few for an order assertion to bite",
        declares.len()
    );

    let spans: Vec<(u64, u64)> = declares
        .iter()
        .map(|d| {
            (
                d["span"][0].as_u64().expect("span start"),
                d["span"][1].as_u64().expect("span end"),
            )
        })
        .collect();
    let mut ascending = spans.clone();
    ascending.sort_unstable();
    assert_eq!(
        spans, ascending,
        "`local.declares` is not in source order — the emitted order is not the \
         file's reading order"
    );

    // The reverse index is the array whose order was least obvious: it is keyed
    // by the definition, so a sort by key puts definitions in `(kind, file,
    // span)` order. Assert it is *sorted*, not merely stable.
    let refs = payload["result"]["show"]["ref_def_map"]["def_to_refs"]
        .as_array()
        .expect("`def_to_refs` is an array");
    let keys: Vec<(u64, u64, u64, u64)> = refs
        .iter()
        .map(|r| {
            (
                r["def_kind"].as_u64().expect("def_kind"),
                r["file_id"].as_u64().expect("file_id"),
                r["byte_start"].as_u64().expect("byte_start"),
                r["byte_end"].as_u64().expect("byte_end"),
            )
        })
        .collect();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(keys, sorted, "`def_to_refs` is not in key order");
}

/// Every pin list in a payload, found structurally: an array whose elements are
/// all objects carrying a string `id` and a string `name`.
///
/// Structural rather than by path on purpose — the point of this test is that
/// *every* pin listing obeys one rule, so a walk that only visited the arrays
/// somebody remembered to name would not be about that.
fn pin_lists(v: &serde_json::Value, out: &mut Vec<Vec<String>>) {
    match v {
        serde_json::Value::Array(items) => {
            let is_pin_list = !items.is_empty()
                && items.iter().all(|p| {
                    p.get("id").and_then(|i| i.as_str()).is_some()
                        && p.get("name").and_then(|n| n.as_str()).is_some()
                });
            if is_pin_list {
                out.push(
                    items
                        .iter()
                        .map(|p| p["id"].as_str().expect("pin id").to_string())
                        .collect(),
                );
            }
            for i in items {
                pin_lists(i, out);
            }
        }
        serde_json::Value::Object(map) => {
            for (_, i) in map {
                pin_lists(i, out);
            }
        }
        _ => {}
    }
}

/// The build readout's pin lists, held to discipline 4 — and to the rule.
///
/// Two **processes**, like the exports above: `McComponentInst.pins` is a
/// `HashMap`, and a collector that walked its `keys()` would look perfectly
/// stable inside one process and shuffle between two. That is not a
/// hypothetical — the delegated face of `mcc build` shipped exactly this, and it
/// disagreed with *itself* between consecutive requests, so a same-process
/// comparison would have called it green.
///
/// Stability alone would also be satisfied by an order that froze arbitrarily,
/// so the lists are read back and checked against `mcc::pin_id_cmp`: the
/// property under test is "determined by the design", not "constant".
#[test]
fn the_build_readout_lists_pins_in_one_order() {
    const MIN_LISTS: usize = 5;
    const MIN_PINS: usize = 3;

    let cwd = scratch("build-pin-order");
    let target = hbl_dir().join("src/hbl.mc");
    let t = target.to_str().expect("fixture path");

    let mut runs: Vec<Vec<Vec<String>>> = Vec::new();
    for _ in 0..2 {
        // The exit code is not this test's business: the fixture trips an
        // error-level electrical net check, so `build` reports failure while
        // still publishing its readout. Order is what is under test.
        let (out, err, _) = run_mcc(&cwd, &["build", t, "-f", "json"]);
        let payload: serde_json::Value = serde_json::from_str(&out)
            .unwrap_or_else(|e| panic!("`mcc build -f json` published no payload: {e}\n{err}"));
        let mut lists = Vec::new();
        pin_lists(&payload, &mut lists);
        runs.push(lists);
    }

    assert!(
        runs[0].len() >= MIN_LISTS,
        "the fixture's build readout has {} pin lists, below the {MIN_LISTS} this \
         lock's reasoning needs",
        runs[0].len()
    );
    assert!(
        runs[0].iter().any(|l| l.len() >= MIN_PINS),
        "no pin list in the fixture reaches {MIN_PINS} pins — an order assertion \
         over lists that short would not bite"
    );

    for (i, run) in runs.iter().enumerate().skip(1) {
        assert_eq!(
            &runs[0], run,
            "`mcc build` run 0 and run {i} list pins differently — the order comes \
             from a container the design does not order (build-design §3.7 \
             discipline 4)"
        );
    }

    for list in &runs[0] {
        let mut ordered = list.clone();
        ordered.sort_by(|a, b| mcc::pin_id_cmp(a, b));
        assert_eq!(
            list, &ordered,
            "a pin list is stable but not in the canonical order — a frozen \
             arbitrary order is not an order the input determines"
        );
    }
}

/// The rule itself, including the tie it has to break.
///
/// `natural_cmp` compares digit runs numerically and ignores leading zeros, so
/// `"1"` and `"01"` reach it equal. A comparator that stopped there would leave
/// those two in whatever order the container handed them over — the same defect
/// one level down. The last term is the raw text, and this is the assertion that
/// keeps it.
#[test]
fn the_pin_id_order_is_total() {
    use mcc::pin_id_cmp;

    // Numeric ids first, numerically — not lexicographically.
    let mut ids = vec!["10", "2", "1", "12", "11", "3"];
    ids.sort_by(|a, b| pin_id_cmp(a, b));
    assert_eq!(ids, vec!["1", "2", "3", "10", "11", "12"]);

    // Then non-numeric ids, digit runs compared numerically.
    let mut mixed = vec!["PA10", "A2", "B1", "PA0", "A10", "AB", "A1"];
    mixed.sort_by(|a, b| pin_id_cmp(a, b));
    assert_eq!(mixed, vec!["A1", "A2", "A10", "AB", "B1", "PA0", "PA10"]);

    // The tie `natural_cmp` leaves: only the raw text separates these.
    assert_eq!(pin_id_cmp("1", "01"), std::cmp::Ordering::Greater);
    assert_eq!(pin_id_cmp("01", "1"), std::cmp::Ordering::Less);
    assert_eq!(pin_id_cmp("1", "1"), std::cmp::Ordering::Equal);

    // Total: an antisymmetric comparator that never says "equal" about two
    // distinct ids is what lets a sort be a function of the input.
    let ids = ["1", "01", "001", "A1", "A01", "a1", "1A", "10", "2"];
    for a in ids {
        for b in ids {
            let ab = pin_id_cmp(a, b);
            let ba = pin_id_cmp(b, a);
            assert_eq!(ab, ba.reverse(), "`{a}` vs `{b}` is not antisymmetric");
            if a == b {
                assert_eq!(ab, std::cmp::Ordering::Equal, "`{a}` must equal itself");
            } else {
                assert_ne!(ab, std::cmp::Ordering::Equal, "`{a}` and `{b}` tie");
            }
        }
    }
}
