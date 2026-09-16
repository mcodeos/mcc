# MCode Compiler (mcc) — Authoring Rules

This repository is the MCode compiler. All content authored here — by humans
or by AI agents — must be written in English.

## Hard rule: English only

All content must be written in English using ASCII characters only; non-ASCII
characters such as CJK are not permitted in tracked files, commit messages, or
PR descriptions. This is enforced mechanically:

- Pre-commit hook (`.githooks/pre-commit`, installed via
  `git config core.hooksPath .githooks`) rejects staged content containing
  non-ASCII characters; the `commit-msg` hook rejects non-ASCII commit messages.
- CI workflow (`.github/workflows/check-cjk.yml`) scans every git-tracked
  file on every push / pull request and fails the build on non-ASCII characters.
- Local scanner: `python3 scripts/check-cjk.py` — exit 0 clean, 1 otherwise.

Applies to:

- Code comments (`//`, `///`, `//!`), doc comments, and identifiers.
- String literals: log / diagnostic / panic messages and CLI output.
- Test names, test comments, and test data (including `tests/golden/hbl/*.golden.toml`).
- Commit messages and pull request descriptions.

If a non-English term must appear in a diagnostic for users, provide it in a
translation layer or docs — never in source strings.

## Rule: no hardcoded user-specific absolute paths

Never hardcode absolute paths that embed a developer's username (for example
`/Users/<user>/work/mo/mcc`) in source code, tests, golden data, docs, or skill
files. Use portable forms instead: `~` in docs and shell examples, `$HOME`-
derived paths (`PathBuf::from(home)` in Rust tests), or paths relative to the
project root (`env!("CARGO_MANIFEST_DIR")`). This applies to the whole project
including test code and test data.

This is enforced mechanically:

- Pre-commit hook (`.githooks/pre-commit`) rejects staged files that embed a
  home-directory path; the `commit-msg` hook rejects commit messages that do.
- CI workflow (`.github/workflows/check-paths.yml`) scans every git-tracked
  file on every push / pull request.
- Local scanner: `python3 scripts/check-paths.py` — exit 0 clean, 1 otherwise.
- Run `scripts/check.sh` for the full local gate (step 10).

The detector is structural — it matches any `/Users/<name>/`, `/home/<name>/`
or `C:\Users\<name>\` prefix, so it covers every account name without carrying
a list of names. Placeholder forms (`/Users/<name>/repo`, `$HOME/...`, `~/...`,
`./src/lib.rs`) pass untouched. A doc that must display a bad example can mark
the line with `check-paths:allow`.

## Rule: diagnostics must carry a real source position

Every diagnostic mcc emits — `--dlog` one-line output, `check`/`parse` reports,
and the LSP layer — must be anchored at the source position where the problem
actually lives, never fabricated and never `file:1:1` unless no position exists
at all.

- Connection-level diagnostics anchor at the wiring statement
  (`ConnectionInst::source_span` → `NetPoint::src_pos`).
- Unconnected pins/ports have no wiring site; anchor at their declaration via
  `InstEntry::fallback_pos` (component pin-id span in the component body, or
  the module span for ports).
- `pos == 0` is only acceptable when the entity truly has no source position
  (e.g. synthetic anonymous net points). If you are about to emit a diagnostic
  with a zero offset, thread a real span through instead of falling back
  silently.

## Rule: keep code comments lean

Comments are the most expensive text in this repository: every future edit pays
for them twice (change the code, change the prose), and nothing verifies them,
so they rot into assertions that are confidently wrong. Budget them. A comment
earns its place only by carrying something the code cannot — a non-obvious
*why*, a constraint imposed from outside the file, a trap that costs the next
reader an afternoon.

- **Explain why, not what.** `// increment i` is noise; `i += 1` already says
  it. The test is deletion: if removing the comment loses no reason, no
  constraint, and no warning, remove it.
- **No change diary.** Not `// previously used X`, `// now also handles Y`,
  `// renamed from Z`, `// an early version used to …`. Git holds the history;
  source states only what is true now. (This is a standing temptation in a
  codebase with a long refactor history.)
- **No commented-out code.** Delete it — git remembers, and a reader cannot
  tell a disabled experiment from a forgotten one.
- **No decorative structure.** Do not fence sections with `// ───── Parse ─────`
  rules or box-drawing banners. They are punctuation, not information.
- **Keep doc comments to the contract.** A `///` states what a caller must
  know: arguments, invariants, panics, units. Not the implementation
  walkthrough, not the design rationale. Multi-paragraph explanation belongs in
  the design docs, with at most a one-line pointer here — and since those docs
  live outside this repo, that pointer is a bare document name
  (`island-attribution-design.md` §2), never a path. The same applies to
  module-level `//!`.
- **Match the neighbours.** Density is local and load-bearing: a file written
  tightly stays tight. Appending a verbose block to a terse file is a
  regression even when every sentence in it is true.

One or two lines is the target. If the point does not fit in two lines, it is
documentation, not a comment.

This is enforced mechanically, on git-tracked `*.rs` only:

- Pre-commit hook (`.githooks/pre-commit`) rejects staged `.rs` files whose
  comments violate the rule.
- CI workflow (`.github/workflows/check-comments.yml`) scans every git-tracked
  `*.rs` file on every push / pull request.
- Local scanner: `python3 scripts/check-comments.py` — exit 0 clean, 1 otherwise.
- Run `scripts/check.sh` for the full local gate (step 11).

The scanner checks three mechanical signals: a decorative rule line, the
change-diary wordings `previously` / `formerly` / `renamed from` /
`an early version` / `used to be`, and a comment line over 100 columns
(rustfmt's default `max_width` — `wrap_comments` is off, so rustfmt never
reflows a comment). It is a net, not a proof: prose that violates the rule
without matching one of the three passes the gate. The net is deliberately
narrow: it never counts the *length of a comment block*, because no threshold
separates multi-paragraph design rationale (which belongs in the design docs,
with at most a one-line pointer here) from a legitimately long contract, module
header or table. The shape of the prose is left to review. Content inside a fenced
block, and a table row (content starting with `|`), are exempt. To keep a line
that is legitimately long — a diagram, a URL, a grammar production — mark it
with `check-comments:allow`. Do not put that marker on a `//!` line: it renders
into the published rustdoc.

## Rule: name comparisons are exact

Every name in mcc — class, instance, pin, net, function, constructor, enum and
enum value — compares by exact string equality, with no case normalization.
`eq_ignore_ascii_case` is therefore a normalizing shortcut, and one is legal
only where it is registered. A registered face is never a name: it is a
user-facing input word table (query-DSL keywords, `--type` kind words, the
`...GATE=0|false` environment variables), which users spell in any case they
like.

This is enforced mechanically, on git-tracked `src/**/*.rs` only:

- Pre-commit hook (`.githooks/pre-commit`) rejects staged `src/` Rust that adds
  an unregistered call.
- CI workflow (`.github/workflows/check-name-case.yml`) scans every git-tracked
  `src/**/*.rs` file on every push / pull request.
- Local scanner: `python3 scripts/check-name-case.py` — exit 0 clean, 1 otherwise.
- Run `scripts/check.sh` for the full local gate (step 12).

The whitelist is the `REGISTERED` table in `scripts/check-name-case.py`, counted
per file. Adding or removing a registered call means updating both that table and
the registry in `01-lexical.md` §2.2, so the two cannot drift. The gate is a net,
not a proof: a comparison written by hand
(`a.to_lowercase() == b.to_lowercase()`) passes it.

## Rule: no guessing from names

No logic or policy may be built on a guess read off a name. The rule above
governs how a comparison is written; this one governs whether a name is
evidence at all — it is not. A name is a label the author chose, and its
spelling carries no promise about what the thing is.

**Forbidden shapes** — a decision resting on any of these is a defect:

- a prefix / suffix / substring test on a name (`starts_with("V")`,
  `contains("power")`), in any case;
- a shape test on the spelling — digit-letter patterns, `contains('V')` plus a
  digit count, length thresholds;
- gating semantics on a fixed list of names (`"Cap"`, `"Pullup"`, `"GND"`,
  `POWER_PIN_NAMES`, …), in whole or in part;
- a name-based fallback that the code reaches only when the real evidence is
  missing, and so silently decides in the gap.

**What is evidence instead:**

- a declared or registered name used AS identity, compared by exact string
  equality (see the rule above): a pin the author named, an interface member, a
  class or function or attribute key. Declaring a name is what makes it
  identity; reading one is not a guess.
- the AST and the syntax: whether a placeholder was written, whether a Set or a
  list was used, where a token sits;
- a decoded VALUE — a voltage, a length, a count read through the value engine,
  never a fragment of the text that wrote it;
- a type, a role, a contract, a structural shape or a position.

**Where this bites.** A value is not a name: `V3V3` and `VDD_3V3` are two
different labels for one 3.3 V rail, and pairing them by the "3V3" both spell is
exactly the kind of inference this rule forbids — pair by the declared voltage.
The binders in `src/instant/mc_mod/phases.rs`, and the ground / supply / rail
decision points enumerated in `CIMP.md` §1 (mcd), are the known instances; read
them as worked examples of the shapes above, not as a closed list.

## Rule: no auto-commit; manual testing and manual commit

Never commit (nor stage, amend, push, or open a PR) automatically after
finishing changes. Leave the working tree for the user to test manually; the
user commits manually. You may propose a ready-to-use commit message and
stage files only when the user explicitly asks, but still wait for the user's
manual confirmation before committing.

## Rule: targeted tests only; full regression is manual

Do not run the full regression suite (`cargo test` without a filter) after
every change. After each change run only the targeted tests that cover the
modified area (for example `cargo test --lib`, a specific `--test` target, or
a single named test), and verify the build compiles. The full regression suite
is launched manually by the user (`cargo test` / `cargo test --no-fail-fast`).

## Rule: build in a private slot, not the shared target dir

Several sessions work in this checkout at once (agent sessions, and
rust-analyzer's `cargo check`). Cargo holds `<target-dir>/<profile>/.cargo-lock`
for the whole of a build, so sessions sharing one target dir queue behind each
other — one editor check can block a build for minutes.

Start a session with:

    eval "$(scripts/mcc-slot.sh)"

That pins the session to an idle slot and exports `CARGO_TARGET_DIR`, `MCC_BIN`
and `MCC_SLOT`. Slot `a` is `target/`; slot `b` is `target/b/`; slot `c` is
`target/c/`. The pin is per session and survives the fresh shell each command
runs in, so later commands reuse the same slot and the same binary without
re-picking. Run the session's binary as `"$MCC_BIN"`; a `mcc` symlink to
`scripts/mcc-slot.sh` resolves the slot for you. `MCC_SLOT=a|b|c` forces one
command without moving the pin.

Slot `0` (`target/0/`) is the editor's and is **never** handed out by a claim:
`.vscode/settings.json` points `rust-analyzer.cargo.targetDir` at it, and
`.vscode/tasks.json` builds into it, so the editor's `cargo check` and debug
build share one dir and neither holds a slot an agent session needs. Reach it
from a shell with `MCC_SLOT=0`.

Never run `cargo clean`: slot `a` *is* `target/`, so a plain clean also deletes
slots `b`, `c`, `0` and the pins in `target/.slots`. Delete `target/debug`
instead.
