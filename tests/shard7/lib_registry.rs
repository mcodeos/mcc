// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The P2 local closure end to end (registry-design.md §4/§6): build solves
//! `[dependencies]` against a `file://` registry, installs what is missing,
//! writes `mcode.lock` once and never rewrites it; `lib install <spec>`
//! solves a single token (`--here` → `<proj>/deps/`); partno keys expand
//! through the alias entry and the lock records them in full; a cycle is
//! named by path; a corrupt thin artifact fails with a checksum mismatch and
//! no half install.
//!
//! The registry tree is synthesized from a real `mcc lib pack` product: the
//! artifacts are copied into `dl/<category>/<name>/<ver>/` (names normalized
//! to the canonical two-segment version) and the per-name metadata is
//! written by hand — the same shapes `mcpub/mkregistry.sh` generates.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

// Fixture

/// Minimal part file, shape lifted from mcpub/power/ams1117 (abstract base +
/// one orderable SKU) with no external library — a fresh `MCC_SYSTEM_ROOT`
/// has no seeded mcode to reach for.
const ENTRY: &str = r#"// regtest fixture part (no external lib)
abstract component REGTEST(v_out::UV.VOLT = 3.3V)
{
    spec = [
        output_voltage = v_out
        output_current = 1A
    ]

    pins = [
        psnk [3,1] = [Vin, GND], "unregulated input pair"
        psrc [2,1] = [Vout, GND], "regulated output pair"
        tab = TAB, "heat tab, tied to Vout (NOT GND)"
    ]
}

component REGTEST_3_3 : REGTEST
{
    partno = "REGTEST-3.3"
}
"#;

const DEP_ENTRY: &str = r#"// dep fixture part (no external lib)
component DEPTEST()
{
    pins = [
        p = [A, B], " lone pair"
    ]
}
"#;

fn pack_manifest(name: &str, entry: &str, deps: &[(&str, &str)]) -> String {
    let mut s = format!(
        r#"[package]
format = "1"
name = "{name}"
version = "0.1.0"
category = "power"
entry = "{entry}"

"#
    );
    for (d, req) in deps {
        if deps.len() == 1 {
            s.push_str("[dependencies]\n");
        }
        s.push_str(&format!("{d} = \"{req}\"\n"));
    }
    s
}

/// The regtest variant matrix (only that pack ships a partno face).
fn variant_section() -> &'static str {
    "[variants.REGTEST_3_3]\nbase = \"REGTEST\"\nsince = \"0.1.0\"\n"
}

fn sha256_hex_file(p: &Path) -> String {
    let data = std::fs::read(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    let mut h = Sha256::new();
    h.update(&data);
    h.finalize().iter().map(|b| format!("{:02x}", b)).collect()
}

struct Fixture {
    base: PathBuf,
    /// The private MCC_SYSTEM_ROOT (data root once packs land there).
    root: PathBuf,
    /// The file:// registry tree.
    reg: PathBuf,
    /// A project with `[config.registry]` pointed at the tree.
    proj: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

fn run_mcc_in(cwd: &Path, root: &Path, args: &[&str]) -> (String, String, bool) {
    let mut full: Vec<String> = vec!["--local".to_string()];
    full.extend(args.iter().map(|s| s.to_string()));
    let out = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .current_dir(cwd)
        .env("MCC_SYSTEM_ROOT", root)
        .args(&full)
        .output()
        .expect("run mcc");
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
        out.status.success(),
    )
}

fn in_proj(f: &Fixture, args: &[&str]) -> (String, String, bool) {
    run_mcc_in(&f.proj, &f.root, args)
}

impl Fixture {
    fn manifest_for(&self, deps: &str) -> String {
        format!(
            "[project]\nname = \"p\"\nversion = \"0.1\"\nentry = \"src/main.mc\"\n\n\
             [dependencies]\nmcode = \"*\"\n{deps}\n\n\
             [config.registry]\nurl = \"file://{}\"\n",
            self.reg.display()
        )
    }

    /// Pack `src` and register it in the tree under its canonical two-segment
    /// version, with the given metadata variants (partno strings) and deps.
    fn register(&self, src: &Path, name: &str, variants: &[&str], deps: &[(&str, &str)]) {
        let out = self.base.join(format!("pack-out-{name}"));
        std::fs::create_dir_all(&out).unwrap();
        let (_, stderr, ok) = run_mcc_in(
            &self.base,
            &self.root,
            &["lib", "pack", src.to_str().unwrap(), "--out", out.to_str().unwrap()],
        );
        assert!(ok, "packing {name} failed: {stderr}");

        let dir = self.reg.join("dl").join("power").join(name).join("0.1");
        std::fs::create_dir_all(&dir).unwrap();
        let mut thin_sum = String::new();
        let mut full_sum = String::new();
        for (src_name, dst_name, sum) in [
            (format!("{name}-0.1.0.thin.mcl"), format!("{name}-0.1.thin.mcl"), &mut thin_sum),
            (format!("{name}-0.1.0.mcl"), format!("{name}-0.1.mcl"), &mut full_sum),
        ] {
            std::fs::copy(out.join(&src_name), dir.join(&dst_name)).unwrap();
            *sum = format!("sha256:{}", sha256_hex_file(&dir.join(&dst_name)));
        }
        let variants_json: Vec<String> = variants.iter().map(|v| format!("\"{v}\"")).collect();
        let deps_json: Vec<String> = deps
            .iter()
            .map(|(d, r)| format!("\"{d}\": \"{r}\""))
            .collect();
        let meta = format!(
            "{{\"name\": \"{name}\", \"category\": \"power\", \"versions\": {{\"0.1\": \
             {{\"checksum\": \"{full_sum}\", \"thin_checksum\": \"{thin_sum}\", \"size\": 1, \
             \"deps\": {{{}}}, \"variants\": [{}]}}}}}}",
            deps_json.join(", "),
            variants_json.join(", "),
        );
        std::fs::create_dir_all(self.reg.join("lib")).unwrap();
        std::fs::write(self.reg.join("lib").join(format!("{name}.json")), meta).unwrap();
    }
}

/// A fixture with the `regtest` pack (one partno REGTEST-3.3) registered and
/// a project whose manifest is `manifest(deps)`.
fn fixture(tag: &str, deps: &str) -> Fixture {
    let base = std::env::temp_dir().join(format!(
        "mcc-libreg-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join("root");
    let reg = base.join("reg");
    let proj = base.join("proj");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(proj.join("src")).unwrap();
    std::fs::write(proj.join("src/main.mc"), "module main()\n{\n}\n").unwrap();

    let f = Fixture { base, root, reg, proj };
    std::fs::write(f.proj.join("project.toml"), f.manifest_for(deps)).unwrap();

    let pack = f.base.join("pack-regtest");
    std::fs::create_dir_all(&pack).unwrap();
    std::fs::write(pack.join("regtest.mc"), ENTRY).unwrap();
    std::fs::write(
        pack.join("pack.toml"),
        format!("{}{}", pack_manifest("regtest", "regtest.mc", &[]), variant_section()),
    )
    .unwrap();
    f.register(&pack, "regtest", &["REGTEST-3.3"], &[]);
    f
}

// build: solve + install + lock

#[test]
fn build__solves_installs_and_writes_the_lock_once() {
    let f = fixture("solve", "regtest = \"*\"");
    let (out, stderr, ok) = in_proj(&f, &["build", "-f", "json"]);
    assert!(ok, "build failed: {stderr}");
    assert!(stderr.contains("installed regtest@0.1"), "reports the install: {stderr}");

    let landed = f.root.join("packtest");
    assert!(
        f.root.join("regtest@0.1").is_dir(),
        "the pack lands in the data root (nothing at {})",
        landed.display()
    );
    let lock = std::fs::read_to_string(f.proj.join("mcode.lock")).unwrap();
    assert!(lock.contains("regtest") && lock.contains("0.1"), "lock records the pack: {lock}");
    assert!(lock.contains("rev"), "lock records the mcode stdlib-rail rev: {lock}");

    // A second build is lock-first: the lock file is byte-identical (never
    // rewritten by build) and the pack is not re-downloaded.
    let before = std::fs::read_to_string(f.proj.join("mcode.lock")).unwrap();
    let (_, stderr2, ok2) = in_proj(&f, &["build", "-f", "json"]);
    assert!(ok2, "second build failed: {stderr2}");
    assert!(!stderr2.contains("installed"), "no reinstall on a locked build: {stderr2}");
    let after = std::fs::read_to_string(f.proj.join("mcode.lock")).unwrap();
    assert_eq!(before, after, "build never rewrites an existing lock");
    let _ = out;
}

#[test]
fn build__offline_rebuild_runs_on_the_lock_alone() {
    let f = fixture("offline", "regtest = \"*\"");
    let (_, stderr, ok) = in_proj(&f, &["build", "-f", "json"]);
    assert!(ok, "first build failed: {stderr}");

    // The registry tree disappears; the lock + the installed copy carry it.
    std::fs::remove_dir_all(&f.reg).unwrap();
    let (_, stderr2, ok2) = in_proj(&f, &["build", "-f", "json"]);
    assert!(ok2, "offline rebuild failed: {stderr2}");
    assert!(!stderr2.contains("installed"), "offline rebuild installs nothing: {stderr2}");
}

#[test]
fn build__stale_lock_warns_e2056_and_stays_unwritten() {
    let f = fixture("stale", "regtest = \"*\"");
    // A lock missing the declared key: build may not write it (only
    // `mcc lib update` holds that authority) — it warns E2056 instead.
    std::fs::write(f.proj.join("mcode.lock"), "mcode = { rev = \"9999\" }\n").unwrap();
    let before = std::fs::read_to_string(f.proj.join("mcode.lock")).unwrap();
    let (_, stderr, ok) = in_proj(&f, &["build", "-f", "json"]);
    assert!(ok, "a stale lock still builds (fresh solve for the gap): {stderr}");
    assert!(
        stderr.contains("E2056") && stderr.contains("regtest"),
        "warns E2056 naming the uncovered key: {stderr}"
    );
    let after = std::fs::read_to_string(f.proj.join("mcode.lock")).unwrap();
    assert_eq!(before, after, "the lock is never build's to rewrite");
}

// partno

#[test]
fn build__partno_key_expands_and_the_lock_records_it_in_full() {
    // The alias entry: package face + the versions where the partno exists.
    let f = fixture("partno", "\"REGTEST-3.3\" = \"*\"");
    let alias = format!(
        "{{\"name\": \"REGTEST-3.3\", \"category\": \"power\", \"package\": \"regtest\", \
         \"versions\": {{\"0.1\": {{\"thin_checksum\": \"sha256:x\", \
         \"variants\": [\"REGTEST-3.3\"]}}}}}}"
    );
    std::fs::write(f.reg.join("lib").join("REGTEST-3.3.json"), alias).unwrap();

    let (_, stderr, ok) = in_proj(&f, &["build", "-f", "json"]);
    assert!(ok, "build failed: {stderr}");
    let lock = std::fs::read_to_string(f.proj.join("mcode.lock")).unwrap();
    assert!(
        lock.contains("\"REGTEST-3.3\"") && lock.contains("package = \"regtest\""),
        "the lock records the partno form in full: {lock}"
    );
}

// install: the registry form

#[test]
fn install__here_places_the_pack_in_deps_and_gitignores_it() {
    let f = fixture("here", "");
    let (_, stderr, ok) = in_proj(&f, &["lib", "install", "regtest@0.1", "--here"]);
    assert!(ok, "install failed: {stderr}");

    let landed = f.proj.join("deps").join("regtest@0.1");
    assert!(landed.join("regtest.mc").is_file(), "the pack lands in <proj>/deps");
    assert!(!f.root.join("regtest@0.1").exists(), "nothing lands in the data root");
    let gi = std::fs::read_to_string(f.proj.join(".gitignore")).unwrap();
    assert!(gi.lines().any(|l| l.trim() == "deps/"), "deps/ is gitignored: {gi}");
}

#[test]
fn install__without_a_configured_registry_names_the_key() {
    let f = fixture("nourl", "");
    // Strip the [config.registry] block: the caller is never left to invent
    // an endpoint.
    let toml = std::fs::read_to_string(f.proj.join("project.toml")).unwrap();
    let bare = toml.split("[config.registry]").next().unwrap();
    std::fs::write(f.proj.join("project.toml"), bare).unwrap();
    let (_, stderr, ok) = in_proj(&f, &["lib", "install", "regtest@0.1"]);
    assert!(!ok, "no registry configured must fail");
    assert!(
        stderr.contains("registry") && stderr.contains("url"),
        "names the config key: {stderr}"
    );
}

// failure faces

#[test]
fn build__cycle_is_named_by_path() {
    let f = fixture("cycle", "packa = \"*\"");
    for (name, deps) in [("packa", &[("packb", "*")][..]), ("packb", &[("packa", "*")][..])] {
        let src = f.base.join(format!("pack-{name}"));
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join(format!("{name}.mc")), DEP_ENTRY).unwrap();
        std::fs::write(
            src.join("pack.toml"),
            pack_manifest(name, &format!("{name}.mc"), deps),
        )
        .unwrap();
        f.register(&src, name, &[], &[]);
    }
    let (_, stderr, ok) = in_proj(&f, &["build", "-f", "json"]);
    assert!(!ok, "a dependency cycle must fail the build");
    assert!(
        stderr.contains("packa -> packb -> packa"),
        "the cycle is named in walk order: {stderr}"
    );
}

#[test]
fn build__corrupt_thin_artifact_fails_checksum_with_no_half_install() {
    let f = fixture("corrupt", "regtest = \"*\"");
    let artifact = f
        .reg
        .join("dl")
        .join("power")
        .join("regtest")
        .join("0.1")
        .join("regtest-0.1.thin.mcl");
    std::fs::write(&artifact, b"garbage bytes, definitely not the packed archive").unwrap();

    let (_, stderr, ok) = in_proj(&f, &["build", "-f", "json"]);
    assert!(!ok, "a checksum mismatch must fail the build");
    assert!(
        stderr.contains("checksum mismatch"),
        "names the integrity failure: {stderr}"
    );
    assert!(
        !f.root.join("regtest@0.1").exists(),
        "no half install: nothing landed in the data root"
    );
}
