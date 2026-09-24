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
STMT_KW = re.compile(
    r"^(in|out|io|net|pin|pins|psrc|psnk|psbi|block|attr|use|for|if|else|"
    r"return|fn|let|pub|module|component|interface|rail|conduit|enum)\b"
)
STMT_LINE = re.compile(
    r"^[A-Za-z_][\w.]*\s*(->|[.]\w|\[|=\s)"  # stmt head: ident .field / -> / [ / =
)
CONSTRUCTION = re.compile(r"^[A-Z0-9_]{2,}\s+[a-z_]")  # CHIP d1


def stmt_like(line):
    t = line.strip()
    if not t or t.startswith("//") or t.startswith("@") or t in ("{", "}"):
        return True
    return bool(
        STMT_LINE.match(t)
        or STMT_KW.match(t)
        or CONSTRUCTION.match(t)
        or "=" in t
        or "->" in t
    )
BARE_KW = re.compile(r"\A\s*(module|component|pub|use|io|net|pin|attr|fn)\s*\Z")
CALL_OPEN = re.compile(r"\b([A-Za-z_][A-Za-z0-9_:]*)!?\s*\(")
LET_RE = re.compile(
    r"\blet\s+(?:mut\s+)?([A-Za-z_][A-Za-z0-9_]*)[^;{=]*=(.*?);", re.S
)
CONST_REF = re.compile(r"\b[A-Z][A-Z0-9_]{2,}\b")
ASSERT_NAME = re.compile(r"^(assert|expect|panic|debug_assert)")


def is_assert_name(name):
    base = name.split("::")[-1]
    if ASSERT_NAME.match(base):
        return True
    return base in {
        "only", "count", "count_of", "reports", "wires",
        "contains", "eq", "ne", "has",
    }


def find_call_spans(masked, s, e):
    """(name, args_start, args_end) for every call in [s,e), balanced-paren."""
    out = []
    for cm in CALL_OPEN.finditer(masked, s, e):
        depth = 0
        k = cm.end() - 1
        while k < e:
            c = masked[k]
            if c == "(":
                depth += 1
            elif c == ")":
                depth -= 1
                if depth == 0:
                    break
            k += 1
        out.append((cm.group(1), cm.end(), k))
    return out


PLACEHOLDER = re.compile(r"\{([A-Za-z_][A-Za-z0-9_]*)\}")
UNRESOLVED_PH = re.compile(r"\{\}|\{[a-z_][a-z0-9_]*\}")


def source_spans(masked, s, e):
    """Union of spans where compiler-bound source lives.

    Seeds: argument spans of every non-assert call (a test feeds the compiler
    through helpers like build()/load_string()/push_str()). Then `let` RHS
    spans join transitively while the bound name shows up inside a live span —
    that is how `let body = "..."; build(CHIP, body)` marks its literal.
    Literals inside assert!/expect! messages never join, so expectation text
    stays out.
    """
    calls = find_call_spans(masked, s, e)
    spans = [(a, b) for name, a, b in calls if not is_assert_name(name)]
    lets = [
        (mm.group(1), mm.start(2), mm.end(2))
        for mm in LET_RE.finditer(masked, s, e)
    ]
    for _ in range(8):
        joined = "\n".join(masked[a:b] for a, b in spans)
        live = set(re.findall(r"[A-Za-z_][A-Za-z0-9_]*", joined))
        changed = False
        for name, a, b in lets:
            if name in live and (a, b) not in spans:
                spans.append((a, b))
                changed = True
        if not changed:
            break
    return spans


def is_mc_fragment(text):
    if text.strip() == "":
        return False
    if JSONISH.match(text) or TOMLISH.match(text):
        return False
    if RUST_FMT_SPEC.search(text):
        return False  # an assert!/expect! message, not source
    if BARE_KW.match(text):
        return False  # a bare keyword token (registry word lists), not source
    if "\n" in text:
        if "{" in text:
            return bool(MC_HINT.search(text))
        # A module *body* has no braces of its own (the shell supplies them);
        # accept it when its lines look like statements, not prose.
        lines = [ln for ln in text.split("\n") if ln.strip()]
        return len(lines) >= 2 and sum(stmt_like(ln) for ln in lines) >= 0.8 * len(
            lines
        )
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


def doc_purpose(rs, line):
    """The `///` doc comment block directly above the #[test], if any."""
    raw = rs.split("\n")
    i = line - 2  # 0-indexed line before the fn
    while i >= 0 and raw[i].strip().startswith("#["):
        i -= 1
    doc = []
    while i >= 0 and raw[i].strip().startswith("///"):
        doc.append(raw[i].strip()[3:].strip())
        i -= 1
    return " ".join(reversed(doc))[:220]


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
                    spans = source_spans(masked, s, e)
                    live_text = "".join(masked[a:b] for a, b in spans)
                    frags = []
                    live_vals = []  # decoded text of live literals: inline
                    # format! captures like "{RES2}" live only here, since
                    # live_text comes from the masked (blanked) text
                    for kind, a, b, raw in strmarks:
                        if a < s or b > e:
                            continue
                        if not any(pa <= a and b <= pb for pa, pb in spans):
                            continue
                        val = literal_value(kind, raw)
                        if val is None:
                            continue
                        live_vals.append(val)
                        if not is_mc_fragment(val):
                            continue
                        frags.append(val)
                    live_src = "\n".join(live_vals)
                    fixtures = []
                    const_map = {}
                    for cname, cfrags in consts.items():
                        if re.search(
                            r"\b%s\b" % cname, live_text
                        ) or re.search(r"\b%s\b" % cname, live_src):
                            fixtures.extend(cfrags)
                            const_map[cname] = "\n".join(cfrags)
                    # let-bound pure-string literals, for {var} placeholders
                    varmap = {}
                    for mm in LET_RE.finditer(masked, s, e):
                        rhs = (mm.start(2), mm.end(2))
                        inner = [
                            literal_value(k, raw)
                            for k, a, b, raw in strmarks
                            if rhs[0] <= a and b <= rhs[1]
                        ]
                        inner = [v for v in inner if v is not None]
                        if len(inner) == 1:
                            varmap[mm.group(1)] = inner[0]
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
                    purpose = doc_purpose(rs, line) or (essence or "case").replace(
                        "_", " "
                    )
                    cases.append(
                        dict(
                            family=family.lower(),
                            essence=essence or "case",
                            frags=frags,
                            fixtures=fixtures,
                            const_map=const_map,
                            varmap=varmap,
                            purpose=purpose,
                            path=rel,
                            line=line,
                            name=name,
                        )
                    )
    return cases


RUST_FMT_SPEC = re.compile(r"\{:[^{}]*\}|\{[A-Za-z_][A-Za-z0-9_]*:\??\}")


def split_docs(frags):
    """Merge fragments into balanced-brace documents.

    Tests that build one source via push_str have its head literal end with
    open braces; keep appending until the balance returns to zero. A test
    that compiles several alternative sources yields several closed docs.
    """
    docs, buf, bal = [], [], 0
    for f in frags:
        buf.append(f)
        bal += f.count("{") - f.count("}")
        if bal <= 0:
            docs.append("\n".join(buf))
            buf, bal = [], 0
    if buf:
        docs.append("\n".join(buf))
    return docs


def dedupe_overlap(pieces):
    """Drop pieces embedded in a longer sibling.

    Tests assemble sources as `format!("{}{}", PREFIX, BODY)` where PREFIX is
    a file-level const; both ends then show up in the fragment list and a
    naive concatenation doubles the content.
    """
    norm = [re.sub(r"\s+", " ", p).strip() for p in pieces]
    kept = []
    for i, p in enumerate(pieces):
        dup = False
        for j, q in enumerate(norm):
            if j == i or not norm[i]:
                continue
            longer = len(q) > len(norm[i]) or (len(q) == len(norm[i]) and j < i)
            if longer and norm[i] in q:
                dup = True
                break
        if not dup:
            kept.append(p)
    return kept


def dedupe_variants(fixtures):
    """Split file-level fixtures into (kept, variant_alternatives).

    A test fn often loops over several similar const fixtures (variant
    families). Those are alternative sources, not parts of one document:
    near-identical fixtures (>0.85 ratio, non-trivial size) collapse to the
    longest one, the rest come back as commented alternatives.
    """
    import difflib

    norm = [re.sub(r"\s+", " ", f).strip() for f in fixtures]
    dropped = set()
    for i in range(len(fixtures)):
        if i in dropped:
            continue
        for j in range(len(fixtures)):
            if j <= i or j in dropped:
                continue
            if min(len(norm[i]), len(norm[j])) <= 40:
                continue
            if difflib.SequenceMatcher(None, norm[i], norm[j]).ratio() > 0.85:
                drop = j if len(norm[j]) <= len(norm[i]) else i
                dropped.add(drop)
                if drop == i:
                    break
    kept = [f for k, f in enumerate(fixtures) if k not in dropped]
    alts = [f for k, f in enumerate(fixtures) if k in dropped]
    return kept, alts


def compose(piece, const_map, varmap, used, depth=0):
    """Substitute `{NAME}` placeholders from consts / let-bound literals.

    Runs on the raw format! text; `{{`/`}}` unescaping happens afterwards.
    Names resolved here are recorded in `used` so their standalone copies can
    be dropped from the piece list.
    """
    if depth > 5:
        return piece

    def repl(m):
        nm = m.group(1)
        if nm in const_map:
            used.add(nm)
            return compose(const_map[nm], const_map, varmap, used, depth + 1)
        if nm in varmap:
            used.add(nm)
            return compose(varmap[nm], const_map, varmap, used, depth + 1)
        return m.group(0)

    return PLACEHOLDER.sub(repl, piece)


def build_document(c):
    """Return (active_docs, inactive_pairs, unresolved).

    Layout: top-level definition docs (component/use libs) first, then the
    first module doc as the active source; later module docs and leftover
    body fragments become commented-out inactive alternatives. A case with
    no module doc gets its body wrapped in a `module main` shell.
    """
    const_map, varmap = c["const_map"], c["varmap"]
    used = set()
    pieces = [compose(p, const_map, varmap, used) for p in c["fixtures"] + c["frags"]]
    # a var/const consumed by a template must not also stand alone
    used_contents = set()
    for n in used:
        src_txt = const_map.get(n) or varmap.get(n)
        if src_txt is not None:
            used_contents.add(compose(src_txt, const_map, varmap, set()).strip())
    pieces = [p for p in pieces if p.strip() not in used_contents]
    pieces = dedupe_overlap(pieces)
    pieces, variant_alts = dedupe_variants(pieces)
    frags = [f.replace("{{", "{").replace("}}", "}") for f in pieces]
    docs = split_docs(frags)
    has_module = lambda d: re.search(r"^\s*module\s+\w", d, re.M) is not None
    TOP_DECL = re.compile(
        r"^\s*(module|component|interface|pub|use|attr|rail|conduit|enum|fn)\b"
    )
    is_top = lambda d: bool(TOP_DECL.match(d))
    mains = [d for d in docs if has_module(d)]
    libs = [d for d in docs if not has_module(d) and is_top(d)]
    bodies = [d for d in docs if not has_module(d) and not is_top(d)]
    active, inactive = [], []
    if mains:
        active.extend(libs)
        active.append(mains[0])
        for k, d in enumerate(mains[1:], 1):
            inactive.append((f"inactive alternative {k}", d))
        for k, d in enumerate(bodies, 1):
            inactive.append((f"inactive body fragment {k}", d))
    elif bodies:
        active.extend(libs)
        shell = "\n".join(bodies).rstrip("\n")
        active.append("module main {\n" + shell + "\n}")
    else:
        active.extend(libs)
    for k, d in enumerate(variant_alts, 1):
        inactive.append((f"inactive variant {k}", d))
    unresolved = any(UNRESOLVED_PH.search(d) for d in active)
    return active, inactive, unresolved


STAGE_DIR = "_stage"
CLASSES = ("valid", "invalid", "template")
HEADER_TOOL = (
    "// Generated by scripts/extract-test-cases.py from the mcc test suite.\n"
    "// Do not edit by hand: regenerate instead.\n"
)


def file_body(c):
    """The mc text of a case: active docs, then inactive alternatives."""
    active, inactive, unresolved = c.setdefault(
        "doc", build_document(c)
    )
    parts = ["\n".join(active).strip("\n")]
    for label, d in inactive:
        block = "\n".join("// " + ln if ln else "//" for ln in d.rstrip().split("\n"))
        parts.append(f"// ---- {label} ----\n{block}")
    return "\n\n".join(parts) + "\n"


def run_mcc(path, mcc):
    """(errors, codes) for one file via `mcc parse -q`."""
    import subprocess

    try:
        p = subprocess.run(
            [mcc, "parse", "-q", path],
            capture_output=True, text=True, timeout=60,
        )
    except Exception as e:
        return -1, [f"spawn-failed:{e}"]
    text = (p.stdout or "") + (p.stderr or "")
    m = re.search(r"summary: errors=(\d+)", text)
    errors = int(m.group(1)) if m else -1
    codes = sorted(set(re.findall(r"[EW]\d{4}", text)))
    return errors, codes


def write_out(cases, outdir, mcc="mcc", workers=8):
    """Two-phase pipeline: stage everything, parse, then finalize by class.

    valid   -- mcc parse reports 0 errors
    invalid -- mcc parse reports >0 errors (the negative / rejection cases)
    template-- format!() runtime interpolation that never resolves to static
               text; the placeholders below were values at runtime
    """
    from concurrent.futures import ThreadPoolExecutor

    stage = os.path.join(outdir, STAGE_DIR)
    if os.path.isdir(stage):
        shutil.rmtree(stage)
    fam_seq = collections.Counter()
    for c in cases:
        fam_seq[c["family"]] += 1
        c["num"] = fam_seq[c["family"]]
        fam = os.path.join(stage, c["family"])
        os.makedirs(fam, exist_ok=True)
        c["stage"] = os.path.join(fam, "%03d__%s.mc" % (c["num"], c["essence"]))
        with open(c["stage"], "w", encoding="utf-8") as f:
            f.write(file_body(c))

    with ThreadPoolExecutor(workers) as ex:
        results = list(ex.map(lambda c: run_mcc(c["stage"], mcc), cases))
    for c, (errors, codes) in zip(cases, results):
        c["errors"], c["codes"] = errors, codes

    # regenerate the tree from scratch: the class split renames everything
    if os.path.isdir(outdir):
        shutil.rmtree(outdir)
    index = {k: [] for k in CLASSES}
    counters = collections.Counter()
    for c in cases:
        active, inactive, unresolved = c["doc"]
        if unresolved:
            klass, why = "template", "format! runtime interpolation unresolved"
        elif c["errors"] > 0:
            klass = "invalid"
            why = "mcc parse: %d error(s): %s" % (c["errors"], ", ".join(c["codes"]))
        elif c["errors"] == 0:
            klass, why = "valid", "mcc parse: 0 errors"
        else:
            klass, why = "invalid", "mcc parse failed to run: %s" % ", ".join(c["codes"])
        counters[klass] += 1
        counters[klass, c["family"]] += 1
        n = counters[klass, c["family"]]
        rel = os.path.join(klass, c["family"], "%03d__%s.mc" % (n, c["essence"]))
        dest = os.path.join(outdir, rel)
        os.makedirs(os.path.dirname(dest), exist_ok=True)
        ph = "\n".join(
            sorted(set(UNRESOLVED_PH.findall("\n".join(active))))
        )
        head = [
            f"// Case file: {rel}",
            f"// Purpose: {c['purpose']}",
            f"// Class: {klass} ({why})",
        ]
        if klass == "template" and ph:
            head.append(f"// Unresolved placeholders: {ph}")
        if klass == "invalid" and c["codes"]:
            head.append("// Fired diagnostics: " + ", ".join(c["codes"]))
        head += [
            f"// Source: {c['path']}:{c['line']}  fn {c['name']}",
            HEADER_TOOL.rstrip("\n"),
        ]
        with open(dest, "w", encoding="utf-8") as f:
            f.write("\n".join(head) + "\n\n" + file_body(c))
        index[klass].append((rel, c))

    lines = [
        "# mcc test-case extraction",
        "",
        "One `.mc` file per `#[test]` that embeds mc source, extracted by",
        "`scripts/extract-test-cases.py`. Do not edit: regenerate instead.",
        "",
        "| Class | Meaning | Count |",
        "|---|---|---|",
        "| valid/ | mcc parse: 0 errors | %d |" % len(index["valid"]),
        "| invalid/ | mcc parse reports errors (rejection cases) | %d |" % len(index["invalid"]),
        "| template/ | format! runtime interpolation, no static text | %d |" % len(index["template"]),
        "",
    ]
    for klass in CLASSES:
        lines += [f"## {klass}/", ""]
        for rel, c in sorted(index[klass]):
            lines.append(f"- [{rel}]({rel}) — {c['purpose']} (`{c['path']}:{c['line']}`)")
        lines.append("")
    with open(os.path.join(outdir, "INDEX.md"), "w", encoding="utf-8") as f:
        f.write("\n".join(lines))
    return counters


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--out", default=os.path.join(ROOT, "build", "test-cases"))
    ap.add_argument("--mcc", default="mcc", help="mcc binary used to classify")
    ap.add_argument("--workers", type=int, default=8)
    args = ap.parse_args()
    cases = collect()
    print(f"collected {len(cases)} cases", file=sys.stderr)
    counters = write_out(cases, args.out, mcc=args.mcc, workers=args.workers)
    for klass in CLASSES:
        print(f"{klass}: {counters[klass]}", file=sys.stderr)
    print(f"index: {os.path.join(args.out, 'INDEX.md')}", file=sys.stderr)


if __name__ == "__main__":
    main()
