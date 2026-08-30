// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Iterated call expansion
//!
//! - `check_and_expand_iterated_call`  —— Detect iterated calls whose caller is a Vector (e.g. `cx[1:2]`)
//! - `resolve_indexed_params`          —— Expand `Set` etc. in parameters by the iteration index

use super::funccall::FuncCallInst;
use super::McModuleInst;
use crate::instant::mc_net::InstError;
use crate::instant::provenance::ExpansionKind;
use crate::semantic::basic::mc_bus::McBus;
use crate::semantic::basic::mc_endpoint::{McEndpoint, McInstanceRef};
use crate::semantic::basic::mc_param::McParamValue;
use crate::semantic::basic::mc_phrase::McPhrase;
use crate::semantic::mc_inst::McInstance;
use crate::McIds;

impl McModuleInst {
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

        // ── Iter-1.3 ─────────────────────────────────────────────────────
        // Originally only recognized the `Series[Parallel[...]]` form — that
        // is the two-level structure the parser has already expanded for
        // `cx[1:2].Cap(...)`. For the case in `cap[4:5]::CAP(1uF)` where the
        // caller is a **bare array name**, the parser does not perform this
        // expansion, and the caller is just
        // `Endpoint::Single(Bus("cap[4:5]"))` / `Endpoint::Single(Label("cap[4:5]"))`.
        //
        // A new recognition path is added here: when the caller is a single
        // Endpoint and the name contains `[N:M]` or `[a,b]`, use
        // McIds::expand() to expand it into a list, then fabricate a
        // Parallel structure and feed it into the existing iteration loop.
        //
        // Cost of building the virtual Parallel: each expanded item is an
        // Endpoint(Label(name)); the name is preserved so that process_stmt
        // inside the iterated.rs loop can walk into
        // resolve_array_caller_to_existing to reuse existing instances.
        let mut synthesized_parallel: Option<Vec<McPhrase>> = None;
        if let McPhrase::Endpoint(McEndpoint::Single(iref)) = caller_phrase {
            let bare_name = match &iref.base {
                McInstance::Label(s) => Some(s.clone()),
                McInstance::Bus(b) if b.member.is_empty() => Some(b.name.clone()),
                _ => None,
            };
            // ── Iter-6.S5.2-diag ──
            let _base_kind = match &iref.base {
                McInstance::Label(s) => format!("Label('{s}')"),
                McInstance::Bus(b) => format!("Bus(name='{}', mem={:?})", b.name, b.member),
                McInstance::Component(c) => format!("Component('{}')", c.name),
                McInstance::Module(m) => format!("Module('{}')", m.name),
                McInstance::List(l) => format!("List(name='{}', mem={:?})", l.name, l.member),
                McInstance::Interface(i) => format!("Interface('{}')", i.name),
                _ => "Other".to_string(),
            };
            if let Some(name) = bare_name {
                if name.contains('[') {
                    let ids = McIds::from(name.as_str());
                    let expanded = ids.expand();
                    // ── Iter-6.S5.2-diag ──
                    if expanded.len() > 1 {
                        synthesized_parallel = Some(
                            expanded
                                .into_iter()
                                .map(|n| {
                                    McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                                        McInstance::Label(n),
                                    )))
                                })
                                .collect(),
                        );
                    }
                }
            }
        }

        // caller must be McPhrase::Series whose first element is Parallel — or be synthesized above
        let items_owned: Vec<McPhrase>;
        let items: &Vec<McPhrase> = if let Some(ref v) = synthesized_parallel {
            items_owned = v.clone();
            &items_owned
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

        // ── Expansion provenance: Iterated (covers the per-item loop,
        //    §4.1-A4 / B8). Per-item constructions / func calls begin their
        //    own records nested under this one. ──
        let eidx = self.expansion.begin(
            ExpansionKind::Iterated,
            None,
            func_name.to_string(),
            self.current_call_site(),
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
                    .components
                    .iter()
                    .find(|c| c.name == inst_name)
                    .and_then(|c| c.def.funcs.find(&func_name_str).cloned())
                    .or_else(|| {
                        self.sub_modules
                            .iter()
                            .find(|m| m.name == inst_name)
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
    /// Every parameter value is **broadcast unchanged** to every iterated
    /// member. The member loop supplies the receiver identity via
    /// `item_left_elems` (each item's own pins) — it never splits a `Set`
    /// arg list or a `Vector` (`X<1:2>`) across members. §5 item 17:
    /// `res[1:2].Pullup([net,vcc])` → res1, res2 each get one `net - RES - vcc`.
    ///
    /// # Parameters
    /// - `params` — the original parameter list
    /// - `index` — current iteration index (retained for signature stability)
    /// - `total` — total iteration count (used for bounds checking)
    fn resolve_indexed_params(
        params: &[McParamValue],
        _index: usize,
        _total: usize,
    ) -> Vec<McParamValue> {
        params.iter().cloned().collect()
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
                _ => None,
            },
            McPhrase::Endpoint(McEndpoint::List(refs)) => refs.first().and_then(|ep| match ep {
                McEndpoint::Single(iref) => match &iref.base {
                    McInstance::Label(s) => Some(s.clone()),
                    McInstance::Bus(b) if b.member.is_empty() => Some(b.name.clone()),
                    _ => None,
                },
                _ => None,
            }),
            _ => None,
        }
    }
}
