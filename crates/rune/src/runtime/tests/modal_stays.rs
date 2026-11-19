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

#[derive(Default)]
struct Field {
    open: bool,
    panel_leaves: bool,
    field_inert: bool,
    field: crate::TextEdit,
}

fn field_view(s: &Field, _: &mut Cx<Field>) -> El {
    let field = crate::text_input(&s.field, crate::TextStyle::new(14.0, 400.0)).key("field").size(100.0, 40.0).on_edit(|s: &mut Field| &mut s.field);
    let panel = col().key("panel").size(100.0, 100.0).child(el().key("inside").size(80.0, 30.0).focusable(true));
    let panel = match (s.open, s.panel_leaves) {
        (true, _) => Some(panel.modal(true, |s: &mut Field| s.open = false)),
        (false, false) => Some(panel.inert()),
        (false, true) => None,
    };
    col().key("root").size(300.0, 300.0)
        .child(if s.field_inert { field.inert() } else { field })
        .child(el().key("fallback").size(80.0, 30.0).focusable(true))
        .children(panel)
}

fn field_key(rt: &mut Runtime<Field>, key: Key) {
    rt.key(KeyEvent { key, mods: Mods::default() });
    rt.frame(0.0);
    rt.frame(0.0);
}

fn closing_leaves_the_keyboard_off_a_field_turned_inert(panel_leaves: bool) {
    let mut rt = Runtime::new(Field { panel_leaves, ..Field::default() }, field_view, (300.0, 300.0), Env::new(true, 1.0));
    rt.frame(0.0);
    field_key(&mut rt, Key::Tab);
    assert_eq!(rt.focused.as_deref(), Some("root/field"));
    rt.update(|s, _| s.open = true);
    rt.frame(0.0);
    rt.frame(0.0);
    assert_eq!(rt.focused.as_deref(), Some("root/panel/inside"));
    rt.update(|s, _| s.field_inert = true);
    rt.frame(0.0);
    field_key(&mut rt, Key::Escape);
    assert!(!rt.state.open);
    field_key(&mut rt, Key::Text("typed".into()));
    assert_eq!(rt.state.field.text, "", "typing never reaches the inert field");
    assert_eq!(rt.focused, None);
    field_key(&mut rt, Key::Tab);
    assert_eq!(rt.focused.as_deref(), Some("root/fallback"), "Tab finds the next usable control");
}

#[test]
fn a_modal_that_stops_being_modal_leaves_the_keyboard_off_a_field_turned_inert() {
    closing_leaves_the_keyboard_off_a_field_turned_inert(false);
}

#[test]
fn a_modal_that_leaves_the_tree_leaves_the_keyboard_off_a_field_turned_inert() {
    closing_leaves_the_keyboard_off_a_field_turned_inert(true);
}
