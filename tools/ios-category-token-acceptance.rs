// SDK18.5 inspection: measure owned Set and Selection snapshot operations.
// Intended in mutable_binding, cfg(all(consent_selection_acceptance,
// category_token_count_acceptance, target_os="ios", target_env="sim",
// target_arch="aarch64")). No existing host/context owner is released.

static CATEGORY_TOKEN_PLAN: AtomicPtr<CategoryTokenCounter> = AtomicPtr::new(std::ptr::null_mut());
static CATEGORY_TOKEN_INITIAL: AtomicUsize = AtomicUsize::new(0);
static CATEGORY_TOKEN_CURRENT: AtomicUsize = AtomicUsize::new(0);
static CATEGORY_SET_COPIES: AtomicUsize = AtomicUsize::new(0);
static CATEGORY_SET_DESTROYS: AtomicUsize = AtomicUsize::new(0);
static CATEGORY_SET_ALLOCS: AtomicUsize = AtomicUsize::new(0);
static CATEGORY_SET_FREES: AtomicUsize = AtomicUsize::new(0);
static CATEGORY_SNAPSHOT_DONE: AtomicBool = AtomicBool::new(false);
static CATEGORY_SNAPSHOT_COUNT: AtomicUsize = AtomicUsize::new(0);
static CATEGORY_SNAPSHOT_COPIES: AtomicUsize = AtomicUsize::new(0);
static CATEGORY_SNAPSHOT_DESTROYS: AtomicUsize = AtomicUsize::new(0);
static CATEGORY_SNAPSHOT_ALLOCS: AtomicUsize = AtomicUsize::new(0);
static CATEGORY_SNAPSHOT_FREES: AtomicUsize = AtomicUsize::new(0);

// These are measurements of the existing explicit Proof counters, not counts
// of all allocations/owners or framework-internal retain/release operations.
fn category_proof_counts(proof: &Proof) -> [usize; 4] {
    [proof.n(Count::InitCopy), proof.n(Count::Destroy),
     proof.n(Count::Alloc), proof.n(Count::Dealloc)]
}
fn category_count_delta(after: [usize; 4], before: [usize; 4]) -> [usize; 4] {
    std::array::from_fn(|i| after[i].checked_sub(before[i]).expect("category proof counter regressed"))
}

// Same public getter and owned Set return contract as the fixed factory's
// read(), but borrow a genuine, initialized Selection snapshot that stays alive
// for this call. Never project a token/Selection field or fabricate a Set.
fn category_read_snapshot(counter: &CategoryTokenCounter, value: &Value<'_>) -> usize {
    assert!(unsafe { pthread_main_np() } == 1);
    assert!(!COUNT_READING.swap(true, Ordering::AcqRel));
    let _reading = CountReading;
    value.proof.check();
    assert!(value.proof.n(Count::Active) == 0);
    assert!(value.initialized && !value.taken && !value.binding && value.witnesses.size == 56);
    let state = STATE.load(Ordering::Acquire);
    assert!(!state.is_null());
    let state = unsafe { &*state.cast::<State<'static>>() };
    let expected_metadata = state.value.borrow().witnesses.metadata;
    assert!(value.witnesses.metadata == expected_metadata);
    assert!(std::ptr::eq(value.proof, state.proof));
    let mut returned = Value::allocate(&counter.set, value.proof, false);
    counter.explicit_gets.fetch_add(1, Ordering::SeqCst);
    let scalar = unsafe { rust_selection_applications(value.ptr(), counter.getter) };
    unsafe { returned.ptr().cast::<usize>().write(scalar) };
    returned.initialized = true;
    let mut copied = Value::allocate(&counter.set, value.proof, false);
    copied.copy(&returned);
    drop(returned);
    let receiver = unsafe { read(copied.ptr().cast::<usize>(), counter.page) };
    assert!(receiver != 0 && receiver & 0xc000_0000_0000_0001 == 0);
    counter.explicit_counts.fetch_add(1, Ordering::SeqCst);
    let count = unsafe { (counter.count)(receiver, counter.token, counter.hashable) };
    drop(copied);
    assert!(count >= 0);
    count as usize
}

fn category_measured_read(counter: &CategoryTokenCounter, proof: &Proof, snapshot: Option<&Value<'_>>) -> usize {
    let before = category_proof_counts(proof);
    let count = match snapshot {
        Some(value) => category_read_snapshot(counter, value),
        None => counter.read(),
    };
    let delta = category_count_delta(category_proof_counts(proof), before);
    // Expected explicit operations are assertions. Output below uses delta,
    // keeping expected constants separate from observed counter values.
    assert!(delta == [1, 2, 2, 2]);
    CATEGORY_SET_COPIES.fetch_add(delta[0], Ordering::SeqCst);
    CATEGORY_SET_DESTROYS.fetch_add(delta[1], Ordering::SeqCst);
    CATEGORY_SET_ALLOCS.fetch_add(delta[2], Ordering::SeqCst);
    CATEGORY_SET_FREES.fetch_add(delta[3], Ordering::SeqCst);
    count
}

fn initialize_category_token_counts(page: usize) {
    assert!(unsafe { pthread_main_np() } == 1);
    assert!(COUNT_BASELINE_READY.load(Ordering::Acquire));
    assert!(CATEGORY_TOKEN_PLAN.load(Ordering::Acquire).is_null());
    let counter = category_token_counter_after_independent_review(page);
    let proof = DIAGNOSTICS.lock().unwrap().clone().expect("pinned category diagnostics missing");
    let initial = category_measured_read(&counter, &proof, None);
    assert!(initial == 0, "fresh category tokens are not empty");
    CATEGORY_TOKEN_INITIAL.store(initial, Ordering::Release);
    CATEGORY_TOKEN_CURRENT.store(initial, Ordering::Release);
    let counter = Box::leak(counter);
    assert!(CATEGORY_TOKEN_PLAN.compare_exchange(std::ptr::null_mut(), counter,
        Ordering::AcqRel, Ordering::Acquire).is_ok());
}

fn check_category_selection_snapshots(counter: &CategoryTokenCounter, expected: usize) {
    assert!(unsafe { pthread_main_np() } == 1);
    assert!(expected > 0 && !CATEGORY_SNAPSHOT_DONE.load(Ordering::Acquire));
    let state = STATE.load(Ordering::Acquire);
    assert!(!state.is_null());
    let state = unsafe { &*state.cast::<State<'static>>() };
    state.proof.check();
    assert!(state.proof.n(Count::Active) == 0);
    let before = category_proof_counts(state.proof);
    let first = {
        let value = state.value.borrow();
        assert!(value.initialized && !value.taken && !value.binding && value.witnesses.size == 56);
        let mut first = Value::allocate(value.witnesses, state.proof, false);
        first.copy(&value);
        first
    };
    let mut surviving = Value::allocate(first.witnesses, state.proof, false);
    surviving.copy(&first);
    drop(first);
    // Destroying the first owned snapshot precedes the public getter read on
    // its surviving copy. The live UI's Selection/context still remain pinned.
    let nested_before = category_proof_counts(state.proof);
    let from_copy = category_measured_read(counter, state.proof, Some(&surviving));
    let nested_delta = category_count_delta(category_proof_counts(state.proof), nested_before);
    drop(surviving);
    let total = category_count_delta(category_proof_counts(state.proof), before);
    let snapshots = category_count_delta(total, nested_delta);
    assert!(from_copy == expected && from_copy > 0);
    assert!(snapshots == [2, 2, 2, 2]);
    CATEGORY_SNAPSHOT_COUNT.store(from_copy, Ordering::Release);
    CATEGORY_SNAPSHOT_COPIES.store(snapshots[0], Ordering::Release);
    CATEGORY_SNAPSHOT_DESTROYS.store(snapshots[1], Ordering::Release);
    CATEGORY_SNAPSHOT_ALLOCS.store(snapshots[2], Ordering::Release);
    CATEGORY_SNAPSHOT_FREES.store(snapshots[3], Ordering::Release);
    CATEGORY_SNAPSHOT_DONE.store(true, Ordering::Release);
}

// Call only in the existing selection_evidence fresh-setter-delta branch,
// after the existing applications count read has finished on MAIN.
fn update_category_token_counts() {
    assert!(unsafe { pthread_main_np() } == 1);
    let counter = CATEGORY_TOKEN_PLAN.load(Ordering::Acquire);
    assert!(!counter.is_null());
    let counter = unsafe { &*counter };
    let proof = DIAGNOSTICS.lock().unwrap().clone().expect("pinned category diagnostics missing");
    let selected = category_measured_read(counter, &proof, None);
    CATEGORY_TOKEN_CURRENT.store(selected, Ordering::Release);
    if selected > 0 && !CATEGORY_SNAPSHOT_DONE.load(Ordering::Acquire) {
        check_category_selection_snapshots(counter, selected);
    }
}

pub(super) fn category_token_snapshot() -> (usize, usize) {
    assert!(unsafe { pthread_main_np() } == 1);
    assert!(!CATEGORY_TOKEN_PLAN.load(Ordering::Acquire).is_null());
    (CATEGORY_TOKEN_INITIAL.load(Ordering::Acquire), CATEGORY_TOKEN_CURRENT.load(Ordering::Acquire))
}
fn category_token_evidence() -> String {
    assert!(unsafe { pthread_main_np() } == 1);
    let plan = CATEGORY_TOKEN_PLAN.load(Ordering::Acquire);
    assert!(!plan.is_null());
    let counter = unsafe { &*plan };
    let (initial, selected) = category_token_snapshot();
    format!("initial_category_tokens_count={initial}\nselected_category_tokens_count={selected}\ncategory_tokens_getter_calls={}\ncategory_tokens_count_calls={}\ncategory_set_initialize_with_copy_calls={}\ncategory_set_destroy_calls={}\ncategory_set_allocations={}\ncategory_set_deallocations={}\nselection_snapshot_lifetime_verified={}\nselection_snapshot_category_tokens_count={}\nselection_snapshot_initialize_with_copy_calls={}\nselection_snapshot_destroy_calls={}\nselection_snapshot_allocations={}\nselection_snapshot_deallocations={}\n",
        counter.explicit_gets.load(Ordering::Acquire),counter.explicit_counts.load(Ordering::Acquire),
        CATEGORY_SET_COPIES.load(Ordering::Acquire),CATEGORY_SET_DESTROYS.load(Ordering::Acquire),
        CATEGORY_SET_ALLOCS.load(Ordering::Acquire),CATEGORY_SET_FREES.load(Ordering::Acquire),
        CATEGORY_SNAPSHOT_DONE.load(Ordering::Acquire),CATEGORY_SNAPSHOT_COUNT.load(Ordering::Acquire),
        CATEGORY_SNAPSHOT_COPIES.load(Ordering::Acquire),CATEGORY_SNAPSHOT_DESTROYS.load(Ordering::Acquire),
        CATEGORY_SNAPSHOT_ALLOCS.load(Ordering::Acquire),CATEGORY_SNAPSHOT_FREES.load(Ordering::Acquire))
}
