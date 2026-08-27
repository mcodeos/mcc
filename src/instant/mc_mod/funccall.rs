// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! FuncCall instantiation dispatch + built-in twopin + endpoint resolution
//!
//! - `FuncCallInst` (enum)        —— FuncCall instantiation result
//! - `instantiate_funccall`       —— FuncCall dispatch entry (with DepthGuard)
//! - `is_builtin_twopin_net_fn` / `wire_builtin_twopin` —— `.Cap/.Pullup/.Pulldown`
//! - `find_user_func`             —— user function lookup
//! - `resolve_funccall_face`       —— FuncCall chain-member face resolution
//!   (func-return-design §6.2: return face for case ②, instance face for case ①)
//!
//! The actual component / module / user_func / instance_method instantiation
//! is in `funccall_inst.rs`, and iterated call expansion is in `iterated.rs`.

use super::expand::{resolve_inst_chain, InstEntry};
use super::McModuleInst;
use crate::db::cmie::cmie::mcb_get_cmie;
use crate::instant::mc_comp::McComponentInst;
use crate::instant::mc_net::{ConnectionInst, InstError, NetPoint, PortInst};
use crate::semantic::basic::mc_bus::McBus;
use crate::semantic::basic::mc_endpoint::McEndpoint;
use crate::semantic::basic::mc_param::McParamValue;
use crate::semantic::basic::mc_phrase::McPhrase;
use crate::semantic::common::{IOType, McCMIE};
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

/// Which mouth of a stereo node a face resolution targets.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum FaceSide {
    Left,
    Right,
}

impl FaceSide {
    fn is_left(self) -> bool {
        matches!(self, FaceSide::Left)
    }
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
    // FuncCall endpoint resolution (unified entry for get_left/right_points)
    // ========================================================================

    /// Which mouth of a stereo node a face resolution targets.

    /// Unified FuncCall chain-member face resolution (func-return-design §6.2).
    ///
    /// Replaces the abolished `resolve_funccall_left_points` /
    /// `resolve_funccall_right_points` (the "left/right point take" operations,
    /// §6.1). A FuncCall chain member's connection face comes from the
    /// **func return value**, never
    /// from instance pins:
    ///
    /// - **case ② (func return)** — both mouths are the *same* return face
    ///   (symmetric stereo node). `instantiate_instance_method` writes it into
    ///   auto_inst_map:
    ///   - `@@RETURN_EP:{inst}.{port}` — instance bus-port return (e.g. `X6.XTAL{X1,X2}`);
    ///   - `@@RETURN_NETS:{n1};{n2}` — net / net-list / group return (substituted names).
    /// - **case ① (bare construction / implicit `this`)** — the face is the
    ///   instance's own default shape: left = entry face (twopin pin1 / input
    ///   pins / sub In ports), right = exit face (pin2 / output pins / sub Out ports).
    ///
    /// No auto_inst_map hit → fall back to the phrase's own interface buses,
    /// filtering synthetic `.in`/`.out` placeholders (robustness for callers
    /// that never instantiated through this module).
    pub(super) fn resolve_funccall_face(
        &mut self,
        member: &McPhrase,
        side: FaceSide,
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
            // ── case ②: return face (both mouths symmetric) ────────────────
            // instantiate_instance_method encoded the func's return value;
            // decode it to the same point set for left and right.
            if let Some(ep_path) = inst_name.strip_prefix("@@RETURN_EP:") {
                return self.decode_return_endpoint(ep_path);
            }
            if let Some(nets) = inst_name.strip_prefix("@@RETURN_NETS:") {
                return self.decode_return_nets(nets);
            }
            // ── @@ARRAY: iterated / array-form caller produces multiple
            //    instances (left = each one's entry face, right = each one's exit face).
            if let Some(list_str) = inst_name.strip_prefix("@@ARRAY:") {
                return self.decode_array_face(list_str, side);
            }
            // ── case ①: instance own face ─────────────────────────────────
            if let Some(comp) = self.components.iter().find(|c| c.name == inst_name) {
                return self.component_own_face(comp, side);
            }
            if let Some(sub) = self.sub_modules.iter().find(|s| s.name == inst_name) {
                return self.submodule_own_face(sub, side);
            }
            // Synthetic stub (P0-4)? Unrecognized class name FuncCall uses independent stub endpoint
            if inst_name.starts_with("@?") {
                let which = if side.is_left() { ".1" } else { ".2" };
                return Ok(vec![NetPoint::with_owner(
                    &format!("{inst_name}{which}"),
                    &inst_name,
                    IOType::None,
                )]);
            }
        }
        // ── fallback: the phrase's own interface buses (placeholder filtering) ──
        let buses: Vec<McBus> = match (member, side) {
            (McPhrase::FuncCall(fc), FaceSide::Left) => fc.left.clone(),
            (McPhrase::FuncCall(fc), FaceSide::Right) => fc.right.clone(),
            _ => return Ok(Vec::new()),
        };
        self.resolve_face_from_buses(&buses, side)
    }

    /// Decode a `@@RETURN_EP:{inst}.{port}` encoded return face: the path names
    /// the instance's own bus/interface port; expand it into member pin NetPoints
    /// (symmetric with the sub-module Out bus port expansion). If the owner is not
    /// a known component or the port is not a bus port, fall back to a single
    /// point on the encoded path.
    fn decode_return_endpoint(&self, ep_path: &str) -> Result<Vec<NetPoint>, InstError> {
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
        Ok(vec![NetPoint::with_owner(ep_path, owner, IOType::None)])
    }

    /// Decode a `@@RETURN_NETS:{n1};{n2}` encoded return face: each substituted
    /// name resolves to its own net point (single-point N=1 / column vector N≥2 /
    /// group members are parallel lanes).
    fn decode_return_nets(&mut self, nets: &str) -> Result<Vec<NetPoint>, InstError> {
        let mut points = Vec::new();
        for name in nets.split(';').filter(|s| !s.is_empty()) {
            let bus = McBus::new(name);
            points.extend(self.expand_node_element(&bus));
        }
        Ok(points)
    }

    /// Decode a `@@ARRAY:{name1},{name2}` iterated / array-form caller: collect
    /// each instance's entry face (left) or exit face (right).
    fn decode_array_face(
        &mut self,
        list_str: &str,
        side: FaceSide,
    ) -> Result<Vec<NetPoint>, InstError> {
        let mut points = Vec::new();
        for n in list_str.split(',').filter(|s| !s.is_empty()) {
            if let Some(comp) = self.components.iter().find(|c| c.name == n) {
                if comp.is_multi_pin() && comp.has_io_annotations() {
                    if side.is_left() {
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
                    } else {
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
                }
                let pin = if side.is_left() {
                    comp.get_left_pin()
                } else {
                    comp.get_right_pin()
                };
                if let Some(pin) = pin {
                    points.push(pin);
                }
            } else if let Some(sub) = self.sub_modules.iter().find(|s| s.name == n) {
                let sub_name = sub.name.clone();
                for p in sub.ports.iter().filter(|p| {
                    if side.is_left() {
                        matches!(p.iotype, IOType::In)
                    } else {
                        matches!(p.iotype, IOType::Out)
                    }
                }) {
                    if !side.is_left() && p.is_bus_port() {
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
        Ok(points)
    }

    /// case ①: the instance's own default shape. Left = entry face (input pins,
    /// falling back to power[0] / pin1); right = exit face (output pins, falling
    /// back to power[1] / pin2). A 1×2 twopin keeps pin1 as the left entry and
    /// pin2 as the right exit (mcode.md default row shape).
    fn component_own_face(
        &self,
        comp: &McComponentInst,
        side: FaceSide,
    ) -> Result<Vec<NetPoint>, InstError> {
        if side.is_left() {
            if comp.is_multi_pin() && comp.has_io_annotations() {
                let input_pins = comp.get_input_pins();
                if !input_pins.is_empty() {
                    return Ok(input_pins);
                }
                let pwr = comp.get_power_pins();
                if !pwr.is_empty() {
                    return Ok(vec![pwr[0].clone()]);
                }
            }
            if let Some(pin) = comp.get_left_pin() {
                return Ok(vec![pin]);
            }
            Ok(Vec::new())
        } else {
            if comp.is_multi_pin() && comp.has_io_annotations() {
                let output_pins = comp.get_output_pins();
                if !output_pins.is_empty() {
                    return Ok(output_pins);
                }
                let pwr = comp.get_power_pins();
                if pwr.len() >= 2 {
                    return Ok(vec![pwr[1].clone()]);
                }
            }
            if let Some(pin) = comp.get_right_pin() {
                return Ok(vec![pin]);
            }
            Ok(Vec::new())
        }
    }

    /// case ①: sub-module's own face. Left = In ports; right = Out ports
    /// (N×1 bus ports expanded into lanes, symmetric with the old right resolver).
    fn submodule_own_face(
        &self,
        sub: &McModuleInst,
        side: FaceSide,
    ) -> Result<Vec<NetPoint>, InstError> {
        if side.is_left() {
            return Ok(sub
                .ports
                .iter()
                .filter(|p| matches!(p.iotype, IOType::In))
                .map(|p| {
                    NetPoint::with_owner(&format!("{}.{}", sub.name, p.name), &sub.name, IOType::In)
                })
                .collect());
        }
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
                points.push(NetPoint::with_owner(
                    &format!("{}.{}", sub_name, p.name),
                    &sub_name,
                    p.iotype.clone(),
                ));
            }
        }
        Ok(points)
    }

    /// Fallback when the FuncCall never hit auto_inst_map: resolve the phrase's
    /// own interface buses, replacing `<sub>.in`/`<sub>.out` placeholders with
    /// the sub-module's In/Out ports (right side expands N×1 bus ports into
    /// lanes) and filtering synthetic `.in`/`.out` class-name placeholders.
    fn resolve_face_from_buses(
        &mut self,
        buses: &[McBus],
        side: FaceSide,
    ) -> Result<Vec<NetPoint>, InstError> {
        let suffix = if side.is_left() { "in" } else { "out" };
        let mut points: Vec<NetPoint> = Vec::new();
        for e in buses {
            // `<sub>.in` / `<sub>.out` placeholder → replace with the sub-module's
            // In/Out ports (right expands N×1 bus ports into lanes).
            if let Some((inst_part, s)) = e.name.split_once('.') {
                if s == suffix {
                    if let Some(sub) = self.sub_modules.iter().find(|s| s.name == inst_part) {
                        let sub_name = sub.name.clone();
                        for p in sub.ports.iter().filter(|p| {
                            if side.is_left() {
                                matches!(p.iotype, IOType::In)
                            } else {
                                matches!(p.iotype, IOType::Out | IOType::InOut)
                            }
                        }) {
                            if !side.is_left() {
                                let members: Vec<String> = if p.is_bus_port() {
                                    p.bus_members.clone()
                                } else if let Some(brace_start) = p.name.find('{') {
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
                                let clean_name = if let Some(brace_pos) = p.name.find('{') {
                                    &p.name[..brace_pos]
                                } else {
                                    &p.name
                                };
                                if members.len() >= 2 {
                                    for m in &members {
                                        points.push(NetPoint::with_owner(
                                            &format!("{sub_name}.{clean_name}.{m}"),
                                            &sub_name,
                                            p.iotype.clone(),
                                        ));
                                    }
                                } else {
                                    points.push(NetPoint::with_owner(
                                        &format!("{sub_name}.{clean_name}"),
                                        &sub_name,
                                        p.iotype.clone(),
                                    ));
                                }
                            } else {
                                // left: In ports stay single points
                                points.push(NetPoint::with_owner(
                                    &format!("{}.{}", sub_name, p.name),
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
            if let Some((inst_part, s)) = e.name.split_once('.') {
                if (s == "in" || s == "out")
                    && self
                        .find_component(inst_part)
                        .is_some_and(|c| c.get_pin(s).is_none())
                {
                    continue;
                }
            }
            // ── P0-4.B: filter class-name placeholder leak ─────────────────────
            if let Some((inst_part, s)) = e.name.rsplit_once('.') {
                if (s == "in" || s == "out")
                    && !inst_part.is_empty()
                    && Self::is_registered_class_name(inst_part)
                    && self.find_component(inst_part).is_none()
                    && self.find_submodule(inst_part).is_none()
                    && !self.is_port(inst_part)
                    && !self.is_bus(inst_part)
                {
                    continue;
                }
            }
            points.extend(self.expand_node_element(e));
        }
        Ok(points)
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
