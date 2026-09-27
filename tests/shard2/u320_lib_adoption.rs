// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U320: `component X :: Recipe` written in a file loaded *as a library* must
//! give X its adopted funcs, exactly like the byte-identical file used from
//! the project. Before the fix, `sync_derivation_edges` rebuilt
//! `adopts`/`effective_funcs` from a project-domain-only enumeration, so a
//! library-side host (mcode is auto-visible, hence `LoadDomain::System`)
//! never entered the loop — silently, with no diagnostic of its own, until
//! the call site fired `E3071 function 'Pull' not found in class 'RES'`.
//!
//! The locks drive `mcc check` / `mcc export netlist` over a temp project
//! whose dependency is a self-contained fake `mcode` library in a private
//! `MCC_SYSTEM_ROOT` (never the developer's live `~/.mcode`), so the host
//! component really travels the library-load path. The no-E3071 half alone
//! would pass vacuously if adoption broke silently the other way, so the
//! netlist half asserts the recipe body's `this.1`/`this.2` connections
//! actually land on the host pins.

use std::path::PathBuf;
use std::process::Command;

/// The fake system library: entry file plus the recipe-and-adopt pair. Named
/// `mcode` because that is the one library `mcb_load_lib` keeps live (any
/// other name is tombstoned for use-only visibility and could not serve the
/// lock). Shapes mirror the shipped `res.mc` probe from the CIMP entry.
const LIB_ENTRY: &str = r#"
recipe U320Tie
{
    func Tie([node, supply])
    {
        node - this.1
        supply - this.2
    }
}

component U320Res
{
    name = "u320 resistor"
    pins = [
        1 = 1, "Term 1"
        2 = 2, "Term 2"
    ]
}

component U320ResTie :: U320Tie
{
    name = "u320 adopting resistor"
    pins = [
        1 = 1, "Term 1"
        2 = 2, "Term 2"
    ]
}
"#;

const PROJECT_TOML: &str = r#"
[project]
name = "u320lock"
version = "0.1.0"
entry = "src/main.mc"
top_module = "main"

[dependencies]
mcode = "*"
"#;

const MAIN_MC: &str = r#"
use mcode/mcode.mc

module main
{
    U320ResTie r1

    net SIG
    net VCC
    r1.Tie([SIG, VCC])
}
"#;

/// One laid-out probe: the fake sysroot, the temp project, and a cleanup
/// handle. `sysroot`/`project` are what the CLI invocations below point at.
struct Probe {
    _dir: PathBuf,
    sysroot: PathBuf,
    project: PathBuf,
    main_mc: PathBuf,
}

impl Probe {
    fn create(tag: &str) -> Probe {
        let dir = std::env::temp_dir().join(format!("u320-{}-{tag}", std::process::id()));
        let sysroot = dir.join("sysroot");
        let project = dir.join("proj");
        std::fs::create_dir_all(sysroot.join("mcode")).expect("create fake lib dir");
        std::fs::create_dir_all(project.join("src")).expect("create project dir");
        std::fs::write(sysroot.join("mcode/mcode.mc"), LIB_ENTRY).expect("write lib entry");
        std::fs::write(project.join("project.toml"), PROJECT_TOML).expect("write project.toml");
        let main_mc = project.join("src/main.mc");
        std::fs::write(&main_mc, MAIN_MC).expect("write main.mc");
        Probe {
            _dir: dir,
            sysroot,
            project,
            main_mc,
        }
    }

    /// Run `mcc` against this probe with `MCC_SYSTEM_ROOT` at the fake root;
    /// the probe directory is removed on drop, so every caller must have read
    /// what it needs out of the output before letting `probe` go.
    fn run(&self, args: &[&str]) -> String {
        let output = Command::new(env!("CARGO_BIN_EXE_mcc"))
            .args(args)
            .env("MCC_SYSTEM_ROOT", &self.sysroot)
            .output()
            .expect("run mcc");
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self._dir);
    }
}

/// The library-side adopted call must not fire E3071 — the derivation sync
/// sees system-domain hosts, so `U320ResTie` carries `Tie` after a library
/// load. Before the fix this fired `function 'Tie' not found in class
/// 'U320ResTie'`.
#[test]
fn lock_check__library_side_adoption_not_reported_3071() {
    let probe = Probe::create("check");
    let text = probe.run(&["check", probe.project.to_str().expect("utf8"), "-f", "json"]);
    assert!(
        !text.contains("E3071"),
        "library-side adopted call must not fire E3071; output: {text}"
    );
}

/// Anti-vacuous half: the recipe body must actually project — `this.1`/
/// `this.2` land on the host's pins, so the netlist carries `r1.1` and
/// `r1.2`. A silent-empty adoption (the U304/U318 failure shape) would pass
/// the no-E3071 half alone.
#[test]
fn lock_netlist__library_side_adopted_body_projects() {
    let probe = Probe::create("netlist");
    let text = probe.run(&[
        "export",
        "netlist",
        probe.main_mc.to_str().expect("utf8"),
        "--top",
        "main",
    ]);
    assert!(
        text.contains("r1.1") && text.contains("r1.2"),
        "the adopted recipe body must land on the host pins; netlist: {text}"
    );
}
