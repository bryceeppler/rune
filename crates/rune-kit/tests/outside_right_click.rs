use rune::{Cx, El, Env, PointerPress, Runtime, col, el, row};
use rune_kit::{Dialog, Kit, MenuItem, Popover, context_menu, root};

#[derive(Clone, Copy, PartialEq)]
enum Open {
    Dialog { outside: bool },
    Popover,
    Menu,
}

struct State {
    open: Open,
    shown: bool,
    menu: Option<PointerPress>,
    pressed: Vec<&'static str>,
}

fn word(key: &'static str) -> El {
    el().key(key).size(60.0, 30.0).on_secondary_press(move |s: &mut State, p, _| {
        s.pressed.push(key);
        if s.open == Open::Menu { s.menu = Some(p); }
    })
}

fn view(s: &State, _: &mut Cx<State>) -> El {
    let k = Kit::neutral(true);
    let mut layer = Vec::new();
    let words = row().gap(400.0).child(word("first")).child(word("second"));
    let body = match s.open {
        Open::Dialog { outside } => {
            Dialog::new("confirm", "Delete item?").description("This cannot be undone.").fit().click_outside(outside)
                .open(s.shown).on_close(|s: &mut State| s.shown = false).build(&k, &mut layer);
            words
        }
        Open::Popover => {
            let trigger = el().key("anchor").size(40.0, 20.0);
            let popover = Popover::new("pop").trigger(trigger).open(s.shown).on_close(|s: &mut State| s.shown = false)
                .build(&k, &mut layer);
            col().child(words).child(row().padding(200.0).child(popover))
        }
        Open::Menu => context_menu("context", words, s.menu)
            .item(MenuItem::new("Copy").on_select(|s: &mut State| s.menu = None))
            .on_close(|s: &mut State| s.menu = None)
            .build(&k, &mut layer),
    };
    root(col().w_full().h_full().child(body), layer)
}

fn runtime(open: Open) -> Runtime<State> {
    let shown = open != Open::Menu;
    let mut rt = Runtime::new(State { open, shown, menu: None, pressed: Vec::new() }, view, (800.0, 600.0), Env::new(true, 1.0));
    for _ in 0..30 { rt.frame(1.0 / 60.0); }
    rt
}

fn right_click(rt: &mut Runtime<State>, (x, y): (f32, f32)) {
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    for _ in 0..30 { rt.frame(1.0 / 60.0); }
}

#[test]
fn a_right_click_outside_a_dialog_dismisses_it_and_reaches_nothing_behind_it() {
    let mut rt = runtime(Open::Dialog { outside: true });
    let first = rt.find("first").unwrap();
    right_click(&mut rt, first);
    assert!(!rt.state.shown, "an outside press dismisses this dialog");
    assert!(rt.state.pressed.is_empty(), "the content behind it heard nothing");
}

#[test]
fn a_right_click_outside_a_dialog_that_outside_presses_leave_open_does_nothing() {
    let mut rt = runtime(Open::Dialog { outside: false });
    let first = rt.find("first").unwrap();
    right_click(&mut rt, first);
    assert!(rt.state.shown);
    assert!(rt.state.pressed.is_empty());
}

#[test]
fn a_right_click_outside_a_popover_reaches_nothing_behind_it() {
    let mut rt = runtime(Open::Popover);
    let first = rt.find("first").unwrap();
    right_click(&mut rt, first);
    assert!(rt.state.pressed.is_empty());
}

#[test]
fn a_right_click_outside_a_context_menu_opens_the_menu_of_what_is_under_it() {
    let mut rt = runtime(Open::Menu);
    let first = rt.find("first").unwrap();
    let second = rt.find("second").unwrap();
    right_click(&mut rt, first);
    assert!(rt.state.menu.is_some());
    assert!(!rt.contains("context-menu", second.0, second.1), "the second word is outside the first menu");
    right_click(&mut rt, second);
    assert_eq!(rt.state.pressed, ["first", "second"]);
    assert_eq!(rt.state.menu.map(|p| p.x), Some(second.0), "the menu opened at the second word");
}
