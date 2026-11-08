use super::*;
use crate::{Key, KeyEvent, Mods};

#[test]
fn the_menu_key_opens_nothing_for_a_focused_link_whose_subtree_is_inert() {
    #[derive(Default)]
    struct State { menus: Vec<String> }
    fn view(_: &State, _: &mut Cx<State>) -> El {
        col().key("row").size(300.0, 100.0)
            .on_secondary_press(|s: &mut State, _, _| s.menus.push("row".into()))
            .child(crate::rich("see the docs", Vec::new()).key("para").w(200.0)
                .link(crate::Link::new(8..12, "docs"))
                .on_link_secondary(|s: &mut State, id, _, _| s.menus.push(id.into())))
    }
    let mut rt = Runtime::new(State::default(), view, (300.0, 100.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt.key(KeyEvent { key: Key::Tab, mods: Mods::default() });
    for _ in 0..30 { rt.frame(1.0 / 60.0); }
    assert_eq!(rt.focused_key().as_deref(), Some("docs"));
    // Focus normally drops at the commit that makes it inert; the key must not depend on that.
    for n in &mut rt.nodes { n.inert = true; }
    for (key, shift) in [(Key::ContextMenu, false), (Key::Function(10), true)] {
        rt.key(KeyEvent { key, mods: Mods { shift, ..Mods::default() } });
        rt.frame(0.0);
    }
    assert!(rt.state.menus.is_empty(), "{:?}", rt.state.menus);
}

#[test]
fn a_secondary_press_on_a_text_input_reports_the_text_it_is_on() {
    #[derive(Default)]
    struct State { field: crate::TextEdit, texts: Vec<Option<usize>>, at: Option<PointerPress> }
    fn view(s: &State, _: &mut Cx<State>) -> El {
        col().size(300.0, 100.0)
            .child(crate::text_input(&s.field, crate::TextStyle::new(16.0, 400.0)).single_line()
                .key("field").size(300.0, 40.0).on_edit(|s: &mut State| &mut s.field)
                .on_secondary_press_local(|s: &mut State, p, _| { s.texts.push(p.text); s.at = Some(p.window()); }))
            .child(el().key("plain").size(300.0, 40.0).on_secondary_press_local(|s: &mut State, p, _| s.texts.push(p.text)))
    }
    let mut field = crate::TextEdit::new("one two");
    field.focused = true;
    let mut rt = Runtime::new(State { field, texts: Vec::new(), at: None }, view, (300.0, 100.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt.key(KeyEvent { key: Key::Home, mods: Mods::default() });
    rt.frame(0.0);
    let mut carets = Vec::new();
    for at in 1..=7 {
        rt.key(KeyEvent { key: Key::Right, mods: Mods::default() });
        rt.frame(0.0);
        let [x, y, _, height] = rt.ime_area.expect("the focused field's caret");
        carets.push((at, x + 0.1, y + height * 0.5));
    }
    rt.key(KeyEvent { key: Key::Home, mods: Mods::default() });
    rt.frame(0.0);
    for &(_, x, y) in &carets {
        rt.secondary_down(x, y);
        rt.secondary_up(x, y);
        rt.frame(0.0);
    }
    let pressed: Vec<_> = carets.iter().map(|&(at, ..)| Some(at)).collect();
    assert_eq!(rt.state.texts, pressed, "each press reports the byte its caret would land on");
    assert_eq!(rt.state.field.focus, 0, "a secondary press leaves the caret where it was");
    rt.state.texts.clear();
    rt.key(KeyEvent { key: Key::End, mods: Mods::default() });
    rt.frame(0.0);
    let [x, y, _, height] = rt.ime_area.expect("the field's caret");
    rt.key(KeyEvent { key: Key::ContextMenu, mods: Mods::default() });
    rt.frame(0.0);
    assert_eq!(rt.state.at, Some(PointerPress { x, y: y + height }), "the Menu key's menu opens below the caret");
    rt.secondary_down(10.0, 60.0);
    rt.secondary_up(10.0, 60.0);
    rt.frame(0.0);
    assert_eq!(rt.state.texts, [Some(7), None], "the Menu key reports the caret, and an element without an input no text");
}
