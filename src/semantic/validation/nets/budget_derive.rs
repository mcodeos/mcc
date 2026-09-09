// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! PWR-4 budget, derived-demand engine (rail-contract-design.md §8.5 — the S-set
//! step that closes the net-local push-up / OR-merge tails of 6021). One demand
//! engine shared by budget.rs's root-scope 6021: `region_demand` answers "what
//! DC current must this net's copper region deliver downstream?", and
//! `derived_charges` turns every converter / OR-merge on the board into
//! additional per-root bucket demand on top of the declared-`amp` gather that
//! budget.rs still performs unchanged.
//!
//! ## Why region demand, not per-net
//!
//! The budget quantity of one *output* side (a converter's output net, an
//! OR-merge's output region) is whatever its downstream copper draws — which may
//! sit several transparent-copper / module-boundary hops away (a buck's LX net →
//! inductor → the output rail the loads hang on). `region_demand` flood-fills
//! the current-transparent copper body of a seed net (two-pin io-only elements
//! and module-boundary co-segments, never crossing `Ret`/`Reference` return
//! copper — mirroring reach.rs's §7 L4 walk) and sums:
//!   1. the declared `amp` sinks on the region, *excluding* the input rows of
//!      every def classified `Regulator`/`Combine` (those rows are derived, not
//!      declared — a converter's VIN row never advertises its own draw);
//!   2. the derived draw of each device whose input/leg face hangs on the
//!      region, each computed once per region:
//!        * a **Regulator** (full spec: `spec.output` + ≥1 Snk + ≥1 Src row)
//!          draws `I_in = Σ (|V_out|·D(out-net)) / (|V_in|·eff)`, `eff` default
//!          1.0 when the Src row omits it;
//!        * an **OR-merge Combine** (≥2 Snk legs + ≥1 Src row, no `spec.output`)
//!          passes `D(merge output region)` through whichever leg is active
//!          (single-source mode, §6.3).
//! Region results are memoized per member net and cycle-guarded: a recursion
//! that re-enters a region already on the stack contributes 0 — never a
//! fabricated demand from a loop.
//!
//! The 6021 buckets are then the sum over *governing roots*: budget.rs opens
//! one bucket per capacity root (self or reached through copper) that accumulates
//! its region's declared `amp` *and* the derived charge of every device whose
//! input/leg net that root governs. A rail that is itself over budget fires at
//! its own root while an upstream root that must push it also fires — two
//! checkpoints, never a double count at one root.

use super::budget::{BudgetRoot, BudgetScan};
use super::window::{classify_supply_def, SupplyClass};
use super::{entry_pos, sink_contract_for, source_contract_for};
use crate::instant::insttab::{InstKind, InstTable};
use crate::instant::island::{NetIslandIndex, NetRole};
use crate::semantic::common::IOType;
use crate::semantic::module::pi::decode_pwr_pin;
use std::collections::{HashMap, HashSet};

/// One derived-demand charge to fold into a 6021 root bucket on top of the
/// declared-`amp` gather. `witness` anchors a bucket that would otherwise have
/// no declared sink (a device input pin on a root with only derived demand) —
/// a fresh bucket is opened only for a genuinely over-capacity root.
pub(super) struct DerivedCharge {
    pub(super) root: u32,
    pub(super) cap: f64,
    pub(super) demand: f64,
    pub(super) count: usize,
    pub(super) witness: (u32, String),
}

/// One output face of a device: the flat net its Src-row hot pin lands on, plus
/// the row's decoded nominal (`v`, for a converter's `|V_out|`) and `eff`
/// (defaulted to 1.0).
#[derive(Clone)]
struct OutputFace {
    net: u32,
    v: Option<f64>,
    eff: f64,
}

/// A `Regulator`/`Combine` device instance, pre-decoded at build time so the
/// region recursion never borrows the def tables while recursing.
#[derive(Clone)]
struct Device {
    class: SupplyClass,
    /// Flat nets of the Snk-row hot pins (input / combine legs).
    inputs: Vec<u32>,
    /// Decoded nominal of the first Snk row (`V_in`, a regulator only).
    v_in: Option<f64>,
    outputs: Vec<OutputFace>,
    /// Position of the first wired input pin (fresh-bucket witness anchor).
    anchor: Option<(u32, String)>,
}

/// Derived-demand engine over one flat run (see module docs).
pub(super) struct BudgetLoadScan<'a> {
    table: &'a InstTable,
    /// The budget root-resolution engine (`root_of` + its `PowerScan` for def /
    /// contract lookups) — reused rather than rebuilt, so root classification
    /// and derived charging can never disagree about a net.
    budget: BudgetScan<'a>,
    /// Island role index for the §7 L4 return-copper barrier.
    idx: NetIslandIndex,
    devices: Vec<Device>,
    /// input/leg net id → device indices with a face on it.
    by_input_net: HashMap<u32, Vec<usize>>,
    memo: HashMap<u32, f64>,
    stack: HashSet<u32>,
}

impl<'a> BudgetLoadScan<'a> {
    pub(super) fn new(table: &'a InstTable) -> BudgetLoadScan<'a> {
        let mut scan = BudgetLoadScan {
            table,
            budget: BudgetScan::new(table),
            idx: NetIslandIndex::build(table),
            devices: Vec::new(),
            by_input_net: HashMap::new(),
            memo: HashMap::new(),
            stack: HashSet::new(),
        };
        scan.index_devices();
        scan
    }

    /// Record every `Regulator`/`Combine` device instance that is actually wired
    /// (both an input face and an output face on nets), pre-decoding the rows so
    /// the recursion below stays borrow-free.
    fn index_devices(&mut self) {
        let mut devs: Vec<Device> = Vec::new();
        for comp in self.table.get_components() {
            if comp.synthetic || comp.unselected || comp.not_fitted {
                continue;
            }
            let Some(def) = self.budget.scan.def_of(comp.id) else {
                continue;
            };
            let class = classify_supply_def(def);
            if class == SupplyClass::Load {
                continue;
            }
            let mut dev = Device {
                class,
                inputs: Vec::new(),
                v_in: None,
                outputs: Vec::new(),
                anchor: None,
            };
            for pin in self.table.get_pins_of(comp.id) {
                let Some(net) = self.table.get_net_of(pin.id) else {
                    continue;
                };
                if let Some(contract) = sink_contract_for(def, pin) {
                    if !dev.inputs.contains(&net.id) {
                        if dev.anchor.is_none() {
                            dev.anchor = Some(entry_pos(pin));
                        }
                        dev.inputs.push(net.id);
                    }
                    if dev.v_in.is_none() {
                        dev.v_in = decode_pwr_pin(contract).v;
                    }
                } else if let Some(contract) = source_contract_for(def, pin) {
                    let dec = decode_pwr_pin(contract);
                    dev.outputs.push(OutputFace {
                        net: net.id,
                        v: dec.v,
                        eff: dec.eff.unwrap_or(1.0),
                    });
                }
            }
            if dev.inputs.is_empty() || dev.outputs.is_empty() {
                continue; // not wired on both sides — nothing to derive
            }
            let idx = devs.len();
            for &innet in &dev.inputs {
                self.by_input_net.entry(innet).or_default().push(idx);
            }
            devs.push(dev);
        }
        self.devices = devs;
    }

    /// Island-role exclusion: `Ret`/`Reference` copper never carries hot-side
    /// demand, so a region neither starts on it nor floods through it.
    fn role_excluded(&self, net_id: u32) -> bool {
        self.idx
            .get(net_id)
            .is_some_and(|a| matches!(a.role, NetRole::Ret | NetRole::Reference))
    }

    /// Flood-fill the current-transparent copper body of `net_id`: every
    /// two-pin io-only element (fuse/inductor/ferrite) forwards to its other
    /// net, and module-boundary co-segments (the same junction point id in a
    /// different scope) join. Stops before return/reference copper.
    fn fill_region(&self, net_id: u32, seen: &mut HashSet<u32>, out: &mut Vec<u32>) {
        if !seen.insert(net_id) {
            return;
        }
        if self.role_excluded(net_id) {
            return;
        }
        let Some(net) = self.table.get_net(net_id) else {
            return;
        };
        out.push(net_id);
        // Transparent-copper arm (mirrors reach.rs / budget.rs §6.3).
        for &pid in &net.points {
            let Some(entry) = self.table.get_entry(pid) else {
                continue;
            };
            if !matches!(entry.kind, InstKind::Pin) {
                continue;
            }
            let Some(cid) = entry.parent_id else {
                continue;
            };
            let Some(def) = self.budget.scan.def_of(cid) else {
                continue;
            };
            if !def.pins.pwr.is_empty() {
                continue; // has DC rows → a power face, not raw copper
            }
            for pin in self.table.get_pins_of(cid) {
                let Some(pnet) = self.table.get_net_of(pin.id) else {
                    continue;
                };
                if pnet.id != net.id {
                    self.fill_region(pnet.id, seen, out);
                }
            }
        }
        // Module-boundary arm (mirrors reach.rs §6.6).
        if let Some(m) = net.module {
            for &pid in &net.points {
                for &cid in self.table.nets_of(pid) {
                    if cid == net.id {
                        continue;
                    }
                    let Some(co) = self.table.get_net(cid) else {
                        continue;
                    };
                    if co.module.is_none() || co.module == Some(m) {
                        continue;
                    }
                    self.fill_region(cid, seen, out);
                }
            }
        }
    }

    /// Declared `amp` on one net: the sinks of plain `Load` defs only. Input
    /// rows of a `Regulator`/`Combine` def are derived (see module docs) and are
    /// excluded here — the same rule budget.rs applies to its declared gather,
    /// so a converter's own push-up can never be double counted as its input
    /// net's local load.
    fn declared_loc(&self, net: &crate::instant::insttab::NetEntry) -> f64 {
        let mut total: f64 = 0.0;
        for &pid in &net.points {
            let Some(entry) = self.table.get_entry(pid) else {
                continue;
            };
            if !matches!(entry.kind, InstKind::Pin) || !matches!(entry.io_type, IOType::Power) {
                continue;
            }
            let Some(cid) = entry.parent_id else {
                continue;
            };
            let Some(def) = self.budget.scan.def_of(cid) else {
                continue;
            };
            if classify_supply_def(def) != SupplyClass::Load {
                continue;
            }
            let Some(contract) = sink_contract_for(def, entry) else {
                continue;
            };
            let Some(a) = decode_pwr_pin(contract).amp else {
                continue;
            };
            total += a;
        }
        total
    }

    /// The DC current one net's copper region must deliver downstream —
    /// memoized per member net, cycle-guarded (a re-entered region contributes
    /// 0, never a fabricated demand from a loop).
    pub(super) fn region_demand(&mut self, net_id: u32) -> f64 {
        if self.role_excluded(net_id) {
            return 0.0;
        }
        if let Some(&d) = self.memo.get(&net_id) {
            return d;
        }
        let mut seen = HashSet::new();
        let mut region = Vec::new();
        self.fill_region(net_id, &mut seen, &mut region);
        if region.is_empty() {
            return 0.0;
        }
        if region.iter().any(|n| self.stack.contains(n)) {
            return 0.0; // a cyclic re-entry edge — contributes nothing
        }
        for &n in &region {
            self.stack.insert(n);
        }
        let region_set: HashSet<u32> = region.iter().copied().collect();

        let mut total: f64 = 0.0;
        for &n in &region {
            let Some(net) = self.table.get_net(n) else {
                continue;
            };
            total += self.declared_loc(net);
        }

        // Device draws attached to the region (dedup by device — a multi-leg
        // combine whose legs share one copper body still passes its merge load
        // through once). Snapshot the records so the recursion below never
        // borrows `self.devices` across a `&mut self` call.
        let mut attached: Vec<usize> = Vec::new();
        for &n in &region {
            if let Some(list) = self.by_input_net.get(&n) {
                for &i in list {
                    if !attached.contains(&i) {
                        attached.push(i);
                    }
                }
            }
        }
        let snaps: Vec<Device> = attached.iter().map(|&i| self.devices[i].clone()).collect();
        for dev in &snaps {
            match dev.class {
                SupplyClass::Regulator => {
                    let Some(v_in) = dev.v_in else {
                        continue;
                    };
                    for of in &dev.outputs {
                        if region_set.contains(&of.net) {
                            continue; // degenerate: draws from its own copper
                        }
                        let Some(v_out) = of.v else {
                            continue;
                        };
                        total += (v_out.abs() * self.region_demand(of.net)) / (v_in.abs() * of.eff);
                    }
                }
                SupplyClass::Combine => {
                    for of in &dev.outputs {
                        if region_set.contains(&of.net) {
                            continue;
                        }
                        total += self.region_demand(of.net);
                    }
                }
                SupplyClass::Load => {}
            }
        }

        for &n in &region {
            self.stack.remove(&n);
        }
        for &n in &region {
            self.memo.insert(n, total);
        }
        total
    }

    /// Derived draw of one device: a regulator's push-up, or the merge output
    /// region demand a combine leg must carry when active.
    fn device_draw(&mut self, dev: &Device) -> f64 {
        match dev.class {
            SupplyClass::Regulator => {
                let Some(v_in) = dev.v_in else {
                    return 0.0;
                };
                let mut total: f64 = 0.0;
                for of in &dev.outputs {
                    let Some(v_out) = of.v else {
                        continue;
                    };
                    total += (v_out.abs() * self.region_demand(of.net)) / (v_in.abs() * of.eff);
                }
                total
            }
            SupplyClass::Combine => {
                let mut total: f64 = 0.0;
                for of in &dev.outputs {
                    total += self.region_demand(of.net);
                }
                total
            }
            SupplyClass::Load => 0.0,
        }
    }

    /// Every derived charge to fold into a 6021 bucket: a regulator's push-up on
    /// the root governing its input net, and — per OR-merge leg, single-source
    /// mode — the full merge-output-region demand on each distinct governing
    /// root of the combine's legs. Opaque roots (a capacity-less source/boundary
    /// input, the golden VMAIN_5V/VBUS_RAW seam) contribute nothing.
    pub(super) fn derived_charges(&mut self) -> Vec<DerivedCharge> {
        let mut out = Vec::new();
        let devs: Vec<Device> = self.devices.clone();
        for dev in &devs {
            let Some(anchor) = dev.anchor.clone() else {
                continue;
            };
            match dev.class {
                SupplyClass::Regulator => {
                    let Some(&innet) = dev.inputs.first() else {
                        continue;
                    };
                    let (root_id, cap) = match self.budget.root_of(innet) {
                        BudgetRoot::Root { cap } => (innet, cap),
                        BudgetRoot::Reached { root, cap } => (root, cap),
                        BudgetRoot::Opaque => continue,
                    };
                    let demand = self.device_draw(dev);
                    if demand <= 1e-12 {
                        continue;
                    }
                    out.push(DerivedCharge {
                        root: root_id,
                        cap,
                        demand,
                        count: 1,
                        witness: anchor.clone(),
                    });
                }
                SupplyClass::Combine => {
                    let demand = self.device_draw(dev);
                    if demand <= 1e-12 {
                        continue;
                    }
                    let mut roots: Vec<(u32, f64)> = Vec::new();
                    for &innet in &dev.inputs {
                        match self.budget.root_of(innet) {
                            BudgetRoot::Root { cap } => {
                                if !roots.iter().any(|(r, _)| *r == innet) {
                                    roots.push((innet, cap));
                                }
                            }
                            BudgetRoot::Reached { root, cap } => {
                                if !roots.iter().any(|(r, _)| *r == root) {
                                    roots.push((root, cap));
                                }
                            }
                            BudgetRoot::Opaque => {}
                        }
                    }
                    for (root, cap) in roots {
                        out.push(DerivedCharge {
                            root,
                            cap,
                            demand,
                            count: 1,
                            witness: anchor.clone(),
                        });
                    }
                }
                SupplyClass::Load => {}
            }
        }
        out
    }
}
