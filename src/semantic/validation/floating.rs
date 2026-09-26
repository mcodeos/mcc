// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Floating-label validation for function bodies.
//!
//! E3136 (FUNC_FLOATING_LABEL): a bare identifier in a func body net
//! statement that resolves to nothing declared (pin, interface, parameter
//! member, or func-local instance) becomes a dangling net label
//! (mc_phrase.rs single-segment fallback). The criterion is positional — a
//! reference that leaves the container must land on a container terminal —
//! but the count rule is per stream. In func bodies the miss is reported
//! however often the name is written (U13): two funcs joining the same
//! undeclared spelling have invented a net, not declared one (`pwr -> DC`
//! with no `DC` pin; `VSW` in both LDO funcs with no `VSW` pin or label),
//! and a missed func reference has no backstop anywhere else. At a module's
//! top level a label tagged at two or more endpoints is the via-label idiom
//! — the same spelling on both ends is the net, no middle wire — so it
//! stays silent and the net layer judges the net (the E3137 division of
//! labor); a single top-level endpoint (a stub with nothing at the other
//! end) still reports.
//!
//! A name referenced only as a method-call receiver or argument (an
//! inline-constructed instance like `DC.LDO(...) ld` then `ld.ldrop(...)`) is
//! an instance, not a wire, and does not trigger. A name that resolves to a
//! real instance by the time the component finished parsing (declared in a
//! sibling func or a conditional block that registers in the component, not
//! the func) is likewise not a dangling label.

use super::ledger::{self, LedgerAction, LedgerEntry, LedgerKind};
use super::{CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, ValidationCheck};

use crate::semantic::basic::mc_ref::McRef;
use crate::semantic::basic::mc_opd::McOpd;
use crate::semantic::basic::mc_param::McParamValue;
use crate::semantic::basic::mc_phrase::McPhrase;
use crate::semantic::mc_func::{HasFindInst, McFunctions, ShapeCtx};
use crate::semantic::mc_inst::McInstance;
use crate::semantic::pwrid::DeclaredFaces;

pub struct FloatingLabelCheck;

impl ValidationCheck for FloatingLabelCheck {
    fn name(&self) -> &'static str {
        "floating_label"
    }
    fn phase(&self) -> CheckPhase {
        CheckPhase::PostParse
    }
    fn default_severity(&self) -> CheckSeverity {
        CheckSeverity::Warning
    }

    fn run_post_parse(&self, acc: &mut CheckAccumulator) {
        check_floating_labels(acc);
    }
}

/// Emit E3136 for every candidate name of its owner that lands on no declared
/// terminal — a component or a module (§1.6 ①: the consumption side
/// extends to modules; module funcs register candidates through the shared
/// func-body context, module top-level body through `McModule.floating_candidates`).
fn check_floating_labels(acc: &mut CheckAccumulator) {
    let comps = crate::definition_space().workspace_components();
    for (sn, comp) in comps.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        check_owner_floating_labels(
            acc,
            "Component",
            &comp.name.to_string(),
            &uri,
            &DeclaredFaces::of_pins(&comp.pins),
            &comp.funcs,
            &[],
            &[],
            |name| comp.find_inst(name),
        );
    }
    let mods = crate::definition_space().workspace_modules();
    for (sn, module) in mods.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        check_owner_floating_labels(
            acc,
            "Module",
            &module.name.to_string(),
            &uri,
            &DeclaredFaces::of_module(&module.pi),
            &module.funcs,
            &module.stmts,
            &module.floating_candidates,
            |name| module.find_inst(name),
        );
    }
}

/// Shared E3136 pass over one owner's func bodies (plus, for modules, the
/// top-level body statements and their candidates).
#[allow(clippy::too_many_arguments)]
fn check_owner_floating_labels<F>(
    acc: &mut CheckAccumulator,
    owner_kind: &str,
    owner_name: &str,
    uri: &str,
    declared: &DeclaredFaces,
    funcs: &McFunctions,
    top_stmts: &[McPhrase],
    top_candidates: &[(String, u32, u32)],
    find_inst: F,
) where
    F: Fn(&str) -> Option<McInstance>,
{
    // Gather candidate names (name → first occurrence pos/len) from every
    // func plus the module top-level body. A name can be recorded by several
    // funcs (each hits the fallback independently before a label exists); keep
    // the earliest span.
    let mut candidates: std::collections::BTreeMap<String, (u32, u32)> =
        std::collections::BTreeMap::new();
    for func in funcs.iter() {
        for (name, pos, len) in &func.floating_candidates {
            candidates
                .entry(name.clone())
                .or_insert_with(|| (*pos, *len));
        }
    }
    for (name, pos, len) in top_candidates {
        candidates
            .entry(name.clone())
            .or_insert_with(|| (*pos, *len));
    }
    if candidates.is_empty() {
        return;
    }

    // Closure formals (`=> |ports| { ... }`) bind their names inside the
    // closure, exactly like a func's params bind inside the func (U300 M2).
    // A formal spelling showing up in the candidates is a bound face of the
    // body it scopes, never a dangling label.
    let bound_formals = collect_closure_formals(funcs, top_stmts);

    for (name, (pos, len)) in candidates {
        if bound_formals.contains(&name) {
            continue;
        }
        // A name the **owner itself declares** is an identity — its own
        // `pins.pwr` rows' `::DC` faces for a component, its rail / power-port /
        // `conduit` declarations for a module — so a reference to it is not a
        // dangling typo (no E3136, no Wire ledger row).
        //
        // World-axioms §1 A1: only a declaration confers identity — never a
        // spelling. The comparison is exact (U49), so no case folding is
        // applied to either side.
        if declared.declares(&name) {
            continue;
        }

        // Declared somewhere by the time the owner finished parsing — a real
        // instance (pin / port / param / inst / component / func), not a
        // dangling label. Func-local declares were already excluded during
        // the body parse; this covers declarations in sibling funcs or
        // conditional blocks that only become visible after this func's
        // body was parsed.
        if let Some(inst) = find_inst(&name) {
            if !matches!(inst, McInstance::Label(_)) {
                continue;
            }
        }

        // Count references across all funcs (top-level stmts + conditional
        // blocks) and, for modules, the top-level body, keeping the two
        // streams separate — the verdict differs by stream. A name used as a
        // call receiver or argument (`ld.ldrop(VSW, ...)`) is an instance
        // reference, not a wire, so it neither triggers nor adds to the wire
        // count. Every other reference is a net endpoint — and the name
        // reached this point because it lands on no container terminal.
        let mut func_counts = RefCounts::default();
        for func in funcs.iter() {
            for stmt in &func.stmts {
                count_refs(stmt, &name, &mut func_counts, true);
            }
            for cond in &func.conds {
                for block in &cond.if_blocks {
                    for stmt in &block.stmts {
                        count_refs(stmt, &name, &mut func_counts, true);
                    }
                }
                for stmt in &cond.else_stmts {
                    count_refs(stmt, &name, &mut func_counts, true);
                }
            }
        }
        let mut top_counts = RefCounts::default();
        for stmt in top_stmts {
            count_refs(stmt, &name, &mut top_counts, true);
        }
        // Failure ledger (observation-only): the action carries the verdict
        // this name gets below, so the count stays pure attribution.
        let refs = func_counts.endpoint + top_counts.endpoint;
        // The usage-count matrix (usage-count-policy-design.md §1, U154 ruling
        // 2026-09-21): func bodies and a module's top level share one verdict —
        // the count is a property of the name in its owner, not of the stream
        // that wrote it, and the net joins the spellings across funcs and the
        // top level all the same. Exactly one endpoint reference is the single
        // stub — the typo signal. Two or more are the via-label idiom (the
        // same spelling on both ends is the net, no middle wire): silent, and
        // the net layer judges the net (the E3137 division of labor). This
        // retires the U13 func-body counter-blindness (2026-09-16); the
        // accepted face is that two funcs writing the same misspelling share a
        // silently invented net. `other` stays joint: an instance-style use
        // anywhere keeps the name out of the wire verdict.
        let other = func_counts.other + top_counts.other;
        let endpoint = func_counts.endpoint + top_counts.endpoint;
        let reported = endpoint == 1 && other == 0;
        let action = if reported {
            LedgerAction::Warning
        } else {
            LedgerAction::Silent
        };
        ledger::record(
            LedgerEntry::new(LedgerKind::Wire, name.clone(), owner_name.to_string())
                .with_action(action)
                .with_uri(uri.to_string())
                .with_span(pos, len)
                .with_refs(refs),
        );

        if !reported {
            continue;
        }

        let (where_clause, scope_kinds) = if owner_kind == "Module" {
            (
                "a module body or function",
                "no declared port, interface, or instance",
            )
        } else {
            (
                "a function body",
                "no declared pin, interface, parameter member, or func-local instance",
            )
        };
        acc.push(CheckResult {
            check_name: "floating_label",
            severity: CheckSeverity::Warning,
            uri: Some(uri.to_string()),
            span: Some((pos as usize)..((pos + len) as usize)),
            message: format!(
                "{owner_kind} '{owner_name}': '{name}' in {where_clause} resolves to {scope_kinds} \
                 — floating net label. Declare it (a local instance, a pin, or a port) or fix \
                 the name."
            ),
            code: crate::errcodes::FUNC_FLOATING_LABEL,
        });
    }
}

// Reference-count walker over parsed func bodies

/// Gather every closure formal name (`=> |ports| { ... }`) reachable in the
/// owner's phrase trees — func bodies plus, for modules, the top-level body.
fn collect_closure_formals(funcs: &McFunctions, top_stmts: &[McPhrase]) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    for func in funcs.iter() {
        walk_closure_formals(&func.stmts, &mut out);
    }
    walk_closure_formals(top_stmts, &mut out);
    out
}

fn walk_closure_formals(phrases: &[McPhrase], out: &mut std::collections::HashSet<String>) {
    for p in phrases {
        walk_closure_formals_phrase(p, out);
    }
}

fn walk_closure_formals_phrase(p: &McPhrase, out: &mut std::collections::HashSet<String>) {
    match p {
        McPhrase::Closure(c) => {
            for d in c.params.iter() {
                if let Some(name) = d.get_primary_name() {
                    out.insert(name.to_string());
                }
            }
            walk_closure_formals(&c.body, out);
        }
        McPhrase::Series(items, _) | McPhrase::Parallel(items) | McPhrase::Multiple(items) => {
            walk_closure_formals(items, out)
        }
        McPhrase::Group(g) => walk_closure_formals(&g.opds, out),
        McPhrase::Transposed(i) | McPhrase::Reversed(i) => walk_closure_formals_phrase(i, out),
        McPhrase::FuncCall(fc) => {
            if let Some(caller) = &fc.caller {
                walk_closure_formals_phrase(caller, out);
            }
        }
        McPhrase::Member(i, _) => walk_closure_formals_phrase(i, out),
        _ => {}
    }
}

/// Reference counts for a candidate label name across a component's funcs.
#[derive(Default)]
pub(crate) struct RefCounts {
    /// References as a net endpoint (the label appears in a connection's
    /// endpoint tree) — the "is it really a wire" signal.
    pub(crate) endpoint: u32,
    /// References outside net endpoints: as a method-call receiver
    /// (`ld.ldrop`) or call argument (`ld.ldrop(VSW, ...)`), or as the tail of
    /// a member access (`X.Y`). An inline-constructed instance is only ever
    /// referenced this way, so it neither triggers E3136 nor contributes to a
    /// label's wire count.
    pub(crate) other: u32,
}

/// Count occurrences of `name` in a phrase tree.
///
/// A reference appears either as a label instance (created by the fallback, or
/// resolved to the same label on a later use) or as a bare id value passed to a
/// function call. `net_ctx` is false when the phrase was reached through a call
/// receiver or argument — its endpoints are instance references, not wires.
/// Shared with `gate::GateCheck` (E3137 single-use inline-net counting).
pub(crate) fn count_refs(phrase: &McPhrase, name: &str, c: &mut RefCounts, net_ctx: bool) {
    use McPhrase::*;
    match phrase {
        Endpoint(ep) => {
            let bucket = if net_ctx {
                &mut c.endpoint
            } else {
                &mut c.other
            };
            count_endpoint_refs(ep, name, bucket);
        }
        Series(items, _) | Parallel(items) | Multiple(items) => {
            for p in items {
                count_refs(p, name, c, net_ctx);
            }
        }
        Group(g) => {
            for p in &g.opds {
                count_refs(p, name, c, net_ctx);
            }
        }
        Transposed(inner) | Reversed(inner) => count_refs(inner, name, c, net_ctx),
        Closure(closure) => {
            for p in &closure.body {
                count_refs(p, name, c, net_ctx);
            }
        }
        FuncCall(fc) => {
            // The receiver is an instance reference (an inline-constructed
            // instance like `ld` is only ever seen here), not a wire.
            if let Some(caller) = &fc.caller {
                count_refs(caller, name, c, false);
            }
            // Call arguments are the connection endpoints of the called
            // interface: `ld.ldrop(VSW, ...)` wires VSW into ldrop, and
            // `Cap([(R108 - q.g) + R109 + C105, VSS])` nets the expression.
            for p in &fc.params {
                count_param_value_refs(p, name, &mut c.endpoint);
            }
        }
        Member(inner, ep) => {
            count_refs(inner, name, c, net_ctx);
            count_endpoint_refs(ep, name, &mut c.other);
        }
        Lead(_) => {}
    }
}

/// Count name matches inside an endpoint tree (`leaves` walks every branch of a
/// list / node junction, so each syntactic reference is counted once).
///
/// Reference face only (U308 ruling B): the question is how often a name is
/// **written**, not what the spelling evaluates to, so this site answers from
/// `McRef` and needs no `ShapeCtx`.
fn count_endpoint_refs(ep: &McRef, name: &str, count: &mut u32) {
    for r in ep.leaves() {
        if inst_name_matches(&r.base, name) {
            *count += 1;
        }
    }
}

/// A net endpoint references `name` if it is a bare label of that name or a
/// bus of that name. Resolved pins / params / components / interfaces carry
/// their own variants and never match a candidate's bare name.
fn inst_name_matches(inst: &McInstance, name: &str) -> bool {
    match inst {
        McInstance::Label(s) => s == name,
        McInstance::Bus(b) => b.name == name,
        _ => false,
    }
}

/// Count name matches inside a function-call parameter value. Bare ids are
/// stored as [`McOpd::Id`] without symbol resolution, so a later use of an
/// already-existing label still reads as the plain name.
fn count_param_value_refs(pv: &McParamValue, name: &str, count: &mut u32) {
    match pv {
        McParamValue::Ids(ids) => {
            if ids.to_string() == name {
                *count += 1;
            }
        }
        McParamValue::Opd(opd) => match opd {
            McOpd::Id(ids) | McOpd::This(ids) | McOpd::Pins(ids) => {
                if ids.to_string() == name {
                    *count += 1;
                }
            }
            McOpd::Uscore => {}
        },
        McParamValue::Phrase(p) => {
            // A phrase-valued call argument (`Cap([(R108 - q.g) + R109 + C105,
            // VSS])`): the labels inside it are net endpoints of the argument.
            let mut tmp = RefCounts::default();
            count_refs(p, name, &mut tmp, true);
            *count += tmp.endpoint + tmp.other;
        }
        McParamValue::Set(values) => {
            for v in values {
                count_param_value_refs(v, name, count);
            }
        }
        _ => {}
    }
}
