#!/usr/bin/env python3
"""Check that name comparisons stay exact (registry: `01-lexical.md` §2.2).

A name in mcc — class, instance, pin, net, function, constructor, enum and enum
value — compares by exact string equality, with no case normalization at all.
`eq_ignore_ascii_case` is therefore a normalizing shortcut, and a shortcut is
legal only where it is *registered*.

The whitelist below is the registry: it names the registered call sites, by file
and allowed occurrence count. A call anywhere else, or one occurrence past the
registered count, fails the gate. Registered faces are not names — they are
user-facing input word tables (query-DSL keywords, `--type` kind words, the
`...GATE=0|false` environment variables), which users may spell in any case.

Two things never need an entry:

  - a mention inside a comment: prose, not a comparison;
  - a call outside `src/`: this gate guards the library, and tests are free to
    compare whatever they like.

Scope is git-tracked `src/**/*.rs`. The gate is a net, not a proof: a comparison
written by hand (`a.to_lowercase() == b.to_lowercase()`) passes, and so does a
call reached through a helper that is itself registered.

Usage:
  python3 scripts/check-name-case.py             # scan git-tracked src *.rs
  python3 scripts/check-name-case.py <file>...   # scan specific files only

Exit code: 0 = clean, 1 = an unregistered call, a stale registry entry, or an
unreadable file (an unchecked file is not a clean one).
"""
import re
import subprocess
import sys

TOKEN = "eq_ignore_ascii_case"

RUST_FILE_RE = re.compile(r"\.rs$")
SRC_PREFIX = "src/"

# Registry (01-lexical.md §2.2): relative path -> allowed occurrences of TOKEN.
# Adding a call means raising a count here *and* registering the face in the
# spec; removing one means lowering the count, so the two never drift apart.
REGISTERED = {
    "src/query/search/dsl.rs": 7,
    "src/cmds/show.rs": 2,
    "src/rpc/handlers/show.rs": 2,
    "src/rpc/handlers/mod.rs": 1,
    "src/cmds/build.rs": 2,
}

COMMENT_PREFIXES = ("//", "/*", "*")


def files_from_git():
    out = subprocess.run(
        ["git", "ls-files", "-z"], capture_output=True, text=True, check=True
    )
    return [f for f in out.stdout.split("\0") if f]


def is_comment(line):
    return line.lstrip().startswith(COMMENT_PREFIXES)


def scan(paths, full):
    """Return (bad, unreadable).

    `bad` is a list of (path, found, registered). In full mode a registered file
    that no longer exists is itself an entry to fix, so the registry cannot
    silently grow dead rows.
    """
    counts = {}
    read = []
    unreadable = []
    for p in paths:
        if not RUST_FILE_RE.search(p) or not p.startswith(SRC_PREFIX):
            continue
        read.append(p)
        try:
            with open(p, encoding="utf-8", errors="replace") as f:
                n = sum(1 for line in f if not is_comment(line) and TOKEN in line)
        except OSError as e:
            unreadable.append((p, e))
            continue
        if n:
            counts[p] = n

    bad = []
    for p in sorted(set(read)):
        found = counts.get(p, 0)
        registered = REGISTERED.get(p, 0)
        if found != registered:
            bad.append((p, found, registered))
    if full:
        live = set(read)
        for p in sorted(REGISTERED):
            if p not in live:
                bad.append((p, 0, REGISTERED[p]))
    return bad, unreadable


def report(bad, unreadable=()):
    rc = 0
    if unreadable:
        print(
            f"check-name-case: {len(unreadable)} file(s) could not be read:",
            file=sys.stderr,
        )
        for p, e in unreadable:
            print(f"  {p}: {e}", file=sys.stderr)
        print(
            "check-name-case: an unreadable file is unchecked, not clean — "
            "restore it or drop it from the index.",
            file=sys.stderr,
        )
        rc = 1
    if not bad:
        return rc
    print(
        f"check-name-case: {len(bad)} file(s) hold an unregistered or stale "
        "case-insensitive comparison:",
        file=sys.stderr,
    )
    for p, found, registered in bad:
        print(f"  {p}: found {found}, registered {registered}", file=sys.stderr)
    print(
        "check-name-case: a name compares by exact string equality, with no case "
        "normalization. Compare with `==` instead, or — if the face is a "
        "user-facing word table, not a name — register it in `01-lexical.md` §2.2 "
        "and raise the REGISTERED count in this script.",
        file=sys.stderr,
    )
    return 1


def main():
    explicit = sys.argv[1:]
    paths = explicit or files_from_git()
    if not paths:
        print("check-name-case: no files to check", file=sys.stderr)
        return 0
    bad, unreadable = scan(paths, full=not explicit)
    return report(bad, unreadable)


if __name__ == "__main__":
    sys.exit(main())
