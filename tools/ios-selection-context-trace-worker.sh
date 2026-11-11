#!/usr/bin/env bash
# Private worker: the outer supervisor bounds this process and all descendants.
set -euo pipefail
test "$#" = 3
OUT=$1 APP=$2 SIM_ID=$3
BID=com.autodevjapan.phonecontrol1614abiprobe
TRACE="$OUT/captured-selection-context"
RAW="$TRACE.unsanitized"
FIFO="$TRACE.control.fifo"
LLDB_PID=''
CONTROL_OPEN=false
TRACE_STARTED=''
CONTINUES=0
CONTEXT_OPS=0
cleanup_trace() {
  STATUS=$?
  trap - EXIT
  if [ "$CONTROL_OPEN" = true ]; then exec 3>&-; exec 3<&-; fi
  if [ -n "$LLDB_PID" ]; then
    kill -TERM "$LLDB_PID" 2>/dev/null || true
    wait "$LLDB_PID" 2>/dev/null || true
  fi
  if [ -f "$RAW" ]; then
    sed -E 's/0x[0-9a-fA-F]+/[address]/g; s/(\$audit_context = )[0-9]+/\1[identity]/g' "$RAW" > "$TRACE.txt"
    rm -f "$RAW"
  fi
  if [ -p "$FIFO" ]; then rm -f "$FIFO"; fi
  printf 'continue_commands_sent=%s\ncontext_operation_stages_sent=%s\ntrace_driver_exit=%s\n' "$CONTINUES" "$CONTEXT_OPS" "$STATUS" >> "$TRACE-status.txt"
  exit "$STATUS"
}
trap cleanup_trace EXIT
trap 'exit 143' TERM
trap 'exit 130' INT
trap 'exit 129' HUP
no_errors() {
  local CODE=0
  grep -Eq '(^|[[:space:]])error:|failed to evaluate' "$RAW" || CODE=$?
  test "$CODE" = 1
}
await_marker() {
  local MARKER=$1 START=$ATTACH_STARTED LIMIT=60
  if [ -n "$TRACE_STARTED" ]; then START=$TRACE_STARTED; LIMIT=90; fi
  while [ ! -f "$MARKER" ]; do
    if ! kill -0 "$LLDB_PID" 2>/dev/null; then return 1; fi
    if [ "$(( $(date +%s) - START ))" -ge "$LIMIT" ]; then
      printf 'trace_deadline=%s\n' "$MARKER" >> "$TRACE-status.txt"
      return 1
    fi
    sleep 0.1
  done
  test "$(( $(date +%s) - START ))" -lt "$LIMIT"
}
send_continue() { CONTINUES=$((CONTINUES + 1)); printf 'continue\n' >&3; }
test -x "$APP/authorization-center-probe"
test ! -e "$FIFO"
for MARKER in attached context-stopped configured exited ready continue-admitted; do test ! -e "$TRACE.$MARKER"; done
printf 'diagnostic_only=true\nmaximum_events=64\nmaximum_frames=12\nattach_limit_seconds=60\ntrace_limit_seconds=90\nproduct_drain_accepted=false\n' > "$TRACE-status.txt"
perl -e 'alarm 10; exec @ARGV; die "exec failed\n"' xcrun lldb --version > "$TRACE-version.txt" 2>&1
perl -e 'alarm 10; exec @ARGV; die "exec failed\n"' xcrun lldb --no-lldbinit --batch -o 'help process attach' -o 'help breakpoint set' > "$TRACE-help.txt" 2>&1
grep -F -- '--pid' "$TRACE-help.txt"
grep -F -- '--ignore-count' "$TRACE-help.txt"
perl -e 'alarm 10; exec @ARGV; die "exec failed\n"' xcrun simctl help launch > "$TRACE-launch-help.txt" 2>&1
grep -F -- '--wait-for-debugger' "$TRACE-launch-help.txt"
shasum -a 256 "$APP/authorization-center-probe" > "$TRACE-binary-hash.txt"
INSTALLED=$(xcrun simctl get_app_container "$SIM_ID" "$BID" app)
INSTALLED_BIN="$INSTALLED/authorization-center-probe"
test -x "$INSTALLED_BIN"
test "$(shasum -a 256 "$INSTALLED_BIN" | awk '{print $1}')" = "$(shasum -a 256 "$APP/authorization-center-probe" | awk '{print $1}')"
printf 'simulator=%s\nbundle_id=%s\ninstalled_binary=%s\n' "$SIM_ID" "$BID" "$INSTALLED_BIN" > "$TRACE-owned-target.txt"
xcrun dwarfdump --uuid "$INSTALLED_BIN" > "$TRACE-image-uuid.txt"
UUID=$(awk '/^UUID: / {print $2}' "$TRACE-image-uuid.txt")
[[ "$UUID" =~ ^[A-Fa-f0-9-]{36}$ ]]
# Start this same owned generation suspended, before UIKit runs. The debugger
# sets the fourth-init breakpoint before its first admitted continue.
LAUNCH_STATUS=0
perl -e 'alarm 30; exec @ARGV; die "exec failed\n"' xcrun simctl launch --wait-for-debugger "$SIM_ID" "$BID" > "$TRACE-launch.txt" 2>&1 || LAUNCH_STATUS=$?
printf 'launch_wait_for_debugger=true\nlaunch_status=%s\n' "$LAUNCH_STATUS" >> "$TRACE-status.txt"
test "$LAUNCH_STATUS" = 0
PID=$(awk -v b="$BID" 'index($0,b ": ")==1 {p=substr($0,length(b)+3);if(p~/^[1-9][0-9]*$/)print p}' "$TRACE-launch.txt")
[[ "$PID" =~ ^[1-9][0-9]*$ ]]
xcrun simctl spawn "$SIM_ID" launchctl list > "$TRACE-owned-processes.txt"
test "$(awk -v p="$PID" -v b="$BID" '$1==p&&index($0,b)>0{n++}END{print n+0}' "$TRACE-owned-processes.txt")" = 1
STARTED=$(LC_ALL=C ps -p "$PID" -o lstart=)
test -n "$STARTED"
test "$(ps -p "$PID" -o comm=)" = "$INSTALLED_BIN"
printf 'pid=%s\nstarted=%s\nsimulator=%s\nbundle_id=%s\n' "$PID" "$STARTED" "$SIM_ID" "$BID" > "$TRACE-process-identity.txt"
mkfifo "$FIFO"
exec 3<> "$FIFO"
CONTROL_OPEN=true
ATTACH_STARTED=$(date +%s)
# Inherit the worker's dedicated process group, including debugserver children.
xcrun lldb --no-lldbinit < "$FIFO" > "$RAW" 2>&1 &
LLDB_PID=$!
{
  printf 'settings set stop-disassembly-count 0\nsettings set stop-line-count-before 0\nsettings set stop-line-count-after 0\n'
  printf 'settings set frame-format "frame #${frame.index}: ${module.file.basename} ${function.name}\\n"\n'
  printf 'settings set thread-format "thread #${thread.index}: ${thread.stop-reason}\\n"\n'
  printf 'process attach --pid %s\nprocess status\n' "$PID"
  printf 'target modules list --uuid --fullpath authorization-center-probe\n'
  printf 'breakpoint set --name rust_selection_capture_binding_init --ignore-count 3\n'
  # FIFO commands run asynchronously: only the real breakpoint stop may emit
  # its stack and completion marker. Never queue a backtrace after continue.
  printf 'breakpoint command add --script-type command --stop-on-error true 1\n'
  printf 'thread backtrace --count 4\nplatform shell touch "%s.context-stopped"\nDONE\n' "$TRACE"
  printf 'platform shell touch "%s.attached"\n' "$TRACE"
} >&3
await_marker "$TRACE.attached"
TRACE_STARTED=$(date +%s)
no_errors
# Real Bash admission: a failed identity check sends no continue/context command.
bash tools/check-selection-context-identity.sh "$OUT" --attached
await_marker "$TRACE.continue-admitted"
test "$(cat "$TRACE.continue-admitted")" = "host_pid=$PID"
bash tools/check-selection-context-identity.sh "$OUT" --attached
send_continue
await_marker "$TRACE.context-stopped"
no_errors
grep -Eq '^[[:space:]]*(\*[[:space:]]*)?frame #0: .* rust_selection_capture_binding_init([[:space:]]|$)' "$RAW"
CONTEXT_OPS=$((CONTEXT_OPS + 1))
{
  printf 'expression --language c -- unsigned long $audit_context = (unsigned long)$x5\n'
  printf 'expression --language c -- unsigned long $audit_events = 0\n'
  printf 'platform shell echo captured_context_identity_set=1\n'
  printf 'breakpoint delete 1\n'
  printf 'breakpoint set --name swift_retain --name swift_release --name swift_retain_n --name swift_release_n --condition "(unsigned long)$x0 == $audit_context && ++$audit_events <= 64"\n'
  printf 'breakpoint command add --script-type command 2\nthread backtrace --count 12\ncontinue\nDONE\n'
  printf 'breakpoint set --name exit --name _exit --name abort\n'
  printf 'breakpoint command add --script-type command --stop-on-error true 3\n'
  printf 'expression --language c -- $audit_events\nthread backtrace --count 4\n'
  printf 'platform shell echo context_trace_reached_exit=1\n'
  printf 'platform shell touch "%s.exited"\nDONE\n' "$TRACE"
  printf 'platform shell touch "%s.configured"\n' "$TRACE"
} >&3
await_marker "$TRACE.configured"
no_errors
send_continue
await_marker "$TRACE.exited"
no_errors
test "$(LC_ALL=C ps -p "$PID" -o lstart=)" = "$STARTED"
printf 'breakpoint delete 2 3\nprocess detach\nquit\n' >&3
while kill -0 "$LLDB_PID" 2>/dev/null; do
  test "$(( $(date +%s) - TRACE_STARTED ))" -lt 90
  sleep 0.1
done
LLDB_STATUS=0
wait "$LLDB_PID" || LLDB_STATUS=$?
printf 'lldb_exit=%s\n' "$LLDB_STATUS" >> "$TRACE-status.txt"
test "$LLDB_STATUS" = 0
no_errors
grep -F "$UUID" "$RAW" | sed -E 's/0x[0-9a-fA-F]+/[address]/g' > "$TRACE-attached-image.txt"
test "$(grep -Fc "$INSTALLED_BIN" "$TRACE-attached-image.txt")" = 1
test "$(grep -Ec '^captured_context_identity_set=1[[:space:]]*$' "$RAW")" = 1
test "$(grep -Ec '^context_trace_reached_exit=1[[:space:]]*$' "$RAW")" = 1
grep -Eq '^[[:space:]]*(\*[[:space:]]*)?frame #0: .* (exit|_exit|abort)([[:space:]]|$)' "$RAW"
EVENTS=$(grep -Ec '^[[:space:]]*(\*[[:space:]]*)?frame #0: .* (swift_retain|swift_release|swift_retain_n|swift_release_n)([[:space:]]|$)' "$RAW" || true)
[[ "$EVENTS" =~ ^[0-9]+$ ]]
test "$EVENTS" -ge 2
test "$EVENTS" -le 64
test "$(grep -Ec '^\(unsigned long\) \$audit_events = [0-9]+[[:space:]]*$' "$RAW")" = 1
MATCHED=$(sed -nE 's/^\(unsigned long\) \$audit_events = ([0-9]+)[[:space:]]*$/\1/p' "$RAW")
test "$MATCHED" = "$EVENTS"
grep -Eq '^[[:space:]]*(\*[[:space:]]*)?frame #0: .* swift_retain(_n)?([[:space:]]|$)' "$RAW"
grep -Eq '^[[:space:]]*(\*[[:space:]]*)?frame #0: .* swift_release(_n)?([[:space:]]|$)' "$RAW"
printf 'observed_context_events=%s\ncontext_arc_stack_capture_complete=true\ninlined_arc_not_observed=true\nproduct_drain_accepted=false\n' "$EVENTS" >> "$TRACE-status.txt"
