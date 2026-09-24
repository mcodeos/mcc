// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The ratings gate (ratings-param-constraint-design.md §2.2/§3/§8).
//!
//! Two PostParse checks over the `ratings` clause (the reader is
//! [`crate::semantic::component::mc_ratings`]):
//!   * the def side — an entry key must name a constructor formal of its
//!     class (E5362), and a side word must sit in the closed `{low, high}`
//!     vocabulary (E5360, the attr-vocabulary family);
//!   * the instance side — every bound formal's actual (explicit argument or
//!     the declaration's default) must fall inside the declared interval
//!     (E5361, closed bounds). Unit prefixes normalise into base units
//!     (`2500mV` reads as `2.5V`); an actual whose unit family differs from
//!     a bound's is v1-unjudged rather than guessed at.
//!
//! Out-of-range is a Pass3 validation error (5xxx), not a bind failure: the
//! instance is created with its arguments - an error never blocks
//! diagnostic tells the author which value to move.

use super::{CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, ValidationCheck};
use crate::semantic::basic::mc_param::{McParamBindings, McParamValue};
use crate::semantic::basic::mc_uval::McUnit;
use crate::semantic::component::mc_ratings::{read_ratings, RatingsEntry, RatingsSide};

pub struct RatingsCheck;

impl ValidationCheck for RatingsCheck {
    fn name(&self) -> &'static str {
        "ratings"
    }
    fn phase(&self) -> CheckPhase {
        CheckPhase::PostParse
    }
    fn default_severity(&self) -> CheckSeverity {
        CheckSeverity::Error
    }

    fn run_post_parse(&self, acc: &mut CheckAccumulator) {
        check_ratings_keys(acc);
        check_ratings_bounds(acc);
    }
}

/// Def side: every entry key names a constructor formal; every side word sits
/// in the closed vocabulary.
fn check_ratings_keys(acc: &mut CheckAccumulator) {
    let comps = crate::definition_space().workspace_components();
    for (sn, comp) in comps.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        let comp_name = sn.ident.to_string();
        let formals: std::collections::HashSet<String> = comp
            .bind_params()
            .iter()
            .filter_map(|d| d.get_primary_name())
            .collect();
        for entry in read_ratings(&comp.attrs) {
            for word in &entry.unreadable {
                acc.push(CheckResult {
                    check_name: "ratings",
                    severity: CheckSeverity::Error,
                    uri: Some(uri.clone()),
                    span: entry.key_span.clone(),
                    message: format!(
                        "Ratings bound side '{word}' on '{param}' in component '{comp_name}' is outside the closed vocabulary [low, high].",
                        param = entry.param
                    ),
                    code: crate::errcodes::ATTR_VALUE_NOT_IN_VOCABULARY,
                });
            }
            if !formals.contains(&entry.param) {
                acc.push(CheckResult {
                    check_name: "ratings",
                    severity: CheckSeverity::Error,
                    uri: Some(uri.clone()),
                    span: entry.key_span.clone(),
                    message: format!(
                        "Ratings key '{param}' in component '{comp_name}' names no constructor parameter.",
                        param = entry.param
                    ),
                    code: crate::errcodes::RATING_KEY_NOT_A_PARAM,
                });
            }
        }
    }
}

/// Instance side: every bound formal's actual falls inside its declared
/// interval. Bind is tolerant — a formal left unbound reads its declaration's
/// default, and a call the binder already refused elsewhere does not mute the
/// defaults the declaration still carries.
fn check_ratings_bounds(acc: &mut CheckAccumulator) {
    let modules = crate::definition_space().workspace_modules();
    for (sn, m) in modules.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        for (inst_name, (_iotype, instance)) in m.insts.iter_with_iotype() {
            let crate::McInstance::Component(c2) = instance else {
                continue;
            };
            let entries = read_ratings(&c2.base.attrs);
            if entries.is_empty() {
                continue;
            }
            let class_name = c2.base.name.to_string();
            let declares = c2.base.bind_params();
            let bindings =
                McParamBindings::bind_tolerant(declares, &c2.base.attr_key_names(), &c2.params);
            let span = super::insts::instance_span(m, inst_name);
            for entry in &entries {
                let Some((value, unit, text)) = actual_of(&bindings, declares, &entry.param)
                else {
                    continue;
                };
                let Some(unit) = unit else {
                    // A unit-less actual (`res = 10`) against a unit-carrying
                    // bound is a family mismatch: unjudged in v1.
                    continue;
                };
                let Some(kind) = bound_violation(entry, value, &unit) else {
                    continue;
                };
                let bound_text = match kind {
                    Side::Low => entry.side_text(RatingsSide::Low).unwrap_or_default(),
                    Side::High => entry.side_text(RatingsSide::High).unwrap_or_default(),
                };
                acc.push(CheckResult {
                    check_name: "ratings",
                    severity: CheckSeverity::Error,
                    uri: Some(uri.clone()),
                    span: span.clone(),
                    message: format!(
                        "Instance '{inst_name}' of component '{class_name}': parameter '{param}' = {text} violates its ratings ({kind_word} {bound_text}). Set the value within the declared bounds or choose a part whose ratings admit it.",
                        param = entry.param,
                        kind_word = if kind == Side::Low { "low bound" } else { "high bound" },
                    ),
                    code: crate::errcodes::RATING_PARAM_OUT_OF_RANGE,
                });
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Low,
    High,
}

/// The first violated side of the closed interval, if any.
fn bound_violation(entry: &RatingsEntry, value: f64, unit: &McUnit) -> Option<Side> {
    for (side, bound) in &entry.sides {
        // Same unit family only: a volt against an ampere compares nothing.
        if bound.unit() != unit {
            continue;
        }
        let violated = match side {
            RatingsSide::Low => value < bound.value(),
            RatingsSide::High => value > bound.value(),
        };
        if violated {
            return Some(match side {
                RatingsSide::Low => Side::Low,
                RatingsSide::High => Side::High,
            });
        }
    }
    None
}

/// The actual a ratings entry must clear: the explicit argument when the slot
/// was claimed, otherwise the declaration's default. `None` when the value is
/// not statically readable (`_`, NC, a reference, an expression, a set).
fn actual_of(
    bindings: &McParamBindings,
    declares: &crate::semantic::basic::mc_param::McParamDeclares,
    param: &str,
) -> Option<(f64, Option<McUnit>, String)> {
    if let Some(binding) = bindings.find_by_name(param) {
        if !binding.is_default {
            return scalar_of(binding.value.as_ref());
        }
        return default_of(&binding.declare);
    }
    // The binder refused the call wholesale (empty bindings on a non-empty
    // call): the declaration's default still governs.
    let declare = declares
        .iter()
        .find(|d| d.get_primary_name().is_some_and(|p| p == param))?;
    default_of(declare)
}

fn scalar_of(value: Option<&McParamValue>) -> Option<(f64, Option<McUnit>, String)> {
    match value? {
        McParamValue::UValue(uval) => Some((uval.value(), Some(uval.unit().clone()), uval.to_string())),
        McParamValue::Int(i) => Some((i.value as f64, None, i.to_string())),
        McParamValue::Hex(h) => Some((h.value as f64, None, h.to_string())),
        McParamValue::Float(f) => Some((f.value, None, f.to_string())),
        _ => None,
    }
}

/// The declaration's default, read through the same text→value parse the
/// eval engine uses (`eval::Value::from_text`), so `3.3V` arrives as a
/// quantity and `_`/text stays unread.
fn default_of(declare: &crate::semantic::basic::mc_param::McParamDeclare) -> Option<(f64, Option<McUnit>, String)> {
    let text = declare.param_type.default_value()?;
    let value = crate::eval::Value::from_text(text);
    let number = value.number()?;
    Some((number, value.unit().cloned(), value.text()))
}
