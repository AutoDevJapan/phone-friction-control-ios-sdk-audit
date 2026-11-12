// Opt-in held integration candidate; not promoted to the live audit.
// Existing SDK18.5 category/Binding/Picker guards remain required. The common
// non-NULL-context constructor guard additionally validates the public runtime
// retain/release imports without changing or authenticating pointer bits.

struct SelectionCaptureResources {
    proof: Box<Arc<Proof>>,
    selection: Box<Witnesses>,
    binding: Box<Witnesses>,
    plan: Box<picker_values::Plan>,
    deadline: Option<Deadline>,
    replacement_entries: RootReplacementEntries,
}
static CAPTURE_ROOT_REPLACEMENTS: AtomicUsize = AtomicUsize::new(0);

pub(super) fn replace_captured_hosting_root(controller: *mut c_void) {
    assert!(unsafe { pthread_main_np() } == 1);
    assert!(SELECTION_CAPTURE_CLOSING.load(Ordering::Acquire));
    assert!(CAPTURE_ROOT_REPLACEMENTS.load(Ordering::Acquire) == 0);
    let pointer = SELECTION_CAPTURE_RESOURCES.load(Ordering::Acquire);
    assert!(!pointer.is_null());
    let resources = unsafe { &*pointer };
    let proof = &resources.proof;
    let mut empty = Value::allocate(&resources.selection, proof, false);
    empty.category(false, resolve(env!("SELECTION_CATEGORY_INIT_SYMBOL")));
    let mut constant = Value::allocate(&resources.binding, proof, true);
    unsafe {
        audit_binding_constant(
            constant.ptr(),
            empty.ptr(),
            resources.selection.metadata,
            resources.replacement_entries.constant,
        )
    };
    constant.initialized = true;
    resources.plan.replace_root(
        &constant,
        proof,
        controller,
        resources.replacement_entries.setter,
    );
    drop(constant);
    drop(empty);
    assert!(CAPTURE_ROOT_REPLACEMENTS.fetch_add(1, Ordering::SeqCst) == 0);
}
static SELECTION_CAPTURE_RESOURCES: AtomicPtr<SelectionCaptureResources> =
    AtomicPtr::new(std::ptr::null_mut());
static SELECTION_CAPTURE_CONSTRUCTION_STAGE: AtomicUsize = AtomicUsize::new(0);
static SELECTION_CAPTURE_CTOR_GETS_BEFORE: AtomicUsize = AtomicUsize::new(0);
static SELECTION_CAPTURE_CTOR_GETS_AFTER: AtomicUsize = AtomicUsize::new(0);

pub(super) fn selection_capture_construction_evidence() -> String {
    let stage = match SELECTION_CAPTURE_CONSTRUCTION_STAGE.load(Ordering::Acquire) {
        1 => "preflights",
        2 => "context_installation",
        3 => "semantic_reader_plans",
        4 => "binding_constructor",
        5 => "binding_constructor_returned",
        6 => "binding_copy",
        7 => "hosting_constructor",
        8 => "hosting_returned",
        _ => "not_started",
    };
    format!(
        "captured_construction_stage={stage}\ncaptured_ctor_getter_calls_before={}\ncaptured_ctor_getter_calls_after={}\n{}",
        SELECTION_CAPTURE_CTOR_GETS_BEFORE.load(Ordering::Acquire),
        SELECTION_CAPTURE_CTOR_GETS_AFTER.load(Ordering::Acquire),
        selection_capture_partial_evidence()
    )
}

pub(super) fn root_replacement_evidence() -> String {
    format!(
        "captured_hosting_root_replacements={}\n",
        CAPTURE_ROOT_REPLACEMENTS.load(Ordering::Acquire)
    )
}

// Separate fresh Binding, Picker-value and unpresented-Hosting lifetimes.
// The Hosting phase never attaches its controller or explicitly requests its
// view. A failed phase is preserved before any later presentation can start.
fn selection_lifetime_preflights(
    resources: &'static SelectionCaptureResources,
    category: *const c_void,
    init: *const c_void,
    page: usize,
) {
    let proof: &'static Arc<Proof> = &resources.proof;
    assert!(!SELECTION_CAPTURE_PREFLIGHT_ACTIVE.swap(true, Ordering::AcqRel));
    let operation_counts = || {
        [
            proof.n(Count::BindingInit),
            proof.n(Count::BindingCopy),
            proof.n(Count::BindingDestroy),
            proof.n(Count::PublicGet),
            proof.n(Count::Alloc),
            proof.n(Count::Dealloc),
        ]
    };
    let picker_counts = || {
        [
            proof.picker.n(picker_values::Count::Constructor),
            proof.picker.n(picker_values::Count::Copy),
            proof.picker.n(picker_values::Count::Destroy),
            proof.picker.n(picker_values::Count::Alloc),
            proof.picker.n(picker_values::Count::Free),
        ]
    };
    let home = std::env::var_os("HOME").expect("preflight HOME missing");
    let path = std::path::PathBuf::from(home).join("Documents/captured-selection-preflight.txt");
    let mut evidence = String::new();
    for phase_index in 0..3 {
        let prefix = match phase_index {
            0 => "preflight",
            1 => "picker_preflight",
            _ => "hosting_preflight",
        };
        std::fs::write(&path, format!("{evidence}{prefix}_phase=started\n"))
            .expect("write native lifetime preflight start");
        let before = SelectionCaptureCounts::current();
        let operations_before = operation_counts();
        let picker_before = picker_counts();
        let host_before = [
            proof.hosting_calls.load(Ordering::Acquire),
            proof.root_transfers.load(Ordering::Acquire),
        ];
        let mut stable = Value::allocate(&resources.selection, proof, false);
        stable.category(false, category);
        let context = create_selection_capture(Box::new(Installed::install(stable, proof)));
        let mut original = Value::allocate(&resources.binding, proof, true);
        retain_selection_capture_inputs(context);
        proof.inc(Count::BindingInit);
        unsafe {
            rust_selection_capture_binding_init(
                original.ptr(),
                rust_mutable_get_closure as *const c_void,
                rust_mutable_set_closure as *const c_void,
                resources.selection.metadata,
                init,
                context,
            )
        };
        original.initialized = true;
        let mut copy = Value::allocate(&resources.binding, proof, true);
        copy.copy(&original);
        drop(original);
        let mut output = Value::allocate(&resources.selection, proof, false);
        copy.get(
            &mut output,
            resources.selection.metadata,
            resolve(env!("BINDING_GET_SYMBOL")),
        );
        drop(output);
        let mut hosting_weak_nil = true;
        let mut hosting_releases = 0;
        if phase_index == 1 {
            // Reuse the existing guarded ctor and full Picker VWT operations.
            // execute returns after destroying both genuine Picker owners; it
            // does not construct Hosting, inspect tokens or create a UI graph.
            resources.plan.execute(&copy, proof);
        }
        if phase_index == 2 {
            (hosting_weak_nil, hosting_releases) =
                super::native_host::inspect_unpresented_hosting(|| {
                    resources.plan.host(&copy, proof, page)
                });
        }
        begin_selection_capture_close();
        drop(copy);
        release_selection_capture_root();
        // No dereference of the released root. Keep phase-specific monotonic
        // deltas; prior successful destruction cannot satisfy this new phase.
        let measured = SelectionCaptureCounts::current().since(before);
        let operations = std::array::from_fn::<_, 6, _>(|index| {
            operation_counts()[index]
                .checked_sub(operations_before[index])
                .unwrap()
        });
        let picker = std::array::from_fn::<_, 5, _>(|index| {
            picker_counts()[index]
                .checked_sub(picker_before[index])
                .unwrap()
        });
        let hosting = [
            proof
                .hosting_calls
                .load(Ordering::Acquire)
                .checked_sub(host_before[0])
                .unwrap(),
            proof
                .root_transfers
                .load(Ordering::Acquire)
                .checked_sub(host_before[1])
                .unwrap(),
        ];
        let identity_clear = SELECTION_CAPTURE_ID.load(Ordering::Acquire).is_null();
        let state_clear = STATE.load(Ordering::Acquire).is_null();
        let expected_copies = if phase_index == 0 { 1 } else { 2 };
        let expected_picker = match phase_index {
            0 => [0; 5],
            1 => [1, 1, 2, 2, 2],
            // The public Hosting allocator consumes/destroys the copied root;
            // only the original Picker destroy is an explicit Rust VWT call.
            _ => [1, 1, 1, 2, 2],
        };
        let expected_hosting = if phase_index == 2 { [1, 1] } else { [0; 2] };
        let drained = measured.allocations == 1
            && measured.state_drops == 1
            && measured.destroys == 1
            && measured.deallocations == 1
            && measured.input_retains == 2
            && measured.root_releases == 1
            && operations[0..4] == [1, expected_copies, 2, 1]
            && operations[4] > 0
            && operations[4] == operations[5]
            && picker == expected_picker
            && hosting == expected_hosting
            && hosting_releases == usize::from(phase_index == 2)
            && hosting_weak_nil
            && identity_clear
            && state_clear
            && proof.n(Count::Active) == 0
            && !COUNT_READING.load(Ordering::Acquire)
            && DIAGNOSTICS.lock().unwrap().is_none();
        let phase = if drained { "complete" } else { "not_drained" };
        evidence.push_str(&format!(
            "{prefix}_phase={phase}\n{prefix}_binding_initializer_calls={}\n{prefix}_binding_copy_calls={}\n{prefix}_binding_destroy_calls={}\n{prefix}_public_getter_calls={}\n{prefix}_context_allocations={}\n{prefix}_state_drop_completed={}\n{prefix}_context_destroy_entered={}\n{prefix}_context_deallocation_completed={}\n{prefix}_input_retains={}\n{prefix}_root_releases={}\n{prefix}_mutable_allocations={}\n{prefix}_mutable_deallocations={}\n{prefix}_context_identity_cleared={identity_clear}\n{prefix}_state_pointer_cleared={state_clear}\n",
            operations[0], operations[1], operations[2], operations[3],
            measured.allocations, measured.state_drops, measured.destroys,
            measured.deallocations, measured.input_retains, measured.root_releases,
            operations[4], operations[5]
        ));
        if phase_index > 0 {
            evidence.push_str(&format!(
                "{prefix}_picker_constructor_calls={}\n{prefix}_picker_copy_calls={}\n{prefix}_picker_destroy_calls={}\n{prefix}_picker_allocations={}\n{prefix}_picker_deallocations={}\n",
                picker[0], picker[1], picker[2], picker[3], picker[4]
            ));
        }
        if phase_index == 2 {
            evidence.push_str(&format!(
                "hosting_preflight_hosting_constructor_calls={}\nhosting_preflight_root_transfer_calls={}\nhosting_preflight_owned_controller_releases={hosting_releases}\nhosting_preflight_weak_controller_nil={hosting_weak_nil}\n",
                hosting[0], hosting[1]
            ));
        }
        std::fs::write(&path, &evidence).expect("write native lifetime preflight result");
        assert!(drained, "nontrivial {prefix} lifetime did not drain");
        // Reopen only after this entire context/State lifetime completed. The
        // next phase allocates a distinct owner and never reuses a live context.
        assert!(SELECTION_CAPTURE_CLOSING.swap(false, Ordering::AcqRel));
    }
    assert!(SELECTION_CAPTURE_PREFLIGHT_EVIDENCE.set(evidence).is_ok());
    // Preserve monotonic global counters; UI output uses the measured delta
    // from all completed preflights rather than resetting/relabeling totals.
    assert!(
        SELECTION_CAPTURE_UI_BASELINE
            .set(SelectionCaptureCounts::current())
            .is_ok()
    );
    assert!(SELECTION_CAPTURE_PREFLIGHT_ACTIVE.swap(false, Ordering::AcqRel));
}

fn selection_capture_host() -> *mut c_void {
    assert!(unsafe { pthread_main_np() } == 1);
    assert!(cfg!(all(
        target_os = "ios",
        target_env = "sim",
        target_arch = "aarch64",
        consent_selection_acceptance,
        category_token_count_acceptance
    )));
    assert!(
        SELECTION_CAPTURE_RESOURCES
            .load(Ordering::Acquire)
            .is_null()
    );
    let proof = Box::new(Arc::new(Proof::new()));
    let deadline = Some(Deadline::start(Arc::clone(&proof)));
    proof.check();
    let page = unsafe { getpagesize() };
    assert!(page > 0);
    let page = page as usize;
    let accessor: unsafe extern "Swift" fn(usize) -> MetadataResponse =
        unsafe { std::mem::transmute(resolve(env!("SELECTION_METADATA_SYMBOL"))) };
    let selection = unsafe { accessor(0) };
    assert!(selection.state == 0 && !selection.value.is_null());
    proof.picker.inc(picker_values::Count::SelectionMetadata);
    let selection = Box::new(Witnesses::load(selection.value, 56, page));
    let accessor: unsafe extern "Swift" fn(usize, *const c_void) -> MetadataResponse =
        unsafe { std::mem::transmute(resolve(env!("BINDING_METADATA_SYMBOL"))) };
    let binding = unsafe { accessor(0, selection.metadata) };
    assert!(binding.state == 0 && !binding.value.is_null());
    proof.picker.inc(picker_values::Count::BindingMetadata);
    let binding = Box::new(Witnesses::load(binding.value, 72, page));
    verify_selection_capture_witness_images(&selection);
    verify_selection_capture_witness_images(&binding);
    assert!(
        unsafe { read(binding.metadata.cast::<*const c_void>().add(2), page) }
            == selection.metadata
    );
    let category = resolve(env!("SELECTION_CATEGORY_INIT_SYMBOL"));
    let init = resolve(env!("BINDING_INIT_SYMBOL"));
    verify_entries(
        category,
        init,
        resolve(env!("BINDING_GET_SYMBOL")),
        resolve(env!("BINDING_SET_SYMBOL")),
        page,
    );
    verify_selection_capture_constructor(init, page);
    let plan = Box::new(picker_values::Plan::load(&proof, page));
    let resources = Box::into_raw(Box::new(SelectionCaptureResources {
        proof,
        selection,
        binding,
        plan,
        deadline,
        replacement_entries: RootReplacementEntries::load(),
    }));
    assert!(
        SELECTION_CAPTURE_RESOURCES
            .compare_exchange(
                std::ptr::null_mut(),
                resources,
                Ordering::AcqRel,
                Ordering::Acquire
            )
            .is_ok()
    );
    // The boxed fields have stable addresses. These references are restricted
    // to this registered owner graph. Reclamation requires completed State and
    // context destruction, no Binding copy, no diagnostics/callbacks/readers,
    // and stopping/joining the watchdog before freeing any referenced field.
    let resources: &'static SelectionCaptureResources = unsafe { &*resources };
    let proof: &'static Arc<Proof> = &resources.proof;
    SELECTION_CAPTURE_CONSTRUCTION_STAGE.store(1, Ordering::Release);
    selection_lifetime_preflights(resources, category, init, page);
    SELECTION_CAPTURE_CONSTRUCTION_STAGE.store(2, Ordering::Release);
    let mut stable = Value::allocate(&resources.selection, proof, false);
    stable.category(false, category);
    let installed = Box::new(Installed::install(stable, proof));
    let context = create_selection_capture(installed);
    SELECTION_CAPTURE_CONSTRUCTION_STAGE.store(3, Ordering::Release);
    collect_selection_abi(page);
    initialize_category_token_counts(page);
    let mut binding = Value::allocate(&resources.binding, proof, true);
    let before_gets = proof.n(Count::ClosureGet);
    SELECTION_CAPTURE_CTOR_GETS_BEFORE.store(before_gets, Ordering::Release);
    retain_selection_capture_inputs(context);
    proof.inc(Count::BindingInit);
    SELECTION_CAPTURE_CONSTRUCTION_STAGE.store(4, Ordering::Release);
    unsafe {
        rust_selection_capture_binding_init(
            binding.ptr(),
            rust_mutable_get_closure as *const c_void,
            rust_mutable_set_closure as *const c_void,
            resources.selection.metadata,
            init,
            context,
        )
    };
    binding.initialized = true;
    let after_gets = proof.n(Count::ClosureGet);
    SELECTION_CAPTURE_CTOR_GETS_AFTER.store(after_gets, Ordering::Release);
    SELECTION_CAPTURE_CONSTRUCTION_STAGE.store(5, Ordering::Release);
    assert!(
        after_gets > before_gets,
        "Captured Binding constructor did not invoke getter"
    );
    rust_selection_capture_initialization_finished();
    SELECTION_CAPTURE_CONSTRUCTION_STAGE.store(6, Ordering::Release);
    preserve_selection_capture_binding(&binding);
    SELECTION_CAPTURE_CONSTRUCTION_STAGE.store(7, Ordering::Release);
    let controller = resources.plan.host(&binding, proof, page);
    SELECTION_CAPTURE_CONSTRUCTION_STAGE.store(8, Ordering::Release);
    drop(binding);
    assert!(proof.n(Count::Active) == 0);
    controller
}

// This performs no Swift/UI call. Call only after releasing the copied Binding,
// the root context reference, and the UI's owned references. If Swift/UI owners
// still keep the context alive, it reports pending rather than freeing State or
// treating process exit as proof. UIKit weak-object checks remain a separate
// required integration before any whole-graph acceptance claim.
pub(super) fn finish_selection_capture_resources() -> Result<String, &'static str> {
    if unsafe { pthread_main_np() } != 1 {
        return Err("Selection resource cleanup requires MAIN");
    }
    let measured = selection_capture_ui_counts();
    if !SELECTION_CAPTURE_CLOSING.load(Ordering::Acquire)
        || !SELECTION_CAPTURE_ROOT.load(Ordering::Acquire).is_null()
        || !SELECTION_CAPTURE_ID.load(Ordering::Acquire).is_null()
        || !SELECTION_CAPTURE_BINDING_COPY
            .load(Ordering::Acquire)
            .is_null()
        || measured.state_drops != 1
        || measured.destroys != 1
        || measured.deallocations != 1
        || measured.allocations != 1
        || measured.input_retains != 2
        || measured.root_releases != 1
        || !STATE.load(Ordering::Acquire).is_null()
        || COUNT_READING.load(Ordering::Acquire)
        || DIAGNOSTICS.lock().unwrap().is_some()
    {
        return Err("Selection capture owners/readers have not drained");
    }
    let pointer = SELECTION_CAPTURE_RESOURCES.load(Ordering::Acquire);
    if pointer.is_null() {
        return Err("Selection resources already consumed");
    }
    let resources = unsafe { &*pointer };
    let proof = &resources.proof;
    if proof.n(Count::Active) != 0
        || proof.n(Count::Alloc) != proof.n(Count::Dealloc)
        || proof.picker.n(picker_values::Count::Alloc) != proof.picker.n(picker_values::Count::Free)
    {
        return Err("Selection callback/buffer ownership has not drained");
    }
    let allocation_count = proof.n(Count::Alloc);
    let deallocation_count = proof.n(Count::Dealloc);
    let picker_allocations = proof.picker.n(picker_values::Count::Alloc);
    let picker_frees = proof.picker.n(picker_values::Count::Free);
    if SELECTION_CAPTURE_RESOURCES
        .compare_exchange(
            pointer,
            std::ptr::null_mut(),
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_err()
    {
        return Err("Selection resource ownership changed");
    }
    // MAIN owns reclamation. Both getter plans own only descriptor/witness
    // records after State destruction; their owned transient Sets were already
    // destroyed before CountReading finished. No callback may read them now.
    let mut resources = unsafe { Box::from_raw(pointer) };
    drop(resources.deadline.take()); // stop and join, not a detached timeout
    let application = COUNT_PLAN.swap(std::ptr::null_mut(), Ordering::AcqRel);
    let category = CATEGORY_TOKEN_PLAN.swap(std::ptr::null_mut(), Ordering::AcqRel);
    assert!(!application.is_null() && !category.is_null());
    unsafe {
        drop(Box::from_raw(application));
        drop(Box::from_raw(category));
    }
    COUNT_BASELINE_READY.store(false, Ordering::Release);
    let proof_owners = Arc::strong_count(&resources.proof);
    assert!(proof_owners == 1);
    let measured_context = selection_capture_partial_evidence();
    let evidence = format!(
        "{measured_context}mutable_rust_value_allocations={allocation_count}\nmutable_rust_value_deallocations={deallocation_count}\npicker_rust_value_allocations={picker_allocations}\npicker_rust_value_deallocations={picker_frees}\nproof_owners_before_final_drop={proof_owners}\nselection_state_watchdog_stopped_and_joined=true\nstate_cleanup_does_not_cover_framework_caches=true\n"
    );
    drop(resources);
    Ok(evidence)
}
