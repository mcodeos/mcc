// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Integration test: `MemberLane.alias` carries the source McIds idx alias
// (e.g. "GPIO1") that was expanded to the canonical pin path (e.g.
// "main.U1.1"), vec-dianlu.md §8.9.4 member access chain. An idx alias is a
// synthesized array-member name ("GPIO1" = prefix + slot) whose numeric slot
// equals the resolved pin leaf; a member written in canonical form (a named
// member like "SCLK") is not an alias.

use mcc::{McIds, McURI};
use std::sync::{Mutex, OnceLock};

/// Global mutex to serialize tests that share mcc's global workspace state.
static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

const IDX_ALIAS_SOURCE: &str = r#"
component GPIO_DEV
{
    pins = [
        io [1:2] = GPIO[1:2]
        ps 3 = GND
    ]
}
module main(ps GND)
{
    GPIO_DEV    U1
    U1.GPIO1 -> NET_A
    U1.GPIO2 -> NET_B
    NET_A -> GND
    NET_B -> GND
}
"#;

const CANONICAL_MEMBER_SOURCE: &str = r#"
component BUS_DEV
{
    pins = [
        io [1,2] = SPI{SCLK, MOSI}
        ps 3 = GND
    ]
}
module main(ps GND)
{
    BUS_DEV     U2
    U2.SPI.SCLK -> NET_C
    NET_C -> GND
}
"#;

fn build_block(source: &str) -> mcc::vector::model::McVecBlock {
    let lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();

    let uri: McURI = "/mcc/member-lane-alias.mc".to_string();
    mcc::mcc_load_from_string(&uri, source);
    let entry = mcc::McSpaceName {
        ident: McIds::from("main"),
        uri: mcc::uri_intern(&uri),
    };
    let (inst, table) = mcc::mcb_pass2_flat(&entry, 1).expect("pass2_flat failed");
    let block = mcc::vector::builder::visit::build_mc_vec(&inst, &table);

    drop(lock);
    block
}

#[test]
fn idx_alias_passed_through_to_member_lane() {
    let block = build_block(IDX_ALIAS_SOURCE);
    let trunks = &block.port_trunks;
    assert_eq!(
        trunks.len(),
        1,
        "expected one GPIO trunk group, got {trunks:#?}"
    );
    let trunk = &trunks[0];
    assert_eq!(trunk.name, "U1");
    assert_eq!(trunk.kind, mcc::vector::model::trunk::TrunkKind::Bus);
    assert_eq!(trunk.members.len(), 2, "expected GPIO1 + GPIO2 lanes");

    // The idx alias source token is kept as the lane alias, while the
    // canonical pin path resolves to the numeric leaf ("main.U1.1").
    assert_eq!(trunk.members[0].member, "GPIO1");
    assert_eq!(trunk.members[0].alias.as_deref(), Some("GPIO1"));
    assert_eq!(trunk.members[1].member, "GPIO2");
    assert_eq!(trunk.members[1].alias.as_deref(), Some("GPIO2"));
}

#[test]
fn canonical_member_is_not_an_alias() {
    let block = build_block(CANONICAL_MEMBER_SOURCE);
    let trunks = &block.port_trunks;
    assert_eq!(
        trunks.len(),
        1,
        "expected one SPI trunk group, got {trunks:#?}"
    );
    let trunk = &trunks[0];
    assert_eq!(trunk.name, "U2");
    assert_eq!(trunk.members.len(), 1);
    assert_eq!(trunk.members[0].member, "SCLK");
    assert_eq!(trunk.members[0].alias, None);
}
