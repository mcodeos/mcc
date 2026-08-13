#!/usr/bin/env python3
"""Analyze emission-point message shapes for full template-ization.

For every emission call (dlog_*, diagnostic_log, record_warning/error),
extract the message argument and classify it:
  - literal   : a plain string literal ("...")
  - format    : format!(...) / format!("...", ...) with interpolation
  - other     : variable / concat! / to_string() etc.

Then aggregate per code to see:
  - how many distinct message shapes each code uses (1-to-1 template feasibility)
  - total emission points per shape class
"""
import os
import re
import json
from collections import defaultdict

FNS = (
    "dlog_error|dlog_error_at|dlog_warning|dlog_warning_at|dlog_trace|"
    "dlog_info|dlog_hint|diagnostic_log|record_warning|record_error"
)
CALL_START_RE = re.compile(rf"(?<![\w:])(?P<fn>{FNS})\s*\(")
CODE_ARG = {
    "dlog_error": 0, "dlog_warning": 0, "dlog_info": 0, "dlog_hint": 0,
    "dlog_error_at": 0, "dlog_warning_at": 0, "dlog_trace": 0,
    "diagnostic_log": 0, "record_warning": 0, "record_error": 0,
}
MSG_ARG = {
    "dlog_error": 2, "dlog_warning": 2, "dlog_info": 2, "dlog_hint": 2,
    "dlog_error_at": 2, "dlog_warning_at": 2, "dlog_trace": 1,
    "diagnostic_log": 4, "record_warning": 1, "record_error": 1,
}


def find_call_end(text: str, start: int) -> int:
    """Given index of '(' return index of the matching ')' (respecting strings/comments)."""
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

# registry: const name -> code
reg = open("src/db/diagnostic/errcodes.rs", encoding="utf-8").read()
CONST_TO_CODE = dict(
    re.findall(r"pub const (\w+): u32 = (\d+);", reg)
)
CODE_TO_CONST = {int(v): k for k, v in CONST_TO_CODE.items()}


def split_top_level(s: str) -> list[str]:
    """Split a call argument list on top-level commas (respecting (), [], {}, "", ')."""
    parts, depth, cur, quote = [], 0, "", None
    for ch in s:
        if quote:
            cur += ch
            if ch == quote and cur.endswith("\\" + quote) is False and (len(cur) < 2 or cur[-2] != "\\"):
                # handle escaped quote: only close if not escaped
                if not (len(cur) >= 2 and cur[-2] == "\\"):
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


def classify_msg(msg: str) -> str:
    msg = msg.strip()
    if msg.startswith("format!"):
        return "format"
    if msg.startswith('"'):
        return "literal"
    if msg.startswith("&"):
        inner = msg[1:].strip()
        if inner.startswith('"'):
            return "literal"
        if inner.startswith("format!"):
            return "format"
        return "other"
    return "other"


def extract_placeholders(msg: str) -> list[str]:
    """Rough placeholder extraction from a format! string: {} / {0} / {name}."""
    m = re.search(r'format!\s*\(\s*"((?:[^"\\]|\\.)*)"', msg)
    if not m:
        return []
    fmt = m.group(1)
    return re.findall(r"\{[^{}]*\}", fmt)


def main():
    per_code = defaultdict(lambda: {"count": 0, "shapes": defaultdict(int), "examples": []})
    shape_total = defaultdict(int)
    total = 0
    parse_errors = []

    for root, _dirs, files in os.walk("src"):
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
                    parse_errors.append((path, fn_name, "<unclosed>"))
                    continue
                args_raw = text[open_paren + 1 : close_paren]
                parts = split_top_level(args_raw)
                code_arg = CODE_ARG[fn_name]
                msg_arg = MSG_ARG[fn_name]
                if len(parts) <= msg_arg:
                    # maybe split failed (multiline call) -> skip, count separately
                    parse_errors.append((path, fn_name, "<argc>"))
                    continue
                code_src = parts[code_arg].strip()
                msg_src = parts[msg_arg].strip()
                # resolve code constant -> numeric (allow multi-segment paths: crate::errcodes::X)
                cm = re.search(r"(\w+)\s*$", code_src)
                code = None
                if cm and cm.group(1) in CONST_TO_CODE:
                    code = int(CONST_TO_CODE[cm.group(1)])
                elif code_src.isdigit():
                    code = int(code_src)
                if code is None:
                    parse_errors.append((path, fn_name, code_src))
                    continue
                shape = classify_msg(msg_src)
                shape_total[shape] += 1
                total += 1
                info = per_code[code]
                info["count"] += 1
                info["shapes"][shape] += 1
                if len(info["examples"]) < 3:
                    info["examples"].append((path, fn_name, msg_src[:120]))

    print(f"== total emission calls parsed: {total} ==")
    print(f"parse errors (unresolved): {len(parse_errors)}")
    print(f"shape distribution: {dict(shape_total)}")
    print()

    multi = [c for c, i in per_code.items() if len(i["shapes"]) > 1]
    print(f"== codes with MIXED message shapes (template conflict risk): {len(multi)} ==")
    for c in sorted(multi):
        i = per_code[c]
        print(f"  E{c:04d} ({CODE_TO_CONST.get(c,'?')}): {dict(i['shapes'])} total={i['count']}")
        for p, fn, ex in i["examples"]:
            print(f"      {p}: {fn}(...) {ex}")

    print()
    print("== sample by shape ==")
    for shape in ("literal", "format", "other"):
        shown = 0
        for c in sorted(per_code):
            if per_code[c]["shapes"].get(shape, 0) > 0:
                for p, fn, ex in per_code[c]["examples"]:
                    if ex.startswith(('"', "format!", "&")):
                        print(f"  E{c:04d} [{shape}] {p}: {fn}(...) {ex}")
                        shown += 1
                        break
            if shown >= 8:
                break

    json.dump(
        {
            str(c): {"count": i["count"], "shapes": dict(i["shapes"])}
            for c, i in sorted(per_code.items())
        },
        open("/tmp/emission-msg-analysis.json", "w"),
        indent=1,
        sort_keys=True,
    )
    print("\nfull breakdown written to /tmp/emission-msg-analysis.json")


if __name__ == "__main__":
    main()
