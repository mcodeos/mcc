// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Dashed frames around a module's in-body `block` partitions (CIMP §1 U168).
//!
//! A partition groups the statements of a body (spec/03 §8) — the semantic face
//! never records the grouping, but the *source region* of every drawn box is
//! known ([`McVecBox::source_span`]). This pass closes the projection: a box is
//! attributed to the innermost partition whose span contains its source offset,
//! and each partition that owns at least one box gets a dashed frame around
//! their bounding rect. Nested partitions nest: a parent's rect encloses its
//! children's.
//!
//! **The grouping law (CIMP §1 U171)**: a block frame is a *pure post-layout
//! derivation*. It groups — one dashed rect around the bounding union of its
//! member boxes — and does nothing else. The frame occupies no volume: it never
//! moves or resizes a box, takes no part in overlap separation, and never
//! grows the viewBox. Frames may therefore overlap each other or enclose a box
//! that belongs to no partition; that is accepted display, not a layout fault
//! — membership is declared by source span, never by geometric containment.
//! A pass that would keep frames apart is a layout pass and belongs to the
//! layout proper, arguing from layout's own grounds — not here. The law binds
//! every scope frame: a `func` frame, when it gains a producer, obeys it too
//! (device-layer-drawing-design §12).
//!
//! Everything is structural — spans against offsets, never names. A layer with
//! no partition table (components, func inner layers) and a module whose boxes
//! all sit outside every partition get no frames at all, so the pass is a
//! no-op for every board that writes no `block`.

use std::collections::HashMap;

use crate::semantic::common::BlockPartition;
use crate::vector::graph::{BoxKind, McVecGraph, ModuleFrame};

/// Gutter between the member boxes and the frame.
const BLOCK_FRAME_PAD: f64 = 10.0;

/// A box rect in layer coordinates (the fields the frame encloses).
struct BoxRect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// Draw the block-partition frames of a module's own layer, if any.
///
/// Called after `fit_content_to_canvas` (final geometry) in the device
/// pipeline. The canvas keeps its [`CANVAS_MARGIN`] around the content, and the
/// frame pad is smaller than the module frame's, so a block frame never leaves
/// the fitted viewBox — the viewBox is returned unchanged.
///
/// Frames are emitted parents-first (pre-order), so the renderer can paint
/// nested frames above their parent's border.
pub fn layout_block_frames(graph: &mut McVecGraph) {
    graph.block_frames.clear();
    if graph.block_partitions.roots.is_empty() {
        return;
    }

    // Direct members: innermost-partition span start -> member rects. Label-kind
    // boxes are tree symbols, not components — the same roster the renderer
    // skips when it draws real boxes.
    let mut direct: HashMap<usize, Vec<BoxRect>> = HashMap::new();
    for b in &graph.boxes {
        if matches!(
            b.kind,
            BoxKind::PowerLabel | BoxKind::Dot | BoxKind::PortTerminal
        ) {
            continue;
        }
        let Some(span) = &b.source_span else {
            continue;
        };
        if span.uri != graph.block_partitions.uri {
            continue;
        }
        let Some(part) = graph.block_partitions.innermost(span.offset_usize()) else {
            continue;
        };
        direct.entry(part.span.start).or_default().push(BoxRect {
            x: b.x,
            y: b.y,
            w: b.w,
            h: b.h,
        });
    }
    if direct.is_empty() {
        return;
    }

    let mut frames = Vec::new();
    for root in &graph.block_partitions.roots {
        frame_for(root, &direct, &mut frames);
    }
    crate::vlog!(
        "[block_frame] layer '{}' drew {} frame(s) for {} partition root(s)",
        graph.name,
        frames.len(),
        graph.block_partitions.roots.len()
    );
    graph.block_frames = frames;
}

/// The frame rect of one partition: the union of its direct members' rects and
/// its children's frame rects, padded. `None` when neither owns a box.
///
/// Emits parent-before-children into `out` so nested frames paint above the
/// parent's border, and returns the rect so the parent's union can enclose it.
fn frame_for(
    part: &BlockPartition,
    direct: &HashMap<usize, Vec<BoxRect>>,
    out: &mut Vec<ModuleFrame>,
) -> Option<(f64, f64, f64, f64)> {
    let mut union: Option<(f64, f64, f64, f64)> = None;
    let absorb = |u: &mut Option<(f64, f64, f64, f64)>, r: (f64, f64, f64, f64)| {
        *u = Some(match *u {
            None => r,
            Some((x, y, w, h)) => {
                let x2 = x + w;
                let y2 = y + h;
                let nx = x.min(r.0);
                let ny = y.min(r.1);
                (
                    nx,
                    ny,
                    (x2.max(r.0 + r.2)) - nx,
                    (y2.max(r.1 + r.3)) - ny,
                )
            }
        });
    };

    for b in direct.get(&part.span.start).into_iter().flatten() {
        absorb(&mut union, (b.x, b.y, b.w, b.h));
    }

    let mut child_frames = Vec::new();
    for child in &part.children {
        if let Some(rect) = frame_for(child, direct, &mut child_frames) {
            absorb(&mut union, rect);
        }
    }

    let (x, y, w, h) = union?;
    out.push(ModuleFrame {
        x: x - BLOCK_FRAME_PAD,
        y: y - BLOCK_FRAME_PAD,
        w: w + 2.0 * BLOCK_FRAME_PAD,
        h: h + 2.0 * BLOCK_FRAME_PAD,
        title: part.name.clone(),
        ports: Vec::new(),
    });
    out.extend(child_frames);
    Some((x - BLOCK_FRAME_PAD, y - BLOCK_FRAME_PAD, w + 2.0 * BLOCK_FRAME_PAD, h + 2.0 * BLOCK_FRAME_PAD))
}
