// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U343-A2: a typed interface reference prefixed by its host instance inside a
//! method body must stay typed through pass2 prefixing. Before the fix,
//! `prefix_instance_phrase_with_skip` downgraded the three typed arms
//! (Component/Module/Interface) to `McInstance::Bus("{inst}.{name}")` — a
//! bare-text bus that only `expand_bus_labels` could rescue, and only when a
//! >=2-member bus table existed. A 1-pin interface carrying a role argument
//! (`CLKOUT::CLK(TRANSMITTER)`, the shipped `mcode/ifs/clk.mc` shape) has no
//! such table, so the prefixed reference silently died with E4007
//! "Shape mismatch in -> connection" — zero diagnostics pointing at the
//! downgrade itself.
//!
//! The lock drives `mcc export netlist` over a temp project whose dependency
//! is a self-contained fake `mcode` library in a private `MCC_SYSTEM_ROOT`
//! (never the developer's live `~/.mcode`), mirroring the u320 harness. The
//! interface below is a faithful minimization of the live witness: one pin
//! plus TRANSMITTER/RECEIVER roles. The role argument is load-bearing —
//! without it the pristine downgrade happens to re-resolve downstream and the
//! shape stops discriminating (verified against a pristine-HEAD build).
//!
//! Anti-vacuous: both halves must hold together. No-E4007 alone would pass if
//! the connection vanished silently, and `osc.CLKP` alone would pass on the
//! old downgrade text — the pristine netlist for this exact shape is *empty*.

use std::path::PathBuf;
use std::process::Command;

const LIB_ENTRY: &str = r#"
interface ONEP(role)
{
    topology = "point to point"
    mode = ["unidirectional"]

    pins = [
        1 = P @class(digital)
    ]

    role TRANSMITTER {
        name = "ONEP Transmitter"
        pins = [
            out 1 = P
        ]
        peer = RECEIVER
    }
    role RECEIVER {
        name = "ONEP Receiver"
        pins = [
            in 1 = P
        ]
        peer = TRANSMITTER
    }
}
"#;

const PROJECT_TOML: &str = r#"
[project]
name = "u343typedprefix"
version = "0.1.0"
entry = "src/main.mc"
top_module = "top"

[dependencies]
mcode = "*"
"#;

const MAIN_MC: &str = r#"
use mcode/mcode.mc

component OSC1 {
    pins = [
        1 = CLKP::ONEP(TRANSMITTER), "clock"
    ]
    func Wire(g) {
        CLKP - g
    }
}
module top {
    io PCLK
    OSC1 osc
    osc.Wire(PCLK)
}
"#;

/// One laid-out probe: the fake sysroot, the temp project, and a cleanup
/// handle. `project`/`main_mc` are what the CLI invocation below points at.
struct Probe {
    _dir: PathBuf,
    sysroot: PathBuf,
    project: PathBuf,
    main_mc: PathBuf,
}

impl Probe {
    fn create(tag: &str) -> Probe {
        let dir = std::env::temp_dir().join(format!("u343-{}-{tag}", std::process::id()));
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

/// The method-body `CLKP - g` prefixed to `osc.CLKP` must stay a typed
/// interface reference: the netlist carries the `PCLK ~ osc.CLKP` connection.
/// Before the fix this exact shape (1-pin interface + role argument) produced
/// an *empty* netlist — the downgraded bare-text bus never re-resolved.
///
/// U356 rebase: the whole-foot single-member interface reference now
/// resolves to the adopted pin's physical id, so the net renders `osc.1`
/// (the pid spelling every component-side interface lane uses — multi-member
/// `c.DP -> d.DH` lands as `c.1`/`c.2` the same way). The pre-U356 spelling
/// `osc.CLKP` in this assertion was the ghost point the U351 fixture
/// exposed: pristine HEAD rendered the net but left `top.osc.1` E4119
/// unconnected behind it. The guarded property — the prefixed reference
/// reaches the receiver's net — now holds onto the physical pin.
#[test]
fn lock_netlist__prefixed_typed_iface_reference_stays_typed() {
    let probe = Probe::create("netlist");
    let text = probe.run(&[
        "export",
        "netlist",
        probe.main_mc.to_str().expect("utf8"),
        "--top",
        "top",
    ]);
    assert!(
        !text.contains("E4007"),
        "prefixed typed reference must not die with E4007; output: {text}"
    );
    assert!(
        text.contains("PCLK: PCLK osc.1"),
        "the prefixed reference must land on the receiver's net through the adopted pin; netlist: {text}"
    );
}

/// The same probe under `check`: the downgrade path must not leak any
/// diagnostic of its own. Guards the no-silent-regression half of the fix.
#[test]
fn lock_check__prefixed_typed_iface_reference_clean() {
    let probe = Probe::create("check");
    let text = probe.run(&["check", probe.project.to_str().expect("utf8"), "-f", "json"]);
    assert!(
        !text.contains("E4007") && !text.contains("E3179") && !text.contains("E2082"),
        "check must stay clean for the typed prefixed reference; output: {text}"
    );
}
