use super::*;
use crate::edit::{Block, RichStyle};
use crate::{TextEdit, TextStyle, text_input};

const BAR: &str = "#e6339a";

fn view(s: &TextEdit, _: &mut Cx<TextEdit>) -> El {
    let style = RichStyle { quote_bar: crate::color::hex(BAR).unwrap(), ..Default::default() };
    col().size(600.0, 900.0).child(text_input(s, TextStyle::new(24.0, 400.0).lh(36.0)).rich_style(style).key("note").w_full().on_edit(|s: &mut TextEdit| s))
}

fn note(paras: &[(&str, Block)]) -> Runtime<TextEdit> {
    let text = std::iter::once("Notes").chain(paras.iter().map(|(p, _)| *p)).collect::<Vec<_>>().join("\n");
    let blocks = std::iter::once(Block::Paragraph).chain(paras.iter().map(|(_, b)| *b)).collect();
    let mut edit = TextEdit::from_parts(text, vec![], blocks);
    edit.focused = true;
    let mut rt = Runtime::new(edit, view, (600.0, 900.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt
}

fn start_x(rt: &mut Runtime<TextEdit>) -> f32 {
    rt.update(|s, _| s.set_caret(crate::Caret::at("Notes\n".len())));
    rt.frame(0.0);
    let i = rt.nodes.iter().position(|n| n.key.as_deref() == Some("note")).unwrap();
    rt.caret_rect(&rt.nodes, i).unwrap()[0]
}

#[derive(Debug, Clone, Copy)]
struct Bar {
    left: f32,
    right: f32,
    height: f32,
}

fn bars(rt: &Runtime<TextEdit>) -> Vec<Bar> {
    let bar = crate::color::hex(BAR).unwrap();
    rt.instances
        .iter()
        .filter(|i| i.fill == bar.premul(1.0) || i.fill == bar.premul_srgb(1.0))
        .map(|i| Bar { left: i.rect[0], right: i.rect[0] + i.rect[2], height: i.rect[3] })
        .collect()
}

#[test]
fn a_right_to_left_quote_draws_its_bar_on_the_right_and_steps_in_from_it_as_a_left_to_right_one_does_on_the_left() {
    let (mut ltr_plain, mut rtl_plain) = (note(&[("hello world", Block::Paragraph)]), note(&[("שלום עולם", Block::Paragraph)]));
    let (left, right) = (start_x(&mut ltr_plain), start_x(&mut rtl_plain));
    for (kind, block) in [
        ("plain", Block::Quote),
        ("bullet", Block::Bullet { level: 2, quote: true }),
        ("checklist", Block::Hanging { marker: 6, level: 2, quote: true }),
    ] {
        let mut ltr = note(&[("hello world", block), ("again", Block::Quote)]);
        let mut rtl = note(&[("שלום עולם", block), ("שוב", Block::Quote)]);
        let (&[ltr_bar], &[rtl_bar]) = (&bars(&ltr)[..], &bars(&rtl)[..]) else {
            panic!("{kind}: one bar each: {:?} {:?}", bars(&ltr), bars(&rtl))
        };
        assert_eq!(rtl_bar.height, ltr_bar.height, "{kind}: each bar runs down both paragraphs");
        let (ltr_start, rtl_start) = (start_x(&mut ltr), start_x(&mut rtl));
        assert!(ltr_bar.right < ltr_start, "{kind}: the left-to-right bar is left of its text");
        assert!(rtl_bar.left > rtl_start, "{kind}: the right-to-left bar is right of its text: {rtl_bar:?} from {rtl_start}");
        assert_eq!(right - rtl_bar.right, ltr_bar.left - left, "{kind}: each bar stands as far in from its margin");
        assert_eq!(right - rtl_start, ltr_start - left, "{kind}: each text steps in as far from its margin");
    }
}

#[test]
fn a_quote_whose_paragraphs_read_both_ways_draws_each_bar_on_its_own_side() {
    let mut rt = note(&[("hello world", Block::Quote), ("שלום עולם", Block::Quote), ("שוב", Block::Bullet { level: 0, quote: true }), ("again", Block::Quote)]);
    let edges = bars(&rt);
    let [ltr, rtl, ltr_again] = &edges[..] else { panic!("a bar for each run of one direction: {edges:?}") };
    let right = start_x(&mut note(&[("שלום עולם", Block::Paragraph)]));
    assert!(ltr.right < start_x(&mut rt), "the left-to-right lines' bar is on the left");
    assert!(rtl.left > right - 10.0 && rtl.right <= right, "the right-to-left lines' bar is on the right: {rtl:?} by {right}");
    assert_eq!((ltr.left, ltr.height), (ltr_again.left, ltr_again.height), "the last line's bar matches the first's");
    assert_eq!(rtl.height, ltr.height + 36.0, "the right-to-left bar runs one line further, down both its lines: {edges:?}");
}

#[test]
fn a_line_with_no_letters_keeps_its_quotes_bar_on_the_side_of_the_lines_around_it() {
    let right = start_x(&mut note(&[("שלום עולם", Block::Paragraph)]));
    let three_lines = 3.0 * 36.0 - 4.0;
    let mut split = Vec::new();
    for (kind, block, prefix) in [
        ("plain", Block::Quote, ""),
        ("bullet", Block::Bullet { level: 1, quote: true }, ""),
        ("checklist", Block::Hanging { marker: 2, level: 1, quote: true }, "- "),
    ] {
        for neutral in ["", "123", "(1) - ..."] {
            let words = "שלום עולם";
            for (place, lines) in [("between", [words, neutral, words]), ("first", [neutral, words, words]), ("last", [words, words, neutral])] {
                let texts = lines.map(|line| format!("{prefix}{line}"));
                let edges = bars(&note(&texts.each_ref().map(|text| (text.as_str(), block))));
                let whole = matches!(&edges[..], &[bar] if bar.left > right - 10.0 && bar.right <= right && bar.height == three_lines);
                if !whole {
                    split.push(format!("{kind}, {neutral:?} {place}: {edges:?}"));
                }
            }
        }
    }
    assert!(split.is_empty(), "each quote draws one bar on the right, by {right}, down all three lines, not:\n{}", split.join("\n"));
    let edges = bars(&note(&[("", Block::Quote), ("123", Block::Quote)]));
    let ltr = bars(&note(&[("hello world", Block::Quote)]));
    let (&[bar], &[ltr]) = (&edges[..], &ltr[..]) else { panic!("one bar each: {edges:?} {ltr:?}") };
    assert_eq!((bar.left, bar.right), (ltr.left, ltr.right), "a quote with no letters draws its bar where a left-to-right one does");
}
