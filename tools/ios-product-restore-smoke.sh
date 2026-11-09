#!/bin/bash
set -euo pipefail
export DEVELOPER_DIR=/Applications/Xcode_16.4.app/Contents/Developer
OUT="$RUNNER_TEMP/product-restore"
mkdir -p "$OUT"
SOURCE="$RUNNER_TEMP/verified-product"
test "$(shasum -a 256 "$SOURCE/Product.app.tar.gz" | awk '{print $1}')" = 446bb9e35592554ca217502039e42f71313f65d8e846aa3820246fc478c67ed3
tar -xzf "$SOURCE/Product.app.tar.gz" -C "$OUT"
test "$(shasum -a 256 "$OUT/Product.app/product" | awk '{print $1}')" = 6fb4cc86dd22f495144f79a12af2875eeb05a96072dd1c06945c0ef2171c8d45
test "$(shasum -a 256 "$SOURCE/effective-Cargo.lock" | awk '{print $1}')" = fd4e3e9fb7974318ad53519754ea619c92fe240c405a5ecafab5524788da9291
cp "$SOURCE/build-hashes.txt" "$OUT/source-build-hashes.txt"
xcodebuild -version > "$OUT/versions.txt"
xcrun simctl list runtimes -j > "$OUT/runtimes.txt"
RUNTIME=$(jq -er '[.runtimes[] | select(.isAvailable and .version=="18.5" and (.identifier|contains("iOS")))|.identifier] | if length==1 then .[0] else error("runtime unavailable") end' "$OUT/runtimes.txt")
SIM_ID=$(xcrun simctl create product-restore com.apple.CoreSimulator.SimDeviceType.iPhone-15 "$RUNTIME")
trap 'xcrun simctl shutdown "$SIM_ID" || true; xcrun simctl delete "$SIM_ID" || true' EXIT
xcrun simctl boot "$SIM_ID"
xcrun simctl bootstatus "$SIM_ID" -b
xcrun simctl install "$SIM_ID" "$OUT/Product.app"
BID=com.autodevjapan.phonefrictioncontrol.simulator
DATA=$(xcrun simctl get_app_container "$SIM_ID" "$BID" data)
STATE="$DATA/Documents/usage-state.ron"
test ! -e "$STATE"
printf 'initial_state_missing=true\nsource_run=42624956614\nrecompiled=false\n' > "$OUT/result.txt"
for pass in first restarted; do
  perl -e 'alarm 45; exec @ARGV; die "exec failed\n"' xcrun simctl launch --stdout="$OUT/$pass-stdout.txt" --stderr="$OUT/$pass-stderr.txt" "$SIM_ID" "$BID" > "$OUT/$pass-launch.txt" 2>&1
  PID=$(awk -v b="$BID" 'index($0,b ": ")==1 {p=substr($0,length(b)+3); if(p ~ /^[0-9]+$/) print p}' "$OUT/$pass-launch.txt")
  [[ "$PID" =~ ^[0-9]+$ ]]
  sleep 20
  xcrun simctl spawn "$SIM_ID" launchctl list > "$OUT/$pass-processes.txt"
  awk -v p="$PID" '$1==p' "$OUT/$pass-processes.txt" > "$OUT/$pass-alive.txt"
  test "$(wc -l < "$OUT/$pass-alive.txt" | tr -d ' ')" = 1
  xcrun simctl io "$SIM_ID" screenshot "$OUT/$pass.png"
  test -s "$STATE"
  HASH=$(shasum -a 256 "$STATE" | awk '{print $1}')
  VALUE=$(sed -nE 's/.*daily_limit_minutes:[[:space:]]*([0-9]+).*/\1/p' "$STATE")
  [[ "$VALUE" =~ ^[0-9]+$ ]]
  printf '%s_pid=%s\n%s_state_hash=%s\n' "$pass" "$PID" "$pass" "$HASH" >> "$OUT/result.txt"
  if [ "$pass" = first ]; then
    FIRST_PID=$PID
    BEFORE_VALUE=$VALUE
    export OUT SIM_ID
    source tools/ios-product-ui-change.sh
    sleep 2
    VALUE=$(sed -nE 's/.*daily_limit_minutes:[[:space:]]*([0-9]+).*/\1/p' "$STATE")
    [[ "$VALUE" =~ ^[0-9]+$ ]]
    printf 'before_limit_minutes=%s\nafter_ui_limit_minutes=%s\n' "$BEFORE_VALUE" "$VALUE" >> "$OUT/result.txt"
    test "$VALUE" != "$BEFORE_VALUE"
    test "$VALUE" -ge 30
    test "$VALUE" -le 360
    test "$((VALUE % 30))" = 0
    FIRST_VALUE=$VALUE
    FIRST_HASH=$(shasum -a 256 "$STATE" | awk '{print $1}')
  else
    test "$PID" != "$FIRST_PID"
    test "$HASH" = "$FIRST_HASH"
    test "$VALUE" = "$FIRST_VALUE"
    printf 'restarted_limit_minutes=%s\n' "$VALUE" >> "$OUT/result.txt"
  fi
  xcrun simctl terminate "$SIM_ID" "$BID" || true
done
printf 'app_created_state=true\nstate_survived_process_restart=true\nsettings_ui_mutation_verified=true\nnondefault_value_restore_verified=true\nscreentime_control_verified=false\n' >> "$OUT/result.txt"
