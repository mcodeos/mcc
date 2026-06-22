// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Conditional expression evaluation (Phase 3 enhanced)
//!
//! Supported syntax:
//!   - `param == value` / `param != value`    (equality comparison)
//!   - `param > value` / `param >= value`     (numeric comparison)
//!   - `param < value` / `param <= value`     (numeric comparison)
//!   - `cond1 && cond2`                       (logical AND)
//!   - `cond1 || cond2`                       (logical OR)
//!   - `!condition`                           (logical NOT)
//!   - `param`                                (truthiness check: non-empty && != "false" && != "NC")
//!   - `(condition)`                          (parentheses grouping)
//!
//! - `evaluate_condition`           —— top-level evaluation entry
//! - `evaluate_comparison`          —— comparison expression evaluation
//! - `evaluate_truthiness`          —— bare parameter name truthiness check
//! - `resolve_param_value`          —— parameter reference resolution (supports dotted paths)
//! - `extract_numeric_value`        —— McParamValue → f64
//! - `find_operator_outside_parens` —— find operator outside parentheses
//! - `is_matching_parens`           —— check whether outermost parentheses match

use super::McModuleInst;
use crate::core::basic::mc_literal::McConst;
use crate::core::basic::mc_param::McParamValue;

impl McModuleInst {
    /// Attempt to statically evaluate a condition expression
    ///
    /// Returns: Some(true/false) if evaluable, None if not evaluable
    #[allow(dead_code)]
    pub(super) fn evaluate_condition(&self, condition: &str) -> Option<bool> {
        let condition = condition.trim();

        if condition.is_empty() {
            return None;
        }

        // 1. Logical OR || (lowest priority, search right-to-left outside parens)
        if let Some(pos) = Self::find_operator_outside_parens(condition, "||") {
            let lhs = self.evaluate_condition(&condition[..pos])?;
            let rhs = self.evaluate_condition(&condition[pos + 2..])?;
            return Some(lhs || rhs);
        }

        // 2. Logical AND &&
        if let Some(pos) = Self::find_operator_outside_parens(condition, "&&") {
            let lhs = self.evaluate_condition(&condition[..pos])?;
            let rhs = self.evaluate_condition(&condition[pos + 2..])?;
            return Some(lhs && rhs);
        }

        // 3. Logical NOT !
        if condition.starts_with('!') {
            let inner = condition[1..].trim();
            // Handle both !(expr) and !name forms
            let inner = if inner.starts_with('(') && inner.ends_with(')') {
                if Self::is_matching_parens(inner) {
                    &inner[1..inner.len() - 1]
                } else {
                    inner
                }
            } else {
                inner
            };
            return self.evaluate_condition(inner).map(|v| !v);
        }

        // 4. Parentheses stripping (condition)
        if condition.starts_with('(')
            && condition.ends_with(')')
            && Self::is_matching_parens(condition)
        {
            return self.evaluate_condition(&condition[1..condition.len() - 1]);
        }

        // 5. Comparison operators (match long-to-short: >=, <=, !=, ==, >, <)
        //    Note: must match two-character operators first, to avoid ">=" being
        //    falsely matched by ">"
        let comparison_ops: &[(&str, usize)] = &[
            (">=", 2),
            ("<=", 2),
            ("!=", 2),
            ("==", 2),
            (">", 1),
            ("<", 1),
        ];

        for &(op, op_len) in comparison_ops {
            if let Some(pos) = Self::find_operator_outside_parens(condition, op) {
                let lhs_str = condition[..pos].trim();
                let rhs_str = condition[pos + op_len..].trim();
                let rhs_str = rhs_str.trim_matches(|c| c == '\'' || c == '"');

                return self.evaluate_comparison(lhs_str, op, rhs_str);
            }
        }

        // 6. Bare parameter name → truthiness check
        self.evaluate_truthiness(condition)
    }

    /// Evaluate a comparison expression
    ///
    /// Left operand is looked up in the parameter bindings, right operand is a literal
    fn evaluate_comparison(&self, lhs_name: &str, op: &str, rhs_literal: &str) -> Option<bool> {
        let lhs_value = self.resolve_param_value(lhs_name)?;
        let lhs_str = format!("{lhs_value}");
        let lhs_trimmed = lhs_str.trim();

        match op {
            "==" => Some(lhs_trimmed == rhs_literal),
            "!=" => Some(lhs_trimmed != rhs_literal),
            ">" | ">=" | "<" | "<=" => {
                // Try numeric comparison
                let lhs_num = Self::extract_numeric_value(lhs_value)?;
                let rhs_num = rhs_literal.parse::<f64>().ok()?;
                match op {
                    ">" => Some(lhs_num > rhs_num),
                    ">=" => Some(lhs_num >= rhs_num),
                    "<" => Some(lhs_num < rhs_num),
                    "<=" => Some(lhs_num <= rhs_num),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Resolve a parameter value reference
    ///
    /// Supports two forms:
    /// - Direct parameter name: "mode" → params.find("mode")
    /// - Dotted path: "dc.VCC" → member "VCC" of params.find("dc")
    fn resolve_param_value<'a>(&'a self, name: &str) -> Option<&'a McParamValue> {
        // 1. Direct parameter name match
        if let Some(binding) = self.params.find(name) {
            return binding.get_value();
        }

        // 2. Dotted path: "base.member"
        if let Some((base, _member)) = name.split_once('.') {
            if let Some(binding) = self.params.find(base) {
                return binding.get_value();
            }
        }

        None
    }

    /// Extract a numeric value from McParamValue (for numeric comparison)
    ///
    /// Supports Int, Float, UnitValue, and strings parseable as numbers
    fn extract_numeric_value(value: &McParamValue) -> Option<f64> {
        match value {
            McParamValue::Int(i) => Some(i.value as f64),
            McParamValue::Float(f) => Some(f.value),
            McParamValue::UValue(uv) => Some(uv.value()),
            McParamValue::Const(McConst::Keyword(s)) => s.parse::<f64>().ok(),
            McParamValue::Ids(ids) => {
                let s = ids.to_string();
                s.parse::<f64>().ok()
            }
            _ => {
                // Try parsing from the Display form
                let s = format!("{value}");
                s.trim().parse::<f64>().ok()
            }
        }
    }

    /// Truthiness check: parameter exists && non-empty && not false/NC/0/_
    ///
    /// Used for conditions of the form `if param { ... }`
    fn evaluate_truthiness(&self, name: &str) -> Option<bool> {
        let value = self.resolve_param_value(name)?;
        let s = format!("{value}");
        let s = s.trim();
        Some(
            !s.is_empty()
                && s != "false"
                && s != "False"
                && s != "FALSE"
                && s != "NC"
                && s != "0"
                && s != "_",
        )
    }

    /// Find the position of an operator outside of parentheses
    ///
    /// Skip content inside parens to avoid "||" in "(a || b) && c" being
    /// incorrectly matched. Search right-to-left to keep || and &&
    /// left-associative.
    fn find_operator_outside_parens(expr: &str, op: &str) -> Option<usize> {
        let bytes = expr.as_bytes();
        let op_bytes = op.as_bytes();
        let op_len = op_bytes.len();

        if bytes.len() < op_len {
            return None;
        }

        let mut depth: i32 = 0;
        // Search right-to-left
        let mut i = bytes.len() - op_len;
        loop {
            let ch = bytes[i];
            // Right-to-left: ')' increases depth, '(' decreases depth
            if ch == b')' {
                depth += 1;
            } else if ch == b'(' {
                depth -= 1;
            }

            if depth == 0 && &bytes[i..i + op_len] == op_bytes {
                // Extra check: avoid ">=" being falsely matched by ">"
                // (guaranteed by outer call order: match long operators first)
                return Some(i);
            }

            if i == 0 {
                break;
            }
            i -= 1;
        }
        None
    }

    /// Check whether the outermost parentheses match
    ///
    /// Used to distinguish "(a && b)" (overall parentheses) from "(a) && (b)" (non-overall parentheses)
    fn is_matching_parens(expr: &str) -> bool {
        if !expr.starts_with('(') || !expr.ends_with(')') {
            return false;
        }
        let mut depth = 0i32;
        for (i, ch) in expr.chars().enumerate() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    // If depth reaches zero before the last character,
                    // the parentheses are not the outermost wrapper
                    if depth == 0 && i < expr.len() - 1 {
                        return false;
                    }
                }
                _ => {}
            }
        }
        depth == 0
    }
}
