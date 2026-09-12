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
  a design doc under `mcd/doc/`, with at most a one-line pointer here. The same
  applies to module-level `//!`.
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
without matching one of the three passes the gate. Content inside a fenced
block, and a table row (content starting with `|`), are exempt. To keep a line
that is legitimately long — a diagram, a URL, a grammar production — mark it
with `check-comments:allow`. Do not put that marker on a `//!` line: it renders
into the published rustdoc.

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
