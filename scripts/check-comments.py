#!/usr/bin/env python3
"""Check that code comments stay lean (AGENTS.md, "keep code comments lean").

Usage:
  python3 scripts/check-comments.py             # scan every git-tracked *.rs
  python3 scripts/check-comments.py <file>...   # scan specific files only

Exit code: 0 = clean, 1 = at least one comment violates a rule, or at least one
file could not be read (an unchecked file is not a clean one).

Three rules, all scoped to comments (`//`, `///`, `//!`) outside fenced code
blocks (a fence holds verbatim content, not prose about the code):

  1. decorative-rule  A banner/separator line, e.g. `// =============` or
                      `// ── Parse ──────`. Punctuation, not information.
  2. change-diary     Prose that narrates the code's history ("used to be X",
                      "previously Y"). Git holds the history; source states
                      only what is true now.
  3. over-length      The physical line exceeds 100 columns, rustfmt's default
                      `max_width`. Comments are not wrapped by rustfmt
                      (`wrap_comments` defaults to false), so nothing else
                      catches this. A table row (content starting with `|`) is
                      exempt: it cannot be reflowed without destroying the
                      table, so the rule has no remedy there — the same `|`
                      test that exempts rows from rule 1.

Scope is `*.rs` only. `mcc.yaml` is compiled in via `include_str!`, and
`src/ast/c/*.c` carries box-drawing inside runtime strings — neither is code
comments.

To show a counterexample, or to keep a line that is legitimately long (a
diagram, a URL, a grammar production), mark the line with
`check-comments:allow`. Do not add the marker to a `//!` line, which renders
into the published rustdoc — fix the line, or let the table exemption cover it.

Note on the change-diary word list: this is a detector for *English prose
wording*, not a list of symbol names. The repo rule against hardcoded name
lists (AGENTS.md) forbids gating language semantics on component or method
names; it does not forbid recognizing stock phrases in documentation. The list
below was converged against this repository's own corpus: every hit was read,
and any wording that produced a false positive was dropped. See the comments on
DIARY_PATTERNS for the specific words that were rejected and why.
"""
import re
import subprocess
import sys

# Escape hatch, per line, mirroring `check-paths:allow` in check-paths.py.
ALLOW_MARKER = "check-comments:allow"

# rustfmt's default `max_width` (this repo has no rustfmt.toml).
MAX_LEN = 100

# Only Rust source. Everything else has its own conventions.
RUST_FILE_RE = re.compile(r"\.rs$")

# Comment markers, longest first so `///` is not read as `//` + `/`.
MARKERS = ("///", "//!", "//")

# Rule characters for a decorative banner. Deliberately excludes:
#   `#`  — `// ### Heading` is a Markdown heading in a doc comment
#   `*`  — `// ***` is a Markdown thematic break
#   the box-drawing corners/joins (┌ ┐ └ ┘ ├ ┤ ┬ ┴ ┼ │ ║ …) — see BOX_CHARS.
RULE_CHARS = "=-─━═_"
RULE_RUN_RE = re.compile(f"[{re.escape(RULE_CHARS)}]{{4,}}")

# Box-drawing glyphs that are NOT part of a plain rule run: corners, joins and
# verticals. A diagram that reaches the edge test still carries one of these in
# its body (`└────┬────┘`), so their presence in what is left after removing the
# rule runs marks the line as a picture, not a banner. `─ ━ ═` are absent here:
# they are rule characters, and a banner is nothing but rule characters.
BOX_CHARS = "┌┐└┘├┤┬┴┼│║╔╗╚╝╠╣╦╩╬"

# A fenced block inside a doc comment holds verbatim content — a diagram, a
# sample, a table. Its lines are not prose about the code, so every rule below
# stops at the fence markers. Only the `///` family can open one; `//` and the
# interior lines of a real code fence in the source are ordinary code.
FENCE_RE = re.compile(r"^(```|~~~)")


def files_from_git():
    out = subprocess.run(
        ["git", "ls-files", "-z"], capture_output=True, text=True, check=True
    )
    return [f for f in out.stdout.split("\0") if f]


def comment_content(line):
    """Return the text after a leading comment marker, or None if not a comment.

    A line whose first non-space characters start a `//`-family comment. This
    naive test is safe for this repository: a string-aware scan (raw strings,
    escapes, char literals) found zero lines that start with `//` while inside
    a string literal.
    """
    t = line.strip()
    for m in MARKERS:
        if t.startswith(m):
            return t[len(m):].strip()
    return None


def is_decorative_rule(content):
    """True if the comment content is a banner/separator line.

    A rule run must sit at the very start of the content, or at the very end —
    a banner is drawn out to an edge. That is what keeps diagrams and lane
    pictures out: in `+---+`, `┌───┐` and `u1.3 ──── nA ──── u2.3` no run
    touches an edge, and counting *interior* runs instead would have matched
    them (`└────┬────┘` splits into two runs at the `┬` join).

    A diagram that survives the edge test — a rule of dashes ending flush
    against the content end, with a corner before it — is caught by BOX_CHARS:
    what is left of the line once the rule runs are removed must be a title,
    not more picture.
    """
    if not content or "|" in content:
        return False
    runs = list(RULE_RUN_RE.finditer(content))
    if not runs:
        return False
    if not (runs[0].start() == 0 or runs[-1].end() == len(content)):
        return False
    return not any(c in BOX_CHARS for c in RULE_RUN_RE.sub("", content))


def is_bare_rule(content):
    """True if the content is *only* rule characters — no title to keep."""
    return is_decorative_rule(content) and not RULE_RUN_RE.sub("", content).strip()


# Change-diary wordings. This list is a deliberately HIGH-PRECISION SUBSET of
# the rule in AGENTS.md: the rule also forbids `// now also handles Y`, but no
# mechanical pattern for that reached zero false positives, and a gate that
# cries wolf gets switched off. A comment can therefore violate the written
# rule without this gate firing — the gate is a net, not a proof.
#
# Each kept pattern was validated by reading every hit in this repository.
#   `previously`      every hit was history narration except one, "a previously
#                     captured version" (src/rpc/handlers/defs.rs:300), where
#                     `previously` is attributive — an adjective on a runtime
#                     value, not a statement about the code. The lookbehinds
#                     exclude that shape only: `a/an/the` + `previously` is an
#                     adjective; a bare `previously` at a clause start, or after
#                     a verb ("was previously…"), is the diary sense.
#   `used to be`      13/13 hits are history narration. ("It used to be…" and
#                     "the tie that used to be…" are both history; only
#                     `previously` has the attributive trap, so only it is
#                     narrowed.)
#   `formerly`        8/8 hits are diagnostic-code renames ("4114 (formerly 4117)").
#   `an early version`  the one hit is history narration.
#   `renamed from`     zero hits today; kept because it has no benign reading.
#
# Rejected candidates, with the evidence that killed them:
#   `used to \w+`   ~50% false positives: "the row's name and uri are used to
#                   fetch" (src/cmds/filter.rs:35), "Used to exempt"
#                   (src/cmds/verify.rs:822) — `used to <verb>` means "employed
#                   for", which is PRESENT purpose, not history.
#   `no longer`     states a current fact: "USB/LDO/DCDC are no longer islands"
#                   (tests/rail_rules.rs:9).
#   `now also`, `at one point`, `once was`
#                   no validating hits, and each has a benign reading
#                   ("at one point the wire crosses the box" is locational).
#   `changed from`  one hit, and "the value changed from A to B" describes
#                   runtime behaviour, not code history.
DIARY_PATTERNS = [
    r"(?<!\ba )(?<!\ban )(?<!\bthe )\bpreviously\b",
    r"\bformerly\b",
    r"\brenamed from\b",
    r"\ban early version\b",
    r"\bused to be\b",
]
DIARY_RE = re.compile("|".join(DIARY_PATTERNS), re.IGNORECASE)


def check(paths):
    """Return (bad, unreadable).

    `bad` is a list of (path, lineno, class, line). An unreadable file is a
    failure, not a skip: an unchecked file is indistinguishable from a clean
    one, so silently passing it turns the gate green for exactly the files
    nobody verified.
    """
    bad = []
    unreadable = []
    for p in paths:
        if not RUST_FILE_RE.search(p):
            continue
        in_fence = False
        try:
            with open(p, encoding="utf-8", errors="replace") as f:
                for lineno, line in enumerate(f, 1):
                    if ALLOW_MARKER in line:
                        continue
                    content = comment_content(line)
                    if content is not None and FENCE_RE.match(content):
                        in_fence = not in_fence
                        continue
                    if content is None or in_fence:
                        continue
                    if is_decorative_rule(content):
                        bad.append((p, lineno, "decorative-rule", line.rstrip()[:120]))
                        continue
                    if DIARY_RE.search(content):
                        bad.append((p, lineno, "change-diary", line.rstrip()[:120]))
                        continue
                    if len(line.rstrip()) > MAX_LEN and not content.startswith("|"):
                        bad.append((p, lineno, "over-length", line.rstrip()[:120]))
        except OSError as e:
            unreadable.append((p, e))
    return bad, unreadable


def report(bad, unreadable=()):
    rc = 0
    if unreadable:
        print(
            f"check-comments: {len(unreadable)} file(s) could not be read:",
            file=sys.stderr,
        )
        for p, e in unreadable:
            print(f"  {p}: {e}", file=sys.stderr)
        print(
            "check-comments: an unreadable file is unchecked, not clean — "
            "restore it or drop it from the index.",
            file=sys.stderr,
        )
        rc = 1
    if not bad:
        return rc
    counts = {}
    for _, _, cls, _ in bad:
        counts[cls] = counts.get(cls, 0) + 1
    summary = ", ".join(f"{n} {cls}" for cls, n in sorted(counts.items()))
    print(f"check-comments: {len(bad)} comment line(s) violate the rule ({summary}):", file=sys.stderr)
    for p, lineno, cls, s in bad:
        print(f"  {p}:{lineno}: [{cls}] {s}", file=sys.stderr)
    print(
        "check-comments: keep comments lean — drop decorative rule lines, state "
        "what is true now instead of what used to be (git holds history), and "
        "wrap comment prose to 100 columns. See AGENTS.md, \"keep code comments "
        "lean\". Mark a legitimate exception with `check-comments:allow`.",
        file=sys.stderr,
    )
    return 1


def main():
    paths = sys.argv[1:] or files_from_git()
    if not paths:
        print("check-comments: no files to check", file=sys.stderr)
        return 0
    bad, unreadable = check(paths)
    return report(bad, unreadable)


if __name__ == "__main__":
    sys.exit(main())
