#!/usr/bin/env bash
# Mirror the repo knowledge tree into the Tauri-bundled resources so the
# packaged IDE ships the same files the server reads at runtime.
# Usage: scripts/sync-knowledge.sh
set -euo pipefail

here="$(cd "$(dirname "$0")/.." && pwd)"
src="$here/mcide/knowledge/"
dst="$here/mcide/src-tauri/resources/knowledge/"

if [ ! -d "$src" ]; then
    echo "error: source knowledge tree not found: $src" >&2
    exit 1
fi

mkdir -p "$dst"
rsync -a --delete "$src" "$dst"
echo "synced: $src -> $dst"
