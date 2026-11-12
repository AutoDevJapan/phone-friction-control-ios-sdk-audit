#!/usr/bin/env bash
# Exact fixed-module compiler evidence; never infer runtime success.
set -euo pipefail
test "$#" = 1
OUT=$1
test -s "$OUT/root-replacement.sil"
test -s "$OUT/root-replacement.ll"
grep -E '^sil .*@\$s21CapturedRootOwnership24inspectConstantSelection[^ ]* : \$@convention\(thin\) \(@in_guaranteed FamilyActivitySelection\) -> @out Binding<FamilyActivitySelection> \{' "$OUT/root-replacement.sil" > "$OUT/constant-declaration.txt"
grep -E '^[[:space:]]*%[0-9]+ = class_method %[0-9]+ : \$UIHostingController<FamilyActivityPicker>, #UIHostingController\.rootView!setter : .*\(@in (τ_0_0, @guaranteed UIHostingController<τ_0_0>|FamilyActivityPicker, @guaranteed UIHostingController<FamilyActivityPicker>)\) -> \(\)' "$OUT/root-replacement.sil" > "$OUT/root-setter-declaration.txt"
awk '/^define .*@"\$s21CapturedRootOwnership016inspectReplacingaB0yy7SwiftUI19UIHostingControllerCy14FamilyControls0J14ActivityPickerVG_AHntF"\(/ { inside=1; next } inside && /^}/ { inside=0 } inside { print }' "$OUT/root-replacement.ll" | grep -E '^[[:space:]]*(%[^=]+ = )?call swiftcc void @"\$s7SwiftUI19UIHostingControllerC8rootViewxvsTj"\(ptr noalias %[^,]+, ptr swiftself %[^)]+\)' > "$OUT/root-setter-dispatch.txt"
