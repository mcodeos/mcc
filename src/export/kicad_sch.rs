// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! KiCad hierarchical schematic export (`.kicad_sch`, KiCad 9 file format).
//!
//! One module layer = one sheet file: the top module's layer is the root
//! schematic and every sub-module opens as a `sheet` instance, mirroring the
//! drill-down hierarchy of `circuit.html`. Electrical truth stays with the
//! netlist exporters (`super::netlist`); the drawing decides only where things
//! sit, so all geometry is read from the viz layout's own final coordinates —
//! box rects, pin anchors, equipotential trees (`build_all_trees`, the same
//! replay the renderer draws) and routed nets — never recomputed here.
//!
//! Coordinate conventions (verified against the local KiCad's own files):
//! sheet coordinates are mm, y grows downward; symbol-internal coordinates are
//! y-up, so a lib pin at internal `(dx, dy)` lands at sheet
//! `(X + dx, Y - dy)` for an unrotated instance. The exporter scales 1 px to
//! 0.254 mm and copies viz coordinates verbatim through that scale, so every
//! wire endpoint coincides with the pin connection point it was routed to.
//!
//! Generated uuids hash the element's stable identity (top name + layer bid +
//! kind + key), so re-exporting an unchanged design is byte-identical and a
//! KiCad-side annotation binding survives a re-export.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::instant::arena::NodeArena;
use crate::instant::inststore::InstanceStore;
use crate::instant::insttab::InstTable;
use crate::vector::builder::build_mc_vec_with_arena;
use crate::vector::graph::boxdef::BoxProvenance;
use crate::vector::graph::netdef::IoDirection;
use crate::vector::graph::{
    build_mc_vec_graph, BoxKind, EntryPoint, EntrySide, LayerStyle, McVecBox, McVecGraph, Symbol,
    VizNet,
};
use crate::viz::api::{render_with_metrics_and_sink, RenderOpts, RenderedLayer};
use crate::viz::layout::equipotential_tree::{build_all_trees, EquiTree, TreeSymbolKind};
use crate::McModuleInst;

/// Sheet scale: 1 viz px = 0.254 mm (10 mil), a quarter of KiCad's default
/// 50 mil grid. Small enough that a full-layout canvas fits A-series paper.
const MM_PER_PX: f64 = 0.254;
/// Pin lead length inside the lib symbol, in px.
const PIN_LEN_PX: f64 = 10.0;
/// Blank margin around the drawing, in px.
const MARGIN_PX: f64 = 80.0;
/// KiCad 9 schematic file format version.
const SCH_VERSION: &str = "20250114";

/// One written `.kicad_sch` file: a file name (relative to the output
/// directory) and its full s-expression body.
#[derive(Debug, Clone)]
pub struct SchFile {
    pub name: String,
    pub content: String,
}

/// Render the whole module hierarchy and emit one sheet file per layer.
///
/// The render runs the production pipeline (the same call `mcviz` makes), so
/// the exported drawing is the drawing the user already reviews; a Tier-1
/// fidelity gate failure is reported on stderr but does not block the export —
/// the sheets still mirror what the renderer drew.
pub fn build_kicad_sch_project(
    tree: &McModuleInst,
    table: &InstTable,
    arena: &NodeArena,
    inst_store: &InstanceStore,
    top: &str,
    flat: bool,
) -> Vec<SchFile> {
    let vec_block = build_mc_vec_with_arena(tree, table, arena, inst_store);
    let graph = build_mc_vec_graph(&vec_block, table);
    let mut layers: Vec<RenderedLayer> = Vec::new();
    let (_doc, _metrics) =
        render_with_metrics_and_sink(graph, RenderOpts::default(), Some(&mut layers));
    if crate::viz::layout::select::RENDER_GATE_FAILED.load(std::sync::atomic::Ordering::Relaxed) {
        eprintln!(
            "[export-kicad-sch] warning: the render fidelity gate (Tier 1) failed for this \
             design; the sheets mirror the drawing as rendered, which may drop content"
        );
    }
    if flat {
        emit_flat_sheet(&layers, table, top)
    } else {
        emit_sheets(&layers, top)
    }
}

// === S-expression emitter ===

/// Indentation-aware s-expression writer. Every KiCad element is a few `open`
/// / `atom` / `close` calls, which keeps parens balanced by construction.
struct Emit {
    s: String,
    depth: usize,
}

impl Emit {
    fn new() -> Self {
        Self {
            s: String::with_capacity(64 * 1024),
            depth: 0,
        }
    }

    fn open(&mut self, head: &str) {
        self.pad();
        self.s.push('(');
        self.s.push_str(head);
        self.s.push('\n');
        self.depth += 1;
    }

    fn atom(&mut self, line: &str) {
        self.pad();
        self.s.push_str(line);
        self.s.push('\n');
    }

    fn close(&mut self) {
        self.depth = self.depth.saturating_sub(1);
        self.pad();
        self.s.push_str(")\n");
    }

    fn pad(&mut self) {
        for _ in 0..self.depth {
            self.s.push('\t');
        }
    }
}

/// One `(...)` element on a single indented line.
macro_rules! line {
    ($e:expr, $($t:tt)*) => {
        $e.atom(&format!($($t)*))
    };
}

// === Sheet set assembly ===

/// The whole export's read-only wiring: layer list plus the names, files,
/// pages and uuids assigned in pre-order.
struct SheetSet<'a> {
    top: String,
    root_uuid: String,
    layers: &'a [RenderedLayer],
    /// bid -> index into `layers`
    by_bid: HashMap<i64, usize>,
    /// bid -> sheet file name
    file_of: HashMap<i64, String>,
    /// bid -> uuid of the `sheet` element (in the parent) that opens it
    sheet_uuid_of: HashMap<i64, String>,
    /// bid -> page number, assigned in pre-order (root = 1)
    page_of: HashMap<i64, usize>,
    /// child bid -> its boundary ports as (name, KiCad label shape). Computed
    /// once from the child's own graph, it is the ONE list both the parent's
    /// sheet pins and the child's hierarchical labels read, so the two sides
    /// agree by construction instead of by two derivations agreeing.
    ports_of: HashMap<i64, Vec<(String, &'static str)>>,
}

/// The boundary-port list of one child layer: its boundary nets first (in net
/// order), then the module's declared ports that no boundary net carried.
fn child_ports_of(graph: &McVecGraph) -> Vec<(String, &'static str)> {
    let mut v: Vec<(String, &'static str)> = Vec::new();
    for n in &graph.nets {
        if let Some(bi) = &n.boundary {
            let shape = shape_of_io(bi.io);
            if writable_port_name(&bi.port_name) && !v.iter().any(|(nm, _)| nm == &bi.port_name) {
                v.push((bi.port_name.clone(), shape));
            }
        }
    }
    for (name, dir, _role) in &graph.module_ports {
        if v.iter().any(|(nm, _)| nm == name) || !writable_port_name(name) {
            continue;
        }
        let shape = match dir {
            crate::vector::graph::PortDir::In => "input",
            crate::vector::graph::PortDir::Out => "output",
            crate::vector::graph::PortDir::Io => "bidirectional",
            _ => "passive",
        };
        v.push((name.clone(), shape));
    }
    v
}

/// KiCad's netname grammar reserves `{…}` for escape syntax, so a port-group
/// display label (`dc{VDD_3V3, GND}`) cannot be written as a pin/label name.
/// Skipping it is a format limitation, reported once — the name is never
/// rewritten into something the source did not say.
fn writable_port_name(name: &str) -> bool {
    let ok = !name.contains('{') && !name.contains('}');
    if !ok {
        eprintln!(
            "[export-kicad-sch] port name not representable in KiCad, skipped: {name}"
        );
    }
    ok
}

/// Per-sheet mutable state, split from [`SheetSet`] so the layer graphs can
/// stay borrowed while references and counters advance.
struct SheetState {
    /// sheet bid -> references already used on it (a repeated designator gets
    /// `_2`, `_3`, …; cross-sheet repeats are legitimate, separate scopes)
    used_refs: HashMap<i64, HashSet<String>>,
    /// running `#PWR` reference counter, reset per sheet
    pwr_seq: usize,
    /// power-net values already flagged in some sheet: a `(power)` symbol
    /// names a GLOBAL net, so a second flag elsewhere would read as a second
    /// driver
    flagged: HashSet<String>,
    /// occupied label boxes (sheet mm): the anti-overlap ledger every text
    /// label registers with before it lands
    labels: Vec<(f64, f64, f64, f64)>,
}

fn emit_sheets(layers: &[RenderedLayer], top: &str) -> Vec<SchFile> {
    let mut by_bid = HashMap::new();
    for (i, l) in layers.iter().enumerate() {
        by_bid.insert(l.graph.bid, i);
    }
    let mut set = SheetSet {
        top: sanitize_file_stem(top),
        root_uuid: det_uuid(&format!("{top}#root")),
        layers,
        by_bid,
        file_of: HashMap::new(),
        sheet_uuid_of: HashMap::new(),
        page_of: HashMap::new(),
        ports_of: HashMap::new(),
    };

    // Names and pages in pre-order, so a re-render of the same design numbers
    // everything the same way.
    for (i, l) in layers.iter().enumerate() {
        let stem = layer_stem(&set, i);
        set.file_of.insert(l.graph.bid, format!("{stem}.kicad_sch"));
        set.page_of.insert(l.graph.bid, i + 1);
        if l.parent.is_some() {
            let uuid = det_uuid(&format!("{}#sheet#{}", set.top, l.graph.bid));
            set.sheet_uuid_of.insert(l.graph.bid, uuid);
            set.ports_of.insert(l.graph.bid, child_ports_of(&l.graph));
        }
    }

    let mut state = SheetState {
        used_refs: HashMap::new(),
        pwr_seq: 0,
        flagged: HashSet::new(),
        labels: Vec::new(),
    };
    let mut files = Vec::new();
    for i in 0..layers.len() {
        let name = set.file_of[&layers[i].graph.bid].clone();
        files.push(SchFile {
            name,
            content: emit_sheet(&set, &mut state, i),
        });
    }
    files
}

/// File stem for a layer: the top name for the root, else `<top>_<leaf>` where
/// the leaf is the layer's own module name — unique per layer because module
/// instance names are unique among siblings, and stable across re-exports.
fn layer_stem(set: &SheetSet, idx: usize) -> String {
    let l = &set.layers[idx];
    if l.parent.is_none() {
        return set.top.clone();
    }
    format!("{}_{}", set.top, sanitize_file_stem(&l.graph.name))
}

// === Coordinate transform ===

/// Layer px -> sheet mm, from the drawing's own bounding box so the margin
/// never clips a label, a glyph, or a pin lead.
struct Xform {
    ox: f64,
    oy: f64,
    /// Global translation in sheet mm, used by the flat single-sheet mode to
    /// place each module's drawing side by side. Zero in hierarchical mode.
    gx: f64,
    gy: f64,
    /// Right/bottom edge of the drawing in sheet mm (for flat tiling).
    max_x: f64,
    max_y: f64,
}

impl Xform {
    fn new(graph: &McVecGraph, trees: &[EquiTree]) -> Self {
        Self::with_offset(graph, trees, 0.0, 0.0)
    }

    fn with_offset(graph: &McVecGraph, trees: &[EquiTree], gx: f64, gy: f64) -> Self {
        let mut min = (f64::MAX, f64::MAX);
        let acc = |x: f64, y: f64, min: &mut (f64, f64)| {
            min.0 = min.0.min(x);
            min.1 = min.1.min(y);
        };
        for b in &graph.boxes {
            acc(b.x - PIN_LEN_PX, b.y - PIN_LEN_PX, &mut min);
        }
        for t in trees {
            for s in &t.segments {
                acc(s.x1, s.y1, &mut min);
                acc(s.x2, s.y2, &mut min);
            }
            for j in &t.junction_dots {
                acc(j.0, j.1, &mut min);
            }
            for s in &t.symbols {
                acc(s.x, s.y, &mut min);
            }
        }
        if let Some(f) = &graph.module_frame {
            acc(f.x, f.y, &mut min);
        }
        let (ox, oy) = if min.0.is_finite() {
            (MARGIN_PX - min.0, if min.1.is_finite() { MARGIN_PX - min.1 } else { MARGIN_PX })
        } else {
            (MARGIN_PX, MARGIN_PX)
        };
        let mut max = (f64::MIN, f64::MIN);
        for b in &graph.boxes {
            max.0 = max.0.max(b.x + b.w + PIN_LEN_PX);
            max.1 = max.1.max(b.y + b.h + PIN_LEN_PX);
        }
        for t in trees {
            for sgm in &t.segments {
                max.0 = max.0.max(sgm.x1).max(sgm.x2);
                max.1 = max.1.max(sgm.y1).max(sgm.y2);
            }
            for sgm in &t.symbols {
                max.0 = max.0.max(sgm.x);
                max.1 = max.1.max(sgm.y);
            }
        }
        if let Some(f) = &graph.module_frame {
            max.0 = max.0.max(f.x + f.w);
            max.1 = max.1.max(f.y + f.h);
        }
        let (mx, my) = if max.0.is_finite() {
            ((max.0 + ox) * MM_PER_PX, (max.1 + oy) * MM_PER_PX)
        } else {
            (0.0, 0.0)
        };
        Xform {
            ox,
            oy,
            gx,
            gy,
            max_x: mx,
            max_y: my,
        }
    }

    fn x(&self, px: f64) -> f64 {
        (px + self.ox) * MM_PER_PX + self.gx
    }

    fn y(&self, px: f64) -> f64 {
        (px + self.oy) * MM_PER_PX + self.gy
    }
}

/// Pin connection point of one anchor, in sheet mm.
fn anchor_mm(xf: &Xform, b: &McVecBox, side: EntrySide, offset: f64) -> (f64, f64) {
    let (px, py) = match side {
        EntrySide::Top => (b.x + b.w * offset, b.y),
        EntrySide::Bottom => (b.x + b.w * offset, b.y + b.h),
        EntrySide::Left => (b.x, b.y + b.h * offset),
        EntrySide::Right => (b.x + b.w, b.y + b.h * offset),
    };
    (xf.x(px), xf.y(py))
}

// === One sheet ===

fn emit_sheet(set: &SheetSet, state: &mut SheetState, idx: usize) -> String {
    let layer = &set.layers[idx];
    let graph = &layer.graph;
    let is_device = graph.layer_style == LayerStyle::Device;
    let trees: Vec<EquiTree> = if is_device {
        build_all_trees(graph)
    } else {
        Vec::new()
    };
    let xf = Xform::new(graph, &trees);

    state.used_refs.entry(graph.bid).or_default();
    state.pwr_seq = 0;

    // One lib symbol per distinct (class, pin set, extent); per-instance
    // layout differences get their own variant, because the pin positions ARE
    // the geometry the wires were routed to.
    let (lib_of_box, _lib_bodies, with_power) = collect_layer_libs(graph, &trees, "");

    let sheet_uuid = if idx == 0 {
        set.root_uuid.clone()
    } else {
        det_uuid(&format!("{}#sch#{}", set.top, graph.bid))
    };
    // Instance-path prefix for items living on this sheet: the root uuid, or
    // root plus the uuid of the `sheet` element that opens this layer.
    let path_prefix = match set.sheet_uuid_of.get(&graph.bid) {
        Some(u) => format!("{}/{}", set.root_uuid, u),
        None => set.root_uuid.clone(),
    };

    let mut e = Emit::new();
    e.open("kicad_sch");
    line!(e, "(version {SCH_VERSION})");
    line!(e, "(generator \"mcc\")");
    line!(e, "(generator_version \"9.0\")");
    line!(e, "(uuid \"{sheet_uuid}\")");
    line!(e, "(paper {})", paper_for(graph, &trees));
    e.open("title_block");
    line!(e, "(title \"{}\")", escape(&graph.name));
    e.close();

    e.open("lib_symbols");
    let mut lib_bodies = BTreeMap::new();
    for b in component_boxes(graph) {
        let name = lib_of_box[&b.id].clone();
        lib_bodies
            .entry(name.clone())
            .or_insert_with(|| lib_symbol_body(b, &name));
    }
    for body in lib_bodies.values() {
        e.atom(body);
    }
    if with_power {
        e.atom(&lib_gnd_body());
        e.atom(&lib_pwr_body());
        e.atom(&lib_flag_body());
    }
    e.close();

    // Wires, junctions, terminals, labels.
    if is_device {
        emit_tree_nets(graph, &trees, &xf, &path_prefix, set, state, &mut e, &HashMap::new());
    } else {
        emit_block_edges(graph, &xf, set, &mut e);
        emit_root_passive_nets(graph, &xf, &mut Vec::new(), &mut e);
    }
    emit_rail_decorations(graph, &xf, &path_prefix, set, state, &mut e, &HashMap::new());

    for b in component_boxes(graph) {
        emit_symbol_instance(
            graph,
            b,
            &lib_of_box[&b.id],
            &xf,
            &path_prefix,
            set,
            state,
            &mut e,
        );
    }

    for b in &graph.boxes {
        if b.kind != BoxKind::SubModule || b.provenance != BoxProvenance::Declared {
            continue;
        }
        emit_sheet_instance(set, idx, graph, b, &xf, &mut e);
    }

    emit_no_connects(graph, &xf, &mut e);

    e.open("sheet_instances");
    e.open("path \"/\"");
    line!(e, "(page \"{}\")", set.page_of[&graph.bid]);
    e.close();
    e.close();
    line!(e, "(embedded_fonts no)");
    e.close();
    e.s
}

/// Declared component boxes — the only boxes that become KiCad symbol
/// instances. Synthesized boxes (rail flags, boundary labels) exist for the
/// drawing alone; sub-module boxes become sheets instead.
fn component_boxes(graph: &McVecGraph) -> Vec<&McVecBox> {
    graph
        .boxes
        .iter()
        .filter(|b| {
            b.provenance == BoxProvenance::Declared
                && matches!(b.kind, BoxKind::TwoPin | BoxKind::MultiPin)
        })
        .collect()
}

/// Per-layer lib symbols: one per distinct (class, pin set, extent), renamed
/// `"{prefix}{class}_vN"` when variants repeat. `prefix` namespaces the names
/// in the flat single-sheet mode, where two modules' `RES` layouts must not
/// collide. Returns the box->lib-name map, the deduped bodies, and whether the
/// layer draws any power glyph (those libs are shared, never prefixed).
fn collect_layer_libs(
    graph: &McVecGraph,
    trees: &[EquiTree],
    prefix: &str,
) -> (
    HashMap<i64, String>,
    BTreeMap<String, String>,
    bool,
) {
    let mut lib_of_box: HashMap<i64, String> = HashMap::new();
    let mut variant_count: HashMap<String, usize> = HashMap::new();
    for b in component_boxes(graph) {
        // One lib symbol per box, keyed by the box id: pin positions in the
        // lib must be THE positions this box's wires were routed to, and a
        // class-signature merge let a sibling box's fallback layout leak in.
        let base = sanitize_lib_id(&b.class_name);
        let n = variant_count.entry(base.clone()).or_insert(0);
        *n += 1;
        let name = if *n == 1 {
            base.clone()
        } else {
            format!("{base}_v{n}")
        };
        lib_of_box.insert(b.id, name);
    }
    let mut bodies = BTreeMap::new();
    for b in component_boxes(graph) {
        let name = lib_of_box[&b.id].clone();
        bodies
            .entry(name.clone())
            .or_insert_with(|| lib_symbol_body(b, &format!("{prefix}{name}")));
    }
    let with_power = has_power_symbols(graph, trees);
    (lib_of_box, bodies, with_power)
}

// === Flat single-sheet mode ===

/// The whole circuit on one sheet: every module's own device drawing is tiled
/// left to right on a shared canvas, and each module's boundary ports are
/// renamed to the net the PARENT's crossing carries. Same-name labels are what
/// KiCad joins a net by, so the modules become one circuit with zero long
/// cross-module wires — the connection lives in the net name, not in a drawn
/// line across the sheet. The hierarchical mode above stays the default; this
/// is the `--flat` face.
fn emit_flat_sheet(
    layers: &[RenderedLayer],
    table: &InstTable,
    top: &str,
) -> Vec<SchFile> {
    let mut by_bid_map: HashMap<i64, usize> = HashMap::new();
    for (i, l) in layers.iter().enumerate() {
        by_bid_map.insert(l.graph.bid, i);
    }
    let mut set = SheetSet {
        top: sanitize_file_stem(top),
        root_uuid: det_uuid(&format!("{top}#root")),
        layers,
        by_bid: by_bid_map,
        file_of: HashMap::new(),
        sheet_uuid_of: HashMap::new(),
        page_of: HashMap::new(),
        ports_of: HashMap::new(),
    };
    let mut state = SheetState {
        used_refs: HashMap::new(),
        pwr_seq: 0,
        flagged: HashSet::new(),
        labels: Vec::new(),
    };

    // Tile width pass: every layer that will be drawn gets an x offset. A
    // Block-style root is skipped unless it carries direct component boxes
    // (its sub-module boxes duplicate the child drawings below).
    let root_is_block_with_subs = layers[0].parent.is_none()
        && layers[0].graph.layer_style == LayerStyle::Block
        && layers[0].graph.boxes.iter().any(|b| b.kind == BoxKind::SubModule);
    let mut included: Vec<usize> = Vec::new();
    for (i, l) in layers.iter().enumerate() {
        let is_root = l.parent.is_none();
        let skip = is_root
            && root_is_block_with_subs
            && component_boxes(&l.graph).is_empty();
        if !skip {
            included.push(i);
        }
    }

    // Port -> parent-side net name, read off the parent's crossing: the
    // parent SubModule box's lead carries the net label the wire uses, which
    // is the name that makes the two drawings one circuit.
    let mut port_net: HashMap<i64, HashMap<String, String>> = HashMap::new();
    for (i, l) in layers.iter().enumerate() {
        let Some(parent_idx) = l
            .parent
            .and_then(|p| layers.iter().position(|l2| l2.graph.bid == p))
        else {
            continue;
        };
        let parent = &layers[parent_idx].graph;
        let mut m: HashMap<String, String> = HashMap::new();
        if let Some(sub) = parent
            .boxes
            .iter()
            .find(|b| b.kind == BoxKind::SubModule && b.name == l.graph.name)
        {
            for bp in &sub.boundary_ports {
                if let Some(ep) = sub.find_entry(bp.entry_pin_id) {
                    m.insert(bp.port_name.clone(), ep.pin_name.clone());
                }
            }
        }
        port_net.insert(layers[i].graph.bid, m);
    }

    // The flat table's copper islands are the ONE authority for "which
    // sub-module local net is which board net": every endpoint's full
    // instance path maps to the island it sits on, and the island's name is
    // what all flat labels must carry for KiCad to join the modules.
    let mut islands: HashMap<String, String> = HashMap::new();
    for (island, pts) in crate::export::netlist::island_nets(table, crate::export::netlist::PointNaming::Hierarchical) {
        for p in pts {
            islands.insert(p, island.clone());
        }
    }

    // Phase 1 - measure every tile at origin and collect the shared
    // lib_symbols. A tile is one module's device drawing (or the root's own
    // component cluster).
    let mut libs: BTreeMap<String, String> = BTreeMap::new();
    let mut with_power = false;
    struct FlatTile {
        idx: usize,
        w: f64,
        h: f64,
        x: f64,
        y: f64,
        lib_of_box: HashMap<i64, String>,
    }
    let mut tiles: Vec<FlatTile> = Vec::new();
    for &i in &included {
        let layer = &layers[i];
        let is_device = layer.graph.layer_style == LayerStyle::Device;
        let trees: Vec<EquiTree> = if is_device {
            build_all_trees(&layer.graph)
        } else {
            Vec::new()
        };
        let xf0 = Xform::new(&layer.graph, &trees);
        let prefix = format!("L{}_", layer.graph.bid);
        let (lib_of_box, bodies, pw) = collect_layer_libs(&layer.graph, &trees, &prefix);
        for (n, b) in bodies {
            libs.insert(format!("{prefix}{n}"), b);
        }
        with_power |= pw;
        tiles.push(FlatTile {
            idx: i,
            w: xf0.max_x,
            h: xf0.max_y,
            x: 0.0,
            y: 0.0,
            lib_of_box,
        });
    }

    // Phase 2 - seed each tile where its module sits in the main-mode block
    // diagram. That diagram is laid out centre-outward, so the flat sheet
    // inherits the arrangement an engineer already reads: modules right of
    // centre stay right, above stay above - all four directions in use, and
    // no strip that would invite long wires.
    let root_graph = &layers[0].graph;
    let root_xf = Xform::new(root_graph, &[]);
    let mut rmin = (f64::MAX, f64::MAX);
    let mut rmax = (f64::MIN, f64::MIN);
    for b in &root_graph.boxes {
        rmin.0 = rmin.0.min(b.x);
        rmin.1 = rmin.1.min(b.y);
        rmax.0 = rmax.0.max(b.x + b.w);
        rmax.1 = rmax.1.max(b.y + b.h);
    }
    let root_center = if rmin.0.is_finite() {
        (
            root_xf.x((rmin.0 + rmax.0) / 2.0),
            root_xf.y((rmin.1 + rmax.1) / 2.0),
        )
    } else {
        (0.0, 0.0)
    };
    for t in &mut tiles {
        let layer = &layers[t.idx];
        let anchor = if layer.parent.is_none() {
            root_center
        } else {
            root_graph
                .boxes
                .iter()
                .find(|b| b.kind == BoxKind::SubModule && b.name == layer.graph.name)
                .map(|b| (root_xf.x(b.x + b.w / 2.0), root_xf.y(b.y + b.h / 2.0)))
                .unwrap_or(root_center)
        };
        t.x = anchor.0 - t.w / 2.0;
        t.y = anchor.1 - t.h / 2.0;
    }

    // Phase 3 - device drawings are far larger than their block boxes, so
    // seeded tiles overlap. Separate them outward: of an overlapping pair,
    // the tile farther from the root centre yields along the thinner axis.
    // Bounded sweeps; the arrangement still reads as the block diagram's.
    const FLAT_GAP_MM: f64 = 15.0;
    for _ in 0..200 {
        let mut moved = 0.0f64;
        for i in 0..tiles.len() {
            for j in i + 1..tiles.len() {
                let (ox, oy) = {
                    let (a, b) = (&tiles[i], &tiles[j]);
                    let pen_x = a.x.min(b.x) + a.w.max(b.w) + FLAT_GAP_MM
                        - a.x.max(b.x);
                    let pen_y = a.y.min(b.y) + a.h.max(b.h) + FLAT_GAP_MM
                        - a.y.max(b.y);
                    (pen_x, pen_y)
                };
                let (a, b) = (&tiles[i], &tiles[j]);
                let overlap_x = a.x < b.x + b.w + FLAT_GAP_MM
                    && b.x < a.x + a.w + FLAT_GAP_MM;
                let overlap_y = a.y < b.y + b.h + FLAT_GAP_MM
                    && b.y < a.y + a.h + FLAT_GAP_MM;
                if !(overlap_x && overlap_y) {
                    continue;
                }
                let da = (a.x + a.w / 2.0 - root_center.0).powi(2)
                    + (a.y + a.h / 2.0 - root_center.1).powi(2);
                let db = (b.x + b.w / 2.0 - root_center.0).powi(2)
                    + (b.y + b.h / 2.0 - root_center.1).powi(2);
                // Push the tile farther from the root centre ALONG ITS OWN
                // BEARING RAY (the direction from the centre through the tile,
                // quantized to the dominant axis of that ray). A module seeded
                // right of centre retreats right, above stays above — the
                // block diagram's quadrant survives every push.
                let (push_idx, ox, oy) = if da >= db { (i, ox, oy) } else { (j, ox, oy) };
                let t = &mut tiles[push_idx];
                let (cx, cy) = (t.x + t.w / 2.0, t.y + t.h / 2.0);
                let (rx, ry) = (cx - root_center.0, cy - root_center.1);
                if rx.abs() >= ry.abs() {
                    t.x += if rx >= 0.0 { ox } else { -ox };
                } else {
                    t.y += if ry >= 0.0 { oy } else { -oy };
                }
                moved += 1.0;
            }
        }
        if moved == 0.0 {
            break;
        }
    }
    // Compact: the seeded positions inherit the block diagram's spacing,
    // far looser than a sheet of drawings needs - pull every tile toward the
    // root centre while the step keeps the gap, closest first.
    for round in 0..300 {
        let step = if round < 100 { 8.0 } else if round < 200 { 4.0 } else { 2.0 };
        let mut moved = 0usize;
        for i in 0..tiles.len() {
            let (tx, ty) = (tiles[i].x + tiles[i].w / 2.0, tiles[i].y + tiles[i].h / 2.0);
            let (dx, dy) = (root_center.0 - tx, root_center.1 - ty);
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1.0 {
                continue;
            }
            let (nx, ny) = (tiles[i].x + dx / len * step, tiles[i].y + dy / len * step);
            let (nx, ny) = (tiles[i].x + dx / len * step, tiles[i].y + dy / len * step);
            let conflict = (0..tiles.len()).any(|j| {
                j != i
                    && nx < tiles[j].x + tiles[j].w + FLAT_GAP_MM
                    && tiles[j].x < nx + tiles[i].w + FLAT_GAP_MM
                    && ny < tiles[j].y + tiles[j].h + FLAT_GAP_MM
                    && tiles[j].y < ny + tiles[i].h + FLAT_GAP_MM
            });
            if !conflict {
                tiles[i].x = nx;
                tiles[i].y = ny;
                moved += 1;
            }
        }
        if moved == 0 {
            break;
        }
    }
    // Fit: shift everything so the sheet starts at one margin.
    let min_x = tiles.iter().map(|t| t.x).fold(f64::MAX, f64::min);
    let min_y = tiles.iter().map(|t| t.y).fold(f64::MAX, f64::min);
    let (shift_x, shift_y) = if min_x.is_finite() {
        (30.0 - min_x, 30.0 - min_y)
    } else {
        (0.0, 0.0)
    };
    for t in &mut tiles {
        t.x += shift_x;
        t.y += shift_y;
    }

    // Phase 4 - final transforms and emit.
    let mut tiling: Vec<(usize, Xform, HashMap<i64, String>)> = Vec::new();
    for t in &tiles {
        let layer = &layers[t.idx];
        let is_device = layer.graph.layer_style == LayerStyle::Device;
        let trees: Vec<EquiTree> = if is_device {
            build_all_trees(&layer.graph)
        } else {
            Vec::new()
        };
        let xf = Xform::with_offset(&layer.graph, &trees, t.x, t.y);
        tiling.push((t.idx, xf, t.lib_of_box.clone()));
    }

    // Phase 4.5 - route the block diagram's inter-module edges between the
    // expanded regions. Four orthogonal candidates per run; the first that
    // crosses no foreign tile wins. A run no candidate can route joins the
    // corner legend instead - the sheet then covers every connection either
    // as copper or as text, never silently drops one.
    struct FlatRun {
        pts: Vec<(f64, f64)>,
        net: String,
    }
    // Ground glyphs deferred to the draw phase (sheet mm + port tag).
    let mut gnd_glyphs: Vec<(f64, f64, String)> = Vec::new();
    let mut runs: Vec<FlatRun> = Vec::new();
    let mut unrouted: Vec<(String, String, String)> = Vec::new();
    let rects: Vec<(f64, f64, f64, f64)> =
        tiles.iter().map(|t| (t.x, t.y, t.w, t.h)).collect();
    // Component bodies are obstacles too: a corridor run that crosses a part
    // reads as a connection to it. Ends sit on pin stubs OUTSIDE bodies, so
    // no endpoint exemption is needed.
    let parts: Vec<(f64, f64, f64, f64)> = {
        let mut v = Vec::new();
        for (i, xf, _) in &tiling {
            let g = &layers[*i].graph;
            for b in component_boxes(g) {
                v.push((xf.x(b.x), xf.y(b.y), b.w * MM_PER_PX, b.h * MM_PER_PX));
            }
            if let Some(f) = &g.module_frame {
                v.push((xf.x(f.x), xf.y(f.y), f.w * MM_PER_PX, f.h * MM_PER_PX));
            }
        }
        v
    };
    if std::env::var("MCC_KSCH_DEBUG").is_ok() {
        for n in &root_graph.nets {
            for ep in &n.endpoints {
                match root_graph.boxes.iter().find(|b| b.id == ep.box_id) {
                    None => eprintln!("[ksch] net {} ep box_id={} NOT FOUND", n.name, ep.box_id),
                    Some(b) => {
                        if !b.pins.iter().any(|p| p.id == ep.pin_id) && !b.pins.iter().any(|p| {
                            ep.pin_number.map(|n| p.pin_id == n.to_string()) == Some(true)
                        }) {
                            eprintln!(
                                "[ksch] net {} ep {} pin_id={} MISSING in box pins {:?}",
                                n.name, b.name, ep.pin_id,
                                b.pins.iter().map(|p| (p.id, p.pin_id.as_str())).take(9).collect::<Vec<_>>()
                            );
                        }
                    }
                }
            }
        }
    }
    let mut port_at: HashMap<(usize, String), (f64, f64)> = HashMap::new();
    for (i, xf, _) in &tiling {
        let graph = &layers[*i].graph;
        if graph.layer_style != LayerStyle::Device {
            continue;
        }
        let trees = build_all_trees(graph);
        for n in &graph.nets {
            let Some(bi) = &n.boundary else { continue };
            let Some(at) = flat_port_anchor(graph, &trees, xf, &bi.port_name) else {
                continue;
            };
            port_at.insert((*i, bi.port_name.clone()), at);
            port_at.insert((*i, n.name.clone()), at);
        }
    }
    let root_tile = tiling
        .iter()
        .position(|(i, _, _)| layers[*i].parent.is_none());
    for net in &root_graph.nets {
        if net.kind == crate::vector::graph::NetKind::Ground {
            // Ground runs are not drawn; one GND glyph per ground net names
            // the copper globally, which is what the drawn line would do.
            let Some((li, port)) = net.endpoints.iter().find_map(|ep| {
                let b = root_graph.boxes.iter().find(|b| b.id == ep.box_id)?;
                if b.kind != BoxKind::SubModule {
                    return None;
                }
                let port = b
                    .boundary_ports
                    .iter()
                    .find(|p| p.entry_pin_id == ep.pin_id)
                    .map(|p| p.port_name.clone())?;
                let child_bid = root_graph.clickable_subs.iter().copied().find(|bid| *bid == b.id)?;
                Some((child_bid, port))
            }) else {
                continue;
            };
            let (li, port) = (li, port.clone());
            let Some(&ci) = set.by_bid.get(&li) else { continue };
            let cg = &layers[ci].graph;
            let trees = build_all_trees(cg);
            let Some(xf) = tiling
                .iter()
                .find(|(i, _, _)| *i == ci)
                .map(|(_, xf, _)| xf)
            else {
                continue;
            };
            if let Some((x, y)) = flat_port_anchor(cg, &trees, xf, &port) {
                gnd_glyphs.push((x, y, port));
            }
            continue;
        }
        let mut anchors: Vec<((f64, f64), String, Option<usize>)> = Vec::new();
        for ep in &net.endpoints {
            let Some(b) = root_graph.boxes.iter().find(|b| b.id == ep.box_id) else {
                continue;
            };
            if b.provenance != BoxProvenance::Declared {
                continue;
            }
            match b.kind {
                BoxKind::SubModule => {
                    let Some(&li) = set.by_bid.get(&b.id) else { continue };
                    let port = b
                        .boundary_ports
                        .iter()
                        .find(|p| p.entry_pin_id == ep.pin_id)
                        .map(|p| p.port_name.clone())
                        .or_else(|| b.find_entry(ep.pin_id).map(|x| x.pin_name.clone()))
                        .unwrap_or_default();
                    let Some(at) = port_at.get(&(li, port.clone())) else { continue };
                    anchors.push((*at, format!("{}.{}", b.name, port), Some(li)));
                }
                BoxKind::TwoPin | BoxKind::MultiPin => {
                    let Some(ri) = root_tile else { continue };
                    let Some(pin) = endpoint_pin(b, ep) else {
                        continue;
                    };
                    let xf = &tiling[ri].1;
                    let (side, offset) = pin_placement(b, pin);
                    let (ax, ay) = anchor_mm(xf, b, side, offset);
                    let (dx, dy) = match side {
                        EntrySide::Left => (-10.0 * MM_PER_PX, 0.0),
                        EntrySide::Right => (10.0 * MM_PER_PX, 0.0),
                        EntrySide::Top => (0.0, -10.0 * MM_PER_PX),
                        EntrySide::Bottom => (0.0, 10.0 * MM_PER_PX),
                    };
                    anchors.push((
                        (ax + dx, ay + dy),
                        format!("{}.{}", b.display_label(), pin.pin_id),
                        Some(ri),
                    ));
                }
                _ => {}
            }
        }
        let mut uniq: Vec<((f64, f64), String, Option<usize>)> = Vec::new();
        for a in anchors {
            if !uniq
                .iter()
                .any(|u| (u.0.0 - a.0.0).abs() < 0.05 && (u.0.1 - a.0.1).abs() < 0.05)
            {
                uniq.push(a);
            }
        }
        if uniq.len() < 2 {
            continue;
        }
        let mut chain: Vec<((f64, f64), String, Option<usize>)> = vec![uniq[0].clone()];
        let mut left: Vec<((f64, f64), String, Option<usize>)> = uniq[1..].to_vec();
        while !left.is_empty() {
            let last = chain.last().unwrap().0;
            let best = left
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    let da = (a.0.0 - last.0).powi(2) + (a.0.1 - last.1).powi(2);
                    let db = (b.0.0 - last.0).powi(2) + (b.0.1 - last.1).powi(2);
                    da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(i, _)| i)
                .unwrap_or(0);
            chain.push(left.remove(best));
        }
        let exits: Vec<((f64, f64), String, Option<usize>)> = chain
            .into_iter()
            .map(|(at, desc, tile)| {
                let at = tile
                    .and_then(|ti| tiles.iter().find(|t| t.idx == ti))
                    .map(|t| {
                        let (dx, dy) = (at.0 - (t.x + t.w / 2.0), at.1 - (t.y + t.h / 2.0));
                        let len = (dx * dx + dy * dy).sqrt().max(0.001);
                        (at.0 + dx / len * 3.0, at.1 + dy / len * 3.0)
                    })
                    .unwrap_or(at);
                (at, desc, tile)
            })
            .collect();
        for w in exits.windows(2) {
            let (p1, d1, t1) = (&w[0].0, &w[0].1, &w[0].2);
            let (p2, d2, t2) = (&w[1].0, &w[1].1, &w[1].2);
            // Same child tile: the child's trees already drew that net. Same
            // ROOT tile: nothing else ever draws root-internal nets, so the
            // run is the only copper those parts get.
            if t1 == t2 && *t1 != root_tile {
                continue;
            }
            let mut skip: Vec<usize> = Vec::new();
            if let Some(i) = t1 {
                skip.push(*i);
            }
            if let Some(i) = t2 {
                skip.push(*i);
            }
            let (x1, y1) = *p1;
            let (x2, y2) = *p2;
            let (xm, ym) = ((x1 + x2) / 2.0, (y1 + y2) / 2.0);
            let cand = [
                vec![(x1, y1), (xm, y1), (xm, y2), (x2, y2)],
                vec![(x1, y1), (x1, ym), (x2, ym), (x2, y2)],
                vec![(x1, y1), (x2, y1), (x2, y2)],
                vec![(x1, y1), (x1, y2), (x2, y2)],
            ];
            let hits = |pts: &Vec<(f64, f64)>| {
                pts.windows(2).any(|s| {
                    let (a, b) = (s[0], s[1]);
                    let crossed = |r: &(f64, f64, f64, f64)| {
                        if (a.1 - b.1).abs() < 0.01 {
                            let (lo, hi) = if a.0 <= b.0 { (a.0, b.0) } else { (b.0, a.0) };
                            a.1 > r.1 && a.1 < r.1 + r.3 && hi > r.0 && lo < r.0 + r.2
                        } else {
                            let (lo, hi) = if a.1 <= b.1 { (a.1, b.1) } else { (b.1, a.1) };
                            a.0 > r.0 && a.0 < r.0 + r.2 && hi > r.1 && lo < r.1 + r.3
                        }
                    };
                    rects
                        .iter()
                        .enumerate()
                        .any(|(i, r)| !skip.contains(&i) && crossed(r))
                        || parts.iter().any(|r| crossed(r))
                })
            };
            match cand.iter().find(|c| !hits(c)) {
                Some(pts) => runs.push(FlatRun {
                    pts: pts.clone(),
                    net: net.name.clone(),
                }),
                None => unrouted.push((net.name.clone(), d1.clone(), d2.clone())),
            }
        }
    }
    let legend_w = if unrouted.is_empty() { 0.0 } else { 90.0 };

    let mut e = Emit::new();
    e.open("kicad_sch");
    line!(e, "(version {SCH_VERSION})");
    line!(e, "(generator \"mcc\")");
    line!(e, "(generator_version \"9.0\")");
    line!(e, "(uuid \"{}\")", set.root_uuid);
    let sheet_w = tiles
        .iter()
        .map(|t| t.x + t.w)
        .fold(297.0f64, f64::max)
        + legend_w;
    let sheet_h = tiles
        .iter()
        .map(|t| t.y + t.h)
        .fold(210.0f64, f64::max);
    line!(e, "(paper \"User\" {} {})", sheet_w.ceil() as i64 + 20, sheet_h.ceil() as i64 + 20);
    e.open("title_block");
    line!(e, "(title \"{}\")", escape(top));
    e.close();

    e.open("lib_symbols");
    for (n, b) in &libs {
        e.atom(b);
        let _ = n;
    }
    if with_power {
        e.atom(&lib_gnd_body());
        e.atom(&lib_pwr_body());
        e.atom(&lib_flag_body());
    }
    e.close();

    // Root rail names by declared voltage — the rename authority for every
    // sub-module's local arm of a shared rail.
    // Cross-layer block edges whose parent-side part is unwired in the root
    // graph (a pull-up inside a child's scope reaching a root part): the run
    // from the part's stub to the child tile's port anchor is the only
    // copper that connection gets, named by the child net's board name.
    {
        let by_id: HashMap<i64, &McVecBox> =
            root_graph.boxes.iter().map(|b| (b.id, b)).collect();
        for edge in &root_graph.block_edges {
            let Some(pin_id) = edge.from_pins.first().or(edge.to_pins.first()) else {
                continue;
            };
            let part_box = edge.from_box;
            let Some(pb) = by_id.get(&part_box) else { continue };
            if !matches!(pb.kind, BoxKind::TwoPin | BoxKind::MultiPin) {
                continue;
            }
            let wired = root_graph.nets.iter().any(|n| {
                n.endpoints
                    .iter()
                    .any(|ep| ep.box_id == part_box && ep.pin_id == *pin_id)
            });
            if wired {
                continue;
            }
            let Some(pin) = pb.pins.iter().find(|p| p.id == *pin_id) else {
                continue;
            };
            let (side, offset) = pin_placement(pb, pin);
            let (ax, ay) = anchor_mm(&root_xf, pb, side, offset);
            let (dx, dy) = match side {
                EntrySide::Left => (-10.0 * MM_PER_PX, 0.0),
                EntrySide::Right => (10.0 * MM_PER_PX, 0.0),
                EntrySide::Top => (0.0, -10.0 * MM_PER_PX),
                EntrySide::Bottom => (0.0, 10.0 * MM_PER_PX),
            };
            // The far end: the child tile that owns the edge's other box.
            let other_box = if edge.from_box == part_box {
                edge.to_box
            } else {
                edge.from_box
            };
            let other_pins = if edge.from_box == part_box {
                &edge.to_pins
            } else {
                &edge.from_pins
            };
            let Some(&oi) = set.by_bid.get(&other_box) else { continue };
            let far = other_pins.iter().find_map(|pid| {
                let ob = root_graph.boxes.iter().find(|b| b.id == other_box)?;
                let port = ob
                    .boundary_ports
                    .iter()
                    .find(|p| p.entry_pin_id == *pid)
                    .map(|p| p.port_name.clone())
                    .or_else(|| ob.find_entry(*pid).map(|x| x.pin_name.clone()))?;
                port_at.get(&(oi, port)).copied()
            });
            let Some((fx, fy)) = far else { continue };
            let (sx, sy) = (ax + dx, ay + dy);
            flat_wire(root_graph.bid, ax, ay, sx, sy, &mut e);
            flat_wire(root_graph.bid, sx, sy, fx, fy, &mut e);
            if !is_anon(&edge.label) {
                text_label(
                    root_graph.bid,
                    &edge.label,
                    (sx + fx) / 2.0,
                    (sy + fy) / 2.0,
                    Slide::X,
                    &mut state.labels,
                    &mut e,
                );
            }
        }
    }

    // Phase 5 - draw the routed runs with their board-level names, then the
    // corner legend for whatever routing could not place.
    for run in &runs {
        for sg in run.pts.windows(2) {
            flat_wire(root_graph.bid, sg[0].0, sg[0].1, sg[1].0, sg[1].1, &mut e);
        }
        let a = run.pts[0];
        let b = *run.pts.last().unwrap();
        text_label(root_graph.bid, &run.net, (a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0, Slide::X, &mut state.labels, &mut e);
    }
    for (x, y, port) in &gnd_glyphs {
        state.pwr_seq += 1;
        let seed = format!("{}#gnd#{}", root_graph.bid, port);
        e.open("symbol");
        line!(e, "(lib_id \"mcc:GND\")");
        line!(e, "(at {} {} 0)", mm(*x), mm(*y));
        line!(e, "(unit 1)");
        line!(e, "(exclude_from_sim no)");
        line!(e, "(in_bom yes)");
        line!(e, "(on_board yes)");
        line!(e, "(dnp no)");
        line!(e, "(uuid \"{}\")", det_uuid(&seed));
        property(&mut e, "Reference", &format!("#PWR{}", state.pwr_seq), x - 5.08, y - 2.54, true);
        property(&mut e, "Value", "GND", x - 5.08, y + 2.54, false);
        property(&mut e, "Footprint", "", *x, *y, true);
        e.open("(pin \"1\"");
        line!(e, "(uuid \"{}\")", det_uuid(&format!("{seed}#pin")));
        e.close();
        emit_instances(&mut e, &set.root_uuid, &set.top, &format!("#PWR{}", state.pwr_seq));
        e.close();
    }
    if !unrouted.is_empty() {
        let lx = tiles.iter().map(|t| t.x + t.w).fold(0.0f64, f64::max) + 15.0;
        let mut ly = 30.0;
        text_label(root_graph.bid, "Unrouted connections", lx, ly, Slide::None, &mut state.labels, &mut e);
        ly += 6.0;
        for (n, a, b) in &unrouted {
            text_label(root_graph.bid, &format!("{n}: {a} - {b}"), lx, ly, Slide::None, &mut state.labels, &mut e);
            ly += 5.0;
        }
    }

    for (i, xf, lib_of_box) in &tiling {
        let layer = &layers[*i];
        let graph = &layer.graph;
        let is_device = graph.layer_style == LayerStyle::Device;
        let pn = port_net.get(&graph.bid).cloned().unwrap_or_default();
        state.used_refs.entry(graph.bid).or_default();
        if is_device {
            let trees: Vec<EquiTree> = build_all_trees(graph);
            let net_names: HashMap<String, String> = graph
                .nets
                .iter()
                .filter_map(|n| {
                    island_name_of_net(graph, n, &islands).map(|i| (n.name.clone(), i))
                })
                .collect();
            emit_tree_nets(graph, &trees, xf, &set.root_uuid, &set, &mut state, &mut e, &net_names);
            emit_wired_pin_guarantee(graph, &trees, &islands, &net_names, xf, &mut state.labels, &mut e);
            // Boundary ports become same-name labels joined to the parent's
            // net name; the hierarchical label itself would be meaningless on
            // a sheetless drawing.
            emit_flat_boundary_labels(graph, &trees, xf, &pn, &mut state.labels, &mut e);
        } else {
            emit_root_passive_nets(graph, xf, &mut state.labels, &mut e);
        }
        let net_names: HashMap<String, String> = graph
            .nets
            .iter()
            .filter_map(|n| {
                island_name_of_net(graph, n, &islands).map(|i| (n.name.clone(), i))
            })
            .collect();
        emit_rail_decorations(graph, xf, &set.root_uuid, &set, &mut state, &mut e, &net_names);
        for b in component_boxes(graph) {
            emit_symbol_instance(
                graph,
                b,
                &format!("L{}_{}", graph.bid, lib_of_box[&b.id]),
                xf,
                &set.root_uuid,
                &set,
                &mut state,
                &mut e,
            );
        }
        emit_no_connects(graph, xf, &mut e);
    }

    e.open("sheet_instances");
    e.open("path \"/\"");
    line!(e, "(page \"1\")");
    e.close();
    e.close();
    line!(e, "(embedded_fonts no)");
    e.close();
    vec![SchFile {
        name: format!("{}.kicad_sch", set.top),
        content: e.s,
    }]
}

/// Flat-mode boundary labels: the same anchors as the hierarchical labels,
/// but plain labels named by the PARENT-side net (falling back to the port
/// name when the parent never drew the crossing).
fn emit_flat_boundary_labels(
    graph: &McVecGraph,
    trees: &[EquiTree],
    xf: &Xform,
    port_net: &HashMap<String, String>,
    ledger: &mut Vec<(f64, f64, f64, f64)>,
    e: &mut Emit,
) {
    for net in &graph.nets {
        let Some(bi) = &net.boundary else { continue };
        // An anon parent crossing would print `_netN` — meaningless on a
        // drawing. The port's own name is the honest fallback.
        let name = port_net
            .get(&bi.port_name)
            .map(|n| if is_anon(n) { bi.port_name.clone() } else { n.clone() })
            .unwrap_or_else(|| bi.port_name.clone());
        if is_anon(&name) {
            continue;
        }
        let terminal = graph.boxes.iter().find(|b| {
            b.kind == BoxKind::PortTerminal
                && b.boundary_ports.iter().any(|p| p.port_name == bi.port_name)
        });
        let pos = terminal.and_then(|b| {
            b.entry_points
                .first()
                .map(|ep| anchor_mm(xf, b, ep.side, ep.offset))
        });
        let Some((x, y)) = pos.or_else(|| boundary_tree_endpoint(graph, trees, xf, net)) else {
            continue;
        };
        text_label(graph.bid, &name, x, y, Slide::X, ledger, e);
    }
}

// === Nets ===

#[allow(clippy::too_many_arguments)]
fn emit_tree_nets(
    graph: &McVecGraph,
    trees: &[EquiTree],
    xf: &Xform,
    path: &str,
    set: &SheetSet,
    state: &mut SheetState,
    e: &mut Emit,
    net_names: &HashMap<String, String>,
) {
    let _ = set;
    let display_of = |name: &str| -> String {
        net_names.get(name).cloned().unwrap_or_else(|| name.to_string())
    };
    let boundary = boundary_net_ids(graph);
    let nid_of_name: HashMap<&str, i64> = graph
        .nets
        .iter()
        .map(|n| (n.name.as_str(), n.nid))
        .collect();
    for t in trees {
        let nid = nid_of_name.get(t.net_name.as_str()).copied();
        let is_boundary = nid.map(|n| boundary.contains(&n)).unwrap_or(false);
        for seg in &t.segments {
            emit_wire(graph.bid, xf, seg.x1, seg.y1, seg.x2, seg.y2, e);
        }
        bridge_pins(graph, t, xf, e);
        for (x, y) in &t.junction_dots {
            e.open("junction");
            line!(e, "(at {} {})", mm(xf.x(*x)), mm(xf.y(*y)));
            line!(e, "(diameter 0)");
            line!(e, "(color 0 0 0 0)");
            line!(
                e,
                "(uuid \"{}\")",
                det_uuid(&format!("{}#tjn#{}#{}#{}", graph.bid, t.net_name, x, y))
            );
            e.close();
        }
        let mut has_text_symbol = false;
        let flat_mode = !net_names.is_empty();
        for s in &t.symbols {
            match s.kind {
                TreeSymbolKind::Ground => {
                    power_symbol(
                        graph,
                        &t.net_name,
                        "GND",
                        true,
                        s.x,
                        s.y,
                        xf,
                        path,
                        &set.top,
                        state,
                        e,
                    );
                    power_flag(
                        graph,
                        &t.net_name,
                        "GND",
                        s.x,
                        s.y,
                        xf,
                        path,
                        &set.top,
                        state,
                        e,
                    );
                }
                TreeSymbolKind::Power => {
                    power_symbol(
                        graph,
                        &t.net_name,
                        "PWR",
                        false,
                        s.x,
                        s.y,
                        xf,
                        path,
                        &set.top,
                        state,
                        e,
                    );
                    power_flag(
                        graph,
                        &t.net_name,
                        "PWR",
                        s.x,
                        s.y,
                        xf,
                        path,
                        &set.top,
                        state,
                        e,
                    );
                }
                TreeSymbolKind::NetLabel | TreeSymbolKind::BusLabel | TreeSymbolKind::PortLabel => {
                    has_text_symbol = true;
                    // Hierarchical mode: a boundary net names itself by its
                    // port through the hierarchical label, so a second label
                    // would only fight it. Flat mode: the renamed label IS
                    // the cross-module join, boundary or not.
                    if !is_anon(&t.net_name) && (!is_boundary || flat_mode) {
                        let shown = display_of(&t.net_name);
                        text_label(graph.bid, &shown, xf.x(s.x), xf.y(s.y), Slide::X, &mut state.labels, e);
                    }
                }
            }
        }
        // Named nets with no drawn label still get their name on the wire, so
        // KiCad's netlist reads the same net names mcc's does. In the flat
        // sheet the label is unconditional — the net-name rename (to the
        // copper island's board-wide name) only joins the modules if it lands
        // on copper, whatever the tree's own terminal style is.
        if (!has_text_symbol || flat_mode) && !is_anon(&t.net_name) && (!is_boundary || flat_mode) {
            if let Some((x, y)) =
                longest_midpoint(t.segments.iter().map(|s| ((s.x1, s.y1), (s.x2, s.y2))))
            {
                let shown = display_of(&t.net_name);
                text_label(graph.bid, &shown, xf.x(x), xf.y(y), Slide::X, &mut state.labels, e);
            }
        }
        if let Some(net) = graph.nets.iter().find(|n| n.name == t.net_name) {
            let shown = display_of(&t.net_name);
            emit_pin_rescue(graph, t, net, xf, &shown, &mut state.labels, e);
        }
    }
    emit_boundary_labels(graph, trees, xf, e);
}

/// The nets of this layer that cross its own boundary (a child sheet's ports).
fn boundary_net_ids(graph: &McVecGraph) -> HashSet<i64> {
    graph
        .nets
        .iter()
        .filter(|n| n.boundary.is_some())
        .map(|n| n.nid)
        .collect()
}

fn longest_midpoint(segs: impl Iterator<Item = ((f64, f64), (f64, f64))>) -> Option<(f64, f64)> {
    let mut best: Option<(f64, (f64, f64))> = None;
    for ((x1, y1), (x2, y2)) in segs {
        let len = (x2 - x1).abs() + (y2 - y1).abs();
        if len > 0.01 && best.map(|(l, _)| len > l).unwrap_or(true) {
            best = Some((len, ((x1 + x2) / 2.0, (y1 + y2) / 2.0)));
        }
    }
    best.map(|(_, p)| p)
}

fn emit_wire(bid: i64, xf: &Xform, x1: f64, y1: f64, x2: f64, y2: f64, e: &mut Emit) {
    if (x1 - x2).abs() < 0.01 && (y1 - y2).abs() < 0.01 {
        return;
    }
    e.open("wire");
    e.open("pts");
    line!(e, "(xy {} {})", mm(xf.x(x1)), mm(xf.y(y1)));
    line!(e, "(xy {} {})", mm(xf.x(x2)), mm(xf.y(y2)));
    e.close();
    e.open("stroke");
    line!(e, "(width 0)");
    line!(e, "(type default)");
    e.close();
    line!(
        e,
        "(uuid \"{}\")",
        det_uuid(&format!("{bid}#w#{x1}#{y1}#{x2}#{y2}"))
    );
    e.close();
}

/// Which way a label may slide while staying electrically connected: along
/// the wire it sits on. Labels whose anchor is a short vertical stub register
/// as `None` — they never slide, and later labels route around them.
#[derive(Clone, Copy, PartialEq)]
enum Slide {
    X,
    Y,
    None,
}

fn text_label(
    bid: i64,
    text: &str,
    x: f64,
    y: f64,
    slide: Slide,
    ledger: &mut Vec<(f64, f64, f64, f64)>,
    e: &mut Emit,
) {
    // Anti-overlap: slide ALONG the wire first; if every candidate collides,
    // drop the font one notch and try the same run again (dense sheets read a
    // smaller label better than an overlapping one).
    let offs: Vec<(f64, f64)> = match slide {
        Slide::X => vec![
            (12.0, 0.0),
            (-12.0, 0.0),
            (24.0, 0.0),
            (-24.0, 0.0),
            (36.0, 0.0),
            (-36.0, 0.0),
            (48.0, 0.0),
            (-48.0, 0.0),
        ],
        Slide::Y => vec![
            (0.0, 3.0),
            (0.0, -3.0),
            (0.0, 6.0),
            (0.0, -6.0),
            (0.0, 9.0),
            (0.0, -9.0),
            (0.0, 12.0),
            (0.0, -12.0),
        ],
        Slide::None => vec![],
    };
    let mut font = 1.27;
    let mut fx = x;
    let mut fy = y;
    for round in 0..2 {
        let w = text.len() as f64 * (font * 0.71) + 2.0;
        let h = font + 0.8;
        let hits = |bx: f64, by: f64| {
            ledger.iter().any(|(ox, oy, ow, oh)| {
                bx < *ox + *ow && *ox < bx + w && by < *oy + *oh && *oy < by + h
            })
        };
        if !hits(x, y) {
            break;
        }
        let mut moved = false;
        for (dx, dy) in &offs {
            if !hits(x + dx, y + dy) {
                fx = x + dx;
                fy = y + dy;
                moved = true;
                break;
            }
        }
        if moved {
            break;
        }
        if round == 0 {
            font = 1.0;
        } else {
            fx = x;
            fy = y;
        }
    }
    let w = text.len() as f64 * (font * 0.71) + 2.0;
    ledger.push((fx, fy, w, font + 0.8));
    e.open(&format!("label \"{}\"", escape(text)));
    line!(e, "(at {} {} 0)", mm(fx), mm(fy));
    e.open("effects");
    e.open("font");
    line!(e, "(size {font} {font})");
    e.close();
    line!(e, "(justify left bottom)");
    e.close();
    line!(
        e,
        "(uuid \"{}\")",
        det_uuid(&format!("{bid}#lbl#{text}#{x}#{y}"))
    );
    e.close();
}

// === Block-diagram edges ===

/// The root block diagram draws merged edges (`block_edges`), not routed
/// nets — the renderer's own choice, mirrored here. Each edge becomes a
/// Z-shaped orthogonal wire between the two endpoint anchors; same-name
/// labels (emitted on every named edge) are what hold a multi-sheet net
/// together, since a merged edge no longer carries the per-net multiplicity.
fn emit_block_edges(graph: &McVecGraph, xf: &Xform, set: &SheetSet, e: &mut Emit) {
    let by_id: HashMap<i64, &McVecBox> = graph.boxes.iter().map(|b| (b.id, b)).collect();
    let pin_anchor = |g: &McVecGraph,
                      by_id: &HashMap<i64, &McVecBox>,
                      box_id: i64,
                      pin_id: i64|
     -> Option<(f64, f64)> {
        let b = by_id.get(&box_id)?;
        match b.find_entry(pin_id) {
            Some(a) => Some(anchor_px(b, a)),
            None => {
                let pin = g
                    .boxes
                    .iter()
                    .find(|bb| bb.id == box_id)?
                    .pins
                    .iter()
                    .find(|p| p.id == pin_id)?;
                let (side, offset) = pin_placement(b, pin);
                Some(match side {
                    EntrySide::Left => (b.x, b.y + b.h * offset),
                    EntrySide::Right => (b.x + b.w, b.y + b.h * offset),
                    EntrySide::Top => (b.x + b.w * offset, b.y),
                    EntrySide::Bottom => (b.x + b.w * offset, b.y + b.h),
                })
            }
        }
    };

    let mut paths: Vec<Vec<(f64, f64)>> = Vec::new();
    for edge in &graph.block_edges {
        // One wire per pin pair: an R-M merge folds several nets into one
        // edge, and each folded pin still needs its own connection.
        let n = edge.from_pins.len().max(edge.to_pins.len()).max(1);
        for k in 0..n {
            let fan = (k as f64) * 4.0;
            let p1 = edge.from_pins.get(k).or(edge.from_pins.first());
            let p2 = edge.to_pins.get(k).or(edge.to_pins.first());
            let (Some(a1), Some(a2)) = (
                p1.and_then(|pid| pin_anchor(graph, &by_id, edge.from_box, *pid)),
                p2.and_then(|pid| pin_anchor(graph, &by_id, edge.to_box, *pid)),
            ) else {
                continue;
            };
            let (x1, y1) = (a1.0, a1.1 + if k > 0 { fan } else { 0.0 });
            let (x2, y2) = (a2.0, a2.1 + if k > 0 { fan } else { 0.0 });
            // Z-shaped detour through the horizontal midpoint; degenerate
            // axes collapse to a straight segment.
            let mut path = vec![(x1, y1)];
            if (x1 - x2).abs() < 0.01 || (y1 - y2).abs() < 0.01 {
                path.push((x2, y2));
            } else {
                let xm = (x1 + x2) / 2.0;
                path.push((xm, y1));
                path.push((xm, y2));
                path.push((x2, y2));
            }
            paths.push(path);
        }
    }

    // Wire ends, counted so a 3-way meeting point gets its junction dot.
    let mut ends: HashMap<(u32, u32), u32> = HashMap::new();
    for path in &paths {
        let first = path.first().map(|p| key_of(*p));
        let last = path.last().map(|p| key_of(*p));
        for k in [first, last].into_iter().flatten() {
            *ends.entry(k).or_insert(0) += 1;
        }
        for w in path.windows(2) {
            emit_wire(graph.bid, xf, w[0].0, w[0].1, w[1].0, w[1].1, e);
        }
    }
    for (k, n) in &ends {
        if *n >= 3 {
            let (x, y) = (k.0 as f64 / 100.0, k.1 as f64 / 100.0);
            e.open("junction");
            line!(e, "(at {} {})", mm(xf.x(x)), mm(xf.y(y)));
            line!(e, "(diameter 0)");
            line!(e, "(color 0 0 0 0)");
            line!(
                e,
                "(uuid \"{}\")",
                det_uuid(&format!("{}#bjn#{}#{}", graph.bid, k.0, k.1))
            );
            e.close();
        }
    }

    // The net name on every named edge — the cross-sheet glue.
    for (edge, path) in graph.block_edges.iter().zip(&paths) {
        if edge.label.is_empty() || is_anon(&edge.label) {
            continue;
        }
        let mid = middle_of(path);
        text_label(graph.bid, &edge.label, xf.x(mid.0), xf.y(mid.1), Slide::X, &mut Vec::new(), e);
    }
    let _ = set;
}

/// Root-layer passives: the block diagram drops them from its edges, but
/// downstream needs them on the net. Each pin gets a short stub and the net's
/// name as a label — same-name labels are what unify a root net across its
/// block edges and sheet pins.
fn emit_root_passive_nets(
    graph: &McVecGraph,
    xf: &Xform,
    ledger: &mut Vec<(f64, f64, f64, f64)>,
    e: &mut Emit,
) {
    if graph.layer_style == LayerStyle::Device {
        return;
    }
    for b in component_boxes(graph) {
        for p in &b.pins {
            let Some(net) = graph.nets.iter().find(|n| {
                n.endpoints.iter().any(|ep| {
                    ep.box_id == b.id
                        && endpoint_pin(b, ep).map(|bp| bp.id) == Some(p.id)
                })
            }) else {
                continue;
            };
            let (side, offset) = pin_placement(b, p);
            let (ax, ay) = anchor_mm(xf, b, side, offset);
            let (dx, dy) = match side {
                EntrySide::Left => (-10.0 * MM_PER_PX, 0.0),
                EntrySide::Right => (10.0 * MM_PER_PX, 0.0),
                EntrySide::Top => (0.0, -10.0 * MM_PER_PX),
                EntrySide::Bottom => (0.0, 10.0 * MM_PER_PX),
            };
            let (sx, sy) = (ax + dx, ay + dy);
            e.open("wire");
            e.open("pts");
            line!(e, "(xy {} {})", mm(ax), mm(ay));
            line!(e, "(xy {} {})", mm(sx), mm(sy));
            e.close();
            e.open("stroke");
            line!(e, "(width 0)");
            line!(e, "(type default)");
            e.close();
            line!(
                e,
                "(uuid \"{}\")",
                det_uuid(&format!("{}#rw#{}#{}", graph.bid, b.id, p.id))
            );
            e.close();
            if !is_anon(&net.name) {
                text_label(graph.bid, &net.name, sx, sy, Slide::X, ledger, e);
            }
        }
    }
}

fn key_of(p: (f64, f64)) -> (u32, u32) {
    ((p.0 * 100.0).round() as u32, (p.1 * 100.0).round() as u32)
}

/// Midpoint of a path's longest span — where the net name reads best.
fn middle_of(path: &[(f64, f64)]) -> (f64, f64) {
    let mut best = (0.0f64, path[0]);
    for w in path.windows(2) {
        let len = (w[1].0 - w[0].0).abs() + (w[1].1 - w[0].1).abs();
        if len > best.0 {
            best = (len, ((w[0].0 + w[1].0) / 2.0, (w[0].1 + w[1].1) / 2.0));
        }
    }
    best.1
}

fn power_symbol(
    graph: &McVecGraph,
    net_name: &str,
    generic: &str,
    is_ground: bool,
    px: f64,
    py: f64,
    xf: &Xform,
    path: &str,
    top: &str,
    state: &mut SheetState,
    e: &mut Emit,
) {
    state.pwr_seq += 1;
    let value = if is_anon(net_name) {
        generic.to_string()
    } else {
        net_name.to_string()
    };
    let seed = format!("{}#pwr#{}#{}#{}", graph.bid, net_name, px, py);
    let x = xf.x(px);
    let y = xf.y(py);
    e.open("symbol");
    line!(
        e,
        "(lib_id \"mcc:{}\")",
        if is_ground { "GND" } else { "PWR" }
    );
    line!(e, "(at {} {} 0)", mm(x), mm(y));
    line!(e, "(unit 1)");
    line!(e, "(exclude_from_sim no)");
    line!(e, "(in_bom yes)");
    line!(e, "(on_board yes)");
    line!(e, "(dnp no)");
    line!(e, "(uuid \"{}\")", det_uuid(&seed));
    property(
        e,
        "Reference",
        &format!("#PWR{}", state.pwr_seq),
        x - 5.08,
        y - 2.54,
        true,
    );
    property(e, "Value", &value, x - 5.08, y + 2.54, false);
    property(e, "Footprint", "", x, y, true);
    e.open("pin \"1\"");
    line!(e, "(uuid \"{}\")", det_uuid(&format!("{seed}#pin")));
    e.close();
    emit_instances(e, path, top, &format!("#PWR{}", state.pwr_seq));
    e.close();
}

fn emit_instances(e: &mut Emit, path: &str, top: &str, reference: &str) {
    e.open("instances");
    e.open(&format!("project \"{}\"", escape(top)));
    e.open(&format!("path \"/{path}\""));
    line!(e, "(reference \"{}\")", escape(reference));
    line!(e, "(unit 1)");
    e.close();
    e.close();
    e.close();
}

/// One PWR_FLAG per power/ground tree that carries a glyph: KiCad's "power pin
/// not driven" rule wants a driver on every power net, and a rail received
/// through a sheet pin or a label has none of type `power_out` on its own.
fn power_flag(
    graph: &McVecGraph,
    net_name: &str,
    generic: &str,
    px: f64,
    py: f64,
    xf: &Xform,
    path: &str,
    top: &str,
    state: &mut SheetState,
    e: &mut Emit,
) {
    let value = if is_anon(net_name) {
        generic.to_string()
    } else {
        net_name.to_string()
    };
    if !state.flagged.insert(value) {
        return;
    }
    state.pwr_seq += 1;
    let seed = format!("{}#flag#{}#{}", graph.bid, px, py);
    let x = xf.x(px);
    let y = xf.y(py);
    e.open("symbol");
    line!(e, "(lib_id \"mcc:PWR_FLAG\")");
    line!(e, "(at {} {} 0)", mm(x), mm(y));
    line!(e, "(unit 1)");
    line!(e, "(exclude_from_sim no)");
    line!(e, "(in_bom yes)");
    line!(e, "(on_board yes)");
    line!(e, "(dnp no)");
    line!(e, "(uuid \"{}\")", det_uuid(&seed));
    property(
        e,
        "Reference",
        &format!("#FLG{}", state.pwr_seq),
        x,
        y - 3.81,
        true,
    );
    property(e, "Value", "PWR_FLAG", x, y + 1.27, false);
    property(e, "Footprint", "", x, y, true);
    e.open("pin \"1\"");
    line!(e, "(uuid \"{}\")", det_uuid(&format!("{seed}#pin")));
    e.close();
    emit_instances(e, path, top, &format!("#FLG{}", state.pwr_seq));
    e.close();
}

// === Symbol instances ===

#[allow(clippy::too_many_arguments)]
fn emit_symbol_instance(
    graph: &McVecGraph,
    b: &McVecBox,
    lib: &str,
    xf: &Xform,
    path: &str,
    set: &SheetSet,
    state: &mut SheetState,
    e: &mut Emit,
) {
    let reference = unique_ref(state, graph.bid, b);
    let value = b.value.clone().unwrap_or_else(|| b.class_name.clone());
    let x = xf.x(b.x);
    let y = xf.y(b.y);
    let rx = x + b.w * MM_PER_PX / 2.0;
    let y_bot = y + b.h * MM_PER_PX;
    let seed = format!("{}#sym#{}", graph.bid, b.id);

    e.open("symbol");
    line!(e, "(lib_id \"mcc:{lib}\")");
    line!(e, "(at {} {} 0)", mm(x), mm(y));
    line!(e, "(unit 1)");
    line!(e, "(exclude_from_sim no)");
    line!(e, "(in_bom yes)");
    line!(e, "(on_board yes)");
    line!(e, "(dnp {})", if b.not_fitted { "yes" } else { "no" });
    line!(e, "(uuid \"{}\")", det_uuid(&seed));
    property(e, "Reference", &reference, rx, y - 3.0, false);
    property(e, "Value", &value, rx, y_bot + 3.0, false);
    property(e, "Footprint", "", rx, y_bot + 6.0, true);
    property(e, "mcc_path", &b.inst_path, rx, y_bot + 9.0, true);
    for p in &b.pins {
        e.open(&format!("pin \"{}\"", escape(&p.pin_id)));
        line!(
            e,
            "(uuid \"{}\")",
            det_uuid(&format!("{}#pin#{}#{}", graph.bid, b.id, p.id))
        );
        e.close();
    }
    emit_instances(e, path, &set.top, &reference);
    e.close();
}

fn property(e: &mut Emit, name: &str, value: &str, x: f64, y: f64, hidden: bool) {
    e.open(&format!("property \"{name}\" \"{}\"", escape(value)));
    line!(e, "(at {} {} 0)", mm(x), mm(y));
    e.open("effects");
    e.open("font");
    line!(e, "(size 1.27 1.27)");
    e.close();
    if hidden {
        line!(e, "(hide yes)");
    }
    e.close();
    e.close();
}

/// Reference unique within the sheet, in encounter order.
fn unique_ref(state: &mut SheetState, bid: i64, b: &McVecBox) -> String {
    let base = b.display_label().to_string();
    let used = state.used_refs.entry(bid).or_default();
    if used.insert(base.clone()) {
        return base;
    }
    let mut n = 2usize;
    loop {
        let candidate = format!("{base}_{n}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        n += 1;
    }
}

// === Rail glyphs (pin decorations) ===

#[allow(clippy::too_many_arguments)]
fn emit_rail_decorations(
    graph: &McVecGraph,
    xf: &Xform,
    path: &str,
    set: &SheetSet,
    state: &mut SheetState,
    e: &mut Emit,
    net_names: &HashMap<String, String>,
) {
    for d in &graph.rail_decorations {
        let Some(b) = graph.boxes.iter().find(|b| b.id == d.box_id) else {
            continue;
        };
        let Some(pin) = b.pins.iter().find(|p| p.id == d.pin_id) else {
            continue;
        };
        // The glyph's connection point is its symbol origin, so dropping it at
        // the pin anchor joins the device pin with zero wire.
        let (side, offset) = pin_placement(b, pin);
        let (x, y) = anchor_mm(xf, b, side, offset);
        let net_name = if d.label.is_empty() {
            rail_name_of_pin(graph, d.box_id, d.pin_id).unwrap_or_default()
        } else {
            d.label.clone()
        };
        let net_name = net_names
            .get(&net_name)
            .cloned()
            .unwrap_or(net_name);
        let shown = if is_anon(&net_name) {
            if d.is_ground { "GND" } else { "PWR" }.to_string()
        } else {
            net_name.clone()
        };
        state.pwr_seq += 1;
        let seed = format!("{}#dec#{}#{}", graph.bid, d.box_id, d.pin_id);
        e.open("symbol");
        line!(
            e,
            "(lib_id \"mcc:{}\")",
            if d.is_ground { "GND" } else { "PWR" }
        );
        line!(e, "(at {} {} 0)", mm(x), mm(y));
        line!(e, "(unit 1)");
        line!(e, "(exclude_from_sim no)");
        line!(e, "(in_bom yes)");
        line!(e, "(on_board yes)");
        line!(e, "(dnp no)");
        line!(e, "(uuid \"{}\")", det_uuid(&seed));
        property(
            e,
            "Reference",
            &format!("#PWR{}", state.pwr_seq),
            x - 5.08,
            y - 2.54,
            true,
        );
        property(e, "Value", &shown, x - 5.08, y + 2.54, false);
        property(e, "Footprint", "", x, y, true);
        e.open("pin \"1\"");
        line!(e, "(uuid \"{}\")", det_uuid(&format!("{seed}#pin")));
        e.close();
        emit_instances(e, path, &set.top, &format!("#PWR{}", state.pwr_seq));
        e.close();
    }
}

/// The declared net a pin sits on — read from the net's own endpoint list, so
/// a glyph names its net without guessing from a spelling.
fn rail_name_of_pin(graph: &McVecGraph, box_id: i64, pin_id: i64) -> Option<String> {
    graph
        .nets
        .iter()
        .find(|n| {
            n.endpoints
                .iter()
                .any(|e| e.box_id == box_id && e.pin_id == pin_id)
        })
        .map(|n| n.name.clone())
        .filter(|n| !is_anon(n))
}

// === No-connects ===

fn emit_no_connects(graph: &McVecGraph, xf: &Xform, e: &mut Emit) {
    let dbg = std::env::var("MCC_KSCH_DEBUG").is_ok();
    let wired: HashSet<(i64, i64)> = graph
        .nets
        .iter()
        .flat_map(|n| n.endpoints.iter().map(|ep| (ep.box_id, ep.pin_id)))
        .collect();
    // A decorated pin carries its rail glyph at the same anchor; a no_connect
    // there would fight the symbol for the connection point.
    let decorated: HashSet<(i64, i64)> = graph
        .rail_decorations
        .iter()
        .map(|d| (d.box_id, d.pin_id))
        .collect();
    for b in component_boxes(graph) {
        for p in &b.pins {
            if wired.contains(&(b.id, p.id)) || decorated.contains(&(b.id, p.id)) {
                if dbg {
                    eprintln!("[ksch] nc skip {} pin{} id={} wired={} deco={}", b.name, p.pin_id, p.id, wired.contains(&(b.id, p.id)), decorated.contains(&(b.id, p.id)));
                }
                continue;
            }
            let (side, offset) = pin_placement(b, p);
            let (x, y) = anchor_mm(xf, b, side, offset);
            e.open("no_connect");
            line!(e, "(at {} {})", mm(x), mm(y));
            line!(
                e,
                "(uuid \"{}\")",
                det_uuid(&format!("{}#nc#{}#{}", graph.bid, b.id, p.id))
            );
            e.close();
        }
    }
}

// === Sheets (hierarchy) ===

fn emit_sheet_instance(
    set: &SheetSet,
    parent_idx: usize,
    graph: &McVecGraph,
    b: &McVecBox,
    xf: &Xform,
    e: &mut Emit,
) {
    let Some(child_bid) = graph
        .clickable_subs
        .iter()
        .copied()
        .find(|bid| *bid == b.id)
    else {
        return;
    };
    let Some(file) = set.file_of.get(&child_bid) else {
        return;
    };
    let uuid = set
        .sheet_uuid_of
        .get(&child_bid)
        .cloned()
        .unwrap_or_else(|| det_uuid(&format!("{}#sheet#{}", set.top, child_bid)));
    let x = xf.x(b.x);
    let y = xf.y(b.y);
    let w = b.w * MM_PER_PX;
    let h = b.h * MM_PER_PX;

    let parent_path = match set.layers[parent_idx].parent {
        None => set.root_uuid.clone(),
        Some(pbid) => format!(
            "{}/{}",
            set.root_uuid,
            set.sheet_uuid_of
                .get(&pbid)
                .cloned()
                .unwrap_or_else(|| det_uuid(&format!("{}#sheet#{}", set.top, pbid)))
        ),
    };
    let page = set.page_of.get(&child_bid).copied().unwrap_or(1);

    e.open("sheet");
    line!(e, "(at {} {})", mm(x), mm(y));
    line!(e, "(size {} {})", mm(w), mm(h));
    e.open("stroke");
    line!(e, "(width 0.1524)");
    line!(e, "(type solid)");
    e.close();
    e.open("fill");
    line!(e, "(color 0 0 0 0.0000)");
    e.close();
    line!(e, "(uuid \"{uuid}\")");
    property(
        e,
        "Sheetname",
        &sanitize_file_stem(&b.name),
        x,
        y - 1.9,
        false,
    );
    property(e, "Sheetfile", file, x, y + h + 1.0, false);
    // Sheet pins mirror the child's port list (see `ports_of`): one pin per
    // child hierarchical label, same name, same direction. Anchored at the
    // parent crossing the port's net actually lands on — by port name, or by
    // the crossing wire that carries the member spelling of the group — and
    // spread down the left edge when the parent never drew that crossing.
    let child_ports = set
        .ports_of
        .get(&child_bid)
        .cloned()
        .unwrap_or_default();
    for (i, (port_name, shape)) in child_ports.iter().enumerate() {
        let via_box = b
            .boundary_ports
            .iter()
            .find(|p| &p.port_name == port_name)
            .and_then(|p| b.find_entry(p.entry_pin_id))
            .or_else(|| {
                b.entry_points
                    .iter()
                    .find(|ep| &ep.pin_name == port_name)
            });
        let (px, py, angle) = match via_box {
            Some(ep) => {
                let (px, py) = anchor_mm(xf, b, ep.side, ep.offset);
                // Sheet pins point away from the sheet body.
                let angle = match ep.side {
                    EntrySide::Left => "180",
                    EntrySide::Right => "0",
                    EntrySide::Top => "90",
                    EntrySide::Bottom => "270",
                };
                (px, py, angle)
            }
            None => {
                let py = y + h * (i as f64 + 1.0) / (child_ports.len() + 1) as f64;
                (x, py, "180")
            }
        };
        e.open(&format!("pin \"{port_name}\" {shape}"));
        line!(e, "(at {} {} {})", mm(px), mm(py), angle);
        e.open("effects");
        e.open("font");
        line!(e, "(size 1.27 1.27)");
        e.close();
        e.close();
        line!(
            e,
            "(uuid \"{}\")",
            det_uuid(&format!("{}#sp#{}#{}", graph.bid, b.id, port_name))
        );
        e.close();
    }
    e.open("instances");
    e.open(&format!("project \"{}\"", escape(&set.top)));
    e.open(&format!("path \"/{parent_path}\""));
    line!(e, "(page \"{page}\")");
    e.close();
    e.close();
    e.close();
    e.close();
}

/// Bridge a tree's endpoint pins to their taps. The routers stop a stub
/// length (`LEAD_STUB_LEN` / `PIN_STUB_LEN`) short of the anchor and the
/// renderer draws the lead itself; KiCad gets that lead as an explicit wire,
/// so the pin's connection point always lands on copper.
fn bridge_pins(graph: &McVecGraph, t: &EquiTree, xf: &Xform, e: &mut Emit) {
    let Some(net) = graph.nets.iter().find(|n| n.name == t.net_name) else {
        return;
    };
    let mut ends: Vec<(f64, f64)> = t
        .segments
        .iter()
        .flat_map(|s| [(s.x1, s.y1), (s.x2, s.y2)])
        .collect();
    for ep in &net.endpoints {
        let Some(b) = graph.boxes.iter().find(|b| b.id == ep.box_id) else {
            continue;
        };
        if !matches!(b.kind, BoxKind::TwoPin | BoxKind::MultiPin) {
            continue;
        }
        let Some(pin) = endpoint_pin(b, ep) else {
            continue;
        };
        let (side, offset) = pin_placement(b, pin);
        let (ax, ay) = match b.find_entry(ep.pin_id) {
            Some(a) => anchor_px(b, a),
            None => match side {
                EntrySide::Left => (b.x, b.y + b.h * offset),
                EntrySide::Right => (b.x + b.w, b.y + b.h * offset),
                EntrySide::Top => (b.x + b.w * offset, b.y),
                EntrySide::Bottom => (b.x + b.w * offset, b.y + b.h),
            },
        };
        if ends
            .iter()
            .any(|(x, y)| (x - ax).abs() < 0.05 && (y - ay).abs() < 0.05)
        {
            continue;
        }
        let Some((ex, ey)) = ends
            .iter()
            .copied()
            .min_by(|p, q| d2(*p, (ax, ay)).total_cmp(&d2(*q, (ax, ay))))
        else {
            continue;
        };
        // A tall IC spreads its pins across the whole body; the tap end can
        // sit hundreds of px from the anchor. Any same-net end within reach
        // bridges - Manhattan, so the L detour stays orthogonal.
        if (ex - ax).abs() + (ey - ay).abs() > 220.0 {
            continue;
        }
        if (ex - ax).abs() < 0.05 || (ey - ay).abs() < 0.05 {
            emit_wire(graph.bid, xf, ax, ay, ex, ey, e);
        } else {
            emit_wire(graph.bid, xf, ax, ay, ex, ay, e);
            emit_wire(graph.bid, xf, ex, ay, ex, ey, e);
        }
        ends.push((ax, ay));
    }
}

/// Last-resort connection for net-endpoint pins whose anchor no tap and no
/// bridge ever reached (pins the placer left without an entry point): a stub
/// plus the net's name joins the copper by name; an anonymous net draws a
/// direct L-wire to its nearest tree end instead, since it has no name to
/// join by.
fn emit_pin_rescue(
    graph: &McVecGraph,
    t: &EquiTree,
    net: &VizNet,
    xf: &Xform,
    display: &str,
    ledger: &mut Vec<(f64, f64, f64, f64)>,
    e: &mut Emit,
) {
    let named = !is_anon(display);
    let mut ends: Vec<(f64, f64)> = t
        .segments
        .iter()
        .flat_map(|s| [(s.x1, s.y1), (s.x2, s.y2)])
        .collect();
    if ends.is_empty() {
        return;
    }
    let dbg = std::env::var("MCC_KSCH_DEBUG").is_ok();
    for ep in &net.endpoints {
        let Some(b) = graph.boxes.iter().find(|b| b.id == ep.box_id) else {
            continue;
        };
        if !matches!(b.kind, BoxKind::TwoPin | BoxKind::MultiPin) {
            continue;
        }
        let Some(pin) = endpoint_pin(b, ep) else {
            if dbg {
                eprintln!("[ksch] rescue {} ep pin_id={} UNJOINED", b.name, ep.pin_id);
            }
            // The id chain failed, but a wired pin's ENTRY names the net its
            // wire carries: that entry is the crossing this endpoint means.
            if named {
                if let Some(entry) = b
                    .entry_points
                    .iter()
                    .find(|e| e.pin_name == net.name)
                {
                    let (ax, ay) = anchor_mm(xf, b, entry.side, entry.offset);
                    let (sx, sy) = match entry.side {
                        EntrySide::Left => (ax - 10.0, ay),
                        EntrySide::Right => (ax + 10.0, ay),
                        EntrySide::Top => (ax, ay - 10.0),
                        EntrySide::Bottom => (ax, ay + 10.0),
                    };
                    emit_wire(graph.bid, xf, ax, ay, sx, sy, e);
                    text_label(graph.bid, display, sx, sy, Slide::Y, ledger, e);
                }
            }
            continue;
        };
        let (side, offset) = pin_placement(b, pin);
        let has_entry = b.find_entry(ep.pin_id).is_some();
        if dbg && b.name == "UC" {
            eprintln!(
                "[ksch] rescue UC pin{} id={} entry={} net={} ends={}",
                pin.pin_id, ep.pin_id, has_entry, net.name, ends.len()
            );
        }
        let (ax, ay) = match side {
            EntrySide::Left => (b.x, b.y + b.h * offset),
            EntrySide::Right => (b.x + b.w, b.y + b.h * offset),
            EntrySide::Top => (b.x + b.w * offset, b.y),
            EntrySide::Bottom => (b.x + b.w * offset, b.y + b.h),
        };
        // A tap that reached the anchor, or a lead that ends within a stub of
        // it, means the tree already serves this pin.
        let served = ends
            .iter()
            .any(|(x, y)| (x - ax).abs() < 0.05 && (y - ay).abs() < 0.05)
            || (has_entry
                && ends.iter().any(|(x, y)| {
                    ((x - ax).abs() < 0.05 && (y - ay).abs() <= 12.0)
                        || ((y - ay).abs() < 0.05 && (x - ax).abs() <= 12.0)
                }));
        if served {
            continue;
        }
        let Some(&(ex, ey)) = ends
            .iter()
            .min_by(|p, q| {
                let dp = d2(**p, (ax, ay));
                let dq = d2(**q, (ax, ay));
                dp.total_cmp(&dq)
            })
        else {
            continue;
        };
        if named {
            emit_stub_label(graph.bid, xf, ax, ay, side, display, ledger, e);
        } else if (ex - ax).abs() < 0.05 || (ey - ay).abs() < 0.05 {
            emit_wire(graph.bid, xf, ax, ay, ex, ey, e);
        } else {
            emit_wire(graph.bid, xf, ax, ay, ex, ay, e);
            emit_wire(graph.bid, xf, ex, ay, ex, ey, e);
        }
        ends.push((ax, ay));
    }
}

/// Tile-level wired-pin guarantee: EVERY pin a net references gets copper at
/// its anchor. Tree taps, bridges and the per-tree rescue can all miss (pins
/// the placer left without an entry, trees that skip a topology) — this pass
/// is tree-independent, so nothing a netlist wires can end up floating. Named
/// nets join by name (stub + label); anonymous ones L-wire to the nearest
/// same-net tree end.
fn emit_wired_pin_guarantee(
    graph: &McVecGraph,
    trees: &[EquiTree],
    islands: &HashMap<String, String>,
    net_names: &HashMap<String, String>,
    xf: &Xform,
    ledger: &mut Vec<(f64, f64, f64, f64)>,
    e: &mut Emit,
) {
    let mut ends: Vec<(f64, f64)> = trees
        .iter()
        .flat_map(|t| t.segments.iter())
        .flat_map(|s| [(s.x1, s.y1), (s.x2, s.y2)])
        .collect();
    for net in &graph.nets {
        let shown = net_names
            .get(&net.name)
            .cloned()
            .unwrap_or_else(|| net.name.clone());
        let named = !is_anon(&shown);
        for ep in &net.endpoints {
            let Some(b) = graph.boxes.iter().find(|b| b.id == ep.box_id) else {
                continue;
            };
            if !matches!(b.kind, BoxKind::TwoPin | BoxKind::MultiPin) {
                continue;
            }
            let Some(pin) = endpoint_pin(b, ep) else { continue };
            let (side, offset) = pin_placement(b, pin);
            let (ax, ay) = match side {
                EntrySide::Left => (b.x, b.y + b.h * offset),
                EntrySide::Right => (b.x + b.w, b.y + b.h * offset),
                EntrySide::Top => (b.x + b.w * offset, b.y),
                EntrySide::Bottom => (b.x + b.w * offset, b.y + b.h),
            };
            if ends
                .iter()
                .any(|(x, y)| (x - ax).abs() < 0.05 && (y - ay).abs() < 0.05)
            {
                continue;
            }
            let (sx, sy) = match side {
                EntrySide::Left => (ax - 10.0, ay),
                EntrySide::Right => (ax + 10.0, ay),
                EntrySide::Top => (ax, ay - 10.0),
                EntrySide::Bottom => (ax, ay + 10.0),
            };
            emit_wire(graph.bid, xf, ax, ay, sx, sy, e);
            if named {
                text_label(graph.bid, &shown, xf.x(sx), xf.y(sy), Slide::None, ledger, e);
            } else if let Some(&(ex, ey)) = ends
                .iter()
                .min_by(|p, q| {
                    let dp = (p.0 - ax).powi(2) + (p.1 - ay).powi(2);
                    let dq = (q.0 - ax).powi(2) + (q.1 - ay).powi(2);
                    dp.total_cmp(&dq)
                })
            {
                if (ex - sx).abs() < 0.05 || (ey - sy).abs() < 0.05 {
                    emit_wire(graph.bid, xf, sx, sy, ex, ey, e);
                } else {
                    emit_wire(graph.bid, xf, sx, sy, ex, sy, e);
                    emit_wire(graph.bid, xf, ex, sy, ex, ey, e);
                }
            }
            ends.push((ax, ay));
        }
    }
    // Second sweep over the flat table's copper islands: members the
    // projected graph dropped (a pull-up referenced only through the net
    // table) still get their stub and island name.
    for b in component_boxes(graph) {
        for p in &b.pins {
            let full = format!("{}.{}", b.inst_path, p.pin_id);
            let Some(island) = islands.get(&full) else { continue };
            if is_anon(island) {
                continue;
            }
            let (side, offset) = pin_placement(b, p);
            let (ax, ay) = match side {
                EntrySide::Left => (b.x, b.y + b.h * offset),
                EntrySide::Right => (b.x + b.w, b.y + b.h * offset),
                EntrySide::Top => (b.x + b.w * offset, b.y),
                EntrySide::Bottom => (b.x + b.w * offset, b.y + b.h),
            };
            if ends
                .iter()
                .any(|(x, y)| (x - ax).abs() < 0.05 && (y - ay).abs() < 0.05)
            {
                continue;
            }
            let (sx, sy) = match side {
                EntrySide::Left => (ax - 10.0, ay),
                EntrySide::Right => (ax + 10.0, ay),
                EntrySide::Top => (ax, ay - 10.0),
                EntrySide::Bottom => (ax, ay + 10.0),
            };
            emit_wire(graph.bid, xf, ax, ay, sx, sy, e);
            text_label(graph.bid, island, xf.x(sx), xf.y(sy), Slide::None, ledger, e);
            ends.push((ax, ay));
        }
    }
}

/// Stub + label as one unit: the stub grows (10 → 25 → 40 px) until the
/// label at its far end finds free space, so adjacent-pin labels never
/// overlap and the wire stays attached to the pin the whole time.
#[allow(clippy::too_many_arguments)]
fn emit_stub_label(
    bid: i64,
    xf: &Xform,
    ax: f64,
    ay: f64,
    side: EntrySide,
    display: &str,
    ledger: &mut Vec<(f64, f64, f64, f64)>,
    e: &mut Emit,
) {
    let (dx, dy) = match side {
        EntrySide::Left => (-1.0, 0.0),
        EntrySide::Right => (1.0, 0.0),
        EntrySide::Top => (0.0, -1.0),
        EntrySide::Bottom => (0.0, 1.0),
    };
    let w = display.len() as f64 * 0.9 + 2.0;
    let free = |sx: f64, sy: f64| {
        !ledger.iter().any(|(ox, oy, ow, oh)| {
            sx < *ox + *ow && *ox < sx + w && sy < *oy + *oh && *oy < sy + 2.0
        })
    };
    let mut ext = 10.0;
    for cand in [10.0, 25.0, 40.0] {
        ext = cand;
        if free(ax + dx * ext, ay + dy * ext) {
            break;
        }
    }
    let (sx, sy) = (ax + dx * ext, ay + dy * ext);
    emit_wire(bid, xf, ax, ay, sx, sy, e);
    text_label(bid, display, xf.x(sx), xf.y(sy), Slide::None, ledger, e);
}

fn d2(p: (f64, f64), q: (f64, f64)) -> f64 {
    (p.0 - q.0) * (p.0 - q.0) + (p.1 - q.1) * (p.1 - q.1)
}

/// The physical pin a net endpoint attaches to. `EndpointRef.pin_id` and
/// `BoxPin.id` agree for most boxes, but pins that entered the table through
/// a different path (typed chips, expansion products) drift apart; when the
/// id join fails, the pin NUMBER is the same identity spelled as a string —
/// the endpoint carries it as `pin_number` and the box as `pin_id`.
fn endpoint_pin<'a>(b: &'a McVecBox, ep: &crate::vector::graph::EndpointRef) -> Option<&'a crate::vector::graph::boxdef::BoxPin> {
    if let Some(p) = b.pins.iter().find(|p| p.id == ep.pin_id) {
        return Some(p);
    }
    if let Some(n) = ep.pin_number {
        if let Some(p) = b.pins.iter().find(|p| p.pin_id == n.to_string()) {
            return Some(p);
        }
    }
    b.pins.iter().find(|p| p.pin_id == ep.pin_name && !ep.pin_name.is_empty())
}

/// Side and offset of a physical pin: the layout's anchor when it assigned
/// one, else the default left/right DIP split — so a pin the placer never
/// reached still gets a connection point on the symbol instead of vanishing.
fn pin_placement(b: &McVecBox, p: &crate::vector::graph::boxdef::BoxPin) -> (EntrySide, f64) {
    if let Some(a) = b.find_entry(p.id) {
        return (a.side, a.offset);
    }
    let n = b.pins.len().max(1);
    let per_side = n.div_ceil(2);
    let idx = b.pins.iter().position(|q| q.id == p.id).unwrap_or(0);
    if idx < per_side {
        (EntrySide::Left, (idx + 1) as f64 / (per_side + 1) as f64)
    } else {
        let k = idx - per_side;
        let rest = n - per_side;
        (EntrySide::Right, (k + 1) as f64 / (rest + 1) as f64)
    }
}

/// Anchor of one entry point, in layer px.
fn anchor_px(b: &McVecBox, a: &EntryPoint) -> (f64, f64) {
    match a.side {
        EntrySide::Top => (b.x + b.w * a.offset, b.y),
        EntrySide::Bottom => (b.x + b.w * a.offset, b.y + b.h),
        EntrySide::Left => (b.x, b.y + b.h * a.offset),
        EntrySide::Right => (b.x + b.w, b.y + b.h * a.offset),
    }
}

/// Hierarchical labels on a child sheet: one per entry of the child's port
/// list (the same list the parent's sheet pins mirror), anchored — in order of
/// preference — at the port terminal's pin, the boundary net's own wire, the
/// module frame port, and finally the frame's left edge, so every port the
/// parent names exists on the child by construction.
fn emit_boundary_labels(graph: &McVecGraph, trees: &[EquiTree], xf: &Xform, e: &mut Emit) {
    let ports = child_ports_of(graph);
    // Left-edge spread for ports with no drawn anchor of their own.
    let frame = graph.module_frame.as_ref();
    let n = ports.len();
    let mut emitted: HashSet<String> = HashSet::new();
    for (i, (name, shape)) in ports.iter().enumerate() {
        if !emitted.insert(name.clone()) {
            continue;
        }
        let anchored = graph.nets.iter().find_map(|net| {
            let bi = net.boundary.as_ref()?;
            if bi.port_name != *name {
                return None;
            }
            let terminal = graph.boxes.iter().find(|b| {
                b.kind == BoxKind::PortTerminal
                    && b.boundary_ports.iter().any(|p| p.port_name == *name)
            });
            terminal
                .and_then(|b| {
                    b.entry_points
                        .first()
                        .map(|ep| (ep.side, anchor_mm(xf, b, ep.side, ep.offset)))
                })
                .or_else(|| boundary_tree_endpoint(graph, trees, xf, net).map(|(x, y)| (EntrySide::Left, (x, y))))
                .map(|(side, xy)| (side, xy, shape_of_io(bi.io)))
        });
        let (angle, (x, y), real_shape) = match anchored {
            Some((side, (x, y), shp)) => (
                // The label points along the wire, into the sheet.
                match side {
                    EntrySide::Left => "0",
                    EntrySide::Right => "180",
                    EntrySide::Top => "270",
                    EntrySide::Bottom => "90",
                },
                (x, y),
                shp,
            ),
            None => match frame.and_then(|f| f.ports.iter().find(|p| &p.name == name)) {
                Some(fp) => (
                    match fp.side {
                        EntrySide::Left => "0",
                        EntrySide::Right => "180",
                        EntrySide::Top => "270",
                        EntrySide::Bottom => "90",
                    },
                    (xf.x(fp.x), xf.y(fp.y)),
                    *shape,
                ),
                None => (
                    "0",
                    (
                        frame.map(|f| xf.x(f.x)).unwrap_or(0.0),
                        frame
                            .map(|f| xf.y(f.y + f.h * (i as f64 + 1.0) / (n + 1) as f64))
                            .unwrap_or(i as f64 * 5.08),
                    ),
                    *shape,
                ),
            },
        };
        e.open(&format!("hierarchical_label \"{}\"", escape(name)));
        line!(e, "(at {} {} {})", mm(x), mm(y), angle);
        line!(e, "(shape {real_shape})");
        e.open("effects");
        e.open("font");
        line!(e, "(size 1.27 1.27)");
        e.close();
        line!(e, "(justify left bottom)");
        e.close();
        line!(e, "(uuid \"{}\")", det_uuid(&format!("{}#hl#{}", graph.bid, name)));
        e.close();
    }
}

/// Fallback anchor when no port terminal box exists: the tree endpoint nearest
/// the frame port — still a point ON the net's wire, so the label connects.
fn boundary_tree_endpoint(
    graph: &McVecGraph,
    trees: &[EquiTree],
    xf: &Xform,
    net: &VizNet,
) -> Option<(f64, f64)> {
    let bi = net.boundary.as_ref()?;
    let frame_port = graph
        .module_frame
        .as_ref()?
        .ports
        .iter()
        .find(|p| p.name == bi.port_name)?;
    let tree = trees.iter().find(|t| {
        graph
            .nets
            .iter()
            .find(|n| n.name == t.net_name)
            .map(|n| n.nid)
            == Some(net.nid)
    })?;
    let (tx, ty) = (frame_port.x, frame_port.y);
    tree.segments
        .iter()
        .flat_map(|s| [(s.x1, s.y1), (s.x2, s.y2)])
        .min_by(|a, b| {
            let da = (a.0 - tx).powi(2) + (a.1 - ty).powi(2);
            let db = (b.0 - tx).powi(2) + (b.1 - ty).powi(2);
            da.total_cmp(&db)
        })
        .map(|(x, y)| (xf.x(x), xf.y(y)))
}

fn shape_of_io(io: IoDirection) -> &'static str {
    match io {
        IoDirection::Input => "input",
        IoDirection::Output => "output",
        IoDirection::Bidir => "bidirectional",
        _ => "passive",
    }
}

fn is_anon(name: &str) -> bool {
    name.is_empty() || crate::instant::mc_net::is_anon_net_name(name) || name.starts_with("_net")
}

/// A name fit for printing on the drawing: anonymous copper (whatever
/// spelling the engine or the island namer gave it) never is.
fn displayable(name: &str) -> bool {
    !is_anon(name)
}

/// Drawn anchor of one boundary port on a flat tile: the port terminal's pin,
/// else the boundary net's own wire - the same anchors the flat boundary
/// labels use, so an inter-module wire that lands here lands on copper.
fn flat_port_anchor(
    graph: &McVecGraph,
    trees: &[EquiTree],
    xf: &Xform,
    port_name: &str,
) -> Option<(f64, f64)> {
    let terminal = graph.boxes.iter().find(|b| {
        b.kind == BoxKind::PortTerminal
            && b.boundary_ports.iter().any(|p| p.port_name == port_name)
    });
    if let Some(b) = terminal {
        if let Some(ep) = b.entry_points.first() {
            return Some(anchor_mm(xf, b, ep.side, ep.offset));
        }
    }
    let net = graph
        .nets
        .iter()
        .find(|n| n.boundary.as_ref().map(|b| b.port_name == port_name).unwrap_or(false))?;
    boundary_tree_endpoint(graph, trees, xf, net)
}

/// Does a horizontal segment at `y` cross a foreign tile rect?
fn hseg_hits(x1: f64, x2: f64, y: f64, rects: &[(f64, f64, f64, f64)], skip: &[usize]) -> bool {
    let (lo, hi) = if x1 <= x2 { (x1, x2) } else { (x2, x1) };
    rects.iter().enumerate().any(|(i, r)| {
        !skip.contains(&i)
            && y > r.1
            && y < r.1 + r.3
            && hi > r.0
            && lo < r.0 + r.2
    })
}

/// Vertical counterpart of [`hseg_hits`].
fn vseg_hits(x: f64, y1: f64, y2: f64, rects: &[(f64, f64, f64, f64)], skip: &[usize]) -> bool {
    let (lo, hi) = if y1 <= y2 { (y1, y2) } else { (y2, y1) };
    rects.iter().enumerate().any(|(i, r)| {
        !skip.contains(&i)
            && x > r.0
            && x < r.0 + r.2
            && hi > r.1
            && lo < r.1 + r.3
    })
}

/// One wire between two sheet-mm points, Z-routed through the horizontal
/// midpoint (straight on degenerate axes).
fn flat_wire(bid: i64, x1: f64, y1: f64, x2: f64, y2: f64, e: &mut Emit) {
    let pts: Vec<(f64, f64)> = if (x1 - x2).abs() < 0.01 || (y1 - y2).abs() < 0.01 {
        vec![(x1, y1), (x2, y2)]
    } else {
        let xm = (x1 + x2) / 2.0;
        vec![(x1, y1), (xm, y1), (xm, y2), (x2, y2)]
    };
    for w in pts.windows(2) {
        e.open("wire");
        e.open("pts");
        line!(e, "(xy {} {})", mm(w[0].0), mm(w[0].1));
        line!(e, "(xy {} {})", mm(w[1].0), mm(w[1].1));
        e.close();
        e.open("stroke");
        line!(e, "(width 0)");
        line!(e, "(type default)");
        e.close();
        line!(
            e,
            "(uuid \"{}\")",
            det_uuid(&format!("{bid}#fw#{}#{}#{}#{}", w[0].0, w[0].1, w[1].0, w[1].1))
        );
        e.close();
    }
}

/// The copper island a layer net sits on, read through ONE endpoint's full
/// instance path (`main.MCU513.UC.1`) — the same spelling
/// `island_nets(Hierarchical)` keys on. A net no endpoint of which lands on a
/// table point (a pure drawing label) keeps its own name.
fn island_name_of_net(
    graph: &McVecGraph,
    net: &VizNet,
    islands: &HashMap<String, String>,
) -> Option<String> {
    for e in &net.endpoints {
        let Some(b) = graph.boxes.iter().find(|b| b.id == e.box_id) else {
            continue;
        };
        let path = format!("{}.{}", b.inst_path, e.pin_name);
        if let Some(name) = islands.get(&path) {
            return Some(name.clone());
        }
    }
    None
}

// === lib symbols ===

/// Lib symbol identity: same class, same pins (number, name, side, position)
/// and the same extent = one shared symbol. The pin positions are part of the
/// identity because they ARE the geometry the wires were routed to.
fn lib_signature(b: &McVecBox) -> String {
    let mut sig = format!("{}|{}x{}|", b.class_name, q(b.w), q(b.h));
    let mut pins: Vec<String> = b
        .pins
        .iter()
        .map(|p| {
            let (side, offset) = pin_placement(b, p);
            let along = match side {
                EntrySide::Top | EntrySide::Bottom => b.w,
                EntrySide::Left | EntrySide::Right => b.h,
            };
            format!(
                "{}|{}|{}|{}",
                p.pin_id,
                side_letter(side),
                q(offset * along),
                p.description
            )
        })
        .collect();
    pins.sort();
    sig.push_str(&pins.join("#"));
    sig
}

fn side_letter(s: EntrySide) -> &'static str {
    match s {
        EntrySide::Top => "T",
        EntrySide::Bottom => "B",
        EntrySide::Left => "L",
        EntrySide::Right => "R",
    }
}

/// One lib symbol body, in KiCad's two-unit convention: `_0_1` carries the
/// graphics, `_1_1` the pins. Pin internal positions mirror the viz anchor (x
/// unchanged, y negated — symbol space is y-up), so a wire routed to an anchor
/// lands on the pin connection point of an instance placed at the box corner.
fn lib_symbol_body(b: &McVecBox, name: &str) -> String {
    let mut e = Emit::new();
    e.open(&format!("symbol \"mcc:{name}\""));
    e.open("pin_numbers");
    line!(e, "(hide yes)");
    e.close();
    e.open("pin_names");
    line!(e, "(offset 0.508)");
    e.close();
    line!(e, "(exclude_from_sim no)");
    line!(e, "(in_bom yes)");
    line!(e, "(on_board yes)");
    let ref_letter = match b.symbol {
        Symbol::Resistor => "R",
        Symbol::Capacitor | Symbol::PolarCapacitor => "C",
        Symbol::Inductor => "L",
        Symbol::Diode | Symbol::Led | Symbol::Zener => "D",
        _ => "U",
    };
    property(&mut e, "Reference", ref_letter, 0.0, 0.0, false);
    property(&mut e, "Value", &b.class_name, 0.0, 0.0, false);
    property(&mut e, "Footprint", "", 0.0, 0.0, true);
    property(&mut e, "Datasheet", "", 0.0, 0.0, true);
    property(&mut e, "Description", &b.class_name, 0.0, 0.0, true);

    // Graphics: a body rect inset by the pin lead on sides that carry pins,
    // plus kind-specific marks for the horizontal two-pin passives.
    e.open(&format!("symbol \"{name}_0_1\""));
    let has = |side: EntrySide| b.pins.iter().any(|p| pin_placement(b, p).0 == side);
    let lead = PIN_LEN_PX * MM_PER_PX;
    let x0 = if has(EntrySide::Left) { lead } else { 0.0 };
    let x1 = b.w * MM_PER_PX - if has(EntrySide::Right) { lead } else { 0.0 };
    let y_top = -if has(EntrySide::Top) { lead } else { 0.0 };
    let y_bot = -(b.h * MM_PER_PX - if has(EntrySide::Bottom) { lead } else { 0.0 });
    let horizontal = has(EntrySide::Left) && has(EntrySide::Right) && b.w >= b.h;
    match b.symbol {
        Symbol::Capacitor | Symbol::PolarCapacitor if horizontal => {
            let cx = b.w * MM_PER_PX / 2.0;
            for px in [cx - 1.016, cx + 1.016] {
                polyline(&mut e, &[(px, y_top), (px, y_bot)], "none", 0.254);
            }
        }
        Symbol::Diode | Symbol::Led | Symbol::Zener if horizontal => {
            let cx = b.w * MM_PER_PX / 2.0;
            let my = (y_top + y_bot) / 2.0;
            let half = 1.778;
            polyline(
                &mut e,
                &[
                    (cx - half, my - half),
                    (cx - half, my + half),
                    (cx + half, my),
                    (cx - half, my - half),
                ],
                "outline",
                0.254,
            );
            polyline(
                &mut e,
                &[(cx + half, my - half), (cx + half, my + half)],
                "none",
                0.254,
            );
        }
        _ => {
            e.open("rectangle");
            line!(e, "(start {} {})", mm(x0), mm(y_top));
            line!(e, "(end {} {})", mm(x1), mm(y_bot));
            e.open("stroke");
            line!(e, "(width 0.254)");
            line!(e, "(type default)");
            e.close();
            e.open("fill");
            line!(
                e,
                "(type {})",
                if matches!(b.kind, BoxKind::MultiPin) {
                    "background"
                } else {
                    "none"
                }
            );
            e.close();
            e.close();
        }
    }
    e.close();

    e.open(&format!("symbol \"{name}_1_1\""));
    for p in &b.pins {
        let (side, offset) = pin_placement(b, p);
        let (ix, iy, angle) = internal_pin(b, side, offset);
        let name_txt = if p.description.is_empty() {
            "~"
        } else {
            p.description.as_str()
        };
        e.open(&format!("pin {} line", pin_elec_type(p.io)));
        line!(e, "(at {} {} {})", mm(ix), mm(iy), angle);
        line!(e, "(length {})", mm(lead));
        pin_text(&mut e, "name", name_txt);
        pin_text(&mut e, "number", &p.pin_id);
        e.close();
    }
    e.close();
    line!(e, "(embedded_fonts no)");
    e.close();
    e.s
}

/// Symbol-internal position and angle of one anchor. Symbol space is y-up, so
/// the y coordinate is the negated offset from the box top; the angle is the
/// direction from the connection point toward the body (screen convention:
/// 0 = right, 90 = up, 180 = left, 270 = down).
fn internal_pin(b: &McVecBox, side: EntrySide, offset: f64) -> (f64, f64, &'static str) {
    let w = b.w * MM_PER_PX;
    let h = b.h * MM_PER_PX;
    match side {
        EntrySide::Left => (0.0, -(h * offset), "0"),
        EntrySide::Right => (w, -(h * offset), "180"),
        EntrySide::Top => (w * offset, 0.0, "270"),
        EntrySide::Bottom => (w * offset, -h, "90"),
    }
}

fn pin_text(e: &mut Emit, tag: &str, text: &str) {
    e.open(&format!("{tag} \"{}\"", escape(text)));
    e.open("effects");
    e.open("font");
    line!(e, "(size 1.27 1.27)");
    e.close();
    e.close();
    e.close();
}

fn polyline(e: &mut Emit, pts: &[(f64, f64)], fill: &str, width: f64) {
    e.open("polyline");
    e.open("pts");
    for (x, y) in pts {
        line!(e, "(xy {} {})", mm(*x), mm(*y));
    }
    e.close();
    e.open("stroke");
    line!(e, "(width {})", mm(width));
    line!(e, "(type default)");
    e.close();
    e.open("fill");
    line!(e, "(type {fill})");
    e.close();
    e.close();
}

fn pin_elec_type(io: IoDirection) -> &'static str {
    match io {
        IoDirection::Input => "input",
        IoDirection::Output => "output",
        IoDirection::Bidir => "bidirectional",
        IoDirection::Power | IoDirection::Ground => "power_in",
        _ => "passive",
    }
}

/// Built-in power glyphs: the connection point is the symbol origin (pin
/// length 0, exactly like KiCad's own power symbols), so a glyph dropped at a
/// pin anchor or a tree symbol position joins the wire with no extra stub.
fn lib_gnd_body() -> String {
    let mut e = Emit::new();
    e.open("symbol \"mcc:GND\"");
    line!(e, "(power)");
    e.open("pin_numbers");
    line!(e, "(hide yes)");
    e.close();
    e.open("pin_names");
    line!(e, "(offset 0)");
    line!(e, "(hide yes)");
    e.close();
    line!(e, "(exclude_from_sim no)");
    line!(e, "(in_bom yes)");
    line!(e, "(on_board yes)");
    property(&mut e, "Reference", "#PWR", 0.0, -6.35, true);
    property(&mut e, "Value", "GND", 0.0, -3.81, false);
    property(&mut e, "Footprint", "", 0.0, 0.0, true);
    polyline(
        &mut e,
        &[
            (0.0, 0.0),
            (0.0, -1.27),
            (1.27, -1.27),
            (0.0, -2.54),
            (-1.27, -1.27),
            (0.0, -1.27),
        ],
        "none",
        0.0,
    );
    e.open("symbol \"GND_1_1\"");
    e.open("pin power_in line");
    line!(e, "(at 0 0 270)");
    line!(e, "(length 0)");
    pin_text(&mut e, "name", "~");
    pin_text(&mut e, "number", "1");
    e.close();
    e.close();
    line!(e, "(embedded_fonts no)");
    e.close();
    e.s
}

fn lib_pwr_body() -> String {
    let mut e = Emit::new();
    e.open("symbol \"mcc:PWR\"");
    line!(e, "(power)");
    e.open("pin_numbers");
    line!(e, "(hide yes)");
    e.close();
    e.open("pin_names");
    line!(e, "(offset 0)");
    line!(e, "(hide yes)");
    e.close();
    line!(e, "(exclude_from_sim no)");
    line!(e, "(in_bom yes)");
    line!(e, "(on_board yes)");
    property(&mut e, "Reference", "#PWR", 0.0, 6.35, true);
    property(&mut e, "Value", "PWR", 0.0, 3.81, false);
    property(&mut e, "Footprint", "", 0.0, 0.0, true);
    polyline(&mut e, &[(0.0, 0.0), (0.0, 1.27)], "none", 0.0);
    e.open("circle");
    line!(e, "(center 0 2.54)");
    line!(e, "(radius 1.27)");
    e.open("stroke");
    line!(e, "(width 0)");
    line!(e, "(type default)");
    e.close();
    e.open("fill");
    line!(e, "(type outline)");
    e.close();
    e.close();
    e.open("symbol \"PWR_1_1\"");
    e.open("pin power_in line");
    line!(e, "(at 0 0 90)");
    line!(e, "(length 0)");
    pin_text(&mut e, "name", "~");
    pin_text(&mut e, "number", "1");
    e.close();
    e.close();
    line!(e, "(embedded_fonts no)");
    e.close();
    e.s
}

fn lib_flag_body() -> String {
    let mut e = Emit::new();
    e.open("symbol \"mcc:PWR_FLAG\"");
    line!(e, "(power)");
    e.open("pin_numbers");
    line!(e, "(hide yes)");
    e.close();
    e.open("pin_names");
    line!(e, "(offset 0)");
    line!(e, "(hide yes)");
    e.close();
    line!(e, "(exclude_from_sim no)");
    line!(e, "(in_bom yes)");
    line!(e, "(on_board yes)");
    property(&mut e, "Reference", "#FLG", 0.0, 1.905, true);
    property(&mut e, "Value", "PWR_FLAG", 0.0, 3.81, false);
    property(&mut e, "Footprint", "", 0.0, 0.0, true);
    // KiCad's own PWR_FLAG: the driving pin lives in unit 0 (common), the
    // flag graphic in unit 1's body namespace.
    e.open("symbol \"PWR_FLAG_0_0\"");
    e.open("pin power_out line");
    line!(e, "(at 0 0 90)");
    line!(e, "(length 0)");
    pin_text(&mut e, "name", "~");
    pin_text(&mut e, "number", "1");
    e.close();
    e.close();
    e.open("symbol \"PWR_FLAG_0_1\"");
    polyline(
        &mut e,
        &[
            (0.0, 0.0),
            (0.0, 1.27),
            (-1.016, 1.905),
            (0.0, 2.54),
            (1.016, 1.905),
            (0.0, 1.27),
        ],
        "none",
        0.0,
    );
    e.close();
    line!(e, "(embedded_fonts no)");
    e.close();
    e.s
}

fn has_power_symbols(graph: &McVecGraph, trees: &[EquiTree]) -> bool {
    !graph.rail_decorations.is_empty()
        || trees.iter().any(|t| {
            t.symbols
                .iter()
                .any(|s| matches!(s.kind, TreeSymbolKind::Ground | TreeSymbolKind::Power))
        })
}

// === Shared helpers ===

fn paper_for(graph: &McVecGraph, trees: &[EquiTree]) -> String {
    let xf = Xform::new(graph, trees);
    let Some(first) = graph.boxes.first() else {
        return "\"A4\"".into();
    };
    let mut max_x = first.x + first.w;
    let mut max_y = first.y + first.h;
    for b in &graph.boxes {
        max_x = max_x.max(b.x + b.w);
        max_y = max_y.max(b.y + b.h);
    }
    for t in trees {
        for s in &t.segments {
            max_x = max_x.max(s.x1).max(s.x2);
            max_y = max_y.max(s.y1).max(s.y2);
        }
    }
    if let Some(f) = &graph.module_frame {
        max_x = max_x.max(f.x + f.w);
        max_y = max_y.max(f.y + f.h);
    }
    let w = xf.x(max_x) + 10.0;
    let h = xf.y(max_y) + 10.0;
    // A-series, either orientation; a drawing larger than A0 gets a sheet
    // sized to itself.
    const SIZES: [(f64, f64, &str); 5] = [
        (297.0, 210.0, "A4"),
        (420.0, 297.0, "A3"),
        (594.0, 420.0, "A2"),
        (841.0, 594.0, "A1"),
        (1189.0, 841.0, "A0"),
    ];
    for (lw, lh, name) in SIZES {
        if (w <= lw && h <= lh) || (w <= lh && h <= lw) {
            return format!("\"{name}\"");
        }
    }
    format!("\"User\" {} {}", w.ceil() as i64, h.ceil() as i64)
}

/// Deterministic uuid-shaped id for a stable seed. Only the textual shape is
/// contractual — KiCad treats these as opaque strings, and the seed is built
/// from stable identities (top name, layer bid, element key), never from
/// allocation order.
fn det_uuid(seed: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h1 = std::collections::hash_map::DefaultHasher::new();
    seed.hash(&mut h1);
    let a = h1.finish();
    let mut h2 = std::collections::hash_map::DefaultHasher::new();
    format!("{seed}#2").hash(&mut h2);
    let b = h2.finish();
    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        (a >> 32) as u32,
        (a & 0xffff) as u16,
        ((b >> 48) & 0x0fff) as u16,
        ((b >> 32) & 0xffff) as u16,
        b & 0xffff_ffff_ffff
    )
}

fn q(v: f64) -> u32 {
    (v * 100.0).round() as u32
}

/// Sheet-mm text: up to 4 decimals, trailing zeros trimmed (KiCad's own style).
fn mm(v: f64) -> String {
    let s = format!("{:.4}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".into()
    } else {
        s.to_string()
    }
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn sanitize_file_stem(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn sanitize_lib_id(s: &str) -> String {
    sanitize_file_stem(s)
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::boxdef::{BoxPin, IoSummary};
    use crate::vector::graph::netdef::{EndpointRef, NetRole, Point, Route, Segment};
    use crate::vector::graph::{NetKind, PortDir};

    fn two_pin_box(id: i64, name: &str, class: &str, x: f64, y: f64) -> McVecBox {
        let mut b = McVecBox::new_v2(
            id,
            name.into(),
            class.into(),
            BoxKind::TwoPin,
            Symbol::Resistor,
            Some(name.into()),
            None,
            2,
            IoSummary::new(),
            name.into(),
            Vec::new(),
        );
        b.x = x;
        b.y = y;
        b.w = 40.0;
        b.h = 20.0;
        b
    }

    fn add_pin(b: &mut McVecBox, pin_id: i64, num: &str, side: EntrySide, offset: f64) {
        b.pins.push(BoxPin {
            id: pin_id,
            pin_id: num.into(),
            description: String::new(),
            io: IoDirection::Passive,
            port_dir: PortDir::None,
            src_span: None,
            point: None,
        });
        b.entry_points.push(EntryPoint {
            pin_id,
            pin_name: num.into(),
            side,
            offset,
        });
    }

    fn net(nid: i64, name: &str, eps: Vec<(i64, i64)>, segs: Vec<(f64, f64, f64, f64)>) -> VizNet {
        let mut n = VizNet::new(
            nid,
            name.into(),
            NetKind::Signal,
            NetRole::Signal,
            eps.into_iter()
                .map(|(b, p)| EndpointRef::new(b, p, "1"))
                .collect(),
        );
        let mut r = Route::new();
        for (x1, y1, x2, y2) in segs {
            r.segments.push(Segment {
                from: Point::new(x1, y1),
                to: Point::new(x2, y2),
            });
        }
        n.route = Some(r);
        n
    }

    fn block_graph() -> McVecGraph {
        let mut g = McVecGraph::new(1, "top".into());
        g.is_root = true;
        g.layer_style = LayerStyle::Block;
        let mut r = two_pin_box(10, "R1", "RES", 100.0, 100.0);
        add_pin(&mut r, 100, "1", EntrySide::Left, 0.5);
        add_pin(&mut r, 101, "2", EntrySide::Right, 0.5);
        let mut c = two_pin_box(11, "C1", "CAP", 300.0, 100.0);
        add_pin(&mut c, 200, "1", EntrySide::Left, 0.5);
        add_pin(&mut c, 201, "2", EntrySide::Right, 0.5);
        g.boxes.push(r);
        g.boxes.push(c);
        // R1.2 -> C1.1, through both anchors.
        g.nets.push(net(
            1,
            "MID",
            vec![(10, 101), (11, 200)],
            vec![
                (140.0, 110.0, 220.0, 110.0),
                (220.0, 110.0, 220.0, 120.0),
                (220.0, 120.0, 300.0, 120.0),
            ],
        ));
        g
    }

    fn one_layer(g: McVecGraph) -> Vec<RenderedLayer> {
        vec![RenderedLayer {
            graph: g,
            parent: None,
            canvas: (500.0, 300.0),
            audited: true,
        }]
    }

    #[test]
    fn uuid_is_deterministic_and_shaped() {
        let a = det_uuid("top#root");
        let b = det_uuid("top#root");
        assert_eq!(a, b);
        assert_eq!(a.len(), 36);
        assert_eq!(a.chars().filter(|c| *c == '-').count(), 4);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
        assert_ne!(det_uuid("a"), det_uuid("b"));
    }

    #[test]
    fn mm_trims_trailing_zeros() {
        assert_eq!(mm(10.0), "10");
        assert_eq!(mm(2.54), "2.54");
        assert_eq!(mm(-0.00001), "0");
        assert_eq!(mm(1.00004), "1");
    }

    #[test]
    fn block_layer_wires_land_on_pin_anchors() {
        let g = block_graph();
        let layers = one_layer(g);
        let files = emit_sheets(&layers, "top");
        assert_eq!(files.len(), 1);
        let s = &files[0].content;

        assert!(s.contains("(lib_id \"mcc:RES\")"), "{s}");
        // The wire starts exactly at the pin anchor: box (100,100) 40x20,
        // right pin at offset 0.5 -> (140,110) px.
        let xf = Xform::new(&layers[0].graph, &[]);
        let (ax, ay) = anchor_mm(&xf, &layers[0].graph.boxes[0], EntrySide::Right, 0.5);
        let expect = format!("(xy {} {}", mm(ax), mm(ay));
        std::fs::write("/tmp/test_block.kicad_sch", s).unwrap();
        assert!(
            s.contains(&expect),
            "wire must start at the pin anchor {expect}"
        );
        assert!(s.contains("(label \"MID\""), "{s}"); // label present (ledger may slide it)
        assert!(!s.contains("mcc:GND"), "{s}");
        assert!(s.contains("(sheet_instances"), "{s}");
        // Symbol placed at the box corner in sheet mm.
        let (bx, by) = (xf.x(100.0), xf.y(100.0));
        assert!(
            s.contains(&format!("(at {} {} 0)", mm(bx), mm(by))),
            "symbol at the box corner: {s}"
        );
    }

    #[test]
    fn no_connect_lands_on_unwired_anchor() {
        let mut g = block_graph();
        g.nets.clear();
        let s = emit_sheets(&one_layer(g), "top")[0].content.clone();
        assert_eq!(s.matches("(no_connect").count(), 4, "{s}");
    }

    #[test]
    fn wire_anchors_match_internal_pin_math() {
        // The connection-point arithmetic must agree in both directions: the
        // instance sits at the box corner, so the pin at internal (dx, dy)
        // lands at (X + dx, Y - dy) = the anchor's sheet coordinates.
        let g = block_graph();
        let layers = one_layer(g);
        let xf = Xform::new(&layers[0].graph, &[]);
        let b = &layers[0].graph.boxes[0];
        let ep = &b.entry_points[1]; // right pin, offset 0.5
        let pin = &b.pins[1];
        let (side, offset) = pin_placement(b, pin);
        let (dx, dy, _) = internal_pin(b, side, offset);
        let (ax, ay) = anchor_mm(&xf, b, ep.side, ep.offset);
        let (ix, iy) = (xf.x(b.x) + dx, xf.y(b.y) - dy);
        assert_eq!(format!("{:.4}", ix), format!("{:.4}", ax));
        assert_eq!(format!("{:.4}", iy), format!("{:.4}", ay));
    }

    #[test]
    fn hierarchy_emits_sheet_pins_and_hierarchical_labels() {
        // Root: one SubModule box with one boundary port; child: a Device
        // layer whose boundary net carries the port identity.
        let mut root = McVecGraph::new(1, "top".into());
        root.is_root = true;
        root.layer_style = LayerStyle::Block;
        let mut sub = McVecBox::new_v2(
            20,
            "ldo".into(),
            "LDO".into(),
            BoxKind::SubModule,
            Symbol::Module,
            Some("ldo".into()),
            None,
            1,
            IoSummary::new(),
            "top.ldo".into(),
            Vec::new(),
        );
        sub.x = 200.0;
        sub.y = 100.0;
        sub.w = 80.0;
        sub.h = 60.0;
        sub.provenance = BoxProvenance::Declared;
        sub.pins.push(BoxPin {
            id: 300,
            pin_id: "vin".into(),
            description: String::new(),
            io: IoDirection::Input,
            port_dir: PortDir::In,
            src_span: None,
            point: None,
        });
        sub.entry_points.push(EntryPoint {
            pin_id: 300,
            pin_name: "vin".into(),
            side: EntrySide::Left,
            offset: 0.5,
        });
        sub.boundary_ports.push(crate::vector::graph::BoundaryPort {
            entry_pin_id: 300,
            port_name: "vin".into(),
            io: IoDirection::Input,
        });
        root.boxes.push(sub);
        root.clickable_subs.push(20);

        let mut child = McVecGraph::new(20, "ldo".into());
        child.layer_style = LayerStyle::Device;
        let mut n = VizNet::new(
            7,
            "vin_net".into(),
            NetKind::Power,
            NetRole::Signal,
            vec![EndpointRef::new(99, 900, "vin")],
        );
        n.boundary = Some(crate::vector::model::net::BoundaryInfo {
            port_group_id: 5,
            port_name: "vin".into(),
            io: IoDirection::Input,
            flow: None,
            is_supply: false,
        });
        let mut r = Route::new();
        r.segments.push(Segment {
            from: Point::new(120.0, 40.0),
            to: Point::new(160.0, 40.0),
        });
        n.route = Some(r);
        child.nets.push(n);
        // A port terminal box at the boundary crossing.
        let mut pt = McVecBox::new_v2(
            99,
            "vin".into(),
            String::new(),
            BoxKind::PortTerminal,
            Symbol::Unknown,
            None,
            None,
            1,
            IoSummary::new(),
            "top.ldo.vin".into(),
            Vec::new(),
        );
        pt.x = 110.0;
        pt.y = 30.0;
        pt.w = 20.0;
        pt.h = 20.0;
        pt.provenance = BoxProvenance::Declared;
        pt.pins.push(BoxPin {
            id: 900,
            pin_id: "vin".into(),
            description: String::new(),
            io: IoDirection::Input,
            port_dir: PortDir::In,
            src_span: None,
            point: None,
        });
        pt.entry_points.push(EntryPoint {
            pin_id: 900,
            pin_name: "vin".into(),
            side: EntrySide::Right,
            offset: 0.5,
        });
        pt.boundary_ports.push(crate::vector::graph::BoundaryPort {
            entry_pin_id: 900,
            port_name: "vin".into(),
            io: IoDirection::Input,
        });
        child.boxes.push(pt);

        let layers = vec![
            RenderedLayer {
                graph: root,
                parent: None,
                canvas: (600.0, 300.0),
                audited: true,
            },
            RenderedLayer {
                graph: child,
                parent: Some(1),
                canvas: (400.0, 200.0),
                audited: false,
            },
        ];
        let files = emit_sheets(&layers, "top");
        assert_eq!(files.len(), 2);
        let root_s = &files[0].content;
        let child_s = &files[1].content;

        // The sheet pin sits exactly at the sub box's lead anchor.
        let xf = Xform::new(&layers[0].graph, &[]);
        let sub = &layers[0].graph.boxes[0];
        let (px, py) = anchor_mm(&xf, sub, EntrySide::Left, 0.5);
        assert!(root_s.contains("(pin \"vin\" input"), "{root_s}");
        assert!(
            root_s.contains(&format!("(at {} {} 180)", mm(px), mm(py))),
            "sheet pin at the boundary anchor: {root_s}"
        );
        // The child carries the matching hierarchical label, anchored on the
        // port terminal's pin.
        assert!(child_s.contains("(hierarchical_label \"vin\""), "{child_s}");
        let cxf = Xform::new(&layers[1].graph, &[]);
        let term = &layers[1].graph.boxes[0];
        let (cx, cy) = anchor_mm(&cxf, term, EntrySide::Right, 0.5);
        assert!(
            child_s.contains(&format!("(at {} {} 180)", mm(cx), mm(cy))),
            "hierarchical label at the terminal pin: {child_s}"
        );
        // The child file is referenced by the sheet and its own header uuid
        // differs from the sheet element uuid.
        assert!(root_s.contains("top_ldo.kicad_sch"), "{root_s}");
        assert!(child_s.contains("(uuid \""));
    }

    #[test]
    fn flat_sheet_has_no_hierarchy() {
        // The flat face is the whole board on one sheet: no sheet instances,
        // no hierarchical labels, no top block diagram — just parts joined by
        // net name.
        let g = block_graph();
        let mut root = McVecGraph::new(1, "top".into());
        root.is_root = true;
        root.layer_style = LayerStyle::Block;
        let mut sub = McVecBox::new_v2(
            20,
            "ldo".into(),
            "LDO".into(),
            BoxKind::SubModule,
            Symbol::Module,
            Some("ldo".into()),
            None,
            0,
            IoSummary::new(),
            "top.ldo".into(),
            Vec::new(),
        );
        sub.provenance = BoxProvenance::Declared;
        sub.x = 10.0;
        sub.y = 10.0;
        sub.w = 40.0;
        sub.h = 30.0;
        root.boxes.push(sub);
        root.clickable_subs.push(20);
        let mut child = McVecGraph::new(20, "ldo".into());
        child.layer_style = LayerStyle::Device;
        let mut dev = McVecBox::new_v2(
            30,
            "U1".into(),
            "LDO".into(),
            BoxKind::MultiPin,
            Symbol::Unknown,
            Some("U1".into()),
            None,
            0,
            IoSummary::new(),
            "top.ldo.U1".into(),
            Vec::new(),
        );
        dev.provenance = BoxProvenance::Declared;
        dev.x = 10.0;
        dev.y = 10.0;
        dev.w = 40.0;
        dev.h = 30.0;
        child.boxes.push(dev);
        let layers = vec![
            RenderedLayer { graph: root, parent: None, canvas: (200.0, 100.0), audited: true },
            RenderedLayer { graph: child, parent: Some(1), canvas: (200.0, 100.0), audited: false },
        ];
        let files = emit_sheets(&layers, "top");
        assert_eq!(files.len(), 2, "hierarchical mode keeps both sheets");

        // flat mode: build via the same path the CLI takes
        let flat = super::emit_flat_sheet(&layers, &dummy_table(), "top");
        assert_eq!(flat.len(), 1);
        let s = &flat[0].content;
        assert!(!s.contains("(sheet\n"), "no sheet instances: {s}");
        assert!(!s.contains("hierarchical_label"), "no hierarchical labels: {s}");
        assert!(s.contains("(lib_id \"mcc:L20_"), "device content present: {s}");
    }

    fn dummy_table() -> crate::InstTable {
        crate::InstTable::new(1)
    }

    #[test]
    fn s_expression_parens_balance() {
        let g = block_graph();
        for f in emit_sheets(&one_layer(g), "top") {
            let mut depth: i64 = 0;
            let mut in_str = false;
            let mut prev_esc = false;
            for ch in f.content.chars() {
                if in_str {
                    if prev_esc {
                        prev_esc = false;
                    } else if ch == '\\' {
                        prev_esc = true;
                    } else if ch == '"' {
                        in_str = false;
                    }
                    continue;
                }
                match ch {
                    '"' => in_str = true,
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
            }
            assert_eq!(depth, 0, "unbalanced parens in {}", f.name);
        }
    }
}
