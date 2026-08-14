#!/usr/bin/env python3
"""Extract canonical emission-message templates from emission points.

Scans every emission call (dlog_*, diagnostic_log, record_warning/error),
derives a message template per code:
  - literal   "..."                 -> the literal itself
  - format!( "...", ..)              -> format string with placeholders
                                       normalized to {0}, {1}, …
  - anything else (constants, &msg)  -> skipped (left without a template)

A code is only assigned a template when ALL its emission points agree on one
message shape+text; otherwise it is reported as a conflict and skipped.

Writes scripts/emission-templates.json: { "new_code": "template" }.
"""
import json
import os
import re

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "emission-templates.json")

FNS = (
    "dlog_error|dlog_error_at|dlog_warning|dlog_warning_at|dlog_trace|"
    "dlog_info|dlog_hint|diagnostic_log|record_warning|record_error"
)
CALL_START_RE = re.compile(rf"(?<![\w:])(?P<fn>{FNS})\s*\(")
MSG_ARG = {
    "dlog_error": 2, "dlog_warning": 2, "dlog_info": 2, "dlog_hint": 2,
    "dlog_error_at": 2, "dlog_warning_at": 2, "dlog_trace": 1,
    "diagnostic_log": 4, "record_warning": 1, "record_error": 1,
}

reg = open(os.path.join(HERE, "..", "src", "db", "diagnostic", "errcodes.rs"), encoding="utf-8").read()
CONST_TO_CODE = dict(re.findall(r"pub const (\w+): u32 = (\d+);", reg))


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


def split_top_level(s):
    parts, depth, cur, quote = [], 0, "", None
    for ch in s:
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
            parts.append(cur.strip())
            cur = ""
        else:
            cur += ch
    if cur.strip():
        parts.append(cur.strip())
    return parts


def unescape_rust_str(raw):
    """Process Rust string-literal escapes on the raw source text between quotes.

    Rust renders the escaped forms (\\n, \\t, \\\\, \\", \\xNN, \\u{...}) and
    treats a backslash directly followed by a newline as a line continuation
    (the newline and all leading whitespace of the next line are dropped).
    Braces (`{0}`, `{{`, `}}`) are formatter markers and are left untouched.
    """
    out = []
    i = 0
    n = len(raw)
    while i < n:
        c = raw[i]
        if c == "\\" and i + 1 < n:
            nxt = raw[i + 1]
            if nxt == "\\":
                out.append("\\")
                i += 2
                continue
            if nxt == '"':
                out.append('"')
                i += 2
                continue
            if nxt == "'":
                out.append("'")
                i += 2
                continue
            if nxt == "n":
                out.append("\n")
                i += 2
                continue
            if nxt == "t":
                out.append("\t")
                i += 2
                continue
            if nxt == "r":
                out.append("\r")
                i += 2
                continue
            if nxt == "0":
                out.append("\0")
                i += 2
                continue
            if nxt == "x" and i + 3 < n and re.match(r"[0-9a-fA-F]{2}", raw[i + 2 : i + 4]):
                out.append(chr(int(raw[i + 2 : i + 4], 16)))
                i += 4
                continue
            if nxt == "u" and raw[i + 2 : i + 3] == "{":
                end = raw.find("}", i + 3)
                if end > 0:
                    try:
                        out.append(chr(int(raw[i + 3 : end], 16)))
                    except ValueError:
                        out.append(raw[i : end + 1])
                    i = end + 1
                    continue
            if nxt == "\n":
                # line continuation: drop backslash, newline and next-line whitespace
                j = i + 2
                while j < n and raw[j] in " \t":
                    j += 1
                i = j
                continue
            out.append(c)  # unknown escape — keep verbatim
        out.append(c)
        i += 1
    return "".join(out)


def fmt_to_template(fmt_str):
    """Normalize a Rust format string to a {0},{1},… template."""
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
                # plain positional {} / {0} — safe to re-render via Display
                out.append("{" + str(n) + "}")
                n += 1
            else:
                # format specs ({:?}, {:.2}) or named ({name}) placeholders
                # cannot be reproduced byte-for-byte via Display — skip template
                return None
            i = j + 1
        elif c == "}":
            if fmt_str[i + 1 : i + 2] == "}":
                out.append("}}")
                i += 2
                continue
            return None  # unbalanced
        else:
            out.append(c)
            i += 1
    return "".join(out)


def extract_template(msg_src):
    msg = msg_src.strip()
    # literal: "..." or &"..."
    m = re.match(r"&?\"((?:[^\"\\]|\\.)*)\"$", msg, re.DOTALL)
    if m:
        return unescape_rust_str(m.group(1))
    # format!( "...", ... )
    m = re.match(r"&?format!\s*\(\s*\"((?:[^\"\\]|\\.)*)\"", msg, re.DOTALL)
    if m:
        return fmt_to_template(unescape_rust_str(m.group(1)))
    return None


def main():
    per_code = {}
    conflicts = {}
    skipped = []

    for root, _dirs, files in os.walk(os.path.join(HERE, "..", "src")):
        for fn in files:
            if not fn.endswith(".rs"):
                continue
            path = os.path.join(root, fn)
            text = open(path, encoding="utf-8").read()
            for m in CALL_START_RE.finditer(text):
                fn_name = m.group("fn")
                open_paren = m.end() - 1
                close_paren = find_call_end(text, open_paren)
                if close_paren < 0:
                    continue
                parts = split_top_level(text[open_paren + 1 : close_paren])
                msg_arg = MSG_ARG[fn_name]
                if len(parts) <= msg_arg:
                    continue
                code_src = parts[0].strip()
                cm = re.search(r"(\w+)\s*$", code_src)
                code = None
                if cm and cm.group(1) in CONST_TO_CODE:
                    code = int(CONST_TO_CODE[cm.group(1)])
                elif code_src.isdigit():
                    code = int(code_src)
                if code is None:
                    continue
                tmpl = extract_template(parts[msg_arg].strip())
                if tmpl is None:
                    skipped.append((path, fn_name, code))
                    continue
                if code in per_code:
                    if per_code[code] != tmpl:
                        conflicts[code] = (per_code[code], tmpl)
                else:
                    per_code[code] = tmpl

    # drop conflicted codes entirely
    for c in list(per_code):
        if c in conflicts:
            del per_code[c]

    # merge with existing templates (e.g. hand-maintained ERC entries in the
    # json! emission form, which this scanner cannot parse); scan results win.
    existing = {}
    if os.path.exists(OUT):
        with open(OUT, encoding="utf-8") as f:
            existing = json.load(f)
    existing.update({str(k): v for k, v in per_code.items()})
    merged = {k: existing[k] for k in sorted(existing, key=int)}

    with open(OUT, "w", encoding="utf-8") as f:
        json.dump(merged, f, indent=1, ensure_ascii=False)

    print(f"codes with unique template: {len(per_code)} (registry total incl. manual: {len(merged)})")
    print(f"conflicted codes (dropped): {len(conflicts)}")
    for c in sorted(conflicts):
        a, b = conflicts[c]
        print(f"  E{c:04d}: {a!r}  vs  {b!r}")
    print(f"emission points skipped (non-literal/format): {len(skipped)}")
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
