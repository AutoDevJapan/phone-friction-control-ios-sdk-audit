// Held audit-host teardown wiring. Public Foundation weak tables observe only
// the four named UIKit objects; they do not hook framework deallocation or
// retain any target. Same-SDK declarations and independent review are required
// before this opt-in candidate may be enabled for an actual acceptance run.

static CAPTURE_INITIAL_CONTROLLER: AtomicUsize = AtomicUsize::new(0);
static CAPTURE_UI_CLOSING: AtomicBool = AtomicBool::new(false);
static CAPTURE_INITIAL_RELEASES: AtomicUsize = AtomicUsize::new(0);
static CAPTURE_BUTTON_TEMP_RETAINS: AtomicUsize = AtomicUsize::new(0);
static CAPTURE_BUTTON_TEMP_RELEASES: AtomicUsize = AtomicUsize::new(0);
static CAPTURE_HOSTING_RELEASES: AtomicUsize = AtomicUsize::new(0);
static CAPTURE_WINDOW_RELEASES: AtomicUsize = AtomicUsize::new(0);
static CAPTURE_DELEGATE_RELEASES: AtomicUsize = AtomicUsize::new(0);
static CAPTURE_WEAK_TABLES: [AtomicUsize; 4] = [const { AtomicUsize::new(0) }; 4];
static CAPTURE_CLOSE_EVIDENCE: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
static CAPTURE_UI_STARTED: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
static CAPTURE_BOOTSTRAP_WATCHDOG: std::sync::Mutex<Option<CaptureBootstrapWatchdog>> =
    std::sync::Mutex::new(None);
static CAPTURE_BOOTSTRAP_JOINS: AtomicUsize = AtomicUsize::new(0);

struct CaptureBootstrapWatchdog {
    done: std::sync::Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Drop for CaptureBootstrapWatchdog {
    fn drop(&mut self) {
        *self.done.0.lock().unwrap() = true;
        self.done.1.notify_all();
        if let Some(thread) = self.thread.take() {
            thread.join().expect("owned bootstrap watchdog failed");
            CAPTURE_BOOTSTRAP_JOINS.fetch_add(1, Ordering::SeqCst);
        }
    }
}
fn capture_start_bootstrap_watchdog() {
    let done = std::sync::Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
    let signal = done.clone();
    let thread = std::thread::spawn(move || {
        let (lock, wake) = &*signal;
        let result = wake
            .wait_timeout_while(lock.lock().unwrap(), Duration::from_secs(180), |finished| {
                !*finished
            })
            .unwrap();
        if !*result.0 {
            eprintln!("Owned audit bootstrap deadline exceeded");
            std::process::exit(1);
        }
    });
    let mut owner = CAPTURE_BOOTSTRAP_WATCHDOG.lock().unwrap();
    assert!(owner.is_none());
    *owner = Some(CaptureBootstrapWatchdog {
        done,
        thread: Some(thread),
    });
}

#[link(name = "System")]
unsafe extern "C" {
    fn dispatch_time(when: u64, delta: i64) -> u64;
    fn dispatch_after_f(
        when: u64,
        queue: Id,
        context: *mut c_void,
        work: unsafe extern "C" fn(*mut c_void),
    );
}

fn capture_weak_record(index: usize, object: Id) {
    assert!(!object.is_null());
    assert!(CAPTURE_WEAK_TABLES[index].load(Ordering::Acquire) == 0);
    unsafe {
        let class = objc_getClass(c"NSHashTable".as_ptr());
        assert!(!class.is_null());
        let table = objc_retain(send0(class, c"weakObjectsHashTable"));
        assert!(!table.is_null());
        send_id(table, c"addObject:", object);
        assert!(send0(table, c"anyObject") == object);
        CAPTURE_WEAK_TABLES[index].store(table.addr(), Ordering::Release);
    }
}

fn capture_write_ui_evidence(
    phase: &str,
    weak_nil: Option<[bool; 4]>,
    state: &str,
    accepted: bool,
) {
    let home = std::env::var_os("HOME").expect("owned audit HOME missing");
    let documents = std::path::PathBuf::from(home).join("Documents");
    std::fs::create_dir_all(&documents).expect("create owned audit result directory");
    let weak_value = |index: usize| {
        weak_nil
            .map(|values| values[index].to_string())
            .unwrap_or_else(|| "unobserved".to_owned())
    };
    let evidence = format!(
        "phase={phase}\n{}{}weak_initial_controller_nil={}\nweak_hosting_controller_nil={}\nweak_window_nil={}\nweak_button_nil={}\ninitial_controller_reference_releases={}\nbutton_temporary_retains={}\nbutton_temporary_releases={}\nhosting_reference_releases={}\nwindow_reference_releases={}\ndelegate_owned_reference_releases={}\nbootstrap_watchdog_joins={}\nwindow_pointer_cleared={}\ncontroller_pointer_cleared={}\nbutton_pointer_cleared={}\nnamed_ui_and_state_graph_release_verified={accepted}\nfull_framework_teardown_not_established=true\nindividual_app_selection_verified=false\nmonitoring_verified=false\nshielding_verified=false\n",
        CAPTURE_CLOSE_EVIDENCE.lock().unwrap().as_str(),
        state,
        weak_value(0),
        weak_value(1),
        weak_value(2),
        weak_value(3),
        CAPTURE_INITIAL_RELEASES.load(Ordering::Acquire),
        CAPTURE_BUTTON_TEMP_RETAINS.load(Ordering::Acquire),
        CAPTURE_BUTTON_TEMP_RELEASES.load(Ordering::Acquire),
        CAPTURE_HOSTING_RELEASES.load(Ordering::Acquire),
        CAPTURE_WINDOW_RELEASES.load(Ordering::Acquire),
        CAPTURE_DELEGATE_RELEASES.load(Ordering::Acquire),
        CAPTURE_BOOTSTRAP_JOINS.load(Ordering::Acquire),
        WINDOW.load(Ordering::Acquire) == 0,
        HOSTING_CONTROLLER.load(Ordering::Acquire) == 0,
        REQUEST_BUTTON.load(Ordering::Acquire) == 0,
    );
    std::fs::write(documents.join("captured-selection-teardown.txt"), evidence)
        .expect("write owned audit teardown evidence");
}

fn capture_ui_close_if_ready() -> bool {
    if !CHOICE_ARMED.load(Ordering::Acquire) {
        return false;
    }
    let (_, selected) = super::mutable_binding::category_token_snapshot();
    let setters = super::mutable_binding::setter_calls();
    if selected == 0 || setters <= SETTER_BASELINE.load(Ordering::Acquire) {
        return true;
    }
    if CAPTURE_UI_CLOSING.swap(true, Ordering::AcqRel) {
        return true;
    }
    let outcome = std::panic::catch_unwind(|| {
        let evidence = super::mutable_binding::selection_capture_close_probe();
        *CAPTURE_CLOSE_EVIDENCE.lock().unwrap() = evidence;
        assert!(CAPTURE_UI_STARTED.set(Instant::now()).is_ok());
        capture_write_ui_evidence("copied_binding_close_probe_complete", None, "", false);
        // Return from the current button event before releasing its hierarchy.
        unsafe {
            dispatch_async_f(
                (&raw const _dispatch_main_q).cast_mut().cast(),
                std::ptr::null_mut(),
                capture_ui_detach,
            )
        };
    });
    if outcome.is_err() {
        std::process::exit(1);
    }
    true
}

unsafe extern "C" fn capture_ui_detach(_: *mut c_void) {
    let outcome = std::panic::catch_unwind(|| unsafe {
        // Clear raw entry points before detaching any owning hierarchy.
        let window = WINDOW.swap(0, Ordering::AcqRel) as Id;
        let controller = HOSTING_CONTROLLER.swap(0, Ordering::AcqRel) as Id;
        let button = REQUEST_BUTTON.swap(0, Ordering::AcqRel) as Id;
        let delegate = DELEGATE.swap(0, Ordering::AcqRel) as Id;
        assert!(
            !window.is_null() && !controller.is_null() && !button.is_null() && !delegate.is_null()
        );
        assert!(CAPTURE_INITIAL_CONTROLLER.load(Ordering::Acquire) == 0);
        send_id(window, c"setRootViewController:", std::ptr::null_mut());
        let hide: unsafe extern "C" fn(Id, Sel, bool) =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn(Id, Sel) -> usize);
        hide(window, sel_registerName(c"setHidden:".as_ptr()), true);
        send_void(button, c"removeFromSuperview");
        swift_unknownObjectRelease(controller);
        CAPTURE_HOSTING_RELEASES.fetch_add(1, Ordering::SeqCst);
        swift_unknownObjectRelease(window);
        CAPTURE_WINDOW_RELEASES.fetch_add(1, Ordering::SeqCst);
        objc_release(delegate); // only our explicit launched() retain, not UIKit's owner
        CAPTURE_DELEGATE_RELEASES.fetch_add(1, Ordering::SeqCst);
        super::mutable_binding::release_selection_capture_root();
        capture_write_ui_evidence("owned_ui_references_released", None, "", false);
        capture_schedule_ui_poll();
    });
    if outcome.is_err() {
        std::process::exit(1);
    }
}

fn capture_schedule_ui_poll() {
    unsafe {
        let when = dispatch_time(0, 200_000_000);
        dispatch_after_f(
            when,
            (&raw const _dispatch_main_q).cast_mut().cast(),
            std::ptr::null_mut(),
            capture_ui_poll,
        );
    }
}

unsafe extern "C" fn capture_ui_poll(_: *mut c_void) {
    let outcome = std::panic::catch_unwind(|| unsafe {
        let weak_nil = std::array::from_fn(|index| {
            let table = CAPTURE_WEAK_TABLES[index].load(Ordering::Acquire) as Id;
            assert!(!table.is_null());
            send0(table, c"anyObject").is_null()
        });
        let state = if weak_nil.iter().all(|empty| *empty) {
            super::mutable_binding::finish_selection_capture_resources().ok()
        } else {
            None
        };
        if let Some(state) = state {
            assert!(CAPTURE_INITIAL_RELEASES.load(Ordering::Acquire) == 1);
            assert!(CAPTURE_BUTTON_TEMP_RETAINS.load(Ordering::Acquire) == 1);
            assert!(CAPTURE_BUTTON_TEMP_RELEASES.load(Ordering::Acquire) == 1);
            assert!(CAPTURE_HOSTING_RELEASES.load(Ordering::Acquire) == 1);
            assert!(CAPTURE_WINDOW_RELEASES.load(Ordering::Acquire) == 1);
            assert!(CAPTURE_DELEGATE_RELEASES.load(Ordering::Acquire) == 1);
            for slot in &CAPTURE_WEAK_TABLES {
                let table = slot.swap(0, Ordering::AcqRel) as Id;
                assert!(!table.is_null());
                objc_release(table);
            }
            let bootstrap = CAPTURE_BOOTSTRAP_WATCHDOG.lock().unwrap().take();
            assert!(bootstrap.is_some());
            drop(bootstrap);
            assert!(CAPTURE_BOOTSTRAP_JOINS.load(Ordering::Acquire) == 1);
            capture_write_ui_evidence("complete", Some(weak_nil), &state, true);
            std::process::exit(0);
        }
        capture_write_ui_evidence(
            "waiting_for_owned_graph",
            Some(weak_nil),
            &super::mutable_binding::selection_capture_partial_evidence(),
            false,
        );
        if CAPTURE_UI_STARTED.get().unwrap().elapsed() >= Duration::from_secs(20) {
            capture_write_ui_evidence(
                "finite_teardown_deadline_exceeded",
                Some(weak_nil),
                &super::mutable_binding::selection_capture_partial_evidence(),
                false,
            );
            std::process::exit(1);
        }
        capture_schedule_ui_poll();
    });
    if outcome.is_err() {
        std::process::exit(1);
    }
}
