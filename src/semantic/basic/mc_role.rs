// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::{
    ast::{macros::*, node::AstNode},
    semantic::{component::mc_attr::McAttributes, component::mc_pins::McPins},
    McIds,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McRole {
    pub name: McIds,
    pub attrs: McAttributes,
    pub pins: McPins,
    /// Outside the serde face, same as `McInterface::body` (U392 leg A).
    #[serde(skip)]
    pub body: AstNode,
}

impl McRole {
    pub fn new(node: &AstNode) -> Option<Self> {
        // role DCE {
        //    |- MCAST_IDS (ids: DCE)  <- Note: NOT MCAST_NAME!
        //    |- MCAST_ATTRIBUTE (name = "...")
        //    |- MCAST_ATTRIBUTE (peer = DTE)
        //    |- MCAST_ATTRIBUTE_PIN (pins = [...])
        //    |- MCAST_BODY (optional)
        // }

        let subnodes = node.get_sub_node()?;

        // Get role name - from MCAST_IDS node
        let ids_node = subnodes.iter().find(|x| x.is_type(MCAST_IDS))?;
        let role_name = McIds::new(&ids_node)?;

        // Find MCAST_BODY
        let body_node_opt = subnodes.iter().find(|x| x.is_type(MCAST_BODY));

        let mut ret = Self {
            name: role_name,
            attrs: McAttributes::new(),
            pins: McPins::new(),
            body: match body_node_opt {
                Some(n) => n.clone(),
                None => node.clone(),
            },
        };

        // Parse attributes and pins — read through `clause_list` so an in-body
        // partition (`block`) is transparent here as everywhere else.
        for child in node.clause_list() {
            match child.get_type() {
                MCAST_ATTRIBUTE => {
                    ret.attrs.parse(&child);
                }
                MCAST_ATTRIBUTE_PIN | MCAST_ATTRIBUTE_PINADD => {
                    ret.pins.parse(&child);
                }
                _ => {}
            }
        }

        Some(ret)
    }

    pub fn get_attr(&self, id: &str) -> Option<&crate::semantic::component::mc_attr::McAttribute> {
        self.attrs.find(&McIds::from(id))
    }
}

impl std::fmt::Display for McRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.pins)
    }
}

/// One `peer =` value decoded (U351): the peer role's name plus the inline
/// peer cardinality the value declares, if any. `peer = MASTER` names the
/// role unrestricted; `peer = MASTER(1)` allows exactly one peer body;
/// `peer = SLAVE(1:2)` an inclusive one-to-two window. The bracket form
/// arrives as the call-shaped value the grammar already produces, and the
/// range as the ordinal `lo:hi` slice (`pins{6:9}`'s spelling), so the
/// decode reads the AST structurally — a role spelled with a non-integer
/// argument (`peer = MASTER(volt)`) yields the role with no cardinality,
/// never a misread.
#[derive(Debug, Clone)]
pub struct PeerRoleRef {
    /// The peer role's name, exactly as the value spells it.
    pub role: String,
    /// The inclusive body-count window the value declares; `None` = the
    /// value states no bound (the bare spelling, unrestricted).
    pub card: Option<(u32, u32)>,
}

/// Decode every peer role reference a `peer` attribute's values declare, in
/// written order — the single reader behind the dangling-peer sweep, the
/// mutual-peer connect judge, and the flatten-time lane carry, so the three
/// cannot disagree about what `peer = MASTER(1)` says. The set spelling
/// (`peer = [Master, Slave]`) flattens to its items, each item decoded on
/// its own.
pub fn peer_role_refs(values: &[crate::McAttrVal]) -> Vec<PeerRoleRef> {
    use crate::semantic::basic::mc_expr::McExpression;
    use crate::semantic::basic::mc_literal::strip_string_quotes;

    fn card_of(e: &McExpression) -> Option<(u32, u32)> {
        match e {
            McExpression::Int(i) if i.value >= 0 => Some((i.value as u32, i.value as u32)),
            McExpression::Slice(lo, hi) => match (lo.as_ref(), hi.as_ref()) {
                (McExpression::Int(a), McExpression::Int(b))
                    if a.value >= 0 && b.value >= 0 && a.value <= b.value =>
                {
                    Some((a.value as u32, b.value as u32))
                }
                _ => None,
            },
            _ => None,
        }
    }

    fn one(e: &McExpression, out: &mut Vec<PeerRoleRef>) {
        match e {
            McExpression::Set(items) => {
                for item in items {
                    one(item, out);
                }
            }
            McExpression::Call { name, args } if !name.is_empty() => {
                // One integer argument is the exact count `ROLE(k)`; one
                // `lo:hi` slice is the window `ROLE(lo:hi)`. Anything else in
                // the bracket is not a cardinality this decode invents a
                // reading for — the role stands, the bound stays open.
                let card = match args.as_slice() {
                    [single] => card_of(single),
                    _ => None,
                };
                out.push(PeerRoleRef {
                    role: name.clone(),
                    card,
                });
            }
            other => {
                let s = strip_string_quotes(other.to_string().trim()).trim().to_string();
                if !s.is_empty() {
                    out.push(PeerRoleRef { role: s, card: None });
                }
            }
        }
    }

    let mut out = Vec::new();
    for val in values {
        match val {
            crate::McAttrVal::AttrExpr(e) => one(e, &mut out),
            crate::McAttrVal::AttrVariable(opd, _) => {
                let s = opd.to_string().trim().to_string();
                if !s.is_empty() {
                    out.push(PeerRoleRef { role: s, card: None });
                }
            }
            crate::McAttrVal::AttrLiteral(lit) => {
                let s = strip_string_quotes(lit.to_string().trim()).trim().to_string();
                if !s.is_empty() {
                    out.push(PeerRoleRef { role: s, card: None });
                }
            }
            _ => {}
        }
    }
    out
}
