// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::db::diagnostic::diagnostic::dlog_error;
use crate::{
    ast::{error::message::*, macros::*, node::AstNode},
    semantic::{
        basic::mc_expr::McExpression, basic::mc_kvs::McKVS, basic::mc_literal::McLiteral,
        basic::mc_uval::McUnitValue,
    },
    McIds, McOpd,
};
use std::vec;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct McAttributes {
    attributes: Vec<McAttribute>,
}

impl McAttributes {
    pub fn new() -> Self {
        Self {
            attributes: Vec::new(),
        }
    }

    /// Call-site-bindable names implied by these attribute keys
    /// (`contract-design.md` §2.4), one per leaf key.
    pub fn key_names(
        &self,
        declares: &crate::semantic::basic::mc_paramd::McParamDeclares,
    ) -> Vec<crate::semantic::basic::mc_param::AttrKeyName> {
        let mut out = Vec::new();
        collect_key_names(&self.attributes, &mut Vec::new(), declares, &mut out);
        out
    }

    pub fn parse(&mut self, node: &AstNode) {
        if let Some(attribute) = McAttribute::new(node) {
            report_duplicate_key(self, &attribute, node);
            report_value_outside_vocabulary(&attribute, node);
            self.push(attribute);
        }
    }

    pub fn push(&mut self, attribute: McAttribute) {
        self.attributes.push(attribute);
    }

    pub fn len(&self) -> usize {
        self.attributes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.attributes.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, McAttribute> {
        self.attributes.iter()
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, McAttribute> {
        self.attributes.iter_mut()
    }

    pub fn get(&self, index: usize) -> Option<&McAttribute> {
        self.attributes.get(index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut McAttribute> {
        self.attributes.get_mut(index)
    }

    pub fn find(&self, id: &McIds) -> Option<&McAttribute> {
        self.attributes.iter().find(|attr| &attr.id == id)
    }

    pub fn find_mut(&mut self, id: &McIds) -> Option<&mut McAttribute> {
        self.attributes.iter_mut().find(|attr| &attr.id == id)
    }
}

// Implement Deref and DerefMut to get Vec-like inherited behavior
impl std::ops::Deref for McAttributes {
    type Target = Vec<McAttribute>;

    fn deref(&self) -> &Self::Target {
        &self.attributes
    }
}

impl std::ops::DerefMut for McAttributes {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.attributes
    }
}

// Implement IntoIterator for easy iteration
impl IntoIterator for McAttributes {
    type Item = McAttribute;
    type IntoIter = vec::IntoIter<McAttribute>;

    fn into_iter(self) -> Self::IntoIter {
        self.attributes.into_iter()
    }
}

impl<'a> IntoIterator for &'a McAttributes {
    type Item = &'a McAttribute;
    type IntoIter = std::slice::Iter<'a, McAttribute>;

    fn into_iter(self) -> Self::IntoIter {
        self.attributes.iter()
    }
}

impl<'a> IntoIterator for &'a mut McAttributes {
    type Item = &'a mut McAttribute;
    type IntoIter = std::slice::IterMut<'a, McAttribute>;

    fn into_iter(self) -> Self::IntoIter {
        self.attributes.iter_mut()
    }
}

#[derive(Debug, Clone)]
pub enum McAttrVal {
    AttrLiteral(McLiteral),
    /// Variable reference (e.g. `spec = volt`). Optional span for LSP goto-def.
    AttrVariable(McOpd, Option<std::ops::Range<usize>>),
    AttrExpr(McExpression),
    Attributes(Vec<McAttribute>),
    KVS(McKVS),
}

/// The text of a value list (U42): a string literal reads as its own content,
/// unquoted; anything else reads as written. Several values join with one
/// space, and an empty list reads as `None`.
pub fn attr_values_text<'a>(values: impl IntoIterator<Item = &'a McAttrVal>) -> Option<String> {
    let parts: Vec<String> = values
        .into_iter()
        .map(|val| match val {
            McAttrVal::AttrLiteral(crate::McLiteral::String(s)) => s.value.clone(),
            other => other.to_string(),
        })
        .collect();
    (!parts.is_empty()).then(|| parts.join(" "))
}

impl std::fmt::Display for McAttrVal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McAttrVal::AttrLiteral(lit) => write!(f, "{lit}"),
            McAttrVal::AttrVariable(opd, _) => write!(f, "{opd}"),
            McAttrVal::AttrExpr(expr) => write!(f, "{expr}"),
            McAttrVal::Attributes(attrs) => {
                let inner: Vec<String> = attrs.iter().map(|a| format!("{a}")).collect();
                write!(f, "[{}]", inner.join(", "))
            }
            McAttrVal::KVS(kvs) => write!(f, "{kvs}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct McAttribute {
    pub no: i32,
    pub id: McIds,
    pub values: Vec<McAttrVal>,
    /// Source span of the key identifier (for LSP goto-definition).
    pub key_span: Option<std::ops::Range<usize>>,
    /// Set when the key is rooted in the `pins` keyword (`pins{6:9} = SWDBG`):
    /// then the curly's members are pin ids and `values` are that pin row's
    /// names — the compact, attribute-shaped spelling of one `pins = [ … ]`
    /// row. `None` for every ordinary key, including `foo{6:9}`.
    ///
    /// `pins` is a keyword, so the root carries its own AST node type
    /// (`MCAST_OPD_PINS`); `McIds` reads it back as the word it spells, which
    /// leaves the two spellings with the *same* id. The distinction is taken
    /// from the node type — structure, not the key name (§1.7) — and recorded
    /// here, where the binder can act on it.
    pub pins_ids: Option<Vec<crate::semantic::basic::mc_ids::IdsSegment>>,
}

/// Collect the bindable name of every leaf attribute key, one level at a time.
///
/// A key whose value is a table (`spec = [ Vout = vout ]`) is a namespace, not
/// a name: it contributes no entry of its own, only its rows — so the table
/// spelling and its dotted equivalent (`spec.Vout = vout`) yield the same
/// single name `Vout` (G2).
fn collect_key_names(
    attrs: &[McAttribute],
    prefix: &mut Vec<String>,
    declares: &crate::semantic::basic::mc_paramd::McParamDeclares,
    out: &mut Vec<crate::semantic::basic::mc_param::AttrKeyName>,
) {
    use crate::semantic::basic::mc_param::AttrKeyName;
    for attr in attrs {
        let mut path = prefix.clone();
        path.push(attr.id.to_string());
        let is_table = attr
            .values
            .iter()
            .any(|v| matches!(v, McAttrVal::Attributes(_)));
        if !is_table {
            let name = path
                .last()
                .and_then(|key| key.rsplit('.').next())
                .unwrap_or_default()
                .to_string();
            out.push(AttrKeyName {
                name,
                path: path.clone(),
                formal: key_formal(&attr.values, declares),
            });
        }
        for val in attr.values.iter() {
            if let McAttrVal::Attributes(rows) = val {
                collect_key_names(rows, &mut path, declares, out);
            }
        }
    }
}

/// The formal parameter a key stands for, when its value is a bare reference to
/// a declared one (`spec.Vout = vout`). Any other value leaves the key with no
/// parameter behind it, so a call-site assignment carries the value instead.
fn key_formal(
    values: &[McAttrVal],
    declares: &crate::semantic::basic::mc_paramd::McParamDeclares,
) -> Option<String> {
    let [McAttrVal::AttrVariable(opd, _)] = values else {
        return None;
    };
    let word = opd.to_string();
    declares
        .iter()
        .any(|d| d.get_primary_name().as_deref() == Some(word.as_str()))
        .then_some(word)
}

/// A subscript glued onto a key's first segment (`pins[1]`, `spec[0]`, `x[0]`)
/// selects nothing: the lexer keeps it inside one identifier, so the key is
/// never a legal spelling of anything and the row declares no attribute at all.
///
/// The criterion is the key's lexical form, not a word list — any first segment
/// carrying a subscript is reported, registered word or not. The registry only
/// decides whether the message can name the legal spelling to use instead.
fn report_fused_subscript_key(ids: &McIds, node: &AstNode) {
    use crate::semantic::basic::attr_keys;
    use crate::semantic::basic::mc_ids::IdsSegment;
    let Some(IdsSegment::Ida(ida)) = ids.segments.first() else {
        return;
    };
    if !ida.has_square() {
        return;
    }
    let uri = crate::current_uri::get();
    if crate::db::diagnostic::diagnostic::has_code_at(
        crate::errcodes::ATTR_RESERVED_KEYWORD,
        &uri,
        node.get_pos(),
    ) {
        return;
    }
    // A subscribed word the grammar reserves (N1) reads as an attempt at that
    // construct, so there the honest fix is to name its legal spelling.
    let hint = if attr_keys::is_reserved(ida.prefix()) {
        format!(
            " Keep the subscript separate: '{}{{...}}' or '{}.N'.",
            ida.prefix(),
            ida.prefix()
        )
    } else {
        String::new()
    };
    dlog_error(
        crate::errcodes::ATTR_RESERVED_KEYWORD,
        node,
        &format!(
            "Attribute key '{}' carries a subscript in its first segment, where a \
             subscript selects nothing: the row declares no attribute.{}",
            ids, hint
        ),
    );
}

/// One attribute list, one declaration per key: report a key that arrives in a
/// list that already holds it (U43).
///
/// The list *is* the declaration site — one row's trailing `@attr…`, one body —
/// and it is flat, so `@class(analog) @class(digital)` leaves two entries
/// standing and no merge ever happened. Which of the two a reader sees is the
/// reader's business (`find` takes the first; `decode_component_spec` walks
/// them all), and that is why the duplicate is worth naming once, here, instead
/// of leaving each reader to it. A key whose declarations are meant to
/// accumulate says so in the registry's arity column; the values are not
/// compared, so a repeat of the *same* value is reported too.
///
/// Keys are compared whole: `spec` and `spec.sub1` are two keys, and two
/// different sub-keys of one namespace are never duplicates of each other.
///
/// Only source-fed lists come through here: a list a later row merges into
/// (`attach_row_attrs`, first declaration wins) is built by `push`, not by
/// `parse`, so a pin declared twice on two rows is not this check's business.
fn report_duplicate_key(attrs: &McAttributes, attribute: &McAttribute, node: &AstNode) {
    use crate::semantic::basic::attr_keys::{self, AttrKeyArity};
    let repeated = attrs.iter().any(|a| a.id == attribute.id);
    if !repeated || attr_keys::arity_of(&attribute.id.to_string()) != AttrKeyArity::Single {
        return;
    }
    crate::db::diagnostic::diagnostic::dlog_warning(
        crate::errcodes::ATTR_KEY_DUPLICATE,
        node,
        &format!(
            "Attribute key '{}' is declared more than once in one attribute list. \
             A single-valued key carries one declaration; the declarations are not merged.",
            attribute.id
        ),
    );
}

/// A key whose values come from a closed set is declared with one of that set's
/// words, and a flag key is declared by being written at all: report a
/// declaration that says something else (5360).
///
/// The four readers of these keys (`pi.rs`'s identity decoders, `insttab.rs`'s
/// `protection_of`, `nets/faces.rs`'s face and `nets/mod.rs`'s axis) see a
/// *word* or nothing, so a misspelling is indistinguishable from an absent
/// declaration — `protect = serise` leaves a part unmarked, `noise = noize`
/// leaves a face unclaimed, and either way every rule that reads the axis goes
/// quiet. This is that missing report, made where the declaration is built
/// (`contract-design.md` §1.8).
///
/// Structural, not a word list: the set comes from the registry row. A key the
/// registry does not register is not judged — the ledger's vocabulary is open
/// for keys and closed for the values of the keys it does register.
///
/// One site covers both write faces of every key: a body sentence
/// (`noise = quiet`, `protect = shunt`) and a row's trailing `@attr…`
/// (`@role(main)`, a pin row's `@class(analog)`) are both built here. A nested
/// table row is the one shape that does not pass through this list; none of
/// these keys is written inside a table.
fn report_value_outside_vocabulary(attribute: &McAttribute, node: &AstNode) {
    use crate::semantic::basic::attr_keys::{self, AttrVocab};
    let key = attribute.id.to_string();
    let Some(vocab) = attr_keys::vocab_of(&key) else {
        return;
    };
    let written = attr_values_text(attribute.values.iter());
    match vocab {
        AttrVocab::Words(words) => {
            let Some(written) = written.as_deref() else {
                dlog_error(
                    crate::errcodes::ATTR_VALUE_NOT_IN_VOCABULARY,
                    node,
                    &format!(
                        "Attribute '{key}' declares no value. Its value is one of the words the \
                         registry holds for it: {}. A declaration with no value claims nothing.",
                        words.join(", ")
                    ),
                );
                return;
            };
            if !words.contains(&written) {
                dlog_error(
                    crate::errcodes::ATTR_VALUE_NOT_IN_VOCABULARY,
                    node,
                    &format!(
                        "Attribute '{key}' is written with the value '{written}', which is outside \
                         the key's word set ({}). Words are compared exactly, without case folding.",
                        words.join(", ")
                    ),
                );
            }
        }
        AttrVocab::Flag => {
            if written.is_some() {
                dlog_error(
                    crate::errcodes::ATTR_VALUE_NOT_IN_VOCABULARY,
                    node,
                    &format!(
                        "Attribute '{key}' is a flag: it is declared by being written, and takes no \
                         value."
                    ),
                );
            }
        }
        // The open word set (contract-design.md §1.8's fourth state): the
        // value is an identifier the author coins, so no spelling is judged —
        // but a bare `@barrier` names no group and would read as "outside
        // every group", a silent no-op. The identifier's presence is the one
        // thing the state judges.
        AttrVocab::Open => {
            if written.is_none() {
                dlog_error(
                    crate::errcodes::ATTR_VALUE_NOT_IN_VOCABULARY,
                    node,
                    &format!(
                        "Attribute '{key}' declares no value. Its value is a name the author coins \
                         (any identifier) — a bare '{key}' names no group and claims nothing."
                    ),
                );
            }
        }
    }
}

impl McAttribute {
    pub fn new(node: &AstNode) -> Option<Self> {
        // MCAST_ATTRIBUTE
        // |- MCAST_ATT_ID (--- MCAST_ATT_VALUES)?
        //
        // Power-intent trailing attributes add two shapes (intent-design.md
        // §5 / mca.y mc_tattr): the valued `@role(main)` form keeps the classic
        // [MCAST_ATT_ID, MCAST_ATT_VALUES] child pair, while a bare flag such as
        // `@star` / `@return` yields MCAST_ATT_ID as the *only* child. The value
        // sibling is therefore optional — a flag attribute has no values.

        //1. Check child node: exists + type
        let subnode1 = node.get_sub_node().expect(MISSING_SUBNODE);

        if !subnode1.is_type(MCAST_ATT_ID) {
            dlog_error(
                crate::errcodes::ATTR_TYPE_MISMATCH,
                &subnode1,
                &crate::errcodes::format_msg(crate::errcodes::ATTR_TYPE_MISMATCH, &[]),
            );
            return None;
        }
        //2. Check child node content: exists
        let snode1_ids_node = subnode1.get_sub_node().expect(MISSING_SUBNODE);

        let attr_id = McIds::new(&snode1_ids_node)?;
        // A pins-rooted key (`pins{6:9} = SWDBG`) is marked by its node type,
        // which is the only thing left to mark it with: `pins` is a keyword, so
        // it arrives as MCAST_OPD_PINS rather than an MCAST_IDA, and `McIds`
        // reads it back as the word it spells — leaving the id identical to an
        // ordinary `foo{6:9}`. Judging by that name is what §1.7 forbids, so the
        // test is the node type and the pin ids come from the curly member list.
        let pins_ids = snode1_ids_node
            .get_sub_node()
            .filter(|root| root.is_type(MCAST_OPD_PINS))
            .map(|_| {
                use crate::semantic::basic::mc_ids::IdsSegment;
                attr_id
                    .segments
                    .iter()
                    .find_map(|seg| match seg {
                        IdsSegment::Curly(members) => Some(members.clone()),
                        _ => None,
                    })
                    .unwrap_or_default()
            });
        // `pins[1] = A` glues the subscript into the key, so the lexer never
        // yields the keyword and the key silently becomes an ordinary attribute.
        report_fused_subscript_key(&attr_id, &snode1_ids_node);
        let key_span = Some(
            (snode1_ids_node.get_pos() as usize)
                ..((snode1_ids_node.get_pos() + snode1_ids_node.get_len()) as usize),
        );

        // The value sibling is optional: `@star` (bare flag) has no MCAST_ATT_VALUES.
        let Some(subnode2) = subnode1.get_next() else {
            return Some(Self {
                no: 0,
                id: attr_id,
                values: Vec::new(),
                key_span,
                pins_ids,
            });
        };

        // Special case: if value is MCAST_OPD_SQUARE_VEC containing colon expressions,
        // treat the attribute id as the KVS key
        // volt:[low:0V ~ 0.7V, high:0.7V ~ 5V] structure
        if subnode2.get_type() == MCAST_OPD_SQUARE_VEC {
            if let Some(kvs_values) = Self::parse_square_vec_kvs(&attr_id, &subnode2) {
                return Some(Self {
                    no: 0,
                    id: attr_id,
                    values: kvs_values,
                    key_span,
                    pins_ids,
                });
            }
        }

        Some(Self {
            no: 0,
            id: attr_id,
            values: McAttribute::new_attr_values(&subnode2)?,
            key_span,
            pins_ids,
        })
    }

    fn parse_square_vec_kvs(_attr_id: &McIds, square_vec: &AstNode) -> Option<Vec<McAttrVal>> {
        let sub = square_vec.get_sub_node()?;
        let kvs_list = Self::extract_kvs_from_iter(sub.iter());

        if kvs_list.is_empty() {
            return None;
        }
        Some(kvs_list)
    }

    pub(crate) fn extract_kvs_from_iter(iter: impl Iterator<Item = AstNode>) -> Vec<McAttrVal> {
        iter.filter_map(|child| McKVS::new(&child).map(McAttrVal::KVS))
            .collect()
    }

    pub fn new_attr_values(node: &AstNode) -> Option<Vec<McAttrVal>> {
        // - MCAST_ATT_VALUES
        //  | mc_attr_value:
        //             | mc_literal
        //             | mc_opd
        //             | mc_phrase (MCAST_EXPRESSION)
        //             | MCPT_LBRACKET mc_attr_lines MCPT_RBRACKET -> MCAST_SET_ATTRIBUTES

        //1. Type
        if !matches!(node.get_type(), MCAST_ATT_VALUES) {
            dlog_error(
                crate::errcodes::ATTR_TYPE_MISMATCH,
                node,
                &crate::errcodes::format_msg(crate::errcodes::ATTR_TYPE_MISMATCH, &[]),
            );
            return None;
        }
        //2. Child node: exists
        let Some(subnodes) = node.get_sub_node() else {
            dlog_error(
                crate::errcodes::ATTR_MISSING_SUBNODE,
                node,
                &crate::errcodes::format_msg(crate::errcodes::ATTR_MISSING_SUBNODE, &[]),
            );
            return None;
        };

        let mut values = Vec::<McAttrVal>::new();

        //3. Child node: type
        for each in subnodes.iter() {
            match each.get_type() {
                MCAST_INT | MCAST_FLOAT | MCAST_HEX | MCAST_STRING | MCAST_CONST | MCAST_UVALUE => {
                    if let Some(lit) = McLiteral::new(&each) {
                        values.push(McAttrVal::AttrLiteral(lit));
                    }
                }

                MCAST_OPD => {
                    if let Some(opd) = McOpd::new(&each) {
                        let span =
                            (each.get_pos() as usize)..((each.get_pos() + each.get_len()) as usize);
                        values.push(McAttrVal::AttrVariable(opd, Some(span)));
                    }
                }

                MCAST_RANGE_PLUSMINUS => {
                    // ±15kV → Range(-15kV, +15kV)
                    if let Some(uval) = McUnitValue::new(&each) {
                        let neg_expr = McExpression::UnitValue(uval.negated());
                        let pos_expr = McExpression::UnitValue(uval);
                        values.push(McAttrVal::AttrExpr(McExpression::Range(
                            Box::new(neg_expr),
                            Box::new(pos_expr),
                        )));
                    }
                }

                MCAST_UVALUE_AT => {
                    // `1Mbps@0.5m` scalar: the same paired-value form the list
                    // face already folds; read it as the expression it is.
                    if let Some(expr) = McExpression::new(&each) {
                        values.push(McAttrVal::AttrExpr(expr));
                    }
                }

                MCAST_EXPRESSION => {
                    // `voltage:3V3` / `volt:[low:0V ~ 0.7V]` is a keyed value.
                    if let Some(kvs) = McKVS::new(&each) {
                        values.push(McAttrVal::KVS(kvs));
                        continue;
                    }

                    let child = each.get_sub_node().expect(MISSING_SUBNODE);
                    if let Some(expr) = McExpression::new(&child) {
                        values.push(McAttrVal::AttrExpr(expr));
                    }
                }

                MCAST_SET_ATTRIBUTES => {
                    let sub = each.get_sub_node();
                    if sub.is_none() {
                        continue;
                    }
                    let mut attributes = Vec::<McAttribute>::new();

                    for astnode in sub.unwrap().iter() {
                        if let Some(attr) = McAttribute::new(&astnode) {
                            attributes.push(attr);
                        }
                    }

                    if !attributes.is_empty() {
                        values.push(McAttrVal::Attributes(attributes));
                    }
                }

                // Handle MCAST_OPD_SQUARE_VEC which can contain KVS-like entries
                // e.g., volt:[low:0V ~ 0.7V, high:0.7V ~ 5V]
                MCAST_OPD_SQUARE_VEC => {
                    let Some(sub) = each.get_sub_node() else {
                        continue;
                    };

                    let kvs_values = Self::extract_kvs_from_iter(sub.iter());
                    values.extend(kvs_values);
                }

                _ => {
                    dlog_error(
                        crate::errcodes::ATTR_TYPE_NOT_SUPPORTED,
                        &each,
                        &crate::errcodes::format_msg(
                            crate::errcodes::ATTR_TYPE_NOT_SUPPORTED,
                            &[&each.get_type() as &dyn std::fmt::Display],
                        ),
                    );
                    continue;
                }
            }
        }
        Some(values)
    }
}

impl PartialEq for McAttribute {
    fn eq(&self, other: &Self) -> bool {
        self.no == other.no && self.id == other.id
    }
}

impl std::fmt::Display for McAttribute {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.values.is_empty() {
            write!(f, "{}", self.id)
        } else {
            let vals: Vec<String> = self.values.iter().map(|v| format!("{v}")).collect();
            write!(f, "{} = {}", self.id, vals.join(", "))
        }
    }
}

impl Eq for McAttribute {}
