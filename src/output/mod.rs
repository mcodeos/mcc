// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Output formatting layer.
//!
//! ## PR-2 changes
//!
//! Before: there was only one generic [`emit`] function, writing any `Serialize + Display` to
//! stdout or a file. Each command constructed its own report type, not reused.
//!
//! Now: added the [`emit_envelope`] path. When the user passes `--json` (or explicitly `--format json`),
//! the command should take the envelope path; otherwise continue with the [`emit`] text path (backward compatible).

pub mod builder;
pub mod compact;
pub mod diagnostic;
pub mod envelope;
pub mod renderer;

use anyhow::Result;
use mcc::cli::OutputFormat;
use serde::Serialize;
use std::fmt::Display;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

// ============================================================================
// Old API: emit (backward compatible, untouched)
// ============================================================================

/// Output any `Serialize + Display` to stdout or a file.
///
/// After PR-2, new code is recommended to use [`emit_envelope`]; [`emit`] is only for simple
/// KPI output like `--summary`, or commands not yet refactored to use envelope.
pub fn emit<T>(value: &T, format: OutputFormat, target: Option<&Path>) -> Result<()>
where
    T: Serialize + Display,
{
    let buf = render(value, format)?;
    write_out(&buf, target)
}

fn render<T>(value: &T, format: OutputFormat) -> Result<String>
where
    T: Serialize + Display,
{
    Ok(match format {
        OutputFormat::Text => format!("{}", value),
        OutputFormat::Json => serde_json::to_string(value)?,
        OutputFormat::JsonPretty => serde_json::to_string_pretty(value)?,
        OutputFormat::Yaml => serde_yaml::to_string(value)?,
        // CSV is rendered by callers (extract/export), not by emit().
        OutputFormat::Csv => format!("{}", value),
    })
}

// ============================================================================
// New API: emit_envelope - main entry of PR-2
// ============================================================================

/// Output [`Envelope`] to stdout or a file.
///
/// The caller invokes this once after assembling the result. Behavior:
///
/// - `format = Json | JsonPretty | Yaml`: serialize the entire envelope
/// - `format = Text`: go through [`render_envelope_text`] for the detailed
///   human-readable report
///
/// **Important convention**: in command implementations, JSON mode should be **completely silent**
/// (no eprintln decoration). This function does not handle stderr itself, but your command dispatch
/// should check [`is_structured`](OutputFormat).
pub fn emit_envelope(
    env: &envelope::Envelope,
    format: OutputFormat,
    target: Option<&Path>,
    skip_diagnostics: bool,
) -> Result<()> {
    let buf = match format {
        OutputFormat::Json => serde_json::to_string(env)?,
        OutputFormat::JsonPretty => serde_json::to_string_pretty(env)?,
        OutputFormat::Yaml => serde_yaml::to_string(env)?,
        OutputFormat::Text => render_envelope_text(env, skip_diagnostics),
        // CSV is structured data emitted by individual commands (extract,
        // export), not by emit_envelope. Fall through to text here so the
        // match is exhaustive; commands that support CSV render the artifact
        // directly to stdout/file.
        OutputFormat::Csv => render_envelope_text(env, skip_diagnostics),
    };
    write_out(&buf, target)
}

/// Emit the envelope as the **brief** trailing report (text) or full
/// serialization (json/yaml). [`crate::cmds::parse`] prints its detailed tables
/// live through the renderer layer, so its envelope tail stays compact here to
/// avoid double-rendering; every other command goes through [`emit_envelope`].
pub fn emit_envelope_brief(
    env: &envelope::Envelope,
    format: OutputFormat,
    target: Option<&Path>,
    skip_diagnostics: bool,
) -> Result<()> {
    let buf = match format {
        OutputFormat::Json => serde_json::to_string(env)?,
        OutputFormat::JsonPretty => serde_json::to_string_pretty(env)?,
        OutputFormat::Yaml => serde_yaml::to_string(env)?,
        OutputFormat::Text | OutputFormat::Csv => render_envelope_brief(env, skip_diagnostics),
    };
    write_out(&buf, target)
}

// ============================================================================
// Text rendering — render envelope as a human-readable report
// ============================================================================

/// Render the envelope as the **brief** trailing report — command / workspace /
/// per-pass counts / diagnostics / summary line. `mcc parse` prints its detailed
/// tables live through the renderer layer, so its envelope tail uses this
/// compact form (via [`emit_envelope_brief`]); the full detailed report for
/// every other command is [`render_envelope_text`].
pub fn render_envelope_brief(env: &envelope::Envelope, skip_diagnostics: bool) -> String {
    let mut out = String::new();

    if let Some(err) = &env.error {
        out.push_str(&format!("✗ Error [{}]: {}\n", err.code, err.message));
        if let Some(d) = &err.data {
            out.push_str(&format!("  data: {}\n", d));
        }
        return out;
    }

    let Some(r) = &env.result else {
        out.push_str("(empty result)\n");
        return out;
    };

    out.push_str(&format!(
        "● {} [{}: {}]\n",
        r.command,
        format!("{:?}", r.workspace.kind).to_lowercase(),
        r.workspace.name
    ));

    if !skip_diagnostics {
        if let Some(p) = &r.pass0 {
            out.push_str(&format!(
                "  pass0: {} files, {} diagnostics\n",
                p.loaded_files.len(),
                p.diagnostics.len()
            ));
            for d in &p.diagnostics {
                out.push_str(&format!("    {}\n", format_diagnostic(d)));
            }
        }

        if let Some(p) = &r.pass1 {
            out.push_str(&format!(
                "  pass1: {} files, {} modules, {} components, {} interfaces, {} diagnostics\n",
                p.loaded_files.len(),
                p.definitions.modules.len(),
                p.definitions.components.len(),
                p.definitions.interfaces.len(),
                p.diagnostics.len()
            ));
            for d in &p.diagnostics {
                out.push_str(&format!("    {}\n", format_diagnostic(d)));
            }
        }

        if let Some(p) = &r.pass2 {
            out.push_str(&format!(
                "  pass2: top={}, {} nets, {} connections, {} diagnostics\n",
                p.top,
                p.nets.len(),
                p.connections.len(),
                p.diagnostics.len()
            ));
            for d in &p.diagnostics {
                out.push_str(&format!("    {}\n", format_diagnostic(d)));
            }
        }
    }

    if let Some(e) = &r.extract {
        out.push_str(&format!("  extract: target={}\n", e.target));
    }

    if let Some(view) = &r.view {
        // tree/ast output: serialize data as formatted JSON for printing
        let tree_str = serde_json::to_string_pretty(&view.data).unwrap_or_default();
        out.push_str(&format!(
            "  {}: ({} nodes)\n",
            view.target,
            view.data.as_array().map(|a| a.len()).unwrap_or(0)
        ));
        for line in tree_str.lines() {
            out.push_str(&format!("    {}\n", line));
        }
    }

    if let Some(v) = &r.viz {
        out.push_str(&format!(
            "  viz: format={}, {} bytes{}\n",
            v.format,
            v.bytes,
            v.written_to
                .as_deref()
                .map(|p| format!(", written to {}", p))
                .unwrap_or_default()
        ));
    }

    if let Some(s) = &r.search {
        out.push_str(&format!(
            "  search: pattern={:?}, kind={}, regex={}, fuzzy={}, count={}\n",
            s.pattern,
            s.kind.as_deref().unwrap_or("*"),
            s.regex,
            s.fuzzy,
            s.count
        ));
    }

    if let Some(q) = &r.query {
        out.push_str(&format!("  query: expr={:?}, count={}\n", q.expr, q.count));
    }

    if let Some(e) = &r.export {
        out.push_str(&format!(
            "  export: kind={}, format={}, count={}\n",
            e.kind, e.format, e.count
        ));
    }

    let s = &r.summary;
    if skip_diagnostics {
        out.push_str("\n═══════════════════════════════════════════════════════════════\n");
        out.push_str(" Summary\n");
        out.push_str("═══════════════════════════════════════════════════════════════\n");
    }
    out.push_str(&format!(
        "  summary: errors={}, warnings={}, elapsed={}ms\n",
        s.errors, s.warnings, s.elapsed_ms
    ));

    out
}

/// Render the envelope's result as the detailed human-readable report — the
/// `-f text` output.
///
/// Shows the Pass-0 load diagnostics, then the Pass-1 definitions, then the
/// Pass-2 instance tree / connections / nets / net summary, then the KPI block.
/// `skip_diagnostics` only gates the per-pass diagnostic lines.
///
/// **Data-driven**: reads only [`envelope::CommandResult`] fields, so it
/// renders identically for the local and RPC paths (both deserialize the
/// result into `CommandResult` before calling [`emit_envelope`]). Mirrors the
/// shape of the legacy live tables (`cmds::print::print_module_inst` /
/// `print_net_summary`) from the envelope's own tree.
pub fn render_envelope_text(env: &envelope::Envelope, skip_diagnostics: bool) -> String {
    let mut out = String::new();

    if let Some(err) = &env.error {
        out.push_str(&format!("✗ Error [{}]: {}\n", err.code, err.message));
        if let Some(d) = &err.data {
            out.push_str(&format!("  data: {}\n", d));
        }
        return out;
    }

    let Some(r) = &env.result else {
        out.push_str("(empty result)\n");
        return out;
    };

    out.push_str(&format!(
        "● {} [{}: {}]\n",
        r.command,
        format!("{:?}", r.workspace.kind).to_lowercase(),
        r.workspace.name
    ));

    // ── Pass 0: lib + project load (usually sparse; mostly diagnostics) ──
    if let Some(p) = &r.pass0 {
        let show_diags = !skip_diagnostics && !p.diagnostics.is_empty();
        if !p.loaded_files.is_empty() || show_diags {
            out.push_str("\n═══════════════════════════════════════════════════════════════\n");
            out.push_str(" Pass 0\n");
            out.push_str("═══════════════════════════════════════════════════════════════\n");
            if !p.loaded_files.is_empty() {
                out.push_str(&format!("loaded files ({}):\n", p.loaded_files.len()));
                for f in &p.loaded_files {
                    out.push_str(&format!(
                        "  {}{}\n",
                        f.uri,
                        if f.is_system { " (system)" } else { "" }
                    ));
                }
            }
            if show_diags {
                out.push_str("  diagnostics:\n");
                for dg in &p.diagnostics {
                    out.push_str(&format!("    {}\n", format_diagnostic(dg)));
                }
            }
        }
    }

    // Instance-tree tallies (used classes + instance breakdown), computed once.
    // Classify classes by space: a class name is "system" when its definition
    // lives under `/mcode/` (parse.rs::group_by_uri convention), "project"
    // otherwise. Same-named project definitions shadow system ones.
    let mut system_modules = std::collections::HashSet::new();
    let mut system_components = std::collections::HashSet::new();
    let mut project_modules = std::collections::HashSet::new();
    let mut project_components = std::collections::HashSet::new();
    if let Some(p) = &r.pass1 {
        for d in &p.definitions.modules {
            if is_system_uri(&d.uri) {
                system_modules.insert(d.name.clone());
            } else {
                project_modules.insert(d.name.clone());
            }
        }
        for d in &p.definitions.components {
            if is_system_uri(&d.uri) {
                system_components.insert(d.name.clone());
            } else {
                project_components.insert(d.name.clone());
            }
        }
    }
    let tallies = tally_tree(
        r.pass2.as_ref().and_then(|p| p.instances.as_ref()),
        &system_modules,
        &project_modules,
        &system_components,
        &project_components,
    );

    // ── Pass 1: loaded files + definitions ──
    if let Some(p) = &r.pass1 {
        out.push_str("\n═══════════════════════════════════════════════════════════════\n");
        out.push_str(" Pass 1\n");
        out.push_str("═══════════════════════════════════════════════════════════════\n");
        if !p.loaded_files.is_empty() {
            out.push_str(&format!("loaded files ({}):\n", p.loaded_files.len()));
            for f in &p.loaded_files {
                out.push_str(&format!(
                    "  {}{}\n",
                    f.uri,
                    if f.is_system { " (system)" } else { "" }
                ));
            }
        }
        let d = &p.definitions;
        out.push_str(&format!(
            "definitions: {} modules, {} components, {} interfaces, {} enums\n",
            d.modules.len(),
            d.components.len(),
            d.interfaces.len(),
            d.enums.len()
        ));
        // Split each category into its definition space: the system library
        // (`/mcode/`) vs the project. Mirrors parse.rs::group_by_uri.
        let (sys_mods, proj_mods) = partition_defs(&d.modules);
        let (sys_comps, proj_comps) = partition_defs(&d.components);
        let (sys_ifaces, proj_ifaces) = partition_defs(&d.interfaces);
        let (sys_enums, proj_enums) = partition_defs(&d.enums);
        render_definition_space(
            &mut out,
            "system",
            &sys_mods,
            &sys_comps,
            &sys_ifaces,
            &sys_enums,
        );
        render_definition_space(
            &mut out,
            "project",
            &proj_mods,
            &proj_comps,
            &proj_ifaces,
            &proj_enums,
        );
        if !d.ports.is_empty() {
            out.push_str("  ports:\n");
            for pr in &d.ports {
                out.push_str(&format!(
                    "    {:<24} {:<10} {}\n",
                    pr.name, pr.iotype, pr.module
                ));
            }
        }
        if !skip_diagnostics && !p.diagnostics.is_empty() {
            out.push_str("  diagnostics:\n");
            for dg in &p.diagnostics {
                out.push_str(&format!("    {}\n", format_diagnostic(dg)));
            }
        }
    }

    // ── Pass 2: instance tree + connections + nets ──
    if let Some(p) = &r.pass2 {
        out.push_str("\n═══════════════════════════════════════════════════════════════\n");
        out.push_str(" Pass 2\n");
        out.push_str("═══════════════════════════════════════════════════════════════\n");
        out.push_str(&format!("top: {}\n", p.top));

        if let Some(root) = &p.instances {
            out.push_str("\n───────────────────────────────────────────────────────────────\n");
            out.push_str(" Instance Tree\n");
            out.push_str("───────────────────────────────────────────────────────────────\n");
            render_table_instance_node(&mut out, root, 0);
        }

        if !p.connections.is_empty() {
            out.push_str("\n───────────────────────────────────────────────────────────────\n");
            out.push_str(&format!(" Connections ({})\n", p.connections.len()));
            out.push_str("───────────────────────────────────────────────────────────────\n");
            // Group consecutive entries by module scope (tree-walk order keeps
            // each module's connections contiguous). Instance names and the
            // engine's per-module connection ids repeat across scopes, so the
            // module header is what disambiguates them.
            let mut counts: std::collections::HashMap<&str, usize> =
                std::collections::HashMap::new();
            for c in &p.connections {
                *counts.entry(c.module.as_str()).or_insert(0) += 1;
            }
            let mut cur_mod: Option<&str> = None;
            for c in &p.connections {
                if cur_mod != Some(c.module.as_str()) {
                    cur_mod = Some(c.module.as_str());
                    out.push_str(&format!(
                        "Module: {} ({} connections)\n",
                        c.module,
                        counts[c.module.as_str()]
                    ));
                }
                let net = c.net_name.as_deref().unwrap_or("-");
                out.push_str(&format!(
                    "  #{:<4} net={:<28} {}\n",
                    c.id,
                    net,
                    c.points.join(" , ")
                ));
            }
        }

        if !p.nets.is_empty() {
            out.push_str("\n───────────────────────────────────────────────────────────────\n");
            out.push_str(&format!(" Nets ({})\n", p.nets.len()));
            out.push_str("───────────────────────────────────────────────────────────────\n");
            let mut counts: std::collections::HashMap<&str, usize> =
                std::collections::HashMap::new();
            for n in &p.nets {
                *counts.entry(n.module.as_str()).or_insert(0) += 1;
            }
            let mut cur_mod: Option<&str> = None;
            for n in &p.nets {
                if cur_mod != Some(n.module.as_str()) {
                    cur_mod = Some(n.module.as_str());
                    out.push_str(&format!(
                        "Module: {} ({} nets)\n",
                        n.module,
                        counts[n.module.as_str()]
                    ));
                }
                out.push_str(&format!("  {:<32} {}\n", n.name, n.points.join(" , ")));
            }
        }

        // Whole-tree aggregate (the netlist statistics), mirroring
        // print_net_summary. "module scopes" = root + instantiated sub-modules,
        // distinct from the namespace module-class count below.
        out.push_str("\n───────────────────────────────────────────────────────────────\n");
        out.push_str(" Net Summary (whole tree)\n");
        out.push_str("───────────────────────────────────────────────────────────────\n");
        out.push_str(&format!(
            "  module scopes:         {}\n",
            tallies.module_insts
        ));
        out.push_str(&format!(
            "  connections (total):   {}\n",
            p.connections.len()
        ));
        out.push_str(&format!("  unique nets per scope: {}\n", p.nets.len()));

        if !skip_diagnostics && !p.diagnostics.is_empty() {
            out.push_str("  diagnostics:\n");
            for dg in &p.diagnostics {
                out.push_str(&format!("    {}\n", format_diagnostic(dg)));
            }
        }
    }

    // ── Aux result lines (mirror text mode) ──
    if let Some(e) = &r.extract {
        out.push_str(&format!("  extract: target={}\n", e.target));
    }
    if let Some(view) = &r.view {
        let tree_str = serde_json::to_string_pretty(&view.data).unwrap_or_default();
        out.push_str(&format!(
            "  {}: ({} nodes)\n",
            view.target,
            view.data.as_array().map(|a| a.len()).unwrap_or(0)
        ));
        for line in tree_str.lines() {
            out.push_str(&format!("    {}\n", line));
        }
    }
    if let Some(v) = &r.viz {
        out.push_str(&format!(
            "  viz: format={}, {} bytes{}\n",
            v.format,
            v.bytes,
            v.written_to
                .as_deref()
                .map(|p| format!(", written to {}", p))
                .unwrap_or_default()
        ));
    }
    if let Some(s) = &r.search {
        out.push_str(&format!(
            "  search: pattern={:?}, kind={}, regex={}, fuzzy={}, count={}\n",
            s.pattern,
            s.kind.as_deref().unwrap_or("*"),
            s.regex,
            s.fuzzy,
            s.count
        ));
    }
    if let Some(q) = &r.query {
        out.push_str(&format!("  query: expr={:?}, count={}\n", q.expr, q.count));
    }
    if let Some(e) = &r.export {
        out.push_str(&format!(
            "  export: kind={}, format={}, count={}\n",
            e.kind, e.format, e.count
        ));
    }

    // ── Summary: the netlist statistics live in the Net Summary section above;
    // this block separates the namespace classes (definitions) from the classes
    // actually used and from the instance count, and splits both class counts
    // into system space (`/mcode/` library) vs project space.
    let s = &r.summary;
    let ns_split = |defs: &[envelope::DefinitionRef]| -> (usize, usize) {
        let sys = defs.iter().filter(|d| is_system_uri(&d.uri)).count();
        (sys, defs.len() - sys)
    };
    let (ns_mod_sys, ns_mod_proj) = r
        .pass1
        .as_ref()
        .map(|p| ns_split(&p.definitions.modules))
        .unwrap_or((0, 0));
    let (ns_comp_sys, ns_comp_proj) = r
        .pass1
        .as_ref()
        .map(|p| ns_split(&p.definitions.components))
        .unwrap_or((0, 0));
    let (ns_iface_sys, ns_iface_proj) = r
        .pass1
        .as_ref()
        .map(|p| ns_split(&p.definitions.interfaces))
        .unwrap_or((0, 0));

    out.push_str("\n═══════════════════════════════════════════════════════════════\n");
    out.push_str(" Summary\n");
    out.push_str("═══════════════════════════════════════════════════════════════\n");
    out.push_str(&format!(
        "  namespace classes: modules={}, components={}, interfaces={}\n",
        s.module_count, s.component_count, s.interface_count
    ));
    out.push_str(&format!(
        "    system:  modules={}, components={}, interfaces={}\n",
        ns_mod_sys, ns_comp_sys, ns_iface_sys
    ));
    out.push_str(&format!(
        "    project: modules={}, components={}, interfaces={}\n",
        ns_mod_proj, ns_comp_proj, ns_iface_proj
    ));
    out.push_str(&format!(
        "  used classes:      modules={}, components={}\n",
        tallies.used_module_classes, tallies.used_component_classes
    ));
    out.push_str(&format!(
        "    system:  modules={}, components={}\n",
        tallies.used_module_classes_system, tallies.used_component_classes_system
    ));
    out.push_str(&format!(
        "    project: modules={}, components={}\n",
        tallies.used_module_classes - tallies.used_module_classes_system,
        tallies.used_component_classes - tallies.used_component_classes_system
    ));
    out.push_str(&format!(
        "  instances:         {} (modules={}, components={})\n",
        s.instance_count, tallies.module_insts, tallies.component_insts
    ));
    out.push_str(&format!(
        "  errors={}, warnings={}, elapsed={}ms\n",
        s.errors, s.warnings, s.elapsed_ms
    ));

    out
}

/// Partition a definition list into (system, project) by its definition space
/// (system = uri under `/mcode/`, mirroring parse.rs::group_by_uri).
fn partition_defs(
    defs: &[envelope::DefinitionRef],
) -> (Vec<&envelope::DefinitionRef>, Vec<&envelope::DefinitionRef>) {
    defs.iter().partition(|d| is_system_uri(d.uri.as_str()))
}

/// Render one definition-space section (system library vs project) of the
/// Pass-1 definitions: a summary header with per-category counts, then the
/// name/uri rows for each non-empty category.
fn render_definition_space(
    out: &mut String,
    title: &str,
    mods: &[&envelope::DefinitionRef],
    comps: &[&envelope::DefinitionRef],
    ifaces: &[&envelope::DefinitionRef],
    enums: &[&envelope::DefinitionRef],
) {
    if mods.is_empty() && comps.is_empty() && ifaces.is_empty() && enums.is_empty() {
        out.push_str(&format!("  {}: (none)\n", title));
        return;
    }
    out.push_str(&format!(
        "  {} ({} modules, {} components, {} interfaces, {} enums):\n",
        title,
        mods.len(),
        comps.len(),
        ifaces.len(),
        enums.len()
    ));
    for (label, items) in [
        ("modules", mods),
        ("components", comps),
        ("interfaces", ifaces),
        ("enums", enums),
    ] {
        if items.is_empty() {
            continue;
        }
        out.push_str(&format!("    {} ({}):\n", label, items.len()));
        for it in items {
            out.push_str(&format!("      {:<26} {}\n", it.name, it.uri));
        }
    }
}

/// Render one [`envelope::InstanceNode`] as an indented tree, mirroring
/// `cmds::print::print_module_inst` (ports bucketed by direction, components
/// with their pin list, then sub-modules recursively).
fn render_table_instance_node(out: &mut String, node: &envelope::InstanceNode, depth: usize) {
    let indent = "  ".repeat(depth);
    out.push_str(&format!(
        "{}>> Module: {} (type: {})\n",
        indent, node.name, node.class_name
    ));

    let mut inputs: Vec<&str> = Vec::new();
    let mut outputs: Vec<&str> = Vec::new();
    let mut bidirs: Vec<&str> = Vec::new();
    let mut powers: Vec<&str> = Vec::new();
    let mut analogs: Vec<&str> = Vec::new();
    for p in &node.ports {
        match p.iotype.as_str() {
            "in" => inputs.push(&p.name),
            "out" => outputs.push(&p.name),
            "inout" => bidirs.push(&p.name),
            "power" => powers.push(&p.name),
            _ => analogs.push(&p.name),
        }
    }
    let has_ports = !inputs.is_empty()
        || !outputs.is_empty()
        || !bidirs.is_empty()
        || !powers.is_empty()
        || !analogs.is_empty();
    if has_ports {
        out.push_str(&format!("{}   Ports:\n", indent));
        if !inputs.is_empty() {
            out.push_str(&format!("{}     -> in:    {}\n", indent, inputs.join(", ")));
        }
        if !outputs.is_empty() {
            out.push_str(&format!(
                "{}     <- out:   {}\n",
                indent,
                outputs.join(", ")
            ));
        }
        if !bidirs.is_empty() {
            out.push_str(&format!("{}     <> io:    {}\n", indent, bidirs.join(", ")));
        }
        if !powers.is_empty() {
            out.push_str(&format!("{}     ~~ power: {}\n", indent, powers.join(", ")));
        }
        if !analogs.is_empty() {
            out.push_str(&format!(
                "{}     -- anlg:  {}\n",
                indent,
                analogs.join(", ")
            ));
        }
    }

    if !node.components.is_empty() {
        out.push_str(&format!(
            "{}   Components ({}):\n",
            indent,
            node.components.len()
        ));
        for comp in &node.components {
            let pins: Vec<String> = comp
                .pins
                .iter()
                .map(|p| {
                    if p.name != p.id {
                        format!("{}({})", p.id, p.name)
                    } else {
                        p.id.clone()
                    }
                })
                .collect();
            let nc = if comp.nc { " (NC)" } else { "" };
            out.push_str(&format!(
                "{}     [C] {}: {}{} [pins: {}]\n",
                indent,
                comp.name,
                comp.class_name,
                nc,
                pins.join(", ")
            ));
        }
    }

    if !node.sub_modules.is_empty() {
        out.push_str(&format!(
            "{}   Sub-modules ({}):\n",
            indent,
            node.sub_modules.len()
        ));
        for sub in &node.sub_modules {
            render_table_instance_node(out, sub, depth + 2);
        }
    }
}

/// Instance-tree tallies: how many instances and how many distinct classes are
/// actually used in the design (from the Pass-2 tree), split by the space the
/// class is defined in (system `/mcode/` library vs project files).
#[derive(Default)]
struct TreeTallies {
    /// Module instances (root + sub-modules).
    module_insts: usize,
    /// Component instances.
    component_insts: usize,
    /// Distinct module classes actually instantiated.
    used_module_classes: usize,
    /// Of those, the ones defined only in the system space.
    used_module_classes_system: usize,
    /// Distinct component classes actually instantiated.
    used_component_classes: usize,
    /// Of those, the ones defined only in the system space.
    used_component_classes_system: usize,
}

/// `mcc` convention (mirrors `parse.rs::group_by_uri`): a definition lives in
/// the system space when its file path contains `/mcode/`.
fn is_system_uri(uri: &str) -> bool {
    uri.contains("/mcode/")
}

/// Walk the instance tree, collecting per-kind instance counts and the distinct
/// set of classes actually instantiated. A used class counts as *system* when
/// it is defined only in the system space — a same-named project definition
/// shadows it.
fn tally_tree(
    node: Option<&envelope::InstanceNode>,
    system_modules: &std::collections::HashSet<String>,
    project_modules: &std::collections::HashSet<String>,
    system_components: &std::collections::HashSet<String>,
    project_components: &std::collections::HashSet<String>,
) -> TreeTallies {
    fn walk(
        n: &envelope::InstanceNode,
        used_modules: &mut std::collections::BTreeSet<String>,
        used_components: &mut std::collections::BTreeSet<String>,
        out: &mut TreeTallies,
    ) {
        out.module_insts += 1;
        out.component_insts += n.components.len();
        used_modules.insert(n.class_name.clone());
        for c in &n.components {
            used_components.insert(c.class_name.clone());
        }
        for sub in &n.sub_modules {
            walk(sub, used_modules, used_components, out);
        }
    }
    let mut out = TreeTallies::default();
    let mut used_modules = std::collections::BTreeSet::new();
    let mut used_components = std::collections::BTreeSet::new();
    if let Some(root) = node {
        walk(root, &mut used_modules, &mut used_components, &mut out);
    }
    let is_system_class = |name: &str,
                           system: &std::collections::HashSet<String>,
                           project: &std::collections::HashSet<String>| {
        system.contains(name) && !project.contains(name)
    };
    out.used_module_classes = used_modules.len();
    out.used_module_classes_system = used_modules
        .iter()
        .filter(|n| is_system_class(n, system_modules, project_modules))
        .count();
    out.used_component_classes = used_components.len();
    out.used_component_classes_system = used_components
        .iter()
        .filter(|n| is_system_class(n, system_components, project_components))
        .count();
    out
}

/// Format a single [`envelope::Diagnostic`] into a rustc-style single-line text.
///
/// Looks like `error[E1309] foo.mc:10:5: message` — `2>&1 | grep E1309` can catch it directly.
///
/// When `location` is missing (rare, mostly INFO/HINT global diagnostics), degrades to
/// `error[E1309]: message`, without the file:line:col prefix.
pub fn format_diagnostic(d: &envelope::Diagnostic) -> String {
    let level = match d.severity {
        envelope::Severity::Error => "error",
        envelope::Severity::Warning => "warning",
        envelope::Severity::Info => "info",
        envelope::Severity::Hint => "hint",
    };
    let code = format!("E{:04}", d.code);
    match &d.location {
        Some(loc) => format!(
            "{}[{}] {}:{}:{}: {}",
            level, code, loc.file, loc.line, loc.column, d.message
        ),
        None => format!("{}[{}]: {}", level, code, d.message),
    }
}

// ============================================================================
// Write helper
// ============================================================================

fn write_out(buf: &str, target: Option<&Path>) -> Result<()> {
    match target {
        Some(p) => {
            let f = File::create(p)?;
            let mut w = BufWriter::new(f);
            w.write_all(buf.as_bytes())?;
            if !buf.ends_with('\n') {
                w.write_all(b"\n")?;
            }
            Ok(())
        }
        None => {
            print!("{}", buf);
            if !buf.ends_with('\n') {
                println!();
            }
            Ok(())
        }
    }
}

// ============================================================================
// OutputFormat extension
// ============================================================================

pub trait OutputFormatExt {
    /// JSON / JsonPretty / Yaml count as structured (go through envelope), Text and Csv do not.
    fn is_structured(&self) -> bool;
}

impl OutputFormatExt for OutputFormat {
    fn is_structured(&self) -> bool {
        matches!(
            self,
            OutputFormat::Json | OutputFormat::JsonPretty | OutputFormat::Yaml
        )
    }
}

#[cfg(test)]
mod tests {
    use super::envelope::*;
    use super::*;

    #[test]
    fn structured_check() {
        assert!(OutputFormat::Json.is_structured());
        assert!(OutputFormat::JsonPretty.is_structured());
        assert!(OutputFormat::Yaml.is_structured());
        assert!(!OutputFormat::Text.is_structured());
    }

    #[test]
    fn text_renders_minimal_envelope() {
        let e = Envelope::ok(CommandResult {
            command: "mcc load".into(),
            workspace: WorkspaceRef::project("test"),
            ..Default::default()
        });
        let s = render_envelope_text(&e, false);
        assert!(s.contains("mcc load"));
        assert!(s.contains("[project: test]"));
        // The detailed text report ends with the KPI Summary block.
        assert!(s.contains("Summary"));
        assert!(s.contains("errors=0"));
    }

    #[test]
    fn text_renders_error() {
        let e = Envelope::err(RpcError::parse_error("bad"));
        let s = render_envelope_text(&e, false);
        assert!(s.contains("✗ Error"));
        assert!(s.contains("32110"));
    }
}
