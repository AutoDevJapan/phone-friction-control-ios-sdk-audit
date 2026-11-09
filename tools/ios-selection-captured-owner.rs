// Opt-in captured Selection owner. Not promoted to the live audit or accepted
// on an Apple runtime. Intended mutable_binding scope.
// Owns the existing Installed<Selection> through an allocated Swift closure
// context. Based on the separately executed Bool owner ABI; the nontrivial
// state/callback/wiring delta still requires independent review.

#[repr(C)]
struct SelectionCaptureLocalMetadata {
    kind: usize,
    first_capture: u32,
    description: *const c_void,
}
#[repr(C)]
struct SelectionCaptureFullMetadata {
    layout: *const c_void,
    destroy: unsafe extern "C" fn(),
    witnesses: *const c_void,
    local: SelectionCaptureLocalMetadata,
}
unsafe impl Sync for SelectionCaptureFullMetadata {}

unsafe extern "C" {
    fn rust_selection_capture_destroy();
}
static SELECTION_CAPTURE_METADATA: SelectionCaptureFullMetadata = SelectionCaptureFullMetadata {
    layout: std::ptr::null(),
    destroy: rust_selection_capture_destroy,
    witnesses: std::ptr::null(),
    local: SelectionCaptureLocalMetadata {
        kind: 0x400,
        first_capture: 16,
        description: std::ptr::null(),
    },
};
#[link(name = "swiftCore")]
unsafe extern "C" {
    fn swift_allocObject(
        metadata: *const SelectionCaptureLocalMetadata,
        size: usize,
        alignment: usize,
    ) -> *mut c_void;
    fn swift_deallocObject(object: *mut c_void, size: usize, alignment: usize);
    fn swift_retain(object: *mut c_void) -> *mut c_void;
    fn swift_release(object: *mut c_void);
}
#[link(name = "System")]
unsafe extern "C" {
    #[link_name = "_dispatch_main_q"]
    static SELECTION_CAPTURE_MAIN_QUEUE: [u8; 0];
    fn dispatch_async_f(
        queue: *mut c_void,
        context: *mut c_void,
        work: unsafe extern "C" fn(*mut c_void),
    );
}

static SELECTION_CAPTURE_ID: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
// This atomic represents exactly the initial +1 returned by swift_allocObject.
// The two closure input +1 references are separately retained then transferred.
static SELECTION_CAPTURE_ROOT: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static SELECTION_CAPTURE_CLOSING: AtomicBool = AtomicBool::new(false);
static SELECTION_CAPTURE_DESTROY_ENTERED: AtomicUsize = AtomicUsize::new(0);
static SELECTION_CAPTURE_DEALLOC_COMPLETED: AtomicUsize = AtomicUsize::new(0);
static SELECTION_CAPTURE_STATE_DROPS: AtomicUsize = AtomicUsize::new(0);
static SELECTION_CAPTURE_DEFERRED_DROPS: AtomicUsize = AtomicUsize::new(0);
static SELECTION_CAPTURE_INPUT_RETAINS: AtomicUsize = AtomicUsize::new(0);
static SELECTION_CAPTURE_ROOT_RELEASES: AtomicUsize = AtomicUsize::new(0);
static SELECTION_CAPTURE_CLOSED_GETS: AtomicUsize = AtomicUsize::new(0);
static SELECTION_CAPTURE_REJECTED_SETS: AtomicUsize = AtomicUsize::new(0);
static SELECTION_CAPTURE_ALLOCATED: AtomicUsize = AtomicUsize::new(0);
static SELECTION_CAPTURE_VALIDATED_CALLBACKS: AtomicUsize = AtomicUsize::new(0);
static SELECTION_CAPTURE_BINDING_COPY: AtomicPtr<Value<'static>> =
    AtomicPtr::new(std::ptr::null_mut());

struct SelectionCapturePayload {
    installed: Option<Box<Installed<'static>>>,
}
impl Drop for SelectionCapturePayload {
    fn drop(&mut self) {
        assert!(unsafe { pthread_main_np() } == 1);
        // Installed unregisters STATE and drops the initialized Selection via
        // its real VWT. Count completion only after that drop returns.
        drop(self.installed.take());
        SELECTION_CAPTURE_STATE_DROPS.fetch_add(1, Ordering::SeqCst);
    }
}

core::arch::global_asm!(
    ".text",
    ".p2align 2",
    ".globl _rust_selection_capture_destroy",
    "_rust_selection_capture_destroy:",
    ".cfi_startproc",
    "stp x29, x30, [sp, #-16]!",
    ".cfi_def_cfa_offset 16",
    ".cfi_offset 29, -16",
    ".cfi_offset 30, -8",
    "mov x29, sp",
    "mov x0, x20",
    "bl _rust_selection_capture_destroy_c",
    "ldp x29, x30, [sp], #16",
    ".cfi_restore 29",
    ".cfi_restore 30",
    ".cfi_def_cfa_offset 0",
    "ret",
    ".cfi_endproc",
    ".globl _rust_selection_capture_binding_init",
    "_rust_selection_capture_binding_init:",
    ".cfi_startproc",
    "stp x20, x30, [sp, #-16]!",
    ".cfi_def_cfa_offset 16",
    ".cfi_offset 20, -16",
    ".cfi_offset 30, -8",
    "mov x8, x0",
    "mov x9, x4",
    "mov x10, x5",
    "mov x0, x1",
    "mov x1, x10",
    "mov x4, x3",
    "mov x3, x10",
    "blr x9",
    "ldp x20, x30, [sp], #16",
    ".cfi_restore 20",
    ".cfi_restore 30",
    ".cfi_def_cfa_offset 0",
    "ret",
    ".cfi_endproc",
);
unsafe extern "C" {
    // Exact same six-argument bridge layout as the executed Bool owner probe.
    fn rust_selection_capture_binding_init(
        dst: *mut c_void,
        get: *const c_void,
        set: *const c_void,
        value_metadata: *const c_void,
        entry: *const c_void,
        context: *mut c_void,
    );
}

unsafe extern "C" fn finish_selection_capture_payload(payload: *mut c_void) {
    // Do not reconstruct an owning Box until the existing Installed drop
    // preconditions are valid. A panic while dropping Installed would otherwise
    // still drop its State field during unwinding.
    if unsafe { pthread_main_np() } != 1 || payload.is_null() {
        eprintln!("Selection capture finalizer context invalid");
        std::process::exit(1);
    }
    let record = unsafe { &*payload.cast::<SelectionCapturePayload>() };
    let Some(installed) = record.installed.as_ref() else {
        eprintln!("Selection capture payload already consumed");
        std::process::exit(1);
    };
    let expected = std::ptr::from_ref(&*installed.state).cast_mut().cast();
    if STATE.load(Ordering::Acquire) != expected
        || installed.state.proof.n(Count::Active) != 0
        || COUNT_READING.load(Ordering::Acquire)
    {
        eprintln!("Selection capture state is not quiescent");
        std::process::exit(1);
    }
    let outcome = std::panic::catch_unwind(|| {
        unsafe { drop(Box::from_raw(payload.cast::<SelectionCapturePayload>())) };
    });
    if outcome.is_err() {
        eprintln!("Selection capture state finalization failed");
        std::process::exit(1);
    }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn rust_selection_capture_destroy_c(object: *mut c_void) {
    let outcome = std::panic::catch_unwind(|| {
        assert!(!object.is_null());
        assert!(
            SELECTION_CAPTURE_ID
                .compare_exchange(
                    object,
                    std::ptr::null_mut(),
                    Ordering::AcqRel,
                    Ordering::Acquire
                )
                .is_ok()
        );
        SELECTION_CAPTURE_DESTROY_ENTERED.fetch_add(1, Ordering::SeqCst);
        let payload = unsafe {
            object
                .cast::<u8>()
                .add(16)
                .cast::<*mut SelectionCapturePayload>()
                .read()
        };
        assert!(!payload.is_null());
        if unsafe { pthread_main_np() } == 1 {
            unsafe { finish_selection_capture_payload(payload.cast()) };
        } else {
            // Move ownership of the Rust payload to MAIN; never touch the
            // RefCell/native Selection on the foreign ARC release thread.
            SELECTION_CAPTURE_DEFERRED_DROPS.fetch_add(1, Ordering::SeqCst);
            let queue = (&raw const SELECTION_CAPTURE_MAIN_QUEUE).cast_mut().cast();
            unsafe { dispatch_async_f(queue, payload.cast(), finish_selection_capture_payload) };
        }
        unsafe { swift_deallocObject(object, 24, 7) };
        SELECTION_CAPTURE_DEALLOC_COMPLETED.fetch_add(1, Ordering::SeqCst);
    });
    if outcome.is_err() {
        eprintln!("Selection capture destroy validation failed");
        std::process::exit(1);
    }
}

fn create_selection_capture(installed: Box<Installed<'static>>) -> *mut c_void {
    assert!(unsafe { pthread_main_np() } == 1);
    assert!(SELECTION_CAPTURE_ID.load(Ordering::Acquire).is_null());
    assert!(SELECTION_CAPTURE_ROOT.load(Ordering::Acquire).is_null());
    assert!(std::mem::offset_of!(SelectionCaptureFullMetadata, destroy) == 8);
    assert!(std::mem::offset_of!(SelectionCaptureFullMetadata, local) == 24);
    let context = unsafe { swift_allocObject(&SELECTION_CAPTURE_METADATA.local, 24, 7) };
    assert!(!context.is_null(), "Selection capture allocation failed");
    SELECTION_CAPTURE_ALLOCATED.fetch_add(1, Ordering::SeqCst);
    let payload = Box::into_raw(Box::new(SelectionCapturePayload {
        installed: Some(installed),
    }));
    unsafe {
        context
            .cast::<u8>()
            .add(16)
            .cast::<*mut SelectionCapturePayload>()
            .write(payload)
    };
    assert!(
        SELECTION_CAPTURE_ID
            .compare_exchange(
                std::ptr::null_mut(),
                context,
                Ordering::AcqRel,
                Ordering::Acquire
            )
            .is_ok()
    );
    assert!(
        SELECTION_CAPTURE_ROOT
            .compare_exchange(
                std::ptr::null_mut(),
                context,
                Ordering::AcqRel,
                Ordering::Acquire
            )
            .is_ok()
    );
    context
}

fn retain_selection_capture_inputs(context: *mut c_void) {
    assert!(context == SELECTION_CAPTURE_ID.load(Ordering::Acquire) && !context.is_null());
    for _ in 0..2 {
        assert!(unsafe { swift_retain(context) } == context);
        SELECTION_CAPTURE_INPUT_RETAINS.fetch_add(1, Ordering::SeqCst);
    }
}

fn begin_selection_capture_close() {
    assert!(unsafe { pthread_main_np() } == 1);
    assert!(!SELECTION_CAPTURE_CLOSING.swap(true, Ordering::AcqRel));
}
pub(super) fn release_selection_capture_root() {
    assert!(unsafe { pthread_main_np() } == 1);
    assert!(SELECTION_CAPTURE_CLOSING.load(Ordering::Acquire));
    let context = SELECTION_CAPTURE_ROOT.swap(std::ptr::null_mut(), Ordering::AcqRel);
    assert!(!context.is_null() && context == SELECTION_CAPTURE_ID.load(Ordering::Acquire));
    unsafe { swift_release(context) };
    SELECTION_CAPTURE_ROOT_RELEASES.fetch_add(1, Ordering::SeqCst);
    // No dereference or refcount query of context after releasing this +1.
}

fn preserve_selection_capture_binding(binding: &Value<'static>) {
    assert!(unsafe { pthread_main_np() } == 1);
    assert!(binding.initialized && binding.binding);
    let mut copied = Box::new(Value::allocate(binding.witnesses, binding.proof, true));
    copied.copy(binding);
    assert!(
        SELECTION_CAPTURE_BINDING_COPY
            .compare_exchange(
                std::ptr::null_mut(),
                Box::into_raw(copied),
                Ordering::AcqRel,
                Ordering::Acquire
            )
            .is_ok()
    );
}

fn validate_selection_capture_callback(context: *mut c_void) {
    assert!(!context.is_null());
    assert!(context == SELECTION_CAPTURE_ID.load(Ordering::Acquire));
    assert!(!STATE.load(Ordering::Acquire).is_null());
}

pub(super) fn selection_capture_partial_evidence() -> String {
    format!(
        "captured_context_allocations={}\ncaptured_selection_state_drop_completed={}\ncaptured_context_destroy_entered={}\ncaptured_context_deallocation_completed={}\ncaptured_context_deferred_main_drops={}\ncaptured_context_input_retains={}\ncaptured_context_root_releases={}\ncaptured_validated_context_callbacks={}\ncaptured_context_identity_cleared={}\nstate_pointer_cleared={}\n",
        SELECTION_CAPTURE_ALLOCATED.load(Ordering::Acquire),
        SELECTION_CAPTURE_STATE_DROPS.load(Ordering::Acquire),
        SELECTION_CAPTURE_DESTROY_ENTERED.load(Ordering::Acquire),
        SELECTION_CAPTURE_DEALLOC_COMPLETED.load(Ordering::Acquire),
        SELECTION_CAPTURE_DEFERRED_DROPS.load(Ordering::Acquire),
        SELECTION_CAPTURE_INPUT_RETAINS.load(Ordering::Acquire),
        SELECTION_CAPTURE_ROOT_RELEASES.load(Ordering::Acquire),
        SELECTION_CAPTURE_VALIDATED_CALLBACKS.load(Ordering::Acquire),
        SELECTION_CAPTURE_ID.load(Ordering::Acquire).is_null(),
        STATE.load(Ordering::Acquire).is_null()
    )
}

// This must be called synchronously on MAIN before UIKit detach. The callback
// wiring must serve getters readonly after close and reject setters without
// assigning or calling selection_updated. It must still initialize every real
// Swift getter destination. No public call is made after root/context release.
pub(super) fn selection_capture_close_probe() -> String {
    assert!(unsafe { pthread_main_np() } == 1);
    begin_selection_capture_close();
    let source = STATE.load(Ordering::Acquire);
    assert!(!source.is_null());
    let state = unsafe { &*source.cast::<State<'static>>() };
    assert!(state.proof.n(Count::Active) == 0);
    let selection_witnesses = state.value.borrow().witnesses;
    let binding = SELECTION_CAPTURE_BINDING_COPY.swap(std::ptr::null_mut(), Ordering::AcqRel);
    assert!(!binding.is_null());
    let binding = unsafe { Box::from_raw(binding) };
    let counter = CATEGORY_TOKEN_PLAN.load(Ordering::Acquire);
    assert!(!counter.is_null());
    let counter = unsafe { &*counter };
    let get_entry = resolve(env!("BINDING_GET_SYMBOL"));
    let set_entry = resolve(env!("BINDING_SET_SYMBOL"));
    let before_gets = SELECTION_CAPTURE_CLOSED_GETS.load(Ordering::Acquire);
    let before_rejected = SELECTION_CAPTURE_REJECTED_SETS.load(Ordering::Acquire);
    let before_binding_destroy = state.proof.n(Count::BindingDestroy);

    let mut output = Value::allocate(selection_witnesses, state.proof, false);
    binding.get(&mut output, selection_witnesses.metadata, get_entry);
    let before_count = category_measured_read(counter, state.proof, Some(&output));
    assert!(before_count > 0);
    drop(output);

    // A genuinely different input: use the reviewed public empty Selection
    // initializer, never manufacture or mutate opaque token/Selection fields.
    let mut empty = Value::allocate(selection_witnesses, state.proof, false);
    empty.category(false, resolve(env!("SELECTION_CATEGORY_INIT_SYMBOL")));
    let empty_count = category_measured_read(counter, state.proof, Some(&empty));
    assert!(empty_count == 0);
    let mut transferred = Value::allocate(selection_witnesses, state.proof, false);
    transferred.copy(&empty);
    drop(empty);
    binding.set(&mut transferred, selection_witnesses.metadata, set_entry);
    // The public outer setter destroys its consumed input. Rust only frees
    // the transferred buffer, following the existing Value::set contract.
    drop(transferred);

    let mut output = Value::allocate(selection_witnesses, state.proof, false);
    binding.get(&mut output, selection_witnesses.metadata, get_entry);
    let after_count = category_measured_read(counter, state.proof, Some(&output));
    drop(output);
    let closed_gets = SELECTION_CAPTURE_CLOSED_GETS
        .load(Ordering::Acquire)
        .checked_sub(before_gets)
        .unwrap();
    let rejected_sets = SELECTION_CAPTURE_REJECTED_SETS
        .load(Ordering::Acquire)
        .checked_sub(before_rejected)
        .unwrap();
    assert!(after_count == before_count && after_count > 0);
    assert!(closed_gets >= 2 && rejected_sets == 1);
    assert!(state.proof.n(Count::Active) == 0);
    drop(binding);
    let binding_destroy = state
        .proof
        .n(Count::BindingDestroy)
        .checked_sub(before_binding_destroy)
        .unwrap();
    assert!(binding_destroy == 1);
    format!(
        "close_probe_initial_category_count={before_count}\nclose_probe_empty_input_count={empty_count}\nclose_probe_final_category_count={after_count}\nclose_probe_readonly_get_callbacks={closed_gets}\nclose_probe_rejected_set_callbacks={rejected_sets}\nclose_probe_binding_copy_destroy_calls={binding_destroy}\n"
    )
}
