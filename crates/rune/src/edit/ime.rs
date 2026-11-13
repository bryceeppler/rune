//! Input method events: what the platform's input method (dead keys, CJK, the emoji picker,
//! dictation) composes, for the focused `text_input` or an app's own editor (`App::on_ime`).
use zeroize::Zeroize;

/// One input method event. Plain typing arrives as keys (`Key::Text`); these carry what an
/// input method composed.
#[derive(Clone, Debug, PartialEq)]
pub enum Ime {
    /// The composition so far (marked text), drawn at the caret and not yet part of the text,
    /// with the byte range of its cursor or selection in it. Empty ends the composition.
    Preedit(String, Option<(usize, usize)>),
    /// Text to insert at the caret, replacing the preedit.
    Commit(String),
    /// The window's input method turned on for the caret that takes text (a focused
    /// `text_input` or an `El::ime_cursor` surface), or off again. Informational: Rune turns it
    /// on and off.
    Enabled,
    Disabled,
}

impl Drop for Ime {
    fn drop(&mut self) {
        match self {
            Self::Preedit(text, _) | Self::Commit(text) => text.zeroize(),
            Self::Enabled | Self::Disabled => {}
        }
    }
}

#[derive(Clone, Debug, PartialEq, Hash)]
pub(super) struct Reconverting {
    bytes: (usize, usize),
    selection: crate::Selection,
    ended: bool,
}

impl super::TextEdit {
    pub(crate) fn clear_preedit(&mut self) {
        if let Some((mut text, _)) = self.preedit.take() { text.zeroize(); }
        self.reconverting = None;
    }

    pub(crate) fn reconvert(&mut self, bytes: std::ops::Range<usize>) {
        self.reconverting = Some(Reconverting { bytes: (bytes.start, bytes.end), selection: self.selection, ended: false });
    }

    pub(crate) fn preedit_range(&self) -> (usize, usize) {
        self.reconverting.as_ref()
            .filter(|r| r.selection == self.selection && r.bytes.0 <= r.bytes.1 && self.text.get(r.bytes.0..r.bytes.1).is_some())
            .map_or_else(|| self.range(), |r| r.bytes)
    }

    pub(super) fn follow_preedit(&mut self, preedit: &str) {
        self.reconverting = self.reconverting.take().filter(|r| !r.ended).map(|r| Reconverting { ended: preedit.is_empty(), ..r });
    }

    /// Which model this is, unchanged by edits, undo history or formatting; clones share it.
    pub(crate) fn identity(&self) -> std::sync::Weak<()> {
        std::sync::Arc::downgrade(&self.identity)
    }

    pub(crate) fn is_model(&self, identity: &std::sync::Weak<()>) -> bool {
        identity.ptr_eq(&self.identity())
    }
}
