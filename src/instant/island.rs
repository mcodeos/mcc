// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Net-island attribution index — flat net → owning-scope identity (L1).
//!
//! Design: `mcd/doc/net-connect/net-island-attribution-design.md` §2/§3/§5/§7.
//! Built once after flatten, this is the reverse-query face for "which module
//! owns this flat net, and which declared identity does its *name* anchor to".
//! It replaces the historical global-name reverse-lookup shape (hot-name last
//! segment guess + cross-scope `remove` on collision) with a **per-scope**
//! resolution: every flat `NetEntry` now records its owning module entry id
//! ([`NetEntry::module`]), and that module's own def declarations (conduits +
//! domain rails) resolve the net name — no global merge, no cross-scope
//! ambiguity.
//!
//! ## L1 scope — what this index does *not* claim
//!
//! L1 anchors identity **only on declarations in the owning scope**:
//!
//! - a net whose name equals an owning-scope `conduit` name → that copper;
//! - a net whose name equals an owning-scope declared rail `hot`/`ret` member
//!   → that supply face (`Hot`/`Ret`) and its declaring domain(s);
//! - a named copper that no rail returns to (`EARTH`, `ESDGND`) → `Reference`;
//! - everything else — derived supply faces (`VMAIN_5V`), dotted pass-through
//!   members (`vin.GND`), bare undeclared grounds, signal and anonymous wires
//!   — is left `resolvable = false`. No name heuristic guesses past the owning
//!   scope (the reference-binding cross-layer merge is the deferred L3 step),
//!   and no existing check consumes this index yet (golden residuals stay
//!   verbatim). (Per-statement `@N` ground fragments were retired with the
//!   flat ground partition — split-ground-copper-design v0.2 §6 — so no flat
//!   net carries an `@owner` electric-fragment suffix.)
//!
//! Role/copper carry the conduit's supply *function*, not its `@role` tag —
//! the tag (`main`/`quiet`/`isolated`/`earth`/`protective`) is the copper's
//! declared world role and stays a declaration-layer fact the ERC role checks
//! consume directly.

use crate::instant::insttab::{InstTable, NetEntry};
use std::collections::BTreeMap;

/// Supply function of a flat net on its owning copper (net-island-attribution
/// §2 `role` column, made concrete at L1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetRole {
    /// Hot member of at least one owning-scope declared DC rail.
    Hot,
    /// Return member of at least one owning-scope declared DC rail — the
    /// rail's `ret` names a return copper (usually a declared conduit).
    Ret,
    /// Named copper (`conduit`) that no owning-scope rail returns to
    /// (`EARTH`, a protective `ESDGND`, a quiet reference without a rail …).
    Reference,
    /// No owning-scope identity anchor — signal wire, anonymous `_net{N}`,
    /// derived supply face, dotted pass-through member.
    /// `resolvable` is `false` and consumers must not judge it.
    Signal,
}

impl NetRole {
    pub fn as_str(self) -> &'static str {
        match self {
            NetRole::Hot => "Hot",
            NetRole::Ret => "Ret",
            NetRole::Reference => "Reference",
            NetRole::Signal => "Signal",
        }
    }
}

/// Per-net attribution record (net-island-attribution §2). One per flat net,
/// keyed by net id.
#[derive(Debug, Clone)]
pub struct NetAttribution {
    /// Flat net id (`NetEntry::id`).
    pub net_id: u32,
    /// Owning module entry id (`NetEntry::module`). Resolve to its `path` /
    /// def via `InstTable::get_entry`.
    pub module: Option<u32>,
    /// Display name (split marker / dotted form kept verbatim — display only).
    pub name: String,
    /// Owning-scope `conduit` identity this net anchors to, when its name
    /// equals a declared conduit name. `None` = no copper anchor (a rail hot
    /// face such as `VDD_3V3` is on copper, but not a *declared* conduit).
    pub copper: Option<String>,
    /// Supply function on that copper (§L1 note above).
    pub role: NetRole,
    /// Owning-scope domain names whose declared DC rail anchors this net
    /// (a return copper may serve several domains, e.g. `GND` returns DVDD and
    /// DCORE). Empty for `Reference`/`Signal`.
    pub worlds: Vec<String>,
    /// True when identity is provable from owning-scope declarations (role is
    /// `Hot`/`Ret`/`Reference`). False → consumers skip (design §8: never
    /// judge or guess an unresolvable net).
    pub resolvable: bool,
}

/// L1 island index: net id → [`NetAttribution`], built once from a flat
/// [`InstTable`]. Read-only; no rule consumes it yet.
#[derive(Debug, Default)]
pub struct NetIslandIndex {
    by_id: BTreeMap<u32, NetAttribution>,
}

impl NetIslandIndex {
    /// Build the index over every flat net of `table`. Each net resolves its
    /// owning module's own declaration set — the map is per-instance scope, so
    /// two instances of one def never collide the way the old global hot-name
    /// lookup did.
    pub fn build(table: &InstTable) -> Self {
        let mut idx = NetIslandIndex::default();
        for net in table.get_nets() {
            idx.by_id.insert(net.id, attribute(table, net));
        }
        idx
    }

    /// Attribution of one flat net.
    pub fn get(&self, net_id: u32) -> Option<&NetAttribution> {
        self.by_id.get(&net_id)
    }

    /// Every attribution, ascending by net id.
    pub fn nets(&self) -> impl Iterator<Item = &NetAttribution> {
        self.by_id.values()
    }

    /// Attributions whose nets belong to `module_id`, ascending by net id.
    pub fn by_module(&self, module_id: u32) -> Vec<&NetAttribution> {
        self.by_id
            .values()
            .filter(|a| a.module == Some(module_id))
            .collect()
    }
}

/// Resolve one flat net against its owning module's declaration set (L1 scope
/// — declaration anchors only; see module docs).
fn attribute(table: &InstTable, net: &NetEntry) -> NetAttribution {
    let module = net.module;
    let decls = module.and_then(|m| table.power_decls().get(&m));

    let refs = decls.map(|d| d.l1_refs()).unwrap_or_default();
    let rails = decls.map(|d| d.l1_rails()).unwrap_or_default();

    let name = net.name.clone();
    // Copper anchor: owning-scope conduit whose bare name equals the net name.
    let copper = refs.iter().find(|r| r.name == name).map(|r| r.name.clone());

    // Declaring domains by rail membership (hot member / ret member). A ret
    // member names a return copper; the net whose name equals it is that
    // copper's flat net (GND serves DVDD + DCORE).
    let mut worlds: Vec<String> = Vec::new();
    let mut is_hot = false;
    let mut is_ret = false;
    for r in &rails {
        if r.hot == name {
            is_hot = true;
            if !worlds.contains(&r.domain) {
                worlds.push(r.domain.clone());
            }
        } else if r.ret == name {
            is_ret = true;
            if !worlds.contains(&r.domain) {
                worlds.push(r.domain.clone());
            }
        }
    }

    let (role, resolvable) = if is_hot {
        (NetRole::Hot, true)
    } else if is_ret {
        (NetRole::Ret, true)
    } else if copper.is_some() {
        (NetRole::Reference, true)
    } else {
        (NetRole::Signal, false)
    };

    NetAttribution {
        net_id: net.id,
        module,
        name,
        copper,
        role,
        worlds,
        resolvable,
    }
}
