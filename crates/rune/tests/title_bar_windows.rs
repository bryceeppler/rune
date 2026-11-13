//! Windows draws a window's title bar in the window's own background, dark where the window
//! is dark, whatever the system's light or dark setting, and keeps it through a settings change,
//! after which winit sets the system's mode again. Opens a native window, so it only runs with
//! RUNE_GUI_TESTS=1 in a desktop session.

#[cfg(not(target_os = "windows"))]
fn main() {}

#[cfg(target_os = "windows")]
fn main() { win::main() }

#[cfg(target_os = "windows")]
mod win {
    use rune::*;
    use std::sync::{OnceLock, mpsc};
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmGetWindowAttribute};
    use windows_sys::Win32::Graphics::Gdi::{
        CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetPixel, ReleaseDC, SelectObject,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowRect, SendMessageW, WM_SETTINGCHANGE};

    const LIGHT: &str = "#f4f5f9";
    const DARK: &str = "#1a1b26";
    const PW_RENDERFULLCONTENT: u32 = 2;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn PrintWindow(hwnd: HWND, dc: windows_sys::Win32::Graphics::Gdi::HDC, flags: u32) -> i32;
    }

    struct State { fill: &'static str }

    static WINDOW: OnceLock<WindowHandle> = OnceLock::new();

    fn view(s: &State, _: &mut Cx<State>) -> El {
        col().w_full().h_full().fill(rune::color::hex(s.fill).unwrap()).child(text("Title bar"))
    }

    fn caption(hwnd: HWND) -> [u8; 3] {
        unsafe {
            let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            GetWindowRect(hwnd, &mut rect);
            let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
            let screen = GetDC(std::ptr::null_mut());
            let dc = CreateCompatibleDC(screen);
            let bitmap = CreateCompatibleBitmap(screen, w, h);
            let old = SelectObject(dc, bitmap);
            assert!(PrintWindow(hwnd, dc, PW_RENDERFULLCONTENT) != 0, "PrintWindow");
            let pixel = GetPixel(dc, w / 2, 12);
            SelectObject(dc, old);
            DeleteObject(bitmap);
            DeleteDC(dc);
            ReleaseDC(std::ptr::null_mut(), screen);
            [pixel as u8, (pixel >> 8) as u8, (pixel >> 16) as u8]
        }
    }

    fn dark_mode(hwnd: HWND) -> bool {
        let mut dark = 0i32;
        let read = unsafe { DwmGetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE as u32, (&raw mut dark).cast(), 4) };
        assert!(read >= 0, "reads DWMWA_USE_IMMERSIVE_DARK_MODE");
        dark != 0
    }

    fn rgb(fill: &str) -> [u8; 3] {
        let at = |i: usize| u8::from_str_radix(&fill[i..i + 2], 16).unwrap();
        [at(1), at(3), at(5)]
    }

    pub fn main() {
        if std::env::var_os("RUNE_GUI_TESTS").is_none() {
            println!("title_bar_windows: skipped (set RUNE_GUI_TESTS=1 in a desktop session)");
            return;
        }
        std::thread::spawn(|| { std::thread::sleep(Duration::from_secs(60)); eprintln!("timeout"); std::process::exit(2) });
        let spec = AppSpec::new("Title bar", (360.0, 160.0), || State { fill: LIGHT }, view);
        let (ready_tx, ready_rx) = mpsc::channel();
        let window = &WINDOW;
        let app = App::new(spec).args(Vec::new()).keep_running().on_ready(move |w| { let _ = window.set(w.clone()); let _ = ready_tx.send(()); });
        let handle = app.handle();
        let result = std::sync::Arc::new(std::sync::Mutex::new(None));
        let out = result.clone();
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
                let shows = |fill: &'static str, after: &str| {
                    let want = (rgb(fill), fill == DARK);
                    let start = Instant::now();
                    loop {
                        let seen = (caption(hwnd), dark_mode(hwnd));
                        if seen == want { println!("title_bar_windows: {fill} {after}: {seen:?}"); break; }
                        assert!(start.elapsed() < Duration::from_secs(10), "the title bar for {fill} {after}: want (caption, dark) {want:?}, see {seen:?}");
                        std::thread::sleep(Duration::from_millis(20));
                    }
                };
                for fill in [LIGHT, DARK, LIGHT] {
                    handle.update(move |s, _| s.fill = fill);
                    shows(fill, "after the switch");
                    let setting: Vec<u16> = "ImmersiveColorSet\0".encode_utf16().collect();
                    unsafe { SendMessageW(hwnd, WM_SETTINGCHANGE, 0, setting.as_ptr() as isize) };
                    shows(fill, "after a settings change");
                }
            });
            *out.lock().unwrap() = Some(run.is_ok());
            handle.quit();
        });
        app.run();
        assert_eq!(*result.lock().unwrap(), Some(true), "title_bar_windows failed");
        println!("title_bar_windows: ok");
    }
}
