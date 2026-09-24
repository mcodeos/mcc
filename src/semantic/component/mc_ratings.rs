// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The `ratings` clause reader (ratings-param-constraint-design.md §2/§8).
//!
//! `ratings` is a component body clause written in the generic attribute
//! shape (`ratings = [ vin:[low:0V, high:30V], vout:[low:0.8V] ]`, b3974's
//! v1 canon: one line, colon-KVS entries, bounds reuse the `low:`/`high:`
//! vocabulary the pin/interface faces already write). Parsed, one entry is
//! a slice expression (`vin:[...]`) whose key is the bounded parameter and
//! whose value is a set of side slices (`low:[0V]`, `high:[30V]`) — the
//! reader walks that expression shape (AST-driven, no text re-parse) and
//! answers, per entry, which constructor parameter it bounds and what the
//! sides hold.
//!
//! Side words outside the closed `{low, high}` set are carried back as
//! `unreadable` rather than dropped: a typo'd side (`lo:`) must not widen
//! the bound by silence - a silent no-op is a defect, not a tolerance. A
//! *known* side whose value is not a
//! unit literal (a range `0V ~ 0.8V`, a parameter reference, an arithmetic
//! expression) is v1-unjudged and silently skipped — the bound evaluates
//! only where it is a constant.

use std::ops::Range;

use crate::semantic::basic::mc_expr::McExpression;
use crate::semantic::basic::mc_uval::McUnitValue;
use crate::semantic::component::mc_attr::{McAttrVal, McAttributes};

/// Which side of the interval one bound word states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RatingsSide {
    Low,
    High,
}

/// One `ratings` entry: the constructor parameter it names and the sides
/// actually readable as unit constants.
pub(crate) struct RatingsEntry {
    /// The entry key, compared exactly against the class's constructor
    /// formals (`bind_params()`).
    pub param: String,
    /// The readable sides, in written order.
    pub sides: Vec<(RatingsSide, McUnitValue)>,
    /// Side words outside the closed `{low, high}` set, as written.
    pub unreadable: Vec<String>,
    /// Span of the `ratings` key the entry was read from (the diagnostic
    /// anchor for the def-side checks; per-entry spans are not retained).
    pub key_span: Option<Range<usize>>,
}

impl RatingsEntry {
    /// The bound word as written (`low:0V`), for the diagnostic text.
    pub fn side_text(&self, side: RatingsSide) -> Option<String> {
        self.sides
            .iter()
            .find(|(s, _)| *s == side)
            .map(|(_, v)| v.to_string())
    }
}

/// Read every `ratings` clause of one definition's attribute list.
pub(crate) fn read_ratings(attrs: &McAttributes) -> Vec<RatingsEntry> {
    let mut entries = Vec::new();
    for attr in attrs.iter() {
        if attr.id.segments.len() != 1 || attr.id.to_string() != "ratings" {
            continue;
        }
        for val in &attr.values {
            let McAttrVal::AttrExpr(McExpression::Set(members)) = val else {
                // A member that is not the entry-set shape names no parameter
                // and is v1-silent (noted in the batch ledger).
                continue;
            };
            for member in members {
                entries.push(entry(member, &attr.key_span));
            }
        }
    }
    entries
}

fn entry(expr: &McExpression, key_span: &Option<Range<usize>>) -> RatingsEntry {
    let mut sides = Vec::new();
    let mut unreadable = Vec::new();
    // `vin:[low:0V, high:30V]` — the key slice and the set of side slices.
    let param = match expr {
        McExpression::Slice(key, _) => expr_name(key),
        _ => None,
    };
    if let McExpression::Slice(_, value) = expr {
        collect_sides(value, &mut sides, &mut unreadable);
    }
    RatingsEntry {
        param: param.unwrap_or_default(),
        sides,
        unreadable,
        key_span: key_span.clone(),
    }
}

/// The identifier a slice key carries (`vin`), when it carries one.
fn expr_name(expr: &McExpression) -> Option<String> {
    match expr {
        McExpression::Variable(opd) => Some(opd.to_string()),
        _ => None,
    }
}

fn collect_sides(
    value: &McExpression,
    sides: &mut Vec<(RatingsSide, McUnitValue)>,
    unreadable: &mut Vec<String>,
) {
    let members: Vec<&McExpression> = match value {
        McExpression::Set(list) => list.iter().collect(),
        other => vec![other],
    };
    for member in members {
        // `low:[0V]` — a side slice whose set holds exactly one unit literal.
        let McExpression::Slice(key, value) = member else {
            continue;
        };
        let Some(word) = expr_name(key) else {
            continue;
        };
        let side = match word.as_str() {
            "low" => RatingsSide::Low,
            "high" => RatingsSide::High,
            _ => {
                unreadable.push(word);
                continue;
            }
        };
        if let Some(uval) = unit_value(value) {
            sides.push((side, uval));
        }
    }
}

/// The unit constant a side holds, when it holds exactly one.
/// `low:[0V]` is a set of one `UnitValue`; a range (`0V ~ 0.8V`), a
/// reference or any other shape stays `None`.
fn unit_value(value: &McExpression) -> Option<McUnitValue> {
    match value {
        McExpression::Set(list) if list.len() == 1 => match &list[0] {
            McExpression::UnitValue(uval) => Some(uval.clone()),
            _ => None,
        },
        McExpression::UnitValue(uval) => Some(uval.clone()),
        _ => None,
    }
}
