# MCode Compiler (mcc) — Authoring Rules

This repository is the MCode compiler. All content authored here — by humans
or by AI agents — must be written in English.

## Rule: English for unified interface

All content shall be written in English using ASCII characters only; non-ASCII
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
