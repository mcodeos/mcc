// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! [`Symbol`] -- component's **symbol type** (semantic role, finer than `BoxKind`)
//!
//! ## Difference from `BoxKind`
//! - [`super::kinds::BoxKind`]: coarse classification, 4 categories (TwoPin / MultiPin / SubModule / PowerLabel)
//!   determines the rendered **geometric shape** (rectangle / module frame / label)
//! - `Symbol`: fine classification, determines **which symbol to draw** (resistor wave vs capacitor bars vs IC rectangle)
//!
//! ## Source
//! `Symbol` is computed once by [`super::detect::detect_symbol`] during the builder phase,
//! afterwards all modules are read-only and don't recompute. This replaces the past approach
//! in `two_pin.rs` of using fuzzy `class_name.contains("CAP")` string matching.
//!
//! ## P05 (future) role
//! The P05 renderer will draw the corresponding standard electrical symbol for each `Symbol`:
//! - `Resistor` -> zigzag (IEEE) or rectangle (IEC)
//! - `Capacitor` -> two short bars
//! - `Inductor` -> half-circle arcs
//! - `Diode` -> triangle + short bar (anode -> cathode)
//! - etc.
//!
//! P01 only fills the field, allowing later reading; the symbol drawing is P05's job.

use std::fmt;

use super::netdef::IoDirection;

// ============================================================================
// Symbol enum
// ============================================================================

/// Component symbol type
///
/// Adds semantic information of "what kind of component" beyond `BoxKind`.
/// `Unknown` is the fallback, rendering degrades to a regular rectangle per `BoxKind`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Symbol {
    // ── Two-pin components (BoxKind::TwoPin) ──
    /// Resistor (R)
    Resistor,
    /// Ordinary capacitor (C)
    Capacitor,
    /// Polarized capacitor (electrolytic / tantalum) — renders the `+` polarity mark.
    /// **Reserved**: never name-derived (`from_class_name` maps every `CAP.*` to
    /// `Capacitor`); reachable only once a def-driven path reads the class's
    /// `polarized` attribute.
    PolarCapacitor,
    /// Inductor (L)
    Inductor,
    /// Ordinary diode (D)
    Diode,
    /// Light-emitting diode (LED, DS)
    Led,
    /// Zener / TVS / voltage regulator diode — renders the bent-bar symbol.
    /// **Reserved**: never name-derived (`from_class_name` maps every `DIO.*`
    /// including `DIO.ZEN`/`DIO.TVS` to `Diode`); reachable only via a def-driven path.
    Zener,

    // ── Multi-pin components (BoxKind::MultiPin) ──
    /// Generic multi-pin IC (>= 3 pins)
    Ic,

    // ── Sub-modules ──
    /// Expandable sub-module
    Module,

    // ── Power labels (BoxKind::PowerLabel) ──
    /// Power / ground label
    PowerRail { is_ground: bool },

    /// A non-power label dot / junction (e.g. `Vin`, `DATA`)
    Dot,

    /// ★ P7-8: boundary port terminal (BoxKind::PortTerminal)
    PortTerminal { io: IoDirection },

    /// ★ C1b: test point (TP) — single pad with probe label
    TestPoint,

    // ── Fallback ──
    /// Unrecognized (degrades to BoxKind's default rendering)
    #[default]
    Unknown,
}

impl Symbol {
    /// Expected pin count (for consistency check):
    /// - Two-pin component: `Some(2)`
    /// - IC / Module: `None` (unlimited)
    /// - PowerRail: `Some(1)` (only one connection out)
    /// - Unknown: `None`
    pub fn expected_pins(&self) -> Option<usize> {
        match self {
            Symbol::Resistor
            | Symbol::Capacitor
            | Symbol::PolarCapacitor
            | Symbol::Inductor
            | Symbol::Diode
            | Symbol::Led
            | Symbol::Zener => Some(2),
            Symbol::PowerRail { .. } | Symbol::PortTerminal { .. } | Symbol::TestPoint => Some(1),
            Symbol::Ic | Symbol::Module | Symbol::Dot | Symbol::Unknown => None,
        }
    }

    /// Whether it's a two-pin passive component (R/C/L/D series)
    pub fn is_two_pin_passive(&self) -> bool {
        matches!(
            self,
            Symbol::Resistor
                | Symbol::Capacitor
                | Symbol::PolarCapacitor
                | Symbol::Inductor
                | Symbol::Diode
                | Symbol::Led
                | Symbol::Zener
        )
    }

    /// Whether it's a power label (Power / Ground)
    pub fn is_power_rail(&self) -> bool {
        matches!(self, Symbol::PowerRail { .. })
    }

    /// Whether it's a ground label
    pub fn is_ground(&self) -> bool {
        matches!(self, Symbol::PowerRail { is_ground: true })
    }

    /// Whether it's a multi-pin IC
    pub fn is_ic(&self) -> bool {
        matches!(self, Symbol::Ic)
    }

    /// Recognize a two-pin component Symbol from the **canonical registered class name**
    ///
    /// `InstEntry.class_name` is the resolved class-def name (`RES`, `CAP.ELEC`,
    /// `DIO.ESD`, `LED.RGB`), never a source-string shorthand — aliases (`ESD`,
    /// `ZENER`, `PULLUP`, …) have already been rewritten to canonical form by
    /// `naming::canonicalize_class_alias` before an entry reaches the vector layer.
    ///
    /// Matching takes the class **root token** (first dotted segment) and looks it
    /// up in a small table of the real library families:
    ///
    /// | root   | real registered classes                        | Symbol      |
    /// |--------|-----------------------------------------------|-------------|
    /// | `RES`  | `RES`, `RES.SMD`, `RES.THT`, `RES.POT`, …     | Resistor    |
    /// | `CAP`  | `CAP`, `CAP.ELEC`, `CAP.TANT`, `CAP.SC`, …    | Capacitor   |
    /// | `IND`  | `IND`, `IND.SMD`, `IND.FB`, `IND.CMC`, …      | Inductor    |
    /// | `DIO`  | `DIO`, `DIO.ESD`, `DIO.ZEN`, `DIO.TVS`, …     | Diode       |
    /// | `LED`  | `LED`, `LED.RGB`, `LED.IR`, `LED.HP`          | Led         |
    /// | `TP`   | `TP`                                           | TestPoint   |
    ///
    /// Deliberately **not** matched here:
    /// - single letters / shorthand aliases (`R`, `C`, `ESD`, `ZENER`, `FERRITE`) — these
    ///   are call-site shorthands resolved by `naming::canonicalize_class_alias`, not class names;
    /// - `_`/spelling variants (`RES_0603`, `ECAP`, `CAP_POL`, `TESTPOINT`) that never matched a
    ///   registered library class;
    /// - leaf-level subtypes that carry a distinct **rendered** symbol (polarized cap `+`,
    ///   zener/tvs bent bar) — they split either on a class attribute (`CAP.ELEC` has
    ///   `polarized=true`) or on the real dotted leaf (`DIO.ZEN`, `DIO.TVS`), so they must be
    ///   derived from the class definition, not from the name. Until a def-driven path exists,
    ///   root-token matching keeps them on the family shape (`CAP.ELEC` → Capacitor,
    ///   `DIO.ZEN`/`DIO.TVS` → Diode).
    ///
    /// Returns `None` if no family matches; caller falls back to `Symbol::Unknown`.
    /// Case-insensitive.
    pub fn from_class_name(class_name: &str) -> Option<Symbol> {
        let u = class_name.to_uppercase();
        // Canonical dotted class → root token: `CAP.ELEC` → `CAP`, scalar `RES` → `RES`.
        let root = u.split('.').next().unwrap_or(&u);
        Some(match root {
            "RES" => Symbol::Resistor,
            "CAP" => Symbol::Capacitor,
            "IND" => Symbol::Inductor,
            "DIO" => Symbol::Diode,
            "LED" => Symbol::Led,
            "TP" => Symbol::TestPoint,
            _ => return None,
        })
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Symbol::Resistor => write!(f, "resistor"),
            Symbol::Capacitor => write!(f, "capacitor"),
            Symbol::PolarCapacitor => write!(f, "polar_capacitor"),
            Symbol::Inductor => write!(f, "inductor"),
            Symbol::Diode => write!(f, "diode"),
            Symbol::Led => write!(f, "led"),
            Symbol::Zener => write!(f, "zener"),
            Symbol::Ic => write!(f, "ic"),
            Symbol::Module => write!(f, "module"),
            Symbol::PowerRail { is_ground: true } => write!(f, "ground"),
            Symbol::PowerRail { is_ground: false } => write!(f, "power"),
            Symbol::Dot => write!(f, "dot"),
            Symbol::PortTerminal { io } => write!(f, "port_terminal({io:?})"),
            Symbol::TestPoint => write!(f, "test_point"),
            Symbol::Unknown => write!(f, "unknown"),
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
    fn expected_pins_basic() {
        assert_eq!(Symbol::Resistor.expected_pins(), Some(2));
        assert_eq!(Symbol::Capacitor.expected_pins(), Some(2));
        assert_eq!(Symbol::PolarCapacitor.expected_pins(), Some(2));
        assert_eq!(Symbol::Inductor.expected_pins(), Some(2));
        assert_eq!(Symbol::Diode.expected_pins(), Some(2));
        assert_eq!(Symbol::Led.expected_pins(), Some(2));
        assert_eq!(Symbol::Zener.expected_pins(), Some(2));
        assert_eq!(Symbol::Ic.expected_pins(), None);
        assert_eq!(Symbol::Module.expected_pins(), None);
        assert_eq!(
            Symbol::PowerRail { is_ground: false }.expected_pins(),
            Some(1)
        );
        assert_eq!(Symbol::Unknown.expected_pins(), None);
    }

    #[test]
    fn is_two_pin_passive_truthy() {
        assert!(Symbol::Resistor.is_two_pin_passive());
        assert!(Symbol::Capacitor.is_two_pin_passive());
        assert!(Symbol::Led.is_two_pin_passive());
        assert!(!Symbol::Ic.is_two_pin_passive());
        assert!(!Symbol::Module.is_two_pin_passive());
        assert!(!Symbol::PowerRail { is_ground: false }.is_two_pin_passive());
        assert!(!Symbol::Unknown.is_two_pin_passive());
    }

    #[test]
    fn from_class_name_canonical_root() {
        // Scalar library classes.
        assert_eq!(Symbol::from_class_name("RES"), Some(Symbol::Resistor));
        assert_eq!(Symbol::from_class_name("CAP"), Some(Symbol::Capacitor));
        assert_eq!(Symbol::from_class_name("IND"), Some(Symbol::Inductor));
        assert_eq!(Symbol::from_class_name("DIO"), Some(Symbol::Diode));
        assert_eq!(Symbol::from_class_name("LED"), Some(Symbol::Led));
        assert_eq!(Symbol::from_class_name("TP"), Some(Symbol::TestPoint));
        // Case-insensitive.
        assert_eq!(Symbol::from_class_name("res"), Some(Symbol::Resistor));
        assert_eq!(Symbol::from_class_name("Led"), Some(Symbol::Led));
        assert_eq!(Symbol::from_class_name("tp"), Some(Symbol::TestPoint));
    }

    #[test]
    fn from_class_name_dotted_root() {
        // Real registered library classes take their root family.
        assert_eq!(Symbol::from_class_name("RES.SMD"), Some(Symbol::Resistor));
        assert_eq!(Symbol::from_class_name("RES.THT"), Some(Symbol::Resistor));
        assert_eq!(Symbol::from_class_name("CAP.ELEC"), Some(Symbol::Capacitor));
        assert_eq!(Symbol::from_class_name("CAP.TANT"), Some(Symbol::Capacitor));
        assert_eq!(Symbol::from_class_name("IND.FB"), Some(Symbol::Inductor));
        assert_eq!(Symbol::from_class_name("DIO.ESD"), Some(Symbol::Diode));
        assert_eq!(Symbol::from_class_name("dio.esd"), Some(Symbol::Diode));
        assert_eq!(Symbol::from_class_name("LED.RGB"), Some(Symbol::Led));
        assert_eq!(Symbol::from_class_name("LED.IR"), Some(Symbol::Led));
        // Leaf-level diode subtypes collapse to the family shape (see `from_class_name` doc):
        // telling Zener/TVS apart needs the class definition, not the name.
        assert_eq!(Symbol::from_class_name("DIO.ZEN"), Some(Symbol::Diode));
        assert_eq!(Symbol::from_class_name("DIO.TVS"), Some(Symbol::Diode));
        // A dotted path whose head is not a library family stays unknown.
        assert_eq!(Symbol::from_class_name("MICROPHONE.SIP2"), None);
        assert_eq!(Symbol::from_class_name("XTAL2"), None);
    }

    #[test]
    fn from_class_name_no_alias_or_variant() {
        // Single letters / shorthand aliases are resolved by
        // `naming::canonicalize_class_alias` before flatten — they are not class names here.
        assert_eq!(Symbol::from_class_name("R"), None);
        assert_eq!(Symbol::from_class_name("C"), None);
        assert_eq!(Symbol::from_class_name("L"), None);
        assert_eq!(Symbol::from_class_name("D"), None);
        assert_eq!(Symbol::from_class_name("ESD"), None);
        assert_eq!(Symbol::from_class_name("ZENER"), None);
        assert_eq!(Symbol::from_class_name("RESISTOR"), None);
        // `_`-joined spelling variants never matched a registered class.
        assert_eq!(Symbol::from_class_name("RES_0603"), None);
        assert_eq!(Symbol::from_class_name("CAP_POL"), None);
        assert_eq!(Symbol::from_class_name("ECAP"), None);
        assert_eq!(Symbol::from_class_name("TEST_POINT"), None);
        assert_eq!(Symbol::from_class_name("TESTPOINT"), None);
    }

    #[test]
    fn from_class_name_negatives() {
        assert_eq!(Symbol::from_class_name("MCU"), None);
        assert_eq!(Symbol::from_class_name("FPGA"), None);
        assert_eq!(Symbol::from_class_name(""), None);
        assert_eq!(Symbol::from_class_name("?"), None);
        // Not false-matched by family prefix / spelling.
        assert_eq!(Symbol::from_class_name("RESET"), None); // not RES
        assert_eq!(Symbol::from_class_name("CAPACITOR"), None); // not CAP
        assert_eq!(Symbol::from_class_name("INDUCTANCE"), None); // not IND
        assert_eq!(Symbol::from_class_name("DSP"), None); // not DIO
        assert_eq!(Symbol::from_class_name("LEDGER"), None); // not LED
    }

    #[test]
    fn from_class_name_testpoint() {
        assert_eq!(Symbol::from_class_name("TP"), Some(Symbol::TestPoint));
        assert_eq!(Symbol::TestPoint.expected_pins(), Some(1));
        assert!(!Symbol::TestPoint.is_two_pin_passive());
    }

    #[test]
    fn powerrail_predicates() {
        let p = Symbol::PowerRail { is_ground: false };
        let g = Symbol::PowerRail { is_ground: true };
        assert!(p.is_power_rail());
        assert!(g.is_power_rail());
        assert!(g.is_ground());
        assert!(!p.is_ground());
    }

    #[test]
    fn display() {
        assert_eq!(Symbol::Resistor.to_string(), "resistor");
        assert_eq!(Symbol::Capacitor.to_string(), "capacitor");
        assert_eq!(Symbol::Ic.to_string(), "ic");
        assert_eq!(Symbol::PowerRail { is_ground: true }.to_string(), "ground");
        assert_eq!(Symbol::PowerRail { is_ground: false }.to_string(), "power");
        assert_eq!(Symbol::Unknown.to_string(), "unknown");
    }
}
