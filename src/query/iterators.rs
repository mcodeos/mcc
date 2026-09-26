// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::db::infra::init::iter_interfaces;
use crate::db::infra::init::mcb_canonicalize_uri;
use crate::db::infra::init::uri_equivalent;
use crate::McURI;

// === pub fn mcb_module_count() -> usize { ===
/// Get the number of all modules (for debugging)
pub fn mcb_module_count() -> usize {
    crate::definition_space().workspace_modules().len()
}

// === pub fn mcb_get_first_module_name() -> Option<String> { ===
/// Get the name of the first module (for auto-detecting the top-level module)
pub fn mcb_get_first_module_name() -> Option<String> {
    crate::definition_space()
        .workspace_modules()
        .iter()
        .next()
        .map(|(sn, _)| sn.ident.to_string())
}

// === pub fn mcb_get_module_name_by_uri(uri: &McURI) -> Option<String> { ===
/// Get module name by matching URI suffix
pub fn mcb_get_module_name_by_uri(uri: &McURI) -> Option<String> {
    let canonical = mcb_canonicalize_uri(uri);
    crate::definition_space()
        .workspace_modules()
        .iter()
        .find(|(sn, _)| uri_equivalent(&sn.uri.as_uri(), uri.as_str(), &canonical))
        .map(|(sn, _)| sn.ident.to_string())
}

// === pub fn mcb_component_count() -> usize { ===
/// Get the number of loaded components
pub fn mcb_component_count() -> usize {
    crate::definition_space().workspace_components().len()
}

// === pub fn mcb_get_modules_in_file(uri: &McURI) -> Vec<String> { ===
/// Get all module names in a specific file (by URI)
///
/// Reads the registry's project (workspace) module view — the definitions
/// loaded into the active project. Key comparison uses [`uri_equivalent`]
/// (not raw `==`): registry keys are canonicalized (resolving
/// `/tmp`→`/private/tmp` etc.), so a caller's raw path must be tested
/// bidirectionally or real modules under a symlinked path would be reported
/// as absent — misclassifying them as virtual targets.
pub fn mcb_get_modules_in_file(uri: &McURI) -> Vec<String> {
    let canonical = mcb_canonicalize_uri(uri);
    crate::definition_space()
        .workspace_modules()
        .into_iter()
        .filter(|(sn, _)| uri_equivalent(&sn.uri.as_uri(), uri.as_str(), &canonical))
        .map(|(sn, _)| sn.ident.to_string())
        .collect()
}

// === pub enum TopPick { ===
/// The outcome of the implicit-top pick for an entry file ([`mcb_pick_top_module_by_uri`]).
pub enum TopPick {
    /// The file defines no module at all.
    NoModule,
    /// Exactly one candidate — use it.
    One(String),
    /// Several candidates survive the helper filter — the caller must resolve
    /// loudly (`--top`), never silently by sort order.
    Ambiguous(Vec<String>),
}

// === pub fn mcb_pick_top_module_by_uri(uri: &McURI) -> TopPick { ===
/// Pick the top module of an entry file for faces that need one but were not
/// told which (`mcc check`; build/vinst reads the same file in source order
/// and stays the sibling face).
///
/// Rule (U305④): among the modules defined in `uri`, a module that another
/// module of the same file instantiates (`SUB s1;`) is a helper, not a top
/// candidate. The old first-row pick read the registry's `(uri, ident)`
/// lexicographically sorted view, so a helper whose name sorted first was
/// deterministically built as the top — silently, with the real top's
/// instances never registering and its diagnostics never generated.
pub fn mcb_pick_top_module_by_uri(uri: &McURI) -> TopPick {
    let canonical = mcb_canonicalize_uri(uri);
    let in_file: Vec<_> = crate::definition_space()
        .workspace_modules()
        .into_iter()
        .filter(|(sn, _)| uri_equivalent(&sn.uri.as_uri(), uri.as_str(), &canonical))
        .collect();
    if in_file.is_empty() {
        return TopPick::NoModule;
    }
    if in_file.len() == 1 {
        return TopPick::One(in_file[0].0.ident.to_string());
    }
    // Names instantiated as a module child anywhere in this file. Note the
    // two name slots on a module instance: `Mc2Module.name` is the *instance*
    // name (`s1`), `Mc2Module.base.name` is the *class* name (`SUB`) — the
    // helper set is over class names.
    let helpers: Vec<String> = in_file
        .iter()
        .flat_map(|(_, module)| module.insts.iter_in_decl_order())
        .filter_map(|(_, inst)| match inst {
            crate::McInstance::Module(target) => Some(target.base.name.to_string()),
            _ => None,
        })
        .collect();
    let candidates: Vec<String> = in_file
        .iter()
        .map(|(sn, _)| sn.ident.to_string())
        .filter(|n| !helpers.contains(n))
        .collect();
    match candidates.len() {
        1 => TopPick::One(candidates.into_iter().next().unwrap()),
        _ => TopPick::Ambiguous(candidates),
    }
}

// === pub fn mcb_interface_count() -> usize { ===
/// Number of distinct interface definitions across workspace and system lib
/// (deduplicated by identity — a def in both tables counts once).
pub fn mcb_interface_count() -> usize {
    crate::definition_space().all_interfaces().len()
}

// === pub fn mcb_iter_modules() -> Vec<(String, String)> { ===
/// Iterate all registered project module definitions, return (name, uri) pairs.
pub fn mcb_iter_modules() -> Vec<(String, String)> {
    crate::definition_space()
        .workspace_modules()
        .into_iter()
        .map(|(sn, _)| (sn.ident.to_string(), sn.uri.to_string()))
        .collect()
}

// === pub fn mcb_iter_modules_with_span() -> Vec<(String, String, [usize; 2])> { ===
/// Like `mcb_iter_modules` but includes source span for LSP goto-def.
pub fn mcb_iter_modules_with_span() -> Vec<(String, String, [usize; 2])> {
    crate::definition_space()
        .workspace_modules()
        .into_iter()
        .map(|(sn, module)| {
            let span = &module.span;
            (
                sn.ident.to_string(),
                sn.uri.to_string(),
                [span.start, span.end],
            )
        })
        .collect()
}

// === pub fn mcb_iter_components() -> Vec<(String, String)> { ===
/// Iterate all registered component definitions (including project and system lib).
pub fn mcb_iter_components() -> Vec<(String, String)> {
    let mut items: Vec<(String, String)> = crate::definition_space()
        .all_components()
        .into_iter()
        .map(|(sn, _)| (sn.ident.to_string(), sn.uri.to_string()))
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    items
}

// === pub fn mcb_iter_components_with_span() -> Vec<(String, String, [usize; 2])> { ===
/// Like `mcb_iter_components` but includes source span for LSP goto-def.
pub fn mcb_iter_components_with_span() -> Vec<(String, String, [usize; 2])> {
    let mut items: Vec<_> = crate::definition_space()
        .all_components()
        .into_iter()
        .map(|(sn, comp)| {
            let span = &comp.span;
            (
                sn.ident.to_string(),
                sn.uri.to_string(),
                [span.start, span.end],
            )
        })
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    items
}

// === pub fn mcb_iter_interfaces() -> Vec<(String, String)> { ===
/// Iterate all registered project interface definitions.
pub fn mcb_iter_interfaces() -> Vec<(String, String)> {
    let mut items: Vec<(String, String)> = iter_interfaces()
        .iter()
        .map(|(space, _)| (space.ident.to_string(), space.uri.to_string()))
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    items
}

// === pub fn mcb_iter_interfaces_with_span() -> Vec<(String, String, [usize; 2])> { ===
/// Like `mcb_iter_interfaces` but includes source span for LSP goto-def.
pub fn mcb_iter_interfaces_with_span() -> Vec<(String, String, [usize; 2])> {
    let mut items: Vec<_> = iter_interfaces()
        .iter()
        .map(|(space, iface)| {
            let span = &iface.span;
            (
                space.ident.to_string(),
                space.uri.to_string(),
                [span.start, span.end],
            )
        })
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    items
}

// === pub fn mcb_iter_enums() -> Vec<(String, String)> { ===
/// Iterate all registered enum definitions (both workspace and system library).
pub fn mcb_iter_enums() -> Vec<(String, String)> {
    let mut items: Vec<(String, String)> = crate::definition_space()
        .all_enums()
        .into_iter()
        .map(|(sn, _)| (sn.ident.to_string(), sn.uri.to_string()))
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    items
}

// === pub fn mcb_iter_enums_with_span() -> Vec<(String, String, [usize; 2])> { ===
/// Same as `mcb_iter_enums`, but also returns the class span
/// `[start, end)` of the `enum PKG { ... }` head — needed by LSP
/// gotodef to know where to land when jumping to the class itself.
/// Includes both workspace and system library enums (deduped, §12.4 rule 1).
pub fn mcb_iter_enums_with_span() -> Vec<(String, String, [usize; 2])> {
    let mut items: Vec<(String, String, [usize; 2])> = crate::definition_space()
        .all_enums()
        .into_iter()
        .map(|(sn, def)| {
            let s = def.span;
            (
                sn.ident.to_string(),
                sn.uri.to_string(),
                [s[0] as usize, s[1] as usize],
            )
        })
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    items
}

// === pub fn mcb_iter_enum_values() -> Vec<(String, String, String, [u32; 2])> { ===
/// Iterate all enum value rows project-wide (both workspace and system library).
/// Returns `Vec<(class, value, uri, [u32;2])>` sorted by class then value.
pub fn mcb_iter_enum_values() -> Vec<(String, String, String, [u32; 2])> {
    let mut items: Vec<(String, String, String, [u32; 2])> = Vec::new();

    for (sn, def) in crate::definition_space().all_enums() {
        let class = sn.ident.to_string();
        let uri = sn.uri.to_string();
        for v in def.values.iter() {
            items.push((class.clone(), v.name.to_string(), uri.clone(), v.span));
        }
    }

    items.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    items
}

// === pub fn mcb_iter_ports() -> Vec<(String, String, String, String)> { ===
/// Iterate all module port definitions (psrc/psnk/psbi/io/in/out).
/// Returns Vec of (port_name, iotype, module_name, uri).
///
/// ★ CIMP §1 U119 (2026-09-19): **written order within each module**, not name
/// order. The listing is grouped by module (the module order `enumerate`
/// returns, sorted by `(uri, ident)`), and inside a module the ports come out in
/// the order they were declared — the same order the module's instantiated port
/// table uses. The explicit sort below keeps that order a property of this
/// function rather than something inherited from the iteration.
pub fn mcb_iter_ports() -> Vec<(String, String, String, String)> {
    use crate::semantic::common::IOType;

    // (module ordinal, written ordinal, name, iotype, module, uri)
    let mut ports: Vec<(usize, usize, String, String, String, String)> = Vec::new();

    for (module_ord, (sn, module)) in crate::definition_space()
        .workspace_modules()
        .into_iter()
        .enumerate()
    {
        let module_name = sn.ident.to_string();
        let uri = sn.uri.to_string();

        for (written_ord, (name, iotype)) in module.insts.iter_ports_in_decl_order().enumerate() {
            let io_name = match iotype {
                IOType::Power => "power".to_string(),
                IOType::In => "input".to_string(),
                IOType::Out => "output".to_string(),
                IOType::InOut => "inout".to_string(),
                IOType::Return | IOType::NonCon | IOType::None => continue, // Skip non-port declarations
            };
            ports.push((
                module_ord,
                written_ord,
                name.to_string(),
                io_name,
                module_name.clone(),
                uri.clone(),
            ));
        }
    }

    ports.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
    });
    ports
        .into_iter()
        .map(|(_, _, name, io_name, module_name, uri)| (name, io_name, module_name, uri))
        .collect()
}

// ── The organization directory's unit rows (CIMP §1 U120, 2026-09-19) ──
//
// `func` / `bus` / `clause` are the three units of the six (design §1) that
// are **not standalone defs**, so they are read off their own carriers rather
// than off the registry: a func off its host's `funcs` table, a bus off the
// declaring host's `bus_defs`, a clause off a body's statement/span parallel
// arrays. Each row carries its own **key** — `(host, name)` for a func, name
// plus host for a bus, `(uri, offset)` for a clause — because none of the three
// holds a `DefId` the way a component or a module does.
//
// Named structs rather than the tuples the older `mcb_iter_*` functions
// return: a row here has up to six fields, three of which are strings that a
// positional tuple would let a caller transpose silently.

/// One func member of a host def, as its host's member (design §12.1).
pub struct FuncRow {
    /// The func name, relative to its host.
    pub func: String,
    /// The module / component / recipe that carries the `funcs` table.
    pub host: String,
    /// That host's kind word (`module` / `component` / `recipe`).
    pub host_kind: String,
    pub uri: String,
    /// Byte offset of the func's own declaration, when the parse recorded one.
    pub offset: Option<usize>,
}

/// One declared bus (`MIC{P, N}`) of a host def.
///
/// Read off `McInstances::iter_bus_defs` — the **declaration** face, which
/// registers a bus once with its whole member list. A bus carries no `DefId`
/// (T12), so its key is its name *in its host*, and its members are the names
/// the declaration lists — never their `PointId`s, which belong to one build
/// (design §3 ④).
pub struct BusRow {
    pub bus: String,
    pub host: String,
    pub host_kind: String,
    /// Member names, in declaration order.
    pub members: Vec<String>,
    pub uri: String,
    /// Byte offset of the bus's base identifier.
    pub offset: usize,
}

/// One clause group — a statement and the position it sits at.
///
/// A clause is the one unit with **no declaration object** (design §1): what
/// the definition space holds is the statement (`McPhrase`) plus its span in a
/// parallel array, so the key is a position and never a name. `(uri, start)` is
/// a **canonical** key, which is why this is the one unit of the six that stays
/// comparable across builds as long as its file is unchanged (§3 ④).
pub struct ClauseRow {
    /// The module / component / recipe whose body carries the statement.
    pub host: String,
    pub host_kind: String,
    /// The func whose body carries it; empty for the host's own body.
    pub owner: String,
    pub uri: String,
    pub start: usize,
    /// Exclusive end of the statement, when the parse recorded a range. A
    /// function body's parallel array (`McFunction.stmt_offsets`) records only
    /// the start, so a func's clause rows carry `None` here rather than a
    /// fabricated end.
    pub end: Option<usize>,
}

// === pub fn mcb_iter_funcs() -> Vec<FuncRow> { ===
/// Every func member of the definition space, one [`FuncRow`] each, sorted by
/// `(uri, host, func)` — the key of a func is `(host, name)`, so this is key
/// order.
pub fn mcb_iter_funcs() -> Vec<FuncRow> {
    let mut rows: Vec<FuncRow> = Vec::new();
    for (kind, sn, data) in crate::definition_space().all_defs() {
        let funcs = match &data {
            crate::DefValue::Module(m) => &m.funcs,
            crate::DefValue::Component(c) => &c.funcs,
            crate::DefValue::Recipe(c) => &c.funcs,
            _ => continue,
        };
        for f in funcs.iter() {
            rows.push(FuncRow {
                func: f.name.to_string(),
                host: sn.ident.to_string(),
                host_kind: def_kind_word(kind),
                uri: sn.uri.to_string(),
                offset: f.span.as_ref().map(|s| s.start),
            });
        }
    }
    rows.sort_by(|a, b| {
        a.uri
            .cmp(&b.uri)
            .then_with(|| a.host.cmp(&b.host))
            .then_with(|| a.func.cmp(&b.func))
    });
    rows
}

// === pub fn mcb_iter_buses() -> Vec<BusRow> { ===
/// Every declared bus of the definition space, one [`BusRow`] each, sorted by
/// `(uri, host, bus)`.
pub fn mcb_iter_buses() -> Vec<BusRow> {
    let mut rows: Vec<BusRow> = Vec::new();
    for (kind, sn, data) in crate::definition_space().all_defs() {
        let insts = match &data {
            crate::DefValue::Module(m) => &m.insts,
            crate::DefValue::Component(c) => &c.insts,
            _ => continue,
        };
        for bus in insts.iter_bus_defs() {
            rows.push(BusRow {
                bus: bus.name.clone(),
                host: sn.ident.to_string(),
                host_kind: def_kind_word(kind),
                members: bus.members.iter().map(|(n, _)| n.clone()).collect(),
                uri: sn.uri.to_string(),
                offset: bus.span.start,
            });
        }
    }
    rows.sort_by(|a, b| {
        a.uri
            .cmp(&b.uri)
            .then_with(|| a.host.cmp(&b.host))
            .then_with(|| a.bus.cmp(&b.bus))
    });
    rows
}

// === pub fn mcb_iter_clauses() -> Vec<ClauseRow> { ===
/// Every clause group of the definition space, one [`ClauseRow`] each, sorted
/// by `(uri, start)` — the clause key, so this is key order.
///
/// ⚠ The two parallel arrays are not the same shape, and the rows say so
/// rather than papering over it: `McModule.stmt_spans` records a full range per
/// module-body statement, while `McFunction.stmt_offsets` records only the
/// start of each function-body statement.
pub fn mcb_iter_clauses() -> Vec<ClauseRow> {
    let mut rows: Vec<ClauseRow> = Vec::new();
    for (kind, sn, data) in crate::definition_space().all_defs() {
        let uri = sn.uri.to_string();
        let host = sn.ident.to_string();
        let word = def_kind_word(kind);
        let funcs = match &data {
            crate::DefValue::Module(m) => {
                for (i, span) in m.stmt_spans.iter().enumerate() {
                    if i < m.stmts.len() {
                        rows.push(ClauseRow {
                            host: host.clone(),
                            host_kind: word.clone(),
                            owner: String::new(),
                            uri: uri.clone(),
                            start: span.start,
                            end: Some(span.end),
                        });
                    }
                }
                &m.funcs
            }
            crate::DefValue::Component(c) => &c.funcs,
            crate::DefValue::Recipe(c) => &c.funcs,
            _ => continue,
        };
        for f in funcs.iter() {
            for (i, off) in f.stmt_offsets.iter().enumerate() {
                if i < f.stmts.len() {
                    rows.push(ClauseRow {
                        host: host.clone(),
                        host_kind: word.clone(),
                        owner: f.name.to_string(),
                        uri: uri.clone(),
                        start: *off as usize,
                        end: None,
                    });
                }
            }
        }
    }
    rows.sort_by(|a, b| {
        a.uri
            .cmp(&b.uri)
            .then_with(|| a.start.cmp(&b.start))
            .then_with(|| a.host.cmp(&b.host))
            .then_with(|| a.owner.cmp(&b.owner))
    });
    rows
}

/// The lowercase word of one def kind, for rows that carry a host kind.
///
/// A row's `host_kind` is the **host's** kind (design §12.1), and the word for
/// it is the registry's own ([`crate::DefKind::word`]) — spelled once, beside
/// the variants, so this face and the projection face cannot drift apart.
fn def_kind_word(kind: crate::DefKind) -> String {
    kind.word().to_string()
}
