//! Text another program types as Unicode input, as password managers' auto-type and AutoHotkey
//! do: `SendInput` with `KEYEVENTF_UNICODE` sends each UTF-16 unit as a `VK_PACKET` key, an
//! emoji as two. It lands in the focused field like typed text, in order with ordinary keys, and
//! half an emoji types U+FFFD before the next ordinary key and before the window closes.
//! Types into the foreground window, so it only runs with RUNE_GUI_TESTS=1 in a desktop session
//! nobody is using.

#[cfg(not(target_os = "windows"))]
fn main() {}

#[cfg(target_os = "windows")]
fn main() { win::main() }

#[cfg(target_os = "windows")]
mod win {
    use rune::*;
    use std::sync::{OnceLock, mpsc};
    use std::time::{Duration, Instant};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, SendInput, VK_SPACE,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, PostMessageW, WM_CLOSE};

    struct State { field: TextEdit }

    static WINDOW: OnceLock<WindowHandle> = OnceLock::new();
    static CLOSING: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

    fn on_close_request(s: &mut State, request: CloseRequest, _: &mut Tx) {
        match request.kind() {
            CloseKind::Window => { *CLOSING.lock().unwrap() = Some(s.field.text.clone()); request.cancel() }
            CloseKind::Quit => request.proceed(),
        }
    }

    const HIGH: &[u16] = &[0xd83d];
    const TYPED: &str = "a 😀é\u{fffd} b";

    fn view(s: &State, _: &mut Cx<State>) -> El {
        col().padding(12.0)
            .child(text_input(&s.field, rune::tok::type_::BODY).key("field").single_line().w(240.0).on_edit(|s: &mut State| &mut s.field))
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
        let spec = AppSpec::new("Unicode input", (300.0, 100.0), || { let mut field = TextEdit::default(); field.focused = true; State { field } }, view);
        let (ready_tx, ready_rx) = mpsc::channel();
        let window = &WINDOW;
        let app = App::new(spec).args(Vec::new()).keep_running().on_close_request(on_close_request).on_ready(move |w| { let _ = window.set(w.clone()); let _ = ready_tx.send(()); });
        let handle = app.handle();
        let typed = std::sync::Arc::new(std::sync::Mutex::new(None));
        let out = typed.clone();
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
                let hwnd = rx.recv_timeout(Duration::from_secs(5)).expect("hwnd");
                wait("the window to come to the front", || (unsafe { GetForegroundWindow() } as isize == hwnd).then_some(()));
                let send = |inputs: Vec<INPUT>| {
                    // SAFETY: `inputs` is a slice of initialised INPUTs whose size is passed.
                    let sent = unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), size_of::<INPUT>() as i32) };
                    assert_eq!(sent as usize, inputs.len(), "SendInput took every key");
                };
                let field = |want: &str| wait(&format!("the field to show {want:?}"), || {
                    let (tx, rx) = mpsc::channel();
                    handle.update(move |s, _| { let _ = tx.send(s.field.text.clone()); });
                    rx.recv_timeout(Duration::from_secs(5)).ok().filter(|text| text == want)
                });
                let space = [key(VK_SPACE, 0, 0), key(VK_SPACE, 0, KEYEVENTF_KEYUP)];
                send([unicode("a"), space.to_vec(), unicode("😀é")].concat());
                field("a 😀é");
                send([units(HIGH), space.to_vec(), unicode("b")].concat());
                field(TYPED);
                send(units(HIGH));
                std::thread::sleep(Duration::from_millis(300));
                // SAFETY: a window of this process; posting needs nothing else.
                unsafe { PostMessageW(hwnd as _, WM_CLOSE, 0, 0) };
                wait("the close request to see the field", || CLOSING.lock().unwrap().clone())
            });
            if let Err(panic) = &run {
                let (tx, rx) = mpsc::channel();
                handle.update(move |s, _| { let _ = tx.send(s.field.text.clone()); });
                eprintln!("unicode_input_windows: the field holds {:?}: {panic:?}", rx.recv_timeout(Duration::from_secs(5)).ok());
            }
            *out.lock().unwrap() = Some(run.ok());
            handle.quit();
        });
        app.run();
        assert_eq!(typed.lock().unwrap().clone(), Some(Some(format!("{TYPED}\u{fffd}"))), "unicode_input_windows failed");
        println!("unicode_input_windows: ok");
    }
}
