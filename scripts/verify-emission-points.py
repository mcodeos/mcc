#!/usr/bin/env python3
"""Verify Phase-3 completeness.

Scans src/ for leftover OLD numeric code literals in emission contexts
(dlog_*, diagnostic_log, record_warning/error, code: fields) and reports them.
Prints:
  - leftover emissions (still using retired old codes)
  - 'new code' emissions whose constant does NOT exist in errcodes.rs
"""
import json
import re
import os

MAPPING = json.load(open("scripts/error-code-mapping.json"))
OLD = sorted({e["old"] for e in MAPPING["mapping"]})

# All old codes known to the mapping (retired)
old_re = re.compile(r"|".join(str(c) for c in OLD))
# Emission-first-arg patterns
call_re = re.compile(
    r"(?P<fn>dlog_error|dlog_error_at|dlog_warning|dlog_warning_at|"
    r"diagnostic_log|record_warning|record_error)\(\s*(?P<code>\d+)\b"
)
code_re = re.compile(r"\bcode:\s*(?P<code>\d+)\b")

# registry constants
reg = open("src/db/diagnostic/errcodes.rs", encoding="utf-8").read()
reg_names = set(re.findall(r"pub const (\w+): u32", reg))
reg_codes = {int(c) for c in re.findall(r"pub const \w+: u32 = (\d+);", reg)}

leftover_calls = []
leftover_codes = []
unknown_names = []

for root, _dirs, files in os.walk("src"):
    for fn in files:
        if not fn.endswith(".rs"):
            continue
        path = os.path.join(root, fn)
        text = open(path, encoding="utf-8").read()
        for m in call_re.finditer(text):
            c = int(m.group("code"))
            if c in OLD:
                leftover_calls.append((path, m.group("fn"), c))
        for m in code_re.finditer(text):
            c = int(m.group("code"))
            if c in OLD:
                leftover_codes.append((path, c))

# constants used in src but missing from registry
use_re = re.compile(r"crate::errcodes::(\w+)")
for root, _dirs, files in os.walk("src"):
    for fn in files:
        if not fn.endswith(".rs"):
            continue
        path = os.path.join(root, fn)
        text = open(path, encoding="utf-8").read()
        for m in use_re.finditer(text):
            if m.group(1) not in reg_names:
                unknown_names.append((path, m.group(1)))

print(f"old codes in mapping: {len(OLD)}")
print(f"registry codes: {len(reg_codes)}  (range {min(reg_codes)}-{max(reg_codes)})")
print(f"\nLEFTOVER emission calls using retired old codes: {len(leftover_calls)}")
for p, fn, c in leftover_calls:
    print(f"  {p}: {fn}({c})")
print(f"\nLEFTOVER 'code:' fields using retired old codes: {len(leftover_codes)}")
for p, c in leftover_codes:
    print(f"  {p}: code: {c}")
print(f"\nUNKNOWN registry constants referenced: {len(unknown_names)}")
for p, n in unknown_names:
    print(f"  {p}: crate::errcodes::{n}")
