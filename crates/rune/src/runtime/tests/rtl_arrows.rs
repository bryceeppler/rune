use super::*;
use crate::{Key, KeyEvent, Mods, TextEdit, TextStyle, text_input};

const NONE: Mods = Mods { shift: false, cmd: false, alt: false, ctrl: false, logo: false };
const WORD: Mods = Mods { alt: true, ..NONE };
const SHIFT: Mods = Mods { shift: true, ..NONE };

fn view<const W: u16>(s: &TextEdit, _: &mut Cx<TextEdit>) -> El {
    col().size(W as f32, 600.0).child(text_input(s, TextStyle::new(24.0, 400.0).lh(36.0)).key("note").w_full().on_edit(|s: &mut TextEdit| s))
}

/// A note of three paragraphs, `para` between two English ones, in a field `W` points wide.
fn note<const W: u16>(para: &str) -> (Runtime<TextEdit>, usize) {
    let mut edit = TextEdit::new(format!("Notes\n{para}\nend"));
    edit.focused = true;
    let mut rt = Runtime::new(edit, view::<W>, (W as f32, 600.0), Env::new(true, 1.0));
    rt.frame(0.0);
    (rt, "Notes\n".len())
}

fn put_caret(rt: &mut Runtime<TextEdit>, at: usize) {
    rt.update(|s, _| s.set_caret(at));
    rt.frame(0.0);
}

fn press(rt: &mut Runtime<TextEdit>, key: Key, mods: Mods) {
    rt.key(KeyEvent { key, mods });
    rt.frame(0.0);
}

/// The caret as drawn: its byte, x and line top.
fn caret(rt: &Runtime<TextEdit>) -> (usize, f32, f32) {
    let i = rt.nodes.iter().position(|n| n.key.as_deref() == Some("note")).unwrap();
    let r = rt.caret_rect(&rt.nodes, i).unwrap();
    (rt.state.focus, r[0], r[1])
}

/// Presses `key` from where the caret is until it leaves its line: the carets it drew on the
/// line, the first included, and the one it left the line to.
fn walk_line(rt: &mut Runtime<TextEdit>, key: Key) -> (Vec<(usize, f32)>, (usize, f32, f32)) {
    let (at, x, y) = caret(rt);
    let mut line = vec![(at, x)];
    for _ in 0..100 {
        press(rt, key.clone(), NONE);
        let (at, x, top) = caret(rt);
        if top != y {
            return (line, (at, x, top));
        }
        line.push((at, x));
    }
    panic!("{key:?} never left the line: {line:?}");
}

fn assert_moves(line: &[(usize, f32)], left: bool, stops: usize) {
    for w in line.windows(2) {
        assert!(if left { w[1].1 < w[0].1 } else { w[1].1 > w[0].1 }, "each press moves the caret {}: {line:?}", if left { "left" } else { "right" });
    }
    assert_eq!(line.len(), stops, "one stop between each two characters, and the ends: {line:?}");
}

#[test]
fn arrows_move_through_a_right_to_left_paragraph_as_it_draws() {
    let para = "שלום עולם";
    let (mut rt, start) = note::<600>(para);
    let stops = para.chars().count() + 1;
    put_caret(&mut rt, start);
    let (line, next) = walk_line(&mut rt, Key::Left);
    assert_moves(&line, true, stops);
    assert_eq!(line.last().unwrap().0, start + para.len(), "Left ends at the paragraph's end, on its left");
    assert_eq!(next.0, start + para.len() + 1, "Left past the left edge goes on to the next paragraph");
    put_caret(&mut rt, start + para.len());
    let (line, previous) = walk_line(&mut rt, Key::Right);
    assert_moves(&line, false, stops);
    assert_eq!(line.last().unwrap().0, start);
    assert_eq!(previous.0, start - 1, "Right past the right edge goes back to the paragraph before");
}

#[test]
fn left_goes_on_from_a_wrapped_right_to_left_line_to_the_next_lines_right_edge() {
    let para = "שלום עולם גדול מאוד וארוך עוד מילים כאן";
    let (mut rt, start) = note::<300>(para);
    put_caret(&mut rt, start);
    let (first, (at, x, _)) = walk_line(&mut rt, Key::Left);
    assert_moves(&first, true, first.len());
    assert!(at > first.last().unwrap().0 && at < start + para.len(), "Left wraps to the paragraph's next line, at {at}");
    assert!(x > first[0].1 - 1.0, "and lands at its right edge, x {x}");
    let (back, (at, x, _)) = walk_line(&mut rt, Key::Right);
    assert_eq!(back.len(), 1, "Right at a line's right edge leaves it: {back:?}");
    assert_eq!((at, x), *first.last().unwrap(), "back to where Left left the line before");
}

#[test]
fn left_crosses_an_english_word_inside_right_to_left_text() {
    let para = "שלום Rune עולם!";
    let (mut rt, start) = note::<600>(para);
    let stops = para.chars().count() + 1;
    put_caret(&mut rt, start);
    let (line, _) = walk_line(&mut rt, Key::Left);
    assert_moves(&line, true, stops);
    put_caret(&mut rt, start + para.len());
    let (line, _) = walk_line(&mut rt, Key::Right);
    assert_moves(&line, false, stops);
}

#[test]
fn right_crosses_a_hebrew_phrase_inside_english_text() {
    let para = "abc אבג דהו xyz";
    let (mut rt, start) = note::<600>(para);
    let stops = para.chars().count() + 1;
    put_caret(&mut rt, start);
    let (line, _) = walk_line(&mut rt, Key::Right);
    assert_moves(&line, false, stops);
    put_caret(&mut rt, start + para.len());
    let (line, _) = walk_line(&mut rt, Key::Left);
    assert_moves(&line, true, stops);
}

#[test]
fn the_caret_stops_on_both_sides_of_a_direction_change() {
    let para = "abc אבג";
    let (mut rt, start) = note::<600>(para);
    let boundary = start + "abc ".len();
    put_caret(&mut rt, start + "abc".len());
    press(&mut rt, Key::Right, NONE);
    let (at, after_space, _) = caret(&rt);
    put_caret(&mut rt, start);
    put_caret(&mut rt, boundary);
    let (_, right_end, _) = caret(&rt);
    assert_eq!(at, boundary, "Right steps over the space");
    assert!(after_space < right_end, "and draws just after it, not at the Hebrew's far end: {after_space} vs {right_end}");
    put_caret(&mut rt, start + "abc".len());
    press(&mut rt, Key::Right, NONE);
    press(&mut rt, Key::Right, NONE);
    let (at, x, _) = caret(&rt);
    assert_eq!(at, start + "abc אב".len(), "the next Right steps over the Hebrew's leftmost letter");
    assert!(x > after_space && x < right_end);
    press(&mut rt, Key::Left, NONE);
    assert_eq!(caret(&rt).1, after_space, "Left comes back to the same place");
    put_caret(&mut rt, start + "abc".len());
    press(&mut rt, Key::Right, NONE);
    press(&mut rt, Key::Text("x".into()), NONE);
    assert_eq!(rt.state.text, "Notes\nabc xאבג\nend", "typing after the space inserts there");
}

#[test]
fn shift_arrows_select_toward_the_pressed_side() {
    let para = "שלום";
    let (mut rt, start) = note::<600>(para);
    let at = start + "של".len();
    put_caret(&mut rt, at);
    let (_, x, _) = caret(&rt);
    press(&mut rt, Key::Left, SHIFT);
    assert_eq!(rt.state.selected(), "ו", "Shift+Left selects the letter drawn to the left");
    assert!(caret(&rt).1 < x);
    press(&mut rt, Key::Left, NONE);
    assert_eq!((rt.state.anchor, rt.state.focus), (at + "ו".len(), at + "ו".len()), "Left collapses the selection to its left end");
    put_caret(&mut rt, at);
    press(&mut rt, Key::Right, SHIFT);
    assert_eq!(rt.state.selected(), "ל", "Shift+Right selects the letter drawn to the right");
    assert!(caret(&rt).1 > x);
    press(&mut rt, Key::Right, NONE);
    assert_eq!((rt.state.anchor, rt.state.focus), (start + "ש".len(), start + "ש".len()), "Right collapses it to its right end");
}

#[test]
fn word_steps_move_as_the_text_draws() {
    let para = "שלום Rune עולם";
    let (mut rt, start) = note::<600>(para);
    put_caret(&mut rt, start);
    let mut stops = vec![caret(&rt)];
    for key in [Key::Left, Key::Left, Key::Left, Key::Right, Key::Right, Key::Right] {
        press(&mut rt, key, WORD);
        stops.push(caret(&rt));
    }
    let bytes: Vec<usize> = stops.iter().map(|s| s.0 - start).collect();
    let words = |s: &str| s.len();
    assert_eq!(
        bytes,
        [0, words("שלום"), words("שלום "), para.len(), words("שלום Rune "), words("שלום Rune"), 0],
        "Alt+Left stops at each word's left edge and Alt+Right at each one's right edge: {stops:?}"
    );
    assert!(stops[..4].windows(2).all(|w| w[1].1 < w[0].1), "Alt+Left moves left: {stops:?}");
    assert!(stops[3..].windows(2).all(|w| w[1].1 > w[0].1), "Alt+Right moves right: {stops:?}");
}

/// The caret after Right from the end of `abc` in `abc אבג`: on the space's far side, where the
/// Hebrew's first letter would draw it at the line's far right.
fn after_the_space(para: &str) -> (Runtime<TextEdit>, usize, (usize, f32, f32)) {
    let (mut rt, start) = note::<600>(para);
    put_caret(&mut rt, start + "abc".len());
    press(&mut rt, Key::Right, NONE);
    let at = caret(&rt);
    (rt, start, at)
}

#[test]
fn the_caret_keeps_its_side_when_its_selection_is_set_again() {
    let (mut rt, start, at) = after_the_space("abc אבג");
    rt.update(|s, _| s.set_selection(s.anchor, s.focus));
    rt.frame(0.0);
    assert_eq!(caret(&rt), at, "setting the same selection, as an app's refresh does, keeps the caret where it was");
    press(&mut rt, Key::Right, NONE);
    assert_eq!(caret(&rt).0, start + "abc אב".len(), "and the next Right steps over one letter");
    rt.update(|s, _| s.set_caret(s.focus - "ב".len()));
    rt.frame(0.0);
    rt.update(|s, _| s.set_caret(start + "abc ".len()));
    rt.frame(0.0);
    assert!(caret(&rt).1 > at.1, "a caret set elsewhere and back takes its default side");
}

#[test]
fn a_plain_arrow_collapses_a_selection_to_its_end_drawn_that_way() {
    let para = "abc אבג xyz";
    let (mut rt, start) = note::<600>(para);
    for (from, key) in [("abc אב", Key::Right), ("abc א", Key::Left)] {
        put_caret(&mut rt, start + from.len());
        press(&mut rt, key.clone(), SHIFT);
        let selected = caret(&rt);
        assert!(!rt.state.selected().is_empty());
        press(&mut rt, key.clone(), NONE);
        assert_eq!(caret(&rt), selected, "{key:?} on a selection inside the Hebrew collapses it to its end drawn {key:?}");
        assert_eq!(rt.state.anchor, rt.state.focus);
    }
    let para = "שלום עולם גדול מאוד וארוך עוד מילים כאן";
    let (mut rt, start) = note::<300>(para);
    let (a, b) = (start + "של".len(), start + para.len() - "אן".len());
    for (key, want) in [(Key::Right, a), (Key::Left, b)] {
        rt.update(|s, _| s.set_selection(a, b));
        rt.frame(0.0);
        press(&mut rt, key.clone(), NONE);
        assert_eq!((rt.state.anchor, rt.state.focus), (want, want), "{key:?} on a selection over lines of Hebrew collapses it toward the side it reads from");
    }
}

#[test]
fn up_and_down_start_from_the_caret_where_it_draws() {
    let digits = "12345678901234567890";
    let (mut rt, start) = note::<600>(&format!("{digits}\nabc אבג\n{digits}"));
    let mixed = start + digits.len() + 1;
    for key in [Key::Down, Key::Up] {
        put_caret(&mut rt, mixed + "abc".len());
        press(&mut rt, Key::Right, NONE);
        let at = caret(&rt);
        press(&mut rt, key.clone(), NONE);
        let to = caret(&rt);
        assert!(to.2 != at.2 && (to.1 - at.1).abs() < 8.0, "{key:?} keeps the column the caret draws in: {at:?} to {to:?}");
    }
}

#[test]
fn a_press_at_a_direction_change_draws_the_caret_where_it_was_pressed() {
    let (mut rt, start, at) = after_the_space("abc אבג");
    put_caret(&mut rt, start);
    let line = rt.caret_rect(&rt.nodes, rt.nodes.iter().position(|n| n.key.as_deref() == Some("note")).unwrap()).unwrap()[3];
    rt.pointer_down(at.1, at.2 + line * 0.5);
    rt.pointer_up(at.1, at.2 + line * 0.5);
    rt.frame(1.0);
    let pressed = caret(&rt);
    assert_eq!(pressed.0, start + "abc ".len());
    assert!((pressed.1 - at.1).abs() < 1.0, "the caret draws at the press, not at the Hebrew's far end: {pressed:?} for {at:?}");
}

#[test]
fn a_lines_ends_on_screen_are_its_drawn_left_and_right() {
    let para = "שלום עולם גדול מאוד וארוך עוד מילים כאן";
    let (rt, start) = note::<300>(para);
    let i = rt.nodes.iter().position(|n| n.key.as_deref() == Some("note")).unwrap();
    let l = rt.nodes[i].text_layout.clone().unwrap();
    let first = l.line_side(start, Affinity::Downstream, false);
    assert_eq!(first, (start, Affinity::Downstream), "the right end of a Hebrew line is where it starts");
    let (end, side) = l.line_side(start, Affinity::Downstream, true);
    let (left, right) = (l.caret_box(end, side, 1.0), l.caret_box(start, Affinity::Downstream, 1.0));
    assert_eq!(left.y0, right.y0, "the left end is on the same line");
    assert!(end > start && left.x0 < 20.0, "and is where the line's text ends, at its left: {end} at {}", left.x0);
}

#[test]
#[cfg_attr(not(target_os = "macos"), ignore = "Cmd+Left and Cmd+Right move to a line's ends only on macOS")]
fn cmd_arrows_go_to_the_ends_of_a_line_as_it_draws() {
    let para = "שלום עולם";
    let (mut rt, start) = note::<600>(para);
    put_caret(&mut rt, start + "של".len());
    press(&mut rt, Key::Left, Mods { cmd: true, ..NONE });
    assert_eq!(rt.state.focus, start + para.len(), "Cmd+Left goes to the Hebrew line's left end");
    press(&mut rt, Key::Right, Mods { cmd: true, ..NONE });
    assert_eq!(rt.state.focus, start, "Cmd+Right goes to its right end");
}
