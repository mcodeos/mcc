#!/usr/bin/env python3
"""Check that tracked files contain no user-specific absolute paths.

Usage:
  python3 scripts/check-paths.py             # scan every git-tracked file
  python3 scripts/check-paths.py <file>...   # scan specific files only

Exit code: 0 = clean, 1 = at least one line embeds a home-directory path.

Background: a path like `/Users/<name>/work/mo/mcc` pins the file to one
developer's machine, leaks their account name, and breaks for everyone else.
The project rule (AGENTS.md, "no hardcoded user-specific absolute paths")
requires a portable form instead: `~` in docs and shell examples, `$HOME`- or
`env!("CARGO_MANIFEST_DIR")`-derived paths in code, or a path relative to the
repository root.

The detector is **structural**: it matches any home-directory prefix, so it
covers every account name (not just the current user's) without carrying a
list of names. Anything that reads as a placeholder passes untouched —
`/Users/<name>/repo`, `$HOME/...`, `~/...`, `./src/lib.rs`.

To show a counterexample in documentation, mark the line with
`check-paths:allow` (see ALLOW_MARKER).
"""
import re
import subprocess
import sys

# Home-directory prefixes, matched structurally:
#   POSIX    /Users/<name>/...   /home/<name>/...
#   Windows  C:\Users\<name>\...  (also forward slashes, repeated separators)
#
# The name segment must start with an alphanumeric, which is what makes the
# placeholder forms pass: `<name>`, `{user}`, `$USER` and `*` all begin with a
# non-alphanumeric and never match.
#
# The lookbehind rejects a match preceded by an identifier character, so a URL
# path segment that merely reads `/home/...` (e.g. `https://host/home/USERS`)
# is not flagged — only a real path boundary (start of line, whitespace,
# quote, `=`, `:`, and the like) starts a home-directory path.
LOOKBEHIND = r"(?<![A-Za-z0-9._-])"
NAME = r"[A-Za-z0-9][A-Za-z0-9._-]*"

HOME_PATH_RE = re.compile(
    rf"{LOOKBEHIND}(?:"
    rf"/(?:Users|home)/{NAME}"  # macOS / Linux
    rf"|[A-Za-z]:[\\/]+Users[\\/]+{NAME}"  # Windows
    rf")"
)

# Escape hatch for docs that must display a bad example: put this token
# anywhere on the line and the line is skipped.
ALLOW_MARKER = "check-paths:allow"


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
                    if ALLOW_MARKER in line:
                        continue
                    if HOME_PATH_RE.search(line):
                        bad.append((p, lineno, line.rstrip()[:120]))
        except OSError as e:
            print(f"check-paths: cannot read {p}: {e}", file=sys.stderr)
    return bad


def report(bad):
    if not bad:
        return 0
    print(
        f"check-paths: {len(bad)} line(s) embed a user-specific absolute path:",
        file=sys.stderr,
    )
    for p, lineno, s in bad:
        print(f"  {p}:{lineno}: {s}", file=sys.stderr)
    print(
        "check-paths: use a portable path instead — `~` in docs and shell "
        "examples, `$HOME` / `env!(\"CARGO_MANIFEST_DIR\")` in code, or a "
        "path relative to the repo root.",
        file=sys.stderr,
    )
    return 1


def main():
    paths = sys.argv[1:] or files_from_git()
    if not paths:
        print("check-paths: no files to check", file=sys.stderr)
        return 0
    return report(check(paths))


if __name__ == "__main__":
    sys.exit(main())
