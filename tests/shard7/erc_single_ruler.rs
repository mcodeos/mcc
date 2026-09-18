// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U89: `mcc erc` is a face over the flat net checks, not a second ruler.
//!
//! Until 2026-09-18 the command carried its own root-net engine over the root
//! module's string net table (ERC 6001-6004), and the RPC `erc` method a third
//! implementation that also lacked the rail-aware driver rule the local face
//! had. Two rulers over one design disagree: on a real board the old engine
//! called a net multi-driven (six drivers on `V3V3.GND`) where the world's ERC
//! truth saw one short, and stayed silent on 6002 where the flat reported
//! fourteen unwired pins. `erc/rules-catalog-design.md` §3.2 maps all four of
//! its checks onto flat equivalents, so the engine was retired and the command
//! kept.
//!
//! Three things are asserted here, and they are the three ways the retirement
//! can silently come undone:
//!
//! 1. **One result set.** `mcc erc` and `mcc check --nets` answer the same
//!    build with the same checks — asserted as multisets of check names, not as
//!    totals, so a rule that fires on one face and not the other cannot cancel
//!    out against a rule that fires the other way round.
//! 2. **The old codes are gone.** Nothing reports 6001-6004 any more, and every
//!    code the command does report is registered in the central catalog.
//! 3. **One top module.** `erc` reads the manifest's `top_module`, the same
//!    declaration `build` reads. It used to fall through to the first module
//!    loaded, so on a multi-module project `erc` and `build` checked different
//!    designs. ⚠ This one needs a manifest whose top is *not* the first loaded
//!    module — the stock `hbl` manifest happens to name `main`, which is also
//!    first, so the assertion would hold either way. The test rewrites the
//!    manifest instead of trusting the fixture.

use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

fn hbl_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hbl")
}

/// A fresh, **empty** directory to run a CLI invocation in — the readout must
/// not depend on where it is run.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mcc-erc-{name}-{}-{:?}",
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
/// instead, and an old server holds an old library and an old binary (skill
/// §5.3), which is exactly the kind of second opinion this file exists to rule
/// out.
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

fn erc_payload(cwd: &Path, target: &Path) -> Value {
    let t = target.to_str().expect("fixture path");
    let (stdout, stderr, _) = run_mcc(cwd, &["erc", t, "-f", "json"]);
    let env: Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!("erc did not emit an envelope: {e}\nstdout: {stdout}\nstderr: {stderr}")
    });
    env["result"]["erc"].clone()
}

/// Check names, with multiplicity, from an `erc` payload.
fn erc_check_names(erc: &Value) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for v in erc["violations"].as_array().expect("violations array") {
        let name = v["check"].as_str().expect("check name").to_string();
        *out.entry(name).or_insert(0) += 1;
    }
    out
}

/// Check names, with multiplicity, from `check --nets`'s stderr listing.
///
/// The lines are `  [<severity>] <check>: <message>` under the
/// `=== Electrical Net Checks (N issues) ===` header.
fn check_nets_check_names(stderr: &str) -> BTreeMap<String, usize> {
    let mut out: BTreeMap<String, usize> = BTreeMap::new();
    for line in stderr.lines() {
        let Some(rest) = line.trim_start().strip_prefix('[') else {
            continue;
        };
        let Some((_sev, rest)) = rest.split_once("] ") else {
            continue;
        };
        let name = rest.split(':').next().unwrap_or("").trim();
        if name.is_empty() {
            continue;
        }
        *out.entry(name.to_string()).or_insert(0) += 1;
    }
    out
}

/// 1. `erc` and `check --nets` are one ruler over one build.
#[test]
fn erc_and_check_nets_report_the_same_checks() {
    let cwd = scratch("one-ruler");
    let target = hbl_dir();

    let erc = erc_payload(&cwd, &target);
    let from_erc = erc_check_names(&erc);

    let t = target.to_str().expect("fixture path");
    let (_out, err, _ok) = run_mcc(&cwd, &["check", "--nets", t]);
    let from_check = check_nets_check_names(&err);

    // The fixture must actually exercise the comparison: `hbl` is a seven-layer
    // board, and an empty-vs-empty comparison would pass over nothing.
    assert!(
        from_erc.len() >= 5 && from_erc.values().sum::<usize>() >= 20,
        "the fixture stopped exercising the net checks ({from_erc:?}); the comparison below would be vacuous"
    );
    assert_eq!(
        from_erc, from_check,
        "`erc` and `check --nets` disagree on the same build — the command has grown a second ruler again"
    );

    // The payload's own summary must be derived from the rows it carries.
    let summary = &erc["summary"];
    assert_eq!(
        summary["violations"].as_u64().unwrap() as usize,
        from_erc.values().sum::<usize>()
    );
    let by_check: BTreeMap<String, usize> = summary["by_check"]
        .as_object()
        .expect("by_check object")
        .iter()
        .map(|(k, v)| (k.clone(), v.as_u64().unwrap() as usize))
        .collect();
    assert_eq!(
        by_check, from_erc,
        "summary.by_check is not the row set it summarises"
    );
}

/// 2. The retired root-net codes are neither emitted nor registered.
#[test]
fn the_retired_root_net_codes_are_gone() {
    let cwd = scratch("retired-codes");
    let erc = erc_payload(&cwd, &hbl_dir());

    let registered: std::collections::HashSet<u32> =
        mcc::errcodes::all_codes().iter().map(|e| e.code).collect();

    let rows = erc["violations"].as_array().expect("violations array");
    assert!(!rows.is_empty(), "no violations to check the codes against");
    for v in rows {
        let code = v["code"].as_u64().expect("code") as u32;
        assert!(
            !(6001..=6004).contains(&code),
            "ERC {code} is a code of the retired `mcc erc` root-net engine; the command must report the flat net checks' own codes"
        );
        assert!(
            registered.contains(&code),
            "ERC {code} ({}) is not in the central code catalog",
            v["check"].as_str().unwrap_or("?")
        );
    }

    // The catalog itself must have let them go, not merely stopped emitting.
    for code in 6001..=6004u32 {
        assert!(
            !registered.contains(&code),
            "ERC {code} is still registered in the code catalog after the engine's retirement"
        );
    }
}

/// 3. `erc` checks the design the manifest declares, not the first module loaded.
#[test]
fn erc_builds_the_manifest_top_module() {
    let cwd = scratch("manifest-top");
    let dir = scratch("manifest-top-project");

    // A copy of `hbl` whose manifest declares a submodule as the top. On the
    // stock fixture the manifest's top (`main`) is also the first module loaded,
    // so following the manifest and ignoring it look identical there.
    let src = hbl_dir();
    copy_tree(&src, &dir);
    let manifest = dir.join("project.toml");
    let text = std::fs::read_to_string(&manifest).expect("read project.toml");
    let rewritten = text.replace("top_module = \"main\"", "top_module = \"POWER_LDO\"");
    assert_ne!(text, rewritten, "the fixture manifest changed shape");
    std::fs::write(&manifest, rewritten).expect("write project.toml");

    let erc = erc_payload(&cwd, &dir);
    assert_eq!(
        erc["top"].as_str().expect("top"),
        "POWER_LDO",
        "`erc` did not check the module the manifest declares as top"
    );

    // And it really built that design: the submodule is smaller than the board.
    let as_submodule = erc["summary"]["violations"].as_u64().unwrap();
    let as_board = erc_payload(&cwd, &hbl_dir())["summary"]["violations"]
        .as_u64()
        .unwrap();
    assert!(
        as_submodule > 0 && as_submodule < as_board,
        "checking {as_submodule} violations for the submodule against {as_board} for the board — \
         the top name changed but the build did not"
    );
}

/// 4. The root module's own ports are owned — by the module-scope face, not by
///    the net checks, and not by nobody.
///
/// This one was measured the hard way. The retired engine reported an unwired
/// top-level port as E6002, and `check_unused_module_ports` skips the top
/// module's ports, so the obvious reading was that dropping E3 left those ports
/// to nobody. **It does not.** Declaring two unwired ports on `hbl`'s top
/// module and looking at the whole readout shows the module-scope face already
/// owns both: E5162 (`MODULE_PORT_UNUSED`) for a header-declared port and E5642
/// (`PORT_NEVER_USED`) for a body `io` — the latter is locked by
/// `flatten_net_check_diagnostics::dlu_flatchk__unused_io_port_sequence_locked`.
///
/// So removing the skip would have made E4114 a second voice for a fact that
/// already has one. This test pins the split in both directions: the net-check
/// face must stay silent, and the readout as a whole must not.
#[test]
fn the_root_modules_unwired_ports_are_owned_by_the_module_scope_face() {
    let cwd = scratch("root-ports");
    let dir = scratch("root-ports-project");
    copy_tree(&hbl_dir(), &dir);

    let src = dir.join("src/hbl.mc");
    let text = std::fs::read_to_string(&src).expect("read hbl.mc");
    let rewritten = text.replace(
        "module main\n{",
        "module main(out UNWIRED_ROOT, out ALSO_UNWIRED)\n{",
    );
    assert_ne!(
        text, rewritten,
        "the fixture's top module header changed shape"
    );
    std::fs::write(&src, rewritten).expect("write hbl.mc");

    // (a) The net-check face does not report them, and does not report them
    //     twice over: adding the two ports moves `unused-module-port` by zero.
    let count = |erc: &Value| -> usize {
        let by_check = &erc["summary"]["by_check"];
        by_check["unused-module-port"].as_u64().unwrap_or(0) as usize
    };
    let bare = erc_payload(&cwd, &hbl_dir());
    let with_ports = erc_payload(&cwd, &dir);
    assert!(
        count(&bare) > 0,
        "`unused-module-port` reports nothing on the stock fixture; the delta below would be vacuous"
    );
    assert_eq!(
        count(&with_ports),
        count(&bare),
        "`unused-module-port` moved by {} when two unwired root ports were declared — the net-check \
         face has taken over a fact the module-scope face already reports",
        count(&with_ports) as i64 - count(&bare) as i64
    );
    for v in with_ports["violations"]
        .as_array()
        .expect("violations array")
    {
        let msg = v["message"].as_str().unwrap_or("");
        assert!(
            !msg.contains("UNWIRED_ROOT") && !msg.contains("ALSO_UNWIRED"),
            "the net-check face reports a root port: {msg}"
        );
    }

    // (b) Somebody does report them: one E5162 per port, each naming its port.
    let t = dir.to_str().expect("fixture path");
    let (out, _err, _ok) = run_mcc(&cwd, &["check", t]);
    for port in ["UNWIRED_ROOT", "ALSO_UNWIRED"] {
        let rows: Vec<&str> = out
            .lines()
            .filter(|l| l.contains("E5162") && l.contains(port))
            .collect();
        assert_eq!(
            rows.len(),
            1,
            "expected exactly one E5162 for the unwired root port {port}, got {rows:?}"
        );
    }
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create dir");
    for entry in std::fs::read_dir(from).expect("read dir") {
        let entry = entry.expect("dir entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy file");
        }
    }
}
