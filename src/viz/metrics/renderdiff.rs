// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ P7-1 · renderdiff —— the ruler of the render layer
//!
//! ## Positioning
//! Separate from netdiff (netlist criteria) and equally strict:
//! - netdiff governs "is it connected right" (pass2 golden)
//! - renderdiff governs "is it drawn right" (render golden: `baseline/render_golden.toml`)
//!
//! ## Criterion groups
//! - **G10 structure conservation**: box count vs golden; synth box count == 0
//!   (provenance marker); all endpoints of every net in the same route connected
//!   component (reuses `RenderedConnectivityReport`)
//! - **G11 power contract**: GND edge count == 0; rail power edge count == R-2 driver
//!   segment expectation; top-level passives == 0
//! - **G12 geometric legality**: box_box / wire_box == 0; box w/h ≥ minimum size for
//!   pin distribution (S6); no negative coordinates / off-canvas
//!
//! ## Principles
//! - Criteria are **structural similarity**, not pixel similarity; every explainable
//!   difference vs the reference figure is reported per criterion
//! - **Never edit golden to turn criteria green**. Large-scale red mid-way is the
//!   correct shape. The file is **generated** rather than written by hand
//!   ([`RenderGolden::save`], `MC_RENDER_GOLDEN_SAVE=1`), and a generating run
//!   never compares in the same run — so a regeneration cannot report green
//! - Every criterion prints its evaluated object count; 0 shows `· SKIP`,
//!   never `✓`

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::vector::graph::kinds::BoxKind;
use crate::vector::graph::{McVecGraph, NetKind};

// Golden (TOML schema)

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct RenderGolden {
    pub layer: BTreeMap<String, LayerGolden>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct LayerGolden {
    /// Module name matching pass2 golden (renderdiff's key is graph.name = instance name)
    #[serde(default)]
    pub module: String,
    pub boxes: usize,
    /// Golden-expected box roster (matched by instance name, for match/missing/extra)
    #[serde(default)]
    pub box_names: Vec<String>,
    /// Target count of Phase 1.5/1.6 synth boxes (always 0)
    #[serde(default)]
    pub synth_boxes: usize,
    /// Target count of rail flag boxes (always 0)
    #[serde(default)]
    pub rail_flags: usize,
    /// Target count of GND edges (cross-box ground nets)
    #[serde(default)]
    pub gnd_edges: usize,
    /// Target count of rail power edges (cross-box power nets, i.e. R-2 driver segments)
    #[serde(default)]
    pub power_edges: usize,
    /// Target count of top-level passives (block diagram draws no R/C; always 0, only
    /// meaningful at top level)
    #[serde(default)]
    pub top_passives: usize,
    /// Expected edge list (from/to by **box name**, label = net name or bus entry name)
    #[serde(default)]
    pub edge: Vec<GEdge>,
    /// G14: expected module-port roster (boundary port names, matched by
    /// multiset diff; empty golden roster -> SKIP)
    #[serde(default)]
    pub port_names: Vec<String>,
    /// G14: expected net-name roster (matched by multiset diff; empty golden
    /// roster -> SKIP)
    #[serde(default)]
    pub net_names: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GEdge {
    pub from: String,
    pub to: String,
    pub label: String,
}

// Reading (measured from the final graph)

#[derive(Debug, Clone, Serialize)]
pub struct LayerReading {
    pub layer: String,
    pub bid: i64,
    /// boxes total
    pub total_boxes: usize,
    pub declared_boxes: usize,
    pub synth_endpoint_boxes: usize,
    pub rail_flag_boxes: usize,
    /// Box roster (declared boxes + synth boxes, all listed for diff)
    pub box_names: Vec<String>,
    /// Cross-box ground net count (= drawn GND edges)
    pub gnd_edges: usize,
    /// Cross-box power net count
    pub power_edges: usize,
    /// TwoPin passive box count
    pub two_pin_passives: usize,
    /// ★ P7-3 S1: ground symbol decoration count (sub-layer = GND endpoint count; always 0 at top)
    #[serde(default)]
    pub decorations_ground: usize,
    /// ★ P7-3 S2: rail dot decoration count (sub-layer = non-GND rail endpoint count; always 0 at
    /// top)
    #[serde(default)]
    pub decorations_power: usize,
    /// ★ P7-4: geometric double-write count of this layer (collected via stage-boundary snapshot
    /// diff; target 0)
    #[serde(default)]
    pub geom_double_writes: usize,
    /// ★ P7-4c: full double-write detail (box / earlier writer → later writer), for baseline
    /// diagnosis;
    /// should converge to empty after P7-4e merges writers by stage.
    #[serde(default)]
    pub geom_double_write_list: Vec<String>,
    /// (from,to,label) list of cross-box nets (unordered pairs)
    pub edges: Vec<(String, String, String)>,
    // G12
    pub box_box: usize,
    pub wire_box: usize,
    pub s6_violations: usize,
    pub offcanvas_boxes: usize,
    // G14 semantic rosters
    /// Module-port names drawn behind this layer's box boundaries
    #[serde(default)]
    pub port_names: Vec<String>,
    /// Net names present on this layer
    #[serde(default)]
    pub net_names: Vec<String>,
    // G10 connectivity (injected by the caller from RenderedConnectivityReport)
    pub pins_total: usize,
    pub pins_unreachable: usize,
    /// Self-check: how many objects each criterion evaluated
    pub evaluated: EvalCounts,
    /// ★ P7-5: device-contract readings (S3~S9)
    #[serde(default)]
    pub g13: G13Reading,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct EvalCounts {
    pub boxes: usize,
    pub nets: usize,
    pub sizes: usize,
}

/// ★ P7-5 · G13 device-contract readings (S3~S9, measured on the final
/// graph — no pixel comparison). Each counter is `ok/total`; S7 is a plain
/// overlap count. Total == 0 means "nothing to judge" (shown as SKIP).
#[derive(Debug, Clone, Default, Serialize)]
pub struct G13Reading {
    /// S3 decoupling caps glued to their pin: vertical, pin1 up toward the
    /// rail, pin2 down toward the GND symbol, |Δx| vs the anchor pin ≤ 1 grid.
    pub s3_decouple_ok: usize,
    pub s3_decouple_total: usize,
    /// S4 passives with a GND end standing vertically.
    pub s4_gnd_vertical_ok: usize,
    pub s4_gnd_total: usize,
    /// S4 series-chain members sharing the chain's orientation.
    pub s4_chain_aligned_ok: usize,
    pub s4_chain_total: usize,
    /// S5 transposed (`'`) devices drawn as a vertical rung between two lanes:
    /// vertical box, pins on Top/Bottom, and the rung must not cut any other
    /// net's wire.
    pub s5_rung_ok: usize,
    pub s5_rung_total: usize,
    /// Names of the rung devices that passed S5 (specimen assertions:
    /// mic C1 / speaker C8 / mcu R1).
    #[serde(default)]
    pub s5_rung_ok_names: Vec<String>,
    /// S7 label rectangles intersecting any box or wire segment.
    pub s7_label_overlaps: usize,
    pub s7_labels_total: usize,
    /// S8 NC devices whose text carries the NC_ prefix.
    pub s8_nc_ok: usize,
    pub s8_nc_total: usize,
    /// S9 cross-module nets terminating on a labeled stub box (not a dangling
    /// wire). Baseline-only reading for now.
    pub s9_stub_ok: usize,
    pub s9_stub_total: usize,
}

impl LayerReading {
    /// Measure all readings of one layer from the final graph, after route and before render.
    ///
    /// * `col` —— collision report from `audit_all` (G12)
    /// * `conn` —— optional connectivity report (pins_total, pins_unreachable) (G10 item 3).
    ///   When `None` that criterion shows `· SKIP`.
    pub fn measure(
        graph: &McVecGraph,
        col: &crate::viz::route::audit::CollisionReport,
        conn: Option<(usize, usize)>,
    ) -> Self {
        use crate::vector::graph::boxdef::BoxProvenance as P;

        let mut declared = 0usize;
        let mut synth = 0usize;
        let mut flags = 0usize;
        let mut passives = 0usize;
        let mut s6 = 0usize;
        let mut offcanvas = 0usize;
        let mut names = Vec::new();

        for b in &graph.boxes {
            match b.provenance {
                P::Declared => declared += 1,
                P::SynthesizedFromEndpoint => synth += 1,
                P::SynthesizedRailFlag => flags += 1,
            }
            if matches!(b.kind, BoxKind::TwoPin) {
                passives += 1;
            }
            names.push(if b.name.is_empty() {
                format!("{}#", b.id)
            } else {
                b.name.clone()
            });

            // S6: does the box fit its own pin distribution (reuses size::box_size's minimum size
            // formula)
            let (mw, mh) = crate::viz::layout::size::box_size(b);
            if b.w + 1.0 < mw || b.h + 1.0 < mh {
                s6 += 1;
            }
            if b.x < -0.5 || b.y < -0.5 {
                offcanvas += 1;
            }
        }

        // G11: cross-box net classification (endpoints of the same net in ≥2 distinct boxes = an
        // edge was drawn)
        let name_of = |id: i64| -> String {
            graph
                .boxes
                .iter()
                .find(|b| b.id == id)
                .map(|b| {
                    if b.name.is_empty() {
                        format!("{}#", b.id)
                    } else {
                        b.name.clone()
                    }
                })
                .unwrap_or_else(|| format!("{}#", id))
        };
        let mut gnd = 0usize;
        let mut pwr = 0usize;
        let mut edges = Vec::new();
        for net in &graph.nets {
            let mut distinct: Vec<i64> = Vec::new();
            for ep in &net.endpoints {
                if !distinct.contains(&ep.box_id) {
                    distinct.push(ep.box_id);
                }
            }
            if distinct.len() < 2 {
                continue;
            }
            match net.kind {
                NetKind::Ground => gnd += 1,
                NetKind::Power => pwr += 1,
                _ => {}
            }
            edges.push((name_of(distinct[0]), name_of(distinct[1]), net.name.clone()));
        }

        let (pins_total, pins_unreachable) = conn.unwrap_or((0, 0));

        // ★ P7-3: S1/S2 readings —— pin decoration counts (ground symbols / rail dots).
        // Always 0 for top-level R-1/R-3 (block diagram places no symbols); sub-layer =
        // endpoint count judged "in-place symbol" by R-1/R-3.
        let (dec_ground, dec_power) = {
            let mut g = 0usize;
            let mut p = 0usize;
            for d in &graph.rail_decorations {
                if d.is_ground {
                    g += 1;
                } else {
                    p += 1;
                }
            }
            (g, p)
        };

        // G14 semantic rosters: module-port names behind box boundaries + net names.
        let port_names: Vec<String> = graph
            .boxes
            .iter()
            .flat_map(|b| b.boundary_ports.iter().map(|p| p.port_name.clone()))
            .collect();
        let net_names: Vec<String> = graph.nets.iter().map(|n| n.name.clone()).collect();

        LayerReading {
            layer: graph.name.clone(),
            bid: graph.bid,
            total_boxes: graph.boxes.len(),
            declared_boxes: declared,
            synth_endpoint_boxes: synth,
            rail_flag_boxes: flags,
            box_names: names,
            gnd_edges: gnd,
            power_edges: pwr,
            two_pin_passives: passives,
            edges,
            decorations_ground: dec_ground,
            decorations_power: dec_power,
            geom_double_writes: graph.geom_double_writes.len(),
            geom_double_write_list: graph
                .geom_double_writes
                .iter()
                .map(|d| {
                    format!(
                        "{}#{}: {} -> {} [{}]",
                        d.box_name,
                        d.box_id,
                        d.prev_writer,
                        d.cur_writer,
                        d.dims.join("+")
                    )
                })
                .collect(),
            box_box: col.box_box,
            wire_box: col.wire_box,
            s6_violations: s6,
            offcanvas_boxes: offcanvas,
            port_names,
            net_names,
            pins_total,
            pins_unreachable,
            evaluated: EvalCounts {
                boxes: graph.boxes.len(),
                nets: graph.nets.len(),
                sizes: graph.boxes.len(),
            },
            g13: measure_device_contracts(graph),
        }
    }
}

// ★ P7-5 · G13 device-contract measurement (S3~S9)

fn measure_device_contracts(graph: &McVecGraph) -> G13Reading {
    use crate::vector::graph::EntrySide;
    use crate::vector::graph::{NetKind, VisualRole};

    let mut r = G13Reading::default();

    // Pin-level rail role: P7-3 rail trichotomy consumes GND/power nets into
    // `rail_decorations`, so a passive's GND/rail end often has no net left.
    // G13 therefore reads rail roles from decorations (falling back to net
    // kinds only when no decoration exists).
    let dec_of =
        |box_id: i64, pin_id: i64| -> Option<&crate::vector::graph::graphdef::RailDecoration> {
            graph
                .rail_decorations
                .iter()
                .find(|d| d.box_id == box_id && d.pin_id == pin_id)
        };
    // For a box pin: kinds of the nets it touches (usually 0..1 for a 2-pin).
    let pin_net_kinds = |b: &crate::vector::graph::McVecBox, pin_id: i64| -> Vec<NetKind> {
        graph
            .nets
            .iter()
            .filter(|n| {
                n.endpoints
                    .iter()
                    .any(|e| e.box_id == b.id && e.pin_id == pin_id)
            })
            .map(|n| n.kind.clone())
            .collect()
    };

    // x position of a box pin (Top/Bottom → x along width; Left/Right → side x).
    let pin_x = |b: &crate::vector::graph::McVecBox, pin_id: i64| -> Option<f64> {
        b.find_entry(pin_id).map(|e| match e.side {
            EntrySide::Left => b.x,
            EntrySide::Right => b.x + b.w,
            _ => b.x + b.w * e.offset,
        })
    };

    // ★ S3 anchor table, shared with the layout pass (passive_inline::
    // stand_grounded_passives): rail label → x of the fed pin on non-TwoPin
    // boxes. Sources: (a) rail decorations on non-TwoPin boxes (the IC pin the
    // rail feeds), (b) Power nets named exactly like the label feeding a
    // non-TwoPin box (driver pins carry no decoration — R-2 draws an edge).
    let rail_anchors: std::collections::HashMap<String, Vec<f64>> = {
        use std::collections::HashMap;
        let mut m: HashMap<String, Vec<f64>> = HashMap::new();
        for d in graph.rail_decorations.iter().filter(|d| !d.is_ground) {
            let Some(ab) = graph.boxes.iter().find(|b| b.id == d.box_id) else {
                continue;
            };
            if matches!(ab.kind, BoxKind::TwoPin) {
                continue;
            }
            if let Some(ax) = pin_x(ab, d.pin_id) {
                m.entry(d.label.clone()).or_default().push(ax);
            }
        }
        for n in graph
            .nets
            .iter()
            .filter(|n| matches!(n.kind, NetKind::Power))
        {
            if m.contains_key(&n.name) {
                continue;
            }
            let mut xs = Vec::new();
            for e in &n.endpoints {
                let Some(ab) = graph.boxes.iter().find(|b| b.id == e.box_id) else {
                    continue;
                };
                if matches!(ab.kind, BoxKind::TwoPin) {
                    continue;
                }
                if let Some(ax) = pin_x(ab, e.pin_id) {
                    xs.push(ax);
                }
            }
            if !xs.is_empty() {
                m.insert(n.name.clone(), xs);
            }
        }
        m
    };

    // Wire segments of every routed net, tagged with nid (S5 "no cutting" /
    // S7 overlap). S5 skips the nets ending on the tested box — those
    // legitimately touch the rung's own pins.
    let segments: Vec<(i64, f64, f64, f64, f64)> = graph
        .nets
        .iter()
        .filter_map(|n| n.route.as_ref().map(|rt| (n, rt)))
        .flat_map(|(n, rt)| {
            rt.segments
                .iter()
                .map(move |s| (n.nid, s.from.x, s.from.y, s.to.x, s.to.y))
                .collect::<Vec<_>>()
        })
        .collect();
    // nid → endpoint box ids (for S5 self-net exclusion).
    let net_boxes: std::collections::HashMap<i64, Vec<i64>> = graph
        .nets
        .iter()
        .map(|n| (n.nid, n.endpoints.iter().map(|e| e.box_id).collect()))
        .collect();

    // (box_id, pin_id) → has rail decoration at all (either polarity). Used to
    // exclude rail-ended passives from S4b chains: they obey S3/S4a instead.
    let dec_pinned: std::collections::HashSet<(i64, i64)> = graph
        .rail_decorations
        .iter()
        .map(|d| (d.box_id, d.pin_id))
        .collect();

    for b in &graph.boxes {
        if !matches!(b.kind, BoxKind::TwoPin) || b.entry_points.len() != 2 {
            continue;
        }
        let (e0, e1) = (&b.entry_points[0], &b.entry_points[1]);
        let d0 = dec_of(b.id, e0.pin_id);
        let d1 = dec_of(b.id, e1.pin_id);
        let vertical = b.h > b.w;
        // Pin rail role: decoration first (is_ground), else net kind.
        let is_gnd_end = |ep_idx: usize| -> bool {
            let ep = if ep_idx == 0 { e0 } else { e1 };
            match if ep_idx == 0 { d0 } else { d1 } {
                Some(d) => d.is_ground,
                None => {
                    let ks = pin_net_kinds(b, ep.pin_id);
                    ks.iter().any(|k| matches!(k, NetKind::Ground))
                }
            }
        };
        let is_rail_end = |ep_idx: usize| -> bool {
            let ep = if ep_idx == 0 { e0 } else { e1 };
            match if ep_idx == 0 { d0 } else { d1 } {
                Some(d) => !d.is_ground,
                None => {
                    let ks = pin_net_kinds(b, ep.pin_id);
                    ks.iter().any(|k| matches!(k, NetKind::Power))
                }
            }
        };

        // ── S3: decoupling cap (Capacitor, one rail end + one GND end) ──
        let is_cap = matches!(
            b.symbol,
            crate::vector::graph::Symbol::Capacitor | crate::vector::graph::Symbol::PolarCapacitor
        );
        if is_cap {
            let g0 = is_gnd_end(0);
            let g1 = is_gnd_end(1);
            let p0 = is_rail_end(0);
            let p1 = is_rail_end(1);
            if (g0 && p1) || (g1 && p0) {
                let rail_idx = if p0 && g1 { 0 } else { 1 };
                let rail_ep = if rail_idx == 0 { e0 } else { e1 };
                let rail_label = if rail_idx == 0 { d0 } else { d1 }
                    .map(|d| d.label.clone())
                    .unwrap_or_default();
                // N/A rule: the rail has no local fed pin at all (R-1 rail
                // without a driver — e.g. dcdc's VCC_1V2, whose only ends
                // are passive terminals). "Glue to the pin" cannot apply;
                // the cap is judged by S4a only. Not counted in the total.
                // (Fall through — S4a/S5 below still apply to this box.)
                match rail_anchors.get(&rail_label) {
                    None => crate::vlog!(
                        "[s3diag] layer='{}' cap='{}' rail='{}': no local anchor → N/A",
                        graph.name,
                        b.name,
                        rail_label
                    ),
                    Some(anchor_x) => {
                        r.s3_decouple_total += 1;
                        // Glue = |Δx| between cap center and the nearest fed pin.
                        let aligned = anchor_x
                            .iter()
                            .any(|&ax| (b.x + b.w / 2.0 - ax).abs() <= b.w.max(1.0));
                        let rail_up = matches!(rail_ep.side, EntrySide::Top);
                        if vertical && rail_up && aligned {
                            r.s3_decouple_ok += 1;
                        } else {
                            crate::vlog!(
                        "[s3diag] layer='{}' cap='{}' v={vertical} rail_up={rail_up} aligned={aligned} anchors={:?} capcx={:.1} w={:.1}",
                        graph.name,
                        b.name,
                        anchor_x,
                        b.x + b.w / 2.0,
                        b.w
                    );
                        }
                    }
                }
            }
        }

        // ── S4a: passive with one GND end stands vertically ──
        let (g0, g1) = (is_gnd_end(0), is_gnd_end(1));
        if g0 != g1 {
            r.s4_gnd_total += 1;
            if vertical {
                r.s4_gnd_vertical_ok += 1;
            }
        }

        // ── S5: transposed device (`'`) as a vertical rung ──
        // Rung shape = vertical box with pins on Top/Bottom; the rung must
        // not cut any other net's wire (segments may only touch its own ends).
        if matches!(b.visual_role, Some(VisualRole::BridgePassive)) {
            r.s5_rung_total += 1;
            let pins_top_bottom =
                matches!(e0.side, EntrySide::Top) && matches!(e1.side, EntrySide::Bottom);
            // A wire cuts the rung when a segment of a net NOT ending on this
            // box intersects the box rectangle.
            let cuts_net = segments.iter().any(|&(nid, x1, y1, x2, y2)| {
                let own = net_boxes
                    .get(&nid)
                    .map(|bs| bs.contains(&b.id))
                    .unwrap_or(false);
                !own && seg_meets_rect(x1, y1, x2, y2, b.x, b.y, b.x + b.w, b.y + b.h)
            });
            if vertical && pins_top_bottom && !cuts_net {
                r.s5_rung_ok += 1;
                r.s5_rung_ok_names.push(b.name.clone());
            } else {
                crate::vlog!(
                    "[s5diag] layer='{}' rung='{}' v={vertical} pins_tb={pins_top_bottom} cuts_net={cuts_net}",
                    graph.name,
                    b.name
                );
            }
        }
    }

    // ── S4b: series-chain members share the chain orientation ──
    // Chain = 2-pin passives connected through shared signal-class nets
    // (transitive). "Signal-class" = anything but Power/Ground: the promote
    // pass rewrites inter-box Signal nets to SubModuleIO, so filtering on
    // Signal alone would evaluate nothing.
    // Passives with a rail/GND-decorated end are NOT chain members: they obey
    // S3/S4a (vertical, glued to the rail), which legitimately differs from a
    // horizontal series chain they happen to touch. Same for transposed
    // devices (`'`): their vertical rung shape is the S5 contract.
    {
        use std::collections::{HashMap, HashSet};
        let passive_ids: HashSet<i64> = graph
            .boxes
            .iter()
            .filter(|b| {
                matches!(b.kind, BoxKind::TwoPin)
                    && b.entry_points.len() == 2
                    && !matches!(b.visual_role, Some(VisualRole::BridgePassive))
                    && !dec_pinned.contains(&(b.id, b.entry_points[0].pin_id))
                    && !dec_pinned.contains(&(b.id, b.entry_points[1].pin_id))
            })
            .map(|b| b.id)
            .collect();
        // signal-class net → passive members on it
        let mut link: HashMap<i64, Vec<i64>> = HashMap::new();
        for n in &graph.nets {
            if matches!(n.kind, NetKind::Power | NetKind::Ground) {
                continue;
            }
            let members: Vec<i64> = n
                .endpoints
                .iter()
                .filter(|e| passive_ids.contains(&e.box_id))
                .map(|e| e.box_id)
                .collect();
            if members.len() >= 2 {
                for &m in &members {
                    link.entry(m)
                        .or_default()
                        .extend(members.iter().filter(|&&x| x != m));
                }
            }
        }
        // connected components over `link`
        let mut visited: HashSet<i64> = HashSet::new();
        let by_id: HashMap<i64, &crate::vector::graph::McVecBox> =
            graph.boxes.iter().map(|b| (b.id, b)).collect();
        for start in passive_ids.iter().copied().collect::<Vec<_>>() {
            if visited.contains(&start) || !link.contains_key(&start) {
                continue;
            }
            let mut comp = Vec::new();
            let mut stack = vec![start];
            while let Some(id) = stack.pop() {
                if visited.contains(&id) {
                    continue;
                }
                visited.insert(id);
                comp.push(id);
                if let Some(nbrs) = link.get(&id) {
                    for &n in nbrs {
                        if passive_ids.contains(&n) && !visited.contains(&n) {
                            stack.push(n);
                        }
                    }
                }
            }
            if comp.len() < 2 {
                continue;
            }
            let verts = comp
                .iter()
                .filter(|id| by_id.get(*id).map_or(false, |b| b.h > b.w))
                .count();
            let majority_vertical = verts * 2 > comp.len();
            for id in &comp {
                r.s4_chain_total += 1;
                let is_v = by_id.get(id).map_or(false, |b| b.h > b.w);
                if is_v == majority_vertical {
                    r.s4_chain_aligned_ok += 1;
                } else {
                    crate::vlog!(
                        "[s4bdiag] layer='{}' chain member '{}' is_v={is_v} majority_v={majority_vertical} chain={:?}",
                        graph.name,
                        by_id.get(id).map(|b| b.name.clone()).unwrap_or_default(),
                        comp.iter()
                            .map(|c| by_id.get(c).map(|b| b.name.clone()).unwrap_or_default())
                            .collect::<Vec<_>>()
                    );
                }
            }
        }
    }

    // ── S7: label rectangles vs boxes / wire segments ──
    {
        let seg4: Vec<(f64, f64, f64, f64)> = segments
            .iter()
            .map(|&(_, x1, y1, x2, y2)| (x1, y1, x2, y2))
            .collect();
        for b in &graph.boxes {
            for lp in &b.label_placements {
                r.s7_labels_total += 1;
                let (lx1, ly1, lx2, ly2) = (lp.x, lp.y, lp.x + lp.w.max(1.0), lp.y + lp.h.max(1.0));
                let mut hit = false;
                for ob in &graph.boxes {
                    if ob.id == b.id {
                        continue;
                    }
                    if lx1 < ob.x + ob.w && lx2 > ob.x && ly1 < ob.y + ob.h && ly2 > ob.y {
                        hit = true;
                        crate::vlog!(
                            "[s7diag] layer='{}' label of '{}' overlaps box '{}'",
                            graph.name,
                            b.name,
                            ob.name
                        );
                        break;
                    }
                }
                if !hit {
                    for &(x1, y1, x2, y2) in &seg4 {
                        if seg_meets_rect(x1, y1, x2, y2, lx1, ly1, lx2, ly2) {
                            hit = true;
                            crate::vlog!(
                                "[s7diag] layer='{}' label of '{}' overlaps a wire segment",
                                graph.name,
                                b.name
                            );
                            break;
                        }
                    }
                }
                if hit {
                    r.s7_label_overlaps += 1;
                }
            }
        }
    }

    // ── S8: NC devices carry the NC_ prefix and a distinct style ──
    for b in &graph.boxes {
        if b.not_fitted {
            r.s8_nc_total += 1;
            // The M8 label pipeline prefixes the designator text of
            // not-fitted devices ("NC_R442"); accept either the placed text
            // or (fallback path) the box's own label fields.
            let ok = b
                .label_placements
                .iter()
                .any(|lp| lp.text.starts_with("NC_"))
                || b.display_label().contains("NC_");
            if ok {
                r.s8_nc_ok += 1;
            } else {
                crate::vlog!(
                    "[s8diag] layer='{}' NC device '{}' carries no NC_ prefix (designator={:?}, value={:?}, placements={:?}, symbol={:?})",
                    graph.name,
                    b.name,
                    b.designator,
                    b.value,
                    b.label_placements.iter().map(|lp| lp.text.clone()).collect::<Vec<_>>(),
                    b.symbol,
                );
            }
        }
    }

    // ── S9: cross-module signal nets terminate on a stub, not a dangling wire ──
    // The projection pass removes module-boundary pseudo-endpoints; a
    // signal-class net left with exactly ONE box endpoint must still render a
    // labeled stub — a named SubModuleIO boundary draws a Port stub, a
    // power/ground rail draws its bus label / ground symbol. Green requires
    // every single-endpoint net to render its stub, so the count of still-
    // dangling (anonymous / bare-signal) nets must be 0. The renders-stub
    // predicate is shared with the F5 skip in equipotential_tree so the two
    // cannot drift apart.
    for n in &graph.nets {
        if !matches!(n.kind, NetKind::Signal | NetKind::SubModuleIO) {
            continue;
        }
        if n.endpoints.len() != 1 {
            continue;
        }
        r.s9_stub_total += 1;
        let e = &n.endpoints[0];
        let on_label = graph
            .boxes
            .iter()
            .find(|b| b.id == e.box_id)
            .map_or(false, |b| {
                matches!(b.kind, BoxKind::PowerLabel | BoxKind::Dot)
            });
        if on_label || crate::viz::layout::equipotential_tree::single_group_net_renders_stub(n) {
            r.s9_stub_ok += 1;
        } else {
            let bname = graph
                .boxes
                .iter()
                .find(|b| b.id == e.box_id)
                .map(|b| format!("{} ({}:{:?})", b.name, b.kind, b.symbol))
                .unwrap_or_default();
            crate::vlog!(
                "[s9diag] layer='{}' net '{}' dangles at endpoint {:?} kind={:?} box={}",
                graph.name,
                n.name,
                (e.box_id, e.pin_id),
                n.kind,
                bname,
            );
        }
    }

    r
}

/// Axis-aligned segment vs rect intersection (segments are axis-aligned here).
fn seg_meets_rect(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    rx1: f64,
    ry1: f64,
    rx2: f64,
    ry2: f64,
) -> bool {
    // horizontal segment
    if (y1 - y2).abs() < 0.5 {
        let (xa, xb) = if x1 < x2 { (x1, x2) } else { (x2, x1) };
        return y1 >= ry1 && y1 <= ry2 && xb >= rx1 && xa <= rx2;
    }
    // vertical segment
    if (x1 - x2).abs() < 0.5 {
        let (ya, yb) = if y1 < y2 { (y1, y2) } else { (y2, y1) };
        return x1 >= rx1 && x1 <= rx2 && yb >= ry1 && ya <= ry2;
    }
    // diagonal: sample both endpoints inside rect (coarse)
    let inside = |x: f64, y: f64| x >= rx1 && x <= rx2 && y >= ry1 && y <= ry2;
    inside(x1, y1) || inside(x2, y2)
}

// Diff (reading vs golden)

/// Conclusion of one criterion. `Skip` = zero evaluated objects, never counts as
/// green.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    Ok(String),
    Fail(String),
    Skip(String),
}

#[derive(Debug, Clone)]
pub struct LayerDiff {
    pub layer: String,
    /// Per-criterion conclusions of G10/G11/G12
    pub findings: Vec<(String, Verdict)>,
    /// Red count (Fail)
    pub red: usize,
    /// Green count (Ok)
    pub green: usize,
    /// Skip count
    pub skipped: usize,
}

impl LayerDiff {
    pub fn report_line(&self) -> String {
        let head = format!(
            "[renderdiff] layer '{}': {} red / {} green / {} skip",
            self.layer, self.red, self.green, self.skipped
        );
        let body: Vec<String> = self
            .findings
            .iter()
            .map(|(id, v)| {
                let mark = match v {
                    Verdict::Ok(_) => "✓",
                    Verdict::Fail(_) => "✗",
                    Verdict::Skip(_) => "·",
                };
                let msg = match v {
                    Verdict::Ok(m) | Verdict::Fail(m) | Verdict::Skip(m) => m,
                };
                format!("{mark} {id}: {msg}")
            })
            .collect();
        if body.is_empty() {
            head
        } else {
            format!("{head}\n  {}", body.join("\n  "))
        }
    }
}

fn sorted_lower(names: &[String]) -> Vec<String> {
    let mut v: Vec<String> = names.iter().map(|s| s.to_lowercase()).collect();
    v.sort();
    v
}

/// Multiset diff: returns (missing, extra)
fn multiset_diff(expected: &[String], actual: &[String]) -> (Vec<String>, Vec<String>) {
    use std::collections::BTreeMap;
    let mut exp: BTreeMap<&str, i32> = BTreeMap::new();
    let mut act: BTreeMap<&str, i32> = BTreeMap::new();
    for s in expected {
        *exp.entry(s.as_str()).or_insert(0) += 1;
    }
    for s in actual {
        *act.entry(s.as_str()).or_insert(0) += 1;
    }
    let mut missing = Vec::new();
    let mut extra = Vec::new();
    for (k, e) in &exp {
        let a = act.get(k).copied().unwrap_or(0);
        if a < *e {
            for _ in 0..(e - a) {
                missing.push(k.to_string());
            }
        }
    }
    for (k, a) in &act {
        let e = exp.get(k).copied().unwrap_or(0);
        if a > &e {
            for _ in 0..(a - e) {
                extra.push(k.to_string());
            }
        }
    }
    (missing, extra)
}

/// Edge matching key: (unordered endpoint name pair, label).
fn edge_key(e: &(String, String, String)) -> (String, String, String) {
    let (a, b) = if e.0 <= e.1 {
        (&e.0, &e.1)
    } else {
        (&e.1, &e.0)
    };
    (a.clone(), b.clone(), e.2.clone())
}

/// Header of a generated baseline. TOML comments survive `load`, so this is where
/// whoever reads (or regenerates) the file is told that it is generated, what to
/// run to regenerate it, and how to read the lines written as 0.
const GOLDEN_HEADER: &str = "\
# render_golden.toml — the render layer's expectation, read by
# src/viz/metrics/renderdiff.rs. Full explanation in that module's docs.
#
# GENERATED, not hand-written. Regenerate from the project root with
#     MC_RENDER_GOLDEN_SAVE=1 mcc build --viz -L
# The generating run judges nothing: writing and comparing are deliberately
# exclusive, so a regeneration can never itself report green.
#
# Two kinds of line, and the difference is the whole design:
#
#   observed (boxes / box_names / power_edges / edge / port_names / net_names)
#     frozen from the run that wrote the file. These criteria are a regression
#     lock on the renderer's current output — not a claim that it is correct.
#   invariant (synth_boxes / rail_flags / gnd_edges / top_passives)
#     written as 0: the target, never the measurement. Freezing a measured
#     non-zero would bless the very defect the criterion exists to catch, so
#     these stay red until the renderer is fixed. A generating run prints any
#     invariant it measured non-zero.
#
# Criteria with a hardcoded zero target (G12.*, G13.S7) read no golden field at
# all, and keep their signal whatever this file says.

";

/// The on/off reading of [`RenderGolden::save_requested`]'s switch, split out so
/// it can be tested without mutating the process environment (other tests in this
/// binary render, and a momentarily-set switch would have them write a baseline).
fn save_flag_set(value: Option<&str>) -> bool {
    match value {
        Some(v) => {
            let t = v.trim();
            !(t.is_empty() || t == "0" || t == "false" || t == "False" || t == "FALSE")
        }
        None => false,
    }
}

impl LayerGolden {
    /// One layer's golden, derived from one run's reading.
    ///
    /// The fields are written two different ways, on purpose:
    ///
    /// - **Observed** (`boxes`, `box_names`, `power_edges`, `edge`, `port_names`,
    ///   `net_names`, `module`): frozen from the reading. That is what a baseline
    ///   is — the figure a later run is compared against, so a changed count or
    ///   roster shows up as red on the next run.
    /// - **Invariant** (`synth_boxes`, `rail_flags`, `gnd_edges`, `top_passives`):
    ///   written as the target **0**, never the measurement. Freezing a measured
    ///   `3` here would bless the very defect the criterion exists to catch, and
    ///   `G10.synth` / `G10.flags` / `G11.gnd_edges` / `G11.top_passives` would
    ///   read green for ever. A reading that violates one is named by
    ///   [`RenderGolden::invariant_violations`] instead, and the criterion stays
    ///   red until the renderer is fixed.
    ///
    /// Rosters and edges are sorted, so two runs of the same build write the same
    /// bytes. Every comparison is order-insensitive (`multiset_diff`,
    /// `multiset_diff_str3`, `edge_key`'s unordered pair), so sorting cannot
    /// change a verdict.
    pub fn from_reading(r: &LayerReading) -> Self {
        let mut box_names = r.box_names.clone();
        box_names.sort_by_key(|s| s.to_lowercase());
        let mut port_names = r.port_names.clone();
        port_names.sort_by_key(|s| s.to_lowercase());
        let mut net_names = r.net_names.clone();
        net_names.sort_by_key(|s| s.to_lowercase());
        let mut edge: Vec<GEdge> = r
            .edges
            .iter()
            .map(|(from, to, label)| {
                // Stored in the comparison's own canonical form (`edge_key`: the
                // endpoint pair ordered). `diff_layer` applies `edge_key` to the
                // reading's edges but not to the golden's, so a golden holding the
                // raw direction would read red against the very reading it was
                // written from whenever the renderer happened to emit the pair the
                // other way round.
                let (a, b, label) = edge_key(&(from.clone(), to.clone(), label.clone()));
                GEdge {
                    from: a,
                    to: b,
                    label,
                }
            })
            .collect();
        edge.sort_by(|a, b| (&a.from, &a.to, &a.label).cmp(&(&b.from, &b.to, &b.label)));

        Self {
            // The reading's own key: `LayerReading::layer` is the graph name, the
            // same string `diff_layer` looks the layer up by.
            module: r.layer.clone(),
            boxes: r.total_boxes,
            box_names,
            synth_boxes: 0,
            rail_flags: 0,
            gnd_edges: 0,
            power_edges: r.power_edges,
            top_passives: 0,
            edge,
            port_names,
            net_names,
        }
    }
}

impl RenderGolden {
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("render_golden.toml read failed: {e}"))?;
        toml::from_str(&text).map_err(|e| format!("render_golden.toml parse failed: {e}"))
    }

    /// A whole baseline from one run's readings.
    ///
    /// Readings arrive in render order; keying them by layer name here (into a
    /// `BTreeMap`) means the file does not depend on the order the layers
    /// happened to be rendered in.
    pub fn from_readings(readings: &[LayerReading]) -> Self {
        let mut layer = BTreeMap::new();
        for r in readings {
            layer.insert(r.layer.clone(), LayerGolden::from_reading(r));
        }
        Self { layer }
    }

    /// Whether this run was asked to regenerate the baseline rather than compare
    /// against it: `MC_RENDER_GOLDEN_SAVE`, the same idiom as `UPDATE_EXPECT=1` in
    /// `scripts/regress.sh`. The path written is whichever
    /// [`crate::viz::api::renderdiff_report`] would have read (`MC_RENDER_GOLDEN`,
    /// else `baseline/render_golden.toml`), so generate and compare target the
    /// same file by construction.
    pub fn save_requested() -> bool {
        save_flag_set(std::env::var("MC_RENDER_GOLDEN_SAVE").ok().as_deref())
    }

    /// Write the baseline to `path`.
    ///
    /// The parent directory is created: `baseline/` is gitignored, so on a fresh
    /// clone it does not exist and the first save would otherwise have nowhere to
    /// land.
    pub fn save(&self, path: &std::path::Path) -> Result<(), String> {
        let body = toml::to_string_pretty(self)
            .map_err(|e| format!("render_golden.toml serialize failed: {e}"))?;
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("render_golden.toml dir create failed: {e}"))?;
            }
        }
        std::fs::write(path, format!("{GOLDEN_HEADER}{body}"))
            .map_err(|e| format!("render_golden.toml write failed: {e}"))
    }

    /// The design invariants this run's readings violate — exactly the fields
    /// [`LayerGolden::from_reading`] records as 0 rather than as measured. Printed
    /// by a generating run, so whoever regenerates sees that the file is red by
    /// construction and which criterion will say so.
    pub fn invariant_violations(readings: &[LayerReading]) -> Vec<String> {
        let mut out = Vec::new();
        for r in readings {
            for (id, what, measured) in [
                ("G10.synth", "synth endpoint boxes", r.synth_endpoint_boxes),
                ("G10.flags", "rail flag boxes", r.rail_flag_boxes),
                ("G11.gnd_edges", "GND edges", r.gnd_edges),
            ] {
                if measured != 0 {
                    out.push(format!(
                        "layer '{}': {what} measured {measured}, golden records 0 — {id} reads red until the renderer is fixed",
                        r.layer
                    ));
                }
            }
            // G11.top_passives is judged at the top level only, mirroring
            // `diff_layer`'s condition (golden 0 + layer "main").
            if r.layer == "main" && r.two_pin_passives != 0 {
                out.push(format!(
                    "layer '{}': top-level passives measured {}, golden records 0 — G11.top_passives reads red until the renderer is fixed",
                    r.layer, r.two_pin_passives
                ));
            }
        }
        out
    }

    /// One layer reading vs one layer golden, outputs per-criterion conclusions.
    pub fn diff_layer(&self, r: &LayerReading) -> LayerDiff {
        let g = match self.layer.get(&r.layer) {
            Some(g) => g,
            None => {
                let mut findings = vec![(
                    "G10".to_string(),
                    Verdict::Fail(format!(
                        "layer '{}' not in golden (boxes={}) — golden needs a new entry or the layer name is wrong",
                        r.layer, r.total_boxes
                    )),
                )];
                findings.push((
                    "G12".to_string(),
                    Verdict::Skip(format!(
                        "no golden for layer, collisions box_box={} wire_box={} unjudged",
                        r.box_box, r.wire_box
                    )),
                ));
                return LayerDiff {
                    layer: r.layer.clone(),
                    findings,
                    red: 1,
                    green: 0,
                    skipped: 1,
                };
            }
        };

        let mut findings: Vec<(String, Verdict)> = Vec::new();

        // G10 structure conservation
        // (1) box count
        findings.push(num_check(
            "G10.boxes",
            g.boxes,
            r.total_boxes,
            r.evaluated.boxes,
            format!(
                "declared={} synth={} flags={}",
                r.declared_boxes, r.synth_endpoint_boxes, r.rail_flag_boxes
            ),
        ));

        // (2) box roster (match/missing/extra)
        if g.box_names.is_empty() {
            findings.push((
                "G10.names".into(),
                Verdict::Skip(format!("golden has no roster, eval={}", r.box_names.len())),
            ));
        } else {
            let (missing, extra) =
                multiset_diff(&sorted_lower(&g.box_names), &sorted_lower(&r.box_names));
            if missing.is_empty() && extra.is_empty() {
                findings.push((
                    "G10.names".into(),
                    Verdict::Ok(format!("{} boxes all match", r.box_names.len())),
                ));
            } else {
                findings.push((
                    "G10.names".into(),
                    Verdict::Fail(format!(
                        "missing={} extra={}",
                        fmt_list(&missing),
                        fmt_list(&extra)
                    )),
                ));
            }
        }

        // (3) synth boxes == 0
        findings.push(num_check(
            "G10.synth",
            g.synth_boxes,
            r.synth_endpoint_boxes,
            r.evaluated.boxes,
            "Phase1.5/1.6 synth".into(),
        ));

        // (4) rail flag boxes == 0
        findings.push(num_check(
            "G10.flags",
            g.rail_flags,
            r.rail_flag_boxes,
            r.evaluated.boxes,
            "terminals are not boxes".into(),
        ));

        // (5) connectivity: all endpoints of every net in one route connected component
        if r.pins_total == 0 {
            findings.push((
                "G10.conn".into(),
                Verdict::Skip("conn report not injected (eval=0)".into()),
            ));
        } else if r.pins_unreachable == 0 {
            findings.push((
                "G10.conn".into(),
                Verdict::Ok(format!(
                    "{}/{} pins reachable",
                    r.pins_total - r.pins_unreachable,
                    r.pins_total
                )),
            ));
        } else {
            findings.push((
                "G10.conn".into(),
                Verdict::Fail(format!(
                    "{}/{} pins unreachable",
                    r.pins_unreachable, r.pins_total
                )),
            ));
        }

        // G11 power contract
        findings.push(num_check(
            "G11.gnd_edges",
            g.gnd_edges,
            r.gnd_edges,
            r.evaluated.nets,
            "GND edges (R-1: no driver, not drawn)".into(),
        ));
        findings.push(num_check(
            "G11.power_edges",
            g.power_edges,
            r.power_edges,
            r.evaluated.nets,
            "rail power edges (R-2: driver segments)".into(),
        ));
        if g.top_passives > 0 || r.layer == "main" {
            // C5 judged at top level only (golden gives top_passives semantics to the main layer
            // only)
            findings.push(num_check(
                "G11.top_passives",
                g.top_passives,
                r.two_pin_passives,
                r.evaluated.boxes,
                "block diagram draws no passives".into(),
            ));
        }

        // Edge list (structural comparison: is each expected edge covered by an actual net; which
        // extras exist)
        if g.edge.is_empty() {
            findings.push((
                "G11.edges".into(),
                Verdict::Skip(format!(
                    "golden has no edge list, actual edges={}",
                    r.edges.len()
                )),
            ));
        } else {
            let exp: Vec<(String, String, String)> = g
                .edge
                .iter()
                .map(|e| (e.from.clone(), e.to.clone(), e.label.clone()))
                .collect();
            let act: Vec<(String, String, String)> = r.edges.iter().map(|e| edge_key(e)).collect();
            let (missing, extra) = multiset_diff_str3(&exp, &act);
            if missing.is_empty() && extra.is_empty() {
                findings.push((
                    "G11.edges".into(),
                    Verdict::Ok(format!("{} edges all match", r.edges.len())),
                ));
            } else {
                findings.push((
                    "G11.edges".into(),
                    Verdict::Fail(format!(
                        "missing=[{}] extra=[{}]",
                        missing
                            .iter()
                            .map(|t| format!("{}~{}:{}", t.0, t.1, t.2))
                            .collect::<Vec<_>>()
                            .join(", "),
                        extra
                            .iter()
                            .map(|t| format!("{}~{}:{}", t.0, t.1, t.2))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )),
                ));
            }
        }

        // G12 geometric legality
        findings.push(num_check(
            "G12.box_box",
            0,
            r.box_box,
            r.evaluated.boxes,
            "box collisions".into(),
        ));
        findings.push(num_check(
            "G12.wire_box",
            0,
            r.wire_box,
            r.evaluated.nets,
            "wire through box".into(),
        ));
        findings.push(num_check(
            "G12.s6_size",
            0,
            r.s6_violations,
            r.evaluated.sizes,
            "box cannot fit pins (S6)".into(),
        ));
        findings.push(num_check(
            "G12.offcanvas",
            0,
            r.offcanvas_boxes,
            r.evaluated.boxes,
            "negative-coordinate boxes".into(),
        ));

        // G13 device contracts (P7-5, S3~S9)
        // Contract checks are ok/total self-contained (no golden numbers):
        // total == 0 → SKIP (visible, never green).
        let ratio_check = |id: &str, ok: usize, total: usize, note: &str| {
            if total == 0 {
                (id.to_string(), Verdict::Skip(format!("eval=0 ({note})")))
            } else if ok == total {
                (
                    id.to_string(),
                    Verdict::Ok(format!("{ok}/{total} ({note})")),
                )
            } else {
                (
                    id.to_string(),
                    Verdict::Fail(format!("{ok}/{total} ({note})")),
                )
            }
        };
        findings.push(ratio_check(
            "G13.S3_decouple_pin",
            r.g13.s3_decouple_ok,
            r.g13.s3_decouple_total,
            "decoupling caps glued to pin, vertical, rail up / GND down",
        ));
        findings.push(ratio_check(
            "G13.S4a_gnd_vertical",
            r.g13.s4_gnd_vertical_ok,
            r.g13.s4_gnd_total,
            "grounded passives stand vertically",
        ));
        findings.push(ratio_check(
            "G13.S4b_chain_aligned",
            r.g13.s4_chain_aligned_ok,
            r.g13.s4_chain_total,
            "series-chain members share the chain orientation",
        ));
        findings.push(ratio_check(
            "G13.S5_transpose_rung",
            r.g13.s5_rung_ok,
            r.g13.s5_rung_total,
            "transposed devices drawn as vertical rungs",
        ));
        findings.push(num_check(
            "G13.S7_label_overlap",
            0,
            r.g13.s7_label_overlaps,
            r.g13.s7_labels_total,
            "label text intersecting boxes/wires".into(),
        ));
        findings.push(ratio_check(
            "G13.S8_nc_visible",
            r.g13.s8_nc_ok,
            r.g13.s8_nc_total,
            "NC devices carry the NC_ prefix",
        ));
        findings.push(ratio_check(
            "G13.S9_stub_terminal",
            r.g13.s9_stub_ok,
            r.g13.s9_stub_total,
            "cross-module signals end on a labeled stub",
        ));

        // G14 semantic rosters (module-port names / net names). Empty golden roster
        // skips: the criterion only judges when the golden names its semantics.
        let roster_check = |id: &str, g: &[String], a: &[String]| {
            if g.is_empty() {
                return (
                    id.to_string(),
                    Verdict::Skip(format!("golden roster empty, eval={}", a.len())),
                );
            }
            let (missing, extra) = multiset_diff(&sorted_lower(g), &sorted_lower(a));
            if missing.is_empty() && extra.is_empty() {
                (
                    id.to_string(),
                    Verdict::Ok(format!("{} names all match", a.len())),
                )
            } else {
                (
                    id.to_string(),
                    Verdict::Fail(format!(
                        "missing={} extra={}",
                        fmt_list(&missing),
                        fmt_list(&extra)
                    )),
                )
            }
        };
        findings.push(roster_check("G14.ports", &g.port_names, &r.port_names));
        findings.push(roster_check("G14.nets", &g.net_names, &r.net_names));

        let red = findings
            .iter()
            .filter(|f| matches!(f.1, Verdict::Fail(_)))
            .count();
        let green = findings
            .iter()
            .filter(|f| matches!(f.1, Verdict::Ok(_)))
            .count();
        let skipped = findings
            .iter()
            .filter(|f| matches!(f.1, Verdict::Skip(_)))
            .count();
        LayerDiff {
            layer: r.layer.clone(),
            findings,
            red,
            green,
            skipped,
        }
    }
}

fn num_check(
    id: &str,
    expect: usize,
    actual: usize,
    evaluated: usize,
    note: String,
) -> (String, Verdict) {
    if evaluated == 0 {
        return (id.to_string(), Verdict::Skip(format!("eval=0 ({note})")));
    }
    if expect == actual {
        (
            id.to_string(),
            Verdict::Ok(format!("{actual} == golden {expect}（{note}）")),
        )
    } else {
        (
            id.to_string(),
            Verdict::Fail(format!("{actual} != golden {expect}（{note}）")),
        )
    }
}

fn fmt_list(v: &[String]) -> String {
    if v.is_empty() {
        "[]".to_string()
    } else if v.len() <= 8 {
        format!("[{}]", v.join(","))
    } else {
        format!("[{} …+{}]", v[..8].join(","), v.len() - 8)
    }
}

fn multiset_diff_str3(
    exp: &[(String, String, String)],
    act: &[(String, String, String)],
) -> (Vec<(String, String, String)>, Vec<(String, String, String)>) {
    use std::collections::BTreeMap;
    let key = |t: &(String, String, String)| format!("{}\u{1}{}\u{1}{}", t.0, t.1, t.2);
    let mut e: BTreeMap<String, i32> = BTreeMap::new();
    let mut a: BTreeMap<String, i32> = BTreeMap::new();
    for t in exp {
        *e.entry(key(t)).or_insert(0) += 1;
    }
    for t in act {
        *a.entry(key(t)).or_insert(0) += 1;
    }
    let mut missing = Vec::new();
    let mut extra = Vec::new();
    for (k, n) in &e {
        let m = a.get(k).copied().unwrap_or(0);
        if m < *n {
            let parts: Vec<&str> = k.split('\u{1}').collect();
            for _ in 0..(n - m) {
                missing.push((parts[0].into(), parts[1].into(), parts[2].into()));
            }
        }
    }
    for (k, m) in &a {
        let n = e.get(k).copied().unwrap_or(0);
        if m > &n {
            let parts: Vec<&str> = k.split('\u{1}').collect();
            for _ in 0..(m - n) {
                extra.push((parts[0].into(), parts[1].into(), parts[2].into()));
            }
        }
    }
    (missing, extra)
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiset_diff_counts_multiplicity() {
        let (m, e) = multiset_diff(
            &["a".to_string(), "b".to_string()],
            &["a".to_string(), "a".to_string(), "c".to_string()],
        );
        assert_eq!(m, vec!["b".to_string()]);
        assert_eq!(e, vec!["a".to_string(), "c".to_string()]);
    }

    #[test]
    fn skip_when_eval_zero() {
        let (_, v) = num_check("G12.box_box", 0, 0, 0, "t".into());
        assert!(matches!(v, Verdict::Skip(_)), "eval=0 must SKIP, not green");
    }

    #[test]
    fn golden_toml_parses() {
        // Minimal sample isomorphic to baseline/render_golden.toml
        let text = r#"
[layer.main]
module = "main"
boxes = 10
synth_boxes = 0
rail_flags = 0
gnd_edges = 0
power_edges = 4
top_passives = 0
box_names = ["a", "b"]

[[layer.main.edge]]
from = "a"
to = "b"
label = "USB_5V"
"#;
        let g: RenderGolden = toml::from_str(text).unwrap();
        assert_eq!(g.layer["main"].boxes, 10);
        assert_eq!(g.layer["main"].edge.len(), 1);
    }

    fn reading_with_rosters(ports: &[&str], nets: &[&str]) -> LayerReading {
        LayerReading {
            layer: "main".into(),
            bid: 0,
            total_boxes: 0,
            declared_boxes: 0,
            synth_endpoint_boxes: 0,
            rail_flag_boxes: 0,
            box_names: vec![],
            gnd_edges: 0,
            power_edges: 0,
            two_pin_passives: 0,
            edges: vec![],
            decorations_ground: 0,
            decorations_power: 0,
            geom_double_writes: 0,
            geom_double_write_list: vec![],
            box_box: 0,
            wire_box: 0,
            s6_violations: 0,
            offcanvas_boxes: 0,
            port_names: ports.iter().map(|s| s.to_string()).collect(),
            net_names: nets.iter().map(|s| s.to_string()).collect(),
            pins_total: 0,
            pins_unreachable: 0,
            evaluated: EvalCounts::default(),
            g13: G13Reading::default(),
        }
    }

    fn verdict_of<'a>(d: &'a LayerDiff, id: &str) -> &'a Verdict {
        d.findings
            .iter()
            .find(|(k, _)| k == id)
            .map(|(_, v)| v)
            .expect("criterion present")
    }

    #[test]
    fn g14_roster_judges_missing_extra_and_skip() {
        let g: RenderGolden = toml::from_str(
            r#"
[layer.main]
module = "main"
boxes = 0
port_names = ["vin", "vout"]
net_names = ["V5V", "GND"]
"#,
        )
        .unwrap();

        // exact match -> green
        let r = reading_with_rosters(&["vin", "vout"], &["V5V", "GND"]);
        let d = g.diff_layer(&r);
        assert!(matches!(verdict_of(&d, "G14.ports"), Verdict::Ok(_)));
        assert!(matches!(verdict_of(&d, "G14.nets"), Verdict::Ok(_)));

        // missing + extra -> red
        let r = reading_with_rosters(&["vin"], &["V5V", "GND", "V3V3"]);
        let d = g.diff_layer(&r);
        assert!(matches!(verdict_of(&d, "G14.ports"), Verdict::Fail(_)));
        assert!(matches!(verdict_of(&d, "G14.nets"), Verdict::Fail(_)));

        // golden roster empty -> skip (never green)
        let g2: RenderGolden = toml::from_str(
            r#"
[layer.main]
module = "main"
boxes = 0
"#,
        )
        .unwrap();
        let r = reading_with_rosters(&["vin"], &["V5V"]);
        let d = g2.diff_layer(&r);
        assert!(matches!(verdict_of(&d, "G14.ports"), Verdict::Skip(_)));
        assert!(matches!(verdict_of(&d, "G14.nets"), Verdict::Skip(_)));
    }

    // ── baseline generation (MC_RENDER_GOLDEN_SAVE / RenderGolden::save) ──

    fn tmpdir(tag: &str) -> std::path::PathBuf {
        let mut d = std::env::temp_dir();
        d.push(format!(
            "mcc-renderdiff-{}-{:?}-{tag}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A reading with non-zero evaluated counts, so `num_check` actually judges
    /// its numbers instead of taking the `eval=0` SKIP.
    fn reading_of(layer: &str, boxes: &[&str]) -> LayerReading {
        let mut r = reading_with_rosters(&[], &[]);
        r.layer = layer.into();
        r.total_boxes = boxes.len();
        r.box_names = boxes.iter().map(|s| s.to_string()).collect();
        r.evaluated = EvalCounts {
            boxes: boxes.len(),
            nets: 1,
            sizes: 0,
        };
        r
    }

    /// The whole point of a generated baseline: what it froze is what the next run
    /// is compared against, so a run judges its own reading green.
    #[test]
    fn save_then_load_round_trips_and_judges_the_frozen_fields() {
        let dir = tmpdir("roundtrip");
        let path = dir.join("render_golden.toml");
        let mut r = reading_of("main", &["U2", "R1"]);
        r.power_edges = 2;
        r.port_names = vec!["vin".into(), "vout".into()];
        r.net_names = vec!["V5V".into()];
        r.edges = vec![("U2".into(), "R1".into(), "V5V".into())];

        let g = RenderGolden::from_readings(&[r.clone()]);
        g.save(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.starts_with("# render_golden.toml"),
            "generated file must carry its own explanation:\n{text}"
        );

        let d = RenderGolden::load(&path).unwrap().diff_layer(&r);
        for id in [
            "G10.boxes",
            "G10.names",
            "G11.power_edges",
            "G11.edges",
            "G14.ports",
            "G14.nets",
        ] {
            let v = verdict_of(&d, id);
            assert!(matches!(v, Verdict::Ok(_)), "{id} should match: {v:?}");
        }
    }

    /// The invariant fields are the target, not the measurement. If a generation
    /// froze a measured 3 here, the criterion that exists to catch that 3 would
    /// read green for ever — so this is the test that keeps the lock honest.
    #[test]
    fn invariants_are_written_as_zero_and_keep_their_criterion_red() {
        let dir = tmpdir("invariants");
        let path = dir.join("render_golden.toml");
        let mut r = reading_of("main", &["U1"]);
        r.synth_endpoint_boxes = 3;
        r.rail_flag_boxes = 2;
        r.gnd_edges = 1;
        r.two_pin_passives = 4;

        let g = RenderGolden::from_readings(&[r.clone()]);
        let l = &g.layer["main"];
        assert_eq!(
            (l.synth_boxes, l.rail_flags, l.gnd_edges, l.top_passives),
            (0, 0, 0, 0),
            "an invariant is recorded as its target, never as the measurement"
        );

        let violations = RenderGolden::invariant_violations(&[r.clone()]);
        assert_eq!(violations.len(), 4, "{violations:?}");
        for id in [
            "G10.synth",
            "G10.flags",
            "G11.gnd_edges",
            "G11.top_passives",
        ] {
            assert!(
                violations.iter().any(|v| v.contains(id)),
                "{id} not reported: {violations:?}"
            );
        }

        g.save(&path).unwrap();
        let d = RenderGolden::load(&path).unwrap().diff_layer(&r);
        for id in [
            "G10.synth",
            "G10.flags",
            "G11.gnd_edges",
            "G11.top_passives",
        ] {
            let v = verdict_of(&d, id);
            assert!(matches!(v, Verdict::Fail(_)), "{id} must stay red: {v:?}");
        }
    }

    /// Two runs of the same build must write the same bytes, or every regeneration
    /// shows up as a whole-file diff: layer order, roster order and edge direction
    /// are all renderer accidents, so none of them may reach the file.
    #[test]
    fn save_is_byte_stable_under_layer_roster_and_edge_order() {
        let dir = tmpdir("stable");
        let (a_path, b_path) = (dir.join("a.toml"), dir.join("b.toml"));

        let mut main_a = reading_of("main", &["U2", "R1", "C3"]);
        main_a.net_names = vec!["GND".into(), "V5V".into()];
        main_a.edges = vec![
            ("U2".into(), "R1".into(), "V5V".into()),
            ("C3".into(), "R1".into(), "GND".into()),
        ];
        let sub = reading_of("sub_u2", &["X1"]);

        // Same figures, every list in a different order, one edge pair reversed.
        let mut main_b = reading_of("main", &["C3", "R1", "U2"]);
        main_b.net_names = vec!["V5V".into(), "GND".into()];
        main_b.edges = vec![
            ("R1".into(), "C3".into(), "GND".into()),
            ("R1".into(), "U2".into(), "V5V".into()),
        ];

        RenderGolden::from_readings(&[main_a.clone(), sub.clone()])
            .save(&a_path)
            .unwrap();
        RenderGolden::from_readings(&[sub, main_b.clone()])
            .save(&b_path)
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(&a_path).unwrap(),
            std::fs::read_to_string(&b_path).unwrap()
        );

        // The reversed pair is also the case that would read red if the golden
        // stored the raw direction (`diff_layer` canonicalizes only the reading).
        let g = RenderGolden::load(&a_path).unwrap();
        let d = g.diff_layer(&main_b);
        let v = verdict_of(&d, "G11.edges");
        assert!(
            matches!(v, Verdict::Ok(_)),
            "reversed pair should match: {v:?}"
        );
    }

    /// `baseline/` is gitignored, so a fresh clone has no directory for the first
    /// save to land in.
    #[test]
    fn save_creates_a_missing_baseline_directory() {
        let dir = tmpdir("mkdir");
        let path = dir.join("baseline/render_golden.toml");
        assert!(!path.parent().unwrap().exists());
        RenderGolden::from_readings(&[reading_of("main", &["U1"])])
            .save(&path)
            .unwrap();
        assert!(path.is_file());
    }

    /// `MC_RENDER_GOLDEN_SAVE` is the write switch, and it must not be tripped by
    /// the values a shell conveniently leaves around: an unset variable, an empty
    /// one and `0` / `false` are all "compare, do not write".
    #[test]
    fn save_flag_is_off_unless_it_says_otherwise() {
        for (set, expect) in [
            (None, false),
            (Some(""), false),
            (Some("0"), false),
            (Some(" 0 "), false),
            (Some("false"), false),
            (Some("False"), false),
            (Some("1"), true),
            (Some("yes"), true),
        ] {
            assert_eq!(save_flag_set(set), expect, "{set:?}");
        }
    }
}
