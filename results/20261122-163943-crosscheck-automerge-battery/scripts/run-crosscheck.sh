#!/usr/bin/env bash
# Cross-check: Automerge's rust/benchmark-battery vs crdt-bench's port, both
# linked against the same development Automerge (automerge checkout HEAD).
# Runs are strictly sequential so they never compete for the CPU.
set -euo pipefail

WT=/home/vincent/proj/crdt-bench/.claude/worktrees/crosscheck-am-battery
AM_BATTERY=/home/vincent/proj/automerge/rust/benchmark-battery
OUT="$1"
SETTINGS=(--tier all --sample-count 10 --warmup-count 1)  # same as results/20261119-170021-battery-all-tiers

mkdir -p "$OUT/automerge-battery"
echo "== $(date -Is) automerge battery: all tiers"
cd "$AM_BATTERY"
../target/release/benchmark-battery run -o "$OUT/automerge-battery/battery.json" "${SETTINGS[@]}" \
  --report-dir "$OUT/automerge-battery/report" --report-format markdown

echo "== $(date -Is) automerge battery: memory"
"$WT/target-am-memory/release/benchmark-battery" run -o "$OUT/automerge-battery/battery.json" \
  --filter memory --append "${SETTINGS[@]}" \
  --report-dir "$OUT/automerge-battery/report" --report-format markdown

cd "$WT"
export CRDT_BATTERY_DATA=/home/vincent/proj/crdt-bench/benches/data/battery
echo "== $(date -Is) crdt-bench battery (automerge only): all tiers"
./target/release/battery run --experiment "$OUT/crdt-bench-dev-automerge" --library automerge "${SETTINGS[@]}"

echo "== $(date -Is) crdt-bench battery (automerge only): memory"
./target-memory/release/battery run --experiment "$OUT/crdt-bench-dev-automerge" --append \
  --filter memory --library automerge "${SETTINGS[@]}"
echo "== $(date -Is) done"
