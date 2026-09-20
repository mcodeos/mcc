// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Pass2 Electrical Net Checks — driver conflict, floating inputs, voltage mismatch, etc.
//!
//! Runs after `mcb_pass2()` when the full flattened netlist (`InstTable`) is available.

use crate::db::diagnostic::diagnostic::Diagnostic;
use crate::instant::insttab::{InstEntry, InstKind, InstOrigin, InstTable, MemberRole, NetEntry};
use crate::semantic::basic::attr_keys;
use crate::semantic::basic::mc_kvs::KVSValue;
use crate::semantic::basic::mc_literal::McLiteral;
use crate::semantic::basic::mc_param::McParamValue;
use crate::semantic::basic::mc_uval::McUnit;
use crate::semantic::common::IOType;
use crate::semantic::component::mc_attr::McAttrVal;
use crate::semantic::component::mc_pins::{McPinPort, McPwrPin, PwrDir};
use crate::semantic::component::McComponent;
use crate::semantic::module::pi::{decode_pwr_pin, L1PwrPin, McPowerDecls, RailAxis};
use crate::semantic::pwrid::{self, Face};
use crate::semantic::validation::finding::CheckFinding;
use std::collections::HashSet;

// Rail-contract window layer (§2/§4.3/§6 interval decode + arithmetic). The
// nominal checks live in this file; window.rs is a sibling leaf consumed by
// the 6023/6024/6025 owners added in the window batch.
mod window;
pub(crate) use window::{
    check_converter_gate_window, check_converter_output_rail_window,
    check_converter_spec_incomplete, check_sink_window_mismatch,
};

// PWR-4 budget root-scope layer (rail-contract-design.md §8.5, S-set step 1).
// budget.rs is a sibling leaf like window.rs: check_net_budget moved out of
// this file (6021 owner re-exported under the same name for rules.rs).
mod budget;
pub(crate) use budget::check_net_budget;

// PWR-4 budget derived-demand leaf (§8.5 — converter push-up + OR-merge
// single-source charging on top of the declared-amp gather). budget.rs owns the
// 6021 buckets/report and folds budget_derive::BudgetLoadScan charges into them.
mod budget_derive;

// PWR-1/PWR-2 supply reach (island-attribution-design.md §7 L4 — the
// nominal face of the S-set step): reach.rs closes 6011/6019's "copper
// pass-through feed = later S-set step" gap. No re-export — the two owners
// (6011 check_sink_nominal_mismatch, 6019 check_undriven_sink_net) live in
// this file and construct reach::ReachScan directly.
mod reach;

// PWR-5 protection-device placement (exposed-protection-design.md §4). protect.rs
// is a sibling leaf like budget.rs: the two owners (6032 shunt leg must reach a
// protective/earth reference, 6033 series element must sit in series on a supply
// path) read the def-level `protect = shunt|series` declaration carried on the
// flat entry, and read the "is this net on a supply tree" face from
// reach::ReachScan.
mod protect;
pub(crate) use protect::{check_protect_series_path, check_protect_shunt_reference};

// PI-3 decoupling-return face (power-quality-design.md §2.3). decouple.rs is a
// sibling leaf like protect.rs: it reads the declared `[hot, ret]` pair through
// the same `eff_class` read this file owns (hence the `super::` accesses), and
// pairs with the ruling-8 clause in `check_return_leg_undeclared` — a capacitor
// is a DC element's complement, judged here instead of there.
mod decouple;
pub(crate) use decouple::check_decoupling_return_face;

// The declared-rail read both rules above and below judge an element by: the
// class each leg resolves to (via `eff_class`, hence `super::` there), the
// declared DC rails projected onto those classes, and the rails of a part's
// owning-scope chain. Two rules, one law — a name only ever finds the net a
// declaration wrote it for.
mod railface;

// §1.4's quiet/sensitive and noisy faces, read off the §1.1 domain projection.
// faces.rs owns the one step four rules would otherwise each re-take — which
// words make a face what it is — so PI-2, PI-4, SN-2 and SN-3 cannot drift
// apart on it.
mod faces;

// PI-2 filter-leg load-side decoupling (power-quality-design.md §2.2). bridge.rs
// is a sibling leaf: it reads the declared `@bridge` clause (the same
// `declared_dc_edges` map 6022 uses), the §1.4 quiet/sensitive face off the
// domain projection, and — for the existence question ruling 11 cut it to — the
// declared capacitor candidate. Its seam with decouple.rs is the point: where a
// capacitor's return lands is 6038's verdict, so this one never repeats it.
mod bridge;
pub(crate) use bridge::check_bridge_load_decoupling;

// PI-1 sink-pin decoupling completeness (power-quality-design.md §2.1). The
// third leaf of the family, and the one that judges the *load* terminal: where
// bridge.rs asks whether a declared filter leg's load side is decoupled and
// decouple.rs whether a capacitor's return closes the rail's loop, this asks
// whether the sink's own declared pair carries a capacitor at all. Ruling 11's
// partition is what keeps the three apart — "is there one" here, "where does it
// land" in decouple.rs.
mod sink_decouple;
pub(crate) use sink_decouple::check_sink_pin_decoupling;

// SN-3 sensitive return landing on a noisy face (power-quality-design.md §3.3,
// ruling 10 decided 2026-09-16). The fourth leaf of the family and the first of
// the two §3 rules: where the PI leaves judge a filter and a load, this judges
// a part's own declared supply pair across the §1.4 faces — the hot member on
// the quiet/sensitive side, the return member landing on the noisy side. Its
// seam with SN-2 (§3.2) is the design's own: this is the direct landing with no
// bridge (the harder error, hence §7's order), and its seam with 6027 is
// structural (a sensitive part's single return pin spans no class pair).
mod sensitive;
pub(crate) use sensitive::check_sensitive_return_on_noisy;

// SN-1 analog signal crossing a split ground (power-quality-design.md §3.1).
// The first of the two §3 rules: where SN-3 judges a part's pair *across* the
// §1.4 faces, this judges the reference a scope's analog face is *declared*
// with — the face's rail and its port rows' `@return(C)` must agree — against
// the returns the parts that face supplies actually close over. The subject is
// fixed by the two agreeing declarations — the face's rail and the port's
// `@return` — so a scope declaring two quiet faces with different references
// judges each face on its own reference, and one level is read throughout: the
// declaration, the face and the part whose supply net that scope owns. §3.1's second
// half (no `@return` declared, two sides returning on different conduits) stays
// deferred by the design (§6 R3: the source→sink chain of a signal net has no
// carrier), so this leaf only reads the declaration-vs-topology half.
mod analog_return;
pub(crate) use analog_return::check_analog_return_reference;

// SN-2 a noisy face and a quiet one sharing one DC ground bridge
// (power-quality-design.md §3.2, ruling 9). The second §3 rule, and the mirror
// of PI-2's leg: a declared ground bridge whose ends are the returns of a noisy
// face and of a quiet/sensitive one, carried by anything but a magnetic
// element, is the two references meeting through plain copper — the filter that
// would let them meet while the quiet face keeps its own reference is missing.
// Its ends are read at the name level (the declaring scope's own rails say what
// each written name is), which is what lets the design's plainest form, a direct
// copper tie, be judged at all: a tie that merges the two coppers collapses to
// one class, while the declaration still names two returns.
mod shared_return;
pub(crate) use shared_return::check_shared_return_bridge;

// PI-4 filter-subface overreach (power-quality-design.md §2.4, ruling 4). The
// axis's last rule and the one that reads the flat hardest: §2.2's supply leg
// protects a load side, and that side is the quiet domain's own declared pair —
// so the verdict is not about the leg's declaration (PI-2's object, 6037) but
// about the **contract of what draws from it**, resolved member by member on the
// sink's own instance through the flatten pass's member carry. That carry is why
// this leaf can exist at all: the flat never holds the caller's bound pair, only
// the declaring scope's member *spellings*, so the pair has to be located on the
// instance the row belongs to — the same seam PI-2 reads the leg through, read
// one level down.
mod subface;
pub(crate) use subface::check_filter_subface_overreach;

// PWR-4b package dissipation (package-thermal-design.md §3). thermal.rs is a
// sibling leaf like protect.rs: the 6035 owner reads the two quantities the
// flat entry carries (`resistance_ohm` / `power_rated_w`, decoded from the
// instance's resolved spec values), the rail pair the element sits across, and
// that rail's declared window — no solver, no new syntax.
mod thermal;
pub(crate) use thermal::check_element_dissipation;

/// Run all electrical net checks and return diagnostics.
///
/// FlatErc rules are declared — and ordered — in `crate::rules`
/// (`FLAT_ERC_RULES`); the runner executes each declared rule in declaration
/// order (§5-5 of the rule-registry design), replacing the former hand-written
/// call table. Output is byte-identical to the old sequence.
pub fn run_net_checks(table: &InstTable) -> Vec<NetCheckResult> {
    let mut results = Vec::new();
    for rule in crate::rules::flat_erc_rules() {
        (rule.run)(table, &mut results);
    }
    results
}

/// The one JSON face of the electrical net checks.
///
/// Both readouts of this engine go through here: the CLI's local path
/// (`mcc erc`, see `cmds/erc.rs`) and the RPC `erc` method. Until 2026-09-18
/// each had its own hand-written root-net engine over the string net table
/// (ERC 6001-6004), so the two faces answered the same question with different
/// rules and different counts -- on a real board they disagreed on which nets
/// are multi-driven. The engine is retired (`erc/rules-catalog-design.md` §3.2
/// maps its four checks onto this one) and the command kept; this function is
/// what "one engine" means in code.
///
/// The summary counts are derived from the results, never from a list of check
/// names: a rule added to `FLAT_ERC_RULES` shows up in `by_check` with no edit
/// here.
pub fn erc_payload(top: &str, results: &[NetCheckResult]) -> serde_json::Value {
    let mut by_check: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for r in results {
        *by_check.entry(r.check).or_default() += 1;
    }
    let violations: Vec<serde_json::Value> = results
        .iter()
        .map(|r| {
            serde_json::json!({
                "code": r.code,
                "severity": r.severity,
                "check": r.check,
                "message": r.message,
                "net_name": r.net_name,
                "pos": r.pos,
                "uri": r.uri,
            })
        })
        .collect();
    serde_json::json!({
        "top": top,
        "summary": {
            "violations": results.len(),
            "errors": results.iter().filter(|r| r.severity == "error").count(),
            "warnings": results.iter().filter(|r| r.severity == "warning").count(),
            "by_check": by_check,
        },
        "violations": violations,
    })
}

#[derive(Debug, Clone)]
pub struct NetCheckResult {
    pub check: &'static str,
    pub severity: &'static str, // "error" | "warning" | "info"
    pub message: String,
    pub net_name: String,
    pub code: u32,
    /// Source byte offset of the relevant point (0 if not available)
    pub pos: u32,
    /// Source file URI (empty if not available)
    pub uri: String,
}

/// One flat electrical net check as the build envelope carries it.
///
/// Distinct from [`NetCheckResult`] on purpose: that one borrows `&'static str`
/// labels and cannot be deserialized, while an envelope has to survive the wire
/// in both directions — the CLI reads a delegated `build.full` payload back into
/// the same type a local build wrote.
///
/// These rows are **not** diagnostics. `mcc build` reports them as their own
/// console section (`=== Electrical Net Checks ===`) and the envelope carries
/// them under `pass2.net_checks`, so they never enter `pass2.diagnostics` and
/// never count into `summary` (build-design §3.7 discipline 4's sibling: the
/// same argv must answer with the same shape, here and in the daemon).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NetCheckRow {
    pub check: String,
    /// "error" | "warning" | "info"
    pub severity: String,
    pub message: String,
    pub net_name: String,
    pub code: u32,
    /// Source byte offset of the relevant point (0 if not available).
    pub pos: u32,
    /// Source file URI (empty if not available).
    pub uri: String,
}

impl From<&NetCheckResult> for NetCheckRow {
    fn from(r: &NetCheckResult) -> Self {
        NetCheckRow {
            check: r.check.to_string(),
            severity: r.severity.to_string(),
            message: r.message.clone(),
            net_name: r.net_name.clone(),
            code: r.code,
            pos: r.pos,
            uri: r.uri.clone(),
        }
    }
}

/// Project net-check results onto the envelope's row shape. One producer for
/// both build faces — see [`NetCheckRow`].
pub fn net_check_rows(results: &[NetCheckResult]) -> Vec<NetCheckRow> {
    results.iter().map(NetCheckRow::from).collect()
}

/// Convert net-check results into ready-to-log `Diagnostic`s (dianlu-tree
/// Phase A). Each carrier is first projected onto the unified
/// [`CheckFinding`] line (`finding.rs`), then flattened by the finding's
/// `to_diagnostic`; the mapping reproduces the former severity-string match
/// and uri/pos passthrough byte-for-byte. A result without a uri keeps an
/// empty uri and [`log_net_check_diagnostics`] falls back to the caller's
/// `current_uri` at log time.
///
/// This is the FlatErc emission choke of the rule-registry normalization
/// point (§8-5): each finding is adjudicated against the process-wide
/// override store before it becomes a diagnostic. Under the empty (or fully
/// non-overridable) store the output is byte-identical to the legacy mapping.
pub fn net_results_to_diagnostics(results: &[NetCheckResult]) -> Vec<Diagnostic> {
    let mut out = Vec::with_capacity(results.len());
    for r in results {
        let finding = CheckFinding::from(r.clone());
        let Some(effective) =
            crate::db::diagnostic::override_store::with_store(|s| s.apply_to_finding(&finding))
        else {
            continue; // allow hit suppresses the finding at the display layers
        };
        out.push(effective.to_diagnostic());
    }
    out
}

/// Log net-check diagnostics at their own uris (dianlu-tree Phase A
/// caller-side helper). A diagnostic with an empty uri falls back to the
/// caller's `current_uri` — the entry module's file. Replaces the logging
/// that used to live inside `DianLu::flatten`.
pub fn log_net_check_diagnostics(diags: &[Diagnostic]) {
    for d in diags {
        let uri = if d.loc.uri.is_empty() {
            crate::current_uri::try_get().unwrap_or_default()
        } else {
            d.loc.uri.clone()
        };
        crate::db::diagnostic::diagnostic::diagnostic_log_at(
            d.code,
            d.level,
            uri,
            d.loc.pos,
            d.loc.len,
            &d.msg,
            &[],
        );
    }
}

/// Extract the best available source position from an InstEntry.
/// `src_pos` is the wiring site (preferred); `fallback_pos` is the declaration
/// site used for unconnected pins/ports; `(0, uri)` is the last resort. The
/// precedence is [`InstEntry::anchor_pos`]'s, not a second copy of it.
fn entry_pos(entry: &InstEntry) -> (u32, String) {
    if let Some(p) = entry.anchor_pos() {
        return (p.offset, p.uri.clone());
    }
    (0, entry.def_uri.clone())
}

/// §2.19: an entry is NC when its iotype is `NonCon` — the `nc` direction word,
/// the only definition-site spelling — or, since U48, when an instance-site
/// `@ncpin(…)` marker names it. A pin *named* `NC` is an ordinary pin
/// (`erc/nc-design.md` §4.1: names carry no NC semantics).
///
/// The instance-level arm is a suppression marker, not a prohibition: a marked
/// pin that is *also* wired is legal (E4109 stays untouched).
fn is_nc_entry(entry: &InstEntry) -> bool {
    matches!(entry.io_type, IOType::NonCon) || entry.nc_marked
}

/// Find the first InstEntry that has a source position among a set of point IDs.
fn best_pos(table: &InstTable, ids: &[u32]) -> (u32, String) {
    for id in ids {
        if let Some(entry) = table.get_entry(*id) {
            if let Some(p) = entry.src_pos.first() {
                return (p.offset, p.uri.clone());
            }
        }
    }
    // Fallback: any entry — prefer a declaration (fallback) position over (0, uri)
    for id in ids {
        if let Some(entry) = table.get_entry(*id) {
            if let Some(p) = &entry.fallback_pos {
                return (p.offset, p.uri.clone());
            }
        }
    }
    for id in ids {
        if let Some(entry) = table.get_entry(*id) {
            if !entry.def_uri.is_empty() {
                return (0, entry.def_uri.clone());
            }
        }
    }
    (0, String::new())
}

// ── P1: Multiple outputs driving the same net ──
pub(crate) fn check_driver_conflict(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    for net in table.get_nets() {
        // Only true signal `Out` pins are "drivers" for short-circuit purposes.
        // Power pins are rails by design — same-name multi-pin groups broadcast
        // every member onto one net (`[[19,32,48,64],[18]]=[VDD,VSS]` → four VDD
        // pins share the VDD rail), so multiple Power entries on a net are normal,
        // not a short. Power-rail conflicts (e.g. two different supplies tied)
        // are handled by NET_VOLTAGE_MISMATCH (E4104) instead.
        let out_ids: Vec<u32> = net
            .points
            .iter()
            .filter_map(|id| table.get_entry(*id))
            .filter(|e| matches!(e.io_type, IOType::Out))
            .map(|e| e.id)
            .collect();
        // Vantage dedupe (rule-registry design §4-a): in the A' flat model a
        // module-boundary `out` port is a junction — the exit of the net segment
        // inside its own module instance. When that exit's net already carries an
        // `Out` point rooted inside the port's module (the internal driver it
        // forwards), the port is not a second physical driver; counting both
        // double-counts one signal (e.g. `SUB { out Y; BUF b; b.Y -> Y }`
        // instanced as `s1` puts `s1.Y` and `s1.b.2` on one internal net). The
        // port still counts when no interior driver shares the net — two out
        // ports on a parent net (`s1.Y -> s2.Y`) are two different modules' real
        // drivers shorted, exactly what E4101 exists to catch.
        let drivers: Vec<&InstEntry> = out_ids
            .iter()
            .filter(|&&id| !is_module_out_port_exit(table, id, &out_ids))
            .filter_map(|id| table.get_entry(*id))
            .collect();
        if drivers.len() > 1 {
            let names: Vec<_> = drivers.iter().map(|e| e.path.as_str()).collect();
            let (pos, uri) = entry_pos(drivers[0]);
            results.push(NetCheckResult {
                check: "driver-conflict",
                severity: "error",
                message: format!(
                    "Net '{}' has {} drivers: {}. Possible short circuit.",
                    net.name,
                    drivers.len(),
                    names.join(", ")
                ),
                net_name: net.name.clone(),
                code: crate::errcodes::NET_MULTI_DRIVE,
                pos,
                uri,
            });
        }
    }
}

/// §4-a vantage: is `id` a module-boundary `out` port that is merely the exit
/// of an interior driver already present on the same net?
fn is_module_out_port_exit(table: &InstTable, id: u32, net_out_ids: &[u32]) -> bool {
    let Some(port) = table.get_entry(id) else {
        return false;
    };
    if !matches!(port.kind, InstKind::Port) {
        return false;
    }
    let Some(owner) = port.parent_id else {
        return false;
    };
    net_out_ids.iter().any(|&oid| {
        if oid == id {
            return false;
        }
        let Some(other) = table.get_entry(oid) else {
            return false;
        };
        // A sibling out port of the same module instance is a distinct signal
        // of that module, not this port's interior driver.
        if other.kind == InstKind::Port && other.parent_id == Some(owner) {
            return false;
        }
        // Walk the other point's parent chain: the port's own module instance
        // must appear strictly between it and the root.
        let mut cur = other.parent_id;
        while let Some(pid) = cur {
            if pid == owner {
                return true;
            }
            cur = table.get_entry(pid).and_then(|e| e.parent_id);
        }
        false
    })
}

// ── P2: Nets with only input endpoints (no driver) ──
pub(crate) fn check_undriven_nets(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    for net in table.get_nets() {
        let points: Vec<&InstEntry> = net
            .points
            .iter()
            .filter_map(|id| table.get_entry(*id))
            .collect();
        let has_driver = points
            .iter()
            .any(|e| matches!(e.io_type, IOType::Out | IOType::Power));
        let has_input = points
            .iter()
            .any(|e| matches!(e.io_type, IOType::In | IOType::InOut));
        // The former exemption — "a net named after an implicit power rail
        // (`VCC`, `GND`, `V3V3`, …) is a source by convention" — is retired
        // (U56). It is redundant now: a **declared** supply member is
        // `IOType::Power` on the flat table, which `has_driver` above already
        // counts, so a declared rail passes structurally. A net that only
        // *looks* like a rail carries no declaration and therefore no promise
        // of a source (world-axioms §1 A1) — it is undriven until something
        // declares it driven.
        // A net containing a module-boundary port (`x.signal -> D_STATUS.1`)
        // receives its drive from the enclosing scope through that port; the
        // flat table keeps the parent-side connection as a separate net sharing
        // the port entry, so drive cannot be judged here. Port connectivity is
        // checked at the boundary instead (check_floating_inputs / C4
        // unused-module-port / check_floating_outputs).
        let has_boundary_port = points
            .iter()
            .any(|e| matches!(e.kind, crate::instant::insttab::InstKind::Port));
        if has_boundary_port {
            continue;
        }
        if !has_driver && has_input && !points.is_empty() {
            let (pos, uri) = best_pos(table, &net.points);
            results.push(NetCheckResult {
                check: "undriven-net",
                severity: "warning",
                message: format!("Net '{}' has inputs but no output/power driver.", net.name),
                net_name: net.name.clone(),
                code: crate::errcodes::NET_NO_DRIVER,
                pos,
                uri,
            });
        }
    }
}

// ── P5: Input ports with no net connection ──
// §A′-scope: directional float checks (E4108 / E-output-undriven / E4117)
// own *component pads* (kind Pin) only. Module-boundary ports (kind Port) are
// owned by C4/E4114 (`check_unused_module_ports`); A′ bus-slash / bare-member
// alias entries (kind Label) and other non-physical spellings never carry a
// directional float — the physical member Port or pad reports for them. This
// split is what stops the same unconnected pad from being double-reported by
// a directional check and by C4.
pub(crate) fn check_floating_inputs(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let connected: HashSet<u32> = table
        .get_nets()
        .iter()
        .flat_map(|n| n.points.iter().cloned())
        .collect();
    for (_, entry) in table.iter() {
        // Same synthetic-wrapper carve-out as E4112/E4116: a virtually-
        // instantiated component/interface is never wired by definition, so
        // its unwired pins are the normal shape of the view, not a defect.
        if matches!(entry.io_type, IOType::In)
            && matches!(entry.kind, InstKind::Pin)
            && !connected.contains(&entry.id)
            && !is_nc_entry(entry)
            && !entry.synthetic
        {
            let (pos, uri) = entry_pos(entry);
            results.push(NetCheckResult {
                check: "floating-input",
                severity: "warning",
                message: format!("Input '{}' is not connected to any net.", entry.path),
                net_name: entry.path.clone(),
                code: crate::errcodes::NET_INPUT_UNCONNECTED,
                pos,
                uri,
            });
        }
    }
}

// ── P6: NC port connected to a net ──
pub(crate) fn check_nc_connected(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    for net in table.get_nets() {
        for id in &net.points {
            if let Some(entry) = table.get_entry(*id) {
                if matches!(entry.io_type, IOType::NonCon) {
                    let (pos, uri) = entry_pos(entry);
                    results.push(NetCheckResult {
                        check: "nc-connected",
                        severity: "warning",
                        message: format!(
                            "NC port '{}' is connected to net '{}'.",
                            entry.path, net.name
                        ),
                        net_name: net.name.clone(),
                        code: crate::errcodes::NET_NC_CONNECTED,
                        pos,
                        uri,
                    });
                }
            }
        }
    }
}

// ── P7: Output ports with no net connection ──
pub(crate) fn check_unconnected_outputs(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let connected: HashSet<u32> = table
        .get_nets()
        .iter()
        .flat_map(|n| n.points.iter().cloned())
        .collect();
    for (_, entry) in table.iter() {
        // Same synthetic-wrapper carve-out as E4112/E4116: a virtually-
        // instantiated component/interface is never wired by definition, so
        // its unwired output pins are the normal shape of the view.
        // §A′-scope: component pads only (kind Pin); module-boundary ports go
        // to C4/E4114, A′ alias spellings never carry a directional float.
        if matches!(entry.io_type, IOType::Out)
            && matches!(entry.kind, InstKind::Pin)
            && !connected.contains(&entry.id)
            && !is_nc_entry(entry)
            && !entry.synthetic
        {
            let (pos, uri) = entry_pos(entry);
            results.push(NetCheckResult {
                check: "unconnected-output",
                severity: "warning",
                message: format!("Output '{}' drives nothing.", entry.path),
                net_name: entry.path.clone(),
                code: crate::errcodes::NET_OUTPUT_UNDRIVEN,
                pos,
                uri,
            });
        }
    }
}

// ── P3+P4: Voltage mismatch between power pins on the same net ──
//
// The declared operating voltage of a power pin is READ from the pin's
// declaration — its attribute KVS (`voltage` / `volt` key) or, for pins bound
// to a power interface, the interface binding's volt parameter (`::DC(3.3V)`).
// No voltage is ever guessed from net names: the old net-name heuristic
// (`VCC_5V` → 5.0, `3V3` → 3.3) is gone. A pin that declares a *range*
// (`2.5V~5.5V`, `±`) is a tolerance statement, not a fixed rail, and is
// skipped. A net carrying two supply pins whose declared voltage sets share
// no common value (within tolerance) is reported as a short.
pub(crate) fn check_voltage_mismatch(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    const TOL: f64 = 0.5;
    for net in table.get_nets() {
        // (pin path, declared alternative voltages) for supply pins on this net
        let mut declared: Vec<(String, Vec<f64>)> = Vec::new();
        for &pid in &net.points {
            let Some(entry) = table.get_entry(pid) else {
                continue;
            };
            if !matches!(entry.kind, InstKind::Pin) {
                continue;
            }
            let Some(voltages) = pin_declared_voltages(table, entry) else {
                continue;
            };
            if voltages.is_empty() {
                continue;
            }
            declared.push((entry.path.clone(), voltages));
        }
        if declared.len() < 2 {
            continue;
        }
        // Conflict: two pins whose declared sets share no value within TOL.
        for i in 0..declared.len() {
            let (p1, v1) = &declared[i];
            let mut conflicted = false;
            for j in i + 1..declared.len() {
                let (p2, v2) = &declared[j];
                let compatible = v1.iter().any(|a| v2.iter().any(|b| (a - b).abs() <= TOL));
                if compatible {
                    continue;
                }
                let (pos, uri) = best_pos(table, &net.points);
                results.push(NetCheckResult {
                    check: "voltage-mismatch",
                    severity: "error",
                    message: format!(
                        "Net '{}': power pins '{}' ({}V) and '{}' ({}V) declare \
                         incompatible voltages; they may be shorted.",
                        net.name,
                        p1,
                        fmt_voltages(v1),
                        p2,
                        fmt_voltages(v2)
                    ),
                    net_name: net.name.clone(),
                    code: crate::errcodes::NET_VOLTAGE_MISMATCH,
                    pos,
                    uri,
                });
                conflicted = true;
                break;
            }
            if conflicted {
                break;
            }
        }
    }
}

/// Read the operating voltages a pin declares, from its definition:
///
/// 1. **Attribute KVS** — the pin's `voltage` / `volt` key (e.g.
///    `voltage:3.3V`, `voltage:[1.2V, 1.3V]`). Read from `McPin.values`.
/// 2. **Interface binding** — for pins bound to a power interface
///    (`[VDD, GND]::DC(3.3V)`, `VIN{Vin, GND}::DC(5V)`), the interface
///    binding's volt parameter. Found by locating the `McPinPort::Interface`
///    whose `registered_pins` / member names include this pin.
///
/// Only **declared supply** pins are voltage sources: a signal pin's `voltage`
/// attribute describes signal levels, not the rail, and a declared return is
/// the reference path, which never participates. Range values (`2.5V~5.5V`) are
/// skipped — they declare tolerance, not a fixed rail. Returns `None` when the
/// pin is not a supply pin or declares no concrete voltage.
fn pin_declared_voltages(table: &InstTable, entry: &InstEntry) -> Option<Vec<f64>> {
    let comp_entry = entry.parent_id.and_then(|pid| table.get_entry(pid))?;
    if comp_entry.class_name.is_empty() {
        return None;
    }
    let comps = crate::definition_space().workspace_components();
    let def = comps
        .iter()
        .find(|(sn, _)| sn.ident.to_string() == comp_entry.class_name)
        .map(|(_, c)| c)?;

    let pin_id = entry.path.rsplit('.').next().unwrap_or("");
    let pin = def.pins.pins.get(pin_id).or_else(|| {
        def.pins
            .pins
            .values()
            .find(|p| p.names.iter().any(|n| n == &entry.class_name))
    })?;
    let names: Vec<&str> = pin.names.iter().map(|n| n.as_str()).collect();

    // Supply pin only, decided by what the pin's owner **declared** it to be.
    // A declared return (`Face::Ret` — the second member of a
    // `psrc/psnk/psbi … ::DC(hot, ret)` pair, or a rail's `ret`) is the
    // reference path and never declares a rail, even though it is typically
    // `Power`-typed; a shared return pin legitimately belongs to several rails
    // (the exposed pad of a multi-rail MCU is one), so it must not seed a
    // voltage comparison. A `conduit` names copper without picking a side and
    // is not a supply either.
    //
    // The former test spelled this out as a word table (`is_ground_name`,
    // `is_supply_name`, plus the literals `GND`/`VSS`/`VSSA`/`EPAD`): whether a
    // pin counted as a supply depended on what the author called it, which
    // world-axioms §1 A1 rules out. The declaration says the same thing and is
    // checkable — `[VCC, EPAD]::DC(3.3V)` declares EPAD a return by position.
    // A pin with no declaration is still a supply candidate when its own
    // direction is `Power` (the carried role).
    let declared = pwrid::member_of_names(&def.pins, &names);
    if let Some(m) = &declared {
        if m.face != Face::Hot {
            return None;
        }
    }
    let is_supply = declared.is_some() || matches!(pin.iotype, IOType::Power);
    if !is_supply {
        return None;
    }

    let mut out: Vec<f64> = Vec::new();
    // 1) Attribute KVS voltage.
    for kvs in crate::semantic::component::mc_pins::pin_kvs_where(pin, |key| {
        crate::semantic::basic::attr_keys::is_voltage_key(
            key,
            crate::semantic::basic::attr_keys::AttrFace::PinRow,
        )
    }) {
        collect_kvs_voltage(&kvs.value, &mut out);
    }
    // 2) Interface binding volt parameter.
    for port in def.pins.names_to_id.values() {
        let McPinPort::Interface(iface) = port else {
            continue;
        };
        let owns_pin = iface.registered_pins.iter().any(|r| r == &pin_id)
            || iface
                .pin_name_mapping
                .iter()
                .any(|n| n == &entry.class_name);
        if !owns_pin {
            continue;
        }
        for p in &iface.params {
            if let McParamValue::UValue(uv) = p {
                if matches!(uv.unit(), McUnit::Volt) && !uv.is_range_or_plusminus() {
                    out.push(uv.value());
                }
            }
        }
    }
    out.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    out.dedup();
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Extract scalar voltages from a KVS value:
/// `Const(Keyword("3.3V"))` → 3.3; `Square([Uval(1.2V), Uval(1.3V)])` →
/// [1.2, 1.3]; nested `low:`/`high:` sub-keys are recursed. Ranges
/// (`0V ~ 0.7V`) are skipped — only concrete scalar volts count.
fn collect_kvs_voltage(value: &KVSValue, out: &mut Vec<f64>) {
    match value {
        KVSValue::Const(c) => {
            let crate::semantic::basic::mc_literal::McConst::Keyword(s) = c;
            if let Some(v) = parse_voltage_str(s) {
                out.push(v);
            }
        }
        KVSValue::Square(vals) => {
            for a in vals {
                match a {
                    McAttrVal::AttrLiteral(McLiteral::Uval(uv)) => {
                        if matches!(uv.unit(), McUnit::Volt) && !uv.is_range_or_plusminus() {
                            out.push(uv.value());
                        }
                    }
                    McAttrVal::KVS(nested) => collect_kvs_voltage(&nested.value, out),
                    _ => {}
                }
            }
        }
        KVSValue::Nested(list) => {
            for k in list {
                collect_kvs_voltage(&k.value, out);
            }
        }
    }
}

/// Parse a scalar voltage text to volts: `"3.3V"` → 3.3, `"3V3"` → 3.3,
/// `"5"` → 5.0. Non-numeric / symbolic values (`0.7*VDD`, ranges) return None.
fn parse_voltage_str(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() || s.contains('~') || s.contains('±') || s.contains('*') {
        return None;
    }
    let digits = s.strip_suffix(['V', 'v']).unwrap_or(s);
    if digits.contains('V') || digits.contains('v') {
        digits
            .replace(['V', 'v'], ".")
            .trim_matches('.')
            .parse()
            .ok()
    } else {
        digits.parse().ok()
    }
}

/// Format a declared-voltage set for display: `[3.3]` → `3.3`, `[1.2, 3.3]` → `1.2/3.3`.
fn fmt_voltages(vs: &[f64]) -> String {
    vs.iter()
        .map(|v| format!("{v}"))
        .collect::<Vec<_>>()
        .join("/")
}

// ── P9: Component instances with no pins connected to any net ──
pub(crate) fn check_unwired_instances(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let connected: HashSet<u32> = table
        .get_nets()
        .iter()
        .flat_map(|n| n.points.iter().cloned())
        .collect();
    for (_, entry) in table.iter() {
        // Skip synthetic virtual-instantiation wrappers: a component/interface
        // viewed standalone is by definition unwired — the E4112 unwired check
        // is meaningless for the fabricated VIRT_* unit (its whole point is a
        // box with no nets), and it fires on every such view.
        if matches!(entry.kind, crate::instant::insttab::InstKind::Component)
            && !entry.class_name.is_empty()
            && !entry.synthetic
        {
            let pins = table.get_pins_of(entry.id);
            // ★ U48: an instance whose every pin is intentionally unconnected
            // is *deliberately* unwired, and this report is one of the things
            // the marker exists to suppress. Both ways of saying so count as
            // one fact — the class-level `nc` (`nc 2 = B`) and the instance-site
            // `@ncpin(1,2)` — so a mixed instance closes as one instance must;
            // `is_nc_entry` is that one predicate. Only the all-open case: a
            // partly open instance still has pins somebody wants wired, and
            // E4112 speaks about the instance, not about one pin.
            let all_open = !pins.is_empty() && pins.iter().all(|p| is_nc_entry(p));
            if !all_open && !pins.is_empty() && pins.iter().all(|p| !connected.contains(&p.id)) {
                let (pos, uri) = entry_pos(entry);
                results.push(NetCheckResult {
                    check: "unwired-instance",
                    severity: "warning",
                    message: format!(
                        "Instance '{}' has no pins connected to any net.",
                        entry.path
                    ),
                    net_name: entry.path.clone(),
                    code: crate::errcodes::NET_INSTANCE_UNCONNECTED,
                    pos,
                    uri,
                });
            }
        }
    }
}

// ── P8: Output connected to PowerSupply (backfeed risk) ──
pub(crate) fn check_backfeed(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    for net in table.get_nets() {
        let has_out = net.points.iter().any(|id| {
            table
                .get_entry(*id)
                .map_or(false, |e| matches!(e.io_type, IOType::Out))
        });
        let has_ps = net.points.iter().any(|id| {
            table
                .get_entry(*id)
                .map_or(false, |e| matches!(e.io_type, IOType::Power))
        });
        if has_out && has_ps {
            let (pos, uri) = best_pos(table, &net.points);
            results.push(NetCheckResult {
                check: "backfeed-risk",
                severity: "warning",
                message: format!(
                    "Net '{}' has both output and power supply. Backfeed risk.",
                    net.name
                ),
                net_name: net.name.clone(),
                code: crate::errcodes::NET_BACKFEED_RISK,
                pos,
                uri,
            });
        }
    }
}

// ── D7: PULLUP_DEGENERATE — pullup/pulldown degraded into a signal bridge ──
// unified-twopin-no-builtin §2.6: after wiring, scan Pullup/Pulldown resistor
// `this{1}`/`this{2}` nets. A pullup is a component instance produced by a
// `func Pullup(...)` / `func Pulldown(...)` method dispatch — tagged at
// instantiation time by the method-name origin marker (M0-B-E.1). A plain
// series resistor has no method provenance and is not scanned.
//
// Rail detection uses network identity (IOType::Power / inferred Ground/Power
// member role) instead of the old name-prefix heuristic. Both ends non-rail →
// the pullup degenerated into a signal-signal bridge (E4056), e.g.
// `Pullup(SCL, SDA)` shorting two signals instead of pulling one up to a rail.
pub(crate) fn check_pullup_degenerate(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let nets: Vec<&NetEntry> = table.get_nets();
    let net_of = |pin_id: u32| -> Option<&NetEntry> {
        nets.iter().find(|n| n.points.contains(&pin_id)).copied()
    };
    let net_is_rail = |net: &NetEntry| -> bool {
        net.points.iter().any(|id| {
            table.get_entry(*id).map_or(false, |e| {
                matches!(e.io_type, IOType::Power)
                    || e.member_info.as_ref().map_or(false, |m| {
                        matches!(m.role, MemberRole::Power | MemberRole::Ground)
                    })
            })
        })
    };
    for (_, entry) in table.iter() {
        let fn_name = match &entry.origin {
            InstOrigin::FuncCall { fn_name, .. } => fn_name.as_str(),
            _ => continue,
        };
        let is_pull = fn_name == "Pullup" || fn_name == "Pulldown";
        if !is_pull || !matches!(entry.kind, InstKind::Component) {
            continue;
        }
        let pins = table.get_pins_of(entry.id);
        if pins.len() < 2 {
            continue;
        }
        let (Some(n1), Some(n2)) = (net_of(pins[0].id), net_of(pins[1].id)) else {
            continue;
        };
        if net_is_rail(n1) || net_is_rail(n2) {
            continue;
        }
        let (pos, uri) = entry_pos(entry);
        results.push(NetCheckResult {
            check: "pullup-degenerate",
            severity: "warning",
            message: crate::errcodes::format_msg(
                crate::errcodes::PULLUP_DEGENERATE,
                &[&fn_name, &n1.name, &n2.name],
            ),
            net_name: n1.name.clone(),
            code: crate::errcodes::PULLUP_DEGENERATE,
            pos,
            uri,
        });
    }
}

// ── V1: Module ports with mismatched IO directions on same net ──
pub(crate) fn check_port_io_mismatch(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    for net in table.get_nets() {
        let mut has_in = false;
        let mut _has_out = false;
        let mut has_ps = false;
        let mut out_count = 0u32;
        for id in &net.points {
            if let Some(e) = table.get_entry(*id) {
                has_in |= matches!(e.io_type, IOType::In);
                _has_out |= matches!(e.io_type, IOType::Out);
                has_ps |= matches!(e.io_type, IOType::Power);
                if matches!(e.io_type, IOType::Out) {
                    out_count += 1;
                }
            }
        }
        if out_count > 1 && !has_in && has_ps {
            let (pos, uri) = best_pos(table, &net.points);
            results.push(NetCheckResult {
                check: "port-io-mismatch",
                severity: "warning",
                message: format!(
                    "Net '{}' has {} outputs and power but no input.",
                    net.name, out_count
                ),
                net_name: net.name.clone(),
                code: crate::errcodes::NET_OUTPUTS_NO_INPUT,
                pos,
                uri,
            });
        }
    }
}

// ── Power net summary ──
pub(crate) fn check_power_nets(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let mut count = 0u32;
    for net in table.get_nets() {
        for id in &net.points {
            if let Some(e) = table.get_entry(*id) {
                if matches!(e.io_type, IOType::Power) {
                    count += 1;
                    break;
                }
            }
        }
    }
    if count > 10 {
        // This is a whole-design summary, not a per-net problem: anchor it at
        // the built module's own `module <name>` header (recorded at flatten
        // time) so it does not collapse onto file:1:1. `None` only when no root
        // span was recorded — then fall back to the historical pos 0.
        let (pos, uri) = match table.root_span() {
            Some(sp) => (sp.offset, sp.uri.clone()),
            None => (0, String::new()),
        };
        results.push(NetCheckResult {
            check: "power-net-count",
            severity: "info",
            message: format!("Design has {} power nets. Review for consolidation.", count),
            net_name: String::new(),
            code: crate::errcodes::NET_POWER_NET_COUNT,
            pos,
            uri,
        });
    }
}

// ── C4: Module boundary ports not connected to any net ──
pub(crate) fn check_unused_module_ports(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    // `connected` = every id a net carries, **plus every ancestor of one** along
    // `parent_id`. The upward walk is the old E3 semantics for a bundle port --
    // "any lane wired ⇒ the port is connected" (`erc/rules-catalog-design.md`
    // §3.2 ②) -- and it is the only way an interface-typed aggregate can be read
    // as wired at all: `io I2C0` has no dotted member `Port`, so its members
    // exist only as `/` lane `Label`s, which are the ids the nets carry.
    // Measured on hbl: without the walk `main.MCU513.I2C0` / `main.MCU513.UART0`
    // are reported unwired while all four of their lane Labels are on nets.
    // A port whose lanes are all unwired (`main.MCU513.UART1`) still reports —
    // the upward walk never reaches it. See CIMP §1 U97 (both halves landed:
    // the predicate here and the registration-side fold in `insttab`).
    let connected: HashSet<u32> = {
        let mut set: HashSet<u32> = HashSet::new();
        for id in table
            .get_nets()
            .iter()
            .flat_map(|n| n.points.iter().cloned())
        {
            set.insert(id);
            let mut cur = table.get_entry(id).and_then(|e| e.parent_id);
            let mut guard = 0usize;
            while let Some(pid) = cur {
                // Bounded by the table size: a corrupt parent cycle must not hang
                // the check, and a re-visited id means the chain is already walked.
                if guard > table.len() || !set.insert(pid) {
                    break;
                }
                guard += 1;
                cur = table.get_entry(pid).and_then(|e| e.parent_id);
            }
        }
        set
    };
    let top_id = table
        .iter()
        .find(|(_, e)| {
            matches!(e.kind, crate::instant::insttab::InstKind::Module) && e.parent_id.is_none()
        })
        .map(|(id, _)| *id);
    for (_, entry) in table.iter() {
        // §A′-scope: this check owns *module-boundary ports* — entries whose
        // kind is Port (the aggregate or dotted member a parent design wires).
        // Component pads (kind Pin) are owned by the directional float checks
        // (E4108 / E-output-undriven / E4117); A′ bus-slash/bare alias spellings
        // (kind Label) are folded onto the Port and never surface here. The
        // former `class_name` guard (which only selected class-carrying pads)
        // is what made C4 re-report Pin pads that the directional checks had
        // already reported — the ldo.4 (E4108+E4114) and UC.8 (E4117+E4114)
        // duplicates on hbl.
        //
        // The top module's own ports are skipped because the module-scope face
        // already owns them: E5162 for a header-declared port, E5642 for a body
        // `io` (measured -- see `erc/rules-catalog-design.md` §3.2).
        //
        // `connected` counts a port as wired when a net carries the port id or
        // **any id under it** (see the construction above). Both halves are
        // needed on a real board: a bundle whose members are enumerated as
        // dotted `Port`s is wired through a member id, and an interface-typed
        // aggregate (`io I2C0`) is wired through a `/` lane `Label` it never
        // names. Measured on hbl before the widening: 6 of the 11 rows were
        // false positives (4 SPI member Ports, 2 interface aggregates), and the
        // 5 that remain are true (`UART1` plus `port1.A`-`D`, whose lanes are on
        // no net). See `log/9.18.bundle-port-readout.md` and CIMP §1 U97.
        if entry.parent_id == top_id || entry.parent_id.is_none() {
            continue;
        }
        if !matches!(entry.kind, crate::instant::insttab::InstKind::Port) {
            continue;
        }
        if matches!(
            entry.io_type,
            IOType::In | IOType::Out | IOType::InOut | IOType::Power
        ) && !connected.contains(&entry.id)
            && !is_nc_entry(entry)
            // Same synthetic-wrapper carve-out as E4112/E4116: a virtually-
            // instantiated component/interface is never wired by definition,
            // so its boundary ports are the normal shape of the view.
            && !entry.synthetic
        {
            let (pos, uri) = entry_pos(entry);
            results.push(NetCheckResult {
                check: "unused-module-port",
                severity: "warning",
                message: format!(
                    "Module port '{}' ({:?}) is not connected to any net.",
                    entry.path, entry.io_type
                ),
                net_name: entry.path.clone(),
                code: crate::errcodes::NET_MODULE_PORT_UNCONNECTED,
                pos,
                uri,
            });
        }
    }
}

// ── Single-point nets (self-loop or isolated point) ──
pub(crate) fn check_single_point_nets(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    for net in table.get_nets() {
        if net.points.len() == 1 {
            if let Some(entry) = table.get_entry(net.points[0]) {
                let (pos, uri) = entry_pos(entry);
                results.push(NetCheckResult {
                    check: "single-point-net",
                    severity: "warning",
                    message: format!(
                        "Net '{}' has only one endpoint: '{}'. Possible dangling connection.",
                        net.name, entry.path
                    ),
                    net_name: net.name.clone(),
                    code: crate::errcodes::NET_DANGLING_ENDPOINT,
                    pos,
                    uri,
                });
            }
        }
    }
}

// ── Pin count mismatch: instance has fewer connected pins than component defines ──
pub(crate) fn check_pin_count_mismatch(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let connected: HashSet<u32> = table
        .get_nets()
        .iter()
        .flat_map(|n| n.points.iter().cloned())
        .collect();
    let comps = crate::definition_space().workspace_components();
    for (_, entry) in table.iter() {
        // Same synthetic-wraper carve-out as E4112: a virtually-instantiated
        // component is never wired, so "N of M pins connected" (E4116) is a
        // guaranteed false positive on every component/interface file view.
        if !matches!(entry.kind, crate::instant::insttab::InstKind::Component)
            || entry.class_name.is_empty()
            || entry.synthetic
        {
            continue;
        }
        if let Some(def) = comps
            .iter()
            .find(|(sn, _)| sn.ident.to_string() == entry.class_name)
            .map(|(_, c)| c)
        {
            // Count distinct non-NC physical pads, not name entries. A single
            // pad can carry several alternate signal names (`2 = SO | IO1`) or
            // receive an interface binding (`[1,2,5,6] = SPI::SPI("Slave")`),
            // each registering its own key in `names_to_id`; the connected-pin
            // numerator below counts pads, so the denominator must too —
            // otherwise every multi-aliased part reports "N of M pins
            // connected" even when fully wired (E4116). NC pads are
            // intentionally unconnected and never counted (OR semantics §2.19).
            let def_pin_count = def.pins.pins.values().filter(|p| !p.is_nc).count();
            if def_pin_count == 0 {
                continue;
            }
            let pins = table.get_pins_of(entry.id);
            let connected_pins = pins.iter().filter(|p| connected.contains(&p.id)).count();
            // ★ U48: an instance-site `@ncpin(…)` marker removes the pin from
            // the denominator the same way a class-level `nc` does. Only the
            // *unconnected* marked pins: a marked pin that is nevertheless wired
            // is connected in the numerator, so subtracting it here would turn
            // a complete part into a false "N of M-1".
            let marked_unconnected = pins
                .iter()
                .filter(|p| p.nc_marked && !connected.contains(&p.id))
                .count();
            let def_pin_count = def_pin_count.saturating_sub(marked_unconnected);
            if def_pin_count == 0 {
                continue;
            }
            if connected_pins < def_pin_count {
                let (pos, uri) = entry_pos(entry);
                results.push(NetCheckResult {
                    check: "pin-count-mismatch",
                    severity: "warning",
                    message: format!(
                        "'{}' has {} of {} pins connected.",
                        entry.path, connected_pins, def_pin_count
                    ),
                    net_name: entry.path.clone(),
                    code: crate::errcodes::NET_PARTIAL_CONNECTION,
                    pos,
                    uri,
                });
            }
        }
    }
}

// Abstract placed unselected (abstract-variant plan §6.1)
/// An instance whose def is an `abstract component` carries the `unselected`
/// marker set at flatten time (never inferred here from a `partno` sentinel —
/// abstract defs may legally hold a reference partno). Placement is legal and
/// the netlist is produced, but a BOM tool must still pick a variant: ERC W.
pub(crate) fn check_unselected_abstract(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    for (_, entry) in table.iter() {
        // Same synthetic-wrapper carve-out as E4112/E4116: a virtually-
        // instantiated component (VIRT view) is never BOM-selected, so the
        // unselected warning is not the normal shape of that view.
        if !matches!(entry.kind, crate::instant::insttab::InstKind::Component)
            || entry.class_name.is_empty()
            || entry.synthetic
            || !entry.unselected
        {
            continue;
        }
        let (pos, uri) = entry_pos(entry);
        results.push(NetCheckResult {
            check: "abstract-unselected",
            severity: "warning",
            message: format!(
                "abstract component instance '{}' is unselected (no partno); \
                 BOM must pick a variant",
                entry.path
            ),
            net_name: entry.path.clone(),
            code: crate::errcodes::ABSTRACT_PART_UNSELECTED,
            pos,
            uri,
        });
    }
}

// ── Floating outputs (output variant of floating input check) ──
pub(crate) fn check_floating_outputs(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let connected: HashSet<u32> = table
        .get_nets()
        .iter()
        .flat_map(|n| n.points.iter().cloned())
        .collect();
    for (_, entry) in table.iter() {
        // Same synthetic-wrapper carve-out as E4112/E4116: a virtually-
        // instantiated interface's io ports render boundary pins and are never
        // wired by definition, so "bidirectional port not connected" is the
        // normal shape of the view, not a defect.
        // §A′-scope: component pads only (kind Pin); an unconnected
        // module-boundary InOut port is owned by C4/E4114, and the A′ bus-slash
        // lane / bare-member alias spellings of that port (kind Label) are
        // folded onto it — reporting the lane and its aggregate twice is the
        // `UART1` + `UART1/TX` + `UART1/RX` phantom triple this guard removes.
        if matches!(entry.io_type, IOType::InOut)
            && matches!(entry.kind, InstKind::Pin)
            && !connected.contains(&entry.id)
            && !is_nc_entry(entry)
            && !entry.synthetic
        {
            let (pos, uri) = entry_pos(entry);
            results.push(NetCheckResult {
                check: "floating-bidirectional",
                severity: "warning",
                message: format!(
                    "Bidirectional port '{}' is not connected to any net.",
                    entry.path
                ),
                net_name: entry.path.clone(),
                code: crate::errcodes::NET_BIDIR_UNCONNECTED,
                pos,
                uri,
            });
        }
    }
}

// ── P10: component pads on no net ──
// The directional checks read `io_type`, so a pad that declares no direction —
// a two-pin passive's terminal — sits outside their object and went unreported
// when a dropped connection left it dangling. This check asks the direction-free
// question instead. Ruling 2026-09-17: every pad, warning for all (no direction
// split, no power-pin downgrade), so a pad may also be reported by a
// directional check above — that overlap is intended, not a duplicate to merge.
pub(crate) fn check_unwired_pins(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let connected: HashSet<u32> = table
        .get_nets()
        .iter()
        .flat_map(|n| n.points.iter().cloned())
        .collect();
    for (_, entry) in table.iter() {
        if matches!(entry.kind, InstKind::Pin)
            && !connected.contains(&entry.id)
            && !is_nc_entry(entry)
            && !entry.synthetic
        {
            let (pos, uri) = entry_pos(entry);
            results.push(NetCheckResult {
                check: "unwired-pin",
                severity: "warning",
                message: format!("Pin '{}' is not connected to any net.", entry.path),
                net_name: entry.path.clone(),
                code: crate::errcodes::NET_PIN_UNWIRED,
                pos,
                uri,
            });
        }
    }
}

// ── Power-intent L1 (design §3 / §13 landing 1): declared relation edges ──
//
// A declared `@bridge`/`@couple`/`@clamp` edge *never* merges L0 copper —
// net-identity already unions real wiring. It only declares a relation between
// two L1 potential classes. These checks are the declaration-local slice of the
// §3.2 role table, emitted as FlatErc rules (PWR-2 loop/@star, PWR-7 clamp
// target). Each verdict is *owning-def local* (an instance's nets are the
// def's nets under the instance path, and relation edges only name same-scope
// nets/refs, iron rule 1 §6), so a def instantiated N times reports once, at
// its own source span.

/// Module instances carrying power-intent declarations, deduped by owning def
/// (`def_uri` + `def.name`). The map is keyed by module entry id; the entry
/// supplies the def's file for span anchoring.
fn power_intent_defs(table: &InstTable) -> Vec<(&McPowerDecls, String)> {
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut out = Vec::new();
    for (id, pi) in table.power_decls() {
        let Some(entry) = table.get_entry(*id) else {
            continue;
        };
        if !matches!(entry.kind, InstKind::Module) {
            continue;
        }
        if !seen.insert((entry.def_uri.clone(), entry.class_name.clone())) {
            continue;
        }
        out.push((pi, entry.def_uri.clone()));
    }
    out
}

/// PWR-2 (design §3.2/§3.4): the DC `@bridge` subgraph must be acyclic. A
/// second/cyclic leg between endpoints already DC-bridged (parallel legs, or a
/// triangle) is a loop — unless a hub conduit on either endpoint carries
/// `@star`, which discharges the intentional loop to the simulation layer.
/// Union-find over declared bridge endpoint nets; an edge whose endpoints
/// already share a root is the redundant path.
pub(crate) fn check_power_bridge_loop(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    for (pi, uri) in power_intent_defs(table) {
        let edges: Vec<crate::semantic::module::pi::L1Edge> = pi
            .l1_edges()
            .into_iter()
            .filter(|e| e.kind == crate::semantic::module::pi::L1EdgeKind::Bridge)
            .collect();
        if edges.is_empty() {
            continue;
        }
        let refs = pi.l1_refs();
        let star: HashSet<&str> = refs
            .iter()
            .filter(|r| r.star)
            .map(|r| r.name.as_str())
            .collect();

        // Distinct endpoint nets → index, then union-find.
        let mut names: Vec<String> = Vec::new();
        let mut idx: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for e in &edges {
            for ep in e.endpoints.iter() {
                if !idx.contains_key(ep) {
                    idx.insert(ep.clone(), names.len());
                    names.push(ep.clone());
                }
            }
        }
        let mut parent: Vec<usize> = (0..names.len()).collect();
        let find = |parent: &mut Vec<usize>, mut x: usize| -> usize {
            while parent[x] != x {
                parent[x] = parent[parent[x]];
                x = parent[x];
            }
            x
        };

        for e in &edges {
            let (Some(a), Some(b)) = (idx.get(&e.endpoints[0]), idx.get(&e.endpoints[1])) else {
                continue;
            };
            let (a, b) = (*a, *b);
            let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
            if ra != rb {
                parent[ra] = rb;
                continue;
            }
            // Redundant DC path. Discharged when either endpoint is a @star hub.
            if star.contains(names[a].as_str()) || star.contains(names[b].as_str()) {
                continue;
            }
            results.push(NetCheckResult {
                check: "power-bridge-loop",
                severity: "error",
                message: crate::errcodes::format_msg(
                    crate::errcodes::POWER_BRIDGE_LOOP,
                    &[&names[a], &names[b]],
                ),
                net_name: names[a].clone(),
                code: crate::errcodes::POWER_BRIDGE_LOOP,
                pos: e.span.start as u32,
                uri: uri.clone(),
            });
        }
    }
}

/// PWR-7 (design §11 / §3.2): `@clamp(ref)` must reference an
/// `@role(protective)`/`@role(earth)` ref — clamping to a main/quiet/isolated
/// ref would dump transient current into the wrong reference. A clamp target
/// that is not a same-scope ref is not adjudicated here (its role is supplied
/// by an ancestor world / port contract, iron rule 1).
pub(crate) fn check_clamp_ref_role(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    for (pi, uri) in power_intent_defs(table) {
        let refs = pi.l1_refs();
        let role_of: std::collections::HashMap<&str, &str> = refs
            .iter()
            .filter_map(|r| r.role.as_deref().map(|role| (r.name.as_str(), role)))
            .collect();
        for e in pi
            .l1_edges()
            .into_iter()
            .filter(|e| e.kind == crate::semantic::module::pi::L1EdgeKind::Clamp)
        {
            let Some(target) = e.endpoints.first() else {
                continue;
            };
            let Some(role) = role_of.get(target.as_str()).copied() else {
                continue;
            };
            if role == attr_keys::WORD_PROTECTIVE || role == attr_keys::WORD_EARTH {
                continue;
            }
            results.push(NetCheckResult {
                check: "clamp-ref-role",
                severity: "error",
                message: crate::errcodes::format_msg(
                    crate::errcodes::CLAMP_REF_NOT_PROTECTIVE,
                    &[&target, &role],
                ),
                net_name: target.clone(),
                code: crate::errcodes::CLAMP_REF_NOT_PROTECTIVE,
                pos: e.span.start as u32,
                uri: uri.clone(),
            });
        }
    }
}

// Power-intent DC rail contract (§4.1 / §13.2): Volt-arg decode
// A domain rail declares the *guarantee* half of a DC contract:
// `rail [hot, ret]::DC(v, tol, capacity, eff)`. Two declaration-local
// verdicts, owning-def local exactly like the relation-edge rules above:
//   * decode (6009): every rail ctor arg decodes to the contract it names
//     (nominal is a signed DC volts, tol is ±%, capacity a current, eff a
//     factor). The Volt-arg self-check — no fake window, no silent pass.
//   * two-roots (6010): a net is the hot member of at most one rail. Writing
//     an intermediate net into a domain rail gives S two handwritten roots and
// every downstream window ERC a fake conflict (§4.1 — an intermediate net never enters a domain).
// The full sink-window E-PWR-001 (`S(net) ⊆ input_req` / sink req window)
// needs the psnk + spec semantic layer (design §13 axis ③) and is not
// adjudicated here — it cannot be golden-verified until that layer lands.
pub(crate) fn check_power_rail_contract(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    for (pi, uri) in power_intent_defs(table) {
        for r in pi.l1_rails() {
            if let Some(bad) = &r.bad {
                results.push(NetCheckResult {
                    check: "rail-contract",
                    severity: "error",
                    message: crate::errcodes::format_msg(
                        crate::errcodes::POWER_RAIL_DECODE,
                        &[&r.hot, &r.ret, bad],
                    ),
                    net_name: r.hot.clone(),
                    code: crate::errcodes::POWER_RAIL_DECODE,
                    pos: r.span.start as u32,
                    uri: uri.clone(),
                });
            }
        }
    }
}

pub(crate) fn check_power_rail_two_roots(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    for (pi, uri) in power_intent_defs(table) {
        let rails = pi.l1_rails();
        let mut first: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for (i, r) in rails.iter().enumerate() {
            if let Some(&j) = first.get(r.hot.as_str()) {
                // A second rail guarantees the same hot net: two handwritten
                // roots on one S. Report once per extra root, at its span.
                let prev = &rails[j];
                results.push(NetCheckResult {
                    check: "rail-two-roots",
                    severity: "error",
                    message: crate::errcodes::format_msg(
                        crate::errcodes::POWER_RAIL_TWO_ROOTS,
                        &[&r.hot, &prev.domain, &r.domain],
                    ),
                    net_name: r.hot.clone(),
                    code: crate::errcodes::POWER_RAIL_TWO_ROOTS,
                    pos: r.span.start as u32,
                    uri: uri.clone(),
                });
            } else {
                first.insert(r.hot.as_str(), i);
            }
        }
    }
}

/// §3.1 domain `@nature(ac|dc)` vs the axis its rail rows name (ac-axis R4):
/// the word and the `::` contract declare the same axis, so writing both makes
/// them agree — a disagreement is a face whose default contradicts a rail
/// inside it. Advisory Info, decl-local on the contradicting rail row (the row
/// an author edits), never a gate: a domain writing no word is the registered
/// default (intent-design.md §5.2), its rail contracts stating the axis alone.
/// Each side is mapped onto an axis (`RailAxis`) instead of comparing
/// spellings, so a word outside `{ac, dc}` and a row whose iface names no axis
/// both pass unjudged rather than mismatching by default.
pub(crate) fn check_rail_nature_consistency(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    for (pi, uri) in power_intent_defs(table) {
        for d in pi.l1_domain_natures() {
            let Some(word) = d.nature.as_deref() else {
                continue;
            };
            let Some(axis) = RailAxis::of_nature_word(word) else {
                continue;
            };
            for r in &d.rails {
                let Some(row_axis) = RailAxis::of_iface(&r.iface) else {
                    continue;
                };
                if row_axis == axis {
                    continue;
                }
                results.push(NetCheckResult {
                    check: "rail-nature-mismatch",
                    severity: "info",
                    message: crate::errcodes::format_msg(
                        crate::errcodes::RAIL_NATURE_MISMATCH,
                        &[&d.name, &word, &r.iface],
                    ),
                    net_name: r.hot.clone(),
                    code: crate::errcodes::RAIL_NATURE_MISMATCH,
                    pos: r.span.start as u32,
                    uri: uri.clone(),
                });
            }
        }
    }
}

/// Shared scan of the flat power face — the per-check duplicate map builds and
/// per-net root loops that 6011 / 6019 / 6021 used to each inline. Owning the
/// workspace Arcs lets `def_of` borrow past the builder, so a check (or the
/// window-layer owner) holds one `PowerScan` for the whole pass and reads the
/// same guarantee / capacity / component maps it used to rebuild by hand.
struct PowerScan {
    /// Rail hot net name → (domain, nominal volts, verbatim text). Same build
    /// as the three checks shared verbatim: two rails claiming one hot with
    /// *different* nominals drop the entry (ambiguous scope → 6010/6013), an
    /// equal-nominal redeclaration keeps the first.
    guarantee: std::collections::HashMap<String, (String, f64, String)>,
    /// Rail hot net name → declared capacity amps (6021's `rail_cap`). Same
    /// ambiguity rule as `guarantee`, over capacity instead of nominal.
    rail_cap: std::collections::HashMap<String, f64>,
    /// Component instance id → def, resolved through the project workspace
    /// class table (module-boundary/library faces stay out — exactly the
    /// membership 6011/6013/6019 relied on).
    comp_def: std::collections::HashMap<u32, std::sync::Arc<McComponent>>,
    /// Module *instance* entry id → its declared power-output (Src/Bi) port
    /// source contracts `(hot member, decoded)` — rail-contract-design.md §8.5
    /// budget face. A module-body `psrc NAME{hot,ret}::DC(v, capacity:…)` row
    /// flattens to a `Port`-kind point whose `parent_id` is this instance id
    /// and whose path tail is `hot`, so the budget axis can recognize the
    /// exported supply face as an explicit capacity root. Budget-local: the
    /// nominal consumers (`source_faces`/`net_nominal`/`has_source_root`) never
    /// read ports — 6011/6019/window skip module-interior sources by design.
    port_src: std::collections::HashMap<u32, Vec<(String, L1PwrPin)>>,
}

impl PowerScan {
    fn build(table: &InstTable) -> PowerScan {
        let mut guarantee: std::collections::HashMap<String, (String, f64, String)> =
            std::collections::HashMap::new();
        let mut rail_cap: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
        for (pi, _uri) in power_intent_defs(table) {
            for r in pi.l1_rails() {
                if let Some(v) = r.v {
                    match guarantee.get(&r.hot) {
                        Some((_, prev, _)) if (*prev - v).abs() > 1e-9 => {
                            guarantee.remove(&r.hot); // ambiguous scope — skip
                        }
                        None => {
                            guarantee
                                .insert(r.hot.clone(), (r.domain.clone(), v, r.v_text.clone()));
                        }
                        _ => {} // same nominal redeclared: keep the first
                    }
                }
                if let Some(c) = r.capacity_amps {
                    match rail_cap.get(&r.hot) {
                        Some(prev) if (*prev - c).abs() > 1e-9 => {
                            rail_cap.remove(&r.hot); // ambiguous scope — skip
                        }
                        None => {
                            rail_cap.insert(r.hot.clone(), c);
                        }
                        _ => {} // same capacity redeclared: keep the first
                    }
                }
            }
        }

        // Component class → def, then component-instance id → def, so every net
        // point recovers its def contract without re-scanning the definition
        // space. (`workspace` is folded into the Arcs owned here, so the
        // borrowed defs stay valid for the whole scan — the same reason the
        // inlined copies kept their `workspace` variable alive.)
        let workspace = crate::definition_space().workspace_components();
        let defs: std::collections::HashMap<String, std::sync::Arc<McComponent>> = workspace
            .into_iter()
            .map(|(sn, c)| (sn.ident.to_string(), c))
            .collect();
        let comp_def: std::collections::HashMap<u32, std::sync::Arc<McComponent>> = table
            .get_components()
            .iter()
            .filter_map(|e| defs.get(&e.class_name).map(|d| (e.id, d.clone())))
            .collect();

        // Module power-output (Src/Bi) port contracts, per module *instance* id
        // (power_decls is keyed by the same instance entry a Port point's
        // `parent_id` carries — intent-design.md §5.2 / §8.5 budget face).
        let port_src: std::collections::HashMap<u32, Vec<(String, L1PwrPin)>> = table
            .power_decls()
            .iter()
            .filter_map(|(id, pi)| {
                let sources = pi.l1_port_sources();
                if sources.is_empty() {
                    None
                } else {
                    Some((*id, sources))
                }
            })
            .collect();

        PowerScan {
            guarantee,
            rail_cap,
            comp_def,
            port_src,
        }
    }

    /// The def behind a flat component instance, if it resolved through the
    /// project class table (None → a non-project face, which the nominal checks
    /// never adjudicate).
    fn def_of(&self, comp_id: u32) -> Option<&McComponent> {
        self.comp_def.get(&comp_id).map(|a| a.as_ref())
    }

    /// Owned def Arc for a flat component instance — lets a caller (e.g. the
    /// recursive window engine in window.rs) hold the def while recursing into
    /// other nets without borrowing the scan.
    fn def_arc(&self, comp_id: u32) -> Option<std::sync::Arc<McComponent>> {
        self.comp_def.get(&comp_id).cloned()
    }

    /// Rail face on a net — exact net name, then the last dotted segment
    /// (module-qualified / power-rail tier-3 spellings). Returns the guarantee
    /// nominal + verbatim text.
    fn rail_face(&self, net: &NetEntry) -> Option<(f64, String)> {
        self.guarantee
            .get(&net.name)
            .or_else(|| {
                net.name
                    .rsplit('.')
                    .next()
                    .and_then(|l| self.guarantee.get(l))
            })
            .map(|(_domain, v, text)| (*v, text.clone()))
    }

    /// Capacity face on a net (rail declared capacity only — the same lookup
    /// shape as `rail_face`).
    fn rail_cap_face(&self, net: &NetEntry) -> Option<f64> {
        self.rail_cap
            .get(&net.name)
            .or_else(|| {
                net.name
                    .rsplit('.')
                    .next()
                    .and_then(|l| self.rail_cap.get(l))
            })
            .copied()
    }

    /// The psrc/psbi hot pins on a net whose parent resolved to a project def
    /// and whose nominal decodes — 6011's `src_nominal`, without the rail.
    fn source_faces(&self, table: &InstTable, net: &NetEntry) -> Vec<(f64, String)> {
        let mut out = Vec::new();
        for &pid in &net.points {
            let Some(entry) = table.get_entry(pid) else {
                continue;
            };
            if !matches!(entry.kind, InstKind::Pin) || !matches!(entry.io_type, IOType::Power) {
                continue;
            }
            let Some(comp_id) = entry.parent_id else {
                continue;
            };
            let Some(def) = self.def_of(comp_id) else {
                continue;
            };
            let Some(contract) = source_contract_for(def, entry) else {
                continue;
            };
            let dec = decode_pwr_pin(contract);
            if let Some(v) = dec.v {
                out.push((v, dec.v_text));
            }
        }
        out
    }

    /// Derived S(net) nominal — 6011's §4.3 derivation, verbatim: the rail face
    /// first (a handwritten §4.1 root); a source pin on the net that disagrees
    /// with the rail defers the net; no rail → one agreeing source nominal,
    /// two disagreeing sources defers. `None` = un-adjudicated (the inlined
    /// `continue`s this method replaces).
    fn net_nominal(&self, table: &InstTable, net: &NetEntry) -> Option<(f64, String)> {
        let rail_root = self.rail_face(net);
        let src_nominal = self.source_faces(table, net);
        match rail_root {
            Some((v, text)) => {
                if src_nominal.iter().any(|(sv, _)| (*sv - v).abs() > 1e-9) {
                    return None; // a source pin on the net disagrees with the rail
                }
                Some((v, text))
            }
            None => {
                let mut it = src_nominal.iter();
                let Some((first, first_text)) = it.next() else {
                    // no supply root on this net — a *reached* upstream root is
                    // net-island §7 L4 reach.rs's job (6011 goes through
                    // ReachScan::nominal_of, which ascends transparent copper /
                    // module boundaries before calling back in here)
                    return None;
                };
                if it.any(|(sv, _)| (*sv - *first).abs() > 1e-9) {
                    return None; // two sources, different nominals — defer
                }
                Some((*first, first_text.clone()))
            }
        }
    }

    /// 6019's root-presence test: a decodable psrc/psbi hot pin sits directly on
    /// the net (agreement with the rail is irrelevant here — existence alone
    /// routes the net to 6013/6010's contention scope, not PWR-1).
    fn has_source_root(&self, table: &InstTable, net: &NetEntry) -> bool {
        for &pid in &net.points {
            let Some(entry) = table.get_entry(pid) else {
                continue;
            };
            if !matches!(entry.kind, InstKind::Pin) || !matches!(entry.io_type, IOType::Power) {
                continue;
            }
            let Some(comp_id) = entry.parent_id else {
                continue;
            };
            let Some(def) = self.def_of(comp_id) else {
                continue;
            };
            let Some(contract) = source_contract_for(def, entry) else {
                continue;
            };
            if decode_pwr_pin(contract).v.is_some() {
                return true;
            }
        }
        false
    }

    /// The declared power-output (Src/Bi) port contract this flat point names,
    /// when the point is a module-power `Port` member (rail-contract-design.md
    /// §8.5 budget face). Budget-local recognition: a point is a budget source
    /// face only when its `parent_id` resolves to a module instance that
    /// declares a source port whose hot member label matches the point's path
    /// tail (the `PortInst.dc_pair` hot member spelling). Sink (`psnk`) port
    /// rows never decode as sources.
    fn port_source_of(&self, entry: &InstEntry) -> Option<L1PwrPin> {
        if !matches!(entry.kind, InstKind::Port) || !matches!(entry.io_type, IOType::Power) {
            return None;
        }
        let module_id = entry.parent_id?;
        let member = entry.path.rsplit('.').next().unwrap_or("");
        let sources = self.port_src.get(&module_id)?;
        sources
            .iter()
            .find(|(hot, _)| hot == member)
            .map(|(_, s)| s.clone())
    }

    /// The capacity roots on a net — rail capacity face, every source pin
    /// carrying a capacity, and every module-power source port face carrying a
    /// capacity — in 6021's encounter order (6021 then requires the roots to
    /// agree before budgeting).
    fn capacity_roots(&self, table: &InstTable, net: &NetEntry) -> Vec<f64> {
        let mut caps: Vec<f64> = Vec::new();
        if let Some(c) = self.rail_cap_face(net) {
            caps.push(c);
        }
        for &pid in &net.points {
            let Some(entry) = table.get_entry(pid) else {
                continue;
            };
            if !matches!(entry.kind, InstKind::Pin) || !matches!(entry.io_type, IOType::Power) {
                continue;
            }
            let Some(comp_id) = entry.parent_id else {
                continue;
            };
            let Some(def) = self.def_of(comp_id) else {
                continue;
            };
            let Some(contract) = source_contract_for(def, entry) else {
                continue;
            };
            let dec = decode_pwr_pin(contract);
            if let Some(c) = dec.capacity_amps {
                caps.push(c);
            }
        }
        // Module-power source port faces (budget root scope §8.5).
        for &pid in &net.points {
            let Some(entry) = table.get_entry(pid) else {
                continue;
            };
            if let Some(s) = self.port_source_of(entry) {
                if let Some(c) = s.capacity_amps {
                    caps.push(c);
                }
            }
        }
        caps
    }
}

/// E-PWR-001 (design §4.4 mandatory-nominal check / §11): a sink (`psnk`) on
/// a net must require that net's derived supply nominal S. The canonical §4.4
/// case: a `::DC(3.3V)`
/// sink sitting on a 5V-supplied net is a wrong hookup (P3/E-PWR-001) — nominal
/// vs nominal is the comparison, no window needed.
///
/// S is derived per net from the net's handwritten supply roots — the design's
/// §4.3 roots: `S(root)` = a handwritten guarantee window (a psrc or a
/// domain-rail block). Both are consumed here, joined to the flat net the
/// root's hot member lands on:
///   * a domain rail whose `hot` resolves to the net name (net name == rail name — identity,
///     not a voltage-from-name heuristic);
///   * a `psrc`/`psbi` source pin whose hot terminal is directly on the net
///     (e.g. ORing `OUT` psrc feeding VMAIN_5V in the golden).
/// If the roots on one net disagree in nominal, the net is left un-adjudicated
/// (source contention is PWR-3 OR-merge territory, deferred) rather than judged
/// against an arbitrary pick. Copper/module-boundary reach (S crossing a fuse /
/// inductor / ferrite / submodule port — net-island §7 L4) is implemented by
/// reach.rs: a root-less sink net *fed* to an upstream root is adjudicated
/// against that root's nominal. Converter re-anchoring is still the later S-set
/// step; for a net with no direct or reached root this rule stays silent.
/// A root whose own nominal failed to decode is reported by the decl-local
/// decode ERC (rail: 6009; pin: 6012), never adjudicated here; likewise a
/// sink whose nominal does not decode is skipped rather than compared blind.
pub(crate) fn check_sink_nominal_mismatch(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    // Shared reach engine over the flat power face — PowerScan (guarantee /
    // capacity / comp_def tables) plus the island role index for §7 L4 copper /
    // module-boundary ascent. `eng.scan` keeps the parent tables reachable to
    // the fire loop below.
    let mut eng = reach::ReachScan::new(table);

    for net in table.get_nets() {
        // ── Derive S(net): a handwritten supply root on the net (design §4.3),
        //    or the upstream root it is fed from through transparent copper / a
        //    module boundary (§7 L4 reach). ──
        // `None` = no agreed root (direct or reached) on this net — intermediate
        // / un-adjudicated (6010/PWR-3 territory), same skip as the inlined S.
        let Some((v_supply, supply_text)) = eng.nominal_of(net.id) else {
            continue;
        };

        // ── mandatory-nominal: every decodable psnk sink on this net must need S. ──
        for &pid in &net.points {
            let Some(entry) = table.get_entry(pid) else {
                continue;
            };
            if !matches!(entry.kind, InstKind::Pin) || !matches!(entry.io_type, IOType::Power) {
                continue;
            }
            let Some(comp_id) = entry.parent_id else {
                continue;
            };
            let Some(def) = eng.scan.def_of(comp_id) else {
                continue;
            };
            let Some(contract) = sink_contract_for(def, entry) else {
                continue;
            };
            let dec = decode_pwr_pin(contract);
            let Some(v_sink) = dec.v else { continue };
            if (v_sink - v_supply).abs() <= 1e-9 {
                continue; // sink nominal == S(net) — the healthy hookup
            }
            // Instance-side sink terminal for the message (`main.s.VDD`, not
            // the positional pin id `main.s.1`).
            let base = entry
                .path
                .rsplit_once('.')
                .map(|(p, _)| p)
                .unwrap_or(&entry.path);
            let sink_path = format!("{base}.{}", contract.hot);
            let (pos, uri) = entry_pos(entry);
            results.push(NetCheckResult {
                check: "sink-nominal-mismatch",
                severity: "error",
                message: crate::errcodes::format_msg(
                    crate::errcodes::POWER_SINK_NOMINAL_MISMATCH,
                    &[&sink_path, &net.name, &dec.v_text, &supply_text],
                ),
                net_name: net.name.clone(),
                code: crate::errcodes::POWER_SINK_NOMINAL_MISMATCH,
                pos,
                uri,
            });
        }
    }
}

/// PWR-1 no-source-face kernel (intent-design.md §11 / §13 landing 3): a
/// flat net that carries component power-sink (`psnk`) terminals yet has no
/// supply root on the net itself — neither a declared domain-rail face
/// (`guarantee`, §4.1) nor a `psrc`/`psbi` hot pin whose nominal decodes (§4.3)
/// — is a face whose loads draw from nothing. This is the complement of 6011's
/// "no agreed root" skip: a root-less net is only a *legal intermediate* when
/// it carries no demand, so a root-less net that does carry a sink is reported
/// unless it is *fed* to an upstream root through transparent copper or a
/// module boundary (§7 L4 reach.rs) — such a net is adjudicated by 6011, not a
/// PWR-1 orphan. The kernel still mirrors 6011/6013 exactly: only parents
/// resolved through the component class table count, so module boundary feed
/// ports (an inlet module's `psnk` port is where an external supply enters the
/// netlist, e.g. a connector feed) never false-fire. Copper pass-through feed
/// (S crossing an inductor/ferrite/fuse from a neighbouring net) is reach.rs's
/// copper arm; converter re-anchoring stays the later S-set step. Reports once
/// per offending net at the first sink terminal's entry.
pub(crate) fn check_undriven_sink_net(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    // Shared reach engine over the flat power face (guarantee/comp_def —
    // 6011's tables, plus the island role index for §7 L4 ascent).
    let mut eng = reach::ReachScan::new(table);

    for net in table.get_nets() {
        // ── Supply root on this net? ──
        // Rail face first (exact name, then last dotted segment), then a
        // psrc/psbi hot pin whose nominal decodes — mirroring 6011's S(net)
        // so a "root-less" net here is exactly the net 6011 defers.
        if eng.scan.rail_face(net).is_some() {
            continue;
        }
        if eng.scan.has_source_root(table, net) {
            continue; // a source drives this net — 6013's contention scope, not PWR-1
        }

        // A net that carries a boundary port of an *instantiated* (non-top)
        // submodule — kind Port whose parent is a Module-kind entry other than
        // the top module — is a declared interface, not a forgotten face:
        // external feeds enter at a module's power-input port (P6) and a
        // sub-domain's source guarantee is adjudicated inside that module's own
        // scope. PWR-1's net-local kernel leaves such boundary nets to the
        // module-local rules, so the Port point exempts the net. The top
        // module's own `io` labels are NOT boundaries — they alias ordinary
        // copper nets and stay in PWR-1's scope.
        let top_id = table
            .iter()
            .find(|(_, e)| matches!(e.kind, InstKind::Module) && e.parent_id.is_none())
            .map(|(id, _)| *id);
        let mut has_submodule_boundary = false;
        for &pid in &net.points {
            let Some(e) = table.get_entry(pid) else {
                continue;
            };
            if !matches!(e.kind, InstKind::Port) {
                continue;
            }
            let is_sub = e.parent_id.is_some_and(|par| {
                par != top_id.unwrap_or(u32::MAX)
                    && table
                        .get_entry(par)
                        .is_some_and(|p| matches!(p.kind, InstKind::Module))
            });
            if is_sub {
                has_submodule_boundary = true;
                break;
            }
        }
        if has_submodule_boundary {
            continue;
        }

        // ── §7 L4 reach-fed: a root-less net *fed* through transparent copper
        //    / a module boundary to an upstream supply root is adjudicated
        //    there by 6011 — not a PWR-1 orphan. The Port exemption above and
        //    the direct-root checks before it still win. ──
        if eng.has_supply_of(net.id) {
            continue;
        }

        // ── Root-less net: report if it still carries a component psnk sink. ──
        for &pid in &net.points {
            let Some(entry) = table.get_entry(pid) else {
                continue;
            };
            if !matches!(entry.kind, InstKind::Pin) || !matches!(entry.io_type, IOType::Power) {
                continue;
            }
            let Some(comp_id) = entry.parent_id else {
                continue;
            };
            let Some(def) = eng.scan.def_of(comp_id) else {
                continue;
            };
            let Some(_contract) = sink_contract_for(def, entry) else {
                continue;
            };
            let (pos, uri) = entry_pos(entry);
            results.push(NetCheckResult {
                check: "undriven-sink-net",
                severity: "error",
                message: crate::errcodes::format_msg(
                    crate::errcodes::SINK_NET_NO_SOURCE,
                    &[&net.name],
                ),
                net_name: net.name.clone(),
                code: crate::errcodes::SINK_NET_NO_SOURCE,
                pos,
                uri,
            });
            break; // one report per offending net
        }
    }
}

/// PWR-3 source-contention kernel (intent-design.md §11 / §13 landing 3):
/// two or more `psrc` hard sources landing their hot terminal on the same net
/// with no declared combine element between them is an undeclared parallel
/// source — a regulator pair wired straight to one node, where a failed or
/// slower source back-feeds the other. The narrow kernel counts only
/// `PwrDir::Src` (`psrc`) contracts, deduped per component instance + hot member
/// (a def that straps several physical pins to one `OUT` is one source, not N);
/// `psbi` (a conditional source — battery coexistence) and rail faces (6010's
/// two-roots scope) are not source points, and copper pass-through propagation /
/// converter re-anchoring stay the later S-set step. Nominal *agreement* does
/// not excuse the parallel — ORing is a topological merge, so even two 5V
/// regulators wire-ORed onto one net still need the declared element.
pub(crate) fn check_power_source_contention(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let workspace = crate::definition_space().workspace_components();
    let defs: std::collections::HashMap<String, &McComponent> = workspace
        .iter()
        .map(|(sn, c)| (sn.ident.to_string(), c.as_ref()))
        .collect();
    let comp_def: std::collections::HashMap<u32, &McComponent> = table
        .get_components()
        .iter()
        .filter_map(|e| defs.get(&e.class_name).map(|d| (e.id, *d)))
        .collect();

    for net in table.get_nets() {
        // Distinct (component instance, hot member) psrc sources driving this
        // net directly, in first-encounter order; the instance-side terminal
        // path (`main.a.OUT`) is kept for the message.
        let mut seen: std::collections::HashSet<(u32, String)> = std::collections::HashSet::new();
        let mut paths: Vec<String> = Vec::new();
        let mut pos: Option<(u32, String)> = None;
        for &pid in &net.points {
            let Some(entry) = table.get_entry(pid) else {
                continue;
            };
            if !matches!(entry.kind, InstKind::Pin) || !matches!(entry.io_type, IOType::Power) {
                continue;
            }
            let Some(comp_id) = entry.parent_id else {
                continue;
            };
            let Some(def) = comp_def.get(&comp_id).copied() else {
                continue;
            };
            let Some(contract) = source_contract_for(def, entry) else {
                continue;
            };
            if contract.dir != PwrDir::Src {
                continue; // psbi is a conditional source, not a hard psrc
            }
            if !seen.insert((comp_id, contract.hot.clone())) {
                continue; // a second physical pin strapped to the same OUT
            }
            let base = entry
                .path
                .rsplit_once('.')
                .map(|(p, _)| p)
                .unwrap_or(&entry.path);
            paths.push(format!("{base}.{}", contract.hot));
            if pos.is_none() {
                pos = Some(entry_pos(entry));
            }
        }
        if paths.len() < 2 {
            continue;
        }
        let (p, uri) = pos.unwrap_or((0, String::new()));
        results.push(NetCheckResult {
            check: "power-source-contention",
            severity: "error",
            message: crate::errcodes::format_msg(
                crate::errcodes::POWER_SOURCE_CONTENTION,
                &[&net.name, &paths.len().to_string(), &paths.join(", ")],
            ),
            net_name: net.name.clone(),
            code: crate::errcodes::POWER_SOURCE_CONTENTION,
            pos: p,
            uri,
        });
    }
}

/// §3.2 role-relation contract, isolated row: a declared DC `@bridge` must not
/// join an `@role(isolated)` member to a non-isolated net. The isolated world
/// is derived (design §4): its member net names are the isolated refs plus
/// every rail whose return member is an isolated ref. Only an explicit Y-cap
/// `@couple` may cross the boundary — a `@bridge` makes the "isolated"
/// secondary side a hard connection (PWR-9 conduit half). Per-module: a child
/// never names an ancestor's conduit (iron rule 1), so each module's own
/// isolated refs + rails + edges are the whole adjudication surface here.
pub(crate) fn check_isolated_dc_bridge(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    for (pi, uri) in power_intent_defs(table) {
        // Owned names: l1_refs()/l1_rails()/l1_edges() decode fresh temporaries
        // per call, so &str views into them cannot outlive the loop body.
        let isolated_refs: HashSet<String> = pi
            .l1_refs()
            .iter()
            .filter(|r| r.role.as_deref() == Some(attr_keys::WORD_ISOLATED))
            .map(|r| r.name.clone())
            .collect();
        if isolated_refs.is_empty() {
            continue;
        }
        let mut isolated: HashSet<String> = isolated_refs.clone();
        for r in pi.l1_rails() {
            if isolated_refs.contains(&r.ret) {
                isolated.insert(r.hot.clone());
            }
        }
        for e in pi
            .l1_edges()
            .into_iter()
            .filter(|e| e.kind == crate::semantic::module::pi::L1EdgeKind::Bridge)
        {
            let (Some(a), Some(b)) = (e.endpoints.first(), e.endpoints.get(1)) else {
                continue;
            };
            let (ia, ib) = (isolated.contains(a), isolated.contains(b));
            if ia == ib {
                continue; // neither isolated, or isolated↔isolated (two zero-DC worlds merge — kernel-accepted)
            }
            let (member, far) = if ia { (a, b) } else { (b, a) };
            results.push(NetCheckResult {
                check: "isolated-dc-bridge",
                severity: "error",
                message: crate::errcodes::format_msg(
                    crate::errcodes::ISOLATED_DC_BRIDGE,
                    &[member, far],
                ),
                net_name: member.clone(),
                code: crate::errcodes::ISOLATED_DC_BRIDGE,
                pos: e.span.start as u32,
                uri: uri.clone(),
            });
        }
    }
}

/// §3.2 role-relation contract, protective row (PWR-8): an `@role(protective)`
/// conduit is allowed exactly one declared DC `@bridge` — its single point to
/// the circuit main reference. A second incident bridge (a parallel
/// protective-ground leg, or a tie to a second island) is a second single
/// point: a ground loop under ESD. Unlike a quiet-leg loop (PWR-2), `@star`
/// does *not* discharge this — the single point is a hard (1,0) invariant, so
/// this rule fires even when the loop check stays silent.
pub(crate) fn check_protective_multi_bridge(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    for (pi, uri) in power_intent_defs(table) {
        // Owned names (see check_isolated_dc_bridge for the lifetime rationale).
        let protective: HashSet<String> = pi
            .l1_refs()
            .iter()
            .filter(|r| r.role.as_deref() == Some(attr_keys::WORD_PROTECTIVE))
            .map(|r| r.name.clone())
            .collect();
        if protective.is_empty() {
            continue;
        }
        // protective name → spans of every incident DC bridge (in edge order).
        let mut inc: std::collections::HashMap<String, Vec<u32>> = std::collections::HashMap::new();
        for e in pi
            .l1_edges()
            .into_iter()
            .filter(|e| e.kind == crate::semantic::module::pi::L1EdgeKind::Bridge)
        {
            for ep in e.endpoints.iter() {
                if protective.contains(ep) {
                    inc.entry(ep.clone()).or_default().push(e.span.start as u32);
                }
            }
        }
        // Emits one row per counted name, in `inc` order - so the order has to
        // come from the input, not from the process (build-design §3.7
        // discipline 4). A `HashMap` draws its iteration order fresh per run;
        // sorting by the name makes it the input's.
        let mut counted: Vec<(String, Vec<u32>)> = inc.into_iter().collect();
        counted.sort_by(|a, b| a.0.cmp(&b.0));
        for (name, spans) in counted {
            if spans.len() < 2 {
                continue;
            }
            results.push(NetCheckResult {
                check: "protective-multi-bridge",
                severity: "error",
                message: crate::errcodes::format_msg(
                    crate::errcodes::PROTECTIVE_MULTI_BRIDGE,
                    &[&name, &spans.len().to_string()],
                ),
                net_name: name.to_string(),
                code: crate::errcodes::PROTECTIVE_MULTI_BRIDGE,
                pos: spans[1],
                uri: uri.clone(),
            });
        }
    }
}

/// §3.2 role-relation contract, earth row: an `@role(earth)` conduit couples to
/// protective/main only through a Y-cap `@couple` (AC-only) — any declared DC
/// `@bridge` incident to it is a low-resistance chassis direct tie and a leakage
/// warning (intent-design.md §3.2 earth row / §11 chassis/earth scene).
/// Unlike the isolated row, no world derivation is needed: earth refs are
/// themselves the full incident surface. `@clamp` into an earth ref stays legal
/// (PWR-7 targets protective/earth), so only `@bridge` rows leak. Severity is a
/// warning, not an error, per the design's "leakage warning" wording — a
/// single-point chassis tie is surfaced, not hard-failed (the protective row
/// 6015 already errors on the circuit side when such a tie doubles).
pub(crate) fn check_earth_dc_leak(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    for (pi, uri) in power_intent_defs(table) {
        // Owned names (see check_isolated_dc_bridge for the lifetime rationale).
        let earth: HashSet<String> = pi
            .l1_refs()
            .iter()
            .filter(|r| r.role.as_deref() == Some(attr_keys::WORD_EARTH))
            .map(|r| r.name.clone())
            .collect();
        if earth.is_empty() {
            continue;
        }
        for e in pi
            .l1_edges()
            .into_iter()
            .filter(|e| e.kind == crate::semantic::module::pi::L1EdgeKind::Bridge)
        {
            let (Some(a), Some(b)) = (e.endpoints.first(), e.endpoints.get(1)) else {
                continue;
            };
            let (ia, ib) = (earth.contains(a), earth.contains(b));
            if !ia && !ib {
                continue;
            }
            let (member, far) = if ia { (a, b) } else { (b, a) };
            results.push(NetCheckResult {
                check: "earth-dc-leak",
                severity: "warning",
                message: crate::errcodes::format_msg(
                    crate::errcodes::EARTH_DC_LEAK,
                    &[member, far],
                ),
                net_name: member.clone(),
                code: crate::errcodes::EARTH_DC_LEAK,
                pos: e.span.start as u32,
                uri: uri.clone(),
            });
        }
    }
}

/// §3.2.1 island-root contract (main row): every DC-bridged reference island
/// carries exactly one `@role(main)` root. A reference island is a connected
/// component of DC `@bridge` edges whose two endpoints are *both* role-bearing
/// reference identities — supply-side legs (`@bridge(VDD_3V3, VDDA)` ties rail
/// hots, not identities) and a bound child leg (`@bridge(ESDGND, vin.GND)` names
/// a member the child owns no role for; its root is supplied by the parent
/// binding) stay out of the graph. Zero mains → the joined identities (two
/// quiets, or a quiet tied to a protective) have no island ground to return to;
/// two or more mains → two power worlds were DC-joined by a `@bridge` — the
/// doc's canonical "two main islands must not be @bridge'd" case (§3.2.1).
/// Isolated/earth worlds declare no DC bridge, so they never enter an island.
pub(crate) fn check_reference_island_root(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    for (pi, uri) in power_intent_defs(table) {
        let refs = pi.l1_refs();
        // Owned names for the node set; the role map borrows `refs` (a named
        // binding, alive for the whole body — see check_isolated_dc_bridge).
        let role: std::collections::HashMap<&str, &str> = refs
            .iter()
            .filter_map(|r| r.role.as_deref().map(|ro| (r.name.as_str(), ro)))
            .collect();
        let bridges: Vec<crate::semantic::module::pi::L1Edge> = pi
            .l1_edges()
            .into_iter()
            .filter(|e| e.kind == crate::semantic::module::pi::L1EdgeKind::Bridge)
            .filter(|e| {
                e.endpoints.len() >= 2
                    && role.contains_key(e.endpoints[0].as_str())
                    && role.contains_key(e.endpoints[1].as_str())
            })
            .collect();
        if bridges.is_empty() {
            continue;
        }
        let mut names: Vec<String> = Vec::new();
        let mut idx: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        // (endpoint a, endpoint b, source span) per ref-ref bridge, in order.
        let mut edge_pairs: Vec<(usize, usize, u32)> = Vec::new();
        for e in &bridges {
            for ep in e.endpoints.iter().take(2) {
                if !idx.contains_key(ep) {
                    idx.insert(ep.clone(), names.len());
                    names.push(ep.clone());
                }
            }
            edge_pairs.push((
                idx[&e.endpoints[0]],
                idx[&e.endpoints[1]],
                e.span.start as u32,
            ));
        }
        let mut parent: Vec<usize> = (0..names.len()).collect();
        let find = |parent: &mut Vec<usize>, mut x: usize| -> usize {
            while parent[x] != x {
                parent[x] = parent[parent[x]];
                x = parent[x];
            }
            x
        };
        for &(a, b, _) in &edge_pairs {
            let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
            if ra != rb {
                parent[ra] = rb;
            }
        }
        // Component root → member indices (source order).
        let mut comp: std::collections::HashMap<usize, Vec<usize>> =
            std::collections::HashMap::new();
        for (i, _) in names.iter().enumerate() {
            comp.entry(find(&mut parent, i)).or_default().push(i);
        }
        // Per component: main count + first ref-ref bridge span (witness pos).
        let mut main_count: std::collections::HashMap<usize, usize> =
            std::collections::HashMap::new();
        let mut witness: std::collections::HashMap<usize, u32> = std::collections::HashMap::new();
        for (i, n) in names.iter().enumerate() {
            if role.get(n.as_str()).copied() == Some("main") {
                *main_count.entry(find(&mut parent, i)).or_default() += 1;
            }
        }
        for &(a, b, span) in &edge_pairs {
            let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
            if ra == rb {
                witness.entry(ra).or_insert(span);
            }
        }
        // One row per component, emitted in this order - so the order has to be
        // the input's, not a `HashMap`'s per-process draw (build-design §3.7
        // discipline 4). `members` is built in ascending index order, so its
        // first element orders the components by source position.
        let mut comps: Vec<(usize, Vec<usize>)> = comp.into_iter().collect();
        comps.sort_by_key(|(_, members)| members[0]);
        for (root, members) in &comps {
            if members.len() < 2 {
                continue; // lone identity with no ref-ref DC leg is not an island
            }
            let mains = main_count.get(root).copied().unwrap_or(0);
            if mains == 1 {
                continue;
            }
            let rep = &names[members[0]];
            results.push(NetCheckResult {
                check: "reference-island-root",
                severity: "error",
                message: crate::errcodes::format_msg(
                    crate::errcodes::REFERENCE_ISLAND_ROOT,
                    &[rep, &mains.to_string()],
                ),
                net_name: rep.clone(),
                code: crate::errcodes::REFERENCE_ISLAND_ROOT,
                pos: witness.get(root).copied().unwrap_or(0),
                uri: uri.clone(),
            });
        }
    }
}

/// conduit-equivalence-design.md §8.4: `@bridge` is an explicit declaration —
/// the design never infers a bridge from component types (a ferrite without a
/// `@bridge` is an ordinary part). But a *forgotten* declaration must not be
/// silent: a conduit declaring `@role(quiet)` or `@role(protective)` expects
/// exactly one declared DC `@bridge` to its island main, and ERC counts it to
/// zero — no incident DC `@bridge` at all means the role-bearing conduit was
/// never wired (a quiet face with no return leg, or a protective single point
/// never declared). Supply-side and bound-member legs are still edges in the
/// same module and count; the upper bound (a second bridge) is discharged by
/// 6007 (quiet leg loop, `@star`-exempt) and 6015 (protective single point,
/// hard) — this rule fires only on the bare zero.
pub(crate) fn check_role_ref_missing_bridge(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    for (pi, uri) in power_intent_defs(table) {
        // Owned names (see check_isolated_dc_bridge for the lifetime rationale).
        let expect: HashSet<String> = pi
            .l1_refs()
            .iter()
            .filter(|r| {
                r.role.as_deref() == Some(attr_keys::WORD_QUIET)
                    || r.role.as_deref() == Some(attr_keys::WORD_PROTECTIVE)
            })
            .map(|r| r.name.clone())
            .collect();
        if expect.is_empty() {
            continue;
        }
        // The refs that actually carry a declared DC bridge in this module.
        let mut bridged: HashSet<String> = HashSet::new();
        for e in pi
            .l1_edges()
            .into_iter()
            .filter(|e| e.kind == crate::semantic::module::pi::L1EdgeKind::Bridge)
        {
            for ep in e.endpoints.iter() {
                if expect.contains(ep) {
                    bridged.insert(ep.clone());
                }
            }
        }
        for r in pi.l1_refs() {
            if !expect.contains(&r.name) || bridged.contains(&r.name) {
                continue;
            }
            results.push(NetCheckResult {
                check: "role-ref-missing-bridge",
                severity: "error",
                message: crate::errcodes::format_msg(
                    crate::errcodes::ROLE_REF_MISSING_BRIDGE,
                    &[&r.name],
                ),
                net_name: r.name.clone(),
                code: crate::errcodes::ROLE_REF_MISSING_BRIDGE,
                pos: r.span.start as u32,
                uri: uri.clone(),
            });
        }
    }
}

/// Pin-contract Volt-arg decode (the pin-side of [`POWER_RAIL_DECODE`] 6009;
/// intent-design.md §5.2 closed word-list discipline): every `psrc`/`psnk`/`psbi`
/// `::DC(…)` ctor arg must decode to the contract it names. `decode_pwr_pin`
/// keeps the first failure as `L1PwrPin::bad` — a non-DC nominal (e.g.
/// `::DC(5A)` on a sink), a source-exclusive budget key (`tol`/`capacity`/`eff`)
/// on a sink, a `spec`-belonging window key (`req`/`abs`) on the pin, or a
/// missing mandatory nominal. This rule reports it decl-locally — once per used
/// component class's power contract, at its own source span — rather than the
/// hookup layer silently skipping an undecodable sink.
pub(crate) fn check_pin_contract_decode(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let workspace = crate::definition_space().workspace_components();
    let defs: std::collections::HashMap<String, &McComponent> = workspace
        .iter()
        .map(|(sn, c)| (sn.ident.to_string(), c.as_ref()))
        .collect();
    // One report per used component class's offending contract (a def
    // instantiated N times is checked once, at its own decl — like 6009).
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for entry in table.get_components() {
        if !seen.insert(entry.class_name.clone()) {
            continue;
        }
        let Some(def) = defs.get(&entry.class_name).copied() else {
            continue;
        };
        for contract in &def.pins.pwr {
            let dec = decode_pwr_pin(contract);
            let Some(bad) = dec.bad else {
                continue;
            };
            results.push(NetCheckResult {
                check: "pin-contract-decode",
                severity: "error",
                message: crate::errcodes::format_msg(
                    crate::errcodes::POWER_PIN_DECODE,
                    &[&entry.class_name, &dec.hot, &bad],
                ),
                net_name: dec.hot.clone(),
                code: crate::errcodes::POWER_PIN_DECODE,
                pos: dec.span.start as u32,
                uri: entry.def_uri.clone(),
            });
        }
    }
}

/// Model A §4.1 `[hot, ret]` pairing: every `psrc`/`psnk`/`psbi` `::DC(…)` row
/// declares a DC crossing, and a crossing *is* the pair — the hot terminal plus
/// the return it closes over (`psnk [1,2] = VIN{Vin, GND}::DC(5V)`). `pins.pwr`
/// holds exactly the `::DC`-carrying rows (`read_pwr_declare` returns early for
/// any other iface), so `ret == None` here is precisely "a `::DC` row that names
/// no second member". That declaration is incomplete rather than quieter: 6022
/// reads the member for the return leg, 6027 for the return span, and the 6021
/// budget kernel needs both ends of the crossing. Reported decl-locally once per
/// used component class, like 6012. A row with no `::DC` never forms a power pin
/// at all — that is the passive leaf (`N = GND`), whose direction belongs to the
/// parent module port, and this rule deliberately does not touch it.
pub(crate) fn check_pin_contract_return_member(
    table: &InstTable,
    results: &mut Vec<NetCheckResult>,
) {
    let workspace = crate::definition_space().workspace_components();
    let defs: std::collections::HashMap<String, &McComponent> = workspace
        .iter()
        .map(|(sn, c)| (sn.ident.to_string(), c.as_ref()))
        .collect();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for entry in table.get_components() {
        if !seen.insert(entry.class_name.clone()) {
            continue;
        }
        let Some(def) = defs.get(&entry.class_name).copied() else {
            continue;
        };
        for contract in def.pins.pwr.iter().filter(|c| c.ret.is_none()) {
            results.push(NetCheckResult {
                check: "pin-contract-return-missing",
                severity: "error",
                message: crate::errcodes::format_msg(
                    crate::errcodes::POWER_PIN_RETURN_MISSING,
                    &[&contract.dir.as_str(), &contract.hot],
                ),
                net_name: contract.hot.clone(),
                code: crate::errcodes::POWER_PIN_RETURN_MISSING,
                pos: contract.span.start as u32,
                uri: entry.def_uri.clone(),
            });
        }
    }
}

/// §6.2③ combine-output re-anchor (rail-contract-design.md §6.1/§6.2): a
/// combine element — a component def with ≥2 input-direction (`psnk`, or a
/// `psbi` charge half) power rows and ≥1 `psrc` output row — is a pass-through
/// OR-merge, not a regulator. `spec.output`, the sole combine-vs-converter
/// discriminator (§6.1), is still parse-dormant, so the structural shape is the
/// combine test today (§6.5): no single-input→output pass element carries a
/// `psrc` output row, and a ≥2-input part that re-anchors a window is a
/// dual-input converter (deferred, §6.5). Such an output `psrc` writes only the
/// merged nominal `::DC(v)`; a ±tol window would claim the merged net's supply
/// holds tighter than any single active input — the exact over-claim the
/// OR-merge ∪ semantics (§6.3) exists to catch under single-source states.
/// Decl-locally, once per used class, mirroring 6012.
pub(crate) fn check_combine_output_tol(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let workspace = crate::definition_space().workspace_components();
    let defs: std::collections::HashMap<String, &McComponent> = workspace
        .iter()
        .map(|(sn, c)| (sn.ident.to_string(), c.as_ref()))
        .collect();
    // One report per used combine class's offending output contract (a def
    // instantiated N times is checked once, at its own decl — like 6012).
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for entry in table.get_components() {
        if !seen.insert(entry.class_name.clone()) {
            continue;
        }
        let Some(def) = defs.get(&entry.class_name).copied() else {
            continue;
        };
        // Combine shape (§6.1): ≥2 input-direction rows (psnk, or a psbi
        // charge half) plus ≥1 psrc output row.
        let inputs = def
            .pins
            .pwr
            .iter()
            .filter(|c| matches!(c.dir, PwrDir::Snk | PwrDir::Bi))
            .count();
        if inputs < 2 {
            continue;
        }
        for contract in def.pins.pwr.iter().filter(|c| c.dir == PwrDir::Src) {
            let dec = decode_pwr_pin(contract);
            let Some(_tol) = dec.tol else {
                continue;
            };
            // Verbatim tol text (`±1%`, `5%`) for the message, off the param.
            let tol_text = contract
                .params
                .iter()
                .find(|p| p.key.as_deref() == Some("tol"))
                .map(|p| p.text.as_str())
                .unwrap_or_default();
            results.push(NetCheckResult {
                check: "combine-output-tol",
                severity: "error",
                message: crate::errcodes::format_msg(
                    crate::errcodes::COMBINE_OUTPUT_TOL,
                    &[&entry.class_name, &dec.hot, &tol_text],
                ),
                net_name: dec.hot.clone(),
                code: crate::errcodes::COMBINE_OUTPUT_TOL,
                pos: dec.span.start as u32,
                uri: entry.def_uri.clone(),
            });
        }
    }
}

/// PWR-4 budget, supply-root scope (rail-contract-design.md §8.5) — the owner
/// moved to the sibling leaf budget.rs (root resolution + aggregation). See
/// `budget::check_net_budget`; rules.rs keeps importing `check_net_budget` from
/// this module via the `pub(crate) use budget::check_net_budget` re-export above.

/// One resolvable "potential class" for a leg pad (conduit-equivalence §8.5):
/// the identity a flat net's pad belongs to, resolved owner-locally against the
/// owning module's own declarations. island attribution semantics are shared and
/// untouched — identity is recovered *here*, never by re-deriving scope.
#[derive(Debug, Clone)]
struct EffClass {
    /// Class display id — the `conduit` copper name, or the rail member net name.
    id: String,
    /// Domain names whose declared DC rail anchors this class (empty for a pure
    /// conduit reference such as ESDGND/EARTH). The audit's disjointness test.
    worlds: Vec<String>,
}

/// Resolve a flat net's potential class. Order:
/// 1. owning-scope `conduit` copper (net name == conduit bare name);
/// 2. a rail hot/ret *member net name* — the rail declaration is the class even
///    without a same-name conduit (attribution keeps role Hot/Ret, copper None);
/// 3. else an A′ boundary walk: follow the net's junction points outward to
///    co-resident segments owned by an *ancestor* scope and resolve those
///    (visited guards the symmetric in-progress cycle; multi-hop reaches the
///    nearest copper anchor — C_y's `shield_to_earth` member → the parent
///    `EARTH` copper). A net that reaches no class (Signal / derived supply
///    face — an LX switch node, a bare legacy ground) is unjudged: never guess
///    past a declaration anchor (net-island-attribution §8).
fn eff_class(
    table: &InstTable,
    idx: &crate::instant::island::NetIslandIndex,
    attr: &crate::instant::island::NetAttribution,
    visiting: &mut Vec<u32>,
) -> Option<EffClass> {
    if let Some(cu) = attr.copper.as_deref() {
        return Some(EffClass {
            id: cu.to_string(),
            worlds: attr.worlds.clone(),
        });
    }
    if matches!(
        attr.role,
        crate::instant::island::NetRole::Hot | crate::instant::island::NetRole::Ret
    ) {
        return Some(EffClass {
            id: attr.name.clone(),
            worlds: attr.worlds.clone(),
        });
    }
    // Unresolvable in the owning scope — only a boundary junction can rescue it.
    if visiting.contains(&attr.net_id) {
        return None;
    }
    visiting.push(attr.net_id);
    let Some(own) = attr.module else {
        return None;
    };
    let Some(net) = table.get_net(attr.net_id) else {
        return None;
    };
    for &pid in &net.points {
        for &cid in table.nets_of(pid) {
            if cid == attr.net_id {
                continue;
            }
            let Some(co) = idx.get(cid) else {
                continue;
            };
            if co.module.is_none() || co.module == Some(own) {
                continue; // same-scope co-segment — not an outward boundary step
            }
            if let Some(cls) = eff_class(table, idx, co, visiting) {
                return Some(cls);
            }
        }
    }
    None
}

/// A declared `@bridge`/`@couple` DC edge, keyed by owning module. `a`/`b` are
/// the sorted endpoint net names as written in the clause; `lo..hi` is the
/// clause byte span in the module's def file, used for the per-leg carrier match.
/// (A Clamp is a net→ref transient dump — no DC tie.)
#[derive(Debug, Clone)]
pub(super) struct DeclEdge {
    /// Which relation the clause declares — the consumers' own filter (6022
    /// judges both DC tie kinds, PI-2's filter leg is the bridge alone).
    kind: crate::semantic::module::pi::L1EdgeKind,
    a: String,
    b: String,
    /// The same two endpoints resolved in the owning module's def space
    /// ([`endpoint_identity`]), sorted by identity. 6022 pairs a leg's pads
    /// against these — a bare `GND` a module declares as a DC port member denotes
    /// `DC:GND`, the identity its pad net carries, and a name the module declares
    /// nothing under degrades to the written spelling. PI-2 / SN-2 resolve the
    /// written `a`/`b` against their own scope tables and read those.
    ident_a: String,
    ident_b: String,
    lo: usize,
    hi: usize,
}

/// Every declared `@bridge`/`@couple` edge per owning module — the one read
/// shared by the two rules that judge a declared relation between two named
/// nets: 6022 (a physical two-terminal leg must carry its relation on its own
/// statement) and PI-2 (a filter leg's load side owes a decoupling element). A
/// non-module owner declares no net relation; a clause with fewer than two
/// endpoints carries no pair to key on; the pair is sorted so the two spellings
/// of one relation are one edge.
///
/// Each endpoint is also resolved in the owning module's own def space
/// ([`endpoint_identity`]): a bare `GND` the module declares as a DC port member
/// denotes that member, so the pair also carries the identity `DC:GND` its pad
/// net carries. PI-2 / SN-2 keep the written spelling; 6022 pairs on the
/// identity ([`pair_matches`]).
///
/// Keyed by module id in a `BTreeMap`, not a `HashMap`. Three of the four
/// consumers — `bridge::check_bridge_load_decoupling` (PI-2),
/// `shared_return::check_shared_return_bridge` (SN-2) and
/// `subface::check_filter_subface_overreach` (PI-4) — flatten this map into
/// their candidate list and then emit one row per candidate, so its iteration
/// order reaches the report's row order and has to be the input's, not the
/// process's (build-design §3.7 discipline 4). The fourth,
/// [`check_return_leg_undeclared`], only predicates on the collected hits and
/// is order-insensitive either way.
pub(super) fn declared_dc_edges(
    table: &InstTable,
) -> std::collections::BTreeMap<u32, Vec<DeclEdge>> {
    let mut out: std::collections::BTreeMap<u32, Vec<DeclEdge>> = std::collections::BTreeMap::new();
    for (id, pi) in table.power_decls() {
        let is_module = table
            .get_entry(*id)
            .is_some_and(|e| matches!(e.kind, InstKind::Module));
        if !is_module {
            continue;
        }
        let edges: Vec<DeclEdge> = pi
            .l1_edges()
            .into_iter()
            .filter(|e| {
                matches!(
                    e.kind,
                    crate::semantic::module::pi::L1EdgeKind::Bridge
                        | crate::semantic::module::pi::L1EdgeKind::Couple
                )
            })
            .filter_map(|e| {
                let (Some(a), Some(b)) = (e.endpoints.first(), e.endpoints.get(1)) else {
                    return None;
                };
                if a == b {
                    return None;
                }
                let (ia, ib) = (endpoint_identity(pi, a), endpoint_identity(pi, b));
                let (a, b) = if a < b {
                    (a.clone(), b.clone())
                } else {
                    (b.clone(), a.clone())
                };
                let (ident_a, ident_b) = if ia < ib { (ia, ib) } else { (ib, ia) };
                Some(DeclEdge {
                    kind: e.kind,
                    a,
                    b,
                    ident_a,
                    ident_b,
                    lo: e.span.start,
                    hi: e.span.end,
                })
            })
            .collect();
        if !edges.is_empty() {
            out.insert(*id, edges);
        }
    }
    out
}

/// One written `@bridge`/`@couple` endpoint resolved in the owning module's own
/// def space: a name that module declares denotes the conductor the declaration
/// names it as, so the identity ([`pwrid::DeclaredMember::identity`]) is what the
/// edge keys on. A name the module declares nothing under stays the written
/// spelling — it is then an undeclared wire name, judged as such.
fn endpoint_identity(pi: &McPowerDecls, name: &str) -> String {
    pwrid::member_of_module(pi, name)
        .map(|m| m.identity())
        .unwrap_or_else(|| name.to_string())
}

/// Every key a flat net answers to in 6022's pair match: the declared identities
/// its points carry, plus the net's own name, which is what a copper the module
/// declares nothing under answers by. Only module-level port, label and bus
/// entries contribute an identity — their `pwr_member` is written from the
/// module's own declarations alone, while a component pin's belongs to that
/// component's def. A module-scope bus counts because a conductor can be spelled
/// as a numbered bus, whose declaration sits on the bus and not on the member
/// labels hanging off it.
fn net_identities(table: &InstTable, module: u32, net: &NetEntry) -> Vec<String> {
    let mut out: Vec<String> = net
        .points
        .iter()
        .filter_map(|p| {
            let e = table.get_entry(*p)?;
            if e.parent_id != Some(module)
                || !matches!(e.kind, InstKind::Port | InstKind::Label | InstKind::Bus)
            {
                return None;
            }
            e.rail_identity()
        })
        .collect();
    out.push(net.name.clone());
    out.sort();
    out.dedup();
    out
}

/// Do two pad key sets name the clause's two endpoints? The clause carries the
/// pair as the identities the declaring module resolves its own names to
/// ([`endpoint_identity`] degrades to the written spelling there for a name the
/// module declares nothing under), and a pad answers with the identities its net
/// carries plus the net's own name. A pair is a set, so the two members are
/// compared unordered.
fn pair_matches(d: &DeclEdge, ia: &[String], ib: &[String]) -> bool {
    ia.iter().any(|a| {
        ib.iter()
            .any(|b| (a == &d.ident_a && b == &d.ident_b) || (a == &d.ident_b && b == &d.ident_a))
    })
}

/// Net names as **the scope that wrote them** reads them: a `@bridge`/`@couple`
/// clause names the nets of its own module, so a name is only ever looked up
/// there (the rule PI-3's rail side keeps too). Built once per rule that reads a
/// declared edge.
pub(crate) fn scope_nets(
    table: &InstTable,
) -> std::collections::HashMap<u32, std::collections::HashMap<String, Vec<u32>>> {
    let mut out: std::collections::HashMap<u32, std::collections::HashMap<String, Vec<u32>>> =
        std::collections::HashMap::new();
    for net in table.get_nets() {
        let Some(module) = net.module else {
            continue;
        };
        out.entry(module)
            .or_default()
            .entry(net.name.clone())
            .or_default()
            .push(net.id);
    }
    out
}

/// One declared-edge endpoint as the declaring scope reads it: the net the name
/// was written for, that net's effective class, and the scope owning it (`None`
/// when the name matches no net there, the net reaches no class, or it belongs
/// to no scope — an unresolvable endpoint takes no verdict, since its identity
/// would come from an ancestor's world).
///
/// PI-2 (a filter leg's load side) and PI-4 (what that load side feeds) both
/// locate their leg through this one read, so which end is which side cannot
/// drift between the two rules.
type EdgeEndpoint = (u32, EffClass, u32);

fn edge_endpoint(
    table: &InstTable,
    idx: &crate::instant::island::NetIslandIndex,
    scope_nets: &std::collections::HashMap<u32, std::collections::HashMap<String, Vec<u32>>>,
    scope: u32,
    name: &str,
) -> Option<EdgeEndpoint> {
    for &net in scope_nets.get(&scope)?.get(name)? {
        let Some(attr) = idx.get(net) else {
            continue;
        };
        let Some(layer) = attr.module else {
            continue;
        };
        let Some(cls) = eff_class(table, idx, attr, &mut Vec::new()) else {
            continue;
        };
        return Some((net, cls, layer));
    }
    None
}

/// Wiring-site positions of a leg's pads and carrier, for the per-leg span
/// match: each pad's `src_pos` (wiring site) first, then its `fallback_pos`
/// (declaration site), then the component's own `src_pos`. The carrier of a
/// clause-declared leg sits inside that clause's span in the module def file.
fn leg_sites(comp: &InstEntry, pins: &[&InstEntry]) -> Vec<(u32, String)> {
    let mut sites = Vec::new();
    for p in pins {
        for s in &p.src_pos {
            sites.push((s.offset, s.uri.clone()));
        }
    }
    for p in pins {
        if let Some(s) = &p.fallback_pos {
            sites.push((s.offset, s.uri.clone()));
        }
    }
    for s in &comp.src_pos {
        sites.push((s.offset, s.uri.clone()));
    }
    sites
}

/// §8.5 cross-plane DC-relation completeness (conduit-equivalence-design.md
/// §8.5, PWR-2 upper clause) — the first consumer of the L1 island index
/// (island-attribution-design.md §7 L2). A two-terminal DC element *is* a
/// relation (the §8.5 unified predicate): when its two pads resolve to two
/// different potential classes whose domain-worlds are DISJOINT — the pads sit
/// in no single declared `rail[hot,ret]` loop — the physical leg is a
/// cross-plane DC relation (return↔return, hot↔hot supply bead, hot↔foreign
/// return) that the declaration layer must carry explicitly on the leg's own
/// statement. A judged leg is a forgotten single-point bridge or an intentional
/// bypass never declared. Advisory Warning (upper side) — the relation is never
/// inferred from the part type (§8.4 iron rule): the declaration is the only
/// evidence of intent.
///
/// Exemption is PER-LEG (data-gap 2 closed): the leg's own statement clause
/// span must contain one of its wiring sites (module def file) and the edge
/// endpoints must resolve, in the owning module's def space, to the declared
/// identities of the pad nets. Decoupling is naturally exempt — the two pads
/// share a domain world (one rail's own hot↔return loop). A leg whose carrier
/// position is unreachable (library/func body — a func cannot write
/// @bridge) falls back to net-pair-anywhere, conservative.
pub(crate) fn check_return_leg_undeclared(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let idx = crate::instant::island::NetIslandIndex::build(table);

    // Declared DC edges per owning module — the shared read (PI-2 reads the
    // bridge subset of the same map).
    let declared = declared_dc_edges(table);
    // Domain-level bridge clause spans per owning module (R3 §10.4) — the
    // stand-down map for legs wired by licensed statements.
    let mut domain_spans: std::collections::BTreeMap<u32, Vec<(usize, usize)>> =
        std::collections::BTreeMap::new();
    for (id, pi) in table.power_decls() {
        let is_module = table
            .get_entry(*id)
            .is_some_and(|e| matches!(e.kind, InstKind::Module));
        if !is_module {
            continue;
        }
        let spans: Vec<(usize, usize)> = pi
            .l1_domain_edges()
            .into_iter()
            .map(|d| (d.span.start, d.span.end))
            .collect();
        if !spans.is_empty() {
            domain_spans.insert(*id, spans);
        }
    }

    for comp in table.get_components() {
        // A through leg is a real two-terminal part wired on both pads.
        if comp.synthetic || comp.unselected || comp.not_fitted || comp.pin_count != 2 {
            continue;
        }
        // A DC element only (ruling 8, 2026-09-16): a capacitor has no DC path,
        // so a capacitive leg is not a DC relation at all — its return placement
        // is PI-3's object (6038), which reads the declared [hot, ret] pair
        // instead. Without this, the two rules would both fire on the exact
        // shape PI-3 exists for (Cap([hot, wrong-ret])), reporting one fact twice.
        if comp.element_class == Some(crate::semantic::basic::attr_keys::ElementClass::Capacitive) {
            continue;
        }
        let pins = table.get_pins_of(comp.id);
        if pins.len() != 2 {
            continue;
        }
        let (Some(an), Some(bn)) = (table.get_net_of(pins[0].id), table.get_net_of(pins[1].id))
        else {
            continue; // an unwired pad is a floating-input matter, not a leg
        };
        if an.id == bn.id {
            continue; // both pads shorted onto one net
        }
        let (Some(a), Some(b)) = (idx.get(an.id), idx.get(bn.id)) else {
            continue;
        };
        let Some(ma) = a.module else {
            continue;
        };
        if b.module != Some(ma) {
            continue; // a boundary-straddling tie is not a same-scope leg
        }
        // Resolve both pads to potential classes; a pad with no class is unjudged.
        let (Some(cla), Some(clb)) = (
            eff_class(table, &idx, a, &mut Vec::new()),
            eff_class(table, &idx, b, &mut Vec::new()),
        ) else {
            continue;
        };
        if cla.id == clb.id {
            continue; // same class = same-copper shunt / 0Ω tie
        }
        // Decoupling is naturally co-resident: the two classes share a declared
        // rail domain world (one rail's own hot↔return loop). Disjoint worlds —
        // including the empty-worlds of two pure conduit references — are
        // separate potential classes that a bare leg silently DC-joins.
        if cla.worlds.iter().any(|w| clb.worlds.contains(w)) {
            continue;
        }
        let ia = net_identities(table, ma, an);
        let ib = net_identities(table, ma, bn);
        let pair_hits: Vec<&DeclEdge> = declared
            .get(&ma)
            .map(|edges| edges.iter().filter(|d| pair_matches(d, &ia, &ib)).collect())
            .unwrap_or_default();
        let pair_declared = !pair_hits.is_empty();
        let sites = leg_sites(comp, &pins);
        let mdef_uri = comp_def_uri(table, ma); // the file the clause spans index against
                                                // Self-declared: an edge on this exact pair whose
                                                // clause span contains a
                                                // pad's wiring site in the module def file.
        let self_declared = pair_hits.iter().any(|d| {
            sites.iter().any(|(off, uri)| {
                uri == mdef_uri.as_deref().unwrap_or_default()
                    && d.lo <= (*off as usize)
                    && (*off as usize) < d.hi
            })
        });
        if self_declared {
            continue;
        }
        // R3 (intent-reference-layer-design.md §10.4/§10.6): a leg wired by a
        // statement licensed with `@bridge(domain, domain)` is the domain-bridge
        // family's object (6046–6049 judge the license, the direction, the leg
        // consistency and the collection completeness). This rule's object is
        // the *net-level* relation — the endpoints its `DeclEdge` texts name —
        // so it stands down on a leg whose wiring site sits inside a
        // domain-bridge clause span, exactly as it does for `self_declared`.
        if domain_edge_covers(
            &domain_spans,
            ma,
            &sites,
            mdef_uri.as_deref().unwrap_or_default(),
        ) {
            continue;
        }
        let in_module = sites
            .iter()
            .any(|(_, uri)| uri == mdef_uri.as_deref().unwrap_or_default());
        if !in_module && pair_declared {
            continue; // library/func carrier — net-pair fallback, conservative
        }
        let (cua, cub) = if cla.id < clb.id {
            (cla.id, clb.id)
        } else {
            (clb.id, cla.id)
        };
        let (pos, uri) = entry_pos(comp);
        let mut message = crate::errcodes::format_msg(
            crate::errcodes::RETURN_LEG_UNDECLARED,
            &[&cua, &cub, &comp.path],
        );
        if pair_declared {
            // Parallel reading: a @bridge on the same pair on another leg does
            // not cover this one — a parallel carrier must carry its own.
            message.push_str(
                " A @bridge on the same pair on another leg does not exempt this \
                 parallel leg — declare it here (and @star to discharge 6007).",
            );
        }
        results.push(NetCheckResult {
            check: "return-leg-undeclared",
            severity: "warning",
            message,
            net_name: an.name.clone(),
            code: crate::errcodes::RETURN_LEG_UNDECLARED,
            pos,
            uri,
        });
    }
}

/// Does one domain-bridge clause span cover a leg's wiring site? The same
/// site-in-span test `self_declared` uses for net-level edges, over the R3
/// domain-edge spans collected above.
fn domain_edge_covers(
    domain_spans: &std::collections::BTreeMap<u32, Vec<(usize, usize)>>,
    module_id: u32,
    sites: &[(u32, String)],
    mdef_uri: &str,
) -> bool {
    domain_spans.get(&module_id).is_some_and(|spans| {
        spans.iter().any(|&(lo, hi)| {
            sites
                .iter()
                .any(|(off, uri)| uri == mdef_uri && lo <= (*off as usize) && (*off as usize) < hi)
        })
    })
}

/// Owning module entry's definition-file URI — the file whose byte spans the
/// module's `l1_edges()` clause spans index against.
fn comp_def_uri(table: &InstTable, module_id: u32) -> Option<String> {
    table
        .get_entry(module_id)
        .map(|e| e.def_uri.clone())
        .filter(|u| !u.is_empty())
}

/// One return-side class a device's DC-pair returns resolve to (6027): the
/// class id, its declared domain worlds, and the direction flags of the
/// `psrc`/`psnk`/`psbi` contracts whose `ret` member that return pin is.
struct DeviceReturnClass {
    ma: u32,
    id: String,
    worlds: Vec<String>,
    net_name: String,
    isolated: bool,
    sink: bool, // some owning contract is Snk (or a psbi charge half)
    src: bool,  // some owning contract is Src (or a psbi discharge half)
}

/// §8.6 device reference-pin cross-plane (conduit-equivalence-design.md §8.6,
/// adjudicated 2026-09-09) — the ≥3-pin functional sibling of 6022. A device
/// whose DC-pair *return* pins (the `ret` member of each `psnk`/`psrc`/`psbi`
/// `::DC` row, §4.1) resolve to two different potential classes whose domain-
/// worlds are DISJOINT is a candidate silent merge: the die/substrate DC-joins
/// two board return planes the declaration layer never tied. A two-terminal leg
/// is 6022's object (the part *is* the relation and carries its own `@bridge`);
/// a functional device's internal return commonality is not a declarable leg, so
/// the span must be covered by a declaration:
///   ① a net-level declared `@bridge`/`@couple` on the class pair (the
///      uc/GND↔GNDA shape — FB_agnd already declares the return tie), or
///   ② a declared power-isolation structure (the iso5/DC.ISO_SRC shape, both
///      conditions): one return class is an `@role(isolated)` copper carried by
///      a *source-side* (`psrc`/`psbi`) contract AND another return class is
///      carried by a *sink-side* (`psnk`/`psbi`) contract — the device is the
///      isolator that defines the isolated world, whose copper expects no DC
///      bridge to any world (§3.2), so its return span is the isolation itself.
///      Isolation alone is not enough: a sink-only device returning across an
///      isolated + a main class is precisely the hidden DC bridge into the
///      isolated world the role forbids, and is judged.
/// A disjoint return-class span under neither fires a Warning. Signal-derived
/// return nets (no class), single-return devices, and returns sharing a world
/// are not adjudicated. The merge is never inferred from a part type (§8.4).
pub(crate) fn check_device_return_span(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let idx = crate::instant::island::NetIslandIndex::build(table);

    // ① net-level declared @bridge/@couple class pairs, keyed by owning module.
    let mut bridges: std::collections::HashMap<u32, HashSet<(String, String)>> =
        std::collections::HashMap::new();
    // Each owning module's conduit `@role` tags (name → tag); isolation is a
    // declared-world fact, not island attribution (net-island-attribution §2).
    let mut roles: std::collections::HashMap<u32, std::collections::HashMap<String, String>> =
        std::collections::HashMap::new();
    for (id, pi) in table.power_decls() {
        let is_module = table
            .get_entry(*id)
            .is_some_and(|e| matches!(e.kind, InstKind::Module));
        if !is_module {
            continue;
        }
        let pair_set: HashSet<(String, String)> = pi
            .l1_edges()
            .into_iter()
            .filter(|e| {
                matches!(
                    e.kind,
                    crate::semantic::module::pi::L1EdgeKind::Bridge
                        | crate::semantic::module::pi::L1EdgeKind::Couple
                )
            })
            .filter_map(|e| {
                let (Some(a), Some(b)) = (e.endpoints.first(), e.endpoints.get(1)) else {
                    return None;
                };
                if a == b {
                    return None;
                }
                let (a, b) = if a < b {
                    (a.clone(), b.clone())
                } else {
                    (b.clone(), a.clone())
                };
                Some((a, b))
            })
            .collect();
        if !pair_set.is_empty() {
            bridges.insert(*id, pair_set);
        }
        let role_map: std::collections::HashMap<String, String> = pi
            .l1_refs()
            .into_iter()
            .filter_map(|r| r.role.map(|role| (r.name.clone(), role)))
            .collect();
        if !role_map.is_empty() {
            roles.insert(*id, role_map);
        }
    }

    let workspace = crate::definition_space().workspace_components();
    let defs: std::collections::HashMap<String, &McComponent> = workspace
        .iter()
        .map(|(sn, c)| (sn.ident.to_string(), c.as_ref()))
        .collect();

    for comp in table.get_components() {
        if comp.synthetic || comp.unselected || comp.not_fitted {
            continue;
        }
        if comp.pin_count < 3 {
            continue; // the two-terminal object is 6022's (§8.6: 6027 is the
                      // ≥3-pin functional device, whose merge is not a declarable leg)
        }
        let Some(def) = defs.get(&comp.class_name).copied() else {
            continue;
        };
        if def.pins.pwr.is_empty() {
            continue; // no DC-pair return contract to straddle
        }
        let pins = table.get_pins_of(comp.id);
        let mut classes: std::collections::HashMap<String, DeviceReturnClass> =
            std::collections::HashMap::new();
        for pin in &pins {
            // Which DC-pair rows is this flat pin the *return* terminal of?
            // The def-side pin's registered names carry its terminal member;
            // a return pin's name equals some contract's `ret`.
            let pin_id = pin.path.rsplit('.').next().unwrap_or("");
            let names: Vec<&str> = def
                .pins
                .pins
                .get(pin_id)
                .map(|p| p.names.iter().map(|n| n.as_str()).collect())
                .unwrap_or_default();
            let own: Vec<&McPwrPin> = def
                .pins
                .pwr
                .iter()
                .filter(|c| {
                    c.ret
                        .as_deref()
                        .is_some_and(|r| names.iter().any(|n| *n == r))
                })
                .collect();
            if own.is_empty() {
                continue; // hot / signal / NC pin — only return members straddle
            }
            let Some(net) = table.get_net_of(pin.id) else {
                continue; // an unwired return pin is a floating-input matter
            };
            let Some(attr) = idx.get(net.id) else {
                continue;
            };
            let Some(ma) = attr.module else {
                continue;
            };
            let Some(cls) = eff_class(table, &idx, attr, &mut Vec::new()) else {
                continue; // return on a Signal/unresolvable net — unjudged
            };
            let sink = own
                .iter()
                .any(|c| matches!(c.dir, PwrDir::Snk | PwrDir::Bi));
            let src = own
                .iter()
                .any(|c| matches!(c.dir, PwrDir::Src | PwrDir::Bi));
            let isolated = roles
                .get(&ma)
                .and_then(|m| m.get(&cls.id))
                .is_some_and(|r| r == attr_keys::WORD_ISOLATED);
            let slot = classes
                .entry(cls.id.clone())
                .or_insert_with(|| DeviceReturnClass {
                    ma,
                    id: cls.id.clone(),
                    worlds: cls.worlds.clone(),
                    net_name: net.name.clone(),
                    isolated,
                    sink,
                    src,
                });
            slot.sink |= sink;
            slot.src |= src;
            slot.isolated |= isolated;
        }
        if classes.len() < 2 {
            continue; // single return class — nothing crosses a plane
        }
        // The pair loops below push one row per unordered pair in `list` order,
        // so `list` order *is* the order of this rule's rows. A `HashMap` draws
        // its iteration order fresh per process, which would make that order -
        // and with it the report's row order - a property of the process rather
        // than of the input (build-design §3.7 discipline 4). Sorting by the
        // class id makes it the input's.
        let mut list: Vec<DeviceReturnClass> = classes.into_values().collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        // Judge pairs only within one owning scope; a device whose return pins
        // straddle modules is a boundary-tie matter, not a die merge.
        if list.iter().any(|c| c.ma != list[0].ma) {
            continue;
        }
        let ma = list[0].ma;
        let pairs = bridges.get(&ma);
        for i in 0..list.len() {
            for j in (i + 1)..list.len() {
                let (a, b) = (&list[i], &list[j]);
                // Co-resident in a declared world = one rail's own return loop —
                // not a cross-plane span.
                if a.worlds.iter().any(|w| b.worlds.contains(w)) {
                    continue;
                }
                let (xa, xb) = if a.id < b.id {
                    (&a.id, &b.id)
                } else {
                    (&b.id, &a.id)
                };
                // ① net-level declared bridge/couple on the class pair.
                if pairs.is_some_and(|ps| ps.contains(&(xa.clone(), xb.clone()))) {
                    continue;
                }
                // ② declared isolation structure (both conditions, §8.6): the
                // isolated return is source-fed (the isolator's output side) and
                // the other return is sink-side (the isolator's input return).
                let covered_iso =
                    (a.isolated && a.src && b.sink) || (b.isolated && b.src && a.sink);
                if covered_iso {
                    continue;
                }
                let (pos, uri) = entry_pos(comp);
                results.push(NetCheckResult {
                    check: "device-return-span-undeclared",
                    severity: "warning",
                    message: crate::errcodes::format_msg(
                        crate::errcodes::DEVICE_RETURN_SPAN_UNDECLARED,
                        &[xa, xb, &comp.path],
                    ),
                    net_name: a.net_name.clone(),
                    code: crate::errcodes::DEVICE_RETURN_SPAN_UNDECLARED,
                    pos,
                    uri,
                });
            }
        }
    }
}

/// §8.7 port role contract: an `out` port's `@bind_role(<role>)` parent binding
/// must resolve to a reference of that role, else an Error (design §8.7).
pub(crate) fn check_port_bind_role(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let idx = crate::instant::island::NetIslandIndex::build(table);

    // Owning-module declaration plane: conduit name -> @role (default main), and
    // (module instance, port name) -> declared @bind_role.
    let mut conduit_roles: std::collections::HashMap<
        u32,
        std::collections::HashMap<String, String>,
    > = std::collections::HashMap::new();
    let mut port_binds: std::collections::HashMap<(u32, String), String> =
        std::collections::HashMap::new();
    for (id, pi) in table.power_decls() {
        if !table
            .get_entry(*id)
            .is_some_and(|e| matches!(e.kind, InstKind::Module))
        {
            continue;
        }
        let mut refs = std::collections::HashMap::new();
        for r in pi.l1_refs() {
            refs.insert(r.name, r.role.unwrap_or_else(|| "main".to_string()));
        }
        if !refs.is_empty() {
            conduit_roles.insert(*id, refs);
        }
        for p in pi.l1_ports() {
            if let Some(br) = p.bind_role {
                port_binds.insert((*id, p.name), br);
            }
        }
    }
    if port_binds.is_empty() {
        return; // no @bind_role anywhere — no contract to witness
    }

    for (id, entry) in table.iter() {
        if !matches!(entry.kind, InstKind::Port) || !matches!(entry.io_type, IOType::Out) {
            continue;
        }
        if entry.synthetic {
            continue; // interface/dynamic wrapper port — no own declaration
        }
        let Some(owner) = entry.parent_id else {
            continue;
        };
        let pname = entry.path.rsplit('.').next().unwrap_or("");
        let Some(want) = port_binds.get(&(owner, pname.to_string())) else {
            continue;
        };

        // The binding lives on the port's parent-side co-segment — the segment
        // NOT produced by the port's own module scope. A port unbound in the
        // parent has only its interior segment; that dangler is C4's (E4114).
        let Some(net) = table
            .nets_of(*id)
            .iter()
            .filter_map(|nid| table.get_net(*nid))
            .find(|n| n.module != Some(owner))
        else {
            continue;
        };

        let got = resolve_bind_role(table, &idx, net, *id, &conduit_roles, &port_binds);
        if got.as_deref() == Some(want.as_str()) {
            continue;
        }
        let got_str = got.as_deref().unwrap_or("none");
        let (pos, uri) = entry_pos(entry);
        results.push(NetCheckResult {
            check: "port-bind-role-mismatch",
            severity: "error",
            message: crate::errcodes::format_msg(
                crate::errcodes::PORT_BIND_ROLE_MISMATCH,
                &[&entry.path, want, &net.name, &got_str],
            ),
            net_name: net.name.clone(),
            code: crate::errcodes::PORT_BIND_ROLE_MISMATCH,
            pos,
            uri,
        });
    }
}

/// The role a parent binding witnesses for a `@bind_role` port: the target
/// segment's conduit `@role`, else a forwarding `out` port; `None` = no role.
fn resolve_bind_role(
    table: &InstTable,
    idx: &crate::instant::island::NetIslandIndex,
    net: &NetEntry,
    subject: u32,
    conduit_roles: &std::collections::HashMap<u32, std::collections::HashMap<String, String>>,
    port_binds: &std::collections::HashMap<(u32, String), String>,
) -> Option<String> {
    let Some(layer) = net.module else {
        return None;
    };
    if let Some(attr) = idx.get(net.id) {
        if let Some(cls) = eff_class(table, idx, attr, &mut Vec::new()) {
            if let Some(role) = conduit_roles.get(&layer).and_then(|m| m.get(&cls.id)) {
                return Some(role.clone());
            }
        }
    }
    // Forwarding: the layer re-exports the contract through one of its own out
    // ports. Only the layer's own ports witness it (a sibling of the subject).
    for pid in &net.points {
        if *pid == subject {
            continue;
        }
        let Some(pe) = table.get_entry(*pid) else {
            continue;
        };
        if pe.kind != InstKind::Port || !matches!(pe.io_type, IOType::Out) {
            continue;
        }
        if pe.parent_id != Some(layer) {
            continue;
        }
        let pname = pe.path.rsplit('.').next().unwrap_or("");
        if let Some(br) = port_binds.get(&(layer, pname.to_string())) {
            return Some(br.clone());
        }
    }
    None
}

/// PWR-6 (exposed-protection-design.md §3, ruled 2026-09-16): a port row
/// declaring `@exposed(<threat>)` puts its net at the board's transient
/// boundary, and a boundary net must carry a declared clamp onto a
/// `@role(protective)`/`@role(earth)` reference — else the declared exposure
/// has no discharge path and a transient pours straight into the interior.
///
/// Coverage (the ruled same-net kernel; the downstream chain is a later step):
/// an exposed port is covered when one of its net segments carries a device
/// that is on that segment *and* on the clamped reference net —
///
/// ```text
/// ∃ device C, ∃ net M of C:  C has a pin on an exposed segment S,
///                            C has a pin on M,
///                            M's effective class resolves in M's scope to a
///                            reference that scope declares, and
///                            that scope declares @clamp(<that reference>).
/// ```
///
/// The `@clamp` declaration is required (ruled 2026-09-16): an ordinary
/// decoupling capacitor or series resistor onto the protective island is not a
/// clamp, so coverage is the declaration rather than the topology alone.
/// [`L1Edge`](crate::semantic::module::pi::L1Edge) carries no host net (only
/// kind / endpoints / span), so the dump leg is read through the reference it
/// names, resolved in the declaration scope exactly like 6029's role lookup.
///
/// The reference's *role* is not part of coverage: PWR-6 is the existence half
/// and sits upstream of PWR-7, which owns "the reference is not
/// protective/earth" (a clamp declared onto a main/quiet reference fires 6008
/// alone — the defect is the wrong reference, not a missing clamp).
///
/// Not adjudicated, never guessed: an exposed port whose segment does not
/// resolve in its own scope (a library port row, a dangling declaration) and a
/// reference whose class cannot be resolved — the role of those is supplied by
/// an ancestor world / port contract (iron rule 1 §6).
/// Every site that declares a transient boundary, in flat terms: `(host path,
/// host entry id, declared levels)`.
///
/// The language has two hosts for one declaration (exposed-protection-design
/// §2): a module port row (`io USB_DP @exposed(esd_contact)`) and a component
/// pin row (`io 3 = D+ @exposed(esd_contact)`). Both spellings are the same
/// word on the same kind of declaration, so PWR-6's two halves read **this one
/// list** — a half that gathered its own hosts would be a second predicate,
/// and the two would drift.
///
/// The port host is resolved by canonical path (never by splitting a path back
/// into names); the pin host is the flat entry itself, whose `exposed` carry
/// was decoded at flatten time from the row. Hosts are yielded in table order.
pub(crate) fn exposed_hosts(table: &InstTable) -> Vec<(String, u32, Vec<String>)> {
    let mut out = Vec::new();
    for (id, pi) in table.power_decls() {
        let Some(m) = table.get_entry(*id) else {
            continue;
        };
        if !matches!(m.kind, InstKind::Module) {
            continue;
        }
        for p in pi.l1_ports() {
            if p.exposed.is_empty() {
                continue;
            }
            let host_path = format!("{}.{}", m.path, p.name);
            let Some(pid) = table.get_id_by_path(&host_path) else {
                continue;
            };
            out.push((host_path, pid, p.exposed.clone()));
        }
    }
    for (_, e) in table.iter() {
        if matches!(e.kind, InstKind::Pin) && !e.exposed.is_empty() {
            out.push((e.path.clone(), e.id, e.exposed.clone()));
        }
    }
    out
}

pub(crate) fn check_exposed_clamp_coverage(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let idx = crate::instant::island::NetIslandIndex::build(table);
    let (ref_names, clamp_refs) = clamp_declaration_plane(table);

    for (host_path, pid, levels) in exposed_hosts(table) {
        let segs: Vec<&NetEntry> = table
            .nets_of(pid)
            .iter()
            .filter_map(|n| table.get_net(*n))
            .collect();
        if segs.is_empty() {
            continue; // no resolvable segment — not adjudicated
        }
        if segs
            .iter()
            .any(|s| segment_is_clamped(table, &idx, s, &ref_names, &clamp_refs))
        {
            continue;
        }
        let (pos, uri) = table
            .get_entry(pid)
            .map(entry_pos)
            .unwrap_or((0, String::new()));
        results.push(NetCheckResult {
            check: "exposed-net-no-clamp",
            severity: "error",
            message: crate::errcodes::format_msg(
                crate::errcodes::EXPOSED_NET_NO_CLAMP,
                &[&host_path, &levels.join(","), &segs[0].name],
            ),
            net_name: segs[0].name.clone(),
            code: crate::errcodes::EXPOSED_NET_NO_CLAMP,
            pos,
            uri,
        });
    }
}

/// Is this exposed segment clamped? A device on the segment whose dump leg sits
/// on a reference the same scope declares `@clamp` on.
///
/// A net's points are the *pins* (and labels) that land on it, so the device is
/// read through a point's parent instance — the pin belongs to the component.
fn segment_is_clamped(
    table: &InstTable,
    idx: &crate::instant::island::NetIslandIndex,
    seg: &NetEntry,
    ref_names: &std::collections::HashMap<u32, HashSet<String>>,
    clamp_refs: &std::collections::HashMap<u32, HashSet<String>>,
) -> bool {
    for &pt in &seg.points {
        let Some(entry) = table.get_entry(pt) else {
            continue;
        };
        let comp_id = match entry.kind {
            InstKind::Pin => entry.parent_id,
            InstKind::Component => Some(entry.id),
            _ => None,
        };
        let Some(comp_id) = comp_id else {
            continue;
        };
        if !table
            .get_entry(comp_id)
            .is_some_and(|c| matches!(c.kind, InstKind::Component))
        {
            continue;
        }
        for pin in table.get_pins_of(comp_id) {
            for &leg in table.nets_of(pin.id) {
                if leg == seg.id {
                    continue;
                }
                let Some(net) = table.get_net(leg) else {
                    continue;
                };
                let Some(layer) = net.module else {
                    continue;
                };
                let Some(attr) = idx.get(net.id) else {
                    continue;
                };
                let Some(cls) = eff_class(table, idx, attr, &mut Vec::new()) else {
                    continue;
                };
                if !ref_names.get(&layer).is_some_and(|s| s.contains(&cls.id)) {
                    continue; // the dump leg does not land on a declared ref
                }
                if clamp_refs.get(&layer).is_some_and(|s| s.contains(&cls.id)) {
                    return true;
                }
            }
        }
    }
    false
}

/// The declaration plane both halves of PWR-6 read: per module instance, the
/// references it declares, and the references its own `@clamp(ref)` legs name.
/// One build, because 6031 (existence) and 6044 (downstream chain) ask the same
/// question of the same tables — a second copy would let the two halves of one
/// rule disagree about what a clamp is.
fn clamp_declaration_plane(
    table: &InstTable,
) -> (
    std::collections::HashMap<u32, HashSet<String>>,
    std::collections::HashMap<u32, HashSet<String>>,
) {
    let mut ref_names: std::collections::HashMap<u32, HashSet<String>> =
        std::collections::HashMap::new();
    let mut clamp_refs: std::collections::HashMap<u32, HashSet<String>> =
        std::collections::HashMap::new();
    for (id, pi) in table.power_decls() {
        if !table
            .get_entry(*id)
            .is_some_and(|e| matches!(e.kind, InstKind::Module))
        {
            continue;
        }
        let refs: HashSet<String> = pi.l1_refs().into_iter().map(|r| r.name).collect();
        if !refs.is_empty() {
            ref_names.insert(*id, refs);
        }
        let clamps: HashSet<String> = pi
            .l1_edges()
            .into_iter()
            .filter(|e| e.kind == crate::semantic::module::pi::L1EdgeKind::Clamp)
            .filter_map(|e| e.endpoints.first().cloned())
            .collect();
        if !clamps.is_empty() {
            clamp_refs.insert(*id, clamps);
        }
    }
    (ref_names, clamp_refs)
}

/// The removal method's "nothing is cut out" sentinel (PWR-4b's series half,
/// package-thermal-design.md §7; the same device PWR-5's series half asks in
/// exposed-protection-design.md §8.3): no component id ever reaches `u32::MAX`,
/// so a removal-aware walk handed it answers the **intact** graph. Pairing a
/// reading taken with a real id against one taken with this makes "did this end
/// lose its feed" a comparison of one predicate with itself, never of two
/// different reach readings.
pub(super) const NO_SKIP: u32 = u32::MAX;

/// `Ret`/`Reference` copper never carries hot-side reach: a region neither
/// starts on it nor floods through it (reach.rs §7 L4's distinction, which keeps
/// a decoupling cap — structurally identical to a feed ferrite at the flat layer
/// — from masquerading as a source).
pub(super) fn role_excluded(idx: &crate::instant::island::NetIslandIndex, net_id: u32) -> bool {
    idx.get(net_id).is_some_and(|a| {
        matches!(
            a.role,
            crate::instant::island::NetRole::Ret | crate::instant::island::NetRole::Reference
        )
    })
}

/// The **current-transparent copper region** of `start`: every net reachable
/// from it through transparent copper, and nothing else. Two arms, verbatim the
/// walk `budget_derive.rs`'s `fill_region` carried — `reach.rs` / `budget.rs` /
/// `window.rs` carry the same two arms in a per-leg shape (first-wins / first
/// *fed* rather than a set), so they are not this helper's callers:
///
/// * **transparent-copper arm** — a component the caller calls `transparent` is
///   a bridge: every one of its pins' nets joins the region. The caller owns
///   that predicate because "no DC rows" is the shape test and a declared gate
///   (`protect = series`) is an overlay it adds on top.
/// * **module-boundary arm** — the *same* physical copper across a submodule
///   port is a second `NetEntry` (both share the junction point id, `module`
///   differs), so a co-segment owned by another scope joins the region.
///
/// `seen` / `out` are threaded so a caller can accumulate several seeds' regions
/// into one walk (budget's per-region demand does exactly that).
pub(super) fn copper_region_into(
    table: &InstTable,
    idx: &crate::instant::island::NetIslandIndex,
    transparent: &dyn Fn(u32) -> bool,
    net_id: u32,
    seen: &mut HashSet<u32>,
    out: &mut Vec<u32>,
) {
    if !seen.insert(net_id) {
        return;
    }
    if role_excluded(idx, net_id) {
        return;
    }
    let Some(net) = table.get_net(net_id) else {
        return;
    };
    out.push(net_id);
    // Transparent-copper arm: the component's other pins' nets forward.
    for &pid in &net.points {
        let Some(entry) = table.get_entry(pid) else {
            continue;
        };
        if !matches!(entry.kind, InstKind::Pin) {
            continue;
        }
        let Some(cid) = entry.parent_id else {
            continue;
        };
        if !transparent(cid) {
            continue;
        }
        for pin in table.get_pins_of(cid) {
            let Some(pnet) = table.get_net_of(pin.id) else {
                continue;
            };
            if pnet.id != net.id {
                copper_region_into(table, idx, transparent, pnet.id, seen, out);
            }
        }
    }
    // Module-boundary arm: a co-segment in another scope is the same copper.
    if let Some(m) = net.module {
        for &pid in &net.points {
            for &cid in table.nets_of(pid) {
                if cid == net.id {
                    continue;
                }
                let Some(co) = table.get_net(cid) else {
                    continue;
                };
                if co.module.is_none() || co.module == Some(m) {
                    continue;
                }
                copper_region_into(table, idx, transparent, cid, seen, out);
            }
        }
    }
}

/// PWR-6 **downstream chain** (exposed-protection-design.md §3.1, six rulings
/// 2026-09-17): 6031 asks the existence question on the exposed net itself; this
/// half asks the *direction* question the canon's "already past a clamp or
/// current-limit chain before entering an intolerant domain" names.
///
/// ```text
/// region(P) = the nets reachable from P's segments by flooding transparent
///             copper
///             crosses: parts with no DC rows, module-boundary co-segments
///             does not cross: Ret / Reference copper (reach.rs §7 L4)
///             stops at a gate: InstEntry.protection == Some(Series)
/// report(P) <=> some segment N of region(P) \ P:
///                 N is a quiet/sensitive face (§1.4's read) and N carries no
///                 declared clamp of its own
/// ```
///
/// Region **existence**, not path search (§3.1.4): `region` holds no path, so a
/// clamp anywhere in it covers the whole region. The gate is a declaration only
/// — an unmarked two-terminal pass is ordinary copper and does not stop the
/// flood ("a declaration is the contract", the same rule PWR-5's classification
/// rests on).
///
/// Not judged, never guessed (§3.1.5): a port whose segments do not resolve in
/// its own scope; a region net with no owning layer, no resolvable class or no
/// declared face (§1.3's silence); a region net that is itself clamped; and a
/// board declaring no faces at all. **Never stacked with 6031** — that rule's
/// object is the exposed net and this one's is the region *minus* it (§3.1.6).
pub(crate) fn check_exposed_clamp_downstream(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let idx = crate::instant::island::NetIslandIndex::build(table);
    let faces = faces::DomainFaces::read(table);
    if faces.is_empty() {
        return; // no declared face anywhere — nothing can be an untolerated domain
    }
    // Def by class name, the resolution 6027 uses: the flat carries the class,
    // the definition space carries the pin contract.
    let workspace = crate::definition_space().workspace_components();
    let defs: std::collections::HashMap<String, &McComponent> = workspace
        .iter()
        .map(|(sn, c)| (sn.ident.to_string(), c.as_ref()))
        .collect();
    let (ref_names, clamp_refs) = clamp_declaration_plane(table);

    for (host_path, pid, levels) in exposed_hosts(table) {
        let segs: Vec<&NetEntry> = table
            .nets_of(pid)
            .iter()
            .filter_map(|n| table.get_net(*n))
            .collect();
        if segs.is_empty() {
            continue; // no resolvable segment — not adjudicated
        }

        // 6031's half owns the uncovered case. An exposed net carrying no
        // clamp at all is that rule's verdict alone — one defect, one code
        // (§3.1.4's "S unclamped means 6031 alone", the same one-cause-one-
        // code law as rulings 8/11) — so the downstream half judges only
        // ports whose own copper a clamp already covers. The read is 6031's
        // own predicate, not a second one.
        if !segs
            .iter()
            .any(|s| segment_is_clamped(table, &idx, s, &ref_names, &clamp_refs))
        {
            continue;
        }
        // The flood: a declared gate stops it, and nothing else does. An
        // unmarked two-terminal pass is ordinary copper.
        let transparent = |cid: u32| {
            let Some(entry) = table.get_entry(cid) else {
                return false;
            };
            if entry.protection == Some(crate::instant::insttab::ProtectionKind::Series) {
                return false; // a declared current-limit chain — stop here
            }
            defs.get(&entry.class_name)
                .is_some_and(|d| d.pins.pwr.is_empty())
        };
        // The host's own copper is the region's seed, not its object: a
        // second segment of the same declaration row is never "downstream".
        let own: HashSet<u32> = segs.iter().map(|s| s.id).collect();
        let mut region: Vec<u32> = Vec::new();
        let mut seen: HashSet<u32> = HashSet::new();
        for s in &segs {
            copper_region_into(table, &idx, &transparent, s.id, &mut seen, &mut region);
        }
        let mut hit: Option<(u32, String)> = None;
        for &n in &region {
            if own.contains(&n) {
                continue;
            }
            let Some(net) = table.get_net(n) else {
                continue;
            };
            let Some(attr) = idx.get(n) else {
                continue;
            };
            let Some(layer) = attr.module else {
                continue;
            };
            let Some(cls) = eff_class(table, &idx, attr, &mut Vec::new()) else {
                continue;
            };
            let Some(world) = faces.quiet_world(table, layer, &cls.worlds) else {
                continue; // not a quiet/sensitive face — no object
            };
            if segment_is_clamped(table, &idx, net, &ref_names, &clamp_refs) {
                continue; // this net is protected on its own account
            }
            hit = Some((n, world));
            break;
        }
        let Some((n, world)) = hit else {
            continue;
        };
        let Some(downstream) = table.get_net(n).map(|net| net.name.clone()) else {
            continue;
        };
        let (pos, uri) = table
            .get_entry(pid)
            .map(entry_pos)
            .unwrap_or((0, String::new()));
        results.push(NetCheckResult {
            check: "exposed-net-downstream-unprotected",
            severity: "error",
            message: crate::errcodes::format_msg(
                crate::errcodes::EXPOSED_NET_DOWNSTREAM_UNPROTECTED,
                &[&host_path, &levels.join(","), &downstream, &world],
            ),
            net_name: downstream,
            code: crate::errcodes::EXPOSED_NET_DOWNSTREAM_UNPROTECTED,
            pos,
            uri,
        });
    }
}

/// Render an amps value for diagnostics: `< 1 A` as mA, else as A (`500mA`,
/// `1.5A`). Sub-milli values keep two decimals.
fn fmt_amps(a: f64) -> String {
    if a.abs() < 1.0 {
        format!("{}mA", fmt_round(a * 1000.0))
    } else {
        format!("{}A", fmt_round(a))
    }
}

/// Round to two decimals; drop the `.00`/`.0` tail for whole values.
fn fmt_round(x: f64) -> String {
    let r = (x * 100.0).round() / 100.0;
    if (r - r.round()).abs() < 1e-9 {
        format!("{}", r.round() as i64)
    } else {
        format!("{r}")
    }
}

/// The `psnk` contract whose *hot* terminal this flat pin is, if any. A flat
/// pin is the hot member of a sink contract when the def-side member names of
/// its pin id include the contract's `hot` (pin ids are positional: `main.s.1`
/// → def pin "1" whose registered names are the terminals); the return member
/// (`ret`, e.g. GND) never matches its own contract's `hot`, so the return pin
/// is naturally skipped.
pub(crate) fn sink_contract_for<'a>(
    def: &'a McComponent,
    entry: &InstEntry,
) -> Option<&'a McPwrPin> {
    let pin_id = entry.path.rsplit('.').next().unwrap_or("");
    let names: Vec<&str> = def
        .pins
        .pins
        .get(pin_id)
        .map(|p| p.names.iter().map(|n| n.as_str()).collect())
        .unwrap_or_default();
    def.pins.pwr.iter().find(|c| {
        c.dir == PwrDir::Snk && (names.iter().any(|n| *n == c.hot) || entry.class_name == c.hot)
    })
}

/// The `psrc`/`psbi` contract whose *hot* terminal this flat pin is, if any —
/// the source-side mirror of [`sink_contract_for`]. A `psbi` counts as a source
/// root because its `::DC(v)` is the *discharge* supply guarantee (§4.1), which
/// is the S its hot net carries while it sources. Shared-return pins never match
/// the contract's own `hot`, so return nets get no S from source pins.
pub(crate) fn source_contract_for<'a>(
    def: &'a McComponent,
    entry: &InstEntry,
) -> Option<&'a McPwrPin> {
    let pin_id = entry.path.rsplit('.').next().unwrap_or("");
    let names: Vec<&str> = def
        .pins
        .pins
        .get(pin_id)
        .map(|p| p.names.iter().map(|n| n.as_str()).collect())
        .unwrap_or_default();
    def.pins.pwr.iter().find(|c| {
        matches!(c.dir, PwrDir::Src | PwrDir::Bi)
            && (names.iter().any(|n| *n == c.hot) || entry.class_name == c.hot)
    })
}

/// The net the instance terminal carrying the declared member `(face, member)`
/// lands on. The pairing is the flatten pass's own ([`InstEntry::pwr_member`],
/// written from the declaration that owns the terminal), so two terminals of one
/// instance answer the same member only when one declaration named them both —
/// which is what lets a rule read a *declared pair* off the instance it was bound
/// on, rather than off the class it came from. Both terminal kinds are searched
/// (a component's `Pin`, an instantiated module's `Port`), so the two shapes of a
/// declared supply pair resolve through one read.
fn member_net_of(table: &InstTable, parent: u32, face: Face, member: &str) -> Option<u32> {
    for (id, entry) in table.iter() {
        if entry.parent_id != Some(parent) || !matches!(entry.kind, InstKind::Pin | InstKind::Port)
        {
            continue;
        }
        let Some(m) = &entry.pwr_member else {
            continue;
        };
        if m.face == face && m.member == member {
            return table.get_net_of(*id).map(|n| n.id);
        }
    }
    None
}

/// A flat net's name, or the empty string when the id is unknown — how the
/// messages in this family spell a net.
fn net_name(table: &InstTable, net: u32) -> String {
    table
        .get_net(net)
        .map(|n| n.name.clone())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::diagnostic::override_store::{
        install_store, AllowEntry, OverrideStore, PathScope,
    };
    use crate::semantic::validation::CheckSeverity;

    fn net_result(code: u32, uri: &str) -> NetCheckResult {
        NetCheckResult {
            check: "probe",
            severity: "error",
            message: "probe message".to_string(),
            net_name: "n".to_string(),
            code,
            pos: 7,
            uri: uri.to_string(),
        }
    }

    fn diag_key(d: &Diagnostic) -> (u32, String) {
        (d.code, d.msg.clone())
    }

    #[test]
    fn net_results_to_diagnostics_is_identity_under_any_store_today() {
        // §8-5 identity anchor: while every catalog rule is non-overridable,
        // even a store that would otherwise hit must leave the FlatErc
        // diagnostic output byte-identical. This is what lets the lock tests
        // run unmodified.
        let results = [
            net_result(crate::errcodes::NET_MULTI_DRIVE, ""),
            net_result(1, "a.mc"), // unregistered code
        ];
        let empty = net_results_to_diagnostics(&results);

        // A populated store (severity override + global allow for E4101)
        // refuses both under the current non-overridable catalog.
        let mut store = OverrideStore::default();
        store
            .project
            .severities
            .insert(crate::errcodes::NET_MULTI_DRIVE, CheckSeverity::Info);
        store.project.allows.push(AllowEntry {
            code: crate::errcodes::NET_MULTI_DRIVE,
            path: PathScope::Project,
            reason: None,
        });
        install_store(store);
        let populated = net_results_to_diagnostics(&results);
        install_store(OverrideStore::default());
        assert_eq!(
            populated.iter().map(diag_key).collect::<Vec<_>>(),
            empty.iter().map(diag_key).collect::<Vec<_>>()
        );
        assert_eq!(populated.len(), 2);
    }
}
