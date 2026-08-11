// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Concrete execution of FuncCall instantiation
//!
//! - `instantiate_component_construction`  —— Inline component construction (`CAP(0.1uF)`, `Diode(...)`)
//! - `instantiate_module_construction`     —— Inline sub-module call (`PowerDomain(dc24v)`)
//! - `instantiate_user_func`               —— User function body expansion (`func input(sin) { ... }`)
//! - `instantiate_instance_method`         —— Instance method (`uC.power(...)`)
//! - `prefix_instance_line/phrase/node_element` —— Label prefixing in instance method bodies

use super::expand::ExpansionContext;
use super::funccall::FuncCallInst;
use super::FailedRecord;
use super::McModuleInst;
use crate::instant::mc_comp::McComponentInst;
use crate::instant::mc_net::{ConnectionInst, InstError, NetPoint};
use crate::semantic::basic::mc_bus::McBus;
use crate::semantic::basic::mc_closure::McClosure;
use crate::semantic::basic::mc_endpoint::{McEndpoint, McInstanceRef};
use crate::semantic::basic::mc_fcall::McFuncCall;
use crate::semantic::basic::mc_group::McGroup;
use crate::semantic::basic::mc_param::{McParamBindings, McParamValue};
use crate::semantic::basic::mc_phrase::McPhrase;
use crate::semantic::common::{ConnDir, IOType};
use crate::semantic::component::McComponent;
use crate::semantic::mc_func::{McFuncReturn, McFunction};
use crate::semantic::mc_inst::McInstance;
use crate::semantic::module::McModule;
use crate::McIds;
use std::collections::HashMap;
use std::sync::Arc;

// ── P2-2: thread_local side channel ──────────────────────────────────────
// instantiate_instance_method writes to this cell when it detects
// McFuncReturn::Endpoint, and line.rs's process_member_internal reads it
// in the PassThrough path and registers it to auto_inst_map.
thread_local! {
    pub(super) static LAST_RETURN_ENDPOINT: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
}

impl McModuleInst {
    // ========================================================================
    // 1. Inline component construction  e.g. CAP(0.1uF) / Diode('SMBJ30A') / HDR(46)
    // ========================================================================

    /// Inline component construction
    ///
    /// Handles patterns like `CAP(0.1uF, 50V)` / `Diode('SMBJ30A')` / `HDR(46)` in connection lines.
    /// Automatically creates component instances, and generates connections from the FuncCall's
    /// own left/right to the component pins.
    ///
    /// # Multi-pin strategy (Iteration 3)
    /// - **2-pin devices**: use get_left_pin / get_right_pin (pin "1" / pin "2")
    /// - **Multi-pin devices (with IO annotations)**: left → input pins, right → output pins (by position)
    /// - **Multi-pin devices (without IO annotations)**: left → first pin, right → second pin (fallback)
    /// - When pin counts mismatch, record a diagnostic and truncate by min
    pub(super) fn instantiate_component_construction(
        &mut self,
        comp_def: Arc<McComponent>,
        params: &[McParamValue],
        left: &[McBus],
        right: &[McBus],
        caller_name: Option<&str>,
    ) -> Result<FuncCallInst, InstError> {
        // 1. Auto-name: CAP → @CAP_1, DIO.ESD → @DIO_ESD_1, ...
        // ── P0-2: replace '.' in type_name with '_' ──────────────────
        // Prevent `DIO.ESD` generating `DIO.ESD_1` (an instance name containing '.'),
        // which makes node_to_netpoint's split_once('.') mistakenly treat "DIO" as
        // the owner, causing multiple calls to share the same "DIO" label
        // → union-find short circuit.
        let type_name = comp_def.name.to_string();
        let safe_type = type_name.replace('.', "_");
        let inst_name = if let Some(name) = caller_name {
            // P2-7: use caller name as instance name (e.g. R442::RES(1MΩ) → R442)
            name.to_string()
        } else {
            self.auto_name(&safe_type)
        };
        if self.name.contains("513") {
            mcc_dbg!(
                "inst::fcall",
                "[COMP-CREATE] module={} inst_name={inst_name} type={type_name} params={params:?}",
                self.name
            );
        }

        // 2. Create the component instance with parameters
        // ★ P0.5-3: if any param is NC, use with_nc (skip param binding).
        // This converges the anonymous inline path with the named-array
        // declaration path (Mc2Component::with_params sets nc=true, then
        // instantiate_declarations_resilient uses with_nc).
        let has_nc = params.iter().any(|p| matches!(p, McParamValue::NC(_)));
        let inst =
            if has_nc {
                McComponentInst::with_nc(&inst_name, comp_def.clone())
            } else {
                match McComponentInst::with_params(&inst_name, comp_def.clone(), params) {
                    Ok(inst) => inst,
                    Err(e) => {
                        let reason = format!("{:?}", e);
                        mcc_dbg!("inst::fcall", 
                        "[ERROR] Failed to instantiate anonymous component '{}' (class '{}'): {}",
                        inst_name, type_name, reason
                    );
                        self.failed_classes.insert(type_name.clone());
                        self.failed_records.push(FailedRecord {
                            module: self.name.clone(),
                            src_line: self.current_line_span.as_ref().map(|s| s.start / 1000),
                            component_name: inst_name.clone(),
                            class_name: type_name.clone(),
                            reason: reason.clone(),
                        });
                        return Err(InstError::Other(format!(
                            "Failed to instantiate '{}': {}",
                            inst_name, reason
                        )));
                    }
                }
            };

        // ── Iter-3.E3 + P4 ───────────────────────────────────────────────
        // Filter out synthetic interface placeholders that mc_fcall.rs injects when
        // caller=None (`<X>.in` / `<X>.out`). These placeholders are intended to
        // carry interface signatures for "outer chain calls", but when they land on
        // a constructed component, they go through left→get_left_pin /
        // right→get_right_pin below and actually connect to pin1/pin2: shorting
        // all same-type parts via the same pair of ghost nodes (original Iter-3.E3),
        // or cross-shorting with real connections (P4).
        //
        // Key fact: the left/right entering instantiate_**component**_construction
        // are **the component's own interface endpoints** — for legitimate items
        // (chain neighbor / net / real pin, e.g. V3V3 / flash.VCC /
        // lp322dcdc.Vin / GND / flash._CS) **the trailing segment is never bare
        // in / out**; only the synthetic placeholders from mc_fcall.rs:882/891
        // have trailing segments in / out (their base, after constructor/method
        // body prefixing, may be type_name(RES), host name
        // (flash/lp322dcdc/uC/X6), or even flash.CAP multi-segment form — the
        // original Iter-3.E3 only compared the exact string `{type_name}.in`,
        // missing all of them). Sub-module construction goes through
        // instantiate_**module**_construction, and `.in/.out` port expansion is
        // handled in resolve_funccall_*_points, not here. So dropping all
        // trailing "in/out" here is safe and complete (CLAUDE.md P4: clears the
        // `<host>.in ~ part.pin1` / `part.pin2 ~ <host>.out` phantom short
        // circuits for flash/lp322dcdc/uC/X6).
        let is_placeholder =
            |e: &McBus| matches!(e.name.rsplit_once('.'), Some((_, "in")) | Some((_, "out")));
        let left_filtered: Vec<McBus> = left
            .iter()
            .filter(|e| !is_placeholder(e))
            .cloned()
            .collect();
        let right_filtered: Vec<McBus> = right
            .iter()
            .filter(|e| !is_placeholder(e))
            .cloned()
            .collect();
        if left.len() != left_filtered.len() || right.len() != right_filtered.len() {
            let _dropped: Vec<&str> = left
                .iter()
                .chain(right.iter())
                .filter(|e| is_placeholder(e))
                .map(|e| e.name.as_str())
                .collect();
        }
        let left = left_filtered.as_slice();
        let right = right_filtered.as_slice();

        // Only when type_name contains '.' (Family.Type form), drop endpoints in
        // left/right that equal the family name (first segment) as placeholders too —
        // this is exactly the leaking interface caller "DIO".
        let family_seg: Option<String> = if type_name.contains('.') {
            type_name
                .split('.')
                .next()
                .filter(|f| !f.is_empty())
                .map(|f| f.to_string())
        } else {
            None
        };
        let is_placeholder = |e: &McBus| {
            matches!(e.name.rsplit_once('.'), Some((_, "in")) | Some((_, "out")))
                || family_seg.as_deref() == Some(e.name.as_str())
        };
        let left_filtered: Vec<McBus> = left
            .iter()
            .filter(|e| !is_placeholder(e))
            .cloned()
            .collect();
        let right_filtered: Vec<McBus> = right
            .iter()
            .filter(|e| !is_placeholder(e))
            .cloned()
            .collect();
        if left.len() != left_filtered.len() || right.len() != right_filtered.len() {
            let _dropped: Vec<&str> = left
                .iter()
                .chain(right.iter())
                .filter(|e| is_placeholder(e))
                .map(|e| e.name.as_str())
                .collect();
        }
        let left = left_filtered.as_slice();
        let right = right_filtered.as_slice();

        // 3. Handle connections from FuncCall's own left/right to component pins
        let mut new_connections = Vec::new();

        if inst.is_multi_pin() && inst.has_io_annotations() {
            // ★ Multi-pin IO-aware strategy
            // left[i] → input_pins[i], output_pins[i] → right[i]
            let input_pins = inst.get_input_pins();
            let output_pins = inst.get_output_pins();

            if !left.is_empty() && !input_pins.is_empty() {
                let left_count = left.len();
                let pin_count = input_pins.len();
                if left_count != pin_count {
                    self.record_warning(
                        930,
                        format!(
                            "Component '{inst_name}' ({type_name}) input pin count mismatch: {left_count} connections vs {pin_count} input pins"
                        ),
                    );
                }
                let connect_count = left_count.min(pin_count);
                for i in 0..connect_count {
                    let lp = self.node_to_netpoint(&left[i]);
                    new_connections.push(ConnectionInst::new(
                        self.next_conn_id(),
                        vec![lp, input_pins[i].clone()],
                    ));
                }
            }

            if !right.is_empty() && !output_pins.is_empty() {
                let right_count = right.len();
                let pin_count = output_pins.len();
                if right_count != pin_count {
                    self.record_warning(
                        931,
                        format!(
                            "Component '{inst_name}' ({type_name}) output pin count mismatch: {right_count} connections vs {pin_count} output pins"
                        ),
                    );
                }
                let connect_count = right_count.min(pin_count);
                for i in 0..connect_count {
                    let rp = self.node_to_netpoint(&right[i]);
                    new_connections.push(ConnectionInst::new(
                        self.next_conn_id(),
                        vec![output_pins[i].clone(), rp],
                    ));
                }
            }

            // If left/right are both empty but there are IO pins, rely on adjacent
            // connections (handled by process_line outer layer)
        } else {
            // 2-pin or no IO annotation: original logic (get_left_pin / get_right_pin)
            if !left.is_empty() {
                if let Some(left_pin) = inst.get_left_pin() {
                    let left_points: Vec<NetPoint> =
                        left.iter().map(|e| self.node_to_netpoint(e)).collect();
                    for lp in &left_points {
                        new_connections.push(ConnectionInst::new(
                            self.next_conn_id(),
                            vec![lp.clone(), left_pin.clone()],
                        ));
                    }
                }
            }

            if !right.is_empty() {
                if let Some(right_pin) = inst.get_right_pin() {
                    let right_points: Vec<NetPoint> =
                        right.iter().map(|e| self.node_to_netpoint(e)).collect();
                    for rp in &right_points {
                        new_connections.push(ConnectionInst::new(
                            self.next_conn_id(),
                            vec![right_pin.clone(), rp.clone()],
                        ));
                    }
                }
            }
        }

        Ok(FuncCallInst::Components {
            new_components: vec![inst],
            new_connections,
        })
    }

    // ========================================================================
    // 2. Inline sub-module call  e.g. PowerDomain(dc24v) / Uart2RS485(DC.IVCC5)
    // ========================================================================

    /// Inline module call
    ///
    /// Handles patterns like `PowerDomain(dc24v)` / `Uart2RS485(DC.IVCC5)` in connection lines.
    /// Automatically creates sub-module instances, recursively instantiates them,
    /// and generates interface connections.
    ///
    /// # Iteration 3: Port matching enhancement
    /// - By position: left[i] → inputs[i], outputs[i] → right[i]
    /// - On count mismatch, record a diagnostic and truncate by min (without aborting)
    /// - On sub-module instantiation failure, record a diagnostic but keep the instance
    pub(super) fn instantiate_module_construction(
        &mut self,
        func_name: &McIds,
        module_def: Arc<McModule>,
        params: &[McParamValue],
        left: &[McBus],
        right: &[McBus],
    ) -> Result<FuncCallInst, InstError> {
        // 1. Auto-name (using the function name at the call site as the type name)
        // ── P0-2: replace '.' to prevent instance names containing dots from
        //    interfering with path resolution ──
        let type_name = func_name.to_string();
        let safe_type = type_name.replace('.', "_");
        let inst_name = self.auto_name(&safe_type);

        // 2. Create the sub-module instance with parameters
        let mut sub_inst = McModuleInst::with_params(&inst_name, module_def, params)?;

        // 3. Recursively instantiate the sub-module interior (expand its ports,
        //    declarations, connection lines)
        //    ★ On failure, record a diagnostic but keep the instance
        if let Err(e) = sub_inst.instantiate() {
            self.record_error(
                932,
                format!("Inline module '{inst_name}' ({type_name}) instantiation failed: {e}"),
            );
        }
        self.merge_diagnostics_from(&sub_inst);

        // 4. Generate connections from FuncCall's own left/right to sub-module ports
        let mut new_connections = Vec::new();

        // Collect input/output ports
        let input_ports: Vec<_> = sub_inst
            .ports
            .iter()
            .filter(|p| matches!(p.iotype, IOType::In))
            .collect();
        let output_ports: Vec<_> = sub_inst
            .ports
            .iter()
            .filter(|p| matches!(p.iotype, IOType::Out))
            .collect();

        // Input connections: left[i] → sub_inst.inputs[i] (by position)
        if !left.is_empty() && !input_ports.is_empty() {
            let left_count = left.len();
            let port_count = input_ports.len();
            if left_count != port_count {
                self.record_warning(
                    933,
                    format!(
                        "Module '{inst_name}' ({type_name}) input port count mismatch: {left_count} connections vs {port_count} input ports"
                    ),
                );
            }
            let connect_count = left_count.min(port_count);
            for i in 0..connect_count {
                let left_point = self.node_to_netpoint(&left[i]);
                let input_point = NetPoint::with_owner(
                    &format!("{}.{}", inst_name, input_ports[i].name),
                    &inst_name,
                    IOType::In,
                );
                new_connections.push(ConnectionInst::new(
                    self.next_conn_id(),
                    vec![left_point, input_point],
                ));
            }
        }

        // Output connections: sub_inst.outputs[i] → right[i] (by position)
        if !right.is_empty() && !output_ports.is_empty() {
            let right_count = right.len();
            let port_count = output_ports.len();
            if right_count != port_count {
                self.record_warning(
                    934,
                    format!(
                        "Module '{inst_name}' ({type_name}) output port count mismatch: {right_count} connections vs {port_count} output ports"
                    ),
                );
            }
            let connect_count = right_count.min(port_count);
            for i in 0..connect_count {
                let right_point = self.node_to_netpoint(&right[i]);
                let output_point = NetPoint::with_owner(
                    &format!("{}.{}", inst_name, output_ports[i].name),
                    &inst_name,
                    IOType::Out,
                );
                new_connections.push(ConnectionInst::new(
                    self.next_conn_id(),
                    vec![output_point, right_point],
                ));
            }
        }

        Ok(FuncCallInst::SubModule {
            inst: sub_inst,
            new_connections,
        })
    }

    // ========================================================================
    // 3. User function body expansion  e.g. func input(sin) { sin -> buffer -> out }
    // ========================================================================

    pub(super) fn instantiate_user_func(
        &mut self,
        func_def: McFunction,
        params: &[McParamValue],
        _left: &[McBus],
        right: &[McBus],
        caller_inst_name: Option<&str>,
    ) -> Result<FuncCallInst, InstError> {
        // Phase 3 (Task 2.3): User function body expansion
        //
        // Expands `func input(sin) { sin -> buffer -> out }` at call site.
        // Body lines were pre-parsed in mc_code.rs::pre_parse_func_bodies.

        // 1. Param binding (formal <- actual, positional + named)
        let bindings = McParamBindings::bind(&func_def.params, params)
            .map_err(|e| InstError::Other(format!("Func param bind: {e:?}")))?;

        // 2. Expand function body lines with parameter substitution
        if !func_def.lines.is_empty() {
            // ── P4-b: Isolate anonymous instance entries for each body line in the same func ──
            // Take an outer snapshot; reset before each line → @CAP/@RES entries
            // from previous body lines do not linger, preventing member_key pointer
            // reuse that causes the next line's .Cap()/.Pullup() to be mis-paired
            // with the previous line's instance (root cause of VDD/VDD_CORE short
            // circuits); entries from outer lines remain because they are in the
            // snapshot (preserves the chained return `X6.setup(...).XTAL`).
            let outer_auto_inst = self.auto_inst_map.clone();
            for (_li, line) in func_def.lines.iter().enumerate() {
                self.auto_inst_map = outer_auto_inst.clone();
                // Substitute formal params -> actual args in each connection line
                // Also substitute 'this' with caller_inst_name
                let substituted = if bindings.is_empty() && caller_inst_name.is_none() {
                    line.clone()
                } else {
                    Self::substitute_line(line, &bindings, None)
                };
                self.process_line(&substituted)?;
            }

            // ── Process conditional blocks (if/else if/else) ──
            if !func_def.conds.is_empty() {
                let params: Vec<(crate::McIds, String)> = bindings
                    .iter()
                    .filter_map(|b| {
                        let name = b.declare.get_primary_name()?;
                        let value = b.get_value().map(|v| v.to_string()).unwrap_or_default();
                        Some((crate::McIds::from(name.as_str()), value))
                    })
                    .collect();
                for conds in &func_def.conds {
                    let matched_lines = conds.evaluate(&params);
                    for line in matched_lines {
                        let substituted = if bindings.is_empty() && caller_inst_name.is_none() {
                            line.clone()
                        } else {
                            Self::substitute_line(line, &bindings, None)
                        };
                        self.process_line(&substituted)?;
                    }
                }
            }
        } else {
            // Body was not parsed (lines is empty)
            mcc_dbg!(
                "inst::fcall",
                "Warning: User function '{}' has no parsed lines. \
                 Ensure function bodies are parsed during pass1.",
                func_def.name
            );
        }

        // 3. Apply return-value semantics
        match &func_def.returns {
            // ── Implicit / This: caller pass-through ─────────────────────────
            // No bridge needed — fc.right was set to caller's right by the
            // parser (or to a symbolic `func.out` label for bare calls), and
            // chain wiring will connect those into the surrounding net.
            McFuncReturn::Implicit | McFuncReturn::This => Ok(FuncCallInst::PassThrough),

            // ── Endpoint(phrase): non-chainable, returns a label/bus ─────────
            // Bridge fc.right (the parser-supplied placeholder) to the
            // resolved endpoint NetPoints. Union-find then merges the two
            // sides so that "func() -> X" effectively becomes "endpoint -> X".
            //
            // NB: This handles bare calls (`f() -> X`) only. For instance
            // methods (`uC.method() -> X`), fc.right is the receiver's full
            // output port set, so a per-element bridge would short receiver
            // outputs to the single endpoint. That case is currently left
            // as PassThrough until `resolve_funccall_right_points` is taught
            // to detect Endpoint returns directly. (See iteration A note.)
            McFuncReturn::Endpoint(endpoint_phrase) => Self::emit_endpoint_return_bridges(
                self,
                &func_def,
                &bindings,
                endpoint_phrase,
                right,
                caller_inst_name,
            ),
        }
    }

    /// Build bridge `ConnectionInst`s linking the parser-supplied right-side
    /// placeholders (`right`) to the function's actual return endpoint.
    ///
    /// Used by [`instantiate_user_func`] when `func_def.returns` is
    /// `McFuncReturn::Endpoint(_)`. The strategy is:
    ///
    /// 1. Substitute formal params in the endpoint phrase.
    /// 2. Get the McBus list via `phrase.get_right()` (for a label/bus this
    ///    is just the same endpoint; for richer expressions it is the value
    ///    side of the phrase).
    /// 3. Pair-up `right[i]` with `endpoint_buses[i]` and emit one
    ///    `ConnectionInst` per pair.
    ///
    /// On shape mismatch we wire the common prefix and warn — this is more
    /// useful than failing outright while the user is still iterating.
    fn emit_endpoint_return_bridges(
        this: &mut Self,
        _func_def: &McFunction,
        bindings: &McParamBindings,
        endpoint_phrase: &McPhrase,
        right: &[McBus],
        this_name: Option<&str>,
    ) -> Result<FuncCallInst, InstError> {
        // 1. Param substitution (including 'this' substitution)
        let substituted = if bindings.is_empty() && this_name.is_none() {
            endpoint_phrase.clone()
        } else {
            Self::substitute_line(endpoint_phrase, bindings, None)
        };

        // 2. Resolve to McBus list
        let endpoint_buses = substituted.get_right();

        // 3. Pair-up and emit bridges
        let pair_count = right.len().min(endpoint_buses.len());
        let mut new_connections = Vec::with_capacity(pair_count);
        for i in 0..pair_count {
            let ext_pt = this.node_to_netpoint(&right[i]);
            let ep_pt = this.node_to_netpoint(&endpoint_buses[i]);
            new_connections.push(ConnectionInst::new(
                this.next_conn_id(),
                vec![ext_pt, ep_pt],
            ));
        }

        Ok(FuncCallInst::Components {
            new_components: Vec::new(),
            new_connections,
        })
    }

    // ========================================================================
    // 4. Instance method call  e.g. uC.power([VDD_3V3, GND]) / flash.init()
    // ========================================================================

    /// Handle method calls on sub-module instances.
    ///
    /// Syntax: `uC.power([VDD_3V3, GND], ...)` or `flash.init()`
    ///
    /// When the caller is a declared sub-module instance and func_name is a function
    /// defined in that module's type definition, expand the function body with
    /// parameter substitution and instance prefixing.
    ///
    /// # Flow
    /// 1. Bind params (formal <- actual)
    /// 2. Substitute formal params in function body
    /// 3. Prefix local labels with instance name (avoid naming conflicts)
    /// 4. Expand each connection line in the function body
    pub(super) fn instantiate_instance_method(
        &mut self,
        inst_name: &str,
        func_def: &McFunction,
        params: &[McParamValue],
        _left: &[McBus],
        _right: &[McBus],
    ) -> Result<FuncCallInst, InstError> {
        mcc_dbg!(
            "inst::fcall",
            "[IIM-DBG] module={} inst_name={inst_name} func={} params={params:?}",
            self.name,
            func_def.name
        );
        // 1. Bind formal parameters
        let bindings = McParamBindings::bind(&func_def.params, params).map_err(|e| {
            InstError::Other(format!(
                "Instance method '{}' on '{}' param bind: {:?}",
                func_def.name, inst_name, e
            ))
        })?;

        // 2. Dispatch by inst type
        //   - Sub-module: body executes inside the sub-module (P3 core fix)
        //   - Component:  body executes at outer (self) layer + prefix pins (peripheral circuit around leaf devices)
        if self.find_submodule(inst_name).is_some() {
            self.run_submodule_method(inst_name, func_def, &bindings)?;
        } else {
            self.run_component_method(inst_name, func_def, &bindings)?;
        }

        // 3. Expose return endpoint (both kinds unified: prefix inst_name)
        //    Example: `X6.setup(...)` returns "XTAL" → `X6.XTAL`; sub-module return port works the same.
        if let McFuncReturn::Endpoint(ref ep_name) = func_def.returns {
            let ep_path = format!("{inst_name}.{ep_name}");
            let encoded = format!("@@RETURN_EP:{ep_path}");
            LAST_RETURN_ENDPOINT.with(|cell| cell.replace(Some(encoded)));
        }

        Ok(FuncCallInst::PassThrough)
    }

    /// ── P3: Sub-module method ────────────────────────────────────────────────
    /// The method body expands **inside the sub-module instance**:
    ///   - Internal components like `uC` are resolved in place (no longer leak `mcu513.uC`)
    ///   - Pull-ups / address resistors created by `uC.i2c(0x36)` go into the **sub-module**'s components
    ///   - `uC.I2C0 ~ I2C0` becomes an internal sub-module connection
    ///     (flattened: `mcu513.uC.I2C0 ~ mcu513.I2C0`)
    /// Parent-scope formals (bound to parent module's component/port/label, e.g. `flash.SPI`):
    ///   - **Not substituted** in body; keep the formal name (e.g. `spi`) as the sub-module boundary label
    ///   - In the parent module, connect by `parent_actual ~ inst.formal`
    ///     (member-level 4-lane completed by P2)
    fn run_submodule_method(
        &mut self,
        inst_name: &str,
        func_def: &McFunction,
        bindings: &McParamBindings,
    ) -> Result<(), InstError> {
        // ── P2-8: skip if already auto-invoked ──
        // Module-level functions (closures) with arity=0 are auto-invoked
        // during instantiate(). When a parent module explicitly calls the
        // same function (e.g. `mcu513.i2c()`), the body should not be
        // executed again — it would create duplicate components and wrong
        // connections.
        {
            let idx = self
                .sub_modules
                .iter()
                .position(|s| s.name == inst_name)
                .ok_or_else(|| {
                    InstError::Other(format!("submodule '{inst_name}' not found for method"))
                })?;
            let sub = &self.sub_modules[idx];
            let func_name = func_def.name.to_string();
            if sub.auto_invoked_funcs.contains(&func_name) {
                mcc_dbg!("inst::fcall", 
                    "[P2-8-SKIP] module={} sub={inst_name} func={func_name} already auto-invoked, skipping",
                    self.name
                );
                return Ok(());
            }
        }

        // Distinguish boundary formals (parent-scope refs) vs value formals (literal/constant)
        let mut boundary_formals: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut boundary_pairs: Vec<(String, McParamValue)> = Vec::new();
        for b in bindings.iter() {
            let Some(fname) = b.declare.get_primary_name() else {
                continue;
            };
            if let Some(v) = b.get_value() {
                if self.actual_is_parent_ref(v) {
                    boundary_formals.insert(fname.clone());
                    boundary_pairs.push((fname, v.clone()));
                }
            }
        }
        // Only substitute value formals; keep names of boundary formals
        let value_bindings = bindings.subset_excluding(&boundary_formals);

        // Phase A: Execute the body inside the sub-module
        let idx = self
            .sub_modules
            .iter()
            .position(|s| s.name == inst_name)
            .ok_or_else(|| {
                InstError::Other(format!("submodule '{inst_name}' not found for method"))
            })?;
        {
            let sub = &mut self.sub_modules[idx];
            // ── P2-2: register boundary formals as buses in the submodule ──
            // When a boundary formal (e.g. `spi`) is paired with a component bus
            // (e.g. `uC[SPI]`), the default broadcast behavior shorts all bus pins.
            // By registering the formal as a bus with the component's pin IDs,
            // the body connection becomes N×N instead of 1×N broadcast.
            for (formal, _) in &boundary_pairs {
                // Resolve formal to declared port name (case-insensitive)
                let resolved_port = sub
                    .ports
                    .iter()
                    .find(|p| p.name.eq_ignore_ascii_case(formal))
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| formal.clone());
                if let Some(pin_ids) = sub
                    .components
                    .iter()
                    .find_map(|comp| comp.find_bus_port_pin_ids(&resolved_port))
                {
                    if pin_ids.len() >= 2 {
                        let members: Vec<String> =
                            pin_ids.iter().map(|(_, pid)| pid.clone()).collect();
                        let _ = sub.ensure_bus(formal, &members);
                    }
                }
            }
            // ── P4-b: Isolate anonymous instance entries for each body line
            //    in the same func ──
            // (This is a sub-module; snapshot-reset sub.auto_inst_map)
            let outer = sub.auto_inst_map.clone();
            for (_li, line) in func_def.lines.iter().enumerate() {
                sub.auto_inst_map = outer.clone();
                let substituted = if value_bindings.is_empty() {
                    line.clone()
                } else {
                    Self::substitute_line(line, &value_bindings, None)
                };
                if let Err(_e) = sub.process_line(&substituted) {
                    // Sub-module's own diagnostics surface with flattening;
                    // here only log, do not abort
                }
            }
        } // sub's mutable borrow ends here

        // ── Process conditional blocks in sub-module ──
        if !func_def.conds.is_empty() {
            let params: Vec<(crate::McIds, String)> = bindings
                .iter()
                .filter_map(|b| {
                    let name = b.declare.get_primary_name()?;
                    let value = b.get_value().map(|v| v.to_string()).unwrap_or_default();
                    Some((crate::McIds::from(name.as_str()), value))
                })
                .collect();

            let idx = self
                .sub_modules
                .iter()
                .position(|s| s.name == inst_name)
                .ok_or_else(|| {
                    InstError::Other(format!("submodule '{inst_name}' not found for method"))
                })?;
            let sub = &mut self.sub_modules[idx];
            for conds in &func_def.conds {
                let matched_lines = conds.evaluate(&params);
                for line in matched_lines {
                    let substituted = if value_bindings.is_empty() {
                        line.clone()
                    } else {
                        Self::substitute_line(line, &value_bindings, None)
                    };
                    if let Err(_e) = sub.process_line(&substituted) {}
                }
            }
        }

        // Phase B: Boundary connections (in parent module self)
        //   Parent actual (flash.SPI) ~ sub-module boundary label (mcu513.spi)
        //   The 4-member expansion + zip of both SPI buses is completed by P2;
        //   P3 establishes this path first.
        //
        // ── S1 Bug D fix (Part 1): Build boundary using the declared port
        //    name instead of the formal ──
        // The formal is the function parameter name (case may not match the
        // declared port name, e.g. `spi` vs `SPI`).
        // expand_port_lanes Case 1 strictly matches the port name (Case 1.a:
        // ports.iter().filter(p.name == port_base)) → case mismatch → fall
        // back to scalar → 1-vs-N broadcast → all 4 uC SPI pins short into
        // the same spi net (S1).
        //
        // Fix: look up via self.find_submodule(inst_name).ports,
        //   - First strict match (p.name == formal)
        //   - Then case-insensitive fallback (p.name.eq_ignore_ascii_case(formal))
        // If both fail, fall back to the original formal (safe fallback,
        // preserving old behavior).
        let resolved: Vec<(String, String, IOType, McParamValue)> = boundary_pairs
            .into_iter()
            .map(|(formal, actual)| {
                let (declared, iotype) = self
                    .find_submodule(inst_name)
                    .and_then(|sub| {
                        // Prefer bus/interface ports (bus_members non-empty);
                        // these are the **declared** ports registered by
                        // instantiate_interface in pass1. Otherwise we would
                        // hit the boundary-formal placeholder port that
                        // Phase A body adds first (bus_members empty, e.g. `spi`).
                        sub.ports
                            .iter()
                            .find(|p| p.name == formal && !p.bus_members.is_empty())
                            .or_else(|| {
                                sub.ports.iter().find(|p| {
                                    p.name.eq_ignore_ascii_case(&formal)
                                        && !p.bus_members.is_empty()
                                })
                            })
                            // Finally fall back to any same-name port (for scalar boundary compatibility)
                            .or_else(|| sub.ports.iter().find(|p| p.name == formal))
                            .or_else(|| {
                                sub.ports
                                    .iter()
                                    .find(|p| p.name.eq_ignore_ascii_case(&formal))
                            })
                            .map(|p| (p.name.clone(), p.iotype.clone()))
                    })
                    .unwrap_or_else(|| (formal.clone(), IOType::None));
                (formal, declared, iotype, actual)
            })
            .collect();
        for (formal, declared_port_name, _port_iotype, actual) in resolved {
            let actual_elems = Self::param_value_to_node_elements(&actual);
            let mut left: Vec<NetPoint> = Vec::new();
            for e in &actual_elems {
                let expanded = self.expand_node_element(e);
                left.extend(expanded);
            }
            // ── P2-2: use the formal name for the boundary connection ──
            // This matches the submodule's internal bus registration (registered
            // with the formal name in Phase A), ensuring the boundary connection
            // and the submodule body use the same label.
            let boundary_name = format!("{inst_name}.{formal}");
            // Try to expand via component pin ID lookup (same as Phase A bus registration)
            let pin_ids: Option<Vec<(String, String)>> =
                self.find_submodule(inst_name).and_then(|sub| {
                    sub.components
                        .iter()
                        .find_map(|comp| comp.find_bus_port_pin_ids(&declared_port_name))
                });
            let right: Vec<NetPoint> = if let Some(ref pids) = pin_ids {
                if pids.len() == left.len() && pids.len() >= 2 {
                    pids.iter()
                        .map(|(member_name, pin_id)| {
                            let path = format!("{boundary_name}.{pin_id}");
                            NetPoint::with_owner(&path, inst_name, IOType::None)
                                .with_member_name(member_name)
                        })
                        .collect()
                } else {
                    self.expand_node_element(&McBus::new(&boundary_name))
                }
            } else {
                self.expand_node_element(&McBus::new(&boundary_name))
            };
            self.create_connection(left, right, ConnDir::Undirected)?;
        }
        Ok(())
    }

    /// ── P3: Component method (uC.power / uC.i2c / X6.setup ...) ──────────────
    /// Behavior is the same as before the rewrite: the body expands in the
    /// **current (outer) module** self, and pin references are prefixed to
    /// the component instance name (`VDD` → `uC.VDD`). These are peripheral
    /// circuits around leaf devices and belong to the outer BOM.
    /// (Reuses Iter-2.3's skip set; no longer does Iter-13.1 boundary projection.)
    fn run_component_method(
        &mut self,
        inst_name: &str,
        func_def: &McFunction,
        bindings: &McParamBindings,
    ) -> Result<(), InstError> {
        let mut skip: std::collections::HashSet<String> = std::collections::HashSet::new();
        for b in bindings.iter() {
            if let Some(n) = b.declare.get_primary_name() {
                skip.insert(n);
            }
            if let Some(v) = b.get_value() {
                for e in Self::param_value_to_node_elements(v) {
                    if !e.name.is_empty() {
                        skip.insert(e.name.clone());
                    }
                    for m in &e.member {
                        skip.insert(m.clone());
                    }
                }
            }
        }
        for p in &self.ports {
            skip.insert(p.name.clone());
        }

        // ── P2-8: Remove component pin names from skip set ──
        // When a component pin name (e.g. GND, VDD) happens to match a name
        // in the skip set (e.g. from actual params or module ports), the skip
        // set incorrectly prevents prefixing it with the instance name.
        // This causes component pins to be treated as module-level labels,
        // breaking connections like uC.GND → module GND.
        // Fix: remove component pin names from skip so they get properly
        // prefixed (e.g. GND → uC.GND).
        if let Some(comp) = self.find_component(inst_name) {
            for name in comp.def.pins.names_to_id.keys() {
                skip.remove(name);
            }
            for names in comp.def.pins.pin_id_to_names.values() {
                for name in names {
                    // Also remove the last segment of dotted names
                    // (e.g. "XTAL.X1" → remove "X1" too)
                    skip.remove(name);
                    if let Some(last) = name.rsplit('.').next() {
                        skip.remove(last);
                    }
                }
            }
        }

        // ── P2-4: Register component bus interfaces as buses ──
        // When a component method body uses labels like `I2C0` (which are
        // component interface names), the skip set prevents prefixing them
        // with the instance name. Register the component's bus interfaces
        // as module-level buses so `expand_port_lanes` can expand standalone
        // labels like `I2C0` to `I2C0.SCL` and `I2C0.SDA`.
        let buses_to_register: Vec<(String, Vec<String>)> = {
            if let Some(comp) = self.find_component(inst_name) {
                // P2-5: Register component bus interfaces as module-level buses.
                // Look at pin_id_to_names to find which interface names appear on
                // multiple pins, then look at names_to_id for dot-separated member
                // names (e.g. PBus.CLK, PBus.DATA → PBus with [CLK, DATA]).
                let mut bus_members: HashMap<String, Vec<String>> = HashMap::new();

                // First pass: collect all dot-separated names (e.g. PBus.CLK → PBus)
                // ── P2-10 fix: order members by interface declaration order ──
                // BTreeMap key iteration order is alphabetical, causing UART0
                // members to be collected as [RX, TX] instead of [TX, RX].
                // For non-monotonic pin mappings (e.g. SPI on [10, 8, 11, 9]),
                // pin-ID order is also wrong. Use the declaration order from
                // the Multi port's pin IDs, then look up member names.
                for (name, _port) in comp.def.pins.names_to_id.iter() {
                    if let Some(dot_pos) = name.find('.') {
                        let base = &name[..dot_pos];
                        let member = &name[dot_pos + 1..];
                        bus_members
                            .entry(base.to_string())
                            .or_default()
                            .push(member.to_string());
                    }
                }
                // For Multi ports with declaration order, reorder members
                for (name, port) in comp.def.pins.names_to_id.iter() {
                    if let crate::semantic::component::mc_pins::McPinPort::Multi(pids) = port {
                        if pids.len() >= 2 && bus_members.contains_key(name) {
                            // Reorder members by declaration order (pids)
                            let mut ordered: Vec<String> = Vec::new();
                            let prefix = format!("{name}.");
                            for pid in pids {
                                if let Some(member_names) = comp.def.pins.pin_id_to_names.get(pid) {
                                    for n in member_names {
                                        if let Some(m) = n.strip_prefix(&prefix) {
                                            if !ordered.contains(&m.to_string()) {
                                                ordered.push(m.to_string());
                                            }
                                        }
                                    }
                                }
                            }
                            if ordered.len() >= 2 {
                                bus_members.insert(name.clone(), ordered);
                            }
                        }
                    }
                }
                // ── P2-10 fix: For Interface ports, reorder members by the
                // interface's pin declaration order (BTreeMap key order = pin ID order).
                // Without this, BTreeMap iteration of names_to_id collects dot-separated
                // member names alphabetically (e.g. UART0.RX before UART0.TX), causing
                // cross-wiring with component arrays like res[1:2].
                for (name, port) in comp.def.pins.names_to_id.iter() {
                    if let crate::semantic::component::mc_pins::McPinPort::Interface(iface) = port {
                        if bus_members.contains_key(name) {
                            // Get member order from the interface's pin declaration
                            let mut ordered: Vec<String> = Vec::new();
                            // BTreeMap<pin_id, McPin> iterates in pin-ID order (1, 2, …)
                            for pin in iface.base.pins.pins.values() {
                                if let Some(first_name) = pin.names.first() {
                                    if !ordered.contains(first_name) {
                                        ordered.push(first_name.clone());
                                    }
                                }
                            }
                            if ordered.len() >= 2 {
                                bus_members.insert(name.clone(), ordered);
                            }
                        }
                    }
                }

                // Second pass: for Multi entries without dot-separated members,
                // try to derive member names from the interface definition.
                // Look at the McInterface base.pins to get member names.
                for (name, port) in comp.def.pins.names_to_id.iter() {
                    if let crate::semantic::component::mc_pins::McPinPort::Multi(pids) = port {
                        if pids.len() >= 2 && !bus_members.contains_key(name) {
                            // Try to find member names from Interface variant (may be stored under a different key)
                            // Fall back to using pin IDs as member names
                            let members: Vec<String> = pids.clone();
                            bus_members.insert(name.clone(), members);
                        }
                    }
                }

                bus_members
                    .into_iter()
                    .filter(|(_, members)| members.len() >= 2)
                    .collect()
            } else {
                Vec::new()
            }
        };
        for (iface_name, member_names) in &buses_to_register {
            let _ = self.ensure_bus(iface_name, member_names);
            // ── P2-7-XTAL: also register prefixed bus name ──
            // When the function body references a component interface (e.g. XTAL),
            // prefix_instance_line_with_skip converts it to X6.XTAL as a Label.
            // Register the prefixed name as a bus so lane-by-lane wiring can
            // expand it to individual pins.
            let prefixed_iface = format!("{inst_name}.{iface_name}");
            let _ = self.ensure_bus(&prefixed_iface, member_names);
        }

        if func_def.lines.is_empty() {
            mcc_dbg!(
                "inst::fcall",
                "Warning: component method '{}.{}' has no parsed lines.",
                inst_name,
                func_def.name
            );
            return Ok(());
        }
        // ── P4-b: Isolate anonymous instance entries for each body line in the same func ──
        let conn_start = self.connections.len(); // ← P4 backstop start point
        let _outer_auto_inst = self.auto_inst_map.clone();
        for (_li, line) in func_def.lines.iter().enumerate() {
            // Do not reset auto_inst_map; let it accumulate line by line inside
            // the function body! This way components created in the previous line
            // can still be resolved correctly in subsequent lines!
            // Build ExpansionContext for this line (lifetime scoped per iteration)
            let expansion_ctx = self
                .find_component(inst_name)
                .map(|comp| ExpansionContext::new(comp, bindings, self));
            let substituted = if bindings.is_empty() {
                line.clone()
            } else {
                Self::substitute_line(line, bindings, expansion_ctx.as_ref())
            };
            // Drop expansion_ctx before mutable self borrows below
            drop(expansion_ctx);
            let prefixed = Self::prefix_instance_line_with_skip(&substituted, inst_name, &skip);
            // ── P2-7-XTAL: convert Labels that are known buses to Bus representations ──
            // When prefix_instance_line_with_skip creates prefixed Labels like
            // "X6.XTAL", they need to be converted to Bus("X6.XTAL", members)
            // so that get_left_points/get_right_points can expand them to
            // individual pin points and lane-by-lane wiring handles them correctly.
            let expanded = self.expand_bus_labels(&prefixed);
            mcc_dbg!(
                "inst::fcall",
                "[RCM-DBG] module={} inst={inst_name} line={_li} expanded={expanded:?}",
                self.name
            );
            self.process_line(&expanded)?;
        }
        // ── P4 backstop: strip synthetic host interface endpoints leaked
        //    during body processing ──
        // NOTE: must be called BEFORE conditional block processing since
        // cond blocks may also create new connections.
        self.strip_host_iface_phantoms(inst_name, conn_start);

        // ── Process conditional blocks (if/else if/else) ──
        if !func_def.conds.is_empty() {
            // Convert bindings to (McIds, String) pairs for condition evaluation
            let params: Vec<(crate::McIds, String)> = bindings
                .iter()
                .filter_map(|b| {
                    let name = b.declare.get_primary_name()?;
                    let value = b.get_value().map(|v| v.to_string()).unwrap_or_default();
                    Some((crate::McIds::from(name.as_str()), value))
                })
                .collect();

            if self.name.contains("513") {
                mcc_dbg!(
                    "inst::fcall",
                    "[COND-EVAL] func={} conds_count={} params={params:?}",
                    func_def.name,
                    func_def.conds.len()
                );
            }

            // Record connection start for cond block phantom stripping
            let cond_conn_start = self.connections.len();

            for (ci, conds) in func_def.conds.iter().enumerate() {
                let matched_lines = conds.evaluate(&params);
                if self.name.contains("513") {
                    mcc_dbg!(
                        "inst::fcall",
                        "[COND-EVAL] func={} cond[{}] matched {} lines",
                        func_def.name,
                        ci,
                        matched_lines.len()
                    );
                }
                for (li, line) in matched_lines.iter().enumerate() {
                    let expansion_ctx = self
                        .find_component(inst_name)
                        .map(|comp| ExpansionContext::new(comp, bindings, self));
                    let substituted = if bindings.is_empty() {
                        line.clone()
                    } else {
                        Self::substitute_line(line, bindings, expansion_ctx.as_ref())
                    };
                    drop(expansion_ctx);
                    let prefixed =
                        Self::prefix_instance_line_with_skip(&substituted, inst_name, &skip);
                    let expanded = self.expand_bus_labels(&prefixed);
                    if self.name.contains("513") {
                        mcc_dbg!(
                            "inst::fcall",
                            "[COND-PROC] func={} cond[{}] line[{}] expanded={expanded:?}",
                            func_def.name,
                            ci,
                            li
                        );
                    }
                    self.process_line(&expanded)?;
                }
            }

            // Strip phantom interfaces from cond block connections too
            self.strip_host_iface_phantoms(inst_name, cond_conn_start);
        }

        Ok(())
    }

    /// ── P4 backstop: Strip synthetic host interface endpoints leaked during
    ///    component method / constructor body processing ──
    ///
    /// `<inst>.in` / `<inst>.out` are synthetic interface placeholders that
    /// mc_fcall.rs injects for constructor / method calls when caller=None
    /// (mc_fcall.rs:882/891). Components themselves **never** have real pins
    /// named in/out (the spec is numeric / VCC / VDD / _CS / Vin / EN / XTAL …),
    /// so after a component method / constructor func body is processed, if any
    /// new connection has an endpoint that is exactly `<inst>.in` / `<inst>.out`,
    /// it must be a leaked phantom node (observed: `flash.in ~ CAP_1.1`,
    /// `lp322dcdc.in ~ RES_1.1`, `uC.in ~ CAP_3.1`, `X6.in ~ CAP_4.1`). These
    /// phantom nodes cross-short with real connections (CLAUDE.md P4).
    ///
    /// points.rs's `[FIX-C]` is supposed to quarantine such `<host>.in` into
    /// `@_phantom_*` at `node_to_netpoint`, but this phantom is a directly
    /// constructed `NetPoint` (bypassing `node_to_netpoint` — evidence: no
    /// `@_phantom` in the netlist), so `[FIX-C]` does not fire. Here we provide a
    /// **final backstop** after body processing: only within the connections
    /// newly added by this body (`conn_start..`), strip these two endpoints from
    /// each connection's `points`; real connections (e.g. `V3V3 ~ CAP_1.1`,
    /// `RES_1.1 ~ flash._CS`) are **independent ConnectionInst**s, do not contain
    /// `<host>.in/.out` endpoints, and are unaffected. Connections with fewer than
    /// 2 points after stripping are dropped entirely.
    pub(super) fn strip_host_iface_phantoms(&mut self, inst_name: &str, conn_start: usize) {
        if conn_start > self.connections.len() {
            return;
        }
        let in_name = format!("{inst_name}.in");
        let out_name = format!("{inst_name}.out");
        let mut tail = self.connections.split_off(conn_start);
        let mut _stripped = 0usize;
        for conn in tail.iter_mut() {
            let before = conn.points.len();
            conn.points
                .retain(|p| p.path != in_name && p.path != out_name);
            _stripped += before - conn.points.len();
        }
        let kept: Vec<_> = tail.into_iter().filter(|c| c.points.len() >= 2).collect();
        self.connections.extend(kept);
    }

    /// ── P2-7-XTAL: Convert Labels that are known bus names to Bus representations ──
    ///
    /// When `prefix_instance_line_with_skip` creates prefixed Labels like
    /// "X6.XTAL", they are stored as `Endpoint(Single(Label("X6.XTAL")))`.
    /// However, `get_left_points`/`get_right_points` return empty for Labels
    /// (see points.rs lines 468-475), and `collect_one_lane_item` only adds
    /// Labels for lane 0. This means multi-pin component interfaces referenced
    /// in function bodies are not expanded to individual pins.
    ///
    /// This function walks the McPhrase tree and converts any
    /// `Endpoint(Single(Label(name)))` where `name` is a known bus to
    /// `Endpoint(Single(Bus(name, members)))`, enabling proper multi-lane
    /// expansion.
    fn expand_bus_labels(&self, phrase: &McPhrase) -> McPhrase {
        match phrase {
            McPhrase::Series(phrases, d) => McPhrase::Series(
                phrases.iter().map(|p| self.expand_bus_labels(p)).collect(),
                *d,
            ),
            McPhrase::Parallel(phrases) => {
                McPhrase::Parallel(phrases.iter().map(|p| self.expand_bus_labels(p)).collect())
            }
            McPhrase::Multiple(inner) => {
                McPhrase::Multiple(inner.iter().map(|p| self.expand_bus_labels(p)).collect())
            }
            McPhrase::Transposed(inner) => {
                McPhrase::Transposed(Box::new(self.expand_bus_labels(inner)))
            }
            McPhrase::Group(g) => McPhrase::Group(McGroup {
                opds: g.opds.iter().map(|p| self.expand_bus_labels(p)).collect(),
                left_match: g.left_match,
                right_match: g.right_match,
            }),
            McPhrase::FuncCall(f) => McPhrase::FuncCall(McFuncCall {
                id: f.id,
                caller: f
                    .caller
                    .as_ref()
                    .map(|c| Box::new(self.expand_bus_labels(c))),
                func_name: f.func_name.clone(),
                params: f.params.clone(),
                left: f.left.clone(),
                right: f.right.clone(),
                dot_member: f.dot_member.clone(),
            }),
            McPhrase::Endpoint(McEndpoint::Single(ref iref))
                if matches!(iref.base, McInstance::Label(_)) =>
            {
                if let McInstance::Label(ref s) = iref.base {
                    if let Some(bus) = self.find_bus(s) {
                        if bus.members.len() >= 2 {
                            let new_bus = McBus {
                                name: s.clone(),
                                member: bus.members.clone(),
                                full_members: Vec::new(),
                            };
                            return McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                                McInstance::Bus(new_bus),
                            )));
                        }
                    }
                }
                phrase.clone()
            }
            _ => phrase.clone(),
        }
    }

    /// ── P3: Does the formal's actual point to a parent-scope entity
    ///    (→ that formal is a "boundary formal")? ──
    /// Hits on component/sub_module/port/bus/label are treated as parent-scope
    /// references (e.g. `flash.SPI`); literals (Const/Int/Hex/NC) do not hit
    /// → value formal, substitute normally into body.
    fn actual_is_parent_ref(&self, value: &McParamValue) -> bool {
        let elems = Self::param_value_to_node_elements(value);
        elems.iter().any(|e| {
            if e.name.is_empty() {
                return false;
            }
            let base = e.name.split('.').next().unwrap_or(&e.name);
            self.find_component(base).is_some()
                || self.find_submodule(base).is_some()
                || self.is_port(base)
                || self.is_bus(base)
                || self.labels.contains_key(&e.name)
                || self.labels.contains_key(base)
        })
    }

    // ========================================================================
    // Label/reference prefixing inside instance method bodies
    // ========================================================================

    /// Prefix labels/identifiers in a connection line with instance name.
    ///
    /// When expanding instance method bodies, local labels need to be prefixed
    /// with the instance name to avoid conflicts with parent module labels.
    pub(super) fn prefix_instance_line(phrase: &McPhrase, inst_name: &str) -> McPhrase {
        let empty: std::collections::HashSet<String> = std::collections::HashSet::new();
        Self::prefix_instance_phrase_with_skip(phrase, inst_name, &empty)
    }

    /// ── Iter-2.3 ────────────────────────────────────────────────────────
    /// skip-aware version: names in the `skip` set are **not** prefixed with inst_name.
    ///
    /// Typical usage:
    ///   - Already-substituted actual names (`VCC_1V2` / `GND` from the call site)
    ///     should not be re-prefixed
    ///   - The formal parameter name itself (`V1V2`), if not successfully substituted,
    ///     should not be prefixed to `uC.V1V2`
    ///   - Parent module port names (not internal pins of this component) should also
    ///     not be prefixed
    pub(super) fn prefix_instance_line_with_skip(
        phrase: &McPhrase,
        inst_name: &str,
        skip: &std::collections::HashSet<String>,
    ) -> McPhrase {
        Self::prefix_instance_phrase_with_skip(phrase, inst_name, skip)
    }

    /// Prefix labels/identifiers in an McPhrase with instance name,
    /// optionally skipping names in the `skip` set.
    fn prefix_instance_phrase_with_skip(
        phrase: &McPhrase,
        inst_name: &str,
        skip: &std::collections::HashSet<String>,
    ) -> McPhrase {
        // P2-10: Build the instance prefix string once. Used to check whether
        // a dotted name is already prefixed (e.g. "uC.GPIO.2") vs a bare pin
        // alias that needs prefixing (e.g. "GPIO.2" → "uC.GPIO.2").
        let inst_prefix = format!("{inst_name}.");
        match phrase {
            McPhrase::Series(phrases, d) => McPhrase::Series(
                phrases
                    .iter()
                    .map(|p| Self::prefix_instance_phrase_with_skip(p, inst_name, skip))
                    .collect(),
                *d,
            ),
            McPhrase::Parallel(phrases) => McPhrase::Parallel(
                phrases
                    .iter()
                    .map(|p| Self::prefix_instance_phrase_with_skip(p, inst_name, skip))
                    .collect(),
            ),
            McPhrase::Closure(c) => McPhrase::Closure(McClosure {
                params: c.params.clone(),
                right: c
                    .right
                    .iter()
                    .map(|e| Self::prefix_instance_node_element_with_skip(e, inst_name, skip))
                    .collect(),
                body: c
                    .body
                    .iter()
                    .map(|p| Self::prefix_instance_phrase_with_skip(p, inst_name, skip))
                    .collect(),
            }),
            McPhrase::Group(g) => McPhrase::Group(McGroup {
                opds: g
                    .opds
                    .iter()
                    .map(|p| Self::prefix_instance_phrase_with_skip(p, inst_name, skip))
                    .collect(),
                left_match: g.left_match,
                right_match: g.right_match,
            }),
            McPhrase::FuncCall(f) => {
                // ── P2-7-XTAL: discriminate caller prefixing ──
                // Class names (e.g. RES, CAP, DIO.ESD) start with uppercase →
                //   caller is a user-specified instance name → do NOT prefix.
                // Method names (e.g. setup, power, i2c) start with lowercase →
                //   caller is a reference to an existing instance → prefix.
                let func_name_str = f.func_name.to_string();
                let is_class_name = func_name_str
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_uppercase());
                McPhrase::FuncCall(McFuncCall {
                    id: 0,
                    caller: if is_class_name {
                        // Class construction: caller is instance name, don't prefix
                        f.caller.clone()
                    } else {
                        f.caller.as_ref().map(|c| {
                            Box::new(Self::prefix_instance_phrase_with_skip(c, inst_name, skip))
                        })
                    },
                    func_name: f.func_name.clone(),
                    // ── P4: Prefix bare pin names in actuals (e.g. `_CS` in `.Pullup(_CS, V3V3)`
                    // → `flash._CS`). The underscore placeholder `_` (McOpd::Uscore) and
                    // the skip set (actuals / parent ports, e.g. V3V3) are protected
                    // inside the helper and are not accidentally prefixed.
                    params: f
                        .params
                        .iter()
                        .map(|p| Self::prefix_param_value_with_skip(p, inst_name, skip))
                        .collect(),
                    left: f.left.clone(), // P2-10: do NOT prefix FuncCall's synthetic left/right placeholders
                    right: f.right.clone(), // (e.g., RES.in, RES.out). They are resolved by resolve_funccall_*_points.
                    dot_member: f.dot_member.clone(),
                })
            }
            McPhrase::Transposed(inner) => McPhrase::Transposed(Box::new(
                Self::prefix_instance_phrase_with_skip(inner, inst_name, skip),
            )),
            McPhrase::Lead => phrase.clone(),

            // ── Iter-3.C ────────────────────────────────────────────────
            // Label endpoint: names in skip (actuals, parent ports) are not
            // prefixed; others are prefixed.
            // Example: enable() body `Vin -> RES(47kΩ) -> EN`; Vin and EN are
            // aliases for component pins and must become
            // `lp322dcdc.Vin` / `lp322dcdc.EN`.
            // Previously, Label was cloned directly, producing ghosts like
            // ".1 : Vin.Vin ~ .1" (because Vin stayed as Label, and get_points
            // resolved it as the anonymous owner's pin).
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Label(ref s),
                ..
            })) => {
                // P2-10: Use starts_with(inst_prefix) instead of contains('.')
                // to allow dotted pin aliases like "GPIO.2" to be prefixed,
                // while still skipping already-prefixed names like "uC.GPIO.2".
                if skip.contains(s) || s.starts_with(&inst_prefix) || s.is_empty() {
                    phrase.clone()
                } else {
                    McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(McInstance::Label(
                        format!("{inst_name}.{s}"),
                    ))))
                }
            }

            // Bare Bus (member empty): same as Label handling
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Bus(ref b),
                ..
            })) if b.member.is_empty() => {
                if skip.contains(&b.name) || b.name.starts_with(&inst_prefix) || b.name.is_empty() {
                    phrase.clone()
                } else {
                    let new_bus = McBus::new(&format!("{}.{}", inst_name, b.name));
                    McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(McInstance::Bus(
                        new_bus,
                    ))))
                }
            }

            // Bus with members (e.g. `[VDD_CORE, GND]`): if name is in skip,
            // do not prefix; members are decided individually (skip skips,
            // others stay as-is — members are usually internal pin names /
            // aliases and do not need extra prefix; the full path is assembled
            // when points.rs expands them).
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Bus(ref b),
                ..
            })) => {
                let prefixed_name = if skip.contains(&b.name)
                    || b.name.starts_with(&inst_prefix)
                    || b.name.is_empty()
                {
                    b.name.clone()
                } else {
                    format!("{}.{}", inst_name, b.name)
                };
                let new_bus = McBus {
                    name: prefixed_name,
                    member: b.member.clone(),
                    full_members: b.full_members.clone(),
                };
                McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(McInstance::Bus(
                    new_bus,
                ))))
            }

            // ── P2 fix: Component / Module endpoints must also be prefixed ──────────────
            // Previously they were kept as-is, so `uC` (an internal component of
            // the sub-module) in the func body could not be found in the parent
            // module's scope. Now we prefix to a `mcu513.uC` form Bus, allowing
            // Pass2's scope-chain dispatch to correctly drill down into the
            // sub-module's internal components.
            //
            // But if the component name is in the skip set (referenced from an
            // actual) or is already a dotted path, do not prefix.
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Component(ref c),
                ..
            })) => {
                let cname = c.name.to_string();
                if skip.contains(&cname) || cname.starts_with(&inst_prefix) || cname.is_empty() {
                    phrase.clone()
                } else {
                    let prefixed = format!("{inst_name}.{cname}");
                    McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(McInstance::Bus(
                        McBus::new(&prefixed),
                    ))))
                }
            }
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Module(ref m),
                ..
            })) => {
                let mname = m.name.to_string();
                if skip.contains(&mname) || mname.starts_with(&inst_prefix) || mname.is_empty() {
                    phrase.clone()
                } else {
                    let prefixed = format!("{inst_name}.{mname}");
                    McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(McInstance::Bus(
                        McBus::new(&prefixed),
                    ))))
                }
            }
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::List(_),
                ..
            }))
            | McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Interface(_),
                ..
            })) => phrase.clone(),
            McPhrase::Multiple(phrases) => McPhrase::Multiple(
                phrases
                    .iter()
                    .map(|p| Self::prefix_instance_phrase_with_skip(p, inst_name, skip))
                    .collect(),
            ),
            McPhrase::Endpoint(McEndpoint::Node {
                ref input,
                ref output,
                ..
            }) => {
                let left_elems: Vec<McBus> = input.iter().flat_map(|e| e.get_left()).collect();
                let right_elems: Vec<McBus> = output.iter().flat_map(|e| e.get_right()).collect();
                let prefixed_left: Vec<McBus> = left_elems
                    .iter()
                    .map(|e| Self::prefix_instance_node_element_with_skip(e, inst_name, skip))
                    .collect();
                let prefixed_right: Vec<McBus> = right_elems
                    .iter()
                    .map(|e| Self::prefix_instance_node_element_with_skip(e, inst_name, skip))
                    .collect();
                let left_bus = Self::node_elements_to_bus(&prefixed_left);
                let right_bus = Self::node_elements_to_bus(&prefixed_right);
                McPhrase::Endpoint(McEndpoint::Node {
                    input: vec![McEndpoint::Single(McInstanceRef::new(McInstance::Bus(
                        left_bus,
                    )))],
                    output: vec![McEndpoint::Single(McInstanceRef::new(McInstance::Bus(
                        right_bus,
                    )))],
                })
            }
            McPhrase::Endpoint(ref ep) => McPhrase::Endpoint(ep.clone()),
            McPhrase::Member(phrase, ep) => McPhrase::Member(
                Box::new(Self::prefix_instance_phrase_with_skip(
                    phrase, inst_name, skip,
                )),
                ep.clone(),
            ),
        }
    }

    /// Prefix a McBus with instance name.
    ///
    /// Rules:
    /// - Names already starting with inst_name (e.g. "uC.VDD") are unchanged
    /// - Names with path separator (e.g. "other.pin") are unchanged (cross-instance ref)
    /// - Names in `skip` set are unchanged (Iter-2.3: substituted actuals / parent ports)
    /// - Other local references get "inst_name." prefix
    /// Flattened version: elem.member is Vec<String>
    fn prefix_instance_node_element_with_skip(
        elem: &McBus,
        inst_name: &str,
        skip: &std::collections::HashSet<String>,
    ) -> McBus {
        // Already prefixed with this instance
        if elem.name.starts_with(inst_name)
            && elem.name.len() > inst_name.len()
            && elem.name.as_bytes().get(inst_name.len()) == Some(&b'.')
        {
            return elem.clone();
        }

        // Already has path separator — check if it's a cross-instance reference
        // ── P2 fix: dotted names no longer skip unconditionally ─────────
        // Previously: `uC.in`, `uC.VDD` were both skipped because they contain '.'.
        // But `uC` is an internal component of the sub-module, which needs to be
        // prefixed to `mcu513.uC.VDD`.
        // Now: only skip when the first segment is in the skip set (e.g. `flash`
        // in `flash.SPI` is an actual); otherwise continue prefixing.
        if elem.name.contains('.') {
            let first_seg = elem.name.split('.').next().unwrap_or("");
            if skip.contains(&first_seg.to_string()) || first_seg.is_empty() {
                return elem.clone();
            }
            // First segment already has the inst_name prefix → do not prefix again
            if first_seg == inst_name {
                return elem.clone();
            }
            // Otherwise: local dotted reference (e.g. uC.VDD), fall through
            // to the prefixing logic below
        }

        // Empty name, skip
        if elem.name.is_empty() {
            return elem.clone();
        }

        // Iter-2.3: names in the skip set are not prefixed
        if skip.contains(&elem.name) {
            // Members are also processed individually against skip
            let new_members: Vec<String> = elem
                .member
                .iter()
                .map(|m| {
                    if skip.contains(m) || m.contains('.') {
                        m.clone()
                    } else {
                        format!("{inst_name}.{m}")
                    }
                })
                .collect();
            return McBus {
                name: elem.name.clone(),
                member: new_members,
                full_members: elem.full_members.clone(),
            };
        }

        // Add instance prefix
        // Flattened: member is a string list; prefix each member name directly
        // (but also respect the skip set)
        let new_members: Vec<String> = elem
            .member
            .iter()
            .map(|m| {
                if skip.contains(m) || m.contains('.') {
                    m.clone()
                } else {
                    format!("{inst_name}.{m}")
                }
            })
            .collect();
        let new_full_members: Vec<String> = elem
            .full_members
            .iter()
            .map(|m| {
                if skip.contains(m) || m.contains('.') {
                    m.clone()
                } else {
                    format!("{inst_name}.{m}")
                }
            })
            .collect();
        McBus {
            name: format!("{}.{}", inst_name, elem.name),
            member: new_members,
            full_members: new_full_members,
        }
    }

    /// ── P4: Prefix bare pin names in builtin-twopin actuals ───────────────────
    /// In actuals of `.Cap/.Pullup/.Pulldown` calls like `.Pullup(_CS, V3V3)` /
    /// `.Cap(x)`, the component's own bare pin names (e.g. flash's
    /// `_CS`/`_WP`/`_HOLD`) must be prefixed to `flash._CS` so that
    /// `wire_builtin_twopin` falls onto the instance's real pin via
    /// `expand_node_element`; otherwise they are parsed as free labels and
    /// the pull-up resistor hangs in the air.
    ///
    /// Rules stay consistent with the Endpoint/Label prefixing (the Label/Bus
    /// branches of prefix_instance_phrase_with_skip in this file):
    ///   - Name in `skip` set (actual / parent port, e.g. V3V3) → no prefix;
    ///   - Name already contains '.' (already a dotted path, e.g. `uC.VDD`) → no prefix;
    ///   - Empty name → no prefix;
    ///   - Underscore placeholder `_` is the **independent variant** `McOpd::Uscore`,
    ///     carries no name, and is naturally skipped by structure (never
    ///     accidentally prefixed to `flash._`; `.Cap(_)` semantics unchanged).
    /// Literals (Int/Hex/Const/Float/String/UValue/NC/…) contain no pin names → kept as-is.
    ///
    /// Only invoked during component method / constructor func body prefixing
    /// (run_component_method / run_component_constructor); sub-module method
    /// bodies are processed directly in the sub-module scope and do not take
    /// this path, so the impact is limited to actuals of peripheral circuits
    /// around leaf devices.
    fn prefix_param_value_with_skip(
        value: &McParamValue,
        inst_name: &str,
        skip: &std::collections::HashSet<String>,
    ) -> McParamValue {
        use crate::semantic::basic::mc_opd::McOpd;
        // Prefix the bare name to inst_name.name; hit skip / contain '.' / empty → return None (keep as-is)
        let prefixed = |name: String| -> Option<McIds> {
            if name.is_empty() || name.contains('.') || skip.contains(&name) {
                None
            } else {
                Some(McIds::from(format!("{inst_name}.{name}").as_str()))
            }
        };
        match value {
            McParamValue::Ids(ids) => match prefixed(ids.to_string()) {
                Some(new_ids) => McParamValue::Ids(new_ids),
                None => value.clone(),
            },
            McParamValue::Opd(McOpd::Id(ids)) => match prefixed(ids.to_string()) {
                Some(new_ids) => McParamValue::Opd(McOpd::Id(new_ids)),
                None => value.clone(),
            },
            McParamValue::Opd(McOpd::Pins(ids)) => match prefixed(ids.to_string()) {
                Some(new_ids) => McParamValue::Opd(McOpd::Pins(new_ids)),
                None => value.clone(),
            },
            // Nested collections (e.g. `[a, b]` actuals) recurse
            McParamValue::Set(vs) => McParamValue::Set(
                vs.iter()
                    .map(|v| Self::prefix_param_value_with_skip(v, inst_name, skip))
                    .collect(),
            ),
            // Others (including Opd(This)/Opd(Uscore) and all literals) → keep as-is
            _ => value.clone(),
        }
    }
}
