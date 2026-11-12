//! A text node's laid-out lines. A text of several paragraphs (each up to and including a
//! `\n`) lays out one Parley layout per paragraph, each cached under that paragraph's text,
//! spans, blocks and style, and stacks them, so an edit reshapes only the paragraph
//! it touches.
//!
//! A `\n` ends a paragraph as the Unicode bidirectional algorithm ends one: nothing before it
//! reaches the text after it. Each paragraph takes its own direction from its first strong
//! character outside an isolate, left to right when it has none, as a natural writing direction
//! does (UAX #9 P2 and P3), so a Hebrew or Arabic line in an English note reads and aligns right to
//! left. Its characters with no script of their own (spaces, digits, punctuation, marks, emoji)
//! take the script of the text before them in the paragraph, or at its start its first script. One
//! Parley layout of the whole text differs: it takes one direction for every paragraph, and carries
//! bidi levels and scripts across `\n`. The text splits only at `\n`, where monospace tab stops
//! also restart their columns; Parley's other hard breaks (`\r`, U+2028, U+2029, and a `\r\n` that
//! a style change splits in two) stay inside a stacked paragraph's layout. Each of Parley's
//! paragraphs takes the next block, so a stacked paragraph lays out with as many blocks as its
//! layout has hard breaks, a count read back from the layout itself.
//!
//! A paragraph's layout holds the `\n` that ends it, so each line keeps the text range, break
//! reason and newline cluster it has in one layout of the whole text. Parley adds an empty
//! line after a final break; the stack drops it from every paragraph but the last, where the
//! whole text has it too, indented by its own block. Paragraph tops are the whole text's line
//! tops rounded to device pixels, as Parley rounds them, so a paragraph's first line lands
//! where one layout puts it. Lines below it in a wrapped paragraph follow Parley's rounding
//! inside the paragraph: with line heights off the pixel grid (a heading, or 1.5x) one can sit
//! a pixel off a whole layout.

use std::ops::Range;
use std::rc::Weak;

use swash::text::{BidiClass, Codepoint as _};

use parley::BoundingBox;
use parley::editing::Cursor;
use parley::layout::Affinity;

use super::*;
use super::cursor::{cursor_from_point_in, drawn, line_for_offset, plain, step_on_line};
use super::layout_cache::fresh;
use super::line_break::line_bytes;

/// A text's lines: one Parley layout, or one per hard paragraph stacked top to bottom.
#[derive(Clone)]
pub struct TextLayout(Laid);

#[derive(Clone)]
enum Laid {
    Whole(Rc<Layout<Ink>>),
    Stacked(Rc<Stack>),
}

/// Texts of several paragraphs laid out as stacks of `layouts`, by break key. A stack holds
/// its paragraphs' layouts, so it stays only while something shows it or for two commits:
/// after that its paragraphs are the layout cache's to keep within its budget.
#[derive(Default)]
pub(crate) struct Stacks {
    pub(super) cache: Cache<Rc<Stack>>,
    /// The stack built last, while it lasts, and what from: the next edit of its text builds
    /// from it.
    last: Option<Source>,
    /// One paragraph's spans and pills while a stack is built.
    spans: Vec<Span>,
    pills: Vec<PillChip>,
}

/// What a stack was laid out from.
struct Source {
    stack: Weak<Stack>,
    text: Box<str>,
    spans: Box<[Span]>,
    pills: Box<[PillChip]>,
    blocks: Box<[crate::edit::Block]>,
    style: TextStyle,
    scale: f32,
    max_width: Option<f32>,
    align: Alignment,
    wrapping: Wrapping,
}

/// One paragraph of a text being stacked: its bytes, spans and pills, the text's blocks and
/// the first of them it takes, and whether it is the text's last.
struct Draft<'a> {
    start: usize,
    end: usize,
    spans: &'a [Span],
    pills: &'a [PillChip],
    blocks: &'a [crate::edit::Block],
    block: usize,
    last: bool,
}

impl Draft<'_> {
    /// The blocks it lays out with when its layout has `breaks` hard breaks: one for each of
    /// Parley's paragraphs it shows, the last paragraph's own or the empty one after a final
    /// break included.
    fn blocks(&self, breaks: usize) -> &[crate::edit::Block] {
        clip(self.blocks, self.block..self.block + breaks + self.last as usize)
    }
}

impl Stacks {
    /// Keeps the stacks something else holds (a node's text) or a commit before this one used.
    pub(super) fn end_commit(&mut self, g: u32) {
        for e in self.cache.values_mut().filter(|e| Rc::strong_count(&e.value) > 1) {
            e.used = g;
        }
        self.cache.retain(|_, e| fresh(g, e.used));
        if self.last.as_ref().is_some_and(|l| l.stack.strong_count() == 0) {
            self.last = None;
        }
    }
}

impl Source {
    /// The paragraph of this earlier text whose layout `p` of `text` would get, sharing
    /// `prefix` bytes from the start and `suffix` from the end with it: the same bytes, the same
    /// blocks and the same spans and pills where they lie in it, as its layout's key covers.
    /// `at` is where the last one was found, the place to look first.
    fn same<'s>(&self, stack: &'s Stack, text: &str, prefix: usize, suffix: usize, p: &Draft<'_>, at: &mut usize) -> Option<&'s Para> {
        let old_start = if p.end <= prefix {
            p.start
        } else if p.start >= text.len() - suffix {
            p.start + self.text.len() - text.len()
        } else {
            return None;
        };
        let paras = &stack.paras;
        if paras.get(*at).is_none_or(|q| q.start != old_start) {
            *at = paras.partition_point(|q| q.start < old_start);
        }
        let q = paras.get(*at).filter(|q| q.start == old_start && q.len == p.end - p.start)?;
        *at += 1;
        let local = |s: &Span, start: usize, len: usize| {
            ((s.start as usize).max(start) - start, ((s.end as usize).min(start + len)) - start, s.mono, s.weight.map(f32::to_bits), s.size.map(f32::to_bits), s.italic)
        };
        let pill = |c: &PillChip, start: usize| (c.start as usize - start, c.end as usize - start, c.icon.is_some());
        let (spans, pills) = (&self.spans[q.spans.clone()], &self.pills[q.pills.clone()]);
        (clip(&self.blocks, q.blocks.clone()) == p.blocks(q.breaks)
            && spans.len() == p.spans.len()
            && spans.iter().zip(p.spans).all(|(a, b)| local(a, q.start, q.len) == local(b, p.start, q.len))
            && pills.len() == p.pills.len()
            && pills.iter().zip(p.pills).all(|(a, b)| pill(a, q.start) == pill(b, p.start)))
            .then_some(q)
    }
}

/// Where `text`'s stacked paragraphs end: after each `\n`, and at its end.
fn paragraph_ends(text: &str) -> impl Iterator<Item = usize> + '_ {
    text.match_indices('\n').map(|(i, _)| i + 1).filter(move |&e| e < text.len()).chain([text.len()])
}

/// Whether `text` holds a right-to-left letter, which can make a paragraph right to left.
fn right_to_left(text: &str) -> bool {
    !text.is_ascii() && text.chars().any(|c| matches!(c.bidi_class(), BidiClass::R | BidiClass::AL))
}

fn clip<T>(all: &[T], r: Range<usize>) -> &[T] {
    &all[r.start.min(all.len())..r.end.min(all.len())]
}

/// The hard breaks Parley takes in `text` when no style change or inline box falls inside a
/// `\r\n`, which then takes two.
fn likely_breaks(text: &str) -> usize {
    let b = text.as_bytes();
    (0..b.len()).filter(|&i| match b[i] {
        b'\n' => true,
        b'\r' => b.get(i + 1) != Some(&b'\n'),
        0xE2 => b.get(i + 1) == Some(&0x80) && matches!(b.get(i + 2), Some(0xA8 | 0xA9)),
        _ => false,
    }).count()
}

fn hard_breaks(layout: &Layout<Ink>) -> usize {
    layout.lines().filter(|l| l.break_reason() == parley::BreakReason::Explicit).count()
}

/// How many bytes `a` and `b` share from the start, and then from the end.
fn shared(a: &[u8], b: &[u8]) -> (usize, usize) {
    let n = a.len().min(b.len());
    let mut prefix = 0;
    while prefix + 64 <= n && a[prefix..prefix + 64] == b[prefix..prefix + 64] { prefix += 64; }
    while prefix < n && a[prefix] == b[prefix] { prefix += 1; }
    let (a, b) = (&a[prefix..], &b[prefix..]);
    let n = a.len().min(b.len());
    let mut suffix = 0;
    while suffix + 64 <= n && a[a.len() - suffix - 64..a.len() - suffix] == b[b.len() - suffix - 64..b.len() - suffix] { suffix += 64; }
    while suffix < n && a[a.len() - suffix - 1] == b[b.len() - suffix - 1] { suffix += 1; }
    (prefix, suffix)
}

pub(crate) struct Stack {
    paras: Box<[Para]>,
    full_width: f32,
    height: f32,
    lines: usize,
    /// Its paragraphs have no pills or indented lines (see [`cursor_from_point`]).
    plain: bool,
    /// Unwrapped, Parley ends each line's box at the widest line's width.
    unwrapped_width: Option<f32>,
}

struct Para {
    layout: Rc<Layout<Ink>>,
    start: usize,
    len: usize,
    y: f32,
    line: usize,
    lines: usize,
    /// Its lines' heights summed, as Parley sums them.
    height: f64,
    plain: bool,
    spans: Range<usize>,
    pills: Range<usize>,
    /// The text's blocks it laid out with, and the hard breaks Parley took in it.
    blocks: Range<usize>,
    breaks: usize,
}

/// One layout of a [`TextLayout`] and where it sits in the text. Parley's bytes, inks, pill
/// indices and y for it are its own; these map them to the text's.
#[derive(Clone, Copy)]
pub(crate) struct Piece<'a> {
    pub(crate) layout: &'a Rc<Layout<Ink>>,
    pub(crate) start: usize,
    pub(crate) y: f32,
    /// Its first line's index in the text, and the lines it shows from there.
    line: usize,
    pub(crate) lines: usize,
    /// Its runs of the text's spans and pills, or all of them for a text laid out whole.
    spans: Option<&'a Range<usize>>,
    pills: Option<&'a Range<usize>>,
}

impl<'a> Piece<'a> {
    fn of(p: &'a Para) -> Self {
        Piece { layout: &p.layout, start: p.start, y: p.y, line: p.line, lines: p.lines, spans: Some(&p.spans), pills: Some(&p.pills) }
    }

    fn whole(layout: &'a Rc<Layout<Ink>>) -> Self {
        Piece { layout, start: 0, y: 0.0, line: 0, lines: layout.len(), spans: None, pills: None }
    }

    /// The spans of `all` (the text's) its inks index.
    pub(crate) fn spans<'s>(&self, all: &'s [Span]) -> &'s [Span] {
        self.spans.map_or(all, |r| all.get(r.clone()).unwrap_or(&[]))
    }

    /// The pills of `all` (the text's) its pill boxes index.
    pub(crate) fn pills<'s>(&self, all: &'s [PillChip]) -> &'s [PillChip] {
        self.pills.map_or(all, |r| all.get(r.clone()).unwrap_or(&[]))
    }

    /// The index in the text's pills of the first one [`Piece::pills`] gives.
    pub(crate) fn first_pill(&self) -> usize {
        self.pills.map_or(0, |r| r.start)
    }

    /// The text's ink for this layout's `ink`.
    pub(crate) fn ink(&self, ink: u16) -> u16 {
        match self.spans {
            Some(r) if ink > 0 => ink + r.start as u16,
            _ => ink,
        }
    }

    fn shift(&self, mut bb: BoundingBox) -> BoundingBox {
        bb.y0 += self.y as f64;
        bb.y1 += self.y as f64;
        bb
    }
}

/// One line of a [`TextLayout`], with its text range and vertical metrics in the whole text.
#[derive(Clone)]
pub(crate) struct TextLine<'a> {
    line: parley::Line<'a, Ink>,
    bytes: Range<usize>,
    start: usize,
    y: f32,
    inline_max: Option<f32>,
    rtl: bool,
}

impl<'a> TextLine<'a> {
    pub(crate) fn metrics(&self) -> parley::LineMetrics {
        let mut m = *self.line.metrics();
        m.baseline += self.y;
        m.block_min_coord += self.y;
        m.block_max_coord += self.y;
        m.content_block_min_coord += self.y;
        m.content_block_max_coord += self.y;
        if let Some(max) = self.inline_max {
            m.inline_max_coord = max;
        }
        m
    }

    /// Its bytes in the text: none for a line holding only an inline box, where the line before it ended.
    pub(crate) fn text_range(&self) -> Range<usize> {
        self.bytes.start + self.start..self.bytes.end + self.start
    }

    pub(crate) fn break_reason(&self) -> parley::BreakReason { self.line.break_reason() }

    /// Whether its paragraph reads right to left.
    pub(crate) fn is_rtl(&self) -> bool { self.rtl }

    /// Parley's line: its x geometry is the text's, but its bytes are its paragraph's
    /// ([`Self::byte`] maps them) and so is its y.
    pub(crate) fn parley(&self) -> &parley::Line<'a, Ink> { &self.line }

    /// The text's byte for Parley's `local` byte on this line.
    pub(crate) fn byte(&self, local: usize) -> usize { local + self.start }

    /// The tallest ascent and descent of the runs on this line that hold bytes of `a..b`.
    pub(crate) fn ink_extent(&self, a: usize, b: usize) -> (f32, f32) {
        let (mut asc, mut desc) = (0.0f32, 0.0f32);
        for run in self.line.runs() {
            let r = run.text_range();
            if self.byte(r.start) < b && a < self.byte(r.end) {
                let fm = run.font_metrics();
                (asc, desc) = (asc.max(fm.ascent), desc.max(fm.descent.abs()));
            }
        }
        (asc, desc)
    }
}

impl From<Rc<Layout<Ink>>> for TextLayout {
    fn from(layout: Rc<Layout<Ink>>) -> Self { TextLayout(Laid::Whole(layout)) }
}

impl TextLayout {
    #[cfg(test)]
    pub(crate) fn as_whole(&self) -> Option<&Rc<Layout<Ink>>> {
        match &self.0 {
            Laid::Whole(l) => Some(l),
            Laid::Stacked(_) => None,
        }
    }

    /// Identifies the layout: the same while the text is laid out the same.
    pub(crate) fn id(&self) -> usize {
        match &self.0 {
            Laid::Whole(l) => Rc::as_ptr(l) as usize,
            Laid::Stacked(s) => Rc::as_ptr(s) as usize,
        }
    }

    pub(crate) fn full_width(&self) -> f32 {
        match &self.0 {
            Laid::Whole(l) => l.full_width(),
            Laid::Stacked(s) => s.full_width,
        }
    }

    pub(crate) fn height(&self) -> f32 {
        match &self.0 {
            Laid::Whole(l) => l.height(),
            Laid::Stacked(s) => s.height,
        }
    }

    /// Lines in the whole text.
    pub(crate) fn len(&self) -> usize {
        match &self.0 {
            Laid::Whole(l) => l.len(),
            Laid::Stacked(s) => s.lines,
        }
    }

    /// Its layouts, top to bottom.
    pub(crate) fn pieces(&self) -> impl DoubleEndedIterator<Item = Piece<'_>> + '_ {
        let (whole, stacked) = match &self.0 {
            Laid::Whole(l) => (Some(Piece::whole(l)), &[][..]),
            Laid::Stacked(s) => (None, &s.paras[..]),
        };
        whole.into_iter().chain(stacked.iter().map(Piece::of))
    }

    /// The layouts whose lines may meet `band`, a span of the text's y (device px).
    pub(crate) fn pieces_in_band(&self, band: Range<f32>) -> impl Iterator<Item = Piece<'_>> + '_ {
        let first = match &self.0 {
            Laid::Whole(_) => 0,
            Laid::Stacked(s) => s.paras.partition_point(|p| p.y <= band.start).saturating_sub(1),
        };
        self.pieces().skip(first).take_while(move |p| p.y <= band.end)
    }

    pub(crate) fn lines(&self) -> impl DoubleEndedIterator<Item = TextLine<'_>> + '_ {
        let inline_max = self.unwrapped_width();
        self.pieces().flat_map(move |p| (0..p.lines).filter_map(move |i| Some(TextLine { line: p.layout.get(i)?, bytes: line_bytes(p.layout, i), start: p.start, y: p.y, inline_max, rtl: p.layout.is_rtl() })))
    }

    fn unwrapped_width(&self) -> Option<f32> {
        match &self.0 {
            Laid::Whole(_) => None,
            Laid::Stacked(s) => s.unwrapped_width,
        }
    }

    pub(crate) fn line(&self, index: usize) -> Option<TextLine<'_>> {
        match &self.0 {
            Laid::Whole(l) => Some(TextLine { line: l.get(index)?, bytes: line_bytes(l, index), start: 0, y: 0.0, inline_max: None, rtl: l.is_rtl() }),
            Laid::Stacked(s) => {
                let p = s.para_of_line(index);
                let i = index.checked_sub(p.line).filter(|&i| i < p.lines)?;
                Some(TextLine { line: p.layout.get(i)?, bytes: line_bytes(&p.layout, i), start: p.start, y: p.y, inline_max: s.unwrapped_width, rtl: p.layout.is_rtl() })
            }
        }
    }

    /// The caret box at byte `byte` on its `affinity` side, `size` device px wide.
    pub(crate) fn caret_box(&self, byte: usize, affinity: Affinity, size: f32) -> BoundingBox {
        let at = self.at_byte(byte);
        at.shift(cursor_geometry(&Cursor::from_byte_index(at.layout, byte - at.start, affinity), at.layout, size))
    }

    /// The caret one stop to the left (`left`) or right of `byte` on its `affinity` side, as the
    /// text draws, or `None` at the text's end in that direction. Off a line's edge it goes on in
    /// reading order: to the next line's start where the paragraph reads toward that edge, else to
    /// the previous line's last character.
    pub(crate) fn step(&self, byte: usize, affinity: Affinity, left: bool) -> Option<(usize, Affinity)> {
        let at = self.at_byte(byte);
        let cur = Cursor::from_byte_index(at.layout, byte - at.start, affinity);
        if let Some(to) = step_on_line(at.layout, cur, left) {
            return Some((to.index() + at.start, to.affinity()));
        }
        let line = at.line + drawn(at.layout, cur).0.unwrap_or(at.lines).min(at.lines.saturating_sub(1));
        if left == at.layout.is_rtl() {
            return Some((self.line(line + 1)?.text_range().start, Affinity::Downstream));
        }
        let start = self.line(line)?.text_range().start;
        Some((self.cluster_at(start.checked_sub(1)?)?.0.start, Affinity::Downstream))
    }

    /// The bytes of the clusters drawn left and right of the caret at `byte` on its `affinity`
    /// side, `None` past its line's edge.
    pub(crate) fn sides(&self, byte: usize, affinity: Affinity) -> [Option<Range<usize>>; 2] {
        let at = self.at_byte(byte);
        let cur = Cursor::from_byte_index(at.layout, byte - at.start, affinity);
        let line = drawn(at.layout, cur).0;
        cur.visual_clusters(at.layout).map(|c| {
            let c = c.filter(|c| Some(c.path().line_index()) == line)?;
            let r = c.text_range();
            Some(r.start + at.start..r.end + at.start)
        })
    }

    /// The caret at the left (`left`) or right end of the line the caret at `byte` on its
    /// `affinity` side draws on. At a soft wrap that is the line's own end, upstream of the next
    /// line's start; at a hard break it is before the break, as past it is the next line.
    pub(crate) fn line_side(&self, byte: usize, affinity: Affinity, left: bool) -> (usize, Affinity) {
        let at = self.at_byte(byte);
        let past_break = |c: Cursor| c.index().checked_sub(1).and_then(|i| parley::Cluster::from_byte_index(at.layout, i)).is_some_and(|c| c.is_hard_line_break());
        let mut cur = Cursor::from_byte_index(at.layout, byte - at.start, affinity);
        let line = drawn(at.layout, cur).0;
        let mut end = cur;
        loop {
            let next = if left { cur.previous_visual(at.layout) } else { cur.next_visual(at.layout) };
            if next == cur || drawn(at.layout, next).0 != line {
                return (end.index() + at.start, end.affinity());
            }
            cur = next;
            if !past_break(cur) {
                end = cur;
            }
        }
    }

    /// Whether the paragraph holding `byte` reads right to left.
    pub(crate) fn is_rtl(&self, byte: usize) -> bool {
        self.at_byte(byte).layout.is_rtl()
    }

    /// The byte and affinity a press at `(x, y)` (device px) puts the caret at.
    pub(crate) fn hit(&self, x: f32, y: f32) -> (usize, Affinity) {
        let s = match &self.0 {
            Laid::Whole(l) => {
                let cur = cursor_from_point(l, x, y);
                return (cur.index(), cur.affinity());
            }
            Laid::Stacked(s) => s,
        };
        let i = s.paras.partition_point(|p| p.y <= y).saturating_sub(1);
        let p = &s.paras[i];
        let mut local = y - p.y;
        // A press below a paragraph's lines, above the next paragraph's, stays on its last line.
        if i + 1 < s.paras.len() && let Some(l) = p.layout.get(p.lines.saturating_sub(1)) {
            local = local.min(l.metrics().block_max_coord - 0.5);
        }
        let cur = cursor_from_point_in(&p.layout, x, local, s.plain);
        (cur.index() + p.start, cur.affinity())
    }

    /// The cluster holding byte `byte`: its bytes, and whether it is a hard line break.
    pub(crate) fn cluster_at(&self, byte: usize) -> Option<(Range<usize>, bool)> {
        let at = self.at_byte(byte);
        let cl = parley::Cluster::from_byte_index(at.layout, byte - at.start)?;
        let r = cl.text_range();
        Some((r.start + at.start..r.end + at.start, cl.is_hard_line_break()))
    }

    /// Start (`!end`) or end (`end`) byte of the visual line holding `byte`.
    pub(crate) fn line_edge(&self, byte: usize, end: bool) -> usize {
        let at = self.at_byte(byte);
        line_edge(at.layout, byte - at.start, end) + at.start
    }

    /// The caret on the line above (`!down`) or below (`down`) the caret at `byte` on its
    /// `affinity` side, at the x it draws at: `(0, Downstream)` from the first line up and `None`
    /// from the last line down, as [`move_line`] moves in one layout.
    pub(crate) fn move_line(&self, byte: usize, affinity: Affinity, down: bool) -> Option<(usize, Affinity)> {
        let s = match &self.0 {
            Laid::Whole(l) => return move_line(l, byte, affinity, down),
            Laid::Stacked(s) => s,
        };
        let p = s.para_at_byte(byte);
        let bb = cursor_geometry(&Cursor::from_byte_index(&p.layout, byte - p.start, affinity), &p.layout, 0.0);
        let y = ((bb.y0 + bb.y1) * 0.5) as f32;
        let local = line_for_offset(&p.layout, y).map(|(i, _)| i).filter(|&i| i < p.lines);
        let at = p.line + local.unwrap_or(p.lines.saturating_sub(1));
        if !down && at == 0 {
            return Some((0, Affinity::Downstream));
        }
        if down && at + 1 >= s.lines {
            return None;
        }
        let to = if down { at + 1 } else { at - 1 };
        let q = s.para_of_line(to);
        let m = *q.layout.get(to - q.line)?.metrics();
        let hit = cursor_from_point_in(&q.layout, bb.x0 as f32, (m.block_min_coord + m.block_max_coord) * 0.5, s.plain);
        Some((hit.index() + q.start, hit.affinity()))
    }

    /// Line boxes (`(BoundingBox, line index)`) covering bytes `a..b`, as [`range_boxes`] makes
    /// them for one layout.
    pub(crate) fn range_boxes(&self, a: usize, b: usize, pill_boxes: bool) -> Vec<(BoundingBox, usize)> {
        let s = match &self.0 {
            Laid::Whole(l) => return range_boxes(l, a, b, pill_boxes),
            Laid::Stacked(s) => s,
        };
        let mut out = Vec::new();
        if a >= b {
            return out;
        }
        let first = s.paras.partition_point(|p| p.start + p.len <= a);
        for p in s.paras[first..].iter().take_while(|p| p.start < b) {
            let (la, lb) = (a.max(p.start) - p.start, b.min(p.start + p.len) - p.start);
            let at = Piece::of(p);
            out.extend(range_boxes(&p.layout, la, lb, pill_boxes).into_iter()
                .filter(|&(_, i)| i < p.lines)
                .map(|(bb, i)| (at.shift(bb), i + p.line)));
        }
        out
    }

    /// The layout holding byte `byte`, the next one at a paragraph's end.
    fn at_byte(&self, byte: usize) -> Piece<'_> {
        match &self.0 {
            Laid::Whole(l) => Piece::whole(l),
            Laid::Stacked(s) => Piece::of(s.para_at_byte(byte)),
        }
    }
}

impl Stack {
    fn para_at_byte(&self, byte: usize) -> &Para {
        &self.paras[self.paras.partition_point(|p| p.start <= byte).saturating_sub(1)]
    }

    fn para_of_line(&self, index: usize) -> &Para {
        &self.paras[self.paras.partition_point(|p| p.line <= index).saturating_sub(1)]
    }
}

/// Whether `text` lays out as stacked paragraphs: it has two or more, and its spans are
/// sorted and apart (as flattened spans are), so each paragraph's spans are a run of them.
/// Lines align within the wrap width, or start-aligned left to right without one: Parley aligns
/// unwrapped lines to the widest, which a paragraph doesn't know.
fn stacks(text: &str, spans: &[Span], max_width: Option<f32>, align: Alignment) -> bool {
    text.len() > 1
        && text.as_bytes()[..text.len() - 1].contains(&b'\n')
        && spans.windows(2).all(|w| w[0].end <= w[1].start)
        && (max_width.is_some() || align == Alignment::Start && !right_to_left(text))
}

impl TextSystem {
    /// `text` laid out, stacked by paragraph when it [`stacks`], else as one layout
    /// ([`Self::layout_keyed`]). `shape` is the whole text's [`shape_key`] when known.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn layout_text(
        &mut self,
        shape: Option<u64>,
        text: &str,
        spans: &[Span],
        blocks: &[crate::edit::Block],
        pills: &[PillChip],
        style: TextStyle,
        scale: f32,
        max_width: Option<f32>,
        align: Alignment,
        wrapping: Wrapping,
        cache: bool,
    ) -> TextLayout {
        self.follow_language();
        if !cache || !stacks(text, spans, max_width, align) {
            return self.layout_keyed(shape, text, spans, blocks, pills, style, scale, max_width, align, wrapping, cache).into();
        }
        let shape = shape.unwrap_or_else(|| shape_key(text, spans, style, scale, wrapping, pills));
        let has_indented_blocks = blocks.iter().any(|&b| b != crate::edit::Block::Paragraph);
        let key = hash::break_key(shape, max_width, align, has_indented_blocks.then_some(blocks));
        let g = self.generation;
        if let Some(e) = self.stacks.cache.get_mut(&key) {
            e.used = g;
            return TextLayout(Laid::Stacked(e.value.clone()));
        }
        let stack = Rc::new(self.stack(text, spans, blocks, pills, style, scale, max_width, align, wrapping));
        let cost = stack.paras.len() * size_of::<Para>();
        self.stacks.cache.insert(key, Entry::with_cost(stack.clone(), g, cost, shape));
        self.stacks.last = Some(Source {
            stack: Rc::downgrade(&stack),
            text: text.into(),
            spans: spans.into(),
            pills: pills.into(),
            blocks: blocks.into(),
            style, scale, max_width, align, wrapping,
        });
        TextLayout(Laid::Stacked(stack))
    }

    #[allow(clippy::too_many_arguments)]
    fn stack(
        &mut self,
        text: &str,
        spans: &[Span],
        blocks: &[crate::edit::Block],
        pills: &[PillChip],
        style: TextStyle,
        scale: f32,
        max_width: Option<f32>,
        align: Alignment,
        wrapping: Wrapping,
    ) -> Stack {
        let (mut local_spans, mut local_pills) = (std::mem::take(&mut self.stacks.spans), std::mem::take(&mut self.stacks.pills));
        let last = self.stacks.last.take().filter(|l| {
            (l.style, l.scale, l.max_width, l.align, l.wrapping) == (style, scale, max_width, align, wrapping)
        }).and_then(|l| Some((l.stack.upgrade()?, l)));
        let (prefix, suffix) = last.as_ref().map_or((0, 0), |(_, l)| shared(l.text.as_bytes(), text.as_bytes()));
        let mut paras = Vec::with_capacity(last.as_ref().map_or(text.len() / 32 + 1, |(old, _)| old.paras.len() + 1));
        let mut at = 0;
        let (mut y, mut lines, mut full_width, mut width, mut all_plain) = (0f64, 0usize, 0f32, 0f32, true);
        let (mut s, mut q, mut start, mut block) = (0usize, 0usize, 0usize, 0usize);
        for end in paragraph_ends(text) {
            let piece = &text[start..end];
            while s < spans.len() && (spans[s].end as usize) <= start { s += 1; }
            let s1 = s + spans[s..].iter().take_while(|sp| (sp.start as usize) < end).count();
            while q < pills.len() && (pills[q].start as usize) < start { q += 1; }
            let q1 = q + pills[q..].iter().take_while(|p| (p.start as usize) < end).count();
            let draft = Draft { start, end, spans: &spans[s..s1], pills: &pills[q..q1], blocks, block, last: end == text.len() };
            let old = last.as_ref().and_then(|(old, l)| l.same(old, text, prefix, suffix, &draft, &mut at));
            let (layout, breaks) = match old {
                Some(old) => (old.layout.clone(), old.breaks),
                None => {
                    local_spans.clear();
                    local_spans.extend(draft.spans.iter().map(|sp| Span {
                        start: (sp.start as usize).max(start) as u32 - start as u32,
                        end: (sp.end as usize).min(end) as u32 - start as u32,
                        ..sp.clone()
                    }));
                    local_pills.clear();
                    local_pills.extend(draft.pills.iter().map(|p| PillChip { start: p.start - start as u32, end: p.end.saturating_sub(start as u32), ..p.clone() }));
                    let lay = |ts: &mut Self, breaks| ts.layout_keyed(None, piece, &local_spans, draft.blocks(breaks), &local_pills, style, scale, max_width, align, wrapping, true);
                    let likely = likely_breaks(piece);
                    let layout = lay(self, likely);
                    let breaks = hard_breaks(&layout);
                    let layout = if draft.blocks(breaks) == draft.blocks(likely) { layout } else { lay(self, breaks) };
                    (layout, breaks)
                }
            };
            let shown = if end == text.len() { layout.len() } else { layout.len().saturating_sub(1) };
            let (height, is_plain) = match old.filter(|o| o.lines == shown) {
                Some(o) => (o.height, o.plain),
                None => (layout.lines().take(shown).map(|l| l.metrics().line_height as f64).sum::<f64>(), plain(&layout)),
            };
            let top = y.round() as f32;
            y += height;
            full_width = full_width.max(layout.full_width());
            width = width.max(layout.width());
            all_plain &= is_plain;
            let laid_with = block..block + breaks + draft.last as usize;
            paras.push(Para { layout, start, len: piece.len(), y: top, line: lines, lines: shown, height, plain: is_plain, spans: s..s1, pills: q..q1, blocks: laid_with, breaks });
            lines += shown;
            (q, start, block) = (q1, end, block + breaks);
        }
        (self.stacks.spans, self.stacks.pills) = (local_spans, local_pills);
        let unwrapped_width = max_width.is_none().then_some(width);
        Stack { paras: paras.into_boxed_slice(), full_width, height: y as f32, lines, plain: all_plain, unwrapped_width }
    }
}

#[cfg(test)]
#[path = "paragraphs_tests.rs"]
mod tests;
