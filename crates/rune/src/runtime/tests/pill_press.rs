use super::*;
use crate::{Ime, Key, KeyEvent, Mark, MarkRange, Mods, PhysicalKeyCode, PhysicalInput, TextEdit, TextStyle, text_input};

#[derive(Default)]
struct State { note: TextEdit, other: TextEdit, pressed: Vec<String>, keys: Vec<KeyEvent>, takes_shift_v: bool, read_only: bool }

fn view(s: &State, _: &mut Cx<State>) -> El {
    let style = TextStyle::new(16.0, 400.0);
    let note = text_input(&s.note, style).key("note").size(400.0, 120.0)
        .pill_style(|_| crate::PillStyle::default())
        .on_input_key(|s: &mut State, ev, _| {
            s.keys.push(ev.clone());
            s.takes_shift_v && ev.key == Key::Command('v') && ev.mods.shift
        })
        .on_pill_press(|s: &mut State, id, _| {
            s.pressed.push(id.to_owned());
            let (anchor, focus) = (s.note.anchor, s.note.focus);
            let range = s.note.pills().find(|(_, pill)| *pill == id).map(|(range, _)| range).expect("the pressed pill");
            s.note.replace_with_pill(range, id, "[x]");
            s.note.set_selection(anchor, focus);
        });
    let note = if s.read_only { note } else { note.on_edit(|s: &mut State| &mut s.note) };
    col().size(400.0, 200.0)
        .child(note)
        .child(text_input(&s.other, style).key("other").size(400.0, 40.0).on_edit(|s: &mut State| &mut s.other))
}

fn boxed() -> TextEdit {
    TextEdit::from_parts("box one\nbox two", vec![
        MarkRange::new(0..3, Mark::Pill("first".into())),
        MarkRange::new(8..11, Mark::Pill("second".into())),
    ], Vec::new())
}

fn runtime() -> Runtime<State> {
    runtime_with(State::default())
}

fn runtime_with(state: State) -> Runtime<State> {
    let mut rt = Runtime::new(State { note: boxed(), other: TextEdit::new("other"), ..state }, view, (400.0, 200.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt
}

fn note_index(rt: &Runtime<State>) -> usize {
    rt.nodes.iter().position(|n| n.path.ends_with("note") && n.input.is_some()).expect("the field")
}

fn chip(rt: &Runtime<State>, id: &str) -> (f32, f32) {
    let i = note_index(rt);
    let [x0, y0, x1, y1] = rt.nodes[i].target;
    let found = (0..400).flat_map(|x| (0..20).map(move |y| (x as f32, y as f32 * 6.0)))
        .map(|(x, y)| (x0 + x, y0 + y))
        .find(|&(x, y)| x < x1 && y < y1 && rt.chip_under(i, x, y).as_deref() == Some(id));
    found.expect("the pill's chip is on screen")
}

fn click(rt: &mut Runtime<State>, (x, y): (f32, f32)) {
    rt.pointer_down(x, y);
    rt.frame(0.0);
    rt.pointer_up(x, y);
    rt.frame(0.0);
}

fn focus(rt: &mut Runtime<State>, note: bool) {
    rt.update(move |s, _| {
        let field = if note { &mut s.note } else { &mut s.other };
        field.focused = true;
        field.set_caret(field.text.len());
    });
    rt.frame(0.0);
}

#[test]
fn a_click_on_a_pill_chip_presses_it_and_leaves_the_caret_and_focus_alone() {
    let mut rt = runtime();
    focus(&mut rt, false);
    let at = chip(&rt, "second");
    click(&mut rt, at);
    assert_eq!(rt.state.pressed, ["second"]);
    assert!(rt.state.other.focused && !rt.state.note.focused, "the other field keeps the focus");
    rt.update(|s, _| { s.other.focused = false; s.note.focused = true; s.note.set_selection(5, 5); });
    rt.frame(0.0);
    let at = chip(&rt, "first");
    click(&mut rt, at);
    assert_eq!(rt.state.pressed, ["second", "first"]);
    assert_eq!((rt.state.note.anchor, rt.state.note.focus), (5, 5), "the caret stays where it was");
    assert!(rt.state.note.focused);
}

#[test]
fn a_press_that_leaves_its_chip_presses_nothing_and_a_click_off_the_chips_places_the_caret() {
    let mut rt = runtime();
    let (x, y) = chip(&rt, "first");
    rt.pointer_down(x, y);
    rt.pointer_move(x, y + 80.0);
    rt.pointer_up(x, y + 80.0);
    rt.frame(0.0);
    assert!(rt.state.pressed.is_empty(), "released off the chip");
    let i = note_index(&rt);
    let [_, _, x1, _] = rt.nodes[i].target;
    click(&mut rt, (x1 - 4.0, y));
    assert!(rt.state.pressed.is_empty());
    assert!(rt.state.note.focused, "a click on the text focuses the field");
    assert_eq!(rt.state.note.focus, 7, "and places the caret at the end of the line");
}

#[test]
fn a_chip_press_commits_its_fields_open_composition_before_the_handler_edits_the_field() {
    let mut rt = runtime();
    focus(&mut rt, true);
    rt.ime(Ime::Preedit("か".into(), Some((0, 3))));
    rt.frame(0.0);
    let ended = rt.ime_session();
    let at = chip(&rt, "first");
    click(&mut rt, at);
    rt.ime_in(ended, Ime::Preedit(String::new(), None));
    rt.ime_in(ended, Ime::Disabled);
    rt.frame(0.0);
    assert_eq!(rt.state.pressed, ["first"]);
    assert_eq!(rt.state.note.text, "[x] one\nbox twoか", "the composition shown at the click is kept");
    assert_eq!(rt.state.other.text, "other");
}

#[test]
fn a_composition_in_another_field_stays_open_through_a_chip_press_and_commits_there() {
    let mut rt = runtime();
    focus(&mut rt, false);
    rt.ime(Ime::Preedit("か".into(), Some((0, 3))));
    rt.frame(0.0);
    let at = chip(&rt, "first");
    click(&mut rt, at);
    rt.ime(Ime::Preedit(String::new(), None));
    rt.ime(Ime::Commit("家".into()));
    rt.frame(0.0);
    assert_eq!(rt.state.note.text, "[x] one\nbox two");
    assert_eq!(rt.state.other.text, "other家");
}

#[test]
fn a_read_only_fields_chip_shows_no_hand_presses_nothing_and_keeps_the_focus() {
    let mut rt = runtime_with(State { read_only: true, ..State::default() });
    focus(&mut rt, false);
    let at = chip(&rt, "first");
    rt.pointer_move(at.0, at.1);
    assert_ne!(rt.cursor(), crate::el::Cursor::Pointer, "no pointing hand over a chip nothing can press");
    click(&mut rt, at);
    assert!(rt.state.pressed.is_empty());
    assert_eq!(rt.state.note.text, "box one\nbox two");
    assert!(rt.state.other.focused, "the focused field keeps the focus");
}

fn shift_v(rt: &mut Runtime<State>, alt: bool) {
    let mods = Mods { cmd: true, ctrl: !cfg!(target_os = "macos"), shift: true, alt, ..Mods::default() };
    rt.physical_with_fallback(PhysicalInput::Key { key: PhysicalKeyCode::KeyV, mods, pressed: true, repeat: false },
        Some(KeyEvent { key: Key::Command('v'), mods }));
    rt.frame(0.0);
}

#[test]
fn shifted_primary_v_reaches_a_handler_that_takes_it_instead_of_pasting() {
    let mut rt = runtime_with(State { takes_shift_v: true, ..State::default() });
    focus(&mut rt, true);
    shift_v(&mut rt, false);
    shift_v(&mut rt, true);
    assert!(rt.paste_request.is_none(), "the field's own Paste as Plain Text took it");
    assert_eq!(rt.state.keys.len(), 2);
}

#[test]
fn shifted_primary_v_still_pastes_when_no_handler_takes_it() {
    let mut rt = runtime();
    focus(&mut rt, true);
    shift_v(&mut rt, false);
    assert!(rt.paste_request.take().is_some(), "an app that binds no Shift-V keeps pasting with it");
    rt.key(KeyEvent { key: Key::Command('v'), mods: Mods { cmd: true, shift: true, ..Mods::default() } });
    rt.frame(0.0);
    assert!(rt.paste_request.is_some(), "so does a key that reaches the window without a physical code");
}

#[test]
fn a_plain_primary_v_pastes_before_any_handler_hears_it() {
    let mut rt = runtime_with(State { takes_shift_v: true, ..State::default() });
    focus(&mut rt, true);
    let mods = Mods { cmd: true, ctrl: !cfg!(target_os = "macos"), ..Mods::default() };
    rt.physical_with_fallback(PhysicalInput::Key { key: PhysicalKeyCode::KeyV, mods, pressed: true, repeat: false },
        Some(KeyEvent { key: Key::Command('v'), mods }));
    rt.frame(0.0);
    assert!(rt.paste_request.is_some());
    assert!(rt.state.keys.is_empty());
}
