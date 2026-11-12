//! The rich-text model's value types: inline marks, paragraph blocks and their palette.

use std::ops::Range;

use crate::color::{oklch, oklcha, Color};

/// Maximum nesting level (`0..=MAX_LIST_LEVEL`) for [`Block::Bullet`], [`Block::Numbered`] and
/// [`Block::Hanging`].
pub const MAX_LIST_LEVEL: u8 = 3;

/// An inline formatting mark stored alongside [`TextEdit::text`]. More marks may come, so a
/// `match` needs a `_` arm.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Mark {
    Bold,
    Italic,
    Underline,
    Strike,
    Code,
    Link(String),
    /// An atomic inline object (an `@file` or `@agent` mention, an attachment) carrying the
    /// app's id; its text is the label. The caret steps over it, deleting takes it whole, a
    /// selection that touches it covers it, and typing never extends it. A field draws it as a
    /// chip ([`El::pill_style`](crate::El::pill_style), [`El::pill_colors`](crate::El::pill_colors),
    /// [`El::pill_icon`](crate::El::pill_icon)). Insert one with [`TextEdit::insert_pill`].
    Pill(String),
}

impl Mark {
    /// Whether `self` and `other` are the same kind of mark (`Link(_)` matches any `Link(_)`).
    pub fn same_kind(&self, other: &Mark) -> bool {
        match (self, other) {
            (Mark::Bold, Mark::Bold)
            | (Mark::Italic, Mark::Italic)
            | (Mark::Underline, Mark::Underline)
            | (Mark::Strike, Mark::Strike)
            | (Mark::Code, Mark::Code)
            | (Mark::Link(_), Mark::Link(_))
            | (Mark::Pill(_), Mark::Pill(_)) => true,
            _ => false,
        }
    }

    /// The URL when `self` is [`Mark::Link`].
    pub fn link_url(&self) -> Option<&str> {
        match self {
            Mark::Link(url) => Some(url),
            _ => None,
        }
    }

    /// The app's id when `self` is [`Mark::Pill`].
    pub fn pill_id(&self) -> Option<&str> {
        match self {
            Mark::Pill(id) => Some(id),
            _ => None,
        }
    }

    pub(crate) fn is_pill(&self) -> bool {
        matches!(self, Mark::Pill(_))
    }

    pub(crate) fn kind_order(&self) -> u8 {
        match self {
            Mark::Bold => 0,
            Mark::Italic => 1,
            Mark::Underline => 2,
            Mark::Strike => 3,
            Mark::Code => 4,
            Mark::Link(_) => 5,
            Mark::Pill(_) => 6,
        }
    }
}

/// A non-empty byte range `[start, end)` in [`TextEdit::text`] carrying an inline [`Mark`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MarkRange {
    pub start: usize,
    pub end: usize,
    pub mark: Mark,
}

impl MarkRange {
    pub fn new(range: Range<usize>, mark: Mark) -> Self {
        Self { start: range.start, end: range.end, mark }
    }

    pub fn range(&self) -> Range<usize> {
        self.start..self.end
    }
}

/// Paragraph-level block attribute for a `\n`-delimited line in [`TextEdit`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Block {
    #[default]
    Paragraph,
    /// Bullet list item (`•`, `◦`, `▪`) at nesting `level` `0..=MAX_LIST_LEVEL`. Inside a
    /// `quote` it draws the quote's bar and its levels step in from the quote's indent.
    Bullet { level: u8, quote: bool },
    /// Numbered list item (`1.`, `2.`, …) at nesting level `0..=MAX_LIST_LEVEL`.
    Numbered(u8),
    /// Indented quote paragraph with a vertical left bar.
    Quote,
    /// Monospace code block line with a background fill; inline marks do not apply.
    Code,
    /// A paragraph that starts with a marker of its own, `marker` bytes long, such as a
    /// Markdown list item's `- [ ] `, at list nesting `level` `0..=MAX_LIST_LEVEL`: its wrapped
    /// lines start under the text after the marker. Inside a `quote` it draws the quote's bar
    /// and its levels step in from the quote's indent. It edits as a [`Block::Paragraph`].
    Hanging { marker: u16, level: u8, quote: bool },
}

impl Block {
    pub fn is_list(self) -> bool {
        matches!(self, Block::Bullet { .. } | Block::Numbered(_))
    }

    pub fn list_level(self) -> Option<u8> {
        match self {
            Block::Bullet { level: l, .. } | Block::Numbered(l) => Some(l),
            _ => None,
        }
    }

    pub fn with_level(self, level: u8) -> Self {
        let l = level.min(MAX_LIST_LEVEL);
        match self {
            Block::Bullet { quote, .. } => Block::Bullet { level: l, quote },
            Block::Numbered(_) => Block::Numbered(l),
            other => other,
        }
    }

    /// Whether this paragraph draws the quote's bar: a [`Block::Quote`], or a
    /// [`Block::Bullet`] or [`Block::Hanging`] inside a quote.
    pub fn quoted(self) -> bool {
        matches!(self, Block::Quote | Block::Bullet { quote: true, .. } | Block::Hanging { quote: true, .. })
    }

    /// Whether `self` and `other` share the same block kind (ignoring list nesting level).
    pub fn same_kind(self, other: Block) -> bool {
        matches!(
            (self, other),
            (Block::Paragraph, Block::Paragraph)
                | (Block::Bullet { .. }, Block::Bullet { .. })
                | (Block::Numbered(_), Block::Numbered(_))
                | (Block::Quote, Block::Quote)
                | (Block::Code, Block::Code)
                | (Block::Hanging { .. }, Block::Hanging { .. })
        )
    }

    /// This block's kind as one bit, for [`TextEdit::allow_blocks`].
    pub(super) fn kind_bit(self) -> u8 {
        1 << match self {
            Block::Paragraph | Block::Hanging { .. } => 0,
            Block::Bullet { .. } => 1,
            Block::Numbered(_) => 2,
            Block::Quote => 3,
            Block::Code => 4,
        }
    }

    /// Left indentation in points (before scale) for this block type, a quoted list item's
    /// counted from the quote's indent.
    pub(crate) fn indent_pt(self, font_size: f32) -> f32 {
        let quote = (font_size * 1.1).round().max(14.0);
        let unit = (font_size * 1.45).round().max(18.0);
        let steps = |level: u8| unit * level.min(MAX_LIST_LEVEL) as f32;
        let in_quote = |quoted: bool| if quoted { quote } else { 0.0 };
        match self {
            Block::Paragraph => 0.0,
            Block::Hanging { level, quote: quoted, .. } => in_quote(quoted) + steps(level),
            Block::Bullet { level, quote: quoted } => in_quote(quoted) + steps(level) + unit,
            Block::Numbered(level) => steps(level) + unit,
            Block::Quote => quote,
            Block::Code => (font_size * 0.75).round().max(10.0),
        }
    }
}

/// A paragraph slice of [`TextEdit`], returned by [`TextEdit::paragraphs`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ParagraphInfo {
    pub index: usize,
    pub range: Range<usize>,
    pub block: Block,
    /// True when this line was split from the previous line by `Shift+Return` (`<br>`).
    pub soft_break: bool,
}

/// Visual palette for rich marks and block attributes inside [`text_input`](crate::text_input).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RichStyle {
    pub code_fg: Option<Color>,
    pub code_bg: Color,
    pub code_block_bg: Color,
    pub link: Color,
    pub quote_bar: Color,
    pub marker: Option<Color>,
}

impl Default for RichStyle {
    fn default() -> Self {
        Self {
            code_fg: None,
            code_bg: oklcha(0.55, 0.02, 260.0, 0.16),
            code_block_bg: oklcha(0.5, 0.02, 260.0, 0.12),
            link: oklch(0.68, 0.16, 250.0),
            quote_bar: oklcha(0.6, 0.04, 255.0, 0.45),
            marker: None,
        }
    }
}
