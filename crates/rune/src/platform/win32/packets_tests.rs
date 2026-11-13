use super::Typing;

fn typed(steps: &[(usize, u16)]) -> Vec<(usize, String)> {
    let mut typing = Typing::default();
    steps.iter().flat_map(|&(window, unit)| { typing.unit(window, unit); typing.take() }).collect()
}

fn text(window: usize, text: &str) -> (usize, String) { (window, text.to_owned()) }

const HIGH: u16 = 0xd83d;
const LOW: u16 = 0xde00;

#[test]
fn each_unit_of_the_basic_plane_types_itself() {
    assert_eq!(typed(&[(1, u16::from(b'a')), (1, 0xe9), (1, 0x20ac)]), [text(1, "a"), text(1, "é"), text(1, "€")]);
}

#[test]
fn a_surrogate_pair_types_its_character_once_both_halves_arrive() {
    assert_eq!(typed(&[(1, HIGH), (1, LOW), (1, u16::from(b'x'))]), [text(1, "😀"), text(1, "x")]);
}

#[test]
fn a_half_without_its_partner_types_a_replacement_character_in_its_own_window() {
    assert_eq!(typed(&[(1, HIGH), (1, u16::from(b'a'))]), [text(1, "\u{fffd}a")]);
    assert_eq!(typed(&[(1, LOW), (1, HIGH), (1, HIGH), (1, LOW)]), [text(1, "\u{fffd}"), text(1, "\u{fffd}"), text(1, "😀")]);
    assert_eq!(typed(&[(1, HIGH), (2, LOW)]), [text(1, "\u{fffd}"), text(2, "\u{fffd}")]);
}

#[test]
fn ending_a_window_types_only_its_own_waiting_half() {
    let mut typing = Typing::default();
    typing.unit(1, HIGH);
    typing.end_in(2);
    assert_eq!(typing.take(), []);
    typing.end_in(1);
    assert_eq!(typing.take(), [text(1, "\u{fffd}")]);
    typing.end_in(1);
    assert_eq!(typing.take(), []);
}

mod hook {
    use std::cell::RefCell;
    use std::rc::Rc;

    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{VK_PACKET, VK_X};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, HWND_MESSAGE, MSG, PM_REMOVE, PeekMessageW, PostMessageW, WM_CHAR,
        WM_IME_COMPOSITION, WM_IME_ENDCOMPOSITION, WM_IME_STARTCOMPOSITION, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN,
    };
    use winit::window::WindowId;

    use crate::runtime::{Cx, Env, Runtime};
    use crate::{CloseKind, CloseRequest, Tx};

    #[derive(Debug, PartialEq)]
    enum Seen { Typed(usize, String), Passed(u32) }

    enum Step { Packet(usize, u16, isize), Key(usize, u16), Click(usize), Other(usize, u32) }

    struct Window(HWND);

    impl Window {
        fn new() -> Window {
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            // SAFETY: the class name is NUL-terminated and outlives the call, and `Drop` destroys the window.
            let hwnd = unsafe { CreateWindowExW(0, class.as_ptr(), std::ptr::null(), 0, 0, 0, 0, 0, HWND_MESSAGE, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null()) };
            assert!(!hwnd.is_null(), "a message-only window");
            Window(hwnd)
        }
    }

    impl Drop for Window {
        fn drop(&mut self) {
            // SAFETY: this thread created the window, and drop destroys it once.
            unsafe { DestroyWindow(self.0) };
        }
    }

    fn message(hwnd: usize, message: u32, wparam: usize, lparam: isize) -> MSG {
        MSG { hwnd: hwnd as HWND, message, wParam: wparam, lParam: lparam, time: 0, pt: windows_sys::Win32::Foundation::POINT { x: 0, y: 0 } }
    }

    fn drain(hwnd: usize) {
        // SAFETY: `MSG` is plain data, for which all zeros is a valid value.
        let mut left: MSG = unsafe { std::mem::zeroed() };
        // SAFETY: `left` is a valid `MSG` to fill, and the window belongs to this thread.
        while unsafe { PeekMessageW(&mut left, hwnd as HWND, 0, 0, PM_REMOVE) } != 0 {}
    }

    fn feed(steps: &[Step]) -> Vec<Seen> {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let typed = seen.clone();
        let mut hook = super::super::hook(move |hwnd, text| typed.borrow_mut().push(Seen::Typed(hwnd as usize, text)));
        let mut send = |msg: MSG| if !hook((&raw const msg).cast()) { seen.borrow_mut().push(Seen::Passed(msg.message)) };
        for step in steps {
            match *step {
                Step::Packet(hwnd, unit, repeat) => {
                    // SAFETY: PostMessageW copies its arguments, and the window belongs to this thread.
                    unsafe { PostMessageW(hwnd as HWND, WM_CHAR, unit as usize, repeat) };
                    send(message(hwnd, WM_KEYDOWN, VK_PACKET as usize, 1));
                    send(message(hwnd, WM_KEYUP, VK_PACKET as usize, 0xc000_0001_u32 as isize));
                    drain(hwnd);
                }
                Step::Key(hwnd, vk) => send(message(hwnd, WM_KEYDOWN, vk as usize, 1)),
                Step::Click(hwnd) => send(message(hwnd, WM_LBUTTONDOWN, 1, 0)),
                Step::Other(hwnd, other) => send(message(hwnd, other, 0, 0)),
            }
        }
        seen.take()
    }

    use super::{HIGH, LOW};

    #[test]
    fn a_half_pair_split_across_two_windows_types_a_replacement_in_each() {
        let (one, two) = (Window::new(), Window::new());
        let (one, two) = (one.0 as usize, two.0 as usize);
        assert_eq!(
            feed(&[Step::Packet(one, HIGH, 1), Step::Packet(two, LOW, 1)]),
            [Seen::Typed(one, "\u{fffd}".into()), Seen::Typed(two, "\u{fffd}".into())],
        );
    }

    #[test]
    fn a_lone_half_before_an_ordinary_key_types_its_replacement_before_that_key() {
        let window = Window::new();
        let w = window.0 as usize;
        assert_eq!(
            feed(&[Step::Packet(w, HIGH, 1), Step::Key(w, VK_X), Step::Packet(w, u16::from(b'a'), 1)]),
            [Seen::Typed(w, "\u{fffd}".into()), Seen::Passed(WM_KEYDOWN), Seen::Typed(w, "a".into())],
        );
    }

    #[test]
    fn a_trailing_lone_half_types_its_replacement_at_the_next_click() {
        let window = Window::new();
        let w = window.0 as usize;
        assert_eq!(
            feed(&[Step::Packet(w, u16::from(b'a'), 1), Step::Packet(w, HIGH, 1), Step::Click(w)]),
            [Seen::Typed(w, "a".into()), Seen::Typed(w, "\u{fffd}".into()), Seen::Passed(WM_LBUTTONDOWN)],
        );
    }

    #[test]
    fn a_packet_typed_with_a_repeat_count_types_its_character_that_many_times() {
        let window = Window::new();
        let w = window.0 as usize;
        assert_eq!(feed(&[Step::Packet(w, u16::from(b'a'), 3)]), [Seen::Typed(w, "aaa".into())]);
    }

    #[test]
    fn a_lone_half_before_input_method_messages_types_its_replacement_before_them() {
        let window = Window::new();
        let w = window.0 as usize;
        let ime = [WM_IME_STARTCOMPOSITION, WM_IME_COMPOSITION, WM_IME_ENDCOMPOSITION];
        assert_eq!(
            feed(&[Step::Packet(w, HIGH, 1), Step::Other(w, ime[0]), Step::Other(w, ime[1]), Step::Other(w, ime[2]), Step::Packet(w, LOW, 1)]),
            [
                Seen::Typed(w, "\u{fffd}".into()),
                Seen::Passed(ime[0]),
                Seen::Passed(ime[1]),
                Seen::Passed(ime[2]),
                Seen::Typed(w, "\u{fffd}".into()),
            ],
        );
    }

    #[derive(Default)]
    struct Note { text: String, saved: Option<String> }

    fn save(note: &mut Note, request: CloseRequest, _: &mut Tx) {
        note.saved = Some(note.text.clone());
        request.proceed();
    }

    fn decode(hook: &mut impl FnMut(*const std::ffi::c_void) -> bool, w: usize, unit: u16) {
        // SAFETY: PostMessageW copies its arguments, and the window belongs to this thread.
        unsafe { PostMessageW(w as HWND, WM_CHAR, usize::from(unit), 1) };
        let packet = message(w, WM_KEYDOWN, VK_PACKET as usize, 1);
        assert!(hook((&raw const packet).cast()), "the hook takes the packet");
        drain(w);
    }

    fn saving() -> Runtime<Note> {
        let mut rt = Runtime::new(Note::default(), |_: &Note, _: &mut Cx<Note>| crate::el::el(), (100.0, 100.0), Env::default());
        rt.on_close_request(save);
        rt.on_key = Some(|note, key, _| { if let crate::Key::Text(text) = &key.key { note.text.push_str(text) } true });
        rt.frame(0.0);
        rt
    }

    #[test]
    fn a_window_asked_to_quit_right_after_a_packet_saves_the_packets_text() {
        let window = Window::new();
        let w = window.0 as usize;
        let mut hook = super::super::hook(super::super::arrive);
        decode(&mut hook, w, u16::from(b'a'));
        let mut rt = saving();
        super::super::ask_quit(&mut rt, WindowId::from(w as u64));
        rt.frame(1.0);
        assert_eq!(rt.state.saved.as_deref(), Some("a"), "the quit's save holds the text typed before it");
        rt.withdraw_close(CloseKind::Quit);
    }

    #[test]
    fn a_quit_asked_while_a_half_waits_saves_its_replacement() {
        let window = Window::new();
        let w = window.0 as usize;
        let mut hook = super::super::hook(super::super::arrive);
        decode(&mut hook, w, HIGH);
        let mut rt = saving();
        super::super::ask_quit(&mut rt, WindowId::from(w as u64));
        rt.frame(1.0);
        assert_eq!(rt.state.saved.as_deref(), Some("\u{fffd}"), "the quit's save holds the half, ended");
        rt.withdraw_close(CloseKind::Quit);
    }

    #[test]
    fn a_window_closed_while_a_half_waits_saves_its_replacement() {
        let window = Window::new();
        let w = window.0 as usize;
        let mut hook = super::super::hook(super::super::arrive);
        decode(&mut hook, w, HIGH);
        let mut rt = saving();
        super::super::close_window(&mut rt, WindowId::from(w as u64));
        rt.frame(1.0);
        assert_eq!(rt.state.saved.as_deref(), Some("\u{fffd}"), "the close's save holds the half, ended");
    }

    #[test]
    fn a_low_half_sent_to_a_closed_windows_handle_pairs_with_nothing() {
        let window = Window::new();
        let w = window.0 as usize;
        let id = WindowId::from(w as u64);
        let mut hook = super::super::hook(super::super::arrive);
        decode(&mut hook, w, HIGH);
        let mut rt = saving();
        super::super::close_window(&mut rt, id);
        rt.frame(1.0);
        super::super::retire(id);
        while super::super::arrived(id, false).is_some() {}
        decode(&mut hook, w, LOW);
        assert_eq!(super::super::arrived(id, false).map(|key| key.key.clone()), Some(crate::Key::Text("\u{fffd}".into())));
    }
}
