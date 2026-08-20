// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Shared module-nesting overview (the "Hierarchy" tree) used by `mcc verify`
//! and `mcc show dianlu`: the top module as root, each module node carrying
//! every instance in source order — declared (`[src]`), declareb (`[decl]`)
//! and funcall-generated anonymous (`[gen]`) — so the whole instance
//! structure is visible at a glance before the per-module detail sections.
//!
//! Pipeline: [`collect_module_nodes`] walks a Pass2 `McModuleInst` tree into
//! per-module JSON nodes (the same `{module, uri, instances}` shape `verify`
//! emits for its sections), [`build_hierarchy`] nests those nodes by their
//! dot path, and [`render_hierarchy_text`] draws the ASCII tree.

use crate::{InstOrigin, McInstance, McModuleInst};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

/// Instance families extracted from one module, shared by `verify`'s
/// per-module instance report and by the hierarchy tree.
pub struct InstanceFamilies {
    /// Declared physical instances (component / module / interface / bus /
    /// label) with kind, source line and class, sorted by declaration line.
    pub source: Vec<(String, String, u32, Option<String>)>,
    /// Declareb instances (`C1::CAP()`) with their declaration line and
    /// component class (`CAP`).
    pub declareb: Vec<(String, u32, String)>,
    /// Function-generated anonymous instances (e.g. `.Cap()` expansion) with
    /// their construction line (0 when unknown), component class, and the
    /// caller chain (`<caller_inst>.<func_name>`, empty when none) from the
    /// expansion record that produced them.
    pub generated: Vec<(String, u32, String, String)>,
    /// Every name that counts as declared (source + declareb).
    pub source_names: HashSet<String>,
}

/// Interface type as `Base(params)` (e.g. `DC(5V)`), matching the `show
/// ports` rendering (show.rs `port_type_members`). A class without arguments
/// still renders the parentheses: `DC()`.
fn iface_class(i: &crate::Mc2Interface) -> String {
    let base = i.base_name();
    let params: Vec<String> = i.params.iter().map(|p| p.to_string()).collect();
    format!("{base}({})", params.join(", "))
}

/// Component class with its written parameter list from the Pass1 instance
/// (source view, argument order preserved): `RES(0R, R0603)`. `NC` modifiers
/// (e.g. `wm7121(NC)`) are dropped; a class without remaining arguments
/// renders the empty parentheses (`LPA4871()`).
fn comp_class_raw(base: &str, params: &[crate::McParamValue]) -> String {
    let vals: Vec<String> = params
        .iter()
        .filter(|p| !matches!(p, crate::McParamValue::NC(_)))
        .map(|p| p.to_string())
        .collect();
    format!("{base}({})", vals.join(", "))
}

/// Interface binding of a module-header bus port, from its parameter
/// declaration: `module SPEAKER_M(USB_VBUS_1{VDD_3V, GND}::DC(3.3V))` renders
/// the bus row's class column as `DC(3.3V)`. The `McBus` instance itself does
/// not carry the `::IFACE(args)` binding — it lives in the module's parameter
/// declaration table. Returns `None` for buses without a binding.
fn bus_class(def: &crate::McModule, name: &str) -> Option<String> {
    def.params
        .iter()
        .find(|d| {
            d.get_primary_name()
                .map(|p| p.split(['{', '[']).next() == Some(name))
                .unwrap_or(false)
        })
        .and_then(|d| d.interface_annotation())
        .map(|(class, args)| format!("{class}({})", args.join(", ")))
}

/// Extract the declared / declareb / funcall-generated instance families of
/// one module. Source-declared physical instance names are the contract;
/// engine-generated pseudo instances (`@RES1` auto ports), non-physical
/// entries (List / BusRef / pins / attrs / funcs / enums) and array-form
/// interfaces (`[VDD_3V3, GND]`, which expand to member labels instead of a
/// bus) are not part of it. Each entry carries its source line for the
/// `L<n>` column.
pub fn extract_instance_families(
    inst: &McModuleInst,
    content: &Option<String>,
) -> InstanceFamilies {
    let mut source: Vec<(String, String, u32, Option<String>)> = Vec::new();
    let mut source_names: HashSet<String> = HashSet::new();
    let line_of_span = |sp: &std::ops::Range<usize>| -> u32 {
        content
            .as_ref()
            .map(|c| line_of_byte(c, sp.start))
            .unwrap_or(0)
    };
    // Line fallbacks for entries whose declaration span is not recorded in
    // the Pass1 inst table: module-header parameter ports (`dc{VDD_3V3,GND}`
    // and `in [VDD_3V3, GND]` rows) carry their span in `def.params`, and
    // inline net labels (e.g. `USB_VBUS` inside a connection statement)
    // resolve through the expanded label NetPoint's src_pos.
    let param_line = |name: &str| -> u32 {
        inst.def
            .params
            .iter_defs_with_span()
            .find(|(n, _)| *n == name)
            .map(|(_, s)| line_of_span(&s))
            .unwrap_or(0)
    };
    let label_line = |name: &str| -> u32 {
        inst.get_labels()
            .get(name)
            .and_then(|p| p.src_pos.as_ref().map(|pos| pos.offset as usize))
            .map(|pos| content.as_ref().map(|c| line_of_byte(c, pos)).unwrap_or(0))
            .unwrap_or(0)
    };
    // Bracket-vec port members (`in [VDD_3V3, GND]::DC(3.3V)`) are stored
    // under the whole-bracket key (`[VDD_3V3, GND]`) with no per-member
    // span; match the member name against those keys for the declaration
    // line of the header row. A member may appear in several bracket keys
    // (e.g. `GND` in `[VDD_3V3, GND]` and `[VCC_1V2, GND]`); `port_spans()`
    // is a HashMap, so pick the earliest declaration span instead of the
    // iteration order to keep the line deterministic.
    let member_line = |name: &str| -> u32 {
        inst.def
            .insts
            .port_spans()
            .iter()
            .filter_map(|(key, spans)| {
                if !(key.starts_with('[') || key.starts_with('{')) {
                    return None;
                }
                let mut members = key
                    .trim_matches(|c| c == '[' || c == ']' || c == '{' || c == '}')
                    .split(',')
                    .map(str::trim);
                members
                    .any(|m| m == name)
                    .then(|| spans.first().cloned())
                    .flatten()
            })
            .min_by_key(|sp| sp.start)
            .map(|sp| line_of_span(&sp))
            .unwrap_or(0)
    };
    for (name, mc_inst) in inst.def.insts.iter() {
        if name.starts_with('@') || name.starts_with('[') {
            continue;
        }
        let (kind, class) = match mc_inst {
            McInstance::Component(c) => (
                Some("component"),
                Some(comp_class_raw(&c.base.name.to_string(), &c.params)),
            ),
            McInstance::Module(m) => (Some("module"), Some(m.base.name.to_string())),
            McInstance::Interface(i) => (Some("interface"), Some(iface_class(i))),
            McInstance::Bus(_) => (Some("bus"), bus_class(&inst.def, &name)),
            McInstance::Label(_) => (Some("label"), None),
            _ => (None, None),
        };
        if let Some(kind) = kind {
            let line = inst
                .def
                .insts
                .get_port_span(name)
                .map(|sp| line_of_span(&sp))
                .or_else(|| {
                    let l = param_line(name);
                    if l > 0 {
                        Some(l)
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    let l = label_line(name);
                    if l > 0 {
                        Some(l)
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    let l = member_line(name);
                    if l > 0 {
                        Some(l)
                    } else {
                        None
                    }
                })
                .unwrap_or(0);
            source.push((name.to_string(), kind.to_string(), line, class));
            source_names.insert(name.to_string());
        }
    }
    // Source rows in file order: sort by the recorded declaration line so the
    // instance list mirrors the order the parts appear in the source module.
    // Unknown lines (0) go last; the stable sort keeps the alphabetical
    // BTreeMap order within the same line.
    source.sort_by_key(|(_, _, l, _)| if *l == 0 { u32::MAX } else { *l });
    // Declareb instances (`C1::CAP()`) bypass `parse_declare`, so their names
    // never enter `insts`; they are recorded in the declareb hint table and
    // expand with a FuncCall origin, so the component class comes from the
    // expanded component def.
    let comp_class: HashMap<String, String> = inst
        .components
        .iter()
        .map(|c| {
            (
                c.name.clone(),
                comp_class_raw(&c.def.name.to_string(), &c.raw_params),
            )
        })
        .collect();
    let mut declareb: Vec<(String, u32, String)> = Vec::new();
    for (name, (_, span)) in inst.def.insts.iter_declareb_defs() {
        source_names.insert(name.clone());
        declareb.push((
            name.clone(),
            line_of_span(&span),
            comp_class.get(name.as_str()).cloned().unwrap_or_default(),
        ));
    }
    // Declareb definitions live in a HashMap (`declareb_defs`), so same-line
    // entries need the name as a tiebreaker for a deterministic order.
    declareb.sort_by_key(|(n, l, _)| (if *l == 0 { u32::MAX } else { *l }, n.clone()));

    // Funcall-generated anonymous components whose name does not match a
    // source declaration (e.g. `.Cap()` expansion instances). Their
    // `InstOrigin` construction site is a byte offset (decision A, §7.1); the
    // owning file comes from the expansion record (`call_site` / `def_site`),
    // which also carries the caller chain (`caller_inst.func_name`) for the
    // call-site view (§5.1 `generated`).
    let mut generated: Vec<(String, u32, String, String)> = Vec::new();
    for comp in &inst.components {
        if let InstOrigin::FuncCall { .. } = comp.origin {
            if !source_names.contains(&comp.name) {
                let (line, caller) = comp
                    .expansion_id
                    .and_then(|k| inst.expansion.records.get(k))
                    .map(|r| {
                        let line = r
                            .call_site
                            .clone()
                            .or(r.def_site.clone())
                            .and_then(|pos| {
                                std::fs::read_to_string(pos.uri.as_str())
                                    .ok()
                                    .map(|c| line_of_byte(&c, pos.offset as usize))
                                    .filter(|l| *l > 0)
                            })
                            .unwrap_or(0);
                        // Caller chain (§5.1 `generated`): walk the expansion
                        // record parent chain to the top-level record so
                        // anonymous products of a method/function call display
                        // the whole call path (e.g. `uC.i2c() → R_PULLUP_1`),
                        // not the innermost record (which for a nested
                        // ComponentCtor carries no caller).
                        let caller = (|| -> Option<String> {
                            let mut cur = comp.expansion_id?;
                            loop {
                                let rec = inst.expansion.records.get(cur)?;
                                match rec.parent {
                                    Some(p) => cur = p,
                                    None => {
                                        return match (&rec.caller_inst, rec.func_name.as_str()) {
                                            (Some(c), f) if !f.is_empty() => {
                                                Some(format!("{c}.{f}"))
                                            }
                                            _ => None,
                                        };
                                    }
                                }
                            }
                        })()
                        .unwrap_or_default();
                        (line, caller)
                    })
                    .unwrap_or((0, String::new()));
                generated.push((
                    comp.name.clone(),
                    line,
                    comp_class_raw(&comp.def.name.to_string(), &comp.raw_params),
                    caller,
                ));
            }
        }
    }
    generated.sort_by_key(|(_, l, _, _)| if *l == 0 { u32::MAX } else { *l });

    InstanceFamilies {
        source,
        declareb,
        generated,
        source_names,
    }
}

/// Recursively collect one module node per module in source order — the same
/// `{module, uri, instances}` shape `verify` emits for its per-module
/// sections — so [`build_hierarchy`] treats `verify` and `show dianlu`
/// alike.
pub fn collect_module_nodes(inst: &McModuleInst, path: &str) -> Vec<Value> {
    let content = std::fs::read_to_string(&inst.def_uri.to_string()).ok();
    let fam = extract_instance_families(inst, &content);
    let instances = json!({
        "source": fam.source.iter().map(|(n, k, l, cl)| json!({"name": n, "kind": k, "line": l, "class": cl})).collect::<Vec<_>>(),
        "declareb": fam.declareb.iter().map(|(n, l, cl)| json!({"name": n, "line": l, "class": cl})).collect::<Vec<_>>(),
        "generated": fam.generated.iter().map(|(n, l, cl, caller)| json!({"name": n, "line": l, "class": cl, "caller": caller})).collect::<Vec<_>>(),
    });
    let mut out = vec![json!({
        "module": path,
        "uri": inst.def_uri.to_string(),
        "instances": instances,
    })];
    for sub in &inst.sub_modules {
        out.extend(collect_module_nodes(sub, &format!("{path}.{}", sub.name)));
    }
    out
}

/// Global module-nesting overview: the top module as root, each module node
/// carrying its declared instances (labels / buses / interfaces / components
/// / sub-modules) in source order; sub-module instances expand recursively
/// into their own module node so the whole instance structure is visible at
/// a glance before the per-module detail sections.
pub fn build_hierarchy(modules: &[Value]) -> Value {
    let mut by_path: BTreeMap<&str, &Value> = BTreeMap::new();
    for m in modules {
        if let Some(p) = m["module"].as_str() {
            by_path.insert(p, m);
        }
    }
    let Some(root) = modules.first() else {
        return json!({ "module": "", "uri": "", "entries": [] });
    };

    fn module_node(m: &Value, by_path: &BTreeMap<&str, &Value>) -> Value {
        let path = m["module"].as_str().unwrap_or("").to_string();
        // Declared instances (source) plus declareb instances (`C4::CAP()`)
        // and funcall-generated anonymous instances, merged and stable-sorted
        // by declaration line so the tree mirrors the order the parts appear
        // in the source module.
        let mut entries: Vec<Value> = Vec::new();
        if let Some(src) = m["instances"]["source"].as_array() {
            for e in src {
                entries.push(json!({
                    "name": e["name"],
                    "kind": e["kind"],
                    "line": e["line"],
                    "class": e["class"],
                    "origin": "src",
                }));
            }
        }
        if let Some(db) = m["instances"]["declareb"].as_array() {
            for e in db {
                entries.push(json!({
                    "name": e["name"],
                    "kind": "declareb",
                    "line": e["line"],
                    "class": e["class"],
                    "origin": "decl",
                }));
            }
        }
        if let Some(gen) = m["instances"]["generated"].as_array() {
            for e in gen {
                entries.push(json!({
                    "name": e["name"],
                    "kind": "component",
                    "line": e["line"],
                    "class": e["class"],
                    "origin": "gen",
                    "caller": e["caller"],
                }));
            }
        }
        entries.sort_by_key(|e| e["line"].as_u64().unwrap_or(u64::MAX));
        for node in &mut entries {
            if node["kind"].as_str() == Some("module") {
                let name = node["name"].as_str().unwrap_or("");
                let child_path = format!("{path}.{name}");
                if let Some(child) = by_path.get(child_path.as_str()) {
                    node["module"] = module_node(child, by_path);
                }
            }
        }
        json!({
            "module": path,
            "uri": m["uri"],
            "entries": entries,
        })
    }

    module_node(root, &by_path)
}

/// Render the global module-nesting overview as an ASCII tree: the top
/// module header, then every instance in source order — declared (`[src]`),
/// declareb (`[decl]`) and funcall-generated anonymous (`[gen]`) — each as
/// `Line [origin] kind name class`. Sub-module instances become branch nodes
/// whose children are that module's own instances.
pub fn render_hierarchy_text(out: &mut String, h: &Value) {
    let module = h["module"].as_str().unwrap_or("");
    let uri = h["uri"].as_str().unwrap_or("");
    let file = Path::new(uri)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| uri.to_string());
    let name_w = hierarchy_max_name(h).max(1) + 1;
    let line_w = format!("L{}", hierarchy_max_line(h)).len();
    let _ = writeln!(out, "{module}  ({file})");
    render_hierarchy_entries(out, h, "  ", name_w, line_w);
}

/// Longest declared instance name across the whole hierarchy, so the kind /
/// class columns line up per line (prefixes differ, so the alignment is
/// per-line only).
fn hierarchy_max_name(h: &Value) -> usize {
    let mut max = 0;
    if let Some(entries) = h["entries"].as_array() {
        for e in entries {
            if let Some(n) = e["name"].as_str() {
                max = max.max(n.chars().count());
            }
            if let Some(sub) = e.get("module") {
                max = max.max(hierarchy_max_name(sub));
            }
        }
    }
    max
}

/// Highest source line across the whole hierarchy, so the `L<n>` column is
/// padded to a fixed width and the `[origin]` column stays aligned even when
/// line numbers grow past one digit.
fn hierarchy_max_line(h: &Value) -> u64 {
    let mut max = 0;
    if let Some(entries) = h["entries"].as_array() {
        for e in entries {
            if let Some(l) = e["line"].as_u64() {
                max = max.max(l);
            }
            if let Some(sub) = e.get("module") {
                max = max.max(hierarchy_max_line(sub));
            }
        }
    }
    max
}

fn render_hierarchy_entries(
    out: &mut String,
    node: &Value,
    prefix: &str,
    name_w: usize,
    line_w: usize,
) {
    let Some(entries) = node["entries"].as_array() else {
        return;
    };
    for (i, e) in entries.iter().enumerate() {
        let last = i + 1 == entries.len();
        let branch = if last { "`-- " } else { "|-- " };
        let cont = if last { "    " } else { "|   " };
        let ln = e["line"]
            .as_u64()
            .map(|l| format!("L{l}"))
            .unwrap_or_default();
        let origin = e["origin"].as_str().unwrap_or("");
        let name = e["name"].as_str().unwrap_or("");
        let kind = e["kind"].as_str().unwrap_or("");
        let class = e["class"].as_str().unwrap_or("");
        if let Some(sub) = e.get("module") {
            let sub_uri = sub["uri"].as_str().unwrap_or("");
            let sub_file = Path::new(sub_uri)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| sub_uri.to_string());
            let _ = writeln!(
                out,
                "{prefix}{branch}{ln:<line_w$}  [{origin}] {kind:<9} {name:<name_w$} {class}  ({sub_file})"
            );
            render_hierarchy_entries(out, sub, &format!("{prefix}{cont}  "), name_w, line_w);
            // One blank line after each sub-module subtree so sibling module
            // branches read as separate blocks; the bar keeps the parent
            // branch visually connected.
            if !last {
                let _ = writeln!(out, "{prefix}|");
            }
        } else {
            let caller = e["caller"]
                .as_str()
                .filter(|c| !c.is_empty())
                .map(|c| format!(" ({c})"))
                .unwrap_or_default();
            let _ = writeln!(
                out,
                "{prefix}{branch}{ln:<line_w$}  [{origin}] {kind:<9} {name:<name_w$} {class}{caller}"
            );
        }
    }
}

/// 1-based line number of a byte offset within `content`.
pub fn line_of_byte(content: &str, offset: usize) -> u32 {
    let end = offset.min(content.len());
    content.as_bytes()[..end]
        .iter()
        .filter(|&&b| b == b'\n')
        .count() as u32
        + 1
}
