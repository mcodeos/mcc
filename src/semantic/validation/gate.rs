// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! resolve-gate §1.3/§1.4: component-finish recheck of ghost-bus true-miss
//! candidates.
//!
//! Phase 1 entry gate (the error gate): at each silent ghost-bus fallback site in
//! mc_phrase.rs, a two-segment dot access whose base resolves to no declared
//! instance in scope either
//!   * pass: the base IS a declared instance name (an instance, a
//!     FuncCall caller label such as `dTrigger`/`PL`, or a func-local inst such
//!     as `timer`/`q`) — the ghost-bus is kept and the reference defers to §3;
//!     or
//!   * true miss: the base is declared nowhere — the phantom ghost-bus is
//!     suppressed (the statement produces no net), the reference is registered
//!     as a [`GateCandidate`], and the statement is dropped.
//!
//! This check is the finish-time half of the true-miss path. It rechecks every
//! registered candidate against the scope as it exists after the component /
//! module finished parsing:
//!   * a candidate whose base became declared by then (declared later in the
//!     same func, in a sibling func's insts/seen_callers, or at the
//!     owner-component/module level) is a late-declared reference — §1.3
//!     `resolved_late`, balanced in the failure ledger, no error; and
//!   * a candidate whose base is still declared nowhere is a genuine
//!     instance-reference miss → E3182 (INSTANCE_REF_UNDECLARED).

use super::ledger;
use super::{CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, ValidationCheck};

use crate::semantic::mc_func::{GateCandidate, HasFindInst, McFunctions};

pub struct GateCheck;

impl ValidationCheck for GateCheck {
    fn name(&self) -> &'static str {
        "instance_ref_gate"
    }
    fn phase(&self) -> CheckPhase {
        CheckPhase::PostParse
    }
    fn default_severity(&self) -> CheckSeverity {
        CheckSeverity::Error
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
        );
    }
}

/// Recheck a candidate list against the scope as it exists at component/
/// module finish. Late-declared candidates are balanced in the ledger
/// (`resolved_late`); still-unresolved candidates become E3182 errors.
fn recheck_owner(
    acc: &mut CheckAccumulator,
    uri: &str,
    owner_label: &str,
    owner: &dyn HasFindInst,
    owner_seen: &[String],
    candidates: Vec<GateCandidate>,
    funcs: &McFunctions,
) {
    for cand in candidates {
        if base_declared_by_finish(owner, owner_seen, funcs, &cand.base) {
            // Late-declared (§1.3 `resolved_late`): the parse-time UnresolvedRef
            // row is balanced — the reference resolved after all, no error.
            ledger::mark_resolved_late();
            continue;
        }
        acc.push(CheckResult {
            check_name: "instance_ref_gate",
            severity: CheckSeverity::Error,
            uri: Some(uri.to_string()),
            span: Some((cand.pos as usize)..((cand.pos + cand.len) as usize)),
            message: format!(
                "The base name '{}' of the structured reference '{}' resolves to no \
                 declared instance in {}; declare it or fix the name.",
                cand.base, cand.form, owner_label
            ),
            code: crate::errcodes::INSTANCE_REF_UNDECLARED,
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
