//! Hardware mouse reports, summed once per render frame; no pixel/DPI conversion.
use std::sync::Arc;
use parking_lot::Mutex;
use std::time::{Duration, Instant};

// Platform backend liveness, not retail sensitivity or motion scaling.
const NATIVE_GRACE: Duration = Duration::from_millis(100);

#[derive(Default)]
struct Counts {
    active: bool,
    native_report: bool,
    delta: (f32, f32),
    appkit_delta: (f32, f32),
    last_native: Option<Instant>,
}
impl Counts {
    // Fed by the native (macOS GameController) reader; elsewhere only the tests call it.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    fn add(&mut self, dx: f32, dy: f32) {
        if self.active {
            self.native_report = true;
            self.delta.0 += dx;
            self.delta.1 += dy;
        }
    }
    fn winit(&mut self, dx: f32, dy: f32) {
        if self.active { self.appkit_delta.0 += dx; self.appkit_delta.1 += dy; }
    }
    fn set_active(&mut self, active: bool) {
        if self.active != active || !active {
            self.delta = (0.0, 0.0);
            self.appkit_delta = (0.0, 0.0);
            self.native_report = false;
            self.last_native = None;
        }
        self.active = active;
    }
    fn take(&mut self, now: Instant) -> ((f32, f32), bool) {
        if std::mem::take(&mut self.native_report) { self.last_native = Some(now); }
        let native = self.last_native.is_some_and(|last| now.duration_since(last) < NATIVE_GRACE);
        if !native { self.last_native = None; }
        let raw = std::mem::take(&mut self.delta);
        // Discard duplicates during the grace interval, rather than replay them on handoff.
        let appkit = std::mem::take(&mut self.appkit_delta);
        (if native { raw } else { appkit }, native)
    }
}

pub(crate) struct RawMouse {
    counts: Arc<Mutex<Counts>>,
    backend: Option<bool>,
    perf: bool,
    #[cfg(target_os = "macos")]
    mice: Vec<native::Mouse>,
}
impl RawMouse {
    pub(crate) fn new() -> Self {
        let mut result = Self {
            counts: Arc::new(Mutex::new(Counts::default())),
            backend: None,
            perf: std::env::var_os("AOMAC_PERF").is_some(),
            #[cfg(target_os = "macos")]
            mice: Vec::new(),
        };
        result.refresh();
        result
    }
    fn refresh(&mut self) {
        #[cfg(target_os = "macos")]
        native::refresh(&mut self.mice, &self.counts);
    }
    pub(crate) fn set_active(&mut self, active: bool) {
        self.counts.lock().set_active(active);
    }
    pub(crate) fn push_winit(&mut self, dx: f32, dy: f32) {
        self.counts.lock().winit(dx, dy);
    }
    pub(crate) fn take(&mut self) -> (f32, f32) {
        self.refresh();
        let (delta, native) = self.counts.lock().take(Instant::now());
        if delta != (0.0, 0.0) && self.backend != Some(native) {
            self.backend = Some(native);
            if self.perf { eprintln!("mouse look backend: {}", if native { "GCMouse raw" } else { "winit fallback" }); }
        }
        delta
    }
}

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use block2::{Block, RcBlock};
    use objc2::{msg_send, runtime::{AnyClass, AnyObject}, rc::Retained};

    #[link(name = "GameController", kind = "framework")]
    unsafe extern "C" {}
    // GCDevice defaults to the main queue, which also runs our entire winit
    // redraw/render callback. Keep hardware delivery independent of that pump.
    const HANDLER_PRIORITY: isize = 2; // DISPATCH_QUEUE_PRIORITY_HIGH (queue.h)
    unsafe extern "C" {
        fn dispatch_get_global_queue(priority: isize, flags: usize) -> *mut std::ffi::c_void;
        #[cfg(test)]
        static _dispatch_main_q: std::ffi::c_void;
        #[cfg(test)]
        fn dispatch_async_f(queue: *mut std::ffi::c_void, context: *mut std::ffi::c_void, callback: extern "C" fn(*mut std::ffi::c_void));
    }
    type MouseMoved = Block<dyn Fn(*mut AnyObject, f32, f32)>;

    pub(super) struct Mouse {
        object: Retained<AnyObject>,
        input: Retained<AnyObject>,
    }
    impl Drop for Mouse {
        fn drop(&mut self) {
            let handler: Option<&MouseMoved> = None;
            unsafe { let _: () = msg_send![&*self.input, setMouseMovedHandler: handler]; }
        }
    }
    // Called on the window thread. Enumerating the current set also handles hotplug without
    // notification observers or global input monitoring. GCMouse is available on macOS 11+.
    pub(super) fn refresh(mice: &mut Vec<Mouse>, counts: &Arc<Mutex<Counts>>) {
        unsafe {
            let Some(class) = AnyClass::get(c"GCMouse") else { return };
            let array: Retained<AnyObject> = msg_send![class, mice];
            let count: usize = msg_send![&*array, count];
            mice.retain(|mouse| {
                (0..count).any(|i| {
                    let object: *mut AnyObject = msg_send![&*array, objectAtIndex: i];
                    object == Retained::as_ptr(&mouse.object).cast_mut()
                })
            });
            for i in 0..count {
                let object: Retained<AnyObject> = msg_send![&*array, objectAtIndex: i];
                if mice.iter().any(|m| Retained::as_ptr(&m.object) == Retained::as_ptr(&object)) { continue; }
                // Apple's GCMouse.h declares this profile nullable; keep AppKit usable without it.
                let input: Option<Retained<AnyObject>> = msg_send![&*object, mouseInput];
                let Some(input) = input else { continue };
                let queue = dispatch_get_global_queue(HANDLER_PRIORITY, 0);
                let _: () = msg_send![&*object, setHandlerQueue: queue];
                let shared = counts.clone();
                let handler = RcBlock::new(move |_input: *mut AnyObject, dx: f32, dy: f32| {
                    // GameController uses positive-up Y; the frontend uses positive-down.
                    shared.lock().add(dx, -dy);
                });
                let _: () = msg_send![&*input, setMouseMovedHandler: &*handler];
                mice.push(Mouse { object, input });
            }
        }
    }

    #[cfg(test)]
    #[test]
    fn native_mouse_handlers_do_not_use_the_main_render_queue() {
        // Headless API smoke: no window, HID injection or physical movement.
        unsafe {
            let queue = dispatch_get_global_queue(HANDLER_PRIORITY, 0);
            assert!(!queue.is_null());
            assert_ne!(queue, std::ptr::addr_of!(_dispatch_main_q).cast_mut());
            let mut mice = Vec::new();
            refresh(&mut mice, &Arc::new(Mutex::new(Counts::default())));
            for mouse in mice {
                let actual: *mut std::ffi::c_void = msg_send![&*mouse.object, handlerQueue];
                assert_eq!(actual, queue, "every attached mouse must bypass the main render queue");
            }
        }
    }

    #[cfg(test)]
    #[test]
    #[ignore = "timed headless dispatch probe; run alone with --nocapture"]
    fn native_handler_queue_delivery_while_main_is_not_pumped() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::mpsc;
        static MAIN_DELIVERED: AtomicBool = AtomicBool::new(false);
        extern "C" fn main_callback(_: *mut std::ffi::c_void) {
            MAIN_DELIVERED.store(true, Ordering::Release);
        }
        extern "C" fn independent_callback(context: *mut std::ffi::c_void) {
            let (start, sender) = unsafe {
                *Box::from_raw(context.cast::<(Instant, mpsc::Sender<Duration>)>())
            };
            let _ = sender.send(start.elapsed());
        }
        let (sender, receiver) = mpsc::channel::<Duration>();
        let start = Instant::now();
        unsafe {
            dispatch_async_f(std::ptr::addr_of!(_dispatch_main_q).cast_mut(), std::ptr::null_mut(), main_callback);
            let context = Box::into_raw(Box::new((start, sender))).cast();
            dispatch_async_f(dispatch_get_global_queue(HANDLER_PRIORITY, 0), context, independent_callback);
        }
        let independent = receiver.recv_timeout(Duration::from_secs(5)).expect("independent queue delivers without a main run-loop pump");
        // libtest waits for this test on its main thread instead of pumping
        // AppKit. This is a real bounded main-queue backlog, not fake timestamps.
        std::thread::sleep(Duration::from_millis(50));
        let observed = start.elapsed();
        assert!(!MAIN_DELIVERED.load(Ordering::Acquire), "run this probe alone without an AppKit pump");
        eprintln!("dispatch delivery: default-main > {observed:?} (still pending); independent {independent:?}; not hardware press-to-orbit timing");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn release_flush_preserves_between_frame_motion_without_replaying_it() {
        for native in [false, true] {
            let mut c = Counts::default();
            let now = Instant::now();
            c.set_active(true);
            if native { c.add(3.0, -2.0); } else { c.winit(3.0, -2.0); }
            // viewer drains before MouseUp reaches Frontend::input and ends capture.
            assert_eq!(c.take(now).0, (3.0, -2.0));
            c.set_active(false);
            assert_eq!(c.take(now).0, (0.0, 0.0));
            c.set_active(true);
            assert_eq!(c.take(now).0, (0.0, 0.0));
        }
    }

    #[test]
    fn reports_sum_without_acceleration_and_clear_at_capture_boundaries() {
        let mut c = Counts { active: true, ..Default::default() };
        let now = Instant::now();
        // A connected-but-silent native source must not swallow initial trackpad motion.
        c.winit(99.0, 101.0);
        assert_eq!(c.take(now), ((99.0, 101.0), false));
        c.add(2.0, -3.0);
        c.add(200.0, -300.0);
        c.winit(99.0, 99.0);
        assert_eq!(c.take(now), ((202.0, -303.0), true));
        // A delayed AppKit duplicate on the next frame is discarded, not replayed.
        c.winit(202.0, -303.0);
        assert_eq!(c.take(now + Duration::from_millis(16)), ((0.0, 0.0), true));
        c.winit(7.0, 11.0);
        assert_eq!(c.take(now + NATIVE_GRACE), ((7.0, 11.0), false));
        c.add(4.0, 5.0);
        c.winit(99.0, 99.0);
        assert_eq!(c.take(now + NATIVE_GRACE), ((4.0, 5.0), true));
        c.add(6.0, 7.0);
        c.winit(8.0, 9.0);
        c.set_active(false);
        assert_eq!(c.take(now + NATIVE_GRACE), ((0.0, 0.0), false));
        c.add(8.0, 9.0);
        c.winit(10.0, 11.0);
        assert_eq!(c.take(now + NATIVE_GRACE), ((0.0, 0.0), false));
        c.set_active(true);
        c.winit(7.0, 11.0);
        assert_eq!(c.take(now + NATIVE_GRACE), ((7.0, 11.0), false));
    }
}
