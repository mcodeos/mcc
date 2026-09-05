// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Rule registry — single declarative ledger for every check rule.
//!
//! Design: `mcd/doc/check-rule-registry-design.md` (§2 descriptor, §4 add
//! path, §5-5 order, §8-5 override store). Stage 1 landed the FlatErc scope:
//! all 16 `nets` P/V/C/D rules are declared here in execution order, and
//! `nets::run_net_checks` now drives its sequence from this table (the former
//! hand-written call table is gone). Execution context stays in the existing
//! host fns (this module is the registry that orders and describes them), so
//! no diagnostic output changes.
//!
//! Stage 2 landed the AssemblyGate scope: every netcheck R-series row
//! (`instant::netcheck`) is registered here with a numeric code from the
//! central `errcodes` table (design §5-1) plus the §7.3 sink/gate axes. The
//! netcheck runner derives its per-row level and display label from this
//! table, so a row's identity lives in one place.
//!
//! Stage 3 (pins part) landed the Declaration scope: the two pin-usage checks
//! in `validation::pins` (E5155 unused-pin / E5156 conflicting-options) are
//! declared here with the same typed-executor shape as FlatErc, and
//! `pins::run_pin_checks` drives its sequence from `DECL_RULES`.
//!
//! Stage 3 (PostParse part) landed the semantic PostParse scope: the 93 codes
//! the `validation` CheckRegistry hosts emit are registered here at per-code
//! granularity (`POSTPARSE_RULES`), ordered by `with_defaults()` host
//! registration (design §6 stage-3 remainder). Rows are data-only descriptors
//! (`PostParseRule { meta, host }`); the object hosts stay the executor and
//! `CheckRegistry::run_post_parse` is untouched, so diagnostic output is
//! byte-identical. Rows carry the §2.2 descriptor plus plane/acceptance
//! defaults (CoreMechanism / Legal) and the owning host module name for
//! queries and the later lock-ledger / override projections.
//!
//! The skeleton answers two open mechanism questions:
//!
//! 1. **Declaration order is execution order.** Rust offers no stable
//!    cross-module collection of `static`/`const` items (inventory/linkme
//!    order is not source order), so §5-5 is anchored by one ordered,
//!    per-scope table literal: `FLAT_ERC_RULES` / `GATE_RULES`. Adding a rule
//!    means declaring it in that table, and the runner order is the table
//!    order.
//! 2. **Owner shape.** FlatErc checks share one context signature, so a typed
//!    `run: fn(&InstTable, &mut Vec<NetCheckResult>)` pointer is `const`-usable
//!    and type-checks against the real host fn. Contexts differ across scopes
//!    (PostParse/AssemblyGate/Declaration/viz), so one global typed table is
//!    impossible: the catalog exposes data-only metadata for queries, while
//!    each scope keeps its own typed executor table. The AssemblyGate host
//!    fns additionally need the netcheck `Index`, so their table carries the
//!    host fn *name* (`host`) instead of a uniform pointer and the executor
//!    stays in `instant::netcheck`.
//!
//! Queries are consumed by tests today and by `cmds/rules.rs`/RPC/MCP once
//! stage 5 lands, so this module follows the repo convention of a local
//! `allow(dead_code)` for registry-driven items.
#![allow(dead_code)]

use crate::instant::insttab::InstTable;
use crate::semantic::validation::nets::{
    check_backfeed, check_driver_conflict, check_floating_inputs, check_floating_outputs,
    check_nc_connected, check_pin_count_mismatch, check_port_io_mismatch, check_power_nets,
    check_pullup_degenerate, check_single_point_nets, check_unconnected_outputs,
    check_undriven_nets, check_unselected_abstract, check_unused_module_ports,
    check_unwired_instances, check_voltage_mismatch, NetCheckResult,
};
use crate::semantic::validation::pins::{
    check_conflicting_pins, check_unused_pins, PinCheckResult,
};
use crate::semantic::validation::CheckSeverity;

// ============================================================================
// Category axes (§2.3)
// ============================================================================

/// Execution scope — which stage runner owns the rule (§2.3 `scope`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleScope {
    /// Semantic validation before flattening (existing `CheckRegistry`).
    PostParse,
    /// R-series report checks gating `build --viz`.
    AssemblyGate,
    /// Flattened-net ERC truth (P/V/C/D series).
    FlatErc,
    /// Pin/declaration semantics refreshed per file.
    Declaration,
    /// Viz layout checks — registered only; execution stays in the viz pipe.
    VizLayout,
}

/// Content (domain) axis — draft values from the design §2.3; expand only via
/// design review, never ad hoc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleDomain {
    /// Connectivity / direction.
    Connectivity,
    /// Power and ground.
    Power,
    /// Bus hierarchy.
    BusHierarchy,
    /// Pin declaration semantics.
    PinDecl,
    /// IO attributes.
    IO,
    /// Signal integrity.
    SignalIntegrity,
    /// Structural shape.
    Structure,
    /// Naming style.
    NamingStyle,
    /// Duplicates.
    Duplicate,
    /// Cross-reference integrity.
    RefIntegrity,
    /// Electrical rating.
    Rating,
}

// ============================================================================
// Governance axes (§7, analysis-design-verification loop)
// ============================================================================

/// Ownership plane (§7.1) — which layer of the loop owns the rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RulePlane {
    /// Language base mechanism; the registry owns execution.
    CoreMechanism,
    /// Domain-package contract; registered only, execution stays in the host.
    DomainPackage,
    /// Simulation / fulfillment assertion; registered only.
    SimFulfillment,
}

/// Acceptance reading (§7.2) — legal compliance vs declared intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acceptance {
    /// The circuit obeys a written language rule.
    Legal,
    /// A declared capability/interface contract holds.
    Contract,
    /// The declared intent is fulfilled (assertions / effect comparison).
    Fulfillment,
}

/// Result destination (§7.3 sink).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleSink {
    /// The LSP/envelope diagnostic channel.
    Envelope,
    /// A build-gate report table (the netcheck R report).
    GateReport,
    /// A projection-owned problems store.
    OwnedStore,
}

/// Whether a firing blocks the build (§7.3 gate).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateKind {
    /// Report only; does not turn the build red.
    Advisory,
    /// The `build --viz` gate set derives from this flag.
    Blocking,
}

/// How often the rule runs (§7.3 cadence).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cadence {
    /// Full-circuit, once per build.
    PerCircuit,
    /// Incremental per-file refresh (editor).
    Incremental,
}

/// Default gate for a severity: error-level rules block the build, anything
/// softer is advisory. This is the anchoring invariant (severity Error
/// implies gate Blocking) that keeps today's gate behavior byte-identical
/// while making the gate set derivable from the catalog (§7.3).
pub const fn gate_for(severity: CheckSeverity) -> GateKind {
    match severity {
        CheckSeverity::Error => GateKind::Blocking,
        _ => GateKind::Advisory,
    }
}

// ============================================================================
// Descriptor (§2.2, data-only metadata shared by every scope)
// ============================================================================

/// Machine-actionable follow-up a fired rule carries (§7.4 closed-loop
/// writeback axis). Today every row is `None`: the registry still has no
/// editor code-action, so the diagnostic text is the whole payload. When an
/// IDE/mcext codeAction lands, a rule assigns its fix kind here and the
/// envelope builder can populate `suggestions` from it; the axis is a filter
/// key, not free text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum FixKind {
    /// No machine quick-fix; the diagnostic text is the whole payload.
    #[default]
    None,
    /// A deterministic source edit the AI/IDE loop can apply directly.
    QuickFix,
    /// A suggested follow-up (for example "grant an override") surfaced as a
    /// diagnostic suggestion rather than a source edit.
    Suggestion,
}

// ============================================================================
// Stable string forms of the §2.3/§2.5 axes — the spelling every consumer
// surface uses (`mcc rules --scope flat-erc`, RPC `rules.list` params, MCP
// tool args, the JSON projection and the text views). Keep one spelling per
// axis so the CLI/RPC/MCP bytes stay identical (design §8 projection).
// ============================================================================

/// Kebab-case axis name used by the §8 read/write surfaces.
impl RuleScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PostParse => "post-parse",
            Self::AssemblyGate => "assembly-gate",
            Self::FlatErc => "flat-erc",
            Self::Declaration => "declaration",
            Self::VizLayout => "viz-layout",
        }
    }
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "post-parse" | "postparse" => Self::PostParse,
            "assembly-gate" | "assemblygate" => Self::AssemblyGate,
            "flat-erc" | "flaterc" => Self::FlatErc,
            "declaration" | "decl" => Self::Declaration,
            "viz-layout" | "vizlayout" => Self::VizLayout,
            _ => return None,
        })
    }
}

impl RuleDomain {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Connectivity => "connectivity",
            Self::Power => "power",
            Self::BusHierarchy => "bus-hierarchy",
            Self::PinDecl => "pin-decl",
            Self::IO => "io",
            Self::SignalIntegrity => "signal-integrity",
            Self::Structure => "structure",
            Self::NamingStyle => "naming-style",
            Self::Duplicate => "duplicate",
            Self::RefIntegrity => "ref-integrity",
            Self::Rating => "rating",
        }
    }
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "connectivity" => Self::Connectivity,
            "power" => Self::Power,
            "bus-hierarchy" | "bushierarchy" => Self::BusHierarchy,
            "pin-decl" | "pindecl" => Self::PinDecl,
            "io" => Self::IO,
            "signal-integrity" | "signalintegrity" => Self::SignalIntegrity,
            "structure" => Self::Structure,
            "naming-style" | "namingstyle" => Self::NamingStyle,
            "duplicate" => Self::Duplicate,
            "ref-integrity" | "refintegrity" => Self::RefIntegrity,
            "rating" => Self::Rating,
            _ => return None,
        })
    }
}

impl RulePlane {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CoreMechanism => "core-mechanism",
            Self::DomainPackage => "domain-package",
            Self::SimFulfillment => "sim-fulfillment",
        }
    }
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "core-mechanism" | "coremechanism" => Self::CoreMechanism,
            "domain-package" | "domainpackage" => Self::DomainPackage,
            "sim-fulfillment" | "simfulfillment" => Self::SimFulfillment,
            _ => return None,
        })
    }
}

impl Acceptance {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Legal => "legal",
            Self::Contract => "contract",
            Self::Fulfillment => "fulfillment",
        }
    }
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "legal" => Self::Legal,
            "contract" => Self::Contract,
            "fulfillment" => Self::Fulfillment,
            _ => return None,
        })
    }
}

impl RuleSink {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Envelope => "envelope",
            Self::GateReport => "gate-report",
            Self::OwnedStore => "owned-store",
        }
    }
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "envelope" => Self::Envelope,
            "gate-report" | "gatereport" => Self::GateReport,
            "owned-store" | "ownedstore" => Self::OwnedStore,
            _ => return None,
        })
    }
}

impl GateKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Advisory => "advisory",
            Self::Blocking => "blocking",
        }
    }
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "advisory" => Self::Advisory,
            "blocking" => Self::Blocking,
            _ => return None,
        })
    }
}

impl Cadence {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PerCircuit => "per-circuit",
            Self::Incremental => "incremental",
        }
    }
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "per-circuit" | "percircuit" => Self::PerCircuit,
            "incremental" => Self::Incremental,
            _ => return None,
        })
    }
}

impl FixKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::QuickFix => "quick-fix",
            Self::Suggestion => "suggestion",
        }
    }
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "none" => Self::None,
            "quick-fix" | "quickfix" => Self::QuickFix,
            "suggestion" => Self::Suggestion,
            _ => return None,
        })
    }
}

/// Rule descriptor metadata. This is the part every scope has in common, so
/// catalog queries (by code/scope/domain/severity) operate on it.
#[derive(Debug, Clone)]
pub struct RuleMeta {
    /// Numeric code constant from the central `errcodes` table — the single
    /// source of a rule's identity.
    pub code: u32,
    /// Stable rule tag (the former hardcoded `check:` string).
    pub name: &'static str,
    /// One-line human title.
    pub title: &'static str,
    /// Default severity; the override store (§8-5) may downgrade it later.
    pub severity: CheckSeverity,
    /// Execution scope.
    pub scope: RuleScope,
    /// Content category.
    pub domain: RuleDomain,
    /// Industry-catalog family A-K when the rule has one (else `None`).
    pub family: Option<&'static str>,
    /// Help text / doc reference.
    pub doc: &'static str,
    /// Test-lock reference used by the lock-ledger projection.
    pub lock: &'static str,
    /// Whether severity overrides / allows are permitted at all (§7.4/§8-5).
    /// Errors are not suppressible unless explicitly granted.
    pub overridable: bool,
    /// Quick-fix kind (§7.4); `None` today for every row.
    pub fix: FixKind,
    /// Ownership plane (§7.1).
    pub plane: RulePlane,
    /// Acceptance reading (§7.2).
    pub acceptance: Acceptance,
    /// Result destination (§7.3).
    pub sink: RuleSink,
    /// Gate behavior for `build --viz` (§7.3); derived from severity today.
    pub gate: GateKind,
    /// Run cadence (§7.3).
    pub cadence: Cadence,
}

/// FlatErc-scoped rule: registry metadata plus a typed executor. All FlatErc
/// checks share the same context signature, so the `run` pointer is typed and
/// `const`-usable; the table order is the runner order (§5-5).
#[derive(Debug, Clone)]
pub struct FlatErcRule {
    pub meta: RuleMeta,
    /// The host check fn invoked for this rule in the FlatErc context.
    pub run: fn(&InstTable, &mut Vec<NetCheckResult>),
}

/// Declare one FlatErc rule as a table element. Fields mirror the §2.2
/// descriptor; `scope` is fixed to `FlatErc` and `owner` is the host fn item.
/// Governance values are the FlatErc defaults (CoreMechanism / Legal /
/// Envelope / gate derived from severity / PerCircuit); a rule that differs
/// on an axis needs an explicit adjudication, not a silent macro default.
macro_rules! declare_flat_erc_rule {
    (
        code = $code:expr,
        name = $name:literal,
        title = $title:literal,
        severity = $sev:ident,
        domain = $dom:ident,
        family = $fam:expr,
        doc = $doc:literal,
        lock = $lock:literal,
        overridable = $ov:expr,
        owner = $owner:path,
    ) => {
        FlatErcRule {
            meta: RuleMeta {
                code: $code,
                name: $name,
                title: $title,
                severity: CheckSeverity::$sev,
                scope: RuleScope::FlatErc,
                domain: RuleDomain::$dom,
                family: $fam,
                doc: $doc,
                lock: $lock,
                overridable: $ov,
                fix: FixKind::None,
                plane: RulePlane::CoreMechanism,
                acceptance: Acceptance::Legal,
                sink: RuleSink::Envelope,
                gate: gate_for(CheckSeverity::$sev),
                cadence: Cadence::PerCircuit,
            },
            run: $owner,
        }
    };
}

/// FlatErc rules in execution order (= declaration order, §5-5). This is the
/// full 16-rule migration of `nets::run_net_checks` (stage 1); the order below
/// reproduces the former hand-written call table byte-for-byte. E4101 (the
/// pilot) stays the first entry, ahead of E4102/E4103.
///
/// `domain` values are the initial content-axis allocation for the FlatErc
/// scope (reviewable). `family` is filled only where the content catalog
/// already adjudicates the rule (E4101 = "A" per the design pilot); the rest
/// wait for the erc-rules-catalog reconciliation pass. `lock` names the main
/// FlatErc lock file today; the per-code lock-ledger projection lands later.
/// `overridable` stays false for all — nothing is suppressible until the
/// override store (§8-5) grants it per rule.
pub static FLAT_ERC_RULES: &[FlatErcRule] = &[
    // P1
    declare_flat_erc_rule! {
        code = crate::errcodes::NET_MULTI_DRIVE,
        name = "driver-conflict",
        title = "multiple drivers on one net",
        severity = Error,
        domain = Connectivity,
        family = Some("A"),
        doc = "More than one Out pin drives the same net; possible short circuit.",
        lock = "tests/flatten_net_check_diagnostics.rs",
        overridable = false,
        owner = check_driver_conflict,
    },
    // P2
    declare_flat_erc_rule! {
        code = crate::errcodes::NET_NO_DRIVER,
        name = "undriven-net",
        title = "net has inputs but no driver",
        severity = Warning,
        domain = Connectivity,
        family = None,
        doc = "A net with input endpoints carries no output or power driver.",
        lock = "tests/flatten_net_check_diagnostics.rs",
        overridable = false,
        owner = check_undriven_nets,
    },
    // P5
    declare_flat_erc_rule! {
        code = crate::errcodes::NET_INPUT_UNCONNECTED,
        name = "floating-input",
        title = "input pin is not connected",
        severity = Warning,
        domain = Connectivity,
        family = None,
        doc = "An input pin or port is not connected to any net.",
        lock = "tests/flatten_net_check_diagnostics.rs",
        overridable = false,
        owner = check_floating_inputs,
    },
    // P6
    declare_flat_erc_rule! {
        code = crate::errcodes::NET_NC_CONNECTED,
        name = "nc-connected",
        title = "NC port is connected to a net",
        severity = Warning,
        domain = Connectivity,
        family = None,
        doc = "An intentionally unconnected NC port appears on a net.",
        lock = "tests/flatten_net_check_diagnostics.rs",
        overridable = false,
        owner = check_nc_connected,
    },
    // P7
    declare_flat_erc_rule! {
        code = crate::errcodes::NET_OUTPUT_UNDRIVEN,
        name = "unconnected-output",
        title = "output pin drives nothing",
        severity = Warning,
        domain = Connectivity,
        family = None,
        doc = "An output pin or port is connected to no net.",
        lock = "tests/flatten_net_check_diagnostics.rs",
        overridable = false,
        owner = check_unconnected_outputs,
    },
    // P8
    declare_flat_erc_rule! {
        code = crate::errcodes::NET_BACKFEED_RISK,
        name = "backfeed-risk",
        title = "output on a power-supply net",
        severity = Warning,
        domain = Power,
        family = None,
        doc = "A net carries both an output and a power supply; backfeed risk.",
        lock = "tests/flatten_net_check_diagnostics.rs",
        overridable = false,
        owner = check_backfeed,
    },
    // P9
    declare_flat_erc_rule! {
        code = crate::errcodes::NET_INSTANCE_UNCONNECTED,
        name = "unwired-instance",
        title = "instance has no wired pins",
        severity = Warning,
        domain = Connectivity,
        family = None,
        doc = "A component instance has pins yet none connects to a net.",
        lock = "tests/flatten_net_check_diagnostics.rs",
        overridable = false,
        owner = check_unwired_instances,
    },
    // P3+P4
    declare_flat_erc_rule! {
        code = crate::errcodes::NET_VOLTAGE_MISMATCH,
        name = "voltage-mismatch",
        title = "power pins declare incompatible voltages",
        severity = Error,
        domain = Power,
        family = None,
        doc = "Two supply pins on one net declare voltage sets with no common value.",
        lock = "tests/flatten_net_check_diagnostics.rs",
        overridable = false,
        owner = check_voltage_mismatch,
    },
    // V1
    declare_flat_erc_rule! {
        code = crate::errcodes::NET_OUTPUTS_NO_INPUT,
        name = "port-io-mismatch",
        title = "outputs and power with no input",
        severity = Warning,
        domain = IO,
        family = None,
        doc = "A net has multiple outputs plus power but no input.",
        lock = "tests/flatten_net_check_diagnostics.rs",
        overridable = false,
        owner = check_port_io_mismatch,
    },
    // power net count summary
    declare_flat_erc_rule! {
        code = crate::errcodes::NET_POWER_NET_COUNT,
        name = "power-net-count",
        title = "power net count is high",
        severity = Info,
        domain = Power,
        family = None,
        doc = "Design carries many power nets; consolidation review advisory.",
        lock = "tests/flatten_net_check_diagnostics.rs",
        overridable = false,
        owner = check_power_nets,
    },
    // C4
    declare_flat_erc_rule! {
        code = crate::errcodes::NET_MODULE_PORT_UNCONNECTED,
        name = "unused-module-port",
        title = "module port is not connected",
        severity = Warning,
        domain = IO,
        family = None,
        doc = "A module boundary port is not connected to any net.",
        lock = "tests/flatten_net_check_diagnostics.rs",
        overridable = false,
        owner = check_unused_module_ports,
    },
    // self-loop / isolated point
    declare_flat_erc_rule! {
        code = crate::errcodes::NET_DANGLING_ENDPOINT,
        name = "single-point-net",
        title = "net has a single endpoint",
        severity = Warning,
        domain = Connectivity,
        family = None,
        doc = "A net holds exactly one endpoint; possible dangling connection.",
        lock = "tests/flatten_net_check_diagnostics.rs",
        overridable = false,
        owner = check_single_point_nets,
    },
    // pin count vs definition
    declare_flat_erc_rule! {
        code = crate::errcodes::NET_PARTIAL_CONNECTION,
        name = "pin-count-mismatch",
        title = "instance pins partly connected",
        severity = Warning,
        domain = PinDecl,
        family = None,
        doc = "An instance has fewer connected pins than its component defines.",
        lock = "tests/flatten_net_check_diagnostics.rs",
        overridable = false,
        owner = check_pin_count_mismatch,
    },
    // abstract-variant (P3)
    declare_flat_erc_rule! {
        code = crate::errcodes::ABSTRACT_PART_UNSELECTED,
        name = "abstract-unselected",
        title = "abstract instance has no variant",
        severity = Warning,
        domain = Structure,
        family = None,
        doc = "An abstract component instance is placed but unselected; BOM must pick a variant.",
        lock = "tests/flatten_net_check_diagnostics.rs",
        overridable = false,
        owner = check_unselected_abstract,
    },
    // floating outputs (output variant of P5)
    declare_flat_erc_rule! {
        code = crate::errcodes::NET_BIDIR_UNCONNECTED,
        name = "floating-bidirectional",
        title = "bidirectional port is not connected",
        severity = Warning,
        domain = Connectivity,
        family = None,
        doc = "A bidirectional port is not connected to any net.",
        lock = "tests/flatten_net_check_diagnostics.rs",
        overridable = false,
        owner = check_floating_outputs,
    },
    // D7 (network-level pullup degradation)
    declare_flat_erc_rule! {
        code = crate::errcodes::PULLUP_DEGENERATE,
        name = "pullup-degenerate",
        title = "pullup degraded to signal bridge",
        severity = Warning,
        domain = Connectivity,
        family = None,
        doc = "A Pullup/Pulldown instance bridges two signals with neither end on a rail.",
        lock = "tests/flatten_net_check_diagnostics.rs",
        overridable = false,
        owner = check_pullup_degenerate,
    },
];

// ============================================================================
// Declaration scope (pins / declaration semantics)
// ============================================================================

/// Declaration-scoped rule row. Pin-usage checks share the FlatErc context
/// shape (`&InstTable` → results), so `run` is a typed fn pointer exactly like
/// `FlatErcRule::run`; the table order is the runner order (§5-5).
#[derive(Debug, Clone)]
pub struct DeclRule {
    pub meta: RuleMeta,
    /// The host check fn invoked for this rule in the Declaration context.
    pub run: fn(&InstTable, &mut Vec<PinCheckResult>),
}

/// Declare one Declaration-scope rule as a table element. `scope` is fixed to
/// `Declaration`. Governance defaults mirror the other core scopes
/// (CoreMechanism / Legal / Envelope / gate derived from severity).
/// `cadence` stays `PerCircuit`: the current pin-usage runner executes on a
/// full flattened circuit (`mcc check --pins`), not on per-file editor deltas;
/// the editor-incremental Declaration cadence applies once per-file semantic
/// declaration checks migrate here.
macro_rules! declare_decl_rule {
    (
        code = $code:expr,
        name = $name:literal,
        title = $title:literal,
        severity = $sev:ident,
        domain = $dom:ident,
        doc = $doc:literal,
        lock = $lock:literal,
        owner = $owner:path,
    ) => {
        DeclRule {
            meta: RuleMeta {
                code: $code,
                name: $name,
                title: $title,
                severity: CheckSeverity::$sev,
                scope: RuleScope::Declaration,
                domain: RuleDomain::$dom,
                family: None,
                doc: $doc,
                lock: $lock,
                overridable: false,
                fix: FixKind::None,
                plane: RulePlane::CoreMechanism,
                acceptance: Acceptance::Legal,
                sink: RuleSink::Envelope,
                gate: gate_for(CheckSeverity::$sev),
                cadence: Cadence::PerCircuit,
            },
            run: $owner,
        }
    };
}

/// Declaration-scope rules in execution order (= declaration order, §5-5) —
/// the migrated `pins::run_pin_checks` sequence (stage 3). Both rows are the
/// flatten-backed pin-usage checks of `validation/pins.rs`.
pub static DECL_RULES: &[DeclRule] = &[
    // §4.2 check 1
    declare_decl_rule! {
        code = crate::errcodes::PIN_UNCONNECTED,
        name = "unused-pin",
        title = "pin is not connected to any net",
        severity = Warning,
        domain = PinDecl,
        doc = "A pin of a placed component instance connects to no net; power pins downgrade to info.",
        lock = "tests/dynamic_pin_expansion.rs",
        owner = check_unused_pins,
    },
    // §4.2 check 2
    declare_decl_rule! {
        code = crate::errcodes::PIN_CONFLICTING_OPTIONS,
        name = "conflicting-pin-options",
        title = "pin uses conflicting option names",
        severity = Warning,
        domain = PinDecl,
        doc = "A pinid is connected under two or more different option names at once.",
        lock = "tests/dynamic_pin_expansion.rs",
        owner = check_conflicting_pins,
    },
];

// ============================================================================
// AssemblyGate scope (netcheck R-series report rows)
// ============================================================================

/// AssemblyGate-scoped rule row. The netcheck host fns share no uniform
/// context signature (each needs the netcheck `Index`), so unlike FlatErc
/// there is no typed `run` pointer here: `host` records the checking fn by
/// name and the executor in `instant::netcheck` keeps its own call sequence.
/// `label` is the exact row label the netcheck report prints (the tag plus
/// its mnemonic), so the report rendering stays byte-identical while the
/// rule identity is single-sourced from the catalog.
#[derive(Debug, Clone)]
pub struct GateRule {
    /// Registry metadata (name is the report tag, e.g. "R02").
    pub meta: RuleMeta,
    /// Report row label, e.g. "R02 SHORT_PASSIVE".
    pub label: &'static str,
    /// Host checking fn name inside `instant::netcheck`.
    pub host: &'static str,
}

/// Declare one AssemblyGate rule as a table element. `scope` is fixed to
/// `AssemblyGate`, `name` is the report tag and `sink` is `GateReport`
/// (the R report is a build-gate table, not an envelope diagnostic).
macro_rules! declare_gate_rule {
    (
        code = $code:expr,
        name = $name:literal,
        label = $label:literal,
        title = $title:literal,
        severity = $sev:ident,
        domain = $dom:ident,
        family = $fam:expr,
        doc = $doc:literal,
        lock = $lock:literal,
        host = $host:literal,
    ) => {
        GateRule {
            meta: RuleMeta {
                code: $code,
                name: $name,
                title: $title,
                severity: CheckSeverity::$sev,
                scope: RuleScope::AssemblyGate,
                domain: RuleDomain::$dom,
                family: $fam,
                doc: $doc,
                lock: $lock,
                overridable: false,
                fix: FixKind::None,
                plane: RulePlane::CoreMechanism,
                acceptance: Acceptance::Legal,
                sink: RuleSink::GateReport,
                gate: gate_for(CheckSeverity::$sev),
                cadence: Cadence::PerCircuit,
            },
            label: $label,
            host: $host,
        }
    };
}

/// AssemblyGate rules — the netcheck R-series report rows in numeric order.
/// Each row's severity below is the report's per-row level today (rule-registry
/// design §5-1); level and label are looked up from this table by the netcheck
/// runner. The runner call sequence itself (R03/R04/R06 share one pass, R05 /
/// R15 are global counters) stays in `instant::netcheck`.
pub static GATE_RULES: &[GateRule] = &[
    declare_gate_rule! {
        code = crate::errcodes::GATE_LITERAL_POINT,
        name = "R01",
        label = "R01 LITERAL_POINT",
        title = "unexpanded vector reference entered the netlist",
        severity = Error,
        domain = Connectivity,
        family = Some("A"),
        doc = "An endpoint path contains `{`, `[` or `,` — a vector reference was not expanded.",
        lock = "tests/netcheck_rules.rs / tests/gate_phase1.rs",
        host = "check_r01_literal_point",
    },
    declare_gate_rule! {
        code = crate::errcodes::GATE_SHORT_PASSIVE,
        name = "R02",
        label = "R02 SHORT_PASSIVE",
        title = "two-terminal device with both pins on one net",
        severity = Error,
        domain = Connectivity,
        family = Some("A"),
        doc = "Both pins of a two-terminal device land on the same net — short circuit.",
        lock = "tests/netcheck_rules.rs",
        host = "check_r02_short_passive",
    },
    declare_gate_rule! {
        code = crate::errcodes::GATE_SHORT_RAIL,
        name = "R03",
        label = "R03 SHORT_RAIL",
        title = "supply and ground on the same net",
        severity = Error,
        domain = Power,
        family = Some("A"),
        doc = "A net contains two different power-domain names (including VDD and GND on the same net).",
        lock = "tests/netcheck_rules.rs / tests/gate_phase1.rs",
        host = "check_r03_r04_r06",
    },
    declare_gate_rule! {
        code = crate::errcodes::GATE_RAIL_ALIAS,
        name = "R03a",
        label = "R03a RAIL_ALIAS",
        title = "multiple power-domain aliases on one net",
        severity = Info,
        domain = Power,
        family = Some("A"),
        doc = "A net carries several power-domain aliases; a short if the names denote different voltages.",
        lock = "tests/netcheck_rules.rs",
        host = "check_r03_r04_r06",
    },
    declare_gate_rule! {
        code = crate::errcodes::GATE_SHORT_LANE,
        name = "R04",
        label = "R04 SHORT_LANE",
        title = "two bus members land on the same net",
        severity = Error,
        domain = BusHierarchy,
        family = Some("A"),
        doc = "Two different members of the same bus land on one net.",
        lock = "tests/netcheck_rules.rs",
        host = "check_r03_r04_r06",
    },
    declare_gate_rule! {
        code = crate::errcodes::GATE_UNRESOLVED_UNIT,
        name = "R05",
        label = "R05 UNRESOLVED_UNIT",
        title = "unit-typed argument claims no formal parameter slot",
        severity = Error,
        domain = Structure,
        family = None,
        doc = "A unit-typed argument cannot claim any formal parameter slot.",
        lock = "tests/netcheck_rules.rs",
        host = "check_r05_unresolved_unit",
    },
    declare_gate_rule! {
        code = crate::errcodes::GATE_MEGANET,
        name = "R06",
        label = "R06 MEGANET",
        title = "non-power net is suspiciously large",
        severity = Warning,
        domain = SignalIntegrity,
        family = None,
        doc = "A non-power net has too many points and spans too many devices.",
        lock = "tests/netcheck_rules.rs",
        host = "check_r03_r04_r06",
    },
    declare_gate_rule! {
        code = crate::errcodes::GATE_GHOST_INSTANCE,
        name = "R07",
        label = "R07 GHOST_INSTANCE",
        title = "net references an unregistered device",
        severity = Error,
        domain = RefIntegrity,
        family = None,
        doc = "A device referenced in a net is missing from the instance table.",
        lock = "tests/netcheck_rules.rs",
        host = "check_r07_ghost",
    },
    declare_gate_rule! {
        code = crate::errcodes::GATE_PHANTOM_PATH,
        name = "R08",
        label = "R08 PHANTOM_PATH",
        title = "endpoint path has an unregistered middle segment",
        severity = Error,
        domain = RefIntegrity,
        family = None,
        doc = "An intermediate path segment is not a registered instance — phantom path.",
        lock = "netcheck context-gated (reorg doc §8.3)",
        host = "check_r08_phantom_path",
    },
    declare_gate_rule! {
        code = crate::errcodes::GATE_FLOATING_POWER_PIN,
        name = "R09",
        label = "R09 FLOATING_PWR_PIN",
        title = "device power/ground pin is unconnected",
        severity = Warning,
        domain = Power,
        family = None,
        doc = "A device's power / ground pin is not connected.",
        lock = "tests/netcheck_rules.rs",
        host = "check_r09_floating_power",
    },
    declare_gate_rule! {
        code = crate::errcodes::GATE_SYMBOL_CONSERVATION,
        name = "R10",
        label = "R10 SYMBOL_CONSERV",
        title = "pass2 device count fell below the pass1 expectation",
        severity = Error,
        domain = Structure,
        family = None,
        doc = "Pass2 device count is less than the pass1 symbol-table device count (expectation required).",
        lock = "tests/netcheck_rules.rs",
        host = "check_r10_conservation",
    },
    declare_gate_rule! {
        code = crate::errcodes::GATE_SPLIT_RAIL,
        name = "R11",
        label = "R11 SPLIT_RAIL",
        title = "same-name power rail is split into unconnected nets",
        severity = Error,
        domain = Power,
        family = None,
        doc = "Same-name power nets inside one module are split into mutually unconnected nets.",
        lock = "netcheck context-gated (reorg doc §8.3)",
        host = "check_r11_split_rail",
    },
    declare_gate_rule! {
        code = crate::errcodes::GATE_DANGLING_PORT,
        name = "R12",
        label = "R12 DANGLING_PORT",
        title = "port net holds only its own point",
        severity = Info,
        domain = Connectivity,
        family = None,
        doc = "A port net has only itself as a point.",
        lock = "netcheck context-gated (reorg doc §8.3)",
        host = "check_r12_dangling_port",
    },
    declare_gate_rule! {
        code = crate::errcodes::GATE_ORPHAN_INSTANCE,
        name = "R14",
        label = "R14 ORPHAN_INSTANCE",
        title = "instance is registered but in no net",
        severity = Warning,
        domain = Structure,
        family = None,
        doc = "An instance is registered but does not appear in any net.",
        lock = "tests/netcheck_rules.rs",
        host = "check_r14_orphan_instance",
    },
    declare_gate_rule! {
        code = crate::errcodes::GATE_SYNTHETIC_PIN,
        name = "R15",
        label = "R15 SYNTHETIC_PIN",
        title = "synthetic terminal with no backing pin",
        severity = Warning,
        domain = Structure,
        family = None,
        doc = "A synthetic terminal (pin_id not belonging to any real pin) was created.",
        lock = "tests/netcheck_rules.rs",
        host = "check_r15_synthetic_pin",
    },
];

// ============================================================================
// Catalog queries
// ============================================================================

/// VizLayout-scope rows — the A-series layout invariants (`equi_audit`) and
/// the F-series fidelity gate tiers (`select::fidelity_gate`). These checks
/// carry string ids ("A1".."F3") and milestone gates instead of errcode
/// numbers, so the numeric tables above do not own them (v0.7 stage 4
/// adjudication): viz authors and orders the rows, and this re-export is the
/// read-only top-level aggregation entry — no row copy, execution stays
/// inside the viz pipeline. Row type: `crate::viz::layout::audit_registry::VizAuditRule`.
/// Consumers today are the lock tests in `tests` below; the §8 read surface
/// (stage 5) is the future caller, so keep this import allowed in non-test
/// builds instead of deleting the aggregation entry.
#[allow(unused_imports)]
pub use crate::viz::layout::audit_registry::viz_audit_rules as viz_layout_rules;

/// All FlatErc rules in declared (execution) order.
pub fn flat_erc_rules() -> &'static [FlatErcRule] {
    FLAT_ERC_RULES
}

/// All Declaration-scope rules in declared (execution) order.
pub fn declaration_rules() -> &'static [DeclRule] {
    DECL_RULES
}

/// All AssemblyGate rules in declared order (numeric tag order).
pub fn assembly_gate_rules() -> &'static [GateRule] {
    GATE_RULES
}

/// All PostParse-scope rules in declared (`with_defaults()` host) order.
pub fn post_parse_rules() -> &'static [PostParseRule] {
    POSTPARSE_RULES
}

/// Number of rules in the four numeric-code tables. The VizLayout rows are
/// aggregated separately through [`viz_layout_rules`] and are not part of
/// this sum (they carry string ids, not errcode codes).
pub fn rule_count() -> usize {
    FLAT_ERC_RULES.len() + DECL_RULES.len() + GATE_RULES.len() + POSTPARSE_RULES.len()
}

/// Find one rule by numeric code across every scope table.
/// Returns `None` for unknown codes.
pub fn find_rule(code: u32) -> Option<&'static RuleMeta> {
    FLAT_ERC_RULES
        .iter()
        .find(|r| r.meta.code == code)
        .map(|r| &r.meta)
        .or_else(|| {
            DECL_RULES
                .iter()
                .find(|r| r.meta.code == code)
                .map(|r| &r.meta)
        })
        .or_else(|| {
            GATE_RULES
                .iter()
                .find(|r| r.meta.code == code)
                .map(|r| &r.meta)
        })
        .or_else(|| {
            POSTPARSE_RULES
                .iter()
                .find(|r| r.meta.code == code)
                .map(|r| &r.meta)
        })
}

/// Rules whose scope matches, in declared order. Every scope table answers
/// the same query shape.
pub fn rules_in_scope(scope: RuleScope) -> Vec<&'static RuleMeta> {
    let mut v: Vec<&'static RuleMeta> = FLAT_ERC_RULES
        .iter()
        .filter(|r| r.meta.scope == scope)
        .map(|r| &r.meta)
        .collect();
    v.extend(
        DECL_RULES
            .iter()
            .filter(|r| r.meta.scope == scope)
            .map(|r| &r.meta),
    );
    v.extend(
        GATE_RULES
            .iter()
            .filter(|r| r.meta.scope == scope)
            .map(|r| &r.meta),
    );
    v.extend(
        POSTPARSE_RULES
            .iter()
            .filter(|r| r.meta.scope == scope)
            .map(|r| &r.meta),
    );
    v
}

/// One row of the test-lock ledger projection: a `lock` anchor and every
/// numeric-code rule that declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockLedgerEntry {
    /// The declaring rules' `lock` field, verbatim — a `tests/` lock file, or
    /// a documented placeholder for codes whose behavior is locked by
    /// whole-file semantic tests with no per-code integration file.
    pub lock: &'static str,
    /// Rule codes declaring this anchor, in declared (table) order.
    pub codes: Vec<u32>,
}

/// The documented non-`tests/` lock placeholder. A rule cites it instead of
/// inventing a lock file when its behavior is locked only by a context-gated
/// netcheck report path with no per-code integration test. Every other lock
/// anchor in the catalog is a concrete `tests/` file that fires (or, for
/// structurally unreachable codes, actively documents and asserts the
/// absence of) the codes it owns - see tests/lock_pp_*.rs.
const DOC_LOCK_PLACEHOLDERS: &[&str] = &["netcheck context-gated (reorg doc §8.3)"];

/// Project the numeric-code scopes' test-lock ledger: group every `lock`
/// anchor across FlatErc / Declaration / AssemblyGate / PostParse into one
/// per-anchor code list (codes keep declared order within an anchor; anchors
/// sort lexically). This is the "ledger = catalog projection" of design §3:
/// per-code lock completeness is validated against this view, and the §8
/// consumer surface (stage 5) reads the same projection. VizLayout rows are
/// excluded — they carry string ids and their bookkeeping anchor is the viz
/// owner host fn (stage-4 adjudication), not a test lock file.
pub fn lock_ledger() -> Vec<LockLedgerEntry> {
    let mut by_lock: std::collections::BTreeMap<&'static str, Vec<u32>> =
        std::collections::BTreeMap::new();
    for r in FLAT_ERC_RULES {
        by_lock.entry(r.meta.lock).or_default().push(r.meta.code);
    }
    for r in DECL_RULES {
        by_lock.entry(r.meta.lock).or_default().push(r.meta.code);
    }
    for r in GATE_RULES {
        by_lock.entry(r.meta.lock).or_default().push(r.meta.code);
    }
    for r in POSTPARSE_RULES {
        by_lock.entry(r.meta.lock).or_default().push(r.meta.code);
    }
    by_lock
        .into_iter()
        .map(|(lock, codes)| LockLedgerEntry { lock, codes })
        .collect()
}

/// Filter axes for [`query_rules`] (design §8: list/detail, filter by the
/// §2.3 category axes and the §2.5 governance attributes). `None` means "any".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RuleFilter {
    pub scope: Option<RuleScope>,
    pub domain: Option<RuleDomain>,
    pub severity: Option<CheckSeverity>,
    pub plane: Option<RulePlane>,
    pub gate: Option<GateKind>,
    /// `Some(true)` = suppressible rows only (`overridable`), `Some(false)` =
    /// the rest.
    pub overridable: Option<bool>,
    pub fix: Option<FixKind>,
}

/// Enumerate every numeric-code rule descriptor matching the filter, in
/// declared table order: FlatErc, Declaration, AssemblyGate, then PostParse.
/// The four scope tables are the catalog's only numeric-code rule carriers —
/// viz rows carry string ids and stay behind [`viz_layout_rules`]. Every
/// consumer surface (`mcc rules` list, RPC `rules.list`, caps summary) reads
/// this same projection so the bytes stay identical across layers.
pub fn query_rules(filter: &RuleFilter) -> Vec<&'static RuleMeta> {
    fn hit(m: &'static RuleMeta, f: &RuleFilter) -> bool {
        f.scope.map_or(true, |v| m.scope == v)
            && f.domain.map_or(true, |v| m.domain == v)
            && f.severity.map_or(true, |v| m.severity == v)
            && f.plane.map_or(true, |v| m.plane == v)
            && f.gate.map_or(true, |v| m.gate == v)
            && f.overridable.map_or(true, |v| m.overridable == v)
            && f.fix.map_or(true, |v| m.fix == v)
    }
    let mut out = Vec::new();
    let tables = FLAT_ERC_RULES
        .iter()
        .map(|r| &r.meta)
        .chain(DECL_RULES.iter().map(|r| &r.meta))
        .chain(GATE_RULES.iter().map(|r| &r.meta))
        .chain(POSTPARSE_RULES.iter().map(|r| &r.meta));
    for m in tables {
        if hit(m, filter) {
            out.push(m);
        }
    }
    out
}

/// Look up an AssemblyGate rule by its report tag (meta name, e.g. "R02").
pub fn gate_rule_by_tag(tag: &str) -> Option<&'static GateRule> {
    GATE_RULES.iter().find(|r| r.meta.name == tag)
}

/// The report level of an AssemblyGate tag. Used by `instant::netcheck` to
/// seed the report rows and assign finding levels; `None` means the tag is
/// not a registered AssemblyGate rule.
pub fn gate_severity(tag: &str) -> Option<CheckSeverity> {
    gate_rule_by_tag(tag).map(|r| r.meta.severity)
}

/// The blocking set of the AssemblyGate scope: every rule whose `gate` axis
/// is `Blocking` (error-level rules today). `build --viz` derives its gate
/// set from the catalog through this query (§7.3).
pub fn assembly_gate_blocking_tags() -> Vec<&'static str> {
    GATE_RULES
        .iter()
        .filter(|r| r.meta.gate == GateKind::Blocking)
        .map(|r| r.meta.name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errcodes::{
        ABSTRACT_PART_UNSELECTED, NET_BACKFEED_RISK, NET_BIDIR_UNCONNECTED, NET_DANGLING_ENDPOINT,
        NET_INPUT_UNCONNECTED, NET_INSTANCE_UNCONNECTED, NET_MODULE_PORT_UNCONNECTED,
        NET_MULTI_DRIVE, NET_NC_CONNECTED, NET_NO_DRIVER, NET_OUTPUTS_NO_INPUT,
        NET_OUTPUT_UNDRIVEN, NET_PARTIAL_CONNECTION, NET_POWER_NET_COUNT, NET_VOLTAGE_MISMATCH,
        PIN_CONFLICTING_OPTIONS, PIN_UNCONNECTED, PULLUP_DEGENERATE,
    };

    /// The execution order of the migrated `nets::run_net_checks` call table.
    /// This is the lock that keeps catalog declaration order byte-identical to
    /// the pre-registry runner sequence.
    const FLAT_ERC_ORDER: [u32; 16] = [
        NET_MULTI_DRIVE,             // P1
        NET_NO_DRIVER,               // P2
        NET_INPUT_UNCONNECTED,       // P5
        NET_NC_CONNECTED,            // P6
        NET_OUTPUT_UNDRIVEN,         // P7
        NET_BACKFEED_RISK,           // P8
        NET_INSTANCE_UNCONNECTED,    // P9
        NET_VOLTAGE_MISMATCH,        // P3+P4
        NET_OUTPUTS_NO_INPUT,        // V1
        NET_POWER_NET_COUNT,         // power net count summary
        NET_MODULE_PORT_UNCONNECTED, // C4
        NET_DANGLING_ENDPOINT,       // self-loop
        NET_PARTIAL_CONNECTION,      // pin count vs definition
        ABSTRACT_PART_UNSELECTED,    // abstract-variant
        NET_BIDIR_UNCONNECTED,       // floating outputs
        PULLUP_DEGENERATE,           // D7
    ];

    /// The report-row tags of the netcheck R-series. This is the lock that
    /// keeps `GATE_RULES` identical to the tags the runner used to seed the
    /// report table before the migration (byte-identical rows).
    const GATE_ORDER: [&str; 15] = [
        "R01", "R02", "R03", "R03a", "R04", "R05", "R06", "R07", "R08", "R09", "R10", "R11", "R12",
        "R14", "R15",
    ];

    /// The execution order of the migrated `pins::run_pin_checks` sequence —
    /// the lock that keeps `DECL_RULES` byte-identical to the former call
    /// sequence (`check_unused_pins` first, then `check_conflicting_pins`).
    const DECL_ORDER: [u32; 2] = [PIN_UNCONNECTED, PIN_CONFLICTING_OPTIONS];

    /// The registration order of the semantic PostParse hosts
    /// (`CheckRegistry::with_defaults()`, §5-5) with each host's codes in
    /// first-emission source order. This is the lock that keeps
    /// `POSTPARSE_RULES` byte-identical to the `validation/*` emission set;
    /// the object hosts stay the executor, so this anchors the catalog copy.
    const POSTPARSE_ORDER: [u32; 93] = [
        // duplicate
        crate::errcodes::DUP_CMIE_CROSS_FILE,
        // dupwithin
        crate::errcodes::DUP_WITHIN,
        crate::errcodes::DUP_ENUM_VALUE,
        // enums
        crate::errcodes::ENUM_DUPLICATE_VALUE,
        crate::errcodes::ENUM_MEMBER_DOT,
        crate::errcodes::ENUM_MEMBER_LEADING_DIGIT,
        crate::errcodes::ENUM_MEMBER_RESERVED,
        crate::errcodes::ATTR_SELF_REFERENTIAL,
        crate::errcodes::RANGE_REVERSED,
        // attrs
        crate::errcodes::ATTR_RESERVED_KEYWORD,
        crate::errcodes::ROLE_EMPTY_BODY,
        crate::errcodes::ATTR_NESTING_TOO_DEEP,
        crate::errcodes::ATTR_PIN_GROUP_UNDEFINED,
        crate::errcodes::PINS_PLUS_AND_PINS_CONFLICT,
        // conds
        crate::errcodes::COND_EMPTY_BODY,
        crate::errcodes::COND_IF_WITHOUT_ELSE,
        crate::errcodes::COND_DUPLICATE,
        crate::errcodes::PIN_NC_COMPONENT_LEVEL,
        crate::errcodes::PIN_IO_MIX_IN_OUT,
        crate::errcodes::PIN_IO_MIX_OUTPUT_POWER,
        crate::errcodes::PIN_IO_MIX_ANALOG_POWER,
        crate::errcodes::PARAM_PIN_NAME_SHADOW,
        crate::errcodes::MODULE_STUB,
        // defs
        crate::errcodes::DEF_AMBIGUOUS_NAME,
        crate::errcodes::DEF_REF_NOT_LOADED,
        crate::errcodes::COMPONENT_INT_SUFFIX,
        crate::errcodes::ENUM_INT_SUFFIX,
        // imports
        crate::errcodes::USE_SELF_IMPORT,
        crate::errcodes::USE_ALIAS_COLLISION,
        crate::errcodes::USE_VERSIONED_TARGET_NOT_FOUND,
        crate::errcodes::USE_IMPORT_SYMBOL_NOT_FOUND,
        crate::errcodes::USE_REEXPORT_SYMBOL_NOT_FOUND,
        // interface
        crate::errcodes::IFACE_PINS_NOT_ALL_BOUND,
        crate::errcodes::IFACE_ROLE_NOT_FOUND,
        crate::errcodes::IFACE_NOT_LOADED,
        crate::errcodes::IFACE_DEPRECATED_CMIE,
        // naming
        crate::errcodes::NAME_COMPONENT_LOWERCASE,
        crate::errcodes::NAME_PIN_MIXED_CONVENTION,
        crate::errcodes::NAME_INSTANCE_SINGLE_CHAR,
        crate::errcodes::NAME_PORT_INST_SHADOWS_CMIE,
        crate::errcodes::NAME_PARAM_SHADOWS_CMIE,
        // ports
        crate::errcodes::PORT_DUPLICATE_NAME,
        crate::errcodes::NAME_PARAM_AND_INSTANCE,
        crate::errcodes::INST_DECLARED_MULTIPLE,
        // refs
        crate::errcodes::FUNC_PARAMS_NO_BODY,
        crate::errcodes::REF_INTEGRITY,
        crate::errcodes::SPEC_KEY_UNDECLARED_PARAM,
        // exprs
        crate::errcodes::EXPR_THIS_TOP_LEVEL,
        crate::errcodes::EXPR_PLACEHOLDER_ONLY,
        crate::errcodes::ATTR_LARGE_INT,
        crate::errcodes::ATTR_INFINITE_FLOAT,
        crate::errcodes::RANGE_SINGLE_ELEMENT,
        crate::errcodes::IDX_MULTIPLE_SLICE_SPEC,
        // extra
        crate::errcodes::NAME_PORT_SHADOWS_CMIE,
        crate::errcodes::ENUM_SINGLE_VALUE,
        crate::errcodes::FUNC_EMPTY_BODY,
        crate::errcodes::IFACE_PIN_COUNT_MISMATCH,
        crate::errcodes::COMPONENT_EMPTY,
        crate::errcodes::COMPONENT_NO_PINS,
        crate::errcodes::INTERFACE_EMPTY,
        crate::errcodes::PARAM_INT_DEFAULT_STRING,
        crate::errcodes::PARAM_STRING_DEFAULT_NUMERIC,
        crate::errcodes::PARAM_UV_DEFAULT_NO_UNIT,
        crate::errcodes::DEFINE_NO_ATTRS,
        crate::errcodes::DEFINE_NON_ATTR_CLAUSE,
        crate::errcodes::INST_CLASS_NOT_LOADED,
        crate::errcodes::BUS_DUPLICATE_MEMBER,
        crate::errcodes::COMPONENT_MIXED_CASE,
        crate::errcodes::PARAM_RESERVED_KEYWORD,
        crate::errcodes::FUNC_SHARES_NAME_WITH_PORT,
        crate::errcodes::PARAM_NEGATIVE_DEFAULT,
        crate::errcodes::PARAM_FLOAT_DEFAULT_INVALID,
        crate::errcodes::SPEC_KEY_DUPLICATE,
        // floating
        crate::errcodes::FUNC_FLOATING_LABEL,
        // gate
        crate::errcodes::SINGLE_USE_INLINE_NET,
        // insts
        crate::errcodes::INST_ARG_COUNT_MISMATCH,
        crate::errcodes::ROLE_NAME_SHADOWS,
        // body
        crate::errcodes::USE_MIXED_PATH_SEPARATORS,
        crate::errcodes::INST_THIS_TYPE,
        crate::errcodes::COND_SINGLE_BINARY,
        crate::errcodes::MODULE_PORT_UNUSED,
        // hw
        crate::errcodes::POWER_PIN_NO_VOLTAGE,
        crate::errcodes::HW_PIN_NUMBER_GAP,
        crate::errcodes::HW_PIN_COUNT_HIGH,
        crate::errcodes::HW_ZERO_PINS_WITH_PARAMS,
        crate::errcodes::HW_IFACE_ROLE_UNBOUND,
        crate::errcodes::HW_ALL_SAME_IO_TYPE,
        crate::errcodes::HW_FUNC_PARAM_SHADOWS_PIN,
        // types
        crate::errcodes::TYPE_INCOMPATIBLE,
        // adopt
        crate::errcodes::VARIANT_BASE_NON_ABSTRACT,
        crate::errcodes::ADOPTS_NON_CAPABILITY,
        crate::errcodes::ADOPTED_FUNC_AMBIGUOUS,
        crate::errcodes::CAPABILITY_SIGNAL_MISSING,
    ];

    #[test]
    fn flat_erc_first_rule_is_e4101_pilot() {
        let r = &FLAT_ERC_RULES[0].meta;
        assert_eq!(r.code, NET_MULTI_DRIVE);
        assert_eq!(r.name, "driver-conflict");
        assert_eq!(r.severity, CheckSeverity::Error);
        assert_eq!(r.scope, RuleScope::FlatErc);
        assert_eq!(r.domain, RuleDomain::Connectivity);
        assert_eq!(r.family, Some("A"));
        assert!(!r.overridable);
    }

    #[test]
    fn declaration_order_is_execution_order() {
        // §5-5: within a scope the runner order is the table order. The 16
        // migrated FlatErc rules reproduce the former run_net_checks sequence.
        let codes: Vec<u32> = FLAT_ERC_RULES.iter().map(|r| r.meta.code).collect();
        assert_eq!(codes, FLAT_ERC_ORDER);
    }

    #[test]
    fn owner_is_a_typed_fn_pointer_to_the_host_check() {
        // Same-scope checks share one context signature, so `run` type-checks
        // against the real host fn item and drives the runner order.
        // `fn_addr_eq` compares actual addresses; plain `==` is linted as
        // unpredictable for fn pointers.
        assert!(std::ptr::fn_addr_eq(
            FLAT_ERC_RULES[0].run,
            check_driver_conflict as fn(&InstTable, &mut Vec<NetCheckResult>)
        ));
    }

    #[test]
    fn catalog_queries_work() {
        assert_eq!(
            rule_count(),
            FLAT_ERC_ORDER.len() + DECL_ORDER.len() + GATE_ORDER.len() + POSTPARSE_ORDER.len()
        );
        assert_eq!(flat_erc_rules().len(), FLAT_ERC_ORDER.len());
        assert_eq!(declaration_rules().len(), DECL_ORDER.len());
        assert_eq!(assembly_gate_rules().len(), GATE_ORDER.len());
        assert_eq!(post_parse_rules().len(), POSTPARSE_ORDER.len());
        assert!(find_rule(NET_MULTI_DRIVE).is_some());
        assert!(find_rule(NET_VOLTAGE_MISMATCH).is_some());
        assert!(find_rule(PIN_UNCONNECTED).is_some());
        assert!(find_rule(0xFFFF).is_none());
        assert_eq!(
            rules_in_scope(RuleScope::FlatErc).len(),
            FLAT_ERC_ORDER.len()
        );
        assert_eq!(
            rules_in_scope(RuleScope::Declaration).len(),
            DECL_ORDER.len()
        );
        assert_eq!(
            rules_in_scope(RuleScope::AssemblyGate).len(),
            GATE_ORDER.len()
        );
        assert_eq!(
            rules_in_scope(RuleScope::PostParse).len(),
            POSTPARSE_ORDER.len()
        );
        // The pilot E4101 is found through both the scope table and the code
        // query; the query answer is the shared data-only metadata.
        let r = find_rule(NET_MULTI_DRIVE).expect("E4101 pilot is registered");
        assert_eq!(r.scope, RuleScope::FlatErc);
        assert_eq!(r.sink, RuleSink::Envelope);
        assert_eq!(r.plane, RulePlane::CoreMechanism);
        assert_eq!(r.acceptance, Acceptance::Legal);
        assert_eq!(r.cadence, Cadence::PerCircuit);
        // A representative PostParse code resolves through the same query.
        let p = find_rule(crate::errcodes::TYPE_INCOMPATIBLE).expect("PostParse code registered");
        assert_eq!(p.scope, RuleScope::PostParse);
        assert_eq!(p.sink, RuleSink::Envelope);
    }

    #[test]
    fn declaration_rules_reproduce_the_pin_check_sequence() {
        // Stage 3: the two migrated pin-usage checks keep the former call
        // sequence (`check_unused_pins` → `check_conflicting_pins`) and the
        // Declaration governance defaults.
        let codes: Vec<u32> = DECL_RULES.iter().map(|r| r.meta.code).collect();
        assert_eq!(codes, DECL_ORDER);
        let names: Vec<&str> = DECL_RULES.iter().map(|r| r.meta.name).collect();
        assert_eq!(names, ["unused-pin", "conflicting-pin-options"]);
        for r in DECL_RULES {
            assert_eq!(r.meta.scope, RuleScope::Declaration);
            assert_eq!(r.meta.domain, RuleDomain::PinDecl);
            assert_eq!(r.meta.plane, RulePlane::CoreMechanism);
            assert_eq!(r.meta.acceptance, Acceptance::Legal);
            assert_eq!(r.meta.sink, RuleSink::Envelope);
            assert_eq!(r.meta.cadence, Cadence::PerCircuit);
            assert_eq!(r.meta.gate, gate_for(r.meta.severity));
            assert!(!r.meta.overridable);
        }
        assert_eq!(DECL_RULES[0].meta.severity, CheckSeverity::Warning);
        assert_eq!(DECL_RULES[1].meta.severity, CheckSeverity::Warning);
        assert!(std::ptr::fn_addr_eq(
            DECL_RULES[0].run,
            check_unused_pins as fn(&InstTable, &mut Vec<PinCheckResult>)
        ));
        assert!(std::ptr::fn_addr_eq(
            DECL_RULES[1].run,
            check_conflicting_pins as fn(&InstTable, &mut Vec<PinCheckResult>)
        ));
    }

    #[test]
    fn flat_erc_governance_defaults_match_severity() {
        // FlatErc rules are language-base envelope diagnostics; the gate axis
        // is derived from severity so an error still blocks the build.
        for r in FLAT_ERC_RULES {
            assert_eq!(r.meta.plane, RulePlane::CoreMechanism);
            assert_eq!(r.meta.acceptance, Acceptance::Legal);
            assert_eq!(r.meta.sink, RuleSink::Envelope);
            assert_eq!(r.meta.cadence, Cadence::PerCircuit);
            assert_eq!(r.meta.gate, gate_for(r.meta.severity));
        }
    }

    #[test]
    fn gate_rules_reproduce_the_report_row_set() {
        // The runner seeds its report from the catalog; the tag set and order
        // must stay byte-identical to the pre-migration seed array.
        let tags: Vec<&str> = GATE_RULES.iter().map(|g| g.meta.name).collect();
        assert_eq!(tags, GATE_ORDER);
        for g in GATE_RULES {
            assert_eq!(g.meta.scope, RuleScope::AssemblyGate);
            assert_eq!(g.meta.sink, RuleSink::GateReport);
            assert_eq!(g.meta.plane, RulePlane::CoreMechanism);
            assert_eq!(g.meta.acceptance, Acceptance::Legal);
            assert!(!g.meta.overridable);
            // A gate row is Blocking exactly when its severity is Error: that
            // invariant makes `is_clean` (no error findings) and the catalog
            // blocking set agree byte-for-byte.
            assert_eq!(g.meta.gate, gate_for(g.meta.severity));
        }
    }

    #[test]
    fn gate_report_levels_match_the_preregistry_levels() {
        // Levels reproduced from the former netcheck `rule_level` table.
        let expect: Vec<(CheckSeverity, u32)> = vec![
            (CheckSeverity::Error, crate::errcodes::GATE_LITERAL_POINT), // R01
            (CheckSeverity::Error, crate::errcodes::GATE_SHORT_PASSIVE), // R02
            (CheckSeverity::Error, crate::errcodes::GATE_SHORT_RAIL),    // R03
            (CheckSeverity::Info, crate::errcodes::GATE_RAIL_ALIAS),     // R03a
            (CheckSeverity::Error, crate::errcodes::GATE_SHORT_LANE),    // R04
            (CheckSeverity::Error, crate::errcodes::GATE_UNRESOLVED_UNIT), // R05
            (CheckSeverity::Warning, crate::errcodes::GATE_MEGANET),     // R06
            (CheckSeverity::Error, crate::errcodes::GATE_GHOST_INSTANCE), // R07
            (CheckSeverity::Error, crate::errcodes::GATE_PHANTOM_PATH),  // R08
            (
                CheckSeverity::Warning,
                crate::errcodes::GATE_FLOATING_POWER_PIN,
            ), // R09
            (
                CheckSeverity::Error,
                crate::errcodes::GATE_SYMBOL_CONSERVATION,
            ), // R10
            (CheckSeverity::Error, crate::errcodes::GATE_SPLIT_RAIL),    // R11
            (CheckSeverity::Info, crate::errcodes::GATE_DANGLING_PORT),  // R12
            (
                CheckSeverity::Warning,
                crate::errcodes::GATE_ORPHAN_INSTANCE,
            ), // R14
            (CheckSeverity::Warning, crate::errcodes::GATE_SYNTHETIC_PIN), // R15
        ];
        for (sev, code) in expect {
            let g = find_rule(code).expect("gate code registered");
            assert_eq!(g.severity, sev, "severity for code {code}");
        }
    }

    #[test]
    fn assembly_gate_blocking_set_is_the_error_rows() {
        // Rows that used to fail the `build --viz` gate (level Error) are the
        // Blocking rows; the blocking set is a catalog projection now.
        let blocking = assembly_gate_blocking_tags();
        assert_eq!(blocking.len(), 9);
        for tag in [
            "R01", "R02", "R03", "R04", "R05", "R07", "R08", "R10", "R11",
        ] {
            assert!(blocking.contains(&tag), "missing blocking tag {tag}");
        }
        for tag in ["R03a", "R06", "R09", "R12", "R14", "R15"] {
            assert!(!blocking.contains(&tag), "{tag} must stay advisory");
        }
    }

    #[test]
    fn gate_tag_lookup_and_severity_query_work() {
        assert_eq!(
            gate_rule_by_tag("R02").map(|g| g.label),
            Some("R02 SHORT_PASSIVE")
        );
        assert_eq!(
            gate_rule_by_tag("R12").map(|g| g.label),
            Some("R12 DANGLING_PORT")
        );
        assert_eq!(gate_severity("R02"), Some(CheckSeverity::Error));
        assert_eq!(gate_severity("R14"), Some(CheckSeverity::Warning));
        assert_eq!(gate_severity("R99"), None);
        assert!(gate_rule_by_tag("R99").is_none());
    }

    #[test]
    fn codes_and_names_unique_across_every_scope() {
        let all_meta: Vec<&RuleMeta> = FLAT_ERC_RULES
            .iter()
            .map(|r| &r.meta)
            .chain(DECL_RULES.iter().map(|r| &r.meta))
            .chain(GATE_RULES.iter().map(|g| &g.meta))
            .chain(POSTPARSE_RULES.iter().map(|p| &p.meta))
            .collect();
        let mut codes: Vec<u32> = all_meta.iter().map(|r| r.code).collect();
        codes.sort_unstable();
        assert!(codes.windows(2).all(|w| w[0] != w[1]));
        let mut names: Vec<&str> = all_meta.iter().map(|r| r.name).collect();
        names.sort_unstable();
        assert!(names.windows(2).all(|w| w[0] != w[1]));
    }

    #[test]
    fn post_parse_rules_reproduce_the_registration_order() {
        // §5-5: the catalog copy of the semantic PostParse codes stays ordered
        // by `CheckRegistry::with_defaults()` host registration; the object
        // hosts remain the executor (`run_post_parse` is untouched), so this
        // anchors the descriptor table rather than the emission sequence.
        let codes: Vec<u32> = POSTPARSE_RULES.iter().map(|r| r.meta.code).collect();
        assert_eq!(codes, POSTPARSE_ORDER);
        let first_host = POSTPARSE_RULES[0].host;
        assert_eq!(first_host, "duplicate");
        let last_host = POSTPARSE_RULES[POSTPARSE_ORDER.len() - 1].host;
        assert_eq!(last_host, "adopt");
    }

    #[test]
    fn post_parse_governance_defaults_match_the_semantic_layer() {
        // Stage-3 remainder adjudication: every semantic host is a language-
        // base legality sweep (CoreMechanism / Legal / Envelope) gated by
        // severity and running per circuit; no exception rows arise (§6).
        for p in POSTPARSE_RULES {
            assert_eq!(p.meta.scope, RuleScope::PostParse);
            assert_eq!(p.meta.plane, RulePlane::CoreMechanism);
            assert_eq!(p.meta.acceptance, Acceptance::Legal);
            assert_eq!(p.meta.sink, RuleSink::Envelope);
            assert_eq!(p.meta.cadence, Cadence::PerCircuit);
            assert_eq!(p.meta.gate, gate_for(p.meta.severity));
            assert!(!p.meta.overridable);
            assert!(!p.host.is_empty(), "every PostParse row names its host");
        }
        // Severity distribution sanity for the per-code adjudication rows.
        let errors = POSTPARSE_RULES
            .iter()
            .filter(|p| p.meta.severity == CheckSeverity::Error)
            .count();
        assert!(
            errors >= 10,
            "language-base errors stay blocking: got {errors}"
        );
    }

    #[test]
    fn every_rule_names_its_owner_severity_and_family_consistently() {
        for r in FLAT_ERC_RULES {
            assert_eq!(r.meta.scope, RuleScope::FlatErc);
            assert!(!r.meta.overridable);
            // E4101 is the only FlatErc rule adjudicated to a content family
            // so far (the AssemblyGate scope adjudicates R01-R04 to "A").
            assert!(r.meta.family.is_none() || r.meta.code == NET_MULTI_DRIVE);
        }
    }

    /// The registered VizLayout row ids in table order (v0.7 stage 4). The A
    /// rows reproduce `audit_equi_tree` collection order — A1..A18 then
    /// A21..A34 with A2b, and the A19/A20/A33 numbering gaps are intentional —
    /// and the F rows are the three fidelity gate tiers.
    const VIZ_LAYOUT_ORDER: [&str; 35] = [
        "A1", "A2", "A2b", "A3", "A4", "A5", "A6", "A7", "A8", "A9", "A10", "A11", "A12", "A13",
        "A14", "A15", "A16", "A17", "A18", "A21", "A22", "A23", "A24", "A25", "A26", "A27", "A28",
        "A29", "A30", "A34", "A31", "A32", "F1", "F2", "F3",
    ];

    #[test]
    fn viz_layout_rows_are_aggregated_read_only() {
        // Stage 4: viz self-describes its checks; the top-level aggregation
        // returns the viz-owned slice unchanged (no central row copy).
        assert_eq!(viz_layout_rules().len(), VIZ_LAYOUT_ORDER.len());
        let ids: Vec<&str> = viz_layout_rules().iter().map(|r| r.id).collect();
        assert_eq!(ids, VIZ_LAYOUT_ORDER);
        let a = viz_layout_rules()
            .iter()
            .filter(|r| r.id.starts_with('A'))
            .count();
        let f = viz_layout_rules()
            .iter()
            .filter(|r| r.id.starts_with('F'))
            .count();
        assert_eq!((a, f), (32, 3));
        // VizLayout rows are string-id rules: they are absent from the numeric
        // scope query and from rule_count(), which sums the code tables only.
        assert_eq!(rules_in_scope(RuleScope::VizLayout).len(), 0);
        let before = rule_count();
        assert!(before > 0);
    }

    #[test]
    fn viz_layout_governance_defaults_are_locked() {
        let all = viz_layout_rules();
        for r in all {
            assert!(
                matches!(r.severity, "error" | "warning" | "info"),
                "unexpected severity '{}' for {}",
                r.severity,
                r.id
            );
            assert!(!r.host.is_empty(), "every row names its viz owner");
            assert!(!r.name.is_empty());
        }
        // A-series rows are error invariants; the column-model pair (A5/A6) is
        // declared but not computable yet, and the fidelity tiers carry the
        // gate levels (blocking / ratchet / informational).
        for id in ["A5", "A6"] {
            let r = all.iter().find(|x| x.id == id).unwrap();
            assert!(!r.computable, "{id} waits on the column model");
        }
        let computable = all.iter().filter(|x| x.computable).count();
        assert_eq!(computable, 33);
        let f1 = all.iter().find(|x| x.id == "F1").unwrap();
        assert_eq!(f1.severity, "error");
        let f2 = all.iter().find(|x| x.id == "F2").unwrap();
        assert_eq!(f2.severity, "warning");
        let f3 = all.iter().find(|x| x.id == "F3").unwrap();
        assert_eq!(f3.severity, "info");
        let ids: Vec<&str> = all.iter().map(|r| r.id).collect();
        assert_eq!(ids.windows(2).position(|w| w == ["A2", "A2b"]), Some(1));
        assert_eq!(ids.windows(2).position(|w| w == ["A2b", "A3"]), Some(2));
        for gap in ["A19", "A20", "A33"] {
            assert!(!ids.contains(&gap), "{gap} is an intentional gap");
        }
    }

    #[test]
    fn lock_ledger_projects_every_numeric_code_exactly_once() {
        // Ledger = catalog projection (design §3): each numeric-code rule
        // appears in exactly one anchor row, and the ledger covers
        // rule_count() codes with no duplication or loss.
        let entries = lock_ledger();
        assert!(entries.iter().all(|e| !e.codes.is_empty()));
        assert_eq!(
            entries.len(),
            entries
                .iter()
                .map(|e| &e.lock)
                .collect::<std::collections::HashSet<_>>()
                .len()
        );
        let total: usize = entries.iter().map(|e| e.codes.len()).sum();
        assert_eq!(total, rule_count());
        let mut codes: Vec<u32> = entries
            .iter()
            .flat_map(|e| e.codes.iter().copied())
            .collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), rule_count());
        // Anchors come from the rules verbatim and the rows are lexically
        // sorted, so the projection itself is deterministic.
        assert!(entries.windows(2).all(|w| w[0].lock < w[1].lock));
    }

    #[test]
    fn lock_ledger_anchors_are_strong_or_documented_and_pinned() {
        // Every numeric-code lock is either a tests/ file (strong anchor) or
        // the one remaining documented placeholder (context-gated netcheck
        // rows with no per-code integration test); nothing else is allowed.
        // The per-kind code counts are the ledger lock: adding a rule, or
        // re-anchoring one, changes this line on purpose.
        let mut strong = 0usize;
        let doc = 0usize;
        let mut note = 0usize;
        for e in lock_ledger() {
            if e.lock.starts_with("tests/") {
                strong += e.codes.len();
            } else if e.lock == DOC_LOCK_PLACEHOLDERS[0] {
                note += e.codes.len();
            } else {
                panic!(
                    "anchor '{}' is neither a tests/ file nor a documented placeholder",
                    e.lock
                );
            }
        }
        // The 63 PostParse codes that once shared the validation-module doc
        // placeholder now carry concrete tests/lock_pp_*.rs anchors, so the
        // doc partition is empty and every one of them counts as strong.
        assert_eq!((strong, doc, note), (123, 0, 3));
        assert_eq!(strong + doc + note, rule_count());
    }

    #[test]
    fn query_rules_filters_axes_and_preserves_table_order() {
        // No filter == every numeric-code rule, in declared table order.
        let all = query_rules(&RuleFilter::default());
        assert_eq!(all.len(), rule_count());
        let order = [
            flat_erc_rules().len(),
            declaration_rules().len(),
            assembly_gate_rules().len(),
            post_parse_rules().len(),
        ];
        // The combined list is the four tables concatenated: boundaries fall
        // exactly at the per-table lengths.
        let boundary_checks = [
            0usize,
            order[0],
            order[0] + order[1],
            order[0] + order[1] + order[2],
        ];
        for (i, &b) in boundary_checks.iter().enumerate() {
            let expected_scope = match i {
                0 => RuleScope::FlatErc,
                1 => RuleScope::Declaration,
                2 => RuleScope::AssemblyGate,
                _ => RuleScope::PostParse,
            };
            assert_eq!(all[b].scope, expected_scope, "table boundary at {b}");
        }

        // Scope filter counts match the table lengths.
        for (scope, len) in [
            (RuleScope::FlatErc, order[0]),
            (RuleScope::Declaration, order[1]),
            (RuleScope::AssemblyGate, order[2]),
            (RuleScope::PostParse, order[3]),
        ] {
            let f = RuleFilter {
                scope: Some(scope),
                ..Default::default()
            };
            assert_eq!(query_rules(&f).len(), len, "{scope:?}");
        }

        // Nothing is suppressible today (overridable all false), and severity
        // filtering narrows the result to the matching default rows.
        let ov = query_rules(&RuleFilter {
            overridable: Some(true),
            ..Default::default()
        });
        assert!(ov.is_empty());
        let errs = query_rules(&RuleFilter {
            severity: Some(CheckSeverity::Error),
            ..Default::default()
        });
        assert!(!errs.is_empty());
        assert!(errs.iter().all(|m| m.severity == CheckSeverity::Error));

        // Every row carries the default `fix = None` descriptor value.
        let fixes = query_rules(&RuleFilter {
            fix: Some(FixKind::None),
            ..Default::default()
        });
        assert_eq!(fixes.len(), rule_count());
    }
}

// ============================================================================
// PostParse scope (semantic layer — validation/* CheckRegistry hosts)
// ============================================================================

/// PostParse-scoped rule row. The semantic layer executes through the object
/// hosts in `CheckRegistry::with_defaults()` registration order (§5-5) against
/// `&mut CheckAccumulator`; the host contexts are not one uniform fn
/// signature, so this row is **data-only** — the §2.2 descriptor plus the
/// owning host module name. `CheckRegistry::run_post_parse` keeps driving from
/// `with_defaults()`; the table registers and describes every code the hosts
/// emit (per-code granularity) without changing any emission.
#[derive(Debug, Clone)]
pub struct PostParseRule {
    /// Registry metadata (§2.2 descriptor).
    pub meta: RuleMeta,
    /// Host module that owns the code's primary emission (a `validation`
    /// submodule); a code emitted by several hosts lists the earliest
    /// registered host and its row comment notes the other sites.
    pub host: &'static str,
}

/// Declare one PostParse rule as a table element. `scope` is fixed to
/// `PostParse`. Governance defaults are the semantic-layer values
/// (CoreMechanism / Legal / Envelope / gate derived from severity /
/// PerCircuit); the semantic sweep is language-base legality checking, so no
/// DomainPackage / SimFulfillment / Contract / Fulfillment exception arises
/// among these hosts (design §6 stage-3 adjudication). `lock` names the
/// concrete test anchor that fires the code (or documents its structural
/// unreachability); every row now points at a `tests/` file, and the
/// per-code lock-ledger projection (stage 4) verifies the assignment.
macro_rules! declare_post_parse_rule {
    (
        code = $code:expr,
        name = $name:literal,
        title = $title:literal,
        severity = $sev:ident,
        domain = $dom:ident,
        host = $host:literal,
        doc = $doc:literal,
        lock = $lock:literal,
    ) => {
        PostParseRule {
            meta: RuleMeta {
                code: $code,
                name: $name,
                title: $title,
                severity: CheckSeverity::$sev,
                scope: RuleScope::PostParse,
                domain: RuleDomain::$dom,
                family: None,
                doc: $doc,
                lock: $lock,
                overridable: false,
                fix: FixKind::None,
                plane: RulePlane::CoreMechanism,
                acceptance: Acceptance::Legal,
                sink: RuleSink::Envelope,
                gate: gate_for(CheckSeverity::$sev),
                cadence: Cadence::PerCircuit,
            },
            host: $host,
        }
    };
}

/// PostParse rules in `with_defaults()` host registration order (§5-5), one
/// row per code the semantic hosts emit. The order reproduces the registration
/// sequence in `validation/mod.rs` (`DuplicateCmie -> DupWithin -> Enums ->
/// Attrs -> ... -> Adoption`) with the codes of each host in first-emission
/// source order; the `POSTPARSE_ORDER` unit lock keeps the table identical to
/// that sequence. The row severity is the code's canonical default — see the
/// per-row notes for codes whose emission sites diverge; runtime emission is
/// untouched and byte-identical.
pub static POSTPARSE_RULES: &[PostParseRule] = &[
    // duplicate — DuplicateCmieCheck: same CMIE name defined in another file.
    // Severity comes from DuplicateCmieCheck::default_severity() (Warning);
    // workspace files only, the system lib shares names by design.
    declare_post_parse_rule! {
        code = crate::errcodes::DUP_CMIE_CROSS_FILE,
        name = "dup-cmie-cross-file",
        title = "same CMIE name defined in another file",
        severity = Warning,
        domain = Duplicate,
        host = "duplicate",
        doc = "Same name defined in another file (cross-file duplicate).",
        lock = "tests/lock_pp_duplicates.rs",
    },
    // dupwithin — DupWithinCheck: duplicate definitions inside one file.
    declare_post_parse_rule! {
        code = crate::errcodes::DUP_WITHIN,
        name = "dup-within",
        title = "duplicate definition within one declaration",
        severity = Warning,
        domain = Duplicate,
        host = "dupwithin",
        doc = "Duplicate definition within the same declaration.",
        lock = "tests/lock_pp_duplicates.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::DUP_ENUM_VALUE,
        name = "dup-enum-value",
        title = "duplicate enum value within one enum",
        severity = Error,
        domain = Duplicate,
        host = "dupwithin",
        doc = "Enum value appears more than once in the enum.",
        lock = "tests/lock_pp_duplicates.rs",
    },
    // enums — EnumsCheck: enum body shape and member hygiene.
    // Sibling of dupwithin's DUP_ENUM_VALUE fired from a different sweep.
    declare_post_parse_rule! {
        code = crate::errcodes::ENUM_DUPLICATE_VALUE,
        name = "enum-duplicate-value",
        title = "enum value declared twice",
        severity = Error,
        domain = Duplicate,
        host = "enums",
        doc = "Enum has a duplicate value.",
        lock = "tests/lock_pp_duplicates.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::ENUM_MEMBER_DOT,
        name = "enum-member-dot",
        title = "enum member contains a dot",
        severity = Error,
        domain = Structure,
        host = "enums",
        doc = "Enum member contains a dot.",
        lock = "tests/lock_pp_duplicates.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::ENUM_MEMBER_LEADING_DIGIT,
        name = "enum-member-leading-digit",
        title = "enum member starts with a digit",
        severity = Error,
        domain = Structure,
        host = "enums",
        doc = "Enum member starts with a digit.",
        lock = "tests/lock_pp_duplicates.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::ENUM_MEMBER_RESERVED,
        name = "enum-member-reserved",
        title = "enum member is a reserved keyword",
        severity = Warning,
        domain = Structure,
        host = "enums",
        doc = "Enum member is a reserved keyword.",
        lock = "tests/lock_pp_duplicates.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::ATTR_SELF_REFERENTIAL,
        name = "attr-self-referential",
        title = "attribute value equals its own key",
        severity = Warning,
        domain = Structure,
        host = "enums",
        doc = "Attribute value equals its own key; likely a copy-paste mistake.",
        lock = "tests/lock_pp_duplicates.rs",
    },
    // Range/vector syntax check; also fired by the exprs host at the same
    // Warning level (range-literal reversal), so this single row covers both.
    declare_post_parse_rule! {
        code = crate::errcodes::RANGE_REVERSED,
        name = "range-reversed",
        title = "range appears reversed",
        severity = Warning,
        domain = Structure,
        host = "enums",
        doc = "Range appears reversed; did you mean the opposite order?",
        lock = "tests/lock_pp_duplicates.rs",
    },
    // attrs — AttrsCheck: attribute naming, nesting, pin-group refs, pins.X.
    declare_post_parse_rule! {
        code = crate::errcodes::ATTR_RESERVED_KEYWORD,
        name = "attr-reserved-keyword",
        title = "attribute name is a reserved keyword",
        severity = Warning,
        domain = Structure,
        host = "attrs",
        doc = "Attribute uses a reserved keyword.",
        lock = "tests/semantic_false_diagnostics.rs",
    },
    // Severity adjudication: insts fires Warning for an interface role with no
    // pins/attrs/clauses (the code's doc meaning); attrs reuses the code at
    // Error for an unrecognized first attribute segment. Catalog default is
    // Warning (the primary role-empty-body meaning); both emissions are kept.
    declare_post_parse_rule! {
        code = crate::errcodes::ROLE_EMPTY_BODY,
        name = "role-empty-body",
        title = "role has an empty body",
        severity = Warning,
        domain = Structure,
        host = "attrs",
        doc = "Role has an empty body.",
        lock = "tests/lock_pp_attrs_insts.rs",
    },
    // Real nesting check in attrs and insts (function attrs), plus an insts
    // func-param IO-direction warning that reuses this code; all sites Warning.
    declare_post_parse_rule! {
        code = crate::errcodes::ATTR_NESTING_TOO_DEEP,
        name = "attr-nesting-too-deep",
        title = "attribute nesting exceeds 16 levels",
        severity = Warning,
        domain = Structure,
        host = "attrs",
        doc = "Attribute nesting depth exceeds 16.",
        lock = "tests/lock_pp_attrs_insts.rs",
    },
    // Fired by attrs (`pins.X` group check) and insts (role binding); both Error.
    declare_post_parse_rule! {
        code = crate::errcodes::ATTR_PIN_GROUP_UNDEFINED,
        name = "attr-pin-group-undefined",
        title = "attribute references an undefined pin group",
        severity = Error,
        domain = RefIntegrity,
        host = "attrs",
        doc = "Attribute references an undefined pin group, or role used outside a component.",
        lock = "tests/lock_pp_attrs_insts.rs",
    },
    // Fired by attrs (N8) and insts; both Warning.
    declare_post_parse_rule! {
        code = crate::errcodes::PINS_PLUS_AND_PINS_CONFLICT,
        name = "pins-plus-and-pins-conflict",
        title = "pins = and pins.X = attributes overlap",
        severity = Warning,
        domain = PinDecl,
        host = "attrs",
        doc = "Component mixes pins = and pins.X = attributes, or uses a non-constant default.",
        lock = "tests/semantic_false_diagnostics.rs",
    },
    // conds — CondsCheck: conditional bodies/duplication, pin IO mixing, stubs.
    declare_post_parse_rule! {
        code = crate::errcodes::COND_EMPTY_BODY,
        name = "cond-empty-body",
        title = "conditional block has an empty body",
        severity = Warning,
        domain = Structure,
        host = "conds",
        doc = "Conditional block has an empty body.",
        lock = "tests/semantic_false_diagnostics.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::COND_IF_WITHOUT_ELSE,
        name = "cond-if-without-else",
        title = "if without a matching else",
        severity = Info,
        domain = Structure,
        host = "conds",
        doc = "An `if` without a matching `else`.",
        lock = "tests/semantic_false_diagnostics.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::COND_DUPLICATE,
        name = "cond-duplicate",
        title = "later branch duplicates an earlier condition",
        severity = Warning,
        domain = Structure,
        host = "conds",
        doc = "A later if/else-if branch duplicates an earlier branch's condition and can never be selected.",
        lock = "tests/cond_duplicate.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::PIN_NC_COMPONENT_LEVEL,
        name = "pin-nc-component-level",
        title = "NC pin declared at component level",
        severity = Info,
        domain = PinDecl,
        host = "conds",
        doc = "NC pin used at component level.",
        lock = "tests/lock_pp_conds.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::PIN_IO_MIX_IN_OUT,
        name = "pin-io-mix-in-out",
        title = "pin mixes In and Out IO types",
        severity = Info,
        domain = IO,
        host = "conds",
        doc = "Pin mixes In and Out IO types.",
        lock = "tests/lock_pp_conds.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::PIN_IO_MIX_OUTPUT_POWER,
        name = "pin-io-mix-output-power",
        title = "pin mixes Output and Power IO types",
        severity = Warning,
        domain = IO,
        host = "conds",
        doc = "Pin mixes Output and Power IO types.",
        lock = "tests/lock_pp_conds.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::PIN_IO_MIX_ANALOG_POWER,
        name = "pin-io-mix-analog-power",
        title = "pin mixes Analog and Power IO types",
        severity = Info,
        domain = IO,
        host = "conds",
        doc = "Pin mixes Analog and Power IO types.",
        lock = "tests/lock_pp_conds.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::PARAM_PIN_NAME_SHADOW,
        name = "param-pin-name-shadow",
        title = "parameter shares a pin name",
        severity = Warning,
        domain = NamingStyle,
        host = "conds",
        doc = "Parameter shares its name with a pin.",
        lock = "tests/lock_pp_conds.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::MODULE_STUB,
        name = "module-stub",
        title = "module is a stub",
        severity = Warning,
        domain = Structure,
        host = "conds",
        doc = "Module is a stub.",
        lock = "tests/lock_pp_conds.rs",
    },
    // defs — DefsCheck: cross-kind name collisions, unresolved class refs,
    // `.int` suffix style.
    // Severity adjudication: interface<->enum fires Warning, component<->
    // module fires Info (resolution prefers components there). Catalog default
    // is Warning (conservative); both emissions are kept.
    declare_post_parse_rule! {
        code = crate::errcodes::DEF_AMBIGUOUS_NAME,
        name = "def-ambiguous-name",
        title = "same name used for different definition kinds",
        severity = Warning,
        domain = Duplicate,
        host = "defs",
        doc = "Same name used for different definition kinds.",
        lock = "tests/lock_pp_defs.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::DEF_REF_NOT_LOADED,
        name = "def-ref-not-loaded",
        title = "definition references a class that is not loaded",
        severity = Warning,
        domain = RefIntegrity,
        host = "defs",
        doc = "Definition references a class that is not loaded.",
        lock = "tests/lock_pp_defs.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::COMPONENT_INT_SUFFIX,
        name = "component-int-suffix",
        title = "component name has an unconventional .int suffix",
        severity = Warning,
        domain = NamingStyle,
        host = "defs",
        doc = "Component has an unconventional '.int' suffix.",
        lock = "tests/lock_pp_defs.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::ENUM_INT_SUFFIX,
        name = "enum-int-suffix",
        title = "enum name has an unconventional .int suffix",
        severity = Info,
        domain = NamingStyle,
        host = "defs",
        doc = "Enum has an unconventional '.int' suffix.",
        lock = "tests/lock_pp_defs.rs",
    },
    // imports — ImportsCheck: use-statement path shape and import(..)
    // resolution across files.
    declare_post_parse_rule! {
        code = crate::errcodes::USE_SELF_IMPORT,
        name = "use-self-import",
        title = "file imports itself via a use statement",
        severity = Warning,
        domain = RefIntegrity,
        host = "imports",
        doc = "File imports itself via a use statement.",
        lock = "tests/use_import_codes.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::USE_ALIAS_COLLISION,
        name = "use-alias-collision",
        title = "use alias collides with an existing name",
        severity = Error,
        domain = RefIntegrity,
        host = "imports",
        doc = "A use alias collides with an existing name.",
        lock = "tests/use_statement_diagnostics.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::USE_VERSIONED_TARGET_NOT_FOUND,
        name = "use-versioned-target-not-found",
        title = "versioned use target not found",
        severity = Error,
        domain = RefIntegrity,
        host = "imports",
        doc = "The versioned use target file was not found.",
        lock = "tests/use_import_codes.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::USE_IMPORT_SYMBOL_NOT_FOUND,
        name = "use-import-symbol-not-found",
        title = "import(...) symbol not found in the target file",
        severity = Error,
        domain = RefIntegrity,
        host = "imports",
        doc = "A symbol listed in use import(...) was not found in the target file.",
        lock = "tests/use_import_codes.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::USE_REEXPORT_SYMBOL_NOT_FOUND,
        name = "use-reexport-symbol-not-found",
        title = "pub use import symbol cannot be re-exported",
        severity = Error,
        domain = RefIntegrity,
        host = "imports",
        doc = "A symbol in pub use import(...) was not found and cannot be re-exported.",
        lock = "tests/use_import_codes.rs",
    },
    // interface — InterfaceCheck: interface pin binding and role resolution.
    declare_post_parse_rule! {
        code = crate::errcodes::IFACE_PINS_NOT_ALL_BOUND,
        name = "iface-pins-not-all-bound",
        title = "interface needs more pins than are bound",
        severity = Warning,
        domain = PinDecl,
        host = "interface",
        doc = "Interface requires more pins than are bound to physical pins.",
        lock = "tests/dynamic_pin_access.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::IFACE_ROLE_NOT_FOUND,
        name = "iface-role-not-found",
        title = "param references a role the interface lacks",
        severity = Warning,
        domain = RefIntegrity,
        host = "interface",
        doc = "Interface role referenced by a param does not exist in the interface.",
        lock = "tests/lock_pp_interface.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::IFACE_NOT_LOADED,
        name = "iface-not-loaded",
        title = "interface referenced by a param is not loaded",
        severity = Warning,
        domain = RefIntegrity,
        host = "interface",
        doc = "Interface referenced by a param is not loaded.",
        lock = "tests/lock_pp_interface.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::IFACE_DEPRECATED_CMIE,
        name = "iface-deprecated-cmie",
        title = "deprecated interface/component/param used",
        severity = Info,
        domain = Structure,
        host = "interface",
        doc = "Deprecated interface/component/param used.",
        lock = "tests/lock_pp_interface.rs",
    },
    // naming — NamingCheck: name conventions and library-name shadows.
    // Also fired by the style host at the same Info level (duplicate sweep).
    declare_post_parse_rule! {
        code = crate::errcodes::NAME_COMPONENT_LOWERCASE,
        name = "name-component-lowercase",
        title = "component name starts with lowercase",
        severity = Info,
        domain = NamingStyle,
        host = "naming",
        doc = "Component name starts with lowercase; convention is UPPER_SNAKE.",
        lock = "tests/lock_pp_naming_ports.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::NAME_PIN_MIXED_CONVENTION,
        name = "name-pin-mixed-convention",
        title = "component mixes pin naming conventions",
        severity = Info,
        domain = NamingStyle,
        host = "naming",
        doc = "Pins use mixed naming conventions.",
        lock = "tests/lock_pp_naming_ports.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::NAME_INSTANCE_SINGLE_CHAR,
        name = "name-instance-single-char",
        title = "instance name is a single character",
        severity = Info,
        domain = NamingStyle,
        host = "naming",
        doc = "Instance name is a single character.",
        lock = "tests/semantic_false_diagnostics.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::NAME_PORT_INST_SHADOWS_CMIE,
        name = "name-port-inst-shadows-cmie",
        title = "port/instance name shadows a library CMIE name",
        severity = Warning,
        domain = NamingStyle,
        host = "naming",
        doc = "Port/instance name shadows a library CMIE name.",
        lock = "tests/semantic_false_diagnostics.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::NAME_PARAM_SHADOWS_CMIE,
        name = "name-param-shadows-cmie",
        title = "parameter name shadows a library CMIE name",
        severity = Info,
        domain = NamingStyle,
        host = "naming",
        doc = "Parameter name shadows a library CMIE name.",
        lock = "tests/semantic_false_diagnostics.rs",
    },
    // ports — PortInstanceCheck: port/instance name conflicts in a module.
    declare_post_parse_rule! {
        code = crate::errcodes::PORT_DUPLICATE_NAME,
        name = "port-duplicate-name",
        title = "duplicate port name in the module",
        severity = Error,
        domain = Duplicate,
        host = "ports",
        doc = "Duplicate port name in the module - ambiguous.",
        lock = "tests/lock_pp_naming_ports.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::NAME_PARAM_AND_INSTANCE,
        name = "name-param-and-instance",
        title = "name is both a value param and an instance",
        severity = Warning,
        domain = NamingStyle,
        host = "ports",
        doc = "Name is both a value parameter and an instance.",
        lock = "tests/semantic_false_diagnostics.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::INST_DECLARED_MULTIPLE,
        name = "inst-declared-multiple",
        title = "instance is declared more than once",
        severity = Warning,
        domain = Duplicate,
        host = "ports",
        doc = "Instance is declared more than once in the module.",
        lock = "tests/module_port_interface_ref.rs",
    },
    // refs — RefIntegrityCheck: function-body shape and reference integrity.
    declare_post_parse_rule! {
        code = crate::errcodes::FUNC_PARAMS_NO_BODY,
        name = "func-params-no-body",
        title = "function has params but no body",
        severity = Warning,
        domain = Structure,
        host = "refs",
        doc = "Function has parameters but no body (empty implementation).",
        lock = "tests/lock_pp_refs.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::REF_INTEGRITY,
        name = "ref-integrity",
        title = "reference integrity violation",
        severity = Warning,
        domain = RefIntegrity,
        host = "refs",
        doc = "Reference integrity violation.",
        lock = "tests/lock_pp_refs.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::SPEC_KEY_UNDECLARED_PARAM,
        name = "spec-key-undeclared-param",
        title = "spec key references an undeclared param",
        severity = Error,
        domain = RefIntegrity,
        host = "refs",
        doc = "Spec key references a parameter that is not declared.",
        lock = "tests/lock_pp_refs.rs",
    },
    // style — StyleCheck (registered between refs and exprs in with_defaults)
    // contributes no codes of its own: its NAME_COMPONENT_LOWERCASE sweep is
    // the duplicate of the naming host row declared above, so the table
    // records no style-only row.
    // exprs — ExprsCheck: expression-context validity and attribute values.
    declare_post_parse_rule! {
        code = crate::errcodes::EXPR_THIS_TOP_LEVEL,
        name = "expr-this-top-level",
        title = "'this' used outside an instance/function context",
        severity = Error,
        domain = Structure,
        host = "exprs",
        doc = "'this' used in a top-level net statement; it is only valid inside instance/function contexts.",
        lock = "tests/lock_pp_exprs.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::EXPR_PLACEHOLDER_ONLY,
        name = "expr-placeholder-only",
        title = "connection reaches only a '_' placeholder",
        severity = Warning,
        domain = Structure,
        host = "exprs",
        doc = "Net connects only to '_' placeholder; the connection has no effect.",
        lock = "tests/lock_pp_exprs.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::ATTR_LARGE_INT,
        name = "attr-large-int",
        title = "attribute has a suspiciously large integer",
        severity = Warning,
        domain = Structure,
        host = "exprs",
        doc = "Attribute has a suspiciously large integer value.",
        lock = "tests/lock_pp_exprs.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::ATTR_INFINITE_FLOAT,
        name = "attr-infinite-float",
        title = "attribute has an infinite float value",
        severity = Warning,
        domain = Structure,
        host = "exprs",
        doc = "Attribute has an infinite float value.",
        lock = "tests/lock_pp_exprs.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::RANGE_SINGLE_ELEMENT,
        name = "range-single-element",
        title = "range expands to a single element",
        severity = Info,
        domain = Structure,
        host = "exprs",
        doc = "Range expands to a single element.",
        lock = "tests/lock_pp_exprs.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::IDX_MULTIPLE_SLICE_SPEC,
        name = "idx-multiple-slice-spec",
        title = "IDX key has multiple slice specs",
        severity = Warning,
        domain = Structure,
        host = "exprs",
        doc = "IDX key has multiple slice specifications.",
        lock = "tests/lock_pp_exprs.rs",
    },
    // extra — ExtraCheck: extra declaration/convention checks (J3, U1, R4, I4,
    // M1/M3/M4, U5, D2, D3, F1/F2, R5, N5, B7, spec sub-keys).
    declare_post_parse_rule! {
        code = crate::errcodes::NAME_PORT_SHADOWS_CMIE,
        name = "name-port-shadows-cmie",
        title = "port name shadows a library CMIE name",
        severity = Warning,
        domain = NamingStyle,
        host = "extra",
        doc = "Port name shadows a library CMIE name.",
        lock = "tests/semantic_false_diagnostics.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::ENUM_SINGLE_VALUE,
        name = "enum-single-value",
        title = "enum has only one value",
        severity = Info,
        domain = Structure,
        host = "extra",
        doc = "Enum has only one value.",
        lock = "tests/lock_pp_extra.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::FUNC_EMPTY_BODY,
        name = "func-empty-body",
        title = "function has an empty body",
        severity = Warning,
        domain = Structure,
        host = "extra",
        doc = "Function has an empty body.",
        lock = "tests/lock_pp_extra.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::IFACE_PIN_COUNT_MISMATCH,
        name = "iface-pin-count-mismatch",
        title = "interface expects more pins than are bound",
        severity = Warning,
        domain = PinDecl,
        host = "extra",
        doc = "Interface expects more pins than are bound.",
        lock = "tests/semantic_false_diagnostics.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::COMPONENT_EMPTY,
        name = "component-empty",
        title = "component has no params, pins, attrs, or funcs",
        severity = Warning,
        domain = Structure,
        host = "extra",
        doc = "Component has no params, pins, attributes, or functions.",
        lock = "tests/lock_pp_extra.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::COMPONENT_NO_PINS,
        name = "component-no-pins",
        title = "component declares no pins",
        severity = Warning,
        domain = PinDecl,
        host = "extra",
        doc = "Component has no pin definitions.",
        lock = "tests/lock_pp_extra.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::INTERFACE_EMPTY,
        name = "interface-empty",
        title = "interface has no pins or roles",
        severity = Warning,
        domain = Structure,
        host = "extra",
        doc = "Interface has no pins or roles.",
        lock = "tests/lock_pp_extra.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::PARAM_INT_DEFAULT_STRING,
        name = "param-int-default-string",
        title = "integer param has a string default",
        severity = Error,
        domain = Structure,
        host = "extra",
        doc = "Integer param has a string default.",
        lock = "tests/lock_pp_extra.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::PARAM_STRING_DEFAULT_NUMERIC,
        name = "param-string-default-numeric",
        title = "string param has a numeric default",
        severity = Warning,
        domain = Structure,
        host = "extra",
        doc = "String param has a numeric-looking default.",
        lock = "tests/lock_pp_extra.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::PARAM_UV_DEFAULT_NO_UNIT,
        name = "param-uv-default-no-unit",
        title = "unit-value default has no unit",
        severity = Warning,
        domain = Structure,
        host = "extra",
        doc = "Unit-value param default has no unit suffix (e.g. '5V').",
        lock = "tests/lock_pp_extra.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::DEFINE_NO_ATTRS,
        name = "define-no-attrs",
        title = "define has no attributes",
        severity = Warning,
        domain = Structure,
        host = "extra",
        doc = "Define has no attributes.",
        lock = "tests/lock_pp_extra.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::DEFINE_NON_ATTR_CLAUSE,
        name = "define-non-attr-clause",
        title = "define contains a non-attribute clause",
        severity = Warning,
        domain = Structure,
        host = "extra",
        doc = "Define contains a non-attribute clause.",
        lock = "tests/lock_pp_extra.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::INST_CLASS_NOT_LOADED,
        name = "inst-class-not-loaded",
        title = "instance class is not loaded",
        severity = Warning,
        domain = RefIntegrity,
        host = "extra",
        doc = "Instance references a class that is not loaded.",
        lock = "tests/use_statement_diagnostics.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::BUS_DUPLICATE_MEMBER,
        name = "bus-duplicate-member",
        title = "bus has a duplicate member",
        severity = Warning,
        domain = Duplicate,
        host = "extra",
        doc = "Bus has a duplicate member.",
        lock = "tests/lock_pp_extra.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::COMPONENT_MIXED_CASE,
        name = "component-mixed-case",
        title = "component name is not UPPER_SNAKE",
        severity = Info,
        domain = NamingStyle,
        host = "extra",
        doc = "Component name uses mixed case; convention is UPPER_SNAKE.",
        lock = "tests/lock_pp_extra.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::PARAM_RESERVED_KEYWORD,
        name = "param-reserved-keyword",
        title = "parameter name is a reserved keyword",
        severity = Warning,
        domain = Structure,
        host = "extra",
        doc = "Parameter uses a reserved keyword.",
        lock = "tests/lock_pp_extra.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::FUNC_SHARES_NAME_WITH_PORT,
        name = "func-shares-name-with-port",
        title = "function name collides with a port or param",
        severity = Warning,
        domain = NamingStyle,
        host = "extra",
        doc = "Function shares its name with a port/param.",
        lock = "tests/lock_pp_extra.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::PARAM_NEGATIVE_DEFAULT,
        name = "param-negative-default",
        title = "integer default is negative",
        severity = Warning,
        domain = Structure,
        host = "extra",
        doc = "Integer param default is negative.",
        lock = "tests/lock_pp_extra.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::PARAM_FLOAT_DEFAULT_INVALID,
        name = "param-float-default-invalid",
        title = "param has an invalid float default",
        severity = Error,
        domain = Structure,
        host = "extra",
        doc = "Param has an invalid float default.",
        lock = "tests/lock_pp_extra.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::SPEC_KEY_DUPLICATE,
        name = "spec-key-duplicate",
        title = "spec key appears twice",
        severity = Warning,
        domain = Duplicate,
        host = "extra",
        doc = "Spec key appears more than once.",
        lock = "tests/lock_pp_extra.rs",
    },
    // floating — FloatingLabelCheck: function-body net endpoints that resolve
    // to nothing declared become one-shot dangling labels.
    declare_post_parse_rule! {
        code = crate::errcodes::FUNC_FLOATING_LABEL,
        name = "func-floating-label",
        title = "function-body net endpoint resolves to nothing declared",
        severity = Warning,
        domain = RefIntegrity,
        host = "floating",
        doc = "A function-body net endpoint resolves to nothing declared and floats.",
        lock = "tests/floating_label.rs",
    },
    // gate — GateCheck: the resolve-gate relaxation (keep the bus, not drop)
    // fires when the resulting inline ghost net is referenced only once.
    declare_post_parse_rule! {
        code = crate::errcodes::SINGLE_USE_INLINE_NET,
        name = "single-use-inline-net",
        title = "inline ghost net referenced only once",
        severity = Warning,
        domain = RefIntegrity,
        host = "gate",
        doc = "An inline ghost net (base resolves to no declared instance) is referenced only once; likely a typo or a forgotten declaration.",
        lock = "tests/ignore_warnings.rs",
    },
    // insts — InstsCheck: S1 arg-count mismatch and R2 role name shadow. The
    // role-empty-body / attr-nesting / pin-group / pins-overlap codes this host
    // also fires are declared under the attrs host rows above.
    declare_post_parse_rule! {
        code = crate::errcodes::INST_ARG_COUNT_MISMATCH,
        name = "inst-arg-count-mismatch",
        title = "instance arg count mismatches the class",
        severity = Warning,
        domain = RefIntegrity,
        host = "insts",
        doc = "Instance passes more/fewer args than the class declares.",
        lock = "tests/semantic_false_diagnostics.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::ROLE_NAME_SHADOWS,
        name = "role-name-shadows",
        title = "role name shadows a param or pin",
        severity = Warning,
        domain = NamingStyle,
        host = "insts",
        doc = "Role shares its name with a parameter or pin/port.",
        lock = "tests/lock_pp_attrs_insts.rs",
    },
    // body — BodyCheck: use-path shape, `this :: TYPE`, condition shape, and
    // unused module ports.
    declare_post_parse_rule! {
        code = crate::errcodes::USE_MIXED_PATH_SEPARATORS,
        name = "use-mixed-path-separators",
        title = "use path mixes separators",
        severity = Warning,
        domain = RefIntegrity,
        host = "body",
        doc = "A use path mixes '.' and '/' separators.",
        lock = "tests/use_import_codes.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::INST_THIS_TYPE,
        name = "inst-this-type",
        title = "'this :: TYPE' is not allowed",
        severity = Error,
        domain = Structure,
        host = "body",
        doc = "'this :: TYPE' declaration is not allowed.",
        lock = "tests/lock_pp_body.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::COND_SINGLE_BINARY,
        name = "cond-single-binary",
        title = "condition against a single binary value",
        severity = Info,
        domain = Structure,
        host = "body",
        doc = "Condition compares against a single binary value.",
        lock = "tests/lock_pp_body.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::MODULE_PORT_UNUSED,
        name = "module-port-unused",
        title = "module port is never used",
        severity = Warning,
        domain = PinDecl,
        host = "body",
        doc = "Module port is declared but never connected.",
        lock = "tests/lock_pp_body.rs",
    },
    // hw — HwCheck: hardware-shape advisories (pin numbering/count, power
    // voltage attributes, role binding, IO-type uniformity).
    declare_post_parse_rule! {
        code = crate::errcodes::POWER_PIN_NO_VOLTAGE,
        name = "power-pin-no-voltage",
        title = "power pin without a voltage attribute",
        severity = Info,
        domain = Power,
        host = "hw",
        doc = "Power pin has no voltage attribute.",
        lock = "tests/flatten_net_check_diagnostics.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::HW_PIN_NUMBER_GAP,
        name = "hw-pin-number-gap",
        title = "pin numbers have gaps",
        severity = Info,
        domain = PinDecl,
        host = "hw",
        doc = "Pin numbers have gaps.",
        lock = "tests/lock_pp_hw.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::HW_PIN_COUNT_HIGH,
        name = "hw-pin-count-high",
        title = "pin count is unusually high",
        severity = Info,
        domain = PinDecl,
        host = "hw",
        doc = "Pin count is unusually high.",
        lock = "tests/lock_pp_hw.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::HW_ZERO_PINS_WITH_PARAMS,
        name = "hw-zero-pins-with-params",
        title = "zero pins yet has param attributes",
        severity = Warning,
        domain = PinDecl,
        host = "hw",
        doc = "Component has zero pins but parameter attributes.",
        lock = "tests/lock_pp_hw.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::HW_IFACE_ROLE_UNBOUND,
        name = "hw-iface-role-unbound",
        title = "interface role is never bound",
        severity = Warning,
        domain = PinDecl,
        host = "hw",
        doc = "Interface role is never bound.",
        lock = "tests/semantic_false_diagnostics.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::HW_ALL_SAME_IO_TYPE,
        name = "hw-all-same-io-type",
        title = "all pins share one IO type",
        severity = Info,
        domain = IO,
        host = "hw",
        doc = "All pins have the same IO type.",
        lock = "tests/lock_pp_hw.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::HW_FUNC_PARAM_SHADOWS_PIN,
        name = "hw-func-param-shadows-pin",
        title = "function param shadows a pin name",
        severity = Warning,
        domain = NamingStyle,
        host = "hw",
        doc = "Function parameter shadows a pin name.",
        lock = "tests/lock_pp_hw.rs",
    },
    // types — TypesCheck: value/unit type incompatibility between net points.
    declare_post_parse_rule! {
        code = crate::errcodes::TYPE_INCOMPATIBLE,
        name = "type-incompatible",
        title = "incompatible types or units",
        severity = Warning,
        domain = Structure,
        host = "types",
        doc = "Incompatible types or unit types.",
        lock = "tests/semantic_false_diagnostics.rs",
    },
    // adopt — AdoptionCheck: `:` / `::` target kinds and adopted-func hygiene.
    declare_post_parse_rule! {
        code = crate::errcodes::VARIANT_BASE_NON_ABSTRACT,
        name = "variant-base-non-abstract",
        title = "':' target is not an abstract component",
        severity = Error,
        domain = RefIntegrity,
        host = "adopt",
        doc = "':' target is not an abstract component.",
        lock = "tests/defspace_golden.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::ADOPTS_NON_CAPABILITY,
        name = "adopts-non-capability",
        title = "'::' target is not a capability",
        severity = Error,
        domain = RefIntegrity,
        host = "adopt",
        doc = "'::' target is not a capability.",
        lock = "tests/defspace_golden.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::ADOPTED_FUNC_AMBIGUOUS,
        name = "adopted-func-ambiguous",
        title = "adopted funcs expose the same name",
        severity = Error,
        domain = Duplicate,
        host = "adopt",
        doc = "Two adopted capabilities expose the same func name.",
        lock = "tests/defspace_golden.rs",
    },
    declare_post_parse_rule! {
        code = crate::errcodes::CAPABILITY_SIGNAL_MISSING,
        name = "capability-signal-missing",
        title = "capability signal is missing",
        severity = Error,
        domain = Structure,
        host = "adopt",
        doc = "Adopting component misses a declared capability signal.",
        lock = "tests/defspace_golden.rs",
    },
];
