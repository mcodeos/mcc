//! ★ Unified layout policy — a `layout=[...]` component inside a MODULE
//! sub-layer.
//!
//! Every module sub-layer is rendered by the device pipeline
//! (`layout_device_layer`), not by `circuit_flow`. Before this test the device
//! pipeline ignored `layout_hint` entirely, so `hbl`'s `USB.MINI_B` drew its
//! unconnected `D+ / D- / ID` as NC crosses on topology-chosen edges. This
//! fixture pins the policy down on a real `.mc` project: declared pins keep the
//! declared edge and the declared counterclockwise order, connected or not.

mod common;

use std::path::PathBuf;

use mcc::vector::graph::{EntrySide, LayerStyle, McVecBox, McVecGraph};

/// The mcc_* workspace is global state; rendering must be serialized.
static RENDER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn project_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/layout_dev")
}

fn build_graph() -> McVecGraph {
    let _guard = RENDER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let project_root = project_dir();
    let entry_path = project_root.join("src/main.mc");
    let entry_uri: String = entry_path.to_string_lossy().into_owned();

    mcc::mcc_init();
    mcc::mcc_set_project_root(&project_root);
    mcc::mcc_load_project(&entry_uri);

    let (tree, table, arena, store) =
        mcc::mcc_build_flat_with_arena(&mcc::McIds::from("main"), &entry_uri, 1000)
            .expect("build layout_dev");
    let vec_block = mcc::vector::builder::visit::build_mc_vec(&tree, &table, &arena, &store);
    mcc::vector::graph::fromblock::build_mc_vec_graph(&vec_block, &table)
}

fn find_box<'a>(g: &'a McVecGraph, class: &str) -> Option<&'a McVecBox> {
    if let Some(b) = g.boxes.iter().find(|b| b.class_name == class) {
        return Some(b);
    }
    g.sub_graphs.iter().find_map(|c| find_box(c, class))
}

/// The graph that directly owns `bid`, navigating the sub-graph tree.
fn owner_of<'a>(g: &'a mut McVecGraph, bid: i64) -> Option<&'a mut McVecGraph> {
    if g.boxes.iter().any(|b| b.id == bid) {
        return Some(g);
    }
    for c in &mut g.sub_graphs {
        if let Some(o) = owner_of(c, bid) {
            return Some(o);
        }
    }
    None
}

/// Physical pin numbers on one edge, in geometric order along it (offset
/// ascending: top→bottom for Left/Right, left→right for Top/Bottom).
fn edge_pin_numbers(b: &McVecBox, side: EntrySide) -> Vec<String> {
    let mut slots: Vec<_> = b.slots.iter().filter(|s| s.side == side).collect();
    slots.sort_by(|a, c| a.offset.partial_cmp(&c.offset).unwrap());
    slots
        .iter()
        .map(|s| {
            b.pins
                .iter()
                .find(|p| p.id == s.pin_id)
                .map(|p| p.pin_id.clone())
                .unwrap_or_else(|| s.pin_id.to_string())
        })
        .collect()
}

#[test]
fn layout_component_in_module_sub_layer_follows_author_layout() {
    let mut graph = build_graph();
    let sock_id = find_box(&graph, "CONN.USB_MINI_B").expect("sock box").id;
    assert!(
        find_box(&graph, "CONN.USB_MINI_B")
            .unwrap()
            .has_pin_layout(),
        "the author layout must be attached to the box by the builder"
    );

    // Replicate the render decision for a module sub-layer (api.rs forces it).
    let sub = owner_of(&mut graph, sock_id).expect("owner sub-layer");
    sub.layer_style = LayerStyle::Device;
    mcc::viz::layout::equipotential_tree::layout_device_layer(sub);

    let b = sub
        .boxes
        .iter()
        .find(|b| b.id == sock_id)
        .expect("sock box");

    // Every listed pin is drawn — including the three that are not wired.
    assert_eq!(b.pins.len(), 9, "the connector has 9 pins");
    assert_eq!(b.slots.len(), 9, "every physical pin gets a slot");
    assert_eq!(
        b.entry_points.len(),
        9,
        "every physical pin gets an entry point"
    );
    assert!(
        b.nc_pins().is_empty(),
        "D+/D-/ID are declared in the layout, so they must be drawn as pins, not NC crosses"
    );

    // `right = [4, 3, 2, 5, 1]` — the list reads bottom→top, so the last entry
    // (pin 1) is topmost.
    assert_eq!(
        edge_pin_numbers(b, EntrySide::Right),
        vec!["1", "5", "2", "3", "4"]
    );
    // `bottom = [6:9]` — the interval expands in order, left→right.
    assert_eq!(
        edge_pin_numbers(b, EntrySide::Bottom),
        vec!["6", "7", "8", "9"]
    );
    assert!(edge_pin_numbers(b, EntrySide::Left).is_empty());
    assert!(edge_pin_numbers(b, EntrySide::Top).is_empty());

    // CCW offsets: `(k+1)/(n+1)` on the entry's own end of the edge.
    let right_offsets: Vec<f64> = {
        let mut slots: Vec<_> = b
            .slots
            .iter()
            .filter(|s| s.side == EntrySide::Right)
            .collect();
        slots.sort_by(|a, c| a.offset.partial_cmp(&c.offset).unwrap());
        slots.iter().map(|s| s.offset).collect()
    };
    for (k, off) in right_offsets.iter().enumerate() {
        let want = (k + 1) as f64 / 6.0;
        assert!(
            (off - want).abs() < 1e-9,
            "right pin #{k} offset {off} != {want}"
        );
    }

    // Connectivity still comes from the netlist: VBUS(1) and the five grounds
    // (5,6,7,8,9) are wired; the three data pins are not.
    let connected: Vec<String> = b
        .slots
        .iter()
        .filter(|s| s.connected)
        .map(|s| {
            b.pins
                .iter()
                .find(|p| p.id == s.pin_id)
                .map(|p| p.pin_id.clone())
                .unwrap()
        })
        .collect();
    for wired in ["1", "5", "6", "7", "8", "9"] {
        assert!(
            connected.contains(&wired.to_string()),
            "pin {wired} is wired"
        );
    }
    for nc in ["2", "3", "4"] {
        assert!(
            !connected.contains(&nc.to_string()),
            "pin {nc} has no net and must stay unconnected"
        );
    }

    // The box is sized for the author's edges (5 right / 4 bottom pins, one
    // PIN_PITCH per ratio step) and frozen.
    assert!(b.geom_locked);
    assert!(
        b.h >= 6.0 * 40.0,
        "right edge needs (5+1) pitches, got h={}",
        b.h
    );
    assert!(
        b.w >= 5.0 * 40.0,
        "bottom edge needs (4+1) pitches, got w={}",
        b.w
    );
}
