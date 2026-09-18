// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Projection-envelope versions (U85 · M3): the two revision tokens and the
//! drawing contract.
//!
//! `projection-schema-design.md` §1 puts two revision tokens on the envelope and
//! §1.1 defines the second one. The interesting claim is not that a hash exists
//! but that the two come from **one machine over nested material**:
//! `top_ver` is `world_ver`'s material minus the project's files outside the
//! top's dependency closure. Every assertion below is a consequence of that
//! sentence:
//!
//! 1. **They agree when the material agrees.** On the real `hbl` project the
//!    top `main` reaches every loaded project file, so the two tokens must be
//!    the same number under the two prefixes. A second hash machine, or a
//!    `top_ver` that quietly hashed something else, fails here.
//! 2. **A file the top does not reach moves one token and not the other.** This
//!    is the token's whole reason to exist — editing file A in a large project
//!    must not invalidate a client watching top B — and it is what makes
//!    `top_ver` a finer token rather than
//!    a second spelling of the root. Both directions are asserted, on a fixture
//!    where each side has ≥ 2 members: with one member per side, an
//!    implementation that ignored the closure entirely would still pass.
//! 3. **A name that does not name exactly one project module has no token.**
//!    `None`, never a guess — the same discipline the report-reference lookup
//!    follows. A token that answers "which top did you mean?" with a guess is
//!    worse than `-`, because it looks like an answer.
//!
//! The three drawing-contract strings are a different kind of value — declared,
//! not derived — and are asserted as such: non-null exactly on the view that
//! draws, and equal to the constants the layout / render / metrics modules
//! publish, so a bump in one of them cannot leave the envelope behind.
//!
//! ## Why the closure half runs in-process
//!
//! The CLI loads a world by walking `use` edges from the entry file, and the
//! entry file is where the top is resolved — so through the CLI the top's
//! closure *is* the loaded project file set, and the two tokens coincide by
//! construction (assertion 1 measures exactly that). A top whose closure is a
//! strict subset only exists once the workspace holds sources the entry does not
//! reach, which is the server/watch case the token is for. That world is built
//! in-process here, where the workspace can be loaded file by file.

use crate::common;

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

// ── Fixture plumbing ──

/// The real seven-layer project. Used for the CLI half, which needs a board
/// whose closure actually covers several files and whose build succeeds.
fn hbl_entry() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hbl/src/hbl.mc")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mcc-stage-topver-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// Run `mcc <args…> -F <entry>` and return `(stdout, stderr, ok)`.
///
/// `--local` on every call: `show stage` / `join` / `trace` are local-only
/// readouts, and with an `mcc start` service running a delegated invocation
/// prints nothing at all rather than an error (design §5.3 ① ⚠).
fn run(cwd: &Path, args: &[&str], entry: &Path) -> (String, String, bool) {
    let mut full: Vec<&str> = vec!["--local"];
    full.extend_from_slice(args);
    full.push("-F");
    full.push(entry.to_str().expect("fixture path"));
    let out = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .current_dir(cwd)
        .args(&full)
        .output()
        .unwrap_or_else(|e| panic!("run mcc {args:?}: {e}"));
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

fn stage_of(stdout: &str) -> Value {
    let envelope: Value =
        serde_json::from_str(stdout).unwrap_or_else(|e| panic!("invalid JSON: {e}\n{stdout}"));
    envelope["result"]["stage"].clone()
}

/// The `#` header line of a text face — the first line, which every face prints
/// ([`mcc::stages::StageView::header_line`]).
fn header_of(text: &str) -> String {
    text.lines()
        .find(|l| l.starts_with('#'))
        .unwrap_or_else(|| panic!("no header line in:\n{text}"))
        .to_string()
}

/// The 16 hex digits of a token, asserting the shape while taking them apart.
///
/// Shape is not cosmetic here: the token's value is compared across projection
/// views and across builds, so a `-` or a truncated hash has to fail loudly
/// rather than compare unequal for an uninteresting reason.
fn hex_of(token: &str, prefix: char) -> String {
    let mut chars = token.chars();
    assert_eq!(
        chars.next(),
        Some(prefix),
        "{token:?} must carry the {prefix:?} prefix of its kind"
    );
    assert_eq!(
        chars.next(),
        Some('_'),
        "{token:?} must separate prefix from digest"
    );
    let digits: String = chars.collect();
    assert_eq!(digits.len(), 16, "{token:?} must be 16 hex digits");
    assert!(
        digits.chars().all(|c| c.is_ascii_hexdigit()),
        "{token:?} must be hex"
    );
    digits
}

/// The two tokens of one JSON projection, shape-checked.
fn tokens(stage: &Value) -> (String, String) {
    let w = stage["world_ver"]
        .as_str()
        .unwrap_or_else(|| panic!("world_ver is not a string: {stage}"));
    let t = stage["top_ver"]
        .as_str()
        .unwrap_or_else(|| panic!("top_ver is not a string: {stage}"));
    (hex_of(w, 'w'), hex_of(t, 't'))
}

// ── 1 · One machine, two materials ──

/// If a top reaches every loaded project file, its material is the whole world
/// and the two tokens must be the same digest under two prefixes.
///
/// This is the assertion a second hash machine cannot survive: it is not a shape
/// check or a range check, it pins the *value* of one token to the value of the
/// other through an independent path.
#[test]
fn the_two_tokens_agree_when_the_top_reaches_the_whole_project() {
    let cwd = scratch("agree");
    let entry = hbl_entry();
    let (stdout, stderr, ok) = run(&cwd, &["show", "stage", "p2", "-f", "json"], &entry);
    assert!(ok, "`show stage p2` failed: {stderr}");
    let (world, top) = tokens(&stage_of(&stdout));
    assert_eq!(
        world, top,
        "`main` is the entry's own module and reaches every loaded project file, \
         so its closure material is the root material — one machine, one digest"
    );
}

// ── 2 · Both tokens on every face, JSON and text ──

/// Every face of the readout family carries both tokens, and the text face
/// prints the same values the JSON face carries.
///
/// Two faces over one structure is the view family's rule; a token that only
/// existed in the JSON would be a second spelling of the envelope, and a
/// comparison tool reading the text face would silently be comparing nothing.
#[test]
fn every_face_publishes_both_tokens_and_prints_what_it_publishes() {
    let cwd = scratch("faces");
    let entry = hbl_entry();
    let key = format!("{}:19", entry.display());

    let segs = ["p1", "p2", "vec", "viz"];
    let mut faces: Vec<(String, Vec<&str>)> = segs
        .iter()
        .map(|s| (format!("stage.{s}"), vec!["show", "stage", s]))
        .collect();
    faces.push(("join.src->p2".to_string(), vec!["join", "src", "p2"]));
    faces.push(("trace".to_string(), vec!["trace", &key]));

    for (name, args) in faces {
        let mut json_args = args.clone();
        json_args.extend_from_slice(&["-f", "json"]);
        let (stdout, stderr, ok) = run(&cwd, &json_args, &entry);
        assert!(ok, "`{args:?}` failed: {stderr}");
        let stage = stage_of(&stdout);
        assert_eq!(stage["view"], name, "the view names itself");
        let (world, top) = tokens(&stage);

        let (text, _, ok) = run(&cwd, &args, &entry);
        assert!(ok, "`{args:?}` (text) failed");
        let header = header_of(&text);
        assert!(
            header.contains(&format!("world_ver=w_{world}")),
            "text face must print the world token it carries:\n{header}"
        );
        assert!(
            header.contains(&format!("top_ver=t_{top}")),
            "text face must print the top token it carries:\n{header}"
        );
    }
}

// ── 3 · The drawing contract, declared where it is honoured ──

/// The three contract strings are non-null exactly on `stage.viz`, equal the
/// constants the owning modules declare, and absent everywhere else.
///
/// Both branches are filled on purpose: a test that only checked `stage.viz`
/// would pass if every view carried the strings, which would say a `stage.p2`
/// readout came from a layout engine it never ran.
#[test]
fn the_drawing_contract_is_carried_by_the_view_that_draws() {
    let cwd = scratch("contract");
    let entry = hbl_entry();

    let (stdout, stderr, ok) = run(&cwd, &["show", "stage", "viz", "-f", "json"], &entry);
    assert!(ok, "`show stage viz` failed: {stderr}");
    let viz = stage_of(&stdout);
    assert_eq!(
        viz["layout_version"].as_str(),
        Some(mcc::viz::layout::LAYOUT_VERSION),
        "the envelope must carry the layout module's own constant"
    );
    assert_eq!(
        viz["render_version"].as_str(),
        Some(mcc::viz::render::RENDER_VERSION),
        "the envelope must carry the render module's own constant"
    );
    assert_eq!(
        viz["metric_schema_version"].as_str(),
        Some(
            mcc::viz::metrics::METRIC_SCHEMA_VERSION
                .to_string()
                .as_str()
        ),
        "the envelope must carry the metrics module's own constant — the same \
         number the metrics snapshot stamps, not a second one"
    );

    // The text face carries them too, and only there.
    let (text, _, ok) = run(&cwd, &["show", "stage", "viz"], &entry);
    assert!(ok, "`show stage viz` (text) failed");
    assert!(
        header_of(&text).contains("layout="),
        "the drawing face prints its contract:\n{}",
        header_of(&text)
    );

    for seg in ["p1", "p2", "vec"] {
        let (stdout, _, ok) = run(&cwd, &["show", "stage", seg, "-f", "json"], &entry);
        assert!(ok, "`show stage {seg}` failed");
        let stage = stage_of(&stdout);
        for field in ["layout_version", "render_version", "metric_schema_version"] {
            assert!(
                stage[field].is_null(),
                "stage.{seg} does not draw, so it must not claim a drawing \
                 contract ({field} = {})",
                stage[field]
            );
        }
        let (text, _, _) = run(&cwd, &["show", "stage", seg], &entry);
        assert!(
            !header_of(&text).contains("layout="),
            "stage.{seg} must not print a drawing contract it does not honour"
        );
    }
}

// ── 4 · The closure, driven in-process ──
//
// The world here holds five project files. `a.mc` uses `b.mc` and `c.mc`; `z1`
// and `z2` are loaded without being reachable from `A`. So the closure has three
// members and its complement two — enough on each side that an implementation
// ignoring the closure cannot pass by accident.

/// `A` reaches `B` and `C`; nothing reaches `Z1` / `Z2`.
const SRC_A: &str = "use ./b.mc\nuse ./c.mc\n\nmodule A\n{\n    io P\n}\n";
const SRC_B: &str = "module B\n{\n    io P\n}\n";
const SRC_C: &str = "module C\n{\n    io P\n}\n";
const SRC_Z1: &str = "module Z1\n{\n    io P\n}\n";
const SRC_Z2: &str = "module Z2\n{\n    io P\n}\n";

/// A source file whose *text* differs but whose shape does not, so a changed
/// token can only mean the content was hashed.
fn variant(marker: &str) -> String {
    format!("# {marker}\nmodule {}\n{{\n    io P\n}}\n", marker.trim())
}

struct Board {
    dir: PathBuf,
}

impl Board {
    /// Write the five sources and load them: `a.mc` recursively (which brings in
    /// `b.mc` and `c.mc` through its `use` lines), then the two the entry does
    /// not reach.
    fn new(name: &str) -> Self {
        let dir = scratch(name);
        let board = Board { dir };
        board.write("a.mc", SRC_A);
        board.write("b.mc", SRC_B);
        board.write("c.mc", SRC_C);
        board.write("z1.mc", SRC_Z1);
        board.write("z2.mc", SRC_Z2);
        mcc::mcc_set_project_root(&board.dir);
        mcc::mcc_load_project(&board.uri("a.mc"));
        mcc::mcc_add(&board.uri("z1.mc"));
        mcc::mcc_add(&board.uri("z2.mc"));
        board
    }

    fn path(&self, file: &str) -> PathBuf {
        self.dir.join(file)
    }

    fn uri(&self, file: &str) -> String {
        self.path(file).to_string_lossy().to_string()
    }

    fn write(&self, file: &str, src: &str) {
        std::fs::write(self.path(file), src).expect("write fixture source");
    }

    /// Rewrite one file and re-load it — an edit round, the way the server does
    /// it. `mcc_add` replaces the file's workspace entry in place.
    fn edit(&self, file: &str, src: &str) {
        self.write(file, src);
        mcc::mcc_add(&self.uri(file));
    }

    fn world(&self) -> String {
        mcc::stages::world_ver::world_ver().expect("the board's world fingerprints")
    }

    fn top(&self, name: &str) -> Option<String> {
        mcc::stages::top_ver::top_ver(name)
    }
}

/// Load `A`'s world, with `common::reset` clearing any state a previous test in
/// this file left behind. `init_no_lib` on purpose: the system library is not
/// loaded, so the world is exactly the five project files and the closure's
/// complement is exactly `{z1, z2}`.
fn board(name: &str) -> (std::sync::MutexGuard<'static, ()>, Board) {
    let guard = common::lock();
    common::reset();
    let b = Board::new(name);
    (guard, b)
}

/// The token's reason to exist: an edit outside the top's closure moves the
/// world token and leaves the top token alone.
#[test]
fn a_file_the_top_does_not_reach_moves_only_the_world_token() {
    let (_lock, b) = board("outsider");

    let world0 = b.world();
    let top0 = b.top("A").expect("A is a project module");
    assert_ne!(
        hex_of(&world0, 'w'),
        hex_of(&top0, 't'),
        "the closure is a strict subset of the world here, so the two tokens \
         must differ — otherwise this test could not tell the two apart"
    );

    // Two files outside the closure, one edit each: two independent chances for
    // a material that ignored the closure to be caught.
    b.edit("z1.mc", &variant("z1 edited"));
    let world1 = b.world();
    assert_ne!(
        world1, world0,
        "editing a loaded source moves the world token"
    );
    assert_eq!(
        b.top("A"),
        Some(top0.clone()),
        "z1 is not reachable from A's use graph, so A did not change"
    );

    b.edit("z2.mc", &variant("z2 edited"));
    let world2 = b.world();
    assert_ne!(
        world2, world1,
        "the second outsider moves the world token too"
    );
    assert_eq!(
        b.top("A"),
        Some(top0),
        "neither outsider may move A's token, before or after the other changed"
    );
}

/// The other direction — and the reason the first test is not vacuous: an edit
/// *inside* the closure must move the top token.
///
/// Without this, an implementation that returned the empty digest for every top
/// would satisfy "unrelated files don't move it" perfectly.
#[test]
fn a_file_the_top_reaches_moves_its_token() {
    let (_lock, b) = board("insider");

    let world0 = b.world();
    let top0 = b.top("A").expect("A is a project module");

    // `b.mc` is reached directly; `c.mc` is reached the same way. Two members of
    // the closure, so "the closure is honoured" is asserted more than once.
    b.edit("b.mc", &variant("b edited"));
    let top1 = b.top("A").expect("A still resolves after the edit");
    assert_ne!(top1, top0, "B is in A's closure, so A's token must move");
    assert_ne!(b.world(), world0, "and so must the world token");

    b.edit("c.mc", &variant("c edited"));
    let top2 = b.top("A").expect("A still resolves");
    assert_ne!(top2, top1, "C is in A's closure too");
}

/// A name that is not a project module, or is more than one, has no token.
///
/// `None` rather than a fallback: the token is compared against a cached value
/// to decide whether to re-read, and a guessed token would answer "nothing here
/// changed" about a top the caller never named.
#[test]
fn a_name_that_is_not_exactly_one_project_module_has_no_token() {
    let (_lock, b) = board("ambiguity");

    assert_eq!(
        b.top("NoSuchModule"),
        None,
        "an unknown name must not fall back to some other module's token"
    );
    assert!(
        b.top("Z1").is_some(),
        "a module loaded without being used is still a project module — the \
         test below is only meaningful if the workspace resolves names at all"
    );

    // Two project files defining the same name: the lookup must refuse, not
    // pick one. The duplicate is a diagnostic elsewhere; here it is the point.
    b.edit("z1.mc", "module DUP\n{\n    io P\n}\n");
    b.edit("z2.mc", "module DUP\n{\n    io P\n}\n");
    assert_eq!(
        b.top("DUP"),
        None,
        "two definitions of DUP leave no single file to hash, and a token for \
         one of them would silently describe a top the caller did not name"
    );
}
