// Request-free arm64 Swift heap-owner lifetime probe.
use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
static DESTROYS: AtomicUsize = AtomicUsize::new(0);
static DEALLOCS: AtomicUsize = AtomicUsize::new(0);
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
unsafe extern "C" { fn getpagesize()->i32; fn dlsym(handle:*mut c_void, name:*const std::ffi::c_char)->*mut c_void; }
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
  }
 }
}
fn main() {
 inspect_binding_entry();
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
 println!("rust_owner_drop_calls=1\ncontext_destroy_calls=1\ncontext_dealloc_calls=1\ncopy_survived_original_release=true\nbinding_verified=false\nfull_teardown_verified=false");
}
