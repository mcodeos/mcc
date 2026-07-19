#!/usr/bin/env bash
# Regression test runner for mcc
# Usage:
#   ./scripts/regress.sh              — run all tests, compare to golden
#   UPDATE_EXPECT=1 ./scripts/regress.sh  — regenerate golden files
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJ_DIR="$(dirname "$SCRIPT_DIR")"
CORPUS="$PROJ_DIR/tests/corpus"
GOLDEN="$PROJ_DIR/tests/golden"
MCC="${MCC:-cargo run --}"

PASS=0
FAIL=0
DIFF_FILES=""

run_one() {
    local mc_file="$1"
    local rel="${mc_file#$CORPUS/}"
    local golden_file="$GOLDEN/${rel%.mc}.expected.json"

    # Run mcc parse --json
    local output
    output=$($MCC parse --format json "$mc_file" 2>/dev/null) || {
        echo "  FAIL (crash): $rel"
        FAIL=$((FAIL + 1))
        DIFF_FILES="$DIFF_FILES $rel"
        return
    }

    # Extract diagnostics + summary (strip volatile fields like workspace)
    local filtered
    filtered=$(echo "$output" | python3 -c "
import json, sys
data = json.load(sys.stdin)
# Keep diagnostics and summary, drop workspace (volatile)
out = {
    'diagnostics': data.get('diagnostics', []),
    'summary': data.get('summary', {}),
}
print(json.dumps(out, indent=2, sort_keys=True))
" 2>/dev/null) || {
        echo "  FAIL (json parse): $rel"
        FAIL=$((FAIL + 1))
        DIFF_FILES="$DIFF_FILES $rel"
        return
    }

    if [ "${UPDATE_EXPECT:-}" = "1" ]; then
        mkdir -p "$(dirname "$golden_file")"
        echo "$filtered" > "$golden_file"
        echo "  UPDATED: $rel"
        PASS=$((PASS + 1))
        return
    fi

    if [ ! -f "$golden_file" ]; then
        echo "  SKIP (no golden): $rel"
        FAIL=$((FAIL + 1))
        DIFF_FILES="$DIFF_FILES $rel"
        return
    fi

    if diff -q <(echo "$filtered") "$golden_file" >/dev/null 2>&1; then
        PASS=$((PASS + 1))
    else
        echo "  FAIL (diff): $rel"
        FAIL=$((FAIL + 1))
        DIFF_FILES="$DIFF_FILES $rel"
        # Show diff snippet
        diff <(echo "$filtered") "$golden_file" | head -20
    fi
}

echo "=== mcc regression ==="
echo "Corpus: $CORPUS"
echo "Golden: $GOLDEN"
echo "Mode:   ${UPDATE_EXPECT:+UPDATE} ${UPDATE_EXPECT:-VERIFY}"
echo ""

# Build first
echo "Building..."
cargo build --quiet 2>/dev/null || { echo "Build failed"; exit 1; }

# Run all .mc files
while IFS= read -r -d '' mc_file; do
    run_one "$mc_file"
done < <(find "$CORPUS" -name "*.mc" -type f -print0)

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
if [ -n "$DIFF_FILES" ] && [ "${UPDATE_EXPECT:-}" != "1" ]; then
    echo ""
    echo "Files with differences:"
    for f in $DIFF_FILES; do
        echo "  $f"
    done
    exit 1
fi
