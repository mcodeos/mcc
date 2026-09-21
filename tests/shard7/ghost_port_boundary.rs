// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Acceptance surface for the GHOST_PORT (4055) exemption of a layer's own
//! declared ports (`fromblock::is_own_boundary_port`).
//!
//! # The law being locked
//!
//! A module's own ports are boundary objects: module-port-drawing paints them
//! on the dashed boundary frame (`viz::layout::module_frame`), never in a
//! box — the P7-8 PortTerminal producer was retired on purpose
//! (fromblock.rs, the Phase 1.45/1.46 deletion notes). So when a net of the
//! module's **own** layer ends on one of them, `make_endpoint` returning
//! `None` is by design, and firing GHOST_PORT ("this pin may cross a module
//! boundary without being properly exposed as a port") at it states the
//! exact opposite of the truth: it **is** the exposed port.
//!
//! Measured on mcs/hbl `us513.mc` (pre-fix, via the build.viz RPC replay):
//! every io/out port member of module US513's own layer fired — `MIC.P`,
//! `MIC.N`, `SPI._(1..4)`, `UART0.RX`, `UART0.TX`, `DAC_OUT`, `SPK_MUTE` —
//! while the psnk power ports (which project.rs routes through the rail
//! faces) stayed clean. Severity: Error, in the IDE store on every save.
//!
//! # What is asserted
//!
//! Branch ① — the exemption: a module whose own layer nets a scalar port
//! (`out DAC_OUT`), a curly port member (`MIC.N`), and an interface-adopted
//! anonymous member (`SPI._(2)`) builds with **zero** GHOST_PORT.
//!
//! Branch ② — the remaining domain: a **usage-born** label (no Port
//! ancestor) still fires GHOST_PORT — the exemption must not swallow the
//! genuinely unmapped endpoint D4 exists for.

// Family naming `{family}__{essence}` uses a doubled underscore to separate
// the grep-able family token from the essence (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use mcc::McIds;
use mcc::McURI;

/// Plain two-pin capacitor body.
const CAP2: &str = "component CAP2 {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n}\n";

/// Local two-lane interface: no dependence on the installed system library's
/// SPI; the port row adopts it role-less (conductor view), so its members
/// read as the anonymous `_(1)` / `_(2)`.
const IFX: &str = "interface IFX(role) {\n    pins = [\n        1 = A\n        2 = B\n    ]\n    role Master {}\n    role Slave {}\n}\n";

/// Build the vec graph of module `top` and return every diagnostic code the
/// graph build left in the store.
fn ghost_codes(src: &str, top: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from("test://ghost_port_boundary/test.mc");
    mcc::mcc_init();
    mcc::mcc_load_from_string(&u, src);
    let (tree, table, arena, store) =
        mcc::mcc_build_flat_with_arena(&McIds::from(top), &u, 1000).expect("flat build");
    let vec_block = mcc::vector::builder::visit::build_mc_vec(&tree, &table, &arena, &store);
    mcc::vector::graph::fromblock::build_mc_vec_graph(&vec_block, &table);
    mcc::mcc_diagnose_all()
        .iter()
        .map(|d| d.code)
        .filter(|c| *c == 4055)
        .collect()
}

#[test]
fn GHOST_PORT__own_boundary_ports_are_exempt() {
    let codes = ghost_codes(
        &format!(
            "{CAP2}{IFX}module mm(io MIC{{P, N}}, io SPI::SPI(), out DAC_OUT) {{\n    \
             CAP2 C1\n    CAP2 C2\n    CAP2 C3\n    \
             MIC.P -> C1.1\n    \
             MIC.N -> C2.1\n    \
             SPI._(2) -> C3.1\n    \
             C1.2 -> DAC_OUT\n    \
             C2.2 - C3.2\n}}\n"
        ),
        "mm",
    );
    assert!(
        codes.is_empty(),
        "the layer's own declared ports must not fire GHOST_PORT, got {codes:?}"
    );
}

#[test]
fn GHOST_PORT__usage_born_label_still_fires() {
    let codes = ghost_codes(
        &format!(
            "{CAP2}module mm() {{\n    \
             CAP2 C1\n    \
             C1.1 -> NOPE\n    \
             C1.2 - C1.2\n}}\n"
        ),
        "mm",
    );
    assert!(
        !codes.is_empty(),
        "a usage-born label with no Port ancestor is a genuinely unmapped \
         endpoint and GHOST_PORT must keep firing for it"
    );
}
