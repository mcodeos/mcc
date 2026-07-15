// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Cross-URI duplicate CMIE name detection.
//!
//! Warns when a user file defines a component/interface/enum/module with the
//! same name as one already defined in the system library or another file.

use super::{
    CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, PostParseContext, ValidationCheck,
};

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

    fn run_post_parse(&self, _ctx: &PostParseContext, acc: &mut CheckAccumulator) {
        use std::collections::HashMap;

        // Collect all CMIE names with their URIs from project tables
        let mut name_uris: HashMap<String, Vec<String>> = HashMap::new();

        // Check components
        {
            let comps = crate::builder::workspace::WORKSPACE.components.borrow();
            for entry in comps.iter() {
                let name = entry.key().ident.to_string();
                let uri = entry.key().uri.to_string();
                name_uris.entry(name).or_default().push(uri);
            }
        }
        // Check interfaces
        {
            let ifaces = crate::builder::workspace::WORKSPACE.interfaces.borrow();
            for entry in ifaces.iter() {
                let name = entry.key().ident.to_string();
                let uri = entry.key().uri.to_string();
                name_uris.entry(name).or_default().push(uri);
            }
        }
        // Check enums
        {
            let enums = crate::builder::workspace::WORKSPACE.enums.borrow();
            for entry in enums.iter() {
                let name = entry.key().ident.to_string();
                let uri = entry.key().uri.to_string();
                name_uris.entry(name).or_default().push(uri);
            }
        }
        // Check modules
        {
            let modules = crate::builder::workspace::WORKSPACE.modules.borrow();
            for entry in modules.iter() {
                let name = entry.key().ident.to_string();
                let uri = entry.key().uri.to_string();
                name_uris.entry(name).or_default().push(uri);
            }
        }

        // Report names that appear in >1 URI
        for (name, uris) in &name_uris {
            if uris.len() > 1 {
                // Filter out test files (unitest/ and cases*/)
                let non_test_uris: Vec<_> = uris
                    .iter()
                    .filter(|u| !u.contains("/unitest/") && !u.contains("/cases"))
                    .collect();
                if non_test_uris.len() > 1 {
                    let first = &non_test_uris[0];
                    for other in &non_test_uris[1..] {
                        acc.push(CheckResult {
                            check_name: self.name(),
                            severity: self.default_severity(),
                            uri: Some(name.clone()),
                            span: None, // no specific source span available here
                            message: format!(
                                "CMIE '{}' defined in both '{}' and '{}'. \
                                 The latter shadows the former.",
                                name, first, other
                            ),
                            code: 2100,
                        });
                    }
                }
            }
        }
    }
}
