#!/bin/sh
# Runs every tests/*_test.lua with `lua`; fails when any does.
cd "$(dirname "$0")/.." || exit 1
status=0
count=0
for file in tests/*_test.lua; do
  count=$((count + 1))
  lua "$file" || status=1
done
if [ "$status" -eq 0 ]; then
  echo "ALL $count TEST FILES PASSED"
fi
exit "$status"
