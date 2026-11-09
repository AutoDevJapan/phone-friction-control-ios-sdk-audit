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
}
static SELECTION_CAPTURE_RESOURCES: AtomicPtr<SelectionCaptureResources> =
    AtomicPtr::new(std::ptr::null_mut());

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
    let mut stable = Value::allocate(&resources.selection, proof, false);
    stable.category(false, category);
    let installed = Box::new(Installed::install(stable, proof));
    let context = create_selection_capture(installed);
    collect_selection_abi(page);
    initialize_category_token_counts(page);
    let mut binding = Value::allocate(&resources.binding, proof, true);
    let before_gets = proof.n(Count::ClosureGet);
    retain_selection_capture_inputs(context);
    proof.inc(Count::BindingInit);
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
    assert!(proof.n(Count::ClosureGet) > before_gets);
    preserve_selection_capture_binding(&binding);
    let controller = resources.plan.host(&binding, proof, page);
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
    if !SELECTION_CAPTURE_CLOSING.load(Ordering::Acquire)
        || !SELECTION_CAPTURE_ROOT.load(Ordering::Acquire).is_null()
        || !SELECTION_CAPTURE_ID.load(Ordering::Acquire).is_null()
        || !SELECTION_CAPTURE_BINDING_COPY
            .load(Ordering::Acquire)
            .is_null()
        || SELECTION_CAPTURE_STATE_DROPS.load(Ordering::Acquire) != 1
        || SELECTION_CAPTURE_DESTROY_ENTERED.load(Ordering::Acquire) != 1
        || SELECTION_CAPTURE_DEALLOC_COMPLETED.load(Ordering::Acquire) != 1
        || SELECTION_CAPTURE_ALLOCATED.load(Ordering::Acquire) != 1
        || SELECTION_CAPTURE_INPUT_RETAINS.load(Ordering::Acquire) != 2
        || SELECTION_CAPTURE_ROOT_RELEASES.load(Ordering::Acquire) != 1
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
