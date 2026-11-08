use super::*;
use crate::{Color, Key, KeyEvent, Mods, row};

#[derive(Default)]
struct State { open: bool, picked: Vec<usize> }

fn view(s: &State, _: &mut Cx<State>) -> El {
    let mut root = col().key("root").size(300.0, 300.0)
        .child(el().key("opener").size(60.0, 30.0).focusable(true).role("button").on_press(|s: &mut State| s.open = true)
            .on_secondary_press(|s: &mut State, _, _| s.open = true));
    if s.open {
        let items = (0..3).map(|i| row().key(format!("item{i}")).role("menuitem").size(200.0, 28.0).focusable(true)
            .focus_highlight(Color::WHITE).on_press(move |s: &mut State| { s.picked.push(i); s.open = false; }));
        root = root.child(col().key("menu").role("menu").focus_group().modal(true, |s: &mut State| s.open = false).children(items));
    }
    root
}

fn runtime() -> Runtime<State> {
    let mut rt = Runtime::new(State::default(), view, (300.0, 300.0), Env::new(true, 1.0));
    rt.on_key = Some(|s, ev, _| {
        let opens = ev.key == Key::Function(10);
        s.open |= opens;
        opens
    });
    rt.frame(0.0);
    rt
}

fn key(rt: &mut Runtime<State>, key: Key) {
    rt.key(KeyEvent { key, mods: Mods::default() });
    rt.frame(0.0);
    rt.frame(0.0);
}

#[test]
fn a_menu_a_key_opened_highlights_its_first_item_and_return_chooses_it() {
    let mut rt = runtime();
    key(&mut rt, Key::Function(10));
    assert!(rt.state.open);
    key(&mut rt, Key::Enter);
    assert_eq!(rt.state.picked, [0]);
}

#[test]
fn a_menu_a_click_opened_waits_for_a_key_before_return_chooses() {
    let mut rt = runtime();
    let (x, y) = rt.find("opener").unwrap();
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.frame(0.0);
    assert!(rt.state.open);
    key(&mut rt, Key::Enter);
    assert!(rt.state.picked.is_empty(), "no item showed a highlight to choose");
    key(&mut rt, Key::Down);
    key(&mut rt, Key::Enter);
    assert_eq!(rt.state.picked.len(), 1);
}

#[test]
fn a_menu_a_click_opened_after_an_earlier_key_still_waits_for_a_key_before_return_chooses() {
    let mut rt = runtime();
    key(&mut rt, Key::Text("a".into()));
    let (x, y) = rt.find("opener").unwrap();
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.frame(0.0);
    assert!(rt.state.open);
    key(&mut rt, Key::Enter);
    assert!(rt.state.picked.is_empty(), "a pointer-opened menu waits for navigation");
}

#[test]
fn a_menu_a_right_click_opened_waits_for_a_key_before_return_chooses_even_after_an_earlier_key() {
    for earlier_key in [false, true] {
        let mut rt = runtime();
        if earlier_key {
            key(&mut rt, Key::Text("a".into()));
        }
        let (x, y) = rt.find("opener").unwrap();
        rt.secondary_down(x, y);
        rt.secondary_up(x, y);
        rt.frame(0.0);
        rt.frame(0.0);
        assert!(rt.state.open);
        key(&mut rt, Key::Enter);
        assert!(rt.state.picked.is_empty(), "earlier key {earlier_key}: picked {:?}", rt.state.picked);
    }
}
