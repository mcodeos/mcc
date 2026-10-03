// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The P3 HTTP face end to end against a real in-process server (registry
//! registry-design.md §2.2, registry-p3-protocol.md §2): solve + install
//! download through `GET /lib/…` + `/dl/…`, `--remote` search reads
//! `/search.json`, ETag revalidation answers 304 and parks the sidecar,
//! tampered signed metadata hard-errors with no cache fallback while a
//! source outage serves the parked row, and `lib trust update` chains a new
//! key from a trusted one over `/trust.json`.
//!
//! The server is a `TcpListener` on an ephemeral port with a detached
//! accept thread — `httpfetch` builds a fresh blocking client per call, so
//! each connection is one request/response and then close. Every path keeps
//! a hit counter and a 304 counter in a shared route table; per-path status
//! and body overrides let a test stage a tamper or an outage mid-test
//! without touching the served files.

use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

// Fixture

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

fn pack_manifest(version: &str) -> String {
    format!(
        "[package]\nformat = \"1\"\nname = \"regtest\"\nversion = \"{version}\"\n\
         category = \"power\"\nentry = \"regtest.mc\"\n\n"
    )
}

const VARIANT_SECTION: &str = "[variants.REGTEST_3_3]\nbase = \"REGTEST\"\nsince = \"0.1.0\"\n";

const SEARCH_JSON: &str = r#"{"packages": [{"name": "regtest", "category": "power", "description": "the http fixture pack", "latest": "0.1"}]}"#;

fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn sha256_hex_file(p: &Path) -> String {
    sha256_hex(&std::fs::read(p).unwrap_or_else(|e| panic!("{}: {e}", p.display())))
}

// The in-process registry server

/// Per-path bookkeeping: hit and 304 counters, plus the staged overrides a
/// test installs mid-flight.
#[derive(Default)]
struct RouteState {
    hits: u32,
    not_modified: u32,
    status: Option<u16>,
    body: Option<Vec<u8>>,
}

type Routes = Arc<Mutex<BTreeMap<String, RouteState>>>;

fn route_hits(routes: &Routes, path: &str) -> (u32, u32) {
    let map = routes.lock().unwrap();
    match map.get(path) {
        Some(r) => (r.hits, r.not_modified),
        None => (0, 0),
    }
}

fn stage_status(routes: &Routes, path: &str, status: u16) {
    routes.lock().unwrap().entry(path.to_string()).or_default().status = Some(status);
}

fn stage_body(routes: &Routes, path: &str, body: Vec<u8>) {
    routes.lock().unwrap().entry(path.to_string()).or_default().body = Some(body);
}

fn respond(stream: &mut std::net::TcpStream, status: u16, etag: Option<&str>, body: &[u8]) {
    let reason = match status {
        200 => "OK",
        304 => "Not Modified",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        _ => "Unknown",
    };
    let mut head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    if let Some(e) = etag {
        head.push_str(&format!("ETag: {e}\r\n"));
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    if status != 304 {
        let _ = stream.write_all(body);
    }
    let _ = stream.flush();
}

/// One connection: read to the header terminator, dispatch, close (the
/// response advertises `Connection: close` and the stream drops).
fn handle_connection(mut stream: std::net::TcpStream, root: PathBuf, routes: Routes) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => return,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            Err(_) => return,
        }
    }
    let text = String::from_utf8_lossy(&buf);
    let mut lines = text.split("\r\n");
    let request = lines.next().unwrap_or("");
    let mut parts = request.split_whitespace();
    let method = parts.next().unwrap_or("");
    let raw_path = parts.next().unwrap_or("");
    let mut if_none_match: Option<String> = None;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("if-none-match") {
                if_none_match = Some(v.trim().to_string());
            }
        }
    }
    if method != "GET" {
        respond(&mut stream, 405, None, b"GET only");
        return;
    }
    let path = raw_path.split(['?', '#']).next().unwrap_or(raw_path).to_string();

    {
        let mut map = routes.lock().unwrap();
        let state = map.entry(path.clone()).or_default();
        state.hits += 1;
        if let Some(status) = state.status {
            let body = state.body.clone().unwrap_or_default();
            drop(map);
            if status == 304 {
                respond(&mut stream, 304, None, &[]);
            } else {
                respond(&mut stream, status, None, &body);
            }
            return;
        }
        if let Some(body) = state.body.clone() {
            drop(map);
            respond(&mut stream, 200, None, &body);
            return;
        }
    }

    // The default face: serve the file tree. Path traversal is refused —
    // the server is a registry endpoint, not a file share.
    let rel = path.trim_start_matches('/');
    if rel.is_empty() || rel.split('/').any(|seg| seg == "..") {
        respond(&mut stream, 404, None, b"not found");
        return;
    }
    let file = root.join(rel);
    let Ok(body) = std::fs::read(&file) else {
        respond(&mut stream, 404, None, b"not found");
        return;
    };
    let etag = format!("\"{}\"", &sha256_hex(&body)[..16]);
    if if_none_match.as_deref() == Some(etag.as_str()) {
        routes.lock().unwrap().get_mut(&path).unwrap().not_modified += 1;
        respond(&mut stream, 304, Some(&etag), &body);
        return;
    }
    respond(&mut stream, 200, Some(&etag), &body);
}

fn spawn_server(root: PathBuf, routes: Routes) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let root = root.clone();
            let routes = routes.clone();
            std::thread::spawn(move || handle_connection(stream, root, routes));
        }
    });
    format!("http://{addr}")
}

// The fixture: a regtest pack in a served tree, a project pointed at the
// tree over http://, everything under a private MCC_SYSTEM_ROOT.

struct HttpFixture {
    base: PathBuf,
    root: PathBuf,
    proj: PathBuf,
    reg: PathBuf,
    url: String,
    routes: Routes,
}

impl Drop for HttpFixture {
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

fn in_proj(f: &HttpFixture, args: &[&str]) -> (String, String, bool) {
    run_mcc_in(&f.proj, &f.root, args)
}

impl HttpFixture {
    /// Write `src/main.mc` + `project.toml` under a fresh project dir; the
    /// dir is returned so a test can hold several consumers at once.
    fn new_project(&self, tag: &str, deps: &str) -> PathBuf {
        let p = self.base.join(tag);
        std::fs::create_dir_all(p.join("src")).unwrap();
        std::fs::write(p.join("src/main.mc"), "module main()\n{\n}\n").unwrap();
        std::fs::write(
            p.join("project.toml"),
            format!(
                "[project]\nname = \"p\"\nversion = \"0.1\"\nentry = \"src/main.mc\"\n\n\
                 [dependencies]\nmcode = \"*\"\n{deps}\n\n\
                 [config.registry]\nurl = \"{}\"\n",
                self.url
            ),
        )
        .unwrap();
        p
    }
}

/// Pack regtest via the real `mcc lib pack`; returns the pack-out dir.
fn build_pack(base: &Path, root: &Path, pack_version: &str) -> PathBuf {
    let pack = base.join("pack");
    std::fs::create_dir_all(&pack).unwrap();
    std::fs::write(pack.join("regtest.mc"), ENTRY).unwrap();
    std::fs::write(
        pack.join("pack.toml"),
        format!("{}{VARIANT_SECTION}", pack_manifest(pack_version)),
    )
    .unwrap();
    let out = base.join("pack-out");
    std::fs::create_dir_all(&out).unwrap();
    let (_, stderr, ok) = run_mcc_in(
        base,
        root,
        &["lib", "pack", pack.to_str().unwrap(), "--out", out.to_str().unwrap()],
    );
    assert!(ok, "pack failed: {stderr}");
    out
}

fn new_fixture(tag: &str) -> (HttpFixture, PathBuf) {
    let base = std::env::temp_dir().join(format!(
        "mcc-libreghttp-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join("sysroot");
    let reg = base.join("reg");
    let proj = base.join("proj");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&proj.join("src")).unwrap();
    // The registry tree exists (a reachable root) with its lib/ face —
    // publish applies its delta into it.
    std::fs::create_dir_all(reg.join("lib")).unwrap();
    let routes: Routes = Arc::new(Mutex::new(BTreeMap::new()));
    let f = HttpFixture {
        base,
        root: root.clone(),
        proj,
        reg,
        url: String::new(),
        routes,
    };
    (f, root)
}

fn finish_fixture(mut f: HttpFixture) -> HttpFixture {
    let url = spawn_server(f.reg.clone(), f.routes.clone());
    f.url = url;
    let _ = f.new_project("proj", "regtest = \"*\"");
    // `proj` is the canonical first consumer; new_project returns its path.
    f.proj = f.base.join("proj");
    f
}

/// The unsigned fixture: artifacts copied in by hand with hand-written
/// metadata (the mkregistry.sh shape).
fn fixture(tag: &str) -> HttpFixture {
    let (f, root) = new_fixture(tag);
    let out = build_pack(&f.base, &root, "0.1.0");
    let dir = f.reg.join("dl/power/regtest/0.1");
    std::fs::create_dir_all(&dir).unwrap();
    for (src_name, dst_name) in [
        ("regtest-0.1.0.thin.mcl", "regtest-0.1.thin.mcl"),
        ("regtest-0.1.0.mcl", "regtest-0.1.mcl"),
    ] {
        std::fs::copy(out.join(src_name), dir.join(dst_name)).unwrap();
    }
    let thin_sum = format!("sha256:{}", sha256_hex_file(&dir.join("regtest-0.1.thin.mcl")));
    let full_sum = format!("sha256:{}", sha256_hex_file(&dir.join("regtest-0.1.mcl")));
    let meta = format!(
        "{{\"name\": \"regtest\", \"category\": \"power\", \"description\": \"the http fixture pack\", \
         \"versions\": {{\"0.1\": {{\"checksum\": \"{full_sum}\", \"thin_checksum\": \"{thin_sum}\", \
         \"size\": 1, \"deps\": {{}}, \"variants\": [\"REGTEST-3.3\"]}}}}}}"
    );
    std::fs::create_dir_all(f.reg.join("lib")).unwrap();
    std::fs::write(f.reg.join("lib/regtest.json"), meta).unwrap();
    finish_fixture(f)
}

/// The signed fixture: the tree is written by a real `mcc lib publish --go`
/// configured with `key` (the publisher's exact wire shapes, not a
/// hand-built approximation). `--go` writes both metadata and search.json.
fn signed_fixture(tag: &str, key: &SigningKey) -> HttpFixture {
    let (f, root) = new_fixture(tag);
    // Pack once (creates `<base>/pack`); publish re-packs from that dir.
    let _ = build_pack(&f.base, &root, "0.1.0");
    std::fs::create_dir_all(f.root.join("config")).unwrap();
    let seed = f.base.join("publisher.ed25519");
    write_seed(&seed, key);
    std::fs::write(
        f.root.join("config/mcc.yaml"),
        format!(
            "registry:\n  url: \"file://{}\"\n  publish:\n    key: \"{}\"\n    transport: \"none\"\n",
            f.reg.display(),
            seed.display()
        ),
    )
    .unwrap();
    let (_, stderr, ok) = run_mcc_in(
        &f.base,
        &root,
        &["lib", "publish", f.base.join("pack").to_str().unwrap(), "--go"],
    );
    assert!(ok, "signed publish failed: {stderr}");
    finish_fixture(f)
}

// Signing helpers (Ed25519 keys from fixed seeds; the wire shapes are the
// registry-p3-protocol.md §3 ones).

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn key_from_seed(seed: [u8; 32]) -> SigningKey {
    SigningKey::from_bytes(&seed)
}

/// Write the hex seed to `path` (the keygen file shape the publish config
/// reads).
fn write_seed(path: &Path, key: &SigningKey) {
    std::fs::write(path, hex_encode(&key.to_bytes())).unwrap();
}

/// Sign a trust row: canonical JSON of the row minus `sig`, `ed25519:<b64>`
/// (the ⑦b wire law verify_trust_row checks).
fn sign_trust_row(row: &serde_json::Value, key: &SigningKey) -> String {
    let mut v = row.clone();
    v.as_object_mut().unwrap().remove("sig");
    let msg = mcc::canonical_json(&v);
    use base64::Engine;
    format!(
        "ed25519:{}",
        base64::engine::general_purpose::STANDARD.encode(key.sign(msg.as_bytes()).to_bytes())
    )
}

fn keyid_of(key: &SigningKey) -> String {
    mcc::keyid_of(&key.verifying_key())
}

fn public_of(key: &SigningKey) -> String {
    hex_encode(&key.verifying_key().to_bytes())
}

/// Seed a trust store row for `key` (the file `lib trust update` merges into;
/// stands in for a factory anchor — the shipped table is empty by design).
fn seed_trust_store(root: &Path, key: &SigningKey) {
    std::fs::create_dir_all(root.join("config")).unwrap();
    std::fs::write(
        root.join("config/trust.toml"),
        format!(
            "[[keys]]\nkeyid = \"{}\"\npublic = \"{}\"\n",
            keyid_of(key),
            public_of(key)
        ),
    )
    .unwrap();
}

fn trust_row(key: &SigningKey, signer: &SigningKey) -> serde_json::Value {
    let mut r = serde_json::json!({
        "keyid": keyid_of(key),
        "public": public_of(key),
        "signer": keyid_of(signer),
    });
    r["sig"] = serde_json::json!(sign_trust_row(&r, signer));
    r
}

fn serve_trust_json(f: &HttpFixture, rows: &[serde_json::Value]) {
    std::fs::write(
        f.reg.join("trust.json"),
        serde_json::to_string(&serde_json::json!({ "keys": rows })).unwrap(),
    )
    .unwrap();
}

// Tests

/// The full download path: solve reads `/lib/regtest.json`, the artifact
/// comes from `/dl/…`, and the pack lands in `deps/` (`--here`).
#[test]
fn http__install_solves_downloads_and_lands_in_the_data_root() {
        let f = fixture("install");
        let (_, stderr, ok) = in_proj(&f, &["lib", "install", "regtest@*", "--here"]);
        assert!(ok, "install failed: {stderr}");
        assert!(f.proj.join("deps/regtest@0.1").is_dir(), "deps placement: {stderr}");

        let (meta_hits, _) = route_hits(&f.routes, "/lib/regtest.json");
        assert!(meta_hits >= 1, "metadata was fetched over http");
        let (dl_hits, _) = route_hits(&f.routes, "/dl/power/regtest/0.1/regtest-0.1.thin.mcl");
        assert!(dl_hits >= 1, "the thin artifact was downloaded over http");
        // The cached row is parked under the URL-authority shard.
        let authority = f.url.trim_start_matches("http://");
        assert!(
            f.root.join("cache/meta").join(authority).join("regtest.json").is_file(),
            "the metadata cache row parks under the authority shard"
        );
}

/// `lib search --remote` reads `/search.json` and classifies through the
/// metadata signature.
#[test]
fn http__search_remote_lists_the_catalog() {
    let f = fixture("search");
    std::fs::write(f.reg.join("search.json"), SEARCH_JSON).unwrap();
    let (_, stderr, ok) = in_proj(&f, &["lib", "search", "--remote", "regtest"]);
    assert!(ok, "remote search failed: {stderr}");
    assert!(stderr.contains("regtest@0.1"), "the catalog row prints: {stderr}");
    assert!(stderr.contains("[community]"), "unsigned metadata classifies as community: {stderr}");
}

/// The 304 conditional arm against a real server (§2.2): the first install
/// parks the row + `.etag` sidecar; the second install revalidates, the
/// server answers 304, and the cached row is served without a refetch.
#[test]
fn http__etag_revalidation_answers_304_and_parks_the_sidecar() {
        let f = fixture("etag");
        let (_, stderr, ok) = in_proj(&f, &["lib", "install", "regtest@*", "--here"]);
        assert!(ok, "first install failed: {stderr}");
        let (hits_first, _) = route_hits(&f.routes, "/lib/regtest.json");

        let authority = f.url.trim_start_matches("http://");
        let sidecar = f.root.join("cache/meta").join(authority).join("regtest.json.etag");
        let parked = std::fs::read_to_string(&sidecar).expect("etag sidecar parked");
        assert!(parked.starts_with('"'), "the sidecar holds the server's etag: {parked}");

        let p2 = f.new_project("proj2", "regtest = \"*\"");
        let (_, stderr, ok) = run_mcc_in(&p2, &f.root, &["lib", "install", "regtest@*", "--here"]);
        assert!(ok, "second install failed: {stderr}");

        let (hits, not_modified) = route_hits(&f.routes, "/lib/regtest.json");
        assert!(hits > hits_first, "the second install revalidated over http");
        // At least one read carried the parked etag and got the 304 answer
        // (some of a solve's reads revalidate, some are plain reads).
        assert!(not_modified >= 1, "the revalidation answered 304");
}

/// The tamper law (registry.rs meta_json_cached): a signed row that stops
/// matching its signature hard-errors even though a cached row exists; a
/// source *outage* over the same cache serves the row. Never the reverse.
#[test]
fn http__tampered_signed_metadata_hard_errors_never_cache_fallback() {
    let key = key_from_seed([0x42; 32]);
    let f = signed_fixture("tamper", &key);
    seed_trust_store(&f.root, &key);

    let (_, stderr, ok) = in_proj(&f, &["lib", "install", "regtest@*", "--here"]);
    assert!(ok, "honest signed install failed: {stderr}");

    // Tamper: flip the thin checksum under the standing signature.
    let text = std::fs::read_to_string(f.reg.join("lib/regtest.json")).unwrap();
    let mut meta: serde_json::Value = serde_json::from_str(&text).unwrap();
    meta["versions"]["0.1"]["thin_checksum"] = serde_json::json!("sha256:deadbeef");
    stage_body(&f.routes, "/lib/regtest.json", serde_json::to_vec(&meta).unwrap());

    let p2 = f.new_project("proj2", "regtest = \"*\"");
    let (_, stderr, ok) = run_mcc_in(&p2, &f.root, &["lib", "install", "regtest@*", "--here"]);
    assert!(!ok, "tampered metadata refuses");
    assert!(
        stderr.contains("signature verification failed"),
        "names the tamper, not a generic fetch failure: {stderr}"
    );
    assert!(!p2.join("deps/regtest@0.1").exists(), "nothing was installed");

    // Outage: HTTP 500 is a source error — the parked cache row is the
    // offline truth and the install goes through.
    stage_status(&f.routes, "/lib/regtest.json", 500);
    let p3 = f.new_project("proj3", "regtest = \"*\"");
    let (_, stderr, ok) = run_mcc_in(&p3, &f.root, &["lib", "install", "regtest@*", "--here"]);
    assert!(ok, "outage falls back to the cached row: {stderr}");
    assert!(p3.join("deps/regtest@0.1").is_dir(), "cache-served install lands");
}

/// The ⑦b auxiliary channel over http: the store seeds key A; the table
/// carries B (signed by A → accepted and persisted), C (bad signature →
/// refused), D (signed by the unknown E → refused — no chain from an
/// anchor). Refusals fail the command. Afterwards B publishes metadata and
/// the remote search classifies it verified.
#[test]
fn http__trust_update_chains_from_a_trusted_key_and_refuses_bogus() {
    let f = fixture("trust");
    // The republish at the end writes 0.1 signed by B — versions are
    // immutable, so the hand-written unsigned row must not be in the way.
    std::fs::remove_file(f.reg.join("lib/regtest.json")).unwrap();
    let a = key_from_seed([0xA1; 32]);
    let b = key_from_seed([0xB2; 32]);
    let c = key_from_seed([0xC3; 32]);
    let d = key_from_seed([0xD4; 32]);
    let e = key_from_seed([0xE5; 32]);
    seed_trust_store(&f.root, &a);

    // C claims A signed it, but the signature is C's own — verification
    // against A's key fails.
    let mut c_row = serde_json::json!({
        "keyid": keyid_of(&c),
        "public": public_of(&c),
        "signer": keyid_of(&a),
    });
    c_row["sig"] = serde_json::json!(sign_trust_row(&c_row, &c));
    // D is signed by E, who chains from nothing.
    let d_row = trust_row(&d, &e);

    serve_trust_json(&f, &[trust_row(&b, &a), c_row, d_row]);

    let (_, stderr, ok) = in_proj(&f, &["lib", "trust", "update"]);
    assert!(!ok, "a refused row fails the command");
    assert!(stderr.contains("accepted"), "accepted rows are reported: {stderr}");
    assert!(stderr.contains("refused"), "refusals are named, never silent: {stderr}");

    // B persisted; C and D did not (A's seeding row is untouched).
    let store = std::fs::read_to_string(f.root.join("config/trust.toml")).unwrap();
    assert!(store.contains(&keyid_of(&b)), "the chained key landed: {store}");
    assert!(store.contains(&keyid_of(&a)), "the seeding key is untouched");
    assert_eq!(store.matches("[[keys]]").count(), 2, "exactly A and B are stored");
    assert!(!store.contains(&keyid_of(&c)) && !store.contains(&keyid_of(&d)));

    // B is now a trusted publisher: a republish signs the row with B and
    // the remote search shows the verified class. Publish takes the pack
    // directory (build_pack's `<base>/pack`, with pack.toml), not the
    // artifact scratch.
    let pack_dir = f.base.join("pack");
    std::fs::create_dir_all(f.root.join("config")).unwrap();
    let seed = f.base.join("publisher-b.ed25519");
    write_seed(&seed, &b);
    std::fs::write(
        f.root.join("config/mcc.yaml"),
        format!(
            "registry:\n  url: \"file://{}\"\n  publish:\n    key: \"{}\"\n    transport: \"none\"\n",
            f.reg.display(),
            seed.display()
        ),
    )
    .unwrap();
    let (_, stderr, ok) = run_mcc_in(
        &f.base,
        &f.root,
        &["lib", "publish", pack_dir.to_str().unwrap(), "--go"],
    );
    assert!(ok, "republish under B failed: {stderr}");

    std::fs::write(f.reg.join("search.json"), SEARCH_JSON).unwrap();
    let (_, stderr, ok) = in_proj(&f, &["lib", "search", "--remote", "regtest"]);
    assert!(ok, "remote search failed: {stderr}");
    assert!(stderr.contains("[verified]"), "B-signed metadata is verified: {stderr}");
}
