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
    assert_eq!(rt.state.text, "say omwx");
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
fn reconverting_a_word_and_undoing_once_brings_the_original_word_back() {
    let mut rt = field("今日は晴れ", 6..6);
    let replacing = document(&rt).and_then(|d| d.replacement(0..2)).expect("the word before the caret");
    rt.ime_replace_from_platform(replacing);
    rt.ime(Ime::Preedit("きょう".into(), None));
    rt.frame(0.0);
    rt.ime(Ime::Preedit(String::new(), None));
    rt.ime(Ime::Commit("京".into()));
    rt.frame(0.0);
    assert_eq!(rt.state.text, "京は晴れ");
    rt.update(|edit, _| edit.undo());
    rt.frame(0.0);
    assert_eq!(rt.state.text, "今日は晴れ", "one undo takes the whole reconversion back");
}

#[test]
fn a_range_read_from_one_field_model_never_edits_another() {
    let mut rt = field("say omw", 7..7);
    let replacing = document(&rt).and_then(|d| d.replacement(4..7)).expect("the shortcut");
    rt.update(|edit, _| {
        *edit = TextEdit::new("new omw");
        edit.focused = true;
        edit.set_selection(crate::Selection::bytes(7, 7));
    });
    rt.frame(0.0);
    rt.ime_replace_from_platform(replacing);
    rt.frame(0.0);
    assert_eq!(rt.state.text, "new omw");
}

#[test]
fn a_range_read_before_the_field_changed_elsewhere_is_left_alone() {
    let mut rt = field("say omw", 7..7);
    let replacing = document(&rt).and_then(|d| d.replacement(4..7)).expect("the shortcut");
    rt.update(|edit, _| edit.text.replace_range(0..3, "omw"));
    rt.frame(0.0);
    rt.ime_replace_from_platform(replacing);
    rt.ime(Ime::Preedit("x".into(), None));
    rt.ime(Ime::Preedit(String::new(), None));
    rt.ime(Ime::Commit("x".into()));
    rt.frame(0.0);
    assert_eq!(rt.state.text, "omw omwx", "the same bytes hold omw, but the field changed since");
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
