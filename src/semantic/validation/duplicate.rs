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

        // Check components (workspace-only: cross-URI duplicates are a
        // project-level check, the system lib is not part of it)
        {
            let comps = crate::definition_space().workspace_components();
            for (sn, comp) in comps.iter() {
                let name = sn.ident.to_string();
                let uri = sn.uri.to_string();
                let span = comp.span.clone();
                uri_spans.entry(uri.clone()).or_insert(span.start..span.end);
                name_entries
                    .entry(name)
                    .or_default()
                    .push((uri, CmieKind::Component));
            }
        }
        // Check interfaces
        {
            let ifaces = crate::definition_space().workspace_interfaces();
            for (sn, iface) in ifaces.iter() {
                let name = sn.ident.to_string();
                let uri = sn.uri.to_string();
                let span = iface.span.clone();
                uri_spans.entry(uri.clone()).or_insert(span.start..span.end);
                name_entries
                    .entry(name)
                    .or_default()
                    .push((uri, CmieKind::Interface));
            }
        }
        // Check enums
        {
            let enums = crate::definition_space().workspace_enums();
            for (sn, def) in enums.iter() {
                let name = sn.ident.to_string();
                let uri = sn.uri.to_string();
                let span = def.span;
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
            let modules = crate::definition_space().workspace_modules();
            for (sn, module) in modules.iter() {
                let name = sn.ident.to_string();
                let uri = sn.uri.to_string();
                let span = module.span.clone();
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
                        // Attribute to the shadowing definition's own file+span
                        // (the actionable location), never to the symbol name.
                        acc.push(CheckResult {
                            check_name: self.name(),
                            severity: self.default_severity(),
                            uri: Some(other.to_string()),
                            span: uri_spans.get(other.as_str()).cloned(),
                            message: format!(
                                "CMIE {} '{}' defined in both '{}' and '{}'. \
                                 The latter shadows the former.",
                                kind_str, name, first, other
                            ),
                            code: crate::errcodes::DUP_CMIE_CROSS_FILE,
                        });
                    }
                }
            }
        }
    }
}
