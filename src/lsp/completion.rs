// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Code completion — provide completion candidates at a cursor position.
//!
//! Uses `unified_lookup_all_layered` to find visible symbols in the current
//! scope, grouped by name-space layer (P1..P5, see the completion design §8.1),
//! plus member-access enumeration for `inst.` / `this.` / enum-value scopes
//! (§5.6).

use crate::ast::macros::{
    MCAST_COMPONENT, MCAST_ENUM, MCAST_FUNCTION, MCAST_INTERFACE, MCAST_MODULE, MCAST_NAME,
};
use crate::ast::node::AstNode;
use crate::db::cmie::cmie::mcb_get_cmie_with_uri;
use crate::query::lookup::{find_container, CmieKind, ContainerRef};
use crate::semantic::component::McComponent;
use crate::semantic::mc_enum::McEnumDef;
use crate::semantic::mc_ifs::McInterface;
use crate::semantic::mc_inst::McInstance;
use crate::semantic::module::McModule;
use crate::semantic::scope::container_scope;
use crate::{LookupSymbolKind, McCMIE, McIds, McURI, ScopeFilter, ScopePath, SpaceLayer};
use serde_json::{json, Value};
use std::ops::Range;
use std::sync::Arc;

/// Get completion candidates for a prefix at a given file location.
/// Filters visible symbols by the optional prefix string.
pub fn complete(uri: &str, prefix: Option<&str>, scope: Option<&str>) -> Vec<Value> {
    let mc_uri = McURI::from(uri);
    let scope_path = if let Some(s) = scope {
        crate::db::infra::mc_code::McCode::scope_path_from_scope_str_public(&mc_uri, s)
    } else {
        ScopePath::file_level(&mc_uri)
    };

    let mut filter = ScopeFilter::new();
    if let Some(pref) = prefix {
        filter = filter.with_prefix(pref);
    }
    filter = filter.with_limit(50);

    let results = crate::unified_lookup_all(&scope_path, &filter);
    results
        .iter()
        .map(|r| {
            json!({
                "label": r.name,
                "kind": format!("{:?}", r.kind).to_lowercase(),
                "detail": r.container.as_ref().map(|c| format!("{:?} {}", c.kind, c.name)),
                "uri": r.uri,
                "span": { "start": r.span.start, "end": r.span.end },
            })
        })
        .collect()
}

// ============================================================================
// Position → authoritative scope (§8.1 item 5)
// ============================================================================

/// Symbol hit at a position: the enclosing container and function names,
/// resolved through the symbol lapper + SourceLocation intern tables.
struct ScopeHit {
    container: String,
    func: String,
}

/// Query the file's AST at `position` and derive the enclosing
/// (container, func) scope.
///
/// The symbol lapper is unsuitable for this: its ClassDef/FuncDef intervals
/// cover the class/function *name span* only, not the body, so a lapper point
/// query cannot answer "which scope encloses this position". This walk
/// mirrors the parse-time scope tracking in `lapper_func_define_role`: it
/// sorts all AST nodes by position, maintains module/component/interface/enum
/// and function stacks (popped when a node starts at or after a body end),
/// and records each container/function push as a `[start, end)` body interval.
/// The innermost interval that *contains* `position` is the authoritative
/// scope — a "nearest start at or before the cursor" query is wrong, because
/// nodes recorded inside a body that already ended would keep that body's
/// name alive past its closing brace.
fn scope_hit_at_pos(uri: &str, position: usize) -> Option<ScopeHit> {
    let mc_uri = McURI::from(uri);
    let ds = crate::definition_space();
    let mcfile = ds.source_file(&mc_uri)?;
    let ast = mcfile.ast.clone();

    // Collect all AST nodes via BFS. The stack pop condition (`node_start >=
    // end`) only holds for a position-monotonic scan, so sort by position.
    let mut all_nodes: Vec<AstNode> = {
        let mut acc: Vec<AstNode> = Vec::new();
        let mut stack: Vec<AstNode> = ast.iter().collect();
        while let Some(node) = stack.pop() {
            if let Some(sub) = node.get_sub_node() {
                for child in sub.iter() {
                    stack.push(child);
                }
            }
            acc.push(node);
        }
        acc
    };
    all_nodes.sort_by_key(|n| n.get_pos());

    let mut container_stack: Vec<(String, usize)> = Vec::new();
    let mut func_stack: Vec<(String, usize)> = Vec::new();
    let mut container_events: Vec<(usize, usize, String)> = Vec::new();
    let mut func_events: Vec<(usize, usize, String)> = Vec::new();
    for node in &all_nodes {
        let ntype = node.get_type();
        let node_start = node.get_pos() as usize;
        let node_end = node_start + node.get_len() as usize;
        while let Some((_, end)) = container_stack.last() {
            if node_start >= *end {
                container_stack.pop();
            } else {
                break;
            }
        }
        while let Some((_, end)) = func_stack.last() {
            if node_start >= *end {
                func_stack.pop();
            } else {
                break;
            }
        }
        if matches!(
            ntype,
            MCAST_MODULE | MCAST_COMPONENT | MCAST_INTERFACE | MCAST_ENUM
        ) {
            if let Some(sub) = node.get_sub_node() {
                if let Some(name_node) = sub.iter().find(|x| x.is_type(MCAST_NAME)) {
                    if let Some(ids_node) = name_node.get_sub_node() {
                        if let Some(ids) = McIds::new(&ids_node) {
                            container_events.push((node_start, node_end, ids.to_string()));
                            container_stack.push((ids.to_string(), node_end));
                        }
                    }
                }
            }
        }
        if ntype == MCAST_FUNCTION {
            if let Some(ids_node) = node.get_sub_node().and_then(|n| n.get_sub_node()) {
                if let Some(ids) = McIds::new(&ids_node) {
                    func_events.push((node_start, node_end, ids.to_string()));
                    func_stack.push((ids.to_string(), node_end));
                }
            }
        }
    }

    // Innermost interval containing `position`: among all body intervals with
    // start <= position < end, the one with the largest start is the deepest
    // enclosing container / function.
    let container = container_events
        .iter()
        .filter(|(s, e, _)| *s <= position && position < *e)
        .max_by_key(|(s, _, _)| *s)
        .map(|(_, _, n)| n.clone())?;
    let func = func_events
        .iter()
        .filter(|(s, e, _)| *s <= position && position < *e)
        .max_by_key(|(s, _, _)| *s)
        .map(|(_, _, n)| n.clone())
        .unwrap_or_default();
    Some(ScopeHit { container, func })
}

/// Derive the authoritative scope string at a cursor position: `"US513.i2c"`
/// inside a function, `"US513"` inside a container body, `""` for file-level
/// or unresolvable positions.
pub fn scope_at_pos(uri: &str, position: usize) -> String {
    let Some(hit) = scope_hit_at_pos(uri, position) else {
        return String::new();
    };
    if hit.container.is_empty() {
        String::new()
    } else if hit.func.is_empty() {
        hit.container
    } else {
        format!("{}.{}", hit.container, hit.func)
    }
}

/// Resolve the container that owns the cursor position (used as the base for
/// member-access resolution and `this.`). Returns `None` at file level or when
/// the position has no registered symbol in this file.
pub fn container_at_pos(uri: &str, position: usize) -> Option<ContainerRef> {
    let hit = scope_hit_at_pos(uri, position)?;
    if hit.container.is_empty() {
        return None;
    }
    let mc_uri = McURI::from(uri);
    find_container(&McIds::from(hit.container.as_str()), &mc_uri, CmieKind::Any)
}

/// Layered completion response (§8.1).
///
/// Position is the authoritative context: mcc derives the scope from the
/// symbol lapper + SourceLocation tables instead of trusting a caller-supplied
/// scope string. Each name-space layer (P1..P5) is serialized separately and
/// capped at [`crate::query::lookup::MAX_PER_LAYER`] entries; layers that hit
/// the cap are reported in `truncated_layers` so the client can mark them
/// incomplete (§8.5).
pub fn complete_at_pos(uri: &str, position: usize, prefix: Option<&str>) -> Value {
    let scope = scope_at_pos(uri, position);
    let mc_uri = McURI::from(uri);
    let scope_path = if scope.is_empty() {
        ScopePath::file_level(&mc_uri)
    } else {
        crate::db::infra::mc_code::McCode::scope_path_from_scope_str_public(&mc_uri, &scope)
    };

    let mut filter = ScopeFilter::new();
    if let Some(pref) = prefix {
        filter = filter.with_prefix(pref);
    }

    let (results, truncated) = crate::unified_lookup_all_layered(&scope_path, &filter);

    let layer_order = [
        SpaceLayer::P1,
        SpaceLayer::P2,
        SpaceLayer::P3,
        SpaceLayer::P4,
        SpaceLayer::P5,
    ];
    let mut layers = serde_json::Map::new();
    for layer in layer_order {
        let items: Vec<Value> = results
            .iter()
            .filter(|r| r.layer == layer)
            .map(|r| {
                json!({
                    "name": r.name,
                    "kind": r.kind.as_str(),
                    "scope": r.scope,
                    "uri": r.uri,
                    "span": { "start": r.span.start, "end": r.span.end },
                })
            })
            .collect();
        if !items.is_empty() {
            layers.insert(layer.as_str().to_string(), json!(items));
        }
    }

    let mut truncated_layers: Vec<&str> = truncated.iter().map(|l| l.as_str()).collect();
    truncated_layers.sort_unstable();

    json!({
        "scope_path": scope,
        "layers": Value::Object(layers),
        "truncated_layers": truncated_layers,
    })
}

// ============================================================================
// Member-access enumeration (§5.6 / §8.1 item 4)
// ============================================================================

/// A single member candidate.
struct MemberItem {
    name: String,
    kind: LookupSymbolKind,
    uri: String,
    span: Range<usize>,
}

/// Owned member-enumeration source — the class definition behind `inst.`,
/// `this.`, or a bare class name.
enum MemberSource {
    Component(Arc<McComponent>),
    Module(Arc<McModule>),
    Interface(Arc<McInterface>),
    Enum(Arc<McEnumDef>),
}

/// Member-access completion: enumerate the members of `member_root` (an
/// instance resolved through the cursor container's category chain, `this`
/// for the cursor's own container, or a bare class name such as an enum) and
/// return them as the `Member` layer (§5.6).
pub fn complete_member_at_pos(
    uri: &str,
    position: usize,
    member_root: &str,
    prefix: Option<&str>,
) -> Value {
    let scope = scope_at_pos(uri, position);
    let mut items = Vec::new();
    if let Some(source) = resolve_member_source(uri, position, member_root) {
        enumerate_source(&source, &mut items);
    }

    // Dedup by (name, kind) — pins may appear both as whole names and as
    // expanded `pin_id_to_names` entries, and DOT ports expand to multi-spans.
    let mut unique: Vec<MemberItem> = Vec::new();
    for it in items {
        if unique
            .iter()
            .any(|u| u.name == it.name && u.kind == it.kind)
        {
            continue;
        }
        unique.push(it);
    }

    let member_items: Vec<Value> = unique
        .iter()
        .filter(|it| prefix.map_or(true, |p| it.name.starts_with(p)))
        .take(crate::query::lookup::MAX_PER_LAYER)
        .map(|it| {
            json!({
                "name": it.name,
                "kind": it.kind.as_str(),
                "scope": scope,
                "uri": it.uri,
                "span": { "start": it.span.start, "end": it.span.end },
            })
        })
        .collect();

    json!({
        "scope_path": scope,
        "layers": { "Member": member_items },
        "truncated_layers": [],
    })
}

/// Resolve `member_root` to a member-enumeration source.
///
/// Priority: `this` → the cursor's own container; otherwise the instance
/// chain of the cursor container (pins/insts/funcs resolve to class
/// definitions); finally a bare class-name lookup (enum values, cross-file
/// classes via the unified P1-P5 policy).
fn resolve_member_source(uri: &str, position: usize, member_root: &str) -> Option<MemberSource> {
    if member_root == "this" {
        let cur = container_at_pos(uri, position)?;
        return Some(match cur {
            ContainerRef::Component(c) => MemberSource::Component(c),
            ContainerRef::Module(m) => MemberSource::Module(m),
            ContainerRef::Interface(i) => MemberSource::Interface(i),
            ContainerRef::Enum(e) => MemberSource::Enum(e),
        });
    }

    if let Some(cur) = container_at_pos(uri, position) {
        if let Some(resolved) = container_scope(&cur).resolve(member_root) {
            return match resolved.inst {
                McInstance::Component(c) => Some(MemberSource::Component(c.base.clone())),
                McInstance::Module(m) => Some(MemberSource::Module(m.base.clone())),
                McInstance::Interface(i) => Some(MemberSource::Interface(i.base.clone())),
                McInstance::EnumVal {
                    def_uri, enum_name, ..
                } => enum_source(&def_uri?, &enum_name),
                _ => None,
            };
        }
    }

    // Bare class name — e.g. `PKG.` (enum) or a cross-file class.
    let ids = McIds::from(member_root);
    let from_uri = McURI::from(uri);
    let (cmie, _def_uri) = mcb_get_cmie_with_uri(&ids, &from_uri)?;
    Some(match cmie {
        McCMIE::Component(c) => MemberSource::Component(c),
        McCMIE::Module(m) => MemberSource::Module(m),
        McCMIE::Interface(i) => MemberSource::Interface(i),
        McCMIE::Enum(e) => MemberSource::Enum(e),
    })
}

/// Locate an enum class by (defining uri, class name) and return it as a
/// member source (enum-value enumeration is scope-qualified, §5.6).
fn enum_source(def_uri: &str, enum_name: &str) -> Option<MemberSource> {
    let cr = find_container(
        &McIds::from(enum_name),
        &McURI::from(def_uri),
        CmieKind::Enum,
    )?;
    match cr {
        ContainerRef::Enum(e) => Some(MemberSource::Enum(e)),
        _ => None,
    }
}

fn enumerate_source(source: &MemberSource, out: &mut Vec<MemberItem>) {
    match source {
        MemberSource::Component(c) => enumerate_component(c, out),
        MemberSource::Module(m) => enumerate_module(m, out),
        MemberSource::Interface(i) => enumerate_interface(i, out),
        MemberSource::Enum(e) => enumerate_enum(e, out),
    }
}

/// Component members: pin names + pin ids (§5.6 ②③), interface/IO-bus
/// instances (④), funcs (⑤).
fn enumerate_component(c: &McComponent, out: &mut Vec<MemberItem>) {
    let uri = c.uri.to_string();
    for name in c.pins.names_to_id.keys() {
        let span = c.pins.pin_name_spans.get(name).cloned().unwrap_or(0..0);
        out.push(MemberItem {
            name: name.clone(),
            kind: LookupSymbolKind::Pin,
            uri: uri.clone(),
            span,
        });
    }
    for pin_id in c.pins.pin_id_to_names.keys() {
        let span = c.pins.pin_id_spans.get(pin_id).cloned().unwrap_or(0..0);
        out.push(MemberItem {
            name: pin_id.clone(),
            kind: LookupSymbolKind::Pin,
            uri: uri.clone(),
            span,
        });
    }
    for (name, (io_type, _)) in c.insts.insts() {
        if matches!(
            io_type,
            crate::IOType::None | crate::IOType::NonCon | crate::IOType::Label
        ) {
            continue;
        }
        let span = c.insts.get_port_span(name).unwrap_or(0..0);
        out.push(MemberItem {
            name: name.clone(),
            kind: LookupSymbolKind::Instance,
            uri: uri.clone(),
            span,
        });
    }
    for f in c.funcs.iter() {
        let span = f.span.clone().unwrap_or(0..0);
        out.push(MemberItem {
            name: f.name.to_string(),
            kind: LookupSymbolKind::Function,
            uri: uri.clone(),
            span,
        });
    }
}

/// Module members: ports (§5.6 ①), labels (②), non-port instances (③),
/// funcs (④).
fn enumerate_module(m: &McModule, out: &mut Vec<MemberItem>) {
    let uri = m.uri.to_string();
    for (name, _, span) in m.insts.iter_ports_with_span() {
        out.push(MemberItem {
            name: name.to_string(),
            kind: LookupSymbolKind::Port,
            uri: uri.clone(),
            span,
        });
    }
    for (name, _, span) in m.insts.iter_labels_with_span() {
        out.push(MemberItem {
            name: name.to_string(),
            kind: LookupSymbolKind::Label,
            uri: uri.clone(),
            span,
        });
    }
    for (name, (io_type, inst)) in m.insts.insts() {
        if !matches!(io_type, crate::IOType::None) || matches!(inst, McInstance::Label(_)) {
            continue;
        }
        let span = m.insts.get_port_span(name).unwrap_or(0..0);
        out.push(MemberItem {
            name: name.clone(),
            kind: LookupSymbolKind::Instance,
            uri: uri.clone(),
            span,
        });
    }
    for f in m.funcs.iter() {
        let span = f.span.clone().unwrap_or(0..0);
        out.push(MemberItem {
            name: f.name.to_string(),
            kind: LookupSymbolKind::Function,
            uri: uri.clone(),
            span,
        });
    }
}

/// Interface members: pin names (§5.6 interface row).
fn enumerate_interface(i: &McInterface, out: &mut Vec<MemberItem>) {
    let uri = i.uri.to_string();
    for (name, span) in &i.pins.pin_name_spans {
        out.push(MemberItem {
            name: name.clone(),
            kind: LookupSymbolKind::Pin,
            uri: uri.clone(),
            span: span.clone(),
        });
    }
}

/// Enum members: values (§5.6 enum.val row).
fn enumerate_enum(e: &McEnumDef, out: &mut Vec<MemberItem>) {
    let uri = e.uri.to_string();
    for v in &e.values {
        let span = v.span[0] as usize..v.span[1] as usize;
        out.push(MemberItem {
            name: v.name.to_string(),
            kind: LookupSymbolKind::EnumValue,
            uri: uri.clone(),
            span,
        });
    }
}
