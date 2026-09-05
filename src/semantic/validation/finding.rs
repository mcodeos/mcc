// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Unified check-finding line (rule-registry design, stage-0 "unified result
//! shape").
//!
//! Every numeric-code check scope keeps emitting its own host carrier and
//! keeps its own push sites and runner signatures; the carriers are projected
//! here into one `CheckFinding` line by pure conversions. The shared shape is
//! what the normalization/adjudication point consumes (design §8-5: one
//! decision point after the unified result is produced, before
//! envelope/gate/lock tests; five hosts' push sites stay untouched).
//!
//! The three numeric-code carriers normalized today:
//!   - `PostParse`   -> `CheckResult`    (the `validation` CheckRegistry)
//!   - `FlatErc`     -> `NetCheckResult` (`validation::nets`)
//!   - `Declaration` -> `PinCheckResult` (`validation::pins`)
//!
//! `Report` (AssemblyGate) and the viz A/F self-registered lines stay out of
//! this stage's production wiring; their host-to-finding projections, when
//! needed, reuse the same shape.
//!
//! A carrier and its finding are field-identical; the equivalence is locked
//! by the tests below (severity enum<->string, span -> (pos,len), uri
//! anchoring, and the two existing sink mappings reproduced through
//! [`CheckFinding::to_diagnostic`]).

use crate::db::diagnostic::diagnostic::{Diagnostic, DiagnosticLevel, Location};
use crate::semantic::validation::nets::NetCheckResult;
use crate::semantic::validation::pins::PinCheckResult;
use crate::semantic::validation::{CheckResult, CheckSeverity};

/// One canonical emitted finding shared by all numeric-code check scopes.
///
/// The scope-specific subject of a host carrier (a `NetCheckResult`'s
/// `net_name`, a `PinCheckResult`'s `instance_path`) is already embedded in
/// `message` and anchored by `uri`/`pos`/`len`, so it is not duplicated here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckFinding {
    /// Rule/check key declared by the emitting scope ("driver-conflict",
    /// "unused-pin", ...). Matches the catalog rule name.
    pub rule: &'static str,
    /// Numeric error code (single source: the central `errcodes` table).
    pub code: u32,
    /// Canonical severity.
    pub severity: CheckSeverity,
    /// Human-readable message, byte-identical to the source carrier's.
    pub message: String,
    /// Source file URI of the anchor. `None` means the source host recorded
    /// no URI: an unattributable PostParse result, or a FlatErc/Declaration
    /// row pushed with an empty uri.
    pub uri: Option<String>,
    /// Source byte offset of the anchor (0 when unavailable).
    pub pos: u32,
    /// Source byte length of the anchor (0 for the hosts that carry a point
    /// position rather than a byte span).
    pub len: u32,
}

impl From<CheckResult> for CheckFinding {
    fn from(r: CheckResult) -> Self {
        let (pos, len) = r
            .span
            .as_ref()
            .map(|s| (s.start as u32, (s.end - s.start) as u32))
            .unwrap_or((0, 0));
        Self {
            rule: r.check_name,
            code: r.code,
            severity: r.severity,
            message: r.message,
            uri: r.uri,
            pos,
            len,
        }
    }
}

impl From<NetCheckResult> for CheckFinding {
    fn from(r: NetCheckResult) -> Self {
        Self {
            rule: r.check,
            code: r.code,
            // String-typed host severity -> enum. Unknown values normalize to
            // Warning, mirroring the net sink's existing default
            // (`_ => DiagnosticLevel::Warning`); no host emits others today.
            severity: CheckSeverity::from_str(r.severity).unwrap_or(CheckSeverity::Warning),
            message: r.message,
            uri: non_empty_uri(r.uri),
            pos: r.pos,
            len: 0,
        }
    }
}

impl From<PinCheckResult> for CheckFinding {
    fn from(r: PinCheckResult) -> Self {
        Self {
            rule: r.check,
            code: r.code,
            severity: CheckSeverity::from_str(r.severity).unwrap_or(CheckSeverity::Warning),
            message: r.message,
            uri: non_empty_uri(r.uri),
            pos: r.pos,
            len: 0,
        }
    }
}

/// Project a host uri string onto the canonical `Option<String>` form: an
/// empty string (the flat hosts' "no anchor" convention) becomes `None`.
fn non_empty_uri(uri: String) -> Option<String> {
    if uri.is_empty() {
        None
    } else {
        Some(uri)
    }
}

impl CheckFinding {
    /// Flatten the finding into the store's ready-to-log `Diagnostic` line.
    ///
    /// Mirrors the two existing sink mappings byte-for-byte: the PostParse
    /// runner's severity-enum -> level and span -> (pos,len)
    /// (build/pass1.rs), and the flat net sink's uri/pos passthrough with a
    /// zero length. The per-host emission policies that live *around* the
    /// sinks — PostParse drops uri-less results; `log_net_check_diagnostics`
    /// falls back to the caller's current uri for empty anchors — are not
    /// encoded here; they stay at the host emission sites, which are
    /// unchanged this stage.
    pub fn to_diagnostic(&self) -> Diagnostic {
        let level = match self.severity {
            CheckSeverity::Error => DiagnosticLevel::Error,
            CheckSeverity::Warning => DiagnosticLevel::Warning,
            CheckSeverity::Info => DiagnosticLevel::Info,
            CheckSeverity::Hint => DiagnosticLevel::Hint,
        };
        Diagnostic::new(
            self.code,
            level,
            Location::new(
                crate::McURI::from(self.uri.as_deref().unwrap_or("")),
                self.pos,
                self.len,
            ),
            self.message.clone(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ops::Range;

    fn diag_eq(a: &Diagnostic, b: &Diagnostic) -> bool {
        a.code == b.code
            && a.level == b.level
            && a.loc.uri == b.loc.uri
            && a.loc.pos == b.loc.pos
            && a.loc.len == b.loc.len
            && a.msg == b.msg
    }

    fn check_result(
        severity: CheckSeverity,
        uri: Option<&str>,
        span: Option<Range<usize>>,
    ) -> CheckResult {
        CheckResult {
            check_name: "ref-integrity",
            severity,
            uri: uri.map(|s| s.to_string()),
            span,
            message: "message text".to_string(),
            code: 5200,
        }
    }

    fn net_result(severity: &'static str, uri: &str, pos: u32) -> NetCheckResult {
        NetCheckResult {
            check: "driver-conflict",
            severity,
            message: "net message".to_string(),
            net_name: "N1".to_string(),
            code: 4101,
            pos,
            uri: uri.to_string(),
        }
    }

    fn pin_result(severity: &'static str, uri: &str, pos: u32) -> PinCheckResult {
        PinCheckResult {
            check: "unused-pin",
            severity,
            message: "pin message".to_string(),
            instance_path: "U1".to_string(),
            code: 5155,
            pos,
            uri: uri.to_string(),
        }
    }

    #[test]
    fn severity_string_round_trip_is_textual() {
        for s in ["error", "warning", "info", "hint"] {
            let sev = CheckSeverity::from_str(s).expect("known severity string");
            assert_eq!(sev.as_str(), s);
        }
        assert_eq!(CheckSeverity::from_str("unknown"), None);
    }

    #[test]
    fn post_parse_projection_is_field_identical() {
        let r = check_result(CheckSeverity::Warning, Some("case.mc"), Some(10..24));
        let f = CheckFinding::from(r);
        assert_eq!(f.rule, "ref-integrity");
        assert_eq!(f.code, 5200);
        assert_eq!(f.severity, CheckSeverity::Warning);
        assert_eq!(f.message, "message text");
        assert_eq!(f.uri, Some("case.mc".to_string()));
        assert_eq!(f.pos, 10);
        assert_eq!(f.len, 14);
    }

    #[test]
    fn post_parse_unattributable_projection_keeps_none() {
        let f = CheckFinding::from(check_result(CheckSeverity::Error, None, None));
        assert_eq!(f.uri, None);
        assert_eq!((f.pos, f.len), (0, 0));
    }

    #[test]
    fn flat_erc_projection_is_field_identical() {
        let f = CheckFinding::from(net_result("error", "net.mc", 42));
        assert_eq!(f.rule, "driver-conflict");
        assert_eq!(f.code, 4101);
        assert_eq!(f.severity, CheckSeverity::Error);
        assert_eq!(f.message, "net message");
        assert_eq!(f.uri, Some("net.mc".to_string()));
        assert_eq!((f.pos, f.len), (42, 0));
    }

    #[test]
    fn flat_erc_empty_uri_projection_is_none() {
        let f = CheckFinding::from(net_result("warning", "", 7));
        assert_eq!(f.uri, None);
        assert_eq!(f.severity, CheckSeverity::Warning);
    }

    #[test]
    fn declaration_projection_is_field_identical() {
        let f = CheckFinding::from(pin_result("info", "pin.mc", 3));
        assert_eq!(f.rule, "unused-pin");
        assert_eq!(f.code, 5155);
        assert_eq!(f.severity, CheckSeverity::Info);
        assert_eq!(f.message, "pin message");
        assert_eq!(f.uri, Some("pin.mc".to_string()));
        assert_eq!((f.pos, f.len), (3, 0));
    }

    #[test]
    fn severity_strings_map_to_their_enum_levels() {
        assert_eq!(
            CheckFinding::from(net_result("warning", "", 0)).severity,
            CheckSeverity::Warning
        );
        assert_eq!(
            CheckFinding::from(pin_result("info", "", 0)).severity,
            CheckSeverity::Info
        );
        assert_eq!(
            CheckFinding::from(net_result("error", "", 0)).severity,
            CheckSeverity::Error
        );
    }

    #[test]
    fn to_diagnostic_matches_nets_sink_mapping() {
        let r = net_result("error", "net.mc", 42);
        // Legacy mapping reproduced from `net_results_to_diagnostics`.
        let legacy = Diagnostic::new(
            r.code,
            match r.severity {
                "error" => DiagnosticLevel::Error,
                "info" => DiagnosticLevel::Info,
                _ => DiagnosticLevel::Warning,
            },
            Location::new(crate::McURI::from(r.uri.as_str()), r.pos, 0),
            r.message.clone(),
        );
        let via_finding = CheckFinding::from(r.clone()).to_diagnostic();
        assert!(diag_eq(&legacy, &via_finding));
    }

    #[test]
    fn to_diagnostic_matches_post_parse_sink_mapping() {
        let r = check_result(CheckSeverity::Warning, Some("case.mc"), Some(10..24));
        // Legacy mapping reproduced from the pass1 runner (span -> pos/len,
        // severity enum -> level).
        let level = match r.severity {
            CheckSeverity::Error => DiagnosticLevel::Error,
            CheckSeverity::Warning => DiagnosticLevel::Warning,
            CheckSeverity::Info => DiagnosticLevel::Info,
            CheckSeverity::Hint => DiagnosticLevel::Hint,
        };
        let (pos, len) = r
            .span
            .as_ref()
            .map(|s| (s.start as u32, (s.end - s.start) as u32))
            .unwrap_or((0, 0));
        let legacy = Diagnostic::new(
            r.code,
            level,
            Location::new(
                crate::McURI::from(r.uri.clone().unwrap_or_default().as_str()),
                pos,
                len,
            ),
            r.message.clone(),
        );
        let via_finding = CheckFinding::from(r).to_diagnostic();
        assert!(diag_eq(&legacy, &via_finding));
    }

    #[test]
    fn to_diagnostic_flat_empty_anchor_stays_empty_uri() {
        // A FlatErc row pushed with an empty uri keeps an empty anchor in the
        // emitted line; the caller-side fallback to `current_uri` remains the
        // log helper's job (`log_net_check_diagnostics`), not the finding's.
        let d = CheckFinding::from(net_result("warning", "", 7)).to_diagnostic();
        assert_eq!(d.loc.uri, "");
        assert_eq!(d.loc.pos, 7);
        assert_eq!(d.loc.len, 0);
        assert_eq!(d.level, DiagnosticLevel::Warning);
    }
}
