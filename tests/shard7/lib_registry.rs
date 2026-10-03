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

// update + fetch-docs (P4)

/// The lock pin nobody can build on: build refuses (and may not rewrite);
/// `lib update` is the only rewrite authority and the rebuild is green.
#[test]
fn update__rewrites_the_lock_the_way_build_never_may() {
    let f = fixture("update", "regtest = \"*\"");
    let (_, stderr, ok) = in_proj(&f, &["build", "-f", "json"]);
    assert!(ok, "first build failed: {stderr}");

    // A pin to a version the registry never had: the lock-first solve fails.
    std::fs::write(
        f.proj.join("mcode.lock"),
        "mcode = { rev = \"9999\" }\n\n[regtest]\nversion = \"0.9\"\n",
    )
    .unwrap();
    let (_, stderr, ok) = in_proj(&f, &["build", "-f", "json"]);
    assert!(!ok, "a lock pin to a missing version must fail the build: {stderr}");

    // update drops the whole lock, re-selects fresh, and rewrites — the
    // reproduction credential is whole again.
    let (_, stderr, ok) = in_proj(&f, &["lib", "update"]);
    assert!(ok, "lib update failed: {stderr}");
    assert!(stderr.contains("updated"), "reports the rewrite: {stderr}");
    let lock = std::fs::read_to_string(f.proj.join("mcode.lock")).unwrap();
    assert!(lock.contains("0.1"), "the lock carries the solved version: {lock}");
    assert!(lock.contains("rev"), "the lock keeps the mcode stdlib-rail rev: {lock}");

    let (_, stderr, ok) = in_proj(&f, &["build", "-f", "json"]);
    assert!(ok, "rebuild on the updated lock failed: {stderr}");
}

/// `lib update <key>` refreshes only that key; the other locked entries
/// survive byte for byte (the untouched credential is not the updater's to
/// re-select), and an undeclared name is refused.
#[test]
fn update__one_name_refreshes_only_that_key() {
    let f = fixture("update1", "regtest = \"*\"\npacka = \"*\"");
    let aux = f.base.join("pack-packa");
    std::fs::create_dir_all(&aux).unwrap();
    std::fs::write(aux.join("packa.mc"), DEP_ENTRY).unwrap();
    std::fs::write(aux.join("pack.toml"), pack_manifest("packa", "packa.mc", &[])).unwrap();
    f.register(&aux, "packa", &[], &[]);

    let (_, stderr, ok) = in_proj(&f, &["build", "-f", "json"]);
    assert!(ok, "first build failed: {stderr}");

    // Break only regtest's pin; packa's entry stays exactly as solved.
    let lock = std::fs::read_to_string(f.proj.join("mcode.lock")).unwrap();
    let broken = lock.replace("[regtest]\nversion = \"0.1\"", "[regtest]\nversion = \"0.9\"");
    assert_ne!(lock, broken, "the fixture lock names regtest");
    std::fs::write(f.proj.join("mcode.lock"), &broken).unwrap();
    let aux_before = lock
        .split("[packa]")
        .nth(1)
        .unwrap_or("")
        .to_string();

    let (_, stderr, ok) = in_proj(&f, &["lib", "update", "regtest"]);
    assert!(ok, "single-name update failed: {stderr}");
    let after = std::fs::read_to_string(f.proj.join("mcode.lock")).unwrap();
    assert!(
        after.contains("version = \"0.1\""),
        "regtest re-solved to the available version: {after}"
    );
    let aux_after = after.split("[packa]").nth(1).unwrap_or("");
    assert_eq!(aux_before, aux_after, "the untouched key keeps its lock face");

    let (_, stderr, ok) = in_proj(&f, &["lib", "update", "nosuch"]);
    assert!(!ok, "an undeclared key is refused");
    assert!(stderr.contains("not declared"), "names the refusal: {stderr}");
}

/// fetch-docs: the thin install carries no bundled attachments (thin normal
/// state); fetch-docs extracts them from the full tier (per-file checksum
/// audited) and prints linked pointers as-is.
#[test]
fn fetch_docs__pulls_bundled_and_prints_linked() {
    let f = fixture("docs", "regtest = \"*\"");

    // Give the pack a material face: one bundled datasheet, one linked note.
    let pack = f.base.join("pack-regtest");
    let ds_rel = "datasheet/DS_regtest.pdf";
    let ds_body = b"%PDF-1.4 regtest datasheet rev 2.70";
    std::fs::create_dir_all(pack.join("datasheet")).unwrap();
    std::fs::write(pack.join(ds_rel), ds_body).unwrap();
    let ds_sum = {
        let mut h = Sha256::new();
        h.update(ds_body);
        format!("sha256:{:x}", h.finalize())
    };
    std::fs::write(
        pack.join("pack.toml"),
        format!(
            "{}{}\n[[attachments]]\nkind = \"datasheet\"\npath = \"{ds_rel}\"\nrev = \"2.70\"\nchecksum = \"{ds_sum}\"\n\n[[attachments]]\nkind = \"doc\"\nname = \"regtest-appnote\"\nurl = \"https://example.com/regtest-appnote.pdf\"\nrev = \"1.0\"\n",
            pack_manifest("regtest", "regtest.mc", &[]),
            variant_section()
        ),
    )
    .unwrap();
    // Re-register: the tree's artifacts now carry the attachment (full tier).
    f.register(&pack, "regtest", &["REGTEST-3.3"], &[]);

    let (_, stderr, ok) = in_proj(&f, &["build", "-f", "json"]);
    assert!(ok, "build failed: {stderr}");
    let installed = f.root.join("regtest@0.1");
    assert!(
        !installed.join(ds_rel).exists(),
        "the thin install omits bundled attachments (thin normal state)"
    );

    let (_, stderr, ok) = in_proj(&f, &["lib", "fetch-docs", "regtest"]);
    assert!(ok, "fetch-docs failed: {stderr}");
    assert!(
        stderr.contains("fetched") && stderr.contains(ds_rel),
        "reports the extraction: {stderr}"
    );
    assert!(
        stderr.contains("https://example.com/regtest-appnote.pdf"),
        "linked pointers print their url: {stderr}"
    );
    assert_eq!(
        std::fs::read(installed.join(ds_rel)).unwrap(),
        ds_body,
        "the extracted attachment is byte-identical"
    );

    // Second run: everything present — nothing is downloaded again.
    let (_, stderr, ok) = in_proj(&f, &["lib", "fetch-docs", "regtest"]);
    assert!(ok, "second fetch-docs failed: {stderr}");
    assert!(
        stderr.contains("all bundled attachments present"),
        "a no-op fetch says so: {stderr}"
    );
    assert!(!stderr.contains("fetched"), "no re-extraction: {stderr}");
}

// the P3 HTTP face: an in-process registry host (no external server)

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

struct ServeCfg {
    /// Document root (the registry tree).
    root: PathBuf,
    /// When set, `/lib/` paths answer with this status (error-face tests).
    fail_status: Option<u16>,
    /// When set, `/dl/` bodies are cut to this many bytes (truncation face).
    truncate_dl: Option<usize>,
}

/// A one-test static registry host: ETag on every 200 (sha256 of the bytes),
/// If-None-Match honoured with a 304, real 404s, optional fault injection.
/// Every answer is logged as `GET <path> <status>` for the tests to assert
/// against — the access log IS the wire-level observation.
struct MiniRegistry {
    url: String,
    addr: std::net::SocketAddr,
    log: Arc<Mutex<Vec<String>>>,
    shutdown: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl MiniRegistry {
    fn start(root: &Path, fail_status: Option<u16>, truncate_dl: Option<usize>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mini registry");
        let addr = listener.local_addr().unwrap();
        let cfg = Arc::new(ServeCfg {
            root: root.to_path_buf(),
            fail_status,
            truncate_dl,
        });
        let log = Arc::new(Mutex::new(Vec::new()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let handle = {
            let cfg = cfg.clone();
            let log = log.clone();
            let shutdown = shutdown.clone();
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    if shutdown.load(Ordering::SeqCst) {
                        break;
                    }
                    let Ok(s) = stream else { continue };
                    let cfg = cfg.clone();
                    let log = log.clone();
                    std::thread::spawn(move || {
                        let _ = handle_conn(s, &cfg, &log);
                    });
                }
            })
        };
        Self {
            url: format!("http://{addr}"),
            addr,
            log,
            shutdown,
            handle: Some(handle),
        }
    }

    fn served(&self, line: &str) -> bool {
        self.log.lock().unwrap().iter().any(|l| l.contains(line))
    }

    #[allow(dead_code)]
    fn log_lines(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
}

impl Drop for MiniRegistry {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.addr); // break the blocking accept
        if let Some(h) = self.handle.take() {
            h.join().unwrap();
        }
    }
}

fn handle_conn(mut s: TcpStream, cfg: &ServeCfg, log: &Mutex<Vec<String>>) -> std::io::Result<()> {
    // Read the request head (headers only; GETs carry no body).
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let n = s.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    let text = String::from_utf8_lossy(&buf);
    let mut lines = text.split("\r\n");
    let path = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .unwrap_or("/")
        .to_string();
    let inm = lines
        .filter_map(|l| l.split_once(':'))
        .find(|(k, _)| k.trim().eq_ignore_ascii_case("if-none-match"))
        .map(|(_, v)| v.trim().to_string());

    let clean = path.split('?').next().unwrap_or("/").trim_start_matches('/');
    let record = |status: u16| log.lock().unwrap().push(format!("GET /{clean} {status}"));

    let resp: Vec<u8> = if clean.starts_with("lib/") && cfg.fail_status.is_some() {
        let status = cfg.fail_status.unwrap();
        record(status);
        format!("HTTP/1.1 {status} Fault\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").into_bytes()
    } else {
        let fpath = cfg.root.join(clean);
        if !fpath.is_file() {
            record(404);
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()
        } else {
            let mut body = std::fs::read(&fpath)?;
            let etag = format!("\"{:x}\"", Sha256::digest(&body));
            if inm.as_deref() == Some(etag.as_str()) {
                record(304);
                format!("HTTP/1.1 304 Not Modified\r\nETag: {etag}\r\nConnection: close\r\n\r\n")
                    .into_bytes()
            } else {
                if let Some(n) = cfg.truncate_dl {
                    if clean.starts_with("dl/") {
                        body.truncate(n);
                    }
                }
                record(200);
                let mut resp = format!(
                    "HTTP/1.1 200 OK\r\nETag: {etag}\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .into_bytes();
                resp.extend_from_slice(&body);
                resp
            }
        }
    };
    s.write_all(&resp)
}

/// The fixture manifest with an arbitrary registry url (the http face).
fn manifest_with_url(deps: &str, url: &str) -> String {
    format!(
        "[project]\nname = \"p\"\nversion = \"0.1\"\nentry = \"src/main.mc\"\n\n\
         [dependencies]\nmcode = \"*\"\n{deps}\n\n\
         [config.registry]\nurl = \"{url}\"\n"
    )
}

#[test]
fn http__build_installs_then_revalidates_the_cached_meta_with_304() {
    let f = fixture("http-304", "regtest = \"*\"");
    let srv = MiniRegistry::start(&f.reg, None, None);
    std::fs::write(
        f.proj.join("project.toml"),
        manifest_with_url("regtest = \"*\"", &srv.url),
    )
    .unwrap();

    let (_, stderr, ok) = in_proj(&f, &["build"]);
    assert!(ok, "http build failed: {stderr}");
    assert!(stderr.contains("installed regtest@"), "installed: {stderr}");
    assert!(f.root.join("regtest@0.1").is_dir(), "landed in the data root");
    assert!(
        srv.served("/lib/regtest.json 200"),
        "meta fetched over the wire: {:?}",
        srv.log_lines()
    );

    // Force a re-solve: drop the lock and the install. The cached meta and
    // its parked ETag must carry the rebuild — the server answers 304 this
    // time and the client serves the cached copy.
    std::fs::remove_file(f.proj.join("mcode.lock")).unwrap();
    std::fs::remove_dir_all(f.root.join("regtest@0.1")).unwrap();
    let (_, stderr, ok) = in_proj(&f, &["build"]);
    assert!(ok, "cache-carried rebuild failed: {stderr}");
    assert!(
        srv.served("/lib/regtest.json 304"),
        "expected a 304 revalidation: {:?}",
        srv.log_lines()
    );
    assert!(f.root.join("regtest@0.1").is_dir(), "reinstalled from cache");
}

#[test]
fn http__unknown_name_is_unresolved_and_the_404_is_seen() {
    let f = fixture("http-404", "nosuch = \"*\"");
    let srv = MiniRegistry::start(&f.reg, None, None);
    std::fs::write(
        f.proj.join("project.toml"),
        manifest_with_url("nosuch = \"*\"", &srv.url),
    )
    .unwrap();

    let (_, stderr, ok) = in_proj(&f, &["build"]);
    assert!(!ok, "an unknown name must fail the build");
    assert!(stderr.contains("nosuch"), "the missing lib is named: {stderr}");
    assert!(
        srv.served("/lib/nosuch.json 404"),
        "the 404 answer went out: {:?}",
        srv.log_lines()
    );
}

#[test]
fn http__a_failing_registry_is_named_not_hung() {
    let f = fixture("http-500", "regtest = \"*\"");
    let srv = MiniRegistry::start(&f.reg, Some(500), None);
    std::fs::write(
        f.proj.join("project.toml"),
        manifest_with_url("regtest = \"*\"", &srv.url),
    )
    .unwrap();

    let (_, stderr, ok) = in_proj(&f, &["build"]);
    assert!(!ok, "a 500 registry must fail the build");
    assert!(stderr.contains("HTTP 500"), "named, not swallowed: {stderr}");
    assert!(
        srv.served("/lib/regtest.json 500"),
        "the fault was injected where aimed: {:?}",
        srv.log_lines()
    );
}

#[test]
fn http__truncated_artifact_fails_the_checksum_with_no_half_install() {
    let f = fixture("http-trunc", "regtest = \"*\"");
    let srv = MiniRegistry::start(&f.reg, None, Some(64));
    std::fs::write(
        f.proj.join("project.toml"),
        manifest_with_url("regtest = \"*\"", &srv.url),
    )
    .unwrap();

    let (_, stderr, ok) = in_proj(&f, &["build"]);
    assert!(!ok, "a truncated artifact must fail the build");
    assert!(
        stderr.contains("checksum mismatch"),
        "names the integrity failure: {stderr}"
    );
    assert!(
        !f.root.join("regtest@0.1").exists(),
        "no half install over http either"
    );
    assert!(
        srv.served("/dl/power/regtest/0.1/regtest-0.1.thin.mcl 200"),
        "the artifact came over the wire: {:?}",
        srv.log_lines()
    );
}
