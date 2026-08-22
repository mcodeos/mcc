// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Phase entry points
//!
//! - Phase 1: Interface instantiation (ports + Iter-5.B member label injection)
//! - Phase 3: Declared instance instantiation (components / sub-modules / labels)
//! - Phase 4: Connection stmt processing entry

use super::expand::pair_members_to_lanes;
use super::FailedRecord;
use super::McModuleInst;
use crate::instant::mc_comp::McComponentInst;
use crate::instant::mc_net::{canonicalize_path, ConnectionInst, InstError, NetPoint, PortInst};
use crate::instant::provenance::ExpansionKind;
use crate::semantic::basic::mc_bus::McBus;
use crate::semantic::basic::mc_endpoint::McEndpoint;
use crate::semantic::basic::mc_ids::IdsSegment;
use crate::semantic::basic::mc_param::{McParamBindings, McParamValue};
use crate::semantic::basic::mc_param_type::{McIoTy, McParamTypeKind};
use crate::semantic::basic::mc_paramd::McParamDeclareKind;
use crate::semantic::basic::mc_phrase::McPhrase;
use crate::semantic::common::{ConnDir, IOType};
use crate::semantic::component::McComponent;
use crate::semantic::mc_inst::McInstance;
use std::collections::HashSet;
use std::sync::Arc;

impl McModuleInst {
    // ========================================================================
    // Phase 1: Interface instantiation
    // ========================================================================
    //
    // ## Iter-5.B — Module bus port passthrough (parent-child boundary label equivalence)
    //
    // ### Problem origin
    //
    // Source `main.mc`:
    //   stmt: V3V3 -> dcdc.[VDD_3V3, GND]    # Parent module main
    // Sub-module `power.mc` POWER_DCDC:
    //   port: in  [VDD_3V3, GND]::DC()
    //
    // Previously `instantiate_interface` only pushed port name `"[VDD_3V3,GND]"` into
    // `self.ports`, **did not register VDD_3V3 / GND as independent symbols**
    // in the sub-module's label namespace. Consequence:
    //
    // ### What happens downstream in the flatten chain
    //
    // Parent's raw connection `V3V3 ~ dcdc.[VDD_3V3,GND]`, when reaching
    // `inst_table.rs::flatten_nets`, runs each `NetPoint.path`
    // through `expand_bracket_list`:
    //
    // ```text
    // "dcdc.[VDD_3V3,GND]"  ──►  ["dcdc.VDD_3V3", "dcdc.GND"]
    // ```
    //
    // Expanded sub-paths are then resolved via `resolve_single_path`:
    //
    //   (1) `main.dcdc.VDD_3V3` ── must be registered in InstTable to hit
    //   (2) `dcdc.VDD_3V3`      ── fallback if (1) misses
    //   (3) `main.dcdc/VDD_3V3` ── bus member fallback (trailing `.`→`/`)
    //
    // (1) is the only reachable path — it requires `main.dcdc.VDD_3V3` to exist as some
    // `InstEntry` (Label / Port / Bus) in the table. Phase 5 of `flatten_module`
    // registers each label in `inst.labels` as `{my_path}.{label_name}`. So **as long as
    // `VDD_3V3` is in the sub-module's `self.labels`**, the expanded lookup will hit.
    //
    // Previously no injection → `main.dcdc.VDD_3V3` doesn't exist → the corresponding
    // endpoint in the parent's V3V3 net is empty → the entire POWER chain is electrically disconnected.
    //
    // ### Fix: inject members into `self.labels` according to port form
    //
    // For ports carrying members, register each member as an independent label in
    // `self.labels`. Three forms must be covered:
    //
    //   * `McInstance::List(list)`       —— Pure bracket `[A, B]` or with prefix
    //                                     `GPIO[1:2]`.
    //   * `McInstance::Bus(bus)`         —— Curly bracket `name{A, B}`.
    //   * `McInstance::Interface(iface)` —— `[A, B]::DC()` form (only when
    //                                     `iface.name.is_list()`).
    //
    // For curly form `dc{VDD_3V3, GND}`, additionally do two things:
    //
    //   (a) Register prefix `dc` as a bus via `ensure_bus` (semantically representing this
    //       curly port is a member-addressable bus). This way when the sub-module body
    //       writes `dc.VDD_3V3`, step 2.3 bus branch of `node_to_netpoint` hits,
    //       returning a stable path.
    //   (b) Also inject `dc.VDD_3V3` / `dc.GND` as independent labels,
    //       working with (a)'s bus path to form a stable connection point.
    //
    // For prefix-named list `GPIO[1:2]`, do not inject bare labels (avoid "1" / "2"
    // polluting the global label namespace), only register prefix bus + dotted label.
    //
    // ### Why not do "port ↔ member bridge connections"
    //
    // One intuitive approach: additionally push a `ConnectionInst` in the sub-module,
    // bundling port literal path (`[VDD_3V3,GND]`) and each member label (`VDD_3V3`,
    // `GND`) into the same connection, letting union-find locally merge them into one net.
    // This way when body stmt writes `[VDD_3V3, GND] -> ...` and reaches the port
    // literal path, it also propagates to member labels.
    //
    // **But this creates electrical shorts**: POWER_DCDC has two bracket-list ports
    // `[VDD_3V3, GND]` and `[VCC_1V2, GND]`, both containing `GND` member.
    // Both bridges contain bare `GND`, union-find merges two nets via `GND`,
    // **connecting 3.3V input and 1.2V output inside the DC-DC chip**.
    // Parent side originally has two independent nets (different names V3V3 and V1V2 don't merge),
    // this introduces connections that don't even exist on the parent side.
    //
    // To avoid this cross-port short, we'd need separate namespaces for each port's members
    // (e.g. `<port>/GND` port-scoped labels), but then `expand_bracket_list` produces
    // `dcdc.GND` which again faces the "parent can't find label in sub-module" old problem — core goal lost.
    //
    // **Conclusion**: bracket-list syntax's "same-name member across ports" ambiguity is a
    // parser-level issue; fully resolving it requires body `[A, B]` to expand into List
    // during parse, going through N×1 adjacency natural path (Iter-5.E vector expansion scope).
    // phases.rs layer only guarantees **parent-child boundary label equivalence**, not
    // doing topology merges that could cause electrical shorts.
    //
    // ### Coverage
    //
    //   * `in [VDD_3V3, GND]::DC()`       → Interface+is_list  ✔
    //   * `ps dc{VDD_3V3, GND}`           → Bus               ✔
    //   * `ps [VDD_3V3, GND]`             → List (@N anonymous)    ✔
    //   * `ps GPIO[1:2]`                  → List (named prefix)    ✔ (bus+dotted only)
    //   * `ps DC1{VDD, GND}`              → Bus               ✔
    //
    // ### Not covered (handled by separate iter)
    //
    //   * `in dc{VDD_3V3, GND}::DC()`     → curly + Interface
    //     `parse_declare` with `Mc2Interface::new_with_str("dc", ...)`
    //     already drops `{VDD_3V3, GND}` members, uninjectable at instantiation stage.
    //     True fix needs to touch `mc_inst.rs::parse_declare` to preserve `inst_ids`
    //     or curly members, outside phases.rs scope.
    //
    //   * Sub-module internal body stmt `[VDD_3V3, GND] -> dcdc{Vin, GND}`
    //     still won't expand — lines 164-168 of `mc_phrase.rs` makes pure bracket fall to
    //     `add_label(ids.to_string())`, becoming a single Label. Plus 1 vs 2
    //     adjacency shape issue, entire body stmt is missing. Iter-5.E vector expansion scope.

    pub(super) fn instantiate_interface(&mut self) -> Result<(), InstError> {
        // ── First clone port list to release immutable borrow of self.def ──────────
        // Loop body needs &mut self (labels / buses write), so can't run
        // directly during iter_with_iotype() borrow.
        let items: Vec<(String, IOType, McInstance)> = self
            .def
            .insts
            .iter_with_iotype()
            .map(|(k, (io, inst))| (k.to_string(), io.clone(), inst.clone()))
            .collect();

        for (port_name, iotype, inst) in &items {
            // ── Bug fix ① ───────────────────────────────────────────
            // `self.def.insts` is a symbol table **shared by ports and body declarations**:
            // contains both real module ports (Label / Bus / List / Interface) and
            // component / sub-module declarations (McInstance::Component / Module).
            //
            // `McInstance::Component` / `McInstance::Module` are instantiated by
            // `instantiate_declarations_resilient`, **are NOT module ports** — even if
            // they have IO annotations in source (e.g. `out flash::FLASH()`,
            // the annotation describes the component's role in the schematic).
            //
            // Old logic indiscriminately pushed every item in insts into self.ports,
            // so `flash` / `X6` with annotations also became PortInst.
            // Downstream `inst_table.rs::flatten_module` first registers ports (step 2)
            // then registers components (step 3), component path collides with existing Port entry
            // and is dedup-skipped — `main.flash` kind ultimately stays Port forever.
            //
            // Fix here: skip these two variants — they don't enter self.ports,
            // so they won't pre-empt component's own path in InstTable.
            if matches!(inst, McInstance::Component(_) | McInstance::Module(_)) {
                continue;
            }

            // ── Bug fix ② ───────────────────────────────────────────
            // Only items with a non-None IOType are real ports.
            // Label/Bus/List items with IOType::None are internal body declarations
            // (e.g. `VCC`/`Vin` power labels in `VCC -> Q1 -> Vin`).
            // They must NOT be pushed as module ports, otherwise viz sees them as
            // module ports instead of internal labels.
            //
            // ── P2-4 exception ──
            // Interface-type items in the module signature (e.g. `[VDD_3V3,GND]::DC(3.3V)`)
            // have IOType::None but ARE real ports. They must be added to self.ports
            // so that `bind_actual_args_to_ports` can find them.
            let is_interface_port = matches!(inst, McInstance::Interface(_));
            if matches!(iotype, IOType::None) && !is_interface_port {
                continue;
            }

            // 1. When creating PortInst, extract bus_members according to port form
            //    —— Iter-8: let N×1 bus ports expand according to declaration during endpoint resolution.
            let mut bus_members = extract_port_bus_members(inst, port_name);
            // ── §8.9.6.6 step 2: scalar → bus upgrade by usage (shape by use) ──
            // A port declared as a single point (e.g. `out spi1`) is not shape-locked
            // at the declaration site; when the module body uses it as a bus
            // (`spi1{CS, SCLK, MOSI, MISO}` / `spi1.CS` / `spi1[1:4]`), upgrade it
            // before instantiation so member labels are injected and body member
            // access resolves to the port's lanes instead of a dangling dotted label.
            let mut inject_inst = inst.clone();
            if bus_members.is_empty() {
                if let Some(members) = self.collect_port_usage_members(port_name) {
                    if members.len() >= 2 {
                        bus_members = members.clone();
                        // Synthesize a Bus instance so inject_port_member_labels
                        // registers bare + dotted member labels and the prefix bus
                        // for the upgraded port.
                        inject_inst = McInstance::Bus(McBus::new_with_members(port_name, members));
                    }
                }
            }
            let port = PortInst::with_members(port_name, iotype.clone(), bus_members.clone());
            self.ports.push(port);

            // 2. Iter-5.B —— inject member labels / register prefix bus according to port form.
            self.inject_port_member_labels(iotype, &inject_inst);
        }

        // ── P2-4: process interface-type parameters from module signature ──
        // Module signature params like `[VDD_3V3,GND]::DC(3.3V)` live in `def.params`,
        // not `def.insts`. Without this, `bind_actual_args_to_ports` can't find them,
        // and parent modules can't pass bus arguments to submodule interface ports.
        for pd in self.def.params.iter() {
            let is_interface_port = matches!(
                pd.param_type.kind,
                McParamTypeKind::Interface { .. } | McParamTypeKind::InterfaceWithRole { .. }
            );
            if !is_interface_port {
                continue;
            }

            let port_name = pd.get_primary_name().unwrap_or_else(|| pd.display_name());
            let iotype = match pd.param_type.direction {
                Some(McIoTy::Input) => IOType::In,
                Some(McIoTy::Output) => IOType::Out,
                Some(McIoTy::InOut) => IOType::InOut,
                Some(McIoTy::PowerSupply) => IOType::Power,
                Some(McIoTy::Analog) => IOType::Analog,
                Some(McIoTy::NotConnected) => IOType::NonCon,
                Some(McIoTy::Label) => IOType::Label,
                None => IOType::InOut,
            };

            // Extract bus members — §11: keep source declaration order.
            // ── Handle both Multiple and Single (curly) forms ──
            let bus_members: Vec<String> = match &pd.kind {
                McParamDeclareKind::Multiple(members) => {
                    members.iter().map(|m| m.to_string()).collect()
                }
                McParamDeclareKind::Single(ids) => {
                    // Handle curly bracket form: vin{VCC, GND} → ["VCC", "GND"]
                    // Handle square bracket form: [VDD_3V3, GND] → ["VDD_3V3", "GND"]
                    let mut members: Vec<String> = Vec::new();
                    for seg in &ids.segments {
                        match seg {
                            IdsSegment::Curly(curly_segs) | IdsSegment::Square(curly_segs) => {
                                for curly_seg in curly_segs {
                                    members.push(curly_seg.to_string());
                                }
                            }
                            _ => {}
                        }
                    }
                    members
                }
                _ => Vec::new(),
            };

            let port = PortInst::with_members(&port_name, iotype.clone(), bus_members.clone());
            self.ports.push(port);

            // Inject member labels so that connection stmts can reference them
            for member in &bus_members {
                self.labels.insert(
                    member.clone(),
                    NetPoint::new(member, iotype.clone()).with_member_name(member),
                );
            }
        }

        Ok(())
    }

    /// Iter-5.B: inject member labels for ports carrying member info into current module,
    /// and register prefix bus for curly form.
    ///
    /// Detailed design see long comment above `instantiate_interface`.
    ///
    /// Side effects (local to this module only, no cross-module / global writes):
    ///   * `self.labels` —— insert bare member and dotted member labels
    ///   * `self.buses` —— register prefix bus for curly form (ensure_bus incremental merge)
    ///
    /// **Does not push any bridge connections to `self.connections`** — reason in long comment
    /// "Why not do port↔member bridge connections" section.
    fn inject_port_member_labels(&mut self, iotype: &IOType, inst: &McInstance) {
        // ── Step 0: Calculate which members to inject according to port form ────────────────────
        //
        // Returned triple meaning:
        //   bare_members    —— inject as prefix-free label into self.labels
        //                      (key searched by parent's `expand_bracket_list`)
        //   dotted_prefix   —— Some(prefix) means also ensure_bus(prefix)
        //                      + inject `prefix.MEMBER` form label
        //                      None means no dotted registration
        //   dotted_members  —— member list for dotted registration (may differ from bare_members:
        //                      `GPIO[1:2]` only goes through dotted, avoids polluting
        //                      bare label namespace with `"1"` / `"2"`)
        let (bare_members, dotted_prefix, dotted_members): (
            Vec<String>,
            Option<String>,
            Vec<String>,
        ) = match inst {
            // Case 1: Pure bracket `[A, B]` or with prefix `GPIO[1:2]`
            //         (parse_opd is_square_only / non-curly bus branch)
            McInstance::List(list) if !list.member.is_empty() => {
                // Distinguish anonymous vs named:
                //   anonymous @N          → member is an independent label in electrical sense
                //   named GPIO[1:2]   → member is a number or sub-signal,
                //                     not suitable as bare label (avoids pollution)
                let is_anonymous = list.name.is_empty() || list.name.starts_with('@');
                if is_anonymous {
                    (list.member.clone(), None, Vec::new())
                } else {
                    (Vec::new(), Some(list.name.clone()), list.member.clone())
                }
            }

            // Case 2: Curly bracket `name{A, B}` (parse_opd curly branch)
            McInstance::Bus(bus) if !bus.member.is_empty() => {
                // curly two access forms must both be covered:
                //   body writes `VDD_3V3`    → hit bare label
                //   body writes `dc.VDD_3V3` → hit dotted label + bus.member fallback
                (
                    bus.member.clone(),
                    Some(bus.name.clone()),
                    bus.member.clone(),
                )
            }

            // Case 3: Bracket + interface `[A, B]::DC()`
            //         (parse_declare::is_square_only branch: iface.name
            //          is a Square segment, list_members() can retrieve members)
            //         and curly + interface `dc{A, B}::DC()` `MIC{P, N}::ADC.DIFF()`
            //         (mc_inst.rs::parse_declare now uses `Mc2Interface::new(inst_ids, ...)`
            //          preserving curly members into `iface.name`, retrieved via `as_bus()`)
            McInstance::Interface(iface) => {
                if let Some(members) = iface.name.list_members() {
                    // Bracket literal `[A, B]`, no meaningful "prefix", only bare label injection.
                    (members, None, Vec::new())
                } else if let Some((prefix, members)) = iface.name.as_bus() {
                    // ★ FIX (paired with mc_inst.rs `Mc2Interface::new(inst_ids, ...)` fix):
                    // curly form `dc{A, B}::DC()` can now retrieve ("dc", ["A", "B"]),
                    // injecting both bare label and registering prefix bus + dotted label,
                    // behavior fully consistent with Case 2 (Bus).
                    (members.clone(), Some(prefix), members)
                } else {
                    // Scalar-named interface (e.g. V3V3::DC(3.3V), vin::DC(5V)):
                    // extract members from interface type's pins in source
                    // declaration order (§11.1), register prefix bus
                    // with dotted labels (e.g. V3V3.VCC, V3V3.GND).
                    let pin_names: Vec<String> = iface.base.pins.member_names();
                    if pin_names.len() >= 2 {
                        let port_name = iface.name.to_string();
                        (pin_names.clone(), Some(port_name), pin_names)
                    } else {
                        return;
                    }
                }
            }

            // Other: Label / Component / Module / BusRef etc. unrelated to members, skip
            _ => return,
        };

        // If both member sets are empty (usually Case 1 named but no real members), return directly.
        if bare_members.is_empty() && dotted_members.is_empty() {
            return;
        }

        // ── Step A1: Inject bare member labels ────────────────────────────────
        //
        // Use entry().or_insert_with(...) instead of insert(...): if same-name
        // label has already been registered by other paths (explicit declaration, earlier ports, build helpers, etc.),
        // keep existing entry, avoid silent overwrite.
        for m in &bare_members {
            if m.is_empty() {
                continue;
            }
            self.labels
                .entry(m.clone())
                .or_insert_with(|| NetPoint::new(m, iotype.clone()).with_member_name(m));
        }

        // ── Step A2: curly form additional register prefix bus + dotted label ────────
        //
        // This is not a "bridge", just declaring "`dc` is a bus with VDD_3V3 / GND members",
        // so that `node_to_netpoint` step 2.3 / step 3 can resolve body stmt `dc.VDD_3V3` reference
        // by bus semantics. Does not append to `self.connections`, does not cause any union-find merges.
        if let Some(prefix) = dotted_prefix.as_ref() {
            if !prefix.is_empty() && !dotted_members.is_empty() {
                // ensure_bus does incremental merge, ignore Err — current implementation always returns Ok
                let _ = self.ensure_bus(prefix, &dotted_members);

                for m in &dotted_members {
                    if m.is_empty() {
                        continue;
                    }
                    let dotted = format!("{prefix}.{m}");
                    self.labels.entry(dotted.clone()).or_insert_with(|| {
                        NetPoint::new(&dotted, iotype.clone()).with_member_name(m)
                    });
                }
            }
        }

        // ── P2-12: bridge ground members between bare and dotted labels ──
        // A port's ground member (e.g. `vin.GND`) is physically the same net
        // as the module-level bare `GND` label. Without this bridge, a
        // connection resolving to the dotted label (`vin -> ldo.VIN` →
        // `vin.GND`) and a connection resolving to the bare label
        // (`.Cap(_)` implicit GND / `connect_scalar_to_dc_bus` ground
        // branch → bare `GND`) form two separate ground nets.
        if let Some(prefix) = dotted_prefix.as_ref() {
            for m in &dotted_members {
                if m.is_empty() || !is_ground_name(m) {
                    continue;
                }
                let bare = self.labels.get(m).cloned();
                let dotted = self.labels.get(&format!("{prefix}.{m}")).cloned();
                if let (Some(b), Some(d)) = (bare, dotted) {
                    let id = self.next_conn_id();
                    self.add_connection(self.make_conn_with_provenance(
                        id,
                        vec![b, d],
                        ConnDir::Undirected,
                        None,
                    ));
                }
            }
        }
    }

    // ========================================================================
    // Phase 3: Declared instance instantiation
    // ========================================================================

    pub(super) fn instantiate_declarations_resilient(&mut self) {
        // ★ Clone to owned Vec to release immutable borrow of self.def,
        //   so loop body can call record_error/push etc. with &mut self
        let items: Vec<(String, McInstance)> = self
            .def
            .insts
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();

        for (_name, ident) in items {
            match &ident {
                McInstance::Component(c) => {
                    // ── Expansion provenance: Declare (leaf record, §4.1-B5) ──
                    // call_site = declared instance position (module port_spans);
                    // def_site = class definition position.
                    let call_site = self
                        .def
                        .insts
                        .port_spans()
                        .get(&c.name.to_string())
                        .and_then(|v| v.first().cloned())
                        .map(|s| {
                            crate::semantic::common::SourcePos::new(
                                self.def_uri.clone(),
                                s.start as u32,
                            )
                        });
                    let def_site = Some(crate::semantic::common::SourcePos::new(
                        c.base.uri.clone(),
                        c.base.span.start as u32,
                    ));
                    let eidx = self.expansion.begin(
                        ExpansionKind::Declare,
                        None,
                        c.name.to_string(),
                        call_site,
                        def_site,
                    );
                    let inst = if c.nc {
                        McComponentInst::with_nc(&c.name.to_string(), c.base.clone(), &c.params)
                    } else if c.params.is_empty() {
                        McComponentInst::new(&c.name.to_string(), c.base.clone())
                    } else {
                        match McComponentInst::with_params(
                            &c.name.to_string(),
                            c.base.clone(),
                            &c.params,
                        ) {
                            Ok(inst) => inst,
                            Err(e) => {
                                let reason = format!("{:?}", e);
                                // Binding failure is a hard error: the
                                // argument list does not match the class
                                // signature (unknown named arg, excess
                                // arg, or missing required param). The
                                // instance is skipped, but the reason is
                                // reported so the author sees it.
                                self.record_error(
                                    crate::errcodes::INST_PARAM_BIND_FAILED,
                                    crate::errcodes::format_msg(
                                        crate::errcodes::INST_PARAM_BIND_FAILED,
                                        &[&c.name.to_string(), &c.base.name.to_string(), &reason],
                                    ),
                                );
                                mcc_dbg!(
                                    "inst::mod",
                                    "[ERROR] Failed to instantiate component '{}' (class '{}'): {}",
                                    c.name,
                                    c.base.name,
                                    reason
                                );
                                self.failed_classes.insert(c.base.name.to_string());
                                self.failed_records.push(FailedRecord {
                                    module: self.name.clone(),
                                    src_line: self
                                        .current_stmt_span
                                        .as_ref()
                                        .map(|s| (s.offset / 1000) as usize),
                                    component_name: c.name.to_string(),
                                    class_name: c.base.name.to_string(),
                                    reason,
                                });
                                self.expansion.end(eidx);
                                continue;
                            }
                        }
                    };
                    self.add_component(inst);

                    // ── P1-C5: Execute same-name constructor func ──
                    // (nested ComponentCtor record, parent = this Declare record)
                    if !c.params.is_empty() {
                        let inst_name = c.name.to_string();
                        let comp_def = c.base.clone();
                        let args = c.params.clone();
                        self.run_component_constructor(&inst_name, &comp_def, &args);
                    }
                    self.expansion.end(eidx);
                }
                McInstance::Module(m) => {
                    // ── Expansion provenance: Declare (leaf record, §4.1-B5) ──
                    let call_site = self
                        .def
                        .insts
                        .port_spans()
                        .get(&m.name.to_string())
                        .and_then(|v| v.first().cloned())
                        .map(|s| {
                            crate::semantic::common::SourcePos::new(
                                self.def_uri.clone(),
                                s.start as u32,
                            )
                        });
                    let def_site = Some(crate::semantic::common::SourcePos::new(
                        m.base.uri.clone(),
                        m.base.span.start as u32,
                    ));
                    let eidx = self.expansion.begin(
                        ExpansionKind::Declare,
                        None,
                        m.name.to_string(),
                        call_site,
                        def_site,
                    );
                    let inst_name = m.name.to_string();
                    let mut inst = McModuleInst::new(&inst_name, m.base.clone());
                    // ★ Sub-module instantiation failure → record diagnostics, but keep instance
                    if let Err(e) = inst.instantiate() {
                        self.record_error(
                            crate::errcodes::INST_SUBMODULE_INSTANTIATE_FAILED,
                            crate::errcodes::format_msg(
                                crate::errcodes::INST_SUBMODULE_INSTANTIATE_FAILED,
                                &[&m.name, &e],
                            ),
                        );
                    }
                    // ── P1-C4: Connect declared args (V3V3, V1V2) to sub-module ports ──
                    if !m.args.is_empty() {
                        let ports = inst.ports.clone(); // Avoid borrow conflict with self
                        self.bind_actual_args_to_ports(&inst_name, &ports, &m.args);
                    }
                    self.merge_diagnostics_from(&inst);
                    self.add_submodule(inst);
                    self.expansion.end(eidx);
                }
                McInstance::Bus(label) => {
                    // ── Iter-5.B cooperation point ───────────────────────────────────
                    // Keep old logic of treating McInstance::Bus as label name injection.
                    // Use entry().or_insert to avoid overwriting the more precise NetPoint
                    // injected by phase 1 using port's iotype.
                    self.labels
                        .entry(label.name.clone())
                        .or_insert_with(|| NetPoint::new(&label.name, IOType::None));
                }
                _ => {}
            }
        }
    }

    // ========================================================================
    // Phase 1-2-4: Connection stmt processing
    // ========================================================================

    pub(super) fn instantiate_stmts_resilient(&mut self) {
        let stmts = self.def.stmts.clone();
        let stmt_spans = self.def.stmt_spans.clone();
        for (_i, _l) in stmts.iter().enumerate() {}
        for (idx, stmt) in stmts.iter().enumerate() {
            // ── Iter-6.S4.3 ──────────────────────────────────────────────
            // **per-stmt auto_inst_map scope reset**
            //
            // Background: auto_inst_map uses McPhrase pointer address as key, associating
            // process_member_internal's product (instance name) with resolve_funccall_*
            // query. This pointer-key mechanism is only safe **within the lifetime of a single McPhrase tree** —
            // after process_stmt call returns, the McPhrase nodes from the previous stmt
            // are freed, their addresses may be reused by newly allocated McPhrase in the next stmt.
            // At this point old entry is a dangling reference, hitting it by new address **points to wrong instance**.
            //
            // Triggering example (captured in practice after Iter-6.S4 fix):
            //   stmt N:   `mcu.setup().add_caps().i2c().do_flash(flash)`
            //             — Iter-6.S4 fallback wrote 4 stale entries
            //             (Note: that insert has been removed by Iter-6.S4.2, but dispatch
            //             success path, iterated calls, builtin twopin and other locations still write)
            //   stmt N+1: `mic(V3V3).MIC -> mcu{...} -> speaker{...}`
            //             — mic FuncCall new address collides with stmt N's old address
            //             — resolve_funccall_right finds "mcu"
            //             — mic.MIC incorrectly resolved as mcu.DAC_OUT/SPK_MUTE
            //             — 5 independent signals shorted into one super net
            //
            // Fix: clear before starting each stmt in top-level connections loop.
            //
            // **Note: can only clear here at top-level loop**, not at process_stmt entry —
            // because instantiate_user_func / instantiate_instance_method
            // **recursively call** process_stmt (to expand function body), that layer must share the outer
            // auto_inst_map. Here at the true "stmt boundary", recursive calls are already in
            // deeper process_stmt call stack, not affected by this clear.
            //
            // Side effect tracking: there is no McPhrase sharing between top-level stmts
            // (each stmt is an independent AST subtree), so clear won't lose any entries
            // that **should be shared across stmts**. The overall instantiation results (components / sub_modules /
            // connections) are in other fields of self, not in auto_inst_map, unaffected by clear.
            self.auto_inst_map.clear();

            // ★ Set current stmt span for diagnostic position reporting.
            //   Used as fallback when NetPoint.src_pos is unavailable (e.g., E2003/E2005).
            let stmt_span = stmt_spans.get(idx).map(|s| {
                crate::semantic::common::SourcePos::new(self.def_uri.clone(), s.start as u32)
            });
            self.current_stmt_span = stmt_span.clone();

            if let Err(e) = self.process_stmt(stmt) {
                // ★ Single connection stmt failure doesn't interrupt, record diagnostics then continue processing subsequent stmts
                self.record_warning(
                    crate::errcodes::INST_STMT_PARSE_FAILED,
                    crate::errcodes::format_msg(
                        crate::errcodes::INST_STMT_PARSE_FAILED,
                        &[&idx as &dyn std::fmt::Display, &e],
                    ),
                );
            }
            // ★ Restore the current stmt span: recursive expansions (user
            // funcs / instance methods) overwrite it, so without this restore
            // connections created after the recursion (e.g. a transposed
            // declareb) are attributed to the callee's stmt instead.
            self.current_stmt_span = stmt_span;
        }
        // Clear after loop to avoid stale span leaking into post-stmt checks.
        // `current_trunk` needs no reset here: every producer is RAII
        // guarded (§7.11(2)) and restores it on exit.
        self.current_stmt_span = None;

        // ── P2-C2: After all body stmts processed, project accumulated bus members to bare ports ──
        // NOTE: These post-processing steps are now invoked from instantiate() after
        // auto_invoke_module_funcs(), so they cover both regular stmts and auto-invoked closures.
        // self.infer_bare_port_members_from_buses();  // moved to instantiate()
        // self.dedup_connections();                    // moved to instantiate()
        // self.check_unbound_param_ports();            // moved to instantiate()
    }

    /// ── P5: Deduplicate equivalent connections ──────────────────────────────────────────────
    /// key = **unordered** set of each point's canonical path in connection (sort + dedup).
    /// Same set ⇒ same electrical connection (order irrelevant, duplicate points meaningless), keep only first.
    /// No-op for net aggregation result (union-find already merged), only clears redundant connections and warnings.
    pub(super) fn dedup_connections(&mut self) {
        let before = self.connections.len();
        let mut seen: HashSet<Vec<String>> = HashSet::new();
        let mut kept: Vec<ConnectionInst> = Vec::with_capacity(before);
        for conn in std::mem::take(&mut self.connections) {
            let mut key: Vec<String> = conn
                .points
                .iter()
                .map(|p| canonicalize_path(&p.path))
                .collect();
            key.sort();
            key.dedup();
            if seen.insert(key) {
                kept.push(conn);
            }
        }
        let _removed = before - kept.len();
        self.connections = kept;
    }

    /// ── P2: unify component instance pin "alias paths" to "pid paths" ──────
    /// `ldo.Vout` / `ldo.GND` / `ldo.VIN.Vin` → `ldo.5` / `ldo.2` / `ldo.1`.
    /// These alias forms come from multiple construction paths (get_left_points
    /// member branch directly concatenates the path, component func body
    /// prefixing, etc.); they bypass node_to_netpoint and so don't get parsed;
    /// whereas .Cap() etc. use the pid form. Different strings → union-find
    /// never merges. Here we collapse them in one pass before union.
    // ========================================================================
    // Post-expansion validation: verify NetPoint references
    // ========================================================================

    /// Validate all generated NetPoints after expansion.
    ///
    /// Checks:
    /// 1. Component pin references — owner is a component, verify pin exists
    /// 2. Sub-module port references — owner is a sub-module, verify port exists
    ///
    /// Emits user-visible warning diagnostics for unresolved references
    /// (migrated from `tracing::warn!` per §7.2.3 — these are the func-body
    /// expansion artifacts Pass1 does not see). Position is unknown at the
    /// instance layer, so warnings are anchored at the file start (0,0),
    /// mirroring the instantiation-layer precedent (PULLUP_DEGENERATE).
    /// Called before `build_net_table()`.
    pub(super) fn validate_expanded_net_points(&self) {
        for conn in &self.connections {
            for pt in &conn.points {
                if let Some(ref owner) = pt.owner {
                    // Check component instance pins
                    if let Some(comp) = self.find_component(owner) {
                        // Extract the pin name from the path (after "owner.")
                        let pin_name = pt
                            .path
                            .strip_prefix(&format!("{owner}."))
                            .unwrap_or(&pt.path);
                        // Check if pin exists in component's pin map
                        if !pin_name.is_empty()
                            && !comp.pins.contains_key(pin_name)
                            && !comp.def.pins.names_to_id.contains_key(pin_name)
                        {
                            let available: Vec<&str> =
                                comp.pins.keys().map(|k| k.as_str()).collect();
                            crate::db::diagnostic::diagnostic::diagnostic_log(
                                crate::errcodes::COMPONENT_PIN_NOT_FOUND,
                                crate::db::diagnostic::diagnostic::DiagnosticLevel::Warning,
                                0,
                                0,
                                &crate::errcodes::format_msg(
                                    crate::errcodes::COMPONENT_PIN_NOT_FOUND,
                                    &[&pin_name, owner, &available.join(", ")],
                                ),
                                &[],
                            );
                        }
                    } else if let Some(sub) = self.find_submodule(owner) {
                        // Sub-module port reference — verify the port reference
                        // resolves structurally (exact port name, bare member of
                        // a bus port, or `port.member` against the member group
                        // registered at instantiation; see `is_valid_port_ref`).
                        let port_name = pt
                            .path
                            .strip_prefix(&format!("{owner}."))
                            .unwrap_or(&pt.path);
                        if !port_name.is_empty() && !sub.is_valid_port_ref(port_name) {
                            let available: Vec<&str> =
                                sub.ports.iter().map(|p| p.name.as_str()).collect();
                            crate::db::diagnostic::diagnostic::diagnostic_log(
                                crate::errcodes::MODULE_PORT_NOT_FOUND,
                                crate::db::diagnostic::diagnostic::DiagnosticLevel::Warning,
                                0,
                                0,
                                &crate::errcodes::format_msg(
                                    crate::errcodes::MODULE_PORT_NOT_FOUND,
                                    &[&port_name, owner, &available.join(", ")],
                                ),
                                &[],
                            );
                        }
                    }
                    // If owner is neither a component nor sub-module, it might be
                    // a net label or external reference — skip validation.
                }
            }
        }
    }

    // ========================================================================
    // P1: Args → Port binding / component constructor func
    // ========================================================================

    /// Connect declared instance args to sub-module formal ports by **position**.
    ///
    /// Formal port order = order of interface ports in sub-module signature
    /// (`module mod.sub([VDD_3V3,GND]::DC, [VCC_1V2,GND]::DC)` → port0, port1).
    ///
    /// Member alignment strategy (short-circuit safe):
    ///   1. Equal-width multi-member zip: `[A,B] -> port{X,Y}` → A~inst.X, B~inst.Y
    ///   2. DC single rail (arg is 1 DC label, port is 2 members and exactly 1 is non-ground):
    ///      Rail member ← arg label; ground member ← global GND.
    ///      (Covers `V3V3 -> [VDD_3V3,GND]`: VDD_3V3~V3V3, GND~GND, **no short**)
    ///   3. Rest (scalar↔scalar / unknown shape): single `arg ~ inst.port` (whole bus,
    ///      left to P2's expand_port_lanes for member expansion).
    pub(super) fn bind_actual_args_to_ports(
        &mut self,
        inst_name: &str,
        ports: &[PortInst],
        args: &[McParamValue],
    ) {
        let formal: Vec<&PortInst> = ports
            .iter()
            .filter(|p| p.name.trim_start().starts_with('[') || !p.bus_members.is_empty())
            .collect();

        let mut used = vec![false; formal.len()];

        for (ai, arg) in args.iter().enumerate() {
            // Expand arg into lane + get name (for voltage matching)
            let arg_elems = Self::param_value_to_node_elements(arg);
            let arg_name = arg_elems
                .first()
                .map(|e| e.name.clone())
                .unwrap_or_default();
            let mut arg_lanes: Vec<NetPoint> = Vec::new();
            for e in &arg_elems {
                arg_lanes.extend(self.expand_node_element(e));
            }

            // Choose formal port: ① voltage token match (order irrelevant); ② position fallback (next unused)
            let arg_v = voltage_token(&arg_name);
            let mut chosen: Option<usize> = None;
            if let Some(ref v) = arg_v {
                chosen = (0..formal.len()).find(|&fi| {
                    !used[fi] && {
                        let members = if !formal[fi].bus_members.is_empty() {
                            formal[fi].bus_members.clone()
                        } else {
                            parse_bracket_members(&formal[fi].name)
                        };
                        members
                            .iter()
                            .any(|m| voltage_token(m).as_deref() == Some(v.as_str()))
                    }
                });
            }
            let pi = match chosen.or_else(|| (0..formal.len()).find(|&fi| !used[fi])) {
                Some(pi) => pi,
                None => {
                    self.record_warning(
                        crate::errcodes::INST_ARG_NO_FORMAL_PORT,
                        crate::errcodes::format_msg(
                            crate::errcodes::INST_ARG_NO_FORMAL_PORT,
                            &[&inst_name, &ai as &dyn std::fmt::Display, &arg_name],
                        ),
                    );
                    continue;
                }
            };
            used[pi] = true;
            let port = formal[pi];

            let members: Vec<String> = if !port.bus_members.is_empty() {
                port.bus_members.clone()
            } else {
                parse_bracket_members(&port.name)
            };

            // Port-side points for a member: named ports give both bare
            // (`inst.MEMBER`) and dotted (`inst.base.MEMBER`) forms, mirroring
            // bind_call_args_to_ports. The sub-module body references the dotted
            // form (`USB_VBUS_1.VDD_3V`); without the dotted point here, the
            // bound arg (e.g. V3V3.VCC) and the body's rail label stay on
            // separate nets (e.g. SPEAKER_M's amp power floating).
            let port_base = port_base_name(&port.name);
            let named =
                !port_base.is_empty() && !port_base.starts_with('@') && !port_base.starts_with('[');
            let pio = port.iotype.clone();
            let make_ports = |member: &str, io: IOType| -> Vec<NetPoint> {
                let mut v = vec![NetPoint::with_owner(
                    &format!("{inst_name}.{member}"),
                    inst_name,
                    io.clone(),
                )];
                if named {
                    v.push(NetPoint::with_owner(
                        &format!("{inst_name}.{port_base}.{member}"),
                        inst_name,
                        io,
                    ));
                }
                v
            };

            // ── Case 1: Equal-width multi-member → §11.3 pairing ──
            // Pair by member name first (deterministic alignment even when the
            // arg side and the port side declare members in different order),
            // with positional fallback for names with no partner; output in
            // port (member) declaration order. No alphabetical re-sorting.
            if members.len() >= 2 && arg_lanes.len() == members.len() {
                let lane_idx = pair_members_to_lanes(&members, &arg_lanes);
                for (m, ai) in members.iter().zip(lane_idx.iter()) {
                    if *ai == usize::MAX {
                        continue;
                    }
                    let mut pts = make_ports(m.as_str(), pio.clone());
                    pts.push(arg_lanes[*ai].clone());
                    let id = self.next_conn_id();
                    self.add_connection(self.make_conn_with_provenance(
                        id,
                        pts,
                        ConnDir::Undirected,
                        None,
                    ));
                }
                continue;
            }
            // ── Case 2: DC single rail (arg scalar, port [rail, ground], exactly 1 non-ground) ──
            let ground_cnt = members.iter().filter(|m| is_ground_name(m)).count();
            if members.len() >= 2 && arg_lanes.len() == 1 && (members.len() - ground_cnt) == 1 {
                let arg_pt = arg_lanes.into_iter().next().unwrap();
                for m in &members {
                    let mut pts = make_ports(m.as_str(), pio.clone());
                    let id = self.next_conn_id();
                    if is_ground_name(m) {
                        let gnd = self.node_to_netpoint(&McBus::new("GND"));
                        pts.push(gnd);
                    } else {
                        pts.push(arg_pt.clone());
                    }
                    self.add_connection(self.make_conn_with_provenance(
                        id,
                        pts,
                        ConnDir::Undirected,
                        None,
                    ));
                }
                continue;
            }
            // ── Case 3: scalar↔scalar / unknown shape ──
            if let Some(a) = arg_lanes.into_iter().next() {
                let port_pt = NetPoint::with_owner(
                    &format!("{}.{}", inst_name, port.name),
                    inst_name,
                    port.iotype.clone(),
                );
                let id = self.next_conn_id();
                self.add_connection(self.make_conn_with_provenance(
                    id,
                    vec![a, port_pt],
                    ConnDir::Undirected,
                    None,
                ));
            }
        }
    }

    /// ── Root cause A fix: Call site arg→port binding (multi-member curly/bracket ports) ──────
    ///
    /// Used for the path of "declared sub-module called again with args" (funccall.rs's
    /// `rebind_submodule_params`), e.g. main.mc's `mic(V3V3).MIC` — mic was declared
    /// without args (`MIC_SIP mic`), the real arg `V3V3` is given in the body stmt.
    ///
    /// Key differences from `bind_actual_args_to_ports` (declared args path):
    ///   * **formal filter relaxed**: no longer only `[...]`. Any "non-Out and with ≥2 members
    ///     (or name containing `{`/`[`)" port is considered bindable — so `dc{VDD_3V3,GND}`
    ///     (iotype=None) curly power ports also enter binding logic, no longer blocked by
    ///     `starts_with('[')`.
    ///   * **Named ports connect two sets of labels**: curly named ports (`dc{…}`) in sub-module
    ///     have both bare(`VDD_3V3`) and dotted(`dc.VDD_3V3`) labels injected by
    ///     `inject_port_member_labels`, so here for each member **simultaneously** connect
    ///     `inst.MEMBER` and `inst.base.MEMBER`, ensuring both forms work in sub-module body;
    ///     anonymous bracket ports (`[…]`, base name empty) only connect bare, consistent with
    ///     inject's anonymous branch.
    ///
    /// Returns newly created connections (does not directly push to self.connections), handed
    /// to caller (via `FuncCallInst::Components`) for unified merge, consistent with existing
    /// funccall dispatch flow.
    ///
    /// # Boundaries / Scope
    ///
    /// * **Scalar interface ports** (`vin::DC(5V)`, no bus_members and no `{}`/`[]`) not in
    ///   this filter scope — they need to supplement `{VCC,GND}` members from interface type `DC`
    ///   before binding, a separate sub-item not handled here (ldo grounding still pending).
    /// * Excess args beyond bindable ports emit warning 940 (mirroring
    ///   `bind_actual_args_to_ports`); port-side missed binding is covered
    ///   by `check_unbound_param_ports`.
    pub(super) fn bind_call_args_to_ports(
        &mut self,
        inst_name: &str,
        ports: &[PortInst],
        args: &[McParamValue],
    ) -> Vec<ConnectionInst> {
        let mut out: Vec<ConnectionInst> = Vec::new();

        // formal = non-Out ports "with >=2 members / name contains {} / name starts with [".
        // Note: `formal` borrows the `ports` parameter (caller-provided clone), unrelated to self,
        // so subsequent `&mut self` calls (next_conn_id/expand_node_element/...) don't conflict.
        let formal: Vec<&PortInst> = ports
            .iter()
            .filter(|p| {
                !matches!(p.iotype, IOType::Out)
                    && (!p.bus_members.is_empty()
                        || p.name.contains('{')
                        || p.name.trim_start().starts_with('['))
            })
            .collect();
        if formal.is_empty() {
            return out;
        }

        let mut used = vec![false; formal.len()];

        for arg in args.iter() {
            // Expand arg into lane + get name (for voltage matching)
            let arg_elems = Self::param_value_to_node_elements(arg);
            let arg_name = arg_elems
                .first()
                .map(|e| e.name.clone())
                .unwrap_or_default();
            if arg_name.is_empty() || arg_name == "_" {
                continue;
            }
            let mut arg_lanes: Vec<NetPoint> = Vec::new();
            for e in &arg_elems {
                arg_lanes.extend(self.expand_node_element(e));
            }

            // Choose formal port: ① voltage token match (order irrelevant); ② positional fallback (next unused)
            let arg_v = voltage_token(&arg_name);
            let mut chosen: Option<usize> = None;
            if let Some(ref v) = arg_v {
                chosen = (0..formal.len()).find(|&fi| {
                    !used[fi] && {
                        port_members(formal[fi])
                            .iter()
                            .any(|m| voltage_token(m).as_deref() == Some(v.as_str()))
                    }
                });
            }
            let pi = match chosen.or_else(|| (0..formal.len()).find(|&fi| !used[fi])) {
                Some(pi) => pi,
                None => {
                    // Actual args exceed ports -> skip (see function header "Scope").
                    // Mirror bind_actual_args_to_ports' 940 warning so excess named
                    // args are no longer silently dropped; log detail for tracing.
                    let bound = used.iter().filter(|u| **u).count();
                    crate::db::diagnostic::diagnostic::dlog_trace(
                        940,
                        &format!(
                            "bind_call_args_to_ports: module='{}' instance='{inst_name}' arg '{arg_name}' has no formal port to bind | formal_ports={} bound={bound}",
                            self.name,
                            formal.len(),
                        ),
                    );
                    self.record_warning(
                        crate::errcodes::INST_ARG_UNBOUND_DETAILED,
                        crate::errcodes::format_msg(
                            crate::errcodes::INST_ARG_UNBOUND_DETAILED,
                            &[
                                &inst_name,
                                &arg_name,
                                &self.name,
                                &bound as &dyn std::fmt::Display,
                                &formal.len() as &dyn std::fmt::Display,
                            ],
                        ),
                    );
                    continue;
                }
            };
            used[pi] = true;

            // Copy port info from formal[pi] (borrowing ports), then use only owned values,
            // decoupled from `&mut self` calls.
            let members: Vec<String> = port_members(formal[pi]);
            let pio: IOType = formal[pi].iotype.clone();
            let base: String = port_base_name(&formal[pi].name);
            let named: bool = !base.is_empty() && !base.starts_with('@') && !base.starts_with('[');

            // Generate port-side points for a member: named port gives both bare + dotted.
            // Closure only borrows inst_name/base/named (locals), not self.
            let make_ports = |member: &str, io: IOType| -> Vec<NetPoint> {
                let mut v = vec![NetPoint::with_owner(
                    &format!("{inst_name}.{member}"),
                    inst_name,
                    io.clone(),
                )];
                if named {
                    v.push(NetPoint::with_owner(
                        &format!("{inst_name}.{base}.{member}"),
                        inst_name,
                        io,
                    ));
                }
                v
            };

            // ── Case 1: Equal-width multi-member → §11.3 pairing ──
            // Pair by member name first, positional fallback for the rest;
            // output in port (member) declaration order. No alphabetical
            // re-sorting.
            if members.len() >= 2 && arg_lanes.len() == members.len() {
                let lane_idx = pair_members_to_lanes(&members, &arg_lanes);
                for (m, ai) in members.iter().zip(lane_idx.iter()) {
                    if *ai == usize::MAX {
                        continue;
                    }
                    let mut pts = make_ports(m.as_str(), pio.clone());
                    pts.push(arg_lanes[*ai].clone());
                    let id = self.next_conn_id();
                    out.push(self.make_conn_with_provenance(id, pts, ConnDir::Undirected, None));
                }
                continue;
            }

            // ── Case 2: DC single rail (arg scalar, port [rail, ground], exactly 1 non-ground) ──
            let ground_cnt = members.iter().filter(|m| is_ground_name(m)).count();
            if members.len() >= 2 && arg_lanes.len() == 1 && (members.len() - ground_cnt) == 1 {
                let arg_pt = arg_lanes.into_iter().next().unwrap();
                for m in &members {
                    let mut pts = make_ports(m.as_str(), pio.clone());
                    let id = self.next_conn_id();
                    if is_ground_name(m) {
                        let gnd = self.node_to_netpoint(&McBus::new("GND"));
                        pts.push(gnd);
                    } else {
                        pts.push(arg_pt.clone());
                    }
                    out.push(self.make_conn_with_provenance(id, pts, ConnDir::Undirected, None));
                }
                continue;
            }

            // ── Case 3: Shape-mismatch fallback (port passed filter but <2 members, e.g. malformed
            //    single-member curly) -> arg connects to inst.base ──
            if let Some(a) = arg_lanes.into_iter().next() {
                let dst_base = if base.is_empty() {
                    formal[pi].name.clone()
                } else {
                    base.clone()
                };
                let port_pt =
                    NetPoint::with_owner(&format!("{inst_name}.{dst_base}"), inst_name, pio);
                let id = self.next_conn_id();
                out.push(self.make_conn_with_provenance(
                    id,
                    vec![a, port_pt],
                    ConnDir::Undirected,
                    None,
                ));
            }
        }

        out
    }

    /// ── Root cause A companion diagnostic: "multi-member DC power port containing ground is never reached by any connection" ──────
    ///
    /// Runs at the end of `instantiate_stmts_resilient` (after declared-arg binding + body stmt's
    /// rebind connections have been merged into self.connections).
    ///
    /// **Only** targets multi-member power ports containing ground (members >= 2 and at least one is a ground name), purpose:
    ///   * Catch truly floating cases like `SPEAKER_M speaker` where the source omits the power arg
    ///     (`dc{VDD_3V3,GND}` neither has a declared arg, nor is called via `speaker(...)` in the body stmt);
    ///   * Exclude **groundless** signal bus ports like `port1{A,B,C,D}` (no false positives);
    ///   * Exclude ldo's scalar `vin` (no members, not in scope, its grounding is a separate matter).
    ///
    /// Determine "connected": self.connections has a point with path == prefix, or starting with `prefix.`.
    /// Prefix contains both bare `inst.MEMBER` and (for named ports) dotted `inst.base.MEMBER`,
    /// aligned with the two label forms injected by inject/bind.
    ///
    /// Use **warning(942)** not error: this is a heuristic based on "connection path prefix matching",
    /// not compilable-verifiable in this environment; in case of false positives on ports indirectly grounded via nets, warning does not block.
    pub(super) fn check_unbound_param_ports(&mut self) {
        // ① Read-only self.sub_modules, compute prefix set for each port to check (borrows released immediately).
        //    key = (instance, base name): curly power port in symbol table exists as both `Bus dc`
        //    and `Label dc{VDD_3V3,GND}` PortInst entries, both with base name `dc`,
        //    use key to dedup and avoid duplicate warnings on the same physical port.
        let mut needs: Vec<(String, String, Vec<String>)> = Vec::new();
        for sub in &self.sub_modules {
            let inst = sub.name.clone();
            for p in &sub.ports {
                if matches!(p.iotype, IOType::Out) {
                    continue;
                }
                let members = port_members(p);
                if members.len() < 2 {
                    continue;
                }
                if !members.iter().any(|m| is_ground_name(m)) {
                    continue;
                }
                let base = port_base_name(&p.name);
                let named = !base.is_empty() && !base.starts_with('@') && !base.starts_with('[');
                let key_name = if base.is_empty() {
                    p.name.clone()
                } else {
                    base.clone()
                };
                let mut prefixes: Vec<String> = Vec::new();
                for m in &members {
                    prefixes.push(format!("{inst}.{m}"));
                    if named {
                        prefixes.push(format!("{inst}.{base}.{m}"));
                    }
                }
                needs.push((inst.clone(), key_name, prefixes));
            }
        }

        // ② Read-only self.connections, collect ports with "no connection hit", dedup by (instance, base name)
        let mut seen: HashSet<(String, String)> = HashSet::new();
        let mut unbound: Vec<(String, String)> = Vec::new();
        for (inst, key_name, prefixes) in &needs {
            let hit = self.connections.iter().any(|c| {
                c.points.iter().any(|pt| {
                    prefixes
                        .iter()
                        .any(|pre| pt.path == *pre || pt.path.starts_with(&format!("{pre}.")))
                })
            });
            if !hit && seen.insert((inst.clone(), key_name.clone())) {
                unbound.push((inst.clone(), key_name.clone()));
            }
        }

        // ③ At this point self has no immutable borrow, record diagnostic with &mut self
        for (inst, key_name) in unbound {
            self.record_warning(
                crate::errcodes::INST_POWER_PORT_UNBOUND,
                crate::errcodes::format_msg(
                    crate::errcodes::INST_POWER_PORT_UNBOUND,
                    &[&inst, &key_name],
                ),
            );
        }
    }

    /// Execute component's "same-name constructor func".
    ///
    /// Convention: func's last segment name == component class's last segment name, that is the constructor
    /// (component `FLASH.sub` ↔ func `sub`).
    /// Body expands inside **parent module self** (peripheral components belong to parent module BOM),
    /// pin references prefixed with instance name (`VCC` → `flash.VCC`); arg names / parent port names not prefixed.
    pub(super) fn run_component_constructor(
        &mut self,
        inst_name: &str,
        comp_def: &Arc<McComponent>,
        args: &[McParamValue],
    ) {
        // Constructor = the one in funcs whose last segment name matches the class's last segment name
        let class_name = comp_def.name.to_string();
        let last = class_name
            .rsplit('.')
            .next()
            .unwrap_or(&class_name)
            .to_string();
        let func = match comp_def.funcs.find(&last) {
            Some(f) => f.clone(),
            None => return, // No same-name constructor func -> no-op (ordinary components like RES/CAP)
        };

        // Formal <- actual arg binding
        let bindings = match McParamBindings::bind(&func.params, args) {
            Ok(b) => b,
            Err(e) => {
                self.record_warning(
                    crate::errcodes::INST_CTOR_PARAM_BIND_FAILED,
                    crate::errcodes::format_msg(
                        crate::errcodes::INST_CTOR_PARAM_BIND_FAILED,
                        &[&last, &inst_name, &format!("{e:?}")],
                    ),
                );
                return;
            }
        };

        // skip set: names appearing in args (parent scope net) + parent module ports -> not prefixed
        let mut skip: HashSet<String> = HashSet::new();
        for b in bindings.iter() {
            if let Some(value) = b.get_value() {
                for e in Self::param_value_to_node_elements(value) {
                    if !e.name.is_empty() {
                        skip.insert(e.name.clone());
                    }
                    for m in &e.member {
                        skip.insert(m.clone());
                    }
                }
            }
        }
        for p in &self.ports {
            skip.insert(p.name.clone());
        }
        skip.insert("GND".to_string());

        // Expand body (constructor func always treated as no-return / Implicit, ignore returns)
        // ── P4-b: isolate anonymous instance entries across body stmts within the same func ──
        // ── Expansion provenance: ComponentCtor (same-name constructor func body) ──
        // Nested under the enclosing declare / construction record when present;
        // body products expand in the current module, tagged with this record.
        let eidx = self.expansion.begin(
            ExpansionKind::ComponentCtor,
            Some(inst_name.to_string()),
            last.clone(),
            self.current_call_site(),
            Self::func_def_site(&func),
        );
        let conn_start = self.connections.len(); // ← P4 backstop start point
        let outer_auto_inst = self.auto_inst_map.clone();
        for (_li, stmt) in func.stmts.iter().enumerate() {
            self.auto_inst_map = outer_auto_inst.clone();
            // Attribute anonymous instances/connections of this body stmt
            // to its exact source stmt in the func's own file. RAII
            // (§7.11(2)): restore happens on every exit.
            self.with_func_stmt(&func, Some(_li), |this| {
                let substituted = Self::substitute_stmt(stmt, &bindings, None);
                let prefixed = Self::prefix_instance_stmt_with_skip(&substituted, inst_name, &skip);
                if let Err(e) = this.process_stmt(&prefixed) {
                    this.record_warning(
                        crate::errcodes::INST_CTOR_BODY_STMT_FAILED,
                        crate::errcodes::format_msg(
                            crate::errcodes::INST_CTOR_BODY_STMT_FAILED,
                            &[&last, &e],
                        ),
                    );
                }
            });
        }
        self.expansion.end(eidx);
        // ── P4 backstop: strip host-synthesized interface endpoints leaked during body processing ──
        // (flash's `flash.in ~ CAP_1.1` / `CAP_1.2 ~ flash.out` etc.)
        self.strip_host_iface_phantoms(inst_name, conn_start);
    }

    /// ── P2: Bare port member inference ──────────────────────────────────────────
    /// For ports with empty `bus_members`, if `self.buses` has accumulated a same-name bus with
    /// >=2 members (from body usage like `PORT{a,b}` / `PORT.x`), project to the port's
    /// declared members, so the parent module's reference to `<sub>.<port>` can expand
    /// by member in expand_port_lanes.
    ///
    /// Example: mcu body `MIC{P,N} -> ...` makes buses["MIC"]=[P,N];
    ///     after final projection PortInst("MIC").bus_members=[P,N];
    ///     parent layer `mic.MIC -> mcu.MIC` both sides expand to [.P, .N] -> zip.
    pub(super) fn infer_bare_port_members_from_buses(&mut self) {
        let inferred: Vec<(usize, Vec<String>)> = self
            .ports
            .iter()
            .enumerate()
            .filter(|(_, p)| p.bus_members.is_empty())
            .filter_map(|(i, p)| {
                self.buses
                    .get(&p.name)
                    .map(|b| b.members.clone())
                    .filter(|m| m.len() >= 2)
                    .map(|m| (i, m))
            })
            .collect();
        for (i, members) in inferred {
            self.ports[i].bus_members = members;
        }
    }

    /// ── §8.9.6.6 step 2: pass1 usage-shape collection ───────────────────────
    /// Scan the module body (declared connection stmts + user-func bodies) for
    /// bus-like uses of `port_name` and return the member union. Called before
    /// port instantiation so a scalar-declared port (e.g. `out spi1`) can be
    /// upgraded to a bus when the body uses it as one.
    ///
    /// Trigger forms (§8.9.6.3):
    ///   1. curly multi-member   `spi1{CS, SCLK, MOSI, MISO}` → `Bus(name, member)`
    ///   2. dotted member access `spi1.CS`                     → `Bus("spi1.CS")`
    ///   3. bracket list         `spi1[1:4]`                   → `List(name, member)`
    ///   5. vector connection    `spi1 -> uC.SPI{CS, ...}`     → sibling operand
    ///      is a >1-member vector; upgrade with the sibling's members.
    /// (Interface binding is a cross-module shape, handled at the
    /// argument-binding layer; `spi1::SPI()` inside a body already carries its
    /// members through the Interface instance's own extraction path.)
    ///
    /// Members from multiple use sites are merged as a union (shape by use);
    /// the port is only upgraded when at least two distinct members appear,
    /// mirroring `extract_port_bus_members`'s >=2-member convention.
    pub(super) fn collect_port_usage_members(&self, port_name: &str) -> Option<Vec<String>> {
        let mut union: Vec<String> = Vec::new();
        let collect = |phrase: &McPhrase, out: &mut Vec<String>| {
            Self::collect_usage_members_in_phrase(phrase, port_name, out);
        };
        for stmt in self.def.stmts.iter() {
            collect(stmt, &mut union);
        }
        for func in self.def.funcs.iter() {
            for stmt in func.stmts.iter() {
                collect(stmt, &mut union);
            }
        }
        if union.len() >= 2 {
            Some(union)
        } else {
            None
        }
    }

    /// Recursive phrase walker used by `collect_port_usage_members`.
    fn collect_usage_members_in_phrase(phrase: &McPhrase, port_name: &str, out: &mut Vec<String>) {
        use McPhrase::*;
        match phrase {
            Endpoint(ep) => Self::collect_usage_members_in_endpoint(ep, port_name, out),
            Series(items, _) | Parallel(items) | Multiple(items) => {
                // §8.9.6.3 form 5: vector-connection trigger — the port is a
                // plain scalar operand and a sibling operand is a >1-member
                // vector (`spi1 -> uC.SPI{CS, SCLK, MOSI, MISO}`), so upgrade
                // with the sibling's members.
                Self::collect_vector_connection_members(items, port_name, out);
                for p in items {
                    Self::collect_usage_members_in_phrase(p, port_name, out);
                }
            }
            Group(g) => {
                for p in &g.opds {
                    Self::collect_usage_members_in_phrase(p, port_name, out);
                }
            }
            Transposed(p) | Member(p, _) => {
                Self::collect_usage_members_in_phrase(p, port_name, out);
            }
            Closure(c) => {
                for stmt in &c.body {
                    Self::collect_usage_members_in_phrase(stmt, port_name, out);
                }
            }
            FuncCall(fc) => {
                if let Some(caller) = &fc.caller {
                    Self::collect_usage_members_in_phrase(caller, port_name, out);
                }
            }
            Lead => {}
        }
    }

    /// §8.9.6.3 form 5: when `port_name` appears as a plain scalar operand of
    /// a connection clause and a sibling operand is a bus/list with >=2
    /// members, the port is upgraded with the sibling's members.
    fn collect_vector_connection_members(
        items: &[McPhrase],
        port_name: &str,
        out: &mut Vec<String>,
    ) {
        // 1. Gather members from sibling bus/list operands (not the port itself).
        let mut sibling: Vec<String> = Vec::new();
        for p in items {
            if let McPhrase::Endpoint(McEndpoint::Single(ir)) = p {
                match &ir.base {
                    McInstance::Bus(bus) => {
                        if bus.name != port_name && bus.member.len() >= 2 {
                            for m in &bus.member {
                                push_union_member(&mut sibling, m);
                            }
                        }
                    }
                    McInstance::List(list) => {
                        if list.name != port_name && list.member.len() >= 2 {
                            for m in &list.member {
                                push_union_member(&mut sibling, m);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        if sibling.len() < 2 {
            return;
        }
        // 2. Only upgrade when the port itself appears as a plain scalar
        //    operand in the same clause.
        let port_is_plain = items.iter().any(|p| {
            matches!(p, McPhrase::Endpoint(McEndpoint::Single(ir))
                if matches!(&ir.base, McInstance::Label(s) if s == port_name))
        });
        if port_is_plain {
            for m in &sibling {
                push_union_member(out, m);
            }
        }
    }

    /// Endpoint walker: extract bus/list member sets that reference `port_name`.
    fn collect_usage_members_in_endpoint(ep: &McEndpoint, port_name: &str, out: &mut Vec<String>) {
        match ep {
            McEndpoint::Single(ir) => {
                match &ir.base {
                    McInstance::Bus(bus) => {
                        if bus.name == port_name {
                            // Curly form `spi1{CS, SCLK, ...}` — members directly.
                            for m in &bus.member {
                                push_union_member(out, m);
                            }
                        } else if let Some(member) = bus
                            .name
                            .strip_prefix(port_name)
                            .and_then(|rest| rest.strip_prefix('.'))
                        {
                            // Dotted form `spi1.CS` → combined label Bus("spi1.CS").
                            // Only a single-level member access upgrades the port;
                            // deeper paths belong to component/sub-module pin access.
                            if !member.is_empty() && !member.contains('.') {
                                push_union_member(out, member);
                            }
                        }
                    }
                    McInstance::List(list) => {
                        if list.name == port_name {
                            // Bracket form `spi1[1:4]` — expanded index members.
                            for m in &list.member {
                                push_union_member(out, m);
                            }
                        }
                    }
                    _ => {}
                }
            }
            McEndpoint::Node { input, output } => {
                for e in input.iter().chain(output.iter()) {
                    Self::collect_usage_members_in_endpoint(e, port_name, out);
                }
            }
            McEndpoint::List(eps) => {
                for e in eps {
                    Self::collect_usage_members_in_endpoint(e, port_name, out);
                }
            }
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Iter-8: Bus member extraction from port declarations
// ────────────────────────────────────────────────────────────────────────────
//
// Consistent with the discrimination logic in `inject_port_member_labels::Step 0`, but only
// extracts the "suitable as dotted expansion lane name" member set——i.e. only returns a
// non-empty member list when the port is declared as a named-prefix N×1 bus:
//
//   ✔ `MIC{P, N}::ADC.DIFF()`     → ["P", "N"]      (Interface + curly)
//   ✔ `dc{VDD_3V3, GND}::DC()`    → ["VDD_3V3","GND"] (Interface + curly)
//   ✔ `name{A, B}` (Bus)          → ["A", "B"]      (curly without interface)
//   ✔ `GPIO[1:2]` (List named)     → ["1", "2"]
//   ✔ `[VDD_3V3, GND]::DC(3.3V)`  → ["VDD_3V3","GND"] (Interface + pure bracket)
//
//   ✘ `[A, B]` anonymous List     → []   (port has no meaningful "prefix name",
//                                          can't form endpoint paths like
//                                          `port.A` / `port.B`; left to
//                                          inject_port_member_labels's
//                                          bare-label injection path)
//   ✘ Single member (`[X]` / `name{Y}`) → []   (1×1 port is essentially a bare scalar,
//                                          expanding to `port.Y` doesn't change net
//                                          topology; to avoid accidentally activating
//                                          downstream lane-splitting code paths,
//                                          only expand for >=2 members)
//
// Returning empty `Vec` means the port is treated as a bare scalar.
//
// §11 (eval.md): members are returned in **source declaration order** (vector
// order). No alphabetical normalization — downstream lane pairing aligns by
// member name and falls back to positional zip, so declaration order is the
// single source of truth.
fn extract_port_bus_members(inst: &McInstance, _port_name: &str) -> Vec<String> {
    match inst {
        // Named List: `GPIO[1:2]`
        McInstance::List(list) if !list.member.is_empty() => {
            let is_anonymous = list.name.is_empty() || list.name.starts_with('@');
            if is_anonymous {
                // Anonymous `[A, B]`: no valid prefix, don't expand
                Vec::new()
            } else if list.member.len() >= 2 {
                list.member.clone()
            } else {
                Vec::new()
            }
        }

        // Curly: `name{A, B}`
        McInstance::Bus(bus) if bus.member.len() >= 2 => bus.member.clone(),

        // Interface: `[A, B]::DC()` or `dc{A, B}::DC()` or `MIC{P, N}::ADC.DIFF()`
        //
        // ── S1 Bug D fix (Part 2) ─────────────────────────────────────
        // **Bare interface ports** like `io SPI` (no curly members, e.g. `io SPI`
        // not `io SPI{CS, SCLK, MISO, MOSI}`) have no member info on iface.name
        // (as_bus / list_members both empty). But Mc2Interface.base is the full
        // McInterface definition, its pins.pins BTreeMap's value (McPin)'s
        // `names[0]` is the original declared pin name (e.g. SPI: CS/SCLK/MISO/
        // MOSI in BTreeMap pinid order = declaration order for numeric pinids).
        //
        // Previously falling back to Vec::new() leaves expand_port_lanes without lanes ->
        // cross sub-module boundary degrades to scalar (1 point) -> 1-vs-N broadcast shorts N
        // physical pins into the same net (S1 SPI four-wire short).
        //
        // Fix: after name-based extraction fails, fall back to iface.base.pins to get names[0]
        // sequence as bus_members. This is consistent with the logic used by
        // derive_interface_subnames in components/mc_pins/mod.rs (same source = same order).
        McInstance::Interface(iface) => {
            if let Some((_prefix, members)) = iface.name.as_bus() {
                if members.len() >= 2 {
                    return members;
                }
            }
            if let Some(members) = iface.name.list_members() {
                if members.len() >= 2 {
                    return members;
                }
            }
            // Fallback: take member names from base McInterface.pins in source
            // declaration order (§11.1 — the member vector order never comes
            // from the BTreeMap pinid key order; `member_names()` reads the
            // recorded declaration order).
            // Applies to BOTH bare interface ports (e.g. `io SPI`) and scalar named
            // ports with interface annotation (e.g. `V3V3::DC(3.3V)`, `in vin::DC(5V)`).
            // The interface type defines the members (e.g. DC → VCC, GND), and the port
            // name is just a label — the electrical members come from the interface type.
            let pin_names: Vec<String> = iface.base.pins.member_names();
            if pin_names.len() >= 2 {
                return pin_names;
            }
            Vec::new()
        }

        _ => Vec::new(),
    }
}

/// Append `m` to the union vector if not already present (member order = first
/// appearance order across all use sites).
fn push_union_member(out: &mut Vec<String>, m: &str) {
    if m.is_empty() {
        return;
    }
    if !out.iter().any(|x| x == m) {
        out.push(m.to_string());
    }
}

/// Extract voltage token from a name (uppercase normalize):
///   "V3V3"->"3V3", "VDD_3V3"->"3V3", "VCC_1V2"->"1V2", "V5V"->"5V", "VDD_CORE"->None
/// Rule: match digit+ 'V' (+digit)? fragment.
fn voltage_token(name: &str) -> Option<String> {
    let b = name.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() {
            let start = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            if i < b.len() && (b[i] == b'V' || b[i] == b'v') {
                i += 1;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                return Some(name[start..i].to_uppercase());
            }
        } else {
            i += 1;
        }
    }
    None
}

/// Lightweight ground name recognition (distinguish power rail vs ground at binding; consistent with
/// the ground subset of mc_net::looks_like_power_rail, to avoid cross-layer imports).
fn is_ground_name(s: &str) -> bool {
    let u = s.to_uppercase();
    matches!(u.as_str(), "GND" | "VSS" | "AGND" | "DGND" | "PGND")
        || u.starts_with("GND")
        || u.starts_with("VSS")
}

/// "[VDD_3V3, GND]" / "[VCC_1V2,GND]" -> ["VDD_3V3","GND"]; non-bracket -> []
fn parse_bracket_members(name: &str) -> Vec<String> {
    let s = name.trim();
    if !(s.starts_with('[') && s.ends_with(']')) {
        return Vec::new();
    }
    s[1..s.len() - 1]
        .split(',')
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .collect()
}

/// Get port base name: strip `{...}` / `[...]` suffix.
///   `dc{VDD_3V3,GND}` -> `dc`;  `vin` -> `vin`;
///   `[VDD_3V3,GND]`   -> ``  (starting with `[`/`{` = anonymous port, no base name).
/// Consistent with `inject_port_member_labels`'s anonymous vs named distinction:
///   named ports have both bare(`MEMBER`) and dotted(`base.MEMBER`) labels,
///   anonymous bracket ports only have bare.
fn port_base_name(name: &str) -> String {
    let s = name.trim();
    let cut = match (s.find('{'), s.find('[')) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };
    match cut {
        Some(0) => String::new(), // Starting with `[`/`{` -> anonymous
        Some(i) => s[..i].trim().to_string(),
        None => s.trim().to_string(),
    }
}

/// Get port members (three sources, priority high to low):
///   ① `port.bus_members` non-empty -> use it (extracted at instantiation, most authoritative);
///   ② name in `[...]` form -> parse_bracket_members;
///   ③ name contains `{...}` -> take curly-brace contents split by comma.
/// Scalar ports (no members) return empty Vec.
fn port_members(port: &PortInst) -> Vec<String> {
    if !port.bus_members.is_empty() {
        return port.bus_members.clone();
    }
    let bracket = parse_bracket_members(&port.name);
    if !bracket.is_empty() {
        return bracket;
    }
    let s = port.name.as_str();
    if let (Some(o), Some(c)) = (s.find('{'), s.rfind('}')) {
        if c > o + 1 {
            return s[o + 1..c]
                .split(',')
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .collect();
        }
    }
    Vec::new()
}
