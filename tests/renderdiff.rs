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
    //   G10.boxes/names —— dangling-port terminal boxes (contract C4, P7-5 domain)
    //   G11.edges      —— signal-net display names still __net_N (net-name projection, P7-5 domain)
    //   G12.s6_size    —— boxes hold pins (S6)
    let golden = RenderGolden::load(&golden_path()).expect("golden parse");
    let (lines, _red, _green, _skip, per_layer) = render_once(&golden);

    let main = per_layer
        .iter()
        .find(|(l, ..)| l == "main")
        .expect("main layer");
    let (_, m_red, _m_green, _m_skip) = main;
    assert!(
        *m_red >= 2 && *m_red <= 6,
        "main layer residual red should be structural (boxes/names/edges/s6), got {m_red}"
    );

    // ── G11 rail contract (§1.2 seven-line checklist, item by item) ──
    let main_reading = metrics_main_reading(&golden);
    assert_eq!(main_reading.gnd_edges, 0, "R-1: GND edges = 0");
    assert_eq!(main_reading.power_edges, 4, "R-2: driver stage = 4");
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

    // The 4 driver edges' (from, to, label) match the golden edge table item by item
    let mut power_edges: Vec<(String, String, String)> = main_reading
        .edges
        .iter()
        .filter(|(_, _, l)| l.contains("V") && l.ends_with(".VCC") || *l == "V5V.VCC")
        .cloned()
        .collect();
    power_edges.sort();
    let mut want: Vec<(String, String, String)> = vec![
        ("modldo".into(), "moddcdc".into(), "V3V3.VCC".into()),
        ("modldo".into(), "mcu513".into(), "V3V3.VCC".into()),
        ("moddcdc".into(), "mcu513".into(), "V1V2.VCC".into()),
        ("usbsocket".into(), "modldo".into(), "V5V.VCC".into()),
    ];
    want.sort();
    assert_eq!(
        power_edges, want,
        "driver stage edge table should equal golden item by item"
    );

    // ── Sub-layers: S1/S2 semantics = no cross-box rail edges (all symbols in place) ──
    for r in sub_readings(&golden) {
        assert_eq!(
            r.gnd_edges, 0,
            "sub-layer {} GND edges should be 0",
            r.layer
        );
        assert_eq!(
            r.power_edges, 0,
            "sub-layer {} power edges should be 0",
            r.layer
        );
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

/// ★ P7-4e acceptance contract: whole-tree geometry double-writes = 0.
///
/// Baseline (P7-4c fine-grained ruler) 343 hits → dimension-ownership ruler
/// (Placement / PinFinal / Route, three stages) 42 true violations → zeroed
/// after removing feedback nudge (19) + dashed-border exemption (1) +
/// PinFinal stage assignment (22). Any regression goes red: any change that
/// writes geometry across stage boundaries is exposed here.
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
    assert_eq!(
        total, 0,
        "geometry single-writer contract broken: {total} cross-stage unauthorized writes (list in output above)"
    );
}

/// ★ P7-5 acceptance contract: device-level contracts S3~S9 green.
///
/// - S3/S4a/S4b/S5: every layer with a non-empty total must be ok == total.
/// - S5 specimens (roadmap §1.3): mic C1 / speaker C8 / mcu513 R442 must all
///   appear in the rung pass list (no device-name special-casing upstream —
///   the `'` modifier drives it; this only pins the result).
/// - S7: zero label overlaps across all layers (baseline was mic=1).
/// - S8: every NC device carries the NC_ prefix (hbl: mcu513 X6,
///   mic wm7121/dio1/dio2/CAP_1/RES_1, speaker DIO_ESD_1/2).
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
        assert_eq!(g.s9_stub_total, 0, "{} S9 dangling nets", r.layer);
        rung_names.extend(g.s5_rung_ok_names.iter().cloned());
    }

    for specimen in ["C1", "C8", "R442"] {
        assert!(
            rung_names.contains(&specimen.to_string()),
            "S5 specimen {specimen} missing from rung pass list: {rung_names:?}"
        );
    }

    // NC coverage: hbl declares 8 NC devices across 3 layers.
    let nc_total: usize = readings.iter().map(|r| r.g13.s8_nc_total).sum();
    assert_eq!(
        nc_total, 8,
        "hbl has 8 NC devices (X6, wm7121, dio1, dio2, mic CAP_1/RES_1, speaker DIO_ESD_1/2)"
    );
}
