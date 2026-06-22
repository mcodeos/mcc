// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Pass1 → Pass2 info completeness debug output (★ NEW)
//!
//! ## Enabling
//! Set env var `MC_INST_DUMP=1` (or any non-empty non-`0`/`false` value) to enable.
//! Can also call `dump_pass_summary()` in code as needed.
//!
//! ## Three output sections
//! 1. `[P1-IN ]` —— Pass1 input snapshot when entering `instantiate()`
//!    - Module name / def_uri / parameter declarations (with bound values)
//!    - `def.insts`: all declared instances (Component / Module / Bus / ports...)
//!    - `def.lines.len()` + each line's Debug form
//!    - `def.funcs`: all user functions (name + parameter signature + function body line count)
//! 2. `[P2-OUT]` —— Pass2 output at end of instantiation
//!    - ports / components / sub_modules
//!    - buses / labels / auto_inst_map (FuncCall→instance association)
//!    - connections (with conn_id) / nets (net_name → endpoints)
//!    - diagnostics (count by level)
//! 3. `[P1→P2]` —— Pass1↔Pass2 reconciliation
//!    - whether each Component/Module in pass1 has an instance in pass2
//!    - whether each pass1 line produced at least one connection / sub_module
//!    - whether pass1 funcs can be found expanded in auto_inst_map
//!    - any missing → `[P2-MISSING]` line, grep to locate
//!
//! ## Relationship with existing diagnostics
//! This module **read-only**, does not write to `self.diagnostics`, does not affect `has_errors()`.
//! This ensures enabling dump doesn't pollute the normal diagnostics flow.

use super::super::mc_net::{canonicalize_path, NetPoint};
use super::McModuleInst;
use crate::core::common::IOType;
use crate::core::mc_inst::McInstance;
use std::sync::OnceLock;

// ============================================================================
// Enable check
// ============================================================================

/// Parsed result of `MC_INST_DUMP` env var, cached once per process
static DUMP_ENABLED: OnceLock<bool> = OnceLock::new();

/// Check whether dump is enabled
///
/// Enable condition: `MC_INST_DUMP` env var exists and value is not `""`/`0`/`false`/`False`/`FALSE`
pub(super) fn dump_enabled() -> bool {
    *DUMP_ENABLED.get_or_init(|| match std::env::var("MC_INST_DUMP") {
        Ok(v) => {
            let t = v.trim();
            !(t.is_empty() || t == "0" || t == "false" || t == "False" || t == "FALSE")
        }
        Err(_) => false,
    })
}

// ============================================================================
// Internal utilities
// ============================================================================

#[inline]
fn p1_prefix(name: &str) -> String {
    format!("[P1-IN ][{name}]")
}
#[inline]
fn p2_prefix(name: &str) -> String {
    format!("[P2-OUT][{name}]")
}
#[inline]
fn diff_prefix(name: &str) -> String {
    format!("[P1→P2 ][{name}]")
}
#[inline]
fn missing_prefix(name: &str) -> String {
    format!("[P2-MISSING][{name}]")
}

// ============================================================================
// Public API (impl McModuleInst)
// ============================================================================

impl McModuleInst {
    // ------------------------------------------------------------------------
    // 1. Pass1 input snapshot
    // ------------------------------------------------------------------------

    /// Print pass1 input snapshot (called at start of `instantiate()`)
    pub(super) fn dump_pass1_input(&self) {
        let p = p1_prefix(&self.name);
        eprintln!("{p} ── BEGIN ────────────────────────────────");
        eprintln!("{} module    = {}", p, self.def.name);
        eprintln!("{} def_uri   = {}", p, self.def_uri);

        // ---- Parameter declarations ----
        let mut param_count = 0usize;
        for decl in self.def.params.iter() {
            let pname = decl.get_primary_name().unwrap_or_else(|| "<anon>".into());
            let bound_value = self
                .params
                .find(&pname)
                .and_then(|b| b.get_value())
                .map(|v| format!("{v}"))
                .unwrap_or_else(|| "<unbound>".to_string());
            eprintln!("{p}   param   {pname} = {bound_value}");
            param_count += 1;
        }
        eprintln!("{p} params    : {param_count} declared");

        // ---- Declared instances ----
        let mut comp_count = 0usize;
        let mut module_count = 0usize;
        let mut bus_count = 0usize;
        let mut other_count = 0usize;
        for (key, inst) in self.def.insts.iter() {
            let (kind, type_or_name) = match inst {
                McInstance::Component(c) => {
                    comp_count += 1;
                    ("Component", c.base.name.to_string())
                }
                McInstance::Module(m) => {
                    module_count += 1;
                    ("Module", m.base.name.to_string())
                }
                McInstance::Bus(b) => {
                    bus_count += 1;
                    ("Bus(label)", b.name.clone())
                }
                _ => {
                    other_count += 1;
                    ("Other", String::new())
                }
            };
            let iotype = self
                .def
                .insts
                .iter_with_iotype()
                .find(|(k, _)| *k == key)
                .map(|(_, (io, _))| iotype_str(io))
                .unwrap_or("");
            if type_or_name.is_empty() {
                eprintln!("{p}   inst    {kind:<10} {iotype:>4}  {key}");
            } else {
                eprintln!("{p}   inst    {kind:<10} {iotype:>4}  {key} : {type_or_name}");
            }
        }
        eprintln!(
            "{p} insts     : {comp_count} component(s), {module_count} module(s), {bus_count} bus/label(s), {other_count} other"
        );

        // ---- Connection lines ----
        eprintln!("{} lines     : {} total", p, self.def.lines.len());
        for (i, line) in self.def.lines.iter().enumerate() {
            // Output in Debug form — truncated to a reasonable length to avoid flooding
            let dbg = format!("{line:?}");
            let truncated = if dbg.len() > 200 {
                format!("{}…(+{}b)", &dbg[..200], dbg.len() - 200)
            } else {
                dbg
            };
            eprintln!("{p}   line[{i:>3}] {truncated}");
        }

        // ---- User functions ----
        let mut func_count = 0usize;
        for func in self.def.funcs.iter() {
            let nparams = func.params.iter().count();
            let nlines = func.lines.len();
            eprintln!(
                "{}   func    {} ({} params, {} body lines)",
                p, func.name, nparams, nlines
            );
            func_count += 1;
        }
        eprintln!("{p} funcs     : {func_count} declared");

        eprintln!("{p} ── END ──────────────────────────────────");
    }

    // ------------------------------------------------------------------------
    // 2. Pass2 output snapshot
    // ------------------------------------------------------------------------
    /// Print pass2 output snapshot (called at end of `instantiate()`, after net table construction)
    pub(super) fn dump_pass2_output(&self) {
        let p = p2_prefix(&self.name);
        eprintln!("{p} ── BEGIN ────────────────────────────────");

        // ---- ports ----
        let n_in = self
            .ports
            .iter()
            .filter(|p| matches!(p.iotype, IOType::In))
            .count();
        let n_out = self
            .ports
            .iter()
            .filter(|p| matches!(p.iotype, IOType::Out))
            .count();
        let n_io = self
            .ports
            .iter()
            .filter(|p| matches!(p.iotype, IOType::InOut))
            .count();
        eprintln!(
            "{} ports     : total={}  In={}  Out={}  InOut={}",
            p,
            self.ports.len(),
            n_in,
            n_out,
            n_io
        );
        for port in &self.ports {
            // ── P0-1: Full print of port bus_members ──────────────────────
            let members_str = if port.bus_members.is_empty() {
                String::new()
            } else {
                format!("{{{}}}", port.bus_members.join(", "))
            };
            eprintln!(
                "{}   port    {:<5} {}{}",
                p,
                iotype_str(&port.iotype),
                port.name,
                members_str
            );
        }

        // ---- components ----
        eprintln!("{} components: {}", p, self.components.len());
        for comp in &self.components {
            eprintln!(
                "{}   comp    {} : {} ({} pin(s))",
                p,
                comp.name,
                comp.def.name,
                comp.pin_count()
            );
            // ── P0-1: Full component pins printing ──────────────────────────────
            let mut pin_list: Vec<(&String, &NetPoint)> = comp.pins.iter().collect();
            pin_list.sort_by(|a, b| {
                let na: i64 = a.0.parse().unwrap_or(i64::MAX);
                let nb: i64 = b.0.parse().unwrap_or(i64::MAX);
                na.cmp(&nb).then_with(|| a.0.cmp(b.0))
            });
            for (pid, pt) in &pin_list {
                eprintln!(
                    "{}     pin {:>4} → {} {}",
                    p,
                    pid,
                    pt.path,
                    iotype_str(&pt.iotype)
                );
            }
        }

        // ---- sub_modules ----
        eprintln!("{} sub_modules: {}", p, self.sub_modules.len());
        for sub in &self.sub_modules {
            eprintln!(
                "{}   submod  {} : {} ({} ports, {} comps, {} subs, {} conns, {} nets)",
                p,
                sub.name,
                sub.def.name,
                sub.ports.len(),
                sub.components.len(),
                sub.sub_modules.len(),
                sub.connections.len(),
                sub.nets.len(),
            );
        }

        // ---- buses ----
        eprintln!("{} buses     : {}", p, self.buses.len());
        for (name, bus) in &self.buses {
            eprintln!("{}   bus     {} {{{}}}", p, name, bus.members.join(", "));
        }

        // ---- labels ----
        eprintln!("{} labels    : {}", p, self.labels.len());
        for (name, point) in &self.labels {
            eprintln!("{p}   label   {name} → {point}");
        }

        // ---- auto_inst_map ----
        eprintln!(
            "{} auto_inst_map: {} entries (FuncCall key → instance name)",
            p,
            self.auto_inst_map.len()
        );
        // Only print instance name (key is address, no readability)
        let mut kinds: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for inst_name in self.auto_inst_map.values() {
            let kind = if inst_name.starts_with("@?") {
                "stub(P0-4)"
            } else if self.components.iter().any(|c| &c.name == inst_name) {
                "component"
            } else if self.sub_modules.iter().any(|s| &s.name == inst_name) {
                "sub_module"
            } else {
                "unknown"
            };
            *kinds.entry(kind).or_insert(0) += 1;
        }
        for (k, n) in &kinds {
            eprintln!("{p}   auto_map[{k}] = {n}");
        }

        // ---- connections ----
        // ── P0-1: Unified path resolver ──────────────────────────────────
        // Both Connections view and nets view go through canonicalize_path, ensuring both views
        // display consistent paths for the same physical node.
        eprintln!("{} connections: {}", p, self.connections.len());
        for conn in &self.connections {
            let canon_pts: Vec<String> = conn
                .points
                .iter()
                .map(|pt| {
                    let canon = canonicalize_path(&pt.path);
                    let io_tag = match pt.iotype {
                        IOType::In => "(in)",
                        IOType::Out => "(out)",
                        IOType::InOut => "(io)",
                        IOType::Power => "(pwr)",
                        IOType::Analog => "(anl)",
                        IOType::Return => "(return)",
                        IOType::NonCon => "(nc)",
                        IOType::None => "",
                    };
                    format!("{canon}{io_tag}")
                })
                .collect();
            let net_tag = conn
                .net_name
                .as_ref()
                .map(|n| format!("net({n})"))
                .unwrap_or_else(|| "net".to_string());
            eprintln!("{}   conn    {}: {}", p, net_tag, canon_pts.join(" ~ "));
        }

        // ---- nets ----
        eprintln!("{} nets      : {}", p, self.nets.len());
        for (name, points) in &self.nets {
            let pts: Vec<String> = points.iter().map(|x| x.to_string()).collect();
            eprintln!("{}   net     {} : [{}]", p, name, pts.join(", "));
        }

        // ---- diagnostics ----
        let n_total = self.diagnostics.len();
        let n_err = self
            .diagnostics
            .iter()
            .filter(|d| matches!(d.level, crate::instant::mc_net::InstDiagLevel::Error))
            .count();
        let n_warn = n_total - n_err;
        eprintln!("{p} diagnostics: total={n_total}  errors={n_err}  warnings={n_warn}");

        eprintln!("{p} ── END ──────────────────────────────────");
    }

    // ------------------------------------------------------------------------
    // 3. Pass1 ↔ Pass2 reconciliation
    // ------------------------------------------------------------------------

    /// Verify pass1 → pass2 info completeness
    pub(super) fn dump_pass_diff(&self) {
        let p = diff_prefix(&self.name);
        let m = missing_prefix(&self.name);
        eprintln!("{p} ── BEGIN ────────────────────────────────");

        // 3.1 Whether declared Component / Module all entered pass2
        let mut declared_comps: usize = 0;
        let mut declared_mods: usize = 0;
        let mut missing_comps: Vec<String> = Vec::new();
        let mut missing_mods: Vec<String> = Vec::new();

        for (_key, inst) in self.def.insts.iter() {
            match inst {
                McInstance::Component(c) => {
                    declared_comps += 1;
                    let want = c.name.to_string();
                    if !self.components.iter().any(|x| x.name == want) {
                        missing_comps.push(want);
                    }
                }
                McInstance::Module(m_) => {
                    declared_mods += 1;
                    let want = m_.name.to_string();
                    if !self.sub_modules.iter().any(|x| x.name == want) {
                        missing_mods.push(want);
                    }
                }
                _ => {}
            }
        }
        eprintln!(
            "{} declared component {} → pass2 found {} ({} missing)",
            p,
            declared_comps,
            declared_comps - missing_comps.len(),
            missing_comps.len()
        );
        for n in &missing_comps {
            eprintln!("{m} declared component '{n}' has no pass2 instance");
        }
        eprintln!(
            "{} declared sub_module {} → pass2 found {} ({} missing)",
            p,
            declared_mods,
            declared_mods - missing_mods.len(),
            missing_mods.len()
        );
        for n in &missing_mods {
            eprintln!("{m} declared sub_module '{n}' has no pass2 instance");
        }

        // 3.2 Whether each line produced connection / sub_module / component
        //     Rough check only: if pass2 simultaneously has connections=0, auto_inst_map=0, sub_modules
        //     delta also 0, then lines>0 but no products — lines were likely silently swallowed.
        let lines_count = self.def.lines.len();
        let conn_count = self.connections.len();
        let auto_map_count = self.auto_inst_map.len();
        let inline_subs = self
            .sub_modules
            .iter()
            .filter(|s| {
                // Names with underscore followed by digit (auto_name generated) are inline module construction products
                let n = &s.name;
                n.rsplit_once('_')
                    .and_then(|(_, suf)| suf.parse::<u32>().ok())
                    .is_some()
            })
            .count();
        eprintln!(
            "{p} lines      : pass1={lines_count}  →  pass2: connections={conn_count}, auto_inst={auto_map_count}, inline_subs={inline_subs}"
        );
        if lines_count > 0 && conn_count == 0 && auto_map_count == 0 && inline_subs == 0 {
            eprintln!(
                "{m} {lines_count} line(s) declared but pass2 produced no connections / inst-map / inline subs"
            );
        }

        // 3.3 Whether user functions were called / expanded
        //     Cannot precisely trace back, but can show "declared N user funcs, X entries in auto_inst_map".
        //     If auto_inst_map empty but funcs non-empty, high probability all user functions were not called.
        let func_count = self.def.funcs.iter().count();
        if func_count > 0 {
            eprintln!(
                "{p} user funcs : {func_count} declared (cannot precisely track expansion - check connections)"
            );
        }

        // 3.4 Port completeness: pass1 IO ports vs pass2 ports
        let pass1_io_ports = self
            .def
            .insts
            .iter_with_iotype()
            .filter(|(_, (io, _))| !matches!(io, IOType::None))
            .count();
        let pass2_ports = self.ports.len();
        eprintln!("{p} ports      : pass1 IO insts={pass1_io_ports} → pass2 ports={pass2_ports}");
        if pass1_io_ports != pass2_ports {
            eprintln!(
                "{m} port count mismatch: pass1 IO insts {pass1_io_ports} vs pass2 ports {pass2_ports}"
            );
        }

        eprintln!("{p} ── END ──────────────────────────────────");
    }

    // ------------------------------------------------------------------------
    // 4. Public API: one-click print three sections (for tests / external code as needed)
    // ------------------------------------------------------------------------

    /// Force print entry not dependent on env var (for unit tests / IDE / debugger use).
    ///
    /// Equivalent to calling in order: `dump_pass1_input()` + `dump_pass2_output()` + `dump_pass_diff()`.
    /// Note: Only has complete pass2 view **after instantiation has completed**.
    pub fn dump_pass_summary(&self) {
        self.dump_pass1_input();
        self.dump_pass2_output();
        self.dump_pass_diff();
    }
}

// ============================================================================
// Helper functions
// ============================================================================

fn iotype_str(io: &IOType) -> &'static str {
    match io {
        IOType::In => "In",
        IOType::Out => "Out",
        IOType::InOut => "IO",
        IOType::None => "-",
        IOType::Power => "Pwr",
        IOType::Analog => "Anl",
        IOType::Return => "Ret",
        IOType::NonCon => "NC",
    }
}
