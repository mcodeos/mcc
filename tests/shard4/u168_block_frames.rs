// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Integration test: block-partition frames in viz (CIMP §1 U168).
//
// A `block` stays group-only on the semantic face (U122's ruling is untouched);
// what is new is the **display-only projection**: the body's partition tree is
// collected at parse time, threaded through the flat table, and the viz layer
// draws one dashed frame per partition that owns drawn boxes — a box belonging
// to the innermost partition whose source span declares it.
//
// What is locked here:
//
//   1. The partition table is collected from the AST with nesting and source
//      order, and its spans mean containment: a statement written inside a
//      block is inside that block's span (innermost wins over its parent), a
//      statement outside every block is inside none.
//   2. The frames follow the spans: a layer's frames are exactly the
//      partitions that own boxes (parents before children), and every member
//      box's rect lies inside its own frame.
//   3. The twin with no partitions draws nothing: an empty table and an empty
//      frame list, so boards that write no `block` render exactly as before.
//   4. The grouping law (CIMP §1 U171): the frame pass is a pure derivation —
//      it moves no box. Frames may overlap each other or enclose a stranger;
//      membership is declared by source span, never by geometry.
//
// Every branch of the fixture has two members: two top-level partitions, two
// boxes directly under the module, two under `power`, and two under the nested
// `trim` — so an engine that dropped any one region could not pass by silence.

#![allow(non_snake_case)]

use crate::common;

use mcc::vector::graph::{LayerStyle, McVecGraph};

/// A module with two sibling partitions, one nested partition, and two boxes
/// outside every partition.
const BOARDED: &str = r#"component RES
{
    pins = [
        1 = 1
        2 = 2
    ]
}

module main(psnk GND)
{
    RES R_top1
    RES R_top2
    block power
    {
        RES R_a
        RES R_b
        R_a.1 -> R_b.1
        block trim
        {
            RES R_a1
            RES R_a2
            R_a1.1 -> R_a.2
            R_a2.1 -> GND
        }
    }
    block idle
    {
        RES R_z1
        RES R_z2
        R_z1.1 -> GND
        R_z2.1 -> GND
    }
    R_top1.1 -> R_a.1
    R_top2.1 -> R_b.2
}
"#;

const UNPARTITIONED: &str = r#"component RES
{
    pins = [
        1 = 1
        2 = 2
    ]
}

module main(psnk GND)
{
    RES R_top1
    RES R_top2
    R_top1.1 -> R_top2.1
    R_top1.2 -> GND
    R_top2.2 -> GND
}
"#;

/// Build the flat table and the drawn graph for `source` (single file, no
/// library). The caller must hold [`common::lock`].
fn build(source: &str) -> (McVecGraph, mcc::InstTable) {
    let uri = "/mcc/u168-main.mc".to_string();
    common::reset();
    common::load_string(&uri, source);
    let (tree, table, arena, store) =
        mcc::mcc_build_flat_with_arena(&mcc::McIds::from("main"), &uri, 1000)
            .unwrap_or_else(|e| panic!("flat build failed: {e:?}"));
    let vec_block = mcc::vector::builder::visit::build_mc_vec(&tree, &table, &arena, &store);
    let graph = mcc::vector::graph::fromblock::build_mc_vec_graph(&vec_block, &table);
    (graph, table)
}

/// Replicate the api.rs device pipeline tail: the passes a module's own layer
/// runs after the layouter, in the same order.
fn run_device_tail(graph: &mut McVecGraph) {
    graph.layer_style = LayerStyle::Device;
    mcc::viz::layout::equipotential_tree::layout_device_layer(graph);
    mcc::viz::layout::overlap::separate_overlaps_x(graph);
    mcc::viz::layout::equipotential_tree::fit_content_to_canvas(graph);
    let viewbox = (
        0.0,
        0.0,
        400.0 + mcc::viz::layout::normalize::CANVAS_MARGIN * 2.0,
        300.0 + mcc::viz::layout::normalize::CANVAS_MARGIN * 2.0,
    );
    let _ = mcc::viz::layout::module_frame::layout_module_frame(graph, viewbox);
    mcc::viz::layout::block_frame::layout_block_frames(graph);
}

fn box_by_name<'a>(g: &'a McVecGraph, name: &str) -> &'a mcc::vector::graph::McVecBox {
    g.boxes
        .iter()
        .find(|b| b.name == name)
        .unwrap_or_else(|| panic!("box {name} not drawn"))
}

fn rect_of(b: &mcc::vector::graph::McVecBox) -> (f64, f64, f64, f64) {
    (b.x, b.y, b.w, b.h)
}

/// Whether `a` (box) lies inside frame rect `f` with a small tolerance.
fn inside(a: (f64, f64, f64, f64), f: (f64, f64, f64, f64)) -> bool {
    const EPS: f64 = 0.5;
    a.0 >= f.0 - EPS && a.1 >= f.1 - EPS && a.0 + a.2 <= f.0 + f.2 + EPS && a.1 + a.3 <= f.1 + f.3 + EPS
}

/// -- 1. the table: nesting, order, and spans that mean containment --

#[test]
fn block_frame__partition_table_collected_with_nesting_and_spans() {
    let _guard = common::lock();
    let (graph, table) = build(BOARDED);

    let parts = table
        .block_parts_of(graph.bid as u32)
        .expect("the main module entry carries its partition table");
    assert_eq!(parts.uri, "/mcc/u168-main.mc");
    assert_eq!(
        parts.roots.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        vec!["power", "idle"],
        "roots in source order"
    );
    let power = &parts.roots[0];
    assert_eq!(power.level, "block");
    assert_eq!(
        power.children.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        vec!["trim"],
        "nested partition kept"
    );
    let trim = &power.children[0];
    let off_top = BOARDED.find("RES R_top1").unwrap();
    let off_a = BOARDED.find("RES R_a").unwrap();
    let off_a1 = BOARDED.find("RES R_a1").unwrap();

    // Containment, both directions: statements written inside a block are
    // inside its span (innermost wins), statements outside every block are in
    // none.
    let off_top = BOARDED.find("RES R_top1").unwrap();
    let off_a = BOARDED.find("RES R_a").unwrap();
    let off_a1 = BOARDED.find("RES R_a1").unwrap();
    assert!(
        parts.innermost(off_top).is_none(),
        "a module-level statement belongs to no partition"
    );
    assert!(
        parts.innermost(off_a).is_some_and(|p| p.name == "power"),
        "a power statement is innermost-attributed to power"
    );
    assert!(
        parts.innermost(off_a1).is_some_and(|p| p.name == "trim"),
        "a trim statement wins over its parent partition"
    );
    assert!(
        power.span.start < off_a && off_a < power.span.end,
        "the parent span covers its direct members"
    );
    assert!(
        trim.span.start > power.span.start && trim.span.end < power.span.end,
        "the child span nests inside the parent"
    );
}

/// -- 2. the frames: exactly the owning partitions, boxes inside their own --

#[test]
fn block_frame__frames_enclose_their_member_boxes_only() {
    let _guard = common::lock();
    let (mut graph, _table) = build(BOARDED);
    run_device_tail(&mut graph);

    assert_eq!(
        graph
            .block_frames
            .iter()
            .map(|f| f.title.as_str())
            .collect::<Vec<_>>(),
        // Pre-order: parents before children, so nested frames paint above.
        vec!["power", "trim", "idle"],
        "exactly the partitions that own boxes, parents first"
    );
    assert!(
        graph.block_frames.iter().all(|f| f.ports.is_empty()),
        "a block frame is a region, not a boundary with terminals"
    );

    // Each member box sits inside its own frame. Whether a frame's rect
    // *geometrically* encloses a box of no partition is display, not
    // membership — the grouping law keeps the pass from moving anything.
    let frame_of = |name: &str| {
        graph
            .block_frames
            .iter()
            .find(|f| f.title == name)
            .map(|f| (f.x, f.y, f.w, f.h))
            .unwrap_or_else(|| panic!("frame {name}"))
    };
    let power = frame_of("power");
    let trim = frame_of("trim");
    let idle = frame_of("idle");

    for name in ["R_a", "R_b"] {
        assert!(inside(rect_of(box_by_name(&graph, name)), power), "{name} inside power");
    }
    for name in ["R_a1", "R_a2"] {
        let r = rect_of(box_by_name(&graph, name));
        assert!(inside(r, trim), "{name} inside trim");
        assert!(inside(r, power), "{name} inside the parent power too");
    }
    for name in ["R_z1", "R_z2"] {
        assert!(inside(rect_of(box_by_name(&graph, name)), idle), "{name} inside idle");
    }
}

/// -- 3. the twin: no partitions, no frames --

#[test]
fn block_frame__unpartitioned_board_draws_no_frames() {
    let _guard = common::lock();
    let (graph, table) = build(UNPARTITIONED);

    assert!(
        table.block_parts_of(graph.bid as u32).is_none_or(|p| p.roots.is_empty()),
        "an unpartitioned module carries an empty table"
    );
    let mut graph = graph;
    run_device_tail(&mut graph);
    assert!(
        graph.block_frames.is_empty(),
        "the twin draws no frames — boards without blocks render exactly as before"
    );
}

/// -- 4. the drawing: frames reach the SVG, named by the partition --

#[test]
fn block_frame__svg_carries_the_frames_under_the_content() {
    let _guard = common::lock();
    let (mut graph, _) = build(BOARDED);
    run_device_tail(&mut graph);

    let svg = mcc::viz::render::SvgRenderer::render(&graph, 0.0, 0.0, 800.0, 600.0);
    // Parent before child before sibling: paint order is the reading order.
    let p = svg.find("data-block=\"power\"").expect("power in svg");
    let t = svg.find("data-block=\"trim\"").expect("trim in svg");
    let i = svg.find("data-block=\"idle\"").expect("idle in svg");
    assert!(p < t && t < i, "nested frames paint above their parent's border");
    assert!(
        svg.contains("class=\"block-frame\""),
        "the frames carry their own class"
    );

    let twin = build(UNPARTITIONED).0;
    let twin_svg = mcc::viz::render::SvgRenderer::render(&twin, 0.0, 0.0, 800.0, 600.0);
    assert!(
        !twin_svg.contains("block-frame"),
        "no partitions, no frames in the drawing"
    );
}

/// -- 5. the grouping law: the frame pass derives, it never moves --

#[test]
fn block_frame__frame_pass_moves_no_box() {
    let _guard = common::lock();
    let (mut graph, _) = build(BOARDED);
    graph.layer_style = LayerStyle::Device;
    mcc::viz::layout::equipotential_tree::layout_device_layer(&mut graph);
    mcc::viz::layout::overlap::separate_overlaps_x(&mut graph);
    mcc::viz::layout::equipotential_tree::fit_content_to_canvas(&mut graph);
    let before: Vec<(f64, f64, f64, f64)> =
        graph.boxes.iter().map(|b| (b.x, b.y, b.w, b.h)).collect();

    mcc::viz::layout::block_frame::layout_block_frames(&mut graph);

    let after: Vec<(f64, f64, f64, f64)> =
        graph.boxes.iter().map(|b| (b.x, b.y, b.w, b.h)).collect();
    assert_eq!(before, after, "a block frame occupies no volume: it moves nothing");
    assert!(
        !graph.block_frames.is_empty(),
        "the fixture does draw frames, so the immutability claim is not vacuous"
    );
}

/// -- 6. the switch: the frames are display, off unless asked (U171) --

#[test]
fn block_frame__frames_off_by_default_opt_in_draws() {
    let _guard = common::lock();

    // Default options: no frame pass runs, the SVG carries no frame group.
    let (graph, _) = build(BOARDED);
    let doc = mcc::viz::api::render_with(graph, mcc::viz::api::RenderOpts::default());
    let svg = doc
        .layers
        .iter()
        .map(|(_, l)| l.svg.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !svg.contains("block-frame"),
        "frames default off: the base drawing carries no frame"
    );

    // Opt in: the same board draws them.
    let (graph, _) = build(BOARDED);
    let mut opts = mcc::viz::api::RenderOpts::default();
    opts.show_block_frames = true;
    let doc = mcc::viz::api::render_with(graph, opts);
    let svg = doc
        .layers
        .iter()
        .map(|(_, l)| l.svg.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        svg.contains("data-block=\"power\""),
        "opt in draws the frames (and the boxes stay drawn either way)"
    );
}
