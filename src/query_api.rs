// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! M5 query DSL — canonical parser + evaluator.
//!
//! Lives at lib root (alongside `search_api`) so the binary's `cmds/query.rs`
//! AND the library's `rpc/handlers.rs` can both call into the same code.
//!
//! **v1 grammar** (see [mcc-cli-roadmap](https://example/mcc-cli-roadmap.md) §7):
//!
//! ```text
//! expr        := or
//! or          := and ( OR and )*
//! and         := not ( (AND | ",")? not )*
//! not         := NOT not | atom
//! atom        := "(" expr ")" | predicate
//! predicate   := field op value | "attr" "(" name ")"
//! op          := "=" | "!=" | "~=" | ">" | "<" | ">=" | "<="
//! field       := "name" | "kind" | "class" | "attr" "(" name ")"
//! value       := number | string | bareword
//! ```
//!
//! - `=` is case-insensitive exact (or anchored glob if RHS contains `*`/`?`).
//! - `!=` is the complement.
//! - `~=` is unanchored case-insensitive Rust regex.
//! - `>` `<` `>=` `<=` are numeric comparisons (SI suffixes supported).
//! - `AND`/`OR`/`NOT`/`attr` are case-insensitive reserved keywords.
//! - `attr(name)` accesses Component/Interface attributes; on Module/Enum
//!   returns `false` (not an error).
//! - Top-level defs have no `class` field, so `class=...` is always false.
//! - NO short-circuit evaluation in v1.

use anyhow::{anyhow, Result};
use mcc::McIds;
use regex::Regex;
use serde_json::Value;

// ============================================================================
// AST
// ============================================================================

pub type Query = Expr;

#[derive(Debug, Clone)]
pub enum Expr {
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    Predicate(Predicate),
}

#[derive(Debug, Clone)]
pub enum Predicate {
    Comparison(Comparison),
    AttrExists(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    Name,
    Kind,
    Class,
    Attr(String),
}

impl Field {
    /// Short name used for `validate_allowed_fields`.
    pub fn key(&self) -> &str {
        match self {
            Field::Name => "name",
            Field::Kind => "kind",
            Field::Class => "class",
            Field::Attr(_) => "attr",
        }
    }
    /// For `Field::Attr`, the attribute name.
    pub fn attr_name(&self) -> Option<&str> {
        match self {
            Field::Attr(n) => Some(n.as_str()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonOp {
    Eq,
    Ne,
    Regex,
    Gt,
    Lt,
    Ge,
    Le,
}

#[derive(Debug, Clone)]
pub enum QueryValue {
    /// Numeric literal, with the SI suffix multiplier pre-applied.
    Number {
        raw: String,
        value: f64,
    },
    String(String),
    Bareword(String),
}

#[derive(Debug, Clone)]
pub struct Comparison {
    pub field: Field,
    pub op: ComparisonOp,
    pub value: QueryValue,
    /// Pre-compiled matcher for `~=` and glob forms of `=`/`!=`. `None` for
    /// exact-equality and numeric comparisons.
    matcher: Option<Regex>,
}

impl Comparison {
    pub fn matcher(&self) -> Option<&Regex> {
        self.matcher.as_ref()
    }
}

// ============================================================================
// compile()
// ============================================================================

/// Parse a query expression into an AST. Errors are prefixed with `query:`
/// and may include a byte offset.
pub fn compile(expr: &str) -> Result<Query> {
    let mut p = Parser::new(expr);
    let q = p.parse_expr()?;
    p.skip_ws();
    if p.pos < p.src.len() {
        return Err(p.err("unexpected trailing input"));
    }
    Ok(q)
}

/// Walk the AST and reject any field/AttrExists name not in the allowed set.
/// `Field::Attr(_)` is treated as the logical key `"attr"`.
pub fn validate_allowed_fields(query: &Query, allowed_keys: &[&str]) -> Result<()> {
    fn visit(expr: &Expr, allowed_keys: &[&str], found: &mut Vec<String>) -> Result<()> {
        match expr {
            Expr::And(a, b) | Expr::Or(a, b) => {
                visit(a, allowed_keys, found)?;
                visit(b, allowed_keys, found)
            }
            Expr::Not(e) => visit(e, allowed_keys, found),
            Expr::Predicate(Predicate::Comparison(c)) => {
                found.push(c.field.key().to_string());
                Ok(())
            }
            Expr::Predicate(Predicate::AttrExists(_)) => {
                found.push("attr".to_string());
                Ok(())
            }
        }
    }
    let mut found = Vec::new();
    visit(query, allowed_keys, &mut found)?;
    let mut unknowns: Vec<String> = found
        .into_iter()
        .filter(|k| !allowed_keys.iter().any(|a| a.eq_ignore_ascii_case(k)))
        .collect();
    unknowns.sort();
    unknowns.dedup();
    if !unknowns.is_empty() {
        return Err(anyhow!(
            "filter: unknown key {:?}, expected one of {:?}",
            unknowns,
            allowed_keys
        ));
    }
    Ok(())
}

// ============================================================================
// Evaluator
// ============================================================================

/// Cheap path: evaluates against name/kind/class/uri without `get_def`.
/// Use when the AST contains no `Attr(_)` or `AttrExists`.
pub fn matches_definition(
    query: &Query,
    kind: Option<&str>,
    name: Option<&str>,
    class: Option<&str>,
    uri: Option<&str>,
) -> bool {
    eval(query, kind, name, class, uri, &[], false)
}

/// Full path: for `Attr(...)` and `AttrExists`, fetch attrs via the caller-
/// supplied closure (which encapsulates the workspace lookup). `attrs` is
/// the list of `(name, value-string)` pairs already resolved.
pub fn matches_definition_with_attrs(
    query: &Query,
    kind: Option<&str>,
    name: Option<&str>,
    class: Option<&str>,
    uri: Option<&str>,
    attrs: &[(String, String)],
) -> bool {
    eval(query, kind, name, class, uri, attrs, true)
}

fn eval(
    query: &Query,
    kind: Option<&str>,
    name: Option<&str>,
    class: Option<&str>,
    uri: Option<&str>,
    attrs: &[(String, String)],
    attrs_resolved: bool,
) -> bool {
    match query {
        Expr::And(a, b) => {
            let la = eval(a, kind, name, class, uri, attrs, attrs_resolved);
            let lb = eval(b, kind, name, class, uri, attrs, attrs_resolved);
            la && lb
        }
        Expr::Or(a, b) => {
            let la = eval(a, kind, name, class, uri, attrs, attrs_resolved);
            let lb = eval(b, kind, name, class, uri, attrs, attrs_resolved);
            la || lb
        }
        Expr::Not(e) => !eval(e, kind, name, class, uri, attrs, attrs_resolved),
        Expr::Predicate(p) => eval_pred(p, kind, name, class, uri, attrs, attrs_resolved),
    }
}

fn eval_pred(
    p: &Predicate,
    kind: Option<&str>,
    name: Option<&str>,
    class: Option<&str>,
    uri: Option<&str>,
    attrs: &[(String, String)],
    _attrs_resolved: bool,
) -> bool {
    match p {
        Predicate::Comparison(c) => eval_comparison(c, kind, name, class, uri, attrs),
        Predicate::AttrExists(name) => {
            // Module/Enum: no attrs possible.
            match kind {
                Some("module") | Some("enum") => false,
                _ => attrs.iter().any(|(n, _)| n == name),
            }
        }
    }
}

fn field_value<'a>(
    field: &Field,
    kind: Option<&'a str>,
    name: Option<&'a str>,
    class: Option<&'a str>,
    _uri: Option<&'a str>,
    attrs: &'a [(String, String)],
) -> Option<&'a str> {
    match field {
        Field::Name => name,
        Field::Kind => kind,
        Field::Class => class,
        Field::Attr(n) => attrs.iter().find(|(k, _)| k == n).map(|(_, v)| v.as_str()),
    }
}

fn eval_comparison(
    c: &Comparison,
    kind: Option<&str>,
    name: Option<&str>,
    class: Option<&str>,
    uri: Option<&str>,
    attrs: &[(String, String)],
) -> bool {
    // Module/Enum + attr(...) → always false (no attrs possible).
    if matches!(c.field, Field::Attr(_)) {
        if matches!(kind, Some("module") | Some("enum")) {
            return false;
        }
    }
    // class is always None on top-level defs — `class=...` always false.
    if matches!(c.field, Field::Class) {
        return false;
    }

    let lhs = field_value(&c.field, kind, name, class, uri, attrs);

    match c.op {
        ComparisonOp::Eq => match &c.value {
            QueryValue::Number { value, .. } => match lhs.and_then(|s| s.parse::<f64>().ok()) {
                Some(n) => approx_eq(n, *value),
                None => false,
            },
            QueryValue::String(s) => match c.matcher.as_ref() {
                Some(re) => lhs.map(|l| re.is_match(l)).unwrap_or(false),
                None => lhs.map(|l| l.eq_ignore_ascii_case(s)).unwrap_or(false),
            },
            QueryValue::Bareword(s) => match c.matcher.as_ref() {
                Some(re) => lhs.map(|l| re.is_match(l)).unwrap_or(false),
                None => lhs.map(|l| l.eq_ignore_ascii_case(s)).unwrap_or(false),
            },
        },
        ComparisonOp::Ne => match &c.value {
            QueryValue::Number { value, .. } => match lhs.and_then(|s| s.parse::<f64>().ok()) {
                Some(n) => !approx_eq(n, *value),
                None => false, // missing field → != is false (per spec)
            },
            QueryValue::String(s) => match c.matcher.as_ref() {
                Some(re) => lhs.map(|l| !re.is_match(l)).unwrap_or(false),
                None => lhs.map(|l| !l.eq_ignore_ascii_case(s)).unwrap_or(false),
            },
            QueryValue::Bareword(s) => match c.matcher.as_ref() {
                Some(re) => lhs.map(|l| !re.is_match(l)).unwrap_or(false),
                None => lhs.map(|l| !l.eq_ignore_ascii_case(s)).unwrap_or(false),
            },
        },
        ComparisonOp::Regex => {
            let re = match c.matcher.as_ref() {
                Some(r) => r,
                None => return false,
            };
            lhs.map(|l| re.is_match(l)).unwrap_or(false)
        }
        ComparisonOp::Gt | ComparisonOp::Lt | ComparisonOp::Ge | ComparisonOp::Le => {
            let needle = match &c.value {
                QueryValue::Number { value, .. } => *value,
                _ => return false, // ordered op requires numeric RHS
            };
            let haystack = match lhs.and_then(|s| s.parse::<f64>().ok()) {
                Some(n) => n,
                None => return false,
            };
            match c.op {
                ComparisonOp::Gt => haystack > needle,
                ComparisonOp::Lt => haystack < needle,
                ComparisonOp::Ge => haystack >= needle || approx_eq(haystack, needle),
                ComparisonOp::Le => haystack <= needle || approx_eq(haystack, needle),
                _ => unreachable!(),
            }
        }
    }
}

fn approx_eq(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

/// For JSON-record filtering (extract/show). Reads name/kind/class/uri from
/// the object and resolves attrs via `mcc::get_def` only when needed.
///
/// `attr_resolver` is called with `(name, uri)` and returns the attribute
/// value strings. Pass a closure that captures the loaded workspace.
pub fn matches_json_record_with<F>(query: &Query, item: &Value, attr_resolver: F) -> bool
where
    F: Fn(&str, &str) -> Vec<(String, String)>,
{
    let kind = item.get("kind").and_then(|v| v.as_str());
    let name = item.get("name").and_then(|v| v.as_str());
    let class = item.get("class").and_then(|v| v.as_str());
    let uri = item.get("uri").and_then(|v| v.as_str());
    let attrs = if needs_attrs(query) {
        match (name, uri) {
            (Some(n), Some(u)) => attr_resolver(n, u),
            _ => Vec::new(),
        }
    } else {
        Vec::new()
    };
    eval(query, kind, name, class, uri, &attrs, true)
}

/// Backward-compat convenience: when no resolver is provided, `attr(...)`
/// and `AttrExists` always evaluate to `false`.
pub fn matches_json_record(query: &Query, item: &Value) -> bool {
    matches_json_record_with(query, item, |_, _| Vec::new())
}

/// True when the AST contains any `Attr(_)` field or `AttrExists` predicate.
pub fn needs_attrs(query: &Query) -> bool {
    match query {
        Expr::And(a, b) | Expr::Or(a, b) => needs_attrs(a) || needs_attrs(b),
        Expr::Not(e) => needs_attrs(e),
        Expr::Predicate(Predicate::Comparison(c)) => matches!(c.field, Field::Attr(_)),
        Expr::Predicate(Predicate::AttrExists(_)) => true,
    }
}

/// Helper used by `cmds/query.rs` and RPC handlers: fetch a Component/Interface
/// definition and return its attribute (name, value-string) pairs. Modules
/// and Enums always return an empty vec. Missing defs return empty.
///
/// `get_def` is the workspace lookup function (`mcc::get_def`).
pub fn attrs_for_def<F>(name: &str, uri: &str, get_def: F) -> Vec<(String, String)>
where
    F: Fn(&str, &str) -> Option<mcc::McCMIE>,
{
    let Some(cmie) = get_def(name, uri) else {
        return Vec::new();
    };
    match cmie {
        mcc::McCMIE::Component(c) => collect_attrs(&c.attrs),
        mcc::McCMIE::Interface(i) => collect_attrs(&i.attrs),
        mcc::McCMIE::Module(_) | mcc::McCMIE::Enum(_) => Vec::new(),
    }
}

fn collect_attrs(attrs: &mcc::core::component::mc_attr::McAttributes) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for a in attrs.iter() {
        let id = a.id.to_string();
        for v in &a.values {
            if let Some(s) = attrval_to_string(v) {
                out.push((id.clone(), s));
            }
        }
    }
    out
}

fn attrval_to_string(v: &mcc::McAttrVal) -> Option<String> {
    use mcc::McAttrVal::*;
    match v {
        AttrLiteral(mcc::McLiteral::String(s)) => Some(s.value.clone()),
        AttrLiteral(mcc::McLiteral::Int(i)) => Some(i.to_string()),
        AttrLiteral(mcc::McLiteral::Float(f)) => Some(f.to_string()),
        AttrLiteral(mcc::McLiteral::Const(c)) => Some(c.to_string()),
        AttrLiteral(mcc::McLiteral::Uval(u)) => Some(format!("{}", u.value())),
        _ => None,
    }
}

/// Internal helper for callers that want `McIds`-based attribute lookup
/// (used when integrating with the `get_def` FFI surface directly).
#[allow(dead_code)]
pub fn attrs_for_def_via_mcids<F>(name: &str, uri: &str, get_def: F) -> Vec<(String, String)>
where
    F: Fn(&McIds, &mcc::McURI) -> Option<mcc::McCMIE>,
{
    let ident = McIds::from(name);
    let u = mcc::McURI::from(uri);
    attrs_for_def(name, uri, |n, u| {
        get_def(&McIds::from(n), &mcc::McURI::from(u))
    })
    .into_iter()
    .filter(|_| get_def(&ident, &u).is_some())
    .collect()
}

// ============================================================================
// Parser
// ============================================================================

const RESERVED_BAREWORDS: &[&str] = &["AND", "OR", "NOT", "ATTR"];

struct Parser<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Self { src, pos: 0 }
    }

    fn err(&self, msg: impl Into<String>) -> anyhow::Error {
        anyhow!("query: {} at offset {}", msg.into(), self.pos)
    }

    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.src[self.pos + offset..].chars().next()
    }

    fn starts_with_ci(&self, kw: &str) -> bool {
        self.src[self.pos..].to_ascii_lowercase().starts_with(kw)
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.pos += c.len_utf8();
            } else {
                break;
            }
        }
    }

    fn consume_kw(&mut self, kw: &str) -> bool {
        self.skip_ws();
        let rest = &self.src[self.pos..];
        // Case-insensitive prefix match
        if rest.len() < kw.len() {
            return false;
        }
        let prefix = &rest[..kw.len()];
        if !prefix.eq_ignore_ascii_case(kw) {
            return false;
        }
        // Word boundary: end-of-input, whitespace, or non-identifier char.
        match rest[kw.len()..].chars().next() {
            None => {
                self.pos += kw.len();
                true
            }
            Some(c) if !c.is_alphanumeric() && c != '_' => {
                self.pos += kw.len();
                true
            }
            _ => false,
        }
    }

    fn consume_char(&mut self, c: char) -> bool {
        self.skip_ws();
        if self.peek() == Some(c) {
            self.pos += c.len_utf8();
            true
        } else {
            false
        }
    }

    fn consume_str(&mut self, s: &str) -> bool {
        self.skip_ws();
        if self.src[self.pos..].starts_with(s) {
            self.pos += s.len();
            true
        } else {
            false
        }
    }

    fn parse_expr(&mut self) -> Result<Expr> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expr> {
        let mut lhs = self.parse_and()?;
        loop {
            self.skip_ws();
            if self.consume_kw("OR") {
                let rhs = self.parse_and()?;
                lhs = Expr::Or(Box::new(lhs), Box::new(rhs));
            } else {
                break;
            }
        }
        Ok(lhs)
    }

    fn parse_and(&mut self) -> Result<Expr> {
        let mut lhs = self.parse_not()?;
        loop {
            self.skip_ws();
            // Only explicit AND or comma-AND continues; no implicit AND in v1
            // (avoids greedy consumption of operator keywords like OR).
            let saw_op = self.consume_kw("AND") || self.consume_char(',');
            if !saw_op {
                break;
            }
            let rhs = self.parse_not()?;
            lhs = Expr::And(Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_not(&mut self) -> Result<Expr> {
        self.skip_ws();
        if self.consume_kw("NOT") {
            let inner = self.parse_not()?;
            return Ok(Expr::Not(Box::new(inner)));
        }
        self.parse_atom()
    }

    fn parse_atom(&mut self) -> Result<Expr> {
        self.skip_ws();
        if self.consume_char('(') {
            let inner = self.parse_expr()?;
            self.skip_ws();
            if !self.consume_char(')') {
                return Err(self.err("expected ')'"));
            }
            return Ok(inner);
        }
        self.parse_predicate()
    }

    fn parse_predicate(&mut self) -> Result<Expr> {
        self.skip_ws();
        let field = self.parse_field()?;

        // `attr(name)` with no comparison → AttrExists.
        if matches!(field, Field::Attr(_)) && !self.peek_is_comparison_op() {
            return Ok(Expr::Predicate(Predicate::AttrExists(
                field.attr_name().unwrap().to_string(),
            )));
        }

        self.skip_ws();
        let op = self.parse_op()?;
        self.skip_ws();
        let value = self.parse_value()?;
        let comp = self.compile_comparison(field, op, value)?;
        Ok(Expr::Predicate(Predicate::Comparison(comp)))
    }

    fn parse_field(&mut self) -> Result<Field> {
        self.skip_ws();
        // attr(name)
        if self.consume_kw("ATTR") {
            self.skip_ws();
            if !self.consume_char('(') {
                return Err(self.err("expected '(' after 'attr'"));
            }
            self.skip_ws();
            let name = self.parse_identifier()?;
            self.skip_ws();
            if !self.consume_char(')') {
                return Err(self.err("expected ')' after attr name"));
            }
            return Ok(Field::Attr(name));
        }
        // Plain identifier → name/kind/class
        let id = self.parse_identifier()?;
        Ok(match id.to_ascii_lowercase().as_str() {
            "name" => Field::Name,
            "kind" => Field::Kind,
            "class" => Field::Class,
            // Forgiving: unknown identifiers treated as a custom field? No —
            // we reject here so the user gets a clear error.
            _ => return Err(self.err(format!("unknown field '{}'", id))),
        })
    }

    fn parse_identifier(&mut self) -> Result<String> {
        self.skip_ws();
        let start = self.pos;
        match self.peek() {
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                self.pos += c.len_utf8();
            }
            _ => return Err(self.err("expected identifier")),
        }
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' {
                self.pos += c.len_utf8();
            } else {
                break;
            }
        }
        Ok(self.src[start..self.pos].to_string())
    }

    fn parse_op(&mut self) -> Result<ComparisonOp> {
        self.skip_ws();
        // Longest match first
        if self.consume_str("!=") {
            return Ok(ComparisonOp::Ne);
        }
        if self.consume_str("~=") {
            return Ok(ComparisonOp::Regex);
        }
        if self.consume_str(">=") {
            return Ok(ComparisonOp::Ge);
        }
        if self.consume_str("<=") {
            return Ok(ComparisonOp::Le);
        }
        if self.consume_char('=') {
            return Ok(ComparisonOp::Eq);
        }
        if self.consume_char('>') {
            return Ok(ComparisonOp::Gt);
        }
        if self.consume_char('<') {
            return Ok(ComparisonOp::Lt);
        }
        // Bare `~` is a common typo → hint.
        if self.consume_char('~') {
            return Err(self.err("unsupported operator '~' (use '~=' for regex)"));
        }
        Err(self.err("expected comparison operator (=, !=, ~=, >, <, >=, <=)"))
    }

    fn peek_is_comparison_op(&self) -> bool {
        let s = self.src[self.pos..].trim_start();
        s.starts_with("==")
            || s.starts_with("!=")
            || s.starts_with("~=")
            || s.starts_with(">=")
            || s.starts_with("<=")
            || s.starts_with('=')
            || s.starts_with('>')
            || s.starts_with('<')
    }

    fn parse_value(&mut self) -> Result<QueryValue> {
        self.skip_ws();
        let c = self.peek().ok_or_else(|| self.err("expected value"))?;
        match c {
            '\'' | '"' => {
                let s = self.parse_string(c)?;
                Ok(QueryValue::String(s))
            }
            c if c.is_ascii_digit() || c == '+' || c == '-' || c == '.' => {
                let n = self.parse_number()?;
                Ok(n)
            }
            _ => {
                let id = self.parse_bareword()?;
                Ok(QueryValue::Bareword(id))
            }
        }
    }

    fn parse_string(&mut self, quote: char) -> Result<String> {
        // Consume opening quote
        self.pos += quote.len_utf8();
        let mut out = String::new();
        loop {
            let c = match self.peek() {
                Some(c) => c,
                None => return Err(self.err("unterminated string")),
            };
            if c == quote {
                self.pos += quote.len_utf8();
                return Ok(out);
            }
            if c == '\\' {
                self.pos += 1;
                match self.peek() {
                    Some('n') => {
                        out.push('\n');
                        self.pos += 1;
                    }
                    Some('t') => {
                        out.push('\t');
                        self.pos += 1;
                    }
                    Some('r') => {
                        out.push('\r');
                        self.pos += 1;
                    }
                    Some('\\') => {
                        out.push('\\');
                        self.pos += 1;
                    }
                    Some('\'') => {
                        out.push('\'');
                        self.pos += 1;
                    }
                    Some('"') => {
                        out.push('"');
                        self.pos += 1;
                    }
                    _ => return Err(self.err("bad escape in string")),
                }
            } else {
                out.push(c);
                self.pos += c.len_utf8();
            }
        }
    }

    fn parse_number(&mut self) -> Result<QueryValue> {
        let start = self.pos;
        // Optional sign
        if matches!(self.peek(), Some('+') | Some('-')) {
            self.pos += 1;
        }
        // Digits + optional .
        let mut saw_digit = false;
        let mut saw_dot = false;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                saw_digit = true;
                self.pos += 1;
            } else if c == '.' && !saw_dot {
                saw_dot = true;
                self.pos += 1;
            } else {
                break;
            }
        }
        if !saw_digit {
            return Err(self.err("expected number"));
        }
        // Exponent
        if matches!(self.peek(), Some('e') | Some('E')) {
            self.pos += 1;
            if matches!(self.peek(), Some('+') | Some('-')) {
                self.pos += 1;
            }
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    self.pos += 1;
                } else {
                    break;
                }
            }
        }
        let raw = self.src[start..self.pos].to_string();
        // Optional SI suffix
        let (num_str, mult) = match self.peek() {
            Some(c) => {
                let (m, len) = si_multiplier(c);
                if m != 1.0 {
                    self.pos += len;
                    (&raw[..], m)
                } else {
                    (&raw[..], 1.0)
                }
            }
            None => (&raw[..], 1.0),
        };
        let value: f64 = num_str
            .parse()
            .map_err(|_| anyhow!("query: invalid number '{}'", raw))?;
        Ok(QueryValue::Number {
            raw,
            value: value * mult,
        })
    }

    fn parse_bareword(&mut self) -> Result<String> {
        let mut s = self.parse_identifier()?;
        // Bareword values may include `*` / `?` for glob forms (=RES*).
        while let Some(c) = self.peek() {
            if c == '*' || c == '?' {
                s.push(c);
                self.pos += c.len_utf8();
            } else {
                break;
            }
        }
        if RESERVED_BAREWORDS
            .iter()
            .any(|r| r.eq_ignore_ascii_case(&s))
        {
            return Err(self.err(format!(
                "reserved keyword '{}' cannot be used as bareword value (quote it: \"{}\")",
                s, s
            )));
        }
        Ok(s)
    }

    fn starts_expression(&self) -> bool {
        let s = self.src[self.pos..].trim_start();
        if s.is_empty() {
            return false;
        }
        let c = s.chars().next().unwrap();
        c == '('
            || s.to_ascii_lowercase().starts_with("not ")
            || s.to_ascii_lowercase().starts_with("not\t")
            || s.to_ascii_lowercase().starts_with("not(")
            || s.to_ascii_lowercase().starts_with("not\r")
            || matches!(c, 'a'..='z' | 'A'..='Z' | '_')
    }

    fn compile_comparison(
        &self,
        field: Field,
        op: ComparisonOp,
        value: QueryValue,
    ) -> Result<Comparison> {
        let matcher = match op {
            ComparisonOp::Regex => {
                let pat = match &value {
                    QueryValue::String(s) | QueryValue::Bareword(s) => s.clone(),
                    QueryValue::Number { raw, .. } => raw.clone(),
                };
                let re = Regex::new(&format!("(?i){}", pat))
                    .map_err(|e| anyhow!("query: invalid regex '{}': {}", pat, e))?;
                Some(re)
            }
            ComparisonOp::Eq | ComparisonOp::Ne => match &value {
                QueryValue::String(s) | QueryValue::Bareword(s)
                    if s.contains('*') || s.contains('?') =>
                {
                    let re = Regex::new(&format!("(?i)^{}$", glob_to_regex_str(s)))
                        .map_err(|e| anyhow!("query: invalid glob '{}': {}", s, e))?;
                    Some(re)
                }
                _ => None,
            },
            _ => None,
        };
        Ok(Comparison {
            field,
            op,
            value,
            matcher,
        })
    }
}

/// SI suffix → multiplier (case-sensitive: M != m).
fn si_multiplier(c: char) -> (f64, usize) {
    match c {
        'k' => (1e3, 1),
        'M' => (1e6, 1),
        'G' => (1e9, 1),
        'm' => (1e-3, 1),
        'u' => (1e-6, 1),
        'n' => (1e-9, 1),
        'p' => (1e-12, 1),
        _ => (1.0, 0),
    }
}

fn glob_to_regex_str(glob: &str) -> String {
    let mut out = String::new();
    for ch in glob.chars() {
        match ch {
            '*' => out.push_str(".*"),
            '?' => out.push('.'),
            '.' | '(' | ')' | '[' | ']' | '{' | '}' | '+' | '|' | '^' | '$' | '\\' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_eq() {
        let q = compile("name=RES").unwrap();
        assert!(matches!(
            q,
            Expr::Predicate(Predicate::Comparison(Comparison {
                field: Field::Name,
                op: ComparisonOp::Eq,
                ..
            }))
        ));
    }

    #[test]
    fn parse_and_or_not_parens() {
        let q = compile("(kind=component OR kind=module) AND NOT name=RES*").unwrap();
        // Top level: And
        match q {
            Expr::And(a, b) => {
                // Left should be Or wrapped in parens
                assert!(matches!(*a, Expr::Or(_, _)));
                // Right should be Not
                assert!(matches!(*b, Expr::Not(_)));
            }
            _ => panic!("expected And"),
        }
    }

    #[test]
    fn parse_legacy_comma_as_and() {
        let q = compile("name=R*,kind=component").unwrap();
        assert!(matches!(q, Expr::And(_, _)));
    }

    #[test]
    fn parse_keywords_case_insensitive() {
        compile("kind=component and name=RES").unwrap();
        compile("NOT kind=enum").unwrap();
        compile("kind=component Or kind=module").unwrap();
    }

    #[test]
    fn parse_attr() {
        let q = compile("attr(value)>100").unwrap();
        match q {
            Expr::Predicate(Predicate::Comparison(c)) => {
                assert!(matches!(c.field, Field::Attr(ref n) if n == "value"));
                assert_eq!(c.op, ComparisonOp::Gt);
            }
            _ => panic!("expected Comparison"),
        }
    }

    #[test]
    fn parse_attr_exists() {
        let q = compile("attr(missing)").unwrap();
        assert!(matches!(
            q,
            Expr::Predicate(Predicate::AttrExists(ref n)) if n == "missing"
        ));
    }

    #[test]
    fn parse_regex() {
        let q = compile(r#"attr(description)~="USB""#).unwrap();
        assert!(matches!(
            q,
            Expr::Predicate(Predicate::Comparison(Comparison {
                op: ComparisonOp::Regex,
                ..
            }))
        ));
    }

    #[test]
    fn parse_bare_tilde_errors_with_hint() {
        let err = compile("name~RES").unwrap_err().to_string();
        assert!(err.contains("~="), "err was: {}", err);
    }

    #[test]
    fn parse_invalid_regex_errors() {
        let err = compile(r#"attr(d)~="[unclosed""#).unwrap_err().to_string();
        assert!(err.contains("regex") || err.contains("query"));
    }

    #[test]
    fn parse_unknown_field_errors() {
        let err = compile("foo=bar").unwrap_err().to_string();
        assert!(err.contains("unknown field"));
    }

    #[test]
    fn parse_reserved_bareword_errors() {
        let err = compile("name=AND").unwrap_err().to_string();
        assert!(err.contains("reserved"));
    }

    #[test]
    fn si_suffix_multipliers() {
        // 1k = 1000, 1M = 1e6, 1m = 1e-3, 1u = 1e-6
        let v = |s: &str| match compile(&format!("attr(x)>{s}")).unwrap() {
            Expr::Predicate(Predicate::Comparison(c)) => match c.value {
                QueryValue::Number { value, .. } => value,
                _ => panic!("not a number"),
            },
            _ => panic!("not a comparison"),
        };
        assert!((v("1k") - 1e3).abs() < 1e-9);
        assert!((v("1M") - 1e6).abs() < 1e-9);
        assert!((v("1m") - 1e-3).abs() < 1e-9);
        assert!((v("1u") - 1e-6).abs() < 1e-9);
        assert!((v("1n") - 1e-9).abs() < 1e-9);
        assert!((v("1p") - 1e-12).abs() < 1e-12);
    }

    #[test]
    fn si_M_vs_m_distinct() {
        // M (mega) vs m (milli)
        let v = |s: &str| match compile(&format!("attr(x)>{s}")).unwrap() {
            Expr::Predicate(Predicate::Comparison(c)) => match c.value {
                QueryValue::Number { value, .. } => value,
                _ => panic!("not a number"),
            },
            _ => panic!("not a comparison"),
        };
        assert!((v("1M") - 1e6).abs() < 1e-9);
        assert!((v("1m") - 1e-3).abs() < 1e-9);
    }

    #[test]
    fn eval_name_exact_and_glob() {
        let q = compile("name=RES").unwrap();
        assert!(matches_definition(
            &q,
            Some("component"),
            Some("RES"),
            None,
            None
        ));
        assert!(!matches_definition(
            &q,
            Some("component"),
            Some("RES1"),
            None,
            None
        ));

        let q = compile("name=RES*").unwrap();
        assert!(matches_definition(
            &q,
            Some("component"),
            Some("RES"),
            None,
            None
        ));
        assert!(matches_definition(
            &q,
            Some("component"),
            Some("RES10K"),
            None,
            None
        ));
        assert!(!matches_definition(
            &q,
            Some("component"),
            Some("CAP1"),
            None,
            None
        ));
    }

    #[test]
    fn eval_ne() {
        let q = compile("name!=RES").unwrap();
        assert!(!matches_definition(
            &q,
            Some("component"),
            Some("RES"),
            None,
            None
        ));
        assert!(matches_definition(
            &q,
            Some("component"),
            Some("CAP"),
            None,
            None
        ));
        // Missing field → != is false
        assert!(!matches_definition(&q, None, None, None, None));
    }

    #[test]
    fn eval_or_and_not_no_short_circuit() {
        // Both branches evaluated (no short-circuit)
        let q = compile("NOT kind=enum").unwrap();
        assert!(matches_definition(
            &q,
            Some("component"),
            Some("X"),
            None,
            None
        ));
        assert!(!matches_definition(&q, Some("enum"), Some("X"), None, None));
    }

    #[test]
    fn eval_class_is_always_false_on_top_level() {
        // class is always None on top-level defs → class=... always false
        let q = compile("class=RES").unwrap();
        assert!(!matches_definition(
            &q,
            Some("component"),
            Some("RES"),
            None,
            None
        ));
    }

    #[test]
    fn eval_attr_on_module_enum_false() {
        let q = compile("attr(value)>100").unwrap();
        assert!(!matches_definition_with_attrs(
            &q,
            Some("module"),
            Some("main"),
            None,
            None,
            &[("value".into(), "1000".into())]
        ));
        assert!(!matches_definition_with_attrs(
            &q,
            Some("enum"),
            Some("PKG"),
            None,
            None,
            &[("value".into(), "1000".into())]
        ));
    }

    #[test]
    fn eval_attr_numeric() {
        let q = compile("attr(maxdistance)>100").unwrap();
        // "1200m" cannot be parsed as f64 → false
        assert!(!matches_definition_with_attrs(
            &q,
            Some("interface"),
            Some("I"),
            None,
            None,
            &[("maxdistance".into(), "1200m".into())]
        ));
        // Numeric "1200" parses to f64 → true
        assert!(matches_definition_with_attrs(
            &q,
            Some("component"),
            Some("R1"),
            None,
            None,
            &[("maxdistance".into(), "1200".into())]
        ));
        assert!(!matches_definition_with_attrs(
            &q,
            Some("component"),
            Some("R2"),
            None,
            None,
            &[("maxdistance".into(), "50".into())]
        ));
    }

    #[test]
    fn eval_attr_exists() {
        let q = compile("attr(missing)").unwrap();
        assert!(matches_definition_with_attrs(
            &q,
            Some("component"),
            Some("X"),
            None,
            None,
            &[("missing".into(), "v".into())]
        ));
        assert!(!matches_definition_with_attrs(
            &q,
            Some("component"),
            Some("X"),
            None,
            None,
            &[("other".into(), "v".into())]
        ));
    }

    #[test]
    fn eval_regex() {
        let q = compile(r#"attr(description)~="USB""#).unwrap();
        assert!(matches_definition_with_attrs(
            &q,
            Some("interface"),
            Some("I"),
            None,
            None,
            &[("description".into(), "USB Type-C connector".into())]
        ));
        assert!(!matches_definition_with_attrs(
            &q,
            Some("interface"),
            Some("I"),
            None,
            None,
            &[("description".into(), "Ethernet RJ45".into())]
        ));
    }

    #[test]
    fn validate_allowed_fields_rejects_unknown() {
        // `kind=component` is a valid AST; allowed_keys excludes `kind` → error
        let q = compile("kind=component").unwrap();
        let err = validate_allowed_fields(&q, &["name"]).unwrap_err();
        assert!(format!("{}", err).contains("unknown key"));

        // Should pass when `kind` is allowed
        let q = compile("attr(x)=1").unwrap();
        validate_allowed_fields(&q, &["name", "attr"]).unwrap();
    }

    #[test]
    fn validate_recurses_into_compound() {
        // `kind=component AND class=foo` is parseable; allowed set excludes `class` → error
        let q = compile("kind=component AND class=foo").unwrap();
        let err = validate_allowed_fields(&q, &["name", "kind"]).unwrap_err();
        assert!(format!("{}", err).contains("unknown key"));
    }

    #[test]
    fn needs_attrs_detects_attr_references() {
        let q = compile("kind=component").unwrap();
        assert!(!needs_attrs(&q));
        let q = compile("attr(x)>1").unwrap();
        assert!(needs_attrs(&q));
        let q = compile("kind=component OR attr(x)>1").unwrap();
        assert!(needs_attrs(&q));
    }

    #[test]
    fn json_record_resolves_attrs_via_closure() {
        let q = compile("attr(x)>100").unwrap();
        let item = serde_json::json!({"name": "R1", "uri": "u", "kind": "component"});
        // Closure returns attrs
        let ok = matches_json_record_with(&q, &item, |_, _| vec![("x".into(), "200".into())]);
        assert!(ok);
        let no = matches_json_record_with(&q, &item, |_, _| vec![("x".into(), "50".into())]);
        assert!(!no);
        // No resolver → attrs unavailable → false
        let none = matches_json_record(&q, &item);
        assert!(!none);
    }

    #[test]
    fn json_record_skips_resolver_when_not_needed() {
        let q = compile("kind=component").unwrap();
        let item = serde_json::json!({"name": "R1", "uri": "u", "kind": "component"});
        // No resolver needed; passes even with empty closure
        assert!(matches_json_record(&q, &item));
    }

    #[test]
    fn parse_quoted_strings_with_escapes() {
        let q = compile(r#"name="USB \"3.0\"""#).unwrap();
        match q {
            Expr::Predicate(Predicate::Comparison(c)) => match c.value {
                QueryValue::String(s) => assert_eq!(s, "USB \"3.0\""),
                _ => panic!("not string"),
            },
            _ => panic!("not comparison"),
        }
    }
}
