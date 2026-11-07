//! Heights and line fill read from a finished layout.

use parley::{Layout, LineMetrics};

use super::{Ink, TextLayout};
use crate::tok::TextStyle;

/// Layout height in device px: whole lines at the style's line height (an empty text still
/// takes one line, like a browser).
pub fn layout_height(l: &Layout<Ink>, style: TextStyle, scale: f32) -> f32 {
    l.height().max(style.line_px() * scale).ceil()
}

/// Fraction of the final line occupied by ink advance, excluding hanging whitespace.
/// Width and the layout use device pixels. No shaping or allocation occurs here.
pub fn last_line_fill(l: &Layout<Ink>, width: f32) -> f32 {
    l.lines().next_back().map_or(0.0, |line| fill(line.metrics(), width))
}

/// Height held back while the last line fills (research/18 §7), in points.
pub fn stream_height_lag(l: &Layout<Ink>, width: f32, scale: f32) -> f32 {
    l.lines().next_back().map_or(0.0, |line| lag(line.metrics(), l.len(), width, scale))
}

fn fill(m: &LineMetrics, width: f32) -> f32 {
    ((m.advance - m.hanging_advance) / width.max(1.0)).clamp(0.0, 1.0)
}

fn lag(m: &LineMetrics, lines: usize, width: f32, scale: f32) -> f32 {
    let cap = if lines == 1 { 0.52 } else { 0.82 };
    let lag = m.line_height / scale * cap * (1.0 - fill(m, width));
    (lag * 64.0).round() / 64.0
}

impl TextLayout {
    /// [`layout_height`] of the whole text.
    pub(crate) fn box_height(&self, style: TextStyle, scale: f32) -> f32 {
        self.height().max(style.line_px() * scale).ceil()
    }

    /// [`stream_height_lag`] of the whole text.
    pub(crate) fn stream_height_lag(&self, width: f32, scale: f32) -> f32 {
        self.lines().next_back().map_or(0.0, |line| lag(&line.metrics(), self.len(), width, scale))
    }
}
