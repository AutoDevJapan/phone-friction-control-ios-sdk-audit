// Held integration source, locally checked for Apple-target code generation.
// No Apple runtime acceptance or independent ownership review yet.
// Intended only for the opt-in captured Selection owner mode on SDK18.5 arm64
// Simulator. Legacy NULL-context callbacks keep their existing implementation.
// Base: audit57b69ab mutable_binding::callback. This adds no new foreign call.
// Requires ios-selection-captured-owner.rs and the opt-in host/teardown mode.

fn selection_capture_callback(pointer: *mut c_void, context: *mut c_void, getter: bool) {
    // A diagnostics Arc is independent of State and safe to clone before the
    // main-thread/context validation. Never dereference an unvalidated context.
    let diagnostics = DIAGNOSTICS.lock().unwrap().clone();
    if let Some(proof) = &diagnostics {
        proof.inc(if getter {
            Count::ClosureGet
        } else {
            Count::ClosureSet
        });
    }
    let outcome = std::panic::catch_unwind(|| {
        assert!(
            unsafe { pthread_main_np() } == 1,
            "foreign mutable callback thread"
        );
        assert!(
            !COUNT_READING.load(Ordering::Acquire),
            "Binding callback during semantic Set read"
        );
        // Identity and the live state registration are checked before loading
        // State. The Swift closure owns its context while it calls this helper.
        validate_selection_capture_callback(context);
        SELECTION_CAPTURE_VALIDATED_CALLBACKS.fetch_add(1, Ordering::SeqCst);
        let source = STATE.load(Ordering::Acquire);
        assert!(!source.is_null());
        let state = unsafe { &*source.cast::<State<'_>>() };
        state.proof.check();
        assert!(
            state.proof.n(Count::Active) == 0,
            "mutable callback reentry"
        );
        state.proof.inc(Count::Active);
        let active = Callback(state.proof);
        let value = state.value.borrow();
        assert!(
            value.initialized
                && !pointer.is_null()
                && pointer.addr() % value.witnesses.layout.align() == 0,
            "invalid mutable callback value"
        );
        assert!(
            pointer.addr().abs_diff(value.ptr().addr()) >= value.witnesses.layout.size(),
            "mutable callback overlaps stable state"
        );
        let closing = SELECTION_CAPTURE_CLOSING.load(Ordering::Acquire);
        if closing && !getter {
            // Borrowed setter input is still owned by the public outer setter,
            // which destroys it. Do not assign, destroy that input, increment
            // witness-return counters or schedule any UIKit work here.
            SELECTION_CAPTURE_REJECTED_SETS.fetch_add(1, Ordering::SeqCst);
            return;
        }
        let result = if getter {
            state.proof.inc(Count::InitCopy);
            unsafe { (value.witnesses.initialize)(pointer, value.ptr(), value.witnesses.metadata) }
        } else {
            state.proof.inc(Count::AssignCopy);
            unsafe { (value.witnesses.assign)(value.ptr(), pointer, value.witnesses.metadata) }
        };
        assert!(
            result == if getter { pointer } else { value.ptr() },
            "mutable closure witness destination changed"
        );
        state.proof.inc(Count::Returned);
        if getter && closing {
            // Every legitimate Swift getter still initializes its destination,
            // including close-time reads through a surviving real Binding copy.
            SELECTION_CAPTURE_CLOSED_GETS.fetch_add(1, Ordering::SeqCst);
        }
        if !getter {
            drop(value);
            drop(active);
            // Initializer callbacks are allowed during the standalone phase,
            // before semantic-reader plans or a Hosting view exist. Mutation
            // still completes, but UI/semantic badge work belongs to UI phase.
            if !SELECTION_CAPTURE_PREFLIGHT_ACTIVE.load(Ordering::Acquire) {
                super::native_host::selection_updated();
            }
        }
    });
    if outcome.is_err() {
        if let Some(proof) = diagnostics {
            proof.verified.store(false, Ordering::SeqCst);
            proof.emit();
        }
        eprintln!("Captured Selection callback validation failed");
        std::process::exit(1);
    }
}
