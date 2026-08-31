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
//!
//! ## §11.4: E3137 is a naming-layer UX heuristic, not the materialized-net scan
//!
//! mcode-architecture-circuit-model.md §5 offered two futures for E3137 —
//! "re-define as a materialized-net scan (GAP2 global 0-pin)" or "explicitly
//! demote to a naming-layer UX heuristic". This check takes the second: E3137
//! MUST fire on the plain `mcc_build` (pass1 diagnostic) path — it is the only
//! report a single-use typo gets without a net build, and the suppression /
//! ledger tests (`tests/ignore_warnings.rs`, `tests/failure_ledger.rs`,
//! `four_gate_forms_each_warn_single_use`) depend on it there. Materialization
//! is pass2; a pass2-only E3137 would silently drop the single-use warning for
//! anyone who runs `mcc build` but not `mcc check --nets`. The materialized-net
//! fact it approximates is instead covered by GAP2 (E4057, `flatten_nets` in
//! insttab.rs): a net statement whose endpoints resolve to **0** physical pins
//! reports NET_DROPPED_STATEMENT there. The domains are disjoint — a 0-pin net
//! is E4057; a stub net with ≥1 kept point whose ghost is referenced once is
//! E3137 — so no net is double-reported. E3136 remains the bare-Wire boundary
//! (`floating.rs`): E3136 = naked undeclared name resolved as a Wire, E3137 =
//! structured ghost (bus/interface member miss) referenced uniquely.
//!
//! ## §11.4 (GAP3): physical-position preemption — the domain audit
//!
//! mcode-architecture-circuit-model.md §9.3.3 rates GAP3 lowest-priority /
//! deferrable (design "can defer", heavily overlapping 4051). The Phase 2.3 audit confirms
//! that empirically: the flat-layer "two different declarations materialize to
//! the same physical pin id" fact has **no well-formed-MCode trigger**, because
//! every collision is absorbed by the pass1 declaration layer before flatten:
//!   * same-scope instance names → E5151 (`check_duplicate_instances`,
//!     validation/ports.rs), dense for same-name / vector-member / port-vs-
//!     component collisions;
//!   * `insts` name-keyed dedup → only ONE of two same-named instances survives
//!     into `McModuleInst.components`, so flatten never sees the second
//!     registration;
//!   * flat paths are scope-unique (`module.instance.pinname`) — two different
//!     scopes cannot produce the same path.
//! The check is implemented as E4062 PIN_OCCUPIED_BY_DECLARATION at the flat
//! registration site (`InstTable::register`, insttab.rs): it fires only when
//! BOTH the existing and the new registration are structural
//! (Module/Component/Pin) AND their declaration classes differ. That gate is
//! mathematically disjoint from the neighbors, so no double-report:
//!   * E5151 (pass1) = same-scope instance NAMES; GAP3 = same flat pin PATH.
//!   * 4051 NET_MERGED_SHORT (build side, visit.rs) = ≥2 point paths inside ONE
//!     connection resolve to the same id — a net-layer merge fact; GAP3 = pin
//!     DECLARATION occupancy, never a connection.
//!   * 4053 SORT_HAZARD (pass1, instref.rs) = bus pin-group member→pin mapping
//!     non-monotonic; GAP3 = a pin id claimed by two declarations.
//! Probe finding (2026-08-31): the only genuinely-silent flat-layer uniqueness
//! hole in valid syntax is the pin **function-name** namespace — `pins =
//! [1 = DUP, 2 = DUP]` (duplicate function name) or a function name equal to
//! another pin's number both resolve silently. That is a pass1 pin-declaration
//! uniqueness gap (name slot, not pin id), orthogonal to GAP3's defined domain;
//! it is left to a future declaration-layer check, consistent with GAP3's
//! "can defer" verdict.

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
