use super::*;
use crate::platform::text_input::{Client, Document, Field, Replacement};
use crate::{Ime, TextEdit, TextStyle, text_input};

fn view(s: &TextEdit, _: &mut Cx<TextEdit>) -> El {
    col().w_full().child(text_input(s, TextStyle::new(15.5, 400.0).lh(25.0)).key("field").w_full().on_edit(|s: &mut TextEdit| s))
}

fn field(text: &str, selection: std::ops::Range<usize>) -> Runtime<TextEdit> {
    let mut e = TextEdit::new(text);
    e.focused = true;
    e.set_selection(crate::Selection::bytes(selection.start, selection.end));
    let mut rt = Runtime::new(e, view, (400.0, 400.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt.ime(Ime::Enabled);
    rt
}

fn document(rt: &Runtime<TextEdit>) -> Option<Document> {
    let mut client = Client::default();
    rt.sync_text_input(&mut client);
    client.document
}

#[test]
fn reconverting_a_selected_word_composes_it_again_in_its_place() {
    let mut rt = field("今日は晴れ", 0..6);
    let selected = document(&rt).and_then(|d| d.selected_range()).expect("the field's selection");
    assert_eq!(selected, 0..2);
    let replacing = document(&rt).and_then(|d| d.replacement(selected)).expect("the selected word");
    rt.ime_replace_from_platform(replacing);
    rt.ime(Ime::Preedit("きょう".into(), Some((9, 9))));
    rt.frame(0.0);
    assert_eq!((rt.state.text.as_str(), rt.state.preedit.as_ref().map(|(p, _)| p.as_str())), ("今日は晴れ", Some("きょう")));
    assert_eq!(rt.state.selection().range(), 0..6, "the composition stands in for the word it reconverts");
    assert_eq!(document(&rt).and_then(|d| d.marked_range()), Some(0..3));
    assert_eq!(document(&rt).and_then(|d| d.substring(0..9)), Some(("きょうは晴れ".to_owned(), 0..6)));
    rt.ime(Ime::Preedit(String::new(), None));
    rt.ime(Ime::Commit("京".into()));
    rt.frame(0.0);
    assert_eq!(rt.state.text, "京は晴れ");
    assert_eq!(rt.state.selection().range(), 3..3);
}

#[test]
fn an_insert_at_a_range_writes_over_what_the_range_holds() {
    let mut rt = field("say omw", 7..7);
    let replacing = document(&rt).and_then(|d| d.replacement(4..7)).expect("the shortcut");
    rt.ime_replace_from_platform(replacing);
    rt.ime(Ime::Preedit("On my way!".into(), None));
    rt.ime(Ime::Preedit(String::new(), None));
    rt.ime(Ime::Commit("On my way!".into()));
    rt.frame(0.0);
    assert_eq!(rt.state.text, "say On my way!");
    assert_eq!(rt.state.selection().range(), 14..14);
    assert_eq!(document(&rt).and_then(|d| d.substring(0..100)), Some(("say On my way!".to_owned(), 0..14)));
}

#[test]
fn a_range_whose_text_changed_since_the_input_method_read_it_is_left_alone() {
    let mut rt = field("say omw", 7..7);
    let mut replacing = document(&rt).and_then(|d| d.replacement(4..7)).expect("the shortcut");
    replacing.expected = "own".into();
    rt.ime_replace_from_platform(replacing);
    rt.ime(Ime::Preedit("x".into(), None));
    rt.ime(Ime::Preedit(String::new(), None));
    rt.ime(Ime::Commit("x".into()));
    rt.frame(0.0);
    assert_eq!(rt.state.text, "say omw", "the composition that came with the stale range is refused with it");
}

#[test]
fn a_secret_field_shows_the_input_method_nothing() {
    let mut e = TextEdit::new("hunter2").secure();
    e.focused = true;
    let mut rt = Runtime::new(e, view, (400.0, 400.0), Env::new(true, 1.0));
    rt.frame(0.0);
    assert!(document(&rt).is_none());
    let field = Field { model: std::sync::Weak::new(), text: crate::platform::text_input::digest("hunter2") };
    rt.ime_replace_from_platform(Replacement { bytes: 0..7, expected: "hunter2".into(), field });
    rt.frame(0.0);
    assert_eq!(rt.state.text, "hunter2");
}

#[test]
fn the_first_rect_of_a_range_is_its_first_line_and_covers_only_that_line() {
    let mut rt = field("first line\nsecond line", 0..0);
    rt.frame(0.0);
    let (first, covered) = rt.text_input_rect(6..17).expect("the field's geometry");
    assert_eq!(covered, 6..11, "from the range's start to the end of its first line");
    let (second, covered) = rt.text_input_rect(11..17).expect("the field's geometry");
    assert_eq!(covered, 11..17);
    assert!(second[1] >= first[1] + first[3] * 0.9, "the second line lies below the first: {first:?} {second:?}");
    let (word, _) = rt.text_input_rect(6..10).expect("the field's geometry");
    assert!(word[2] > 0.0 && word[2] < first[2] + 1.0 && (word[0] - first[0]).abs() < 0.5, "{word:?} {first:?}");
    let (caret, covered) = rt.text_input_rect(3..3).expect("the field's geometry");
    assert_eq!(covered, 3..3);
    assert!(caret[3] > 0.0);
}

fn reconverting(text: &str, caret: usize, units: std::ops::Range<usize>, composing: &str, cursor: Option<(usize, usize)>) -> Runtime<TextEdit> {
    let mut rt = field(text, caret..caret);
    assert!(rt.ime_replacing(units), "the field has the range to give");
    rt.ime(Ime::Preedit(composing.into(), cursor));
    rt.frame(0.0);
    rt
}

#[test]
fn a_reconversion_from_a_caret_composes_in_the_word_s_place_and_leaves_the_caret_alone() {
    let rt = reconverting("今日は晴れ", 6, 0..2, "きょう", Some((9, 9)));
    assert_eq!(rt.state.display().0, "きょうは晴れ");
    assert_eq!(rt.state.selection().range(), 6..6, "the person's caret is not the input method's to move");
    let document = document(&rt).expect("the field's text");
    assert_eq!((document.marked_range(), document.selected_range()), (Some(0..3), Some(3..3)));
    assert_eq!(document.substring(0..9), Some(("きょうは晴れ".to_owned(), 0..6)));
}

#[test]
fn cancelling_a_reconversion_gives_back_the_caret_the_person_had() {
    let mut rt = reconverting("今日は晴れ", 6, 0..2, "きょう", None);
    rt.ime(Ime::Preedit(String::new(), None));
    rt.frame(0.0);
    assert_eq!((rt.state.text.as_str(), rt.state.display().0.as_str()), ("今日は晴れ", "今日は晴れ"));
    assert_eq!(rt.state.selection().range(), 6..6);
    assert_eq!(document(&rt).and_then(|d| d.selected_range()), Some(2..2));
    rt.key(crate::KeyEvent { key: crate::Key::Text("x".into()), mods: Default::default() });
    rt.frame(0.0);
    assert_eq!(rt.state.text, "今日xは晴れ", "the next key types at the caret and keeps the word");
}

#[test]
fn a_composition_after_a_cancelled_reconversion_goes_in_at_the_caret() {
    let mut rt = reconverting("今日は晴れ", 6, 0..2, "きょう", None);
    rt.ime(Ime::Preedit(String::new(), None));
    rt.frame(0.0);
    rt.ime(Ime::Preedit("か".into(), None));
    rt.ime(Ime::Preedit(String::new(), None));
    rt.ime(Ime::Commit("か".into()));
    rt.frame(0.0);
    assert_eq!(rt.state.text, "今日かは晴れ");
}

#[test]
fn copy_during_a_reconversion_copies_only_what_the_person_selected() {
    let mut rt = reconverting("今日は晴れ", 6, 0..2, "きょう", Some((0, 9)));
    let out = rt.state.key(&crate::KeyEvent { key: crate::Key::Command('c'), mods: crate::Mods { cmd: true, ..Default::default() } }, None);
    assert_eq!(out.copy.as_deref(), Some(""), "the person's caret selects nothing, and the word under the composition is not theirs");
}

#[test]
fn a_reconversion_from_a_caret_commits_over_its_word_as_one_undo_step() {
    let mut rt = reconverting("今日は晴れ", 6, 0..2, "きょう", None);
    rt.ime(Ime::Preedit(String::new(), None));
    rt.ime(Ime::Commit("京".into()));
    rt.frame(0.0);
    assert_eq!((rt.state.text.as_str(), rt.state.selection().range()), ("京は晴れ", 3..3));
    rt.update(|edit, _| edit.undo());
    rt.frame(0.0);
    assert_eq!((rt.state.text.as_str(), rt.state.selection().range()), ("今日は晴れ", 6..6));
}

#[test]
fn a_replacement_read_from_another_field_model_writes_nothing_anywhere() {
    let mut rt = field("say omw", 7..7);
    let replacing = document(&rt).and_then(|d| d.replacement(4..7)).expect("the shortcut");
    rt.update(|edit, _| {
        *edit = TextEdit::new("new omw");
        edit.focused = true;
        edit.set_selection(crate::Selection::bytes(7, 7));
    });
    rt.frame(0.0);
    rt.ime_replace_from_platform(replacing);
    rt.ime(Ime::Preedit("On my way!".into(), None));
    rt.ime(Ime::Preedit(String::new(), None));
    rt.ime(Ime::Commit("On my way!".into()));
    rt.frame(0.0);
    assert_eq!((rt.state.text.as_str(), rt.state.preedit.as_ref()), ("new omw", None), "the whole replacement is refused, its text too");
    assert!(rt.ime_reset, "the input method forgets the composition Rune refused");
}

#[test]
fn a_replacement_whose_text_changed_writes_nothing_anywhere() {
    let mut rt = field("say omw", 7..7);
    let replacing = document(&rt).and_then(|d| d.replacement(4..7)).expect("the shortcut");
    rt.update(|edit, _| edit.text.replace_range(0..3, "omw"));
    rt.frame(0.0);
    rt.ime_replace_from_platform(replacing);
    rt.ime(Ime::Preedit("On my way!".into(), None));
    rt.ime(Ime::Preedit(String::new(), None));
    rt.ime(Ime::Commit("On my way!".into()));
    rt.frame(0.0);
    assert_eq!(rt.state.text, "omw omw");
    rt.ime(Ime::Preedit("x".into(), None));
    rt.ime(Ime::Preedit(String::new(), None));
    rt.ime(Ime::Commit("x".into()));
    rt.frame(0.0);
    assert_eq!(rt.state.text, "omw omwx", "the next composition, in the session after, types at the caret");
}

#[test]
fn the_first_rect_refuses_a_layout_of_other_text_of_the_same_length() {
    let rt = field("WWW rest", 0..3);
    let mut client = Client::default();
    rt.sync_text_input(&mut client);
    let document = client.document.as_mut().expect("the field's text");
    let replacing = document.replacement(0..3).expect("the selected word");
    document.mark("iii", 0..3, Some(&replacing));
    let stale = client.geometry.as_ref().expect("the field's geometry").first_rect(document, 0..3);
    assert!(stale.is_none(), "the layout shows WWW, not the iii the input method just marked: {stale:?}");
}

#[test]
fn the_first_rect_of_an_insertion_point_has_no_width() {
    let rt = field("hello", 3..3);
    let (rect, covered) = rt.text_input_rect(3..3).expect("the field's geometry");
    assert_eq!(covered, 3..3);
    assert_eq!(rect[2], 0.0, "AppKit asks for a zero-width rectangle at an insertion point");
    assert!(rect[3] > 0.0);
    let (word, _) = rt.text_input_rect(0..3).expect("the field's geometry");
    assert!((rect[0] - (word[0] + word[2])).abs() < 0.5, "at the caret, after hel: {rect:?} {word:?}");
}

#[test]
fn the_first_rect_of_part_of_a_character_covers_the_whole_character() {
    let rt = field("e\u{301}cole", 0..0);
    let (whole, _) = rt.text_input_rect(0..2).expect("the field's geometry");
    let (base, covered) = rt.text_input_rect(0..1).expect("the field's geometry");
    assert_eq!(covered, 0..2, "a range inside e and its combining accent widens to the whole character");
    assert_eq!(base, whole);
    let (accent, covered) = rt.text_input_rect(1..2).expect("the field's geometry");
    assert_eq!((accent, covered), (whole, 0..2));
}

#[test]
fn the_first_rect_at_a_caret_clicked_to_the_end_of_a_wrapped_line_stays_on_that_line() {
    let text = "word ".repeat(60);
    let mut rt = field(&text, 0..0);
    rt.pointer_down(399.0, 12.5);
    rt.pointer_up(399.0, 12.5);
    rt.frame(0.0);
    let byte = rt.state.selection().focus.byte;
    let at = text[..byte].encode_utf16().count();
    let caret = rt.ime_area.expect("the drawn caret");
    assert_eq!(caret[1], 0.0, "the click put the caret at the end of the first line");
    let (rect, covered) = rt.text_input_rect(at..at).expect("the field's geometry");
    assert_eq!(covered, at..at);
    assert!((rect[0] - caret[0]).abs() < 0.5 && (rect[1] - caret[1]).abs() < 0.5, "the insertion point is where the caret is drawn: {rect:?} vs {caret:?}");
}

#[test]
fn the_first_rect_of_a_selection_from_a_right_to_left_line_end_is_that_line_s_break() {
    let text = "אבג\nمرحبا";
    let mut rt = field(text, 0..0);
    rt.pointer_down(372.0, 12.5);
    rt.frame(0.0);
    rt.pointer_move(360.0, 37.5);
    rt.frame(0.0);
    rt.pointer_up(360.0, 37.5);
    rt.frame(0.0);
    let selected = document(&rt).expect("the field's text").selected_range().expect("a selection");
    assert_eq!(selected, 3..9, "the drag selected the first line's break and the whole second line");
    let (first_line, _) = rt.text_input_rect(0..4).expect("the field's geometry");
    let (rect, covered) = rt.text_input_rect(selected).expect("the field's geometry");
    assert_eq!(covered, 3..4, "the first fragment is the first line's break");
    assert!((rect[1] - first_line[1]).abs() < 0.5, "on the first line: {rect:?} vs {first_line:?}");
}

#[test]
fn the_first_rect_of_any_character_covers_it_even_when_it_draws_nothing() {
    for text in ["abc\nxyz", "abc\r\nxyz", "\n", "\n\n", "אבג\nمرحبا", "a\u{200b}b", "a\u{200e}b", "a\u{200f}b", "\u{200d}", "\u{feff}"] {
        let rt = field(text, 0..0);
        let mut units = 0;
        for ch in text.chars() {
            let end = units + ch.len_utf16();
            let (_, covered) = rt.text_input_rect(units..end).expect("the field's geometry");
            assert!(covered.start <= units && covered.end >= end, "{text:?} {units}..{end} covered {covered:?}");
            units = end;
        }
    }
}
