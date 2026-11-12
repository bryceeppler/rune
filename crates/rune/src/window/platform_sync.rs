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
    #[cfg(target_os = "windows")]
    if let Some(fill) = rt.window_fill() { crate::platform::title_bar::show(&l.window, crate::platform::title_bar::TitleBar::of(fill)); }
    #[cfg(target_os = "macos")]
    crate::platform::macos::text_input::publish(&l.window, rt);
}

/// Turns the window's input method off, discarding the composition the runtime finished, then
/// back on if `allowed`. The runtime hears the platform's answer as the ended session's.
pub(super) fn reset_ime<S: 'static>(l: &mut Win, rt: &mut Runtime<S>, allowed: bool) {
    let answers = crate::platform::ime::discard_composition(&l.window);
    #[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
    if l.native.ime.reset(&l.window, allowed) {
        // Its serials tell the ended session's answers apart.
        rt.ime_reset_sent(false);
        return;
    }
    l.window.set_ime_allowed(false);
    l.window.set_ime_allowed(allowed);
    rt.ime_reset_sent(answers);
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
/// Windows, whose input methods answer a reset while it runs, the loop going idle.
pub(super) fn poll_ime<S: 'static>(l: &mut Win, rt: &mut Runtime<S>) {
    #[cfg(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android")))]
    if l.native.ime.poll().into_iter().map(|(late, ev)| rt.ime_from_platform(ev, late)).count() > 0 {
        l.window.request_redraw();
    }
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    rt.ime_platform_idle();
    let _ = (l, rt);
}

impl Win {
    /// The runtime's cursor while the pointer is in the window, the arrow once it leaves.
    /// `Hidden` needs focus, so it can't outlive the window being active.
    pub(super) fn apply_cursor<S>(&mut self, rt: &Runtime<S>, focused: bool) {
        let want = match rt.cursor() { crate::el::Cursor::Hidden if !focused => crate::el::Cursor::Default, c => c };
        self.cursor.apply(&self.window, want, rt.pointer().is_some());
    }
}
