# Contributing to MCC

## Code Standards

### Comments

- **All comments must be in English.**
- **Keep them lean.** Explain *why*, not *what* — if a comment can be deleted
  without losing a reason, a constraint, or a warning, delete it.
- No change diary (`// previously…`, `// renamed from…`, `// an early version
  used to…`) and no commented-out code: git holds the history.
- No `// ───── Parse ─────` banner rules — they are punctuation, not information.
- Keep `///` doc comments to the contract (arguments, invariants, panics, units).
  Longer rationale belongs in the design docs, with at most a one-line pointer.
- Keep comments up-to-date with code changes.

This is checked by `python3 scripts/check-comments.py` (also run by the
pre-commit hook, CI, and `scripts/check.sh` step 11). A line that is
legitimately long — a diagram, a URL, a grammar production — can be marked
with `check-comments:allow`.

The full rule is in [AGENTS.md](AGENTS.md).

### Code Style

- Follow Rust idioms and conventions.
- Use `cargo fmt` for formatting.
- Run `cargo clippy` to catch common mistakes.

### Commit Messages

- Use clear, descriptive commit messages.
- Start with a verb (e.g., "Add", "Fix", "Refactor").

