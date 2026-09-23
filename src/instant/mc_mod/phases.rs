// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Phase entry points
//!
//! - Phase 1: Interface instantiation (ports + Iter-5.B member label injection)
//! - Phase 3: Declared instance instantiation (components / sub-modules / labels)
//! - Phase 4: Connection stmt processing entry

use super::matching::{pair_members_to_lanes, parse_bracket_members};
use super::FailedRecord;
use super::{InstantiationBuilder, McModuleInst};
use crate::instant::mc_comp::McComponentInst;
use crate::instant::mc_net::{canonicalize_path, ConnectionInst, InstError, NetPoint, PortInst};
use crate::instant::provenance::ExpansionKind;
use crate::semantic::basic::mc_ids::IdsSegment;
use crate::semantic::basic::mc_param::{McParamBindings, McParamValue};
use crate::semantic::basic::mc_param_type::{McIoTy, McParamTypeKind};
use crate::semantic::basic::mc_paramd::McParamDeclareKind;
use crate::semantic::basic::mc_uval::McUnit;
use crate::semantic::common::{ConnDir, ConnOp, IOType};
use crate::semantic::component::McComponent;
use crate::semantic::component::mc_pins::McPins;
use crate::semantic::mc_ifs::Mc2Interface;
use crate::semantic::mc_inst::McInstance;
use crate::semantic::module::McModule;
use crate::semantic::nc_pin::{NcPinKind, NcPinSpec};
use crate::semantic::validation::ledger::{self, LedgerAction, LedgerEntry, LedgerKind};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

impl InstantiationBuilder {
    // Phase 1: Interface instantiation
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
    // `instantiate_interface` must push the port name `"[VDD_3V3,GND]"` into
    // `self.ports` **and register VDD_3V3 / GND as independent symbols**
    // in the sub-module's label namespace. Otherwise:
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
    // Without the injection `main.dcdc.VDD_3V3` does not exist, so the
    // corresponding endpoint in the parent's V3V3 net is empty and the entire
    // POWER chain is electrically disconnected.
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
    // `dcdc.GND` which again faces the "parent can't find label in sub-module" old problem — core
    // goal lost.
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
    //   * `psnk dc{VDD_3V3, GND}`         → Bus               ✔
    //   * `psnk [VDD_3V3, GND]`           → List (@N anonymous)    ✔
    //   * `psnk GPIO[1:2]`                → List (named prefix)    ✔ (bus+dotted only)
    //   * `psnk DC1{VDD, GND}`            → Bus               ✔
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
        // Hoisted once: every point this call builds carries the same source site.
        let site = self.construction_site();
        // First clone port list to release immutable borrow of self.def
        // Loop body needs &mut self (labels / buses write), so can't run
        // directly during iter_with_iotype() borrow.
        let mut items: Vec<(String, IOType, McInstance)> = self
            .def
            .insts
            .iter_with_iotype()
            .map(|(k, (io, inst))| (k.to_string(), io.clone(), inst.clone()))
            .collect();

        // ★ CIMP §1 U119: the port table is the module's **written order**, not
        // the name order of `insts` (a `BTreeMap`). Presentation (the drawing,
        // the port listings) and positional pairing (the `② position fallback`
        // in `bind_actual_args_to_ports`, which reads the port slice this loop
        // builds) both follow the order the author wrote. Non-port items keep
        // the name order they had: a stable sort moves only the ports, and the
        // loop skips the rest anyway.
        //
        // The member ledger follows this same order, deliberately: a module
        // port's `DefMemberId` is its ordinal in the list the author wrote, the
        // rule the component side has always had (`McPins.decl_order`). The two
        // faces -- the drawn port list and `PointId`'s `N<node>:<m>` half --
        // therefore agree, and neither is a name-order reading.
        {
            let rank: HashMap<&str, usize> = self
                .def
                .insts
                .iter_ports_in_decl_order()
                .enumerate()
                .map(|(i, (name, _))| (name, i))
                .collect();
            items
                .sort_by_key(|(name, _, _)| rank.get(name.as_str()).copied().unwrap_or(usize::MAX));
        }

        for (port_name, iotype, inst) in &items {
            // Bug fix ①
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

            // Bug fix ②
            // Only items with a non-None IOType are real ports.
            // Label/Bus/List items with IOType::None are internal body declarations
            // (e.g. `VCC`/`Vin` power labels in `VCC -> Q1 -> Vin`).
            // They must NOT be pushed as module ports, otherwise viz sees them as
            // module ports instead of internal labels.
            //
            // ── P2-4 exception ──
            // Interface-type items in the module signature (e.g. `psnk [VDD_3V3,GND]::DC(3.3V)`)
            // have IOType::Power (or IOType::None on the removed no-direction
            // sugar, E3055) but ARE real ports. They must be added to self.ports
            // so that `bind_actual_args_to_ports` can find them.
            let is_interface_port = matches!(inst, McInstance::Interface(_));
            if matches!(iotype, IOType::None) && !is_interface_port {
                continue;
            }

            // 1. When creating PortInst, extract bus_members according to port form
            // —— Iter-8: let N×1 bus ports expand according to declaration during endpoint
            // resolution.
            //
            //    ★ Authoritative declared shape (no usage auto-expansion): the
            //    member set comes only from the port's own declaration — a bare
            //    `io X` stays a scalar 1×1 port and body member access on it is a
            //    Pass1 error (E3183). The old §8.9.6.6 step-2 "scalar → bus
            //    upgrade by usage" is removed.
            //
            //    ★ A scalar interface-type port declaration (`in vin::DC(5V)`)
            //    is DEFINED, not sugar: for an interface-typed declareb the
            //    interface's own sub-pin definitions are brought over by
            //    default, so extract_port_bus_members expands the declared
            //    port from the interface's pin set. No phantom lane is
            //    synthesized beyond what the interface itself declares.
            let bus_members = extract_port_bus_members(inst, port_name);
            let inject_inst = inst.clone();

            // Model-A connection-point DC pair (classification-retirement-design
            // §4, C full capture). The pair is a property of the DECLARATION, not
            // of the spelling: a `::DC` contract with two faces declares a supply
            // face and a return face, and the faces sit at the declared positions.
            // Written members (`[hot, ret]` / `base{hot, ret}`) name them; a scalar
            // `x::DC(v)` brings the same two faces over from the interface's own
            // pin table, in the same order — so it carries the same pair. Reading
            // only the written form made identity depend on which of the two
            // equivalent spellings the author happened to use.
            // Positional decode: 1st member=hot (supply side), 2nd=ret (declared
            // return / ground side); names are copper labels only (a member is ret
            // because it sits second, not because it is named GND) — no-hardcoding.
            let dc_pair: Option<(String, String)> = match inst {
                McInstance::Interface(iface) if iface.base_name() == "DC" => {
                    // Written members: curly `base{hot, ret}` (as_bus) or
                    // bracket `[hot, ret]` (list_members).
                    let written = iface
                        .name
                        .as_bus()
                        .map(|(_prefix, members)| members)
                        .or_else(|| iface.name.list_members());
                    match written {
                        Some(members) if members.len() == 2 => {
                            Some((members[0].clone(), members[1].clone()))
                        }
                        None => {
                            // Scalar `x::DC(v)`: not written, so the pair comes
                            // from `iface_ordinal_member_names` — the same source
                            // `extract_port_bus_members` expands the members
                            // from, hence the same order.
                            let faces = iface_ordinal_member_names(iface);
                            if faces.len() == 2 {
                                Some((faces[0].clone(), faces[1].clone()))
                            } else {
                                None
                            }
                        }
                        Some(_) => None,
                    }
                }
                _ => None,
            };

            // Interface-declared differential pairs (diff-pair-design.md,
            // ruled 2026-09-23). The interface's member rows carry `@pair`
            // tags; the two rows sharing a group are the faces of one pair,
            // in member order. Like the DC pair, the pair is a property of
            // the DECLARATION: a port whose interface declares none carries
            // none, and no spelling of a net name is ever consulted. The
            // rows read are the rows the adoption expanded — role rows when
            // the adopted role wrote its own, else the conductor view
            // (U205②), so a declared pair always meets its consumers.
            let diff_pair: Vec<(String, String)> = match inst {
                McInstance::Interface(iface) => {
                    read_iface_diff_groups(iface_adopted_pin_table(iface))
                }
                _ => Vec::new(),
            };

            // Phase C1: intern the port's canonical path before it enters the
            // module's port list (its node id lives in the circuit registry).
            let port_path = self.child_path(port_name);
            let port_id = self.identity_mut().intern(&port_path);
            let port = PortInst::with_members(port_name, iotype.clone(), bus_members.clone());
            let mut port = port;
            port.dc_pair = dc_pair;
            port.diff_pair = diff_pair;
            port.volt = match inst {
                McInstance::Interface(iface) => declared_volt_of_params(&iface.params),
                _ => None,
            };
            port.node_id = Some(port_id);
            // ★ U217 (ac-interface-design.md §5): the AC mains face's declared
            // region nominal rides the port the same way `volt` does for DC —
            // decoded once, from the row's own `::AC.*(...)` arguments, for
            // exactly the rows whose interface family is `AC` (the dotted
            // variants included). The empty form `::AC.1P()` decodes to
            // `None`/`None`: a region-neutral face states no nominal, which is
            // a real answer the AC gates stay silent on.
            port.ac_face = match inst {
                McInstance::Interface(iface) if crate::instant::insttab::is_ac_family(&iface.base_name()) => {
                    let crate::instant::insttab::AcFaceCarryVolts { volts, hz } =
                        crate::instant::insttab::declared_ac_face_of_params(&iface.params);
                    Some(crate::instant::mc_net::AcPortFace { volts, hz })
                }
                _ => None,
            };
            // Phase C S3: lay the port's arena node down beside the Vec push
            // (the arena is the structural store; `ports` stays on the tree).
            self.append_port_arena(&port);
            self.ports.push(port);

            // 2. Iter-5.B —— inject member labels / register prefix bus according to port form.
            self.inject_port_member_labels(iotype, &inject_inst);
        }

        // ── P2-4: process interface-type parameters from module signature ──
        // Module signature params like `[VDD_3V3,GND]::DC(3.3V)` live in `def.params`,
        // not `def.insts`. Without this, `bind_actual_args_to_ports` can't find them,
        // and parent modules can't pass bus arguments to submodule interface ports.
        // The param list is cloned so the `self.ports` / `self.labels` writes below
        // do not fight the `self.def` read (both go through the builder deref).
        //
        // A direction-word signature port (`psnk [VDD_3V3,GND]::DC(3.3V)`) is an
        // exception: it already entered `self.ports` in the loop above, because
        // the direction word sits on the IOTYPE clause that `parse_declare`
        // consumes. `def.params` carries the same declaration so arity and
        // goto-def can see it; materializing it a second time here would push a
        // duplicate PortInst — same name, direction-less io_type.
        //
        // The key is every name the loop above actually materialized, under the
        // same two guards (not a component/sub-module declaration; not a
        // direction-less non-interface item).
        let already_registered: std::collections::HashSet<&str> = items
            .iter()
            .filter(|(_, io, inst)| {
                !matches!(inst, McInstance::Component(_) | McInstance::Module(_))
                    && !(matches!(io, IOType::None) && !matches!(inst, McInstance::Interface(_)))
            })
            .map(|(name, _, _)| name.as_str())
            .collect();
        let def_params = self.def.params.clone();
        for pd in def_params.iter() {
            let is_interface_port = matches!(
                pd.param_type.kind,
                McParamTypeKind::Interface { .. } | McParamTypeKind::InterfaceWithRole { .. }
            );
            if !is_interface_port {
                continue;
            }

            let port_name = pd.get_primary_name().unwrap_or_else(|| pd.display_name());
            if already_registered.contains(port_name.as_str()) {
                continue;
            }
            let iotype = match pd.param_type.direction {
                Some(McIoTy::Input) => IOType::In,
                Some(McIoTy::Output) => IOType::Out,
                Some(McIoTy::InOut) => IOType::InOut,
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

            // Phase C1: intern the port's canonical path (interface-signature
            // ports enter the circuit registry like declared ones).
            let port_path = self.child_path(&port_name);
            let port_id = self.identity_mut().intern(&port_path);
            let port = PortInst::with_members(&port_name, iotype.clone(), bus_members.clone());
            let mut port = port;
            port.volt = match &pd.param_type.kind {
                McParamTypeKind::Interface { params, .. } => declared_volt_of_texts(params),
                _ => None,
            };
            port.node_id = Some(port_id);
            // Phase C S3: interface-signature ports enter the arena too.
            self.append_port_arena(&port);
            self.ports.push(port);

            // Inject member labels so that connection stmts can reference them
            for member in &bus_members {
                self.labels.insert(
                    member.clone(),
                    NetPoint::new(member, iotype.clone(), site.clone()).with_member_name(member),
                );
            }
        }

        // T4 (defspace-id-core-plan M1b): merge the just-finalized port table
        // into the module def's registry-owned port ledger — the port list is
        // built here (module ports are an instantiation product, not a parse
        // artifact), so this is the single point where the ledger learns the
        // def's ports. The merge is by name (a re-parse that inserts a port
        // mid-declaration never shifts the later ports' member ids); module
        // trees whose def is not a registered identity (func-expanded
        // synthetic modules, empty def uri) are skipped and keep the
        // positional ordinal in the lane layer.
        if !self.def_uri.is_empty() {
            // ★ CIMP §1 U119: the feed carries the port list's own order, which
            // is now the **written** order -- the same rule the component side
            // has always had (`McPins.decl_order` is what `component_member_seq`
            // numbers from). A module port's `DefMemberId` is therefore its
            // ordinal in the list the author wrote, and it agrees with the
            // drawn port list this same list becomes. Recorded in the
            // organization-units design draft (§6.2, CIMP §1 U119).
            let ports: Vec<(String, String)> = self
                .ports
                .iter()
                .map(|p| (p.name.clone(), format!("{:?}", p.iotype)))
                .collect();
            let sn =
                crate::semantic::common::McSpaceName::new(&self.def.name, self.def_uri.clone());
            crate::db::defregistry::sync_module_ports(&sn, &ports);
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
        // Hoisted: the `or_insert_with` closures below hold `&mut self.labels`,
        // so the site cannot be reached through `self` inside them.
        let site = self.construction_site();
        // Step 0: Calculate which members to inject according to port form
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
                    // extract members in connection-ordinal order — the declared
                    // role's table when present, else the interface base table
                    // (§11.1, U137) — register prefix bus with dotted labels
                    // (e.g. V3V3.VCC, V3V3.GND).
                    let pin_names: Vec<String> = iface_ordinal_member_names(iface);
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

        // If both member sets are empty (usually Case 1 named but no real members), return
        // directly.
        if bare_members.is_empty() && dotted_members.is_empty() {
            return;
        }

        // Step A1: Inject bare member labels
        //
        // Use entry().or_insert_with(...) instead of insert(...): if same-name
        // label has already been registered by other paths (explicit declaration, earlier ports,
        // build helpers, etc.),
        // keep existing entry, avoid silent overwrite.
        for m in &bare_members {
            if m.is_empty() {
                continue;
            }
            self.labels.entry(m.clone()).or_insert_with(|| {
                NetPoint::new(m, iotype.clone(), site.clone()).with_member_name(m)
            });
        }

        // Step A2: curly form additional register prefix bus + dotted label
        //
        // This is not a "bridge", just declaring "`dc` is a bus with VDD_3V3 / GND members",
        // so that `node_to_netpoint` step 2.3 / step 3 can resolve body stmt `dc.VDD_3V3` reference
        // by bus semantics. Does not append to `self.connections`, does not cause any union-find
        // merges.
        if let Some(prefix) = dotted_prefix.as_ref() {
            if !prefix.is_empty() && !dotted_members.is_empty() {
                // ensure_bus does incremental merge, ignore Err — current implementation always
                // returns Ok
                let _ = self.ensure_bus(prefix, &dotted_members);

                for m in &dotted_members {
                    if m.is_empty() {
                        continue;
                    }
                    let dotted = format!("{prefix}.{m}");
                    self.labels.entry(dotted.clone()).or_insert_with(|| {
                        NetPoint::new(&dotted, iotype.clone(), site.clone()).with_member_name(m)
                    });
                }
            }
        }

        // ── Strict DC rail identity (user-confirmed, GENERAL) ──
        // A port's ground member (e.g. `vin.GND`) belongs to that rail only.
        // It is NOT auto-bridged to the module-level bare `GND` label: within a
        // module, different DC rails may carry different grounds, and the bare
        // `GND` reference stays an independent net that the author wires
        // explicitly. Merging happens only through real wiring ties (shared
        // component ground pins, explicit `X.GND -> GND` connections).
    }

    // Phase 3: Declared instance instantiation

    pub(super) fn instantiate_declarations_resilient(&mut self) {
        // Hoisted: the `or_insert_with` closures below hold `&mut self.labels`,
        // so the site cannot be reached through `self` inside them.
        let site = self.construction_site();
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
                    let mut inst = if c.params.is_empty() {
                        // No arguments: plain instance. An NC-marked declaration
                        // with no parameter list keeps the not-connected flag.
                        if c.nc {
                            McComponentInst::with_nc(&c.name.to_string(), c.base.clone(), &c.params)
                        } else {
                            // An empty argument list is still a resolved
                            // parameter list: a formal whose declaration
                            // records a default contributes it, which is what
                            // the instance's conditional blocks read (CIMP
                            // U54). A genuinely required formal left unbound
                            // binds nothing rather than failing the list.
                            McComponentInst::with_params(&c.name.to_string(), c.base.clone(), &[])
                                .unwrap_or_else(|_| {
                                    McComponentInst::new(&c.name.to_string(), c.base.clone())
                                })
                        }
                    } else {
                        // NC rule
                        // An NC-marked instance still binds its remaining
                        // arguments: with_params strips NC from arity, binds
                        // the rest and sets nc=true. NC occupies no slot and
                        // never covers a missing required parameter. Binding
                        // failure is a hard error — the argument list does not
                        // match the class signature (unknown named arg, excess
                        // arg, type-mismatched arg, or a genuinely missing
                        // required param). The instance is skipped, but the
                        // reason is reported so the author sees it.
                        match McComponentInst::with_params(
                            &c.name.to_string(),
                            c.base.clone(),
                            &c.params,
                        ) {
                            Ok(inst) => inst,
                            Err(e) => {
                                let reason = format!("{e}");
                                let message = crate::errcodes::format_msg(
                                    crate::errcodes::INST_PARAM_BIND_FAILED,
                                    &[&c.name.to_string(), &c.base.name.to_string(), &reason],
                                );
                                // Pass1 already reports this same bind failure at
                                // the instance's own declaration (E4176); a second
                                // report here would only duplicate it, and with no
                                // span in scope it collapsed to row 1. Report only
                                // when Pass1 did not, anchored at the declaration.
                                match self.def.insts.get_port_span(&c.name.to_string()) {
                                    Some(r) => {
                                        if !crate::db::diagnostic::diagnostic::has_code_at(
                                            crate::errcodes::INST_PARAM_BIND_FAILED,
                                            &self.def_uri,
                                            r.start as u32,
                                        ) {
                                            self.record_error_at(
                                                crate::errcodes::INST_PARAM_BIND_FAILED,
                                                message,
                                                self.def_uri.clone(),
                                                r.start as u32,
                                            );
                                        }
                                    }
                                    None => self.record_error(
                                        crate::errcodes::INST_PARAM_BIND_FAILED,
                                        message,
                                    ),
                                }
                                mcc_dbg!(
                                    "inst::mod",
                                    "[ERROR] Failed to instantiate component '{}' (class '{}'): {}",
                                    c.name,
                                    c.base.name,
                                    reason
                                );
                                self.failed_classes.insert(c.base.name.to_string());
                                let stmt_line = self
                                    .current_stmt_span
                                    .as_ref()
                                    .map(|s| (s.offset / 1000) as usize);
                                let module_name = self.name.clone();
                                self.failed_records.push(FailedRecord {
                                    module: module_name,
                                    src_line: stmt_line,
                                    component_name: c.name.to_string(),
                                    class_name: c.base.name.to_string(),
                                    reason,
                                });
                                self.expansion.end(eidx);
                                continue;
                            }
                        }
                    };
                    // U39: a conditional block of the class may have failed to
                    // evaluate against the arguments this declaration binds
                    // (`CH ch1("WIDE")` where the condition compares the formal
                    // with a voltage). The failing condition is the class file's
                    // syntax, so the diagnostic is anchored here instead — at
                    // the declaration that supplied the argument it could not
                    // use.
                    if !inst.cond_eval_errors.is_empty() {
                        let anchor = self
                            .def
                            .insts
                            .get_port_span(&c.name.to_string())
                            .map(|r| r.start as u32);
                        for err in &inst.cond_eval_errors {
                            let (code, message) = (err.code(), err.message());
                            match anchor {
                                Some(pos) => {
                                    if !crate::db::diagnostic::diagnostic::has_code_at(
                                        code,
                                        &self.def_uri,
                                        pos,
                                    ) {
                                        self.record_error_at(
                                            code,
                                            message,
                                            self.def_uri.clone(),
                                            pos,
                                        );
                                    }
                                }
                                None => self.record_error(code, message),
                            }
                        }
                    }
                    // U212: `error()` clauses the class fired while this
                    // instance was built. Anchored at the declaration that
                    // supplied the arguments, like the U39 block above — the
                    // clause's own node is long gone with the class AST.
                    if !inst.cond_author_errors.is_empty() {
                        let anchor = self
                            .def
                            .insts
                            .get_port_span(&c.name.to_string())
                            .map(|r| r.start as u32);
                        for (code, message) in &inst.cond_author_errors {
                            match anchor {
                                Some(pos) => {
                                    if !crate::db::diagnostic::diagnostic::has_code_at(
                                        *code,
                                        &self.def_uri,
                                        pos,
                                    ) {
                                        self.record_error_at(
                                            *code,
                                            message.clone(),
                                            self.def_uri.clone(),
                                            pos,
                                        );
                                    }
                                }
                                None => self.record_error(*code, message.clone()),
                            }
                        }
                    }
                    // ★ U48: the declaration's `@ncpin(…)` marker becomes this
                    // instance's marked pin-id set. Resolved after the instance
                    // is built (so conditional / dynamic pins are already in
                    // `pins`) and before it is added (so the table can never
                    // see the instance without its marker).
                    let nc_pins = self.resolve_component_nc_pins(&inst, &c.nc_pins);
                    inst.set_nc_pins(nc_pins);
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
                    // Phase C1: intern into the circuit registry under the full path
                    // (`{parent}.{inst_name}`), so this sub-module and its products
                    // carry circuit-global node ids.
                    let sub_path = self.child_path(&inst_name);
                    // Phase D: hand the shared circuit-wide net-table store
                    // down so the sub-module's frozen table lands in the same
                    // store the parent reads for ground-tie propagation. Phase C
                    // S3: likewise share the construction arena + instance
                    // store so the sub-module's products append to the same
                    // structures. Clone the `Rc`s before taking the mutable
                    // identity borrow to avoid a borrow conflict.
                    let net_store = self.net_store.clone();
                    let arena = self.arena.clone();
                    let store = self.store.clone();
                    let identity = self.identity_mut();
                    if let Err(e) =
                        inst.instantiate_in_scope(identity, &sub_path, net_store, arena, store)
                    {
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
                        let sub_def = inst.def.clone();
                        self.bind_actual_args_to_ports(&inst_name, &sub_def, &ports, &m.args);
                    }
                    // ★ U48: resolve the declaration's `@ncpin(…)` marker into
                    // the sub-module's registered port suffixes. After
                    // `instantiate_in_scope` (the ports exist only then), before
                    // `add_submodule` (so the frozen table sees them), and after
                    // the arg binding above (which does not touch port identity).
                    let nc_ports = self.resolve_module_nc_ports(&inst, &m.nc_pins);
                    inst.set_nc_ports(nc_ports);
                    self.merge_diagnostics_from(&inst);
                    self.add_submodule(inst);
                    self.expansion.end(eidx);
                }
                McInstance::Bus(label) => {
                    // Iter-5.B cooperation point
                    // Keep old logic of treating McInstance::Bus as label name injection.
                    // Use entry().or_insert to avoid overwriting the more precise NetPoint
                    // injected by phase 1 using port's iotype.
                    self.labels
                        .entry(label.name.clone())
                        .or_insert_with(|| NetPoint::new(&label.name, IOType::None, site.clone()));
                }
                _ => {}
            }
        }

        // §11.2: build module-level vector grouping nodes
        // `self.def.insts` carries the `base -> ordered member names` map
        // recorded at parse_declare; the flat member instances were just
        // materialized into `self.components`. Promote each multi-member group
        // to an `McVectorInst` (module-level: empty prefix). Arc clone keeps
        // the `&McInstances` borrow off `self` so `&mut self` is available.
        let def = self.def.clone();
        self.materialize_vector_groups(&def.insts, "");
    }

    // ── U48: instance-site per-pin NC marker (`@ncpin(…)`) ──
    //
    // The marker is carried down from the declaration as written AST operands
    // (`Mc2Component.nc_pins` / `Mc2Module.nc_pins`) and resolved *here*, once
    // per instance, into the exact strings the flat table registers: pin ids
    // for a component, port path suffixes for a sub-module. Everything
    // downstream is then a plain set lookup — no second parsing, no name
    // heuristics, and no way for the two sides to drift.

    /// Compile a component declaration's `@ncpin(…)` marker into the pin ids
    /// this instance actually carries.
    ///
    /// Every written pin name goes through [`super::points::declared_pin_id`] —
    /// the one authority for "written pin identity → physical pin id" (raw pin
    /// id, declared pinname, conditional alias), the same one the connection
    /// face reads — so `@ncpin(VDD)` and `@ncpin(5)` land on the same pin. A
    /// written range (`1:3`, both ends inclusive) selects numeric pin ids.
    /// Names that resolve to nothing are reported (E3179, with the pins the
    /// instance does have) and mark nothing.
    fn resolve_component_nc_pins(
        &mut self,
        comp: &McComponentInst,
        specs: &[NcPinSpec],
    ) -> BTreeSet<String> {
        let mut marked = BTreeSet::new();
        for spec in specs {
            match &spec.kind {
                NcPinKind::Names(names) => {
                    let mut missing: Vec<String> = Vec::new();
                    for name in names {
                        match super::points::declared_pin_id(comp, name) {
                            Some(id) => {
                                marked.insert(id);
                            }
                            None => missing.push(name.clone()),
                        }
                    }
                    if !missing.is_empty() {
                        // Sorted so the message is deterministic: `comp.pins` is
                        // a HashMap and its key order is not.
                        let mut available: Vec<&str> =
                            comp.pins.keys().map(|k| k.as_str()).collect();
                        available.sort();
                        self.report_nc_operand_miss(
                            crate::errcodes::COMPONENT_PIN_NOT_FOUND,
                            &missing.join(", "),
                            &comp.name,
                            &comp.def.uri,
                            spec,
                            &available,
                        );
                    }
                }
                NcPinKind::Range(from, to) => {
                    let before = marked.len();
                    for id in comp.pins.keys() {
                        if id.parse::<i64>().is_ok_and(|n| *from <= n && n <= *to) {
                            marked.insert(id.clone());
                        }
                    }
                    if marked.len() == before {
                        let mut available: Vec<&str> =
                            comp.pins.keys().map(|k| k.as_str()).collect();
                        available.sort();
                        self.report_nc_operand_miss(
                            crate::errcodes::COMPONENT_PIN_NOT_FOUND,
                            &format!("{from}:{to}"),
                            &comp.name,
                            &comp.def.uri,
                            spec,
                            &available,
                        );
                    }
                }
            }
        }
        marked
    }

    /// Compile a sub-module declaration's `@ncpin(…)` marker into the port path
    /// suffixes this instance's flat rows answer to.
    ///
    /// The strings stored are the registered ones ([`PortInst::path_suffixes`]),
    /// never the written spelling, so the table side stays a pure lookup.
    /// Naming a port's own header or bracket alias means the whole port,
    /// members included (see [`nc_port_hits`] for why); naming a member means
    /// that member. Names that resolve to nothing are reported (E3175, with the
    /// ports the sub-module does have) and mark nothing.
    fn resolve_module_nc_ports(
        &mut self,
        sub: &McModuleInst,
        specs: &[NcPinSpec],
    ) -> BTreeSet<String> {
        let mut marked = BTreeSet::new();
        for spec in specs {
            match &spec.kind {
                NcPinKind::Names(names) => {
                    let mut missing: Vec<String> = Vec::new();
                    for name in names {
                        let mut hit = false;
                        for port in &sub.ports {
                            if let Some(suffixes) = nc_port_hits(name, port) {
                                marked.extend(suffixes);
                                hit = true;
                            }
                        }
                        if !hit {
                            missing.push(name.clone());
                        }
                    }
                    if !missing.is_empty() {
                        self.report_nc_operand_miss(
                            crate::errcodes::MODULE_PORT_NOT_FOUND,
                            &missing.join(", "),
                            &sub.name,
                            &sub.def_uri,
                            spec,
                            &sub.ports
                                .iter()
                                .map(|p| p.name.as_str())
                                .collect::<Vec<_>>(),
                        );
                    }
                }
                NcPinKind::Range(from, to) => {
                    let before = marked.len();
                    for port in &sub.ports {
                        marked.extend(nc_port_range_hits(*from, *to, port));
                    }
                    if marked.len() == before {
                        self.report_nc_operand_miss(
                            crate::errcodes::MODULE_PORT_NOT_FOUND,
                            &format!("{from}:{to}"),
                            &sub.name,
                            &sub.def_uri,
                            spec,
                            &sub.ports
                                .iter()
                                .map(|p| p.name.as_str())
                                .collect::<Vec<_>>(),
                        );
                    }
                }
            }
        }
        marked
    }

    /// Report one `@ncpin(…)` operand that names nothing on its instance.
    ///
    /// Anchored at the operand itself — the wrong name is written there, not at
    /// the instance — and deduplicated by position, so a module instantiated
    /// twice reports the same written mistake once. Warning level, like every
    /// other unresolved reference of this family.
    fn report_nc_operand_miss(
        &mut self,
        code: u32,
        operand: &str,
        owner: &str,
        uri: &crate::McURI,
        spec: &NcPinSpec,
        available: &[&str],
    ) {
        if crate::db::diagnostic::diagnostic::has_code_at(code, uri, spec.offset) {
            return;
        }
        crate::db::diagnostic::diagnostic::diagnostic_log_at(
            code,
            crate::db::diagnostic::diagnostic::DiagnosticLevel::Warning,
            uri.to_string(),
            spec.offset,
            operand.len() as u32,
            &crate::errcodes::format_msg(code, &[&operand, &owner, &available.join(", ")]),
            &[],
        );
    }

    // Phase 1-2-4: Connection stmt processing

    pub(super) fn instantiate_stmts_resilient(&mut self) {
        let stmts = self.def.stmts.clone();
        let stmt_spans = self.def.stmt_spans.clone();
        for (_i, _l) in stmts.iter().enumerate() {}
        for (idx, stmt) in stmts.iter().enumerate() {
            // Iter-6.S4.3
            // **per-stmt auto_inst_map scope reset**
            //
            // Background: auto_inst_map uses McPhrase pointer address as key, associating
            // process_member_internal's product (instance name) with resolve_funccall_*
            // query. This pointer-key mechanism is only safe **within the lifetime of a single
            // McPhrase tree** —
            // after process_stmt call returns, the McPhrase nodes from the previous stmt
            // are freed, their addresses may be reused by newly allocated McPhrase in the next
            // stmt.
            // At this point old entry is a dangling reference, hitting it by new address **points
            // to wrong instance**.
            //
            // Triggering example (captured in practice after Iter-6.S4 fix):
            //   stmt N:   `mcu.setup().add_caps().i2c().do_flash(flash)`
            //             — Iter-6.S4 fallback wrote 4 stale entries
            //             (Note: that insert has been removed by Iter-6.S4.2, but dispatch
            // success path, iterated calls, builtin twopin and other locations still write)
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
            // **recursively call** process_stmt (to expand function body), that layer must share
            // the outer
            // auto_inst_map. Here at the true "stmt boundary", recursive calls are already in
            // deeper process_stmt call stack, not affected by this clear.
            //
            // Side effect tracking: there is no McPhrase sharing between top-level stmts
            // (each stmt is an independent AST subtree), so clear won't lose any entries
            // that **should be shared across stmts**. The overall instantiation results (components
            // / sub_modules /
            // connections) are in other fields of self, not in auto_inst_map, unaffected by clear.
            self.auto_inst_map.clear();

            // ★ Set current stmt span for diagnostic position reporting.
            //   Used as fallback when NetPoint.src_pos is unavailable (e.g., E2003/E2005).
            let stmt_span = stmt_spans.get(idx).map(|s| {
                crate::semantic::common::SourcePos::new(self.def_uri.clone(), s.start as u32)
            });
            self.current_stmt_span = stmt_span.clone();
            // A new statement also retires the previous one's func-body
            // anchor: `last_func_stmt` must never outlive the top-level
            // statement whose func expansion set it, or a library chain in a
            // later, func-free statement would attribute to a body that is
            // no longer executing.
            self.last_func_stmt = None;
            // The next stmt's start bounds this one; the last stmt is bounded
            // by the file end, which `u32::MAX` stands in for.
            let stmt_end = stmt_spans.get(idx + 1).map_or(u32::MAX, |s| s.start as u32);
            self.current_stmt_end = Some(stmt_end);

            if let Err(e) = self.process_stmt(stmt) {
                // ★ Single connection stmt failure doesn't interrupt, record diagnostics then
                // continue processing subsequent stmts
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
        self.current_stmt_end = None;

        // ── P2-C2: After all body stmts processed, project accumulated bus members to bare ports
        // ──
        // NOTE: These post-processing steps are now invoked from instantiate() after
        // auto_invoke_module_funcs(), so they cover both regular stmts and auto-invoked closures.
        // self.infer_bare_port_members_from_buses();  // moved to instantiate()
        // self.dedup_connections();                    // moved to instantiate()
        // self.check_unbound_param_ports();            // moved to instantiate()
    }

    /// P5: Deduplicate equivalent connections
    /// key = **unordered** set of each point's canonical path in connection (sort + dedup)
    /// **plus** the edge `dir` and `op` (§4.6 C-3).
    /// Same set ⇒ same electrical connection (order irrelevant, duplicate points meaningless), keep
    /// only first.
    /// No-op for net aggregation result (union-find already merged), only clears redundant
    /// connections and warnings.
    ///
    /// The `dir`/`op` half of the key is deliberate: two connections over the same
    /// unordered point set but written with a different arrow/operator are distinct
    /// per-edge truths — each is a separate vote for the downstream
    /// `majority_dir` projection (§4.6 C-3) — so deduplicating them by point set
    /// alone would silently drop one direction.
    pub(super) fn dedup_connections(&mut self) {
        let before = self.connections.len();
        let mut seen: HashSet<(Vec<String>, ConnDir, Option<ConnOp>)> = HashSet::new();
        let mut kept: Vec<ConnectionInst> = Vec::with_capacity(before);
        for conn in std::mem::take(&mut self.connections) {
            let mut key: Vec<String> = conn
                .points
                .iter()
                .map(|p| canonicalize_path(&p.path))
                .collect();
            key.sort();
            key.dedup();
            if seen.insert((key, conn.dir, conn.op)) {
                kept.push(conn);
            }
        }
        let _removed = before - kept.len();
        self.connections = kept;
    }

    /// P2: unify component instance pin "alias paths" to "pid paths"
    /// `ldo.Vout` / `ldo.GND` / `ldo.VIN.Vin` → `ldo.5` / `ldo.2` / `ldo.1`.
    /// These alias forms come from multiple construction paths (get_left_points
    /// member branch directly concatenates the path, component func body
    /// prefixing, etc.); they bypass node_to_netpoint and so don't get parsed;
    /// whereas .Cap() etc. use the pid form. Different strings → union-find
    /// never merges. Here we collapse them in one pass before union.
    // Post-expansion validation: verify NetPoint references

    /// Validate all generated NetPoints after expansion.
    ///
    /// Checks:
    /// 1. Component pin references — owner is a component, verify pin exists
    /// 2. Sub-module port references — owner is a sub-module, verify port exists
    ///
    /// Emits user-visible warning diagnostics for unresolved references
    /// (migrated from `tracing::warn!` per §7.2.3 — these are the func-body
    /// expansion artifacts Pass1 does not see). Warnings are anchored at the
    /// offending reference's own source position when available (group.rs
    /// pattern), else the connection's source span, else the file start — not
    /// the (1,1) file start that hid these warnings' true location.
    /// Called before `build_net_table()`.
    /// Anchor for a submodule-member diagnostic: the net point's own source
    /// position when available, else the connection's source span, else the
    /// current file start (same chain the E3175 branch uses).
    fn member_anchor(
        pt: &crate::instant::mc_net::NetPoint,
        conn: &crate::instant::mc_net::ConnectionInst,
    ) -> (crate::semantic::common::McURI, u32) {
        pt.src_pos
            .first()
            .map(|s| (s.uri.clone(), s.offset))
            .or_else(|| conn.source_span.as_ref().map(|s| (s.uri.clone(), s.offset)))
            .unwrap_or_else(|| (crate::current_uri::get(), 0))
    }

    /// U151 boundary ticket check: does this submodule member carry a direction
    /// word on the module *boundary*? Only `In`/`Out`/`InOut`/`Power` rows are
    /// boundary members; `label` rows, direction-less buses, `nc`/
    /// return faces are module-internal (label-boundary-gate-design.md).
    ///
    /// The authoritative face is the instance's own [`McModuleInst::ports`]:
    /// a member of an aggregate port (`in [VDD, GND]::DC(…)`) has no port row
    /// of its own (its def-store entry is an implicit label), so the *owning*
    /// port's direction decides for it via `bus_members`.
    pub(super) fn is_internal_member(sub: &McModuleInst, port_name: &str) -> bool {
        use crate::semantic::common::IOType;
        let exportable = |io: &IOType| {
            matches!(
                io,
                IOType::In | IOType::Out | IOType::InOut | IOType::Power
            )
        };
        let base = port_name.split('.').next().unwrap_or(port_name);
        if base.is_empty() || base.chars().all(|c: char| c.is_ascii_digit()) {
            return false;
        }
        // 1. Boundary face: a port row (direct, or as a member of an aggregate
        //    port) — the port's direction is the ticket.
        for p in &sub.ports {
            if port_base_name(&p.name) == base || p.bus_members.iter().any(|m| m == base) {
                return !exportable(&p.iotype);
            }
        }
        // 2. Def-store fallback: a bare `label X` row (or a direction-less bus
        //    row) that never became a port. Class instances (component /
        //    submodule rows) are not declaration members — reaching *them*
        //    keeps its not-a-port verdict (E3175), which is what the
        //    pins-through-boundary audit locks.
        match sub.def.insts.get_with_iotype(base) {
            Some((io, inst)) => {
                matches!(
                    inst,
                    McInstance::Label(_) | McInstance::Bus(_) | McInstance::List(_)
                ) && !exportable(io)
            }
            // Not a store name at all — leave the verdict to the E3175 face.
            None => false,
        }
    }

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
                            // Anchor at the offending reference: the net point's
                            // own source position when available, else the
                            // connection's source span, else the current file
                            // start — group.rs pattern, so the Problems entry
                            // points near the actual source instead of (1,1).
                            let (uri, pos) = pt
                                .src_pos
                                .first()
                                .map(|s| (s.uri.clone(), s.offset))
                                .or_else(|| {
                                    conn.source_span.as_ref().map(|s| (s.uri.clone(), s.offset))
                                })
                                .unwrap_or_else(|| (crate::current_uri::get(), 0));
                            crate::db::diagnostic::diagnostic::diagnostic_log_at(
                                crate::errcodes::COMPONENT_PIN_NOT_FOUND,
                                crate::db::diagnostic::diagnostic::DiagnosticLevel::Warning,
                                uri.clone(),
                                pos,
                                pt.path.len() as u32,
                                &crate::errcodes::format_msg(
                                    crate::errcodes::COMPONENT_PIN_NOT_FOUND,
                                    &[&pin_name, owner, &available.join(", ")],
                                ),
                                &[],
                            );
                            // ★ Ledger (resolve-gate §1.2③): pass2 net-point pin
                            // miss on a resolved component → UnresolvedRef
                            // (action mirrors the Warning-level diagnostic).
                            ledger::record(
                                LedgerEntry::new(
                                    LedgerKind::UnresolvedRef,
                                    pt.path.clone(),
                                    "phases.rs:pass2 net-point pin not found",
                                )
                                .with_action(LedgerAction::Warning)
                                .with_uri(uri)
                                .with_span(pos, pt.path.len() as u32),
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
                        // U151 first: a member that EXISTS but carries no
                        // direction word is a boundary violation (E3184), not
                        // a port miss — the internal verdict outranks the
                        // not-found one.
                        if !port_name.is_empty() && Self::is_internal_member(&sub, port_name) {
                            // U151: the member resolves structurally but carries no
                            // direction word — `label` rows and direction-less
                            // declarations are module-internal. A direction word is
                            // the only boundary ticket, so reaching it through an
                            // instance dot-path is E3184 (label-boundary-gate-design.md).
                            let (uri, pos) = Self::member_anchor(pt, conn);
                            // Anchors already reported at the mint face
                            // (`points.rs::note_internal_member_ref`) stay
                            // reported once — this face adds only violations
                            // the minter never saw (chain expansions).
                            if self
                                .internal_member_reported
                                .contains(&format!("{uri}:{pos}"))
                            {
                                continue;
                            }
                            crate::db::diagnostic::diagnostic::diagnostic_log_at(
                                crate::errcodes::LABEL_NOT_EXPORTABLE,
                                crate::db::diagnostic::diagnostic::DiagnosticLevel::Error,
                                uri.clone(),
                                pos,
                                pt.path.len() as u32,
                                &crate::errcodes::format_msg(
                                    crate::errcodes::LABEL_NOT_EXPORTABLE,
                                    &[&port_name, owner],
                                ),
                                &[],
                            );
                            // ★ Ledger: boundary violation on a resolved sub-module
                            // → UnresolvedRef (the connection does not form).
                            ledger::record(
                                LedgerEntry::new(
                                    LedgerKind::UnresolvedRef,
                                    pt.path.clone(),
                                    "phases.rs:pass2 submodule member not exportable",
                                )
                                .with_action(LedgerAction::Error)
                                .with_uri(uri)
                                .with_span(pos, pt.path.len() as u32),
                            );
                        } else if !port_name.is_empty() && !sub.is_valid_port_ref(port_name) {
                            let available: Vec<&str> =
                                sub.ports.iter().map(|p| p.name.as_str()).collect();
                            // Anchor at the offending reference: the net point's
                            // own source position when available, else the
                            // connection's source span, else the current file
                            // start — group.rs pattern, so the Problems entry
                            // points near the actual source instead of (1,1).
                            let (uri, pos) = Self::member_anchor(pt, conn);
                            crate::db::diagnostic::diagnostic::diagnostic_log_at(
                                crate::errcodes::MODULE_PORT_NOT_FOUND,
                                crate::db::diagnostic::diagnostic::DiagnosticLevel::Warning,
                                uri.clone(),
                                pos,
                                pt.path.len() as u32,
                                &crate::errcodes::format_msg(
                                    crate::errcodes::MODULE_PORT_NOT_FOUND,
                                    &[&port_name, owner, &available.join(", ")],
                                ),
                                &[],
                            );
                            // ★ Ledger (resolve-gate §1.2③): pass2 net-point port
                            // miss on a resolved sub-module → UnresolvedRef.
                            ledger::record(
                                LedgerEntry::new(
                                    LedgerKind::UnresolvedRef,
                                    pt.path.clone(),
                                    "phases.rs:pass2 net-point port not found",
                                )
                                .with_action(LedgerAction::Warning)
                                .with_uri(uri)
                                .with_span(pos, pt.path.len() as u32),
                            );
                        }
                    }
                    // If owner is neither a component nor sub-module, it might be
                    // a net label or external reference — skip validation.
                }
            }
        }
    }

    // P1: Args → Port binding / component constructor func

    /// The voltage the actual argument itself was declared at, read from the
    /// caller's symbol table.
    ///
    /// A connection-line `V3V3::DC(3.3V)` registers an `Interface` instance
    /// under its own name even though the phrase layer drops the interface from
    /// the endpoint, so the declaration is reachable here by exact name. The
    /// lookup is exact and whole: a dotted path, a `_` placeholder, a component
    /// instance or a name that simply is not declared all answer `None`, and the
    /// caller falls back to position. Nothing is split off the name, no first
    /// segment is taken, no spelling is pattern-matched — that is what made
    /// `V3V3` "match" `VDD_3V3` before (AGENTS.md, "no guessing from names").
    fn arg_declared_volt(&self, name: &str) -> Option<f64> {
        match self.def.insts.get(name)? {
            McInstance::Interface(iface) => declared_volt_of_params(&iface.params),
            _ => None,
        }
    }

    /// Connect declared instance args to sub-module formal ports by **position**.
    ///
    /// Formal port order = order of interface ports in sub-module signature
    /// (`module mod.sub([VDD_3V3,GND]::DC, [VCC_1V2,GND]::DC)` → port0, port1),
    /// over the ports a rail can legally land on — see `bindable_formals`.
    ///
    /// Member alignment strategy (short-circuit safe, matching-rules-design.md
    /// §3):
    ///   1. Equal-width multi-member zip: `[A,B] -> port{X,Y}` → A~inst.X, B~inst.Y
    ///   2. Vector port with scalar/unequal-width arg → E4180 (no implicit
    ///      expansion, no member dropping — the former "DC single rail"
    ///      inference was removed)
    ///   3. Rest (scalar↔scalar / unknown shape): single `arg ~ inst.port` (whole bus,
    ///      left to P2's expand_port_lanes for member expansion).
    pub(super) fn bind_actual_args_to_ports(
        &mut self,
        inst_name: &str,
        sub_def: &McModule,
        ports: &[PortInst],
        args: &[McParamValue],
    ) {
        // Hoisted before `make_ports` is defined: the closure is called after
        // further `&mut self` work, so it must capture this local rather than
        // reach through `self` (which would keep `self` borrowed for its life).
        let site = self.construction_site();
        let formal = bindable_formals(Some(sub_def), ports);

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

            // Choose formal port: ① the port DECLARED at the argument's own
            // declared voltage (order irrelevant); ② position fallback (next
            // unused). A spelling never decides — the pairing used to compare
            // digit-V-digit fragments of the two names, which made `V3V3` pick
            // `[VDD_3V3,GND]` for no reason beyond both spelling 3.3 V as "3V3"
            // (AGENTS.md, "no guessing from names"). When either side declares
            // no voltage there is nothing to pair on and position decides.
            let arg_v = self.arg_declared_volt(&arg_name);
            let chosen = arg_v.and_then(|v| {
                (0..formal.len()).find(|&fi| {
                    !used[fi]
                        && formal[fi]
                            .volt
                            .is_some_and(|pv| crate::eval::same_value(pv, v))
                })
            });
            let pi = match chosen.or_else(|| (0..formal.len()).find(|&fi| !used[fi])) {
                Some(pi) => pi,
                None => {
                    // An excess actual has no formal left to bind. It must reach
                    // the build report: `record_warning` only feeds the
                    // `InstDiagnostic` surface that mcviz metrics and module dumps
                    // read, so on its own the argument would vanish with nothing
                    // on screen. `log_global_diag` is the user-visible channel
                    // (deduped on (code, uri, offset) — see its doc).
                    let message = crate::errcodes::format_msg(
                        crate::errcodes::INST_ARG_NO_FORMAL_PORT,
                        &[&inst_name, &ai as &dyn std::fmt::Display, &arg_name],
                    );
                    self.record_warning(crate::errcodes::INST_ARG_NO_FORMAL_PORT, message.clone());
                    self.log_global_diag(
                        crate::errcodes::INST_ARG_NO_FORMAL_PORT,
                        crate::db::diagnostic::diagnostic::DiagnosticLevel::Warning,
                        message,
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
                    site.clone(),
                )];
                if named {
                    v.push(NetPoint::with_owner(
                        &format!("{inst_name}.{port_base}.{member}"),
                        inst_name,
                        io,
                        site.clone(),
                    ));
                }
                v
            };

            // ── Case 1: Equal-width multi-member → §11.3 pairing ──
            // Positional zip in write order: members[i] binds the i-th
            // argument lane. Member names are each side's local view, never a
            // matching criterion (interface-connect rule, 2026-09-19; the
            // former name-first pass is removed). Output stays in port
            // (member) declaration order.
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
            // ── Width mismatch for a vector port: scalar→vector / unequal →
            //    E4180. No implicit expansion, no member dropping, and no
            //    `[rail, GND]`-style inference (matching-rules-design.md §3
            //    B3/B4, P5).
            if members.len() >= 2 {
                let message = crate::errcodes::format_msg(
                    crate::errcodes::VECTOR_WIDTH_MISMATCH,
                    &[
                        &port.name,
                        &members.len() as &dyn std::fmt::Display,
                        &arg_name,
                        &arg_lanes.len() as &dyn std::fmt::Display,
                    ],
                );
                // Decl-context bindings (`US513 MCU513(V3V3, V1V2)`) are iterated
                // outside any func/stmt span; anchor the E4180 at the instance's
                // own declaration line so it doesn't collapse to row 1. Inline
                // instances (e.g. `MIC(V3V3)` inside a chain) have no decl
                // entry — fall back to the func/stmt span via record_error.
                match self.def.insts.get_port_span(inst_name) {
                    Some(r) => self.record_error_at(
                        crate::errcodes::VECTOR_WIDTH_MISMATCH,
                        message,
                        self.def_uri.clone(),
                        r.start as u32,
                    ),
                    None => self.record_error(crate::errcodes::VECTOR_WIDTH_MISMATCH, message),
                }
                continue;
            }
            // ── Case 3: scalar↔scalar / unknown shape ──
            if let Some(a) = arg_lanes.into_iter().next() {
                let port_pt = NetPoint::with_owner(
                    &format!("{}.{}", inst_name, port.name),
                    inst_name,
                    port.iotype.clone(),
                    site.clone(),
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

    /// Root cause A fix: Call site arg→port binding (multi-member curly/bracket ports)
    ///
    /// Used for the path of "declared sub-module called again with args" (funccall.rs's
    /// `rebind_submodule_params`), e.g. main.mc's `mic(V3V3).MIC` — mic was declared
    /// without args (`MIC_SIP mic`), the real arg `V3V3` is given in the body stmt.
    ///
    /// Key differences from `bind_actual_args_to_ports` (declared args path):
    ///   * **candidate set, two stages**: the declaration-borne set first
    ///     (`is_power_terminal` — a power direction word, a `::DC` face pair or a
    ///     declared voltage — in declaration order, so `dc{VDD_3V3,GND}`
    ///     (iotype=None) curly power ports are included and `io MIC{P,N}` is
    ///     not); the shape set only when that comes out EMPTY, i.e. the callee
    ///     declares no power contract at all, so a leaf (`component CAP` =
    ///     `pins = [1 = 1, 2 = 2]`) still binds its ordered endpoint list
    ///     (CIMP §1 U63).
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
    /// * Excess args beyond bindable ports are reported to the user as
    ///   `INST_ARG_UNBOUND_DETAILED` (W) through `log_global_diag`, mirrored
    ///   from `bind_actual_args_to_ports`; the 940 trace carries the same fact
    ///   into the diagnostic log. Port-side missed binding is covered by
    ///   `check_unbound_param_ports`.
    pub(super) fn bind_call_args_to_ports(
        &mut self,
        inst_name: &str,
        sub_def: &McModule,
        ports: &[PortInst],
        args: &[McParamValue],
    ) -> Vec<ConnectionInst> {
        // Hoisted before `make_ports` is defined: the closure is called after
        // further `&mut self` work, so it must capture this local rather than
        // reach through `self` (which would keep `self` borrowed for its life).
        let site = self.construction_site();
        let mut out: Vec<ConnectionInst> = Vec::new();

        // Stage 1 — the declaration-borne set. Stage 2 runs only when it is
        // empty: a callee declaring no power contract at all has no declaration
        // to bind by, and its argument list is an ordered list of CONNECTION
        // endpoints (`CAP(10uF).Cap([vin.V5V, vin.GND])`). Narrowing
        // unconditionally would empty the set for every passive leaf and the
        // args would silently stop binding (CIMP §1 U63).
        //
        // `formal` borrows the `ports` parameter (caller-provided clone), unrelated to self,
        // so subsequent `&mut self` calls (next_conn_id/expand_node_element/...) don't conflict.
        let mut formal: Vec<&PortInst> = bindable_formals(Some(sub_def), ports);
        if formal.is_empty() {
            formal = ports
                .iter()
                .filter(|p| {
                    !matches!(p.iotype, IOType::Out)
                        && (!p.bus_members.is_empty()
                            || p.name.contains('{')
                            || p.name.trim_start().starts_with('['))
                })
                .collect();
        }
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

            // Choose formal port: ① the port DECLARED at the argument's own
            // declared voltage (order irrelevant); ② position fallback (next
            // unused). Same rule as the declared-args binder — see there for why
            // a spelling must not decide.
            let arg_v = self.arg_declared_volt(&arg_name);
            let chosen = arg_v.and_then(|v| {
                (0..formal.len()).find(|&fi| {
                    !used[fi]
                        && formal[fi]
                            .volt
                            .is_some_and(|pv| crate::eval::same_value(pv, v))
                })
            });
            let pi = match chosen.or_else(|| (0..formal.len()).find(|&fi| !used[fi])) {
                Some(pi) => pi,
                None => {
                    // Actual args exceed ports -> skip (see function header "Scope").
                    // Mirror bind_actual_args_to_ports' warning so excess named
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
                    // `record_warning` alone is not surfaced in the build report
                    // (see its doc), and the 940 trace above lives only in the
                    // diagnostic log — so the fact has to be emitted on the
                    // user-visible channel too, or the argument disappears with
                    // nothing on screen.
                    let message = crate::errcodes::format_msg(
                        crate::errcodes::INST_ARG_UNBOUND_DETAILED,
                        &[
                            &inst_name,
                            &arg_name,
                            &self.name,
                            &bound as &dyn std::fmt::Display,
                            &formal.len() as &dyn std::fmt::Display,
                        ],
                    );
                    self.record_warning(
                        crate::errcodes::INST_ARG_UNBOUND_DETAILED,
                        message.clone(),
                    );
                    self.log_global_diag(
                        crate::errcodes::INST_ARG_UNBOUND_DETAILED,
                        crate::db::diagnostic::diagnostic::DiagnosticLevel::Warning,
                        message,
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
            // Closure borrows inst_name/base/named and site - all locals, not self.
            let make_ports = |member: &str, io: IOType| -> Vec<NetPoint> {
                let mut v = vec![NetPoint::with_owner(
                    &format!("{inst_name}.{member}"),
                    inst_name,
                    io.clone(),
                    site.clone(),
                )];
                if named {
                    v.push(NetPoint::with_owner(
                        &format!("{inst_name}.{base}.{member}"),
                        inst_name,
                        io,
                        site.clone(),
                    ));
                }
                v
            };

            // ── Case 1: Equal-width multi-member → §11.3 pairing ──
            // Positional zip in write order: members[i] binds the i-th
            // argument lane. Member names are never a matching criterion
            // (interface-connect rule, 2026-09-19; the former name-first pass
            // is removed). Output stays in port (member) declaration order.
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

            // ── Width mismatch for a vector port: scalar→vector / unequal →
            //    E4180. No implicit expansion, no member dropping, and no
            //    `[rail, GND]`-style inference (matching-rules-design.md §3
            //    B3/B4, P5).
            if members.len() >= 2 {
                self.record_error(
                    crate::errcodes::VECTOR_WIDTH_MISMATCH,
                    crate::errcodes::format_msg(
                        crate::errcodes::VECTOR_WIDTH_MISMATCH,
                        &[
                            &base,
                            &members.len() as &dyn std::fmt::Display,
                            &arg_name,
                            &arg_lanes.len() as &dyn std::fmt::Display,
                        ],
                    ),
                );
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
                let port_pt = NetPoint::with_owner(
                    &format!("{inst_name}.{dst_base}"),
                    inst_name,
                    pio,
                    site.clone(),
                );
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

    /// Root cause A companion diagnostic: "multi-member DC power port containing ground is never
    /// reached by any connection"
    ///
    /// Runs at the end of `instantiate_stmts_resilient` (after declared-arg binding + body stmt's
    /// rebind connections have been merged into self.connections).
    ///
    /// **Only** targets multi-member power ports containing ground (members >= 2 and at least one
    /// is a ground name), purpose:
    ///   * Catch truly floating cases like `SPEAKER_M speaker` where the source omits the power arg
    /// (`dc{VDD_3V3,GND}` neither has a declared arg, nor is called via `speaker(...)` in the body
    /// stmt);
    ///   * Exclude **groundless** signal bus ports like `port1{A,B,C,D}` (no false positives);
    /// * Exclude ldo's scalar `vin` (no members, not in scope, its grounding is a separate matter).
    ///
    /// Determine "connected": self.connections has a point with path == prefix, or starting with
    /// `prefix.`.
    /// Prefix contains both bare `inst.MEMBER` and (for named ports) dotted `inst.base.MEMBER`,
    /// aligned with the two label forms injected by inject/bind.
    ///
    /// Use **warning(942)** not error: this is a heuristic based on "connection path prefix
    /// matching",
    /// not compilable-verifiable in this environment; in case of false positives on ports
    /// indirectly grounded via nets, warning does not block.
    pub(super) fn check_unbound_param_ports(&mut self) {
        // ① Read-only self.sub_modules, compute prefix set for each port to check (borrows released
        // immediately).
        //    key = (instance, base name): curly power port in symbol table exists as both `Bus dc`
        //    and `Label dc{VDD_3V3,GND}` PortInst entries, both with base name `dc`,
        //    use key to dedup and avoid duplicate warnings on the same physical port.
        let mut needs: Vec<(String, String, Vec<String>)> = Vec::new();
        for sub in self.submodules_view() {
            let inst = sub.name.clone();
            for p in &sub.ports {
                if matches!(p.iotype, IOType::Out) {
                    continue;
                }
                let members = port_members(p);
                if members.len() < 2 {
                    continue;
                }
                // A power port is one whose own `::DC` declaration splits it
                // into a supply/return pair. The former test asked whether any
                // member was **named** like a ground (`is_ground_name`), so
                // whether the warning fired depended on the author's spelling;
                // the declaration says the same thing and is checkable
                // (world-axioms §1 A1).
                if p.dc_pair.is_none() {
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

        // ② Read-only self.connections, collect ports with "no connection hit", dedup by (instance,
        // base name)
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
    /// Convention: func's last segment name == component class's last segment name, that is the
    /// constructor
    /// (component `FLASH.sub` ↔ func `sub`).
    /// Body expands inside **parent module self** (peripheral components belong to parent module
    /// BOM),
    /// pin references prefixed with instance name (`VCC` → `flash.VCC`); arg names / parent port
    /// names not prefixed.
    pub(super) fn run_component_constructor(
        &mut self,
        inst_name: &str,
        comp_def: &Arc<McComponent>,
        args: &[McParamValue],
    ) {
        // Constructor = the one in funcs whose last segment name matches the class's last segment
        // name
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
        let mut bindings = match McParamBindings::bind(&func.params, args) {
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
        // Vector-formal width rules at the boundary (matching-rules-design.md
        // §3): equal width pairs member-to-lane, scalar/unequal reports E4180.
        // e.g. `FLASH.GD25Q32E flash(V3V3)` binds the `[V3V3, GND]` formal to
        // the caller's V3V3 DC bus, aligning body `V3V3`/`GND` to V3V3.VCC /
        // V3V3.GND lanes.
        // Constructors run from declarations (no func/stmt span), so anchor the
        // E4180 at the instance's own declaration line.
        let anchor = self
            .def
            .insts
            .port_spans()
            .get(inst_name)
            .and_then(|v| v.first().cloned())
            .map(|r| crate::semantic::common::SourcePos::new(self.def_uri.clone(), r.start as u32));
        bindings = self.align_vector_bindings(&bindings, anchor);

        // skip set: names appearing in args (parent scope net) + parent module ports -> not
        // prefixed
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
        let call_site = self.current_call_site();
        let eidx = self.expansion.begin(
            ExpansionKind::ComponentCtor,
            Some(inst_name.to_string()),
            last.clone(),
            call_site,
            Self::func_def_site(&func),
        );
        let conn_start = self.connections.len(); // ← P4 backstop start point
        let outer_auto_inst = self.auto_inst_map.clone();
        // ── §3.4: materialize the ctor func's standalone declarations (func.insts) ──
        if let Err(e) = self.materialize_declared_subinstances(&func, inst_name) {
            self.record_warning(
                crate::errcodes::INST_CTOR_BODY_STMT_FAILED,
                crate::errcodes::format_msg(
                    crate::errcodes::INST_CTOR_BODY_STMT_FAILED,
                    &[&last, &e.to_string()],
                ),
            );
        }
        for (_li, stmt) in func.stmts.iter().enumerate() {
            self.auto_inst_map = outer_auto_inst.clone();
            // Attribute anonymous instances/connections of this body stmt
            // to its exact source stmt in the func's own file. RAII
            // (§7.11(2)): restore happens on every exit.
            let _ = self.with_func_stmt(
                &func,
                Some(_li),
                |this| -> Result<(), crate::instant::mc_net::InstError> {
                    let mut substituted = Self::substitute_stmt(stmt, &bindings, None);
                    // ── §3.3: materialize deferred constructions in ctor body stmts ──
                    if let Err(e) =
                        this.materialize_deferred_subinstances(&mut substituted, inst_name)
                    {
                        this.record_warning(
                            crate::errcodes::INST_CTOR_BODY_STMT_FAILED,
                            crate::errcodes::format_msg(
                                crate::errcodes::INST_CTOR_BODY_STMT_FAILED,
                                &[&last, &e.to_string()],
                            ),
                        );
                        return Ok(());
                    }
                    let prefixed =
                        Self::prefix_instance_stmt_with_skip(&substituted, inst_name, &skip);
                    if let Err(e) = this.process_stmt(&prefixed) {
                        this.record_warning(
                            crate::errcodes::INST_CTOR_BODY_STMT_FAILED,
                            crate::errcodes::format_msg(
                                crate::errcodes::INST_CTOR_BODY_STMT_FAILED,
                                &[&last, &e],
                            ),
                        );
                    }
                    Ok(())
                },
            );
        }
        self.expansion.end(eidx);
        // ── P4 backstop: strip host-synthesized interface endpoints leaked during body processing
        // ──
        // (flash's `flash.in ~ CAP_1.1` / `CAP_1.2 ~ flash.out` etc.)
        self.strip_host_iface_phantoms(inst_name, conn_start);
    }
}

// Iter-8: Bus member extraction from port declarations
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
/// The member names of an interface-typed instance, in connection-ordinal
/// order (§11.1 — declaration order, never the BTreeMap pinid key order).
///
/// The label rides the same table that supplies the ordinal (interface-connect
/// rule §1.5.1: a member name is that role's local term for ordinal k —
/// CIMP §1 U137): the declared role's own table > the interface base table
/// (T1). Mirrors the role half of the mc_pins attach chain
/// (`components/mc_pins`), so every face that expands an interface instance —
/// port lanes, DC pair decode, in-body label injection — reads one source.
/// Written members (`U{A,B}::FAM(...)`, `[A,B]::FAM(...)`) are the author's
/// own spellings and are consumed by the callers before this fallback.
///
/// `parsed_pins` is deliberately not consulted here (CIMP §1 U141, ruled
/// 2026-09-20): the parameterized pin tables are attach-face wiring
/// spellings — the ordinal is the identity carrier across the boundary, and
/// the per-parameter names vary per instantiation, so labeling from them
/// splits the name-based net merge (real boards hbl1/hs: +1 net, +5 errors).
/// Locked by `tests/shard7/u141_parsed_pins_boundary.rs`.
fn iface_ordinal_member_names(iface: &Mc2Interface) -> Vec<String> {
    iface_adopted_pin_table(iface).member_names()
}

/// The pin table an interface adoption consults: the adopted role's rows when
/// the role wrote its own pin rows, else the conductor view. The port members
/// and the differential-pair read both expand from THIS table, so a pair is
/// declared on the same rows its consumers were born from — the two views of
/// one interface never cross (U205② ruled 2026-09-23: the read follows the
/// role view; a pair declared only on the other view is unreachable data).
fn iface_adopted_pin_table(iface: &Mc2Interface) -> &McPins {
    if let Some(McParamValue::Ids(role_ids)) = iface.params.first() {
        let role_name = role_ids.to_string();
        for role in &iface.base.roles {
            if role.name.to_string() == role_name && !role.pins.member_names().is_empty() {
                return &role.pins;
            }
        }
    }
    &iface.base.pins
}

fn extract_port_bus_members(inst: &McInstance, _port_name: &str) -> Vec<String> {
    match inst {
        // List: `[A, B]` or `GPIO[1:2]`
        //
        // Both spellings carry their members the same way — the written list IS
        // the member list. The unnamed form used to return nothing, on the
        // grounds that it has no prefix to hang members from; that left a
        // declared `psnk [VDDIO,GND]` a port with no members at all, so its
        // argument could never be bound and its member paths were never
        // registered for the parent to connect to (CIMP §1 U31).
        McInstance::List(list) if list.member.len() >= 2 => list.member.clone(),

        // Curly: `name{A, B}`
        McInstance::Bus(bus) if bus.member.len() >= 2 => bus.member.clone(),

        // Interface: `[A, B]::DC()` or `dc{A, B}::DC()` or `MIC{P, N}::ADC.DIFF()`
        //
        // S1 Bug D fix (Part 2)
        // **Bare interface ports** like `io SPI` (no curly members, e.g. `io SPI`
        // not `io SPI{CS, SCLK, MISO, MOSI}`) have no member info on iface.name
        // (as_bus / list_members both empty). But Mc2Interface.base is the full
        // McInterface definition, its pins.pins BTreeMap's value (McPin)'s
        // `names[0]` is the original declared pin name (e.g. SPI: CS/SCLK/MISO/
        // MOSI in BTreeMap pinid order = declaration order for numeric pinids).
        //
        // Falling back to Vec::new() leaves expand_port_lanes without lanes,
        // so a cross sub-module boundary degrades to scalar (1 point) -> 1-vs-N
        // fan (the §5.3.1-abolished single-point broadcast) shorts N physical
        // pins into the same net (S1 SPI four-wire short).
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
            // Fallback: take member names in connection-ordinal order — the
            // declared role's own table when the port carries a role, else the
            // base interface's pins table (§11.1 — the member vector order never
            // comes from the BTreeMap pinid key order; `member_names()` reads the
            // recorded declaration order). The label rides the same table that
            // supplies the ordinal (U137), mirroring the mc_pins attach chain.
            // Applies to BOTH bare interface ports (e.g. `io SPI`) and scalar named
            // ports with interface annotation (e.g. `V3V3::DC(3.3V)`, `in vin::DC(5V)`).
            // The interface type defines the members (e.g. DC → VCC, GND), and the port
            // name is just a label — the electrical members come from the interface type.
            let pin_names: Vec<String> = iface_ordinal_member_names(iface);
            if pin_names.len() >= 2 {
                return pin_names;
            }
            Vec::new()
        }

        _ => Vec::new(),
    }
}

/// Read an interface body's `@pair(group)` member-row tags as the pairs they
/// declare: one `(leg_a, leg_b)` tuple per group, legs in member order (the
/// language declares no polarity; the first member is the derived leg A).
///
/// The group name is the interface author's own identifier — equality of the
/// tag is the only operation. Only a group with exactly two legs becomes a
/// tuple; a diseased count is the definition-side gate's verdict
/// (HW_IFACE_PAIR_NOT_TWO), never flattened into a guessed pair. Nothing here
/// compares a member name with a net name.
/// The interface's declared differential pairs, read off ONE pin table (the
/// rows the adoption expanded — see `iface_adopted_pin_table`): rows carrying
/// a `@pair(group)` tag cluster by that tag, and a group with exactly two rows
/// declares one pair, leg order = member order. The tag spelling itself is
/// never read. Two groups resolving to the same pair of member names are both
/// returned; consumers match by member name, so same-name groups (USB.C's
/// A-side and B-side `USB2_D±`) collapse into one pair on the nets — ruled
/// 2026-09-23 (U205③): declarable, the collapse is the defined behavior.
fn read_iface_diff_groups(pins: &McPins) -> Vec<(String, String)> {
    let pair_key = crate::semantic::basic::attr_keys::KEY_PAIR;
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    for (name, id) in pins.member_entries() {
        let Some(pin) = pins.pins.get(&id) else {
            continue;
        };
        let Some(group) = crate::semantic::module::pi::attr_texts(&pin.attrs, pair_key)
            .into_iter()
            .next()
        else {
            continue;
        };
        match groups.iter_mut().find(|(g, _)| *g == group) {
            Some((_, members)) => members.push(name),
            None => groups.push((group, vec![name])),
        }
    }
    groups
        .into_iter()
        .filter_map(|(_, members)| {
            if members.len() != 2 {
                return None;
            }
            let mut it = members.into_iter();
            let a = it.next()?;
            let b = it.next()?;
            Some((a, b))
        })
        .collect()
}

/// Get port base name: strip `{...}` / `[...]` suffix.
///   `dc{VDD_3V3,GND}` -> `dc`;  `vin` -> `vin`;
///   `[VDD_3V3,GND]`   -> ``  (starting with `[`/`{` = anonymous port, no base name).
/// Consistent with `inject_port_member_labels`'s anonymous vs named distinction:
///   named ports have both bare(`MEMBER`) and dotted(`base.MEMBER`) labels,
///   anonymous bracket ports only have bare.
pub(super) fn port_base_name(name: &str) -> String {
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

/// Is this port a power terminal — the only kind of formal an actual
/// argument may be bound to?
///
/// The evidence is declaration-borne, never name-borne (AGENTS.md, "no
/// guessing from names"): a power direction word (`psrc`/`psnk`/`psbi` →
/// `IOType::Power`), a supply/return face pair (`::DC` → `dc_pair`), or a
/// declared voltage. Shape is not evidence either: `io MIC{P,N}` and
/// `port1{A,B,C,D}` carry members, and under the old "name starts with `[`
/// or has members" criterion they entered the candidate set, where the
/// positional fallback could land a caller's rail on a signal bus with zero
/// diagnostics (CIMP §1 U31).
fn is_power_terminal(p: &PortInst) -> bool {
    matches!(p.iotype, IOType::Power) || p.dc_pair.is_some() || p.volt.is_some()
}

/// Candidate formals for an argument list, in **declaration order**.
///
/// Neither set the binder wants is `self.ports` order: `def.insts` is a
/// sorted map, so the port table comes out alphabetical, and interface-typed
/// signature params are appended after the body ports. Declaration position
/// comes from the recorded source spans instead (§11 — member/declaration
/// order is source order, never alphabetical). Ports with no recorded span
/// sort last, by table index, so the order stays total.
fn bindable_formals<'a>(def: Option<&McModule>, ports: &'a [PortInst]) -> Vec<&'a PortInst> {
    let mut keyed: Vec<(usize, usize, &'a PortInst)> = ports
        .iter()
        .enumerate()
        .filter(|(_, p)| is_power_terminal(p))
        .map(|(i, p)| {
            (
                def.and_then(|d| d.port_decl_span(&p.name))
                    .map_or(usize::MAX, |r| r.start),
                i,
                p,
            )
        })
        .collect();
    keyed.sort_by_key(|(pos, i, _)| (*pos, *i));
    keyed.into_iter().map(|(_, _, p)| p).collect()
}

// ── Declared voltage of a port / of an argument (CIMP U12) ──
//
// The argument binders pair an actual argument with the formal port that was
// DECLARED at the same voltage. Two declaration shapes carry that value, so
// there are two readers; both answer `None` the moment the value would have to
// be guessed at — no `Volt` argument at all, more than one, or a range /
// `±` literal, which is not a value and therefore pairs with nothing.
// See `PortInst::volt`.

/// Voltage declared by an interface instance's constructor arguments
/// (`[VDD_3V3,GND]::DC(3.3V)` → `Some(3.3)`). The `DC`/`AC`/... class is not
/// consulted: a declared voltage is a declared voltage whatever the class.
fn declared_volt_of_params(params: &[McParamValue]) -> Option<f64> {
    let mut found: Option<f64> = None;
    for p in params {
        let McParamValue::UValue(uv) = p else {
            continue;
        };
        if uv.unit() != &McUnit::Volt || uv.is_range_or_plusminus() {
            continue;
        }
        if found.is_some() {
            // Two voltages in one declaration: nothing to pair on.
            return None;
        }
        found = Some(uv.value());
    }
    found
}

/// The same decode for a declaration that reached us as source text — the
/// module-signature parameters, which carry `["3.3V"]` rather than a value
/// object. Reads through the one value engine, so both shapes agree on what a
/// written voltage is; a range literal comes back `None` there as here.
fn declared_volt_of_texts(params: &[String]) -> Option<f64> {
    let mut found: Option<f64> = None;
    for s in params {
        let Some(v) = crate::eval::quantity_in(s, &McUnit::Volt) else {
            continue;
        };
        if found.is_some() {
            return None;
        }
        found = Some(v);
    }
    found
}

// ── U48: `@ncpin(…)` matching against a module port ──

/// The registered path suffixes of `port` that the written operand `operand`
/// names. `None` when it names nothing on this port.
///
/// Matching is against the port's *registered* surface
/// ([`PortInst::path_suffixes`]) — the strings the flat table actually carries —
/// so the marker and the connection face agree on what a spelling denotes.
///
/// Naming the port's own header (`VIN`, `GPIO[1:2]`) or its bracket alias
/// (`[VDD_3V3, GND]`) covers the whole port, members included. That is not
/// generosity: the header is de-electrified and never reported on its own
/// (E4114 fires on the members), so a marker that covered only the header would
/// suppress nothing at all — a silent no-op, which is the one outcome this
/// marker must never produce. Naming one member covers that member only, so
/// `@ncpin(MIC.P)` keeps `MIC.N` reported.
fn nc_port_hits(operand: &str, port: &PortInst) -> Option<Vec<String>> {
    let s = port.path_suffixes();
    if s.header == operand || s.bracket.as_deref() == Some(operand) {
        let mut whole = vec![s.header.clone()];
        whole.extend(s.bracket.iter().cloned());
        whole.extend(s.members.iter().cloned());
        return Some(whole);
    }
    (s.members.iter().any(|m| m == operand)).then(|| vec![operand.to_string()])
}

/// The member suffixes of `port` whose name is a number inside the inclusive
/// range. A range only ever selects members: no header spelling of a
/// member-bearing port is a number.
fn nc_port_range_hits(from: i64, to: i64, port: &PortInst) -> Vec<String> {
    port.path_suffixes()
        .members
        .into_iter()
        .filter(|m| m.parse::<i64>().is_ok_and(|v| from <= v && v <= to))
        .collect()
}
