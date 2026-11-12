// Compile-only SDK ownership evidence. This function is not linked or called.
import FamilyControls
import SwiftUI

@MainActor
func inspectCapturedHosting(_ root: consuming FamilyActivityPicker)
    -> UIHostingController<FamilyActivityPicker> {
    UIHostingController(rootView: root)
}
