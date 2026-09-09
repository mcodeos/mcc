// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Rail-contract window layer — interval decode of a component's `spec` block
//! and the window arithmetic the pass-2 checks share (rail-contract-design.md
//! §2 / §4.3 / §6).
//!
//! The nominal layer (6011/6013/6019/6021) reasons about scalar S(net) volts.
//! The window layer instead carries each handwritten root as a closed interval
//! `[lo, hi]` — a source tolerance (±%), a converter `spec.output` guarantee,
//! or a sink `spec.input_req` requirement — and pushes the interval along the
//! copper (regulator re-anchor at §6.1, OR-merge ∪ at §6.2, pass-through at
//! §6.3) instead of re-deriving a single nominal at every point.
//!
//! Decoding is AST-driven: only a `lo ~ hi` range (`McExpression::Range`) whose
//! two endpoints parse as volts produces a window. Anything else — a point, a
//! `±`, a foreign unit, a missing bound — leaves the field `None`: a window is
//! never fabricated from a non-window.

// Leaf lands ahead of its consumers: `decode_component_spec`/`DecodedSpec`
// are first exercised by the spec-shape probe, then wired by the 6023/6024/
// 6025 owners in the window batch (registry-driven, same convention as
// rules.rs). Remove this allow once every owner consumes them.
#![allow(dead_code)]

use super::NetCheckResult;
use crate::instant::insttab::{InstKind, InstTable, NetEntry};
use crate::semantic::basic::mc_expr::McExpression;
use crate::semantic::basic::mc_ids::McIds;
use crate::semantic::common::IOType;
use crate::semantic::component::mc_attr::{McAttrVal, McAttribute};
use crate::semantic::component::mc_pins::PwrDir;
use crate::semantic::component::McComponent;
use crate::semantic::module::pi::{decode_pwr_pin, parse_volts, L1PwrPin};
use std::collections::{HashMap, HashSet};

/// Closed supply/spec window `[lo, hi]`, volts. Endpoint comparisons use the
/// same 1e-9 comparator as the nominal pass (nets/mod.rs).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PwrWindow {
    pub lo: f64,
    pub hi: f64,
}

impl PwrWindow {
    /// The ±`tol`-fraction window around a nominal point. `tol == None` (or
    /// a non-positive fraction) degrades to the point window `[v, v]` — the
    /// window twin of a plain scalar nominal.
    pub(crate) fn around(v: f64, tol: Option<f64>) -> PwrWindow {
        match tol {
            Some(t) if t > 0.0 => PwrWindow {
                lo: v * (1.0 - t),
                hi: v * (1.0 + t),
            },
            _ => PwrWindow { lo: v, hi: v },
        }
    }

    /// Point membership — `v ∈ [lo, hi]` (closed, 1e-9 slop).
    pub(crate) fn contains(&self, v: f64) -> bool {
        self.lo - 1e-9 <= v && v <= self.hi + 1e-9
    }

    /// `self` covers `sub` (`sub ⊆ self`) with the 1e-9 comparator — the
    /// §6 regulator-gate and sink-window shape: the actual supply window must
    /// cover the required (or converter-output) window.
    pub(crate) fn covers(&self, sub: &PwrWindow) -> bool {
        self.lo - 1e-9 <= sub.lo && self.hi + 1e-9 >= sub.hi
    }

    /// Merge (∪) — the OR/combine semantics (§6.2): a rail fed by several
    /// sources can ride anywhere inside the union of their guarantees.
    pub(crate) fn union(&self, other: &PwrWindow) -> PwrWindow {
        PwrWindow {
            lo: self.lo.min(other.lo),
            hi: self.hi.max(other.hi),
        }
    }
}

/// The decoded `spec = [...]` block of one component def
/// (rail-contract-design.md §2). Each window keeps its `a~b` source text
/// verbatim for diagnostics.
#[derive(Debug, Default, Clone)]
pub(crate) struct DecodedSpec {
    /// A `spec` attribute is present on the component (even when a bound
    /// failed to decode — that asymmetry feeds 6025).
    pub has_spec: bool,
    /// `input_req` gate window — the converter's declared input operating
    /// window (§6.1 pre-condition).
    pub input_req: Option<(PwrWindow, String)>,
    /// `output` guarantee window — the converter's declared output window
    /// under the gate (§6.1 post-condition).
    pub output: Option<(PwrWindow, String)>,
}

/// Decode a component def's `spec = [ input_req = a~b, output = c~d ]` block.
///
/// The block sits on the component top-level as an attribute whose value is a
/// bracketed set of inner attributes (`McAttrVal::Attributes`), each keyed
/// `input_req` / `output` and valued with a `lo ~ hi` range expression. A
/// missing `spec`, a non-`Attributes` value, an unknown key, or an un-parseable
/// window all contribute `None` — never a fabricated window.
///
/// Storage shape pinned by an end-to-end load probe (window-layer A1, since
/// removed): `input_req = 3.6V ~ 5.5V` lands as `AttrExpr(Range(UnitValue(3.6V),
/// UnitValue(5.5V)))` and decodes to `[3.6, 5.5]` with verbatim text `3.6V~5.5V`
/// (endpoint raw text echoes the author's source; `Range` Display is `lo~hi`).
pub(crate) fn decode_component_spec(def: &McComponent) -> DecodedSpec {
    let mut out = DecodedSpec::default();
    let Some(spec_attr) = def.attrs.find(&McIds::from("spec")) else {
        return out;
    };
    out.has_spec = true;
    for val in &spec_attr.values {
        let McAttrVal::Attributes(inner) = val else {
            continue;
        };
        for attr in inner {
            let key = attr.id.to_string();
            let Some((win, text)) = window_of_attr(attr) else {
                continue;
            };
            match key.as_str() {
                "input_req" => out.input_req = Some((win, text)),
                "output" => out.output = Some((win, text)),
                _ => {}
            }
        }
    }
    out
}

/// Read one inner `spec` attribute's value as a `lo ~ hi` volts window.
///
/// Only `AttrExpr(Range(lo, hi))` is a window; each endpoint is parsed through
/// the same `parse_volts` used for `::DC(v)` nominals (rejects `~`/`±`/`*` and
/// foreign units). The verbatim `a~b` text echoes the author's source.
fn window_of_attr(attr: &McAttribute) -> Option<(PwrWindow, String)> {
    let first = attr.values.first()?;
    let McAttrVal::AttrExpr(McExpression::Range(lo, hi)) = first else {
        return None;
    };
    let lo_v = parse_volts(&lo.to_string())?;
    let hi_v = parse_volts(&hi.to_string())?;
    Some((PwrWindow { lo: lo_v, hi: hi_v }, first.to_string()))
}

// ============================================================================
// Window-of-net derivation (rail-contract-design.md §4.3 / §6, window batch A3)
//
// `WindowDeriv::window_of_net` answers "what supply window rides this net?" for
// a flat net id. Resolution follows the §6 priority, never a name:
//   1. a declared rail face on the net — the handwritten §4.1 root wins even
//      when a converter output lands on the same copper (rail = the promise);
//   2. else the psrc/psbi source faces, grouped by owning instance, then judged
//      by the owning def's *shape* (spec block presence + row directions):
//        * `spec.output` present  → regulator: the net is its output net,
//          re-anchored to the spec.output guarantee (§6.1 post-condition);
//        * ≥2 Snk input rows      → OR-merge: S = ∪ of each input net's window
//          (any non-Resolved input → the whole merge is Unresolved, never a
//          fabricated partial ∪);
//        * exactly 1 Snk input row → pass-through identity: S = S(input net);
//        * 0 Snk rows             → leaf source: S = ±tol window of its own
//          `::DC(v)` (SRC5, a psbi battery);
//        * ≥2 *distinct* source instances, no merge → NoSupply (6010/6013's
//          contention scope, deferred like the nominal layer);
//   3. no rail and no source face → copper pass-through (§6.3): a 2-pin io
//      device with no DC rows (fuse / inductor / ferrite) forwards its other
//      net's window verbatim; then a module-boundary feed (§6.6): a net that
//      reaches a submodule port forwards the first Resolved window of a
//      cross-boundary co-segment (A′: the child-scope NetEntry shares the
//      junction point id and `module` differs). With no Resolved co-segment
//      and no driver the net stays NoSupply (the shared 6011/6019 leave for
//      un-driven sink faces).
//
// Recursion is memoized per net id and guarded against cycles (a revisited net
// on the current stack is Unresolved). A3 lands the engine ahead of the 6023/
// 6024/6025 owners that consume it; nothing is wired yet.
// ============================================================================

/// Result of classifying one net's supply window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum WindowState {
    /// A window rides this net (rail root, regulator output, leaf source, or
    /// a merge of fully-resolved inputs).
    Resolved(PwrWindow),
    /// No resolvable supply reaches the net — un-driven sink face, module
    /// boundary, or undeclared source contention. Never judged, never guessed.
    NoSupply,
    /// A merge/pass leg could not resolve — never fabricate a partial window.
    Unresolved,
}

/// Recursive window-of-net engine (see module-level note).
pub(crate) struct WindowDeriv<'a> {
    table: &'a InstTable,
    /// Shared face maps (rail guarantee + comp_def), reused for def lookup.
    scan: super::PowerScan,
    /// Rail hot net name → (±tol window, verbatim nominal). Built like the
    /// shared `guarantee` but carries tolerance; same scope-ambiguity drop.
    rail_win: HashMap<String, (PwrWindow, String)>,
    memo: HashMap<u32, WindowState>,
    stack: HashSet<u32>,
}

impl<'a> WindowDeriv<'a> {
    /// New engine over one flat table, resolving windows only within that
    /// table's scope (one FlatErc run = one deriv).
    pub(crate) fn new(table: &'a InstTable) -> WindowDeriv<'a> {
        let scan = super::PowerScan::build(table);
        let mut rail_win: HashMap<String, (PwrWindow, String)> = HashMap::new();
        for (pi, _uri) in super::power_intent_defs(table) {
            for r in pi.l1_rails() {
                let Some(v) = r.v else {
                    continue;
                };
                let w = PwrWindow::around(v, r.tol);
                match rail_win.get(&r.hot) {
                    Some((prev, _))
                        if (prev.lo - w.lo).abs() > 1e-9 || (prev.hi - w.hi).abs() > 1e-9 =>
                    {
                        rail_win.remove(&r.hot); // two rails, one hot → ambiguous scope
                    }
                    None => {
                        rail_win.insert(r.hot.clone(), (w, r.v_text.clone()));
                    }
                    _ => {} // same window redeclared: keep the first
                }
            }
        }
        WindowDeriv {
            table,
            scan,
            rail_win,
            memo: HashMap::new(),
            stack: HashSet::new(),
        }
    }

    /// The supply window riding a flat net — memoized, cycle-guarded.
    pub(crate) fn window_of_net(&mut self, net_id: u32) -> WindowState {
        if let Some(s) = self.memo.get(&net_id) {
            return *s;
        }
        if !self.stack.insert(net_id) {
            return WindowState::Unresolved; // re-entry on this stack = a cycle
        }
        let state = match self.table.get_net(net_id) {
            None => WindowState::NoSupply,
            Some(net) => self.resolve_net(net),
        };
        self.stack.remove(&net_id);
        self.memo.insert(net_id, state);
        state
    }

    /// §6 priority resolution for one net (net = &borrow of `table`, so it
    /// survives the recursive `&mut self` calls below).
    fn resolve_net(&mut self, net: &NetEntry) -> WindowState {
        // 1. rail face — declared §4.1 root, exact name then last dotted
        //    segment (mirrors PowerScan::rail_face's lookup).
        if let Some((w, _)) = self.rail_face(net) {
            return WindowState::Resolved(w);
        }

        // 2. source faces, grouped by owning instance.
        let mut drivers: Vec<u32> = Vec::new(); // distinct owning instances
        let mut faces: Vec<(u32, L1PwrPin)> = Vec::new(); // (instance, decoded row)
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
            let Some(def) = self.scan.def_of(cid) else {
                continue;
            };
            let Some(contract) = super::source_contract_for(def, entry) else {
                continue;
            };
            if !drivers.contains(&cid) {
                drivers.push(cid);
            }
            faces.push((cid, decode_pwr_pin(contract)));
        }
        match drivers.len() {
            0 => self.resolve_rootless(net),
            1 => self.resolve_driver(drivers[0], &faces),
            _ => WindowState::NoSupply, // ≥2 hard sources, no declared merge
        }
    }

    /// Classify a single driver instance by its owning def's shape.
    fn resolve_driver(&mut self, cid: u32, faces: &[(u32, L1PwrPin)]) -> WindowState {
        let Some(def) = self.scan.def_arc(cid) else {
            return WindowState::NoSupply;
        };
        let spec = decode_component_spec(&def);

        // 2a. regulator output re-anchor (§6.1 post-condition) — the spec.output
        //     guarantee, present because this def has a source face on the net.
        if let Some((out, _)) = spec.output {
            return WindowState::Resolved(out);
        }

        // 2b. shape by row directions of the owning def (never by name).
        let (n_snk, n_src) = pwr_row_counts(&def);

        // ≥2 sink input rows + ≥1 source row, no spec.output → OR-merge (§6.2).
        if n_snk >= 2 && n_src >= 1 {
            let input_nets = self.sink_input_nets(cid, &def);
            if input_nets.is_empty() {
                return WindowState::NoSupply;
            }
            let mut lo = f64::INFINITY;
            let mut hi = f64::NEG_INFINITY;
            for inet in input_nets {
                match self.window_of_net(inet) {
                    WindowState::Resolved(w) => {
                        lo = lo.min(w.lo);
                        hi = hi.max(w.hi);
                    }
                    _ => return WindowState::Unresolved, // never a partial ∪
                }
            }
            return WindowState::Resolved(PwrWindow { lo, hi });
        }

        // exactly 1 sink input row → pass-through identity (§6.3 / pass row).
        if n_snk == 1 && n_src >= 1 {
            let input_nets = self.sink_input_nets(cid, &def);
            let Some(inet) = input_nets.first().copied() else {
                return WindowState::NoSupply;
            };
            return self.window_of_net(inet);
        }

        // 0 sink rows → leaf source: ±tol window of its own ::DC(v). Every face
        // on the net belongs to this one driver (drivers.len() == 1).
        if n_snk == 0 && n_src >= 1 {
            let mut leaf: Option<PwrWindow> = None;
            for dec in faces.iter().map(|(_, d)| d) {
                let Some(v) = dec.v else {
                    continue;
                };
                let w = PwrWindow::around(v, dec.tol);
                match leaf {
                    None => leaf = Some(w),
                    Some(prev)
                        if (prev.lo - w.lo).abs() > 1e-9 || (prev.hi - w.hi).abs() > 1e-9 =>
                    {
                        return WindowState::NoSupply; // two leaves, one net
                    }
                    _ => {}
                }
            }
            return match leaf {
                Some(w) => WindowState::Resolved(w),
                None => WindowState::NoSupply,
            };
        }

        WindowState::NoSupply
    }

    /// Distinct net ids of an instance's Snk-contract hot pins (its input
    /// faces). Unconnected sinks contribute nothing.
    fn sink_input_nets(&self, cid: u32, def: &McComponent) -> Vec<u32> {
        let mut out: Vec<u32> = Vec::new();
        for pin in self.table.get_pins_of(cid) {
            if super::sink_contract_for(def, pin).is_none() {
                continue;
            }
            if let Some(net) = self.table.get_net_of(pin.id) {
                if !out.contains(&net.id) {
                    out.push(net.id);
                }
            }
        }
        out
    }

    /// §6 priority 3 — no rail, no source face: copper pass-through or leave.
    fn resolve_rootless(&mut self, net: &NetEntry) -> WindowState {
        // Pass-through copper (§6.3): a 2-pin io device with no DC rows on this
        // net forwards its other net's window verbatim (fuse/inductor/ferrite).
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
            let Some(def) = self.scan.def_arc(cid) else {
                continue;
            };
            if !def.pins.pwr.is_empty() {
                continue; // has DC rows → it is a power face, not raw copper
            }
            // The other pin of this pass device lands on a different net.
            let mut other: Option<u32> = None;
            for pin in self.table.get_pins_of(cid) {
                let Some(pnet) = self.table.get_net_of(pin.id) else {
                    continue;
                };
                if pnet.id != net.id {
                    other = Some(pnet.id);
                    break;
                }
            }
            if let Some(onet) = other {
                return self.window_of_net(onet);
            }
        }
        // Module-boundary feed (rail-contract-design.md §6.6): a net that only
        // reaches a submodule port is not yet adjudicated — across the boundary
        // the SAME physical copper is a second NetEntry owned by the child scope
        // (A′: both share the junction point id, `module` differs). If that
        // co-segment already resolved a window, forward it verbatim. Cycle-safe:
        // a rootless child-sink co-segment recursing back to this net hits the
        // in_progress guard and is Unresolved, so it is skipped and this net
        // stays NoSupply; only a genuinely Resolved child-source feed wins.
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
                        continue; // same-scope segment, not a module boundary
                    }
                    if let WindowState::Resolved(w) = self.window_of_net(cid) {
                        return WindowState::Resolved(w);
                    }
                }
            }
        }
        // Module-boundary / un-driven sink net — the shared 6011/6019 leave.
        WindowState::NoSupply
    }

    /// Rail face on a net: exact net name, then the last dotted segment,
    /// returning the ±tol window + verbatim nominal (unlike the shared
    /// `rail_face`, which keeps only the scalar nominal).
    fn rail_face(&self, net: &NetEntry) -> Option<(PwrWindow, String)> {
        self.rail_win
            .get(&net.name)
            .or_else(|| {
                net.name
                    .rsplit('.')
                    .next()
                    .and_then(|l| self.rail_win.get(l))
            })
            .cloned()
    }
}

/// (sink rows, source rows) of a def's DC row family — the shape axis that
/// classifies a driver, counted from the written rows (never from names).
pub(crate) fn pwr_row_counts(def: &McComponent) -> (usize, usize) {
    let n_snk = def.pins.pwr.iter().filter(|c| c.dir == PwrDir::Snk).count();
    let n_src = def
        .pins
        .pwr
        .iter()
        .filter(|c| matches!(c.dir, PwrDir::Src | PwrDir::Bi))
        .count();
    (n_snk, n_src)
}

/// Render a window for diagnostics: a point writes one value, an interval the
/// `lo~hi` pair (composed fresh — never re-parsed).
pub(crate) fn window_text(w: PwrWindow) -> String {
    if (w.hi - w.lo).abs() < 1e-9 {
        format!("{}V", super::fmt_round(w.lo))
    } else {
        format!("{}V~{}V", super::fmt_round(w.lo), super::fmt_round(w.hi))
    }
}

/// An un-derivable net is one a window rule must not judge.
fn resolved_window(state: WindowState) -> Option<PwrWindow> {
    match state {
        WindowState::Resolved(w) => Some(w),
        WindowState::NoSupply | WindowState::Unresolved => None,
    }
}

/// 6023 POWER_CONVERTER_GATE — §6.1 regulator pre-condition. Every component
/// whose def writes the full regulator spec (input_req + output, ≥1 Snk input
/// row, ≥1 Src row) gates its own input net: a Resolved supply window there
/// must sit inside the declared input_req window.
pub(crate) fn check_converter_gate_window(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let mut deriv = WindowDeriv::new(table);
    for comp in table.get_components() {
        if comp.synthetic || comp.unselected || comp.not_fitted {
            continue;
        }
        let Some(def) = deriv.scan.def_arc(comp.id) else {
            continue;
        };
        let spec = decode_component_spec(&def);
        let (Some((req, req_text)), Some(_)) = (spec.input_req.as_ref(), spec.output.as_ref())
        else {
            continue; // a partial spec is 6025's Info, not a gate
        };
        let (n_snk, n_src) = pwr_row_counts(&def);
        if n_snk < 1 || n_src < 1 {
            continue; // no input row / no output row → not a regulator shape
        }
        for inet in deriv.sink_input_nets(comp.id, &def) {
            let Some(s) = resolved_window(deriv.window_of_net(inet)) else {
                continue; // un-derivable input feed: not adjudicated
            };
            if req.covers(&s) {
                continue;
            }
            let (Some(net), (pos, uri)) = (table.get_net(inet), super::entry_pos(comp)) else {
                continue;
            };
            results.push(NetCheckResult {
                check: "converter-gate-window",
                severity: "error",
                message: crate::errcodes::format_msg(
                    crate::errcodes::POWER_CONVERTER_GATE,
                    &[&window_text(s), req_text, &comp.path],
                ),
                net_name: net.name.clone(),
                code: crate::errcodes::POWER_CONVERTER_GATE,
                pos,
                uri,
            });
        }
    }
}

/// 6024 POWER_SINK_WINDOW_MISMATCH — §6.3 sink req window. A load (a def with
/// no output post-condition) whose spec declares an input_req states the supply
/// window it accepts; the actual supply window on each of its sink nets must
/// sit inside it. A regulator's own input row is gated per net by 6023, so a
/// def that writes `output` is never re-judged here.
pub(crate) fn check_sink_window_mismatch(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let mut deriv = WindowDeriv::new(table);
    for comp in table.get_components() {
        if comp.synthetic || comp.unselected || comp.not_fitted {
            continue;
        }
        let Some(def) = deriv.scan.def_arc(comp.id) else {
            continue;
        };
        let spec = decode_component_spec(&def);
        let Some((req, req_text)) = spec.input_req.as_ref() else {
            continue; // no accepted window declared → nothing to mismatch
        };
        if spec.output.is_some() {
            continue; // a regulator's sink is 6023's per-net gate
        }
        for inet in deriv.sink_input_nets(comp.id, &def) {
            let Some(s) = resolved_window(deriv.window_of_net(inet)) else {
                continue; // un-derivable supply: not adjudicated
            };
            if req.covers(&s) {
                continue;
            }
            let (Some(net), (pos, uri)) = (table.get_net(inet), super::entry_pos(comp)) else {
                continue;
            };
            results.push(NetCheckResult {
                check: "sink-window-mismatch",
                severity: "error",
                message: crate::errcodes::format_msg(
                    crate::errcodes::POWER_SINK_WINDOW_MISMATCH,
                    &[&window_text(s), req_text, &comp.path],
                ),
                net_name: net.name.clone(),
                code: crate::errcodes::POWER_SINK_WINDOW_MISMATCH,
                pos,
                uri,
            });
        }
    }
}

/// 6025 POWER_CONVERTER_SPEC_INCOMPLETE — §6.1 partial-spec advisory. A def
/// with a `psrc`/`psbi` output row that writes a spec block but only one side
/// of the Hoare triple (input_req / output) cannot be gated by 6023/6024. The
/// written side still decodes; the missing side is what this Info names. A pure
/// load (no output row) legitimately writes `input_req` alone and is never
/// flagged here.
pub(crate) fn check_converter_spec_incomplete(
    table: &InstTable,
    results: &mut Vec<NetCheckResult>,
) {
    let deriv = WindowDeriv::new(table);
    for comp in table.get_components() {
        if comp.synthetic || comp.unselected || comp.not_fitted {
            continue;
        }
        let Some(def) = deriv.scan.def_arc(comp.id) else {
            continue;
        };
        let spec = decode_component_spec(&def);
        if !spec.has_spec {
            continue;
        }
        let (_, n_src) = pwr_row_counts(&def);
        if n_src < 1 {
            continue; // no output row → a load, whose input_req-only spec is its own
        }
        let (out_side, missing) = match (spec.input_req.is_some(), spec.output.is_some()) {
            (true, false) => (Some("input_req"), "output"),
            (false, true) => (Some("output"), "input_req"),
            _ => (None, ""), // both or neither written → not the one-sided case
        };
        let Some(written) = out_side else {
            continue;
        };
        let (pos, uri) = super::entry_pos(comp);
        let written_s = written.to_string();
        let missing_s = missing.to_string();
        results.push(NetCheckResult {
            check: "converter-spec-incomplete",
            severity: "info",
            message: crate::errcodes::format_msg(
                crate::errcodes::POWER_CONVERTER_SPEC_INCOMPLETE,
                &[&comp.path, &written_s, &missing_s],
            ),
            net_name: String::new(),
            code: crate::errcodes::POWER_CONVERTER_SPEC_INCOMPLETE,
            pos,
            uri,
        });
    }
}

/// 6026 POWER_CONVERTER_OUTPUT_RAIL_WINDOW — §6.7 converter output vs the rail
/// promise of the net it drives (rail-contract-design.md §6.7, window-notes
/// batch). A regulator's `spec.output` guarantee must sit inside the declared
/// rail window of the net its Src row lands on: the rail is that net's promise
/// (§4.1 priority 1 — the rail wins even when a converter drives the same
/// copper), so a converter guaranteeing a window the rail does not cover can
/// deliver outside what the scope allows on that net. Only Src rows landing on
/// a *declared rail face* are judged; a Src on a plain driven node (buck `LX`
/// → filter → rail net), or a degenerate rail face (a bare nominal with no ±tol
/// — no allowed spread is declared), is not — the rail promise lives on the
/// rail net and the feed side is 6023's gate.
pub(crate) fn check_converter_output_rail_window(
    table: &InstTable,
    results: &mut Vec<NetCheckResult>,
) {
    let deriv = WindowDeriv::new(table);
    for comp in table.get_components() {
        if comp.synthetic || comp.unselected || comp.not_fitted {
            continue;
        }
        let Some(def) = deriv.scan.def_arc(comp.id) else {
            continue;
        };
        let spec = decode_component_spec(&def);
        let Some((out, out_text)) = spec.output.as_ref() else {
            continue; // no output guarantee → nothing to cross-check
        };
        for pin in deriv.table.get_pins_of(comp.id) {
            if super::source_contract_for(&def, pin).is_none() {
                continue; // not a Src output row
            }
            let Some(net) = deriv.table.get_net_of(pin.id) else {
                continue;
            };
            // Only a declared rail face on the Src net carries a scope-level
            // window to cross-check (§6.7); a plain driven net's window is the
            // upstream converter's own guarantee and is not double-judged.
            let Some((rail, rail_text)) = deriv.rail_face(net) else {
                continue;
            };
            // A degenerate rail window (a bare nominal, no ±tol → [v, v]) is a
            // declaration of *no* allowed spread — the tolerance lives with the
            // converters feeding it, so there is no window to cross-check.
            if (rail.hi - rail.lo).abs() < 1e-9 {
                continue;
            }
            if rail.covers(out) {
                continue;
            }
            let (pos, uri) = super::entry_pos(comp);
            results.push(NetCheckResult {
                check: "converter-output-rail-window",
                severity: "error",
                message: crate::errcodes::format_msg(
                    crate::errcodes::POWER_CONVERTER_OUTPUT_RAIL_WINDOW,
                    &[out_text, &rail_text, &comp.path, &net.name],
                ),
                net_name: net.name.clone(),
                code: crate::errcodes::POWER_CONVERTER_OUTPUT_RAIL_WINDOW,
                pos,
                uri,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(w: &PwrWindow, lo: f64, hi: f64) {
        assert!((w.lo - lo).abs() < 1e-9, "lo {} != {}", w.lo, lo);
        assert!((w.hi - hi).abs() < 1e-9, "hi {} != {}", w.hi, hi);
    }

    #[test]
    fn around_point_no_tol_is_degenerate() {
        let w = PwrWindow::around(5.0, None);
        approx(&w, 5.0, 5.0);
        assert!(w.contains(5.0));
        assert!(!w.contains(5.0 + 1e-6));
    }

    #[test]
    fn around_tol_scales_both_sides() {
        let w = PwrWindow::around(5.0, Some(0.05));
        approx(&w, 4.75, 5.25);
        assert!(w.contains(4.75) && w.contains(5.25));
        assert!(!w.contains(4.7499));
    }

    #[test]
    fn covers_is_subset_with_slop() {
        let wide = PwrWindow { lo: 4.6, hi: 5.4 };
        assert!(wide.covers(&PwrWindow { lo: 4.75, hi: 5.25 }));
        assert!(!wide.covers(&PwrWindow { lo: 4.0, hi: 5.0 }), "low escapes");
        assert!(
            !wide.covers(&PwrWindow { lo: 5.0, hi: 5.6 }),
            "high escapes"
        );
    }

    #[test]
    fn union_extends_to_minmax() {
        let a = PwrWindow::around(5.0, Some(0.05)); // [4.75, 5.25]
        let b = PwrWindow::around(5.0, Some(0.08)); // [4.6, 5.4]
        approx(&a.union(&b), 4.6, 5.4);
        // union of a point with a window keeps the window
        let pt = PwrWindow { lo: 5.0, hi: 5.0 };
        approx(&b.union(&pt), 4.6, 5.4);
    }
}
