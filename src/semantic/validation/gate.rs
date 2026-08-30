// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! resolve-gate §1.3/§1.4 (relax-everything): component-finish recheck of inlined
//! ghost-bus candidates.
//!
//! The Phase 1 entry gate (resolve-gate-design.md §1.3/§1.4) previously made a
//! structured dot access whose base resolves to no declared instance a hard
//! error: the phantom ghost-bus was suppressed, the statement dropped, and the
//! finish recheck emitted E3182. Since the relax-everything decision the ghost-bus is
//! kept and inlined at every gate site in mc_phrase.rs — the reference either
//!   * passes: the base IS a declared instance name (an instance, a FuncCall
//!     caller label such as `dTrigger`/`PL`, or a func-local inst such as
//!     `timer`/`q`) — the ghost-bus defers to §3; or
//!   * true misses: the base is declared nowhere — the ghost-bus is inlined
//!     (the statement keeps its net) and the reference is registered as a
//!     [`GateCandidate`].
//!
//! This check is the finish-time half of the true-miss path. It rechecks every
//! registered candidate against the scope as it exists after the component /
//! module finished parsing:
//!   * a candidate whose base became declared by then (declared later in the
//!     same func, in a sibling func's insts/seen_callers, or at the
//!     owner-component/module level) is a late-declared reference — §1.3
//!     `resolved_late`, balanced in the failure ledger, no diagnostic; and
//!   * a candidate whose base is still declared nowhere is an inline ghost-net:
//!     if it is referenced exactly once it is almost certainly a typo or a
//!     forgotten declaration → E3137 (SINGLE_USE_INLINE_NET, Warning); if it is
//!     referenced twice or more it is a shared net and left alone (the net
//!     layer decides — e.g. R03 catches a net that joins a supply and a ground).

use super::floating::{count_refs, RefCounts};
use super::ledger;
use super::{CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, ValidationCheck};

use crate::semantic::basic::mc_phrase::McPhrase;
use crate::semantic::mc_func::{GateCandidate, HasFindInst, McFunctions};

pub struct GateCheck;

impl ValidationCheck for GateCheck {
    fn name(&self) -> &'static str {
        "inline_ghost_net"
    }
    fn phase(&self) -> CheckPhase {
        CheckPhase::PostParse
    }
    fn default_severity(&self) -> CheckSeverity {
        CheckSeverity::Warning
    }

    fn run_post_parse(&self, acc: &mut CheckAccumulator) {
        check_gate_candidates(acc);
    }
}

/// Run the finish recheck over every component and module in the workspace.
fn check_gate_candidates(acc: &mut CheckAccumulator) {
    let comps = &crate::db::cmie::tables::WORKSPACE.components;
    for entry in comps.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp = entry.value();
        // Func-level candidates (a component body has no net stmts, so no
        // component-level candidates exist).
        let candidates: Vec<GateCandidate> = comp
            .funcs
            .iter()
            .flat_map(|f| f.gate_candidates.iter().cloned())
            .collect();
        if candidates.is_empty() {
            continue;
        }
        let owner: &dyn HasFindInst = &**comp;
        recheck_owner(
            acc,
            &uri,
            &comp.name.to_string(),
            owner,
            &[],
            candidates,
            &comp.funcs,
            &[],
        );
    }

    let mods = &crate::db::cmie::tables::WORKSPACE.modules;
    for entry in mods.iter() {
        let uri = entry.key().uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let module = entry.value();
        // Module-level candidates plus any func-level candidates inside the
        // module's own funcs.
        let mut candidates: Vec<GateCandidate> = module.gate_candidates.clone();
        candidates.extend(
            module
                .funcs
                .iter()
                .flat_map(|f| f.gate_candidates.iter().cloned()),
        );
        if candidates.is_empty() {
            continue;
        }
        let owner: &dyn HasFindInst = &**module;
        recheck_owner(
            acc,
            &uri,
            &module.name.to_string(),
            owner,
            &module.seen_callers,
            candidates,
            &module.funcs,
            &module.stmts,
        );
    }
}

/// Recheck a candidate list against the scope as it exists at component/
/// module finish. Late-declared candidates are balanced in the ledger
/// (`resolved_late`); a still-unresolved candidate is an inline ghost-net that
/// warns E3137 only when referenced exactly once across the owner's funcs /
/// top-level body (a shared net is left alone).
fn recheck_owner(
    acc: &mut CheckAccumulator,
    uri: &str,
    owner_label: &str,
    owner: &dyn HasFindInst,
    owner_seen: &[String],
    candidates: Vec<GateCandidate>,
    funcs: &McFunctions,
    top_stmts: &[McPhrase],
) {
    for cand in candidates {
        if base_declared_by_finish(owner, owner_seen, funcs, &cand.base) {
            // Late-declared (§1.3 `resolved_late`): the reference resolved after
            // all — balanced in the ledger, no diagnostic.
            ledger::mark_resolved_late();
            continue;
        }
        // Implicit power-rail bases (`VCC`, `GND`, …) are conventional rails the
        // net layer already recognizes via is_supply_name / is_ground_name
        // without a declaration — not a dangling inline net (mirror E3136).
        if crate::instant::insttab::is_supply_name(&cand.base)
            || crate::instant::insttab::is_ground_name(&cand.base)
        {
            continue;
        }
        // Count references to the ghost-bus (its `McBus.name` is the base, so
        // matching the base name is exact) across every func body plus, for
        // modules, the top-level body. Single-use → E3137; referenced twice or
        // more it is a shared net and left alone (e.g. RS485.A series reuse).
        let mut counts = RefCounts::default();
        for func in funcs.iter() {
            for stmt in &func.stmts {
                count_refs(stmt, &cand.base, &mut counts, true);
            }
            for cond in &func.conds {
                for block in &cond.if_blocks {
                    for stmt in &block.stmts {
                        count_refs(stmt, &cand.base, &mut counts, true);
                    }
                }
                for stmt in &cond.else_stmts {
                    count_refs(stmt, &cand.base, &mut counts, true);
                }
            }
        }
        for stmt in top_stmts {
            count_refs(stmt, &cand.base, &mut counts, true);
        }
        if counts.endpoint != 1 || counts.other != 0 {
            continue;
        }
        acc.push(CheckResult {
            check_name: "inline_ghost_net",
            severity: CheckSeverity::Warning,
            uri: Some(uri.to_string()),
            span: Some((cand.pos as usize)..((cand.pos + cand.len) as usize)),
            message: format!(
                "The base name '{}' of the structured reference '{}' resolves to no \
                 declared instance in {} and the inline net is referenced only once; \
                 declare it or fix the name.",
                cand.base, cand.form, owner_label
            ),
            code: crate::errcodes::SINGLE_USE_INLINE_NET,
        });
    }
}

/// Is `base` a declared instance name anywhere in the owner's scope by the
/// time it finished parsing?
///
/// Sources, in order: the owner scope itself (component params/pins/attrs/
/// insts/funcs, or module scope), the owner's parse-time seen_callers set
/// (FuncCall caller labels that never enter insts, e.g. `PL`), and every func
/// of the owner — its local insts (late declares and direct body DECLAREs such
/// as `timer`/`q`) and its seen_callers.
fn base_declared_by_finish(
    owner: &dyn HasFindInst,
    owner_seen: &[String],
    funcs: &McFunctions,
    base: &str,
) -> bool {
    if owner.find_inst(base).is_some() {
        return true;
    }
    if owner_seen.iter().any(|s| s == base) {
        return true;
    }
    funcs
        .iter()
        .any(|f| f.insts.contains(base) || f.seen_callers.iter().any(|s| s == base))
}
