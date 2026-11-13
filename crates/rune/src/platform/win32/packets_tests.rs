use std::cell::RefCell;
use std::rc::Rc;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{VK_PACKET, VK_X};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, HWND_MESSAGE, MSG, PM_REMOVE, PeekMessageW, PostMessageW, WM_CHAR,
    WM_IME_COMPOSITION, WM_IME_ENDCOMPOSITION, WM_IME_STARTCOMPOSITION, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN,
};

use crate::runtime::{Cx, Env, Runtime};
use crate::{CloseKind, CloseRequest, Tx};

const HIGH: u16 = 0xd83d;
const LOW: u16 = 0xde00;

#[derive(Debug, PartialEq)]
enum Seen { Typed(usize, String), Passed(u32) }

enum Step { Packet(usize, u16), Repeated(usize, u16, isize), Key(usize, u16), Click(usize), Other(usize, u32) }

struct Window(HWND);

impl Window {
    fn new() -> Window {
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        // SAFETY: the class name is NUL-terminated and outlives the call, and `Drop` destroys the window.
        let hwnd = unsafe { CreateWindowExW(0, class.as_ptr(), std::ptr::null(), 0, 0, 0, 0, 0, HWND_MESSAGE, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null()) };
        assert!(!hwnd.is_null(), "a message-only window");
        Window(hwnd)
    }

    fn id(&self) -> usize { self.0 as usize }
}

impl Drop for Window {
    fn drop(&mut self) {
        // SAFETY: this thread created the window, and drop destroys it once.
        unsafe { DestroyWindow(self.0) };
    }
}

fn post(window: usize, message: u32, wparam: usize, lparam: isize) {
    // SAFETY: PostMessageW copies its arguments, and the window belongs to this thread.
    assert!(unsafe { PostMessageW(window as HWND, message, wparam, lparam) } != 0, "posted");
}

// Every step is queued before the loop starts, because the hook reads what is queued behind a packet.
fn run(steps: &[Step], typed: impl FnMut(HWND, String) + 'static, mut passed: impl FnMut(u32)) {
    for step in steps {
        match *step {
            Step::Packet(window, unit) => {
                post(window, WM_KEYDOWN, VK_PACKET as usize, 1);
                post(window, WM_CHAR, usize::from(unit), 1);
                post(window, WM_KEYUP, VK_PACKET as usize, 0xc000_0001_u32 as isize);
            }
            Step::Repeated(window, unit, repeat) => {
                post(window, WM_KEYDOWN, VK_PACKET as usize, repeat);
                post(window, WM_CHAR, usize::from(unit), repeat);
                post(window, WM_KEYUP, VK_PACKET as usize, 0xc000_0001_u32 as isize);
            }
            Step::Key(window, vk) => post(window, WM_KEYDOWN, usize::from(vk), 1),
            Step::Click(window) => post(window, WM_LBUTTONDOWN, 1, 0),
            Step::Other(window, message) => post(window, message, 0, 0),
        }
    }
    let mut hook = super::hook(typed);
    // SAFETY: `MSG` is plain data, for which all zeros is a valid value.
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    // SAFETY: `msg` is a valid `MSG` to fill with this thread's next queued message.
    while unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0 {
        if !hook((&raw const msg).cast()) && msg.message != WM_CHAR { passed(msg.message); }
    }
}

fn seen(steps: &[Step]) -> Vec<Seen> {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let (typed, passed) = (seen.clone(), seen.clone());
    run(steps, move |hwnd, text| typed.borrow_mut().push(Seen::Typed(hwnd as usize, text)), move |message| passed.borrow_mut().push(Seen::Passed(message)));
    seen.take()
}

fn typed(text: &str, window: usize) -> Seen { Seen::Typed(window, text.to_owned()) }

#[test]
fn each_unit_of_the_basic_plane_types_itself_as_its_packet_is_handled() {
    let window = Window::new();
    let w = window.id();
    assert_eq!(seen(&[Step::Packet(w, u16::from(b'a')), Step::Packet(w, 0xe9), Step::Packet(w, 0x20ac)]), [typed("a", w), typed("é", w), typed("€", w)]);
}

#[test]
fn a_surrogate_pair_queued_together_types_its_character() {
    let window = Window::new();
    let w = window.id();
    assert_eq!(seen(&[Step::Packet(w, HIGH), Step::Packet(w, LOW), Step::Packet(w, u16::from(b'x'))]), [typed("😀", w), typed("x", w)]);
}

#[test]
fn a_lone_high_half_at_the_end_of_the_queue_types_a_replacement_at_once() {
    let window = Window::new();
    let w = window.id();
    assert_eq!(seen(&[Step::Packet(w, u16::from(b'a')), Step::Packet(w, HIGH)]), [typed("a", w), typed("\u{fffd}", w)]);
}

#[test]
fn a_lone_low_half_types_a_replacement() {
    let window = Window::new();
    let w = window.id();
    assert_eq!(seen(&[Step::Packet(w, LOW), Step::Packet(w, HIGH), Step::Packet(w, HIGH), Step::Packet(w, LOW)]), [typed("\u{fffd}", w), typed("\u{fffd}😀", w)]);
}

#[test]
fn a_half_pair_split_across_two_windows_types_a_replacement_in_each() {
    let (one, two) = (Window::new(), Window::new());
    let (one, two) = (one.id(), two.id());
    assert_eq!(seen(&[Step::Packet(one, HIGH), Step::Packet(two, LOW)]), [typed("\u{fffd}", one), typed("\u{fffd}", two)]);
}

#[test]
fn a_lone_half_before_an_ordinary_key_types_its_replacement_before_that_key() {
    let window = Window::new();
    let w = window.id();
    assert_eq!(
        seen(&[Step::Packet(w, HIGH), Step::Key(w, VK_X), Step::Packet(w, u16::from(b'a'))]),
        [typed("\u{fffd}", w), Seen::Passed(WM_KEYDOWN), typed("a", w)],
    );
}

#[test]
fn a_lone_half_before_a_click_types_its_replacement_before_the_click() {
    let window = Window::new();
    let w = window.id();
    assert_eq!(seen(&[Step::Packet(w, u16::from(b'a')), Step::Packet(w, HIGH), Step::Click(w)]), [typed("a", w), typed("\u{fffd}", w), Seen::Passed(WM_LBUTTONDOWN)]);
}

#[test]
fn a_lone_half_before_input_method_messages_types_its_replacement_before_them() {
    let window = Window::new();
    let w = window.id();
    let ime = [WM_IME_STARTCOMPOSITION, WM_IME_COMPOSITION, WM_IME_ENDCOMPOSITION];
    assert_eq!(
        seen(&[Step::Packet(w, HIGH), Step::Other(w, ime[0]), Step::Other(w, ime[1]), Step::Other(w, ime[2]), Step::Packet(w, LOW)]),
        [typed("\u{fffd}", w), Seen::Passed(ime[0]), Seen::Passed(ime[1]), Seen::Passed(ime[2]), typed("\u{fffd}", w)],
    );
}

#[test]
fn a_packet_typed_with_a_repeat_count_types_its_character_that_many_times() {
    let window = Window::new();
    let w = window.id();
    assert_eq!(seen(&[Step::Repeated(w, u16::from(b'a'), 3)]), [typed("aaa", w)]);
}

#[derive(Default)]
struct Note { text: String, saved: Option<String> }

fn save(note: &mut Note, request: CloseRequest, _: &mut Tx) {
    note.saved = Some(note.text.clone());
    request.proceed();
}

#[test]
fn a_quit_asked_right_after_a_packet_saves_the_packets_text() {
    let window = Window::new();
    let mut rt = Runtime::new(Note::default(), |_: &Note, _: &mut Cx<Note>| crate::el::el(), (100.0, 100.0), Env::default());
    rt.on_close_request(save);
    rt.on_key = Some(|note, key, _| { if let crate::Key::Text(text) = &key.key { note.text.push_str(text) } true });
    rt.frame(0.0);
    let rt = Rc::new(RefCell::new(rt));
    let typing = rt.clone();
    run(&[Step::Packet(window.id(), u16::from(b'a')), Step::Packet(window.id(), HIGH)], move |_, text| {
        typing.borrow_mut().key(crate::KeyEvent { key: crate::Key::Text(text), mods: crate::Mods::default() });
    }, |_| {});
    let mut rt = rt.borrow_mut();
    rt.request_close(CloseKind::Quit);
    rt.frame(1.0);
    assert_eq!(rt.state.saved.as_deref(), Some("a\u{fffd}"), "the quit's save holds every packet handled before it, a lone half ended");
    rt.withdraw_close(CloseKind::Quit);
}
