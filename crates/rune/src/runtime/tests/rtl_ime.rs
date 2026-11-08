//! Right-to-left compositions: the text an input method shows mid-composition lays out, draws
//! its underline and commits into the field wherever the caret sits, whatever runs around it.
use super::*;
use crate::{Block, Ime, Mark, MarkRange, TextEdit, TextStyle, text_input};

fn view(s: &TextEdit, _: &mut Cx<TextEdit>) -> El {
    col().w_full().child(text_input(s, TextStyle::new(15.5, 400.0).lh(25.0)).key("field").w_full().on_edit(|s: &mut TextEdit| s))
}

fn field(text: &str, caret: usize, width: f32) -> Runtime<TextEdit> {
    let mut e = TextEdit::new(text);
    e.focused = true;
    e.set_caret(caret);
    let mut rt = Runtime::new(e, view, (width, 400.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt.ime(Ime::Enabled);
    rt
}

/// A Hebrew composition ending in a carriage return, typed between a letter and its combining
/// accent, leaves the accent straight after a hard break in right-to-left text.
#[test]
fn review4_rtl_ime_repro() {
    let blocks = vec![Block::Code, Block::Quote, Block::Bullet(0), Block::Quote, Block::Bullet(2), Block::Bullet(2), Block::Quote, Block::Code, Block::Bullet(2), Block::Bullet(0), Block::Paragraph];
    let text = "a\nword\u{2028}e\u{301}\u{2028}漢שלום\rbb\u{2028}\r\n\u{2029}e\u{301} ";
    let mut e = TextEdit::from_parts(text, vec![MarkRange::new(0..1, Mark::Code), MarkRange::new(5..26, Mark::Bold)], blocks);
    e.focused = true;
    e.set_caret(10);
    let mut rt = Runtime::new(e, view, (148.0, 2000.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt.ime(Ime::Enabled);
    rt.ime(Ime::Preedit("שלום\r\n\r".into(), Some((0, 11))));
    rt.frame(0.0);
    assert!(rt.state.preedit.is_some(), "the field composes");
    rt.ime(Ime::Preedit(String::new(), None));
    rt.ime(Ime::Commit("שלום".into()));
    rt.frame(0.0);
    assert_eq!(rt.state.text, format!("{}שלום{}", &text[..10], &text[10..]));
}

#[test]
fn arabic_and_hebrew_compositions_commit_where_the_caret_is() {
    for (text, caret, word) in [
        ("abc def", 4, "مرحبا"),
        ("abc def", 4, "שלום"),
        ("שלום עולם", 9, "مرحبا"),
        ("مرحبا", "مرحبا".len(), "שלום"),
        ("", 0, "سلام"),
    ] {
        for width in [60.0, 400.0] {
            let mut rt = field(text, caret, width);
            let mut shown = String::new();
            for ch in word.chars() {
                shown.push(ch);
                rt.ime(Ime::Preedit(shown.clone(), Some((shown.len(), shown.len()))));
                rt.frame(0.0);
                assert_eq!(rt.state.preedit.as_ref().map(|(p, _)| p.as_str()), Some(shown.as_str()), "{word} composing into {text:?}");
            }
            rt.ime(Ime::Preedit(String::new(), None));
            rt.ime(Ime::Commit(word.into()));
            rt.frame(0.0);
            let want = format!("{}{word}{}", &text[..caret], &text[caret..]);
            assert_eq!(rt.state.text, want, "{word} committed into {text:?} at width {width}");
            assert_eq!((rt.state.anchor, rt.state.focus), (caret + word.len(), caret + word.len()), "the caret follows {word}");
            assert!(rt.state.preedit.is_none());
        }
    }
}
