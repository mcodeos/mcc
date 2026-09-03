// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Capability-adoption (`::`) link diagnostics (abstract-variant-capability
//! plan §2.1 / §4.2 / §5).
//!
//! The declaration-relation ledgers (`adopts` / `effective_funcs`) are filled
//! silently at the top of every load round so dispatch never depends on this
//! check. These three diagnostics are the *user-facing* half of the same link
//! pass, attributed to the adopting component's file:
//!
//! - `ADOPTS_NON_CAPABILITY` — a `::` target that resolves to a live
//!   non-capability def (a component/module …), with the use-`:` hint.
//! - `CAPABILITY_SIGNAL_MISSING` — the adopting component does not declare a
//!   signal (name, or a group label + its member) the adopted capability
//!   funcs are written against, or declares it with an incompatible direction.
//! - `ADOPTED_FUNC_AMBIGUOUS` — two adopted capabilities expose the same func
//!   name and the component does not override it with its own func.
//! - `VARIANT_BASE_NON_ABSTRACT` — a `: Base` variant whose declared base
//!   resolves to a def that is not an abstract component (P4 §7.1), with the
//!   use-`::` hint.
//!
//! Pure analysis lives in [`crate::db::adoption`] (host analysis + the variant
//! base resolver); this check only maps its verdicts to diagnostics. It runs
//! over every *workspace* component that carries a derivation header (`adopts`
//! or a declared `variant_base`); the pass1 harness filters the results down
//! to re-derived files (post-parse sweep), so a clean file never accrues a
//! duplicate row.

use super::{CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, ValidationCheck};

use crate::db::adoption::analyze_host_adoption;
use crate::errcodes;

pub struct AdoptionCheck;

impl ValidationCheck for AdoptionCheck {
    fn name(&self) -> &'static str {
        "capability_adoption"
    }
    fn phase(&self) -> CheckPhase {
        CheckPhase::PostParse
    }
    fn default_severity(&self) -> CheckSeverity {
        CheckSeverity::Error
    }

    fn run_post_parse(&self, acc: &mut CheckAccumulator) {
        use crate::db::adoption::resolve_variant_base;
        use crate::db::adoption::VariantBaseTarget;
        let comps = crate::definition_space().workspace_components();
        for (sn, comp) in comps.iter() {
            let uri = sn.uri.to_string();
            if super::is_test_file(&uri) {
                continue;
            }
            if comp.adopts.is_empty() {
                // P4 §7.1 — a `: Base` variant whose declared base is not an
                // abstract component. The registry seam leaves the child
                // un-materialized for such a base, so this is the user-facing
                // half of that same link verdict.
                if let Some(base_name) = &comp.variant_base {
                    let host = comp.name.to_string();
                    let from_uri = crate::McURI::from(sn.uri_string().as_ref());
                    if matches!(
                        resolve_variant_base(&from_uri, base_name),
                        VariantBaseTarget::NonAbstract(_)
                    ) {
                        let msg =
                            errcodes::format_msg(errcodes::VARIANT_BASE_NON_ABSTRACT, &[&host]);
                        acc.push(CheckResult {
                            check_name: self.name(),
                            severity: CheckSeverity::Error,
                            uri: Some(uri.clone()),
                            span: None,
                            message: msg,
                            code: errcodes::VARIANT_BASE_NON_ABSTRACT,
                        });
                    }
                }
                continue;
            }
            let f = analyze_host_adoption(comp);
            let host = comp.name.to_string();

            for name in &f.non_capabilities {
                let msg = errcodes::format_msg(errcodes::ADOPTS_NON_CAPABILITY, &[&name]);
                acc.push(CheckResult {
                    check_name: self.name(),
                    severity: CheckSeverity::Error,
                    uri: Some(uri.clone()),
                    span: None,
                    message: msg,
                    code: errcodes::ADOPTS_NON_CAPABILITY,
                });
            }
            for func in &f.ambiguous_funcs {
                let msg = errcodes::format_msg(errcodes::ADOPTED_FUNC_AMBIGUOUS, &[&func]);
                acc.push(CheckResult {
                    check_name: self.name(),
                    severity: CheckSeverity::Error,
                    uri: Some(uri.clone()),
                    span: None,
                    message: msg,
                    code: errcodes::ADOPTED_FUNC_AMBIGUOUS,
                });
            }
            for ms in &f.missing_signals {
                let msg = errcodes::format_msg(
                    errcodes::CAPABILITY_SIGNAL_MISSING,
                    &[&host, &ms.form, &ms.hint],
                );
                acc.push(CheckResult {
                    check_name: self.name(),
                    severity: CheckSeverity::Error,
                    uri: Some(uri.clone()),
                    span: None,
                    message: msg,
                    code: errcodes::CAPABILITY_SIGNAL_MISSING,
                });
            }
        }
    }
}
