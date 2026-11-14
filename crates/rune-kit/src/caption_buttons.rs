//! The minimize, maximize and close buttons of a Windows window that draws its own title bar
//! (`rune::Chrome::caption_buttons`), as Windows 11 draws them: a full-height 46-point column
//! each, Segoe Fluent's chrome glyphs at 10 points, the subtle hover fill, and the red close.

use rune::{Caption, CaptionButton, Color, El, Icon, icon, row};

use crate::Kit;

/// Windows 11's close button fill while hovered (`#C42B1C`); pressed, it is 90% opaque.
const CLOSE_FILL: Color = Color { r: 0.552_011_5, g: 0.024_157_6, b: 0.011_612_2, a: 1.0 };

static MINIMIZE: Icon = Icon { name: "kit-caption-minimize", view: 10.0, body: r#"<path d="M0 5.5H10" stroke="currentColor"/>"# };
static MAXIMIZE: Icon = Icon {
    name: "kit-caption-maximize",
    view: 10.0,
    body: r#"<rect x="0.5" y="0.5" width="9" height="9" rx="1.5" stroke="currentColor"/>"#,
};
static RESTORE: Icon = Icon {
    name: "kit-caption-restore",
    view: 10.0,
    body: r#"<rect x="0.5" y="2.5" width="7" height="7" rx="1.5" stroke="currentColor"/><path d="M2.5 2.5V2A1.5 1.5 0 0 1 4 0.5H8A1.5 1.5 0 0 1 9.5 2V6A1.5 1.5 0 0 1 8 7.5H7.5" stroke="currentColor"/>"#,
};
static CLOSE: Icon =
    Icon { name: "kit-caption-close", view: 10.0, body: r#"<path d="M0.5 0.5L9.5 9.5M9.5 0.5L0.5 9.5" stroke="currentColor"/>"# };

/// The three caption buttons for the band `caption` describes ([`rune::Cx::caption`]). Each one
/// is marked with [`El::caption_button`], so Windows offers Snap Layouts over maximize, and
/// presses minimize, maximize or restore, or close the window (`App::on_close` runs). Put it at
/// the band's top right edge. Screen readers hear Minimize, Maximize or Restore, and Close; like
/// Windows' own, the buttons take no keyboard focus (Alt+Space opens the system menu).
pub struct CaptionButtons {
    caption: Caption,
}

impl CaptionButtons {
    pub fn new(caption: Caption) -> Self {
        Self { caption }
    }

    pub fn build<S: 'static>(self, k: &Kit) -> El {
        let Caption { height, button_width, maximized, active } = self.caption;
        let ink = if active { k.c.icon_base } else { k.c.icon_faint };
        let button = |kind: CaptionButton, key: &str, label: &str, glyph: &'static Icon| {
            let glyph = icon(glyph, 10.0).key("glyph").color(ink).a11y_hidden();
            let glyph = if kind == CaptionButton::Close { glyph.group_color(Color::WHITE) } else { glyph };
            row().key(key).role("button").label(label).size(button_width, height).center().caption_button(kind).child(glyph)
        };
        let (zoom, zoom_glyph) = if maximized { ("Restore", &RESTORE) } else { ("Maximize", &MAXIMIZE) };
        row().key("caption-buttons").h(height)
            .child(button(CaptionButton::Minimize, "minimize", "Minimize", &MINIMIZE).hover(k.c.hover).press(k.c.pressed)
                .on_press_tx(|_: &mut S, tx| tx.minimize()))
            .child(button(CaptionButton::Maximize, "maximize", zoom, zoom_glyph).hover(k.c.hover).press(k.c.pressed)
                .on_press_tx(|_: &mut S, tx| tx.toggle_maximized()))
            .child(button(CaptionButton::Close, "close", "Close", &CLOSE).hover_group().hover(CLOSE_FILL).press(CLOSE_FILL.alpha(0.9))
                .on_press_tx(|_: &mut S, tx| tx.close_window()))
    }
}
