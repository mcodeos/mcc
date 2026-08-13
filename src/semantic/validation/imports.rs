// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Import / Use declaration validation.
//!
//! Checks:
//!   K1 — self-import (file imports itself)
//!   K2 — `as` alias collision with existing names
//!   K3 — version string referencing non-existent version
//!   K4 — colon import of non-exported / non-existent symbol
//!   K5 — `pub use` of private / non-existent symbol

use super::{CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, ValidationCheck};
use std::collections::HashSet;

pub struct ImportsCheck;

impl ValidationCheck for ImportsCheck {
    fn name(&self) -> &'static str {
        "imports"
    }
    fn phase(&self) -> CheckPhase {
        CheckPhase::PostParse
    }
    fn default_severity(&self) -> CheckSeverity {
        CheckSeverity::Warning
    }

    fn run_post_parse(&self, acc: &mut CheckAccumulator) {
        let mcodes = &crate::db::cmie::tables::WORKSPACE.mcodes;

        // Build URI → module span map for source locations
        let uri_spans: std::collections::HashMap<String, std::ops::Range<usize>> = {
            let mut m = std::collections::HashMap::new();
            let modules = &crate::db::cmie::tables::WORKSPACE.modules;
            for e in modules.iter() {
                let mod_span = e.value().span.clone();
                m.insert(e.key().uri.to_string(), mod_span.start..mod_span.end);
            }
            m
        };

        // Build the set of all loaded URIs for K3 version checks
        let all_uris: HashSet<String> = mcodes.iter().map(|e| e.key().clone()).collect();

        // Build the set of all known CMIE names across all tables for K2 alias check
        let all_cmie_names = build_cmie_name_set();

        for entry in mcodes.iter() {
            let self_uri = entry.key();
            let mcode = entry.value();

            for mcu in &mcode.uselist {
                // ★ Fix (Defect 76): Canonicalize the use-target URI so it matches
                // DashMap keys. Without this, relative paths like "./power.mc"
                // would miss in mcodes.get() and produce false "target file not
                // loaded" / "symbol not found" errors (K4/K5 false positives).
                let target_uri = crate::build::pass1::canonicalize_project_uri(&mcu.uri);

                // ── K1: Self-import ──
                if !super::is_test_file(self_uri) && target_uri == *self_uri {
                    acc.push(CheckResult {
                        check_name: "imports",
                        severity: CheckSeverity::Warning,
                        uri: Some(self_uri.clone()),
                        span: uri_spans.get(self_uri).cloned(),
                        message: format!("File imports itself via 'use {}'.", target_uri),
                        code: crate::errcodes::USE_SELF_IMPORT,
                    });
                }

                // ── K2: `as` alias collision ──
                // Only check against registered CMIE names (all_cmie_names), not
                // against local spacenames (local_names), because the spacename
                // entry was just created by this same `use as` statement, which
                // would produce a false "collides with itself" diagnostic.
                if let Some(ref alias) = mcu.as_id {
                    if all_cmie_names.contains(alias.as_str()) {
                        acc.push(CheckResult {
                            check_name: "imports",
                            severity: CheckSeverity::Error,
                            uri: Some(self_uri.clone()),
                            span: uri_spans.get(self_uri).cloned(),
                            message: format!(
                                "'use {} as {}' — alias '{}' collides with an existing name.",
                                mcu.uri, alias, alias
                            ),
                            code: crate::errcodes::USE_ALIAS_COLLISION,
                        });
                    }
                }

                // ── K3: Non-existent version ──
                if mcu.version.is_some() && !all_uris.contains(&target_uri) {
                    acc.push(CheckResult {
                        check_name: "imports",
                        severity: CheckSeverity::Error,
                        uri: Some(self_uri.clone()),
                        span: uri_spans.get(self_uri).cloned(),
                        message: format!(
                            "'use {}' — versioned file not found. The target may not exist.",
                            mcu.uri
                        ),
                        code: crate::errcodes::USE_VERSIONED_TARGET_NOT_FOUND,
                    });
                }

                // ── K4: Non-exported/non-existent symbol import ──
                if let Some(ref impt_ids) = mcu.impt_ids {
                    // Look up the target file's spacenames to verify symbols exist
                    if let Some(target_mcode) = mcodes.get(&target_uri) {
                        let target_spacenames = &target_mcode.spacenames;
                        for id in impt_ids {
                            let id_str = id.to_string();
                            if !target_spacenames
                                .iter()
                                .any(|(k, _)| k.to_string() == id_str)
                            {
                                acc.push(CheckResult {
                                    check_name: "imports",
                                    severity: CheckSeverity::Error,
                                    uri: Some(self_uri.clone()),
                                    span: uri_spans.get(self_uri).cloned(),
                                    message: format!(
                                        "'use {} import({})' — symbol '{}' not found in target file.",
                                        mcu.uri, id_str, id_str
                                    ),
                                    code: crate::errcodes::USE_IMPORT_SYMBOL_NOT_FOUND,
                                });
                            }
                        }
                    } else {
                        // Target file not loaded — report as K4
                        for id in impt_ids {
                            let id_str = id.to_string();
                            acc.push(CheckResult {
                                check_name: "imports",
                                severity: CheckSeverity::Error,
                                uri: Some(self_uri.clone()),
                                span: uri_spans.get(self_uri).cloned(),
                                message: format!(
                                    "'use {} import({})' — target file not loaded; symbol '{}' unresolvable.",
                                    mcu.uri, id_str, id_str
                                ),
                                code: crate::errcodes::USE_IMPORT_SYMBOL_NOT_FOUND,
                            });
                        }
                    }

                    // ── K5: `pub use` of non-existent symbol ──
                    if mcu.public {
                        if let Some(target_mcode) = mcodes.get(&target_uri) {
                            let target_spacenames = &target_mcode.spacenames;
                            for id in impt_ids {
                                let id_str = id.to_string();
                                if !target_spacenames
                                    .iter()
                                    .any(|(k, _)| k.to_string() == id_str)
                                {
                                    acc.push(CheckResult {
                                        check_name: "imports",
                                        severity: CheckSeverity::Error,
                                        uri: Some(self_uri.clone()),
                                        span: uri_spans.get(self_uri).cloned(),
                                        message: format!(
                                            "'pub use {} import({})' — symbol '{}' not found in target; cannot re-export.",
                                            mcu.uri, id_str, id_str
                                        ),
                                        code: crate::errcodes::USE_REEXPORT_SYMBOL_NOT_FOUND,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Build the set of all known CMIE names across components, interfaces, enums, and modules.
fn build_cmie_name_set() -> HashSet<String> {
    let mut names = HashSet::new();
    let comps = &crate::db::cmie::tables::WORKSPACE.components;
    for e in comps.iter() {
        names.insert(e.key().ident.to_string());
    }
    let ifaces = &crate::db::cmie::tables::WORKSPACE.interfaces;
    for e in ifaces.iter() {
        names.insert(e.key().ident.to_string());
    }
    let enums = &crate::db::cmie::tables::WORKSPACE.enums;
    for e in enums.iter() {
        names.insert(e.key().ident.to_string());
    }
    let modules = &crate::db::cmie::tables::WORKSPACE.modules;
    for e in modules.iter() {
        names.insert(e.key().ident.to_string());
    }
    names
}

/// Build a set of local names from a file's spacenames map.
fn build_local_name_set(
    spacenames: &std::collections::BTreeMap<crate::McIds, crate::McSpaceName>,
) -> HashSet<String> {
    spacenames.iter().map(|(k, _)| k.to_string()).collect()
}
