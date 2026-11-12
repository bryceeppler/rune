use rune::{Cx, El, Env, Key, KeyEvent, Mods, Runtime, col, el};
use rune_kit::{Kit, Menu, MenuItem, root};

#[derive(Default)]
struct State { open: bool, chosen: usize }

fn view(s: &State, _: &mut Cx<State>) -> El {
    let k = Kit::neutral(true);
    let mut layer = Vec::new();
    let button = el().key("more").size(60.0, 24.0).focusable(true).role("button").on_press(|s: &mut State| s.open = !s.open);
    let menu = Menu::new("more")
        .trigger(button)
        .item(MenuItem::new("Pin").on_select(|s: &mut State| { s.chosen += 1; s.open = false; }))
        .item(MenuItem::new("Delete").on_select(|s: &mut State| { s.chosen += 1; s.open = false; }))
        .open(s.open)
        .on_close(|s: &mut State| s.open = false)
        .build(&k, &mut layer);
    let list = col().key("list").role("list").focus_group()
        .child(el().key("first").size(200.0, 30.0).focusable(true))
        .child(el().key("second").size(200.0, 30.0).focusable(true));
    root(col().padding(20.0).gap(10.0).child(list).child(menu), layer)
}

fn runtime() -> Runtime<State> {
    let mut rt = Runtime::new(State::default(), view, (400.0, 300.0), Env::new(true, 1.0));
    rt.on_key = Some(|s, ev, _| {
        let f10 = ev.key == Key::Function(10);
        if f10 { s.open = !s.open; }
        f10
    });
    settle(&mut rt);
    rt
}

fn settle(rt: &mut Runtime<State>) {
    rt.frame(0.0);
    for _ in 0..60 { rt.frame(1.0 / 60.0); }
}

fn press(rt: &mut Runtime<State>, key: Key) {
    rt.key(KeyEvent { key, mods: Mods::default() });
    settle(rt);
}

fn click(rt: &mut Runtime<State>, x: f32, y: f32) {
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

fn focused(rt: &Runtime<State>) -> Option<String> {
    rt.elements().into_iter().find(|e| e.focused).and_then(|e| e.key)
}

#[derive(Clone, Copy, Debug)]
enum Close { Escape, Outside, Choice }

fn close(rt: &mut Runtime<State>, how: Close) {
    assert!(rt.state.open, "the menu is open before {how:?}");
    match how {
        Close::Escape => press(rt, Key::Escape),
        Close::Outside => click(rt, 390.0, 290.0),
        Close::Choice => {
            let (x, y) = rt.find("more-item0").expect("the first item");
            click(rt, x, y);
        }
    }
    assert!(!rt.state.open, "{how:?} closes the menu");
    assert_eq!(rt.state.chosen, usize::from(matches!(how, Close::Choice)), "only a choice chooses");
}

const EVERY_CLOSE: [Close; 3] = [Close::Escape, Close::Outside, Close::Choice];

#[test]
fn a_menu_a_key_opened_from_a_list_row_gives_the_row_back_on_every_close() {
    for how in EVERY_CLOSE {
        let mut rt = runtime();
        press(&mut rt, Key::Tab);
        press(&mut rt, Key::Down);
        assert_eq!(focused(&rt).as_deref(), Some("second"));
        press(&mut rt, Key::Function(10));
        close(&mut rt, how);
        assert_eq!(focused(&rt).as_deref(), Some("second"), "{how:?} gives the row back, not the menu's button");
        press(&mut rt, Key::Up);
        assert_eq!(focused(&rt).as_deref(), Some("first"), "after {how:?} the list takes the next arrow");
    }
}

#[test]
fn a_menu_opened_by_clicking_its_button_gives_the_button_back_on_every_close() {
    for how in EVERY_CLOSE {
        let mut rt = runtime();
        let (x, y) = rt.find("more").expect("the button");
        click(&mut rt, x, y);
        close(&mut rt, how);
        assert_eq!(focused(&rt).as_deref(), Some("more"), "{how:?} gives the button back");
    }
}

#[test]
fn a_menu_a_key_opened_with_nothing_focused_leaves_nothing_focused_on_every_close() {
    for how in EVERY_CLOSE {
        let mut rt = runtime();
        assert_eq!(focused(&rt), None);
        press(&mut rt, Key::Function(10));
        close(&mut rt, how);
        assert_eq!(focused(&rt), None, "{how:?} leaves nothing focused");
    }
}
