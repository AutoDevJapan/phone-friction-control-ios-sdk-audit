#!/usr/bin/env bash
# Whole-flight deadline, independent of synchronous worker/checker/simctl waits.
set -euo pipefail
test "$#" = 3
OUT=$1
TRACE="$OUT/captured-selection-context"
WORKER_PID=''
WORKER_EXIT=1
DEADLINE=none
cleanup_flight() {
  STATUS=$?
  trap - EXIT
  if [ -n "$WORKER_PID" ]; then
    kill -TERM -- "-$WORKER_PID" 2>/dev/null || true
    GRACE=$SECONDS
    while kill -0 -- "-$WORKER_PID" 2>/dev/null && [ "$((SECONDS-GRACE))" -lt 2 ]; do sleep 0.1; done
    kill -KILL -- "-$WORKER_PID" 2>/dev/null || true
    GRACE=$SECONDS
    while kill -0 "$WORKER_PID" 2>/dev/null && [ "$((SECONDS-GRACE))" -lt 2 ]; do sleep 0.1; done
    REAPED=0
    if ! kill -0 "$WORKER_PID" 2>/dev/null; then wait "$WORKER_PID" 2>/dev/null || true; REAPED=1; fi
    printf 'worker_reaped=%s\n' "$REAPED" >> "$TRACE-status.txt"
    if [ "$REAPED" != 1 ]; then STATUS=1; fi
    GRACE=$SECONDS
    while kill -0 -- "-$WORKER_PID" 2>/dev/null && [ "$((SECONDS-GRACE))" -lt 2 ]; do sleep 0.1; done
    REMAINING=0
    if kill -0 -- "-$WORKER_PID" 2>/dev/null; then REMAINING=1; fi
    printf 'owned_flight_group_remaining=%s\n' "$REMAINING" >> "$TRACE-status.txt"
    if [ "$REMAINING" != 0 ]; then STATUS=1; fi
  fi
  # A KILL of a stuck worker cannot run its EXIT trap. Export and unlink here
  # only after the owned group has been terminated and its direct child reaped.
  if [ -f "$TRACE.unsanitized" ]; then
    sed -E 's/0x[0-9a-fA-F]+/[address]/g; s/(\$audit_context = )[0-9]+/\1[identity]/g' "$TRACE.unsanitized" > "$TRACE.txt"
    rm -f "$TRACE.unsanitized"
  fi
  if [ -p "$TRACE.control.fifo" ]; then rm -f "$TRACE.control.fifo"; fi
  printf 'whole_flight_deadline=%s\nworker_exit=%s\nsupervisor_exit=%s\n' "$DEADLINE" "$WORKER_EXIT" "$STATUS" >> "$TRACE-status.txt"
  exit "$STATUS"
}
trap cleanup_flight EXIT
trap 'exit 143' TERM
trap 'exit 130' INT
trap 'exit 129' HUP
test ! -e "$TRACE.attached"
START=$SECONDS
TRACE_START=''
# The supervisor remains outside this new session. No worker child starts a
# second session: LLDB, debugserver, checker and simctl inherit this owned PGID.
perl -MPOSIX -e 'POSIX::setsid() >= 0 or die "setsid failed\n"; exec @ARGV; die "exec failed\n"' bash tools/ios-selection-context-trace-worker.sh "$@" &
WORKER_PID=$!
printf 'owned_flight_group=%s\n' "$WORKER_PID" > "$TRACE-flight.txt"
while kill -0 "$WORKER_PID" 2>/dev/null; do
  if [ -z "$TRACE_START" ] && [ -f "$TRACE.attached" ]; then TRACE_START=$SECONDS; fi
  if [ -z "$TRACE_START" ]; then
    if [ "$((SECONDS-START))" -ge 60 ]; then DEADLINE=attach; exit 1; fi
  elif [ "$((SECONDS-TRACE_START))" -ge 90 ]; then
    DEADLINE=trace
    exit 1
  fi
  sleep 0.1
done
WORKER_EXIT=0
wait "$WORKER_PID" || WORKER_EXIT=$?
test "$WORKER_EXIT" = 0
