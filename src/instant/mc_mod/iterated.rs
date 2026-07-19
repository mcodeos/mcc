// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Iterated call expansion
//!
//! - `check_and_expand_iterated_call`  —— Detect iterated calls whose caller is a Vector (e.g. `cx[1:2]`)
//! - `resolve_indexed_params`          —— Expand `Set` etc. in parameters by the iteration index

use super::funccall::FuncCallInst;
use super::McModuleInst;
use crate::semantic::basic::mc_bus::McBus;
use crate::semantic::basic::mc_endpoint::{McEndpoint, McInstanceRef};
use crate::semantic::basic::mc_param::McParamValue;
use crate::semantic::basic::mc_phrase::McPhrase;
use crate::semantic::mc_inst::McInstance;
use crate::instant::mc_net::InstError;
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
    /// → cx1.Cap(XTAL.X1, gnd) + cx2.Cap(XTAL.X2, gnd)
    /// → creates two independent CAP component instances
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
        // Endpoint(Label(name)); the name is preserved so that process_line
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
                McPhrase::Series(phrases) => phrases,
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

        let mut all_components = Vec::new();
        let mut all_connections = Vec::new();

        for (i, item) in items.iter().enumerate() {
            // 1. Process the caller of each item (recursive instantiation)
            self.process_line(item)?;

            // 2. Get item as the new left endpoints
            let item_right_pts = self.get_right_points_from_phrase(item)?;
            let item_left_elems: Vec<McBus> =
                item_right_pts.iter().map(|p| McBus::new(&p.path)).collect();

            // 3. Resolve indices in parameters (e.g. XTAL.X<1:2> expands to XTAL.X1, XTAL.X2)
            let resolved_params = Self::resolve_indexed_params(params, i, count);

            // 4. Call instantiate_funccall for each iterated item
            let result = self.instantiate_funccall(
                func_name,
                &resolved_params,
                &item_left_elems,
                right,
                caller.as_deref(),
            )?;

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
                    self.sub_modules.push(inst);
                    all_connections.extend(new_connections);
                }
                FuncCallInst::PassThrough => {}
            }
        }

        // ── Iter-6.S5.2-diag ──

        if all_components.is_empty() && all_connections.is_empty() {
            Ok(Some(FuncCallInst::PassThrough))
        } else {
            Ok(Some(FuncCallInst::Components {
                new_components: all_components,
                new_connections: all_connections,
            }))
        }
    }

    /// Resolve index-related values in parameters
    ///
    /// For each parameter value:
    /// - If it is `Set` (e.g. `[DC1, DC2]`), take the element at `index`
    /// - If it is `Opdc::Vector` (IDA expansion such as `X<1:2>` → `[X1, X2]`), take the element at `index`
    /// - Other types remain unchanged (e.g. the constant `gnd` is identical for every iteration)
    ///
    /// # Parameters
    /// - `params` — the original parameter list
    /// - `index` — current iteration index
    /// - `total` — total iteration count (used for bounds checking)
    fn resolve_indexed_params(
        params: &[McParamValue],
        index: usize,
        _total: usize,
    ) -> Vec<McParamValue> {
        params
            .iter()
            .map(|p| {
                match p {
                    // Set: [DC1, DC2] → take the element at position `index`
                    McParamValue::Set(values) => {
                        if index < values.len() {
                            values[index].clone()
                        } else {
                            // When out of bounds, use the last element (broadcasting semantics)
                            values.last().cloned().unwrap_or_else(|| p.clone())
                        }
                    }
                    // Opdc SquareVec: IDA expansion result such as X<1:2> → [X1, X2]
                    // Take the element at position `index`
                    /*McParamValue::Opd(McOpd::SquareVec(items)) => {
                        if index < items.len() {
                            McParamValue::Opdc(items[index].clone())
                        } else {
                            items
                                .last()
                                .map(|i| McParamValue::Opdc(i.clone()))
                                .unwrap_or_else(|| p.clone())
                        }
                    }*/
                    // Other types (constants, single IDs, etc.) → broadcast directly
                    _ => p.clone(),
                }
            })
            .collect()
    }
}
