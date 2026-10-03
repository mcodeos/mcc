// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! E3192 (FUNC_SHAPE_DRIFT): same-name funcs on different components declare
//! different parameter shapes. The shape (scalar slot vs Set/index formal vs
//! value/role slot) is the call-site spelling contract — scalar and Set forms
//! never auto-fold into each other (matching-rules-design.md B table) — so a
//! consumer of `flash.Power` cannot carry the spelling to `tc275.Power` when
//! the shapes drift. Warning, not error: the drift may be a legitimate domain
//! difference (an MCU's 7-domain `Power` vs a flash's single-domain one).
//! Adjudication: doc/ee/func-param-shape-design.md (survey log
//! log/10.3.func-param-shape-survey.md; corpus exemplars `Protect` in
//! mcode/comp/dio.mc — ESD Set vs TVS scalar — and `Power` mclibs Set vs
//! tc275 scalar×7).

use super::{CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, ValidationCheck};
use crate::semantic::basic::mc_paramd::McParamDeclareKind;
use crate::semantic::basic::mc_paramd::McParamDeclares;
use std::collections::HashMap;

pub struct FuncShapeDriftCheck;

/// One token per formal: `net` = scalar network slot, `setN` = Set/index
/// formal with N members (one argument position takes the whole bundle),
/// `val` = value / role / enum-class slot (not a network quantifier).
fn shape_token(kind: &McParamDeclareKind) -> &'static str {
    match kind {
        McParamDeclareKind::Single(_) => "net",
        McParamDeclareKind::Multiple(_) => "set",
        McParamDeclareKind::Role { .. } | McParamDeclareKind::UValue(_) => "val",
        // Enum-class formals classify the call's vocabulary, not its shape —
        // grouped with value slots so vocabulary-only drift stays silent.
        McParamDeclareKind::EnumClass(_) => "val",
    }
}

/// Shape signature of a func declaration, e.g. `net,net` vs `set,set`.
fn shape_sig(params: &McParamDeclares) -> String {
    params
        .iter()
        .map(|p| shape_token(&p.kind).to_string())
        .collect::<Vec<_>>()
        .join(",")
}

impl ValidationCheck for FuncShapeDriftCheck {
    fn name(&self) -> &'static str {
        "funcshape"
    }
    fn phase(&self) -> CheckPhase {
        CheckPhase::PostParse
    }
    fn default_severity(&self) -> CheckSeverity {
        CheckSeverity::Warning
    }

    fn run_post_parse(&self, acc: &mut CheckAccumulator) {
        // func name -> [(component display name, uri, span, shape sig)]
        let mut groups: HashMap<String, Vec<(String, String, Option<std::ops::Range<usize>>, String)>> =
            HashMap::new();
        for (sn, comp) in crate::definition_space().workspace_components() {
            let uri = sn.uri.to_string();
            if super::is_test_file(&uri) {
                continue;
            }
            let comp_name = comp.name.to_string();
            for func in comp.funcs.iter() {
                let fname = func.name.to_string();
                // Constructor funcs share the component's own base name — a
                // per-component construction arity by design, never a
                // cross-component convention.
                let base = comp_name.rsplit('.').next().unwrap_or("");
                if fname == base {
                    continue;
                }
                groups
                    .entry(fname)
                    .or_default()
                    .push((comp_name.clone(), uri.clone(), func.span.clone(), shape_sig(&func.params)));
            }
        }

        for (fname, decls) in groups {
            if decls.len() < 2 {
                continue;
            }
            // Distinct shapes only; the majority shape is the reference
            // (ties → lexicographically smallest) so a lone odd-one-out is
            // flagged once instead of flooding every agreeable declaration.
            let mut counts: HashMap<&str, usize> = HashMap::new();
            for (_, _, _, sig) in &decls {
                *counts.entry(sig.as_str()).or_default() += 1;
            }
            if counts.len() < 2 {
                continue;
            }
            let reference = counts
                .iter()
                .min_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)))
                .map(|(s, _)| *s)
                .unwrap();
            for (comp_name, uri, span, sig) in &decls {
                if sig == reference {
                    continue;
                }
                // Point at one conforming declaration for the message.
                let exemplar = decls
                    .iter()
                    .find(|(_, _, _, s)| s == reference)
                    .expect("reference shape exists in group");
                acc.push(CheckResult {
                    check_name: "funcshape",
                    severity: CheckSeverity::Warning,
                    uri: Some(uri.clone()),
                    span: span.clone(),
                    message: format!(
                        "func '{}' is declared with parameter shape ({}) on '{}', but ({}) on '{}' — same-name funcs should keep one shape (the shape is the call-site spelling contract; see func-param-shape-design.md)",
                        fname, sig, comp_name, exemplar.3, exemplar.0
                    ),
                    code: crate::errcodes::FUNC_SHAPE_DRIFT,
                });
            }
        }
    }
}
