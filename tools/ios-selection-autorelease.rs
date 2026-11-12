// The Objective-C bridge calls synchronously once and retains neither pointer.
// Current callers return only through borrowed Rust locals (unit/bool/arrays),
// never an unretained Objective-C value whose pool ownership might expire.
unsafe extern "C" {
    fn audit_capture_autorelease_pool(
        body: unsafe extern "C" fn(*mut c_void),
        context: *mut c_void,
    );
}

fn capture_autorelease_scope<F: FnOnce()>(body: F) {
    unsafe extern "C" fn invoke<F: FnOnce()>(context: *mut c_void) {
        if context.is_null() {
            eprintln!("Captured autorelease callback has no context");
            std::process::exit(1);
        }
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // The caller's Option<F> is stable on its stack until the C bridge
            // returns. No other reference accesses it during this callback.
            let slot = unsafe { &mut *context.cast::<Option<F>>() };
            let body = slot.take().expect("autorelease callback repeated");
            body();
        }));
        if outcome.is_err() {
            // A Rust unwind must never cross the Objective-C/C boundary.
            eprintln!("Captured autorelease callback failed");
            std::process::exit(1);
        }
    }
    let mut slot = Some(body);
    unsafe { audit_capture_autorelease_pool(invoke::<F>, (&raw mut slot).cast::<c_void>()) };
    if slot.is_some() {
        eprintln!("Captured autorelease callback was not invoked");
        std::process::exit(1);
    }
}
