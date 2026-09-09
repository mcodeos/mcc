// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Top-level power-flow single view — v0.1 **derived generator**.
//!
//! `mcc show pwrflow` (design: `mcd/doc/power-signal/power-flow-single-view-design.md`)
//! turns one flat build into the *generative* single view of "how this board's
//! power flows", replacing the hand-written `power-flow-view.md` block:
//!
//!   1. **world crowns** — resolvable return/reference copper (flat island
//!      role `Ret`/`Reference`) × the owning scope's conduit `@role`
//!      (main / quiet / isolated / earth);
//!   2. **rail contract table** — each declared DC domain rail (hot net /
//!      return copper / world / nominal / gen spine / loads / decouplers);
//!   3. **supply tree** — producer chains from export source faces and
//!      `psrc`/`psbi` faces through pass-through elements and converter
//!      components down to the domain rails and their sink loads.
//!
//! Predicates are structural, never class-name lists: a *pass-through* is a
//! component whose def carries **no** `psrc/psnk/psbi` contract and exactly two
//! wired pins on supply-class nets; a *converter / merge* is any component that
//! *does* carry power contracts. Bus-vs-rail is split on declared `domain`
//! rails vs merely-named supply nets (the flat-island `Hot`/`Signal` roles).
//!
//! Flat pin → def pin resolution: flatten names a component pin by its def pin
//! id, but the def-side member *names* registered for that id are the semantic
//! spellings a `psrc/psnk/psbi` contract's `hot`/`ret` use (bracket row `[VDD,
//! GND]` and group row `OUT{Vout, GND}` alike — McPwrPin keeps the dotted
//! member spelling). A flat pin is matched to its contract terminal by looking
//! up the def pin whose registered names carry the wanted spelling; the pin's
//! wired flat net is then the terminal's net. This is the bridge that lets one
//! contract matcher see both flatten spellings without a path-leaf assumption.

use crate::instant::insttab::{InstEntry, InstKind, InstTable};
use crate::instant::island::{NetIslandIndex, NetRole};
use crate::semantic::common::IOType;
use crate::semantic::component::mc_pins::PwrDir;
use crate::semantic::component::McComponent;
use crate::semantic::module::pi::L1Rail;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

// ────────────────────────────────────────────────────────────────────────────
// Public typed view (lib.rs re-exports these to cmds + tests)
// ────────────────────────────────────────────────────────────────────────────

/// One world-crown row: return/reference copper + its conduit world.
#[derive(Debug, Clone, Default)]
pub struct CrownRow {
    /// Return copper / conduit name (`GND`, `GNDA`, `GND_ISO`, `EARTH`).
    pub copper: String,
    /// Conduit `@role` (`main`/`quiet`/`isolated`/`earth`; default `main`).
    pub world: String,
    /// Conduit carried `@star`.
    pub star: bool,
    /// True when some DC rail returns to this copper (`role == Ret`);
    /// false for a pure reference copper with no rail (`EARTH`).
    pub rail_return: bool,
}

/// One declared DC domain rail, projected for the single view.
#[derive(Debug, Clone, Default)]
pub struct RailRow {
    /// Domain name (`DVDD`).
    pub domain: String,
    /// Hot net name (`VDD_3V3`).
    pub hot: String,
    /// Return copper name (`GND`).
    pub ret: String,
    /// World of the return copper (`main`/`quiet`/`isolated`).
    pub world: String,
    /// Verbatim nominal (`3.3V`).
    pub v_text: String,
    pub v: Option<f64>,
    pub tol: Option<f64>,
    pub capacity_amps: Option<f64>,
    pub eff: Option<f64>,
    /// Producer spine text (`ldo33←VMAIN_5V`, `buck12←VMAIN_5V`,
    /// `FB_a←DVDD`). Pass-through elements between a converter output and the
    /// rail hot net are transparent here (they show in the §3 tree).
    pub gen: String,
    /// True when the edge from this rail's producer crosses worlds: producer
    /// input return copper ≠ this rail's return copper.
    pub cross_world: bool,
    /// Sink loads directly on the hot net, e.g. `uc{VDD, GND}`.
    pub loads: Vec<String>,
    /// Folded two-pad decoupler instance labels on the hot/return pair.
    pub decaps: Vec<String>,
}

/// One node of the §3 supply tree. The builder orders and nests nodes; the
/// text renderer walks `children` (fan-out), the JSON renderer keeps the tree.
#[derive(Debug, Clone)]
pub struct FlowNode {
    /// `source` | `bus` | `rail` | `load` | `note`.
    pub class: String,
    /// Stable identity (net name / component path / element path).
    pub id: String,
    /// Primary display label.
    pub label: String,
    /// Element that produces this node from its parent (`F1`, `oring`, …).
    pub via: Option<String>,
    /// World of this node's return copper.
    pub world: Option<String>,
    /// This node's net crosses worlds vs its producer.
    pub cross_world: bool,
    /// Secondary note (`(×n decaps gen …)`).
    pub note: String,
    /// Nested children (fan-out / sub-rails / loads).
    pub children: Vec<FlowNode>,
}

/// The whole derived power-flow single view.
#[derive(Debug, Clone, Default)]
pub struct PwrFlow {
    pub top: String,
    pub crown: Vec<CrownRow>,
    pub rails: Vec<RailRow>,
    /// §3 forest roots (source faces / loose rails), in display order.
    pub roots: Vec<FlowNode>,
}

// ────────────────────────────────────────────────────────────────────────────
// Scope
// ────────────────────────────────────────────────────────────────────────────

/// Trace hook — `MCC_PWRFLOW_TRACE=1` prints the scan on stderr (build aid).
fn trace(args: std::fmt::Arguments<'_>) {
    if std::env::var("MCC_PWRFLOW_TRACE").is_ok() {
        eprintln!("[pwrflow] {args}");
    }
}

struct Scope {
    top_id: u32,
    /// Declared DC rails of the top scope (l1_rails order = domain source order).
    rails: Vec<L1Rail>,
    /// conduit copper name → @role (default main).
    role: HashMap<String, String>,
    /// copper name carrying @star.
    star: HashSet<String>,
    island: NetIslandIndex,
}

impl Scope {
    fn att(&self, net_id: u32) -> Option<&crate::instant::island::NetAttribution> {
        self.island.get(net_id)
    }
    fn role_of(&self, copper: &str) -> String {
        self.role
            .get(copper)
            .cloned()
            .unwrap_or_else(|| "main".to_string())
    }
}

/// Flat net id of a declared rail hot name that has a live top-scope net.
fn rail_hot_net(scope: &Scope, hot: &str) -> Option<u32> {
    scope
        .island
        .nets()
        .find(|a| a.module == Some(scope.top_id) && a.resolvable && a.name == hot)
        .map(|a| a.net_id)
}

/// Flat net id of a resolvable return copper by name (top scope).
fn copper_net(scope: &Scope, copper: &str) -> Option<u32> {
    scope
        .island
        .nets()
        .find(|a| {
            a.module == Some(scope.top_id) && a.resolvable && a.copper.as_deref() == Some(copper)
        })
        .map(|a| a.net_id)
}

/// Index into `scope.rails` when `net` is the hot net of a declared rail.
fn is_rail_net(scope: &Scope, net: u32) -> Option<usize> {
    scope
        .rails
        .iter()
        .position(|r| rail_hot_net(scope, &r.hot) == Some(net))
}

fn rail_of_net(scope: &Scope, net: u32) -> Option<&L1Rail> {
    is_rail_net(scope, net).map(|i| &scope.rails[i])
}

/// The return copper name of a supply net when it is provable:
///  * the net is a resolvable return/reference copper → its own name;
///  * the net is a declared rail hot → that rail's declared return.
fn ret_of_net(scope: &Scope, net: u32) -> Option<String> {
    let a = scope.att(net)?;
    if !a.resolvable {
        return None;
    }
    match a.role {
        NetRole::Ret | NetRole::Reference => a.copper.clone(),
        NetRole::Hot => rail_of_net(scope, net).map(|r| r.ret.clone()),
        NetRole::Signal => None,
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Component / terminal model
// ────────────────────────────────────────────────────────────────────────────

/// One resolved power-contract terminal of a component: which flat nets the
/// contract's hot pin and return pin landed on.
#[derive(Debug, Clone)]
struct Term {
    /// Contract hot spelling (`VDD`, `OUT`, `LX.Lx`, `VIN.Vin`).
    hot: String,
    /// Contract return spelling (`GND`, `AGND`, dotted member).
    ret: Option<String>,
    /// Net of the contract hot pin.
    hot_net: Option<u32>,
    /// Net of the contract return pin (a resolvable return copper).
    ret_net: Option<u32>,
}

impl Term {
    /// Display `{hot, ret}` suffix used for load leaves.
    fn pair(&self) -> String {
        let ret = self.ret.clone().unwrap_or_default();
        format!("{}{{{}}}", self.hot, ret)
    }
}

struct Comp {
    leaf: String,
    def: Option<Arc<McComponent>>,
    /// Wired pins: (flat pin, primary net id).
    pins: Vec<(InstEntry, u32)>,
    /// Src/Bi (source-side) contracts + terminal nets.
    src: Vec<Term>,
    /// Snk (sink-side) contracts + terminal nets.
    snk: Vec<Term>,
    /// No pwr contract and exactly two wired pins → pass-through / decap / leg.
    two_pin: bool,
    /// Component def carries at least one pwr contract.
    has_pwr: bool,
}

impl Comp {
    fn src_nets(&self) -> impl Iterator<Item = u32> + '_ {
        self.src.iter().filter_map(|t| t.hot_net)
    }
    fn snk_nets(&self) -> impl Iterator<Item = u32> + '_ {
        self.snk.iter().filter_map(|t| t.hot_net)
    }
    fn src_on(&self, net: u32) -> bool {
        self.src.iter().any(|t| t.hot_net == Some(net))
    }
    fn snk_on(&self, net: u32) -> bool {
        self.snk.iter().any(|t| t.hot_net == Some(net))
    }
    /// psbi — conditional source (charge = sink, discharge = source).
    fn is_bi(&self) -> bool {
        self.def
            .as_ref()
            .is_some_and(|d| d.pins.pwr.iter().any(|c| c.dir == PwrDir::Bi))
    }
    /// Pure consumer (Snk contracts, no Src terminal).
    fn is_pure_sink(&self) -> bool {
        self.has_pwr && self.src.is_empty() && !self.snk.is_empty()
    }
}

/// Def-side names of a flat pin: the def McPin for this flat pin, resolved by
/// (1) exact physical-id path tail, (2) def pin whose registered `names`
/// contain the path tail or its leaf.
fn def_pin_names<'a>(
    def: &'a McComponent,
    comp_path: &str,
    pin: &InstEntry,
) -> Option<&'a Vec<String>> {
    let tail = pin.path.strip_prefix(comp_path)?.strip_prefix('.')?;
    let leaf = tail.rsplit('.').next().unwrap_or(tail);
    if let Some(mp) = def.pins.pins.get(tail) {
        return Some(&mp.names);
    }
    if let Some(mp) = def.pins.pins.get(leaf) {
        return Some(&mp.names);
    }
    def.pins.pins.values().find_map(|mp| {
        if mp.names.iter().any(|n| n == tail || n == leaf) {
            Some(&mp.names)
        } else {
            None
        }
    })
}

/// Whether this flat pin is the terminal named `wanted` of its def — true when
/// the def pin's registered names carry `wanted`, or the flat path literally
/// spells it (dotted group members: `buck12.LX.Lx` tail == contract hot
/// `LX.Lx`).
fn pin_is_terminal(def: &McComponent, comp_path: &str, pin: &InstEntry, wanted: &str) -> bool {
    let tail = pin
        .path
        .strip_prefix(comp_path)
        .and_then(|p| p.strip_prefix('.'));
    if tail == Some(wanted) {
        return true;
    }
    def_pin_names(def, comp_path, pin).is_some_and(|names| names.iter().any(|n| n == wanted))
}

/// Resolve every power contract of a resolved def to the flat nets its hot pin
/// and return pin landed on. Mirrors the ERC `source_contract_for` intent but
/// resolves terminals through the def pin name table rather than the flat path
/// leaf, so physical (`uc.5`) and dotted-group (`ldo33.VOUT.Vout`) spellings
/// both resolve. Returns (src+bi, snk) terminals.
fn contract_terms(
    def: &McComponent,
    comp_path: &str,
    pins: &[(InstEntry, u32)],
) -> (Vec<Term>, Vec<Term>) {
    let mut src = Vec::new();
    let mut snk = Vec::new();
    let find_net = |wanted: &str| {
        pins.iter()
            .find(|(p, _)| pin_is_terminal(def, comp_path, p, wanted))
            .map(|(_, n)| *n)
    };
    for c in &def.pins.pwr {
        let hot_net = find_net(&c.hot);
        let ret_net = c.ret.as_deref().and_then(find_net);
        let term = Term {
            hot: c.hot.clone(),
            ret: c.ret.clone(),
            hot_net,
            ret_net,
        };
        match c.dir {
            PwrDir::Src | PwrDir::Bi => src.push(term),
            PwrDir::Snk => snk.push(term),
        }
    }
    (src, snk)
}

fn collect_components(table: &InstTable, scope: &Scope) -> Vec<Comp> {
    let workspace: HashMap<String, Arc<McComponent>> = crate::definition_space()
        .workspace_components()
        .into_iter()
        .map(|(sn, c)| (sn.ident.to_string(), c))
        .collect();

    let mut out = Vec::new();
    for entry in table.children_of(scope.top_id) {
        if entry.kind != InstKind::Component {
            continue;
        }
        let def = workspace.get(&entry.class_name).cloned();
        let mut pins = Vec::new();
        for p in table.get_pins_of(entry.id) {
            if let Some(net) = table.get_net_of(p.id) {
                pins.push((p.clone(), net.id));
            }
        }
        let (src, snk) = match &def {
            Some(d) => contract_terms(d, &entry.path, &pins),
            None => (Vec::new(), Vec::new()),
        };
        let has_pwr = def.as_ref().is_some_and(|d| !d.pins.pwr.is_empty());
        let two_pin = !has_pwr && pins.len() == 2;
        let leaf = entry.path.rsplit('.').next().unwrap_or("").to_string();

        trace(format_args!(
            "comp {} class={} leaf={} def={} pins=[{}] src=[{}] snk=[{}] two_pin={}",
            entry.path,
            entry.class_name,
            leaf,
            def.is_some(),
            pins.iter()
                .map(|(p, n)| format!("{}(net {})", p.path, n))
                .collect::<Vec<_>>()
                .join(" "),
            src.iter()
                .map(|t| format!(
                    "{}({})",
                    t.hot,
                    t.hot_net.map(|x| x.to_string()).unwrap_or_default()
                ))
                .collect::<Vec<_>>()
                .join(" "),
            snk.iter()
                .map(|t| format!(
                    "{}({})",
                    t.hot,
                    t.hot_net.map(|x| x.to_string()).unwrap_or_default()
                ))
                .collect::<Vec<_>>()
                .join(" "),
            two_pin,
        ));

        out.push(Comp {
            leaf,
            def,
            pins,
            src,
            snk,
            two_pin,
            has_pwr,
        });
    }
    out
}

// ────────────────────────────────────────────────────────────────────────────
// Supply-class nets + source-distance orientation
// ────────────────────────────────────────────────────────────────────────────

/// Every top-scope supply net the analysis carries: declared rail hot nets and
/// nets that sit under a component power terminal or an export power port.
fn supply_nets(table: &InstTable, scope: &Scope, comps: &[Comp]) -> HashSet<u32> {
    let mut s: HashSet<u32> = HashSet::new();
    for r in &scope.rails {
        if let Some(n) = rail_hot_net(scope, &r.hot) {
            s.insert(n);
        }
    }
    for c in comps {
        s.extend(c.src_nets());
        s.extend(c.snk_nets());
    }
    for n in table.get_nets() {
        if n.module != Some(scope.top_id) {
            continue;
        }
        let exported = n.points.iter().any(|&pid| {
            table
                .get_entry(pid)
                .is_some_and(|e| e.kind == InstKind::Port && e.io_type == IOType::Power)
        });
        if exported {
            s.insert(n.id);
        }
    }
    s
}

/// Distance of every supply net from its source face (export power port or psbi
/// component output), used only to orient pass-through edges toward the source.
/// Converter edges are already directed (snk side → src side); a two-pin pass
/// between two supply nets is oriented from the closer-to-source net.
fn net_dist(
    table: &InstTable,
    scope: &Scope,
    comps: &[Comp],
    supply: &HashSet<u32>,
) -> HashMap<u32, usize> {
    let mut dist: HashMap<u32, usize> = HashMap::new();
    let mut q: VecDeque<u32> = VecDeque::new();
    for n in table.get_nets() {
        if n.module != Some(scope.top_id) {
            continue;
        }
        let exported = n.points.iter().any(|&pid| {
            table
                .get_entry(pid)
                .is_some_and(|e| e.kind == InstKind::Port && e.io_type == IOType::Power)
        });
        if exported && supply.contains(&n.id) {
            dist.insert(n.id, 0);
            q.push_back(n.id);
        }
    }
    for c in comps {
        // A psbi / psrc face root with no snk inputs seeds its output nets at 0.
        if c.has_pwr && c.snk.is_empty() {
            for n in c.src_nets() {
                if supply.contains(&n) && !dist.contains_key(&n) {
                    dist.insert(n, 0);
                    q.push_back(n);
                }
            }
        }
    }

    let mut pass: Vec<(u32, u32)> = Vec::new();
    for c in comps {
        if c.two_pin && c.pins.len() == 2 {
            let (a, b) = (c.pins[0].1, c.pins[1].1);
            if a != b && supply.contains(&a) && supply.contains(&b) {
                pass.push((a, b));
            }
        }
    }

    // Relaxation to a fixed point: converter output dist = max(snk dist)+1;
    // pass far net = near net dist + 1.
    loop {
        let mut changed = false;
        for c in comps {
            if !c.has_pwr {
                continue;
            }
            let inputs: Vec<u32> = c.snk_nets().filter(|n| dist.contains_key(n)).collect();
            if inputs.is_empty() {
                continue;
            }
            let base = inputs.iter().map(|n| dist[n]).max().unwrap_or(0) + 1;
            for s in c.src_nets() {
                if supply.contains(&s) && dist.get(&s).map_or(true, |&d| d > base) {
                    dist.insert(s, base);
                    changed = true;
                }
            }
        }
        for &(a, b) in &pass {
            match (dist.get(&a), dist.get(&b)) {
                (Some(&da), None) if supply.contains(&b) => {
                    dist.insert(b, da + 1);
                    changed = true;
                }
                (None, Some(&db)) if supply.contains(&a) => {
                    dist.insert(a, db + 1);
                    changed = true;
                }
                _ => {}
            }
        }
        if !changed {
            break;
        }
    }
    dist
}

// ────────────────────────────────────────────────────────────────────────────
// Build
// ────────────────────────────────────────────────────────────────────────────

/// Build the power-flow single view of the top module `top_path`.
pub fn build_pwrflow(table: &InstTable, top_path: &str) -> Result<PwrFlow, String> {
    let top_id = table
        .get_id_by_path(top_path)
        .filter(|&id| {
            table
                .get_entry(id)
                .map(|e| e.kind == InstKind::Module)
                .unwrap_or(false)
        })
        .ok_or_else(|| format!("no module entry at path '{top_path}'"))?;

    let island = NetIslandIndex::build(table);

    let mut role: HashMap<String, String> = HashMap::new();
    let mut star: HashSet<String> = HashSet::new();
    let mut rails: Vec<L1Rail> = Vec::new();
    if let Some(pd) = table.power_decls().get(&top_id) {
        for r in pd.l1_refs() {
            role.insert(
                r.name.clone(),
                r.role.clone().unwrap_or_else(|| "main".to_string()),
            );
            if r.star {
                star.insert(r.name.clone());
            }
        }
        for rail in pd.l1_rails() {
            rails.push(rail);
        }
    }

    let scope = Scope {
        top_id,
        rails,
        role,
        star,
        island,
    };

    let comps = collect_components(table, &scope);

    let crown = build_crown(table, &scope);
    let rail_rows = build_rail_rows(table, &scope, &comps);
    let roots = build_tree(table, &scope, &comps, &rail_rows);

    let flow = PwrFlow {
        top: top_path.to_string(),
        crown,
        rails: rail_rows
            .iter()
            .map(|r| RailRow {
                domain: r.domain.clone(),
                hot: r.hot.clone(),
                ret: r.ret.clone(),
                world: r.world.clone(),
                v_text: r.v_text.clone(),
                v: r.v,
                tol: r.tol,
                capacity_amps: r.capacity_amps,
                eff: r.eff,
                gen: r.gen.clone(),
                cross_world: r.cross_world,
                loads: r.loads.clone(),
                decaps: r.decaps.clone(),
            })
            .collect(),
        roots,
    };

    trace_flow(&flow);
    Ok(flow)
}

// ────────────────────────────────────────────────────────────────────────────
// Crown
// ────────────────────────────────────────────────────────────────────────────

fn build_crown(table: &InstTable, scope: &Scope) -> Vec<CrownRow> {
    // Which declared conduits are live resolvable return/reference copper in
    // the top scope (net named == conduit, or a rail ret member).
    let mut live: HashMap<String, NetRole> = HashMap::new();
    for a in scope.island.nets() {
        if a.module != Some(scope.top_id) || !a.resolvable {
            continue;
        }
        if matches!(a.role, NetRole::Ret | NetRole::Reference) {
            if let Some(c) = &a.copper {
                live.entry(c.clone()).or_insert(a.role);
            }
        }
    }
    let mut rows = Vec::new();
    if let Some(pd) = table.power_decls().get(&scope.top_id) {
        for r in pd.l1_refs() {
            let Some(&role) = live.get(&r.name) else {
                continue; // conduit with no live top net (interior / unused)
            };
            rows.push(CrownRow {
                copper: r.name.clone(),
                world: scope.role_of(&r.name),
                star: scope.star.contains(&r.name),
                rail_return: role == NetRole::Ret,
            });
        }
    }
    rows
}

// ────────────────────────────────────────────────────────────────────────────
// Rail analysis + rows
// ────────────────────────────────────────────────────────────────────────────

/// Everything the [2] rail row and the [3] tree need about one declared rail.
struct RailInfo {
    domain: String,
    hot: String,
    hot_net: u32,
    ret: String,
    world: String,
    v_text: String,
    v: Option<f64>,
    tol: Option<f64>,
    capacity_amps: Option<f64>,
    eff: Option<f64>,
    /// Element text feeding this rail (`ldo33`, `buck12─IND`, `FB_a`).
    via: String,
    /// Net feeding the branch element (a trunk bus net, or a declared rail hot
    /// net for a derived rail). None = produced by no found element.
    up_net: Option<u32>,
    /// up_net is itself a declared rail (derived rail, e.g. AVDD off DVDD).
    up_is_rail: bool,
    gen: String,
    cross_world: bool,
    loads: Vec<String>,
    decaps: Vec<String>,
}

fn build_rail_rows(table: &InstTable, scope: &Scope, comps: &[Comp]) -> Vec<RailInfo> {
    let supply = supply_nets(table, scope, comps);
    let dist = net_dist(table, scope, comps, &supply);

    let mut infos = Vec::new();
    for rail in &scope.rails {
        let Some(hot_net) = rail_hot_net(scope, &rail.hot) else {
            continue; // declared rail with no live top net
        };
        let ret_net = copper_net(scope, &rail.ret);
        let world = scope.role_of(&rail.ret);
        let ret = rail.ret.rsplit('.').next().unwrap_or(&rail.ret).to_string();

        let (via, up_net, up_is_rail, gen, cross_world) = producer_of(scope, comps, &dist, hot_net);

        // Sink loads directly on the rail hot net (pure consumers).
        let mut loads = Vec::new();
        for c in comps {
            if !c.is_pure_sink() {
                continue;
            }
            for t in &c.snk {
                if t.hot_net == Some(hot_net) {
                    loads.push(format!(
                        "{}{{{}, {}}}",
                        c.leaf,
                        t.hot,
                        t.ret.clone().unwrap_or_default()
                    ));
                }
            }
        }

        // Decouplers: two-pad pwr-less elements straddling hot/ret nets.
        let mut decaps = Vec::new();
        for c in comps {
            if !c.two_pin || c.pins.len() != 2 {
                continue;
            }
            let pads = [c.pins[0].1, c.pins[1].1];
            if pads.contains(&hot_net) && ret_net.is_some_and(|r| pads.contains(&r)) {
                decaps.push(c.leaf.clone());
            }
        }

        infos.push(RailInfo {
            domain: rail.domain.clone(),
            hot: rail.hot.clone(),
            hot_net,
            ret,
            world: world.clone(),
            v_text: rail.v_text.clone(),
            v: rail.v,
            tol: rail.tol,
            capacity_amps: rail.capacity_amps,
            eff: rail.eff,
            via,
            up_net,
            up_is_rail,
            gen,
            cross_world,
            loads,
            decaps,
        });
    }
    infos
}

/// Resolve the producer of a declared rail hot net: the branch element text,
/// the net feeding it, whether that feeder is itself a declared rail, the [2]
/// `gen` spine, and whether the rail crosses worlds vs its producer's input.
///
/// The walk climbs from the rail hot net through two-pin pass elements toward
/// the source:
///   * a converter / merge with its Src terminal on the walked net is the
///     branch element; pass elements already walked are folded into a `─`
///     chain (`buck12─IND`);
///   * arriving at another declared rail through pass elements only makes a
///     *derived rail* — the pass element is the branch (`FB_a` off DVDD).
fn producer_of(
    scope: &Scope,
    comps: &[Comp],
    dist: &HashMap<u32, usize>,
    hot_net: u32,
) -> (String, Option<u32>, bool, String, bool) {
    let mut cur = hot_net;
    let mut pass_up: Vec<String> = Vec::new();
    let mut guard = 0;
    loop {
        guard += 1;
        if guard > 40 {
            break;
        }
        // A converter/merge Src on `cur` produces it.
        if let Some(c) = comps.iter().find(|c| c.has_pwr && c.src_on(cur)) {
            let up = pick_up_net(c, dist);
            let up_is_rail = up.is_some_and(|n| is_rail_net(scope, n).is_some());
            let (gen, cross) = gen_and_cross_converter(scope, c, up, &pass_up);
            return (elem_text(&c.leaf, &pass_up), up, up_is_rail, gen, cross);
        }
        // A two-pin pass bridging cur toward the source.
        let Some((el, far)) = two_pin_up(scope, comps, dist, cur) else {
            break; // face/export root or unknown producer
        };
        let el_name = el.clone();
        pass_up.push(el);
        if is_rail_net(scope, far).is_some() {
            // Derived rail: pass alone reaches another declared rail.
            let up_is_rail = true;
            let domain = far_rail_domain(scope, far);
            let gen = format!("{}←{}", el_name, domain);
            let cross = rails_cross(scope, far, hot_net);
            return (el_name, Some(far), up_is_rail, gen, cross);
        }
        cur = far;
    }
    (String::new(), None, false, String::new(), false)
}

/// The upstream net of a converter: its sink-side hot net nearest the source.
fn pick_up_net(c: &Comp, dist: &HashMap<u32, usize>) -> Option<u32> {
    c.snk_nets()
        .min_by_key(|n| dist.get(n).copied().unwrap_or(usize::MAX))
}

/// Element display text: `producer` or `producer─pass1─pass2` (passes ordered
/// rail→source).
fn elem_text(producer: &str, pass_up: &[String]) -> String {
    if pass_up.is_empty() {
        producer.to_string()
    } else {
        format!("{producer}─{}", pass_up.join("─"))
    }
}

/// The gen spine / cross flag when a converter/merge feeds the walked rail.
fn gen_and_cross_converter(
    scope: &Scope,
    comp: &Comp,
    up_net: Option<u32>,
    pass_up: &[String],
) -> (String, bool) {
    let up_label = up_net.map(|u| {
        if is_rail_net(scope, u).is_some() {
            far_rail_domain(scope, u)
        } else {
            scope.att(u).map(|a| a.name.clone()).unwrap_or_default()
        }
    });
    // [2] gen column is pass-transparent for a converter (design §5 `buck12←
    // VMAIN`): the converter leaf names the producer, the pass elements folded
    // between its output and the rail are shown in the §3 tree (`via`), not here.
    let _ = pass_up;
    let gen = match &up_label {
        Some(l) if !l.is_empty() => format!("{}←{}", comp.leaf, l),
        _ => String::new(),
    };

    // Input side world = world of the return copper the converter's snk side
    // returns to (falls back to the up net's own return copper).
    let input_world = comp
        .snk
        .iter()
        .filter(|t| t.hot_net == up_net)
        .find_map(|t| t.ret_net)
        .or(up_net)
        .and_then(|n| ret_of_net(scope, n))
        .map(|c| scope.role_of(&c));
    // Output side world = world of the return copper the src side returns to.
    let src_world = comp
        .src
        .first()
        .and_then(|t| t.ret_net)
        .and_then(|n| ret_of_net(scope, n))
        .map(|c| scope.role_of(&c));
    let cross = match (src_world.as_deref(), input_world.as_deref()) {
        (Some(a), Some(b)) => a != b,
        _ => false,
    };
    (gen, cross)
}

/// Whether the rail net `down` crosses worlds vs the rail net `up` it is
/// derived from: different return coppers (or roles) ⇒ world cross.
fn rails_cross(scope: &Scope, up: u32, down: u32) -> bool {
    match (ret_of_net(scope, up), ret_of_net(scope, down)) {
        (Some(a), Some(b)) => {
            let (wa, wb) = (scope.role_of(&a), scope.role_of(&b));
            a != b || wa != wb
        }
        _ => false,
    }
}

/// The domain name of the declared rail a net is the hot of.
fn far_rail_domain(scope: &Scope, net: u32) -> String {
    rail_of_net(scope, net)
        .map(|r| r.domain.clone())
        .unwrap_or_else(|| scope.att(net).map(|a| a.name.clone()).unwrap_or_default())
}

/// A two-pin pass element bridging `cur` to a supply net strictly closer to the
/// source. Returns (element leaf, far net). Decouplers (far pad on a return
/// copper) never match — return coppers are not in `dist`.
fn two_pin_up(
    _scope: &Scope,
    comps: &[Comp],
    dist: &HashMap<u32, usize>,
    cur: u32,
) -> Option<(String, u32)> {
    for c in comps {
        if !c.two_pin || c.pins.len() != 2 {
            continue;
        }
        let (a, b) = (c.pins[0].1, c.pins[1].1);
        let (this, far) = if a == cur {
            (a, b)
        } else if b == cur {
            (b, a)
        } else {
            continue;
        };
        let _ = this;
        if far == cur {
            continue;
        }
        match (dist.get(&cur), dist.get(&far)) {
            (Some(&dc), Some(&df)) if df < dc => return Some((c.leaf.clone(), far)),
            _ => {}
        }
    }
    None
}

// ────────────────────────────────────────────────────────────────────────────
// Supply tree
// ────────────────────────────────────────────────────────────────────────────

/// Pair label for a supply net: `[name, ret]` when a return copper is
/// provable, else the bare net name.
fn net_pair(scope: &Scope, comps: &[Comp], net: u32) -> String {
    let name = scope.att(net).map(|a| a.name.clone()).unwrap_or_default();
    let ret = ret_of_net(scope, net).or_else(|| {
        // Borrow a return copper from any component terminal on this net.
        comps.iter().find_map(|c| {
            c.src
                .iter()
                .chain(c.snk.iter())
                .find(|t| t.hot_net == Some(net))
                .and_then(|t| t.ret_net)
                .and_then(|n| ret_of_net(scope, n))
        })
    });
    match ret {
        Some(r) => format!("[{name} , {r}]"),
        None => name,
    }
}

fn build_tree(
    table: &InstTable,
    scope: &Scope,
    comps: &[Comp],
    infos: &[RailInfo],
) -> Vec<FlowNode> {
    let supply = supply_nets(table, scope, comps);
    let dist = net_dist(table, scope, comps, &supply);
    let mut placed: HashSet<u32> = HashSet::new();

    // Source faces in a deterministic order: export power ports first (net id
    // order), then psbi/psrc face roots (component order). Face = the net.
    let mut faces: Vec<(u32, String, String)> = Vec::new(); // (net, label, note)
    for n in table.get_nets() {
        if n.module != Some(scope.top_id) {
            continue;
        }
        if let Some(p) = n.points.iter().find_map(|&pid| {
            table
                .get_entry(pid)
                .filter(|e| e.kind == InstKind::Port && e.io_type == IOType::Power)
        }) {
            let label = export_face_label(table, p);
            faces.push((n.id, label, export_note(table, p)));
        }
    }
    for c in comps {
        if c.has_pwr && c.snk.is_empty() && !c.src.is_empty() {
            let note = if c.is_bi() {
                "psbi backup source".to_string()
            } else {
                "psrc source".to_string()
            };
            let face = c
                .src
                .first()
                .and_then(|t| t.hot.split('.').next())
                .unwrap_or(&c.leaf)
                .to_string();
            let label = format!("{}.{}", c.leaf, face);
            for s in c.src_nets() {
                if supply.contains(&s) && !faces.iter().any(|(n, _, _)| *n == s) {
                    faces.push((s, label.clone(), note.clone()));
                }
            }
        }
    }

    let mut roots: Vec<FlowNode> = Vec::new();
    for (net, label, note) in faces {
        if placed.contains(&net) {
            continue;
        }
        let mut node = emit_net(
            table,
            scope,
            comps,
            infos,
            &supply,
            &dist,
            &mut placed,
            net,
            None,
        );
        node.class = "source".to_string();
        node.label = label;
        node.via = None;
        node.note = note;
        roots.push(node);
    }

    // Rails not reached from any face (no live source): standalone roots.
    for info in infos {
        if !placed.contains(&info.hot_net) {
            let node = emit_rail(scope, comps, infos, &mut placed, info, None);
            roots.push(node);
        }
    }

    roots
}

/// Owner module path of `port`, trimmed of the leading top-module segment
/// (`main.usb` → `usb`); a top-level port → ``.
fn export_module(table: &InstTable, port: &InstEntry) -> String {
    let Some(id) = port.parent_id else {
        return String::new();
    };
    let Some(module) = table.get_entry(id) else {
        return String::new();
    };
    match module.path.split_once('.') {
        Some((_, rest)) => rest.to_string(),
        None => String::new(),
    }
}

/// Source-face label for an export power port: `{owner_module}.{interface}`
/// (e.g. `usb.vin`). Entry paths are `{module}.{iface}.{hot_member}` for a DC
/// pair member (`main.usb.vin.VBUS_5V`) or `{module}.{port}` for a single-name
/// port; the trailing hot-member segment is dropped so the face names the
/// power interface the parent binds, not its internal member.
fn export_face_label(table: &InstTable, port: &InstEntry) -> String {
    let Some(module_id) = port.parent_id else {
        return String::new();
    };
    let Some(module) = table.get_entry(module_id) else {
        return String::new();
    };
    let module_rel = export_module(table, port);
    let prefix = format!("{}.", module.path);
    let name_part = match port.path.strip_prefix(&prefix) {
        Some(rest) => rest.to_string(),
        None => port.path.clone(),
    };
    // name_part = "vin.VBUS_5V" (interface.member) → face "vin";
    //             "vcc" (single-name port)          → face "vcc".
    let face = match name_part.rsplit_once('.') {
        Some((head, _)) if !head.is_empty() => head.to_string(),
        _ => name_part,
    };
    if module_rel.is_empty() {
        face
    } else {
        format!("{module_rel}.{face}")
    }
}

/// A short source-face note for an export power port (e.g. `usb export`).
fn export_note(table: &InstTable, port: &InstEntry) -> String {
    let module = export_module(table, port);
    if module.is_empty() {
        "export face".to_string()
    } else {
        format!("{module} export")
    }
}

/// Emit the node for a supply net with its downstream fan-out. A declared rail
/// net is terminal (decorated leaf); a bus net recurses through its consumer
/// elements (converters, pass elements, sink loads).
fn emit_net(
    table: &InstTable,
    scope: &Scope,
    comps: &[Comp],
    infos: &[RailInfo],
    _supply: &HashSet<u32>,
    dist: &HashMap<u32, usize>,
    placed: &mut HashSet<u32>,
    net: u32,
    via: Option<String>,
) -> FlowNode {
    if let Some(info) = infos.iter().find(|i| i.hot_net == net) {
        return emit_rail(scope, comps, infos, placed, info, via);
    }

    let world = ret_of_net(scope, net).map(|c| scope.role_of(&c));
    let mut children: Vec<FlowNode> = Vec::new();

    for c in comps {
        // Converter / merge consuming this net → its src output nets.
        if c.has_pwr && c.snk_on(net) {
            for s in c.src_nets() {
                if s == net {
                    continue;
                }
                emit_downstream(
                    table,
                    scope,
                    comps,
                    infos,
                    _supply,
                    dist,
                    placed,
                    &mut children,
                    s,
                    c.leaf.clone(),
                );
            }
        }
        // Two-pin pass bridging net to a downstream supply net.
        if c.two_pin && c.pins.len() == 2 {
            let (a, b) = (c.pins[0].1, c.pins[1].1);
            let far = if a == net {
                Some(b)
            } else if b == net {
                Some(a)
            } else {
                None
            };
            if let Some(far) = far {
                let downstream = far != net
                    && dist
                        .get(&net)
                        .zip(dist.get(&far))
                        .is_some_and(|(&d, &f)| f > d);
                if downstream {
                    emit_downstream(
                        table,
                        scope,
                        comps,
                        infos,
                        _supply,
                        dist,
                        placed,
                        &mut children,
                        far,
                        c.leaf.clone(),
                    );
                }
            }
        }
        // Pure sink load directly on a bus net (rare).
        if c.is_pure_sink() {
            for t in &c.snk {
                if t.hot_net == Some(net) {
                    children.push(FlowNode {
                        class: "load".to_string(),
                        id: t.pair(),
                        label: format!(
                            "{}{{{}, {}}}",
                            c.leaf,
                            t.hot,
                            t.ret.clone().unwrap_or_default()
                        ),
                        via: None,
                        world: world.clone(),
                        cross_world: false,
                        note: String::new(),
                        children: Vec::new(),
                    });
                }
            }
        }
    }

    let node = FlowNode {
        class: "bus".to_string(),
        id: scope.att(net).map(|a| a.name.clone()).unwrap_or_default(),
        label: net_pair(scope, comps, net),
        via,
        world,
        cross_world: false,
        note: String::new(),
        children,
    };
    placed.insert(net);
    node
}

fn emit_downstream(
    table: &InstTable,
    scope: &Scope,
    comps: &[Comp],
    infos: &[RailInfo],
    _supply: &HashSet<u32>,
    dist: &HashMap<u32, usize>,
    placed: &mut HashSet<u32>,
    children: &mut Vec<FlowNode>,
    s: u32,
    via: String,
) {
    if placed.contains(&s) {
        children.push(annotation_leaf(scope, s));
        return;
    }
    let node = emit_net(
        table,
        scope,
        comps,
        infos,
        _supply,
        dist,
        placed,
        s,
        Some(via),
    );
    children.push(node);
}

fn annotation_leaf(scope: &Scope, net: u32) -> FlowNode {
    let name = scope.att(net).map(|a| a.name.clone()).unwrap_or_default();
    FlowNode {
        class: "note".to_string(),
        id: name.clone(),
        label: format!("→ merges into [{}]", name),
        via: None,
        world: None,
        cross_world: false,
        note: String::new(),
        children: Vec::new(),
    }
}

/// A declared rail's decorated node: pair label, world, gen/cross, then sink
/// loads and any derived rails (rails whose up feeder is this rail).
fn emit_rail(
    scope: &Scope,
    comps: &[Comp],
    infos: &[RailInfo],
    placed: &mut HashSet<u32>,
    info: &RailInfo,
    via: Option<String>,
) -> FlowNode {
    placed.insert(info.hot_net);
    let label = format!("{} [{} , {}]", info.domain, info.hot, info.ret);
    let mut children: Vec<FlowNode> = Vec::new();

    for load in &info.loads {
        children.push(FlowNode {
            class: "load".to_string(),
            id: load.clone(),
            label: load.clone(),
            via: None,
            world: Some(info.world.clone()),
            cross_world: false,
            note: String::new(),
            children: Vec::new(),
        });
    }
    // Derived rails off this rail's hot net.
    for d in infos
        .iter()
        .filter(|d| d.up_net == Some(info.hot_net) && d.up_is_rail)
    {
        children.push(emit_rail(
            scope,
            comps,
            infos,
            placed,
            d,
            Some(d.via.clone()),
        ));
    }

    let mut note = String::new();
    if !info.decaps.is_empty() {
        note = format!("×{} decaps", info.decaps.len());
    }
    if !info.gen.is_empty() {
        if !note.is_empty() {
            note.push(' ');
        }
        note.push_str(&format!("gen {}", info.gen));
    }

    FlowNode {
        class: "rail".to_string(),
        id: info.hot.clone(),
        label,
        via,
        world: Some(info.world.clone()),
        cross_world: info.cross_world,
        note,
        children,
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Debug trace
// ────────────────────────────────────────────────────────────────────────────

fn trace_flow(flow: &PwrFlow) {
    trace(format_args!("top {}", flow.top));
    for c in &flow.crown {
        trace(format_args!(
            "  crown {} world={} star={} rail_ret={}",
            c.copper, c.world, c.star, c.rail_return
        ));
    }
    for r in &flow.rails {
        trace(format_args!(
            "  rail {} [{} / {}] world={} gen={} cross={} loads={:?} decaps={:?}",
            r.domain, r.hot, r.ret, r.world, r.gen, r.cross_world, r.loads, r.decaps
        ));
    }
}
