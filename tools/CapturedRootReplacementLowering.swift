// Compiler inspection only. Never linked into or executed by the audit host.
import FamilyControls
import SwiftUI

@MainActor
func inspectConstantSelection(_ selection: FamilyActivitySelection)
    -> Binding<FamilyActivitySelection> {
    Binding.constant(selection)
}

@MainActor
func inspectReplacingCapturedRoot(
    _ controller: UIHostingController<FamilyActivityPicker>,
    _ replacement: consuming FamilyActivityPicker
) {
    controller.rootView = replacement
}

@MainActor
func inspectRootWithoutCapturedContext(
    _ controller: UIHostingController<FamilyActivityPicker>
) {
    let empty = FamilyActivitySelection(includeEntireCategory: false)
    let selection = Binding.constant(empty)
    controller.rootView = FamilyActivityPicker(selection: selection)
}
