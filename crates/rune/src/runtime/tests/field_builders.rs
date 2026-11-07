use super::*;
use crate::{Link, Span, TextEdit, TextStyle, text_input};

const STYLE: TextStyle = TextStyle::new(15.5, 400.0).lh(25.0);

/// A field's own `.strike` and `.link`, given after its other builders, stay drawn and
/// pressable once its rich spans are rebuilt for the frame.
#[test]
fn a_fields_explicit_strike_and_link_survive_its_rich_spans() {
    fn view(s: &TextEdit, _: &mut Cx<TextEdit>) -> El {
        col().size(400.0, 400.0)
            .child(text_input(s, STYLE).key("struck").strike(0, 5, None))
            .child(text_input(s, STYLE).key("linked").spans(vec![Span::new(0, 5).bold()]).link(Link::new(0..5, "explicit")))
    }
    let mut rt = Runtime::new(TextEdit::new("first\nsecond"), view, (400.0, 400.0), Env::new(true, 2.0));
    rt.frame(0.0);
    let spec = |key: &str| rt.nodes.iter().find(|n| n.key.as_deref() == Some(key)).unwrap().text.clone().unwrap();
    assert_eq!(spec("struck").strikes, [(0, 5, None)]);
    let linked = spec("linked");
    assert_eq!(linked.links.iter().map(|l| (l.start, l.end)).collect::<Vec<_>>(), [(0, 5)]);
    assert!(linked.spans.iter().any(|s| s.weight == Some(600.0)), "its spans were rebuilt");
}
