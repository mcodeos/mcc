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
//! * **PWR-9** (`ISOLATED_DC_BRIDGE` = 6014, §3.2 isolated row) — the isolated
//!   world (isolated refs + every rail returned to one) carries zero declared DC
//!   `@bridge` to any non-isolated member.
//! * **PWR-8** (`PROTECTIVE_MULTI_BRIDGE` = 6015, §3.2 protective row) — a
//!   protective conduit allows exactly one DC `@bridge`; a second fires even
//!   when `@star` discharges the 6007 loop (single point is a hard (1,0)).
//! * **earth** (`EARTH_DC_LEAK` = 6016, §3.2 earth row) — an `@role(earth)`
//!   conduit couples only through a Y-cap `@couple`; a DC `@bridge` incident to
//!   it is a leakage warning. Severity is a warning, not an error, per the
//!   design's "leakage warning" wording.
//! * **main** (`REFERENCE_ISLAND_ROOT` = 6017, §3.2.1) — a DC-bridged reference
//!   island (role-bearing refs joined by DC `@bridge` legs) carries exactly one
//!   `@role(main)` root; zero mains or two mains is an error.
//! * **zero-bridge** (`ROLE_REF_MISSING_BRIDGE` = 6018, conduit-equivalence-design.md
//!   §8.4) — `@bridge` is explicit, never inferred from a component type, but a
//!   `@role(quiet)`/`@role(protective)` conduit with no declared DC `@bridge`
//!   (a bare count of zero) is an unwired declaration and must not be silent; a
//!   Y-cap `@couple` does not discharge it. The upper bound (a second bridge) is
//!   6007's / 6015's job — this fires on the zero only.
//! * **PWR-1** (`SINK_NET_NO_SOURCE` = 6019, §11 no-source face / axis ③) — a
//!   net that carries component power-sink (`psnk`) terminals yet has no supply
//!   root on the net itself (no declared domain-rail face, no decodable
//!   psrc/psbi hot pin) is a face whose loads draw from nothing. Net-local,
//!   mirroring 6011/6013: module boundary feed ports and copper pass-through
//!   feed stay the S-set step.
//! * **§6.2③ combine output** (`COMBINE_OUTPUT_TOL` = 6020,
//!   rail-contract-design.md §6.1/§6.2) — a combine element (a def with ≥2
//!   input-direction `psnk`/`psbi` power rows and a `psrc` output row) is a
//!   pass-through OR-merge, not a regulator; its output `psrc` writes the
//!   merged nominal `::DC(v)` only, so a ±tol on it is a declaration error
//!   (a literal OUT window would over-claim under single-source states).
//! * **PWR-4 budget** (`NET_BUDGET_EXCEEDED` = 6021, rail-contract-design.md
//!   §8) — a net whose supply root declares a `capacity` (a domain-rail face or
//!   a `psrc`/`psbi` hot pin carrying `capacity`) is budgeted against the psnk
//!   sinks on the same net that declare an `amp` demand (a sink-exclusive,
//!   opt-in key on the psnk `::DC`, §8.1): Σ amp ≤ capacity. Net-local mirror
//!   of 6011/6013/6019 — converter-input push-up (`I_in = ΣP_out/(|V_in|×eff)`)
//!   and cross-net/module-boundary feed are the S-set step, and a net with no
//!   declared capacity (or with disagreeing capacity roots) is not adjudicated.
//! * **§6.1 regulator gate** (`POWER_CONVERTER_GATE` = 6023,
//!   rail-contract-design.md window batch) — a regulator that writes the full
//!   spec (input_req + output, ≥1 Snk input row, ≥1 Src output row) gates its
//!   own input net: a Resolved supply window there must sit inside input_req
//!   (`S(input) ⊆ input_req`). Un-derivable feeds are not adjudicated.
//! * **§6.3 sink req window** (`POWER_SINK_WINDOW_MISMATCH` = 6024,
//!   rail-contract-design.md window batch) — a load whose spec declares
//!   input_req accepts supply only inside that window; a Resolved supply window
//!   on its sink net escaping it (over/under-volts the load) is an Error. A
//!   regulator's own input row is 6023's per-net gate, not re-judged here.
//! * **§6.1 partial spec** (`POWER_CONVERTER_SPEC_INCOMPLETE` = 6025,
//!   rail-contract-design.md window batch) — a def with a psrc/psbi output row
//!   whose spec block writes only one of input_req / output cannot be gated
//!   (advisory Info). A pure load with no output row legitimately writes
//!   input_req alone.
//! * **§6.6 module-boundary feed** (`rail-contract-design.md` §6.6,
//!   window-notes batch) — a net that reaches only a submodule port is not
//!   dead: the same copper across the boundary is a second NetEntry in the
//!   child scope (A′ junction, `module` differs). When that co-segment has
//!   resolved a supply window (a genuine child-source feed), `WindowDeriv`
//!   forwards it instead of leaving the net NoSupply; a rootless child-sink
//!   co-segment recursing back is Unresolved and stays skipped. Consumers like
//!   6023 then gate the regulator fed across the boundary.
//! * **§6.7 output vs rail window** (`POWER_CONVERTER_OUTPUT_RAIL_WINDOW` =
//!   6026, rail-contract-design.md §6.7) — a regulator's `spec.output`
//!   guarantee must sit inside the declared rail window of the rail net its Src
//!   row drives; a guarantee a genuine (non-degenerate) rail window does not
//!   cover can deliver outside the rail → Error. A Src landing on a plain
//!   driven node, or a bare-nominal rail with no ±tol, is not cross-checked.
//!
//! Golden board (`mcs/pwrint/src/main.mc`) shape: GND carries `@star`, so its
//! two parallel `@bridge(GND, GNDA)` legs are discharged (that island holds the
//! single main root GND), and POWER_USB clamps to its own `@role(protective)`
//! ESDGND with exactly one bridge; GND_ISO's isolated world (V5V_ISO ret
//! GND_ISO) and EARTH declare no DC edge. The two quiet/protective conduits
//! (GNDA, ESDGND) each carry a declared DC bridge, so 6018 stays silent; the
//! two combine shapes — ORING.IDEAL's nominal-only OUT psrc, and the
//! psrc+psbi USB/battery coexistence — keep 6020 silent. None of the new codes
//! fire.

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

/// A sink on a net that is *not* a declared rail face carries no rail guarantee
/// to compare, so this first rule (6011) defers it to the S-set step — the
/// bare `VMID` label is also fed by nothing, so PWR-1 (6019) reports the
/// dead net while 6011 itself stays silent.
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
        "a sink on a non-rail net carries no guarantee to compare (6011 S-set later); got codes: {codes:?}"
    );
    assert!(
        codes.contains(&mcc::errcodes::SINK_NET_NO_SOURCE),
        "the bare VMID net feeds the sink from nothing — PWR-1 (6019) must report it; got codes: {codes:?}"
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

// ────────────────────────────────────────────────────────────────────────────
// PWR-3 source contention (axis ③ — §11 / §13 landing 3).
//
// The narrow kernel: two or more `psrc` HARD sources landing their hot
// terminal on the same net with no declared ORing/combine element between them
// is an undeclared parallel source (6013). Nominal *agreement* does not excuse
// the parallel — ORing is a topological merge, so even two 5V regulators
// wire-ORed to one node still need the declared element. `psbi` (battery
// coexistence, a conditional source) and rail faces (6010's two-roots scope)
// are not source points; copper pass-through propagation / converter
// re-anchoring stay the later S-set step.

/// Two `psrc` on one net — even with *agreeing* nominals — is an undeclared
/// parallel source (the golden never does this: every net there has ≤1 psrc).
#[test]
fn two_psrc_same_nominal_on_one_net_fire_contention() {
    let src = format!(
        "{SRC5}\nmodule main {{\n    conduit GND @role(main)\n    io V5\n    \
         SRC5 a\n    SRC5 b\n    a.OUT -> V5\n    a.GND -> GND\n    \
         b.OUT -> V5\n    b.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::POWER_SOURCE_CONTENTION),
        "two 5V psrc on one net must fire 6013 (undeclared parallel even at the same nominal); got codes: {codes:?}"
    );
}

/// Two `psrc` at *different* nominals on one net — the case the sink nominal
/// check used to defer silently — is the same contention: 6013 replaces the
/// defer. 6011 stays silent because there is no single S to judge sinks against.
#[test]
fn two_psrc_different_nominal_on_one_net_fire_contention() {
    let src = format!(
        "{SRC5}{SRC3}{SINK3}\nmodule main {{\n    conduit GND @role(main)\n    io VX\n    \
         SRC5 a\n    SRC3 b\n    SINK3 k\n    a.OUT -> VX\n    a.GND -> GND\n    \
         b.OUT -> VX\n    b.GND -> GND\n    k.VDD -> VX\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::POWER_SOURCE_CONTENTION)
            && !codes.contains(&mcc::errcodes::POWER_SINK_NOMINAL_MISMATCH),
        "conflicting 5V/3.3V psrc on one net must fire 6013 and leave the net un-adjudicated by 6011; got codes: {codes:?}"
    );
}

/// Golden battery-coexistence shape: one `psrc` (regulator OUT) + one `psbi`
/// (battery BAT) on the same net is NOT contention — the battery is a
/// conditional source. The 3.3V sink still fires 6011 against the agreed 5V S,
/// proving both roots were detected before the psbi was excluded from the count.
#[test]
fn psrc_plus_psbi_on_one_net_is_not_contention() {
    let src = format!(
        "{SRC5}{BAT5}{SINK3}\nmodule main {{\n    conduit GND @role(main)\n    io VB\n    \
         SRC5 a\n    BAT5 b\n    SINK3 k\n    a.OUT -> VB\n    a.GND -> GND\n    \
         b.BAT -> VB\n    b.GND -> GND\n    k.VDD -> VB\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::POWER_SOURCE_CONTENTION)
            && codes.contains(&mcc::errcodes::POWER_SINK_NOMINAL_MISMATCH),
        "a psrc + a psbi on one net must not fire 6013, but the 5V S must still fire the 3.3V sink (6011); got codes: {codes:?}"
    );
}

/// One psrc per net is the healthy shape — sources on *different* nets are
/// never compared.
#[test]
fn two_psrc_on_different_nets_are_clean() {
    let src = format!(
        "{SRC5}\nmodule main {{\n    conduit GND @role(main)\n    io VA\n    io VB\n    \
         SRC5 a\n    SRC5 b\n    a.OUT -> VA\n    a.GND -> GND\n    \
         b.OUT -> VB\n    b.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::POWER_SOURCE_CONTENTION),
        "one psrc per net must not fire 6013; got codes: {codes:?}"
    );
}

/// Golden VDD_3V3 / V5V_ISO shape: a rail face *and* one same-nominal `psrc`
/// on the same net (DVDD rail 3.3V + LDO OUT psrc 3.3V) is one agreeing
/// supply — rail faces are not source points for 6013, so this never fires.
#[test]
fn rail_face_plus_single_psrc_is_not_contention() {
    let src = format!(
        "{SRC3}{SINK3}\nmodule main {{\n    conduit GND @role(main)\n    \
         domain DVDD @class(digital) {{ rail [VS, GND]::DC(3.3V) }}\n    \
         io VS\n    SRC3 a\n    SINK3 k\n    a.OUT -> VS\n    a.GND -> GND\n    \
         k.VDD -> VS\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::POWER_SOURCE_CONTENTION),
        "a rail face + one psrc on the same net must not fire 6013; got codes: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// §3.2 role-relation contract rows 6014 (isolated zero-DC-bridge, PWR-9) and
// 6015 (protective single-point, PWR-8), landed with the loop/clamp rows above.
// ---------------------------------------------------------------------------

/// 6014 fire: an `@role(isolated)` ref DC-`@bridge`d to the main reference is
/// a hard tie out of the zero-DC world — a declared DC bridge across the
/// isolation boundary.
#[test]
fn isolated_ref_dc_bridged_to_main_net_fires_6014() {
    let src = format!(
        "{FB}\nmodule main {{\n    conduit GND     @role(main)\n    conduit GND_ISO @role(isolated)\n    \
         GND_ISO - fb1::FB() - GND @bridge(GND_ISO, GND)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::ISOLATED_DC_BRIDGE),
        "a DC bridge out of an isolated ref must fire 6014; got codes: {codes:?}"
    );
}

/// 6014 fires via the *derived* member too: V5V_ISO carries no role of its own,
/// but its rail returns to `@role(isolated)` GND_ISO (design §4), so a DC
/// `@bridge` from it out to GND is still a bridge out of the isolated world.
#[test]
fn rail_returned_to_isolated_ref_dc_bridged_outside_fires_6014() {
    let src = format!(
        "{FB}\nmodule main {{\n    conduit GND     @role(main)\n    conduit GND_ISO @role(isolated)\n    \
         domain ISO {{ rail [V5V_ISO, GND_ISO]::DC(5V) }}\n    \
         V5V_ISO - fb1::FB() - GND @bridge(V5V_ISO, GND)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::ISOLATED_DC_BRIDGE),
        "a DC bridge out of a rail hot returned to an isolated ref must fire 6014; got codes: {codes:?}"
    );
}

/// 6014 clean (golden GND_ISO mirror): an isolated world that declares no DC
/// edge is silent — the isolation is stated by role, not derived from an edge.
#[test]
fn isolated_world_without_dc_edge_is_clean() {
    let src =
        "module main {\n    conduit GND     @role(main)\n    conduit GND_ISO @role(isolated)\n    \
        domain ISO { rail [V5V_ISO, GND_ISO]::DC(5V) }\n}\n";
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::ISOLATED_DC_BRIDGE),
        "an isolated world with no DC edge must not fire 6014; got codes: {codes:?}"
    );
}

/// 6014 kernel boundary: an isolated↔isolated DC `@bridge` merges two zero-DC
/// worlds — both endpoints are isolated, so it is not a bridge *out* and 6014
/// stays silent (kernel-accepted world merge).
#[test]
fn isolated_to_isolated_dc_bridge_is_kernel_clean() {
    let src = format!(
        "{FB}\nmodule main {{\n    conduit GND_ISO  @role(isolated)\n    conduit GND_ISO2 @role(isolated)\n    \
         GND_ISO2 - fb1::FB() - GND_ISO @bridge(GND_ISO2, GND_ISO)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::ISOLATED_DC_BRIDGE),
        "an isolated-to-isolated DC bridge must not fire 6014 (world merge); got codes: {codes:?}"
    );
}

/// 6015 clean (golden POWER_USB ESDGND mirror): exactly one declared DC
/// `@bridge` to the circuit reference is the protective conduit's single point.
#[test]
fn protective_with_single_dc_bridge_is_clean() {
    let src = format!(
        "{FB}\nmodule main {{\n    conduit ESDGND @role(protective)\n    conduit GND    @role(main)\n    \
         ESDGND - fb1::FB() - GND @bridge(ESDGND, GND)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::PROTECTIVE_MULTI_BRIDGE),
        "one protective DC bridge must not fire 6015; got codes: {codes:?}"
    );
}

/// 6015 fires where 6007 stays silent: a second protective-ground leg is a
/// single-point violation even though `@star` on the far GND hub discharges
/// the 6007 loop — the protective single point is a hard (1,0) invariant.
#[test]
fn protective_second_dc_bridge_fires_6015_even_with_star() {
    let src = format!(
        "{FB}\nmodule main {{\n    conduit ESDGND @role(protective)\n    conduit GND    @role(main) @star\n    \
         ESDGND - fb1::FB() - GND @bridge(ESDGND, GND)\n    \
         ESDGND - fb2::FB() - GND @bridge(ESDGND, GND)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::PROTECTIVE_MULTI_BRIDGE)
            && !codes.contains(&mcc::errcodes::POWER_BRIDGE_LOOP),
        "a second protective bridge must fire 6015 even with @star on the hub (6007 stays silent); got codes: {codes:?}"
    );
}

/// 6016 fire (design §11 chassis/earth scene / §3.2 earth row): an `@role(earth)`
/// conduit DC-`@bridge`d to the circuit reference is a low-resistance chassis
/// direct tie — a leakage warning.
#[test]
fn earth_ref_dc_bridged_to_main_net_leaks_6016() {
    let src = format!(
        "{FB}\nmodule main {{\n    conduit EARTH @role(earth)\n    conduit GND   @role(main)\n    \
         EARTH - fb1::FB() - GND @bridge(EARTH, GND)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::EARTH_DC_LEAK),
        "a DC bridge into an earth ref must leak (6016); got codes: {codes:?}"
    );
}

/// 6016 clean: an earth ref met by a Y-cap `@couple` (the AC-only (0,1)
/// relation) is exactly the legal chassis coupling — no DC row, no leak.
#[test]
fn earth_ref_with_only_ycap_couple_is_clean() {
    let src = format!(
        "{FB}\nmodule main {{\n    conduit EARTH @role(earth)\n    conduit GND   @role(main)\n    \
         EARTH - fb1::FB() - GND @couple(EARTH, GND)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::EARTH_DC_LEAK),
        "a Y-cap @couple into an earth ref is legal — 6016 only rows DC @bridges; got codes: {codes:?}"
    );
}

/// 6016 clean + PWR-7 pass: an ESD clamp into an `@role(earth)` ref is a legal
/// clamp target (6008 accepts protective/earth), and the clamp edge is not a DC
/// bridge so no leak fires.
#[test]
fn clamp_into_earth_ref_is_not_a_leak() {
    let src = format!(
        "{TV}\nmodule main {{\n    conduit EARTH @role(earth)\n    io DP @exposed(esd_contact)\n    \
         TV tv\n    tv.IO -> DP\n    tv.G -> EARTH @clamp(EARTH)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::EARTH_DC_LEAK)
            && !codes.contains(&mcc::errcodes::CLAMP_REF_NOT_PROTECTIVE),
        "a clamp into an earth ref must not leak (6016) nor fire PWR-7 (6008); got codes: {codes:?}"
    );
}

/// 6017 fire (§3.2.1): DC-`@bridge`ing two `@role(main)` islands merges them
/// into one L1 island that then carries two roots — the doc's canonical "two
/// main islands must not be @bridge'd" case.
#[test]
fn two_main_islands_dc_bridged_fire_6017() {
    let src = format!(
        "{FB}\nmodule main {{\n    conduit GND  @role(main)\n    conduit GND2 @role(main)\n    \
         GND - fb1::FB() - GND2 @bridge(GND, GND2)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::REFERENCE_ISLAND_ROOT),
        "a DC bridge between two main islands must fire 6017; got codes: {codes:?}"
    );
}

/// 6017 fire: a DC-bridged reference group with *no* main root (two quiet
/// leaves tied to each other, not to an island main) has no ground to return
/// to — zero mains is as much a violation as two.
#[test]
fn bridge_joined_quiet_group_without_main_fires_6017() {
    let src = format!(
        "{FB}\nmodule main {{\n    conduit GNDA @role(quiet)\n    conduit GNDB @role(quiet)\n    \
         GNDA - fb1::FB() - GNDB @bridge(GNDA, GNDB)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::REFERENCE_ISLAND_ROOT),
        "a DC-bridged quiet group with no main root must fire 6017; got codes: {codes:?}"
    );
}

/// 6017 clean (golden GND/GNDA shape): one main + one quiet DC-`@bridge`d is an
/// island with exactly one root.
#[test]
fn main_quiet_island_with_single_root_is_clean() {
    let src = format!(
        "{FB}\nmodule main {{\n    conduit GND  @role(main)\n    conduit GNDA @role(quiet)\n    \
         GNDA - fb1::FB() - GND @bridge(GNDA, GND)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::REFERENCE_ISLAND_ROOT),
        "one main + one quiet bridged must not fire 6017; got codes: {codes:?}"
    );
}

/// 6017 clean (golden four-role shape): isolated and earth conduits declare no
/// DC bridge, so they are not reference islands and the main+quiet island keeps
/// its single root.
#[test]
fn isolated_earth_singletons_do_not_break_the_main_island() {
    let src = format!(
        "{FB}\nmodule main {{\n    conduit GND     @role(main)\n    conduit GNDA    @role(quiet)\n    \
         conduit GND_ISO @role(isolated)\n    conduit EARTH   @role(earth)\n    \
         GNDA - fb1::FB() - GND @bridge(GNDA, GND)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::REFERENCE_ISLAND_ROOT),
        "isolated/earth singletons must not turn a one-root island into a violation; got codes: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// §8.4 zero-bridge rule 6018 (quiet/protective conduit with no declared DC
// `@bridge`) — conduit-equivalence-design.md §8.4. The upper bound is 6007 /
// 6015; this fires on the bare zero only.
// ---------------------------------------------------------------------------

/// 6018 fire (primary §8.4 scenario): a `@role(quiet)` conduit that is a real
/// supply face — a rail returns to it — but declares no DC `@bridge` is an
/// unwired quiet reference: its loads have no declared return path to the island
/// main. The forgotten `@bridge` must not be silent.
#[test]
fn quiet_face_without_return_bridge_fires_6018() {
    let src = "module main {\n    conduit GND  @role(main)\n    conduit GNDA @role(quiet)\n    \
        domain AV { rail [VDDA, GNDA]::DC(3V3) }\n}\n";
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::ROLE_REF_MISSING_BRIDGE),
        "a quiet rail return with no declared DC bridge must fire 6018; got codes: {codes:?}"
    );
}

/// 6018 fire, bare protective form: an `@role(protective)` conduit with no
/// declared DC `@bridge` has no single point to the circuit reference at all —
/// the protective expectation (§3.2) is one bridge, and zero is unwired.
#[test]
fn protective_conduit_without_dc_bridge_fires_6018() {
    let src =
        "module main {\n    conduit GND    @role(main)\n    conduit ESDGND @role(protective)\n}\n";
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::ROLE_REF_MISSING_BRIDGE),
        "a protective conduit with no declared DC bridge must fire 6018; got codes: {codes:?}"
    );
}

/// 6018 clean (golden GNDA mirror, single leg): a quiet conduit carrying a
/// declared DC `@bridge` to the main reference satisfies the expectation — the
/// upper bound (a second leg = loop) is 6007's job, not this rule's.
#[test]
fn quiet_ref_with_single_dc_bridge_is_clean() {
    let src = format!(
        "{FB}\nmodule main {{\n    conduit GND  @role(main)\n    conduit GNDA @role(quiet)\n    \
         GNDA - fb1::FB() - GND @bridge(GNDA, GND)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::ROLE_REF_MISSING_BRIDGE),
        "a quiet conduit with one declared DC bridge must not fire 6018; got codes: {codes:?}"
    );
}

/// 6018 clean (golden POWER_USB ESDGND mirror): exactly one declared DC bridge
/// is the protective conduit's single point — 6018 counts the bare zero only.
#[test]
fn protective_ref_with_single_dc_bridge_is_clean() {
    let src = format!(
        "{FB}\nmodule main {{\n    conduit ESDGND @role(protective)\n    conduit GND    @role(main)\n    \
         ESDGND - fb1::FB() - GND @bridge(ESDGND, GND)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::ROLE_REF_MISSING_BRIDGE),
        "a protective conduit with one declared DC bridge must not fire 6018; got codes: {codes:?}"
    );
}

/// 6018 discriminator: only a declared DC `@bridge` satisfies the quiet/protective
/// expectation. A Y-cap `@couple` is the (0,1) AC-only relation — a quiet tied
/// only by a couple is still unwired at DC and fires (mirrors how 6016 rows DC
/// only; here the couple does not discharge the zero-bridge count).
#[test]
fn quiet_ref_with_only_ycap_couple_still_fires_6018() {
    let src = format!(
        "{FB}\nmodule main {{\n    conduit GND  @role(main)\n    conduit GNDA @role(quiet)\n    \
         GNDA - fb1::FB() - GND @couple(GNDA, GND)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::ROLE_REF_MISSING_BRIDGE),
        "a Y-cap @couple alone does not tie a quiet conduit at DC — 6018 must fire; got codes: {codes:?}"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// PWR-1 no-source face (§11 / axis ③): a net that carries component power-sink
// (psnk) terminals but no supply root on the net itself — no declared
// domain-rail face, no decodable psrc/psbi hot pin. Net-local, mirroring
// 6011/6013: module boundary feed ports and copper pass-through feed (S
// crossing a fuse/inductor/ferrite from a neighbouring net) stay the S-set
// step, so a root-less net that is silent in 6011 is only legal when it
// carries no demand.

/// 6019 fire, canonical form: a 3.3V sink wired to a bare `io` net that is
/// neither a declared rail face nor driven by any source — the load draws from
/// nothing. This is exactly the net 6011 skips as "intermediate (S-set later)"
/// — once it carries a sink, the skip must not be silent.
#[test]
fn sink_on_rootless_net_fires_6019() {
    let src = format!(
        "{SINK3}\nmodule main {{\n    conduit GND @role(main)\n    \
         io VMID\n    SINK3 s\n    s.VDD -> VMID\n    s.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::SINK_NET_NO_SOURCE),
        "a psnk sink on a net with no supply root must fire 6019 (PWR-1); got codes: {codes:?}"
    );
}

/// 6019 fire, second nominal: a 5V sink on a root-less net fires the same way —
/// PWR-1 does not depend on the sink nominal, only on the missing source root.
#[test]
fn fivesink_on_rootless_net_fires_6019() {
    let src = format!(
        "{SNK5}\nmodule main {{\n    conduit GND @role(main)\n    \
         io VX\n    SNK5 s\n    s.VDD -> VX\n    s.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::SINK_NET_NO_SOURCE),
        "a 5V psnk sink on a root-less net must fire 6019 (PWR-1); got codes: {codes:?}"
    );
}

/// 6019 clean: a sink on its *declared* domain-rail face has a handwritten
/// source root (§4.1) — the domain rail block is the guarantee, so no PWR-1.
#[test]
fn sink_on_declared_rail_face_is_clean_6019() {
    let src = format!(
        "{SINK3}\nmodule main {{\n    conduit GND @role(main)\n    \
         domain DVDD @class(digital) {{ rail [V3V3, GND]::DC(3.3V) }}\n    \
         io V3V3\n    SINK3 s\n    s.VDD -> V3V3\n    s.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::SINK_NET_NO_SOURCE),
        "a sink on a declared rail face carries the domain guarantee — no 6019; got codes: {codes:?}"
    );
}

/// 6019 clean (golden VMAIN_5V shape): a sink on a net driven by a bare `psrc`
/// source pin (no rail face in sight) has an S root on the net — no PWR-1.
#[test]
fn sink_on_psrc_driven_net_is_clean_6019() {
    let src = format!(
        "{SRC5}{SNK5}\nmodule main {{\n    conduit GND @role(main)\n    \
         io VMAIN\n    SRC5 src\n    src.OUT -> VMAIN\n    src.GND -> GND\n    \
         SNK5 load\n    load.VDD -> VMAIN\n    load.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::SINK_NET_NO_SOURCE),
        "a sink on a psrc-driven net is fed on-net — no 6019; got codes: {codes:?}"
    );
}

/// 6019 clean: a root-less net that carries no component power sink at all is a
/// legal intermediate/copper net — PWR-1 only fires on nets that *demand* power.
#[test]
fn rootless_net_without_sink_is_clean_6019() {
    let src = format!(
        "{FB}\nmodule main {{\n    conduit GND @role(main)\n    conduit GNDA @role(quiet)\n    \
         io VX\n    io VY\n    VX - fb1::FB() - VY\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::SINK_NET_NO_SOURCE),
        "a root-less net with no component psnk sink must not fire 6019; got codes: {codes:?}"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// Multi-source combine S-set (§6.2③, rail-contract-design.md §6) — the
// combine-output nominal-only guard. The ∪ window merge itself rides the future
// S-set window engine (§6.3/§7.3); this kernel lands the one piece the design
// prescribes at the current nominal/declaration layer: a *combine element* (a
// def with ≥2 input-direction psnk/psbi rows + a psrc output row) is a
// pass-through OR-merge, so its output psrc writes the merged nominal only.
// A ±tol there re-anchors a window no single live input can hold — 6020, at the
// def's own declaration, once per used class.

/// Golden ORING.IDEAL *clean* shape: two 5V `psnk` input groups + a 5V psrc
/// output writing only its nominal.
const OR2: &str = "component OR2 {\n    pins = [\n        psnk [1,2] = [IN1, G1]::DC(5V)\n        psnk [3,4] = [IN2, G2]::DC(5V)\n        psrc [5,6] = [OUT, G3]::DC(5V)\n    ]\n}\n";

/// The same combine with the §6.2③ over-claim: the output `psrc` carries a tol.
const OR2T: &str = "component OR2T {\n    pins = [\n        psnk [1,2] = [IN1, G1]::DC(5V)\n        psnk [3,4] = [IN2, G2]::DC(5V)\n        psrc [5,6] = [OUT, G3]::DC(5V, tol:±1%)\n    ]\n}\n";

/// A combine whose second input is a `psbi` charge half (BAT row): the Bi
/// direction is an input group too (§6.1), so the output tol still fires.
const ORBT: &str = "component ORBT {\n    pins = [\n        psnk [1,2] = [IN, G1]::DC(5V)\n        psbi [3,4] = [BAT, G2]::DC(5V)\n        psrc [5,6] = [OUT, G3]::DC(5V, tol:±1%)\n    ]\n}\n";

/// A single-input converter (LDO/DCDC shape): its toleranced output is a legal
/// §2 re-anchor — never a combine, so no 6020.
const CONVT: &str = "component CONVT {\n    pins = [\n        psnk [1,2] = [VIN, G1]::DC(5V)\n        psrc [3,4] = [VOUT, G2]::DC(3.3V, tol:±1%)\n    ]\n}\n";

/// 6020 fire: a two-psnk combine whose output psrc carries a ±tol window.
#[test]
fn two_psnk_inputs_with_tol_output_fires_6020() {
    let src = format!(
        "{OR2T}\nmodule main {{\n    conduit GND @role(main)\n    io VA\n    io VB\n    io VC\n    \
         OR2T o\n    o.IN1 -> VA\n    o.G1 -> GND\n    o.IN2 -> VB\n    o.G2 -> GND\n    \
         o.OUT -> VC\n    o.G3 -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::COMBINE_OUTPUT_TOL),
        "a two-psnk combine output carrying tol must fire 6020 (§6.2③); got codes: {codes:?}"
    );
}

/// 6020 fire through a `psbi` second input: a psbi charge half is an input
/// group (§6.1), so 1 psnk + 1 psbi + a toleranced psrc output is still a
/// combine output over-claim.
#[test]
fn psbi_input_counts_for_combine_shape_fires_6020() {
    let src = format!(
        "{ORBT}\nmodule main {{\n    conduit GND @role(main)\n    io VA\n    io VB\n    io VC\n    \
         ORBT o\n    o.IN -> VA\n    o.G1 -> GND\n    o.BAT -> VB\n    o.G2 -> GND\n    \
         o.OUT -> VC\n    o.G3 -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::COMBINE_OUTPUT_TOL),
        "a psbi input must count toward the ≥2-input combine shape, so the output tol fires 6020; got codes: {codes:?}"
    );
}

/// A *single-input* converter (the LDO/DCDC golden shape) may re-anchor a
/// window at its psrc output (§2 Hoare break) — never a combine, so the tol is
/// legal and 6020 stays silent.
#[test]
fn single_input_converter_with_tol_output_is_not_combine() {
    let src = format!(
        "{CONVT}\nmodule main {{\n    conduit GND @role(main)\n    io VA\n    io VC\n    \
         CONVT c\n    c.VIN -> VA\n    c.G1 -> GND\n    c.VOUT -> VC\n    c.G2 -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::COMBINE_OUTPUT_TOL),
        "a single-input converter's toleranced output is a legal re-anchor, not a combine — no 6020; got codes: {codes:?}"
    );
}

/// 6020 clean + the §6.4 seam, in one golden-equivalent board: two 5V psrc
/// sources each feed one combine input group, the nominal-only output psrc
/// roots the merged rail, and a 5V sink on it adjudicates against S = 5V —
/// no 6020 (nominal-only OUT), no 6011 (sink matches the merged nominal), no
/// 6019 (OUT is the merged net's root), no 6013 (sources sit on different nets).
#[test]
fn combine_nominal_only_output_is_clean_6020() {
    let src = format!(
        "{OR2}{SRC5}{SNK5}\nmodule main {{\n    conduit GND @role(main)\n    \
         io VA\n    io VB\n    io VMAIN\n    \
         SRC5 a\n    SRC5 b\n    OR2 o\n    SNK5 load\n    \
         a.OUT -> VA\n    a.GND -> GND\n    b.OUT -> VB\n    b.GND -> GND\n    \
         o.IN1 -> VA\n    o.G1 -> GND\n    o.IN2 -> VB\n    o.G2 -> GND\n    \
         o.OUT -> VMAIN\n    o.G3 -> GND\n    load.VDD -> VMAIN\n    load.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::COMBINE_OUTPUT_TOL)
            && !codes.contains(&mcc::errcodes::POWER_SINK_NOMINAL_MISMATCH)
            && !codes.contains(&mcc::errcodes::SINK_NET_NO_SOURCE)
            && !codes.contains(&mcc::errcodes::POWER_SOURCE_CONTENTION),
        "a nominal-only combine output must stay silent (6020/6011/6019/6013) — the golden ORING/VMAIN_5V seam; got codes: {codes:?}"
    );
}

// ────────────────────────────────────────────────────────────────────────────
// PWR-4 net budget (§8, rail-contract-design.md) — sink `amp` demand vs the
// supply root's capacity, net-local. `amp` is a sink-exclusive opt-in key on
// the psnk `::DC` (§8.1); the budget sums declared demand over the same net a
// capacity-bearing root (domain-rail face / psrc / psbi) governs and fires 6021
// when Σ amp > capacity. A net with no declared capacity (or with disagreeing
// capacity roots) is not adjudicated; converter-input push-up and cross-net
// feed are the S-set step.

/// A regulated source whose output declares its capacity (500mA at 3.3V).
const SRC_CAP: &str = "component SRC_CAP {\n    pins = [\n        psrc [1,2] = [OUT, GND]::DC(3.3V, capacity:500mA)\n    ]\n}\n";

/// A sink that declares its instance demand (`amp:300mA`) on its 3.3V nominal.
const SNK_AMP3: &str = "component SNK_AMP3 {\n    pins = [\n        psnk [1,2] = [VDD, GND]::DC(3.3V, amp:300mA)\n    ]\n}\n";

/// The same demand declaration on a 5V nominal (for capacity-less 5V roots).
const SNK_AMP5: &str = "component SNK_AMP5 {\n    pins = [\n        psnk [1,2] = [VDD, GND]::DC(5V, amp:300mA)\n    ]\n}\n";

/// A 3.3V sink that draws more than the golden VDD_3V3 rail's own capacity.
const SNK_AMP_HI: &str = "component SNK_AMP_HI {\n    pins = [\n        psnk [1,2] = [VDD, GND]::DC(3.3V, amp:600mA)\n    ]\n}\n";

/// 6021 fire through a *psrc* capacity root: two 300mA sinks on a 500mA source
/// net sum to 600mA > 500mA. amp on a sink is legal (no 6012), the nominals
/// match (no 6011), the source root is present (no 6019).
#[test]
fn source_capacity_over_declared_sinks_fires_budget() {
    let src = format!(
        "{SRC_CAP}{SNK_AMP3}\nmodule main {{\n    conduit GND @role(main)\n    \
         io V33\n    SRC_CAP s\n    SNK_AMP3 a\n    SNK_AMP3 b\n    \
         s.OUT -> V33\n    s.GND -> GND\n    a.VDD -> V33\n    a.GND -> GND\n    \
         b.VDD -> V33\n    b.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::NET_BUDGET_EXCEEDED)
            && !codes.contains(&mcc::errcodes::POWER_PIN_DECODE)
            && !codes.contains(&mcc::errcodes::POWER_SINK_NOMINAL_MISMATCH)
            && !codes.contains(&mcc::errcodes::SINK_NET_NO_SOURCE),
        "600mA of declared sink amp on a 500mA psrc net must fire 6021 (and stay clean on 6012/6011/6019); got codes: {codes:?}"
    );
}

/// 6021 through a *domain-rail face* capacity root — the golden VDD_3V3 shape
/// (rail declares capacity, converter output carries none), loaded past budget.
#[test]
fn rail_face_capacity_over_declared_sinks_fires_budget() {
    let src = format!(
        "{SNK_AMP_HI}\nmodule main {{\n    conduit GND @role(main)\n    \
         domain DVDD @class(digital) {{ rail [V3V3, GND]::DC(3.3V, capacity:500mA) }}\n    \
         io V3V3\n    SNK_AMP_HI a\n    a.VDD -> V3V3\n    a.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::NET_BUDGET_EXCEEDED),
        "a 600mA sink on a 500mA domain-rail face must fire 6021; got codes: {codes:?}"
    );
}

/// Within budget on a psrc capacity root is the healthy hookup → silent.
#[test]
fn declared_load_within_source_capacity_is_silent() {
    let src = format!(
        "{SRC_CAP}{SNK_AMP3}\nmodule main {{\n    conduit GND @role(main)\n    \
         io V33\n    SRC_CAP s\n    SNK_AMP3 a\n    \
         s.OUT -> V33\n    s.GND -> GND\n    a.VDD -> V33\n    a.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::NET_BUDGET_EXCEEDED)
            && !codes.contains(&mcc::errcodes::POWER_PIN_DECODE)
            && !codes.contains(&mcc::errcodes::POWER_SINK_NOMINAL_MISMATCH),
        "a 300mA load on a 500mA source must stay silent (6021/6012/6011); got codes: {codes:?}"
    );
}

/// No declared capacity → no budget oracle: even a declared amp load on a
/// capacity-less 5V source stays silent (PWR-4 does not fake a capacity).
#[test]
fn amp_load_on_capacity_less_source_is_silent() {
    let src = format!(
        "{SRC5}{SNK_AMP5}\nmodule main {{\n    conduit GND @role(main)\n    \
         io V5\n    SRC5 s\n    SNK_AMP5 a\n    \
         s.OUT -> V5\n    s.GND -> GND\n    a.VDD -> V5\n    a.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::NET_BUDGET_EXCEEDED),
        "a source with no declared capacity is not a budget root — no 6021 without an oracle; got codes: {codes:?}"
    );
}

/// `amp` on a *source* row is off-register (a source declares capacity, not a
/// net load, §8.1) → 6012, decl-locally.
#[test]
fn amp_on_source_row_fires_pin_decode() {
    let src = "component SRC_BAD {\n    pins = [\n        psrc [1,2] = [OUT, GND]::DC(3.3V, amp:100mA)\n    ]\n}\n\
module main {\n    conduit GND @role(main)\n    io V33\n    SRC_BAD s\n    \
         s.OUT -> V33\n    s.GND -> GND\n}\n";
    let codes = build_codes(src);
    assert!(
        codes.contains(&mcc::errcodes::POWER_PIN_DECODE),
        "an amp demand key on a psrc row must fire 6012 (§8.1 sink-exclusive); got codes: {codes:?}"
    );
}

/// §8.5 return-path completeness (6022, conduit-equivalence-design.md §8.5;
/// first NetIslandIndex consumer, net-island-attribution §7 L2). Two quiet
/// rail-return coppers (GNDA, GNDB) are each DC-bridged to the main GND (golden
/// FB_agnd shape) — but a *bare* two-terminal leg then ties GNDA ↔ GNDB with no
/// declared @bridge/@couple on that net pair. The physical leg is a DC
/// relation between two different resolvable return coppers that the
/// declaration layer never adjudicates → the forgotten bridge must warn.
/// Exactly one finding: the two declared bridge legs are exempt, the bare
/// cross-quiet leg fires.
#[test]
fn undeclared_leg_between_quiet_return_coppers_fires_6022() {
    let src = format!(
        "{FB}\nmodule main {{\n    conduit GND  @role(main) @star\n    \
         conduit GNDA @role(quiet)\n    conduit GNDB @role(quiet)\n    \
         domain DV {{ rail [VDD_3V3, GND]::DC(3V3) }}\n    \
         domain AV {{ rail [VDDA, GNDA]::DC(3V3) }}\n    \
         domain BV {{ rail [VDDB, GNDB]::DC(3V3) }}\n    \
         GNDA - ba::FB() - GND @bridge(GNDA, GND)\n    \
         GNDB - bb::FB() - GND @bridge(GNDB, GND)\n    \
         GNDA - rr::FB() - GNDB\n}}\n"
    );
    let codes = build_codes(&src);
    let n = codes
        .iter()
        .filter(|&&c| c == mcc::errcodes::RETURN_LEG_UNDECLARED)
        .count();
    assert_eq!(
        n, 1,
        "the bare GNDA↔GNDB leg must be the only undeclared return leg (6022 ×1); got codes: {codes:?}"
    );
}

/// Control for the firing shape: the *same* geometry with every return leg
/// carrying its declaration — the bare cross-quiet leg now declares
/// `@bridge(GNDA, GNDB)` — leaves no undeclared physical tie, so 6022 is silent
/// (6007's declared-loop bookkeeping is @star-discharged and not this rule's).
#[test]
fn declared_return_legs_stay_silent_on_6022() {
    let src = format!(
        "{FB}\nmodule main {{\n    conduit GND  @role(main) @star\n    \
         conduit GNDA @role(quiet)\n    conduit GNDB @role(quiet)\n    \
         domain DV {{ rail [VDD_3V3, GND]::DC(3V3) }}\n    \
         domain AV {{ rail [VDDA, GNDA]::DC(3V3) }}\n    \
         domain BV {{ rail [VDDB, GNDB]::DC(3V3) }}\n    \
         GNDA - ba::FB() - GND @bridge(GNDA, GND)\n    \
         GNDB - bb::FB() - GND @bridge(GNDB, GND)\n    \
         GNDA - rr::FB() - GNDB @bridge(GNDA, GNDB)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::RETURN_LEG_UNDECLARED),
        "all return legs declared → no 6022; got codes: {codes:?}"
    );
}

/// §8.5 first-layer boundary (data-gap 2): exemption is per *declared net
/// pair*, not per leg carrier. A second bare leg paralleling an already-declared
/// pair (GNDA ↔ GND declared once, a duplicate return ferrite left bare) is
/// exempt — its physical carrier cannot yet be matched to the @bridge
/// statement (L1Edge holds endpoints+span, not the leg), so a deliberately
/// parallel return path must carry its own declaration to be visible. Locking
/// the accepted granularity so the deferral reads as intent, not as a miss.
#[test]
fn bare_parallel_leg_on_declared_pair_stays_exempt_6022() {
    let src = format!(
        "{FB}\nmodule main {{\n    conduit GND  @role(main) @star\n    \
         conduit GNDA @role(quiet)\n    \
         domain AV {{ rail [VDDA, GNDA]::DC(3V3) }}\n    \
         GNDA - ba::FB() - GND @bridge(GNDA, GND)\n    \
         GNDA - rr::FB() - GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::RETURN_LEG_UNDECLARED),
        "a bare leg on an already-declared net pair is exempt at L2 (data-gap 2); got codes: {codes:?}"
    );
}

/// Decoupling carve-out: a two-terminal passive across the supply face
/// (hot↔return, e.g. a rail decoupling cap) is not a return-relation leg —
/// 6022 audits *return-side* coppers only, so bare hot↔return legs stay silent
/// even with no @bridge anywhere.
#[test]
fn decoupling_legs_across_hot_return_are_not_audited() {
    let src = format!(
        "{FB}\nmodule main {{\n    conduit GND @role(main) @star\n    \
         domain DV {{ rail [VDD_3V3, GND]::DC(3V3) }}\n    \
         VDD_3V3 - d1::FB() - GND\n    \
         VDD_3V3 - d2::FB() - GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::RETURN_LEG_UNDECLARED),
        "hot↔return legs are decoupling, not undeclared return relations; got codes: {codes:?}"
    );
}

// ============================================================================
// Window batch (rail-contract-design.md §6.1/§6.3) — S(net) as a closed
// interval, judged by spec + structure, never by name. A2/A3 landed the shared
// `WindowDeriv::window_of_net` engine; these fixtures exercise its two first
// consumers (6023 regulator gate, 6024 sink req window).
// ============================================================================

/// A regulator family component: one Snk input row + one Src output row, with a
/// component-level `spec` block writing the Hoare gate (`input_req`, the input
/// window inside which the output holds) and the `output` post-condition.
/// Returns are per-row distinct members tied to the GND net in the board
/// (mirrors the golden LDO/DCDC list-pair shape, no library dependency).
const CONV: &str = "component CONV {\n    pins = [\n        psnk [1,2] = [VIN, GN1]::DC(5V)\n        psrc [3,4] = [VOUT, GN2]::DC(3.3V)\n    ]\n    spec = [\n        input_req = 5.5V ~ 6.0V\n        output    = 3.2V ~ 3.4V\n    ]\n}\n";

/// Same regulator shape with an input window wide enough for a 5V point feed.
const CONV_WIDE: &str = "component CONV_WIDE {\n    pins = [\n        psnk [1,2] = [VIN, GN1]::DC(5V)\n        psrc [3,4] = [VOUT, GN2]::DC(3.3V)\n    ]\n    spec = [\n        input_req = 4.5V ~ 5.5V\n        output    = 3.2V ~ 3.4V\n    ]\n}\n";

/// A pure load that states the supply window it accepts (`input_req` only — no
/// output post-condition, so it is a load, never a regulator).
const LOAD_REQ: &str =
    "component LOAD_REQ {\n    pins = [\n        psnk [1,2] = [VDD, GND]::DC(5V)\n    ]\n    spec = [ input_req = 4.9V ~ 5.1V ]\n}\n";

/// A psbi cell whose discharge tolerance spreads wider than ±5%.
const BATW: &str =
    "component BATW {\n    pins = [\n        psbi [1,2] = [BAT, GND]::DC(5V, tol:±8%)\n    ]\n}\n";

/// A load accepting [4.7V, 6.0V] — the union-window target below.
const LOAD_HI: &str =
    "component LOAD_HI {\n    pins = [\n        psnk [1,2] = [VDD, GND]::DC(5V)\n    ]\n    spec = [ input_req = 4.7V ~ 6.0V ]\n}\n";

/// 6023 gate — fixture (1): a 5V point feed on the regulator's input net lies
/// outside the declared `input_req` 5.5V~6.0V → the gate fires.
#[test]
fn regulator_input_point_outside_req_fires_6023() {
    let src = format!(
        "{SRC5}{CONV}{SINK3}\nmodule main {{\n    conduit GND @role(main)\n    \
         io VIN\n    io VOUT\n    SRC5 s\n    CONV c\n    SINK3 k\n    \
         s.OUT -> VIN\n    s.GND -> GND\n    \
         c.VIN -> VIN\n    c.GN1 -> GND\n    \
         c.VOUT -> VOUT\n    c.GN2 -> GND\n    \
         k.VDD -> VOUT\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::POWER_CONVERTER_GATE),
        "a 5V point on a 5.5V~6.0V input window must fire 6023 (S(input) ⊄ input_req); got codes: {codes:?}"
    );
}

/// 6023 gate — control: the same regulator declares `input_req` 4.5V~5.5V; the
/// 5V point feed is inside → no gate.
#[test]
fn regulator_input_point_inside_req_is_clean_6023() {
    let src = format!(
        "{SRC5}{CONV_WIDE}{SINK3}\nmodule main {{\n    conduit GND @role(main)\n    \
         io VIN\n    io VOUT\n    SRC5 s\n    CONV_WIDE c\n    SINK3 k\n    \
         s.OUT -> VIN\n    s.GND -> GND\n    \
         c.VIN -> VIN\n    c.GN1 -> GND\n    \
         c.VOUT -> VOUT\n    c.GN2 -> GND\n    \
         k.VDD -> VOUT\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::POWER_CONVERTER_GATE),
        "a 5V point inside a 4.5V~5.5V input window must pass 6023; got codes: {codes:?}"
    );
}

/// 6024 — fixture (2): a ±5% source puts a [4.75V, 5.25V] window on the load's
/// net; the load only accepts [4.9V, 5.1V] → the supply escapes it → fires.
#[test]
fn supply_window_escaping_load_req_fires_6024() {
    let src = format!(
        "{SRC_FULL}{LOAD_REQ}\nmodule main {{\n    conduit GND @role(main)\n    io V5\n    \
         SRC_FULL s\n    LOAD_REQ k\n    \
         s.OUT -> V5\n    s.GND -> GND\n    \
         k.VDD -> V5\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::POWER_SINK_WINDOW_MISMATCH),
        "a ±5% supply on a 4.9V~5.1V load must fire 6024 (S(net) ⊄ input_req); got codes: {codes:?}"
    );
}

/// 6024 — control: a 5V point feed sits inside the 4.9V~5.1V accepted window →
/// clean.
#[test]
fn point_supply_inside_load_req_is_clean_6024() {
    let src = format!(
        "{SRC5}{LOAD_REQ}\nmodule main {{\n    conduit GND @role(main)\n    io V5\n    \
         SRC5 s\n    LOAD_REQ k\n    \
         s.OUT -> V5\n    s.GND -> GND\n    \
         k.VDD -> V5\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::POWER_SINK_WINDOW_MISMATCH),
        "a 5V point inside a 4.9V~5.1V accepted window must pass 6024; got codes: {codes:?}"
    );
}

/// 6024 through the OR-merge ∪ — fixture (3): OR2 (two psnk + a psrc, no spec)
/// merges SRC_FULL ([4.75, 5.25]) and BATW ([4.6, 5.4]) into ∪ = [4.6, 5.4];
/// only the union can catch that LOAD_HI's [4.7, 6.0] accepted window is
/// escaped on the low side (an intersection or converter-re-anchor reading
/// would return [4.75, 5.25] ⊆ [4.7, 6.0] and stay wrongly silent).
#[test]
fn union_window_escaping_load_req_fires_6024() {
    let src = format!(
        "{SRC_FULL}{BATW}{OR2}{LOAD_HI}\nmodule main {{\n    conduit GND @role(main)\n    \
         io VIN1\n    io VIN2\n    io VMAIN\n    \
         SRC_FULL a\n    BATW b\n    OR2 o\n    LOAD_HI k\n    \
         a.OUT -> VIN1\n    a.GND -> GND\n    \
         b.BAT -> VIN2\n    b.GND -> GND\n    \
         o.IN1 -> VIN1\n    o.G1 -> GND\n    \
         o.IN2 -> VIN2\n    o.G2 -> GND\n    \
         o.OUT -> VMAIN\n    o.G3 -> GND\n    \
         k.VDD -> VMAIN\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::POWER_SINK_WINDOW_MISMATCH),
        "∪ [4.6, 5.4] must escape LOAD_HI's [4.7, 6.0] and fire 6024 (only the ∪ reading catches it); got codes: {codes:?}"
    );
}

/// 6024 ∪ control: drop the OR2 merge — one ±5% source feeds VMAIN directly, so
/// S = [4.75, 5.25] ⊆ [4.7, 6.0] → clean.
#[test]
fn single_source_union_control_is_clean_6024() {
    let src = format!(
        "{SRC_FULL}{LOAD_HI}\nmodule main {{\n    conduit GND @role(main)\n    io VMAIN\n    \
         SRC_FULL a\n    LOAD_HI k\n    \
         a.OUT -> VMAIN\n    a.GND -> GND\n    \
         k.VDD -> VMAIN\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::POWER_SINK_WINDOW_MISMATCH),
        "a single ±5% source [4.75, 5.25] inside [4.7, 6.0] must pass 6024; got codes: {codes:?}"
    );
}

/// All-clear window board (pwrint mirror without modules): a point 5V feed → a
/// fully-spec'd regulator (wide input window) → a declared 3.3V rail, into a
/// nominal-only sink. Neither 6023 nor 6024 may appear.
#[test]
fn full_window_board_is_clean_6023_6024() {
    let src = format!(
        "{SRC5}{CONV_WIDE}{SINK3}\nmodule main {{\n    conduit GND @role(main)\n    \
         domain DVDD @class(digital) {{ rail [V3V3, GND]::DC(3.3V) }}\n    \
         io VIN\n    io V3V3\n    SRC5 s\n    CONV_WIDE c\n    SINK3 k\n    \
         s.OUT -> VIN\n    s.GND -> GND\n    \
         c.VIN -> VIN\n    c.GN1 -> GND\n    \
         c.VOUT -> V3V3\n    c.GN2 -> GND\n    \
         k.VDD -> V3V3\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::POWER_CONVERTER_GATE),
        "a 5V point inside a wide input window must not fire 6023; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::POWER_SINK_WINDOW_MISMATCH),
        "a nominal-only 3.3V sink on its declared rail declares no req window — no 6024; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::POWER_CONVERTER_SPEC_INCOMPLETE),
        "a fully-spec'd regulator writes both input_req and output — no 6025; got codes: {codes:?}"
    );
}

// ---- 6025 partial-spec advisory -------------------------------------------

/// A one-sided regulator: psnk input row + psrc output row, but the spec block
/// writes only the `output` post-condition — no `input_req` pre-condition, so
/// 6023 cannot gate it and 6024 cannot judge it. Advisory Info, decl-local.
const HALF: &str = "component HALF {\n    pins = [\n        psnk [1,2] = [VIN, GN1]::DC(5V)\n        psrc [3,4] = [VOUT, GN2]::DC(3.3V)\n    ]\n    spec = [\n        output = 3.2V ~ 3.4V\n    ]\n}\n";

/// 6025 — fixture (4): an output-only regulator spec is a one-sided Hoare
/// triple → exactly one Info. The written output still decodes (so the output
/// net is not adjudicated by 6024), and there is no input_req to gate.
#[test]
fn output_only_regulator_fires_one_6025() {
    let src = format!(
        "{SRC5}{HALF}{SINK3}\nmodule main {{\n    conduit GND @role(main)\n    \
         io VIN\n    io VOUT\n    SRC5 s\n    HALF h\n    SINK3 k\n    \
         s.OUT -> VIN\n    s.GND -> GND\n    \
         h.VIN -> VIN\n    h.GN1 -> GND\n    \
         h.VOUT -> VOUT\n    h.GN2 -> GND\n    \
         k.VDD -> VOUT\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let n = codes
        .iter()
        .filter(|&&c| c == mcc::errcodes::POWER_CONVERTER_SPEC_INCOMPLETE)
        .count();
    assert_eq!(
        n, 1,
        "an output-only regulator spec must fire exactly one 6025 Info; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::POWER_CONVERTER_GATE),
        "no input_req → no 6023 gate on HALF; got codes: {codes:?}"
    );
}

/// 6025 — control: a pure load (psnk only, no output row) writes `input_req`
/// alone — that is its accepted window, not a half regulator → no 6025.
#[test]
fn input_req_only_load_never_fires_6025() {
    let src = format!(
        "{SRC5}{LOAD_REQ}\nmodule main {{\n    conduit GND @role(main)\n    io V5\n    \
         SRC5 s\n    LOAD_REQ k\n    \
         s.OUT -> V5\n    s.GND -> GND\n    \
         k.VDD -> V5\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::POWER_CONVERTER_SPEC_INCOMPLETE),
        "a load's input_req-only spec is its own — no 6025; got codes: {codes:?}"
    );
}

// ============================================================================
// Window-notes batch (rail-contract-design.md §6.6/§6.7) — module-boundary S
// feed forwarding + the converter-output-vs-rail-window cross-check (6026).
// ============================================================================

/// A submodule that contains its own 5V `psrc` source and exports it through an
/// `io` member — the A′ module-boundary feed §6.6's forward arm carries. The
/// source is top-level `SRC5` (component defs are file-scoped and visible to
/// module bodies, as in the golden POWER_USB / net-island fixtures).
const FEED: &str = "module FEED {\n    conduit GND @role(main)\n    io V5_OUT\n    \
                    SRC5 s\n    s.OUT -> V5_OUT\n    s.GND -> GND\n}\n";

/// §6.6 module-boundary feed — fixture: main's regulator CONV (input_req
/// 5.5V~6.0V) draws its input net from the child FEED's exported 5V source
/// across the boundary. Without the forward arm the main input net is a
/// module-boundary leave (NoSupply) and 6023 stays silent; §6.6 forwards the
/// Resolved 5V point from the child co-segment so the gate adjudicates and
/// fires (5V ⊄ 5.5V~6.0V).
#[test]
fn module_boundary_feed_resolves_regulator_input_fires_6023() {
    let src = format!(
        "{SRC5}{FEED}{CONV}{SINK3}\nmodule main {{\n    conduit GND @role(main)\n    \
         io VIN\n    io VOUT\n    FEED f\n    CONV c\n    SINK3 k\n    \
         f.V5_OUT -> VIN\n    \
         c.VIN -> VIN\n    c.GN1 -> GND\n    \
         c.VOUT -> VOUT\n    c.GN2 -> GND\n    \
         k.VDD -> VOUT\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::POWER_CONVERTER_GATE),
        "a child-fed 5V point across the module boundary must forward to the regulator input and fire 6023 (S(input)=5V ⊄ input_req 5.5V~6.0V); got codes: {codes:?}"
    );
}

/// §6.6 control — the same boundary feed into a regulator whose input_req
/// covers 5V: the forwarded window is adjudicated and passes (no 6023).
#[test]
fn module_boundary_feed_inside_input_req_is_clean_6023() {
    let src = format!(
        "{SRC5}{FEED}{CONV_WIDE}{SINK3}\nmodule main {{\n    conduit GND @role(main)\n    \
         io VIN\n    io VOUT\n    FEED f\n    CONV_WIDE c\n    SINK3 k\n    \
         f.V5_OUT -> VIN\n    \
         c.VIN -> VIN\n    c.GN1 -> GND\n    \
         c.VOUT -> VOUT\n    c.GN2 -> GND\n    \
         k.VDD -> VOUT\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::POWER_CONVERTER_GATE),
        "a child-fed 5V point inside a 4.5V~5.5V input window must pass 6023; got codes: {codes:?}"
    );
}

/// A regulator whose output guarantee `[4.5V, 5.5V]` is a genuine interval a
/// ±1% rail window ([4.95, 5.05]) does not cover — the §6.7 mismatch target.
/// The input_req is wide, so the feed side (6023) stays quiet.
const REG_OFF: &str = "component REG_OFF {\n    pins = [\n        psnk [1,2] = [VIN, GN1]::DC(5V)\n        psrc [3,4] = [VOUT, GN2]::DC(5V)\n    ]\n    spec = [\n        input_req = 4.5V ~ 5.5V\n        output    = 4.5V ~ 5.5V\n    ]\n}\n";

/// The same regulator whose output guarantee `[4.98V, 5.02V]` sits inside the
/// ±1% rail window.
const REG_OK: &str = "component REG_OK {\n    pins = [\n        psnk [1,2] = [VIN, GN1]::DC(5V)\n        psrc [3,4] = [VOUT, GN2]::DC(5V)\n    ]\n    spec = [\n        input_req = 4.5V ~ 5.5V\n        output    = 4.98V ~ 5.02V\n    ]\n}\n";

/// 6026 — fixture (5): the regulator drives its Src output straight onto a
/// declared rail face whose ±1% window [4.95, 5.05] does not cover the
/// guaranteed [4.5, 5.5] → the converter can deliver outside the rail.
#[test]
fn converter_output_escaping_rail_window_fires_6026() {
    let src = format!(
        "{SRC5}{REG_OFF}\nmodule main {{\n    conduit GND @role(main)\n    \
         domain DVDD @class(digital) {{ rail [V5R, GND]::DC(5V, tol:±1%) }}\n    \
         io V5R\n    io VIN\n    SRC5 s\n    REG_OFF c\n    \
         s.OUT -> VIN\n    s.GND -> GND\n    \
         c.VIN -> VIN\n    c.GN1 -> GND\n    \
         c.VOUT -> V5R\n    c.GN2 -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::POWER_CONVERTER_OUTPUT_RAIL_WINDOW),
        "an output guarantee [4.5, 5.5] on a ±1% rail window [4.95, 5.05] must fire 6026; got codes: {codes:?}"
    );
}

/// 6026 — control: the same rail, an output guarantee inside it → clean.
#[test]
fn converter_output_inside_rail_window_is_clean_6026() {
    let src = format!(
        "{SRC5}{REG_OK}\nmodule main {{\n    conduit GND @role(main)\n    \
         domain DVDD @class(digital) {{ rail [V5R, GND]::DC(5V, tol:±1%) }}\n    \
         io V5R\n    io VIN\n    SRC5 s\n    REG_OK c\n    \
         s.OUT -> VIN\n    s.GND -> GND\n    \
         c.VIN -> VIN\n    c.GN1 -> GND\n    \
         c.VOUT -> V5R\n    c.GN2 -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::POWER_CONVERTER_OUTPUT_RAIL_WINDOW),
        "an output guarantee [4.98, 5.02] inside a ±1% rail window must pass 6026; got codes: {codes:?}"
    );
}

/// 6026 — non-trigger: the Src output lands on a plain driven net (no declared
/// rail face) — there is no scope-level window to cross-check. Mirrors the buck
/// `LX` → filter → rail net case.
#[test]
fn converter_output_on_plain_driven_net_never_fires_6026() {
    let src = format!(
        "{SRC5}{REG_OFF}\nmodule main {{\n    conduit GND @role(main)\n    \
         io V5R\n    io VIN\n    SRC5 s\n    REG_OFF c\n    \
         s.OUT -> VIN\n    s.GND -> GND\n    \
         c.VIN -> VIN\n    c.GN1 -> GND\n    \
         c.VOUT -> V5R\n    c.GN2 -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::POWER_CONVERTER_OUTPUT_RAIL_WINDOW),
        "a Src output on a net with no declared rail face must not fire 6026; got codes: {codes:?}"
    );
}
