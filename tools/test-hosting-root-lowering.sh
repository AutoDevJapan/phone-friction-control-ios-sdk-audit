#!/usr/bin/env bash
# Synthetic compiler-text fixtures only; no SDK output is reconstructed.
set -euo pipefail
ROOT=$(mktemp -d)
trap 'rm -rf "$ROOT"' EXIT
cat > "$ROOT/valid.sil" <<'SIL'
sil hidden [ossa] @$s21CapturedRootOwnership24inspectConstantSelectiony7SwiftUI7BindingVy14FamilyControls0j8ActivityF0VGAHF : $@convention(thin) (@in_guaranteed FamilyActivitySelection) -> @out Binding<FamilyActivitySelection> {
%14 = class_method %0 : $UIHostingController<FamilyActivityPicker>, #UIHostingController.rootView!setter : (@in FamilyActivityPicker, @guaranteed UIHostingController<FamilyActivityPicker>) -> ()
SIL
cat > "$ROOT/valid.ll" <<'IR'
define hidden swiftcc void @"$s21CapturedRootOwnership016inspectReplacingaB0yy7SwiftUI19UIHostingControllerCy14FamilyControls0J14ActivityPickerVG_AHntF"(ptr %0, ptr %1) {
call swiftcc void @"$s7SwiftUI19UIHostingControllerC8rootViewxvsTj"(ptr noalias %6, ptr swiftself %0)
ret void
}
IR
if grep -F 'UIHostingControllerC8rootViewxvs' "$ROOT/valid.sil"; then exit 1; fi
printf 'old_direct_symbol_gate_rejects_valid_class_method=true\n'
for CASE in valid missing_constant missing_method owned_instead_guaranteed missing_dispatch missing_swiftself comment_constant comment_method comment_call wrong_selection wrong_picker unrelated_call; do
  mkdir "$ROOT/$CASE"
  cp "$ROOT/valid.sil" "$ROOT/$CASE/root-replacement.sil"
  cp "$ROOT/valid.ll" "$ROOT/$CASE/root-replacement.ll"
  case "$CASE" in
    missing_constant) sed '/^sil /d' "$ROOT/valid.sil" > "$ROOT/$CASE/root-replacement.sil";;
    missing_method) sed '/class_method/d' "$ROOT/valid.sil" > "$ROOT/$CASE/root-replacement.sil";;
    owned_instead_guaranteed) sed 's/@guaranteed/@owned/' "$ROOT/valid.sil" > "$ROOT/$CASE/root-replacement.sil";;
    missing_dispatch) printf 'unrelated\n' > "$ROOT/$CASE/root-replacement.ll";;
    missing_swiftself) sed 's/ptr swiftself/ptr/' "$ROOT/valid.ll" > "$ROOT/$CASE/root-replacement.ll";;
    comment_constant) sed 's/^sil /\/\/ sil /' "$ROOT/valid.sil" > "$ROOT/$CASE/root-replacement.sil";;
    comment_method) sed 's/^%14/\/\/ %14/' "$ROOT/valid.sil" > "$ROOT/$CASE/root-replacement.sil";;
    comment_call) sed 's/^call/;call/' "$ROOT/valid.ll" > "$ROOT/$CASE/root-replacement.ll";;
    wrong_selection) sed 's/FamilyActivitySelection/WrongSelection/g' "$ROOT/valid.sil" > "$ROOT/$CASE/root-replacement.sil";;
    wrong_picker) sed 's/FamilyActivityPicker/WrongPicker/g' "$ROOT/valid.sil" > "$ROOT/$CASE/root-replacement.sil";;
    unrelated_call) sed '/^call /d' "$ROOT/valid.ll" > "$ROOT/$CASE/root-replacement.ll"; printf 'define void @unrelated() {\ncall swiftcc void @"$s7SwiftUI19UIHostingControllerC8rootViewxvsTj"(ptr noalias %%6, ptr swiftself %%0)\nret void\n}\n' >> "$ROOT/$CASE/root-replacement.ll";;
  esac
  STATUS=0
  bash tools/check-hosting-root-lowering.sh "$ROOT/$CASE" || STATUS=$?
  if [ "$CASE" = valid ]; then test "$STATUS" = 0; else test "$STATUS" != 0; fi
  printf 'case=%s exit=%s\n' "$CASE" "$STATUS"
done
