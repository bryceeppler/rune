use super::{CaptionButton, CaptionHit, hit};
use crate::{Caption, Cx, El, Env, Runtime, col, el, row};

struct State {
    maximize_presses: u32,
}

fn title_bar(_: &State, cx: &mut Cx<State>) -> El {
    let band = cx.caption().map_or(32.0, |c| c.height);
    let button = |key: &str, kind: CaptionButton| el().key(key).size(46.0, band).caption_button(kind);
    col().size(400.0, 300.0)
        .child(row().key("band").w_full().h(band).window_drag()
            .child(el().key("search").size(40.0, band).on_press(|_: &mut State| {}))
            .child(el().key("spacer").grow(1.0))
            .child(button("minimize", CaptionButton::Minimize).on_press_tx(|_: &mut State, tx| tx.minimize()))
            .child(button("maximize", CaptionButton::Maximize).on_press_tx(|s: &mut State, tx| {
                s.maximize_presses += 1;
                tx.toggle_maximized();
            }))
            .child(button("close", CaptionButton::Close).on_press_tx(|_: &mut State, tx| tx.close_window())))
        .child(el().key("body").w_full().h(200.0).on_press(|_: &mut State| {}))
}

#[test]
fn the_title_bar_answers_windows_hit_test_as_caption_buttons_and_drag_region_around_its_controls() {
    let mut rt = Runtime::new(State { maximize_presses: 0 }, title_bar, (400.0, 300.0), Env::new(true, 1.0));
    rt.frame(0.0);
    let map = rt.caption_map();
    for (x, y, want) in [
        (200.0, 16.0, CaptionHit::Drag),
        (20.0, 16.0, CaptionHit::Client),
        (400.0 - 3.0 * 46.0 + 1.0, 16.0, CaptionHit::Button(CaptionButton::Minimize)),
        (400.0 - 46.0 - 23.0, 31.0, CaptionHit::Button(CaptionButton::Maximize)),
        (399.0, 0.0, CaptionHit::Button(CaptionButton::Close)),
        (200.0, 100.0, CaptionHit::Client),
    ] {
        assert_eq!(hit(&map, x, y), want, "at {x}, {y}");
    }
    assert!(map.iter().all(|(r, _)| r[1] < 32.0), "content below the band stays out of the map: {map:?}");
}

#[test]
fn a_caption_button_presses_like_any_button_and_asks_the_window_to_maximize() {
    let mut rt = Runtime::new(State { maximize_presses: 0 }, title_bar, (400.0, 300.0), Env::new(true, 1.0));
    rt.frame(0.0);
    let (x, y) = (400.0 - 46.0 - 23.0, 16.0);
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    assert!(rt.take_titlebar_action().is_none(), "a caption button never drags the window");
    rt.pointer_up(x, y);
    rt.frame(1.0 / 120.0);
    assert_eq!(rt.state.maximize_presses, 1);
    assert!(rt.window_frame.take().is_some_and(|f| f.toggle_maximized && f.size.is_none()), "the press asks the shell to maximize");
}

#[test]
fn the_view_lays_out_against_the_caption_band_and_rebuilds_when_it_changes() {
    let mut rt = Runtime::new(State { maximize_presses: 0 }, title_bar, (400.0, 300.0), Env::new(true, 1.0));
    rt.frame(0.0);
    let height = |rt: &Runtime<State>| rt.presented("band").unwrap().0[3];
    assert_eq!(height(&rt), 32.0, "no caption band: the view's own height");
    rt.set_caption(Some(Caption { height: 31.0, button_width: Caption::BUTTON_WIDTH, maximized: false, active: true }));
    rt.frame(0.1);
    rt.frame(5.0);
    assert_eq!(height(&rt), 31.0, "the system's title bar height");
}
