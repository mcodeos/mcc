// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use std::convert::From;

/// Which mouth of a call a synthetic funcall sentinel stands for.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) enum IoSide {
    In,
    Out,
}

impl IoSide {
    /// The word the sentinel's own spelling carries (`<base>.in` / `<base>.out`).
    pub(crate) fn word(self) -> &'static str {
        match self {
            IoSide::In => "in",
            IoSide::Out => "out",
        }
    }

    /// The default face pin this side stands for on a two-pin part.
    pub(crate) fn pin(self) -> &'static str {
        match self {
            IoSide::In => "1",
            IoSide::Out => "2",
        }
    }
}

/// The shape-level truth an error placeholder bus stands for (U313). Every
/// `<error:…>` spelling is minted by [`McBus::new_error`] with one of these
/// kinds, and consumers ask [`McBus::error_kind`] instead of sniffing the
/// name — a new spelling cannot bypass the kind.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) enum BusErrorKind {
    /// The two sides of a connect disagree on lane count.
    ShapeMismatch,
    /// A parallel group had no operands.
    EmptyParallel,
    /// A series chain had no members.
    EmptySeq,
    /// A bracketed group named nothing.
    EmptyList,
    /// A port face had no left side.
    EmptyInput,
    /// A port face had no right side.
    EmptyOutput,
}

impl BusErrorKind {
    /// The segment the bus name carries between `<error:` and `>`.
    pub(crate) fn spelling(self) -> &'static str {
        match self {
            BusErrorKind::ShapeMismatch => "shape_mismatch",
            BusErrorKind::EmptyParallel => "empty_parallel",
            BusErrorKind::EmptySeq => "empty_seq",
            BusErrorKind::EmptyList => "empty_list",
            BusErrorKind::EmptyInput => "empty_input",
            BusErrorKind::EmptyOutput => "empty_output",
        }
    }
}

/// A bus or parameterised identifier with optional member access.
///
/// # `.` (dot) and `{}` (curly braces) equivalence
///
/// In MCode, member/sub access via dot (`.`) and curly braces (`{}`) is semantically
/// equivalent:
///
/// ```text
/// DC2.VDD    ≡  DC2{VDD}      // single member access
/// res5.1     ≡  res5{1}       // single member (index)
/// rs485.A    ≡  rs485{A}      // bus member access
/// ```
///
/// Both forms resolve to the same internal representation (`McBus`). Which form is
/// used in source code is a stylistic choice; the parser normalises both to the same
/// AST and the Display/Debug output uses `{}` notation.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct McBus {
    pub(crate) name: String,
    pub(crate) member: Vec<String>,
    pub(crate) full_members: Vec<String>,
    /// The side a synthetic funcall sentinel stands for; `None` on every real
    /// bus. The producer declares it ([`McBus::synthetic_io`]) and consumers
    /// read it — nobody decodes the trailing segment (world-axioms §1 A1).
    /// Excluded from equality: it records where the bus came from, not what the
    /// bus is.
    pub(crate) synthetic: Option<IoSide>,
    /// The shape-level error this placeholder stands for; `None` on every real
    /// bus. The producer declares it ([`McBus::new_error`]) and consumers ask
    /// [`McBus::error_kind`] — the `<error:…>` spelling is never sniffed.
    /// Excluded from equality, same rule as `synthetic`.
    pub(crate) error_kind: Option<BusErrorKind>,
    /// U328: this element is a **substituted formal value** — its spelling was
    /// written at the caller's site and only happens to be re-read inside the
    /// callee body (the two-space model has no cross-space mapping table, so
    /// the value travels as a spelling). Producers are the formal→actual
    /// substitution return sites (`subst::substitute_node_element`); consumers
    /// are the callee-scope name passes (`fcallinst` prefixing / point
    /// resolution), which must not re-resolve the spelling against the callee
    /// instance's own pins — a same-spelled local pin would hijack the caller
    /// net (the pwrint ground split, U328 addendum). Excluded from equality,
    /// same rule as `synthetic`.
    pub(crate) caller_scope: bool,
}

impl PartialEq for McBus {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.member == other.member
            && self.full_members == other.full_members
    }
}

impl Eq for McBus {}

impl std::fmt::Debug for McBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.full_members.is_empty() {
            write!(f, "{}", self.name)
        } else {
            write!(f, "{}{{{}}}", self.name, self.full_members.join(", "))
        }
    }
}

impl McBus {
    pub(crate) fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            member: Vec::new(),
            full_members: Vec::new(),
            synthetic: None,
            error_kind: None,
            caller_scope: false,
        }
    }

    /// The `<base>.{in,out}` endpoint a funcall synthesizes when it has no
    /// caller: the call's own interface is unknown, so the bus is a sentinel
    /// with no identity, carrying only the side it stands for. Producers are the
    /// funcall parse sites; every consumer must drop it.
    pub(crate) fn synthetic_io(base: &str, side: IoSide) -> Self {
        Self {
            name: format!("{base}.{}", side.word()),
            member: Vec::new(),
            full_members: Vec::new(),
            synthetic: Some(side),
            error_kind: None,
            caller_scope: false,
        }
    }

    /// The `<error:…>` placeholder bus for a shape-level error. The kind is
    /// stamped here so consumers ask [`McBus::error_kind`] and never sniff the
    /// name; the name keeps the canonical spelling for display and export
    /// exclusion.
    pub(crate) fn new_error(kind: BusErrorKind) -> Self {
        Self {
            name: format!("{}{}>", Self::ERROR_PREFIX, kind.spelling()),
            member: Vec::new(),
            full_members: Vec::new(),
            synthetic: None,
            error_kind: Some(kind),
            caller_scope: false,
        }
    }

    /// The shape-level error this placeholder stands for, if it is one.
    pub(crate) fn error_kind(&self) -> Option<BusErrorKind> {
        self.error_kind
    }

    pub(crate) fn is_synthetic(&self) -> bool {
        self.synthetic.is_some()
    }

    pub(crate) fn synthetic_side(&self) -> Option<IoSide> {
        self.synthetic
    }

    /// Whether this element's spelling originates from the caller's site (a
    /// substituted formal value). The producer declares it at the substitution
    /// return site; consumers ask instead of guessing from the shape.
    pub(crate) fn is_caller_scope(&self) -> bool {
        self.caller_scope
    }

    /// Mark this element as a caller-scope spelling (builder-style).
    pub(crate) fn from_caller_scope(mut self) -> Self {
        self.caller_scope = true;
        self
    }

    pub(crate) fn new_with_members(name: &str, members: Vec<String>) -> Self {
        Self {
            name: name.to_string(),
            member: members.clone(),
            full_members: members,
            synthetic: None,
            error_kind: None,
            caller_scope: false,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Sentinel prefix used for error buses that should not appear in
    /// netlist or LSP output (Defect 89).
    pub(crate) const ERROR_PREFIX: &'static str = "<error:";

    pub(crate) fn add_member(&mut self, name: &str) {
        if !self.full_members.contains(&name.to_string()) {
            self.full_members.push(name.to_string());
        }
    }

    pub(crate) fn get_full_members(&self) -> &Vec<String> {
        &self.full_members
    }

    pub(crate) fn member_ref(base: &str, member: String) -> Self {
        Self {
            name: base.to_string(),
            member: vec![member.clone()],
            full_members: vec![member],
            synthetic: None,
            error_kind: None,
            caller_scope: false,
        }
    }

    /// Calculate node size (number of leaf nodes)
    pub(crate) fn size(&self) -> usize {
        if self.member.is_empty() {
            1
        } else {
            self.member.len()
        }
    }
}

impl From<&McBus> for Vec<McBus> {
    fn from(bus: &McBus) -> Self {
        vec![McBus {
            name: bus.name.clone(),
            member: bus.member.clone(),
            full_members: bus.full_members.clone(),
            synthetic: bus.synthetic,
            error_kind: bus.error_kind,
            caller_scope: bus.caller_scope,
        }]
    }
}

impl From<McBus> for Vec<McBus> {
    fn from(bus: McBus) -> Self {
        vec![McBus {
            name: bus.name,
            member: bus.member,
            full_members: bus.full_members,
            synthetic: bus.synthetic,
            error_kind: bus.error_kind,
            caller_scope: bus.caller_scope,
        }]
    }
}

// McList

#[derive(Clone, serde::Serialize, serde::Deserialize)]
/// A list of identifiers, e.g. `[VDD1, GND1]`.
///
/// # `.` (dot) and `{}` (curly braces) equivalence
///
/// Same equivalence as [`McBus`]: square-bracket grouped identifiers are internally
/// represented as a list, and member access via `.` or `{}` resolves to the same
/// underlying element.
pub struct McList {
    pub(crate) name: String,
    pub(crate) member: Vec<String>,
}

impl std::fmt::Debug for McList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.member.is_empty() {
            write!(f, "{}", self.name)
        } else {
            write!(f, "{}[{}]", self.name, self.member.join(", "))
        }
    }
}

impl McList {
    pub(crate) fn new_with_members(name: &str, members: Vec<String>) -> Self {
        Self {
            name: name.to_string(),
            member: members,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn add_member(&mut self, name: &str) {
        self.member.push(name.to_string());
    }
}


impl std::fmt::Display for McBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.member.is_empty() {
            write!(f, "{}", self.name)
        } else {
            let members = self.member.to_vec().join(",");
            write!(f, "{}{{{}}}", self.name, members)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sentinel records which mouth of the call it stands for, and the base
    /// it was named after. Consumers read both off the bus; the spelling is the
    /// producer's, never decoded back (world-axioms §1 A1).
    #[test]
    fn synthetic_io_declares_its_side() {
        let left = McBus::synthetic_io("uC", IoSide::In);
        assert!(left.is_synthetic());
        assert_eq!(left.synthetic_side(), Some(IoSide::In));
        assert_eq!(left.name(), "uC.in");

        let right = McBus::synthetic_io("uC", IoSide::Out);
        assert_eq!(right.synthetic_side(), Some(IoSide::Out));
        assert_eq!(right.name(), "uC.out");
    }

    /// Provenance records where a bus came from, not which net it is: a sentinel
    /// and a plain bus of the same shape are the same node. Deriving `PartialEq`
    /// over the field instead would split them.
    #[test]
    fn provenance_is_not_identity() {
        let sentinel = McBus::synthetic_io("uC", IoSide::In);
        let plain = McBus::new("uC.in");
        assert_eq!(sentinel, plain);
        assert!(!plain.is_synthetic());
    }

    /// Every conversion the funccall dispatch uses to hand left/right on must
    /// carry the provenance with it, otherwise downstream consumers see an
    /// ordinary bus.
    #[test]
    fn provenance_survives_the_hand_offs() {
        let sentinel = McBus::synthetic_io("uC", IoSide::Out);
        for carrier in [
            Vec::from(&sentinel),
            Vec::from(sentinel.clone()),
            vec![sentinel.clone()],
        ] {
            assert!(
                carrier
                    .iter()
                    .all(|b| b.synthetic_side() == Some(IoSide::Out)),
                "provenance lost in hand-off: {carrier:?}"
            );
        }
    }
}
