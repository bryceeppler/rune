use super::{Document, Replacement};

#[test]
fn the_selection_counts_utf16_units_past_emoji_and_kanji() {
    let text = "a🐟今日b";
    let doc = Document::new(text, 5..11, None);
    assert_eq!(doc.selected_range(), Some(3..5));
    assert_eq!(doc.substring(3..5), Some(("今日".to_owned(), 3..5)));
    assert_eq!(Document::new(text, 11..5, None).selected_range(), Some(3..5), "a selection made backwards");
    assert_eq!(doc.marked_range(), None);
}

#[test]
fn a_composition_stands_in_for_the_selection_at_the_caret() {
    let preedit = ("きょう".to_owned(), Some((3, 9)));
    let doc = Document::new("ab🐟c", 6..6, Some(&preedit));
    assert_eq!(doc.marked_range(), Some(4..7));
    assert_eq!(doc.selected_range(), Some(5..7), "the input method's cursor inside its composition");
    assert_eq!(doc.substring(0..100), Some(("ab🐟きょうc".to_owned(), 0..8)));
}

#[test]
fn a_range_through_half_an_emoji_widens_to_the_whole_character() {
    let doc = Document::new("x🐟y", 0..0, None);
    assert_eq!(doc.substring(2..3), Some(("🐟".to_owned(), 1..3)));
    assert_eq!(doc.substring(9..10), None, "past the end");
    assert_eq!(doc.substring(3..9), Some(("y".to_owned(), 3..4)));
}

#[test]
fn reconversion_takes_the_selected_word_into_the_composition() {
    let text = "今日は晴れ";
    let mut doc = Document::new(text, 0..6, None);
    let selected = doc.selected_range().expect("a selection");
    let replacing = doc.replacement(selected, true).expect("the selected word");
    assert_eq!(replacing, Replacement { bytes: 0..6, expected: "今日".to_owned(), delete: true });
    doc.mark("きょう", 0..3, Some(&replacing));
    assert_eq!(doc.marked_range(), Some(0..3));
    assert_eq!(doc.substring(0..6), Some(("きょうは晴れ".to_owned(), 0..6)));
    doc.insert("京", None);
    assert_eq!(doc.substring(0..10), Some(("京は晴れ".to_owned(), 0..4)));
    assert_eq!((doc.selected_range(), doc.marked_range()), (Some(1..1), None));
}

#[test]
fn an_insert_at_a_range_replaces_what_it_names() {
    let mut doc = Document::new("say omw", 7..7, None);
    let replacing = doc.replacement(4..7, false).expect("the shortcut");
    assert_eq!(replacing, Replacement { bytes: 4..7, expected: "omw".to_owned(), delete: false });
    doc.insert("On my way!", Some(&replacing));
    assert_eq!(doc.substring(0..100), Some(("say On my way!".to_owned(), 0..14)));
    assert_eq!(doc.selected_range(), Some(14..14));
}

#[test]
fn inside_a_composition_the_input_method_replaces_only_its_own_text() {
    let preedit = ("にほ".to_owned(), None);
    let doc = Document::new("ab", 2..2, Some(&preedit));
    assert_eq!(doc.replacement(2..4, false), None);
    let mut unmarked = doc.clone();
    unmarked.unmark();
    assert_eq!((unmarked.marked_range(), unmarked.substring(0..9)), (None, Some(("ab".to_owned(), 0..2))));
}
