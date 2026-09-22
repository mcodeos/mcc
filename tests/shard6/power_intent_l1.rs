// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Power-intent L1 relation-edge ERC (intent-design.md §3 / §13 landing 1).
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
//! * **§3.1 rail axis vs @nature** (`RAIL_NATURE_MISMATCH` = 6034,
//!   ac-axis-interface-design.md §6 R4) — a domain's `@nature(ac|dc)` word and
//!   the `::AC*`/`::DC` contract of a rail inside it declare the same axis, so
//!   writing both makes them agree (advisory Info). Only the both-written
//!   contradiction is judged, decl-locally on the rail row: a missing word is
//!   the registered default (the rail contract states the axis alone), and each
//!   side is mapped onto an axis rather than compared by spelling.
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

use crate::common;

use mcc::{McIds, McURI};

/// A two-pin ferrite-like device, declared in-file so the tests don't depend on
/// the installed mcode library.
const FB: &str = "component FB {\n    pins = [\n        io [1,2] = [X, Y]\n    ]\n}\n";

/// A TVS-like device: one signal pin + one clamp-reference pin.
const TV: &str = "component TV {\n    pins = [\n        io 1 = IO\n        psnk 2 = G\n    ]\n}\n";

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

/// The messages of one code (for wording assertions).
fn msgs_of(code: u32, src: &str) -> Vec<String> {
    let _lock = common::lock();
    common::reset();
    let uri: McURI = "/mcc/power-intent-l1.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");
    mcc::mcc_diagnose_all()
        .iter()
        .filter(|d| d.code == code)
        .map(|d| d.msg.clone())
        .collect()
}

/// The flat electrical net-check rows in **emission order**, as
/// `(code, net_name, message)`.
///
/// [`build_codes`] sorts, which is what a code-set assertion wants and exactly
/// what an *order* assertion must not do. This is the other half: the sequence
/// `run_net_checks` hands the report, which `src/output/net_check.rs` prints in
/// the order it arrives — so that sequence is a product, and build-design §3.7
/// discipline 4 holds it to the input. Mirrors the call in `src/cmds/check.rs`
/// (`mcb_pass2_flat` → `run_net_checks`).
fn net_rows(src: &str) -> Vec<(u32, String, String)> {
    let _lock = common::lock();
    common::reset();
    let uri: McURI = "/mcc/power-intent-l1.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let entry = mcc::McSpaceName {
        ident: McIds::from("main"),
        uri: mcc::uri_intern(&uri),
    };
    let (_tree, table) = mcc::mcb_pass2_flat(&entry, 1).expect("pass2_flat failed");
    mcc::check::nets::run_net_checks(&table)
        .iter()
        .map(|r| (r.code, r.net_name.clone(), r.message.clone()))
        .collect()
}

/// One code's rows, in emission order, as `(net_name, message)`.
fn rows_of(code: u32, src: &str) -> Vec<(String, String)> {
    net_rows(src)
        .into_iter()
        .filter(|(c, _, _)| *c == code)
        .map(|(_, n, m)| (n, m))
        .collect()
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

// Landing 2 — rail DC-contract Volt decode (intent-design.md §4 / §13).
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

// §3.1 @nature word vs rail axis (ac-axis-interface-design.md §6 R4, ruled
// 2026-09-16).
//
// The domain's `@nature(ac|dc)` word and the `::AC*`/`::DC` contract of a rail
// declared inside it name the same axis, so writing both makes them agree. The
// two sides are written in different vocabularies (a lowercase value word
// against a `::` iface name), so the verdict maps each onto an axis instead of
// comparing spellings.

/// A face whose `@nature` word contradicts a rail inside it is reported, in
/// both directions, at the contradicting rail row.
#[test]
fn rail_nature_contradicting_rail_axis_is_reported() {
    let src = "module main {\n    conduit GND @role(main)\n    \
        domain MAINS @nature(ac) { rail [VBUS, GND]::DC(310V) }\n    \
        domain VBULK @nature(dc) { rail [L, N]::AC(230V, 50Hz) }\n}\n";
    let codes = build_codes(src);
    assert!(
        codes.contains(&mcc::errcodes::RAIL_NATURE_MISMATCH),
        "an @nature(ac) face with a ::DC rail (and the mirror) must fire 6034; got codes: {codes:?}"
    );
    let msgs = msgs_of(mcc::errcodes::RAIL_NATURE_MISMATCH, src);
    assert!(
        msgs.iter()
            .any(|m| m.contains("declares @nature(ac)") && m.contains("writes DC")),
        "6034 must name the word and the contradicting contract: {msgs:?}"
    );
    assert!(
        msgs.iter()
            .any(|m| m.contains("declares @nature(dc)") && m.contains("writes AC")),
        "the mirror direction must be named too: {msgs:?}"
    );
}

/// Silence where there is nothing to contradict: agreeing words, a face writing
/// no word (the registered default — its rail contract states the axis alone),
/// and a rail whose iface names no axis at all.
#[test]
fn rail_nature_agreement_and_absence_are_clean() {
    let src = "module main {\n    conduit GND @role(main)\n    \
        domain MAINS @nature(ac) { rail [L, N]::AC(230V, 50Hz) }\n    \
        domain VBULK @nature(dc) { rail [VB, GND]::DC(310V) }\n    \
        domain PLAIN { rail [VDD, GND]::DC(3.3V) }\n}\n";
    let codes = build_codes(src);
    assert!(
        !codes.contains(&mcc::errcodes::RAIL_NATURE_MISMATCH),
        "agreeing words and absent words must stay silent; got codes: {codes:?}"
    );
}

// E-PWR-001 — sink nominal vs derived net supply S (intent-design.md
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

// §3.2 role-relation contract rows 6014 (isolated zero-DC-bridge, PWR-9) and
// 6015 (protective single-point, PWR-8), landed with the loop/clamp rows above.

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

/// 6015 emits one row per over-bridged conduit, and the rows come in an order
/// the **source** determines (build-design §3.7 discipline 4).
///
/// The rule counts incident bridges per endpoint name in a map and then walks
/// it to emit; a `HashMap` there would make the report's row order a property
/// of the process. Two conduits, each with two legs, give the map two entries —
/// and the assertion is the canonical (ascending-name) order, not merely
/// "the same twice": a frozen arbitrary order is stable too, and is still not
/// an order the input determines.
#[test]
fn protective_multi_bridge_rows_follow_the_inputs_order() {
    let src = format!(
        "{FB}\nmodule main {{\n    conduit ESDGND_A @role(protective)\n    conduit ESDGND_B @role(protective)\n    conduit GND @role(main)\n    \
         ESDGND_A - fb1::FB() - GND @bridge(ESDGND_A, GND)\n    \
         ESDGND_A - fb2::FB() - GND @bridge(ESDGND_A, GND)\n    \
         ESDGND_B - fb3::FB() - GND @bridge(ESDGND_B, GND)\n    \
         ESDGND_B - fb4::FB() - GND @bridge(ESDGND_B, GND)\n}}\n"
    );
    let rows = rows_of(mcc::errcodes::PROTECTIVE_MULTI_BRIDGE, &src);
    assert!(
        rows.len() >= 2,
        "two over-bridged conduits must give 6015 two rows to order; got {}: {rows:?}",
        rows.len()
    );
    let names: Vec<&str> = rows.iter().map(|(n, _)| n.as_str()).collect();
    let mut ascending = names.clone();
    ascending.sort_unstable();
    assert_eq!(
        names, ascending,
        "6015's rows are not in the source's own name order — the emitted order \
         comes from a container the input does not order (build-design §3.7 \
         discipline 4)"
    );
}

/// The same lock for **6017**, whose rows are one per DC-bridged reference
/// island.
///
/// Four **disjoint** bridged pairs, so the component map has four entries: its
/// iteration order would be the row order, and four entries make that order a
/// permutation rather than a coin flip. The rule orders components by their
/// first member index, which is source order (the members are collected by
/// walking the island names in `names` order as the bridges are read). The
/// row's `net_name` is the island's first member, so the assertion is that the
/// rows come out in the order the source writes the pairs.
#[test]
fn reference_island_root_rows_follow_the_inputs_order() {
    let src = format!(
        "{FB}\nmodule main {{\n    \
         conduit REF_A @role(quiet)\n    conduit REF_B @role(quiet)\n    \
         conduit REF_C @role(quiet)\n    conduit REF_D @role(quiet)\n    \
         conduit REF_E @role(quiet)\n    conduit REF_F @role(quiet)\n    \
         conduit REF_G @role(quiet)\n    conduit REF_H @role(quiet)\n    \
         REF_A - fb1::FB() - REF_B @bridge(REF_A, REF_B)\n    \
         REF_C - fb2::FB() - REF_D @bridge(REF_C, REF_D)\n    \
         REF_E - fb3::FB() - REF_F @bridge(REF_E, REF_F)\n    \
         REF_G - fb4::FB() - REF_H @bridge(REF_G, REF_H)\n}}\n"
    );
    let rows = rows_of(mcc::errcodes::REFERENCE_ISLAND_ROOT, &src);
    assert!(
        rows.len() >= 4,
        "four DC-bridged quiet pairs with no main root must give 6017 four \
         rows to order; got {}: {rows:?}",
        rows.len()
    );
    let names: Vec<&str> = rows.iter().map(|(n, _)| n.as_str()).collect();
    let mut ascending = names.clone();
    ascending.sort_unstable();
    assert_eq!(
        names, ascending,
        "6017's rows are not in the source's own order — the emitted order comes \
         from a container the input does not order (build-design §3.7 \
         discipline 4)"
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

// §8.4 zero-bridge rule 6018 (quiet/protective conduit with no declared DC
// `@bridge`) — conduit-equivalence-design.md §8.4. The upper bound is 6007 /
// 6015; this fires on the bare zero only.

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

// PWR-4 net budget (§8/§8.5, rail-contract-design.md) — sink `amp` demand vs
// the supply root's capacity, supply-root scope. `amp` is a sink-exclusive
// opt-in key on the psnk `::DC` (§8.1); the budget accumulates declared demand
// onto the budget root governing each net and fires 6021 when Σ amp > capacity.
// A net whose own capacity-bearing root (domain-rail face / psrc / psbi)
// governs it is the net-local §8.2 case; a rootless net reached through
// current-transparent copper / a module boundary inherits its upstream root
// (§8.5). A net with no declared capacity (or with disagreeing capacity roots)
// is a source boundary, not adjudicated; converter-input push-up and the
// combine single-source-mode budget remain the deferred S-set tail.

/// A regulated source whose output declares its capacity (500mA at 3.3V).
const SRC_CAP: &str = "component SRC_CAP {\n    pins = [\n        psrc [1,2] = [OUT, GND]::DC(3.3V, capacity:500mA)\n    ]\n}\n";

/// A sink that declares its instance demand (`amp:300mA`) on its 3.3V nominal.
const SNK_AMP3: &str = "component SNK_AMP3 {\n    pins = [\n        psnk [1,2] = [VDD, GND]::DC(3.3V, amp:300mA)\n    ]\n}\n";

/// The same demand declaration on a 5V nominal (for capacity-less 5V roots).
const SNK_AMP5: &str = "component SNK_AMP5 {\n    pins = [\n        psnk [1,2] = [VDD, GND]::DC(5V, amp:300mA)\n    ]\n}\n";

/// A 3.3V sink that draws more than the golden VDD_3V3 rail's own capacity.
const SNK_AMP_HI: &str = "component SNK_AMP_HI {\n    pins = [\n        psnk [1,2] = [VDD, GND]::DC(3.3V, amp:600mA)\n    ]\n}\n";

/// A 3.3V sink drawing a share small enough that two legs stay within a 500mA
/// root — the within-budget half of the copper split (§8.5).
const SNK_AMP2: &str = "component SNK_AMP2 {\n    pins = [\n        psnk [1,2] = [VDD, GND]::DC(3.3V, amp:200mA)\n    ]\n}\n";

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

// PWR-4 root-scope budget (§8.5, rail-contract-design.md) — 6021 raised from
// net-local to supply-root scope. A capacity root governs downstream nets
// separated from it only by current-transparent copper (a two-pin element with
// no DC rows: fuse/ferrite/inductor) or a module-boundary junction, so a split
// leg no longer escapes its upstream rail's budget. A rootless net reached via
// copper accumulates onto the upstream self-root; Σ amp > capacity fires once
// per root. The discriminating proofs live here because the real pwrint board
// cannot host a plain leaf sink on its one copper-split net (VBUS_RAW) without
// tripping the unchanged net-local 6019 — see §8.5 of rail-contract-design.md.
// These locks assert only on 6021. Their far downstream nets (V5B / V33B /
// V3V3B) are *reach-fed* to the root by §7 L4 reach (net-island-attribution-
// design.md; reach.rs) once the net-island L4 batch lands, so 6011/6019 stay
// silent on them — the individual asserts pin 6021 alone.

/// §8.5 copper split, over budget: a 500mA psrc root feeds net A (300mA sink)
/// and, through an `FB` pass leg, a second net B (300mA sink). Net-local each
/// leg is within budget (A: 300 ≤ 500; B: no capacity root → silent), but the
/// root-scope sum is 600 > 500 → one 6021 on the root net. Discriminating:
/// the §8.2 kernel stayed silent on this exact topology.
#[test]
fn root_scope_copper_split_over_budget_fires_once() {
    let src = format!(
        "{SRC_CAP}{SNK_AMP3}{FB}\nmodule main {{\n    conduit GND @role(main)\n    \
         io V33\n    io V33B\n    SRC_CAP s\n    SNK_AMP3 a\n    SNK_AMP3 b\n    \
         s.OUT -> V33\n    s.GND -> GND\n    a.VDD -> V33\n    a.GND -> GND\n    \
         V33 - fb::FB() - V33B\n    b.VDD -> V33B\n    b.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let n_6021 = codes
        .iter()
        .filter(|&&c| c == mcc::errcodes::NET_BUDGET_EXCEEDED)
        .count();
    assert_eq!(
        n_6021, 1,
        "300mA + 300mA across a copper pass leg must sum to one 6021 on the 500mA root net; got codes: {codes:?}"
    );
}

/// §8.5 copper split, within budget: 200mA + 200mA across the same leg stays
/// under the 500mA root → silent (the transparent leg adds no false positive).
#[test]
fn root_scope_copper_split_within_budget_is_silent() {
    let src = format!(
        "{SRC_CAP}{SNK_AMP2}{FB}\nmodule main {{\n    conduit GND @role(main)\n    \
         io V33\n    io V33B\n    SRC_CAP s\n    SNK_AMP2 a\n    SNK_AMP2 b\n    \
         s.OUT -> V33\n    s.GND -> GND\n    a.VDD -> V33\n    a.GND -> GND\n    \
         V33 - fb::FB() - V33B\n    b.VDD -> V33B\n    b.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::NET_BUDGET_EXCEEDED),
        "200mA + 200mA across a copper pass leg stays within the 500mA root — no 6021; got codes: {codes:?}"
    );
}

/// §8.5 source boundary: a capacity-less source (nominal only) is opaque to the
/// ascent. An amp sink reached through a copper leg off it has no budget oracle
/// upstream → silent, exactly as a net-local capacity-less source (§8.2). The
/// walk must NOT fabricate a capacity by stepping past the boundary.
#[test]
fn root_scope_capacity_less_source_boundary_stays_silent() {
    let src = format!(
        "{SRC5}{SNK_AMP5}{FB}\nmodule main {{\n    conduit GND @role(main)\n    \
         io V5\n    io V5B\n    SRC5 s\n    SNK_AMP5 k\n    \
         s.OUT -> V5\n    s.GND -> GND\n    V5 - fb::FB() - V5B\n    k.VDD -> V5B\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::NET_BUDGET_EXCEEDED),
        "an amp sink behind a capacity-less source has no budget oracle — no 6021; got codes: {codes:?}"
    );
}

/// §8.5 no double-count across a self-root: a declared 500mA psrc net D feeds a
/// 300mA-capacity domain-rail net C through copper. D carries its own 300mA
/// load, C carries a 600mA load. Each rail is self-root (a rail face is never
/// re-rooted upstream toward the source that feeds it), so C alone fires
/// (600 > 300) and C's load must NOT also accumulate onto D (D stays at 300 ≤
/// 500 silent). Exactly one 6021 — the double-count guard.
#[test]
fn root_scope_rail_load_does_not_double_count_upstream() {
    let src = format!(
        "{SRC_CAP}{SNK_AMP3}{SNK_AMP_HI}{FB}\nmodule main {{\n    conduit GND @role(main)\n    \
         domain DV {{ rail [V3V3B, GND]::DC(3.3V, capacity:300mA) }}\n    \
         io V3V3B\n    io D33\n    SRC_CAP s\n    SNK_AMP3 c\n    SNK_AMP_HI h\n    \
         s.OUT -> D33\n    s.GND -> GND\n    c.VDD -> D33\n    c.GND -> GND\n    \
         D33 - fb::FB() - V3V3B\n    h.VDD -> V3V3B\n    h.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let n_6021 = codes
        .iter()
        .filter(|&&c| c == mcc::errcodes::NET_BUDGET_EXCEEDED)
        .count();
    assert_eq!(
        n_6021, 1,
        "the 600mA load on the 300mA rail fires once and must not also re-root onto the 500mA source net; got codes: {codes:?}"
    );
}

// PWR-4 budget, derived-demand tail (§8.5, rail-contract-design.md) — the three
// semantics that close the budget axis on top of the root-scope copper walk
// above. All fire 6021 only (zero new codes):
//   * module power-output port (`psrc NAME{hot,ret}::DC(…)` in a module body) as
//     an explicit capacity root — a parent-side load reached through a fuse leg
//     is budgeted against the port's declared capacity;
//   * converter push-up — a Regulator (spec.output + ≥1 Snk + ≥1 Src) draws
//     `I_in = Σ(|V_out|·D(out)) / (|V_in|·eff)`, eff default 1.0 when the Src
//     row omits it, charged on the root governing its input net;
//   * OR-merge single-source mode — a Combine's merge-output demand is charged
//     per distinct governing root of its legs, each leg covering the FULL demand
//     (dedup when two legs share one root); recursion carries demand through
//     nested merges (region_demand, budget_derive.rs).
// The module-port boards load through real files + the mcode library (the
// `::DC` port adopt needs interface DC from ifs/dc.mc and the recursive project
// loader) — the same pipeline as the golden pwrint board. Their ret member and
// the parent hot net are reach-unfed by design (Port faces are budget-local and
// never leak into reach.rs), so 6019/4114 noise is tolerated; the asserts pin
// 6021 alone, exactly like the root-scope locks above.

/// A 5V source declaring an explicit `capacity` — the budget oracle for the
/// converter-push-up / OR-merge fixtures (200mA…1A roots).
const SRC5_CAP2: &str = "component SRC5_CAP_200 {\n    pins = [\n        psrc [1,2] = [OUT, GND]::DC(5V, capacity:200mA)\n    ]\n}\n";
const SRC5_CAP3: &str = "component SRC5_CAP_300 {\n    pins = [\n        psrc [1,2] = [OUT, GND]::DC(5V, capacity:300mA)\n    ]\n}\n";
const SRC5_CAP4: &str = "component SRC5_CAP_400 {\n    pins = [\n        psrc [1,2] = [OUT, GND]::DC(5V, capacity:400mA)\n    ]\n}\n";
const SRC5_CAP5: &str = "component SRC5_CAP_500 {\n    pins = [\n        psrc [1,2] = [OUT, GND]::DC(5V, capacity:500mA)\n    ]\n}\n";
const SRC5_CAP6: &str = "component SRC5_CAP_600 {\n    pins = [\n        psrc [1,2] = [OUT, GND]::DC(5V, capacity:600mA)\n    ]\n}\n";
const SRC5_CAP1A: &str = "component SRC5_CAP_1000 {\n    pins = [\n        psrc [1,2] = [OUT, GND]::DC(5V, capacity:1000mA)\n    ]\n}\n";

/// 5V amp sinks drawn by the OR-merge fixtures' final loads.
const SNK5_500: &str = "component SNK5_500 {\n    pins = [\n        psnk [1,2] = [VDD, GND]::DC(5V, amp:500mA)\n    ]\n}\n";
const SNK5_600: &str = "component SNK5_600 {\n    pins = [\n        psnk [1,2] = [VDD, GND]::DC(5V, amp:600mA)\n    ]\n}\n";

/// Regulator-shaped converter (5V in → 3.3V out) whose Src row omits `eff`
/// (default 1.0) — the eff-default half of the push-up proof.
const CONV_BUDGET: &str = "component CONV_BUDGET {\n    pins = [\n        psnk [1,2] = [VIN, GN1]::DC(5V)\n        psrc [3,4] = [VOUT, GN2]::DC(3.3V)\n    ]\n    spec = [\n        input_req = 4.5V ~ 5.5V\n        output    = 3.2V ~ 3.4V\n    ]\n}\n";

/// The same regulator with `eff:0.9` on the output Src row.
const CONV_BUDGET_EFF: &str = "component CONV_BUDGET_EFF {\n    pins = [\n        psnk [1,2] = [VIN, GN1]::DC(5V)\n        psrc [3,4] = [VOUT, GN2]::DC(3.3V, eff:0.9)\n    ]\n    spec = [\n        input_req = 4.5V ~ 5.5V\n        output    = 3.2V ~ 3.4V\n    ]\n}\n";

/// Load a module-power-port board through the golden pipeline: system library +
/// recursive project load over real temp files (module body `psrc …::DC(…)` rows
/// require interface DC from the mcode library, which the no-lib single-string
/// harness above never loads). `tag` keeps each test's temp directory unique.
fn build_project_codes(tag: &str, psu_mc: &str, main_mc: &str) -> Vec<u32> {
    let _lock = common::lock();
    let dir = std::env::temp_dir().join(format!("mcc-pwr4-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("psu.mc"), psu_mc).unwrap();
    std::fs::write(dir.join("main.mc"), main_mc).unwrap();
    let entry = dir.join("main.mc").canonicalize().unwrap();
    let uri: McURI = entry.to_string_lossy().to_string();
    mcc::mcc_init();
    mcc::mcc_set_project_root(&dir);
    mcc::mcc_load_project(&uri);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    codes.sort_unstable();
    let _ = std::fs::remove_dir_all(&dir);
    codes
}

/// §8.5 module power-output port as an explicit budget root (fire): a 500mA
/// module-body `psrc vin{V33, GND}::DC(3.3V, capacity:500mA)` port feeds, through
/// a fuse leg in the parent, a 600mA sink. The port member is the root; the
/// parent load reaches it over transparent copper + the module-boundary
/// co-segment → one 6021. Discriminator: the §8.2 net-local kernel and the
/// §8.5 copper walk both stayed silent on a module port (no capacity capture).
#[test]
fn module_port_psrc_capacity_root_budgets_parent_load() {
    let psu = "module PSU()\n{\n    psrc vin{V33, GND}::DC(3.3V, capacity:500mA)\n}\n";
    let main = format!(
        "use ./psu.mc\n{FB}{SNK_AMP_HI}\nmodule main {{\n    conduit GND @role(main)\n    \
         PSU psu\n    SNK_AMP_HI k\n    \
         psu.vin -> [fb::FB(), _] -> [V33B, GND]\n    \
         k.VDD -> V33B\n    k.GND -> GND\n}}\n"
    );
    let codes = build_project_codes("modport-cap", psu, &main);
    let n_6021 = codes
        .iter()
        .filter(|&&c| c == mcc::errcodes::NET_BUDGET_EXCEEDED)
        .count();
    assert_eq!(
        n_6021, 1,
        "a 600mA parent load behind a 500mA module-power port must fire 6021 once at the port root; got codes: {codes:?}"
    );
}

/// §8.5 module-port root, within capacity: the same board with a 300mA sink
/// stays under the 500mA port capacity → silent (the port root is read as the
/// governing budget, not ignored).
#[test]
fn module_port_psrc_within_capacity_is_silent() {
    let psu = "module PSU()\n{\n    psrc vin{V33, GND}::DC(3.3V, capacity:500mA)\n}\n";
    let main = format!(
        "use ./psu.mc\n{FB}{SNK_AMP3}\nmodule main {{\n    conduit GND @role(main)\n    \
         PSU psu\n    SNK_AMP3 k\n    \
         psu.vin -> [fb::FB(), _] -> [V33B, GND]\n    \
         k.VDD -> V33B\n    k.GND -> GND\n}}\n"
    );
    let codes = build_project_codes("modport-within", psu, &main);
    assert!(
        !codes.contains(&mcc::errcodes::NET_BUDGET_EXCEEDED),
        "a 300mA parent load inside a 500mA module-power port stays silent — no 6021; got codes: {codes:?}"
    );
}

/// §8.5 module power-output port, no capacity → source boundary: the SAME 600mA
/// parent load behind a capacity-less `psrc vin{…}::DC(3.3V)` export has no
/// budget oracle → silent. PWR-4 does not fabricate a capacity for a port
/// (mirrors the golden VBUS_RAW seam).
#[test]
fn module_port_psrc_nocap_is_source_boundary_silent() {
    let psu = "module PSU()\n{\n    psrc vin{V33, GND}::DC(3.3V)\n}\n";
    let main = format!(
        "use ./psu.mc\n{FB}{SNK_AMP_HI}\nmodule main {{\n    conduit GND @role(main)\n    \
         PSU psu\n    SNK_AMP_HI k\n    \
         psu.vin -> [fb::FB(), _] -> [V33B, GND]\n    \
         k.VDD -> V33B\n    k.GND -> GND\n}}\n"
    );
    let codes = build_project_codes("modport-nocap", psu, &main);
    assert!(
        !codes.contains(&mcc::errcodes::NET_BUDGET_EXCEEDED),
        "a 600mA load behind a capacity-less module-power port has no budget oracle — no 6021; got codes: {codes:?}"
    );
}

/// §8.5 converter push-up over budget: a 300mA 5V root feeds a 5V→3.3V regulator
/// whose 3.3V output net carries a 600mA amp sink. The converter's input is a
/// derived load I_in = (3.3 × 0.6) / (5 × 1.0) = 396mA — over the 300mA root →
/// one 6021 at the input net VIN. The output sink must NOT also fire its own
/// bucket (VOUT is a source boundary, no self-root).
#[test]
fn converter_pushup_over_budget_fires_at_input_root() {
    let src = format!(
        "{SRC5_CAP3}{SNK_AMP_HI}{CONV_BUDGET}\nmodule main {{\n    conduit GND @role(main)\n    \
         io VIN\n    io VOUT\n    SRC5_CAP_300 s\n    CONV_BUDGET c\n    SNK_AMP_HI k\n    \
         s.OUT -> VIN\n    s.GND -> GND\n    \
         c.VIN -> VIN\n    c.GN1 -> GND\n    c.VOUT -> VOUT\n    c.GN2 -> GND\n    \
         k.VDD -> VOUT\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let n_6021 = codes
        .iter()
        .filter(|&&c| c == mcc::errcodes::NET_BUDGET_EXCEEDED)
        .count();
    assert_eq!(
        n_6021, 1,
        "396mA of push-up on a 300mA input root fires 6021 once at VIN (eff default 1.0); got codes: {codes:?}"
    );
}

/// §8.5 converter within budget + the eff-default proof: the same 600mA output
/// on a 400mA input root draws only 396mA with the default eff of 1.0 → silent.
/// Had the missing `eff` defaulted anywhere else (or the converter's input row
/// been counted as a declared load), this boundary would not hold.
#[test]
fn converter_within_budget_is_silent_and_eff_defaults_to_one() {
    let src = format!(
        "{SRC5_CAP4}{SNK_AMP_HI}{CONV_BUDGET}\nmodule main {{\n    conduit GND @role(main)\n    \
         io VIN\n    io VOUT\n    SRC5_CAP_400 s\n    CONV_BUDGET c\n    SNK_AMP_HI k\n    \
         s.OUT -> VIN\n    s.GND -> GND\n    \
         c.VIN -> VIN\n    c.GN1 -> GND\n    c.VOUT -> VOUT\n    c.GN2 -> GND\n    \
         k.VDD -> VOUT\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::NET_BUDGET_EXCEEDED),
        "396mA of push-up at eff 1.0 stays within a 400mA input root — silent; got codes: {codes:?}"
    );
}

/// §8.5 `eff` is read, not hard-coded: the same 400mA root + 600mA output with
/// `eff:0.9` on the converter's Src row draws I_in = 3.3×0.6/(5×0.9) = 440mA —
/// over the 400mA root → fires. Together with the eff-default lock this pins
/// the denominator exactly (eff present vs absent moves the boundary).
#[test]
fn converter_eff_below_one_raises_input_draw_fires() {
    let src = format!(
        "{SRC5_CAP4}{SNK_AMP_HI}{CONV_BUDGET_EFF}\nmodule main {{\n    conduit GND @role(main)\n    \
         io VIN\n    io VOUT\n    SRC5_CAP_400 s\n    CONV_BUDGET_EFF c\n    SNK_AMP_HI k\n    \
         s.OUT -> VIN\n    s.GND -> GND\n    \
         c.VIN -> VIN\n    c.GN1 -> GND\n    c.VOUT -> VOUT\n    c.GN2 -> GND\n    \
         k.VDD -> VOUT\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let n_6021 = codes
        .iter()
        .filter(|&&c| c == mcc::errcodes::NET_BUDGET_EXCEEDED)
        .count();
    assert_eq!(
        n_6021, 1,
        "440mA of push-up at eff 0.9 on a 400mA input root fires 6021 once at VIN; got codes: {codes:?}"
    );
}

/// §8.5 OR-merge single-source mode, weakest leg: two 5V sources (1A and 200mA)
/// feed the two input groups of an OR2 combine whose output rail carries a 500mA
/// amp sink. Each leg is a distinct governing root that single-source mode makes
/// cover the FULL 500mA merge demand → the 200mA leg fires once (net VB); the 1A
/// leg stays silent.
#[test]
fn combine_single_source_fires_on_weakest_leg() {
    let src = format!(
        "{SRC5_CAP1A}{SRC5_CAP2}{SNK5_500}{OR2}\nmodule main {{\n    conduit GND @role(main)\n    \
         io VA\n    io VB\n    io VMAIN\n    SRC5_CAP_1000 a\n    SRC5_CAP_200 b\n    \
         OR2 o\n    SNK5_500 k\n    \
         a.OUT -> VA\n    a.GND -> GND\n    b.OUT -> VB\n    b.GND -> GND\n    \
         o.IN1 -> VA\n    o.G1 -> GND\n    o.IN2 -> VB\n    o.G2 -> GND\n    \
         o.OUT -> VMAIN\n    o.G3 -> GND\n    \
         k.VDD -> VMAIN\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let n_6021 = codes
        .iter()
        .filter(|&&c| c == mcc::errcodes::NET_BUDGET_EXCEEDED)
        .count();
    assert_eq!(
        n_6021, 1,
        "the weakest combine leg (200mA < full 500mA merge demand) must fire once; got codes: {codes:?}"
    );
}

/// §8.5 OR-merge, every leg covers the full demand: both source roots (1A and
/// 600mA) hold ≥ the 500mA merge demand → silent (no false positive from the
/// merge output's own lack of capacity).
#[test]
fn combine_both_legs_cover_is_silent() {
    let src = format!(
        "{SRC5_CAP1A}{SRC5_CAP6}{SNK5_500}{OR2}\nmodule main {{\n    conduit GND @role(main)\n    \
         io VA\n    io VB\n    io VMAIN\n    SRC5_CAP_1000 a\n    SRC5_CAP_600 b\n    \
         OR2 o\n    SNK5_500 k\n    \
         a.OUT -> VA\n    a.GND -> GND\n    b.OUT -> VB\n    b.GND -> GND\n    \
         o.IN1 -> VA\n    o.G1 -> GND\n    o.IN2 -> VB\n    o.G2 -> GND\n    \
         o.OUT -> VMAIN\n    o.G3 -> GND\n    \
         k.VDD -> VMAIN\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::NET_BUDGET_EXCEEDED),
        "both combine legs (1A, 600mA) cover the 500mA merge demand — no 6021; got codes: {codes:?}"
    );
}

/// §8.5 OR-merge shared-root dedupe: ONE 600mA source feeds BOTH combine input
/// nets through two copper legs, so the legs resolve to a single governing root.
/// Single-source mode must charge the 500mA merge demand ONCE on that root (≤
/// 600mA → silent); a naive per-leg double count (1000mA > 600mA) would fire.
#[test]
fn combine_shared_root_dedupes_to_one_charge() {
    let src = format!(
        "{SRC5_CAP6}{SNK5_500}{OR2}{FB}\nmodule main {{\n    conduit GND @role(main)\n    \
         io VS\n    io VA\n    io VB\n    io VMAIN\n    SRC5_CAP_600 s\n    \
         OR2 o\n    SNK5_500 k\n    \
         s.OUT -> VS\n    s.GND -> GND\n    \
         VS - fa::FB() - VA\n    VS - fb::FB() - VB\n    \
         o.IN1 -> VA\n    o.G1 -> GND\n    o.IN2 -> VB\n    o.G2 -> GND\n    \
         o.OUT -> VMAIN\n    o.G3 -> GND\n    \
         k.VDD -> VMAIN\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::NET_BUDGET_EXCEEDED),
        "two legs sharing one 600mA root charge the 500mA merge demand once — silent; got codes: {codes:?}"
    );
}

/// §8.5 nested merges, two recursion hops: a top OR2 merge (1A + 500mA legs) on
/// VMAIN passes through copper into a SECOND OR2 merge's first input net VX; the
/// second merge's output carries the 600mA sink. region_demand(VMAIN) recurses
/// into the second merge (its input hangs on the top merge's output copper), so
/// the top legs must each independently cover the full 600mA → the 500mA top leg
/// fires once (net VB). Discriminator: the recursion that carries a merge demand
/// through a downstream merge back to an upstream weak leg.
#[test]
fn combine_nested_two_hops_fires_on_top_weak_leg() {
    let src = format!(
        "{SRC5_CAP1A}{SRC5_CAP5}{SNK5_600}{OR2}{FB}\nmodule main {{\n    conduit GND @role(main)\n    \
         io VA\n    io VB\n    io VMAIN\n    io VX\n    io VD\n    io VFINAL\n    \
         SRC5_CAP_1000 a\n    SRC5_CAP_500 b\n    SRC5_CAP_1000 d\n    \
         OR2 c1\n    OR2 c2\n    SNK5_600 k\n    \
         a.OUT -> VA\n    a.GND -> GND\n    b.OUT -> VB\n    b.GND -> GND\n    \
         c1.IN1 -> VA\n    c1.G1 -> GND\n    c1.IN2 -> VB\n    c1.G2 -> GND\n    \
         c1.OUT -> VMAIN\n    c1.G3 -> GND\n    \
         VMAIN - fb::FB() - VX\n    \
         c2.IN1 -> VX\n    c2.G1 -> GND\n    d.OUT -> VD\n    d.GND -> GND\n    \
         c2.IN2 -> VD\n    c2.G2 -> GND\n    \
         c2.OUT -> VFINAL\n    c2.G3 -> GND\n    \
         k.VDD -> VFINAL\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let n_6021 = codes
        .iter()
        .filter(|&&c| c == mcc::errcodes::NET_BUDGET_EXCEEDED)
        .count();
    assert_eq!(
        n_6021, 1,
        "600mA pushed through two merges fires the 500mA top leg once (2-hop recursion); got codes: {codes:?}"
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

/// §8.5 v1.2 unified predicate — exemption is PER-LEG (data-gap 2 closed). A
/// bare second leg paralleling an already-declared pair (GNDA ↔ GND declared
/// once by `ba`, a duplicate return ferrite `rr` left bare on its OWN
/// statement) is no longer covered by the pair: the physical carrier must carry
/// its own declaration. Parallel reading — a @bridge elsewhere on the pair does
/// not exempt this leg (the golden FB_agnd/FB_agnd2 pair stays silent because
/// *each* carrier is declared on its own statement, see
/// `two_declared_parallel_legs_on_one_pair_stay_silent_6022`).
#[test]
fn bare_parallel_leg_on_declared_pair_fires_6022() {
    let src = format!(
        "{FB}\nmodule main {{\n    conduit GND  @role(main) @star\n    \
         conduit GNDA @role(quiet)\n    \
         domain AV {{ rail [VDDA, GNDA]::DC(3V3) }}\n    \
         GNDA - ba::FB() - GND @bridge(GNDA, GND)\n    \
         GNDA - rr::FB() - GND\n}}\n"
    );
    let codes = build_codes(&src);
    let n = codes
        .iter()
        .filter(|&&c| c == mcc::errcodes::RETURN_LEG_UNDECLARED)
        .count();
    assert_eq!(
        n, 1,
        "the bare parallel leg on its own statement must fire 6022 (per-leg); got codes: {codes:?}"
    );
}

/// Per-leg span discriminator control (golden FB_agnd/FB_agnd2 shape): two
/// *declared* parallel legs on one pair, each carrier on its own `@bridge`
/// statement, are each self-declared by their own clause span — 6022 is silent
/// even though the net pair is shared.
#[test]
fn two_declared_parallel_legs_on_one_pair_stay_silent_6022() {
    let src = format!(
        "{FB}\nmodule main {{\n    conduit GND  @role(main) @star\n    \
         conduit GNDA @role(quiet)\n    \
         domain AV {{ rail [VDDA, GNDA]::DC(3V3) }}\n    \
         GNDA - p1::FB() - GND @bridge(GNDA, GND)\n    \
         GNDA - p2::FB() - GND @bridge(GNDA, GND)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::RETURN_LEG_UNDECLARED),
        "each declared leg is covered by its own clause span → no 6022; got codes: {codes:?}"
    );
}

/// The golden `FB_agnd` shape: the clause names the return member a **domain**
/// declares, and the copper the leg lands on is the bare `@role(main)` conduit.
/// The clause *is* this leg's declaration, so 6022 stays silent. The pad's net
/// here carries the `DVDD:GND` identity through a module-scope label, so this
/// fixture locks the verdict, not the identity read: the golden board spells the
/// same conductor as a numbered **bus** (`GND` carrying pin 21), whose
/// declaration sits on the bus while its member labels hang off the bus — there
/// the scan has to take the bus entry itself (measured, `pwrint` `FB_agnd` /
/// `FB_agnd2` read 6022 twice when it does not).
#[test]
fn a_declared_leg_on_a_bare_conduit_of_a_declared_member_stays_silent_6022() {
    let src = format!(
        "{FB}{CAP_DECOUP}module main {{\n    conduit GND @role(main) @star\n    \
         conduit GNDA @role(quiet)\n    \
         domain DVDD @class(digital) {{ rail [VDD_3V3, GND]::DC(3.3V) }}\n    \
         domain AVDD @class(analog) {{ rail [VDDA, GNDA]::DC(3.3V) }}\n    \
         GND <- p1::FB() <- GNDA @bridge(GND, GNDA)\n    \
         CAP_DECOUP c\n    c.1 -> VDDA\n    c.2 -> GNDA\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::RETURN_LEG_UNDECLARED),
        "the leg carries its own @bridge clause, so 6022 must stay silent however the pads are named; got codes: {codes:?}"
    );
}

/// The `MIC_SIP` shape: a header port row's member renames the net
/// (`dc.VDD_3V3`), so the clause's written `VDD_3V3` cannot match the pad by
/// spelling — the declared identity is what carries it across. This is the
/// identity key's lock: with the spelling as the only key the pair is lost and
/// the declared leg is judged undeclared (measured, hbl `MIC.FB_vmic`).
#[test]
fn a_declared_leg_on_a_renamed_port_member_stays_silent_6022() {
    let sub = "module SUB_BRIDGE(psnk dc{VDD_3V3, GND}::DC(3.3V)) {\n    \
               domain AVDD @class(analog) { rail [VDDA, VMIC]::DC(3.3V) }\n    \
               dc.VDD_3V3 - fb::FB() - VMIC @bridge(VDD_3V3, VMIC)\n}\n";
    let src = format!(
        "{DC_IFACE}{FB}{sub}module main {{\n    conduit GND @role(main) @star\n    \
         conduit VMIC @role(quiet)\n    \
         domain DVDD @class(digital) {{ rail [VDD_3V3, GND]::DC(3.3V) }}\n    \
         SUB_BRIDGE m1\n    [VDD_3V3, GND] -> m1.dc\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::RETURN_LEG_UNDECLARED),
        "the clause names the port member the leg lands on, so 6022 must stay silent; got codes: {codes:?}"
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

/// §8.5 v1.2 — the hot plane is judged too (data-gap 5). A bare two-terminal leg
/// between the hot faces of two *different* declared rails (VDD_3V3 of DVDD,
/// VDDA of AVDD) is a (1,0) cross-plane DC relation (conduit-equivalence §5: a
/// supply ferrite and a return ferrite are the same bridge, just drawn on the
/// other side of the identity graph) whose classes share no rail world — it
/// must carry its own `@bridge`.
#[test]
fn undeclared_hotplane_supply_bead_fires_6022() {
    let src = format!(
        "{FB}\nmodule main {{\n    conduit GND  @role(main) @star\n    \
         conduit GNDA @role(quiet)\n    \
         domain DV {{ rail [VDD_3V3, GND]::DC(3V3) }}\n    \
         domain AV {{ rail [VDDA, GNDA]::DC(3V3) }}\n    \
         VDD_3V3 - h::FB() - VDDA\n}}\n"
    );
    let codes = build_codes(&src);
    let n = codes
        .iter()
        .filter(|&&c| c == mcc::errcodes::RETURN_LEG_UNDECLARED)
        .count();
    assert_eq!(
        n, 1,
        "a bare hot↔hot supply leg across two rail faces must fire 6022; got codes: {codes:?}"
    );
}

/// Declared control for the hot-plane shape — the same bead carrying its own
/// `@bridge(VDD_3V3, VDDA)` is self-declared (its own clause span) and silent.
#[test]
fn declared_hotplane_supply_bead_stays_silent_6022() {
    let src = format!(
        "{FB}\nmodule main {{\n    conduit GND  @role(main) @star\n    \
         conduit GNDA @role(quiet)\n    \
         domain DV {{ rail [VDD_3V3, GND]::DC(3V3) }}\n    \
         domain AV {{ rail [VDDA, GNDA]::DC(3V3) }}\n    \
         VDD_3V3 - h::FB() - VDDA @bridge(VDD_3V3, VDDA)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::RETURN_LEG_UNDECLARED),
        "the declared supply bead is covered by its own clause → no 6022; got codes: {codes:?}"
    );
}

/// §8.5 v1.2 identity arm ② — a rail ret *member net name* is a class even when
/// no same-name conduit exists (attribution keeps role Ret, copper None). A bare
/// leg from such a member (RTN, DVDD's declared return face) to a different
/// rail's return (GNDA of AVDD) is cross-plane and fires; the declared
/// GNDA↔GND leg is silent.
#[test]
fn ret_member_without_conduit_still_judged_6022() {
    let src = format!(
        "{FB}\nmodule main {{\n    conduit GND  @role(main) @star\n    \
         conduit GNDA @role(quiet)\n    \
         domain DV {{ rail [VDD_3V3, RTN]::DC(3V3) }}\n    \
         domain AV {{ rail [VDDA, GNDA]::DC(3V3) }}\n    \
         GNDA - g::FB() - GND @bridge(GNDA, GND)\n    \
         GNDA - q::FB() - RTN\n}}\n"
    );
    let codes = build_codes(&src);
    let n = codes
        .iter()
        .filter(|&&c| c == mcc::errcodes::RETURN_LEG_UNDECLARED)
        .count();
    assert_eq!(
        n, 1,
        "a bare leg off a rail-ret member with no conduit must fire 6022; got codes: {codes:?}"
    );
}

/// §8.5 v1.2 identity arm ③ — the A′ boundary read (golden C_y shape). A
/// two-terminal leg inside a child module between a local conduit copper
/// (ESDGND) and the child's own out-port member (P) — where P's net is Signal
/// in the child scope but co-resides across the module-boundary junction with
/// the parent's `EARTH` conduit — resolves P to the parent class through the
/// junction and fires: the child "forgot" the declaration even though it only
/// ever sees its own net names (the far copper identity comes from the parent
/// binding, exactly conduit-equivalence §8.5's boundary model).
#[test]
fn boundary_out_member_reaching_parent_copper_fires_6022() {
    let src = format!(
        "{FB}\nmodule CHILD() {{\n    conduit ESDGND\n    out P\n    \
         ESDGND - cy::FB() - P\n}}\nmodule main {{\n    \
         conduit EARTH @role(earth)\n    CHILD u\n    u.P -> EARTH\n}}\n"
    );
    let codes = build_codes(&src);
    let n = codes
        .iter()
        .filter(|&&c| c == mcc::errcodes::RETURN_LEG_UNDECLARED)
        .count();
    assert_eq!(
        n, 1,
        "a bare leg inside a child between a local copper and a parent-bound out member must fire 6022; got codes: {codes:?}"
    );
}

/// Declared control for the boundary shape — the child leg carries its own
/// `@couple(ESDGND, P)` (endpoint names are the child-scope net names; the far
/// EARTH identity rides the port, not the clause) → self-declared and silent.
/// This is exactly the golden fix in `mcs/pwrint/src/power-usb.mc` (C_y).
#[test]
fn boundary_out_member_leg_with_own_couple_stays_silent_6022() {
    let src = format!(
        "{FB}\nmodule CHILD() {{\n    conduit ESDGND\n    out P\n    \
         ESDGND - cy::FB() - P @couple(ESDGND, P)\n}}\nmodule main {{\n    \
         conduit EARTH @role(earth)\n    CHILD u\n    u.P -> EARTH\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::RETURN_LEG_UNDECLARED),
        "the child leg with its own @couple is covered → no 6022; got codes: {codes:?}"
    );
}

/// §8.5 v1.2 identity arm ③ multi-hop — the A′ boundary walk crosses two
/// module boundaries (GRAND → CHILD → main) before it reaches the anchoring
/// copper. A bare leg inside GRAND between its local conduit H and its own out
/// member (whose net is Signal at every interior scope and only becomes the
/// parent `EARTH` copper two junctions out) still fires; the recursion is
/// depth-generic, not one-hop.
#[test]
fn boundary_deep_multihop_leg_fires_6022() {
    let src = format!(
        "{FB}\nmodule GRAND() {{\n    conduit H\n    out P2\n    \
         H - cy::FB() - P2\n}}\nmodule CHILD() {{\n    GRAND g\n    out P\n    \
         g.P2 -> P\n}}\nmodule main {{\n    \
         conduit EARTH @role(earth)\n    CHILD u\n    u.P -> EARTH\n}}\n"
    );
    let codes = build_codes(&src);
    let n = codes
        .iter()
        .filter(|&&c| c == mcc::errcodes::RETURN_LEG_UNDECLARED)
        .count();
    assert_eq!(
        n, 1,
        "a bare leg reaching the anchor copper across two module boundaries must fire 6022; got codes: {codes:?}"
    );
}

// §8.6 device reference-pin cross-plane (6027, conduit-equivalence-design.md
// §8.6) — the ≥3-pin functional sibling of 6022. A device whose DC-pair
// *return* pins resolve to two disjoint potential classes silently DC-joins
// them through the die/substrate unless ① a net-level @bridge/@couple covers
// the class pair (uc/GND↔GNDA) or ② a declared isolation structure does — one
// return class is an @role(isolated) copper carried by a source-side contract
// AND another return class is sink-side (iso5/DC.ISO_SRC; both conditions).

/// A two-sink digital+analog device (uc-like): two `psnk` rows, two *distinct*
/// return members G1/G2. Whether 6027 fires depends entirely on the board — on
/// which two coppers the returns land and whether a declaration covers the span.
const DEV2R: &str = "component DEV2R {\n    pins = [\n        psnk [1,2] = [VDD, G1]::DC(3.3V)\n        psnk [3,4] = [AVDD, G2]::DC(3.3V)\n    ]\n}\n";

/// An isolated power source (iso5-like): one sink row on the primary side, one
/// source row whose return is the isolated side's reference.
const ISODC: &str = "component ISODC {\n    pins = [\n        psnk [1,2] = [PRI, G]::DC(5V)\n        psrc [3,4] = [SEC, GI]::DC(5V)\n    ]\n}\n";

/// A sink-only device whose *both* returns are sink-fed — one on an isolated
/// copper, one on a main/quiet copper (a load straddling an isolation boundary).
const ISOAMP: &str = "component ISOAMP {\n    pins = [\n        psnk [1,2] = [VDD, GI]::DC(3.3V)\n        psnk [3,4] = [AVDD, G]::DC(3.3V)\n    ]\n}\n";

/// §8.6 fire — the real defect: a uc-like digital+analog part (both returns
/// sink-side) whose AGND/GND return pins land on two disjoint quiet coppers with
/// no net-level bridge. The die silently bridges the analog and digital ground
/// planes — the exact AGND/PGND straddle 6027 exists to catch.
#[test]
fn device_sink_returns_across_two_quiet_planes_fire_6027() {
    let src = format!(
        "{DEV2R}\nmodule main {{\n    conduit GNDA @role(quiet)\n    \
         conduit GNDB @role(quiet)\n    \
         domain AD {{ rail [VA, GNDA]::DC(3.3V) }}\n    \
         domain BD {{ rail [VB, GNDB]::DC(3.3V) }}\n    \
         DEV2R s\n    s.VDD -> VA\n    s.G1 -> GNDA\n    \
         s.AVDD -> VB\n    s.G2 -> GNDB\n}}\n"
    );
    let codes = build_codes(&src);
    let n = codes
        .iter()
        .filter(|&&c| c == mcc::errcodes::DEVICE_RETURN_SPAN_UNDECLARED)
        .count();
    assert_eq!(
        n, 1,
        "two sink-side returns on two disjoint unbridged quiet planes must fire 6027; got codes: {codes:?}"
    );
}

/// §8.6 exemption ① — the uc/GND↔GNDA control: the *same* straddle is covered
/// by a net-level `@bridge(GNDA, GNDB)`, so the device's return span is declared
/// at the net layer and 6027 (and 6022 on the declared ferrite leg) are silent.
#[test]
fn device_sink_return_span_covered_by_net_bridge_stays_silent_6027() {
    let src = format!(
        "{DEV2R}{FB}\nmodule main {{\n    conduit GNDA @role(quiet)\n    \
         conduit GNDB @role(quiet)\n    \
         domain AD {{ rail [VA, GNDA]::DC(3.3V) }}\n    \
         domain BD {{ rail [VB, GNDB]::DC(3.3V) }}\n    \
         DEV2R s\n    s.VDD -> VA\n    s.G1 -> GNDA\n    \
         s.AVDD -> VB\n    s.G2 -> GNDB\n    \
         GNDA - fb::FB() - GNDB @bridge(GNDA, GNDB)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::DEVICE_RETURN_SPAN_UNDECLARED),
        "a net-level @bridge on the class pair exempts the device return span; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::RETURN_LEG_UNDECLARED),
        "the declared ferrite leg itself stays covered on 6022; got codes: {codes:?}"
    );
}

/// Control (not a misfire): the *normal* multi-supply shape — two supply rails
/// that genuinely share one return net (G1/G2 both land on main GND). A single
/// return class spans nothing, so a multi-row part is not a candidate.
#[test]
fn multi_supply_single_return_stays_silent_6027() {
    let src = format!(
        "{DEV2R}\nmodule main {{\n    conduit GND @role(main)\n    \
         domain AD {{ rail [VA, GND]::DC(3.3V) }}\n    \
         domain BD {{ rail [VB, GND]::DC(3.3V) }}\n    \
         DEV2R s\n    s.VDD -> VA\n    s.G1 -> GND\n    \
         s.AVDD -> VB\n    s.G2 -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::DEVICE_RETURN_SPAN_UNDECLARED),
        "two supply rails sharing one return net span nothing → no 6027; got codes: {codes:?}"
    );
}

/// §8.6 exemption ② — the iso5/DC.ISO_SRC control: the primary return is
/// sink-side on main GND, the secondary return is source-side (`psrc`) on the
/// `@role(isolated)` copper. The device is the isolator that defines the
/// isolated world (both conditions), so the return span is the isolation — 6027
/// stays silent exactly as golden iso5 must.
#[test]
fn isolated_source_return_span_stays_silent_6027() {
    let src = format!(
        "{ISODC}\nmodule main {{\n    conduit GND  @role(main)\n    \
         conduit GISO @role(isolated)\n    \
         domain DV {{ rail [VMAIN, GND]::DC(5V) }}\n    \
         domain ISO {{ rail [VSEC, GISO]::DC(5V) }}\n    \
         ISODC d\n    d.PRI -> VMAIN\n    d.G -> GND\n    \
         d.SEC -> VSEC\n    d.GI -> GISO\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::DEVICE_RETURN_SPAN_UNDECLARED),
        "isolated source-side return + sink-side primary return is the declared isolation → no 6027; got codes: {codes:?}"
    );
}

/// §8.6 "both conditions" — isolation ALONE is not enough. A sink-only device
/// (both `psnk`) returning on an `@role(isolated)` copper AND on a main/quiet
/// copper is the hidden DC bridge into the isolated world the role forbids: its
/// isolated return is sink-fed, not source-fed, so no isolation structure covers
/// the span → 6027 fires.
#[test]
fn sink_only_isolated_return_straddle_fires_6027() {
    let src = format!(
        "{ISOAMP}\nmodule main {{\n    conduit GNDA @role(quiet)\n    \
         conduit GISO @role(isolated)\n    \
         domain AD {{ rail [VA, GNDA]::DC(3.3V) }}\n    \
         domain ISO {{ rail [VIS, GISO]::DC(3.3V) }}\n    \
         ISOAMP a\n    a.VDD -> VIS\n    a.GI -> GISO\n    \
         a.AVDD -> VA\n    a.G -> GNDA\n}}\n"
    );
    let codes = build_codes(&src);
    let n = codes
        .iter()
        .filter(|&&c| c == mcc::errcodes::DEVICE_RETURN_SPAN_UNDECLARED)
        .count();
    assert_eq!(
        n, 1,
        "an isolated copper alone does not exempt a sink-only span (both conditions required) → 6027 ×1; got codes: {codes:?}"
    );
}

// Window batch (rail-contract-design.md §6.1/§6.3) — S(net) as a closed
// interval, judged by spec + structure, never by name. A2/A3 landed the shared
// `WindowDeriv::window_of_net` engine; these fixtures exercise its two first
// consumers (6023 regulator gate, 6024 sink req window).

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

// 6025 partial-spec advisory

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

// Window-notes batch (rail-contract-design.md §6.6/§6.7) — module-boundary S
// feed forwarding + the converter-output-vs-rail-window cross-check (6026).

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

// Net-island L4 supply reach (island-attribution-design.md §7 L4) — 6011/
// 6019 close the "copper pass-through feed = later S-set step" gap through
// reach.rs. A root-less sink net *fed* to an upstream supply root through a
// current-transparent two-pin element (fuse/inductor/ferrite/decoupling cap)
// or a module boundary is not a PWR-1 orphan (6019 silent) and is adjudicated
// by 6011 against the reached root's nominal. Return/reference copper is never
// a feed (a cap to GND does not supply), so 6019 still fires on genuinely
// undriven nets even when they have a decoupling cap to the shared return.
// These are the discriminating proofs — the real pwrint board cannot demo the
// copper arm without tripping 4118 (see the §9 landing log).

/// §7 L4 copper arm via a *psrc* root: a 5V sink on net V5B, fed from the 5V
/// psrc net V5 through an `FB` leg. Old net-local layer: V5B is rootless with a
/// sink → 6019 fires. Reach: V5B is fed (5V) → 6019 silent and 6011 adjudicates
/// a match → silent.
#[test]
fn copper_psrc_fed_sink_net_silent_6019_l4_reach() {
    let src = format!(
        "{SRC5}{SNK5}{FB}\nmodule main {{\n    conduit GND @role(main)\n    \
         io V5\n    io V5B\n    SRC5 s\n    SNK5 k\n    \
         s.OUT -> V5\n    s.GND -> GND\n    \
         V5 - fb::FB() - V5B\n    k.VDD -> V5B\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::SINK_NET_NO_SOURCE)
            && !codes.contains(&mcc::errcodes::POWER_SINK_NOMINAL_MISMATCH),
        "a 5V sink fed from a 5V psrc through copper must be silent on 6019 and 6011 (§7 L4 reach); got codes: {codes:?}"
    );
}

/// §7 L4 copper arm via a *domain-rail face*: a 3.3V sink on net V3V3B, fed from
/// the declared 3.3V rail through an `FB` leg → both 6019 and 6011 silent.
#[test]
fn copper_rail_fed_match_silent_6019_and_6011_l4_reach() {
    let src = format!(
        "{SINK3}{FB}\nmodule main {{\n    conduit GND @role(main)\n    \
         domain DVDD @class(digital) {{ rail [V3V3, GND]::DC(3.3V) }}\n    \
         io V3V3\n    io V3V3B\n    SINK3 k\n    \
         V3V3 - fb::FB() - V3V3B\n    k.VDD -> V3V3B\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::SINK_NET_NO_SOURCE)
            && !codes.contains(&mcc::errcodes::POWER_SINK_NOMINAL_MISMATCH),
        "a 3.3V sink fed from its 3.3V rail through copper must be silent on 6019 and 6011 (§7 L4 reach); got codes: {codes:?}"
    );
}

/// §7 L4 flagship discriminator: the *mismatch* on a copper-fed net is now
/// adjudicated. A 5V sink on V3V3B (fed from the 3.3V rail) fired 6019 under the
/// net-local layer and *skipped* 6011 (no root on V3V3B). Reach closes both: the
/// net is fed, so 6019 stays silent, and 6011 compares against the reached 3.3V
/// root → exactly one mismatch.
#[test]
fn copper_rail_fed_mismatch_fires_6011_l4_reach() {
    let src = format!(
        "{SNK5}{FB}\nmodule main {{\n    conduit GND @role(main)\n    \
         domain DVDD @class(digital) {{ rail [V3V3, GND]::DC(3.3V) }}\n    \
         io V3V3\n    io V3V3B\n    SNK5 k\n    \
         V3V3 - fb::FB() - V3V3B\n    k.VDD -> V3V3B\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let n_6011 = codes
        .iter()
        .filter(|&&c| c == mcc::errcodes::POWER_SINK_NOMINAL_MISMATCH)
        .count();
    assert_eq!(
        n_6011, 1,
        "a 5V sink reached to a 3.3V rail through copper must fire exactly one 6011 (§7 L4 reach); got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::SINK_NET_NO_SOURCE),
        "the fed net must not also fire 6019; got codes: {codes:?}"
    );
}

/// §7 L4 hop-exclusion guard — the decoupling-cap hazard. A genuinely undriven
/// sink net VPROBE has a decap to the shared return GND, and the rail V3V3 has
/// its *own* decap to that same GND (the normal golden shape). A naive engine
/// climbs VPROBE → cap → GND → rail cap → V3V3 and false-silences an orphan;
/// reach excludes the `Ret` hop at GND, so 6019 still fires on VPROBE.
#[test]
fn decap_to_return_net_is_not_a_feed_l4_reach() {
    let src = format!(
        "{SINK3}{FB}\nmodule main {{\n    conduit GND @role(main)\n    \
         domain DVDD @class(digital) {{ rail [V3V3, GND]::DC(3.3V) }}\n    \
         io V3V3\n    io VPROBE\n    SINK3 k\n    \
         k.VDD -> VPROBE\n    k.GND -> GND\n    \
         VPROBE - ca::FB() - GND\n    \
         V3V3 - cb::FB() - GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::SINK_NET_NO_SOURCE),
        "a decap to the shared return net must not feed an undriven net — 6019 fires (§7 L4 hop exclusion); got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::POWER_SINK_NOMINAL_MISMATCH),
        "the undriven net carries no agreed nominal, so 6011 must stay silent; got codes: {codes:?}"
    );
}

/// §7 L4 seed-exclusion guard: a sink hot wired straight onto the declared
/// return copper GND, with the rail's decap (V3V3 – cap – GND) present. Without
/// seed exclusion a naive engine feeds GND through the rail cap and drops the
/// miswire; reach never *starts* on return/reference copper → 6019 fires.
#[test]
fn return_net_sink_not_fed_by_decap_bead_l4_reach() {
    let src = format!(
        "{SINK3}{FB}\nmodule main {{\n    conduit GND @role(main)\n    \
         domain DVDD @class(digital) {{ rail [V3V3, GND]::DC(3.3V) }}\n    \
         io V3V3\n    io RTN\n    SINK3 k\n    \
         k.VDD -> GND\n    k.GND -> RTN\n    \
         V3V3 - ca::FB() - GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::SINK_NET_NO_SOURCE),
        "a sink hot on the return copper is a miswire — 6019 fires even with the rail decap present (§7 L4 seed exclusion); got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::POWER_SINK_NOMINAL_MISMATCH),
        "return copper never carries an agreed nominal, so 6011 must stay silent; got codes: {codes:?}"
    );
}

// §8.7 port role contract (6029, conduit-equivalence-design.md §8.7) — a module
// `out` port carrying @bind_role(<role>) demands its parent binding land on a
// reference of that role. The child names only a role, never an ancestor
// conduit, so the parent binding is the witness. Judged in the binding layer:
// the target must resolve to a conduit of the declared @role (a bare conduit
// defaults to main), or to the layer's own out port re-declaring the same
// @bind_role (layer-by-layer forwarding). A different role, or a target with no
// role identity, is an Error.

/// A child exporting one role-contract `out` port — the golden POWER_USB shape
/// (`shield_to_earth @bind_role(earth)`) reduced to the contract alone.
const BIND_CHILD: &str = "module CHILD() {\n    out P @bind_role(earth)\n}\n";

/// §8.7 silent control: the parent binds the earth-contract port to the EARTH
/// conduit — the contract is witnessed (golden `usb.shield_to_earth -> EARTH`).
#[test]
fn port_bind_role_earth_to_earth_stays_silent_6029() {
    let src = format!(
        "{BIND_CHILD}\nmodule main {{\n    conduit EARTH @role(earth)\n    \
         CHILD u\n    u.P -> EARTH\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::PORT_BIND_ROLE_MISMATCH),
        "binding the earth-contract port to the EARTH conduit witnesses it → no 6029; got codes: {codes:?}"
    );
}

/// §8.7 fire — the wrong role: the earth-contract port is bound to a
/// `@role(main)` conduit, so the parent binding contradicts the declared
/// contract. The message names both roles.
#[test]
fn port_bind_role_wrong_role_binding_fires_6029() {
    let src = format!(
        "{BIND_CHILD}\nmodule main {{\n    conduit GND @role(main)\n    \
         CHILD u\n    u.P -> GND\n}}\n"
    );
    let ms = msgs_of(mcc::errcodes::PORT_BIND_ROLE_MISMATCH, &src);
    assert_eq!(
        ms.len(),
        1,
        "an earth-contract port bound to a main-role conduit must fire 6029 once; got: {ms:?}"
    );
    assert!(
        ms[0].contains("@bind_role(earth)"),
        "6029 must name the declared role; got: {ms:?}"
    );
    assert!(
        ms[0].contains("role main"),
        "6029 must name the resolved target role; got: {ms:?}"
    );
}

/// §8.7 fire — a target with no role identity: the port is bound to a plain net
/// (no conduit, no role), so the contract is not witnessed.
#[test]
fn port_bind_role_no_role_target_fires_6029() {
    let src = format!("{BIND_CHILD}\nmodule main {{\n    CHILD u\n    u.P -> SHIELD\n}}\n");
    let ms = msgs_of(mcc::errcodes::PORT_BIND_ROLE_MISMATCH, &src);
    assert_eq!(
        ms.len(),
        1,
        "an earth-contract port bound to a role-less net must fire 6029 once; got: {ms:?}"
    );
    assert!(
        ms[0].contains("role none"),
        "6029 must report the missing role; got: {ms:?}"
    );
}

/// §8.7 silent control — the layer-by-layer forwarding that makes nesting work:
/// the intermediate module re-exports the same `@bind_role` through its own out
/// port, and only the top layer binds that port to the EARTH conduit. No layer
/// ever names an ancestor conduit.
#[test]
fn port_bind_role_forwarded_through_layer_stays_silent_6029() {
    let src = "module LEAF() {\n    out P @bind_role(earth)\n}\n\
        module MID() {\n    LEAF l\n    out P2 @bind_role(earth)\n    l.P -> P2\n}\n\
        module main {\n    conduit EARTH @role(earth)\n    MID m\n    m.P2 -> EARTH\n}\n";
    let codes = build_codes(src);
    assert!(
        !codes.contains(&mcc::errcodes::PORT_BIND_ROLE_MISMATCH),
        "forwarding the contract through a layer's own out port witnesses it → no 6029; got codes: {codes:?}"
    );
}

/// §8.7 default — a bare `conduit CHASSIS` (no `@role`) defaults to `main`
/// (§5.2), so an `@bind_role(main)` port bound to it is witnessed and silent.
#[test]
fn port_bind_role_bare_conduit_defaults_to_main_stays_silent_6029() {
    let src = "module CHILD() {\n    out P @bind_role(main)\n}\n\
        module main {\n    conduit CHASSIS\n    CHILD u\n    u.P -> CHASSIS\n}\n";
    let codes = build_codes(src);
    assert!(
        !codes.contains(&mcc::errcodes::PORT_BIND_ROLE_MISMATCH),
        "a bare conduit defaults to main, witnessing an @bind_role(main) port; got codes: {codes:?}"
    );
}

/// Model A §4.1 `[hot, ret]` pairing, the gap side: a `::DC` power row that
/// names no return member declares an incomplete crossing. The check reads
/// `McPwrPin.ret`, so both spellings of the gap land on one code — the bare
/// lone name (`psnk 5 = VCC::DC(3.3V)`), which used to vanish before any pin
/// existed, and the single-member pair (`psnk [5] = [VCC]::DC(3.3V)`).
#[test]
fn dc_row_without_return_member_fires_6030() {
    let src = "component LONE {\n    pins = [\n        psnk 5 = VCC::DC(3.3V)\n        6 = GND\n    ]\n}\n\
        component BRACKET {\n    pins = [\n        psnk [5] = [VCC]::DC(3.3V)\n    ]\n}\n\
        module main {\n    LONE U1\n    BRACKET U2\n}\n";
    let codes = build_codes(src);
    let n = codes
        .iter()
        .filter(|c| **c == mcc::errcodes::POWER_PIN_RETURN_MISSING)
        .count();
    assert_eq!(
        n, 2,
        "one 6030 per return-less ::DC row; got codes: {codes:?}"
    );
}

/// The completed crossing — the pair `[hot, ret]` — is clean, and a bare row
/// carrying no `::DC` (a passive leaf's `N = GND`, whose direction belongs to
/// the parent module port) is not a crossing at all: it must not be dragged in.
#[test]
fn dc_pair_and_non_dc_rows_stay_silent_6030() {
    let src =
        "component PAIRED {\n    pins = [\n        psnk [5,6] = [VCC, GND]::DC(3.3V)\n    ]\n}\n\
        component PASSIVE {\n    pins = [\n        6 = GND\n    ]\n}\n\
        module main {\n    PAIRED U1\n    PASSIVE U2\n}\n";
    let codes = build_codes(src);
    assert!(
        !codes.contains(&mcc::errcodes::POWER_PIN_RETURN_MISSING),
        "a completed pair and a non-DC row are both outside the check; got codes: {codes:?}"
    );
}

// ── PWR-6 exposed-net clamp coverage (exposed-protection-design.md §3, v0.2) ──
//
// `@exposed(<threat>)` puts a port's net at the board's transient boundary, and
// a boundary net must carry a declared clamp: a device on that net whose dump
// leg lands on a reference the same scope declares `@clamp` on. Coverage is the
// declaration, not the topology alone — a device tying the exposed net to the
// protective island without a `@clamp` is not a clamp. The reference's role is
// not part of coverage: PWR-6 is the existence half, upstream of 6008 (PWR-7),
// so a clamp onto a main/quiet reference fires 6008 alone.

/// The gold shape (POWER_USB): a TVS-like device on the exposed net dumps onto
/// the module's own `@role(protective)` ref through a declared `@clamp` → PWR-6
/// is silent.
#[test]
fn exposed_net_with_declared_clamp_is_silent_6031() {
    let src = format!(
        "{TV}\nmodule main {{\n    conduit ESDGND @role(protective)\n    io DP @exposed(esd_contact)\n    \
         TV tv\n    tv.IO -> DP\n    tv.G -> ESDGND @clamp(ESDGND)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::EXPOSED_NET_NO_CLAMP),
        "a declared clamp onto the protective island covers the exposed net; got codes: {codes:?}"
    );
}

/// Nothing dumps the exposed nets: both ports of the row fire (the row's two
/// operands are two exposed ports, each judged on its own net).
#[test]
fn exposed_nets_without_clamp_each_fire_6031() {
    let src = format!(
        "{TV}\nmodule main {{\n    conduit ESDGND @role(protective)\n    io DP, DM @exposed(esd_contact)\n    \
         TV tva\n    TV tvb\n    tva.IO -> DP\n    tvb.IO -> DM\n    \
         tva.G -> ESDGND\n    tvb.G -> ESDGND\n}}\n"
    );
    let codes = build_codes(&src);
    let n = codes
        .iter()
        .filter(|c| **c == mcc::errcodes::EXPOSED_NET_NO_CLAMP)
        .count();
    assert_eq!(
        n, 2,
        "one 6031 per uncovered exposed port; got codes: {codes:?}"
    );
}

/// The ruled distinction: the device *is* on the exposed net and *is* tied to
/// the protective island, but no `@clamp` is declared for that reference — the
/// ordinary PI-axis connection is not a clamp, so the exposure stays uncovered.
#[test]
fn device_onto_the_ref_without_a_clamp_declaration_fires_6031() {
    let src = format!(
        "{TV}\nmodule main {{\n    conduit ESDGND @role(protective)\n    io DP @exposed(esd_contact)\n    \
         TV tv\n    tv.IO -> DP\n    tv.G -> ESDGND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::EXPOSED_NET_NO_CLAMP),
        "coverage is the @clamp declaration, not the topology alone; got codes: {codes:?}"
    );
}

/// No double report: a clamp declared onto a `main`-role reference is *present*
/// (PWR-6 silent) and *wrong* (6008 fires). The defect is the reference, so it
/// belongs to PWR-7 alone.
#[test]
fn clamp_onto_a_main_ref_is_pwr7_only_6031() {
    let src = format!(
        "{TV}\nmodule main {{\n    conduit GND @role(main)\n    io DP @exposed(esd_contact)\n    \
         TV tv\n    tv.IO -> DP\n    tv.G -> GND @clamp(GND)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::CLAMP_REF_NOT_PROTECTIVE),
        "a clamp onto a @role(main) ref is PWR-7's verdict; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::EXPOSED_NET_NO_CLAMP),
        "the clamp is present — PWR-6 must not stack on PWR-7; got codes: {codes:?}"
    );
}

/// An `@exposed` port with no net of its own (nothing in the module body touches
/// it) has no segment to judge: not adjudicated rather than guessed.
#[test]
fn exposed_port_with_no_net_is_not_adjudicated_6031() {
    let src = format!(
        "{TV}\nmodule main {{\n    conduit ESDGND @role(protective)\n    io DP @exposed(esd_contact)\n    \
         TV tv\n    tv.IO -> tv.G\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::EXPOSED_NET_NO_CLAMP),
        "a dangling exposed port has no segment; got codes: {codes:?}"
    );
}

// ── PWR-6 downstream chain (exposed-protection-design.md §3.1, six rulings 2026-09-17) ──
//
// 6031 asks the *existence* question on the exposed net itself. This half asks
// the *direction* question the canon's "already past a clamp or current-limit
// chain before entering an intolerant domain" names: from the exposed port's
// own copper, flood the current-transparent
// region (transparent copper + module-boundary co-segments, never across
// Ret/Reference copper) **stopping at every declared gate** (`protect = series`
// — the current-limit chain), and report when a net of that region — the port's
// own copper excluded — is a quiet/sensitive face (§1.4) carrying no clamp of
// its own. Region *existence*, not path search: a clamp anywhere covers.
//
// The precondition that keeps the two halves from stacking: the port's own
// copper must already be covered (`6031` silent), else the uncovered case is
// that rule's verdict alone.

/// The board every case below flips one axis of: a protective island, a quiet
/// face (`@class(analog)`, so the rail's hot net resolves a world the scope
/// declares quiet), and its own reference.
const DOWNSTREAM_QUIET_BOARD: &str = "conduit ESDGND @role(protective)\n    \
                                      conduit GNDA @role(quiet)\n    \
                                      domain AVDD @class(analog) { rail [VDDA, GNDA]::DC(3.3V) }\n    ";

/// The noisy twin: same structure, `@noise(noisy)`, so the face read answers
/// Noisy and the downstream net is not an untolerated domain.
const DOWNSTREAM_NOISY_BOARD: &str = "conduit ESDGND @role(protective)\n    \
                                      conduit GND @role(main)\n    \
                                      domain DVDD @class(digital) @noise(noisy) { rail [VDD_3V3, GND]::DC(3.3V) }\n    ";

/// The ruled defect: the exposed port *is* clamped (6031 silent), but an
/// ordinary two-terminal pass carries the same copper on to an unclamped quiet
/// face — a clamp covers its own side of every branch.
#[test]
fn exposed_branch_reaching_an_unclamped_quiet_face_fires_6044() {
    let src = format!(
        "{TV}{PROT_TWOPIN}module main {{\n    {DOWNSTREAM_QUIET_BOARD}\
         io DP @exposed(esd_contact)\n    \
         TV tv\n    tv.IO -> DP\n    tv.G -> ESDGND @clamp(ESDGND)\n    \
         TWOPIN.PLAIN r\n    r.A -> DP\n    r.B -> VDDA\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::EXPOSED_NET_DOWNSTREAM_UNPROTECTED, &src);
    assert_eq!(
        msgs.len(),
        1,
        "the branch reaching the unclamped quiet face must fire 6044 exactly once; got codes: {codes:?}"
    );
    assert!(
        msgs[0].contains("main.DP") && msgs[0].contains("VDDA"),
        "6044 must name the exposed port and the uncovered downstream net: {msgs:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::EXPOSED_NET_NO_CLAMP),
        "the clamp on DP covers it — 6031 stays silent; got codes: {codes:?}"
    );
}

/// The control for "the face read is the §1.4 read, not the topology": the same
/// shape into a `@noise(noisy)` face is a tolerated domain and stays silent.
#[test]
fn exposed_branch_reaching_a_noisy_face_is_silent_6044() {
    let src = format!(
        "{TV}{PROT_TWOPIN}module main {{\n    {DOWNSTREAM_NOISY_BOARD}\
         io DP @exposed(esd_contact)\n    \
         TV tv\n    tv.IO -> DP\n    tv.G -> ESDGND @clamp(ESDGND)\n    \
         TWOPIN.PLAIN r\n    r.A -> DP\n    r.B -> VDD_3V3\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::EXPOSED_NET_DOWNSTREAM_UNPROTECTED),
        "a noisy face is not an untolerated domain; got codes: {codes:?}"
    );
}

/// A declared gate stops the flood: the same two-terminal shape, but the class
/// declares `protect = series` (fuse / PTC / ferrite — the current-limit chain
/// the canon names), so the transient never reaches the quiet face unclamped.
/// The declaration is the only witness — the unmarked twin fires (above).
#[test]
fn a_declared_series_gate_stops_the_flood_6044() {
    let src = format!(
        "{TV}{PROT_FUSE}module main {{\n    {DOWNSTREAM_QUIET_BOARD}\
         io DP @exposed(esd_contact)\n    \
         TV tv\n    tv.IO -> DP\n    tv.G -> ESDGND @clamp(ESDGND)\n    \
         FUSE.PROT f\n    f.A -> DP\n    f.B -> VDDA\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::EXPOSED_NET_DOWNSTREAM_UNPROTECTED),
        "a declared series gate is the current-limit chain — nothing fires past it; got codes: {codes:?}"
    );
}

/// A region net clamped on its own account is covered: the quiet rail carries
/// its own clamp, so the branch that reaches it is protected at the far end.
#[test]
fn a_clamped_quiet_face_downstream_is_silent_6044() {
    let src = format!(
        "{TV}{PROT_TWOPIN}module main {{\n    {DOWNSTREAM_QUIET_BOARD}\
         io DP @exposed(esd_contact)\n    \
         TV tv\n    tv.IO -> DP\n    tv.G -> ESDGND @clamp(ESDGND)\n    \
         TWOPIN.PLAIN r\n    r.A -> DP\n    r.B -> VDDA\n    \
         TV tv2\n    tv2.IO -> VDDA\n    tv2.G -> ESDGND @clamp(ESDGND)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::EXPOSED_NET_DOWNSTREAM_UNPROTECTED),
        "the downstream net is clamped on its own account; got codes: {codes:?}"
    );
}

/// The uncovered case belongs to 6031 alone: an exposed net carrying no clamp at
/// all fires that rule and never the downstream half (one defect, one code),
/// even though the same unclamped quiet face sits behind it.
#[test]
fn an_unclamped_exposed_net_reports_6031_only() {
    let src = format!(
        "{TV}{PROT_TWOPIN}module main {{\n    {DOWNSTREAM_QUIET_BOARD}\
         io DP @exposed(esd_contact)\n    \
         TV tv\n    tv.IO -> tv.G\n    \
         TWOPIN.PLAIN r\n    r.A -> DP\n    r.B -> VDDA\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::EXPOSED_NET_NO_CLAMP),
        "an exposed net with no clamp is 6031's verdict; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::EXPOSED_NET_DOWNSTREAM_UNPROTECTED),
        "6031 owns the uncovered case — the downstream half must not stack; got codes: {codes:?}"
    );
}

/// The flood does not cross return/reference copper (reach.rs §7 L4): a quiet
/// *reference* (`@role(quiet)` conduit) reachable only through that copper is
/// never in the region, so no verdict.
#[test]
fn the_flood_does_not_cross_reference_copper_6044() {
    let src = format!(
        "{TV}{PROT_TWOPIN}module main {{\n    {DOWNSTREAM_QUIET_BOARD}\
         io DP @exposed(esd_contact)\n    \
         TV tv\n    tv.IO -> DP\n    tv.G -> ESDGND @clamp(ESDGND)\n    \
         TWOPIN.PLAIN r\n    r.A -> DP\n    r.B -> GNDA\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::EXPOSED_NET_DOWNSTREAM_UNPROTECTED),
        "return/reference copper is never flooded through; got codes: {codes:?}"
    );
}

/// Two exposed ports, each with its own over-reaching branch: the rule is
/// per-port, so both fire (and the per-port read is not a board-wide one-shot).
#[test]
fn two_exposed_ports_with_overreaching_branches_fire_twice_6044() {
    let src = format!(
        "{TV}{PROT_TWOPIN}module main {{\n    {DOWNSTREAM_QUIET_BOARD}\
         io DP, DM @exposed(esd_contact)\n    \
         TV tv\n    tv.IO -> DP\n    tv.G -> ESDGND @clamp(ESDGND)\n    \
         TWOPIN.PLAIN r1\n    r1.A -> DP\n    r1.B -> VDDA\n    \
         TV tv2\n    tv2.IO -> DM\n    tv2.G -> ESDGND @clamp(ESDGND)\n    \
         TWOPIN.PLAIN r2\n    r2.A -> DM\n    r2.B -> VDDA\n}}\n"
    );
    let codes = build_codes(&src);
    let n = codes
        .iter()
        .filter(|c| **c == mcc::errcodes::EXPOSED_NET_DOWNSTREAM_UNPROTECTED)
        .count();
    assert_eq!(n, 2, "one verdict per exposed port; got codes: {codes:?}");
}

// ── PWR-5 protection-device placement (exposed-protection-design.md §4, ruled 2026-09-16) ──
//
// A class declares itself a protection element in its own definition body
// (`protect = shunt|series`); the declaration is the sole witness, since a fuse
// and an ordinary copper pass are structurally identical two-terminal elements.
// The decoded value rides the flat entry (`InstEntry::protection`), decoded once
// at flatten time from the definition's resolved attributes. Two Error verdicts:
//
// * **shunt** (6032) — the device dumps to a reference, so a leg of it must land
//   on a reference its scope (or an ancestor world) declares protective/earth.
// * **series** (6033) — the device carries the supply through itself, so it must
//   be a two-terminal element with no DC row whose ends sit on two different
//   nets, both on a supply tree. "On a supply tree" is the *fed* face 6019 owns,
//   not the 6021 budget root (a capacity-less source boundary is deliberately
//   Opaque to the budget walk); a series device with an end on a return/reference
//   net is not adjudicated (a protective earth-bond element is in series on a
//   reference path, a face §4 does not rule).

/// A class declaring itself an in-line (series) protection element.
const PROT_FUSE: &str =
    "component FUSE.PROT {\n    protect = series\n    pins = [\n        io 1 = A\n        io 2 = B\n    ]\n}\n";

/// A class declaring itself a dumping (shunt) protection element.
const PROT_TVS: &str =
    "component TVS.PROT {\n    protect = shunt\n    pins = [\n        io 1 = A\n        io 2 = B\n    ]\n}\n";

/// The same two-terminal shape with no declaration at all — the control for
/// "the declaration is the only witness".
const PROT_TWOPIN: &str =
    "component TWOPIN.PLAIN {\n    pins = [\n        io 1 = A\n        io 2 = B\n    ]\n}\n";

/// A three-terminal class that still declares itself series.
const PROT_FUSE3: &str = "component FUSE3.PROT {\n    protect = series\n    pins = [\n        \
                          io 1 = A\n        io 2 = B\n        io 3 = C\n    ]\n}\n";

/// A series declaration on a class that also carries a DC power row: a power
/// face, not raw copper, so the supply does not pass through it.
const PROT_FUSE_DC: &str =
    "component FUSEDC.PROT {\n    protect = series\n    pins = [\n        psrc [1,2] = [A, B]::DC(5V)\n    ]\n}\n";

/// A series declaration whose value is misspelled: the decode matches the two
/// words whole, so the class is silently unmarked and both halves stay quiet
/// (the value vocabulary belongs to the declaration plane — design §6 R9).
const PROT_TYPO: &str =
    "component TYPO.PROT {\n    protect = serise\n    pins = [\n        io 1 = A\n        io 2 = B\n    ]\n}\n";

/// A 5V source with no declared capacity — its net is a source boundary, not a
/// budget root (§8.5), which is exactly the oracle the series half must not use.
const PROT_SRC_NOCAP: &str =
    "component SRC.PLAIN {\n    pins = [\n        psrc [1,2] = [OUT, GND]::DC(5V)\n    ]\n}\n";

/// A shunt device dumping onto the protective island is the ruled shape → 6032
/// is silent.
#[test]
fn shunt_leg_on_a_protective_ref_is_silent_6032() {
    let src = format!(
        "{PROT_TVS}{SRC_CAP}\nmodule main {{\n    conduit GND @role(main)\n    \
         conduit ESDGND @role(protective)\n    io V33\n    SRC_CAP s\n    s.OUT -> V33\n    s.GND -> GND\n    \
         TVS.PROT tv\n    tv.A -> V33\n    tv.B -> ESDGND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::PROTECT_SHUNT_NO_REFERENCE),
        "a leg on a @role(protective) reference discharges the shunt declaration; got codes: {codes:?}"
    );
}

/// The ruled defect: the device declares itself a shunt element but every leg
/// lands on an ordinary reference, so it cannot dump anything.
#[test]
fn shunt_with_no_protective_leg_fires_6032() {
    let src = format!(
        "{PROT_TVS}{SRC_CAP}\nmodule main {{\n    conduit GND @role(main)\n    io V33\n    \
         SRC_CAP s\n    s.OUT -> V33\n    s.GND -> GND\n    \
         TVS.PROT tv\n    tv.A -> V33\n    tv.B -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::PROTECT_SHUNT_NO_REFERENCE),
        "a shunt declaration with no leg on a protective/earth reference must fire 6032; got codes: {codes:?}"
    );
}

/// A reference's role may be supplied by an ancestor world (iron rule 1): the
/// child module only forwards its dump leg to an `io` port, and the parent binds
/// that port to its own protective conduit — the declaration is read along the
/// module chain, so the child device is discharged.
#[test]
fn shunt_leg_forwarded_through_a_layer_child_is_silent_6032() {
    let src = format!(
        "{PROT_TVS}{SRC_CAP}\n\
         module LEAF {{\n    io EARTHP\n    io VIN\n    TVS.PROT tv\n    tv.A -> VIN\n    tv.B -> EARTHP\n}}\n\
         module main {{\n    conduit GND @role(main)\n    conduit ESDGND @role(protective)\n    \
         io V33\n    SRC_CAP s\n    s.OUT -> V33\n    s.GND -> GND\n    \
         LEAF u\n    u.VIN -> V33\n    u.EARTHP -> ESDGND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::PROTECT_SHUNT_NO_REFERENCE),
        "the forwarded leg resolves to the parent's protective conduit; got codes: {codes:?}"
    );
}

/// A device with no wired leg at all is the floating-input family's business —
/// the declaration is adjudicated only where the wiring exists.
#[test]
fn shunt_class_with_no_wired_leg_is_not_adjudicated_6032() {
    let src = format!("{PROT_TVS}\nmodule main {{\n    TVS.PROT tv\n}}\n");
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::PROTECT_SHUNT_NO_REFERENCE),
        "no leg means no wiring witness — never guessed here; got codes: {codes:?}"
    );
}

/// The unmarked twin of the same shape is never judged: the declaration is the
/// only witness, so no name table and no pin shape can mark it.
#[test]
fn unmarked_two_terminal_class_is_not_adjudicated_6032_6033() {
    let src = format!(
        "{PROT_TWOPIN}{SRC_CAP}\nmodule main {{\n    conduit GND @role(main)\n    io V33\n    \
         SRC_CAP s\n    s.OUT -> V33\n    s.GND -> GND\n    \
         TWOPIN.PLAIN tp\n    tp.A -> V33\n    tp.B -> GND\n    \
         TWOPIN.PLAIN tb\n    tb.A -> V33\n    tb.B -> V33\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::PROTECT_SHUNT_NO_REFERENCE)
            && !codes.contains(&mcc::errcodes::PROTECT_SERIES_NOT_IN_PATH),
        "an unmarked two-terminal class is an ordinary pass element; got codes: {codes:?}"
    );
}

/// A series declaration really in line on a supply path (source → fuse → load,
/// both ends fed) is the healthy shape → 6033 is silent.
#[test]
fn series_element_in_line_on_a_supply_path_is_silent_6033() {
    let src = format!(
        "{PROT_FUSE}{SRC_CAP}{SNK_AMP3}\nmodule main {{\n    conduit GND @role(main)\n    \
         SRC_CAP s\n    SNK_AMP3 a\n    FUSE.PROT f\n    \
         s.OUT -> V33\n    s.GND -> GND\n    f.A -> V33\n    f.B -> VLOAD\n    \
         a.VDD -> VLOAD\n    a.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::PROTECT_SERIES_NOT_IN_PATH),
        "a two-terminal fuse in line between source and load is the ruled shape; got codes: {codes:?}"
    );
}

/// The ruled defect: both terminals on one net means the device bypasses itself.
#[test]
fn series_element_bypassing_itself_fires_6033() {
    let src = format!(
        "{PROT_FUSE}{SRC_CAP}\nmodule main {{\n    conduit GND @role(main)\n    \
         SRC_CAP s\n    FUSE.PROT f\n    s.OUT -> V33\n    s.GND -> GND\n    \
         f.A -> V33\n    f.B -> V33\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::PROTECT_SERIES_NOT_IN_PATH),
        "both ends on one net is a bypass, not an in-line element; got codes: {codes:?}"
    );
}

/// The ordering half (design §8.3), firing shape: an ordinary pass element in
/// parallel with the declared one carries the same current, so the supply
/// reaches both terminals *without* the declaration — the element is bypassed,
/// not in line between a source and a load. The same law as the "both ends on
/// one net" bypass above, one step further out.
#[test]
fn series_element_with_a_parallel_copper_path_fires_6033() {
    let src = format!(
        "{PROT_FUSE}{PROT_TWOPIN}{SRC_CAP}{SNK_AMP3}\nmodule main {{\n    conduit GND @role(main)\n    \
         SRC_CAP s\n    SNK_AMP3 a\n    FUSE.PROT f\n    TWOPIN.PLAIN t\n    \
         s.OUT -> V33\n    s.GND -> GND\n    f.A -> V33\n    f.B -> VLOAD\n    \
         t.A -> V33\n    t.B -> VLOAD\n    a.VDD -> VLOAD\n    a.GND -> GND\n}}\n"
    );
    let msgs = msgs_of(mcc::errcodes::PROTECT_SERIES_NOT_IN_PATH, &src);
    assert!(
        msgs.iter().any(|m| m.contains("bypassed")),
        "a paralleled plain pass carries the current around the declaration — the element is \
         bypassed, not in line; got messages: {msgs:?}"
    );
}

/// The ordering half, same verdict one scope out: the parallel path runs through
/// a child module's copper, so the removal walk has to cross the module boundary
/// to find the second feed (the boundary arm of the same walk).
#[test]
fn series_element_bypassed_through_a_child_module_fires_6033() {
    let src = format!(
        "{PROT_FUSE}{PROT_TWOPIN}{SRC_CAP}{SNK_AMP3}\n\
         module LEAF {{\n    io P1\n    io P2\n    TWOPIN.PLAIN t\n    t.A -> P1\n    t.B -> P2\n}}\n\
         module main {{\n    conduit GND @role(main)\n    SRC_CAP s\n    SNK_AMP3 a\n    \
         FUSE.PROT f\n    LEAF u\n    \
         s.OUT -> V33\n    s.GND -> GND\n    f.A -> V33\n    f.B -> VLOAD\n    \
         u.P1 -> V33\n    u.P2 -> VLOAD\n    a.VDD -> VLOAD\n    a.GND -> GND\n}}\n"
    );
    let msgs = msgs_of(mcc::errcodes::PROTECT_SERIES_NOT_IN_PATH, &src);
    assert!(
        msgs.iter().any(|m| m.contains("bypassed")),
        "a parallel path routed through a child module is the same bypass; got messages: {msgs:?}"
    );
}

/// The ordering half, silent shape: two declared elements in series. Each is a
/// genuine cut — remove either one and the far side (or the node between them)
/// loses its feed — so both read as in line between a source and a load.
#[test]
fn series_elements_in_a_daisy_chain_are_silent_6033() {
    let src = format!(
        "{PROT_FUSE}{SRC_CAP}{SNK_AMP3}\nmodule main {{\n    conduit GND @role(main)\n    \
         SRC_CAP s\n    SNK_AMP3 a\n    FUSE.PROT f1\n    FUSE.PROT f2\n    \
         s.OUT -> V33\n    s.GND -> GND\n    f1.A -> V33\n    f1.B -> VMID\n    \
         f2.A -> VMID\n    f2.B -> VLOAD\n    a.VDD -> VLOAD\n    a.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::PROTECT_SERIES_NOT_IN_PATH),
        "each element of a series pair is the cut between its own two sides; got codes: {codes:?}"
    );
}

/// The oracle is the *fed* face (6019), not the 6021 budget root: a
/// capacity-less source is a source boundary — deliberately Opaque to the budget
/// walk — so a fuse downstream of it must stay silent. Reading the budget root
/// here would call an ordinary fuse "off the supply tree".
#[test]
fn series_element_downstream_of_a_capacity_less_source_is_silent_6033() {
    let src = format!(
        "{PROT_FUSE}{PROT_SRC_NOCAP}{SNK_AMP5}\nmodule main {{\n    conduit GND @role(main)\n    \
         SRC.PLAIN s\n    SNK_AMP5 a\n    FUSE.PROT f\n    \
         s.OUT -> V5\n    s.GND -> GND\n    f.A -> V5\n    f.B -> VLOAD\n    \
         a.VDD -> VLOAD\n    a.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::PROTECT_SERIES_NOT_IN_PATH),
        "a capacity-less source boundary is not a budget root, but its net is still fed; got codes: {codes:?}"
    );
}

/// An end off every supply tree means the device protects nothing: a fuse on an
/// island with no supply root anywhere is not in series on a supply path.
#[test]
fn series_element_with_no_supply_tree_fires_6033() {
    let src = format!(
        "{PROT_FUSE}\nmodule main {{\n    conduit GND @role(main)\n    \
         FUSE.PROT f\n    f.A -> N1\n    f.B -> N2\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::PROTECT_SERIES_NOT_IN_PATH),
        "a fuse between two supply-less nets protects nothing; got codes: {codes:?}"
    );
}

/// A series device with an end on a return/reference net is not adjudicated: a
/// protective earth-bond element is in series on a *reference* path, a face §4
/// does not rule (a return/reference net is never "fed" by construction).
#[test]
fn series_element_in_the_return_path_is_not_adjudicated_6033() {
    let src = format!(
        "{PROT_FUSE}\nmodule main {{\n    conduit GND @role(main)\n    conduit PGND @role(main)\n    \
         FUSE.PROT f\n    f.A -> GND\n    f.B -> PGND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::PROTECT_SERIES_NOT_IN_PATH),
        "a reference-path series element is not a supply-path face — never guessed; got codes: {codes:?}"
    );
}

/// Not a two-terminal element: a three-terminal class declaring series cannot be
/// an in-line element, whatever its wiring.
#[test]
fn three_terminal_series_class_fires_6033() {
    let src = format!(
        "{PROT_FUSE3}{SRC_CAP}\nmodule main {{\n    conduit GND @role(main)\n    \
         SRC_CAP s\n    FUSE3.PROT f\n    s.OUT -> V33\n    s.GND -> GND\n    \
         f.A -> V33\n    f.B -> N1\n    f.C -> N2\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::PROTECT_SERIES_NOT_IN_PATH),
        "a three-terminal class is not an in-line element; got codes: {codes:?}"
    );
}

/// A DC row on the device itself: the supply does not pass through a power face
/// (the same transparent-copper test §8.5 reads, taken on the declaration).
#[test]
fn series_class_with_a_dc_row_fires_6033() {
    let src = format!(
        "{PROT_FUSE_DC}{SRC_CAP}\nmodule main {{\n    conduit GND @role(main)\n    \
         SRC_CAP s\n    FUSEDC.PROT f\n    s.OUT -> V33\n    s.GND -> GND\n    \
         f.A -> V33\n    f.B -> N1\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::PROTECT_SERIES_NOT_IN_PATH),
        "a DC row makes the device a power face, not a transparent pass; got codes: {codes:?}"
    );
}

/// A misspelled value is silently unmarked — the decode matches the two ruled
/// words whole, so nothing here guesses an intent from a near-miss
/// (exposed-protection-design.md §6 R9 owns the value vocabulary).
#[test]
fn misspelled_protect_value_is_silently_unmarked_6032_6033() {
    let src = format!(
        "{PROT_TYPO}{SRC_CAP}\nmodule main {{\n    conduit GND @role(main)\n    io V33\n    \
         SRC_CAP s\n    s.OUT -> V33\n    s.GND -> GND\n    \
         TYPO.PROT t\n    t.A -> V33\n    t.B -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::PROTECT_SHUNT_NO_REFERENCE)
            && !codes.contains(&mcc::errcodes::PROTECT_SERIES_NOT_IN_PATH),
        "a value that is neither ruled word leaves the class unmarked; got codes: {codes:?}"
    );
}

// ── PWR-4b package dissipation (package-thermal-design.md §3, ruled 2026-09-16) ──
//
// The second half of PWR-4: PWR-4's first layer (6021) budgets a whole net's
// declared sink demand against a declared source capacity; this one asks the
// same question of a single element — the power it dissipates in place against
// the ceiling its own package declares (`spec.power_rated`). Only the **shunt**
// placement is judged: the class declares itself dissipating (`spec.resistance`
// — the ledger's resistive certificate), it is a two-terminal element, and one
// leg sits on a declared rail hot face while the other sits on that rail's
// return or a named reference. The rail's own window is then the volts *across*
// the element — a declared value, so `P = V²/R` needs no solver — taken at its
// far corner, and compared against the declared rating as it stands (the
// design's derating factor stays 1.0, the same ruling that keeps a multiplier
// out of the budget axis). A series pass element is not judged: the engine
// reads a two-terminal device with no DC row as current-transparent copper, so
// both its legs carry one window and neither the volts across it nor a
// per-element current exists (design §3.2 R2).

/// A two-terminal class that declares itself dissipating and rates its package
/// — the rated shunt shape `res.mc` writes (`resistance` + `power_rated`).
const SHUNT_R: &str = "component RSHUNT.PWR(rs::UV.OHM, prated::UV.WATT) {\n    pins = [\n        \
                       io 1 = A\n        io 2 = B\n    ]\n    spec = [\n        resistance = rs\n        \
                       power_rated = prated\n    ]\n}\n";

/// The same shape with the rating left unset (`_`, the PTC/NTC spelling): there
/// is no ceiling to compare, so the element is never judged.
const SHUNT_NORATE: &str = "component RSHUNT.NORATE(rs::UV.OHM) {\n    pins = [\n        \
                            io 1 = A\n        io 2 = B\n    ]\n    spec = [\n        resistance = rs\n        \
                            power_rated = _\n    ]\n}\n";

/// A rated two-terminal class that declares no resistance: no dissipating
/// certificate, so it is not an element this rule judges at all.
const SHUNT_NOCLASS: &str = "component RSHUNT.NOCC(prated::UV.WATT) {\n    pins = [\n        \
                             io 1 = A\n        io 2 = B\n    ]\n    spec = [\n        \
                             power_rated = prated\n    ]\n}\n";

/// A 100 Ω / 0.25 W shunt across a 5 V ±5% rail: the far corner is 5.25 V, so
/// `P = 5.25²/100 = 0.28 W` exceeds the rated 0.25 W → Warning 6035.
#[test]
fn shunt_over_its_package_rating_fires_6035() {
    let src = format!(
        "{SHUNT_R}\nmodule main {{\n    conduit GND @role(main)\n    \
         domain DVDD @class(digital) {{ rail [V5R, GND]::DC(5V, tol:±5%) }}\n    \
         io V5R\n    RSHUNT.PWR r1(100Ω, 0.25W)\n    r1.A -> V5R\n    r1.B -> GND\n}}\n"
    );
    let msgs = msgs_of(mcc::errcodes::SHUNT_DISSIPATION_OVER_RATING, &src);
    assert_eq!(
        msgs.len(),
        1,
        "a 100Ω/0.25W shunt across a 5V±5% rail must fire 6035 once; got: {msgs:?}"
    );
    assert!(
        msgs[0].contains("main.r1"),
        "6035 must name the element; got: {msgs:?}"
    );
    assert!(
        msgs[0].contains("0.28 W") && msgs[0].contains("5.25 V"),
        "6035 must print the far-corner power and volts; got: {msgs:?}"
    );
    assert!(
        msgs[0].contains("power_rated 0.25 W"),
        "6035 must print the declared rating; got: {msgs:?}"
    );
}

/// The same rail with a 1 kΩ part: `P = 0.028 W`, well inside the rating → the
/// verdict is silent. The rail window is the same; only the resistance changed.
#[test]
fn shunt_inside_its_package_rating_is_silent_6035() {
    let src = format!(
        "{SHUNT_R}\nmodule main {{\n    conduit GND @role(main)\n    \
         domain DVDD @class(digital) {{ rail [V5R, GND]::DC(5V, tol:±5%) }}\n    \
         io V5R\n    RSHUNT.PWR r1(1kΩ, 0.25W)\n    r1.A -> V5R\n    r1.B -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::SHUNT_DISSIPATION_OVER_RATING),
        "a part dissipating inside its rating is the healthy shape; got codes: {codes:?}"
    );
}

/// An unrated part (`power_rated = _`) has no ceiling to overrun: the carry is
/// `None` and the element is outside the check rather than guessed.
#[test]
fn shunt_without_a_declared_rating_is_silent_6035() {
    let src = format!(
        "{SHUNT_NORATE}\nmodule main {{\n    conduit GND @role(main)\n    \
         domain DVDD @class(digital) {{ rail [V5R, GND]::DC(5V, tol:±5%) }}\n    \
         io V5R\n    RSHUNT.NORATE r1(100Ω)\n    r1.A -> V5R\n    r1.B -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::SHUNT_DISSIPATION_OVER_RATING),
        "an undeclared rating is not a failing rating — never guessed; got codes: {codes:?}"
    );
}

/// A rated two-terminal part that declares no `resistance` carries no
/// dissipating certificate, so the rule never classifies it as a shunt.
#[test]
fn rated_part_without_a_resistance_certificate_is_silent_6035() {
    let src = format!(
        "{SHUNT_NOCLASS}\nmodule main {{\n    conduit GND @role(main)\n    \
         domain DVDD @class(digital) {{ rail [V5R, GND]::DC(5V, tol:±5%) }}\n    \
         io V5R\n    RSHUNT.NOCC r1(0.25W)\n    r1.A -> V5R\n    r1.B -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::SHUNT_DISSIPATION_OVER_RATING),
        "no resistive certificate means the element is not classified; got codes: {codes:?}"
    );
}

/// A divider's middle leg is `Signal` — no declared identity, so its potential
/// is not a fact this layer holds. Neither half is judged (design §3.1 rule 5,
/// the deliberate conservatism that also keeps two-rail elements out).
#[test]
fn divider_middle_leg_is_not_judged_6035() {
    let src = format!(
        "{SHUNT_R}\nmodule main {{\n    conduit GND @role(main)\n    \
         domain DVDD @class(digital) {{ rail [V5R, GND]::DC(5V, tol:±5%) }}\n    \
         io V5R\n    io VMID\n    RSHUNT.PWR r1(100Ω, 0.25W)\n    RSHUNT.PWR r2(100Ω, 0.25W)\n    \
         r1.A -> V5R\n    r1.B -> VMID\n    r2.A -> VMID\n    r2.B -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::SHUNT_DISSIPATION_OVER_RATING),
        "a divider's middle leg carries no declared potential — never guessed; got codes: {codes:?}"
    );
}

/// Both terminals on one net is the short-circuit face (R02), not a shunt in
/// place: the element reaches one net, so there is no window across it.
#[test]
fn shunt_bypassing_itself_is_not_judged_6035() {
    let src = format!(
        "{SHUNT_R}\nmodule main {{\n    conduit GND @role(main)\n    \
         domain DVDD @class(digital) {{ rail [V5R, GND]::DC(5V, tol:±5%) }}\n    \
         io V5R\n    RSHUNT.PWR r1(100Ω, 0.25W)\n    r1.A -> V5R\n    r1.B -> V5R\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::SHUNT_DISSIPATION_OVER_RATING),
        "a self-bypass is R02's face; 6035 judges a shunt in place only; got codes: {codes:?}"
    );
}

// ── PI-3 decoupling-return face (power-quality-design.md §2.3, ruled 2026-09-16) ──
//
// A decoupling capacitor's two legs are one declared DC pair: the rail the part
// sits across states it (`rail [hot, ret]::DC(…)`), and closing that loop is the
// element's whole job, so its return leg must land on that rail's return member.
// The certificate is the element class read off the definition's spec table
// (capacitive), never a name; both legs are read through the net's effective
// class, so a part instantiated in a sub-module is judged by the class its leg
// resolves to across the boundary — never by the raw island attribution, which
// calls every sub-module net unjudged (R4's measured trap). Error, and
// deliberately unfiltered by @class(analog).
//
// The rail side is read in the scope that *declares* it: its two members are
// looked up by the name written there, so a rail whose member names no net at
// all states no pair this rule can compare against (an incomplete rail
// declaration is another family's verdict — 6038 does not stack on it).

/// A decoupling capacitor in-file, so the fixtures don't depend on the library.
const CAP_DECOUP: &str = "component CAP_DECOUP {\n    pins = [ io [1:2] = [P, N] ]\n    \
                           spec = [ capacitance = 1uF ]\n}\n";

/// The resistive sibling: same two terminals, no capacitive certificate.
const RES_TIE: &str = "component RES_TIE {\n    pins = [ io [1:2] = [P, N] ]\n    \
                       spec = [ resistance = 10k ]\n}\n";

/// The two domains both fixtures share: DVDD's rail returns on GND, AVDD's on
/// GNDA (the golden main.mc shape). The two classes are world-disjoint, which is
/// what makes the wrong-return leg a 6022 candidate as well.
const PI3_DOMAINS: &str = "conduit GND @role(main)\n    conduit GNDA @role(quiet)\n    \
                           domain DVDD @class(digital) { rail [VDD_3V3, GND]::DC(3.3V) }\n    \
                           domain AVDD @class(analog) { rail [VDDA, GNDA]::DC(3.3V) }\n    ";

/// The judged shape, both ways in one board: a capacitor whose return lands on
/// the return member its rail declares is silent, the same part with its return
/// on the other rail's return fires — naming the declared member and the class
/// the leg actually reaches. (The green twin also gives the parent's `GNDA` a
/// net, which is what makes AVDD's declared pair readable at all.)
#[test]
fn decoupling_return_on_the_declared_member_is_clean_and_off_it_fires() {
    let src = format!(
        "{CAP_DECOUP}module main {{\n    {PI3_DOMAINS}\
         CAP_DECOUP ok\n    ok.1 -> VDDA\n    ok.2 -> GNDA\n    \
         CAP_DECOUP bad\n    bad.1 -> VDDA\n    bad.2 -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let n = codes
        .iter()
        .filter(|&&c| c == mcc::errcodes::DECOUPLING_RETURN_MISMATCH)
        .count();
    assert_eq!(
        n, 1,
        "a return leg off the rail's declared member must fire 6038 exactly once (the green twin must not); got codes: {codes:?}"
    );
    let msgs = msgs_of(mcc::errcodes::DECOUPLING_RETURN_MISMATCH, &src);
    assert!(
        msgs.iter().any(|m| m.contains("main.bad")),
        "6038 must name the part: {msgs:?}"
    );
    assert!(
        msgs.iter().any(|m| m.contains("GNDA") && m.contains("GND")),
        "6038 must name the declared return member and the class the leg reaches: {msgs:?}"
    );
}

/// Ruling 8 (2026-09-16), the PI-3 front condition: a capacitor is not a DC
/// element, so 6022 no longer fires on the exact shape PI-3 owns. The fixture's
/// two classes are world-disjoint (`GND` is DVDD's return alone — dumped), so a
/// dissipating element on the same leg *is* 6022's (the test below proves it);
/// the silence here is the ruling's, not co-residence's.
#[test]
fn capacitor_leg_is_not_6022_after_ruling_8() {
    let src = format!(
        "{CAP_DECOUP}module main {{\n    {PI3_DOMAINS}\
         CAP_DECOUP ok\n    ok.1 -> VDDA\n    ok.2 -> GNDA\n    \
         CAP_DECOUP bad\n    bad.1 -> VDDA\n    bad.2 -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert_eq!(
        codes
            .iter()
            .filter(|&&c| c == mcc::errcodes::RETURN_LEG_UNDECLARED)
            .count(),
        0,
        "a capacitive leg carries no DC path — 6022 must stay silent (ruling 8); got codes: {codes:?}"
    );
    assert_eq!(
        codes
            .iter()
            .filter(|&&c| c == mcc::errcodes::DECOUPLING_RETURN_MISMATCH)
            .count(),
        1,
        "the same shape is PI-3's object and must still fire 6038; got codes: {codes:?}"
    );
}

/// The complement, locked in the same shape: the identical leg on a dissipating
/// element IS a DC relation, so 6022 keeps it and 6038 says nothing — the
/// element-class certificate is the only thing separating the two rules.
#[test]
fn resistive_leg_stays_6022_and_is_not_6038() {
    let src = format!(
        "{RES_TIE}module main {{\n    {PI3_DOMAINS}\
         RES_TIE tie\n    tie.1 -> VDDA\n    tie.2 -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert_eq!(
        codes
            .iter()
            .filter(|&&c| c == mcc::errcodes::RETURN_LEG_UNDECLARED)
            .count(),
        1,
        "a resistive leg across world-disjoint classes is still 6022's; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::DECOUPLING_RETURN_MISMATCH),
        "6038 judges capacitors only — no capacitive certificate, no verdict; got codes: {codes:?}"
    );
}

/// Not judged, never guessed: a capacitor that sits across no declared rail has
/// no pair to be wrong about. Both spellings — two bare nets, and two hot faces
/// of two different rails (a bridging part, not a decoupling placement).
#[test]
fn capacitor_off_every_declared_rail_is_not_judged_6038() {
    let bare = format!(
        "{CAP_DECOUP}module main {{\n    {PI3_DOMAINS}\
         CAP_DECOUP ok\n    ok.1 -> VDDA\n    ok.2 -> GNDA\n    \
         CAP_DECOUP c1\n    c1.1 -> RAWX\n    c1.2 -> RAWY\n}}\n"
    );
    let codes = build_codes(&bare);
    assert!(
        !codes.contains(&mcc::errcodes::DECOUPLING_RETURN_MISMATCH),
        "no declared rail on either leg means no pair to violate; got codes: {codes:?}"
    );
    let two_hots = format!(
        "{CAP_DECOUP}module main {{\n    {PI3_DOMAINS}\
         CAP_DECOUP ok\n    ok.1 -> VDDA\n    ok.2 -> GNDA\n    \
         CAP_DECOUP c1\n    c1.1 -> VDD_3V3\n    c1.2 -> VDDA\n}}\n"
    );
    let codes = build_codes(&two_hots);
    assert!(
        !codes.contains(&mcc::errcodes::DECOUPLING_RETURN_MISMATCH),
        "a capacitor across two hot faces is not a decoupling placement (§2.3's shape is one hot leg); got codes: {codes:?}"
    );
}

/// The four boundary cells (§2.3, R4) — a capacitor instantiated inside a child
/// module whose legs the parent layer binds, so both legs only resolve through
/// the A′ boundary walk: ① the return lands on the parent rail's declared member
/// → silent; ② a sibling cap in the same child with its return bound to the
/// other rail's return → that one fires, judged by the class it resolves to and
/// not by the child's net name; ③ a cap whose hot leg's parent-side co-segment
/// carries no declaration at all → never guessed (while the rail stays readable,
/// so the silence is the leg's); ④ the rail declared by the child itself → the
/// part's own layer supplies the pair, and the owning-scope chain (not only
/// ancestors) is what finds it.
#[test]
fn decoupling_return_across_the_module_boundary() {
    let child = "module CHILD() {\n    out HP\n    out RP\n    out HP2\n    out RP2\n    \
                 CAP_DECOUP c1\n    c1.1 -> HP\n    c1.2 -> RP\n    \
                 CAP_DECOUP c2\n    c2.1 -> HP2\n    c2.2 -> RP2\n}\n";
    // ① + ② in one child: c2 is bound to the declared return (silent), c1 to the
    // other rail's return (fires).
    let src = format!(
        "{CAP_DECOUP}{child}module main {{\n    {PI3_DOMAINS}\
         CHILD u\n    u.HP -> VDDA\n    u.RP -> GND\n    u.HP2 -> VDDA\n    u.RP2 -> GNDA\n}}\n"
    );
    let codes = build_codes(&src);
    let fired = msgs_of(mcc::errcodes::DECOUPLING_RETURN_MISMATCH, &src);
    assert_eq!(
        codes
            .iter()
            .filter(|&&c| c == mcc::errcodes::DECOUPLING_RETURN_MISMATCH)
            .count(),
        1,
        "the child cap whose leg the parent binds to the other rail's return must fire once; got codes: {codes:?}"
    );
    assert!(
        fired.iter().any(|m| m.contains("main.u.c1")),
        "the verdict must name the child part whose leg resolves off the declared return: {fired:?}"
    );

    // ③ the hot leg reaches no declared class: silence, not a guess.
    let unreadable = format!(
        "{CAP_DECOUP}{child}module main {{\n    {PI3_DOMAINS}\
         CHILD u\n    u.HP -> RAWX\n    u.RP -> GND\n    u.HP2 -> VDDA\n    u.RP2 -> GNDA\n}}\n"
    );
    let codes = build_codes(&unreadable);
    assert!(
        !codes.contains(&mcc::errcodes::DECOUPLING_RETURN_MISMATCH),
        "a hot leg whose class does not resolve is never guessed past a declaration anchor; got codes: {codes:?}"
    );

    // ④ the child declares the rail itself.
    let own = format!(
        "{CAP_DECOUP}module CHILD() {{\n    domain CD {{ rail [CL, CR]::DC(3.3V) }}\n    \
         CAP_DECOUP c1\n    c1.1 -> CL\n    c1.2 -> CR\n}}\nmodule main {{\n    CHILD u\n}}\n"
    );
    let codes = build_codes(&own);
    assert!(
        !codes.contains(&mcc::errcodes::DECOUPLING_RETURN_MISMATCH),
        "a rail declared by the part's own scope is the same witness as a parent's; got codes: {codes:?}"
    );
}

/// The boundary face of 6035 (R4's measured hole): the 100 Ω/0.25 W shunt that
/// fires in `main` is judged **identically** when it is instantiated one module
/// down and its legs are bound to the parent's rail. Neither leg carries a role
/// in its own scope (both read `Signal` there), so the pair comes from the rail
/// the parent declares, reached by the effective-class walk — and with it the
/// rail's own declared window, which is why the numbers below are the in-`main`
/// numbers to the digit (5.25 V far corner, 0.28 W).
#[test]
fn shunt_inside_a_submodule_is_judged_through_the_boundary_6035() {
    let src = format!(
        "{SHUNT_R}\nmodule CHILD() {{\n    out HA\n    out HB\n    RSHUNT.PWR r1(100Ω, 0.25W)\n    \
         r1.A -> HA\n    r1.B -> HB\n}}\nmodule main {{\n    conduit GND @role(main)\n    \
         domain DVDD @class(digital) {{ rail [V5R, GND]::DC(5V, tol:±5%) }}\n    io V5R\n    \
         CHILD u\n    u.HA -> V5R\n    u.HB -> GND\n}}\n"
    );
    let msgs = msgs_of(mcc::errcodes::SHUNT_DISSIPATION_OVER_RATING, &src);
    assert_eq!(
        msgs.len(),
        1,
        "a shunt inside a sub-module, its legs bound to the parent rail, is the same shunt; got: {msgs:?}"
    );
    assert!(
        msgs[0].contains("main.u.r1"),
        "6035 must name the part by its flat path; got: {msgs:?}"
    );
    assert!(
        msgs[0].contains("0.28 W") && msgs[0].contains("5.25 V"),
        "the parent rail's declared window is the one across the element, at either layer; got: {msgs:?}"
    );
}

/// The boundary walk widens *which layer* supplies the declared pair, never
/// whether one exists: a child part whose legs land on two bare nets, or across
/// two different rails' hot faces (a divider's shape), is still not a shunt in
/// place — no window, no volts across the element, no verdict.
#[test]
fn shunt_inside_a_submodule_off_every_rail_pair_is_silent_6035() {
    let bare = format!(
        "{SHUNT_R}\nmodule CHILD() {{\n    out HA\n    out HB\n    RSHUNT.PWR r1(100Ω, 0.25W)\n    \
         r1.A -> HA\n    r1.B -> HB\n}}\nmodule main {{\n    conduit GND @role(main)\n    \
         domain DVDD @class(digital) {{ rail [V5R, GND]::DC(5V, tol:±5%) }}\n    io V5R\n    \
         CHILD u\n    u.HA -> RAWX\n    u.HB -> RAWY\n}}\n"
    );
    let codes = build_codes(&bare);
    assert!(
        !codes.contains(&mcc::errcodes::SHUNT_DISSIPATION_OVER_RATING),
        "legs on two bare nets carry no declared pair at any layer; got codes: {codes:?}"
    );
    let two_hots = format!(
        "{SHUNT_R}\nmodule CHILD() {{\n    out HA\n    out HB\n    RSHUNT.PWR r1(100Ω, 0.25W)\n    \
         r1.A -> HA\n    r1.B -> HB\n}}\nmodule main {{\n    conduit GND @role(main)\n    \
         conduit GNDA @role(quiet)\n    \
         domain DVDD @class(digital) {{ rail [V5R, GND]::DC(5V, tol:±5%) }}\n    \
         domain AVDD @class(analog) {{ rail [V3R, GNDA]::DC(3.3V) }}\n    io V5R\n    \
         CHILD u\n    u.HA -> V5R\n    u.HB -> V3R\n}}\n"
    );
    let codes = build_codes(&two_hots);
    assert!(
        !codes.contains(&mcc::errcodes::SHUNT_DISSIPATION_OVER_RATING),
        "a part across two hot faces has no return leg, so no rail pair spans it; got codes: {codes:?}"
    );
}

// ── PWR-4b series face (package-thermal-design.md §7, ruled 2026-09-17) ──
//
// The removal method: cut the element out of the copper and ask each end
// whether it still carries a feed. Exactly one end going dark makes the element
// the cut between a source and that side, and the current through it the whole
// demand of the region that went dark — 6021's own reading of that copper, asked
// with the element removed from the flood. `P = I²R` against the declared rating.
//
// Every fixture below writes its rating explicitly (`r1(2Ω, 0.25W)`): the
// corpus's `RES(rs, tol)` spelling never passes `prated`, so a fixture copied
// from a board would carry no rating and quietly test nothing (§7.1).
//
// The silences are single-axis flips off the board that fires — each one is the
// flipped axis talking, not a rule that never ran.

/// A sink that declares no `amp` at all: the demand key is opt-in, so its draw
/// is unknown rather than zero.
const SNK5_NOAMP: &str =
    "component SNK5_NOAMP {\n    pins = [\n        psnk [1,2] = [VDD, GND]::DC(5V)\n    ]\n}\n";

/// A second two-pin part with no DC rows — a fuse/ferrite at the flat layer, and
/// therefore transparent copper to the engine. Wired in parallel with the
/// element under test it *is* the bypass path.
const WIRE2: &str = "component WIRE2 {\n    pins = [\n        io [1,2] = [X, Y]\n    ]\n}\n";

/// A silence cell must not be green because the board was never wired: a
/// connection statement that fails to parse is **not** a build failure (an
/// error still instantiates — see the project's E4112/E4116 ruling), so a
/// dropped wire reads as "nothing to judge". Assert the board was understood
/// before believing its silence. (A board spelled `x -> [a, B]` does exactly
/// this: E4007 shape mismatch + E3132, and the element ends up unconnected.)
fn assert_wiring_understood(codes: &[u32]) {
    for code in [
        mcc::errcodes::CONN_STMT_PARSE_FAILED,
        mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH,
    ] {
        assert!(
            !codes.contains(&code),
            "the board's connections must be understood before its silence means anything; got codes: {codes:?}"
        );
    }
}

/// The judged series shape: a 5V source feeds a 500mA sink only through a
/// 2 Ω/0.25 W element, so `I = 0.5 A` and `P = I²R = 0.5 W` — above the rating
/// → Warning 6035. The element's own end toward the sink is the one that loses
/// its feed, which is what makes it the part the region's current passes
/// through.
#[test]
fn series_element_over_its_package_rating_fires_6035() {
    let src = format!(
        "{SRC5_CAP1A}{SNK5_500}{SHUNT_R}\nmodule main {{\n    conduit GND @role(main)\n    \
         SRC5_CAP_1000 s\n    SNK5_500 k\n    RSHUNT.PWR r1(2Ω, 0.25W)\n    \
         s.OUT -> r1.A\n    r1.B -> k.VDD\n    s.GND -> GND\n    k.GND -> GND\n}}\n"
    );
    let msgs = msgs_of(mcc::errcodes::SHUNT_DISSIPATION_OVER_RATING, &src);
    assert_eq!(
        msgs.len(),
        1,
        "a 2Ω/0.25W element carrying the 500mA a 5V root feeds must fire 6035 once; got: {msgs:?}"
    );
    assert!(
        msgs[0].contains("main.r1"),
        "6035 must name the element by its flat path; got: {msgs:?}"
    );
    assert!(
        msgs[0].contains("0.5 W") && msgs[0].contains("power_rated 0.25 W"),
        "6035 must print the dissipated power and the declared rating; got: {msgs:?}"
    );
    assert!(
        msgs[0].contains("0.5 A") && msgs[0].contains("2 Ω"),
        "the series reading is the current through the part at its resistance — not volts across it; got: {msgs:?}"
    );
}

/// The same board with a 0.2 Ω part: `P = I²R = 0.05 W`, comfortably inside the
/// rating → silent. Only the resistance moved.
#[test]
fn series_element_inside_its_package_rating_is_silent_6035() {
    let src = format!(
        "{SRC5_CAP1A}{SNK5_500}{SHUNT_R}\nmodule main {{\n    conduit GND @role(main)\n    \
         SRC5_CAP_1000 s\n    SNK5_500 k\n    RSHUNT.PWR r1(0.2Ω, 0.25W)\n    \
         s.OUT -> r1.A\n    r1.B -> k.VDD\n    s.GND -> GND\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert_wiring_understood(&codes);
    assert!(
        !codes.contains(&mcc::errcodes::SHUNT_DISSIPATION_OVER_RATING),
        "an element dissipating inside its rating is the healthy shape; got codes: {codes:?}"
    );
}

/// `I` is the demand of the region that went **dark**, not everything the cut
/// region can reach once the flood is let back across the element. A second
/// 500mA sink hangs on the *source* side; without the removal the region flood
/// would walk through the element and add it, reporting 1 A where the part
/// carries 0.5 A. Both spellings fire, so the printed current is the
/// discriminator.
#[test]
fn series_element_counts_only_its_own_downstream_region_6035() {
    let src = format!(
        "{SRC5_CAP1A}{SNK5_500}{SHUNT_R}\nmodule main {{\n    conduit GND @role(main)\n    \
         io VIN\n    io VLOAD\n    SRC5_CAP_1000 s\n    SNK5_500 k1\n    SNK5_500 k2\n    \
         RSHUNT.PWR r1(2Ω, 0.25W)\n    \
         s.OUT -> VIN\n    k2.VDD -> VIN\n    r1.A -> VIN\n    r1.B -> VLOAD\n    k1.VDD -> VLOAD\n    \
         s.GND -> GND\n    k1.GND -> GND\n    k2.GND -> GND\n}}\n"
    );
    let msgs = msgs_of(mcc::errcodes::SHUNT_DISSIPATION_OVER_RATING, &src);
    assert_eq!(
        msgs.len(),
        1,
        "the element's own downstream region is the one that went dark; got: {msgs:?}"
    );
    assert!(
        msgs[0].contains("0.5 A") && !msgs[0].contains("1 A"),
        "the current is the downstream region's demand alone — the source side's own sink is not carried by this part; got: {msgs:?}"
    );
}

/// A declared fuse/ferrite in parallel with the element: the supply reaches both
/// ends without it, so the element is **bypassed** rather than in line, and no
/// current through it is a fact here (PWR-5's 6033 owns that shape, not this
/// one). The board is the previous cell's — the element that fires there — with
/// one part added, so the silence is the bypass talking.
#[test]
fn series_element_bypassed_by_parallel_copper_is_silent_6035() {
    let src = format!(
        "{SRC5_CAP1A}{SNK5_500}{SHUNT_R}{WIRE2}\nmodule main {{\n    conduit GND @role(main)\n    \
         io VIN\n    io VLOAD\n    SRC5_CAP_1000 s\n    SNK5_500 k1\n    SNK5_500 k2\n    \
         WIRE2 w1\n    RSHUNT.PWR r1(2Ω, 0.25W)\n    \
         s.OUT -> VIN\n    k2.VDD -> VIN\n    r1.A -> VIN\n    w1.X -> VIN\n    \
         r1.B -> VLOAD\n    w1.Y -> VLOAD\n    k1.VDD -> VLOAD\n    \
         s.GND -> GND\n    k1.GND -> GND\n    k2.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert_wiring_understood(&codes);
    assert!(
        !codes.contains(&mcc::errcodes::SHUNT_DISSIPATION_OVER_RATING),
        "the supply reaches both ends with the element removed — it is bypassed, so no current through it is judged; got codes: {codes:?}"
    );
}

/// The downstream region draws, but nobody declared how much: `amp` is opt-in
/// (`SNK5_NOAMP` writes only `::DC(5V)`), so the current is **unknown** rather
/// than zero — and an unknown is never converted into a verdict.
#[test]
fn series_element_with_no_declared_demand_is_silent_6035() {
    let src = format!(
        "{SRC5_CAP1A}{SNK5_NOAMP}{SHUNT_R}\nmodule main {{\n    conduit GND @role(main)\n    \
         SRC5_CAP_1000 s\n    SNK5_NOAMP k\n    RSHUNT.PWR r1(2Ω, 0.25W)\n    \
         s.OUT -> r1.A\n    r1.B -> k.VDD\n    s.GND -> GND\n    k.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert_wiring_understood(&codes);
    assert!(
        !codes.contains(&mcc::errcodes::SHUNT_DISSIPATION_OVER_RATING),
        "an undeclared demand is not a zero current — the region's draw is unknown, so it is never judged; got codes: {codes:?}"
    );
}

/// Two elements in series each carry the *same* current (the region's whole
/// demand), so each is judged on its own resistance — one row per element, both
/// naming their own path. This is also the proof that the cut region floods
/// *through* the other element (transparent copper) to reach the sink.
#[test]
fn series_elements_in_a_daisy_chain_each_fire_6035() {
    let src = format!(
        "{SRC5_CAP1A}{SNK5_500}{SHUNT_R}\nmodule main {{\n    conduit GND @role(main)\n    \
         SRC5_CAP_1000 s\n    SNK5_500 k\n    RSHUNT.PWR r1(2Ω, 0.25W)\n    RSHUNT.PWR r2(2Ω, 0.25W)\n    \
         s.OUT -> r1.A\n    r1.B -> r2.A\n    r2.B -> k.VDD\n    s.GND -> GND\n    k.GND -> GND\n}}\n"
    );
    let msgs = msgs_of(mcc::errcodes::SHUNT_DISSIPATION_OVER_RATING, &src);
    assert_eq!(
        msgs.len(),
        2,
        "the one current is carried by both elements, so each is judged once; got: {msgs:?}"
    );
    assert!(
        msgs.iter().any(|m| m.contains("main.r1")) && msgs.iter().any(|m| m.contains("main.r2")),
        "each row names the element it is about; got: {msgs:?}"
    );
}

/// The two faces are mutually exclusive: a shunt's hot leg is a rail face and
/// its return leg is `Ret`/`Reference` copper (never fed at all), so no end
/// loses a feed and the series face never speaks. The board below is the shunt
/// lock's own; it must still be exactly one row, reading volts rather than
/// amperes.
#[test]
fn shunt_shape_is_not_judged_again_by_the_series_face_6035() {
    let src = format!(
        "{SHUNT_R}\nmodule main {{\n    conduit GND @role(main)\n    \
         domain DVDD @class(digital) {{ rail [V5R, GND]::DC(5V, tol:±5%) }}\n    \
         io V5R\n    RSHUNT.PWR r1(100Ω, 0.25W)\n    r1.A -> V5R\n    r1.B -> GND\n}}\n"
    );
    let msgs = msgs_of(mcc::errcodes::SHUNT_DISSIPATION_OVER_RATING, &src);
    assert_eq!(
        msgs.len(),
        1,
        "a shunt across a declared rail is judged once, by the window face; got: {msgs:?}"
    );
    assert!(
        msgs[0].contains(" V across ") && !msgs[0].contains("passes through"),
        "the shunt's reading is the rail window's far corner, never the removal method's current; got: {msgs:?}"
    );
}

// ── PI-2 filter-leg load-side decoupling (power-quality-design.md §2.2, ruling 11) ──
//
// A `@bridge` whose two endpoints are both hot faces is a supply filter leg: the
// ferrite is the series half of a filter, so the LC only exists once the load
// side it protects carries a decoupling element. The load side is read from the
// declaration — the endpoint whose domain world is a quiet/sensitive face (§1.4)
// — never from the arrow order, which the corpus writes both ways.
//
// Ruling 11 (2026-09-17) cuts the verdict to existence: *a* declared capacitor on
// that net answers, wherever its own return leg lands (that placement is PI-3's
// 6038). Every silence below is a single-axis flip off the board that fires, so each
// negative is the flipped axis talking, not a rule that never ran.

/// A bridge part that is neither capacitive nor the load side's own element —
/// an in-file ferrite stand-in (the fixture never needs its class: the bridge is
/// read from the declared clause, not from the part).
const FB_BRIDGE: &str =
    "component FB_BRIDGE {\n    pins = [\n        io [1,2] = [X, Y]\n    ]\n}\n";

/// A second analog domain, for the "both sides quiet" flip.
const PI2_ANALOG2: &str = "domain AVDD2 @class(analog) { rail [VDDA2, GNDA]::DC(3.3V) }\n    ";

/// The judged shape: a declared filter bridge from the digital rail onto the
/// analog one, with no capacitor on the analog side — the ferrite alone is not
/// the filter. The message names the load member, the domain that made it the
/// load side, and the bridge clause it was declared on.
#[test]
fn filter_bridge_without_load_side_decoupling_fires_6037() {
    let src = format!(
        "{FB_BRIDGE}{CAP_DECOUP}module main {{\n    {PI3_DOMAINS}\
         VDD_3V3 - fba::FB_BRIDGE() - VDDA @bridge(VDD_3V3, VDDA)\n}}\n"
    );
    let codes = build_codes(&src);
    let n = codes
        .iter()
        .filter(|&&c| c == mcc::errcodes::BRIDGE_LOAD_DECOUPLING_MISSING)
        .count();
    assert_eq!(
        n, 1,
        "a declared filter leg whose load side carries no capacitor must fire 6037 exactly once; got codes: {codes:?}"
    );
    let msgs = msgs_of(mcc::errcodes::BRIDGE_LOAD_DECOUPLING_MISSING, &src);
    assert!(
        msgs.iter().any(|m| m.contains("VDDA") && m.contains("AVDD")),
        "6037 must name the load-side member and the quiet domain that made it the load side: {msgs:?}"
    );
    assert!(
        msgs.iter()
            .any(|m| m.contains("VDD_3V3") && m.contains("VDDA")),
        "6037 must name the bridge clause it was declared on: {msgs:?}"
    );
}

/// The same board with the load-side capacitor added — the LC exists. The
/// capacitor's own return is the declared one, so 6038 is silent too: the green
/// here is coverage, not the placement rule carrying the load.
#[test]
fn filter_bridge_with_load_side_decoupling_is_clean_6037() {
    let src = format!(
        "{FB_BRIDGE}{CAP_DECOUP}module main {{\n    {PI3_DOMAINS}\
         VDD_3V3 - fba::FB_BRIDGE() - VDDA @bridge(VDD_3V3, VDDA)\n    \
         CAP_DECOUP ok\n    ok.1 -> VDDA\n    ok.2 -> GNDA\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::BRIDGE_LOAD_DECOUPLING_MISSING),
        "a capacitor on the load-side hot member closes the LC; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::DECOUPLING_RETURN_MISMATCH),
        "…and that capacitor's return lands on the declared member, so 6038 stays silent; got codes: {codes:?}"
    );
}

/// The load side is the quiet one, and that is what the verdict reads: the same
/// board with its only capacitor on the **supply** side still fires. A rule that
/// merely asked "does this bridge have a capacitor somewhere" would pass here.
#[test]
fn capacitor_on_the_supply_side_does_not_cover_the_load_side_6037() {
    let src = format!(
        "{FB_BRIDGE}{CAP_DECOUP}module main {{\n    {PI3_DOMAINS}\
         VDD_3V3 - fba::FB_BRIDGE() - VDDA @bridge(VDD_3V3, VDDA)\n    \
         CAP_DECOUP ok\n    ok.1 -> VDD_3V3\n    ok.2 -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::BRIDGE_LOAD_DECOUPLING_MISSING),
        "the load side is read from the quiet face, not from the arrow order or from any leg of the bridge; got codes: {codes:?}"
    );
}

/// Ruling 11's seam (§4.2's partition, the ruling-8 shape): a capacitor whose
/// return lands **off** the declared member answers PI-2's existence question all
/// the same — the placement is 6038's verdict, and it must not be reported twice.
#[test]
fn mis_landed_return_is_6038_alone_not_6037_as_well() {
    let src = format!(
        "{FB_BRIDGE}{CAP_DECOUP}module main {{\n    {PI3_DOMAINS}\
         VDD_3V3 - fba::FB_BRIDGE() - VDDA @bridge(VDD_3V3, VDDA)\n    \
         CAP_DECOUP ok\n    ok.1 -> VDDA\n    ok.2 -> GNDA\n    \
         CAP_DECOUP bad\n    bad.1 -> VDDA\n    bad.2 -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::DECOUPLING_RETURN_MISMATCH),
        "a return leg off the declared member is PI-3's verdict; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::BRIDGE_LOAD_DECOUPLING_MISSING),
        "…and the load side does carry a capacitor, so PI-2 must stay silent — one fact, one code; got codes: {codes:?}"
    );
}

/// Ruling 3's silence (§2.2): with no quiet/sensitive side there is no declared
/// load side to judge — the flip is one word on AVDD's domain row.
#[test]
fn bridge_with_no_quiet_side_is_not_judged_6037() {
    let src = format!(
        "{FB_BRIDGE}module main {{\n    conduit GND @role(main)\n    conduit GNDA @role(quiet)\n    \
         domain DVDD @class(digital) {{ rail [VDD_3V3, GND]::DC(3.3V) }}\n    \
         domain AVDD @class(digital) {{ rail [VDDA, GNDA]::DC(3.3V) }}\n    \
         VDD_3V3 - fba::FB_BRIDGE() - VDDA @bridge(VDD_3V3, VDDA)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::BRIDGE_LOAD_DECOUPLING_MISSING),
        "neither side reads quiet, so no side is the declared load side; got codes: {codes:?}"
    );
}

/// Both sides quiet is a coin flip, not a load side: silent rather than guessed.
/// The flip is one bridge row onto a second analog domain — both ends quiet.
#[test]
fn bridge_with_both_sides_quiet_is_not_judged_6037() {
    let src = format!(
        "{FB_BRIDGE}module main {{\n    {PI3_DOMAINS}{PI2_ANALOG2}\
         VDDA - fba::FB_BRIDGE() - VDDA2 @bridge(VDDA, VDDA2)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::BRIDGE_LOAD_DECOUPLING_MISSING),
        "two quiet sides leave the load side undecidable — silence, not a guess; got codes: {codes:?}"
    );
}

/// §2.2's subject is the supply filter leg: a ground-side bridge (`FB_agnd`'s
/// shape — both ends on return faces) is not one, so its load side owes nothing.
#[test]
fn ground_side_bridge_is_not_judged_6037() {
    let src = format!(
        "{FB_BRIDGE}module main {{\n    {PI3_DOMAINS}\
         GNDA - fbg::FB_BRIDGE() - GND @bridge(GND, GNDA)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::BRIDGE_LOAD_DECOUPLING_MISSING),
        "a bridge whose two ends are both return faces is not a supply filter leg; got codes: {codes:?}"
    );
}

/// A `@couple` edge is a DC-blocking coupling element, not a filter leg — the
/// flip is the relation word alone.
#[test]
fn couple_edge_is_not_judged_6037() {
    let src = format!(
        "{FB_BRIDGE}module main {{\n    {PI3_DOMAINS}\
         VDD_3V3 - fba::FB_BRIDGE() - VDDA @couple(VDD_3V3, VDDA)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::BRIDGE_LOAD_DECOUPLING_MISSING),
        "@couple declares an AC path, not a filtered supply leg; got codes: {codes:?}"
    );
}

// ── PI-1 sink-pin decoupling completeness (power-quality-design.md §2.1, ruling 2) ──
//
// A pin that **draws** from a declared DC pair is the load half of that pair, and
// the pair is what says the load is fed across two nets: a load drawing across a
// pair nothing decouples is the completeness gap. The subject is the load
// terminal — a `psnk` component pin row, and a module supply port (§6 R2) — never
// the filter: PI-2 judges a ferrite leg's load side, PI-3 a capacitor's return.
//
// The pair is read from the sink's **own contract row**, not from the net's
// declared rail face. That is what makes the golden board's ⑤ power chain
// judgeable at all: `[VMAIN_5V, GND]` there is written on connection lines, a
// pair owned by no domain's rail, so a rule reading declared rail faces would be
// blind to exactly the loads §0.4 counted. §2.1's parenthetical role read would
// read the same way only for loads that happen to sit on a rail face.
//
// Warning, per ruling 2 — a completeness gap, not a contradiction. Ruling 11's
// partition (§4.2 item 9) holds here too: the verdict is whether a capacitor sits
// on the sink's hot net, and where that capacitor's *return* leg lands is PI-3's
// 6038 — one defect, one code, never two.
//
// Every silence below is a single-axis flip off the board that fires, and where
// the flip could otherwise pass for a rule that never ran, the firing twin is
// kept on the board so the count proves it.

/// The judged load: a two-pin sink drawing 3.3V across the declared pair
/// `[VDD, GND]`. Pin 2 is the return half of the same row and carries no
/// direction, so it is never a site of its own.
const SINK_DC: &str =
    "component SINK_DC {\n    pins = [\n        psnk [1,2] = [VDD, GND]::DC(3.3V)\n    ]\n}\n";

/// The same load with a single-member row: §2.1's shape that declares no pair at
/// all, so there is no pair to be decoupled across.
const SINK_SCALAR: &str =
    "component SINK_SCALAR {\n    pins = [\n        psnk 1 = VDD::DC(3.3V)\n    ]\n}\n";

/// A source in the sink's shape — the flip is the direction word alone.
const SRC_DC: &str =
    "component SRC_DC {\n    pins = [\n        psrc [1,2] = [OUT, GND]::DC(3.3V)\n    ]\n}\n";

/// A submodule whose supply face is a `psnk` **header** port — the spelling the
/// corpus writes for every module supply face (`MIC_SIP(psnk dc{VDD_3V3,
/// GND}::DC(3.3V))`, §6 R2's second half). The body is deliberately empty: the
/// header row is the whole subject, and E2115 is the parser's remark on the
/// fixture's shape, not a code this test reads.
const SUB_PSNK: &str = "module SUB_PSNK(psnk dc{VDD_3V3, GND}::DC(3.3V)) {\n}\n";

/// The `DC` interface, declared in-file. This harness loads no system library,
/// and a header port row resolves its `::DC(…)` tail as a class reference — the
/// one place the single-string harness differs from a project build (see
/// `dc_binding_arrow_dir.rs`). Only the header row needs it: a component pin
/// row's `::DC` is captured structurally and never resolved.
const DC_IFACE: &str =
    "interface DC(volt) {\n    pins = [\n        1 = VCC\n        2 = GND\n    ]\n}\n";

/// The board every case below flips one axis of: two declared digital rails, one
/// pair each, so a single board can carry a covered load beside a bare one.
const PI1_BOARD: &str = "conduit GND @role(main)\n    \
                         domain DVDD @class(digital) { rail [VDD_3V3, GND]::DC(3.3V) }\n    \
                         domain DVDD5 @class(digital) { rail [VDD_5V, GND]::DC(5V) }\n    ";

/// The judged shape: a sink drawing from a declared pair that nothing decouples.
/// Exactly **one** verdict — the return-side pin of the same row is not a load,
/// and the pair is stated on the message so the reader can see which two nets
/// the fix has to bridge.
#[test]
fn sink_pin_whose_declared_pair_carries_no_capacitor_fires_6036() {
    let src = format!(
        "{SINK_DC}module main {{\n    {PI1_BOARD}\
         SINK_DC s\n    s.VDD -> VDD_3V3\n    s.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let n = codes
        .iter()
        .filter(|&&c| c == mcc::errcodes::SINK_PIN_NO_DECOUPLING)
        .count();
    assert_eq!(
        n, 1,
        "a load drawing across an undecoupled declared pair must fire 6036 exactly once; got codes: {codes:?}"
    );
    let msgs = msgs_of(mcc::errcodes::SINK_PIN_NO_DECOUPLING, &src);
    assert!(
        msgs.iter().any(|m| m.contains("main.s.VDD")),
        "6036 must name the load terminal, not the component or a positional pin id: {msgs:?}"
    );
    assert!(
        msgs.iter()
            .any(|m| m.contains("VDD_3V3") && m.contains("GND")),
        "6036 must state the declared pair the fix has to bridge: {msgs:?}"
    );
}

/// The covered load and a bare twin on one board: a capacitor across the pair
/// answers, and the twin proves the silence is the capacitor talking — a rule
/// that never ran would report neither.
#[test]
fn capacitor_across_the_declared_pair_covers_the_load_6036() {
    let src = format!(
        "{SINK_DC}{CAP_DECOUP}module main {{\n    {PI1_BOARD}\
         SINK_DC s\n    s.VDD -> VDD_3V3\n    s.GND -> GND\n    \
         CAP_DECOUP ok\n    ok.1 -> VDD_3V3\n    ok.2 -> GND\n    \
         SINK_DC t\n    t.VDD -> VDD_5V\n    t.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::SINK_PIN_NO_DECOUPLING, &src);
    assert_eq!(
        msgs.len(),
        1,
        "only the bare twin may fire — the covered load must not; got codes: {codes:?}"
    );
    assert!(
        msgs[0].contains("main.t.VDD") && !msgs[0].contains("main.s.VDD"),
        "…and it is the bare twin that fires: {msgs:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::DECOUPLING_RETURN_MISMATCH),
        "the covering capacitor returns on the declared member, so 6038 stays silent; got codes: {codes:?}"
    );
}

/// Ruling 11's seam (§4.2 item 9, the shape ruling 8 gave the 6022 cut): a
/// capacitor whose return lands **off** the declared member answers PI-1's
/// existence question all the same. The placement is 6038's verdict and must not
/// be reported twice — "no decoupling" would be false; there is one, misplaced.
#[test]
fn mis_landed_return_is_6038_alone_not_6036_as_well() {
    let src = format!(
        "{SINK_DC}{CAP_DECOUP}module main {{\n    {PI3_DOMAINS}\
         SINK_DC s\n    s.VDD -> VDD_3V3\n    s.GND -> GND\n    \
         CAP_DECOUP bad\n    bad.1 -> VDD_3V3\n    bad.2 -> GNDA\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::DECOUPLING_RETURN_MISMATCH),
        "a return leg off the declared member is PI-3's verdict; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::SINK_PIN_NO_DECOUPLING),
        "…and a capacitor does sit on the sink's hot net, so PI-1 must stay silent — one fact, one code; got codes: {codes:?}"
    );
}

/// §1.2's class law: the candidate is the element class, never a name or a pin
/// shape. Same two terminals, same pair, no capacitive certificate — a resistor
/// across the pair is not decoupling, so the load stays uncovered.
#[test]
fn resistive_part_on_the_pair_does_not_cover_the_load_6036() {
    let src = format!(
        "{SINK_DC}{RES_TIE}module main {{\n    {PI1_BOARD}\
         SINK_DC s\n    s.VDD -> VDD_3V3\n    s.GND -> GND\n    \
         RES_TIE r\n    r.P -> VDD_3V3\n    r.N -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::SINK_PIN_NO_DECOUPLING),
        "a part with no capacitance in its spec table is not a decoupling capacitor; got codes: {codes:?}"
    );
}

/// The other condition on the candidate: two terminals on two nets. The same
/// capacitor def, on the same pair, covers when its legs are the pair and not
/// when both legs land on the hot net — a shorted part decouples nothing, and
/// the covering instance is what proves the class read is not what changed.
#[test]
fn capacitor_with_both_legs_on_one_net_does_not_cover_the_load_6036() {
    let src = format!(
        "{SINK_DC}{CAP_DECOUP}module main {{\n    {PI1_BOARD}\
         SINK_DC s\n    s.VDD -> VDD_3V3\n    s.GND -> GND\n    \
         CAP_DECOUP short\n    short.1 -> VDD_3V3\n    short.2 -> VDD_3V3\n    \
         SINK_DC t\n    t.VDD -> VDD_5V\n    t.GND -> GND\n    \
         CAP_DECOUP ok\n    ok.1 -> VDD_5V\n    ok.2 -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::SINK_PIN_NO_DECOUPLING, &src);
    assert_eq!(
        msgs.len(),
        1,
        "the shorted capacitor must not cover the load; the covered twin must not fire; got codes: {codes:?}"
    );
    assert!(
        msgs[0].contains("main.s.VDD"),
        "…and it is the load behind the shorted capacitor that fires: {msgs:?}"
    );
}

/// A designed-in but not placed part is not on the board: the `NC` construction
/// argument leaves the pair undecoupled, so the load still fires. The flip is the
/// one word on the capacitor's row.
#[test]
fn not_fitted_capacitor_does_not_cover_the_load_6036() {
    let src = format!(
        "{SINK_DC}{CAP_DECOUP}module main {{\n    {PI1_BOARD}\
         SINK_DC s\n    s.VDD -> VDD_3V3\n    s.GND -> GND\n    \
         CAP_DECOUP(NC) dnp\n    dnp.1 -> VDD_3V3\n    dnp.2 -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        codes.contains(&mcc::errcodes::SINK_PIN_NO_DECOUPLING),
        "a not-fitted capacitor is not a decoupling placement; got codes: {codes:?}"
    );
}

/// U163 (hbl `wm7121(NC)`): a sink whose **own part** is marked `nc` is not on
/// the board, so its pins draw nothing and its pair feeds no load here — the
/// not-fitted read the candidate side already applies reaches the subject. The
/// fitted twin on the same board is what proves the rule ran and stayed silent
/// only for the unmounted part.
#[test]
fn not_fitted_sink_part_is_not_judged_6036() {
    let src = format!(
        "{SINK_DC}{CAP_DECOUP}module main {{\n    {PI1_BOARD}\
         SINK_DC(NC) z\n    z.VDD -> VDD_3V3\n    z.GND -> GND\n    \
         SINK_DC t\n    t.VDD -> VDD_5V\n    t.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::SINK_PIN_NO_DECOUPLING, &src);
    assert_eq!(
        msgs.len(),
        1,
        "only the fitted twin may fire — the unmounted part draws nothing; got codes: {codes:?}"
    );
    assert!(
        msgs[0].contains("main.t.VDD") && !msgs[0].contains("main.z.VDD"),
        "…and it is the fitted sink that fires, never the `nc` part: {msgs:?}"
    );
}

/// A terminal off the board draws from nothing: an unwired sink pad is a
/// floating-input matter, not a decoupling gap. The wired twin is what proves
/// the rule ran on this board.
#[test]
fn unwired_sink_pin_is_not_judged_6036() {
    let src = format!(
        "{SINK_DC}module main {{\n    {PI1_BOARD}\
         SINK_DC z\n    SINK_DC t\n    t.VDD -> VDD_5V\n    t.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::SINK_PIN_NO_DECOUPLING, &src);
    assert_eq!(
        msgs.len(),
        1,
        "only the wired twin may fire; got codes: {codes:?}"
    );
    assert!(
        msgs[0].contains("main.t.VDD"),
        "…and the unwired pad carries no verdict: {msgs:?}"
    );
}

/// §2.1's subject is the **load** terminal: the mirror-image source row on the
/// same pair owes nothing. The flip is the direction word alone, and the bare
/// sink beside it keeps the board's own verdict visible.
#[test]
fn source_row_of_the_same_shape_is_not_judged_6036() {
    let src = format!(
        "{SRC_DC}{SINK_DC}module main {{\n    {PI1_BOARD}\
         SRC_DC src\n    src.OUT -> VDD_3V3\n    src.GND -> GND\n    \
         SINK_DC t\n    t.VDD -> VDD_5V\n    t.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::SINK_PIN_NO_DECOUPLING, &src);
    assert_eq!(
        msgs.len(),
        1,
        "a source row of the same shape is not a load — only the sink twin may fire; got codes: {codes:?}"
    );
    assert!(
        msgs[0].contains("main.t.VDD"),
        "…and the verdict lands on the sink, never on the source: {msgs:?}"
    );
}

/// A row with no return half declares no pair, so there is nothing to decouple
/// across (§2.1's single-phase shape). Whether the row is captured as a sink
/// with an empty return or not captured as a pair at all, no pair means no
/// verdict — and the boxed sink beside it is what proves the rule ran.
#[test]
fn sink_row_without_a_declared_return_is_not_judged_6036() {
    let src = format!(
        "{SINK_SCALAR}{SINK_DC}module main {{\n    {PI1_BOARD}\
         SINK_SCALAR z\n    z.VDD -> VDD_3V3\n    \
         SINK_DC t\n    t.VDD -> VDD_5V\n    t.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::SINK_PIN_NO_DECOUPLING, &src);
    assert_eq!(
        msgs.len(),
        1,
        "a single-member row has no pair to be decoupled across; got codes: {codes:?}"
    );
    assert!(
        msgs[0].contains("main.t.VDD"),
        "…and the pair-carrying sink is the one that fires: {msgs:?}"
    );
}

/// §6 R2's second half: a module supply port is judged like a component's sink
/// pin. Both instances draw across the same declared pair through their own
/// port member, and the one capacitor sits on the parent's copper — so the two
/// verdicts also prove the boundary is transparent (a sub-module leg and the
/// parent's net are one node, §1.3's effective-class read).
#[test]
fn module_supply_port_is_judged_like_a_sink_pin_6036() {
    let src = format!(
        "{DC_IFACE}{SUB_PSNK}{CAP_DECOUP}module main {{\n    {PI1_BOARD}\
         SUB_PSNK u1\n    [VDD_3V3, GND] -> u1.dc\n    \
         SUB_PSNK u2\n    [VDD_5V, GND] -> u2.dc\n    \
         CAP_DECOUP ok\n    ok.1 -> VDD_5V\n    ok.2 -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::SINK_PIN_NO_DECOUPLING, &src);
    assert_eq!(
        msgs.len(),
        1,
        "the port of u1 draws across an undecoupled pair, u2's is covered by the parent's capacitor; got codes: {codes:?}"
    );
    assert!(
        msgs[0].contains("u1") && msgs[0].contains("VDD_3V3") && msgs[0].contains("GND"),
        "6036 must name the port terminal and the pair it draws across: {msgs:?}"
    );
}

// ── SN-3 sensitive return landing on a noisy face (power-quality-design.md §3.3, ruling 10) ──
//
// A part supplied from a quiet/sensitive face (§1.4: `@class(analog)`,
// `@noise(quiet)`, `@noise(sensitive)`) whose declared DC pair returns into a
// noisy one (`@noise(noisy)`): the plane a protected part returns to is part of
// its protection, so landing that return on a noise source's own reference puts
// the sensitive signal back onto the copper the quiet face was isolating it
// from. Error, and the harder of the §3 pair — SN-2 judges the *bridged*
// coupling, this the direct landing.
//
// The subject is a **declared pair of a part**: both members are read from one
// `pins.pwr` row, so the return judged is the return of the pair that was
// declared, and the two faces come from each net's own attribution against the
// words the declaring scopes wrote — the §1.4 read PI-2/PI-4/SN-2 share. A part
// whose definition declares no pair carries no witness here, which is where the
// seams are: a two-terminal passive's return placement is PI-3's 6038, and a
// *bridged* coupling between the two faces is SN-2's.
//
// Every silence below is a single-axis flip off the board that fires, and the
// firing twin stays on the board wherever the silence could otherwise pass for
// a rule that never ran.

/// The judged part: two declared supply pairs, one on a quiet face and one on a
/// digital face. The second row is what shows the rule reads the *pair* — its
/// hot member is on no face at all, so the same return net that convicts the
/// first row is innocent there.
const ANALOG_PART: &str = "component ANALOG_PART {\n    pins = [\n        \
                           psnk [1,2] = [AVDD, AGND]::DC(3.3V)\n        \
                           psnk [3,4] = [VDD, GND]::DC(3.3V)\n    ]\n}\n";

/// The same part with a single-member supply row: it closes over no return at
/// all, so there is no pair whose return could land anywhere.
const ANALOG_SCALAR: &str = "component ANALOG_SCALAR {\n    pins = [\n        \
                             psnk 1 = AVDD::DC(3.3V)\n    ]\n}\n";

/// The part's two pairs written twice each — two pin groups of one supply rail,
/// the BGA spelling. A defect of the rail is one defect, however many groups
/// carry it.
const ANALOG_TWOGROUP: &str = "component ANALOG_TWOGROUP {\n    pins = [\n        \
                               psnk [1,2] = [AVDD, AGND]::DC(3.3V)\n        \
                               psnk [3,4] = [AVDD, AGND]::DC(3.3V)\n    ]\n}\n";

/// §1.4's two faces as *words*: a quiet face declared `@noise(quiet)`, a noisy
/// one `@noise(noisy)`. `@class(analog)` is the third word of the same read and
/// the board below carries it; these two prove the read is the registered value
/// set, not one spelling the rule happens to know.
const SN3_WORDS_BOARD: &str = "conduit GND @role(main)\n    \
                               conduit GNDA @role(quiet)\n    \
                               domain AQ @noise(quiet) { rail [VDDA, GNDA]::DC(3.3V) }\n    \
                               domain AN @noise(noisy) { rail [VDD_3V3, GND]::DC(3.3V) }\n    ";

/// The board every case below flips one axis of: a quiet face with its own
/// reference (`@class(analog)` + `@role(quiet)` copper) beside a noisy one, each
/// carrying a declared rail so both nets resolve a world.
const SN3_BOARD: &str = "conduit GND @role(main)\n    \
                         conduit GNDA @role(quiet)\n    \
                         domain AVDD @class(analog) { rail [VDDA, GNDA]::DC(3.3V) }\n    \
                         domain DVDD @class(digital) @noise(noisy) { rail [VDD_3V3, GND]::DC(3.3V) }\n    ";

/// The judged shape: a part supplied from the quiet face returning through the
/// noisy one. Exactly **one** verdict — the second pair's hot member is on no
/// face, so its return is not this rule's object — and the message names the
/// part, the two faces and the pair, since that is the whole repair.
#[test]
fn sensitive_return_landing_on_the_noisy_face_fires_6041() {
    let src = format!(
        "{ANALOG_PART}module main {{\n    {SN3_BOARD}\
         ANALOG_PART u\n    u.AVDD -> VDDA\n    u.AGND -> GND\n    \
         u.VDD -> VDD_3V3\n    u.GND -> GND\n    \
         ANALOG_PART v\n    v.AVDD -> VDDA\n    v.AGND -> GNDA\n    \
         v.VDD -> VDD_3V3\n    v.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::SENSITIVE_RETURN_ON_NOISY, &src);
    assert_eq!(
        msgs.len(),
        1,
        "the part returning through the noisy face must fire 6041 exactly once, and the twin returning on the quiet reference must not; got codes: {codes:?}"
    );
    assert!(
        msgs[0].contains("main.u") && !msgs[0].contains("main.v"),
        "6041 must name the part whose return lands wrong: {msgs:?}"
    );
    assert!(
        msgs[0].contains("AVDD") && msgs[0].contains("DVDD"),
        "6041 must name both faces — the one being protected and the one violated: {msgs:?}"
    );
    assert!(
        msgs[0].contains("AGND") && msgs[0].contains("net 'GND'"),
        "6041 must name the return member and the net it landed on: {msgs:?}"
    );
}

/// §1.4's read is the registered value set, not one spelling: a quiet face
/// declared `@noise(quiet)` and one declared `@noise(sensitive)` each fire, and
/// the `@class(analog)` twin on the same board proves all three words are read
/// by one rule.
#[test]
fn every_quiet_word_makes_a_part_judgeable_6041() {
    let src = format!(
        "{ANALOG_PART}module main {{\n    {SN3_WORDS_BOARD}\
         ANALOG_PART u\n    u.AVDD -> VDDA\n    u.AGND -> GND\n    \
         u.VDD -> VDD_3V3\n    u.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::SENSITIVE_RETURN_ON_NOISY, &src);
    assert_eq!(
        msgs.len(),
        1,
        "@noise(quiet) is a quiet face like @class(analog); the return onto GND must fire once; got codes: {codes:?}"
    );
    assert!(
        msgs[0].contains("AQ"),
        "…and the message names the world the declaration gave it: {msgs:?}"
    );
}

/// The quiet side is the witness: a part whose supply comes from a face no word
/// marks — the digital pair — returns onto the very noisy net that convicts the
/// quiet pair beside it, and is not judged.
#[test]
fn supply_from_a_faceless_domain_returning_to_noise_is_not_judged_6041() {
    let src = format!(
        "{ANALOG_PART}module main {{\n    {SN3_BOARD}\
         ANALOG_PART z\n    z.AVDD -> VDD_3V3\n    z.AGND -> GND\n    \
         z.VDD -> VDD_3V3\n    z.GND -> GND\n    \
         ANALOG_PART u\n    u.AVDD -> VDDA\n    u.AGND -> GND\n    \
         u.VDD -> VDD_3V3\n    u.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::SENSITIVE_RETURN_ON_NOISY, &src);
    assert_eq!(
        msgs.len(),
        1,
        "a digital supply returning onto the noisy net is not a protected part; got codes: {codes:?}"
    );
    assert!(
        msgs[0].contains("main.u"),
        "…only the quiet-faced part is judged: {msgs:?}"
    );
}

/// A return that reaches no declared face is not judged: the noisy side of the
/// pair is a declaration too (§1.3 — silence, never a guess). The wire is the
/// only flip, and the firing twin proves the rule ran on this board.
#[test]
fn return_landing_on_an_undeclared_net_is_not_judged_6041() {
    let src = format!(
        "{ANALOG_PART}module main {{\n    {SN3_BOARD}\
         ANALOG_PART z\n    z.AVDD -> VDDA\n    z.AGND -> FLOATY\n    \
         z.VDD -> VDD_3V3\n    z.GND -> GND\n    \
         ANALOG_PART u\n    u.AVDD -> VDDA\n    u.AGND -> GND\n    \
         u.VDD -> VDD_3V3\n    u.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::SENSITIVE_RETURN_ON_NOISY, &src);
    assert_eq!(
        msgs.len(),
        1,
        "a net no scope declares anchors no face, so the return onto it takes no verdict; got codes: {codes:?}"
    );
    assert!(
        msgs[0].contains("main.u"),
        "…and the part returning onto the declared noisy net is the one judged: {msgs:?}"
    );
}

/// A row with no return half declares no pair, so there is no return to land
/// anywhere (the single-phase AC shape, axis ④'s object). The pair-carrying
/// twin beside it keeps the board's verdict visible.
#[test]
fn pair_without_a_return_member_is_not_judged_6041() {
    let src = format!(
        "{ANALOG_SCALAR}{ANALOG_PART}module main {{\n    {SN3_BOARD}\
         ANALOG_SCALAR z\n    z.AVDD -> VDDA\n    \
         ANALOG_PART u\n    u.AVDD -> VDDA\n    u.AGND -> GND\n    \
         u.VDD -> VDD_3V3\n    u.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::SENSITIVE_RETURN_ON_NOISY, &src);
    assert_eq!(
        msgs.len(),
        1,
        "a single-member supply row closes over no return; got codes: {codes:?}"
    );
    assert!(
        msgs[0].contains("main.u"),
        "…and the pair-carrying part is the one judged: {msgs:?}"
    );
}

/// A return member that reaches no net (an unwired pad) is located nowhere, so
/// the pair has no second leg to judge — a floating-input matter, not this
/// rule's. The wired twin proves the rule ran.
#[test]
fn unwired_return_member_is_not_judged_6041() {
    let src = format!(
        "{ANALOG_PART}module main {{\n    {SN3_BOARD}\
         ANALOG_PART z\n    z.AVDD -> VDDA\n    \
         z.VDD -> VDD_3V3\n    z.GND -> GND\n    \
         ANALOG_PART u\n    u.AVDD -> VDDA\n    u.AGND -> GND\n    \
         u.VDD -> VDD_3V3\n    u.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::SENSITIVE_RETURN_ON_NOISY, &src);
    assert_eq!(
        msgs.len(),
        1,
        "an unwired return member lands on no net, so no pair is judged; got codes: {codes:?}"
    );
    assert!(
        msgs[0].contains("main.u"),
        "…and the wired part is the one judged: {msgs:?}"
    );
}

/// One rail, two pin groups: the defect is the rail's, so it is reported once,
/// not once per group. Both returns land on the noisy net and the message names
/// the pair, not the group.
#[test]
fn duplicated_pair_rows_report_once_6041() {
    let src = format!(
        "{ANALOG_TWOGROUP}module main {{\n    {SN3_BOARD}\
         ANALOG_TWOGROUP u\n    u.AVDD -> VDDA\n    u.AGND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::SENSITIVE_RETURN_ON_NOISY, &src);
    assert_eq!(
        msgs.len(),
        1,
        "two pin groups of one declared rail are one declared pair, hence one verdict; got codes: {codes:?}"
    );
}

/// The seam with PI-3: a two-terminal passive has no declared supply row, so it
/// carries no witness here — and the very same mis-landed return is exactly what
/// 6038 judges. The capacitor is the flip; the part beside it keeps 6041 armed,
/// and the second capacitor is what puts `GNDA` on the board at all (a rail
/// whose return member names no net is no pair to compare against).
#[test]
fn two_terminal_part_across_the_faces_is_pi3_not_sn3() {
    let src = format!(
        "{ANALOG_PART}{CAP_DECOUP}module main {{\n    {SN3_BOARD}\
         CAP_DECOUP c\n    c.1 -> VDDA\n    c.2 -> GND\n    \
         CAP_DECOUP k\n    k.1 -> VDDA\n    k.2 -> GNDA\n    \
         ANALOG_PART u\n    u.AVDD -> VDDA\n    u.AGND -> GND\n    \
         u.VDD -> VDD_3V3\n    u.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::SENSITIVE_RETURN_ON_NOISY, &src);
    assert_eq!(
        msgs.len(),
        1,
        "a two-terminal passive declares no supply pair, so it is never this rule's subject; got codes: {codes:?}"
    );
    assert!(
        msgs[0].contains("main.u"),
        "…and only the part with a declared pair is judged: {msgs:?}"
    );
    assert!(
        codes.contains(&mcc::errcodes::DECOUPLING_RETURN_MISMATCH),
        "…while the capacitor's own mis-landed return is 6038's verdict, not a second 6041; got codes: {codes:?}"
    );
}

/// A board whose scopes declare neither face asks this rule nothing, whatever
/// its wiring looks like: the faces are declarations, so with none of them there
/// is no protected part to speak of, and the board says so by not being judged.
#[test]
fn board_with_no_declared_face_is_not_judged_6041() {
    let src = format!(
        "{ANALOG_PART}module main {{\n    \
         conduit GND @role(main)\n    \
         domain AVDD @class(analog) {{ rail [VDDA, GNDA]::DC(3.3V) }}\n    \
         conduit GNDA @role(quiet)\n    \
         ANALOG_PART u\n    u.AVDD -> VDDA\n    u.AGND -> GND\n    \
         u.VDD -> VDD_3V3\n    u.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::SENSITIVE_RETURN_ON_NOISY, &src);
    assert!(
        msgs.is_empty(),
        "with no noisy face declared there is no violated face, so the return onto GND is not a finding; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::SENSITIVE_RETURN_ON_NOISY),
        "…and the rule stays silent rather than guessing a face; got codes: {codes:?}"
    );
}

// ── SN-1 analog signal crossing a split ground (power-quality-design.md §3.1) ──
//
// A port row that claims the quiet/sensitive face (§1.4) and names its
// reference with `@return(C)` states the plane the scope's analog signals are
// measured against, so the parts that face supplies must return over that
// reference. §3.1 drafts this as the sink-side part; the flat table carries no
// source→sink chain for a signal net (model A), so the part is found by what
// supplies it — the same face read PI-2/PI-4/SN-2/SN-3 use — and the subject is
// then fixed by two agreeing declarations: the face's rail and the port's
// `@return` must name the same reference. What is measured is the *topology*:
// the effective class of the net the part's return member lands on.
//
// Every silence below is a single-axis flip off the board that fires, and the
// firing twin stays on the board wherever the silence could otherwise pass for
// a rule that never ran.

/// The board every case flips one axis of: a quiet face with its own reference
/// (`@class(analog)` + `@role(quiet)` copper) beside a digital one, each
/// carrying a declared rail so both nets resolve a world.
const SN1_BOARD: &str = "conduit GND @role(main)\n    \
                         conduit GNDA @role(quiet)\n    \
                         domain AVDD @class(analog) { rail [VDDA, GNDA]::DC(3.3V) }\n    \
                         domain DVDD @class(digital) { rail [VDD_3V3, GND]::DC(3.3V) }\n    ";

/// The declaration that binds the face: an analog port row naming the reference
/// the face's own rail states.
const SN1_PORT: &str = "io MIC{P, N} @class(analog) @return(GNDA)\n    ";

/// The judged shape: a part drawing from the analog face and returning over
/// another reference, beside a twin whose return closes on the declared one.
/// Exactly **one** verdict — the part's *second* pair is supplied from the
/// digital face, so its return onto the same net is not this rule's object —
/// and the message names the part, the face, the landing, the declaring scope
/// and the declared reference, since that is the whole repair.
#[test]
fn analog_face_return_landing_elsewhere_fires_6039() {
    let src = format!(
        "{ANALOG_PART}module main {{\n    {SN1_BOARD}{SN1_PORT}\
         ANALOG_PART u\n    u.AVDD -> VDDA\n    u.AGND -> GND\n    \
         u.VDD -> VDD_3V3\n    u.GND -> GND\n    \
         ANALOG_PART v\n    v.AVDD -> VDDA\n    v.AGND -> GNDA\n    \
         v.VDD -> VDD_3V3\n    v.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::ANALOG_RETURN_MISMATCH, &src);
    assert_eq!(
        msgs.len(),
        1,
        "the part returning off the declared reference must fire 6039 exactly once, and neither the honoured twin nor its own digital-supplied pair may; got codes: {codes:?}"
    );
    assert!(
        msgs[0].contains("main.u") && !msgs[0].contains("main.v"),
        "6039 must name the part whose return lands wrong: {msgs:?}"
    );
    assert!(
        msgs[0].contains("AVDD"),
        "…and the face that part draws from: {msgs:?}"
    );
    assert!(
        msgs[0].contains("'GND'") && msgs[0].contains("GNDA"),
        "…and both references — the one landed on and the one declared: {msgs:?}"
    );
    assert!(
        msgs[0].contains("scope 'main'"),
        "…and the scope whose declaration it contradicts: {msgs:?}"
    );
}

/// §1.4's read is the registered value set, not one spelling: a port row
/// claiming the face with `@noise(quiet)` or `@noise(sensitive)` arms the rule
/// exactly as `@class(analog)` does (proved in the case above).
#[test]
fn every_quiet_word_on_the_port_row_arms_the_rule_6039() {
    for (word, row) in [
        (
            "@noise(quiet)",
            "io MIC{P, N} @noise(quiet) @return(GNDA)\n    ",
        ),
        (
            "@noise(sensitive)",
            "io MIC{P, N} @noise(sensitive) @return(GNDA)\n    ",
        ),
    ] {
        let src = format!(
            "{ANALOG_PART}module main {{\n    {SN1_BOARD}{row}\
             ANALOG_PART u\n    u.AVDD -> VDDA\n    u.AGND -> GND\n    \
             u.VDD -> VDD_3V3\n    u.GND -> GND\n}}\n"
        );
        let codes = build_codes(&src);
        let msgs = msgs_of(mcc::errcodes::ANALOG_RETURN_MISMATCH, &src);
        assert_eq!(
            msgs.len(),
            1,
            "{word} claims the quiet face like @class(analog), so the return onto GND must fire once; got codes: {codes:?}"
        );
    }
}

/// A port row stating no reference declares nothing to contradict (the design's
/// deferred half, §6 R3), and the same board with the `@return` written back is
/// the flip that shows the rule read the row.
#[test]
fn port_row_without_a_return_is_not_judged_6039() {
    let board = |port: &str| {
        format!(
            "{ANALOG_PART}module main {{\n    {SN1_BOARD}{port}\
             ANALOG_PART u\n    u.AVDD -> VDDA\n    u.AGND -> GND\n    \
             u.VDD -> VDD_3V3\n    u.GND -> GND\n}}\n"
        )
    };
    let silent = board("io MIC{P, N} @class(analog)\n    ");
    let codes = build_codes(&silent);
    assert!(
        msgs_of(mcc::errcodes::ANALOG_RETURN_MISMATCH, &silent).is_empty(),
        "with no @return the scope declares no reference, so no return can miss it; got codes: {codes:?}"
    );
    let fires = board(SN1_PORT);
    assert_eq!(
        msgs_of(mcc::errcodes::ANALOG_RETURN_MISMATCH, &fires).len(),
        1,
        "…and the same board fires once the row names the reference; got codes: {:?}",
        build_codes(&fires)
    );
}

/// A port row claiming neither quiet word takes no part in the read: the words
/// are the declaration, so a `@class(digital)` row's `@return` states no analog
/// face, and the same board flips the row to `@class(analog)`.
#[test]
fn port_row_claiming_no_quiet_word_is_not_judged_6039() {
    let board = |port: &str| {
        format!(
            "{ANALOG_PART}module main {{\n    {SN1_BOARD}{port}\
             ANALOG_PART u\n    u.AVDD -> VDDA\n    u.AGND -> GND\n    \
             u.VDD -> VDD_3V3\n    u.GND -> GND\n}}\n"
        )
    };
    let silent = board("io MIC{P, N} @class(digital) @return(GNDA)\n    ");
    let codes = build_codes(&silent);
    assert!(
        msgs_of(mcc::errcodes::ANALOG_RETURN_MISMATCH, &silent).is_empty(),
        "a row claiming the digital face states no analog reference; got codes: {codes:?}"
    );
    let fires = board(SN1_PORT);
    assert_eq!(
        msgs_of(mcc::errcodes::ANALOG_RETURN_MISMATCH, &fires).len(),
        1,
        "…and the same row claiming the analog face fires; got codes: {:?}",
        build_codes(&fires)
    );
}

/// The verdict is **per face**: a scope declaring two quiet faces resolves each
/// against its own reference, so a port naming the first face's reference never
/// judges the second face's parts — and a part that returns over the *other*
/// face's reference is convicted by its own face, not let through.
#[test]
fn two_quiet_faces_in_one_scope_are_judged_against_their_own_reference_6039() {
    let src = format!(
        "{ANALOG_PART}module main {{\n    \
         conduit GND  @role(main)\n    \
         conduit GNDA @role(quiet)\n    \
         conduit GNDB @role(quiet)\n    \
         domain AVDD @class(analog) {{ rail [VDDA, GNDA]::DC(3.3V) }}\n    \
         domain BVDD @class(analog) {{ rail [VDDB, GNDB]::DC(3.3V) }}\n    \
         domain CVDD @class(analog) {{ rail [VDDC, GNDC]::DC(3.3V) }}\n    \
         io MIC{{P, N}} @class(analog) @return(GNDA)\n    \
         io AUX{{P, N}} @class(analog) @return(GNDB)\n    \
         ANALOG_PART u\n    u.AVDD -> VDDA\n    u.AGND -> GND\n    \
         u.VDD -> VDD_3V3\n    u.GND -> GND\n    \
         ANALOG_PART w\n    w.AVDD -> VDDB\n    w.AGND -> GND\n    \
         w.VDD -> VDD_3V3\n    w.GND -> GND\n    \
         ANALOG_PART x\n    x.AVDD -> VDDC\n    x.AGND -> GND\n    \
         x.VDD -> VDD_3V3\n    x.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::ANALOG_RETURN_MISMATCH, &src);
    assert_eq!(
        msgs.len(),
        2,
        "the part on GNDA's face and the one on GNDB's face are each convicted by their own face's reference, while the face no port declares reference for is silent; got codes: {codes:?}"
    );
    let joined = msgs.join("\n");
    assert!(
        joined.contains("main.u") && joined.contains("main.w") && !joined.contains("main.x"),
        "…and the part whose face reference no port declares is not judged: {msgs:?}"
    );
    assert!(
        msgs.iter()
            .any(|m| m.contains("AVDD") && m.contains("GNDA"))
            && msgs
                .iter()
                .any(|m| m.contains("BVDD") && m.contains("GNDB")),
        "…each report naming its own face and that face's reference: {msgs:?}"
    );
}

/// The supply half is the witness: a part drawing from a face no word marks —
/// the digital pair — returns onto the very net that convicts the analog pair
/// beside it, and is not judged.
#[test]
fn supply_from_a_faceless_domain_is_not_judged_6039() {
    let src = format!(
        "{ANALOG_PART}module main {{\n    {SN1_BOARD}{SN1_PORT}\
         ANALOG_PART z\n    z.AVDD -> VDD_3V3\n    z.AGND -> GND\n    \
         z.VDD -> VDD_3V3\n    z.GND -> GND\n    \
         ANALOG_PART u\n    u.AVDD -> VDDA\n    u.AGND -> GND\n    \
         u.VDD -> VDD_3V3\n    u.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::ANALOG_RETURN_MISMATCH, &src);
    assert_eq!(
        msgs.len(),
        1,
        "a part fed from the digital face belongs to no analog face; got codes: {codes:?}"
    );
    assert!(
        msgs[0].contains("main.u"),
        "…only the part on the declared analog face is judged: {msgs:?}"
    );
}

/// A return that reaches no declared class is not judged (§1.3 — silence, never
/// a guess). The wire is the only flip, and the firing twin proves the rule ran
/// on this board.
#[test]
fn return_landing_on_an_undeclared_net_is_not_judged_6039() {
    let src = format!(
        "{ANALOG_PART}module main {{\n    {SN1_BOARD}{SN1_PORT}\
         ANALOG_PART z\n    z.AVDD -> VDDA\n    z.AGND -> FLOATY\n    \
         z.VDD -> VDD_3V3\n    z.GND -> GND\n    \
         ANALOG_PART u\n    u.AVDD -> VDDA\n    u.AGND -> GND\n    \
         u.VDD -> VDD_3V3\n    u.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::ANALOG_RETURN_MISMATCH, &src);
    assert_eq!(
        msgs.len(),
        1,
        "a net no scope declares resolves no class, so the return onto it takes no verdict; got codes: {codes:?}"
    );
    assert!(
        msgs[0].contains("main.u"),
        "…and the part returning onto the declared reference's competitor is the one judged: {msgs:?}"
    );
}

/// A row with no return half declares no pair, so there is no return to land
/// anywhere (the single-phase AC shape, axis ④'s object). The pair-carrying
/// twin beside it keeps the board's verdict visible.
#[test]
fn pair_without_a_return_member_is_not_judged_6039() {
    let src = format!(
        "{ANALOG_SCALAR}{ANALOG_PART}module main {{\n    {SN1_BOARD}{SN1_PORT}\
         ANALOG_SCALAR z\n    z.AVDD -> VDDA\n    \
         ANALOG_PART u\n    u.AVDD -> VDDA\n    u.AGND -> GND\n    \
         u.VDD -> VDD_3V3\n    u.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::ANALOG_RETURN_MISMATCH, &src);
    assert_eq!(
        msgs.len(),
        1,
        "a single-member supply row closes over no return; got codes: {codes:?}"
    );
    assert!(
        msgs[0].contains("main.u"),
        "…and the pair-carrying part is the one judged: {msgs:?}"
    );
}

/// Two pin groups of one supply rail are one pair, so a defect of the rail is
/// one defect however many groups carry it.
#[test]
fn duplicated_pair_rows_report_once_6039() {
    let src = format!(
        "{ANALOG_TWOGROUP}module main {{\n    {SN1_BOARD}{SN1_PORT}\
         ANALOG_TWOGROUP u\n    u.AVDD -> VDDA\n    u.AGND -> GND\n    \
         u.VDD -> VDD_3V3\n    u.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::ANALOG_RETURN_MISMATCH, &src);
    assert_eq!(
        msgs.len(),
        1,
        "the same declared pair written twice is one defect; got codes: {codes:?}"
    );
}

/// A board whose scopes declare no analog port row asks this rule nothing,
/// whatever its wiring looks like: the reference is a declaration, so with none
/// of them there is nothing for a return to miss.
#[test]
fn board_with_no_analog_port_is_not_judged_6039() {
    let src = format!(
        "{ANALOG_PART}module main {{\n    {SN1_BOARD}\
         ANALOG_PART u\n    u.AVDD -> VDDA\n    u.AGND -> GND\n    \
         u.VDD -> VDD_3V3\n    u.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::ANALOG_RETURN_MISMATCH, &src);
    assert!(
        msgs.is_empty(),
        "the face is declared but no port states its reference, so no return can miss it; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::ANALOG_RETURN_MISMATCH),
        "…and the rule stays silent rather than guessing a reference; got codes: {codes:?}"
    );
}

/// A quiet face whose `::DC` rail is missing declares no pair, so there is no
/// reference of its own to check against — a face declared by word alone is
/// silent, and the twin face carrying its rail keeps the board's verdict visible.
#[test]
fn quiet_face_without_a_rail_is_not_judged_6039() {
    let src = format!(
        "{ANALOG_PART}module main {{\n    {SN1_BOARD}\
         domain RAILESS @class(analog) {{ rail [VDDR, GNDR]::AC(3.3V) }}\n    \
         io MIC{{P, N}} @class(analog) @return(GNDA)\n    \
         io AUX{{P, N}} @class(analog) @return(GNDR)\n    \
         ANALOG_PART z\n    z.AVDD -> VDDR\n    z.AGND -> GND\n    \
         z.VDD -> VDD_3V3\n    z.GND -> GND\n    \
         ANALOG_PART u\n    u.AVDD -> VDDA\n    u.AGND -> GND\n    \
         u.VDD -> VDD_3V3\n    u.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::ANALOG_RETURN_MISMATCH, &src);
    assert_eq!(
        msgs.len(),
        1,
        "an AC rail states no DC pair, so the face it sits in declares no return member to miss; got codes: {codes:?}"
    );
    assert!(
        msgs[0].contains("main.u"),
        "…and the DC-rail face beside it is the one judged: {msgs:?}"
    );
}

// ── SN-2: noisy/quiet returns joined by one DC ground bridge (§3.2, ruling 9) ──
//
// A declared `@bridge` whose two ends are the **returns** of a noisy face (§1.4
// `@noise(noisy)`) and of a quiet/sensitive one is the two references meeting
// through plain copper: the filter — a magnetic element on the leg — is what lets
// a quiet face keep its own reference while the two coppers meet, so without one
// the plane the protected parts are measured against sits on the noise source's
// return. "No filtering intent" is ruling 9's reading (a) negated (2026-09-16),
// read from the leg's element class (the axle PI-2 also reads), never from a name.
//
// The ground side is PI-2's supply-leg test **negated**: a bridge with a rail's
// hot member at either end is the supply filter leg 6037 judges, so the two rules
// cut the corpus without both claiming a leg. Both ends are read at the name
// level — the declaring scope's own rails say what each written name is — which
// is what lets the design's plainest form, a direct copper tie, be judged at all.
// Every silence below is a single-axis flip off the board that fires, and the
// firing twin stays on the board wherever the silence could otherwise pass for a
// rule that never ran.
//
// §3.2's vacuous-truth risk (its samples are synthetic: no domain in the corpus
// `@noise(noisy)`, and the golden `FB_agnd` ground legs join two non-noisy
// returns) is why the judged board below adds the noise word to the golden
// pwrint shape — the acceptance of this rule is synthetic-only, recorded in the
// batch ledger.

/// The judged board: a noisy digital face returning on GND, a quiet analog face
/// returning on GNDA. The two loads are part of the board, not decoration: a rail
/// member becomes a net — and so a class the leg's carrier can be matched
/// against — only once something wires it.
const SN2_BOARD: &str = "conduit GND @role(main)\n    \
                         conduit GNDA @role(quiet)\n    \
                         domain DVDD @class(digital) @noise(noisy) { rail [VDD_3V3, GND]::DC(3.3V) }\n    \
                         domain AVDD @class(analog) { rail [VDDA, GNDA]::DC(3.3V) }\n    \
                         RES_TIE ld1\n    ld1.1 -> VDD_3V3\n    ld1.2 -> GND\n    \
                         RES_TIE ld2\n    ld2.1 -> VDDA\n    ld2.2 -> GNDA\n    ";

/// The declared filter element: a magnetic two-terminal part. Ruling 9's reading
/// (a) takes the class off the definition's spec keys, so this is what "with
/// filtering intent" means on the leg — not the part's name, which the rule never
/// reads.
const TIE_MAG: &str =
    "component TIE_MAG {\n    pins = [ io [1:2] = [P, N] ]\n    spec = [ inductance = 1uH ]\n}\n";

/// The judged shape: a resistor tying the noisy face's return to the quiet
/// face's. The message names the clause as written, both faces it joins and the
/// carrier standing in for the missing filter — the whole repair.
#[test]
fn noisy_and_quiet_returns_joined_by_a_resistor_fire_6040() {
    let src = format!(
        "{RES_TIE}module main {{\n    {SN2_BOARD}\
         GNDA - t::RES_TIE() - GND @bridge(GND, GNDA)\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::SHARED_RETURN_BRIDGE, &src);
    assert_eq!(
        msgs.len(),
        1,
        "a declared ground bridge between a noisy face's return and a quiet face's, carried by no magnetic element, must fire 6040 exactly once; got codes: {codes:?}"
    );
    assert!(
        msgs[0].contains("GND <-> GNDA"),
        "6040 must name the bridge clause as written: {msgs:?}"
    );
    assert!(
        msgs[0].contains("DVDD") && msgs[0].contains("AVDD"),
        "…and both faces it joins: {msgs:?}"
    );
    assert!(
        msgs[0].contains("main.t"),
        "…and the carrier standing in for the missing filter: {msgs:?}"
    );
}

/// The single-axis flip that proves the element class is what the rule read: the
/// same board with a magnetic part on the leg is the declared filter, and whether
/// that filter is complete is PI-2's verdict (ruling 11's partition) — not this
/// rule's.
#[test]
fn magnetic_carrier_is_the_declared_filter_6040() {
    let src = format!(
        "{TIE_MAG}module main {{\n    {SN2_BOARD}\
         GNDA - t::TIE_MAG() - GND @bridge(GND, GNDA)\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::SHARED_RETURN_BRIDGE, &src);
    assert!(
        msgs.is_empty(),
        "a magnetic element on the leg is filtering intent — the rule reports the tie, not the filter's adequacy; got codes: {codes:?}"
    );
}

/// §3.2's subject is the DC ground bridge: a `@couple` is a DC-blocking coupling
/// element, not a tie. The flip is the relation word alone.
#[test]
fn couple_edge_is_not_judged_6040() {
    let src = format!(
        "{RES_TIE}module main {{\n    {SN2_BOARD}\
         GNDA - t::RES_TIE() - GND @couple(GND, GNDA)\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::SHARED_RETURN_BRIDGE, &src);
    assert!(
        msgs.is_empty(),
        "a @couple edge is not a DC ground tie; got codes: {codes:?}"
    );
}

/// The same test the other way (PI-2's leg): a bridge with a rail's hot member at
/// either end is the supply filter leg 6037 judges, so this rule stays off it —
/// and 6037 firing on the same board is the twin proving the board ran.
#[test]
fn supply_leg_bridge_is_pi2_not_sn2() {
    let src = format!(
        "{RES_TIE}module main {{\n    {SN2_BOARD}\
         VDD_3V3 - t::RES_TIE() - VDDA @bridge(VDD_3V3, VDDA)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::SHARED_RETURN_BRIDGE),
        "a bridge with a rail's hot member at an end is the supply filter leg, not a ground tie; got codes: {codes:?}"
    );
    assert!(
        codes.contains(&mcc::errcodes::BRIDGE_LOAD_DECOUPLING_MISSING),
        "…and PI-2 is the rule that owns it, on this very board; got codes: {codes:?}"
    );
}

/// With no noisy face declared there is no reference for the quiet one to be
/// shared with — the golden corpus's own reason for silence (§3.2's vacuous truth).
/// The flip is one word on DVDD's domain row, and the firing case above is the
/// board that carries it.
#[test]
fn bridge_with_no_noisy_side_is_not_judged_6040() {
    let src = format!(
        "{RES_TIE}module main {{\n    conduit GND @role(main)\n    conduit GNDA @role(quiet)\n    \
         domain DVDD @class(digital) {{ rail [VDD_3V3, GND]::DC(3.3V) }}\n    \
         domain AVDD @class(analog) {{ rail [VDDA, GNDA]::DC(3.3V) }}\n    \
         RES_TIE ld1\n    ld1.1 -> VDD_3V3\n    ld1.2 -> GND\n    \
         RES_TIE ld2\n    ld2.1 -> VDDA\n    ld2.2 -> GNDA\n    \
         GNDA - t::RES_TIE() - GND @bridge(GND, GNDA)\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::SHARED_RETURN_BRIDGE, &src);
    assert!(
        msgs.is_empty(),
        "with no noisy face there is nothing for the quiet face's reference to be shared with; got codes: {codes:?}"
    );
}

/// Two noisy returns are two faces of one kind: the flip is a second
/// `@noise(noisy)` domain, and there is no protected side left undecided but
/// unjudged — silence, not a guess.
#[test]
fn bridge_between_two_noisy_returns_is_not_judged_6040() {
    let src = format!(
        "{RES_TIE}module main {{\n    {SN2_BOARD}\
         conduit GNDB\n    \
         domain DVDD2 @class(digital) @noise(noisy) {{ rail [VDD_5V, GNDB]::DC(5V) }}\n    \
         RES_TIE ld3\n    ld3.1 -> VDD_5V\n    ld3.2 -> GNDB\n    \
         GNDB - t::RES_TIE() - GND @bridge(GND, GNDB)\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::SHARED_RETURN_BRIDGE, &src);
    assert!(
        msgs.is_empty(),
        "two noisy returns share one kind of face, so no quiet reference is at stake; got codes: {codes:?}"
    );
}

/// …and two quiet returns are the same flip the other way: which side is the
/// noisy one is undecidable, so the pair is not judged.
#[test]
fn bridge_between_two_quiet_returns_is_not_judged_6040() {
    let src = format!(
        "{RES_TIE}module main {{\n    {SN2_BOARD}\
         conduit GNDB @role(quiet)\n    \
         domain AVDD2 @class(analog) {{ rail [VDDA2, GNDB]::DC(3.3V) }}\n    \
         RES_TIE ld3\n    ld3.1 -> VDDA2\n    ld3.2 -> GNDB\n    \
         GNDB - t::RES_TIE() - GNDA @bridge(GNDA, GNDB)\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::SHARED_RETURN_BRIDGE, &src);
    assert!(
        msgs.is_empty(),
        "two quiet returns name no noise source this bridge would short past a filter; got codes: {codes:?}"
    );
}

/// A name both faces return on answers neither question on its own: the flip is
/// one more analog domain returning on the noisy GND, which is the design's own
/// case (a quiet face whose return *is* the noisy copper) — the later half of
/// §3.1, not this rule's subject.
#[test]
fn bridge_to_a_net_two_faces_return_on_is_not_judged_6040() {
    let src = format!(
        "{RES_TIE}module main {{\n    conduit GND @role(main)\n    conduit GNDA @role(quiet)\n    \
         domain DVDD @class(digital) @noise(noisy) {{ rail [VDD_3V3, GND]::DC(3.3V) }}\n    \
         domain AVDD @class(analog) {{ rail [VDDA, GNDA]::DC(3.3V) }}\n    \
         domain AVDDR @class(analog) {{ rail [VDDR, GND]::DC(3.3V) }}\n    \
         RES_TIE ld1\n    ld1.1 -> VDD_3V3\n    ld1.2 -> GND\n    \
         RES_TIE ld2\n    ld2.1 -> VDDA\n    ld2.2 -> GNDA\n    \
         RES_TIE ld3\n    ld3.1 -> VDDR\n    ld3.2 -> GND\n    \
         GNDA - t::RES_TIE() - GND @bridge(GND, GNDA)\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::SHARED_RETURN_BRIDGE, &src);
    assert!(
        msgs.is_empty(),
        "a name two faces return on names no single side, so the pair is not judged; got codes: {codes:?}"
    );
}

/// A clause endpoint no rail of its scope writes declares no return this rule can
/// read — §1.3's silence, and the firing case above is the same board with the
/// name written back.
#[test]
fn bridge_endpoint_naming_no_rail_member_is_not_judged_6040() {
    let src = format!(
        "{RES_TIE}module main {{\n    {SN2_BOARD}\
         GNDA - t::RES_TIE() - GND @bridge(GND, GNDX)\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::SHARED_RETURN_BRIDGE, &src);
    assert!(
        msgs.is_empty(),
        "an endpoint no declared rail writes is not judged, never guessed; got codes: {codes:?}"
    );
}

/// A third face whose reference is a net of its own still joins the rule: the
/// declared return `GNDR` tied to the noisy GND is §3.2's defect, and it is the
/// twin that proves the silence below is the copper talking.
#[test]
fn a_third_quiet_face_tied_to_noise_fires_6040() {
    let src = format!(
        "{RES_TIE}module main {{\n    {SN2_BOARD}\
         domain RAILESS @class(analog) {{ rail [VDD_R, GNDR]::DC(3.3V) }}\n    \
         GND - t::RES_TIE() - GNDR @bridge(GND, GNDR)\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::SHARED_RETURN_BRIDGE, &src);
    assert_eq!(
        msgs.len(),
        1,
        "a quiet face returning on its own reference, tied to the noisy return by a resistor, is the same defect; got codes: {codes:?}"
    );
    assert!(
        msgs[0].contains("RAILESS") && msgs[0].contains("DVDD"),
        "…and the message names both faces: {msgs:?}"
    );
}

/// …and a declared return whose copper is not there is one step of silence
/// later: the name is a rail member but reaches no class, so the leg cannot be
/// located at all. The flip against the case above is the copper the clause's
/// second name never meets.
#[test]
fn bridge_endpoint_whose_class_does_not_resolve_is_not_judged_6040() {
    let src = format!(
        "{RES_TIE}module main {{\n    {SN2_BOARD}\
         domain RAILESS @class(analog) {{ rail [VDD_R, GNDR]::DC(3.3V) }}\n    \
         GNDA - t::RES_TIE() - GND @bridge(GND, GNDR)\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::SHARED_RETURN_BRIDGE, &src);
    assert!(
        msgs.is_empty(),
        "a declared return whose net reaches no class leaves the leg unlocatable; got codes: {codes:?}"
    );
}

/// The plainest form of the defect: the clause declares the tie and no element
/// carries it at all. (A *bare copper* tie — `GND - GNDA` — is not expressible
/// on this surface: every copper join either goes through a component or is
/// read as a class reference, so the two forms this branch is exercised by are
/// this one and the chain below.)
#[test]
fn bridge_no_element_carries_is_judged_6040() {
    let src = format!(
        "{RES_TIE}module main {{\n    {SN2_BOARD}\
         GND - t::RES_TIE() - GNDB @bridge(GND, GNDA)\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::SHARED_RETURN_BRIDGE, &src);
    assert_eq!(
        msgs.len(),
        1,
        "a declared ground bridge between two returns, carried by no element, must fire 6040; got codes: {codes:?}"
    );
    assert!(
        msgs[0].contains("carried by no single two-terminal element"),
        "…and the message must say no element carries the leg: {msgs:?}"
    );
}

/// …and a leg carried by a chain of elements, none of which spans it: the design
/// reads the bridge's *own* element (ruling 9's bridge carrier), so a chain carries no
/// magnetic bridge element either — reported the same way, with the same wording.
#[test]
fn chained_leg_with_no_magnetic_element_is_judged_6040() {
    let src = format!(
        "{RES_TIE}module main {{\n    {SN2_BOARD}\
         GND - r1::RES_TIE() - X - r2::RES_TIE() - GNDA @bridge(GND, GNDA)\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::SHARED_RETURN_BRIDGE, &src);
    assert_eq!(
        msgs.len(),
        1,
        "a ground bridge carried by elements none of which is the bridge's own must fire 6040 once; got codes: {codes:?}"
    );
    assert!(
        msgs[0].contains("carried by no single two-terminal element"),
        "…and the message must say no single element carries the leg: {msgs:?}"
    );
}

/// Two parallel legs between the same two returns, each with its own clause: the
/// magnetic element is the *declared filter's* leg, and the resistor's leg owns
/// no filter of its own. Reading the pair alone, one filtered leg would answer
/// for its unfiltered twin and the message would name an arbitrary element of the
/// two — so the clause's own span picks the carrier out (the per-leg match 6022
/// makes). This is the golden board's own shape: pwrint's `FB_agnd`/`FB_agnd2`
/// stand as two parallel ground legs.
#[test]
fn parallel_legs_each_read_their_own_carrier_6040() {
    let src = format!(
        "{RES_TIE}{TIE_MAG}module main {{\n    {SN2_BOARD}\
         GNDA - bad::RES_TIE() - GND @bridge(GND, GNDA)\n    \
         GNDA - good::TIE_MAG() - GND @bridge(GND, GNDA)\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::SHARED_RETURN_BRIDGE, &src);
    assert_eq!(
        msgs.len(),
        1,
        "only the leg with no magnetic element of its own is the reported tie; got codes: {codes:?}"
    );
    assert!(
        msgs[0].contains("main.bad"),
        "…and the message must name that leg's own carrier: {msgs:?}"
    );
    assert!(
        !msgs[0].contains("main.good"),
        "…not the parallel leg's magnetic element: {msgs:?}"
    );
}

/// A board whose domains declare no DC rail declares no pair for the bridge to
/// be a return of — the flip is the rail word alone (`::AC`), and the firing case
/// above is the same board with `::DC` written back.
#[test]
fn bridge_on_a_board_with_no_dc_rail_is_not_judged_6040() {
    let src = format!(
        "{RES_TIE}module main {{\n    conduit GND @role(main)\n    conduit GNDA @role(quiet)\n    \
         domain DVDD @class(digital) @noise(noisy) {{ rail [VDD_3V3, GND]::AC(3.3V) }}\n    \
         domain AVDD @class(analog) {{ rail [VDDA, GNDA]::AC(3.3V) }}\n    \
         GNDA - t::RES_TIE() - GND @bridge(GND, GNDA)\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::SHARED_RETURN_BRIDGE, &src);
    assert!(
        msgs.is_empty(),
        "an AC rail states no DC pair, so neither end is a declared return; got codes: {codes:?}"
    );
}

// PI-4 filter-subface overreach (power-quality-design.md §2.4, ruling 4)
//
// §2.2's supply leg protects a load side, and that side is a subface: the quiet
// domain's own declared pair. A sink drawing across that pair is inside the
// domain the filter was declared for; a sink that touches it with only one
// member of its own pair is fed by a filter it never declared to belong to.
//
// The witness is the sink's **own declared pair as bound on its instance** — a
// component's `psnk` row or an instantiated module's `psnk` port row — which is
// why the same class instantiated twice can be judged apart, and why the message
// can name the pair as this call site reads it. Both sides compare class ids,
// never spellings.
//
// Every silence below is a single-axis flip off a board that fires, and the
// firing twin is kept on the same board wherever the silence could otherwise
// pass for a rule that never ran.

/// §2.2's leg, complete: the in-file ferrite stand-in across the two rails, plus
/// the load-side decoupling that closes the LC — so 6037 is silent and the only
/// question left on this board is what draws from the subface it protects.
const PI4_LEG: &str = "VDD_3V3 - fba::FB_BRIDGE() - VDDA @bridge(VDD_3V3, VDDA)\n    \
                       CAP_DECOUP c\n    c.1 -> VDDA\n    c.2 -> GNDA\n    ";

/// The judged shape: a declared sink pair whose return is the subface's own
/// (GNDA) but whose supply comes from the other domain's hot copper — the filter
/// feeding a part that never declared to belong to it. The message names the
/// terminal, the pair as the call site binds it, the quiet domain, the leg as
/// written and the pair that leg protects.
#[test]
fn sink_pair_piercing_the_filter_subface_fires_6042() {
    let src = format!(
        "{FB_BRIDGE}{CAP_DECOUP}{SINK_DC}module main {{\n    {PI3_DOMAINS}{PI4_LEG}\
         SINK_DC s\n    s.VDD -> VDD_3V3\n    s.GND -> GNDA\n}}\n"
    );
    let codes = build_codes(&src);
    let n = codes
        .iter()
        .filter(|&&c| c == mcc::errcodes::FILTER_SUBFACE_OVERREACH)
        .count();
    assert_eq!(
        n, 1,
        "a sink pair touching the subface with only its return member must fire 6042 exactly once; got codes: {codes:?}"
    );
    let msgs = msgs_of(mcc::errcodes::FILTER_SUBFACE_OVERREACH, &src);
    assert!(
        msgs[0].contains("main.s.VDD"),
        "6042 must name the sink terminal the pair is declared on: {msgs:?}"
    );
    assert!(
        msgs[0].contains("[VDD_3V3, GNDA]"),
        "…and the pair as this call site binds it, not the row's member spellings: {msgs:?}"
    );
    assert!(
        msgs[0].contains("AVDD") && msgs[0].contains("[VDDA, GNDA]"),
        "…and the quiet domain with the pair the leg protects: {msgs:?}"
    );
    assert!(
        msgs[0].contains("VDDA <-> VDD_3V3"),
        "…and the leg clause as written (its two names in clause order), so the reader can find it: {msgs:?}"
    );
}

/// The honoured shape, on the same board as the judged one: a sink declaring the
/// subface's own pair is exactly the part the filter was declared for. The
/// fired-twin count is what proves the rule ran on this board.
#[test]
fn sink_declaring_the_subface_pair_is_clean_6042() {
    let src = format!(
        "{FB_BRIDGE}{CAP_DECOUP}{SINK_DC}module main {{\n    {PI3_DOMAINS}{PI4_LEG}\
         SINK_DC ok\n    ok.VDD -> VDDA\n    ok.GND -> GNDA\n    \
         SINK_DC bad\n    bad.VDD -> VDD_3V3\n    bad.GND -> GNDA\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::FILTER_SUBFACE_OVERREACH, &src);
    assert_eq!(
        msgs.len(),
        1,
        "the sink declaring the subface's own pair is the domain's own load and must not be reported (the flipped twin must be); got codes: {codes:?}"
    );
    assert!(
        msgs[0].contains("bad") && !msgs[0].contains("ok"),
        "…and the one verdict is the flipped twin's: {msgs:?}"
    );
}

/// The other single-axis flip: the supply on the subface's hot member and the
/// return off it. One member of the pair on the subface is enough for the verdict
/// — the filter's load side is being drawn from by a part that returns elsewhere.
#[test]
fn sink_returning_off_the_subface_fires_6042() {
    let src = format!(
        "{FB_BRIDGE}{CAP_DECOUP}{SINK_DC}module main {{\n    {PI3_DOMAINS}{PI4_LEG}\
         SINK_DC s\n    s.VDD -> VDDA\n    s.GND -> GND\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::FILTER_SUBFACE_OVERREACH, &src);
    assert_eq!(
        msgs.len(),
        1,
        "a sink supplied from the subface but returning off it must fire 6042 exactly once; got codes: {codes:?}"
    );
    assert!(
        msgs[0].contains("[VDDA, GND]"),
        "6042 must state the pair as bound, off-subface member and all: {msgs:?}"
    );
}

/// And the pair that touches no member of the subface at all: that is not this
/// filter's business, and the rule says nothing about it. The fired twin is again
/// kept on board — a rule that had stopped running would report neither.
#[test]
fn sink_pair_off_the_subface_is_not_judged_6042() {
    let src = format!(
        "{FB_BRIDGE}{CAP_DECOUP}{SINK_DC}module main {{\n    {PI3_DOMAINS}\
         domain DVDD5 @class(digital) {{ rail [VDD_5V, GND]::DC(5V) }}\n    {PI4_LEG}\
         SINK_DC other\n    other.VDD -> VDD_5V\n    other.GND -> GND\n    \
         SINK_DC bad\n    bad.VDD -> VDD_3V3\n    bad.GND -> GNDA\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::FILTER_SUBFACE_OVERREACH, &src);
    assert_eq!(
        msgs.len(),
        1,
        "a pair drawing from another domain's pair is not the subface's business; got codes: {codes:?}"
    );
    assert!(
        msgs[0].contains("bad") && !msgs[0].contains("other"),
        "…so the one verdict is the pair that did touch the subface: {msgs:?}"
    );
}

/// The module shape, the same law one level up: an instantiated module's `psnk`
/// **port row** declares its pair the way a component's pin row does, and this
/// instance carries it. The message names the sub-module's terminal.
#[test]
fn sub_module_supply_port_piercing_the_subface_fires_6042() {
    let sub_a = "module SUB_A(psnk dc{VDD_3V3, GNDA}::DC(3.3V)) {\n}\n";
    let sub_b = "module SUB_B(psnk dc{VDDA, GNDA}::DC(3.3V)) {\n}\n";
    let src = format!(
        "{DC_IFACE}{sub_a}{sub_b}{FB_BRIDGE}{CAP_DECOUP}module main {{\n    {PI3_DOMAINS}{PI4_LEG}\
         SUB_A bad\n    [VDD_3V3, GNDA] -> bad.dc\n    \
         SUB_B ok\n    [VDDA, GNDA] -> ok.dc\n}}\n"
    );
    let codes = build_codes(&src);
    let n = codes
        .iter()
        .filter(|&&c| c == mcc::errcodes::FILTER_SUBFACE_OVERREACH)
        .count();
    assert_eq!(
        n, 1,
        "a sub-module whose own supply port row pierces the subface must fire 6042 once (its honoured twin must not); got codes: {codes:?}"
    );
    let msgs = msgs_of(mcc::errcodes::FILTER_SUBFACE_OVERREACH, &src);
    assert!(
        msgs[0].contains("main.bad"),
        "6042 must name the instance whose port row was pierced: {msgs:?}"
    );
}

/// The prerequisite guard: 6042 is about what draws from a declared filter leg's
/// load side, so a board whose only bridge is **ground-side** has no such leg —
/// PI-2's own test is what makes a bridge a supply leg, and a sink pair here is
/// judged by no one.
#[test]
fn board_with_no_supply_filter_leg_is_not_judged_6042() {
    let src = format!(
        "{RES_TIE}{CAP_DECOUP}{SINK_DC}module main {{\n    {PI3_DOMAINS}\
         GNDA - t::RES_TIE() - GND @bridge(GND, GNDA)\n    \
         SINK_DC s\n    s.VDD -> VDD_3V3\n    s.GND -> GNDA\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::FILTER_SUBFACE_OVERREACH, &src);
    assert!(
        msgs.is_empty(),
        "no supply filter leg means no subface to have drawn from; got codes: {codes:?}"
    );
}

// ── PWR-6 pin-row host (exposed-protection-design.md §2/§8.4, R8) ──
//
// The language spells a declared transient boundary on two hosts: a module port
// row (`io DP @exposed(...)`) and a component pin row (`io 1 = IO @exposed(...)`).
// One word on one kind of declaration, so PWR-6's two halves read one seed list
// and both hosts are judged alike. Before this carrier the pin row had no home
// in the flat layer at all: its words lived in the definition table, where no
// rule can see them.

/// A TVS-like device whose **pin row** carries the boundary word.
const TV_PIN_EXPOSED: &str = "component TVP {\n    pins = [\n        io 1 = IO @exposed(esd_contact)\n        psnk 2 = G\n    ]\n}\n";

/// The pin-row host is judged exactly as the port-row host: an undeclared clamp
/// leaves the boundary uncovered, and the message names the pin's own path.
#[test]
fn pin_row_exposed_without_clamp_fires_6031() {
    let src = format!(
        "{TV_PIN_EXPOSED}module main {{\n    conduit ESDGND @role(protective)\n    io DPN\n    \
         TVP tv\n    tv.IO -> DPN\n    tv.G -> ESDGND\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::EXPOSED_NET_NO_CLAMP, &src);
    assert_eq!(
        msgs.len(),
        1,
        "a pin row declaring @exposed is a boundary host; got codes: {codes:?}"
    );
    assert!(
        msgs[0].contains("main.tv.1") && msgs[0].contains("esd_contact"),
        "6031 must name the pin's path and its declared levels: {msgs:?}"
    );
}

/// The positive control for the same host: the device's own dump leg lands on a
/// reference the scope declares `@clamp` on → covered, silent.
#[test]
fn pin_row_exposed_with_declared_clamp_is_silent_6031() {
    let src = format!(
        "{TV_PIN_EXPOSED}module main {{\n    conduit ESDGND @role(protective)\n    io DPN\n    \
         TVP tv\n    tv.IO -> DPN\n    tv.G -> ESDGND @clamp(ESDGND)\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::EXPOSED_NET_NO_CLAMP),
        "a declared clamp covers the pin-declared boundary too; got codes: {codes:?}"
    );
}

/// The two hosts are one predicate: both spelled on one board, both uncovered,
/// both on the same copper — one report each, because the host is the
/// declaration and each declaration answers for itself.
#[test]
fn port_row_and_pin_row_hosts_fire_alike_6031() {
    let src = format!(
        "{TV_PIN_EXPOSED}{TV}module main {{\n    conduit ESDGND @role(protective)\n    \
         io DP @exposed(esd_contact)\n    \
         TVP tvp\n    tvp.IO -> DP\n    tvp.G -> ESDGND\n    \
         TV tva\n    tva.IO -> DP\n    tva.G -> ESDGND\n}}\n"
    );
    let msgs = msgs_of(mcc::errcodes::EXPOSED_NET_NO_CLAMP, &src);
    assert_eq!(
        msgs.len(),
        2,
        "one report per uncovered host, whichever way the row spells it: {msgs:?}"
    );
    assert!(
        msgs.iter().any(|m| m.contains("'main.DP'"))
            && msgs.iter().any(|m| m.contains("'main.tvp.1'")),
        "both the port host and the pin host are named: {msgs:?}"
    );
}

/// A pin-row host inside a `func` (the pass-2 flatten site) is the same host —
/// the carrier is decoded at both flatten sites, so neither placement is blind.
#[test]
fn pin_row_exposed_host_inside_a_func_is_judged_too_6031() {
    let src = format!(
        "{TV_PIN_EXPOSED}module main {{\n    conduit ESDGND @role(protective)\n    io DPN\n    \
         func M() {{\n        TVP tv\n        tv.IO -> DPN\n        tv.G -> ESDGND\n    }}\n}}\n"
    );
    let msgs = msgs_of(mcc::errcodes::EXPOSED_NET_NO_CLAMP, &src);
    assert_eq!(
        msgs.len(),
        1,
        "a func-placed component is a declared host too: {msgs:?}"
    );
    assert!(
        msgs[0].contains("1") && msgs[0].contains("esd_contact"),
        "6031 must name the func-placed pin: {msgs:?}"
    );
}

/// The two halves do not stack on the pin host either: covered on its own net
/// (6031 silent), the flood past an ordinary pass still reaches an unclamped
/// quiet face → 6044 alone, exactly as for a port host.
#[test]
fn pin_row_exposed_host_does_not_stack_the_two_halves() {
    let src = format!(
        "{TV_PIN_EXPOSED}{PROT_TWOPIN}module main {{\n    {DOWNSTREAM_QUIET_BOARD}\
         io DPN\n    \
         TVP tv\n    tv.IO -> DPN\n    tv.G -> ESDGND @clamp(ESDGND)\n    \
         TWOPIN.PLAIN r\n    r.A -> DPN\n    r.B -> VDDA\n}}\n"
    );
    let codes = build_codes(&src);
    let msgs = msgs_of(mcc::errcodes::EXPOSED_NET_DOWNSTREAM_UNPROTECTED, &src);
    assert_eq!(
        msgs.len(),
        1,
        "the downstream half judges the pin host too; got codes: {codes:?}"
    );
    assert!(
        msgs[0].contains("main.tv.1") && msgs[0].contains("VDDA"),
        "6044 must name the func-host path and the uncovered face: {msgs:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::EXPOSED_NET_NO_CLAMP),
        "the clamp on the pin's net covers it — 6031 stays silent; got codes: {codes:?}"
    );
}

// ── pin copper expectation (6051/6052, pin-expectation-design.md §3 v0.1) ──
//
// A component pin row carrying @role(quiet) expects the landed copper to
// anchor a quiet identity; the module layer's binding is the witness. Every
// verdict branch gets two members (two ground rows), per the acceptance
// discipline: a one-member branch cannot tell a per-pin rule from a
// per-instance one.

/// A speaker-like part: two signal pins and two ground rows carrying the
/// quiet expectation (the hbl PHB2AWB shape).
const SPK: &str = "component SPK {\n    pins = [\n        in [1,2] = IN{P, N}\n        3 = GND @role(quiet)\n        4 = GND @role(quiet)\n    ]\n}\n";

/// Anchored through the copper conduit's own @role word: both ground rows sit
/// on a conduit declared @role(quiet), so neither code may fire.
#[test]
fn quiet_expectation_on_quiet_conduit_stays_silent() {
    let src = format!(
        "{SPK}\nmodule main {{\n    conduit GND  @role(main) @star\n    \
         conduit GNDA @role(quiet)\n    \
         GNDA - fb::FB() - GND @bridge(GNDA, GND)\n    \
         SPK u1\n    u1{{3, 4}} - [GNDA, GNDA]\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::PIN_COPPER_EXPECTATION_MISMATCH),
        "quiet conduit anchors the quiet expectation — no 6051; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::PIN_COPPER_EXPECTATION_UNANCHORED),
        "the conduit route needs no class at all — no 6052; got codes: {codes:?}"
    );
}

/// Anchored through the domain face: GNDA is a rail *member* with no same-name
/// conduit, and AV declares its own face quiet (@class/@noise — the §1.4 read;
/// a domain that says nothing anchors nothing).
#[test]
fn quiet_expectation_on_quiet_rail_member_stays_silent() {
    let src = format!(
        "{SPK}\nmodule main {{\n    conduit GND @role(main) @star\n    \
         domain AV @class(analog) @noise(sensitive) {{ rail [VDDA, GNDA]::DC(3V3) }}\n    \
         SPK u1\n    u1{{3, 4}} - [GNDA, GNDA]\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::PIN_COPPER_EXPECTATION_MISMATCH),
        "the quiet face anchors the expectation — no 6051; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::PIN_COPPER_EXPECTATION_UNANCHORED),
        "a rail member resolves a class — no 6052; got codes: {codes:?}"
    );
}

/// Contradicted through the conduit route: both ground rows sit on a conduit
/// declared @role(main). Two rows, two findings — the rule is per pin.
#[test]
fn quiet_expectation_on_main_ground_fires_6051_twice() {
    let src = format!(
        "{SPK}\nmodule main {{\n    conduit GND @role(main) @star\n    \
         SPK u1\n    u1{{3, 4}} - [GND, GND]\n}}\n"
    );
    let codes = build_codes(&src);
    let n = codes
        .iter()
        .filter(|&&c| c == mcc::errcodes::PIN_COPPER_EXPECTATION_MISMATCH)
        .count();
    assert_eq!(
        n, 2,
        "two quiet-expectation rows on a main conduit → 6051 ×2; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::PIN_COPPER_EXPECTATION_UNANCHORED),
        "the main conduit resolves a class — no 6052; got codes: {codes:?}"
    );
}

/// Contradicted through the face route: the rows land on a hot rail member of
/// a domain that declares no quiet face at all.
#[test]
fn quiet_expectation_on_hot_rail_member_fires_6051() {
    let src = format!(
        "{SPK}\nmodule main {{\n    conduit GND @role(main) @star\n    \
         domain DV {{ rail [VDD_3V3, GND]::DC(3V3) }}\n    \
         SPK u1\n    u1{{3, 4}} - [VDD_3V3, VDD_3V3]\n}}\n"
    );
    let codes = build_codes(&src);
    let n = codes
        .iter()
        .filter(|&&c| c == mcc::errcodes::PIN_COPPER_EXPECTATION_MISMATCH)
        .count();
    assert_eq!(
        n, 2,
        "a hot member anchors no quiet face → 6051 ×2; got codes: {codes:?}"
    );
}

/// Unanchored: the rows land on bare nets — no conduit copper, no rail
/// membership, so there is no identity to compare against. Info, not a
/// mismatch, and still once per row.
#[test]
fn quiet_expectation_on_bare_net_fires_6052_not_6051() {
    let src = format!(
        "{SPK}\nmodule main {{\n    conduit GND @role(main) @star\n    \
         SPK u1\n    u1{{3, 4}} - [BARE1, BARE2]\n}}\n"
    );
    let codes = build_codes(&src);
    let n = codes
        .iter()
        .filter(|&&c| c == mcc::errcodes::PIN_COPPER_EXPECTATION_UNANCHORED)
        .count();
    assert_eq!(
        n, 2,
        "two bare-net rows → 6052 ×2 (an empty reading is still a reading); got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::PIN_COPPER_EXPECTATION_MISMATCH),
        "absence is not contradiction — no 6051; got codes: {codes:?}"
    );
}

/// v0.1 scope: only `quiet` has a net-side reading. A non-quiet expectation
/// passes the write-site vocabulary and carries no verdict yet — the same
/// bare-net shape the quiet rows fire 6052 on stays silent here.
#[test]
fn non_quiet_expectation_carries_no_verdict_yet() {
    let src = "component SP2 {\n    pins = [\n        3 = GND @role(main)\n        4 = GND @role(main)\n    ]\n}\n\
         module main {\n    conduit GND @role(main) @star\n    \
         SP2 u1\n    u1{3, 4} - [BARE1, BARE2]\n}\n"
        .to_string();
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::PIN_COPPER_EXPECTATION_MISMATCH)
            && !codes.contains(&mcc::errcodes::PIN_COPPER_EXPECTATION_UNANCHORED),
        "v0.1 judges quiet only — main carries no verdict; got codes: {codes:?}"
    );
}

/// An unwired pin carrying the expectation is the unwired-pin rule's object,
/// never an identity verdict: no net, no class, no code from this rule.
#[test]
fn unwired_pin_with_expectation_is_not_judged() {
    let src = format!(
        "{SPK}\nmodule main {{\n    conduit GND @role(main) @star\n    \
         SPK u1\n    u1.1 -> GNDA\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::PIN_COPPER_EXPECTATION_MISMATCH)
            && !codes.contains(&mcc::errcodes::PIN_COPPER_EXPECTATION_UNANCHORED),
        "unwired pins are out of this rule's object; got codes: {codes:?}"
    );
}

// ── v0.3: the signal-class axis and the strength tier (§3.1/§4) ──
//
// `@class(analog|digital)` on a pin row is the second expectation axis: the
// landed class must carry the expected class word — a quiet face reads
// analog, a declared `@class(digital)` or `@noise(noisy)` world reads
// digital. `@req` on the component header is the strength tier: a violated
// expectation of that part is an Error, the unanchored half stays Info.
// Every verdict branch keeps two members, per the acceptance discipline.

/// An op-amp-like part: two signal rows carrying the analog class expectation.
const AMP: &str = "component AMP {\n    pins = [\n        1 = INP @class(analog)\n        2 = INN @class(analog)\n    ]\n}\n";

/// A comparator-like part: two signal rows carrying the digital expectation.
const CMP: &str = "component CMP {\n    pins = [\n        1 = OUT @class(digital)\n        2 = CLK @class(digital)\n    ]\n}\n";

/// The diagnostic levels of one code, in emission order — the tier face.
fn levels_of(code: u32, src: &str) -> Vec<mcc::DiagnosticLevel> {
    let _lock = common::lock();
    common::reset();
    let uri: McURI = "/mcc/power-intent-l1.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");
    mcc::mcc_diagnose_all()
        .iter()
        .filter(|d| d.code == code)
        .map(|d| d.level)
        .collect()
}

/// Anchored on the class axis: both signal rows sit on a rail member of a
/// domain whose face is quiet (@class(analog) — the §1.4 read), so the class
/// reads analog and neither code may fire.
#[test]
fn analog_class_expectation_on_quiet_face_stays_silent() {
    let src = format!(
        "{AMP}\nmodule main {{\n    \
         domain AV @class(analog) {{ rail [VDDA, GNDA]::DC(3V3) }}\n    \
         AMP u1\n    u1.1 - GNDA\n    u1.2 - GNDA\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::PIN_COPPER_EXPECTATION_MISMATCH),
        "the quiet face reads analog — no 6051; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::PIN_COPPER_EXPECTATION_UNANCHORED),
        "the rail member resolves a class — no 6052; got codes: {codes:?}"
    );
}

/// Contradicted on the class axis: the rows land on a rail member of a domain
/// declared `@class(digital)` — the class reads digital, the expectation says
/// analog. Two rows, two findings, both at the default Warning tier.
#[test]
fn analog_class_expectation_on_digital_world_fires_6051_warning() {
    let src = format!(
        "{AMP}\nmodule main {{\n    \
         domain DV @class(digital) {{ rail [VDD_3V3, GND]::DC(3V3) }}\n    \
         AMP u1\n    u1.1 - VDD_3V3\n    u1.2 - VDD_3V3\n}}\n"
    );
    let codes = build_codes(&src);
    let n = codes
        .iter()
        .filter(|&&c| c == mcc::errcodes::PIN_COPPER_EXPECTATION_MISMATCH)
        .count();
    assert_eq!(n, 2, "analog expectation on a digital world → 6051 ×2; got codes: {codes:?}");
    let levels = levels_of(mcc::errcodes::PIN_COPPER_EXPECTATION_MISMATCH, &src);
    assert!(
        levels.iter().all(|l| *l == mcc::DiagnosticLevel::Warning),
        "no @req on the part — the default tier is Warning; got {levels:?}"
    );
}

/// Anchored through the class word itself: `@class(digital)` rows land on a
/// domain declared `@class(digital)` — the same word, no code.
#[test]
fn digital_class_expectation_on_digital_domain_stays_silent() {
    let src = format!(
        "{CMP}\nmodule main {{\n    \
         domain DV @class(digital) {{ rail [VDD_3V3, GND]::DC(3V3) }}\n    \
         CMP u1\n    u1.1 - VDD_3V3\n    u1.2 - VDD_3V3\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::PIN_COPPER_EXPECTATION_MISMATCH),
        "the class word anchors the expectation — no 6051; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::PIN_COPPER_EXPECTATION_UNANCHORED),
        "the rail member resolves a class — no 6052; got codes: {codes:?}"
    );
}

/// Anchored through the face model's own opposite: `@class(digital)` rows on
/// a domain declared only `@noise(noisy)` — the noisy face is §1.4's declared
/// opposite of the quiet/analog one, so the class reads digital.
#[test]
fn digital_class_expectation_on_noisy_face_stays_silent() {
    let src = format!(
        "{CMP}\nmodule main {{\n    \
         domain PW @noise(noisy) {{ rail [VS, GNDP]::DC(12V) }}\n    \
         CMP u1\n    u1.1 - VS\n    u1.2 - VS\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::PIN_COPPER_EXPECTATION_MISMATCH),
        "the noisy face reads digital — no 6051; got codes: {codes:?}"
    );
}

/// Contradicted the other way: `@class(digital)` rows land on the quiet face
/// — the class reads analog, the expectation says digital.
#[test]
fn digital_class_expectation_on_quiet_face_fires_6051() {
    let src = format!(
        "{CMP}\nmodule main {{\n    \
         domain AV @class(analog) {{ rail [VDDA, GNDA]::DC(3V3) }}\n    \
         CMP u1\n    u1.1 - GNDA\n    u1.2 - GNDA\n}}\n"
    );
    let codes = build_codes(&src);
    let n = codes
        .iter()
        .filter(|&&c| c == mcc::errcodes::PIN_COPPER_EXPECTATION_MISMATCH)
        .count();
    assert_eq!(n, 2, "digital expectation on a quiet face → 6051 ×2; got codes: {codes:?}");
}

/// Unanchored on the class axis: the rows land on bare nets — no class, so
/// there is no class word to compare against. 6052 Info, never 6051.
#[test]
fn class_expectation_on_bare_net_fires_6052_not_6051() {
    let src = format!(
        "{AMP}\nmodule main {{\n    \
         AMP u1\n    u1.1 - BARE1\n    u1.2 - BARE2\n}}\n"
    );
    let codes = build_codes(&src);
    let n = codes
        .iter()
        .filter(|&&c| c == mcc::errcodes::PIN_COPPER_EXPECTATION_UNANCHORED)
        .count();
    assert_eq!(n, 2, "bare nets carry no class word → 6052 ×2; got codes: {codes:?}");
    assert!(
        !codes.contains(&mcc::errcodes::PIN_COPPER_EXPECTATION_MISMATCH),
        "unprovable is not violated — no 6051; got codes: {codes:?}"
    );
}

/// A part whose header carries `@req`: the same contradicted board the
/// default-tier test uses, now judged as a physical fact — the same code,
/// the same count, the Error tier.
#[test]
fn req_tier_lifts_violated_expectation_to_error() {
    let spk_req = "component SPK @req {\n    pins = [\n        3 = GND @role(quiet)\n        4 = GND @role(quiet)\n    ]\n}\n";
    let src = format!(
        "{spk_req}\nmodule main {{\n    conduit GND @role(main) @star\n    \
         SPK u1\n    u1{{3, 4}} - [GND, GND]\n}}\n"
    );
    let codes = build_codes(&src);
    let n = codes
        .iter()
        .filter(|&&c| c == mcc::errcodes::PIN_COPPER_EXPECTATION_MISMATCH)
        .count();
    assert_eq!(n, 2, "@req does not change the object — 6051 ×2; got codes: {codes:?}");
    let levels = levels_of(mcc::errcodes::PIN_COPPER_EXPECTATION_MISMATCH, &src);
    assert_eq!(
        levels,
        vec![mcc::DiagnosticLevel::Error, mcc::DiagnosticLevel::Error],
        "@req lifts the violated leg to Error; got {levels:?}"
    );
}

/// The tier lifts only the violated leg: an `@req` part whose rows land on
/// bare nets still fires the unanchored half at Info — a board that never
/// splits domains is not forced into it.
#[test]
fn req_tier_does_not_lift_the_unanchored_half() {
    let spk_req = "component SPK @req {\n    pins = [\n        3 = GND @role(quiet)\n        4 = GND @role(quiet)\n    ]\n}\n";
    let src = format!(
        "{spk_req}\nmodule main {{\n    conduit GND @role(main) @star\n    \
         SPK u1\n    u1{{3, 4}} - [BARE1, BARE2]\n}}\n"
    );
    let codes = build_codes(&src);
    assert!(
        !codes.contains(&mcc::errcodes::PIN_COPPER_EXPECTATION_MISMATCH),
        "unprovable stays unviolated — no 6051 at any tier; got codes: {codes:?}"
    );
    let levels = levels_of(mcc::errcodes::PIN_COPPER_EXPECTATION_UNANCHORED, &src);
    assert_eq!(levels.len(), 2, "two bare rows → 6052 ×2; got {levels:?}");
    assert!(
        levels.iter().all(|l| *l == mcc::DiagnosticLevel::Info),
        "@req never lifts the unanchored half — Info stays Info; got {levels:?}"
    );
}
