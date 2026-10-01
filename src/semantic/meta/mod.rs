// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The meta engine (U377) — the value layer beneath the attribute system.
//!
//! Reception: normalize the five [`McAttrVal`] arms into [`McMetaValue`]
//! (doc/meta/metadata-engine-design.md §3.1). Read: one iteration API over
//! both bracket faces — `Set` items read nameless, `Record` rows carry their
//! id (ruling ②, read-face unification; storage keeps both arms) — plus a
//! dotted-chain lookup (§3.2). Comparison and computation operators are
//! leg2/leg3. This module adds nothing behavioral: every function is a pure
//! read over the existing shapes (ruling ⑤, no cache — normalize on the fly).

use crate::semantic::basic::mc_expr::McExpression;
use crate::semantic::basic::mc_kvs::{KVSValue, McKVS};
use crate::semantic::basic::mc_literal::McLiteral;
use crate::semantic::basic::mc_opd::McOpd;
use crate::semantic::basic::mc_uval::McUnitValue;
use crate::semantic::component::mc_attr::{McAttrVal, McAttribute, McAttributes};

/// The normalized value model (ruling ①: `McMetaValue`).
#[derive(Debug, Clone)]
pub enum McMetaValue {
    /// Unit quantity (`3.3V`, `80mΩ`); the carrier echoes the author's
    /// notation through its own `Display`.
    Unit(McUnitValue),
    /// Plain decimal number (`8`, `0.5`) with its source text.
    Num(f64, String),
    /// Quoted string, quotes stripped (`"full duplex"`).
    Text(String),
    /// Bare keyword constant (`FAST`, `SWDBG`) — the exact-compare object.
    Word(String),
    /// Dotted reference (`CAP.X7R`, `spec.volt`) — segments kept, so a
    /// single-segment parameter reference stays distinguishable from a
    /// multi-segment class-enum reference (mc_comp.rs resolves only the
    /// former).
    Ref(Vec<String>),
    /// `lo ~ hi` range; `±v` folds to `Range(neg, pos)`. An absent arm is
    /// `None`.
    Range(Option<Box<McMetaValue>>, Option<Box<McMetaValue>>),
    /// Bare bracket list (`[1Mbps@0.5m, ...]`) — the Set face.
    Set(Vec<McMetaValue>),
    /// Bracket records (`[a = 80mΩ, ...]`) — the Record face: row id, values.
    Record(Vec<(String, Vec<McMetaValue>)>),
    /// `key:value` colon entries.
    KVS(Vec<(String, McMetaValue)>),
    /// `1Mbps@0.5m` read as a pair.
    Pair(Box<McMetaValue>, Box<McMetaValue>),
    /// Expression/call kept as written — the attribute-row call site stores
    /// the literal text and does not evaluate it (canon §7; U216).
    Expr(String),
    /// `_` (Undetermined): a pending value the BOM face backfills (§7.1).
    /// Comparison/computation propagate it as pending (ruling ④, leg2+).
    Undetermined,
}

/// Normalize one value of an attribute row.
pub fn normalize(val: &McAttrVal) -> McMetaValue {
    match val {
        McAttrVal::AttrLiteral(lit) => normalize_literal(lit),
        McAttrVal::AttrVariable(opd, _) => normalize_opd(opd),
        McAttrVal::AttrExpr(expr) => normalize_expr(expr),
        McAttrVal::Attributes(rows) => McMetaValue::Record(normalize_rows(rows)),
        McAttrVal::KVS(kvs) => McMetaValue::KVS(vec![normalize_kvs(kvs)]),
    }
}

/// Normalize a whole value list.
pub fn normalize_values(values: &[McAttrVal]) -> Vec<McMetaValue> {
    values.iter().map(normalize).collect()
}

fn normalize_literal(lit: &McLiteral) -> McMetaValue {
    match lit {
        McLiteral::String(s) => McMetaValue::Text(s.value.clone()),
        McLiteral::Const(c) => McMetaValue::Word(c.0.clone()),
        McLiteral::Uval(u) => McMetaValue::Unit(u.clone()),
        McLiteral::Int(i) => McMetaValue::Num(i.value as f64, i.value.to_string()),
        McLiteral::Float(f) => McMetaValue::Num(f.value, format!("{}", f.value)),
        // A hex literal is a bit pattern, not a quantity: keep the written
        // form rather than lose it to a decimal conversion.
        McLiteral::Hex(h) => McMetaValue::Expr(h.hex_str.clone()),
    }
}

fn normalize_opd(opd: &McOpd) -> McMetaValue {
    match opd {
        McOpd::Uscore => McMetaValue::Undetermined,
        // Id / this / pins references all read as their dotted segments; the
        // receiver word (`this`/`pins`) arrives inside `expand()` like the
        // resolution faces see it.
        other => McMetaValue::Ref(other.expand()),
    }
}

fn normalize_expr(expr: &McExpression) -> McMetaValue {
    match expr {
        McExpression::UnitValue(u) => McMetaValue::Unit(u.clone()),
        McExpression::UnitValueAt(at) => McMetaValue::Pair(
            Box::new(McMetaValue::Unit(at.left.clone())),
            Box::new(McMetaValue::Unit(at.right.clone())),
        ),
        McExpression::Const(c) => McMetaValue::Word(c.0.clone()),
        McExpression::String(s) => McMetaValue::Text(s.value.clone()),
        McExpression::Int(i) => McMetaValue::Num(i.value as f64, i.value.to_string()),
        McExpression::Float(f) => McMetaValue::Num(f.value, format!("{}", f.value)),
        McExpression::Variable(opd) => normalize_opd(opd),
        McExpression::Range(l, r) => McMetaValue::Range(
            Some(Box::new(normalize_expr(l))),
            Some(Box::new(normalize_expr(r))),
        ),
        // A bit slice is a shape, not a quantity: keep it as written.
        McExpression::Slice(_, _) => McMetaValue::Expr(format!("{expr}")),
        McExpression::Set(items) => {
            McMetaValue::Set(items.iter().map(normalize_expr).collect())
        }
        // Arithmetic and calls stay unevaluated here — instantiation's
        // resolve face is the only evaluator (mc_comp.rs); the engine reads.
        McExpression::Plus(_, _)
        | McExpression::Minus(_, _)
        | McExpression::Multiply(_, _)
        | McExpression::Divide(_, _)
        | McExpression::Call { .. } => McMetaValue::Expr(format!("{expr}")),
    }
}

fn normalize_rows(rows: &[McAttribute]) -> Vec<(String, Vec<McMetaValue>)> {
    rows.iter()
        .map(|r| (r.id.to_string(), normalize_values(&r.values)))
        .collect()
}

fn normalize_kvs(kvs: &McKVS) -> (String, McMetaValue) {
    let value = match &kvs.value {
        KVSValue::Const(c) => McMetaValue::Word(c.0.clone()),
        KVSValue::Square(vals) => McMetaValue::Set(normalize_values(vals)),
        KVSValue::Nested(list) => {
            McMetaValue::KVS(list.iter().map(normalize_kvs).collect())
        }
    };
    (kvs.key.to_string(), value)
}

/// One value as the unified read API hands it out (ruling ②). `name` is
/// `Some(record-id)` only for Record rows; everything on the Set face and
/// every scalar reads nameless, so consumers iterate once instead of
/// matching arms — the wrong-arm-reads-empty trap closes by shape.
pub struct NamedValue {
    pub name: Option<String>,
    pub value: McMetaValue,
}

/// Read a value list through the unified API: Set items flatten to nameless
/// entries, Record rows carry their id, every other value is one nameless
/// entry.
pub fn read_values(values: &[McAttrVal]) -> Vec<NamedValue> {
    let mut out = Vec::new();
    for val in values {
        match val {
            McAttrVal::AttrExpr(McExpression::Set(items)) => out.extend(
                items
                    .iter()
                    .map(|e| NamedValue { name: None, value: normalize_expr(e) }),
            ),
            other => out.push(NamedValue { name: None, value: normalize(other) }),
        }
    }
    out
}

/// Read one attribute row (a key with its whole value list).
pub fn read_attr(attr: &McAttribute) -> Vec<NamedValue> {
    read_values(&attr.values)
}

/// Dotted-chain lookup (§3.2). The dotted spelling is one attribute whose id
/// carries the whole path (`spec.workingtemperature`); the table spelling
/// nests bracket records (`spec = [workingtemperature = ...]`) — both name
/// the same leaf (G2), and both resolve here. Returns every matching leaf's
/// values through the unified read API.
pub fn resolve_path(attrs: &McAttributes, path: &str) -> Vec<NamedValue> {
    if let Some(attr) = attrs.iter().find(|a| a.id.to_string() == path) {
        return read_attr(attr);
    }
    let segs: Vec<&str> = path.split('.').collect();
    if segs.len() > 1 {
        return resolve_segs(attrs.iter(), &segs);
    }
    Vec::new()
}

fn resolve_segs<'a>(
    attrs: impl Iterator<Item = &'a McAttribute>,
    segs: &[&str],
) -> Vec<NamedValue> {
    let mut out = Vec::new();
    for attr in attrs {
        if attr.id.to_string() != segs[0] {
            continue;
        }
        if segs.len() == 1 {
            out.extend(read_attr(attr));
            continue;
        }
        for val in &attr.values {
            if let McAttrVal::Attributes(rows) = val {
                out.extend(resolve_segs(rows.iter(), &segs[1..]));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::McIds;
    use crate::semantic::basic::mc_ida::{IdaSegment, McIda};
    use crate::semantic::basic::mc_ids::IdsSegment;
    use crate::semantic::basic::mc_literal::{McConst, McFloat, McInt};
    use crate::semantic::basic::mc_uval::McUnit;

    // Ruling ② — the two bracket faces read through one API.
    #[test]
    fn set_face_reads_nameless_through_one_api() {
        let vals = vec![McAttrVal::AttrExpr(McExpression::Set(vec![
            McExpression::UnitValue(uval(1.0, "1Mbps@0.5m")),
            McExpression::UnitValue(uval(10.0, "10Mbps@0.1m")),
        ]))];
        let read = read_values(&vals);
        assert_eq!(read.len(), 2);
        assert!(read.iter().all(|n| n.name.is_none()));
        for (got, want) in read.iter().zip(["1Mbps", "10Mbps"]) {
            match &got.value {
                McMetaValue::Unit(u) => assert!(format!("{u}").starts_with(want)),
                other => panic!("expected unit, got {other:?}"),
            }
        }
    }

    #[test]
    fn record_face_reads_named_rows_through_one_api() {
        let rows = vec![McAttrVal::Attributes(vec![
            attr("a", vec![word("80mΩ")]),
            attr("b", vec![word("100mΩ")]),
        ])];
        let read = read_values(&rows);
        assert_eq!(read.len(), 1, "Record face is one value, not two");
        assert_eq!(read[0].name, None, "the row-list itself is nameless");
        match &read[0].value {
            McMetaValue::Record(rows) => {
                assert_eq!(rows.len(), 2);
                assert_eq!(rows[0].0, "a");
                assert!(matches!(rows[0].1.first(), Some(McMetaValue::Word(w)) if w == "80mΩ"));
                assert_eq!(rows[1].0, "b");
            }
            other => panic!("expected record, got {other:?}"),
        }
    }

    // KVS entry: `Vgs:-10V` reads as a named pair with a unit value.
    #[test]
    fn kvs_reads_as_named_pair() {
        let kvs = McKVS {
            key: word_ids("Vgs"),
            value: KVSValue::Const(McConst("-10V".into())),
        };
        let vals = vec![McAttrVal::KVS(kvs)];
        match normalize(&vals[0]) {
            McMetaValue::KVS(entries) => {
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].0, "Vgs");
                assert!(matches!(entries[0].1, McMetaValue::Word(_)));
            }
            other => panic!("expected kvs, got {other:?}"),
        }
    }

    // Ruling ④ surface: `_` normalizes to Undetermined, ready for pending
    // propagation in the leg2 operators.
    #[test]
    fn undetermined_normalizes_from_uscore() {
        let vals = vec![McAttrVal::AttrVariable(McOpd::Uscore, None)];
        assert!(matches!(normalize(&vals[0]), McMetaValue::Undetermined));
    }

    // §3.2 — dotted spelling and table spelling name the same leaf (G2).
    #[test]
    fn dotted_and_table_spellings_resolve_to_one_leaf() {
        // Dotted spelling: one attribute whose id carries the whole path.
        let mut attrs = McAttributes::new();
        attrs.push(attr("spec.workingtemperature", vec![range_val()]));
        let hit = resolve_path(&attrs, "spec.workingtemperature");
        assert_eq!(hit.len(), 1);
        assert!(matches!(hit[0].value, McMetaValue::Range(_, _)));
        // Table spelling: nested bracket records resolve by segments.
        let mut attrs = McAttributes::new();
        attrs.push(attr(
            "spec",
            vec![McAttrVal::Attributes(vec![attr(
                "workingtemperature",
                vec![range_val()],
            )])],
        ));
        let hit = resolve_path(&attrs, "spec.workingtemperature");
        assert_eq!(hit.len(), 1);
        assert!(matches!(hit[0].value, McMetaValue::Range(_, _)));
        // A path with no leaf resolves empty, never panics.
        assert!(resolve_path(&attrs, "spec.nosuch").is_empty());
    }

    #[test]
    fn range_and_expr_read_as_modeled() {
        // -20V ~ +20V → Range(unit, unit)
        let range = McExpression::Range(
            Box::new(McExpression::UnitValue(uval(-20.0, "-20V"))),
            Box::new(McExpression::UnitValue(uval(20.0, "+20V"))),
        );
        match normalize_expr(&range) {
            McMetaValue::Range(Some(l), Some(r)) => {
                assert!(matches!(*l, McMetaValue::Unit(_)));
                assert!(matches!(*r, McMetaValue::Unit(_)));
            }
            other => panic!("expected range, got {other:?}"),
        }
        // `double(21)` stays unevaluated text (canon §7).
        let call = McExpression::Call {
            name: "double".into(),
            args: vec![McExpression::Int(McInt { value: 21 })],
        };
        match normalize_expr(&call) {
            McMetaValue::Expr(text) => assert_eq!(text, "double(21)"),
            other => panic!("expected expr text, got {other:?}"),
        }
        // Plain numbers read as Num with source text.
        let lit = McAttrVal::AttrLiteral(McLiteral::Float(McFloat { value: 0.5 }));
        match normalize(&lit) {
            McMetaValue::Num(v, text) => {
                assert!((v - 0.5).abs() < f64::EPSILON);
                assert_eq!(text, "0.5");
            }
            other => panic!("expected num, got {other:?}"),
        }
    }

    fn uval(v: f64, raw: &str) -> McUnitValue {
        McUnitValue::from_normalized(v, McUnit::Float, Some(raw.into()))
    }

    fn word(w: &str) -> McAttrVal {
        McAttrVal::AttrLiteral(McLiteral::Const(McConst(w.into())))
    }

    fn range_val() -> McAttrVal {
        McAttrVal::AttrExpr(McExpression::Range(
            Box::new(McExpression::UnitValue(uval(-40.0, "-40°C"))),
            Box::new(McExpression::UnitValue(uval(85.0, "+85°C"))),
        ))
    }

    fn word_ids(word: &str) -> McIds {
        McIds { segments: vec![IdsSegment::Ida(Box::new(ida(word)))] }
    }

    fn ida(word: &str) -> McIda {
        McIda { segments: vec![IdaSegment::Id(word.into())] }
    }

    /// One attribute with the given key path; a dotted path becomes
    /// `Ida(head)` + `DotIda(tail)` — the spelling `McIds::new` reads back
    /// from the parser's nodes.
    fn attr(id: &str, values: Vec<McAttrVal>) -> McAttribute {
        let segments = match id.split_once('.') {
            Some((head, tail)) => vec![
                IdsSegment::Ida(Box::new(ida(head))),
                IdsSegment::DotIda(Box::new(ida(tail))),
            ],
            None => vec![IdsSegment::Ida(Box::new(ida(id)))],
        };
        McAttribute {
            no: 0,
            id: McIds { segments },
            values,
            key_span: None,
            pins_ids: None,
            annotations: Vec::new(),
        }
    }
}
