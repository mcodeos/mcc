// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Parameter-table dump face (U364 batch A, binding-contract §2.1): one
//! component's parameter rows as stable JSON — faces × rows × value slots ×
//! cond × datasheet provenance — the wire shape the mce transcriber (batch B,
//! `models/src/dump.rs`) consumes.
//!
//! The rows the rfsoc corpus writes live inside `::DC(...)` ctor argument
//! lists, which the semantic layer captures as text-only `PwrParam`
//! projections, and the per-item `@ds` bags never reach it at all (b4421:
//! chain bags are invisible to fcall readers). The typed row structure
//! exists only in the parse tree, so this face walks the file's AST visit
//! capture (`record_ast_visit_json`) and decodes shape only — meta name,
//! axis words verbatim, kvalue slots, value@condition envelope points,
//! `@ds` page provenance. Nothing here interprets semantics; units ride
//! along unparsed (SI normalization is the transcriber's job).
//!
//! Schema stability (contract design §7-5): the field set is cross-repo —
//! a column change lands in mcc and mce in the same batch. Mirror of
//! `mce/crates/models/src/dump.rs`; batch B's fixture (`models/fixtures/`)
//! is the golden example.
//!
//! Not to be confused with `show params` (`drill_params`), which reads
//! DECLARED parameters (`McParamDeclare`) — a different face on different
//! data (contract design §2.1).

use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

// ── Wire schema (mirror of mce models/src/dump.rs) ──────────────────────────

/// One component's parameter table: faces × rows.
#[derive(Debug, Serialize)]
pub struct ParamsDump {
    pub component: String,
    pub faces: Vec<FaceDump>,
}

/// One supply/measure face carrying attribute rows.
#[derive(Debug, Serialize)]
pub struct FaceDump {
    /// Face kind word, e.g. "psnk".
    pub kind: String,
    /// Rail names in face order, e.g. ["VDD3P3", "GND"].
    pub rails: Vec<String>,
    pub rows: Vec<RowDump>,
}

/// One parameter row. `key` is the row's meta name (the call target —
/// `supply_range`, `current_draw`), not the local binding name (`vin`);
/// `axis` carries axis words verbatim ("tx(20dBm)"); `cond` is the raw
/// measurement-context prose; `ds` is the datasheet provenance. A `[..., ...]`
/// value list expands: one row per item, each with its own `@ds` bag.
#[derive(Debug, Serialize)]
pub struct RowDump {
    pub key: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub axis: BTreeMap<String, String>,
    pub value: RowValue,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cond: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ds: Option<DsRef>,
}

/// The row's value face: exactly one of the three carriers is set.
#[derive(Debug, Serialize, Default)]
pub struct RowValue {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub q: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slots: Option<Vec<ValueSlot>>,
}

/// One value slot: a kvalue dict entry (`peak: 140mA`), an envelope point
/// (`10mA@periph-clk-off`), or a bare quantity.
#[derive(Debug, Serialize)]
pub struct ValueSlot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slot: Option<String>,
    pub q: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
}

/// Datasheet provenance from a `@ds(p, trust, cond)` bag.
#[derive(Debug, Serialize)]
pub struct DsRef {
    pub p: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust: Option<String>,
}

/// Decode one component's parameter table out of its visit-capture tree.
/// Returns `None` when the tree holds no component of that name.
pub fn dump_component(component: &str, tree: &Value) -> Option<ParamsDump> {
    let nodes: Vec<&Value> = match tree {
        Value::Array(items) => items.iter().collect(),
        v @ Value::Object(_) => vec![v],
        _ => return None,
    };
    for n in &nodes {
        if kind(n) != Some("component") || component_name(n).as_deref() != Some(component) {
            continue;
        }
        let mut faces = Vec::new();
        if let Some(body) = find_first(n, "body") {
            for ap in children_of(body).iter().filter(|c| kind(c) == Some("attribute_pin")) {
                for pl in children_of(ap).iter().filter(|c| kind(c) == Some("pin_line")) {
                    if let Some(face) = face_of_line(pl) {
                        faces.push(face);
                    }
                }
            }
        }
        return Some(ParamsDump {
            component: component.to_string(),
            faces,
        });
    }
    None
}

// ── Face assembly ────────────────────────────────────────────────────────────

/// One `pin_line`: the iotype word (a direct child) is the face kind; each
/// `pin_name` under `pin_names` contributes the declare's rail vector as
/// rails and the class call's params as rows.
fn face_of_line(pin_line: &Value) -> Option<FaceDump> {
    let mut kind_word = None;
    let mut rails = Vec::new();
    let mut rows = Vec::new();
    if let Some(io) = find_first(pin_line, "iotype") {
        if let Some(t) = children_of(io).first().and_then(val_of) {
            kind_word = Some(t.to_string());
        }
    }
    for pn in descendants_of_kind(pin_line, "pin_name") {
        // The declare wraps both halves: class (the `::DC(...)` rows) and
        // instance (the `[VBAT, GND]` rail vector). Rails ride the
        // instance's square vec directly — a whole-subtree `opd` walk would
        // sweep the rows' axis words in.
        if let Some(vec) = find_first(pn, "instance").and_then(|i| find_first(i, "opd_square_vec")) {
            for opd in children_of(vec).iter().filter(|c| kind(c) == Some("opd")) {
                if let Some(text) = text_of(opd) {
                    rails.push(text);
                }
            }
        }
        if let Some(class) = find_first(pn, "class") {
            if let Some(params) = find_first(class, "params") {
                for param in children_of(params).iter().filter(|c| kind(c) == Some("param")) {
                    rows.extend(rows_of_param(param));
                }
            }
        }
    }
    let kind = kind_word?;
    Some(FaceDump { kind, rails, rows })
}

// ── Rows ─────────────────────────────────────────────────────────────────────

/// One body param (e.g. `idraw = [... calls ...] @ds(...)`): the value face
/// decides the row count. A call value makes one row (or one per list item),
/// anything else is not a parameter row.
fn rows_of_param(param: &Value) -> Vec<RowDump> {
    let attr = match find_first(param, "attribute") {
        Some(a) => a,
        None => return Vec::new(),
    };
    let values = match find_first(attr, "att_values") {
        Some(v) => v,
        None => return Vec::new(),
    };
    // The row-level `@ds` bag rides outside the call chain (as a child of
    // the param or beside the value inside `att_values`). Descent stops at
    // `opd_fcall` so per-item bags never leak into the row level.
    let row_bag = find_bag_stopping_at_call(param);
    let calls: Vec<&Value> = match find_first(values, "opd_square_vec") {
        // A list: only the direct call items are rows.
        Some(list) => children_of(list)
            .iter()
            .filter(|c| kind(c) == Some("opd_fcall"))
            .collect(),
        None => match find_first(values, "opd_fcall") {
            Some(call) => vec![call],
            None => return Vec::new(),
        },
    };
    calls
        .into_iter()
        .filter_map(|call| row_of_call(call, row_bag.as_ref()))
        .collect()
}

/// One `meta_call(axis..., value = ...) @ds(...)` item.
fn row_of_call(call: &Value, row_bag: Option<&Value>) -> Option<RowDump> {
    let key = find_first(call, "name").and_then(text_of)?;
    let mut axis = BTreeMap::new();
    let mut value = RowValue::default();
    if let Some(params) = find_first(call, "params") {
        for param in children_of(params).iter().filter(|c| kind(c) == Some("param")) {
            decode_call_param(param, &mut axis, &mut value);
        }
    }
    // The call's own bag wins over the row-level one.
    let bag = find_first(call, "set_attributes").or(row_bag);
    let (ds, cond) = match bag.and_then(ds_of_bag) {
        Some((ds, cond)) => (Some(ds), cond),
        None => (None, None),
    };
    Some(RowDump {
        key,
        axis,
        value,
        cond,
        ds,
    })
}

/// One argument of the meta call. Named: `value = ...` fills the value
/// face, every other named argument is an axis word kept verbatim.
/// Positional (no `attribute` wrapper — `supply_range(3V ~ 3.6V)`) is the
/// value face itself.
fn decode_call_param(param: &Value, axis: &mut BTreeMap<String, String>, value: &mut RowValue) {
    let attr = match find_first(param, "attribute") {
        Some(a) => a,
        None => {
            *value = decode_value(param);
            return;
        }
    };
    let name = match find_first(attr, "att_id").and_then(text_of) {
        Some(n) => n,
        None => return,
    };
    let values = match find_first(attr, "att_values") {
        Some(v) => v,
        None => return,
    };
    if name == "value" {
        *value = decode_value(values);
    } else if let Some(text) = arg_text(values) {
        axis.insert(name, text);
    }
}

/// The value face of a row: `a ~ b` range, `[...]` slots, `q@cond` point,
/// or a bare quantity.
fn decode_value(values: &Value) -> RowValue {
    if let Some(tilde) = find_first(values, "opd_tilde") {
        let halves: Vec<String> = children_of(tilde).iter().filter_map(arg_text).collect();
        return RowValue {
            q: None,
            range: Some(halves),
            slots: None,
        };
    }
    if let Some(list) = find_first(values, "opd_square_vec") {
        let slots: Vec<ValueSlot> = children_of(list).iter().filter_map(slot_of_item).collect();
        return RowValue {
            q: None,
            range: None,
            slots: Some(slots),
        };
    }
    if let Some(at) = at_node(values) {
        if let Some(slot) = slot_of_item(at) {
            return RowValue {
                q: None,
                range: None,
                slots: Some(vec![slot]),
            };
        }
    }
    RowValue {
        q: text_of(values),
        range: None,
        slots: None,
    }
}

/// One item of a value list: `slot: q` dict entry, `q@cond` envelope point,
/// or a bare quantity.
fn slot_of_item(item: &Value) -> Option<ValueSlot> {
    if kind(item) == Some("opd_colon") {
        let kids = children_of(item);
        let slot = kids.first().and_then(text_of);
        let q = kids.get(1).and_then(text_of)?;
        return Some(ValueSlot { slot, q, at: None });
    }
    if let Some(at) = at_node(item) {
        let kids = children_of(at);
        let q = kids.first().and_then(text_of)?;
        let at = kids.get(1).and_then(text_of);
        return Some(ValueSlot { slot: None, q, at });
    }
    Some(ValueSlot {
        slot: None,
        q: text_of(item)?,
        at: None,
    })
}

/// The `q@cond` node under a value or list item: the bareword-point kind
/// visits as `TYPE_118` (per the MCAST_UVALUE_AT tag) with the quantity and
/// the condition halves as children.
fn at_node(item: &Value) -> Option<&Value> {
    if kind(item) == Some("TYPE_118") {
        return Some(item);
    }
    find_first(item, "TYPE_118")
}

/// The `@ds(p=.., trust=.., cond=..)` payload of a set-attributes bag. The
/// `ds` attribute's value is itself a small attribute bag; `cond` rides the
/// same bag but the wire carries it on the row.
fn ds_of_bag(bag: &Value) -> Option<(DsRef, Option<String>)> {
    for attr in descendants_of_kind(bag, "attribute") {
        if find_first(attr, "att_id").and_then(text_of).as_deref() != Some("ds") {
            continue;
        }
        let values = match find_first(attr, "att_values") {
            Some(v) => v,
            None => continue,
        };
        let inner = match find_first(values, "set_attributes") {
            Some(i) => i,
            None => continue,
        };
        let mut ds = DsRef { p: 0, trust: None };
        let mut cond = None;
        for pair in children_of(inner).iter().filter(|c| kind(c) == Some("attribute")) {
            let key = find_first(pair, "att_id")
                .and_then(text_of)
                .unwrap_or_default();
            let val = find_first(pair, "att_values").and_then(text_of);
            match (key.as_str(), val) {
                ("p", Some(v)) => ds.p = v.parse().unwrap_or(0),
                ("trust", Some(v)) => ds.trust = Some(v),
                ("cond", Some(v)) => cond = Some(v),
                _ => {}
            }
        }
        return Some((ds, cond));
    }
    None
}

// ── Tree helpers ─────────────────────────────────────────────────────────────

fn kind(n: &Value) -> Option<&str> {
    n.get("kind").and_then(Value::as_str)
}

fn val_of(n: &Value) -> Option<&str> {
    n.get("value").and_then(Value::as_str)
}

fn children_of<'a>(n: &'a Value) -> &'a [Value] {
    match n.get("children").and_then(Value::as_array) {
        Some(a) => a,
        None => &[],
    }
}

/// The component's declared name (`component → name → ids → id`).
fn component_name(component: &Value) -> Option<String> {
    find_first(component, "name").and_then(text_of)
}

/// Depth-first first node of `want` kind.
fn find_first<'a>(n: &'a Value, want: &str) -> Option<&'a Value> {
    if kind(n) == Some(want) {
        return Some(n);
    }
    children_of(n).iter().find_map(|c| find_first(c, want))
}

/// Depth-first iterator over every node of `want` kind.
fn descendants_of_kind<'a>(n: &'a Value, want: &'a str) -> impl Iterator<Item = &'a Value> {
    let mut stack: Vec<&'a Value> = vec![n];
    std::iter::from_fn(move || {
        while let Some(node) = stack.pop() {
            stack.extend(children_of(node).iter().rev());
            if kind(node) == Some(want) {
                return Some(node);
            }
        }
        None
    })
}

/// The nearest set-attributes bag that does not sit inside a call chain.
fn find_bag_stopping_at_call(n: &Value) -> Option<Value> {
    if kind(n) == Some("set_attributes") {
        return Some(n.clone());
    }
    if kind(n) == Some("opd_fcall") {
        return None;
    }
    children_of(n)
        .iter()
        .find_map(find_bag_stopping_at_call)
}

/// The text a node spells. Leaf kinds carry it in `value`; wrapper kinds
/// (`ids`, `opd`, `expression`, `params`) resolve to their first leaf.
/// String leaves lose their quotes.
fn text_of(n: &Value) -> Option<String> {
    match kind(n) {
        Some("string") => val_of(n).map(|s| s.trim_matches('"').to_string()),
        Some("id") | Some("int") => val_of(n).map(str::to_string),
        Some(k) if k.starts_with("TYPE_") => val_of(n).map(str::to_string),
        _ => children_of(n).iter().find_map(text_of),
    }
}

/// The verbatim spelling of a value argument: a meta call rebuilds as
/// `word(arg, ...)`, a range as `a ~ b`, anything else as its leaf text.
fn arg_text(n: &Value) -> Option<String> {
    match kind(n) {
        Some("opd_fcall") => {
            let name = find_first(n, "name").and_then(text_of)?;
            let args: Vec<String> = match find_first(n, "params") {
                Some(params) => children_of(params)
                    .iter()
                    .filter(|c| kind(c) == Some("param"))
                    .filter_map(arg_text)
                    .collect(),
                None => Vec::new(),
            };
            Some(format!("{name}({})", args.join(", ")))
        }
        Some("opd_tilde") => {
            let halves: Vec<String> = children_of(n).iter().filter_map(arg_text).collect();
            Some(halves.join(" ~ "))
        }
        _ => {
            // Wrapper kinds (`att_values`, `expression`, `opd`, `param`)
            // carry no text of their own — descend; a leaf spells itself.
            if !children_of(n).is_empty() {
                return children_of(n).iter().find_map(arg_text);
            }
            text_of(n)
        }
    }
}
