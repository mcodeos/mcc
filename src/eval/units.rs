// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Unit suffix table — the single truth source for unit text (doc/eval V4/V7).
//!
//! Two entry points read this table and nothing else:
//!
//!   - the AST path (`semantic::basic::mc_uval`) filters rows by the value
//!     node's own family, so its per-family validation is unchanged;
//!   - the text path ([`parse_text`]) resolves the family from the suffix
//!     alone, so a value that arrives as text (a bound argument, a condition
//!     operand) is normalized by exactly the same numbers.
//!
//! Every suffix is matched exactly (case-sensitive) and belongs to exactly one
//! family — `eval__suffixes_are_collision_free` locks that, so `50mV` can never
//! mean two things depending on which door it came through.

use crate::semantic::basic::mc_uval::McUnit;

/// One accepted suffix. A value normalizes to `value * factor + offset` in the
/// family's canonical unit; the offset is non-zero only for the affine family
/// (temperature).
pub struct UnitSuffix {
    pub text: &'static str,
    pub unit: McUnit,
    pub factor: f64,
    pub offset: f64,
}

const fn sfx(text: &'static str, unit: McUnit, factor: f64) -> UnitSuffix {
    UnitSuffix {
        text,
        unit,
        factor,
        offset: 0.0,
    }
}

const fn sfx_off(text: &'static str, unit: McUnit, factor: f64, offset: f64) -> UnitSuffix {
    UnitSuffix {
        text,
        unit,
        factor,
        offset,
    }
}

/// Every suffix the language accepts, grouped by family for review.
pub static UNIT_SUFFIXES: &[UnitSuffix] = &[
    // Volt
    sfx("V", McUnit::Volt, 1.0),
    sfx("mV", McUnit::Volt, 1e-3),
    sfx("μV", McUnit::Volt, 1e-6),
    sfx("µV", McUnit::Volt, 1e-6),
    sfx("uV", McUnit::Volt, 1e-6),
    sfx("nV", McUnit::Volt, 1e-9),
    sfx("pV", McUnit::Volt, 1e-12),
    sfx("fV", McUnit::Volt, 1e-15),
    sfx("kV", McUnit::Volt, 1e3),
    sfx("KV", McUnit::Volt, 1e3),
    sfx("MV", McUnit::Volt, 1e6),
    sfx("GV", McUnit::Volt, 1e9),

    // Amp
    sfx("A", McUnit::Amp, 1.0),
    sfx("mA", McUnit::Amp, 1e-3),
    sfx("μA", McUnit::Amp, 1e-6),
    sfx("µA", McUnit::Amp, 1e-6),
    sfx("uA", McUnit::Amp, 1e-6),
    sfx("nA", McUnit::Amp, 1e-9),
    sfx("pA", McUnit::Amp, 1e-12),
    sfx("fA", McUnit::Amp, 1e-15),
    sfx("kA", McUnit::Amp, 1e3),
    sfx("KA", McUnit::Amp, 1e3),
    sfx("MA", McUnit::Amp, 1e6),
    sfx("GA", McUnit::Amp, 1e9),

    // Cap
    sfx("F", McUnit::Cap, 1.0),
    sfx("mF", McUnit::Cap, 1e-3),
    sfx("μF", McUnit::Cap, 1e-6),
    sfx("µF", McUnit::Cap, 1e-6),
    sfx("uF", McUnit::Cap, 1e-6),
    sfx("nF", McUnit::Cap, 1e-9),
    sfx("pF", McUnit::Cap, 1e-12),
    sfx("kF", McUnit::Cap, 1e3),
    sfx("KF", McUnit::Cap, 1e3),
    sfx("MF", McUnit::Cap, 1e6),
    sfx("GF", McUnit::Cap, 1e9),

    // Ind
    sfx("H", McUnit::Ind, 1.0),
    sfx("mH", McUnit::Ind, 1e-3),
    sfx("μH", McUnit::Ind, 1e-6),
    sfx("µH", McUnit::Ind, 1e-6),
    sfx("uH", McUnit::Ind, 1e-6),
    sfx("nH", McUnit::Ind, 1e-9),
    sfx("pH", McUnit::Ind, 1e-12),
    sfx("kH", McUnit::Ind, 1e3),
    sfx("KH", McUnit::Ind, 1e3),
    sfx("MH", McUnit::Ind, 1e6),
    sfx("GH", McUnit::Ind, 1e9),

    // Time
    sfx("s", McUnit::Time, 1.0),
    sfx("ms", McUnit::Time, 1e-3),
    sfx("μs", McUnit::Time, 1e-6),
    sfx("µs", McUnit::Time, 1e-6),
    sfx("us", McUnit::Time, 1e-6),
    sfx("ns", McUnit::Time, 1e-9),
    sfx("ps", McUnit::Time, 1e-12),
    sfx("fs", McUnit::Time, 1e-15),
    sfx("ks", McUnit::Time, 1e3),
    sfx("Ks", McUnit::Time, 1e3),
    sfx("Ms", McUnit::Time, 1e6),
    sfx("Gs", McUnit::Time, 1e9),
    sfx("min", McUnit::Time, 60.0),
    sfx("h", McUnit::Time, 3600.0),
    sfx("hr", McUnit::Time, 3600.0),

    // Len
    sfx("m", McUnit::Len, 1.0),
    sfx("dm", McUnit::Len, 1e-1),
    sfx("cm", McUnit::Len, 1e-2),
    sfx("mm", McUnit::Len, 1e-3),
    sfx("μm", McUnit::Len, 1e-6),
    sfx("µm", McUnit::Len, 1e-6),
    sfx("um", McUnit::Len, 1e-6),
    sfx("nm", McUnit::Len, 1e-9),
    sfx("pm", McUnit::Len, 1e-12),
    sfx("fm", McUnit::Len, 1e-15),
    sfx("km", McUnit::Len, 1e3),
    sfx("in", McUnit::Len, 0.0254),
    sfx("inch", McUnit::Len, 0.0254),
    sfx("inches", McUnit::Len, 0.0254),
    sfx("mil", McUnit::Len, 25.4e-6),
    sfx("mils", McUnit::Len, 25.4e-6),
    sfx("ft", McUnit::Len, 0.3048),
    sfx("yd", McUnit::Len, 0.9144),

    // Wat
    sfx("W", McUnit::Wat, 1.0),
    sfx("mW", McUnit::Wat, 1e-3),
    sfx("μW", McUnit::Wat, 1e-6),
    sfx("µW", McUnit::Wat, 1e-6),
    sfx("uW", McUnit::Wat, 1e-6),
    sfx("nW", McUnit::Wat, 1e-9),
    sfx("kW", McUnit::Wat, 1e3),
    sfx("KW", McUnit::Wat, 1e3),
    sfx("MW", McUnit::Wat, 1e6),
    sfx("GW", McUnit::Wat, 1e9),
    sfx("VA", McUnit::Wat, 1.0),
    sfx("mVA", McUnit::Wat, 1e-3),
    sfx("kVA", McUnit::Wat, 1e3),
    sfx("VAR", McUnit::Wat, 1.0),
    sfx("var", McUnit::Wat, 1.0),
    sfx("mVAR", McUnit::Wat, 1e-3),
    sfx("mvar", McUnit::Wat, 1e-3),
    sfx("kVAR", McUnit::Wat, 1e3),
    sfx("kvar", McUnit::Wat, 1e3),
    sfx("Wh", McUnit::Wat, 1.0),
    sfx("kWh", McUnit::Wat, 1e3),
    sfx("MWh", McUnit::Wat, 1e6),

    // Hz
    sfx("Hz", McUnit::Hz, 1.0),
    sfx("mHz", McUnit::Hz, 1e-3),
    sfx("μHz", McUnit::Hz, 1e-6),
    sfx("µHz", McUnit::Hz, 1e-6),
    sfx("uHz", McUnit::Hz, 1e-6),
    sfx("nHz", McUnit::Hz, 1e-9),
    sfx("kHz", McUnit::Hz, 1e3),
    sfx("MHz", McUnit::Hz, 1e6),
    sfx("GHz", McUnit::Hz, 1e9),
    sfx("THz", McUnit::Hz, 1e12),

    // Db (the legacy gain path keeps the value as-is: 3dB is 3.0, no scaling)
    sfx("dB", McUnit::Db, 1.0),
    sfx("dBm", McUnit::Db, 1.0),
    sfx("dBw", McUnit::Db, 1.0),
    sfx("dBi", McUnit::Db, 1.0),
    sfx("dBd", McUnit::Db, 1.0),
    sfx("dBc", McUnit::Db, 1.0),
    sfx("dBV", McUnit::Db, 1.0),
    sfx("dBu", McUnit::Db, 1.0),
    sfx("dBFS", McUnit::Db, 1.0),
    sfx("dBμV", McUnit::Db, 1.0),
    sfx("dBµV", McUnit::Db, 1.0),
    sfx("dBuV", McUnit::Db, 1.0),

    // Ppm
    sfx("ppm", McUnit::Ppm, 1.0),
    sfx("ppb", McUnit::Ppm, 1e-3),
    sfx("ppt", McUnit::Ppm, 1e-6),
    sfx("ppq", McUnit::Ppm, 1e-9),

    // Percent
    sfx("%", McUnit::Percent, 1.0),
    sfx("‰", McUnit::Percent, 1e-3),
    sfx("‱", McUnit::Percent, 1e-4),
    sfx("%RH", McUnit::Percent, 1.0),

    // Baud
    sfx("bps", McUnit::Baud, 1.0),
    sfx("Bps", McUnit::Baud, 8.0),
    sfx("kbps", McUnit::Baud, 1e3),
    sfx("kBps", McUnit::Baud, 8e3),
    sfx("Mbps", McUnit::Baud, 1e6),
    sfx("MBps", McUnit::Baud, 8e6),
    sfx("Gbps", McUnit::Baud, 1e9),
    sfx("GBps", McUnit::Baud, 8e9),
    sfx("Tbps", McUnit::Baud, 1e12),
    sfx("TBps", McUnit::Baud, 8e12),
    sfx("sym/s", McUnit::Baud, 1.0),

    // DataSize
    sfx("b", McUnit::DataSize, 0.125),
    sfx("bit", McUnit::DataSize, 0.125),
    sfx("B", McUnit::DataSize, 1.0),
    sfx("Byte", McUnit::DataSize, 1.0),
    sfx("kb", McUnit::DataSize, 1.25e2),
    sfx("kB", McUnit::DataSize, 1e3),
    sfx("Mb", McUnit::DataSize, 1.25e5),
    sfx("MB", McUnit::DataSize, 1e6),
    sfx("Gb", McUnit::DataSize, 1.25e8),
    sfx("GB", McUnit::DataSize, 1e9),
    sfx("Tb", McUnit::DataSize, 1.25e11),
    sfx("TB", McUnit::DataSize, 1e12),
    sfx("Pb", McUnit::DataSize, 1.25e14),
    sfx("PB", McUnit::DataSize, 1e15),
    sfx("Eb", McUnit::DataSize, 1.25e17),
    sfx("EB", McUnit::DataSize, 1e18),
    sfx("Kib", McUnit::DataSize, 1024.0),
    sfx("KiB", McUnit::DataSize, 1024.0),
    sfx("Mib", McUnit::DataSize, 1024.0 * 1024.0),
    sfx("MiB", McUnit::DataSize, 1024.0 * 1024.0),
    sfx("Gib", McUnit::DataSize, 1024.0 * 1024.0 * 1024.0),
    sfx("GiB", McUnit::DataSize, 1024.0 * 1024.0 * 1024.0),
    sfx("Tib", McUnit::DataSize, 1024.0 * 1024.0 * 1024.0 * 1024.0),
    sfx("TiB", McUnit::DataSize, 1024.0 * 1024.0 * 1024.0 * 1024.0),
    sfx("Pib", McUnit::DataSize, 1024.0 * 1024.0 * 1024.0 * 1024.0 * 1024.0),
    sfx("PiB", McUnit::DataSize, 1024.0 * 1024.0 * 1024.0 * 1024.0 * 1024.0),
    sfx("Eib", McUnit::DataSize, 1024.0 * 1024.0 * 1024.0 * 1024.0 * 1024.0 * 1024.0),
    sfx("EiB", McUnit::DataSize, 1024.0 * 1024.0 * 1024.0 * 1024.0 * 1024.0 * 1024.0),

    // Sps
    sfx("SPS", McUnit::Sps, 1.0),
    sfx("kSPS", McUnit::Sps, 1e3),
    sfx("MSPS", McUnit::Sps, 1e6),
    sfx("GSPS", McUnit::Sps, 1e9),
    sfx("TSPS", McUnit::Sps, 1e12),
    sfx("sps", McUnit::Sps, 1.0),
    sfx("ksps", McUnit::Sps, 1e3),
    sfx("Msps", McUnit::Sps, 1e6),
    sfx("Gsps", McUnit::Sps, 1e9),
    sfx("Sa/s", McUnit::Sps, 1.0),
    sfx("kSa/s", McUnit::Sps, 1e3),
    sfx("MSa/s", McUnit::Sps, 1e6),
    sfx("GSa/s", McUnit::Sps, 1e9),

    // Siemens
    sfx("S", McUnit::Siemens, 1.0),
    sfx("mS", McUnit::Siemens, 1e-3),
    sfx("μS", McUnit::Siemens, 1e-6),
    sfx("µS", McUnit::Siemens, 1e-6),
    sfx("uS", McUnit::Siemens, 1e-6),
    sfx("nS", McUnit::Siemens, 1e-9),
    sfx("pS", McUnit::Siemens, 1e-12),
    sfx("kS", McUnit::Siemens, 1e3),
    sfx("MS", McUnit::Siemens, 1e6),
    sfx("GS", McUnit::Siemens, 1e9),

    // Energy
    sfx("J", McUnit::Energy, 1.0),
    sfx("mJ", McUnit::Energy, 1e-3),
    sfx("kJ", McUnit::Energy, 1e3),

    // Efield
    sfx("V/m", McUnit::Efield, 1.0),
    sfx("mV/m", McUnit::Efield, 1e-3),

    // Hfield
    sfx("A/m", McUnit::Hfield, 1.0),
    sfx("mA/m", McUnit::Hfield, 1e-3),

    // Flux
    sfx("Wb", McUnit::Flux, 1.0),
    sfx("mWb", McUnit::Flux, 1e-3),
    sfx("μWb", McUnit::Flux, 1e-6),
    sfx("µWb", McUnit::Flux, 1e-6),
    sfx("uWb", McUnit::Flux, 1e-6),

    // Bfield
    sfx("T", McUnit::Bfield, 1.0),
    sfx("mT", McUnit::Bfield, 1e-3),
    sfx("μT", McUnit::Bfield, 1e-6),
    sfx("µT", McUnit::Bfield, 1e-6),
    sfx("uT", McUnit::Bfield, 1e-6),
    sfx("G", McUnit::Bfield, 1e-4),

    // Slew
    sfx("V/μs", McUnit::Slew, 1e6),
    sfx("V/µs", McUnit::Slew, 1e6),
    sfx("V/us", McUnit::Slew, 1e6),
    sfx("A/μs", McUnit::Slew, 1e6),
    sfx("A/µs", McUnit::Slew, 1e6),
    sfx("A/us", McUnit::Slew, 1e6),

    // Charge
    sfx("Ah", McUnit::Charge, 1.0),
    sfx("mAh", McUnit::Charge, 1e-3),
    sfx("μAh", McUnit::Charge, 1e-6),
    sfx("µAh", McUnit::Charge, 1e-6),
    sfx("uAh", McUnit::Charge, 1e-6),
    sfx("nAh", McUnit::Charge, 1e-9),
    sfx("pAh", McUnit::Charge, 1e-12),
    sfx("kAh", McUnit::Charge, 1e3),
    sfx("MAh", McUnit::Charge, 1e6),
    sfx("GAh", McUnit::Charge, 1e9),

    // Temp (affine: normalizes to Celsius)
    sfx_off("℃", McUnit::Temp, 1.0, 0.0),
    sfx_off("°C", McUnit::Temp, 1.0, 0.0),
    sfx_off("degC", McUnit::Temp, 1.0, 0.0),
    sfx_off("℉", McUnit::Temp, 5.0 / 9.0, -160.0 / 9.0),
    sfx_off("°F", McUnit::Temp, 5.0 / 9.0, -160.0 / 9.0),
    sfx_off("degF", McUnit::Temp, 5.0 / 9.0, -160.0 / 9.0),
    sfx_off("K", McUnit::Temp, 1.0, -273.15),

    // Angle (normalizes to radians)
    sfx("rad", McUnit::Angle, 1.0),
    sfx("deg", McUnit::Angle, std::f64::consts::PI / 180.0),
    sfx("°", McUnit::Angle, std::f64::consts::PI / 180.0),

    // AngularRate (normalizes to rad/s)
    sfx("rad/s", McUnit::AngularRate, 1.0),
    sfx("deg/s", McUnit::AngularRate, std::f64::consts::PI / 180.0),
    sfx("rpm", McUnit::AngularRate, 2.0 * std::f64::consts::PI / 60.0),
    sfx("rps", McUnit::AngularRate, 2.0 * std::f64::consts::PI),

    // Ohm (symbol, R-code, and spelled forms; see mc_uval::resist_multiplier)
    sfx("R", McUnit::Ohm, 1.0),
    sfx("Ω", McUnit::Ohm, 1.0),
    sfx("ohm", McUnit::Ohm, 1.0),
    sfx("Ohm", McUnit::Ohm, 1.0),
    sfx("mR", McUnit::Ohm, 1e-3),
    sfx("mΩ", McUnit::Ohm, 1e-3),
    sfx("mohm", McUnit::Ohm, 1e-3),
    sfx("mOhm", McUnit::Ohm, 1e-3),
    sfx("μR", McUnit::Ohm, 1e-6),
    sfx("µR", McUnit::Ohm, 1e-6),
    sfx("uR", McUnit::Ohm, 1e-6),
    sfx("μΩ", McUnit::Ohm, 1e-6),
    sfx("µΩ", McUnit::Ohm, 1e-6),
    sfx("uΩ", McUnit::Ohm, 1e-6),
    sfx("μohm", McUnit::Ohm, 1e-6),
    sfx("µohm", McUnit::Ohm, 1e-6),
    sfx("uohm", McUnit::Ohm, 1e-6),
    sfx("nR", McUnit::Ohm, 1e-9),
    sfx("nΩ", McUnit::Ohm, 1e-9),
    sfx("nohm", McUnit::Ohm, 1e-9),
    sfx("nOhm", McUnit::Ohm, 1e-9),
    sfx("kR", McUnit::Ohm, 1e3),
    sfx("kΩ", McUnit::Ohm, 1e3),
    sfx("kohm", McUnit::Ohm, 1e3),
    sfx("kOhm", McUnit::Ohm, 1e3),
    sfx("MR", McUnit::Ohm, 1e6),
    sfx("MΩ", McUnit::Ohm, 1e6),
    sfx("Mohm", McUnit::Ohm, 1e6),
    sfx("MOhm", McUnit::Ohm, 1e6),
    sfx("GR", McUnit::Ohm, 1e9),
    sfx("GΩ", McUnit::Ohm, 1e9),
    sfx("Gohm", McUnit::Ohm, 1e9),
    sfx("GOhm", McUnit::Ohm, 1e9),

    // Responsivity (normalizes to A/W)
    sfx("A/W", McUnit::Responsivity, 1.0),
    sfx("mA/W", McUnit::Responsivity, 1e-3),
    sfx("μA/W", McUnit::Responsivity, 1e-6),
    sfx("µA/W", McUnit::Responsivity, 1e-6),
    sfx("uA/W", McUnit::Responsivity, 1e-6),
    sfx("nA/W", McUnit::Responsivity, 1e-9),
    sfx("kA/W", McUnit::Responsivity, 1e3),

    // Noise density: the lexer's forms (lex.re UV_NOISE / UV_NOISE_BARE). The
    // AST path accepts any suffix for this family, so these rows only serve the
    // text path (round-tripping Display's "nV/√Hz").
    sfx("nV/√Hz", McUnit::Noise, 1.0),
    sfx("μV/√Hz", McUnit::Noise, 1.0),
    sfx("µV/√Hz", McUnit::Noise, 1.0),
    sfx("uV/√Hz", McUnit::Noise, 1.0),
    sfx("pA/√Hz", McUnit::Noise, 1.0),
    sfx("nA/√Hz", McUnit::Noise, 1.0),
    sfx("V/√Hz", McUnit::Noise, 1.0),
    sfx("V/rtHz", McUnit::Noise, 1.0),
    sfx("nV/rtHz", McUnit::Noise, 1.0),
    sfx("μV/rtHz", McUnit::Noise, 1.0),
    sfx("µV/rtHz", McUnit::Noise, 1.0),
    sfx("uV/rtHz", McUnit::Noise, 1.0),
];

/// Normalize `value`, written in `suffix`'s notation, to the canonical unit.
pub fn normalize(suffix: &UnitSuffix, value: f64) -> f64 {
    value * suffix.factor + suffix.offset
}

/// Look a suffix up inside one family (the AST path's validation view).
pub fn in_family(unit: &McUnit, suffix: &str) -> Option<&'static UnitSuffix> {
    UNIT_SUFFIXES
        .iter()
        .find(|s| &s.unit == unit && s.text == suffix)
}

/// Look a suffix up in any family (the text path's view).
pub fn any_family(suffix: &str) -> Option<&'static UnitSuffix> {
    UNIT_SUFFIXES.iter().find(|s| s.text == suffix)
}

/// Split `text` into its numeric head and its trailing unit suffix. Mirrors the
/// scanner in `mc_uval::extract_value_and_unit`: optional sign or `±`, a decimal
/// or exponent number, then everything else is the suffix.
pub fn split_number(text: &str) -> Option<(f64, &str)> {
    static UVAL_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = UVAL_RE.get_or_init(|| {
        regex::Regex::new(r"^([±+\-]?\d*\.?\d+(?:[eE][+\-]?\d+)?)(.*)$")
            .expect("valid unit-value number regex")
    });
    let captures = re.captures(text.trim())?;
    let head = captures.get(1)?.as_str();
    let head = head.strip_prefix('±').unwrap_or(head);
    let value = head.parse::<f64>().ok()?;
    Some((value, captures.get(2)?.as_str()))
}

/// Text -> (normalized value, family). `None` when the text carries no
/// recognized unit suffix, leaving the bare-number case to the caller.
pub fn parse_text(text: &str) -> Option<(f64, McUnit)> {
    let (value, suffix) = split_number(text)?;
    if suffix.is_empty() {
        return None;
    }
    let entry = any_family(suffix)?;
    Some((normalize(entry, value), entry.unit.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn family_of(text: &str) -> Option<McUnit> {
        any_family(text).map(|s| s.unit.clone())
    }

    #[test]
    fn eval__suffixes_are_collision_free() {
        for (i, a) in UNIT_SUFFIXES.iter().enumerate() {
            for b in UNIT_SUFFIXES.iter().skip(i + 1) {
                assert!(
                    a.text != b.text,
                    "suffix {:?} is registered twice ({:?} and {:?})",
                    a.text,
                    a.unit,
                    b.unit
                );
            }
        }
    }

    #[test]
    fn eval__suffix_families_are_the_legacy_ones() {
        assert_eq!(family_of("mV"), Some(McUnit::Volt));
        assert_eq!(family_of("KV"), Some(McUnit::Volt));
        assert_eq!(family_of("µohm"), Some(McUnit::Ohm));
        assert_eq!(family_of("KiB"), Some(McUnit::DataSize));
        assert_eq!(family_of("rpm"), Some(McUnit::AngularRate));
        assert_eq!(family_of("nV/√Hz"), Some(McUnit::Noise));
        // Same spelling shape, different family: only the exact form decides.
        assert_eq!(family_of("S"), Some(McUnit::Siemens));
        assert_eq!(family_of("s"), Some(McUnit::Time));
        assert_eq!(family_of("ps"), Some(McUnit::Time));
        assert_eq!(family_of("pS"), Some(McUnit::Siemens));
        assert_eq!(family_of("T"), Some(McUnit::Bfield));
        assert_eq!(family_of("m"), Some(McUnit::Len));
        assert_eq!(family_of("min"), Some(McUnit::Time));
        assert_eq!(family_of("in"), Some(McUnit::Len));
    }

    #[test]
    fn eval__text_normalizes_like_the_ast_path() {
        assert_eq!(parse_text("1200mV"), Some((1.2, McUnit::Volt)));
        assert_eq!(parse_text("1.2V"), Some((1.2, McUnit::Volt)));
        assert_eq!(parse_text("3000mV"), Some((3.0, McUnit::Volt)));
        assert_eq!(parse_text("3V"), Some((3.0, McUnit::Volt)));
        assert_eq!(parse_text("32kHz"), Some((32000.0, McUnit::Hz)));
        assert_eq!(parse_text("-5.0V"), Some((-5.0, McUnit::Volt)));
        assert_eq!(parse_text("2.5%"), Some((2.5, McUnit::Percent)));
        assert_eq!(parse_text("10kohm"), Some((10000.0, McUnit::Ohm)));
        // Affine and angle families keep their legacy conversion.
        assert_eq!(parse_text("32degF"), Some((0.0, McUnit::Temp)));
        assert_eq!(parse_text("273.15K"), Some((0.0, McUnit::Temp)));
        assert_eq!(
            parse_text("180deg"),
            Some((std::f64::consts::PI, McUnit::Angle))
        );
    }

    #[test]
    fn eval__canonical_symbols_round_trip() {
        // Every symbol `McUnit`'s Display can emit must be re-readable, or a
        // normalized value could not survive a trip through text.
        for text in [
            "V", "A", "F", "H", "s", "m", "W", "Ω", "°C", "Hz", "dB", "ppm", "%", "bps", "B", "SPS",
            "S", "A/W", "rad", "rad/s", "J", "V/m", "A/m", "Wb", "T", "V/μs", "nV/√Hz", "Ah",
        ] {
            assert!(
                any_family(text).is_some(),
                "canonical symbol {text:?} is not re-readable"
            );
        }
    }

    #[test]
    fn eval__text_without_suffix_is_left_to_the_caller() {
        assert_eq!(parse_text("42"), None);
        assert_eq!(parse_text("GPIO16"), None);
        assert_eq!(parse_text(""), None);
        assert_eq!(parse_text("0x36"), None);
    }

    #[test]
    fn eval__family_filter_is_the_ast_validation_view() {
        assert!(in_family(&McUnit::Volt, "mV").is_some());
        assert!(in_family(&McUnit::Volt, "mA").is_none());
        assert!(in_family(&McUnit::Time, "min").is_some());
        assert!(in_family(&McUnit::Len, "min").is_none());
    }
}
