//! winit 0.30 drops text typed as Unicode input (`SendInput` with `KEYEVENTF_UNICODE`): each
//! UTF-16 unit is a `VK_PACKET` key whose character only the `WM_CHAR` from `TranslateMessage`
//! carries, and an emoji is two such keys. The event loop's message hook takes them before winit.

use std::cell::RefCell;
use std::ffi::c_void;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_PACKET;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MSG, PM_REMOVE, PeekMessageW, TranslateMessage, WM_CHAR, WM_KEYDOWN, WM_KEYFIRST, WM_KEYLAST, WM_KEYUP,
    WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_NCLBUTTONDOWN, WM_NCMBUTTONDOWN, WM_NCRBUTTONDOWN, WM_NCXBUTTONDOWN,
    WM_RBUTTONDOWN, WM_XBUTTONDOWN,
};
use winit::window::WindowId;

thread_local! {
    static TYPING: RefCell<Typing> = RefCell::default();
    static ARRIVED: RefCell<Vec<(usize, String)>> = const { RefCell::new(Vec::new()) };
}

#[derive(Default)]
pub(crate) struct Typing {
    high: Option<(usize, u16)>,
    typed: Vec<(usize, String)>,
}

impl Typing {
    pub(crate) fn unit(&mut self, window: usize, unit: u16) {
        match (self.high.take(), unit) {
            (Some((pending, high)), 0xdc00..=0xdfff) if pending == window => {
                self.type_into(window, &String::from_utf16_lossy(&[high, unit]));
            }
            (pending, _) => {
                if let Some((pending, _)) = pending { self.type_into(pending, "\u{fffd}"); }
                match unit {
                    0xd800..=0xdbff => self.high = Some((window, unit)),
                    _ => self.type_into(window, &String::from_utf16_lossy(&[unit])),
                }
            }
        }
    }

    pub(crate) fn end(&mut self) {
        if let Some((window, _)) = self.high.take() { self.type_into(window, "\u{fffd}"); }
    }

    pub(crate) fn end_in(&mut self, window: usize) {
        if self.high.is_some_and(|(pending, _)| pending == window) { self.end(); }
    }

    pub(crate) fn take(&mut self) -> Vec<(usize, String)> { std::mem::take(&mut self.typed) }

    fn type_into(&mut self, window: usize, text: &str) {
        match self.typed.last_mut() {
            Some((last, typed)) if *last == window => typed.push_str(text),
            _ => self.typed.push((window, text.to_owned())),
        }
    }
}

fn ends_typing(message: u32) -> bool {
    matches!(message, WM_KEYFIRST..=WM_KEYLAST
        | WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN
        | WM_NCLBUTTONDOWN | WM_NCRBUTTONDOWN | WM_NCMBUTTONDOWN | WM_NCXBUTTONDOWN)
}

pub(crate) fn hook(mut typed: impl FnMut(HWND, String) + 'static) -> impl FnMut(*const c_void) -> bool + 'static {
    move |msg| {
        // SAFETY: winit hands the hook the `MSG` it just took from the queue.
        let msg = unsafe { &*msg.cast::<MSG>() };
        let packet = msg.wParam == VK_PACKET as usize && matches!(msg.message, WM_KEYDOWN | WM_KEYUP);
        if packet && msg.message == WM_KEYDOWN {
            // SAFETY: an all-zero `MSG` is valid.
            let mut posted: MSG = unsafe { std::mem::zeroed() };
            // SAFETY: `msg` is a key message of this thread, whose `WM_CHAR` goes to the same window.
            let translated = unsafe {
                TranslateMessage(msg);
                PeekMessageW(&mut posted, msg.hwnd, WM_CHAR, WM_CHAR, PM_REMOVE) != 0
            };
            if translated {
                let repeat = (posted.lParam & 0xffff).max(1);
                TYPING.with_borrow_mut(|typing| (0..repeat).for_each(|_| typing.unit(msg.hwnd as usize, posted.wParam as u16)));
            }
        } else if !packet && ends_typing(msg.message) {
            TYPING.with_borrow_mut(Typing::end);
        }
        for (window, text) in TYPING.with_borrow_mut(Typing::take) { typed(window as HWND, text); }
        packet
    }
}

pub(crate) fn arrive(window: HWND, text: String) {
    ARRIVED.with_borrow_mut(|arrived| arrived.push((window as usize, text)));
}

pub(crate) fn arrived(window: WindowId, ending: bool) -> Option<crate::KeyEvent> {
    let window = u64::from(window) as usize;
    if ending {
        TYPING.with_borrow_mut(|typing| typing.end_in(window));
        for (into, text) in TYPING.with_borrow_mut(Typing::take) { arrive(into as HWND, text); }
    }
    let text: String = ARRIVED.with_borrow_mut(|arrived| {
        let (mine, others) = std::mem::take(arrived).into_iter().partition::<Vec<_>, _>(|(into, _)| *into == window);
        *arrived = others;
        mine.into_iter().map(|(_, text)| text).collect()
    });
    (!text.is_empty()).then(|| crate::KeyEvent { key: crate::Key::Text(text), mods: crate::Mods::default() })
}

#[cfg(test)]
#[path = "packets_tests.rs"]
mod tests;
