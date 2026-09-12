// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Power-flow single view — L1 structural assertions (flow-single-view-design.md
//! §7). Mirrors the golden main shape at small scale: a psrc source face feeds
//! a trunk bus, one converter to the DVDD rail, a DC-bridge pass off DVDD to the
//! quiet AVDD rail (world cross), a separate-converter secondary to the isolated
//! ISO rail (world cross), sink loads, and two-pad decouplers folded on each
//! rail's pair.
//!
//! Assertions:
//!   (a) rail vs bus classification (VDD_3V3 / VDDA / V5V_ISO → Rail, VBUS → Bus);
//!   (b) Source → (pass f1) → converter → Rail gen chains form (`ldo←VBUS`,
//!       `fba←DVDD`, `iso←VBUS`);
//!   (c) world-cross counting — exactly one quiet (AVDD) and one isolated (ISO);
//!   (d) an interior protective island (child `shield` ESDGND) is folded out of
//!       the top single view;
//!   (e) rail contract fields survive when declared (tol / capacity / eff);
//!   (f) two-pad decouplers straddling (rail hot, rail ret) fold per rail.
//!
//! No byte-golden oracle: assertions are structural, on the typed view.

mod common;

use mcc::{McIds, McURI};

/// The L1 board. Conduits for the four roles; a psrc source `src` → trunk
/// `VBUS` through a two-pin `f1` pass → converter `ldo` → DVDD rail; a quiet
/// AVDD derived off DVDD through the DC-bridge pass `fba`; an isolated ISO off
/// the same trunk through converter `iso`; loads + decaps on each rail pair;
/// and a child `shield` module whose interior protective ESDGND is not exported.
const SRC: &str = r#"
component FB {
    pins = [
        io [1,2] = [X, Y]
    ]
}

component LDO_PWR {
    pins = [
        psnk [1,2] = VIN{Vin, GND}::DC(12V)
        psrc [3,2] = VOUT{Vout, GND}::DC(3.3V)
    ]
}

component SRC12 {
    pins = [
        psrc [1,2] = [OUT, GND]::DC(12V)
    ]
}

component SINK3 {
    pins = [
        psnk [1,2] = [VDD, GND]::DC(3.3V)
    ]
}

component ISO5 {
    pins = [
        psnk [1,2] = [PRI, GN1]::DC(12V)
        psrc [3,4] = [SEC, GN2]::DC(5V)
    ]
}

component SINK5 {
    pins = [
        psnk [1,2] = [VDD, GND]::DC(5V)
    ]
}

module shield {
    conduit GND    @role(main)
    conduit ESDGND @role(protective)
    ESDGND - e::FB() - GND @bridge(ESDGND, GND)
}

module main {
    conduit GND     @role(main) @star
    conduit GNDA    @role(quiet)
    conduit GND_ISO @role(isolated)
    conduit EARTH   @role(earth)

    shield sh

    domain DVDD { rail [VDD_3V3, GND]::DC(3.3V) }
    domain AVDD { rail [VDDA,   GNDA]::DC(3.3V, tol:±5%, capacity:300mA, eff:0.9) }
    domain ISO  { rail [V5V_ISO, GND_ISO]::DC(5V) }

    SRC12   src
    LDO_PWR ldo
    SINK3   sink
    SINK3   sinka
    ISO5    iso
    SINK5   isoamp

    src.OUT   -> VBUS_RAW
    src.GND   -> GND
    VBUS_RAW - f1::FB() - VBUS

    [VBUS, GND] -> ldo{VIN | VOUT} -> [VDD_3V3, GND]
    sink.VDD  -> VDD_3V3
    sink.GND  -> GND

    // quiet derived rail: AVDD off DVDD via a DC-bridge pass fba (world cross)
    VDD_3V3 - fba::FB() - VDDA @bridge(VDD_3V3, VDDA)
    sinka.VDD -> VDDA
    sinka.GND -> GNDA

    // isolated rail from a separate-converter secondary (world cross)
    [VBUS, GND] -> iso{ [PRI, GN1] | [SEC, GN2] } -> [V5V_ISO, GND_ISO]
    isoamp.VDD -> V5V_ISO
    isoamp.GND -> GND_ISO

    // decouplers: two-pin FB elements straddling (rail hot, rail ret)
    VDD_3V3 - d1::FB() - GND
    VDDA   - d2::FB() - GNDA
    VDDA   - d3::FB() - GNDA

    EARTH - y::FB() - GND
}
"#;

fn build_flow() -> (mcc::InstTable, mcc::PwrFlow) {
    let _lock = common::lock();
    common::reset();
    let uri: McURI = "/mcc/pwrflow-l1.mc".to_string();
    mcc::mcc_load_from_string(&uri, SRC);
    let (_, table) = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");
    let flow = mcc::build_pwrflow(&table, "main").expect("pwrflow build");
    (table, flow)
}

fn rail_row<'a>(flow: &'a mcc::PwrFlow, domain: &str) -> &'a mcc::RailRow {
    flow.rails
        .iter()
        .find(|r| r.domain == domain)
        .unwrap_or_else(|| panic!("no rail row for domain {domain:?} in {:?}", flow.rails))
}

/// Depth-first walk of the §3 forest for a node of a given class + id.
fn find_node<'a>(roots: &'a [mcc::FlowNode], class: &str, id: &str) -> Option<&'a mcc::FlowNode> {
    for root in roots {
        if root.class == class && root.id == id {
            return Some(root);
        }
        if let Some(hit) = find_node(&root.children, class, id) {
            return Some(hit);
        }
    }
    None
}

#[test]
fn rail_and_bus_classification_and_gen() {
    // (a) The three declared rails are rail rows; the intermediate trunk VBUS
    // is not a rail. (b) Each rail's producer spine resolves to its branch:
    // converter `ldo`/`iso` off the trunk bus, DC-bridge pass `fba` off DVDD.
    let (_table, flow) = build_flow();

    let domains: Vec<&str> = flow.rails.iter().map(|r| r.domain.as_str()).collect();
    assert_eq!(domains, vec!["DVDD", "AVDD", "ISO"]);

    let dvdd = rail_row(&flow, "DVDD");
    assert_eq!(dvdd.gen, "ldo←VBUS");
    assert_eq!(dvdd.world, "main");
    assert!(!dvdd.cross_world);

    let avdd = rail_row(&flow, "AVDD");
    assert_eq!(avdd.gen, "fba←DVDD");
    assert_eq!(avdd.world, "quiet");

    let iso = rail_row(&flow, "ISO");
    assert_eq!(iso.gen, "iso←VBUS");
    assert_eq!(iso.world, "isolated");

    // Tree side: the source face is a psrc root; VBUS is a Bus node; each rail
    // hot is a Rail node fed by its via element.
    let roots = &flow.roots;
    assert!(
        find_node(roots, "source", "VBUS_RAW").is_some(),
        "psrc source face VBUS_RAW must root the tree"
    );
    let vbus = find_node(roots, "bus", "VBUS")
        .expect("intermediate trunk VBUS must be a Bus node, not a rail");
    assert_eq!(vbus.via.as_deref(), Some("f1"));
    assert!(
        find_node(roots, "rail", "VDD_3V3").is_some(),
        "VDD_3V3 must classify as a Rail node"
    );
    assert!(
        find_node(roots, "rail", "VDDA").is_some(),
        "VDDA must classify as a Rail node"
    );
    assert!(
        find_node(roots, "rail", "V5V_ISO").is_some(),
        "V5V_ISO must classify as a Rail node"
    );
}

#[test]
fn cross_world_flags_count_quiet_and_isolated_once() {
    // (c) Exactly one quiet and one isolated world cross (AVDD off DVDD across
    // GND→GNDA; ISO off the main-side trunk across GND→GND_ISO). DVDD is clean.
    let (_table, flow) = build_flow();

    let crossed: Vec<&str> = flow
        .rails
        .iter()
        .filter(|r| r.cross_world)
        .map(|r| r.domain.as_str())
        .collect();
    assert_eq!(crossed, vec!["AVDD", "ISO"]);

    // On the edge level (the tree child carries the flag) both crosses appear
    // exactly once: iso→V5V_ISO and fba→VDDA.
    let mut quiet = 0;
    let mut isolated = 0;
    fn walk(nodes: &[mcc::FlowNode], quiet: &mut usize, isolated: &mut usize) {
        for n in nodes {
            if n.cross_world {
                match n.world.as_deref() {
                    Some("quiet") => *quiet += 1,
                    Some("isolated") => *isolated += 1,
                    _ => {}
                }
            }
            walk(&n.children, quiet, isolated);
        }
    }
    walk(&flow.roots, &mut quiet, &mut isolated);
    assert_eq!(quiet, 1, "exactly one quiet world cross (AVDD)");
    assert_eq!(isolated, 1, "exactly one isolated world cross (ISO)");
}

#[test]
fn interior_protective_island_is_folded_out_of_top_view() {
    // (d) The child shield's ESDGND is interior (module-scoped net) and never
    // exported, so the top single view shows exactly the four declared world
    // coppers and no trace of ESDGND anywhere in crown/rails/tree.
    let (_table, flow) = build_flow();

    let coppers: Vec<&str> = flow.crown.iter().map(|c| c.copper.as_str()).collect();
    assert_eq!(coppers, vec!["GND", "GNDA", "GND_ISO", "EARTH"]);
    let worlds: Vec<&str> = flow.crown.iter().map(|c| c.world.as_str()).collect();
    assert_eq!(worlds, vec!["main", "quiet", "isolated", "earth"]);

    let all_rails: Vec<&str> = flow
        .rails
        .iter()
        .flat_map(|r| [r.hot.as_str(), r.ret.as_str()])
        .collect();
    assert!(
        !all_rails.iter().any(|n| n.contains("ESDGND")),
        "shield interior ESDGND must not leak into rail hot/ret: {all_rails:?}"
    );

    fn any_esd(nodes: &[mcc::FlowNode]) -> bool {
        nodes
            .iter()
            .any(|n| n.id.contains("ESDGND") || n.label.contains("ESDGND") || any_esd(&n.children))
    }
    assert!(
        !any_esd(&flow.roots),
        "shield interior ESDGND must not appear in the §3 tree"
    );
}

#[test]
fn rail_contract_fields_and_decaps_fold() {
    // (e) The AVDD rail declares a budget; the flat L1Rail decode must carry
    // through to the view row. (f) Two-pad elements straddling (rail hot, rail
    // ret) fold as decouplers on each rail's pair — never as tree elements.
    let (_table, flow) = build_flow();

    let avdd = rail_row(&flow, "AVDD");
    assert_eq!(avdd.v_text, "3.3V");
    let v = avdd.v.expect("AVDD nominal value decodes");
    assert!((v - 3.3).abs() < 1e-9, "AVDD v = {v}");
    let tol = avdd.tol.expect("AVDD tol decodes");
    assert!((tol - 0.05).abs() < 1e-9, "AVDD tol = {tol}");
    let cap = avdd.capacity_amps.expect("AVDD capacity decodes");
    assert!((cap - 0.3).abs() < 1e-9, "AVDD capacity_amps = {cap}");
    let eff = avdd.eff.expect("AVDD eff decodes");
    assert!((eff - 0.9).abs() < 1e-9, "AVDD eff = {eff}");

    assert_eq!(rail_row(&flow, "DVDD").decaps, vec!["d1"]);
    assert_eq!(rail_row(&flow, "AVDD").decaps, vec!["d2", "d3"]);
    assert!(rail_row(&flow, "ISO").decaps.is_empty());

    // Decap loads hang off the rail node as folded note/leaf, not as elements
    // of the tree chain: none of the rail nodes list d1/d2/d3 as a child via.
    let roots = &flow.roots;
    let vdd_node = find_node(roots, "rail", "VDD_3V3").expect("VDD_3V3 rail node");
    assert!(
        !vdd_node
            .children
            .iter()
            .any(|c| c.via.as_deref() == Some("d1")),
        "decap d1 must not appear as a tree element on DVDD"
    );
}
