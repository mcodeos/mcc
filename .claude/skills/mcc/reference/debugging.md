# 6. Debugging mcode Projects

### 6.1 Project Structure

```
my-project/
├── project.toml          # Required: [project] + [dependencies]
├── src/
│   ├── main.mc           # Entry file (referenced in project.toml)
│   └── sub_module.mc     # Other .mc files
```

### 6.2 Common Workflows

```bash
# Create a new project
mcc proj create my-project

# Quick syntax/diagnostic check
mcc check ./my-project

# Parse and show structure
mcc parse ./my-project -f json-pretty

# Build and visualize
mcc build --viz
# Opens circuit.html in browser

# Show what's defined in a file
mcc show all -F src/main.mc

# Find a component definition
mcc show component RES --lib mcode

# Search for components matching a pattern
mcc search "CAP" --kind component

# Show instances (what's actually used)
mcc show instances TOP_MODULE --top TOP_MODULE -F src/main.mc

# Export netlist
mcc export netlist src/main.mc --top main --json
```

### 6.3 Diagnosing Errors

```bash
# Get all diagnostics for a file
mcc check path/to/file.mc

# With strict checking
mcc check path/to/file.mc --strict

# Explain a specific error code
mcc explain 1100

# Full diagnostics via RPC (with server running)
curl -X POST http://127.0.0.1:8080/rpc \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"diagnostics","params":{"uri":"file:///absolute/path/to/file.mc"},"id":1}'

# Get semantic tokens + symbols for a file
curl -X POST http://127.0.0.1:8080/rpc \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"sem","params":{"uri":"file:///absolute/path/to/file.mc"},"id":1}'
```

### 6.4 Common Error Codes

| Code               | Meaning                          | Typical Cause                                                 |
| ------------------ | -------------------------------- | ------------------------------------------------------------- |
| 1001-1005          | Duplicate definition             | Same name used twice in scope                                 |
| 1051-1060          | Definition structure / CMIE load | Missing subnodes, malformed IO type, define-as-CMIE           |
| 2001-2010          | `use` statement errors           | Bad path, target not found, self import, alias collision      |
| 2051 / 2061 / 2071 | Use-stage dependency errors      | Undeclared dep, symbol conflict, import not found             |
| 2080-2119          | Parser errors                    | Syntax error, invalid clause / pin / net / conds              |
| 2121-2127          | Name / declaration parse errors  | Missing subnode, failed name extraction                       |
| 2171-2172          | Unsupported / unresolved symbol  | P1-P5 lookup failed, not supported yet                        |
| 2901-2906          | Vector shape validation          | Shape mismatch, transpose limit, expand mismatch              |
| 3001-3008          | Pin/port definition              | Pin ID/name mismatch, count errors                            |
| 3021-3023          | Attribute errors                 | Type mismatch, unsupported type, missing subnode              |
| 3041-3049          | Unit value (UVAL) errors         | Invalid/unsupported unit, bad value format                    |
| 3051-3054          | Module body errors               | PINS unsupported, role unsupported, unexpected param          |
| 3071 / 3081        | Module method / clause           | Method not found, unexpected clause type                      |
| 3101-3111          | Params / functions               | Invalid param, class/instance expected                        |
| 3131-3135          | Function calls / lines           | Missing name, parse failure, dropped line                     |
| 3151-3180          | Instance / interface reference   | Class unresolved, member / pin / port not found               |
| 4001-4026          | Connection / shape               | Transpose mismatch, parallel/series invalid, dot misuse       |
| 4050-4058          | Netlist heuristics (D-series)    | Ghost port, merged short, sort hazard, floating `_`           |
| 4081-4098          | Layout attribute errors          | Missing subnode, type mismatch, malformed edge                |
| 4101-4118          | Netlist / interface binding      | Multi-drive, no driver, unconnected, backfeed risk            |
| 4150-4178          | Instantiation checks             | Chain link skipped, arg count mismatch, bind failed           |
| 5001-5003          | Cross-file duplicates            | Same name defined in another file                             |
| 5050-5099          | Naming / style                   | Lowercase component, single-char instance, shadows CMIE       |
| 5101-5104          | Reference integrity              | Undeclared spec key, function without body                    |
| 5151-5163          | Ports / pins                     | Duplicate port, unused pin/port, conflicting options          |
| 5201-5206          | Functions / roles / defaults     | Bad param default, enum single value                          |
| 5251-5267          | Definition structure (M-series)  | Empty body, no pins, duplicate spec key                       |
| 5301-5304          | `.int` class checks              | Ambiguous name, class not loaded, unconventional suffix       |
| 5351-5357          | Instance / attribute checks      | Reserved keyword, arg count, nesting too deep                 |
| 5401-5412          | Enum / expression checks         | Duplicate value, reversed range, `this` at top level          |
| 5451-5459          | Condition blocks                 | Empty body, if without else, NC at component level            |
| 5501-5510          | Hardware checks                  | Power pins excess, pin number gaps, NC contiguous             |
| 5551-5552          | Type / unit compatibility        | Free closure variable, incompatible types                     |
| 5641-5643          | Global diagnostics               | Unused param/port, untyped param                              |
| 6001-6004          | ERC                              | Single-point net, unconnected port, multi-drive, floating net |

The D1-d7 detector codes referenced by build.rs tests map as follows:

| Detector | Code | Constant                  |
| -------- | ---- | ------------------------- |
| D1       | 4053 | SORT\_HAZARD              |
| D2       | 4054 | FLOATING\_PLACEHOLDER     |
| D3       | 4051 | NET\_MERGED\_SHORT        |
| D4       | 4050 | GHOST\_PORT\_BOX          |
| D5       | 4052 | NET\_BUS\_ORDER\_MISMATCH |
| D6       | 4057 | NET\_DROPPED\_STATEMENT   |
| D7       | 4056 | PULLUP\_DEGENERATE        |

### 6.5 Validating Library Changes

When modifying mcode library files:

```bash
# 0. Sync the repo library into the runtime dir, then restart any stale server.
#    mcc reads ~/.mcode/mcode at RUNTIME, and a running `mcc start` server holds
#    its own in-memory copy — it does not pick up file edits. Skip this step and
#    you will chase phantom diagnostics that vanish under --local (§5.3).
bash ~/work/mo/mcode/cp.sh        # repo mcode -> ~/.mcode/mcode
mcc stop || true                  # kill any stale server (also: lsof -i :8080)
mcc check ./path/to/changed.mc -L # -L forces local: verify against on-disk state

# 1. Check modified file for syntax errors
mcc check ./path/to/changed.mc

# 2. Parse with library loaded
mcc parse ./path/to/changed.mc --lib mcode

# 3. Build a test project that uses the changed component
cd ~/work/mo/mcs/hbl
mcc build

# 4. Full rebuild with visualization
mcc build --viz

# 5. Run mcc's internal test suite
cd ~/work/mo/mcc
cargo test
```

After editing a library file, always re-sync (`cp.sh`) AND restart the server
before trusting a default `mcc check` / IDE diagnostic. A clean result from a
`-L` run proves the source is fine; a still-failing default run means the
server is serving stale state (§5.3).

### 6.6 Lapper / RefDefMap Debug Dump

```bash
# Local mode (no server needed) — F12_DIAG text format (default)
mcc show lapper path/to/file.mc

# Load library first
mcc show lapper --lib mcode path/to/file.mc

# JSON output
mcc show lapper path/to/file.mc -f json-pretty

# Save to file (suppress AST tree noise)
mcc show lapper path/to/file.mc 2>/dev/null > dump.txt
```

**Text output sections:**

| Section          | Content                                               |
| ---------------- | ----------------------------------------------------- |
| `LAPPER ENTRIES` | All symbol intervals: kind, id, span, name, file      |
| `DECLARES`       | name\_to\_declare\_id entries: id, span, scope, name  |
| `REFERENCES`     | inst\_id\_to\_span entries: id, span, declare\_id     |
| `DEF_MAP`        | (def\_kind, decl\_id) → SourceLocation                |
| `REF_ENTRIES`    | Pre-collected refs: (ref\_kind, decl\_id, span, name) |
| `REF_DEF_MAP`    | **Core**: Ref→Def resolution with kind\_names legend  |

**Quick analysis:**

```bash
# Count MAP entries by ref→def kind pair
grep "F12_DIAG MAP:" dump.txt | sed 's/.*Ref(//;s/).*=> Def(/ -> /;s/,.*//' | sort | uniq -c | sort -rn

# Check all ClassRef resolutions
grep "Ref(ClassRef" dump.txt

# Check all FuncParamRef resolutions
grep "Ref(FuncParamRef" dump.txt
```

**Common debugging workflow:**

```bash
# 1. Without library — check local def/ref mappings
mcc show lapper src/us513.mc 2>/dev/null | grep "^F12_DIAG MAP:"

# 2. With library — check cross-file library class resolution
mcc show lapper --lib mcode src/us513.mc 2>/dev/null | grep "^F12_DIAG MAP:"

# 3. Drill into specific ref type
mcc show lapper --lib mcode src/us513.mc 2>/dev/null | grep "Ref(ClassRef"

# 4. Compare two runs
diff <(mcc show lapper file.mc 2>/dev/null | grep MAP) \
     <(mcc show lapper --lib mcode file.mc 2>/dev/null | grep MAP)
```

**JSON analysis — F12 failure root cause (Lapper ID vs RefDefMap ID match):**

```bash
mcc show lapper us513.mc --lib mcode -f json | python3 -c "
import json,sys
d=json.load(sys.stdin)
l={e['id'] for e in d['lapper'] if e['kind']==1}          # ClassRef kind=1
r={e['ref_id'] for e in d['ref_def_map']['entries'] if e['ref_kind']==1}
print('MISMATCHED:', sorted(l - r))                       # non-empty = F12 broken
"
```

**Quick debug flow (F12 goto-def issues):**

```bash
mcc show interface DC --lib mcode && mcc show component CAP --lib mcode  # def exists?
mcc def DC --lib mcode && mcc def CAP --lib mcode                        # correct target?
mcc show lapper us513.mc --lib mcode -f json | python3 -m json.tool      # raw data
```
