use super::*;

const KEY: &str = "notes1-afcd1d7b39a820e2f465b9a16a9e786e4f450980185dc406ec814c72a8b88bf8-c8cbf783";

#[test]
fn break_all_fills_each_line_of_a_hyphenated_token_where_wrap_anywhere_breaks_at_its_hyphen() {
    use crate::text;
    fn view(_: &(), _: &mut Cx<()>) -> El {
        col()
            .size(400.0, 400.0)
            .child(col().w(220.0).child(text(KEY).wrap_anywhere().key("anywhere")))
            .child(col().w(220.0).child(text(KEY).break_all().key("all")))
    }
    let mut rt = Runtime::new((), view, (400.0, 400.0), Env::new(true, 1.0));
    rt.frame(0.0);
    let lines = |k: &str| -> Vec<std::ops::Range<usize>> {
        let node = rt.nodes.iter().find(|n| n.key.as_deref() == Some(k)).unwrap();
        node.text_layout.as_ref().unwrap().lines().map(|l| l.text_range()).collect()
    };

    let anywhere = lines("anywhere");
    assert_eq!(anywhere[0], 0..7, "overflow-wrap ends the first line at the hyphen");

    let all = lines("all");
    assert!(all.len() < anywhere.len(), "break-all takes fewer lines: {all:?} against {anywhere:?}");
    assert!(all[0].end > 7, "break-all fills the first line past the hyphen: {all:?}");
    let shortest = all[..all.len() - 1].iter().map(|r| r.len()).min().unwrap();
    assert!(shortest + 1 >= all[0].len(), "every line but the last is full: {all:?}");
    assert_eq!(all.last().unwrap().end, KEY.len(), "every character is laid out once");
    let el = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("all")).unwrap();
    assert!(el.rect[2] <= 220.5, "the token stays inside its box (got {})", el.rect[2]);
}
