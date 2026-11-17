//! Pills in a paragraph: the room their chips take in its layout, and the chips drawn behind
//! their labels.
use parley::BaseDirection;
use swash::text::{BidiClass, Codepoint as _};

use super::*;

/// A pill's label and the room its chip takes before and after it, in device px. A sized pill
/// takes its box's `size` before the label instead, with `room` more above and below it for
/// its margins and selection ring, and hides the label; one that draws `after` its label
/// takes that room after it and leaves the label as text.
pub(super) struct LabelRoom {
    pub index: usize,
    pub label: std::ops::Range<usize>,
    pub lead: u32,
    pub trail: u32,
    pub size: Option<[f32; 2]>,
    pub room: [f32; 2],
    pub after: bool,
}

/// The room each pill of `pills` takes in `text` laid out at `size` points and `scale`, and the
/// direction the text runs past the labels. A range that is empty, off a character boundary or
/// across a line break has no label and keeps no chip.
pub(super) fn label_rooms(text: &str, pills: &[PillChip], size: f32, scale: f32) -> (BaseDirection, Vec<LabelRoom>) {
    let pm = PillMetrics::new(size);
    let labels: Vec<(usize, &PillChip, std::ops::Range<usize>)> = pills
        .iter()
        .enumerate()
        .map(|(i, p)| (i, p, (p.start as usize).min(text.len())..(p.end as usize).min(text.len())))
        .filter(|(_, _, r)| !r.is_empty() && text.is_char_boundary(r.start) && text.is_char_boundary(r.end) && !text.as_bytes()[r.clone()].contains(&b'\n'))
        .collect();
    let hidden: Vec<_> = labels.iter().filter(|(_, p, _)| !p.after).map(|(_, _, r)| r.clone()).collect();
    let direction = direction_past_labels(text, &hidden);
    let rooms = labels
        .into_iter()
        .filter_map(|(index, p, label)| {
            let after = p.after;
            if let Some(size) = p.style.size.filter(|s| s.iter().all(|v| v.is_finite() && *v > 0.0)) {
                let room = p.room().map(|v| (v * scale).round());
                return Some(LabelRoom { index, label, lead: 0, trail: 0, size: Some(size.map(|v| (v * scale).round())), room, after });
            }
            if after {
                return None;
            }
            let (lead, trail) = (pm.lead(p.style.icon.is_some()), pm.trail());
            let (lead, trail) = if starts_rtl(direction, &label) { (0.0, lead + trail) } else { (lead, trail) };
            Some(LabelRoom { index, label, lead: (lead * scale).round() as u32, trail: (trail * scale).round() as u32, size: None, room: [0.0; 2], after })
        })
        .collect();
    (direction, rooms)
}

/// Boxes before and after each pill's label make room for its chip; they keep the pill's index
/// in their id. The label may break like any text; `break_lines_with_blocks` moves a split pill
/// whole to the next line and lets it wrap only when it is wider than a line, as a browser wraps
/// a span. A sized pill's box grows its line to its height and its label takes no room;
/// [`fit_sized_boxes`] sets where the box meets the baseline. The empty box after a sized
/// pill's label is as tall as the room below its box, so the caret beside it can end there. A
/// box drawn after its label is that pill's only box, at the label's end.
pub(super) fn push_label_rooms(b: &mut parley::RangedBuilder<'_, Ink>, rooms: &[LabelRoom]) {
    for room in rooms {
        if room.after && let Some([width, height]) = room.size {
            b.push_inline_box(parley::InlineBox {
                id: PILL_BOX | PILL_SIZED | PILL_AFTER | (room.index as u64) << 1,
                kind: parley::InlineBoxKind::InFlow,
                index: room.label.end,
                width,
                height: height + room.room[0] + room.room[1],
                baseline: None,
                vertical_align: parley::VerticalAlign::BASELINE,
            });
            continue;
        }
        let tag = if room.size.is_some() { PILL_BOX | PILL_SIZED } else { PILL_BOX };
        let [width, height] = room.size.map_or([room.lead as f32, 0.0], |[w, h]| [w, h + room.room[0] + room.room[1]]);
        let below = if room.size.is_some() { room.room[1] } else { 0.0 };
        for (side, index, width, height) in [(0, room.label.start, width, height), (1, room.label.end, room.trail as f32, below)] {
            b.push_inline_box(parley::InlineBox {
                id: tag | (room.index as u64) << 1 | side,
                kind: parley::InlineBoxKind::InFlow,
                index,
                width,
                height,
                baseline: None,
                vertical_align: parley::VerticalAlign::BASELINE,
            });
        }
        if room.size.is_some() {
            b.push(StyleProperty::FontSize(HIDDEN_LABEL_SIZE), room.label.clone());
            b.push(LineHeight::Absolute(0.0), room.label.clone());
        }
    }
}

/// The widths of the sized pills' boxes on `line`, in the order they draw (device px).
pub(crate) fn sized_box_widths(line: &parley::Line<'_, Ink>) -> Vec<f32> {
    line.items()
        .filter_map(|it| match it {
            PositionedLayoutItem::InlineBox(b) if b.id & PILL_SIZED != 0 && b.id & PILL_AFTER == 0 && b.id & 1 == 0 => Some(b.width),
            _ => None,
        })
        .collect()
}

/// The font size a sized pill's label is shaped at, in points: too small to draw or take room.
const HIDDEN_LABEL_SIZE: f32 = 0.01;

/// Scales each sized pill's box in `layout` down to fit the room its paragraph's line has at
/// `max_width` (device px), keeping its shape, and sets each box's baseline so its foot meets
/// the foot of a line of text `line_px` tall: the room below the box is then its own margin.
/// `rooms` hold the boxes' full sizes.
#[allow(clippy::too_many_arguments)]
pub(super) fn fit_sized_boxes(layout: &mut Layout<Ink>, text: &str, rooms: &[LabelRoom], blocks: &[crate::edit::Block], font_size: f32, line_px: f32, scale: f32, max_width: Option<f32>) {
    let sized: Vec<(u64, &LabelRoom, [f32; 2], f32)> = rooms
        .iter()
        .filter_map(|r| {
            let paragraph = text[..r.label.start].bytes().filter(|&c| c == b'\n').count();
            let indent = blocks.get(paragraph).map_or(0.0, |b| (b.indent_pt(font_size) * scale).round());
            let room = max_width.map_or(f32::INFINITY, |max| (max - indent - scale).max(1.0));
            let tag = if r.after { PILL_BOX | PILL_SIZED | PILL_AFTER } else { PILL_BOX | PILL_SIZED };
            Some((tag | (r.index as u64) << 1, r, r.size?, room))
        })
        .collect();
    if sized.is_empty() {
        return;
    }
    let depth = layout.root_span_metrics().map_or(0.0, |font| {
        let (ascent, descent) = (font.ascent, font.descent.abs());
        ((line_px - ascent - descent) * 0.5 + descent).max(0.0)
    });
    for b in layout.inline_boxes_mut() {
        if let Some(&(_, r, [w, h], room)) = sized.iter().find(|s| s.0 == b.id) {
            let k = (room / w).min(1.0);
            let [above, below] = r.room;
            (b.width, b.height) = ((w * k).floor().max(1.0), (h * k).round().max(1.0) + above + below);
            b.baseline = Some((b.height - depth).max(0.0));
        }
    }
}

fn direction_past_labels(text: &str, labels: &[std::ops::Range<usize>]) -> BaseDirection {
    if labels.is_empty() {
        return BaseDirection::Auto;
    }
    strong_direction(text, labels).unwrap_or(BaseDirection::Ltr)
}

/// The direction of a text's first strong character outside every pill label and every
/// isolate (rule P2), or `None` when it has none. A chip draws in place of its label like an
/// inline object, which the bidirectional algorithm counts as neutral, and an isolate runs to
/// its matching PDI or the paragraph's end.
pub(crate) fn strong_direction(text: &str, labels: &[std::ops::Range<usize>]) -> Option<BaseDirection> {
    let mut isolates = 0usize;
    text.char_indices().find_map(|(at, c)| {
        let direction = match c.bidi_class() {
            BidiClass::LRI | BidiClass::RLI | BidiClass::FSI => {
                isolates += 1;
                return None;
            }
            BidiClass::PDI => {
                isolates = isolates.saturating_sub(1);
                return None;
            }
            BidiClass::B => {
                isolates = 0;
                return None;
            }
            BidiClass::L => BaseDirection::Ltr,
            BidiClass::R | BidiClass::AL => BaseDirection::Rtl,
            _ => return None,
        };
        (isolates == 0 && !labels.iter().any(|r| r.contains(&at))).then_some(direction)
    })
}

/// Whether a pill's `label` starts a right-to-left text laid out `direction`. Parley gives an
/// inline box before a text's first character the left-to-right level, which draws it at the
/// line's far left, so such a pill keeps all its chip's room in the box after its label, which
/// draws beside the label on its left, and its icon at that room's right.
fn starts_rtl(direction: BaseDirection, label: &std::ops::Range<usize>) -> bool {
    direction == BaseDirection::Rtl && label.start == 0
}

/// The chip behind each pill in the layouts of `layout` that meet `band` (its y, device px),
/// at `origin`, and each leading icon with its size in px and top-left corner. A pill whose
/// label wraps draws one chip per line, the first to its line's end and the rest from the
/// line's start. A sized pill's chip fills its box; returns those boxes, by the pill's index in
/// `pills`, for the image, icon and caption drawn in them.
#[allow(clippy::too_many_arguments)]
pub(crate) fn pill_chips(
    layout: &TextLayout,
    pills: &[PillChip],
    size: f32,
    scale: f32,
    origin: [f32; 2],
    band: std::ops::Range<f32>,
    bg: Color,
    out: &mut Vec<PaintedDeco>,
    icons: &mut Vec<(&'static Icon, u32, [f32; 2], Option<Color>)>,
) -> Vec<(usize, [f32; 4])> {
    let mut boxes = Vec::new();
    for piece in layout.pieces_in_band(band) {
        let (first, local) = (piece.first_pill(), piece.pills(pills));
        if !local.is_empty() {
            let mut each = |i: usize, chip: PaintedDeco| {
                if local[i].style.size.is_some() {
                    boxes.push((first + i, chip.deco.rect));
                }
                out.push(chip);
            };
            chips(piece.layout, piece.start, local, size, scale, [origin[0], origin[1] + piece.y], bg, &mut each, icons);
        }
    }
    boxes
}

/// Where each pill's chips draw in the paragraphs of `layout` that meet `band` (device px,
/// layout top-left at the origin), by the pill's index in `pills`: what a press on a chip
/// lands on.
pub(crate) fn pill_chip_rects(layout: &TextLayout, pills: &[PillChip], size: f32, scale: f32, band: std::ops::Range<f32>) -> Vec<(usize, [f32; 4])> {
    let mut rects = Vec::new();
    for piece in layout.pieces_in_band(band) {
        let (first, local) = (piece.first_pill(), piece.pills(pills));
        if !local.is_empty() {
            chips(piece.layout, piece.start, local, size, scale, [0.0, piece.y], Color::TRANSPARENT, &mut |i, chip| rects.push((first + i, chip.deco.rect)), &mut Vec::new());
        }
    }
    rects
}

/// Where pill `k` of `pills` first draws in `layout`, as [`pill_chip_rects`] gives it, read from
/// the one paragraph holding the pill.
pub(crate) fn pill_chip_rect(layout: &TextLayout, pills: &[PillChip], k: usize, size: f32, scale: f32) -> Option<[f32; 4]> {
    let piece = layout.at_byte(pills.get(k)?.start as usize);
    let first = piece.first_pill();
    let mut rect = None;
    let mut found = |i: usize, chip: PaintedDeco| if first + i == k { rect.get_or_insert(chip.deco.rect); };
    chips(piece.layout, piece.start, piece.pills(pills), size, scale, [0.0, piece.y], Color::TRANSPARENT, &mut found, &mut Vec::new());
    rect
}

/// [`pill_chips`] for one layout whose text starts at byte `start` of the text `pills` index.
#[allow(clippy::too_many_arguments)]
fn chips(
    layout: &Layout<Ink>,
    start: usize,
    pills: &[PillChip],
    size: f32,
    scale: f32,
    origin: [f32; 2],
    bg: Color,
    out: &mut dyn FnMut(usize, PaintedDeco),
    icons: &mut Vec<(&'static Icon, u32, [f32; 2], Option<Color>)>,
) {
    let pm = PillMetrics::new(size);
    let local = |at: u32| (at as usize).saturating_sub(start);
    for line in layout.lines() {
        let m = line.metrics();
        let boxes: Vec<parley::PositionedInlineBox> = line
            .items()
            .filter_map(|it| match it {
                PositionedLayoutItem::InlineBox(b) if b.id & PILL_BOX != 0 => Some(b),
                _ => None,
            })
            .collect();
        // Pills whose chip starts on this line, then those whose label wrapped onto it.
        let mut starts: Vec<(usize, Option<&parley::PositionedInlineBox>)> =
            boxes.iter().filter(|b| b.id & 1 == 0).map(|b| (pill_index(b.id), Some(b))).collect();
        let text = line.text_range();
        for (i, p) in pills.iter().enumerate() {
            let inside = local(p.start) < text.start && text.start < local(p.end);
            if inside && !starts.iter().any(|s| s.0 == i) {
                starts.push((i, None));
            }
        }
        let line_end = m.inline_min_coord + m.offset + m.advance - m.hanging_advance;
        for (i, lead) in starts {
            let Some(pill) = pills.get(i) else { continue };
            if pill.style.size.is_some() {
                if let Some(b) = lead {
                    let [above, below] = pill.room().map(|v| (v * scale).round());
                    let rect = [origin[0] + b.x, origin[1] + b.y + above, b.width, b.height - above - below];
                    out(i, PaintedDeco::new(Deco { rect, color: pill.style.bg.unwrap_or(bg), radius: pill.radius(rect, scale) }, None, 0));
                }
                continue;
            }
            let id = PILL_BOX | (i as u64) << 1;
            let trail = boxes.iter().find(|b| b.id == id | 1);
            let (mut asc, mut desc, mut gr_x0, mut gr_x1) = (0.0f32, 0.0f32, f32::INFINITY, f32::NEG_INFINITY);
            for item in line.items() {
                if let PositionedLayoutItem::GlyphRun(gr) = item {
                    let r = gr.run().text_range();
                    if r.start < local(pill.end) && local(pill.start) < r.end {
                        let fm = gr.run().font_metrics();
                        (asc, desc) = (asc.max(fm.ascent), desc.max(fm.descent.abs()));
                        gr_x0 = gr_x0.min(gr.offset());
                        gr_x1 = gr_x1.max(gr.offset() + gr.advance());
                    }
                }
            }
            let margin = pm.margin * scale;
            let pad = pm.pad * scale;
            let Some(lead) = lead else {
                // A label carried on from the line above: its chip runs from the line's start,
                // flush with the label (no room was made before it), to its trail, or on to the
                // line's end if it wraps again.
                if gr_x0 > gr_x1 {
                    continue;
                }
                let x1 = trail.filter(|t| (t.x - gr_x1).abs() < 1.0).map_or(gr_x1 + pad, |t| t.x + t.width - margin);
                let x1 = if trail.is_none() { line_end } else { x1 };
                let base = origin[1] + m.baseline;
                let (top, bot) = ((base - asc - 1.0 * scale).round(), (base + desc + 1.0 * scale).round());
                let x0 = gr_x0.min(m.inline_min_coord + m.offset);
                let rect = [(origin[0] + x0).round(), top, (x1 - x0).max(0.0).round(), bot - top];
                out(i, PaintedDeco::new(Deco { rect, color: pill.style.bg.unwrap_or(bg), radius: pill.radius(rect, scale) }, None, 0));
                continue;
            };
            if lead.width == 0.0
                && let Some(room) = trail
            {
                let (x0, x1) = (room.x + margin, (room.x + room.width - margin).max(gr_x1));
                let (top, bot) = chip_band(origin[1] + m.baseline, asc, desc, size, scale);
                let rect = [(origin[0] + x0).round(), top, (x1 - x0).max(0.0).round(), bot - top];
                out(i, PaintedDeco::new(Deco { rect, color: pill.style.bg.unwrap_or(bg), radius: pill.radius(rect, scale) }, None, 0));
                if let Some(icon) = pill.style.icon {
                    let px = (pm.icon * scale).round().max(1.0);
                    let ix = origin[0] + room.x + room.width - (pm.margin + pm.pad) * scale - px;
                    icons.push((icon, px as u32, [ix.round(), ((top + bot - px) * 0.5).round()], pill.style.fg));
                }
                continue;
            }
            let rtl_lead = gr_x0 <= gr_x1 && (lead.x - gr_x1).abs() < 1.0;
            let x0 = if gr_x0 <= gr_x1 {
                if rtl_lead {
                    gr_x0 - pad
                } else {
                    (gr_x0 - (lead.width - margin)).max(lead.x + margin)
                }
            } else {
                lead.x + margin
            };
            let x1 = if gr_x0 <= gr_x1 {
                if let Some(t) = trail.filter(|t| (t.x - gr_x1).abs() < 1.0) {
                    t.x + t.width - margin
                } else if rtl_lead {
                    lead.x + lead.width - margin
                } else if trail.is_none() {
                    line_end
                } else {
                    gr_x1 + pad
                }
            } else {
                trail.map_or(line_end, |t| t.x + t.width - margin)
            };
            let (top, bot) = chip_band(origin[1] + m.baseline, asc, desc, size, scale);
            let rect = [(origin[0] + x0).round(), top, (x1 - x0).max(0.0).round(), bot - top];
            out(i, PaintedDeco::new(Deco { rect, color: pill.style.bg.unwrap_or(bg), radius: pill.radius(rect, scale) }, None, 0));
            if let Some(icon) = pill.style.icon {
                let px = (pm.icon * scale).round().max(1.0);
                let ix = if rtl_lead { rect[0] + rect[2] - pm.pad * scale - px } else { rect[0] + pm.pad * scale };
                icons.push((icon, px as u32, [ix.round(), ((top + bot - px) * 0.5).round()], pill.style.fg));
            }
        }
    }
}

/// A chip's top and bottom for a label on the baseline at `base` whose glyphs rise `asc` and
/// fall `desc`, or for text of `size` points when its glyphs are too small to measure.
fn chip_band(base: f32, asc: f32, desc: f32, size: f32, scale: f32) -> (f32, f32) {
    let (asc, desc) = if asc < size * scale * 0.25 { (size * scale * 0.95, size * scale * 0.25) } else { (asc, desc) };
    ((base - asc - 1.0 * scale).round(), (base + desc + 1.0 * scale).round())
}
