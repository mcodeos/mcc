#!/usr/bin/env python3
"""Reconciliation gate for the diagnostic codes the spec cites.

The catalog `src/db/diagnostic/errcodes.rs` is the single authority for which
diagnostic codes exist. A spec document may cite a code only when the catalog
declares it AND some emission site raises it: `src/rules.rs` is the rule
registry (it describes rules, it never raises one) and `src/ast/c/astdef.h` is
the parser alias table (it names the parser codes, it never raises one), so
neither counts as an emission site.

This keeps the docs from pointing at a code that no longer exists, or at one
that exists but nothing emits.

Usage:
    python3 scripts/check-errcodes.py [FILE ...]

With no arguments every `*.md` under the doc directory is read. With arguments
(the pre-commit form) the scan runs only when the staged set touches something
it reads, namely a path under `src/`.

The doc directory is `<repo>/../mcd/spec`, overridable with
`MCC_ERRCODES_DOC`. When it is absent -- a CI checkout holds one repository
only -- the scan reports the skip and exits 0.

Exit 0 clean, 1 on a stale citation.
"""

import os
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
SRC = REPO / "src"
CATALOG = SRC / "db" / "diagnostic" / "errcodes.rs"
REGISTRY = SRC / "rules.rs"
ALIASES = SRC / "ast" / "c" / "astdef.h"
DOC_DIR = Path(os.environ.get("MCC_ERRCODES_DOC", str(REPO.parent / "mcd" / "spec")))

# A code is `E` + four digits, delimited by anything that is not a word
# character. The lookbehind keeps a device part number such as `TLE7368E` from
# reading as a code, and the lookahead keeps `E12345` out.
CODE_RE = re.compile(r"(?<![0-9A-Za-z_])E(\d{4})(?![0-9])")
# A range such as `E3042`-`E3049` cites every code it spans.
RANGE_RE = re.compile(r"E(\d{4})\s*[-\u2013\u2014]\s*E(\d{4})")

# A document may also cite a code as a name/number pair rather than `E####`.
# Three shapes appear: prose runs the name first (`NAME` 3001), the catalog
# tables run the number first (`| 1001 | `NAME` |`), and the attribute chapter
# puts the `E####` and the name in adjacent cells.
PAIR_NAME_FIRST_RE = re.compile(r"`([A-Z][A-Z0-9_]{2,})`\s*=?\s*(\d{4})(?![0-9])")
PAIR_CODE_FIRST_RE = re.compile(r"`E(\d{4})`\s*[|\s]\s*`([A-Z][A-Z0-9_]{2,})`")
ROW_NAME_RE = re.compile(r"`([A-Z][A-Z0-9_]{2,})`")
ROW_NUM_RE = re.compile(r"(?<![0-9A-Za-z_])(\d{4})(?![0-9])")

CONST_RE = re.compile(r"^\s*pub const ([A-Z][A-Z0-9_]*): u32 = (\d+);", re.M)
ALIAS_RE = re.compile(r"^\s*#define\s+(MCD_[EW]\d{4}_[A-Z0-9_]+)\s+(\d+)", re.M)

# A Rust emission site names the constant through the `errcodes` module.
SITE_RE = re.compile(r"errcodes::([A-Z][A-Z0-9_]*)")
# A C emission site names the parser alias macro.
ALIAS_USE_RE = re.compile(r"\b(MCD_[EW]\d{4}_[A-Z0-9_]+)\b")

C_SUFFIXES = (".c", ".h", ".l", ".y")


def read_catalog():
    """Return the declared codes as `{number: constant name}`."""
    source = CATALOG.read_text(encoding="utf-8")
    return {int(num): name for name, num in CONST_RE.findall(source)}


def read_aliases():
    """Return the parser alias macros as `{number: macro name}`."""
    source = ALIASES.read_text(encoding="utf-8")
    return {int(num): macro for macro, num in ALIAS_RE.findall(source)}


def read_emissions(aliases):
    """Return the set of code numbers raised somewhere under `src/`."""
    emitted = set()
    for path in sorted(SRC.rglob("*")):
        if path == CATALOG or path == REGISTRY or path == ALIASES:
            continue
        if path.suffix == ".rs":
            for name in SITE_RE.findall(path.read_text(encoding="utf-8", errors="replace")):
                emitted.add(name)
        elif path.suffix in C_SUFFIXES:
            text = path.read_text(encoding="utf-8", errors="replace")
            for macro in ALIAS_USE_RE.findall(text):
                emitted.add(macro)
    return emitted


def code_citations(path):
    """Yield `(line number, code number)` for every `E####` a document cites."""
    text = path.read_text(encoding="utf-8")
    # The dash of a range may sit between two backticked codes; dropping the
    # backticks lets the range pattern see it, and a code is delimited by
    # non-word characters either way.
    text = text.replace("`", " ")
    for index, line in enumerate(text.splitlines(), start=1):
        covered = set()
        for start, end in RANGE_RE.findall(line):
            lo, hi = int(start), int(end)
            if lo > hi:
                lo, hi = hi, lo
            covered.update(range(lo, hi + 1))
            for code in range(lo, hi + 1):
                yield index, code
        # A range already covers its own endpoints.
        for code in CODE_RE.findall(RANGE_RE.sub(" ", line)):
            if int(code) not in covered:
                yield index, int(code)


def binding_citations(path):
    """Yield `(line number, name, code number)` for every name/number pair."""
    for index, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        seen = set()
        for name, code in PAIR_NAME_FIRST_RE.findall(line):
            seen.add((name, int(code)))
        for code, name in PAIR_CODE_FIRST_RE.findall(line):
            seen.add((name, int(code)))
        # A catalog row runs `| 1001 | `NAME` | meaning |`: exactly one bare
        # number and exactly one name, so the pair is unambiguous.
        if line.lstrip().startswith("|"):
            numbers = ROW_NUM_RE.findall(line)
            names = ROW_NAME_RE.findall(line)
            if len(numbers) == 1 and len(names) == 1:
                seen.add((names[0], int(numbers[0])))
        yield from ((index, name, code) for name, code in sorted(seen))


def main(argv):
    if argv and not any(a.startswith("src/") for a in argv):
        return 0
    if not DOC_DIR.is_dir():
        print("skip: spec directory not found at %s (sibling checkout required)" % DOC_DIR)
        return 0

    declared = read_catalog()
    aliases = read_aliases()
    by_name = {name: num for num, name in declared.items()}
    alias_by_name = {macro: num for num, macro in aliases.items()}

    raised = read_emissions(aliases)
    emitted = set()
    for name in raised:
        if name in by_name:
            emitted.add(by_name[name])
        elif name in alias_by_name:
            emitted.add(alias_by_name[name])

    problems = []
    cited = set()

    def check(rel, line, code, label):
        cited.add(code)
        if code not in declared:
            problems.append(
                "%s:%d: %s is not declared in errcodes.rs" % (rel, line, label)
            )
        elif code not in emitted:
            problems.append(
                "%s:%d: %s is declared but has no emission site" % (rel, line, label)
            )

    for path in sorted(DOC_DIR.glob("*.md")):
        rel = os.path.relpath(path, DOC_DIR)
        for line, code in code_citations(path):
            check(rel, line, code, "E%04d" % code)
        for line, name, code in binding_citations(path):
            table = alias_by_name if name.startswith("MCD_") else by_name
            if name not in table:
                problems.append(
                    "%s:%d: %s is not declared in %s"
                    % (rel, line, name, os.path.basename(ALIASES if name.startswith("MCD_") else CATALOG))
                )
            elif table[name] != code:
                problems.append(
                    "%s:%d: %s is code %d, not %d"
                    % (rel, line, name, table[name], code)
                )
            else:
                check(rel, line, code, "%s = E%04d" % (name, code))

    if problems:
        sys.stderr.write("errcodes: the spec has %d stale citation(s):\n" % len(problems))
        for problem in problems:
            sys.stderr.write("  %s\n" % problem)
        sys.stderr.write(
            "errcodes: %s is authoritative; a cited code must be declared there "
            "and raised somewhere under src/.\n" % os.path.relpath(CATALOG, REPO)
        )
        return 1

    print("errcodes: %d cited code(s), all declared and emitted" % len(cited))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
