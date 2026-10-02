// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc lib pack` / `lib inspect` / `lib install --from *.mcl` — the P1 local
//! loop (registry-design.md §3/§4.2).
//!
//! What is locked here, per surface:
//!
//! * **pack emits the dual artifact** — `<name>-<ver>.mcl` + `.thin.mcl`, both
//!   starting with the zstd frame magic (`28 B5 2F FD`), full ⊇ thin; the
//!   format gate (`format = "1"` only) and the variant base check refuse
//!   *before* any archive is written (the no-compile-no-pack structural siblings);
//! * **inspect is a pure manifest dump** — no install, coordinates and
//!   attachment checksums straight off pack.toml;
//! * **install does the three checks** — manifest present (①), entry present
//!   (②), sha256 per attachment (③) — lands at `<root>/<name>@<ver>/` with the
//!   version taken from pack.toml, refuses a caller-supplied name that
//!   contradicts it, and rebuilds the index;
//! * **thin installs too** — entry + manifest only, no attachments; the
//!   compile face must not need the datasheet.
//!
//! Every call runs against a private `MCC_SYSTEM_ROOT` so the machine's real
//! `~/.mcode` is never touched; the bootstrap seeds the default libraries into
//! an empty root on first use, which is what lets the check gate resolve
//! `PKG.*` inside the fixture entry.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

// Fixture

/// Minimal part file, shape lifted from mcpub/power/ams1117 (abstract base +
/// one orderable SKU) but with **no library dependency** — a fresh
/// `MCC_SYSTEM_ROOT` has no mcode seeded, so the fixture may not reach for
/// `PKG.*` (the same inline-face discipline ac_face_gates.rs uses). It must
/// stay green through the check gate and exercise the variant base check.
const ENTRY: &str = r#"// packtest fixture part (shape: ams1117, no external lib)
abstract component PKTEST(v_out::UV.VOLT = 3.3V)
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

component PKTEST_3_3 : PKTEST
{
    partno = "PKTEST-3.3"
}
"#;

const DATASHEET: &str = "PKTEST datasheet stand-in\n";

fn manifest(checksum: &str) -> String {
    format!(
        r#"[package]
format = "1"
name = "packtest"
version = "0.1.0"
category = "power"
entry = "packtest.mc"

[variants.PKTEST_3_3]
base = "PKTEST"
since = "0.1.0"

[[attachments]]
path = "datasheet.txt"
kind = "datasheet"
checksum = "sha256:{checksum}"
"#
    )
}

fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().iter().map(|b| format!("{:02x}", b)).collect()
}

/// A fresh fixture pack directory + a fresh system root. Returns (pack, root, out).
fn fixture(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
    let base = std::env::temp_dir().join(format!(
        "mcc-libpack-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&base);
    let pack = base.join("pack");
    let root = base.join("root");
    let out = base.join("out");
    std::fs::create_dir_all(&pack).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(pack.join("packtest.mc"), ENTRY).unwrap();
    std::fs::write(pack.join("datasheet.txt"), DATASHEET).unwrap();
    std::fs::write(pack.join("README.md"), "# packtest\n").unwrap();
    std::fs::write(pack.join("pack.toml"), manifest(&sha256_hex(DATASHEET.as_bytes()))).unwrap();
    (pack, root, out)
}

/// Run `mcc --local <args…>` with `MCC_SYSTEM_ROOT=<root>`.
fn run_mcc(root: &Path, args: &[&str]) -> (String, String, bool) {
    let mut full: Vec<String> = vec!["--local".to_string()];
    full.extend(args.iter().map(|s| s.to_string()));
    let out = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .current_dir(std::env::temp_dir())
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

fn first_line(stderr: &str) -> String {
    stderr
        .lines()
        .find(|l| l.starts_with("error:"))
        .unwrap_or("")
        .to_string()
}

// pack

#[test]
fn pack__emits_thin_and_full_with_zstd_magic() {
    let (pack, root, out) = fixture("emit");
    let (stdout, stderr, ok) = run_mcc(&root, &["lib", "pack", pack.to_str().unwrap(), "--out", out.to_str().unwrap()]);
    assert!(ok, "pack failed: {stderr}");
    assert!(stdout.contains("Pack: packtest@0.1.0"), "report carries coordinates: {stdout}");

    let full = out.join("packtest-0.1.0.mcl");
    let thin = out.join("packtest-0.1.0.thin.mcl");
    for f in [&full, &thin] {
        let bytes = std::fs::read(f).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        assert_eq!(&bytes[0..4], &ZSTD_MAGIC, "{} must start with the zstd frame magic", f.display());
    }
    // full ⊇ thin: the datasheet only rides the full artifact.
    assert!(std::fs::metadata(&full).unwrap().len() > std::fs::metadata(&thin).unwrap().len());
}

#[test]
fn pack__refuses_format_2_without_writing() {
    let (pack, root, out) = fixture("format");
    let bad = manifest(&sha256_hex(DATASHEET.as_bytes())).replace("format = \"1\"", "format = \"2\"");
    std::fs::write(pack.join("pack.toml"), bad).unwrap();
    let (_, stderr, ok) = run_mcc(&root, &["lib", "pack", pack.to_str().unwrap(), "--out", out.to_str().unwrap()]);
    assert!(!ok, "format=2 must not pack");
    assert!(stderr.contains("format = 2"), "names the offending format: {stderr}");
    assert!(out.read_dir().unwrap().next().is_none(), "no archive may exist");
}

#[test]
fn pack__refuses_variant_base_missing_from_entry() {
    let (pack, root, out) = fixture("variant");
    let bad = manifest(&sha256_hex(DATASHEET.as_bytes()))
        .replace("base = \"PKTEST\"", "base = \"PKTEST_ABSENT\"");
    std::fs::write(pack.join("pack.toml"), bad).unwrap();
    let (_, stderr, ok) = run_mcc(&root, &["lib", "pack", pack.to_str().unwrap(), "--out", out.to_str().unwrap()]);
    assert!(!ok, "declared base must be on the entry face");
    assert!(stderr.contains("not present on the entry source face"), "names the missing base: {stderr}");
    assert!(out.read_dir().unwrap().next().is_none(), "no archive may exist");
}

#[test]
fn pack__refuses_wrong_attachment_checksum() {
    let (pack, root, out) = fixture("cksum");
    let bogus = "0".repeat(64); // manifest() adds the sha256: prefix
    let bad = manifest(&bogus);
    std::fs::write(pack.join("pack.toml"), bad).unwrap();
    let (_, stderr, ok) = run_mcc(&root, &["lib", "pack", pack.to_str().unwrap(), "--out", out.to_str().unwrap()]);
    assert!(!ok, "declared checksum must match the bytes");
    assert!(stderr.contains("checksum mismatch"), "names the mismatch: {stderr}");
    assert!(out.read_dir().unwrap().next().is_none(), "no archive may exist");
}

// inspect

#[test]
fn inspect__dumps_manifest_without_installing() {
    let (pack, root, out) = fixture("inspect");
    let (s, stderr, ok) = run_mcc(&root, &["lib", "pack", pack.to_str().unwrap(), "--out", out.to_str().unwrap()]);
    assert!(ok, "pack failed: {stderr}");
    let full = out.join("packtest-0.1.0.mcl");

    let (stdout, stderr, ok) = run_mcc(&root, &["lib", "inspect", full.to_str().unwrap()]);
    assert!(ok, "inspect failed: {stderr}");
    assert!(stdout.contains("name: packtest"), "coordinates: {stdout}");
    assert!(stdout.contains("variant: PKTEST_3_3 base=PKTEST"), "variant table: {stdout}");
    assert!(stdout.contains("datasheet.txt"), "attachment listing: {stdout}");
    assert!(stdout.contains(&sha256_hex(DATASHEET.as_bytes())), "attachment checksum: {stdout}");
    // Pure inspection: no pack may land in root (bootstrap config/logs/index don't count).
    assert!(!root.join("packtest@0.1.0").exists(), "inspect must not install");
}

// install

fn pack_first(root: &Path, pack: &Path, out: &Path) -> PathBuf {
    let (_, stderr, ok) = run_mcc(root, &["lib", "pack", pack.to_str().unwrap(), "--out", out.to_str().unwrap()]);
    assert!(ok, "pack failed: {stderr}");
    out.join("packtest-0.1.0.mcl")
}

#[test]
fn install__lands_name_at_version_and_reindexes() {
    let (pack, root, out) = fixture("install");
    let full = pack_first(&root, &pack, &out);

    // No <name> argument: an .mcl carries its own coordinates.
    let (stdout, stderr, ok) = run_mcc(&root, &["lib", "install", "--from", full.to_str().unwrap()]);
    assert!(ok, "install failed: {stderr}");
    assert!(stdout.contains("packtest@0.1.0") || stderr.contains("packtest@0.1.0"),
            "reports the coordinates: {stdout}{stderr}");

    let landed = root.join("packtest@0.1.0");
    assert!(landed.join("packtest.mc").is_file(), "entry lands");
    assert!(landed.join("datasheet.txt").is_file(), "attachments land");
    assert!(landed.join("pack.toml").is_file(), "manifest lands");
    let index = std::fs::read_to_string(root.join("index.json")).unwrap();
    assert!(index.contains("\"packtest\""), "index lists the pack: {index}");

    // A contradicting caller-supplied name must not install over it.
    let (_, stderr, ok) = run_mcc(
        &root,
        &["lib", "install", "othername", "--from", full.to_str().unwrap()],
    );
    assert!(!ok, "name contradiction must refuse");
    assert!(stderr.contains("contradicting"), "names the contradiction: {stderr}");
}

#[test]
fn install__thin_lands_entry_without_attachments() {
    let (pack, root, out) = fixture("thin");
    let (_, stderr, ok) = run_mcc(&root, &["lib", "pack", pack.to_str().unwrap(), "--out", out.to_str().unwrap()]);
    assert!(ok, "pack failed: {stderr}");
    let thin = out.join("packtest-0.1.0.thin.mcl");

    let (_, stderr, ok) = run_mcc(&root, &["lib", "install", "--from", thin.to_str().unwrap()]);
    assert!(ok, "thin install failed: {stderr}");
    let landed = root.join("packtest@0.1.0");
    assert!(landed.join("packtest.mc").is_file(), "entry lands");
    assert!(landed.join("pack.toml").is_file(), "manifest lands");
    assert!(!landed.join("datasheet.txt").exists(), "thin carries no attachments");
}

#[test]
fn install__refuses_tampered_archive() {
    let (pack, root, out) = fixture("tamper");
    let full = pack_first(&root, &pack, &out);
    let tampered = out.join("tampered.mcl");
    let mut bytes = std::fs::read(&full).unwrap();
    let last = bytes.len() - 8;
    bytes[last] ^= 0xff;
    std::fs::write(&tampered, &bytes).unwrap();

    let (_, stderr, ok) = run_mcc(&root, &["lib", "install", "--from", tampered.to_str().unwrap()]);
    assert!(!ok, "a corrupted frame must refuse");
    assert!(
        stderr.contains("decode failed") || stderr.contains("is not an .mcl"),
        "names the corruption: {stderr}"
    );
    assert!(!root.join("packtest@0.1.0").exists(), "nothing lands");
}

#[test]
fn install__refuses_manifest_less_archive() {
    // Hand-built tar+zstd without pack.toml: check (1) (manifest present) is the
    // gate this locks — a valid zstd frame is not enough.
    let (pack, root, out) = fixture("nomanifest");
    let _ = pack_first(&root, &pack, &out);

    let bare = out.join("bare.mcl");
    let f = std::fs::File::create(&bare).unwrap();
    let enc = zstd::stream::Encoder::new(f, 1).unwrap();
    let mut tar = tar::Builder::new(enc);
    tar.append_path_with_name(pack.join("packtest.mc"), "packtest.mc").unwrap();
    tar.into_inner().unwrap().finish().unwrap();

    let (_, stderr, ok) = run_mcc(&root, &["lib", "install", "--from", bare.to_str().unwrap()]);
    assert!(!ok, "manifest-less archive must refuse");
    assert!(stderr.contains("check (1)") || stderr.contains("pack.toml"), "names check ①: {stderr}");
    assert!(!root.join("packtest@0.1.0").exists(), "nothing lands");
}
