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

/// A power bundle is drawn as a shared vertical trunk with taps only once it has
/// this many members; below it, the edges are drawn individually so that short
/// pass-through stubs (e.g. a V1V2 bridge) still take part in the tap-collision
/// check in the renderer.
pub const TRUNK_MIN_CONSUMERS: usize = 3;

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
    let mut by_label: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut individual: Vec<usize> = Vec::new();

    for (i, edge) in edges.iter().enumerate() {
        if edge.kind == EdgeKind::Power && !edge.label.is_empty() {
            by_label.entry(edge.label.as_str()).or_default().push(i);
        } else {
            individual.push(i);
        }
    }

    let mut trunks: Vec<SupplyTrunk> = Vec::new();
    let mut demoted: Vec<usize> = Vec::new();
    for (label, mut members) in by_label {
        members.sort_unstable();
        if members.len() >= TRUNK_MIN_CONSUMERS {
            trunks.push(SupplyTrunk {
                label: label.to_string(),
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

/// Where the lead at `ep` meets the box border.
fn side_point(b: &McVecBox, ep: &crate::vector::graph::EntryPoint) -> (f64, f64) {
    match ep.side {
        EntrySide::Left => (b.x, b.y + ep.offset * b.h),
        EntrySide::Right => (b.x + b.w, b.y + ep.offset * b.h),
        EntrySide::Top => (b.x + ep.offset * b.w, b.y),
        EntrySide::Bottom => (b.x + ep.offset * b.w, b.y + b.h),
    }
}

/// The landing point of the box's lead for an edge end, if it has one (**L1+L4**).
///
/// `pins` are the endpoint identities of that end (ascending pin id, §5), so the
/// choice is structural: the first pin that the box actually draws a lead for.
/// This is the one authority for "where does this end attach" -- the lead's own
/// `(side, offset)`, never a re-derived facing edge.
fn lead_anchor(b: &McVecBox, pins: &[i64]) -> Option<(f64, f64)> {
    pins.iter().filter(|p| **p > 0).find_map(|p| {
        b.entry_points
            .iter()
            .find(|ep| ep.pin_id == *p)
            .map(|ep| side_point(b, ep))
    })
}

/// One end's landing point: the lead if there is one, else the old facing-edge
/// midpoint.
///
/// The fallback is not silent (§3.1): "this end has no lead" means the port is not
/// drawn at all, which is a fact worth seeing. Geometry under the fallback is
/// byte-identical to the pre-P1-b drawing, so a change in the drawing is exactly a
/// change in which ends have leads -- that is what makes the batch's gate predictive.
fn end_anchor(
    b: &McVecBox,
    pins: &[i64],
    target: (f64, f64),
    label: &str,
    end: &str,
) -> (f64, f64) {
    if let Some(p) = lead_anchor(b, pins) {
        return p;
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
    rail_anchor(b, target.0, target.1)
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
    pub driver: Option<(f64, f64)>,
    /// One landing point per consumer, already slid clear of other anchors.
    pub taps: Vec<(f64, f64)>,
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
}

/// Everything the renderer needs to draw the root layer's edges: no geometry and
/// no grouping is decided while drawing.
#[derive(Debug, Clone, Default)]
pub struct SupplyBundlePlan {
    pub trunks: Vec<TrunkDraw>,
    pub individual: Vec<IndividualDraw>,
}

/// Build the drawing plan for the root layer of `graph`.
pub fn build_plan(graph: &McVecGraph) -> SupplyBundlePlan {
    let (edges, _report) = crate::viz::layout::edge_decide::decide_edges(graph);
    build_plan_for(graph, &edges)
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
        let driver_anchor: Option<(f64, f64)> = driver_box.map(|b| {
            let cy = b.y + b.h / 2.0;
            end_anchor(b, &driver_pins, (trunk_x, cy), label, "driver")
        });

        // Consumer taps, each landing on its own lead against the resolved rail x.
        let mut taps: Vec<(f64, f64)> = Vec::new();
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

        let mut all_ys: Vec<f64> = taps.iter().map(|(_, y)| *y).collect();
        if let Some((_, dy)) = driver_anchor {
            all_ys.push(dy);
        }
        all_ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let trunk_y_min = all_ys.first().copied().unwrap_or(100.0);
        let trunk_y_max = all_ys.last().copied().unwrap_or(740.0);

        trunks.push(TrunkDraw {
            label: label.clone(),
            x: trunk_x,
            y_min: trunk_y_min,
            y_max: trunk_y_max,
            driver: driver_anchor,
            taps,
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
        let (x1, y1) = end_anchor(
            from,
            &edge.from_pins,
            (to.x + to.w / 2.0, to.y + to.h / 2.0),
            &edge.label,
            "from",
        );
        let (x2, y2) = if edge.kind == EdgeKind::Power {
            // Keep the same coordinate as the source on the axis where the boxes
            // are aligned.
            let (tx, ty) = end_anchor(
                to,
                &edge.to_pins,
                (from.x + from.w / 2.0, from.y + from.h / 2.0),
                &edge.label,
                "to",
            );
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
        };

        individual.push(IndividualDraw {
            kind: edge.kind,
            label: edge.label.clone(),
            lane_count: edge.lane_count,
            from: (x1, y1),
            to: (x2, y2),
            ortho: (x1 - x2).abs() > 1.0 && (y1 - y2).abs() > 1.0 && edge.kind == EdgeKind::Power,
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
}
