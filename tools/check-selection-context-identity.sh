#!/usr/bin/env bash
# Match the debugger's observed process to this same XCTest/bundle generation.
set -euo pipefail
test "$#" = 1 || test "$#" = 2
OUT=$1
TRACE="$OUT/captured-selection-context"
one() {
  test "$(grep -Ec "^$2=" "$1")" = 1 || return 1
  sed -n "s/^$2=//p" "$1"
}
PID=$(one "$TRACE-process-identity.txt" pid)
[[ "$PID" =~ ^[1-9][0-9]*$ ]]
SIM=$(one "$TRACE-process-identity.txt" simulator)
BID=$(one "$TRACE-process-identity.txt" bundle_id)
test -n "$SIM"
test "$BID" = com.autodevjapan.phonecontrol1614abiprobe
test "$(one "$TRACE-owned-target.txt" simulator)" = "$SIM"
test "$(one "$TRACE-owned-target.txt" bundle_id)" = "$BID"
START=$(one "$TRACE-process-identity.txt" started)
test -n "$START"
BIN=$(one "$TRACE-owned-target.txt" installed_binary)
test -n "$BIN"
UUID=$(awk '/^UUID: / {print $2}' "$TRACE-image-uuid.txt")
[[ "$UUID" =~ ^[A-Fa-f0-9-]{36}$ ]]
if [ "${2:-}" = --attached ]; then
  # LLDB's platform shell forwards arguments; redirection belongs inside this
  # actual Bash process, never in the LLDB command line passed to ps.
  LC_ALL=C ps -p "$PID" -o lstart= > "$TRACE-attached-start.txt"
  ps -p "$PID" -o comm= > "$TRACE-attached-command.txt"
  test "$(cat "$TRACE-attached-start.txt")" = "$START"
  test "$(cat "$TRACE-attached-command.txt")" = "$BIN"
  grep -Eq "^Process $PID (stopped|is stopped)" "$TRACE.unsanitized"
  test "$(grep -F "$UUID" "$TRACE.unsanitized" | grep -Fc "$BIN")" = 1
  test "$(LC_ALL=C ps -p "$PID" -o lstart=)" = "$START"
  test "$(ps -p "$PID" -o comm=)" = "$BIN"
  xcrun simctl spawn "$SIM" launchctl list > "$TRACE-attached-processes.txt"
  test "$(awk -v p="$PID" -v b="$BID" '$1==p&&index($0,b)>0{n++}END{print n+0}' "$TRACE-attached-processes.txt")" = 1
  touch "$TRACE.ready"
  exit 0
fi
test "$(cat "$TRACE-attached-start.txt")" = "$START"
test "$(cat "$TRACE-attached-command.txt")" = "$BIN"
test "$(grep -Ec '^XCTest context-trace: target_pid_before=[1-9][0-9]*$' "$OUT/xcode-ui-test.txt")" = 1
test "$(grep -Ec '^XCTest context-trace: target_pid_before_close=[1-9][0-9]*$' "$OUT/xcode-ui-test.txt")" = 1
test "$(sed -n 's/^XCTest context-trace: target_pid_before=//p' "$OUT/xcode-ui-test.txt")" = "$PID"
test "$(sed -n 's/^XCTest context-trace: target_pid_before_close=//p' "$OUT/xcode-ui-test.txt")" = "$PID"
grep -Eq "^Process $PID (stopped|is stopped)" "$TRACE.txt"
test "$(grep -Fc "$UUID" "$TRACE-attached-image.txt")" = 1
test "$(grep -Fc "$BIN" "$TRACE-attached-image.txt")" = 1
printf 'same_xctest_process_identity_verified=true\npid=%s\nsimulator=%s\n' "$PID" "$SIM" > "$TRACE-identity-result.txt"
