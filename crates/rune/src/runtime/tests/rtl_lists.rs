use super::*;
use crate::edit::Block;
use crate::{Key, KeyEvent, Mods, TextEdit, TextStyle, text_input};

const NONE: Mods = Mods { shift: false, cmd: false, alt: false, ctrl: false, logo: false };

fn view(s: &TextEdit, _: &mut Cx<TextEdit>) -> El {
    col().size(600.0, 600.0).child(text_input(s, TextStyle::new(24.0, 400.0).lh(36.0)).key("note").w_full().on_edit(|s: &mut TextEdit| s))
}

/// A focused field of `Notes`, `para` laid out as `block`, and `end`; and where `para` starts.
fn note(para: &str, block: Block) -> (Runtime<TextEdit>, usize) {
    let mut edit = TextEdit::from_parts(format!("Notes\n{para}\nend"), vec![], vec![Block::Paragraph, block, Block::Paragraph]);
    edit.focused = true;
    let mut rt = Runtime::new(edit, view, (600.0, 600.0), Env::new(true, 1.0));
    rt.frame(0.0);
    (rt, "Notes\n".len())
}

fn put_caret(rt: &mut Runtime<TextEdit>, at: usize) {
    rt.update(|s, _| s.set_caret(crate::Caret::at(at)));
    rt.frame(0.0);
}

fn press(rt: &mut Runtime<TextEdit>, key: Key) {
    rt.key(KeyEvent { key, mods: NONE });
    rt.frame(0.0);
}

fn click(rt: &mut Runtime<TextEdit>, x: f32, y: f32) {
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
}

/// The caret as drawn: its byte, x, and the middle of its line.
fn caret(rt: &Runtime<TextEdit>) -> (usize, f32, f32) {
    let i = rt.nodes.iter().position(|n| n.key.as_deref() == Some("note")).unwrap();
    let r = rt.caret_rect(&rt.nodes, i).unwrap();
    (rt.state.selection().focus.byte, r[0], r[1] + r[3] / 2.0)
}

fn caret_at(rt: &mut Runtime<TextEdit>, at: usize) -> (f32, f32) {
    put_caret(rt, at);
    let (_, x, y) = caret(rt);
    (x, y)
}

/// The bullet dots drawn: small square rects, left edge and right edge.
fn dots(rt: &Runtime<TextEdit>) -> Vec<(f32, f32)> {
    rt.instances.iter()
        .filter(|q| q.params[1] == crate::render::KIND_RECT && q.rect[2] == q.rect[3] && q.rect[2] <= 8.0)
        .map(|q| (q.rect[0], q.rect[0] + q.rect[2]))
        .collect()
}

#[test]
fn a_right_to_left_bullet_indents_and_draws_its_dot_on_the_right_as_a_left_to_right_one_does_on_the_left() {
    let (mut ltr, at) = note("hello world", Block::Bullet(0));
    let (mut ltr_plain, _) = note("hello world", Block::Paragraph);
    let (mut rtl, _) = note("שלום עולם", Block::Bullet(0));
    let (mut rtl_plain, _) = note("שלום עולם", Block::Paragraph);
    let (left, right) = (caret_at(&mut ltr_plain, at).0, caret_at(&mut rtl_plain, at).0);
    let (ltr_start, rtl_start) = (caret_at(&mut ltr, at).0, caret_at(&mut rtl, at).0);
    assert!(ltr_start > left + 10.0, "the left-to-right item indents from the left: {left} to {ltr_start}");
    assert_eq!(right - rtl_start, ltr_start - left, "the right-to-left item indents as far from the right");
    let (&[ltr_dot], &[rtl_dot]) = (&dots(&ltr)[..], &dots(&rtl)[..]) else { panic!("one dot each: {:?} {:?}", dots(&ltr), dots(&rtl)) };
    assert!(ltr_dot.1 < ltr_start, "the left-to-right dot is left of its text");
    assert!(rtl_dot.0 > rtl_start && rtl_dot.1 < right, "the right-to-left dot is in the right gutter: {rtl_dot:?} between {rtl_start} and {right}");
    assert_eq!(rtl_dot.0 - rtl_start, ltr_start - ltr_dot.1, "each dot stands as far from its text");
}

#[test]
fn nested_right_to_left_bullets_step_in_from_the_right() {
    let (mut one, at) = note("שלום עולם", Block::Bullet(0));
    let (mut two, _) = note("שלום עולם", Block::Bullet(1));
    let (mut ltr_one, _) = note("hello world", Block::Bullet(0));
    let (mut ltr_two, _) = note("hello world", Block::Bullet(1));
    let step = caret_at(&mut one, at).0 - caret_at(&mut two, at).0;
    assert!(step > 10.0, "the nested item starts further left: {step}");
    assert_eq!(step, caret_at(&mut ltr_two, at).0 - caret_at(&mut ltr_one, at).0, "by the same step a left-to-right one moves right");
}

#[test]
fn a_right_to_left_numbered_item_draws_its_number_in_the_right_gutter() {
    let (mut rt, at) = note("שלום עולם", Block::Numbered(0));
    let (mut plain, _) = note("שלום עולם", Block::Paragraph);
    let (start, right) = (caret_at(&mut rt, at).0, caret_at(&mut plain, at).0);
    assert!(start < right - 10.0, "the item indents from the right");
    let (_, _, y) = caret(&rt);
    let in_gutter = rt.instances.iter()
        .filter(|q| q.params[1] == crate::render::KIND_GLYPH && (q.rect[1] + q.rect[3] / 2.0 - y).abs() < 18.0)
        .filter(|q| q.rect[0] > start && q.rect[0] + q.rect[2] <= right + 1.0)
        .count();
    assert_eq!(in_gutter, 2, "`.` and `1` draw between the text and the right margin");
}

#[test]
fn a_click_beside_a_right_to_left_line_lands_at_the_end_it_is_nearest() {
    let para = "שלום עולם";
    let (mut plain, at) = note(para, Block::Paragraph);
    let (right, _) = caret_at(&mut plain, at);
    for block in [Block::Bullet(0), Block::Paragraph] {
        let (mut rt, _) = note(para, block);
        let (end, _) = caret_at(&mut rt, at + para.len());
        let (_, y) = caret_at(&mut rt, at);
        put_caret(&mut rt, 0);
        click(&mut rt, right - 2.0, y);
        assert_eq!(caret(&rt).0, at, "{block:?}: a click between the text's start and the right margin lands at the start");
        put_caret(&mut rt, 0);
        click(&mut rt, end - 40.0, y);
        let (byte, x, line) = caret(&rt);
        assert_eq!(byte, at + para.len(), "{block:?}: a click left of the text lands at its end");
        assert_eq!((x, line), (end, y), "{block:?}: and draws there, on the same line");
    }
}

#[test]
fn arrows_cross_a_right_to_left_items_edges_as_they_draw() {
    let para = "שלום עולם";
    let (mut rt, at) = note(para, Block::Bullet(0));
    put_caret(&mut rt, at);
    let (_, start_x, y) = caret(&rt);
    press(&mut rt, Key::Left);
    let (byte, x, _) = caret(&rt);
    assert_eq!(byte, at + "ש".len(), "Left from the item's start moves into its first letter");
    assert!(x < start_x);
    put_caret(&mut rt, at);
    press(&mut rt, Key::Right);
    let (byte, _, line) = caret(&rt);
    assert_eq!(byte, at - 1, "Right past the item's right edge goes back to the paragraph before");
    assert!(line < y);
    put_caret(&mut rt, at + para.len());
    press(&mut rt, Key::Left);
    let (byte, _, line) = caret(&rt);
    assert_eq!(byte, at + para.len() + 1, "Left past the item's left edge goes on to the next paragraph");
    assert!(line > y);
}
