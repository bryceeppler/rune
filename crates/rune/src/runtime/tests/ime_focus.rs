use super::*;
use crate::{Ime, Key, KeyEvent, Mods, TextEdit, TextStyle, text_input};

struct Notes {
    a: TextEdit,
    b: TextEdit,
    show_a: bool,
    changed: Vec<&'static str>,
}

fn view(s: &Notes, _: &mut Cx<Notes>) -> El {
    let field = |edit: &TextEdit, key: &'static str| text_input(edit, TextStyle::new(14.0, 400.0)).key(key).size(200.0, 40.0);
    col().size(300.0, 200.0)
        .children(s.show_a.then(|| field(&s.a, "a")
            .on_edit(|s: &mut Notes| &mut s.a)
            .on_input_change(|s: &mut Notes, _| s.changed.push("a"))))
        .child(field(&s.b, "b")
            .on_edit(|s: &mut Notes| &mut s.b)
            .on_input_change(|s: &mut Notes, _| s.changed.push("b")))
        .child(el().key("button").size(100.0, 40.0).focusable(true).role("button"))
}

fn composing_in_a() -> Runtime<Notes> {
    let mut a = TextEdit::new("");
    a.focused = true;
    let notes = Notes { a, b: TextEdit::new(""), show_a: true, changed: Vec::new() };
    let mut rt = Runtime::new(notes, view, (300.0, 200.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt.ime(Ime::Preedit("か".into(), Some((0, 3))));
    rt.frame(0.0);
    assert!(rt.state.a.preedit.is_some(), "a composes");
    rt
}

fn send(rt: &mut Runtime<Notes>, events: impl IntoIterator<Item = Ime>) {
    for ev in events { rt.ime(ev); }
    rt.frame(0.0);
}

/// What an input method sends once the platform resets it and it commits: the preedit
/// clears, the text commits, and the input method turns off.
fn reset_commits() -> [Ime; 3] {
    [Ime::Preedit(String::new(), None), Ime::Commit("家".into()), Ime::Disabled]
}

fn reset_cancels() -> [Ime; 2] {
    [Ime::Preedit(String::new(), None), Ime::Disabled]
}

fn click(rt: &mut Runtime<Notes>, x: f32, y: f32) {
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
}

fn focused(rt: &Runtime<Notes>) -> Option<String> {
    rt.elements().into_iter().find(|e| e.focused).and_then(|e| e.key)
}

fn assert_landed_in_a(rt: &Runtime<Notes>) {
    assert_eq!(rt.state.a.text, "家", "the commit lands in the field that composed it");
    assert_eq!(rt.state.b.text, "", "the field that took focus gets nothing from it");
    assert_eq!(rt.state.changed, ["a"], "only a hears on_input_change");
    assert!(rt.state.a.preedit.is_none() && rt.state.b.preedit.is_none());
}

fn assert_b_composes_its_own(rt: &mut Runtime<Notes>) {
    send(rt, [Ime::Enabled, Ime::Preedit("ね".into(), None)]);
    assert_eq!(rt.state.b.preedit.as_ref().map(|(s, _)| s.as_str()), Some("ね"), "a new composition belongs to b");
    send(rt, [Ime::Preedit(String::new(), None), Ime::Commit("猫".into())]);
    assert_eq!(rt.state.b.text, "猫");
    assert_eq!(rt.state.changed.last(), Some(&"b"));
}

#[test]
fn a_pointer_press_on_another_field_finishes_the_composition_where_it_began() {
    let mut rt = composing_in_a();
    click(&mut rt, 100.0, 60.0);
    assert_eq!(focused(&rt).as_deref(), Some("b"));
    assert!(rt.ime_reset, "the platform input method resets");
    send(&mut rt, reset_commits());
    assert_landed_in_a(&rt);
    assert_b_composes_its_own(&mut rt);
}

#[test]
fn a_pointer_press_on_a_control_finishes_the_composition_where_it_began() {
    let mut rt = composing_in_a();
    click(&mut rt, 50.0, 100.0);
    assert_eq!(focused(&rt).as_deref(), Some("button"));
    send(&mut rt, reset_commits());
    assert_landed_in_a(&rt);
}

#[test]
fn tab_finishes_the_composition_where_it_began() {
    let mut rt = composing_in_a();
    rt.key(KeyEvent { key: Key::Tab, mods: Mods::default() });
    rt.frame(0.0);
    assert_eq!(focused(&rt).as_deref(), Some("b"));
    send(&mut rt, reset_commits());
    assert_landed_in_a(&rt);
    assert_b_composes_its_own(&mut rt);
}

#[test]
fn a_programmatic_focus_request_finishes_the_composition_where_it_began() {
    let mut rt = composing_in_a();
    rt.update(|s, _| s.b.focused = true);
    rt.frame(0.0);
    assert_eq!(focused(&rt).as_deref(), Some("b"));
    assert!(rt.ime_reset);
    send(&mut rt, reset_commits());
    assert_landed_in_a(&rt);
    assert_b_composes_its_own(&mut rt);
}

#[test]
fn window_deactivation_finishes_the_composition_where_it_began_even_if_focus_moves_first() {
    let mut rt = composing_in_a();
    rt.cancel_composition();
    rt.frame(0.0);
    assert!(rt.state.a.preedit.is_none() && rt.state.a.focused, "a keeps focus while the window is away");
    rt.update(|s, _| s.b.focused = true);
    rt.frame(0.0);
    send(&mut rt, reset_commits());
    assert_landed_in_a(&rt);
}

#[test]
fn a_removed_field_takes_its_composition_with_it() {
    let mut rt = composing_in_a();
    rt.update(|s, _| { s.show_a = false; s.b.focused = true; });
    rt.frame(0.0);
    assert_eq!(focused(&rt).as_deref(), Some("b"));
    assert!(rt.ime_reset);
    send(&mut rt, reset_commits());
    assert_eq!(rt.state.b.text, "", "a removed field's commit never reaches the field that took focus");
    assert!(rt.state.changed.is_empty());
    assert_b_composes_its_own(&mut rt);
}

#[test]
fn a_new_model_under_the_same_field_never_hears_the_old_composition() {
    let mut rt = composing_in_a();
    rt.update(|s, _| { s.a = TextEdit::new("next note"); s.a.focused = true; });
    rt.frame(0.0);
    assert_eq!(focused(&rt).as_deref(), Some("a"));
    assert!(rt.ime_reset, "the platform input method resets for the new note");
    send(&mut rt, reset_commits());
    assert_eq!(rt.state.a.text, "next note", "the old note's commit stays out of the new note");
    assert!(rt.state.changed.is_empty());
}

#[test]
fn a_cancelled_composition_releases_cleanly() {
    let mut rt = composing_in_a();
    click(&mut rt, 100.0, 60.0);
    send(&mut rt, reset_cancels());
    assert_eq!((rt.state.a.text.as_str(), rt.state.b.text.as_str()), ("", ""));
    assert!(rt.state.changed.is_empty() && rt.state.a.preedit.is_none());
    assert_b_composes_its_own(&mut rt);
}

#[test]
fn a_composition_the_platform_drops_silently_releases_at_the_next_one() {
    let mut rt = composing_in_a();
    click(&mut rt, 100.0, 60.0);
    assert_b_composes_its_own(&mut rt);
    assert_eq!(rt.state.a.text, "");
    assert_eq!(rt.state.changed, ["b"]);
}
