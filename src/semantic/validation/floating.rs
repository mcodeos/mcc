// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Floating-label validation for function bodies.
//!
//! E3136 (FUNC_FLOATING_LABEL): a bare identifier in a func body net
//! statement that resolves to nothing declared (pin, interface, parameter
//! member, or func-local instance) becomes a one-shot dangling net label
//! (mc_phrase.rs single-segment fallback). If the name is referenced exactly
//! once across all funcs of the component it has no peer to join a net with —
//! almost always a typo or a forgotten declaration (e.g. `pwr -> DC` where the
//! component has no `DC` pin or interface).
//!
//! Names referenced twice or more are a shared net — two funcs joining the
//! same rail by label (e.g. `VSW` feeding both LDO2 and LDO3) — and are left
//! alone. A name referenced only as a method-call receiver or argument (an
//! inline-constructed instance like `DC.LDO(...) ld` then `ld.ldrop(...)`) is
//! an instance, not a wire, and does not trigger. A name that resolves to a
//! real instance by the time the component finished parsing (declared in a
//! sibling func or a conditional block that registers in the component, not
//! the func) is likewise not a dangling label.

use super::ledger::{self, LedgerAction, LedgerEntry, LedgerKind};
use super::{CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, ValidationCheck};

use crate::semantic::basic::mc_endpoint::{McEndpoint, McInstanceRef};
use crate::semantic::basic::mc_opd::McOpd;
use crate::semantic::basic::mc_param::McParamValue;
use crate::semantic::basic::mc_phrase::McPhrase;
use crate::semantic::mc_func::{HasFindInst, McFunctions};
use crate::semantic::mc_inst::McInstance;

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

/// Emit E3136 for every candidate name referenced exactly once across all
/// funcs of its owner — a component or a module (§1.6 ①: the consumption side
/// extends to modules; module funcs register candidates through the shared
/// func-body context, module top-level body through `McModule.floating_candidates`).
fn check_floating_labels(acc: &mut CheckAccumulator) {
    for entry in crate::db::cmie::tables::WORKSPACE.components.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();
        check_owner_floating_labels(
            acc,
            "Component",
            &comp.name.to_string(),
            &uri,
            &comp.funcs,
            &[],
            &[],
            |name| comp.find_inst(name),
        );
    }
    for entry in crate::db::cmie::tables::WORKSPACE.modules.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let module = entry.value();
        check_owner_floating_labels(
            acc,
            "Module",
            &module.name.to_string(),
            &uri,
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

    for (name, (pos, len)) in candidates {
        // Implicit power-rail labels (`VCC`, `GND`, `V3V3`, …) are
        // conventional rails the net layer already recognizes via
        // is_supply_name / is_ground_name (insttab.rs) without a local
        // declaration. A reference to an implicit rail is not a dangling
        // typo — skip it entirely (no E3136, no Wire ledger row).
        let upper = name.to_uppercase();
        if crate::instant::insttab::is_supply_name(&upper)
            || crate::instant::insttab::is_ground_name(&upper)
        {
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
        // blocks) and, for modules, the top-level body. A floating label is
        // one referenced exactly once and only as a net endpoint — it has no
        // peer to join a net with. A name used as a call receiver or argument
        // (`ld.ldrop(VSW, ...)`) is an instance reference, not a wire, so it
        // neither triggers nor adds to the wire count.
        let mut counts = RefCounts::default();
        for func in funcs.iter() {
            for stmt in &func.stmts {
                count_refs(stmt, &name, &mut counts, true);
            }
            for cond in &func.conds {
                for block in &cond.if_blocks {
                    for stmt in &block.stmts {
                        count_refs(stmt, &name, &mut counts, true);
                    }
                }
                for stmt in &cond.else_stmts {
                    count_refs(stmt, &name, &mut counts, true);
                }
            }
        }
        for stmt in top_stmts {
            count_refs(stmt, &name, &mut counts, true);
        }
        // ── Failure ledger (observation-only) ────────────────────────────
        // Every name that survived the owner-finish recheck is a floating
        // label: referenced once it is a dangling net (E3136 below);
        // referenced 2+ it is a shared rail that also resolves to nothing
        // declared. Both are recorded so the miss is attributable.
        let refs = counts.endpoint;
        let action = if refs == 1 && counts.other == 0 {
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

        if counts.endpoint != 1 || counts.other != 0 {
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
                 — floating net label. It is referenced only once and connects to nothing else; \
                 declare it or fix the name."
            ),
            code: crate::errcodes::FUNC_FLOATING_LABEL,
        });
    }
}

// ============================================================================
// Reference-count walker over parsed func bodies
// ============================================================================

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
        Transposed(inner) => count_refs(inner, name, c, net_ctx),
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
        Lead => {}
    }
}

/// Count name matches inside an endpoint tree (flatten handles list / node
/// junctions, so each syntactic reference is counted once).
fn count_endpoint_refs(ep: &McEndpoint, name: &str, count: &mut u32) {
    for single in ep.flatten() {
        if let McEndpoint::Single(McInstanceRef { base, .. }) = single {
            if inst_name_matches(&base, name) {
                *count += 1;
            }
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
