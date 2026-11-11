set -euo pipefail
SIM_ID=$1
OUT="$RUNNER_TEMP/ios-sdk-audit"
TRACE_PID=''
UI_TEST_PID=''
UI_TEST_GROUP=false
HOST_LOG_PID=''
stop_owned_ui() {
  if [ -z "$UI_TEST_PID" ]; then return; fi
  if [ "$UI_TEST_GROUP" = true ]; then
    # The owner retains unreaped child/group identity. Bash submits
    # a request only; it never signals a recyclable numeric ID.
    printf 'stop=true\n' > "$OUT/captured-selection-context-xctest-stop.txt"
    GRACE=$SECONDS
    while kill -0 "$UI_TEST_PID" 2>/dev/null && [ "$((SECONDS-GRACE))" -lt 10 ]; do sleep 0.1; done
    if kill -0 "$UI_TEST_PID" 2>/dev/null; then return 1; fi
  fi
  wait "$UI_TEST_PID" 2>/dev/null || true
  UI_TEST_PID=''
}
cleanup_observers() {
  STATUS=$?
  trap - EXIT
  if [ -n "$HOST_LOG_PID" ]; then kill "$HOST_LOG_PID" 2>/dev/null || true; wait "$HOST_LOG_PID" 2>/dev/null || true; fi
  if [ -n "$TRACE_PID" ]; then
    kill "$TRACE_PID" 2>/dev/null || true
    wait "$TRACE_PID" 2>/dev/null || true
  fi
  if ! stop_owned_ui; then STATUS=1; fi
  exit "$STATUS"
}
trap cleanup_observers EXIT
if [ "$INPUT_SELECTION_INSPECTION_MODE" = consent-selection-acceptance ] && [ "$INPUT_CATEGORY_SELECTION_OBSERVATION" != true ]; then
  # Finite official fixture acquisition: built-in Messages in this NEW
  # Simulator only. No third-party app download/account/playback.
  # A separate ordinary app bundle, with no Family Controls entitlement.
  # Reuse our built Rust/UIKit executable; do not request permission
  # in this fixture or manufacture activity tokens.
  FIXTURE_APP="$OUT/PhoneControlFixture.app"
  cp -R "$PROBE_APP" "$FIXTURE_APP"
  /usr/libexec/PlistBuddy -c 'Set :CFBundleIdentifier com.autodevjapan.phonecontrolfixture' "$FIXTURE_APP/Info.plist"
  /usr/libexec/PlistBuddy -c 'Set :CFBundleName Phone Control Fixture' "$FIXTURE_APP/Info.plist"
  /usr/libexec/PlistBuddy -c 'Add :CFBundleDisplayName string Phone Control Fixture' "$FIXTURE_APP/Info.plist"
  printf '<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict/></plist>\n' > "$RUNNER_TEMP/fixture-entitlements.plist"
  codesign --force --sign - --entitlements "$RUNNER_TEMP/fixture-entitlements.plist" "$FIXTURE_APP" > "$OUT/fixture-signing.txt" 2>&1
  codesign --verify --strict "$FIXTURE_APP" > "$OUT/fixture-signature-verify.txt" 2>&1
  codesign --display --entitlements :- "$FIXTURE_APP" > "$OUT/fixture-signed-entitlements.plist" 2> "$OUT/fixture-signature-readback.txt"
  plutil -convert json -o "$OUT/fixture-signed-entitlements.json" "$OUT/fixture-signed-entitlements.plist"
  jq -e 'type == "object" and length == 0' "$OUT/fixture-signed-entitlements.json"
  /usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$FIXTURE_APP/Info.plist" > "$OUT/fixture-bundle-id.txt"
  /usr/libexec/PlistBuddy -c 'Print :CFBundleDisplayName' "$FIXTURE_APP/Info.plist" > "$OUT/fixture-display-name.txt"
  grep -Fx 'com.autodevjapan.phonecontrolfixture' "$OUT/fixture-bundle-id.txt"
  grep -Fx 'Phone Control Fixture' "$OUT/fixture-display-name.txt"
  xcrun simctl install "$SIM_ID" "$FIXTURE_APP"
  SIMCTL_CHILD_PHONE_CONTROL_ORDINARY_FIXTURE=1 xcrun simctl launch "$SIM_ID" com.autodevjapan.phonecontrolfixture > "$OUT/fixture-launch.txt"
  sleep 20
  xcrun simctl terminate "$SIM_ID" com.autodevjapan.phonecontrolfixture
  printf 'fixture_source=owned_rust_uikit_bundle\nforeground_seconds_requested=20\npicker_selectable_verified=false\n' > "$OUT/fixture-install-scope.txt"
  xcrun simctl listapps "$SIM_ID" > "$RUNNER_TEMP/official-simulator-apps.plist"
  plutil -convert json -o "$RUNNER_TEMP/official-simulator-apps.json" "$RUNNER_TEMP/official-simulator-apps.plist"
  jq -e 'has("com.apple.MobileSMS")' "$RUNNER_TEMP/official-simulator-apps.json" > "$OUT/official-simulator-app-fixture.txt"
  xcrun simctl launch "$SIM_ID" com.apple.MobileSMS > "$RUNNER_TEMP/messages-fixture-launch.txt"
  xcrun simctl terminate "$SIM_ID" com.apple.MobileSMS
  printf 'fixture_source=official_fresh_simulator\nfixture_acquisition_attempts=1\nthird_party_playback_verified=false\n' >> "$OUT/official-simulator-app-fixture.txt"
fi
xcodebuild -project "$XCODE_ROOT/Probe.xcodeproj" -scheme Probe -configuration Debug -sdk iphonesimulator -destination "id=$SIM_ID" -parallel-testing-enabled NO SYMROOT="$OUT/xcode-products" OBJROOT="$RUNNER_TEMP/xcode-objects" build-for-testing > "$OUT/xcode-ui-build.txt" 2>&1
xcrun otool -L "$PROBE_APP/authorization-center-probe" > "$OUT/host-linked-libraries.txt"
if grep -Fq 'XCTest.framework' "$OUT/host-linked-libraries.txt"; then
  printf 'Test framework unexpectedly linked into the inspected host\n' >&2
  exit 1
fi
if [ "$INPUT_CATEGORY_TOKEN_COUNT_ACCEPTANCE" = true ]; then
  {
    printf 'audit_source=%s\nmode=category_token_count_acceptance\ncargo_lock_applicable=false\n' "$(git rev-parse HEAD)"
    rustc --version --verbose
    shasum -a 256 tools/ios-abi-probe.rs tools/ios-category-token-count.rs tools/ios-category-token-acceptance.rs tools/PickerCategoryObservation.m .github/workflows/ios-sdk-audit.yml "$PROBE_APP/authorization-center-probe" "$XCODE_ROOT/libui-test.a" "$XCODE_ROOT/category-observation.o"
  } > "$OUT/category-token-count-build.txt"
fi
if [ "$INPUT_SELECTION_CAPTURE_OWNER_ACCEPTANCE" = true ]; then
  {
    printf 'audit_source=%s\nmode=selection_capture_owner_acceptance\ncargo_lock_applicable=false\n' "$(git rev-parse HEAD)"
    rustc --version --verbose
    shasum -a 256 tools/ios-abi-probe.rs tools/ios-selection-captured-owner.rs tools/ios-selection-captured-callback.rs tools/ios-selection-captured-guards.rs tools/ios-selection-captured-host.rs tools/ios-selection-captured-ui.rs tools/ios-selection-autorelease.rs tools/CapturedSelectionAutoreleasePool.m tools/CapturedHostingLowering.swift tools/build-captured-autorelease.sh tools/check-captured-selection-result.rs tools/CapturedSelectionDeclarations.m tools/PickerCategoryObservation.m .github/workflows/ios-sdk-audit.yml "$PROBE_APP/authorization-center-probe" "$XCODE_ROOT/libui-test.a" "$XCODE_ROOT/category-observation.o" "$XCODE_ROOT/captured-autorelease.o"
    shasum -a 256 "$RUNNER_TEMP/check-captured-selection-result"
  } > "$OUT/captured-selection-build.txt"
fi
TEST_SECONDS=90
if [ "$INPUT_SELECTION_INSPECTION_MODE" = consent-selection-acceptance ]; then TEST_SECONDS=180; fi
printf 'xctest_started_at=%s\ntest_limit_seconds=%s\n' "$(date -u +%FT%TZ)" "$TEST_SECONDS" > "$OUT/consent-selection-phase.txt"
if [ "$INPUT_SELECTION_CONTEXT_ARC_TRACE" = true ]; then
  test ! -e "$OUT/xcode-ui-test.txt"
  test ! -e "$OUT/captured-selection-xctest-exit.txt"
  tar -czf "$OUT/captured-selection-probe.app.tar.gz" -C "$(dirname "$PROBE_APP")" "$(basename "$PROBE_APP")"
  shasum -a 256 "$OUT/captured-selection-probe.app.tar.gz" > "$OUT/captured-selection-probe-archive-hash.txt"
fi
# Capture host errors (including a pre-UI ABI guard panic) independently
# of whether XCTest reaches its first application control.
# Limit exported diagnostics to known guard failures and aggregates;
# never retain framework debug messages or selected application names.
xcrun simctl spawn "$SIM_ID" log stream --style compact --predicate 'process == "authorization-center-probe" AND (eventMessage CONTAINS "panicked at" OR eventMessage CONTAINS "prefix changed" OR eventMessage CONTAINS "Selection snapshot setters=" OR eventMessage CONTAINS "timed out")' > "$OUT/consent-selection-host-runtime.txt" 2>&1 &
HOST_LOG_PID=$!
UI_COMMAND=(xcodebuild -project "$XCODE_ROOT/Probe.xcodeproj" -scheme Probe -configuration Debug -sdk iphonesimulator -destination "id=$SIM_ID" -parallel-testing-enabled NO -test-timeouts-enabled YES -default-test-execution-time-allowance "$TEST_SECONDS" -maximum-test-execution-time-allowance "$TEST_SECONDS" -resultBundlePath "$OUT/consent-ui.xcresult" SYMROOT="$OUT/xcode-products" OBJROOT="$RUNNER_TEMP/xcode-objects" test-without-building)
# Both normal XCTest and ARC diagnostics use the same child owner.
# Cleanup submits a stop request rather than waiting on an unowned
# live xcodebuild or signalling a recyclable Bash child PID.
OWNER_MODE=ordinary
if [ "$INPUT_SELECTION_CONTEXT_ARC_TRACE" = true ]; then OWNER_MODE=arc; fi
UI_COMMAND=(perl tools/ios-xctest-process-owner.pl "$OUT" "$OWNER_MODE" "${UI_COMMAND[@]}")
UI_TEST_GROUP=true
UI_PROCESS_STARTED=$SECONDS
"${UI_COMMAND[@]}" > "$OUT/xcode-ui-test.txt" 2>&1 &
UI_TEST_PID=$!
if [ "$INPUT_SELECTION_CONTEXT_ARC_TRACE" = true ]; then
  printf '%s\n' "$UI_TEST_PID" > "$OUT/captured-selection-context-xctest-process.txt"
  # Only an executing test method can publish the fresh runner
  # generation before the bounded LLDB flight is started.
  bash tools/ios-selection-xctest-admission.sh "$OUT" "$PROBE_APP" "$SIM_ID" &
  TRACE_PID=$!
fi
UI_TEST_STATUS=0
if [ "$INPUT_SELECTION_CONTEXT_ARC_TRACE" = true ]; then
  while kill -0 "$UI_TEST_PID" 2>/dev/null; do
    if [ "$((SECONDS-UI_PROCESS_STARTED))" -ge 180 ]; then
      UI_TEST_STATUS=124
      printf 'xctest_process_deadline=true\n' >> "$OUT/captured-selection-context-admission-status.txt"
      stop_owned_ui || true
      break
    fi
    if [ -n "$TRACE_PID" ] && ! kill -0 "$TRACE_PID" 2>/dev/null; then
      TRACE_EARLY_STATUS=0
      wait "$TRACE_PID" || TRACE_EARLY_STATUS=$?
      TRACE_PID=''
      if [ "$TRACE_EARLY_STATUS" != 0 ]; then
        UI_TEST_STATUS=1
        stop_owned_ui || true
        break
      fi
    fi
    sleep 0.1
  done
fi
if [ -n "$UI_TEST_PID" ] && [ "$UI_TEST_STATUS" = 0 ]; then
  wait "$UI_TEST_PID" || UI_TEST_STATUS=$?
  UI_TEST_PID=''
fi
if [ "$UI_TEST_GROUP" = true ]; then
  if ! stop_owned_ui; then UI_TEST_STATUS=1; fi
  if ! grep -Fxq 'leader_reaped=1' "$OUT/captured-selection-context-xctest-owner-status.txt" ||
     ! grep -Fxq 'xctest_process_group_remaining=0' "$OUT/captured-selection-context-xctest-owner-status.txt"; then UI_TEST_STATUS=1; fi
fi
printf 'xctest_exit=%s\n' "$UI_TEST_STATUS" > "$OUT/captured-selection-xctest-exit.txt"
kill "$HOST_LOG_PID" 2>/dev/null || true
wait "$HOST_LOG_PID" 2>/dev/null || true
HOST_LOG_PID=''
TRACE_STATUS=0
if [ "${TRACE_EARLY_STATUS:-0}" != 0 ]; then TRACE_STATUS=$TRACE_EARLY_STATUS; fi
if [ -n "$TRACE_PID" ]; then
  wait "$TRACE_PID" || TRACE_STATUS=$?
  TRACE_PID=''
  printf 'trace_process_status=%s\n' "$TRACE_STATUS" >> "$OUT/captured-selection-context-status.txt"
fi
# Always preserve partial app files BEFORE asserting test success.
if [ "$INPUT_SELECTION_INSPECTION_MODE" = consent-selection-acceptance ]; then
  printf 'xctest_exit_status=%s\nmac_acceptance_verified=false\n' "$UI_TEST_STATUS" > "$OUT/consent-selection-test-status.txt"
  if DATA=$(xcrun simctl get_app_container "$SIM_ID" com.autodevjapan.phonecontrol1614abiprobe data); then
    for FILE in public-authorization-result.txt consent-selection-result.txt consent-selection-construction-error.txt consent-selection-picker-code.txt selection-count-entry-prefixes.txt selection-count-layout.txt captured-selection-teardown.txt captured-selection-preflight.txt; do
      if [ -f "$DATA/Documents/$FILE" ]; then cp "$DATA/Documents/$FILE" "$OUT/$FILE"; fi
    done
  fi
  # XCTest may retain screenshots before terminating the app.
  # Export these existing attachments rather than starting a second
  # background simctl capture process with a separate lifetime.
  ATTACHMENT_STATUS=0
  perl -e 'alarm 60; exec @ARGV; die "exec failed\n"' xcrun xcresulttool export attachments --path "$OUT/consent-ui.xcresult" --output-path "$OUT/consent-selection-attachments" > "$OUT/consent-selection-attachment-export.txt" 2>&1 || ATTACHMENT_STATUS=$?
  printf 'attachment_export_status=%s\n' "$ATTACHMENT_STATUS" >> "$OUT/consent-selection-attachment-export.txt"
  ATTACHMENT_COUNT=0
  if [ -d "$OUT/consent-selection-attachments" ]; then ATTACHMENT_COUNT=$(find "$OUT/consent-selection-attachments" -type f | wc -l | tr -d ' '); fi
  printf 'attachment_file_count=%s\n' "$ATTACHMENT_COUNT" >> "$OUT/consent-selection-attachment-export.txt"
  grep '^XCTest consent-selection:' "$OUT/xcode-ui-test.txt" > "$OUT/consent-selection-xctest-observations.txt" || true
  xcrun simctl io "$SIM_ID" screenshot "$OUT/consent-selection-after-test.png" || true
  # Category-only, callback-only, and ABI-pending runs all fail closed.
  test "$UI_TEST_STATUS" = 0
  grep -Fx 'error_present=false' "$OUT/public-authorization-result.txt"
  grep -Fx 'status_label=approved' "$OUT/public-authorization-result.txt"
  grep -Fx 'status_read_on_main_thread=true' "$OUT/public-authorization-result.txt"
  if [ "$INPUT_CATEGORY_SELECTION_OBSERVATION" = true ]; then
    grep -F 'category_control_tap_returned=true;' "$OUT/consent-selection-xctest-observations.txt"
    grep -Fx 'semantic_app_selection_verified=false' "$OUT/consent-selection-result.txt"
    grep -Fx 'owned_set_vwt_copy_destroy=true' "$OUT/consent-selection-result.txt"
    if [ "$INPUT_CATEGORY_TOKEN_COUNT_ACCEPTANCE" = true ]; then
      grep -Eq 'category_tokens_before=0; category_tokens_after=[1-9][0-9]*; category_token_count_verified=true; individual_app_tap=false$' "$OUT/consent-selection-xctest-observations.txt"
      PRE=$(sed -n 's/^XCTest consent-selection: category_control_tap_returned=true; setter_before=\([0-9][0-9]*\); setter_after=[0-9][0-9]*;.*category_token_count_verified=true; individual_app_tap=false$/\1/p' "$OUT/consent-selection-xctest-observations.txt")
      POST=$(sed -n 's/^XCTest consent-selection: category_control_tap_returned=true; setter_before=[0-9][0-9]*; setter_after=\([0-9][0-9]*\);.*category_token_count_verified=true; individual_app_tap=false$/\1/p' "$OUT/consent-selection-xctest-observations.txt")
      test "$POST" -gt "$PRE"
      RESULTS="$OUT/consent-selection-result.txt"
      grep -Fx 'initial_category_tokens_count=0' "$RESULTS"
      grep -Eq '^selected_category_tokens_count=[1-9][0-9]*$' "$RESULTS"
      grep -Fx 'selection_snapshot_lifetime_verified=true' "$RESULTS"
      grep -Eq '^selection_snapshot_category_tokens_count=[1-9][0-9]*$' "$RESULTS"
      for FIELD in selection_snapshot_initialize_with_copy_calls selection_snapshot_destroy_calls selection_snapshot_allocations selection_snapshot_deallocations; do grep -Fx "$FIELD=2" "$RESULTS"; done
      for FIELD in category_tokens_getter_calls category_tokens_count_calls category_set_initialize_with_copy_calls category_set_destroy_calls category_set_allocations category_set_deallocations; do test "$(grep -Ec "^$FIELD=[0-9]+$" "$RESULTS")" = 1; done
      GETS=$(sed -n 's/^category_tokens_getter_calls=//p' "$RESULTS")
      COUNTS=$(sed -n 's/^category_tokens_count_calls=//p' "$RESULTS")
      COPIES=$(sed -n 's/^category_set_initialize_with_copy_calls=//p' "$RESULTS")
      DESTROYS=$(sed -n 's/^category_set_destroy_calls=//p' "$RESULTS")
      ALLOCS=$(sed -n 's/^category_set_allocations=//p' "$RESULTS")
      FREES=$(sed -n 's/^category_set_deallocations=//p' "$RESULTS")
      test "$GETS" -ge 2
      test "$COUNTS" = "$GETS"
      test "$COPIES" = "$GETS"
      test "$DESTROYS" = "$((GETS*2))"
      test "$ALLOCS" = "$DESTROYS"
      test "$FREES" = "$ALLOCS"
      grep -Fx 'selection_state_pinned=true' "$RESULTS"
      grep -Fx 'full_teardown_verified=false' "$RESULTS"
      printf 'xctest_exit_status=0\ncategory_ui_setter_delta_observed=true\ncategory_token_count_verified=true\nselection_snapshot_lifetime_verified=true\nindividual_app_selection_verified=false\ncategory_instance_count_verified=false\nfull_teardown_verified=false\n' > "$OUT/consent-selection-test-status.txt"
      if [ "$INPUT_SELECTION_CAPTURE_OWNER_ACCEPTANCE" = true ]; then
        grep -Fx 'XCTest consent-selection: owned_teardown_requested=true; target_not_running=true; teardown_file_acceptance_required=true' "$OUT/consent-selection-xctest-observations.txt"
        if [ "$INPUT_SELECTION_CONTEXT_ARC_TRACE" = true ]; then
          test "$TRACE_STATUS" = 0
          grep -Fx 'context_arc_stack_capture_complete=true' "$OUT/captured-selection-context-status.txt"
          grep -Fx 'product_drain_accepted=false' "$OUT/captured-selection-context-status.txt"
          bash tools/check-selection-context-identity.sh "$OUT"
          printf 'diagnostic_only=true\ncontext_arc_stack_capture_complete=true\nproduct_drain_accepted=false\n' > "$OUT/captured-selection-context-result.txt"
          exit 0
        fi
        "$RUNNER_TEMP/check-captured-selection-result" "$OUT/captured-selection-teardown.txt" "$RESULTS" > "$OUT/captured-selection-result-gate.txt"
        cat "$OUT/captured-selection-result-gate.txt" >> "$OUT/consent-selection-test-status.txt"
      fi
    else
      grep -F 'category_token_count_verified=false; individual_app_tap=false' "$OUT/consent-selection-xctest-observations.txt"
      printf 'xctest_exit_status=0\ncategory_ui_setter_delta_observed=true\ncategory_token_count_verified=false\nindividual_app_selection_verified=false\nfull_teardown_verified=false\n' > "$OUT/consent-selection-test-status.txt"
    fi
    exit 0
  fi
  grep -Eq '^rust_setter_calls=[1-9][0-9]*$' "$OUT/consent-selection-result.txt"
  grep -Fx 'application_choice_armed=true' "$OUT/consent-selection-result.txt"
  grep -Eq '^rust_setter_calls_after_picker_presentation=[1-9][0-9]*$' "$OUT/consent-selection-result.txt"
  grep -Fx 'initial_applications_count=0' "$OUT/consent-selection-result.txt"
  grep -Eq '^selected_applications_count=[1-9][0-9]*$' "$OUT/consent-selection-result.txt"
  grep -Fx 'applications_count_abi_verified=true' "$OUT/consent-selection-result.txt"
  grep -Fx 'semantic_app_selection_verified=true' "$OUT/consent-selection-result.txt"
  grep -Fx 'count_read_on_main_thread=true' "$OUT/consent-selection-result.txt"
  grep -Fx 'owned_set_vwt_copy_destroy=true' "$OUT/consent-selection-result.txt"
  grep -Eq '^applications_getter_calls=[0-9]+$' "$OUT/consent-selection-result.txt"
  GETTER_CALLS=$(sed -n 's/^applications_getter_calls=//p' "$OUT/consent-selection-result.txt")
  COUNT_CALLS=$(sed -n 's/^count_getter_calls=//p' "$OUT/consent-selection-result.txt")
  test "$GETTER_CALLS" -ge 2
  test "$GETTER_CALLS" = "$COUNT_CALLS"
  grep -F 'available_app_control_observed=true' "$OUT/consent-selection-xctest-observations.txt"
  grep -F 'actual_app_control_tap_returned=true' "$OUT/consent-selection-xctest-observations.txt"
  PRE=$(sed -n 's/^XCTest consent-selection: pre_tap_setter_calls=\([0-9][0-9]*\); pre_tap_applications_count=0$/\1/p' "$OUT/consent-selection-xctest-observations.txt")
  POST=$(sed -n 's/^XCTest consent-selection: post_tap_setter_calls=\([0-9][0-9]*\); post_tap_setter_delta=[1-9][0-9]*; post_tap_applications_count=[1-9][0-9]*; semantic_app_selection_verified=true$/\1/p' "$OUT/consent-selection-xctest-observations.txt")
  DELTA=$(sed -n 's/^XCTest consent-selection: post_tap_setter_calls=[0-9][0-9]*; post_tap_setter_delta=\([1-9][0-9]*\); post_tap_applications_count=[1-9][0-9]*; semantic_app_selection_verified=true$/\1/p' "$OUT/consent-selection-xctest-observations.txt")
  test "$POST" -gt "$PRE"
  test "$((POST-PRE))" = "$DELTA"
  HOST_SETTERS=$(sed -n 's/^rust_setter_calls=//p' "$OUT/consent-selection-result.txt")
  test "$HOST_SETTERS" -ge "$POST"
  printf 'xctest_exit_status=0\nmac_acceptance_verified=true\nmonitoring_verified=false\nshielding_verified=false\nproduct_release_verified=false\n' > "$OUT/consent-selection-test-status.txt"
  exit 0
fi
xcrun simctl io "$SIM_ID" screenshot "$OUT/consent-after-test.png"
xcrun simctl spawn "$SIM_ID" log show --last 2m --style compact --predicate 'process == "authorization-center-probe" OR process == "FamilyControlsAgent"' > "$OUT/authorization-process-log.txt" 2>&1
test "$UI_TEST_STATUS" = 0
DATA=$(xcrun simctl get_app_container "$SIM_ID" com.autodevjapan.phonecontrol1614abiprobe data)
cp "$DATA/Documents/public-authorization-result.txt" "$OUT/public-authorization-result.txt"
grep -Fx 'public_authorization_continuation_observed=true' "$OUT/public-authorization-result.txt"
if [ "$INPUT_UI_CONTINUE" = false ]; then
  grep -Fx 'error_present=true' "$OUT/public-authorization-result.txt"
else
  grep -Fx 'error_present=false' "$OUT/public-authorization-result.txt"
fi
if [ "$INPUT_AUTHORIZATION_STATUS_CONSENT_PROBE" = true ]; then
  grep -Fx 'authorization_status_verified=true' "$OUT/public-authorization-result.txt"
  grep -Fx 'status_read_on_main_thread=true' "$OUT/public-authorization-result.txt"
  grep -Fx 'status_getter_calls=1' "$OUT/public-authorization-result.txt"
  grep -Fx 'raw_value_getter_calls=1' "$OUT/public-authorization-result.txt"
  grep -Fx 'enum_case_name_calls=1' "$OUT/public-authorization-result.txt"
  if [ "$INPUT_UI_CONTINUE" = false ]; then
    # SDK17.5 individual cancellation returned error + notDetermined
    # in run38081982214. Refusal must never count as approved; the
    # Rust reader already checks the exact raw/reflected-case pair.
    grep -Eq '^status_raw_value=[01]$' "$OUT/public-authorization-result.txt"
    grep -Eq '^status_label=(notDetermined|denied)$' "$OUT/public-authorization-result.txt"
  else
    grep -Fx 'status_raw_value=2' "$OUT/public-authorization-result.txt"
    grep -Fx 'status_label=approved' "$OUT/public-authorization-result.txt"
  fi
  printf 'xcode_host_launched=true\nordinary_permission_button_tested=true\nauthorization_status_verified=true\nmonitoring=false\nshielding=false\n' > "$OUT/consent-ui-scope.txt"
else
  printf 'xcode_host_launched=true\nordinary_permission_button_tested=true\nauthorization_status_verified=false\nmonitoring=false\nshielding=false\n' > "$OUT/consent-ui-scope.txt"
fi
exit 0
