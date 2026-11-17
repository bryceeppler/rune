//! What an app styles a text element's byte ranges with: spans, links, highlights and pills.

use super::*;

/// Brush carried through Parley: 0 is the element's own colour, n is `spans[n - 1]`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ink(pub u16);

/// A styled byte range of a text element (markdown inline code, bold, links).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Span {
    pub start: u32,
    pub end: u32,
    pub weight: Option<f32>,
    pub mono: bool,
    pub size: Option<f32>,
    pub color: Option<Color>,
    /// Rounded background behind the span's runs (inline code), padded by `bg_pad` points.
    pub bg: Option<Color>,
    pub bg_pad: f32,
    pub underline: Option<Color>,
    /// The family's italic face, or a synthetic oblique when it has none.
    pub italic: bool,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Span {
        Span { start: start as u32, end: end as u32, ..Default::default() }
    }

    pub fn weight(mut self, weight: f32) -> Span {
        self.weight = Some(weight);
        self
    }

    /// Weight 600.
    pub fn bold(mut self) -> Span {
        self.weight = Some(600.0);
        self
    }

    pub fn italic(mut self) -> Span {
        self.italic = true;
        self
    }

    pub fn mono(mut self) -> Span {
        self.mono = true;
        self
    }

    pub fn size(mut self, size: f32) -> Span {
        self.size = Some(size);
        self
    }

    pub fn color(mut self, color: Color) -> Span {
        self.color = Some(color);
        self
    }

    /// A rounded `color` background padded by `pad` points, as inline code draws.
    pub fn bg(mut self, color: Color, pad: f32) -> Span {
        self.bg = Some(color);
        self.bg_pad = pad;
        self
    }

    pub fn underline(mut self, color: Color) -> Span {
        self.underline = Some(color);
        self
    }
}

/// How one [`Mark::Pill`](crate::Mark::Pill) chip looks: any part left unset falls back to
/// [`El::pill_colors`](crate::El::pill_colors) and [`El::pill_icon`](crate::El::pill_icon), then
/// the field's defaults. Build one with `PillStyle::default().bg(..).icon(..)`.
///
/// A [`sized`](PillStyle::sized) pill draws one box in place of its label, such as a photo:
///
/// ```ignore
/// PillStyle::new().sized([640.0, 400.0]).image(photo.pixels()).description("Photo of the harbour")
///     .bg(well).fg(muted).icon(&PHOTO).caption("Downloading photo…", caption_style)
///     .radius(8.0).border(hairline, 1.0).selection_ring(accent, 3.0, 2.0)
/// ```
#[derive(Clone, Debug, Default)]
pub struct PillStyle {
    pub(crate) bg: Option<Color>,
    pub(crate) fg: Option<Color>,
    pub(crate) icon: Option<&'static Icon>,
    pub(crate) radius: Option<f32>,
    pub(crate) size: Option<[f32; 2]>,
    pub(crate) image: Option<crate::Pixels>,
    pub(crate) description: Option<String>,
    pub(crate) caption: Option<(String, crate::TextStyle)>,
    pub(crate) border: Option<(Color, f32)>,
    pub(crate) selection_ring: Option<SelectionRing>,
    pub(crate) margin: [f32; 2],
}

/// A sized pill's ring while the selection covers it: `width` points wide, `gap` points outside
/// its box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SelectionRing {
    pub color: Color,
    pub width: f32,
    pub gap: f32,
}

impl PillStyle {
    pub const fn new() -> Self {
        Self { bg: None, fg: None, icon: None, radius: None, size: None, image: None, description: None, caption: None, border: None, selection_ring: None, margin: [0.0; 2] }
    }
    /// One box `size` points wide and tall in place of the label, scaled down to fit the line's
    /// width, and the line grows to its height. [`PillStyle::bg`] fills it and the icon draws
    /// centred in it, which shows while the [`image`](PillStyle::image) loads or when it has
    /// none. The caret steps over the box and Backspace deletes it whole, as with any pill.
    pub const fn sized(mut self, size: [f32; 2]) -> Self {
        self.size = Some(size);
        self
    }
    /// Pixels a sized pill draws over its background, filling its box and cropped to its shape.
    /// Until they hold an image the box shows its background and icon; when they do, the window
    /// draws them without laying the text out again.
    pub fn image(mut self, pixels: &crate::Pixels) -> Self {
        self.image = Some(pixels.clone());
        self
    }
    /// What assistive technology reads for a sized pill: the field's text holds one object
    /// replacement character (U+FFFC) for the pill, and an image beside the text's lines carries
    /// this description.
    pub fn description(mut self, text: impl Into<String>) -> Self {
        self.description = Some(text.into());
        self
    }
    /// The chip's or the sized box's corner radius in points. Default: 5 points, at most half
    /// the chip's height.
    pub const fn radius(mut self, radius: f32) -> Self {
        self.radius = Some(radius);
        self
    }
    /// One short line centred under a sized pill's icon while its image isn't ready, such as
    /// "Downloading photo…", in [`PillStyle::fg`], half its line height below the icon.
    pub fn caption(mut self, text: impl Into<String>, style: crate::TextStyle) -> Self {
        self.caption = Some((text.into(), style));
        self
    }
    /// A ring `width` points wide just inside a sized pill's box, over its image: a hairline
    /// that keeps a pale photo's edge.
    pub const fn border(mut self, color: Color, width: f32) -> Self {
        self.border = Some((color, width));
        self
    }
    /// A ring `width` points wide, `gap` points outside a sized pill's box, while the selection
    /// covers the pill, in place of the selection's tint. The line keeps room for it above the
    /// box; beside and below the box it draws over whatever is there, so the field's ancestors
    /// must not clip within `width + gap` of the box.
    pub const fn selection_ring(mut self, color: Color, width: f32, gap: f32) -> Self {
        self.selection_ring = Some(SelectionRing { color, width, gap });
        self
    }
    /// Room a sized pill's line keeps above and below its box, in points, so the text before
    /// and after stands that far from it. The room above is at least the selection ring's.
    pub const fn margin(mut self, above: f32, below: f32) -> Self {
        self.margin = [above, below];
        self
    }
    /// The chip's colour.
    pub const fn bg(mut self, bg: Color) -> Self {
        self.bg = Some(bg);
        self
    }
    /// The label and icon colour.
    pub const fn fg(mut self, fg: Color) -> Self {
        self.fg = Some(fg);
        self
    }
    /// The leading icon, in the label's colour.
    pub const fn icon(mut self, icon: &'static Icon) -> Self {
        self.icon = Some(icon);
        self
    }
}

/// A pill over byte range `start..end` of the content, drawn in `style`: an atomic chip or box
/// in place of its label, or, `after` them, a sized box on a line of its own while the bytes
/// stay text.
#[derive(Clone, Debug, Default)]
pub(crate) struct PillChip {
    pub start: u32,
    pub end: u32,
    pub style: PillStyle,
    /// A field's id for it, which `on_pill_press` and `Cx::pill_rect` name.
    pub id: Option<String>,
    pub after: bool,
}

impl PillChip {
    /// Whether bytes `a..b` cover this pill whole.
    pub fn within(&self, a: usize, b: usize) -> bool {
        a <= self.start as usize && self.end as usize <= b
    }

    /// The radius of a chip or box `rect` (device px) at `scale`.
    pub fn radius(&self, rect: [f32; 4], scale: f32) -> f32 {
        (self.style.radius.unwrap_or(5.0) * scale).min(rect[2].min(rect[3]) * 0.5).max(0.0)
    }

    /// The room a sized pill's line keeps above and below its box, in points: its margins, and
    /// above it at least as far as its selection ring reaches.
    pub fn room(&self) -> [f32; 2] {
        let ring = self.style.selection_ring.map_or(0.0, |r| (r.width + r.gap).max(0.0));
        let [above, below] = self.style.margin.map(|m| m.max(0.0));
        [above.max(ring), below]
    }

    /// Whether this sized pill's image is ready to draw.
    pub fn image_ready(&self) -> bool {
        self.style.image.as_ref().is_some_and(|p| p.size().0 > 0)
    }
}

/// Whether a selection of bytes `a..b` covers any sized pill of `pills`.
pub(crate) fn covers_sized(pills: &[PillChip], a: usize, b: usize) -> bool {
    a < b && pills.iter().any(|p| p.style.size.is_some() && p.within(a, b))
}

/// A pill chip's horizontal geometry in points for text of `size` points: the room before its
/// label (outer margin, padding, then any icon and its gap) and after it (padding, margin).
pub(crate) struct PillMetrics {
    pub margin: f32,
    pub pad: f32,
    pub icon: f32,
    pub icon_gap: f32,
}

impl PillMetrics {
    pub fn new(size: f32) -> Self {
        Self { margin: (size * 0.12).max(1.5), pad: (size * 0.36).max(3.0), icon: (size * 0.95).round(), icon_gap: (size * 0.22).max(2.0) }
    }

    /// Room before the label, inside the chip and out.
    pub fn lead(&self, icon: bool) -> f32 {
        self.margin + self.pad + if icon { self.icon + self.icon_gap } else { 0.0 }
    }

    pub fn trail(&self) -> f32 {
        self.pad + self.margin
    }
}

/// Tags the inline boxes around pill labels: the rest of the id is the pill's index shifted
/// left once, with the low bit set on the box after the label.
pub(super) const PILL_BOX: u64 = 1 << 62;
/// Tags the box that stands for a [`PillStyle::sized`] pill's label.
pub(super) const PILL_SIZED: u64 = 1 << 61;
/// Tags a sized pill's box that draws after its label, which stays text.
pub(super) const PILL_AFTER: u64 = 1 << 60;

/// The index of the pill a pill box's `id` belongs to.
pub(super) fn pill_index(id: u64) -> usize {
    ((id & !(PILL_BOX | PILL_SIZED | PILL_AFTER)) >> 1) as usize
}

/// Flattens overlapping [`Span`]s into non-overlapping elementary intervals so that Parley's
/// single `Brush(Ink)` index per glyph run carries the combined colour, background, underline,
/// weight, italic, and monospace attributes of all overlapping spans.
pub(crate) fn flatten_spans(spans: &[Span]) -> Vec<Span> {
    if spans.len() <= 1
        || spans
            .windows(2)
            .all(|w| w[0].start < w[0].end && w[0].end <= w[1].start)
    {
        return spans.iter().filter(|s| s.start < s.end).cloned().collect();
    }
    let mut bounds = Vec::with_capacity(spans.len() * 2);
    let mut order: Vec<usize> = Vec::with_capacity(spans.len());
    for (i, s) in spans.iter().enumerate() {
        if s.start < s.end {
            bounds.push(s.start);
            bounds.push(s.end);
            order.push(i);
        }
    }
    bounds.sort_unstable();
    bounds.dedup();
    order.sort_by_key(|&i| spans[i].start);
    // Sweep the elementary intervals, keeping the spans that cover the current one in input
    // order so later spans still win, without rescanning every span per interval.
    let mut active: Vec<usize> = Vec::new();
    let mut next = 0;
    let mut out: Vec<Span> = Vec::with_capacity(bounds.len());
    for w in bounds.windows(2) {
        let (a, b) = (w[0], w[1]);
        active.retain(|&i| spans[i].end > a);
        while next < order.len() && spans[order[next]].start <= a {
            let i = order[next];
            let at = active.partition_point(|&j| j < i);
            active.insert(at, i);
            next += 1;
        }
        let mut merged: Option<Span> = None;
        for &i in &active {
            let s = &spans[i];
            let m = merged.get_or_insert_with(|| Span::new(a as usize, b as usize));
            if s.weight.is_some() {
                m.weight = s.weight;
            }
            m.mono |= s.mono;
            if s.size.is_some() {
                m.size = s.size;
            }
            if s.color.is_some() {
                m.color = s.color;
            }
            if s.bg.is_some() {
                m.bg = s.bg;
                m.bg_pad = s.bg_pad;
            }
            if s.underline.is_some() {
                m.underline = s.underline;
            }
            m.italic |= s.italic;
        }
        if let Some(m) = merged {
            if let Some(last) = out.last_mut()
                && last.end == m.start
                && last.weight == m.weight
                && last.mono == m.mono
                && last.size == m.size
                && last.color == m.color
                && last.bg == m.bg
                && last.bg_pad == m.bg_pad
                && last.underline == m.underline
                && last.italic == m.italic
            {
                last.end = m.end;
            } else {
                out.push(m);
            }
        }
    }
    out
}

/// A pressable byte range of a text element: a link, an @mention, a file chip. Attach with
/// [`El::link`](crate::El::link) and handle with [`El::on_link`](crate::El::on_link). Its resting
/// look is whatever [`Span`]s cover it; [`LinkPaint`]s add the hover, pressed and focus looks.
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub(crate) start: u32,
    pub(crate) end: u32,
    pub(crate) id: String,
    pub(crate) role: &'static str,
    pub(crate) label: Option<String>,
    pub(crate) hover: LinkPaint,
    pub(crate) pressed: Option<LinkPaint>,
    pub(crate) focused: Option<LinkPaint>,
    pub(crate) help: Option<String>,
}

impl Link {
    /// Bytes `range` of the text, reported to `on_link` as `id`. Its role is "link".
    pub fn new(range: std::ops::Range<usize>, id: impl Into<String>) -> Link {
        Link {
            start: range.start as u32,
            end: range.end as u32,
            id: id.into(),
            role: "link",
            label: None,
            hover: LinkPaint::default(),
            pressed: None,
            focused: None,
            help: None,
        }
    }
    /// Accessibility role, such as "button" for a file chip.
    pub fn role(mut self, role: &'static str) -> Self {
        self.role = role;
        self
    }
    /// Accessible name; the range's text by default.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
    /// The look while the pointer is over it.
    pub fn hover(mut self, paint: LinkPaint) -> Self {
        self.hover = paint;
        self
    }
    /// The look while it is held down; the hover look by default.
    pub fn pressed(mut self, paint: LinkPaint) -> Self {
        self.pressed = Some(paint);
        self
    }
    /// The look while it has keyboard focus; the hover look by default, or an underline in
    /// its text colour when that is empty.
    pub fn focused(mut self, paint: LinkPaint) -> Self {
        self.focused = Some(paint);
        self
    }
    /// A tooltip for this link alone, as [`El::help`](crate::El::help) is for an element:
    /// shown after the hover delay while the pointer rests on the link's glyphs, and hidden
    /// by a press, scroll, key or the pointer leaving them. An empty string sets none.
    pub fn help(mut self, text: impl Into<String>) -> Self {
        let text = text.into();
        self.help = (!text.is_empty()).then_some(text);
        self
    }
}

/// A link's look in one state, painted over its resting span style without reshaping. Unset
/// parts keep the span's own.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LinkPaint {
    pub(crate) color: Option<Color>,
    pub(crate) bg: Option<Color>,
    pub(crate) ring: Option<(Color, f32)>,
    pub(crate) underline: Option<Color>,
}

impl LinkPaint {
    pub fn new() -> LinkPaint {
        LinkPaint::default()
    }
    /// Glyph colour.
    pub fn color(mut self, c: Color) -> Self {
        self.color = Some(c);
        self
    }
    /// A rounded background behind each line of the range.
    pub fn bg(mut self, c: Color) -> Self {
        self.bg = Some(c);
        self
    }
    /// An inset `c` border ring `width` points wide inside the rounded background on each line.
    pub fn ring(mut self, c: Color, width: f32) -> Self {
        self.ring = (width > 0.0 && width.is_finite()).then_some((c, width));
        self
    }
    pub fn underline(mut self, c: Color) -> Self {
        self.underline = Some(c);
        self
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.color.is_none() && self.bg.is_none() && self.ring.is_none() && self.underline.is_none()
    }
}

/// A paint-only highlight over `start..end` UTF-8 bytes of a [`text`](crate::text) element.
///
/// Drawn from the shaped layout's glyph boxes without splitting runs, so adding, moving or
/// clearing highlights never reshapes text or shifts line breaks.
#[derive(Clone, Debug, PartialEq)]
pub struct TextHighlight {
    pub(crate) start: u32,
    pub(crate) end: u32,
    pub(crate) bg: Color,
    pub(crate) fg: Option<Color>,
    pub(crate) ring: Option<(Color, f32)>,
    pub(crate) pad: f32,
    pub(crate) key: Option<String>,
}

impl TextHighlight {
    /// Bytes `range` of the text, painted with a rounded `bg` fill behind each line's glyphs.
    pub fn new(range: std::ops::Range<usize>, bg: Color) -> TextHighlight {
        TextHighlight {
            start: range.start.min(u32::MAX as usize) as u32,
            end: range.end.min(u32::MAX as usize) as u32,
            bg,
            fg: None,
            ring: None,
            pad: 1.5,
            key: None,
        }
    }
    /// Glyph colour over the highlighted range.
    pub fn color(mut self, c: Color) -> Self {
        self.fg = Some(c);
        self
    }
    /// Inset `c` border ring `width` points wide inside the rounded background on each line.
    pub fn ring(mut self, c: Color, width: f32) -> Self {
        self.ring = (width > 0.0 && width.is_finite()).then_some((c, width));
        self
    }
    /// Horizontal padding around each line's background, in points (`1.5` by default).
    pub fn pad(mut self, pad: f32) -> Self {
        self.pad = pad;
        self
    }
    /// Optional key for [`Tx::scroll_into_view`](crate::Tx::scroll_into_view),
    /// [`Cx::layout`](crate::Cx::layout) and [`Runtime::find`](crate::Runtime::find).
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
    /// The UTF-8 byte range of this highlight.
    pub fn range(&self) -> std::ops::Range<usize> {
        self.start as usize..self.end as usize
    }
    /// Returns this highlight with its UTF-8 byte range replaced by `range`.
    pub fn with_range(mut self, range: std::ops::Range<usize>) -> Self {
        self.start = range.start.min(u32::MAX as usize) as u32;
        self.end = range.end.min(u32::MAX as usize) as u32;
        self
    }
}
