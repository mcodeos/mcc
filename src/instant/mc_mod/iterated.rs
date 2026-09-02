// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Iterated call expansion
//!
//! - `check_and_expand_iterated_call`  —— Detect iterated calls whose caller is a Vector (e.g. `cx[1:2]`)
//! - `resolve_indexed_params`          —— Expand `Set` etc. in parameters by the iteration index

use super::funccall::FuncCallInst;
use super::InstantiationBuilder;
use crate::instant::mc_net::InstError;
use crate::instant::provenance::ExpansionKind;
use crate::semantic::basic::mc_bus::McBus;
use crate::semantic::basic::mc_endpoint::McEndpoint;
use crate::semantic::basic::mc_opd::McOpd;
use crate::semantic::basic::mc_param::McParamValue;
use crate::semantic::basic::mc_phrase::McPhrase;
use crate::semantic::mc_inst::McInstance;
use crate::McIds;

impl InstantiationBuilder {
    /// Detect and process iterated calls
    ///
    /// When the FuncCall caller is a Vector (e.g. `cx[1:2]`),
    /// expand the call into multiple independent calls.
    ///
    /// # Example
    /// ```text
    /// cx[1:2].Cap(XTAL.X<1:2>, gnd)
    /// → cx1.Cap(XTAL.X<1:2>, gnd) + cx2.Cap(XTAL.X<1:2>, gnd)
    /// → each member receives the full arg list, broadcast unchanged (§5 item 17);
    ///   creates two independent CAP component instances
    /// ```
    ///
    /// # Return value
    /// - `Some(result)` — iterated call, expanded
    /// - `None` — not an iterated call, fall through to the normal flow
    pub(super) fn check_and_expand_iterated_call(
        &mut self,
        caller: &Option<Box<McPhrase>>,
        func_name: &McIds,
        params: &[McParamValue],
        _left: &[McBus],
        right: &[McBus],
    ) -> Result<Option<FuncCallInst>, InstError> {
        // Check whether caller is McPhrase::Series and contains Parallel
        let caller_phrase = match caller {
            Some(phrase) => phrase.as_ref(),
            None => {
                // ── Iter-6.S5.2-diag ──
                return Ok(None);
            }
        };

        // ── Iter-11.4: lane-structured List receiver (§11.3 ③) ─────────────
        // Phase 1.3 pass1 vector resolution turns `c[1:2]` into
        // `Endpoint(List([Single(c1), Single(c2), ...]))` — one lane per ordered
        // member. Iterate the lanes directly; no `McIds::from(name).expand()`
        // string re-parse (the producer already carried the member set).
        let lanes_owned: Vec<McPhrase>;
        if let McPhrase::Endpoint(McEndpoint::List(eps)) = caller_phrase {
            lanes_owned = eps
                .iter()
                .map(|ep| McPhrase::Endpoint(ep.clone()))
                .collect();
            if lanes_owned.is_empty() {
                return Ok(Some(FuncCallInst::PassThrough));
            }
        } else {
            lanes_owned = Vec::new();
        }

        // Caller must be McPhrase::Series whose first element is Parallel —
        // or a lane-structured List receiver (taken above). The bare-bracket
        // `McIds::from(name).expand()` synthesis (Iter-1.3) is gone: pass1's
        // vector arm (§11.3 ③) resolves declared arrays to
        // `Endpoint::List`, so no single-Endpoint caller reaches here with a
        // bracket name; an undeclared array base falls to the scalar-miss
        // decision (E3136/Wire twin) like any other undeclared name.
        let items: &Vec<McPhrase> = if !lanes_owned.is_empty() {
            &lanes_owned
        } else {
            let phrases = match caller_phrase {
                McPhrase::Series(phrases, _) => phrases,
                _ => {
                    // ── Iter-6.S5.2-diag ──
                    return Ok(None);
                }
            };

            let first_phrase = match phrases.first() {
                Some(p) => p,
                None => return Ok(None),
            };

            match first_phrase {
                McPhrase::Parallel(items) => items,
                _ => {
                    // ── Iter-6.S5.2-diag ──
                    return Ok(None);
                }
            }
        };

        let count = items.len();
        // ── Iter-6.S5.2-diag ──
        if count == 0 {
            return Ok(Some(FuncCallInst::PassThrough));
        }

        // ── §11.4 GAP1: member-set alignment, before the broadcast ──────
        // Compare the iterated receiver's member count against each
        // multi-member slice lane in the arg list (`cap[1:2].Cap([XTAL.X[1:2], gnd])`:
        // {cap1,cap2} vs {XTAL.X1,XTAL.X2} — one-to-one correspondence needs
        // equal widths). Fires once per mismatched slice; scalar lanes
        // broadcast (§5 item 17) and the arg-list-vs-formals lane-count
        // mismatch stays with the existing E4180 downstream. The zip itself
        // clamps to the receiver width so GAP1 is the single report.
        self.emit_gap1_member_set_mismatch(params, count);

        // ── Expansion provenance: Iterated (covers the per-item loop,
        //    §4.1-A4 / B8). Per-item constructions / func calls begin their
        //    own records nested under this one. ──
        let call_site = self.current_call_site();
        let eidx = self.expansion.begin(
            ExpansionKind::Iterated,
            None,
            func_name.to_string(),
            call_site,
            None,
        );

        let mut all_components = Vec::new();
        let mut all_connections = Vec::new();
        // §3.3: true once any item dispatched as a method onto a materialized
        // member. `instantiate_instance_method` returns PassThrough by design
        // (its products are side effects on `self`), so an empty Components
        // result is still a successful per-member dispatch — it must not be
        // reported as "all pass-through, iterated connection dropped".
        let mut dispatched_any = false;

        for (i, item) in items.iter().enumerate() {
            // 1. Process the caller of each item (recursive instantiation)
            if let Err(e) = self.process_stmt(item) {
                self.expansion.end(eidx);
                return Err(e);
            }

            // 2. Get item as the new left endpoints
            let item_right_pts = self.get_right_points_from_phrase(item)?;
            let item_left_elems: Vec<McBus> =
                item_right_pts.iter().map(|p| McBus::new(&p.path)).collect();

            // 3. Resolve indices in parameters (e.g. XTAL.X<1:2> expands to XTAL.X1, XTAL.X2)
            let resolved_params = Self::resolve_indexed_params(params, i, count);

            // 3.5 ── §3.3: per-member method dispatch ─────────────────────
            // Array receiver whose members are already-materialized instances
            // (`r[1:2]::RES(0)` then `r[1:2].Pullup([net,vcc])`): each item
            // (`U1.r1`) is a real instance. Dispatch the method on it rather
            // than feeding `instantiate_funccall` — a bare-call alias
            // (PULLUP/PULLDOWN→RES) would otherwise hijack the per-item call
            // and construct a phantom `r[1:2]` RES (§2.6 Table A, E3179).
            // `resolved_params` is already broadcast (every member gets the
            // full arg list, §5 item 17).
            let func_name_str = func_name.to_string();
            if let Some(inst_name) = Self::iterated_item_inst_name(item) {
                let member_func = self
                    .find_component(&inst_name)
                    .and_then(|c| c.def.funcs.find(&func_name_str).cloned())
                    .or_else(|| {
                        self.find_submodule(&inst_name)
                            .and_then(|m| m.def.funcs.find(&func_name_str).cloned())
                    });
                if let Some(func_def) = member_func {
                    let result = self.instantiate_instance_method(
                        &inst_name,
                        &func_def,
                        &resolved_params,
                        &item_left_elems,
                        right,
                    )?;
                    match result {
                        FuncCallInst::Components {
                            new_components,
                            new_connections,
                        } => {
                            all_components.extend(new_components);
                            all_connections.extend(new_connections);
                        }
                        FuncCallInst::SubModule {
                            inst,
                            new_connections,
                        } => {
                            self.add_submodule(inst);
                            all_connections.extend(new_connections);
                        }
                        // PassThrough is the normal method-dispatch result —
                        // run_component_method / run_submodule_method already
                        // added the body's products to `self` as side effects.
                        FuncCallInst::PassThrough => {}
                    }
                    dispatched_any = true;
                    continue;
                }
            }

            // 4. Call instantiate_funccall for each iterated item
            let result = match self.instantiate_funccall(
                func_name,
                &resolved_params,
                &item_left_elems,
                right,
                caller.as_deref(),
            ) {
                Ok(r) => r,
                Err(e) => {
                    self.expansion.end(eidx);
                    return Err(e);
                }
            };

            // ── Iter-6.S5.2-diag ──

            match result {
                FuncCallInst::Components {
                    new_components,
                    new_connections,
                } => {
                    all_components.extend(new_components);
                    all_connections.extend(new_connections);
                }
                FuncCallInst::SubModule {
                    inst,
                    new_connections,
                } => {
                    // Iterated calls can also create sub-modules (rare, but supported)
                    self.add_submodule(inst);
                    all_connections.extend(new_connections);
                }
                FuncCallInst::PassThrough => {
                    // Single iterated item degraded to pass-through (the per-item
                    // warning 944 is emitted by instantiate_funccall). Log the item
                    // index so the overall iterated call is traceable.
                    crate::db::diagnostic::diagnostic::dlog_trace(
                        944,
                        &format!(
                            "iterated: item #{i}/{count} of '{}' → pass-through (module='{}')",
                            func_name, self.name,
                        ),
                    );
                }
            }
        }

        self.expansion.end(eidx);
        // ── Iter-6.S5.2-diag ──

        if all_components.is_empty() && all_connections.is_empty() {
            if dispatched_any {
                // Per-member method dispatch succeeded (side-effect products);
                // an empty result must not degrade to "all pass-through".
                Ok(Some(FuncCallInst::Components {
                    new_components: Vec::new(),
                    new_connections: Vec::new(),
                }))
            } else {
                Ok(Some(FuncCallInst::PassThrough))
            }
        } else {
            Ok(Some(FuncCallInst::Components {
                new_components: all_components,
                new_connections: all_connections,
            }))
        }
    }

    /// Resolve index-related values in parameters
    ///
    /// Every scalar parameter value is **broadcast unchanged** to every
    /// iterated member (§5 item 17: `res[1:2].Pullup([net,vcc])` → res1, res2
    /// each get one `net - RES - vcc`). A **multi-member slice lane** in an
    /// arg list (`cap[1:2].Cap([XTAL.X[1:2], gnd])`) zips positionally
    /// against the receiver's members — at index i the slice collapses to its
    /// i-th member (c1↔XTAL.X1, c2↔XTAL.X2). The member set comes from
    /// `McIds::expand` (pipeline-③ producer); a slice shorter than the
    /// receiver clamps to its last member — the width mismatch is reported by
    /// GAP1 ([`Self::emit_gap1_member_set_mismatch`]) at the call level, so
    /// the downstream E4180 (arg-list lane count vs formals) stays quiet.
    ///
    /// # Parameters
    /// - `params` — the original parameter list
    /// - `index` — current iteration index
    /// - `total` — total iteration count (receiver member count)
    fn resolve_indexed_params(
        params: &[McParamValue],
        index: usize,
        total: usize,
    ) -> Vec<McParamValue> {
        params
            .iter()
            .map(|p| Self::zip_param_lane(p, index, total))
            .collect()
    }

    /// §11.4 GAP1: expand a vector-slice lane to its `index`-th member.
    ///
    /// Recurses into `Set` arg lists (the `[..]` square-vector form); a
    /// multi-member `Opd(Id)` / `Ids` lane whose slice width is ≥ 2 becomes
    /// its `index`-th expanded member. Clamps to the last member when the
    /// slice is narrower than the receiver (GAP1 already reported the width
    /// mismatch). Scalar lanes pass through unchanged (broadcast).
    fn zip_param_lane(param: &McParamValue, index: usize, total: usize) -> McParamValue {
        match param {
            McParamValue::Set(values) => McParamValue::Set(
                values
                    .iter()
                    .map(|v| Self::zip_param_lane(v, index, total))
                    .collect(),
            ),
            McParamValue::Opd(McOpd::Id(ids)) => Self::zip_slice_member(ids, index, total)
                .map(|m| McParamValue::Opd(McOpd::Id(McIds::from(m.as_str()))))
                .unwrap_or_else(|| param.clone()),
            McParamValue::Ids(ids) => Self::zip_slice_member(ids, index, total)
                .map(|m| McParamValue::Ids(McIds::from(m.as_str())))
                .unwrap_or_else(|| param.clone()),
            _ => param.clone(),
        }
    }

    /// Member of a slice at `index`, or `None` when the id is not a
    /// multi-member vector slice (scalar → broadcast unchanged).
    fn zip_slice_member(ids: &McIds, index: usize, total: usize) -> Option<String> {
        if total < 2 {
            return None;
        }
        let members = ids.expand();
        if members.len() < 2 {
            return None;
        }
        Some(members[index.min(members.len() - 1)].clone())
    }

    /// §11.4 GAP1: report receiver-vs-slice member-set mismatches.
    ///
    /// `cap[1:2].Cap([XTAL.X[1:3], gnd])` — receiver {cap1,cap2} (2) against
    /// the slice {XTAL.X1,XTAL.X2,XTAL.X3} (3): one-to-one correspondence
    /// needs equal widths. Fires once per distinct mismatched slice; scalar
    /// lanes (broadcast) and aligned slices stay quiet. Span from the current
    /// statement when available.
    fn emit_gap1_member_set_mismatch(&self, params: &[McParamValue], receiver_count: usize) {
        if receiver_count < 2 {
            return;
        }
        let mut slices: Vec<String> = Vec::new();
        for lane in params {
            Self::gap1_collect_slices(lane, receiver_count, &mut slices);
        }
        if slices.is_empty() {
            return;
        }
        let site = self.current_call_site();
        for display in &slices {
            let msg = crate::errcodes::format_msg(
                crate::errcodes::VECTOR_ZIP_WIDTH_MISMATCH,
                &[
                    &receiver_count.to_string(),
                    display as &dyn std::fmt::Display,
                ],
            );
            match &site {
                Some(spos) => crate::db::diagnostic::diagnostic::diagnostic_log_at(
                    crate::errcodes::VECTOR_ZIP_WIDTH_MISMATCH,
                    crate::db::diagnostic::diagnostic::DiagnosticLevel::Error,
                    spos.uri.clone(),
                    spos.offset,
                    0,
                    &msg,
                    &[],
                ),
                None => crate::db::diagnostic::diagnostic::diagnostic_log(
                    crate::errcodes::VECTOR_ZIP_WIDTH_MISMATCH,
                    crate::db::diagnostic::diagnostic::DiagnosticLevel::Error,
                    0,
                    0,
                    &msg,
                    &[],
                ),
            }
        }
    }

    /// Collect the display forms of every multi-member slice lane whose width
    /// differs from the receiver count (deduplicated).
    fn gap1_collect_slices(param: &McParamValue, receiver_count: usize, out: &mut Vec<String>) {
        match param {
            McParamValue::Set(values) => {
                for v in values {
                    Self::gap1_collect_slices(v, receiver_count, out);
                }
            }
            McParamValue::Opd(McOpd::Id(ids)) | McParamValue::Ids(ids) => {
                let members = ids.expand();
                if members.len() >= 2 && members.len() != receiver_count {
                    let display = ids.to_string();
                    if !out.contains(&display) {
                        out.push(display);
                    }
                }
            }
            _ => {}
        }
    }

    /// §3.3: extract the materialized instance name from an iterated item.
    ///
    /// A bare array caller (`r[1:2]`) is synthesized into per-member items as
    /// `Endpoint(Single(Label("U1.r1")))` (or a plain `Label`/`Bus`) — the
    /// full dotted name of the already-declared member instance, which the
    /// `#[...]` expansion preserved. Returns `None` for anything that isn't a
    /// plain named instance reference (construction callers etc.).
    fn iterated_item_inst_name(item: &McPhrase) -> Option<String> {
        match item {
            McPhrase::Endpoint(McEndpoint::Single(iref)) => match &iref.base {
                McInstance::Label(s) => Some(s.clone()),
                McInstance::Bus(b) if b.member.is_empty() => Some(b.name.clone()),
                // §11.3: a resolved member endpoint (find_inst hit at pass1
                // for module-scope declares) is the same instance — dispatch
                // the method onto its materialized name.
                McInstance::Component(c) => Some(c.name.to_string()),
                McInstance::Module(m) => Some(m.name.to_string()),
                _ => None,
            },
            McPhrase::Endpoint(McEndpoint::List(refs)) => refs.first().and_then(|ep| match ep {
                McEndpoint::Single(iref) => match &iref.base {
                    McInstance::Label(s) => Some(s.clone()),
                    McInstance::Bus(b) if b.member.is_empty() => Some(b.name.clone()),
                    McInstance::Component(c) => Some(c.name.to_string()),
                    _ => None,
                },
                _ => None,
            }),
            _ => None,
        }
    }
}
