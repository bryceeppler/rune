//! Discards the platform input method's composition once Rune has committed the text it
//! showed. winit's `set_ime_allowed(false)` leaves it open on macOS and Windows, where the input
//! method's next keystroke would extend it. On X11 the toggle destroys the input context and on
//! Wayland it deactivates the input method, either of which ends it.

/// The input method drops the composition; one that commits it anyway sends a commit the
/// runtime drops as the finished composition's late answer.
pub(crate) fn discard_composition(window: &winit::window::Window) {
    #[cfg(target_os = "macos")]
    if let Some(view) = super::macos::content_view(window) {
        // SAFETY: main thread; winit's content view implements NSTextInputClient.
        unsafe { commit_composition(&view) }
    }
    #[cfg(target_os = "windows")]
    if let Some(hwnd) = super::win32::hwnd(window) {
        use windows_sys::Win32::UI::Input::Ime::{CPS_CANCEL, ImmGetContext, ImmNotifyIME, ImmReleaseContext, NI_COMPOSITIONSTR};
        // SAFETY: the window's own thread. IMM32 bridges TSF input methods too.
        unsafe {
            let context = ImmGetContext(hwnd);
            if context.is_null() { return; }
            ImmNotifyIME(context, NI_COMPOSITIONSTR, CPS_CANCEL, 0);
            ImmReleaseContext(hwnd, context);
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = window;
}

/// An IME composition (or a dead key's accent) in the view ends as in an NSTextView: the input
/// method commits it with `insertText:`, which winit reports as `Ime::Commit`; one that doesn't
/// is dropped.
///
/// # Safety
/// Main thread, with `view` a live WinitView.
#[cfg(target_os = "macos")]
pub(crate) unsafe fn commit_composition(view: &objc2_app_kit::NSView) {
    use objc2::{msg_send, runtime::AnyObject};
    // SAFETY: the caller's contract; WinitView implements NSTextInputClient.
    unsafe {
        let marked: bool = msg_send![view, hasMarkedText];
        if !marked { return; }
        let context: *mut AnyObject = msg_send![view, inputContext];
        if let Some(context) = context.as_ref() { let _: () = msg_send![context, discardMarkedText]; }
        // Back to winit's ground state whether or not it committed: right after a commit winit
        // treats the next key as part of it and drops it.
        let _: () = msg_send![view, unmarkText];
    }
}
