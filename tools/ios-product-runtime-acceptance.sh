set -euo pipefail
export DEVELOPER_DIR=/Applications/Xcode_16.4.app/Contents/Developer
OUT="$RUNNER_TEMP/app-simulator"
mkdir -p "$OUT/Product.app"
SIM_ID=''
BID=com.autodevjapan.phonefrictioncontrol.runtime-test
cleanup() {
  STATUS=$?
  trap - EXIT
  printf 'step_exit_status=%s\n' "$STATUS" >> "$OUT/cleanup.txt"
  if [ -n "$SIM_ID" ]; then
    DIAGNOSTIC_STATUS=0
    perl -e 'alarm 30; exec @ARGV; die "exec failed\n"' xcrun simctl spawn "$SIM_ID" log show --last 5m --style compact --info --predicate 'process == "product" OR eventMessage CONTAINS "phonefrictioncontrol.runtime-test"' > "$OUT/log-full.txt" 2>&1 || DIAGNOSTIC_STATUS=$?
    printf 'system_log_status=%s\n' "$DIAGNOSTIC_STATUS" >> "$OUT/cleanup.txt"
    if [ -n "${STATE:-}" ] && [ -f "$STATE" ]; then
      COPY_STATUS=0
      cp "$STATE" "$OUT/partial-state.txt" || COPY_STATUS=$?
      printf 'partial_state_copy_status=%s\n' "$COPY_STATUS" >> "$OUT/cleanup.txt"
    fi
    for ACTION in terminate shutdown delete; do
      CLEANUP_STATUS=0
      if [ "$ACTION" = terminate ]; then
        perl -e 'alarm 20; exec @ARGV; die "exec failed\n"' xcrun simctl terminate "$SIM_ID" "$BID" >> "$OUT/cleanup.txt" 2>&1 || CLEANUP_STATUS=$?
      else
        perl -e 'alarm 20; exec @ARGV; die "exec failed\n"' xcrun simctl "$ACTION" "$SIM_ID" >> "$OUT/cleanup.txt" 2>&1 || CLEANUP_STATUS=$?
      fi
      printf '%s_status=%s\n' "$ACTION" "$CLEANUP_STATUS" >> "$OUT/cleanup.txt"
    done
  fi
  exit "$STATUS"
}
trap cleanup EXIT
test "$(uname -m)" = arm64
test "$(git -C app rev-parse HEAD)" = 3d6e7a9278e7dcf43758588745953b5e1a1ed268
git -C app rev-parse HEAD > "$OUT/product-source.txt"
git rev-parse HEAD > "$OUT/audit-source.txt"
printf 'entry=phone-friction-control-ios-runtime\nfeature=ios-runtime\npatches_applied=false\ndiagnostic_logger=false\nindividual_app_selection_verified=false\nmonitoring_verified=false\nshielding_verified=false\n' > "$OUT/product-scope.txt"
xcodebuild -version > "$OUT/versions.txt"
rustc -vV >> "$OUT/versions.txt"
test "$(xcrun --sdk iphonesimulator --show-sdk-version)" = 18.5
xcrun --sdk iphonesimulator --show-sdk-version >> "$OUT/versions.txt"
(cd app && perl -e 'alarm 720; exec @ARGV; die "exec failed\n"' cargo build --locked --release --bin phone-friction-control-ios-runtime --features ios-runtime --target aarch64-apple-ios-sim) > "$OUT/build-full.txt" 2>&1
tail -n 40 "$OUT/build-full.txt"
cp app/target/aarch64-apple-ios-sim/release/phone-friction-control-ios-runtime "$OUT/Product.app/product"
printf '<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>com.autodevjapan.phonefrictioncontrol.runtime-test</string><key>CFBundleName</key><string>PhoneFrictionControl</string><key>CFBundleExecutable</key><string>product</string><key>CFBundlePackageType</key><string>APPL</string><key>CFBundleVersion</key><string>1</string><key>CFBundleShortVersionString</key><string>0.1.0</string><key>MinimumOSVersion</key><string>18.5</string><key>UIDeviceFamily</key><array><integer>1</integer></array><key>UILaunchScreen</key><dict/><key>CFBundleSupportedPlatforms</key><array><string>iPhoneSimulator</string></array></dict></plist>\n' > "$OUT/Product.app/Info.plist"
codesign --sign - "$OUT/Product.app"
codesign --verify --strict "$OUT/Product.app" > "$OUT/signature-verify.txt" 2>&1
codesign --display --entitlements :- "$OUT/Product.app" > "$OUT/entitlements.txt" 2> "$OUT/entitlements-status.txt"
shasum -a 256 "$OUT/Product.app/product" app/Cargo.lock > "$OUT/build-hashes.txt"
test "$(shasum -a 256 app/Cargo.lock | awk '{print $1}')" = ce89b963e268a1dd2b724be1c1a04d54a58864ed1c8db31d05788301c680b661
cp app/Cargo.lock "$OUT/effective-Cargo.lock"
tar -czf "$OUT/Product.app.tar.gz" -C "$OUT" Product.app
shasum -a 256 "$OUT/Product.app.tar.gz" > "$OUT/archive-hash.txt"
shasum -a 256 tools/ProductPersistenceTests.m tools/ios-product-ui-change.sh > "$OUT/test-source-hashes.txt"
xcrun simctl list runtimes -j > "$OUT/runtimes.txt"
RUNTIME=$(jq -er '[.runtimes[] | select(.isAvailable and .version == "18.5" and (.identifier | contains("iOS"))) | .identifier] | if length == 1 then .[0] else error("runtime unavailable") end' "$OUT/runtimes.txt")
SIM_ID=$(xcrun simctl create runtime-product-audit com.apple.CoreSimulator.SimDeviceType.iPhone-15 "$RUNTIME")
printf 'simulator=%s\nbundle_id=%s\n' "$SIM_ID" "$BID" > "$OUT/target.txt"
xcrun simctl boot "$SIM_ID"
perl -e 'alarm 120; exec @ARGV; die "exec failed\n"' xcrun simctl bootstatus "$SIM_ID" -b > "$OUT/boot.txt" 2>&1
xcrun simctl install "$SIM_ID" "$OUT/Product.app"
DATA=$(xcrun simctl get_app_container "$SIM_ID" "$BID" data)
case "$DATA" in /*/data/Containers/Data/Application/*) ;; *) echo 'Unexpected app container path'; exit 1;; esac
STATE="$DATA/Documents/usage-state.ron"
PREVIEW_STATE="$DATA/Documents/preview-usage-state.ron"
test ! -e "$STATE"
test ! -e "$PREVIEW_STATE"
export SIMCTL_CHILD_RUST_BACKTRACE=full
launch_and_observe() {
  PASS=$1
  LAUNCH_STATUS=0
  perl -e 'alarm 45; exec @ARGV; die "exec failed\n"' xcrun simctl launch --stdout="$OUT/$PASS-stdout.txt" --stderr="$OUT/$PASS-stderr.txt" "$SIM_ID" "$BID" > "$OUT/$PASS-launch.txt" 2>&1 || LAUNCH_STATUS=$?
  printf 'launch_status=%s\n' "$LAUNCH_STATUS" > "$OUT/$PASS-launch-status.txt"
  PID=$(awk -v b="$BID" 'index($0,b ": ")==1 {p=substr($0,length(b)+3); if(p ~ /^[0-9]+$/) print p}' "$OUT/$PASS-launch.txt")
  printf 'pid=%s\n' "$PID" > "$OUT/$PASS-pid.txt"
  test "$LAUNCH_STATUS" = 0
  [[ "$PID" =~ ^[1-9][0-9]*$ ]]
  sleep 3
  xcrun simctl io "$SIM_ID" screenshot "$OUT/$PASS-early.png"
  sleep 17
  xcrun simctl spawn "$SIM_ID" launchctl list > "$OUT/$PASS-processes.txt"
  awk -v p="$PID" -v b="$BID" '$1==p && index($0,b)>0' "$OUT/$PASS-processes.txt" > "$OUT/$PASS-same-pid-alive.txt"
  test "$(wc -l < "$OUT/$PASS-same-pid-alive.txt" | tr -d ' ')" = 1
  xcrun simctl io "$SIM_ID" screenshot "$OUT/$PASS.png"
  test -s "$STATE"
  test ! -e "$PREVIEW_STATE"
  cp "$STATE" "$OUT/$PASS-state.txt"
  VALUE=$(sed -nE 's/.*daily_limit_minutes:[[:space:]]*([0-9]+).*/\1/p' "$STATE")
  [[ "$VALUE" =~ ^[0-9]+$ ]]
  printf 'daily_limit_minutes=%s\n' "$VALUE" > "$OUT/$PASS-value.txt"
  test -f "$OUT/$PASS-stdout.txt"
  test -f "$OUT/$PASS-stderr.txt"
  ERROR_STATUS=0
  grep -iE 'ERROR|panicked|uncaught exception|fatal error' "$OUT/$PASS-stdout.txt" "$OUT/$PASS-stderr.txt" > "$OUT/$PASS-errors.txt" || ERROR_STATUS=$?
  test "$ERROR_STATUS" = 1
}
launch_and_observe first
FIRST_PID=$PID
BEFORE_VALUE=$VALUE
test "$BEFORE_VALUE" = 120
xcrun simctl terminate "$SIM_ID" "$BID"
# Reuse the existing public XCTest with only the disposable target ID changed.
# Product source and the signed product bytes are not patched.
TEST_SOURCE="$RUNNER_TEMP/runtime-ui-source"
mkdir -p "$TEST_SOURCE/tools"
sed 's/com.autodevjapan.phonefrictioncontrol.simulator/com.autodevjapan.phonefrictioncontrol.runtime-test/g' tools/ProductPersistenceTests.m > "$TEST_SOURCE/tools/ProductPersistenceTests.m"
cp tools/ios-product-ui-change.sh "$TEST_SOURCE/tools/ios-product-ui-change.sh"
shasum -a 256 "$TEST_SOURCE/tools/ProductPersistenceTests.m" "$TEST_SOURCE/tools/ios-product-ui-change.sh" >> "$OUT/test-source-hashes.txt"
diff -u tools/ProductPersistenceTests.m "$TEST_SOURCE/tools/ProductPersistenceTests.m" > "$OUT/test-bundle-id-delta.txt" || test "$?" = 1
export OUT SIM_ID
(cd "$TEST_SOURCE" && bash tools/ios-product-ui-change.sh)
test -s "$STATE"
cp "$STATE" "$OUT/after-ui-state.txt"
AFTER_VALUE=$(sed -nE 's/.*daily_limit_minutes:[[:space:]]*([0-9]+).*/\1/p' "$STATE")
[[ "$AFTER_VALUE" =~ ^[0-9]+$ ]]
test "$AFTER_VALUE" != "$BEFORE_VALUE"
test "$AFTER_VALUE" -ge 30
test "$AFTER_VALUE" -le 360
test "$((AFTER_VALUE % 30))" = 0
UI_FIRST_PID=$(sed -nE 's/.*PRODUCT_FIRST_PID=([0-9]+).*/\1/p' "$OUT/ui-test.txt")
UI_RESTARTED_PID=$(sed -nE 's/.*PRODUCT_RESTARTED_PID=([0-9]+).*/\1/p' "$OUT/ui-test.txt")
[[ "$UI_FIRST_PID" =~ ^[1-9][0-9]*$ ]]
[[ "$UI_RESTARTED_PID" =~ ^[1-9][0-9]*$ ]]
test "$UI_FIRST_PID" != "$UI_RESTARTED_PID"
AFTER_HASH=$(shasum -a 256 "$STATE" | awk '{print $1}')
xcrun simctl terminate "$SIM_ID" "$BID" > "$OUT/ui-terminate.txt" 2>&1 || true
launch_and_observe restarted
test "$PID" != "$FIRST_PID"
test "$PID" != "$UI_RESTARTED_PID"
test "$VALUE" = "$AFTER_VALUE"
test "$(shasum -a 256 "$STATE" | awk '{print $1}')" = "$AFTER_HASH"
cmp "$OUT/after-ui-state.txt" "$OUT/restarted-state.txt"
printf 'runtime_entry_executed=true\nfirst_pid=%s\nui_first_pid=%s\nui_restarted_pid=%s\nfinal_restarted_pid=%s\nbefore_limit_minutes=%s\nafter_ui_limit_minutes=%s\nrestored_limit_minutes=%s\nfirst_same_pid_after_20_seconds=true\nrestarted_same_pid_after_20_seconds=true\napp_created_runtime_documents=true\npreview_state_absent=true\nnondefault_state_survived_process_restart=true\nproduct_ui_visual_review=pending\nindividual_app_selection_verified=false\nmonitoring_verified=false\nshielding_verified=false\nfull_teardown_verified=false\n' "$FIRST_PID" "$UI_FIRST_PID" "$UI_RESTARTED_PID" "$PID" "$BEFORE_VALUE" "$AFTER_VALUE" "$VALUE" > "$OUT/result.txt"
