// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::{
    ast::{ast_node::AstNode, c_macros::*},
    builder::diagnostic::dlog_error,
    McIds,
};
use regex::Regex;

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum McUnit {
    Int,
    Hex,
    Float,
    String,
    Volt,
    Amp,
    Cap,
    Ind,
    Time,
    Len,
    Wat,
    Ohm,
    Temp,
    Hz,
    Db,
    Ppm,
    Percent,
    Baud,
    DataSize,
    Sps,
    Siemens,
    Responsivity,
    Angle,
    AngularRate,
    Energy,
    Efield,
    Hfield,
    Flux,
    Bfield,
    Slew,
    Noise,
}

#[derive(Clone, Debug)]
pub struct McUnitValueDeclare {
    pub name: McIds,
    pub unit: McUnit,
    pub default: Option<String>,
}

impl McUnitValueDeclare {
    pub fn new(node: &AstNode) -> Option<Self> {
        let class_node = node.get_sub_node()?;
        let instance_node = class_node.get_next()?;

        if class_node.get_type() != MCAST_CLASS {
            dlog_error(1308, node, "Expected MCAST_CLASS in MCAST_DECLARE_UV.");
            return None;
        }
        if instance_node.get_type() != MCAST_INSTANCE {
            dlog_error(1309, node, "Expected MCAST_INSTANCE in MCAST_DECLARE_UV.");
            return None;
        }

        let unit_node = class_node.get_sub_node()?;
        let unit = McUnit::from_ast(&unit_node)?;

        let name_node = instance_node.get_sub_node()?;
        let name = McIds::new(&name_node)?;

        let default = Self::parse_default_value(&instance_node);

        Some(Self {
            name,
            unit,
            default,
        })
    }

    fn parse_default_value(instance_node: &AstNode) -> Option<String> {
        let name_node = instance_node.get_sub_node()?;
        name_node
            .get_next()
            .map(|default_node| Self::node_to_string(&default_node))
    }

    fn node_to_string(node: &AstNode) -> String {
        match node.get_type() {
            MCAST_STRING => {
                unsafe {
                    let c_str =
                        std::ffi::CStr::from_ptr(node.get_data() as *const std::ffi::c_char);
                    if let Ok(str_value) = c_str.to_str() {
                        if str_value.starts_with('"')
                            && str_value.ends_with('"')
                            && str_value.len() >= 2
                        {
                            return str_value[1..str_value.len() - 1].to_string();
                        }
                        return str_value.to_string();
                    }
                }
                String::new()
            }
            MCAST_INT | MCAST_HEX => {
                unsafe {
                    let c_str =
                        std::ffi::CStr::from_ptr(node.get_data() as *const std::ffi::c_char);
                    if let Ok(str_value) = c_str.to_str() {
                        return str_value.to_string();
                    }
                }
                String::new()
            }
            MCAST_FLOAT => {
                unsafe {
                    let c_str =
                        std::ffi::CStr::from_ptr(node.get_data() as *const std::ffi::c_char);
                    if let Ok(str_value) = c_str.to_str() {
                        return str_value.to_string();
                    }
                }
                String::new()
            }
            MCAST_CONST => {
                unsafe {
                    let c_str =
                        std::ffi::CStr::from_ptr(node.get_data() as *const std::ffi::c_char);
                    if let Ok(str_value) = c_str.to_str() {
                        return str_value.to_string();
                    }
                }
                String::new()
            }
            _ => node.to_string().unwrap_or_default(),
        }
    }
}
impl std::fmt::Display for McUnitValueDeclare {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}::{}", self.name, self.unit)
    }
}

#[derive(Clone)]
pub struct McUnitValue {
    plusminus: bool,
    value: f64,
    unit: McUnit,
    at: Option<Box<McUnitValue>>,
}

impl McUnitValue {
    pub fn new(child_node: &AstNode) -> Option<Self> {
        let Some(child_node) = child_node.get_sub_node() else {
            dlog_error(1800, child_node, "missing unit value data node.");
            return None;
        };

        let data_ptr = child_node.get_data() as *const i8;
        let Ok(data_str) = (unsafe { std::ffi::CStr::from_ptr(data_ptr).to_str() }) else {
            dlog_error(303, &child_node, "Invalid unit value data node.");
            return None;
        };

        match child_node.get_type() {
            MCAST_UVAL_VOLT => parse_volt_unit(&child_node, data_str),
            MCAST_UVAL_AMP => parse_amp_unit(&child_node, data_str),
            MCAST_UVAL_CAP => parse_capc_unit(&child_node, data_str),
            MCAST_UVAL_IND => parse_induct_unit(&child_node, data_str),
            MCAST_UVAL_TIME => parse_time_unit(&child_node, data_str),
            MCAST_UVAL_LEN => parse_length_unit(&child_node, data_str),
            MCAST_UVAL_WAT => parse_power_unit(&child_node, data_str),
            MCAST_UVAL_OHM => parse_resist_unit(&child_node, data_str),
            MCAST_UVAL_TEMP => parse_temperature_unit(&child_node, data_str),
            MCAST_UVAL_HZ => parse_freq_unit(&child_node, data_str),
            MCAST_UVAL_DB => parse_gain_unit(&child_node, data_str),
            MCAST_UVAL_PPM => parse_ppm_unit(&child_node, data_str),
            MCAST_UVAL_PERCENT => parse_percent_unit(&child_node, data_str),
            MCAST_UVAL_BAUD => parse_comm_speed_unit(&child_node, data_str),
            MCAST_UVAL_DATASIZE => parse_data_size_unit(&child_node, data_str),
            MCAST_UVAL_SPS => parse_sps_unit(&child_node, data_str),
            MCAST_UVAL_SIEMENS => parse_conductance_unit(&child_node, data_str),
            MCAST_UVAL_RESPONSIVITY => parse_responsivity_unit(&child_node, data_str),
            MCAST_UVAL_ANGLE => parse_angle_unit(&child_node, data_str),
            MCAST_UVAL_ANGULAR_RATE => parse_angular_rate_unit(&child_node, data_str),
            MCAST_UVAL_ENERGY => parse_energy_unit(&child_node, data_str),
            MCAST_UVAL_EFIELD => parse_efield_unit(&child_node, data_str),
            MCAST_UVAL_HFIELD => parse_hfield_unit(&child_node, data_str),
            MCAST_UVAL_FLUX => parse_flux_unit(&child_node, data_str),
            MCAST_UVAL_BFIELD => parse_bfield_unit(&child_node, data_str),
            MCAST_UVAL_SLEW => parse_slew_unit(&child_node, data_str),
            MCAST_UVAL_NOISE => parse_noise_unit(&child_node, data_str),
            _ => {
                dlog_error(302, &child_node, "Invalid unit value type.");
                None
            }
        }
    }

    pub fn unit(&self) -> &McUnit {
        &self.unit
    }

    pub fn value(&self) -> f64 {
        self.value
    }
}

impl std::fmt::Debug for McUnitValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "McUnitValue({} {})", self.value, self.unit)
    }
}

fn extract_value_and_unit<'a>(node: &'a AstNode, data: &'a str) -> Option<(f64, &'a str)> {
    let re = Regex::new(r"^([+-]?\d*\.?\d+(?:[eE][+-]?\d+)?)(.*)$").unwrap();

    let Some(captures) = re.captures(data) else {
        dlog_error(1803, node, "Invalid unit value format.");
        return None;
    };
    let Some(value_str) = captures.get(1) else {
        dlog_error(304, node, "Invalid value.");
        return None;
    };
    let Some(unit_str) = captures.get(2) else {
        dlog_error(304, node, "Invalid unit.");
        return None;
    };
    let Ok(value) = value_str.as_str().parse::<f64>() else {
        dlog_error(1803, node, "Invalid float format.");
        return None;
    };
    Some((value, unit_str.as_str()))
}

fn parse_volt_unit(node: &AstNode, data: &str) -> Option<McUnitValue> {
    let (value, unit_str) = extract_value_and_unit(node, data)?;

    let multiplier = match unit_str {
        "V" => 1.0,
        "mV" => 1e-3,
        "μV" | "µV" | "uV" => 1e-6,
        "nV" => 1e-9,
        "pV" => 1e-12,
        "fV" => 1e-15,
        "kV" | "KV" => 1e3,
        "MV" => 1e6,
        "GV" => 1e9,
        _ => {
            dlog_error(1804, node, "Invalid Unit.");
            return None;
        }
    };

    Some(McUnitValue {
        plusminus: false,
        value: value * multiplier,
        unit: McUnit::Volt,
        at: None,
    })
}

fn parse_amp_unit(node: &AstNode, data: &str) -> Option<McUnitValue> {
    let (value, unit_str) = extract_value_and_unit(node, data)?;

    let multiplier = match unit_str {
        "A" => 1.0,
        "mA" => 1e-3,
        "μA" | "µA" | "uA" => 1e-6,
        "nA" => 1e-9,
        "pA" => 1e-12,
        "fA" => 1e-15,
        "kA" | "KA" => 1e3,
        "MA" => 1e6,
        "GA" => 1e9,
        _ => {
            dlog_error(1804, node, "Invalid Unit.");
            return None;
        }
    };

    Some(McUnitValue {
        plusminus: false,
        value: value * multiplier,
        unit: McUnit::Amp,
        at: None,
    })
}

fn parse_capc_unit(node: &AstNode, data: &str) -> Option<McUnitValue> {
    let (value, unit_str) = extract_value_and_unit(node, data)?;

    let multiplier = match unit_str {
        "F" => 1.0,
        "mF" => 1e-3,
        "μF" | "µF" | "uF" => 1e-6,
        "nF" => 1e-9,
        "pF" => 1e-12,
        "kF" | "KF" => 1e3,
        "MF" => 1e6,
        "GF" => 1e9,
        _ => {
            dlog_error(305, node, "Invalid Unit.");
            return None;
        }
    };

    Some(McUnitValue {
        plusminus: false,
        value: value * multiplier,
        unit: McUnit::Cap,
        at: None,
    })
}

fn parse_induct_unit(node: &AstNode, data: &str) -> Option<McUnitValue> {
    let (value, unit_str) = extract_value_and_unit(node, data)?;

    let multiplier = match unit_str {
        "H" => 1.0,
        "mH" => 1e-3,
        "μH" | "µH" | "uH" => 1e-6,
        "nH" => 1e-9,
        "pH" => 1e-12,
        "kH" | "KH" => 1e3,
        "MH" => 1e6,
        "GH" => 1e9,
        _ => {
            dlog_error(305, node, "Invalid Unit.");
            return None;
        }
    };

    Some(McUnitValue {
        plusminus: false,
        value: value * multiplier,
        unit: McUnit::Ind,
        at: None,
    })
}

fn parse_time_unit(node: &AstNode, data: &str) -> Option<McUnitValue> {
    let (value, unit_str) = extract_value_and_unit(node, data)?;

    let multiplier = match unit_str {
        "s" => 1.0,
        "ms" => 1e-3,
        "μs" | "µs" | "us" => 1e-6,
        "ns" => 1e-9,
        "ps" => 1e-12,
        "fs" => 1e-15,
        "ks" | "Ks" => 1e3,
        "Ms" => 1e6,
        "Gs" => 1e9,
        "min" => 60.0,
        "h" | "hr" => 3600.0,
        _ => {
            dlog_error(305, node, "Invalid Unit.");
            return None;
        }
    };

    Some(McUnitValue {
        plusminus: false,
        value: value * multiplier,
        unit: McUnit::Time,
        at: None,
    })
}

fn parse_length_unit(node: &AstNode, data: &str) -> Option<McUnitValue> {
    let (value, unit_str) = extract_value_and_unit(node, data)?;

    let multiplier = match unit_str {
        "m" => 1.0,
        "dm" => 1e-1,
        "cm" => 1e-2,
        "mm" => 1e-3,
        "μm" | "µm" | "um" => 1e-6,
        "nm" => 1e-9,
        "pm" => 1e-12,
        "fm" => 1e-15,
        "km" => 1e3,
        "in" | "inch" | "inches" => 0.0254, // 1 inch = 0.0254 meters
        "mil" | "mils" => 25.4e-6,          // 1 mil = 25.4 microns = 25.4e-6 meters
        "ft" => 0.3048,                     // 1 foot = 0.3048 meters
        "yd" => 0.9144,                     // 1 yard = 0.9144 meters
        _ => {
            dlog_error(305, node, "Invalid Unit.");
            return None;
        }
    };

    Some(McUnitValue {
        plusminus: false,
        value: value * multiplier,
        unit: McUnit::Len,
        at: None,
    })
}

fn parse_power_unit(node: &AstNode, data: &str) -> Option<McUnitValue> {
    let (value, unit_str) = extract_value_and_unit(node, data)?;

    let multiplier = match unit_str {
        // Power units
        "W" => 1.0,
        "mW" => 1e-3,
        "μW" | "µW" | "uW" => 1e-6,
        "nW" => 1e-9,
        "kW" | "KW" => 1e3,
        "MW" => 1e6,
        "GW" => 1e9,
        // Apparent power units
        "VA" => 1.0,
        "mVA" => 1e-3,
        "kVA" => 1e3,
        // Reactive power units
        "VAR" | "var" => 1.0,
        "mVAR" | "mvar" => 1e-3,
        "kVAR" | "kvar" => 1e3,
        // Energy units
        "Wh" => 1.0,
        "kWh" => 1e3,
        "MWh" => 1e6,
        _ => {
            dlog_error(305, node, "Invalid Unit.");
            return None;
        }
    };

    Some(McUnitValue {
        plusminus: false,
        value: value * multiplier,
        unit: McUnit::Wat,
        at: None,
    })
}

fn parse_resist_unit(node: &AstNode, data: &str) -> Option<McUnitValue> {
    let (value, unit_str) = extract_value_and_unit(node, data)?;

    let multiplier = match unit_str {
        "R" | "Ω" => 1.0,
        "mR" | "mΩ" => 1e-3,
        "μR" | "µR" | "uR" | "μΩ" | "µΩ" | "uΩ" => 1e-6,
        "nR" | "nΩ" => 1e-9,
        "kR" | "kΩ" => 1e3,
        "MR" | "MΩ" => 1e6,
        "GR" | "GΩ" => 1e9,
        _ => {
            dlog_error(305, node, "Invalid Unit.");
            return None;
        }
    };

    Some(McUnitValue {
        plusminus: false,
        value: value * multiplier,
        unit: McUnit::Ohm,
        at: None,
    })
}

fn parse_temperature_unit(node: &AstNode, data: &str) -> Option<McUnitValue> {
    let (value, unit_str) = extract_value_and_unit(node, data)?;

    let (converted_value, unit) = match unit_str {
        "℃" | "°C" | "degC" => (value, McUnit::Temp),
        "℉" | "°F" | "degF" => ((value - 32.0) * 5.0 / 9.0, McUnit::Temp),
        "K" => (value - 273.15, McUnit::Temp), // Convert Kelvin to Celsius
        _ => {
            dlog_error(305, node, "Invalid Unit.");
            return None;
        }
    };

    Some(McUnitValue {
        plusminus: false,
        value: converted_value,
        unit,
        at: None,
    })
}

fn parse_freq_unit(node: &AstNode, data: &str) -> Option<McUnitValue> {
    let (value, unit_str) = extract_value_and_unit(node, data)?;

    let multiplier = match unit_str {
        "Hz" => 1.0,
        "mHz" => 1e-3,
        "μHz" | "µHz" | "uHz" => 1e-6,
        "nHz" => 1e-9,
        "kHz" => 1e3,
        "MHz" => 1e6,
        "GHz" => 1e9,
        "THz" => 1e12,
        _ => {
            dlog_error(305, node, "Invalid Unit.");
            return None;
        }
    };

    Some(McUnitValue {
        plusminus: false,
        value: value * multiplier,
        unit: McUnit::Hz,
        at: None,
    })
}

fn parse_gain_unit(node: &AstNode, data: &str) -> Option<McUnitValue> {
    let (value, unit_str) = extract_value_and_unit(node, data)?;

    Some(McUnitValue {
        plusminus: false,
        value,
        unit: match unit_str {
            "dB" | "dBm" | "dBw" | "dBi" | "dBd" | "dBc" | "dBV" | "dBu" | "dBFS" | "dBμV"
            | "dBµV" | "dBuV" => McUnit::Db,
            _ => {
                dlog_error(305, node, "Invalid Unit.");
                return None;
            }
        },
        at: None,
    })
}

fn parse_ppm_unit(node: &AstNode, data: &str) -> Option<McUnitValue> {
    let (value, unit_str) = extract_value_and_unit(node, data)?;

    let multiplier = match unit_str {
        "ppm" => 1.0,
        "ppb" => 1e-3, // Parts per billion
        "ppt" => 1e-6, // Parts per trillion
        "ppq" => 1e-9, // Parts per quadrillion
        _ => {
            dlog_error(305, node, "Invalid Unit.");
            return None;
        }
    };

    Some(McUnitValue {
        plusminus: false,
        value: value * multiplier,
        unit: McUnit::Ppm,
        at: None,
    })
}

fn parse_percent_unit(node: &AstNode, data: &str) -> Option<McUnitValue> {
    let (value, unit_str) = extract_value_and_unit(node, data)?;

    let multiplier = match unit_str {
        "%" => 1.0,   // Percent (10^-2)
        "‰" => 1e-3,  // Permille (10^-3)
        "‱" => 1e-4,  // Per ten thousand (10^-4)
        "%RH" => 1.0, // Relative humidity (percent)
        _ => {
            dlog_error(305, node, "Invalid Unit.");
            return None;
        }
    };

    Some(McUnitValue {
        plusminus: false,
        value: value * multiplier,
        unit: McUnit::Percent,
        at: None,
    })
}

fn parse_comm_speed_unit(node: &AstNode, data: &str) -> Option<McUnitValue> {
    let (value, unit_str) = extract_value_and_unit(node, data)?;

    let multiplier = match unit_str {
        "bps" => 1.0,   // Bits per second
        "Bps" => 8.0,   // Bytes per second (1 Byte = 8 bits)
        "kbps" => 1e3,  // Kilobits per second
        "kBps" => 8e3,  // Kilobytes per second
        "Mbps" => 1e6,  // Megabits per second
        "MBps" => 8e6,  // Megabytes per second
        "Gbps" => 1e9,  // Gigabits per second
        "GBps" => 8e9,  // Gigabytes per second
        "Tbps" => 1e12, // Terabits per second
        "TBps" => 8e12, // Terabytes per second
        "sym/s" => 1.0, // Symbols per second
        _ => {
            dlog_error(305, node, "Invalid Unit.");
            return None;
        }
    };

    Some(McUnitValue {
        plusminus: false,
        value: value * multiplier,
        unit: McUnit::Baud,
        at: None,
    })
}

fn parse_data_size_unit(node: &AstNode, data: &str) -> Option<McUnitValue> {
    let (value, unit_str) = extract_value_and_unit(node, data)?;

    let multiplier = match unit_str {
        // Basic units
        "b" | "bit" => 0.125, // Bit (1/8 Byte)
        "B" | "Byte" => 1.0,  // Byte

        // Decimal prefix (10^3)
        "kb" => 1.25e2,  // Kilobit (10^3 bits)
        "kB" => 1e3,     // Kilobyte (10^3 bytes)
        "Mb" => 1.25e5,  // Megabit (10^6 bits)
        "MB" => 1e6,     // Megabyte (10^6 bytes)
        "Gb" => 1.25e8,  // Gigabit (10^9 bits)
        "GB" => 1e9,     // Gigabyte (10^9 bytes)
        "Tb" => 1.25e11, // Terabit (10^12 bits)
        "TB" => 1e12,    // Terabyte (10^12 bytes)
        "Pb" => 1.25e14, // Petabit (10^15 bits)
        "PB" => 1e15,    // Petabyte (10^15 bytes)
        "Eb" => 1.25e17, // Exabit (10^18 bits)
        "EB" => 1e18,    // Exabyte (10^18 bytes)

        // Binary prefix (2^10)
        "Kib" | "KiB" => 1024.0,                   // Kibibyte (2^10 bytes)
        "Mib" | "MiB" => 1024.0 * 1024.0,          // Mebibyte (2^20 bytes)
        "Gib" | "GiB" => 1024.0 * 1024.0 * 1024.0, // Gibibyte (2^30 bytes)
        "Tib" | "TiB" => 1024.0 * 1024.0 * 1024.0 * 1024.0, // Tebibyte (2^40 bytes)
        "Pib" | "PiB" => 1024.0 * 1024.0 * 1024.0 * 1024.0 * 1024.0, // Pebibyte (2^50 bytes)
        "Eib" | "EiB" => 1024.0 * 1024.0 * 1024.0 * 1024.0 * 1024.0 * 1024.0, // Exbibyte (2^60 bytes)

        _ => {
            dlog_error(305, node, "Invalid Unit.");
            return None;
        }
    };

    Some(McUnitValue {
        plusminus: false,
        value: value * multiplier,
        unit: McUnit::DataSize,
        at: None,
    })
}

fn parse_sps_unit(node: &AstNode, data: &str) -> Option<McUnitValue> {
    let (value, unit_str) = extract_value_and_unit(node, data)?;

    let multiplier = match unit_str {
        // Sampling rate units (Samples Per Second)
        "SPS" => 1.0,
        "kSPS" => 1e3,
        "MSPS" => 1e6,
        "GSPS" => 1e9,
        "TSPS" => 1e12,

        // Lowercase form
        "sps" => 1.0,
        "ksps" => 1e3,
        "Msps" => 1e6,
        "Gsps" => 1e9,

        // Sa/s form (Samples per second)
        "Sa/s" => 1.0,
        "kSa/s" => 1e3,
        "MSa/s" => 1e6,
        "GSa/s" => 1e9,

        _ => {
            dlog_error(305, node, "Invalid Unit.");
            return None;
        }
    };

    Some(McUnitValue {
        plusminus: false,
        value: value * multiplier,
        unit: McUnit::Sps,
        at: None,
    })
}

impl McUnit {
    pub fn name(&self) -> &str {
        match self {
            McUnit::Int => "Integer (INT)",
            McUnit::Hex => "Hexadecimal (HEX)",
            McUnit::Float => "Float (FLOAT)",
            McUnit::String => "String (STRING)",
            McUnit::Volt => "Volt (V)",
            McUnit::Amp => "Ampere (A)",
            McUnit::Cap => "Farad (F)",
            McUnit::Ind => "Henry (H)",
            McUnit::Time => "second (s)",
            McUnit::Len => "meter (m)",
            McUnit::Wat => "Watt (W)",
            McUnit::Ohm => "Ohm (Ω)",
            McUnit::Temp => "degree Celsius (°C)",
            McUnit::Hz => "Hertz (Hz)",
            McUnit::Db => "decibel (dB)",
            McUnit::Ppm => "parts per million (ppm)",
            McUnit::Percent => "percent (%)",
            McUnit::Baud => "bits per second (bps)",
            McUnit::DataSize => "byte (B)",
            McUnit::Sps => "samples per second (SPS)",
            McUnit::Siemens => "Siemens (S)",
            McUnit::Responsivity => "Ampere per Watt (A/W)",
            McUnit::Angle => "Radian (rad)",
            McUnit::AngularRate => "Radian per second (rad/s)",
            McUnit::Energy => "Joule (J)",
            McUnit::Efield => "Volt per meter (V/m)",
            McUnit::Hfield => "Ampere per meter (A/m)",
            McUnit::Flux => "Weber (Wb)",
            McUnit::Bfield => "Tesla (T)",
            McUnit::Slew => "Volt per microsecond (V/μs)",
            McUnit::Noise => "Noise Density",
        }
    }

    /// Create McUnit from AST node
    pub fn from_ast(node: &AstNode) -> Option<Self> {
        match node.get_type() {
            MCAST_UNIT_INT => Some(McUnit::Int),
            MCAST_UNIT_HEX => Some(McUnit::Hex),
            MCAST_UNIT_FLOAT => Some(McUnit::Float),
            MCAST_UNIT_STRING => Some(McUnit::String),
            MCAST_UNIT_VOLT => Some(McUnit::Volt),
            MCAST_UNIT_AMP => Some(McUnit::Amp),
            MCAST_UNIT_CAP => Some(McUnit::Cap),
            MCAST_UNIT_IND => Some(McUnit::Ind),
            MCAST_UNIT_TIME => Some(McUnit::Time),
            MCAST_UNIT_LEN => Some(McUnit::Len),
            MCAST_UNIT_WAT => Some(McUnit::Wat),
            MCAST_UNIT_OHM => Some(McUnit::Ohm),
            MCAST_UNIT_TEMP => Some(McUnit::Temp),
            MCAST_UNIT_HZ => Some(McUnit::Hz),
            MCAST_UNIT_DB => Some(McUnit::Db),
            MCAST_UNIT_PPM => Some(McUnit::Ppm),
            MCAST_UNIT_PERCENT => Some(McUnit::Percent),
            MCAST_UNIT_BAUD => Some(McUnit::Baud),
            MCAST_UNIT_DATASIZE => Some(McUnit::DataSize),
            MCAST_UNIT_SPS => Some(McUnit::Sps),
            MCAST_UNIT_SIEMENS => Some(McUnit::Siemens),
            MCAST_UNIT_RESPONSIVITY => Some(McUnit::Responsivity),
            MCAST_UNIT_ANGLE => Some(McUnit::Angle),
            MCAST_UNIT_ANGULAR_RATE => Some(McUnit::AngularRate),
            MCAST_UNIT_ENERGY => Some(McUnit::Energy),
            MCAST_UNIT_EFIELD => Some(McUnit::Efield),
            MCAST_UNIT_HFIELD => Some(McUnit::Hfield),
            MCAST_UNIT_FLUX => Some(McUnit::Flux),
            MCAST_UNIT_BFIELD => Some(McUnit::Bfield),
            MCAST_UNIT_SLEW => Some(McUnit::Slew),
            MCAST_UNIT_NOISE => Some(McUnit::Noise),
            _ => None,
        }
    }
}

impl std::fmt::Display for McUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McUnit::Int => write!(f, "INT"),
            McUnit::Hex => write!(f, "HEX"),
            McUnit::Float => write!(f, "FLOAT"),
            McUnit::String => write!(f, "STRING"),
            McUnit::Volt => write!(f, "V"),
            McUnit::Amp => write!(f, "A"),
            McUnit::Cap => write!(f, "F"),
            McUnit::Ind => write!(f, "H"),
            McUnit::Time => write!(f, "s"),
            McUnit::Len => write!(f, "m"),
            McUnit::Wat => write!(f, "W"),
            McUnit::Ohm => write!(f, "Ω"),
            McUnit::Temp => write!(f, "°C"),

            McUnit::Hz => write!(f, "Hz"),
            McUnit::Db => write!(f, "dB"),
            McUnit::Ppm => write!(f, "ppm"),
            McUnit::Percent => write!(f, "%"),
            McUnit::Baud => write!(f, "bps"),
            McUnit::DataSize => write!(f, "B"),
            McUnit::Sps => write!(f, "SPS"),
            McUnit::Siemens => write!(f, "S"),
            McUnit::Responsivity => write!(f, "A/W"),
            McUnit::Angle => write!(f, "rad"),
            McUnit::AngularRate => write!(f, "rad/s"),
            McUnit::Energy => write!(f, "J"),
            McUnit::Efield => write!(f, "V/m"),
            McUnit::Hfield => write!(f, "A/m"),
            McUnit::Flux => write!(f, "Wb"),
            McUnit::Bfield => write!(f, "T"),
            McUnit::Slew => write!(f, "V/μs"),
            McUnit::Noise => write!(f, "nV/√Hz"),
        }
    }
}

impl std::fmt::Display for McUnitValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.value == 0.0 {
            return write!(f, "0{}", self.unit);
        }

        let abs_val = self.value.abs();

        match self.unit {
            McUnit::Cap => Self::format_capacitance(f, self.value),
            McUnit::Ind => Self::format_inductance(f, self.value),
            McUnit::Ohm => Self::format_resistance(f, self.value),
            _ => {
                if abs_val >= 1000.0 {
                    write!(f, "{:.1}{}", self.value, self.unit)
                } else if abs_val >= 1.0 {
                    write!(f, "{:.2}{}", self.value, self.unit)
                } else if abs_val >= 0.01 {
                    write!(f, "{:.3}{}", self.value, self.unit)
                } else if abs_val >= 0.0001 {
                    write!(f, "{:.6}{}", self.value, self.unit)
                } else {
                    write!(f, "{:e}{}", self.value, self.unit)
                }
            }
        }
    }
}

impl McUnitValue {
    fn format_capacitance(f: &mut std::fmt::Formatter<'_>, value: f64) -> std::fmt::Result {
        let abs_val = value.abs();
        let sign = if value < 0.0 { "-" } else { "" };

        if abs_val >= 1.0 {
            write!(f, "{sign}{abs_val:.2}F")
        } else if abs_val >= 1e-3 {
            write!(f, "{}{:.2}mF", sign, abs_val * 1e3)
        } else if abs_val >= 1e-6 {
            write!(f, "{}{:.2}µF", sign, abs_val * 1e6)
        } else if abs_val >= 1e-9 {
            write!(f, "{}{:.2}nF", sign, abs_val * 1e9)
        } else {
            write!(f, "{}{:.2}pF", sign, abs_val * 1e12)
        }
    }

    fn format_inductance(f: &mut std::fmt::Formatter<'_>, value: f64) -> std::fmt::Result {
        let abs_val = value.abs();
        let sign = if value < 0.0 { "-" } else { "" };

        if abs_val >= 1.0 {
            write!(f, "{sign}{abs_val:.2}H")
        } else if abs_val >= 1e-3 {
            write!(f, "{}{:.2}mH", sign, abs_val * 1e3)
        } else if abs_val >= 1e-6 {
            write!(f, "{}{:.2}µH", sign, abs_val * 1e6)
        } else if abs_val >= 1e-9 {
            write!(f, "{}{:.2}nH", sign, abs_val * 1e9)
        } else {
            write!(f, "{}{:.2}pH", sign, abs_val * 1e12)
        }
    }

    fn format_resistance(f: &mut std::fmt::Formatter<'_>, value: f64) -> std::fmt::Result {
        let abs_val = value.abs();
        let sign = if value < 0.0 { "-" } else { "" };

        if abs_val >= 1e6 {
            write!(f, "{}{:.2}MΩ", sign, abs_val * 1e-6)
        } else if abs_val >= 1e3 {
            write!(f, "{}{:.2}kΩ", sign, abs_val * 1e-3)
        } else {
            write!(f, "{sign}{abs_val:.2}Ω")
        }
    }
}

fn parse_conductance_unit(node: &AstNode, data: &str) -> Option<McUnitValue> {
    let (value, unit_str) = extract_value_and_unit(node, data)?;

    let multiplier = match unit_str {
        "S" => 1.0,
        "mS" => 1e-3,
        "μS" | "µS" | "uS" => 1e-6,
        "nS" => 1e-9,
        "pS" => 1e-12,
        "kS" => 1e3,
        "MS" => 1e6,
        "GS" => 1e9,
        _ => {
            dlog_error(305, node, "Invalid Unit.");
            return None;
        }
    };

    Some(McUnitValue {
        plusminus: false,
        value: value * multiplier,
        unit: McUnit::Siemens,
        at: None,
    })
}

fn parse_responsivity_unit(node: &AstNode, data: &str) -> Option<McUnitValue> {
    let (value, unit_str) = extract_value_and_unit(node, data)?;

    if unit_str == "A/W" {
        Some(McUnitValue {
            plusminus: false,
            value,
            unit: McUnit::Responsivity,
            at: None,
        })
    } else if unit_str == "mA/W" {
        Some(McUnitValue {
            plusminus: false,
            value: value * 1e-3,
            unit: McUnit::Responsivity,
            at: None,
        })
    } else if unit_str == "μA/W" || unit_str == "µA/W" || unit_str == "uA/W" {
        Some(McUnitValue {
            plusminus: false,
            value: value * 1e-6,
            unit: McUnit::Responsivity,
            at: None,
        })
    } else if unit_str == "nA/W" {
        Some(McUnitValue {
            plusminus: false,
            value: value * 1e-9,
            unit: McUnit::Responsivity,
            at: None,
        })
    } else if unit_str == "kA/W" {
        Some(McUnitValue {
            plusminus: false,
            value: value * 1e3,
            unit: McUnit::Responsivity,
            at: None,
        })
    } else {
        dlog_error(305, node, "Invalid Unit.");
        return None;
    }
}

fn parse_angle_unit(node: &AstNode, data: &str) -> Option<McUnitValue> {
    let (value, unit_str) = extract_value_and_unit(node, data)?;

    match unit_str {
        "rad" => Some(McUnitValue {
            plusminus: false,
            value,
            unit: McUnit::Angle,
            at: None,
        }),
        "deg" | "°" => Some(McUnitValue {
            plusminus: false,
            value: value * std::f64::consts::PI / 180.0, // Convert degrees to radians
            unit: McUnit::Angle,
            at: None,
        }),
        _ => {
            dlog_error(1804, node, "Invalid angle unit.");
            None
        }
    }
}

fn parse_angular_rate_unit(node: &AstNode, data: &str) -> Option<McUnitValue> {
    let (value, unit_str) = extract_value_and_unit(node, data)?;

    match unit_str {
        "rad/s" => Some(McUnitValue {
            plusminus: false,
            value,
            unit: McUnit::AngularRate,
            at: None,
        }),
        "deg/s" => Some(McUnitValue {
            plusminus: false,
            value: value * std::f64::consts::PI / 180.0, // Convert degrees to radians
            unit: McUnit::AngularRate,
            at: None,
        }),
        "rpm" => Some(McUnitValue {
            plusminus: false,
            value: value * 2.0 * std::f64::consts::PI / 60.0, // Convert rpm to rad/s
            unit: McUnit::AngularRate,
            at: None,
        }),
        "rps" => Some(McUnitValue {
            plusminus: false,
            value: value * 2.0 * std::f64::consts::PI, // Convert rps to rad/s
            unit: McUnit::AngularRate,
            at: None,
        }),
        _ => {
            dlog_error(1804, node, "Invalid angular rate unit.");
            None
        }
    }
}

fn parse_energy_unit(node: &AstNode, data: &str) -> Option<McUnitValue> {
    let (value, unit_str) = extract_value_and_unit(node, data)?;

    let multiplier = match unit_str {
        "J" => 1.0,
        "mJ" => 1e-3,
        "kJ" => 1e3,
        _ => {
            dlog_error(305, node, "Invalid energy unit.");
            return None;
        }
    };

    Some(McUnitValue {
        plusminus: false,
        value: value * multiplier,
        unit: McUnit::Energy,
        at: None,
    })
}

fn parse_efield_unit(node: &AstNode, data: &str) -> Option<McUnitValue> {
    let (value, unit_str) = extract_value_and_unit(node, data)?;

    let multiplier = match unit_str {
        "V/m" => 1.0,
        "mV/m" => 1e-3,
        _ => {
            dlog_error(305, node, "Invalid electric field unit.");
            return None;
        }
    };

    Some(McUnitValue {
        plusminus: false,
        value: value * multiplier,
        unit: McUnit::Efield,
        at: None,
    })
}

fn parse_hfield_unit(node: &AstNode, data: &str) -> Option<McUnitValue> {
    let (value, unit_str) = extract_value_and_unit(node, data)?;

    let multiplier = match unit_str {
        "A/m" => 1.0,
        "mA/m" => 1e-3,
        _ => {
            dlog_error(305, node, "Invalid magnetic field strength unit.");
            return None;
        }
    };

    Some(McUnitValue {
        plusminus: false,
        value: value * multiplier,
        unit: McUnit::Hfield,
        at: None,
    })
}

fn parse_flux_unit(node: &AstNode, data: &str) -> Option<McUnitValue> {
    let (value, unit_str) = extract_value_and_unit(node, data)?;

    let multiplier = match unit_str {
        "Wb" => 1.0,
        "mWb" => 1e-3,
        "μWb" | "µWb" | "uWb" => 1e-6,
        _ => {
            dlog_error(1804, node, "Invalid magnetic flux unit.");
            return None;
        }
    };

    Some(McUnitValue {
        plusminus: false,
        value: value * multiplier,
        unit: McUnit::Flux,
        at: None,
    })
}

fn parse_bfield_unit(node: &AstNode, data: &str) -> Option<McUnitValue> {
    let (value, unit_str) = extract_value_and_unit(node, data)?;

    let multiplier = match unit_str {
        "T" => 1.0,
        "mT" => 1e-3,
        "μT" | "µT" | "uT" => 1e-6,
        "G" => 1e-4, // 1 Gauss = 1e-4 Tesla
        _ => {
            dlog_error(1804, node, "Invalid magnetic flux density unit.");
            return None;
        }
    };

    Some(McUnitValue {
        plusminus: false,
        value: value * multiplier,
        unit: McUnit::Bfield,
        at: None,
    })
}

fn parse_slew_unit(node: &AstNode, data: &str) -> Option<McUnitValue> {
    let (value, unit_str) = extract_value_and_unit(node, data)?;

    let multiplier = match unit_str {
        "V/μs" | "V/µs" | "V/us" => 1e6, // V/μs to V/s
        "A/μs" | "A/µs" | "A/us" => 1e6, // A/μs to A/s
        _ => {
            dlog_error(1804, node, "Invalid slew rate unit.");
            return None;
        }
    };

    Some(McUnitValue {
        plusminus: false,
        value: value * multiplier,
        unit: McUnit::Slew,
        at: None,
    })
}

fn parse_noise_unit(node: &AstNode, data: &str) -> Option<McUnitValue> {
    let (value, _unit_str) = extract_value_and_unit(node, data)?;

    // For noise density, we just store the value as-is with the appropriate unit
    Some(McUnitValue {
        plusminus: false,
        value,
        unit: McUnit::Noise,
        at: None,
    })
}
