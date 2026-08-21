// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! [`PortDock`] — coarse-grained bus / interface docking layer
//!
//! Net delivery is two-level (vec-dianlu.md §8.9.4):
//!
//! - **Coarse layer**: one [`PortDock`] per bus/interface dock in the source —
//!   "`uC.UART0` ↔ `J_DEBUG.UART0`". It carries the dock identity (`kind`,
//!   `iface_class`), the two `DockEnd`s (lopd / ropd sides), and the
//!   connection semantics (`op` / `dir` / `order`) of the dock itself.
//! - **Fine layer**: [`PortDock::members`] — one [`MemberLink`] per member lane,
//!   giving the pin2pin relation (`TX ↔ RX`) with the member name and the two
//!   pin ids.
//!
//! Flat [`McVecNet`]s stay untouched; each net that belongs to a dock points
//! back via a [`DockRef`] so downstream can navigate coarse → fine or fine →
//! coarse. The dock list is recursive (one `Vec<PortDock>` per graph layer,
//! following `sub_graphs`), which covers module nesting.

use std::fmt;

use crate::semantic::common::{ConnOp, IOType};

/// What kind of source object produced this dock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockKind {
    /// A component/module bus port (`SPI{CS,SCLK,MOSI,MISO}`, `[VDD,GND]`)
    Bus,
    /// A standardized interface (`UART.TTL`, `I2C`, `SPI`) bound via `::`
    Interface,
    /// A bracket list (`[A, B]`, `M[1:2]`)
    List,
    /// No coarse identity (plain scalar-ish connection with a port group)
    Plain,
}

impl DockKind {
    /// Human-readable label for displays
    pub fn label(self) -> &'static str {
        match self {
            DockKind::Bus => "bus",
            DockKind::Interface => "ifs",
            DockKind::List => "list",
            DockKind::Plain => "plain",
        }
    }
}

impl fmt::Display for DockKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// One side of a dock: the owning instance + the port name.
///
/// `instance == None` means the module-port boundary (the port is declared on
/// the module itself; its mate lives in the sub-graph or the parent layer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockEnd {
    /// Owning instance display name (last path segment, e.g. `uC`); `None` for
    /// a module port declaration.
    pub instance: Option<String>,
    /// Port name on the instance, e.g. `UART0` / `SPI` / `PDM`
    pub port: String,
    /// Standardized interface class when the port is an interface, e.g.
    /// `UART.TTL` (None for plain buses / lists)
    pub iface_class: Option<String>,
    /// Port direction / IO type when known
    pub io: Option<IOType>,
}

impl fmt::Display for DockEnd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.instance {
            Some(i) => write!(f, "{i}.{}", self.port),
            None => write!(f, "{}", self.port),
        }
    }
}

/// Fine layer: one member lane of the dock, pin2pin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberLink {
    /// Member name, e.g. `TX` / `RX` / `SCLK` / `MISO`
    pub member: String,
    /// Stable lane index (position in the left-aligned merge order)
    pub lane: u16,
    /// Pin id (InstTable entry) of the lopd side member
    pub left_pin: i64,
    /// Pin id (InstTable entry) of the ropd side member
    pub right_pin: i64,
}

impl fmt::Display for MemberLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}#{}: {left} -> {right}",
            self.member,
            self.lane,
            left = self.left_pin,
            right = self.right_pin
        )
    }
}

/// Coarse layer: one bus / interface dock between two `DockEnd`s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortDock {
    /// Stable dock id (grouping key for the member nets)
    pub id: i64,
    /// Dock display name (usually the port group name, e.g. `UART0` / `SPI`)
    pub name: String,
    /// Coarse identity of the dock
    pub kind: DockKind,
    /// Connection operator that produced the dock (`Series` for `->`, `Parallel` for `+`)
    pub op: Option<ConnOp>,
    /// lopd (left operand) side
    pub left: DockEnd,
    /// ropd (right operand) side
    pub right: DockEnd,
    /// Fine layer: per-member pin2pin links
    pub members: Vec<MemberLink>,
}

impl PortDock {
    /// Create a dock with the given coarse identity and empty member list.
    pub fn new(id: i64, name: String, kind: DockKind, left: DockEnd, right: DockEnd) -> Self {
        Self {
            id,
            name,
            kind,
            op: None,
            left,
            right,
            members: Vec::new(),
        }
    }
}

impl fmt::Display for PortDock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{kind}{op} {name}] {left} <-> {right}",
            kind = self.kind,
            op = match self.op {
                Some(ConnOp::Series) => "-",
                Some(ConnOp::Parallel) => "+",
                None => "",
            },
            name = self.name,
            left = self.left,
            right = self.right,
        )
    }
}

/// §8.9.6: structured group context of one connection lane, decided at the
/// AST layer (from the source phrase) instead of re-derived by string
/// heuristics in the render layer.
///
/// The legacy `port_group` string merged the group name and the member name
/// into one dotted string (`"SPI0.CS"`), while scalar labels (`"V3V3"`) had
/// no dot. [`PortGroupCtx`] splits that string into pure parts and carries
/// the coarse [`DockKind`], so every layer — instant → Pass2 → JSON output →
/// render — consumes the same structured connection info.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortGroupCtx {
    /// Pure group name without any member suffix, e.g. `SPI0` / `UART0` /
    /// `V3V3`. `None` for anonymous groups.
    pub name: Option<String>,
    /// Member name of this lane, e.g. `CS` / `TX`. `None` for non-member
    /// (scalar label / plain pin) connections.
    pub member: Option<String>,
    /// Coarse kind of the group (`Bus` / `Interface` / `List` / `Plain`).
    pub kind: DockKind,
}

impl PortGroupCtx {
    /// Build from the legacy combined group string plus the coarse kind.
    ///
    /// - `"SPI0.CS"` → `{ name: "SPI0", member: Some("CS"), kind }`
    /// - `"V3V3"` → `{ name: "V3V3", member: None, kind }`
    ///
    /// A dot is only treated as the name/member separator when it is not the
    /// first or last character, so plain names like `V3V3` or `1` stay whole.
    pub fn from_group_member(group: &str, kind: Option<DockKind>) -> Self {
        match group.rfind('.') {
            Some(d) if d > 0 && d + 1 < group.len() => Self {
                name: Some(group[..d].to_string()),
                member: Some(group[d + 1..].to_string()),
                kind: kind.unwrap_or(DockKind::Bus),
            },
            _ => Self {
                name: Some(group.to_string()),
                member: None,
                kind: kind.unwrap_or(DockKind::Bus),
            },
        }
    }

    /// Structured JSON form `{"name": ..., "member": ..., "kind": ...}`
    /// shared by every serialized output (graph JSON, `show` / `verify`).
    pub fn to_json_value(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "member": self.member,
            "kind": self.kind.label(),
        })
    }
}

/// Back-reference from a flat `McVecNet` to its coarse dock (if any).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DockRef {
    /// Dock id (index into `Vec<PortDock>` of the same graph layer)
    pub id: i64,
    /// Lane of this net inside the dock (member index)
    pub lane: u16,
}
