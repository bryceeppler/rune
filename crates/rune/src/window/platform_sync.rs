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
        reset_ime(&l.window, ime_allowed);
        *ime = ime_allowed;
    }
    l.apply_cursor(rt, focused);
    #[cfg(target_os = "macos")]
    l.native.help.sync(&l.window, rt.help_regions());
    clipboard.flush(rt);
    if let Some(a) = rt.ime_area {
        l.window.set_ime_cursor_area(
            LogicalPosition::new(a[0] as f64, a[1] as f64),
            LogicalSize::new(a[2] as f64, a[3] as f64),
        );
    }
}

/// Turns the window's input method off, ending its composition first, then back on if `allowed`.
/// The composition's commit, if any, follows as events for the field that composed it.
pub(super) fn reset_ime(window: &winit::window::Window, allowed: bool) {
    crate::platform::ime::end_composition(window);
    window.set_ime_allowed(false);
    window.set_ime_allowed(allowed);
}

impl Win {
    /// The runtime's cursor while the pointer is in the window, the arrow once it leaves.
    /// `Hidden` needs focus, so it can't outlive the window being active.
    pub(super) fn apply_cursor<S>(&mut self, rt: &Runtime<S>, focused: bool) {
        let want = match rt.cursor() { crate::el::Cursor::Hidden if !focused => crate::el::Cursor::Default, c => c };
        self.cursor.apply(&self.window, want, rt.pointer().is_some());
    }
}
