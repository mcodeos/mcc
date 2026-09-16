// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The value-evaluation engine (domain doc/eval).
//!
//! One value domain, one suffix table, one operator law. Everything that needs
//! to read a number out of the language — conditions, expression arithmetic,
//! pin-id expansion, power-intent decoders — goes through here, so "what does
//! `3000mV == 3V` mean" has exactly one answer (it is true).
//!
//! Laws (doc/eval §2):
//!   - V1 first-class values: a value is a value whatever door it came through;
//!   - V2 operators dispatch on operand nature: value+value is arithmetic,
//!     terminal+terminal is concatenation, and a string with `+` interpolates;
//!   - V3 one value domain: [`Value`] is the leaf projection shared by literals,
//!     expressions and bound arguments; `raw` text is display-only;
//!   - V5 no implicit promotion: a unitless number adopts the dimensioned
//!     operand's family, a dimensioned value never silently changes family;
//!   - V6 pure evaluation: these functions read nothing but their arguments;
//!   - V7 one engine: no caller may re-implement suffix stripping or unit math;
//!   - V8 failure is a diagnostic: [`apply`]/[`satisfies`] return an
//!     [`EvalError`], and [`report`] turns it into the registered code.

pub mod units;

use std::cmp::Ordering;

use crate::ast::node::AstNode;
use crate::db::diagnostic::diagnostic::dlog_error;
use crate::semantic::basic::mc_uval::{McUnit, McUnitValue};

/// A first-class value (V1/V3). `raw` text lives inside [`McUnitValue`] and is
/// never read for semantics — only for display.
#[derive(Debug, Clone)]
pub enum Value {
    /// A whole number, dimension-less.
    Int(i64),
    /// A real number, dimension-less.
    Float(f64),
    /// A number with a unit family (this is where `1200mV` lands, as 1.2 V).
    Quantity(McUnitValue),
    /// Text: a quoted string, an identifier, a keyword. Terminal, not a value.
    Str(String),
    /// The `_` placeholder: usable nowhere, comparable with nothing.
    Undef,
}

/// The four arithmetic operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
}

impl Op {
    /// Operator as written in source, for diagnostics.
    pub fn symbol(self) -> &'static str {
        match self {
            Op::Add => "+",
            Op::Sub => "-",
            Op::Mul => "*",
            Op::Div => "/",
        }
    }
}

/// The six comparison operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compare {
    Eq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
}

impl Compare {
    /// Operator as written in source, for diagnostics.
    pub fn symbol(self) -> &'static str {
        match self {
            Compare::Eq => "==",
            Compare::NotEq => "!=",
            Compare::Lt => "<",
            Compare::Gt => ">",
            Compare::LtEq => "<=",
            Compare::GtEq => ">=",
        }
    }

    fn is_equality(self) -> bool {
        matches!(self, Compare::Eq | Compare::NotEq)
    }
}

/// Why an evaluation could not produce a value (V8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalError {
    /// The divisor was zero.
    DivideByZero,
    /// The operator has no meaning for these operands — a mixed value/terminal
    /// pair, two families that do not meet, or a family that forbids the
    /// operator (temperature addition, decibel arithmetic, ordering on a
    /// non-ordered family).
    OperandNotNumeric {
        op: String,
        lhs: String,
        rhs: String,
    },
    /// Integer arithmetic left the i64 range (or a real result left the finite
    /// range).
    Overflow {
        op: String,
        lhs: String,
        rhs: String,
    },
}

impl EvalError {
    /// The registered diagnostic code for this failure.
    pub fn code(&self) -> u32 {
        match self {
            EvalError::DivideByZero => crate::errcodes::EVAL_DIVIDE_BY_ZERO,
            EvalError::OperandNotNumeric { .. } => crate::errcodes::EVAL_OPERAND_NOT_NUMERIC,
            EvalError::Overflow { .. } => crate::errcodes::EVAL_OVERFLOW,
        }
    }

    /// The canonical message for this failure.
    pub fn message(&self) -> String {
        match self {
            EvalError::DivideByZero => crate::errcodes::format_msg(self.code(), &[]),
            EvalError::OperandNotNumeric { op, lhs, rhs }
            | EvalError::Overflow { op, lhs, rhs } => {
                crate::errcodes::format_msg(self.code(), &[op, lhs, rhs])
            }
        }
    }
}

/// Emit `err` at `node` (V8). Callers that hold no node — the condition and
/// expression paths, which see bound text rather than syntax — cannot position
/// a diagnostic and handle the error themselves.
pub fn report(err: &EvalError, node: &AstNode) {
    dlog_error(err.code(), node, &err.message());
}

fn not_numeric(op: &str, lhs: &Value, rhs: &Value) -> EvalError {
    EvalError::OperandNotNumeric {
        op: op.to_string(),
        lhs: class_of(lhs),
        rhs: class_of(rhs),
    }
}

/// Operand class for a diagnostic: the family name when there is one, else the
/// kind of text.
fn class_of(value: &Value) -> String {
    match value {
        Value::Int(_) => "integer".to_string(),
        Value::Float(_) => "float".to_string(),
        Value::Quantity(q) => format!("'{}'", q.unit()),
        Value::Str(s) => format!("text '{s}'"),
        Value::Undef => "'_'".to_string(),
    }
}

/// Operator support per unit family — the ruled arithmetic table (doc/eval
/// §2.6). Rows are families, columns are the operators. A `false` cell means
/// "this operator is an error for this family", never "some other result".
///
/// Temperature is additive only in the difference sense (a - b is a span,
/// a + b is not a temperature); decibel and noise-density values are not
/// numbers on a ratio scale, so only equality is defined for them.
struct FamilyOps {
    add: bool,
    sub: bool,
    mul: bool,
    div: bool,
    order: bool,
}

const LINEAR: FamilyOps = FamilyOps {
    add: true,
    sub: true,
    mul: true,
    div: true,
    order: true,
};

fn family_ops(unit: &McUnit) -> FamilyOps {
    match unit {
        McUnit::Temp => FamilyOps {
            add: false,
            sub: true,
            mul: false,
            div: false,
            order: true,
        },
        McUnit::Db | McUnit::Noise => FamilyOps {
            add: false,
            sub: false,
            mul: false,
            div: false,
            order: false,
        },
        _ => LINEAR,
    }
}

impl FamilyOps {
    fn allows(&self, op: Op) -> bool {
        match op {
            Op::Add => self.add,
            Op::Sub => self.sub,
            Op::Mul => self.mul,
            Op::Div => self.div,
        }
    }
}

impl Value {
    /// Read a value out of text (V4: the same tables the AST path uses).
    ///
    /// Text that carries no unit suffix and is not a number is [`Value::Str`] —
    /// the caller decides nothing further, because at this layer a bound
    /// argument and a quoted string are the same kind of thing.
    pub fn from_text(text: &str) -> Value {
        let t = text.trim();
        if t == "_" {
            return Value::Undef;
        }
        if let Some(n) = int_literal(t) {
            return Value::Int(n);
        }
        if let Some((value, unit)) = units::parse_text(t) {
            // Keep the author's notation for display, exactly as the AST path
            // does; it is never read back for semantics.
            return Value::Quantity(McUnitValue::from_normalized(
                value,
                unit,
                Some(t.to_string()),
            ));
        }
        if let Ok(f) = t.parse::<f64>() {
            return Value::Float(f);
        }
        // Text is kept exactly as it arrived: a caller that passes a string
        // literal's own text must get that text back, trailing space and all.
        Value::Str(text.to_string())
    }

    /// Wrap an already-typed unit value.
    pub fn from_quantity(value: McUnitValue) -> Value {
        Value::Quantity(value)
    }

    /// The unit family, when this value has one.
    pub fn unit(&self) -> Option<&McUnit> {
        match self {
            Value::Quantity(q) => Some(q.unit()),
            _ => None,
        }
    }

    /// The number this value denotes, ignoring any unit (V3: the numeric
    /// projection). `None` for text and for `_`.
    pub fn number(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Float(f) => Some(*f),
            Value::Quantity(q) => Some(q.value()),
            Value::Str(_) | Value::Undef => None,
        }
    }

    /// How this value would be written back out. Unit values echo their source
    /// text when they have it, so a value that only travelled through the
    /// engine keeps the author's notation.
    pub fn text(&self) -> String {
        match self {
            Value::Int(i) => i.to_string(),
            Value::Float(f) => f.to_string(),
            Value::Quantity(q) => q.to_string(),
            Value::Str(s) => s.clone(),
            Value::Undef => "_".to_string(),
        }
    }
}

/// Decimal or `0x`/`0X` hexadecimal integer literal.
fn int_literal(text: &str) -> Option<i64> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let magnitude = match digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        Some(hex) => i64::from_str_radix(hex, 16).ok()?,
        None => {
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            digits.parse::<i64>().ok()?
        }
    };
    Some(if negative { -magnitude } else { magnitude })
}

/// A numeric view of a value: the two bases the arithmetic laws distinguish.
enum Num<'a> {
    Int(i64),
    Float(f64),
    Dim(&'a McUnitValue),
}

impl Num<'_> {
    /// The number this operand denotes, whatever its family.
    fn number(&self) -> f64 {
        match self {
            Num::Int(i) => *i as f64,
            Num::Float(f) => *f,
            Num::Dim(q) => q.value(),
        }
    }
}

fn num_of(value: &Value) -> Option<Num<'_>> {
    match value {
        Value::Int(i) => Some(Num::Int(*i)),
        Value::Float(f) => Some(Num::Float(*f)),
        Value::Quantity(q) => Some(Num::Dim(q)),
        Value::Str(_) | Value::Undef => None,
    }
}

fn is_text(value: &Value) -> bool {
    matches!(value, Value::Str(_) | Value::Undef)
}

/// Apply an arithmetic operator (V2).
///
/// `Str + anything` interpolates (that is how `"width " + cols` resolves);
/// every other use of text is an error. Numeric operands follow the family
/// table; a unitless number adopts the dimensioned operand's family, and two
/// dimensioned operands must already share one.
pub fn apply(op: Op, lhs: &Value, rhs: &Value) -> Result<Value, EvalError> {
    if matches!(op, Op::Add) {
        match (lhs, rhs) {
            (Value::Str(a), other) if !matches!(other, Value::Undef) => {
                return Ok(Value::Str(format!("{a}{}", other.text())));
            }
            (other, Value::Str(b)) if !matches!(other, Value::Undef) => {
                return Ok(Value::Str(format!("{}{b}", other.text())));
            }
            _ => {}
        }
    }
    if is_text(lhs) || is_text(rhs) {
        return Err(not_numeric(op.symbol(), lhs, rhs));
    }
    let (Some(l), Some(r)) = (num_of(lhs), num_of(rhs)) else {
        return Err(not_numeric(op.symbol(), lhs, rhs));
    };
    numeric_apply(op, l, r, lhs, rhs)
}

fn numeric_apply(
    op: Op,
    lhs: Num<'_>,
    rhs: Num<'_>,
    lhs_value: &Value,
    rhs_value: &Value,
) -> Result<Value, EvalError> {
    match (&lhs, &rhs) {
        (Num::Int(a), Num::Int(b)) => int_apply(op, *a, *b, lhs_value, rhs_value),
        (Num::Dim(a), Num::Dim(b)) => {
            // Two dimensioned operands must share a family, and the result of
            // `*` or `/` would be a derived unit this domain does not have.
            if a.unit() != b.unit() || matches!(op, Op::Mul | Op::Div) {
                return Err(not_numeric(op.symbol(), lhs_value, rhs_value));
            }
            if !family_ops(a.unit()).allows(op) {
                return Err(not_numeric(op.symbol(), lhs_value, rhs_value));
            }
            let value = real_apply(op, a.value(), b.value(), lhs_value, rhs_value)?;
            Ok(Value::Quantity(McUnitValue::from_normalized(
                value,
                a.unit().clone(),
                None,
            )))
        }
        (Num::Dim(q), Num::Int(n)) => dim_and_number(op, q, *n as f64, true, lhs_value, rhs_value),
        (Num::Int(n), Num::Dim(q)) => dim_and_number(op, q, *n as f64, false, lhs_value, rhs_value),
        (Num::Dim(q), Num::Float(f)) => dim_and_number(op, q, *f, true, lhs_value, rhs_value),
        (Num::Float(f), Num::Dim(q)) => dim_and_number(op, q, *f, false, lhs_value, rhs_value),
        (Num::Int(a), Num::Float(b)) => Ok(Value::Float(real_apply(
            op, *a as f64, *b, lhs_value, rhs_value,
        )?)),
        (Num::Float(a), Num::Int(b)) => Ok(Value::Float(real_apply(
            op, *a, *b as f64, lhs_value, rhs_value,
        )?)),
        (Num::Float(a), Num::Float(b)) => {
            Ok(Value::Float(real_apply(op, *a, *b, lhs_value, rhs_value)?))
        }
    }
}

/// A dimensioned value against a unitless number. The number carries no family
/// of its own, so for `+`/`-` it adopts the dimensioned side's; for `*`/`/` it
/// scales. `number / quantity` would need a reciprocal unit and is an error.
fn dim_and_number(
    op: Op,
    quantity: &McUnitValue,
    number: f64,
    dim_on_left: bool,
    lhs_value: &Value,
    rhs_value: &Value,
) -> Result<Value, EvalError> {
    let ops = family_ops(quantity.unit());
    let value = match op {
        Op::Add | Op::Sub => {
            if !ops.allows(op) {
                return Err(not_numeric(op.symbol(), lhs_value, rhs_value));
            }
            if dim_on_left {
                if matches!(op, Op::Add) {
                    quantity.value() + number
                } else {
                    quantity.value() - number
                }
            } else if matches!(op, Op::Add) {
                number + quantity.value()
            } else {
                number - quantity.value()
            }
        }
        Op::Mul => {
            if !ops.allows(op) {
                return Err(not_numeric(op.symbol(), lhs_value, rhs_value));
            }
            quantity.value() * number
        }
        Op::Div => {
            if !ops.allows(op) || !dim_on_left {
                return Err(not_numeric(op.symbol(), lhs_value, rhs_value));
            }
            if number == 0.0 {
                return Err(EvalError::DivideByZero);
            }
            quantity.value() / number
        }
    };
    check_finite(value, op, lhs_value, rhs_value)?;
    Ok(Value::Quantity(McUnitValue::from_normalized(
        value,
        quantity.unit().clone(),
        None,
    )))
}

fn int_apply(
    op: Op,
    a: i64,
    b: i64,
    lhs_value: &Value,
    rhs_value: &Value,
) -> Result<Value, EvalError> {
    let result = match op {
        Op::Add => a.checked_add(b),
        Op::Sub => a.checked_sub(b),
        Op::Mul => a.checked_mul(b),
        Op::Div => {
            if b == 0 {
                return Err(EvalError::DivideByZero);
            }
            a.checked_div(b)
        }
    };
    match result {
        Some(value) => Ok(Value::Int(value)),
        None => Err(EvalError::Overflow {
            op: op.symbol().to_string(),
            lhs: lhs_value.text(),
            rhs: rhs_value.text(),
        }),
    }
}

fn real_apply(
    op: Op,
    a: f64,
    b: f64,
    lhs_value: &Value,
    rhs_value: &Value,
) -> Result<f64, EvalError> {
    if matches!(op, Op::Div) && b == 0.0 {
        return Err(EvalError::DivideByZero);
    }
    let value = match op {
        Op::Add => a + b,
        Op::Sub => a - b,
        Op::Mul => a * b,
        Op::Div => a / b,
    };
    check_finite(value, op, lhs_value, rhs_value)?;
    Ok(value)
}

/// A finite input can still overflow to infinity; that is an arithmetic
/// failure, not a value (V8), so it is reported rather than propagated.
fn check_finite(value: f64, op: Op, lhs_value: &Value, rhs_value: &Value) -> Result<(), EvalError> {
    if value.is_finite() {
        return Ok(());
    }
    Err(EvalError::Overflow {
        op: op.symbol().to_string(),
        lhs: lhs_value.text(),
        rhs: rhs_value.text(),
    })
}

/// Value comparison slop, absolute — the same 1e-9 the 6xxx rules compare
/// decoded electrical values with (nets/mod.rs, nets/window.rs). It is what
/// keeps the notations of one quantity interchangeable: `3300mV` and `3.3V`
/// come out of the suffix table four ulps apart, and an author who wrote one
/// where the other is written must not get a dead branch.
const VALUE_EPSILON: f64 = 1e-9;

/// The ordering of two values, or the reason there is none.
pub fn ordering(lhs: &Value, rhs: &Value) -> Result<Ordering, EvalError> {
    if is_text(lhs) || is_text(rhs) {
        return Err(not_numeric("compare", lhs, rhs));
    }
    let (Some(l), Some(r)) = (num_of(lhs), num_of(rhs)) else {
        return Err(not_numeric("compare", lhs, rhs));
    };
    let (a, b, family) = match (&l, &r) {
        (Num::Dim(x), Num::Dim(y)) => {
            if x.unit() != y.unit() {
                return Err(not_numeric("compare", lhs, rhs));
            }
            (x.value(), y.value(), Some(x.unit().clone()))
        }
        (Num::Dim(x), _) => (x.value(), r.number(), Some(x.unit().clone())),
        (_, Num::Dim(y)) => (l.number(), y.value(), Some(y.unit().clone())),
        _ => (l.number(), r.number(), None),
    };
    // Within the comparator's slop the two are the same value, so the slop
    // reads as equality here and `<=`/`>=` come out true on it.
    let ordering = if (a - b).abs() <= VALUE_EPSILON {
        Ordering::Equal
    } else {
        a.partial_cmp(&b)
            .ok_or_else(|| not_numeric("compare", lhs, rhs))?
    };
    // Non-ordered families (decibel, noise density) admit equality only.
    if !matches!(ordering, Ordering::Equal) && family.as_ref().is_some_and(|u| !family_ops(u).order)
    {
        return Err(not_numeric("compare", lhs, rhs));
    }
    Ok(ordering)
}

/// Evaluate a comparison (V2 + the family table).
pub fn satisfies(cmp: Compare, lhs: &Value, rhs: &Value) -> Result<bool, EvalError> {
    if let (Value::Str(a), Value::Str(b)) = (lhs, rhs) {
        // Text is a terminal value with no order (V2 / doc §2.4): equality is
        // the only comparison it answers.
        if !cmp.is_equality() {
            return Err(not_numeric(cmp.symbol(), lhs, rhs));
        }
        return Ok(if matches!(cmp, Compare::Eq) {
            a == b
        } else {
            a != b
        });
    }
    let ordering = ordering(lhs, rhs)?;
    // Ordering comparisons additionally require an ordered family.
    if !cmp.is_equality() {
        if let Some(unit) = lhs.unit().or_else(|| rhs.unit()) {
            if !family_ops(unit).order {
                return Err(not_numeric(cmp.symbol(), lhs, rhs));
            }
        }
    }
    Ok(match cmp {
        Compare::Eq => ordering == Ordering::Equal,
        Compare::NotEq => ordering != Ordering::Equal,
        Compare::Lt => ordering == Ordering::Less,
        Compare::Gt => ordering == Ordering::Greater,
        Compare::LtEq => ordering != Ordering::Greater,
        Compare::GtEq => ordering != Ordering::Less,
    })
}

/// Read a quantity written in `family`'s notation out of text (`3.3V`,
/// `500mA`, `1.5A`).
///
/// A bare number reads as the family's canonical unit — the rail notation
/// `::DC(5)` means 5 V — which is the one promotion this door allows (V5: the
/// family comes from the question, never from the text). Text naming another
/// family (`5A` where volts are asked), text carrying no number at all
/// (`WIDE`), and window forms (`2.5V~5.5V`, `5V±5%`) do not decode.
pub fn quantity_in(text: &str, family: &McUnit) -> Option<f64> {
    match Value::from_text(text) {
        Value::Quantity(q) if q.unit() == family => Some(q.value()),
        Value::Int(i) => Some(i as f64),
        Value::Float(f) => Some(f),
        _ => None,
    }
}

/// Read a ratio out of text: `95%` → 0.95, `0.95` → 0.95.
///
/// The percent family is projected to its fraction here (its suffix factor is
/// 1, so `95%` normalizes to 95); a bare number *is* the ratio. Any other
/// family, and any text with no number, does not decode.
pub fn ratio_of(text: &str) -> Option<f64> {
    match Value::from_text(text) {
        Value::Quantity(q) if q.unit() == &McUnit::Percent => Some(q.value() / 100.0),
        Value::Int(i) => Some(i as f64),
        Value::Float(f) => Some(f),
        _ => None,
    }
}

/// Read a percent out of text: `5%` → 0.05 **and** `5` → 0.05.
///
/// The `%` is notation, not meaning, on a key whose value is a percentage — a
/// bare number there reads as percent rather than as the ratio 5. Use
/// [`ratio_of`] for a key whose bare number is a factor.
pub fn percent_of(text: &str) -> Option<f64> {
    match Value::from_text(text) {
        Value::Quantity(q) if q.unit() == &McUnit::Percent => Some(q.value() / 100.0),
        Value::Int(i) => Some(i as f64 / 100.0),
        Value::Float(f) => Some(f / 100.0),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Value {
        Value::from_text(text)
    }

    fn add(l: &str, r: &str) -> Result<Value, EvalError> {
        apply(Op::Add, &v(l), &v(r))
    }

    #[test]
    fn eval__unit_text_is_one_value() {
        // The defect this engine exists for: the same rail written two ways.
        assert_eq!(v("1200mV").number(), Some(1.2));
        assert_eq!(v("1.2V").number(), Some(1.2));
        assert!(satisfies(Compare::Eq, &v("3000mV"), &v("3V")).unwrap());
        assert!(satisfies(Compare::Eq, &v("1.2V"), &v("1200mV")).unwrap());
        assert!(satisfies(Compare::NotEq, &v("3000mV"), &v("3.3V")).unwrap());
        assert!(satisfies(Compare::Lt, &v("1200mV"), &v("1.3V")).unwrap());
    }

    #[test]
    fn eval__unitless_number_adopts_the_dimensioned_family() {
        // `if (volt < 0)` in the shipped standard library must keep meaning
        // "a negative rail": a bare number carries no family of its own.
        assert!(satisfies(Compare::Lt, &v("-2.5V"), &v("0")).unwrap());
        assert!(!satisfies(Compare::Lt, &v("2.5V"), &v("0")).unwrap());
        assert!(satisfies(Compare::Eq, &v("3.3V"), &v("3.3")).unwrap());
    }

    #[test]
    fn eval__text_is_terminal_and_text_plus_value_interpolates() {
        assert!(satisfies(Compare::Eq, &v("WIDE"), &v("WIDE")).unwrap());
        assert!(satisfies(Compare::NotEq, &v("WIDE"), &v("NARROW")).unwrap());
        // Text is pasted verbatim, spacing included — this is how the shipped
        // `"width " + cols + " pins"` descriptions resolve.
        let joined = apply(Op::Add, &Value::Str("width ".to_string()), &v("42")).unwrap();
        assert_eq!(joined.text(), "width 42");
        let chained = apply(Op::Add, &joined, &Value::Str(" pins".to_string())).unwrap();
        assert_eq!(chained.text(), "width 42 pins");
        // Text is not a number: arithmetic on it other than `+` fails.
        assert_eq!(
            apply(Op::Sub, &v("42"), &v("WIDE")).unwrap_err(),
            not_numeric("-", &v("42"), &v("WIDE"))
        );
        // Mixed comparison is a mixed-typing error, not a silent false.
        assert!(satisfies(Compare::Eq, &v("1.2V"), &v("WIDE")).is_err());
        // Text answers equality only; ordering it is not defined.
        assert!(satisfies(Compare::Lt, &v("WIDE"), &v("NARROW")).is_err());
        assert!(satisfies(Compare::GtEq, &v("WIDE"), &v("WIDE")).is_err());
    }

    #[test]
    fn eval__arithmetic_is_checked() {
        assert!(matches!(
            apply(Op::Div, &v("4"), &v("0")),
            Err(EvalError::DivideByZero)
        ));
        assert!(matches!(
            apply(Op::Div, &v("3.3V"), &v("0")),
            Err(EvalError::DivideByZero)
        ));
        let big = i64::MAX.to_string();
        assert!(matches!(
            apply(Op::Add, &v(&big), &v("1")),
            Err(EvalError::Overflow { .. })
        ));
        assert_eq!(apply(Op::Mul, &v("6"), &v("7")).unwrap().text(), "42");
        assert_eq!(apply(Op::Sub, &v("6"), &v("7")).unwrap().text(), "-1");
        assert_eq!(apply(Op::Div, &v("7"), &v("2")).unwrap().text(), "3");
    }

    #[test]
    fn eval__same_family_arithmetic_and_derived_unit_refusal() {
        let sum = apply(Op::Add, &v("1.2V"), &v("300mV")).unwrap();
        assert_eq!(sum.number(), Some(1.5));
        assert_eq!(sum.unit(), Some(&McUnit::Volt));
        let diff = apply(Op::Sub, &v("2V"), &v("500mV")).unwrap();
        assert_eq!(diff.number(), Some(1.5));
        // Two dimensioned operands never produce a derived unit here.
        assert!(apply(Op::Mul, &v("2V"), &v("3V")).is_err());
        assert!(apply(Op::Div, &v("2V"), &v("1A")).is_err());
        // Families do not meet.
        assert!(apply(Op::Add, &v("1V"), &v("1A")).is_err());
        assert!(satisfies(Compare::Eq, &v("1V"), &v("1A")).is_err());
        // Scaling by a unitless number keeps the family.
        let scaled = apply(Op::Mul, &v("3.3V"), &v("2")).unwrap();
        assert_eq!(scaled.number(), Some(6.6));
        assert_eq!(scaled.unit(), Some(&McUnit::Volt));
        let halved = apply(Op::Div, &v("6.6V"), &v("2")).unwrap();
        assert_eq!(halved.number(), Some(3.3));
        assert!(apply(Op::Div, &v("2"), &v("3.3V")).is_err());
    }

    #[test]
    fn eval__family_table_holds_the_ruled_exceptions() {
        // Temperature: a difference is defined, a sum is not.
        let span = apply(Op::Sub, &v("25°C"), &v("10°C")).unwrap();
        assert_eq!(span.number(), Some(15.0));
        assert_eq!(span.unit(), Some(&McUnit::Temp));
        assert!(apply(Op::Add, &v("25°C"), &v("10°C")).is_err());
        assert!(apply(Op::Mul, &v("25°C"), &v("2")).is_err());
        assert!(satisfies(Compare::Lt, &v("10°C"), &v("25°C")).unwrap());
        // Celsius and Kelvin are the same family, so they compare.
        assert!(satisfies(Compare::Eq, &v("0°C"), &v("273.15K")).unwrap());
        // Decibel and noise density: equality only.
        assert!(satisfies(Compare::Eq, &v("3dB"), &v("3dB")).unwrap());
        assert!(satisfies(Compare::Gt, &v("3dB"), &v("2dB")).is_err());
        assert!(apply(Op::Add, &v("3dB"), &v("3dB")).is_err());
        assert!(apply(Op::Add, &v("1nV/√Hz"), &v("1nV/√Hz")).is_err());
    }

    #[test]
    fn eval__quantity_reader_takes_the_family_from_the_question() {
        assert_eq!(quantity_in("3.3V", &McUnit::Volt), Some(3.3));
        assert_eq!(quantity_in("-15V", &McUnit::Volt), Some(-15.0));
        assert_eq!(quantity_in("500mV", &McUnit::Volt), Some(0.5));
        // The bare-number rail notation: a `::DC(5)` nominal means 5 V.
        assert_eq!(quantity_in("5", &McUnit::Volt), Some(5.0));
        assert_eq!(quantity_in("500mA", &McUnit::Amp), Some(0.5));
        // A foreign family, a window form and plain text do not decode.
        assert_eq!(quantity_in("5A", &McUnit::Volt), None);
        assert_eq!(quantity_in("5Hz", &McUnit::Volt), None);
        assert_eq!(quantity_in("2.5V~5.5V", &McUnit::Volt), None);
        assert_eq!(quantity_in("WIDE", &McUnit::Volt), None);
    }

    #[test]
    fn eval__ratio_reader_and_percent_reader_differ_on_a_bare_number() {
        assert_eq!(ratio_of("95%"), Some(0.95));
        assert_eq!(ratio_of("0.95"), Some(0.95));
        // A factor key's bare number is the factor itself.
        assert_eq!(ratio_of("95"), Some(95.0));
        assert_eq!(ratio_of("5V"), None);
        // A percent key's bare number is percent — the `%` is optional notation.
        assert_eq!(percent_of("5%"), Some(0.05));
        assert_eq!(percent_of("5"), Some(0.05));
        assert_eq!(percent_of("-5%").map(f64::abs), Some(0.05));
        assert_eq!(percent_of("5V"), None);
    }

    #[test]
    fn eval__hexadecimal_literals_are_integers() {
        assert_eq!(v("0x36").number(), Some(54.0));
        assert_eq!(v("0X36").number(), Some(54.0));
        assert!(satisfies(Compare::Eq, &v("0x36"), &v("54")).unwrap());
        assert_eq!(apply(Op::Add, &v("0x01"), &v("0x01")).unwrap().text(), "2");
    }

    #[test]
    fn eval__placeholder_participates_in_nothing() {
        assert!(apply(Op::Add, &v("_"), &v("1")).is_err());
        assert!(satisfies(Compare::Eq, &v("_"), &v("1")).is_err());
        assert!(satisfies(Compare::Eq, &v("_"), &v("_")).is_err());
    }

    #[test]
    fn eval__errors_carry_their_registered_codes() {
        assert_eq!(EvalError::DivideByZero.code(), 5413);
        assert_eq!(
            not_numeric("+", &v("1V"), &v("x")).code(),
            crate::errcodes::EVAL_OPERAND_NOT_NUMERIC
        );
        assert_eq!(
            EvalError::Overflow {
                op: "*".to_string(),
                lhs: "2".to_string(),
                rhs: "2".to_string(),
            }
            .code(),
            crate::errcodes::EVAL_OVERFLOW
        );
        assert!(!EvalError::DivideByZero.message().is_empty());
        assert!(not_numeric("+", &v("1V"), &v("x")).message().contains('+'));
    }
}
