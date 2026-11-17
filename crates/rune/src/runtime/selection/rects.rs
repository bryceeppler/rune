//! `Cx::selection_rects`: the line boxes of the [`El::selectable`](crate::El::selectable)
//! text selection, for a view that places something over it (a Quote button), and
//! `Cx::pill_rect`: where a field's pill draws, for a bar over a selected photo. Each is a
//! layout read like `Cx::layout`, under a key no element can have, so it settles, replays from
//! `memo` and re-runs the view the same way; the selection's boxes ride along with the rects
//! the view is given.
use super::*;

/// The layout-read key `Cx::selection_rects` reads under.
pub(crate) const SELECTION: &str = "\u{1}selection";
/// The start of the layout-read key `Cx::pill_rect` reads under, then the field's key, a NUL
/// and the pill's id.
pub(crate) const PILL: &str = "\u{1}pill\u{0}";

impl<S> Cx<S> {
    /// The line boxes of the text selection, `[x, y, w, h]` each in window points after
    /// scrolling, in reading order: one per line of each selected text, as its highlight draws
    /// them. Empty (and allocating nothing) when nothing is selected or only a caret is placed.
    /// Boxes aren't clipped to their scroll viewport, so a view can tell when the selection has
    /// scrolled out of sight. Only mounted texts have boxes: the rows of a virtual list that
    /// scrolled away have none. Like [`Cx::layout`], boxes are layout points: an ancestor's
    /// [`El::scale`](crate::El::scale) isn't applied.
    ///
    /// A view that reads it re-runs when a box appears, goes, or moves by more than half a
    /// point: in the same commit when layout moved it, the next frame when the selection
    /// changed or scrolling moved it. While a drag changes the selection that's every frame,
    /// so a toolbar that waits for the release
    /// ([`SelectionChange::dragging`](crate::SelectionChange::dragging)) should only read it
    /// once shown. A `memo` that reads it is rebuilt when the boxes move.
    pub fn selection_rects(&self) -> Vec<[f32; 4]> {
        layout_read::note_read(&mut self.layout_reads.borrow_mut(), SELECTION, None);
        crate::memo::read_selection(&self.layout.selection);
        self.layout.selection.to_vec()
    }

    /// Where the [`Mark::Pill`](crate::Mark::Pill) with id `id` draws in the text field keyed
    /// `key`: a [`sized`](crate::PillStyle::sized) pill's box, or a chip's first line,
    /// `[x, y, w, h]` in the field's own points from its top-left, so it holds still while the
    /// field scrolls. `None` when the field has no such pill. Like [`Cx::layout`], a view that
    /// reads it re-runs when it moves or resizes past half a point.
    #[track_caller]
    pub fn pill_rect(&self, key: &str, id: &str) -> Option<[f32; 4]> {
        self.layout(&format!("{PILL}{key}\u{0}{id}"))
    }
}

impl<S: 'static> Runtime<S> {
    /// [`Cx::pill_rect`] of the pill `id` in the field at `nodes[i]`.
    pub(super) fn pill_rect_in(&self, nodes: &[Node], i: usize, id: &str) -> Option<[f32; 4]> {
        let n = &nodes[i];
        let (input, spec, layout) = (n.input.as_ref()?, n.text.as_ref()?, n.text_layout.as_ref()?);
        let k = spec.pills.iter().position(|p| p.id.as_deref() == Some(id))?;
        let s = self.env.scale;
        let r = crate::text::pill_chip_rect(layout, &spec.pills, k, spec.style.size, s)?;
        let scroll = if input.single_line { self.input_scroll.get(&n.path).copied().unwrap_or(0.0) } else { 0.0 };
        let x = text_x(spec, n.inset[0], n.local.w - n.inset[2], layout.full_width() / s) - scroll;
        Some([x + r[0] / s, n.inset[1] + r[1] / s, r[2] / s, r[3] / s])
    }
}

/// Bytes `a..b` of the text at a path.
pub(crate) type SelectedRange = (String, usize, usize);

/// Whether any box of `a` and `b` differs by more than half a point, or one has a box the
/// other lacks.
pub(crate) fn boxes_moved(a: &[[f32; 4]], b: &[[f32; 4]]) -> bool {
    a.len() != b.len() || a.iter().zip(b).any(|(x, y)| layout_read::moved(Some(*x), Some(*y)))
}

impl<S: 'static> Runtime<S> {
    /// The selection's line boxes laid out in the last commit's nodes.
    pub(super) fn selection_boxes(&self) -> Vec<[f32; 4]> {
        let mut out = Vec::new();
        for (i, a, b) in self.selection_ranges() { self.text_boxes(&self.nodes, i, a, b, &mut out); }
        out
    }

    /// The selection's ranges by the path of each selected text. A measure reads them before it
    /// runs: reusing a `memo` moves its nodes out of the last commit's, leaving placeholders
    /// the selection can't be found in.
    pub(crate) fn selection_by_path(&self) -> Vec<SelectedRange> {
        self.selection_ranges().into_iter().map(|(i, a, b)| (self.nodes[i].path.clone(), a, b)).collect()
    }

    /// The line boxes of `ranges` laid out in a measure's `nodes`, found there by path.
    pub(super) fn selection_boxes_in(&self, nodes: &[Node], ranges: &[SelectedRange]) -> Vec<[f32; 4]> {
        let mut out = Vec::new();
        for (path, a, b) in ranges {
            if let Some(j) = nodes.iter().position(|n| n.path == *path) { self.text_boxes(nodes, j, *a, *b, &mut out); }
        }
        out
    }

    /// Pushes the line boxes of bytes `a..b` of node `j`'s text onto `out`.
    fn text_boxes(&self, nodes: &[Node], j: usize, a: usize, b: usize, out: &mut Vec<[f32; 4]>) {
        let n = &nodes[j];
        let (Some(spec), Some(layout)) = (&n.text, &n.text_layout) else { return };
        let len = spec.content.len();
        let (a, b) = (a.min(len), b.min(len));
        if a >= b { return; }
        let (a, b) = n.elided.map_or((a, b), |e| e.range(a, b));
        let [x, y, w, h] = self.laid_out(nodes, j);
        let s = self.env.scale;
        let [ox, oy] = links::origin(n, spec, layout, [x, y, x + w, y + h], s, 1.0);
        for (bb, _) in layout.range_boxes(a, b, true) {
            let (x0, y0) = (ox + bb.x0 as f32 / s, oy + bb.y0 as f32 / s);
            out.push([x0, y0, (bb.x1 - bb.x0) as f32 / s, (bb.y1 - bb.y0) as f32 / s]);
        }
    }

    /// Once a frame: the selection moved since the view last read it (a drag, a click, code,
    /// scrolling). That isn't layout feedback, so the view re-runs as for a state change.
    pub(super) fn selection_read_moved(&self) -> bool {
        let seen = &self.layout_seen.rects;
        seen.contains_key(SELECTION) && boxes_moved(&seen.selection, &self.selection_boxes())
    }
}
