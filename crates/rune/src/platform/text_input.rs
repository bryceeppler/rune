
use std::ops::Range;

#[cfg(test)]
#[path = "text_input_tests.rs"]
mod tests;

pub(crate) type Utf16Range = Range<usize>;

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Document {
    text: String,
    selection: Range<usize>,
    marked: Option<Marked>,
}

#[derive(Clone, Debug, PartialEq)]
struct Marked {
    text: String,
    selected: Utf16Range,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Replacement {
    pub bytes: Range<usize>,
    pub expected: String,
    pub delete: bool,
}

impl Document {
    pub(crate) fn new(text: &str, selection: Range<usize>, preedit: Option<&(String, Option<(usize, usize)>)>) -> Document {
        let mut document = Document::default();
        document.refresh(text, selection, preedit);
        document
    }

    pub(crate) fn refresh(&mut self, text: &str, selection: Range<usize>, preedit: Option<&(String, Option<(usize, usize)>)>) {
        if self.text != text {
            self.text.clear();
            self.text.push_str(text);
        }
        let start = floor(text, selection.start.min(selection.end).min(text.len()));
        let end = ceil(text, selection.start.max(selection.end).min(text.len()));
        self.selection = start..end;
        self.marked = preedit.filter(|(s, _)| !s.is_empty()).map(|(s, cursor)| {
            let (a, b) = cursor.unwrap_or((s.len(), s.len()));
            Marked { text: s.clone(), selected: units(s, floor(s, a.min(s.len())))..units(s, floor(s, b.min(s.len()))) }
        });
    }

    pub(crate) fn selected_range(&self) -> Option<Utf16Range> {
        let start = units(&self.text, self.selection.start);
        Some(match &self.marked {
            Some(m) => start + m.selected.start..start + m.selected.end,
            None => start..units(&self.text, self.selection.end),
        })
    }

    pub(crate) fn marked_range(&self) -> Option<Utf16Range> {
        let start = units(&self.text, self.selection.start);
        self.marked.as_ref().map(|m| start..start + m.text.encode_utf16().count())
    }

    pub(crate) fn substring(&self, wanted: Utf16Range) -> Option<(String, Utf16Range)> {
        let composed = self.composed();
        let total = composed.encode_utf16().count();
        if wanted.start > total { return None; }
        let a = byte(&composed, wanted.start, false);
        let b = byte(&composed, wanted.end.min(total).max(wanted.start), true);
        Some((composed[a..b].to_owned(), units(&composed, a)..units(&composed, b)))
    }

    pub(crate) fn replacement(&self, wanted: Utf16Range, delete: bool) -> Option<Replacement> {
        if self.marked.is_some() { return None; }
        let total = self.text.encode_utf16().count();
        if wanted.start > total { return None; }
        let a = byte(&self.text, wanted.start, false);
        let b = byte(&self.text, wanted.end.min(total).max(wanted.start), true);
        Some(Replacement { bytes: a..b, expected: self.text[a..b].to_owned(), delete })
    }

    pub(crate) fn mark(&mut self, string: &str, selected: Utf16Range, replacing: Option<&Replacement>) {
        if let Some(r) = replacing { self.splice(r.bytes.clone(), ""); }
        let length = string.encode_utf16().count();
        self.marked = (!string.is_empty()).then(|| Marked {
            text: string.to_owned(),
            selected: selected.start.min(length)..selected.end.min(length).max(selected.start.min(length)),
        });
    }

    pub(crate) fn insert(&mut self, string: &str, replacing: Option<&Replacement>) {
        self.marked = None;
        let at = replacing.map_or_else(|| self.selection.clone(), |r| r.bytes.clone());
        self.splice(at, string);
    }

    pub(crate) fn unmark(&mut self) {
        self.marked = None;
    }

    fn splice(&mut self, bytes: Range<usize>, with: &str) {
        let a = floor(&self.text, bytes.start.min(self.text.len()));
        let b = ceil(&self.text, bytes.end.min(self.text.len()).max(a));
        self.text.replace_range(a..b, with);
        self.selection = a + with.len()..a + with.len();
    }

    fn composed(&self) -> std::borrow::Cow<'_, str> {
        match &self.marked {
            Some(m) => {
                let (a, b) = (self.selection.start, self.selection.end);
                format!("{}{}{}", &self.text[..a], m.text, &self.text[b..]).into()
            }
            None => self.text.as_str().into(),
        }
    }
}

fn units(s: &str, bytes: usize) -> usize {
    s[..bytes].encode_utf16().count()
}

fn byte(s: &str, at: usize, after: bool) -> usize {
    let mut counted = 0;
    for (i, c) in s.char_indices() {
        let next = counted + c.len_utf16();
        if at < next {
            return if at == counted || !after { i } else { i + c.len_utf8() };
        }
        counted = next;
    }
    s.len()
}

fn floor(s: &str, mut at: usize) -> usize {
    while !s.is_char_boundary(at) { at -= 1; }
    at
}

fn ceil(s: &str, mut at: usize) -> usize {
    while !s.is_char_boundary(at) { at += 1; }
    at
}
