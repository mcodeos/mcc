// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Attribute key registry — the data dictionary for first-level attribute keys.
//!
//! An attribute is a `key = value` pair, so the set of keys that mean something
//! is a dictionary, and this module is its single in-code registration point.
//! The key space is a document tree, not a relational schema: only the *first*
//! level is registered here. Below it, values are self-describing — a bracket
//! yields a nested attribute list or a KVS record, an unbounded shape that
//! carries no schema of its own.
//!
//! Every row answers two questions, asked by different consumers:
//!
//!   * [`AttrKeyDef::general`] — may the word be used as a general attribute
//!     key at all? Words the grammar reserves in attribute position answer
//!     `false` (N1, `validation/attrs.rs`).
//!   * [`AttrKeyDef::class`] — which semantic class does a value stored under
//!     this key belong to? (D5, `doc/attribute/contract-design.md` §3.3).
//!
//! Contract: `mcd/doc/attribute/contract-design.md` §1.7 (G6, the single
//! registration point) and §3.3 (D5, the key decides value semantics). The key
//! ledger of `spec/07-attrs.md` owns this data once written; this table is the
//! code-side mirror and the seed of that ledger. Consumers query it — no
//! consumer may compare key strings itself (`if id != "spec"`, `match
//! key.to_lowercase()`), because that is how the same dictionary ends up
//! written out three times.

/// Semantic class of the value stored under a key (D5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttrKeyClass {
    /// Reserved in attribute position: the word never carries a value.
    Reserved,
    /// Nominal electrical values — the value-parameter table (`spec`).
    Nominal,
}

/// One row of the dictionary: a first-level key and its two columns.
pub(crate) struct AttrKeyDef {
    pub(crate) key: &'static str,
    /// May the key be used as a general attribute key (`key = ...`)?
    pub(crate) general: bool,
    /// Semantic class of the values stored under the key (D5).
    ///
    /// No reader yet: the D5 consumer (value semantics, and the `spec` value
    /// table behind `spec_key_to_unit`) lands with the `spec/07-attrs.md` key
    /// ledger. The column is filled now so that landing adds a reader rather
    /// than a second table.
    #[allow(dead_code)]
    pub(crate) class: AttrKeyClass,
}

/// The dictionary. Rows are added when a consumer needs them; a key with no
/// registered row is not yet known to the compiler, not silently accepted.
pub(crate) const ATTR_KEYS: &[AttrKeyDef] = &[
    // Words the grammar reserves in attribute position (N1). They name no
    // value, so they can never be general attribute keys.
    row("this", false, AttrKeyClass::Reserved),
    row("pins", false, AttrKeyClass::Reserved),
    row("role", false, AttrKeyClass::Reserved),
    row("func", false, AttrKeyClass::Reserved),
    row("return", false, AttrKeyClass::Reserved),
    row("in", false, AttrKeyClass::Reserved),
    row("out", false, AttrKeyClass::Reserved),
    row("io", false, AttrKeyClass::Reserved),
    row("psrc", false, AttrKeyClass::Reserved),
    row("psnk", false, AttrKeyClass::Reserved),
    row("psbi", false, AttrKeyClass::Reserved),
    row("anl", false, AttrKeyClass::Reserved),
    row("nc", false, AttrKeyClass::Reserved),
    row("if", false, AttrKeyClass::Reserved),
    row("else", false, AttrKeyClass::Reserved),
    // Recognized first-level attribute keys. `spec` is the nominal value
    // table (`spec.Vout = vout`, `spec = [resistance = rs]`).
    row("spec", true, AttrKeyClass::Nominal),
];

const fn row(key: &'static str, general: bool, class: AttrKeyClass) -> AttrKeyDef {
    AttrKeyDef { key, general, class }
}

/// Look up one key in the dictionary.
pub(crate) fn lookup(key: &str) -> Option<&'static AttrKeyDef> {
    ATTR_KEYS.iter().find(|d| d.key == key)
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
