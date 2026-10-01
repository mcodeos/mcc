// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The condition-axis self-consistency gate (U371 leg4,
//! `mcd/doc/eval/paired-value-design.md` §8).
//!
//! The first consumer of a paired value's condition half
//! (`McParamValue::at_condition()` / `McUnitValueAt::right`): the rows of one
//! key carry their `@` halves so a later working-point pass (ruling 20, case
//! c — default judge-all-rows, a declared working point tightens) can select
//! among them. One written shape defeats that selection before any pass runs:
//! rows of one key paired on **different unit families** (`@10km` next to
//! `@25℃`) — no single axis to select on (E5363).
//!
//! The warning, not an error: the canon has not ruled the envelope's shape
//! yet (G2), so a multi-axis reading stays open and the gate must not bake
//! the single-axis assumption in as a hard verdict. Everything else is
//! skipped silently, per the honest-default law (6062/E4124 precedent): a
//! single row, rows with no `@` half (an axis-free row matches any declared
//! axis — Pass C's axis-free supply), and values whose halves do not resolve
//! are data the gate does not guess about. Two rows **at the same axis
//! point** are equally skipped — the K form is a point *set* (membership,
//! not a function), so `25MHz@0.1m, 50MHz@0.1m, 100MHz@0.1m, 208MHz@0.1m`
//! (`sdio.mc`'s speed-grade ladder, same spelling in `usb.mc`/`dp.mc`) is
//! the corpus's own legitimate alternative-capability data; a first draft of
//! this gate warned on it and the corpus refuted the draft (U371 leg4,
//! withdrawal note in the design doc).
//!
//! Two faces are walked, the two places a paired value set can stand:
//!   * the attribute face — interface and component body rows
//!     (`maxspeed = [10kbps@10km, …]`, the corpus's dominant carrier); a
//!     nested table row is its own key and is judged on its own;
//!   * the parameter face — an instance argument set
//!     (`u1(p = [1A@5V, 2A@12V])`), read through the tolerant binder so a
//!     refused call elsewhere does not mute the rows that did bind.

use super::{CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, ValidationCheck};
use crate::semantic::basic::mc_expr::McExpression;
use crate::semantic::basic::mc_param::{McParamBindings, McParamValue};
use crate::semantic::basic::mc_uval::McUnit;
use crate::semantic::component::mc_attr::{McAttrVal, McAttribute};

pub struct CondAxisCheck;

impl ValidationCheck for CondAxisCheck {
    fn name(&self) -> &'static str {
        "cond_axis"
    }
    fn phase(&self) -> CheckPhase {
        CheckPhase::PostParse
    }
    fn default_severity(&self) -> CheckSeverity {
        CheckSeverity::Warning
    }

    fn run_post_parse(&self, acc: &mut CheckAccumulator) {
        check_attr_rows(acc);
        check_param_sets(acc);
    }
}

/// One key's condition halves, as collected from either face.
struct AxisRow {
    /// Where the diagnostic points (the key that owns the rows).
    span: Option<std::ops::Range<usize>>,
    uri: String,
    /// The key spelled as written (`maxspeed`, or the formal `p`).
    key: String,
    /// `(axis family, pair text)` per paired row.
    halves: Vec<(McUnit, String)>,
}

/// Attribute face: interface and component body rows.
fn check_attr_rows(acc: &mut CheckAccumulator) {
    let space = crate::definition_space();
    let ifaces = space.workspace_interfaces();
    let comps = space.workspace_components();
    for (sn, def) in ifaces.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        walk_attr_rows(def.attrs.iter(), String::new(), &uri, acc);
    }
    for (sn, def) in comps.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        walk_attr_rows(def.attrs.iter(), String::new(), &uri, acc);
    }
}

/// Judge each attribute row; a table value's inner rows are the row's own
/// keys (`spec` and `spec.row` are two keys, the G2 spelling law) and are
/// judged separately under their dotted name.
fn walk_attr_rows<'a>(
    rows: impl IntoIterator<Item = &'a McAttribute>,
    prefix: String,
    uri: &str,
    acc: &mut CheckAccumulator,
) {
    for attr in rows {
        let path = if prefix.is_empty() {
            attr.id.to_string()
        } else {
            format!("{prefix}.{}", attr.id)
        };
        let mut halves = Vec::new();
        let mut tables = Vec::new();
        for val in &attr.values {
            match val {
                McAttrVal::AttrExpr(expr) => collect_expr_halves(expr, &mut halves),
                McAttrVal::Attributes(inner) => tables.push(inner),
                _ => {}
            }
        }
        if halves.len() >= 2 {
            judge_row(
                acc,
                AxisRow {
                    span: attr.key_span.clone(),
                    uri: uri.to_string(),
                    key: path.clone(),
                    halves,
                },
            );
        }
        for inner in tables {
            walk_attr_rows(inner.iter(), path.clone(), uri, acc);
        }
    }
}

/// Walk one value expression collecting every `uv@uv` pair's condition half.
fn collect_expr_halves(expr: &McExpression, out: &mut Vec<(McUnit, String)>) {
    match expr {
        McExpression::UnitValueAt(at) => push_half(&at.right, &at.to_string(), out),
        McExpression::Set(items) => {
            for item in items {
                collect_expr_halves(item, out);
            }
        }
        McExpression::Plus(l, r)
        | McExpression::Minus(l, r)
        | McExpression::Multiply(l, r)
        | McExpression::Divide(l, r)
        | McExpression::Slice(l, r)
        | McExpression::Range(l, r) => {
            collect_expr_halves(l, out);
            collect_expr_halves(r, out);
        }
        McExpression::Call { args, .. } => {
            for arg in args {
                collect_expr_halves(arg, out);
            }
        }
        _ => {}
    }
}

fn push_half(
    cond: &crate::semantic::basic::mc_uval::McUnitValue,
    pair: &str,
    out: &mut Vec<(McUnit, String)>,
) {
    out.push((cond.unit().clone(), pair.to_string()));
}

/// Parameter face: instance argument sets, one key per bound formal. The
/// condition half is read through `McParamValue::at_condition()` — this gate
/// is that endpoint's first consumer (U371 leg4).
fn check_param_sets(acc: &mut CheckAccumulator) {
    let modules = crate::definition_space().workspace_modules();
    for (sn, m) in modules.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }
        for (inst_name, (_iotype, instance)) in m.insts.iter_with_iotype() {
            let span = super::insts::instance_span(m, inst_name);
            match instance {
                crate::McInstance::Component(c2) => {
                    let declares = c2.base.bind_params();
                    let bindings = McParamBindings::bind_tolerant(
                        declares,
                        &c2.base.attr_key_names(),
                        &c2.params,
                    );
                    for binding in bindings.iter() {
                        let Some(name) = binding.declare.get_primary_name() else {
                            continue;
                        };
                        judge_param_value(
                            acc,
                            &uri,
                            span.clone(),
                            inst_name,
                            &c2.base.name.to_string(),
                            &name,
                            binding.value.as_ref(),
                        );
                    }
                }
                crate::McInstance::Module(m2) => {
                    let Ok(bindings) = McParamBindings::bind(&m2.base.params, &m2.args) else {
                        // The call was refused at bind time and refused
                        // there; the rows it carried are not re-judged.
                        continue;
                    };
                    for binding in bindings.iter() {
                        let Some(name) = binding.declare.get_primary_name() else {
                            continue;
                        };
                        judge_param_value(
                            acc,
                            &uri,
                            span.clone(),
                            inst_name,
                            &m2.base.name.to_string(),
                            &name,
                            binding.value.as_ref(),
                        );
                    }
                }
                _ => {}
            }
        }
    }
}

fn judge_param_value(
    acc: &mut CheckAccumulator,
    uri: &str,
    span: Option<std::ops::Range<usize>>,
    inst_name: &str,
    class_name: &str,
    param: &str,
    value: Option<&McParamValue>,
) {
    let Some(McParamValue::Set(items)) = value else {
        return;
    };
    let mut halves = Vec::new();
    for item in items {
        if let Some(cond) = item.at_condition() {
            push_half(cond, &item.to_string(), &mut halves);
        }
    }
    if halves.len() >= 2 {
        judge_row(
            acc,
            AxisRow {
                span,
                uri: uri.to_string(),
                key: format!("{inst_name}({class_name}).{param}"),
                halves,
            },
        );
    }
}

/// The one verdict of a key's row set. A warning; it does not block.
fn judge_row(acc: &mut CheckAccumulator, row: AxisRow) {
    // E5363: the rows must share one axis family.
    let mut families: Vec<&McUnit> = Vec::new();
    for (unit, _) in &row.halves {
        if !families.contains(&unit) {
            families.push(unit);
        }
    }
    if families.len() > 1 {
        let list = families
            .iter()
            .map(|u| u.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        acc.push(CheckResult {
            check_name: "cond_axis",
            severity: CheckSeverity::Warning,
            uri: Some(row.uri.clone()),
            span: row.span.clone(),
            message: format!(
                "Key '{key}' pairs its values on more than one condition axis ({list}). The rows of one key share one axis — a working-point declaration selects rows by that axis, and mixed families leave no coherent selection.",
                key = row.key,
                list = list,
            ),
            code: crate::errcodes::COND_AXIS_MIXED,
        });
    }
}
