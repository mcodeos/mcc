// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The MCP mirror of the three chain verbs (design §5.3, U84's last cell).
//!
//! `mcc show stage` / `mcc join` / `mcc trace` read the compile chain; the MCP
//! server publishes the same three readings to an agent over its own transport.
//! A mirror is worth having only if it is the **same reading**: a second
//! construction would not fail, it would answer the same question with a
//! different payload, and nothing would say so.
//!
//! So every case here drives both faces over one source set and requires the
//! tool's payload to equal the CLI's `result`, field for field. The one field
//! excluded is `summary.elapsed_ms`, which is a wall clock: it is measured
//! around a different amount of work on each side and is unstable even between
//! two runs of one command (§5.3 O15). Masking it is the project's standing
//! procedure, not a weakened assertion — everything else, `world_ver` included,
//! is compared byte for byte.
//!
//! Why the CLI side runs `--local`: a stage view is read off the **caller's**
//! loaded source set, so a delegated invocation would answer from the daemon's
//! world instead. The MCP server always reads in its own process (that is the
//! whole reason `stages::read` is a library function rather than an RPC
//! method), so `--local` is what makes the two faces comparable at all.
//!
//! ⚠ **The reset is asserted, not assumed.** A server answers many requests in
//! one process and the workspace tables are append-only, so a weak reset would
//! let request N's source set leak into request N+1's `world_ver`. The last test
//! reads two different projects through one server process and requires the
//! second answer to be the second project's, not a union.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};

// ── Fixtures ──

/// The entry of the real seven-layer project. Real and not synthetic because
/// the interesting cases (all four segments non-empty, three distinct hops,
/// traceable keys) need a board the compiler actually has something to say
/// about.
fn hbl_entry() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hbl/src/hbl.mc")
}

/// A fresh, **empty** directory to run a CLI invocation in: the readout must
/// not depend on where it is run.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mcc-stage-mcp-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// A two-pin part on two nets: small enough to read, complete enough to build.
const SRC_TWO: &str = r#"component CAP(cap::INT) {
    pins = [
        1 = 1
        2 = 2
    ]
    func Cap([n1, n2]) {
        n1 - this - n2
    }
}
module main {
    io VDD
    io GND
    CAP c1(1)
    CAP c2(1)
    c1.Cap([VDD, GND])
    c2.Cap([VDD, c1.2])
}
"#;

/// The same circuit with one more part. The two differ in more than one object,
/// so a payload built from a union of the two loads cannot be mistaken for
/// either.
const SRC_THREE: &str = r#"component CAP(cap::INT) {
    pins = [
        1 = 1
        2 = 2
    ]
    func Cap([n1, n2]) {
        n1 - this - n2
    }
}
module main {
    io VDD
    io GND
    CAP c1(1)
    CAP c2(1)
    CAP c3(1)
    c1.Cap([VDD, GND])
    c2.Cap([VDD, c1.2])
    c3.Cap([c2.2, GND])
}
"#;

/// Write `src` into a fresh scratch directory and return the file's path.
fn write_fixture(name: &str, src: &str) -> PathBuf {
    let dir = scratch(name);
    let path = dir.join("main.mc");
    std::fs::write(&path, src).expect("write the fixture source");
    path
}

// ── The CLI face ──

/// Run `mcc --local <args> -F <entry> -f json` from a scratch directory and
/// return its `result` object, or the error message the envelope carries.
///
/// The error branch returns the CLI's own message rather than a bool, so an
/// error case can require the two faces to explain themselves **in the same
/// words** — a mirror that fails differently is a mirror of a different thing.
fn cli_result(entry: &Path, args: &[&str]) -> Result<Value, String> {
    let out = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .current_dir(scratch("cli"))
        .arg("--local")
        .args(args)
        .arg("-F")
        .arg(entry)
        .arg("-f")
        .arg("json")
        .output()
        .expect("run mcc");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let envelope: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("invalid JSON from `mcc {args:?}`: {e}\n{stdout}"));
    match envelope.get("error") {
        Some(e) => Err(e["message"].as_str().unwrap_or_default().to_string()),
        None => Ok(envelope["result"].clone()),
    }
}

// ── The MCP face ──

/// One live `mcc-mcp` process, driven over line-delimited JSON-RPC.
struct Mcp {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    next_id: u64,
}

impl Mcp {
    /// Start a server and complete the handshake.
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_mcc-mcp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn mcc-mcp");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = BufReader::new(child.stdout.take().expect("stdout"));
        let mut mcp = Self {
            child,
            stdin,
            stdout,
            next_id: 0,
        };
        mcp.request(
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "stage-mcp-mirror", "version": "0" },
            }),
        );
        mcp.notify("notifications/initialized", json!({}));
        mcp
    }

    fn send(&mut self, value: &Value) {
        let mut line = serde_json::to_string(value).expect("serialize request");
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .expect("write request");
        self.stdin.flush().expect("flush request");
    }

    fn notify(&mut self, method: &str, params: Value) {
        let value = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        self.send(&value);
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let value = json!({
            "jsonrpc": "2.0",
            "id": self.next_id,
            "method": method,
            "params": params,
        });
        self.send(&value);
        let mut line = String::new();
        let n = self.stdout.read_line(&mut line).expect("read response");
        assert!(n > 0, "mcc-mcp closed the stream during `{method}`");
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("invalid JSON-RPC: {e}\n{line}"))
    }

    /// One `tools/call`. `Err` is the tool's own message.
    fn call(&mut self, tool: &str, arguments: Value) -> Result<Value, String> {
        let response = self.request(
            "tools/call",
            json!({ "name": tool, "arguments": arguments }),
        );
        if let Some(e) = response.get("error") {
            return Err(e["message"].as_str().unwrap_or_default().to_string());
        }
        let text = response["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_default();
        serde_json::from_str(text).map_err(|e| format!("tool payload is not JSON: {e}\n{text}"))
    }
}

impl Drop for Mcp {
    fn drop(&mut self) {
        // The server exits when its stdin closes; `kill` is the backstop for a
        // test that failed between the two.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// One tool call in a server of its own, which is what every case below wants:
/// the reading must not depend on what the process read before it (that
/// independence is asserted separately, in the reset test).
fn call_once(tool: &str, arguments: Value) -> Result<Value, String> {
    let mut mcp = Mcp::start();
    mcp.call(tool, arguments)
}

/// The MCP arguments for one entry, with the library list the CLI resolves on
/// its own side through the project manifest and the config default.
fn args_for(entry: &Path, extra: Value) -> Value {
    let mut obj = json!({
        "entry": entry.to_str().expect("fixture path"),
        "libs": ["mcode"],
    });
    let map = obj.as_object_mut().expect("object");
    for (k, v) in extra.as_object().expect("object") {
        map.insert(k.clone(), v.clone());
    }
    obj
}

// ── The comparison ──

/// Drop the wall-clock field. Everything else is compared exactly.
fn mask(mut value: Value) -> Value {
    if let Some(summary) = value.get_mut("summary").and_then(|s| s.as_object_mut()) {
        summary.remove("elapsed_ms");
    }
    value
}

/// Require both faces to agree on one reading, and say so out loud when they
/// do not.
fn assert_mirrors(what: &str, entry: &Path, cli_args: &[&str], tool: &str, extra: Value) {
    let want =
        cli_result(entry, cli_args).unwrap_or_else(|e| panic!("the CLI refused `{what}`: {e}"));
    let got = call_once(tool, args_for(entry, extra))
        .unwrap_or_else(|e| panic!("the MCP tool refused `{what}`: {e}"));
    let (want, got) = (mask(want), mask(got));
    assert_eq!(
        want, got,
        "`{what}`: the MCP tool and the CLI published different payloads"
    );
}

// ── Acceptance ──

/// All four segments, including the reserved `p1` slot — a segment whose item
/// list is empty is exactly the case where a mirror could quietly publish a
/// different key set and still look right.
#[test]
fn every_segment_mirrors_the_cli() {
    let entry = hbl_entry();
    for seg in ["p1", "p2", "vec", "viz"] {
        assert_mirrors(
            &format!("show stage {seg}"),
            &entry,
            &["show", "stage", seg],
            "mcc_stage_view",
            json!({ "segment": seg }),
        );
    }
}

/// The segment default: `show stage` with no positional reads `p2`, and a tool
/// call with no `segment` must read the same one.
#[test]
fn a_missing_segment_defaults_the_same_way() {
    assert_mirrors(
        "show stage (default)",
        &hbl_entry(),
        &["show", "stage"],
        "mcc_stage_view",
        json!({}),
    );
}

/// All three hops. The two circuit hops are the ones that need the vector
/// graph, so they are the two where a second construction would most easily
/// diverge.
#[test]
fn every_hop_mirrors_the_cli() {
    let entry = hbl_entry();
    for (a, b) in [("src", "p2"), ("p2", "vec"), ("vec", "viz")] {
        assert_mirrors(
            &format!("join {a} {b}"),
            &entry,
            &["join", a, b],
            "mcc_stage_join",
            json!({ "from": a, "to": b }),
        );
    }
}

/// `--only` filters rows and leaves the counts alone. Both halves are the
/// point: a filtered readout that rewrote its counts would be a different
/// reading, not a narrower one.
///
/// `drop` is taken from the hop's own counts rather than written down: the
/// class has members on this board, and the assertion that follows would be
/// vacuous on a class that has none.
#[test]
fn the_class_filter_mirrors_the_cli() {
    let entry = hbl_entry();
    let want = cli_result(&entry, &["join", "p2", "vec"]).expect("join p2 vec");
    let rows = want["stage"]["counts"]["drop"].as_u64().unwrap_or(0);
    assert!(
        rows >= 2,
        "the fixture has too few `drop` rows to test with"
    );

    assert_mirrors(
        "join p2 vec --only drop",
        &entry,
        &["join", "p2", "vec", "--only", "drop"],
        "mcc_stage_join",
        json!({ "from": "p2", "to": "vec", "only": "drop" }),
    );
}

/// A trace reads the whole chain at once, so it is the one tool whose payload
/// is built from all three hops.
///
/// Two keys of different forms are used, because the form is read off the key
/// itself and the two forms reach the object by different routes.
#[test]
fn trace_mirrors_the_cli() {
    let entry = hbl_entry();
    for key in ["main.DCDC", "main.LDO.VOUT"] {
        assert_mirrors(
            &format!("trace {key}"),
            &entry,
            &["trace", key],
            "mcc_stage_trace",
            json!({ "key": key }),
        );
    }
}

/// An argument that cannot be honoured is refused, and refused in the same
/// words: the CLI puts the message in its error envelope and the tool puts it
/// in the MCP error, and a reader comparing the two should see one sentence.
#[test]
fn an_unhonourable_argument_is_refused_in_the_same_words() {
    let entry = hbl_entry();
    let cases: &[(&str, &[&str], &str, Value)] = &[
        (
            "an unknown segment",
            &["show", "stage", "nope"],
            "mcc_stage_view",
            json!({ "segment": "nope" }),
        ),
        (
            "a pair that is not adjacent",
            &["join", "src", "vec"],
            "mcc_stage_join",
            json!({ "from": "src", "to": "vec" }),
        ),
        (
            "a pair in the wrong order",
            &["join", "p2", "src"],
            "mcc_stage_join",
            json!({ "from": "p2", "to": "src" }),
        ),
        (
            "an unknown class word",
            &["join", "p2", "vec", "--only", "nope"],
            "mcc_stage_join",
            json!({ "from": "p2", "to": "vec", "only": "nope" }),
        ),
        (
            "a key that names nothing in this build",
            &["trace", "main.DCDC.VCC"],
            "mcc_stage_trace",
            json!({ "key": "main.DCDC.VCC" }),
        ),
    ];

    for (what, cli_args, tool, extra) in cases {
        let want = cli_result(&entry, cli_args).expect_err(&format!("the CLI accepted `{what}`"));
        let got = call_once(tool, args_for(&entry, extra.clone()))
            .expect_err(&format!("the MCP tool accepted `{what}`"));
        assert_eq!(
            want, got,
            "`{what}`: the two faces explained it differently"
        );
    }
}

/// The key set is the envelope's, not one command's: whatever a caller reads
/// out of the CLI's payload is there in the tool's too, `null` included. This
/// is the readability of the single-sourced field list, asserted rather than
/// assumed.
#[test]
fn the_payload_carries_the_envelope_key_set() {
    let entry = hbl_entry();
    let want = cli_result(&entry, &["show", "stage", "p2"]).expect("show stage p2");
    let got = call_once(
        "mcc_stage_view",
        args_for(&entry, json!({ "segment": "p2" })),
    )
    .expect("mcc_stage_view");

    let keys = |v: &Value| {
        v.as_object()
            .expect("object")
            .keys()
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(keys(&want), keys(&got), "the result key sets differ");
    assert_eq!(
        keys(&want["stage"]),
        keys(&got["stage"]),
        "the stage key sets differ"
    );
    assert_eq!(
        keys(&want["summary"]),
        keys(&got["summary"]),
        "the summary key sets differ"
    );
}

/// Two projects through one server process.
///
/// The second answer must be the second project's own reading, not one that
/// still carries the first. A server whose reset only dropped the active
/// project would fail here, and nothing about either answer alone would show
/// it.
#[test]
fn one_server_process_reads_two_projects_independently() {
    let first = write_fixture("reset-a", SRC_TWO);
    let second = write_fixture("reset-b", SRC_THREE);

    let want_first = cli_result(&first, &["show", "stage", "p2"]).expect("CLI on the first");
    let want_second = cli_result(&second, &["show", "stage", "p2"]).expect("CLI on the second");
    assert_ne!(
        want_first["stage"]["world_ver"], want_second["stage"]["world_ver"],
        "the two fixtures are not two sources, so this test proves nothing"
    );

    let mut mcp = Mcp::start();
    let got_first = mcp
        .call(
            "mcc_stage_view",
            args_for(&first, json!({ "segment": "p2" })),
        )
        .expect("the first read");
    let got_second = mcp
        .call(
            "mcc_stage_view",
            args_for(&second, json!({ "segment": "p2" })),
        )
        .expect("the second read");

    assert_eq!(
        mask(want_first),
        mask(got_first),
        "the first read does not mirror the CLI"
    );
    assert_eq!(
        mask(want_second),
        mask(got_second),
        "the second read in the same process does not mirror the CLI — \
         the first read's source set is still in the world"
    );
}
