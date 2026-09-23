// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::ast::node::AstNode;
use crate::eval;
use crate::semantic::basic::mc_expr::McExpression;
use crate::semantic::basic::mc_opd::McOpd;
use crate::semantic::common::IOType;
use crate::semantic::component::mc_attr::{McAttrVal, McAttributes};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct DynamicPinExpr {
    pub expr: McExpression,
    pub has_param_ref: bool,
}

impl DynamicPinExpr {
    pub fn from_ast(node: &AstNode) -> Option<Self> {
        let expr = McExpression::new(node)?;
        let has_param_ref = Self::check_param_ref(&expr);
        Some(Self {
            expr,
            has_param_ref,
        })
    }

    pub fn check_param_ref(expr: &McExpression) -> bool {
        match expr {
            // A `Variable` is a parameter reference only when it really contains
            // square-bracket parameter references inside (e.g., `[1:rows]`).
            // Treating every pure identifier as one (e.g. `I0::I2C`) takes the
            // dynamic pin path and skips normal interface binding.
            McExpression::Variable(opd) => Self::variable_has_param_ref(opd),
            McExpression::Plus(l, r) => Self::check_param_ref(l) || Self::check_param_ref(r),
            McExpression::Minus(l, r) => Self::check_param_ref(l) || Self::check_param_ref(r),
            McExpression::Multiply(l, r) => Self::check_param_ref(l) || Self::check_param_ref(r),
            McExpression::Divide(l, r) => Self::check_param_ref(l) || Self::check_param_ref(r),
            McExpression::Slice(l, r) => Self::check_param_ref(l) || Self::check_param_ref(r),
            McExpression::Range(l, r) => Self::check_param_ref(l) || Self::check_param_ref(r),
            _ => false,
        }
    }

    /// Check if McOpd truly contains parameter references (e.g., `[1:rows]`).
    /// Pure identifiers (`I0::I2C`, `XTAL`) are not param refs, should take the normal interface
    /// binding path.
    fn variable_has_param_ref(opd: &McOpd) -> bool {
        use crate::semantic::basic::mc_opd::McOpd;
        match opd {
            McOpd::Id(id) => {
                // Has square bracket param refs (e.g. [1:rows])
                if id.has_param_ref() {
                    return true;
                }
                // Single-segment plain identifiers (e.g. rows, cols) are likely param refs
                // Multi-segment identifiers (e.g. I0::I2C) are interface bindings, not param refs
                if id.segments.len() == 1 {
                    if let crate::semantic::basic::mc_ids::IdsSegment::Ida(ida) = &id.segments[0] {
                        // Only one Ida segment with no square brackets → plain identifier → param
                        // ref
                        if !ida.has_square() && !ida.is_empty() {
                            return true;
                        }
                    }
                }
                false
            }
            McOpd::This(t) => t.has_param_ref(),
            McOpd::Pins(p) => p.has_param_ref(),
            McOpd::Uscore => false,
        }
    }

    pub fn evaluate_with_bindings(&self, bindings: &[(String, i64)]) -> Option<i64> {
        let evaluated = self.substitute_params(bindings);
        evaluated.eval_int().ok()
    }

    fn substitute_params(&self, bindings: &[(String, i64)]) -> McExpression {
        self.substitute_recursive(&self.expr, bindings)
    }

    fn substitute_recursive(
        &self,
        expr: &McExpression,
        bindings: &[(String, i64)],
    ) -> McExpression {
        match expr {
            McExpression::Variable(opd) => {
                if let Some(val) = self.resolve_binding(opd, bindings) {
                    McExpression::Int(crate::McInt { value: val })
                } else {
                    expr.clone()
                }
            }
            McExpression::Plus(l, r) => McExpression::Plus(
                Box::new(self.substitute_recursive(l, bindings)),
                Box::new(self.substitute_recursive(r, bindings)),
            ),
            McExpression::Minus(l, r) => McExpression::Minus(
                Box::new(self.substitute_recursive(l, bindings)),
                Box::new(self.substitute_recursive(r, bindings)),
            ),
            McExpression::Multiply(l, r) => McExpression::Multiply(
                Box::new(self.substitute_recursive(l, bindings)),
                Box::new(self.substitute_recursive(r, bindings)),
            ),
            McExpression::Divide(l, r) => McExpression::Divide(
                Box::new(self.substitute_recursive(l, bindings)),
                Box::new(self.substitute_recursive(r, bindings)),
            ),
            McExpression::Slice(l, r) => McExpression::Slice(
                Box::new(self.substitute_recursive(l, bindings)),
                Box::new(self.substitute_recursive(r, bindings)),
            ),
            McExpression::Range(l, r) => McExpression::Range(
                Box::new(self.substitute_recursive(l, bindings)),
                Box::new(self.substitute_recursive(r, bindings)),
            ),
            _ => expr.clone(),
        }
    }

    fn resolve_binding(
        &self,
        opd: &crate::semantic::basic::mc_opd::McOpd,
        bindings: &[(String, i64)],
    ) -> Option<i64> {
        let names = opd.expand();
        if names.len() == 1 {
            let name = &names[0];
            bindings.iter().find(|(n, _)| n == name).map(|(_, v)| *v)
        } else {
            None
        }
    }

    pub fn expand_range(&self, bindings: &[(String, i64)]) -> Option<Vec<i64>> {
        match &self.expr {
            McExpression::Slice(left, right) => {
                let start = self.substitute_and_eval(left, bindings)?;
                let end = self.substitute_and_eval(right, bindings)?;
                if start <= end {
                    Some((start..=end).collect())
                } else {
                    Some((end..=start).rev().collect())
                }
            }
            _ => {
                let val = self.evaluate_with_bindings(bindings)?;
                Some(vec![val])
            }
        }
    }

    /// Expand expression to string list (for pin names, e.g., R[1:rows]C[1:cols] -> R1C1, R1C2,
    /// ...)
    pub fn expand_with_bindings(&self, bindings: &[(String, i64)]) -> Vec<String> {
        match &self.expr {
            McExpression::Variable(opd) => {
                // For variables, try to use McIds::expand_with_bindings
                if let crate::semantic::basic::mc_opd::McOpd::Id(ids) = opd {
                    return ids.expand_with_bindings(bindings);
                }
                // If not Id type, fall back to default expand
                self.expr.expand()
            }
            _ => self.expr.expand(),
        }
    }

    fn substitute_and_eval(&self, expr: &McExpression, bindings: &[(String, i64)]) -> Option<i64> {
        let substituted = self.substitute_recursive(expr, bindings);
        substituted.eval_int().ok()
    }

    /// Evaluate an expression to text on the value engine (doc/eval V2/V7):
    /// `Str + value` interpolates, a quantity echoes the author's notation
    /// (`"VCC" + volt` with `volt = 3.3V` reads `VCC3.3V`). `None` when a
    /// variable is not bound in `values` or an operation fails — U211, the
    /// name-slot door for exactly this shape.
    pub fn eval_text(expr: &McExpression, values: &[(String, String)]) -> Option<String> {
        eval_value(expr, values).map(|v| v.text())
    }
}

/// Recursive value-engine evaluation backing [`DynamicPinExpr::eval_text`].
fn eval_value(expr: &McExpression, values: &[(String, String)]) -> Option<eval::Value> {
    match expr {
        McExpression::Int(i) => Some(eval::Value::Int(i.value)),
        McExpression::Float(f) => Some(eval::Value::Float(f.value)),
        McExpression::String(s) => Some(eval::Value::Str(s.value.clone())),
        McExpression::UnitValue(u) => Some(eval::Value::Quantity(u.clone())),
        McExpression::Variable(opd) => {
            let names = opd.expand();
            if names.len() != 1 {
                return None;
            }
            let text = values
                .iter()
                .find(|(n, _)| n == &names[0])
                .map(|(_, v)| v.clone())?;
            // The bound text is re-read as a value, so a quantity argument
            // (`3.3V`) carries its family into the operation, not just digits.
            Some(eval::Value::from_text(&text))
        }
        McExpression::Plus(l, r) => {
            eval::apply(eval::Op::Add, &eval_value(l, values)?, &eval_value(r, values)?).ok()
        }
        McExpression::Minus(l, r) => {
            eval::apply(eval::Op::Sub, &eval_value(l, values)?, &eval_value(r, values)?).ok()
        }
        McExpression::Multiply(l, r) => {
            eval::apply(eval::Op::Mul, &eval_value(l, values)?, &eval_value(r, values)?).ok()
        }
        McExpression::Divide(l, r) => {
            eval::apply(eval::Op::Div, &eval_value(l, values)?, &eval_value(r, values)?).ok()
        }
        // Ranges/sets are pin-id shapes, not names; a const has no value here.
        _ => None,
    }
}

/// Why a dynamic row materialized nothing — U211: the two causes the old
/// `resolve` folded into one empty Vec are distinguishable here, so the
/// caller can report an unresolvable row instead of dropping it silently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DynPinFail {
    /// The id expression reads a parameter that is not bound to an integer
    /// here.
    IdExpr,
    /// The name expression reads a parameter that is not bound here, or
    /// mixes operands the value engine refuses.
    NameExpr,
}

#[derive(Debug, Clone)]
pub struct DynamicPinLine {
    pub iotype: IOType,
    pub pin_id_expr: Option<DynamicPinExpr>,
    pub pin_name_expr: Option<DynamicPinExpr>,
    pub values: Arc<Vec<McAttrVal>>,
    /// Row-level identity words (`@class/@noise/…`, intent-design.md
    /// §5.1) trail a *bank* declaration like `out [1:N] = D[1:N]`. The bank is
    /// only materialized into concrete pins per instantiation, so the words
    /// ride the line itself — mirroring how a static row's words ride each
    /// registered McPin's `attrs` (see `attach_row_attrs`). Never dropped.
    pub attrs: McAttributes,
    /// The `pins.<group>` block this line sits in, if any. Rides the line for the
    /// same reason the words above do: the bank's pins exist only per
    /// instantiation, so at parse time there is no pin to attach the block to.
    pub group: Option<String>,
}

impl DynamicPinLine {
    pub fn new() -> Self {
        Self {
            iotype: IOType::None,
            pin_id_expr: None,
            pin_name_expr: None,
            values: Arc::new(Vec::new()),
            attrs: McAttributes::new(),
            group: None,
        }
    }

    pub fn with_group(mut self, group: Option<String>) -> Self {
        self.group = group;
        self
    }

    pub fn with_attrs(mut self, attrs: McAttributes) -> Self {
        self.attrs = attrs;
        self
    }

    pub fn with_iotype(mut self, iotype: IOType) -> Self {
        self.iotype = iotype;
        self
    }

    pub fn with_values(mut self, values: Vec<McAttrVal>) -> Self {
        self.values = Arc::new(values);
        self
    }

    pub fn with_pin_id(mut self, expr: DynamicPinExpr) -> Self {
        self.pin_id_expr = Some(expr);
        self
    }

    pub fn with_pin_name(mut self, expr: DynamicPinExpr) -> Self {
        self.pin_name_expr = Some(expr);
        self
    }

    pub fn resolve(&self, bindings: &[(String, i64)]) -> Vec<(i64, String)> {
        self.resolve_checked(bindings, &[]).unwrap_or_default()
    }

    /// [`Self::resolve`] with the failure mode made visible, plus the
    /// value-engine name path: a text-shaped name (`"VCC" + volt`, U211)
    /// resolves against the string parameter bindings `values` — the integer
    /// path below cannot read a quantity parameter at all. The quiet `resolve`
    /// keeps the pre-U211 behavior for def-level id lookups.
    pub fn resolve_checked(
        &self,
        bindings: &[(String, i64)],
        values: &[(String, String)],
    ) -> Result<Vec<(i64, String)>, DynPinFail> {
        let pin_ids: Vec<i64> = match &self.pin_id_expr {
            Some(expr) => expr.expand_range(bindings).ok_or(DynPinFail::IdExpr)?,
            None => return Ok(Vec::new()),
        };

        let pin_names: Vec<String> = match &self.pin_name_expr {
            Some(expr) => {
                // Text evaluation first: it is the only path that reads a
                // quantity parameter (`volt = 3.3V`), and for it to answer at
                // all every variable the expression reads must be bound.
                if let Some(text) = DynamicPinExpr::eval_text(&expr.expr, values) {
                    vec![text]
                    // For Variable expressions (e.g. R[1:rows]C[1:cols]), use expand_with_bindings
                    // which handles string Cartesian product expansion with parameter substitution.
                    // For numeric expressions, use expand_range.
                } else if matches!(&expr.expr, McExpression::Variable(_)) {
                    expr.expand_with_bindings(bindings)
                } else if let Some(names) = expr.expand_range(bindings) {
                    names.iter().map(|v| v.to_string()).collect()
                } else {
                    return Err(DynPinFail::NameExpr);
                }
            }
            None => {
                return Ok(pin_ids.iter().map(|id| (*id, String::new())).collect());
            }
        };

        let mut results: Vec<(i64, String)> = Vec::new();
        for (i, pin_id) in pin_ids.iter().enumerate() {
            let pin_name = pin_names.get(i).cloned().unwrap_or_default();
            results.push((*pin_id, pin_name));
        }

        Ok(results)
    }

    /// Number of pins this dynamic line materializes under `bindings`, or
    /// `None` when its id range expression references a param that isn't bound
    /// to an integer here — the count is unknowable at this stage.
    ///
    /// This is the *count* counterpart of [`Self::resolve`] (which returns an
    /// empty Vec both for a legitimately empty range and for an unbound param,
    /// so it can't answer "how many pins would this line create?").
    pub fn dynamic_pin_count(&self, bindings: &[(String, i64)]) -> Option<usize> {
        match &self.pin_id_expr {
            Some(expr) => Some(expr.expand_range(bindings)?.len()),
            None => Some(0),
        }
    }

    pub fn has_param_refs(&self) -> bool {
        self.pin_id_expr
            .as_ref()
            .map(|e| e.has_param_ref)
            .unwrap_or(false)
            || self
                .pin_name_expr
                .as_ref()
                .map(|e| e.has_param_ref)
                .unwrap_or(false)
    }
}

impl Default for DynamicPinLine {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for DynamicPinExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.expr)
    }
}

impl std::fmt::Display for DynamicPinLine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(pin_id) = &self.pin_id_expr {
            if let Some(pin_name) = &self.pin_name_expr {
                write!(f, "{pin_id} = {pin_name}")
            } else {
                write!(f, "{pin_id}")
            }
        } else {
            write!(f, "?")
        }
    }
}
