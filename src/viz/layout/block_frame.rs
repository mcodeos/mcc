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
//! Everything is structural — spans against offsets, never names. A layer with
//! no partition table (components, func inner layers) and a module whose boxes
//! all sit outside every partition get no frames at all, so the pass is a
//! no-op for every board that writes no `block`.

use std::collections::HashMap;

use crate::semantic::common::BlockPartition;
use crate::vector::graph::{BoxKind, McVecBox, McVecGraph, ModuleFrame};

/// Gutter between the member boxes and the frame.
const BLOCK_FRAME_PAD: f64 = 10.0;

/// Horizontal clearance between sibling clusters, and between a cluster and a
/// box that belongs to no partition. Frame borders pad their union by
/// [`BLOCK_FRAME_PAD`], so sibling borders stay visibly apart — two frames,
/// never one merged outline.
const BLOCK_SEP_GAP: f64 = 40.0;
/// Sweep bound: one sweep resolves every overlapping pair once, the same
/// budget shape as the overlap pass (`SEP_SWEEPS`).
const BLOCK_SEP_SWEEPS: usize = 64;

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

// === Cluster separation: the frames must not merge ===

/// Keep sibling block clusters from interleaving, so each partition's frame
/// encloses exactly its own members (CIMP §1 U168: frames never merge).
///
/// The device layout orders boxes by net trees and knows nothing of the
/// partition face, so members of two `block`s can interleave and their frames
/// — bounding rects of the members — overlap and read as one merged frame.
/// This pass runs in the same slot as `separate_overlaps_x` (after layout,
/// before the canvas fit): it shifts whole clusters along X, the axis the
/// M7.4 lesson declared safe — rows and trunks are y-based, and the trees
/// re-derive at render from the moved anchors.
///
/// Two moves, both deterministic:
///
/// 1. sibling clusters (same parent, source order) are pushed apart so their
///    member rects clear [`BLOCK_SEP_GAP`] — the later partition moves;
/// 2. a movable box of *no* partition caught inside a cluster's rect is pushed
///    right of it — a frame holds its own members and nothing else.
///
/// Nested clusters ride rigidly with their parent; their own siblings were
/// settled first (depth-first). Returns the number of boxes moved.
pub fn separate_block_frames_x(graph: &mut McVecGraph) -> usize {
    if graph.block_partitions.roots.is_empty() {
        return 0;
    }

    // Direct members per partition (keyed by span start); movable boxes of no
    // partition are the drifters.
    let mut members: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut drifters: Vec<usize> = Vec::new();
    for (i, b) in graph.boxes.iter().enumerate() {
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
        match graph.block_partitions.innermost(span.offset_usize()) {
            Some(p) => members.entry(p.span.start).or_default().push(i),
            None => {
                if separable(b) {
                    drifters.push(i);
                }
            }
        }
    }
    if members.is_empty() {
        return 0;
    }

    // A cluster moves rigidly: its subtree box set, direct members included.
    // Cloned because the sweep mutates boxes while walking the tree.
    let roots = graph.block_partitions.roots.clone();
    let mut subtree: HashMap<usize, Vec<usize>> = HashMap::new();
    for root in &roots {
        collect_subtree(root, &members, &mut subtree);
    }

    let mut total = 0usize;
    for _ in 0..BLOCK_SEP_SWEEPS {
        let mut moved = sep_siblings(&roots, graph, &subtree);

        // Drifters out of every cluster rect, rects by ascending x so a box
        // pushed out of one frame converges out of the next.
        let mut rects: Vec<(usize, [f64; 4])> = Vec::new();
        for part in &roots {
            flatten_parts(part, &mut |p| {
                rects.push((p.span.start, cluster_rect(graph, &subtree[&p.span.start])));
            });
        }
        rects.sort_by(|a, b| a.1[0].partial_cmp(&b.1[0]).unwrap_or(std::cmp::Ordering::Equal));
        for &d in &drifters {
            let (bx, by, bw, bh) = {
                let b = &graph.boxes[d];
                (b.x, b.y, b.w, b.h)
            };
            for (_, r) in &rects {
                // Hit = the box reaches into the frame's padded border.
                let hit = bx < r[0] + r[2] + BLOCK_FRAME_PAD
                    && r[0] < bx + bw + BLOCK_FRAME_PAD
                    && by < r[1] + r[3] + BLOCK_FRAME_PAD
                    && r[1] < by + bh + BLOCK_FRAME_PAD;
                if hit {
                    let dx = r[0] + r[2] + BLOCK_SEP_GAP - bx;
                    if dx > 0.0 {
                        graph.boxes[d].x += dx;
                        moved += 1;
                    }
                }
            }
        }

        total += moved;
        if moved == 0 {
            break;
        }
    }
    if total > 0 {
        crate::vlog!(
            "[block_frame] layer '{}' separated block clusters: {} box move(s)",
            graph.name,
            total
        );
    }
    total
}

/// Whether a box is pushed when it sits inside a partition it does not belong
/// to: real placed parts, the roster `overlap::separate_overlaps_x` moves.
fn separable(b: &McVecBox) -> bool {
    b.id >= 0 && matches!(b.kind, BoxKind::TwoPin | BoxKind::MultiPin)
}

/// The subtree box set of one partition, direct members included.
fn collect_subtree(
    part: &BlockPartition,
    members: &HashMap<usize, Vec<usize>>,
    out: &mut HashMap<usize, Vec<usize>>,
) {
    let mut ids = members.get(&part.span.start).cloned().unwrap_or_default();
    for child in &part.children {
        collect_subtree(child, members, out);
        ids.extend(out.get(&child.span.start).into_iter().flatten().copied());
    }
    out.insert(part.span.start, ids);
}

fn flatten_parts<'a, F: FnMut(&'a BlockPartition)>(part: &'a BlockPartition, f: &mut F) {
    f(part);
    for child in &part.children {
        flatten_parts(child, f);
    }
}

/// Bounding rect `[x, y, w, h]` of a cluster's member boxes from current
/// geometry. An empty cluster (a partition owning no box never reaches here
/// from the sibling pass) reads as a point at the origin.
fn cluster_rect(graph: &McVecGraph, ids: &[usize]) -> [f64; 4] {
    let mut r = [0.0f64; 4];
    let mut any = false;
    for &i in ids {
        let b = &graph.boxes[i];
        if !any {
            r = [b.x, b.y, b.w, b.h];
            any = true;
        } else {
            let x2 = (r[0] + r[2]).max(b.x + b.w);
            let y2 = (r[1] + r[3]).max(b.y + b.h);
            r[0] = r[0].min(b.x);
            r[1] = r[1].min(b.y);
            r[2] = x2 - r[0];
            r[3] = y2 - r[1];
        }
    }
    r
}

/// One sweep of sibling separation over `parts` and their descendants:
/// children settle first, then this sibling set is pairwise separated — the
/// later partition in source order moves right. Returns boxes moved.
fn sep_siblings(
    parts: &[BlockPartition],
    graph: &mut McVecGraph,
    subtree: &HashMap<usize, Vec<usize>>,
) -> usize {
    let mut moved = 0usize;
    for part in parts {
        moved += sep_siblings(&part.children, graph, subtree);
    }
    for i in 0..parts.len() {
        for j in (i + 1)..parts.len() {
            let ri = cluster_rect(graph, &subtree[&parts[i].span.start]);
            let rj = cluster_rect(graph, &subtree[&parts[j].span.start]);
            // Vertically disjoint bands never collide on the x axis.
            let vert =
                ri[1] < rj[1] + rj[3] + BLOCK_SEP_GAP && rj[1] < ri[1] + ri[3] + BLOCK_SEP_GAP;
            if !vert {
                continue;
            }
            // The right-hand cluster gives way; equal left edges break the
            // tie by source order (the later partition moves).
            let (target, dx) = if ri[0] <= rj[0] {
                (j, (ri[0] + ri[2] + BLOCK_SEP_GAP) - rj[0])
            } else {
                (i, (rj[0] + rj[2] + BLOCK_SEP_GAP) - ri[0])
            };
            if dx <= 0.0 {
                continue;
            }
            for &bi in &subtree[&parts[target].span.start] {
                graph.boxes[bi].x += dx;
            }
            moved += subtree[&parts[target].span.start].len();
        }
    }
    moved
}
