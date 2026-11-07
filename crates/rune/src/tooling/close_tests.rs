use std::sync::Mutex;

use super::tests::{HEADLESS, focus_of, left, right};
use super::{HeadlessRun, headless_run};
use crate::{AppSpec, CloseKind, CloseRequest, Key};

/// A window that asks before it closes. Its keys: Enter closes it, `q` asks to quit, `y` and
/// `n` answer the request it holds, as a save finishing or failing would.
struct Doc {
    name: &'static str,
    holds: bool,
    held: Option<CloseRequest>,
}

static ASKED: Mutex<Vec<(&'static str, CloseKind)>> = Mutex::new(Vec::new());
static CLOSED: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

fn ask(d: &mut Doc, request: CloseRequest, _: &mut crate::Tx) {
    ASKED.lock().unwrap().push((d.name, request.kind()));
    if d.holds { d.held = Some(request) } else { request.proceed() }
}

fn doc(name: &'static str, holds: bool, options: crate::WindowOptions) -> crate::AnyWindowSpec {
    let spec = AppSpec {
        on_key: Some(|d: &mut Doc, e: &crate::KeyEvent, tx: &mut crate::Tx| {
            match &e.key {
                Key::Enter => tx.close_window(),
                Key::Text(t) if t == "q" => tx.request_quit(),
                Key::Text(t) if t == "y" => if let Some(r) = d.held.take() { r.proceed() },
                Key::Text(t) if t == "n" => if let Some(r) = d.held.take() { r.cancel() },
                _ => return false,
            }
            true
        }),
        ..AppSpec::new("doc", (1.0, 1.0), || Doc { name: "", holds: false, held: None }, |_, _| crate::el().w_full().h_full())
    };
    let app = crate::App::new(spec).init(move || Doc { name, holds, held: None })
        .on_close_request(ask)
        .on_close(|d, _| CLOSED.lock().unwrap().push(d.name));
    crate::WindowSpec::from_app(name, app).options(options).boxed()
}

fn run(script: &str, windows: impl FnOnce() -> Vec<crate::AnyWindowSpec> + 'static) -> &'static HeadlessRun {
    ASKED.lock().unwrap().clear();
    CLOSED.lock().unwrap().clear();
    let dir = std::env::temp_dir().join(format!("rune-close-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let backdrop = dir.join("backdrop.png");
    ::image::RgbaImage::from_pixel(240, 160, ::image::Rgba([128, 128, 128, 255])).save(&backdrop).unwrap();
    let (script_path, tree) = (dir.join("script.json"), dir.join("tree.json"));
    std::fs::write(&script_path, script).unwrap();
    let args = ["--desktop", backdrop.to_str().unwrap(), "--script", script_path.to_str().unwrap(),
        "--scale", "2", "--fps", "10", "--tree", tree.to_str().unwrap()];
    crate::run_windows_with_args(crate::RunOptions::default(), args.map(String::from), move |_| windows());
    std::fs::remove_dir_all(&dir).unwrap();
    headless_run().unwrap()
}

fn closed() -> Vec<&'static str> { CLOSED.lock().unwrap().clone() }

#[test]
fn a_close_waits_for_the_app_and_goes_once_it_proceeds() {
    let _turn = HEADLESS.lock().unwrap_or_else(|e| e.into_inner());
    let script = r#"[{"t": 0.1, "type": "key", "name": "Enter"}, {"t": 0.2, "type": "key", "name": "Enter"},
        {"t": 0.3, "type": "key", "name": "y"}]"#;
    let run = run(script, || vec![doc("a", true, left())]);
    assert_eq!(focus_of(run, 0.2, "a").0, true, "the window stays while its save runs");
    assert_eq!(*ASKED.lock().unwrap(), [("a", CloseKind::Window)], "the second close joins the first");
    assert_eq!(focus_of(run, 0.3, "a").0, false, "it closes once the app proceeds");
    assert_eq!(closed(), ["a"]);
}

#[test]
fn a_cancelled_close_keeps_the_window_and_the_next_close_asks_again() {
    let _turn = HEADLESS.lock().unwrap_or_else(|e| e.into_inner());
    let script = r#"[{"t": 0.1, "type": "key", "name": "Enter"}, {"t": 0.2, "type": "key", "name": "n"},
        {"t": 0.3, "type": "key", "name": "Enter"}]"#;
    let run = run(script, || vec![doc("a", true, left())]);
    assert_eq!(focus_of(run, 0.3, "a"), (true, true));
    assert_eq!(ASKED.lock().unwrap().len(), 2);
    assert!(closed().is_empty());
}

#[test]
fn a_quit_waits_for_every_window_then_closes_them_all() {
    let _turn = HEADLESS.lock().unwrap_or_else(|e| e.into_inner());
    let script = r#"[{"t": 0.1, "quit": true}, {"t": 0.2, "type": "key", "name": "y"}]"#;
    let run = run(script, || vec![doc("b", false, right()), doc("a", true, left())]);
    assert_eq!((focus_of(run, 0.1, "a").0, focus_of(run, 0.1, "b").0), (true, true), "nothing closes while one window waits");
    assert!(closed().len() == 2 && focus_of(run, 0.2, "a").0 == false && focus_of(run, 0.2, "b").0 == false);
    assert_eq!(*ASKED.lock().unwrap(), [("b", CloseKind::Quit), ("a", CloseKind::Quit)]);
}

#[test]
fn one_cancel_ends_a_quit_the_app_asked_for() {
    let _turn = HEADLESS.lock().unwrap_or_else(|e| e.into_inner());
    let script = r#"[{"t": 0.1, "type": "key", "name": "q"}, {"t": 0.2, "type": "key", "name": "n"},
        {"t": 0.3, "type": "key", "name": "q"}]"#;
    let run = run(script, || vec![doc("b", false, right()), doc("a", true, left())]);
    assert_eq!((focus_of(run, 0.3, "a").0, focus_of(run, 0.3, "b").0), (true, true));
    assert!(closed().is_empty());
    assert_eq!(ASKED.lock().unwrap().iter().filter(|(w, _)| *w == "a").count(), 2, "a cancelled quit is asked afresh");
}
