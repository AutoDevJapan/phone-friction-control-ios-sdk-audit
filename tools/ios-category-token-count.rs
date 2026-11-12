// SDK18.5 arm64 iOS Simulator category-token count inspection.
// Compile-time scoped by the opt-in consent/category-count inspection mode.
// Inputs: SDK18.5 declarations and bounded getter/metadata evidence.
// Existing Application Count ABI baseline must finish before this factory.
// Host/context remain pinned. This does NOT implement whole-view teardown.

struct CategoryTokenCounter {
    set: Witnesses,
    token: *const c_void,
    hashable: *const c_void,
    getter: *const c_void,
    count: unsafe extern "Swift" fn(usize, *const c_void, *const c_void) -> isize,
    page: usize,
    explicit_gets: AtomicUsize,
    explicit_counts: AtomicUsize,
}
fn category_image(p: *const c_void, suffix: &[u8]) -> *const c_void {
    assert!(!p.is_null());
    let mut info = DlInfo::default();
    assert!(unsafe { dladdr(p, &mut info) } != 0 && !info.base.is_null() && !info.filename.is_null());
    assert!(unsafe { std::ffi::CStr::from_ptr(info.filename) }.to_bytes().ends_with(suffix));
    info.base
}
fn category_adrp_address(p: *const c_void, instruction: u32) -> usize {
    let bits = (((instruction >> 5) & 0x7ffff) << 2) | ((instruction >> 29) & 3);
    let displacement = (((bits as u64) << 43) as i64 >> 43) as isize * 4096;
    (p.addr() & !4095).checked_add_signed(displacement).unwrap()
}
fn category_relative_pointer(p: *const c_void, address: usize) -> *const c_void {
    let delta = isize::try_from(address as i128 - p.addr() as i128).unwrap();
    let result = p.cast::<u8>().wrapping_offset(delta).cast::<c_void>();
    assert!(result.addr() == address);
    result
}
// Compare an SDK stub's untouched GOT pointer to a public runtime entry.
// Never call the computed address, strip/authenticate it, or overwrite the GOT.
// If a lazy/unrecognized stub remains, fail rather than force an unknown call.
fn category_runtime_stub(stub: *const c_void, image: &[u8], expected: &str, page: usize) {
    category_image(stub, image);
    let a = word(stub, 0, page);
    let b = word(stub, 4, page);
    assert!(a & 0x9f00001f == 0x90000010);
    assert!(b & 0xffc003ff == 0xf9400210);
    assert!(word(stub, 8, page) == 0xd61f0200);
    let slot = category_adrp_address(stub, a).checked_add(((b >> 10) & 4095) as usize * 8).unwrap();
    let pointer = category_relative_pointer(stub, slot);
    category_image(pointer, image);
    let target = unsafe { read(pointer.cast::<*const c_void>(), page) };
    let public = resolve(expected);
    category_image(public, b"/libswiftCore.dylib");
    assert!(target == public, "runtime import not identified; dependent public call rejected");
}
fn category_words(p: *const c_void, expected: &[(usize,u32,u32)], page: usize) {
    for &(offset,mask,value) in expected { assert!(word(p,offset,page) & mask == value); }
}
fn category_token_counter_after_independent_review(page: usize) -> Box<CategoryTokenCounter> {
    assert!(cfg!(all(target_os="ios",target_env="sim",target_arch="aarch64")));
    assert!(unsafe { pthread_main_np() } == 1);
    assert!(COUNT_BASELINE_READY.load(Ordering::Acquire));
    let category_ma = resolve("$s15ManagedSettings16ActivityCategoryVMa");
    let category_desc = resolve("$s15ManagedSettings16ActivityCategoryVMn");
    category_image(category_ma,b"/ManagedSettings");
    category_image(category_desc,b"/ManagedSettings");
    category_words(category_ma,&[(0,0x9f00001f,0x90000000),(4,0xffc003ff,0x91000000),
        (8,0xffffffff,0xd2800001),(12,0xffffffff,0xd65f03c0)],page);
    let category_fn: unsafe extern "Swift" fn(usize)->MetadataResponse = unsafe {std::mem::transmute(category_ma)};
    let category = unsafe {category_fn(0)};
    assert!(category.state == 0 && !category.value.is_null());
    assert!(unsafe {read(category.value.cast::<usize>(),page)} == 0x200);
    assert!(unsafe {read(category.value.cast::<*const c_void>().wrapping_add(1),page)} == category_desc);

    let token_ma = resolve("$s15ManagedSettings5TokenVMa");
    let token_desc = resolve("$s15ManagedSettings5TokenVMn");
    category_image(token_ma,b"/ManagedSettings");
    category_image(token_desc,b"/ManagedSettings");
    // Captured Token<T>: one type parameter, no requirements, one key argument.
    for (offset,value) in [(0,0x001000d1),(20,1),(24,3),(36,1),(40,1),(44,0x80)] {
        assert!(word(token_desc,offset,page) == value);
    }
    category_words(token_ma,&[(0,0x9f00001f,0x90000004),(4,0xffc003ff,0x91000084)],page);
    let descriptor_address = category_adrp_address(token_ma,word(token_ma,0,page))
        .checked_add(((word(token_ma,4,page)>>10)&4095) as usize).unwrap();
    assert!(descriptor_address == token_desc.addr());
    let helper = branch(token_ma,8,false,page);
    category_words(helper,&[(0,0xffffffff,0xd100c3ff),(4,0xffffffff,0xa9027bfd),
        (8,0xffffffff,0x910083fd),(12,0xffffffff,0xa9008be1),(16,0xffffffff,0xf9000fe3),
        (20,0xffffffff,0x910023e1),(24,0xffffffff,0xaa0403e2),
        (32,0xffffffff,0xa9427bfd),(36,0xffffffff,0x9100c3ff),(40,0xffffffff,0xd65f03c0)],page);
    category_runtime_stub(branch(helper,28,true,page),b"/ManagedSettings","swift_getGenericMetadata",page);
    // The helper spills x1/x2/x3, but the descriptor admits ONLY key x1.
    let token_fn: unsafe extern "Swift" fn(usize,*const c_void)->MetadataResponse =
        unsafe {std::mem::transmute(token_ma)};
    let token = unsafe {token_fn(0,category.value)};
    assert!(token.state == 0 && !token.value.is_null());
    assert!(unsafe {read(token.value.cast::<usize>(),page)} == 0x200);
    assert!(unsafe {read(token.value.cast::<*const c_void>().wrapping_add(1),page)} == token_desc);
    assert!(unsafe {read(token.value.cast::<*const c_void>().wrapping_add(2),page)} == category.value);
    unsafe extern "C" {
        fn swift_conformsToProtocol(metadata:*const c_void,protocol:*const c_void)->*const c_void;
    }
    let hashable = unsafe {swift_conformsToProtocol(token.value,resolve(env!("HASHABLE_PROTOCOL_SYMBOL")))};
    assert!(!hashable.is_null());
    let hashable_desc = resolve("$s15ManagedSettings5TokenVyxGSHAAMc");
    category_image(hashable_desc,b"/ManagedSettings");
    assert!(unsafe {read(hashable.cast::<*const c_void>(),page)} == hashable_desc);
    let set_fn: unsafe extern "Swift" fn(usize,*const c_void,*const c_void)->MetadataResponse =
        unsafe {std::mem::transmute(resolve(env!("SET_METADATA_SYMBOL")))};
    let set = unsafe {set_fn(0,token.value,hashable)};
    assert!(set.state == 0 && !set.value.is_null());
    assert!(unsafe {read(set.value.cast::<*const c_void>().wrapping_add(1),page)} == resolve(env!("SET_DESCRIPTOR_SYMBOL")));
    assert!(unsafe {read(set.value.cast::<*const c_void>().wrapping_add(2),page)} == token.value);
    assert!(unsafe {read(set.value.cast::<*const c_void>().wrapping_add(3),page)} == hashable);
    let table = unsafe {read(set.value.cast::<*const ValueWitnessTable>().wrapping_sub(1),page)};
    assert!(!table.is_null() && unsafe {read(&raw const (*table).flags,page)} & 0x10000 != 0);
    let set = Witnesses::load(set.value,8,page);
    let getter = resolve("$s14FamilyControls0A17ActivitySelectionV14categoryTokensShy15ManagedSettings5TokenVyAE0C8CategoryVGGvg");
    category_image(getter,b"/FamilyControls");
    assert!(word(getter,0,page) == 0xf9400a80);
    category_runtime_stub(branch(getter,4,false,page),b"/FamilyControls","swift_bridgeObjectRetain",page);
    let count = resolve(env!("SET_COUNT_SYMBOL"));
    category_image(count,b"/libswiftCore.dylib");
    category_words(count,&[(0,0xffffffff,0xf242081f),(4,0xffffffff,0x540000e0),
        (32,0xffffffff,0xf9400800),(36,0xffffffff,0xd65f03c0)],page);
    Box::new(CategoryTokenCounter {set,token:token.value,hashable,getter,
        count:unsafe {std::mem::transmute(count)},page,
        explicit_gets:AtomicUsize::new(0),explicit_counts:AtomicUsize::new(0)})
}
impl CategoryTokenCounter {
    fn read(&self)->usize {
        assert!(unsafe {pthread_main_np()} == 1);
        assert!(!COUNT_READING.swap(true,Ordering::AcqRel));
        let _guard = CountReading;
        let state = STATE.load(Ordering::Acquire);
        assert!(!state.is_null());
        let state = unsafe {&*state.cast::<State<'static>>()};
        state.proof.check();
        assert!(state.proof.n(Count::Active) == 0);
        let source = {let value=state.value.borrow();
            assert!(value.initialized && !value.binding && value.witnesses.size==56);value.ptr()};
        let mut returned=Value::allocate(&self.set,state.proof,false);
        self.explicit_gets.fetch_add(1,Ordering::SeqCst);
        let scalar=unsafe {rust_selection_applications(source,self.getter)};
        unsafe {returned.ptr().cast::<usize>().write(scalar)};
        returned.initialized=true;
        let mut copied=Value::allocate(&self.set,state.proof,false);
        copied.copy(&returned);
        drop(returned);
        let receiver=unsafe {read(copied.ptr().cast::<usize>(),self.page)};
        assert!(receiver!=0 && receiver & 0xc000000000000001 == 0,"unaudited Set representation");
        self.explicit_counts.fetch_add(1,Ordering::SeqCst);
        let count=unsafe {(self.count)(receiver,self.token,self.hashable)};
        drop(copied);
        assert!(count>=0);
        count as usize
    }
}
// Future integration, ONLY after independent ABI/ownership approval:
// empty baseline -> real category tap -> fresh setter delta + read()>0;
// Selection snapshots copy/destroy with measured deltas; keep host/context pinned.
// No category-instance/application equivalence, no disk-roundtrip/full-teardown claim.
