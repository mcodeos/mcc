// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The boundary frame of a module drawn as its own layer.
//!
//! A module's ports are **drawn objects**. In a parent's block diagram they are
//! the sub-module box's leads, named by the port each wire crosses
//! (`sub_module.rs` + `McVecBox::boundary_ports`). In the module's own layer they
//! are the terminals on this dashed frame. Same module, same boundary, same name
//! — the picture of a module never depends on whether it was opened on its own or
//! expanded inside its project.
//!
//! Everything here is structural. The rect is the content's bbox; the ports are
//! the nets carrying a `BoundaryInfo` marker, and each anchor is that net's own
//! terminal symbol placed on the frame edge the port's declaration names — an
//! `in` / `psnk` port on the left, an `out` / `psrc` port on the right, and only
//! a port that declares no side on the edge its crossing points at. No name is
//! consulted, and the renderer recomputes nothing — it draws the rect and the
//! labels as written.

use crate::vector::graph::{
    BoxKind, EntrySide, FrameLeadSeg, FramePort, McVecGraph, ModuleFrame, NetKind,
};
use crate::vector::model::PortFlow;

use super::equipotential_tree::{build_all_trees, content_bbox, EquiTree, GUTTER_STEP};

/// Gutter between the content and the frame.
const FRAME_PAD: f64 = 18.0;
/// Room outside the frame for the title, and the floor for the label ring even
/// when every port name is short.
const FRAME_LABEL_PAD: f64 = 34.0;
/// Minimum spacing between two port anchors sharing one frame edge, so their
/// labels cannot overlap on the frame.
const PORT_MIN_SPACING: f64 = 18.0;
/// Keep an anchor this far off the frame's corners.
const PORT_CORNER_KEEP: f64 = 14.0;
/// Port-label font size, and a conservative advance per character at that size.
///
/// The same kind of rough estimate `equipotential_tree::content_bbox` makes for
/// its own labels: this pass only needs the canvas to be big enough, it is not
/// a text-metrics engine.
const PORT_FONT_SIZE: f64 = 11.0;
const PORT_GLYPH_W: f64 = 7.6;
/// Distance from the frame edge to the far end of a port name. The label is
/// anchored 6px off the edge, so this leaves a few px of slack the glyph
/// estimate can be wrong by without the canvas clipping the name.
const PORT_LABEL_GAP: f64 = 10.0;

/// Estimated width of a port label.
fn label_width(name: &str) -> f64 {
    name.chars().count() as f64 * PORT_GLYPH_W
}

/// Draw the boundary frame of a module's own layer, if this layer is one.
///
/// Returns the viewBox, grown when a frame was written so the port names have
/// room outside the rect. Called after `fit_content_to_canvas` (the pass needs
/// final geometry and the final tree symbols) and before rendering.
///
/// A layer is framed when it is a **module's own schematic**: it renders through
/// the device pipeline and its module declares ports. Anything else — a block
/// diagram, a layer with no declared port — is left untouched: `module_frame`
/// stays `None` and the viewBox is returned unchanged.
pub fn layout_module_frame(
    graph: &mut McVecGraph,
    viewbox: (f64, f64, f64, f64),
) -> (f64, f64, f64, f64) {
    graph.module_frame = None;
    if graph.module_ports.is_empty() {
        return viewbox;
    }

    let trees = build_all_trees(graph);
    let Some((min_x, min_y, max_x, max_y)) = content_bbox(graph, &trees) else {
        return viewbox;
    };

    let rect = (
        min_x - FRAME_PAD,
        min_y - FRAME_PAD,
        (max_x - min_x) + 2.0 * FRAME_PAD,
        (max_y - min_y) + 2.0 * FRAME_PAD,
    );
    let ports = frame_ports(graph, &trees, rect);
    let frame = ModuleFrame {
        x: rect.0,
        y: rect.1,
        w: rect.2,
        h: rect.3,
        title: graph.name.clone(),
        ports,
    };

    crate::vlog!(
        "[module_frame] layer '{}' frame {}x{} at ({},{}) with {} port(s): {:?}",
        graph.name,
        frame.w as i32,
        frame.h as i32,
        frame.x as i32,
        frame.y as i32,
        frame.ports.len(),
        frame
            .ports
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
    );

    // Grow the viewBox to leave the label ring free; never shrink it. The pad per
    // side is driven by the names actually on that side — a fixed pad clips
    // `I2C0/SDA` while leaving `vin` swimming in space.
    let (mut left, mut right, mut top, mut bottom) = (
        FRAME_LABEL_PAD,
        FRAME_LABEL_PAD,
        FRAME_LABEL_PAD,
        FRAME_LABEL_PAD,
    );
    for p in &frame.ports {
        let w = label_width(&p.name);
        match p.side {
            // Anchored at the edge, running outward: the whole name needs room.
            EntrySide::Left => left = left.max(w + PORT_LABEL_GAP),
            EntrySide::Right => right = right.max(w + PORT_LABEL_GAP),
            // Centred on the anchor: the pad is the vertical room, but a label
            // near a corner also sticks out sideways past the frame.
            EntrySide::Top | EntrySide::Bottom => {
                let pad = PORT_LABEL_GAP + PORT_FONT_SIZE;
                top = top.max(pad);
                bottom = bottom.max(pad);
                left = left.max(frame.x - (p.x - w / 2.0));
                right = right.max((p.x + w / 2.0) - (frame.x + frame.w));
            }
        }
    }
    let vx = viewbox.0.min(frame.x - left);
    let vy = viewbox.1.min(frame.y - top);
    let vx2 = (viewbox.0 + viewbox.2).max(frame.x + frame.w + right);
    let vy2 = (viewbox.1 + viewbox.3).max(frame.y + frame.h + bottom);
    let out = (vx, vy, vx2 - vx, vy2 - vy);
    graph.module_frame = Some(frame);
    out
}

/// One anchor per port crossing this layer's boundary.
///
/// The crossing is read from the nets: a net whose `boundary` marker names the
/// port, and whose own terminal symbol gives the point the lead leaves through.
/// The marker is what makes it a port — never the net's or the port's name.
///
/// The edge is the declaration's, not the geometry's: a port that declares a
/// side — `in` / `psnk` left, `out` / `psrc` right — is drawn there, so the net
/// it carries stays inside the frame and the reader meets the terminal where the
/// module's own contract says it is. Only a port that declares no side falls
/// back to the dominant axis of its crossing.
fn frame_ports(
    graph: &McVecGraph,
    trees: &[crate::viz::layout::equipotential_tree::EquiTree],
    rect: (f64, f64, f64, f64),
) -> Vec<FramePort> {
    let (rx, ry, rw, rh) = rect;
    let (min_x, min_y, max_x, max_y) = (rx, ry, rx + rw, ry + rh);
    let cx = rx + rw / 2.0;
    let cy = ry + rh / 2.0;

    // One anchor per port group: several nets cross the same port (`vin.V5V`,
    // `vin.GND`), and the frame shows the port once, where it actually leaves.
    // The group's nets are not interchangeable: the one whose marker carries the
    // declared side is the port's own face, so it wins the anchor its lead.
    //
    // ★ U160-5: every net's crossing is KEPT, not just the flow-winning one —
    // each crossing gets its own lead wire out of the shared anchor (a `psnk
    // dc{VDD_3V3, GND}` head port or a `MIC{P,N}` bus port fans out into one
    // wire per net).
    let mut seen: Vec<(
        i64,
        Vec<(i64, f64, f64)>,
        Option<(f64, f64)>,
        String,
        bool,
        Option<PortFlow>,
    )> = Vec::new();
    // A ground net's crossing is the rail's own ground glyph — the glyph is the
    // label, so the anchor keeps the port dot but no lead is routed back to it.
    let ground_nets: Vec<i64> = graph
        .nets
        .iter()
        .filter(|n| n.kind == NetKind::Ground)
        .map(|n| n.nid)
        .collect();
    for net in &graph.nets {
        let Some(bi) = net.boundary.as_ref() else {
            continue;
        };
        // The lead's own terminal: the outermost symbol of this net, i.e. the one
        // farthest from the content centre. That symbol IS the boundary crossing.
        let Some(p) = trees
            .iter()
            .flat_map(|t| t.symbols.iter())
            .filter(|s| s.net_id == net.nid)
            .map(|s| (s.x, s.y))
            .max_by(|a, b| {
                let da = (a.0 - cx).abs() + (a.1 - cy).abs();
                let db = (b.0 - cx).abs() + (b.1 - cy).abs();
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
        else {
            continue;
        };
        if let Some(slot) = seen.iter_mut().find(|(id, ..)| *id == bi.port_group_id) {
            slot.1.push((net.nid, p.0, p.1));
            if slot.5.is_none() && bi.flow.is_some() {
                // The flow-declaring net is the port's own face: its crossing
                // drives where the anchor projects onto the declared edge.
                slot.2 = Some(p);
                slot.5 = bi.flow;
            }
            continue;
        }
        let face = if bi.flow.is_some() { Some(p) } else { None };
        seen.push((
            bi.port_group_id,
            vec![(net.nid, p.0, p.1)],
            face,
            bi.port_name.clone(),
            bi.is_supply,
            bi.flow,
        ));
    }

    // Project each crossing onto the frame edge it leaves through, then spread
    // the anchors on each edge so two labels cannot land on top of each other.
    let mut out: Vec<FramePort> = Vec::with_capacity(seen.len());
    let mut crossings: Vec<Vec<(i64, f64, f64)>> = Vec::with_capacity(seen.len());
    for (_, crosses, face, name, is_supply, flow) in seen {
        crossings.push(crosses);
        let (px, py) = face.unwrap_or_else(|| {
            let c = &crossings[crossings.len() - 1];
            (c[0].1, c[0].2)
        });
        let (side, x, y) = match flow {
            Some(PortFlow::In) => (EntrySide::Left, min_x, py),
            Some(PortFlow::Out) => (EntrySide::Right, max_x, py),
            None => {
                let dx = (px - cx) / ((max_x - min_x) / 2.0).max(1.0);
                let dy = (py - cy) / ((max_y - min_y) / 2.0).max(1.0);
                if dx.abs() >= dy.abs() {
                    let x = if dx < 0.0 { min_x } else { max_x };
                    (
                        if dx < 0.0 {
                            EntrySide::Left
                        } else {
                            EntrySide::Right
                        },
                        x,
                        py,
                    )
                } else {
                    let y = if dy < 0.0 { min_y } else { max_y };
                    (
                        if dy < 0.0 {
                            EntrySide::Top
                        } else {
                            EntrySide::Bottom
                        },
                        px,
                        y,
                    )
                }
            }
        };
        out.push(FramePort {
            name,
            x,
            y,
            side,
            is_supply,
            leads: Vec::new(),
        });
    }
    spread_along_edges(&mut out, min_x, min_y, max_x, max_y);
    // ★ U160-5: the anchors are final — route one lead per crossing net, out of
    // the shared anchor, to that net's own boundary-crossing symbol.
    //
    // ★ U160-2: several nets of one port can route leads that SHARE pieces (the
    // hbl `SPI` port: all four nets' leads run the same first piece out of the
    // shared anchor, then branch to their own crossings). Rendering every lead
    // whole draws those shared pieces twice over. Dedup at the SEGMENT level
    // within the port — a piece already drawn for this port is dropped from the
    // later leads that would repeat it, so each wire pixel goes out exactly
    // once. Keys are rounded to the renderer's 0.1 formatting precision, so
    // sub-pixel route differences still count as "the same piece".
    for (port, crosses) in out.iter_mut().zip(&crossings) {
        let mut seg_keys: Vec<(i64, i64, i64, i64)> = Vec::new();
        let mut leads: Vec<Vec<FrameLeadSeg>> = Vec::new();
        for &(nid, tx, ty) in crosses {
            if ground_nets.contains(&nid) {
                continue;
            }
            let Some(raw) = route_lead(port, nid, tx, ty, graph, trees) else {
                continue;
            };
            let mut lead: Vec<FrameLeadSeg> = Vec::new();
            for s in raw {
                let key = (
                    (s.x1 * 10.0).round() as i64,
                    (s.y1 * 10.0).round() as i64,
                    (s.x2 * 10.0).round() as i64,
                    (s.y2 * 10.0).round() as i64,
                );
                if seg_keys.contains(&key) {
                    continue;
                }
                seg_keys.push(key);
                lead.push(s);
            }
            if !lead.is_empty() {
                leads.push(lead);
            }
        }
        port.leads = leads;
    }
    out
}

/// ★ U160-5: route one port lead from the anchor tick's inner end to the net's
/// boundary-crossing symbol.
///
/// Candidates are tried in order of visual simplicity and the FIRST one whose
/// whole corridor is clear is drawn: straight, then the two L shapes, then a
/// dodged Z at successive [`GUTTER_STEP`] offsets around the crossing's axis
/// (the hbl `dc` port: its symbol sits on the VMIC row, so the straight
/// corridor would run the supply lead through the bead's trunk — a visual
/// short). A crossing tree segment of a FOREIGN net, or a real component box,
/// blocks a corridor; the lead's own net is free (landing on its own wire is a
/// junction, not a short). If nothing clears — content packed to the frame —
/// the plain horizontal-first L is drawn anyway: a crossing beats a missing
/// connection.
fn route_lead(
    port: &FramePort,
    net_nid: i64,
    tx: f64,
    ty: f64,
    graph: &McVecGraph,
    trees: &[EquiTree],
) -> Option<Vec<FrameLeadSeg>> {
    // Mirrors the tick the renderer draws; the lead continues it inward.
    const TICK: f64 = 9.0;
    let (sx, sy) = match port.side {
        EntrySide::Left => (port.x + TICK, port.y),
        EntrySide::Right => (port.x - TICK, port.y),
        EntrySide::Top => (port.x, port.y + TICK),
        EntrySide::Bottom => (port.x, port.y - TICK),
    };

    let seg = |x1: f64, y1: f64, x2: f64, y2: f64| FrameLeadSeg { x1, y1, x2, y2 };
    let tidy = |mut pieces: Vec<FrameLeadSeg>| -> Vec<FrameLeadSeg> {
        pieces.retain(|s| (s.x1 - s.x2).abs() > 0.5 || (s.y1 - s.y2).abs() > 0.5);
        pieces
    };

    let mut cands: Vec<Vec<FrameLeadSeg>> = Vec::new();
    // Straight — only when it really is orthogonal (a crossing off the anchor's
    // row/edge is the L shapes' job; an unconstrained "straight" piece would be
    // a diagonal across the drawing).
    if (sy - ty).abs() <= 0.5 || (sx - tx).abs() <= 0.5 {
        cands.push(tidy(vec![seg(sx, sy, tx, ty)]));
    }
    if (sy - ty).abs() > 0.5 {
        cands.push(tidy(vec![seg(sx, sy, tx, sy), seg(tx, sy, tx, ty)]));
        cands.push(tidy(vec![seg(sx, sy, sx, ty), seg(sx, ty, tx, ty)]));
    }
    // Dodged Z routes: the lead's long run shifts off the crossing's axis until
    // it slips between the rows — or, when the content fills every lane between
    // the anchor and the crossing, all the way through the channel along the
    // frame edge. `GUTTER_STEP` keeps it parallel to any gutter another net
    // already deflected into.
    let horizontal_travel = matches!(port.side, EntrySide::Left | EntrySide::Right);
    for k in 1..=30i32 {
        for s in [1.0, -1.0] {
            if horizontal_travel {
                let mid_y = ty + s * k as f64 * GUTTER_STEP;
                cands.push(tidy(vec![
                    seg(sx, sy, sx, mid_y),
                    seg(sx, mid_y, tx, mid_y),
                    seg(tx, mid_y, tx, ty),
                ]));
            } else {
                let mid_x = tx + s * k as f64 * GUTTER_STEP;
                cands.push(tidy(vec![
                    seg(sx, sy, mid_x, sy),
                    seg(mid_x, sy, mid_x, ty),
                    seg(mid_x, ty, tx, ty),
                ]));
            }
        }
    }

    let best = cands
        .iter()
        .find(|pieces| corridor_clear(graph, trees, net_nid, pieces))
        // Best effort: the first orthogonal L (horizontal-first) — a crossing
        // beats a missing connection.
        .or_else(|| cands.first())?
        .clone();
    if best.is_empty() {
        return None;
    }
    crate::vlog!(
        "[frame-lead] port '{}' net#{} {} piece(s): {:?}",
        port.name,
        net_nid,
        best.len(),
        best.iter().map(|s| format!("({:.0},{:.0})->({:.0},{:.0})", s.x1, s.y1, s.x2, s.y2)).collect::<Vec<_>>()
    );
    Some(best)
}

/// ★ U160-5: is every piece of a candidate lead free of foreign wires and
/// component bodies? Each piece is a rectangle inflated by [`LEAD_CLEARANCE`];
/// a tree segment of another net (or a real box — glyph kinds are drawn as tree
/// symbols, not boxes) intersecting that rectangle blocks the route. Leads of
/// sibling ports are not yet in the trees, so two leads may overlap; each is
/// routed against the wired world only.
fn corridor_clear(
    graph: &McVecGraph,
    trees: &[EquiTree],
    net_nid: i64,
    pieces: &[FrameLeadSeg],
) -> bool {
    const LEAD_CLEARANCE: f64 = 2.0;
    for p in pieces {
        let (xa, xb) = (
            p.x1.min(p.x2) - LEAD_CLEARANCE,
            p.x1.max(p.x2) + LEAD_CLEARANCE,
        );
        let (ya, yb) = (
            p.y1.min(p.y2) - LEAD_CLEARANCE,
            p.y1.max(p.y2) + LEAD_CLEARANCE,
        );
        for t in trees {
            // A tree's own wire is free: meeting it is a junction with this
            // very net. (`symbols` is never empty for a rendered tree.)
            let Some(sym) = t.symbols.first() else {
                continue;
            };
            if sym.net_id == net_nid {
                continue;
            }
            for s in &t.segments {
                let (sa, sb) = (s.x1.min(s.x2) - 1.0, s.x1.max(s.x2) + 1.0);
                let (ta, tb) = (s.y1.min(s.y2) - 1.0, s.y1.max(s.y2) + 1.0);
                if sa < xb && xa < sb && ta < yb && ya < tb {
                    return false;
                }
            }
        }
        for b in &graph.boxes {
            if b.w <= 0.0 || b.h <= 0.0 {
                continue;
            }
            if matches!(
                b.kind,
                BoxKind::PowerLabel | BoxKind::Dot | BoxKind::PortTerminal
            ) {
                continue;
            }
            if b.x - 1.0 < xb && xa < b.x + b.w + 1.0 && b.y - 1.0 < yb && ya < b.y + b.h + 1.0 {
                return false;
            }
        }
    }
    true
}

/// Push same-edge anchors apart so their labels stay legible.
///
/// Pure geometry: sort the edge's anchors, then walk them forward keeping at
/// least [`PORT_MIN_SPACING`] between neighbours, clamped to the edge's usable
/// span. No reordering by name, no preference by kind.
fn spread_along_edges(ports: &mut [FramePort], min_x: f64, min_y: f64, max_x: f64, max_y: f64) {
    for side in [
        EntrySide::Left,
        EntrySide::Right,
        EntrySide::Top,
        EntrySide::Bottom,
    ] {
        let (lo, hi) = match side {
            EntrySide::Left | EntrySide::Right => {
                (min_y + PORT_CORNER_KEEP, max_y - PORT_CORNER_KEEP)
            }
            _ => (min_x + PORT_CORNER_KEEP, max_x - PORT_CORNER_KEEP),
        };
        let hi = hi.max(lo);
        let mut idx: Vec<usize> = ports
            .iter()
            .enumerate()
            .filter(|(_, p)| p.side == side)
            .map(|(i, _)| i)
            .collect();
        idx.sort_by(|&a, &b| {
            let pa = if matches!(side, EntrySide::Left | EntrySide::Right) {
                ports[a].y
            } else {
                ports[a].x
            };
            let pb = if matches!(side, EntrySide::Left | EntrySide::Right) {
                ports[b].y
            } else {
                ports[b].x
            };
            pa.partial_cmp(&pb).unwrap_or(std::cmp::Ordering::Equal)
        });
        // Forward pass, then pull the run back if it overshot the far end.
        let mut last = f64::NEG_INFINITY;
        for &i in &idx {
            let cur = if matches!(side, EntrySide::Left | EntrySide::Right) {
                ports[i].y
            } else {
                ports[i].x
            };
            let v = cur.max(last + PORT_MIN_SPACING).min(hi);
            set_along(&mut ports[i], side, v, lo);
            last = v;
        }
        if let Some(&last_i) = idx.last() {
            let end = if matches!(side, EntrySide::Left | EntrySide::Right) {
                ports[last_i].y
            } else {
                ports[last_i].x
            };
            if end > hi {
                let mut next = hi;
                for &i in idx.iter().rev() {
                    let cur = if matches!(side, EntrySide::Left | EntrySide::Right) {
                        ports[i].y
                    } else {
                        ports[i].x
                    };
                    let v = cur.min(next).max(lo);
                    set_along(&mut ports[i], side, v, lo);
                    next = v - PORT_MIN_SPACING;
                }
            }
        }
    }
}

fn set_along(p: &mut FramePort, side: EntrySide, v: f64, lo: f64) {
    if matches!(side, EntrySide::Left | EntrySide::Right) {
        p.y = v.max(lo);
    } else {
        p.x = v.max(lo);
    }
}
