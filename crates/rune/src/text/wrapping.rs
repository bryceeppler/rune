/// Where wrapped text may break a line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum Wrapping {
    /// Between words only.
    #[default]
    Words,
    /// Between words, and inside a word too wide for the line (`overflow-wrap: anywhere`).
    Anywhere,
    /// Between any two characters (`word-break: break-all`).
    BreakAll,
}

impl Wrapping {
    /// At least [`Wrapping::Anywhere`], keeping [`Wrapping::BreakAll`].
    pub(crate) fn anywhere(self) -> Wrapping {
        match self {
            Wrapping::Words => Wrapping::Anywhere,
            kept => kept,
        }
    }
}
