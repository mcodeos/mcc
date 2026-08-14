#!/usr/bin/env python3
"""Migrate emission points to render messages from the registry templates.

For every emission call whose code has a unique template in
`emission-templates.json`:
  - literal messages matching the template      -> &PFX::format_msg(CODE, &[])
  - plain-positional format!(...) whose format string normalizes to the
    template                                    -> &PFX::format_msg(CODE, &[&a, &b])

Only byte-identical rewrites are performed. Emission points whose message
differs from the template, use named/spec placeholders, or whose code has no
unique template are left untouched (reported as skipped).

Usage:
  python3 scripts/migrate-emission-points.py [--apply]
"""
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, ".."))
APPLY = "--apply" in sys.argv

FNS = (
    "dlog_error|dlog_error_at|dlog_warning|dlog_warning_at|dlog_trace|"
    "dlog_info|dlog_hint|diagnostic_log|record_warning|record_error"
)
CALL_START_RE = re.compile(rf"(?<![\w:])(?P<fn>{FNS})\s*\(")

# message argument index per function
MSG_ARG = {
    "dlog_error": 2, "dlog_warning": 2, "dlog_info": 2, "dlog_hint": 2,
    "dlog_error_at": 2, "dlog_warning_at": 2, "dlog_trace": 1,
    "diagnostic_log": 4, "record_warning": 1, "record_error": 1,
}
# functions whose message parameter is `&str` (borrow the rendered String)
STR_REF = {
    "dlog_error", "dlog_warning", "dlog_info", "dlog_hint",
    "dlog_error_at", "dlog_warning_at", "dlog_trace", "diagnostic_log",
}
# record_warning / record_error take `String` (rendered without a borrow)

templates = json.load(open(os.path.join(HERE, "emission-templates.json"), encoding="utf-8"))
CODE_TMPL = {int(k): v for k, v in templates.items()}

reg = open(os.path.join(ROOT, "src", "db", "diagnostic", "errcodes.rs"), encoding="utf-8").read()
CONST_CODE = dict(re.findall(r"pub const (\w+): u32 = (\d+);", reg))


def find_call_end(text, start):
    depth = 0
    i = start
    quote = None
    while i < len(text):
        ch = text[i]
        if quote:
            if ch == "\\":
                i += 2
                continue
            if ch == quote:
                quote = None
            i += 1
            continue
        if ch in "\"'":
            quote = ch
        elif ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
            if depth == 0:
                return i
        i += 1
    return -1


def split_segments(s):
    """Split a call argument list into (start, end, text) segments at depth 0."""
    segs = []
    depth = 0
    cur = ""
    start = 0
    quote = None
    for i, ch in enumerate(s):
        if quote:
            cur += ch
            if ch == quote and not (len(cur) >= 2 and cur[-2] == "\\"):
                quote = None
            continue
        if ch in "\"'":
            quote = ch
            cur += ch
        elif ch in "([{":
            depth += 1
            cur += ch
        elif ch in ")]}":
            depth -= 1
            cur += ch
        elif ch == "," and depth == 0:
            segs.append((start, i, cur.strip()))
            cur = ""
            start = i + 1
        else:
            cur += ch
    if cur.strip():
        segs.append((start, len(s), cur.strip()))
    return segs


def fmt_str_to_template(fmt_str):
    out = []
    i = 0
    n = 0
    while i < len(fmt_str):
        c = fmt_str[i]
        if c == "{":
            if fmt_str[i + 1 : i + 2] == "{":
                out.append("{{")
                i += 2
                continue
            j = fmt_str.find("}", i + 1)
            if j < 0:
                return None
            inner = fmt_str[i + 1 : j]
            if inner == "" or inner.isdigit():
                out.append("{" + str(n) + "}")
                n += 1
            else:
                return None  # named / spec placeholders -> skip
            i = j + 1
        elif c == "}":
            if fmt_str[i + 1 : i + 2] == "}":
                out.append("}}")
                i += 2
                continue
            return None
        else:
            out.append(c)
            i += 1
    return "".join(out)


def file_prefix(text):
    for cand in ("crate::errcodes", "mcc::errcodes", "db::diagnostic::errcodes"):
        if cand in text:
            return cand
    return "crate::errcodes"


def resolve_code(code_src, text, open_paren):
    """Return (numeric code, source token) for a call's first argument."""
    m = re.search(r"(\w+)\s*$", code_src)
    if not m:
        return None, None
    token = m.group(1)
    if token.isdigit():
        return int(token), token
    if token in CONST_CODE:
        return int(CONST_CODE[token]), token
    if token == "code":
        # resolve the nearest preceding `let code = PFX::NAME;`
        m2 = re.search(
            r"let\s+code\s*=\s*(?:\w+::)*(\w+)\s*;",
            text[max(0, open_paren - 500) : open_paren],
        )
        if m2 and m2.group(1) in CONST_CODE:
            return int(CONST_CODE[m2.group(1)]), m2.group(1)
    return None, None


def rewrite_file(path):
    text = open(path, encoding="utf-8").read()
    pf = file_prefix(text)
    changed = 0
    skipped = []
    rewrites = []  # (abs_start, abs_end, new_text) — applied back-to-front

    for m in list(CALL_START_RE.finditer(text)):
        # skip function *definitions* (`fn NAME(`), not call sites
        if text[max(0, m.start() - 4) : m.start()].rstrip().endswith("fn"):
            continue
        fn_name = m.group("fn")
        open_paren = m.end() - 1
        close_paren = find_call_end(text, open_paren)
        if close_paren < 0:
            continue
        segs = split_segments(text[open_paren + 1 : close_paren])
        if len(segs) < 1:
            continue
        code, token = resolve_code(segs[0][2], text, open_paren)
        if code is None:
            skipped.append((path, fn_name, f"unresolved-code: {segs[0][2][:40]}"))
            continue
        tmpl = CODE_TMPL.get(code)
        if tmpl is None:
            skipped.append((path, fn_name, f"E{code:04d} no-template"))
            continue

        msg_arg = MSG_ARG.get(fn_name)
        if msg_arg is None or len(segs) <= msg_arg:
            continue
        msg_src = segs[msg_arg][2]

        new_msg = None
        code_arg = token if token.isdigit() else f"{pf}::{token}"
        lit = re.fullmatch(r'&?"((?:[^"\\]|\\.)*)"', msg_src, re.DOTALL)
        if lit and lit.group(1) == tmpl:
            new_msg = f"{pf}::format_msg({code_arg}, &[])"
        else:
            fm = re.fullmatch(
                r'&?format!\s*\(\s*"((?:[^"\\]|\\.)*)"(?:,(.*))?\)',
                msg_src,
                re.DOTALL,
            )
            if fm:
                norm = fmt_str_to_template(fm.group(1))
                if norm == tmpl:
                    arg_src = fm.group(2) or ""
                    args = [s[2] for s in split_segments(arg_src)]
                    if args:
                        disp = ", ".join(f"&{a} as &dyn std::fmt::Display" for a in args)
                        new_msg = f"{pf}::format_msg({code_arg}, &[{disp}])"
                    else:
                        new_msg = f"{pf}::format_msg({code_arg}, &[])"
                else:
                    skipped.append((path, fn_name, f"E{code:04d} fmt-mismatch"))
                    continue
            else:
                skipped.append((path, fn_name, f"E{code:04d} non-lit/fmt"))
                continue

        if new_msg is None:
            continue
        if fn_name in STR_REF:
            new_msg = f"&{new_msg}"
        s0, s1, _ = segs[msg_arg]
        rewrites.append((open_paren + 1 + s0, open_paren + 1 + s1, new_msg))
        changed += 1

    if not rewrites:
        return text, 0, skipped
    out = text
    for s0, s1, new_msg in sorted(rewrites, reverse=True):
        out = out[:s0] + new_msg + out[s1:]
    return out, changed, skipped


def main():
    total_changed = 0
    skip_counts = {}

    for root, _dirs, files in os.walk(os.path.join(ROOT, "src")):
        for fn in files:
            if not fn.endswith(".rs"):
                continue
            path = os.path.join(root, fn)
            out, changed, skipped = rewrite_file(path)
            if changed:
                if APPLY:
                    with open(path, "w", encoding="utf-8") as f:
                        f.write(out)
                    print(f"  {path}: {changed} rewrites")
                else:
                    print(f"  [dry] {path}: {changed} rewrites")
            total_changed += changed
            for _p, _f, why in skipped:
                skip_counts[why] = skip_counts.get(why, 0) + 1

    print(f"\ntotal rewrites: {total_changed}  ({'APPLIED' if APPLY else 'dry-run'})")
    print("skipped:")
    for k, v in sorted(skip_counts.items(), key=lambda t: -t[1]):
        print(f"    {v:4d}  {k}")


if __name__ == "__main__":
    main()
