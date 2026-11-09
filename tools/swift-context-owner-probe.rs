// Request-free owner audit and noncalling Binding ABI capture.
// Request-free arm64 Swift heap-owner lifetime probe.
#![feature(abi_swift)]
use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
static DESTROYS: AtomicUsize = AtomicUsize::new(0);
static DEALLOCS: AtomicUsize = AtomicUsize::new(0);
static GETS:AtomicUsize=AtomicUsize::new(0);
static OWNER_DROPS: AtomicUsize = AtomicUsize::new(0);
struct Payload { value: usize }
impl Drop for Payload { fn drop(&mut self) { OWNER_DROPS.fetch_add(1, Ordering::SeqCst); } }
#[repr(C)]
struct LocalMetadata { kind: usize, first_capture: u32, description: *const c_void }
#[repr(C)]
struct FullMetadata { layout: *const c_void, destroy: unsafe extern "C" fn(), witnesses: *const c_void, local: LocalMetadata }
unsafe impl Sync for FullMetadata {}
unsafe extern "C" { fn rust_context_destroy(); }
static METADATA: FullMetadata = FullMetadata {
 layout: std::ptr::null(), destroy: rust_context_destroy, witnesses: std::ptr::null(),
 local: LocalMetadata { kind: 0x400, first_capture: 16, description: std::ptr::null() },
};
#[link(name="swiftCore")]
unsafe extern "C" {
 fn swift_allocObject(metadata: *const LocalMetadata, size: usize, alignment: usize) -> *mut c_void;
 fn swift_deallocObject(object: *mut c_void, size: usize, alignment: usize);
}
#[link(name="swiftCore")]
unsafe extern "C" {
 fn swift_retain(object: *mut c_void) -> *mut c_void;
 fn swift_release(object: *mut c_void);
}
core::arch::global_asm!(
 ".text", ".p2align 2", ".globl _rust_context_destroy", "_rust_context_destroy:",
 ".cfi_startproc", "stp x29, x30, [sp, #-16]!", ".cfi_def_cfa_offset 16",
 ".cfi_offset 29, -16", ".cfi_offset 30, -8", "mov x29, sp",
 "mov x0, x20", "bl _rust_context_destroy_c",
 "ldp x29, x30, [sp], #16", ".cfi_restore 29", ".cfi_restore 30",
 ".cfi_def_cfa_offset 0", "ret", ".cfi_endproc",
);
#[unsafe(no_mangle)]
unsafe extern "C" fn rust_context_destroy_c(object: *mut c_void) {
 DESTROYS.fetch_add(1, Ordering::SeqCst);
 unsafe {
  let payload = object.cast::<u8>().add(16).cast::<*mut Payload>().read();
  drop(Box::from_raw(payload));
  swift_deallocObject(object, 24, 7);
 }
 DEALLOCS.fetch_add(1, Ordering::SeqCst);
}
#[link(name="SwiftUI", kind="framework")]
unsafe extern "C" {}
#[repr(C)]
#[derive(Default)]
struct DlInfo { filename:*const std::ffi::c_char, base:*mut c_void, symbol:*const std::ffi::c_char, address:*mut c_void }
unsafe extern "C" { fn dladdr(p:*const c_void,info:*mut DlInfo)->i32; fn getpagesize()->i32; fn dlsym(handle:*mut c_void, name:*const std::ffi::c_char)->*mut c_void; }
#[repr(C)]
struct MetadataResponse { value:*const c_void, state:usize }
#[repr(C)]
struct Vwt { functions:[*const c_void;8], size:usize, stride:usize, flags:u32, extra:u32 }
fn inspect_binding_metadata() -> (*const c_void,*const Vwt) {
 unsafe {
  let handle=std::ptr::without_provenance_mut(usize::MAX-1);
  let boolean=dlsym(handle,c"$sSbN".as_ptr());
  let entry=dlsym(handle,c"$s7SwiftUI7BindingVMa".as_ptr());
  assert!(!boolean.is_null() && !entry.is_null());
  let accessor:unsafe extern "Swift" fn(usize,*const c_void)->MetadataResponse=std::mem::transmute(entry);
  let result=accessor(0,boolean);
  assert_eq!(result.state,0); assert!(!result.value.is_null());
  let page=getpagesize(); assert!(page>0); let page=page as usize;
  assert_eq!(result.value.addr()%std::mem::align_of::<usize>(),0);
  assert!(page-result.value.addr()%page>=3*std::mem::size_of::<usize>());
  assert_eq!(result.value.cast::<usize>().read(),0x200);
  assert_eq!(result.value.cast::<*const c_void>().add(2).read(),boolean.cast_const());
  let header=result.value.cast::<*const Vwt>().wrapping_sub(1);
  assert_eq!(header.addr()%std::mem::align_of::<usize>(),0);
  assert!(page-header.addr()%page>=std::mem::size_of::<usize>());
  let vwt=header.read(); assert!(!vwt.is_null());
  assert_eq!(vwt.addr()%std::mem::align_of::<Vwt>(),0);
  assert!(page-vwt.addr()%page>=std::mem::size_of::<Vwt>());
  let vwt=&*vwt;
  assert!(vwt.flags&0x00400000==0);
  let alignment=((vwt.flags&255)+1) as usize;
  assert!(alignment.is_power_of_two());
  let stride=vwt.size.checked_add(alignment-1).unwrap()&!(alignment-1);
  assert!(vwt.size>0 && vwt.stride==stride);
  assert!(vwt.functions.iter().all(|f| !f.is_null()));
  println!("binding_bool_metadata_complete=true; size={}; stride={}; alignment={}; nonpod={}",vwt.size,vwt.stride,(vwt.flags&255)+1,vwt.flags&0x10000!=0);
  #[cfg(target_os="ios")]
  for (name,slot) in [("destroy",1usize),("copy",2usize)] {
   let pointer=vwt.functions[slot];
   let mut info=DlInfo::default(); assert_ne!(dladdr(pointer,&mut info),0);
   assert!(!info.base.is_null()&&!info.filename.is_null());
   let filename=std::ffi::CStr::from_ptr(info.filename).to_bytes();
   assert!(filename.ends_with(b"/SwiftUICore")||filename.ends_with(b"/libswiftCore.dylib"));
   assert_eq!(pointer.addr()%4,0);
   let count=160usize.min(page-pointer.addr()%page);
   print!("binding_vwt_entry={name}; bytes=");
   for byte in std::slice::from_raw_parts(pointer.cast::<u8>(),count) { print!("{byte:02x}"); }
   println!();
  }
  (result.value,vwt as *const Vwt)
 }
}
static CALLEES_VERIFIED:AtomicUsize=AtomicUsize::new(0);
fn inspect_binding_entry() {
 unsafe {
  let page=getpagesize(); assert!(page>0); let page=page as usize;
  for (label,name) in [
   ("bool_metadata_data", c"$sSbN"),
   ("binding_metadata", c"$s7SwiftUI7BindingVMa"),
   ("binding_init", c"$s7SwiftUI7BindingV3get3setACyxGxyc_yxctcfC"),
   ("binding_get", c"$s7SwiftUI7BindingV12wrappedValuexvg"),
   ("binding_set", c"$s7SwiftUI7BindingV12wrappedValuexvs"),
  ] {
   let entry=dlsym(std::ptr::without_provenance_mut(usize::MAX-1),name.as_ptr());
   assert!(!entry.is_null(), "public Binding symbol missing");
   // One page-local prefix only; never call these unreviewed entries.
   if label == "bool_metadata_data" {
    // Type metadata contains runtime pointers. Report kind only, never bytes.
    assert_eq!(entry.addr()%std::mem::align_of::<usize>(),0);
    assert!(page-entry.addr()%page>=std::mem::size_of::<usize>());
    let kind=entry.cast::<usize>().read();
    assert_eq!(kind,0x200);
    println!("binding_metadata_data=bool; kind_struct=true");
    continue;
   }
   let count=64usize.min(page-entry.addr()%page);
   print!("binding_entry={label}; bytes=");
   for b in std::slice::from_raw_parts(entry.cast::<u8>(),count) { print!("{b:02x}"); }
   println!();
   if label == "binding_init" {
    let offset=if cfg!(target_os="ios") {16usize} else {28usize};
    assert_eq!(entry.addr()%4,0); assert!(page-entry.addr()%page>=offset+4);
    let instruction=entry.cast::<u8>().add(offset).cast::<u32>().read();
    assert_eq!(instruction&0xfc000000,0x14000000);
    let relative=((instruction<<6) as i32>>6) as isize*4;
    let displacement=(offset as isize).checked_add(relative).unwrap();
    let address=entry.addr().checked_add_signed(displacement).unwrap();
    let target=entry.cast::<u8>().wrapping_offset(displacement).cast::<c_void>();
    assert_eq!(address,target.addr()); assert_eq!(address%4,0);
    let mut origin=DlInfo::default(); let mut destination=DlInfo::default();
    assert_ne!(dladdr(entry,&mut origin),0); assert_ne!(dladdr(target,&mut destination),0);
    assert!(!origin.base.is_null()&&origin.base==destination.base);
    if cfg!(target_os="ios") {
     assert!(page-target.addr()%page>=0x10c);
     for (offset,expected) in [(0x9cusize,c"swift_retain"),(0xa4,c"swift_retain"),(0x100,c"swift_release"),(0x108,c"swift_release")] {
      let word=target.cast::<u8>().add(offset).cast::<u32>().read();
      assert_eq!(word&0xfc000000,0x94000000);
      let delta=((word<<6)as i32>>6)as isize*4;
      let displacement=(offset as isize).checked_add(delta).unwrap();
      let address=target.addr().checked_add_signed(displacement).unwrap();
      let called=target.cast::<u8>().wrapping_offset(displacement).cast::<c_void>();
      assert_eq!(called.addr(),address);assert_eq!(address%4,0);
      let mut info=DlInfo::default();assert_ne!(dladdr(called,&mut info),0);
      assert!(!info.base.is_null()&&!info.filename.is_null());
      let expected_entry=dlsym(std::ptr::without_provenance_mut(usize::MAX-1),expected.as_ptr());
      assert!(!expected_entry.is_null());
      let symbol_matches=info.address==called && !info.symbol.is_null() && {
       let name=std::ffi::CStr::from_ptr(info.symbol).to_bytes();
       name==expected.to_bytes() || name.strip_prefix(b"_")==Some(expected.to_bytes())
      };
      println!("binding_context_callee; offset={offset}; exact_entry={}; expected_symbol={symbol_matches}",called==expected_entry);
      assert!(info.base==origin.base);
      let count=32usize.min(page-called.addr()%page);
      print!("binding_context_stub; offset={offset}; bytes=");
      for byte in std::slice::from_raw_parts(called.cast::<u8>(),count) {print!("{byte:02x}");}
      println!();
      assert!(page-called.addr()%page>=12);
      let adrp=called.cast::<u32>().read();let load=called.cast::<u32>().add(1).read();let branch=called.cast::<u32>().add(2).read();
      assert_eq!(adrp&0x9f00001f,0x90000010);assert_eq!(load&0xffc003ff,0xf9400210);assert_eq!(branch,0xd61f0200);
      let immediate=(((adrp>>5)&0x7ffff)<<2)|((adrp>>29)&3);
      let signed=((immediate<<11)as i32>>11)as isize;
      let relative=signed.checked_mul(4096).unwrap();
      let slot_addr=(called.addr()&!4095usize).checked_add_signed(relative).unwrap().checked_add((((load>>10)&0xfff)as usize)*8).unwrap();
      assert_eq!(slot_addr%std::mem::align_of::<usize>(),0);assert!(page-slot_addr%page>=std::mem::size_of::<usize>());
      let displacement=slot_addr.checked_signed_diff(called.addr()).unwrap();
      let slot=called.cast::<u8>().wrapping_offset(displacement).cast::<*const c_void>();
      assert_eq!(slot.addr(),slot_addr);
      let mut slot_info=DlInfo::default();assert_ne!(dladdr(slot.cast(),&mut slot_info),0);assert_eq!(slot_info.base,origin.base);
      let resolved=slot.read(); assert!(!resolved.is_null());
      assert_eq!(resolved,expected_entry);
      CALLEES_VERIFIED.fetch_add(1,Ordering::SeqCst);
      println!("binding_context_stub_resolved; offset={offset}; exact_expected_entry={}",resolved==expected_entry);


     }
    }
    let bytes=512usize.min(page-target.addr()%page);
    print!("binding_init_helper; bytes=");
    for byte in std::slice::from_raw_parts(target.cast::<u8>(),bytes) { print!("{byte:02x}"); }
    println!();
   }

  }
 }
}
core::arch::global_asm!(
 ".text", ".p2align 2", ".globl _binding_owned_init", "_binding_owned_init:",
 ".cfi_startproc",
 "stp x20, x30, [sp, #-16]!", ".cfi_def_cfa_offset 16", ".cfi_offset 20, -16", ".cfi_offset 30, -8", "mov x8, x0", "mov x9, x4", "mov x10, x5",
 "mov x0, x1", "mov x1, x10", "mov x4, x3", "mov x3, x10", "blr x9",
 "ldp x20, x30, [sp], #16", ".cfi_restore 20", ".cfi_restore 30", ".cfi_def_cfa_offset 0", "ret", ".cfi_endproc",
 ".globl _binding_owned_get", "_binding_owned_get:",
 ".cfi_startproc",
 "stp x20, x30, [sp, #-16]!", ".cfi_def_cfa_offset 16", ".cfi_offset 20, -16", ".cfi_offset 30, -8", "mov x0, x8", "mov x1, x20",
 "bl _binding_owned_get_c", "ldp x20, x30, [sp], #16", ".cfi_restore 20", ".cfi_restore 30", ".cfi_def_cfa_offset 0", "ret", ".cfi_endproc",
 ".globl _binding_owned_set", "_binding_owned_set:",
 ".cfi_startproc",
 "stp x20, x30, [sp, #-16]!", ".cfi_def_cfa_offset 16", ".cfi_offset 20, -16", ".cfi_offset 30, -8", "mov x1, x20",
 "bl _binding_owned_set_c", "ldp x20, x30, [sp], #16", ".cfi_restore 20", ".cfi_restore 30", ".cfi_def_cfa_offset 0", "ret", ".cfi_endproc",
);
unsafe extern "C" {
 fn binding_owned_init(dst:*mut c_void,get:*const c_void,set:*const c_void,metadata:*const c_void,entry:*const c_void,context:*mut c_void);
 fn binding_owned_get(); fn binding_owned_set();
}
#[unsafe(no_mangle)]
unsafe extern "C" fn binding_owned_get_c(dst:*mut u8,context:*mut c_void) {
 let payload=unsafe {context.cast::<u8>().add(16).cast::<*mut Payload>().read()};
 unsafe { dst.write(((*payload).value!=0) as u8); }
 GETS.fetch_add(1,Ordering::SeqCst);
}
#[unsafe(no_mangle)]
unsafe extern "C" fn binding_owned_set_c(src:*const u8,context:*mut c_void) {
 let payload=unsafe {context.cast::<u8>().add(16).cast::<*mut Payload>().read()};
 unsafe {(*payload).value=src.read() as usize;}
}
// Only the non-authenticated arm64 ABI is admitted by the build/target checks.
// Function slots retain their original values; no pointer stripping/resigning.
unsafe fn binding_copy_drop(metadata:*const c_void,vwt:*const Vwt) {
 assert!(cfg!(all(target_os="ios",target_env="sim",target_arch="aarch64")), "Binding copy requires the validated Simulator target");
 unsafe {
  let table=&*vwt;
  assert_eq!((table.size,table.stride,(table.flags&255)+1),(17,24,8));
  let layout=std::alloc::Layout::from_size_align(table.stride,8).unwrap();
  let original=std::alloc::alloc_zeroed(layout).cast::<c_void>();
  let copied=std::alloc::alloc_zeroed(layout).cast::<c_void>();
  assert!(!original.is_null()&&!copied.is_null());
  let context=swift_allocObject(&METADATA.local,24,7);
  assert!(!context.is_null());
  context.cast::<u8>().add(16).cast::<*mut Payload>().write(Box::into_raw(Box::new(Payload{value:0})));
  swift_retain(context); swift_retain(context); // two independent owned closures
  let handle=std::ptr::without_provenance_mut(usize::MAX-1);
  let boolean=dlsym(handle,c"$sSbN".as_ptr());
  let entry=dlsym(handle,c"$s7SwiftUI7BindingV3get3setACyxGxyc_yxctcfC".as_ptr());
  assert!(!entry.is_null());
  assert_eq!(CALLEES_VERIFIED.load(Ordering::SeqCst),4);
  for slot in [1usize,2] {
   let mut image=DlInfo::default();
   assert_ne!(dladdr(table.functions[slot],&mut image),0);
   assert!(!image.base.is_null()&&!image.filename.is_null());
   let path=std::ffi::CStr::from_ptr(image.filename).to_bytes();
   assert!(path.ends_with(b"/SwiftUICore") || path.ends_with(b"/libswiftCore.dylib"));
  }
  verify_ios_binding_calls(entry,table);
  binding_owned_init(original,binding_owned_get as *const c_void,binding_owned_set as *const c_void,boolean,entry,context);
  assert!(GETS.load(Ordering::SeqCst)>=1);
  println!("binding_initial_getter_verified=true");
  swift_release(context); // initial owner; two closure inputs were transferred
  let copy:unsafe extern "Swift" fn(*mut c_void,*const c_void,*const c_void)->*mut c_void=std::mem::transmute(table.functions[2]);
  let destroy:unsafe extern "Swift" fn(*mut c_void,*const c_void)=std::mem::transmute(table.functions[1]);
  assert_eq!(copy(copied,original,metadata),copied);
  destroy(original,metadata); std::alloc::dealloc(original.cast(),layout);
  assert_eq!(OWNER_DROPS.load(Ordering::SeqCst),1); // previous standalone payload only
  destroy(copied,metadata); std::alloc::dealloc(copied.cast(),layout);
  assert_eq!(OWNER_DROPS.load(Ordering::SeqCst),2);
  println!("binding_constructor_calls=1\nbinding_copy_calls=1\nbinding_destroy_calls=2\nbinding_context_drop_calls=1");
 }
}
unsafe fn verify_ios_binding_calls(entry:*const c_void,table:&Vwt) { unsafe {
 let page=getpagesize(); assert!(page>0);let page=page as usize;
 assert_eq!(entry.addr()%4,0);assert!(page-entry.addr()%page>=20);
 for(offset,mask,expected) in [(0usize,0x9f00001fu32,0x90000005u32),(4,0xffc003ff,0x910000a5),(8,0x9f00001f,0x90000006),(12,0xffc003ff,0x910000c6)] {
 assert_eq!(entry.cast::<u8>().add(offset).cast::<u32>().read()&mask,expected);
 }
 let instruction=entry.cast::<u8>().add(16).cast::<u32>().read();assert_eq!(instruction&0xfc000000,0x14000000);
 let relative=((instruction<<6)as i32>>6)as isize*4;let offset=16isize.checked_add(relative).unwrap();
 let address=entry.addr().checked_add_signed(offset).unwrap();
 let helper=entry.cast::<u8>().wrapping_offset(offset).cast::<c_void>();
 assert_eq!(helper.addr(),address);assert_eq!(address%4,0);
 let mut origin=DlInfo::default();let mut target=DlInfo::default();
 assert_ne!(dladdr(entry,&mut origin),0);assert_ne!(dladdr(helper,&mut target),0);assert!(!origin.base.is_null()&&origin.base==target.base);
 let pointer=helper;assert_eq!(pointer.addr()%4,0);assert!(page-pointer.addr()%page>=360);
 for (offset,mask,expected) in [
 (0,0xffffffffu32,0xa9ba6ffcu32),
 (4,0xffffffffu32,0xa90167fau32),
 (8,0xffffffffu32,0xa9025ff8u32),
 (12,0xffffffffu32,0xa90357f6u32),
 (16,0xffffffffu32,0xa9044ff4u32),
 (20,0xffffffffu32,0xa9057bfdu32),
 (24,0xffffffffu32,0x910143fdu32),
 (28,0xffffffffu32,0xd100c3ffu32),
 (32,0xffffffffu32,0xaa0603f4u32),
 (36,0xffffffffu32,0xaa0403f8u32),
 (40,0xffffffffu32,0xaa0303f6u32),
 (44,0xffffffffu32,0xaa0203f9u32),
 (48,0xffffffffu32,0xaa0103f7u32),
 (52,0xffffffffu32,0xaa0003fau32),
 (56,0xffffffffu32,0xf81883a8u32),
 (60,0xffffffffu32,0xf85f8088u32),
 (64,0xffffffffu32,0xf9402108u32),
 (68,0xffffffffu32,0x910003e9u32),
 (72,0xffffffffu32,0x91003d08u32),
 (76,0xffffffffu32,0x927ced08u32),
 (80,0xffffffffu32,0xcb080135u32),
 (84,0xffffffffu32,0x910002bfu32),
 (88,0xffffffffu32,0xaa0503e0u32),
 (92,0xffffffffu32,0x52800501u32),
 (96,0xffffffffu32,0x528000e2u32),
 (100,0xfc000000u32,0x94000000u32),
 (104,0xffffffffu32,0xaa0003e3u32),
 (108,0xffffffffu32,0xa9016418u32),
 (112,0xffffffffu32,0xf9001016u32),
 (116,0xffffffffu32,0xaa1a03e0u32),
 (120,0xffffffffu32,0xaa1703e1u32),
 (124,0xffffffffu32,0xaa1403e2u32),
 (128,0xffffffffu32,0xaa1803e4u32),
 (132,0xfc000000u32,0x94000000u32),
 (136,0xffffffffu32,0xaa0003f9u32),
 (140,0xffffffffu32,0xaa0103fbu32),
 (144,0xffffffffu32,0xaa0203fcu32),
 (148,0xffffffffu32,0xaa0303f3u32),
 (152,0xffffffffu32,0xaa1703e0u32),
 (156,0xfc000000u32,0x94000000u32),
 (160,0xffffffffu32,0xaa1603e0u32),
 (164,0xfc000000u32,0x94000000u32),
 (168,0xffffffffu32,0xaa1503e8u32),
 (172,0xffffffffu32,0xaa1703f4u32),
 (176,0xffffffffu32,0xd63f0340u32),
 (180,0xffffffffu32,0x52801fe0u32),
 (184,0xffffffffu32,0xaa1803e1u32),
 (188,0xfc000000u32,0x94000000u32),
 (192,0xffffffffu32,0xaa0003f4u32),
 (196,0x9f00001fu32,0x90000000u32),
 (200,0xffffffffu32,0x911b0000u32),
 (204,0xffffffffu32,0xaa1403e1u32),
 (208,0xfc000000u32,0x94000000u32),
 (212,0xffffffffu32,0xaa0003e2u32),
 (216,0xffffffffu32,0xd2800000u32),
 (220,0xffffffffu32,0xaa1403e1u32),
 (224,0xfc000000u32,0x94000000u32),
 (228,0xffffffffu32,0xaa0003f4u32),
 (232,0xffffffffu32,0xa9396fb9u32),
 (236,0xffffffffu32,0xa93a4fbcu32),
 (240,0xffffffffu32,0xd101c3a0u32),
 (244,0xfc000000u32,0x94000000u32),
 (248,0xffffffffu32,0xaa0003f3u32),
 (252,0xffffffffu32,0xaa1703e0u32),
 (256,0xfc000000u32,0x94000000u32),
 (260,0xffffffffu32,0xaa1603e0u32),
 (264,0xfc000000u32,0x94000000u32),
 (268,0xffffffffu32,0xf9400268u32),
 (272,0xffffffffu32,0xf85883a9u32),
 (276,0xffffffffu32,0xa9004d3fu32),
 (280,0xffffffffu32,0xaa0903f4u32),
 (284,0xffffffffu32,0xf9402d13u32),
 (288,0xffffffffu32,0xd2800000u32),
 (292,0xffffffffu32,0xaa1303e1u32),
 (296,0xfc000000u32,0x94000000u32),
 (300,0xffffffffu32,0xb9802008u32),
 (304,0xffffffffu32,0xf85f8269u32),
 (308,0xffffffffu32,0xf9401129u32),
 (312,0xffffffffu32,0x8b080280u32),
 (316,0xffffffffu32,0xaa1503e1u32),
 (320,0xffffffffu32,0xaa1303e2u32),
 (324,0xffffffffu32,0xd63f0120u32),
 (328,0xffffffffu32,0xd10143bfu32),
 (332,0xffffffffu32,0xa9457bfdu32),
 (336,0xffffffffu32,0xa9444ff4u32),
 (340,0xffffffffu32,0xa94357f6u32),
 (344,0xffffffffu32,0xa9425ff8u32),
 (348,0xffffffffu32,0xa94167fau32),
 (352,0xffffffffu32,0xa8c66ffcu32),
 (356,0xffffffffu32,0xd65f03c0u32),
 ] {assert_eq!(pointer.cast::<u8>().add(offset).cast::<u32>().read()&mask,expected);}
 let pointer=table.functions[2];assert_eq!(pointer.addr()%4,0);assert!(page-pointer.addr()%page>=140);
 for (offset,mask,expected) in [
 (0,0xffffffffu32,0xa9bb67fau32),
 (4,0xffffffffu32,0xa9015ff8u32),
 (8,0xffffffffu32,0xa90257f6u32),
 (12,0xffffffffu32,0xa9034ff4u32),
 (16,0xffffffffu32,0xa9047bfdu32),
 (20,0xffffffffu32,0x910103fdu32),
 (24,0xffffffffu32,0xaa0003f3u32),
 (28,0xffffffffu32,0xf840f420u32),
 (32,0xffffffffu32,0xaa1303e8u32),
 (36,0xffffffffu32,0xf800f500u32),
 (40,0xffffffffu32,0x927df108u32),
 (44,0xffffffffu32,0x927df029u32),
 (48,0xffffffffu32,0xf9400134u32),
 (52,0xffffffffu32,0xf9000114u32),
 (56,0xffffffffu32,0xf9400855u32),
 (60,0xffffffffu32,0xf85f82aau32),
 (64,0xffffffffu32,0x39414156u32),
 (68,0xffffffffu32,0x910022cbu32),
 (72,0xffffffffu32,0x8b080177u32),
 (76,0xffffffffu32,0x8b090178u32),
 (80,0xffffffffu32,0xf9400959u32),
 (84,0xfc000000u32,0x94000000u32),
 (88,0xffffffffu32,0xaa1403e0u32),
 (92,0xfc000000u32,0x94000000u32),
 (96,0xffffffffu32,0x8a3602e0u32),
 (100,0xffffffffu32,0x8a360301u32),
 (104,0xffffffffu32,0xaa1503e2u32),
 (108,0xffffffffu32,0xd63f0320u32),
 (112,0xffffffffu32,0xaa1303e0u32),
 (116,0xffffffffu32,0xa9447bfdu32),
 (120,0xffffffffu32,0xa9434ff4u32),
 (124,0xffffffffu32,0xa94257f6u32),
 (128,0xffffffffu32,0xa9415ff8u32),
 (132,0xffffffffu32,0xa8c567fau32),
 (136,0xffffffffu32,0xd65f03c0u32),
 ] {assert_eq!(pointer.cast::<u8>().add(offset).cast::<u32>().read()&mask,expected);}
 let pointer=table.functions[1];assert_eq!(pointer.addr()%4,0);assert!(page-pointer.addr()%page>=80);
 for (offset,mask,expected) in [
 (0,0xffffffffu32,0xa9be4ff4u32),
 (4,0xffffffffu32,0xa9017bfdu32),
 (8,0xffffffffu32,0x910043fdu32),
 (12,0xffffffffu32,0xaa0103f3u32),
 (16,0xffffffffu32,0xaa0003f4u32),
 (20,0xffffffffu32,0xf840f680u32),
 (24,0xfc000000u32,0x94000000u32),
 (28,0xffffffffu32,0x927df294u32),
 (32,0xffffffffu32,0xf9400280u32),
 (36,0xfc000000u32,0x94000000u32),
 (40,0xffffffffu32,0xf9400a61u32),
 (44,0xffffffffu32,0xf85f8028u32),
 (48,0xffffffffu32,0x39414109u32),
 (52,0xffffffffu32,0x8b09028au32),
 (56,0xffffffffu32,0x9100214au32),
 (60,0xffffffffu32,0xf9400502u32),
 (64,0xffffffffu32,0x8a290140u32),
 (68,0xffffffffu32,0xa9417bfdu32),
 (72,0xffffffffu32,0xa8c24ff4u32),
 (76,0xffffffffu32,0xd61f0040u32),
 ] {assert_eq!(pointer.cast::<u8>().add(offset).cast::<u32>().read()&mask,expected);}
 }}
fn main() {
 inspect_binding_entry();
 let binding=inspect_binding_metadata();
 assert_eq!(std::mem::offset_of!(FullMetadata, local),24);
 assert_eq!(std::mem::offset_of!(FullMetadata, destroy),8);
 unsafe {
  let object=swift_allocObject(&METADATA.local,24,7);
  assert!(!object.is_null());
  object.cast::<u8>().add(16).cast::<*mut Payload>().write(Box::into_raw(Box::new(Payload { value: 4431 })));
  assert_eq!(swift_retain(object),object);
  swift_release(object);
  assert_eq!(DESTROYS.load(Ordering::SeqCst),0);
  let payload=object.cast::<u8>().add(16).cast::<*mut Payload>().read();
  assert_eq!((*payload).value,4431);
  assert_eq!(OWNER_DROPS.load(Ordering::SeqCst),0);
  swift_release(object);
 }
 assert_eq!(DESTROYS.load(Ordering::SeqCst),1);
 assert_eq!(DEALLOCS.load(Ordering::SeqCst),1);
 assert_eq!(OWNER_DROPS.load(Ordering::SeqCst),1);


 #[cfg(all(target_os="ios",target_env="sim",target_arch="aarch64"))]
 unsafe {binding_copy_drop(binding.0,binding.1);}
 let expected=if cfg!(all(target_os="ios",target_env="sim",target_arch="aarch64")){2}else{1};
 assert_eq!(OWNER_DROPS.load(Ordering::SeqCst),expected);assert_eq!(DESTROYS.load(Ordering::SeqCst),expected);assert_eq!(DEALLOCS.load(Ordering::SeqCst),expected);
 println!("rust_owner_drop_calls={}\ncontext_destroy_calls={}\ncontext_dealloc_calls={}\ncopy_survived_original_release=true\nbinding_verified=false\nfull_teardown_verified=false",OWNER_DROPS.load(Ordering::SeqCst),DESTROYS.load(Ordering::SeqCst),DEALLOCS.load(Ordering::SeqCst));
}
