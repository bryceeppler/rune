//! Windows draws a window's title bar from the system's light or dark setting, not the window's.
//! Rune paints it in the colour the window shows behind its content, dark where white text
//! reads better on that colour. winit sets the system's mode again on every `WM_SETTINGCHANGE`,
//! so the window's own mode goes back on after each one.

use crate::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) struct TitleBar {
    pub(crate) dark: bool,
    pub(crate) caption: [u8; 3],
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
impl TitleBar {
    pub(crate) fn of(fill: Color) -> TitleBar {
        let byte = |c: f32| (crate::color::srgb_encode(c.clamp(0.0, 1.0)) * 255.0).round() as u8;
        let luminance = fill.luminance();
        let (on_white, on_black) = (1.05 / (luminance + 0.05), (luminance + 0.05) / 0.05);
        TitleBar { dark: on_white > on_black, caption: [byte(fill.r), byte(fill.g), byte(fill.b)] }
    }
}

#[cfg(target_os = "windows")]
pub(crate) use win::show;

#[cfg(target_os = "windows")]
mod win {
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::Graphics::Dwm::{DWMWA_CAPTION_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{WM_NCDESTROY, WM_SETTINGCHANGE, WM_THEMECHANGED};

    use super::TitleBar;

    const SUBCLASS: usize = 0x5449_544c;
    const DARK: usize = 1 << 24;

    // comctl32 without a version 6 manifest exports no `GetWindowSubclass` by name, so the
    // subclass keeps the bar in its own data, which setting the subclass again replaces.
    pub(crate) fn show(window: &winit::window::Window, bar: TitleBar) {
        let Some(hwnd) = crate::platform::win32::hwnd(window) else { return };
        let [r, g, b] = bar.caption;
        let kept = usize::from(r) | usize::from(g) << 8 | usize::from(b) << 16 | if bar.dark { DARK } else { 0 };
        // SAFETY: the subclass's data is a plain value, and the subclass is removed at WM_NCDESTROY.
        unsafe { SetWindowSubclass(hwnd, Some(subclass), SUBCLASS, kept) };
        apply(hwnd, kept);
    }

    fn apply(hwnd: HWND, kept: usize) {
        let dark = i32::from(kept & DARK != 0);
        let caption = (kept & 0xff_ffff) as u32;
        // SAFETY: each attribute reads four bytes from a local that outlives the call.
        unsafe {
            DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE as u32, (&raw const dark).cast(), 4);
            DwmSetWindowAttribute(hwnd, DWMWA_CAPTION_COLOR as u32, (&raw const caption).cast(), 4);
        }
    }

    unsafe extern "system" fn subclass(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM, _: usize, kept: usize) -> LRESULT {
        // SAFETY: removes this subclass from the window it was set on.
        if message == WM_NCDESTROY { unsafe { RemoveWindowSubclass(hwnd, Some(subclass), SUBCLASS) }; }
        // SAFETY: forwards to the next window procedure.
        let result = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
        if matches!(message, WM_SETTINGCHANGE | WM_THEMECHANGED) { apply(hwnd, kept); }
        result
    }
}

#[cfg(test)]
#[path = "title_bar_tests.rs"]
mod tests;
