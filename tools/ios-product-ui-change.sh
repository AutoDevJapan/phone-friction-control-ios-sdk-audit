#!/bin/bash
set -euo pipefail
ROOT="$RUNNER_TEMP/product-ui-test"
mkdir -p "$ROOT/Tests.xcodeproj/xcshareddata/xcschemes"
cp tools/ProductPersistenceTests.m "$ROOT/Test.m"
cat > "$ROOT/Tests.xcodeproj/project.pbxproj" <<'PROJECT'
// !$*UTF8*$!
{
 archiveVersion=1; objectVersion=56; classes={};
 objects={
 A00000000000000000000001={isa=PBXProject; buildConfigurationList=A00000000000000000000002; compatibilityVersion="Xcode 14.0"; mainGroup=A00000000000000000000003; targets=(A00000000000000000000004);};
 A00000000000000000000002={isa=XCConfigurationList; buildConfigurations=(A00000000000000000000005); defaultConfigurationName=Debug;};
 A00000000000000000000003={isa=PBXGroup; children=(A00000000000000000000006,A00000000000000000000007); sourceTree="<group>";};
 A00000000000000000000004={isa=PBXNativeTarget; buildConfigurationList=A00000000000000000000008; buildPhases=(A00000000000000000000009,A00000000000000000000010); name=ProductPersistenceTests; productName=ProductPersistenceTests; productReference=A00000000000000000000007; productType="com.apple.product-type.bundle.ui-testing";};
 A00000000000000000000005={isa=XCBuildConfiguration; buildSettings={SDKROOT=iphonesimulator; IPHONEOS_DEPLOYMENT_TARGET=18.5; CLANG_ENABLE_MODULES=YES; CLANG_ENABLE_OBJC_ARC=YES;}; name=Debug;};
 A00000000000000000000006={isa=PBXFileReference; lastKnownFileType=sourcecode.c.objc; path=Test.m; sourceTree="<group>";};
 A00000000000000000000007={isa=PBXFileReference; explicitFileType=wrapper.cfbundle; path=ProductPersistenceTests.xctest; sourceTree=BUILT_PRODUCTS_DIR;};
 A00000000000000000000008={isa=XCConfigurationList; buildConfigurations=(A00000000000000000000011); defaultConfigurationName=Debug;};
 A00000000000000000000009={isa=PBXSourcesBuildPhase; buildActionMask=2147483647; files=(A00000000000000000000012); runOnlyForDeploymentPostprocessing=0;};
 A00000000000000000000010={isa=PBXFrameworksBuildPhase; buildActionMask=2147483647; files=(); runOnlyForDeploymentPostprocessing=0;};
 A00000000000000000000011={isa=XCBuildConfiguration; buildSettings={ARCHS=arm64; CODE_SIGN_STYLE=Automatic; CODE_SIGN_IDENTITY="-"; GENERATE_INFOPLIST_FILE=YES; PRODUCT_BUNDLE_IDENTIFIER=com.autodevjapan.productpersistencetests; PRODUCT_NAME=ProductPersistenceTests; TARGETED_DEVICE_FAMILY=1; SUPPORTED_PLATFORMS=iphonesimulator; FRAMEWORK_SEARCH_PATHS=("$(inherited)","$(PLATFORM_DIR)/Developer/Library/Frameworks"); OTHER_LDFLAGS=("-framework",XCTest);}; name=Debug;};
 A00000000000000000000012={isa=PBXBuildFile; fileRef=A00000000000000000000006;};
 }; rootObject=A00000000000000000000001;
}
PROJECT
cat > "$ROOT/Tests.xcodeproj/xcshareddata/xcschemes/Tests.xcscheme" <<'SCHEME'
<?xml version="1.0" encoding="UTF-8"?>
<Scheme LastUpgradeVersion="1640" version="1.3">
<BuildAction parallelizeBuildables="NO" buildImplicitDependencies="YES"><BuildActionEntries><BuildActionEntry buildForTesting="YES" buildForRunning="NO" buildForProfiling="NO" buildForArchiving="NO" buildForAnalyzing="YES"><BuildableReference BuildableIdentifier="primary" BlueprintIdentifier="A00000000000000000000004" BuildableName="ProductPersistenceTests.xctest" BlueprintName="ProductPersistenceTests" ReferencedContainer="container:Tests.xcodeproj"/></BuildActionEntry></BuildActionEntries></BuildAction>
<TestAction buildConfiguration="Debug" selectedDebuggerIdentifier="Xcode.DebuggerFoundation.Debugger.LLDB" selectedLauncherIdentifier="Xcode.IDEFoundation.Launcher.LLDB"><Testables><TestableReference skipped="NO"><BuildableReference BuildableIdentifier="primary" BlueprintIdentifier="A00000000000000000000004" BuildableName="ProductPersistenceTests.xctest" BlueprintName="ProductPersistenceTests" ReferencedContainer="container:Tests.xcodeproj"/></TestableReference></Testables></TestAction>
</Scheme>
SCHEME
UI_STATUS=0
perl -e 'alarm 180; exec @ARGV; die "exec failed\n"' xcodebuild -project "$ROOT/Tests.xcodeproj" -scheme Tests -destination "id=$SIM_ID" -parallel-testing-enabled NO -test-timeouts-enabled YES -default-test-execution-time-allowance 60 -maximum-test-execution-time-allowance 60 -resultBundlePath "$OUT/ui.xcresult" test > "$OUT/ui-test.txt" 2>&1 || UI_STATUS=$?
printf 'ui_test_status=%s\n' "$UI_STATUS" > "$OUT/ui-status.txt"
perl -e 'alarm 60; exec @ARGV; die "exec failed\n"' xcrun xcresulttool export attachments --path "$OUT/ui.xcresult" --output-path "$OUT/ui-attachments" > "$OUT/attachment-export.txt" 2>&1 || true
test "$UI_STATUS" = 0
