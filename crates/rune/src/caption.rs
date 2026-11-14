//! A title bar the app draws itself on Windows (`Chrome::caption_buttons`): the band's metrics
//! the view lays out against, and the map from window points to what Windows' hit test answers
//! there, so drags, Snap, the system menu and Snap Layouts stay the system's own.

#[cfg(test)]
#[path = "caption_tests.rs"]
mod tests;

/// One of the three buttons a Windows title bar ends with. Mark the element that draws it with
/// [`El::caption_button`](crate::El::caption_button).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CaptionButton {
    Minimize,
    Maximize,
    Close,
}

/// The title bar band of a window that draws its own (`Chrome::caption_buttons` on Windows),
/// read with [`Cx::caption`](crate::Cx::caption).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Caption {
    /// The band's height in points: the system title bar's, which the caption buttons fill.
    pub height: f32,
    /// One caption button's width in points, as Windows 11 draws them.
    pub button_width: f32,
    /// Maximized: the maximize button restores.
    pub maximized: bool,
    /// The active window: an inactive one dims its caption glyphs.
    pub active: bool,
}

impl Caption {
    /// Windows 11's caption button width, which no system metric reports.
    pub const BUTTON_WIDTH: f32 = 46.0;
}

/// What a spot of the window is to the system's hit test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CaptionHit {
    /// Content: presses go to the view.
    Client,
    /// A `.window_drag()` region with nothing pressable over it: the system's title bar.
    Drag,
    Button(CaptionButton),
}

/// Window-point rects, topmost last, of the drag regions, the caption buttons and whatever
/// takes presses over them. Everywhere else is content.
pub(crate) type CaptionMap = Vec<([f32; 4], CaptionHit)>;

/// What the topmost rect of `map` containing (x, y) says, else content.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn hit(map: &[([f32; 4], CaptionHit)], x: f32, y: f32) -> CaptionHit {
    map.iter().rev().find(|(r, _)| x >= r[0] && x < r[2] && y >= r[1] && y < r[3]).map_or(CaptionHit::Client, |&(_, hit)| hit)
}

/// Keeps the content rects that overlap a drag region or caption button, which alone can
/// change an answer there, and drops the map when there are neither.
pub(crate) fn prune(map: &mut CaptionMap) {
    let caption: Vec<[f32; 4]> = map.iter().filter(|(_, h)| *h != CaptionHit::Client).map(|(r, _)| *r).collect();
    let overlaps = |r: &[f32; 4]| caption.iter().any(|c| r[0] < c[2] && c[0] < r[2] && r[1] < c[3] && c[1] < r[3]);
    map.retain(|(r, h)| *h != CaptionHit::Client || overlaps(r));
}
