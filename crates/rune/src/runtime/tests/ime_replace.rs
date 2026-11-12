//! An input method that names the range its event replaces (macOS's `replacementRange`):
//! reconverting a typed word takes it into the composition, and an insert at a range writes over
//! what the range holds. The platform's text input client reads the field from the runtime.
use super::*;
use crate::platform::text_input::{Document, Replacement};
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
    let mut document = None;
    rt.sync_text_input(&mut document);
    document
}

#[test]
fn reconverting_a_selected_word_composes_it_again_in_its_place() {
    let mut rt = field("今日は晴れ", 0..6);
    let selected = document(&rt).and_then(|d| d.selected_range()).expect("the field's selection");
    assert_eq!(selected, 0..2);
    let replacing = document(&rt).and_then(|d| d.replacement(selected, true)).expect("the selected word");
    rt.ime_replace_from_platform(replacing);
    rt.ime(Ime::Preedit("きょう".into(), Some((9, 9))));
    rt.frame(0.0);
    assert_eq!((rt.state.text.as_str(), rt.state.preedit.as_ref().map(|(p, _)| p.as_str())), ("は晴れ", Some("きょう")));
    assert_eq!(document(&rt).and_then(|d| d.marked_range()), Some(0..3));
    rt.ime(Ime::Preedit(String::new(), None));
    rt.ime(Ime::Commit("京".into()));
    rt.frame(0.0);
    assert_eq!(rt.state.text, "京は晴れ");
    assert_eq!(rt.state.selection().range(), 3..3);
}

#[test]
fn an_insert_at_a_range_writes_over_what_the_range_holds() {
    let mut rt = field("say omw", 7..7);
    rt.ime_replace_from_platform(Replacement { bytes: 4..7, expected: "omw".into(), delete: false });
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
    rt.ime_replace_from_platform(Replacement { bytes: 4..7, expected: "own".into(), delete: true });
    rt.ime(Ime::Preedit("x".into(), None));
    rt.ime(Ime::Preedit(String::new(), None));
    rt.ime(Ime::Commit("x".into()));
    rt.frame(0.0);
    assert_eq!(rt.state.text, "say omwx");
}

#[test]
fn a_secret_field_shows_the_input_method_nothing() {
    let mut e = TextEdit::new("hunter2").secure();
    e.focused = true;
    let mut rt = Runtime::new(e, view, (400.0, 400.0), Env::new(true, 1.0));
    rt.frame(0.0);
    assert_eq!(document(&rt), None);
    rt.ime_replace_from_platform(Replacement { bytes: 0..7, expected: "hunter2".into(), delete: true });
    rt.frame(0.0);
    assert_eq!(rt.state.text, "hunter2");
}
