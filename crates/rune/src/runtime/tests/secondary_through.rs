use super::*;
use crate::{ModalKind, row};

#[derive(Default)]
struct State {
    menu: Option<&'static str>,
    pressed: Vec<&'static str>,
    keeps_open: bool,
    kind: ModalKind,
}

fn view(s: &State, _: &mut Cx<State>) -> El {
    let word = |key: &'static str| {
        el().key(key).size(80.0, 30.0).on_secondary_press(move |s: &mut State, _, _| {
            s.pressed.push(key);
            s.menu = Some(key);
        })
    };
    let mut root = col().key("root").size(300.0, 300.0).child(row().child(word("first")).child(word("second")));
    if let Some(on) = s.menu {
        let close = s.keeps_open;
        root = root.child(col().key(format!("menu-{on}")).role("menu").size(100.0, 60.0).focus_group()
            .modal(!close, |s: &mut State| s.menu = None).modal_kind(s.kind)
            .child(row().key("item").role("menuitem").size(100.0, 28.0).focusable(true).on_press(|_: &mut State| {})));
    }
    root
}

fn runtime(keeps_open: bool) -> Runtime<State> {
    runtime_of(ModalKind::Menu, keeps_open)
}

fn runtime_of(kind: ModalKind, keeps_open: bool) -> Runtime<State> {
    let mut rt = Runtime::new(State { keeps_open, kind, ..State::default() }, view, (300.0, 300.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt
}

fn right_click(rt: &mut Runtime<State>, (x, y): (f32, f32)) {
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    while rt.stale() { rt.frame(0.0); }
}

#[test]
fn a_right_click_outside_a_menu_closes_it_and_opens_the_menu_of_what_is_under_it() {
    let mut rt = runtime(false);
    let first = rt.find("first").unwrap();
    let second = rt.find("second").unwrap();
    right_click(&mut rt, first);
    assert_eq!(rt.state.menu, Some("first"));
    right_click(&mut rt, second);
    assert_eq!(rt.state.menu, Some("second"), "the second right-click's own menu");
    assert_eq!(rt.state.pressed, ["first", "second"]);
}

#[test]
fn a_right_click_held_across_the_frame_that_closes_the_menu_opens_the_next_on_release() {
    let mut rt = runtime(false);
    let first = rt.find("first").unwrap();
    let (x, y) = rt.find("second").unwrap();
    right_click(&mut rt, first);
    rt.secondary_down(x, y);
    rt.frame(0.0);
    rt.frame(0.0);
    assert_eq!(rt.state.menu, None, "the first menu closed while the button is held");
    rt.secondary_up(x, y);
    rt.frame(0.0);
    assert_eq!(rt.state.menu, Some("second"));
}

#[test]
fn a_right_click_outside_a_modal_that_outside_presses_leave_open_does_nothing() {
    let mut rt = runtime(true);
    let first = rt.find("first").unwrap();
    let second = rt.find("second").unwrap();
    right_click(&mut rt, first);
    right_click(&mut rt, second);
    assert_eq!(rt.state.menu, Some("first"));
    assert_eq!(rt.state.pressed, ["first"]);
}

#[test]
fn a_right_click_outside_a_sheet_dismisses_it_and_reaches_nothing_behind_it() {
    let mut rt = runtime_of(ModalKind::Dialog, false);
    let first = rt.find("first").unwrap();
    let second = rt.find("second").unwrap();
    right_click(&mut rt, first);
    right_click(&mut rt, second);
    assert_eq!(rt.state.menu, None, "an outside press dismisses this sheet");
    assert_eq!(rt.state.pressed, ["first"], "the content behind it heard nothing");
}
