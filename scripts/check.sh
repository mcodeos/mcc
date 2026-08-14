#!/usr/bin/env bash
# mcc pre-push / pre-merge quality gate.
#
# Tool roles:
#   check    — fastest compile-time type check
#   fmt      — code formatting (--check: read-only)
#   clippy   — static analysis: logic / performance / dead code
#   test     — unit + integration tests
#   doc      — doc build validity (-D warnings)
#   audit    — dependency vulnerability scan (cargo-audit)
#   machete  — unused dependencies in Cargo.toml (cargo-machete)
#   fix      — auto-apply rustc lint suggestions (MODIFIES CODE, opt-in)
#   miri     — undefined behavior / unsafe memory checks (nightly, slow, opt-in)
#   cjk      — project rule: English only (scripts/check-cjk.py)
#
# Opt-in steps are gated behind env vars because they change the working
# tree (fix) or need a nightly toolchain and are very slow (miri):
#   RUN_FIX=1   scripts/check.sh
#   RUN_MIRI=1  scripts/check.sh
set -e

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "===== 1. cargo check (type check) ====="
cargo check --all-targets --all-features

echo "===== 2. fmt check ====="
cargo fmt --all --check

echo "===== 3. clippy (static analysis) ====="
cargo clippy --all-targets --all-features -- -D warnings

if [ "${RUN_FIX:-0}" = "1" ]; then
    echo "===== 3b. cargo fix (auto-apply lint suggestions) ====="
    cargo fix --all-targets --all-features --allow-dirty --allow-staged
fi

echo "===== 4. test ====="
cargo test --all-targets --all-features

echo "===== 5. doc check ====="
RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps

echo "===== 6. audit (dependency vulnerabilities) ====="
if command -v cargo-audit >/dev/null 2>&1; then
    cargo audit
else
    echo "skip: cargo-audit not installed (cargo install cargo-audit)"
fi

echo "===== 7. machete (unused dependencies) ====="
if command -v cargo-machete >/dev/null 2>&1; then
    cargo machete
else
    echo "skip: cargo-machete not installed (cargo install cargo-machete)"
fi

if [ "${RUN_MIRI:-0}" = "1" ]; then
    echo "===== 8. miri (UB / memory safety, needs nightly) ====="
    cargo +nightly miri test --all-targets
fi

echo "===== 9. cjk scan (english-only rule) ====="
python3 scripts/check-cjk.py

echo "all checks passed"
