// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Cross-URI duplicate CMIE name detection.
//!
//! Warns when a user file defines a component/interface/enum/module with the
//! same name as one already defined in the system library or another file.

use super::{CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, ValidationCheck};

pub struct DuplicateCmieCheck;

impl ValidationCheck for DuplicateCmieCheck {
    fn name(&self) -> &'static str {
        "duplicate-cmie"
    }
    fn phase(&self) -> CheckPhase {
        CheckPhase::PostParse
    }
    fn default_severity(&self) -> CheckSeverity {
        CheckSeverity::Warning
    }

    fn run_post_parse(&self, acc: &mut CheckAccumulator) {
        use std::collections::{HashMap, HashSet};

        /// CMIE kind for duplicate tracking.
        #[derive(Clone, Copy, PartialEq, Eq, Hash)]
        enum CmieKind {
            Component,
            Interface,
            Enum,
            Module,
        }

        // Collect all CMIE names with their (uri, kind) pairs from project tables.
        // key: name → value: list of (uri, kind)
        let mut name_entries: HashMap<String, Vec<(String, CmieKind)>> = HashMap::new();
        let mut uri_spans: HashMap<String, std::ops::Range<usize>> = HashMap::new();

        // Check components
        {
            let comps = &crate::db::cmie::tables::WORKSPACE.components;
            for entry in comps.iter() {
                let name = entry.key().ident.to_string();
                let uri = entry.key().uri.to_string();
                let span = entry.value().span.clone();
                uri_spans.entry(uri.clone()).or_insert(span.start..span.end);
                name_entries
                    .entry(name)
                    .or_default()
                    .push((uri, CmieKind::Component));
            }
        }
        // Check interfaces
        {
            let ifaces = &crate::db::cmie::tables::WORKSPACE.interfaces;
            for entry in ifaces.iter() {
                let name = entry.key().ident.to_string();
                let uri = entry.key().uri.to_string();
                let span = entry.value().span.clone();
                uri_spans.entry(uri.clone()).or_insert(span.start..span.end);
                name_entries
                    .entry(name)
                    .or_default()
                    .push((uri, CmieKind::Interface));
            }
        }
        // Check enums
        {
            let enums = &crate::db::cmie::tables::WORKSPACE.enums;
            for entry in enums.iter() {
                let name = entry.key().ident.to_string();
                let uri = entry.key().uri.to_string();
                let span = entry.value().span;
                uri_spans
                    .entry(uri.clone())
                    .or_insert(span[0] as usize..span[1] as usize);
                name_entries
                    .entry(name)
                    .or_default()
                    .push((uri, CmieKind::Enum));
            }
        }
        // Check modules
        {
            let modules = &crate::db::cmie::tables::WORKSPACE.modules;
            for entry in modules.iter() {
                let name = entry.key().ident.to_string();
                let uri = entry.key().uri.to_string();
                let span = entry.value().span.clone();
                uri_spans.entry(uri.clone()).or_insert(span.start..span.end);
                name_entries
                    .entry(name)
                    .or_default()
                    .push((uri, CmieKind::Module));
            }
        }

        // Report only same-kind collisions across different URIs.
        // enum+component with the same name is ALLOWED (namespace merges).
        for (name, entries) in &name_entries {
            let mut kind_uris: HashMap<CmieKind, HashSet<&String>> = HashMap::new();
            for (uri, kind) in entries {
                kind_uris.entry(*kind).or_default().insert(uri);
            }

            for (kind, uris) in &kind_uris {
                // Filter out test files
                let non_test: Vec<_> = uris.iter().filter(|u| !super::is_test_file(u)).collect();
                if non_test.len() > 1 {
                    let kind_str = match kind {
                        CmieKind::Component => "component",
                        CmieKind::Interface => "interface",
                        CmieKind::Enum => "enum",
                        CmieKind::Module => "module",
                    };
                    let first = non_test[0];
                    for other in &non_test[1..] {
                        acc.push(CheckResult {
                            check_name: self.name(),
                            severity: self.default_severity(),
                            uri: Some(name.clone()),
                            span: uri_spans.get(first.as_str()).cloned(),
                            message: format!(
                                "CMIE {} '{}' defined in both '{}' and '{}'. \
                                 The latter shadows the former.",
                                kind_str, name, first, other
                            ),
                            code: 2100,
                        });
                    }
                }
            }
        }
    }
}
