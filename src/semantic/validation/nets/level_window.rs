// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U358 — the level-window compatibility gate
//! (level-window-compat-design.md §3): when one net joins a driving pin and a
//! receiving pin whose `voltage:[low:…, high:…]` rows share a level key, the
//! driver's band for that key must sit inside the receiver's band.
//!
//! Why definition space and not a flatten carry: the windows live on the
//! declaring rows (a role member's row for an adopted pin, the pin's own row
//! otherwise), and E4105 already reads its voltages the same way — a sweep
//! that decodes per entry keeps the flatten pass untouched. The trigger is
//! the declared value, never a family, role, or net name.
//!
//! The bands compare as written, per level key: `low`/`high` name logic
//! levels, not voltage ordering, so an inverted differential leg
//! (`high:-6V ~ -2V`) is an ordinary band and containment never normalizes
//! the endpoints. A side either side declares but whose endpoints do not
//! resolve to volt scalars (`0.3*VCC`, a symbol endpoint) is unknown, and an
//! unknown pair stays silent (unknown-stays-silent, same law as E6062).

use super::{best_pos, NetCheckResult};
use crate::instant::insttab::{InstEntry, InstTable, IfaceLane};
use crate::semantic::basic::mc_expr::McExpression;
use crate::semantic::basic::mc_kvs::{KVSValue, McKVS};
use crate::semantic::basic::mc_literal::McLiteral;
use crate::semantic::basic::mc_uval::{McUnit, McUnitValue};
use crate::semantic::basic::attr_keys::{is_voltage_key, AttrFace};
use crate::semantic::common::IOType;
use crate::semantic::component::mc_attr::McAttrVal;
use crate::semantic::component::mc_pins::McPinPort;
use crate::semantic::component::McComponent;

/// One level's declared band (`low: A ~ B`), endpoints in volts in the order
/// the row writes them. In a normal band `lo <= hi`; an inverted differential
/// leg may write the key's band in reverse — containment compares the fields
/// as declared, never sorted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LevelBand {
    pub lo: f64,
    pub hi: f64,
}

/// The level bands one pin row declares. A missing side is an open
/// declaration (one-sided windows parse), not a zero.
#[derive(Debug, Clone, Default)]
pub(crate) struct LevelWindows {
    pub low: Option<LevelBand>,
    pub high: Option<LevelBand>,
}

impl LevelWindows {
    fn has_any(&self) -> bool {
        self.low.is_some() || self.high.is_some()
    }
}

/// Volts of one band endpoint: a plain volt unit literal. An expression
/// (`0.3*VCC`), a symbol, a range, or a `±` form has no scalar here.
fn unit_volts(expr: &McExpression) -> Option<f64> {
    let McExpression::UnitValue(uv) = expr else {
        return None;
    };
    scalar_volts(uv)
}

fn scalar_volts(uv: &McUnitValue) -> Option<f64> {
    if matches!(uv.unit(), McUnit::Volt) && !uv.is_range_or_plusminus() {
        Some(uv.value())
    } else {
        None
    }
}

/// One `low:`/`high:` side value: a `A ~ B` band or a bare scalar point.
/// Anything else (a symbol endpoint, `0.3*VCC`, `±`) is unknown.
fn side_band(vals: &[McAttrVal]) -> Option<LevelBand> {
    match vals {
        [McAttrVal::AttrExpr(McExpression::Range(a, b))] => {
            Some(LevelBand { lo: unit_volts(a)?, hi: unit_volts(b)? })
        }
        [McAttrVal::AttrExpr(McExpression::UnitValue(uv))] => {
            let v = scalar_volts(uv)?;
            Some(LevelBand { lo: v, hi: v })
        }
        [McAttrVal::AttrLiteral(McLiteral::Uval(uv))] => {
            let v = scalar_volts(uv)?;
            Some(LevelBand { lo: v, hi: v })
        }
        _ => None,
    }
}

/// The level windows a pin row declares. Only the keys the grammar writes
/// today (`low`/`high`) carry bands; a side the row wrote but that does not
/// decode to volt scalars leaves its slot `None`.
pub(crate) fn pin_level_windows(pin: &crate::semantic::component::mc_pins::McPin) -> LevelWindows {
    let mut out = LevelWindows::default();
    for kvs in crate::semantic::component::mc_pins::pin_kvs_where(pin, |key| {
        is_voltage_key(key, AttrFace::PinRow)
    }) {
        let KVSValue::Square(members) = &kvs.value else {
            continue;
        };
        for m in members {
            let McAttrVal::KVS(McKVS { key, value }) = m else {
                continue;
            };
            let slot = match key.to_string().as_str() {
                "low" => &mut out.low,
                "high" => &mut out.high,
                _ => continue,
            };
            if slot.is_some() {
                continue;
            }
            let KVSValue::Square(vals) = value else {
                continue;
            };
            *slot = side_band(vals);
        }
    }
    out
}

/// The windows an interface-adopted pin inherits from its role member's row:
/// the entry's pin id resolves through `PORT.MEMBER` to the member name, the
/// member names into the role's pin table. A roleless adoption or a member
/// that names no role pin declares nothing here.
fn adopted_windows(
    def: &McComponent,
    pin_id: &str,
    lane: &IfaceLane,
) -> Option<LevelWindows> {
    let role_name = lane.role.as_ref()?;
    let names = def.pins.pin_id_to_names.get(pin_id)?;
    let mut segs = names.first()?.split('.');
    let port_name = segs.next()?;
    if port_name.is_empty() {
        return None;
    }
    let member = segs.next()?;
    let port = def.pins.names_to_id.get(port_name)?;
    let McPinPort::Interface(iface) = port else {
        return None;
    };
    let role = iface
        .base
        .roles
        .iter()
        .find(|r| &r.name.to_string() == role_name)?;
    let rp = role
        .pins
        .pins
        .values()
        .find(|p| p.names.iter().any(|n| n == member))?;
    let w = pin_level_windows(rp);
    w.has_any().then_some(w)
}

/// Whether an interface-adopted pin's role member row carries the active-low
/// flag (§2.8): the entry's pin id resolves through `PORT.MEMBER` to the
/// member name, the member names into the role's pin table. A roleless
/// adoption or a member that names no role pin answers nothing.
fn adopted_active_low(def: &McComponent, pin_id: &str, lane: &IfaceLane) -> Option<bool> {
    let role_name = lane.role.as_ref()?;
    let names = def.pins.pin_id_to_names.get(pin_id)?;
    let mut segs = names.first()?.split('.');
    let port_name = segs.next()?;
    if port_name.is_empty() {
        return None;
    }
    let member = segs.next()?;
    let port = def.pins.names_to_id.get(port_name)?;
    let McPinPort::Interface(iface) = port else {
        return None;
    };
    let role = iface
        .base
        .roles
        .iter()
        .find(|r| &r.name.to_string() == role_name)?;
    let rp = role
        .pins
        .pins
        .values()
        .find(|p| p.names.iter().any(|n| n == member))?;
    Some(rp.active_low)
}

/// Whether a flat entry's pin is active-low (§2.8), resolved in definition
/// space with the same walk as [`entry_level_windows`]: an adopted pin reads
/// its role member's row, a plain pin its own row. A pin that resolves to no
/// row is simply not active-low — the flag shapes wording only, it never
/// opens a question the row did not declare.
pub(crate) fn entry_active_low(table: &InstTable, entry: &InstEntry) -> bool {
    let Some(comp_entry) = entry.parent_id.and_then(|pid| table.get_entry(pid)) else {
        return false;
    };
    if comp_entry.class_name.is_empty() {
        return false;
    }
    let comps = crate::definition_space().workspace_components();
    let Some(def) = comps
        .iter()
        .find(|(sn, _)| sn.ident.to_string() == comp_entry.class_name)
        .map(|(_, c)| c)
    else {
        return false;
    };
    let pin_id = entry.path.rsplit('.').next().unwrap_or("");
    if let Some(lane) = entry.iface_lane.as_ref() {
        if let Some(al) = adopted_active_low(def, pin_id, lane) {
            return al;
        }
    }
    def.pins
        .pins
        .get(pin_id)
        .map(|p| p.active_low)
        .unwrap_or(false)
}

/// The windows a flat entry declares, resolved in definition space: an
/// adopted pin reads its role member's row, a plain pin its own row.
/// Component pins only (E4105's guard) — module-port members resolve through
/// a different def walk and are not judged yet.
pub(crate) fn entry_level_windows(table: &InstTable, entry: &InstEntry) -> Option<LevelWindows> {
    let comp_entry = entry.parent_id.and_then(|pid| table.get_entry(pid))?;
    if comp_entry.class_name.is_empty() {
        return None;
    }
    let comps = crate::definition_space().workspace_components();
    let def = comps
        .iter()
        .find(|(sn, _)| sn.ident.to_string() == comp_entry.class_name)
        .map(|(_, c)| c)?;
    let pin_id = entry.path.rsplit('.').next().unwrap_or("");
    if let Some(lane) = entry.iface_lane.as_ref() {
        if let Some(w) = adopted_windows(def, pin_id, lane) {
            return Some(w);
        }
    }
    let pin = def
        .pins
        .pins
        .get(pin_id)
        .or_else(|| {
            def.pins
                .pins
                .values()
                .find(|p| p.names.iter().any(|n| n == &entry.class_name))
        })?;
    let w = pin_level_windows(pin);
    w.has_any().then_some(w)
}

/// One band as the row wrote it: `1.2V ~ 1.8V`, a point as `0.8V`.
fn band_text(b: LevelBand) -> String {
    if (b.lo - b.hi).abs() < 1e-9 {
        format!("{}V", b.lo)
    } else {
        format!("{}V ~ {}V", b.lo, b.hi)
    }
}

pub(crate) fn check_level_window_mismatch(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    const EPS: f64 = 1e-9;
    for net in table.get_nets() {
        // (path, direction, declared windows, active-low) for signal pins on
        // this net
        let mut ends: Vec<(String, IOType, LevelWindows, bool)> = Vec::new();
        for &pid in &net.points {
            let Some(entry) = table.get_entry(pid) else {
                continue;
            };
            if !matches!(entry.kind, crate::instant::insttab::InstKind::Pin) {
                continue;
            }
            if !matches!(entry.io_type, IOType::Out | IOType::In) {
                continue;
            }
            let Some(w) = entry_level_windows(table, entry) else {
                continue;
            };
            ends.push((
                entry.path.clone(),
                entry.io_type.clone(),
                w,
                entry_active_low(table, entry),
            ));
        }
        for (dp, d_io, d_w, d_al) in &ends {
            if *d_io != IOType::Out {
                continue;
            }
            for (rp, r_io, r_w, r_al) in &ends {
                if *r_io != IOType::In || rp == dp {
                    continue;
                }
                for (key, db, rb) in [
                    ("low", d_w.low, r_w.low),
                    ("high", d_w.high, r_w.high),
                ] {
                    let (Some(db), Some(rb)) = (db, rb) else {
                        continue;
                    };
                    if rb.lo - EPS <= db.lo && db.hi <= rb.hi + EPS {
                        continue;
                    }
                    // U365 polarity arm: an active-low receiver reads its low
                    // band as the asserted level, so the same mismatch means
                    // "the signal cannot assert" there. The comparison law is
                    // polarity-independent (bands judge as written); the flag
                    // only shapes the wording. Severity modulation (asserted
                    // vs inactive level) waits for logic-state semantics.
                    let polarity = if *r_al {
                        format!(" '{rp}' is active-low: its low band is the asserted level.")
                    } else if *d_al {
                        format!(" '{dp}' is active-low: its low band is the asserted level.")
                    } else {
                        String::new()
                    };
                    let (pos, uri) = best_pos(table, &net.points);
                    results.push(NetCheckResult {
                        check: "level-window-mismatch",
                        severity: "error",
                        message: format!(
                            "Net '{}': '{}' drives {} {} outside '{}' accepted {} band {}.{}",
                            net.name,
                            dp,
                            key,
                            band_text(db),
                            rp,
                            key,
                            band_text(rb),
                            polarity
                        ),
                        net_name: net.name.clone(),
                        code: crate::errcodes::LEVEL_WINDOW_MISMATCH,
                        pos,
                        uri,
                    });
                    break;
                }
            }
        }
    }
}
