// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Naming recognition rules' **single source of truth**
//!
//! ## Design principles
//! 1. **Case-insensitive**: all rules first `to_uppercase()`
//! 2. **Conservative priority**: uncertain ones go to Unknown / Generic, don't misclassify
//! 3. **Exact names before prefixes**: `VCC` exact match is more credible than `V*` prefix
//! 4. **Configurable**: main vocabulary extracted as constants, can be expanded to configurable later
//!
//! ## Reference relationships
//! Previously scattered across 4 files, the naming heuristics now all forward to this module:
//! - `detect::is_power_label`     -> `naming::is_power_rail`
//! - `detect::is_signal_like`     -> `naming::is_signal_like`
//! - `kinds::NetKind::classify_by_name` -> `naming::classify_net`
//! - `entry_points::classify_pin` -> `naming::pin_role`
//! - `radial::find_hub`'s `mcu/cpu/soc/fpga` heuristic -> `naming::is_main_chip`

use super::kinds::NetKind;

// ============================================================================
// Public enums
// ============================================================================

/// Role derived from a pin name (for entry-side assignment / render hints)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameRole {
    Power,
    Ground,
    Input,
    Output,
    Clock,
    Reset,
    Generic,
}

// ============================================================================
// vocabulary constants (centrally maintained)
// ============================================================================

/// Power names for whole-string match (must equal the entire string)
const EXACT_POWER: &[&str] = &["VCC", "VDD", "VBUS", "V3P3", "V5P0", "V1P8", "VPP", "AVDD"];

/// Prefix-matched power (`VCC3V3` / `VDD_CORE` / `V3V3` / ...)
const PREFIX_POWER: &[&str] = &["VCC", "VDD", "V3V", "V5V", "V1V"];

const EXACT_GROUND: &[&str] = &["GND", "VSS", "AGND", "DGND", "PGND"];
const PREFIX_GROUND: &[&str] = &["GND", "VSS"];

/// Names that look like inputs (exact name)
const EXACT_INPUT: &[&str] = &[
    "IN", "RX", "RXD", "DIN", "SDI", "MOSI", "SCL", "CLKIN", "CS", "NCS", "EN", "ENABLE",
];
const PREFIX_INPUT: &[&str] = &["IN_"];
const SUFFIX_INPUT: &[&str] = &["_IN"];

/// Names that look like outputs
const EXACT_OUTPUT: &[&str] = &["OUT", "TX", "TXD", "DOUT", "SDO", "MISO", "INT", "IRQ"];
const PREFIX_OUTPUT: &[&str] = &["OUT_"];
const SUFFIX_OUTPUT: &[&str] = &["_OUT"];

const EXACT_CLOCK: &[&str] = &["CLK", "SCK", "SCLK"];
const PREFIX_CLOCK: &[&str] = &["CLK"];

const EXACT_RESET: &[&str] = &["RST", "NRST", "RESET", "RESETN"];

/// Main chip name fragments (for `radial::find_hub`'s hub-bonus scoring)
const MAIN_CHIP_KEYWORDS: &[&str] = &["MCU", "CPU", "SOC", "FPGA", "DSP"];

// ============================================================================
// ★ P0-2: 2-pin class name list + alias normalization
// ============================================================================

/// Known "system library 2-pin" class names (including aliases / dotted form head segments)
///
/// This table answers: "I see a dynamic-pins component instance with class name `XYZ`,
/// should I treat it as a 2-pin component (returning `.1`/`.2`) or as multi-pin?"
///
/// Added types: `CAP / RES / IND / DIODE / DIO / LED / FUSE / ESD / ZENER /
/// TVS / SCHOTTKY / VARISTOR / PULLUP / PULLDOWN / FERRITE / FB`. Dotted form
/// (`DIO.ESD`) takes the first segment `DIO` for a hit.
///
/// Two different things from `canonicalize_class_alias`:
///   - `is_known_twopin_class`: determines **pin topology** (2 pin vs multi pin), affects mc_phrase's
///     `get_left/get_right` pin resolution
///   - `canonicalize_class_alias`: determines **symbol lookup**, redirects `ESD(...)` to `DIO.ESD`'s
///     CMIE table, so bare `ESD(...)` can also go through instantiate_component_construction
const TWOPIN_CLASS_KEYWORDS: &[&str] = &[
    "CAP", "RES", "IND", "DIODE", "DIO", "LED", "FUSE", "ESD", "ZENER", "TVS", "SCHOTTKY",
    "VARISTOR", "PULLUP", "PULLDOWN", "FERRITE", "FB",
];

/// Class name shorthand -> canonical class name actually existing in CMIE
///
/// For example, `ESD(...)` in `.mc` code is a shorthand, the actual CMIE registration is `DIO.ESD`.
/// Before going through `mcb_get_cmie` lookup, use this table to replace the shorthand with the
/// canonical name, otherwise the query fails
/// -> `instantiate_funccall` returns `PassThrough` -> line.rs generates `@?ESD_N` stub
/// -> subsequent resolve can't find `@?ESD_N.1` -> entire net lost (viz.md A1 diagnostic chain).
///
/// **Note**: `Pullup` / `Pulldown` are chain-methods (caller.Pullup(...)), going through
/// `is_builtin_twopin_net_fn` path, **not** in this table, otherwise it would rename the actual
/// RES instance corresponding to the caller.
const CLASS_ALIAS_TO_CANONICAL: &[(&str, &str)] = &[
    ("ESD", "DIO.ESD"),
    ("ZENER", "DIO.ZENER"),
    ("TVS", "DIO.TVS"),
    ("SCHOTTKY", "DIO.SCHOTTKY"),
    ("VARISTOR", "DIO.VARISTOR"),
    ("LED", "DIO.LED"),
    ("FERRITE", "IND.FERRITE"),
    ("FB", "IND.FERRITE"),
];

/// Whether this class name is a known "2-pin class" (including dotted head hit, case-insensitive)
///
/// ```ignore
/// assert!(is_known_twopin_class("CAP"));
/// assert!(is_known_twopin_class("res"));         // case-insensitive
/// assert!(is_known_twopin_class("DIO.ESD"));     // dotted head hit
/// assert!(is_known_twopin_class("IND.FERRITE"));
/// assert!(!is_known_twopin_class("LPA"));
/// assert!(!is_known_twopin_class("FLASH"));
/// ```
pub fn is_known_twopin_class(class_name: &str) -> bool {
    let u = class_name.to_uppercase();
    if TWOPIN_CLASS_KEYWORDS.contains(&u.as_str()) {
        return true;
    }
    if let Some((head, _)) = u.split_once('.') {
        if TWOPIN_CLASS_KEYWORDS.contains(&head) {
            return true;
        }
    }
    false
}

/// Normalize shorthand class name to the canonical name actually registered in CMIE,
/// returns None if no match
///
/// ```ignore
/// assert_eq!(canonicalize_class_alias("ESD"), Some("DIO.ESD".to_string()));
/// assert_eq!(canonicalize_class_alias("zener"), Some("DIO.ZENER".to_string()));
/// assert_eq!(canonicalize_class_alias("CAP"), None);  // CAP itself is already canonical
/// assert_eq!(canonicalize_class_alias("Pullup"), None); // not a class, it's a method
/// ```
pub fn canonicalize_class_alias(class_name: &str) -> Option<String> {
    let u = class_name.to_uppercase();
    for (alias, canonical) in CLASS_ALIAS_TO_CANONICAL {
        if u == *alias {
            return Some((*canonical).to_string());
        }
    }
    None
}

/// ★ ITER-2: additional aliases that only take effect in "bare call" (no caller) form
///
/// `PULLUP` / `PULLDOWN` are essentially a specific semantic role of a resistor (RES), not
/// independent CMIE classes. **As a chain method** (`RES(10k).Pullup(sig, V3V3)`) it's already
/// taken over by `is_builtin_twopin_net_fn`, going through the `wire_builtin_twopin` path, not
/// entering `instantiate_funccall`'s alias fallback -- there **absolutely cannot** do Pullup->RES,
/// otherwise it would create another orphan RES instance on the outer `.Pullup(...)` (this is the
/// thing explicitly warned about in the `CLASS_ALIAS_TO_CANONICAL` comment).
///
/// **But** the bare call `PULLUP(10k)` (no caller, appearing alone as a 2-pin component) is
/// another valid syntax, currently falls to the P0-4 stub path producing `@?PULLUP_N` -- this
/// name doesn't exist in InstTable at all, the entire net is lost (the symptom of hbl's main
/// `__net_5`, failed=["@?PULLUP_1.1"]).
///
/// Fix: add an additional alias fallback to `instantiate_funccall` that **only takes effect when
/// caller is None**, redirecting `PULLUP(...)` / `PULLDOWN(...)` to `RES`, letting it go through
/// the real `instantiate_component_construction` to create RES_N instance.
///
/// ```ignore
/// assert_eq!(canonicalize_class_alias_bare_call("PULLUP"), Some("RES".to_string()));
/// assert_eq!(canonicalize_class_alias_bare_call("Pullup"), Some("RES".to_string()));
/// assert_eq!(canonicalize_class_alias_bare_call("PULLDOWN"), Some("RES".to_string()));
/// assert_eq!(canonicalize_class_alias_bare_call("CAP"), None);
/// ```
pub fn canonicalize_class_alias_bare_call(class_name: &str) -> Option<String> {
    let u = class_name.to_uppercase();
    match u.as_str() {
        "PULLUP" | "PULLDOWN" => Some("RES".to_string()),
        _ => None,
    }
}

// ============================================================================
// Public API: name -> semantics
// ============================================================================

/// Whether a name is power / ground (rough screening, used for "is this a PowerLabel")
pub fn is_power_rail(name: &str) -> bool {
    is_power(name) || is_ground(name)
}

/// Strict "is this power" (excluding ground)
pub fn is_power(name: &str) -> bool {
    let u = name.to_uppercase();
    if EXACT_POWER.contains(&u.as_str()) {
        return true;
    }
    if PREFIX_POWER.iter().any(|p| u.starts_with(p)) {
        return true;
    }
    // Recognize voltage patterns like `3V3` / `5V0` / `+5V` / `1V8`
    matches!(detect_voltage_pattern(&u), Some(VoltagePattern::Power))
}

/// Strict "is this ground"
pub fn is_ground(name: &str) -> bool {
    let u = name.to_uppercase();
    EXACT_GROUND.contains(&u.as_str()) || PREFIX_GROUND.iter().any(|p| u.starts_with(p))
}

/// Pin naming role (replaces legacy `entry_points::classify_pin`)
pub fn pin_role(name: &str) -> NameRole {
    let u = name.to_uppercase();

    if is_power(&u) {
        return NameRole::Power;
    }
    if is_ground(&u) {
        return NameRole::Ground;
    }
    if EXACT_RESET.contains(&u.as_str()) {
        return NameRole::Reset;
    }
    if EXACT_CLOCK.contains(&u.as_str()) || PREFIX_CLOCK.iter().any(|p| u.starts_with(p)) {
        return NameRole::Clock;
    }
    if matches_role(&u, EXACT_INPUT, PREFIX_INPUT, SUFFIX_INPUT) {
        return NameRole::Input;
    }
    if matches_role(&u, EXACT_OUTPUT, PREFIX_OUTPUT, SUFFIX_OUTPUT) {
        return NameRole::Output;
    }
    NameRole::Generic
}

/// Whether a name looks like "signal" (used for rail-synth, excludes pin numbers / `@` prefix)
///
/// Rules:
/// - Must not start with `@` (excludes inst_table auto-numbered instances like `@CAP1`)
/// - Length >= 2
/// - Must not be all digits (excludes pin numbers `1` / `14`)
pub fn is_signal_like(name: &str) -> bool {
    if name.is_empty() || name.starts_with('@') {
        return false;
    }
    if name.len() < 2 {
        return false;
    }
    if name.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    true
}

/// Whether a box name is the main chip (used by radial for hub-bonus calculation)
pub fn is_main_chip(name: &str) -> bool {
    let u = name.to_uppercase();
    MAIN_CHIP_KEYWORDS.iter().any(|k| u.contains(k))
}

/// Derive `NetKind` from net name (replaces legacy `NetKind::classify_by_name`)
pub fn classify_net(name: &str) -> NetKind {
    if is_ground(name) {
        NetKind::Ground
    } else if is_power(name) {
        NetKind::Power
    } else {
        NetKind::Signal
    }
}

// ============================================================================
// Internal helpers
// ============================================================================

#[derive(Debug)]
enum VoltagePattern {
    Power,
}

/// Recognize voltage patterns like `1V8` / `3V3` / `+5V` / `+12V` / `5V0`
///
/// Accepted forms (already to_uppercase):
/// - `<digits>V<digits?>` e.g. `3V3`, `5V0`, `1V8`, `12V`
/// - `+<digits>V<digits?>` e.g. `+5V`, `+12V`, `+3V3`
/// - `<digits>V` e.g. `5V`, `12V`
fn detect_voltage_pattern(u: &str) -> Option<VoltagePattern> {
    let s = u.trim_start_matches(['+', '-']);
    let v_idx = s.find('V')?;
    let head = &s[..v_idx];
    let tail = &s[v_idx + 1..];

    let head_ok = !head.is_empty() && head.chars().all(|c| c.is_ascii_digit());
    let tail_ok = tail.is_empty() || tail.chars().all(|c| c.is_ascii_digit());

    if head_ok && tail_ok {
        Some(VoltagePattern::Power)
    } else {
        None
    }
}

fn matches_role(u: &str, exact: &[&str], prefix: &[&str], suffix: &[&str]) -> bool {
    exact.contains(&u)
        || prefix.iter().any(|p| u.starts_with(p))
        || suffix.iter().any(|s| u.ends_with(s))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_power_exact() {
        assert!(is_power("VCC"));
        assert!(is_power("VDD"));
        assert!(is_power("VBUS"));
        assert!(is_power("V3P3"));
        assert!(is_power("V5P0"));
        assert!(is_power("V1P8"));
        assert!(is_power("AVDD"));
        assert!(is_power("VPP"));
    }

    #[test]
    fn is_power_prefix() {
        assert!(is_power("VCC_CORE"));
        assert!(is_power("VDD_IO"));
        assert!(is_power("V3V3"));
        assert!(is_power("V5V0"));
        assert!(is_power("V1V8"));
    }

    #[test]
    fn is_power_voltage_pattern() {
        // digit-V-digit
        assert!(is_power("3V3"));
        assert!(is_power("5V0"));
        assert!(is_power("1V8"));
        assert!(is_power("12V"));
        // with +/- sign
        assert!(is_power("+5V"));
        assert!(is_power("+3V3"));
        assert!(is_power("-12V"));
    }

    #[test]
    fn is_power_negatives() {
        assert!(!is_power("GND"));
        assert!(!is_power("VSS"));
        assert!(!is_power("RX"));
        assert!(!is_power("PA0"));
        assert!(!is_power(""));
        assert!(!is_power("1"));
    }

    #[test]
    fn is_ground_basic() {
        assert!(is_ground("GND"));
        assert!(is_ground("VSS"));
        assert!(is_ground("AGND"));
        assert!(is_ground("DGND"));
        assert!(is_ground("PGND"));
        assert!(is_ground("GND_DIG"));
        assert!(is_ground("VSS_CORE"));

        assert!(!is_ground("VCC"));
        assert!(!is_ground("RX"));
    }

    #[test]
    fn is_power_rail_includes_both() {
        assert!(is_power_rail("VCC"));
        assert!(is_power_rail("GND"));
        assert!(!is_power_rail("RX"));
    }

    #[test]
    fn pin_role_power_ground() {
        assert_eq!(pin_role("VCC"), NameRole::Power);
        assert_eq!(pin_role("VDD"), NameRole::Power);
        assert_eq!(pin_role("V3V3"), NameRole::Power);
        assert_eq!(pin_role("V5V0"), NameRole::Power);
        assert_eq!(pin_role("VBUS"), NameRole::Power);

        assert_eq!(pin_role("GND"), NameRole::Ground);
        assert_eq!(pin_role("VSS"), NameRole::Ground);
        assert_eq!(pin_role("AGND"), NameRole::Ground);
        assert_eq!(pin_role("DGND"), NameRole::Ground);
    }

    #[test]
    fn pin_role_io() {
        assert_eq!(pin_role("MOSI"), NameRole::Input);
        assert_eq!(pin_role("MISO"), NameRole::Output);
        assert_eq!(pin_role("RX"), NameRole::Input);
        assert_eq!(pin_role("TX"), NameRole::Output);
        assert_eq!(pin_role("INT"), NameRole::Output);
        assert_eq!(pin_role("DATA_IN"), NameRole::Input);
        assert_eq!(pin_role("ADDR_OUT"), NameRole::Output);
        assert_eq!(pin_role("IN_0"), NameRole::Input);
        assert_eq!(pin_role("OUT_5"), NameRole::Output);
    }

    #[test]
    fn pin_role_clock_reset() {
        assert_eq!(pin_role("CLK"), NameRole::Clock);
        assert_eq!(pin_role("SCK"), NameRole::Clock);
        assert_eq!(pin_role("SCLK"), NameRole::Clock);
        assert_eq!(pin_role("CLKOUT"), NameRole::Clock);

        assert_eq!(pin_role("RST"), NameRole::Reset);
        assert_eq!(pin_role("NRST"), NameRole::Reset);
        assert_eq!(pin_role("RESET"), NameRole::Reset);
        assert_eq!(pin_role("RESETN"), NameRole::Reset);
    }

    #[test]
    fn pin_role_generic() {
        // Pure digits
        assert_eq!(pin_role("1"), NameRole::Generic);
        assert_eq!(pin_role("14"), NameRole::Generic);
        // Generic GPIO names
        assert_eq!(pin_role("PA0"), NameRole::Generic);
        assert_eq!(pin_role("GPIO5"), NameRole::Generic);
    }

    #[test]
    fn signal_like() {
        assert!(is_signal_like("VCC"));
        assert!(is_signal_like("RX"));
        assert!(is_signal_like("DATA_IN"));

        assert!(!is_signal_like("1"));
        assert!(!is_signal_like("14"));
        assert!(!is_signal_like("@CAP1"));
        assert!(!is_signal_like(""));
        assert!(!is_signal_like("V")); // length < 2
    }

    #[test]
    fn main_chip() {
        assert!(is_main_chip("mcu513"));
        assert!(is_main_chip("STM32_CPU"));
        assert!(is_main_chip("FPGA_top"));
        assert!(is_main_chip("DSP_core"));
        assert!(is_main_chip("SOC_main"));

        assert!(!is_main_chip("R1"));
        assert!(!is_main_chip("C5"));
        assert!(!is_main_chip("flash"));
    }

    #[test]
    fn classify_net_dispatch() {
        assert_eq!(classify_net("VCC"), NetKind::Power);
        assert_eq!(classify_net("V3V3"), NetKind::Power);
        assert_eq!(classify_net("GND"), NetKind::Ground);
        assert_eq!(classify_net("VSS"), NetKind::Ground);
        assert_eq!(classify_net("RX"), NetKind::Signal);
        assert_eq!(classify_net("__net_42"), NetKind::Signal);
    }

    // ── ★ P0-2 tests ──────────────────────────────────────────────────────

    #[test]
    fn twopin_class_exact_match() {
        // Originally hard-coded in mc_phrase.rs's list
        assert!(is_known_twopin_class("CAP"));
        assert!(is_known_twopin_class("RES"));
        assert!(is_known_twopin_class("IND"));
        assert!(is_known_twopin_class("DIODE"));
        assert!(is_known_twopin_class("LED"));
        assert!(is_known_twopin_class("FUSE"));
        // Aliases added in P0-2
        assert!(is_known_twopin_class("DIO"));
        assert!(is_known_twopin_class("ESD"));
        assert!(is_known_twopin_class("ZENER"));
        assert!(is_known_twopin_class("PULLUP"));
        assert!(is_known_twopin_class("PULLDOWN"));
    }

    #[test]
    fn twopin_class_case_insensitive() {
        assert!(is_known_twopin_class("cap"));
        assert!(is_known_twopin_class("Res"));
        assert!(is_known_twopin_class("esd"));
    }

    #[test]
    fn twopin_class_dotted_head() {
        // In hbl, when `DIO.ESD dio1` is registered, class.name == "DIO.ESD"
        // mc_phrase's is_known_2pin_class should hit
        assert!(is_known_twopin_class("DIO.ESD"));
        assert!(is_known_twopin_class("DIO.ZENER"));
        assert!(is_known_twopin_class("DIO.SCHOTTKY"));
        assert!(is_known_twopin_class("IND.FERRITE"));
    }

    #[test]
    fn twopin_class_negatives() {
        // Multi-pin components must not be misjudged
        assert!(!is_known_twopin_class("LPA"));
        assert!(!is_known_twopin_class("FLASH"));
        assert!(!is_known_twopin_class("SPEAKER"));
        assert!(!is_known_twopin_class("US513"));
        assert!(!is_known_twopin_class("LDO.SGM")); // SGM not in the table
        assert!(!is_known_twopin_class(""));
    }

    #[test]
    fn alias_resolves_to_canonical() {
        assert_eq!(canonicalize_class_alias("ESD"), Some("DIO.ESD".to_string()));
        assert_eq!(canonicalize_class_alias("esd"), Some("DIO.ESD".to_string()));
        assert_eq!(
            canonicalize_class_alias("Zener"),
            Some("DIO.ZENER".to_string())
        );
        assert_eq!(canonicalize_class_alias("LED"), Some("DIO.LED".to_string()));
        assert_eq!(
            canonicalize_class_alias("FERRITE"),
            Some("IND.FERRITE".to_string())
        );
    }

    #[test]
    fn canonical_names_are_unchanged() {
        // Canonical names (already in CMIE) should not be redirected again
        assert_eq!(canonicalize_class_alias("CAP"), None);
        assert_eq!(canonicalize_class_alias("RES"), None);
        assert_eq!(canonicalize_class_alias("DIO.ESD"), None);
        assert_eq!(canonicalize_class_alias("IND.FERRITE"), None);
        // Pullup / Pulldown are methods, not class aliases
        assert_eq!(canonicalize_class_alias("Pullup"), None);
        assert_eq!(canonicalize_class_alias("Pulldown"), None);
    }
}
