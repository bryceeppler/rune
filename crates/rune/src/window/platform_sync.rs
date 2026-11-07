use winit::dpi::{LogicalPosition, LogicalSize};

use crate::runtime::Runtime;
use super::Win;

/// What a built frame hands the window besides its pixels: secure input, the IME, the cursor,
/// help tags and the clipboard.
pub(super) fn sync_platform<S: 'static>(l: &mut Win, rt: &mut Runtime<S>, secure_input: &mut crate::input::secure_input::SecureInput,
    ime: &mut bool, clipboard: &crate::clipboard::Client) {
    let focused = l.window.has_focus();
    secure_input.set(rt.has_secure_focus() && focused);
    let ime_allowed = rt.ime_area.is_some() && !rt.has_secure_focus() && focused;
    if std::mem::take(&mut rt.ime_reset) || ime_allowed != *ime {
        reset_ime(l, rt, ime_allowed);
        *ime = ime_allowed;
    }
    l.apply_cursor(rt, focused);
    #[cfg(target_os = "macos")]
    l.native.help.sync(&l.window, rt.help_regions());
    clipboard.flush(rt);
    if let Some(a) = rt.ime_area { set_ime_area(l, a); }
}

/// Turns the window's input method off, discarding the composition the runtime finished, then
/// back on if `allowed`. The runtime drops what the platform still sends for the session this
/// ends until the platform ends it: at once with Rune's own Wayland input method, at the
/// platform's next `Disabled` elsewhere.
pub(super) fn reset_ime<S: 'static>(l: &mut Win, rt: &mut Runtime<S>, allowed: bool) {
    crate::platform::ime::discard_composition(&l.window);
    #[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
    if l.native.ime.reset(&l.window, allowed) {
        rt.ime_reset_sent();
        rt.ime_session_over();
        return;
    }
    l.window.set_ime_allowed(false);
    l.window.set_ime_allowed(allowed);
    rt.ime_reset_sent();
    #[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
    l.native.ime.wake(&l.window);
}

/// Places the input method's candidates at the caret, `a` in window points.
pub(super) fn set_ime_area(l: &mut Win, a: [f32; 4]) {
    #[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
    if l.native.ime.set_area(&l.window, a) { return; }
    l.window.set_ime_cursor_area(LogicalPosition::new(a[0] as f64, a[1] as f64), LogicalSize::new(a[2] as f64, a[3] as f64));
}

/// Each turn of the event loop: what Rune's own Wayland input method sent, and on macOS and
/// Windows, whose input methods answer a reset while it runs, the end of the old session.
pub(super) fn poll_ime<S: 'static>(l: &mut Win, rt: &mut Runtime<S>) {
    #[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
    let heard = l.native.ime.poll().into_iter().map(|ev| rt.ime(ev)).count() > 0;
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    let heard = rt.ime_session_over();
    #[cfg(not(any(target_os = "macos", target_os = "windows", all(unix, not(target_os = "ios"), not(target_os = "android")))))]
    let heard = false;
    if heard { l.window.request_redraw(); }
}

impl Win {
    /// The runtime's cursor while the pointer is in the window, the arrow once it leaves.
    /// `Hidden` needs focus, so it can't outlive the window being active.
    pub(super) fn apply_cursor<S>(&mut self, rt: &Runtime<S>, focused: bool) {
        let want = match rt.cursor() { crate::el::Cursor::Hidden if !focused => crate::el::Cursor::Default, c => c };
        self.cursor.apply(&self.window, want, rt.pointer().is_some());
    }
}
