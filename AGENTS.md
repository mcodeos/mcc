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
