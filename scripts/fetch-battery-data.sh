#!/usr/bin/env bash
# Copy the benchmark battery's input data (editing traces) from an Automerge
# checkout into benches/data/battery (or $CRDT_BATTERY_DATA).
#
# Usage: scripts/fetch-battery-data.sh [path/to/automerge/rust/benchmark-battery]
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
dest="${CRDT_BATTERY_DATA:-$root/benches/data/battery}"
src="${1:-$root/../automerge/rust/benchmark-battery}"

if [ ! -d "$src/egwalker-paper" ] || [ ! -f "$src/data/edits.json" ]; then
  echo "error: $src doesn't look like automerge/rust/benchmark-battery" >&2
  echo "usage: $0 [path/to/automerge/rust/benchmark-battery]" >&2
  exit 1
fi

mkdir -p "$dest/egwalker-paper"
cp "$src/data/edits.json" "$dest/"
cp "$src"/egwalker-paper/*.json "$src/egwalker-paper/README.md" "$dest/egwalker-paper/"
echo "copied traces to $dest"
echo "next: cargo run --release --bin battery -- generate-fixtures"
