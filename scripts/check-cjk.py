#!/usr/bin/env python3
"""Check that no CJK (Chinese) characters appear in tracked files.

Usage:
  python3 scripts/check-cjk.py             # scan every git-tracked file
  python3 scripts/check-cjk.py <file>...   # scan specific files only

Exit code: 0 = clean, 1 = at least one line contains CJK characters.
The project requires English only: comments, strings, diagnostics, test data.
"""
import re
import subprocess
import sys

# Basic CJK + Extension A + Compatibility Ideographs (covers all common Han).
CJK_RE = re.compile(r"[\u3400-\u4DBF\u4E00-\u9FFF\uF900-\uFAFF]")


def files_from_git():
    out = subprocess.run(
        ["git", "ls-files", "-z"], capture_output=True, text=True, check=True
    )
    return [f for f in out.stdout.split("\0") if f]


def check(paths):
    bad = []
    for p in paths:
        try:
            with open(p, encoding="utf-8", errors="replace") as f:
                for lineno, line in enumerate(f, 1):
                    if CJK_RE.search(line):
                        bad.append((p, lineno, line.rstrip()[:120]))
        except OSError as e:
            print(f"check-cjk: cannot read {p}: {e}", file=sys.stderr)
    return bad


def main():
    paths = sys.argv[1:] or files_from_git()
    if not paths:
        print("check-cjk: no files to check", file=sys.stderr)
        return 0
    bad = check(paths)
    if bad:
        print(
            f"check-cjk: {len(bad)} line(s) contain CJK (Chinese) characters:",
            file=sys.stderr,
        )
        for p, lineno, s in bad:
            print(f"  {p}:{lineno}: {s}", file=sys.stderr)
        print(
            "check-cjk: this project requires English only — translate before committing.",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
