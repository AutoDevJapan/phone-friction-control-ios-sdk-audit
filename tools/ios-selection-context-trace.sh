#!/usr/bin/env bash
# Diagnostic only. Observe public Swift ARC entry calls in our own Simulator app.
set -euo pipefail
test "$#" = 3
OUT=$1
APP=$2
SIM_ID=$3
BID=com.autodevjapan.phonecontrol1614abiprobe
test -x "$APP/authorization-center-probe"
TRACE="$OUT/captured-selection-context"
COMMANDS="$TRACE.lldb"
RAW="$TRACE.unsanitized"
ATTACHED="$TRACE.attached"
READY="$TRACE.ready"
LLDB_PID=''
WATCH_PID=''
cleanup_trace() {
  STATUS=$?
  trap - EXIT
  if [ -n "$LLDB_PID" ]; then
    kill -TERM -- "-$LLDB_PID" 2>/dev/null || true
    sleep 1
    kill -KILL -- "-$LLDB_PID" 2>/dev/null || true
    wait "$LLDB_PID" 2>/dev/null || true
  fi
  if [ -n "$WATCH_PID" ]; then
    kill "$WATCH_PID" 2>/dev/null || true
    wait "$WATCH_PID" 2>/dev/null || true
  fi
  if [ -f "$RAW" ]; then
    # Frame formatting omits addresses and arguments. Redact remaining LLDB
    # diagnostics too; never export the context identity or register contents.
    sed -E 's/0x[0-9a-fA-F]+/[address]/g; s/(\$audit_context = )[0-9]+/\1[identity]/g' "$RAW" > "$TRACE.txt"
    rm -f "$RAW"
  fi
  printf 'trace_driver_exit=%s\n' "$STATUS" >> "$TRACE-status.txt"
  exit "$STATUS"
}
trap cleanup_trace EXIT
trap 'exit 143' TERM
trap 'exit 130' INT
trap 'exit 129' HUP
perl -e 'alarm 10; exec @ARGV; die "exec failed\n"' xcrun lldb --version > "$TRACE-version.txt" 2>&1
perl -e 'alarm 10; exec @ARGV; die "exec failed\n"' xcrun lldb --no-lldbinit --batch -o 'help process attach' -o 'help breakpoint set' -o 'help breakpoint command add' > "$TRACE-help.txt" 2>&1
grep -F -- '--pid' "$TRACE-help.txt"
grep -F -- '--ignore-count' "$TRACE-help.txt"
shasum -a 256 "$APP/authorization-center-probe" > "$TRACE-binary-hash.txt"
INSTALLED=$(xcrun simctl get_app_container "$SIM_ID" "$BID" app)
INSTALLED_BIN="$INSTALLED/authorization-center-probe"
test -x "$INSTALLED_BIN"
test "$(shasum -a 256 "$INSTALLED_BIN" | awk '{print $1}')" = "$(shasum -a 256 "$APP/authorization-center-probe" | awk '{print $1}')"
printf 'simulator=%s\nbundle_id=%s\ninstalled_binary=%s\n' "$SIM_ID" "$BID" "$INSTALLED_BIN" > "$TRACE-owned-target.txt"
xcrun dwarfdump --uuid "$INSTALLED_BIN" > "$TRACE-image-uuid.txt"
UUID=$(awk '/^UUID: / {print $2}' "$TRACE-image-uuid.txt")
[[ "$UUID" =~ ^[A-Fa-f0-9-]{36}$ ]]
LAUNCH_STATUS=0
perl -e 'alarm 30; exec @ARGV; die "exec failed\n"' xcrun simctl launch "$SIM_ID" "$BID" > "$TRACE-launch.txt" 2>&1 || LAUNCH_STATUS=$?
test "$LAUNCH_STATUS" = 0
PID=$(awk -v b="$BID" 'index($0,b ": ")==1 {p=substr($0,length(b)+3);if(p~/^[1-9][0-9]*$/)print p}' "$TRACE-launch.txt")
[[ "$PID" =~ ^[1-9][0-9]*$ ]]
xcrun simctl spawn "$SIM_ID" launchctl list > "$TRACE-owned-processes.txt"
test "$(awk -v p="$PID" -v b="$BID" '$1==p&&index($0,b)>0{n++}END{print n+0}' "$TRACE-owned-processes.txt")" = 1
STARTED=$(LC_ALL=C ps -p "$PID" -o lstart=)
test -n "$STARTED"
COMMAND=$(ps -p "$PID" -o comm=)
test "$COMMAND" = "$INSTALLED_BIN"
printf 'pid=%s\nstarted=%s\nsimulator=%s\nbundle_id=%s\n' "$PID" "$STARTED" "$SIM_ID" "$BID" > "$TRACE-process-identity.txt"
{
  printf 'settings set stop-disassembly-count 0\n'
  printf 'settings set stop-line-count-before 0\n'
  printf 'settings set stop-line-count-after 0\n'
  printf 'settings set frame-format "frame #${frame.index}: ${module.file.basename} ${function.name}\\n"\n'
  printf 'settings set thread-format "thread #${thread.index}: ${thread.stop-reason}\\n"\n'
  printf 'process attach --pid %s\n' "$PID"
  printf 'process status\n'
  printf 'target modules list --uuid --fullpath authorization-center-probe\n'
  printf 'platform shell LC_ALL=C ps -p %s -o lstart= > "%s"\n' "$PID" "$TRACE-attached-start.txt"
  printf 'platform shell ps -p %s -o comm= > "%s"\n' "$PID" "$TRACE-attached-command.txt"
  printf 'platform shell touch "%s"\n' "$ATTACHED"
  # Three independently measured preflights precede the genuine UI context.
  # The bridge's sixth C argument x5 is that exact context; no heap scan occurs.
  printf 'breakpoint set --name rust_selection_capture_binding_init --ignore-count 3\n'
  printf 'platform shell bash "%s/tools/check-selection-context-identity.sh" "%s" --attached\n' "$PWD" "$OUT"
  printf 'continue\n'
  printf 'thread backtrace --count 4\n'
  printf 'expression --language c -- unsigned long $audit_context = (unsigned long)$x5\n'
  printf 'expression --language c -- unsigned long $audit_events = 0\n'
  printf 'platform shell echo captured_context_identity_set=1\n'
  printf 'breakpoint delete 1\n'
  printf 'breakpoint set --name swift_retain --name swift_release --name swift_retain_n --name swift_release_n --condition "(unsigned long)$x0 == $audit_context && ++$audit_events <= 64"\n'
  # At most 64 matching stops, twelve symbol-only frames per stop. Calls that
  # use inlined refcount operations are outside this observation's coverage.
  printf 'breakpoint command add --script-type command 2\n'
  printf 'thread backtrace --count 12\ncontinue\nDONE\n'
  printf 'breakpoint set --name exit --name _exit --name abort\n'
  printf 'continue\n'
  printf 'expression --language c -- $audit_events\n'
  printf 'thread backtrace --count 4\n'
  printf 'platform shell echo context_trace_reached_exit=1\n'
  printf 'breakpoint delete 2 3\nprocess detach\nquit\n'
} > "$COMMANDS"
printf 'diagnostic_only=true\nmaximum_events=64\nmaximum_frames=12\nattach_limit_seconds=60\ntrace_limit_seconds=90\nproduct_drain_accepted=false\n' > "$TRACE-status.txt"
# A separate process group bounds LLDB and any debugserver it creates. All
# processes in this group belong to this exact diagnostic invocation.
perl -MPOSIX -e 'POSIX::setsid() >= 0 or die "setsid failed\n"; exec @ARGV; die "exec failed\n"' \
  xcrun lldb --no-lldbinit --batch -s "$COMMANDS" > "$RAW" 2>&1 &
LLDB_PID=$!
(
  for ((WAIT=0; WAIT<60; WAIT++)); do
    if [ -f "$ATTACHED" ]; then break; fi
    if ! kill -0 "$LLDB_PID" 2>/dev/null; then exit 0; fi
    sleep 1
  done
  if [ ! -f "$ATTACHED" ]; then
    printf 'trace_deadline=attach\n' >> "$TRACE-status.txt"
  else
    for ((WAIT=0; WAIT<90; WAIT++)); do
      if ! kill -0 "$LLDB_PID" 2>/dev/null; then exit 0; fi
      CURRENT_START=$(LC_ALL=C ps -p "$PID" -o lstart= 2>/dev/null || true)
      if [ -n "$CURRENT_START" ] && [ "$CURRENT_START" != "$STARTED" ]; then
        printf 'trace_identity_changed=true\n' >> "$TRACE-status.txt"
        break
      fi
      sleep 1
    done
    printf 'trace_deadline=events\n' >> "$TRACE-status.txt"
  fi
  kill -TERM -- "-$LLDB_PID" 2>/dev/null || true
  sleep 1
  kill -KILL -- "-$LLDB_PID" 2>/dev/null || true
) &
WATCH_PID=$!
LLDB_STATUS=0
wait "$LLDB_PID" || LLDB_STATUS=$?
kill -TERM -- "-$LLDB_PID" 2>/dev/null || true
sleep 1
kill -KILL -- "-$LLDB_PID" 2>/dev/null || true
LLDB_PID=''
kill "$WATCH_PID" 2>/dev/null || true
wait "$WATCH_PID" 2>/dev/null || true
WATCH_PID=''
printf 'lldb_exit=%s\n' "$LLDB_STATUS" >> "$TRACE-status.txt"
test -f "$ATTACHED"
test "$LLDB_STATUS" = 0
if grep -q '^trace_deadline=' "$TRACE-status.txt"; then exit 1; fi
if grep -q '^trace_identity_changed=' "$TRACE-status.txt"; then exit 1; fi
if grep -Eq '(^|\n)error:|error: evaluating condition|failed to evaluate' "$RAW"; then exit 1; fi
grep -Fq "$UUID" "$RAW"
grep -Eq "^Process $PID (stopped|is stopped)" "$RAW"
grep -Fq "$INSTALLED_BIN" "$RAW"
grep -F "$UUID" "$RAW" | sed -E 's/0x[0-9a-fA-F]+/[address]/g' > "$TRACE-attached-image.txt"
test "$(grep -Fc "$INSTALLED_BIN" "$TRACE-attached-image.txt")" = 1
test "$(cat "$TRACE-attached-start.txt")" = "$STARTED"
test "$(cat "$TRACE-attached-command.txt")" = "$INSTALLED_BIN"
test "$(grep -Ec '^captured_context_identity_set=1$' "$RAW")" = 1
test "$(grep -Ec '^context_trace_reached_exit=1$' "$RAW")" = 1
grep -Eq '^ *frame #0: .* rust_selection_capture_binding_init( |$)' "$RAW"
grep -Eq '^ *frame #0: .* (exit|_exit|abort)( |$)' "$RAW"
EVENTS=$(grep -Ec '^ *frame #0: .* (swift_retain|swift_release|swift_retain_n|swift_release_n)( |$)' "$RAW" || true)
[[ "$EVENTS" =~ ^[0-9]+$ ]]
test "$EVENTS" -ge 2
test "$EVENTS" -le 64
# The persistent debugger counter increments on every matching call, including
# calls beyond the output cap. A capped/incomplete trace must not pass.
MATCHED=$(sed -nE 's/^\(unsigned long\) \$audit_events = ([0-9]+)$/\1/p' "$RAW" | tail -n 1)
test "$MATCHED" = "$EVENTS"
grep -Eq '^ *frame #0: .* swift_retain(_n)?( |$)' "$RAW"
grep -Eq '^ *frame #0: .* swift_release(_n)?( |$)' "$RAW"
printf 'observed_context_events=%s\ncontext_arc_stack_capture_complete=true\ninlined_arc_not_observed=true\nproduct_drain_accepted=false\n' "$EVENTS" >> "$TRACE-status.txt"
