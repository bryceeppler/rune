//! A sized pill drawn after bytes that stay text: a photo under its Markdown reference while the
//! caret edits it, beside the same photo drawn in place of the reference.
use accesskit::Role;

use crate::render::KIND_IMAGE;
use crate::{Caret, Color, Cx, El, Env, Key, KeyEvent, Mark, MarkRange, Mods, PillStyle, Pixels, Runtime, Selection, TextEdit, TextStyle, col, el, text_input};

const REFERENCE: &str = "![](nokto-photo:3f9a1c7e5b2d48a6c0e19f7b3a5d2c8e4f6a0b1c9d7e5f3a2b4c6d8e0f1a3b5c)";
const LINE: f32 = 22.0;
const MARGIN: f32 = 10.0;
const RING: Color = Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };

struct S {
    edit: TextEdit,
    photo: Pixels,
    after: bool,
    pressed: Vec<String>,
}

fn view(s: &S, cx: &mut Cx<S>) -> El {
    let photo = s.photo.clone();
    let at = s.edit.text.find(REFERENCE);
    let field = text_input(&s.edit, TextStyle::new(15.0, 400.0).lh(LINE))
        .key("note")
        .w(500.0)
        .wrap_anywhere()
        .pill_style(move |_| PillStyle::new().sized([640.0, 400.0]).image(&photo).description("Lake at dusk").margin(MARGIN, MARGIN).selection_ring(RING, 3.0, 2.0))
        .pills_after(at.filter(|_| s.after).map(|at| (at..at + REFERENCE.len(), "photo".to_owned())))
        .on_edit(|s: &mut S| &mut s.edit)
        .on_pill_press(|s: &mut S, id, _| s.pressed.push(id.to_owned()));
    let bar = cx.pill_rect("note", "photo").map(|r| el().key("bar").size(90.0, 34.0).pin(Some(r[0] + r[2] - 100.0), Some(r[1] + 10.0), None, None));
    col().child(el().child(field).children(bar))
}

/// "Pack\n", the reference, then "\nAfter", the reference a pill drawn in its place unless the
/// photo draws `after` it; the caret at the reference's end.
fn runtime(after: bool) -> (Runtime<S>, usize, usize) {
    let text = format!("Pack\n{REFERENCE}\nAfter");
    let at = text.find(REFERENCE).unwrap();
    let end = at + REFERENCE.len();
    let marks = if after { Vec::new() } else { vec![MarkRange::new(at..end, Mark::Pill("photo".into()))] };
    let mut edit = TextEdit::from_parts(text, marks, Vec::new());
    edit.focused = true;
    edit.set_caret(Caret::at(end));
    let photo = Pixels::new();
    photo.set(64, 40, (0..64 * 40).flat_map(|_| [90, 120, 160, 255]).collect());
    let mut rt = Runtime::new(S { edit, photo, after, pressed: Vec::new() }, view, (600.0, 900.0), Env::new(true, 2.0));
    settle(&mut rt);
    (rt, at, end)
}

fn settle(rt: &mut Runtime<S>) {
    for _ in 0..30 {
        if !rt.frame(1.0 / 60.0) {
            break;
        }
    }
}

fn key(rt: &mut Runtime<S>, key: Key) {
    rt.key(KeyEvent { key, mods: Mods::default() });
    settle(rt);
}

fn caret(rt: &Runtime<S>) -> [f32; 4] {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some("note")).unwrap().caret.unwrap()
}

fn image(rt: &Runtime<S>) -> Option<[f32; 4]> {
    rt.instances.iter().find(|i| i.params[1] == KIND_IMAGE).map(|i| i.rect.map(|v| v / 2.0))
}

fn caret_at(rt: &mut Runtime<S>, byte: usize) -> [f32; 4] {
    rt.update(move |s, _| s.edit.set_caret(Caret::at(byte)));
    settle(rt);
    caret(rt)
}

fn byte(rt: &Runtime<S>) -> usize {
    rt.state.edit.selection().focus.byte
}

#[test]
fn a_box_after_text_draws_under_it_at_the_size_a_pill_box_has_and_keeps_its_room_below() {
    let (mut inline, _, end) = runtime(false);
    let (mut after, at, _) = runtime(true);
    let (boxed, photo) = (image(&inline).expect("the pill's box draws"), image(&after).expect("the box after the text draws"));
    assert!((photo[2] - boxed[2]).abs() < 0.5 && (photo[3] - boxed[3]).abs() < 0.5, "at the same size: {photo:?} {boxed:?}");
    assert!((photo[0] - boxed[0]).abs() < 0.5, "from the same left edge: {photo:?} {boxed:?}");
    let reference = [caret_at(&mut after, at), caret_at(&mut after, end)];
    assert!(reference[1][1] > reference[0][1] + LINE * 0.5, "the reference wraps onto a second line: {reference:?}");
    let text_foot = reference[1][1] + reference[1][3];
    assert!((photo[1] - (text_foot + MARGIN)).abs() < 1.0, "the box starts its margin under the text's last line: {photo:?} {text_foot}");
    let below = |rt: &mut Runtime<S>, photo: [f32; 4]| caret_at(rt, rt.state.edit.text.len())[1] - (photo[1] + photo[3]);
    let (inline_below, after_below) = (below(&mut inline, boxed), below(&mut after, photo));
    assert!((after_below - inline_below).abs() < 0.5, "the next line stands as far under the box as under the pill's: {after_below} {inline_below}");
    assert!(after_below >= MARGIN - 0.5, "at least the bottom margin: {after_below}");
}

#[test]
fn the_caret_edits_every_byte_before_the_box_and_never_draws_beside_it() {
    let (mut rt, at, end) = runtime(true);
    let photo = image(&rt).unwrap();
    let c = caret(&rt);
    assert!((c[3] - LINE).abs() < 1.0, "the caret at the text's end is text height: {c:?}");
    assert!(c[1] + c[3] <= photo[1], "and stands on the text's last line, above the box: {c:?} {photo:?}");
    let mut stops = vec![byte(&rt)];
    while byte(&rt) > at {
        key(&mut rt, Key::Left);
        stops.push(byte(&rt));
    }
    assert_eq!(stops, (at..=end).rev().collect::<Vec<_>>(), "Left stops at every byte of the text");
    for b in at..=end {
        let c = caret_at(&mut rt, b);
        assert!(c[1] + c[3] <= photo[1] && (c[3] - LINE).abs() < 1.0, "no caret beside the box, at byte {b}: {c:?}");
    }
    rt.update(move |s, _| s.edit.set_caret(Caret::at(end - 1)));
    key(&mut rt, Key::Text("x".into()));
    assert!(rt.state.edit.text.contains(&format!("{}x)", &REFERENCE[..REFERENCE.len() - 1])), "typing edits the text");
}

#[test]
fn up_and_down_step_over_the_box_line() {
    let (mut rt, at, end) = runtime(true);
    key(&mut rt, Key::Down);
    assert!(byte(&rt) > end, "Down from the text's last line goes to the line under the box: {}", byte(&rt));
    key(&mut rt, Key::Up);
    assert!((at..=end).contains(&byte(&rt)), "Up from there comes back to the text: {}", byte(&rt));
    rt.update(move |s, _| s.edit.set_caret(Caret::at(end)));
    key(&mut rt, Key::Right);
    assert_eq!(byte(&rt), end + 1, "Right from the text's end goes to the next line");
}

#[test]
fn a_press_on_the_box_fires_on_pill_press_and_pill_rect_reads_it() {
    let (mut rt, _, end) = runtime(true);
    let photo = image(&rt).unwrap();
    let (x, y) = (photo[0] + photo[2] * 0.5, photo[1] + photo[3] * 0.5);
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(&mut rt);
    assert_eq!(rt.state.pressed, ["photo"]);
    assert_eq!(byte(&rt), end, "the press leaves the caret where it was");
    let bar = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("bar")).expect("the bar shows").rect;
    assert!((bar[0] + bar[2] - (photo[0] + photo[2] - 10.0)).abs() < 0.5 && (bar[1] - (photo[1] + 10.0)).abs() < 0.5, "placed from the box's rect: {bar:?} {photo:?}");
}

#[test]
fn a_selection_covering_the_text_rings_its_box_and_assistive_technology_reads_the_text() {
    let (mut rt, at, end) = runtime(true);
    let ringed = |rt: &Runtime<S>| rt.instances.iter().any(|i| i.border[0] > 0.9 && i.border[1] < 0.1 && i.border[3] > 0.9);
    assert!(!ringed(&rt), "no ring while the caret stands in the text");
    rt.update(move |s, _| s.edit.set_selection(Selection::bytes(at + 1, end)));
    settle(&mut rt);
    assert!(!ringed(&rt), "nor while part of it is selected");
    rt.update(move |s, _| s.edit.set_selection(Selection::bytes(at, end)));
    settle(&mut rt);
    assert!(ringed(&rt), "a selection of the whole text rings the box");
    let tree = rt.accessibility_tree();
    let field = &tree.nodes.iter().find(|(_, n)| n.author_id() == Some("note")).unwrap().1;
    assert_eq!(field.value(), Some(format!("Pack\n{REFERENCE}\nAfter").as_str()), "the text reads as text");
    let images: Vec<_> = tree.nodes.iter().filter(|(_, n)| n.role() == Role::Image).map(|(_, n)| n).collect();
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].label(), Some("Lake at dusk"));
}
