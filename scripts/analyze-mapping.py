#!/usr/bin/env python3
"""Analyze error-code-mapping.json to plan Phase-3 replacements.

Prints:
  1. Per-file replacement table (old -> new) for an optional [lo, hi] cluster.
  2. Ambiguous sites: same (file, old) mapping to different constants — these
     need manual/site-aware replacement.
"""
import json
import sys
from collections import defaultdict

MAPPING = json.load(open("scripts/error-code-mapping.json"))["mapping"]

lo, hi = 0, 99999
if len(sys.argv) >= 3:
    lo, hi = int(sys.argv[1]), int(sys.argv[2])

by_file = defaultdict(list)
for e in MAPPING:
    if e["new"] is None:
        continue
    if not (lo <= e["new"] <= hi):
        continue
    file = e["site"].split(":")[0]
    by_file[file].append(e)

print("=" * 72)
print("AMBIGUOUS: same (file, old) -> different constants")
print("=" * 72)
amb = 0
for file, entries in sorted(by_file.items()):
    by_old = defaultdict(list)
    for e in entries:
        by_old[e["old"]].append(e)
    for old, es in sorted(by_old.items()):
        names = {e["name"] for e in es}
        if len(names) > 1:
            amb += 1
            print(f"{file}: old={old} -> " + "; ".join(
                f"{e['name']}({e['new']})[{e['site']}]" for e in es))
print(f"(ambiguous groups: {amb})")

print()
print("=" * 72)
print("REPLACEMENT TABLE")
print("=" * 72)
for file, entries in sorted(by_file.items()):
    entries.sort(key=lambda x: (x["old"], x["site"]))
    print(f"--- {file} ---")
    for e in entries:
        flag = ""
        print(f"  {e['old']:>5} -> {e['name']:<40} ({e['new']:<5}) {e['site']}")
