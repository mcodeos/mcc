// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ SP model — B-phase placement (`apply_sp_model`).
//!
//! Sibling of `ladder_place`. Consumes the pure `SpModel` from `sp_model` and is
//! the **geometry last writer** for the boxes it owns: it writes `x/y/w/h`,
//! `entry_points`, and — crucially — **two locks** on every box it places:
//!
//!   * `visual_role = Some(VisualRole::SeriesInline)` → makes `place_series_passives`
//!     / `place_passive_chains` skip it (they key on `visual_role.is_none()`).
//!   * `geom_locked = true` → makes `place_bridge_passives` and the idiom/satellite
//!     movers skip it (they key on `geom_locked`).
//!
//! Setting only one lock is a bug: the other family of passive passes would then
//! re-place these boxes and destroy the golden coordinates. (See `SP_layout_spec`.)
//!
//! Risers/leads are NOT drawn here — same-node pins are placed on a shared column
//! and the router (TrunkTap) grows the vertical rail / horizontal lead itself.

use crate::vector::graph::boxdef::{PinLayout, VisualRole};
use crate::vector::graph::{EntryPoint, EntrySide, McVecBox, McVecGraph, Point, Route, Segment};

use super::sp_model::{SpKind, SpModel, SpTree};

// Grid → pixel. Kept close to the flow layout scale; tune to match the rest.
const COL_W: f64 = 120.0;
const ROW_H: f64 = 80.0;
const MARGIN: f64 = 60.0;
const BODY_W: f64 = COL_W * 0.55; // horizontal passive body
const BODY_H: f64 = ROW_H * 0.40;
const TERM_GAP: f64 = COL_W * 0.4; // clearance between a terminal box and its rail

/// One placed leaf in grid units. `x_slot` = the component's **left** node column;
/// the component spans `[x_slot, x_slot+1]`; `y_row` = its horizontal symbol row.
#[derive(Debug, Clone, PartialEq)]
pub struct GridPlacement {
    pub box_id: i64,
    pub x_slot: f64,
    pub y_row: f64,
}

// ============================================================================
// Public entry
// ============================================================================

pub fn apply_sp_model(graph: &mut McVecGraph, m: &SpModel) {
    let grid = place_grid(&m.root);
    let (root_w, root_h) = m.root.size();

    // ── place the passive edges ─────────────────────────────────────────────
    for gp in &grid {
        // node identity: the leaf spans (a, b); a is its left node, b its right.
        let (na, nb) = span_of(&m.root, gp.box_id).expect("leaf must have a span");
        write_passive(graph, gp, na, nb);
    }

    // ── place the two terminal anchors, each oriented to FACE the block ──────
    // This is the fix for the "u1.6→u2.6 not shown / messy" render: the terminal's
    // connecting pin must sit on the edge facing the block, or the router snakes the
    // wire around the IC and lands it on the wrong (near-side) pins.
    //   left terminal  → its pin on the RIGHT edge, box left of the left rail (col 0)
    //   right terminal → its pin on the LEFT  edge, box right of the right rail (col root_w)
    //
    // ★ Both terminals get the SAME size, driven by the larger pin count, so u2 is not
    // drawn tiny next to u1. SP owns the boundary ICs' geometry here, so it sizes them
    // consistently instead of inheriting the generic satellite size.
    let (term_w, term_h) = terminal_size(graph, m.left_box, m.right_box);
    let mid_y = MARGIN + (root_h - 1.0) / 2.0 * ROW_H;
    place_terminal(graph, m.left_box, m.left_node, EntrySide::Right, 0.0, mid_y, term_w, term_h);
    place_terminal(graph, m.right_box, m.right_node, EntrySide::Left, root_w, mid_y, term_w, term_h);

    // ── ★ wiring: emit rails + taps + leads directly into net.route ─────────
    // Every same-node pin already shares a grid column, so the rail is a vertical
    // line at that column and each pin taps in horizontally (short branches tap
    // across as leads). Emitting here — instead of leaving it to the generic
    // router — is what removes the over-the-top / snaking wires. The router is
    // told to skip any net that already carries a route (see the flow patch).
    emit_sp_routes(graph, m, &grid, root_w);
}

/// A common size for both terminal ICs so u1 and u2 render at the same scale.
/// = max(pin-count size, each terminal's current w/h). Never shrinks either.
fn terminal_size(graph: &McVecGraph, a: i64, b: i64) -> (f64, f64) {
    const PIN_PITCH: f64 = 28.0;
    const PAD: f64 = 26.0;
    const MIN_W: f64 = COL_W * 1.1;
    let get = |id: i64| graph.boxes.iter().find(|x| x.id == id);
    let pins = |id: i64| -> usize {
        get(id).map(|x| x.pins.len().max(x.entry_points.len()).max(x.pin_count)).unwrap_or(0)
    };
    let cur_w = |id: i64| get(id).map(|x| x.w).unwrap_or(0.0);
    let cur_h = |id: i64| get(id).map(|x| x.h).unwrap_or(0.0);
    let n = pins(a).max(pins(b));
    let h = ((n as f64) * PIN_PITCH + PAD).max(cur_h(a)).max(cur_h(b));
    let w = MIN_W.max(cur_w(a)).max(cur_w(b));
    (w, h)
}

// ============================================================================
// Pure grid placement (Phase 3 core; the golden test asserts on this)
// ============================================================================

/// Recursively assign `(x_slot, y_row)` to every leaf. Pure, deterministic.
pub fn place_grid(root: &SpTree) -> Vec<GridPlacement> {
    let mut out = Vec::new();
    place(root, 0.0, 0.0, &mut out);
    out.sort_by(|a, b| a.box_id.cmp(&b.box_id));
    out
}

fn place(t: &SpTree, x0: f64, y0: f64, out: &mut Vec<GridPlacement>) {
    let (_w, h) = t.size();
    match &t.kind {
        SpKind::Leaf { box_id, .. } => {
            out.push(GridPlacement { box_id: *box_id, x_slot: x0, y_row: y0 });
        }
        SpKind::Series(cs) => {
            // children left→right; each vertically centered in the parent's h band
            let mut cx = x0;
            for c in cs {
                let (cw, ch) = c.size();
                let cy = y0 + (h - ch) / 2.0;
                place(c, cx, cy, out);
                cx += cw;
            }
        }
        SpKind::Parallel(cs) => {
            // children top→bottom, left-aligned (short branches get a router lead)
            let mut cy = y0;
            for c in cs {
                let (_cw, ch) = c.size();
                place(c, x0, cy, out);
                cy += ch;
            }
        }
    }
}

/// Find a leaf's span `(a, b)` by box id.
fn span_of(t: &SpTree, box_id: i64) -> Option<(usize, usize)> {
    match &t.kind {
        SpKind::Leaf { box_id: id, .. } if *id == box_id => Some((t.a, t.b)),
        SpKind::Leaf { .. } => None,
        SpKind::Series(cs) | SpKind::Parallel(cs) => {
            cs.iter().find_map(|c| span_of(c, box_id))
        }
    }
}

// ============================================================================
// Writers
// ============================================================================

fn write_passive(graph: &mut McVecGraph, gp: &GridPlacement, na: usize, nb: usize) {
    // pin ids on each node BEFORE the mutable borrow
    let left_pin = pin_on_net(graph, gp.box_id, na);
    let right_pin = pin_on_net(graph, gp.box_id, nb);

    let center_col = gp.x_slot + 0.5;
    let cx = MARGIN + center_col * COL_W;
    let cy = MARGIN + gp.y_row * ROW_H;

    let Some(b) = graph.boxes.iter_mut().find(|b| b.id == gp.box_id) else {
        return;
    };
    b.w = BODY_W;
    b.h = BODY_H;
    b.x = cx - BODY_W / 2.0;
    b.y = cy - BODY_H / 2.0;

    // entry points: left node pin on Left edge, right node pin on Right edge
    for ep in &mut b.entry_points {
        if Some(ep.pin_id) == left_pin {
            ep.side = EntrySide::Left;
            ep.offset = 0.5;
        } else if Some(ep.pin_id) == right_pin {
            ep.side = EntrySide::Right;
            ep.offset = 0.5;
        }
    }
    // if the model synthesized no entry points yet, seed the two we know
    if b.entry_points.is_empty() {
        if let Some(p) = left_pin {
            b.entry_points.push(EntryPoint {
                pin_id: p,
                pin_name: p.to_string(),
                side: EntrySide::Left,
                offset: 0.5,
            });
        }
        if let Some(p) = right_pin {
            b.entry_points.push(EntryPoint {
                pin_id: p,
                pin_name: p.to_string(),
                side: EntrySide::Right,
                offset: 0.5,
            });
        }
    }

    // ★ two locks
    b.visual_role = Some(VisualRole::SeriesInline);
    b.geom_locked = true;
}

/// Place a terminal IC flush against `rail_col`, with its connecting pin oriented
/// on the `facing` edge so the block terminates cleanly on it (no snaking).
///
/// * `term_node`      — the net the terminal connects through (left_node / right_node).
/// * `facing`         — `Right` for the left terminal, `Left` for the right terminal.
/// * `rail_col`       — grid column of the rail the terminal binds to (0 / root_w).
/// * `term_w/term_h`  — the shared terminal size (see `terminal_size`) so u1 and u2
///   render at the same scale; applied grow-only.
///
/// The terminal's OTHER pins keep their sides/offsets (unconnected stubs); only the
/// one connecting pin is moved to face the block. Enlarging the box also spreads the
/// stub pins apart (their fractional offsets stretch), fixing the crowded-pin look.
fn place_terminal(
    graph: &mut McVecGraph,
    box_id: i64,
    term_node: usize,
    facing: EntrySide,
    rail_col: f64,
    cy: f64,
    term_w: f64,
    term_h: f64,
) {
    let pin = pin_on_net(graph, box_id, term_node);
    let Some(b) = graph.boxes.iter_mut().find(|b| b.id == box_id) else {
        return;
    };
    // ★ apply the shared size so u1 and u2 match (terminal_size already took the
    // max of both current sizes, so this never shrinks a legitimately larger IC)
    b.w = term_w;
    b.h = term_h;
    let rail_x = MARGIN + rail_col * COL_W;
    match facing {
        // left terminal: right edge sits just left of the rail
        EntrySide::Right => b.x = rail_x - TERM_GAP - b.w,
        // right terminal: left edge sits just right of the rail
        EntrySide::Left => b.x = rail_x + TERM_GAP,
        _ => {}
    }
    b.y = cy - b.h / 2.0;

    // ── pin distribution ────────────────────────────────────────────────────
    // Connecting pin faces the block (`facing`); EVERY other pin goes to the far
    // edge (`opp`) and is spread evenly so nothing overlaps. Without this, the
    // coarse pass leaves all of u2's pins clustered on the block side.
    let opp = opposite(&facing);

    // make sure the connecting pin actually has an entry point
    if let Some(pin_id) = pin {
        if !b.entry_points.iter().any(|e| e.pin_id == pin_id) {
            b.entry_points.push(EntryPoint {
                pin_id,
                pin_name: pin_id.to_string(),
                side: facing.clone(),
                offset: 0.5,
            });
        }
    }

    // 1. assign sides
    let mut far: Vec<usize> = Vec::new();
    for (i, ep) in b.entry_points.iter_mut().enumerate() {
        if Some(ep.pin_id) == pin {
            ep.side = facing.clone();
            ep.offset = 0.5;
        } else {
            ep.side = opp.clone();
            far.push(i);
        }
    }
    // 2. spread the far-edge pins evenly along that edge
    let n = far.len();
    for (k, &i) in far.iter().enumerate() {
        b.entry_points[i].offset = ((k as f64) + 1.0) / ((n as f64) + 1.0);
    }

    // 3. freeze the arrangement via layout_hint so nothing downstream re-classifies it
    let mut facing_keys: Vec<String> = Vec::new();
    let mut far_keys: Vec<String> = Vec::new();
    for ep in &b.entry_points {
        let dst = if Some(ep.pin_id) == pin { &mut facing_keys } else { &mut far_keys };
        dst.push(ep.pin_name.clone());
        dst.push(ep.pin_id.to_string());
    }
    let mut hint = PinLayout::default();
    match facing {
        EntrySide::Right => {
            hint.right = facing_keys;
            hint.left = far_keys;
        }
        EntrySide::Left => {
            hint.left = facing_keys;
            hint.right = far_keys;
        }
        _ => {}
    }
    b.set_layout_hint(hint);

    b.geom_locked = true;
}

/// The opposite edge (Left↔Right, Top↔Bottom).
fn opposite(s: &EntrySide) -> EntrySide {
    match s {
        EntrySide::Left => EntrySide::Right,
        EntrySide::Right => EntrySide::Left,
        EntrySide::Top => EntrySide::Bottom,
        EntrySide::Bottom => EntrySide::Top,
    }
}

/// The pin_id of `box_id` that sits on net index `ni` (if any).
fn pin_on_net(graph: &McVecGraph, box_id: i64, ni: usize) -> Option<i64> {
    graph
        .nets
        .get(ni)?
        .endpoints
        .iter()
        .find(|e| e.box_id == box_id)
        .map(|e| e.pin_id)
}

// ============================================================================
// Wiring — emit rails / taps / leads directly into net.route
// ============================================================================

const EPS: f64 = 1e-6;

/// Pixel position of a pin on a box edge, from its entry point.
fn pin_pixel(b: &McVecBox, pin_id: i64) -> Option<Point> {
    let ep = b.entry_points.iter().find(|e| e.pin_id == pin_id)?;
    let p = match ep.side {
        EntrySide::Left => Point::new(b.x, b.y + ep.offset * b.h),
        EntrySide::Right => Point::new(b.x + b.w, b.y + ep.offset * b.h),
        EntrySide::Top => Point::new(b.x + ep.offset * b.w, b.y),
        EntrySide::Bottom => Point::new(b.x + ep.offset * b.w, b.y + b.h),
    };
    Some(p)
}

/// One tap contribution to a node's rail: the pin's pixel position + its grid column.
struct Tap {
    px: f64,
    py: f64,
    col: f64,
}

/// For every net the SP model fully owns, build `route = vertical rail + horizontal
/// taps (+ leads) + junctions`, deterministically, from the placed geometry.
fn emit_sp_routes(graph: &mut McVecGraph, m: &SpModel, grid: &[GridPlacement], root_w: f64) {
    // ── read pass: gather taps per net + compute each route (immutable borrow) ──
    let computed: Vec<(usize, Route)> = {
        let mut per_net: std::collections::HashMap<usize, Vec<Tap>> =
            std::collections::HashMap::new();
        let mut owned: std::collections::HashSet<i64> = std::collections::HashSet::new();
        owned.insert(m.left_box);
        owned.insert(m.right_box);

        let bx = |id: i64| graph.boxes.iter().find(|b| b.id == id);

        // passives: left pin on node a (col = x_slot), right pin on node b (col = x_slot+1)
        for gp in grid {
            owned.insert(gp.box_id);
            let Some((a, b)) = span_of(&m.root, gp.box_id) else { continue };
            let Some(bo) = bx(gp.box_id) else { continue };
            if let Some(pid) = pin_on_net(graph, gp.box_id, a) {
                if let Some(p) = pin_pixel(bo, pid) {
                    per_net.entry(a).or_default().push(Tap { px: p.x, py: p.y, col: gp.x_slot });
                }
            }
            if let Some(pid) = pin_on_net(graph, gp.box_id, b) {
                if let Some(p) = pin_pixel(bo, pid) {
                    per_net.entry(b).or_default().push(Tap { px: p.x, py: p.y, col: gp.x_slot + 1.0 });
                }
            }
        }

        // terminals: connecting pin on its node (col = 0 / root_w)
        for &(bid, node, col) in &[(m.left_box, m.left_node, 0.0), (m.right_box, m.right_node, root_w)] {
            if let (Some(bo), Some(pid)) = (bx(bid), pin_on_net(graph, bid, node)) {
                if let Some(p) = pin_pixel(bo, pid) {
                    per_net.entry(node).or_default().push(Tap { px: p.x, py: p.y, col });
                }
            }
        }

        let mut out = Vec::new();
        for (ni, taps) in per_net {
            let Some(net) = graph.nets.get(ni) else { continue };
            // only own a net if every endpoint box is SP-placed or a terminal
            if !net.endpoints.iter().all(|e| owned.contains(&e.box_id)) || taps.len() < 2 {
                continue;
            }
            out.push((ni, build_rail_route(&taps)));
        }
        out
    };

    // ── write pass: commit routes (mutable borrow) ──
    for (ni, route) in computed {
        if let Some(net) = graph.nets.get_mut(ni) {
            net.route = Some(route);
        }
    }
}

/// Build one net's route: vertical rail at the max-column, horizontal taps from
/// every pin (short branches tap across as leads), junction dots where 3+ meet.
fn build_rail_route(taps: &[Tap]) -> Route {
    let rail_col = taps.iter().map(|t| t.col).fold(f64::MIN, f64::max);
    let rail_x = MARGIN + rail_col * COL_W;
    let y_top = taps.iter().map(|t| t.py).fold(f64::MAX, f64::min);
    let y_bot = taps.iter().map(|t| t.py).fold(f64::MIN, f64::max);

    let mut route = Route::new();
    if (y_bot - y_top).abs() > EPS {
        route.segments.push(Segment {
            from: Point::new(rail_x, y_top),
            to: Point::new(rail_x, y_bot),
        });
    }
    for t in taps {
        if (t.px - rail_x).abs() > EPS {
            route.segments.push(Segment {
                from: Point::new(t.px, t.py),
                to: Point::new(rail_x, t.py),
            });
        }
    }
    if taps.len() >= 3 {
        for t in taps {
            route.junctions.push(Point::new(rail_x, t.py));
        }
    }
    route
}

// ============================================================================
// Golden regression (Phase 5)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::boxdef::IoSummary;
    use crate::vector::graph::netdef::{EndpointRef, VizNet};
    use crate::vector::graph::{BoxKind, McVecBox, NetKind, Symbol};
    use crate::viz::layout::sp_model::{build_sp_model, SpBail};

    // ---- builders (mirror ladder_model tests) -----------------------------
    fn term(id: i64, name: &str, outputs: usize) -> McVecBox {
        let mut io = IoSummary::new();
        io.outputs = outputs;
        McVecBox::new_v2(id, name.into(), "".into(), BoxKind::TwoPin, Symbol::Ic, None, None, 1, io)
    }
    fn res(id: i64, name: &str) -> McVecBox {
        McVecBox::new_v2(
            id, name.into(), "RES".into(), BoxKind::TwoPin, Symbol::Resistor,
            Some(name.into()), None, 2, IoSummary::new(),
        )
    }
    fn cap(id: i64, name: &str) -> McVecBox {
        McVecBox::new_v2(
            id, name.into(), "CAP".into(), BoxKind::TwoPin, Symbol::Capacitor,
            Some(name.into()), None, 2, IoSummary::new(),
        )
    }
    fn net(nid: i64, name: &str, eps: &[(i64, i64)]) -> VizNet {
        VizNet::new(
            nid, name.into(), NetKind::Signal,
            eps.iter().map(|&(b, p)| EndpointRef::new(b, p, &p.to_string())).collect(),
        )
    }

    /// The SP golden case (post-coalesce): 5 nodes, 6 edges.
    /// nodes: N1=0 N2=1 N3=2 N4=3 N5=4 ; terminals u1→N1, u2→N3.
    fn golden() -> McVecGraph {
        let mut g = McVecGraph::new(1, "main".into());
        // ids chosen so min-id ordering is R1<C2<R3<R4<C5<R6
        g.boxes.push(res(1, "R1"));
        g.boxes.push(cap(2, "C2"));
        g.boxes.push(res(3, "R3"));
        g.boxes.push(res(4, "R4"));
        g.boxes.push(cap(5, "C5"));
        g.boxes.push(res(6, "R6"));
        g.boxes.push(term(101, "u1", 1)); // source → left
        g.boxes.push(term(102, "u2", 0)); // sink → right

        // pin ids are arbitrary but distinct per box; (box, pin)
        g.nets.push(net(0, "N1", &[(101, 6), (1, 11), (3, 31)])); // u1.6, R1.1, R3.1
        g.nets.push(net(1, "N2", &[(1, 12), (2, 21)])); //           R1.2, C2.1
        g.nets.push(net(2, "N3", &[(2, 22), (102, 6), (5, 52), (6, 62)])); // C2.2,u2.6,C5.2,R6.2
        g.nets.push(net(3, "N4", &[(3, 32), (4, 41), (6, 61)])); //  R3.2, R4.1, R6.1
        g.nets.push(net(4, "N5", &[(4, 42), (5, 51)])); //           R4.2, C5.1
        g
    }

    #[test]
    fn golden_expression() {
        let m = build_sp_model(&golden()).expect("should be SP");
        assert_eq!(m.root.expr(), "(R1 + C2) ∥ (R3 + ((R4 + C5) ∥ R6))");
        assert_eq!(m.root.size(), (3.0, 3.0));
        assert_eq!(m.left_box, 101);
        assert_eq!(m.right_box, 102);
    }

    #[test]
    fn golden_coordinates() {
        let m = build_sp_model(&golden()).unwrap();
        let grid = place_grid(&m.root);
        let at = |id: i64| grid.iter().find(|g| g.box_id == id).unwrap();
        // (box_id, x_slot, y_row) — the golden table
        assert_eq!((at(1).x_slot, at(1).y_row), (0.0, 0.0)); // R1
        assert_eq!((at(2).x_slot, at(2).y_row), (1.0, 0.0)); // C2
        assert_eq!((at(3).x_slot, at(3).y_row), (0.0, 1.5)); // R3  ← the tricky one
        assert_eq!((at(4).x_slot, at(4).y_row), (1.0, 1.0)); // R4
        assert_eq!((at(5).x_slot, at(5).y_row), (2.0, 1.0)); // C5
        assert_eq!((at(6).x_slot, at(6).y_row), (1.0, 2.0)); // R6
    }

    #[test]
    fn golden_two_locks_and_no_riser() {
        let mut g = golden();
        let m = build_sp_model(&g).unwrap();
        apply_sp_model(&mut g, &m);
        // every placed passive carries BOTH locks
        for id in [1, 2, 3, 4, 5, 6] {
            let b = g.boxes.iter().find(|b| b.id == id).unwrap();
            assert!(b.geom_locked, "#{id} must be geom_locked");
            assert_eq!(
                b.visual_role,
                Some(VisualRole::SeriesInline),
                "#{id} must be SeriesInline"
            );
        }
        // N2 (net 1) and N5 (net 4): their two pins land on one row → no riser
        for ni in [1usize, 4usize] {
            let rows: Vec<f64> = g.nets[ni]
                .endpoints
                .iter()
                .filter_map(|e| g.boxes.iter().find(|b| b.id == e.box_id))
                .map(|b| b.y + b.h / 2.0)
                .collect();
            let span = rows.iter().cloned().fold(f64::MIN, f64::max)
                - rows.iter().cloned().fold(f64::MAX, f64::min);
            assert!(span.abs() < 1.0, "net {ni} should need no riser (single row)");
        }
    }

    #[test]
    fn wheatstone_bridge_bails_non_sp() {
        // L=u1(net0), R=u2(net3); internal X=net1, Y=net2; bridge R5 across X-Y
        let mut g = McVecGraph::new(1, "main".into());
        g.boxes.push(res(1, "R1"));
        g.boxes.push(res(2, "R2"));
        g.boxes.push(res(3, "R3"));
        g.boxes.push(res(4, "R4"));
        g.boxes.push(res(5, "R5"));
        g.boxes.push(term(101, "u1", 1));
        g.boxes.push(term(102, "u2", 0));
        g.nets.push(net(0, "L", &[(101, 6), (1, 11), (2, 21)]));
        g.nets.push(net(1, "X", &[(1, 12), (3, 31), (5, 51)]));
        g.nets.push(net(2, "Y", &[(2, 22), (4, 41), (5, 52)]));
        g.nets.push(net(3, "R", &[(3, 32), (4, 42), (102, 6)]));
        match build_sp_model(&g) {
            Err(SpBail::NonSpBridge { .. }) => {}
            other => panic!("expected NonSpBridge, got {other:?}"),
        }
    }

    #[test]
    fn right_terminal_connecting_pin_faces_block_others_go_far() {
        let mut g = golden();
        // simulate the post-coarse state: u2 (id 102) has its wired IND pin (6)
        // PLUS several unconnected physical pins, all clustered on the block side.
        let u2 = g.boxes.iter_mut().find(|b| b.id == 102).unwrap();
        u2.entry_points = vec![
            EntryPoint { pin_id: 6, pin_name: "IND".into(), side: EntrySide::Left, offset: 0.5 },
            EntryPoint { pin_id: 11, pin_name: "OUTD".into(), side: EntrySide::Left, offset: 0.5 },
            EntryPoint { pin_id: 12, pin_name: "OUTC".into(), side: EntrySide::Left, offset: 0.5 },
            EntryPoint { pin_id: 13, pin_name: "OUTB".into(), side: EntrySide::Left, offset: 0.5 },
        ];

        let m = build_sp_model(&g).unwrap();
        apply_sp_model(&mut g, &m);

        let u2 = g.boxes.iter().find(|b| b.id == 102).unwrap();
        // connecting pin (6) faces the block (Left); every other pin goes to the far edge (Right)
        for ep in &u2.entry_points {
            if ep.pin_id == 6 {
                assert_eq!(ep.side, EntrySide::Left, "connecting pin must face the block");
            } else {
                assert_eq!(ep.side, EntrySide::Right, "pin {} must go to the far edge", ep.pin_id);
            }
        }
        // far-edge pins are spread (no two share an offset)
        let mut offs: Vec<f64> = u2
            .entry_points
            .iter()
            .filter(|e| e.side == EntrySide::Right)
            .map(|e| e.offset)
            .collect();
        offs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for w in offs.windows(2) {
            assert!((w[1] - w[0]).abs() > 1e-6, "far-edge offsets must be distinct");
        }
        // and the left terminal (u1) is symmetric: connecting pin on the Right
        let u1 = g.boxes.iter().find(|b| b.id == 101).unwrap();
        let conn = u1.entry_points.iter().find(|e| e.pin_id == 6).unwrap();
        assert_eq!(conn.side, EntrySide::Right, "u1 connecting pin must face the block");
    }

    #[test]
    fn golden_rails_and_leads_emitted() {
        let mut g = golden();
        let m = build_sp_model(&g).unwrap();
        apply_sp_model(&mut g, &m);

        // a net has a vertical rail iff it carries a segment with equal x, differing y
        let has_rail = |ni: usize| {
            g.nets[ni].route.as_ref().map_or(false, |r| {
                r.segments.iter().any(|s| {
                    (s.from.x - s.to.x).abs() < 1e-6 && (s.from.y - s.to.y).abs() > 1e-6
                })
            })
        };
        // horizontal leads (equal y, differing x)
        let n_leads = |ni: usize| {
            g.nets[ni].route.as_ref().map_or(0, |r| {
                r.segments
                    .iter()
                    .filter(|s| (s.from.y - s.to.y).abs() < 1e-6 && (s.from.x - s.to.x).abs() > 1e-6)
                    .count()
            })
        };

        // every SP net is routed by SP
        for ni in 0..5 {
            assert!(g.nets[ni].route.is_some(), "net {ni} should be SP-routed");
        }
        // N1(0), N3(2), N4(3) are risers; N2(1), N5(4) are single-row (no rail)
        assert!(has_rail(0), "N1 rail");
        assert!(has_rail(2), "N3 rail");
        assert!(has_rail(3), "N4 rail");
        assert!(!has_rail(1), "N2 no rail");
        assert!(!has_rail(4), "N5 no rail");
        // N3 has the two leads (C2 row0, R6 row2) plus C5's tap → several horizontal segments
        assert!(n_leads(2) >= 2, "N3 should carry the C2 + R6 leads");
    }
}