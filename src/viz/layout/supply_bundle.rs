// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ P1-a · supply-bundle grouping for the root block diagram.
//!
//! Which root edges share one drawn "trunk" and which are drawn on their own used
//! to be decided inline in `render_block_edges`. That put a layout decision inside
//! the renderer, and the decision was made by iterating a `HashMap` -- so the
//! *order* of the trunk groups (and of the demoted edges appended after them) was
//! whatever the hash seed produced that run. Nothing caught it: the SVG element
//! emission order is not part of any comparison set, so the golden, `renderdiff`
//! and the root anchor all stay green while the bytes move.
//!
//! This module owns the rule now. It is a plain function over the decided edges,
//! with the ordering pinned by one explicit sort key -- see `plan_groups`.
//!
//! The geometry lives here too now (P1-a step ii, P1-b): one authority for where
//! an edge attaches. Every end lands on its own **lead** when the box draws one --
//! found by endpoint pin id, never by matching a net label against a pin name --
//! and falls back to the midpoint of the edge facing the other box, loudly. See
//! `edge-anchor-design.md` §3 (L1-L4) and §6 for the batch.

use std::collections::HashMap;

use crate::vector::graph::{EntrySide, McVecBox, McVecGraph};
use crate::viz::layout::edge_decide::{BlockEdge, EdgeKind};
use crate::viz::render::pin_render::LEAD_STUB_LEN;

/// A power bundle is drawn as a shared vertical trunk with taps only once it has
/// this many members; below it, the edges are drawn individually so that short
/// pass-through stubs (e.g. a V1V2 bridge) still take part in the tap-collision
/// check in the renderer.
pub const TRUNK_MIN_CONSUMERS: usize = 3;

/// Stroke width of a drawn supply trunk and its taps. A bundle draws heavier
/// than its individual edges so the shared path reads as one thick trunk
/// (P2: the width is decided here with the bundle, not by a literal in the
/// renderer).
pub const TRUNK_WIDTH: f64 = 3.0;

/// One power label drawn as a shared trunk: a vertical rail plus one tap per
/// member edge.
#[derive(Debug, Clone)]
pub struct SupplyTrunk {
    /// The net/trunk label the members share; also the drawn trunk label.
    pub label: String,
    /// Indices into the `decide_edges` result, ascending.
    pub members: Vec<usize>,
}

/// The grouping of one layer's edges into trunk bundles and individual edges.
#[derive(Debug, Clone, Default)]
pub struct SupplyGroups {
    pub trunks: Vec<SupplyTrunk>,
    /// Indices into the `decide_edges` result, in draw order: every non-power or
    /// unlabeled edge in `decide_edges` order, then the demoted power groups.
    pub individual: Vec<usize>,
}

/// Ordering key for one edge: source position first (declaration order), then the
/// endpoints and label so the key is total even when a `source_span` is missing.
///
/// `source_span` is `None` for a good many edges, so the key routinely degenerates
/// to `(from_box, to_box, label)` -- stable and explainable, but *arbitrary*: it is
/// neither declaration order nor "where the net attaches". The meaningful order is
/// P1-b's L4 (endpoint identity). What matters here is that the key is **total**,
/// so the order stops depending on a `HashMap`'s iteration order.
fn edge_key(edges: &[BlockEdge], idx: usize) -> (u32, i64, i64, String) {
    let edge = &edges[idx];
    let line = edge
        .source_span
        .as_ref()
        .map(|p| p.offset)
        .unwrap_or(u32::MAX);
    (line, edge.from_box, edge.to_box, edge.label.clone())
}

/// Ordering key for a trunk group: the smallest member key. Two groups can never
/// share a label (labels are the grouping key), so this orders them by where their
/// earliest member was declared.
fn trunk_key(edges: &[BlockEdge], members: &[usize]) -> (u32, i64, i64, String) {
    members
        .iter()
        .map(|&i| edge_key(edges, i))
        .min()
        .unwrap_or((u32::MAX, 0, 0, String::new()))
}

/// The bundle identity of a power edge: the structured trunk name when the
/// source carries one (P0), else the stripped power label. The drawn label is
/// display-only; grouping keys on this identity, never on label text.
fn bundle_key(edge: &BlockEdge) -> Option<String> {
    if edge.kind != EdgeKind::Power {
        return None;
    }
    edge.trunk
        .as_ref()
        .and_then(|t| t.name.clone())
        .or_else(|| (!edge.label.is_empty()).then(|| edge.label.clone()))
}

/// Split `edges` into shared-trunk bundles and individual edges.
///
/// Grouping rule (unchanged from the inline original): a `Power` edge with a
/// non-empty label joins the bundle named by that label; a bundle is drawn as a
/// trunk once it has `TRUNK_MIN_CONSUMERS` members, otherwise its members are
/// demoted to individual edges.
///
/// Ordering (the part this module fixes): the bundles are grouped through a
/// `HashMap`, so **both** the returned trunk order and the order of the demoted
/// members are pinned by an explicit sort below. Without it they follow the hash
/// iteration order, which is stable within a process and different across runs.
pub fn plan_groups(edges: &[BlockEdge]) -> SupplyGroups {
    let mut by_bundle: HashMap<String, Vec<usize>> = HashMap::new();
    let mut individual: Vec<usize> = Vec::new();

    for (i, edge) in edges.iter().enumerate() {
        match bundle_key(edge) {
            Some(key) => by_bundle.entry(key).or_default().push(i),
            None => individual.push(i),
        }
    }

    let mut trunks: Vec<SupplyTrunk> = Vec::new();
    let mut demoted: Vec<usize> = Vec::new();
    for (key, mut members) in by_bundle {
        members.sort_unstable();
        if members.len() >= TRUNK_MIN_CONSUMERS {
            trunks.push(SupplyTrunk {
                label: key,
                members,
            });
        } else {
            demoted.extend(members);
        }
    }

    trunks.sort_by(|a, b| trunk_key(edges, &a.members).cmp(&trunk_key(edges, &b.members)));
    demoted.sort_by(|a, b| edge_key(edges, *a).cmp(&edge_key(edges, *b)));
    individual.extend(demoted);

    SupplyGroups { trunks, individual }
}

// Geometry

/// **Fallback only** (L2). The midpoint of the box edge facing `(target_x,
/// target_y)` — the shape the drawing used before P1-b, kept for ends that have
/// no lead at all (edge-anchor §3.1: such an end means its port is not drawn,
/// which is a fact worth seeing rather than a shape worth inventing).
///
/// Its geometry is deliberately byte-identical to the pre-P1-b anchor, so a
/// change in the drawing is exactly a change in *which ends have leads* — that is
/// what makes the batch's predictive gate meaningful.
fn rail_anchor(b: &McVecBox, target_x: f64, target_y: f64) -> (f64, f64) {
    let bx = b.x + b.w / 2.0;
    let by = b.y + b.h / 2.0;
    let dx = target_x - bx;
    let dy = target_y - by;

    if dx.abs() >= dy.abs() {
        // Horizontal: left or right edge
        if dx > 0.0 {
            (b.x + b.w, b.y + b.h / 2.0)
        } else {
            (b.x, b.y + b.h / 2.0)
        }
    } else {
        // Vertical: top or bottom edge
        if dy > 0.0 {
            (b.x + b.w / 2.0, b.y + b.h)
        } else {
            (b.x + b.w / 2.0, b.y)
        }
    }
}

/// Where the lead at `ep` meets the box border — the lead's **root**.
fn side_point(b: &McVecBox, ep: &crate::vector::graph::EntryPoint) -> (f64, f64) {
    match ep.side {
        EntrySide::Left => (b.x, b.y + ep.offset * b.h),
        EntrySide::Right => (b.x + b.w, b.y + ep.offset * b.h),
        EntrySide::Top => (b.x + ep.offset * b.w, b.y),
        EntrySide::Bottom => (b.x + ep.offset * b.w, b.y + b.h),
    }
}

/// One end's landing on a box: the lead's root and its tip.
///
/// The root is where the lead meets the border (also what M13 measures a pin's
/// reachability from, so a wire must still end there); the tip is the outer end of
/// the drawn lead. **L5**: a wire approaches a lead along the lead's own axis, so
/// a lead on a horizontal face is entered from its tip's row -- running at the
/// border's row would lay the wire along the box border itself, indistinguishable
/// from the frame. A left/right lead is entered on the tip's row either way, so
/// the two points share it and the run stays the one segment it always was. An end
/// with no lead at all has root == tip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LeadEnd {
    pub root: (f64, f64),
    pub tip: (f64, f64),
}

impl LeadEnd {
    /// A landing that is not a lead (the facing-edge fallback): one point, no axis.
    fn point(p: (f64, f64)) -> Self {
        Self { root: p, tip: p }
    }

    /// The row the wire must run along before it comes in over the lead.
    pub fn approach_y(&self) -> f64 {
        self.tip.1
    }
}

/// The landing of the box's lead for an edge end, if it has one (**L1+L4**).
///
/// `pins` are the endpoint identities of that end (ascending pin id, §5), so the
/// choice is structural: the first pin that the box actually draws a lead for.
/// This is the one authority for "where does this end attach" -- the lead's own
/// `(side, offset)`, never a re-derived facing edge.
fn lead_anchor(b: &McVecBox, pins: &[i64]) -> Option<LeadEnd> {
    pins.iter().filter(|p| **p > 0).find_map(|p| {
        b.entry_points.iter().find(|ep| ep.pin_id == *p).map(|ep| {
            let root = side_point(b, ep);
            // The tip is the root walked one lead-length along the lead's own
            // outward normal -- the same length the lead is drawn with (L4:
            // one computation, one authority).
            let outward = match ep.side {
                EntrySide::Left => (-LEAD_STUB_LEN, 0.0),
                EntrySide::Right => (LEAD_STUB_LEN, 0.0),
                EntrySide::Top => (0.0, -LEAD_STUB_LEN),
                EntrySide::Bottom => (0.0, LEAD_STUB_LEN),
            };
            LeadEnd {
                root,
                tip: (root.0 + outward.0, root.1 + outward.1),
            }
        })
    })
}

/// One end's landing: the lead if there is one, else the old facing-edge midpoint.
///
/// The fallback is not silent (§3.1): "this end has no lead" means the port is not
/// drawn at all, which is a fact worth seeing. Geometry under the fallback is
/// byte-identical to the pre-P1-b drawing, so a change in the drawing is exactly a
/// change in which ends have leads -- that is what makes the batch's gate predictive.
fn end_anchor(b: &McVecBox, pins: &[i64], target: (f64, f64), label: &str, end: &str) -> LeadEnd {
    if let Some(lead) = lead_anchor(b, pins) {
        return lead;
    }
    crate::vlog!(
        "[supply_bundle] no lead for '{}' {} end on box {} '{}' ({} pin id(s)): \
         falling back to the facing edge midpoint",
        label,
        end,
        b.id,
        b.name,
        pins.len()
    );
    LeadEnd::point(rail_anchor(b, target.0, target.1))
}

/// One power label drawn as a shared trunk: a vertical rail plus a tap per member.
#[derive(Debug, Clone)]
pub struct TrunkDraw {
    pub label: String,
    /// x of the vertical rail.
    pub x: f64,
    /// y extent of the rail: the driver anchor and the taps decide it.
    pub y_min: f64,
    pub y_max: f64,
    /// Where the driver's stub leaves the driver box, if a driver was resolved.
    pub driver: Option<LeadEnd>,
    /// One landing per consumer: the rail-to-consumer run ends on the lead's root,
    /// approached along the lead's axis from `tip` (L5).
    pub taps: Vec<LeadEnd>,
    /// P2: stroke width of the rail and its taps (bundle presence decides it).
    pub stroke_width: f64,
    /// ★ P3 (ret lineage, opt-in): the end of the single return lead drawn at
    /// the driver end of a fan-out (the design draws one return lead, not one
    /// per load). `Some` when the bundle's members carry a declared DC-pair
    /// return and a driver anchor exists; `None` otherwise. The renderer only
    /// draws it under the ret-lane opt-in flag — it never touches the edges.
    pub ret_stub: Option<(f64, f64)>,
}

/// The paired return lane of a point-to-point power edge (opt-in drawing).
#[derive(Debug, Clone)]
pub struct RetLane {
    /// The lane start — the hot lane's `from` translated by the offset.
    pub from: (f64, f64),
    /// The lane end — the hot lane's `to` translated by the same offset.
    pub to: (f64, f64),
    /// Whether the hot lane draws an L (the return lane mirrors the shape).
    pub ortho: bool,
}

/// One edge drawn on its own, both endpoints already resolved.
#[derive(Debug, Clone)]
pub struct IndividualDraw {
    pub kind: EdgeKind,
    pub label: String,
    pub lane_count: usize,
    pub from: (f64, f64),
    pub to: (f64, f64),
    /// The ends differ on both axes, so a power edge draws an L instead of a
    /// straight segment.
    pub ortho: bool,
    /// P2: stroke width, decided here with the edge (bus / power / signal).
    pub stroke_width: f64,
    /// ★ P3 (ret lineage, opt-in): for a point-to-point power edge whose net
    /// carries a declared DC-pair return, the parallel second lane along the
    /// same spine (design §6). `None` for every other edge. Drawn only under
    /// the ret-lane opt-in flag.
    pub ret_lane: Option<RetLane>,
}

/// Everything the renderer needs to draw the root layer's edges: no geometry and
/// no grouping is decided while drawing.
#[derive(Debug, Clone, Default)]
pub struct SupplyBundlePlan {
    pub trunks: Vec<TrunkDraw>,
    pub individual: Vec<IndividualDraw>,
}

/// Build the drawing plan for the root layer of `graph` from the edges the
/// layout phase already projected onto it (P1-c: the renderer no longer runs
/// `decide_edges`).
pub fn build_plan(graph: &McVecGraph) -> SupplyBundlePlan {
    build_plan_for(graph, &graph.block_edges)
}

/// Build the plan from already-decided edges. Split out so a test can drive the
/// geometry with a hand-built edge list.
pub fn build_plan_for(graph: &McVecGraph, edges: &[BlockEdge]) -> SupplyBundlePlan {
    let groups = plan_groups(edges);
    let mut trunks: Vec<TrunkDraw> = Vec::new();

    for trunk in &groups.trunks {
        let label = &trunk.label;
        let indices = &trunk.members;

        // ★ B5: the driver is *declared*, not voted. `decide_edges` stamps every
        // power edge with the box owning the model layer's `RailSpec.driver_pin`;
        // re-deriving it here by counting `from_box` frequencies would break ties
        // by HashMap iteration order (nondeterministic across runs).
        let driver_box_id = indices.iter().find_map(|&idx| edges[idx].driver_box);
        if driver_box_id.is_none() {
            // §3.1: a fallback must be visible, never silent. Power edges are
            // stamped by construction, so this only fires for an empty group.
            crate::vlog!(
                "[supply_bundle] trunk group '{}': no declared driver among {} edge(s); \
                 falling back to the first edge's `from` (identity degraded)",
                label,
                indices.len()
            );
        }
        let driver_box_id =
            driver_box_id.or_else(|| indices.first().map(|&idx| edges[idx].from_box));

        // Compute the rail x: midpoint between the driver's right edge and the
        // rightmost consumer's left edge. If no clear driver, use the midpoint of
        // all boxes in the group.
        // The driver end's identity, accumulated across the group's member edges
        // rather than taken from whichever member happens to be scanned last.
        // R2: re-anchoring the stub per edge makes its start "the last consumer
        // edge in iteration order" -- a position decided by the loop, not by the
        // driver. Collecting the pins and resolving once afterwards makes it a
        // property of the driver end alone (L1/L4).
        let mut driver_box: Option<&McVecBox> = None;
        let mut driver_pins: Vec<i64> = Vec::new();
        let mut all_box_xs: Vec<f64> = Vec::new();
        // ★ Power-trunk lane fix: the trunk must sit in the open gutter, not on a
        //   box edge. Track the driver's right edge and the rightmost consumer's
        //   left edge separately. Min/max'ing *every* collected box edge instead
        //   lets a left-column consumer (e.g. flash/modldo/moddcdc @ x=340..500)
        //   drag the rail down to the shared right edge x=500 of the power column
        //   -- the vertical rail then runs exactly along the box borders and
        //   vanishes behind the fill.
        let mut driver_right_edge: Option<f64> = None;
        let mut rightmost_consumer_left: Option<f64> = None;

        for &idx in indices {
            let edge = &edges[idx];
            let from_box = graph.boxes.iter().find(|b| b.id == edge.from_box);
            let to_box = graph.boxes.iter().find(|b| b.id == edge.to_box);
            let (Some(from), Some(to)) = (from_box, to_box) else {
                continue;
            };
            all_box_xs.push(from.x + from.w);
            all_box_xs.push(to.x);

            let is_driver = Some(from.id) == driver_box_id;
            let is_driver_to = Some(to.id) == driver_box_id;
            if is_driver {
                driver_right_edge = Some(from.x + from.w);
                driver_box = Some(from);
                driver_pins.extend_from_slice(&edge.from_pins);
            } else if is_driver_to {
                driver_box = Some(to);
                driver_pins.extend_from_slice(&edge.to_pins);
            }
            rightmost_consumer_left = Some(
                rightmost_consumer_left
                    .map(|x: f64| x.max(to.x))
                    .unwrap_or(to.x),
            );
        }

        let trunk_x = match (driver_right_edge, rightmost_consumer_left) {
            (Some(dre), Some(rcl)) => (dre + rcl) / 2.0,
            _ => {
                all_box_xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
                if all_box_xs.len() >= 2 {
                    let leftmost = all_box_xs[0];
                    let rightmost = all_box_xs[all_box_xs.len() - 1];
                    (leftmost + rightmost) / 2.0
                } else {
                    580.0
                }
            }
        };

        // The driver stub's start is the driver end's own lead, facing the rail it
        // feeds (L2/L4). Resolved once, from the identity collected above. The
        // pins are deduplicated and ascending so the lead choice does not depend
        // on which member edge contributed first.
        driver_pins.sort_unstable();
        driver_pins.dedup();
        let driver_anchor: Option<LeadEnd> = driver_box.map(|b| {
            let cy = b.y + b.h / 2.0;
            end_anchor(b, &driver_pins, (trunk_x, cy), label, "driver")
        });

        // Consumer taps, each landing on its own lead against the resolved rail x.
        let mut taps: Vec<LeadEnd> = Vec::new();
        for &idx in indices {
            let edge = &edges[idx];
            let to_box = graph.boxes.iter().find(|b| b.id == edge.to_box);
            let Some(to) = to_box else {
                continue;
            };
            let is_driver_to = Some(to.id) == driver_box_id;
            // ★ Rail trunk-tap fix: `decide_edges` emits power edges as
            // driver→consumer, so `is_driver` is true on every edge of a
            // single-driver star. A `!is_driver && !is_driver_to` guard skips all
            // of them → empty tap list → the trunk collapses to a zero-length stub
            // at the driver. Take every edge whose *target* is a consumer
            // (secondary consumer→consumer edges in a multi-driver mesh still
            // qualify; edges pointing back at the driver are correctly excluded).
            if !is_driver_to {
                // ★ The anti-collision slide that used to live here is gone: two
                // taps can no longer land on the same point, because each end now
                // lands on a lead *it* owns, and one lead belongs to one net. If
                // two ends ever do collide on one lead, the thing to de-duplicate
                // is the lead/entry point, not the line (edge-anchor §4c).
                taps.push(end_anchor(
                    to,
                    &edge.to_pins,
                    (trunk_x, to.y + to.h / 2.0),
                    label,
                    "tap",
                ));
            }
        }

        // The rail must reach every row a run leaves it on -- for a horizontal-face
        // lead that is the tip's row, which is where the run itself sits (L5).
        let mut all_ys: Vec<f64> = taps.iter().map(|t| t.approach_y()).collect();
        if let Some(driver) = driver_anchor {
            all_ys.push(driver.approach_y());
        }
        all_ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let trunk_y_min = all_ys.first().copied().unwrap_or(100.0);
        let trunk_y_max = all_ys.last().copied().unwrap_or(740.0);

        // P3 (ret lineage): a fan-out draws one return lead at the driver end —
        // a short tick below the driver anchor, from the first member that
        // carries a declared DC-pair return.
        let ret_stub: Option<(f64, f64)> =
            if driver_anchor.is_some() && indices.iter().any(|&idx| edges[idx].ret.is_some()) {
                driver_anchor.map(|d| (d.root.0, d.root.1 + 10.0))
            } else {
                None
            };

        trunks.push(TrunkDraw {
            label: label.clone(),
            x: trunk_x,
            y_min: trunk_y_min,
            y_max: trunk_y_max,
            driver: driver_anchor,
            taps,
            stroke_width: TRUNK_WIDTH,
            ret_stub,
        });
    }

    let mut individual: Vec<IndividualDraw> = Vec::new();
    for &idx in &groups.individual {
        let edge = &edges[idx];
        let from_box = graph.boxes.iter().find(|b| b.id == edge.from_box);
        let to_box = graph.boxes.iter().find(|b| b.id == edge.to_box);
        let (Some(from), Some(to)) = (from_box, to_box) else {
            continue;
        };

        // L2/L4: every end lands on its own lead when the box has one -- power
        // and signal alike. The identity is the endpoint pin ids on the edge, so
        // this no longer matches a net label against a pin *name* (two disjoint
        // namespaces, and a miss used to land on the box centre invisibly).
        let from_end = end_anchor(
            from,
            &edge.from_pins,
            (to.x + to.w / 2.0, to.y + to.h / 2.0),
            &edge.label,
            "from",
        );
        let (x1, y1) = from_end.root;
        let (x2, y2) = if edge.kind == EdgeKind::Power {
            // Keep the same coordinate as the source on the axis where the boxes
            // are aligned.
            let to_end = end_anchor(
                to,
                &edge.to_pins,
                (from.x + from.w / 2.0, from.y + from.h / 2.0),
                &edge.label,
                "to",
            );
            let (tx, ty) = to_end.root;
            if (x1 - tx).abs() < 1.0 {
                (x1, ty)
            } else {
                (tx, y1)
            }
        } else {
            end_anchor(
                to,
                &edge.to_pins,
                (from.x + from.w / 2.0, from.y + from.h / 2.0),
                &edge.label,
                "to",
            )
            .root
        };

        // P2: the stroke width is decided here, by kind and lane count, so the
        // renderer only draws what the plan names.
        let stroke_width = if edge.lane_count > 1 {
            4.0
        } else {
            match edge.kind {
                EdgeKind::Power => 2.5,
                EdgeKind::Bus => 2.5,
                EdgeKind::Signal => 2.0,
            }
        };

        // P3 (ret lineage): a point-to-point power edge with a declared DC-pair
        // return draws a parallel second lane along the same spine. The lane is
        // the hot lane translated by a fixed perpendicular offset, so both the
        // straight and the L shape stay parallel (every segment is axis-aligned).
        let ortho = (x1 - x2).abs() > 1.0 && (y1 - y2).abs() > 1.0 && edge.kind == EdgeKind::Power;
        let ret_lane = if edge.kind == EdgeKind::Power && edge.ret.is_some() {
            let (ox, oy) = if (x2 - x1).abs() >= (y2 - y1).abs() {
                (0.0, 7.0) // dominant horizontal → shift down
            } else {
                (7.0, 0.0) // dominant vertical → shift right
            };
            Some(RetLane {
                from: (x1 + ox, y1 + oy),
                to: (x2 + ox, y2 + oy),
                ortho,
            })
        } else {
            None
        };

        individual.push(IndividualDraw {
            kind: edge.kind,
            label: edge.label.clone(),
            lane_count: edge.lane_count,
            from: (x1, y1),
            to: (x2, y2),
            ortho,
            stroke_width,
            ret_lane,
        });
    }

    SupplyBundlePlan { trunks, individual }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viz::layout::edge_decide::EdgeKind;

    fn edge(from: i64, to: i64, label: &str, kind: EdgeKind) -> BlockEdge {
        BlockEdge {
            from_box: from,
            to_box: to,
            from_pins: Vec::new(),
            to_pins: Vec::new(),
            driver_box: None,
            label: label.to_string(),
            lane_count: 1,
            kind,
            source_span: None,
            trunk: None,
            ret: None,
            bidirectional: false,
        }
    }

    /// A bundle of two is below the threshold: demoted, and demoted *after* the
    /// edges that were never grouped.
    #[test]
    fn small_bundles_are_demoted_behind_the_individual_edges() {
        let edges = vec![
            edge(1, 2, "[V1, GND]", EdgeKind::Power),
            edge(3, 4, "sig", EdgeKind::Signal),
            edge(5, 6, "[V1, GND]", EdgeKind::Power),
        ];
        let plan = plan_groups(&edges);
        assert!(
            plan.trunks.is_empty(),
            "two members are below the threshold"
        );
        assert_eq!(plan.individual[0], 1, "signal edge keeps its own order");
        assert_eq!(plan.individual, vec![1, 0, 2]);
    }

    /// The defect this module exists for: the grouping is built through a
    /// `HashMap`, so without the explicit sort the trunk order and the demoted
    /// order would follow the hash iteration order. Re-running from a differently
    /// ordered input-of-the-same-set must produce the same plan shape.
    #[test]
    fn group_order_does_not_follow_the_hash_map() {
        let edges = vec![
            edge(1, 2, "[Z, GND]", EdgeKind::Power),
            edge(1, 2, "[Z, GND]", EdgeKind::Power),
            edge(1, 2, "[Z, GND]", EdgeKind::Power),
            edge(1, 2, "[A, GND]", EdgeKind::Power),
            edge(1, 2, "[A, GND]", EdgeKind::Power),
            edge(1, 2, "[A, GND]", EdgeKind::Power),
            edge(1, 2, "[M, GND]", EdgeKind::Power),
            edge(1, 2, "[M, GND]", EdgeKind::Power),
            edge(1, 2, "[M, GND]", EdgeKind::Power),
        ];
        let labels: Vec<String> = plan_groups(&edges)
            .trunks
            .iter()
            .map(|t| t.label.clone())
            .collect();
        assert_eq!(labels, vec!["[A, GND]", "[M, GND]", "[Z, GND]"]);

        // Same set, edges listed in a different order: the labels never change.
        let mut shuffled = edges.clone();
        shuffled.reverse();
        let shuffled_labels: Vec<String> = plan_groups(&shuffled)
            .trunks
            .iter()
            .map(|t| t.label.clone())
            .collect();
        assert_eq!(shuffled_labels, labels);
    }

    /// Declaration order outranks the box ids: `[B]` is declared first even though
    /// its members carry the higher box ids.
    #[test]
    fn source_position_outranks_box_ids() {
        let at = |from: i64, label: &str, line: Option<u32>| {
            let mut e = edge(from, 2, label, EdgeKind::Power);
            e.source_span = line.map(|l| crate::semantic::common::SourcePos::new("t.mc", l));
            e
        };
        // Box ids alone would put every A edge (id 1) before every B edge (id 9).
        let edges = vec![
            at(1, "[A, GND]", Some(900)),
            at(1, "[A, GND]", None),
            at(1, "[A, GND]", None),
            at(9, "[B, GND]", Some(10)),
            at(9, "[B, GND]", Some(11)),
            at(9, "[B, GND]", None),
        ];
        let labels: Vec<String> = plan_groups(&edges)
            .trunks
            .iter()
            .map(|t| t.label.clone())
            .collect();
        assert_eq!(labels, vec!["[B, GND]", "[A, GND]"]);
    }

    // ── P3 (ret lineage) ──

    fn box_at(id: i64, name: &str, x: f64, y: f64) -> crate::vector::graph::McVecBox {
        use crate::vector::graph::{BoxKind, IoSummary};
        let mut b = crate::vector::graph::McVecBox::new(
            id,
            name.into(),
            "IC".into(),
            BoxKind::MultiPin,
            4,
            IoSummary::new(),
        );
        b.x = x;
        b.y = y;
        b.w = 100.0;
        b.h = 100.0;
        b
    }

    /// A box with one drawn lead on `side`: an end lands on a lead, and which face
    /// the lead sits on is what decides whether a wire can reach it without running
    /// along the box border (L5).
    fn box_with_lead(
        id: i64,
        name: &str,
        x: f64,
        y: f64,
        pin_id: i64,
        side: EntrySide,
        offset: f64,
    ) -> crate::vector::graph::McVecBox {
        let mut b = box_at(id, name, x, y);
        b.entry_points.push(crate::vector::graph::EntryPoint {
            pin_id,
            pin_name: format!("P{pin_id}"),
            side,
            offset,
        });
        b
    }

    /// **L5**: a wire approaches a lead along the lead's own axis. On a horizontal
    /// face that means the run leaves the rail at the **tip's** row -- the lead's
    /// root sits on the border, and a run at the border's row would lie along the
    /// box frame. On a vertical face the axis is the run's own row, so the landing
    /// does not move. Both faces are in this trunk: the fixture board draws only
    /// vertical-face leads, so a test with one branch would go green without ever
    /// drawing the other.
    #[test]
    fn trunk_taps_approach_each_lead_along_its_axis() {
        let mut g = crate::vector::graph::McVecGraph::new(0, "test".into());
        g.boxes.push(box_at(1, "src", 0.0, 0.0)); // driver, no lead drawn
        g.boxes
            .push(box_with_lead(2, "top", 300.0, 0.0, 21, EntrySide::Top, 0.5));
        g.boxes.push(box_with_lead(
            3,
            "bottom",
            300.0,
            200.0,
            31,
            EntrySide::Bottom,
            0.5,
        ));
        g.boxes.push(box_with_lead(
            4,
            "right",
            300.0,
            400.0,
            41,
            EntrySide::Right,
            0.5,
        ));
        let mk = |to: i64, pin: i64| {
            let mut e = edge(1, to, "V5V", EdgeKind::Power);
            e.driver_box = Some(1);
            e.to_pins = vec![pin];
            e
        };
        let plan = build_plan_for(&g, &[mk(2, 21), mk(3, 31), mk(4, 41)]);
        let taps = &plan.trunks[0].taps;
        assert_eq!(taps.len(), 3);

        // Box 2's lead points up out of its top face: the tip is one lead-length
        // above the border, and the run has to sit on the tip's row.
        assert_eq!(taps[0].root, (350.0, 0.0), "root stays on the border");
        assert_eq!(taps[0].tip, (350.0, -LEAD_STUB_LEN));
        assert_eq!(taps[0].approach_y(), -LEAD_STUB_LEN);

        // Box 3's lead points down: the tip is below the border.
        assert_eq!(taps[1].root, (350.0, 300.0));
        assert_eq!(taps[1].approach_y(), 300.0 + LEAD_STUB_LEN);

        // Box 4's lead points right, along the run: root and tip share the row.
        assert_eq!(taps[2].root, (400.0, 450.0));
        assert_eq!(
            taps[2].approach_y(),
            taps[2].root.1,
            "a side lead's run needs no extra leg"
        );

        // The rail spans the rows the runs leave it on -- the tip rows, not the
        // border rows.
        assert_eq!(plan.trunks[0].y_min, -LEAD_STUB_LEN);
        assert_eq!(plan.trunks[0].y_max, 450.0);
    }

    /// P3: a point-to-point power edge with a declared DC-pair return carries a
    /// parallel second lane (translated by the perpendicular offset, the hot
    /// lane itself untouched); an edge without a declared return carries none.
    #[test]
    fn point_to_point_power_edge_carries_ret_lane() {
        let mut g = crate::vector::graph::McVecGraph::new(0, "test".into());
        g.boxes.push(box_at(1, "src", 0.0, 0.0));
        g.boxes.push(box_at(2, "ld", 300.0, 0.0));
        let mut e = edge(1, 2, "V5V", EdgeKind::Power);
        e.driver_box = Some(1);
        e.ret = Some("GND".into());
        let plan = build_plan_for(&g, &[e]);
        assert_eq!(plan.individual.len(), 1);
        let d = &plan.individual[0];
        let lane = d.ret_lane.as_ref().expect("declared ret draws a lane");
        // Dominant horizontal: shift down by 7.
        assert_eq!(lane.from, (d.from.0, d.from.1 + 7.0));
        assert_eq!(lane.to, (d.to.0, d.to.1 + 7.0));
        assert!(!lane.ortho);

        let plain = build_plan_for(&g, &[edge(1, 2, "V5V", EdgeKind::Power)]);
        assert!(plain.individual[0].ret_lane.is_none());
    }

    /// P3: a fan-out trunk whose members carry a declared DC-pair return draws
    /// one return stub at the driver end (not one per load); a trunk without a
    /// declared return draws none.
    #[test]
    fn fan_out_trunk_carries_driver_return_stub() {
        let mut g = crate::vector::graph::McVecGraph::new(0, "test".into());
        g.boxes.push(box_at(1, "src", 0.0, 0.0));
        g.boxes.push(box_at(2, "a", 300.0, 0.0));
        g.boxes.push(box_at(3, "b", 300.0, 200.0));
        g.boxes.push(box_at(4, "c", 300.0, 400.0));
        let mk = |to: i64| {
            let mut e = edge(1, to, "V5V", EdgeKind::Power);
            e.driver_box = Some(1);
            e.ret = Some("GND".into());
            e
        };
        let edges = vec![mk(2), mk(3), mk(4)];
        let plan = build_plan_for(&g, &edges);
        assert_eq!(plan.trunks.len(), 1);
        let t = &plan.trunks[0];
        assert_eq!(t.taps.len(), 3);
        let stub = t.ret_stub.expect("fan-out draws one return stub");
        let driver = t.driver.expect("driver anchor");
        assert_eq!(stub, (driver.root.0, driver.root.1 + 10.0));

        let plain: Vec<_> = [2, 3, 4]
            .iter()
            .map(|&to| {
                let mut e = edge(1, to, "V5V", EdgeKind::Power);
                e.driver_box = Some(1);
                e
            })
            .collect();
        let plan = build_plan_for(&g, &plain);
        assert!(plan.trunks[0].ret_stub.is_none());
    }
}
