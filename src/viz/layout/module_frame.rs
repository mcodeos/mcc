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
//! terminal symbol projected onto the nearest frame edge. No name is consulted,
//! and the renderer recomputes nothing — it draws the rect and the labels as
//! written.

use crate::vector::graph::{EntrySide, FramePort, McVecGraph, ModuleFrame};

use super::equipotential_tree::{build_all_trees, content_bbox};

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
    let frame = ModuleFrame {
        x: rect.0,
        y: rect.1,
        w: rect.2,
        h: rect.3,
        title: graph.name.clone(),
        ports: frame_ports(graph, &trees, rect),
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
    let mut seen: Vec<(i64, f64, f64, String, bool)> = Vec::new();
    for net in &graph.nets {
        let Some(bi) = net.boundary.as_ref() else {
            continue;
        };
        if seen.iter().any(|(id, ..)| *id == bi.port_group_id) {
            continue;
        }
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
        seen.push((
            bi.port_group_id,
            p.0,
            p.1,
            bi.port_name.clone(),
            bi.is_supply,
        ));
    }

    // Project each crossing onto the frame edge it leaves through, then spread
    // the anchors on each edge so two labels cannot land on top of each other.
    let mut out: Vec<FramePort> = Vec::with_capacity(seen.len());
    for (_, px, py, name, is_supply) in seen {
        let dx = (px - cx) / ((max_x - min_x) / 2.0).max(1.0);
        let dy = (py - cy) / ((max_y - min_y) / 2.0).max(1.0);
        let (side, x, y) = if dx.abs() >= dy.abs() {
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
        };
        out.push(FramePort {
            name,
            x,
            y,
            side,
            is_supply,
        });
    }
    spread_along_edges(&mut out, min_x, min_y, max_x, max_y);
    out
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
