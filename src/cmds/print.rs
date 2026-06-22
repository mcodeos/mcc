// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Pass2 output print functions (revised version)
//!
//! ## This fix
//!
//! ### 1) print_module_inst — No longer prints "??:" fake ports
//! Old version filtered all `inst.ports.iter()` by IOType, anything not In/Out/InOut
//! was displayed with "??:" fallback. But `inst.ports` also contains:
//!   - Real ports (In / Out / InOut / Power / Analog)
//!   - Top-level components / sub-modules (IOType::None)
//!   - Return marker (IOType::Return)
//!   - NC placeholder (IOType::NonCon)
//! The latter three are already displayed separately in `Components` / `Sub-modules` and other sections,
//! repeating them as "??:" in Ports section is redundant and misleading. New version filters them directly.
//!
//! ### 2) print_nets — Aggregate from connections, no longer depends on inst.nets
//! Pass2 currently doesn't populate `inst.nets`, causing old print_nets to always go to empty branch.
//! But `inst.connections` is complete and valid — the same `net_name` appears in multiple
//! connections, collecting all their points and deduplicating gives the actual
//! coverage of that net. New version calculates directly from connections, no need to wait for Pass2 to change implementation.

use crate::cli::PinSortMode;
use mcc::{IOType, McEndpoint, McInstance, McInstanceRef, McPhrase, MccProjectTree};
use std::collections::BTreeMap;

// ============================================================================
// Print Line members (McPhrase detailed structure) — same as old version
// ============================================================================

pub fn print_phrase_members(phrase: &McPhrase, prefix: &str) {
    match phrase {
        McPhrase::Series(phrases) => {
            for (i, p) in phrases.iter().enumerate() {
                if i > 0 {
                    println!("{}    |", prefix);
                    println!("{}    v", prefix);
                }
                print_phrase_members(p, prefix);
            }
        }
        McPhrase::Parallel(phrases) => {
            println!("{}(Parallel {})", prefix, phrases.len());
            for (i, p) in phrases.iter().enumerate() {
                print_phrase_members(p, &format!("{}  [{}]:", prefix, i));
            }
        }
        McPhrase::Closure(c) => {
            println!("{}(closure {} lines)", prefix, c.body.len());
            for (i, p) in c.body.iter().enumerate() {
                print_phrase_members(p, &format!("{}  body[{}]:", prefix, i));
            }
        }
        McPhrase::Group(g) => {
            println!("{}(group {} items)", prefix, g.opds.len());
            for (i, p) in g.opds.iter().enumerate() {
                print_phrase_members(p, &format!("{}  [{}]:", prefix, i));
            }
        }
        McPhrase::FuncCall(f) => {
            // pre-closure mode check
            let is_pre_closure = if let Some(c) = &f.caller {
                if let McPhrase::FuncCall(inner_fc) = c.as_ref() {
                    let func_name_str = inner_fc.func_name.to_string();
                    func_name_str
                        .chars()
                        .next()
                        .map_or(false, |c| c.is_uppercase())
                } else {
                    false
                }
            } else {
                false
            };

            print!("{}(funcall: ", prefix);
            if let Some(c) = &f.caller {
                if is_pre_closure {
                    if let McPhrase::FuncCall(inner_fc) = c.as_ref() {
                        print!("{}(", inner_fc.func_name);
                        let inner_params: Vec<String> =
                            inner_fc.params.iter().map(|p| format!("{}", p)).collect();
                        print!("{})", inner_params.join(", "));
                    }
                    print!(" -> ");
                } else {
                    print!("{}", c);
                    print!(".");
                }
            }
            print!("{}", f.func_name);
            let param_strs: Vec<String> = f.params.iter().map(|p| format!("{}", p)).collect();
            let display_params = if is_pre_closure && param_strs.first() == Some(&"_".to_string()) {
                &param_strs[1..]
            } else {
                &param_strs
            };
            print!("({})", display_params.join(", "));
            println!(")");
        }
        McPhrase::Member(inner, endpoint) => {
            print_phrase_members(inner, prefix);
            println!("{}    .{}", prefix, endpoint);
        }
        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
            base: McInstance::Component(c),
            ..
        })) => {
            println!("{}(component: {})", prefix, c.name);
        }
        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
            base: McInstance::Module(m),
            ..
        })) => {
            println!("{}(module: {})", prefix, m.name);
        }
        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
            base: McInstance::Label(label),
            ..
        })) => {
            println!("{}(label: {})", prefix, label);
        }
        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef { base: p, .. })) => {
            println!("{}(port: {})", prefix, p);
        }
        McPhrase::Endpoint(McEndpoint::Node { input, output }) => {
            let input_str: Vec<String> = input.iter().map(|e| format!("{}", e)).collect();
            let output_str: Vec<String> = output.iter().map(|e| format!("{}", e)).collect();
            println!(
                "{}(node: {{{} | {}}})",
                prefix,
                input_str.join(", "),
                output_str.join(", ")
            );
        }
        McPhrase::Endpoint(McEndpoint::List(nodes)) => {
            let items: Vec<String> = nodes.iter().map(|n| format!("{}", n)).collect();
            println!("{}(list: [{}])", prefix, items.join(", "));
        }
        McPhrase::Multiple(phrases) => {
            println!("{}(multiple {} items)", prefix, phrases.len());
            for (i, p) in phrases.iter().enumerate() {
                print_phrase_members(p, &format!("{}  [{}]:", prefix, i));
            }
        }
        McPhrase::Transposed(p) => {
            print!("{}(transposed: ", prefix);
            print_phrase_members(p, "");
            println!(")");
        }
        McPhrase::Lead => {
            println!("{}(lead)", prefix);
        }
    }
}

// ============================================================================
// Print Pass2 instantiation tree (FIXED)
// ============================================================================

pub fn print_module_inst(inst: &MccProjectTree, depth: usize, sort_mode: PinSortMode) {
    let indent = "  ".repeat(depth);
    println!(
        "{}>> Module: {} (type: {})",
        indent, inst.name, inst.def.name
    );

    // ── Ports: bucket by IOType, skip None / NonCon / Return ──
    // None     → placeholder for top-level components/sub-modules (displayed separately in Components / Sub-modules section below)
    // NonCon   → NC marker (internal detail)
    // Return   → function return marker (internal detail)
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    let mut bidirs = Vec::new();
    let mut powers = Vec::new();
    let mut analogs = Vec::new();

    for p in inst.ports.iter() {
        match p.iotype {
            IOType::In => inputs.push(p),
            IOType::Out => outputs.push(p),
            IOType::InOut => bidirs.push(p),
            IOType::Power => powers.push(p),
            IOType::Analog => analogs.push(p),
            IOType::None | IOType::NonCon | IOType::Return => { /* skip */ }
        }
    }

    let has_any = !inputs.is_empty()
        || !outputs.is_empty()
        || !bidirs.is_empty()
        || !powers.is_empty()
        || !analogs.is_empty();

    if has_any {
        println!("{}   Ports:", indent);
        for port in &inputs {
            println!("{}     -> in:    {}", indent, port.name);
        }
        for port in &outputs {
            println!("{}     <- out:   {}", indent, port.name);
        }
        for port in &bidirs {
            println!("{}     <> io:    {}", indent, port.name);
        }
        for port in &powers {
            println!("{}     ~~ power: {}", indent, port.name);
        }
        for port in &analogs {
            println!("{}     -- anlg:  {}", indent, port.name);
        }
    }

    // ── Params ──
    if !inst.params.is_empty() {
        println!("{}   Params:", indent);
        for binding in inst.params.iter() {
            let name = binding.declare.get_primary_name().unwrap_or_default();
            let value = binding
                .get_value()
                .map(|v| format!("{:?}", v))
                .unwrap_or_else(|| "(default)".to_string());
            println!("{}     {} = {}", indent, name, value);
        }
    }

    // ── Components ──
    if !inst.components.is_empty() {
        println!("{}   Components ({}):", indent, inst.components.len());
        for comp in &inst.components {
            // For each pin id, find the "longest" interface member name in pin_id_to_names (prefer dot-separated multi-level like I2C0.SCL)
            let mut pins: Vec<String> = comp
                .pins
                .keys()
                .map(|pid| {
                    let alias = comp.def.pins.pin_id_to_names.get(pid).and_then(|names| {
                        // Select longest (most informative) alias, None if not available
                        names.iter().max_by_key(|n| n.len()).cloned()
                    });
                    match alias {
                        Some(n) if n.as_str() != pid.as_str() => format!("{pid}({n})"),
                        _ => pid.clone(),
                    }
                })
                .collect();
            // Sort by user-specified mode
            match sort_mode {
                PinSortMode::PinId => {
                    // Sort by pinid number ascending (default). Try to parse as i64; if failed, put at the end.
                    pins.sort_by_key(|p| {
                        p.split('(')
                            .next()
                            .and_then(|s| s.parse::<i64>().ok())
                            .unwrap_or(i64::MAX)
                    });
                }
                PinSortMode::Interface => {
                    // Sort by interface name grouping. Each pin's (alias) part as group key.
                    // Within same interface, still sort by pinid ascending.
                    pins.sort_by(|a, b| {
                        let grp_a = a
                            .split_once('(')
                            .map(|(_, alias)| alias.trim_end_matches(')'))
                            .unwrap_or("");
                        let grp_b = b
                            .split_once('(')
                            .map(|(_, alias)| alias.trim_end_matches(')'))
                            .unwrap_or("");
                        let key_a = (
                            grp_a,
                            a.split('(')
                                .next()
                                .and_then(|s| s.parse::<i64>().ok())
                                .unwrap_or(i64::MAX),
                        );
                        let key_b = (
                            grp_b,
                            b.split('(')
                                .next()
                                .and_then(|s| s.parse::<i64>().ok())
                                .unwrap_or(i64::MAX),
                        );
                        key_a.cmp(&key_b)
                    });
                }
            }
            let nc_str = if comp.nc { "(NC)" } else { "" };
            println!(
                "{}     [C] {}: {}{} [pins: {}]",
                indent,
                comp.name,
                comp.def.name,
                nc_str,
                pins.join(", ")
            );
        }
    }

    // ── Sub-modules ──
    if !inst.sub_modules.is_empty() {
        println!("{}   Sub-modules ({}):", indent, inst.sub_modules.len());
        for sub in &inst.sub_modules {
            print_module_inst(sub, depth + 2, PinSortMode::PinId);
        }
    }
}

// ============================================================================
// Print Connections — same as old version (this function worked fine originally)
// ============================================================================

pub fn print_connections(inst: &MccProjectTree, depth: usize) {
    let indent = "  ".repeat(depth);
    if inst.connections.is_empty() {
        println!("{}Module: {} (no connections)", indent, inst.name);
    } else {
        println!(
            "{}Module: {} ({} connections)",
            indent,
            inst.name,
            inst.connections.len()
        );
        for conn in &inst.connections {
            let points: Vec<_> = conn.points.iter().map(|p| p.path.clone()).collect();
            let net_name = conn
                .net_name
                .clone()
                .unwrap_or_else(|| format!("__net_{}", conn.id));
            let conn_line = if points.len() >= 2 {
                format!("{} - {}", points[0], points[1])
            } else {
                points.join(" - ")
            };
            println!("{}  {} : {}", indent, net_name, conn_line);
        }
    }
    for sub in &inst.sub_modules {
        print_connections(sub, depth + 1);
    }
}

// ============================================================================
// Print Nets (REWRITTEN) — aggregate from connections
// ============================================================================
//
// Approach:
//   One "net" = union of *all points* from all connections with same net_name.
//   Iterate inst.connections once, collect with BTreeMap<net_name, Vec<point_label>>,
//   each point label internally formatted as "owner.last_segment" by (owner, path),
//   deduplicate, so whether Pass2 populated inst.nets or not, we get correct net view.

pub fn print_nets(inst: &MccProjectTree, depth: usize) {
    let indent = "  ".repeat(depth);

    // ── Aggregate ──
    let mut nets: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for conn in &inst.connections {
        let net_name = conn
            .net_name
            .clone()
            .unwrap_or_else(|| format!("__net_{}", conn.id));

        // ── Iter-9 (bugfix_report error 11) ────────────────────────────
        // Skip pure NC nets. NC means "not connected" (rules doc §11.4), shouldn't appear as electrical
        // net. Corresponding connections like `NC : CAP_3.2 - NC`, net
        // name is just "NC".
        if net_name == "NC" {
            continue;
        }

        let bucket = nets.entry(net_name).or_default();

        for p in &conn.points {
            // ── Iter-9: skip NC nodes (even if NC points mixed in non-NC-named nets) ──
            if p.path == "NC" {
                continue;
            }
            let label = if let Some(ref owner) = p.owner {
                let last = p.path.split('.').last().unwrap_or(&p.path);
                format!("{}.{}", owner, last)
            } else {
                p.path.clone()
            };
            // Deduplicate (same pin appearing in multiple connections only counts once)
            if !bucket.contains(&label) {
                bucket.push(label);
            }
        }
    }

    // ── Iter-9 (bugfix_report error 13): merge duplicate points ──
    //
    // Multiple connections have identical endpoints, but net_name each falls to `__net_{id}`
    // auto-numbering (different ids) and displayed as multiple independent nets. modldo module's
    // `vin -> ldo.VIN => CAP(...).Cap(_)` chaining + closure syntax causes same
    // connection to be instantiated twice (__net_1 and __net_3 have identical endpoints). Here at display layer
    // deduplicate by "sorted endpoint set signature", nets with identical endpoint sets only keep the first
    // (first by BTreeMap lexicographic order).
    {
        use std::collections::HashSet;
        let mut canonical: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut seen_signatures: HashSet<Vec<String>> = HashSet::new();
        for (name, points) in nets.into_iter() {
            // Skip empty nets or (after NC filtering) degenerate to single-point nets
            if points.len() < 2 {
                // Still preserve stub display semantics: skip 0 points, keep 1 point (will be printed as
                // (stub) marker below). But only keep if path hasn't appeared before.
                if points.is_empty() {
                    continue;
                }
            }
            let mut signature = points.clone();
            signature.sort();
            if seen_signatures.insert(signature) {
                canonical.insert(name, points);
            }
        }
        nets = canonical;
    }

    // ── Print ──
    if nets.is_empty() {
        println!("{}Module: {} (no nets)", indent, inst.name);
    } else {
        // Distinguish two net types: actual multi-terminal (>=2 points) and stub (1 point)
        let multi_count = nets.values().filter(|pts| pts.len() >= 2).count();
        let stub_count = nets.len() - multi_count;
        println!(
            "{}Module: {} ({} nets: {} connected, {} stub) [from {} connections]",
            indent,
            inst.name,
            nets.len(),
            multi_count,
            stub_count,
            inst.connections.len()
        );
        for (net_name, points) in &nets {
            let marker = if points.len() < 2 { " (stub)" } else { "" };
            println!(
                "{}  {} ({} pts){} : {}",
                indent,
                net_name,
                points.len(),
                marker,
                points.join(" ~ ")
            );
        }
    }

    for sub in &inst.sub_modules {
        print_nets(sub, depth + 1);
    }
}

// ============================================================================
// Global summary (new) — statistics net / connection count across entire module tree
// ============================================================================

/// Total connections / total nets for entire instance tree (deduplicated by (module_path, net_name)).
pub fn print_net_summary(inst: &MccProjectTree) {
    let mut total_conn = 0usize;
    let mut total_nets = 0usize;
    let mut total_modules = 0usize;
    let mut suspicious_dup = 0usize; // Duplicate paths appearing within same connection

    fn walk(
        inst: &MccProjectTree,
        total_conn: &mut usize,
        total_nets: &mut usize,
        total_modules: &mut usize,
        suspicious_dup: &mut usize,
    ) {
        *total_modules += 1;
        *total_conn += inst.connections.len();

        let mut local_nets: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

        for conn in &inst.connections {
            let net_name = conn
                .net_name
                .clone()
                .unwrap_or_else(|| format!("__net_{}", conn.id));
            local_nets.insert(net_name);

            // Check for duplicate paths within same connection (early signal of Pass2 data anomaly)
            let mut seen: std::collections::HashSet<&String> = std::collections::HashSet::new();
            for p in &conn.points {
                if !seen.insert(&p.path) {
                    *suspicious_dup += 1;
                }
            }
        }

        *total_nets += local_nets.len();

        for sub in &inst.sub_modules {
            walk(sub, total_conn, total_nets, total_modules, suspicious_dup);
        }
    }

    walk(
        inst,
        &mut total_conn,
        &mut total_nets,
        &mut total_modules,
        &mut suspicious_dup,
    );

    println!();
    println!("───────────────────────────────────────────────────────────────");
    println!(" Net Summary (whole tree)");
    println!("───────────────────────────────────────────────────────────────");
    println!("  modules:               {}", total_modules);
    println!("  connections (total):   {}", total_conn);
    println!("  unique nets per scope: {}", total_nets);
    if suspicious_dup > 0 {
        println!(
            "  ⚠ duplicate-point connections: {} (same path appears twice in one connection, may be Pass2 bug)",
            suspicious_dup
        );
    }
}
