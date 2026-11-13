//! Text another program types as Unicode input, as password managers' auto-type and AutoHotkey
//! do: `SendInput` with `KEYEVENTF_UNICODE` sends each UTF-16 unit as a `VK_PACKET` key, an
//! emoji as two. It lands in the focused field like typed text, in order with ordinary keys;
//! half an emoji types U+FFFD before the next ordinary key and before the window closes; and a
//! quit through winit's hidden event window saves what was typed before it. A pointer move sent
//! after the keys shows when the window has taken them. Types into the foreground window and
//! moves the pointer over it, so it only runs with RUNE_GUI_TESTS=1 in a desktop session nobody
//! is using.

#[cfg(not(target_os = "windows"))]
fn main() {}

#[cfg(target_os = "windows")]
fn main() { win::main() }

#[cfg(target_os = "windows")]
mod win {
    use rune::*;
    use std::sync::{Mutex, OnceLock, mpsc};
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
    use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOUSEEVENTF_ABSOLUTE,
        MOUSEEVENTF_MOVE, MOUSEEVENTF_VIRTUALDESK, MOUSEINPUT, SendInput, VK_SPACE,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetClientRect, GetForegroundWindow, GetSystemMetrics, GetWindowThreadProcessId,
        PostMessageW, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, WM_CLOSE,
    };

    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Side { Left, Right }

    struct State { field: TextEdit, pointer: Option<Side> }

    static WINDOW: OnceLock<WindowHandle> = OnceLock::new();
    static CLOSING: Mutex<Option<String>> = Mutex::new(None);
    static QUITTING: Mutex<Option<String>> = Mutex::new(None);

    fn on_close_request(s: &mut State, request: CloseRequest, _: &mut Tx) {
        match request.kind() {
            CloseKind::Window => { *CLOSING.lock().unwrap() = Some(s.field.text.clone()); request.cancel() }
            CloseKind::Quit => { *QUITTING.lock().unwrap() = Some(s.field.text.clone()); request.proceed() }
        }
    }

    const HIGH: &[u16] = &[0xd83d];
    const TYPED: &str = "a 😀é\u{fffd} b";

    fn half(side: Side) -> El {
        col().h_full().grow(1.0).on_hover_change(move |s: &mut State, inside, _| if inside { s.pointer = Some(side) })
    }

    fn view(s: &State, _: &mut Cx<State>) -> El {
        row().w_full().h_full()
            .child(half(Side::Left).padding(12.0)
                .child(text_input(&s.field, rune::tok::type_::BODY).key("field").single_line().w(240.0).on_edit(|s: &mut State| &mut s.field)))
            .child(half(Side::Right))
    }

    fn key(vk: u16, unit: u16, flags: u32) -> INPUT {
        INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: unit, dwFlags: flags, time: 0, dwExtraInfo: 0 } } }
    }

    fn unicode(text: &str) -> Vec<INPUT> {
        units(&text.encode_utf16().collect::<Vec<_>>())
    }

    fn units(units: &[u16]) -> Vec<INPUT> {
        units.iter().copied().flat_map(|unit| [key(0, unit, KEYEVENTF_UNICODE), key(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP)]).collect()
    }

    fn point_at(hwnd: HWND, side: Side) -> INPUT {
        let mut client = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        // SAFETY: `client` is a valid RECT to fill.
        unsafe { GetClientRect(hwnd, &mut client) };
        let mut at = POINT { x: client.right * if side == Side::Left { 1 } else { 3 } / 4, y: client.bottom * 3 / 4 };
        // SAFETY: `at` is a valid POINT to convert in place, and the metrics take no pointers.
        let (vx, vy, vw, vh) = unsafe {
            ClientToScreen(hwnd, &mut at);
            (GetSystemMetrics(SM_XVIRTUALSCREEN), GetSystemMetrics(SM_YVIRTUALSCREEN), GetSystemMetrics(SM_CXVIRTUALSCREEN), GetSystemMetrics(SM_CYVIRTUALSCREEN))
        };
        let (dx, dy) = ((at.x - vx) * 65535 / (vw - 1), (at.y - vy) * 65535 / (vh - 1));
        let flags = MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK;
        INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi: MOUSEINPUT { dx, dy, mouseData: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } } }
    }

    unsafe extern "system" fn collect(hwnd: HWND, found: LPARAM) -> i32 {
        let mut pid = 0;
        let mut class = [0u16; 64];
        // SAFETY: `found` is the `Vec` that `event_window` lends for the enumeration, and the length passed bounds GetClassNameW's write.
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut pid);
            let len = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32);
            if pid == GetCurrentProcessId() && String::from_utf16_lossy(&class[..len.max(0) as usize]) == "Winit Thread Event Target" {
                (*(found as *mut Vec<usize>)).push(hwnd as usize);
            }
        }
        1
    }

    fn event_window() -> HWND {
        let mut found = Vec::<usize>::new();
        // SAFETY: `collect` has the callback's signature and only pushes into `found`, which outlives the call.
        unsafe { EnumWindows(Some(collect), &mut found as *mut Vec<usize> as LPARAM) };
        assert_eq!(found.len(), 1, "winit's one event window");
        found[0] as HWND
    }

    fn wait<T>(what: &str, mut f: impl FnMut() -> Option<T>) -> T {
        let start = Instant::now();
        loop {
            if let Some(v) = f() { return v; }
            assert!(start.elapsed() < Duration::from_secs(10), "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    pub fn main() {
        if std::env::var_os("RUNE_GUI_TESTS").is_none() {
            println!("unicode_input_windows: skipped (set RUNE_GUI_TESTS=1 in a desktop session)");
            return;
        }
        std::thread::spawn(|| { std::thread::sleep(Duration::from_secs(60)); eprintln!("timeout"); std::process::exit(2) });
        let spec = AppSpec::new("Unicode input", (320.0, 120.0), || { let mut field = TextEdit::default(); field.focused = true; State { field, pointer: None } }, view);
        let (ready_tx, ready_rx) = mpsc::channel();
        let window = &WINDOW;
        let app = App::new(spec).args(Vec::new()).keep_running().on_close_request(on_close_request).on_ready(move |w| { let _ = window.set(w.clone()); let _ = ready_tx.send(()); });
        let handle = app.handle();
        std::thread::spawn(move || {
            let run = std::panic::catch_unwind(|| {
                ready_rx.recv_timeout(Duration::from_secs(10)).expect("window ready");
                let (tx, rx) = mpsc::channel();
                handle.update(move |_, _| {
                    let _ = tx.send(window.get().unwrap().with_raw_handle(|raw| match raw.as_raw() {
                        rune::raw_window_handle::RawWindowHandle::Win32(h) => h.hwnd.get(),
                        _ => panic!("not Win32"),
                    }).expect("window handle"));
                });
                let hwnd = rx.recv_timeout(Duration::from_secs(5)).expect("hwnd") as HWND;
                // SAFETY: GetForegroundWindow takes no arguments.
                wait("the window to come to the front", || (unsafe { GetForegroundWindow() } == hwnd).then_some(()));
                let send = |inputs: Vec<INPUT>| {
                    // SAFETY: `inputs` holds initialised INPUTs, and its length and element size are the ones passed.
                    let sent = unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), size_of::<INPUT>() as i32) };
                    assert_eq!(sent as usize, inputs.len(), "SendInput took every key");
                };
                let field = |want: &'static str| wait(&format!("the field to show {want:?}"), || {
                    let (tx, rx) = mpsc::channel();
                    handle.update(move |s, _| { let _ = tx.send(s.field.text == want); });
                    rx.recv_timeout(Duration::from_secs(5)).ok().filter(|shown| *shown)
                });
                let pointer = |side: Side| wait("the pointer move sent after the keys", || {
                    let (tx, rx) = mpsc::channel();
                    handle.update(move |s, _| { let _ = tx.send(s.pointer == Some(side)); });
                    rx.recv_timeout(Duration::from_secs(5)).ok().filter(|over| *over)
                });
                send(vec![point_at(hwnd, Side::Right)]);
                pointer(Side::Right);
                let space = [key(VK_SPACE, 0, 0), key(VK_SPACE, 0, KEYEVENTF_KEYUP)];
                send([unicode("a"), space.to_vec(), unicode("😀é")].concat());
                field("a 😀é");
                send([units(HIGH), space.to_vec(), unicode("b")].concat());
                field(TYPED);
                send([units(HIGH), vec![point_at(hwnd, Side::Left)]].concat());
                pointer(Side::Left);
                // SAFETY: PostMessageW copies its arguments, and the window belongs to this process.
                unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0) };
                wait("the close request to see the field", || CLOSING.lock().unwrap().clone());
                send([unicode("c"), vec![point_at(hwnd, Side::Right)]].concat());
                pointer(Side::Right);
                // SAFETY: PostMessageW copies its arguments, and the window belongs to this process.
                unsafe { PostMessageW(event_window(), WM_CLOSE, 0, 0) };
            });
            if run.is_err() {
                let (tx, rx) = mpsc::channel();
                handle.update(move |s, _| { let _ = tx.send(s.field.text.clone()); });
                eprintln!("unicode_input_windows: the field holds {:?}", rx.recv_timeout(Duration::from_secs(5)).ok());
                std::process::exit(1);
            }
        });
        app.run();
        assert_eq!(CLOSING.lock().unwrap().clone(), Some(format!("{TYPED}\u{fffd}")), "a lone half types U+FFFD before the window closes");
        assert_eq!(QUITTING.lock().unwrap().clone(), Some(format!("{TYPED}\u{fffd}c")), "a quit through the hidden event window saves what was typed before it");
        println!("unicode_input_windows: ok");
    }
}
