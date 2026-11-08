//! The secondary gesture (right-click, macOS control-click, the Menu key and Shift+F10):
//! which element's menu it opens.
use super::*;
use crate::el::OutsideRightClick;

/// Where a secondary gesture that dismissed a modal went down and, once it has, came up.
pub(crate) struct Through {
    down: (f32, f32),
    up: Option<(f32, f32)>,
}

impl<S: 'static> Runtime<S> {
    /// The open modal a press at (x, y) falls outside of, where nothing it holds is drawn, and
    /// what a right-click there does to it.
    fn modal_outside(&self, x: f32, y: f32) -> Option<(String, OutsideRightClick)> {
        let modal = self.active_modal()?;
        let rule = self.nodes[modal].handlers.modal.as_ref()?.outside_right_click();
        let inside = (0..self.nodes.len()).any(|i| self.modal_members[i] && !self.subtree_exiting(i) && self.node_contains(i, x, y));
        (!inside).then(|| (self.nodes[modal].path.clone(), rule))
    }

    /// Hands a gesture that dismissed a modal to what is under it, once the modal has gone. A
    /// modal still open then, one the dismiss kept or a new one, takes nothing.
    pub(crate) fn replay_secondary_through(&mut self) {
        if self.queue.iter().any(|a| matches!(a, Action::Dismiss(_))) { return; }
        let Some(through) = self.secondary_through.take() else { return };
        if self.active_modal().is_some() { return; }
        self.secondary_down(through.down.0, through.down.1);
        if let Some((x, y)) = through.up { self.secondary_up(x, y); }
    }

    /// The nearest element with `on_secondary_press` (or a link with `on_link_secondary`) at
    /// (x, y), else the selectable text there (its Copy menu): an app's context menu on a row
    /// wins over its text's.
    pub(super) fn secondary_target(&self, x: f32, y: f32) -> Option<&str> {
        let id = self.hits.iter().rev().find(|&&(id, r)| self.hit_contains(id, r, x, y))?.0;
        let mut index = *self.node_index.get(&id)?;
        let mut text = None;
        loop {
            let n = &self.nodes[index];
            if self.subtree_exiting(index) || !self.allowed(index) { break; }
            if n.handlers.on_secondary_press.is_some() || self.secondary_link_at(index, x, y).is_some() { return Some(&n.path); }
            if text.is_none() && self.is_selectable(index) { text = Some(n.path.as_str()); }
            let Some(p) = n.parent else { break };
            index = p;
        }
        text
    }

    /// Capture a secondary gesture without activating primary actions or text selection.
    /// Outside a modal, it does what the modal's [`ModalKind`](crate::ModalKind) says: outside a
    /// menu that outside presses dismiss, it dismisses the menu, and the frame after, which no
    /// longer has it, takes the gesture as if it had just begun there, so a second right-click
    /// opens its own menu; outside a dialog it dismisses it when an outside press does and
    /// reaches nothing behind it.
    pub fn secondary_down(&mut self, x: f32, y: f32) {
        self.pointer = Some((x, y));
        self.release_hover_hold();
        self.keyboard_focus = false;
        self.keyed_since_press = false;
        self.dismiss_help();
        self.secondary_capture = self.secondary_target(x, y).map(str::to_owned);
        if self.secondary_capture.is_none() && let Some((modal, rule)) = self.modal_outside(x, y) {
            if rule != OutsideRightClick::Nothing { self.queue.push(Action::Dismiss(modal)); }
            if rule == OutsideRightClick::DismissesAndPassesOn { self.secondary_through = Some(Through { down: (x, y), up: None }); }
            return;
        }
        let i = self.secondary_capture.as_ref().and_then(|p| self.nodes.iter().position(|n| &n.path == p));
        self.secondary_link_down(i, x, y);
    }

    /// The byte of input `ji`'s text a secondary gesture is on, once a composition open there
    /// has committed where it began, as a primary press commits it first, so the menu acts on
    /// the text the person sees and nothing later from that composition's session lands.
    fn secondary_text(&mut self, ji: usize, point: Option<(f32, f32)>) -> Option<usize> {
        let path = self.nodes[ji].path.clone();
        self.finish_composition_in(&path);
        self.input_text_at(ji, point)
    }

    pub fn secondary_up(&mut self, x: f32, y: f32) {
        self.pointer = Some((x, y));
        if let Some(through) = &mut self.secondary_through {
            through.up = Some((x, y));
            self.leave_after_release();
            return;
        }
        let down_link = self.take_secondary_link();
        if let Some(path) = self.secondary_capture.take()
            && self.secondary_target(x, y) == Some(path.as_str())
        {
            let i = self.nodes.iter().position(|n| n.path == path);
            let mut point = i.map_or(PointerEvent { x, y, window_x: x, window_y: y, text: None }, |i| self.pointer_event(i, x, y));
            point.text = i.and_then(|i| self.textbox_input(i)).and_then(|ji| self.secondary_text(ji, Some((x, y))));
            let link = i.and_then(|i| self.secondary_link_at(i, x, y));
            // Like a primary link press, it must end on the link it began on; one that moved
            // onto, off or between links opens nothing.
            match (link, down_link) {
                (Some(k), Some(d)) if k == d => self.queue.push(Action::LinkSecondary(path, k, point)),
                (None, None) if !self.selection_menu(&path, x, y) => self.queue.push(Action::SecondaryPress(path, point)),
                _ => {}
            }
        }
        self.leave_after_release();
    }

    /// The Menu key, or Shift+F10, that nothing before it took: the focused element's context
    /// menu. That is its focused link's `on_link_secondary`, else its own or its nearest
    /// ancestor's `on_secondary_press`, at the bottom-left of the focused link or element, or of
    /// a focused text input's caret, where a menu opens below it as AppKit's and Windows'
    /// keyboard menus do.
    pub(super) fn context_menu_key(&mut self, ev: &KeyEvent, tx: &mut Tx) -> bool {
        let m = ev.mods;
        let plain = !m.cmd && !m.ctrl && !m.alt && !m.logo;
        let menu_key = match ev.key {
            crate::Key::ContextMenu => plain,
            crate::Key::Function(10) => plain && m.shift,
            _ => false,
        };
        let Some(focused) = self.focused_index().filter(|&i| menu_key && self.can_focus(i)) else { return false };
        if self.focused_link_menu(focused, tx) { return true; }
        let input = self.textbox_input(focused);
        let [x, y] = match input.and(self.ime_area) {
            Some([x, y, _, h]) => [x, y + h],
            None => { let [x, _, _, y] = self.drawn_box(focused); [x, y] }
        };
        let mut index = focused;
        loop {
            let n = &self.nodes[index];
            if !self.allowed(index) { return false; }
            if let Some(f) = n.handlers.on_secondary_press.clone() {
                let path = n.path.clone();
                let mut point = self.pointer_event(index, x, y);
                point.text = input.and_then(|ji| self.secondary_text(ji, None));
                log_event!(self, "secondary press {path}");
                f(states!(self), point, tx);
                return true;
            }
            let Some(p) = n.parent else { return false };
            index = p;
        }
    }
}
