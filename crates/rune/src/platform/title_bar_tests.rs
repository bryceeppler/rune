use super::TitleBar;
use crate::color::hex;

#[test]
fn a_light_window_has_a_light_title_bar_in_its_own_colour() {
    assert_eq!(TitleBar::of(hex("#f4f5f9").unwrap()), TitleBar { dark: false, caption: [0xf4, 0xf5, 0xf9] });
    assert_eq!(TitleBar::of(hex("#ffffff").unwrap()), TitleBar { dark: false, caption: [0xff, 0xff, 0xff] });
}

#[test]
fn a_dark_window_has_a_dark_title_bar_in_its_own_colour() {
    assert_eq!(TitleBar::of(hex("#1a1b26").unwrap()), TitleBar { dark: true, caption: [0x1a, 0x1b, 0x26] });
    assert_eq!(TitleBar::of(hex("#000000").unwrap()), TitleBar { dark: true, caption: [0, 0, 0] });
}

#[test]
fn the_title_bar_turns_dark_where_white_text_reads_better_than_black() {
    assert!(TitleBar::of(hex("#757575").unwrap()).dark, "white text has the higher contrast on #757575");
    assert!(!TitleBar::of(hex("#777777").unwrap()).dark, "black text has the higher contrast on #777777");
}
