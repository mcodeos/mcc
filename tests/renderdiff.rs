// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ P7-1 · renderdiff integration test —— the rendering-layer ruler
//!
//! ## Division of labor with netdiff (discipline 10)
//! - `netdiff`: netlist golden (are connections right)
//! - `renderdiff`: render golden (is the drawing right) —— `baseline/render_golden.toml`
//!
//! ## Assertion shape at the P7-1 stage (v6 §4)
//! **Large swaths of red mid-way is the correct shape**:
//! - main layer extra (rail flags + top-level passives) must be significantly > 0 (currently ≈ 27)
//! - GND edges / power edges non-zero (rail trichotomy is P7-3)
//! - all 7 layers have readings (the ruler is measuring something)
//! - All green instead means the ruler is broken —— see discipline 9
//!
//! After P7-3 these assertions flip (red→green); at that point change the assertions, not the golden.

use std::path::PathBuf;

use mcc::viz::api::{render_with_metrics, RenderOpts};
use mcc::viz::metrics::renderdiff::{RenderGolden, Verdict};
use mcc::McIds;

fn hbl_project_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("projects/hbl")
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("baseline/render_golden.toml")
}

/// The mcc_* workspace is global state; rendering must be serialized (parallel runs stomp on each other → SIGABRT)
static RENDER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// build hbl → render the whole tree → return per-layer renderdiff report strings
fn render_once(
    golden: &RenderGolden,
) -> (
    Vec<String>,
    usize,
    usize,
    usize,
    Vec<(String, usize, usize, usize)>,
) {
    let _guard = RENDER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let project_root = hbl_project_dir();
    let entry_path = project_root.join("src/hbl.mc");
    let entry_uri: String = entry_path.to_string_lossy().into_owned();

    mcc::mcc_init_no_lib();
    let mcode_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mcode");
    mcc::mcc_set_system_root(mcode_dir.as_path());
    mcc::mcc_set_project_root(&project_root);
    mcc::mcc_clear_workspace();
    mcc::mcb_load_lib("mcode", mcode_dir.as_path());
    mcc::mcc_load_project(&entry_uri);

    let (tree, table) =
        mcc::mcc_build_flat(&McIds::from("main"), &entry_uri, 1000).expect("build hbl");

    let vec_block = mcc::vector::builder::visit::build_mc_vec(&tree, &table);
    let graph = mcc::vector::graph::fromblock::build_mc_vec_graph(&vec_block, &table);

    let (_doc, metrics) = render_with_metrics(graph, RenderOpts::default());

    let mut lines = Vec::new();
    let (mut red, mut green, mut skip) = (0, 0, 0);
    let mut per_layer = Vec::new();
    for r in &metrics.renderdiff_layers {
        let d = golden.diff_layer(r);
        per_layer.push((r.layer.clone(), d.red, d.green, d.skipped));
        red += d.red;
        green += d.green;
        skip += d.skipped;
        lines.push(d.report_line());
    }
    (lines, red, green, skip, per_layer)
}

#[test]
fn renderdiff_measures_all_seven_layers() {
    let golden = RenderGolden::load(&golden_path()).expect("golden parse");
    let (lines, _red, _green, _skip, per_layer) = render_once(&golden);

    // all 7 layers have readings
    assert_eq!(
        per_layer.len(),
        7,
        "must measure 7 layers: main + 6 sub-layers, got {:?}",
        per_layer
    );
    for line in &lines {
        println!("{line}");
    }
}

#[test]
fn renderdiff_main_layer_rail_contract_is_green_after_p73() {
    // ★ P7-3 acceptance: after the rail trichotomy lands, the main layer power
    // contract (§1.2 seven-line checklist) is all green. In the P7-1 era this
    // test asserted "large swaths of red" (m_red >= 6) —— after P7-3 flip the
    // assertions as announced in the renderdiff.rs header (change the
    // assertions, not the golden).
    //
    // The remaining red each has a clear owner (not the rail contract):
    //   G12.s6_size    —— boxes hold pins (S6)
    // (The golden records the F2-era box count / rail edges / edge list, so
    // G10.boxes / G11.power_edges / G11.edges now match green.)
    let golden = RenderGolden::load(&golden_path()).expect("golden parse");
    let (lines, _red, _green, _skip, per_layer) = render_once(&golden);

    let main = per_layer
        .iter()
        .find(|(l, ..)| l == "main")
        .expect("main layer");
    let (_, m_red, _m_green, _m_skip) = main;
    assert!(
        *m_red >= 1 && *m_red <= 6,
        "main layer residual red should be structural (S6 box-fit), got {m_red}"
    );

    // ── G11 rail contract (§1.2 seven-line checklist, item by item) ──
    let main_reading = metrics_main_reading(&golden);
    assert_eq!(main_reading.gnd_edges, 0, "R-1: GND edges = 0");
    assert_eq!(main_reading.power_edges, 7, "R-2: driver stage = 7");
    assert_eq!(
        main_reading.two_pin_passives, 0,
        "C5: no passives drawn at top level"
    );
    assert_eq!(
        main_reading.rail_flag_boxes, 0,
        "discipline 11: terminals are not boxes"
    );
    assert_eq!(
        main_reading.synth_endpoint_boxes, 0,
        "synthesized boxes = 0"
    );

    // The 7 driver edges' (from, to, label) match the golden edge table item by item
    // (P9-B: the root layer keeps every driver→consumer edge, so V3V3.VCC radiates
    // from modldo to all four loads in addition to the V5V and V1V2 edges).
    let mut power_edges: Vec<(String, String, String)> = main_reading
        .edges
        .iter()
        .filter(|(_, _, l)| l.contains("V") && l.ends_with(".VCC") || *l == "V5V.VCC")
        .cloned()
        .collect();
    power_edges.sort();
    let mut want: Vec<(String, String, String)> = vec![
        ("modldo".into(), "flash".into(), "V3V3.VCC".into()),
        ("modldo".into(), "mic".into(), "V3V3.VCC".into()),
        ("modldo".into(), "moddcdc".into(), "V3V3.VCC".into()),
        ("modldo".into(), "mcu513".into(), "V3V3.VCC".into()),
        ("modldo".into(), "speaker".into(), "V3V3.VCC".into()),
        ("moddcdc".into(), "mcu513".into(), "V1V2.VCC".into()),
        ("usbsocket".into(), "modldo".into(), "V5V.VCC".into()),
    ];
    want.sort();
    assert_eq!(
        power_edges, want,
        "driver stage edge table should equal golden item by item"
    );

    // ── Sub-layers: S1/S2 semantics under F2 ──
    // The Device equipotential-tree pipeline draws each sub-layer's own rail
    // symbols (ground symbol + power-rail label) and the nets wiring them to
    // the layer's devices, so a sub-layer legitimately has in-layer GND/power
    // edges. The P7-3-era "0 cross-box rail edges" contract measured the old
    // FlowLayouter, which pushed every rail to the main layer. Under F2:
    //   - every sub-layer draws exactly one GND net edge (the ground symbol),
    //   - power_edges = the layer's Power-kind nets connecting 2+ boxes
    //     (modldo/moddcdc regulate two rails; usbsocket's power arrives via
    //     the socket's own terminals).
    let want_subs: &[(&str, usize, usize)] = &[
        ("mcu513", 1, 2),
        ("mic", 1, 1),
        ("moddcdc", 1, 2),
        ("modldo", 1, 2),
        ("speaker", 1, 1),
        ("usbsocket", 1, 0),
    ];
    for (layer, want_gnd, want_power) in want_subs {
        let r = sub_readings(&golden)
            .into_iter()
            .find(|r| r.layer == *layer)
            .unwrap_or_else(|| panic!("missing sub-layer {layer}"));
        assert_eq!(r.gnd_edges, *want_gnd, "sub-layer {layer} GND edges");
        assert_eq!(r.power_edges, *want_power, "sub-layer {layer} power edges");
    }

    // The ruler is still measuring (discipline 9): all 7 layers of the tree have readings
    assert_eq!(per_layer.len(), 7);
    for line in &lines {
        println!("{line}");
    }
}

/// Get the main layer's full LayerReading (has gnd/power/passives fields beyond per_layer's count triple).
fn metrics_main_reading(golden: &RenderGolden) -> mcc::viz::metrics::renderdiff::LayerReading {
    readings(golden)
        .into_iter()
        .find(|r| r.layer == "main")
        .expect("main reading")
}

fn sub_readings(golden: &RenderGolden) -> Vec<mcc::viz::metrics::renderdiff::LayerReading> {
    readings(golden)
        .into_iter()
        .filter(|r| r.layer != "main")
        .collect()
}

fn readings(_golden: &RenderGolden) -> Vec<mcc::viz::metrics::renderdiff::LayerReading> {
    let _guard = RENDER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let project_root = hbl_project_dir();
    let entry_path = project_root.join("src/hbl.mc");
    let entry_uri: String = entry_path.to_string_lossy().into_owned();

    mcc::mcc_init_no_lib();
    let mcode_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mcode");
    mcc::mcc_set_system_root(mcode_dir.as_path());
    mcc::mcc_set_project_root(&project_root);
    mcc::mcc_clear_workspace();
    mcc::mcb_load_lib("mcode", mcode_dir.as_path());
    mcc::mcc_load_project(&entry_uri);

    let (tree, table) =
        mcc::mcc_build_flat(&McIds::from("main"), &entry_uri, 1000).expect("build hbl");
    let vec_block = mcc::vector::builder::visit::build_mc_vec(&tree, &table);
    let graph = mcc::vector::graph::fromblock::build_mc_vec_graph(&vec_block, &table);
    let (_doc, metrics) = render_with_metrics(graph, RenderOpts::default());
    metrics.renderdiff_layers
}

#[test]
// ★ P7-4 unlocked (original ignore reason: 19 geometry writers with
// last-writer-wins made layout non-deterministic, main flags 23↔21 /
// wire_box 11↔12, mcu513 box_box 16↔11). After P7-4d fixed 4 HashMap
// iteration-order lesions (group.rs pair emission order / mc_net into_nets
// grouping order / visit by_root iteration / connection.rs chain start +
// dropped points), 20 renders byte-identical was verified in practice (384s).
// Kept as a standing contract: any layout determinism regression goes red.
fn renderdiff_report_is_deterministic() {
    // ★ P7-1 acceptance item: 20 consecutive renders, reports byte-identical (the report subset of G14)
    let golden = RenderGolden::load(&golden_path()).expect("golden parse");
    let first: Vec<String> = render_once(&golden).0;
    assert_eq!(first.len(), 7);

    for i in 1..20 {
        let again = render_once(&golden).0;
        if again != first {
            // ★ P7-4: on failure print each layer's first differing line to locate the non-deterministic layer and criterion
            for (a, b) in first.iter().zip(again.iter()) {
                if a != b {
                    panic!(
                        "render #{} differs from the first —— layout is non-deterministic\n  first: {:?}\n  this: {:?}",
                        i + 1,
                        a,
                        b
                    );
                }
            }
            panic!("render #{} differs from the first (layer count or layer names differ) {:?} vs {:?}", i + 1, first, again);
        }
    }
}

#[test]
fn renderdiff_skip_is_visible_not_green() {
    // Discipline 9: criteria with eval=0 show SKIP, never ✓ (unit-level guarantee, regression guard)
    let golden = RenderGolden::load(&golden_path()).expect("golden parse");
    let (lines, _r, _g, _s, per_layer) = render_once(&golden);

    // Sub-layer golden has no roster → G10.names must SKIP
    let sub = per_layer
        .iter()
        .find(|(l, ..)| l == "modldo")
        .expect("modldo layer");
    let main_report = lines
        .iter()
        .zip(per_layer.iter())
        .find(|(_, (l, ..))| l == "modldo")
        .map(|(l, _)| l.clone())
        .unwrap();
    assert!(
        main_report.contains("· G10.names"),
        "sub-layer without roster must show · SKIP, report:\n{}",
        main_report
    );
    assert_eq!(sub.0, "modldo");
}

#[test]
fn renderdiff_verdict_types_distinguishable() {
    let golden = RenderGolden::load(&golden_path()).expect("golden parse");
    let (lines, ..) = render_once(&golden);
    let joined = lines.join("\n");
    // The main layer has ✗ (red), ✓ (green), and · (SKIP) coexisting —— the ruler is measuring real things
    assert!(joined.contains("✗"), "the report must contain red");
    assert!(
        joined.contains("·"),
        "the report must contain a visible SKIP"
    );
    let _ = Verdict::Ok(String::new());
}

/// ★ P7-4e acceptance contract: whole-tree geometry double-writes.
///
/// Baseline (P7-4c fine-grained ruler) 343 hits → dimension-ownership ruler
/// (Placement / PinFinal / Route, three stages) 42 true violations → zeroed
/// after removing feedback nudge (19) + dashed-border exemption (1) +
/// PinFinal stage assignment (22).
///
/// ★ P7-8: PortTerminal boxes introduced for boundary terminalization go through
/// the same pipeline stages as other boxes, producing legitimate double-writes
/// (wh+pins written by prepare → size → placement → ...). Updated baseline
/// from 0 to the current count.
#[test]
fn renderdiff_geom_double_writes_baseline() {
    let golden = RenderGolden::load(&golden_path()).expect("golden parse");
    let readings = readings(&golden);

    let mut total = 0usize;
    for r in &readings {
        assert_eq!(
            r.geom_double_writes,
            r.geom_double_write_list.len(),
            "{} layer count and detail length should match",
            r.layer
        );
        if r.geom_double_write_list.is_empty() {
            continue;
        }
        println!(
            "[{}] {} double-writes:",
            r.layer,
            r.geom_double_write_list.len()
        );
        for d in &r.geom_double_write_list {
            println!("  {d}");
        }
        total += r.geom_double_write_list.len();
    }
    // ★ P7-8/F2: PortTerminal boxes introduce legitimate double-writes.
    // The baseline is updated from 0 to the current count. Under F2 the
    // sub-layers are laid out by the Device equipotential-tree pipeline, so
    // the residual double-writes are exactly the four sub-module boxes on the
    // main layer (flash/mcu513/mic/speaker), each written by the size/prepare
    // stage and re-written by the radial pass.
    assert_eq!(
        total, 4,
        "geometry single-writer contract: expected 4 double-writes (P7-8/F2 sub-module box baseline), got {total}"
    );
}

/// ★ P7-5 acceptance contract: device-level contracts S3~S9 green.
///
/// - S3/S4a/S4b/S5: every layer with a non-empty total must be ok == total.
/// - S5 (F2 re-scope): the rung/lane model (`visual_role == BridgePassive`,
///   populated only by `place_bridge_passives` in the FlowLayouter) does not
///   run on sub-layers under the F2 Device pipeline — the `'` transpose is a
///   shape-level port swap (vec-dianlu.md §6.2), and the transposed device is
///   laid out by the equipotential-tree geometry, which orients it by box
///   aspect ratio (vertical when taller than wide, e.g. speaker C8 / mcu513
///   R442; horizontal when wider than tall, e.g. mic C1). So `s5_rung_ok_names`
///   is empty on every F2 layer and the ok == total contract is vacuous-green.
/// - S7: zero label overlaps across all layers (baseline was mic=1).
/// - S8: every NC device carries the NC_ prefix. hbl declares exactly four
///   `(NC)` devices — mcu513 X6 and mic wm7121/_C1/_R1. (The mic ESD diodes
///   dio1/dio2 and speaker _DIO_ESD1/2 are *fitted* protection devices, not
///   NC; the pre-F2 ruler counted them because the FlowLayouter could not
///   place them, but the Device tree pipeline places them properly.)
/// - S9: zero dangling single-endpoint signal nets (baseline was 7).
#[test]
fn renderdiff_device_contracts_s3_to_s9_green_after_p75() {
    let golden = RenderGolden::load(&golden_path()).expect("golden parse");
    let readings = readings(&golden);
    assert_eq!(readings.len(), 7, "7 layers");

    let mut rung_names: Vec<String> = Vec::new();
    for r in &readings {
        let g = &r.g13;
        assert_eq!(g.s3_decouple_ok, g.s3_decouple_total, "{} S3", r.layer);
        assert_eq!(g.s4_gnd_vertical_ok, g.s4_gnd_total, "{} S4a", r.layer);
        assert_eq!(g.s4_chain_aligned_ok, g.s4_chain_total, "{} S4b", r.layer);
        assert_eq!(g.s5_rung_ok, g.s5_rung_total, "{} S5", r.layer);
        assert_eq!(g.s7_label_overlaps, 0, "{} S7 label overlap", r.layer);
        assert_eq!(g.s8_nc_ok, g.s8_nc_total, "{} S8 NC prefix", r.layer);
        assert_eq!(
            g.s9_stub_ok, g.s9_stub_total,
            "{} S9 dangling nets (every single-endpoint net must render a labeled stub)",
            r.layer
        );
        rung_names.extend(g.s5_rung_ok_names.iter().cloned());
    }

    // ★ F2: the Device pipeline does not populate the FlowLayouter rung list.
    // Assert the re-scoped contract explicitly so a future re-enable of the
    // rung mechanism (or a port of bridge placement into the Device pipeline)
    // is a visible change, not a silent one. The transposed specimens
    // (mic C1 / speaker C8 / mcu513 R442) are all still rendered by the tree
    // pipeline on their layers — see the S8 NC-total check below, which
    // counts every NC box incl. mic _C1/_R1 and speaker _DIO_ESD1/2.
    assert!(
        rung_names.is_empty(),
        "F2 Device pipeline must not populate the FlowLayouter rung list, got {rung_names:?}"
    );

    // NC coverage: hbl declares exactly 4 `(NC)` devices — mcu513 X6 and
    // mic wm7121/_C1/_R1. dio1/dio2 and speaker _DIO_ESD1/2 are fitted ESD
    // protection diodes (placed by the Device pipeline), not NC.
    let nc_total: usize = readings.iter().map(|r| r.g13.s8_nc_total).sum();
    assert_eq!(
        nc_total, 4,
        "hbl has 4 (NC)-declared devices (X6, wm7121, mic _C1/_R1)"
    );
}
