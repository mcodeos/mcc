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

use super::expand::{resolve_inst_chain, InstEntry};
use super::McModuleInst;
use crate::db::cmie::cmie::mcb_get_cmie;
use crate::instant::mc_comp::McComponentInst;
use crate::instant::mc_net::{ConnectionInst, InstError, NetPoint, PortInst};
use crate::instant::provenance::ExpansionKind;
use crate::semantic::basic::mc_bus::McBus;
use crate::semantic::basic::mc_endpoint::McEndpoint;
use crate::semantic::basic::mc_param::McParamValue;
use crate::semantic::basic::mc_phrase::McPhrase;
use crate::semantic::common::{ConnDir, IOType, McCMIE};
use crate::semantic::mc_func::McFunction;
use crate::semantic::mc_inst::McInstance;
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
                McPhrase::Series(_, _) => "Series",
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
            crate::db::cmie::cmie::mcb_get_cmie(func_name, &crate::current_uri::get()).is_some();

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
        // like `mcu.setup(...).add_caps().i2c().do_flash(...)`.
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
        // In an example project, `dio1 = DIO.ESD()` registers with CMIE as class.name == "DIO.ESD",
        // but when .mc code uses bare `ESD(...)`, func_name == "ESD", mcb_get_cmie
        // can't find it → returns PassThrough → stmt.rs generates `@?ESD_N` stub
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
        // enablement condition —— but the top-level `mcu.I2C0 -> PULLUP(10k) -> V3V3`
        // chain in an example project, after parsing, has fc.caller set to left-Endpoint (mcu.I2C0)
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
        // Only use direct lookup if it's a Component or Module; otherwise
        // fall through to alias fallback (e.g. "ESD" → "DIO.ESD").
        let cmie = match cmie_raw {
            Some(c @ McCMIE::Component(_)) | Some(c @ McCMIE::Module(_)) => Some(c),
            _ => {
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
                        let canon_ids =
                            crate::semantic::basic::mc_ids::McIds::from(canonical.as_str());
                        let uri = current_uri::get();
                        let result = mcb_get_cmie(&canon_ids, &uri);
                        result
                    }
                    None => None,
                }
            }
        };
        if let Some(cmie) = cmie {
            match cmie {
                McCMIE::Component(comp_def) => {
                    let caller_label = caller.and_then(|c| match c {
                        McPhrase::Endpoint(McEndpoint::Single(iref)) => match &iref.base {
                            McInstance::Label(s) => {
                                let s = s.as_str();
                                // P2-7: check if this Label is part of a dotted class name.
                                // For example, `DIO.ESD(...)` is parsed as caller=Label("DIO")
                                // and func_name="ESD". But "DIO" is not an instance — it's
                                // part of the class name "DIO.ESD". If the Label+func_name
                                // matches the component definition's full name, don't use it
                                // as the component name.
                                let func_name_str = func_name.to_string();
                                let dotted_name = format!("{s}.{func_name_str}");
                                if dotted_name == comp_def.name.to_string() {
                                    // Label is part of the class name, not a user-specified instance name
                                    None
                                } else {
                                    // Label is a user-specified instance name (e.g. R442::RES(1MΩ))
                                    Some(s)
                                }
                            }
                            _ => None,
                        },
                        _ => None,
                    });
                    return self.instantiate_component_construction(
                        comp_def,
                        params,
                        left,
                        right,
                        caller_label,
                    );
                }
                McCMIE::Module(module_def) => {
                    return self.instantiate_module_construction(
                        func_name, module_def, params, left, right,
                    );
                }
                McCMIE::Interface(_) => {
                    // Interface cannot be used as FuncCall construction.
                    // Deliberately do NOT return here: fall through to the
                    // user-func / instance-method lookup below (the caller may
                    // still be a legitimate method call whose name collides
                    // with an Interface type). The fall-through is structural —
                    // code after the match runs only when no arm returned —
                    // so keep it explicit for future readers.
                    mcc_dbg!(
                        "inst::fcall",
                        "[WARN] Cannot instantiate Interface '{func_name}' as FuncCall"
                    );
                }
                McCMIE::Enum(_) => {
                    mcc_dbg!(
                        "inst::fcall",
                        "[WARN] Cannot instantiate Enum '{func_name}' as FuncCall"
                    );
                    return Ok(FuncCallInst::PassThrough);
                }
            }
        } else {
            // ★ P0.5-2: CMIE not found → class not loaded.
            // Record as failed so that resolve_funccall_left/right_points
            // return empty and prevent class-name fragments from entering nets.
            let class_name = func_name.to_string();
            self.failed_classes.insert(class_name);
            return Ok(FuncCallInst::PassThrough);
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
        //     When the caller is a declared sub-module/component instance and
        //     func_name is a method of that instance's type, expand the method
        //     body in the current module scope (with parameter substitution).
        //     Unified scope-chain resolution via InstFindInst (P0-3) supports
        //     arbitrary-depth nesting (module → sub_module → component → …).
        //     `try_resolve_instance_method` returns Ok(None) when the caller
        //     isn't a sub-module/component or the type has no such method —
        //     we then continue to the final PassThrough handling below.
        if let Some(fc) = self.try_resolve_instance_method(&name_str, params, left, right)? {
            return Ok(fc);
        }

        // Unrecognized FuncCall → PassThrough (preserve existing behavior: endpoint direct mapping)
        // Detailed diagnostics for troubleshooting: an unrecognized call usually means a
        // misspelled method/class name, and the downstream `@?CLASS_n` ghost stub
        // would otherwise swallow the whole net without any trace.
        let caller_desc = caller
            .map(|c| match c {
                McPhrase::FuncCall(inner) => format!("FuncCall({})", inner.func_name),
                McPhrase::Endpoint(_) => "Endpoint".into(),
                _ => format!("{:?}", std::mem::discriminant(c)),
            })
            .unwrap_or_else(|| "None".into());
        crate::db::diagnostic::diagnostic::dlog_trace(
            944,
            &format!(
                "instantiate_funccall: module='{}' func='{name_str}' unrecognized → PassThrough | caller={caller_desc} | params={} | left_elems={} | right_elems={}",
                self.name,
                params.len(),
                left.len(),
                right.len(),
            ),
        );
        self.record_warning(
            crate::errcodes::INST_METHOD_FALLBACK,
            crate::errcodes::format_msg(
                crate::errcodes::INST_METHOD_FALLBACK,
                &[&name_str, &self.name],
            ),
        );
        Ok(FuncCallInst::PassThrough)
    }

    /// Phase 2.3: Instance method call (uC.power(...), flash.init(...))
    ///
    /// Resolves the caller scope chain via `resolve_inst_chain` (P0-3 unified
    /// InstFindInst resolution, supporting arbitrary-depth nesting) and expands
    /// `func_name` when it is a method defined on the resolved instance's type.
    ///
    /// Returns:
    ///   - `Ok(Some(inst))` — method found and instantiated
    ///   - `Ok(None)`       — caller isn't a sub-module/component, or the type
    ///     has no such method. The caller decides how to report the miss (the
    ///     final PassThrough warning 944 in `instantiate_funccall` covers it).
    fn try_resolve_instance_method(
        &mut self,
        name_str: &str,
        params: &[McParamValue],
        left: &[McBus],
        right: &[McBus],
    ) -> Result<Option<FuncCallInst>, InstError> {
        // Infer caller scope chain from left endpoint
        let caller_path = left
            .first()
            .map(|elem| elem.name.clone())
            .unwrap_or_default();
        let scope_segments: Vec<String> = caller_path.split('.').map(|s| s.to_string()).collect();
        if scope_segments.is_empty() || scope_segments[0].is_empty() {
            // No caller on the left endpoint — nothing to resolve.
            crate::db::diagnostic::diagnostic::dlog_trace(
                944,
                &format!(
                    "try_resolve_instance_method: no caller scope on left endpoint (caller_path='{caller_path}') — method '{name_str}' cannot be resolved (module='{}')",
                    self.name,
                ),
            );
            return Ok(None);
        }

        // Resolve the full scope chain via InstFindInst. `entry` is owned, so
        // the `&self` borrow ends here and `&mut self` calls below are legal.
        let Some(entry) = resolve_inst_chain(&scope_segments, &*self) else {
            // Scope chain doesn't resolve to a declared instance — the caller
            // is probably a typo or an undeclared instance name.
            crate::db::diagnostic::diagnostic::dlog_trace(
                944,
                &format!(
                    "try_resolve_instance_method: scope chain '{}' does not resolve to any declared instance — method '{name_str}' cannot be resolved (module='{}')",
                    scope_segments.join("."),
                    self.name,
                ),
            );
            return Ok(None);
        };
        let full_scope = scope_segments.join(".");

        match entry {
            InstEntry::SubModule(sub_mod) => {
                // Sub-module's own method (e.g. mcu.some_func())
                if let Some(func) = sub_mod.def.funcs.find(name_str) {
                    let func_clone = func.clone();
                    let func_arity = func_clone.params.iter().count();
                    let call_arity = params.len();
                    // Don't dispatch no-arg version when caller passed args
                    if !(func_arity == 0 && call_arity > 0) {
                        crate::db::diagnostic::diagnostic::dlog_trace(
                            944,
                            &format!(
                                "try_resolve_instance_method: resolved '{full_scope}.{name_str}' (sub-module method) — instantiating in module '{}'",
                                self.name,
                            ),
                        );
                        return Ok(Some(self.instantiate_instance_method(
                            &full_scope,
                            &func_clone,
                            params,
                            left,
                            right,
                        )?));
                    }
                    crate::db::diagnostic::diagnostic::dlog_trace(
                        944,
                        &format!(
                            "try_resolve_instance_method: sub-module '{full_scope}' method '{name_str}' has arity 0 but the call passed {call_arity} args — not dispatched (module='{}')",
                            self.name,
                        ),
                    );
                } else {
                    crate::db::diagnostic::diagnostic::dlog_trace(
                        944,
                        &format!(
                            "try_resolve_instance_method: sub-module '{full_scope}' has no method '{name_str}' (module='{}')",
                            self.name,
                        ),
                    );
                }
            }
            InstEntry::Component(comp) => {
                // Component method (e.g. uC.power(...), mcu.uC.i2c(...))
                if let Some(func) = comp.def.funcs.find(name_str) {
                    let func_clone = func.clone();
                    crate::db::diagnostic::diagnostic::dlog_trace(
                        944,
                        &format!(
                            "try_resolve_instance_method: resolved '{full_scope}.{name_str}' (component method) — instantiating in module '{}'",
                            self.name,
                        ),
                    );
                    return Ok(Some(self.instantiate_instance_method(
                        &full_scope,
                        &func_clone,
                        params,
                        left,
                        right,
                    )?));
                }
                crate::db::diagnostic::diagnostic::dlog_trace(
                    944,
                    &format!(
                        "try_resolve_instance_method: component '{full_scope}' has no method '{name_str}' (module='{}')",
                        self.name,
                    ),
                );
            }
            _ => {
                // Port/Label/Bus — not applicable for func calls. Log the
                // attempted chain for troubleshooting; the final PassThrough
                // warning (944) in `instantiate_funccall` still applies.
                crate::db::diagnostic::diagnostic::dlog_trace(
                    944,
                    &format!(
                        "instantiate_funccall: instance-method chain '{full_scope}' resolved to a port/label/bus terminal — method call '{name_str}' not applicable (module='{}')",
                        self.name,
                    ),
                );
            }
        }
        Ok(None)
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

    /// Collect every Series net-expression reachable from a McParamValue
    /// (including through `Set`), as `(elems, dir)` pairs in order.
    fn collect_series_params<'a>(
        value: &'a McParamValue,
        out: &mut Vec<(&'a [McPhrase], ConnDir)>,
    ) {
        match value {
            McParamValue::Set(values) => {
                for v in values {
                    Self::collect_series_params(v, out);
                }
            }
            McParamValue::Phrase(phrase) => {
                if let McPhrase::Series(elems, dir) = phrase.as_ref() {
                    out.push((elems.as_slice(), *dir));
                }
            }
            _ => {}
        }
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
        // ── Expansion provenance: BuiltinTwopin (§4.1-A3) ──
        let eidx = self.expansion.begin(
            ExpansionKind::BuiltinTwopin,
            Some(inst_name.to_string()),
            func_name.to_string(),
            self.current_call_site(),
            None,
        );
        // ── Diagnostic position ───────────────────────────────────────────
        // Point diagnostics at the current construction site (func-body stmt
        // offset in the func's own file, else the top-level statement start)
        // instead of the file origin — pos 0 renders as `file:1:1`, which
        // hides where the offending call actually is.
        let diag_site = self
            .current_func_span
            .clone()
            .or_else(|| self.current_stmt_span.clone())
            .unwrap_or_else(|| crate::semantic::common::SourcePos::new(self.def_uri.clone(), 0));
        // ── NC is not a pin-level builtin argument ─────────────────────
        // `.Cap(a, NC)` has no meaning: NC is a system keyword valid only in
        // a class construction (`CLASS(NC)`) or a constructor argument list.
        // Report E4176 and leave the element's pins unwired.
        if params.iter().any(|p| matches!(p, McParamValue::NC(_))) {
            let reason = format!(
                "NC is not allowed as a '{func_name}' argument; NC is valid only in CLASS(NC) or a constructor argument list"
            );
            crate::db::diagnostic::diagnostic::diagnostic_log_at(
                crate::errcodes::INST_PARAM_BIND_FAILED,
                crate::db::diagnostic::diagnostic::DiagnosticLevel::Error,
                diag_site.uri.clone(),
                diag_site.offset,
                0,
                &crate::errcodes::format_msg(
                    crate::errcodes::INST_PARAM_BIND_FAILED,
                    &[&inst_name.to_string(), &func_name, &reason],
                ),
                &[],
            );
            self.expansion.end(eidx);
            return Ok(());
        }

        // ── §11.6 strict arity + `[net1, net2]` merge form ──────────────────
        // `.Cap(a, b)` / `.Pullup(sig, rail)` are the canonical 2-arg forms.
        // A single `[net1, net2]` Set argument is an equivalent merge form —
        // the two bracket members occupy the two parameter positions, so
        // `.Cap([a, b])` ≡ `.Cap(a, b)` and `.Cap([_, _])` ≡ `.Cap(_, _)`.
        // `.Cap(x)` (1 bare net arg) and wrong counts are E4176; `_` counts as
        // an argument. The check runs before any side-effecting expansion.
        let last_seg = func_name.rsplit('.').next().unwrap_or(func_name);
        let is_cap = last_seg == "Cap";
        let is_pull =
            last_seg.eq_ignore_ascii_case("Pullup") || last_seg.eq_ignore_ascii_case("Pulldown");
        // Normalized two-endpoint positions (`pair`); the merge form splits the
        // two Set members into their own positions for positional wiring.
        // A single non-Set argument (`.Cap(ldo.VOUT)`, `.Cap(V3V3)`, or the
        // `=>` parameter-prefixing fold result) is a vector reference whose
        // member count is decided at wiring time from its resolved points
        // (§11.6): exactly 2 members fill both endpoint positions, a scalar
        // (1 point) is E4176.
        let mut pair: Vec<McParamValue> = Vec::new();
        let mut single_vector_arg = false;
        if is_cap || is_pull {
            match params.len() {
                2 => pair = params.to_vec(),
                1 => match &params[0] {
                    McParamValue::Set(values) if values.len() == 2 => pair = values.clone(),
                    McParamValue::Set(_) => {} // wrong-size Set → E4176 below
                    _ => {
                        single_vector_arg = true;
                        pair = params.to_vec();
                    }
                },
                _ => {}
            }
            if pair.is_empty() && !single_vector_arg {
                let reason = format!(
                    "'{func_name}' expects exactly 2 network-endpoint arguments, got {} \
                     (strict arity, §11.6; `_` counts as an argument; merge two \
                     endpoints as `[net1, net2]`)",
                    params.len()
                );
                crate::db::diagnostic::diagnostic::diagnostic_log_at(
                    crate::errcodes::INST_PARAM_BIND_FAILED,
                    crate::db::diagnostic::diagnostic::DiagnosticLevel::Error,
                    diag_site.uri.clone(),
                    diag_site.offset,
                    0,
                    &crate::errcodes::format_msg(
                        crate::errcodes::INST_PARAM_BIND_FAILED,
                        &[&inst_name.to_string(), &func_name, &reason],
                    ),
                    &[],
                );
                self.expansion.end(eidx);
                return Ok(());
            }
        } else {
            pair = params.to_vec();
        }

        // 1. Flatten all params into a McBus list, then expand to NetPoint
        // Per-param groups are kept so Pullup/Pulldown can select the rail
        // member (VCC/VDD) instead of the ground member when the rail arg
        // expands into a multi-member DC bus (e.g. `Pullup(_CS, V3V3)` →
        // V3V3 expands to [V3V3.GND, V3V3.VCC], pin2 must land on VCC).
        let mut param_groups: Vec<Vec<NetPoint>> = Vec::new();
        for p in &pair {
            let mut group: Vec<NetPoint> = Vec::new();
            // ── P2-13: Series net-expression param (e.g. `Cap([A -> B], GND)`) ──
            // A net expression like `[dc.VDD_3V3 -> wm7121.VCC]` (possibly
            // Set-wrapped: `Set([Phrase(Series(...))])`) as a two-pin builtin
            // argument is a real sub-line: it must create the internal `A ↔ B`
            // connection (chain each adjacent pair). Previously
            // param_value_to_node_elements only returned the left endpoint and
            // the internal chain was never wired (wm7121.VCC stayed floating).
            let mut series_list: Vec<(&[McPhrase], ConnDir)> = Vec::new();
            Self::collect_series_params(p, &mut series_list);
            for (elems, dir) in &series_list {
                if let Err(e) = self.process_series_branch_inplace(elems, *dir) {
                    self.expansion.end(eidx);
                    return Err(e);
                }
            }
            // Normal expansion: for a Series param `phrase.get_left()` already
            // returns the chain's first element, consistent with the target
            // the builtin element's pin should land on.
            for e in Self::param_value_to_node_elements(p) {
                group.extend(self.expand_node_element(&e));
            }
            param_groups.push(group);
        }
        let targets: Vec<NetPoint> = param_groups.iter().flatten().cloned().collect();
        let _found = self.components.iter().any(|c| c.name == inst_name);

        // ── D7: PULLUP_DEGENERATE detection ──────────────────────────────────
        // For Pullup/Pulldown, the two ends should be (signal, rail).
        // If both explicit targets are non-rail nets, the pullup degenerates
        // into a signal-signal bridge (e.g. SCL-SDA bridge instead of SCL-VDD).
        // (`is_pull` is computed by the §11.6 arity gate above.)
        if is_pull && targets.len() >= 2 {
            // Rail-name check shared by the direct path segment and the
            // declared names re-resolved from a numeric pin id.
            let is_rail_name = |name: &str| -> bool {
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
            };
            let is_rail = |p: &NetPoint| -> bool {
                let name = p.path.rsplit('.').next().unwrap_or(&p.path);
                if is_rail_name(name) || matches!(p.iotype, IOType::Power) {
                    return true;
                }
                // Pin-id form (`uC.5`): component pin alias paths are unified
                // to pin-id paths before this check, so `uC.VDD` arrives as
                // `uC.5`. Re-resolve the numeric pin id to its declared names
                // (and IOType) and re-check the rail patterns.
                if name.chars().all(|c| c.is_ascii_digit()) {
                    if let Some((owner, _)) = p.path.rsplit_once('.') {
                        if let Some(comp) = self.find_component(owner) {
                            if let Some(pin) = comp.def.pins.pins.get(name) {
                                return pin.names.iter().any(|n| is_rail_name(n))
                                    || matches!(pin.iotype, IOType::Power);
                            }
                        }
                    }
                }
                false
            };
            let t1_is_rail = is_rail(&targets[0]);
            let t2_is_rail = is_rail(&targets[1]);
            if !t1_is_rail && !t2_is_rail {
                crate::db::diagnostic::diagnostic::diagnostic_log_at(
                    crate::errcodes::PULLUP_DEGENERATE,
                    crate::db::diagnostic::diagnostic::DiagnosticLevel::Warning,
                    diag_site.uri.clone(),
                    diag_site.offset,
                    0,
                    &crate::errcodes::format_msg(
                        crate::errcodes::PULLUP_DEGENERATE,
                        &[&func_name, &targets[0].path, &targets[1].path],
                    ),
                    &[],
                );
            }
        }

        // ── §11.6 placeholder deferral ─────────────────────────────────────
        // All args are `_` (targets empty): `_` is an explicit argument
        // placeholder, NOT an implicit GND (the old P1-1 rule is abolished,
        // §11.6). In a chain-shunt series the outer chain provides both
        // endpoints and `wire_chain_with_shunts` wires them, so defer here.
        // Standalone (no outer chain), a placeholder with no bindable endpoint
        // is E4176 — the author must write the explicit endpoint or place the
        // element in a chain.
        if targets.is_empty() {
            if self.defer_twopin_placeholders {
                self.expansion.end(eidx);
                return Ok(());
            }
            let reason = format!(
                "'{func_name}' has only `_` placeholder arguments and no outer chain \
                 provides their network endpoints; placeholders do not implicitly \
                 connect to GND (§11.6)"
            );
            crate::db::diagnostic::diagnostic::diagnostic_log_at(
                crate::errcodes::INST_PARAM_BIND_FAILED,
                crate::db::diagnostic::diagnostic::DiagnosticLevel::Error,
                diag_site.uri.clone(),
                diag_site.offset,
                0,
                &crate::errcodes::format_msg(
                    crate::errcodes::INST_PARAM_BIND_FAILED,
                    &[&inst_name.to_string(), &func_name, &reason],
                ),
                &[],
            );
            self.expansion.end(eidx);
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

        // 3. Wire — §11.6 positional pairing
        //    `.Cap(a, b)` / `.Pullup(sig, rail)`: pin1 ← param_groups[0],
        //    pin2 ← param_groups[1]. A param group left empty by a `_`
        //    placeholder defers that pin to the outer chain (no implicit GND);
        //    the all-placeholder case was already handled above
        //    (defer in chain-shunt context, else E4176).
        let mut pin1_pts = param_groups.first().cloned().unwrap_or_default();
        let mut pin2_pts = param_groups.get(1).cloned().unwrap_or_default();

        // ── §11.6 single-arg vector split ─────────────────────────────────
        // `.Cap(ldo.VOUT)` / `.Cap(V3V3)` (a single non-Set arg resolving to
        // a 2-member vector) fills both endpoint positions positionally:
        // member[0] → pin1, member[1] → pin2. A scalar (1 point) or an
        // oversized group (≠2 points) is E4176 (strict arity); a placeholder
        // (0 points) was already handled by the `targets.is_empty()` deferral.
        if single_vector_arg {
            if !param_groups.is_empty() {
                let n = param_groups[0].len();
                if n == 2 {
                    let mut g = param_groups.remove(0);
                    pin2_pts = g.split_off(1);
                    pin1_pts = g;
                } else if n != 0 {
                    let reason = format!(
                        "'{func_name}' single argument resolves to {n} network points, \
                         expected exactly 2 to fill both endpoint positions \
                         (strict arity, §11.6; write a scalar with its explicit \
                         second endpoint: `.Cap(SIG, GND)`)"
                    );
                    crate::db::diagnostic::diagnostic::diagnostic_log_at(
                        crate::errcodes::INST_PARAM_BIND_FAILED,
                        crate::db::diagnostic::diagnostic::DiagnosticLevel::Error,
                        diag_site.uri.clone(),
                        diag_site.offset,
                        0,
                        &crate::errcodes::format_msg(
                            crate::errcodes::INST_PARAM_BIND_FAILED,
                            &[&inst_name.to_string(), &func_name, &reason],
                        ),
                        &[],
                    );
                    self.expansion.end(eidx);
                    return Ok(());
                }
            }
        }

        // ── Pullup/Pulldown (signal, rail) ─────────────────────────────────
        // `Pullup(_CS, V3V3)` has two params: signal and rail. When the rail
        // arg expands into a multi-member DC bus (`V3V3` → [V3V3.GND, V3V3.VCC]),
        // pick the power member (VCC/VDD) for the rail end so the pull-up
        // reaches the rail. Either end may be a `_` placeholder left for the
        // outer chain.
        if is_pull {
            if let Some(s) = pin1_pts.first() {
                let id1 = self.next_conn_id();
                self.add_connection(self.make_conn_with_provenance(
                    id1,
                    vec![pin1, s.clone()],
                    ConnDir::Undirected,
                    None,
                ));
            }
            if let Some(r) = Self::pick_power_point(&pin2_pts) {
                let id2 = self.next_conn_id();
                self.add_connection(self.make_conn_with_provenance(
                    id2,
                    vec![pin2, r],
                    ConnDir::Undirected,
                    None,
                ));
            }
            self.expansion.end(eidx);
            return Ok(());
        }

        // ── Cap ─────────────────────────────────────────────────────────────
        // `.Cap(SIG, GND)` → pin1→SIG, pin2→GND. `.Cap(_, GND)` → pin1 left
        // for the outer chain, pin2→GND. No single-arg implicit pin2→GND
        // (that is E4176 via the §11.6 arity gate above). For a multi-member
        // first group (bus/Set on one argument position) only the first member
        // is bound — a 2-pin element's pin lands on a single net, and fanning a
        // cap pin across a bus would short the bus members together.
        for (pin, pts) in [(pin1, &pin1_pts), (pin2, &pin2_pts)] {
            if let Some(t) = pts.first() {
                let id = self.next_conn_id();
                self.add_connection(self.make_conn_with_provenance(
                    id,
                    vec![pin, t.clone()],
                    ConnDir::Undirected,
                    None,
                ));
            }
        }
        self.expansion.end(eidx);
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
        // ★ P0.5-2 hard gate: if the FuncCall's class failed to instantiate,
        //   return empty to prevent class-name fragments from entering the netlist.
        if let McPhrase::FuncCall(ref fc) = member {
            let class_name = fc.func_name.to_string();
            if self.failed_classes.contains(&class_name) {
                return Ok(Vec::new());
            }
        }

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
                        // Sub-module array (no such case in the example project currently, kept as fallback)
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
        // `<sub>.in` (diagnostic evidence: main.mc:35 has fc_right_len=1, matching
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
            // ── P4 (flash / dcdc): instance-name form .in/.out placeholder leak ──
            // In the `inst(args)` construction call / `inst.method()` form, mc_fcall.rs
            // fills fc.left with `<inst>.in` when caller=None (see mc_fcall.rs:882).
            // When <inst> is a **component instance** (sub-modules have already been
            // rewritten as In ports and continued in the Iter-8.B block above, won't
            // reach here), and that component has no real pin named in, this is a
            // synthetic interface placeholder leak: if wired, it would cross-short
            // the components (CAP_1 / RES_1 …) generated by the constructor func body
            // via the same `<inst>.in` pseudo-node. P0-4.B only
            // blocks class names (CAP/RES) because of `is_ascii_uppercase`, missing
            // lowercase instance names (flash/dcdc), filled in here.
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
            // ★ P0.5-2 fix: use rsplit_once to handle multi-segment class names
            //   (e.g. "DIO.ESD.in" → inst_part="DIO.ESD", suffix="in").
            if let Some((inst_part, suffix)) = e.name.rsplit_once('.') {
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
        // ★ P0.5-2 hard gate: if the FuncCall's class failed to instantiate,
        //   return empty to prevent class-name fragments from entering the netlist.
        if let McPhrase::FuncCall(ref fc) = member {
            let class_name = fc.func_name.to_string();
            if self.failed_classes.contains(&class_name) {
                return Ok(Vec::new());
            }
        }

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
                                    .map(|(name, pid)| {
                                        NetPoint::with_owner(
                                            &format!("{}.{}", owner_name, pid),
                                            owner_name,
                                            IOType::None,
                                        )
                                        .with_member_name(name)
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
                // ── P2-4: Cap() right point return pin1 (signal side) ──
                // .Cap() is a decoupling cap: pin1=signal, pin2=GND.
                // In a chain, the right point should be pin1 (signal pass-through),
                // not pin2 (GND side). Otherwise connect_scalar_to_dc_bus shorts
                // the power rail to GND through the cap's pin2.
                let is_cap_call = match member {
                    McPhrase::FuncCall(fc) => {
                        let fname = fc.func_name.to_string();
                        let last = fname.rsplit('.').next().unwrap_or("");
                        last == "Cap"
                    }
                    _ => false,
                };
                if is_cap_call {
                    if let Some(pin) = comp.get_left_pin() {
                        return Ok(vec![pin]);
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
            // flattened in `mic(V3V3).MIC -> mcu{...}`). The phrase goes
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
        // placeholder `<inst>.out`. Diagnostic evidence (main.mc:35):
        //
        //   [FC-ENTER] func_name='mic' caller_variant=None fc_right_len=1
        //   [resolve_funccall_right] key=... looking up         (not found)
        //   Netlist: __net_7 (2 pts) : mic.out ~ mcu.MIC
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
            // ── P4 (flash / dcdc): instance-name form .out/.in placeholder leak ──
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
            // ★ P0.5-2 fix: use rsplit_once to handle multi-segment class names.
            if let Some((inst_part, suffix)) = e.name.rsplit_once('.') {
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

    /// Pick the power member of a rail endpoint list for Pullup/Pulldown.
    ///
    /// `Pullup(_CS, V3V3)` expands `V3V3` (a DC bus port) into
    /// `[V3V3.GND, V3V3.VCC]`; the pull-up rail end must land on the power
    /// member (VCC/VDD), not the ground member. Preference order:
    /// 1. explicit power-named member (VDD*/VCC*/VIN*/VBAT*/VSYS*/V3V*/V5*)
    /// 2. any non-ground member
    /// 3. first member
    fn pick_power_point(points: &[NetPoint]) -> Option<NetPoint> {
        if points.is_empty() {
            return None;
        }
        let is_ground = |p: &NetPoint| -> bool {
            let name = p.path.rsplit('.').next().unwrap_or(&p.path);
            let u = name.to_uppercase();
            matches!(u.as_str(), "GND" | "VSS" | "AGND" | "DGND" | "PGND")
                || u.starts_with("GND")
                || u.starts_with("VSS")
        };
        let is_power = |p: &NetPoint| -> bool {
            let name = p.path.rsplit('.').next().unwrap_or(&p.path);
            let u = name.to_uppercase();
            u.starts_with("VDD")
                || u.starts_with("VCC")
                || u.starts_with("VIN")
                || u.starts_with("VBAT")
                || u.starts_with("VSYS")
                || u.starts_with("V3V")
                || u.starts_with("V5")
                || matches!(p.iotype, IOType::Power)
        };
        points
            .iter()
            .find(|p| is_power(p))
            .or_else(|| points.iter().find(|p| !is_ground(p)))
            .or_else(|| points.first())
            .cloned()
    }
}
