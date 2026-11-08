use std::sync::Mutex;

use super::tests::{HEADLESS, focus_of, left, right};
use super::{HeadlessRun, headless_run};
use crate::{AppSpec, CloseKind, CloseRequest, Key, WindowGeometry};

/// A window that asks before it closes, saving first when it `holds`. Its keys: Enter closes it,
/// `q` asks to quit, `y` and `n` answer the request it holds, as its save finishing or failing
/// would, and `m` moves it. A request asked once it has saved proceeds at once.
struct Doc {
    name: &'static str,
    holds: bool,
    saved: bool,
    held: Option<CloseRequest>,
    me: Option<crate::AppHandle<Doc>>,
}

static ASKED: Mutex<Vec<(&'static str, CloseKind)>> = Mutex::new(Vec::new());
static CLOSED: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
static GEOMETRY: Mutex<Vec<(&'static str, WindowGeometry)>> = Mutex::new(Vec::new());

fn ask(d: &mut Doc, request: CloseRequest, _: &mut crate::Tx) {
    ASKED.lock().unwrap().push((d.name, request.kind()));
    if d.holds && !d.saved { d.held = Some(request) } else { request.proceed() }
}

fn doc(name: &'static str, holds: bool, options: crate::WindowOptions, restore: Option<WindowGeometry>) -> crate::AnyWindowSpec {
    crate::WindowSpec::from_app(name, doc_app(name, holds, restore)).options(options).boxed()
}

fn doc_app(name: &'static str, holds: bool, restore: Option<WindowGeometry>) -> crate::App<Doc> {
    let spec = AppSpec {
        on_key: Some(|d: &mut Doc, e: &crate::KeyEvent, tx: &mut crate::Tx| {
            match &e.key {
                Key::Enter => tx.close_window(),
                Key::Text(t) if t == "q" => tx.request_quit(),
                Key::Text(t) if t == "y" => if let Some(r) = d.held.take() { d.saved = true; r.proceed() },
                Key::Text(t) if t == "n" => if let Some(r) = d.held.take() { r.cancel() },
                Key::Text(t) if t == "m" => if let Some(me) = &d.me { me.set_position((40, 30)) },
                _ => return false,
            }
            true
        }),
        ..AppSpec::new("doc", (1.0, 1.0), || Doc { name: "", holds: false, saved: false, held: None, me: None }, |_, _| crate::el().w_full().h_full())
    };
    let app = crate::App::new(spec);
    let me = Some(app.handle());
    let mut app = app.init(move || Doc { name, holds, saved: false, held: None, me })
        .on_close_request(ask)
        .on_close(|d, _| CLOSED.lock().unwrap().push(d.name))
        .on_geometry(|d, g, _| { GEOMETRY.lock().unwrap().push((d.name, g.clone())); false });
    if let Some(saved) = restore { app = app.restore_geometry(saved); }
    app
}

fn run(script: &str, windows: impl FnOnce() -> Vec<crate::AnyWindowSpec> + 'static) -> &'static HeadlessRun {
    ASKED.lock().unwrap().clear();
    CLOSED.lock().unwrap().clear();
    GEOMETRY.lock().unwrap().clear();
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
    let run = run(script, || vec![doc("a", true, left(), None)]);
    assert_eq!(focus_of(run, 0.2, "a").0, true, "the window stays while its save runs");
    // The second close joins the first; the answer, given after more input reached the window,
    // asks again, and the saved window proceeds.
    assert_eq!(*ASKED.lock().unwrap(), [("a", CloseKind::Window); 2]);
    assert_eq!(focus_of(run, 0.3, "a").0, false, "it closes once the app proceeds");
    assert_eq!(closed(), ["a"]);
}

#[test]
fn a_cancelled_close_keeps_the_window_and_the_next_close_asks_again() {
    let _turn = HEADLESS.lock().unwrap_or_else(|e| e.into_inner());
    let script = r#"[{"t": 0.1, "type": "key", "name": "Enter"}, {"t": 0.2, "type": "key", "name": "n"},
        {"t": 0.3, "type": "key", "name": "Enter"}]"#;
    let run = run(script, || vec![doc("a", true, left(), None)]);
    assert_eq!(focus_of(run, 0.3, "a"), (true, true));
    assert_eq!(ASKED.lock().unwrap().len(), 2);
    assert!(closed().is_empty());
}

#[test]
fn a_quit_waits_for_every_window_then_closes_them_all() {
    let _turn = HEADLESS.lock().unwrap_or_else(|e| e.into_inner());
    let script = r#"[{"t": 0.1, "quit": true}, {"t": 0.2, "type": "key", "name": "y"}]"#;
    let run = run(script, || vec![doc("b", false, right(), None), doc("a", true, left(), None)]);
    assert_eq!((focus_of(run, 0.1, "a").0, focus_of(run, 0.1, "b").0), (true, true), "nothing closes while one window waits");
    assert!(closed().len() == 2 && focus_of(run, 0.2, "a").0 == false && focus_of(run, 0.2, "b").0 == false);
    assert_eq!(*ASKED.lock().unwrap(), [("b", CloseKind::Quit), ("a", CloseKind::Quit), ("a", CloseKind::Quit)]);
}

#[test]
fn one_cancel_ends_a_quit_the_app_asked_for() {
    let _turn = HEADLESS.lock().unwrap_or_else(|e| e.into_inner());
    let script = r#"[{"t": 0.1, "type": "key", "name": "q"}, {"t": 0.2, "type": "key", "name": "n"},
        {"t": 0.3, "type": "key", "name": "q"}]"#;
    let run = run(script, || vec![doc("b", false, right(), None), doc("a", true, left(), None)]);
    assert_eq!((focus_of(run, 0.3, "a").0, focus_of(run, 0.3, "b").0), (true, true));
    assert!(closed().is_empty());
    assert_eq!(ASKED.lock().unwrap().iter().filter(|(w, _)| *w == "a").count(), 2, "a cancelled quit is asked afresh");
}

#[test]
fn closing_the_last_main_window_asks_the_others_before_the_app_ends() {
    let _turn = HEADLESS.lock().unwrap_or_else(|e| e.into_inner());
    let script = r#"[{"t": 0.1, "type": "tap", "x": 20, "y": 20}, {"t": 0.2, "type": "key", "name": "Enter"},
        {"t": 0.3, "type": "key", "name": "y"}]"#;
    let run = run(script, || {
        let mut main = doc_app("a", false, None);
        let _ = main.window(crate::WindowSpec::from_app("settings", doc_app("settings", true, None)).options(right()));
        vec![crate::WindowSpec::from_app("a", main).options(left()).boxed()]
    });
    assert_eq!(focus_of(run, 0.2, "settings").0, true, "Settings stays while its save runs");
    assert_eq!(*ASKED.lock().unwrap(), [("a", CloseKind::Window), ("settings", CloseKind::Quit), ("settings", CloseKind::Quit)]);
    assert_eq!(focus_of(run, 0.3, "settings").0, false, "the app ends once Settings proceeds");
    assert_eq!(closed(), ["a", "settings"]);
}

#[test]
fn a_window_saved_on_a_display_that_is_gone_opens_wholly_on_a_present_one() {
    let _turn = HEADLESS.lock().unwrap_or_else(|e| e.into_inner());
    // The run's one display is the backdrop's 120 by 80 points.
    let saved = WindowGeometry { frame: Some([2400, 300, 60, 40]), size: (60.0, 40.0), scale: 2.0, ..Default::default() };
    let small = crate::WindowOptions { min_size: Some((40.0, 30.0)), ..Default::default() };
    let run = run(r#"[{"t": 0.1, "type": "key", "name": "m"}]"#, move || vec![doc("a", false, small, Some(saved))]);
    let [x, y, w, h] = run.windows_at(0.0)[0].frame;
    assert_eq!([x, y, w, h], [30.0, 20.0, 60.0, 40.0], "centred on the display present");
    let reported = GEOMETRY.lock().unwrap().first().cloned().unwrap().1;
    assert_eq!(reported.frame, Some([x, y, w, h].map(|v| v as i32)), "on_geometry reports where it opened");
    let moved = GEOMETRY.lock().unwrap().last().cloned().unwrap().1;
    assert_eq!(moved.frame, Some([20, 15, 60, 40]), "and again when it moves");
}
