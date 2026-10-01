#!/usr/bin/env python3
"""Ledger reconciliation gate for the attribute key registry.

The ledger has two copies: the authoritative tables in `doc/NAMING.md`
section 8.3 (the language-core keys, one table per admission), and the
mirror `ATTR_KEYS` in `src/semantic/basic/attr_keys.rs` (which also
carries the domain keys until the metadata batch takes them). The scan
is a subset check: every doc row must exist in the mirror with the same
faces, value, admission, and words; mirror-only rows are reported, not
failed, so the two cannot drift on what the doc does carry.

Usage:
    python3 scripts/check-attr-keys.py [FILE ...]

With no arguments both sides are read. With arguments (the pre-commit form)
the scan runs when either side is among them.

Exit 0 clean, 1 on a mismatch.
"""

import os
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
MIRROR = REPO / "src" / "semantic" / "basic" / "attr_keys.rs"
MIRROR_REL = "src/semantic/basic/attr_keys.rs"
DOC = REPO / "doc" / "NAMING.md"
DOC_REL = "doc/NAMING.md"

# The ledger table is located by an ASCII marker line in the doc, never by its
# header text: this repository is English-only, so a scanner may not carry the
# doc's own words as a literal.
DOC_MARKER = "attr-keys-ledger:"

# The constructors that build a row, and the columns each one fills. A
# `vocab_row` may be written with a key constant instead of a literal (`KEY_ROLE`
# instead of `"role"`), so the key a reader asks for and the key the row
# registers are one spelling: `str_constants` resolves it the way
# `face_constants` resolves the face argument.
ROW_KINDS = ("row", "value_row", "voltage_row", "contract_row", "element_row", "vocab_row", "open_row")

# Columns compared between doc and mirror. The doc tables cover the
# language-core keys only (domain keys live mirror-only until the metadata
# batch), so admission — which table a row sits in — is doc-side state, and
# the mirror's contract / supply / arity fields are outside the comparison.
COLUMNS = ("key", "faces", "value", "admission", "vocab")

# The four states the word column carries. The words themselves are named by
# constants on the mirror side (`WORD_SHUNT`), so the column is only comparable
# once the names are resolved to the words they hold.
VOCAB_UNREGISTERED = "-"
VOCAB_FLAG = "flag"
VOCAB_OPEN = "open"


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
    if arg == "AttrVocab::Open":
        return VOCAB_OPEN
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
        "admission": "general",
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
        elif kind == "open_row":
            # An open-vocabulary row (`AttrVocab::Open`, contract-design.md
            # §1.8's fourth state): the value kind is registered, the word set
            # is the author's own identifiers, and the word is reserved for the
            # `@key(word)` row form (`general = false` is fixed in the
            # constructor, so the scanner carries it, not the source).
            row["value"] = value_token(args[2])
            row["vocab"] = VOCAB_OPEN
            row["admission"] = "reserved"
        rows.append(row)
    return rows


def read_doc():
    """Parse the ledger tables of section 8.3 into the same rows.

    Section 8.3 carries two tables, one per admission: each sits under a
    `attr-keys-ledger-table: <admission>` comment token, and the admission
    is carried by which table a row sits in, not by a cell. Each token
    owns the table that follows it (a prose heading may sit between);
    the first non-table line after the table closes the block.
    """
    lines = DOC.read_text(encoding="utf-8").splitlines()

    marker = next((i for i, line in enumerate(lines) if DOC_MARKER in line), None)
    if marker is None:
        sys.stderr.write(
            "attr-keys: no ledger marker in %s; section 8.3 must carry a line "
            "containing %r above its tables\n" % (DOC, DOC_MARKER)
        )
        sys.exit(1)

    table_marker = "attr-keys-ledger-table:"
    admissions = ("reserved", "general")

    rows = []
    current = None
    seen_row = False
    for line in lines[marker + 1 :]:
        if line.startswith("### "):
            break  # the section ends at the next heading
        if table_marker in line:
            current = line.split(table_marker, 1)[1].strip()
            current = current.split("-->")[0].strip()
            if current not in admissions:
                sys.stderr.write(
                    "attr-keys: %r in %s is not one of %s\n"
                    % (current, DOC, ", ".join(admissions))
                )
                sys.exit(1)
            seen_row = False
            continue
        if line.startswith("|"):
            cells = [c.strip() for c in line.strip().strip("|").split("|")]
            if not cells[0].startswith("`"):
                continue  # a header, a separator, or a foreign table
            if current is None:
                sys.stderr.write(
                    "attr-keys: a ledger row in %s appears before any %s token: %s\n"
                    % (DOC, table_marker, line)
                )
                sys.exit(1)
            if len(cells) != len(COLUMNS) - 1:
                sys.stderr.write(
                    "attr-keys: the ledger table in %s has %d columns, expected %d: %s\n"
                    % (DOC, len(cells), len(COLUMNS) - 1, line)
                )
                sys.exit(1)
            row = dict(zip(COLUMNS, cells[:3] + [current] + cells[3:]))
            row["key"] = row["key"].strip("`")
            rows.append(row)
            seen_row = True
        elif current is not None and seen_row:
            current = None  # the first non-table line closes the block
    return rows


def describe(row):
    return "/".join(row[c] for c in COLUMNS[1:])


def main(argv):
    if argv and not any(
        os.path.normpath(a) in (MIRROR_REL, DOC_REL) for a in argv
    ):
        return 0
    if not DOC.is_file():
        sys.stderr.write("attr-keys: ledger doc not found at %s\n" % DOC_REL)
        return 1

    mirror = {r["key"]: r for r in read_mirror()}
    doc = {r["key"]: r for r in read_doc()}

    problems = []
    for key in sorted(doc):
        if key not in mirror:
            problems.append("'%s' is in the doc table but not in ATTR_KEYS" % key)
        elif describe(doc[key]) != describe(mirror[key]):
            problems.append(
                "'%s': doc says '%s', ATTR_KEYS says '%s'"
                % (key, describe(doc[key]), describe(mirror[key]))
            )

    mirror_only = sorted(set(mirror) - set(doc))
    if problems:
        sys.stderr.write(
            "attr-keys: the ledger doc and ATTR_KEYS disagree (%d row(s)):\n" % len(problems)
        )
        for p in problems:
            sys.stderr.write("  %s\n" % p)
        sys.stderr.write(
            "attr-keys: %s is authoritative for the core keys; %s carries the"
            " domain rows (metadata batch pending).\n" % (DOC, MIRROR_REL)
        )
        return 1

    print(
        "attr-keys: %d doc rows agree; %d mirror-only rows (domain keys,"
        " metadata batch pending)" % (len(doc), len(mirror_only))
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
