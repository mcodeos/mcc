// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Duck typing recognition + naming/IO helpers
//!
//! ## Duck typing background
//! The dedup mechanism of `InstTable.register()` may cause `InstKind` labels to be incorrect:
//! for example, Port registers the `main.flash` path first, then Component registering the same
//! path is skipped, causing flash to be labeled as Port rather than Component.
//!
//! `detect_kind` determines the true identity by checking the child node structure, bypassing
//! InstKind label errors.
//!
//! ## TODO
//! In the long term the registration dedup logic should be fixed in `instant/inst_table`, making
//! InstKind labels trustworthy. Once fixed, this file can shrink by 80%.

use crate::core::common::IOType;
use crate::instant::inst_table::{InstEntry, InstKind, InstTable};

// ============================================================================
// DetectedKind -- duck typing recognition result
// ============================================================================

#[derive(Debug, PartialEq)]
pub enum DetectedKind {
    Component {
        pin_count: usize,
        class_name: String,
    },
    SubModule {
        port_count: usize,
        class_name: String,
    },
    PowerLabel,
    /// A non-power label (e.g. `Vin`) that participates in net connections.
    /// Rendered as a small Dot/Junction box in the schematic.
    Label,
    Skip,
}

// ============================================================================
// Main API: detect_kind
// ============================================================================

/// Detect what an `InstEntry` actually is (by checking child node structure)
///
/// ## Priority (Iter 7)
/// 1. PowerLabel (by name, Component is the exception)
/// 2. SubModule (any Port/Component/Module child node -> module evidence)
/// 3. Component (has Pin child nodes, or Bus expanded members)
/// 4. Skip (Bus itself, unrecognized entries)
pub fn detect_kind(table: &InstTable, id: u32) -> DetectedKind {
    let entry = match table.get_entry(id) {
        Some(e) => e,
        None => return DetectedKind::Skip,
    };

    let name = extract_last_segment(&entry.path);

    // 1. Check if it's a power label
    if is_power_label(&name) && entry.kind != InstKind::Component {
        return DetectedKind::PowerLabel;
    }

    let children = table.children_of(id);

    // 2. Prefer SubModule (Iter 7 priority inversion, avoids mcu513 + 2 Pins being misjudged as TwoPin)
    let port_count = children.iter().filter(|c| c.kind == InstKind::Port).count();
    let has_module_evidence = children.iter().any(|c| {
        c.kind == InstKind::Component
            || c.kind == InstKind::Module
            || c.kind == InstKind::Port
            // Pin in grandchildren also counts as module evidence
            || table.children_of(c.id).iter().any(|gc| gc.kind == InstKind::Pin)
    });

    if has_module_evidence {
        return DetectedKind::SubModule {
            port_count,
            class_name: entry.class_name.clone(),
        };
    }

    // 3. No module evidence, look at Pin child nodes -> Component
    let pin_children: Vec<&InstEntry> = children
        .iter()
        .filter(|c| c.kind == InstKind::Pin)
        .cloned()
        .collect();

    // Fallback: when component's pins use bus expansion syntax, the direct child nodes are not
    // Pin but Bus
    let bus_member_count: usize = children
        .iter()
        .filter(|c| c.kind == InstKind::Bus)
        .map(|bus_entry| table.children_of(bus_entry.id).len())
        .sum();

    let total_pin_like = pin_children.len() + bus_member_count;
    if total_pin_like > 0 {
        return DetectedKind::Component {
            pin_count: total_pin_like,
            class_name: entry.class_name.clone(),
        };
    }

    // ── ★ Phase F.1: Component with no registered Pin children -> still emit box ─────────────
    //
    // Trigger scenario: in the hbl project, many "typed" chips (DCDC.LP3220AB5F, MCU.US513_20_F,
    // LPA4871, MICROPHONE.WM7121P, Crystal2.DST310S, FLASH.GD25Q32E, USB.MINI_B,
    // TEST_POINT, SPEAKER.PHB2AWB, etc.) during Pass2 registration **don't split pins into
    // independent Pin child entries**, but let `chip.PIN_NAME` references go through
    // resolve_netpoint_v2's owner-fallback to resolve back to the chip's own id.
    //
    // Old logic: `total_pin_like == 0` -> default Skip -> these chips have no box at all,
    // sub-module drill-down sees Figure 3 (moddcdc inner layer) where lp322dcdc is missing,
    // and the 4 capacitors + 1 resistor + VDD_3V3 label around it are all floating with no
    // connection -- a disaster.
    //
    // Fix: as long as entry.kind == Component, even without registered pin children, emit a
    // Component box. pin_count is estimated using a class_name heuristic -- only used to decide
    // the shape (TwoPin vs MultiPin), the actual pin connection position is done by Phase 2 BFS
    // mapping all `<chip>.<pin>` references to the chip id, unrelated to the estimated value here.
    //
    // Heuristic rules:
    //   - Known 1-2 pin types (Crystal/Resonator/TestPoint/Speaker/Fuse/Diode/
    //     LED/Zener, MICROPHONE.SIP2 such typed-2pin) -> pin_count = 2
    //   - Others -> 5 (covers typical IC pin count lower bound, lets make_box_from_id go through
    //     MultiPin branch)
    if entry.kind == InstKind::Component {
        let pin_count = guess_chip_pin_count(&entry.class_name);
        // ★ P4 diagnostic: these chips' class definitions don't declare pins -> all
        //   `<chip>.<pin>` references collapse to chip id, pin labels show as chip name.
        //   Printing the class name is the list of classes needing pin declarations in lib.
        crate::velog!(
            "[detect][P4] typed-chip '{}' (class='{}') has NO declared pins -> \
             refs collapse to chip id, pin labels show as '{}'. \
             Declare pins in class '{}' to get real names + side placement.",
            name,
            entry.class_name,
            name,
            entry.class_name
        );
        return DetectedKind::Component {
            pin_count,
            class_name: entry.class_name.clone(),
        };
    }

    // 4. Bus itself is not drawn (members handled individually)
    if entry.kind == InstKind::Bus {
        return DetectedKind::Skip;
    }

    // 5. Label entries (non-power labels like `Vin`, `DATA`, etc.) participate in net
    //    connections. Even though they have no pins, they must be drawn as Dot boxes so that
    //    their nets pass the `classify_nets_by_box_coverage` gate (need >=2 box endpoints).
    //    Power labels are handled in step 1 above; anything reaching here with InstKind::Label
    //    is a non-power label that still needs a box.
    if entry.kind == InstKind::Label && !is_power_label(&name) {
        return DetectedKind::Label;
    }

    DetectedKind::Skip
}

/// ── ★ Phase F.1 helper ───────────────────────────────────────────────────
///
/// Estimate pin count for "Component with no registered Pin children", only used for BoxKind
/// classification (TwoPin <= 2 < MultiPin).
///
/// Doesn't need to be precise -- this is just a fallback for "draw the box first".
fn guess_chip_pin_count(class_name: &str) -> usize {
    if class_name.is_empty() {
        return 2; // conservative: no class name, treat as 2-pin
    }
    let upper = class_name.to_ascii_uppercase();
    // Known 1-2 pin "typed" components (can't discover pin count via normal child scanning)
    let two_pin_prefixes: &[&str] = &[
        "CRYSTAL", // Crystal2.DST310S etc. (2-terminal crystal)
        "RESONATOR",
        "TEST_POINT",
        "TESTPOINT",
        "TP", // TP1, TP2 labels
        "FUSE",
        "VARISTOR",
        "LED",
        "ZENER",
        // typed 2-pin (note: `MICROPHONE.SIP2` is 2pin, `MICROPHONE.WM7121P` is 3pin
        // -- class_name containing `SIP2` such "model number ending in 2" is most likely 2-pin)
        "MICROPHONE.SIP",
        "SPEAKER.", // SPEAKER.PHB2AWB such audio speaker is 2-terminal
    ];
    for p in two_pin_prefixes {
        if upper.starts_with(p) {
            return 2;
        }
    }
    // Default: unknown component with no declared pins, use 0 (no placeholder pins)
    0
}

// ============================================================================
// Public helper functions (shared by detect / from_table / from_block / promote etc.)
// ============================================================================

/// Extract the last segment of a path
///
/// `"main.mcu513.uC.XTAL"` -> `"XTAL"`
/// `"main.mic.MIC/P"` -> `"P"`
pub fn extract_last_segment(path: &str) -> String {
    path.rsplit('.')
        .next()
        .unwrap_or(path)
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .to_string()
}

/// Whether a name looks like a power label (including ground)
///
/// **P04 (S1)**: Implementation has been migrated to [`super::naming::is_power_rail`]
/// This function forwards the call to keep the caller API unchanged.
pub fn is_power_label(name: &str) -> bool {
    super::naming::is_power_rail(name)
}

/// Whether a name looks like a signal name (used by synthesize_rail_edges)
///
/// - Length >= 2 and not all digits (excludes Pin numbers "1"/"2"/"14")
/// - Does not start with `@` (excludes inst_table auto-numbered instance names `@CAP1`/`@RES2`)
///
/// **P04 (S1)**: Implementation has been migrated to [`super::naming::is_signal_like`]
/// This function forwards the call to keep the caller API unchanged.
pub fn is_signal_like(name: &str) -> bool {
    super::naming::is_signal_like(name)
}

/// Compute IO distribution from a list of InstEntries
pub fn compute_io(entries: &[&InstEntry]) -> super::box_def::IoSummary {
    let mut s = super::box_def::IoSummary::new();
    for e in entries {
        match &e.io_type {
            IOType::In => s.inputs += 1,
            IOType::Out => s.outputs += 1,
            IOType::Power | IOType::Analog => s.power += 1,
            _ => s.other += 1,
        }
    }
    s
}

// ============================================================================
// ★ P01 (S2): Symbol recognition / designator extraction / IOType translation
// ============================================================================

use super::kinds::BoxKind;
use super::net_def::IoDirection;
use super::symbol::Symbol;

/// Infer the `Symbol` (fine classification) of a box
///
/// Call order: detect_kind first decides BoxKind, then calls this function to fill Symbol.
///
/// - `PowerLabel` -> `Symbol::PowerRail { is_ground: ... }`
/// - `SubModule`  -> `Symbol::Module`
/// - `MultiPin`   -> `Symbol::Ic`
/// - `TwoPin`     -> look at `class_name` matching R/C/L/D etc. via [`Symbol::from_class_name`];
///                  if no match, `Symbol::Unknown` (to avoid misjudgment)
pub fn detect_symbol(table: &InstTable, id: u32, kind: &BoxKind) -> Symbol {
    let entry = match table.get_entry(id) {
        Some(e) => e,
        None => return Symbol::Unknown,
    };
    let name = extract_last_segment(&entry.path);

    match kind {
        BoxKind::PowerLabel => Symbol::PowerRail {
            is_ground: super::naming::is_ground(&name),
        },
        BoxKind::SubModule => Symbol::Module,
        BoxKind::MultiPin => Symbol::Ic,
        BoxKind::TwoPin => Symbol::from_class_name(&entry.class_name).unwrap_or(Symbol::Unknown),
        BoxKind::Dot => Symbol::Dot,
    }
}

/// Extract designator from instance name (`R1` / `C5` / `U3`)
///
/// Rules:
/// - First letter in the set `{R, C, L, D, U, J, Q, Y, F, X}` (resistor/capacitor/inductor/diode/IC/connector/transistor/crystal/fuse/crystal)
/// - All remaining characters are digits
/// - Name length >= 2
///
/// Returns `None` if not satisfied.
pub fn extract_designator(name: &str) -> Option<String> {
    if name.len() < 2 {
        return None;
    }
    let mut chars = name.chars();
    let first = chars.next()?;
    if !matches!(
        first,
        'R' | 'C' | 'L' | 'D' | 'U' | 'J' | 'Q' | 'Y' | 'F' | 'X'
    ) {
        return None;
    }
    let rest: String = chars.collect();
    if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
        Some(name.to_string())
    } else {
        None
    }
}

/// Extract physical pin number from pin name (`"1"` / `"14"` / `"22"`)
///
/// `"1"` -> `Some(1)`, `"VCC"` -> `None`, `"PA0"` -> `None` (has letter prefix, not a pure number)
pub fn parse_pin_number(name: &str) -> Option<u32> {
    if name.is_empty() {
        return None;
    }
    name.parse::<u32>().ok()
}

/// Translate pass2 `IOType` to viz layer `IoDirection`
///
/// pass2's IOType may have In / Out / Power / Analog / other enum values,
/// we map to the 7 categories needed for drawing. Unknown value -> `Unknown`.
pub fn translate_io_type(t: &IOType) -> IoDirection {
    match t {
        IOType::In => IoDirection::Input,
        IOType::Out => IoDirection::Output,
        // mc `io` -> bidirectional pin (transistor B/C/E, SPI data lines, etc.)
        IOType::InOut => IoDirection::Bidir,
        IOType::Power => IoDirection::Power,
        // pass2's Analog is mostly passive components like resistors / capacitors
        IOType::Analog => IoDirection::Passive,
        // Other cases (pass2 IOType may later extend Ground etc.) -> fallback Unknown
        _ => IoDirection::Unknown,
    }
}

/// Consistency check: check whether `box.pin_count` matches `symbol.expected_pins()`
///
/// Does not block the pipeline, just prints stderr warnings. Can be disabled in production.
pub fn warn_if_pin_mismatch(b: &super::box_def::McVecBox) {
    if let Some(expected) = b.symbol.expected_pins() {
        if b.pin_count != expected && b.pin_count != 0 {
            crate::velog!(
                "[detect] WARN: '{}' symbol={} expects {} pin(s), got {}",
                b.name,
                b.symbol,
                expected,
                b.pin_count
            );
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
    fn designator_basic() {
        assert_eq!(extract_designator("R1"), Some("R1".into()));
        assert_eq!(extract_designator("R123"), Some("R123".into()));
        assert_eq!(extract_designator("C5"), Some("C5".into()));
        assert_eq!(extract_designator("U7"), Some("U7".into()));
        assert_eq!(extract_designator("D2"), Some("D2".into()));
        assert_eq!(extract_designator("J10"), Some("J10".into()));
        assert_eq!(extract_designator("Q1"), Some("Q1".into()));
        assert_eq!(extract_designator("Y1"), Some("Y1".into()));
    }

    #[test]
    fn designator_negative() {
        assert_eq!(extract_designator(""), None);
        assert_eq!(extract_designator("R"), None); // no number
        assert_eq!(extract_designator("RX"), None); // RX is not a number
        assert_eq!(extract_designator("R1A"), None); // suffix letter not allowed
        assert_eq!(extract_designator("VCC"), None);
        assert_eq!(extract_designator("mcu513"), None); // doesn't start with uppercase
        assert_eq!(extract_designator("PA0"), None); // P not in allowed set
    }

    #[test]
    fn pin_number_basic() {
        assert_eq!(parse_pin_number("1"), Some(1));
        assert_eq!(parse_pin_number("14"), Some(14));
        assert_eq!(parse_pin_number("100"), Some(100));
        assert_eq!(parse_pin_number(""), None);
        assert_eq!(parse_pin_number("VCC"), None);
        assert_eq!(parse_pin_number("PA0"), None);
        assert_eq!(parse_pin_number("1A"), None);
    }

    #[test]
    fn translate_iotype_basic() {
        assert_eq!(translate_io_type(&IOType::In), IoDirection::Input);
        assert_eq!(translate_io_type(&IOType::Out), IoDirection::Output);
        assert_eq!(translate_io_type(&IOType::Power), IoDirection::Power);
        assert_eq!(translate_io_type(&IOType::Analog), IoDirection::Passive);
    }
}
