//! The `@pair(group, match: …)` physical-constraint slots (U214,
//! pair-constraint-design.md v0.1, ruled 2026-09-23): mcc records the
//! requirement and checks its declaration self-consistency — the slot value
//! must be a length (E5514), both legs write it and write equal values
//! (E5515), and a slot with no group name to ride is an orphan (E5516).
//! mcc never judges the routing length itself — that gate does not exist
//! here and must not.

use crate::common;

use mcc::McIds;
use mcc::McURI;

/// Both legs write the slot with the same value: the pair declares its
/// requirement, no diagnostic is the verdict.
const WELL_FORMED: &str = r#"
interface USB3TX(role) {
    pins = [
        1 = SSTX\+ @pair(sstx, match: 0.2mm), "SuperSpeed TX Positive"
        2 = SSTX\- @pair(sstx, match: 0.2mm), "SuperSpeed TX Negative"
    ]
    role Driver { name = "Differential driver" }
}

component RCV {
    pins = [ io [1:2] = PINS[1:2] ]
}

module main {
    io tx{SSTX\+, SSTX\-}::USB3TX(Driver)
    RCV u1
    RCV u2
    tx.SSTX\+ -> u1.1
    tx.SSTX\- -> u2.1
    u1.2 -> u2.2
}
"#;

/// `0.2mm` and `200um` are the same length: the gate compares normalized
/// quantities, not spellings.
const UNIT_EQUIVALENT: &str = r#"
interface DIFF(role) {
    pins = [
        1 = P @pair(p, match: 0.2mm)
        2 = N @pair(p, match: 200um)
    ]
    role Receiver { name = "Differential receiver" }
}

component RCV {
    pins = [ io [1:2] = PINS[1:2] ]
}

module main {
    io d{P, N}::DIFF(Receiver)
    RCV u1
    RCV u2
    d.P -> u1.1
    d.N -> u2.1
    u1.2 -> u2.2
}
"#;

/// Two legs, two different tolerances: the requirement is not one
/// requirement (E5515).
const VALUE_MISMATCH: &str = r#"
interface DIFF(role) {
    pins = [
        1 = P @pair(p, match: 0.2mm)
        2 = N @pair(p, match: 0.5mm)
    ]
    role Receiver { name = "Differential receiver" }
}

component RCV {
    pins = [ io [1:2] = PINS[1:2] ]
}

module main {
    io d{P, N}::DIFF(Receiver)
    RCV u1
    RCV u2
    d.P -> u1.1
    d.N -> u2.1
    u1.2 -> u2.2
}
"#;

/// One leg writes the slot, the other does not: half a requirement is no
/// requirement (E5515).
const ONE_LEG_SLOT: &str = r#"
interface DIFF(role) {
    pins = [
        1 = P @pair(p, match: 0.2mm)
        2 = N @pair(p)
    ]
    role Receiver { name = "Differential receiver" }
}

component RCV {
    pins = [ io [1:2] = PINS[1:2] ]
}

module main {
    io d{P, N}::DIFF(Receiver)
    RCV u1
    RCV u2
    d.P -> u1.1
    d.N -> u2.1
    u1.2 -> u2.2
}
"#;

/// A voltage is not a length: the slot value has no length reading (E5514).
const NOT_LENGTH_VOLT: &str = r#"
interface DIFF(role) {
    pins = [
        1 = P @pair(p, match: 3.3V)
        2 = N @pair(p, match: 3.3V)
    ]
    role Receiver { name = "Differential receiver" }
}

component RCV {
    pins = [ io [1:2] = PINS[1:2] ]
}

module main {
    io d{P, N}::DIFF(Receiver)
    RCV u1
    RCV u2
    d.P -> u1.1
    d.N -> u2.1
    u1.2 -> u2.2
}
"#;

/// A bare number carries no unit: the shape gate wants a length quantity
/// (E5514).
const NOT_LENGTH_BARE: &str = r#"
interface DIFF(role) {
    pins = [
        1 = P @pair(p, match: 5)
        2 = N @pair(p, match: 5)
    ]
    role Receiver { name = "Differential receiver" }
}

component RCV {
    pins = [ io [1:2] = PINS[1:2] ]
}

module main {
    io d{P, N}::DIFF(Receiver)
    RCV u1
    RCV u2
    d.P -> u1.1
    d.N -> u2.1
    u1.2 -> u2.2
}
"#;

/// The slot rides no group: `@pair(match: …)` without a group name is a
/// constraint on nothing (E5516).
const ORPHAN_SLOT: &str = r#"
interface DIFF(role) {
    pins = [
        1 = P @pair(match: 0.2mm)
        2 = N @pair(match: 0.2mm)
    ]
    role Receiver { name = "Differential receiver" }
}

component RCV {
    pins = [ io [1:2] = PINS[1:2] ]
}

module main {
    io d{P, N}::DIFF(Receiver)
    RCV u1
    RCV u2
    d.P -> u1.1
    d.N -> u2.1
    u1.2 -> u2.2
}
"#;

/// Non-benign diagnostic codes of one build; the unused-param family is
/// noise here.
fn codes_of(src: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let uri: McURI = "/mcc/pair-constraint-slots.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build_flat_with_arena(&McIds::from("main"), &uri, 1);
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| d.code)
        .filter(|c| !matches!(c, 5641 | 5642 | 5643 | 5054))
        .collect();
    codes.sort_unstable();
    codes
}

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|c| **c == code).count()
}

#[test]
fn well_formed_slot_is_silent() {
    let codes = codes_of(WELL_FORMED);
    assert!(
        codes.is_empty(),
        "a well-formed match slot draws nothing, got {codes:?}"
    );
}

#[test]
fn unit_equivalent_values_agree() {
    let codes = codes_of(UNIT_EQUIVALENT);
    assert!(
        !codes.contains(&5515),
        "0.2mm and 200um are one length, got {codes:?}"
    );
}

#[test]
fn differing_values_disagree_e5515() {
    let codes = codes_of(VALUE_MISMATCH);
    assert!(
        codes.contains(&5515),
        "0.2mm vs 0.5mm is a disagreement, got {codes:?}"
    );
}

#[test]
fn slot_on_one_leg_only_disagrees_e5515() {
    let codes = codes_of(ONE_LEG_SLOT);
    assert_eq!(
        count(&codes, 5515),
        1,
        "one leg wrote the slot, one did not: exactly one 5515, got {codes:?}"
    );
}

#[test]
fn voltage_slot_is_not_a_length_e5514() {
    let codes = codes_of(NOT_LENGTH_VOLT);
    assert_eq!(
        count(&codes, 5514),
        2,
        "both legs spell the same shape error, got {codes:?}"
    );
    assert!(
        !codes.contains(&5515),
        "an ill-formed slot is reported once, not again as a mismatch, got {codes:?}"
    );
}

#[test]
fn bare_number_slot_is_not_a_length_e5514() {
    let codes = codes_of(NOT_LENGTH_BARE);
    assert_eq!(
        count(&codes, 5514),
        2,
        "a unitless number has no length reading, got {codes:?}"
    );
}

#[test]
fn slot_without_group_is_an_orphan_e5516() {
    let codes = codes_of(ORPHAN_SLOT);
    assert_eq!(
        count(&codes, 5516),
        2,
        "each leg's group-less slot is an orphan, got {codes:?}"
    );
}
