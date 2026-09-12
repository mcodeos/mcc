// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Graph entity "kind" enum collection
//!
//! - [`BoxKind`]   -- box kind (two-pin / multi-pin / sub-module / power label)
//! - [`NetKind`]   -- ★ NEW: net semantic type (power / ground / signal / bus / module IO)
//!
//! `NetKind` is used with [`super::netdef::VizNet`] so the router can automatically choose
//! different routing strategies based on "this is power / signal / bus".

use std::fmt;

// BoxKind -- box kind

/// Box kind
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoxKind {
    /// Two-pin device (R / C / L / D etc.)
    TwoPin,
    /// Multi-pin IC (>= 3 pin device)
    MultiPin,
    /// Sub-module (expandable)
    SubModule,
    /// Power / ground label
    PowerLabel,
    /// Non-power label dot / junction (e.g. `Vin`)
    Dot,
    /// ★ P7-8: boundary port terminal — a single-pin box at the canvas edge
    /// representing a port group (not individual members). Created by fromblock.rs
    /// from BoundaryInfo markers on projected nets.
    PortTerminal,
}

impl fmt::Display for BoxKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BoxKind::TwoPin => write!(f, "two_pin"),
            BoxKind::MultiPin => write!(f, "multi_pin"),
            BoxKind::SubModule => write!(f, "sub_module"),
            BoxKind::PowerLabel => write!(f, "power_label"),
            BoxKind::Dot => write!(f, "dot"),
            BoxKind::PortTerminal => write!(f, "port_terminal"),
        }
    }
}

// NetKind -- ★ NEW: net semantic type (used by VizNet)

/// Net semantic type
///
/// Filled by builder/promote phase, router chooses different drawing strategies:
///
/// | NetKind         | Recommended router    | Visual style                |
/// |-----------------|----------------------|-----------------------------|
/// | `Power`         | StarRouter           | Red thin line + power symbol|
/// | `Ground`        | StarRouter           | Blue thin line + ground sym |
/// | `Signal`        | OrthogonalRouter     | Black thin line             |
/// | `Bus(width)`    | BusBundleRouter      | Black thick line + taps     |
/// | `SubModuleIO`   | OrthogonalRouter     | Bold, with direction arrows |
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetKind {
    /// Power (VCC / VDD / V3V3 etc.)
    Power,
    /// Ground (GND / VSS etc.)
    Ground,
    /// Ordinary signal
    Signal,
    /// Bus (with width)
    Bus(usize),
    /// Sub-module external IO signal (annotated by promote phase after cross-layer promotion)
    SubModuleIO,
}

impl NetKind {
    /// Quickly guess NetKind from name (for initial classification)
    ///
    /// Only rough classification; precise classification should be done by the builder looking
    /// at the NetPoint's IOType.
    ///
    /// **P04 (S1)**: Implementation has been migrated to
    /// [`crate::vector::graph::naming::classify_net`]
    /// this method just forwards the call, keeping caller API unchanged.
    pub fn classify_by_name(name: &str) -> Self {
        super::naming::classify_net(name)
    }
}

impl fmt::Display for NetKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetKind::Power => write!(f, "power"),
            NetKind::Ground => write!(f, "ground"),
            NetKind::Signal => write!(f, "signal"),
            NetKind::Bus(n) => write!(f, "bus_{n}"),
            NetKind::SubModuleIO => write!(f, "submodule_io"),
        }
    }
}
