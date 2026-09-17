#!/usr/bin/env python3
"""Ledger reconciliation gate for the attribute key registry.

The ledger has two copies: the authoritative table in `mcd/spec/07-attrs.md`
section 3.1, and its mirror `ATTR_KEYS` in `src/semantic/basic/attr_keys.rs`.
This scanner reads both and fails when a row disagrees, so the two cannot
drift.

Usage:
    python3 scripts/check-attr-keys.py [FILE ...]

With no arguments both sides are read. With arguments (the pre-commit form)
the scan runs only when the mirror is among them; the doc lives in a sibling
repository and is never staged here.

The doc is located at `<repo>/../mcd/spec/07-attrs.md`, overridable with
`MCC_ATTR_KEYS_DOC`. When it is absent -- a CI checkout holds one repository
only -- the scan reports the skip and exits 0.

Exit 0 clean, 1 on a mismatch.
"""

import os
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
MIRROR = REPO / "src" / "semantic" / "basic" / "attr_keys.rs"
MIRROR_REL = "src/semantic/basic/attr_keys.rs"
DOC = Path(
    os.environ.get("MCC_ATTR_KEYS_DOC", str(REPO.parent / "mcd" / "spec" / "07-attrs.md"))
)

# The ledger table is located by an ASCII marker line in the doc, never by its
# header text: this repository is English-only, so a scanner may not carry the
# doc's own words as a literal.
DOC_MARKER = "attr-keys-ledger:"

# The constructors that build a row, and the columns each one fills. A
# `vocab_row` may be written with a key constant instead of a literal (`KEY_ROLE`
# instead of `"role"`), so the key a reader asks for and the key the row
# registers are one spelling: `str_constants` resolves it the way
# `face_constants` resolves the face argument.
ROW_KINDS = ("row", "value_row", "voltage_row", "contract_row", "element_row", "vocab_row")

COLUMNS = ("key", "faces", "value", "contract", "admission", "arity", "supply", "vocab")

# The three states the word column carries. The words themselves are named by
# constants on the mirror side (`WORD_SHUNT`), so the column is only comparable
# once the names are resolved to the words they hold.
VOCAB_UNREGISTERED = "-"
VOCAB_FLAG = "flag"


def split_args(text):
    """Split a constructor's argument list on its top-level commas."""
    args, depth, cur = [], 0, ""
    for ch in text:
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
        if ch == "," and depth == 0:
            args.append(cur.strip())
            cur = ""
        else:
            cur += ch
    if cur.strip():
        args.append(cur.strip())
    return args


def face_constants(source):
    """Map each `const NAME: &[AttrFace]` to the face names it lists."""
    table = {}
    for m in re.finditer(
        r"const\s+(\w+)\s*:\s*&\[AttrFace\]\s*=\s*&\[([^\]]*)\]", source
    ):
        table[m.group(1)] = re.findall(r"AttrFace::(\w+)", m.group(2))
    return table


def str_constants(source):
    """Map each `const NAME: &str = "word"` to the word it holds.

    Covers both the key constants (`KEY_ROLE`) and the value-word constants
    (`WORD_SHUNT`): each is one spelling that a row refers to by name.
    """
    return {
        m.group(1): m.group(2)
        for m in re.finditer(r'const\s+(\w+)\s*:\s*&str\s*=\s*"([^"]*)"', source)
    }


def word_sets(source):
    """Map each `const NAME: &[&str]` to the word constants it lists."""
    return {
        m.group(1): re.findall(r"\w+", m.group(2))
        for m in re.finditer(
            r"const\s+(\w+)\s*:\s*&\[&str\]\s*=\s*&\[([^\]]*)\]", source
        )
    }


def vocab_token(arg, sets, words):
    """Render a row's vocabulary argument the way the doc spells it."""
    if arg == "AttrVocab::Flag":
        return VOCAB_FLAG
    listed = re.match(r"AttrVocab::Words\((\w+)\)", arg)
    if listed:
        return ", ".join("`%s`" % words[w] for w in sets[listed.group(1)])
    return VOCAB_UNREGISTERED


def value_token(arg):
    """Render a row's value argument the way the doc spells it."""
    if arg == "None":
        return "-"
    if arg.startswith("Some(") and arg.endswith(")"):
        arg = arg[5:-1]
    quantity = re.match(r"AttrValueKind::Quantity\(McUnit::(\w+)\)", arg)
    if quantity:
        return "Quantity(%s)" % quantity.group(1)
    named = re.match(r"AttrValueKind::(\w+)", arg)
    if named:
        return {"Text": "Text", "Count": "Number"}.get(named.group(1), named.group(1))
    return arg


def empty_row(key):
    return {
        "key": key,
        "faces": "",
        "value": "-",
        "contract": "Plain",
        "admission": "general",
        "arity": "Single",
        "supply": "no",
        "vocab": VOCAB_UNREGISTERED,
    }


def read_mirror():
    """Parse `ATTR_KEYS` into rows of the doc's columns."""
    source = MIRROR.read_text(encoding="utf-8")
    consts = face_constants(source)
    keys = str_constants(source)
    sets = word_sets(source)
    start = source.index("ATTR_KEYS: &[AttrKeyDef] = &[")
    end = source.index("\n];", start)
    body = source[start:end]

    if "AttrKeyArity::Set" in body:
        sys.stderr.write(
            "attr-keys: a literal row carrying AttrKeyArity::Set is not supported by "
            "this scanner; teach it the literal form in %s\n" % MIRROR_REL
        )
        sys.exit(1)

    rows = []
    for m in re.finditer(r"\b(%s)\(" % "|".join(ROW_KINDS), body):
        depth, i = 1, m.end()
        while depth:
            if body[i] == "(":
                depth += 1
            elif body[i] == ")":
                depth -= 1
            i += 1
        args = split_args(body[m.end() : i - 1])
        kind = m.group(1)
        first = args[0].strip('"')
        row = empty_row(keys.get(first, first))
        row["faces"] = ", ".join(consts[args[1]])

        if kind in ("row", "vocab_row"):
            # A `vocab_row` fills the same columns as a `row` plus the word
            # column; its admission is compared like any other reserved/general
            # flag.
            if args[2] == "false":
                row["admission"] = "reserved"
            if kind == "vocab_row":
                row["vocab"] = vocab_token(args[3], sets, keys)
        elif kind == "value_row":
            row["value"] = value_token(args[2])
        elif kind == "voltage_row":
            row["value"] = value_token(args[2])
            row["supply"] = "yes"
        elif kind == "contract_row":
            row["value"] = value_token(args[2])
            row["contract"] = args[3].split("::")[-1]
        elif kind == "element_row":
            # A value row that also marks what the element is
            # (`AttrKeyDef::element`). The ledger has no column for that
            # classification, so only the columns it does carry are compared.
            row["value"] = value_token(args[2])
        rows.append(row)
    return rows


def read_doc():
    """Parse the ledger table of section 3.1 into the same rows."""
    lines = DOC.read_text(encoding="utf-8").splitlines()

    marker = next((i for i, line in enumerate(lines) if DOC_MARKER in line), None)
    if marker is None:
        sys.stderr.write(
            "attr-keys: no ledger marker in %s; section 3.1 must carry a line "
            "containing %r above its table\n" % (DOC, DOC_MARKER)
        )
        sys.exit(1)

    header = next(
        (i for i in range(marker, len(lines)) if lines[i].startswith("|")), None
    )
    if header is None:
        sys.stderr.write("attr-keys: no table follows the ledger marker in %s\n" % DOC)
        sys.exit(1)

    rows = []
    for line in lines[header + 2 :]:
        if not line.startswith("|"):
            break
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) != len(COLUMNS):
            sys.stderr.write(
                "attr-keys: the ledger table in %s has %d columns, expected %d: %s\n"
                % (DOC, len(cells), len(COLUMNS), line)
            )
            sys.exit(1)
        row = dict(zip(COLUMNS, cells))
        row["key"] = row["key"].strip("`")
        rows.append(row)
    return rows


def describe(row):
    return "/".join(row[c] for c in COLUMNS[1:])


def main(argv):
    if argv and not any(os.path.normpath(a) == MIRROR_REL for a in argv):
        return 0
    if not DOC.is_file():
        print("skip: ledger doc not found at %s (sibling checkout required)" % DOC)
        return 0

    mirror = {r["key"]: r for r in read_mirror()}
    doc = {r["key"]: r for r in read_doc()}

    problems = []
    for key in sorted(set(doc) | set(mirror)):
        if key not in mirror:
            problems.append("'%s' is in the doc table but not in ATTR_KEYS" % key)
        elif key not in doc:
            problems.append("'%s' is in ATTR_KEYS but not in the doc table" % key)
        elif describe(doc[key]) != describe(mirror[key]):
            problems.append(
                "'%s': doc says '%s', ATTR_KEYS says '%s'"
                % (key, describe(doc[key]), describe(mirror[key]))
            )

    if problems:
        sys.stderr.write(
            "attr-keys: the ledger doc and ATTR_KEYS disagree (%d row(s)):\n" % len(problems)
        )
        for p in problems:
            sys.stderr.write("  %s\n" % p)
        sys.stderr.write(
            "attr-keys: %s is authoritative; %s is its mirror.\n" % (DOC, MIRROR_REL)
        )
        return 1

    print("attr-keys: %d rows agree" % len(mirror))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
