//! A text field's selection: two carets, each a byte and the side of it the caret draws on.

use std::ops::Range;

use parley::layout::Affinity;

/// Which character a caret at a byte keeps to. Where the text changes direction, or at a soft
/// wrap, one byte draws in two places: before the character after it, or after the one before it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CaretSide {
    /// Before the character after the byte.
    #[default]
    Next,
    /// After the character before the byte.
    Previous,
}

/// A place in a text: a byte on a char boundary, and the side of it the caret draws on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Caret {
    pub byte: usize,
    pub side: CaretSide,
}

impl Caret {
    /// A caret at `byte`, drawn before the character after it.
    pub const fn at(byte: usize) -> Caret {
        Caret { byte, side: CaretSide::Next }
    }

    pub(crate) fn affinity(self) -> Affinity {
        match self.side {
            CaretSide::Next => Affinity::Downstream,
            CaretSide::Previous => Affinity::Upstream,
        }
    }

    pub(crate) fn of(byte: usize, affinity: Affinity) -> Caret {
        let side = match affinity {
            Affinity::Downstream => CaretSide::Next,
            Affinity::Upstream => CaretSide::Previous,
        };
        Caret { byte, side }
    }
}

/// What a text field has selected: the caret it started from and the caret that moves, one
/// caret when nothing is selected.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Selection {
    pub anchor: Caret,
    pub focus: Caret,
}

impl Selection {
    /// Nothing selected, the caret at `at`.
    pub const fn caret(at: Caret) -> Selection {
        Selection { anchor: at, focus: at }
    }

    /// From byte `anchor` to byte `focus`, each caret drawn before the character after it.
    pub const fn bytes(anchor: usize, focus: usize) -> Selection {
        Selection { anchor: Caret::at(anchor), focus: Caret::at(focus) }
    }

    pub fn is_collapsed(&self) -> bool {
        self.anchor.byte == self.focus.byte
    }

    /// The selected bytes, in text order.
    pub fn range(&self) -> Range<usize> {
        self.anchor.byte.min(self.focus.byte)..self.anchor.byte.max(self.focus.byte)
    }
}
