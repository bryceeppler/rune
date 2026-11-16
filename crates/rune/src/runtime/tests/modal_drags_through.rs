use super::*;

#[derive(Default)]
struct State {
    open: bool,
    dropped: Vec<f32>,
    pressed: u32,
}

fn view(s: &State, _: &mut Cx<State>) -> El {
    let row = el().key("row").size(200.0, 30.0).on_press(|s: &mut State| s.pressed += 1).on_drag(|s: &mut State, d: &mut Drag, _| {
        if d.phase == Phase::Up && !d.cancelled() {
            s.dropped.push(d.dx);
        }
    });
    let plain = el().key("plain").size(200.0, 30.0).on_press(|s: &mut State| s.pressed += 1);
    let mut root = col().key("root").size(300.0, 300.0).child(row).child(plain);
    if s.open {
        root = root.child(
            col().key("float").absolute(0.0, 100.0).size(100.0, 100.0).modal(true, |s: &mut State| s.open = false)
                .modal_drags_through(true)
                .child(el().key("inside").size(100.0, 30.0).focusable(true).on_press(|_: &mut State| {})),
        );
    }
    root
}

fn runtime() -> Runtime<State> {
    let mut rt = Runtime::new(State { open: true, ..State::default() }, view, (300.0, 300.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt
}

fn settle(rt: &mut Runtime<State>) {
    while rt.stale() { rt.frame(0.0); }
}

#[test]
fn a_drag_that_starts_behind_a_drag_through_modal_keeps_it_open_and_reaches_its_element() {
    let mut rt = runtime();
    let (x, y) = rt.find("row").unwrap();
    rt.pointer_down(x, y);
    rt.pointer_move(x - 20.0, y + 60.0);
    rt.pointer_move(x - 40.0, y + 120.0);
    rt.pointer_up(x - 40.0, y + 120.0);
    settle(&mut rt);
    assert!(rt.state.open, "the modal stays open through the drag");
    assert_eq!(rt.state.dropped, [-40.0]);
    assert_eq!(rt.state.pressed, 0, "a drag through the modal is no press");
}

#[test]
fn a_click_behind_a_drag_through_modal_dismisses_it_without_pressing_what_is_under_it() {
    let mut rt = runtime();
    let (x, y) = rt.find("row").unwrap();
    rt.pointer_down(x, y);
    rt.pointer_up(x + 1.0, y);
    settle(&mut rt);
    assert!(!rt.state.open);
    assert_eq!(rt.state.pressed, 0);
}

#[test]
fn a_press_behind_it_on_what_does_not_drag_dismisses_it_at_once() {
    let mut rt = runtime();
    let (x, y) = rt.find("plain").unwrap();
    rt.pointer_down(x, y);
    settle(&mut rt);
    assert!(!rt.state.open);
    rt.pointer_up(x, y);
    settle(&mut rt);
    assert_eq!(rt.state.pressed, 0);
}
