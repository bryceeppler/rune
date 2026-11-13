//! A close message to winit's hidden event window, which `Process.CloseMainWindow()` sends
//! while the app's window is minimized and `taskkill` without `/F` sends to every top-level
//! window: it asks the app to quit, and the app can still quit after refusing that quit. Opens
//! a native window, so it only runs with RUNE_GUI_TESTS=1 in a desktop session.

#[cfg(not(target_os = "windows"))]
fn main() {}

#[cfg(target_os = "windows")]
fn main() { win::main() }

#[cfg(target_os = "windows")]
mod win {
    use rune::*;
    use std::sync::{Mutex, mpsc};
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{EnumWindows, GetClassNameW, GetWindowThreadProcessId, PostMessageW, WM_CLOSE};

    static ASKED: Mutex<Vec<CloseKind>> = Mutex::new(Vec::new());

    fn view(_: &(), _: &mut Cx<()>) -> El { text("Quit me") }

    fn on_close_request(_: &mut (), request: CloseRequest, _: &mut Tx) {
        let mut asked = ASKED.lock().unwrap();
        asked.push(request.kind());
        if asked.len() == 1 { request.cancel() } else { request.proceed() }
    }

    fn asked() -> Vec<CloseKind> { ASKED.lock().unwrap().clone() }

    unsafe extern "system" fn collect(hwnd: HWND, found: LPARAM) -> i32 {
        let mut pid = 0;
        // SAFETY: `found` is the `Vec` `event_windows` passes, alive for the enumeration.
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut pid);
            let mut class = [0u16; 64];
            let len = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32);
            if pid == GetCurrentProcessId() && String::from_utf16_lossy(&class[..len.max(0) as usize]) == "Winit Thread Event Target" {
                (*(found as *mut Vec<usize>)).push(hwnd as usize);
            }
        }
        1
    }

    fn event_windows() -> Vec<usize> {
        let mut found = Vec::<usize>::new();
        // SAFETY: `collect` reads `found` only during the call.
        unsafe { EnumWindows(Some(collect), &mut found as *mut Vec<usize> as LPARAM) };
        found
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
            println!("hidden_window_close_windows: skipped (set RUNE_GUI_TESTS=1 in a desktop session)");
            return;
        }
        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_secs(60));
            eprintln!("hidden_window_close_windows: timed out with the app still running, asked {:?}", asked());
            std::process::exit(2)
        });
        let (ready_tx, ready_rx) = mpsc::channel();
        let app = App::new(AppSpec::new("Quit me", (240.0, 120.0), || (), view))
            .args(Vec::new())
            .keep_running()
            .on_close_request(on_close_request)
            .on_ready(move |_| { let _ = ready_tx.send(()); });
        let handle = app.handle();
        std::thread::spawn(move || {
            let closed = std::panic::catch_unwind(move || {
                ready_rx.recv_timeout(Duration::from_secs(10)).expect("window ready");
                let hidden = event_windows();
                assert_eq!(hidden.len(), 1, "winit's one event window");
                // SAFETY: a window of this process; posting needs nothing else.
                assert!(unsafe { PostMessageW(hidden[0] as HWND, WM_CLOSE, 0, 0) } != 0, "WM_CLOSE posted");
                wait("the close message to ask the app to quit", || (asked() == [CloseKind::Quit]).then_some(()));
                assert_eq!(event_windows(), hidden, "the event window outlives its close message");
            });
            if closed.is_err() { std::process::exit(1) }
            handle.update(|_, tx| tx.request_quit());
        });
        app.run();
        assert_eq!(asked(), [CloseKind::Quit, CloseKind::Quit], "the refused quit, then the one that ended the app");
        println!("hidden_window_close_windows: ok");
    }
}
