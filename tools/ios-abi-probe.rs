//! Default mode: compiler experiment with an undefined external symbol.
//! runtime_probe mode: isolated Simulator getter and async runtime inspection.
//! No authorization request is made by this inspection stage.
#![cfg_attr(not(runtime_probe), no_std)]
#![feature(abi_swift)]
#![cfg_attr(xcode_host, no_main)]

#[cfg(all(
    runtime_probe,
    not(all(target_os = "ios", target_arch = "aarch64", target_abi = "sim"))
))]
compile_error!("runtime inspection is limited to arm64 iOS Simulator");

#[cfg(not(runtime_probe))]
unsafe extern "Swift" {
    fn abi_probe_external();
}

#[unsafe(no_mangle)]
#[cfg(not(runtime_probe))]
pub unsafe extern "C" fn rust_calls_swift_probe() {
    unsafe { abi_probe_external() };
}

#[cfg(runtime_probe)]
#[repr(C)]
struct MetadataResponse {
    value: *const std::ffi::c_void,
    state: usize,
}

#[cfg(runtime_probe)]
#[link(name = "FamilyControls", kind = "framework")]
unsafe extern "Swift" {
    // Non-generic accessor: MetadataRequest -> MetadataResponse (Swift 5.10 ABI).
    #[link_name = env!("AUTH_METADATA_SYMBOL")]
    fn authorization_center_metadata(request: usize) -> MetadataResponse;
}

// A class static getter receives its metatype through swiftself, which uses x20
// on Darwin arm64. This Rust-owned assembly boundary preserves the C caller's
// callee-saved x20 and return address. It makes no type-layout assumptions about
// the Swift object beyond its documented single class-reference pointer.
#[cfg(runtime_probe)]
core::arch::global_asm!(concat!(
    ".text\n.p2align 2\n.globl _rust_shared_with_metadata\n",
    "_rust_shared_with_metadata:\n.cfi_startproc\n",
    "stp x20, x30, [sp, #-16]!\n.cfi_def_cfa_offset 16\n",
    ".cfi_offset 20, -16\n.cfi_offset 30, -8\nmov x20, x0\n",
    "bl _",
    env!("AUTH_SHARED_SYMBOL"),
    "\nldp x20, x30, [sp], #16\n.cfi_restore 20\n.cfi_restore 30\n",
    ".cfi_def_cfa_offset 0\nret\n.cfi_endproc\n",
));

#[cfg(runtime_probe)]
unsafe extern "C" {
    fn rust_shared_with_metadata(metadata: *const std::ffi::c_void) -> *mut std::ffi::c_void;
}

#[cfg(runtime_probe)]
#[link(name = "objc")]
unsafe extern "C" {
    fn object_getClass(object: *mut std::ffi::c_void) -> *const std::ffi::c_void;
    fn class_getName(class: *const std::ffi::c_void) -> *const std::ffi::c_char;
}

#[cfg(runtime_probe)]
fn inspect_singleton() -> *mut std::ffi::c_void {
    // This deliberately does not request authorization or read another app's data.
    // Request 0 means a blocking request for complete metadata.
    let metadata = unsafe { authorization_center_metadata(0) };
    assert_eq!(metadata.state, 0, "SDK class metadata is incomplete");
    assert!(!metadata.value.is_null(), "SDK class metadata is null");
    let object = unsafe { rust_shared_with_metadata(metadata.value) };
    assert!(
        !object.is_null(),
        "SDK getter returned a null class reference"
    );
    let class = unsafe { object_getClass(object) };
    assert!(
        !class.is_null(),
        "SDK reference has no Objective-C runtime class"
    );
    assert_eq!(
        class, metadata.value,
        "SDK getter returned a reference of a different runtime class"
    );
    let name = unsafe { class_getName(class) };
    assert!(!name.is_null(), "SDK runtime class has no name");
    let name = unsafe { std::ffi::CStr::from_ptr(name) }.to_str().unwrap();
    println!("SDK singleton class: {name}");
    object
}

#[cfg(runtime_probe)]
#[cfg(not(xcode_host))]
fn main() {
    #[cfg(uikit_host)]
    native_host::run();
    #[cfg(not(uikit_host))]
    {
        let object = inspect_singleton();
        async_inspection::inspect();
        #[cfg(auth_request)]
        request_probe::run(object);
    }
}

#[cfg(all(uikit_host, not(auth_request)))]
compile_error!("UIKit host inspection requires auth_request");

#[cfg(all(runtime_probe, xcode_host, uikit_host, auth_request))]
#[unsafe(no_mangle)]
pub extern "C" fn main(_: i32, _: *mut *mut std::ffi::c_char) -> i32 {
    native_host::run();
    1
}

#[cfg(all(xcode_host, not(all(runtime_probe, uikit_host, auth_request))))]
compile_error!("Xcode static library host requires the UIKit authorization mode");

#[cfg(all(
    selection_layout_probe,
    not(all(runtime_probe, uikit_host, auth_request, xcode_host))
))]
compile_error!("selection layout inspection requires the Xcode UIKit authorization host");

// Layout and entry-byte evidence only; no Selection or Binding value is built.
#[cfg(selection_layout_probe)]
mod selection_layout {
    use super::MetadataResponse;
    use std::ffi::{CString, c_char, c_void};

    // Swift 5.10 ABI/ValueWitness.def (required witnesses), in declaration order:
    // initializeBufferWithCopyOfBuffer, destroy, initializeWithCopy,
    // assignWithCopy, initializeWithTake, assignWithTake,
    // getEnumTagSinglePayload, storeEnumTagSinglePayload, then the data fields.
    // ABI/ValueWitnessTable.h uses size_t, size_t, uint32_t, uint32_t.
    #[repr(C)]
    struct ValueWitnessTable {
        functions: [*const c_void; 8],
        size: usize,
        stride: usize,
        flags: u32,
        _extra_inhabitant_count: u32,
    }

    // Runtime/HeapObject.h: Swift CC returns object then uninitialized buffer.
    #[repr(C)]
    struct BoxPair {
        object: *mut c_void,
        buffer: *mut c_void,
    }
    #[link(name = "FamilyControls", kind = "framework")]
    #[link(name = "SwiftUI", kind = "framework")]
    unsafe extern "C" {}
    #[link(name = "swiftCore")]
    unsafe extern "Swift" {
        fn swift_allocBox(metadata: *const c_void) -> BoxPair;
    }
    #[link(name = "swiftCore")]
    unsafe extern "C" {
        // These declarations have no SWIFT_CC(swift) in HeapObject.h.
        fn swift_projectBox(object: *mut c_void) -> *mut c_void;
        fn swift_deallocBox(object: *mut c_void);
    }
    #[link(name = "System")]
    unsafe extern "C" {
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        fn getpagesize() -> i32;
    }
    struct UninitializedBox(*mut c_void);
    impl Drop for UninitializedBox {
        fn drop(&mut self) {
            // Never release an uninitialized value through its destroy witness.
            unsafe { swift_deallocBox(self.0) };
        }
    }

    fn resolve(symbol: &str) -> *const c_void {
        let name = CString::new(symbol).unwrap();
        // Darwin dyld include/dlfcn.h: RTLD_DEFAULT is ((void *) -2).
        // Only the four exact SDK symbols checked by the workflow are looked up.
        let pointer = unsafe {
            dlsym(
                std::ptr::without_provenance_mut(usize::MAX - 1),
                name.as_ptr(),
            )
        };
        assert!(
            !pointer.is_null(),
            "required public selection symbol is missing"
        );
        assert!(pointer.addr() % 4 == 0, "unaligned arm64 public entry");
        pointer
    }

    fn page_remaining(pointer: *const c_void, page: usize) -> usize {
        assert!(!pointer.is_null(), "null inspection address");
        page - pointer.addr() % page
    }
    unsafe fn read<T: Copy>(pointer: *const T, page: usize) -> T {
        assert!(
            pointer.addr() % std::mem::align_of::<T>() == 0,
            "unaligned ABI field"
        );
        assert!(
            page_remaining(pointer.cast(), page) >= std::mem::size_of::<T>(),
            "ABI field crosses page boundary"
        );
        unsafe { pointer.read() }
    }
    fn entry_bytes(label: &str, pointer: *const c_void, page: usize) {
        // This is a bounded entry prefix, not a claim about argument registers,
        // ownership, or a whole function. Do not follow branches or call it.
        let count = 128.min(page_remaining(pointer, page));
        let bytes = unsafe { std::slice::from_raw_parts(pointer.cast::<u8>(), count) };
        print!("Selection entry bytes: {label}; length={count}; hex=");
        for byte in bytes {
            print!("{byte:02x}");
        }
        println!();
    }

    fn layout(label: &str, response: MetadataResponse, page: usize) -> usize {
        assert!(response.state == 0, "incomplete selection metadata");
        assert!(!response.value.is_null(), "null selection metadata");
        let metadata = response.value.cast::<usize>();
        // Swift 5.10 ABI/MetadataKind.def: Struct = MetadataKindIsNonHeap,
        // ABI/MetadataValues.h: MetadataKindIsNonHeap = 0x200.
        assert!(
            unsafe { read(metadata, page) } == 0x200,
            "expected struct metadata"
        );
        // ABI/Metadata.h: FullMetadata inherits HeaderType before Metadata;
        // HeaderType's final word is ValueWitnesses. The layout-string prefix
        // is before that word, so VWT remains at canonical metadata minus one.
        let witnesses = unsafe {
            read(
                metadata.cast::<*const ValueWitnessTable>().wrapping_sub(1),
                page,
            )
        };
        assert!(!witnesses.is_null(), "null selection value witnesses");
        let size = unsafe { read(&raw const (*witnesses).size, page) };
        let stride = unsafe { read(&raw const (*witnesses).stride, page) };
        let flags = unsafe { read(&raw const (*witnesses).flags, page) };
        // MetadataValues.h: AlignmentMask=0xff, Incomplete=0x00400000.
        assert!(flags & 0x0040_0000 == 0, "incomplete value layout");
        let alignment = (flags as usize & 0xff) + 1;
        assert!(alignment.is_power_of_two(), "invalid value alignment");
        let expected_stride = size.checked_add(alignment - 1).unwrap() & !(alignment - 1);
        assert!(stride == expected_stride.max(1), "invalid value stride");
        println!(
            "Selection layout: {label}; size={size}; stride={stride}; alignment={alignment}; metadata_complete=true; kind_struct=true"
        );
        // Slots 2 and 3 are initializeWithCopy and assignWithCopy. Read only
        // their addresses; no VWT function is called and no value is accessed.
        for (slot, name) in [(2, "initializeWithCopy"), (3, "assignWithCopy")] {
            let entry = unsafe {
                read(
                    (&raw const (*witnesses).functions)
                        .cast::<*const c_void>()
                        .wrapping_add(slot),
                    page,
                )
            };
            assert!(entry.addr() % 4 == 0, "unaligned arm64 value witness entry");
            entry_bytes(&format!("{label}.{name}"), entry, page);
        }
        alignment
    }

    pub(super) fn inspect() {
        let page = unsafe { getpagesize() };
        assert!(page > 0, "invalid Darwin page size");
        let page = page as usize;
        // ABI/Metadata.h MetadataAccessFunction::operator(): Fn0 and Fn1
        // both use Swift CC, with request before the direct generic argument.
        // SDK Binding's <Value> has no conformance requirements.
        let selection_accessor: unsafe extern "Swift" fn(usize) -> MetadataResponse =
            unsafe { std::mem::transmute(resolve(env!("SELECTION_METADATA_SYMBOL"))) };
        let binding_accessor: unsafe extern "Swift" fn(usize, *const c_void) -> MetadataResponse =
            unsafe { std::mem::transmute(resolve(env!("BINDING_METADATA_SYMBOL"))) };
        // MetadataRequest(0) requests complete metadata synchronously.
        let selection = unsafe { selection_accessor(0) };
        let selection_pointer = selection.value;
        let alignment = layout("FamilyActivitySelection", selection, page);
        let binding = unsafe { binding_accessor(0, selection_pointer) };
        layout("Binding<FamilyActivitySelection>", binding, page);

        let pair = unsafe { swift_allocBox(selection_pointer) };
        assert!(!pair.object.is_null(), "null selection box object");
        let owner = UninitializedBox(pair.object);
        assert!(
            pair.object.addr() % std::mem::align_of::<*mut c_void>() == 0,
            "unaligned box object"
        );
        assert!(!pair.buffer.is_null(), "null selection box buffer");
        assert!(
            pair.buffer.addr() % alignment == 0,
            "unaligned selection box buffer"
        );
        let projected = unsafe { swift_projectBox(owner.0) };
        assert!(
            !projected.is_null() && projected == pair.buffer,
            "box projection mismatch"
        );
        drop(owner);
        println!(
            "Selection box: buffer_aligned=true; project_matches=true; uninitialized_deallocated=true"
        );
        entry_bytes(
            "FamilyActivitySelection.init()",
            resolve(env!("SELECTION_INIT_SYMBOL")),
            page,
        );
        entry_bytes(
            "Binding.init(get:set:) without Transaction",
            resolve(env!("BINDING_INIT_SYMBOL")),
            page,
        );
        println!(
            "Selection layout inspection complete; selection_values_constructed=0; selection_values_read=0; initializer_calls=0; closure_calls=0; value_witness_calls=0; authorization_status_verified=false; monitoring=false; shielding=false"
        );
    }
}

#[cfg(runtime_probe)]
#[cfg_attr(uikit_host, allow(dead_code))]
mod async_inspection {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    // Swift 5.10 include/swift/ABI/Executor.h: AsyncFunctionPointer.
    // This probe is limited to non-pointer-authenticated arm64 Simulator code.
    #[repr(C)]
    struct AsyncFunctionPointer {
        entry_offset: i32,
        context_size: u32,
    }

    #[repr(C)]
    pub(super) struct TaskAndContext {
        pub(super) task: *mut c_void,
        pub(super) context: *mut c_void,
    }

    #[link(name = "FamilyControls", kind = "framework")]
    unsafe extern "C" {
        #[link_name = env!("AUTH_ASYNC_SYMBOL")]
        static AUTHORIZATION_POINTER: AsyncFunctionPointer;
    }

    #[link(name = "swift_Concurrency")]
    unsafe extern "Swift" {
        pub(super) fn swift_task_create_common(
            flags: usize,
            options: *const c_void,
            result_type: *const c_void,
            entry: *const c_void,
            closure: *const c_void,
            context_size: usize,
        ) -> TaskAndContext;
    }

    #[link(name = "swiftCore")]
    unsafe extern "Swift" {
        fn swift_release(object: *mut c_void);
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        static kCFRunLoopDefaultMode: *const c_void;
        fn CFRunLoopRunInMode(mode: *const c_void, seconds: f64, return_after_source: u8) -> i32;
    }

    unsafe extern "C" {
        fn rust_async_smoke_entry();
    }

    static ENTERED: AtomicBool = AtomicBool::new(false);

    #[unsafe(no_mangle)]
    extern "C" fn rust_async_smoke_report() {
        ENTERED.store(true, Ordering::Release);
    }

    // Runtime non_future_adapter passes the initial context in x22, and its
    // null closure in x20. AsyncContext.ResumeParent is at byte offset 8.
    // Swift 5.10 Task.cpp installs completeTaskAndRelease here. Its error
    // argument uses swiftself/x20, NOT the synchronous swifterror/x21.
    // All synchronous stack storage is removed before the async tail call.
    core::arch::global_asm!(
        ".text",
        ".p2align 2",
        ".globl _rust_async_smoke_entry",
        "_rust_async_smoke_entry:",
        "stp x29, x30, [sp, #-16]!",
        "mov x29, sp",
        "bl _rust_async_smoke_report",
        "ldp x29, x30, [sp], #16",
        "ldr x16, [x22, #8]",
        "mov x20, #0",
        "br x16",
    );

    pub fn inspect() {
        let pointer = &raw const AUTHORIZATION_POINTER;
        let descriptor = unsafe { &*pointer };
        assert_ne!(descriptor.entry_offset, 0, "null relative async entry");
        assert!(descriptor.context_size >= 16 && descriptor.context_size <= 1_048_576);
        let entry = pointer
            .cast::<u8>()
            .wrapping_offset(descriptor.entry_offset as isize);
        assert_eq!(entry.addr() % 4, 0, "unaligned arm64 async entry");
        println!(
            "Public async descriptor: offset={}, context_size={}",
            descriptor.entry_offset, descriptor.context_size
        );

        // EnqueueJob bit 12 from Swift 5.10 MetadataValues.h. No state is
        // written to InitialContext after enqueue: only runtime initializes it.
        // A null closure keeps the runtime from releasing a Rust allocation.
        let task = unsafe {
            swift_task_create_common(
                1 << 12,
                std::ptr::null(),
                std::ptr::null(),
                rust_async_smoke_entry as *const c_void,
                std::ptr::null(),
                32,
            )
        };
        assert!(!task.task.is_null() && !task.context.is_null());
        // Drop the create caller's +1; queued/running task owns a separate +1.
        unsafe { swift_release(task.task) };
        let deadline = Instant::now() + Duration::from_secs(10);
        while !ENTERED.load(Ordering::Acquire) && Instant::now() < deadline {
            unsafe { CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.1, 1) };
        }
        assert!(
            ENTERED.load(Ordering::Acquire),
            "Swift runtime task entry timed out"
        );
        println!("Rust async task entry observed; authorization_requested=false");
        // ENTERED proves entry only. It is not a runtime teardown assertion.
        unsafe { CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.1, 0) };
    }
}

// Prepared separately while the descriptor inspection runs. Merge into the
// existing two-file audit only after the real SDK descriptor is confirmed.
#[cfg(all(runtime_probe, auth_request))]
#[cfg_attr(selection_layout_probe, allow(dead_code))]
mod request_probe {
    use super::async_inspection::swift_task_create_common;
    use std::ffi::c_void;
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    #[repr(C)]
    struct Descriptor {
        offset: i32,
        size: u32,
    }
    #[repr(C)]
    struct Executor {
        identity: *const c_void,
        implementation: usize,
    }
    #[repr(C)]
    struct ExecutorOption {
        flags: usize,
        parent: *const c_void,
        executor: Executor,
    }
    #[repr(C, align(16))]
    struct Root {
        parent: *mut c_void,
        resume_parent: *const c_void,
        child: *mut c_void,
        entry: *const c_void,
        object: *mut c_void,
        member: isize,
    }
    #[repr(C, align(16))]
    struct Header {
        parent: *mut Root,
        resume_parent: *const c_void,
    }
    struct Input {
        entry: usize,
        size: usize,
        object: usize,
        member: isize,
    }
    static INPUT: OnceLock<Input> = OnceLock::new();
    // 0 pending, 1 returned normally, 2 returned an error. Neither outcome is
    // represented as verified authorization status, monitoring, or shielding.
    static RESULT: AtomicU8 = AtomicU8::new(0);
    static CALLER_ROOT: AtomicUsize = AtomicUsize::new(0);

    #[link(name = "FamilyControls", kind = "framework")]
    unsafe extern "C" {
        #[link_name = env!("AUTH_ASYNC_SYMBOL")]
        static REQUEST: Descriptor;
    }
    #[link(name = "swift_Concurrency")]
    unsafe extern "Swift" {
        fn swift_task_alloc(size: usize) -> *mut c_void;
        fn swift_task_dealloc(pointer: *mut c_void);
        fn swift_task_getMainExecutor() -> Executor;
        fn swift_task_cancel(task: *mut c_void);
    }
    #[link(name = "swiftCore")]
    unsafe extern "Swift" {
        fn swift_release(object: *mut c_void);
    }
    #[link(name = "swiftCore")]
    unsafe extern "Swift" {
        // Swift 5.10 ErrorObject.h/mm: compiler runtime bridge consumes an
        // owned Error box and returns an owned NSError. This is version-pinned
        // Swift runtime SPI, not a FamilyControls private authorization API.
        fn _swift_stdlib_bridgeErrorToNSError(error: *mut c_void) -> *mut c_void;
    }
    #[link(name = "Foundation", kind = "framework")]
    unsafe extern "C" {}
    #[link(name = "objc")]
    unsafe extern "C" {
        fn sel_registerName(name: *const std::ffi::c_char) -> *const c_void;
        // These three zero-argument selectors return a pointer or NSInteger:
        // all use one integer result register on this arm64 target.
        pub(super) fn objc_msgSend(object: *mut c_void, selector: *const c_void) -> usize;
        fn objc_release(object: *mut c_void);
    }
    #[cfg(not(uikit_host))]
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        static kCFRunLoopDefaultMode: *const c_void;
        fn CFRunLoopRunInMode(mode: *const c_void, seconds: f64, return_after_source: u8) -> i32;
    }
    unsafe extern "C" {
        fn rust_authorization_entry();
        fn rust_authorization_resume();
    }
    pub(super) struct TaskOwner(*mut c_void);
    // The retained task handle supports concurrent cancellation and ARC; no
    // task-context allocator or UIKit object is accessed through this handle.
    #[cfg(uikit_host)]
    unsafe impl Send for TaskOwner {}
    impl TaskOwner {
        fn cancel(&self) {
            unsafe { swift_task_cancel(self.0) };
        }
    }
    impl Drop for TaskOwner {
        fn drop(&mut self) {
            unsafe { swift_release(self.0) };
        }
    }

    // Called synchronously within the active Swift task. All Rust input was
    // initialized before task creation, so enqueue cannot race initialization.
    #[unsafe(no_mangle)]
    unsafe extern "C" fn rust_authorization_prepare(root: *mut Root) -> *mut c_void {
        CALLER_ROOT.store(root.addr(), Ordering::Release);
        let input = INPUT.get().unwrap();
        let child = unsafe { swift_task_alloc(input.size) };
        assert!(!child.is_null());
        unsafe {
            child.cast::<Header>().write(Header {
                parent: root,
                resume_parent: rust_authorization_resume as *const c_void,
            });
            (*root).child = child;
            (*root).entry = input.entry as *const c_void;
            (*root).object = input.object as *mut c_void;
            (*root).member = input.member;
        }
        child
    }

    #[unsafe(no_mangle)]
    unsafe extern "C" fn rust_authorization_finish(root: *mut Root, error: *mut c_void) {
        assert!(!root.is_null());
        assert_eq!(
            root.addr(),
            CALLER_ROOT.load(Ordering::Acquire),
            "async resume did not project the original caller frame"
        );
        println!("SDK async continuation caller frame validated");
        let child = unsafe { (*root).child };
        // The callee has returned to its parent; its task allocation is now the
        // last allocation. Release it before completing the caller task.
        unsafe { swift_task_dealloc(child) };
        let result = if error.is_null() {
            1
        } else {
            // Do not message an uninitialized native Swift error box. The
            // runtime bridge lazily creates its Cocoa domain/code first.
            let ns_error = unsafe { _swift_stdlib_bridgeErrorToNSError(error) };
            assert!(!ns_error.is_null());
            let domain = unsafe { objc_msgSend(ns_error, sel_registerName(c"domain".as_ptr())) };
            assert_ne!(domain, 0);
            let utf8 = unsafe {
                objc_msgSend(
                    domain as *mut c_void,
                    sel_registerName(c"UTF8String".as_ptr()),
                )
            };
            assert_ne!(utf8, 0);
            let domain = unsafe { std::ffi::CStr::from_ptr(utf8 as *const std::ffi::c_char) };
            let code =
                unsafe { objc_msgSend(ns_error, sel_registerName(c"code".as_ptr())) } as isize;
            // Only system error domain/code; no userInfo, application tokens,
            // account identity, or personal application data is read/logged.
            println!(
                "Public authorization NSError: domain={}, code={code}",
                domain.to_string_lossy()
            );
            unsafe { objc_release(ns_error) };
            2
        };
        RESULT.store(result, Ordering::Release);
    }

    // Swift 5.10 async convention on arm64: x22 context, x20 swiftself.
    // FamilyControlsMember is the SDK's @objc Swift.Int enum, passed in x0
    // with the C-compatible layout of GenEnum.cpp in Swift 5.10.
    // A Void throwing continuation receives its error in x20 and the callee
    // context in x22. Swift 5.10 GenCall.cpp emitAsyncReturn passes its CURRENT
    // context; the caller's continuation projects the parent before accessing
    // its own frame. Task.h's manual resumeParent helper is a different adapter.
    core::arch::global_asm!(
        ".text",
        ".p2align 2",
        ".globl _rust_authorization_entry",
        "_rust_authorization_entry:",
        "stp x29, x30, [sp, #-16]!",
        "mov x29, sp",
        "mov x0, x22",
        "bl _rust_authorization_prepare",
        "mov x17, x0",
        "ldr x16, [x22, #24]",
        "ldr x20, [x22, #32]",
        "ldr x0, [x22, #40]",
        "mov x22, x17",
        "ldp x29, x30, [sp], #16",
        "br x16",
        ".p2align 2",
        ".globl _rust_authorization_resume",
        "_rust_authorization_resume:",
        "stp x29, x30, [sp, #-16]!",
        "mov x29, sp",
        "ldr x22, [x22, #0]",
        "mov x0, x22",
        "mov x1, x20",
        "bl _rust_authorization_finish",
        "ldr x16, [x22, #8]",
        "mov x20, #0",
        "ldp x29, x30, [sp], #16",
        "br x16",
    );

    pub(super) fn start(object: *mut c_void) -> TaskOwner {
        let descriptor = &raw const REQUEST;
        let descriptor_value = unsafe { &*descriptor };
        assert!(descriptor_value.size >= 16 && descriptor_value.size <= 1_048_576);
        assert_ne!(descriptor_value.offset, 0);
        let entry = descriptor
            .cast::<u8>()
            .wrapping_offset(descriptor_value.offset as isize);
        assert_eq!(entry.addr() % 4, 0);
        let member: isize = env!("AUTH_INDIVIDUAL_VALUE").parse().unwrap();
        let input = Input {
            entry: entry.addr(),
            size: descriptor_value.size as usize,
            object: object.addr(),
            member,
        };
        assert!(INPUT.set(input).is_ok());
        assert_eq!(std::mem::size_of::<Header>(), 16);
        assert_eq!(std::mem::offset_of!(Root, child), 16);
        assert_eq!(std::mem::offset_of!(Root, entry), 24);
        assert_eq!(std::mem::offset_of!(Root, object), 32);
        assert_eq!(std::mem::offset_of!(Root, member), 40);
        let executor = unsafe { swift_task_getMainExecutor() };
        assert!(!executor.identity.is_null());
        // TaskOptionRecordKind::Executor = 0. The runtime reads the record
        // synchronously and copies ExecutorRef before enqueueing (Task.cpp).
        let option = ExecutorOption {
            flags: 0,
            parent: std::ptr::null(),
            executor,
        };
        println!("Public individual authorization request starting; member={member}");
        let task = unsafe {
            swift_task_create_common(
                1 << 12,
                (&raw const option).cast(),
                std::ptr::null(),
                rust_authorization_entry as *const c_void,
                std::ptr::null(),
                std::mem::size_of::<Root>(),
            )
        };
        assert!(!task.task.is_null() && !task.context.is_null());
        // Keep the create caller's reference until the bounded wait ends, so
        // cancellation on timeout never uses an already-deallocated task.
        TaskOwner(task.task)
    }

    #[cfg(not(uikit_host))]
    pub fn run(object: *mut c_void) {
        let deadline = Instant::now() + Duration::from_secs(20);
        let owner = start(object);
        while RESULT.load(Ordering::Acquire) == 0 && Instant::now() < deadline {
            unsafe { CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.1, 1) };
        }
        let result = RESULT.load(Ordering::Acquire);
        if result == 0 {
            owner.cancel();
            let cancel_deadline = Instant::now() + Duration::from_secs(5);
            while RESULT.load(Ordering::Acquire) == 0 && Instant::now() < cancel_deadline {
                unsafe { CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.1, 1) };
            }
            // A cancellation response is not an accepted authorization flow.
            panic!(
                "public authorization continuation timed out; cancellation requested; process will exit"
            );
        }
        assert_ne!(result, 0, "public authorization continuation timed out");
        println!(
            "Public individual authorization continuation observed; error_present={}",
            result == 2
        );
        println!("authorization_status_verified=false; monitoring=false; shielding=false");
        unsafe { CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.1, 0) };
    }

    #[cfg(uikit_host)]
    pub(super) fn wait_from_worker(owner: TaskOwner, started: Instant) -> bool {
        let deadline = started + Duration::from_secs(20);
        while RESULT.load(Ordering::Acquire) == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        let result = RESULT.load(Ordering::Acquire);
        if result == 0 {
            owner.cancel();
            let deadline = Instant::now() + Duration::from_secs(5);
            while RESULT.load(Ordering::Acquire) == 0 && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(50));
            }
            eprintln!("UIKit authorization wait timed out; cancellation requested");
            return false;
        }
        println!(
            "Public individual authorization continuation observed; error_present={}",
            result == 2
        );
        println!("authorization_status_verified=false; monitoring=false; shielding=false");
        true
    }
}

#[cfg(all(runtime_probe, uikit_host, auth_request))]
mod native_host {
    use super::request_probe::objc_msgSend;
    use std::ffi::{CString, c_char, c_void};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::Duration;
    #[cfg(not(selection_layout_probe))]
    use std::time::Instant;
    type Id = *mut c_void;
    type Sel = *const c_void;
    #[derive(Clone, Copy)]
    #[repr(C)]
    struct Rect {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    }
    static ACTIVE: AtomicBool = AtomicBool::new(false);
    static WINDOW: AtomicUsize = AtomicUsize::new(0);
    static DELEGATE: AtomicUsize = AtomicUsize::new(0);
    #[link(name = "UIKit", kind = "framework")]
    unsafe extern "C" {
        fn UIApplicationMain(argc: i32, argv: *mut *mut c_char, principal: Id, delegate: Id)
        -> i32;
    }
    #[link(name = "objc")]
    unsafe extern "C" {
        fn objc_getClass(name: *const c_char) -> Id;
        fn objc_getProtocol(name: *const c_char) -> Id;
        fn objc_allocateClassPair(superclass: Id, name: *const c_char, extra: usize) -> Id;
        fn objc_registerClassPair(class: Id);
        fn class_addProtocol(class: Id, protocol: Id) -> bool;
        fn class_addMethod(
            class: Id,
            selector: Sel,
            implementation: *const c_void,
            types: *const c_char,
        ) -> bool;
        fn sel_registerName(name: *const c_char) -> Sel;
        fn objc_retain(object: Id) -> Id;
    }
    #[link(name = "System")]
    unsafe extern "C" {
        // dispatch/queue.h exposes dispatch_get_main_queue as a C inline
        // function returning DISPATCH_GLOBAL_OBJECT(..., _dispatch_main_q).
        // Only take the address of this opaque global; never inspect its layout.
        static _dispatch_main_q: [u8; 0];
        fn dispatch_async_f(
            queue: Id,
            context: *mut c_void,
            function: unsafe extern "C" fn(*mut c_void),
        );
    }
    unsafe fn send0(object: Id, name: &std::ffi::CStr) -> Id {
        let f: unsafe extern "C" fn(Id, Sel) -> Id =
            unsafe { std::mem::transmute(objc_msgSend as unsafe extern "C" fn(Id, Sel) -> usize) };
        unsafe { f(object, sel_registerName(name.as_ptr())) }
    }
    unsafe fn send_id(object: Id, name: &std::ffi::CStr, value: Id) {
        let f: unsafe extern "C" fn(Id, Sel, Id) =
            unsafe { std::mem::transmute(objc_msgSend as unsafe extern "C" fn(Id, Sel) -> usize) };
        unsafe { f(object, sel_registerName(name.as_ptr()), value) };
    }
    unsafe fn send_void(object: Id, name: &std::ffi::CStr) {
        let f: unsafe extern "C" fn(Id, Sel) =
            unsafe { std::mem::transmute(objc_msgSend as unsafe extern "C" fn(Id, Sel) -> usize) };
        unsafe { f(object, sel_registerName(name.as_ptr())) };
    }
    unsafe extern "C" fn window(_: Id, _: Sel) -> Id {
        WINDOW.load(Ordering::Acquire) as Id
    }
    unsafe extern "C" fn launched(delegate: Id, _: Sel, _: Id, _: Id) -> bool {
        assert!(!delegate.is_null());
        DELEGATE.store(unsafe { objc_retain(delegate) }.addr(), Ordering::Release);
        let screen = unsafe { send0(objc_getClass(c"UIScreen".as_ptr()), c"mainScreen") };
        assert!(!screen.is_null());
        let bounds: unsafe extern "C" fn(Id, Sel) -> Rect =
            unsafe { std::mem::transmute(objc_msgSend as unsafe extern "C" fn(Id, Sel) -> usize) };
        let rect = unsafe { bounds(screen, sel_registerName(c"bounds".as_ptr())) };
        assert!(rect.width > 0.0 && rect.height > 0.0);
        let allocated = unsafe { send0(objc_getClass(c"UIWindow".as_ptr()), c"alloc") };
        let init: unsafe extern "C" fn(Id, Sel, Rect) -> Id =
            unsafe { std::mem::transmute(objc_msgSend as unsafe extern "C" fn(Id, Sel) -> usize) };
        let view_window = unsafe {
            init(
                allocated,
                sel_registerName(c"initWithFrame:".as_ptr()),
                rect,
            )
        };
        assert!(!view_window.is_null());
        WINDOW.store(view_window.addr(), Ordering::Release);
        let controller = unsafe {
            send0(
                send0(objc_getClass(c"UIViewController".as_ptr()), c"alloc"),
                c"init",
            )
        };
        assert!(!controller.is_null());
        unsafe {
            send_id(view_window, c"setRootViewController:", controller);
            send_void(view_window, c"makeKeyAndVisible");
        }
        // Keep the allocation references for this bounded inspection process.
        println!("Rust UIKit launch callback observed; blank_window=true");
        true
    }
    unsafe extern "C" fn exit_on_main(context: *mut c_void) {
        std::process::exit(context.addr() as i32);
    }
    unsafe extern "C" fn active(_: Id, _: Sel, _: Id) {
        if ACTIVE.swap(true, Ordering::AcqRel) {
            return;
        }
        #[cfg(not(selection_layout_probe))]
        let started = Instant::now();
        println!("Rust UIKit active callback observed");
        let object = super::inspect_singleton();
        #[cfg(selection_layout_probe)]
        {
            // Keep the getter's allocation reference for this bounded process,
            // as with its window/controller. This mode starts no Swift task.
            let _ = object;
            super::selection_layout::inspect();
            unsafe {
                dispatch_async_f(
                    (&raw const _dispatch_main_q).cast_mut().cast(),
                    std::ptr::null_mut(),
                    exit_on_main,
                )
            };
        }
        #[cfg(not(selection_layout_probe))]
        {
            let owner = super::request_probe::start(object);
            // Return to UIKit immediately; its main executor must remain available
            // to run the Swift task. The worker only polls an atomic result and
            // cancels a retained task; it never accesses UIKit or task allocations.
            std::thread::spawn(move || {
                let success = super::request_probe::wait_from_worker(owner, started);
                let code = if success { 0 } else { 1 };
                unsafe {
                    dispatch_async_f(
                        (&raw const _dispatch_main_q).cast_mut().cast(),
                        std::ptr::without_provenance_mut(code),
                        exit_on_main,
                    )
                };
            });
        }
    }
    pub(super) fn run() {
        let name = c"PhoneControl1614ProbeDelegate";
        unsafe {
            let class =
                objc_allocateClassPair(objc_getClass(c"NSObject".as_ptr()), name.as_ptr(), 0);
            assert!(!class.is_null());
            assert!(class_addProtocol(
                class,
                objc_getProtocol(c"UIApplicationDelegate".as_ptr())
            ));
            assert!(class_addMethod(
                class,
                sel_registerName(c"application:didFinishLaunchingWithOptions:".as_ptr()),
                launched as *const c_void,
                c"B@:@@".as_ptr()
            ));
            assert!(class_addMethod(
                class,
                sel_registerName(c"applicationDidBecomeActive:".as_ptr()),
                active as *const c_void,
                c"v@:@".as_ptr()
            ));
            assert!(class_addMethod(
                class,
                sel_registerName(c"window".as_ptr()),
                window as *const c_void,
                c"@@:".as_ptr()
            ));
            objc_registerClassPair(class);
            let string_class = objc_getClass(c"NSString".as_ptr());
            let init_string: unsafe extern "C" fn(Id, Sel, *const c_char) -> Id =
                std::mem::transmute(objc_msgSend as unsafe extern "C" fn(Id, Sel) -> usize);
            let delegate_name = init_string(
                send0(string_class, c"alloc"),
                sel_registerName(c"initWithUTF8String:".as_ptr()),
                name.as_ptr(),
            );
            assert!(!delegate_name.is_null());
            std::thread::spawn(|| {
                std::thread::sleep(Duration::from_secs(30));
                if !ACTIVE.load(Ordering::Acquire) {
                    eprintln!("UIKit active callback timed out");
                    std::process::exit(1);
                }
            });
            let executable = CString::new("authorization-center-probe").unwrap();
            let mut argv = [executable.as_ptr().cast_mut(), std::ptr::null_mut()];
            let result =
                UIApplicationMain(1, argv.as_mut_ptr(), std::ptr::null_mut(), delegate_name);
            panic!("UIApplicationMain unexpectedly returned {result}");
        }
    }
}
