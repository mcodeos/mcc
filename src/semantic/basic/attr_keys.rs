// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Attribute key registry — the data dictionary behind every attribute decision.
//!
//! A key is registered by the path it is written at: a first-level key
//! (`voltage`), or a key of the `spec` value table (`spec.capacitance`). Below
//! those, values are self-describing and carry no schema of their own.
//!
//! One row answers the five questions its consumers ask: which namespaces the
//! key may be written in ([`AttrKeyDef::faces`]), which kind of value it holds,
//! unit included ([`AttrKeyDef::value`]), which words that value may be written
//! with, where the vocabulary is closed ([`AttrKeyDef::vocab`]), which half of a
//! demand/supply contract it states ([`AttrKeyDef::contract`]), and whether it
//! may be a general attribute key at all, and how often
//! ([`AttrKeyDef::general`], [`AttrKeyDef::arity`]).
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

/// What an element **is**, as its own `spec` table declares it — the class a
/// registered `spec` key's presence marks.
///
/// The default question ("is this a decoupling capacitor?") is answered from the
/// declaration, never from a class name or a pin shape (world-axioms §1 A1):
/// `spec.capacitance` marks a capacitor because the key *is* a capacitance, and
/// the unit column of that same row already says `F`. The classes are the three
/// the power-quality axis (PI-1~4) asks about; the fold over a component's whole
/// `spec` key set — which resolves clashes and maps the absence of every marked
/// key to "no class" — lives where the flat table is built
/// ([`crate::instant::insttab::InstEntry::element_class`]).
///
/// Only keys that *decide* the class are registered here. A quantity that merely
/// accompanies an element (`spec.esr` on a capacitor, `spec.dcr` on an inductor)
/// marks nothing: it is written on a part that is already classified by a key
/// that does decide, and a bare `esr` on its own would name no element at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementClass {
    /// It stores charge across its two legs (`spec.capacitance`).
    Capacitive,
    /// It resists a change of current, or presents a frequency-dependent
    /// impedance (`spec.inductance`, `spec.impedance`).
    Magnetic,
    /// It dissipates in a declared resistance (`spec.resistance`).
    Resistive,
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

/// The value face of a key whose values are a **closed set** — the words a
/// declaration may be written with ([`AttrKeyDef::vocab`]).
///
/// A key's *shape* is [`AttrValueKind`]; this column is orthogonal to it: the
/// value of `protect` is text and the value of `noise` is a word, yet only one
/// of the two keys admits an arbitrary word. Which words exist is registered in
/// the ledger's word column (`mcd/spec/07-attrs.md` §3.1, reconciled row by row
/// with `mcc/scripts/check-attr-keys.py`) and mirrored here; the canons
/// (`power/intent-design.md` §5.2, `power/exposed-protection-design.md` §4) keep
/// what the words mean and point back at the ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttrVocab {
    /// One of these words, written exactly. A declaration that writes another
    /// word is reported, never silently read as "no declaration".
    Words(&'static [&'static str]),
    /// No value at all: a flag key is live by being there, and a value written
    /// on it is itself the error (`@star`).
    Flag,
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
    /// The words the key's value is written with, where the key's vocabulary is
    /// closed ([`AttrVocab`]); `None` for every key whose values are open —
    /// including every key with no row at all, which is not judged (the ledger's
    /// "the semantics are closed and the vocabulary is open", `spec/07-attrs.md`
    /// §3.1). [`vocab_of`] is its reader.
    pub(crate) vocab: Option<AttrVocab>,
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
    /// The element class this key's presence marks, where it marks one
    /// ([`ElementClass`]); `None` for every key that does not answer "what is
    /// this element". [`element_of_spec_key`] is its reader.
    pub(crate) element: Option<ElementClass>,
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
/// `role` is written at two declaration sites: a body conduit row (the copper's
/// own identity) and a component pin row (the pin's identity *expectation* —
/// pin-expectation-design.md §3; the row cannot name a conduit, so the word is
/// what the landed copper must anchor). `class` shares the pair for the same
/// reason: a domain's `@class` is the board's classification decision, a pin
/// row's `@class` is the library's signal-class *expectation* (§4, v0.3) —
/// same word, same value table, each face its own layer.
const BODY_PIN: &[AttrFace] = &[AttrFace::Body, AttrFace::PinRow];
const IFACE: &[AttrFace] = &[AttrFace::Interface];
const SPEC: &[AttrFace] = &[AttrFace::Spec];
/// `voltage` is written at all three value sites: a component body
/// (`voltage = "5V"`), a pin row (`voltage: [low:…, high:…]`) and an interface
/// body (`mcode/ifs/uart.mc`).
const BODY_PIN_IFACE: &[AttrFace] = &[AttrFace::Body, AttrFace::PinRow, AttrFace::Interface];

/// The identity-axis keys and the protection gate, by the name the rows below
/// register them under (`mcd/doc/attribute/contract-design.md` §1.8).
///
/// A reader asks for one of these instead of spelling the name, so the word a
/// key is written with lives in its row alone: "no consumer may compare key
/// strings itself" (§1.7 G6) is then true of these keys too.
pub(crate) const KEY_ROLE: &str = "role";
pub(crate) const KEY_CLASS: &str = "class";
pub(crate) const KEY_NATURE: &str = "nature";
pub(crate) const KEY_NOISE: &str = "noise";
pub(crate) const KEY_EXPOSED: &str = "exposed";
pub(crate) const KEY_BIND_ROLE: &str = "bind_role";
pub(crate) const KEY_RETURN: &str = "return";
pub(crate) const KEY_STAR: &str = "star";
pub(crate) const KEY_PROTECT: &str = "protect";

/// The words of the closed sets the rows below register. `role` and `bind_role`
/// share one set, which is what the canon says of them: `bind_role` takes the
/// `role` words.
pub(crate) const WORD_MAIN: &str = "main";
pub(crate) const WORD_QUIET: &str = "quiet";
pub(crate) const WORD_PROTECTIVE: &str = "protective";
pub(crate) const WORD_EARTH: &str = "earth";
pub(crate) const WORD_ISOLATED: &str = "isolated";
pub(crate) const WORD_DIGITAL: &str = "digital";
pub(crate) const WORD_ANALOG: &str = "analog";
pub(crate) const WORD_NOISY: &str = "noisy";
pub(crate) const WORD_SENSITIVE: &str = "sensitive";
pub(crate) const WORD_AC: &str = "ac";
pub(crate) const WORD_DC: &str = "dc";
pub(crate) const WORD_SHUNT: &str = "shunt";
pub(crate) const WORD_SERIES: &str = "series";
pub(crate) const WORD_ESD_CONTACT: &str = "esd_contact";
pub(crate) const WORD_ESD_AIR: &str = "esd_air";
pub(crate) const WORD_EFT: &str = "eft";
pub(crate) const WORD_SURGE: &str = "surge";
pub(crate) const WORD_LIGHTNING: &str = "lightning";

const ROLE_WORDS: &[&str] = &[
    WORD_MAIN,
    WORD_QUIET,
    WORD_PROTECTIVE,
    WORD_EARTH,
    WORD_ISOLATED,
];
const CLASS_WORDS: &[&str] = &[WORD_DIGITAL, WORD_ANALOG];
const NATURE_WORDS: &[&str] = &[WORD_AC, WORD_DC];
const NOISE_WORDS: &[&str] = &[WORD_NOISY, WORD_QUIET, WORD_SENSITIVE];
const EXPOSED_WORDS: &[&str] = &[
    WORD_ESD_CONTACT,
    WORD_ESD_AIR,
    WORD_EFT,
    WORD_SURGE,
    WORD_LIGHTNING,
];
const PROTECT_WORDS: &[&str] = &[WORD_SHUNT, WORD_SERIES];

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
    // `@role`'s values are the ledger's five identity words (R9).
    vocab_row(KEY_ROLE, BODY_PIN, false, AttrVocab::Words(ROLE_WORDS)),
    row("func", BODY, false),
    // `@return` names a return conduit, so its value is a reference rather than
    // a word: no vocabulary is registered for it.
    row(KEY_RETURN, BODY, false),
    row("in", BODY, false),
    row("out", BODY, false),
    row("io", BODY, false),
    row("psrc", BODY, false),
    row("psnk", BODY, false),
    row("psbi", BODY, false),
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
    // The identity axis and the protection gate (contract-design.md §1.8). Each
    // of these keys states a classification word, and the words are the closed
    // sets registered in the ledger's word column (`spec/07-attrs.md` §3.1) —
    // the six identity keys of `power/intent-design.md` §5.2 and `protect`'s two
    // of `power/exposed-protection-design.md` §4. Registering them is what makes
    // a misspelled word reportable instead of reading as "the declaration is
    // absent": a reader of `protect` sees `None` either way.
    // `star` is the one flag: it carries no value, and a value on it is the
    // error — which is the vocabulary check's verdict, so the key is a general
    // one (the reserved column would report the word instead of the value).
    vocab_row(KEY_CLASS, BODY_PIN, true, AttrVocab::Words(CLASS_WORDS)),
    vocab_row(KEY_NATURE, BODY, true, AttrVocab::Words(NATURE_WORDS)),
    vocab_row(KEY_NOISE, BODY, true, AttrVocab::Words(NOISE_WORDS)),
    vocab_row(KEY_EXPOSED, BODY, true, AttrVocab::Words(EXPOSED_WORDS)),
    vocab_row(KEY_BIND_ROLE, BODY, true, AttrVocab::Words(ROLE_WORDS)),
    vocab_row(KEY_STAR, BODY, true, AttrVocab::Flag),
    vocab_row(KEY_PROTECT, BODY, true, AttrVocab::Words(PROTECT_WORDS)),
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
    element_row(
        "spec.resistance",
        SPEC,
        AttrValueKind::Quantity(McUnit::Ohm),
        ElementClass::Resistive,
    ),
    // The bead's only classification key, and the power-quality axis's filter
    // element (PI-2 / SN-2) — the golden board's `IND.FB` legs carry no other
    // spec key. Deliberately read as magnetic on its own: in the library the
    // bare `impedance` key is written by more than one part family — the
    // two-leg ferrite bead (`ind.mc`, `spec = [impedance, rated_current,
    // test_frequency, …]`) and the common-mode choke, but also the antenna
    // (`ant.mc`) and the circular connector (`conn/circular.mc`). No key
    // separates them and A1 forbids asking the class name, so this row is a
    // named over-approximation, not a certificate: a consumer reading the class
    // must judge the element's *shape and placement* too, and may not conclude
    // "filter" from the class alone.
    element_row(
        "spec.impedance",
        SPEC,
        AttrValueKind::Quantity(McUnit::Ohm),
        ElementClass::Magnetic,
    ),
    value_row("spec.esr", SPEC, AttrValueKind::Quantity(McUnit::Ohm)),
    value_row("spec.voltage", SPEC, AttrValueKind::Quantity(McUnit::Volt)),
    value_row("spec.HBM", SPEC, AttrValueKind::Quantity(McUnit::Volt)),
    element_row(
        "spec.capacitance",
        SPEC,
        AttrValueKind::Quantity(McUnit::Cap),
        ElementClass::Capacitive,
    ),
    element_row(
        "spec.inductance",
        SPEC,
        AttrValueKind::Quantity(McUnit::Ind),
        ElementClass::Magnetic,
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
    // The package's own dissipation ceiling (`res.mc` writes it on every
    // resistor family). Registering it is what lets a rule compare an
    // application power against a declared rating instead of reading the key
    // as an unregistered, meaningless tail parameter.
    value_row(
        "spec.power_rated",
        SPEC,
        AttrValueKind::Quantity(McUnit::Wat),
    ),
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
    // The differential pair an interface body declares (`mcode/ifs/adcdiff.mc`):
    // two of its own pins, the first named being the positive face.
    value_row("diff_pair", IFACE, AttrValueKind::Text),
];

const fn row(key: &'static str, faces: &'static [AttrFace], general: bool) -> AttrKeyDef {
    AttrKeyDef {
        key,
        faces,
        general,
        value: None,
        vocab: None,
        contract: AttrContract::Plain,
        supply_voltage: false,
        arity: AttrKeyArity::Single,
        element: None,
    }
}

/// A row for a key whose values come from a closed set ([`AttrVocab`]). Written
/// through its own constructor so the closedness of a key is visible where the
/// key is registered, and so a reader of the table can tell it from a key whose
/// values are open (`row`).
const fn vocab_row(
    key: &'static str,
    faces: &'static [AttrFace],
    general: bool,
    vocab: AttrVocab,
) -> AttrKeyDef {
    AttrKeyDef {
        key,
        faces,
        general,
        value: None,
        vocab: Some(vocab),
        contract: AttrContract::Plain,
        supply_voltage: false,
        arity: AttrKeyArity::Single,
        element: None,
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
        vocab: None,
        contract: AttrContract::Plain,
        supply_voltage: false,
        arity: AttrKeyArity::Single,
        element: None,
    }
}

/// A row for a key whose presence marks what an element **is**
/// ([`AttrKeyDef::element`]). Separate from [`value_row`] so the class is
/// written where the key is, next to the unit that says the same thing.
const fn element_row(
    key: &'static str,
    faces: &'static [AttrFace],
    value: AttrValueKind,
    element: ElementClass,
) -> AttrKeyDef {
    AttrKeyDef {
        key,
        faces,
        general: true,
        value: Some(value),
        vocab: None,
        contract: AttrContract::Plain,
        supply_voltage: false,
        arity: AttrKeyArity::Single,
        element: Some(element),
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
        vocab: None,
        contract: AttrContract::Plain,
        supply_voltage: true,
        arity: AttrKeyArity::Single,
        element: None,
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
        vocab: None,
        contract,
        supply_voltage: false,
        arity: AttrKeyArity::Single,
        element: None,
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

/// What does this key's presence mark the element as ([`ElementClass`])?
///
/// `key` is the whole dotted key as written (`spec.capacitance`), like every
/// other reader here. `None` for every key that answers no such question —
/// including the quantities that merely accompany a marked element
/// (`spec.esr`, `spec.dcr`).
pub(crate) fn element_of_key(key: &str) -> Option<ElementClass> {
    lookup(key).and_then(|d| d.element)
}

/// [`element_of_key`] asked with a **`spec` sub-key** the way the body writes
/// it (`capacitance`) — the spelling the `spec` table hands its reader. The
/// registered path is built here, where the path form lives, so no caller
/// re-spells it.
pub(crate) fn element_of_spec_key(sub: &str) -> Option<ElementClass> {
    element_of_key(&format!("spec.{sub}"))
}

/// The closed word set `key`'s values must come from ([`AttrVocab`]), where the
/// dictionary registers one.
///
/// `None` for a key whose values are open — which includes every key with no row
/// at all: a key the ledger does not know carries no registered meaning, so no
/// word is outside anything. `key` is the whole dotted key as written, like
/// every other reader here.
pub(crate) fn vocab_of(key: &str) -> Option<AttrVocab> {
    lookup(key).and_then(|d| d.vocab)
}

/// Does `key` open a value-table namespace — a name the dictionary registers
/// rows *under*, by path?
///
/// A definition writes such a table two ways (G2, one fact): as a table-valued
/// attribute (`spec = [capacitance = cap]`, the namespace is the whole key) and
/// as dotted keys (`spec.capacitance = cap`, the namespace is the first
/// segment). A reader that walks a definition's attribute list for one of them
/// asks this first, so the namespace name is written in one place instead of in
/// each walker.
///
/// Derived, not registered: a namespace is exactly a prefix under which the
/// dictionary holds path rows, so the [`AttrFace::Spec`] rows answer for `spec`
/// without a column restating what their keys already say. A name nothing is
/// registered under opens no table.
/// The component spec-table attribute: `spec = [...]` holds the whole table,
/// `spec.<key>` rows extend it. Consumers read this constant instead of
/// spelling the key, so a rename is a one-line ledger edit.
pub(crate) const SPEC_TABLE_KEY: &str = "spec";

pub(crate) fn is_table_namespace(key: &str) -> bool {
    let prefix = format!("{key}.");
    ATTR_KEYS
        .iter()
        .any(|d| d.faces.contains(&AttrFace::Spec) && d.key.starts_with(&prefix))
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
        // `diff_pair` is written by an interface body and by nothing else.
        assert_eq!(lookup("diff_pair").map(|d| d.faces), Some(IFACE));
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

    #[test]
    fn attrkeys__element_column_marks_the_keys_that_decide_the_class() {
        // The key that *is* the quantity decides the class (world-axioms §1 A1).
        assert_eq!(
            element_of_key("spec.capacitance"),
            Some(ElementClass::Capacitive)
        );
        assert_eq!(
            element_of_key("spec.resistance"),
            Some(ElementClass::Resistive)
        );
        assert_eq!(
            element_of_key("spec.inductance"),
            Some(ElementClass::Magnetic)
        );
        // `impedance` is the column's one over-approximation: a ferrite bead, a
        // common-mode choke, an antenna and a connector all declare it and no
        // key separates them, so the surplus is silence, never a false class.
        assert_eq!(
            element_of_key("spec.impedance"),
            Some(ElementClass::Magnetic)
        );
        // An accompanying quantity decides nothing: `esr`/`dcr` ride on a part
        // the deciding key already classified, and alone they name no element.
        assert_eq!(element_of_key("spec.esr"), None);
        assert_eq!(element_of_key("spec.dcr"), None);
        assert_eq!(element_of_key("spec.tolerance"), None);
        assert_eq!(element_of_key("voltage"), None);
        assert_eq!(element_of_key("name"), None);
    }

    #[test]
    fn attrkeys__element_lookup_is_whole_and_exact() {
        // Same read as every other column here: the dotted key is the key, so a
        // bare sub-key is not one (and case is not folded).
        assert_eq!(element_of_key("capacitance"), None);
        assert_eq!(
            element_of_spec_key("capacitance"),
            Some(ElementClass::Capacitive)
        );
        assert_eq!(element_of_spec_key("Capacitance"), None);
        assert_eq!(element_of_spec_key("spec.capacitance"), None);
        // A longer key is a different key: a crystal's `load_capacitance` is
        // not the bare capacitance the class is asked about.
        assert_eq!(element_of_spec_key("load_capacitance"), None);
    }

    #[test]
    fn attrkeys__vocab_column_registers_the_closed_word_sets() {
        assert_eq!(
            vocab_of(KEY_ROLE),
            Some(AttrVocab::Words(ROLE_WORDS)),
            "role takes the canon's five identity words"
        );
        assert_eq!(vocab_of(KEY_CLASS), Some(AttrVocab::Words(CLASS_WORDS)));
        assert_eq!(vocab_of(KEY_NATURE), Some(AttrVocab::Words(NATURE_WORDS)));
        assert_eq!(vocab_of(KEY_NOISE), Some(AttrVocab::Words(NOISE_WORDS)));
        assert_eq!(vocab_of(KEY_EXPOSED), Some(AttrVocab::Words(EXPOSED_WORDS)));
        assert_eq!(vocab_of(KEY_PROTECT), Some(AttrVocab::Words(PROTECT_WORDS)));
        // `bind_role` takes the `role` words: one set, not a copy of it.
        assert_eq!(vocab_of(KEY_BIND_ROLE), Some(AttrVocab::Words(ROLE_WORDS)));
        // `star` is the flag: presence is the declaration, so it registers no
        // word at all.
        assert_eq!(vocab_of(KEY_STAR), Some(AttrVocab::Flag));
    }

    #[test]
    fn attrkeys__vocab_is_absent_where_values_are_open() {
        // `@return` names a conduit — a reference, not a word from a set.
        assert_eq!(vocab_of(KEY_RETURN), None);
        // A key with a value kind but no closed set states its value freely.
        assert_eq!(vocab_of("voltage"), None);
        assert_eq!(vocab_of("spec.dielectric"), None);
        // A key the ledger does not know is not judged: no row, no word set.
        assert_eq!(vocab_of("coil_voltage"), None);
        // `role` is still the reserved word it was (N1): the column is added,
        // not the admission.
        assert!(is_reserved(KEY_ROLE));
        assert!(!is_reserved(KEY_CLASS));
        assert!(vocab_of(KEY_CLASS).is_some());
    }

    #[test]
    fn attrkeys__spec_namespace_is_derived_from_the_rows() {
        // "Is this attribute the spec table?" is answered by the registered
        // Spec-face rows, so no consumer writes the word `spec` itself.
        assert!(is_table_namespace("spec"));
        assert!(!is_table_namespace("name"));
        assert!(!is_table_namespace("voltage"));
        // A whole dotted key is a key, not the namespace it lives in.
        assert!(!is_table_namespace("spec.capacitance"));
    }
}
