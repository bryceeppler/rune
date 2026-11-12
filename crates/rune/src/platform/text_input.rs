//! The focused text field as macOS's text input client answers for it. winit's client answers
//! no selection and no text, and ignores `replacementRange`.

use std::ops::Range;

#[cfg(test)]
#[path = "text_input_tests.rs"]
mod tests;

/// A range of the composed text in UTF-16 units.
pub(crate) type Units = Range<usize>;

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Document;

/// What the field does before the input method's next event: select `bytes`, which held
/// `expected` when the input method asked, and delete them when a composition takes their place.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Replacement {
    pub bytes: Range<usize>,
    pub expected: String,
    pub delete: bool,
}

impl Document {
    pub(crate) fn new(_: &str, _: Range<usize>, _: Option<&(String, Option<(usize, usize)>)>) -> Document {
        Document
    }

    pub(crate) fn selected_range(&self) -> Option<Units> {
        None
    }

    pub(crate) fn marked_range(&self) -> Option<Units> {
        None
    }

    pub(crate) fn substring(&self, _: Units) -> Option<(String, Units)> {
        None
    }

    pub(crate) fn replacement(&self, _: Units, _: bool) -> Option<Replacement> {
        None
    }

    pub(crate) fn mark(&mut self, _: &str, _: Units, _: Option<&Replacement>) {}

    pub(crate) fn insert(&mut self, _: &str, _: Option<&Replacement>) {}

    pub(crate) fn unmark(&mut self) {}
}
