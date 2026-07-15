// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Validation Module — centralized semantic checks.
//!
//! Two phases:
//!   Instant  — runs during parsing (per-definition). Fast, AST available.
//!   PostParse — runs after all files loaded. Cross-URI analysis.
//!
//! Usage:
//!   let mut registry = CheckRegistry::with_defaults();
//!   registry.run_instant(&ctx);   // called from McComponent::new(), etc.
//!   registry.run_post_parse(&ctx); // called from mcb_parse_all_modules()

use crate::McURI;
use std::ops::Range;

// ============================================================================
// Check Phase
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckPhase {
    Instant,
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
// Instant Context (available during parsing)
// ============================================================================

pub struct InstantContext<'a> {
    pub def_name: &'a str,
    pub def_uri: &'a McURI,
    pub params: &'a crate::core::basic::mc_param::McParamDeclares,
    pub insts: Option<&'a crate::core::mc_inst::McInstances>,
}

// ============================================================================
// Post-Parse Context (available after all files loaded)
// ============================================================================

pub struct PostParseContext;

impl PostParseContext {
    pub fn new() -> Self {
        Self
    }
}

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
    uri.contains("/unitest/") || uri.contains("/cases")
}

// ============================================================================
// Check Trait
// ============================================================================

pub trait ValidationCheck: Send + Sync {
    fn name(&self) -> &'static str;
    fn phase(&self) -> CheckPhase;
    fn default_severity(&self) -> CheckSeverity;
    fn run_instant(&self, _ctx: &InstantContext) -> Vec<CheckResult> {
        vec![]
    }
    fn run_post_parse(&self, _ctx: &PostParseContext, acc: &mut CheckAccumulator) {}
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
        r.register(Box::new(check_duplicate::DuplicateCmieCheck));
        r.register(Box::new(check_dup_within::DupWithinCheck));
        r.register(Box::new(check_attrs::AttrsCheck));
        r.register(Box::new(check_defs::DefsCheck));
        r.register(Box::new(check_imports::ImportsCheck));
        r.register(Box::new(check_naming::NamingCheck));
        r.register(Box::new(check_ports::PortInstanceCheck));
        r.register(Box::new(check_refs::RefIntegrityCheck));
        r.register(Box::new(check_style::StyleCheck));
        r.register(Box::new(check_exprs::ExprsCheck));
        r.register(Box::new(check_extra::ExtraCheck));
        r
    }

    pub fn register(&mut self, check: Box<dyn ValidationCheck>) {
        self.checks.push(check);
    }

    pub fn run_instant(&self, ctx: &InstantContext) -> Vec<CheckResult> {
        let mut results = Vec::new();
        for check in &self.checks {
            if check.phase() == CheckPhase::Instant {
                results.extend(check.run_instant(ctx));
            }
        }
        results
    }

    pub fn run_post_parse(&self, ctx: &PostParseContext) -> Vec<CheckResult> {
        let mut acc = CheckAccumulator::new();
        for check in &self.checks {
            if check.phase() == CheckPhase::PostParse {
                check.run_post_parse(ctx, &mut acc);
            }
        }
        acc.results
    }
}

// ============================================================================
// Sub-modules
// ============================================================================

pub mod check_attrs;
pub mod check_defs;
pub mod check_dup_within;
pub mod check_duplicate;
pub mod check_exprs;
pub mod check_extra;
pub mod check_imports;
pub mod check_naming;
pub mod check_ports;
pub mod check_refs;
pub mod check_style;
