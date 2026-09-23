// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Hardware-specific validation checks.
//!
//! Checks:
//!   HW1 — Declared power pin without a voltage/power declaration
//!   HW2 — Pin ID gaps in component pin definitions
//!   HW3 — Pin count extremes (too many or too few)
//!   HW5 — Interface role with dangling peer reference
//!   HW6 — Component with only single-type IO pins (all inputs, all outputs)
//!   HW9/HW10 — Interface role peer not mutual (E5508) / peer width mismatch (E5509)
//!   HW11/HW12 — @pair group without two legs (E5512) / retired diff_pair key (E5513)

use super::{CheckAccumulator, CheckPhase, CheckResult, CheckSeverity, ValidationCheck};
use crate::semantic::pwrid::{self, DeclaredFaces, Face};
use std::collections::HashSet;

pub struct HwCheck;

impl ValidationCheck for HwCheck {
    fn name(&self) -> &'static str {
        "hw"
    }
    fn phase(&self) -> CheckPhase {
        CheckPhase::PostParse
    }
    fn default_severity(&self) -> CheckSeverity {
        CheckSeverity::Warning
    }

    fn run_post_parse(&self, acc: &mut CheckAccumulator) {
        check_power_pin_no_voltage(acc); // HW1
        check_pin_id_gaps(acc); // HW2
        check_pin_count_extremes(acc); // HW3
        check_role_peer_dangling(acc); // HW5
        check_single_ioc_type_component(acc); // HW6
        check_func_param_pin_shadow(acc); // HW8
        check_role_peer_mutual_and_width(acc); // HW9/HW10
        check_iface_pair_groups(acc); // HW11/HW12
    }
}

// HW1: Power pin without voltage/power attributes

fn check_power_pin_no_voltage(acc: &mut CheckAccumulator) {
    let comps = crate::definition_space().workspace_components();
    for (sn, comp) in comps.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }

        // Unified power-pin collection: a pin is a power-face pin when its own
        // **declaration** says so — a `psrc/psnk/psbi … ::DC(…)` row that names
        // it, or its own direction word (`IOType::Power`). The former test also
        // accepted a list of spellings (`VCC`, `VDD`, `GND`, `VSS`, `VREF`, …),
        // so the hint fired on what the author happened to call the pin
        // (world-axioms §1 A1). The display name is the spelling the declaration
        // wrote, else the pin's first name.
        let declared = DeclaredFaces::of_pins(&comp.pins);
        let power_pins: Vec<(String, String)> = comp
            .pins
            .pins
            .iter()
            .filter(|(_, pin)| {
                matches!(pin.iotype, crate::IOType::Power)
                    || pin.names.iter().any(|n| declared.declares(n))
            })
            .map(|(pin_id, pin)| {
                let name = pin
                    .names
                    .iter()
                    .find(|n| declared.declares(n))
                    .or_else(|| pin.names.first())
                    .cloned()
                    .unwrap_or_else(|| pin_id.clone());
                (pin_id.clone(), name)
            })
            .collect();

        if power_pins.is_empty() {
            continue;
        }

        // GND-only passives: a component whose power faces are all **returns**
        // has no supply rail to document, so the voltage-attribute hint does not
        // apply (e.g. passive mics/speakers). The former test compared the pin's
        // display name against a ground word list (`GROUND_PIN_NAMES`).
        let has_supply_pin = comp.pins.pins.values().any(|pin| {
            let names: Vec<&str> = pin.names.iter().map(|n| n.as_str()).collect();
            match pwrid::member_of_names(&comp.pins, &names) {
                Some(m) => m.face == Face::Hot,
                // No `::DC` row names this pin: its own direction word is the
                // only declaration there is, and a `psrc/psnk/psbi` row is a
                // power terminal, not a return.
                None => matches!(pin.iotype, crate::IOType::Power),
            }
        });
        if !has_supply_pin {
            continue;
        }

        // Check if component has voltage-related attributes
        let has_voltage_attr = comp.attrs.iter().any(|a| {
            crate::semantic::basic::attr_keys::is_voltage_key(
                &a.id.to_string(),
                crate::semantic::basic::attr_keys::AttrFace::Body,
            )
        });

        // Check if the component declares a voltage-typed parameter
        // (`volt::UV.VOLT`). A parameter name is not an attribute key, so the
        // registry has nothing to say here: the declared type is the evidence.
        let has_voltage_param = comp.params.iter().any(|d| {
            matches!(
                d.param_type.kind,
                crate::semantic::basic::mc_param_type::McParamTypeKind::UnitValue {
                    unit: crate::semantic::basic::mc_uval::McUnit::Volt
                } | crate::semantic::basic::mc_param_type::McParamTypeKind::UnitValueDefault {
                    unit: crate::semantic::basic::mc_uval::McUnit::Volt,
                    ..
                }
            )
        });

        // Check if any interface binding provides voltage info. The evidence is
        // a **Volt-typed** argument in the binding (`[VDD, GND]::DC(3.3V)`),
        // never the binding's name: the old test also matched
        // `contains("dc")` / `contains("power")` / `contains("supply")` on the
        // interface spelling (lower-cased), so a class merely *called* `DC`
        // satisfied the hint with no voltage written anywhere (world-axioms §1
        // A1). `has_pwr_contract` below already covers the `pins.pwr` spelling
        // of the same fact.
        let has_voltage_iface = comp.pins.names_to_id.values().any(|port| {
            if let crate::semantic::component::mc_pins::McPinPort::Interface(ref iface) = port {
                return iface.params.iter().any(|p| {
                    matches!(
                        p,
                        crate::semantic::basic::mc_param::McParamValue::UValue(uv)
                            if matches!(uv.unit(), crate::semantic::basic::mc_uval::McUnit::Volt)
                    )
                });
            }
            false
        });

        // §4.1/§5.2 power-intent: a `psrc/psnk/psbi` row's trailing `::DC(...)`
        // declare is the modern way to document a supply pin's nominal — it is
        // captured into `pins.pwr` (mc_pins), so a component carrying such a
        // contract needs no legacy `voltage` attribute. (Same coarse axis as
        // `has_voltage_iface`: any contract anywhere satisfies the hint.)
        let has_pwr_contract = !comp.pins.pwr.is_empty();

        if !has_voltage_attr && !has_voltage_param && !has_voltage_iface && !has_pwr_contract {
            // One Info per supply pin, anchored on the pin's own name span: the
            // suggested fix (a `voltage` attribute) belongs on the supply pin,
            // so the marker lives there, not on the component name. A pin that
            // carries its own inline voltage (e.g. `volt:1.2V`) is already
            // documented and is skipped individually.
            for (pin_id, name) in &power_pins {
                let pin_has_voltage = comp.pins.pins.get(pin_id).is_some_and(|pin| {
                    crate::semantic::component::mc_pins::pin_kvs_where(pin, |key| {
                        crate::semantic::basic::attr_keys::is_voltage_key(
                            key,
                            crate::semantic::basic::attr_keys::AttrFace::PinRow,
                        )
                    })
                    .next()
                    .is_some()
                });
                if pin_has_voltage {
                    continue;
                }
                // Anchor on the individual pin ID first: alias-style pin lists
                // (`A4 = VBUS, "VBUS"`) share one name span per name, but the
                // pin-id span (`pin_id_spans` keyed by "A4") is unique per pin,
                // so four VBUS pins each land on their own declaration line.
                let span = comp
                    .pins
                    .pin_id_spans
                    .get(pin_id)
                    .or_else(|| comp.pins.pin_name_spans.get(pin_id))
                    .or_else(|| comp.pins.pin_name_spans.get(name))
                    .cloned()
                    .filter(|s| s.end > s.start)
                    .unwrap_or_else(|| comp.span.start..comp.span.end);
                acc.push(CheckResult {
                    check_name: "hw",
                    severity: CheckSeverity::Info,
                    uri: Some(uri.clone()),
                    span: Some(span),
                    message: format!(
                        "Component '{}': power pin '{}' ({}) has no associated \
                         voltage attribute. Consider adding e.g. `voltage = \"5V\"`.",
                        comp.name, name, pin_id
                    ),
                    code: crate::errcodes::POWER_PIN_NO_VOLTAGE,
                });
            }
        }
    }
}

// HW2: Pin ID gaps in component pin definitions

/// Components with non-sequential pin IDs (e.g., pins 1,2,3,5,6 — missing 4)
/// may indicate accidentally skipped pins or copy-paste errors. This is common
/// for NC (not-connected) pins but worth flagging for review.
fn check_pin_id_gaps(acc: &mut CheckAccumulator) {
    let comps = crate::definition_space().workspace_components();
    for (sn, comp) in comps.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }

        // Collect all numeric pin IDs
        let mut pin_ids: Vec<u32> = Vec::new();
        for pin_id in comp.pins.pins.keys() {
            if let Ok(num) = pin_id.parse::<u32>() {
                pin_ids.push(num);
            }
        }

        if pin_ids.len() < 3 {
            continue; // Too few pins for meaningful gap analysis
        }

        pin_ids.sort_unstable();

        // Find gaps
        let mut gaps: Vec<u32> = Vec::new();
        for window in pin_ids.windows(2) {
            let curr = window[0];
            let next = window[1];
            if next > curr + 1 {
                for missing in (curr + 1)..next {
                    gaps.push(missing);
                }
            }
        }

        // Only report if there are a reasonable number of gaps
        // (1-2 gaps in a large component is normal for NC pins)
        let total_pins = pin_ids.len();
        let gap_count = gaps.len();

        if gap_count > 0 && (gap_count as f64 / total_pins as f64) > 0.05 {
            let gap_list: Vec<String> = gaps.iter().take(10).map(|g| g.to_string()).collect();
            let suffix = if gaps.len() > 10 {
                format!(" ... and {} more", gaps.len() - 10)
            } else {
                String::new()
            };

            acc.push(CheckResult {
                check_name: "hw",
                severity: CheckSeverity::Info,
                uri: Some(uri.clone()),
                span: Some(comp.span.start..comp.span.end),
                message: format!(
                    "Component '{}' has {} pin ID gap(s) ({} of {} pins): {}{}. \
                     These may be intentional NC pins or could indicate missing definitions.",
                    comp.name,
                    gap_count,
                    gap_count,
                    total_pins,
                    gap_list.join(", "),
                    suffix
                ),
                code: crate::errcodes::HW_PIN_NUMBER_GAP,
            });
        }
    }
}

// HW3: Pin count extremes

/// Components with unusually many pins (>300) or zero pins (not abstract)
/// deserve a second look. Extremely high pin counts may indicate a data error;
/// zero-pin components should probably be abstract or use an interface instead.
fn check_pin_count_extremes(acc: &mut CheckAccumulator) {
    let comps = crate::definition_space().workspace_components();
    for (sn, comp) in comps.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }

        let pin_count = comp.pins.pins.len();

        // HW3a: Too many pins (likely a large BGA or data error)
        if pin_count > 300 {
            acc.push(CheckResult {
                check_name: "hw",
                severity: CheckSeverity::Info,
                uri: Some(uri.clone()),
                span: Some(comp.span.start..comp.span.end),
                message: format!(
                    "Component '{}' has {} pins. Verify this is correct — \
                     high pin counts may indicate a data entry error.",
                    comp.name, pin_count
                ),
                code: crate::errcodes::HW_PIN_COUNT_HIGH,
            });
        }

        // HW3b: Zero pins but not abstract (has params or attrs suggesting it should have pins)
        // Skip components with any pin definitions — dynamic pin definitions
        // (§2.20) and conditional pin blocks (cond_pins) are resolved at
        // instantiation time, so the template may legitimately have 0 static pins.
        if pin_count == 0
            && !comp.has_pin_defs()
            && !comp.params.is_empty()
            && !comp.attrs.is_empty()
            && comp.funcs.is_empty()
        {
            acc.push(CheckResult {
                check_name: "hw",
                severity: CheckSeverity::Warning,
                uri: Some(uri.clone()),
                span: Some(comp.span.start..comp.span.end),
                message: format!(
                    "Component '{}' has 0 pins but has params and attributes. \
                     Is this an abstract component? Consider adding a pin definition \
                     or marking it as abstract.",
                    comp.name
                ),
                code: crate::errcodes::HW_ZERO_PINS_WITH_PARAMS,
            });
        }
    }
}

// HW5: Interface role with dangling peer reference

/// An interface role that specifies a `peer` relationship should have a
/// corresponding peer role defined in the same interface. A dangling peer
/// reference indicates an incomplete interface definition.
fn check_role_peer_dangling(acc: &mut CheckAccumulator) {
    // D10 (interface-connect-rule-design.md section 6, ruled 2026-09-20): the
    // definition-side self-checks scan the unified view — workspace plus the
    // loaded system library. A library-side disease is exactly the
    // "fix once, every project benefits" kind, so it must surface in every
    // project build that loads the library, not only when the library file
    // is fed as workspace input.
    let ifaces = crate::definition_space().all_interfaces();
    for (sn, iface) in ifaces.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }

        // Collect all role names in this interface
        let role_names: HashSet<String> = iface.roles.iter().map(|r| r.name.to_string()).collect();

        for role in &iface.roles {
            // Check if role has a peer attr referencing another role
            for attr in &role.attrs {
                let key = attr.id.to_string();
                if key == "peer" {
                    for peer_name in peer_role_names(&attr.values) {
                        if !role_names.contains(&peer_name) {
                            acc.push(CheckResult {
                                check_name: "hw",
                                severity: CheckSeverity::Warning,
                                uri: Some(uri.clone()),
                                span: attr.key_span.clone(),
                                message: format!(
                                    "Interface '{}': role '{}' references peer '{}' \
                                     which is not defined in this interface. \
                                     Available roles: {}",
                                    iface.name,
                                    role.name,
                                    peer_name,
                                    if role_names.is_empty() {
                                        "(none)".to_string()
                                    } else {
                                        role_names
                                            .iter()
                                            .map(|s| s.as_str())
                                            .collect::<Vec<_>>()
                                            .join(", ")
                                    }
                                ),
                                code: crate::errcodes::HW_IFACE_PEER_DANGLING,
                            });
                        }
                    }
                }
            }
        }
    }
}

/// HW9/HW10: definition-side peer self-check (interface-connect-rule-design.md
/// §3.3 A/B — the interface definition itself is diseased, so it is reported
/// where it is declared, not where it is connected):
///
///   * peer pairs must be **mutual** (E5508): a role names a peer that never
///     names it back.
///   * declared peers should declare **equal member widths** (E5509):
///     unequal member tables can never pair positionally.
///
/// D8 exemption (ruled 2026-09-20, one-to-many relay semantics): a declared
/// pair with a **relay side** — a `peer` attribute holding two or more role
/// names (`peer = [Master, Slave]`) — is exempt from both sub-checks. A
/// relay's member table is the union of its sides, so no wholesale width
/// comparison against it is meaningful; and an attach-side role need not
/// name the relay back, because the relay sits transparently on the bus (a
/// Master's peer is a Slave; the repeater in between is not part of their
/// pair law). The former `UART.RS485` Repeater shape (deleted from the
/// library 2026-09-21) was exactly this legal form, not a disease.
///
/// Both sub-checks are definition-space facts — no connection statement is
/// involved. Dangling peer names stay with HW5 above; absence of a member
/// table is E3180's territory and is skipped here, not reported as a
/// mismatch.
fn check_role_peer_mutual_and_width(acc: &mut CheckAccumulator) {
    // Same D10 scan scope as check_role_peer_dangling above: unified view,
    // library interfaces included.
    let ifaces = crate::definition_space().all_interfaces();
    for (sn, iface) in ifaces.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }

        // Member width per role, from the role's own declaration table.
        let width_of =
            |role: &crate::semantic::basic::mc_role::McRole| role.pins.member_names().len();

        for role in &iface.roles {
            for attr in &role.attrs {
                let key = attr.id.to_string();
                if key != "peer" {
                    continue;
                }
                let my_width = width_of(role);
                for peer_name in peer_role_names(&attr.values) {
                    // Dangling names are HW5's (E5506) verdict — skip here.
                    let Some(peer_role) =
                        iface.roles.iter().find(|r| r.name.to_string() == peer_name)
                    else {
                        continue;
                    };
                    // D8 exemption first: a pair with a relay side (either
                    // this role or its peer declaring a multi-peer set) is
                    // legal one-to-many authoring — see the function comment.
                    if is_relay_peer_decl(role) || is_relay_peer_decl(peer_role) {
                        continue;
                    }
                    // A: mutuality. The peer's own `peer` attribute must name
                    // this role back.
                    let is_named_back = peer_role
                        .attrs
                        .iter()
                        .filter(|a| a.id.to_string() == "peer")
                        .flat_map(|a| peer_role_names(&a.values))
                        .any(|n| n == role.name.to_string());
                    if !is_named_back {
                        acc.push(CheckResult {
                            check_name: "hw",
                            severity: CheckSeverity::Warning,
                            uri: Some(uri.clone()),
                            span: attr.key_span.clone(),
                            message: format!(
                                "Interface '{}': role '{}' names peer '{}' \
                                 but '{}' does not name '{}' back; \
                                 peer pairs must be mutual",
                                iface.name, role.name, peer_name, peer_name, role.name,
                            ),
                            code: crate::errcodes::HW_IFACE_PEER_NOT_MUTUAL,
                        });
                    }
                    // B: width agreement. Both sides must actually declare a
                    // member table; an empty table is E3180's verdict.
                    let peer_width = width_of(peer_role);
                    if my_width > 0 && peer_width > 0 && my_width != peer_width {
                        acc.push(CheckResult {
                            check_name: "hw",
                            severity: CheckSeverity::Warning,
                            uri: Some(uri.clone()),
                            span: attr.key_span.clone(),
                            message: format!(
                                "Interface '{}': role '{}' declares {} member(s) \
                                 but its peer '{}' declares {}",
                                iface.name, role.name, my_width, peer_name, peer_width,
                            ),
                            code: crate::errcodes::HW_IFACE_PEER_WIDTH_MISMATCH,
                        });
                    }
                }
            }
        }
    }
}

/// HW11/HW12: the differential-pair declaration checks (diff-pair-design.md,
/// ruled 2026-09-23 — the pair is declared by tagging member rows, never read
/// off a name):
///
///   * a `@pair(group)` group on an interface member table must hold **two**
///     legs (E5512, Error): a differential signal has exactly two faces.
///   * the retired `diff_pair` body key still written (E5513, Warning): tag
///     the member rows with `@pair(group)` instead.
///
/// HW13/HW14/HW15: the physical-constraint slots of a `@pair` row attr
/// (pair-constraint-design.md v0.1, ruled 2026-09-23 — mcc records the
/// requirement and checks its declaration self-consistency, never the
/// routing geometry itself):
///
///   * a constraint slot value must be a length quantity (E5514, Error):
///     `match: 0.2mm`; the time conversion is the consumer's.
///   * both legs of a group write the slot and write equal values
///     (E5515, Error) — a leg without the slot counts as disagreeing when
///     its partner wrote one.
///   * a constraint slot with no group name to ride (E5516, Error): the
///     slot is a property of the pair, so `@pair(match: …)` is dead data.
///
/// All are definition-space facts, reported where they are declared — the
/// same D10 scan scope as the peer checks above. A group whose legs are fine
/// declares its pair and is not reported; an interface with no `@pair` rows
/// is not differential and that is the normal case, never a disease.
///
/// Every pin table of the interface is its own scope: the conductor view AND
/// each role's rows (U205② ruled 2026-09-23 — the adoption reads the pair
/// from the adopted role's rows, so the gate must accept declarations there;
/// one role's group never merges with another role's or the view's).
fn check_iface_pair_groups(acc: &mut CheckAccumulator) {
    let ifaces = crate::definition_space().all_interfaces();
    for (sn, iface) in ifaces.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }

        // E5513: the retired body key, one warning per written attribute.
        for attr in iface.attrs.iter() {
            if attr.id.to_string() != "diff_pair" {
                continue;
            }
            acc.push(CheckResult {
                check_name: "hw",
                severity: CheckSeverity::Warning,
                uri: Some(uri.clone()),
                span: attr.key_span.clone(),
                message: format!(
                    "Interface '{}': the diff_pair key is retired; \
                     tag the member rows with @pair(group) instead",
                    iface.name,
                ),
                code: crate::errcodes::HW_IFACE_DIFF_PAIR_RETIRED,
            });
        }

        // E5512 + the constraint gates, per pin table: group the member rows
        // by their `@pair` tag. The group name is the author's own
        // identifier — equality of the tag is the only operation, the
        // spelling is never read. First-seen group order, member order as
        // declared.
        let pair_key = crate::semantic::basic::attr_keys::KEY_PAIR;
        let tables = std::iter::once(&iface.pins).chain(iface.roles.iter().map(|r| &r.pins));
        for pins in tables {
            let mut groups: Vec<PairGroup> = Vec::new();
            for (_name, id) in pins.member_entries() {
                let Some(pin) = pins.pins.get(&id) else {
                    continue;
                };
                let Some(attr) = pin.attrs.iter().find(|a| a.id.to_string() == pair_key) else {
                    continue;
                };
                // One read per row: the attr's plain values name the group (a
                // constraint slot is a KVS value and never names one), the same
                // tag's key span anchors the report. A row whose tag carries a
                // slot but no group already drew E5516 inside the read — there is
                // no group to attach it to; a bare `@pair` with neither is inert.
                let slot = read_pair_constraint(attr, &uri, acc);
                if slot.group_text.is_empty() {
                    continue;
                }
                match groups.iter_mut().find(|g| g.name == slot.group_text) {
                    Some(g) => g.legs.push(slot),
                    None => groups.push(PairGroup {
                        name: slot.group_text.clone(),
                        legs: vec![slot],
                        span: attr.key_span.clone(),
                    }),
                }
            }
            for group in groups {
                let count = group.legs.len();
                if count != 2 {
                    acc.push(CheckResult {
                        check_name: "hw",
                        severity: CheckSeverity::Error,
                        uri: Some(uri.clone()),
                        span: group.span.clone(),
                        message: format!(
                            "Interface '{}': @pair group '{}' has {} leg(s); \
                             a differential pair has exactly two",
                            iface.name, group.name, count,
                        ),
                        code: crate::errcodes::HW_IFACE_PAIR_NOT_TWO,
                    });
                }
                check_pair_constraint_equality(&iface.name.to_string(), &group, &uri, acc);
            }
        }
    }
}

/// One leg's read of a `@pair` row attr, for the constraint gates: the group
/// name (empty = the row wrote no plain group value) and the `match` slot.
struct PairLegSlot {
    group_text: String,
    kvs_count: usize,
    match_value: PairSlotValue,
    slot_span: Option<std::ops::Range<usize>>,
}

enum PairSlotValue {
    /// The row wrote no `match` slot.
    None,
    /// A length quantity, normalized to meters, with the slot's own text for
    /// the mismatch report.
    Length(f64, String),
    /// A slot value that is not a length quantity — E5514 fired at read time,
    /// so the equality gate skips this leg rather than double-reporting.
    Bad,
}

struct PairGroup {
    name: String,
    legs: Vec<PairLegSlot>,
    span: Option<std::ops::Range<usize>>,
}

/// Read one row's `@pair` attr: the named constraint slots and their shapes.
/// Fires E5516 (slot without a group) and E5514 (slot value not a length)
/// here, where the span and the interface name are at hand.
fn read_pair_constraint(
    attr: &crate::semantic::component::mc_attr::McAttribute,
    uri: &str,
    acc: &mut CheckAccumulator,
) -> PairLegSlot {
    use crate::semantic::component::mc_attr::McAttrVal;

    let mut slot = PairLegSlot {
        group_text: crate::semantic::module::pi::value_texts(attr).join(" "),
        kvs_count: 0,
        match_value: PairSlotValue::None,
        slot_span: None,
    };
    for value in attr.values.iter() {
        let McAttrVal::KVS(kvs) = value else {
            continue;
        };
        slot.kvs_count += 1;
        if kvs.key.to_string() != "match" {
            // The slot-word set is open (pair-constraint-design.md §3.1):
            // `match` is the first word, other keys are inert data until a
            // consumer rules them in.
            continue;
        }
        if slot.slot_span.is_none() {
            slot.slot_span = attr.key_span.clone();
        }
        slot.match_value = match length_of_kvs(kvs) {
            Some(meters) => PairSlotValue::Length(meters, kvs.to_string()),
            None => {
                acc.push(CheckResult {
                    check_name: "hw",
                    severity: CheckSeverity::Error,
                    uri: Some(uri.to_string()),
                    span: attr.key_span.clone(),
                    message: format!(
                        "@pair match slot value '{}' is not a length quantity; \
                         spell the tolerance in a length unit (0.2mm, 8mil)",
                        kvs,
                    ),
                    code: crate::errcodes::HW_PAIR_CONSTRAINT_NOT_LENGTH,
                });
                PairSlotValue::Bad
            }
        };
    }
    if slot.kvs_count > 0 && slot.group_text.is_empty() {
        acc.push(CheckResult {
            check_name: "hw",
            severity: CheckSeverity::Error,
            uri: Some(uri.to_string()),
            span: slot.slot_span.clone().or_else(|| attr.key_span.clone()),
            message:
                "@pair carries a constraint slot but no group name; the constraint has no \
                 pair to ride — write @pair(group, match: 0.2mm)"
                    .to_string(),
            code: crate::errcodes::HW_PAIR_CONSTRAINT_ORPHAN,
        });
    }
    slot
}

/// The length reading of a `match` slot value: a unit value on the length
/// axis, normalized to meters. Anything else — a bare number, a string, a
/// voltage, a parameter reference — has no length reading here.
fn length_of_kvs(kvs: &crate::semantic::basic::mc_kvs::McKVS) -> Option<f64> {
    use crate::semantic::basic::mc_literal::McLiteral;
    use crate::semantic::basic::mc_uval::McUnit;
    use crate::semantic::component::mc_attr::McAttrVal;

    let crate::semantic::basic::mc_kvs::KVSValue::Square(vals) = &kvs.value else {
        return None;
    };
    let [McAttrVal::AttrLiteral(McLiteral::Uval(uv))] = vals.as_slice() else {
        return None;
    };
    if *uv.unit() != McUnit::Len {
        return None;
    }
    Some(uv.value())
}

/// E5515: the legs of one group must agree on the `match` slot — equal
/// values, and a leg whose partner wrote the slot writes it too. Compared in
/// normalized meters with a relative epsilon, so `0.2mm` and `200um` agree.
fn check_pair_constraint_equality(
    iface_name: &str,
    group: &PairGroup,
    uri: &str,
    acc: &mut CheckAccumulator,
) {
    let Some(first) = group
        .legs
        .iter()
        .find(|leg| matches!(leg.match_value, PairSlotValue::Length(..)))
    else {
        return; // no leg declared a constraint — the ordinary pair case
    };
    let PairSlotValue::Length(first_m, first_text) = &first.match_value else {
        unreachable!("found by the matcher above")
    };
    for leg in &group.legs {
        let agrees = match leg.match_value {
            PairSlotValue::Length(m, _) => {
                (m - *first_m).abs() <= 1e-9 * first_m.abs().max(1e-12)
            }
            _ => false,
        };
        if agrees {
            continue;
        }
        let this = match &leg.match_value {
            PairSlotValue::Length(_, text) => format!("a different value ({text})"),
            PairSlotValue::None => "no match slot".to_string(),
            PairSlotValue::Bad => "an ill-formed match slot".to_string(),
        };
        acc.push(CheckResult {
            check_name: "hw",
            severity: CheckSeverity::Error,
            uri: Some(uri.to_string()),
            span: leg.slot_span.clone().or_else(|| group.span.clone()),
            message: format!(
                "Interface '{}': @pair group '{}' legs disagree on the match \
                 constraint: one leg wrote {}, this leg wrote {}",
                iface_name, group.name, first_text, this,
            ),
            code: crate::errcodes::HW_PAIR_CONSTRAINT_MISMATCH,
        });
    }
}

/// Whether `role` declares **relay** peer semantics: its `peer` attribute(s)
/// name two or more roles in total (`peer = [Master, Slave]`). Structural —
/// the count of declared peer names, never their spellings (world-axioms §1
/// A1). A single peer, bare or as a one-element set, stays an ordinary pair.
fn is_relay_peer_decl(role: &crate::semantic::basic::mc_role::McRole) -> bool {
    role.attrs
        .iter()
        .filter(|a| a.id.to_string() == "peer")
        .flat_map(|a| peer_role_names(&a.values))
        .count()
        >= 2
}

/// The role name(s) a `peer` attribute references.
///
/// `peer` accepts either a single role (`peer = Master2W`) or a set of roles
/// (`peer = [Master, Slave]`). The set form parses as
/// `McExpression::Set(..)`; read its items structurally instead of comparing
/// the bracket-list display string as a whole (which never equals any single
/// role name, so `peer = [Master, Slave]` would false-positive).
fn peer_role_names(values: &[crate::McAttrVal]) -> Vec<String> {
    let mut out = Vec::new();
    for val in values {
        if let crate::McAttrVal::AttrExpr(crate::semantic::basic::mc_expr::McExpression::Set(
            items,
        )) = val
        {
            out.extend(
                items
                    .iter()
                    .map(|e| e.to_string().trim().to_string())
                    .filter(|s| !s.is_empty()),
            );
        } else {
            let s = format!("{}", val).trim().to_string();
            if !s.is_empty() {
                out.push(s);
            }
        }
    }
    out
}

// HW6: Component with only single-type IO pins

/// A component where ALL pins share the same IO type (all Input, all Output,
/// or all Power) is unusual. Most real components have a mix of input,
/// output, and power pins. A single-type component may indicate incomplete
/// pin definitions or a misclassified component.
fn check_single_ioc_type_component(acc: &mut CheckAccumulator) {
    let comps = crate::definition_space().workspace_components();
    for (sn, comp) in comps.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }

        let pin_count = comp.pins.pins.len();
        if pin_count < 3 {
            continue; // Too few pins to make this meaningful
        }

        use crate::IOType;
        let mut in_count = 0usize;
        let mut out_count = 0usize;
        let mut ps_count = 0usize;
        let mut nc_count = 0usize;
        let mut io_count = 0usize;

        for pin in comp.pins.pins.values() {
            match pin.iotype {
                IOType::In => in_count += 1,
                IOType::Out => out_count += 1,
                IOType::Power => ps_count += 1,
                IOType::NonCon => nc_count += 1,
                IOType::InOut => io_count += 1,
                IOType::Return | IOType::None => {} // these don't indicate direction
            }
        }

        let active_types = [in_count, out_count, ps_count, nc_count, io_count]
            .iter()
            .filter(|&&x| x > 0)
            .count();

        // If only one active type is present (excluding passive), that's unusual
        if active_types == 1 && pin_count >= 4 {
            // A chip whose every pin is a power pin (e.g. an LDO like AMS1117
            // with IN/OUT/ADJ/GND) is the normal shape of a power component,
            // not an incomplete definition — don't flag it.
            if ps_count > 0 {
                return;
            }
            // Report the active type's actual coverage: the remaining pins are
            // direction-less (Return/None/Label), not members of the named
            // type — e.g. an explicit-`in` load beside bare shield-GND pins.
            let (io_desc, active_count) = if in_count > 0 {
                ("Input", in_count)
            } else if out_count > 0 {
                ("Output", out_count)
            } else {
                return; // NC-only or passive-only, skip
            };

            acc.push(CheckResult {
                check_name: "hw",
                severity: CheckSeverity::Info,
                uri: Some(uri.clone()),
                span: Some(comp.span.start..comp.span.end),
                message: format!(
                    "Component '{}': only one active IO type ('{}', {} of {} pins); \
                     the remaining {} declare no direction. \
                     Most components have mixed IO types (input, output, power). \
                     Verify the pin definitions are complete.",
                    comp.name,
                    io_desc,
                    active_count,
                    pin_count,
                    pin_count - active_count
                ),
                code: crate::errcodes::HW_ALL_SAME_IO_TYPE,
            });
        }
    }
}

// HW8: Function parameter shadows a component pin name

/// When a component function declares a parameter with the same name as a
/// component pin, it creates ambiguity in net expressions. The function
/// parameter may unintentionally shadow the pin reference.
fn check_func_param_pin_shadow(acc: &mut CheckAccumulator) {
    let comps = crate::definition_space().workspace_components();
    for (sn, comp) in comps.iter() {
        let uri = sn.uri.to_string();
        if super::is_test_file(&uri) {
            continue;
        }

        // Collect all pin names
        let pin_names: HashSet<String> = comp.pins.names_to_id.keys().cloned().collect();

        if pin_names.is_empty() || comp.funcs.is_empty() {
            continue;
        }

        for func in comp.funcs.iter() {
            for d in func.params.iter() {
                if let Some(pname) = d.get_primary_name() {
                    if pin_names.contains(&pname) {
                        acc.push(CheckResult {
                            check_name: "hw",
                            severity: CheckSeverity::Warning,
                            uri: Some(uri.clone()),
                            span: Some(comp.span.start..comp.span.end),
                            message: format!(
                                "Component '{}': function '{}' param '{}' shadows a pin name. \
                                 This may cause ambiguity in net expressions within the function body.",
                                comp.name,
                                func.name,
                                pname
                            ),
                            code: crate::errcodes::HW_FUNC_PARAM_SHADOWS_PIN,
                        });
                    }
                }
            }
        }
    }
}
