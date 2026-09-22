// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Reference-designator prefixes by component class -- the one prefix table
//! (`refdes-design.md` §2).
//!
//! A prefix belongs to a *declared* class, so it is read by exact lookup on the
//! class name's root segment: `CAP.ELEC` and `CAP.MLCC` answer with the `CAP`
//! row. A class with no row has no prefix -- no consumer may derive one from
//! the spelling, nor from an instance name. Rows cover the mcode system
//! library's top-level classes (`XTAL2`/`XTAL4` are classes in their own
//! right, not variants); a project or third-party class
//! stays unregistered until a row names it.

/// One row of the table: a top-level class name and the prefix its instances
/// take.
pub(crate) struct RefdesPrefixDef {
    pub(crate) class: &'static str,
    pub(crate) prefix: &'static str,
}

const fn row(class: &'static str, prefix: &'static str) -> RefdesPrefixDef {
    RefdesPrefixDef { class, prefix }
}

/// The table, grouped as `refdes-design.md` §2 groups it.
///
/// `M` is reserved for the module segment of a full refdes (`M1C1`), so no
/// prefix below may start with it.
pub(crate) const REFDES_PREFIXES: &[RefdesPrefixDef] = &[
    // Discrete devices.
    row("CAP", "C"),
    row("RES", "R"),
    row("IND", "L"),
    row("DIO", "D"),
    row("XTAL", "Y"),
    row("XTAL2", "Y"),
    row("XTAL4", "Y"),
    row("FUSE", "F"),
    row("XFR", "T"),
    row("RELAY", "K"),
    row("SWITCH", "S"),
    row("TP", "TP"),
    row("ANT", "E"),
    // Semiconductors, emitters, sensors.
    row("TRANS", "Q"),
    row("FET", "QF"),
    row("LED", "LED"),
    row("OPTO", "U"),
    row("SENSOR", "U"),
    // Connectors and electromechanical parts.
    row("CONN", "J"),
    row("WTB", "J"),
    row("USB", "J"),
    row("USB3", "J"),
    row("POWER", "J"),
    row("CIRC", "J"),
    row("AUDIO", "JA"),
    row("VIDEO", "JV"),
    row("HDR", "J"),
    // ICs, power sources, function blocks.
    row("REG", "U"),
    row("AMP", "U"),
    row("OSC", "XO"),
    row("DC", "PS"),
    row("FILTER", "FL"),
];

/// The prefix for `class_name`, or `None` when the class has no row.
///
/// `class_name` is the class name the instance resolves to (`CAP.MLCC`), never
/// a call-site alias and never an instance name.
pub(crate) fn prefix_for_class(class_name: &str) -> Option<&'static str> {
    let root = class_name.split('.').next().unwrap_or(class_name);
    REFDES_PREFIXES
        .iter()
        .find(|d| d.class == root)
        .map(|d| d.prefix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_segment_answers_for_dotted_variants() {
        assert_eq!(prefix_for_class("CAP"), Some("C"));
        assert_eq!(prefix_for_class("CAP.MLCC"), Some("C"));
        assert_eq!(prefix_for_class("DIO.ZEN"), Some("D"));
        assert_eq!(prefix_for_class("FET.MOSFET.N"), Some("QF"));
        assert_eq!(prefix_for_class("HDR.1X9"), Some("J"));
        assert_eq!(prefix_for_class("TP"), Some("TP"));
    }

    #[test]
    fn unregistered_class_has_no_prefix() {
        for class in ["", "RES_0603", "res", "Cap", "FLASH", "uC", "BUTTON"] {
            assert_eq!(prefix_for_class(class), None, "class {class:?}");
        }
    }

    #[test]
    fn table_names_every_class_once_and_keeps_m_reserved() {
        // One row per top-level class; the HDR family collapsed from 27
        // per-face rows to its single family row (b3815), and the stale
        // BUTTON row went with the SWITCH.BUTTON rename (b3815 H1).
        assert_eq!(REFDES_PREFIXES.len(), 32);
        let mut classes: Vec<&str> = REFDES_PREFIXES.iter().map(|d| d.class).collect();
        classes.sort_unstable();
        let rows = classes.len();
        classes.dedup();
        assert_eq!(classes.len(), rows, "duplicate class row");
        for d in REFDES_PREFIXES {
            assert!(!d.prefix.is_empty(), "empty prefix for {}", d.class);
            assert!(!d.prefix.starts_with('M'), "M is reserved: {}", d.prefix);
        }
    }
}
