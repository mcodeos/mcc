#!/usr/bin/env python3
"""Baseline scan: extract every hardcoded diagnostic code emitted by mcc.

Sweeps `src/**/*.rs` for:
  1. dlog_error / dlog_warning / dlog_trace / dlog_error_at / dlog_warning_at
     numeric code literals, plus the message literal that follows.
  2. `code: <N>,` inside CheckResult / NetCheckResult / PinCheckResult
     initializers (PostParse validation), plus the check_name / message.

Also parses `src/ast/c/astdef.h` for the C-parser MCD_E*/MCD_W* code macros.

Outputs `scripts/error-code-inventory.json` — the authoritative "before" list.

Usage:
    python3 scripts/scan-error-codes.py [src_dir]
"""

import json
import os
import re
import sys
from collections import defaultdict

SRC = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(__file__), "..", "src")
OUT = os.path.join(os.path.dirname(__file__), "error-code-inventory.json")

DLOG_RE = re.compile(
    r"dlog_(?:error|warning|trace|error_at|warning_at)\(\s*(\d+)\s*,", re.DOTALL
)
CODE_FIELD_RE = re.compile(r"code\"?\s*:\s*(\d+)\s*,")
MSG_LITERAL_RE = re.compile(r"\"([^\"]*)\"")
CHECK_NAME_RE = re.compile(r"check_name:\s*\"([^\"]*)\"")
ASTDEF_RE = re.compile(r"#define\s+(MCD_[EW]\d+\w*)\s+(\d+)")

# Files that are generated / vendored C sources, excluded from the .rs sweep.
EXCLUDE_DIRS = {"ast/c"}


def rs_files():
    for root, dirs, files in os.walk(SRC):
        dirs[:] = [d for d in dirs if d not in EXCLUDE_DIRS]
        for f in files:
            if f.endswith(".rs"):
                yield os.path.join(root, f)


def emit_sites():
    """code -> list of {file, line, kind, msg}"""
    sites = defaultdict(list)
    for path in sorted(rs_files()):
        rel = os.path.relpath(path, os.path.dirname(OUT))
        with open(path, encoding="utf-8", errors="replace") as fh:
            text = fh.read()
        lines = text.splitlines()
        for m in DLOG_RE.finditer(text):
            lineno = text.count("\n", 0, m.start()) + 1
            if lines[lineno - 1].lstrip().startswith("//"):
                continue
            code = int(m.group(1))
            if 32100 <= code <= 32999:
                continue  # RPC application codes (out of diagnostic scope)
            window = text[m.start() : m.start() + 300]
            mm = MSG_LITERAL_RE.search(window)
            msg = mm.group(1) if mm else ""
            sites[code].append(
                {"file": rel, "line": lineno, "kind": "dlog", "msg": msg}
            )
        for code, occ in code_field_sites(path, rel, lines).items():
            sites[code].extend(occ)
    return sites


def code_field_sites(path, rel, lines):
    """For `code: N,` lines, resolve the enclosing check's message from nearby lines."""
    sites = {}
    for lineno, line in enumerate(lines, 1):
        m = CODE_FIELD_RE.search(line)
        if not m:
            continue
        code = int(m.group(1))
        if 32100 <= code <= 32999:
            continue  # RPC application codes (out of diagnostic scope)
        # look back up to 12 lines for the first string literal (message)
        msg = ""
        for j in range(lineno - 1, max(lineno - 13, -1), -1):
            if lines[j].lstrip().startswith("//"):
                continue
            mm = MSG_LITERAL_RE.search(lines[j])
            if mm:
                msg = mm.group(1)
                break
        sites.setdefault(code, []).append(
            {"file": rel, "line": lineno, "kind": "check", "msg": msg}
        )
    return sites


def astdef_codes():
    """Parse MCD_E*/MCD_W* defines from the C parser header."""
    out = {}
    hdr = os.path.join(SRC, "ast", "c", "astdef.h")
    if not os.path.exists(hdr):
        return out
    with open(hdr, encoding="utf-8") as fh:
        for m in ASTDEF_RE.finditer(fh.read()):
            out[int(m.group(2))] = m.group(1)
    return out


def main():
    sites = emit_sites()
    astdef = astdef_codes()

    inventory = []
    for code in sorted(sites):
        occ = sites[code]
        msgs = sorted({o["msg"] for o in occ if o["msg"]})
        kinds = sorted({o["kind"] for o in occ})
        files = sorted({o["file"] for o in occ})
        inventory.append(
            {
                "code": code,
                "macro": astdef.get(code),
                "count": len(occ),
                "kinds": kinds,
                "files": files,
                "messages": msgs[:8],
            }
        )
    # C-parser codes declared in astdef.h but never seen in .rs
    seen = set(sites)
    for code, macro in sorted(astdef.items()):
        if code in seen:
            continue
        inventory.append(
            {
                "code": code,
                "macro": macro,
                "count": 0,
                "kinds": ["c-parser"],
                "files": ["src/ast/c/astdef.h"],
                "messages": [],
            }
        )
    inventory.sort(key=lambda e: e["code"])

    with open(OUT, "w", encoding="utf-8") as fh:
        json.dump(
            {
                "generated": "scripts/scan-error-codes.py",
                "total_codes": len(inventory),
                "total_sites": sum(i["count"] for i in inventory),
                "codes": inventory,
            },
            fh,
            ensure_ascii=False,
            indent=2,
        )

    print(f"codes: {len(inventory)}  sites: {sum(i['count'] for i in inventory)}")
    for i in inventory:
        print(
            f"  {i['code']:>5}  x{i['count']:<3} {i['macro'] or '':<24} "
            f"{','.join(i['files'][:2])}  {i['messages'][0][:60] if i['messages'] else ''}"
        )


if __name__ == "__main__":
    main()
