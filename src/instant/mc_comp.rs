// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Pass2 Instantiation - Component instance
//!
//! McComponentInst

use super::mc_net::{InstError, NetPoint};
use crate::core::basic::mc_param::{McParamBindings, McParamValue};
use crate::core::common::IOType;
use crate::core::component::McComponent;
use std::collections::HashMap;
use std::sync::Arc;

// ============================================================================
// McComponentInst - Component instance
// ============================================================================

/// Pass2 Instantiation - Component instance
#[derive(Debug)]
pub struct McComponentInst {
    /// Component instance name
    pub name: String,

    /// Base definition of the component instance
    pub def: Arc<McComponent>,

    /// Parameter bindings
    pub params: McParamBindings,

    /// Pin instances (pin_name -> NetPoint)
    pub pins: HashMap<String, NetPoint>,

    /// NC (Not Connected) instance
    pub nc: bool,
}

impl McComponentInst {
    /// Create a new component instance
    pub fn new(name: &str, def: Arc<McComponent>) -> Self {
        let mut inst = Self {
            name: name.to_string(),
            def: def.clone(),
            params: McParamBindings::new(),
            pins: HashMap::new(),
            nc: false,
        };

        inst.init_pins();
        inst
    }

    /// Create a component instance with parameters
    pub fn with_params(
        name: &str,
        def: Arc<McComponent>,
        param_values: &[McParamValue],
    ) -> Result<Self, InstError> {
        let params = McParamBindings::bind_quiet(&def.params, param_values)
            .map_err(|e| InstError::Other(format!("Parameter binding failed: {e:?}")))?;

        let nc = param_values
            .iter()
            .any(|p| matches!(p, McParamValue::NC(_)));

        let mut inst = Self {
            name: name.to_string(),
            def: def.clone(),
            params,
            pins: HashMap::new(),
            nc,
        };

        inst.init_pins();
        Ok(inst)
    }

    /// Create a component instance with NC status
    pub fn with_nc(name: &str, def: Arc<McComponent>) -> Self {
        let mut inst = Self {
            name: name.to_string(),
            def: def.clone(),
            params: McParamBindings::new(),
            pins: HashMap::new(),
            nc: true,
        };

        inst.init_pins();
        inst
    }

    /// Initialize pins of the component instance
    fn init_pins(&mut self) {
        let pids = self.def.pins.get_all_pins();

        for pin_id in pids {
            let path = format!("{}.{}", self.name, pin_id);
            let iotype = self.def.pins.get_pin_io(&pin_id).unwrap_or(IOType::None);
            let net_point = NetPoint::with_owner(&path, &self.name, iotype);
            self.pins.insert(pin_id, net_point);
        }

        if self.def.pins.has_dynamic_pins() {
            self.init_dynamic_pins();
        }
    }

    /// Initialize dynamic pins
    /// Dynamic pins contain parameter references (e.g., `1:cols`) and need to be resolved
    /// at instantiation time based on actual parameter values
    fn init_dynamic_pins(&mut self) {
        let bindings = self.get_param_bindings();
        let dynamic_pins = self.def.pins.resolve_dynamic_pins(&bindings);

        for (pin_id, _pin_name, iotype) in dynamic_pins {
            let path = format!("{}.{}", self.name, pin_id);
            let net_point = NetPoint::with_owner(&path, &self.name, iotype);
            self.pins.insert(pin_id.to_string(), net_point);
        }
    }

    /// Get (name, i64) list of parameter bindings
    fn get_param_bindings(&self) -> Vec<(String, i64)> {
        use crate::core::basic::mc_paramd::McParamDeclare;
        let mut bindings = Vec::new();

        for binding in self.params.iter() {
            // Try as_int_binding (handle Single/Roles)
            if let Some((name, value)) = binding.as_int_binding() {
                bindings.push((name, value));
                continue;
            }

            // Handle UValue type (like cols::INT = 6)
            if let McParamDeclare::UValue(uval) = &binding.declare {
                let name = uval.name.get_primary_name().unwrap_or_default();
                if let Some(value) = &binding.value {
                    if let crate::core::basic::mc_param::McParamValue::Int(int_val) = value {
                        bindings.push((name, int_val.value));
                    }
                }
            }
        }

        bindings
    }

    /// Get a pin by ID
    pub fn get_pin(&self, pin_id: &str) -> Option<&NetPoint> {
        self.pins.get(pin_id)
    }

    /// Get the left pin of the component instance (usually pin "1" or the first pin)
    ///
    /// For multi-pin components (3+), prefer IO annotations:
    /// - If has In pins → return first In pin
    /// - If has Power pins → return first Power pin (VIN)
    /// - Otherwise fallback to pin "1" or first pin
    pub fn get_left_pin(&self) -> Option<NetPoint> {
        if self.pins.len() > 2 {
            // Multi-pin components: try IO-aware selection
            let in_pins = self.get_input_pins();
            if let Some(first) = in_pins.first() {
                return Some(first.clone());
            }
            let pwr_pins = self.get_power_pins();
            if let Some(first) = pwr_pins.first() {
                return Some(first.clone());
            }
        }
        // 2-pin components: fallback
        if let Some(pin) = self.pins.get("1") {
            return Some(pin.clone());
        }
        self.pins.values().next().cloned()
    }

    /// Get the right pin of the component instance (usually pin "2" or the second pin)
    ///
    /// For multi-pin components (3+), prefer IO annotations:
    /// - If has Out pins → return first Out pin
    /// - If has Power pins → return second Power pin (GND)
    /// - Otherwise fallback to pin "2" or second pin
    pub fn get_right_pin(&self) -> Option<NetPoint> {
        if self.pins.len() > 2 {
            // Multi-pin components: try IO-aware selection
            let out_pins = self.get_output_pins();
            if let Some(first) = out_pins.first() {
                return Some(first.clone());
            }
            let pwr_pins = self.get_power_pins();
            if pwr_pins.len() >= 2 {
                return Some(pwr_pins[1].clone());
            }
        }
        // 2-pin components: fallback
        if let Some(pin) = self.pins.get("2") {
            return Some(pin.clone());
        }
        self.pins.values().nth(1).cloned()
    }

    /// Get all input pins (IOType::In)
    ///
    /// Returns: Vec<NetPoint> with instance path prefix (e.g., "U1.SDA", "U1.SCL")
    pub fn get_input_pins(&self) -> Vec<NetPoint> {
        self.get_pins_by_io(&IOType::In)
    }

    /// Get all output pins (IOType::Out)
    ///
    /// Returns: Vec<NetPoint> with instance path prefix (e.g., "U1.SDA", "U1.SCL")
    pub fn get_output_pins(&self) -> Vec<NetPoint> {
        self.get_pins_by_io(&IOType::Out)
    }

    /// Get all pins of type Power (IOType::Power)
    pub fn get_power_pins(&self) -> Vec<NetPoint> {
        self.get_pins_by_io(&IOType::Power)
    }

    /// Get pins by IO type
    ///
    /// Query pins in component definition by IO type, return matching instantiated pins.
    /// Returned NetPoint already includes instance path prefix (e.g., "U1.3").
    pub fn get_pins_by_io(&self, iotype: &IOType) -> Vec<NetPoint> {
        let pin_ids = self.def.pins.get_pins_by_io(iotype);
        pin_ids
            .iter()
            .filter_map(|pid| self.pins.get(pid).cloned())
            .collect()
    }

    /// Get pins grouped by IO type
    ///
    /// Returns: (input_pins, output_pins, power_pins, other_pins)
    pub fn get_pins_grouped(&self) -> (Vec<NetPoint>, Vec<NetPoint>, Vec<NetPoint>, Vec<NetPoint>) {
        let input = self.get_input_pins();
        let output = self.get_output_pins();
        let power = self.get_power_pins();

        let known_ids: std::collections::HashSet<String> = {
            let mut s = std::collections::HashSet::new();
            for p in &input {
                s.insert(p.path.clone());
            }
            for p in &output {
                s.insert(p.path.clone());
            }
            for p in &power {
                s.insert(p.path.clone());
            }
            s
        };

        let other: Vec<NetPoint> = self
            .pins
            .values()
            .filter(|p| !known_ids.contains(&p.path))
            .cloned()
            .collect();

        (input, output, power, other)
    }

    /// Get all pins as Vec<NetPoint>
    pub fn get_all_pins(&self) -> Vec<NetPoint> {
        self.pins.values().cloned().collect()
    }

    /// Check if the component is a two-port device
    pub fn is_two_port(&self) -> bool {
        self.pins.len() == 2
    }

    /// Check if the component has multiple pins (3+)
    pub fn is_multi_pin(&self) -> bool {
        self.pins.len() > 2
    }

    /// Check if the component definition has any IO annotations
    ///
    /// Returns true if the component definition has any IO annotations.
    /// Components with IO annotations can use IO-aware connection strategies.
    pub fn has_io_annotations(&self) -> bool {
        !self.def.pins.get_pins_by_io(&IOType::In).is_empty()
            || !self.def.pins.get_pins_by_io(&IOType::Out).is_empty()
            || !self.def.pins.get_pins_by_io(&IOType::Power).is_empty()
    }

    /// Get the number of pins of the component instance
    pub fn pin_count(&self) -> usize {
        self.pins.len()
    }

    // ========================================================================
    // Iter-10 (Bucket D): Component bus port → pin_id list query
    // ========================================================================

    /// Query the pin_ids of a component's bus port.
    ///
    /// ## Usage
    /// `points.rs::expand_port_lanes` Case 3 uses this method to expand `<comp>.<port>`
    /// form into `[<comp>.<pid_1>, <comp>.<pid_2>, ...]` multiple lanes. This is
    /// bugfix_report errors 1 / 3 / 4 / 8 unified root cause fix — when parent/same module
    /// body writes `uC.UART0` / `uC.SPI` / `uC.XTAL` / `uC.I2C0` "component instance
    /// bus interface" reference, current implementation treats it as single-point path "uC.UART0", neither
    /// hitting inst_table (component pin registration path is `<comp>.<pid>` numeric form),
    /// nor doing N×N alignment connection.
    ///
    /// Ignore `McPinPort` form (Bus / Interface / List / Multi), uniformly scan
    /// `names_to_id`:
    ///   1. Direct hit `Multi(pids)` (typical List form like `PDM[CLK,DATA]`):
    ///      pids returned directly.
    ///   2. General case: scan all name keys starting with `{port_name}.`,
    ///      expect them to correspond to `Single(pid)` form (mc_pins.rs
    ///      `register_pin` in Bus / Interface / List three forms all
    ///      register a dotted name entry for each physical pid).
    ///      e.g.:
    ///      - Bus(VIN, ["Vin","GND"])      → "VIN.Vin"/"VIN.GND" → pid
    ///      - Interface(UART0::UART.TTL)  → "UART0.TX"/"UART0.RX" → pid
    ///      - Interface(XTAL{X1,X2})      → "XTAL.X1"/"XTAL.X2"  → pid
    ///
    /// ## Sorting
    /// Sort pin_ids in ascending order by pid value (e.g., `"6","7"` not `"7","6"`).
    /// This matches the order of pin declarations in mc source code,
    /// ensuring stable N×N pin connections.
    ///
    /// ## Return
    /// - At least 2 pin_ids → `Some(pids)`
    /// - Port not found / single pin / not found associated pid → `None`
    pub fn find_bus_port_pin_ids(&self, port_name: &str) -> Option<Vec<String>> {
        // ── [P2-FBPPI] Temporary probe (commented)
        // if port_name.starts_with("UART") {
        //     use crate::core::component::mc_pins::McPinPort;
        //     let kind = self.def.pins.names_to_id.get(port_name).map(|p| match p {
        //         McPinPort::Single(id) => format!("Single({})", id),
        //         McPinPort::Bus(_) => "Bus".to_string(),
        //         McPinPort::Interface(_) => "Interface".to_string(),
        //         McPinPort::List(_, _) => "List".to_string(),
        //         McPinPort::Multi(v) => format!("Multi({:?})", v),
        //         McPinPort::NC => "NC".to_string(),
        //         McPinPort::MultiGroup(v) => format!("MultiGroup({:?})", v),
        //     });
        //     let n2i: Vec<String> = self.def.pins.names_to_id.iter()
        //         .map(|(n, p)| format!("{}=>{}", n, match p {
        //             McPinPort::Single(id) => format!("S({})", id),
        //             McPinPort::Bus(_) => "Bus".to_string(),
        //             McPinPort::Interface(_) => "Iface".to_string(),
        //             McPinPort::List(_, _) => "List".to_string(),
        //             McPinPort::Multi(v) => format!("Multi{:?}", v),
        //             McPinPort::NC => "NC".to_string(),
        //             McPinPort::MultiGroup(v) => format!("MultiGroup{:?}", v),
        //         }))
        //         .collect();
        //     let p2n: Vec<String> = self.def.pins.pin_id_to_names.iter()
        //         .map(|(k, v)| format!("{}=>{:?}", k, v))
        //         .collect();
        //     let raw_pins: Vec<String> = self.def.pins.pins.keys().cloned().collect();
        //     eprintln!("[P2-FBPPI] comp={} port={} kind={:?} static_count={}",
        //         self.name, port_name, kind, self.def.pins.count());
        //     eprintln!("[P2-FBPPI-N2I] {:?}", n2i);
        //     eprintln!("[P2-FBPPI-P2N] {:?}", p2n);
        //     eprintln!("[P2-FBPPI-RAW] {:?}", raw_pins);
        // }

        use crate::core::component::mc_pins::McPinPort;

        if self.def.name.to_string().contains("US513_20_F")
            || self.def.name.to_string().contains("GD25Q32E")
        {
            let _n2i: Vec<String> = self
                .def
                .pins
                .names_to_id
                .iter()
                .map(|(n, p)| {
                    let tag = match p {
                        McPinPort::Single(id) => format!("S({})", id),
                        McPinPort::Multi(v) => format!("M{:?}", v),
                        McPinPort::MultiGroup(v) => format!("MG{:?}", v),
                        McPinPort::List(_, v) => format!("L{:?}", v),
                        McPinPort::Bus(b) => format!("Bus({};{:?})", b.name, b.member),
                        McPinPort::Interface(i) => format!("Iface({})", i.name),
                        McPinPort::NC => "NC".to_string(),
                    };
                    format!("{}=>{}", n, tag)
                })
                .collect();
        }

        // Port must be registered in names_to_id as some bus type
        let port_kind = self.def.pins.names_to_id.get(port_name)?;
        if !matches!(
            port_kind,
            McPinPort::Bus(_)
                | McPinPort::Interface(_)
                | McPinPort::List(_, _)
                | McPinPort::Multi(_)
        ) {
            return None;
        }

        // ── P1 fix: parse pid by "member declaration order", not by pid ascending order ──
        // Root cause: BTreeMap(pins / names_to_id) key order ≠ source declaration order.
        // For non-monotonic pin orders like `out [5,2] = VOUT{Vout, GND}`, ascending
        // sort reorders [Vout(5), GND(2)] to [GND(2), Vout(5)], flipping the whole
        // bus mapping lane by lane. Bus.member / Interface names preserve declaration
        // order, so look up pid member by member from it.
        {
            let decl_members: Vec<String> = match port_kind {
                McPinPort::Bus(b) => b.member.clone(),
                McPinPort::Interface(i) => {
                    if let Some((_, m)) = i.name.as_bus() {
                        m
                    } else if i.name.is_list() {
                        i.name.list_members().unwrap_or_default()
                    } else {
                        Vec::new()
                    }
                }
                McPinPort::List(_, members) => members.clone(),
                _ => Vec::new(),
            };

            if decl_members.len() >= 2 {
                let mut ordered: Vec<String> = Vec::new();
                let mut seen: std::collections::BTreeSet<String> =
                    std::collections::BTreeSet::new();
                for m in &decl_members {
                    // Members may be registered as dotted ("VOUT.Vout"), numeric-concat ("GPIO1"), or bare ("CLK")
                    let pid_opt = [
                        format!("{port_name}.{m}"),
                        format!("{port_name}{m}"),
                        m.clone(),
                    ]
                    .iter()
                    .find_map(|key| match self.def.pins.names_to_id.get(key) {
                        Some(McPinPort::Single(id)) => Some(id.clone()),
                        _ => None,
                    });
                    if let Some(pid) = pid_opt {
                        if seen.insert(pid.clone()) {
                            ordered.push(pid);
                        }
                    }
                }
                if ordered.len() >= 2 {
                    // ── [P1-PROBE] delete after verification: compare against ascending order; mismatches are non-monotonic ports being fixed ──
                    let mut asc = ordered.clone();
                    asc.sort_by(|a, b| {
                        a.parse::<i64>()
                            .unwrap_or(0)
                            .cmp(&b.parse::<i64>().unwrap_or(0))
                    });
                    if asc != ordered {
                        eprintln!(
                            "[P1-FIX] {}.{} declaration_order={:?} (ascending would give {:?})",
                            self.name, port_name, ordered, asc
                        );
                    }
                    return Some(ordered);
                }
                // Resolved < 2 → fall through to the original scan fallback below, no regression
            }
        }

        // Direct hit: Multi(pids)
        if let McPinPort::Multi(pids) = port_kind {
            if pids.len() >= 2 {
                let mut sorted = pids.clone();
                sorted.sort_by(|a, b| {
                    let na: i64 = a.parse().unwrap_or(0);
                    let nb: i64 = b.parse().unwrap_or(0);
                    na.cmp(&nb)
                });
                // ── [P1-MULTI-PROBE] delete after verification: for Multi/List ports, if non-monotonic, declaration order ≠ ascending order ──
                // If this prints and is empirically flipped, change the next line to `return Some(pids.clone());` (preserve declaration order).
                if sorted != *pids {
                    eprintln!(
                        "[P1-MULTI-NONMONO] {}.{} declared={:?} ascending={:?}",
                        self.name, port_name, pids, sorted
                    );
                }
                return Some(sorted);
            }
            return None;
        }

        // General case: Scan names_to_id for all dotted name,
        // reverse lookup Single(pid)
        let prefix = format!("{port_name}.");
        let mut pid_with_name: Vec<(String, String)> = Vec::new();
        for (name, port) in self.def.pins.names_to_id.iter() {
            if !name.starts_with(&prefix) {
                continue;
            }
            if let McPinPort::Single(pid) = port {
                pid_with_name.push((name.clone(), pid.clone()));
            }
        }

        if pid_with_name.len() < 2 {
            return None;
        }

        // ── Iter-10.D-fix1: Remove duplicate pids ─────────────────────────────────
        // some components (e.g., lp322dcdc) register same physical pin with multiple
        // dotted name aliases in names_to_id (e.g., `GND.GND` and some other alias both map to
        // pid="2"), this case scan returns N entries but unique pid only 1.
        // This isn't a real bus port — real bus port should have N different physical pins.
        // Before dedup, lp322dcdc.GND was incorrectly expanded to 2 lanes (both pointing to pid 2), causing
        // `@CAP1.2 ~ lp322dcdc.GND` single-pin connection incorrectly expanded to two identical
        // `@CAP1.2 ~ lp322dcdc.2`.
        let unique_pid_count = {
            use std::collections::BTreeSet;
            let set: BTreeSet<&String> = pid_with_name.iter().map(|(_, p)| p).collect();
            set.len()
        };
        if unique_pid_count < 2 {
            return None;
        }

        // Sort pin_ids in ascending order by pid value (e.g., "6","7" not "7","6").
        // This matches the order of pin declarations in mc source code,
        // ensuring stable N×N pin connections.
        pid_with_name.sort_by(|a, b| {
            let na: i64 = a.1.parse().unwrap_or(0);
            let nb: i64 = b.1.parse().unwrap_or(0);
            na.cmp(&nb)
        });

        // ── Iter-11.D-fix2: Remove duplicate pids ─────────────────────────────────
        {
            use std::collections::BTreeSet;
            let mut seen: BTreeSet<String> = BTreeSet::new();
            pid_with_name.retain(|(_, pid)| seen.insert(pid.clone()));
        }

        Some(pid_with_name.into_iter().map(|(_, pid)| pid).collect())
    }
}

impl std::fmt::Display for McComponentInst {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}::{}", self.name, self.def.name)?;
        if self.nc {
            write!(f, "(NC)")?;
        }

        if !self.pins.is_empty() {
            let pins: Vec<String> = self.pins.keys().cloned().collect();
            write!(f, " [{}]", pins.join(", "))?;
        }

        Ok(())
    }
}
