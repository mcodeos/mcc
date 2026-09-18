// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use super::*;
use std::collections::{BTreeMap, BTreeSet};

// === handle_import (design world-repartition-design.md §6.2) ===

/// Why an `import` request produced no report. Both arms are exit `2` (input
/// construction failed); they stay split so the message names which half.
pub enum ImportError {
    /// The artifact could not be read, or no producer writes this format.
    Input(String),
    /// The scope world could not be built or exported.
    Build(String),
}

/// MCP face of `mcc import` (design §6.2: one engine behind CLI and MCP).
pub fn handle_import(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize)]
    struct ImportParams {
        file: String,
        entry: String,
        #[serde(default)]
        format: Option<String>,
        #[serde(default)]
        top: Option<String>,
        #[serde(default)]
        libs: Vec<String>,
    }
    let p: ImportParams = parse_strict(params)?;
    let format = crate::cli::ImportFormat::from_name(p.format.as_deref().unwrap_or("netlist"));
    import_report(&p.file, format, &p.entry, p.top.as_deref(), &p.libs).map_err(|e| match e {
        ImportError::Input(s) => JsonRpcError::custom(-32602, &s),
        ImportError::Build(s) => JsonRpcError::custom(-32603, &s),
    })
}

/// Read an EDA artifact back and report how it differs from what the scope world
/// exports right now (design §6.2).
///
/// The default half only: a read-back is a **convergence suggestion**, so nothing
/// is written — no `.mc` fragment, no store mutation. Generating a fragment is
/// the overlay-gated half and a later batch.
///
/// The reference side is the current world run through the *same* reader, so the
/// comparison is between two readings of the same format and an exporter quirk
/// lands on both sides equally. Export-then-read-back of an unchanged world is
/// therefore an empty difference by construction.
pub fn import_report(
    path: &str,
    format: crate::cli::ImportFormat,
    entry: &str,
    top: Option<&str>,
    libs: &[String],
) -> Result<Value, ImportError> {
    let kind = format.export_kind().ok_or_else(|| {
        ImportError::Input(format!(
            "import: no exporter writes an '{}' netlist yet, so there is no reference side to read",
            format.name()
        ))
    })?;
    let text = std::fs::read_to_string(path)
        .map_err(|e| ImportError::Input(format!("import: cannot read '{}': {}", path, e)))?;
    let foreign = read_model(&text, format)
        .map_err(|e| ImportError::Input(format!("import: {}: {}", path, e)))?;

    let (tree, table, arena, inst_store, diags) = crate::export::build_tree_diags(entry, top, libs)
        .map_err(|e| ImportError::Build(format!("import: {}", e)))?;
    let top = top
        .map(str::to_string)
        .or_else(crate::mcb_get_first_module_name)
        .unwrap_or_else(|| "?".to_string());

    let (exported, _, _) = crate::export::build_payload(
        &tree,
        &table,
        &arena,
        &inst_store,
        &top,
        kind.id(),
        crate::cli::OutputFormat::Text.id(),
    );
    let reference = read_model(&exported, format)
        .map_err(|e| ImportError::Build(format!("import: own export: {}", e)))?;

    let changes = diff_models(&reference, &foreign, &table);
    Ok(json!({
        "schema_version": "import.1.0",
        "format": format.name(),
        "uri": entry,
        "top": top,
        "count": changes.len(),
        "changes": changes,
        "diagnostics": diags.len(),
        "world_ver": crate::stages::world_ver::world_ver(),
    }))
}

/// The two sides of the comparison in one shape. Refs and net names are names,
/// so every comparison here is exact string equality.
#[derive(Default)]
struct EdaModel {
    /// Instance refs (source instance leaf names).
    components: BTreeSet<String>,
    /// Net name -> point tokens (`REF.PIN`, or a bare label with no owner).
    nets: BTreeMap<String, BTreeSet<String>>,
}

fn read_model(text: &str, format: crate::cli::ImportFormat) -> Result<EdaModel, String> {
    match format {
        crate::cli::ImportFormat::Netlist => Ok(read_netlist(text)),
        crate::cli::ImportFormat::KiCad => read_kicad(text),
        crate::cli::ImportFormat::EasyEda => Err("no easyeda reader".to_string()),
    }
}

/// The text netlist shape `netlist::build_netlist` writes: one `NAME: tok tok`
/// line per net, `#` for comments. A point token carries `REF.PIN`, or is a bare
/// label where the net has no owning instance.
fn read_netlist(text: &str) -> EdaModel {
    let mut model = EdaModel::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, rest)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        let entry = model.nets.entry(name.to_string()).or_default();
        for point in rest.split_whitespace() {
            if let Some((rf, _pin)) = point.rsplit_once('.') {
                if !rf.is_empty() {
                    model.components.insert(rf.to_string());
                }
            }
            entry.insert(point.to_string());
        }
    }
    model
}

/// One s-expression node: an atom (bare or quoted) or a list.
enum Sexp {
    Atom(String),
    List(Vec<Sexp>),
}

impl Sexp {
    /// The list's head atom, if it has one.
    fn head(&self) -> Option<&str> {
        match self {
            Sexp::List(v) => v.first().and_then(Sexp::atom),
            Sexp::Atom(_) => None,
        }
    }

    fn atom(&self) -> Option<&str> {
        match self {
            Sexp::Atom(a) => Some(a.as_str()),
            Sexp::List(_) => None,
        }
    }

    /// Every element after the head.
    fn args(&self) -> &[Sexp] {
        match self {
            Sexp::List(v) => v.get(1..).unwrap_or(&[]),
            Sexp::Atom(_) => &[],
        }
    }

    /// The first argument of the first child whose head is `head`.
    fn child_arg(&self, head: &str) -> Option<&str> {
        self.args()
            .iter()
            .find(|c| c.head() == Some(head))
            .and_then(|c| c.args().first())
            .and_then(Sexp::atom)
    }
}

/// The KiCad shape `kicad::build_kicad_netlist` writes: a `(components ...)` list
/// of `(comp (ref X) ...)` and a `(nets ...)` list of `(net (name "X") (node
/// (ref X) (pin Y)) ...)`.
fn read_kicad(text: &str) -> Result<EdaModel, String> {
    let roots = parse_sexp(text)?;
    let mut model = EdaModel::default();
    for node in &roots {
        match node.head() {
            Some("components") => {
                for comp in node.args() {
                    if comp.head() != Some("comp") {
                        continue;
                    }
                    if let Some(refdes) = comp.child_arg("ref") {
                        model.components.insert(refdes.to_string());
                    }
                }
            }
            Some("nets") => {
                for net in node.args() {
                    if net.head() != Some("net") {
                        continue;
                    }
                    let Some(name) = net.child_arg("name") else {
                        continue;
                    };
                    let entry = model.nets.entry(name.to_string()).or_default();
                    for node in net.args() {
                        if node.head() != Some("node") {
                            continue;
                        }
                        match (node.child_arg("ref"), node.child_arg("pin")) {
                            (Some(refdes), Some(pin)) => {
                                entry.insert(format!("{}.{}", refdes, pin));
                            }
                            _ => continue,
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(model)
}

fn parse_sexp(text: &str) -> Result<Vec<Sexp>, String> {
    let mut stack: Vec<Vec<Sexp>> = vec![Vec::new()];
    let mut atom = String::new();
    let mut quoted = false;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '\\' => {
                    if let Some(escaped) = chars.next() {
                        atom.push(escaped);
                    }
                }
                '"' => quoted = false,
                _ => atom.push(c),
            }
            continue;
        }
        match c {
            '"' => {
                flush_atom(&mut stack, &mut atom);
                quoted = true;
            }
            '(' => {
                flush_atom(&mut stack, &mut atom);
                stack.push(Vec::new());
            }
            ')' => {
                flush_atom(&mut stack, &mut atom);
                let done = stack.pop().ok_or("unbalanced ')'")?;
                match stack.last_mut() {
                    Some(outer) => outer.push(Sexp::List(done)),
                    None => return Err("unbalanced ')'".to_string()),
                }
            }
            c if c.is_whitespace() => flush_atom(&mut stack, &mut atom),
            _ => atom.push(c),
        }
    }
    if quoted {
        return Err("unterminated string".to_string());
    }
    flush_atom(&mut stack, &mut atom);
    if stack.len() != 1 {
        return Err("unbalanced '('".to_string());
    }
    Ok(stack.pop().unwrap_or_default())
}

fn flush_atom(stack: &mut [Vec<Sexp>], atom: &mut String) {
    if atom.is_empty() {
        return;
    }
    let a = std::mem::take(atom);
    if let Some(top) = stack.last_mut() {
        top.push(Sexp::Atom(a));
    }
}

/// The changes that would reconcile the world with the artifact, one
/// `change{type,kind,id,delta}` per entry (projection schema §2.5). Only the
/// kinds a netlist read-back can see are emitted: `instance` and `net`.
fn diff_models(
    reference: &EdaModel,
    foreign: &EdaModel,
    table: &crate::InstTable,
) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();

    for refdes in foreign.components.difference(&reference.components) {
        out.push(json!({
            "type": "add",
            "kind": "instance",
            "id": instance_id(table, refdes),
            "delta": { "ref": refdes },
        }));
    }
    for refdes in reference.components.difference(&foreign.components) {
        out.push(json!({
            "type": "remove",
            "kind": "instance",
            "id": instance_id(table, refdes),
            "delta": { "ref": refdes },
        }));
    }

    for (name, points) in &foreign.nets {
        let Some(own) = reference.nets.get(name) else {
            out.push(json!({
                "type": "add",
                "kind": "net",
                "id": net_id(table, name),
                "delta": { "name": name, "points": points.iter().collect::<Vec<_>>() },
            }));
            continue;
        };
        let added: Vec<&String> = points.difference(own).collect();
        let removed: Vec<&String> = own.difference(points).collect();
        if !added.is_empty() || !removed.is_empty() {
            out.push(json!({
                "type": "modify",
                "kind": "net",
                "id": net_id(table, name),
                "delta": { "name": name, "add": added, "remove": removed },
            }));
        }
    }
    for (name, points) in &reference.nets {
        if !foreign.nets.contains_key(name) {
            out.push(json!({
                "type": "remove",
                "kind": "net",
                "id": net_id(table, name),
                "delta": { "name": name, "points": points.iter().collect::<Vec<_>>() },
            }));
        }
    }
    out
}

/// The identity an instance change anchors on: the NodeId the world assigned the
/// instance, or the ref itself where the world has none (an added instance).
fn instance_id(table: &crate::InstTable, refdes: &str) -> Value {
    let mut hits: Vec<&crate::InstEntry> = table
        .get_components()
        .into_iter()
        .filter(|e| e.path.rsplit('.').next() == Some(refdes))
        .collect();
    hits.sort_by(|a, b| a.path.cmp(&b.path));
    match hits.first().and_then(|e| e.node_id) {
        Some(node) => json!(node.to_string()),
        None => json!(refdes),
    }
}

/// The identity a net change anchors on: the world's NetId for that name, or the
/// name itself where the world has no such net (an added net).
fn net_id(table: &crate::InstTable, name: &str) -> Value {
    let mut hits: Vec<&crate::NetEntry> = table
        .get_nets()
        .into_iter()
        .filter(|n| n.name == name)
        .collect();
    hits.sort_by_key(|n| n.id);
    match hits.first() {
        Some(net) => json!(net.id.to_string()),
        None => json!(name),
    }
}
