//! Windows draws a title bar from the system's light or dark setting, and winit sets that
//! setting again on every `WM_SETTINGCHANGE`.

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
    use std::cell::RefCell;

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::Graphics::Dwm::{DWMWA_CAPTION_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{WM_NCDESTROY, WM_SETTINGCHANGE, WM_THEMECHANGED};

    use super::TitleBar;

    const SUBCLASS: usize = 0x5449_544c;
    const DARK: usize = 1 << 24;

    thread_local! {
        static SHOWN: RefCell<Vec<(usize, usize)>> = const { RefCell::new(Vec::new()) };
    }

    // comctl32 without a version 6 manifest exports no `GetWindowSubclass` by name, so the
    // subclass keeps the bar in its own data, which setting the subclass again replaces.
    pub(crate) fn show(window: &winit::window::Window, bar: TitleBar) {
        let Some(hwnd) = crate::platform::win32::hwnd(window) else { return };
        let [r, g, b] = bar.caption;
        let kept = usize::from(r) | usize::from(g) << 8 | usize::from(b) << 16 | if bar.dark { DARK } else { 0 };
        let changed = SHOWN.with_borrow_mut(|shown| match shown.iter_mut().find(|(window, _)| *window == hwnd as usize) {
            Some((_, was)) => std::mem::replace(was, kept) != kept,
            None => { shown.push((hwnd as usize, kept)); true }
        });
        if !changed { return; }
        unsafe { SetWindowSubclass(hwnd, Some(subclass), SUBCLASS, kept) };
        apply(hwnd, kept);
    }

    fn apply(hwnd: HWND, kept: usize) {
        let dark = i32::from(kept & DARK != 0);
        let caption = (kept & 0xff_ffff) as u32;
        unsafe {
            DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE as u32, (&raw const dark).cast(), 4);
            DwmSetWindowAttribute(hwnd, DWMWA_CAPTION_COLOR as u32, (&raw const caption).cast(), 4);
        }
    }

    unsafe extern "system" fn subclass(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM, _: usize, kept: usize) -> LRESULT {
        if message == WM_NCDESTROY {
            unsafe { RemoveWindowSubclass(hwnd, Some(subclass), SUBCLASS) };
            SHOWN.with_borrow_mut(|shown| shown.retain(|(window, _)| *window != hwnd as usize));
        }
        let result = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
        if matches!(message, WM_SETTINGCHANGE | WM_THEMECHANGED) { apply(hwnd, kept); }
        result
    }
}

#[cfg(test)]
#[path = "title_bar_tests.rs"]
mod tests;
