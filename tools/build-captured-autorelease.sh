#!/usr/bin/env bash
set -euo pipefail
test "$#" = 4
SDK=$1
OUT=$2
XCODE_ROOT=$3
NATIVE_LIBS=$4

# Non-calling Swift compiler evidence for the exact public allocator parameter
# and return ownership, before any newly observed Hosting lifetime is executed.
xcrun swiftc -target arm64-apple-ios18.5-simulator -sdk "$SDK" -swift-version 6 -parse-as-library -emit-silgen tools/CapturedHostingLowering.swift -o "$OUT/captured-hosting-ownership-sil.txt"
test -n "${HOSTING_INIT_SYMBOL:-}"
grep '^sil ' "$OUT/captured-hosting-ownership-sil.txt" | grep -F "@$HOSTING_INIT_SYMBOL " > "$OUT/captured-hosting-allocator-convention.txt"
grep -Eq '(^|[[:space:](,])@in([[:space:],)]|$)' "$OUT/captured-hosting-allocator-convention.txt"
grep -F '@owned UIHostingController' "$OUT/captured-hosting-allocator-convention.txt"

# Compiler-generated pool boundary; no target-specific retain-count adjustment.
xcrun clang --version > "$OUT/captured-autorelease-compiler.txt"
xcrun clang -target arm64-apple-ios18.5-simulator -isysroot "$SDK" -fobjc-arc -Werror -S -emit-llvm tools/CapturedSelectionAutoreleasePool.m -o "$OUT/captured-autorelease-lowering.txt"
grep -Eq 'call.*@(llvm\.objc\.autoreleasePoolPush|objc_autoreleasePoolPush)\(' "$OUT/captured-autorelease-lowering.txt"
grep -Eq 'call.*@(llvm\.objc\.autoreleasePoolPop|objc_autoreleasePoolPop)\(' "$OUT/captured-autorelease-lowering.txt"
xcrun clang -target arm64-apple-ios18.5-simulator -isysroot "$SDK" -fobjc-arc -Werror -c tools/CapturedSelectionAutoreleasePool.m -o "$XCODE_ROOT/captured-autorelease.o"
xcrun nm -gU "$XCODE_ROOT/captured-autorelease.o" > "$OUT/captured-autorelease-symbols.txt"
grep -Eq '[[:space:]]T[[:space:]]+_audit_capture_autorelease_pool$' "$OUT/captured-autorelease-symbols.txt"
printf 'OTHER_LDFLAGS = $(inherited) -L$(SDKROOT)/usr/lib/swift %s "%s"\n' "$NATIVE_LIBS" "$XCODE_ROOT/captured-autorelease.o" > "$XCODE_ROOT/Host.xcconfig"
