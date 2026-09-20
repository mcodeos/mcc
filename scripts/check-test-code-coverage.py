#!/usr/bin/env python3
"""Diagnostic-code test-coverage gate (ratchet).

Every code declared in `src/db/diagnostic/errcodes.rs` should be *cited* by at
least one test, so that renumbering or repurposing a code turns a test red
(the coverage ledger doctrine). This gate implements the 2026-09-20 audit
methodology as a ratchet: the set of declared-but-never-cited codes may only
shrink. A new code that ships without a citing test fails the gate; a code
that gains coverage must have its baseline row removed by the same batch.

A code counts as cited when it appears, in any form, inside a test body or a
test asset:

  - `E4176` literal, bare 4-digit, `errcodes::SYMBOL`, or bare SCREAMING symbol
  - `tests/` files count as a whole (helpers and module docs cite too);
    `src/` counts only the bodies of `#[test]` functions, so the catalog
    cannot vouch for itself
  - assets: `tests/{golden,corpus,projects,fixtures}`

Codes whose ALL_CODES message starts with "Retired" are exempt: a retired
code's producer is gone by ruling, and the registration is kept only so old
numbers resolve.

Usage:
  python3 scripts/check-test-code-coverage.py            # gate (ratchet vs baseline)
  python3 scripts/check-test-code-coverage.py --report   # print the zero-cited set

Exit code: 0 = no regression, 1 = a declared code went uncited that the
baseline does not list, or an unreadable file.
"""
import re
import subprocess
import sys
from pathlib import Path

ERRCODES = "src/db/diagnostic/errcodes.rs"
BASELINE = Path("scripts/test-code-coverage-baseline.txt")
ASSET_DIRS = ("tests/golden", "tests/corpus", "tests/projects", "tests/fixtures")
TEST_DIRS = ("tests", "src")

CONST_RE = re.compile(r"pub const ([A-Z_0-9]+): u32 = (\d+);")
ENTRY_RE = re.compile(r"entry!\(\s*([A-Z_0-9]+)\s*,")
TEST_FN_RE = re.compile(r"#\[(?:tokio::)?test\]")
CITE_RE = re.compile(r"\b(?:E(\d{4})|(\d{4})|errcodes::([A-Z_0-9]+)|([A-Z][A-Z_0-9]{3,}))\b")


def git_tracked():
    out = subprocess.run(
        ["git", "ls-files", "-z", "tests", "src"], capture_output=True, text=True, check=True
    )
    return [f for f in out.stdout.split("\0") if f]


def declared_codes(text):
    """number -> symbol, for every declared code."""
    codes = {}
    for sym, num in CONST_RE.findall(text):
        codes.setdefault(int(num), sym)
    return codes


def retired_numbers(text):
    """Numbers whose ALL_CODES message starts with 'Retired'."""
    retired = set()
    for m in ENTRY_RE.finditer(text):
        seg = text[m.end(): m.end() + 200]
        if re.match(r"\s*,?\s*\"Retired", seg):
            sym = m.group(1)
            for n, s in declared_codes(text).items():
                if s == sym:
                    retired.add(n)
    return retired


def fn_bodies(text):
    """Yield the body of every #[test] function (brace-matched)."""
    for m in TEST_FN_RE.finditer(text):
        rest = text[m.end():]
        fn = re.search(r"fn \w+\([^)]*\)[^{]*\{", rest)
        if not fn:
            continue
        start = m.end() + fn.end() - 1
        depth = 0
        i = start
        while i < len(text):
            if text[i] == "{":
                depth += 1
            elif text[i] == "}":
                depth -= 1
                if depth == 0:
                    yield text[start: i + 1]
                    break
            i += 1


def cited_numbers(text):
    """All code numbers a text cites, in any of the accepted forms."""
    by_num = declared_codes(text)
    by_sym = {s: n for n, s in by_num.items()}
    found = set()
    for m in CITE_RE.finditer(text):
        e_num, bare, sym, upper = m.groups()
        if e_num:
            found.add(int(e_num))
        elif bare:
            found.add(int(bare))
        elif sym and sym in by_sym:
            found.add(by_sym[sym])
        elif upper and upper in by_sym:
            found.add(by_sym[upper])
    return found


def collect_cited():
    cited = set()
    for p in git_tracked():
        path = Path(p)
        if not path.exists():
            # Deleted in the working tree, still in the index (a pending
            # deletion) - there is nothing left to scan.
            continue
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError as e:
            print(f"check-test-code-coverage: unreadable {p}: {e}", file=sys.stderr)
            continue
        in_asset = any(p == d or p.startswith(d + "/") for d in ASSET_DIRS)
        if in_asset or p.startswith("tests/"):
            cited |= cited_numbers(text)
        else:  # src/: function bodies of #[test] fns only
            for body in fn_bodies(text):
                cited |= cited_numbers(body)
    return cited


def main():
    flag_report = "--report" in sys.argv[1:]
    text = Path(ERRCODES).read_text(encoding="utf-8", errors="replace")
    declared = declared_codes(text)
    retired = retired_numbers(text)
    cited = collect_cited()

    zero = sorted(n for n in declared if n not in cited and n not in retired)
    if flag_report:
        print(f"declared {len(declared)}, retired {len(retired)}, "
              f"cited {len(declared) - len(zero) - len(retired & set())}, "
              f"zero-cited {len(zero)}")
        for n in zero:
            print(f"  E{n} {declared[n]}")
        return 0

    baseline = set()
    if BASELINE.exists():
        baseline = {int(line) for line in BASELINE.read_text().split() if line.strip()}

    regressions = sorted(set(zero) - baseline)
    if regressions:
        print("check-test-code-coverage: newly declared code(s) without a "
              "citing test:", file=sys.stderr)
        for n in regressions:
            print(f"  E{n} {declared[n]}", file=sys.stderr)
        print("cite the code from a test (any form: E%d, bare number, "
              "errcodes::SYMBOL) or shrink the baseline in the same batch "
              "if coverage was deliberately dropped.", file=sys.stderr)
        return 1
    print(f"check-test-code-coverage: no regression ({len(zero)} zero-cited, "
          f"baseline {len(baseline)})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
