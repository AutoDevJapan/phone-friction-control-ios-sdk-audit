#!/bin/bash
set -euo pipefail
# Diagnostic only: attach to the test app launched by this job.
xcrun simctl help launch > "$OUT/simctl-launch-help.txt" 2>&1
xcrun lldb --version > "$OUT/lldb-version.txt" 2>&1
grep -F -- '--wait-for-debugger' "$OUT/simctl-launch-help.txt"
LAUNCH_STATUS=0
perl -e 'alarm 45; exec @ARGV; die "exec failed\n"' xcrun simctl launch --wait-for-debugger --stdout="$OUT/app-stdout.txt" --stderr="$OUT/app-stderr.txt" "$SIM_ID" "$BID" > "$OUT/launch.txt" 2>&1 || LAUNCH_STATUS=$?
printf 'launch_status=%s\n' "$LAUNCH_STATUS" > "$OUT/launch_status.txt"
PID=$(awk -v b="$BID" 'index($0,b ": ")==1 {p=substr($0,length(b)+3); if(p ~ /^[0-9]+$/) print p}' "$OUT/launch.txt")
printf 'pid=%s\n' "$PID" > "$OUT/pid.txt"
test "$LAUNCH_STATUS" = 0
[[ "$PID" =~ ^[0-9]+$ ]]
STATUS=0
perl -e 'alarm 90; exec @ARGV; die "exec failed\n"' xcrun lldb --batch \
  -o "target create $OUT/Product.app/product" \
  -o "process attach --pid $PID" \
  -o 'breakpoint set --name exit --name _exit --name abort' \
  -o 'continue' -o 'thread backtrace all' -o 'process detach' \
  > "$OUT/exit-stack.txt" 2>&1 || STATUS=$?
printf 'lldb_status=%s\n' "$STATUS" > "$OUT/lldb-status.txt"
# A completed debugger command alone is not an application acceptance result.
