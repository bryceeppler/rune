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
    use std::cell::Cell;

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::Graphics::Dwm::{DWMWA_CAPTION_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, GetWindowSubclass, RemoveWindowSubclass, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{WM_NCDESTROY, WM_SETTINGCHANGE, WM_THEMECHANGED};

    use super::TitleBar;

    const SUBCLASS: usize = 0x5449_544c;

    pub(crate) fn show(window: &winit::window::Window, bar: TitleBar) {
        let Some(hwnd) = crate::platform::win32::hwnd(window) else { return };
        let mut kept = 0usize;
        // SAFETY: the subclass's data is the `Cell` boxed here, freed at WM_NCDESTROY.
        unsafe {
            if GetWindowSubclass(hwnd, Some(subclass), SUBCLASS, &mut kept) != 0 {
                (*(kept as *const Cell<TitleBar>)).set(bar);
            } else {
                let kept = Box::into_raw(Box::new(Cell::new(bar)));
                if SetWindowSubclass(hwnd, Some(subclass), SUBCLASS, kept as usize) == 0 { drop(Box::from_raw(kept)); }
            }
        }
        apply(hwnd, bar);
    }

    fn apply(hwnd: HWND, bar: TitleBar) {
        let dark = i32::from(bar.dark);
        let [r, g, b] = bar.caption;
        let caption = u32::from(r) | u32::from(g) << 8 | u32::from(b) << 16;
        // SAFETY: each attribute reads four bytes from a local that outlives the call.
        unsafe {
            DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE as u32, (&raw const dark).cast(), 4);
            DwmSetWindowAttribute(hwnd, DWMWA_CAPTION_COLOR as u32, (&raw const caption).cast(), 4);
        }
    }

    unsafe extern "system" fn subclass(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM, _: usize, kept: usize) -> LRESULT {
        if message == WM_NCDESTROY {
            // SAFETY: removes this subclass and frees the data `show` boxed for it.
            unsafe {
                RemoveWindowSubclass(hwnd, Some(subclass), SUBCLASS);
                drop(Box::from_raw(kept as *mut Cell<TitleBar>));
            }
        }
        // SAFETY: forwards to the next window procedure.
        let result = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
        if matches!(message, WM_SETTINGCHANGE | WM_THEMECHANGED) {
            // SAFETY: the data `show` boxed, alive until WM_NCDESTROY.
            apply(hwnd, unsafe { (*(kept as *const Cell<TitleBar>)).get() });
        }
        result
    }
}

#[cfg(test)]
#[path = "title_bar_tests.rs"]
mod tests;
