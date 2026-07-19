// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Builder health diagnostics: `BuilderReport` struct + `BuildMode` enum
//!
//! ## Design goals (P02 / S1)
//! Replace `resolve_netpoint`'s "silently swallow on failure" with **structured recording**:
//! - Who failed (path / module / net_name)
//! - Which fallback level was used (Direct / Owner / BareLabel / Bracket / Failed)
//! - Entire net dropped vs partial point loss
//!
//! ## Output form
//! After `McVecBuilder` runs, get `&BuilderReport` via `builder.report()`,
//! caller can print / serialize / assert 0 dropped nets in CI.
//!
//! ## Modes
//! - `Tolerant` (default): swallow errors, only log
//! - `Strict`: any resolve failure immediately `Err(BuilderError::DataLoss)`
//! - `NoDataLoss`: entire net dropped is ok, but partial net (some points lost) errors

use std::fmt;

// ============================================================================
// BuildMode
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BuildMode {
    /// default — any error is ignored, only logged
    #[default]
    Tolerant,
    /// any resolve failure fails the build (CI recommended)
    Strict,
    /// moderate — dropped net is acceptable, but partial net is not
    NoDataLoss,
}

// ============================================================================
// ResolutionRecord
// ============================================================================

/// ResolutionRecord: every resolve operation
#[derive(Debug, Clone)]
pub struct ResolutionRecord {
    pub module_path: String,
    pub net_name: String,
    pub point_path: String,
    pub outcome: ResolutionOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolutionOutcome {
    /// direct path hit (`module_path.path` / `path` / end `.→/`)
    Direct,
    /// walked owner fallback
    OwnerFallback,
    /// walked bare-label fallback
    BareLabelFallback,
    /// bracket expanded, member hit
    BracketExpanded { member: String },
    /// ── ★ Phase D ─────────────────────────────────────────────────
    /// bare name matches a member of a bracket-form Port in this module
    /// example `path="VDD_3V3"` + `module_path="main.mcu513"` hits
    /// `main.mcu513.[VDD_3V3, GND]` (id=NNN), returns that Port's id.
    BracketPortMember { member: String, port_path: String },
    /// All failed
    Failed,
}

impl fmt::Display for ResolutionOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResolutionOutcome::Direct => write!(f, "direct"),
            ResolutionOutcome::OwnerFallback => write!(f, "owner_fallback"),
            ResolutionOutcome::BareLabelFallback => write!(f, "bare_label_fallback"),
            ResolutionOutcome::BracketExpanded { member } => write!(f, "bracket[{member}]"),
            ResolutionOutcome::BracketPortMember { member, port_path } => {
                write!(f, "bracket_port_member[{member} ∈ {port_path}]")
            }
            ResolutionOutcome::Failed => write!(f, "failed"),
        }
    }
}

// ============================================================================
// DroppedNet / PartialNet
// ============================================================================
/// DroppedNet: dropped net (all points failed or resolved to <2)
#[derive(Debug, Clone)]
pub struct DroppedNet {
    pub module_path: String,
    pub net_name: String,
    /// original point count (usually 3)
    pub original_point_count: usize,
    /// resolved point count (usually 0 or 1)
    pub resolved_point_count: usize,
}

/// PartialNet: partial net (some points failed or resolved to <2)
#[derive(Debug, Clone)]
pub struct PartialNet {
    pub module_path: String,
    pub net_name: String,
    /// failed point paths (usually bare names)
    pub failed_points: Vec<String>,
    /// resolved point count (usually 1)
    pub resolved_point_count: usize,
}

// ============================================================================
// BuilderReport
// ============================================================================

#[derive(Debug, Clone, Default)]
pub struct BuilderReport {
    /// every resolve operation
    pub resolutions: Vec<ResolutionRecord>,
    /// dropped net (all points failed or resolved to <2)
    pub dropped_nets: Vec<DroppedNet>,
    /// partial net (some points failed or resolved to <2)   
    pub partial_nets: Vec<PartialNet>,
    /// unresolved modules (bid = -1)
    pub unresolved_modules: Vec<String>,
}

impl BuilderReport {
    pub fn new() -> Self {
        Self::default()
    }

    /// no data loss
    pub fn is_clean(&self) -> bool {
        self.dropped_nets.is_empty()
            && self.partial_nets.is_empty()
            && self.unresolved_modules.is_empty()
    }

    /// compatible `np_warn_count()`: total warn count (drop = 1, partial = failed points)
    pub fn warn_count(&self) -> usize {
        self.dropped_nets.len()
            + self
                .partial_nets
                .iter()
                .map(|p| p.failed_points.len())
                .sum::<usize>()
    }

    /// success rate (0.0 ~ 1.0)
    pub fn success_rate(&self) -> f64 {
        if self.resolutions.is_empty() {
            return 1.0;
        }
        let total = self.resolutions.len();
        let failed = self
            .resolutions
            .iter()
            .filter(|r| matches!(r.outcome, ResolutionOutcome::Failed))
            .count();
        (total - failed) as f64 / total as f64
    }

    /// summary string
    pub fn summary_string(&self) -> String {
        let mut s = String::new();
        s.push_str("\n[builder] === REPORT ===\n");
        s.push_str(&format!(
            "  resolutions: {} (success rate: {:.1}%)\n",
            self.resolutions.len(),
            self.success_rate() * 100.0
        ));
        s.push_str(&format!(
            "  dropped nets (all): {}\n",
            self.dropped_nets.len()
        ));
        s.push_str(&format!(
            "  partial nets (some): {}\n",
            self.partial_nets.len()
        ));
        s.push_str(&format!(
            "  unresolved modules: {}\n",
            self.unresolved_modules.len()
        ));
        for d in &self.dropped_nets {
            s.push_str(&format!(
                "    DROP: net '{}' in '{}' ({} pts → {})\n",
                d.net_name, d.module_path, d.original_point_count, d.resolved_point_count
            ));
        }
        for p in &self.partial_nets {
            s.push_str(&format!(
                "    PARTIAL: net '{}' in '{}' ({} pts resolved, {} failed: {:?})\n",
                p.net_name,
                p.module_path,
                p.resolved_point_count,
                p.failed_points.len(),
                p.failed_points
            ));
        }
        for m in &self.unresolved_modules {
            s.push_str(&format!("    UNRESOLVED MODULE: '{m}'\n"));
        }
        s.push_str("[builder] === END ===\n");
        s
    }

    /// print summary to stderr
    pub fn print_summary(&self) {
        crate::velog!("{}", self.summary_string());
    }
}

// ============================================================================
// BuilderError
// ============================================================================

#[derive(Debug, Clone)]
pub enum BuilderError {
    /// Strict mode: any resolve failed triggers this
    DataLoss(BuilderReport),
    /// NoDataLoss mode: partial net
    PartialNets(BuilderReport),
}

impl fmt::Display for BuilderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuilderError::DataLoss(r) => {
                writeln!(f, "BuilderError::DataLoss")?;
                write!(f, "{}", r.summary_string())
            }
            BuilderError::PartialNets(r) => {
                writeln!(f, "BuilderError::PartialNets")?;
                write!(f, "{}", r.summary_string())
            }
        }
    }
}

impl std::error::Error for BuilderError {}

impl BuilderError {
    pub fn report(&self) -> &BuilderReport {
        match self {
            BuilderError::DataLoss(r) => r,
            BuilderError::PartialNets(r) => r,
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_report_is_clean() {
        let r = BuilderReport::new();
        assert!(r.is_clean());
        assert_eq!(r.warn_count(), 0);
        assert_eq!(r.success_rate(), 1.0);
    }

    #[test]
    fn dropped_net_not_clean() {
        let mut r = BuilderReport::new();
        r.dropped_nets.push(DroppedNet {
            module_path: "main".into(),
            net_name: "test".into(),
            original_point_count: 3,
            resolved_point_count: 0,
        });
        assert!(!r.is_clean());
        assert_eq!(r.warn_count(), 1);
    }

    #[test]
    fn partial_net_counts_each_failed_point() {
        let mut r = BuilderReport::new();
        r.partial_nets.push(PartialNet {
            module_path: "main".into(),
            net_name: "VCC".into(),
            failed_points: vec!["a.b".into(), "c.d".into()],
            resolved_point_count: 3,
        });
        assert_eq!(r.warn_count(), 2);
    }

    #[test]
    fn success_rate_calc() {
        let mut r = BuilderReport::new();
        for _ in 0..7 {
            r.resolutions.push(ResolutionRecord {
                module_path: "m".into(),
                net_name: "n".into(),
                point_path: "p".into(),
                outcome: ResolutionOutcome::Direct,
            });
        }
        for _ in 0..3 {
            r.resolutions.push(ResolutionRecord {
                module_path: "m".into(),
                net_name: "n".into(),
                point_path: "p".into(),
                outcome: ResolutionOutcome::Failed,
            });
        }
        assert!((r.success_rate() - 0.7).abs() < 1e-9);
    }

    #[test]
    fn builder_error_carries_report() {
        let mut r = BuilderReport::new();
        r.dropped_nets.push(DroppedNet {
            module_path: "m".into(),
            net_name: "n".into(),
            original_point_count: 2,
            resolved_point_count: 0,
        });
        let err = BuilderError::DataLoss(r.clone());
        assert_eq!(err.report().dropped_nets.len(), 1);
    }
}
