// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ R-N (discipline 29): Equipotential tree — four-layer architecture
//!
//! Layer 1: Topology   — pure logic, zero coordinates
//! Layer 2: Layout     — places boxes by topology, writes x/y/w/h and entry_points
//! Layer 3: Geometry   — compute segments + junction dots from topology + placed coords
//! Layer 4: Render     — segments → SVG (in equipotential_tree_render.rs)
//!
//! Each net is rendered as ONE connected orthogonal tree, not n-1 independent edges.
//!
//! Forms: 2-point → single line, 3-point → T-shape, 4-point → cross, n-point → comb
//!
//! ## Ground model (deliberate divergence from `rails.rs` R-1)
//! `rails.rs` R-1 says a driverless ground rail draws **no edges at all** and
//! one in-place glyph per endpoint, all into `rail_decorations` — that rule
//! governs non-device layers. The device layer here is different on purpose:
//! **one ground net → one trunk → one ground glyph** (`A9` in `equi_audit`).
//! `moddcdc` carries five separate GND nets (the projection layer explodes a
//! driverless ground rail per consumer, and `coalesce.rs` keeps Power/Ground out
//! of the union-find), and the target schematic draws five independent ground
//! symbols. Do NOT "fix" this layer to match `rails.rs` R-1 — it is a different
//! rendering contract.
//!
//! ## M1 row model
//! Every trunk is a horizontal row. A W/E net, whose trunk used to be vertical,
//! becomes a horizontal row at its anchor's first-pin y, extending outward from
//! the anchor edge; N/S nets keep their outside-the-anchor rail position.
//! `Lane::horizontal` carries the orientation so no pass re-derives it from the
//! Region (which keeps only semantics — which side of the IC the net leaves).
//! Lane resolution runs in **dependency order** (a net's lane is computed only
//! once its anchor box is placed), so the layout phase and the render replay are
//! constructively identical (A2).

use std::collections::{BTreeMap, BTreeSet};

use crate::vector::graph::netdef::IoDirection;
use crate::vector::graph::{BoxKind, EntrySide, McVecGraph, NetKind, PinSlot, VizNet};
use crate::vector::model::RailClass;

// ============================================================================
// Constants
// ============================================================================

/// Minimum pin pitch for R-D box sizing
pub const PIN_PITCH: f64 = 40.0;

/// Margin on each end of pin row
pub const PIN_MARGIN: f64 = 20.0;

/// Gap from anchor right edge to trunk
pub const TRUNK_GAP: f64 = 100.0;

/// Gap from trunk to member box
pub const MEMBER_GAP: f64 = 60.0;

/// ★ Drop from the tree to a terminal symbol (Ground symbol / NetLabel bus
/// circle). The lane span is extended by this amount past the last tap point
/// when the net carries a Ground terminal, so the ground symbol hangs on its
/// own short wire instead of sitting on the last pin's tooth; NetLabel/BusLabel
/// symbols are placed this far off the trunk and connected by a stub wire.
pub const SYMBOL_DROP: f64 = 60.0;

/// ★ E4: Fixed symbol size for two-pin passive components (R/C/L/D).
/// R-D formula (pin_count × PIN_PITCH + 2 × MARGIN) applies only to MultiPin boxes.
pub const TWO_PIN_SYMBOL_W: f64 = 60.0;
pub const TWO_PIN_SYMBOL_H: f64 = 20.0;

/// Minimum anchor box width (device_layout_v2.md sec.4.5) when the North/South
/// sides carry few or no pins.
pub const MIN_BOX_W: f64 = 120.0;

/// ★ M7.6: minimum height of a NON-anchor multi-pin box (a connector, a second
/// IC — a `TapRole::Sink`). `h.max(20)` left `speaker`'s `spk` 20px tall with
/// four pins on one edge; a real component needs room for its pins and its
/// name. Width is label-driven, see [`sink_box_size`].
pub const MIN_SINK_H: f64 = 60.0;

/// M2.5 Step 4: estimated character width for a label (used to size the anchor
/// box so pin names do not spill past the box edge).
pub const LABEL_CHAR_W: f64 = 7.0;

/// M2.5 Step 4: padding on each side of a label inside the anchor box.
pub const LABEL_PAD: f64 = 16.0;

/// M2.5 Step 5: clearance between the IC edge and the North/South edge rails.
/// Replaces `TRUNK_GAP` for the rails — `TRUNK_GAP` remains for the terminal
/// symbol stub spacing.
///
/// M3.5 (R4): 40 → 80 — the South rail used to sit only 40px below the IC
/// bottom, and a Drop member hanging off it pressed its top edge against the
/// box edge / pin numbers. 80 gives room for pin arrows + numbers + names.
pub const RAIL_GAP: f64 = 80.0;

/// M2.5 Step 6: minimum clearance between two rows when searching for a free
/// row for a free net. Replaces the use of `MEMBER_GAP` as clearance (which
/// blew the IC up into a tall strip).
pub const ROW_CLEAR: f64 = 20.0;

/// M3.4: short lead from the trunk to a `Bridge`/`Drop` member's entry pin.
///
/// M3.5 (R4): 0 → 20 — the M3.4 spec's lead-wire segment was cancelled by
/// `LEAD = 0`; restoring it makes the member hang off the trunk on a visible
/// stub instead of sitting flush against the rail/trunk.
pub const LEAD: f64 = 20.0;

/// ★ M8.8: how far a W/E single-pin member (a test point) hangs DOWN off its
/// row. `assign_rows` keeps the power rows at the top of a layer, so a signal
/// row that sits one `LEAD` under them (the `speaker` VO2 test point under the
/// VDD trunk) reads as a wall. Hanging the test point a little lower frees the
/// band under the power rows. Only InlineEnd (test points) — grounded shunts
/// (Drop) are pinned by the fixture contracts and must stay at `LEAD`.
pub const SIDE_HANG: f64 = LEAD + 30.0;

/// The vertical corridor a 2-pin Bridge/Drop member needs below/above its row:
/// its body (`TWO_PIN_SYMBOL_W` tall) plus the `LEAD` wire to the trunk. The
/// next band must clear `LEAD + h`, otherwise its trunk would run collinear
/// with (or through) the member's body.
pub const CORRIDOR_DEMAND: f64 = LEAD + TWO_PIN_SYMBOL_W;

/// M3.5 (R3): an anchor tooth is drawn this far OUTWARD from the box edge, so
/// it does not run along the border (a West pin's tooth used to coincide with
/// `x = box.x`, drawing a wire on top of the box's left border).
pub const TOOTH_GAP: f64 = 20.0;

/// ★ M7.2: step a label symbol takes OUTWARD along its trunk when the trunk's
/// outer end has no free stub direction.
///
/// The trunk ends exactly ON the outermost member's tap, and `segment_hits_box`
/// counts grazing a box edge as a hit — so every stub off that point is
/// rejected, `pick_stub_dir` returns `None` and `realize` used to fall straight
/// through to `symbol_alt_node`: the INNER end, hard against the layer anchor,
/// where the label renders on top of the IC (`usbsock` `USB_VBUS`). Walking one
/// or two of these steps outward first is the "the chain is not expanded far
/// enough" fix — the symbol hangs in free space and its text points outward.
///
/// 40 clears the half-width of every drawn two-pin glyph (see [`COL_MARGIN`]),
/// so one step is normally enough.
///
/// [`COL_MARGIN`]: super::equi_column::COL_MARGIN
pub const SYMBOL_LANE: f64 = 40.0;

// ============================================================================
// Region / Lane — direction as a first-class citizen (device_layout_v2.md)
// ============================================================================

/// A net's orientation relative to the layer anchor. Pins inherit the Region of
/// the net they belong to — no pass hardcodes `Right`/`Left` anymore.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Region {
    North,
    West,
    East,
    South,
}

impl Region {
    /// Unit outward normal: (dx, dy). W=(-1,0) E=(1,0) N=(0,-1) S=(0,1)
    pub fn outward(self) -> (f64, f64) {
        match self {
            Region::North => (0.0, -1.0),
            Region::West => (-1.0, 0.0),
            Region::East => (1.0, 0.0),
            Region::South => (0.0, 1.0),
        }
    }

    /// The box edge a pin in this region lives on.
    pub fn entry_side(self) -> EntrySide {
        match self {
            Region::North => EntrySide::Top,
            Region::West => EntrySide::Left,
            Region::East => EntrySide::Right,
            Region::South => EntrySide::Bottom,
        }
    }
}

/// A net's trunk lane — the single source of truth for trunk coordinates.
///
/// Written exactly once by P3 `resolve_lanes`; P4 (place members) and
/// P5 (realize) only read it. This kills the "Layer 2 and Layer 3 each compute
/// x" bug class: layout and render can no longer drift apart on trunk position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lane {
    pub region: Region,
    /// Channel index within the region. Kept for compatibility; in the M1 row
    /// model it no longer participates in the axis computation (nets on the
    /// same anchor are already separated by their pin positions, so an index
    /// offset would scatter the rows).
    pub index: usize,
    /// ★ M1: trunk orientation, no longer derived from the Region. Region keeps
    /// only semantics (which side of the IC the net leaves); the trunk itself
    /// is always a horizontal row. `true` for every net from M1 on — the field
    /// exists so M4's column model does not re-derive direction from Region.
    pub horizontal: bool,
    /// Trunk axis coordinate: `y` for the horizontal (row) form.
    pub axis: f64,
    /// Extent along the trunk: `(x_lo, x_hi)` for the horizontal form.
    pub span: (f64, f64),
}

impl Default for Lane {
    fn default() -> Self {
        Lane {
            region: Region::East,
            index: 0,
            horizontal: true,
            axis: 0.0,
            span: (0.0, 0.0),
        }
    }
}

// ============================================================================
// Layer 1: Topology (zero coordinates)
// ============================================================================

/// Trunk direction: derived from the anchor's pin edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrunkAxis {
    /// Pins on left/right edge → trunk is vertical
    Vertical,
    /// Pins on top/bottom edge → trunk is horizontal
    Horizontal,
}

/// A group of pins on the same box.
#[derive(Debug, Clone)]
pub struct PinGroup {
    pub box_id: i64,
    /// Pin IDs sorted by pin number (deterministic)
    pub pin_ids: Vec<i64>,
    /// Number of pins in this group
    pub pin_count: usize,
}

/// Terminal symbol type (topology level, no coordinates).
#[derive(Debug, Clone)]
pub enum Terminal {
    Ground,
    NetLabel(String),
    Port { name: String },
}

/// Net topology: pure logic, zero coordinates.
#[derive(Debug, Clone)]
pub struct NetTopology {
    /// ★ M0: the net's nid. `net_name` is not unique (`moddcdc` has five
    /// separate `GND` nets); the observatory and later passes need to tell
    /// them apart.
    pub nid: i64,
    pub net_name: String,
    pub net_kind: NetKind,
    /// ★ Power rail (DC-interface Power member, from the projection-layer
    /// rail spec): renders as a bus label, even with a single real group.
    pub is_power_rail: bool,
    /// Anchor box ID
    pub anchor: i64,
    /// Groups: anchor first, then non-anchor sorted by box_id (deterministic)
    pub groups: Vec<PinGroup>,
    /// Terminal symbols for this net
    pub terminals: Vec<Terminal>,
    /// Trunk axis derived from anchor's pin edge
    pub trunk_axis: TrunkAxis,
    /// ★ P3: trunk lane — single source of truth for trunk coordinates.
    /// Written once by `resolve_lanes`; place_members/realize read it.
    pub lane: Lane,
    /// ★ M1: whether this net's lane was resolved with its anchor box already
    /// placed. `place_by_topology` resolves lanes in dependency order, so this
    /// is true for every net whose anchor ever got placed; a net whose anchor
    /// was never placed (e.g. a passive that no net placed) keeps `false`. The
    /// observatory exposes it as the NETS table `plcd` column.
    pub(crate) anchor_placed: bool,
    /// ★ M2: single real group + terminal symbols — the "terminal only" shape.
    /// Such a net has no trunk to draw: the glyph hangs directly off the anchor
    /// pin on a short stub (`moddcdc` 502/503/504). Set at topology build so the
    /// layout and render sides agree; `assign_rows` skips it, `realize` draws
    /// only the stub.
    pub terminal_only: bool,
    /// ★ M2 (B1 fix): how this net's row was produced (`None` = no row yet).
    /// The island fallback is the only source A1 treats as `rows_fallback` — a
    /// free net that found a partner is fine, one that fell back to "below the
    /// IC" is a real fallback.
    pub(crate) row_source: RowSource,
    /// ★ M8.2: the `nid` of the RUN this net belongs to (its own nid when it is
    /// the run root, or a ground net). Two nets with the same `run_root` are
    /// collinear: the part between them lies ALONG the row and their trunk
    /// spans meet on its two pins. Written by `assign_rows` from
    /// [`super::equi_chain::analyse`]; both the layout and the render replay run
    /// `assign_rows`, so it is consistent on both sides (A2 stays green).
    pub(crate) run_root: i64,
    /// ★ M8.2: hops from the run root (0 = the root). Orders the run outward:
    /// depth `d` sits strictly further from the anchor than depth `d - 1`.
    pub(crate) run_depth: usize,
    /// ★ M11.3: this net's OUTER horizontal end is occupied by something
    /// physical — the `Along` part that continues the wire, or a satellite
    /// component's facing pin. A name on such a row cannot be written ALONG the
    /// wire (it would be painted onto the part), so `realize` pulls it off on a
    /// vertical stub instead. Written by `assign_rows` from
    /// [`super::equi_chain::NetEnds`], right next to `run_root`/`run_depth`, so
    /// the layout and the render replay derive it identically (A2 stays green).
    ///
    /// Up to M10 that decision was GEOMETRIC (`text_collides`): the glyph went
    /// vertical only once its text happened to overlap a box, so the same
    /// netlist rendered two ways depending on how long a name was.
    pub(crate) outer_end_taken: bool,
    /// ★ M12.1: this GROUND net is a COLUMN — a shared ground node carrying two
    /// or more parts, each lying ALONG its own net's row and stopping at the
    /// node's x. Such a net places none of its own members (they belong to the
    /// live rows) and its glyph continues OUTWARD off the row it was adopted
    /// onto. Written by `assign_rows` from [`super::equi_chain::ChainPlan`].
    pub(crate) ground_column: bool,
}

/// M2: where a net's row came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RowSource {
    /// IC-anchored West/East net, row on the side pin.
    SidePin,
    /// IC-anchored North/South net, row on the edge rail.
    EdgeRail,
    /// Free net inheriting a partner net's row (partner nid).
    Partner(i64),
    /// Free net with no partner — deterministic fallback below the IC. This is
    /// the `rows_fallback` that A1 counts.
    IslandFallback,
}

/// M2.5 Step 2: the row-allocation result, consumed by `assign_anchor_slots`
/// (and later by `realize` for the ground band). Single source of truth for the
/// layer anchor's vertical extent so `assign_rows` and `assign_anchor_slots`
/// can never drift (B4).
#[derive(Debug, Clone, Default)]
pub(crate) struct RowPlan {
    /// pin_id → the side (West/East) pin's row y. Only layer-anchor side pins;
    /// South/North pins sit on the box edge, NC pins are handled separately.
    pub pin_rows: BTreeMap<i64, f64>,
    /// Layer-anchor vertical extent (top edge y, bottom edge y), from the side
    /// rows.
    pub ic_top: f64,
    pub ic_bottom: f64,
    /// M3.2: the ordered side/free band sequence (W/E side bands share an
    /// index; free nets append after their partner). Carries the up/down
    /// corridor demand so A11/A12 can be checked as a pure RowPlan self-test.
    pub bands: Vec<RowBand>,
    /// M3.2: net index → band index (rows not on the side/free sequence —
    /// North/South rails — are `None` here).
    pub net_band: Vec<Option<usize>>,
}

/// M3.2: one row band — the horizontal strip a trunk occupies, with the
/// vertical corridor its members need below (`down`) and above (`up`).
#[derive(Debug, Clone, Default)]
pub(crate) struct RowBand {
    /// vertical demand below the row (members hanging South / Bridge-Drop down).
    pub down: f64,
    /// vertical demand above the row (members hanging North / Bridge-Drop up).
    pub up: f64,
    /// (nid, region) of every net on this band.
    pub occupants: Vec<(i64, Region)>,
}

/// Build topology for all nets. Zero coordinates — does not read b.x / b.y / b.w / b.h.
pub fn build_topology(graph: &McVecGraph) -> Vec<NetTopology> {
    let mut topos = Vec::new();

    for net in &graph.nets {
        // Skip nets with <1 real endpoint (need at least one box to anchor)
        let real_count = net
            .endpoints
            .iter()
            .filter(|ep| {
                graph
                    .boxes
                    .iter()
                    .any(|b| b.id == ep.box_id && b.kind != BoxKind::PowerLabel)
            })
            .count();
        if real_count < 1 {
            continue;
        }

        if let Some(topo) = build_one_topology(net, graph) {
            topos.push(topo);
        }
    }

    topos
}

/// ★ S9/F5b: whether a net with a single real group (one box endpoint after
/// projection) still renders — as a ground symbol, a power-rail bus label, or
/// a named SubModuleIO port stub — instead of being skipped as a bare dangling
/// pin. The F5 skip (build_one_topology) and the S9 dangling-net metric
/// (renderdiff) share this predicate so the metric exactly mirrors what the
/// Device tree pipeline draws.
pub(crate) fn single_group_net_renders_stub(net: &VizNet) -> bool {
    net.kind == NetKind::Ground
        || net.kind == NetKind::Power
        || net
            .rail
            .as_ref()
            .is_some_and(|r| r.class == RailClass::Power)
        || (net.kind == NetKind::SubModuleIO
            && !net.name.is_empty()
            && !crate::instant::mc_net::is_anon_net_name(&net.name))
}

fn build_one_topology(net: &VizNet, graph: &McVecGraph) -> Option<NetTopology> {
    // Group real-box endpoints by box_id (BTreeMap for determinism)
    let mut real_groups: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    let mut terminals: Vec<Terminal> = Vec::new();

    // ★ Power-rail net: a DC-interface Power member (e.g. `vin.POWER_SYS` from
    // `io vin{POWER_SYS, GND}::DC(5V)`). The projection layer drops the
    // port-side pseudo endpoint (rail rule (c)), so such a net may carry a
    // single real group — it still renders, terminated by a bus label.
    let is_power_rail_net = net
        .rail
        .as_ref()
        .is_some_and(|r| r.class == RailClass::Power)
        || net.kind == NetKind::Power;

    for ep in &net.endpoints {
        let Some(b) = graph.boxes.iter().find(|b| b.id == ep.box_id) else {
            continue;
        };
        if b.kind == BoxKind::PowerLabel || b.kind == BoxKind::PortTerminal {
            // ★ Label-kind endpoints (PowerLabel / PortTerminal) become terminal
            // symbols, so they are NOT placed/laid out as physical boxes. A
            // PortTerminal such as `USB_VBUS` (from `usbsock.VBUS -> USB_VBUS`)
            // is an inline net label, not a component — it must render as a
            // BusLabel/NetLabel circle+text, not a square box.
            match &net.kind {
                NetKind::Ground => {
                    if !terminals.iter().any(|t| matches!(t, Terminal::Ground)) {
                        terminals.push(Terminal::Ground);
                    }
                }
                _ => {
                    if !terminals.iter().any(|t| matches!(t, Terminal::NetLabel(_))) {
                        terminals.push(Terminal::NetLabel(net.name.clone()));
                    }
                }
            }
        } else {
            real_groups.entry(ep.box_id).or_default().push(ep.pin_id);
        }
    }

    if real_groups.is_empty() {
        return None;
    }

    // Sort pin IDs within each group for determinism
    for pins in real_groups.values_mut() {
        pins.sort();
    }

    // Anchor: most pins, tiebreak: degree → source_line → box_id
    let anchor = select_anchor_deterministic(&real_groups, net, graph);

    // Build pin groups: anchor first, then non-anchor sorted by box_id
    let mut groups: Vec<PinGroup> = Vec::new();

    // Anchor group first
    if let Some(pins) = real_groups.get(&anchor) {
        groups.push(PinGroup {
            box_id: anchor,
            pin_ids: pins.clone(),
            pin_count: pins.len(),
        });
    }

    // Non-anchor groups sorted by box_id
    let mut non_anchor: Vec<(i64, &Vec<i64>)> = real_groups
        .iter()
        .filter(|(&id, _)| id != anchor)
        .map(|(&id, pins)| (id, pins))
        .collect();
    non_anchor.sort_by_key(|(id, _)| *id);

    for (box_id, pins) in non_anchor {
        groups.push(PinGroup {
            box_id,
            pin_ids: pins.clone(),
            pin_count: pins.len(),
        });
    }

    // ★ F5: skip nets with only the anchor (no non-anchor groups)
    // BUT only if there are no terminals — a power net with a label should still render.
    // Power-rail nets (single real group after the port pseudo endpoint was
    // dropped by projection) are kept: they render as anchor + bus label.
    // ★ PR3: Ground nets are always kept too — a single-group ground net (e.g.
    // the input decoupling cap's ground after the GND label pseudo endpoint is
    // projected away) must still render its ground symbol.
    if groups.len() < 2 && terminals.is_empty() && !single_group_net_renders_stub(net) {
        return None;
    }

    // Trunk axis: derived from anchor's entry_points
    let trunk_axis = trunk_axis_from_anchor(anchor, &real_groups[&anchor], graph);

    // Add net-kind-based terminals (only if not already added from PowerLabel)
    if is_power_rail_net {
        // ★ Power rail: bus label stripped of the port prefix
        // ("vin.POWER_SYS" -> "POWER_SYS", "V3V3.VCC" -> "VCC")
        if !terminals.iter().any(|t| matches!(t, Terminal::NetLabel(_))) {
            let label = net
                .name
                .rsplit_once('.')
                .map_or(net.name.clone(), |(_, leaf)| leaf.to_string());
            terminals.push(Terminal::NetLabel(label));
        }
    } else {
        match &net.kind {
            NetKind::Ground => {
                if !terminals.iter().any(|t| matches!(t, Terminal::Ground)) {
                    terminals.push(Terminal::Ground);
                }
            }
            NetKind::Signal => {
                // ★ M7.6: skip anonymous `_net{N}` nets (the old `starts_with("__net")`
                // never matched — the builder names them `_net5` etc., single
                // underscore + digit). An engine-generated net has nothing
                // meaningful to label the trunk with.
                if !net.name.is_empty()
                    && !crate::instant::mc_net::is_anon_net_name(&net.name)
                    && !terminals.iter().any(|t| matches!(t, Terminal::NetLabel(_)))
                {
                    terminals.push(Terminal::NetLabel(net.name.clone()));
                }
            }
            NetKind::SubModuleIO => {
                // ★ The PortTerminal endpoint (e.g. USB_VBUS) was already
                // extracted as a NetLabel above; do not emit a duplicate Port
                // label that would overlap it.
                //
                // ★ M7.6: also skip anonymous `_net{N}` ports (the builder maps
                // unnamed cross-module hops to SubModuleIO; those have no real
                // port name to display).
                if !net.name.is_empty()
                    && !crate::instant::mc_net::is_anon_net_name(&net.name)
                    && !terminals
                        .iter()
                        .any(|t| matches!(t, Terminal::Port { .. } | Terminal::NetLabel(_)))
                {
                    terminals.push(Terminal::Port {
                        name: net.name.clone(),
                    });
                }
            }
            _ => {}
        }
    }

    Some(NetTopology {
        nid: net.nid,
        net_name: net.name.clone(),
        net_kind: net.kind.clone(),
        is_power_rail: is_power_rail_net,
        anchor,
        groups,
        terminals,
        trunk_axis,
        lane: Lane::default(),
        anchor_placed: false,
        // Corrected in `assign_rows`: terminal_only needs the layer anchor
        // (a single-group net anchored on the IC is a real net, not
        // terminal-only).
        terminal_only: false,
        row_source: RowSource::IslandFallback,
        run_root: net.nid,
        run_depth: 0,
        outer_end_taken: false,
        ground_column: false,
    })
}

/// Select anchor deterministically: most pins, tiebreak: degree → source_line → box_id.
fn select_anchor_deterministic(
    groups: &BTreeMap<i64, Vec<i64>>,
    net: &VizNet,
    graph: &McVecGraph,
) -> i64 {
    // Compute degree for each box (number of nets it connects to)
    let mut degree: BTreeMap<i64, usize> = BTreeMap::new();
    for n in &graph.nets {
        for ep in &n.endpoints {
            if groups.contains_key(&ep.box_id) {
                *degree.entry(ep.box_id).or_default() += 1;
            }
        }
    }

    // Find source line for each endpoint (lowest source_line wins)
    // Use pin_number as a deterministic proxy for source ordering
    let source_line: BTreeMap<i64, usize> = net
        .endpoints
        .iter()
        .filter(|ep| groups.contains_key(&ep.box_id))
        .map(|ep| (ep.box_id, ep.pin_number.unwrap_or(0) as usize))
        .collect();
    // ★ M15.5: total pins each candidate box carries, so a net whose endpoint
    // groups are all single-pin ties resolves toward the real component instead
    // of a 2-pin passive (`mic`'s `MIC.N` net has `_R1`, `C1`, `dio2` and `mic`
    // each at one pin; without this the 0R resistor can win the anchor, and the
    // layer then anchors on the wrong side of the net).
    let box_pins: BTreeMap<i64, usize> = graph
        .boxes
        .iter()
        .filter(|b| groups.contains_key(&b.id))
        .map(|b| (b.id, b.pins.len()))
        .collect();

    groups
        .iter()
        .max_by(|(id_a, pins_a), (id_b, pins_b)| {
            pins_a
                .len()
                .cmp(&pins_b.len())
                .then_with(|| {
                    degree
                        .get(id_a)
                        .unwrap_or(&0)
                        .cmp(degree.get(id_b).unwrap_or(&0))
                })
                .then_with(|| {
                    box_pins
                        .get(id_a)
                        .unwrap_or(&0)
                        .cmp(box_pins.get(id_b).unwrap_or(&0))
                })
                .then_with(|| {
                    // ★ F5: lower source_line wins — reverse order for max_by
                    source_line
                        .get(id_b)
                        .unwrap_or(&0)
                        .cmp(source_line.get(id_a).unwrap_or(&0))
                })
                .then_with(|| id_a.cmp(id_b))
        })
        .map(|(&id, _)| id)
        .unwrap_or(0)
}

/// Determine trunk axis from anchor's pin count (topology), NOT from entry_points.
/// Device layer: anchor pins are all on the right side → Vertical trunk for multi-pin,
/// horizontal for single-pin (trunk is the line from pin to junction).
fn trunk_axis_from_anchor(_anchor_id: i64, pin_ids: &[i64], _graph: &McVecGraph) -> TrunkAxis {
    if pin_ids.len() > 1 {
        TrunkAxis::Vertical
    } else {
        TrunkAxis::Horizontal
    }
}

// ============================================================================
// Layer 2: Layout (topology determines coordinates)
// ============================================================================

/// ★ P1: assign each net's Region from its electrical role (device_layout_v2.md sec.3).
/// Pure function of (graph, topos) — layout and render both replay it, so the
/// regions are guaranteed identical. Rules, in order:
///   0. Ground net                      → South  (ungated: pure function of kind)
///   1. Ground pin on the anchor net    → South
///   2. Power net, anchor is driver     → East  (the rail leaves the anchor)
///   3. Power net, anchor not driver    → West  (the rail enters the anchor)
///   4. Input pin                       → West
///   5. Output / Bidir / Bus pin        → East
///   6. Passive / Unknown               → balance (W/E with fewer pins so far)
///   7. Net not touching the layer anchor → inherit the region of a net that
///      shares one of its member boxes (lane-index smallest, then name).
///   Fallback: East (with a `[region] fallback` log — a hit means the design
///   missed a rule class).
pub fn assign_regions(graph: &McVecGraph, topos: &mut [NetTopology]) -> usize {
    let layer_anchor = layer_anchor_id(topos);
    let mut resolved: Vec<bool> = vec![false; topos.len()];
    let mut west_pins: BTreeMap<i64, usize> = BTreeMap::new();
    let mut east_pins: BTreeMap<i64, usize> = BTreeMap::new();

    // Pass 0: ungated ground rule — every Ground net hangs South, whether or
    // not it touches the layer anchor. `direct_region`'s Ground→South branch is
    // gated on the layer anchor (Pass 1), so the other `moddcdc` GND nets used
    // to inherit W/E from a partner net and were then patched back to South
    // inside `other_net_info`. Hoisting the rule here makes the region a pure
    // function of net kind and deletes that patch.
    for (i, topo) in topos.iter_mut().enumerate() {
        if topo.net_kind == NetKind::Ground {
            topo.lane.region = Region::South;
            resolved[i] = true;
        }
    }

    // Pass 1: direct rules for nets that touch the layer anchor.
    // ★ M7.1: `strength[i]` records HOW firmly the choice is pinned, so Pass 1.5
    // knows which of two coupled nets may be pulled across.
    let mut strength: Vec<SideStrength> = vec![SideStrength::Balanced; topos.len()];
    for (i, topo) in topos.iter_mut().enumerate() {
        if resolved[i] {
            continue;
        }
        let touches = topo.groups.iter().any(|g| g.box_id == layer_anchor);
        if !touches {
            continue;
        }
        let region = match direct_region(graph, topo) {
            Some((r, s)) => {
                strength[i] = s;
                r
            }
            // Passive / Unknown → balance: W/E with fewer pins so far.
            None => {
                let w = west_pins.get(&topo.anchor).copied().unwrap_or(0);
                let e = east_pins.get(&topo.anchor).copied().unwrap_or(0);
                if w <= e {
                    Region::West
                } else {
                    Region::East
                }
            }
        };
        topo.lane.region = region;
        resolved[i] = true;
        let pin_cnt = topo.groups.first().map(|g| g.pin_ids.len()).unwrap_or(1);
        match region {
            Region::West => *west_pins.entry(topo.anchor).or_default() += pin_cnt,
            Region::East => *east_pins.entry(topo.anchor).or_default() += pin_cnt,
            _ => {}
        }
    }

    // ── Pass 1.5 (★ M7.1): netlist-driven side coupling ──────────────────────
    //
    // The side decision must START FROM THE NETLIST, not from each pin's IO
    // direction read in isolation. Two nets that share a TWO-PIN member form a
    // loop through that component:
    //
    //     moddcdc   VDD_3V3{lp322dcdc.4, _R1.1}  ~  _net1{_R1.2, lp322dcdc.1}
    //                 → pin 4 and pin 1 have a resistor between them
    //     moddcdc   _net3{lp322dcdc.3, _C5.2}    ~  _net5{_C5.1, lp322dcdc.5}
    //                 → pin 3 and pin 5 have a capacitor between them
    //
    // On OPPOSITE sides that component has to stretch across the whole IC (the
    // cross-side residue A7 has tolerated since M3). On the SAME side the two
    // nets land on adjacent bands and the component becomes one short vertical
    // Bridge between them — which is how the schematic is meant to read.
    //
    // Only W/E nets on the layer anchor take part; Ground (South) and the N/S
    // rails are positional, not side decisions. Each net moves at most once, so
    // the pass cannot oscillate, and pairs are visited in index order, so it is
    // deterministic (A2: still no rect is read anywhere here).
    {
        // A coupling may not push the two sides more than this many pins
        // apart: the IC height is `max(west, east)` pins, so draining one side
        // doubles the box for no gain.
        const SIDE_IMBALANCE_MAX: isize = 2;

        let pairs = coupled_net_pairs(graph, topos, layer_anchor);
        let mut moved = vec![false; topos.len()];
        let anchor_pins =
            |t: &NetTopology| t.groups.first().map_or(1, |g| g.pin_ids.len()) as isize;
        for (i, j) in pairs {
            if !is_w_e_opposite(topos[i].lane.region, topos[j].lane.region) {
                continue; // already the same side (or one of them is a N/S rail)
            }
            // The weaker pin gives way; equal strength falls back to the anchor
            // pin's electrical rank (a source outranks a sink), `nid` last.
            let key_i = (strength[i], anchor_io_rank(graph, &topos[i]));
            let key_j = (strength[j], anchor_io_rank(graph, &topos[j]));
            let (loser, winner) = match key_i.cmp(&key_j) {
                std::cmp::Ordering::Less => (i, j),
                std::cmp::Ordering::Greater => (j, i),
                std::cmp::Ordering::Equal => {
                    if topos[i].nid > topos[j].nid {
                        (i, j)
                    } else {
                        (j, i)
                    }
                }
            };
            if moved[loser] || strength[loser] >= SideStrength::RailDriver {
                continue;
            }
            let target = topos[winner].lane.region;
            // Balance guard. Only Pass-1 nets are counted: Pass 2 has not run
            // yet, so an unresolved net still carries `Lane::default()`'s East
            // and would poison the tally.
            //
            // ★ FIX: keep the base rule (a coupling may not create a side
            // imbalance past `SIDE_IMBALANCE_MAX`), but WAIVE it when the move
            // does not grow the IC box — the box height is `max(west,east)`.
            // mcu513's GPIO.20 address net `_net19` is coupled to `VDD_3V3`
            // (pin5) through `_R1`; ignoring that coupling stranded `_R1` on the
            // opposite side and the net's trunk ran straight across the chip.
            // The layer is already East-heavy, so pulling `_net19` West keeps the
            // post-move difference > 2 but SHRINKS the box (15→14). Letting any
            // box-shrinking coupling through fixes the cross-IC bridge while a
            // box-growing drain is still vetoed.
            let (mut w, mut e) = (0isize, 0isize);
            for (k, t) in topos.iter().enumerate() {
                if !resolved[k] {
                    continue;
                }
                match t.lane.region {
                    Region::West => w += anchor_pins(t),
                    Region::East => e += anchor_pins(t),
                    _ => {}
                }
            }
            let d = anchor_pins(&topos[loser]);
            let box_before = w.max(e);
            let (w, e) = match target {
                Region::West => (w + d, e - d),
                _ => (w - d, e + d),
            };
            if (w - e).abs() > SIDE_IMBALANCE_MAX && w.max(e) > box_before {
                crate::vlog!(
                    "[region] couple skipped (balance): net '{}' would grow box W/E {}/{}",
                    topos[loser].net_name,
                    w,
                    e
                );
                continue;
            }
            crate::vlog!(
                "[region] couple: net '{}' {:?} → {:?} (shares a 2-pin part with '{}')",
                topos[loser].net_name,
                topos[loser].lane.region,
                target,
                topos[winner].net_name
            );
            topos[loser].lane.region = target;
            moved[loser] = true;
        }
    }

    // ── Pass 1.6 (★ M10.2): the satellite side belongs to the shared nets ────
    //
    // `lpa.VDD` is an `IoDirection::Power` pin, so `direct_region` puts it WEST —
    // and `spk` is ALSO west, because that is where the two shared output nets
    // (`_net8` at VO1, `_net9` at VO2) live. The power net has nothing to do with
    // `spk`: forcing it through the same corridor stacks its rail label, its
    // decoupling cap, the two ESD diodes and the two test points into one column
    // band, which is how `VDD_3V3` ended up behind `_DIO_ESD2` and `TP1`.
    //
    // The rule is the one M9 already applies to the SATELLITE's own pins
    // (`facing` / `away`), pointed at the ANCHOR's pins instead: **a pin that has
    // to reach the other component goes between the two components; one that does
    // not gets pushed to the far side.**
    //
    // Only nets outside the satellite's COUPLING CLOSURE move: `IN.N` shares a
    // feedback resistor with `VO1`, so it stays west even though it is not itself
    // a shared net (moving it would stretch that resistor across the whole IC —
    // exactly the cross-side residue M7.1 exists to kill). `RailDriver` strength
    // never moves, and the same `SIDE_IMBALANCE_MAX` guard applies.
    //
    // A no-op on a layer with no satellite (`plan_satellites` returns empty), so
    // `moddcdc` / `modldo` / buck do not move a pixel.
    {
        const SIDE_IMBALANCE_MAX: isize = 2;
        let anchor_pins =
            |t: &NetTopology| t.groups.first().map_or(1, |g| g.pin_ids.len()) as isize;
        let claims = satellite_side_claims(graph, topos, layer_anchor);
        let mut yielded = vec![false; topos.len()];
        let mut moved_here = vec![false; topos.len()];
        // The balance tally, recomputed on demand: only Pass-1 nets count, since
        // an unresolved net still carries `Lane::default()`'s East.
        let tally = |topos: &[NetTopology], resolved: &[bool]| -> (isize, isize) {
            let (mut w, mut e) = (0isize, 0isize);
            for (k, t) in topos.iter().enumerate() {
                if !resolved[k] {
                    continue;
                }
                match t.lane.region {
                    Region::West => w += anchor_pins(t),
                    Region::East => e += anchor_pins(t),
                    _ => {}
                }
            }
            (w, e)
        };
        for (region, keep) in claims {
            let opposite = match region {
                Region::West => Region::East,
                _ => Region::West,
            };

            // ── ★ M13.1: PULL the bridge closure IN, before pushing anything out
            //
            // M10.2 only ever pushed: nets with no business next to the satellite
            // yielded its side. It never asked the opposite question — whether a
            // net that DOES have business there is stranded on the far side.
            //
            // `mic`: `MIC.P` runs to `wm7121.1`, and `MIC.N` is bridged to `MIC.P`
            // through `C1`. `direct_region` reads `mic.2` in isolation, sees an
            // input, and files it West; the microphone's differential pair then
            // straddles the IC and `C1` has to reach across it.
            //
            // > when placing a pin, look not only at whether it connects to another
            // > component, but also at whether its BRIDGE reaches a pin that belongs
            // > to that other component.
            //
            // Which is what `keep` already is — `satellite_side_claims` closes the
            // shared nets under `coupled_net_pairs`, so `MIC.N` is in it. The
            // closure was being used only as a veto ("do not push these out"); as
            // an attractor it puts the whole differential pair on the side facing
            // the part it talks to, and `C1` becomes one short Bridge between two
            // adjacent rows, drawn between the two components.
            //
            // Same three guards as the push: a `RailDriver` never moves, a net
            // moves at most once, and the side balance may not tip past
            // `SIDE_IMBALANCE_MAX`.
            for i in 0..topos.len() {
                if moved_here[i] || !resolved[i] || !keep.contains(&i) {
                    continue;
                }
                if !is_w_e_opposite(topos[i].lane.region, region)
                    || strength[i] >= SideStrength::RailDriver
                {
                    continue;
                }
                let (w, e) = tally(topos, &resolved);
                let d = anchor_pins(&topos[i]);
                let (w, e) = match region {
                    Region::West => (w + d, e - d),
                    _ => (w - d, e + d),
                };
                if (w - e).abs() > SIDE_IMBALANCE_MAX {
                    crate::vlog!(
                        "[region] satellite pull skipped (balance): net '{}' would make W/E {}/{}",
                        topos[i].net_name,
                        w,
                        e
                    );
                    continue;
                }
                crate::vlog!(
                    "[region] satellite pull: net '{}' {:?} → {:?} (bridged to a net the \
                     component on {:?} shares)",
                    topos[i].net_name,
                    topos[i].lane.region,
                    region,
                    region
                );
                topos[i].lane.region = region;
                moved_here[i] = true;
            }

            for i in 0..topos.len() {
                if yielded[i] || moved_here[i] || keep.contains(&i) || !resolved[i] {
                    continue;
                }
                if topos[i].lane.region != region || strength[i] >= SideStrength::RailDriver {
                    continue;
                }
                let (w, e) = tally(topos, &resolved);
                let d = anchor_pins(&topos[i]);
                let (w, e) = match opposite {
                    Region::West => (w + d, e - d),
                    _ => (w - d, e + d),
                };
                if (w - e).abs() > SIDE_IMBALANCE_MAX {
                    crate::vlog!(
                        "[region] satellite yield skipped (balance): net '{}' would make W/E {}/{}",
                        topos[i].net_name,
                        w,
                        e
                    );
                    continue;
                }
                crate::vlog!(
                    "[region] satellite yield: net '{}' {:?} → {:?} (shares nothing with the \
                     component on {:?})",
                    topos[i].net_name,
                    region,
                    opposite,
                    region
                );
                topos[i].lane.region = opposite;
                yielded[i] = true;
                moved_here[i] = true;
            }
        }
    }

    // ── Pass 1.6b (★ M14.4): a SATELLITE's own nets take their side from the
    // satellite, not from whoever they happen to inherit from ─────────────────
    //
    // A satellite's non-shared nets touch no anchor pin, so Pass 1 skips them
    // and Pass 2 hands them whatever a member-sharing neighbour happens to have.
    // That is a coin flip, and on `mic` it lands wrong: `MIC.N` inherits West
    // from `MIC.P`, so `place_members_for_topo` grows its members WESTWARD from
    // `mic.2` — straight back through the microphone.
    //
    // The satellite already knows the answer. A pin that FACES the parent has
    // its wire in the GAP between the two components, so its net's members grow
    // toward the parent: the side OPPOSITE the satellite's own. A pin pointing
    // AWAY grows away, on the satellite's side. So:
    //
    //     mic (West of wm7121)
    //       mic.2 MIC.N  facing → East → C1 / _R1 land in the gap  ★ "in between"
    //       mic.3 _net2  away   → West → its ESD diode sits west of mic
    //
    // Pure topology: reads the satellite plan and the parent's region, no rect.
    {
        for (sat, region) in satellite_plan_for(graph, topos, layer_anchor) {
            let facing_region = match region {
                Region::West => Region::East,
                _ => Region::West,
            };
            let facing_nets: BTreeSet<usize> = sat
                .bridged
                .iter()
                .copied()
                .filter(|&n| topos.get(n).is_some_and(|t| t.net_kind != NetKind::Ground))
                .collect();
            let away_nets: BTreeSet<usize> = sat
                .away
                .iter()
                .filter_map(|&pid| {
                    topos.iter().position(|t| {
                        t.groups
                            .iter()
                            .any(|g| g.box_id == sat.box_id && g.pin_ids.contains(&pid))
                    })
                })
                .filter(|&n| topos[n].net_kind != NetKind::Ground && !topos[n].terminal_only)
                .collect();
            for (n, r) in facing_nets
                .into_iter()
                .map(|n| (n, facing_region))
                .chain(away_nets.into_iter().map(|n| (n, region)))
            {
                // Never touch a net that owns an anchor pin — Pass 1 decided it,
                // and the satellite has no standing to overrule the IC.
                if resolved[n] || topos[n].groups.iter().any(|g| g.box_id == layer_anchor) {
                    continue;
                }
                crate::vlog!(
                    "[region] satellite net: '{}' → {:?} (pin on '{}', {:?} of the anchor)",
                    topos[n].net_name,
                    r,
                    sat.box_id,
                    region
                );
                topos[n].lane.region = r;
                resolved[n] = true;
            }
        }
    }

    // ── Pass 1.75 (★ M10.3): an ADOPTED ground lives on its run's ROW ────────
    //
    // Pass 0 sends every Ground net South unconditionally. That is right for a
    // net hanging off a real IC GND pin, and wrong for the "cap into a ground
    // glyph" shape: the chain analyser has decided that such a ground is the
    // OUTER END of a W/E run (`equi_chain` step 3.5), so it has to share that
    // run's row or the part between them cannot be collinear.
    //
    // `chain_plan_for` is pure topology (net kind, groups, pin IO, endpoint box
    // kinds, satellite membership) and reads no region, so running it here —
    // before `assign_rows` runs it again — cannot produce a different answer.
    // A2 is untouched.
    {
        let chain = chain_plan_for(graph, topos, layer_anchor);
        let adopt: Vec<(usize, Region)> = (0..topos.len())
            .filter(|&i| topos[i].net_kind == NetKind::Ground)
            .filter_map(|i| {
                let r = chain.region.get(i).copied().flatten()?;
                if r == i || !resolved[r] {
                    return None;
                }
                match topos[r].lane.region {
                    reg @ (Region::West | Region::East) => Some((i, reg)),
                    _ => None,
                }
            })
            .collect();
        for (i, reg) in adopt {
            crate::vlog!(
                "[region] ground '{}' adopted as a run end → {:?} (was South)",
                topos[i].net_name,
                reg
            );
            topos[i].lane.region = reg;
        }
    }

    // Pass 2: inheritance — nets not touching the layer anchor share a member
    // box with a regioned net; inherit its region. Iterate to a fixed point
    // (a net's partner may itself be resolved by inheritance).
    //
    // ★ M16: a net resolved by inheritance is WEAK — it may be revised once a
    // better partner resolves. The DAC junction net `_net27` used to stick with
    // South because GND (resolved in Pass 0, lowest nid) was the only candidate
    // when `_net27`'s turn came before its `_net31` partner; the row side only
    // became available later. Re-examining weak nets each sweep lets the best
    // partner (W/E preferred, see [`inherited_region`]) win regardless of
    // resolution order. Strong resolutions (Pass 0 ground, Pass 1 anchor nets,
    // satellite / adopted) are never revised.
    let mut weak: Vec<bool> = vec![false; topos.len()];
    for _ in 0..topos.len() {
        let mut changed = false;
        for i in 0..topos.len() {
            if resolved[i] && !weak[i] {
                continue;
            }
            let Some(r) = inherited_region(topos, i, &resolved) else {
                continue;
            };
            if resolved[i] {
                if topos[i].lane.region == r {
                    continue;
                }
                crate::vlog!(
                    "[region] revise '{}' {:?} → {:?} (a W/E partner resolved)",
                    topos[i].net_name,
                    topos[i].lane.region,
                    r
                );
            }
            topos[i].lane.region = r;
            resolved[i] = true;
            weak[i] = true;
            changed = true;
        }
        if !changed {
            break;
        }
    }

    // Pass 3: fallback — a hit here means the design missed a rule class.
    // The count is returned so assertion 6 (fallback == 0) can be tested.
    let mut fallback = 0usize;
    for (i, topo) in topos.iter_mut().enumerate() {
        if !resolved[i] {
            fallback += 1;
            crate::vlog!("[region] fallback: net '{}' → East", topo.net_name);
            topo.lane.region = Region::East;
        }
    }
    fallback
}

/// The layer anchor: the box referenced by the most topologies as anchor.
pub(crate) fn layer_anchor_id(topos: &[NetTopology]) -> i64 {
    let mut counts: BTreeMap<i64, usize> = BTreeMap::new();
    // ★ M15.5: how central each candidate is — the total member groups over the
    // nets it anchors. A 2-pin passive (e.g. `mic`'s `_R1`) can win a net-anchor
    // tie against a real component, and then the satellite machinery
    // (`plan_satellites`) silently bails — its BFS needs a ≥3-pin anchor in
    // `comps`. Break ties toward the more pin-rich, more central box so a layer
    // anchors on its actual component.
    let mut central: BTreeMap<i64, usize> = BTreeMap::new();
    let mut pins: BTreeMap<i64, BTreeSet<i64>> = BTreeMap::new();
    for t in topos {
        *counts.entry(t.anchor).or_default() += 1;
        *central.entry(t.anchor).or_default() += t.groups.len();
        for g in &t.groups {
            pins.entry(g.box_id)
                .or_default()
                .extend(g.pin_ids.iter().copied());
        }
    }
    counts
        .iter()
        .max_by(|(id_a, c_a), (id_b, c_b)| {
            c_a.cmp(c_b)
                .then_with(|| {
                    central
                        .get(id_a)
                        .unwrap_or(&0)
                        .cmp(central.get(id_b).unwrap_or(&0))
                })
                .then_with(|| {
                    pins.get(id_a)
                        .map_or(0, BTreeSet::len)
                        .cmp(&pins.get(id_b).map_or(0, BTreeSet::len))
                })
                .then_with(|| id_a.cmp(id_b))
        })
        .map(|(id, _)| *id)
        .unwrap_or(topos.first().map(|t| t.anchor).unwrap_or(0))
}

/// ★ M7.1: how firmly a net's West/East choice is pinned. The netlist coupling
/// pass may only pull the WEAKER of two coupled nets across; equal strength
/// falls back to [`anchor_io_rank`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SideStrength {
    /// Balance fallback — no usable IO on the anchor pin, free to move.
    Balanced,
    /// A feedback / sense pin. It is named after the node it MEASURES, so its
    /// side follows whatever it is tied back to, not its own IO direction.
    Sense,
    /// A definite IO direction on the anchor pin.
    Io,
    /// A rail with a recorded driver pin on the anchor, or a Ground/N-S rail —
    /// never moved.
    RailDriver,
}

/// ★ M7.1: electrical precedence when two coupled nets are equally strongly
/// pinned — the one whose anchor pin is "more of a source" keeps its side, so
/// e.g. `LX`(Output) holds East and `FB` follows it across.
fn anchor_io_rank(graph: &McVecGraph, topo: &NetTopology) -> u8 {
    match anchor_pin_io(graph, topo) {
        Some(IoDirection::Power) => 4,
        Some(IoDirection::Output) => 3,
        Some(IoDirection::Bidir) => 2,
        Some(IoDirection::Input) => 1,
        _ => 0,
    }
}

/// ★ M7.1: is this net's anchor pin a feedback / sense pin? Plan M3.1 listed
/// `FB` / `SENSE` / `ADJ` as a pin class of its own and M5 item 2 parked it as
/// "semantic pin ordering"; this is that rule. A feedback pin is electrically
/// part of the OUTPUT network (it is always tied back to it through a divider
/// or a compensation cap), so putting it opposite the output is what makes the
/// feedback component stretch across the whole IC.
fn anchor_pin_is_sense(graph: &McVecGraph, topo: &NetTopology) -> bool {
    let Some(b) = graph.boxes.iter().find(|b| b.id == topo.anchor) else {
        return false;
    };
    let Some(g) = topo.groups.first() else {
        return false;
    };
    g.pin_ids.iter().any(|pid| {
        b.pins.iter().find(|p| p.id == *pid).is_some_and(|p| {
            let n = p.description.to_ascii_uppercase();
            n == "FB"
                || n == "VFB"
                || n == "ADJ"
                || n == "VSENSE"
                || n.starts_with("FB_")
                || n.starts_with("SENSE")
        })
    })
}

/// Direct region rules for a net that touches the layer anchor, plus how firmly
/// the choice is pinned (★ M7.1). `None` = Passive/Unknown → caller applies the
/// balance rule (and leaves the strength at [`SideStrength::Balanced`]).
fn direct_region(graph: &McVecGraph, topo: &NetTopology) -> Option<(Region, SideStrength)> {
    if topo.net_kind == NetKind::Ground {
        return Some((Region::South, SideStrength::RailDriver));
    }
    if topo.net_kind == NetKind::Power {
        let driver_on_anchor = find_net(graph, topo.nid)
            .and_then(|n| n.rail.as_ref())
            .and_then(|r| r.driver_pin)
            .is_some_and(|dp| topo.groups.first().is_some_and(|g| g.pin_ids.contains(&dp)));
        if driver_on_anchor {
            return Some((Region::East, SideStrength::RailDriver));
        }
        // ★ Fall back to the anchor pin IO when no rail driver is recorded
        // (module-internal rails often carry `rail: None`): an Output pin means
        // the anchor drives the rail → East; anything else enters → West.
        return match anchor_pin_io(graph, topo) {
            Some(IoDirection::Output) | Some(IoDirection::Bidir) => {
                Some((Region::East, SideStrength::Io))
            }
            _ => Some((Region::West, SideStrength::Io)),
        };
    }
    // ★ M7.1: a sense pin sits with the output it measures, and only weakly —
    // the coupling pass may still pull it to whichever net it actually shares a
    // component with.
    if anchor_pin_is_sense(graph, topo) {
        return Some((Region::East, SideStrength::Sense));
    }
    match anchor_pin_io(graph, topo) {
        Some(IoDirection::Input) => Some((Region::West, SideStrength::Io)),
        Some(IoDirection::Output | IoDirection::Bidir) => Some((Region::East, SideStrength::Io)),
        Some(IoDirection::Power) => Some((Region::West, SideStrength::Io)),
        // ★ M13.2: a pin DECLARED as ground whose NET is not a ground net.
        //
        // The `net_kind == Ground` arm at the top of this function has already
        // taken every real ground, so reaching here means the netlist says
        // otherwise: `mic.3` and `mic.4` are the microphone's GND pins, but each
        // one goes through an ESD diode first, so `_net2` / `_net3` are ordinary
        // signal nets. Sending them South on the strength of the pin's NAME gave
        // them a rail below the IC while `assign_anchor_slots` kept their pins on
        // the West edge — the wire then left the pin westward, ran the full
        // height of the box down the left margin and came back east along the
        // rail to reach a diode parked under the canvas. (A27, "a pin lies on its
        // own net's row", is exactly this defect; `mic` is not one of the four
        // fixtures, so nothing was watching.)
        //
        // A ground-declared pin on a signal net is a RETURN pin. West, and only
        // `Io`-weak, so the coupling passes can still move it.
        Some(IoDirection::Ground) => Some((Region::West, SideStrength::Io)),
        _ => None,
    }
}

/// ★ M7.1: the two-pin member boxes a net hangs off the layer anchor. Two nets
/// that share one of these are **coupled** — the netlist holds a loop
/// `IC.pin_a — component — IC.pin_b`, so the two pins belong on the same side.
fn two_pin_member_boxes(
    graph: &McVecGraph,
    topo: &NetTopology,
    layer_anchor: i64,
) -> BTreeSet<i64> {
    topo.groups
        .iter()
        .filter(|g| g.box_id != layer_anchor)
        .filter(|g| {
            graph
                .boxes
                .iter()
                .find(|b| b.id == g.box_id)
                .is_some_and(|b| b.pins.len() == 2)
        })
        .map(|g| g.box_id)
        .collect()
}

/// ★ M7.1: pairs `(i, j)`, `i < j`, of nets coupled through a shared two-pin
/// component. Both must touch the layer anchor and be side (non-Ground) nets —
/// a cap to ground is a Drop, not a coupling.
///
/// Pure topology: reads group box ids and pin COUNTS, never a rect (A2).
pub(crate) fn coupled_net_pairs(
    graph: &McVecGraph,
    topos: &[NetTopology],
    layer_anchor: i64,
) -> Vec<(usize, usize)> {
    let eligible: Vec<bool> = topos
        .iter()
        .map(|t| {
            t.net_kind != NetKind::Ground
                && !t.terminal_only
                && t.groups.iter().any(|g| g.box_id == layer_anchor)
        })
        .collect();
    let members: Vec<BTreeSet<i64>> = topos
        .iter()
        .enumerate()
        .map(|(i, t)| {
            if eligible[i] {
                two_pin_member_boxes(graph, t, layer_anchor)
            } else {
                BTreeSet::new()
            }
        })
        .collect();
    let mut out = Vec::new();
    for i in 0..topos.len() {
        if !eligible[i] {
            continue;
        }
        for j in (i + 1)..topos.len() {
            if !eligible[j] {
                continue;
            }
            if members[i].intersection(&members[j]).next().is_some() {
                out.push((i, j));
            }
        }
    }
    out
}

/// ★ M11.2: every multi-pin box of this layer, as [`equi_place`] sees it.
///
/// Shared by [`satellite_side_claims`] (M10.2) and [`chain_plan_for`] (M11.2):
/// the chain analyser has to know which nets END at another component before it
/// can decide what is allowed to extend horizontally, and the region pass has to
/// know which side that component took. Same input, same function, so the two
/// can never disagree.
///
/// Pure topology — pin→net membership only, no rect.
///
/// [`equi_place`]: super::equi_place
fn comp_views(graph: &McVecGraph, topos: &[NetTopology]) -> Vec<super::equi_place::CompView> {
    use super::equi_place::CompView;
    let mut comps: Vec<CompView> = Vec::new();
    for b in &graph.boxes {
        if b.pins.len() < 3 {
            continue;
        }
        let mut pins: Vec<(i64, usize)> = Vec::new();
        for p in &b.pins {
            if let Some(nn) = topos.iter().position(|t| {
                t.groups
                    .iter()
                    .any(|g| g.box_id == b.id && g.pin_ids.contains(&p.id))
            }) {
                pins.push((p.id, nn));
            }
        }
        if !pins.is_empty() {
            comps.push(CompView { box_id: b.id, pins });
        }
    }
    comps
}

/// ★ M11.2: the nets a SATELLITE component sits on — "ends at a component".
///
/// M9.2 places a satellite past every member of its side and puts its facing
/// pins on the shared nets' rows, so such a net's OUTER end is spent on a box
/// before any two-pin part gets a look at it. Handing that to `equi_chain` is
/// what stops a run trying to continue horizontally THROUGH a component, and
/// what keeps a rail label from being written on top of one.
fn satellite_nets(graph: &McVecGraph, topos: &[NetTopology], layer_anchor: i64) -> BTreeSet<usize> {
    let net_is_ground: Vec<bool> = topos
        .iter()
        .map(|t| t.net_kind == NetKind::Ground)
        .collect();
    let comps = comp_views(graph, topos);
    let bridged = bridged_net_pairs(graph, topos);
    super::equi_place::plan_satellites(&comps, &net_is_ground, &bridged, layer_anchor)
        .into_iter()
        .flat_map(|s| s.shared)
        .collect()
}

/// ★ M14.1: the two-pin relation over NET INDICES — `(a, b)` for every two-pin
/// box joining two distinct non-ground nets.
///
/// `coupled_net_pairs` answers a narrower question (both nets must own a pin on
/// the LAYER ANCHOR), which is exactly why it could not help here: `MIC.N` hangs
/// off `mic`, and `mic` is a satellite, so the pair `(MIC.P, MIC.N)` was never
/// even generated. This one asks only "is there a part between them".
fn bridged_net_pairs(graph: &McVecGraph, topos: &[NetTopology]) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for b in &graph.boxes {
        if b.pins.len() != 2 {
            continue;
        }
        let mut nets: Vec<usize> = topos
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                t.net_kind != NetKind::Ground && t.groups.iter().any(|g| g.box_id == b.id)
            })
            .map(|(i, _)| i)
            .collect();
        nets.sort_unstable();
        nets.dedup();
        if nets.len() == 2 {
            out.push((nets[0], nets[1]));
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// ★ M10.2: which W/E side each satellite component occupies, and the set of
/// nets that have an electrical reason to stay on that side.
///
/// The side is the majority region of the nets shared with the parent (the same
/// vote `satellite_plan_for` makes later, on the same inputs). The keep-set is
/// the shared nets closed under [`coupled_net_pairs`]: a net tied to a shared net
/// through a two-pin part is part of that side's loop and must not be pulled
/// across.
///
/// Pure topology — `plan_satellites` reads pin→net membership and net kind only,
/// and the vote reads `lane.region`, which Pass 1/1.5 has already written. No
/// rect, so A2 is untouched.
fn satellite_side_claims(
    graph: &McVecGraph,
    topos: &[NetTopology],
    layer_anchor: i64,
) -> Vec<(Region, BTreeSet<usize>)> {
    let net_is_ground: Vec<bool> = topos
        .iter()
        .map(|t| t.net_kind == NetKind::Ground)
        .collect();
    let comps = comp_views(graph, topos);
    let bridged = bridged_net_pairs(graph, topos);
    let pairs = coupled_net_pairs(graph, topos, layer_anchor);
    let mut out: Vec<(Region, BTreeSet<usize>)> = Vec::new();
    for sat in super::equi_place::plan_satellites(&comps, &net_is_ground, &bridged, layer_anchor) {
        let (mut w, mut e) = (0usize, 0usize);
        for &nn in &sat.shared {
            match topos.get(nn).map(|t| t.lane.region) {
                Some(Region::West) => w += 1,
                Some(Region::East) => e += 1,
                _ => {}
            }
        }
        if w == 0 && e == 0 {
            continue;
        }
        let region = if w >= e { Region::West } else { Region::East };
        let mut keep: BTreeSet<usize> = sat.shared.iter().copied().collect();
        loop {
            let before = keep.len();
            for &(i, j) in &pairs {
                if keep.contains(&i) {
                    keep.insert(j);
                }
                if keep.contains(&j) {
                    keep.insert(i);
                }
            }
            if keep.len() == before {
                break;
            }
        }
        out.push((region, keep));
    }
    out
}

/// For a net that does not touch the layer anchor: inherit the region of a net
/// that shares one of its member boxes and is already resolved.
///
/// Candidates are sorted by `(is_terminal_only, nid)`: a single-group
/// (terminal-only) net is not a region source — it has no trunk of its own, its
/// region only decides where its terminal glyph hangs, and letting a multi-group
/// neighbour inherit from it pollutes the neighbour's side (e.g. a Power net
/// inheriting South from a single-cap GND net, `moddcdc` 506). `nid` breaks the
/// tie deterministically — `net_name` is not unique (five `GND` nets). The old
/// sort key `(lane.index, net_name)` was dead: Pass 2 runs before any lane
/// resolution, so `lane.index` was always 0 and the sort degraded to the
/// unstable `net_name` tiebreak.
fn inherited_region(topos: &[NetTopology], idx: usize, resolved: &[bool]) -> Option<Region> {
    let topo = &topos[idx];
    let member_box_ids: Vec<i64> = topo.groups.iter().map(|g| g.box_id).collect();
    let mut candidates: Vec<(bool, u8, i64, Region)> = Vec::new();
    for (j, other) in topos.iter().enumerate() {
        if j == idx || !resolved[j] {
            continue;
        }
        let shares = other
            .groups
            .iter()
            .any(|g| member_box_ids.contains(&g.box_id));
        if shares {
            // ★ M16: prefer a W/E partner over a N/S one. A junction net that
            // shares a two-pin part with BOTH a row net and a hanging rail (e.g.
            // the DAC chain's `_net27`: R2/C7 on the row with `_net31`, C8/R3
            // down to GND) must inherit the ROW side. GND resolves first (Pass 0,
            // nid 0) and used to hand the junction net South, which put its
            // Series members on Top/Bottom slots (box-centre taps, missed M8
            // carve) and dropped the net out of the W/E chain. The trunk of such
            // a net lies on the row, so a W/E partner is the right parent when
            // one exists; a pure N/S net has no W/E partner and keeps South.
            let is_ns = match other.lane.region {
                Region::North | Region::South => 1u8,
                _ => 0,
            };
            candidates.push((
                other.groups.len() == 1, // terminal-only sorts last
                is_ns,
                other.nid,
                other.lane.region,
            ));
        }
    }
    candidates.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
    });
    candidates.first().map(|(_, _, _, r)| *r)
}

/// Majority IO direction of this net's pins on its anchor box.
fn anchor_pin_io(graph: &McVecGraph, topo: &NetTopology) -> Option<IoDirection> {
    let anchor_id = topo.anchor;
    let anchor_box = graph.boxes.iter().find(|b| b.id == anchor_id);
    let mut counts: Vec<(IoDirection, usize)> = Vec::new();
    if let Some(net) = find_net(graph, topo.nid) {
        for ep in &net.endpoints {
            if ep.box_id != anchor_id {
                continue;
            }
            let io = if ep.io_type != IoDirection::Unknown {
                ep.io_type
            } else {
                anchor_box
                    .and_then(|b| b.pins.iter().find(|p| p.id == ep.pin_id))
                    .map(|p| p.io)
                    .unwrap_or(IoDirection::Unknown)
            };
            if let Some(e) = counts.iter_mut().find(|(d, _)| *d == io) {
                e.1 += 1;
            } else {
                counts.push((io, 1));
            }
        }
    }
    // Prefer the most common definite direction; a definite direction beats
    // Unknown/Passive on ties.
    counts
        .into_iter()
        .max_by_key(|(io, cnt)| (*cnt, is_definite(*io) as u8))
        .map(|(io, _)| io)
}

fn is_definite(io: IoDirection) -> bool {
    io != IoDirection::Unknown && io != IoDirection::Passive
}

/// Look up a net by nid. Net *names* are not unique (`moddcdc` carries five
/// separate `GND` nets), so any name-based lookup silently reads the first one.
/// M2: switch every caller to nid.
fn find_net<'a>(graph: &'a McVecGraph, nid: i64) -> Option<&'a VizNet> {
    graph.nets.iter().find(|n| n.nid == nid)
}

/// The name the renderer prints for a pin (`description`, falling back to
/// `pin_id`, then the raw id).
fn box_pin_name(b: &crate::vector::graph::McVecBox, pid: i64) -> String {
    b.pins
        .iter()
        .find(|p| p.id == pid)
        .map(|p| {
            if p.description.is_empty() {
                p.pin_id.clone()
            } else {
                p.description.clone()
            }
        })
        .unwrap_or_else(|| pid.to_string())
}

/// Mirror `slots` onto `entry_points` (the renderer draws pins from them),
/// synthesising them when the device graph left them empty.
fn sync_entry_points(b: &mut crate::vector::graph::McVecBox, connected: &BTreeSet<i64>) {
    let placed: Vec<(i64, EntrySide, f64)> = b
        .slots
        .iter()
        .map(|s| (s.pin_id, s.side, s.offset))
        .collect();
    for ep in b.entry_points.iter_mut() {
        if let Some(&(_, side, offset)) = placed.iter().find(|(pid, _, _)| *pid == ep.pin_id) {
            ep.side = side;
            ep.offset = offset;
        }
    }
    if b.entry_points.is_empty() {
        for (pid, side, offset) in placed {
            if !connected.contains(&pid) {
                continue;
            }
            let pin_name = box_pin_name(b, pid);
            b.entry_points
                .push(crate::vector::graph::boxdef::EntryPoint {
                    pin_id: pid,
                    pin_name,
                    side,
                    offset,
                });
        }
    }
}

/// ★ M9.2: build the satellite plan and assign each satellite a W/E side — the
/// majority region of the nets it shares with its parent. Nets that fall on N/S
/// (or span neither) drop out and the component stays a Sink.
fn satellite_plan_for(
    graph: &McVecGraph,
    topos: &[NetTopology],
    layer_anchor: i64,
) -> Vec<(super::equi_place::Satellite, Region)> {
    use super::equi_place::{plan_satellites, CompView};

    let net_is_ground: Vec<bool> = topos
        .iter()
        .map(|t| t.net_kind == NetKind::Ground)
        .collect();

    let mut comps: Vec<CompView> = Vec::new();
    for b in &graph.boxes {
        if b.pins.len() < 3 {
            continue;
        }
        let mut pins: Vec<(i64, usize)> = Vec::new();
        for p in &b.pins {
            let owner = topos.iter().position(|t| {
                t.groups
                    .iter()
                    .any(|g| g.box_id == b.id && g.pin_ids.contains(&p.id))
            });
            if let Some(n) = owner {
                pins.push((p.id, n));
            }
        }
        if !pins.is_empty() {
            comps.push(CompView { box_id: b.id, pins });
        }
    }

    let bridged = bridged_net_pairs(graph, topos);
    plan_satellites(&comps, &net_is_ground, &bridged, layer_anchor)
        .into_iter()
        .filter_map(|sat| {
            // Majority region over the shared nets; W/E only.
            let (mut w, mut e) = (0usize, 0usize);
            for &n in &sat.shared {
                match topos.get(n).map(|t| t.lane.region) {
                    Some(Region::West) => w += 1,
                    Some(Region::East) => e += 1,
                    _ => {}
                }
            }
            if w == 0 && e == 0 {
                return None;
            }
            let region = if w >= e { Region::West } else { Region::East };
            Some((sat, region))
        })
        .collect()
}

/// ★ M9.2: place every planned satellite as a real component.
///
/// Facing pins land ON their own net's row, so each shared net is a straight
/// horizontal wire between the two components; everything else is pushed to the
/// far edge. The x here is provisional — [`push_satellites_clear`] moves it
/// outside the member columns once those are allocated.
///
/// Returns `(box id, region)` for the second pass. Marks each satellite
/// `geom_locked`, which is also what keeps `place_members_for_topo` from
/// re-placing it as a `TapRole::Sink`.
fn place_satellites(
    graph: &mut McVecGraph,
    topos: &[NetTopology],
    layer_anchor: i64,
) -> Vec<(i64, Region)> {
    let plan = satellite_plan_for(graph, topos, layer_anchor);
    let Some((ax, aw)) = graph
        .boxes
        .iter()
        .find(|b| b.id == layer_anchor)
        .map(|b| (b.x, b.w))
    else {
        return Vec::new();
    };

    let mut out: Vec<(i64, Region)> = Vec::new();
    for (sat, region) in plan {
        // A facing pin whose net has no row cannot sit on one — demote it.
        let mut rows: Vec<(i64, f64)> = Vec::new();
        let mut away: Vec<i64> = sat.away.clone();
        for &pid in &sat.facing {
            let row = topos
                .iter()
                .find(|t| {
                    t.groups
                        .iter()
                        .any(|g| g.box_id == sat.box_id && g.pin_ids.contains(&pid))
                })
                .filter(|t| t.lane.horizontal)
                .map(|t| t.lane.axis);
            match row {
                Some(y) => rows.push((pid, y)),
                None => away.push(pid),
            }
        }
        if rows.is_empty() {
            continue; // nothing to face with; leave it to the Sink path
        }
        rows.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));

        // ★ M14.3: the same lookup for the away side. M14.2 gives these nets a
        // row of their own next to the satellite, so most of them have one. A
        // net qualifies only when it actually owns a W/E trunk — `lane.horizontal`
        // is `true` by default even for a terminal-only ground with no lane, and
        // handing that zero-axis to two GND pins would stack them on one offset.
        let away_rows: Vec<(i64, f64)> = away
            .iter()
            .filter_map(|&pid| {
                topos
                    .iter()
                    .find(|t| {
                        t.groups
                            .iter()
                            .any(|g| g.box_id == sat.box_id && g.pin_ids.contains(&pid))
                    })
                    .filter(|t| {
                        !t.terminal_only && matches!(t.lane.region, Region::West | Region::East)
                    })
                    .map(|t| (pid, t.lane.axis))
            })
            .collect();

        let connected: BTreeSet<i64> = topos
            .iter()
            .flat_map(|t| t.groups.iter())
            .filter(|g| g.box_id == sat.box_id)
            .flat_map(|g| g.pin_ids.iter().copied())
            .collect();

        let Some(b) = graph.boxes.iter_mut().find(|b| b.id == sat.box_id) else {
            continue;
        };
        let facing_side = match region {
            Region::West => EntrySide::Right,
            _ => EntrySide::Left,
        };
        let away_side = opposite_side(facing_side);

        // ★ M14.3: the box has to cover the away rows as well, or an away pin's
        // offset clamps to the edge and lands off its trunk.
        let all_y = rows
            .iter()
            .chain(away_rows.iter())
            .map(|&(_, y)| y)
            .collect::<Vec<f64>>();
        let lo = all_y.iter().copied().fold(f64::MAX, f64::min);
        let hi = all_y.iter().copied().fold(f64::MIN, f64::max);
        let box_y = lo - PIN_MARGIN;
        let box_h = ((hi - lo) + 2.0 * PIN_MARGIN)
            .max(rows.len() as f64 * PIN_PITCH + 2.0 * PIN_MARGIN)
            .max(away.len() as f64 * PIN_PITCH + 2.0 * PIN_MARGIN);

        let facing_ids: Vec<i64> = rows.iter().map(|&(p, _)| p).collect();
        let name_w = b.name.chars().count() as f64 * LABEL_CHAR_W + 2.0 * LABEL_PAD;
        let box_w =
            (side_label_width(b, &facing_ids) + side_label_width(b, &away) + 3.0 * LABEL_PAD)
                .max(name_w)
                .max(MIN_BOX_W);

        b.w = box_w;
        b.h = box_h;
        b.y = box_y;
        b.x = match region {
            Region::West => ax - MEMBER_GAP - box_w,
            _ => ax + aw + MEMBER_GAP,
        };
        b.geom_locked = true;

        b.slots.clear();
        for (k, &(pid, y)) in rows.iter().enumerate() {
            let name = box_pin_name(b, pid);
            b.slots.push(PinSlot {
                pin_id: pid,
                number: k as u32,
                name,
                side: facing_side,
                offset: ((y - box_y) / box_h).clamp(0.0, 1.0),
                connected: connected.contains(&pid),
            });
        }
        // ★ M14.3: an away pin whose net HAS a row sits ON it, exactly like a
        // facing pin. Spreading them evenly down the far edge was the other half
        // of the `mic` defect: `mic.3` / `mic.4` were parked at 1/3 and 2/3 of
        // the box while `_net2` / `_net3` ran somewhere else entirely, so the
        // wire had to leave the pin, find the trunk, and come back (A27).
        let n = away.len();
        for (k, &pid) in away.iter().enumerate() {
            let name = box_pin_name(b, pid);
            let row_y = away_rows.iter().find(|&&(p, _)| p == pid).map(|&(_, y)| y);
            let offset = match row_y {
                Some(y) => ((y - box_y) / box_h).clamp(0.0, 1.0),
                None => (k as f64 + 1.0) / (n as f64 + 1.0),
            };
            b.slots.push(PinSlot {
                pin_id: pid,
                number: (rows.len() + k) as u32,
                name,
                side: away_side,
                offset,
                connected: connected.contains(&pid),
            });
        }
        sync_entry_points(b, &connected);
        out.push((sat.box_id, region));
    }
    out
}

/// ★ M15.2: force every satellite pin onto the row of the net that owns it.
///
/// `place_satellites` already derives the slot offsets from the rows, but it
/// does so from a box height it computes at that moment, and the offsets are
/// stored as a FRACTION of that height. Anything that later changes the box —
/// or any row the first pass could not see — silently slides every pin off its
/// trunk, and the symptom is the one `mic` kept showing: a pin stub on one edge
/// with its wire somewhere else entirely (A27, and now A34).
///
/// So: recompute the offsets from the rows, and GROW the box if a row falls
/// outside it rather than clamping the pin to the edge. Idempotent — running it
/// twice changes nothing — and it touches satellites only, so a layer with none
/// is untouched.
fn snap_satellite_pins_to_rows(
    graph: &mut McVecGraph,
    topos: &[NetTopology],
    sats: &[(i64, Region)],
) {
    for &(id, _) in sats {
        let Some(pins) = graph
            .boxes
            .iter()
            .find(|b| b.id == id)
            .map(|b| b.pins.iter().map(|p| p.id).collect::<Vec<i64>>())
        else {
            continue;
        };
        let want: Vec<(i64, f64)> = pins
            .iter()
            .filter_map(|&pid| {
                topos
                    .iter()
                    .find(|t| {
                        t.groups
                            .iter()
                            .any(|g| g.box_id == id && g.pin_ids.contains(&pid))
                    })
                    .filter(|t| t.lane.horizontal && !t.terminal_only)
                    .map(|t| (pid, t.lane.axis))
            })
            .collect();
        if want.is_empty() {
            continue;
        }
        let lo = want.iter().map(|&(_, y)| y).fold(f64::MAX, f64::min);
        let hi = want.iter().map(|&(_, y)| y).fold(f64::MIN, f64::max);
        let connected: BTreeSet<i64> = topos
            .iter()
            .flat_map(|t| t.groups.iter())
            .filter(|g| g.box_id == id)
            .flat_map(|g| g.pin_ids.iter().copied())
            .collect();
        let Some(b) = graph.boxes.iter_mut().find(|b| b.id == id) else {
            continue;
        };
        let top = b.y.min(lo - PIN_MARGIN);
        let bottom = (b.y + b.h).max(hi + PIN_MARGIN);
        b.y = top;
        b.h = (bottom - top).max(PIN_PITCH);
        for slot in b.slots.iter_mut() {
            if let Some(&(_, y)) = want.iter().find(|&&(p, _)| p == slot.pin_id) {
                slot.offset = ((y - b.y) / b.h).clamp(0.0, 1.0);
            }
        }
        sync_entry_points(b, &connected);
    }
}

/// ★ M9.2 second pass: move each satellite outside the member columns, once
/// `resolve_columns_for_side` has allocated them. The shared nets' trunks then
/// run from the anchor pin, past every member on that row, straight into the
/// satellite's facing pin — which is exactly the connected-pins-between-the-two
/// shape. `envelop_lanes` runs after this and picks the new tap up.
fn push_satellites_clear(
    graph: &mut McVecGraph,
    topos: &[NetTopology],
    sats: &[(i64, Region)],
    layer_anchor: i64,
) {
    use crate::viz::layout::equi_column::{COL_MARGIN, COL_STEP};
    let Some((ax, aw)) = graph
        .boxes
        .iter()
        .find(|b| b.id == layer_anchor)
        .map(|b| (b.x, b.w))
    else {
        return;
    };
    for &(id, region) in sats {
        let Some(w) = graph.boxes.iter().find(|b| b.id == id).map(|b| b.w) else {
            continue;
        };
        let mut edge = match region {
            Region::West => ax,
            _ => ax + aw,
        };
        for b in graph.boxes.iter() {
            if b.id == id || b.id == layer_anchor || b.w <= 0.0 || b.h <= 0.0 {
                continue;
            }
            match region {
                Region::West if b.x + b.w <= ax + 1.0 => edge = edge.min(b.x),
                Region::East if b.x >= ax + aw - 1.0 => edge = edge.max(b.x + b.w),
                _ => {}
            }
        }
        let Some(sat) = graph.boxes.iter_mut().find(|b| b.id == id) else {
            continue;
        };
        let new_x = match region {
            Region::West => edge - MEMBER_GAP - w,
            _ => edge + MEMBER_GAP,
        };
        let delta = new_x - sat.x;
        sat.x = new_x;
        // ★ M15.7: the satellite's members were placed (by the run / column
        // pass, which runs BEFORE this) against the satellite's PROVISIONAL x.
        // Pushing the satellite outward now leaves them stranded on the wrong
        // side of the box — on `mic` the away diodes `dio1`/`dio2` ended up to
        // the RIGHT of the box while `mic.3`/`mic.4` sit on the LEFT edge (so
        // their ground wires crossed the body). Carry the away members along so
        // `mic.3/4` meet their diodes with a short outward stub. The away side
        // is the net region MATCHING the satellite's region (West satellite →
        // West trunks point away from the anchor).
        if delta != 0.0 {
            let away: BTreeSet<i64> = topos
                .iter()
                .filter(|t| t.anchor == id && t.lane.region == region)
                .flat_map(|t| t.groups.iter().map(|g| g.box_id))
                .collect();
            for bid in away {
                if bid == id || bid == layer_anchor {
                    continue;
                }
                if let Some(m) = graph.boxes.iter_mut().find(|b| b.id == bid) {
                    m.x += delta;
                }
            }
            // ★ M15.9: the FACE-side members were also placed against the
            // provisional edge, so they too land wrong after the push — on
            // `mic`, `_R1` (the `MIC.N` 0R to ground) sat at x=90, INSIDE
            // `wm7121`, dragging `MIC.N`'s trunk underneath it so the picture
            // read as if `mic.2` reached `wm7121.2`. Re-anchor them from the
            // satellite's FINAL face edge, keeping their relative order, so the
            // face trunk stays in the gap between the two components. The face
            // side is the net region opposite the satellite's (West satellite →
            // East trunks point toward the anchor).
            let face_side = opposite_side(region.entry_side());
            let mut face: Vec<i64> = topos
                .iter()
                .filter(|t| t.anchor == id && t.lane.region.entry_side() == face_side)
                .flat_map(|t| t.groups.iter().map(|g| g.box_id))
                .collect();
            face.sort_unstable();
            face.dedup();
            let face_edge = new_x + w;
            for (k, &bid) in face.iter().enumerate() {
                if bid == id || bid == layer_anchor {
                    continue;
                }
                if let Some(m) = graph.boxes.iter_mut().find(|b| b.id == bid) {
                    m.x = face_edge + COL_MARGIN + k as f64 * COL_STEP - m.w / 2.0;
                }
            }
        }
    }
}

/// Place boxes by topology. Writes x/y/w/h and entry_points on boxes,
/// sets geom_locked = true. Overrides FlowLayouter placement.
///
/// Pipeline:
///   P1 assign_regions  (semantic, pure — runs first: rows need the regions)
///   P0 assign_rows     (pure topology — per-side row allocation + pin authority)
///   P2 place the layer anchor box (and, ★ M9, every satellite component)
///   P3+P4+P5 resolve lanes + place members in **dependency order** to a fixed
///        point, then
///   P6 envelop_lanes
///
/// Dependency order: a net's lane may only be resolved once its anchor box is
/// placed. The anchor is often itself a member placed by another net (a GND net
/// is anchored on the decoupling cap a power net hangs), so a flat nid-order
/// pass would read that anchor's unplaced fallback rect — the drift source that
/// A2 catches. Iterating to a fixed point makes every lane read a placed rect,
/// so the layout phase and the render phase (which replays on the fully placed
/// graph) are constructively identical.
pub fn place_by_topology(graph: &mut McVecGraph, topos: &mut [NetTopology]) {
    if topos.is_empty() {
        return;
    }

    let layer_anchor = layer_anchor_id(topos);

    // P1: assign regions (semantic, pure)
    assign_regions(graph, topos);

    // P0: assign rows (pure topology) — the layer anchor's pin offsets follow
    // the rows (pin-offset ownership inversion, M2).
    let row_plan = assign_rows(graph, topos, layer_anchor);

    // P2: place the layer anchor box (pin side / box size / PinSlots) — pins
    // land on the rows assigned by P0.
    assign_anchor_slots(graph, layer_anchor, topos, &row_plan);

    // ★ M9.2: place the OTHER components before the member fixed point. Each
    // satellite is `geom_locked` here, which both records the placement and
    // keeps `place_members_for_topo` from hanging it off one row as a Sink.
    let satellites = place_satellites(graph, topos, layer_anchor);

    let mut resolved = vec![false; topos.len()];
    // Terminal-only nets carry no trunk and place nothing — mark them resolved
    // so the fixed point does not try to give them a lane.
    for (i, t) in topos.iter().enumerate() {
        if t.terminal_only {
            resolved[i] = true;
        }
    }
    // M5.3: a SHARED per-row Drop counter so shunts on the SAME row coordinate
    // across DIFFERENT nets (e.g. ldo POWER_SYS/W<->VCC/E both at row 100) and
    // alternate up/down instead of piling below the trunk. Rows are fixed by
    // `assign_rows` (P0) before the loop, so the order here is deterministic.
    let mut drop_counter: BTreeMap<i64, usize> = BTreeMap::new();
    // Bounded fixed point: every net resolves at most once and at least one new
    // net resolves per pass, so `topos.len()` passes always converge.
    for _ in 0..=topos.len() {
        let placed = placed_box_ids(graph);
        let mut progressed = false;

        // P3: resolve lanes for nets whose anchor box is already placed — OR
        // (★ FIX) whose anchor is a non-layer-anchor 2-pin component that has
        // not been sized yet. Such a sub-anchor (e.g. `C_DAC22`, `C_DAC330` in
        // a DAC chain) sits on a row assigned by `assign_rows` and is placed as
        // a member of its OWN net in P4 below, so a net anchored on it may
        // resolve now; the fixed point previously dead-locked here because the
        // sub-anchor was never placed and thus its net could never resolve.
        let mut to_resolve: Vec<(usize, usize)> = Vec::new();
        for (i, topo) in topos.iter().enumerate() {
            if resolved[i] {
                continue;
            }
            let anchor_ready = placed.contains(&topo.anchor)
                || is_self_placeable_sub_anchor(graph, topo, layer_anchor);
            if !anchor_ready {
                continue;
            }
            to_resolve.push((i, lane_index_within_group(topos, topo)));
        }
        for (i, index) in to_resolve {
            resolve_lane_for_topo(graph, index, &mut topos[i]);
            topos[i].anchor_placed = true;
            resolved[i] = true;
            progressed = true;
        }

        if !progressed {
            break;
        }

        // P4: place member boxes by reading the lanes (never recompute x).
        // Idempotent: members already placed (geom_locked) are skipped.
        place_members(graph, topos, &resolved, &mut drop_counter, layer_anchor);
    }

    // P4b (M4.2b): override the W/E member x with a single side-wide column
    // allocation (correct granularity — all members of a side share one
    // occupancy table, so two nets anchoring the same IC edge no longer pile
    // onto one column). N/S members keep their provisional x.
    resolve_columns_for_side(graph, topos, layer_anchor);

    // ★ M12.4: with x final, a shunt that would hang DOWN through another row
    // may flip UP instead.
    flip_shunts_clear_of_rows(graph, topos, layer_anchor);

    // ★ M15.2: and force every satellite pin back onto its own net's row.
    snap_satellite_pins_to_rows(graph, topos, &satellites);

    // ★ M9.2b: now that the member columns are final, push each satellite
    // outside them — the shared nets then run straight from the anchor pin,
    // past the members, into the satellite's facing pin.
    push_satellites_clear(graph, topos, &satellites, layer_anchor);

    // P6: after members are placed, re-envelope the lane span over all tap
    // points (anchor pins + member taps) so the trunk reaches every tap.
    envelop_lanes(graph, topos);

    dump_layer(graph, topos, layer_anchor);
}

/// ★ M15.3: one `[equi-dump]` line per net and per member, with everything
/// needed to diagnose a layer WITHOUT reading the SVG.
///
/// Three rounds on `mic` went: read the picture, guess the mechanism, patch,
/// get a differently-broken picture. Every one of those guesses would have been
/// settled in one run by the four numbers below — a net's region, its row, where
/// that row came from, and whether each of its pins actually sits on it.
///
/// Costs nothing when `vlog!` is off.
fn dump_layer(graph: &McVecGraph, topos: &[NetTopology], layer_anchor: i64) {
    crate::vlog!("[equi-dump] layer_anchor = box {}", layer_anchor);
    for (i, t) in topos.iter().enumerate() {
        crate::vlog!(
            "[equi-dump] net#{} '{}' nid={} kind={:?} region={:?} row={:.0} src={:?} \
             span=({:.0},{:.0}) term_only={} anchor=box{} run_root={} depth={} \
             outer_taken={} gcol={}",
            i,
            t.net_name,
            t.nid,
            t.net_kind,
            t.lane.region,
            t.lane.axis,
            t.row_source,
            t.lane.span.0,
            t.lane.span.1,
            t.terminal_only,
            t.anchor,
            t.run_root,
            t.run_depth,
            t.outer_end_taken,
            t.ground_column
        );
        for (gi, g) in t.groups.iter().enumerate() {
            let Some(b) = graph.boxes.iter().find(|b| b.id == g.box_id) else {
                crate::vlog!("[equi-dump]     group#{} box{} MISSING", gi, g.box_id);
                continue;
            };
            let role = if b.pins.len() == 2 && gi > 0 {
                format!(
                    "{:?}",
                    tap_role(b, t, partner_info(topos, i, g), layer_anchor)
                )
            } else if gi == 0 {
                "anchor".to_string()
            } else {
                "multi".to_string()
            };
            // The part of A27/A34 that keeps going wrong: does each pin of this
            // group actually sit on this net's row, and inside its span?
            let pins: Vec<String> = g
                .pin_ids
                .iter()
                .map(|&pid| match slot_of(b, pid) {
                    Some(s) => {
                        let (px, py) = slot_point(b, s);
                        let (lo, hi) = (
                            t.lane.span.0.min(t.lane.span.1),
                            t.lane.span.0.max(t.lane.span.1),
                        );
                        format!(
                            "{}@({:.0},{:.0}){}{}",
                            pid,
                            px,
                            py,
                            if (py - t.lane.axis).abs() < 1.0 {
                                ""
                            } else {
                                " OFF-ROW"
                            },
                            if px >= lo - 1.0 && px <= hi + 1.0 {
                                ""
                            } else {
                                " OFF-SPAN"
                            }
                        )
                    }
                    None => format!("{pid}@NO-SLOT"),
                })
                .collect();
            crate::vlog!(
                "[equi-dump]     group#{} box{} '{}' npins={} role={} rect=({:.0},{:.0},{:.0},{:.0}) {}",
                gi,
                b.id,
                b.name,
                b.pins.len(),
                role,
                b.x,
                b.y,
                b.w,
                b.h,
                pins.join(" ")
            );
        }
    }
}

/// Boxes whose geometry has an owner — everything `place_by_topology` has
/// placed so far (the layer anchor via P2, members via P4).
fn placed_box_ids(graph: &McVecGraph) -> BTreeSet<i64> {
    graph
        .boxes
        .iter()
        .filter(|b| b.geom_locked)
        .map(|b| b.id)
        .collect()
}

/// ★ FIX: can this net resolve even though its anchor box is not placed yet?
/// A non-layer-anchor 2-pin component (e.g. `C_DAC22`, `C_DAC330` in a DAC
/// chain) is placed as a member of its own net in P4, so its net need not
/// wait for it. The layer anchor, satellites (already `geom_locked`) and
/// multi-pin Sink boxes are never self-placeable here.
fn is_self_placeable_sub_anchor(graph: &McVecGraph, topo: &NetTopology, layer_anchor: i64) -> bool {
    if topo.anchor == layer_anchor {
        return false;
    }
    graph
        .boxes
        .iter()
        .find(|b| b.id == topo.anchor)
        .is_some_and(|b| b.pins.len() == 2 && !b.geom_locked)
}

/// M2 B4 fix: the layer anchor's vertical extent is computed ONCE from the side
/// rows and shared by `assign_rows` (which places the North/South edge rails)
/// and `assign_anchor_slots` (which sizes the box and places the pins). The two
/// used to recompute `(lo - PIN_MARGIN, hi + PIN_MARGIN)` independently, so any
/// formula change silently misaligned the South rail against the pins.
fn side_row_extent(side_rows: &[f64]) -> Option<(f64, f64)> {
    if side_rows.is_empty() {
        return None;
    }
    let lo = side_rows.iter().cloned().fold(f64::MAX, f64::min);
    let hi = side_rows.iter().cloned().fold(f64::MIN, f64::max);
    Some((lo - PIN_MARGIN, hi + PIN_MARGIN))
}

/// ★ M3.1: pin ordering — the layer anchor's physical pins bucketed by side,
/// with an in-side sequence index. Pure topology: reads regions, the pin's
/// net membership and the physical pin order; never a rect.
///
/// The in-side order is the IC physical pin order for now (zero diff with the
/// M2 row allocation). v1's M5 "semantic pin ordering" (West: Power → Input →
/// Ground; East: Output → Bidir → Feedback) lands *in this function only*.
#[derive(Debug, Clone, Default)]
pub struct PinPlan {
    /// pin_id → (which side, in-side sequence index).
    pub sides: BTreeMap<i64, (EntrySide, usize)>,
    /// NC pins — belong to no net, no side.
    pub unassigned: Vec<i64>,
}

/// ★ M8.2: how hard ONE pin drives, for [`super::equi_chain`]'s seed order.
/// Same ladder as [`anchor_io_rank`], but per pin rather than per net — a run
/// grows out of a single pin, so the seed key must be that pin's own direction.
fn pin_io_rank(graph: &McVecGraph, box_id: i64, pin_id: i64) -> u8 {
    graph
        .boxes
        .iter()
        .find(|b| b.id == box_id)
        .and_then(|b| b.pins.iter().find(|p| p.id == pin_id))
        .map_or(0, |p| match p.io {
            IoDirection::Power => 4,
            IoDirection::Output => 3,
            IoDirection::Bidir => 2,
            IoDirection::Input => 1,
            _ => 0,
        })
}

/// ★ M8.6: is this net an ENDPOINT of its own — a place the netlist NAMES?
///
/// True when an explicit label / port box sits on it, or it is a power rail.
/// Deliberately NOT `!topo.terminals.is_empty()`: `build_one_topology` also
/// synthesises a `NetLabel` for named signal nets, and its auto-name guard tests
/// `starts_with("__net")` while the real project emits single-underscore
/// `_net7` — so plain internal nodes carry a terminal too and would every one of
/// them cut a run after a single hop. Reading the endpoint BOXES asks the
/// question that actually matters: did the author put a label here.
fn net_is_endpoint(graph: &McVecGraph, topo: &NetTopology) -> bool {
    if topo.net_kind == NetKind::Power {
        return true;
    }
    find_net(graph, topo.nid).is_some_and(|n| {
        n.endpoints.iter().any(|ep| {
            graph
                .boxes
                .iter()
                .find(|b| b.id == ep.box_id)
                .is_some_and(|b| {
                    matches!(
                        b.kind,
                        BoxKind::PowerLabel | BoxKind::PortTerminal | BoxKind::Dot
                    )
                })
        })
    })
}

/// ★ M8.2: build the chain analyser's view of this layer and run it.
///
/// Pure topology: reads group box ids, pin COUNTS, pin IO and endpoint box KINDS
/// — never a rect, so the A2 guard is untouched. A two-pin box owned by exactly
/// two topologies is a part; one owned by fewer (its far pin is in no net) is
/// left out and defaults to `Shunt`, which is the M7 behaviour.
pub(crate) fn chain_plan_for(
    graph: &McVecGraph,
    topos: &[NetTopology],
    layer_anchor: i64,
) -> super::equi_chain::ChainPlan {
    use super::equi_chain::{NetView, PartView};
    // ★ M11.2: computed ONCE — `plan_satellites` walks the component graph and
    // this function runs twice per layer (Pass 1.75 and `assign_rows`).
    let sat_nets = satellite_nets(graph, topos, layer_anchor);
    let nets: Vec<NetView> = topos
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let is_ground = t.net_kind == NetKind::Ground;
            let anchor_pin = if is_ground {
                None
            } else {
                t.groups
                    .iter()
                    .find(|g| g.box_id == layer_anchor)
                    .and_then(|g| {
                        g.pin_ids
                            .iter()
                            .map(|&pid| (pin_io_rank(graph, layer_anchor, pid), pid))
                            .max()
                    })
            };
            NetView {
                anchor_pin,
                is_ground,
                is_endpoint: net_is_endpoint(graph, t),
                // ★ M10.3: a ground net that owns a real GND pin on the layer
                // anchor belongs to the South rail — its shunts drop and its
                // pins stay on the Bottom edge (`ground_pins_on_south`). Only a
                // ground reached PURELY through a part ("the far end of this cap
                // is ground") may act as a label.
                ground_adoptable: is_ground && !t.groups.iter().any(|g| g.box_id == layer_anchor),
                // ★ M11.2: the row already ends at another component.
                ends_at_component: sat_nets.contains(&i),
            }
        })
        .collect();

    let mut owners: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
    for (i, t) in topos.iter().enumerate() {
        for g in t.groups.iter().filter(|g| g.box_id != layer_anchor) {
            let two_pin = graph
                .boxes
                .iter()
                .find(|b| b.id == g.box_id)
                .is_some_and(|b| b.pins.len() == 2);
            if two_pin {
                let e = owners.entry(g.box_id).or_default();
                if !e.contains(&i) {
                    e.push(i);
                }
            }
        }
    }
    let parts: Vec<PartView> = owners
        .into_iter()
        .filter(|(_, o)| o.len() == 2)
        .map(|(box_id, o)| PartView {
            box_id,
            nets: (o[0], o[1]),
        })
        .collect();

    super::equi_chain::analyse(&nets, &parts)
}

/// M3.1: derive the layer anchor's [`PinPlan`]. The side of a pin comes from
/// the region of the net it belongs to (first group of an anchor-touching
/// net); the in-side index follows the physical pin order. Pins in no net are
/// collected into `unassigned` (M2.5 Step 3 kept them out of the W/E/N/S
/// buckets; the NC group still lives here so `assign_anchor_slots` does not
/// re-derive it).
pub fn assign_pin_order(graph: &McVecGraph, topos: &[NetTopology], layer_anchor: i64) -> PinPlan {
    let mut pin_side: BTreeMap<i64, EntrySide> = BTreeMap::new();
    let mut pin_net: BTreeMap<i64, usize> = BTreeMap::new();
    for (i, topo) in topos.iter().enumerate() {
        if topo.anchor != layer_anchor {
            continue;
        }
        let side = topo.lane.region.entry_side();
        if let Some(g) = topo.groups.first() {
            for &pid in &g.pin_ids {
                pin_side.entry(pid).or_insert(side);
                pin_net.entry(pid).or_insert(i);
            }
        }
    }
    let mut sides: BTreeMap<i64, (EntrySide, usize)> = BTreeMap::new();
    let mut unassigned: Vec<i64> = Vec::new();
    let Some(anchor_box) = graph.boxes.iter().find(|b| b.id == layer_anchor) else {
        return PinPlan { sides, unassigned };
    };

    // M3.5 (R3): a multi-pin net's pins on one side must occupy ADJACENT
    // in-side slots, so the RowAllocator puts them on adjacent rows and the
    // stray pin's tooth stays short. Each net's position is its first pin's
    // physical order; pins within a net keep physical order. Without this, a
    // net whose pins are interleaved with another net's pins (VIN, GND, CE
    // with VIN+CE on one net) gets its rows far apart and the tooth of the
    // non-trunk pin runs along the box edge for the whole span.
    let mut side_nets: [Vec<Vec<i64>>; 4] = Default::default();
    let mut side_net_ids: [Vec<usize>; 4] = Default::default();
    let mut net_pos: [BTreeMap<usize, usize>; 4] = Default::default();
    for p in &anchor_box.pins {
        let Some(side) = pin_side.get(&p.id).copied() else {
            unassigned.push(p.id);
            continue;
        };
        let slot = side_slot(side);
        let net = pin_net[&p.id];
        let pos = *net_pos[slot].entry(net).or_insert_with(|| {
            side_nets[slot].push(Vec::new());
            side_net_ids[slot].push(net);
            side_nets[slot].len() - 1
        });
        side_nets[slot][pos].push(p.id);
    }

    // ★ M7.1: two nets COUPLED through a shared two-pin part take CONSECUTIVE
    // in-side slots. `assign_rows` hands out bands in in-side index order, so
    // the coupling component is a short vertical Bridge only when the two nets
    // land on adjacent bands; scattered slots turn the same part into a long
    // wire running past every row in between. Same reasoning as the M3.5 (R3)
    // rule above, one level up: R3 keeps ONE net's pins together, this keeps
    // two LOOPED nets together.
    //
    // Walk the physical order and, whenever a net is emitted, pull its
    // still-unemitted coupled partners in right behind it — deterministic,
    // because both loops walk the existing order.
    let couples = coupled_net_pairs(graph, topos, layer_anchor);
    if !couples.is_empty() {
        for slot in 0..4 {
            let n = side_net_ids[slot].len();
            if n < 3 {
                continue; // 0/1/2 groups are adjacent whatever the order
            }
            let mut emitted = vec![false; n];
            let mut order: Vec<usize> = Vec::with_capacity(n);
            for k in 0..n {
                if emitted[k] {
                    continue;
                }
                emitted[k] = true;
                order.push(k);
                for m in (k + 1)..n {
                    if emitted[m] {
                        continue;
                    }
                    let (a, b) = (side_net_ids[slot][k], side_net_ids[slot][m]);
                    if couples
                        .iter()
                        .any(|&(x, y)| (x == a && y == b) || (x == b && y == a))
                    {
                        emitted[m] = true;
                        order.push(m);
                    }
                }
            }
            let reordered: Vec<Vec<i64>> =
                order.iter().map(|&k| side_nets[slot][k].clone()).collect();
            side_nets[slot] = reordered;
        }
    }

    let mut counter = [0usize; 4];
    for slot in 0..4 {
        for net in &side_nets[slot] {
            let side = side_from_slot(slot);
            for &pid in net {
                sides.insert(pid, (side, counter[slot]));
                counter[slot] += 1;
            }
        }
    }
    PinPlan { sides, unassigned }
}

/// Reverse of [`side_slot`]: 0=Top, 1=Right, 2=Bottom, 3=Left.
fn side_from_slot(slot: usize) -> EntrySide {
    match slot {
        0 => EntrySide::Top,
        1 => EntrySide::Right,
        2 => EntrySide::Bottom,
        _ => EntrySide::Left,
    }
}

/// 0..3 index of an [`EntrySide`] (Top=0, Right=1, Bottom=2, Left=3) — the
/// variant order in `boxdef.rs`, used where `EntrySide` lacks `Ord`.
fn side_slot(side: EntrySide) -> usize {
    match side {
        EntrySide::Top => 0,
        EntrySide::Right => 1,
        EntrySide::Bottom => 2,
        EntrySide::Left => 3,
    }
}

/// Final bottom edge of the layer-anchor box, mirroring the M2.5 Step 3/4
/// growth in `assign_anchor_slots`: the box is at least as tall as the
/// connected rows' span (`ic_bottom - ic_top`) and the pin-count pitch
/// (`count_h`), and grows again for unassigned (NC) pins stacked below the
/// connected right pins. The South rail and the cycle-break rows must hug
/// THIS edge, not `ic_bottom` — when the box grows below the row span (a tall
/// component with many NC pins), a rail computed from `ic_bottom + RAIL_GAP`
/// lands inside the box body, and the filled box hides the whole ground tree.
fn final_box_bottom(
    ic_top: f64,
    ic_bottom: f64,
    per_side: &BTreeMap<Region, Vec<(usize, usize, i64)>>,
    pin_band: &BTreeMap<i64, usize>,
    band_y: &[f64],
    unassigned: &[i64],
) -> f64 {
    let west_len = per_side.get(&Region::West).map_or(0, |l| l.len());
    let east_len = per_side.get(&Region::East).map_or(0, |l| l.len());
    let span_h = (ic_bottom - ic_top).max(0.0);
    let count_h = west_len.max(east_len).max(1) as f64 * PIN_PITCH + 2.0 * PIN_MARGIN;
    let mut box_h = span_h.max(count_h);
    let right_max = per_side
        .get(&Region::East)
        .into_iter()
        .flatten()
        .filter_map(|(_, _, pid)| pin_band.get(pid).map(|&b| band_y[b]))
        .fold(f64::MIN, f64::max);
    let base = if right_max > f64::MIN {
        right_max
    } else {
        ic_top + PIN_MARGIN
    };
    for (k, _) in unassigned.iter().enumerate() {
        let y = base + (k as f64 + 1.0) * PIN_PITCH;
        box_h = box_h.max(y + PIN_MARGIN - ic_top);
    }
    ic_top + box_h
}

/// ★ M2 P0: assign a row (trunk y) to every trunk-bearing net. Pure topology —
/// reads regions, the layer anchor's pin order, member counts and mount
/// directions; never a rect. This is the single authority for trunk y: the
/// layer anchor's pin offsets are derived from these rows by
/// `assign_anchor_slots` (pin-offset ownership inversion — the rows come
/// first, the pins follow). Returns the [`RowPlan`] the anchor slot placement
/// consumes.
///
///  * IC-anchored West/East nets: **every anchor pin** takes its own row slot,
///    in IC pin order, spaced by a variable pitch `max(PIN_PITCH,
///    down_demand + MEMBER_GAP, up_demand + MEMBER_GAP)` so a member hanging
///    from one row clears the next (Class B). A net's trunk sits on its first
///    (min) pin's row; its other IC pins connect via a tooth.
///  * IC-anchored North/South nets: rows above/below the IC (the side rows
///    determine the IC extent).
///  * Free nets (multi-group, passive anchor): inherit the row of a partner
///    net they share a NON-anchor member with, below the partner's downward
///    extent — decoupled from the accidental y of wherever their anchor was
///    placed (`moddcdc` 501←507, 506←510).
pub(crate) fn assign_rows(
    graph: &McVecGraph,
    topos: &mut [NetTopology],
    layer_anchor: i64,
) -> RowPlan {
    // Terminal-only = single real group NOT anchored on the layer anchor.
    // (A single-group net anchored on the IC, e.g. `moddcdc` 505 GND, is a real
    // net.) Corrected here because the predicate needs `layer_anchor`, which is
    // only known once the topology is built; both the layout and render phases
    // call `assign_rows`, so the flag stays consistent.
    for t in topos.iter_mut() {
        if t.groups.len() == 1 && t.anchor != layer_anchor {
            t.terminal_only = true;
        }
    }

    let n = topos.len();
    let mut rows: Vec<Option<f64>> = vec![None; n];
    let mut sources: Vec<Option<RowSource>> = vec![None; n];

    // M3.1: pin ordering is owned by `assign_pin_order` — the layer anchor's
    // (pin → side, in-side index) map replaces the inline `pin_order` lookup.
    let pin_plan = assign_pin_order(graph, topos, layer_anchor);

    // ★ M8.2: chain analysis, BEFORE any row is handed out. Up to M7 the order
    // was `assign_rows` -> `tap_role` (which read the row delta), so orientation
    // was a by-product of a decision already frozen — the M3 landing note "truly
    // sharing a row is structurally unreachable under the RowAllocator" is
    // exactly that. M8 turns it around: the netlist says which nets are
    // collinear, and the row allocator is told, not asked.
    let chain = chain_plan_for(graph, topos, layer_anchor);
    let nids: Vec<i64> = topos.iter().map(|t| t.nid).collect();
    for (i, t) in topos.iter_mut().enumerate() {
        t.run_depth = chain.depth.get(i).copied().unwrap_or(0);
        t.run_root = match chain.region.get(i).copied().flatten() {
            Some(r) => nids.get(r).copied().unwrap_or(t.nid),
            None => t.nid,
        };
        // ★ M11.3: and whether anything physical already owns its outer end.
        t.outer_end_taken = chain.outer_end_taken(i);
        // ★ M12.1: and whether it is a shared ground NODE rather than a row.
        t.ground_column = chain.is_ground_column(i);
    }
    // IC-anchored trunk-bearing nets: every anchor pin on the layer anchor
    // enters its region bucket independently (M2.5 Step 2) — a net with two IC
    // pins takes two consecutive row slots instead of collapsing onto one.
    let mut per_side: BTreeMap<Region, Vec<(usize, usize, i64)>> = BTreeMap::new();
    for (i, t) in topos.iter().enumerate() {
        if t.terminal_only || t.anchor != layer_anchor {
            continue;
        }
        let Some(g) = t.groups.first() else { continue };
        for &pid in &g.pin_ids {
            let pin_idx = pin_plan.sides.get(&pid).map(|&(_, idx)| idx).unwrap_or(0);
            per_side
                .entry(t.lane.region)
                .or_default()
                .push((pin_idx, i, pid));
        }
    }
    for v in per_side.values_mut() {
        v.sort_by_key(|(p, _, _)| *p);
    }

    // ── M3.2 Phase 1: band allocation (order, not coordinates yet) ──
    // A band index is a row SLOT. West pin k and East pin k share band k
    // ("two taps share a row ⟺ regions are W/E-opposite"); free nets get the
    // first band at/after their partner's band + 1 that they can share
    // (single occupant, W/E opposite) or a fresh band; North/South rails are
    // handled separately below (they hug the box edge).
    let mut band_nets: Vec<Vec<usize>> = Vec::new();
    let mut net_band: Vec<Option<usize>> = vec![None; n];
    let mut pin_band: BTreeMap<i64, usize> = BTreeMap::new();

    let west = per_side.get(&Region::West);
    let east = per_side.get(&Region::East);
    let side_len = west.map_or(0, |l| l.len()).max(east.map_or(0, |l| l.len()));
    for k in 0..side_len {
        let mut nets = Vec::new();
        if let Some(w) = west {
            if let Some(&(_, ti, pid)) = w.get(k) {
                nets.push(ti);
                net_band[ti] = Some(net_band[ti].unwrap_or(k));
                pin_band.insert(pid, k);
            }
        }
        if let Some(e) = east {
            if let Some(&(_, ti, pid)) = e.get(k) {
                nets.push(ti);
                net_band[ti] = Some(net_band[ti].unwrap_or(k));
                pin_band.insert(pid, k);
            }
        }
        band_nets.push(nets);
    }

    // ★ M8.2: every net of a RUN shares the run root's band — that is what makes
    // the connecting part lie ALONG the row instead of bridging two rows. Only
    // roots that already own a side band are propagated here; an island run's
    // root gets its band from the free-net pass below and is propagated after
    // it. A2: still pure topology.
    let share_run_bands = |band_nets: &mut Vec<Vec<usize>>,
                           net_band: &mut Vec<Option<usize>>,
                           sources: &mut Vec<Option<RowSource>>,
                           topos: &[NetTopology]| {
        for i in 0..topos.len() {
            if net_band[i].is_some() || topos[i].terminal_only {
                continue;
            }
            if topos[i].run_root == topos[i].nid {
                continue;
            }
            // ★ M10.3: a ground net shares a run's band only when it was ADOPTED
            // as that run's outer end, which Pass 1.75 records by moving it off
            // South. A South ground still gets its row from the edge rail.
            if topos[i].net_kind == NetKind::Ground
                && !matches!(topos[i].lane.region, Region::West | Region::East)
            {
                continue;
            }
            let Some(root) = topos.iter().position(|t| t.nid == topos[i].run_root) else {
                continue;
            };
            let Some(rb) = net_band[root] else { continue };
            net_band[i] = Some(rb);
            band_nets[rb].push(i);
            sources[i] = Some(RowSource::Partner(topos[root].nid));
        }
    };
    share_run_bands(&mut band_nets, &mut net_band, &mut sources, topos);

    // Free nets in nid ascending order (M2.5 Step 6 determinism guarantee).
    let mut is_free = vec![false; n];
    let mut free_order: Vec<usize> = (0..n)
        .filter(|&i| {
            net_band[i].is_none()
                && !topos[i].terminal_only
                // ★ M8.6: a RUN MEMBER (run_root != its own nid) must not grab a
                // band of its own — it is collinear with its root and must sit on
                // the SAME band, which `share_run_bands` assigns once the root has
                // one. Letting it through here let `speaker`'s `VDD_3V3` (the far
                // end of the `US_SPEAKER_MUTE` run) pick an independent row, so
                // the mute resistor bridged two rows instead of lying along one.
                && topos[i].run_root == topos[i].nid
                // M3.5 (R4): IC-anchored North/South nets are RAILS, not free
                // nets — they get their row from the IC extent. Treating them
                // as island free nets gave them a phantom band whose y then
                // clobbered the rail row (505 ended up at 540 instead of 400).
                && topos[i].anchor != layer_anchor
        })
        .collect();
    free_order.sort_by_key(|&i| topos[i].nid);
    for i in free_order {
        is_free[i] = true;
        let region = topos[i].lane.region;
        let partner = free_net_partner_band(topos, &net_band, i);
        let start = partner.map_or(0, |(_, pb)| pb + 1);
        // ★ M15.6: two nets anchored on the SAME multi-pin component are two of
        // its pins — they must sit on distinct rows, not share the W/E pair row.
        // Without this `mic`'s `MIC.N` (East) and `_net2` (West) both anchored
        // on the microphone get paired onto one band, and the box loses the
        // one-pin-per-row shape the satellite stack was built to give it.
        let shared_anchor_is_multi = graph
            .boxes
            .iter()
            .find(|b| b.id == topos[i].anchor)
            .is_some_and(|b| b.pins.len() >= 3);
        let mut chosen = None;
        for k in start..band_nets.len() {
            if band_nets[k].len() == 1
                && is_w_e_opposite(topos[band_nets[k][0]].lane.region, region)
                && !(shared_anchor_is_multi && topos[band_nets[k][0]].anchor == topos[i].anchor)
            {
                band_nets[k].push(i);
                chosen = Some(k);
                break;
            }
        }
        let k = match chosen {
            Some(k) => k,
            None => {
                band_nets.push(vec![i]);
                band_nets.len() - 1
            }
        };
        net_band[i] = Some(k);
        match partner {
            Some((p, _)) => sources[i] = Some(RowSource::Partner(topos[p].nid)),
            None => sources[i] = Some(RowSource::IslandFallback),
        }
    }
    // ★ M8.2 second pass: island runs whose ROOT only just got a band.
    share_run_bands(&mut band_nets, &mut net_band, &mut sources, topos);

    // ── M3.2 Phase 2: per-band corridor demand (M3.3 demand attribution) ──
    // A 2-pin passive connecting bands a < b occupies a vertical corridor of
    // TWO_PIN_SYMBOL_W between them; it is counted as down-demand on band a and
    // up-demand on band b so the rows between them clear the member no matter
    // which net happens to place it (dependency order is not a design input).
    let mut up: Vec<f64> = vec![0.0; band_nets.len()];
    let mut down: Vec<f64> = vec![0.0; band_nets.len()];
    for (i, t) in topos.iter().enumerate() {
        let Some(bi) = net_band[i] else { continue };
        for group in t.groups.iter().skip(1) {
            let pin_count = graph
                .boxes
                .iter()
                .find(|b| b.id == group.box_id)
                .map(|b| b.pins.len())
                .unwrap_or(2);
            if pin_count == 2 {
                match find_partner(topos, i, group) {
                    None => down[bi] = down[bi].max(CORRIDOR_DEMAND),
                    // ★ M10.3: a partner on MY RUN lies ALONG the row and books
                    // no vertical corridor at all. This arm has to come FIRST
                    // because an adopted ground is terminal-only, so it never
                    // gets a band and `net_band[j]` cannot see it — the old arm
                    // charged a full CORRIDOR_DEMAND and pushed the next row
                    // 80px down for a wire that is horizontal.
                    Some((_, other)) if other.run_root == t.run_root => {}
                    // ★ M12.1: an arm of a ground COLUMN is horizontal too, even
                    // though the node itself sits on somebody else's run.
                    Some((_, other)) if other.ground_column => {}
                    Some((_, other))
                        if other.terminal_only || other.net_kind == NetKind::Ground =>
                    {
                        down[bi] = down[bi].max(CORRIDOR_DEMAND);
                    }
                    Some((j, _)) => match net_band[j] {
                        Some(pb) if pb > bi => down[bi] = down[bi].max(CORRIDOR_DEMAND),
                        Some(pb) if pb < bi => up[bi] = up[bi].max(CORRIDOR_DEMAND),
                        _ => {} // same band → Series (horizontal), no corridor
                    },
                }
            } else if pin_count >= 3 {
                // Sink: region-based demand. ★ M7.6: a Sink is now sized like a
                // real component (>= MIN_SINK_H tall) and hangs one LEAD off the
                // row, so reserve LEAD + its height, not the two-pin body.
                let demand = LEAD + MIN_SINK_H;
                if member_hangs_toward(topos, i, Region::South) {
                    down[bi] = down[bi].max(demand);
                }
                if member_hangs_toward(topos, i, Region::North) {
                    up[bi] = up[bi].max(demand);
                }
            }
        }
    }

    // ── M3.2 Phase 3: y from the band sequence ──
    // y[k+1] = y[k] + max(PIN_PITCH, down(k) + ROW_CLEAR, up(k+1) + ROW_CLEAR).
    // The clearance is ROW_CLEAR, not MEMBER_GAP — the old 60px clearance blew
    // the IC up (`lp322dcdc` used to be 280 tall, plan target <= 200). The
    // corridor demand above is what keeps a Bridge/Drop body clear of the next
    // row (the naive `MEMBER_GAP → ROW_CLEAR` swap alone regressed A10/A7).
    const BASE_Y: f64 = 100.0;
    let mut band_y = vec![0.0; band_nets.len()];
    let mut y = BASE_Y;
    for k in 0..band_nets.len() {
        band_y[k] = y;
        if k + 1 < band_nets.len() {
            y += PIN_PITCH
                .max(down[k] + ROW_CLEAR)
                .max(up[k + 1] + ROW_CLEAR);
        }
    }

    // ── M3.5 (R4) + ★ M7.4: settle the band ys BEFORE deriving anything ──────
    //
    // A free net's band must not land on a rail row (`RAIL_GAP` 40→80 moved the
    // South rail onto `moddcdc` 501's band, which A11 flags). The colliding
    // band — and everything after it — shifts down until it clears the rail
    // plus the band's own up-demand. The uniform shift preserves the pitch, so
    // A12 still holds.
    //
    // ★ M7.4 — THE BUG THIS REWRITE FIXES. The shift used to run AFTER
    // `pin_rows`, `side_rows`, `ic_top`/`ic_bottom` and the North/South rail
    // rows had already been derived from `band_y`, and only `rows[i]` (→
    // `lane.axis`) was re-derived afterwards. So the moment any shift happened,
    // every layer-anchor SIDE PIN stayed `step` px away from its OWN net's
    // trunk. `realize` then drew a long vertical tooth from the pin down to the
    // row — "the wire out of the pin bends for no reason" — and that tooth runs
    // straight through any member hanging on the way, which is the "half the
    // part sits on top of a wire" symptom. The offset is uniform across every
    // pin and every row, which is exactly what the picture shows.
    //
    // The cycle is (band ys → IC extent → rail rows → shift → band ys), so run
    // it to a fixed point and derive pins, the IC extent and the rails from the
    // settled `band_y` once, at the end. Bounded by the band count: every pass
    // either converges or moves a band strictly down.
    let mut ic_top = BASE_Y;
    let mut ic_bottom = BASE_Y + 120.0;
    for _ in 0..=band_nets.len() {
        let side_rows: Vec<f64> = pin_band.values().map(|&b| band_y[b]).collect();
        let (t, b) = side_row_extent(&side_rows).unwrap_or((BASE_Y, BASE_Y + 120.0));
        ic_top = t;
        ic_bottom = b;
        // The box grows below `ic_bottom` for pin count and NC pins; the
        // South rail must clear the FINAL bottom edge, not the row span.
        let box_bottom = final_box_bottom(
            ic_top,
            ic_bottom,
            &per_side,
            &pin_band,
            &band_y,
            &pin_plan.unassigned,
        );
        let mut rail_rows: Vec<(f64, Region)> = Vec::new();
        for region in [Region::North, Region::South] {
            let Some(list) = per_side.get(&region) else {
                continue;
            };
            let base = if region == Region::South {
                box_bottom + RAIL_GAP
            } else {
                ic_top - RAIL_GAP
            };
            for k in 0..list.len() {
                rail_rows.push((base + k as f64 * PIN_PITCH, region));
            }
        }
        let mut moved = false;
        for i in 0..n {
            if !is_free[i] {
                continue;
            }
            let Some(k) = net_band[i] else { continue };
            let yk = band_y[k];
            let collides = rail_rows.iter().any(|&(ry, rr)| {
                (yk - ry).abs() < 1.0 && !is_w_e_opposite(topos[i].lane.region, rr)
            });
            if collides {
                let step = PIN_PITCH.max(up[k] + ROW_CLEAR);
                for v in band_y.iter_mut().skip(k) {
                    *v += step;
                }
                moved = true;
                break;
            }
        }
        if !moved {
            break;
        }
    }

    // Everything below reads the SETTLED `band_y` — pins, trunks and the IC
    // extent can no longer drift apart (A27 gates this).
    let mut pin_rows: BTreeMap<i64, f64> = BTreeMap::new();
    for (&pid, &b) in &pin_band {
        pin_rows.insert(pid, band_y[b]);
    }
    for (i, b) in net_band.iter().enumerate() {
        if let Some(b) = b {
            rows[i] = Some(band_y[*b]);
            // ★ M8.2: only a genuine side PIN (a run root on the layer anchor)
            // is marked `SidePin`. A run member inherits the root's band from
            // `share_run_bands` and must keep its `Partner` source — it is not
            // an IC side pin, and the observatory's NETS `src` column would
            // misreport it otherwise.
            if !is_free[i] && topos[i].run_root == topos[i].nid {
                sources[i] = Some(RowSource::SidePin);
            }
        }
    }

    // North/South rails: independent rows hugging the box edge (M2.5 Step 5).
    // The South rail hugs the FINAL box bottom — `assign_anchor_slots` grows
    // the box below the connected rows' extent for pin count and NC pins, so
    // `ic_bottom + RAIL_GAP` would land inside the box body and the filled
    // box would hide the whole ground tree.
    let box_bottom = final_box_bottom(
        ic_top,
        ic_bottom,
        &per_side,
        &pin_band,
        &band_y,
        &pin_plan.unassigned,
    );
    for region in [Region::North, Region::South] {
        let Some(list) = per_side.get(&region) else {
            continue;
        };
        let base = if region == Region::South {
            box_bottom + RAIL_GAP
        } else {
            ic_top - RAIL_GAP
        };
        for (k, &(_, ti, _)) in list.iter().enumerate() {
            rows[ti] = Some(base + k as f64 * PIN_PITCH);
            sources[ti] = Some(RowSource::EdgeRail);
        }
    }

    // M3.2 Pass 3 (cycle break): a trunk-bearing net still unassigned here had
    // no shareable partner (a row-inheritance cycle). Open a fresh band below,
    // log it, and do NOT count it as a fallback (A1 only counts islands).
    // ── ★ M14.2: a SATELLITE's own nets get rows NEXT TO IT ──────────────────
    //
    // A net that touches no anchor pin and shares no run has, until now, fallen
    // through to the cycle break below — a fresh band under the IC. On a layer
    // whose satellite carries most of the interesting netlist that is three of
    // four nets: `mic`'s `MIC.N`, `_net2` and `_net3` all got trunks below the
    // canvas while their pins stayed on the microphone, so every one of them was
    // drawn as a wire down the left margin and back. (A27 is exactly this, and
    // it never ran here — `mic` is not one of the four fixtures.)
    //
    // The satellite is a small IC in its own right: stack its unrowed nets under
    // its shared row in PHYSICAL PIN ORDER, one `PIN_PITCH` apart, so the box
    // ends up with one pin per row like any other component. A candidate that
    // would land on an existing row steps down until it is clear, which keeps
    // the allocation deterministic without needing to know the band layout.
    for (sat, _region) in satellite_plan_for(graph, topos, layer_anchor) {
        let net_of = |pid: i64| -> Option<usize> {
            topos.iter().position(|t| {
                t.groups
                    .iter()
                    .any(|g| g.box_id == sat.box_id && g.pin_ids.contains(&pid))
            })
        };
        let pin_order: Vec<i64> = graph
            .boxes
            .iter()
            .find(|b| b.id == sat.box_id)
            .map(|b| b.pins.iter().map(|p| p.id).collect())
            .unwrap_or_default();
        // The shared row anchors the stack; without one there is nothing to be
        // adjacent to and the cycle break stays in charge.
        let Some(base) = pin_order
            .iter()
            .filter_map(|&pid| net_of(pid))
            .filter_map(|n| rows[n])
            .fold(None, |acc: Option<f64>, y| {
                Some(acc.map_or(y, |a| a.min(y)))
            })
        else {
            continue;
        };
        let mut k = 0usize;
        for pid in pin_order {
            let Some(n) = net_of(pid) else { continue };
            if rows[n].is_some() || topos[n].terminal_only || topos[n].net_kind == NetKind::Ground {
                continue;
            }
            k += 1;
            let mut y = base + k as f64 * PIN_PITCH;
            // ★ M15.1: dodge only the rows this stack could actually collide
            // with — W/E TRUNKS of nets that are not this satellite's own.
            //
            // M14.2 compared against every row in the layer, including the
            // South edge rails, which on a small anchor start barely half a
            // PIN_PITCH below the last band. Every candidate hit one, every
            // candidate stepped down, and the stack marched away from the
            // satellite one net at a time — which is how `MIC.N` ended up
            // sharing a row with `_net2` and the pins stopped matching their
            // trunks. A South rail lives below the IC and to the side of
            // nothing; it can never be confused with a W/E trunk.
            let mine: BTreeSet<usize> = graph
                .boxes
                .iter()
                .find(|b| b.id == sat.box_id)
                .map(|b| {
                    b.pins
                        .iter()
                        .filter_map(|p| net_of(p.id))
                        .collect::<BTreeSet<usize>>()
                })
                .unwrap_or_default();
            let mut guard = 0;
            while rows.iter().enumerate().any(|(j, r)| {
                !mine.contains(&j)
                    && matches!(topos[j].lane.region, Region::West | Region::East)
                    && r.is_some_and(|other| (other - y).abs() < PIN_PITCH * 0.5)
            }) {
                y += PIN_PITCH;
                guard += 1;
                if guard > 32 {
                    break;
                }
            }
            crate::vlog!(
                "[equi-tree] satellite row: net '{}' → {:.0} (pin {} on box {})",
                topos[n].net_name,
                y,
                pid,
                sat.box_id
            );
            rows[n] = Some(y);
            sources[n] = Some(RowSource::Partner(sat.box_id));
        }
    }

    // Cycle-break rows open below the FINAL box bottom too (same reasoning as
    // the South rail): a trunk below `ic_bottom` can still sit inside the box.
    let mut cycle_open = box_bottom + RAIL_GAP;
    for (i, t) in topos.iter().enumerate() {
        if t.terminal_only || rows[i].is_some() {
            continue;
        }
        crate::vlog!(
            "[equi-tree] row cycle: net '{}' (nid={}) had no shareable partner — opening a fresh band",
            t.net_name,
            t.nid
        );
        rows[i] = Some(cycle_open);
        sources[i] = Some(RowSource::Partner(-1));
        cycle_open += PIN_PITCH;
    }

    for (i, row) in rows.into_iter().enumerate() {
        if let Some(y) = row {
            topos[i].lane.axis = y;
            topos[i].lane.horizontal = true;
            topos[i].row_source = sources[i].unwrap_or(RowSource::IslandFallback);
        }
    }

    let bands = band_nets
        .into_iter()
        .enumerate()
        .map(|(k, nets)| RowBand {
            down: down[k],
            up: up[k],
            occupants: nets
                .iter()
                .map(|&ti| (topos[ti].nid, topos[ti].lane.region))
                .collect(),
        })
        .collect();

    RowPlan {
        pin_rows,
        ic_top,
        ic_bottom,
        bands,
        net_band,
    }
}

/// M2: does any NON-anchor member of `topos[idx]` connect to a net living in
/// `region`? If so, a member hangs toward that region (South → down, North →
/// up). Pure topology — reads regions only.
fn member_hangs_toward(topos: &[NetTopology], idx: usize, region: Region) -> bool {
    for (j, other) in topos.iter().enumerate() {
        if j == idx || other.lane.region != region {
            continue;
        }
        if topos[idx]
            .groups
            .iter()
            .skip(1)
            .any(|g| other.groups.iter().any(|og| og.box_id == g.box_id))
        {
            return true;
        }
    }
    false
}

/// M3.2: for a free net, pick the partner net to inherit a BAND from — the
/// smallest-nid net with an assigned band that shares one of this net's
/// NON-anchor member boxes (the unified `shares_row`-style predicate). Its own
/// anchor's placement is an accident (it is placed as a member of another
/// net), not a design input, so that net is excluded.
fn free_net_partner_band(
    topos: &[NetTopology],
    net_band: &[Option<usize>],
    idx: usize,
) -> Option<(usize, usize)> {
    let member_boxes: Vec<i64> = topos[idx].groups.iter().skip(1).map(|g| g.box_id).collect();
    let mut best: Option<(i64, usize, usize)> = None;
    for (j, other) in topos.iter().enumerate() {
        if j == idx {
            continue;
        }
        let Some(b) = net_band[j] else { continue };
        if member_boxes
            .iter()
            .any(|&bid| other.groups.iter().any(|og| og.box_id == bid))
        {
            let key = (other.nid, j, b);
            if best.map_or(true, |x| key.0 < x.0) {
                best = Some(key);
            }
        }
    }
    best.map(|(_, j, b)| (j, b))
}

/// Two regions are opposite W/E — the only pair allowed to share one row band
/// ("two taps share a row ⟺ regions are W/E-opposite", M3.2).
pub(crate) fn is_w_e_opposite(a: Region, b: Region) -> bool {
    matches!(
        (a, b),
        (Region::West, Region::East) | (Region::East, Region::West)
    )
}

/// M3.3 demand attribution, row-based: the vertical corridor a net's members
/// need above (`up`) and below (`down`) its row. Reads the partner's ROW from
/// `lane.axis` — valid on a fully-placed graph (the layout side, the render
/// replay, and the audit). `assign_rows` Phase 2 computes the same demands
/// from the band table before lanes exist.
pub(crate) fn net_corridor_demand(
    graph: &McVecGraph,
    topos: &[NetTopology],
    idx: usize,
) -> (f64, f64) {
    let my_row = topos[idx].lane.axis;
    let mut up: f64 = 0.0;
    let mut down: f64 = 0.0;
    for group in topos[idx].groups.iter().skip(1) {
        let pin_count = graph
            .boxes
            .iter()
            .find(|b| b.id == group.box_id)
            .map(|b| b.pins.len())
            .unwrap_or(2);
        if pin_count == 2 {
            match partner_info(topos, idx, group) {
                None => down = down.max(CORRIDOR_DEMAND),
                Some(p) if p.is_terminal_only || p.kind == NetKind::Ground || p.row.is_none() => {
                    down = down.max(CORRIDOR_DEMAND);
                }
                Some(p) => {
                    let pr = p.row.unwrap_or(my_row);
                    if pr > my_row + 1.0 {
                        down = down.max(CORRIDOR_DEMAND);
                    } else if pr < my_row - 1.0 {
                        up = up.max(CORRIDOR_DEMAND);
                    }
                }
            }
        } else if pin_count >= 3 {
            // Sink: region-based demand. ★ M7.6: a Sink hangs a LEAD off the row
            // and is at least MIN_SINK_H tall.
            let demand = LEAD + MIN_SINK_H;
            if member_hangs_toward(topos, idx, Region::South) {
                down = down.max(demand);
            }
            if member_hangs_toward(topos, idx, Region::North) {
                up = up.max(demand);
            }
        }
    }
    (up, down)
}

/// ★ P3: resolve one net's trunk lane — the SPAN seed and channel index. The
/// axis (row y) is owned by `assign_rows` (P0, pure topology) so that the
/// layer-anchor pins can be placed on the rows; this pass runs after the anchor
/// is placed and only seeds the span from the anchor's placed rect. `horizontal`
/// is always true (M1 row model). `envelop_lanes` recomputes the span over all
/// tap points once members are placed.
///
/// Terminal-only nets carry no trunk and keep their default lane.
fn resolve_lane_for_topo(graph: &McVecGraph, index: usize, topo: &mut NetTopology) {
    if topo.terminal_only {
        return;
    }
    topo.lane.index = index;
    topo.lane.horizontal = true;

    let (ax, _ay, aw, _ah) = anchor_box_rect(graph, topo.anchor);

    // M4.4: the old `(member_count+1)*MEMBER_GAP` outward seed is gone. Member
    // x now comes from the column allocator (not the trunk span), so this seed
    // only needs to pin the trunk's anchor-end; `envelop_lanes` recomputes the
    // full span over the placed taps right after the column placement.
    let span = match topo.lane.region {
        // W/E: start tight at the anchor edge; closing tap x comes from the
        // column placements via `envelop_lanes`.
        Region::West => (ax, ax),
        Region::East => (ax + aw, ax + aw),
        // N/S: the trunk spans the anchor's width.
        Region::North | Region::South => (ax, ax + aw),
    };
    topo.lane.span = span;
}

/// Deterministic channel index within the (anchor, region) group, by nid.
/// Order-independent: computed before any mutation so it does not depend on
/// which nets happen to resolve first.
fn lane_index_within_group(topos: &[NetTopology], topo: &NetTopology) -> usize {
    topos
        .iter()
        .filter(|t| {
            t.anchor == topo.anchor && t.lane.region == topo.lane.region && t.nid < topo.nid
        })
        .count()
}

/// Flat lane resolution over every topology — the render-side replay
/// (`build_all_trees`) and the A2 audit. On a fully placed graph every anchor
/// box exists, so every lane reads a real rect and matches the layout phase.
pub fn resolve_lanes(graph: &McVecGraph, topos: &mut [NetTopology]) {
    let indices: Vec<usize> = (0..topos.len())
        .map(|i| lane_index_within_group(topos, &topos[i]))
        .collect();
    for (i, topo) in topos.iter_mut().enumerate() {
        resolve_lane_for_topo(graph, indices[i], topo);
    }
}

/// Bounding rect of a box (fallback 0,0,120,60 when missing).
fn anchor_box_rect(graph: &McVecGraph, box_id: i64) -> (f64, f64, f64, f64) {
    graph
        .boxes
        .iter()
        .find(|b| b.id == box_id)
        .map(|b| (b.x, b.y, b.w, b.h))
        .unwrap_or((0.0, 0.0, 120.0, 60.0))
}

/// ★ PR4: span enveloping — recompute each net's lane span as the min/max of
/// ALL tap points along the trunk direction: the anchor pins plus every member's
/// entry pin (the point where its tap lands on this trunk).
///
/// This is the general fix for "the trunk does not reach the cap hanging below"
/// (device_layout_v2.md sec.5): trunk length is the envelope of the endpoint set,
/// not a hand-computed extension amount. The old per-NetKind
/// `extend_trunk_for_symbols` table (Ground +60 / Power −20 / Signal +40) is
/// deleted — it was patching a span that ignored member taps.
///
/// Runs after P4 `place_members` in the layout phase, and is replayed right
/// after `resolve_lanes` in the render phase. Both run on the same placed graph
/// (member boxes already carry PinSlots), so the recomputed span is identical —
/// keeping the `lanes_layout_match_render` invariant. `realize` then reads this
/// enveloped span and never touches a layout constant.
///
/// The span intentionally covers ONLY tap points (anchor + members). Terminal
/// symbols (Ground / NetLabel / BusLabel) do NOT extend it: `realize` hangs
/// them off the trunk's far end on their own short wire, so the trunk itself
/// never runs through another component (which extending span_hi used to do).
pub fn envelop_lanes(graph: &McVecGraph, topos: &mut [NetTopology]) {
    for topo in topos.iter_mut() {
        // Terminal-only nets have no trunk to envelop.
        if topo.terminal_only {
            continue;
        }
        // M1/M2 row model: every trunk is horizontal — the old `vertical_trunk`
        // branch is dead code (B5), removed; M4 reintroduces direction via an
        // `Axis` enum, not this flag.
        debug_assert!(topo.lane.horizontal, "row model: all trunks are horizontal");
        let mut vals: Vec<f64> = Vec::new();

        // Anchor pins: each tooth lands on the trunk at the pin's position.
        if let Some(group) = topo.groups.first() {
            if let Some(b) = graph.boxes.iter().find(|b| b.id == group.box_id) {
                let ox = topo.lane.region.outward().0;
                for &pid in &group.pin_ids {
                    if let Some(s) = slot_of(b, pid) {
                        let (px, py) = slot_point(b, s);
                        // M4.2 (A3): the trunk must reach where the anchor
                        // actually connects. If the pin sits ON the trunk row
                        // (py == axis) realize draws no tooth and connects at
                        // the pin x; otherwise the tooth is drawn TOOTH_GAP
                        // outward of the pin edge and the trunk must reach that
                        // x — using the raw pin x there leaves a dangling stub.
                        let cx = if (py - topo.lane.axis).abs() < 0.5 {
                            px
                        } else {
                            px + ox * TOOTH_GAP
                        };
                        vals.push(cx);
                    }
                }
            }
        }

        // Member taps: the exact point `realize` connects to the trunk (group's
        // first pin, read from the placed box's PinSlots).
        for group in topo.groups.iter().skip(1) {
            let Some(b) = graph.boxes.iter().find(|b| b.id == group.box_id) else {
                continue;
            };
            let (mx, _my) = member_pin_point(b, group);
            vals.push(mx);
        }

        if !vals.is_empty() {
            let lo = vals.iter().cloned().fold(f64::MAX, f64::min);
            let hi = vals.iter().cloned().fold(f64::MIN, f64::max);
            topo.lane.span = (lo, hi);
        }
    }
}

/// Absolute coordinate of a pin slot (single source of truth).
pub(crate) fn slot_point(b: &crate::vector::graph::McVecBox, s: &PinSlot) -> (f64, f64) {
    match s.side {
        EntrySide::Top => (b.x + b.w * s.offset, b.y),
        EntrySide::Bottom => (b.x + b.w * s.offset, b.y + b.h),
        EntrySide::Left => (b.x, b.y + b.h * s.offset),
        EntrySide::Right => (b.x + b.w, b.y + b.h * s.offset),
    }
}

/// P4: place member boxes, reading trunk coordinates from `topo.lane`.
/// Only runs for nets whose lane has been resolved (dependency order); members
/// already placed by another net (`geom_locked`) are skipped.
fn place_members(
    graph: &mut McVecGraph,
    topos: &[NetTopology],
    resolved: &[bool],
    drop_counter: &mut BTreeMap<i64, usize>,
    layer_anchor: i64,
) {
    for (idx, topo) in topos.iter().enumerate() {
        if resolved[idx] {
            place_members_for_topo(graph, topos, idx, topo, drop_counter, layer_anchor);
        }
    }
}

/// The anchor edge x a net's members grow from, by region: West → the anchor
/// box's left edge, East → its right edge. Using the region EDGE (not the
/// anchor group's first pin slot) guarantees a West member always sits left of
/// the IC and an East member right — the anchor group's first pin may be on a
/// different edge, which previously pushed a "West" member onto the IC's right.
/// Reads only the anchor box rect, never a member box rect, so A2 stays intact.
fn net_anchor_pin_x(graph: &McVecGraph, topo: &NetTopology) -> f64 {
    graph
        .boxes
        .iter()
        .find(|b| b.id == topo.anchor)
        .map(|b| match topo.lane.region {
            Region::West => b.x,
            Region::East => b.x + b.w,
            _ => b.x,
        })
        .unwrap_or(0.0)
}

/// M4.2b: the world's column x. `place_members_for_topo` gives every member a
/// provisional x, but a per-net allocator anchors every net at its OWN anchor
/// pin, so two nets sharing the IC edge both take col0 = same x (A21 collision).
/// This pass re-runs the allocation at the CORRECT granularity — all members of
/// a side at once, against a shared occupancy table — and overrides the W/E
/// member x. N/S rail members keep the provisional x.
///
/// ★ M8.4: where each net of a RUN starts growing its members, plus the x of
/// every ALONG part.
///
/// Up to M7 every net on a side started from the same place — the IC edge —
/// because that was the only anchor a net had. A run needs the nets laid out one
/// after another instead: net `d` starts where net `d-1` finished, with the
/// connecting part in the gap. Net `d-1`'s footprint is `COL_MARGIN` plus its
/// own (non-Along) members, so this is a **prefix sum along the run** — one
/// pass, no fixed point, which is the whole reason x does not come from the
/// column allocator here.
///
/// ★ M11: a run can no longer BRANCH. `equi_chain` hands each net's outer end to
/// exactly one bundle of parts, so a run is a PATH and `(depth, nid)` order is
/// the order the parts physically sit in. Up to M10 a fan-out claimed every
/// neighbour and this loop queued the branches nose-to-tail along one row —
/// never overlapping, but reading as a chain that is not in the netlist.
///
/// Returns `(nid -> origin x, [(Along part box id, centre x)])`. Runs after
/// `place_members_for_topo`, so member widths are final; it reads rects, which
/// is fine — `resolve_columns_for_side` is layout-only and is not replayed by
/// the render side, so A2 is not involved.
/// ★ FIX: the "along" groups of a run net — its member groups PLUS a
/// non-layer-anchor box that anchors the net itself (a chain sub-anchor such as
/// `C_DAC22`/`C_DAC330` is an Along part of its own net). Only the layer anchor
/// is never an along part.
fn chain_groups<'a>(
    topo: &'a NetTopology,
    layer_anchor: i64,
) -> impl Iterator<Item = &'a PinGroup> + 'a {
    topo.groups.iter().filter(move |g| g.box_id != layer_anchor)
}

fn chain_origins(
    graph: &McVecGraph,
    topos: &[NetTopology],
    layer_anchor: i64,
) -> (BTreeMap<i64, f64>, Vec<(i64, f64)>) {
    use crate::viz::layout::equi_column::{COL_CLEAR, COL_MARGIN};
    let mut origins: BTreeMap<i64, f64> = BTreeMap::new();
    let mut series_x: Vec<(i64, f64)> = Vec::new();

    // ★ FIX: a run's "along" groups are its members PLUS a non-layer-anchor
    // box that anchors the net itself (e.g. `C_DAC22`, `C_DAC330` — the
    // sub-anchor of a DAC chain is a Series part of its own net, so it must
    // take a place in the prefix-sum or it is only ever given a side-column
    // x near the IC edge and misses the chain completely). The layer anchor is
    // never an along part.
    let mut runs: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
    for (i, t) in topos.iter().enumerate() {
        if t.terminal_only || !matches!(t.lane.region, Region::West | Region::East) {
            continue;
        }
        runs.entry(t.run_root).or_default().push(i);
    }

    for (_root, mut members) in runs {
        members.sort_by_key(|&i| (topos[i].run_depth, topos[i].nid));
        // ★ M11: a path — depths strictly increase. A repeat means something
        // wrote `run_depth` behind `equi_chain`'s back; the prefix sum below
        // would then hand two nets the same origin. Reported, not fatal: the
        // layout still comes out, just crowded.
        if let Some(w) = members
            .windows(2)
            .find(|w| topos[w[0]].run_depth >= topos[w[1]].run_depth)
        {
            crate::vlog!(
                "[chain] run is not a path: '{}' and '{}' share depth {}",
                topos[w[0]].net_name,
                topos[w[1]].net_name,
                topos[w[0]].run_depth
            );
        }
        let Some(&first) = members.first() else {
            continue;
        };
        let dir = if topos[first].lane.region == Region::West {
            -1.0
        } else {
            1.0
        };
        let mut cursor = net_anchor_pin_x(graph, &topos[first]);
        for (k, &i) in members.iter().enumerate() {
            origins.insert(topos[i].nid, cursor);
            let mut foot = COL_MARGIN;
            for group in chain_groups(&topos[i], layer_anchor) {
                let Some(b) = graph.boxes.iter().find(|b| b.id == group.box_id) else {
                    continue;
                };
                let role = tap_role(b, &topos[i], partner_info(topos, i, group), layer_anchor);
                if matches!(role, TapRole::Series { .. }) {
                    continue;
                }
                foot += b.w.max(TWO_PIN_SYMBOL_H) + COL_CLEAR;
            }
            if k + 1 >= members.len() {
                // ★ M12.3: the run's TAIL can still carry an Along part whose
                // far net owns no trunk — an adopted terminal-only ground, or a
                // bare rail label. Such a partner never enters `members`, so the
                // joint loop below never saw it and the part kept whatever
                // provisional x the per-net allocator gave it, which the
                // side-wide pass may since have moved another member onto. Give
                // it the same prefix-sum slot every other joint gets. A
                // sub-anchor of the tail net (★ FIX, e.g. `C_DAC330`) is such an
                // Along part and lands here; anything already placed as a run
                // joint above is skipped so it is not pushed to the tail again.
                for group in chain_groups(&topos[i], layer_anchor) {
                    let Some(b) = graph.boxes.iter().find(|b| b.id == group.box_id) else {
                        continue;
                    };
                    let role = tap_role(b, &topos[i], partner_info(topos, i, group), layer_anchor);
                    if !matches!(role, TapRole::Series { .. }) {
                        continue;
                    }
                    if series_x.iter().any(|&(bid, _)| bid == group.box_id) {
                        continue;
                    }
                    series_x.push((
                        group.box_id,
                        cursor + dir * (foot + COL_CLEAR + TWO_PIN_SYMBOL_W / 2.0),
                    ));
                }
                continue;
            }
            let j = members[k + 1];
            let joint = chain_groups(&topos[i], layer_anchor)
                .map(|g| g.box_id)
                .find(|id| topos[j].groups.iter().any(|h| h.box_id == *id));
            if let Some(box_id) = joint {
                series_x.push((
                    box_id,
                    cursor + dir * (foot + COL_CLEAR + TWO_PIN_SYMBOL_W / 2.0),
                ));
            }
            cursor += dir * (foot + COL_CLEAR + TWO_PIN_SYMBOL_W + COL_CLEAR);
        }
    }

    // ★ M12.1: every arm of a ground COLUMN shares ONE x, so their cold pins
    // line up on the node's vertical and a single glyph serves them all. Each
    // arm's own candidate is the tail slot of its row (the same prefix sum used
    // above); the OUTWARD-most candidate wins, because pulling an arm inward
    // would drop it on top of a member already sitting there.
    for (gi, g) in topos.iter().enumerate() {
        if !g.ground_column {
            continue;
        }
        let mut xs: Vec<(i64, f64)> = Vec::new();
        let mut dir = 0.0f64;
        for group in &g.groups {
            let Some((li, live)) = find_partner(topos, gi, group) else {
                continue;
            };
            let d = match live.lane.region {
                Region::West => -1.0,
                Region::East => 1.0,
                _ => continue,
            };
            dir = d;
            let base = origins
                .get(&live.nid)
                .copied()
                .unwrap_or_else(|| net_anchor_pin_x(graph, live));
            let mut foot = COL_MARGIN;
            for m in live.groups.iter().skip(1) {
                if m.box_id == group.box_id {
                    continue;
                }
                let Some(b) = graph.boxes.iter().find(|b| b.id == m.box_id) else {
                    continue;
                };
                if matches!(
                    tap_role(b, live, partner_info(topos, li, m), layer_anchor),
                    TapRole::Series { .. }
                ) {
                    continue;
                }
                foot += b.w.max(TWO_PIN_SYMBOL_H) + COL_CLEAR;
            }
            xs.push((
                group.box_id,
                base + d * (foot + COL_CLEAR + TWO_PIN_SYMBOL_W / 2.0),
            ));
        }
        let Some(&(_, first)) = xs.first() else {
            continue;
        };
        let mut x_col = first;
        for &(_, x) in &xs {
            if (x - x_col) * dir > 0.0 {
                x_col = x;
            }
        }
        for (bid, _) in xs {
            series_x.push((bid, x_col));
        }
    }
    (origins, series_x)
}

/// ★ M12.4: **a vertical may hang UP.**
///
/// M7.3 pinned every ground shunt DOWN, on the reasoning that "every ground
/// row/band of the layer is BELOW". M7.5 then took that away — a ground glyph
/// now hangs one `SYMBOL_DROP` off its own trunk in the first free direction, so
/// there is no band below to aim at any more, and the pin is now a pure
/// preference. It is a good preference (grounds read best pointing down) but a
/// bad rule: on `moddcdc` the output caps hang off the `VCC_1V2` row down past
/// the `FB` row, so the FB trunk running east to its divider passes straight
/// through both drop wires.
///
/// So: keep hanging down, unless down would cross another row of the SAME side
/// inside this member's column — and only flip when there is room above, which
/// there is exactly when nothing up there would be crossed either. Runs after
/// `resolve_columns_for_side`, so x is final; the spans are not enveloped yet,
/// but every tap they will be enveloped over is already placed, so the x-extent
/// below is the same one `envelop_lanes` will arrive at.
///
/// Layout-only, like `resolve_columns_for_side` — the render side does not
/// replay member placement, so A2 is not involved.
fn flip_shunts_clear_of_rows(graph: &mut McVecGraph, topos: &[NetTopology], layer_anchor: i64) {
    use crate::viz::layout::equi_column::COL_CLEAR;
    // The x-extent each net's trunk will be enveloped to: its anchor pins and
    // its member taps, all of which are placed by now.
    let extents: Vec<(f64, f64)> = topos
        .iter()
        .map(|t| {
            let (mut lo, mut hi) = (f64::MAX, f64::MIN);
            for g in &t.groups {
                if let Some(b) = graph.boxes.iter().find(|b| b.id == g.box_id) {
                    if b.w <= 0.0 {
                        continue;
                    }
                    lo = lo.min(b.x + b.w / 2.0);
                    hi = hi.max(b.x + b.w / 2.0);
                }
            }
            (lo, hi)
        })
        .collect();

    let crossed = |me: usize, x: f64, lo_y: f64, hi_y: f64| -> bool {
        topos.iter().enumerate().any(|(j, u)| {
            if j == me || u.terminal_only || u.lane.region != topos[me].lane.region {
                return false;
            }
            if !matches!(u.lane.region, Region::West | Region::East) {
                return false;
            }
            if u.lane.axis <= lo_y + 1.0 || u.lane.axis >= hi_y - 1.0 {
                return false;
            }
            let (lo, hi) = extents[j];
            lo - COL_CLEAR <= x && x <= hi + COL_CLEAR
        })
    };

    let mut flips: Vec<(i64, f64, i64)> = Vec::new();
    for (i, t) in topos.iter().enumerate() {
        if t.terminal_only || !matches!(t.lane.region, Region::West | Region::East) {
            continue;
        }
        for group in t.groups.iter().skip(1) {
            let Some(b) = graph.boxes.iter().find(|b| b.id == group.box_id) else {
                continue;
            };
            if b.pins.len() != 2 || b.w <= 0.0 || b.h <= 0.0 {
                continue;
            }
            // Only a DROP may be flipped: a Bridge's direction is pinned by the
            // partner's row, and a Series is horizontal to begin with.
            let TapRole::Drop { dir } = tap_role(b, t, partner_info(topos, i, group), layer_anchor)
            else {
                continue;
            };
            if dir < 0.0 || b.y < t.lane.axis {
                continue; // already hanging up
            }
            // ★ M12.4b: never flip a Drop that is pinned DOWN by a ground partner
            // (`tap_role` only emits `dir > 0.0` for the M7.3 ground rule). Its
            // exit pin is the ground: hanging up puts that pin at the top and
            // forces the ground tooth to climb back OVER the member's own body
            // (`moddcdc` `_C2` = `_net1`↔GND@lp322dcdc). A down-hang tooth
            // crossing another net's trunk is a clean wire crossing; a tooth
            // through the body is not — electrical direction beats row clearance.
            if dir > 0.0 {
                continue;
            }
            let x = b.x + b.w / 2.0;
            let down_to = b.y + b.h + SYMBOL_DROP;
            let up_to = t.lane.axis - LEAD - b.h - SYMBOL_DROP;
            if !crossed(i, x, t.lane.axis, down_to) {
                continue;
            }
            if crossed(i, x, up_to, t.lane.axis) {
                continue; // no better up there
            }
            let Some(&pid) = group.pin_ids.first() else {
                continue;
            };
            crate::vlog!(
                "[members] '{}' on net '{}' flips UP — hanging down crosses another row at x={:.0}",
                b.name,
                t.net_name,
                x
            );
            flips.push((b.id, t.lane.axis - LEAD - b.h, pid));
        }
    }
    for (bid, ny, pid) in flips {
        if let Some(b) = graph.boxes.iter_mut().find(|b| b.id == bid) {
            b.y = ny;
            assign_shunt_slots(b, pid, EntrySide::Bottom);
        }
    }
}

fn resolve_columns_for_side(graph: &mut McVecGraph, topos: &[NetTopology], layer_anchor: i64) {
    use crate::viz::layout::equi_column::{allocate_columns_for_side, SideMember};
    // ★ M8.4: every net grows from its own place along the run, not from the
    // shared IC edge; Along parts sit in the gaps and skip the allocator.
    let (origins, series_x) = chain_origins(graph, topos, layer_anchor);
    for (box_id, cx) in &series_x {
        if let Some(b) = graph.boxes.iter_mut().find(|b| b.id == *box_id) {
            b.x = cx - b.w / 2.0;
        }
    }
    let mut west: Vec<SideMember> = Vec::new();
    let mut east: Vec<SideMember> = Vec::new();
    // ★ M7.1: a two-pin member belongs to BOTH of the nets it joins, so it is
    // reachable twice here. Before the coupling pass the two owners always sat
    // on opposite sides (a same-row pair is W/E-opposite by A11), so the two
    // entries landed in different lists and never met. Coupled nets are now on
    // the SAME side, where the two entries would take two different columns
    // against a shared occupancy table and the second write would silently move
    // the box off the first one's column — leaving a hole and a mis-aimed
    // bridge. Allocate each member box exactly ONCE, first owner in topo order
    // (the same order `place_members` uses, so the column matches the y).
    let mut claimed: BTreeSet<i64> = BTreeSet::new();
    // ★ M16.3b: boxes some W/E net owns as a REGULAR member (gi > 0). A 2-pin
    // sub-anchor that also appears as a regular member of another W/E net must
    // keep that placement — its own net's pass runs first in topo order and
    // would otherwise hijack the box with its own (possibly far) run origin,
    // yanking it off the member net's column (`moddcdc` `_R2`: VCC_1V2's
    // anchor, also `_net15`'s bridge — re-allocating it from VCC_1V2's origin
    // put it 250px west with its pin off the span).
    let mut member_owned: BTreeSet<i64> = BTreeSet::new();
    for topo in topos.iter() {
        if !matches!(topo.lane.region, Region::West | Region::East) {
            continue;
        }
        for g in topo.groups.iter().skip(1) {
            member_owned.insert(g.box_id);
        }
    }
    for (ti, topo) in topos.iter().enumerate() {
        let is_east = match topo.lane.region {
            Region::West => false,
            Region::East => true,
            _ => continue,
        };
        // ★ M12.1: a ground COLUMN owns no member columns — every arm is
        // allocated by the live net whose row it sits on.
        if topo.ground_column {
            continue;
        }
        let base_x = origins
            .get(&topo.nid)
            .copied()
            .unwrap_or_else(|| net_anchor_pin_x(graph, topo));
        let outward = if is_east { 1.0 } else { -1.0 };
        for (gi, group) in topo.groups.iter().enumerate().filter(|(_, g)| {
            // The layer anchor is placed by P2 — never re-allocated here.
            if g.box_id == layer_anchor {
                return false;
            }
            // A terminal-only net's anchor has a degenerate row; never re-allocate
            // it (same guard as `place_members_for_topo`).
            if topo.terminal_only && g.box_id == topo.anchor {
                return false;
            }
            // ★ M16.3: a run's SUB-ANCHOR — a non-layer 2-pin part that anchors
            // its own net (e.g. the DAC junction `_R3`) — is a member like any
            // other and must be re-allocated on the run. The old `skip(1)` left
            // it at the provisional x P4's `span_lo` fallback gave it, far left
            // of the junction, so the junction trunk had to cross the whole IC
            // to reach it. Multi-pin Sink anchors keep their P4 x.
            if g.box_id == topo.anchor {
                // ★ M16.3b: but NOT when another W/E net already owns the box
                // as a regular member — that net's column is authoritative and
                // this one would only drag it to its own origin.
                if member_owned.contains(&g.box_id) {
                    return false;
                }
                return graph
                    .boxes
                    .iter()
                    .any(|b| b.id == g.box_id && b.pins.len() == 2);
            }
            true
        }) {
            let Some(b) = graph.boxes.iter().find(|b| b.id == group.box_id) else {
                continue;
            };
            if b.w <= 0.0 || b.h <= 0.0 {
                continue;
            }
            if !claimed.insert(group.box_id) {
                continue;
            }
            let partner = partner_info(topos, ti, group);
            let role = tap_role(b, topo, partner.clone(), layer_anchor);
            // ★ M8.4: Along parts already have their x from the prefix sum.
            if matches!(role, TapRole::Series { .. }) {
                continue;
            }
            // ★ M12.2: a member shared with ANOTHER net has to clear BOTH nets'
            // origins. `moddcdc`'s divider `_R2` belongs to `VCC_1V2` (which
            // starts east of the inductor, being depth 1 of `LX`'s run) and to
            // `_net5` (which starts at the FB pin). Allocating it against
            // whichever owner came first in topo order put it at the FB origin —
            // WEST of the inductor — so `VCC_1V2`'s trunk had to reach back over
            // `_net3`'s trunk to get to it. That is the red wire crossing the
            // purple one on the right of the picture, and it is an A29 (run
            // trunks disjoint) violation as well as an A24 crossing.
            //
            // The outward-most origin is the only x that satisfies both.
            let mut anchor_pin_x = base_x;
            if let Some(p) = &partner {
                // ★ M15.8: only push a shared member outward when both nets sit
                // on the SAME component. `mic`'s bridge cap `C1` joins `MIC.N`
                // (anchored on `mic`) to `MIC.P` (anchored on `wm7121`): the
                // partner origin is the OTHER component's pin, and pushing the
                // bridge there tucks it under that box — dragging `MIC.N`'s
                // trunk underneath `wm7121` so the picture reads as if `mic.2`
                // reached `wm7121.2`. A cross-component bridge belongs in the
                // gap, anchored at its own net's pin.
                if topos
                    .get(p.topo_idx)
                    .is_some_and(|o| o.anchor == topo.anchor)
                {
                    if let Some(&po) = topos.get(p.topo_idx).and_then(|o| origins.get(&o.nid)) {
                        if (po - anchor_pin_x) * outward > 0.0 {
                            anchor_pin_x = po;
                        }
                    }
                }
            }
            let m = SideMember {
                idx: Some((ti, gi)),
                role,
                w: b.w,
                h: b.h,
                row_y: topo.lane.axis,
                anchor_pin_x,
            };
            if is_east {
                east.push(m);
            } else {
                west.push(m);
            }
        }
    }
    let out_west = allocate_columns_for_side(&west, -1.0);
    let out_east = allocate_columns_for_side(&east, 1.0);
    for (ti, gi, x) in out_west.into_iter().chain(out_east) {
        let Some(g) = topos.get(ti).and_then(|t| t.groups.get(gi)) else {
            continue;
        };
        if let Some(b) = graph.boxes.iter_mut().find(|b| b.id == g.box_id) {
            b.x = x - b.w / 2.0;
        }
    }
}

// ============================================================================
// ★ M3.3: TapRole — electrical role by partner ROW (device_layout_v2.md sec.3.3)
// ============================================================================

/// Electrical role of a member box, decided by where the member's OTHER pin's
/// net ROW lies relative to this net's row — the formal answer to "which way
/// does the member go". Replaces the M2 region-based `MemberRole` ("which side
/// of the IC" was never the right answer to "which way the member runs").
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TapRole {
    /// Partner net on the same row — the member spans HORIZONTALLY between the
    /// two trunks (pins Left/Right).
    Series { partner: usize },
    /// Partner net on a different row — the member hangs VERTICALLY off this
    /// row, directed toward the partner's row (pins Top/Bottom).
    Bridge { partner: usize, dir: f64 },
    /// No partner / terminal-only partner / Ground partner — hangs off this
    /// row (pins Top/Bottom).
    ///
    /// ★ M7.3: `dir` is now three-valued.
    ///   * `+1` — **pinned down**: the far pin belongs to a Ground net, and
    ///     every ground row/band of the layer is BELOW. Flipping such a shunt
    ///     up makes its ground pin the topmost point, so the ground tooth has
    ///     to climb back over the body to reach the rail — the stray vertical
    ///     above `modldo` `_C2` and `moddcdc` `_C2`.
    ///   * `-1` — pinned up (partner row above this one).
    ///   * ` 0` — **free**: no partner row constrains it, so
    ///     `place_members_for_topo` may alternate it up/down for A26 balance.
    Drop { dir: f64 },
    /// Three+ pins — distributed along the trunk, pins face the trunk.
    Sink,
    /// Single pin — end of the line.
    InlineEnd,
}

impl TapRole {
    /// Short string for the observatory's TAPS `role` column (M0 froze the
    /// column as `None` until M3).
    pub fn short(&self) -> &'static str {
        match self {
            TapRole::Series { .. } => "Series",
            TapRole::Bridge { .. } => "Bridge",
            TapRole::Drop { .. } => "Drop",
            TapRole::Sink => "Sink",
            TapRole::InlineEnd => "InlineEnd",
        }
    }
}

/// The net a member's OTHER pin connects to, plus that net's row.
#[derive(Debug, Clone)]
pub(crate) struct PartnerInfo {
    pub(crate) topo_idx: usize,
    row: Option<f64>,
    kind: NetKind,
    is_terminal_only: bool,
    /// ★ M8.3: the partner's run root. Equal to mine ⇒ the part between us
    /// lies ALONG the row.
    run_root: i64,
    /// ★ M8.3: the partner's depth along that run — decides which of the two
    /// pins faces the anchor.
    run_depth: usize,
    /// ★ M12.1: the partner is a ground COLUMN, so the part into it is
    /// horizontal on MY row regardless of where the node's own row is.
    ground_column: bool,
}

/// Find the net that shares this member box with `topos[idx]`, on a pin this
/// net does not own. The partner's row is read from `lane.axis`, which
/// `assign_rows` (P0) sets for every trunk-bearing net BEFORE any member is
/// placed — so a free-net partner's row is already valid here even though that
/// partner's own lane is not "resolved" until later in dependency order.
pub(crate) fn partner_info(
    topos: &[NetTopology],
    idx: usize,
    group: &PinGroup,
) -> Option<PartnerInfo> {
    find_partner(topos, idx, group).map(|(j, other)| PartnerInfo {
        topo_idx: j,
        row: if other.lane.horizontal {
            Some(other.lane.axis)
        } else {
            None
        },
        kind: other.net_kind.clone(),
        is_terminal_only: other.terminal_only,
        run_root: other.run_root,
        run_depth: other.run_depth,
        ground_column: other.ground_column,
    })
}

/// The net that shares `group.box_id` with `topos[idx]` on a pin that `idx`
/// does not own. Row-agnostic — the callers read the partner's row themselves
/// (from `lane.axis`, or from the band table during row allocation).
fn find_partner<'a>(
    topos: &'a [NetTopology],
    idx: usize,
    group: &PinGroup,
) -> Option<(usize, &'a NetTopology)> {
    let this_pins: std::collections::BTreeSet<i64> = group.pin_ids.iter().cloned().collect();
    for (j, other) in topos.iter().enumerate() {
        if j == idx {
            continue;
        }
        if let Some(g) = other.groups.iter().find(|g| g.box_id == group.box_id) {
            if g.pin_ids.iter().any(|p| !this_pins.contains(p)) {
                return Some((j, other));
            }
        }
    }
    None
}

/// M3.3: role of a member, from its partner net's row vs this net's row
/// (plan M3.3 — multi-pin boxes are `Sink`, single pins `InlineEnd`).
pub(crate) fn tap_role(
    member: &crate::vector::graph::McVecBox,
    me: &NetTopology,
    p: Option<PartnerInfo>,
    layer_anchor: i64,
) -> TapRole {
    let my_row = me.lane.axis;
    match member.pins.len() {
        0 | 1 => TapRole::InlineEnd,
        n if n >= 3 => TapRole::Sink,
        _ => match p {
            // ★ M7.3: no partner at all → nothing constrains the hang
            // direction, the balance counter owns it.
            None => TapRole::Drop { dir: 0.0 },
            // ★ M8.3: same RUN ⇒ the part extends this endpoint outward, so it
            // lies ALONG the row and both nets are collinear. This is the branch
            // that makes `Series` reachable at all — under the M7 row-delta rule
            // two nets on one side could never share a row, so every part came
            // out vertical.
            //
            // ★ M8.6 — must come BEFORE the terminal-only guard below: a run
            // ends at a labelled ENDPOINT, and that endpoint net is very often
            // terminal-only (`speaker` `VDD_3V3` = one group + its rail label).
            // The part INTO it lies ALONG the row (the mute `_R1`), so a same-run
            // partner is Series even when it owns no trunk of its own.
            //
            // ★ M10.3 — and it must now come before the GROUND guard too. A
            // ground adopted as this run's outer end (`equi_chain` step 3.5)
            // carries this run's `run_root`, which is precisely the statement
            // "the cap lies along the row and the glyph is its far end".
            Some(p) if p.run_root == me.run_root && me.net_kind != NetKind::Ground => {
                TapRole::Series {
                    partner: p.topo_idx,
                }
            }
            // ★ M12.1: a GROUND COLUMN partner. The node is a column, not a
            // row: the part lies ALONG my row and stops at the column's x, and
            // the node's own short vertical joins the cold pins. Deliberately
            // NOT collinear — A28 exempts a ground-column partner.
            Some(p) if p.ground_column && me.net_kind != NetKind::Ground => TapRole::Series {
                partner: p.topo_idx,
            },
            // ★ M16: a decoupling cap hung to its OWN terminal-only ground (a
            // per-consumer `GND@xx` = the cap's second pin + a glyph) from a
            // Power net that is anchored DIRECTLY on the IC (the layer anchor)
            // lies ALONG the power trunk — horizontal, one pin on the rail, the
            // far pin carrying the ground symbol (`us513` `_C1`/`_C2`). A decap
            // on a power net anchored on a passive chain element (moddcdc
            // `VCC_1V2` anchored on `IND_1`) stays a vertical Drop: several
            // such filters share one rail row, and horizontal would overlap. A
            // SHARED ground rail still hangs DOWN via the guard below.
            Some(p)
                if p.kind == NetKind::Ground
                    && p.is_terminal_only
                    && me.net_kind != NetKind::Ground
                    && me.anchor == layer_anchor =>
            {
                TapRole::Series {
                    partner: p.topo_idx,
                }
            }
            // ★ M7.3: a GROUND partner that was NOT adopted is pinned DOWN —
            // ground rails and the shared ground band are always below the side
            // rows, so an upward shunt would route its ground pin back over its
            // own body.
            Some(p) if p.kind == NetKind::Ground => TapRole::Drop { dir: 1.0 },
            // A terminal-only partner OUTSIDE the run has no trunk of its own;
            // its glyph hangs wherever this member ends up, so the direction
            // stays free.
            Some(p) if p.is_terminal_only => TapRole::Drop { dir: 0.0 },
            Some(p) => match p.row {
                None => TapRole::Drop { dir: 0.0 },
                Some(r) if (r - my_row).abs() < 1.0 => TapRole::Series {
                    partner: p.topo_idx,
                },
                Some(r) => TapRole::Bridge {
                    partner: p.topo_idx,
                    dir: (r - my_row).signum(),
                },
            },
        },
    }
}

/// Shunt slots: the pin on this net (entry) faces the tap, the other pin
/// (exit) faces the hang direction (outward). Both at mid-edge.
fn assign_shunt_slots(
    b: &mut crate::vector::graph::McVecBox,
    entry_pin_id: i64,
    entry_side: EntrySide,
) {
    let exit_side = opposite_side(entry_side);
    let connected: std::collections::HashSet<i64> =
        b.entry_points.iter().map(|ep| ep.pin_id).collect();
    b.slots.clear();
    for (i, p) in b.pins.iter().enumerate() {
        let side = if p.id == entry_pin_id {
            entry_side
        } else {
            exit_side
        };
        let name = if p.description.is_empty() {
            p.pin_id.clone()
        } else {
            p.description.clone()
        };
        b.slots.push(PinSlot {
            pin_id: p.id,
            number: i as u32,
            name,
            side,
            offset: 0.5,
            connected: connected.contains(&p.id),
        });
    }
    // entry_points connectivity-only side sync
    for ep in b.entry_points.iter_mut() {
        if let Some(slot) = b.slots.iter().find(|s| s.pin_id == ep.pin_id) {
            ep.side = slot.side;
            ep.offset = slot.offset;
        }
    }
}

/// Place the non-anchor member boxes of one net by their electrical role
/// (M3.4: the role comes from the partner net's ROW via [`tap_role`]).
///   * `Bridge`/`Drop` — vertical body hanging off this row toward the
///     partner's row (pins Top/Bottom, entry faces the trunk);
///   * `Series` — horizontal body spanning two trunks on the SAME row
///     (pins Left/Right, entry faces the trunk);
///   * `Sink` — multi-pin device along the trunk, pins face the trunk;
///   * `InlineEnd` — single-pin end-of-line.
fn place_members_for_topo(
    graph: &mut McVecGraph,
    topos: &[NetTopology],
    idx: usize,
    topo: &NetTopology,
    drop_counter: &mut BTreeMap<i64, usize>,
    layer_anchor: i64,
) {
    let (_dx, dy) = topo.lane.region.outward();
    // ★ M7.2: a W/E row has `outward().1 == 0`, so any placement written as
    // `axis + dy * something` collapses onto the trunk itself. Name the case
    // instead of comparing a float to zero.
    let on_side = matches!(topo.lane.region, Region::West | Region::East);
    // M1/M2 row model: all trunks horizontal — the old `vertical_trunk` branch
    // is dead (B5), removed.
    debug_assert!(topo.lane.horizontal, "row model: all trunks are horizontal");
    let axis = topo.lane.axis;
    // ★ M12.1: a ground COLUMN places none of its own members. Each arm sits on
    // the LIVE net's row, not on the node's, so letting the node place them
    // first (whichever net the fixed point reaches first) would centre them on
    // the wrong row.
    if topo.ground_column {
        return;
    }
    // Distribute members along the column model (M4.2). The span is only used
    // as a fallback anchor x here; member x comes from the column allocator.
    let (span_lo, _span_hi) = topo.lane.span;

    // M4.2: the same column allocator is used for the members AND for a
    // non-layer-anchor box that anchors this net (★ FIX). Up to now `skip(1)`
    // dropped the anchor group entirely, so a chain's sub-anchor (e.g.
    // `C_DAC22`/`C_DAC330`) was its own net's only owner and was never placed —
    // zero-size + fallback tiling, and worse, its net could never resolve.
    // We therefore place any group whose box is not the layer anchor; the
    // geom_locked guard below keeps already-placed satellites/members out.
    let non_anchor: Vec<&PinGroup> = topo
        .groups
        .iter()
        .filter(|g| {
            if g.box_id == layer_anchor {
                return false;
            }
            // ★ FIX: a TERMINAL-ONLY net carries no trunk and its row is
            // degenerate (often `IslandFallback`, row 0), so it must never
            // plant its own anchor box here — otherwise `DAC_OUT` would drop
            // `C_DAC330` at row 0 before the real chain net `_net44` gets to
            // sit it on the shared chain row 780. Non-anchor members (rare in
            // a terminal net) still place as usual.
            if topo.terminal_only && g.box_id == topo.anchor {
                return false;
            }
            true
        })
        .collect();
    let member_count = non_anchor.len();
    if member_count == 0 {
        return;
    }

    // The side of a member box that faces the trunk (opposite the region's
    // entry edge). Used for Sink and as the fallback for InlineEnd.
    let inner_side = opposite_side(topo.lane.region.entry_side());

    // M4.2: replace frac-based x with the pure column allocator. Build member
    // views for the members actually placed here (non-`geom_locked`), allocate
    // columns, then place each at its column x. The allocator reads no box
    // rect, so A2 stays intact; member x no longer depends on the trunk span,
    // killing the D4 cycle.
    let side = match topo.lane.region {
        Region::West => -1.0,
        _ => 1.0,
    };
    // Anchor pin x — the lateral reference the column allocator grows members
    // from. For a normal net this is the layer-anchor box's pin edge. A net
    // whose own (non-layer) anchor box is placed HERE for the first time (★
    // FIX) has no PinSlot yet, so the slot lookup falls back to the trunk
    // anchor-end `span_lo` instead of panicking.
    let anchor_pin_x = topo
        .groups
        .first()
        .and_then(|g| g.pin_ids.first().copied())
        .and_then(|pid| {
            let ab = graph.boxes.iter().find(|b| b.id == topo.anchor)?;
            let s = slot_of(ab, pid)?;
            Some(slot_point(ab, s).0)
        })
        .unwrap_or(span_lo);

    // Pass 1: collect placeable members (role + dims + row span from topo,
    // never from box rects).
    let mut entries: Vec<(i64, i64, super::equi_column::MemberView)> = Vec::new();
    for group in &non_anchor {
        if graph
            .boxes
            .iter()
            .any(|b| b.id == group.box_id && b.geom_locked)
        {
            continue;
        }
        let member_box = graph
            .boxes
            .iter()
            .find(|b| b.id == group.box_id)
            .expect("member box exists");
        let partner = partner_info(topos, idx, group);
        let role = tap_role(member_box, topo, partner, layer_anchor);
        let (w, h) = match &role {
            TapRole::Series { .. } => (TWO_PIN_SYMBOL_W, TWO_PIN_SYMBOL_H),
            TapRole::Bridge { .. } | TapRole::Drop { .. } => (TWO_PIN_SYMBOL_H, TWO_PIN_SYMBOL_W),
            TapRole::InlineEnd => (member_box.w.max(40.0), member_box.h.max(20.0)),
            // ★ M7.6: a Sink is a real component — size it from its labels here
            // so the column allocator reserves the right width for it (otherwise
            // it reserves 80 against a box drawn 180 wide and overlaps a neighbour).
            TapRole::Sink => {
                let (t, bm, _) = sink_pin_sides(member_box, topos);
                sink_box_size(member_box, &t, &bm)
            }
        };
        let partner_y = match &role {
            TapRole::Bridge { partner: p, .. } | TapRole::Series { partner: p } => {
                Some(topos[*p].lane.axis)
            }
            _ => None,
        };
        entries.push((
            group.box_id,
            group.pin_ids.first().copied().unwrap_or_default(),
            super::equi_column::MemberView {
                role,
                w,
                h,
                row_y: axis,
                partner_y,
            },
        ));
    }
    if entries.is_empty() {
        return;
    }
    let views: Vec<super::equi_column::MemberView> =
        entries.iter().map(|(_, _, v)| v.clone()).collect();
    let col_plan = super::equi_column::allocate_columns(&views, anchor_pin_x, side);

    // M5.3: the Drop hang direction comes from the shared per-row counter
    // (created in `place_by_topology`), so shunts on the SAME row alternate
    // up/down even across DIFFERENT nets (A26). Rows are fixed before the
    // fixed point, so the order is deterministic.
    //
    // ★ M7.3: the counter only owns the direction of a **free** Drop
    // (`dir == 0.0`). A Drop into a Ground net is pinned DOWN by `tap_role`,
    // because every ground row and the shared ground band lie below the side
    // rows; flipping it up for cosmetic balance put the member's ground pin at
    // the TOP and forced the ground tooth to climb back over the body — the
    // stray upward capacitor in `modldo` (`_C2`, VCC↔GND_OUT) and in `moddcdc`
    // (`_C2`, _net1↔GND@lp322dcdc). Electrical direction beats balance.
    let row_key = (axis * 10.0).round() as i64;

    // Pass 2: place each member at its column centreline, keeping the M3 role
    // orientation + y exactly as before.
    for (k, (box_id, entry_pin, view)) in entries.iter().enumerate() {
        let line_x = col_plan.x_values[col_plan.slots[k].col_idx];
        let Some(member_box) = graph.boxes.iter_mut().find(|b| b.id == *box_id) else {
            continue;
        };
        if member_box.geom_locked {
            continue;
        }
        match &view.role {
            TapRole::Series { partner } => {
                member_box.w = view.w;
                member_box.h = view.h;
                member_box.x = line_x - view.w / 2.0;
                member_box.y = axis - view.h / 2.0;
                member_box.geom_locked = true;
                // ★ M8.3: which pin faces the anchor is decided by DEPTH, not by
                // the region. Both nets of an Along part sit on one row and one
                // of them is further out; the region is the same for both, so
                // the old region-based side put the outer net's pin on the wrong
                // end and the two spans crossed through the body.
                let toward_anchor = match topo.lane.region {
                    Region::West => EntrySide::Right,
                    Region::East => EntrySide::Left,
                    Region::North => EntrySide::Bottom,
                    Region::South => EntrySide::Top,
                };
                // I am the INNER net of the pair when my depth is the smaller
                // one, and then my pin is the one facing the anchor.
                let i_am_inner = match topos.get(*partner) {
                    Some(o) => topo.run_depth <= o.run_depth,
                    None => true,
                };
                let entry_side = if i_am_inner {
                    toward_anchor
                } else {
                    opposite_side(toward_anchor)
                };
                assign_shunt_slots(member_box, *entry_pin, entry_side);
            }
            TapRole::Bridge { dir, .. } => {
                member_box.w = view.w;
                member_box.h = view.h;
                member_box.x = line_x - view.w / 2.0;
                member_box.y = if *dir > 0.0 {
                    axis + LEAD
                } else {
                    axis - LEAD - view.h
                };
                member_box.geom_locked = true;
                let entry_side = if *dir > 0.0 {
                    EntrySide::Top
                } else {
                    EntrySide::Bottom
                };
                assign_shunt_slots(member_box, *entry_pin, entry_side);
            }
            TapRole::Drop { dir } => {
                member_box.w = view.w;
                member_box.h = view.h;
                member_box.x = line_x - view.w / 2.0;
                // ★ M7.3: a pinned direction (Ground partner, or a partner row
                // on a known side) wins; only a free Drop alternates per row.
                let up = if *dir != 0.0 {
                    *dir < 0.0
                } else {
                    let e = drop_counter.entry(row_key).or_default();
                    let up = *e % 2 == 1;
                    *e += 1;
                    up
                };
                member_box.y = if up {
                    axis - LEAD - view.h
                } else {
                    axis + LEAD
                };
                member_box.geom_locked = true;
                let entry_side = if up {
                    EntrySide::Bottom
                } else {
                    EntrySide::Top
                };
                assign_shunt_slots(member_box, *entry_pin, entry_side);
            }
            TapRole::InlineEnd => {
                member_box.w = view.w;
                member_box.h = view.h;
                // ★ M7.2: on a W/E row `dy == 0`, so the old
                // `axis + dy * (h/2 + MEMBER_GAP)` put the box's TOP EDGE
                // exactly on the trunk and left its mid-edge pin h/2 below it —
                // `realize` then drew the tap segment straight down the box's
                // own border ("the bus runs over the component", `usbsock`
                // TP1). Worse, the trunk's outer end then coincided with that
                // box, `segment_hits_box` counts grazing as a hit, every stub
                // direction was rejected and the net's label fell back onto the
                // IC. Hang the body off the row instead (pin on top, one LEAD
                // of wire): nothing straddles a W/E trunk any more, so the
                // outer end is clean and `realize`'s outward walk can find it.
                if on_side {
                    member_box.x = line_x - view.w / 2.0;
                    // ★ M8.8: a W/E test point hangs a little lower so the
                    // left/right signal band clears the power rows above
                    // (speaker `TP1` under the VDD trunk).
                    member_box.y = axis + SIDE_HANG;
                    member_box.geom_locked = true;
                    for ep in &mut member_box.entry_points {
                        ep.side = EntrySide::Top;
                    }
                    assign_pin_slots(member_box, EntrySide::Top);
                } else {
                    let side2 = if k % 2 == 0 { -1.0 } else { 1.0 };
                    member_box.x = line_x + side2 * (view.w / 2.0 + MEMBER_GAP);
                    member_box.y = axis + dy * (view.h / 2.0 + MEMBER_GAP);
                    member_box.geom_locked = true;
                    for ep in &mut member_box.entry_points {
                        ep.side = inner_side;
                    }
                    assign_pin_slots(member_box, inner_side);
                }
            }
            TapRole::Sink => {
                // ★ M7.6: size + pin it like the component it is (see
                // `sink_pin_sides`). ★ M7.2: it hangs OFF the row rather than
                // straddling it — a W/E Sink used to sit with its top edge on the
                // trunk (`dy == 0`) and its pins on the Left/Right edge, so every
                // tap ran along the box border.
                let (top, bottom, connected) = sink_pin_sides(member_box, topos);
                member_box.w = view.w;
                member_box.h = view.h;
                member_box.x = line_x - view.w / 2.0;
                member_box.y = if on_side {
                    axis + LEAD
                } else {
                    axis + dy * MEMBER_GAP
                };
                member_box.geom_locked = true;
                assign_sink_slots(member_box, &top, &bottom, &connected);
            }
        }
    }
}

fn opposite_side(side: EntrySide) -> EntrySide {
    match side {
        EntrySide::Top => EntrySide::Bottom,
        EntrySide::Bottom => EntrySide::Top,
        EntrySide::Left => EntrySide::Right,
        EntrySide::Right => EntrySide::Left,
    }
}

/// M2.5 Step 4: the max pin-label width on one side, from the physical pins
/// (same `description / pin_id` naming rule as the slot builder).
fn side_label_width(b: &crate::vector::graph::McVecBox, pin_ids: &[i64]) -> f64 {
    let mut max_chars = 0usize;
    for &pid in pin_ids {
        let name = b
            .pins
            .iter()
            .find(|p| p.id == pid)
            .map(|p| {
                if p.description.is_empty() {
                    p.pin_id.clone()
                } else {
                    p.description.clone()
                }
            })
            .unwrap_or_default();
        max_chars = max_chars.max(name.chars().count());
    }
    max_chars as f64 * LABEL_CHAR_W
}

/// ★ P2: assign anchor pin slots by Region (device_layout_v2.md sec.4).
/// Pins inherit the Region of the net they belong to; box size is driven by the
/// single most-crowded side, not the total pin count. Row y's and the IC extent
/// come exclusively from the `RowPlan` produced by P0 `assign_rows` (single
/// source of truth — pin-offset ownership inversion).
///
/// M2.5 Step 4: the box height is `max(row span, pin-count pitch)` and the box
/// width fits the pin labels on both sides, so a collapsed row span can no
/// longer shrink the IC into a strip that spills its labels.
fn assign_anchor_slots(
    graph: &mut McVecGraph,
    anchor_id: i64,
    topos: &[NetTopology],
    plan: &RowPlan,
) {
    let Some(anchor_box) = graph.boxes.iter_mut().find(|b| b.id == anchor_id) else {
        return;
    };

    // pin_id → EntrySide, from the nets this pin belongs to. The row y is NOT
    // duplicated here — it lives in `plan.pin_rows`.
    let mut pin_side: BTreeMap<i64, EntrySide> = BTreeMap::new();
    for topo in topos.iter().filter(|t| t.anchor == anchor_id) {
        let side = topo.lane.region.entry_side();
        if let Some(g) = topo.groups.first() {
            for &pid in &g.pin_ids {
                pin_side.entry(pid).or_insert(side);
            }
        }
    }

    // Bucket physical pins by side, preserving physical order for stability.
    // NC pins (no side — belong to no topo) go to a separate `unassigned` group
    // (M2.5 Step 3) instead of silently landing on the default Right side.
    let mut west: Vec<i64> = Vec::new();
    let mut east: Vec<i64> = Vec::new();
    let mut north: Vec<i64> = Vec::new();
    let mut south: Vec<i64> = Vec::new();
    let mut unassigned: Vec<i64> = Vec::new();
    for p in &anchor_box.pins {
        match pin_side.get(&p.id).copied() {
            Some(EntrySide::Left) => west.push(p.id),
            Some(EntrySide::Right) => east.push(p.id),
            Some(EntrySide::Top) => north.push(p.id),
            Some(EntrySide::Bottom) => south.push(p.id),
            None => unassigned.push(p.id),
        }
    }

    // Box vertical extent (M2.5 Step 4): take the LARGER of the row span and
    // the pin-count pitch, so a collapsed row span cannot shrink the box below
    // what the pins need.
    let box_y = plan.ic_top;
    let span_h = (plan.ic_bottom - plan.ic_top).max(0.0);
    let count_h = west.len().max(east.len()).max(1) as f64 * PIN_PITCH + 2.0 * PIN_MARGIN;
    let mut box_h = span_h.max(count_h);

    // M2.5 Step 3: NC pins all go on the right, BELOW the connected right pins,
    // one PIN_PITCH apart. Their rows join the map so every L/R pin has one,
    // and the box grows to cover them.
    let mut pin_rows = plan.pin_rows.clone();
    if !unassigned.is_empty() {
        let right_max = east
            .iter()
            .filter_map(|pid| plan.pin_rows.get(pid))
            .cloned()
            .fold(f64::MIN, f64::max);
        let base = if right_max > f64::MIN {
            right_max
        } else {
            plan.ic_top + PIN_MARGIN
        };
        for (k, &pid) in unassigned.iter().enumerate() {
            let y = base + (k as f64 + 1.0) * PIN_PITCH;
            pin_rows.insert(pid, y);
            box_h = box_h.max(y + PIN_MARGIN - box_y);
        }
        east.extend(unassigned.iter().cloned());
    }

    // Box width (M2.5 Step 4): fit the pin labels on both sides plus padding.
    let left_w = side_label_width(anchor_box, &west);
    let right_w = side_label_width(anchor_box, &east);
    let label_w = left_w + right_w + 3.0 * LABEL_PAD;
    // M3.5 (R2, fixed): the top/bottom pins sit at `(i+1)/(n+1)` along the box
    // width, so the slot spacing is `w/(n+1)` — the width must be
    // `(n+1)*(label+pad)` to satisfy A14, NOT `n*(...)` (a 2..3-pin bottom edge
    // with long labels used to come up 1px short and fail). Each edge is
    // computed separately: taking the max of both label widths against the max
    // of both pin counts cross-pollutes a wide-label edge with the other edge's
    // pin count.
    let tb_pin_w = [&north, &south]
        .iter()
        .map(|pins| {
            if pins.is_empty() {
                0.0
            } else {
                (pins.len() as f64 + 1.0) * (side_label_width(anchor_box, pins) + LABEL_PAD)
            }
        })
        .fold(0.0f64, f64::max);
    let pin_w = north.len().max(south.len()) as f64 * PIN_PITCH + 2.0 * PIN_MARGIN;
    let box_w = label_w.max(pin_w).max(tb_pin_w).max(MIN_BOX_W);
    anchor_box.x = 80.0;
    anchor_box.y = box_y;
    anchor_box.w = box_w;
    anchor_box.h = box_h;
    anchor_box.geom_locked = true;

    // Assign slots per side.
    anchor_box.slots.clear();
    assign_side_slots(anchor_box, &west, EntrySide::Left, &pin_rows, box_y, box_h);
    assign_side_slots(anchor_box, &east, EntrySide::Right, &pin_rows, box_y, box_h);
    assign_side_slots(anchor_box, &north, EntrySide::Top, &pin_rows, box_y, box_h);
    assign_side_slots(
        anchor_box,
        &south,
        EntrySide::Bottom,
        &pin_rows,
        box_y,
        box_h,
    );

    // ★ Sync entry_points with the per-side slot layout: the render draws pins
    // from entry_points (side + offset), so both must agree. Geometry stays in
    // PinSlot (single source of truth); entry_points mirror it for rendering.
    // Device-layer graphs often carry empty entry_points, so synthesize them
    // for connected pins from the slots (unconnected pins stay NC / X-mark).
    let mut connected_pins: std::collections::BTreeSet<i64> = std::collections::BTreeSet::new();
    for topo in topos.iter().filter(|t| t.anchor == anchor_id) {
        if let Some(g) = topo.groups.first() {
            connected_pins.extend(g.pin_ids.iter().cloned());
        }
    }
    for ep in anchor_box.entry_points.iter_mut() {
        if let Some(slot) = anchor_box.slots.iter().find(|s| s.pin_id == ep.pin_id) {
            ep.side = slot.side;
            ep.offset = slot.offset;
        }
    }
    if anchor_box.entry_points.is_empty() {
        for &pid in &connected_pins {
            let Some(slot) = anchor_box.slots.iter().find(|s| s.pin_id == pid) else {
                continue;
            };
            let name = anchor_box
                .pins
                .iter()
                .find(|p| p.id == pid)
                .map(|p| {
                    if p.description.is_empty() {
                        p.pin_id.clone()
                    } else {
                        p.description.clone()
                    }
                })
                .unwrap_or_else(|| pid.to_string());
            anchor_box
                .entry_points
                .push(crate::vector::graph::boxdef::EntryPoint {
                    pin_id: pid,
                    pin_name: name,
                    side: slot.side,
                    offset: slot.offset,
                });
        }
    }
}

/// Assign PinSlots for the given pins on one box side. West/East pins land on
/// the row from the `RowPlan` (the offset is derived from the row — pin-offset
/// ownership inversion); North/South pins are spread along the box edge as
/// before.
fn assign_side_slots(
    b: &mut crate::vector::graph::McVecBox,
    pin_ids: &[i64],
    side: EntrySide,
    rows: &BTreeMap<i64, f64>,
    box_y: f64,
    box_h: f64,
) {
    let n = pin_ids.len();
    if n == 0 {
        return;
    }
    let connected: std::collections::HashSet<i64> =
        b.entry_points.iter().map(|ep| ep.pin_id).collect();
    for (i, &pid) in pin_ids.iter().enumerate() {
        let offset = if matches!(side, EntrySide::Left | EntrySide::Right) {
            match rows.get(&pid) {
                // Connected or NC pin → land on its assigned row.
                Some(&r) => ((r - box_y) / box_h).clamp(0.0, 1.0),
                // Every L/R pin must have a row after M2.5 Step 3 — a miss here
                // is a bug, not a fallback.
                None => {
                    debug_assert!(false, "pin {pid} on side {side:?} has no assigned row");
                    crate::vlog!(
                        "[equi-tree] anchor pin {pid} on side {side:?} missing a row — falling back to spread"
                    );
                    (i as f64 + 1.0) / (n as f64 + 1.0)
                }
            }
        } else {
            (i as f64 + 1.0) / (n as f64 + 1.0)
        };
        let name = b
            .pins
            .iter()
            .find(|p| p.id == pid)
            .map(|p| {
                if p.description.is_empty() {
                    p.pin_id.clone()
                } else {
                    p.description.clone()
                }
            })
            .unwrap_or_else(|| pid.to_string());
        b.slots.push(PinSlot {
            pin_id: pid,
            number: i as u32,
            name,
            side,
            offset,
            connected: connected.contains(&pid),
        });
    }
}

/// ★ E1: Generate PinSlots for every physical pin on a box.
/// This is the single source of truth for pin geometry — renderers read only slots.
fn assign_pin_slots(b: &mut crate::vector::graph::McVecBox, side: EntrySide) {
    let n = b.pins.len();
    if n == 0 {
        return;
    }
    b.slots.clear();
    // Build set of connected pin IDs
    let connected: std::collections::HashSet<i64> =
        b.entry_points.iter().map(|ep| ep.pin_id).collect();
    for (i, p) in b.pins.iter().enumerate() {
        let name = if p.description.is_empty() {
            p.pin_id.clone()
        } else {
            p.description.clone()
        };
        b.slots.push(PinSlot {
            pin_id: p.id,
            number: i as u32,
            name,
            side,
            offset: (i as f64 + 1.0) / (n as f64 + 1.0),
            connected: connected.contains(&p.id),
        });
    }
}

/// ★ F1: find a PinSlot by pin_id. Single source of truth for pin geometry.
pub(crate) fn slot_of(b: &crate::vector::graph::McVecBox, pin_id: i64) -> Option<&PinSlot> {
    b.slots.iter().find(|s| s.pin_id == pin_id)
}

/// ★ M7.6: which edge each pin of a NON-anchor multi-pin box sits on.
///
/// Up to M7.5 only the LAYER ANCHOR was treated as a real component:
/// `assign_anchor_slots` sizes it from its labels, spreads its pins over four
/// sides and syncs `connected` from the topology. Every OTHER multi-pin box
/// fell through to the `TapRole::Sink` branch — `w.max(80) × h.max(20)` with
/// ALL pins crammed onto one edge at `(i+1)/(n+1)` and `connected` read from
/// the (empty, on a device graph) `entry_points`. On the `speaker` layer that
/// gave `spk` an 89×20 box with four pins 18px apart, two "GND" labels printed
/// on top of each other, and every pin marked NC. A schematic has many
/// components; only the first one was being drawn.
///
/// The split is by NET KIND, which is what a connector looks like on paper:
/// signal pins face the trunk (Top), ground and unconnected pins face the rail
/// (Bottom). That also puts each ground pin's stub on the side the ground rail
/// is actually on, instead of hanging a ground glyph above the part.
///
/// Returns `(top, bottom, connected)` — pure topology, no rect is read.
fn sink_pin_sides(
    b: &crate::vector::graph::McVecBox,
    topos: &[NetTopology],
) -> (Vec<i64>, Vec<i64>, BTreeSet<i64>) {
    let mut ground: BTreeSet<i64> = BTreeSet::new();
    let mut connected: BTreeSet<i64> = BTreeSet::new();
    for t in topos {
        for g in t.groups.iter().filter(|g| g.box_id == b.id) {
            for &pid in &g.pin_ids {
                connected.insert(pid);
                if t.net_kind == NetKind::Ground {
                    ground.insert(pid);
                }
            }
        }
    }
    let mut top: Vec<i64> = Vec::new();
    let mut bottom: Vec<i64> = Vec::new();
    for p in &b.pins {
        if ground.contains(&p.id) || !connected.contains(&p.id) {
            bottom.push(p.id);
        } else {
            top.push(p.id);
        }
    }
    (top, bottom, connected)
}

/// ★ M7.6: size a Sink box so its labels fit, by the same A14 rule the anchor
/// uses: Top/Bottom slots sit at `(i+1)/(n+1)`, so the spacing is `w/(n+1)` and
/// the width must be `(n+1)*(label + LABEL_PAD)`. The box name is included
/// because the renderer draws it (and the class line) above the box.
fn sink_box_size(b: &crate::vector::graph::McVecBox, top: &[i64], bottom: &[i64]) -> (f64, f64) {
    let edge_w = |pins: &[i64]| {
        if pins.is_empty() {
            0.0
        } else {
            (pins.len() as f64 + 1.0) * (side_label_width(b, pins) + LABEL_PAD)
        }
    };
    let name_w = b.name.chars().count() as f64 * LABEL_CHAR_W + 2.0 * LABEL_PAD;
    let w = edge_w(top).max(edge_w(bottom)).max(name_w).max(MIN_BOX_W);
    (w, MIN_SINK_H.max(b.h))
}

/// ★ M7.6: PinSlots for a Sink — signal pins along the Top edge, ground and NC
/// pins along the Bottom, each edge spread at `(i+1)/(n+1)`. `connected` comes
/// from the TOPOLOGY, not from `entry_points`: device-layer graphs carry empty
/// `entry_points`, which is why every `spk` pin used to render as an NC cross.
fn assign_sink_slots(
    b: &mut crate::vector::graph::McVecBox,
    top: &[i64],
    bottom: &[i64],
    connected: &BTreeSet<i64>,
) {
    let pin_name = |b: &crate::vector::graph::McVecBox, pid: i64| {
        b.pins
            .iter()
            .find(|p| p.id == pid)
            .map(|p| {
                if p.description.is_empty() {
                    p.pin_id.clone()
                } else {
                    p.description.clone()
                }
            })
            .unwrap_or_else(|| pid.to_string())
    };
    b.slots.clear();
    for (side, pins) in [(EntrySide::Top, top), (EntrySide::Bottom, bottom)] {
        let n = pins.len();
        for (i, &pid) in pins.iter().enumerate() {
            let name = pin_name(b, pid);
            b.slots.push(PinSlot {
                pin_id: pid,
                number: i as u32,
                name,
                side,
                offset: (i as f64 + 1.0) / (n as f64 + 1.0),
                connected: connected.contains(&pid),
            });
        }
    }
    // Mirror the slots onto entry_points (the renderer draws pins from them),
    // synthesising them when the device graph left them empty.
    let placed: Vec<(i64, EntrySide, f64)> = b
        .slots
        .iter()
        .map(|s| (s.pin_id, s.side, s.offset))
        .collect();
    for ep in b.entry_points.iter_mut() {
        if let Some(&(_, side, offset)) = placed.iter().find(|(pid, _, _)| *pid == ep.pin_id) {
            ep.side = side;
            ep.offset = offset;
        }
    }
    if b.entry_points.is_empty() {
        for (pid, side, offset) in placed {
            if !connected.contains(&pid) {
                continue;
            }
            let name = pin_name(b, pid);
            b.entry_points
                .push(crate::vector::graph::boxdef::EntryPoint {
                    pin_id: pid,
                    pin_name: name,
                    side,
                    offset,
                });
        }
    }
}

// ============================================================================
// Layer 3: Geometry (topology + placed coords → segments + dots)
// ============================================================================

/// A line segment.
#[derive(Debug, Clone)]
pub struct Segment {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

/// Terminal symbol type.
#[derive(Debug, Clone, PartialEq)]
pub enum TreeSymbolKind {
    Ground,
    Power,
    NetLabel,
    /// ★ F5: Bus label — circle with text at the end of the line
    BusLabel,
    PortLabel,
}

/// A terminal symbol placed on the tree.
#[derive(Debug, Clone)]
pub struct TreeSymbol {
    pub kind: TreeSymbolKind,
    pub x: f64,
    pub y: f64,
    pub label: String,
    /// Direction from the tree's attachment node toward this symbol — the
    /// direction of the stub wire that connects it.
    pub dir: (f64, f64),
    /// ★ M3.5 (R1): which side the label text sits on, -1 = left, +1 = right.
    /// Decided by the ACTUAL attachment point (`realize` writes it via
    /// `text_side_away_from`), NOT by `dir` (which is always 0.0 after M1
    /// flipped the trunks horizontal) and NOT by the net's region (which
    /// disagrees with the attachment on the `symbol_alt_node` fallback,
    /// terminal-only nets and N/S spans).
    pub text_side: f64,
    /// ★ M5.0: the owning net's id — lets the audit tell a symbol's OWN net
    /// from FOREIGN nets (A25) and lets `push_labels_clear` (M5.2) avoid
    /// pushing a label into its own net's member boxes.
    pub net_id: i64,
    /// ★ M8.7: render the label text VERTICALLY (rotated -90 degrees, rising
    /// off the trunk). A run-end label whose horizontal text span would sit on
    /// top of an ALONG member lying on the same row (e.g. the mute name pasted
    /// over the series resistor it names) is turned vertical so its glyph
    /// clears the row instead of overlapping the part.
    pub vertical: bool,
}

/// M3.5 (R1, fixed): which side a label's text sits on. The text must point
/// AWAY from whatever the symbol hangs off, so it is decided by the ATTACHMENT
/// point, not by the net's region. `region` only says which side of the IC the
/// net lives on; it disagrees with the attachment end on three paths — the
/// `symbol_alt_node` fallback, terminal-only nets (no trunk at all), and N/S
/// nets whose span straddles the whole IC.
///
/// `body` is the extent the symbol hangs off (the trunk span `(lo, hi)`, or a
/// member box's x-range for terminal-only nets). If the attachment is left of
/// the body's centre the text points left, else right.
fn text_side_away_from(attach_x: f64, body: (f64, f64)) -> f64 {
    let mid = (body.0 + body.1) / 2.0;
    if attach_x <= mid {
        -1.0
    } else {
        1.0
    }
}

/// An equipotential tree for one net.
#[derive(Debug)]
pub struct EquiTree {
    pub net_name: String,
    pub net_kind: NetKind,
    /// All line segments
    pub segments: Vec<Segment>,
    /// Junction dots (degree >= 3)
    pub junction_dots: Vec<(f64, f64)>,
    /// Terminal symbols
    pub symbols: Vec<TreeSymbol>,
}

// ============================================================================
// M15: foreign-body trunk deflection (gutter routing)
// ============================================================================

/// How far a deflected trunk runs below/above its row's parts. On a row whose
/// on-row parts are 20px tall (the two-pin symbol box) the band from the parts'
/// edge to the hanging members' near edge is 10px; 15 splits it with margin.
pub(crate) const GUTTER_BASE: f64 = 15.0;
/// Step between successive gutter levels on one row, so two nets that both have
/// to route around the same foreign body can run parallel without coinciding.
pub(crate) const GUTTER_STEP: f64 = 10.0;
/// A jog lands this far OUTSIDE the blocked body, so it crosses a neighbouring
/// net's wire perpendicularly (a clean non-connection) instead of landing on a
/// foreign pin (which would read as a junction).
pub(crate) const JOG_OFFSET: f64 = 8.0;
/// Foreign bodies closer than this (in x) share ONE gutter run. Dipping down
/// and back up per component would zigzag the rail through every gap between
/// the series parts.
pub(crate) const GUTTER_MERGE_GAP: f64 = 140.0;

/// ★ M15: cross-net coordination for trunk deflections. When several nets on
/// the same row all need to dip under the same foreign member (a ladder: two
/// nets cross the same series part), they must not run in the same gutter at
/// the same x — two coincident wires read as one connected node. `alloc` hands
/// each blocked x-interval a gutter level that is clear of every box in the
/// x-range, clear of every row axis, and not already claimed by an overlapping
/// interval on the same row. One instance is shared across the whole layer, so
/// allocations are deterministic (same topos, same order) on both the render
/// side and the audit side.
pub(crate) struct DeflectAlloc {
    /// Every row axis in the layer (rounded) — a gutter must never crowd one.
    row_axes: BTreeSet<i64>,
    /// Row axis (rounded) → allocations `(gutter_y, x_lo, x_hi)`.
    by_row: BTreeMap<i64, Vec<(f64, f64, f64)>>,
}

impl DeflectAlloc {
    pub(crate) fn new(row_axes: BTreeSet<i64>) -> Self {
        DeflectAlloc {
            row_axes,
            by_row: BTreeMap::new(),
        }
    }

    /// Pick a gutter y for the blocked x-interval `[x_lo, x_hi]` on `axis`.
    /// Returns `f64::NAN` when no level is free — the caller then keeps the
    /// trunk on the row (a through-body wire beats a broken connection).
    fn alloc(&mut self, graph: &McVecGraph, axis: f64, x_lo: f64, x_hi: f64) -> f64 {
        let key = axis.round() as i64;
        let entries = self.by_row.entry(key).or_default();
        // Prefer the BELOW-row side for every level before stepping above the
        // row (where the designator labels sit); a hanging member typically
        // leaves room below, above is label country.
        let levels: Vec<f64> = (0..2)
            .flat_map(|pass| {
                (1..12).map(move |k| {
                    let b = GUTTER_BASE + (k - 1) as f64 * GUTTER_STEP;
                    if pass == 0 {
                        axis + b
                    } else {
                        axis - b
                    }
                })
            })
            .collect();
        for y in levels {
            // Never crowd another row's trunk (on-row parts reach ±10).
            if self.row_axes.iter().any(|&ra| (ra as f64 - y).abs() < 12.0) {
                continue;
            }
            // Clear of every box in the deflected x-range.
            let box_hit = graph.boxes.iter().any(|b| {
                b.w > 0.0
                    && b.h > 0.0
                    && x_lo < b.x + b.w
                    && b.x < x_hi
                    && b.y <= y
                    && y <= b.y + b.h
            });
            if box_hit {
                continue;
            }
            // Not already claimed by an overlapping interval at this level.
            let overlap = entries
                .iter()
                .any(|&(ey, elo, ehi)| (ey - y).abs() < 0.5 && elo < x_hi && x_lo < ehi);
            if overlap {
                continue;
            }
            entries.push((y, x_lo, x_hi));
            return y;
        }
        f64::NAN
    }
}

/// Realize every topo with a shared deflection allocator. The single entry
/// point guarantees the render phase and the audit derive identical trunk
/// geometry (A2/A7 stay in agreement).
pub(crate) fn realize_all(topo_list: &[NetTopology], graph: &McVecGraph) -> Vec<EquiTree> {
    let row_axes: BTreeSet<i64> = topo_list
        .iter()
        .map(|t| t.lane.axis.round() as i64)
        .collect();
    let mut deflect = DeflectAlloc::new(row_axes);
    let mut trees = Vec::with_capacity(topo_list.len());
    for t in topo_list {
        trees.push(realize(t, graph, &mut deflect));
    }
    trees
}

/// Compute geometry from topology + placed graph. Zero judgment.
/// ★ P5 is read-only for coordinates: trunk axis and span come exclusively from
/// `topo.lane` (axis written by P3 resolve_lanes, span re-enveloped over all
/// tap points by PR4 `envelop_lanes`). `realize` owns no layout constant — it
/// only reads the Lane and the placed PinSlots, then connects the points.
pub(crate) fn realize(
    topo: &NetTopology,
    graph: &McVecGraph,
    deflect: &mut DeflectAlloc,
) -> EquiTree {
    let mut segments: Vec<Segment> = Vec::new();
    let mut degree_map: BTreeMap<(i64, i64), u8> = BTreeMap::new();

    let anchor_group = topo.groups.first();
    let anchor_box = anchor_group.and_then(|g| graph.boxes.iter().find(|b| b.id == g.box_id));

    // ★ P3 lane — single source of truth. axis = trunk coordinate (x for W/E,
    // y for N/S); span = extent along the trunk direction.
    let lane = topo.lane;
    // M1/M2 row model: all trunks horizontal — the old `vertical_trunk` branch
    // is dead (B5), removed.
    debug_assert!(lane.horizontal, "row model: all trunks are horizontal");
    let axis = lane.axis;
    let (span_lo, span_hi) = lane.span;

    // Anchor pin points (from slots — single source of truth).
    let anchor_pins: Vec<(f64, f64)> = anchor_box
        .map(|b| {
            anchor_group
                .unwrap()
                .pin_ids
                .iter()
                .filter_map(|&pid| slot_of(b, pid).map(|s| slot_point(b, s)))
                .collect()
        })
        .unwrap_or_default();

    if anchor_pins.is_empty() {
        // Fallback: no anchor pins, return empty tree
        return EquiTree {
            net_name: topo.net_name.clone(),
            net_kind: topo.net_kind.clone(),
            segments,
            junction_dots: vec![],
            symbols: build_symbols(topo, lane, graph),
        };
    }

    // ★ M2 terminal-only: no trunk — the glyph hangs directly off the anchor's
    // first pin on a short stub (`moddcdc` 502/503/504 single-group GND nets).
    if topo.terminal_only {
        let mut symbols: Vec<TreeSymbol> = Vec::new();
        let mut junction_dots: Vec<(f64, f64)> = Vec::new();
        if let Some(&(px, py)) = anchor_pins.first() {
            // ★ M9: a terminal-only net may carry SEVERAL pins on the same box
            // (a satellite's two away-side ground pins, `spk.3`/`spk.4`); up to
            // M9 only `anchor_pins.first()` was wired, so the second pin drew a
            // slot but no wire reached the symbol. Join every pin on the shared
            // edge with a runner just OUTSIDE the box, then hang the glyph off
            // THAT runner (hanging it off a pin would throw its stub back along
            // the box border — A18).
            let mut ordered: Vec<(f64, f64)> = anchor_pins.to_vec();
            ordered.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.total_cmp(&b.0)));
            let (abx, aby, abw, abh) = anchor_box_rect(graph, topo.anchor);
            // `runner` = the point on the connecting wire the glyph hangs from.
            let mut runner: Option<(f64, f64)> = None;
            if ordered.len() >= 2 {
                // Outward points away from the box CENTRE, so any edge works and
                // the runner never sits on the border (A18).
                let colinear_x = (ordered[0].0 - ordered[ordered.len() - 1].0).abs() < 0.5;
                if colinear_x {
                    let outward = if ordered[0].0 <= abx + abw / 2.0 {
                        -1.0
                    } else {
                        1.0
                    };
                    let rx = ordered[0].0 + outward * (MEMBER_GAP / 2.0);
                    let lo = ordered.iter().map(|p| p.1).fold(f64::MAX, f64::min);
                    let hi = ordered.iter().map(|p| p.1).fold(f64::MIN, f64::max);
                    add_segment(
                        &Segment {
                            x1: rx,
                            y1: lo,
                            x2: rx,
                            y2: hi,
                        },
                        &mut segments,
                        &mut degree_map,
                    );
                    for &(q2x, q2y) in &ordered {
                        add_segment(
                            &Segment {
                                x1: q2x,
                                y1: q2y,
                                x2: rx,
                                y2: q2y,
                            },
                            &mut segments,
                            &mut degree_map,
                        );
                    }
                    runner = Some((rx, (lo + hi) / 2.0));
                } else {
                    let outward = if ordered[0].1 <= aby + abh / 2.0 {
                        -1.0
                    } else {
                        1.0
                    };
                    let ry = ordered[0].1 + outward * (MEMBER_GAP / 2.0);
                    let lo = ordered.iter().map(|p| p.0).fold(f64::MAX, f64::min);
                    let hi = ordered.iter().map(|p| p.0).fold(f64::MIN, f64::max);
                    add_segment(
                        &Segment {
                            x1: lo,
                            y1: ry,
                            x2: hi,
                            y2: ry,
                        },
                        &mut segments,
                        &mut degree_map,
                    );
                    for &(q2x, q2y) in &ordered {
                        add_segment(
                            &Segment {
                                x1: q2x,
                                y1: q2y,
                                x2: q2x,
                                y2: ry,
                            },
                            &mut segments,
                            &mut degree_map,
                        );
                    }
                    runner = Some(((lo + hi) / 2.0, ry));
                }
            }
            for term in &topo.terminals {
                // Hang the glyph off the RUNNER when there is one; otherwise off
                // the single pin as before. `pick_stub_dir` finds a free spot.
                let (ax0, ay0) = runner.unwrap_or((px, py));
                if runner.is_some() && junction_dots.is_empty() {
                    // A T in the runner (two stubs + the hook) is a 3-way
                    // junction — A8 wants a dot on it.
                    junction_dots.push((ax0, ay0));
                }
                // ★ M11.4: an ADOPTED ground (M10.3) is the OUTER END of a
                // row. Its glyph has to keep going the way the wire was going,
                // or "a row has a start and an end" stops reading as a wire.
                // Ask for that direction FIRST and let it walk outward when the
                // first try is blocked; only then fall back to the generic
                // four-direction search. See [`terminal_stub`].
                let outward = match topo.lane.region {
                    r @ (Region::West | Region::East) => Some(r.outward()),
                    _ => None,
                };
                let ((ax, ay), dir, lead) = terminal_stub(graph, &segments, (ax0, ay0), outward);
                if lead {
                    add_segment(
                        &Segment {
                            x1: ax0,
                            y1: ay0,
                            x2: ax,
                            y2: ay,
                        },
                        &mut segments,
                        &mut degree_map,
                    );
                }
                let (sx, sy) = (ax + dir.0 * SYMBOL_DROP, ay + dir.1 * SYMBOL_DROP);
                let kind = match term {
                    Terminal::Ground => TreeSymbolKind::Ground,
                    Terminal::NetLabel(name) => {
                        let is_bus =
                            topo.is_power_rail || name.contains("BUS") || name.contains("_VBUS");
                        if is_bus {
                            TreeSymbolKind::BusLabel
                        } else {
                            TreeSymbolKind::NetLabel
                        }
                    }
                    Terminal::Port { .. } => TreeSymbolKind::PortLabel,
                };
                let label = match term {
                    Terminal::Ground => String::new(),
                    Terminal::NetLabel(n) => n.clone(),
                    Terminal::Port { name } => name.clone(),
                };
                symbols.push(TreeSymbol {
                    kind,
                    x: sx,
                    y: sy,
                    label,
                    dir,
                    text_side: text_side_away_from(sx, (abx, abx + abw)),
                    net_id: topo.nid,
                    vertical: false,
                });
                add_segment(
                    &Segment {
                        x1: ax,
                        y1: ay,
                        x2: sx,
                        y2: sy,
                    },
                    &mut segments,
                    &mut degree_map,
                );
            }
        }
        return EquiTree {
            net_name: topo.net_name.clone(),
            net_kind: topo.net_kind.clone(),
            segments,
            junction_dots,
            symbols,
        };
    }

    // Trunk: one horizontal line along the row at `axis`.
    //
    // ★ M8: split it so it never runs THROUGH an ALONG (Series) member's body. An
    // Along part is the wire-continuation itself: its two nets' trunks meet at its
    // two PINS and the component body sits in the middle. When a run net has far
    // members on the far side (dcdc `VCC_1V2` pulled across the inductor `_L1` by
    // its `_R2` divider anchor), the enveloped span runs straight through the
    // glyph; carving the member's x-interval out of the trunk keeps every member
    // tap on a piece (dropping only the zero-length gap at the member) while the
    // connecting member itself bridges the gap electrically.
    let mut trunk_pieces: Vec<Segment> = vec![Segment {
        x1: span_lo,
        y1: axis,
        x2: span_hi,
        y2: axis,
    }];
    for group in topo.groups.iter().skip(1) {
        let Some(b) = graph.boxes.iter().find(|b| b.id == group.box_id) else {
            continue;
        };
        // A horizontal two-pin body (slots on Left/Right) is an Along part lying
        // ON this row; only those carve the trunk.
        let horizontal = b
            .slots
            .iter()
            .any(|s| matches!(s.side, EntrySide::Left | EntrySide::Right));
        crate::vlog!(
            "[CARVE] net '{}' member '{}' id={} hor={} w={} h={} y={} slots={:?}",
            topo.net_name,
            b.name,
            b.id,
            horizontal,
            b.w,
            b.h,
            b.y,
            b.slots
                .iter()
                .map(|s| format!("{:?}", s.side))
                .collect::<Vec<_>>()
        );
        if !horizontal || b.w <= 0.0 || b.h <= 0.0 {
            continue;
        }
        let (ylo, yhi) = (b.y, b.y + b.h);
        if axis + 0.5 <= ylo || axis - 0.5 >= yhi {
            continue;
        }
        let (bx, bw) = (b.x, b.w);
        let mut next: Vec<Segment> = Vec::new();
        for seg in trunk_pieces.drain(..) {
            let (lo, hi) = (seg.x1.min(seg.x2), seg.x1.max(seg.x2));
            if hi <= bx + 0.5 || lo >= bx + bw - 0.5 {
                next.push(seg);
                continue;
            }
            if lo < bx - 0.5 {
                next.push(Segment {
                    x1: seg.x1,
                    y1: axis,
                    x2: bx,
                    y2: axis,
                });
            }
            if hi > bx + bw + 0.5 {
                next.push(Segment {
                    x1: bx + bw,
                    y1: axis,
                    x2: seg.x2,
                    y2: axis,
                });
            }
        }
        trunk_pieces = next;
    }

    // ★ M15: deflect trunk pieces that would run through a FOREIGN member's
    // body (M8 only carves the net's OWN series members, so a ladder net whose
    // trunk spans several foreign parts — the mic `VMIC.VCC` rail crossing
    // `_R3`/`_C2`/`_R4` — drew its wire straight through their glyphs). Each
    // blocked x-interval is routed through a gutter between this row's parts
    // and the hanging members, with vertical jogs at the ends. The jogs land
    // `JOG_OFFSET` outside the blocked body so they cross a neighbour net's
    // trunk perpendicularly (a clean non-connection) instead of landing on the
    // foreign pin (which would read as a junction).
    //
    // The ANCHOR (group 0) is deliberately left out of `own_ids`: M8 skips the
    // anchor's carve (`topo.groups.iter().skip(1)`), so a horizontal on-row
    // anchor body — the mic `VMIC.VCC` trunk rooted at series `_R2` — would
    // otherwise be neither carved nor deflectable and the rail runs straight
    // through its glyph. Every non-anchor own body was already carved by M8,
    // so excluding those (and only those) keeps each body handled exactly once.
    let own_ids: std::collections::HashSet<i64> =
        topo.groups.iter().skip(1).map(|g| g.box_id).collect();
    let foreign: Vec<(f64, f64)> = graph
        .boxes
        .iter()
        .filter(|b| !own_ids.contains(&b.id) && b.w > 0.0 && b.h > 0.0)
        // A horizontal two-pin body lying ON this row is the only shape a trunk
        // can pass through; hanging members clear the axis by `LEAD`.
        .filter(|b| {
            b.slots
                .iter()
                .any(|s| matches!(s.side, EntrySide::Left | EntrySide::Right))
                && axis + 0.5 <= b.y + b.h
                && axis - 0.5 >= b.y
        })
        .map(|b| (b.x, b.x + b.w))
        .collect();
    // `(x_lo, x_hi, gutter_y)` per deflected run — teeth and anchor pins draw
    // to the deflected level when their x falls inside one.
    let mut deflect_regions: Vec<(f64, f64, f64)> = Vec::new();
    let mut deflected: Vec<Segment> = Vec::new();
    for seg in trunk_pieces {
        let (lo, hi) = (seg.x1.min(seg.x2), seg.x1.max(seg.x2));
        let mut blocks: Vec<(f64, f64)> = foreign
            .iter()
            .filter(|&&(fx0, fx1)| fx1 > lo + 0.5 && fx0 < hi - 0.5)
            .copied()
            .collect();
        if blocks.is_empty() {
            deflected.push(seg);
            continue;
        }
        blocks.sort_by(|a, b| a.0.total_cmp(&b.0));
        // Merge bodies whose gaps are small enough that dipping per component
        // would zigzag the rail — one long gutter run reads cleaner.
        let mut merged: Vec<(f64, f64)> = Vec::new();
        for (fx0, fx1) in blocks {
            match merged.last_mut() {
                Some((_, prev_hi)) if *prev_hi + GUTTER_MERGE_GAP >= fx0 => {
                    *prev_hi = prev_hi.max(fx1)
                }
                _ => merged.push((fx0, fx1)),
            }
        }
        let mut cursor = lo;
        for (bxl, bxh) in &merged {
            let jl = (*bxl - JOG_OFFSET).max(cursor);
            let jh = (*bxh + JOG_OFFSET).min(hi);
            if jh - jl < 2.0 {
                // Degenerate: no room to route around — keep the axis run.
                cursor = jh;
                continue;
            }
            if jl - cursor > 0.5 {
                deflected.push(Segment {
                    x1: cursor,
                    y1: axis,
                    x2: jl,
                    y2: axis,
                });
            }
            let gutter = deflect.alloc(graph, axis, jl, jh);
            if gutter.is_nan() {
                // No free gutter on this row — keep the through-body run rather
                // than break the connection.
                deflected.push(Segment {
                    x1: jl,
                    y1: axis,
                    x2: jh,
                    y2: axis,
                });
            } else {
                deflected.push(Segment {
                    x1: jl,
                    y1: axis,
                    x2: jl,
                    y2: gutter,
                });
                deflected.push(Segment {
                    x1: jl,
                    y1: gutter,
                    x2: jh,
                    y2: gutter,
                });
                deflected.push(Segment {
                    x1: jh,
                    y1: gutter,
                    x2: jh,
                    y2: axis,
                });
                deflect_regions.push((jl, jh, gutter));
            }
            cursor = jh;
        }
        if hi - cursor > 0.5 {
            deflected.push(Segment {
                x1: cursor,
                y1: axis,
                x2: hi,
                y2: axis,
            });
        }
    }
    trunk_pieces = deflected;
    crate::vlog!(
        "[SEG] net '{}' axis={} span=({}, {}) pieces={:?} deflect={:?} groups={}",
        topo.net_name,
        axis,
        span_lo,
        span_hi,
        trunk_pieces
            .iter()
            .filter(|s| (s.y1 - s.y2).abs() < 0.5)
            .map(|s| format!("{:.0}->{:.0}", s.x1.min(s.x2), s.x1.max(s.x2)))
            .collect::<Vec<_>>(),
        deflect_regions
            .iter()
            .map(|&(xl, xh, gy)| format!("{:.0}->{:.0}@{:.0}", xl, xh, gy))
            .collect::<Vec<_>>(),
        topo.groups.len()
    );
    for seg in trunk_pieces {
        if (seg.x1 - seg.x2).abs() > 0.5 || (seg.y1 - seg.y2).abs() > 0.5 {
            add_segment(&seg, &mut segments, &mut degree_map);
        }
    }

    // ★ M15: the y a vertical tooth/member-tap connects to at x — the trunk is
    // deflected into the gutter inside a region, back on the row everywhere else.
    let trunk_y = |x: f64| -> f64 {
        for &(xl, xh, gy) in &deflect_regions {
            if xl - 0.5 <= x && x <= xh + 0.5 {
                return gy;
            }
        }
        axis
    };

    // Teeth: from each anchor pin to the trunk (vertical). M3.5 (R3): the tooth
    // x is offset OUTWARD from the box edge (TOOTH_GAP) with a short horizontal
    // lead from the pin, so a West/East pin's tooth no longer runs along the
    // box border. N/S pins have outward_x == 0 and keep the plain vertical.
    let outward_x = topo.lane.region.outward().0;
    for &(px, py) in &anchor_pins {
        // M3.5: a pin that already sits on the row needs no tooth — the trunk
        // end reaches it. Drawing one would duplicate the trunk (a horizontal
        // lead collinear with it) and add a zero-length vertical that pollutes
        // `degree_map` (and can spawn a spurious junction dot).
        if (py - axis).abs() < 0.5 {
            continue;
        }
        let tx = px + outward_x * TOOTH_GAP;
        if (tx - px).abs() > 0.5 {
            add_segment(
                &Segment {
                    x1: px,
                    y1: py,
                    x2: tx,
                    y2: py,
                },
                &mut segments,
                &mut degree_map,
            );
        }
        let seg = Segment {
            x1: tx,
            y1: py,
            x2: tx,
            y2: trunk_y(px),
        };
        add_segment(&seg, &mut segments, &mut degree_map);
    }

    // Member taps: from each member pin to the trunk (vertical). M15: a hanging
    // member whose tap x falls inside a deflected run connects to the gutter
    // level, not to the row.
    for group in topo.groups.iter().skip(1) {
        let Some(member_box) = graph.boxes.iter().find(|b| b.id == group.box_id) else {
            continue;
        };
        let (mx, my) = member_pin_point(member_box, group);
        let seg = Segment {
            x1: mx,
            y1: my,
            x2: mx,
            y2: trunk_y(mx),
        };
        add_segment(&seg, &mut segments, &mut degree_map);
    }

    // ★ F4: Junction dot fix — count internal points too.
    // add_segment only counts segment ENDPOINTS. For a comb-shaped tree, tooth
    // endpoints that land on the trunk interior get degree=1 (only the tooth's
    // endpoint counted). Fix: for each segment, check if any other segment's
    // endpoint lies on its interior, and add the passing-through segment's TWO
    // directions (a tee: the trunk continues both ways + the tooth = 3 wires →
    // a junction dot). M3.5: the increment was +1, which gave an interior tooth
    // junction degree 2 (no dot) — masked pre-M3.5 by zero-length teeth
    // (LEAD=0) that counted twice at the trunk endpoints.
    for (i, si) in segments.iter().enumerate() {
        for (j, sj) in segments.iter().enumerate() {
            if i == j {
                continue;
            }
            for &(ex, ey) in &[(sj.x1, sj.y1), (sj.x2, sj.y2)] {
                let ix = ex.round() as i64;
                let iy = ey.round() as i64;
                let sx1 = si.x1.round() as i64;
                let sy1 = si.y1.round() as i64;
                let sx2 = si.x2.round() as i64;
                let sy2 = si.y2.round() as i64;
                // Skip if this point is already an endpoint of si
                if (ix == sx1 && iy == sy1) || (ix == sx2 && iy == sy2) {
                    continue;
                }
                // Check if (ex, ey) lies on segment si
                if point_on_segment(ex, ey, si) {
                    *degree_map.entry((ix, iy)).or_default() += 2;
                }
            }
        }
    }

    // Symbols — read from the lane (P3), never recompute junction/trunk x.
    let mut symbols = build_symbols(topo, lane, graph);

    // ★ Terminal-symbol connection: the Ground / NetLabel / BusLabel symbol
    // hangs off the trunk's far end on its own short wire. Instead of forcing a
    // direction (up for labels, or blindly extending the trunk end for GND),
    // find the node where the symbol attaches (the trunk's far end) and pick
    // the FIRST free direction (down → up → left → right): free means the stub
    // wire neither passes through a component box nor overlaps an existing
    // segment of this net. This keeps the GND/bus connection clean and never
    // on top of another line.
    // M1 row model: every trunk is horizontal; the symbol hangs from the trunk's
    // OUTER end (away from the anchor), so the stub never runs along the IC edge.
    let (node_x, node_y) = symbol_node(topo.lane.region, axis, lane.span);
    for sym in symbols.iter_mut() {
        if !matches!(
            sym.kind,
            TreeSymbolKind::Ground | TreeSymbolKind::NetLabel | TreeSymbolKind::BusLabel
        ) {
            continue;
        }
        // ★ M7.2: a label hangs off the trunk's OUTER end. Ending the trunk
        // exactly on the outermost member's tap leaves NO free direction there
        // — `segment_hits_box` counts grazing along a box edge as a hit, so
        // every stub off that point is rejected and the old code fell straight
        // through to `symbol_alt_node`: the INNER end, hard against the layer
        // anchor, where the label renders on top of the IC (`usbsock`
        // `USB_VBUS`). Walk OUTWARD in `SYMBOL_LANE` steps first, extending the
        // trunk with a short lead, and only consider the inner end after that.
        //
        // This is the "the chain is not expanded far enough" fix. It lives here
        // rather than in `envelop_lanes` on purpose: a step is taken only when
        // it actually buys a free direction, so the trunk never grows an end
        // with nothing attached to it (A3). Only W/E trunks step — N/S rails
        // have `outward().0 == 0` and would spin on the same point.
        let outward = topo.lane.region.outward().0;
        // ★ M12.1: a ground COLUMN is the outer END of the row it was adopted
        // onto, and its trunk is degenerate (every cold pin shares the node's
        // x). `pick_stub_dir`'s down-first order would send the glyph back UP
        // over the row; it has to continue OUTWARD instead, which is the same
        // rule M11.4 gives an adopted terminal-only ground.
        let prefer_outward =
            topo.ground_column && matches!(topo.lane.region, Region::West | Region::East);
        let mut walked: Option<((f64, f64), (f64, f64))> = None;
        let steps = match topo.lane.region {
            Region::West | Region::East => 3,
            _ => 1,
        };
        for step in 0..steps {
            let ax = node_x + outward * step as f64 * SYMBOL_LANE;
            // The lead itself must be clear of every component box.
            if step > 0
                && graph.boxes.iter().any(|b| {
                    !matches!(
                        b.kind,
                        BoxKind::PowerLabel | BoxKind::Dot | BoxKind::PortTerminal
                    ) && segment_hits_box(node_x, node_y, ax, node_y, b.x, b.y, b.w, b.h)
                })
            {
                break;
            }
            if prefer_outward && stub_dir_is_free(graph, &segments, (ax, node_y), (outward, 0.0)) {
                walked = Some(((ax, node_y), (outward, 0.0)));
                break;
            }
            if let Some(dir) = pick_stub_dir(graph, &segments, (ax, node_y)) {
                walked = Some(((ax, node_y), dir));
                break;
            }
        }
        // ★ M8: an ALONG (Series) part now sits ON the trunk at its outer end,
        // so once the run continues outward the outer end is crowded all the
        // way out and the inner end is hard against the IC — neither gives a
        // label a home (moddcdc `__net_3` next to the now-horizontal inductor).
        // A label legitimately hangs off any point of its OWN trunk, so sample
        // the interior for the first free stub direction before giving up and
        // falling back to the alt node. The attach point is on the trunk, so no
        // lead segment is needed. W/E only: N/S rails have no interior option.
        if walked.is_none() && matches!(topo.lane.region, Region::West | Region::East) {
            let (lo, hi) = (lane.span.0.min(lane.span.1), lane.span.0.max(lane.span.1));
            let mut ax = node_x - outward * SYMBOL_LANE;
            while (ax - lo).abs() > 0.5 && (ax - hi).abs() > 0.5 {
                if let Some(dir) = pick_stub_dir(graph, &segments, (ax, node_y)) {
                    walked = Some(((ax, node_y), dir));
                    break;
                }
                ax -= outward * SYMBOL_LANE;
            }
        }
        // Try the outer end first; if every direction there is crowded (a member
        // box hugs the trunk end), fall back to the inner end so the symbol
        // still finds a clean spot instead of drawing through the crowd.
        let (attach, dir) = match walked {
            Some((a, d)) => (a, Some(d)),
            None => {
                let (alt_x, alt_y) = symbol_alt_node(topo.lane.region, axis, lane.span);
                match pick_stub_dir(graph, &segments, (alt_x, alt_y)) {
                    Some(dir) => ((alt_x, alt_y), Some(dir)),
                    // Truly no free spot on either end (a degenerate trunk with
                    // a member box right on the node). Sit the symbol at the
                    // node WITHOUT a wire, so it never draws a line through
                    // another component.
                    None => ((node_x, node_y), None),
                }
            }
        };
        // The lead that carries the trunk out to a walked attach point.
        if (attach.0 - node_x).abs() > 0.5 && (attach.1 - node_y).abs() < 0.5 && walked.is_some() {
            add_segment(
                &Segment {
                    x1: node_x,
                    y1: node_y,
                    x2: attach.0,
                    y2: node_y,
                },
                &mut segments,
                &mut degree_map,
            );
        }
        // ★ M3.5 (R1, fixed): the text points away from the trunk body, using
        // the end the symbol ACTUALLY attached to (outer or alt), not the
        // region — `symbol_alt_node` fallback and N/S spans disagree with the
        // region-based guess.
        //
        // ★ M7.2: "away from the trunk body" is not enough on the alt-node
        // path. The alt node is the INNER end, i.e. right against the layer
        // anchor, and it is by construction on the anchor side of the span
        // midpoint — so `text_side_away_from(attach, span)` aimed the text
        // straight INTO the IC (`usbsock` `USB_VBUS`). Whenever the symbol sits
        // beside the anchor box, the anchor decides: a symbol left of the IC
        // always writes left, one right of it always writes right. The span
        // rule still governs terminal-only nets and N/S trunks that straddle
        // the anchor.
        sym.text_side = match anchor_box {
            Some(ab) if attach.0 <= ab.x => -1.0,
            Some(ab) if attach.0 >= ab.x + ab.w => 1.0,
            _ => text_side_away_from(attach.0, lane.span),
        };
        // ★ M8.7: a run-end label whose HORIZONTAL text span would sit on top
        // of an ALONG member lying on this same row (e.g. the mute name pasted
        // over the series resistor it names) is turned VERTICAL so its glyph
        // rises off the trunk instead of overlapping the part. Only W/E trunks
        // (horizontal) can host an Along member on the row — N/S rails cannot.
        let is_label = matches!(
            sym.kind,
            TreeSymbolKind::NetLabel | TreeSymbolKind::BusLabel | TreeSymbolKind::PortLabel
        );
        let text_collides = if is_label && !sym.label.is_empty() {
            let label_w = sym.label.len() as f64 * 7.0;
            let (ls0, ls1) = if sym.text_side < 0.0 {
                (attach.0 - 4.0 - label_w, attach.0 - 4.0)
            } else {
                (attach.0 + 4.0, attach.0 + 4.0 + label_w)
            };
            const BAND: f64 = 20.0; // vertical band around the trunk row
            graph.boxes.iter().any(|b| {
                if b.w <= 0.0
                    || b.h <= 0.0
                    || matches!(
                        b.kind,
                        BoxKind::PowerLabel | BoxKind::Dot | BoxKind::PortTerminal
                    )
                {
                    return false;
                }
                ls0 < b.x + b.w && ls1 > b.x && axis - BAND < b.y + b.h && axis + BAND > b.y
            })
        } else {
            false
        };
        // ★ M11.3: two triggers now. `text_collides` is the geometric one M8.7
        // added; `outer_end_taken` is the TOPOLOGICAL one — the run continues
        // through a part, or the row ends at a satellite, so the outer end was
        // never the label's to take (A31). A union, so nothing that reads
        // correctly today changes; it only adds cases that used to depend on
        // how long a name happened to be.
        let vertical_now = (text_collides || topo.outer_end_taken)
            && matches!(topo.lane.region, Region::West | Region::East);
        if vertical_now {
            // ★ M10.1: a label that cannot lie along the row still has to be
            // PULLED OFF it on a real wire — that stub is the drawing convention
            // that says "this wire is named" / "this is a bus". M8.7 parked the
            // glyph 4px off the trunk, so the bus circle merged with the junction
            // dot underneath it and `speaker`'s `US_SPEAKER_MUTE` read as a name
            // painted onto the wire with no stub at all. Use the full
            // SYMBOL_DROP, preferring UP — a row's members hang DOWN.
            let vdir = if stub_dir_is_free(graph, &segments, attach, (0.0, -1.0)) {
                (0.0, -1.0)
            } else if stub_dir_is_free(graph, &segments, attach, (0.0, 1.0)) {
                (0.0, 1.0)
            } else {
                (0.0, -1.0)
            };
            sym.vertical = true;
            sym.dir = vdir;
            sym.x = attach.0 + vdir.0 * SYMBOL_DROP;
            sym.y = attach.1 + vdir.1 * SYMBOL_DROP;
            add_segment(
                &Segment {
                    x1: attach.0,
                    y1: attach.1,
                    x2: sym.x,
                    y2: sym.y,
                },
                &mut segments,
                &mut degree_map,
            );
        } else if let Some(dir) = dir {
            sym.dir = dir;
            sym.x = attach.0 + dir.0 * SYMBOL_DROP;
            sym.y = attach.1 + dir.1 * SYMBOL_DROP;
            add_segment(
                &Segment {
                    x1: attach.0,
                    y1: attach.1,
                    x2: sym.x,
                    y2: sym.y,
                },
                &mut segments,
                &mut degree_map,
            );
        } else {
            sym.dir = (0.0, 1.0);
            sym.x = attach.0;
            sym.y = attach.1;
        }
    }

    // ★ M12.1: a ground COLUMN's arms meet its glyph at the node on the adopted
    // row. The column spine (a member tooth from another row) plus the glyph
    // stub plus the adopting arm's cold pin — which sits AT the node, so its
    // tooth is zero-length and never reaches degree 3 — make a genuine 3-way
    // junction, so the drawing must show the dot (A8).
    let column_dots: Vec<(f64, f64)> = if topo.ground_column {
        symbols
            .iter()
            .filter(|s| matches!(s.kind, TreeSymbolKind::Ground))
            .map(|s| (s.x - s.dir.0 * SYMBOL_DROP, s.y - s.dir.1 * SYMBOL_DROP))
            .collect()
    } else {
        Vec::new()
    };

    // Junction dots are computed AFTER the terminal-symbol stubs are added
    // (moved at M3): a ground / label stub attaches at the trunk's outer end,
    // and when an anchor tooth also lands there (e.g. `ldo` GND 303) that is a
    // genuine 3-way junction that MUST carry a dot. Computing the dots first
    // (M2.5) silently dropped the ground stub's junction — masked on `ldo` by a
    // degenerate zero-length tooth that counted twice. A stub on a clean trunk
    // end still yields degree 2, so no spurious dot appears.
    let junction_dots: Vec<(f64, f64)> = degree_map
        .iter()
        .filter(|(_, &deg)| deg >= 3)
        .map(|(&(x, y), _)| (x as f64, y as f64))
        .chain(column_dots)
        .collect();

    EquiTree {
        net_name: topo.net_name.clone(),
        net_kind: topo.net_kind.clone(),
        segments,
        junction_dots,
        symbols,
    }
}

/// Pick the first free direction (down, up, left, right) for a terminal-symbol
/// stub from the trunk's far end. A direction is free when the stub wire
/// neither passes through a component box nor overlaps an existing segment of
/// this net beyond the attachment node (a collinear continuation of the trunk,
/// e.g. hanging down from a vertical trunk, is not an overlap). `None` means
/// every direction is blocked (e.g. a member box crowds the node).
fn pick_stub_dir(graph: &McVecGraph, segments: &[Segment], node: (f64, f64)) -> Option<(f64, f64)> {
    for (dx, dy) in [(0.0, 1.0), (0.0, -1.0), (-1.0, 0.0), (1.0, 0.0)] {
        if stub_dir_is_free(graph, segments, node, (dx, dy)) {
            return Some((dx, dy));
        }
    }
    None
}

/// ★ M10.1: is a `SYMBOL_DROP`-long stub out of `node` in direction `dir` clear
/// of every component box and of every segment already drawn for this net?
///
/// Extracted from [`pick_stub_dir`] so a caller that needs a SPECIFIC direction
/// (the vertical label stub, the outward ground stub) can ask about that one
/// direction instead of re-deriving the geometry or taking whatever
/// `pick_stub_dir` happened to prefer.
pub(crate) fn stub_dir_is_free(
    graph: &McVecGraph,
    segments: &[Segment],
    node: (f64, f64),
    dir: (f64, f64),
) -> bool {
    let ex = node.0 + dir.0 * SYMBOL_DROP;
    let ey = node.1 + dir.1 * SYMBOL_DROP;
    let hits_box = graph.boxes.iter().any(|b| {
        if matches!(
            b.kind,
            BoxKind::PowerLabel | BoxKind::Dot | BoxKind::PortTerminal
        ) {
            return false;
        }
        segment_hits_box(node.0, node.1, ex, ey, b.x, b.y, b.w, b.h)
    });
    if hits_box {
        return false;
    }
    !segments
        .iter()
        .any(|s| segments_overlap(node.0, node.1, ex, ey, s.x1, s.y1, s.x2, s.y2))
}

/// ★ M11.4: where a TERMINAL-ONLY net's glyph hangs, and whether the wire had to
/// be extended to get there.
///
/// Three tiers, each a fallback for the one before it:
///
/// 1. **outward along the row** — the M10.3 case. An adopted ground IS the row's
///    outer end, so its glyph continues in the direction the wire was already
///    travelling. If that direction is blocked at the attach point, walk out in
///    `SYMBOL_LANE` steps (max 3) and retry, extending the wire with a short
///    lead — the same escape M7.2 gave trunk-end labels when `segment_hits_box`
///    ("grazing an edge counts as a hit") left no free direction at the node
///    itself. The lead is a SEPARATE segment, so A15, which measures the
///    segment that ends ON the glyph, still sees a `SYMBOL_DROP` stub.
/// 2. **the generic four-direction search** (`pick_stub_dir`: down → up → left →
///    right). Where a cap whose outer end butts against a satellite ends up —
///    the glyph drops off the far pin instead. Uglier, still connected.
/// 3. **outward anyway**, with no free direction at all (a degenerate layer).
///    Prefer the row's own direction over the old hard-coded `(0,1)`: dropping a
///    horizontal row's terminal DOWN lands it in the next row's corridor, which
///    is the one place it must never go.
///
/// Returns `(attach, dir, needs_lead)`.
fn terminal_stub(
    graph: &McVecGraph,
    segments: &[Segment],
    node: (f64, f64),
    outward: Option<(f64, f64)>,
) -> ((f64, f64), (f64, f64), bool) {
    if let Some(d) = outward {
        for step in 0..3 {
            let ax = node.0 + d.0 * step as f64 * SYMBOL_LANE;
            let ay = node.1 + d.1 * step as f64 * SYMBOL_LANE;
            // The lead itself must be clear of every component box.
            if step > 0
                && graph.boxes.iter().any(|b| {
                    !matches!(
                        b.kind,
                        BoxKind::PowerLabel | BoxKind::Dot | BoxKind::PortTerminal
                    ) && segment_hits_box(node.0, node.1, ax, ay, b.x, b.y, b.w, b.h)
                })
            {
                break;
            }
            if stub_dir_is_free(graph, segments, (ax, ay), d) {
                return ((ax, ay), d, step > 0);
            }
        }
    }
    if let Some(d) = pick_stub_dir(graph, segments, node) {
        return (node, d, false);
    }
    (node, outward.unwrap_or((0.0, 1.0)), false)
}

/// Does the axis-aligned segment (ax,ay)-(bx,by) pass through the interior of
/// the box (x,y)-(x+w,y+h)? Grazing along an edge counts as a hit.
pub(crate) fn segment_hits_box(
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> bool {
    let eps = 0.5;
    if (ax - bx).abs() < 0.01 {
        // vertical segment
        if !(x - eps <= ax && ax <= x + w + eps) {
            return false;
        }
        let lo = ay.min(by);
        let hi = ay.max(by);
        return lo.max(y - eps) + eps < hi.min(y + h + eps);
    }
    if (ay - by).abs() < 0.01 {
        // horizontal segment
        if !(y - eps <= ay && ay <= y + h + eps) {
            return false;
        }
        let lo = ax.min(bx);
        let hi = ax.max(bx);
        return lo.max(x - eps) + eps < hi.min(x + w + eps);
    }
    false
}

/// Do two axis-aligned segments share more than a single endpoint — either a
/// parallel overlap over a length, or a proper interior crossing?
fn segments_overlap(
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
    cx: f64,
    cy: f64,
    dx: f64,
    dy: f64,
) -> bool {
    let eps = 0.5;
    let va = (ax - bx).abs() < eps; // seg1 vertical
    let vb = (cx - dx).abs() < eps; // seg2 vertical
    let ha = (ay - by).abs() < eps; // seg1 horizontal
    let hb = (cy - dy).abs() < eps; // seg2 horizontal
    if va && vb {
        if (ax - cx).abs() > eps {
            return false;
        }
        return ay.min(by).max(cy.min(dy)) + eps < ay.max(by).min(cy.max(dy));
    }
    if ha && hb {
        if (ay - cy).abs() > eps {
            return false;
        }
        return ax.min(bx).max(cx.min(dx)) + eps < ax.max(bx).min(cx.max(dx));
    }
    if va && hb {
        // seg1 vertical, seg2 horizontal — crossing at (ax, cy)
        if !(cx.min(dx) - eps <= ax && ax <= cx.max(dx) + eps) {
            return false;
        }
        if !(ay.min(by) - eps <= cy && cy <= ay.max(by) + eps) {
            return false;
        }
        let on_a = (cy - ay).abs() > eps && (cy - by).abs() > eps;
        let on_b = (ax - cx).abs() > eps && (ax - dx).abs() > eps;
        return on_a && on_b;
    }
    if ha && vb {
        // seg1 horizontal, seg2 vertical — crossing at (cx, ay)
        if !(cx.min(dx) - eps <= ax && ax <= cx.max(dx) + eps) {
            return false;
        }
        if !(ay.min(by) - eps <= cy && cy <= ay.max(by) + eps) {
            return false;
        }
        let on_a = (cx - ax).abs() > eps && (cx - bx).abs() > eps;
        let on_b = (ay - cy).abs() > eps && (ay - dy).abs() > eps;
        return on_a && on_b;
    }
    false
}

/// Absolute position of a member group's first pin (from PinSlots).
pub(crate) fn member_pin_point(
    member_box: &crate::vector::graph::McVecBox,
    group: &PinGroup,
) -> (f64, f64) {
    if let Some(&pid) = group.pin_ids.first() {
        if let Some(slot) = slot_of(member_box, pid) {
            return slot_point(member_box, slot);
        }
    }
    (member_box.x, member_box.y + member_box.h / 2.0)
}

fn add_segment(
    seg: &Segment,
    segments: &mut Vec<Segment>,
    degree_map: &mut BTreeMap<(i64, i64), u8>,
) {
    let x1 = seg.x1.round() as i64;
    let y1 = seg.y1.round() as i64;
    let x2 = seg.x2.round() as i64;
    let y2 = seg.y2.round() as i64;

    *degree_map.entry((x1, y1)).or_default() += 1;
    *degree_map.entry((x2, y2)).or_default() += 1;

    segments.push(seg.clone());
}

/// ★ F4: check if a point lies on a segment (within rounding tolerance).
pub(crate) fn point_on_segment(px: f64, py: f64, seg: &Segment) -> bool {
    let x1 = seg.x1;
    let y1 = seg.y1;
    let x2 = seg.x2;
    let y2 = seg.y2;
    let eps = 0.5; // tolerance for f64 rounding
    if (x1 - x2).abs() < eps {
        // Vertical segment
        (px - x1).abs() < eps && py >= y1.min(y2) - eps && py <= y1.max(y2) + eps
    } else if (y1 - y2).abs() < eps {
        // Horizontal segment
        (py - y1).abs() < eps && px >= x1.min(x2) - eps && px <= x1.max(x2) + eps
    } else {
        false
    }
}

/// The end of a horizontal trunk the terminal symbols hang from: the **outer**
/// end, away from the anchor. For a West net that is the left end (`span.0`),
/// for all others the right end (`span.1`) — attaching at the anchor-edge end
/// would make the symbol stub run along the IC edge and cross the IC body.
fn symbol_node(region: Region, axis: f64, span: (f64, f64)) -> (f64, f64) {
    match region {
        Region::West => (span.0, axis),
        _ => (span.1, axis),
    }
}

/// The opposite end of the trunk, used as a fallback hang point when the outer
/// end is crowded.
fn symbol_alt_node(region: Region, axis: f64, span: (f64, f64)) -> (f64, f64) {
    match region {
        Region::West => (span.1, axis),
        _ => (span.0, axis),
    }
}

fn build_symbols(topo: &NetTopology, lane: Lane, graph: &McVecGraph) -> Vec<TreeSymbol> {
    let mut symbols = Vec::new();

    // M1 row model: every trunk is horizontal; the symbol hangs from the trunk's
    // OUTER end (away from the anchor), so the stub never runs along the IC edge.
    let axis = lane.axis;
    let (nx, ny) = symbol_node(lane.region, axis, lane.span);

    // ★ F4: check if there is already a label box for this net (e.g. PowerLabel),
    // to avoid drawing a duplicate label symbol. PortTerminal is excluded: it is
    // now extracted as a Terminal::NetLabel (see build_one_topology), so it must
    // not suppress the symbol it is meant to produce.
    let has_label_box = graph
        .boxes
        .iter()
        .any(|b| matches!(b.kind, BoxKind::PowerLabel | BoxKind::Dot) && b.name == topo.net_name);

    for term in &topo.terminals {
        match term {
            Terminal::Ground => {
                // ★ M7.5: seed like every other terminal — one SYMBOL_DROP off
                // the trunk's outer end. Up to M6 the glyph was pinned to the
                // shared `ground_band` (max South row + SYMBOL_DROP), which on a
                // layer with a deep free ground band produced a metres-long
                // vertical that ran off the canvas. `realize` re-picks a free
                // direction and wires the short stub.
                let (x, y) = (nx, ny + SYMBOL_DROP);
                symbols.push(TreeSymbol {
                    kind: TreeSymbolKind::Ground,
                    x,
                    y,
                    label: String::new(),
                    dir: (0.0, 1.0),
                    text_side: 1.0, // Ground has no text; value irrelevant
                    net_id: topo.nid,
                    vertical: false,
                });
            }
            Terminal::NetLabel(name) => {
                // ★ F4: skip if there is already a label box for this net
                if has_label_box {
                    continue;
                }
                // ★ F5: power rails (incl. DC-interface Power members like
                // vin.POWER_SYS) always use BusLabel (circle + text).
                // Bus names (containing "BUS" or "_VBUS") also use BusLabel.
                let is_bus = topo.is_power_rail || name.contains("BUS") || name.contains("_VBUS");
                let kind = if is_bus {
                    TreeSymbolKind::BusLabel
                } else {
                    TreeSymbolKind::NetLabel
                };
                // Seed position: off the trunk's outer end (down by default);
                // realize re-picks a free direction and wires it.
                let (x, y) = (nx, ny + SYMBOL_DROP);
                symbols.push(TreeSymbol {
                    kind,
                    x,
                    y,
                    label: name.clone(),
                    dir: (0.0, 1.0),
                    // M3.5 (R1, fixed): seed only — realize overwrites this from
                    // the ACTUAL attach point (outer end or alt end).
                    text_side: 1.0,
                    net_id: topo.nid,
                    vertical: false,
                });
            }
            Terminal::Port { name } => {
                let (x, y) = (nx, ny + SYMBOL_DROP);
                symbols.push(TreeSymbol {
                    kind: TreeSymbolKind::PortLabel,
                    x,
                    y,
                    label: name.clone(),
                    dir: (0.0, 1.0),
                    // M3.5 (R3): no more `+ 140.0` offset; realize overwrites
                    // text_side from the attach point.
                    text_side: 1.0,
                    net_id: topo.nid,
                    vertical: false,
                });
            }
        }
    }

    symbols
}

// ============================================================================
// Main entry points
// ============================================================================

/// ★ E2: Layout device layer — topology + placement.
/// Called during the layout phase (before render). Writes x/y/w/h and
/// PinSlots on boxes, sets geom_locked = true.
pub fn layout_device_layer(graph: &mut McVecGraph) {
    // ★ M6.5: ground grouping comes from the pass2 netlist (project_nets no
    // longer merges every Ground net into one global GND), so each distinct
    // ground net already renders one ground symbol — no explosion here.
    let mut topos = build_topology(graph);
    eprintln!(
        "[equi-tree] layout_device_layer: {} nets, {} topos, {} boxes",
        graph.nets.len(),
        topos.len(),
        graph.boxes.len(),
    );
    place_by_topology(graph, &mut topos);
    // Log topology regions (assigned by P1 inside place_by_topology).
    for t in &topos {
        eprintln!(
            "[equi-tree]   topo: net='{}' anchor={} groups={} region={:?}",
            t.net_name,
            t.anchor,
            t.groups.len(),
            t.lane.region,
        );
    }
    // Log placed box positions
    let mut placed_count = 0;
    let mut unplaced_count = 0;
    for b in &graph.boxes {
        if b.geom_locked {
            eprintln!(
                "[equi-tree]   placed box: '{}' id={} x={:.0} y={:.0} w={:.0} h={:.0}",
                b.name, b.id, b.x, b.y, b.w, b.h,
            );
            placed_count += 1;
        } else {
            unplaced_count += 1;
        }
    }
    // ★ Fallback: boxes not in any topology get a default position (stacked right)
    if unplaced_count > 0 {
        eprintln!(
            "[equi-tree]   {} unplaced boxes — assigning fallback positions",
            unplaced_count,
        );
        let mut fallback_x = 500.0;
        let fallback_y = 100.0;
        for b in &mut graph.boxes {
            if !b.geom_locked && b.kind != BoxKind::PowerLabel {
                b.x = fallback_x;
                b.y = fallback_y;
                b.w = 120.0;
                b.h = 60.0;
                b.geom_locked = true;
                // ★ M6.5: a box that is ONLY the anchor of a terminal-only net
                // (e.g. a lone testpoint in a per-consumer ground net) is placed
                // here but never gets slots from `assign_anchor_slots`. Without a
                // slot, `realize`'s `anchor_pins` is empty and the ground glyph
                // falls back to the degenerate lane span (x=0) instead of hanging
                // off the box — so it ignores the canvas shift and clips.
                if b.slots.is_empty() {
                    for (i, p) in b.pins.iter().enumerate() {
                        b.slots.push(PinSlot {
                            pin_id: p.id,
                            number: i as u32,
                            name: if p.description.is_empty() {
                                p.pin_id.clone()
                            } else {
                                p.description.clone()
                            },
                            side: EntrySide::Right,
                            offset: 0.5,
                            connected: true,
                        });
                    }
                }
                fallback_x += 160.0;
                eprintln!(
                    "[equi-tree]   fallback box: '{}' id={} x={:.0} y={:.0}",
                    b.name, b.id, b.x, b.y,
                );
            }
        }
    }
    eprintln!(
        "[equi-tree] layout_device_layer done: {} placed, {} fallback",
        placed_count, unplaced_count,
    );
}

/// Build equipotential trees for all nets in the graph (render phase, read-only).
/// Replays P0 (build_topology) → P1 (assign_regions) → P3 (resolve_lanes) → P5 (realize).
/// Does NOT modify the graph.
pub fn build_all_trees(graph: &McVecGraph) -> Vec<EquiTree> {
    let mut topos = build_topology(graph);
    eprintln!(
        "[equi-tree] build_all_trees: {} nets, {} topos",
        graph.nets.len(),
        topos.len(),
    );
    // ★ P1 + P0 + P3 replay: assign regions, assign rows and resolve lanes
    // identically to the layout phase, so render-side trunk coordinates always
    // match the members (and the layer-anchor pins that were placed on the
    // rows).
    assign_regions(graph, &mut topos);
    let layer_anchor = layer_anchor_id(&topos);
    let _row_plan = assign_rows(graph, &mut topos, layer_anchor);
    resolve_lanes(graph, &mut topos);
    // ★ PR4: span enveloping replay — members are already placed in the graph
    // (layout phase), so the recomputed span (anchor + member taps) matches the
    // layout phase exactly; realize then reads only this enveloped Lane.
    envelop_lanes(graph, &mut topos);
    realize_all(&topos, graph)
}

/// ★ Content-adaptive canvas (fix "circuit clipped at negative coordinates").
///
/// The SVG viewBox starts at `0 0`, so any content with a negative x/y (West
/// trunks, left-side caps, symbols above the anchor) is silently clipped by the
/// current canvas logic which only grows the max (positive) extent. Here we:
///   1. compute the bounding box of ALL rendered content (boxes + tree segments
///      + junction dots + symbols, min and max in both axes),
///   2. shift every box in X so the content starts at the canvas margin,
///   3. return the SVG viewBox `(x, y, w, h)` sized to the content
///      (`content + 2×margin`), starting at the TRUE content top so nothing —
///      including M8.7/M10.1 vertical labels that read UPWARD off their trunk —
///      hangs off the canvas edge.
///
/// The render phase calls `build_all_trees` again on the shifted graph, so the
/// re-derived trees are consistent with the shifted boxes.
pub fn fit_content_to_canvas(graph: &mut McVecGraph) -> (f64, f64, f64, f64) {
    let margin = crate::viz::layout::normalize::CANVAS_MARGIN;

    // ★ Single-pass fit. NOTE: this must be ONE shift — the renderer re-derives
    // the trees (`build_all_trees`) on the shifted graph, and trunk/axis-derived
    // symbols (labels above a row, `assign_rows`'s absolute `BASE_Y`) do NOT
    // follow a Y shift, so iterating would overshoot the vertical axis. The
    // horizontal axis IS stable once every terminal-only anchor has a slot
    // (M6.5 fallback), so a single shift is sufficient.
    let trees = build_all_trees(graph);
    let Some((min_x, min_y, max_x, max_y)) = content_bbox(graph, &trees) else {
        return (0.0, 0.0, 200.0, 100.0); // no content
    };
    let shift_x = margin - min_x;
    // ★ M7.4: never shift Y. `assign_rows` writes absolute `BASE_Y`-derived
    // rows and the render replay (`build_all_trees`) replays them on the
    // (X-shifted) graph, so any vertical box shift moves every layer-anchor pin
    // off its OWN trunk by exactly `shift_y` — the uniform offset that put
    // `moddcdc`'s EN/LX pins 50px above their rows, with the tooth running
    // through the members. Instead of moving the boxes, the viewBox below starts
    // at the true content top, so a negative `min_y` (a vertical label rising
    // above its row) is given room instead of being clipped.
    let shift_y: f64 = 0.0;
    crate::vlog!(
        "[fit] layer '{}' bbox x[{min_x:.0},{max_x:.0}] y[{min_y:.0},{max_y:.0}] shift=({shift_x:.0},{shift_y:.0})",
        graph.name
    );
    if shift_x.abs() > 0.01 || shift_y.abs() > 0.01 {
        for b in &mut graph.boxes {
            b.x += shift_x;
            b.y += shift_y;
            for lp in &mut b.label_placements {
                lp.x += shift_x;
                lp.y += shift_y;
            }
        }
    }

    // ★ M10.x: the viewBox starts at the TRUE content top (not `0`). `min_y`
    // can now be negative — M8.7/M10.1 vertical labels read UPWARD off their
    // trunk, so a long label on a high row rises above y=0. The old `max_y +
    // margin` height with a `0 0` viewBox clipped everything above y=0; instead
    // the canvas gets a symmetric margin on every side and nothing is cut off.
    let viewbox_x = 0.0; // after the X shift above, content min_x == margin
    let viewbox_y = min_y - margin;
    let viewbox_w = (max_x - min_x) + 2.0 * margin;
    let viewbox_h = (max_y - min_y) + 2.0 * margin;
    // Modest floor so tiny layers still get a usable "paper".
    (
        viewbox_x,
        viewbox_y,
        viewbox_w.max(300.0),
        viewbox_h.max(200.0),
    )
}

/// Bounding box of every rendered element: boxes, tree segments, junction dots
/// and symbols (with the symbol glyph extents — ground bars, bus circles, and
/// `text_side`-anchored label text — so a left-anchored label or a ground
/// symbol cannot hang off the canvas edge).
fn content_bbox(graph: &McVecGraph, trees: &[EquiTree]) -> Option<(f64, f64, f64, f64)> {
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;

    // Boxes (including zero-size labels; their symbols render small).
    for b in &graph.boxes {
        min_x = min_x.min(b.x);
        min_y = min_y.min(b.y);
        max_x = max_x.max(b.x + b.w);
        max_y = max_y.max(b.y + b.h);
    }
    // Tree geometry (segments / junction dots / symbols).
    for t in trees {
        for s in &t.segments {
            min_x = min_x.min(s.x1).min(s.x2);
            min_y = min_y.min(s.y1).min(s.y2);
            max_x = max_x.max(s.x1).max(s.x2);
            max_y = max_y.max(s.y1).max(s.y2);
        }
        for &(jx, jy) in &t.junction_dots {
            min_x = min_x.min(jx);
            min_y = min_y.min(jy);
            max_x = max_x.max(jx);
            max_y = max_y.max(jy);
        }
        for sym in &t.symbols {
            // rough text width estimate: ~7px per char at font-size 10.
            let label_w = sym.label.len() as f64 * 7.0;
            // Ground glyph: three horizontal bars extend ±10px around `sym.x`
            // and the vertical lead spans `y-4..y+8` — the text-only bbox below
            // would let a ground symbol hang off the left/right canvas edge.
            if matches!(sym.kind, TreeSymbolKind::Ground) {
                min_x = min_x.min(sym.x - 10.0);
                max_x = max_x.max(sym.x + 10.0);
                min_y = min_y.min(sym.y - 4.0);
                max_y = max_y.max(sym.y + 8.0);
                continue;
            }
            // ★ M8.7: a VERTICAL label is rotated -90 deg (reads upward off the
            // trunk), so its extent is a column — a vertical run of ~label-width
            // above `sym.y`, with a horizontal span of roughly one glyph height.
            if sym.vertical {
                let label_w = sym.label.len() as f64 * 7.0;
                min_x = min_x.min(sym.x - 6.0);
                max_x = max_x.max(sym.x + 6.0);
                min_y = min_y.min(sym.y - label_w - 6.0);
                max_y = max_y.max(sym.y + 6.0);
                continue;
            }
            // BusLabel: a circle of radius 6 at `(sym.x, sym.y)` plus text that
            // starts one radius + 4px outside it on the `text_side`.
            if matches!(sym.kind, TreeSymbolKind::BusLabel) {
                const R: f64 = 6.0;
                min_x = min_x.min(sym.x - R);
                max_x = max_x.max(sym.x + R);
                min_y = min_y.min(sym.y - R);
                max_y = max_y.max(sym.y + R);
                let (tx0, tx1) = if sym.text_side < 0.0 {
                    (sym.x - R - 4.0 - label_w, sym.x - R - 4.0)
                } else {
                    (sym.x + R + 4.0, sym.x + R + 4.0 + label_w)
                };
                min_x = min_x.min(tx0);
                max_x = max_x.max(tx1);
                continue;
            }
            // NetLabel / PortLabel: text is anchored by `text_side` — -1 (end)
            // extends LEFT of `sym.x - 4`, +1 (start) extends RIGHT — so the
            // bbox must branch on it or a West-side label is clipped at the
            // left canvas edge even though `sym.x` itself is inside the viewBox.
            let (lx0, lx1) = if sym.text_side < 0.0 {
                (sym.x - 4.0 - label_w, sym.x - 4.0)
            } else {
                (sym.x + 4.0, sym.x + 4.0 + label_w)
            };
            min_x = min_x.min(lx0);
            max_x = max_x.max(lx1);
            min_y = min_y.min(sym.y - 12.0);
            max_y = max_y.max(sym.y + 12.0);
        }
    }
    if min_x == f64::MAX {
        return None;
    }
    Some((min_x, min_y, max_x, max_y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::boxdef::{BoxPin, IoSummary, PortDir};
    use crate::vector::graph::kinds::{BoxKind, NetKind};
    use crate::vector::graph::netdef::{EndpointRef, IoDirection, NetRole};
    use crate::vector::graph::symbol::Symbol;
    use crate::vector::graph::{LayerStyle, McVecBox, VizNet};

    fn mk_ic(id: i64, pin_count: usize, pin_ids: &[i64]) -> McVecBox {
        let mut b = McVecBox::new_v2(
            id,
            format!("U{id}"),
            "IC.TEST".into(),
            BoxKind::MultiPin,
            Symbol::Ic,
            None,
            None,
            pin_count,
            IoSummary::new(),
            format!("U{id}"),
            vec![],
        );
        for (i, pid) in pin_ids.iter().enumerate() {
            b.pins.push(BoxPin {
                id: *pid,
                pin_id: (i + 1).to_string(),
                description: String::new(),
                io: IoDirection::Unknown,
                port_dir: PortDir::None,
            });
        }
        b
    }

    fn mk_two_pin(id: i64, name: &str, pin_ids: &[i64]) -> McVecBox {
        let mut b = McVecBox::new_v2(
            id,
            name.into(),
            "CAP.TEST".into(),
            BoxKind::TwoPin,
            Symbol::Capacitor,
            None,
            None,
            2,
            IoSummary::new(),
            name.into(),
            vec![],
        );
        for (i, pid) in pin_ids.iter().enumerate() {
            b.pins.push(BoxPin {
                id: *pid,
                pin_id: (i + 1).to_string(),
                description: String::new(),
                io: IoDirection::Unknown,
                port_dir: PortDir::None,
            });
        }
        b
    }

    fn mk_net(nid: i64, name: &str, kind: NetKind, endpoints: &[(i64, i64)]) -> VizNet {
        VizNet::new(
            nid,
            name.into(),
            kind,
            NetRole::Signal,
            endpoints
                .iter()
                .map(|&(b, p)| EndpointRef::new(b, p, ""))
                .collect(),
        )
    }

    /// Build a two-net device layer: a multi-pin anchor IC + two series caps,
    /// one per net. Covers both a vertical-trunk lane (PWR, 2 anchor pins) and a
    /// single-pin horizontal-trunk lane (GND).
    fn build_test_graph() -> McVecGraph {
        let mut g = McVecGraph::new(100, "test".into());
        g.layer_style = LayerStyle::Device;
        g.boxes.push(mk_ic(1, 3, &[11, 12, 13]));
        g.boxes.push(mk_two_pin(2, "CAP_1", &[21, 22]));
        g.boxes.push(mk_two_pin(3, "CAP_2", &[31, 32]));
        g.nets.push(mk_net(
            201,
            "PWR",
            NetKind::Power,
            &[(1, 11), (1, 12), (2, 21)],
        ));
        g.nets
            .push(mk_net(202, "GND", NetKind::Ground, &[(1, 13), (3, 31)]));
        g
    }

    /// ★ Assertion 1 (device_layout_v2.md §7.1): the layout phase and the render
    /// phase must compute identical lanes. Both call `assign_regions` + `resolve_lanes`
    /// + `envelop_lanes` on the post-anchor-placement graph; if they drift, members
    /// and trunks diverge ("two layers each compute x" bug class).
    #[test]
    fn lanes_layout_match_render() {
        let mut g = build_test_graph();
        let mut layout_topos = build_topology(&g);
        place_by_topology(&mut g, &mut layout_topos);

        // Render side: replay P1 + P0 + P3 + PR4-envelope on the same (now
        // placed) graph — members already carry PinSlots from the layout phase.
        let mut render_topos = build_topology(&g);
        assign_regions(&g, &mut render_topos);
        let anchor = layer_anchor_id(&render_topos);
        assign_rows(&g, &mut render_topos, anchor);
        resolve_lanes(&g, &mut render_topos);
        envelop_lanes(&g, &mut render_topos);

        assert_eq!(layout_topos.len(), render_topos.len());
        for (a, b) in layout_topos.iter().zip(render_topos.iter()) {
            assert_eq!(
                a.lane, b.lane,
                "lane for net '{}' differs between layout and render",
                a.net_name
            );
        }
    }

    /// ★ Assertion 1b: two topologies on the same anchor must not share a lane —
    /// each gets its own channel offset, otherwise members would overlap.
    #[test]
    fn lanes_are_per_topology() {
        let mut g = build_test_graph();
        let mut topos = build_topology(&g);
        place_by_topology(&mut g, &mut topos);
        assert!(topos.len() >= 2);
        assert_ne!(
            topos[0].lane.axis, topos[1].lane.axis,
            "two topologies on one anchor must occupy distinct lanes"
        );
    }

    /// ★ Assertion 2 (device_layout_v2.md §7.2): no dangling segments — every
    /// segment endpoint must land on a pin point, on a terminal symbol, or on
    /// another segment of the same net. Catches the "line crossing the whole
    /// graph" bug class.
    #[test]
    fn no_dangling_segments() {
        let mut g = build_test_graph();
        let mut topos = build_topology(&g);
        place_by_topology(&mut g, &mut topos);

        let trees = build_all_trees(&g);
        assert_eq!(trees.len(), topos.len());
        for (topo, tree) in topos.iter().zip(trees.iter()) {
            let dangling = dangling_segments(topo, tree, &g);
            assert!(
                dangling.is_empty(),
                "net '{}' has dangling segment endpoints: {:?}",
                topo.net_name,
                dangling
            );
        }
    }

    /// ★ Terminal-symbol wire hygiene: every stub that connects a Ground /
    /// NetLabel / BusLabel symbol to the tree must NOT pass through any
    /// component box. This is the "GND/bus connecting line must not lie on top
    /// of another element" requirement — realized picks a free direction.
    #[test]
    fn terminal_wires_clear_of_boxes() {
        let mut g = build_test_graph();
        let mut topos = build_topology(&g);
        place_by_topology(&mut g, &mut topos);
        let trees = build_all_trees(&g);

        for tree in &trees {
            for sym in &tree.symbols {
                if !matches!(
                    sym.kind,
                    TreeSymbolKind::Ground | TreeSymbolKind::NetLabel | TreeSymbolKind::BusLabel
                ) {
                    continue;
                }
                for seg in &tree.segments {
                    let ends_at_sym =
                        (seg.x2 - sym.x).abs() < 0.01 && (seg.y2 - sym.y).abs() < 0.01;
                    if !ends_at_sym {
                        continue;
                    }
                    for b in &g.boxes {
                        if matches!(
                            b.kind,
                            BoxKind::PowerLabel | BoxKind::Dot | BoxKind::PortTerminal
                        ) {
                            continue;
                        }
                        assert!(
                            !segment_hits_box(seg.x1, seg.y1, seg.x2, seg.y2, b.x, b.y, b.w, b.h),
                            "net '{}' terminal wire ({:.0},{:.0})-({:.0},{:.0}) crosses box '{}'",
                            tree.net_name,
                            seg.x1,
                            seg.y1,
                            seg.x2,
                            seg.y2,
                            b.name
                        );
                    }
                }
            }
        }
    }

    /// ★ PR4 assertion: the enveloped lane span must cover ALL tap points along
    /// the trunk direction — the anchor pins AND every member's tap (where its
    /// line lands on this trunk). This is what makes the trunk reach a cap
    /// hanging below the anchor's own pin range; before the envelope, the span
    /// only covered anchor pins (plus a hand-tuned per-NetKind table) and a
    /// member hanging beyond the anchor range left a dangling tap.
    #[test]
    fn span_envelops_member_taps() {
        let mut g = build_test_graph();
        let mut topos = build_topology(&g);
        place_by_topology(&mut g, &mut topos);

        for topo in &topos {
            debug_assert!(topo.lane.horizontal, "row model: all trunks are horizontal");
            let mut taps: Vec<f64> = Vec::new();

            if let Some(group) = topo.groups.first() {
                let b = g.boxes.iter().find(|b| b.id == group.box_id).unwrap();
                for &pid in &group.pin_ids {
                    let s = slot_of(b, pid).unwrap();
                    let (px, _py) = slot_point(b, s);
                    taps.push(px);
                }
            }
            for group in topo.groups.iter().skip(1) {
                let b = g.boxes.iter().find(|b| b.id == group.box_id).unwrap();
                let (mx, _my) = member_pin_point(b, group);
                taps.push(mx);
            }

            let lo = taps.iter().cloned().fold(f64::MAX, f64::min);
            let hi = taps.iter().cloned().fold(f64::MIN, f64::max);
            assert!(
                topo.lane.span.0 <= lo + 0.001 && topo.lane.span.1 >= hi - 0.001,
                "net '{}' span {:?} does not envelop its tap points [{:.1}, {:.1}]",
                topo.net_name,
                topo.lane.span,
                lo,
                hi
            );
        }
    }

    /// ★ PR4: the trunk must extend to reach a member whose tap point lies BEYOND
    /// the anchor pin range — the classic decoupling cap whose other pin hangs
    /// down to a ground net farther out than the anchor's own ground pins.
    /// Before span enveloping the ground trunk ended at the anchor pins and the
    /// cap's ground tap dangled off the end.
    #[test]
    fn trunk_reaches_member_beyond_anchor_range() {
        let mut g = McVecGraph::new(300, "beyond".into());
        g.layer_style = LayerStyle::Device;
        g.boxes.push(mk_ic(1, 3, &[11, 12, 13]));
        g.boxes.push(mk_two_pin(2, "CAP_1", &[21, 22]));
        // Power rail (West): anchor pins 11,12 ↔ cap pin 21.
        g.nets.push(mk_net(
            401,
            "PWR",
            NetKind::Power,
            &[(1, 11), (1, 12), (2, 21)],
        ));
        // Ground (South): anchor pin 13 ↔ cap pin 22. The cap is placed by the
        // power net (hangs down from the West trunk), so its ground pin ends up
        // West of the anchor's own pin range.
        g.nets
            .push(mk_net(402, "GND", NetKind::Ground, &[(1, 13), (2, 22)]));

        let mut topos = build_topology(&g);
        place_by_topology(&mut g, &mut topos);

        let trees = build_all_trees(&g);
        let gnd = topos.iter().find(|t| t.net_name == "GND").unwrap();
        let gnd_tree = trees.iter().find(|t| t.net_name == "GND").unwrap();

        // The cap's ground pin must be covered by the enveloped GND span
        // (horizontal trunk → span is the x-range).
        let cap = g.boxes.iter().find(|b| b.id == 2).unwrap();
        let gnd_grp = gnd.groups.iter().find(|g| g.box_id == 2).unwrap();
        let (mx, _) = member_pin_point(cap, gnd_grp);
        assert!(
            gnd.lane.span.0 <= mx + 0.001 && gnd.lane.span.1 >= mx - 0.001,
            "GND trunk span {:?} does not reach cap ground tap x={:.1}",
            gnd.lane.span,
            mx
        );

        // And the realized trunk actually touches the tap (no dangling segment).
        let dangling = dangling_segments(gnd, gnd_tree, &g);
        assert!(
            dangling.is_empty(),
            "GND has dangling segments: {:?}",
            dangling
        );
    }

    /// Collect segment indices whose endpoints are neither a net pin point, a
    /// terminal symbol position, nor on another segment of the same net.
    fn dangling_segments(topo: &NetTopology, tree: &EquiTree, graph: &McVecGraph) -> Vec<usize> {
        let mut pin_points: Vec<(f64, f64)> = Vec::new();
        for grp in &topo.groups {
            if let Some(b) = graph.boxes.iter().find(|b| b.id == grp.box_id) {
                for &pid in &grp.pin_ids {
                    if let Some(s) = slot_of(b, pid) {
                        let (px, py) = match s.side {
                            EntrySide::Top => (b.x + b.w * s.offset, b.y),
                            EntrySide::Bottom => (b.x + b.w * s.offset, b.y + b.h),
                            EntrySide::Left => (b.x, b.y + b.h * s.offset),
                            EntrySide::Right => (b.x + b.w, b.y + b.h * s.offset),
                        };
                        pin_points.push((px, py));
                    }
                }
                // ★ M10.3: a two-pin member is a WIRE-CONTINUATION (Along/Series).
                // Its far pin is owned by the PARTNER net, yet this net's trunk
                // legitimately reaches it when the wire runs on past the member.
                if b.pins.len() == 2 {
                    for p in &b.pins {
                        if !grp.pin_ids.contains(&p.id) {
                            if let Some(s) = slot_of(b, p.id) {
                                let (px, py) = match s.side {
                                    EntrySide::Top => (b.x + b.w * s.offset, b.y),
                                    EntrySide::Bottom => (b.x + b.w * s.offset, b.y + b.h),
                                    EntrySide::Left => (b.x, b.y + b.h * s.offset),
                                    EntrySide::Right => (b.x + b.w, b.y + b.h * s.offset),
                                };
                                pin_points.push((px, py));
                            }
                        }
                    }
                }
            }
        }
        let sym_points: Vec<(f64, f64)> = tree.symbols.iter().map(|s| (s.x, s.y)).collect();
        let on_point = |ex: f64, ey: f64, pts: &[(f64, f64)], tol: f64| -> bool {
            pts.iter()
                .any(|&(px, py)| (px - ex).abs() <= tol && (py - ey).abs() <= tol)
        };

        let mut dangling = Vec::new();
        for (idx, seg) in tree.segments.iter().enumerate() {
            for (ex, ey) in [(seg.x1, seg.y1), (seg.x2, seg.y2)] {
                let ok = on_point(ex, ey, &pin_points, 1.0)
                    || on_point(ex, ey, &sym_points, 16.0) // labels sit a few px off the trunk
                    || tree
                        .segments
                        .iter()
                        .enumerate()
                        .any(|(j, other)| j != idx && point_on_segment(ex, ey, other));
                if !ok {
                    dangling.push(idx);
                    break;
                }
            }
        }
        dangling
    }

    /// ★ Assertion 3 (device_layout_v2.md §7.3): no single side is overloaded —
    /// `max(side_count) ≤ ceil(total_pins / 2) + 1`. "5 pins on the right" fails.
    #[test]
    fn anchor_side_not_overloaded() {
        let mut g = build_test_graph();
        let mut topos = build_topology(&g);
        place_by_topology(&mut g, &mut topos);

        // Anchor is box 1 (referenced by both topologies).
        let anchor = g.boxes.iter().find(|b| b.id == 1).expect("anchor box");
        let total = anchor.pins.len();
        let mut counts: [usize; 4] = [0; 4];
        for s in &anchor.slots {
            let idx = match s.side {
                EntrySide::Left => 0,
                EntrySide::Right => 1,
                EntrySide::Top => 2,
                EntrySide::Bottom => 3,
            };
            counts[idx] += 1;
        }
        let max_side = *counts.iter().max().unwrap();
        let bound = (total + 1) / 2 + 1; // ceil(total/2) + 1
        assert!(
            max_side <= bound,
            "side overloaded: max={} total={} bound={}",
            max_side,
            total,
            bound
        );
    }

    /// ★ Assertion 4 (device_layout_v2.md §7.4): every Ground-direction pin must
    /// live on the South edge (Region::South.entry_side()).
    #[test]
    fn ground_pins_on_south() {
        let mut g = build_test_graph();
        let mut topos = build_topology(&g);
        place_by_topology(&mut g, &mut topos);

        for topo in &topos {
            if topo.lane.region != Region::South {
                continue;
            }
            let anchor_box = g.boxes.iter().find(|b| b.id == topo.anchor).unwrap();
            for &pid in &topo.groups.first().unwrap().pin_ids {
                let slot = anchor_box.slots.iter().find(|s| s.pin_id == pid).unwrap();
                assert_eq!(
                    slot.side,
                    EntrySide::Bottom,
                    "Ground pin {} of net '{}' is not on the South edge",
                    pid,
                    topo.net_name
                );
            }
        }
    }

    /// ★ Assertion 6 (device_layout_v2.md §7.6): the `[region] fallback` path is
    /// never hit — every net resolves to a definite Region by a direct rule or
    /// inheritance.
    #[test]
    fn no_region_fallback() {
        let g = build_test_graph();
        let mut topos = build_topology(&g);
        let fallbacks = assign_regions(&g, &mut topos);
        assert_eq!(fallbacks, 0, "unexpected [region] fallback hits");
    }

    /// ★ Assertion 5 (device_layout_v2.md §7.5): a two-pin passive whose OTHER
    /// pin net is N/S (Ground below a power rail) hangs vertically — h > w.
    /// This is the decoupling-cap-to-ground look.
    #[test]
    fn shunt_cap_hangs_vertical() {
        let mut g = McVecGraph::new(200, "shunt".into());
        g.layer_style = LayerStyle::Device;
        g.boxes.push(mk_ic(1, 4, &[11, 12, 13, 14]));
        g.boxes.push(mk_two_pin(2, "CAP_1", &[21, 22]));
        // Power rail (West): anchor pins 11,12 ↔ cap pin 21.
        g.nets.push(mk_net(
            301,
            "PWR",
            NetKind::Power,
            &[(1, 11), (1, 12), (2, 21)],
        ));
        // Ground (South): anchor pins 13,14 ↔ cap pin 22.
        g.nets.push(mk_net(
            302,
            "GND",
            NetKind::Ground,
            &[(1, 13), (1, 14), (2, 22)],
        ));

        let mut topos = build_topology(&g);
        place_by_topology(&mut g, &mut topos);

        // The IC (box 1) must be the layer anchor of both nets.
        assert!(
            topos.iter().all(|t| t.anchor == 1),
            "test IC should anchor both nets"
        );
        let cap = g.boxes.iter().find(|b| b.id == 2).expect("cap box");
        assert!(
            cap.h > cap.w,
            "Shunt cap to a South ground must hang vertically, got w={} h={}",
            cap.w,
            cap.h
        );
        // And it must sit below the power trunk (hanging down).
        let pwr = topos
            .iter()
            .find(|t| t.net_name == "PWR")
            .expect("PWR topo");
        let pwr_axis = pwr.lane.axis;
        assert!(
            cap.y >= pwr_axis,
            "Shunt cap should hang downward from the power trunk, cap.y={} axis={}",
            cap.y,
            pwr_axis
        );
    }

    /// ★ M16: a decoupling cap whose OTHER pin goes to its OWN terminal-only
    /// ground (no IC pin — just the cap's second pin + a glyph) and which is
    /// on a Power net anchored DIRECTLY on the IC lies ALONG the power trunk —
    /// horizontal (w > h), the far pin carrying the ground symbol. This is the
    /// `us513` `_C1`/`_C2` look, and the mirror image of
    /// `shunt_cap_hangs_vertical` where the cap returns to a SHARED IC ground
    /// and must hang down.
    #[test]
    fn decoupling_cap_to_own_ground_lies_horizontal() {
        let mut g = McVecGraph::new(400, "decap".into());
        g.layer_style = LayerStyle::Device;
        g.boxes.push(mk_ic(1, 3, &[11, 12, 13]));
        g.boxes.push(mk_two_pin(2, "CAP_1", &[21, 22]));
        // Power rail (West): the IC anchors it, the cap's pin 21 joins.
        g.nets
            .push(mk_net(401, "PWR", NetKind::Power, &[(1, 11), (2, 21)]));
        // The cap's OWN ground: cap pin 22 only — no IC pin → terminal-only.
        g.nets.push(mk_net(402, "GND", NetKind::Ground, &[(2, 22)]));

        let mut topos = build_topology(&g);
        place_by_topology(&mut g, &mut topos);

        let cap = g.boxes.iter().find(|b| b.id == 2).expect("cap box");
        assert!(
            cap.w > cap.h,
            "decap to its own terminal-only ground must lie horizontal, got w={} h={}",
            cap.w,
            cap.h
        );
        // The body sits ON the power rail row (not hanging below it).
        let pwr = topos
            .iter()
            .find(|t| t.net_name == "PWR")
            .expect("PWR topo");
        assert!(
            cap.y <= pwr.lane.axis + 0.5,
            "decap should sit ON the rail row, cap.y={} axis={}",
            cap.y,
            pwr.lane.axis
        );
    }

    /// ★ M12.4b: a Drop whose ground partner is a SHARED (non-terminal-only)
    /// ground must hang DOWN even when its body would cross another net's trunk
    /// row. `flip_shunts_clear_of_rows` used to flip it UP, which put the ground
    /// pin on top and forced the ground tooth back over the member's own body
    /// (`moddcdc` `_C2` = `_net1`↔GND@lp322dcdc). A down-hang tooth crossing a
    /// trunk is a clean wire crossing; a tooth through the body is not.
    #[test]
    fn ground_drop_not_flipped_up_across_row() {
        let mut g = McVecGraph::new(500, "gnddrop".into());
        g.layer_style = LayerStyle::Device;
        let mut ic = mk_ic(1, 4, &[11, 12, 13, 14]);
        // Pins 11 and 13 are INPUTS → their nets go West (same side); pin 12 is
        // the IC's shared ground. `direct_region` sends an Input anchor pin West,
        // so PWR rides the top West row and AUX the lower one — AUX's trunk lands
        // INSIDE CAP_A's down-hang (axis 200 vs down-to 240), the geometry that
        // used to trigger the M12.4 up-flip.
        for p in &mut ic.pins {
            if p.id == 11 || p.id == 13 {
                p.io = IoDirection::Input;
            }
        }
        g.boxes.push(ic);
        g.boxes.push(mk_two_pin(2, "CAP_A", &[21, 22]));
        g.boxes.push(mk_two_pin(3, "CAP_B", &[31, 32]));
        g.nets
            .push(mk_net(501, "PWR", NetKind::Power, &[(1, 11), (2, 21)]));
        // GND is SHARED — it also joins the IC, so it is not terminal-only and
        // gets its own trunk row below the power rows.
        g.nets.push(mk_net(
            502,
            "GND",
            NetKind::Ground,
            &[(1, 12), (2, 22), (3, 32)],
        ));
        // A second net on the SAME side (West) with a member whose column sits
        // under CAP_A's x, so a down-hang would cross AUX's trunk row.
        g.nets
            .push(mk_net(503, "AUX", NetKind::Signal, &[(1, 13), (3, 31)]));

        let mut topos = build_topology(&g);
        place_by_topology(&mut g, &mut topos);

        let cap = g.boxes.iter().find(|b| b.id == 2).expect("CAP_A box");
        let pwr = topos.iter().find(|t| t.net_name == "PWR").expect("PWR");
        // The drop must hang DOWN from the power trunk: box top below the trunk.
        assert!(
            cap.y > pwr.lane.axis + 0.5,
            "ground drop must hang DOWN (M12.4b), cap.y={} pwr.axis={}",
            cap.y,
            pwr.lane.axis
        );
        // ...with the power pin (pin 21, on the PWR net) facing the trunk — the
        // ground pin therefore sits at the bottom, clear of the body.
        let pwr_slot = cap
            .slots
            .iter()
            .find(|s| s.pin_id == 21)
            .expect("power pin slot");
        assert_eq!(
            pwr_slot.side,
            EntrySide::Top,
            "power pin should face the trunk (hang down), slots={:?}",
            cap.slots
        );
    }
}
