// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Project-local library install — the cargo-style law (use-design §19.10,
//! registry-design.md as amended by the project-vendored layout).
//!
//! What is locked here, per surface:
//!
//! * **install defaults to the project tier** — a third-party library lands
//!   in `<project>/libs/<name>@<ver>` and never in the global data root; no
//!   project above the cwd is a hard error pointing at the missing manifest;
//! * **the global data root is official-library territory** — mcode installs
//!   there as a versioned `mcode@<ver>` copy, and `--global` is refused for
//!   third-party names;
//! * **`lib list` merges both tiers** — origin labels, project first, and a
//!   duplicate-name warning stating the project copy wins;
//! * **uninstall resolves project-first**, `--global` takes the data-root
//!   copy;
//! * **an unmet exact pin degrades loudly** — the load proceeds with a
//!   warning carrying the `mcc lib install` hint, never a silent version
//!   swap.
//!
//! Every call runs against a private `MCC_SYSTEM_ROOT` with `--local` (the
//! daemon cannot see this cwd — the project tier is a client-side fact).

use std::path::{Path, PathBuf};
use std::process::Command;

/// A minimal third-party library directory (entry basename must match the
/// pack-name law for bare-dir installs the lib name is given explicitly).
/// Shape lifted from the lib_pack fixture (abstract base + one orderable
/// SKU, no external lib) — that shape is proven to parse both as an entry
/// face and as a loaded-library face.
const LIB_ENTRY: &str = r#"// ztestpack fixture part (no external lib)
abstract component ZTESTPACK(v_out::UV.VOLT = 3.3V)
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

component ZTESTPACK_3_3 : ZTESTPACK
{
    partno = "ZTESTPACK-3.3"
}
"#;

const MCODE_STUB: &str = "// stub official library: never loaded, only installed\n";

struct Fixture {
    base: PathBuf,
    /// The private MCC_SYSTEM_ROOT (global data root).
    root: PathBuf,
    /// A project root with `[dependencies] mcode = "*"` (+ extra raw text).
    proj: PathBuf,
    /// A third-party library source directory named `ztestpack`.
    lib_src: PathBuf,
    /// A fake official-library source directory (installs as mcode).
    mcode_src: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

fn fixture(tag: &str) -> Fixture {
    let base = std::env::temp_dir().join(format!(
        "mcc-libproj-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join("root");
    let proj = base.join("proj");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(proj.join("src")).unwrap();
    std::fs::write(
        proj.join("project.toml"),
        "[project]\nname = \"p\"\nversion = \"0.1\"\nentry = \"src/main.mc\"\n\n\
         [dependencies]\nmcode = \"*\"\n",
    )
    .unwrap();
    std::fs::write(proj.join("src/main.mc"), "module main()\n{\n}\n").unwrap();
    let lib_src = base.join("ztestpack");
    std::fs::create_dir_all(&lib_src).unwrap();
    std::fs::write(lib_src.join("ztestpack.mc"), LIB_ENTRY).unwrap();
    let mcode_src = base.join("mcodesrc");
    std::fs::create_dir_all(&mcode_src).unwrap();
    std::fs::write(mcode_src.join("mcode.mc"), MCODE_STUB).unwrap();
    Fixture {
        base,
        root,
        proj,
        lib_src,
        mcode_src,
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

fn run_in_proj(f: &Fixture, args: &[&str]) -> (String, String, bool) {
    run_mcc_in(&f.proj, &f.root, args)
}

fn run_bare(root: &Path, args: &[&str]) -> (String, String, bool) {
    run_mcc_in(&std::env::temp_dir(), root, args)
}

// install target law

#[test]
fn install__without_project_is_a_hard_error_naming_the_law() {
    let f = fixture("noproj");
    let (_, stderr, ok) = run_bare(
        &f.root,
        &[
            "lib",
            "install",
            "ztestpack",
            "--from",
            f.lib_src.to_str().unwrap(),
        ],
    );
    assert!(!ok, "third-party install without a project must fail");
    assert!(
        stderr.contains("project.toml") && stderr.contains("<project>/libs"),
        "names the missing manifest and the vendor target: {stderr}"
    );
    assert!(!f.root.join("ztestpack@0.0").exists(), "nothing lands globally");
}

#[test]
fn install__vendors_into_project_libs_and_never_the_data_root() {
    let f = fixture("vendor");
    let (stdout, stderr, ok) = run_in_proj(
        &f,
        &[
            "lib",
            "install",
            "ztestpack",
            "--from",
            f.lib_src.to_str().unwrap(),
            "--version",
            "0.3",
        ],
    );
    assert!(ok, "install failed: {stderr}");
    assert!(
        stdout.contains("ztestpack@0.3") || stderr.contains("ztestpack@0.3"),
        "reports coordinates: {stdout}{stderr}"
    );

    let landed = f.proj.join("libs").join("ztestpack@0.3");
    assert!(landed.join("ztestpack.mc").is_file(), "entry lands in <proj>/libs");
    assert!(
        !f.root.join("ztestpack@0.3").exists(),
        "the data root must stay official-only"
    );
    // index.json is a global-root face; project tiers list by scan.
    if let Ok(index) = std::fs::read_to_string(f.root.join("index.json")) {
        assert!(!index.contains("\"ztestpack\""), "project install stays out of the global index: {index}");
    }
}

#[test]
fn install__mcode_lands_global_and_versioned() {
    let f = fixture("mcode-install");
    let (stdout, stderr, ok) = run_in_proj(
        &f,
        &[
            "lib",
            "install",
            "mcode",
            "--from",
            f.mcode_src.to_str().unwrap(),
            "--version",
            "0.5",
        ],
    );
    assert!(ok, "mcode install failed: {stderr}");
    assert!(
        stdout.contains("mcode@0.5") || stderr.contains("mcode@0.5"),
        "reports the versioned copy: {stdout}{stderr}"
    );
    assert!(f.root.join("mcode@0.5").join("mcode.mc").is_file(), "versioned copy lands globally");
    assert!(
        !f.proj.join("libs").join("mcode@0.5").exists(),
        "mcode is never vendored into a project"
    );
    let index = std::fs::read_to_string(f.root.join("index.json")).unwrap();
    assert!(index.contains("\"mcode\""), "global installs reindex: {index}");
}

#[test]
fn install__global_flag_is_refused_for_third_party() {
    let f = fixture("global-refused");
    let (_, stderr, ok) = run_in_proj(
        &f,
        &[
            "lib",
            "install",
            "ztestpack",
            "--from",
            f.lib_src.to_str().unwrap(),
            "--global",
        ],
    );
    assert!(!ok, "--global is mcode-only");
    assert!(
        stderr.contains("reserved for mcode"),
        "names the official-library law: {stderr}"
    );
}

#[test]
fn install__target_exists_is_an_error() {
    let f = fixture("exists");
    let args = [
        "lib",
        "install",
        "ztestpack",
        "--from",
        f.lib_src.to_str().unwrap(),
        "--version",
        "0.3",
    ];
    let (_, stderr, ok) = run_in_proj(&f, &args);
    assert!(ok, "first install failed: {stderr}");
    let (_, stderr, ok) = run_in_proj(&f, &args);
    assert!(!ok, "reinstall over an existing target must refuse");
    assert!(stderr.contains("already installed"), "names the conflict: {stderr}");
}

// list

#[test]
fn list__merges_tiers_with_origin_and_duplicate_warning() {
    let f = fixture("list");
    // Project tier: ztestpack@0.3.
    let (_, stderr, ok) = run_in_proj(
        &f,
        &[
            "lib",
            "install",
            "ztestpack",
            "--from",
            f.lib_src.to_str().unwrap(),
            "--version",
            "0.3",
        ],
    );
    assert!(ok, "project install failed: {stderr}");
    // Global tier: a same-name copy at another version (hand-made; the data
    // root is install-refused for third-party, but legacy installs exist).
    std::fs::create_dir_all(f.root.join("ztestpack@0.2")).unwrap();

    let (stdout, _, ok) = run_in_proj(&f, &["lib", "list"]);
    assert!(ok);
    assert!(
        stdout.contains("project") && stdout.contains("global"),
        "origin labels present: {stdout}"
    );
    assert!(
        stdout.contains("the project copy wins"),
        "duplicate-name warning present: {stdout}"
    );
    // Project first.
    let proj_pos = stdout.find("ztestpack@0.3").unwrap();
    let glob_pos = stdout.find("ztestpack@0.2").unwrap();
    assert!(proj_pos < glob_pos, "project tier lists before global: {stdout}");
}

// uninstall

#[test]
fn uninstall__removes_the_project_copy_first() {
    let f = fixture("uninstall-proj");
    let install_args = [
        "lib",
        "install",
        "ztestpack",
        "--from",
        f.lib_src.to_str().unwrap(),
        "--version",
        "0.3",
    ];
    let (_, stderr, ok) = run_in_proj(&f, &install_args);
    assert!(ok, "install failed: {stderr}");
    // A legacy global copy at another version must survive a project uninstall.
    std::fs::create_dir_all(f.root.join("ztestpack@0.2")).unwrap();

    let (_, stderr, ok) = run_in_proj(&f, &["lib", "uninstall", "ztestpack"]);
    assert!(ok, "uninstall failed: {stderr}");
    assert!(
        !f.proj.join("libs").join("ztestpack@0.3").exists(),
        "project copy removed"
    );
    assert!(f.root.join("ztestpack@0.2").exists(), "global copy untouched");
}

#[test]
fn uninstall__global_flag_removes_the_data_root_copy() {
    let f = fixture("uninstall-global");
    std::fs::create_dir_all(f.root.join("ztestpack@0.2")).unwrap();

    let (_, stderr, ok) = run_in_proj(&f, &["lib", "uninstall", "ztestpack", "--global"]);
    assert!(ok, "uninstall failed: {stderr}");
    assert!(!f.root.join("ztestpack@0.2").exists(), "global copy removed");
}

// version pins

#[test]
fn pin__unmet_exact_pin_warns_with_the_install_hint() {
    let f = fixture("pin-unmet");
    // A copy IS installed — at the wrong version — so the pin degrades to it
    // loudly rather than vanishing entirely.
    let (_, stderr, ok) = run_in_proj(
        &f,
        &[
            "lib",
            "install",
            "ztestpack",
            "--from",
            f.lib_src.to_str().unwrap(),
            "--version",
            "0.2",
        ],
    );
    assert!(ok, "seed install failed: {stderr}");
    std::fs::write(
        f.proj.join("project.toml"),
        "[project]\nname = \"p\"\nversion = \"0.1\"\nentry = \"src/main.mc\"\n\n\
         [dependencies]\nmcode = \"*\"\nztestpack = \"9.9\"\n",
    )
    .unwrap();

    let (stdout, stderr, ok) = run_in_proj(&f, &["check", "src/main.mc"]);
    assert!(
        stderr.contains("project pins ztestpack@9.9"),
        "the unmet pin degrades loudly: {stderr}"
    );
    assert!(
        stderr.contains("mcc lib install ztestpack"),
        "the warning carries the install hint: {stderr}"
    );
    // Degrades loudly, not fatally: the copy that IS installed still loads.
    assert!(ok, "an unmet pin must not fail the build: {stdout}{stderr}");
}

#[test]
fn pin__exact_pin_loads_from_the_project_tier() {
    let f = fixture("pin-exact");
    let (_, stderr, ok) = run_in_proj(
        &f,
        &[
            "lib",
            "install",
            "ztestpack",
            "--from",
            f.lib_src.to_str().unwrap(),
            "--version",
            "0.3",
        ],
    );
    assert!(ok, "install failed: {stderr}");

    std::fs::write(
        f.proj.join("project.toml"),
        "[project]\nname = \"p\"\nversion = \"0.1\"\nentry = \"src/main.mc\"\n\n\
         [dependencies]\nmcode = \"*\"\nztestpack = \"0.3\"\n",
    )
    .unwrap();

    // lib load resolves through the project tier (the lib commands seed the
    // project root from the cwd manifest walk).
    let (stdout, stderr, ok) = run_in_proj(&f, &["lib", "load", "ztestpack"]);
    assert!(ok, "project-tier load failed: {stdout}{stderr}");
    assert!(stdout.contains("loaded") || stderr.contains("loaded"), "load report: {stdout}{stderr}");
}
