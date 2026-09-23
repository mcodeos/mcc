// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The three reads of a definition's attribute face (`contract-design.md` §3.1
//! D0), spelled once so every face of the product can carry the same answers.
//!
//! D0 rules that *is the key there* (presence), *what is its value* (value) and
//! *what does the key's name say* (key) are three different queries, and that
//! folding them into one `get(key) -> value` loses a state: "the key is not
//! declared" and "the key is declared, and its value is `_`" would collapse
//! into the same empty cell (D3). A file is read once by a consumer, so the
//! distinction has to be **written down** at projection time — this module is
//! that projection, and it is the only place the three reads are computed.
//!
//! # One walk, three answers
//!
//! [`leaf_reads`] walks a definition's attribute list **once**, in source order
//! (`build-design.md` §3.7 discipline 4), and answers presence and value
//! together: §10.8 rules that the two have the *same* key set (a key that is
//! there but carries no readable value reads as undetermined), so they are two
//! halves of one record rather than two walks that could drift apart. The key
//! read is a genuinely different set — a real subset — and is asked per key by
//! [`key_signal`], which answers from the registry alone and never looks at a
//! value.
//!
//! # Depth: read to the bottom, and the criterion is a shape, not a depth
//!
//! A table the dictionary opens (`spec = [ resistance = rs ]`) contributes its
//! rows as dotted keys (`spec.resistance`), which is the same fact as the dotted
//! spelling (G2) and the same key a key-table query asks under. A **record**
//! (`[ case1 = 60mΩ ]`) is one value of one key, however many rows it holds —
//! the `inst.key` ruling (§10.9: the criterion is the value's shape, not the
//! nesting depth). Which of the two a `[ … ]` is, is asked of the dictionary
//! ([`attr_keys::is_table_namespace`]), never of the number of rows it holds.
//!
//! # What a view names, and what it deliberately does not
//!
//! [`AttrView`] names the value's **shape** — the eight views of §3.2, plus one
//! honest fallback for an expression the eight do not name. It does **not**
//! answer whether a `[ … ]` is a nominal set or a bound: §1.5 rules that the
//! node type only answers "is this written `k:v`", and that the *semantic*
//! category comes from the key's registration (D5). That category is the key
//! read's answer, which is why it lives in [`key_signal`] and not here.
//!
//! Two values the eight views would each read the same way are kept apart by
//! construction: a **window** (`2.5V ~ 5.5V`) is its own view, never a
//! quantity, so a tolerance can never be compared as a nominal value (D2); and
//! an author's `_` is [`AttrView::Undetermined`] **with the text `_`**, while a
//! key that is declared but whose declaration carries no readable value is the
//! same view **with no text** — the reader can tell them apart, and neither is
//! the absence of a key, which is the key not being in [`leaf_reads`] at all.

use crate::semantic::basic::attr_keys::{self, AttrValueKind};
use crate::semantic::basic::mc_expr::McExpression;
use crate::semantic::basic::mc_literal::McLiteral;
use crate::semantic::basic::mc_opd::McOpd;
use crate::semantic::component::mc_attr::{attr_values_text, McAttrVal, McAttribute, McAttributes};

/// Which value view (`contract-design.md` §3.2) a written value is.
///
/// The eight views of the canon, in the canon's order, plus [`AttrView::Expr`]:
/// a written expression that the eight do not name (an arithmetic one) is
/// recorded as itself rather than mislabelled as one of the eight, and it is
/// never evaluated here (D6 — the reader sees the evaluated result the layers
/// above produced, and does not re-evaluate).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttrView {
    /// A quantity: a number, with or without a unit (`3.3V`, `-40°C`, `8`).
    Quantity,
    /// A text: a string literal or a keyword constant (`'TLE7368E'`, `HIGH`).
    Text,
    /// A window: `~` / `±`, a tolerance and never a nominal value (D2).
    Window,
    /// A set: a plain list of values (`[1.2V, 1.3V]`).
    Set,
    /// A bound: a list of `k:v` pairs (`[low:0V, high:30V]`).
    Bound,
    /// A record: a table-valued key that opens no namespace (`[case1 = 60mΩ]`).
    Record,
    /// A ref: a reference to a name (`resistance = rs`).
    Ref,
    /// Undetermined: `_`, or a declaration with no readable value (§10.8).
    Undetermined,
    /// Not one of the eight: an expression (D6 keeps the reader out of it).
    Expr,
}

impl AttrView {
    /// The token the product prints for this view.
    ///
    /// ASCII, one word, no colon and no space, so a text face's cell can be
    /// `token` or `token:text` and be split on the **first** colon without
    /// ambiguity — the tag set is closed, and no tag contains the separator.
    pub(crate) fn tag(self) -> &'static str {
        match self {
            AttrView::Quantity => "quantity",
            AttrView::Text => "text",
            AttrView::Window => "window",
            AttrView::Set => "set",
            AttrView::Bound => "bound",
            AttrView::Record => "record",
            AttrView::Ref => "ref",
            AttrView::Undetermined => "undetermined",
            AttrView::Expr => "expr",
        }
    }
}

/// One leaf key of a definition's attribute face: the key path as written, the
/// view of its value, and that value as written.
///
/// Presence is this record existing; the value read is `view` + `text`. A
/// consumer that wants the key set gets it from the records' keys, and a
/// consumer that wants values gets `view`/`text` — the two reads D0 keeps
/// apart, carried side by side exactly as §10.8 rules.
pub(crate) struct LeafRead {
    /// The leaf key, dotted the way the dictionary registers it
    /// (`spec.resistance`), which is also the way the definition wrote it (G2).
    pub key: String,
    pub view: AttrView,
    /// The value **as written**, never normalised: unit normalisation is the
    /// reader's job (D1), and re-rendering here would hide the author's
    /// notation from a reader that does want it.
    pub text: String,
}

/// What the key's **name** says about its value, without reading one — D0's key
/// read, and the answer the registry already holds (D5).
///
/// `None` means the dictionary registers no row for this key. That is not an
/// error and not an absence: the ledger is open for keys and closed for the
/// values of the keys it registers (§1.8), so an unregistered key is legal and
/// simply carries no registered meaning. The distinction is carried by whether
/// this returns `None` at all — a registered key with no unit signal still
/// answers (`kind`), so "not registered" and "registered, no unit" are never
/// the same empty cell.
pub(crate) struct KeySignal {
    /// `quantity` / `text` / `count` — the kind of value the key holds (D5).
    pub kind: &'static str,
    /// The unit the kind carries, where it carries one.
    pub unit: Option<String>,
}

/// Walk a definition's attribute list and answer presence and value for every
/// leaf key, in source order.
pub(crate) fn leaf_reads(attrs: &McAttributes) -> Vec<LeafRead> {
    let mut out = Vec::new();
    collect(attrs.iter(), "", &mut out);
    out
}

/// What the key's name says about its value (`key_signal`'s read of the
/// registry). `key` is the whole dotted key as written, like every other reader
/// of the dictionary.
pub(crate) fn key_signal(key: &str) -> Option<KeySignal> {
    match attr_keys::value_kind(key)? {
        AttrValueKind::Quantity(unit) => Some(KeySignal {
            kind: "quantity",
            unit: Some(unit.to_string()),
        }),
        AttrValueKind::Text => Some(KeySignal {
            kind: "text",
            unit: None,
        }),
        AttrValueKind::Count => Some(KeySignal {
            kind: "count",
            unit: None,
        }),
    }
}

/// One attribute list's contribution to the walk. `prefix` is the dotted path
/// of the table this list is a row of (`spec.`), empty at the top level.
fn collect<'a>(
    attrs: impl Iterator<Item = &'a McAttribute>,
    prefix: &str,
    out: &mut Vec<LeafRead>,
) {
    for attr in attrs {
        // A row rooted in the `pins` keyword (`pins{6:9} = SWDBG`) rides the
        // attribute node but is not in the attribute system (§1.6): it declares
        // pin rows. It is skipped because of what the row *is* — the compact
        // spelling of one `pins = [ … ]` row — and `pins_ids` is where the
        // binder recorded that, taken from the node type and not from the key
        // name (§1.7).
        if attr.pins_ids.is_some() {
            continue;
        }
        let key = format!("{prefix}{}", attr.id);
        if opens_table(&key, &attr.values) {
            let inner = format!("{key}.");
            for val in attr.values.iter() {
                if let McAttrVal::Attributes(rows) = val {
                    collect(rows.iter(), &inner, out);
                }
            }
            continue;
        }
        let (view, text) = read_values(&attr.values);
        out.push(LeafRead { key, view, text });
    }
}

/// Does this attribute open a table namespace — is its `[ … ]` a set of rows
/// that are keys, rather than one record value?
///
/// Both halves are needed and neither is a heuristic: the value has to be a
/// table at all (a dotted key's value is not), and the dictionary has to
/// register rows *under* the key, which is what makes the rows keys rather than
/// fields of a record.
fn opens_table(key: &str, values: &[McAttrVal]) -> bool {
    values.iter().any(|v| matches!(v, McAttrVal::Attributes(_)))
        && attr_keys::is_table_namespace(key)
}

/// The value read of one key's written value list.
fn read_values(values: &[McAttrVal]) -> (AttrView, String) {
    match values {
        // A declaration with no readable value. Unreachable from the grammar
        // (an attribute is written `key = value`), so this is a parse-layer
        // failure upstream: §10.8 rules that a key which is there but carries
        // no value reads as undetermined, and the empty text is what keeps it
        // apart from an author's `_`.
        [] => (AttrView::Undetermined, String::new()),
        [one] => read_one(one),
        // Several values under one key: `k = v1, v2`. All of them named pairs
        // is the bound shape (`k:v`); anything else is a plain set.
        many if many.iter().all(|v| matches!(v, McAttrVal::KVS(_))) => {
            (AttrView::Bound, written(many))
        }
        many => (AttrView::Set, written(many)),
    }
}

/// The text of a value list, read the way U42 reads it: a string literal is its
/// own content, anything else reads as written, and several values join with
/// one space.
fn written(values: &[McAttrVal]) -> String {
    attr_values_text(values.iter()).unwrap_or_default()
}

fn read_one(value: &McAttrVal) -> (AttrView, String) {
    match value {
        McAttrVal::AttrLiteral(lit) => (literal_view(lit), literal_text(lit)),
        McAttrVal::AttrVariable(opd, _) => match opd {
            McOpd::Uscore => (AttrView::Undetermined, "_".to_string()),
            _ => (AttrView::Ref, opd.to_string()),
        },
        McAttrVal::AttrExpr(expr) => expr_view(expr),
        McAttrVal::Attributes(_) => (AttrView::Record, value.to_string()),
        McAttrVal::KVS(_) => (AttrView::Bound, value.to_string()),
    }
}

fn literal_view(lit: &McLiteral) -> AttrView {
    match lit {
        McLiteral::String(_) | McLiteral::Const(_) => AttrView::Text,
        // A number is a quantity whether or not it carries a unit; `INT` /
        // `FLOAT` / `HEX` are the dimensionless rows of the unit table, and
        // nothing here re-renders them (D1 normalises on the reader's side).
        McLiteral::Uval(_) | McLiteral::Int(_) | McLiteral::Hex(_) | McLiteral::Float(_) => {
            AttrView::Quantity
        }
    }
}

fn literal_text(lit: &McLiteral) -> String {
    match lit {
        // A string literal reads as its own content, unquoted (U42).
        McLiteral::String(s) => s.value.clone(),
        other => other.to_string(),
    }
}

fn expr_view(expr: &McExpression) -> (AttrView, String) {
    match expr {
        McExpression::Range(_, _) => (AttrView::Window, expr.to_string()),
        McExpression::UnitValue(_) | McExpression::UnitValueAt(_) => {
            (AttrView::Quantity, expr.to_string())
        }
        McExpression::Int(_) | McExpression::Float(_) => (AttrView::Quantity, expr.to_string()),
        McExpression::String(_) | McExpression::Const(_) => (AttrView::Text, expr.to_string()),
        McExpression::Variable(McOpd::Uscore) => (AttrView::Undetermined, "_".to_string()),
        McExpression::Variable(_) => (AttrView::Ref, expr.to_string()),
        // A `k:v` list is the bound shape; the same list written flat is a set.
        McExpression::Set(items) if items.iter().all(named_pair) => {
            (AttrView::Bound, expr.to_string())
        }
        McExpression::Set(_) => (AttrView::Set, expr.to_string()),
        McExpression::Slice(_, _) => (AttrView::Bound, expr.to_string()),
        McExpression::Plus(_, _)
        | McExpression::Minus(_, _)
        | McExpression::Multiply(_, _)
        | McExpression::Divide(_, _)
        | McExpression::Call { .. } => (AttrView::Expr, expr.to_string()),
    }
}

/// Is this member of a `[ … ]` a named pair? The shape question §1.5 rules the
/// node type answers, asked of the node: a colon phrase is one.
fn named_pair(expr: &McExpression) -> bool {
    matches!(expr, McExpression::Slice(_, _))
}
