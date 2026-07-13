#!/usr/bin/env bash
# Auto-bump patch version in Cargo.toml before each commit.
# Install: cp scripts/bump-version.sh .git/hooks/pre-commit && chmod +x .git/hooks/pre-commit
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
CARGO="$ROOT/Cargo.toml"

current=$(grep -m1 '^version\s*=' "$CARGO" | sed 's/.*"\(.*\)".*/\1/')
if [[ -z "$current" ]]; then
    echo "bump-version: could not parse version from Cargo.toml" >&2
    exit 1
fi

major=$(echo "$current" | cut -d. -f1)
minor=$(echo "$current" | cut -d. -f2)
patch=$(echo "$current" | cut -d. -f3)
new="$major.$minor.$((patch + 1))"

sed -i '' "s/^version = \"$current\"/version = \"$new\"/" "$CARGO"
git add "$CARGO"

echo "bump-version: $current → $new"
