//! Text another program types as Unicode input (`SendInput` with `KEYEVENTF_UNICODE`, as
//! password managers' auto-type and AutoHotkey do). Windows sends each UTF-16 unit as a
//! `VK_PACKET` key whose character only the `WM_CHAR` that `TranslateMessage` posts carries.
//! winit 0.30 reports a BMP character with an unidentified key and drops each half of a
//! surrogate pair, so the event loop's message hook takes `VK_PACKET` keys before winit and hands
//! their text, pairs joined, to the window as typed text.

use std::ffi::c_void;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_PACKET;
use windows_sys::Win32::UI::WindowsAndMessaging::{MSG, PM_REMOVE, PeekMessageW, TranslateMessage, WM_CHAR, WM_KEYDOWN, WM_KEYUP};

#[derive(Default)]
pub(crate) struct Packets {
    high: Option<u16>,
}

impl Packets {
    pub(crate) fn unit(&mut self, unit: u16) -> String {
        let units: Vec<u16> = match (self.high.take(), unit) {
            (Some(high), 0xdc00..=0xdfff) => vec![high, unit],
            (high, 0xd800..=0xdbff) => {
                self.high = Some(unit);
                high.into_iter().collect()
            }
            (high, _) => high.into_iter().chain([unit]).collect(),
        };
        char::decode_utf16(units).map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER)).collect()
    }
}

pub(crate) fn hook(mut typed: impl FnMut(HWND, String) + 'static) -> impl FnMut(*const c_void) -> bool + 'static {
    let mut packets = Packets::default();
    move |msg| {
        // SAFETY: winit hands the hook the `MSG` it just took from the queue.
        let msg = unsafe { &*msg.cast::<MSG>() };
        if msg.wParam != VK_PACKET as usize { return false; }
        match msg.message {
            WM_KEYDOWN => {
                // SAFETY: an all-zero `MSG` is valid.
                let mut posted: MSG = unsafe { std::mem::zeroed() };
                // SAFETY: `msg` is a key message of this thread; its `WM_CHAR` is posted to the
                // same window, and `posted` is a valid `MSG` to write into.
                let translated = unsafe {
                    TranslateMessage(msg);
                    PeekMessageW(&mut posted, msg.hwnd, WM_CHAR, WM_CHAR, PM_REMOVE) != 0
                };
                if translated {
                    let text = packets.unit(posted.wParam as u16);
                    if !text.is_empty() { typed(msg.hwnd, text); }
                }
                true
            }
            WM_KEYUP => true,
            _ => false,
        }
    }
}

#[cfg(test)]
#[path = "packets_tests.rs"]
mod tests;
