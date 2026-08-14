#!/usr/bin/env python3
"""Phase-3 emission-point replacement.

Reads scripts/error-code-mapping.json and replaces hardcoded OLD diagnostic
code literals at their emission sites with the new registry constants
(`crate::errcodes::NAME`). Runs per cluster [lo, hi] of NEW codes.

Ambiguous (file, old) groups — same file, same old code mapping to different
constants — are skipped and reported for manual handling.

Usage: python3 scripts/replace-emission-points.py [lo hi]
"""
import json
import os
import re
import sys
from collections import defaultdict

MAPPING = json.load(open("scripts/error-code-mapping.json"))["mapping"]

lo, hi = 0, 99999
if len(sys.argv) >= 3:
    lo, hi = int(sys.argv[1]), int(sys.argv[2])

# site basename -> real src paths (multi-file sites apply in every listed file)
SITE_PATHS = {
    "": [],  # registry-constant references only; no literal replacement
    "astdef.h": ["src/ast/c/astdef.h"],
    "ast_node.rs": ["src/ast/ast_node.rs"],
    "mc_code.rs": ["src/db/infra/mc_code.rs"],
    "mc_use.rs": ["src/db/infra/mc_use.rs"],
    "mc_inst.rs": ["src/semantic/mc_inst.rs"],
    "mc_enum.rs": ["src/semantic/mc_enum.rs"],
    "mc_func.rs": ["src/semantic/mc_func.rs"],
    "mc_fcall.rs": ["src/semantic/basic/mc_fcall.rs"],
    "mc_attr.rs": ["src/semantic/component/mc_attr.rs"],
    "mc_paramd.rs": ["src/semantic/basic/mc_paramd.rs"],
    "mc_kvs.rs": ["src/semantic/basic/mc_kvs.rs"],
    "mc_uval.rs": ["src/semantic/basic/mc_uval.rs"],
    "mc_ids.rs": ["src/semantic/basic/mc_ids.rs"],
    "mc_opd.rs": ["src/semantic/basic/mc_opd.rs"],
    "mc_phrase.rs": ["src/semantic/basic/mc_phrase.rs"],
    "mc_layout.rs": ["src/semantic/component/mc_layout.rs"],
    "mc_pins/mod.rs": ["src/semantic/component/mc_pins/mod.rs"],
    "module/mod.rs": ["src/semantic/module/mod.rs"],
    "lib.rs": ["src/lib.rs"],
    "visit.rs": ["src/vector/builder/visit.rs"],
    "group.rs": ["src/instant/mc_mod/group.rs"],
    "funccall.rs": ["src/instant/mc_mod/funccall.rs"],
    "fcallinst.rs": ["src/instant/mc_mod/fcallinst.rs"],
    "iterated.rs": ["src/instant/mc_mod/iterated.rs"],
    "line.rs": ["src/instant/mc_mod/line.rs"],
    "phases.rs": ["src/instant/mc_mod/phases.rs"],
    "mod.rs": ["src/instant/mc_mod/mod.rs"],
    "fromblock.rs": ["src/vector/graph/fromblock.rs"],
    "instref.rs": ["src/semantic/instref.rs"],
    "body.rs": ["src/semantic/validation/body.rs"],
    "duplicate.rs": ["src/semantic/validation/duplicate.rs"],
    "dupwithin.rs": ["src/semantic/validation/dupwithin.rs"],
    "refs.rs": ["src/semantic/validation/refs.rs"],
    "naming.rs": ["src/semantic/validation/naming.rs"],
    "insts.rs": ["src/semantic/validation/insts.rs"],
    "attrs.rs": ["src/semantic/validation/attrs.rs"],
    "defs.rs": ["src/semantic/validation/defs.rs"],
    "exprs.rs": ["src/semantic/validation/exprs.rs"],
    "enums.rs": ["src/semantic/validation/enums.rs"],
    "conds.rs": ["src/semantic/validation/conds.rs"],
    "extra.rs": ["src/semantic/validation/extra.rs"],
    "interface.rs": ["src/semantic/validation/interface.rs"],
    "hw.rs": ["src/semantic/validation/hw.rs"],
    "imports.rs": ["src/semantic/validation/imports.rs"],
    "nets/mod.rs": ["src/semantic/validation/nets/mod.rs"],
    "pins.rs": ["src/semantic/validation/pins.rs"],
    "ports.rs": ["src/semantic/validation/ports.rs"],
    "phases.rs": ["src/instant/mc_mod/phases.rs"],
    "types.rs": ["src/semantic/validation/types.rs"],
    "style.rs": ["src/semantic/validation/style.rs"],
    # multi-file sites
    "attrs.rs/insts.rs": ["src/semantic/validation/attrs.rs", "src/semantic/validation/insts.rs"],
    "naming.rs/style.rs": ["src/semantic/validation/naming.rs", "src/semantic/validation/style.rs"],
    "mc_func.rs/module/mod.rs": ["src/semantic/mc_func.rs", "src/semantic/module/mod.rs"],
    "mc_paramd.rs/mc_pins/mod.rs": ["src/semantic/basic/mc_paramd.rs", "src/semantic/component/mc_pins/mod.rs"],
    "mc_ids.rs/mc_opd.rs/mc_inst.rs": ["src/semantic/basic/mc_ids.rs", "src/semantic/basic/mc_opd.rs", "src/semantic/mc_inst.rs"],
    "group.rs/visit.rs": ["src/instant/mc_mod/group.rs", "src/vector/builder/visit.rs"],
    "mc_layout.rs/mc_phrase.rs": ["src/semantic/component/mc_layout.rs", "src/semantic/basic/mc_phrase.rs"],
    "mc_paramd.rs/mc_pins": ["src/semantic/basic/mc_paramd.rs", "src/semantic/component/mc_pins/mod.rs"],
    "mc_pins/mod.rs/mc_inst.rs": ["src/semantic/component/mc_pins/mod.rs", "src/semantic/mc_inst.rs"],
    "nets/mod.rs/interface.rs": ["src/semantic/validation/nets/mod.rs", "src/semantic/validation/interface.rs"],
    "funccall.rs/iterated.rs/line.rs": ["src/instant/mc_mod/funccall.rs", "src/instant/mc_mod/iterated.rs", "src/instant/mc_mod/line.rs"],
    "errcodes.rs": [],
}

def resolve_paths(site: str):
    base = site.split(":")[0]
    return SITE_PATHS.get(base, [])

# entries in cluster range
entries = [e for e in MAPPING if e["new"] is not None and lo <= e["new"] <= hi]

# group by (site-file) and detect ambiguity
by_file_old = defaultdict(list)
for e in entries:
    for p in resolve_paths(e["site"]):
        by_file_old[(p, e["old"])].append(e)

# split into replaceable / ambiguous
replace_map = {}   # (path, old) -> name
ambiguous = defaultdict(list)
for (path, old), es in sorted(by_file_old.items()):
    names = {e["name"] for e in es}
    if len(names) > 1:
        for e in es:
            ambiguous[(path, old)].append((e["name"], e["new"], e["site"]))
    else:
        replace_map[(path, old)] = es[0]["name"]

# apply replacements
call_re = re.compile(
    r"(?P<fn>dlog_error|dlog_error_at|dlog_warning|dlog_warning_at|diagnostic_log|"
    r"record_warning|record_error)"
    r"\(\s*(?P<code>\d+)\b"
)
code_re = re.compile(r"\bcode:\s*(?P<code>\d+)\b")

files_touched = set()
replaced_calls = 0
replaced_code_fields = 0
for (path, old), name in sorted(replace_map.items()):
    src = open(path, encoding="utf-8").read()
    orig = src

    def sub_call(m):
        global replaced_calls
        if int(m.group("code")) == old:
            replaced_calls += 1
            return f"{m.group('fn')}(crate::errcodes::{name}"
        return m.group(0)

    def sub_code(m):
        global replaced_code_fields
        if int(m.group("code")) == old:
            replaced_code_fields += 1
            return f"code: crate::errcodes::{name}"
        return m.group(0)

    src = call_re.sub(sub_call, src)
    src = code_re.sub(sub_code, src)
    if src != orig:
        open(path, "w", encoding="utf-8").write(src)
        files_touched.add(path)

print(f"cluster {lo}-{hi}: {len(entries)} entries, "
      f"{len(replace_map)} replaceable (file,old) groups, "
      f"{len(ambiguous)} ambiguous groups")
print(f"replaced {replaced_calls} dlog/diagnostic calls, "
      f"{replaced_code_fields} 'code:' fields in {len(files_touched)} files")

if ambiguous:
    print("\nAMBIGUOUS — handle manually:")
    for (path, old), es in sorted(ambiguous.items()):
        print(f"  {path}: old={old} -> " + "; ".join(
            f"{n}({c})[{s}]" for n, c, s in es))

# report mapping entries whose old code was NOT found anywhere (possible stale)
for e in entries:
    if e["site"].split(":")[0] == "":
        continue
    paths = resolve_paths(e["site"])
    for p in paths:
        if not os.path.exists(p):
            print(f"  !! MISSING PATH: {e['site']} -> {p}")
