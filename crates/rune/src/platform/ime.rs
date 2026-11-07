//! The platform's input method around Rune's runtime. Once the runtime commits a composition
//! it finished, the shell discards the platform's: winit's `set_ime_allowed(false)` leaves it
//! open on macOS and Windows, where the input method's next keystroke would extend it. On X11
//! the toggle destroys the input context, which ends it, and on Wayland Rune runs the input
//! method itself, so that a reset starts a session the compositor's old answers can't reach.

#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
mod wayland;
#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
mod x11;

/// A Linux window's input method, made on first use: Rune's own on Wayland, winit's on X11
/// with a wake that has winit apply each toggle at once.
#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
#[derive(Default)]
pub(crate) struct Linux {
    made: bool,
    text_input: Option<wayland::TextInput>,
    wake: Option<x11::Wake>,
}

#[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
impl Linux {
    fn make(&mut self, window: &winit::window::Window) {
        if std::mem::replace(&mut self.made, true) { return; }
        self.text_input = wayland::TextInput::new(window);
        if self.text_input.is_none() { self.wake = x11::Wake::new(window); }
    }

    /// Resets Rune's own Wayland input method, if the window has one: the compositor's answers
    /// for the session it ends never reach the runtime.
    pub(crate) fn reset(&mut self, window: &winit::window::Window, allowed: bool) -> bool {
        self.make(window);
        self.text_input.as_mut().map(|t| t.reset(allowed)).is_some()
    }

    /// Has winit apply the toggles just sent before it handles the user's next key (X11).
    pub(crate) fn wake(&mut self, window: &winit::window::Window) {
        self.make(window);
        if let Some(wake) = &self.wake { wake.send(); }
    }

    /// Places Rune's own Wayland input method at the caret, if the window has one.
    pub(crate) fn set_area(&mut self, window: &winit::window::Window, area: [f32; 4]) -> bool {
        self.make(window);
        self.text_input.as_mut().map(|t| t.set_area(area)).is_some()
    }

    /// What Rune's own Wayland input method sent since the last call.
    pub(crate) fn poll(&mut self) -> Vec<crate::Ime> {
        self.text_input.as_mut().map_or_else(Vec::new, |t| t.poll())
    }
}

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
