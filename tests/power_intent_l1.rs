// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Power-intent L1 relation-edge ERC (power-intent-design.md §3 / §13 landing 1).
//!
//! The captured `@bridge`/`@clamp` relation edges + `ref` roles (McPowerDecls,
//! threaded into the flat `InstTable`) are *consumed* by two FlatErc rules:
//!
//! * **PWR-2** (`POWER_BRIDGE_LOOP` = 6007) — the DC `@bridge` subgraph must be
//!   acyclic. A second/cyclic leg between endpoints already DC-bridged (parallel
//!   ferrite legs, or a triangle) is a loop unless a hub ref carries `@star`.
//!   A `@bridge` edge never merges L0 copper — it only relates two L1 classes.
//! * **PWR-7** (`CLAMP_REF_NOT_PROTECTIVE` = 6008) — `@clamp(ref)` must target an
//!   `@role(protective)`/`@role(earth)` ref.
//!
//! Golden board (`mcs/pwrint/src/main.mc`) shape: GND carries `@star`, so its
//! two parallel `@bridge(GND, GNDA)` legs are discharged, and POWER_USB clamps
//! to its own `@role(protective)` ESDGND — neither new code fires.

mod common;

use mcc::{McIds, McURI};

/// A two-pin ferrite-like device, declared in-file so the tests don't depend on
/// the installed mcode library.
const FB: &str = "component FB {\n    pins = [\n        io [1,2] = [X, Y]\n    ]\n}\n";

/// A TVS-like device: one signal pin + one clamp-reference pin.
const TV: &str = "component TV {\n    pins = [\n        io 1 = IO\n        ps 2 = G\n    ]\n}\n";

/// Build the source and return every diagnostic code (sorted).
fn build_codes(src: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let uri: McURI = "/mcc/power-intent-l1.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

/// Golden: two parallel DC `@bridge(GNDA, GND)` legs are a loop, but `@star`
/// on the GND hub discharges it (main.mc ①). No PWR-2.
#[test]
fn bridge_parallel_legs_discharged_by_star() {
    let src = format!(
        "{FB}\nmodule main {{\n    conduit GND  @role(main) @star\n    conduit GNDA @role(quiet)\n    \
         GNDA - fb1::FB() - GND @bridge(GNDA, GND)\n    \
         GNDA - fb2::FB() - GND @bridge(GNDA, GND)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::POWER_BRIDGE_LOOP),
        "@star on the hub must discharge the parallel-leg loop; got codes: {codes:?}"
    );
}

/// Same shape without `@star` → the second leg is an undeclared loop: PWR-2.
#[test]
fn bridge_parallel_legs_without_star_fire_loop() {
    let src = format!(
        "{FB}\nmodule main {{\n    conduit GND  @role(main)\n    conduit GNDA @role(quiet)\n    \
         GNDA - fb1::FB() - GND @bridge(GNDA, GND)\n    \
         GNDA - fb2::FB() - GND @bridge(GNDA, GND)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::POWER_BRIDGE_LOOP),
        "a second parallel @bridge without @star is a PWR-2 loop; got codes: {codes:?}"
    );
}

/// A single bridge leg (one quiet subface return path) is a tree, not a loop.
#[test]
fn bridge_single_leg_is_clean() {
    let src = format!(
        "{FB}\nmodule main {{\n    conduit GND  @role(main)\n    conduit GNDA @role(quiet)\n    \
         GNDA - fb1::FB() - GND @bridge(GNDA, GND)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::POWER_BRIDGE_LOOP),
        "a single bridge leg is a tree, not a loop; got codes: {codes:?}"
    );
}

/// Two quiet leaves to the same root is a star tree (no cycle between them).
#[test]
fn bridge_two_separate_quiet_leaves_are_not_a_loop() {
    let src = format!(
        "{FB}\nmodule main {{\n    conduit GND  @role(main)\n    conduit GNDA @role(quiet)\n    \
         conduit GNDB @role(quiet)\n    GNDA - fb1::FB() - GND @bridge(GNDA, GND)\n    \
         GNDB - fb2::FB() - GND @bridge(GNDB, GND)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::POWER_BRIDGE_LOOP),
        "independent quiet leaves form a tree, not a loop; got codes: {codes:?}"
    );
}

/// PWR-7: a clamp into a `main`-role ref (the digital GND) dumps transient
/// current into the wrong reference → fires.
#[test]
fn clamp_to_main_ref_fires() {
    let src = format!(
        "{TV}\nmodule main {{\n    conduit GND @role(main)\n    io DP @exposed(esd_contact)\n    \
         TV tv\n    tv.IO -> DP\n    tv.G -> GND @clamp(GND)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::CLAMP_REF_NOT_PROTECTIVE),
        "@clamp to a @role(main) ref must fire PWR-7; got codes: {codes:?}"
    );
}

/// PWR-7 pass: clamping into the module's own `@role(protective)` ref
/// (POWER_USB.mc shape) is legal.
#[test]
fn clamp_to_protective_ref_passes() {
    let src = format!(
        "{TV}\nmodule main {{\n    conduit ESDGND @role(protective)\n    io DP @exposed(esd_contact)\n    \
         TV tv\n    tv.IO -> DP\n    tv.G -> ESDGND @clamp(ESDGND)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::CLAMP_REF_NOT_PROTECTIVE),
        "@clamp to a @role(protective) ref must pass PWR-7; got codes: {codes:?}"
    );
}

/// A clamp target that is not a same-scope ref is not adjudicated here (its
/// role is supplied by an ancestor world / port contract, iron rule 1 §6).
#[test]
fn clamp_to_undeclared_net_is_not_adjudicated() {
    let src = format!(
        "{TV}\nmodule main {{\n    conduit GND @role(main)\n    io DP @exposed(esd_contact)\n    \
         io CLAMPNET\n    TV tv\n    tv.IO -> DP\n    tv.G -> CLAMPNET @clamp(CLAMPNET)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::CLAMP_REF_NOT_PROTECTIVE),
        "an undeclared clamp net carries no role to judge; got codes: {codes:?}"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// Landing 2 — rail DC-contract Volt decode (power-intent-design.md §4 / §13).
//
// The `rail [hot, ret]::DC(v, tol, capacity, eff)` guarantee line is now
// *decoded* (the Volt-arg decode), not name-heuristic: 3.3V → 3.3, ±5% → 0.05,
// 500mA → 0.5A, 0.95 → 0.95. Two declaration-local FlatErc rules consume it:
//
// * **decode** (`POWER_RAIL_DECODE` = 6009) — a ctor argument that does not
//   decode to a DC volts nominal / tolerance window / capacity / efficiency
//   (e.g. `::DC(5A)` is a current, not a volts value; `req:±3%` is a sink-side
//   window key that has no place on a source rail).
// * **two-roots** (`POWER_RAIL_TWO_ROOTS` = 6010) — the same net is the hot
//   member of two rails in two domains (P3: one net carries one handwritten
//   supply root). Sharing a *return* across domains (DVDD + DCORE → GND) is
//   the golden main.mc shape and stays clean.

/// Golden rail contract shape: DVDD + DCORE share the GND return, each hot has
/// one handwritten supply root, every ctor arg decodes. No 6009/6010.
#[test]
fn rail_contract_valid_domains_are_clean() {
    let src = "module main {\n    conduit GND  @role(main) @star\n    conduit GNDA @role(quiet)\n    \
        domain DVDD  @class(digital) { rail [VDD_3V3, GND]::DC(3.3V, tol:±5%, capacity:500mA, eff:0.95) }\n    \
        domain DCORE @class(digital) { rail [VCC_1V2, GND]::DC(1.2V, tol:±3%, capacity:1A) }\n    \
        domain ISO   { rail [V5V_ISO, GND_ISO]::DC(5V) }\n}\n";
    let codes = build_codes(src);
    assert!(
        !codes.contains(&mcc::errcodes::POWER_RAIL_DECODE)
            && !codes.contains(&mcc::errcodes::POWER_RAIL_TWO_ROOTS),
        "valid DC contracts must not fire 6009/6010; got codes: {codes:?}"
    );
}

/// A rail nominal that is a current (`5A`) cannot be a DC volts value — the
/// decode is flagged (6009). The rail still lists (v = None, bad = Some).
#[test]
fn rail_contract_bad_nominal_fires_decode() {
    let src = "module main {\n    conduit GND @role(main)\n    domain DVDD @class(digital) { rail [VDD_3V3, GND]::DC(5A) }\n}\n";
    let codes = build_codes(src);
    assert!(
        codes.contains(&mcc::errcodes::POWER_RAIL_DECODE),
        "::DC(5A) is not a volts nominal and must fire 6009; got codes: {codes:?}"
    );
}

/// P3: one net carries one handwritten supply root. Two domains each claiming
/// the same hot (`VSAME`) is a fake power-domain conflict → 6010. Sharing only
/// the *return* net is the legitimate golden shape (test above).
#[test]
fn rail_same_hot_in_two_domains_fires_two_roots() {
    let src = "module main {\n    conduit GND @role(main)\n    domain DVDD  @class(digital) { rail [VSAME, GND]::DC(3.3V) }\n    \
        domain DCORE @class(digital) { rail [VSAME, GND]::DC(1.2V) }\n}\n";
    let codes = build_codes(src);
    assert!(
        codes.contains(&mcc::errcodes::POWER_RAIL_TWO_ROOTS),
        "two rails on one hot is a P3 two-roots conflict and must fire 6010; got codes: {codes:?}"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// E-PWR-001 — sink nominal vs derived net supply S (power-intent-design.md
// §4.3/§4.4/§11).
//
// A `psnk` sink must require the net's derived supply nominal S (the
// mandatory-nominal check). The rule consumes the *decoded* pin DC contract
// (decode_pwr_pin, §5.2 direction-word family) joined — by the net the sink's
// hot member lands on — to S derived from that net's handwritten supply roots
// (§4.3): a domain-rail face *or* a `psrc`/`psbi` hot directly on the net. The
// canonical §4.4 case: a `::DC(3.3V)` sink wired onto a 5V rail/net is a wrong
// hookup (P3/E-PWR-001). Roots on one net that disagree in nominal leave
// the net un-adjudicated (source contention → PWR-3/6010 territory). Sinks on
// nets with no direct root (S would need §4.3 copper pass-through propagation)
// are not adjudicated yet.

/// A two-pin sink load: its power terminal requires 3.3V (`psnk`). `io_type`
/// reads `Power`; the direction + requirement nominal live in the captured pwr
/// contract (McPins.pwr), not the net model.
const SINK3: &str =
    "component SINK3 {\n    pins = [\n        psnk [1,2] = [VDD, GND]::DC(3.3V)\n    ]\n}\n";

/// Golden member-bus sink shape (LDO/DCDC `VIN{Vin, GND}` rows): the power
/// member binds to pin 1, whose registered name — and flat class_name — is the
/// *dotted* `VIN.Vin`. The E-PWR-001 lookup must match that dotted capture.
const LDO_MB: &str =
    "component LDO_MB {\n    pins = [\n        psnk [1,2] = VIN{Vin, GND}::DC(3.3V)\n    ]\n}\n";

/// A `::DC(3.3V)` sink wired onto a 5V rail is the canonical §4.4 case → E-PWR-001.
#[test]
fn sink_on_wrong_rail_fires_nominal_mismatch() {
    let src = format!(
        "{SINK3}\nmodule main {{\n    conduit GND @role(main)\n    \
         domain DVDD @class(digital) {{ rail [V5V, GND]::DC(5V) }}\n    \
         io V5V\n    SINK3 s\n    s.VDD -> V5V\n    s.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::POWER_SINK_NOMINAL_MISMATCH),
        "a 3.3V sink on a 5V rail must fire 6011 (E-PWR-001); got codes: {codes:?}"
    );
}

/// The same sink on its *own* 3.3V rail is the healthy hookup → clean.
#[test]
fn sink_on_its_own_rail_is_clean() {
    let src = format!(
        "{SINK3}\nmodule main {{\n    conduit GND @role(main)\n    \
         domain DVDD @class(digital) {{ rail [V3V3, GND]::DC(3.3V) }}\n    \
         io V3V3\n    SINK3 s\n    s.VDD -> V3V3\n    s.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::POWER_SINK_NOMINAL_MISMATCH),
        "a 3.3V sink on its own 3.3V rail must pass 6011; got codes: {codes:?}"
    );
}

/// A sink on a net that is *not* a declared rail face (its S-set would be
/// derived by §4.3 propagation) is not adjudicated by this first rule — the
/// net carries no rail guarantee to compare against.
#[test]
fn sink_on_intermediate_net_is_not_adjudicated() {
    let src = format!(
        "{SINK3}\nmodule main {{\n    conduit GND @role(main)\n    \
         domain DVDD @class(digital) {{ rail [V3V3, GND]::DC(3.3V) }}\n    \
         io V3V3\n    io VMID\n    SINK3 s\n    s.VDD -> VMID\n    s.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::POWER_SINK_NOMINAL_MISMATCH),
        "a sink on a non-rail net carries no guarantee to compare (S-set later); got codes: {codes:?}"
    );
}

/// The golden member-bus spelling (LDO/DCDC `VIN{Vin, GND}` rows) is captured
/// and adjudicated: a 3.3V `VIN` sink bound onto its own 3.3V rail net stays
/// clean — the dotted `VIN.Vin` flat class matches the capture's dotted hot.
#[test]
fn member_bus_sink_on_its_own_rail_is_clean() {
    let src = format!(
        "{LDO_MB}\nmodule main {{\n    conduit GND @role(main)\n    \
         domain DVDD @class(digital) {{ rail [V3V3, GND]::DC(3.3V) }}\n    \
         io V3V3\n    LDO_MB s\n    s.VIN.Vin -> V3V3\n    s.VIN.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::POWER_SINK_NOMINAL_MISMATCH),
        "a member-bus 3.3V sink on its own 3.3V rail must pass 6011; got codes: {codes:?}"
    );
}

/// Same member-bus sink on a 5V rail → the canonical §4.4 case fires through the dotted
/// `VIN.Vin` flat class.
#[test]
fn member_bus_sink_on_wrong_rail_fires_nominal_mismatch() {
    let src = format!(
        "{LDO_MB}\nmodule main {{\n    conduit GND @role(main)\n    \
         domain DVDD @class(digital) {{ rail [V5V, GND]::DC(5V) }}\n    \
         io V5V\n    LDO_MB s\n    s.VIN.Vin -> V5V\n    s.VIN.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::POWER_SINK_NOMINAL_MISMATCH),
        "a member-bus 3.3V sink on a 5V rail must fire 6011; got codes: {codes:?}"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// E-PWR-001 + §4.3 S-set — direct psrc/psbi supply roots.
//
// S(net) comes not only from a declared rail face but from any handwritten
// supply root on the net: an instantiated `psrc`/`psbi` hot directly on it
// (design §4.3 `S(root)` = handwritten guarantee window: a psrc or a domain-rail
// block). This is the golden VMAIN_5V shape — oring's `OUT` psrc (5V) feeds the
// main pair net that the LDO/DCDC/ISO VIN sinks sit on, with no rail face in
// sight. A psbi roots its hot net with its *discharge* nominal (§4.1). Roots
// that disagree on one net leave it un-adjudicated (source contention is PWR-3
// OR-merge, deferred).

/// A two-pin regulated source: `OUT` guarantees 5V (`psrc`).
const SRC5: &str =
    "component SRC5 {\n    pins = [\n        psrc [1,2] = [OUT, GND]::DC(5V)\n    ]\n}\n";

/// A two-pin regulated source: `OUT` guarantees 3.3V.
const SRC3: &str =
    "component SRC3 {\n    pins = [\n        psrc [1,2] = [OUT, GND]::DC(3.3V)\n    ]\n}\n";

/// A two-pin sink load requiring 5V.
const SNK5: &str =
    "component SNK5 {\n    pins = [\n        psnk [1,2] = [VDD, GND]::DC(5V)\n    ]\n}\n";

/// A rechargeable cell: `BAT` is charge-sink / discharge-source (`psbi`), whose
/// `::DC(5V)` is the discharge supply guarantee.
const BAT5: &str =
    "component BAT5 {\n    pins = [\n        psbi [1,2] = [BAT, GND]::DC(5V)\n    ]\n}\n";

/// A 5V-sink load on a net driven by a bare `psrc` (no rail face at all) —
/// S(V5) = 5V from the source root, and the sink needs exactly that → clean.
#[test]
fn sink_on_psrc_driven_net_matching_is_clean() {
    let src = format!(
        "{SRC5}{SNK5}\nmodule main {{\n    conduit GND @role(main)\n    io V5\n    \
         SRC5 s\n    SNK5 k\n    s.OUT -> V5\n    s.GND -> GND\n    \
         k.VDD -> V5\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::POWER_SINK_NOMINAL_MISMATCH),
        "a 5V sink on a 5V psrc net must pass 6011; got codes: {codes:?}"
    );
}

/// The same 5V-psrc net with a 3.3V sink → the canonical §4.4 case fires through the
/// *source root*, exactly as through a rail face.
#[test]
fn sink_on_psrc_driven_net_mismatch_fires() {
    let src = format!(
        "{SRC5}{SINK3}\nmodule main {{\n    conduit GND @role(main)\n    io V5\n    \
         SRC5 s\n    SINK3 k\n    s.OUT -> V5\n    s.GND -> GND\n    \
         k.VDD -> V5\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::POWER_SINK_NOMINAL_MISMATCH),
        "a 3.3V sink on a 5V psrc net must fire 6011; got codes: {codes:?}"
    );
}

/// A `psbi` hot roots its net as a source (discharge): a 3.3V sink on the 5V
/// battery net is a wrong hookup → 6011.
#[test]
fn sink_on_psbi_battery_net_mismatch_fires() {
    let src = format!(
        "{BAT5}{SINK3}\nmodule main {{\n    conduit GND @role(main)\n    io VBAT\n    \
         BAT5 b\n    SINK3 k\n    b.BAT -> VBAT\n    b.GND -> GND\n    \
         k.VDD -> VBAT\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::POWER_SINK_NOMINAL_MISMATCH),
        "a 3.3V sink on a 5V psbi net must fire 6011; got codes: {codes:?}"
    );
}

/// A rail face and a same-nominal source pin on one net (golden VDD_3V3: DVDD
/// rail 3.3V + LDO VOUT psrc 3.3V) is *one* agreeing root — a 3.3V sink there
/// is clean, and the agreeing pair must not silence adjudication of a wrong sink.
#[test]
fn agreeing_rail_and_psrc_roots_still_adjudicate() {
    let src = format!(
        "{SRC5}{SINK3}\nmodule main {{\n    conduit GND @role(main)\n    \
         domain DVDD @class(digital) {{ rail [VDD_3V3, GND]::DC(5V) }}\n    \
         io VDD_3V3\n    SRC5 s\n    SINK3 k\n    s.OUT -> VDD_3V3\n    s.GND -> GND\n    \
         k.VDD -> VDD_3V3\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::POWER_SINK_NOMINAL_MISMATCH),
        "a rail+psrc pair both guaranteeing 5V must still fire a 3.3V sink; got codes: {codes:?}"
    );
}

/// Roots that *disagree* on one net (a 5V rail face + a 3.3V source pin feeding
/// the same net) leave it un-adjudicated — no arbitrary pick decides the 3.3V
/// sink. Source contention is PWR-3 OR-merge territory, not this nominal check.
#[test]
fn disagreeing_roots_leave_net_unadjudicated() {
    let src = format!(
        "{SRC3}{SINK3}\nmodule main {{\n    conduit GND @role(main)\n    \
         domain DVDD @class(digital) {{ rail [VSAME, GND]::DC(5V) }}\n    \
         io VSAME\n    SRC3 s\n    SINK3 k\n    s.OUT -> VSAME\n    s.GND -> GND\n    \
         k.VDD -> VSAME\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::POWER_SINK_NOMINAL_MISMATCH),
        "disagreeing supply roots on one net must defer (PWR-3), not fire 6011; got codes: {codes:?}"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// Pin-contract decode ERC (6012, §5.2 closed word-list discipline — the pin-side Volt-arg decode).
//
// The pin `::DC` is a typed contract: a sink declares *only* its mandatory
// nominal (source-exclusive budget keys and spec-window keys are flagged), the
// nominal must be a DC volts value, and a source may carry its tol/capacity/eff
// budget. decode_pwr_pin keeps the first failure; 6012 reports it decl-locally,
// once per *used* component class — the hookup layer never adjudicates a sink
// it cannot decode, so the decode error must not be silently dropped.

/// A sink whose `::DC` nominal is a current (`5A`), not a DC volts value.
const BADSINK: &str =
    "component BADSINK {\n    pins = [\n        psnk [1,2] = [VDD, GND]::DC(5A)\n    ]\n}\n";

/// A sink that illegally carries a source-exclusive PWR-4 budget key.
const OVERKEYED: &str = "component OVERKEYED {\n    pins = [\n        psnk [1,2] = [VDD, GND]::DC(3.3V, capacity:500mA)\n    ]\n}\n";

/// A sink that puts a spec-window key (`req`) on its per-schematic `::DC`
/// (§4.4 write-site rule: req/abs live in the component spec, never on the pin).
const WINDOWED: &str =
    "component WINDOWED {\n    pins = [\n        psnk [1,2] = [VDD, GND]::DC(3.3V, req:3.0V)\n    ]\n}\n";

/// A source carrying its full PWR-4 budget (`tol`/`capacity`/`eff`).
const SRC_FULL: &str = "component SRC_FULL {\n    pins = [\n        psrc [1,2] = [OUT, GND]::DC(5V, tol:±5%, capacity:1A, eff:0.95)\n    ]\n}\n";

/// A `::DC(5A)` sink nominal cannot be a DC volts value → 6012 fires at the
/// pin's own declaration, and (because its nominal does not decode) 6011 does
/// *not* adjudicate it blind.
#[test]
fn sink_non_volts_nominal_fires_pin_decode() {
    let src = format!(
        "{BADSINK}\nmodule main {{\n    conduit GND @role(main)\n    io V\n    BADSINK s\n    \
         s.VDD -> V\n    s.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::POWER_PIN_DECODE),
        "a current nominal on a psnk ::DC must fire 6012; got codes: {codes:?}"
    );
}

/// A source-exclusive budget key (`capacity`) on a sink is off-register (§5.2)
/// → 6012.
#[test]
fn sink_source_exclusive_key_fires_pin_decode() {
    let src = format!(
        "{OVERKEYED}\nmodule main {{\n    conduit GND @role(main)\n    io V\n    OVERKEYED s\n    \
         s.VDD -> V\n    s.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::POWER_PIN_DECODE),
        "a PWR-4 budget key on a psnk must fire 6012; got codes: {codes:?}"
    );
}

/// A spec-window key (`req`) on the pin `::DC` violates §4.4 write-site rule → 6012.
#[test]
fn sink_spec_window_key_on_pin_fires_pin_decode() {
    let src = format!(
        "{WINDOWED}\nmodule main {{\n    conduit GND @role(main)\n    io V\n    WINDOWED s\n    \
         s.VDD -> V\n    s.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::POWER_PIN_DECODE),
        "a req/abs window key on the pin ::DC must fire 6012; got codes: {codes:?}"
    );
}

/// The same budget keys are *legal* on a source guarantee (`psrc`): a source
/// with tol/capacity/eff decodes clean — no 6012, and its 5V guarantee still
/// adjudicates the sink.
#[test]
fn source_budget_keys_are_clean() {
    let src = format!(
        "{SRC_FULL}{SINK3}\nmodule main {{\n    conduit GND @role(main)\n    io V5\n    \
         SRC_FULL s\n    SINK3 k\n    s.OUT -> V5\n    s.GND -> GND\n    \
         k.VDD -> V5\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::POWER_PIN_DECODE)
            && codes.contains(&mcc::errcodes::POWER_SINK_NOMINAL_MISMATCH),
        "source-side budget keys must not fire 6012, and the 5V guarantee must still fire the 3.3V sink (6011); got codes: {codes:?}"
    );
}

/// The check is decl-local over *used* classes: a def with a bad contract that
/// is never instantiated contributes no instance to the flat table, so its
/// latent decode error is not reported in this build.
#[test]
fn unused_bad_contract_is_not_reported() {
    let src = format!(
        "{SINK3}{BADSINK}\nmodule main {{\n    conduit GND @role(main)\n    io V3\n    \
         SINK3 k\n    k.VDD -> V3\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::POWER_PIN_DECODE),
        "an uninstantiated def's bad contract must not fire 6012; got codes: {codes:?}"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// Pin-name parse hygiene — `::DC(v)` on a power row is a §4.1/§5.2 power
// contract, never an interface binding.
//
// capture_pwr_lines reads the contract off the pin AST, but the same rows also
// flow through the generic pin-name parser (McPinNames), which treats a
// trailing `X::Y` as an `instance::Interface` binding and — finding no
// interface named `DC` — degraded every power row to a "plain pin alias" with
// a spurious 3110 PARAM_INST_LOOKUP_FAILED warning. Real power boards (the
// golden components.mc) carried one such warning per `psrc/psnk/psbi` row.
// The generic parser now knows a `psrc/psnk/psbi` line's `::X` declare is a
// power contract and skips interface resolution silently.

/// Every §4.1 spelling — square-vec `[OUT, GND]::DC`, member-bus
/// `VIN{Vin, GND}::DC`, and all three direction words psrc/psnk/psbi — parses
/// its trailing `::DC(v)` as a power contract, emitting no 3110.
#[test]
fn power_dc_rows_do_not_degrade_through_interface_lookup() {
    let src = format!(
        "{SRC5}{SNK5}{LDO_MB}{BAT5}\nmodule main {{\n    conduit GND @role(main)\n    io V5\n    \
         SRC5 s\n    SNK5 k\n    LDO_MB l\n    BAT5 b\n    \
         s.OUT -> V5\n    s.GND -> GND\n    k.VDD -> V5\n    k.GND -> GND\n    \
         l.VIN.Vin -> V5\n    l.VIN.GND -> GND\n    b.BAT -> V5\n    b.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::PARAM_INST_LOOKUP_FAILED),
        "power `::DC` rows must not degrade through the interface resolver; got codes: {codes:?}"
    );
}

/// Non-power `instance::Iface` misuse must still warn exactly once — the
/// power-row gate must not silence genuine interface-binding errors.
#[test]
fn interface_misuse_still_warns_while_power_rows_are_silent() {
    // A golden interface-binding row `io [8,9] = I2C0::I2C(Master)` with no
    // `I2C` interface defined is a real (mis)use of `::` on a *non-power* pin
    // — it must still degrade with one 3110 warning. The power rows beside it
    // contribute none.
    let src = format!(
        "{SRC5}{SNK5}\ncomponent BADIF {{\n    pins = [\n        io [8,9] = I2C0::I2C(Master)\n    ]\n}}\n\
         module main {{\n    conduit GND @role(main)\n    io V5\n    \
         SRC5 s\n    SNK5 k\n    \
         s.OUT -> V5\n    s.GND -> GND\n    k.VDD -> V5\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let warns = codes
        .iter()
        .filter(|&&c| c == mcc::errcodes::PARAM_INST_LOOKUP_FAILED)
        .count();
    assert_eq!(
        warns, 1,
        "exactly the non-power interface misuse must warn 3110 (once); got {warns} in {codes:?}"
    );
}
