#!/usr/bin/env python3
"""Extract every mcc test case's embedded .mc source into review files.

One .mc file per #[test] function that embeds mc source. Grouped by the test
name family (the token before the `__` in the fn name), numbered per group.
Output: build/test-cases/<family>/<NNN>__<essence>.mc plus an INDEX.md.

The Rust scanner is string/comment aware, so brace matching and string
extraction survive literals that contain braces, quotes, or //.

Usage: python3 scripts/extract-test-cases.py [--out build/test-cases]
"""

import argparse
import collections
import os
import re
import shutil
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCAN_DIRS = [os.path.join(ROOT, "src"), os.path.join(ROOT, "tests")]
SKIP_DIRS = {"target", ".git", "fixtures", "golden", "corpus", "projects"}

MC_HINT = re.compile(
    r"\b(module|component|pub|use|io|pins|pin|net|attr|role|psrc|psnk|psbi|"
    r"conduit|rail|fn|for|if|select)\b"
)
JSONISH = re.compile(r'\A\s*[\{\[]\s*"')  # JSON object/array of strings
TOMLISH = re.compile(r"\A\s*\[\s*\[")  # TOML [[table]]
TOKENISH = re.compile(r"\A[A-Za-z0-9_@./:$\\+~\-]+\Z")
# `{}` / `{name}` with no inner whitespace — mc blocks always carry
# whitespace or a newline inside the braces, so this stays placeholder-only.
FORMAT_PLACEHOLDER = re.compile(r"\{\}|\{[A-Za-z0-9_]+\}")


def scan_rust(rs):
    """Yield (kind, start, end, raw) for comments and string literals."""
    i, n = 0, len(rs)
    out = []
    while i < n:
        c = rs[i]
        prev = rs[i - 1] if i else ""
        if c == "/" and rs[i + 1 : i + 2] == "/":
            j = rs.find("\n", i)
            j = n if j < 0 else j
            out.append(("comment", i, j, None))
            i = j
        elif c == "/" and rs[i + 1 : i + 2] == "*":
            j = rs.find("*/", i + 2)
            j = n if j < 0 else j + 2
            out.append(("comment", i, j, None))
            i = j
        elif (
            c == "r"
            and rs[i + 1 : i + 2] in ('"', "#")
            and not (prev.isalnum() or prev == "_")
        ):
            j = i + 1
            hashes = 0
            while rs[j] == "#":
                hashes += 1
                j += 1
            term = '"' + "#" * hashes
            k = rs.find(term, j + 1)
            if k < 0:
                i = j
                continue
            k += len(term)
            out.append(("rawstr", i, k, rs[i:k]))
            i = k
        elif c == '"':
            j = i + 1
            while j < n:
                if rs[j] == "\\":
                    j += 2
                    continue
                if rs[j] == '"':
                    break
                j += 1
            out.append(("str", i, j + 1, rs[i : j + 1]))
            i = j + 1
        elif c == "'":
            m = re.match(r"'(\\.|[^\\'\n])'", rs[i : i + 12])
            if m:
                out.append(("char", i, i + m.end(), None))
                i += m.end()
            else:
                i += 1
        else:
            i += 1
    return out


def mask(rs, marks):
    """Blank out every marked region, keeping newlines so offsets hold."""
    buf = list(rs)
    for _kind, s, e, _raw in marks:
        for k in range(s, e):
            if buf[k] != "\n":
                buf[k] = " "
    return "".join(buf)


def decode_str(raw):
    """Decode a normal Rust string literal body (without quotes)."""
    body = raw[1:-1]
    out = []
    i, n = 0, len(body)
    while i < n:
        c = body[i]
        if c != "\\":
            out.append(c)
            i += 1
            continue
        nxt = body[i + 1]
        if nxt == "n":
            out.append("\n")
        elif nxt == "t":
            out.append("\t")
        elif nxt == "r":
            out.append("\r")
        elif nxt == "\\":
            out.append("\\")
        elif nxt == '"':
            out.append('"')
        elif nxt == "'":
            out.append("'")
        elif nxt == "0":
            out.append("\0")
        elif nxt == "u":
            j = body.index("}", i)
            out.append(chr(int(body[i + 3 : j], 16)))
        elif nxt == "x":
            out.append(chr(int(body[i + 2 : i + 4], 16)))
        else:
            out.append(nxt)
        i += 2 if nxt != "u" else 0
        if nxt == "u":
            i = j + 1
    return "".join(out)


def literal_value(kind, raw):
    if kind == "rawstr":
        m = re.match(r'r(#+)"(.*)"\1\Z', raw, re.S)
        return m.group(2) if m else None
    if kind == "str":
        try:
            return decode_str(raw)
        except Exception:
            return None
    return None


MC_LEAD = re.compile(r"\A\s*(module|component|pub|use|io|net|pin|pins|attr|fn)\b")


def is_mc_fragment(text):
    if text.strip() == "":
        return False
    if JSONISH.match(text) or TOMLISH.match(text):
        return False
    if "\n" in text:
        return "{" in text and bool(MC_HINT.search(text))
    # Single-line: accept only a leading mc keyword, so English sentences
    # that merely contain "use" or "if" stay out; dotted names are file
    # names/URIs, not source.
    if re.match(r"\A\s*\w+(\.\w+)+\s*\Z", text):
        return False
    return bool(MC_LEAD.match(text))


def find_tests(rs):
    """Return [(fn_name, body_start, body_end)] using the masked source."""
    marks = scan_rust(rs)
    masked = mask(rs, marks)
    tests = []
    for m in re.finditer(r"#\s*\[\s*test\s*(\(|\])", masked):
        fm = re.compile(r"\bfn\s+([A-Za-z0-9_]+)").search(masked, m.end())
        if not fm:
            continue
        ob = masked.index("{", fm.end())
        depth = 0
        k = ob
        while k < len(masked):
            if masked[k] == "{":
                depth += 1
            elif masked[k] == "}":
                depth -= 1
                if depth == 0:
                    break
            k += 1
        tests.append((fm.group(1), ob + 1, k))
    return tests, marks


def const_fragments(rs, marks, strmarks):
    """Map each file-level `const/static NAME` to its mc-looking literals.

    The item spans from the declaration to the next `const`/`static`/`fn`
    keyword in the masked text — good enough for fixture blocks.
    """
    masked = mask(rs, marks)
    bounds = sorted(
        [mm.start() for mm in re.finditer(r"\b(?:const|static|fn)\s", masked)]
    )
    out = {}
    for cm in re.finditer(r"\b(?:static|const)\s+([A-Z_0-9]+)\s*:", masked):
        name = cm.group(1)
        nxt = [b for b in bounds if b > cm.end()]
        end = nxt[0] if nxt else len(masked)
        frags = []
        for kind, a, b, raw in strmarks:
            if not (cm.end() <= a and b <= end):
                continue
            val = literal_value(kind, raw)
            if val is None or not is_mc_fragment(val):
                continue
            frags.append(val)
        if frags:
            out[name] = frags
    return out


def collect():
    cases = []  # dicts: family, essence, frags, fixtures, assembled, path, line
    for base in SCAN_DIRS:
        for dirpath, dirnames, filenames in os.walk(base):
            dirnames[:] = [
                d for d in dirnames if d not in SKIP_DIRS and not d.startswith(".")
            ]
            for fn in sorted(filenames):
                if not fn.endswith(".rs"):
                    continue
                path = os.path.join(dirpath, fn)
                with open(path, encoding="utf-8") as f:
                    rs = f.read()
                try:
                    tests, marks = find_tests(rs)
                except Exception as e:  # unparsable file: skip loudly
                    print(f"skip {path}: {e}", file=sys.stderr)
                    continue
                strmarks = [m for m in marks if m[0] in ("str", "rawstr")]
                consts = const_fragments(rs, marks, strmarks)
                masked = mask(rs, marks)
                for name, s, e in tests:
                    frags = []
                    for kind, a, b, raw in strmarks:
                        if a < s or b > e:
                            continue
                        val = literal_value(kind, raw)
                        if val is None or not is_mc_fragment(val):
                            continue
                        frags.append(val)
                    body = masked[s:e]
                    fixtures = []
                    for cname, cfrags in consts.items():
                        if re.search(r"\b%s\b" % cname, body):
                            fixtures.extend(cfrags)
                    if not frags and not fixtures:
                        continue
                    line = rs.count("\n", 0, s) + 1
                    rel = os.path.relpath(path, ROOT)
                    parts = name.split("__", 1)
                    stem = os.path.splitext(fn)[0]
                    family = parts[0] if len(parts) == 2 and parts[0] else stem
                    essence = (
                        re.sub(r"[^A-Za-z0-9_]", "_", parts[1]).strip("_")[:80]
                        if len(parts) == 2
                        else name[:80]
                    )
                    assembled = any(FORMAT_PLACEHOLDER.search(f) for f in frags)
                    cases.append(
                        dict(
                            family=family.lower(),
                            essence=essence or "case",
                            frags=frags,
                            fixtures=fixtures,
                            assembled=assembled,
                            path=rel,
                            line=line,
                            name=name,
                        )
                    )
    return cases


def write_out(cases, outdir):
    if os.path.isdir(outdir):
        shutil.rmtree(outdir)  # no stale files from a previous filter set
    os.makedirs(outdir, exist_ok=True)
    by_family = collections.defaultdict(list)
    for c in cases:
        by_family[c["family"]].append(c)
    index = ["# mcc test-case extraction — for case code review", ""]
    index.append(
        f"{len(cases)} test cases with embedded .mc source, "
        f"in {len(by_family)} family groups. One file per #[test] fn; "
        "fragments are the fn's string literals in source order."
    )
    index.append("")
    for family in sorted(by_family):
        famdir = os.path.join(outdir, family)
        os.makedirs(famdir, exist_ok=True)
        by_family[family].sort(key=lambda c: (c["path"], c["line"]))
        index.append(f"## {family}/ ({len(by_family[family])} cases)")
        index.append("")
        for idx, c in enumerate(by_family[family], 1):
            fname = f"{idx:03d}__{c['essence']}.mc"
            header = [
                f"// Test case: {c['name']}",
                f"// Source: {c['path']}:{c['line']}",
            ]
            if c["assembled"]:
                header.append(
                    "// NOTE: this fn assembles source with format!(); "
                    "{placeholders} below were interpolations at runtime."
                )
            body = []
            pieces = [(f"file-level fixture {i}", f) for i, f in
                      enumerate(c["fixtures"], 1)]
            pieces += [(f"fragment {i}", f) for i, f in enumerate(c["frags"], 1)]
            if len(pieces) == 1:
                body.append(pieces[0][1].rstrip("\n"))
            else:
                for k, (label, frag) in enumerate(pieces, 1):
                    body.append(f"// ---- {label} ({k}/{len(pieces)}) ----")
                    body.append(frag.rstrip("\n"))
            with open(
                os.path.join(famdir, fname), "w", encoding="utf-8"
            ) as f:
                f.write("\n".join(header) + "\n\n" + "\n".join(body) + "\n")
            src = (
                f"{c['path']}:{c['line']}"
                + (" (assembled)" if c["assembled"] else "")
            )
            index.append(f"- `{family}/{fname}` ← {src}")
        index.append("")
    with open(os.path.join(outdir, "INDEX.md"), "w", encoding="utf-8") as f:
        f.write("\n".join(index) + "\n")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=os.path.join(ROOT, "build", "test-cases"))
    args = ap.parse_args()
    cases = collect()
    write_out(cases, args.out)
    total_frags = sum(len(c["frags"]) for c in cases)
    assembled = sum(1 for c in cases if c["assembled"])
    print(
        f"{len(cases)} cases -> {args.out} "
        f"({total_frags} fragments, {assembled} with format! placeholders)"
    )


if __name__ == "__main__":
    main()
