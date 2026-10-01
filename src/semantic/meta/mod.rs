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
use crate::semantic::basic::mc_uval::{McUnit, McUnitValue};
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

/// The Record face of one value: the bracket-record rows when the value is
/// `McAttrVal::Attributes`, empty for every other arm. The scattered consumers
/// (mc_attr.rs `collect_key_names`, insttab.rs record selection, the
/// mc_attr_view walk) iterate through here instead of matching the arm
/// themselves — the wrong-arm-reads-empty trap closes by shape (§2.4 gap 1).
pub fn record_rows(val: &McAttrVal) -> &[McAttribute] {
    const NONE: &[McAttribute] = &[];
    match val {
        McAttrVal::Attributes(rows) => rows,
        _ => NONE,
    }
}

/// Does any value on this list carry the Record face (`[ id = ... ]`)?
pub fn has_records(values: &[McAttrVal]) -> bool {
    values.iter().any(|v| matches!(v, McAttrVal::Attributes(_)))
}

/// The id of every row on one value's Record face, in written order.
pub fn record_names(val: &McAttrVal) -> Vec<String> {
    record_rows(val).iter().map(|row| row.id.to_string()).collect()
}

/// The record row whose id is exactly `name`, on one value's Record face —
/// the exact-name comparison the insttab record selection does by hand.
pub fn record_row<'a>(val: &'a McAttrVal, name: &str) -> Option<&'a McAttribute> {
    record_rows(val).iter().find(|row| row.id.to_string() == name)
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
            out.extend(resolve_segs(record_rows(val).iter(), &segs[1..]));
        }
    }
    out
}

/// The three-valued comparison result (ruling ④). `Pending` is the `_`
/// propagation: a comparison that touches an undetermined value is itself
/// undetermined — never a violation. The vocabulary gate stays silent on a
/// pending declaration (canon §7.1: a `_` value is backfilled later, on the
/// BOM face), and arithmetic through a `_` yields `_` (leg3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compare {
    True,
    False,
    Pending,
}

fn decided(b: bool) -> Compare {
    if b {
        Compare::True
    } else {
        Compare::False
    }
}

/// Word-exact comparison. Word/Text/Ref arms compare by their exact written
/// form — no case folding, the vocabulary gate's law. Numeric and unit arms
/// are not words (they compare through [`eq_norm`]); any other arm pair reads
/// False, and a pending operand propagates pending.
pub fn exact(a: &McMetaValue, b: &McMetaValue) -> Compare {
    match (a, b) {
        (McMetaValue::Undetermined, _) | (_, McMetaValue::Undetermined) => Compare::Pending,
        (McMetaValue::Word(x), McMetaValue::Word(y))
        | (McMetaValue::Text(x), McMetaValue::Text(y)) => decided(x == y),
        (McMetaValue::Ref(x), McMetaValue::Ref(y)) => decided(x == y),
        _ => Compare::False,
    }
}

/// The comparable magnitude of one value: a unit quantity reads its
/// prefix-normalized magnitude with its unit family (the mc_uval suffix table
/// folds `80mΩ` into `0.08` Ohm at parse), a plain number reads dimensionless.
/// Every other arm has no magnitude.
fn magnitude(v: &McMetaValue) -> Option<(f64, McUnit)> {
    match v {
        McMetaValue::Unit(u) => Some((u.value(), u.unit().clone())),
        McMetaValue::Num(n, _) => Some((*n, McUnit::Float)),
        _ => None,
    }
}

/// Unit-normalized numeric equality (`80mΩ` ≡ `0.08Ω`, dimensionless `8` ≡
/// `8.0`). Both operands must carry a magnitude in the same unit family;
/// anything wordish, textual or structural reads False, pending propagates.
pub fn eq_norm(a: &McMetaValue, b: &McMetaValue) -> Compare {
    match (a, b) {
        (McMetaValue::Undetermined, _) | (_, McMetaValue::Undetermined) => Compare::Pending,
        _ => match (magnitude(a), magnitude(b)) {
            (Some((x, ux)), Some((y, uy))) => decided(ux == uy && x == y),
            _ => Compare::False,
        },
    }
}

/// One element comparison behind [`member`] and [`overlap`]: numeric arms
/// pair through [`eq_norm`], everything else through [`exact`].
fn element_eq(a: &McMetaValue, b: &McMetaValue) -> Compare {
    match (a, b) {
        (McMetaValue::Unit(_), _)
        | (_, McMetaValue::Unit(_))
        | (McMetaValue::Num(..), _)
        | (_, McMetaValue::Num(..)) => eq_norm(a, b),
        _ => exact(a, b),
    }
}

/// Set membership — is `item` one of `set`'s elements (∃-Kleene)? A definite
/// match decides True even when other elements are pending; no match with a
/// pending element present stays Pending. A `Range` set reads as interval
/// membership (closed, unbounded where an arm is absent); a non-collection
/// set reads as direct comparison.
pub fn member(item: &McMetaValue, set: &McMetaValue) -> Compare {
    match set {
        McMetaValue::Undetermined => Compare::Pending,
        McMetaValue::Set(items) => {
            let mut pending = false;
            for e in items {
                match element_eq(item, e) {
                    Compare::True => return Compare::True,
                    Compare::Pending => pending = true,
                    Compare::False => {}
                }
            }
            if pending {
                Compare::Pending
            } else {
                Compare::False
            }
        }
        McMetaValue::Range(lo, hi) => {
            let Some((x, ux)) = magnitude(item) else {
                return Compare::False;
            };
            // One closed-interval bound: satisfied (True), violated (False),
            // undecidable (Pending — the endpoint is `_`), or trivially
            // satisfied because the interval is unbounded on this side.
            let bound = |end: &Option<Box<McMetaValue>>, inside: bool| match end.as_deref() {
                // Absent arm: the interval is unbounded on this side.
                None => Compare::True,
                // A `_` endpoint is undecidable, never violating (ruling ④).
                Some(McMetaValue::Undetermined) => Compare::Pending,
                Some(v) => match magnitude(v) {
                    None => Compare::False,
                    Some((v, vy)) => {
                        if vy != ux {
                            Compare::False
                        } else {
                            decided(if inside {
                                x <= v
                            } else {
                                x >= v
                            })
                        }
                    }
                },
            };
            let lo_cmp = bound(lo, false);
            let hi_cmp = bound(hi, true);
            if matches!(lo_cmp, Compare::False) || matches!(hi_cmp, Compare::False) {
                Compare::False
            } else if matches!(lo_cmp, Compare::True) && matches!(hi_cmp, Compare::True) {
                Compare::True
            } else {
                Compare::Pending
            }
        }
        other => exact(item, other),
    }
}

/// Containment — [`member`] with the arguments in set order.
pub fn contains(set: &McMetaValue, item: &McMetaValue) -> Compare {
    member(item, set)
}

/// Range intersection (the level-window family's low-level operator) and
/// set-element overlap (∃∃-Kleene: a definite common element decides True,
/// pending pairs keep the answer Pending). Non-comparable shapes read False.
pub fn overlap(a: &McMetaValue, b: &McMetaValue) -> Compare {
    match (a, b) {
        (McMetaValue::Undetermined, _) | (_, McMetaValue::Undetermined) => Compare::Pending,
        (McMetaValue::Range(l_lo, l_hi), McMetaValue::Range(r_lo, r_hi)) => {
            let end = |e: &Option<Box<McMetaValue>>| e.as_deref().map(|v| magnitude(&v));
            // Closed intervals are disjoint iff one known hi < one known lo;
            // each direction alone can decide False. Unbounded ends and
            // pending endpoints leave the answer pending unless disjointness
            // is already proven.
            let disjoint = |hi: &Option<Box<McMetaValue>>, lo: &Option<Box<McMetaValue>>| {
                match (end(hi), end(lo)) {
                    (Some(Some((h, uh))), Some(Some((l, ul)))) => Some(uh == ul && h < l),
                    _ => None,
                }
            };
            if disjoint(l_hi, r_lo) == Some(true) || disjoint(r_hi, l_lo) == Some(true) {
                return Compare::False;
            }
            let four = [l_lo, l_hi, r_lo, r_hi];
            // Every end decidable: absent (unbounded) or known. Only a `_`
            // endpoint leaves the intersection undetermined (ruling ④).
            if four.iter().all(|e| !matches!(end(e), Some(None))) {
                let inside = |hi: &Option<Box<McMetaValue>>, lo: &Option<Box<McMetaValue>>| {
                    !matches!(disjoint(hi, lo), Some(true))
                };
                decided(inside(l_hi, r_lo) && inside(r_hi, l_lo))
            } else {
                Compare::Pending
            }
        }
        (McMetaValue::Set(xs), McMetaValue::Set(ys)) => {
            let mut pending = false;
            for x in xs {
                for y in ys {
                    match element_eq(x, y) {
                        Compare::True => return Compare::True,
                        Compare::Pending => pending = true,
                        Compare::False => {}
                    }
                }
            }
            if pending {
                Compare::Pending
            } else {
                Compare::False
            }
        }
        _ => Compare::False,
    }
}

// --- leg3: the computation family (design doc §3.3) ---
//
// Arithmetic returns values, not verdicts: a pending operand yields a
// pending result (ruling ④), arms with no arithmetic reading yield `None`.
// The unit laws mirror the parse-time composite construction (U370): a
// denominator is a unit label only — no factor, no offset ("composites have
// no algebra", doc/eval/composite-unit-tempco-design.md) — exactly one
// temperature denominator folds to [`McUnit::TempCo`], anything else to a
// right-nested [`McUnit::Composite`].

/// The two halves of a `quantity @ condition` pair (`1Mbps@0.5m`):
/// magnitude first, condition second. `None` on every other arm — the
/// wrong-arm-reads-none trap closes by shape, as on the record face.
pub fn pair_halves(v: &McMetaValue) -> Option<(&McMetaValue, &McMetaValue)> {
    match v {
        McMetaValue::Pair(a, b) => Some((a, b)),
        _ => None,
    }
}

/// The magnitude half of a pair (`1Mbps` of `1Mbps@0.5m`) — the half the
/// rating and type faces quote (ratings.rs `scalar_of`).
pub fn pair_magnitude(v: &McMetaValue) -> Option<&McMetaValue> {
    pair_halves(v).map(|(a, _)| a)
}

/// The condition half of a pair (`0.5m` of `1Mbps@0.5m`) — reference data,
/// the half the condition-axis face judges (cond_axis.rs).
pub fn pair_condition(v: &McMetaValue) -> Option<&McMetaValue> {
    pair_halves(v).map(|(_, b)| b)
}

/// Quantity times a dimensionless factor (`unit algebra keeps the family`).
/// Dimensionless numbers scale too; a pending operand stays pending.
pub fn scale(v: &McMetaValue, k: f64) -> Option<McMetaValue> {
    match v {
        McMetaValue::Undetermined => Some(McMetaValue::Undetermined),
        McMetaValue::Unit(u) => {
            Some(McMetaValue::Unit(McUnitValue::from_normalized(u.value() * k, u.unit().clone(), None)))
        }
        McMetaValue::Num(n, _) => Some(McMetaValue::Num(n * k, format!("{}", n * k))),
        _ => None,
    }
}

/// Quantity divided by a quantity: the magnitudes divide, the units compose
/// per the parse law above. A dimensionless denominator leaves the numerator
/// family unchanged; a dimensionless numerator has no derived family to
/// compose into (`None` — no algebra is invented). A zero denominator is not
/// a quantity (`None`); a pending operand divides to pending.
pub fn div(a: &McMetaValue, b: &McMetaValue) -> Option<McMetaValue> {
    match (a, b) {
        (McMetaValue::Undetermined, _) | (_, McMetaValue::Undetermined) => {
            Some(McMetaValue::Undetermined)
        }
        _ => match (magnitude(a), magnitude(b)) {
            (Some((x, ux)), Some((y, uy))) if y != 0.0 => {
                let unit = match (&ux, &uy) {
                    // Dimensionless denominator: the family stands.
                    (_, McUnit::Float) => Some(ux),
                    // Dimensionless numerator: no derived family is invented.
                    (McUnit::Float, _) => None,
                    // Exactly one temperature denominator: per-degree drift.
                    (_, McUnit::Temp) => Some(McUnit::TempCo {
                        numerator: Box::new(ux),
                        denominator: Box::new(McUnit::Temp),
                    }),
                    (_, other) => Some(McUnit::Composite {
                        numerator: Box::new(ux),
                        denominator: Box::new(other.clone()),
                    }),
                };
                let q = x / y;
                unit.map(|u| match u {
                    McUnit::Float => McMetaValue::Num(q, format!("{q}")),
                    u => McMetaValue::Unit(McUnitValue::from_normalized(q, u, None)),
                })
            }
            _ => None,
        },
    }
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

    // The Record-face primitives the scattered consumers delegate to: empty
    // on every other arm (the wrong-arm-reads-empty trap closes by shape).
    #[test]
    fn record_face_primitives_read_rows_names_and_exact_row() {
        let record = McAttrVal::Attributes(vec![
            attr("a", vec![word("80mΩ")]),
            attr("b", vec![word("100mΩ")]),
        ]);
        assert!(has_records(std::slice::from_ref(&record)));
        assert_eq!(record_names(&record), vec!["a".to_string(), "b".to_string()]);
        assert_eq!(record_row(&record, "b").map(|r| r.id.to_string()), Some("b".into()));
        assert_eq!(record_row(&record, "c"), None);
        // The Set face and scalars read as zero rows, never as a panic.
        let set = McAttrVal::AttrExpr(McExpression::Set(vec![]));
        assert!(!has_records(std::slice::from_ref(&set)));
        assert!(record_rows(&set).is_empty());
        assert!(record_names(&set).is_empty());
        assert_eq!(record_row(&set, "a"), None);
    }

    // Leg2 — the comparison family. Ruling ④: `_` propagates as pending,
    // never a violation.
    #[test]
    fn exact_compares_wordish_arms_and_propagates_pending() {
        let word = |w: &str| McMetaValue::Word(w.into());
        assert_eq!(exact(&word("main"), &word("main")), Compare::True);
        assert_eq!(exact(&word("Main"), &word("main")), Compare::False);
        assert_eq!(exact(&word("x"), &McMetaValue::Text("x".into())), Compare::False);
        assert_eq!(
            exact(&McMetaValue::Undetermined, &word("main")),
            Compare::Pending
        );
        assert_eq!(exact(&word("main"), &McMetaValue::Undetermined), Compare::Pending);
    }

    #[test]
    fn eq_norm_reads_prefix_normalized_quantities() {
        let uval = |v: f64, unit: McUnit| McMetaValue::Unit(McUnitValue::from_normalized(v, unit, None));
        let ohm = || McUnit::Ohm;
        // `80mΩ` parses to (0.08, Ohm); it equals `0.08Ω`, not `80Ω`.
        assert_eq!(eq_norm(&uval(0.08, ohm()), &uval(0.08, ohm())), Compare::True);
        assert_eq!(eq_norm(&uval(80.0, ohm()), &uval(0.08, ohm())), Compare::False);
        // A dimensionless number and its unit-family quantity compare equal.
        assert_eq!(eq_norm(&uval(8.0, McUnit::Float), &McMetaValue::Num(8.0, "8".into())), Compare::True);
        // Cross-family and wordish operands do not decode.
        assert_eq!(eq_norm(&uval(1.0, McUnit::Ohm), &uval(1.0, McUnit::Volt)), Compare::False);
        assert_eq!(
            eq_norm(&McMetaValue::Word("80mΩ".into()), &uval(0.08, ohm())),
            Compare::False
        );
        // Pending propagates.
        assert_eq!(eq_norm(&McMetaValue::Undetermined, &uval(1.0, ohm())), Compare::Pending);
    }

    #[test]
    fn member_and_contains_read_sets_ranges_and_pending() {
        let word = |w: &str| McMetaValue::Word(w.into());
        let set = |ws: &[&str]| McMetaValue::Set(ws.iter().map(|w| word(w)).collect());
        assert_eq!(member(&word("shunt"), &set(&["shunt", "series"])), Compare::True);
        assert_eq!(member(&word("serise"), &set(&["shunt", "series"])), Compare::False);
        assert_eq!(contains(&set(&["shunt", "series"]), &word("series")), Compare::True);
        // ∃-Kleene: a definite match decides; otherwise pending infects.
        let mixed = McMetaValue::Set(vec![word("a"), McMetaValue::Undetermined]);
        assert_eq!(member(&word("a"), &mixed), Compare::True);
        assert_eq!(member(&word("b"), &mixed), Compare::Pending);
        // Interval membership: closed, unbounded where an arm is absent.
        let range = |lo: Option<f64>, hi: Option<f64>| {
            let end = |v: Option<f64>| {
                v.map(|x| Box::new(McMetaValue::Unit(McUnitValue::from_normalized(x, McUnit::Volt, None))))
            };
            McMetaValue::Range(end(lo), end(hi))
        };
        let three = McMetaValue::Unit(McUnitValue::from_normalized(3.6, McUnit::Volt, None));
        assert_eq!(member(&three, &range(Some(3.3), Some(5.5))), Compare::True);
        assert_eq!(member(&three, &range(Some(4.0), None)), Compare::False);
        assert_eq!(member(&three, &range(None, Some(5.5))), Compare::True);
        // A pending endpoint leaves membership pending, never false.
        let pending_end = McMetaValue::Range(
            Some(Box::new(McMetaValue::Undetermined)),
            Some(Box::new(McMetaValue::Unit(McUnitValue::from_normalized(5.5, McUnit::Volt, None)))),
        );
        assert_eq!(member(&three, &pending_end), Compare::Pending);
    }

    #[test]
    fn overlap_reads_range_intersection_and_set_commons() {
        let vr = |lo: f64, hi: f64| {
            let end = |v: f64| {
                Box::new(McMetaValue::Unit(McUnitValue::from_normalized(v, McUnit::Volt, None)))
            };
            McMetaValue::Range(Some(end(lo)), Some(end(hi)))
        };
        assert_eq!(overlap(&vr(3.0, 5.5), &vr(4.5, 6.0)), Compare::True);
        assert_eq!(overlap(&vr(3.0, 3.4), &vr(4.5, 6.0)), Compare::False);
        // Touching closed intervals share an endpoint.
        assert_eq!(overlap(&vr(3.0, 4.5), &vr(4.5, 6.0)), Compare::True);
        // An unbounded end decides; a `_` endpoint stays pending.
        let unbounded = McMetaValue::Range(None, Some(Box::new(McMetaValue::Unit(McUnitValue::from_normalized(5.5, McUnit::Volt, None)))));
        assert_eq!(overlap(&unbounded, &vr(1.0, 2.0)), Compare::True);
        let pending_hi = McMetaValue::Range(
            Some(Box::new(McMetaValue::Unit(McUnitValue::from_normalized(1.0, McUnit::Volt, None)))),
            Some(Box::new(McMetaValue::Undetermined)),
        );
        assert_eq!(overlap(&pending_hi, &vr(4.0, 6.0)), Compare::Pending);
        // Set overlap: a common element decides, pending pairs keep pending.
        let word = |w: &str| McMetaValue::Word(w.into());
        let set = |ws: &[&str]| McMetaValue::Set(ws.iter().map(|w| word(w)).collect());
        assert_eq!(overlap(&set(&["a", "b"]), &set(&["b", "c"])), Compare::True);
        assert_eq!(overlap(&set(&["a"]), &set(&["c"])), Compare::False);
    }

    // The pair-read primitives (leg3): halves split, wrong arms read None.
    #[test]
    fn pair_read_splits_halves_and_rejects_other_arms() {
        let at = McExpression::UnitValueAt(crate::semantic::basic::mc_expr::McUnitValueAt {
            left: uval(1.0, "1Mbps"),
            right: uval(0.5, "0.5m"),
        });
        let pair = normalize(&McAttrVal::AttrExpr(at));
        assert_eq!(pair_magnitude(&pair).map(|v| magnitude(&v)), Some(Some((1.0, McUnit::Float))));
        assert_eq!(pair_condition(&pair).map(|v| magnitude(&v)), Some(Some((0.5, McUnit::Float))));
        // Wrong arms read None, never a panic (the record-face law again).
        assert!(pair_halves(&McMetaValue::Undetermined).is_none());
        assert!(pair_halves(&McMetaValue::Num(8.0, "8".into())).is_none());
        assert!(pair_magnitude(&McMetaValue::Undetermined).is_none());
        assert!(pair_condition(&McMetaValue::Undetermined).is_none());
    }

    // The computation family (leg3): pending in, pending out; unit laws
    // mirror the parse-time composite construction (U370).
    #[test]
    fn computation_operators_scale_divide_and_propagate_pending() {
        let volt = |v: f64| McMetaValue::Unit(McUnitValue::from_normalized(v, McUnit::Volt, None));
        let num = |n: f64| McMetaValue::Num(n, format!("{n}"));
        // Scale keeps the family; dimensionless scales too.
        assert_eq!(scale(&volt(3.3), 2.0).map(|v| magnitude(&v)), Some(Some((6.6, McUnit::Volt))));
        assert_eq!(scale(&num(8.0), 0.5).map(|v| magnitude(&v)), Some(Some((4.0, McUnit::Float))));
        // Pending scales to pending; wordish arms have no arithmetic.
        assert!(matches!(scale(&McMetaValue::Undetermined, 2.0), Some(McMetaValue::Undetermined)));
        assert!(scale(&McMetaValue::Text("x".into()), 2.0).is_none());
        // Division composes: per-degree drift folds to TempCo, a general
        // denominator nests a Composite, a dimensionless denominator keeps
        // the family, two dimensionless numbers divide plainly.
        let ppm = McMetaValue::Unit(McUnitValue::from_normalized(100.0, McUnit::Ppm, None));
        let deg = McMetaValue::Unit(McUnitValue::from_normalized(1.0, McUnit::Temp, None));
        match div(&ppm, &deg) {
            Some(McMetaValue::Unit(u)) => assert_eq!(
                *u.unit(),
                McUnit::TempCo {
                    numerator: Box::new(McUnit::Ppm),
                    denominator: Box::new(McUnit::Temp)
                }
            ),
            other => panic!("expected TempCo, got {other:?}"),
        }
        let amp = McMetaValue::Unit(McUnitValue::from_normalized(1.0, McUnit::Amp, None));
        match div(&volt(3.3), &amp) {
            Some(McMetaValue::Unit(u)) => assert_eq!(
                *u.unit(),
                McUnit::Composite {
                    numerator: Box::new(McUnit::Volt),
                    denominator: Box::new(McUnit::Amp)
                }
            ),
            other => panic!("expected Composite, got {other:?}"),
        }
        let floaty = McMetaValue::Unit(McUnitValue::from_normalized(2.0, McUnit::Float, None));
        assert_eq!(div(&volt(3.3), &floaty).map(|v| magnitude(&v)), Some(Some((1.65, McUnit::Volt))));
        assert_eq!(div(&num(8.0), &num(4.0)).map(|v| magnitude(&v)), Some(Some((2.0, McUnit::Float))));
        // No algebra is invented: dimensionless numerator over a family.
        assert!(div(&num(8.0), &amp).is_none());
        // A zero denominator is not a quantity.
        assert!(div(&volt(3.3), &volt(0.0)).is_none());
        // Pending divides to pending on either side.
        assert!(matches!(div(&McMetaValue::Undetermined, &amp), Some(McMetaValue::Undetermined)));
        assert!(matches!(div(&volt(1.0), &McMetaValue::Undetermined), Some(McMetaValue::Undetermined)));
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
        }
    }
}
