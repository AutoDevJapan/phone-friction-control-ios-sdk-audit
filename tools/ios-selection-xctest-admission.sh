#!/usr/bin/env bash
# Start the existing bounded debugger flight only after this real XCTest method.
set -euo pipefail
test "$#" = 3
OUT=$1 APP=$2 SIM_ID=$3
TRACE="$OUT/captured-selection-context"
RUNNER_BID=com.autodevjapan.phonecontrol1614consenttests.xctrunner
TRACE_PID=''
bounded_ps() { perl -e 'alarm 5; exec @ARGV; die "exec failed\n"' ps "$@"; }
cleanup_admission() {
  STATUS=$?
  trap - EXIT
  if [ -n "$TRACE_PID" ]; then
    kill "$TRACE_PID" 2>/dev/null || true
    wait "$TRACE_PID" 2>/dev/null || true
  fi
  printf 'xctest_admission_exit=%s\n' "$STATUS" >> "$TRACE-admission-status.txt"
  exit "$STATUS"
}
trap cleanup_admission EXIT
trap 'exit 143' TERM
trap 'exit 130' INT
test -f "$TRACE-xctest-process.txt"
XCTEST_PID=$(cat "$TRACE-xctest-process.txt")
[[ "$XCTEST_PID" =~ ^[1-9][0-9]*$ ]]
XCTEST_STARTED=$(LC_ALL=C bounded_ps -p "$XCTEST_PID" -o lstart=)
test -n "$XCTEST_STARTED"
test ! -e "$TRACE-runner-binding.txt"
START=$SECONDS
while :; do
  test ! -e "$OUT/captured-selection-xctest-exit.txt"
  test "$(LC_ALL=C bounded_ps -p "$XCTEST_PID" -o lstart=)" = "$XCTEST_STARTED"
  test "$((SECONDS-START))" -lt 180
  if [ -f "$OUT/xcode-ui-test.txt" ]; then
    LINES=$(grep -Ec '^XCTest context-trace: runner_ready_pid=[1-9][0-9]*; nonce=[1-9][0-9]*-[0-9]+; host_pid=[1-9][0-9]*$' "$OUT/xcode-ui-test.txt" || true)
    if [ "$LINES" = 1 ]; then break; fi
    test "$LINES" = 0
  fi
  sleep 0.1
done
RUNNER_PID=$(sed -nE 's/^XCTest context-trace: runner_ready_pid=([1-9][0-9]*); nonce=[1-9][0-9]*-[0-9]+; host_pid=[1-9][0-9]*$/\1/p' "$OUT/xcode-ui-test.txt")
NONCE=$(sed -nE 's/^XCTest context-trace: runner_ready_pid=[1-9][0-9]*; nonce=([1-9][0-9]*-[0-9]+); host_pid=[1-9][0-9]*$/\1/p' "$OUT/xcode-ui-test.txt")
HOST_READY_PID=$(sed -nE 's/^XCTest context-trace: runner_ready_pid=[1-9][0-9]*; nonce=[1-9][0-9]*-[0-9]+; host_pid=([1-9][0-9]*)$/\1/p' "$OUT/xcode-ui-test.txt")
[[ "$HOST_READY_PID" =~ ^[1-9][0-9]*$ ]]
[[ "$RUNNER_PID" =~ ^[1-9][0-9]*$ ]]
[[ "$NONCE" =~ ^$RUNNER_PID-[0-9]+$ ]]
DATA=$(perl -e 'alarm 5; exec @ARGV; die "exec failed\n"' xcrun simctl get_app_container "$SIM_ID" "$RUNNER_BID" data)
RUNNER_APP=$(perl -e 'alarm 5; exec @ARGV; die "exec failed\n"' xcrun simctl get_app_container "$SIM_ID" "$RUNNER_BID" app)
RUNNER_BIN="$RUNNER_APP/ProbeUITests-Runner"
test -x "$RUNNER_BIN"
READY="$DATA/Documents/captured-selection-trace-runner-ready.txt"
printf 'runner_pid=%s\nnonce=%s\nhost_pid=%s\n' "$RUNNER_PID" "$NONCE" "$HOST_READY_PID" > "$TRACE-runner-expected.txt"
cmp "$READY" "$TRACE-runner-expected.txt"
STARTED=$(LC_ALL=C bounded_ps -p "$RUNNER_PID" -o lstart=)
test -n "$STARTED"
test "$(bounded_ps -p "$RUNNER_PID" -o comm=)" = "$RUNNER_BIN"
perl -e 'alarm 5; exec @ARGV; die "exec failed\n"' xcrun simctl spawn "$SIM_ID" launchctl list > "$TRACE-runner-processes.txt"
test "$(awk -v p="$RUNNER_PID" -v b="$RUNNER_BID" '$1==p&&index($0,b)>0{n++}END{print n+0}' "$TRACE-runner-processes.txt")" = 1
printf 'runner_pid=%s\nnonce=%s\nsimulator=%s\nrunner_bundle_id=%s\nrunner_started=%s\nrunner_binary=%s\n' "$RUNNER_PID" "$NONCE" "$SIM_ID" "$RUNNER_BID" "$STARTED" "$RUNNER_BIN" > "$TRACE-runner-binding.txt"
ADMISSION="$DATA/Documents/captured-selection-trace-admission.txt"
test ! -e "$ADMISSION"
bash tools/ios-selection-context-trace.sh "$OUT" "$APP" "$SIM_ID" &
TRACE_PID=$!
START=$SECONDS
while [ ! -f "$TRACE.ready" ]; do
  kill -0 "$TRACE_PID" 2>/dev/null
  test "$((SECONDS-START))" -lt 60
  test "$(LC_ALL=C bounded_ps -p "$RUNNER_PID" -o lstart=)" = "$STARTED"
  sleep 0.1
done
test "$(LC_ALL=C bounded_ps -p "$RUNNER_PID" -o lstart=)" = "$STARTED"
test "$(bounded_ps -p "$RUNNER_PID" -o comm=)" = "$RUNNER_BIN"
cmp "$READY" "$TRACE-runner-expected.txt"
HOST_PID=$(sed -n 's/^pid=//p' "$TRACE-process-identity.txt")
[[ "$HOST_PID" =~ ^[1-9][0-9]*$ ]]
test "$HOST_PID" = "$HOST_READY_PID"
test "$(sed -n 's/^simulator=//p' "$TRACE-process-identity.txt")" = "$SIM_ID"
cat "$TRACE-process-identity.txt" "$TRACE-image-uuid.txt" "$TRACE-binary-hash.txt" > "$TRACE-admitted-host-identity.txt"
printf 'runner_pid=%s\nnonce=%s\nhost_pid=%s\n' "$RUNNER_PID" "$NONCE" "$HOST_PID" > "$TRACE-runner-admission.txt"
# Publish complete bytes once to only the verified runner's own container.
# Linking the completed sibling never exposes a partially written admission.
(set -o noclobber; cat "$TRACE-runner-admission.txt" > "$ADMISSION.pending")
ln "$ADMISSION.pending" "$ADMISSION"
rm "$ADMISSION.pending"
printf 'host_pid=%s\n' "$HOST_PID" > "$TRACE.continue-admitted.pending"
ln "$TRACE.continue-admitted.pending" "$TRACE.continue-admitted"
rm "$TRACE.continue-admitted.pending"
printf 'runner_identity_verified=true\nrunner_admission_published=true\n' >> "$TRACE-admission-status.txt"
TRACE_STATUS=0
wait "$TRACE_PID" || TRACE_STATUS=$?
TRACE_PID=''
test "$TRACE_STATUS" = 0
