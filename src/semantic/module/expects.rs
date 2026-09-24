// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The module-body `expects = [ ... ]` clause (declaration face only).
//!
//! Rows are captured as written and stored on [`crate::semantic::module::McModule`];
//! no engine consumes them yet. Each row names a target and states what is
//! expected of it: a class/role word, `driven`, a `[low:/high:]` window, or a
//! `~` range. A row in none of these forms is reported (E3082) and skipped —
//! never silently dropped.

use crate::ast::node::AstNode;
use crate::ast::macros::*;
use crate::db::diagnostic::diagnostic::dlog_error;
use crate::semantic::basic::mc_expr::McExpression;
use crate::McIds;

/// The value of one `expects` clause: the rows it declares.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Ledger {
    pub rows: Vec<Row>,
}

/// One `target = <form>` row of an `expects` clause.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub target: String,
    pub kind: Kind,
    pub span: std::ops::Range<usize>,
}

/// What a row expects of its target.
#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    /// A class/role word (`u2 = LDO.ADJ`).
    Class(String),
    /// The bare word `driven`.
    Driven,
    /// A window, written as `[low:3.2V, high:3.4V]` or `3.2V ~ 3.4V`.
    Window {
        low: Option<String>,
        high: Option<String>,
    },
}

impl Ledger {
    /// Claim one `MCAST_ATTRIBUTE` clause whose key text is exactly `expects`.
    /// Any other key (or a shape without a readable key) returns `None` without
    /// emitting anything — the caller falls back to its own unexpected-clause
    /// diagnostic. Repeated clauses append rows to the module's ledger.
    pub(crate) fn new(node: &AstNode) -> Option<Self> {
        if !node.is_type(MCAST_ATTRIBUTE) {
            return None;
        }
        // node: MCAST_ATTRIBUTE( MCAST_ATT_ID( ids ), MCAST_ATT_VALUES( ... ) )
        let att_id = node.get_sub_node()?;
        if !att_id.is_type(MCAST_ATT_ID) {
            return None;
        }
        let ids = att_id.get_sub_node()?;
        let id = McIds::new(&ids)?.to_string();
        if id != "expects" {
            return None;
        }

        let mut ledger = Ledger::default();
        let Some(values_node) = att_id.get_next() else {
            return Some(ledger);
        };
        if !values_node.is_type(MCAST_ATT_VALUES) {
            return Some(ledger);
        }
        let Some(first_value) = values_node.get_sub_node() else {
            return Some(ledger);
        };
        // Each mc_attr_value under ATT_VALUES; the row set lives in the
        // bracketed form `[ target = form ... ]` (MCAST_SET_ATTRIBUTES).
        for value in first_value.iter() {
            if value.is_type(MCAST_SET_ATTRIBUTES) {
                // SET children (linked) are the rows.
                let Some(rows) = value.get_sub_node() else {
                    continue;
                };
                for row in rows.iter() {
                    match read_row(&row) {
                        Some(read) => ledger.rows.push(read),
                        None => malformed(&row),
                    }
                }
            } else if is_empty_bracket(&value) {
                // `expects = []` — zero rows, not a malformed row.
                continue;
            } else {
                malformed(&value);
            }
        }
        Some(ledger)
    }

    /// True when no row is recorded.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// Report one malformed row (E3082) at its own span.
fn malformed(node: &AstNode) {
    dlog_error(
        crate::errcodes::EXPECTS_ROW_MALFORMED,
        node,
        &crate::errcodes::format_msg(crate::errcodes::EXPECTS_ROW_MALFORMED, &[]),
    );
}

/// An empty bracket value: `MCAST_EXPRESSION( MCAST_OPD_SQUARE_VEC )` with no
/// member inside. Read on the AST directly — the expression reader would
/// unwrap into an empty vector, but walking the node keeps the emptiness test
/// allocation-free and unambiguous.
fn is_empty_bracket(node: &AstNode) -> bool {
    if node.is_type(MCAST_EXPRESSION) {
        let Some(sub) = node.get_sub_node() else {
            return true;
        };
        return is_empty_bracket(&sub);
    }
    if node.is_type(MCAST_OPD_SQUARE_VEC) {
        return match node.get_sub_node() {
            Some(sub) => sub.is_null(),
            None => true,
        };
    }
    false
}

/// Read one `target = form` row. `None` means the row is malformed — the
/// caller reports E3082 and skips it.
fn read_row(row: &AstNode) -> Option<Row> {
    if !row.is_type(MCAST_ATTRIBUTE) {
        return None;
    }
    let att_id = row.get_sub_node()?;
    if !att_id.is_type(MCAST_ATT_ID) {
        return None;
    }
    let ids = att_id.get_sub_node()?;
    let target = McIds::new(&ids)?.to_string();

    let values_node = att_id.get_next()?;
    if !values_node.is_type(MCAST_ATT_VALUES) {
        return None;
    }
    let Some(first_value) = values_node.get_sub_node() else {
        return None;
    };
    let values: Vec<AstNode> = first_value.iter().collect();
    // A row carries exactly one value; several are a malformed row.
    let [value] = values.as_slice() else {
        return None;
    };
    let expr = McExpression::new(value)?;

    let start = row.get_pos() as usize;
    let span = start..start + row.get_len() as usize;
    let kind = match expr {
        McExpression::Variable(opd) => {
            let text = opd.to_string();
            if text == "driven" {
                Kind::Driven
            } else {
                Kind::Class(text)
            }
        }
        McExpression::Set(members) => read_sides(&members)?,
        McExpression::Range(low, high) => {
            // `vout2 = 3.2V ~ 3.4V` — both sides must be unit literals.
            if !matches!(&*low, McExpression::UnitValue(_))
                || !matches!(&*high, McExpression::UnitValue(_))
            {
                return None;
            }
            Kind::Window {
                low: Some(side_text(&low)?),
                high: Some(side_text(&high)?),
            }
        }
        _ => return None,
    };
    Some(Row { target, kind, span })
}

/// Read a `[low:..., high:...]` side set into a window. Every member must be a
/// `word : unit-literal` slice whose word is `low` or `high`; an unknown word,
/// a non-unit value, or no side at all is malformed.
fn read_sides(members: &[McExpression]) -> Option<Kind> {
    let mut low: Option<String> = None;
    let mut high: Option<String> = None;
    let mut seen = false;
    for member in members {
        let McExpression::Slice(key, value) = member else {
            return None;
        };
        let word = side_text(key)?;
        let text = match &**value {
            McExpression::UnitValue(uval) => uval.to_string(),
            _ => return None,
        };
        match word.as_str() {
            "low" => {
                low = Some(text);
                seen = true;
            }
            "high" => {
                high = Some(text);
                seen = true;
            }
            _ => return None,
        }
    }
    if !seen {
        return None;
    }
    Some(Kind::Window { low, high })
}

/// The text an expression carries directly: a name (`driven`, `low`) or one
/// unit literal (`3.2V`). Any other form has no direct text.
fn side_text(expr: &McExpression) -> Option<String> {
    match expr {
        McExpression::Variable(opd) => Some(opd.to_string()),
        McExpression::UnitValue(uval) => Some(uval.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::db::infra::init::MCC_TEST_PARSE_LOCK;

    const SRC: &str = r#"module main {
    expects = [
        u2 = LDO.ADJ
        v1v2 = driven
        vout = [low:3.2V, high:3.4V]
    ]
    expects = [
        vout2 = 3.2V ~ 3.4V
        bad = 42
    ]
}
"#;

    /// Parse `src` and return the module `main` with its expects ledger.
    fn parse(src: &str, uri: &str) -> crate::semantic::module::McModule {
        // The C parser / workspace tables are process-global and not
        // re-entrant across threads — hold the suite-wide parse lock
        // (init.rs), not a private one, or this races other tests.
        let _guard = MCC_TEST_PARSE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let root = crate::cli::datadir::data_root();
        crate::mcc_set_system_root(&root);
        crate::mcc_init();
        let uri: crate::McURI = uri.to_string();
        crate::mcc_load_from_string(&uri, src);
        crate::definition_space()
            .workspace_modules()
            .into_iter()
            .find(|(sn, _)| sn.ident.to_string() == "main")
            .expect("module 'main' not parsed")
            .1
            .as_ref()
            .clone()
    }

    #[test]
    fn reads_all_four_forms_across_repeated_clauses() {
        let module = parse(SRC, "/mcc/expects-forms-test.mc");
        let rows = &module.expects.rows;
        // The malformed row (`bad = 42`) is reported and skipped; the other
        // four rows land, and the second clause appends to the first.
        assert_eq!(rows.len(), 4, "rows: {rows:?}");

        assert_eq!(rows[0].target, "u2");
        assert_eq!(rows[0].kind, crate::semantic::module::expects::Kind::Class("LDO.ADJ".to_string()));

        assert_eq!(rows[1].target, "v1v2");
        assert_eq!(rows[1].kind, crate::semantic::module::expects::Kind::Driven);

        // Colon-form window; the unit text echoes the author's spelling.
        assert_eq!(rows[2].target, "vout");
        assert_eq!(
            rows[2].kind,
            crate::semantic::module::expects::Kind::Window {
                low: Some("3.2V".to_string()),
                high: Some("3.4V".to_string()),
            }
        );

        // Tilde-form window reads the same way.
        assert_eq!(rows[3].target, "vout2");
        assert_eq!(
            rows[3].kind,
            crate::semantic::module::expects::Kind::Window {
                low: Some("3.2V".to_string()),
                high: Some("3.4V".to_string()),
            }
        );

        assert!(!module.expects.is_empty());
    }

    #[test]
    fn skips_malformed_row_and_reports_it() {
        let module = parse(SRC, "/mcc/expects-malformed-test.mc");
        assert!(
            module.expects.rows.iter().all(|row| row.target != "bad"),
            "malformed row leaked: {:?}",
            module.expects.rows
        );
        // E3082 was emitted for the skipped row.
        assert!(
            crate::db::diagnostic::diagnostic::has_code_in_range(
                crate::errcodes::EXPECTS_ROW_MALFORMED,
                &"/mcc/expects-malformed-test.mc".to_string(),
                0,
                SRC.len() as u32,
            ),
            "E3082 not emitted"
        );
    }

    #[test]
    fn empty_bracket_is_zero_rows() {
        let module = parse(
            "module main {\n    expects = []\n}\n",
            "/mcc/expects-empty-test.mc",
        );
        assert!(module.expects.is_empty());
        assert!(
            !crate::db::diagnostic::diagnostic::has_code_in_range(
                crate::errcodes::EXPECTS_ROW_MALFORMED,
                &"/mcc/expects-empty-test.mc".to_string(),
                0,
                40,
            ),
            "empty bracket must not be malformed"
        );
    }
}
