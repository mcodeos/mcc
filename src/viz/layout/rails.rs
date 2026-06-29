// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ Stage A / A2 —— Power rails "explode" into local flags (rail → per-consumer flags)
//!
//! ## What problem does this file solve
//! Top-level splits hyperedges like `V3V3 / V5V / GND / VCC_1V2 ...` where "one PowerLabel connects to N modules"
//! into N single-pin `PowerLabel` flags **before layout**, each flag connects to only one
//! consumer pin (one 2-terminal stub net).
//!
//! Benefits:
//! - Routing side **no longer needs** horizontal/vertical trunks spanning the entire canvas —— each flag just has one nearby short stub.
//!   (Those top-level trunks with `span≈690` for V5V/GND/VDD_3V3 directly disappear)
//! - Rail boxes no longer pollute layout adjacency as "fully connected hub" —— real signal connections
//!   between modules determine placement.
//! - This is the real schematic power drawing practice: same-name flags implicitly connected by net name, no global bus drawn.
//!
//! ## Trigger condition
//! `box.symbol.is_power_rail() || box.kind == BoxKind::PowerLabel`
//!
//! ## Call location
//! Only called at **top-level** (see beginning of `flow.rs::FlowLayouter::layout`). Sub-level does not explode ——
//! Sub-level rail handling is already visually flags (see user screenshots image 2/3), leave unchanged.
//!
//! ## Cooperation with scheduler (important)
//! `scheduler.rs::merge_same_name_power_ground_nets` must **skip stub nets connected to PowerLabel**,
//! otherwise it will merge the per-consumer stubs exploded here back into trunk.
//! See scheduler.rs patch in STAGE_A_patches.md.

use std::collections::{HashMap, HashSet};

use crate::vector::graph::naming;
use crate::vector::graph::net_def::IoDirection;
use crate::vector::graph::{
    BoxKind, EndpointRef, EntryPoint, EntrySide, IoSummary, McVecBox, McVecGraph, NetKind, Symbol,
    VizNet,
};

use super::normalize::{compute_canvas, normalize_positions};

/// Flag box ID starting base (avoid existing ids; rail-synth synthetic ids are in 1e9 range)
pub const FLAG_ID_BASE: i64 = 9_000_000_000;
/// Stub net ID starting base
pub const STUB_NET_ID_BASE: i64 = 9_500_000_000;

/// Whether name contains power/ground token.
///
/// ★ FIX (subgraph iteration): Handles `"[VCC_1V2, GND]"` / `"[VDD_3V3, GND]"` kind of **bracket /
/// composite** power port names —— `naming::is_power_rail` returns false for the whole string because it starts with `[`,
/// causing these "obviously power/ground boundary ports" to be treated as ordinary core nodes (their endpoints are mostly synthetic pin_ids,
/// degenerate wire routing, floating wires / compressed wires). Here we split the name into tokens by non-alphanumeric (keep `_`),
/// if any token is power/ground, treat as power name. Pure signal names (UART0 / SCLK / MOSI / DAC_OUT ...)
/// don't contain power tokens, still returns false.
fn name_has_power_token(name: &str) -> bool {
    name.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|s| !s.is_empty())
        .any(naming::is_power_rail)
}

/// Determine if a box is a power/ground label (rail)
///
/// ★ FIX (subgraph fix): In addition to kind/symbol, **name must also contain power/ground token**
/// (`name_has_power_token`, see its docs).
///
/// Reason: Submodule boundary I/O ports (UART0 / SCLK / MOSI / MISO / CSN / DAC_OUT
/// / SPK_MUTE ...) **borrow** `BoxKind::PowerLabel` in `from_block.rs` Phase E.1
/// to draw "arrow + name", but they are semantically signals, shouldn't be treated by
/// FlowLayouter as power flags extracted from core layout / randomly placed by place_flags / skipped by
/// align_hub_to_spokes and order_pins_by_neighbor.
///
/// Name gating **won't hurt real power rails**: real rails in `detect.rs::detect_kind` /
/// `from_block.rs` are only made PowerLabel if name passes `is_power_rail`,
/// and bracket power ports (containing GND/VCC token) via `name_has_power_token` are also
/// correctly recognized as rail —— only pure signal ports are excluded.
pub fn is_rail_box(b: &McVecBox) -> bool {
    (b.symbol.is_power_rail() || b.kind == BoxKind::PowerLabel) && name_has_power_token(&b.name)
}

/// ★ A2 main entry: explode all rail hyperedges in this layer into per-consumer flags + stubs
///
/// Directly modifies `graph.boxes` / `graph.nets` in place:
/// - Delete all rail boxes
/// - Delete all nets "connected to rail"
/// - For each (rail, consumer) pair, add one single-pin PowerLabel flag + one 2-terminal stub
pub fn explode_power_rails_to_flags(graph: &mut McVecGraph) {
    // 1. Mark rail boxes
    let rail_ids: HashSet<i64> = graph
        .boxes
        .iter()
        .filter(|b| is_rail_box(b))
        .map(|b| b.id)
        .collect();
    if rail_ids.is_empty() {
        return;
    }

    // 2. Extract rail metadata (immutable phase)
    //    (id, name, class_name, symbol, is_ground)
    let rails: Vec<(i64, String, String, Symbol, bool)> = graph
        .boxes
        .iter()
        .filter(|b| rail_ids.contains(&b.id))
        .map(|b| {
            let is_gnd = match b.symbol {
                Symbol::PowerRail { is_ground } => is_ground,
                _ => naming::is_ground(&b.name),
            };
            (
                b.id,
                b.name.clone(),
                b.class_name.clone(),
                b.symbol.clone(),
                is_gnd,
            )
        })
        .collect();

    // rail_id → (name, class, symbol, is_gnd)
    let rail_meta: std::collections::HashMap<i64, (String, String, Symbol, bool)> = rails
        .into_iter()
        .map(|(id, name, class, sym, gnd)| (id, (name, class, sym, gnd)))
        .collect();

    // ★ Merge by (consumer_box, rail name): same-name rails on same module = same net → one flag
    //   (fixes visual duplication like V1V2×2 / V3V3×2 where "same-name pins each create a flag")
    struct Grp {
        is_gnd: bool,
        sym: Symbol,
        class: String,
        name: String,
        pins: Vec<EndpointRef>,
        seen: HashSet<i64>,
    }
    let mut groups: std::collections::BTreeMap<(i64, String), Grp> =
        std::collections::BTreeMap::new();
    let mut nets_to_drop: HashSet<usize> = HashSet::new();

    for (idx, net) in graph.nets.iter().enumerate() {
        let rail_ep = match net.endpoints.iter().find(|e| rail_ids.contains(&e.box_id)) {
            Some(e) => e,
            None => continue,
        };
        nets_to_drop.insert(idx);
        let (rname, rclass, rsym, is_gnd) = match rail_meta.get(&rail_ep.box_id) {
            Some(m) => m.clone(),
            None => continue,
        };
        for e in &net.endpoints {
            if rail_ids.contains(&e.box_id) {
                continue; // Skip rail itself / other rails
            }
            let key = (e.box_id, rname.clone());
            let g = groups.entry(key).or_insert_with(|| Grp {
                is_gnd,
                sym: rsym.clone(),
                class: rclass.clone(),
                name: rname.clone(),
                pins: Vec::new(),
                seen: HashSet::new(),
            });
            if g.seen.insert(e.pin_id) {
                g.pins.push(e.clone());
            }
        }
    }

    let mut next_box_id = FLAG_ID_BASE;
    let mut next_net_id = STUB_NET_ID_BASE;
    let mut new_flags: Vec<McVecBox> = Vec::new();
    let mut new_stubs: Vec<VizNet> = Vec::new();

    for (_key, g) in groups {
        if g.pins.is_empty() {
            continue;
        }
        let flag_id = next_box_id;
        next_box_id += 1;
        let flag_pin_id = flag_id; // Single pin, reuse box_id as pin_id to ensure uniqueness

        let flag_symbol = match g.sym {
            Symbol::PowerRail { .. } => g.sym.clone(),
            _ => Symbol::PowerRail {
                is_ground: g.is_gnd,
            },
        };
        let net_kind = if g.is_gnd {
            NetKind::Ground
        } else {
            NetKind::Power
        };
        let flag_io = if g.is_gnd {
            IoDirection::Ground
        } else {
            IoDirection::Power
        };

        let mut io = IoSummary::new();
        io.power += 1;
        let flag = McVecBox::new_v2(
            flag_id,
            g.name.clone(),
            g.class.clone(),
            BoxKind::PowerLabel,
            flag_symbol.clone(),
            None,
            None,
            1,
            io,
        );
        new_flags.push(flag);

        // stub: flag + this module's **one representative pin** of this rail (same-name pins = same net, only one wire → straight line no fork)
        let mut eps = vec![EndpointRef::with_io(
            flag_id,
            flag_pin_id,
            g.name.clone(),
            flag_io,
        )];
        if let Some(first) = g.pins.into_iter().next() {
            eps.push(first);
        }
        let stub = VizNet::new(next_net_id, g.name.clone(), net_kind, eps);
        next_net_id += 1;
        new_stubs.push(stub);
    }

    let n_rails = rail_ids.len();
    let n_flags = new_flags.len();
    let n_dropped = nets_to_drop.len();

    // 3. Apply changes
    graph.boxes.retain(|b| !rail_ids.contains(&b.id));
    let mut idx = 0usize;
    graph.nets.retain(|_| {
        let keep = !nets_to_drop.contains(&idx);
        idx += 1;
        keep
    });
    graph.boxes.extend(new_flags);
    graph.nets.extend(new_stubs);

    crate::vlog!(
        "[layout::rails] A2: exploded {} rail box(es) → {} flag(s), dropped {} net(s)",
        n_rails, n_flags, n_dropped
    );
}

// ============================================================================
// ★ Stage 1: net labels / air wires (long-net → named stubs)
// ============================================================================
//
// Long signal nets spanning the whole graph pass through a bunch of boxes → a bunch of crossings → a bunch of jumpers (bridges), the graph becomes messy. Industrial schematic
// standard practice is **net labels (net label / air wires)**: don't draw that long wire, but place a same-name short label stub next to each endpoint,
// same name = electrically connected. This pass transforms "long signal nets" into such label stubs:
//   - Create a **single-pin PowerLabel** next to each endpoint (reuses existing flag rendering, same style as sub-graph boundary ports) +
//     one **short stub** (label pin ↔ original pin), then **delete that long net**.
//   - Only modify nets of `NetKind::Signal` with **span over threshold**; power/ground (already flags), buses, and nets with either endpoint
//     already connected to label/flag are not touched.
//
// Must run **after layout, before routing** (at this point boxes have coordinates, can judge "long" by span; routing hasn't run yet,
// modifying boxes is safe). Hooked in api.rs Phase 1.8. Returns new canvas size (added label boxes, boundary needs recalculation).

const NETLABEL_LONG_SPAN: f64 = 650.0; // Span over this value (px) to convert to air wire (adjustable)
const NETLABEL_GAP: f64 = 42.0; // Distance of label from pin
const NETLABEL_W: f64 = 14.0;
const NETLABEL_H: f64 = 14.0;

/// Pin coordinates = box edge + offset (consistent with renderer pin_position, inlined to avoid cross-module dependencies).
fn pin_xy(b: &McVecBox, ep: &EntryPoint) -> (f64, f64) {
    match ep.side {
        EntrySide::Top => (b.x + b.w * ep.offset, b.y),
        EntrySide::Bottom => (b.x + b.w * ep.offset, b.y + b.h),
        EntrySide::Left => (b.x, b.y + b.h * ep.offset),
        EntrySide::Right => (b.x + b.w, b.y + b.h * ep.offset),
    }
}

/// ★ Stage 1 main entry: convert long signal nets to net label stubs. Returns `Some(new canvas)` if changed, else `None`.
pub fn apply_net_labels(graph: &mut McVecGraph) -> Option<(f64, f64)> {
    // 1. (box_id, pin_id) → (pin coordinates, side); record which boxes are labels/flags (PowerLabel).
    let mut pin_pos: HashMap<(i64, i64), ((f64, f64), EntrySide)> = HashMap::new();
    let mut label_boxes: HashSet<i64> = HashSet::new();
    for b in &graph.boxes {
        if b.kind == BoxKind::PowerLabel {
            label_boxes.insert(b.id);
        }
        for ep in &b.entry_points {
            pin_pos.insert((b.id, ep.pin_id), (pin_xy(b, ep), ep.side.clone()));
        }
    }

    // New box / new net ids increment from existing max value, eliminating collisions (two namespaces are independent).
    let mut next_box = graph.boxes.iter().map(|b| b.id).max().unwrap_or(0) + 1;
    let mut next_net = graph.nets.iter().map(|n| n.nid).max().unwrap_or(0) + 1;

    let mut new_boxes: Vec<McVecBox> = Vec::new();
    let mut new_stubs: Vec<VizNet> = Vec::new();
    let mut drop_idx: HashSet<usize> = HashSet::new();

    for (idx, net) in graph.nets.iter().enumerate() {
        if !matches!(net.kind, NetKind::Signal) {
            continue; // Only process signal nets
        }
        if net.endpoints.len() < 2 {
            continue;
        }
        // ★ FIX: Only convert "meaningfully named" nets to air wires. Anonymous nets (__net_N / empty name) converting to labels is meaningless
        //   —— both ends labeled __net_25 users can't read, equals making visible wires disappear (this regression root cause). Anonymous nets drawn normally.
        if net.name.is_empty() || net.name.starts_with("__net") {
            continue;
        }
        // Either endpoint already label/flag → already "labeled", don't repeat (includes sub-graph boundary ports, power flags).
        if net
            .endpoints
            .iter()
            .any(|e| label_boxes.contains(&e.box_id))
        {
            continue;
        }
        // Endpoint coordinates + span (max pairwise distance between endpoints).
        let pts: Vec<(f64, f64)> = net
            .endpoints
            .iter()
            .filter_map(|e| pin_pos.get(&(e.box_id, e.pin_id)).map(|(p, _)| *p))
            .collect();
        if pts.len() < 2 {
            continue;
        }
        let mut span = 0.0_f64;
        for a in 0..pts.len() {
            for b in (a + 1)..pts.len() {
                let d = ((pts[a].0 - pts[b].0).powi(2) + (pts[a].1 - pts[b].1).powi(2)).sqrt();
                if d > span {
                    span = d;
                }
            }
        }
        if span < NETLABEL_LONG_SPAN {
            continue; // Short nets drawn normally
        }

        // ── Long signal net → one same-name label + one short stub per endpoint ──
        let is_gnd = naming::is_ground(&net.name);
        let lio = if is_gnd {
            IoDirection::Ground
        } else {
            IoDirection::Passive
        };
        for e in &net.endpoints {
            let ((px, py), side) = match pin_pos.get(&(e.box_id, e.pin_id)) {
                Some(v) => (v.0, v.1.clone()),
                None => continue,
            };
            // Label placed at GAP away from pin's outward direction; label's own pin turns back to face original pin (stub is a short straight line).
            let (bx, by, lside) = match side {
                EntrySide::Right => (px + NETLABEL_GAP, py - NETLABEL_H / 2.0, EntrySide::Left),
                EntrySide::Left => (
                    px - NETLABEL_GAP - NETLABEL_W,
                    py - NETLABEL_H / 2.0,
                    EntrySide::Right,
                ),
                EntrySide::Top => (
                    px - NETLABEL_W / 2.0,
                    py - NETLABEL_GAP - NETLABEL_H,
                    EntrySide::Bottom,
                ),
                EntrySide::Bottom => (px - NETLABEL_W / 2.0, py + NETLABEL_GAP, EntrySide::Top),
            };

            let box_id = next_box;
            next_box += 1;
            let pin_id = box_id; // Single pin, pin_id reuses box_id for uniqueness

            let mut io = IoSummary::new();
            io.other += 1;
            let mut lbox = McVecBox::new_v2(
                box_id,
                net.name.clone(),
                String::new(),
                BoxKind::PowerLabel,
                Symbol::PowerRail { is_ground: is_gnd },
                None,
                None,
                1,
                io,
            );
            lbox.x = bx;
            lbox.y = by;
            lbox.w = NETLABEL_W;
            lbox.h = NETLABEL_H;
            lbox.entry_points = vec![EntryPoint {
                pin_id,
                pin_name: net.name.clone(),
                side: lside,
                offset: 0.5,
            }];
            new_boxes.push(lbox);

            let eps = vec![
                EndpointRef::with_io(box_id, pin_id, net.name.clone(), lio),
                e.clone(),
            ];
            // stub inherits original kind → SubModuleIO air wire stubs remain purple, consistent with same-name other segments visually
            new_stubs.push(VizNet::new(
                next_net,
                net.name.clone(),
                net.kind.clone(),
                eps,
            ));
            next_net += 1;
        }
        drop_idx.insert(idx);
    }

    if new_boxes.is_empty() {
        return None;
    }

    // Apply: delete long net, add label + stub.
    let mut i = 0usize;
    graph.nets.retain(|_| {
        let keep = !drop_idx.contains(&i);
        i += 1;
        keep
    });
    let n_lbl = new_boxes.len();
    let n_drop = drop_idx.len();
    graph.boxes.extend(new_boxes);
    graph.nets.extend(new_stubs);

    crate::vlog!(
        "[viz::net_label] layer '{}' bid={}: {} long signal net(s) → {} label stub(s)",
        graph.name, graph.bid, n_drop, n_lbl
    );

    // Labels may extend past original canvas / land in negative coordinates → renormalize + recompute canvas (no routing yet, only modifying boxes is safe).
    normalize_positions(graph);
    Some(compute_canvas(graph))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_rail(id: i64, name: &str, is_ground: bool) -> McVecBox {
        McVecBox::new_v2(
            id,
            name.into(),
            String::new(),
            BoxKind::PowerLabel,
            Symbol::PowerRail { is_ground },
            None,
            None,
            1,
            IoSummary::new(),
        )
    }

    fn mk_mod(id: i64, name: &str) -> McVecBox {
        McVecBox::new_v2(
            id,
            name.into(),
            String::new(),
            BoxKind::SubModule,
            Symbol::Module,
            None,
            None,
            4,
            IoSummary::new(),
        )
    }

    /// Set box position + one pin (for net-label testing).
    fn placed(mut b: McVecBox, x: f64, w: f64, pin: i64, side: EntrySide) -> McVecBox {
        b.x = x;
        b.y = 0.0;
        b.w = w;
        b.h = 100.0;
        b.entry_points = vec![EntryPoint {
            pin_id: pin,
            pin_name: "S".into(),
            side,
            offset: 0.5,
        }];
        b
    }

    #[test]
    fn net_label_converts_long_signal_net() {
        // A right pin (100,50) ↔ B left pin (1000,50): span 900 > 650 → convert to label
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes
            .push(placed(mk_mod(1, "A"), 0.0, 100.0, 11, EntrySide::Right));
        g.boxes
            .push(placed(mk_mod(2, "B"), 1000.0, 100.0, 21, EntrySide::Left));
        g.nets.push(VizNet::new(
            50,
            "SIG".into(),
            NetKind::Signal,
            vec![
                EndpointRef::with_io(1, 11, "S", IoDirection::Output),
                EndpointRef::with_io(2, 21, "S", IoDirection::Input),
            ],
        ));

        let r = apply_net_labels(&mut g);
        assert!(
            r.is_some(),
            "Long signal net should be converted to label (returns new canvas)"
        );
        assert!(
            g.nets.iter().all(|n| n.nid != 50),
            "Original long net should be deleted"
        );
        assert_eq!(g.nets.len(), 2, "2 endpoints → 2 short stubs");
        let labels = g
            .boxes
            .iter()
            .filter(|x| x.kind == BoxKind::PowerLabel)
            .count();
        assert_eq!(labels, 2, "2 endpoints → 2 label boxes");
    }

    #[test]
    fn net_label_leaves_short_net_alone() {
        // A right pin (100,50) ↔ B left pin (150,50): span 50 < 650 → don't touch
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes
            .push(placed(mk_mod(1, "A"), 0.0, 100.0, 11, EntrySide::Right));
        g.boxes
            .push(placed(mk_mod(2, "B"), 150.0, 100.0, 21, EntrySide::Left));
        g.nets.push(VizNet::new(
            50,
            "SIG".into(),
            NetKind::Signal,
            vec![
                EndpointRef::with_io(1, 11, "S", IoDirection::Output),
                EndpointRef::with_io(2, 21, "S", IoDirection::Input),
            ],
        ));

        let r = apply_net_labels(&mut g);
        assert!(r.is_none(), "Short net doesn't convert to label");
        assert_eq!(g.nets.len(), 1, "Short net stays as is");
        assert!(
            g.boxes.iter().all(|x| x.kind != BoxKind::PowerLabel),
            "Shouldn't create label boxes"
        );
    }

    #[test]
    fn net_label_skips_power_net() {
        // Same distance, but kind=Ground → don't process (power/ground have their own flag rendering)
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes
            .push(placed(mk_mod(1, "A"), 0.0, 100.0, 11, EntrySide::Right));
        g.boxes
            .push(placed(mk_mod(2, "B"), 1000.0, 100.0, 21, EntrySide::Left));
        g.nets.push(VizNet::new(
            50,
            "GND".into(),
            NetKind::Ground,
            vec![
                EndpointRef::with_io(1, 11, "S", IoDirection::Ground),
                EndpointRef::with_io(2, 21, "S", IoDirection::Ground),
            ],
        ));
        assert!(
            apply_net_labels(&mut g).is_none(),
            "Ground net doesn't convert to label"
        );
        assert_eq!(g.nets.len(), 1);
    }

    #[test]
    fn a2_explodes_one_rail_into_per_consumer_flags() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_rail(100, "V3V3", false));
        g.boxes.push(mk_mod(1, "modA"));
        g.boxes.push(mk_mod(2, "modB"));
        // V3V3 connects to 2 modules
        g.nets.push(VizNet::new(
            10,
            "V3V3".into(),
            NetKind::Power,
            vec![
                EndpointRef::with_io(100, 1001, "V3V3", IoDirection::Power),
                EndpointRef::with_io(1, 11, "VDD", IoDirection::Power),
                EndpointRef::with_io(2, 21, "VDD", IoDirection::Power),
            ],
        ));

        explode_power_rails_to_flags(&mut g);

        // rail box removed; 2 flags added
        assert!(!g.boxes.iter().any(|b| b.id == 100), "rail box removed");
        let flags: Vec<_> = g.boxes.iter().filter(|b| is_rail_box(b)).collect();
        assert_eq!(flags.len(), 2, "one flag per consumer");
        // Original hyperedge removed; 2 2-terminal stubs added
        assert_eq!(g.nets.len(), 2);
        for n in &g.nets {
            assert_eq!(n.endpoints.len(), 2);
            assert!(matches!(n.kind, NetKind::Power));
            assert_eq!(n.name, "V3V3");
        }
    }

    #[test]
    fn a2_dangling_rail_is_dropped_without_flag() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_rail(100, "VDDA", false)); // ★ Name must pass is_power_rail to count as real rail
        g.boxes.push(mk_mod(1, "modA"));
        // VDDA doesn't connect to any consumer (no net)
        explode_power_rails_to_flags(&mut g);
        assert!(!g.boxes.iter().any(|b| b.id == 100));
        assert!(!g.boxes.iter().any(|b| is_rail_box(b)));
    }

    #[test]
    fn a2_signal_named_powerlabel_is_not_rail() {
        // ★ FIX (step 1 contract): Boundary I/O ports (signal name) that borrow PowerLabel kind to draw "arrow annotation"
        // shouldn't be treated as power flags; real power/ground names are still rails.
        let port = mk_rail(200, "UART0", false); // kind=PowerLabel, but name is signal
        assert!(
            !is_rail_box(&port),
            "signal-named PowerLabel must NOT be a rail"
        );
        let sclk = mk_rail(203, "SCLK", false);
        assert!(!is_rail_box(&sclk), "SCLK port must NOT be a rail");

        let rail = mk_rail(201, "V3V3", false);
        assert!(is_rail_box(&rail), "power-named PowerLabel is still a rail");
        let gnd = mk_rail(202, "GND", true);
        assert!(is_rail_box(&gnd), "GND is still a rail");

        // ★ FIX (subgraph iteration): bracket / composite power port containing power token is still a rail
        let pbus = mk_rail(204, "[VCC_1V2, GND]", false);
        assert!(
            is_rail_box(&pbus),
            "bracket power port must still be a rail"
        );
        let pbus2 = mk_rail(205, "[VDD_3V3, GND]", true);
        assert!(
            is_rail_box(&pbus2),
            "bracket power+gnd port must still be a rail"
        );
    }

    #[test]
    fn a2_noop_when_no_rails() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_mod(1, "modA"));
        g.boxes.push(mk_mod(2, "modB"));
        g.nets.push(VizNet::new(
            10,
            "sig".into(),
            NetKind::Signal,
            vec![
                EndpointRef::with_io(1, 11, "OUT", IoDirection::Output),
                EndpointRef::with_io(2, 21, "IN", IoDirection::Input),
            ],
        ));
        let before_boxes = g.boxes.len();
        let before_nets = g.nets.len();
        explode_power_rails_to_flags(&mut g);
        assert_eq!(g.boxes.len(), before_boxes);
        assert_eq!(g.nets.len(), before_nets);
    }
}
