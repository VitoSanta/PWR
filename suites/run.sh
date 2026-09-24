#!/bin/sh
# Runs a model-backed suite and scores it.
#
#   sh suites/run.sh a2|a3|a4
#
# The suite's task cases run through `pwr eval run`, one campaign per corpus
# the suite draws on and limited to its cases, on PWR's MLX engine, into
# .pwr/suite-runs/<suite>-<rev>-<time>; the suite is then scored from those
# reports. Set PWR_BIN to use a frozen binary, PWR_SUITE_BACKEND for
# another backend and PWR_SUITE_MODEL for another model.
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"
case "${1:-}" in
  a2) SUITE=suites/a2-navigation.json ;;
  a3) SUITE=suites/a3-editing.json ;;
  a4) SUITE=suites/a4-verification.json ;;
  *) echo "usage: sh suites/run.sh a2|a3|a4" >&2; exit 2 ;;
esac
BIN=${PWR_BIN:-$ROOT/target/release/pwr}
BACKEND=${PWR_SUITE_BACKEND:-mlx}
MODEL=${PWR_SUITE_MODEL:-lmstudio-community/Qwen3.6-35B-A3B-MLX-4bit}
if [ "$BACKEND" = "mlx" ]; then
  export PWR_MLX_PYTHON=${PWR_MLX_PYTHON:-$ROOT/experiments/engine-spike-mlx-20260917/.venv/bin/python}
fi
# Named by the binary that runs, not by the checkout: they differ whenever a
# frozen binary is used or the tree moved on after the build.
REV=$(strings "$BIN" | grep -o 'eval-[0-9a-f]\{12\}' | head -1 | cut -c6-12)
REV=${REV:-unknown}
SLUG=$(printf '%s' "$BACKEND-$MODEL" | tr '/' '_' | cut -c1-48)
OUT=.pwr/suite-runs/$1-$REV-$SLUG-$(date +%Y%m%d-%H%M%S)
mkdir -p "$OUT"
# The model's raw replies, for harvesting suite A1 replay cases.
export PWR_MLX_TRACE=$ROOT/$OUT/model-trace.jsonl

python3 "$ROOT/suites/plan.py" "$SUITE" > "$OUT/plan.txt"
n=0
while read -r corpus tasks; do
  n=$((n + 1))
  only=""
  for task in $tasks; do only="$only --only $task"; done
  # shellcheck disable=SC2086
  "$BIN" --json --backend "$BACKEND" eval run "$corpus" --model "$MODEL" --arm b1 --seed 1 \
      --out-dir "$OUT" $only > "$OUT/campaign-$n.json" 2>&1 || true
done < "$OUT/plan.txt"

"$BIN" --json eval suite "$SUITE" --reports "$OUT" > "$OUT/suite-report.json" 2>&1 || true
python3 "$ROOT/suites/summary.py" "$OUT/suite-report.json"
echo "reports: $OUT"
