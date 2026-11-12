//! Keys, IME and paste on a [`TextEdit`].


use super::*;
use crate::text::TextLayout;

impl TextEdit {
    fn insert_newline(&mut self, block_for_new_line: Block, soft: bool) {
        if self.secure || self.is_single_line() {
            return;
        }
        self.sync_invariants();
        self.break_history_group();
        let before = self.snapshot();
        let (a, b) = self.range();
        let inserted_marks = if let Some((armed, p)) = self.pending.take()
            && armed == self.ends()
        {
            p
        } else {
            self.inherited_marks_at(a)
        };
        self.apply_text_and_attr_replace((a, b), "\n", &inserted_marks, Some(block_for_new_line), soft);
        self.record_change(before, false);
    }

    /// Puts the focus at `to`, and the anchor with it unless `extend`.
    pub(super) fn set(&mut self, to: Caret, extend: bool) {
        // A caret moving into a pill steps over it, the way it was going.
        let to = match self.out_of_pill(to.byte, to.byte > self.selection.focus.byte) {
            byte if byte == to.byte => to,
            byte => Caret::at(byte),
        };
        self.selection.focus = to;
        if !extend {
            self.selection.anchor = to;
        }
        self.snap_to_pills();
        self.pending = None;
    }

    /// `layout` when it draws the text itself: no composition in it and no mask over it.
    fn drawn<'l>(&self, layout: Option<&'l TextLayout>) -> Option<&'l TextLayout> {
        layout.filter(|_| self.preedit.is_none() && (!self.secure || self.revealed))
    }

    /// Where Left (`left`) or Right moves the caret, by a character or a `word`. With a layout it
    /// moves as the text draws: Left always toward the screen's left, through either direction
    /// of text.
    fn step_h(&self, layout: Option<&TextLayout>, left: bool, word: bool) -> Caret {
        let focus = self.selection.focus.byte;
        let Some(l) = self.drawn(layout) else {
            return Caret::at(match (left, word) {
                (true, true) => self.prev_word(focus),
                (true, false) => prev_char(&self.text, focus),
                (false, true) => self.next_word(focus),
                (false, false) => next_char(&self.text, focus),
            });
        };
        let blank = |side: Option<Range<usize>>| side.is_none_or(|r| self.text[r].starts_with(|c: char| c.is_ascii_whitespace()));
        let mut at = self.selection.focus;
        while let Some((byte, affinity)) = l.step(at.byte, at.affinity(), left) {
            at = Caret::of(byte, affinity);
            let [l_side, r_side] = l.sides(at.byte, at.affinity());
            let (behind, ahead) = if left { (r_side, l_side) } else { (l_side, r_side) };
            if !word || (!blank(behind) && blank(ahead)) {
                break;
            }
        }
        at
    }

    /// The end of the selection a plain Left (`left`) or Right collapses it to: the end drawn
    /// furthest that way when both are on one line, else its start or end as its paragraph reads.
    fn collapse_end(&self, layout: Option<&TextLayout>, left: bool) -> Caret {
        let Selection { anchor, focus } = self.selection;
        let (a, b) = self.range();
        let in_order = |start: bool| {
            let byte = if start { a } else { b };
            [anchor, focus].into_iter().find(|c| c.byte == byte).unwrap_or(Caret::at(byte))
        };
        let Some(l) = self.drawn(layout) else {
            return in_order(left);
        };
        let [p, q] = [anchor, focus].map(|c| l.caret_box(c.byte, c.affinity(), 0.0));
        if p.y0 == q.y0 {
            let anchor_further = if left { p.x0 < q.x0 } else { p.x0 > q.x0 };
            return if anchor_further { anchor } else { focus };
        }
        in_order(left != l.is_rtl(focus.byte))
    }

    /// `to`, or the anchor where `to` draws in the same place, so a selection stepped back to where
    /// it began selects nothing.
    fn onto_anchor(&self, layout: Option<&TextLayout>, to: Caret) -> Caret {
        let anchor = self.selection.anchor;
        let Some(l) = self.drawn(layout) else { return to };
        let [p, q] = [to, anchor].map(|c| l.caret_box(c.byte, c.affinity(), 0.0));
        if (p.x0, p.y0) == (q.x0, q.y0) { anchor } else { to }
    }

    /// The left (`left`) or right end of the caret's line as it draws, or its start or end in
    /// the text where the layout doesn't draw the text itself.
    fn line_side(&self, layout: Option<&TextLayout>, left: bool) -> Caret {
        match self.drawn(layout) {
            Some(l) => {
                let (byte, affinity) = l.line_side(self.selection.focus.byte, self.selection.focus.affinity(), left);
                Caret::of(byte, affinity)
            }
            None => Caret::at(self.line_edge(layout, !left)),
        }
    }

    /// Applies one key. `layout` is the field's layout for the current text (device px).
    pub fn key(&mut self, ev: &KeyEvent, layout: Option<&TextLayout>) -> EditOutcome {
        let mut out = EditOutcome::default();
        if matches!(ev.key, Key::Modifier { .. }) { return out; }
        self.sync_invariants();
        let m = ev.mods;
        self.normalize_hidden_selection();
        if self.is_hidden_multiline() && matches!(ev.key, Key::Left | Key::Right | Key::Up | Key::Down | Key::Home | Key::End) {
            let end = Caret::at(self.text.len());
            self.selection = Selection { anchor: if m.shift { Caret::at(0) } else { end }, focus: end };
            return out;
        }
        if let Some(c) = ev.control_binding() {
            let (key, line) = match c {
                // Ctrl-A, B, E and F move through the text in its order, as `moveBackward:` does.
                'a' | 'b' | 'e' | 'f' if !self.is_hidden_multiline() => {
                    let (a, b) = self.range();
                    let collapse = a != b && !m.shift;
                    let i = match c {
                        'a' => self.line_edge(layout, false),
                        'e' => self.line_edge(layout, true),
                        'b' if collapse => a,
                        'f' if collapse => b,
                        'b' => prev_char(&self.text, self.selection.focus.byte),
                        _ => next_char(&self.text, self.selection.focus.byte),
                    };
                    self.set(Caret::at(i), m.shift);
                    return out;
                }
                'a' => (Key::Left, true), 'e' => (Key::Right, true), 'b' => (Key::Left, false),
                'f' => (Key::Right, false), 'p' => (Key::Up, false), 'n' => (Key::Down, false),
                'd' => (Key::Delete, false), 'h' => (Key::Backspace, false),
                _ => {
                    // Ctrl-K deletes to the end of the line, or the line break at its end.
                    self.break_history_group();
                    if self.is_hidden_multiline() && self.is_pristine() {
                        self.insert("");
                        return out;
                    }
                    let (a, b) = self.range();
                    let end = if a == b { self.line_edge(layout, true) } else { b };
                    let to = if end > a { end } else if a < self.text.len() { next_char(&self.text, a) } else { a };
                    if to > a { self.replace((a, to), "", false); }
                    return out;
                }
            };
            return self.key(&KeyEvent { key, mods: Mods { shift: m.shift, cmd: line, ..Mods::default() } }, layout);
        }
        if ev.is_command_chord() { return out; }
        if !matches!(&ev.key, Key::Text(s) if !s.contains(['\r', '\n'])) { self.break_history_group(); }
        let mac = cfg!(target_os = "macos");
        let word = m.alt || (m.ctrl && !mac);
        match &ev.key {
            Key::Text(s) => {
                if s.chars().any(|c| !c.is_control() || (!self.is_single_line() && matches!(c, '\n' | '\r' | '\t'))) {
                    let s = self.sanitize(s);
                    if !s.is_empty() { self.replace(self.range(), &s, !s.contains(['\r', '\n'])); }
                }
            }
            // A composition's Return and Escape are the input method's.
            Key::Enter | Key::Escape if self.preedit.is_some() => {}
            Key::Enter => {
                if self.is_single_line() {
                    out.submit = true;
                } else if self.secure {
                    if m.cmd || m.ctrl {
                        out.submit = true;
                    } else {
                        self.replace(self.range(), "\n", false);
                    }
                } else {
                    let (a, b) = self.range();
                    let line_idx = self.line_of_byte(a);
                    let blk = self.blocks.get(line_idx).copied().unwrap_or(Block::Paragraph);
                    let line_range = self.line_range(line_idx);
                    if m.shift && self.rich {
                        // Shift+Return: soft line break keeping the current paragraph's block.
                        self.insert_newline(blk, true);
                    } else if m.shift || m.alt {
                        self.insert_newline(blk, false);
                    } else {
                        match blk {
                            Block::Code => {
                                // Return on a blank last line leaves the block, as on an empty
                                // list item; anywhere else it inserts a newline.
                                if a == b && line_range.is_empty() && self.blocks.get(line_idx + 1) != Some(&Block::Code) {
                                    self.set_block(Block::Paragraph);
                                } else {
                                    self.insert_newline(Block::Code, false);
                                }
                            }
                            Block::Bullet(_) | Block::Numbered(_) => {
                                // Return in an empty list item ends the list; otherwise continues it.
                                if a == b && line_range.is_empty() {
                                    self.set_block(Block::Paragraph);
                                } else {
                                    self.insert_newline(blk, false);
                                }
                            }
                            Block::Quote => {
                                if a == b && line_range.is_empty() {
                                    self.set_block(Block::Paragraph);
                                } else {
                                    self.insert_newline(Block::Quote, false);
                                }
                            }
                            Block::Paragraph | Block::Hanging { .. } => {
                                out.submit = true;
                            }
                        }
                    }
                }
            }
            Key::Tab => {
                if !m.cmd && !m.alt && !m.ctrl && !self.secure && !self.is_single_line() {
                    if self.active_block() == Block::Code && !m.shift {
                        self.replace(self.range(), "\t", true);
                    } else if m.shift {
                        self.outdent_list();
                    } else {
                        self.indent_list();
                    }
                }
            }
            Key::PageUp | Key::PageDown => {}
            Key::Escape => out.blur = true,
            Key::Backspace => {
                let (a, b) = self.range();
                if a != b || (self.is_hidden_multiline() && self.is_pristine()) {
                    self.insert("");
                } else {
                    let line_idx = self.line_of_byte(a);
                    let line_range = self.line_range(line_idx);
                    let blk = self.blocks.get(line_idx).copied().unwrap_or(Block::Paragraph);
                    let is_soft = self.soft_breaks.binary_search(&(line_idx as u32)).is_ok();
                    if !m.cmd && !word && a == line_range.start && !is_soft && !matches!(blk, Block::Paragraph | Block::Hanging { .. }) {
                        match blk {
                            Block::Bullet(lvl) | Block::Numbered(lvl) => {
                                if lvl > 0 {
                                    self.set_block(blk.with_level(lvl - 1));
                                } else {
                                    self.set_block(Block::Paragraph);
                                }
                            }
                            Block::Quote | Block::Code => {
                                if a == 0 || line_range.is_empty() {
                                    self.set_block(Block::Paragraph);
                                } else {
                                    let from = prev_char(&self.text, a);
                                    self.replace((from, a), "", false);
                                }
                            }
                            Block::Paragraph | Block::Hanging { .. } => unreachable!(),
                        }
                    } else if a > 0 {
                        let from = if m.cmd {
                            if self.secure && !self.revealed { 0 } else { self.text[..a].rfind('\n').map_or(0, |i| i + 1) }
                        } else if word {
                            self.prev_word(a)
                        } else {
                            prev_char(&self.text, a)
                        };
                        self.replace((from, a), "", false);
                    }
                }
            }
            Key::Delete => {
                let (a, b) = self.range();
                if a != b || (self.is_hidden_multiline() && self.is_pristine()) {
                    self.insert("");
                } else if a < self.text.len() {
                    let to = if word { self.next_word(a) } else { next_char(&self.text, a) };
                    self.replace((a, to), "", false);
                }
            }
            Key::Right if !m.shift && !m.cmd && !m.alt && !m.ctrl && self.at_end_of_trailing_block() => self.leave_trailing_block(),
            Key::Down if !m.shift && !m.cmd && !m.alt && !m.ctrl && self.at_end_of_trailing_block() => self.leave_trailing_block(),
            Key::Left | Key::Right => {
                let left = ev.key == Key::Left;
                let to = if m.cmd && mac {
                    self.line_side(layout, left)
                } else if !m.shift && !self.selection.is_collapsed() {
                    self.collapse_end(layout, left)
                } else {
                    self.step_h(layout, left, word)
                };
                let to = if m.shift { self.onto_anchor(layout, to) } else { to };
                self.set(to, m.shift);
            }
            // Windows and Linux move Home and End along the line, and to the text's ends with Ctrl.
            Key::Home => {
                let i = if mac || m.cmd { 0 } else { self.line_edge(layout, false) };
                self.set(Caret::at(i), m.shift)
            }
            Key::End => {
                let i = if mac || m.cmd { self.text.len() } else { self.line_edge(layout, true) };
                self.set(Caret::at(i), m.shift)
            }
            Key::Up | Key::Down => {
                let down = ev.key == Key::Down;
                let n = self.text.len();
                let end = Caret::at(if down { n } else { 0 });
                let focus = self.selection.focus;
                let to = if m.cmd || (self.secure && !self.revealed) {
                    end
                } else if let Some(l) = layout {
                    match l.move_line(self.display_index(focus.byte), self.caret_affinity(), down) {
                        Some((j, affinity)) => match self.model_index(j) {
                            j if j == focus.byte => end,
                            j => Caret::of(j.min(n), affinity),
                        },
                        None => Caret::at(n),
                    }
                } else {
                    end
                };
                if m.shift {
                    self.selection.focus = to;
                    self.snap_to_pills();
                    self.pending = None;
                } else {
                    self.set_caret(to);
                }
            }
            Key::Command(c) => match c {
                'z' if m.shift => self.redo(),
                'z' => self.undo(),
                'a' => {
                    self.selection = Selection { anchor: Caret::at(0), focus: Caret::at(self.text.len()) };
                    self.pending = None;
                }
                'b' | 'i' | 'u' => {
                    if let Some(mark) = self.format_chord(ev) {
                        self.toggle_mark(mark);
                    }
                }
                'c' if !self.secure => out.copy = Some(self.selected().to_owned()),
                'x' if !self.secure => {
                    out.copy = Some(self.selected().to_owned());
                    self.insert("");
                }
                _ => {}
            },
            Key::Function(_) | Key::ContextMenu | Key::Insert | Key::PrintScreen | Key::ScrollLock | Key::Pause | Key::Modifier { .. } => {}
        }
        out
    }

    /// The caret is at the end of the text, in a code block or quote there that it can't move
    /// out of by itself.
    fn at_end_of_trailing_block(&self) -> bool {
        let n = self.text.len();
        !self.secure
            && !self.is_single_line()
            && self.preedit.is_none()
            && self.ends() == (n, n)
            && matches!(self.blocks.last(), Some(Block::Code | Block::Quote))
    }

    /// Adds an empty paragraph after a trailing code block or quote and puts the caret in it:
    /// one undo step.
    fn leave_trailing_block(&mut self) {
        self.sync_invariants();
        self.break_history_group();
        let before = self.snapshot();
        let n = self.text.len();
        self.pending = None;
        self.apply_text_and_attr_replace((n, n), "\n", &[], Some(Block::Paragraph), false);
        self.record_change(before, false);
    }

    fn line_edge(&self, layout: Option<&TextLayout>, end: bool) -> usize {
        if self.secure && !self.revealed {
            return if end { self.text.len() } else { 0 };
        }
        if let Some(l) = layout {
            return self.model_index(l.line_edge(self.display_index(self.selection.focus.byte), end));
        }
        let focus = self.selection.focus.byte;
        if end {
            self.text[focus..].find('\n').map_or(self.text.len(), |i| focus + i)
        } else {
            self.text[..focus].rfind('\n').map_or(0, |i| i + 1)
        }
    }

    pub fn ime(&mut self, ev: &Ime) -> EditOutcome {
        if self.secure { self.clear_preedit(); return EditOutcome::default(); }
        self.sync_invariants();
        self.break_history_group();
        match ev {
            Ime::Preedit(s, cur) => {
                self.preedit = if s.is_empty() { None } else { Some((s.clone(), *cur)) };
            }
            Ime::Commit(s) => {
                self.preedit = None;
                let s = self.sanitize(s);
                self.insert(&s);
            }
            Ime::Enabled | Ime::Disabled => {}
        }
        EditOutcome::default()
    }

    pub fn paste(&mut self, s: &str) -> EditOutcome {
        self.normalize_hidden_selection();
        let s = self.sanitize(s);
        self.insert(&s);
        EditOutcome::default()
    }

    /// Paste rich HTML (`html`) with plain-text fallback (`plain`) into `self` at the current
    /// selection, recorded as a single undo step. Does not touch the OS clipboard. A plain field,
    /// or HTML with no marks or blocks this model keeps, pastes `plain` instead. Pills
    /// (`<span data-pill="id">`) paste as their plain labels: a field restores them only from
    /// its app's own copies, and an app's paste hook can with
    /// [`paste_html_with_pills`](Self::paste_html_with_pills).
    pub fn paste_html(&mut self, html_str: &str, plain: &str) -> EditOutcome {
        self.paste_rich(html_str, plain, false)
    }

    /// [`paste_html`](Self::paste_html), keeping the HTML's [`Mark::Pill`]s.
    pub fn paste_html_with_pills(&mut self, html_str: &str, plain: &str) -> EditOutcome {
        self.paste_rich(html_str, plain, true)
    }

    fn paste_rich(&mut self, html_str: &str, plain: &str, pills: bool) -> EditOutcome {
        if self.secure || html_str.trim().is_empty() {
            return self.paste(plain);
        }
        let mut frag = Self::from_html(html_str);
        if !pills {
            frag.marks.retain(|m| !m.mark.is_pill());
        }
        // HTML that only colours or lays out text (a code editor's copy) would collapse the
        // plain text's whitespace for nothing.
        let frag = self.keep_allowed(frag);
        let unformatted = frag.marks.is_empty() && frag.blocks.iter().all(|&b| b == Block::Paragraph);
        if !plain.is_empty() && (frag.text.is_empty() || unformatted) {
            return self.paste(plain);
        }
        if !self.rich {
            return self.paste(if plain.is_empty() { frag.text.as_str() } else { plain });
        }
        self.sync_invariants();
        // A code block takes the plain text, whose whitespace HTML would have collapsed.
        if !self.is_single_line() {
            let (a, _) = self.range();
            if self.blocks.get(self.line_of_byte(a)) == Some(&Block::Code) {
                return self.paste(if plain.is_empty() { frag.text.as_str() } else { plain });
            }
        }
        self.break_history_group();
        let before = self.snapshot();
        let (a, b) = self.range();

        if self.is_single_line() {
            // Single-line field: strip line breaks from `frag.text` while keeping inline marks.
            let mut clean = String::with_capacity(frag.text.len());
            let mut byte_map = vec![0usize; frag.text.len() + 1];
            for (i, ch) in frag.text.char_indices() {
                byte_map[i] = clean.len();
                if !ch.is_control() && !matches!(ch, '\u{2028}' | '\u{2029}') {
                    clean.push(ch);
                } else if ch == '\n' && !clean.ends_with(' ') && !clean.is_empty() {
                    clean.push(' ');
                }
            }
            byte_map[frag.text.len()] = clean.len();
            self.apply_text_and_attr_replace((a, b), &clean, &[], None, false);
            for m in &frag.marks {
                let s_idx = a + byte_map[m.start.min(frag.text.len())];
                let e_idx = a + byte_map[m.end.min(frag.text.len())];
                if s_idx < e_idx {
                    self.marks.push(MarkRange { start: s_idx, end: e_idx, mark: m.mark.clone() });
                }
            }
            self.pending = None;
            self.normalize_marks();
            self.record_change(before, false);
            return EditOutcome::default();
        }

        // First delete `a..b` if non-empty.
        if a < b {
            self.apply_text_and_attr_replace((a, b), "", &[], None, false);
        }
        let ins_at = a;
        let line_idx = self.line_of_byte(ins_at);
        let line_range = self.line_range(line_idx);
        let line_was_empty = line_range.is_empty();
        let orig_block = self.blocks.get(line_idx).copied().unwrap_or(Block::Paragraph);

        self.apply_text_and_attr_replace((ins_at, ins_at), &frag.text, &[], None, false);

        // Apply `frag.blocks` and `frag.soft_breaks`.
        let frag_lines = frag.blocks.len();
        if frag_lines == 1 {
            if line_was_empty && orig_block == Block::Paragraph && frag.blocks[0] != Block::Paragraph {
                if let Some(slot) = self.blocks.get_mut(line_idx) {
                    *slot = frag.blocks[0];
                }
            }
        } else if frag_lines > 1 {
            if (line_was_empty || ins_at == line_range.start) && orig_block == Block::Paragraph {
                if let Some(slot) = self.blocks.get_mut(line_idx) {
                    *slot = frag.blocks[0];
                }
            }
            for k in 1..frag_lines {
                if let Some(slot) = self.blocks.get_mut(line_idx + k) {
                    *slot = frag.blocks[k];
                }
            }
            for &sb in &frag.soft_breaks {
                let target = line_idx + sb as usize;
                if target > 0 {
                    self.soft_breaks.push(target as u32);
                }
            }
        }

        // Add `frag.marks` shifted by `ins_at`.
        for m in &frag.marks {
            self.marks.push(MarkRange {
                start: ins_at + m.start,
                end: ins_at + m.end,
                mark: m.mark.clone(),
            });
        }
        self.pending = None;
        self.sync_invariants();
        self.record_change(before, false);
        EditOutcome::default()
    }
}
