// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Attribute key registry — the data dictionary behind every attribute decision.
//!
//! A key is registered by the path it is written at: a first-level key
//! (`voltage`), or a key of the `spec` value table (`spec.capacitance`). Below
//! those, values are self-describing and carry no schema of their own.
//!
//! One row answers the four questions its consumers ask: which namespaces the
//! key may be written in ([`AttrKeyDef::faces`]), which kind of value it holds,
//! unit included ([`AttrKeyDef::value`]), which half of a demand/supply contract
//! it states ([`AttrKeyDef::contract`]), and whether it may be a general
//! attribute key at all, and how often ([`AttrKeyDef::general`],
//! [`AttrKeyDef::arity`]).
//!
//! The vocabulary is open and the semantics are closed. A key with no row is
//! legal everywhere and carries no registered meaning: no unit inference, no
//! contract half, and no directional match verdict. Device parameters are
//! unbounded, so no row set enumerates them — a row exists because a consumer
//! has a question to ask, never because a device states a parameter. Adding a
//! row is the only way to extend the vocabulary, and it happens here and in
//! `mcd/spec/07-attrs.md` §3.1, which owns the ledger; this table is its
//! mirror, reconciled row by row by `scripts/check-attr-keys.py`. Library- and
//! project-level extension are registered later: today the ledger is the
//! language core alone.
//!
//! Contract: `mcd/doc/attribute/contract-design.md` §1.7 (G6, the single
//! registration point) and §3.3 (D5, the key decides value semantics). No
//! consumer may compare key strings itself (`if id != "spec"`,
//! `match key.to_lowercase()`) — that is how one dictionary ends up written out
//! three times.
//!
//! Lookup is exact and whole: a key is a name, so `spec.Voltage` and
//! `spec.voltage` are two keys of which only one is registered, and a `spec`
//! key answers to its path, never to its last segment alone.

use crate::semantic::basic::mc_uval::McUnit;

/// A namespace an attribute key can be written in.
///
/// A key is registered under the faces where it is actually written, and a
/// query states the face it is asking from. `voltage` answers a component body
/// and a pin row; `volt` answers only a pin row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttrFace {
    /// A component or module body (`voltage = "3.3V"`). `define` bodies hold
    /// the same attribute clauses and share this face.
    Body,
    /// An interface body (`output = ±5V`, `mcode/ifs/uart.mc`).
    Interface,
    /// A key of the `spec` value table, under its path (`spec.capacitance`).
    Spec,
    /// A value key on a pin row (`volt: 1.2V`).
    PinRow,
}

/// Semantic kind of the values stored under a key (D5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AttrValueKind {
    /// A physical quantity; the unit is part of what the key means.
    Quantity(McUnit),
    /// A name or a classification word.
    Text,
    /// A counted number.
    ///
    /// Unconstructed on purpose, like [`AttrKeyArity::Set`]: every key that
    /// holds a count is a device parameter (`spec.pin_count`), and the ledger
    /// registers common parameters, not device ones.
    #[allow(dead_code)]
    Count,
}

/// Which half of a demand/supply contract a key states.
///
/// A requirement and its guarantee are written under *different* keys — a
/// module's `spec.input_req` (the window it needs on its input) against its
/// `spec.output`, an interface body's `receiver` against its `output` — so a
/// matcher pairs a demand with a supply by this column, never by the key name.
/// The comparison itself (at least / at most / contained) is not registered
/// here: no reader asks for it yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttrContract {
    /// Neither half: recorded and read, never part of a match.
    Plain,
    /// What the writer needs from outside (`spec.input_req`, an interface
    /// body's `receiver`).
    Demand,
    /// What the writer guarantees to the outside (`spec.output`, an interface
    /// body's `output`).
    Supply,
}

/// How many declarations of one key a single attribute list may hold.
///
/// A list is one declaration site (a row's trailing `@attr…`, a body), and it
/// is flat: `@class(analog) @class(digital)` leaves both entries standing, so
/// which one a reader sees depends on the reader. This column is where a key
/// says whether that is allowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttrKeyArity {
    /// The default, and the rule for every key not registered below: one
    /// declaration. A second one in the same list is a duplicate.
    Single,
    /// The declarations accumulate. This is the exception the column exists
    /// for; **no row claims it today** — no key in the live corpus is declared
    /// twice at one site, and none has a union rather than a clash behind its
    /// repetition. The registration point is open here (U43).
    ///
    /// Unconstructed on purpose: the arm is live the day a key needs it, and
    /// the compiler must not be told the question is closed.
    #[allow(dead_code)]
    Set,
}

/// One row of the dictionary: a key path and its columns.
pub(crate) struct AttrKeyDef {
    pub(crate) key: &'static str,
    /// The namespaces the key is written in. A query that states a face the key
    /// is not registered under gets no answer.
    pub(crate) faces: &'static [AttrFace],
    /// May the key be used as a general attribute key (`key = ...`)?
    pub(crate) general: bool,
    /// Kind of value the key holds (D5), unit included; `None` where no kind is
    /// registered for the key. [`value_kind`] is its reader.
    pub(crate) value: Option<AttrValueKind>,
    /// Which half of a demand/supply contract the key states. Registered for
    /// the matcher that pairs a requirement with a guarantee; read by no
    /// consumer until that matcher exists.
    #[allow(dead_code)]
    pub(crate) contract: AttrContract,
    /// Does the key name the supply voltage of the thing it is written on?
    /// ([`is_voltage_key`])
    pub(crate) supply_voltage: bool,
    /// How many declarations of this key one attribute list may hold (U43).
    /// [`arity_of`] is its reader.
    pub(crate) arity: AttrKeyArity,
}

/// Does `key` name a supply voltage, written in `face`?
///
/// The three former substring tests (`contains("volt")`, `contains("vcc")`,
/// `contains("vdd")`) asked one question of three word sets; they now share
/// this row set. `key` is matched exactly — no case folding — and only against
/// the rows registered for `face`, so a pin-row word cannot answer a question
/// asked about a component body.
pub(crate) fn is_voltage_key(key: &str, face: AttrFace) -> bool {
    matches!(lookup(key), Some(d) if d.supply_voltage && d.faces.contains(&face))
}

const BODY: &[AttrFace] = &[AttrFace::Body];
const PIN: &[AttrFace] = &[AttrFace::PinRow];
const IFACE: &[AttrFace] = &[AttrFace::Interface];
const SPEC: &[AttrFace] = &[AttrFace::Spec];
/// `voltage` is written at all three value sites: a component body
/// (`voltage = "5V"`), a pin row (`voltage: [low:…, high:…]`) and an interface
/// body (`mcode/ifs/uart.mc`).
const BODY_PIN_IFACE: &[AttrFace] = &[AttrFace::Body, AttrFace::PinRow, AttrFace::Interface];

/// The dictionary. Rows are added when a consumer needs them; a key with no
/// registered row is not yet known to the compiler, not rejected.
///
/// A key that may repeat in one attribute list is written out as a literal
/// row carrying `AttrKeyArity::Set` rather than through a constructor, so the
/// one exception is visible where it is made.
pub(crate) const ATTR_KEYS: &[AttrKeyDef] = &[
    // Words the grammar reserves in attribute position (N1). They name no
    // value, so they can never be general attribute keys.
    row("this", BODY, false),
    row("pins", BODY, false),
    row("role", BODY, false),
    row("func", BODY, false),
    row("return", BODY, false),
    row("in", BODY, false),
    row("out", BODY, false),
    row("io", BODY, false),
    row("psrc", BODY, false),
    row("psnk", BODY, false),
    row("psbi", BODY, false),
    row("anl", BODY, false),
    row("nc", BODY, false),
    row("if", BODY, false),
    row("else", BODY, false),
    // The nominal value table (`spec.Vout = vout`, `spec = [capacitance = cap]`).
    // Its keys are registered by path further down.
    row("spec", BODY, true),
    // Metadata keys (BOM, `spec/07-attrs.md` §4). Not constructor parameters,
    // never inside `spec`.
    value_row("name", BODY, AttrValueKind::Text),
    value_row("description", BODY, AttrValueKind::Text),
    value_row("partno", BODY, AttrValueKind::Text),
    value_row("package", BODY, AttrValueKind::Text),
    value_row("manufacturer", BODY, AttrValueKind::Text),
    // Voltage words a component body may state its supply voltage with.
    voltage_row(
        "voltage",
        BODY_PIN_IFACE,
        Some(AttrValueKind::Quantity(McUnit::Volt)),
    ),
    // `volt` is a pin-row word: the corpus spells the row's supply voltage
    // `volt: 1.2V` and the component's `voltage = "5V"`.
    voltage_row("volt", PIN, Some(AttrValueKind::Quantity(McUnit::Volt))),
    voltage_row("power", BODY, Some(AttrValueKind::Quantity(McUnit::Wat))),
    voltage_row("vcc", BODY, None),
    voltage_row("vdd", BODY, None),
    voltage_row("vss", BODY, None),
    voltage_row("supply", BODY, None),
    voltage_row("operating_voltage", BODY, None),
    voltage_row("input_voltage", BODY, None),
    voltage_row("output_voltage", BODY, None),
    voltage_row("vrange", BODY, None),
    // General electrical quantities of the `spec` table. Keys beyond these —
    // the unbounded tail of device parameters — are deliberately absent: they
    // are legal, and they carry no registered unit.
    value_row(
        "spec.resistance",
        SPEC,
        AttrValueKind::Quantity(McUnit::Ohm),
    ),
    value_row("spec.impedance", SPEC, AttrValueKind::Quantity(McUnit::Ohm)),
    value_row("spec.esr", SPEC, AttrValueKind::Quantity(McUnit::Ohm)),
    value_row("spec.voltage", SPEC, AttrValueKind::Quantity(McUnit::Volt)),
    value_row("spec.HBM", SPEC, AttrValueKind::Quantity(McUnit::Volt)),
    value_row(
        "spec.capacitance",
        SPEC,
        AttrValueKind::Quantity(McUnit::Cap),
    ),
    value_row(
        "spec.inductance",
        SPEC,
        AttrValueKind::Quantity(McUnit::Ind),
    ),
    value_row("spec.current", SPEC, AttrValueKind::Quantity(McUnit::Amp)),
    value_row(
        "spec.rated_current",
        SPEC,
        AttrValueKind::Quantity(McUnit::Amp),
    ),
    value_row(
        "spec.sat_current",
        SPEC,
        AttrValueKind::Quantity(McUnit::Amp),
    ),
    value_row(
        "spec.ripple_rated",
        SPEC,
        AttrValueKind::Quantity(McUnit::Amp),
    ),
    value_row("spec.frequency", SPEC, AttrValueKind::Quantity(McUnit::Hz)),
    value_row(
        "spec.test_frequency",
        SPEC,
        AttrValueKind::Quantity(McUnit::Hz),
    ),
    value_row("spec.power", SPEC, AttrValueKind::Quantity(McUnit::Wat)),
    value_row(
        "spec.capacity",
        SPEC,
        AttrValueKind::Quantity(McUnit::Charge),
    ),
    value_row(
        "spec.tolerance",
        SPEC,
        AttrValueKind::Quantity(McUnit::Percent),
    ),
    value_row(
        "spec.accuracy",
        SPEC,
        AttrValueKind::Quantity(McUnit::Percent),
    ),
    value_row("spec.temp_min", SPEC, AttrValueKind::Quantity(McUnit::Temp)),
    value_row("spec.temp_max", SPEC, AttrValueKind::Quantity(McUnit::Temp)),
    value_row(
        "spec.life_hours",
        SPEC,
        AttrValueKind::Quantity(McUnit::Time),
    ),
    value_row("spec.length", SPEC, AttrValueKind::Quantity(McUnit::Len)),
    // Classification words of the `spec` table (`spec/07-attrs.md` §3 rule 4).
    value_row("spec.dielectric", SPEC, AttrValueKind::Text),
    value_row("spec.construction", SPEC, AttrValueKind::Text),
    value_row("spec.polarized", SPEC, AttrValueKind::Text),
    value_row("spec.safety_class", SPEC, AttrValueKind::Text),
    value_row("spec.rohs", SPEC, AttrValueKind::Text),
    value_row("spec.derating_note", SPEC, AttrValueKind::Text),
    // The demand/supply pair a converter breaks its tolerance chain with
    // (`doc/power/rail-contract-design.md` §1, §2): a Hoare triple whose
    // pre-condition and post-condition are written under two different keys.
    contract_row(
        "spec.input_req",
        SPEC,
        AttrValueKind::Quantity(McUnit::Volt),
        AttrContract::Demand,
    ),
    contract_row(
        "spec.output",
        SPEC,
        AttrValueKind::Quantity(McUnit::Volt),
        AttrContract::Supply,
    ),
    // An interface body states the same pair under its own two keys
    // (`mcode/ifs/uart.mc`, `can.mc`): `receiver` is the window it accepts,
    // `output` the window it drives. Two keys in a different face — neither is
    // the `spec` path above.
    contract_row(
        "receiver",
        IFACE,
        AttrValueKind::Quantity(McUnit::Volt),
        AttrContract::Demand,
    ),
    contract_row(
        "output",
        IFACE,
        AttrValueKind::Quantity(McUnit::Volt),
        AttrContract::Supply,
    ),
];

const fn row(key: &'static str, faces: &'static [AttrFace], general: bool) -> AttrKeyDef {
    AttrKeyDef {
        key,
        faces,
        general,
        value: None,
        contract: AttrContract::Plain,
        supply_voltage: false,
        arity: AttrKeyArity::Single,
    }
}

const fn value_row(
    key: &'static str,
    faces: &'static [AttrFace],
    value: AttrValueKind,
) -> AttrKeyDef {
    AttrKeyDef {
        key,
        faces,
        general: true,
        value: Some(value),
        contract: AttrContract::Plain,
        supply_voltage: false,
        arity: AttrKeyArity::Single,
    }
}

const fn voltage_row(
    key: &'static str,
    faces: &'static [AttrFace],
    value: Option<AttrValueKind>,
) -> AttrKeyDef {
    AttrKeyDef {
        key,
        faces,
        general: true,
        value,
        contract: AttrContract::Plain,
        supply_voltage: true,
        arity: AttrKeyArity::Single,
    }
}

const fn contract_row(
    key: &'static str,
    faces: &'static [AttrFace],
    value: AttrValueKind,
    contract: AttrContract,
) -> AttrKeyDef {
    AttrKeyDef {
        key,
        faces,
        general: true,
        value: Some(value),
        contract,
        supply_voltage: false,
        arity: AttrKeyArity::Single,
    }
}

/// Look up one key in the dictionary.
pub(crate) fn lookup(key: &str) -> Option<&'static AttrKeyDef> {
    ATTR_KEYS.iter().find(|d| d.key == key)
}

/// Which kind of value does `key` hold (D5)?
///
/// `key` is the whole dotted key as written (`spec.capacitance`), not its last
/// segment and not a folded form of it — see the module header. The kind is a
/// property of the key, not of a face: a row states one kind for every
/// namespace it is registered in.
pub(crate) fn value_kind(key: &str) -> Option<AttrValueKind> {
    lookup(key).and_then(|d| d.value.clone())
}

/// How many declarations of `key` one attribute list may hold (U43).
///
/// An unregistered key is [`AttrKeyArity::Single`]: the default is that a key
/// written twice in one list is a duplicate, and the rows above are where a
/// key says otherwise. `key` is the whole dotted key as written
/// (`spec.sub1`), not its first segment — two different sub-keys of one
/// namespace are two keys, and never duplicates of each other.
pub(crate) fn arity_of(key: &str) -> AttrKeyArity {
    lookup(key).map_or(AttrKeyArity::Single, |d| d.arity)
}

/// Is `key` reserved in attribute position? (N1)
pub(crate) fn is_reserved(key: &str) -> bool {
    matches!(lookup(key), Some(d) if !d.general)
}

/// Is `key` a registered first-level attribute key? (N2)
///
/// Reserved words count as known: N1 (and the dedicated `pins.X` check, N7)
/// own their diagnostics, and N2 must not re-report them.
pub(crate) fn is_known_key(key: &str) -> bool {
    lookup(key).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attrkeys__spec_key_carries_its_unit() {
        assert_eq!(
            value_kind("spec.capacitance"),
            Some(AttrValueKind::Quantity(McUnit::Cap))
        );
        assert_eq!(
            value_kind("spec.tolerance"),
            Some(AttrValueKind::Quantity(McUnit::Percent))
        );
        assert_eq!(value_kind("spec.polarized"), Some(AttrValueKind::Text));
        assert_eq!(
            value_kind("spec.input_req"),
            Some(AttrValueKind::Quantity(McUnit::Volt))
        );
    }

    #[test]
    fn attrkeys__standalone_key_carries_its_kind() {
        assert_eq!(value_kind("name"), Some(AttrValueKind::Text));
        assert_eq!(
            value_kind("voltage"),
            Some(AttrValueKind::Quantity(McUnit::Volt))
        );
    }

    #[test]
    fn attrkeys__lookup_is_exact_and_whole() {
        // Case is not folded: only the spelling that is registered answers.
        assert_eq!(value_kind("spec.Capacitance"), None);
        assert_eq!(value_kind("spec.Voltage"), None);
        // A registered key answers to its whole path, not to its last segment.
        assert_eq!(value_kind("capacitance"), None);
        assert_eq!(value_kind("resistance"), None);
        // A key is registered by the path the corpus writes (`spec.HBM`).
        assert_eq!(
            value_kind("spec.HBM"),
            Some(AttrValueKind::Quantity(McUnit::Volt))
        );
    }

    #[test]
    fn attrkeys__face_decides_which_namespace_answers() {
        // `voltage` answers a body, a pin row and an interface body; `volt`
        // only a pin row.
        assert!(is_voltage_key("voltage", AttrFace::Body));
        assert!(is_voltage_key("voltage", AttrFace::PinRow));
        assert!(is_voltage_key("voltage", AttrFace::Interface));
        assert!(is_voltage_key("volt", AttrFace::PinRow));
        assert!(!is_voltage_key("volt", AttrFace::Body));
        // A key answers to the face it is written in. The interface's guarantee
        // and the spec table's guarantee are two keys, not one path read two
        // ways: `output` (`mcode/ifs/uart.mc`) is not `spec.output`.
        assert_eq!(lookup("output").map(|d| d.faces), Some(IFACE));
        assert_eq!(lookup("spec.output").map(|d| d.faces), Some(SPEC));
        assert!(!lookup("spec.output").is_some_and(|d| d.faces.contains(&AttrFace::Interface)));
    }

    #[test]
    fn attrkeys__contract_half_pairs_demand_with_supply() {
        assert_eq!(
            lookup("spec.input_req").map(|d| d.contract),
            Some(AttrContract::Demand)
        );
        assert_eq!(
            lookup("spec.output").map(|d| d.contract),
            Some(AttrContract::Supply)
        );
        assert_eq!(
            lookup("receiver").map(|d| d.contract),
            Some(AttrContract::Demand)
        );
        assert_eq!(
            lookup("output").map(|d| d.contract),
            Some(AttrContract::Supply)
        );
        assert_eq!(
            lookup("spec.voltage").map(|d| d.contract),
            Some(AttrContract::Plain)
        );
    }

    #[test]
    fn attrkeys__clipped_spec_key_is_not_registered() {
        // A single letter or a clipped form spells a quantity but is not a key
        // (`spec/07-attrs.md` §3 rule 1): only the full word carries a unit.
        assert_eq!(value_kind("spec.r"), None);
        assert_eq!(value_kind("spec.v"), None);
        assert_eq!(value_kind("spec.c"), None);
        assert_eq!(value_kind("spec.l"), None);
        assert_eq!(value_kind("spec.i"), None);
        assert_eq!(value_kind("spec.f"), None);
        assert_eq!(value_kind("spec.t"), None);
        assert_eq!(value_kind("spec.p"), None);
        assert_eq!(value_kind("spec.volt"), None);
        assert_eq!(value_kind("spec.cap"), None);
        assert_eq!(value_kind("spec.freq"), None);
        assert_eq!(value_kind("spec.temp"), None);
        assert_eq!(value_kind("spec.len"), None);
        assert_eq!(value_kind("spec.part"), None);
        assert_eq!(value_kind("spec.tol"), None);
        assert_eq!(
            value_kind("spec.resistance"),
            Some(AttrValueKind::Quantity(McUnit::Ohm))
        );
    }

    #[test]
    fn attrkeys__bom_field_has_no_spec_row() {
        // `spec/07-attrs.md` §4: BOM fields are not constructor parameters and
        // do not live inside `spec`.
        assert_eq!(value_kind("spec.partno"), None);
        assert_eq!(value_kind("spec.name"), None);
        assert_eq!(value_kind("spec.package"), None);
        assert_eq!(value_kind("partno"), Some(AttrValueKind::Text));
    }

    #[test]
    fn attrkeys__unregistered_device_key_is_silent() {
        // The device parameter tail is unbounded: a key outside the ledger is
        // legal and simply carries no registered meaning.
        assert_eq!(value_kind("spec.Rdson"), None);
        assert_eq!(value_kind("spec.Vgs"), None);
        assert!(!is_reserved("spec.Rdson"));
    }

    #[test]
    fn attrkeys__registered_spec_key_is_not_reserved() {
        assert!(!is_reserved("spec.capacitance"));
        assert!(is_reserved("this"));
        assert!(!is_voltage_key("spec.voltage", AttrFace::Spec));
        assert!(is_voltage_key("vcc", AttrFace::Body));
    }
}
