use super::*;
use crate::{Block, Ime, Key, KeyEvent, Mods, Span, TextEdit, TextStyle, text_input};

const STYLE: TextStyle = TextStyle::new(15.5, 400.0).lh(25.0);

fn note_text() -> String {
    (0..30)
        .map(|i| match i % 3 {
            0 => format!("Paragraph {i}, long enough that it wraps onto a second line in a field this narrow, and then a third."),
            1 => format!("Short {i}"),
            _ => String::new(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn view(s: &TextEdit, _: &mut Cx<TextEdit>) -> El {
    col().size(420.0, 2400.0).child(text_input(s, STYLE).key("note").w_full().on_edit(|s: &mut TextEdit| s))
}

fn note() -> Runtime<TextEdit> {
    let mut edit = TextEdit::new(note_text());
    edit.focused = true;
    let mut rt = Runtime::new(edit, view, (420.0, 2400.0), Env::new(true, 2.0));
    rt.frame(0.0);
    rt
}

fn node(rt: &Runtime<TextEdit>) -> &Node {
    rt.nodes.iter().find(|n| n.key.as_deref() == Some("note")).unwrap()
}

/// The field's layout, and one layout of the whole of its text at the same width.
fn layouts(rt: &mut Runtime<TextEdit>) -> (TextLayout, Rc<Layout<Ink>>) {
    let n = node(rt);
    let (spec, layout) = (n.text.as_deref().unwrap().clone(), n.text_layout.clone().unwrap());
    let w = (n.local.w - n.inset[0] - n.inset[2]) * rt.env.scale;
    let whole = rt.text.layout_blocks_inner(&spec.content, &spec.spans, &spec.blocks, &spec.pills, spec.style, rt.env.scale, Some(w), parley::Alignment::Start, crate::text::Wrapping::Words, true);
    (layout, whole)
}

/// The field shows its text in stacked paragraphs whose lines and carets are one layout's.
fn assert_laid_out_whole(rt: &mut Runtime<TextEdit>) {
    let (layout, whole) = layouts(rt);
    assert!(layout.pieces().count() > 1, "the note stacks by paragraph");
    assert_eq!(layout.len(), whole.len());
    for (line, w) in layout.lines().zip(whole.lines()) {
        assert_eq!((line.text_range(), line.metrics()), (w.text_range(), *w.metrics()));
    }
    let caret = rt.state.focus;
    let cur = parley::editing::Cursor::from_byte_index(&whole, caret, Affinity::Downstream);
    assert_eq!(layout.caret_box(caret, 3.0), crate::text::cursor_geometry(&cur, &whole, 3.0), "caret at {caret}");
}

fn press(rt: &mut Runtime<TextEdit>, key: Key, mods: Mods) {
    rt.key(KeyEvent { key, mods });
    rt.frame(0.0);
}

fn type_text(rt: &mut Runtime<TextEdit>, s: &str) {
    for c in s.chars() {
        press(rt, Key::Text(c.to_string()), Mods::default());
    }
}

const SHIFT: Mods = Mods { shift: true, alt: false, ctrl: false, cmd: false, logo: false };

/// Typing, Shift+Return, Backspace over a paragraph break and typing over a selection that spans
/// paragraphs each reshape no more than the paragraphs they touch, and leave a layout that is
/// one layout's, line for line.
#[test]
fn edits_across_paragraph_breaks_lay_out_as_one_layout() {
    let mut rt = note();
    let at = rt.state.text.find("Short 4").unwrap() + "Short 4".len();
    rt.update(|s, _| s.set_caret(at));
    rt.frame(0.0);
    let shaped = rt.text.shaped;
    type_text(&mut rt, " more");
    assert_eq!(rt.text.shaped - shaped, 5, "each keystroke shapes its paragraph");
    assert_laid_out_whole(&mut rt);

    let shaped = rt.text.shaped;
    press(&mut rt, Key::Enter, SHIFT);
    assert!(rt.state.text.contains("Short 4 more\n\n"));
    assert!(rt.text.shaped - shaped <= 2, "Return shapes the two halves");
    assert_laid_out_whole(&mut rt);

    press(&mut rt, Key::Backspace, Mods::default());
    assert!(rt.state.text.contains("Short 4 more\n\nParagraph 6"), "Backspace took the new paragraph back");
    assert_laid_out_whole(&mut rt);
    let next = rt.state.text.find("Paragraph 6").unwrap();
    rt.update(|s, _| s.set_caret(next));
    rt.frame(0.0);
    let shaped = rt.text.shaped;
    press(&mut rt, Key::Backspace, Mods::default());
    press(&mut rt, Key::Backspace, Mods::default());
    assert!(rt.state.text.contains("Short 4 moreParagraph 6"), "Backspace joined the paragraphs");
    assert_eq!(rt.text.shaped - shaped, 1, "joining the empty paragraph reshapes nothing, the next join its paragraph");
    assert_laid_out_whole(&mut rt);

    let (a, b) = (rt.state.text.find("Short 7").unwrap() + 2, rt.state.text.find("Paragraph 9").unwrap() + 4);
    rt.update(|s, _| s.set_selection(a, b));
    rt.frame(0.0);
    let shaped = rt.text.shaped;
    type_text(&mut rt, "X");
    assert!(rt.state.text.contains("ShXgraph 9"), "typing replaced the selection across paragraphs");
    assert_eq!(rt.text.shaped - shaped, 1, "the joined paragraph shapes once");
    assert_laid_out_whole(&mut rt);
}

/// Shift+Down from the middle of a paragraph selects across the paragraphs below as one layout
/// moves, and the highlight covers the same boxes.
#[test]
fn a_selection_extends_across_paragraphs() {
    let mut rt = note();
    let start = rt.state.text.find("Paragraph 3").unwrap() + 20;
    rt.update(|s, _| s.set_caret(start));
    rt.frame(0.0);
    let (_, whole) = layouts(&mut rt);
    let mut want = start;
    for _ in 0..5 {
        press(&mut rt, Key::Down, SHIFT);
        want = crate::text::move_line(&whole, want, true).unwrap();
        assert_eq!((rt.state.anchor, rt.state.focus), (start, want));
    }
    assert!(rt.state.selected().matches('\n').count() >= 2, "the selection spans paragraphs");
    let (layout, whole) = layouts(&mut rt);
    assert_eq!(layout.range_boxes(start, want, true), crate::text::range_boxes(&whole, start, want, true));
    for _ in 0..5 {
        press(&mut rt, Key::Up, SHIFT);
        want = crate::text::move_line(&whole, want, false).unwrap();
        assert_eq!((rt.state.anchor, rt.state.focus), (start, want));
    }
}

/// A press anywhere in the field, between paragraphs or past a line's end included, puts the
/// caret where one layout's hit test does.
#[test]
fn a_press_in_a_later_paragraph_puts_the_caret_where_one_layout_does() {
    let mut rt = note();
    let (_, whole) = layouts(&mut rt);
    let n = node(&rt);
    let (ox, oy) = (n.target[0] + n.inset[0], n.target[1] + n.inset[1]);
    let s = rt.env.scale;
    for (k, &(x, y)) in [(30.0, 200.0), (300.0, 262.5), (5.0, 520.0), (410.0, 37.5), (120.0, 812.5), (60.0, 970.0)].iter().enumerate() {
        rt.pointer_down(ox + x, oy + y);
        rt.pointer_up(ox + x, oy + y);
        rt.frame(1.0 + k as f64);
        let at = crate::text::cursor_from_point(&whole, x * s, y * s).index();
        assert_eq!(rt.state.focus, at, "press at ({x}, {y})");
    }
}

/// A composition in the middle of a paragraph lays out with the paragraphs around it untouched,
/// its caret where one layout of the shown text puts it, and commits into that paragraph.
#[test]
fn an_ime_composition_inside_a_paragraph() {
    let mut rt = note();
    let at = rt.state.text.find("Paragraph 6").unwrap() + 4;
    rt.update(|s, _| s.set_caret(at));
    rt.frame(0.0);
    let shaped = rt.text.shaped;
    rt.ime(Ime::Enabled);
    rt.ime(Ime::Preedit("かな".into(), Some((6, 6))));
    rt.frame(0.0);
    assert_eq!(rt.text.shaped - shaped, 1, "only the composing paragraph shapes");
    let (layout, whole) = layouts(&mut rt);
    assert!(layout.pieces().count() > 1);
    let shown = node(&rt).text.as_deref().unwrap().content.clone();
    assert!(shown.contains("Paraかなgraph 6"));
    let caret = at + 6;
    let cur = parley::editing::Cursor::from_byte_index(&whole, caret, Affinity::Downstream);
    assert_eq!(layout.caret_box(caret, 3.0), crate::text::cursor_geometry(&cur, &whole, 3.0));
    assert_eq!(layout.range_boxes(at, at + 6, false), crate::text::range_boxes(&whole, at, at + 6, false), "the composition's underline");
    rt.ime(Ime::Commit("かな".into()));
    rt.frame(0.0);
    assert!(rt.state.text.contains("Paraかなgraph 6"));
    assert_eq!(rt.state.focus, at + 6);
    assert_laid_out_whole(&mut rt);
}

/// Undo and redo after typing in one paragraph put back texts whose paragraphs are all still
/// shaped, so they reshape nothing.
#[test]
fn undo_and_redo_reshape_nothing() {
    let mut rt = note();
    let before = rt.state.text.clone();
    let at = rt.state.text.find("Short 10").unwrap();
    rt.update(|s, _| s.set_caret(at));
    rt.frame(0.0);
    type_text(&mut rt, "word ");
    press(&mut rt, Key::Enter, SHIFT);
    let typed = rt.state.text.clone();
    let shaped = rt.text.shaped;
    let cmd = Mods { cmd: true, ..Mods::default() };
    for _ in 0..4 {
        if rt.state.text != before { press(&mut rt, Key::Command('z'), cmd); }
    }
    assert_eq!(rt.state.text, before);
    assert_eq!(rt.text.shaped, shaped, "undo found every paragraph shaped");
    assert_laid_out_whole(&mut rt);
    let shaped = rt.text.shaped;
    for _ in 0..4 {
        if rt.state.text != typed { press(&mut rt, Key::Command('z'), Mods { shift: true, ..cmd }); }
    }
    assert_eq!(rt.state.text, typed);
    assert_eq!(rt.text.shaped, shaped, "redo found every paragraph shaped");
    assert_laid_out_whole(&mut rt);
}

/// A composition holding line breaks lays out as one layout of the text it shows: each break
/// Parley takes moves the lines after it to the next block, also a `\r\n` that the code
/// block's spans split in two.
#[test]
fn a_composition_with_line_breaks_takes_one_layouts_blocks() {
    let mut differ = Vec::new();
    for pre in ["a\nb", "a\r\nb", "a\rb", "a\u{2028}b", "\r\n", "\r", "a\r\n\rb"] {
        for at in [0, 2, 5, 9] {
            let mut edit = TextEdit::from_parts("aa\nbb\nccc", vec![], vec![Block::Quote, Block::Bullet(1), Block::Code]);
            edit.focused = true;
            edit.set_caret(at);
            let mut rt = Runtime::new(edit, view, (420.0, 2400.0), Env::new(true, 2.0));
            rt.frame(0.0);
            rt.ime(Ime::Enabled);
            rt.ime(Ime::Preedit(pre.into(), Some((0, pre.len()))));
            rt.frame(0.0);
            let (layout, whole) = layouts(&mut rt);
            if layout.lines().map(|l| (l.text_range(), l.metrics())).ne(whole.lines().map(|w| (w.text_range(), *w.metrics()))) {
                differ.push((pre, at));
            }
        }
    }
    assert!(differ.is_empty(), "compositions laid out apart from one layout: {differ:?}");
}

/// Monospace tabs after a `\r`, U+2028 or U+2029 fill to the stops they reach in one layout
/// of the text, which counts their columns from the last `\n`.
#[test]
fn monospace_tabs_after_any_line_break_stop_where_one_layout_stops() {
    fn view(s: &TextEdit, _: &mut Cx<TextEdit>) -> El {
        col().size(420.0, 2400.0).child(crate::rich(s.text.clone(), vec![Span::new(0, s.text.len()).mono()]).font(STYLE).key("tabs").w_full())
    }
    let mut differ = Vec::new();
    for prefix in ["", "a", "ab", "abc"] {
        for sep in ["\n", "\r\n", "\r", "\u{2028}", "\u{2029}"] {
            let mut rt = Runtime::new(TextEdit::new(format!("{prefix}{sep}x\ty\nz")), view, (420.0, 2400.0), Env::new(true, 2.0));
            rt.frame(0.0);
            let n = rt.nodes.iter().find(|n| n.key.as_deref() == Some("tabs")).unwrap();
            let (spec, layout) = (n.text.as_deref().unwrap().clone(), n.text_layout.clone().unwrap());
            let w = (n.local.w - n.inset[0] - n.inset[2]) * rt.env.scale;
            let whole = rt.text.layout_rich(&spec.content, &spec.spans, spec.style, rt.env.scale, Some(w));
            if layout.lines().map(|l| l.metrics().advance).ne(whole.lines().map(|w| w.metrics().advance)) {
                differ.push((prefix, sep));
            }
        }
    }
    assert!(differ.is_empty(), "tabs that stop apart from one layout's: {differ:?}");
}
