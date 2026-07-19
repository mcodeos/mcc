// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::ast::ast_node::AstNode;
use crate::core::basic::mc_expr::McExpression;
use crate::core::basic::mc_opd::McOpd;
use crate::core::common::IOType;
use crate::core::component::mc_attr::McAttrVal;
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
            // Bug 1 fix: Variable previously always returned true, causing pure identifiers like `I0::I2C`
            // to be incorrectly treated as parameter references, taking the dynamic pin path and skipping normal interface binding.
            // Now check if Variable truly contains square bracket parameter references inside (e.g., [1:rows]).
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
    /// Pure identifiers (`I0::I2C`, `XTAL`) are not param refs, should take the normal interface binding path.
    fn variable_has_param_ref(opd: &McOpd) -> bool {
        use crate::core::basic::mc_opd::McOpd;
        match opd {
            McOpd::Id(id) => {
                // Has square bracket param refs (e.g. [1:rows])
                if id.has_param_ref() {
                    return true;
                }
                // Single-segment plain identifiers (e.g. rows, cols) are likely param refs
                // Multi-segment identifiers (e.g. I0::I2C) are interface bindings, not param refs
                if id.segments.len() == 1 {
                    if let crate::core::basic::mc_ids::IdsSegment::Ida(ida) = &id.segments[0] {
                        // Only one Ida segment with no square brackets → plain identifier → param ref
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
        evaluated.evaluate_to_int()
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
        opd: &crate::core::basic::mc_opd::McOpd,
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

    /// Expand expression to string list (for pin names, e.g., R[1:rows]C[1:cols] -> R1C1, R1C2, ...)
    pub fn expand_with_bindings(&self, bindings: &[(String, i64)]) -> Vec<String> {
        match &self.expr {
            McExpression::Variable(opd) => {
                // For variables, try to use McIds::expand_with_bindings
                if let crate::core::basic::mc_opd::McOpd::Id(ids) = opd {
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
        substituted.evaluate_to_int()
    }
}

impl McExpression {
    pub fn evaluate_to_int(&self) -> Option<i64> {
        match self {
            McExpression::Int(int_val) => Some(int_val.value),
            McExpression::Plus(l, r) => Some(l.evaluate_to_int()? + r.evaluate_to_int()?),
            McExpression::Minus(l, r) => Some(l.evaluate_to_int()? - r.evaluate_to_int()?),
            McExpression::Multiply(l, r) => Some(l.evaluate_to_int()? * r.evaluate_to_int()?),
            McExpression::Divide(l, r) => {
                let divisor = r.evaluate_to_int()?;
                if divisor == 0 {
                    return None;
                }
                Some(l.evaluate_to_int()? / divisor)
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DynamicPinLine {
    pub iotype: IOType,
    pub pin_id_expr: Option<DynamicPinExpr>,
    pub pin_name_expr: Option<DynamicPinExpr>,
    pub values: Arc<Vec<McAttrVal>>,
}

impl DynamicPinLine {
    pub fn new() -> Self {
        Self {
            iotype: IOType::None,
            pin_id_expr: None,
            pin_name_expr: None,
            values: Arc::new(Vec::new()),
        }
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
        let mut results: Vec<(i64, String)> = Vec::new();

        let pin_ids: Vec<i64> = match &self.pin_id_expr {
            Some(expr) => {
                if let Some(ids) = expr.expand_range(bindings) {
                    ids
                } else {
                    return results;
                }
            }
            None => return results,
        };

        let pin_names: Vec<String> = match &self.pin_name_expr {
            Some(expr) => {
                // For Variable expressions (e.g. R[1:rows]C[1:cols]), use expand_with_bindings
                // which handles string Cartesian product expansion with parameter substitution.
                // For numeric expressions, use expand_range.
                if matches!(&expr.expr, McExpression::Variable(_)) {
                    expr.expand_with_bindings(bindings)
                } else if let Some(names) = expr.expand_range(bindings) {
                    names.iter().map(|v| v.to_string()).collect()
                } else {
                    return results;
                }
            }
            None => {
                return pin_ids.iter().map(|id| (*id, String::new())).collect();
            }
        };

        for (i, pin_id) in pin_ids.iter().enumerate() {
            let pin_name = pin_names.get(i).cloned().unwrap_or_default();
            results.push((*pin_id, pin_name));
        }

        results
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
