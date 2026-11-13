//! winit 0.30 drops text typed as Unicode input (`SendInput` with `KEYEVENTF_UNICODE`): each
//! UTF-16 unit is a `VK_PACKET` key whose character only the `WM_CHAR` from `TranslateMessage`
//! carries, and an emoji is two such keys, which one `SendInput` call puts in the queue together.
//! The event loop's message hook takes those keys before winit.

use std::ffi::c_void;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_PACKET;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MSG, PM_NOREMOVE, PM_REMOVE, PeekMessageW, TranslateMessage, WM_CHAR, WM_KEYDOWN, WM_KEYUP,
};

const HIGH: std::ops::RangeInclusive<u16> = 0xd800..=0xdbff;
const LOW: std::ops::RangeInclusive<u16> = 0xdc00..=0xdfff;

pub(crate) fn hook(mut typed: impl FnMut(HWND, String) + 'static) -> impl FnMut(*const c_void) -> bool + 'static {
    move |msg| {
        // SAFETY: winit hands the hook a pointer to the `MSG` it just took from the queue, valid for this call.
        let msg = unsafe { &*msg.cast::<MSG>() };
        if msg.wParam != VK_PACKET as usize { return false; }
        match msg.message {
            WM_KEYDOWN => {
                let text = text_of(msg);
                if !text.is_empty() { typed(msg.hwnd, text); }
                true
            }
            WM_KEYUP => true,
            _ => false,
        }
    }
}

fn text_of(packet: &MSG) -> String {
    let mut text = String::new();
    let mut next = character(packet);
    while let Some((unit, repeat)) = next.take() {
        if HIGH.contains(&unit) {
            match following(packet.hwnd) {
                Some((low, _)) if LOW.contains(&low) => text.push_str(&String::from_utf16_lossy(&[unit, low])),
                other => {
                    text.push(char::REPLACEMENT_CHARACTER);
                    next = other;
                }
            }
        } else {
            let typed = char::from_u32(u32::from(unit)).unwrap_or(char::REPLACEMENT_CHARACTER);
            text.extend(std::iter::repeat_n(typed, repeat));
        }
    }
    text
}

fn character(packet: &MSG) -> Option<(u16, usize)> {
    // SAFETY: `MSG` is plain data, for which all zeros is a valid value.
    let mut posted: MSG = unsafe { std::mem::zeroed() };
    // SAFETY: `packet` is a key message taken on this thread, and `posted` is a valid `MSG` for PeekMessageW to fill.
    let translated = unsafe {
        TranslateMessage(packet);
        PeekMessageW(&mut posted, packet.hwnd, WM_CHAR, WM_CHAR, PM_REMOVE) != 0
    };
    translated.then(|| (posted.wParam as u16, (posted.lParam & 0xffff).max(1) as usize))
}

fn following(window: HWND) -> Option<(u16, usize)> {
    loop {
        // SAFETY: `MSG` is plain data, for which all zeros is a valid value.
        let mut next: MSG = unsafe { std::mem::zeroed() };
        // SAFETY: `next` is a valid `MSG` to fill, and PM_NOREMOVE leaves the queue as it is.
        if unsafe { PeekMessageW(&mut next, window, 0, 0, PM_NOREMOVE) } == 0 { return None; }
        if next.wParam != VK_PACKET as usize || !matches!(next.message, WM_KEYDOWN | WM_KEYUP) { return None; }
        // SAFETY: removes the message just peeked, the first for `window` of its kind, into a valid `MSG`.
        unsafe { PeekMessageW(&mut next, window, next.message, next.message, PM_REMOVE) };
        if next.message == WM_KEYDOWN { return character(&next); }
    }
}

#[cfg(test)]
#[path = "packets_tests.rs"]
mod tests;
