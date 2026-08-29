// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Pass2 Instantiation - Connection Types
//!
//! Define basic data structures for connection types:
//!
//! - `NetPoint`       - Network Connection Point
//! - `ConnectionInst` - Connection Instance
//! - `PortInst`       - Port Instance
//! - `InstError`      - Instantiation Error
//! - `NetTable`       - Network Table (union-find)

use crate::semantic::common::{ConnDir, ConnOp, IOType, SourcePos};
use crate::semantic::validation::ledger::{self, LedgerAction, LedgerEntry, LedgerKind};
use crate::vector::model::trunk::TrunkCtx;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Mutex;

/// Literal (unexpanded) vector reference count (R01). Counting only, non-blocking; also active in release builds.
pub static LITERAL_POINTS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Literal (unexpanded) vector reference details: quarantined (original path, src_pos)
pub static LITERAL_POINT_DETAILS: std::sync::LazyLock<Mutex<Vec<(String, Option<i32>)>>> =
    std::sync::LazyLock::new(|| Mutex::new(Vec::new()));

// ============================================================================
// Pin path normalization — unified canonical form
// ============================================================================

/// Normalize a pin path to canonical form.
///
/// Rules:
/// - Remove `pins` segment (pins transparency): `uC.pins.VDD` → `uC.VDD`
/// - Fold consecutive identical segments: `uC.VDD.VDD` → `uC.VDD`
/// - Preserve `pins` when followed by numeric index (e.g. `uC.pins.8` keeps
///   `pins` — the numeric index indicates ID-based access, not name-based)
///
/// Returns the normalized path, or the original if no changes were needed.
///
/// # Examples
///
/// ```
/// use mcc::instant::mc_net::normalize_pin_path;
/// assert_eq!(normalize_pin_path("uC.pins.VDD"), "uC.VDD");
/// assert_eq!(normalize_pin_path("uC.VDD"), "uC.VDD");
/// assert_eq!(normalize_pin_path("uC.VDD.VDD"), "uC.VDD");
/// assert_eq!(normalize_pin_path("uC.pins.8"), "uC.pins.8");
/// assert_eq!(normalize_pin_path("R1.1"), "R1.1");
/// ```
pub fn normalize_pin_path(path: &str) -> String {
    let segments: Vec<&str> = path.split('.').collect();
    if segments.len() <= 1 {
        return path.to_string();
    }

    let mut result: Vec<&str> = Vec::with_capacity(segments.len());
    let mut changed = false;

    for (i, &seg) in segments.iter().enumerate() {
        // Pins transparency: skip "pins" segment UNLESS followed by a numeric
        // index segment (e.g. "uC.pins.8" → "uC.pins.8", but "uC.pins.VDD" → "uC.VDD")
        if seg == "pins" {
            if let Some(next) = segments.get(i + 1) {
                if next.chars().all(|c| c.is_ascii_digit()) {
                    // Numeric index follows — keep both "pins" and the index
                    result.push(seg);
                    continue;
                }
            }
            // Non-numeric or end-of-path — skip "pins" transparently
            changed = true;
            continue;
        }

        // Fold consecutive identical segments: "VDD.VDD" → "VDD"
        if result.last() == Some(&seg) {
            changed = true;
            continue;
        }

        result.push(seg);
    }

    if changed {
        result.join(".")
    } else {
        path.to_string()
    }
}

// ============================================================================
// NetPoint - Network Connection Point
// ============================================================================

/// Network Connection Point, representing an endpoint in the netlist.
///
/// Examples:
/// - `NetPoint { path: "a", owner: None, io: In }`            (port/label)
/// - `NetPoint { path: "R1.1", owner: Some("R1"), io: None }`  (component pin)
/// - `NetPoint { path: "sub1.clk", owner: Some("sub1"), io: In }` (submodule port)
#[derive(Debug, Clone)]
pub struct NetPoint {
    /// Full path: "a", "R1.1", "sub1.SPI.SCLK"
    pub path: String,

    /// Instance owner (None for ports/labels)
    pub owner: Option<String>,

    /// IO direction (None for ports/labels)
    pub iotype: IOType,

    /// Source position in the AST (for diagnostic source-line reporting).
    /// Unified [`SourcePos`] (uri + byte offset, §7.11(3)).
    pub src_pos: Option<crate::semantic::common::SourcePos>,

    /// P2-1: bus member name (e.g. "CS", "SCLK", "MISO", "MOSI" for SPI).
    /// Used for name-based matching in create_connection.
    pub member_name: Option<String>,

    /// Same-name multi-pin group pads (same-name-pin-group.md §2/§6): a
    /// logical slot point references ONE logical net whose physical pads are
    /// listed here (e.g. `spk{GND}` → [spk.3, spk.4]). Non-empty means the
    /// point must expand to its pads (fan-in) when a connection is generated;
    /// empty = ordinary point with no expansion.
    pub same_name_pads: Vec<NetPoint>,
}

impl NetPoint {
    /// Create a simple net point (port/label)
    ///
    /// ★ Patch 2-1: literal reference → quarantine, no panic.
    /// When `{`, `[`, `,` is detected, replace the path with a unique
    /// `@_phantom_<N>`, record the original path in `LITERAL_POINT_DETAILS`
    /// and the count in `LITERAL_POINTS`.
    /// Quarantined points never enter union-find merging (filtered by
    /// `NetTable::add_connection`), so they cannot spread from R01 into a
    /// giant R06 net.
    pub fn new(path: &str, iotype: IOType) -> Self {
        let normalized = normalize_pin_path(path);
        let p = &normalized;
        let actual_path = if p.contains(['{', '[', ',']) {
            let n = LITERAL_POINTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let quarantine = format!("@_phantom_{n}");
            LITERAL_POINT_DETAILS
                .lock()
                .unwrap()
                .push((p.to_string(), None));
            ledger::record(
                LedgerEntry::new(LedgerKind::Phantom, p.to_string(), "net-point")
                    .with_action(LedgerAction::Silent),
            );
            quarantine
        } else {
            p.to_string()
        };
        Self {
            path: actual_path,
            owner: None,
            iotype,
            src_pos: None,
            member_name: None,
            same_name_pads: Vec::new(),
        }
    }

    /// Create a net point belonging to a component instance (pin/submodule port)   
    pub fn with_owner(path: &str, owner: &str, iotype: IOType) -> Self {
        let normalized = normalize_pin_path(path);
        let p = &normalized;
        let actual_path = if p.contains(['{', '[', ',']) {
            let n = LITERAL_POINTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let quarantine = format!("@_phantom_{n}");
            LITERAL_POINT_DETAILS
                .lock()
                .unwrap()
                .push((p.to_string(), None));
            ledger::record(
                LedgerEntry::new(LedgerKind::Phantom, p.to_string(), "net-point")
                    .with_action(LedgerAction::Silent),
            );
            quarantine
        } else {
            p.to_string()
        };
        Self {
            path: actual_path,
            owner: Some(owner.to_string()),
            iotype,
            src_pos: None,
            member_name: None,
            same_name_pads: Vec::new(),
        }
    }

    /// Set source position (for diagnostic source-line reporting).
    /// Unified [`SourcePos`] (uri + byte offset, §7.11(3)).
    pub fn with_src_pos(mut self, pos: crate::semantic::common::SourcePos) -> Self {
        self.src_pos = Some(pos);
        self
    }

    /// P2-1: set bus member name for name-based matching
    pub fn with_member_name(mut self, name: &str) -> Self {
        self.member_name = Some(name.to_string());
        self
    }

    /// Same-name multi-pin group: attach the physical pads this logical slot
    /// expands to (fan-in) at connection generation (same-name-pin-group.md §6.3).
    pub fn with_same_name_pads(mut self, pads: Vec<NetPoint>) -> Self {
        self.same_name_pads = pads;
        self
    }
}

impl fmt::Display for NetPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.path)?;
        match self.iotype {
            IOType::In => write!(f, "(in)"),
            IOType::Out => write!(f, "(out)"),
            IOType::InOut => write!(f, "(io)"),
            IOType::Power => write!(f, "(pwr)"),
            IOType::Analog => write!(f, "(anl)"),
            IOType::Return => write!(f, "(return)"),
            IOType::NonCon => write!(f, "(nc)"),
            IOType::Label => write!(f, "(label)"),
            IOType::None => Ok(()),
        }
    }
}

// ============================================================================
// ConnectionInst - Connection Instance
// ============================================================================

/// Connection Instance, representing a group of connected network points.
///
/// Generated by `process_stmt()` when processing adjacent `McPhrase`s:
/// ```text
/// McPhrase: [M0] - [M1] - [M2]
///              ↑       ↑
///         conn_0   conn_1
/// ```
#[derive(Debug, Clone)]
pub struct ConnectionInst {
    /// Connection ID (auto-incremented)
    pub id: u32,

    /// All connected points
    pub points: Vec<NetPoint>,

    /// Network name (first label owner or anonymous `_net{N}`)
    pub net_name: Option<String>,

    /// Source connector direction (from `->` / `-` / `+`)
    pub dir: ConnDir,

    /// Connection operator that produced this connection: `Series` for
    /// `-`/`->`/`<-`, `Parallel` for `+`. `None` when the operator is unknown
    /// (engine-generated projection trunks, e.g. interface / bus member nets).
    /// Carried through to `ConnPair` / `NetShape` so downstream can tell a
    /// series net from a parallel one without reverse-engineering the shape.
    pub op: Option<ConnOp>,

    /// Bus lane index (0=first lane, 1=second...). None for scalar connections.
    pub lane: Option<u16>,

    /// Name of the two-terminal device instance this connection "passes through"
    /// (e.g. the R1 instance name in VCC→R1→GPIO). None for non-chain topologies.
    /// Resolved to i64 ID in visit.rs and stored in ConnPair.
    pub via: Option<String>,

    /// ★ P9-A2: source span for bidirectional traceability.
    /// Unified [`SourcePos`] — which source file and byte offset created this
    /// connection (§7.11(3)).
    pub source_span: Option<crate::semantic::common::SourcePos>,

    /// ★ §8.9.6: structured trunk context of this connection lane (trunk name,
    /// lane member, coarse kind), decided at the AST layer. All lanes of the
    /// same trunk share the same `name` / `kind` but carry distinct `member`s.
    pub trunk: Option<TrunkCtx>,

    /// Expansion provenance: index into the owning module's `ExpansionLog`.
    /// None = created at module top level (no active expansion).
    pub expansion_id: Option<usize>,
}

impl ConnectionInst {
    /// Deduplicate points by canonical path, keeping the first occurrence
    /// (with its rich info like owner / iotype) and discarding later points
    /// with the same canon. `canonicalize_path` folds `X.X→X`, `X.Y.Y→X.Y`,
    /// and curly brace/arrow remnants, so the same canonical path = the same
    /// physical node; appearing twice in one connection has no electrical
    /// meaning. Reused after same-name group fan-in expansion (add_connection),
    /// which may re-introduce duplicates that `new` already folded.
    pub(super) fn dedup_canonical(points: Vec<NetPoint>) -> Vec<NetPoint> {
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut out: Vec<NetPoint> = Vec::with_capacity(points.len());
        for p in points {
            let canon = canonicalize_path(&p.path);
            if seen.insert(canon) {
                out.push(p);
            }
        }
        out
    }

    /// Create new connection
    ///
    /// ── Iter-10.1: Normalize path for net_name inference ──
    pub fn new(id: u32, points: Vec<NetPoint>) -> Self {
        // ── P5: Deduplicate points by canonical path ──────────────────────────
        // `canonicalize_path` folds `X.X→X`, `X.Y.Y→X.Y`, and curly brace/arrow
        // remnants, resulting in a path string that serves as the node merge key
        // (see claude.md §2: "Path string is the net merge key").
        // Therefore, the same canonical path = the same physical node; appearing
        // twice in the same connection has no electrical meaning, only triggers
        // the upper-layer "duplicate-point connections" warning and causes that
        // point to be counted multiple times in the net. We converge to a single
        // construction entry point for unified deduplication: keep the first
        // appearing NetPoint (including its rich info like owner / iotype),
        // discard subsequent points with the same canon.
        //
        // Note: callers like create_connection have already used the original len
        // for shape (1:1 / 1:N / N:1) judgment before calling new(), so the
        // deduplication here does not affect shape alignment logic.
        let points = Self::dedup_canonical(points);

        // ── P6: Infer net_name from first label owner ──────────────────────────
        // Try to infer the first label owner as the net_name.
        //
        // ── Iter-9 (bugfix_report error 14) ─────────────────────────────
        // Exclude path segments with >= 3 parts as net_name candidates.
        //
        // ── Iter-10.1 ──
        // Normalize path before counting segments to avoid suffix expansion.
        let net_name = points
            .iter()
            .find(|p| {
                p.owner.is_none() && {
                    let canon = canonicalize_path(&p.path);
                    canon.matches('.').count() < 2
                }
            })
            .map(|p| canonicalize_path(&p.path));

        Self {
            id,
            points,
            net_name,
            dir: ConnDir::Undirected,
            op: None,
            lane: None,
            via: None,
            source_span: None,
            trunk: None,
            expansion_id: None,
        }
    }

    /// Set source connector direction
    pub fn with_dir(mut self, dir: ConnDir) -> Self {
        self.dir = dir;
        self
    }

    /// Set the connection operator (`Series` for `-`/`->`/`<-`, `Parallel`
    /// for `+`). `None` means the operator is unknown (projection trunks).
    pub fn with_op(mut self, op: ConnOp) -> Self {
        self.op = Some(op);
        self
    }

    /// Set bus lane index (not called for scalar connections)
    pub fn with_lane(mut self, lane: u16) -> Self {
        self.lane = Some(lane);
        self
    }

    /// Set pass-through device instance name
    pub fn with_via(mut self, via: String) -> Self {
        self.via = Some(via);
        self
    }

    /// ★ P9-A2: Set source span for traceability (unified [`SourcePos`]).
    pub fn with_source_span(mut self, pos: crate::semantic::common::SourcePos) -> Self {
        self.source_span = Some(pos);
        self
    }

    /// ★ §8.9.6: Set the structured trunk context of this connection lane.
    pub fn with_trunk(mut self, ctx: TrunkCtx) -> Self {
        self.trunk = Some(ctx);
        self
    }

    /// Get effective net_name (first label owner or `_net{id}`)
    pub fn effective_net_name(&self) -> String {
        self.net_name
            .clone()
            .unwrap_or_else(|| format!("_net{}", self.id))
    }
}

/// Whether a net name is an engine-generated anonymous net (`_net30`).
/// Requires the `_net` prefix followed by a digit so that a
/// user-written name like `_network` or `_net_1` is not misclassified.
pub fn is_anon_net_name(name: &str) -> bool {
    name.starts_with("_net") && name.as_bytes().get(4).is_some_and(|b| b.is_ascii_digit())
}

impl fmt::Display for ConnectionInst {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "net")?;
        if let Some(ref name) = self.net_name {
            write!(f, "({name})")?;
        }
        write!(f, ": ")?;
        for (i, p) in self.points.iter().enumerate() {
            if i > 0 {
                write!(f, " ~ ")?;
            }
            write!(f, "{p}")?;
        }
        Ok(())
    }
}

// ============================================================================
// PortInst - Port Instance
// ============================================================================

/// Port Instance, representing an exposed port of a module.
#[derive(Debug, Clone)]
pub struct PortInst {
    /// Port name
    pub name: String,

    /// IO direction
    pub iotype: IOType,

    /// Corresponding network point
    pub net_point: NetPoint,

    /// ── Iter-8 ────────────────────────────────────────────────────
    /// Bus port members (N×1 bus ports only)
    ///
    /// Example:
    ///   `out MIC{P,N}::ADC.DIFF()`         → bus_members = ["P", "N"]
    ///   `XTAL{X1,X2}` port                 → bus_members = ["X1", "X2"]
    ///   `UART0::UART.TTL(DCE)` port        → bus_members = ["TX", "RX"]   *
    ///   `[VDD_3V3,GND]::DC(3.3V)` port     → bus_members = ["VDD_3V3","GND"]
    ///   Scalar port `out DAC_OUT`            → bus_members = []
    ///
    /// (* Note: Members of system library interfaces like UART need to be
    ///   extracted from the base interface's pins table; the current implementation
    ///   only covers the directly-written syntax form; see
    ///   `phases.rs::extract_port_bus_members` for details.)
    ///
    /// `points.rs::expand_port_lanes` uses this field at endpoint resolution to
    /// expand port references into N independent NetPoints, so that the
    /// rules document §10.4 "[N×1] vs [N×1] element-by-element correspondence"
    /// is truly realized at the endpoint layer.
    pub bus_members: Vec<String>,
}

impl PortInst {
    /// Create port instance (scalar port, no members)
    pub fn new(name: &str, iotype: IOType) -> Self {
        let net_point = NetPoint::new(name, iotype.clone());
        Self {
            name: name.to_string(),
            iotype,
            net_point,
            bus_members: Vec::new(),
        }
    }

    /// Create bus port instance with members
    ///
    /// Equivalent to `new()` when `members` is empty.
    pub fn with_members(name: &str, iotype: IOType, members: Vec<String>) -> Self {
        let net_point = NetPoint::new(name, iotype.clone());
        Self {
            name: name.to_string(),
            iotype,
            net_point,
            bus_members: members,
        }
    }

    /// Is this port a N×1 bus port (members ≥ 2)
    #[inline]
    pub fn is_bus_port(&self) -> bool {
        self.bus_members.len() >= 2
    }
}

impl fmt::Display for PortInst {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)?;
        match self.iotype {
            IOType::In => write!(f, " (in)"),
            IOType::Out => write!(f, " (out)"),
            IOType::InOut => write!(f, " (io)"),
            _ => Ok(()),
        }
    }
}

// ============================================================================
// InstError - Instant Error
// ============================================================================

/// Instantiation errors
#[derive(Debug)]
pub enum InstError {
    /// Port not defined
    PortNotDefined(String),
    /// Module not found
    ModuleNotFound(String),

    /// Component not found
    ComponentNotFound(String),

    /// Connection shape mismatch
    ShapeMismatch {
        left_size: usize,
        right_size: usize,
        context: String,
    },

    /// Bus member mismatch
    BusMemberMismatch {
        bus_name: String,
        expected: Vec<String>,
        found: Vec<String>,
    },

    /// Condition evaluation failed
    ConditionEvalFailed { condition: String, context: String },

    /// Closure parameter mismatch
    ClosureParamMismatch {
        expected: usize,
        found: usize,
        context: String,
    },

    /// Unknown function call
    UnknownFunction { name: String, uri: String },

    /// General error   
    Other(String),
}

impl fmt::Display for InstError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InstError::ModuleNotFound(name) => {
                write!(f, "Module not found: '{name}'")
            }
            InstError::ComponentNotFound(name) => {
                write!(f, "Component not found: '{name}'")
            }
            InstError::ShapeMismatch {
                left_size,
                right_size,
                context,
            } => write!(
                f,
                "Shape mismatch: left={left_size}, right={right_size} ({context})"
            ),
            InstError::BusMemberMismatch {
                bus_name,
                expected,
                found,
            } => write!(
                f,
                "Bus '{bus_name}' member mismatch: expected {expected:?}, found {found:?}"
            ),
            InstError::ConditionEvalFailed { condition, context } => {
                write!(f, "Cannot evaluate condition '{condition}' in {context}")
            }
            InstError::ClosureParamMismatch {
                expected,
                found,
                context,
            } => write!(
                f,
                "Closure parameter mismatch: expected {expected}, found {found} in {context}"
            ),
            InstError::UnknownFunction { name, uri } => {
                write!(f, "Unknown function '{name}' in {uri}")
            }
            InstError::Other(msg) => write!(f, "{msg}"),
            InstError::PortNotDefined(name) => write!(f, "Port not defined: '{name}'"),
        }
    }
}

impl std::error::Error for InstError {}

// ============================================================================
// InstDiagnostic - Instant Diagnostic (Non-Fatal)
// ============================================================================

/// Diagnostic level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstDiagLevel {
    Error,
    Warning,
}

/// Instantiation diagnostic (non-fatal)
///
/// Unlike `InstError`, `InstDiagnostic` does not interrupt the instantiation process.
/// It records the problem for LSP/IDE display.
///
/// # Design motivation
/// In IDE scenarios, users often find themselves in the middle of editing
/// (missing files, undefined types, incomplete connections, etc.).
/// If the instantiation process interrupts on the first error, all subsequent modules
/// will be instantiated.
#[derive(Debug, Clone)]
pub struct InstDiagnostic {
    /// Diagnostic level
    /// Diagnostic level
    pub level: InstDiagLevel,
    /// Diagnostic code (aligns with diagnostic.rs)
    pub code: u32,
    /// Context path (module instance name, e.g. "top.sub1.sub2")
    pub context: String,
    /// Diagnostic message
    pub message: String,
}

impl InstDiagnostic {
    /// Create error diagnostic
    pub fn error(code: u32, context: &str, message: String) -> Self {
        Self {
            level: InstDiagLevel::Error,
            code,
            context: context.to_string(),
            message,
        }
    }

    /// Create warning diagnostic   
    pub fn warning(code: u32, context: &str, message: String) -> Self {
        Self {
            level: InstDiagLevel::Warning,
            code,
            context: context.to_string(),
            message,
        }
    }
}

impl fmt::Display for InstDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tag = match self.level {
            InstDiagLevel::Error => "ERROR",
            InstDiagLevel::Warning => "WARN",
        };
        write!(
            f,
            "[{}](#{}) {}: {}",
            tag, self.code, self.context, self.message
        )
    }
}

// ============================================================================
// Iter-10.1: Path Normalization
// ============================================================================

/// ── P7: pins segment normalization (same rule as expand_member_ida, converged to this single point) ──
/// Normalize the `pins` qualifier in the path to a bare physical pin id:
///   "uC.pins7"  → "uC.7"
///   "uC.pins.7" → "uC.7"
///   "pins[8:11]" already expanded upstream into pins8.. → here pins8 → 8
/// Only strip when the character(s) after "pins" (or the next segment) are pure digits, to avoid damaging real segment names.
pub(crate) fn normalize_pin_segments(path: &str) -> String {
    let segs: Vec<&str> = path.split('.').collect();
    let mut out: Vec<String> = Vec::with_capacity(segs.len());
    let mut i = 0;
    while i < segs.len() {
        let s = segs[i];
        if let Some(rest) = s.strip_prefix("pins") {
            if !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()) {
                out.push(rest.to_string()); // "pins7" → "7"
                i += 1;
                continue;
            }
            if rest.is_empty()
                && i + 1 < segs.len()
                && !segs[i + 1].is_empty()
                && segs[i + 1].bytes().all(|b| b.is_ascii_digit())
            {
                out.push(segs[i + 1].to_string()); // "pins" "." "7" → "7"
                i += 2;
                continue;
            }
        }
        out.push(s.to_string());
        i += 1;
    }
    out.join(".")
}

/// Normalize NetPoint path, eliminating different string representations of the same physical node.
///
/// Handles the following patterns:
///   1. **Duplicate suffix**: `VCC_1V2.VCC_1V2` → `VCC_1V2`
///      When the path is of the form `A.A` and the two segments are identical, remove the duplicate.
///   2. **Pin number duplication**: `uC.21.21` → `uC.21`
///      When the last two segments are identical (including numbers and non-numbers), remove the duplicate.
///   3. **Curly brace duplicate suffix**: `dc{VDD_3V3, GND}.dc{VDD_3V3, GND}` → `dc{VDD_3V3, GND}`
///      When the segments inside curly braces are repeated, remove the duplicate.
///   4. **Arrow residual rejection**: if the path contains `->` or `<-`, it is
///      considered an AST flattening failure; strip both sides of the arrow
///      and take the last valid identifier.
///   5. **pins qualifier normalization**: `uC.pins7` → `uC.7`, `uC.pins.7` → `uC.7`
///
/// # Design constraints
/// - Only handles **explicit bug artifacts**, not fuzzy matches
/// - `MIC.P` (different segment names) is unaffected
/// - `dcdc.FB` (normal component.pin) is unaffected
/// - Empty string / single-segment path returns as is
pub fn canonicalize_path(path: &str) -> String {
    if path.is_empty() {
        return path.to_string();
    }
    // ── P7: pins segment normalization (same rule as expand_member_ida, converged to this single point) ──
    let path = normalize_pin_segments(path);

    // ── 4. Arrow residual rejection: strip arrow sides and take last valid token ──
    if path.contains("->") || path.contains("<-") {
        let cleaned = path
            .split("->")
            .flat_map(|s| s.split("<-"))
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .last()
            .unwrap_or(&path);
        // Recursively normalize the cleaned result
        return canonicalize_path(cleaned);
    }

    // ── 3. Curly brace duplicate suffix ──
    // `dc{VDD_3V3, GND}.dc{VDD_3V3, GND}` → `dc{VDD_3V3, GND}`
    // Detection: Split at first `}.`, check if prefix == suffix
    if path.contains('{') && path.contains('}') {
        if let Some(close_dot) = path.find("}.") {
            let first_part = &path[..close_dot + 1]; // Includes '}'
            let second_part = &path[close_dot + 2..]; // Skip '}.', get suffix
            if first_part == second_part {
                return first_part.to_string();
            }
        }
    }

    // ── 1 & 2. Duplicate suffix ──
    if let Some(last_dot) = path.rfind('.') {
        let prefix = &path[..last_dot];
        let suffix = &path[last_dot + 1..];

        // Case 1: `A.A` — Duplicate suffix (e.g. `VCC_1V2.VCC_1V2`)
        if prefix == suffix {
            return prefix.to_string();
        }

        // Case 2: `X.Y.Y` — Duplicate suffix (e.g. `uC.21.21`, `AVDD09_CAP.AVDD09_CAP`)
        // Detection: Find last '.', check if last two segments are identical
        if let Some(prev_dot) = prefix.rfind('.') {
            let prev_suffix = &prefix[prev_dot + 1..];
            if prev_suffix == suffix {
                return prefix.to_string();
            }
        }
    }

    path.to_string()
}

/// ── ★ ITER-5: Lightweight power/ground name recognition (for `into_nets` tier-3 naming) ──
///
/// Here we **deliberately** do not call `crate::vector::graph::naming::is_power_rail`
/// —— `mc_net.rs` is in the `crate::instant` layer, while `naming.rs` is in the
/// `crate::vector` layer; cross-layer imports would break the current
/// "vector depends on instant" one-way dependency graph. We maintain a local
/// **converged subset**: only used when naming a net (everything that reaches
/// here has already been skipped by tier1/tier2; any non-standard misrecognition
/// at most gives the net a **meaningful but slightly literary** name without
/// affecting electrical connections — risk is very low).
///
/// Recognition rules are consistent with `naming::is_power_rail` (simplified version):
///   - exact power: VCC / VDD / VBUS / V3P3 / V5P0 / V1P8 / VPP / AVDD
///   - prefix power: VCC* / VDD* / V3V* / V5V* / V1V*
///   - exact ground: GND / VSS / AGND / DGND / PGND
///   - prefix ground: GND* / VSS*
///   - voltage patterns (`3V3` / `5V0`) are treated as power
///
/// Ground leaf-name recognition (exact + prefix), shared by
/// [`looks_like_power_rail`] and the raw-layer sub-module internal ground tie
/// propagation in `build_net_table`. Mirrors `naming::is_ground`'s leaf
/// classification (EXACT_GROUND + PREFIX_GROUND), kept local to the `instant`
/// layer — no `crate::vector` import (one-way dependency graph).
pub fn is_ground_name(name: &str) -> bool {
    let u = name.to_uppercase();
    const EXACT_GROUND: &[&str] = &["GND", "VSS", "AGND", "DGND", "PGND"];
    if EXACT_GROUND.contains(&u.as_str()) {
        return true;
    }
    const PREFIX_GROUND: &[&str] = &["GND", "VSS"];
    PREFIX_GROUND.iter().any(|p| u.starts_with(p))
}

/// Example: `looks_like_power_rail("VDD_3V3") == true`, `..("vout") == false`,
///     `..("gnd") == true` (case-insensitive), `..("DAC_OUT") == false`.
pub fn looks_like_power_rail(name: &str) -> bool {
    let u = name.to_uppercase();
    // exact power
    const EXACT_POWER: &[&str] = &["VCC", "VDD", "VBUS", "V3P3", "V5P0", "V1P8", "VPP", "AVDD"];
    if EXACT_POWER.contains(&u.as_str()) {
        return true;
    }
    // exact + prefix ground
    if is_ground_name(name) {
        return true;
    }
    // prefix power
    const PREFIX_POWER: &[&str] = &["VCC", "VDD", "V3V", "V5V", "V1V"];
    if PREFIX_POWER.iter().any(|p| u.starts_with(p)) {
        return true;
    }
    // Voltage patterns `3V3` / `5V0` / `1V8` — simplified: digits + 'V' + digits
    let bytes = u.as_bytes();
    let mut found_v = false;
    let mut has_digit_before = false;
    let mut has_digit_after = false;
    for (i, &c) in bytes.iter().enumerate() {
        if c == b'V' && i > 0 && i < bytes.len() - 1 {
            if bytes[i - 1].is_ascii_digit() {
                has_digit_before = true;
            }
            if bytes[i + 1].is_ascii_digit() {
                has_digit_after = true;
            }
            found_v = true;
        }
    }
    found_v && has_digit_before && has_digit_after
}

// ============================================================================
// NetTable - Network table (union-find merge)
// ============================================================================

/// Network table: merges ConnectionInsts into a unified net view
///
/// Uses union-find algorithm to correctly handle transitive connections:
/// ```text
/// conn_0: VCC ~ R1     →  net "VCC": [VCC, R1, R2, GND]
/// conn_1: R1  ~ R2         (all labels on the same net)
/// conn_2: R2  ~ GND
/// ```
///
/// ── Iter-10 enhancements ──
/// On top of union-find we add:
/// 1. **Path normalization**: `ensure_point` entry calls `canonicalize_path`
/// 2. **Post-connection batch union**: after all connections are added, do a
///    second scan of all connections to ensure transitive merges of shared
///    nodes are not missed
#[derive(Debug)]
pub struct NetTable {
    /// path → index mapping
    path_to_idx: HashMap<String, usize>,
    /// all registered points
    points: Vec<NetPoint>,
    /// Union-Find parent array
    parent: Vec<usize>,
    /// port name set (preferred for net naming)
    port_names: HashSet<String>,
    /// ── Iter-10.2 ──
    /// raw point path lists of all added connections, used for batch union
    all_conn_paths: Vec<Vec<String>>,
}

impl NetTable {
    pub fn new() -> Self {
        Self {
            path_to_idx: HashMap::new(),
            points: Vec::new(),
            parent: Vec::new(),
            port_names: HashSet::new(),
            all_conn_paths: Vec::new(),
        }
    }

    /// Register port (marked as port name, prioritized for net naming)
    ///
    /// ── Iter-10: also normalize the port name ──
    pub fn register_port(&mut self, name: &str, iotype: IOType) {
        let canon = canonicalize_path(name);
        // ★ Anonymous bracket-literal ports (`in [A, B]::DC(5V)` → name
        // "[A, B]") carry no meaningful whole-port name — only their members
        // do. Registering the literal name creates an orphan 1-point stub net
        // (the body references expand to members, never the literal), e.g.
        // `[POWER_SYS, GND] (1 pts) (stub)`. Skip it.
        if canon.contains('[') {
            return;
        }
        self.port_names.insert(canon.clone());
        self.ensure_point(&canon, None, iotype.clone(), None);
        // If normalization changed the name, also add the original name to
        // the port set (prevents missing it on name fallback)
        if canon != name {
            self.port_names.insert(name.to_string());
        }
    }

    /// Add a connection (merge all points into the same net)
    ///
    /// ── Iter-9 (bugfix_report error 11) ─────────────────────────────
    /// Skip nodes where path == "NC".
    ///
    /// ── Iter-10.1: path normalization ──
    /// All paths are processed by `canonicalize_path` before registration.
    ///
    /// ── Iter-10.2: record connection paths for batch union ──
    ///
    /// ── ★ FIX-A retraction note (don't reintroduce!) ────────────────────────────
    /// An early version used to do "defensive bracket expansion" here on paths
    /// of the form `prefix.[m1, m2, ...]` —— splitting it into N sub-paths and
    /// unioning all into the first. **That was wrong**, because:
    ///
    ///   - mc_net is the union-find layer, all points within a single connection
    ///     are unioned to one root;
    ///   - the real semantics of bracket endpoints is "positional pairing" or
    ///     "broadcast": when paired with a scalar endpoint on the other side,
    ///     all members inside the bracket should **not** be unioned with each
    ///     other (otherwise VDD_3V3 and GND would share a root).
    ///   - the example project's top level has `dcdc.[VDD_3V3, GND] ~ V3V3` + `dcdc.[VCC_1V2, GND] ~ V1V2`
    ///     two connections sharing `dcdc.GND`, after expansion → all 5 main
    ///     rails unioned to the same root, the entire top half of the graph
    ///     electrically shorted (measured net 101035 has 6 endpoints mixing
    ///     V1V2/V3V3/VDD_3V3/VCC_1V2/GND together).
    ///
    /// Correct approach: the positional matching semantics of bracket endpoints
    /// is already handled properly in the separate pipeline
    /// visit.rs::build_nets_from_connections via `resolve_netpoint_v2` (which
    /// coordinates with InstTable to generate multiple sub-nets by position);
    /// that's **two independent paths** from the union-find here in mc_net.rs.
    /// The mc_net side just needs to **honestly** treat the bracket literal as
    /// a single atomic endpoint (one path string → one ensure_point), letting
    /// the InstTable dump display in the `[A, B]` form, without touching it.
    pub fn add_connection(&mut self, conn: &ConnectionInst) {
        if conn.points.is_empty() {
            return;
        }

        // ── Iter-9: NC filter ──
        // ★ Patch 2-1: also filter @_phantom_ quarantined points to keep them out of union-find merging
        let kept: Vec<&NetPoint> = conn
            .points
            .iter()
            .filter(|p| p.path != "NC" && !p.path.starts_with("@_phantom_"))
            .collect();

        if kept.is_empty() {
            return;
        }

        // ── Iter-10.1: normalize path ──
        let canon_paths: Vec<String> = kept.iter().map(|p| canonicalize_path(&p.path)).collect();

        // Effective per-point source position: prefer the point's own (e.g. a
        // pin declaration), fall back to the connection's statement span (the
        // wiring site). This is the ONLY place the net list gets point
        // positions, and it feeds net-level diagnostics (E4103 etc.) — without
        // it every net diagnostic resolves to offset 0 → file:1:1.
        let conn_src = conn.source_span.clone();
        let eff_pos =
            |p: &NetPoint| -> Option<SourcePos> { p.src_pos.clone().or_else(|| conn_src.clone()) };

        if kept.len() == 1 {
            let p = kept[0];
            self.ensure_point(
                &canon_paths[0],
                p.owner.clone(),
                p.iotype.clone(),
                eff_pos(p),
            );
            return;
        }

        // Multi-point connection: register all points and union them
        let first = self.ensure_point(
            &canon_paths[0],
            kept[0].owner.clone(),
            kept[0].iotype.clone(),
            eff_pos(kept[0]),
        );
        for (i, p) in kept[1..].iter().enumerate() {
            let other = self.ensure_point(
                &canon_paths[i + 1],
                p.owner.clone(),
                p.iotype.clone(),
                eff_pos(p),
            );
            self.union(first, other);
        }

        // ── Iter-10.2: record for batch union ──
        self.all_conn_paths.push(canon_paths);
    }

    /// Tie the given point paths into one net.
    ///
    /// Only paths **already registered** in this table are unioned; unknown
    /// paths are skipped (never auto-created as new stub points). Used by the
    /// raw-layer sub-module internal ground tie propagation in `build_net_table`:
    /// a sub-module net carrying >= 2 boundary ground points (e.g. modldo's
    /// `vin.GND ~ ldo.2 ~ vout.GND`) electrically ties those port members
    /// inside the sub-module, so their parent-scope paths (`modldo.vin.GND`,
    /// `modldo.vout.GND`) must land on one parent net too — mirrors the
    /// projection layer's mechanism (3) (dc-rail-identity-design §5.3).
    pub fn tie_paths(&mut self, paths: &[&str]) {
        let mut idxs: Vec<usize> = Vec::new();
        for p in paths {
            let canon = canonicalize_path(p);
            if let Some(&idx) = self.path_to_idx.get(&canon) {
                idxs.push(idx);
            }
        }
        if idxs.len() < 2 {
            return;
        }
        for i in 1..idxs.len() {
            self.union(idxs[0], idxs[i]);
        }
    }

    /// ── Iter-10.2: Batch union second scan ──
    ///
    /// Called after all connections are added. Scans every point in every
    /// connection; if the same path appears in multiple connections, ensures
    /// all points in those connections are unioned into the same set.
    ///
    /// This fixes scenarios like:
    /// - In `conn_A: X ~ Y` and `conn_B: X ~ Z`, X should make Y and Z connected too
    /// - Theoretically add_connection already achieves this through the idempotency
    ///   of ensure_point (X's second ensure_point returns the same idx), but we do
    ///   an explicit second verification here to ensure nothing is missed
    ///
    /// Actual effect: when different connections reference the same physical node
    /// using different string representations (which become identical after
    /// normalization), batch union ensures they are merged.
    pub fn batch_union_shared_nodes(&mut self) {
        // Collect for each path → all connection indices where it appears
        let mut path_to_conns: HashMap<String, Vec<usize>> = HashMap::new();
        for (conn_idx, paths) in self.all_conn_paths.iter().enumerate() {
            for p in paths {
                path_to_conns.entry(p.clone()).or_default().push(conn_idx);
            }
        }

        // For paths appearing in multiple connections, union all points in those connections
        for (shared_path, conn_indices) in &path_to_conns {
            if conn_indices.len() < 2 {
                continue;
            }
            // All paths of all connections should be merged into the same set
            let anchor = match self.path_to_idx.get(shared_path) {
                Some(&idx) => idx,
                None => continue,
            };
            let mut to_union: Vec<usize> = Vec::new();
            for &conn_idx in conn_indices {
                if conn_idx >= self.all_conn_paths.len() {
                    continue;
                }
                for p in &self.all_conn_paths[conn_idx] {
                    if let Some(&idx) = self.path_to_idx.get(p) {
                        to_union.push(idx);
                    }
                }
            }
            for idx in to_union {
                self.union(anchor, idx);
            }
        }
    }

    /// Consume self, return network table (net_name → connection point set)
    ///
    /// ── Iter-10: execute batch union before grouping ──
    pub fn into_nets(mut self) -> HashMap<String, Vec<NetPoint>> {
        // ── Iter-10.2: batch union ensures transitive merge is complete ──
        self.batch_union_shared_nodes();

        // Build result
        // [P0-DET] deterministic group order: auto-number naming (__net_N) uses a
        // counter over insertion order, so group order must NOT depend on HashMap
        // iteration or DSU root ids (roots differ when batch_union iterates a
        // HashMap). First-seen order (min member index) is stable regardless.
        let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut nets = HashMap::new();
        let mut order: Vec<usize> = Vec::new();
        // Anonymous nets are numbered per module by a dedicated counter
        // (`_net0`, `_net1`, ...), independent of how many named nets precede
        // them. Previously the number was `nets.len()` (insertion position
        // among ALL nets), so named nets occupying earlier slots left gaps and
        // the anonymous sequence looked arbitrary.
        let mut anon_count: usize = 0;
        for idx in 0..self.points.len() {
            let root = self.find(idx);
            let slot = groups.entry(root).or_default();
            if slot.is_empty() {
                order.push(root);
            }
            slot.push(idx);
        }
        for root in order {
            let indices = &groups[&root];
            let group_points: Vec<NetPoint> =
                indices.iter().map(|&i| self.points[i].clone()).collect();

            // Net naming priority: port name > label with no owner and <3 segments > auto-number
            //
            // ── Iter-9 (bugfix_report error 14) ──────────────────────────
            // Exclude multi-segment paths with segment count >= 3 as net name candidates
            //
            // ── Iter-10: normalized matching against port_names ──
            //
            // ── ★ ITER-5: add "last-segment power/ground name" as fallback candidate ──
            // The old logic only fell back to `__net_N` after both tier1 and tier2
            // missed, leading to cases like:
            //   group_points = [main.ldo.gnd, main.dcdc.GND]
            //   group_points = [main.ldo.vout, main.dcdc.VDD_3V3]
            // These "SubModule↔SubModule internal bridge power/ground connections
            // with both-side path segment counts >= 2 and neither in port_names"
            // were named `__net_10` / `__net_11`, rendered with anonymous strings
            // as labels, geometrically adjacent to V3V3/GND but with no semantic
            // association.
            //
            // Fix: when both tier1/tier2 miss, first scan group_points, find a
            // point whose **last segment** looks like a power/ground name (using
            // the lightweight helper `looks_like_power_rail` in this file, to
            // avoid cross-layer dependency from `instant` importing `naming` of
            // `vector`). If hit, use that name (normalized UPPER) as the net name.
            //
            // Examples:
            //   [ldo.gnd, dcdc.GND]      → "GND"
            //   [ldo.vout, dcdc.VDD_3V3] → "VDD_3V3"
            // This way the downstream `from_block` `naming::classify_net` can
            // correctly classify Power/Ground, and ITER-4's hyperedge merge can
            // also find these nets by "duplicate name".
            //
            // Note: we keep the matched point's FULL path (e.g. `va.GND`,
            // `main.ldo.gnd`) as the net name — see the "Strict DC rail identity"
            // note below. The last segment is only probed by `looks_like_power_rail`
            // to decide *whether* a point qualifies as the net name candidate, never
            // as the name itself. Degenerate case: a path of only 1 segment that
            // matches a power name was already caught by tier2, so we never reach
            // here with it.
            let net_name = group_points
                .iter()
                .find(|p| {
                    self.port_names.contains(&p.path)
                        || self.port_names.contains(&canonicalize_path(&p.path))
                })
                .map(|p| p.path.clone())
                .or_else(|| {
                    group_points
                        .iter()
                        .find(|p| p.owner.is_none() && p.path.matches('.').count() < 2)
                        .map(|p| p.path.clone())
                })
                .or_else(|| {
                    // scan full paths for power/ground names
                    //
                    // ── Strict DC rail identity ──────────────────────────
                    // The net name keeps the FULL path of the matched point
                    // (e.g. `va.GND`, `main.ldo.gnd`) instead of the normalized
                    // last segment (`GND`). Rationale: different DC rails in one
                    // module may carry different grounds (`va.GND` != `vb.GND`);
                    // last-segment normalization would name both `GND`, and the
                    // downstream duplicate-name hyperedge merge would wrongly
                    // short them together. Full-path names keep every rail
                    // traceable and never merge by name.
                    group_points
                        .iter()
                        .filter_map(|p| {
                            let last = p.path.rsplit('.').next()?;
                            if looks_like_power_rail(last) {
                                Some(p.path.clone())
                            } else {
                                None
                            }
                        })
                        .next()
                })
                .unwrap_or_else(|| {
                    let name = format!("_net{anon_count}");
                    anon_count += 1;
                    name
                });

            nets.insert(net_name, group_points);
        }

        nets
    }

    // ==== Internal methods ====

    /// Ensure the point is registered, return its index
    ///
    /// ── Iter-10.1: path already normalized at the caller, used directly here ──
    ///
    /// `src_pos` is stored when a point is first created; if the point already
    /// exists with no position and a more specific one arrives (e.g. a port
    /// first registered bare, then referenced by a connection), it is
    /// back-filled. The first position wins on ties.
    fn ensure_point(
        &mut self,
        path: &str,
        owner: Option<String>,
        iotype: IOType,
        src_pos: Option<SourcePos>,
    ) -> usize {
        if let Some(&idx) = self.path_to_idx.get(path) {
            if let Some(sp) = src_pos {
                if self.points[idx].src_pos.is_none() {
                    self.points[idx].src_pos = Some(sp);
                }
            }
            return idx;
        }
        let idx = self.points.len();
        self.path_to_idx.insert(path.to_string(), idx);
        self.points.push(NetPoint {
            path: path.to_string(),
            owner,
            iotype,
            src_pos,
            member_name: None,
            same_name_pads: Vec::new(),
        });
        self.parent.push(idx);
        idx
    }

    /// Union-Find: find with path compression
    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.find(self.parent[x]);
        }
        self.parent[x]
    }

    /// Union-Find: union two sets (optimize rank by index)
    fn union(&mut self, a: usize, b: usize) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra != rb {
            // Let the smaller index be the root (stability)
            if ra < rb {
                self.parent[rb] = ra;
            } else {
                self.parent[ra] = rb;
            }
        }
    }
}

// ============================================================================
// Iter-10 + Iter-12.3: Unit tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canonicalize_duplicate_suffix() {
        assert_eq!(canonicalize_path("VCC_1V2.VCC_1V2"), "VCC_1V2");
        assert_eq!(canonicalize_path("AVDD09_CAP.AVDD09_CAP"), "AVDD09_CAP");
    }

    #[test]
    fn test_canonicalize_double_pin_number() {
        assert_eq!(canonicalize_path("uC.21.21"), "uC.21");
    }

    #[test]
    fn test_canonicalize_curly_brace_repeat() {
        assert_eq!(
            canonicalize_path("dc{VDD_3V3, GND}.dc{VDD_3V3, GND}"),
            "dc{VDD_3V3, GND}"
        );
        assert_eq!(canonicalize_path("mic{1, 2}.mic{1, 2}"), "mic{1, 2}");
    }

    #[test]
    fn test_canonicalize_arrow_residual() {
        assert_eq!(
            canonicalize_path("dc{VDD_3V3} -> wm7121{VCC}.dc{VDD_3V3} -> wm7121{VCC}"),
            "wm7121{VCC}"
        );
    }

    #[test]
    fn test_canonicalize_no_change() {
        // Normal paths should not be modified
        assert_eq!(canonicalize_path("lp322dcdc.FB"), "lp322dcdc.FB");
        assert_eq!(canonicalize_path("MIC.P"), "MIC.P");
        assert_eq!(canonicalize_path("@CAP1.1"), "@CAP1.1");
        assert_eq!(canonicalize_path("VCC"), "VCC");
        assert_eq!(canonicalize_path(""), "");
    }

    #[test]
    fn test_batch_union_merges_shared_nodes() {
        let mut table = NetTable::new();

        // Simulate moddcdc's FB node scenario:
        // conn_0: @RES6.2 ~ lp322dcdc.FB
        // conn_1: lp322dcdc.FB ~ @RES7.1
        // conn_2: @CAP8.1 ~ lp322dcdc.FB
        let conn0 = ConnectionInst::new(
            0,
            vec![
                NetPoint::with_owner("@RES6.2", "@RES6", IOType::None),
                NetPoint::with_owner("lp322dcdc.FB", "lp322dcdc", IOType::None),
            ],
        );
        let conn1 = ConnectionInst::new(
            1,
            vec![
                NetPoint::with_owner("lp322dcdc.FB", "lp322dcdc", IOType::None),
                NetPoint::with_owner("@RES7.1", "@RES7", IOType::None),
            ],
        );
        let conn2 = ConnectionInst::new(
            2,
            vec![
                NetPoint::with_owner("@CAP8.1", "@CAP8", IOType::None),
                NetPoint::with_owner("lp322dcdc.FB", "lp322dcdc", IOType::None),
            ],
        );

        table.add_connection(&conn0);
        table.add_connection(&conn1);
        table.add_connection(&conn2);

        let nets = table.into_nets();

        // All 4 points should merge into 1 net
        assert_eq!(nets.len(), 1, "Expected 1 merged net, got {}", nets.len());
        let (_, points) = nets.iter().next().unwrap();
        assert_eq!(points.len(), 4, "Expected 4 points, got {}", points.len());
    }

    #[test]
    fn test_canonicalize_merges_duplicate_suffix_paths() {
        let mut table = NetTable::new();

        // Simulate: one connection uses "VCC_1V2", another uses "VCC_1V2.VCC_1V2"
        let conn0 = ConnectionInst::new(
            0,
            vec![
                NetPoint::new("VCC_1V2", IOType::None),
                NetPoint::with_owner("@RES6.1", "@RES6", IOType::None),
            ],
        );
        let conn1 = ConnectionInst::new(
            1,
            vec![
                NetPoint::new("VCC_1V2.VCC_1V2", IOType::None),
                NetPoint::with_owner("@CAP5.1", "@CAP5", IOType::None),
            ],
        );

        table.add_connection(&conn0);
        table.add_connection(&conn1);

        let nets = table.into_nets();

        // After normalization the two VCC_1V2 are the same point, should merge into 1 3-pts net
        assert_eq!(nets.len(), 1, "Expected 1 merged net, got {}", nets.len());
        let (_, points) = nets.iter().next().unwrap();
        assert_eq!(points.len(), 3, "Expected 3 points, got {}", points.len());
    }
}
