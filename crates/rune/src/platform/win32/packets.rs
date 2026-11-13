//! winit 0.30 drops text typed as Unicode input (`SendInput` with `KEYEVENTF_UNICODE`): each
//! UTF-16 unit is a `VK_PACKET` key, and an emoji is two such keys, which one `SendInput` call puts
//! in the queue together. The event loop's message hook takes those keys before winit.
//! `ToUnicode` reads the unit of the packet key this thread last took or peeked from its queue.

use std::ffi::c_void;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{ToUnicode, VK_PACKET};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MSG, PM_NOREMOVE, PM_NOYIELD, PM_QS_INPUT, PM_QS_POSTMESSAGE, PM_REMOVE, PeekMessageW, WM_KEYDOWN, WM_KEYUP,
};

const HIGH: std::ops::RangeInclusive<u16> = 0xd800..=0xdbff;
const LOW: std::ops::RangeInclusive<u16> = 0xdc00..=0xdfff;

pub(crate) fn hook(typed: impl FnMut(HWND, String) + 'static) -> impl FnMut(*const c_void) -> bool + 'static {
    reading(typed, injected)
}

fn reading(
    mut typed: impl FnMut(HWND, String) + 'static,
    unit: impl Fn(&MSG) -> Option<u16> + 'static,
) -> impl FnMut(*const c_void) -> bool + 'static {
    move |msg| {
        // SAFETY: winit hands the hook a pointer to the `MSG` it just took from the queue, valid for this call.
        let msg = unsafe { &*msg.cast::<MSG>() };
        if msg.wParam != VK_PACKET as usize { return false; }
        match msg.message {
            WM_KEYDOWN => {
                let Some(first) = unit(msg) else { return true };
                let text = if HIGH.contains(&first) {
                    let ended = replacements(repeat(msg) - 1);
                    match low_after(msg.hwnd, &unit) {
                        Some((low, also)) => ended + &String::from_utf16_lossy(&[first, low]) + &replacements(also - 1),
                        None => ended + &replacements(1),
                    }
                } else {
                    let typed = char::from_u32(u32::from(first)).unwrap_or(char::REPLACEMENT_CHARACTER);
                    std::iter::repeat_n(typed, repeat(msg)).collect()
                };
                typed(msg.hwnd, text);
                true
            }
            WM_KEYUP => true,
            _ => false,
        }
    }
}

// PeekMessage runs pending sent messages before it returns, whatever PM_QS flags it has, unless PM_NOYIELD joins
// PM_QS_INPUT | PM_QS_POSTMESSAGE; then, while one is pending, it returns no message.
const QUEUED: u32 = PM_QS_INPUT | PM_QS_POSTMESSAGE | PM_NOYIELD;

const KEEP_KEYBOARD_STATE: u32 = 1 << 2;

fn injected(packet: &MSG) -> Option<u16> {
    let keys = [0u8; 256];
    let mut unit = [0u16; 2];
    // SAFETY: the key state and the buffer are locals that outlive the call, and the length passed is the buffer's own.
    let written = unsafe {
        ToUnicode(VK_PACKET.into(), ((packet.lParam >> 16) & 0xff) as u32, keys.as_ptr(), unit.as_mut_ptr(), unit.len() as i32, KEEP_KEYBOARD_STATE)
    };
    (written == 1 && unit[0] != 0).then_some(unit[0])
}

fn repeat(key: &MSG) -> usize { (key.lParam & 0xffff).max(1) as usize }

fn replacements(count: usize) -> String { std::iter::repeat_n(char::REPLACEMENT_CHARACTER, count).collect() }

fn low_after(window: HWND, unit: impl Fn(&MSG) -> Option<u16>) -> Option<(u16, usize)> {
    let mut next = queued()?;
    if is_packet(&next, window, WM_KEYUP) {
        take(window, WM_KEYUP);
        next = queued()?;
    }
    if !is_packet(&next, window, WM_KEYDOWN) { return None; }
    let low = unit(&next).filter(|unit| LOW.contains(unit))?;
    take(window, WM_KEYDOWN);
    Some((low, repeat(&next)))
}

fn is_packet(msg: &MSG, window: HWND, message: u32) -> bool {
    msg.hwnd == window && msg.message == message && msg.wParam == VK_PACKET as usize
}

fn queued() -> Option<MSG> {
    // SAFETY: `MSG` is plain data, for which all zeros is a valid value.
    let mut next: MSG = unsafe { std::mem::zeroed() };
    // SAFETY: `next` is a valid `MSG` to fill, for the thread's next queued message.
    (unsafe { PeekMessageW(&mut next, std::ptr::null_mut(), 0, 0, PM_NOREMOVE | QUEUED) } != 0).then_some(next)
}

fn take(window: HWND, message: u32) {
    // SAFETY: `MSG` is plain data, for which all zeros is a valid value.
    let mut taken: MSG = unsafe { std::mem::zeroed() };
    // SAFETY: `taken` is a valid `MSG` to fill, and removes the first queued `message` for `window`, the one just read.
    unsafe { PeekMessageW(&mut taken, window, message, message, PM_REMOVE | QUEUED) };
}

#[cfg(test)]
#[path = "packets_tests.rs"]
mod tests;
