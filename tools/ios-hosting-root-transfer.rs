// Held bridge candidate. Public export/image guards and replacement-value
// ownership wiring require independent review before a runtime acceptance.
// ABI source: SDK18.5 / Swift6.1.2 compiler run43067567260 SIL, IR and assembly.

#[cfg(all(target_os = "ios", target_env = "sim", target_arch = "aarch64"))]
core::arch::global_asm!(
    ".text",
    ".p2align 2",
    ".globl _audit_binding_constant",
    "_audit_binding_constant:",
    ".cfi_startproc",
    "stp x29, x30, [sp, #-16]!",
    ".cfi_def_cfa_offset 16",
    ".cfi_offset 29, -16",
    ".cfi_offset 30, -8",
    "mov x29, sp",
    // C: destination, borrowed Selection, Selection metadata, public entry.
    "mov x8, x0",
    "mov x0, x1",
    "mov x1, x2",
    "blr x3",
    "ldp x29, x30, [sp], #16",
    ".cfi_restore 29",
    ".cfi_restore 30",
    ".cfi_def_cfa_offset 0",
    "ret",
    ".cfi_endproc",
    ".globl _audit_hosting_replace_root",
    "_audit_hosting_replace_root:",
    ".cfi_startproc",
    "stp x20, x30, [sp, #-16]!",
    ".cfi_def_cfa_offset 16",
    ".cfi_offset 20, -16",
    ".cfi_offset 30, -8",
    // C: consumed Picker, borrowed Hosting controller, public dispatch thunk.
    "mov x20, x1",
    "blr x2",
    "ldp x20, x30, [sp], #16",
    ".cfi_restore 20",
    ".cfi_restore 30",
    ".cfi_def_cfa_offset 0",
    "ret",
    ".cfi_endproc",
);

// Transfer accounting is deliberately independent of an actual Swift call.
// The caller must mark its real Picker uninitialized before invoking enter,
// then free only that allocation. This does not establish native acceptance.
#[derive(Default)]
pub(crate) struct RootTransfer {
    entered: bool,
    completed: bool,
}
impl RootTransfer {
    pub(crate) fn enter(&mut self, initialized: bool) -> Result<(), &'static str> {
        if self.entered || !initialized {
            return Err("replacement must transfer one initialized Picker exactly once");
        }
        self.entered = true;
        Ok(())
    }
    pub(crate) fn returned(&mut self) -> Result<(), &'static str> {
        if !self.entered || self.completed {
            return Err("replacement completion without one owned transfer");
        }
        self.completed = true;
        Ok(())
    }
    pub(crate) fn complete(&self) -> bool {
        self.entered && self.completed
    }
}

fn root_symbol_matches(observed: &[u8], expected: &[u8]) -> bool {
    !expected.is_empty() && (observed == expected || observed.strip_prefix(b"_") == Some(expected))
}

#[cfg(not(test))]
struct RootReplacementEntries {
    constant: *const c_void,
    setter: *const c_void,
}

#[cfg(not(test))]
impl RootReplacementEntries {
    fn load() -> Self {
        fn exact(symbol: &str, image: &[u8]) -> *const c_void {
            let entry = resolve(symbol);
            category_image(entry, image);
            let mut info = DlInfo::default();
            assert!(unsafe { dladdr(entry, &mut info) } != 0);
            assert!(!info.symbol.is_null() && info.symbol_address == entry);
            let observed = unsafe { std::ffi::CStr::from_ptr(info.symbol) }.to_bytes();
            assert!(root_symbol_matches(observed, symbol.as_bytes()));
            entry
        }
        Self {
            constant: exact("$s7SwiftUI7BindingV8constantyACyxGxFZ", b"/SwiftUICore"),
            setter: exact(
                "$s7SwiftUI19UIHostingControllerC8rootViewxvsTj",
                b"/SwiftUI",
            ),
        }
    }
}

#[cfg(not(test))]
unsafe extern "C" {
    fn audit_binding_constant(
        destination: *mut c_void,
        selection: *mut c_void,
        metadata: *const c_void,
        entry: *const c_void,
    );
    fn audit_hosting_replace_root(
        picker: *mut c_void,
        controller: *mut c_void,
        entry: *const c_void,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_symbol_identity_is_exact() {
        let expected = b"$s7SwiftUI19UIHostingControllerC8rootViewxvsTj";
        assert!(root_symbol_matches(expected, expected));
        let mut macho = vec![b'_'];
        macho.extend_from_slice(expected);
        assert!(root_symbol_matches(&macho, expected));
        for invalid in [
            Vec::new(),
            [expected.as_slice(), b"suffix"].concat(),
            [b"prefix", expected.as_slice()].concat(),
            [b"__", expected.as_slice()].concat(),
            expected[..expected.len() - 2].to_vec(),
        ] {
            assert!(!root_symbol_matches(&invalid, expected));
        }
        assert!(!root_symbol_matches(b"", b""));
        assert!(!root_symbol_matches(b"_", b""));
    }

    #[test]
    fn one_transfer_and_return_are_required() {
        let mut state = RootTransfer::default();
        assert!(!state.complete());
        assert!(state.returned().is_err());
        assert!(state.enter(false).is_err());
        assert!(!state.complete());
        state.enter(true).unwrap();
        assert!(!state.complete());
        assert!(state.enter(true).is_err());
        state.returned().unwrap();
        assert!(state.complete());
        assert!(state.returned().is_err());
    }
}
