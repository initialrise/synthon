#!/usr/bin/env bash
# PRD §6: first-frame time + peak RSS on the 1 MB reference doc. Needs a display.
set -euo pipefail
cd "$(dirname "$0")"
cargo build --release -q
mkdir -p target/bench
python3 fixtures/gen_reference.py > target/bench/reference.md
for mode in "" "--dark --full-width"; do
  echo "== synthon $mode reference.md ($(du -h target/bench/reference.md | cut -f1))"
  SYNTHON_BENCH=1 /usr/bin/time -f "peak RSS: %M KB" ./target/release/synthon $mode target/bench/reference.md
done
echo "budgets: first frame < 100 ms, peak RSS < 80000 KB"
