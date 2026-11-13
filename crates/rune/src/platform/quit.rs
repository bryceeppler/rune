//! The system asking a running app to end, per platform.
//!
//! macOS asks the app delegate `applicationShouldTerminate:` for Quit from the menu, ⌘Q, the
//! Dock and a logout, and lets it answer later. winit's delegate doesn't implement it, so Rune
//! adds the method (`install`) as it does for reopening: the method sends the quit to Rune's
//! loop and answers "later"; the loop asks each window's hook and `reply` answers AppKit.
//!
//! Windows asks with `WM_QUERYENDSESSION` and can't wait: the query asks each window's hook,
//! and once all proceed each window's `on_close` runs before the query answers; while one saves
//! the session waits, the app named in the shutdown screen until its quit settles
//! (`on_session`). `WM_ENDSESSION` then ends the session, after which the process may die at any
//! moment, so the `on_close` still due runs before it returns. A desktop session on Unix ends
//! with `SIGTERM`, which runs `on_close` too (`on_sigterm`).
//!
//! The one path that can't reach the app synchronously: a step that arrives while a handler runs
//! a message loop of its own (a modal dialog), which holds the app's state borrowed. A query
//! there makes the session wait, so only a forced end can arrive that way, and its `on_close`
//! runs once the handler returns, if the process lives that long (`quitting::session_later`).

/// Who asked the app to quit, and so who hears the answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QuitFrom {
    /// `Tx::request_quit` off macOS, or the last main window closing: the loop exits once
    /// every window proceeds.
    Rune,
    /// AppKit's `applicationShouldTerminate:`, waiting for `reply`.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    AppKit,
    /// Windows' session query, which the session waits on: the app exits once every window
    /// proceeds, and `unblock`s the session when one cancels.
    Session,
}

/// What Windows tells the app of a session ending.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) enum SessionStep {
    /// `WM_QUERYENDSESSION`: may the session end?
    Query,
    /// `WM_ENDSESSION` going ahead: the process may end once this returns.
    Ends,
    /// `WM_ENDSESSION` called off: the session goes on.
    Continues,
}

/// The app's answer to a session query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) enum SessionReply {
    Ends,
    /// A window is saving: the session waits, and the app exits once it is done.
    Waits,
    /// A window refused.
    Refuses,
}

#[cfg(target_os = "macos")]
pub(crate) use mac::{install, reply, terminate};
#[cfg(all(unix, not(target_os = "ios"), not(target_os = "android")))]
pub(crate) use unix::on_sigterm;
#[cfg(target_os = "windows")]
pub(crate) use win::{keep_event_window, on_session, unblock, watch_session_end};

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
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::Shutdown::{ShutdownBlockReasonCreate, ShutdownBlockReasonDestroy};
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{EnumThreadWindows, GetClassNameW, WM_CLOSE, WM_ENDSESSION, WM_NCDESTROY, WM_QUERYENDSESSION};

    use super::{SessionReply, SessionStep};

    const SUBCLASS: usize = 0x5255_4e45;

    thread_local! {
        static SESSION: RefCell<Option<Rc<dyn Fn(SessionStep) -> SessionReply>>> = const { RefCell::new(None) };
        /// The window the shutdown screen names while the session waits on the app.
        static BLOCKED: Cell<HWND> = const { Cell::new(std::ptr::null_mut()) };
    }

    /// Runs `step` on the event loop's thread for each session step, inside the window
    /// procedure that hears it, a step inside another included: the query's answer goes back to
    /// Windows, and the end returns only once `step` has.
    pub(crate) fn on_session(step: impl Fn(SessionStep) -> SessionReply + 'static) {
        SESSION.with(|s| *s.borrow_mut() = Some(Rc::new(step)));
    }

    /// Makes `window` hear the session for `on_session`.
    pub(crate) fn watch_session_end(window: &winit::window::Window) {
        let Some(hwnd) = crate::platform::win32::hwnd(window) else { return };
        // SAFETY: the subclass carries no data and is removed at WM_NCDESTROY.
        unsafe { SetWindowSubclass(hwnd, Some(subclass), SUBCLASS, 0) };
    }

    // winit's hidden event window carries every `EventLoopProxy` wake, quits included, and lets
    // `DefWindowProc` destroy it on `WM_CLOSE`.
    pub(crate) fn keep_event_window() {
        // SAFETY: `event_window` has the callback's signature and touches only the window it is handed.
        unsafe { EnumThreadWindows(GetCurrentThreadId(), Some(event_window), 0) };
    }

    unsafe extern "system" fn event_window(hwnd: HWND, _: LPARAM) -> windows_sys::Win32::Foundation::BOOL {
        let mut class = [0u16; 32];
        // SAFETY: the length passed is the buffer's own, so GetClassNameW writes inside it.
        let len = unsafe { GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32) };
        if class[..len.max(0) as usize].iter().copied().eq(EVENT_WINDOW_CLASS.encode_utf16()) {
            // SAFETY: the subclass keeps no data, and `event_window_closing` removes it at WM_NCDESTROY.
            unsafe { SetWindowSubclass(hwnd, Some(event_window_closing), SUBCLASS, 0) };
        }
        1
    }

    const EVENT_WINDOW_CLASS: &str = "Winit Thread Event Target";

    unsafe extern "system" fn event_window_closing(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM, _: usize, _: usize) -> LRESULT {
        match message {
            WM_CLOSE => {
                crate::window::quitting::request();
                return 0;
            }
            // SAFETY: `hwnd` carries this subclass, and nothing calls it after the window is destroyed.
            WM_NCDESTROY => unsafe { RemoveWindowSubclass(hwnd, Some(event_window_closing), SUBCLASS); },
            _ => {}
        }
        // SAFETY: the arguments are the message this subclass procedure was called with, for its own window.
        unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
    }

    /// Lets a session that waited on the app end without it: the quit it waited for was refused.
    pub(crate) fn unblock() {
        let hwnd = BLOCKED.with(|b| b.replace(std::ptr::null_mut()));
        // SAFETY: a window this thread named; a destroyed one fails harmlessly.
        if !hwnd.is_null() { unsafe { ShutdownBlockReasonDestroy(hwnd) }; }
    }

    fn step(step: SessionStep) -> SessionReply {
        let Some(f) = SESSION.with(|s| s.borrow().clone()) else { return SessionReply::Ends };
        f(step)
    }

    unsafe extern "system" fn subclass(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM, _: usize, _: usize) -> LRESULT {
        match message {
            WM_QUERYENDSESSION => return match step(SessionStep::Query) {
                SessionReply::Ends => 1,
                SessionReply::Waits => {
                    let reason: Vec<u16> = "Saving before it closes".encode_utf16().chain([0]).collect();
                    // SAFETY: a window of this thread and a NUL-terminated string that outlives the call.
                    if unsafe { ShutdownBlockReasonCreate(hwnd, reason.as_ptr()) } != 0 { BLOCKED.with(|b| b.set(hwnd)); }
                    0
                }
                SessionReply::Refuses => 0,
            },
            WM_ENDSESSION => {
                step(if wparam != 0 { SessionStep::Ends } else { SessionStep::Continues });
                if wparam == 0 { unblock(); }
            }
            // SAFETY: removes this subclass from the window it was set on.
            WM_NCDESTROY => unsafe { RemoveWindowSubclass(hwnd, Some(subclass), SUBCLASS); },
            _ => {}
        }
        // SAFETY: forwards to the next window procedure.
        unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
    }
}
