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
    /// Polarized capacitor (electrolytic / tantalum) - room left, P05 render distinguishes +/- poles
    PolarCapacitor,
    /// Inductor (L)
    Inductor,
    /// Ordinary diode (D)
    Diode,
    /// Light-emitting diode (LED, DS)
    Led,
    /// Zener / TVS / voltage regulator diode
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
            Symbol::PowerRail { .. } => Some(1),
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

    /// Recognize a two-pin component Symbol from `class_name` string
    ///
    /// pass2's `InstEntry.class_name` is usually something like "RES" / "CAP" / "IND" / "DIODE".
    /// Returns `None` if no match found, caller falls back to `Unknown`.
    ///
    /// Case-insensitive.
    pub fn from_class_name(class_name: &str) -> Option<Symbol> {
        let u = class_name.to_uppercase();

        // Exact match
        match u.as_str() {
            "R" | "RES" | "RESISTOR" => return Some(Symbol::Resistor),
            "C" | "CAP" | "CAPACITOR" => return Some(Symbol::Capacitor),
            "C_POL" | "CAP_POL" | "CAP_POLAR" | "CAP_ELECTROLYTIC" | "ECAP" => {
                return Some(Symbol::PolarCapacitor)
            }
            "L" | "IND" | "INDUCTOR" => return Some(Symbol::Inductor),
            "D" | "DIODE" => return Some(Symbol::Diode),
            "LED" | "DS" => return Some(Symbol::Led),
            "ZENER" | "TVS" | "ZD" => return Some(Symbol::Zener),
            _ => {}
        }

        // Prefix heuristic (only allowed for these explicit prefixes, to avoid false hits)
        for (pre, sym) in [
            ("RES_", Symbol::Resistor),
            ("CAP_", Symbol::Capacitor),
            ("IND_", Symbol::Inductor),
            ("DIODE_", Symbol::Diode),
            ("LED_", Symbol::Led),
        ] {
            if u.starts_with(pre) {
                return Some(sym);
            }
        }

        None
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
    fn from_class_name_exact() {
        assert_eq!(Symbol::from_class_name("R"), Some(Symbol::Resistor));
        assert_eq!(Symbol::from_class_name("RES"), Some(Symbol::Resistor));
        assert_eq!(Symbol::from_class_name("Resistor"), Some(Symbol::Resistor));
        assert_eq!(Symbol::from_class_name("C"), Some(Symbol::Capacitor));
        assert_eq!(Symbol::from_class_name("CAP"), Some(Symbol::Capacitor));
        assert_eq!(Symbol::from_class_name("L"), Some(Symbol::Inductor));
        assert_eq!(Symbol::from_class_name("D"), Some(Symbol::Diode));
        assert_eq!(Symbol::from_class_name("LED"), Some(Symbol::Led));
        assert_eq!(Symbol::from_class_name("ZENER"), Some(Symbol::Zener));
        assert_eq!(
            Symbol::from_class_name("CAP_POL"),
            Some(Symbol::PolarCapacitor)
        );
        assert_eq!(
            Symbol::from_class_name("ECAP"),
            Some(Symbol::PolarCapacitor)
        );
    }

    #[test]
    fn from_class_name_prefix() {
        assert_eq!(Symbol::from_class_name("RES_0603"), Some(Symbol::Resistor));
        assert_eq!(Symbol::from_class_name("CAP_0402"), Some(Symbol::Capacitor));
        assert_eq!(Symbol::from_class_name("IND_4_7uH"), Some(Symbol::Inductor));
        assert_eq!(Symbol::from_class_name("LED_RED"), Some(Symbol::Led));
    }

    #[test]
    fn from_class_name_negatives() {
        assert_eq!(Symbol::from_class_name("MCU"), None);
        assert_eq!(Symbol::from_class_name("FPGA"), None);
        assert_eq!(Symbol::from_class_name(""), None);
        assert_eq!(Symbol::from_class_name("?"), None);
        // Should not be false-matched by prefix
        assert_eq!(Symbol::from_class_name("RESET"), None); // not RES_
        assert_eq!(Symbol::from_class_name("DSP"), None); // not D, not DIODE
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
