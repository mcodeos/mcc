# MCode Compiler (mcc) — Authoring Rules

This repository is the MCode compiler. All content authored here — by humans
or by AI agents — must be written in English.

## Hard rule: English only

No CJK (Chinese) characters anywhere in tracked files, commit messages, or
PR descriptions. This is enforced mechanically:

- Pre-commit hook (`.githooks/pre-commit`, installed via
  `git config core.hooksPath .githooks`) rejects staged content and commit
  messages containing CJK characters.
- CI workflow (`.github/workflows/check-cjk.yml`) scans every git-tracked
  file on every push / pull request and fails the build on CJK characters.
- Local scanner: `python3 scripts/check-cjk.py` — exit 0 clean, 1 otherwise.

Applies to:

- Code comments (`//`, `///`, `//!`), doc comments, and identifiers.
- String literals: log / diagnostic / panic messages and CLI output.
- Test names, test comments, and test data (including `tests/golden/hbl/*.golden.toml`).
- Commit messages and pull request descriptions.

If a Chinese term must appear in a diagnostic for users, provide it in a
translation layer or docs — never in source strings.
