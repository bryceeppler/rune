//! The system asking a running app to end, per platform.
//!
//! macOS asks the app delegate `applicationShouldTerminate:` for Quit from the menu, ⌘Q, the
//! Dock and a logout, and lets it answer later. winit's delegate doesn't implement it, so Rune
//! adds the method (`install`) as it does for reopening: the method sends the quit to Rune's
//! loop and answers "later"; the loop asks each window's hook and `reply` answers AppKit.
//!
//! Elsewhere nothing asks: Windows ends a session with `WM_ENDSESSION`, after which the process
//! may die at any moment, and a desktop session on Unix ends with `SIGTERM`. Both run each
//! window's `on_close` before the app goes (`on_session_end`, `on_sigterm`).

/// Who asked the app to quit, and so who hears the answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QuitFrom {
    /// `Tx::request_quit` off macOS: the loop exits once every window proceeds.
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    Rune,
    /// AppKit's `applicationShouldTerminate:`, waiting for `reply`.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    AppKit,
}

#[cfg(target_os = "macos")]
pub(crate) use mac::{install, reply, terminate};
#[cfg(all(unix, not(target_os = "ios"), not(target_os = "android")))]
pub(crate) use unix::on_sigterm;
#[cfg(target_os = "windows")]
pub(crate) use win::{on_session_end, watch_session_end};

#[cfg(target_os = "macos")]
mod mac {
    use std::cell::{Cell, RefCell};
    use std::ffi::{c_char, c_void};
    use objc2::{msg_send, sel};
    use objc2::runtime::{AnyClass, AnyObject, Bool, Sel};
    use objc2_app_kit::NSApplication;
    use objc2_foundation::MainThreadMarker;

    const TERMINATE_NOW: usize = 1;
    const TERMINATE_LATER: usize = 2;

    thread_local! {
        /// Sends the quit to Rune's loop; false once the loop is gone.
        static ASK: RefCell<Option<Box<dyn Fn() -> bool>>> = const { RefCell::new(None) };
        static HOOKED: Cell<*const AnyClass> = const { Cell::new(std::ptr::null()) };
    }

    unsafe extern "C" {
        fn class_addMethod(class: *const AnyClass, selector: Sel, implementation: *const c_void, types: *const c_char) -> Bool;
    }

    extern "C" fn should_terminate(_: *mut AnyObject, _: Sel, _: *mut AnyObject) -> usize {
        let sent = std::panic::catch_unwind(|| {
            ASK.with(|ask| ask.try_borrow().ok().and_then(|f| f.as_ref().map(|f| f())).unwrap_or(false))
        }).unwrap_or_else(|_| { eprintln!("rune: quit callback panicked"); false });
        if sent { TERMINATE_LATER } else { TERMINATE_NOW }
    }

    /// Calls `ask` on the main thread each time AppKit asks the app to quit; it returns whether
    /// the quit reached Rune's loop, which then owes AppKit a `reply`. Adds the method to the
    /// app delegate's class unless the class already answers it (a later winit, or an app's own
    /// delegate), in which case quitting stays with that delegate. Main thread only.
    pub(crate) fn install(ask: impl Fn() -> bool + 'static) {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let app = NSApplication::sharedApplication(mtm);
        let selector = sel!(applicationShouldTerminate:);
        // SAFETY: AppKit calls and runtime changes on the main thread. The added function has
        // the selector's ABI (NSApplicationTerminateReply is an NSUInteger), and adding a method
        // the class lacks leaves its other methods and instances alone.
        unsafe {
            let delegate: *mut AnyObject = msg_send![&app, delegate];
            let Some(delegate) = delegate.as_ref() else { return };
            let class = delegate.class();
            if !HOOKED.with(|hooked| std::ptr::eq(hooked.get(), class)) {
                let answers: bool = msg_send![delegate, respondsToSelector: selector];
                if answers { return; }
                let imp = should_terminate as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject) -> usize;
                if !class_addMethod(class, selector, imp as *const c_void, c"Q@:@".as_ptr()).as_bool() {
                    eprintln!("rune: could not add the quit method to the app delegate");
                    return;
                }
                HOOKED.with(|hooked| hooked.set(class));
                let _: () = msg_send![&app, setDelegate: delegate];
            }
        }
        ASK.with(|a| if let Ok(mut a) = a.try_borrow_mut() { *a = Some(Box::new(ask)); });
    }

    /// Answers the quit `install`'s method deferred. From the main queue, outside winit's
    /// handler: a quit that goes ahead ends with `applicationWillTerminate:`, which winit
    /// delivers to the handler.
    pub(crate) fn reply(proceed: bool) {
        crate::platform::main_queue::on_main(move || {
            let Some(mtm) = MainThreadMarker::new() else { return };
            // SAFETY: AppKit call on the main thread.
            unsafe { NSApplication::sharedApplication(mtm).replyToApplicationShouldTerminate(proceed) };
        });
    }

    /// Asks AppKit to quit as Quit from the menu does, from the main queue: AppKit runs a
    /// deferred quit in a nested run loop, which must not sit inside winit's handler.
    pub(crate) fn terminate() {
        crate::platform::main_queue::on_main(|| {
            let Some(mtm) = MainThreadMarker::new() else { return };
            // SAFETY: AppKit call on the main thread.
            unsafe { NSApplication::sharedApplication(mtm).terminate(None) };
        });
    }
}

#[cfg(all(unix, not(target_os = "ios"), not(target_os = "android")))]
mod unix {
    use std::sync::atomic::{AtomicI32, Ordering};

    static WRITE: AtomicI32 = AtomicI32::new(-1);

    extern "C" fn on_signal(_: libc::c_int) {
        let fd = WRITE.load(Ordering::Relaxed);
        // SAFETY: write(2) is async-signal-safe; the byte outlives the call.
        if fd >= 0 { unsafe { libc::write(fd, [1u8].as_ptr().cast(), 1) }; }
    }

    /// Runs `ended` on a thread of its own when the process receives `SIGTERM`, unless the app
    /// already handles the signal. The handler resets itself, so a second `SIGTERM` ends the
    /// process at once even if the loop never gets to `ended`'s work.
    pub(crate) fn on_sigterm(ended: impl Fn() + Send + 'static) {
        // SAFETY: plain libc calls on locals; the handler only writes to the pipe.
        unsafe {
            let mut old: libc::sigaction = std::mem::zeroed();
            if libc::sigaction(libc::SIGTERM, std::ptr::null(), &mut old) != 0 || old.sa_sigaction != libc::SIG_DFL { return; }
            let mut fds = [0; 2];
            if libc::pipe(fds.as_mut_ptr()) != 0 { return; }
            for fd in fds { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC); }
            WRITE.store(fds[1], Ordering::Relaxed);
            let read = fds[0];
            std::thread::Builder::new().name("rune-sigterm".into()).spawn(move || {
                let mut byte = 0u8;
                if libc::read(read, (&raw mut byte).cast(), 1) == 1 { ended(); }
            }).ok();
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t;
            action.sa_flags = libc::SA_RESETHAND | libc::SA_RESTART;
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaction(libc::SIGTERM, &action, std::ptr::null_mut());
        }
    }
}

#[cfg(target_os = "windows")]
mod win {
    use std::cell::RefCell;

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{WM_ENDSESSION, WM_NCDESTROY};

    const SUBCLASS: usize = 0x5255_4e45;

    thread_local! {
        static ENDED: RefCell<Option<Box<dyn FnMut()>>> = const { RefCell::new(None) };
    }

    /// Runs `ended` on the event loop's thread when Windows ends the session, before the
    /// window that hears it returns from `WM_ENDSESSION`, after which the process may end at any
    /// moment. Runs once.
    pub(crate) fn on_session_end(ended: impl FnMut() + 'static) {
        ENDED.with(|e| *e.borrow_mut() = Some(Box::new(ended)));
    }

    /// Makes `window` hear the session end for `on_session_end`.
    pub(crate) fn watch_session_end(window: &winit::window::Window) {
        let Some(hwnd) = crate::platform::win32::hwnd(window) else { return };
        // SAFETY: the subclass carries no data and is removed at WM_NCDESTROY.
        unsafe { SetWindowSubclass(hwnd, Some(subclass), SUBCLASS, 0) };
    }

    unsafe extern "system" fn subclass(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM, _: usize, _: usize) -> LRESULT {
        match message {
            WM_ENDSESSION if wparam != 0 => {
                let ended = ENDED.with(|e| e.try_borrow_mut().ok().and_then(|mut e| e.take()));
                if let Some(mut ended) = ended { ended(); }
            }
            // SAFETY: removes this subclass from the window it was set on.
            WM_NCDESTROY => unsafe { RemoveWindowSubclass(hwnd, Some(subclass), SUBCLASS); },
            _ => {}
        }
        // SAFETY: forwards to the next window procedure.
        unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
    }
}
