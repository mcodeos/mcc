// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Virtual instantiation for non-project single-file views (mcd docs-mc
// 16-export-viz §6): a file opened outside a project (no project.toml) that
// has no `module` but declares components/interfaces must not fail with
// "no top module found"; each unit is wrapped in a synthetic module so the
// standard build + viz pipeline can render it standalone.

use mcc::McURI;
use std::fs;
use std::sync::{Mutex, OnceLock};

static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn fixture(name: &str, content: &str) -> (std::path::PathBuf, McURI) {
    let dir = std::env::temp_dir().join(format!("mcc-virtual-{}-{}", name, std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("part.mc");
    fs::write(&path, content).unwrap();
    let uri: McURI = path.to_string_lossy().into_owned();
    (path, uri)
}

fn setup(uri: &McURI) {
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();
    mcc::mcc_load_project(uri);
}

const COMPONENT_ONLY: &str = r#"
component HUM011D_5_S
{
    partno = "HUM011D_5_S"
    package = "USB-MINI-SOCKET"
    pins = [
        [1, [5,6,7]] = [VBUS, GND]::DC(5V)
        2 = D\-
        3 = D\+
        4 = ID
        8 = SHIELD3
        9 = SHIELD4
    ]
}
"#;

#[test]
fn component_only_file_resolves_and_builds() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (path, uri) = fixture("comp", COMPONENT_ONLY);
    setup(&uri);

    let targets = mcc::mcc_virtual_resolve_targets(&uri, None).expect("resolve targets");
    assert_eq!(targets, vec!["HUM011D_5_S".to_string()]);

    let (inst, table) =
        mcc::mcc_virtual_build_flat(&targets[0], &uri, 1000).expect("virtual build must succeed");
    assert_eq!(inst.name, "VIRT_HUM011D_5_S");

    let mut pins = Vec::new();
    for net in table.get_nets() {
        for &pid in &net.points {
            if let Some(e) = table.get_entry(pid) {
                pins.push(e.path.clone());
            }
        }
    }
    // The lone component is unwired, so the component itself is present as an
    // instance; the point here is the build succeeds instead of E32107.
    let _ = pins;
    let diags = mcc::mcc_diagnose_all();
    let has_32107_style: Vec<_> = diags
        .iter()
        .filter(|d| d.msg.contains("no top module found"))
        .collect();
    assert!(has_32107_style.is_empty());

    fs::remove_dir_all(path.parent().unwrap()).ok();
}

#[test]
fn component_view_renders_box() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (path, uri) = fixture("comp-viz", COMPONENT_ONLY);
    setup(&uri);

    let targets = mcc::mcc_virtual_resolve_targets(&uri, None).expect("resolve targets");
    let (inst, table) =
        mcc::mcc_virtual_build_flat(&targets[0], &uri, 1000).expect("virtual build must succeed");
    let block = mcc::build_mc_vec(&inst, &table);
    let graph =
        mcc::mcc_virtual_prepare_graph(mcc::build_mc_vec_graph(&block, &table), &targets[0]);
    let doc = mcc::viz::api::render(graph);
    let html = mcc::viz::template::wrap_document(&doc);

    assert!(
        html.contains("HUM011D_5_S"),
        "the component class name must render, got {} bytes",
        html.len()
    );
    assert!(
        !html.contains("u_1"),
        "the fabricated instance name must be hidden"
    );
    assert!(doc.validate().is_empty(), "invalid visualization document");
    fs::remove_dir_all(path.parent().unwrap()).ok();
}

#[test]
fn component_view_renders_pin_name_and_id() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (path, uri) = fixture("comp-pins", COMPONENT_ONLY);
    setup(&uri);

    let targets = mcc::mcc_virtual_resolve_targets(&uri, None).expect("resolve targets");
    let (inst, table) =
        mcc::mcc_virtual_build_flat(&targets[0], &uri, 1000).expect("virtual build must succeed");
    let block = mcc::build_mc_vec(&inst, &table);
    let graph =
        mcc::mcc_virtual_prepare_graph(mcc::build_mc_vec_graph(&block, &table), &targets[0]);
    let doc = mcc::viz::api::render(graph);
    let html = mcc::viz::template::wrap_document(&doc);

    // The SVG is JSON-escaped inside the HTML document (`\"`); unescape to
    // assert on the rendered pin groups.
    let svg = html.replace("\\\"", "\"");
    for name in ["VBUS", "GND", "D-", "D+", "ID", "SHIELD3", "SHIELD4"] {
        assert!(
            svg.contains(name),
            "pin name {name} must render on the component view"
        );
    }
    // Pin ids 1..9: each physical pin renders a stub with its number.
    for id in ["1", "5", "6", "7", "8", "9"] {
        assert!(
            svg.contains(&format!(">{id}<")),
            "pin id {id} must render on the component view"
        );
    }
    // Every pin draws a stub line; the virtual view wires nothing, so no NC
    // cross marks appear.
    let pin_groups = svg.matches("class=\"pin\"").count();
    assert_eq!(pin_groups, 9, "all 9 pins must render as stubs");
    assert!(
        !svg.contains("stroke=\"#C0392B\""),
        "virtual view must not draw NC cross marks"
    );
    fs::remove_dir_all(path.parent().unwrap()).ok();
}

const MULTI_MODULE: &str = r#"
module BLINKER { }
module BUZZER { }
"#;

#[test]
fn multi_module_file_resolves_all_modules() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (path, uri) = fixture("multi", MULTI_MODULE);
    setup(&uri);

    let targets = mcc::mcc_virtual_resolve_targets(&uri, None).expect("resolve targets");
    assert_eq!(targets.len(), 2);
    assert!(targets.contains(&"BLINKER".to_string()));
    assert!(targets.contains(&"BUZZER".to_string()));
    fs::remove_dir_all(path.parent().unwrap()).ok();
}

const INTERFACE_ONLY: &str = r#"
interface I2C(role)
{
    pins = [
        1 = SCL
        2 = SDA
    ]
    role Master { name = "Master" }
}
"#;

#[test]
fn interface_only_file_resolves_and_builds() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (path, uri) = fixture("iface", INTERFACE_ONLY);
    setup(&uri);

    let targets = mcc::mcc_virtual_resolve_targets(&uri, None).expect("resolve targets");
    assert_eq!(targets, vec!["I2C".to_string()]);

    let (inst, _table) =
        mcc::mcc_virtual_build_flat(&targets[0], &uri, 1000).expect("virtual build must succeed");
    assert_eq!(inst.name, "VIRT_I2C");
    fs::remove_dir_all(path.parent().unwrap()).ok();
}

#[test]
fn component_view_default_pin_order_is_counterclockwise() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (path, uri) = fixture(
        "ccw",
        r#"
component CONN12
{
    pins = [
        9 = P9
        10 = P10
        11 = P11
        12 = P12
        5 = P5
        6 = P6
        7 = P7
        8 = P8
        1 = P1
        2 = P2
        3 = P3
        4 = P4
    ]
}
"#,
    );
    setup(&uri);

    let targets = mcc::mcc_virtual_resolve_targets(&uri, None).expect("resolve targets");
    let (inst, table) =
        mcc::mcc_virtual_build_flat(&targets[0], &uri, 1000).expect("virtual build must succeed");
    let block = mcc::build_mc_vec(&inst, &table);
    let graph =
        mcc::mcc_virtual_prepare_graph(mcc::build_mc_vec_graph(&block, &table), &targets[0]);
    let b = &graph.boxes[0];
    let got: Vec<(&str, &str)> = b
        .entry_points
        .iter()
        .map(|ep| {
            let id = b
                .pins
                .iter()
                .find(|p| p.id == ep.pin_id)
                .map(|p| p.pin_id.as_str())
                .unwrap_or_default();
            (
                id,
                match ep.side {
                    mcc::vector::graph::EntrySide::Top => "Top",
                    mcc::vector::graph::EntrySide::Right => "Right",
                    mcc::vector::graph::EntrySide::Bottom => "Bottom",
                    mcc::vector::graph::EntrySide::Left => "Left",
                },
            )
        })
        .collect();
    // 12 pins, no layout -> counterclockwise on the left/right columns only,
    // ordered by numeric pin number (not alphabetical: 10/11/12 come after 9):
    // left 6 (top→bottom), right 6 (bottom→top), no top/bottom pins.
    let expected: Vec<(&str, &str)> = vec![
        ("1", "Left"),
        ("2", "Left"),
        ("3", "Left"),
        ("4", "Left"),
        ("5", "Left"),
        ("6", "Left"),
        ("7", "Right"),
        ("8", "Right"),
        ("9", "Right"),
        ("10", "Right"),
        ("11", "Right"),
        ("12", "Right"),
    ];
    assert_eq!(
        got, expected,
        "default pin order must be counterclockwise and numeric"
    );
    // Right column reads bottom→top (counterclockwise continuation).
    let right_offsets: Vec<f64> = b
        .entry_points
        .iter()
        .filter(|ep| matches!(ep.side, mcc::vector::graph::EntrySide::Right))
        .map(|ep| ep.offset)
        .collect();
    assert!(
        right_offsets.windows(2).all(|w| w[0] > w[1]),
        "right column must run bottom→top, got {right_offsets:?}"
    );
    fs::remove_dir_all(path.parent().unwrap()).ok();
}

#[test]
fn group_range_bus_pins_all_register() {
    // Regression: `io [4:11] = IO0{0:7}` must register 8 pins (IO00..IO07).
    // as_bus() used to ignore the numeric range inside curly braces, so the
    // RHS bus had zero members and the whole slice registered no pins.
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (path, uri) = fixture(
        "busrange",
        r#"
component PCA9555(partno)
{
    pins = [
        io [4:11] = IO0{0:7}
        io [13:20] = IO1{0:7}
    ]
}
"#,
    );
    setup(&uri);

    let targets = mcc::mcc_virtual_resolve_targets(&uri, None).expect("resolve targets");
    let (inst, table) =
        mcc::mcc_virtual_build_flat(&targets[0], &uri, 1000).expect("virtual build must succeed");
    let block = mcc::build_mc_vec(&inst, &table);
    let graph = mcc::build_mc_vec_graph(&block, &table);
    let b = &graph.boxes[0];
    assert_eq!(b.pins.len(), 16, "16 IO pins must register");
    for i in 0..8 {
        assert!(
            b.pins.iter().any(|p| p.pin_id == format!("{}", 4 + i)),
            "pin {} missing",
            4 + i
        );
        assert!(
            b.pins.iter().any(|p| p.pin_id == format!("{}", 13 + i)),
            "pin {} missing",
            13 + i
        );
    }
    assert!(
        b.pins.iter().any(|p| p.description == "IO07"),
        "IO07 member name must be present"
    );
    fs::remove_dir_all(path.parent().unwrap()).ok();
}

#[test]
fn component_view_box_w_fits_left_plus_right_pin_names() {
    // Regression: a long left-column name and a long right-column name both
    // render INSIDE the box (left from the left edge inward, right from the
    // right edge inward — `pin_render.rs` `label_positions`), so the box must
    // be at least `left_longest + right_longest` wide. A single-widest-label
    // width lets `PRIMARY+` collide with `SECONDARY+` mid-box.
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (path, uri) = fixture(
        "wide-pins",
        r#"
component XFR
{
    pins = [
        1 = PRIMARY\+
        2 = PRIMARY\-
        3 = SECONDARY\+
        4 = SECONDARY\-
    ]
}
"#,
    );
    setup(&uri);

    let targets = mcc::mcc_virtual_resolve_targets(&uri, None).expect("resolve targets");
    let (inst, table) =
        mcc::mcc_virtual_build_flat(&targets[0], &uri, 1000).expect("virtual build must succeed");
    let block = mcc::build_mc_vec(&inst, &table);
    let mut graph =
        mcc::mcc_virtual_prepare_graph(mcc::build_mc_vec_graph(&block, &table), &targets[0]);
    // The fallback sizing runs inside the device layout; the render path calls
    // it, so drive it directly to assert on the box width.
    mcc::viz::layout::equipotential_tree::layout_device_layer(&mut graph);
    let b = graph
        .boxes
        .iter()
        .find(|b| b.class_name == "XFR")
        .expect("XFR component box must be present");

    let longest_on = |side: mcc::vector::graph::EntrySide| -> usize {
        b.entry_points
            .iter()
            .filter(|ep| ep.side == side)
            .map(|ep| {
                b.pins
                    .iter()
                    .find(|p| p.id == ep.pin_id)
                    .map(|p| {
                        if p.description.is_empty() {
                            p.pin_id.clone()
                        } else {
                            p.description.clone()
                        }
                    })
                    .unwrap_or_default()
                    .chars()
                    .count()
            })
            .max()
            .unwrap_or(0)
    };
    let left = longest_on(mcc::vector::graph::EntrySide::Left);
    let right = longest_on(mcc::vector::graph::EntrySide::Right);
    assert!(
        left >= 8 && right >= 9,
        "sides mis-assigned: {left} / {right}"
    );
    // 7 px/char (LABEL_CHAR_W) + 3 * 16 px pad (LABEL_PAD) — keep the literals
    // in sync with the layout constants.
    let need = (left + right) as f64 * 7.0 + 3.0 * 16.0;
    assert!(
        b.w >= need,
        "box width {:.0} must fit left({left}) + right({right}) pin names (need {need:.0})",
        b.w
    );
    fs::remove_dir_all(path.parent().unwrap()).ok();
}

#[test]
fn virtual_view_skips_unwired_pin_diagnostics() {
    // E4112 "no pins connected to any net" / E4116 "N of M pins connected" are
    // false positives on a virtually-instantiated standalone component view —
    // an unwired box is exactly what such a view IS. The fabricated wrapper is
    // flagged `synthetic` at build time, and both checks must skip it.
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (path, uri) = fixture(
        "xtal-view",
        r#"
component XTAL.CERAMIC
{
    pins = [
        [1,2] = XTAL{X1,X2}
    ]
}
"#,
    );
    setup(&uri);

    let targets = mcc::mcc_virtual_resolve_targets(&uri, None).expect("resolve targets");
    let (inst, table) =
        mcc::mcc_virtual_build_flat(&targets[0], &uri, 1000).expect("virtual build must succeed");
    let _ = (inst, table);

    let diags = mcc::mcc_diagnose_all();
    let bad: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 4112 | 4116))
        .collect();
    assert!(
        bad.is_empty(),
        "virtual view must not report unwired / pin-count diagnostics: {:?}",
        bad
    );
    fs::remove_dir_all(path.parent().unwrap()).ok();
}

#[test]
fn virtual_targets_follow_source_declaration_order() {
    // The multi-target combined viz view stacks one SVG per target, in the
    // order `resolve_targets` hands back — and the workspace class table is a
    // DashMap (hash order). Targets must follow the .mc source declaration
    // order, not hash order and not alphabetical order (the names here are
    // deliberately non-alphabetical: DELTA before ALPHA before CHARLIE).
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let (path, uri) = fixture(
        "decl-order",
        r#"
component DELTA
{
    pins = [ 1 = X, "x" ]
}
component ALPHA
{
    pins = [ 1 = X, "x" ]
}
component CHARLIE
{
    pins = [ 1 = X, "x" ]
}
"#,
    );
    setup(&uri);

    let targets = mcc::mcc_virtual_resolve_targets(&uri, None).expect("resolve targets");
    assert_eq!(
        targets,
        vec![
            "DELTA".to_string(),
            "ALPHA".to_string(),
            "CHARLIE".to_string()
        ],
        "targets must follow source declaration order"
    );
    fs::remove_dir_all(path.parent().unwrap()).ok();
}
