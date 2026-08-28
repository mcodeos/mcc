// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Box-related data structures
//!
//! - [`IoSummary`] -- box IO port count statistics
//! - [`Wire`]      -- single wire (compatible with legacy `McVecEdge.wires` field)
//! - [`McVecBox`]  -- single box (component / sub-module / power label)
//! - [`EntryPoint`] -- pin position on the box edge (for router to accurately exit lines)
//!
//! ## ★ P01 (S2) Changes
//! `McVecBox` added three semantic fields, filled once by the builder (`from_block.rs::detect_symbol`):
//! - `symbol: Symbol`           -- component symbol type (Resistor / Capacitor / Ic / ...)
//! - `designator: Option<String>` -- project designator (R1 / C5 / U3)
//! - `value: Option<String>`    -- nominal value (10k / 100nF)
//!
//! Legacy `McVecBox::new(...)` is kept, internally forwarding to `new_v2(symbol=Unknown, designator=None, value=None)`,
//! callers gradually migrate to `new_v2`.

use super::kinds::BoxKind;
use super::netdef::IoDirection;
use super::symbol::Symbol;

// ============================================================================
// IoSummary
// ============================================================================

/// Box IO port count statistics
#[derive(Debug, Clone)]
pub struct IoSummary {
    pub inputs: usize,
    pub outputs: usize,
    pub power: usize,
    pub other: usize,
}

impl IoSummary {
    pub fn new() -> Self {
        Self {
            inputs: 0,
            outputs: 0,
            power: 0,
            other: 0,
        }
    }
}

impl Default for IoSummary {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Wire (compatible with legacy McVecEdge model)
// ============================================================================

/// Single wire (unit stored in legacy `McVecEdge.wires`)
///
/// **Note**: only used by the legacy binary `McVecEdge`. New code uses
/// [`super::netdef::EndpointRef`].
#[derive(Debug, Clone)]
pub struct Wire {
    pub src_pin_id: i64,
    pub src_pin_name: String,
    pub dst_pin_id: i64,
    pub dst_pin_name: String,
}

// ============================================================================
// EntryPoint -- pin's precise position on the box edge
// ============================================================================

/// Pin / port's precise position on a box's edge
///
/// Router uses this data to know exactly where lines should leave from a box,
/// instead of the current approximate algorithm of "evenly divide the box's 4 edges, guess
/// which side is closest to the opposite side".
///
/// Filled by the layout phase (after computing box.x/y/w/h).
#[derive(Debug, Clone, PartialEq)]
pub struct EntryPoint {
    /// Pin / port's global ID (corresponds to an entry in InstTable)
    pub pin_id: i64,
    /// Pin name (used for labeling on the graph)
    pub pin_name: String,
    /// Which edge of the box this pin is on
    pub side: EntrySide,
    /// Relative position along that edge [0.0, 1.0] (0.0 = edge start / top / left)
    pub offset: f64,
}

/// Box edge the pin is on
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntrySide {
    Top,
    Right,
    Bottom,
    Left,
}

// ============================================================================
// BoxPin -- box's pin (from mcode parsing, not related to wiring)
// ============================================================================

/// A physical pin of a box (filled by the builder from `InstTable.get_pins_of` / `get_ports_of`)
///
/// This is the "what pins the component itself has" source of truth, **independent of whether
/// the pin is wired**. Previously `entry_points` only collected from `graph.nets` (already-wired
/// pins), causing unconnected boxes to not have a single pin drawn. `pins` solves this:
/// even if the box is completely unconnected, all pins can still be drawn.
///
/// ## Pin's two pieces of information (corresponds to mcode `pins = [ <pin_id> = <description> ]`)
/// - `pin_id`      -- pin's **common name / number** (mc `=` left: `"1"` / `"B"` / `"A1"`).
///   This is the marker drawn on the pin stub (industry convention: pin number / pin name).
///   **Taken directly from mcode, no longer self-numbering 1/2/3** -- letter pins (B/C/E)
///   draw B/C/E, numeric pins draw 1/2/3.
/// - `description` -- pin's **specific description / function name** (mc `=` right: `"TX"` / `"Base"`).
///   Drawn on the box's **inside**. When data is missing, it's empty, then only `pin_id` is drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct BoxPin {
    /// Pin's global ID (corresponds to the id of this pin entry in InstTable, same as net endpoint pin_id)
    pub id: i64,
    /// Pin's common name / number (mc `=` left: "1"/"B"/"A1"); drawn on the pin stub
    pub pin_id: String,
    /// Pin's specific description / function name (mc `=` right: "TX"/"Base"); drawn on box inside, may be empty
    pub description: String,
    /// Pin direction (input / output / power / ...)
    pub io: IoDirection,
    /// ★ M0-2: module port direction (in/out/io/ps); non-ports are PortDir::None
    pub port_dir: PortDir,
}

// ============================================================================
// ★ M0-3: PinConstraint — pin placement constraint level
// ============================================================================

/// Pin placement constraint level
///
/// Rules:
/// - Source has a `layout` block → `FixedOrder` (side and order both fixed; only whole-box mirror/rotation allowed)
/// - Otherwise → `Free` (the placer may freely assign pins to any side, any order)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PinConstraint {
    /// The placer may freely assign pins to any side, any order (current behavior)
    #[default]
    Free,
    /// Side fixed, order within the side adjustable
    FixedSide,
    /// Side and order both fixed; only whole-box mirror/rotation allowed
    FixedOrder,
}

// ============================================================================
// ★ M0-2: PortDir — module port direction
// ============================================================================

/// Module port direction (from `in` / `out` / `io` / `ps` declarations)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PortDir {
    In,
    Out,
    Io,
    #[default]
    Ps,
    /// Not a port (device pin)
    None,
}

// ============================================================================
// ★ P7-1: BoxProvenance — where did this box come from?
// ============================================================================

/// Box provenance marker (the data basis of the G10 structure-conservation criterion).
///
/// - `Declared`: from `block.insts` (real instances: devices / sub-modules / declared labels)
/// - `SynthesizedFromEndpoint`: boxes **synthesized** by `fromblock.rs` Phase 1.5 from net
///   endpoints (products of netlist stubs / duplicate endpoints / label pseudo-points
///   entering viz directly; golden target = 0)
/// - `SynthesizedRailFlag`: per-consumer flag boxes synthesized when `rails.rs` explodes
///   a rail (golden target = 0 —— terminals should be pin decorations, not boxes;
///   see discipline 11)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BoxProvenance {
    #[default]
    Declared,
    SynthesizedFromEndpoint,
    SynthesizedRailFlag,
}

impl BoxPin {
    /// Text to label on the pin stub = common name / number (`pin_id`).
    pub fn stub_label(&self) -> &str {
        &self.pin_id
    }
}

// ============================================================================
// PinLayout -- reserved interface ①: component customizes "which pins go on which edge"
// ============================================================================

/// Component pin-per-edge layout (drawing-side form, decoupled from core layer's
/// `semantic::component::mc_layout::McLayout`).
///
/// Each Vec contains the pin identifiers on that edge -- `BoxPin.pin_id` (number, like "B"/"1"/"VCC")
/// **or** `BoxPin.description` (function name, like "Base"), both can match. The order within Vec is
/// the arrangement order from edge start to end (Left/Right from top to bottom, Top/Bottom from
/// left to right).
///
/// ## Source / Consumer
/// - **Source**: `core`'s `McComponent.layout` -> convert to this struct ->
///   `McVecBox::set_layout_hint` (filled by `fromblock.rs::apply_reserved_overrides`).
/// - **Consumer** (in place): `entry_points::compute_entry_points` finds `layout_hint` non-empty then
///   assigns edges / sorts by it, otherwise goes through `classify_pin` heuristic. Pins not listed in
///   the layout fall back to the heuristic, ensuring none are missed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PinLayout {
    pub left: Vec<String>,
    pub right: Vec<String>,
    pub top: Vec<String>,
    pub bottom: Vec<String>,
}

impl PinLayout {
    pub fn is_empty(&self) -> bool {
        self.left.is_empty()
            && self.right.is_empty()
            && self.top.is_empty()
            && self.bottom.is_empty()
    }

    /// Query which edge a pin identifier (pin_id or description) is assigned to in the layout;
    /// returns `None` if not listed.
    pub fn side_of(&self, key: &str) -> Option<EntrySide> {
        if self.left.iter().any(|s| s == key) {
            Some(EntrySide::Left)
        } else if self.right.iter().any(|s| s == key) {
            Some(EntrySide::Right)
        } else if self.top.iter().any(|s| s == key) {
            Some(EntrySide::Top)
        } else if self.bottom.iter().any(|s| s == key) {
            Some(EntrySide::Bottom)
        } else {
            None
        }
    }
}

// ============================================================================
// CustomSymbol -- reserved interface ②: user-customized component symbol (overrides system default)
// ============================================================================

/// User-provided custom symbol (replaces system-provided R/C/L/D/IC etc. drawing).
///
/// `svg_body` is a validated SVG fragment with an explicit source `viewBox`. At render time it is
/// scaled into the box while pin markers remain overlaid by `pin_render` per entry point. A custom
/// symbol is only responsible for what the component body looks like.
///
/// ## Source / Consumer
/// - **Source**: project-local `symbols/manifest.toml` -> class-name match ->
///   `McVecBox::set_custom_symbol`. Missing or invalid assets leave `custom_symbol` as `None`.
/// - **Consumer** (in place): `shape::render_box` checks `custom_symbol` first, uses it if available,
///   otherwise falls back to system-provided symbol.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SvgViewBox {
    pub min_x: f64,
    pub min_y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CustomSymbol {
    /// Source identifier (symbol library key / class_name), only for debugging / tracing, will be written to `data-symbol-source`.
    pub source: String,
    /// SVG fragment (without outer `<g>`; renderer wraps translate + data-id + pin overlay).
    pub svg_body: String,
    /// Validated source viewBox used to scale the fragment into the component body.
    pub view_box: SvgViewBox,
}

// ============================================================================
// BoxLabelPlacement (M8)
// ============================================================================

/// A lightweight label placement hint stored on McVecBox.
/// When non-empty, render and metrics use these instead of hardcoded defaults.
#[derive(Debug, Clone, PartialEq)]
pub struct BoxLabelPlacement {
    pub text: String,
    pub kind: LabelPlacementKind,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub inside_owner_box: bool,
    pub font_size: f64,
    pub text_anchor: &'static str,
    pub dominant_baseline: &'static str,
}

/// Distinguishes designator vs value labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelPlacementKind {
    Designator,
    Value,
}

// ============================================================================
// McVecBox
// ============================================================================

/// A box on the graph (component / sub-module / power label)
#[derive(Debug, Clone)]
pub struct McVecBox {
    pub id: i64,
    pub name: String,
    pub class_name: String,
    pub kind: BoxKind,

    /// ★ M0: full hierarchical instance path
    ///
    /// `"main.ldo.ldo"` vs `"main.mcu.ldo"` — this is the prerequisite
    /// for M2 zone partitioning. `name` is still the leaf segment.
    pub inst_path: String,

    /// ★ M0: parent module chain (excluding the leaf)
    ///
    /// `"main.ldo.ldo"` → `["main", "main.ldo"]`.
    /// Computed from `inst_path` by splitting on `'.'`.
    pub scope_chain: Vec<String>,

    /// ★ P01: component symbol type (R / C / IC / power / ...)
    ///
    /// Filled once by the builder in the `detect_symbol` phase, then read-only and unchanged.
    /// `Unknown` is the fallback, rendering degrades to a regular rectangle per `BoxKind`.
    pub symbol: Symbol,

    /// ★ P01: project designator (R1, C5, U3), None if not available
    pub designator: Option<String>,

    /// ★ P01: nominal value (10k, 100nF, 1uH), None if not available
    ///
    /// In this phase, pass2's `InstEntry` has no value field, all None,
    /// P05 render only shows designator, not value.
    pub value: Option<String>,

    pub pin_count: usize,
    pub io_summary: IoSummary,

    // ── Filled by layout phase ──
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,

    /// Pin position on box edge (filled after layout, used by router)
    ///
    /// Empty by default, the layout algorithm should fill this field after computing (x, y, w, h).
    /// When the old layout algorithm doesn't fill this field, the router falls back to the
    /// approximate algorithm of "evenly divide the box's four edges".
    pub entry_points: Vec<EntryPoint>,

    /// ★ Box's physical pin list (filled by builder from mcode/InstTable, independent of wiring)
    ///
    /// Empty by default. Connected pins still go through net->entry_points route;
    /// this field ensures "unconnected pins can also be drawn" (pin number / name / direction complete).
    pub pins: Vec<BoxPin>,

    /// ★ M8: Label placement hints (filled by label optimizer, used by render/metrics).
    ///
    /// Empty by default. When non-empty, render and metrics use these positions
    /// instead of the fixed default positions.
    pub label_placements: Vec<BoxLabelPlacement>,

    /// ★ Reserved interface ①: component customizes pin-per-edge layout. `None` = goes through
    /// heuristic edge assignment (default). Filled by builder later from `McComponent.layout`;
    /// see [`PinLayout`].
    pub layout_hint: Option<PinLayout>,

    /// ★ M0-3: pin placement constraint level. Has a layout block → FixedOrder, otherwise Free.
    pub pin_constraint: PinConstraint,

    /// ★ M0-B-D: not-fitted marker (from InstEntry.not_fitted)
    pub not_fitted: bool,

    /// ★ M0-B-E: instance origin (declaration vs funcall), from InstEntry.origin
    pub origin: crate::instant::insttab::InstOrigin,

    /// ★ Reserved interface ②: user-customized component symbol. `None` = uses system-provided
    /// symbol (default). Filled by builder later based on class_name hitting user symbol library;
    /// see [`CustomSymbol`].
    pub custom_symbol: Option<CustomSymbol>,

    /// ★ P2 (bridge passive): visual role hint for layout. `None` = normal placement.
    /// Set to `Some(BridgePassive)` when a 2-pin passive component is a transposed
    /// bridge/shunt across two parallel lanes (e.g., CAP' in two-lane series).
    pub visual_role: Option<VisualRole>,

    /// ★ Virtual instantiation view (mcd docs-mc 16-export-viz §6): suppress the
    /// instance-name label. Used by the synthetic module wrapper (VIRT_<Name>)
    /// whose instance (u_1) is a fabrication, not a real designator — the box
    /// then shows only the class name and the physical pins.
    pub suppress_instance_name: bool,

    /// ★ virtual: this box belongs to a synthetic wrapper generated by virtual
    /// instantiation (`module VIRT_<T> { T u_1 }`). True only for boxes under
    /// the wrapper module's scope. Renderers use it to hide the fabricated
    /// module/instance and draw the wrapped unit as a bare IC with pins.
    pub synthetic: bool,

    /// Geometry + entry points owned by a deterministic placer (ladder). Passes that
    /// reposition boxes heuristically must skip these.
    pub geom_locked: bool,

    /// ★ P7-7: rail anchor hint — where to place a degree=0 passive whose
    /// both ends are rail. Set by classify_rails, consumed by phase_placement.
    pub anchor_hint: Option<AnchorHint>,

    /// ★ P7-4: the last pipeline stage that changed this box's geometry
    /// (x/y/w/h/entry_points).
    ///
    /// For observation (roadmap P7-4 task 1): written by stage-boundary snapshot
    /// comparison (see `McVecGraph::claim_geom_changes`), **observe-only, no
    /// blocking**. The target shape is Placement (x/y) → PinPlace
    /// (entry_point) → Route (read-only), three stages with no write-back
    /// between stages —— a cross-stage change of `geom_writer` is the direct
    /// evidence of write-back.
    pub geom_writer: Option<&'static str>,

    /// ★ P7-1: box provenance marker (default Declared). renderdiff G10 uses it to count synthesized boxes.
    pub provenance: BoxProvenance,

    /// ★ P8-2 (G16): source span for bidirectional traceability.
    /// `(file, line)` — which source file and line declared this box.
    /// Populated by fromblock.rs from InstEntry source info.
    pub source_span: Option<crate::semantic::common::SourcePos>,
    /// ★ C1b F1: Pin slots — single source of truth for pin positions.
    /// Set by layout, read by renderer and geometry.
    pub slots: Vec<PinSlot>,
}

/// ★ C1b F1: Pin slot — single source of truth for pin positions.
/// Set by layout, read by renderer and geometry.
#[derive(Debug, Clone)]
pub struct PinSlot {
    pub pin_id: i64,
    /// Sort key for pin ordering (0, 1, 2, ...)
    pub number: u32,
    pub name: String,
    /// Which edge of the box this pin is on
    pub side: EntrySide,
    /// Position along the edge (0.0 = start, 1.0 = end)
    pub offset: f64,
    /// Whether this pin is connected to a net
    pub connected: bool,
}

/// ★ P2: Visual role hint for layout placement
#[derive(Debug, Clone, PartialEq)]
pub enum VisualRole {
    /// Transposed 2-pin passive that bridges two parallel lanes.
    /// Pin1 connects to the upper lane, Pin2 to the lower lane.
    BridgePassive,
    /// Series 2-pin passive already placed inline on a lane by a dedicated
    /// layouter (e.g. two_lane_ladder). Generic passive passes skip it.
    SeriesInline,
}

/// ★ P7-7: rail anchor hint — where a degree=0 passive should be placed.
///
/// When `classify_rails` removes rail nets, two-pin passives whose both ends are
/// rail lose all net connections (degree=0) and would be parked as isolated.
/// The anchor hint tells the placer where to pin them instead, using the rail net
/// itself as the truth source (zero name matching).
#[derive(Debug, Clone, PartialEq)]
pub struct AnchorHint {
    /// The box that stays connected after rail removal (the IC being decoupled).
    pub host_box: i64,
    /// The pin on host_box that belongs to this rail net.
    pub host_pin: i64,
    /// Which side of the host to place the passive on (Bottom / Right etc.).
    pub side: EntrySide,
}

impl McVecBox {
    /// Simplest construction (legacy API before P01)
    ///
    /// symbol defaults to `Unknown`, designator/value defaults to `None`.
    /// To fill semantic information use [`McVecBox::new_v2`].
    pub fn new(
        id: i64,
        name: String,
        class_name: String,
        kind: BoxKind,
        pin_count: usize,
        io_summary: IoSummary,
    ) -> Self {
        let inst_path = name.clone();
        Self::new_v2(
            id,
            name,
            class_name,
            kind,
            Symbol::Unknown,
            None,
            None,
            pin_count,
            io_summary,
            inst_path,
            Vec::new(),
        )
    }

    /// ★ P01: full construction (including symbol / designator / value)
    ///
    /// `from_block.rs::build_mc_vec_graph` uses this, semantic information filled in one go.
    #[allow(clippy::too_many_arguments)]
    pub fn new_v2(
        id: i64,
        name: String,
        class_name: String,
        kind: BoxKind,
        symbol: Symbol,
        designator: Option<String>,
        value: Option<String>,
        pin_count: usize,
        io_summary: IoSummary,
        inst_path: String,
        scope_chain: Vec<String>,
    ) -> Self {
        Self {
            id,
            name,
            class_name,
            kind,
            inst_path,
            scope_chain,
            symbol,
            designator,
            value,
            pin_count,
            io_summary,
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            entry_points: Vec::new(),
            pins: Vec::new(),
            label_placements: Vec::new(),
            layout_hint: None,
            pin_constraint: PinConstraint::Free,
            not_fitted: false,
            origin: crate::instant::insttab::InstOrigin::Declared,
            custom_symbol: None,
            visual_role: None,
            suppress_instance_name: false,
            synthetic: false,
            geom_locked: false,
            anchor_hint: None,
            geom_writer: None,
            provenance: BoxProvenance::Declared,
            source_span: None,
            slots: Vec::new(),
        }
    }

    /// Find the EntryPoint for a pin by pin_id (for router to query exit position)
    pub fn find_entry(&self, pin_id: i64) -> Option<&EntryPoint> {
        self.entry_points.iter().find(|e| e.pin_id == pin_id)
    }

    /// ★ P7-4: geometry writer observation (since P7-4e the adjudication moved up to
    /// `McVecGraph::claim_geom_changes`, judging violations by dimension ownership).

    /// ★ Set the box's physical pin list (called by builder)
    pub fn set_pins(&mut self, pins: Vec<BoxPin>) {
        // pin_count follows the real pin count (if previously 0 / estimated, use real value to override)
        if !pins.is_empty() {
            self.pin_count = pins.len();
        }
        self.pins = pins;
    }

    /// Find the physical pin by pin_id (for render to query pin number / name)
    pub fn find_pin(&self, pin_id: i64) -> Option<&BoxPin> {
        self.pins.iter().find(|p| p.id == pin_id)
    }

    /// ★ R-C2: return physical pins that are not connected to any net (NC pins).
    /// These pins should be drawn with an NC mark (discipline 27).
    pub fn nc_pins(&self) -> Vec<&BoxPin> {
        let connected: std::collections::HashSet<i64> =
            self.entry_points.iter().map(|ep| ep.pin_id).collect();
        self.pins
            .iter()
            .filter(|p| !connected.contains(&p.id))
            .collect()
    }

    /// ★ Reserved interface ①: set component custom pin layout (empty layout considered not set, still goes through heuristic).
    pub fn set_layout_hint(&mut self, layout: PinLayout) {
        if !layout.is_empty() {
            self.layout_hint = Some(layout);
        }
    }

    /// ★ Reserved interface ②: set user-customized symbol (called by builder after hitting symbol library by class_name).
    pub fn set_custom_symbol(&mut self, sym: CustomSymbol) {
        self.custom_symbol = Some(sym);
    }

    /// ★ P01: whether it's a two-pin passive component (R/C/L/D series)
    pub fn is_two_pin_passive(&self) -> bool {
        self.symbol.is_two_pin_passive()
    }

    /// Container / border box: provides only a visual border and never participates in collision detection.
    /// Boxes with a negative id (e.g. `main` id=-1001) are SubModule container borders;
    /// they naturally enclose all child boxes and wires, so they are not counted as collisions.
    #[inline]
    pub fn is_container_box(&self) -> bool {
        self.id < 0 || self.kind == BoxKind::SubModule
    }

    /// This passive's geometry has no owner yet —— only when this is true may the fallback pass move it.
    ///
    /// `geom_locked` is the real meaning of "this box's geometry already has an owner";
    /// `visual_role` is a rendering intent, and the two should not share one field.
    /// The three passive passes all use this predicate in their filters.
    #[inline]
    pub fn is_unowned_passive(&self) -> bool {
        self.is_two_pin_passive() && !self.geom_locked && self.visual_role.is_none()
    }

    /// ★ P01: display label (prefer designator, fall back to name)
    pub fn display_label(&self) -> &str {
        self.designator.as_deref().unwrap_or(&self.name)
    }
}

// ============================================================================
// ZoneBorder — functional zone border (M2-3)
// ============================================================================

/// Border info of one zone, for the renderer to draw a dashed rounded rect + title
#[derive(Debug, Clone)]
pub struct ZoneBorder {
    /// x coordinate
    pub x: f64,
    /// y coordinate
    pub y: f64,
    /// width
    pub w: f64,
    /// height
    pub h: f64,
    /// title
    pub title: String,
    /// title anchor
    pub title_x: f64,
    pub title_y: f64,
}
