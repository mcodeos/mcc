// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Validation Module — centralized semantic checks.
//!
//! PostParse — runs after all files loaded. Cross-URI analysis.
//!
//! Usage:
//!   let registry = CheckRegistry::with_defaults();
//!   registry.run_post_parse(&ctx); // called from mcb_parse_all_modules()

use std::ops::Range;

// ============================================================================
// Check Phase
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckPhase {
    PostParse,
}

// ============================================================================
// Check Severity
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CheckSeverity {
    Hint = 0,
    Info = 1,
    Warning = 2,
    Error = 3,
}

impl CheckSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Hint => "hint",
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

// ============================================================================
// Check Result
// ============================================================================

#[derive(Debug, Clone)]
pub struct CheckResult {
    pub check_name: &'static str,
    pub severity: CheckSeverity,
    pub uri: Option<String>,
    pub span: Option<Range<usize>>,
    pub message: String,
    pub code: u32,
}

// ============================================================================
// Post-Parse Check Infrastructure
// ============================================================================

/// Accumulator for collecting check results during a post-parse pass.
pub struct CheckAccumulator {
    pub results: Vec<CheckResult>,
}

impl CheckAccumulator {
    pub fn new() -> Self {
        Self {
            results: Vec::new(),
        }
    }
    pub fn push(&mut self, r: CheckResult) {
        self.results.push(r);
    }
}

// ============================================================================
// Shared Utilities
// ============================================================================

/// Returns true if the given URI belongs to a test file (unit test or test case).
pub(crate) fn is_test_file(uri: &str) -> bool {
    let path = std::path::Path::new(uri);
    path.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        s == "unitest" || s == "cases"
    })
}

// ============================================================================
// Check Trait
// ============================================================================

pub trait ValidationCheck: Send + Sync {
    fn name(&self) -> &'static str;
    fn phase(&self) -> CheckPhase;
    fn default_severity(&self) -> CheckSeverity;
    fn run_post_parse(&self, _acc: &mut CheckAccumulator) {}
}

pub struct CheckRegistry {
    checks: Vec<Box<dyn ValidationCheck>>,
}

impl CheckRegistry {
    pub fn new() -> Self {
        Self { checks: Vec::new() }
    }

    pub fn with_defaults() -> Self {
        let mut r = Self::new();
        r.register(Box::new(duplicate::DuplicateCmieCheck));
        r.register(Box::new(dupwithin::DupWithinCheck));
        r.register(Box::new(enums::EnumsCheck));
        r.register(Box::new(attrs::AttrsCheck));
        r.register(Box::new(conds::CondsCheck));
        r.register(Box::new(defs::DefsCheck));
        r.register(Box::new(imports::ImportsCheck));
        r.register(Box::new(interface::InterfaceCheck));
        r.register(Box::new(naming::NamingCheck));
        r.register(Box::new(ports::PortInstanceCheck));
        r.register(Box::new(refs::RefIntegrityCheck));
        r.register(Box::new(style::StyleCheck));
        r.register(Box::new(exprs::ExprsCheck));
        r.register(Box::new(extra::ExtraCheck));
        r.register(Box::new(insts::InstsCheck));
        r.register(Box::new(body::BodyCheck));
        r.register(Box::new(hw::HwCheck));
        r.register(Box::new(types::TypesCheck));
        r
    }

    pub fn register(&mut self, check: Box<dyn ValidationCheck>) {
        self.checks.push(check);
    }

    pub fn run_post_parse(&self) -> Vec<CheckResult> {
        let mut acc = CheckAccumulator::new();
        for check in &self.checks {
            if check.phase() == CheckPhase::PostParse {
                check.run_post_parse(&mut acc);
            }
        }
        acc.results
    }
}

// ============================================================================
// Sub-modules
// ============================================================================

pub mod attrs;
pub mod body;
pub mod conds;
pub mod defs;
pub mod duplicate;
pub mod dupwithin;
pub mod enums;
pub mod exprs;
pub mod extra;
pub mod hw;
pub mod imports;
pub mod insts;
pub mod interface;
pub mod naming;
pub mod nets;
pub mod pins;
pub mod ports;
pub mod refs;
pub mod style;
pub mod types;
