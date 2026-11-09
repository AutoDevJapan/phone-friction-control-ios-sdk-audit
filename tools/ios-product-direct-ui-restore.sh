#!/bin/bash
set -euo pipefail
export DEVELOPER_DIR=/Applications/Xcode_16.4.app/Contents/Developer
OUT="$RUNNER_TEMP/product-restore"
SOURCE="$RUNNER_TEMP/verified-product"
mkdir -p "$OUT"
test "$(shasum -a 256 "$SOURCE/Product.app.tar.gz" | awk '{print $1}')" = 1091943ddf6c215c774da1267671db72cd4ae21879212430db0958d8b2f16575
tar -xzf "$SOURCE/Product.app.tar.gz" -C "$OUT"
test "$(shasum -a 256 "$OUT/Product.app/product" | awk '{print $1}')" = 231c96d8335abd518bbd86c04b34815cd36155d30b81f6fddfe6fc2f1bd204d0
test "$(shasum -a 256 "$SOURCE/effective-Cargo.lock" | awk '{print $1}')" = ce89b963e268a1dd2b724be1c1a04d54a58864ed1c8db31d05788301c680b661
cp "$SOURCE/build-hashes.txt" "$OUT/source-build-hashes.txt"
cp "$SOURCE/product-source.txt" "$OUT/product-source.txt"
xcodebuild -version > "$OUT/versions.txt"
RUNTIME=$(xcrun simctl list runtimes -j | jq -er '[.runtimes[]|select(.isAvailable and .version=="18.5" and (.identifier|contains("iOS")))|.identifier]|if length==1 then .[0] else error("runtime unavailable") end')
SIM_ID=$(xcrun simctl create product-ui-restore com.apple.CoreSimulator.SimDeviceType.iPhone-15 "$RUNTIME")
trap 'xcrun simctl shutdown "$SIM_ID" || true; xcrun simctl delete "$SIM_ID" || true' EXIT
xcrun simctl boot "$SIM_ID"
xcrun simctl bootstatus "$SIM_ID" -b
xcrun simctl install "$SIM_ID" "$OUT/Product.app"
BID=com.autodevjapan.phonefrictioncontrol.simulator
DATA=$(xcrun simctl get_app_container "$SIM_ID" "$BID" data)
STATE="$DATA/Documents/usage-state.ron"
test ! -e "$STATE"
printf 'initial_state_missing=true\nsource_product_commit=ebb79b4f58898a5557e8806dfa66d55a0820c297\nsource_run=42630176629\nrecompiled=false\n' > "$OUT/result.txt"
export OUT SIM_ID
source tools/ios-product-ui-change.sh
test -s "$STATE"
VALUE=$(sed -nE 's/.*daily_limit_minutes:[[:space:]]*([0-9]+).*/\1/p' "$STATE")
[[ "$VALUE" =~ ^[0-9]+$ ]]
printf 'saved_limit_minutes=%s\n' "$VALUE" >> "$OUT/result.txt"
test "$VALUE" != 120
test "$VALUE" -ge 30
test "$VALUE" -le 360
test "$((VALUE % 30))" = 0
grep -E 'PRODUCT_(FIRST|RESTARTED)_PID=' "$OUT/ui-test.txt" > "$OUT/process-observations.txt"
printf 'nondefault_state_written=true\nxctest_restart_and_20s_foreground_passed=true\nrestored_ui_visual_review=pending\nscreentime_control_verified=false\n' >> "$OUT/result.txt"
