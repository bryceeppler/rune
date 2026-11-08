use super::*;
use crate::{Key, KeyEvent, Mark, MarkRange, Mods, PhysicalKeyCode, PhysicalInput, TextEdit, TextStyle, text_input};

#[derive(Default)]
struct State { note: TextEdit, other: TextEdit, pressed: Vec<String>, keys: Vec<KeyEvent> }

fn view(s: &State, _: &mut Cx<State>) -> El {
    let style = TextStyle::new(16.0, 400.0);
    col().size(400.0, 200.0)
        .child(text_input(&s.note, style).key("note").size(400.0, 120.0)
            .on_edit(|s: &mut State| &mut s.note)
            .pill_style(|_| crate::PillStyle::default())
            .on_input_key(|s: &mut State, ev, _| { s.keys.push(ev.clone()); false })
            .on_pill_press(|s: &mut State, id, _| s.pressed.push(id.to_owned())))
        .child(text_input(&s.other, style).key("other").size(400.0, 40.0).on_edit(|s: &mut State| &mut s.other))
}

fn boxed() -> TextEdit {
    TextEdit::from_parts("box one\nbox two", vec![
        MarkRange::new(0..3, Mark::Pill("first".into())),
        MarkRange::new(8..11, Mark::Pill("second".into())),
    ], Vec::new())
}

fn runtime() -> Runtime<State> {
    let mut rt = Runtime::new(State { note: boxed(), ..State::default() }, view, (400.0, 200.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt
}

fn chip(rt: &Runtime<State>, id: &str) -> (f32, f32) {
    let i = rt.nodes.iter().position(|n| n.path.ends_with("note") && n.input.is_some()).expect("the field");
    let [x0, y0, x1, y1] = rt.nodes[i].target;
    let found = (0..400).flat_map(|x| (0..20).map(move |y| (x as f32, y as f32 * 6.0)))
        .map(|(x, y)| (x0 + x, y0 + y))
        .find(|&(x, y)| x < x1 && y < y1 && rt.pill_at(i, x, y).as_deref() == Some(id));
    found.expect("the pill's chip is on screen")
}

fn click(rt: &mut Runtime<State>, (x, y): (f32, f32)) {
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
}

#[test]
fn a_click_on_a_pill_chip_presses_it_and_leaves_the_caret_and_focus_alone() {
    let mut rt = runtime();
    rt.update(|s, _| { s.other.focused = true; });
    rt.frame(0.0);
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
    let i = rt.nodes.iter().position(|n| n.path.ends_with("note") && n.input.is_some()).expect("the field");
    let [_, _, x1, _] = rt.nodes[i].target;
    click(&mut rt, (x1 - 4.0, y));
    assert!(rt.state.pressed.is_empty());
    assert!(rt.state.note.focused, "a click on the text focuses the field");
    assert_eq!(rt.state.note.focus, 7, "and places the caret at the end of the line");
}

#[test]
fn shift_or_option_with_v_reaches_the_field_as_a_key_and_only_plain_v_pastes() {
    let mut rt = runtime();
    rt.update(|s, _| { s.note.focused = true; });
    rt.frame(0.0);
    for mods in [Mods { cmd: true, shift: true, ..Mods::default() }, Mods { cmd: true, shift: true, alt: true, ..Mods::default() }] {
        rt.physical_with_fallback(PhysicalInput::Key { key: PhysicalKeyCode::KeyV, mods, pressed: true, repeat: false },
            Some(KeyEvent { key: Key::Command('v'), mods }));
        rt.frame(0.0);
        assert!(rt.paste_request.is_none(), "{mods:?} is not a paste");
    }
    assert_eq!(rt.state.keys.len(), 2, "both reached the field's key handler");
    let mods = Mods { cmd: true, ..Mods::default() };
    rt.physical_with_fallback(PhysicalInput::Key { key: PhysicalKeyCode::KeyV, mods, pressed: true, repeat: false },
        Some(KeyEvent { key: Key::Command('v'), mods }));
    rt.frame(0.0);
    assert!(rt.paste_request.is_some(), "a plain primary V pastes");
}
