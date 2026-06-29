// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! FuncCall instantiation dispatch + built-in twopin + endpoint resolution
//!
//! - `FuncCallInst` (enum)        —— FuncCall instantiation result
//! - `instantiate_funccall`       —— FuncCall dispatch entry (with DepthGuard)
//! - `is_builtin_twopin_net_fn` / `wire_builtin_twopin` —— `.Cap/.Pullup/.Pulldown`
//! - `find_user_func`             —— user function lookup
//! - `resolve_funccall_left/right_points` —— FuncCall left/right endpoint resolution
//!
//! The actual component / module / user_func / instance_method instantiation
//! is in `funccall_inst.rs`, and iterated call expansion is in `iterated.rs`.

use super::McModuleInst;
use crate::builder::mcb_get_cmie;
use crate::core::basic::mc_bus::McBus;
use crate::core::basic::mc_param::McParamValue;
use crate::core::basic::mc_phrase::McPhrase;
use crate::core::common::{IOType, McCMIE};
use crate::core::mc_func::McFunction;
use crate::instant::mc_comp::McComponentInst;
use crate::instant::mc_net::{ConnectionInst, InstError, NetPoint, PortInst};
use crate::{current_uri, McIds};

// ============================================================================
// FuncCallInst - FuncCall instantiation result
// ============================================================================

/// Return value of FuncCall instantiation
///
/// Indicates what type of instance `instantiate_funccall` produced
pub(super) enum FuncCallInst {
    /// Produced new components and connections (inline component construction / builtin function)
    Components {
        new_components: Vec<McComponentInst>,
        new_connections: Vec<ConnectionInst>,
    },
    /// Produced sub-module instance and connections (inline module call, Step 2 implementation)
    SubModule {
        inst: McModuleInst,
        new_connections: Vec<ConnectionInst>,
    },
    /// No additional product (endpoint direct mapping, compatible with existing behavior)
    PassThrough,
}

impl McModuleInst {
    // ========================================================================
    // FuncCall dispatch entry
    // ========================================================================

    /// FuncCall dispatch entry
    ///
    /// Look up global definition by func_name, dispatch to different instantiation paths:
    /// 1. Component construction — `CAP(0.1uF)`, `Diode('SMBJ30A')`, `HDR(46)` etc.
    /// 2. Module call — `PowerDomain(V3V3)` etc. (Step 2 implementation)
    /// 3. User function — `func input(sin){...}` expansion (Step 3 implementation)
    /// 4. Built-in function — `rc2()`, `Cap()`, `Pullup()` etc. (Step 4 implementation)
    pub(super) fn instantiate_funccall(
        &mut self,
        func_name: &McIds,
        params: &[McParamValue],
        left: &[McBus],
        right: &[McBus],
        caller: Option<&McPhrase>,
    ) -> Result<FuncCallInst, InstError> {
        // ── Add diagnostic info ──
        let _caller_kind = caller
            .as_ref()
            .map(|c| match c {
                McPhrase::FuncCall(_) => "FuncCall",
                McPhrase::Endpoint(_) => "Endpoint",
                McPhrase::Series(_) => "Series",
                McPhrase::Parallel(_) => "Parallel",
                McPhrase::Group(_) => "Group",
                McPhrase::Transposed(_) => "Transposed",
                McPhrase::Closure(_) => "Closure",
                McPhrase::Lead => "Lead",
                McPhrase::Member(_, _) => "Member",
                McPhrase::Multiple(_) => "Multiple",
            })
            .unwrap_or("None");
        let func_name_str = func_name.to_string();
        let _sub_mod_hit = self.sub_modules.iter().any(|m| m.name == func_name_str);
        let _comp_hit = self.components.iter().any(|c| c.name == func_name_str);
        let _cmie_hit =
            crate::builder::mcb_get_cmie(func_name, &crate::current_uri::get()).is_some();

        let name_str = func_name.to_string();

        // ── Iter-6 P0-3.1: re-call of declared sub-module ──────────────────
        // Syntax: `MIC_SIP mic`  (first declared in declarations stage, no args)
        //         `mic(V3V3).MIC` (re-call in connection line, passing V3V3 as input port arg)
        //
        // func_name at this point is the instance name (parser's context.find_inst hit),
        // a same-name instance can be found in self.sub_modules. Does not go through
        // the CMIE path (which would create a new instance).
        let func_name_str = func_name.to_string();
        if let Some(sub_idx) = self
            .sub_modules
            .iter()
            .position(|m| m.name == func_name_str)
        {
            return self.rebind_submodule_params(sub_idx, params, left, right);
        }

        // ── P0-3 fix ─────────────────────────────────────────────────────
        // Originally, calls with `func_name` containing >3 segments were all
        // returned as PassThrough, dropping legitimate chained method calls
        // like `mcu513.setup(...).capIt().i2c().loadFlash(...)`.
        //
        // Actually, chained calls are recursively assembled into `caller` during
        // AST parsing —— at each level, `func_name` is only a single or double
        // segment (type name). This guard is no longer needed. Recursion depth
        // protection is already covered by the DepthGuard below.

        // ★ Guard: recursion depth protection (to prevent infinite recursive module instantiation)
        // Can use a thread_local counter
        thread_local! {
            static DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
        }
        DEPTH.with(|d| {
            let current = d.get();
            if current > 50 {
                return Err(InstError::Other(format!(
                    "Recursion depth exceeded (>50) for '{name_str}'"
                )));
            }
            d.set(current + 1);
            Ok(())
        })?;

        // Remember to depth -= 1 before function ends (including all return paths)
        // The simplest way is to use Drop guard:
        struct DepthGuard;
        impl Drop for DepthGuard {
            fn drop(&mut self) {
                DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
            }
        }
        let _guard = DepthGuard;

        // ── Iter-6 P0-3.1: re-call of declared sub-module ─────────────────
        // Syntax: `MIC_SIP mic`  (first declared, no args)
        //         `mic(V3V3).MIC` (re-call, passing V3V3 as dc formal arg)
        //
        // func_name is the instance name (from parser), can be found in self.sub_modules.
        // This path **does not create a new instance**, only generates binding
        // connections for existing sub_module's input ports, binding the parameter values.
        //
        // Must be determined before CMIE query —— otherwise `mic` is not a CMIE
        // class name, will fall to P0-4 stub, args are dropped, .MIC member
        // selection collapses.
        if caller.is_none() {
            if let Some(idx) = self
                .sub_modules
                .iter()
                .position(|m| m.name == func_name_str)
            {
                return self.rebind_submodule_params(idx, params, left, right);
            }
        }

        // 1. Look up in global symbol table to see if it's a known Component/Module/Interface
        //
        // ── ★ P0-2: alias fallback ─────────────────────────────────────────────
        // In hbl, `dio1 = DIO.ESD()` registers with CMIE as class.name == "DIO.ESD",
        // but when .mc code uses bare `ESD(...)`, func_name == "ESD", mcb_get_cmie
        // can't find it → returns PassThrough → line.rs generates `@?ESD_N` stub
        // → downstream resolve can't find `@?ESD_N.1` → entire net is swallowed by
        // dropped_nets (viz.md A1).
        //
        // Fix: on direct lookup miss, go through naming::canonicalize_class_alias to
        // map ESD → DIO.ESD and search again. This fallback only kicks in on direct
        // lookup miss, doesn't affect already-correct lookups. After alias hit,
        // the entire instantiation goes through `instantiate_component_construction`,
        // exactly equivalent to explicitly writing `DIO.ESD(...)` → InstTable
        // registers real Pin → resolve no longer loses points.
        //
        // ── ★ ITER-2 P1 fix: bare call PULLUP/PULLDOWN → RES ─────────────────
        // Regular aliases (ESD→DIO.ESD etc.) are independent of "whether there's a
        // caller", because they are all independent CMIE classes. But PULLUP/PULLDOWN
        // are an exception: they can be used either as chain method
        // (`RES(10k).Pullup(sig, rail)`, taken by is_builtin_twopin_net_fn, not
        // entering this path), or as bare call (`PULLUP(10k)` standalone as 2-pin
        // element). The latter currently all gets lost (`@?PULLUP_1.1` not found).
        //
        // Here we **must** use caller.is_none() as the gate: otherwise in chain
        // method form, if outer `.Pullup(...)` is not taken by P1-D (e.g. old
        // version with case mismatch), this path will use the RES alias to
        // construct a new isolated RES instance, putting inner's real RES and this
        // outer's "ghost RES" side by side, replicating the bug we meant to fix.
        //
        // ── ★ ITER-2 fix (first-run feedback): relaxed gate ─────────────────
        //
        // The first version used `caller.is_none()` as the PULLUP→RES alias
        // enablement condition —— but the top-level `mcu513.I2C0 -> PULLUP(10k) -> V3V3`
        // chain in hbl, after parsing, has fc.caller set to left-Endpoint (mcu513.I2C0)
        // by the parser, **not None**, so my alias fallback never activates,
        // and `@?PULLUP_1.1` is still lost (verified: the first version's log
        // doesn't show `[P0-2] PULLUP → RES` at all).
        //
        // What we should really block is **only chain-method form**
        // (`RES(10k).PULLUP(...)`):
        //   - That kind of fc.caller = inner FuncCall (RES construction);
        //   - This path should be taken by P1-D's wire_builtin_twopin, not enter
        //     instantiate_funccall;
        //   - If P1-D misses due to pointer mismatch, and here we use the RES alias
        //     to construct a new RES instance, it will be side by side with the
        //     already-existing RES_X (replicating the bug).
        //
        // Chained connection (`A -> PULLUP(x) -> B`) has caller as Endpoint/Lead/
        // other phrase, **not** FuncCall; in this case using the alias is safe
        // —— no inner FuncCall real component, no double construction.
        //
        // Fix gate: use "caller is not FuncCall" instead of "caller is None".
        let caller_is_funccall = matches!(caller, Some(McPhrase::FuncCall(_)));
        let cmie_raw = mcb_get_cmie(func_name, &current_uri::get());
        let cmie = match cmie_raw {
            Some(c) => Some(c),
            None => {
                let raw_name = func_name.to_string();
                // First try the regular alias (ESD→DIO.ESD etc.), no caller gating
                let standard_alias =
                    crate::vector::graph::naming::canonicalize_class_alias(&raw_name);
                // Then try the bare-call-specific alias (PULLUP/PULLDOWN→RES), only
                // enabled when caller is not FuncCall (i.e. not chain-method form)
                let bare_alias = if !caller_is_funccall {
                    crate::vector::graph::naming::canonicalize_class_alias_bare_call(&raw_name)
                } else {
                    None
                };
                match standard_alias.or(bare_alias) {
                    Some(canonical) => {
                        let canon_ids = crate::core::basic::mc_ids::McIds::from(canonical.as_str());
                        mcb_get_cmie(&canon_ids, &current_uri::get())
                    }
                    None => None,
                }
            }
        };
        if let Some(cmie) = cmie {
            match cmie {
                McCMIE::Component(comp_def) => {
                    return self.instantiate_component_construction(comp_def, params, left, right);
                }
                McCMIE::Module(module_def) => {
                    return self.instantiate_module_construction(
                        func_name, module_def, params, left, right,
                    );
                }
                McCMIE::Interface(_) => {
                    // Interface cannot be used as FuncCall construction
                }
                McCMIE::Enum(_) => {
                    eprintln!("[WARN] Cannot instantiate Enum '{func_name}' as FuncCall");
                    return Ok(FuncCallInst::PassThrough);
                }
            }
        }

        // 2. User function (look up in current module's func table)
        let name_str = func_name.to_string();
        if let Some(func_def) = self.find_user_func(&name_str) {
            // Try to infer the caller instance name from the left endpoint (for 'this' replacement)
            let caller_inst_name = left
                .first()
                .and_then(|elem| elem.name.split('.').next().map(|s| s.to_string()));
            return self.instantiate_user_func(
                func_def,
                params,
                left,
                right,
                caller_inst_name.as_deref(),
            );
        }

        // 2.5 Phase 2.3: Instance method call (uC.power(...), flash.init(...))
        //     When caller is a declared sub-module instance and func_name is a
        //     function defined in that module's type, expand the function body
        //     in the current module scope (with parameter substitution)
        //
        // ── P0-3 fix: explicit scope chain resolution ─────────────────────────────
        // Previously used "first segment name + single funcs table" for dispatch,
        // couldn't distinguish "sub-module method" from "sub-module internal component method".
        //
        // Now resolve the complete scope chain from the left endpoint:
        //   `mcu513.uC.i2c(0x36)` → scope=["mcu513","uC"], method="i2c"
        // Drill down level by level by scope: sub_modules["mcu513"] → .components["uC"]
        //   → .def.funcs.find("i2c")
        // Also do double verification by parameter arity, to avoid mismatching
        // no-arg version with arg version.
        {
            // Infer caller scope chain from left endpoint
            let caller_path = left
                .first()
                .map(|elem| elem.name.clone())
                .unwrap_or_default();
            let scope_segments: Vec<&str> = caller_path.split('.').collect();

            if !scope_segments.is_empty() && !scope_segments[0].is_empty() {
                // ── Depth 1: check if first segment is a sub-module ──
                let first_seg = scope_segments[0];
                let sub_mod_opt = self.sub_modules.iter().find(|m| m.name == first_seg);

                if let Some(sub_mod) = sub_mod_opt {
                    // ── Depth 2+: check for deeper scope (component method within sub-module) ──
                    if scope_segments.len() >= 2 {
                        let inner_seg = scope_segments[1];

                        // Look up component inside sub-module
                        let inner_comp_func = sub_mod
                            .components
                            .iter()
                            .find(|c| c.name == inner_seg)
                            .and_then(|comp| {
                                comp.def.funcs.find(&name_str).map(|f| (comp, f.clone()))
                            });

                        if let Some((_comp, func_clone)) = inner_comp_func {
                            // ── arity double verification ──
                            // Component method (with-args version) takes priority over
                            // module's same-name method (no-args version)
                            let func_arity = func_clone.params.iter().count();
                            let call_arity = params.len();
                            let arity_ok =
                                func_arity == call_arity || (func_arity == 0 && call_arity == 0);

                            if arity_ok || func_arity > 0 {
                                // Use the full scope path prefix (mcu513.uC.) instead of single segment
                                let full_scope = format!("{first_seg}.{inner_seg}");
                                return self.instantiate_instance_method(
                                    &full_scope,
                                    &func_clone,
                                    params,
                                    left,
                                    right,
                                );
                            }
                        }
                    }

                    // ── Depth 1: sub-module's own method ──
                    let module_def = sub_mod.def.clone();
                    if let Some(func) = module_def.funcs.find(&name_str) {
                        let func_clone = func.clone();
                        // ── arity verification: module-level func no-args version vs component-level with-args version ──
                        let func_arity = func_clone.params.iter().count();
                        let call_arity = params.len();
                        // If the caller passed args but module-level func has no args,
                        // and depth 2 path was already tried but didn't hit, don't
                        // mistakenly dispatch to the no-args version
                        if func_arity == 0 && call_arity > 0 {
                            // fall through to PassThrough
                        } else {
                            return self.instantiate_instance_method(
                                first_seg,
                                &func_clone,
                                params,
                                left,
                                right,
                            );
                        }
                    }
                }

                // ── Check if caller is a component (not sub-module) ──
                // e.g. in uC.power(...), uC is a component of the current module
                let comp_def_opt = self
                    .components
                    .iter()
                    .find(|c| c.name == first_seg)
                    .map(|c| c.def.clone());

                if let Some(comp_def) = comp_def_opt {
                    if let Some(func) = comp_def.funcs.find(&name_str) {
                        let func_clone = func.clone();
                        return self.instantiate_instance_method(
                            first_seg,
                            &func_clone,
                            params,
                            left,
                            right,
                        );
                    }
                }
            }
        }

        // Unrecognized FuncCall → PassThrough (preserve existing behavior: endpoint direct mapping)
        Ok(FuncCallInst::PassThrough)
    }

    /// Look up user-defined function in current module's function table
    pub(super) fn find_user_func(&self, name: &str) -> Option<McFunction> {
        self.def.funcs.find(name).cloned()
    }

    // ========================================================================
    // P1-D: Built-in chain wiring helpers
    // ========================================================================

    /// Determine if it's a built-in 2-pin wiring chain function
    ///
    /// These functions have the semantics of "take the caller element's 2 pins,
    /// connect to specified nets per params":
    ///   - `.Cap(a, b)` / `.Cap({a, b})` / `.Cap(a)`: decoupling cap wiring
    ///   - `.Pullup(sig, rail)`: pull-up resistor
    ///   - `.Pulldown(sig, rail)`: pull-down resistor
    ///
    /// ── ★ ITER-2: case-insensitive for Pullup/Pulldown ───────────────────────
    /// The old version strictly distinguished case for all three. `.Cap` must be
    /// distinguished from class name `CAP` very carefully (CAP is a CMIE class,
    /// making it case-insensitive would cause strange syntax like `flash.CAP(...)`
    /// to incorrectly go through wire_builtin_twopin), so keep exact match.
    /// But `.Pullup/.Pulldown` are different: `PULLUP/PULLDOWN` is not a CMIE class,
    /// ITER-2 adds a "bare call → RES" alias fallback for them. If in chain form
    /// the user uses `RES(10k).PULLUP(...)` (uppercase), and this function still
    /// strictly distinguishes case, it would bypass P1-D and go to the alias
    /// fallback, constructing an isolated RES_X on outer (side by side with
    /// inner's real RES, replicating the P0-4 stub bug). Therefore for PULLUP/
    /// PULLDOWN, lift the case constraint — **all case variants** of chain method
    /// are first intercepted by this path and go through wire_builtin_twopin.
    pub(super) fn is_builtin_twopin_net_fn(name: &str) -> bool {
        // Only match the last segment (the call form is always `FOO(...).Cap(...)`,
        // func_name is exactly "Cap")
        let last = name.rsplit('.').next().unwrap_or("");
        // Cap: strictly case-sensitive (avoid false hit on CAP class constructor)
        if last == "Cap" {
            return true;
        }
        // Pullup / Pulldown: case-insensitive (linked with ITER-2's bare-call alias)
        let u = last.to_uppercase();
        matches!(u.as_str(), "PULLUP" | "PULLDOWN")
    }

    /// Wire the 2 pins of the 2-pin element created by the caller per params
    ///
    /// See `is_builtin_twopin_net_fn` documentation for the calling convention.
    pub(super) fn wire_builtin_twopin(
        &mut self,
        inst_name: &str,
        params: &[McParamValue],
        func_name: &str,
    ) -> Result<(), InstError> {
        // 1. Flatten all params into a McBus list, then expand to NetPoint
        let mut elements: Vec<McBus> = Vec::new();
        for p in params {
            elements.extend(Self::param_value_to_node_elements(p));
        }
        let mut targets: Vec<NetPoint> = Vec::new();
        for e in &elements {
            targets.extend(self.expand_node_element(e));
        }
        let _found = self.components.iter().any(|c| c.name == inst_name);

        // ── D7: PULLUP_DEGENERATE detection ──────────────────────────────────
        // For Pullup/Pulldown, the two ends should be (signal, rail).
        // If both explicit targets are non-rail nets, the pullup degenerates
        // into a signal-signal bridge (e.g. SCL-SDA bridge instead of SCL-VDD).
        let last_seg = func_name.rsplit('.').next().unwrap_or(func_name);
        let is_pull = last_seg.eq_ignore_ascii_case("Pullup")
            || last_seg.eq_ignore_ascii_case("Pulldown");
        if is_pull && targets.len() >= 2 {
            let is_rail = |p: &NetPoint| -> bool {
                let name = p.path.rsplit('.').next().unwrap_or(&p.path);
                let upper = name.to_uppercase();
                // Power rails
                upper.starts_with("VDD")
                    || upper.starts_with("VCC")
                    || upper.starts_with("V3V")
                    || upper.starts_with("V5")
                    || upper.starts_with("V33")
                    || upper.starts_with("VIN")
                    || upper.starts_with("VBAT")
                    || upper.starts_with("VSYS")
                    || upper.starts_with("VREF")
                    // Ground rails
                    || upper == "GND"
                    || upper == "VSS"
                    || upper == "AGND"
                    || upper == "DGND"
                    || upper == "PGND"
                    || matches!(p.iotype, IOType::Power)
            };
            let t1_is_rail = is_rail(&targets[0]);
            let t2_is_rail = is_rail(&targets[1]);
            if !t1_is_rail && !t2_is_rail {
                crate::builder::diagnostic::diagnotic_log(
                    2007,
                    crate::builder::diagnostic::DiagnosticLevel::Warning,
                    0,
                    0,
                    &format!(
                        "PULLUP_DEGENERATE: '{}' both ends are non-rail nets ({} ~ {}). \
                         Pullup/Pulldown may have degenerated into a signal-signal bridge instead of (signal, rail).",
                        func_name, targets[0].path, targets[1].path
                    ),
                    &[],
                );
            }
        }

        // `.Cap(_)` → all args are underscores → pin2 implicitly connects to GND
        // ── P1-1: implicit GND rule ─────────────────────────────────────
        // Rules doc §2.2: `_` is the "star connection center point", in decoupling
        // scenarios the center is GND. `.Cap(_)` → pin1 left for the outer chain
        // upstream, pin2 connects to GND.
        if targets.is_empty() {
            // Try to get pin2 from the real component; for @? stub use synthetic pin
            let pin2 = self
                .components
                .iter()
                .find(|c| c.name == inst_name)
                .and_then(|c| c.get_right_pin())
                .unwrap_or_else(|| {
                    NetPoint::with_owner(&format!("{inst_name}.2"), inst_name, IOType::None)
                });
            let gnd = self.node_to_netpoint(&McBus::new("GND"));
            let id = self.next_conn_id();
            self.connections
                .push(ConnectionInst::new(id, vec![pin2, gnd]));
            return Ok(());
        }

        // 2. Find the two pins of the caller element
        //    For @? stub (unrecognized class name) use synthetic pin1/pin2
        let (pin1, pin2) = match self.components.iter().find(|c| c.name == inst_name) {
            Some(c) => {
                let p1 = c.get_left_pin().unwrap_or_else(|| {
                    NetPoint::with_owner(&format!("{inst_name}.1"), inst_name, IOType::None)
                });
                let p2 = c.get_right_pin().unwrap_or_else(|| {
                    NetPoint::with_owner(&format!("{inst_name}.2"), inst_name, IOType::None)
                });
                (p1, p2)
            }
            None => {
                // @? stub or not-found component: synthesize .1/.2 pins
                let p1 = NetPoint::with_owner(&format!("{inst_name}.1"), inst_name, IOType::None);
                let p2 = NetPoint::with_owner(&format!("{inst_name}.2"), inst_name, IOType::None);
                (p1, p2)
            }
        };

        // 3. Wire
        //    1 target → pin1 → target, pin2 → GND (implicit)
        //    ≥2 targets → pin1 → targets[0], pin2 → targets[1]
        //
        // ── P1-1: `.Cap(x)` single-arg case, pin2 implicitly connects to GND ──────────────
        // Decoupling cap's other pin fixed to GND.
        let mut it = targets.into_iter();
        let t1 = it.next().unwrap();
        match it.next() {
            None => {
                // .Cap(x) → pin1 → x, pin2 → GND
                let id1 = self.next_conn_id();
                self.connections
                    .push(ConnectionInst::new(id1, vec![pin1, t1]));
                let gnd = self.node_to_netpoint(&McBus::new("GND"));
                let id2 = self.next_conn_id();
                self.connections
                    .push(ConnectionInst::new(id2, vec![pin2, gnd]));
            }
            Some(t2) => {
                let id1 = self.next_conn_id();
                self.connections
                    .push(ConnectionInst::new(id1, vec![pin1, t1]));
                let id2 = self.next_conn_id();
                self.connections
                    .push(ConnectionInst::new(id2, vec![pin2, t2]));
            }
        }
        Ok(())
    }

    // ========================================================================
    // FuncCall endpoint resolution (unified entry for get_left/right_points)
    // ========================================================================

    /// Resolve FuncCall's left endpoint
    ///
    /// Unify the left-endpoint return logic for components/sub-modules/user
    /// functions/built-in functions. Look up the instance associated with this
    /// FuncCall via `auto_inst_map`, return the corresponding pin or port.
    ///
    /// # Iteration 3: multi-pin IO-aware
    /// - Multi-pin component (with IO annotation): return all input pins
    /// - Multi-pin component (no IO annotation): return get_left_pin (compatible fallback)
    /// - 2-pin component: return get_left_pin
    pub(super) fn resolve_funccall_left_points(
        &mut self,
        member: &McPhrase,
        left: &[McBus],
    ) -> Result<Vec<NetPoint>, InstError> {
        let key = Self::member_key(member);
        if let Some(inst_name) = self.auto_inst_map.get(&key).cloned() {
            // ── Iter-1.2 ────────────────────────────────────────────────
            // Encoding forms like `@@ARRAY:cap4,cap5`: iterated call / array-form
            // caller produces multiple instances. Collect the left pin of **each**
            // instance, so that the chain's 2×1 vs 2×1 connection can be correctly
            // dispatched by create_connection.
            if let Some(list_str) = inst_name.strip_prefix("@@ARRAY:") {
                let mut points = Vec::new();
                for n in list_str.split(',').filter(|s| !s.is_empty()) {
                    if let Some(comp) = self.components.iter().find(|c| c.name == n) {
                        if comp.is_multi_pin() && comp.has_io_annotations() {
                            let ins = comp.get_input_pins();
                            if !ins.is_empty() {
                                points.extend(ins);
                                continue;
                            }
                            let pwr = comp.get_power_pins();
                            if !pwr.is_empty() {
                                points.push(pwr[0].clone());
                                continue;
                            }
                        }
                        if let Some(pin) = comp.get_left_pin() {
                            points.push(pin);
                        }
                    } else if let Some(sub) = self.sub_modules.iter().find(|s| s.name == n) {
                        // Sub-module array (no such case in hbl currently, kept as fallback)
                        for p in sub.ports.iter().filter(|p| matches!(p.iotype, IOType::In)) {
                            points.push(NetPoint::with_owner(
                                &format!("{}.{}", sub.name, p.name),
                                &sub.name,
                                IOType::In,
                            ));
                        }
                    }
                }
                return Ok(points);
            }

            // Component?
            if let Some(comp) = self.components.iter().find(|c| c.name == inst_name) {
                if comp.is_multi_pin() && comp.has_io_annotations() {
                    // ★ Multi-pin IO-aware: return all input pins
                    let input_pins = comp.get_input_pins();
                    if !input_pins.is_empty() {
                        return Ok(input_pins);
                    }
                    // fallback: when no input pins, use power[0]
                    let pwr = comp.get_power_pins();
                    if !pwr.is_empty() {
                        return Ok(vec![pwr[0].clone()]);
                    }
                }
                // 2-pin or no IO annotation
                if let Some(pin) = comp.get_left_pin() {
                    return Ok(vec![pin]);
                }
            }
            // Sub-module? → return input port list
            if let Some(sub) = self.sub_modules.iter().find(|s| s.name == inst_name) {
                return Ok(sub
                    .ports
                    .iter()
                    .filter(|p| matches!(p.iotype, IOType::In))
                    .map(|p| {
                        NetPoint::with_owner(
                            &format!("{}.{}", sub.name, p.name),
                            &sub.name,
                            IOType::In,
                        )
                    })
                    .collect());
            }
            // Synthetic stub (P0-4)? Unrecognized class name FuncCall uses independent stub endpoint
            if inst_name.starts_with("@?") {
                return Ok(vec![NetPoint::with_owner(
                    &format!("{inst_name}.1"),
                    &inst_name,
                    IOType::None,
                )]);
            }
        }
        // ── Iter-8.B ────────────────────────────────────────────────────
        // Placeholder recognition (left mirrors right):
        // mc_fcall.rs in the `mic(V3V3).MIC` form with caller=None loses the
        // `.MIC` named information, fc.left is filled with the generic placeholder
        // `<sub>.in` (diagnostic evidence: hbl.mc:35 has fc_right_len=1, matching
        // the `mic.out` naming in the netlist).
        // When auto_inst_map doesn't hit and falls through to here, check if
        // there's a `<inst>.in` form placeholder in left and <inst> is a sub-module
        // of the current module; if so, replace with all that sub-module's In
        // ports (expand N×1 by bus_members).
        //
        // Consistent with expand_port_lanes, only do lane expansion for IOType::Out;
        // here because left takes In ports, In ports still only expand to single
        // points (to avoid regression). In other words, left's placeholder
        // replacement = port list (each port still 1 point), not like right
        // that truly activates N×1 lane expansion.
        let mut left_points: Vec<NetPoint> = Vec::new();
        for e in left {
            if let Some((inst_part, suffix)) = e.name.split_once('.') {
                if suffix == "in" {
                    if let Some(sub) = self.sub_modules.iter().find(|s| s.name == inst_part) {
                        let sub_name = sub.name.clone();
                        for p in sub.ports.iter().filter(|p| matches!(p.iotype, IOType::In)) {
                            left_points.push(NetPoint::with_owner(
                                &format!("{}.{}", sub_name, p.name),
                                &sub_name,
                                p.iotype.clone(),
                            ));
                        }
                        continue;
                    }
                }
            }
            // ── P4 (flash / lp322dcdc): instance-name form .in/.out placeholder leak ──
            // In the `inst(args)` construction call / `inst.method()` form, mc_fcall.rs
            // fills fc.left with `<inst>.in` when caller=None (see mc_fcall.rs:882).
            // When <inst> is a **component instance** (sub-modules have already been
            // rewritten as In ports and continued in the Iter-8.B block above, won't
            // reach here), and that component has no real pin named in, this is a
            // synthetic interface placeholder leak: if wired, it would cross-short
            // the components (CAP_1 / RES_1 …) generated by the constructor func body
            // via the same `<inst>.in` pseudo-node (CLAUDE.md P4). P0-4.B only
            // blocks class names (CAP/RES) because of `is_ascii_uppercase`, missing
            // lowercase instance names (flash/lp322dcdc), filled in here.
            if let Some((inst_part, suffix)) = e.name.split_once('.') {
                if (suffix == "in" || suffix == "out")
                    && self
                        .find_component(inst_part)
                        .is_some_and(|c| c.get_pin(suffix).is_none())
                {
                    continue;
                }
            }
            // Not a placeholder, go to original fallback
            // ── P0-4.B: filter class-name placeholder leak ──────────────────────────
            // mc_fcall.rs generates `{CLASS}.in`/`{CLASS}.out` placeholders when
            // caller=None. If CLASS is not an existing instance/port/bus, these are
            // class-name leaks; all anonymous components of the same class sharing
            // the same label would cause union-find short. Detect and filter these
            // ghost nodes.
            if let Some((inst_part, suffix)) = e.name.split_once('.') {
                if (suffix == "in" || suffix == "out")
                    && !inst_part.is_empty()
                    && inst_part
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_uppercase())
                    && self.find_component(inst_part).is_none()
                    && self.find_submodule(inst_part).is_none()
                    && !self.is_port(inst_part)
                    && !self.is_bus(inst_part)
                {
                    continue;
                }
            }
            left_points.extend(self.expand_node_element(e));
        }
        Ok(left_points)
    }

    /// Resolve FuncCall's right endpoint
    ///
    /// # Iteration 3: multi-pin IO-aware
    /// - Multi-pin component (with IO annotation): return all output pins
    /// - Multi-pin component (no IO annotation): return get_right_pin (compatible fallback)
    /// - 2-pin component: return get_right_pin
    pub(super) fn resolve_funccall_right_points(
        &mut self,
        member: &McPhrase,
        right: &[McBus],
    ) -> Result<Vec<NetPoint>, InstError> {
        let key = Self::member_key(member);
        // ── P1-diag: print right element content ──────────────────────────────
        if let Some(inst_name) = self.auto_inst_map.get(&key).cloned() {
            // ── P2-2: @@RETURN_EP decoding ─────────────────────────────────
            // instantiate_instance_method encodes the endpoint path for methods
            // returning Endpoint (e.g. `@@RETURN_EP:X6.XTAL`). Resolve directly
            // to that path's NetPoint, without going through component pin /
            // sub-module port lookup.
            if let Some(ep_path) = inst_name.strip_prefix("@@RETURN_EP:") {
                // Expand the component bus/interface return endpoint into member
                // pins, symmetric with the sub-module Out bus port expansion
                // below (lines 814-836)
                if let Some((owner_name, port_name)) = ep_path.split_once('.') {
                    if let Some(comp) = self.components.iter().find(|c| c.name == owner_name) {
                        if let Some(pids) = comp.find_bus_port_pin_ids(port_name) {
                            if pids.len() >= 2 {
                                return Ok(pids
                                    .iter()
                                    .map(|pid| {
                                        NetPoint::with_owner(
                                            &format!("{}.{}", owner_name, pid),
                                            owner_name,
                                            IOType::None,
                                        )
                                    })
                                    .collect());
                            }
                        }
                    }
                }
                // Otherwise fall back to the original single point
                let owner = ep_path.split('.').next().unwrap_or(ep_path);
                return Ok(vec![NetPoint::with_owner(ep_path, owner, IOType::None)]);
            }

            // ── Iter-1.2 ────────────────────────────────────────────────
            // Symmetric with resolve_funccall_left_points: @@ARRAY decoding
            if let Some(list_str) = inst_name.strip_prefix("@@ARRAY:") {
                let mut points = Vec::new();
                for n in list_str.split(',').filter(|s| !s.is_empty()) {
                    if let Some(comp) = self.components.iter().find(|c| c.name == n) {
                        if comp.is_multi_pin() && comp.has_io_annotations() {
                            let outs = comp.get_output_pins();
                            if !outs.is_empty() {
                                points.extend(outs);
                                continue;
                            }
                            let pwr = comp.get_power_pins();
                            if pwr.len() >= 2 {
                                points.push(pwr[1].clone());
                                continue;
                            }
                        }
                        if let Some(pin) = comp.get_right_pin() {
                            points.push(pin);
                        }
                    } else if let Some(sub) = self.sub_modules.iter().find(|s| s.name == n) {
                        // ── Iter-8 ──────────────────────────────────────
                        // N×1 bus port expansion: sub-modules under array-form
                        // caller go through the same expansion logic. See the
                        // single-instance sub branch comment below for details.
                        let sub_name = sub.name.clone();
                        for p in sub.ports.iter().filter(|p| matches!(p.iotype, IOType::Out)) {
                            if p.is_bus_port() {
                                for m in &p.bus_members {
                                    points.push(NetPoint::with_owner(
                                        &format!("{}.{}.{}", sub_name, p.name, m),
                                        &sub_name,
                                        p.iotype.clone(),
                                    ));
                                }
                            } else {
                                points.push(NetPoint::with_owner(
                                    &format!("{}.{}", sub_name, p.name),
                                    &sub_name,
                                    p.iotype.clone(),
                                ));
                            }
                        }
                    }
                }
                return Ok(points);
            }

            // Component?
            if let Some(comp) = self.components.iter().find(|c| c.name == inst_name) {
                if comp.is_multi_pin() && comp.has_io_annotations() {
                    // ★ Multi-pin IO-aware: return all output pins
                    let output_pins = comp.get_output_pins();
                    if !output_pins.is_empty() {
                        return Ok(output_pins);
                    }
                    // fallback: when no output pins, use power[1] (GND)
                    let pwr = comp.get_power_pins();
                    if pwr.len() >= 2 {
                        return Ok(vec![pwr[1].clone()]);
                    }
                }
                // 2-pin or no IO annotation
                if let Some(pin) = comp.get_right_pin() {
                    return Ok(vec![pin]);
                }
            }
            // Sub-module? → return output port list
            //
            // ── Iter-8 ───────────────────────────────────────────────────
            // N×1 bus port expansion: if the Out port declares ≥2 bus_members
            // (e.g. MIC_SIP's `out MIC{P,N}::ADC.DIFF()`), expand into N
            // independent NetPoints per declared lane, letting the upper
            // create_connection use the rules doc §10.4 "[N×1] vs [N×1]"
            // positional connection (instead of flattening the entire port
            // to a single point).
            //
            // This is the main fix for bugfix_report error 2 (mic.MIC being
            // flattened in `mic(V3V3).MIC -> mcu513{...}`). The phrase goes
            // through the FuncCall path, not Endpoint(Bus), so the expansion
            // added in points.rs::expand_port_lanes within get_left_points/
            // get_right_points doesn't fire here, requiring corresponding
            // expansion inside resolve_funccall_*_points.
            //
            // Same safety policy as points.rs::expand_port_lanes: only expand
            // IOType::Out. In/InOut ports still return single points, to
            // avoid the engineering convention usage `usbsocket.vin -> V5V`
            // (whole port to single label) becoming a "broadcast to POWER_SYS
            // and GND" power short regression.
            // (Right-end resolution only filters Out ports, so we only expand Out here.)
            if let Some(sub) = self.sub_modules.iter().find(|s| s.name == inst_name) {
                let sub_name = sub.name.clone();
                let mut points: Vec<NetPoint> = Vec::new();
                for p in sub.ports.iter().filter(|p| matches!(p.iotype, IOType::Out)) {
                    if p.is_bus_port() {
                        // N×1 bus: expand into multiple lanes
                        for m in &p.bus_members {
                            points.push(NetPoint::with_owner(
                                &format!("{}.{}.{}", sub_name, p.name, m),
                                &sub_name,
                                p.iotype.clone(),
                            ));
                        }
                    } else {
                        // Scalar port: single point
                        points.push(NetPoint::with_owner(
                            &format!("{}.{}", sub_name, p.name),
                            &sub_name,
                            p.iotype.clone(),
                        ));
                    }
                }
                return Ok(points);
            }
            // Synthetic stub (P0-4)? Unrecognized class name FuncCall uses independent stub endpoint
            if inst_name.starts_with("@?") {
                return Ok(vec![NetPoint::with_owner(
                    &format!("{inst_name}.2"),
                    &inst_name,
                    IOType::None,
                )]);
            }
        }
        // ── Iter-8.B ────────────────────────────────────────────────────
        // Placeholder recognition (core fix, main path for bugfix_report error 2):
        //
        // mc_fcall.rs in the `mic(V3V3).MIC` form with caller=None loses the
        // `.MIC` named information, fc.right is filled with the generic
        // placeholder `<inst>.out`. Diagnostic evidence (hbl.mc:35):
        //
        //   [FC-ENTER] func_name='mic' caller_variant=None fc_right_len=1
        //   [resolve_funccall_right] key=... looking up         (not found)
        //   Netlist: __net_7 (2 pts) : mic.out ~ mcu513.MIC
        //
        // When auto_inst_map doesn't hit and falls through to here, replace
        // the `<inst>.out` form placeholder in right (where <inst> is a
        // sub-module name of the current module) with all that sub-module's
        // Out ports, while expanding N×1 bus ports (e.g. MIC_SIP's
        // `out MIC{P,N}::ADC.DIFF()`) into P/N independent lanes per
        // PortInst.bus_members.
        //
        // This is consistent with the IOType::Out safety restriction in
        // points.rs::expand_port_lanes: only expand Out ports (In/InOut
        // not expanded), to avoid the engineering convention usage
        // `usbsocket.vin -> V5V` (whole port to single label) becoming
        // a "broadcast to POWER_SYS and GND" power short regression.
        // (Right-end resolution only filters Out anyway, so the safety
        // constraint naturally holds here.)
        let mut right_points: Vec<NetPoint> = Vec::new();
        for e in right {
            if let Some((inst_part, suffix)) = e.name.split_once('.') {
                if suffix == "out" {
                    if let Some(sub) = self.sub_modules.iter().find(|s| s.name == inst_part) {
                        let sub_name = sub.name.clone();
                        for p in sub
                            .ports
                            .iter()
                            .filter(|p| matches!(p.iotype, IOType::Out | IOType::InOut))
                        {
                            // ── P1 fix: extract member list from port.name or bus_members
                            // bus_members may be empty but name contains {P, N} format members
                            let members: Vec<String> = if p.is_bus_port() {
                                p.bus_members.clone()
                            } else if let Some(brace_start) = p.name.find('{') {
                                // Parse {members} from name: "MIC{P, N}" → ["P", "N"]
                                let brace_end = p.name.find('}').unwrap_or(p.name.len());
                                let members_str = &p.name[brace_start + 1..brace_end];
                                members_str
                                    .split(',')
                                    .map(|s| s.trim().to_string())
                                    .filter(|s| !s.is_empty())
                                    .collect()
                            } else {
                                vec![]
                            };

                            // Extract clean port name (remove {members})
                            let clean_name = if let Some(brace_pos) = p.name.find('{') {
                                &p.name[..brace_pos]
                            } else {
                                &p.name
                            };

                            if members.len() >= 2 {
                                for m in &members {
                                    right_points.push(NetPoint::with_owner(
                                        &format!("{sub_name}.{clean_name}.{m}"),
                                        &sub_name,
                                        p.iotype.clone(),
                                    ));
                                }
                            } else {
                                // Scalar Out port: single point
                                right_points.push(NetPoint::with_owner(
                                    &format!("{sub_name}.{clean_name}"),
                                    &sub_name,
                                    p.iotype.clone(),
                                ));
                            }
                        }
                        continue;
                    }
                }
            }
            // ── P4 (flash / lp322dcdc): instance-name form .out/.in placeholder leak ──
            // Mirror of left: in `inst(args)` / `inst.method()` with caller=None,
            // mc_fcall.rs:891 fills fc.right with `<inst>.out`. When <inst> is a
            // component instance (sub-modules have already been rewritten as
            // Out ports and continued in the block above), and has no real pin
            // named out, filter the synthetic interface placeholder to prevent
            // cross-shorting with body components.
            if let Some((inst_part, suffix)) = e.name.split_once('.') {
                if (suffix == "in" || suffix == "out")
                    && self
                        .find_component(inst_part)
                        .is_some_and(|c| c.get_pin(suffix).is_none())
                {
                    continue;
                }
            }
            // Not a `<sub>.out` placeholder, go to original fallback
            // ── P0-4.B: filter class-name placeholder leak (mirror of left)─────────────
            if let Some((inst_part, suffix)) = e.name.split_once('.') {
                if (suffix == "in" || suffix == "out")
                    && !inst_part.is_empty()
                    && inst_part
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_uppercase())
                    && self.find_component(inst_part).is_none()
                    && self.find_submodule(inst_part).is_none()
                    && !self.is_port(inst_part)
                    && !self.is_bus(inst_part)
                {
                    continue;
                }
            }
            // ── P1: use expand_node_element instead of node_to_netpoint ─────
            // expand_node_element has built-in:
            //   1. McBus member expansion (e.g. mic.MIC{P,N} → [mic.MIC.P, mic.MIC.N])
            //   2. expand_port_lanes (N×1 bus port expansion)
            //   3. Final fallback to node_to_netpoint
            // This fixes the issue of mic.MIC{P,N} being compressed to a single point.
            right_points.extend(self.expand_node_element(e));
        }
        Ok(right_points)
    }

    // ========================================================================
    // P0-3.2: rebind_submodule_params - re-call of declared sub-module
    // ========================================================================

    fn rebind_submodule_params(
        &mut self,
        sub_idx: usize,
        params: &[McParamValue],
        _left: &[McBus],
        _right: &[McBus],
    ) -> Result<FuncCallInst, InstError> {
        // ── Root cause A fix ───────────────────────────────────────────────
        // The old logic had two errors:
        //   (a) Only filter `IOType::In` -> missed `dc{VDD_3V3,GND}` such
        //       iotype=None bus power ports -> input_ports empty -> nothing
        //       connected;
        //   (b) Treat ports as **scalar** (dst = inst.port_name, not expanding
        //       members) -> `dc.VDD_3V3`/`dc.GND` in sub-module body never
        //       connect -> mic floats.
        //
        // Now uniformly delegate to phases.rs::bind_call_args_to_ports: it
        // takes members by bus_members/`{…}`/`[…]`, does the DC single-rail
        // connection for "scalar arg -> [rail,gnd]" (rail ← arg, gnd ← GND),
        // named ports simultaneously connect bare `inst.MEMBER` and dotted
        // `inst.base.MEMBER` two label forms, consistent with the inject
        // convention from inject_port_member_labels in sub-modules.
        let (inst_name, ports): (String, Vec<PortInst>) = {
            let sub = &self.sub_modules[sub_idx];
            (sub.name.clone(), sub.ports.clone())
        };

        let new_connections = self.bind_call_args_to_ports(&inst_name, &ports, params);

        Ok(FuncCallInst::Components {
            new_components: Vec::new(),
            new_connections,
        })
    }
}
