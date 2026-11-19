use super::*;
use crate::{Key, KeyEvent, Mods};

#[derive(Default)]
struct State {
    open: bool,
    stays_usable: bool,
}

fn view(s: &State, _: &mut Cx<State>) -> El {
    let panel = col().key("panel").size(100.0, 100.0).child(el().key("inside").size(80.0, 30.0).focusable(true).role("button"));
    let panel = match (s.open, s.stays_usable) {
        (true, _) => panel.modal(true, |s: &mut State| s.open = false),
        (false, true) => panel,
        (false, false) => panel.inert(),
    };
    col().key("root").size(300.0, 300.0)
        .child(el().key("opener").size(60.0, 30.0).focusable(true).role("button").on_press(|s: &mut State| s.open = true))
        .child(panel)
}

fn key(rt: &mut Runtime<State>, key: Key) {
    rt.key(KeyEvent { key, mods: Mods::default() });
    rt.frame(0.0);
    rt.frame(0.0);
}

fn opened(stays_usable: bool) -> (Runtime<State>, String) {
    let mut rt = Runtime::new(State { open: false, stays_usable }, view, (300.0, 300.0), Env::new(true, 1.0));
    rt.frame(0.0);
    key(&mut rt, Key::Tab);
    let opener = rt.focused.clone().expect("the opener focused");
    assert!(opener.ends_with("opener"), "{opener}");
    (rt, opener)
}

fn open(rt: &mut Runtime<State>) {
    key(rt, Key::Text(" ".into()));
    assert!(rt.state.open);
    assert!(rt.focused.as_deref().is_some_and(|f| f.ends_with("inside")), "{:?}", rt.focused);
}

#[test]
fn a_modal_that_stays_mounted_but_stops_being_modal_returns_focus_each_time_it_closes() {
    let (mut rt, opener) = opened(false);
    for _ in 0..2 {
        open(&mut rt);
        key(&mut rt, Key::Escape);
        assert!(!rt.state.open);
        assert_eq!(rt.focused.as_deref(), Some(opener.as_str()), "closing returns focus to the opener");
    }
}

#[test]
fn a_modal_that_stops_being_modal_but_stays_usable_keeps_the_focus_inside_it() {
    let (mut rt, _) = opened(true);
    open(&mut rt);
    rt.update(|s, _| s.open = false);
    rt.frame(0.0);
    rt.frame(0.0);
    assert!(rt.focused.as_deref().is_some_and(|f| f.ends_with("inside")), "{:?}", rt.focused);
}
