//! Text inside the accessibility tree: a text field's lines as text runs with each
//! character's place, so a reader moves by character, word and line and a magnifier follows
//! the caret; a selectable text's lines likewise; and a text's inline links as link nodes.

use accesskit::{Action as Ak, Node as AkNode, NodeId as AkId, Rect as AkRect, Role, TextPosition, TextSelection};

use super::*;
use super::a11y_tree::{image_id, line_id, link_id, round};

/// Text longer than this gets no per-character runs (a reader still gets its whole value).
const RUN_MAX_BYTES: usize = 64 * 1024;

/// What a reader hears for a sized pill: the object replacement character.
const OBJECT: &str = "\u{FFFC}";

/// One laid-out line: its bytes in the drawn text, what a reader hears for it, and each
/// character's place in points. A sized pill is one character, [`OBJECT`], over its label's
/// bytes.
pub(crate) struct Line {
    pub range: std::ops::Range<usize>,
    pub rect: [f32; 4],
    pub value: String,
    /// Each character's length in `value`.
    pub lengths: Vec<u8>,
    /// Each character's length in the drawn text.
    pub bytes: Vec<usize>,
    pub positions: Vec<f32>,
    pub widths: Vec<f32>,
    pub word_starts: Vec<u8>,
}

impl Line {
    /// Appends the character drawn from `drawn`, heard as `heard`.
    fn push(&mut self, heard: &str, drawn: usize, position: f32, width: f32) {
        self.value.push_str(heard);
        self.lengths.push(heard.len().min(255) as u8);
        self.bytes.push(drawn);
        self.positions.push(position);
        self.widths.push(width);
    }

    /// Joins drawn bytes `skipped` to the last character, as part of it.
    fn join(&mut self, skipped: &str) {
        let (Some(len), Some(bytes)) = (self.lengths.last_mut(), self.bytes.last_mut()) else { return };
        if self.value.ends_with(OBJECT) { return; }
        self.value.push_str(skipped);
        *len = (*len as usize + skipped.len()).min(255) as u8;
        *bytes += skipped.len();
    }
}

/// `content`'s lines from `layout` (device px at `s`), placed from `origin` in points.
/// `sized` holds the drawn bytes of each sized pill's label.
pub(crate) fn lines(layout: &crate::text::TextLayout, content: &str, s: f32, origin: [f32; 2], sized: &[std::ops::Range<usize>]) -> Vec<Line> {
    let mut out = Vec::new();
    for line in layout.lines() {
        let m = line.metrics();
        let range = line.text_range();
        if range.end > content.len() { break; }
        let mut l = Line { range: range.clone(), rect: [0.0; 4], value: String::new(), lengths: Vec::new(), bytes: Vec::new(), positions: Vec::new(), widths: Vec::new(), word_starts: Vec::new() };
        let mut boxes = crate::text::sized_box_widths(line.parley()).into_iter();
        let mut x = m.offset;
        let mut at = range.start;
        for run in line.parley().runs() {
            for cluster in run.clusters() {
                let r = cluster.text_range();
                let r = line.byte(r.start)..line.byte(r.end);
                if r.start < at || r.end > range.end || r.is_empty() { continue; }
                // Bytes a cluster skipped (an ignorable) join the character before.
                if r.start > at { l.join(&content[at..r.start]); }
                if let Some(label) = sized.iter().find(|p| p.contains(&r.start)) {
                    let (end, width) = (label.end.min(range.end), boxes.next().unwrap_or_default());
                    l.push(OBJECT, end - r.start, (x - m.offset) / s, width / s);
                    x += width;
                    at = end;
                    continue;
                }
                if cluster.is_word_boundary() && !l.lengths.is_empty() && l.lengths.len() < 256 { l.word_starts.push(l.lengths.len() as u8); }
                let adv = cluster.advance();
                l.push(&content[r.clone()], r.end - r.start, (x - m.offset) / s, adv / s);
                x += adv;
                at = r.end;
            }
        }
        if at < range.end {
            if l.lengths.is_empty() { l.push(&content[at..range.end], range.end - at, 0.0, 0.0) } else { l.join(&content[at..range.end]) }
        }
        // AccessKit indexes word starts by u8: a line past 255 characters keeps the first ones.
        l.word_starts.retain(|&w| (w as usize) < l.lengths.len());
        let top = m.block_min_coord / s;
        let height = (m.block_max_coord - m.block_min_coord) / s;
        l.rect = [origin[0] + m.offset / s, origin[1] + top, (x - m.offset) / s, height];
        out.push(l);
    }
    out
}

/// The line and character index of drawn byte `byte`.
fn position(lines: &[Line], byte: usize) -> Option<(usize, usize)> {
    let last = lines.len().checked_sub(1)?;
    let k = lines.iter().position(|l| byte < l.range.end).unwrap_or(last);
    let l = &lines[k];
    let mut at = l.range.start;
    for (c, len) in l.bytes.iter().enumerate() {
        if byte < at + len { return Some((k, c)); }
        at += len;
    }
    Some((k, l.bytes.len()))
}

/// Drawn byte at line `k`'s character `c`.
pub(crate) fn byte_at(lines: &[Line], k: usize, c: usize) -> Option<usize> {
    let l = lines.get(k)?;
    Some(l.range.start + l.bytes.iter().take(c).sum::<usize>())
}

impl<S: 'static> Runtime<S> {
    /// Node `i`'s text as AccessKit sees it: the lines of a field or selectable text, and the
    /// links of any text, pushed after it as its children.
    pub(super) fn a11y_text_nodes(&self, i: usize, w: &a11y_tree::Walk, out: &mut Vec<(AkId, AkNode)>) {
        let n = &self.nodes[i];
        let Some(spec) = n.text.as_deref() else { return };
        let me = out.len() - 1;
        let runs = n.input.is_some() || (spec.selectable && !n.is_secure());
        if runs && let Some(lines) = self.a11y_lines(i) {
            for (k, l) in lines.iter().enumerate() {
                let id = line_id(n.id, k);
                let mut run = AkNode::new(Role::TextRun);
                run.set_value(l.value.as_str());
                run.set_bounds(AkRect {
                    x0: round(l.rect[0] + w.shift[0]), y0: round(l.rect[1] + w.shift[1]),
                    x1: round(l.rect[0] + l.rect[2] + w.shift[0]), y1: round(l.rect[1] + l.rect[3] + w.shift[1]),
                });
                run.set_character_lengths(l.lengths.clone());
                run.set_character_positions(l.positions.clone());
                run.set_character_widths(l.widths.clone());
                run.set_word_starts(l.word_starts.clone());
                out[me].1.push_child(id);
                out.push((id, run));
            }
            if let Some(sel) = self.a11y_selection(i, &lines) { out[me].1.set_text_selection(sel); }
        }
        if !n.is_secure() && !spec.redacted && let (Some(origin), Some(layout)) = (self.a11y_text_origin(i), n.text_layout.as_ref()) {
            let s = self.env.scale;
            let rects = crate::text::pill_chip_rects(layout, &spec.pills, spec.style.size, s, f32::NEG_INFINITY..f32::INFINITY);
            for (k, r) in rects.into_iter().filter(|(k, _)| spec.pills[*k].style.size.is_some()) {
                let mut image = AkNode::new(Role::Image);
                if let Some(description) = &spec.pills[k].style.description { image.set_label(description.as_str()); }
                let [x, y] = [origin[0] + r[0] / s + w.shift[0], origin[1] + r[1] / s + w.shift[1]];
                image.set_bounds(AkRect { x0: round(x), y0: round(y), x1: round(x + r[2] / s), y1: round(y + r[3] / s) });
                let id = image_id(n.id, k);
                out[me].1.push_child(id);
                out.push((id, image));
            }
        }
        if spec.links.is_empty() || spec.redacted { return; }
        let boxes = self.link_boxes(i, self.drawn_box(i));
        for (k, l) in spec.links.iter().enumerate() {
            let Some(r) = boxes.iter().filter(|(j, _)| *j == k).fold(None, |u: Option<[f32; 4]>, (_, r)| {
                Some(u.map_or(*r, |u| [u[0].min(r[0]), u[1].min(r[1]), u[2].max(r[2]), u[3].max(r[3])]))
            }) else { continue };
            let role = crate::a11y::roles::map(l.role, &crate::a11y::roles::RoleCtx {
                pressable: true, input: false, multiline: false, secure: false, scrolls: false, modal: false,
                default_button: false, labelled: true, keyed: true, has_text: true, image: false,
            }).unwrap_or(Role::Link);
            let mut link = AkNode::new(role);
            let words = spec.source_slice(l.start, l.end).unwrap_or("");
            link.set_label(l.label.as_deref().unwrap_or(words));
            link.set_author_id(l.id.as_str());
            if role == Role::Link && l.id.contains("://") { link.set_url(l.id.as_str()); }
            link.set_bounds(AkRect {
                x0: round(r[0] + w.shift[0]), y0: round(r[1] + w.shift[1]),
                x1: round(r[2] + w.shift[0]), y1: round(r[3] + w.shift[1]),
            });
            if self.allowed(i) {
                if spec.on_link.is_some() { link.add_action(Ak::Click); }
                if self.can_focus(i) { link.add_action(Ak::Focus); }
                if spec.on_link_secondary.is_some() { link.add_action(Ak::ShowContextMenu); }
            }
            let id = link_id(n.id, k);
            out[me].1.push_child(id);
            out.push((id, link));
        }
    }

    /// What node `i` draws as text: a field's display text (its mask when secure), else its
    /// content.
    pub(super) fn a11y_drawn_text(&self, i: usize) -> Option<String> {
        let n = &self.nodes[i];
        match n.input.as_deref() {
            Some(input) => Some(input.edit.display().0),
            None => n.text.as_deref().map(|t| t.content.clone()),
        }
    }

    /// Node `i`'s laid-out lines, in points in its scroll viewport's content space (before the
    /// walk's shift).
    pub(super) fn a11y_lines(&self, i: usize) -> Option<Vec<Line>> {
        let n = &self.nodes[i];
        let (spec, layout) = (n.text.as_deref()?, n.text_layout.as_ref()?);
        let content = self.a11y_drawn_text(i)?;
        if content.len() > RUN_MAX_BYTES { return None; }
        let origin = self.a11y_text_origin(i)?;
        Some(lines(layout, &content, self.env.scale, origin, &sized_labels(spec)))
    }

    /// Where node `i`'s text layout starts, in points in its scroll viewport's content space
    /// (before the walk's shift); `None` for elided text, whose layout isn't what it draws.
    fn a11y_text_origin(&self, i: usize) -> Option<[f32; 2]> {
        let n = &self.nodes[i];
        let (spec, layout) = (n.text.as_deref()?, n.text_layout.as_ref()?);
        if n.elided.is_some() { return None; }
        let s = self.env.scale;
        let r = self.drawn[i];
        let (x0, x1) = (r[0] + n.inset[0], r[0] + r[2] - n.inset[2]);
        let mut x = text_x(spec, x0, x1, layout.full_width() / s);
        if let Some(input) = n.input.as_deref() && input.single_line {
            x -= self.input_scroll.get(&n.path).copied().unwrap_or(0.0);
        }
        Some([x, r[1] + n.inset[1]])
    }

    /// A field's caret or selection, in its lines.
    fn a11y_selection(&self, i: usize, lines: &[Line]) -> Option<TextSelection> {
        let n = &self.nodes[i];
        let input = n.input.as_deref()?;
        let edit = &input.edit;
        let (anchor, focus) = if edit.preedit.is_some() {
            let caret = caret_index(edit, &self.a11y_drawn_text(i)?);
            (caret, caret)
        } else {
            (edit.display_index(edit.selection().anchor.byte), edit.display_index(edit.selection().focus.byte))
        };
        let at = |byte: usize| position(lines, byte).map(|(k, c)| TextPosition { node: line_id(n.id, k), character_index: c });
        Some(TextSelection { anchor: at(anchor)?, focus: at(focus)? })
    }

    /// The value and state a text field adds to its node: its text, or with sized pills its
    /// drawn text with each one heard as [`OBJECT`].
    pub(super) fn a11y_input(&self, i: usize, input: &crate::el::InputSpec, node: &mut AkNode) {
        if !input.placeholder.is_empty() { node.set_placeholder(input.placeholder.as_str()); }
        if !input.edit.is_secure() {
            let sized = self.nodes[i].text.as_deref().map(sized_labels).unwrap_or_default();
            if sized.is_empty() {
                node.set_value(input.edit.text.as_str());
            } else {
                let drawn = input.edit.display().0;
                let mut value = String::with_capacity(drawn.len());
                let mut at = 0;
                for label in sized.iter().filter(|l| l.end <= drawn.len()) {
                    value.push_str(&drawn[at..label.start]);
                    value.push_str(OBJECT);
                    at = label.end;
                }
                value.push_str(&drawn[at..]);
                node.set_value(value);
            }
        }
        if input.access.is_none() { node.set_read_only(); }
    }
}

/// The drawn bytes of each sized pill's label in `spec`.
fn sized_labels(spec: &crate::runtime::TextSpec) -> Vec<std::ops::Range<usize>> {
    spec.pills.iter().filter(|p| p.style.size.is_some() && !p.after).map(|p| p.start as usize..p.end as usize).collect()
}
