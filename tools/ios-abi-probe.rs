//! Default mode: compiler experiment with an undefined external symbol.
//! runtime_probe mode: isolated Simulator getter and async runtime inspection.
//! No authorization request is made by this inspection stage.
#![cfg_attr(not(runtime_probe), no_std)]
#![feature(abi_swift)]

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
fn main() {
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
    async_inspection::inspect();
    // Keep the singleton reference until this isolated process ends.
}

#[cfg(runtime_probe)]
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
    struct TaskAndContext {
        task: *mut c_void,
        context: *mut c_void,
    }

    #[link(name = "FamilyControls", kind = "framework")]
    unsafe extern "C" {
        #[link_name = env!("AUTH_ASYNC_SYMBOL")]
        static AUTHORIZATION_POINTER: AsyncFunctionPointer;
    }

    #[link(name = "swift_Concurrency")]
    unsafe extern "Swift" {
        fn swift_task_create_common(
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
