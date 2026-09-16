// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Integration test: the real board's supplies reach the ports whose DECLARED
// voltage they match (CIMP U12).
//
// `hbl.mc` hands `V3V3` and `V1V2` to the MCU and to the microphone -- both the
// declared-instance form (`US513 mcu513(V3V3, V1V2)`) and the call-site form
// (`mic(V3V3)`). The ports they land on are declared `[VDD_3V3,GND]::DC(3.3V)`
// and `[VCC_1V2,GND]::DC(1.2V)` in `us513.mc`, and the supplies are declared
// `V3V3::DC(3.3V)` / `V1V2::DC(1.2V)` in `hbl.mc`. The DECLARED voltage is what
// pairs them: a pairing driven by the digit-V-digit fragment of the two names
// takes `V3V3` for `VDD_3V3` on no better grounds than both spelling 3.3 volts
// as "3V3", which is the whole defect. Both faces of each supply are pinned, so
// a pairing that swapped the rails shows up as a red here.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use std::collections::BTreeSet;
use std::path::PathBuf;

use mcc::McIds;

/// The fixture board's nets, each as the set of point paths it holds. The
/// caller must hold [`common::lock`]: the registry is process-global.
fn hbl_nets() -> Vec<BTreeSet<String>> {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hbl");
    let entry_uri = project_root
        .join("src/hbl.mc")
        .to_string_lossy()
        .into_owned();
    mcc::mcc_init();
    mcc::mcc_set_project_root(&project_root);
    mcc::mcc_load_project(&entry_uri);
    let (_, _, _, store) = mcc::mcc_build_with_nets(&McIds::from("main"), &entry_uri)
        .expect("build the hbl fixture board");

    let mut out: Vec<BTreeSet<String>> = Vec::new();
    if let Some(table) = store.get("main") {
        for (_, pts) in table.iter() {
            let mut net: BTreeSet<String> = BTreeSet::new();
            for p in pts.iter() {
                net.insert(p.path.clone());
            }
            if !net.is_empty() {
                out.push(net);
            }
        }
    }
    out
}

/// The point paths sharing a net with `probe`, sorted, for pinning and for
/// failure messages.
fn net_of(nets: &[BTreeSet<String>], probe: &str) -> Vec<String> {
    nets.iter()
        .find(|n| n.iter().any(|p| p == probe))
        .map(|n| n.iter().cloned().collect())
        .unwrap_or_default()
}

#[test]
fn u12__hbl_supplies_reach_the_ports_they_declare() {
    let _lock = common::lock();
    let nets = hbl_nets();

    // Each rail is pinned whole, not by sampling a member: a pairing that put
    // one port on the wrong rail changes a set, and a set is what the
    // expectation is.

    // The 3.3 V positive face. Everything here arrives from an argument named
    // `V3V3`: the MCU's `[VDD_3V3,GND]::DC(3.3V)`, the microphone's
    // `dc{VDD_3V3,GND}::DC(3.3V)`, the flash's `[V3V3,GND]::DC(3.3V)`, the
    // speaker and both converters.
    assert_eq!(
        net_of(&nets, "V3V3.VCC"),
        [
            "DCDC.VDD_3V3",
            "FLASH.8",
            "LDO.vout.VCC",
            "MCU513.VDD_3V3",
            "MIC.VDD_3V3",
            "MIC.dc.VDD_3V3",
            "SPK.USB_VBUS_1.VDD_3V",
            "SPK.VDD_3V",
            "V3V3.VCC",
            "_C1.1",
            "_R1.2",
            "_R2.2",
            "_R3.2",
        ]
    );

    // The 1.2 V positive face: the core side only. Nothing off the 3.3 V rail
    // is on it, and the MCU's two supply ports are on different rails.
    assert_eq!(
        net_of(&nets, "V1V2.VCC"),
        ["DCDC.VCC_1V2", "MCU513.VCC_1V2", "V1V2.VCC"]
    );

    // The return faces are one net: both declarations carry `GND` as their
    // second member, and the two reaches meet at the shared ground.
    let gnd = [
        "DCDC.GND",
        "FLASH.4",
        "LDO.vin.GND",
        "LDO.vout.GND",
        "MCU513.GND",
        "MIC.GND",
        "MIC.dc.GND",
        "SPK.GND",
        "SPK.USB_VBUS_1.GND",
        "USB.vin.GND",
        "V1V2.GND",
        "V3V3.GND",
        "V5V.GND",
        "_C1.2",
    ];
    assert_eq!(net_of(&nets, "V3V3.GND"), gnd);
    assert_eq!(
        net_of(&nets, "V1V2.GND"),
        gnd,
        "both supplies' return faces are the same ground net"
    );
}
