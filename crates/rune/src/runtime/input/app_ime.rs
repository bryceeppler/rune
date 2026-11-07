//! Input method events: the focused text field hears them, else `App::on_ime` (an editor the
//! app draws itself, whose `El::ime_cursor` caret has the input method).
//!
//! A composition ends where it began, as a browser's does on blur. Before focus moves off its
//! editor, or as the window stops taking keys (after any input that arrived first), Rune
//! commits the text the editor shows there; a field hears `on_input_change` while it still has
//! focus. The shell then resets the platform's input method, and everything the platform sends
//! for the session that reset ended is dropped: until the shell reports that session over, or
//! the platform's own answer commits. None of it reaches the new focus.
use super::*;
use std::sync::Weak;

/// The editor a composition is open in: the app's own, showing `shown`, or the field at `path`
/// that edits `model` (a `TextEdit`'s identity, which its edits and undo history keep).
enum Owner {
    App { shown: String },
    Field { path: String, model: Weak<()> },
}

/// An open composition. `since` numbers the input method event that started it, in arrival
/// order, which places it among the platform's sessions.
struct Composition {
    since: u64,
    owner: Owner,
}

/// What the shell reports of the platform's input method, in order with its events.
pub(crate) enum ImeStep {
    /// The window stopped taking keys: the composition finishes after the input before it.
    Finish,
    /// The platform ended every session open when the shell's last reset was sent, which
    /// covered the events numbered up to this.
    SessionOver(u64),
}

/// `App::on_ime`, and the composition open in any editor.
pub(crate) struct AppIme<S> {
    pub(crate) hook: Option<fn(&mut S, &Ime, &mut Tx) -> bool>,
    open: Option<Composition>,
    /// The `since` of a composition Rune finished: what the platform sends for its session is
    /// dropped.
    ended: Option<u64>,
    /// Input method events that arrived so far.
    arrived: u64,
    /// `arrived` when the shell last reset the platform's input method.
    reset_at: u64,
    /// Models of removed fields whose finished composition commits once the view shows them
    /// again: only that view can reach them.
    unplaced: Vec<Weak<()>>,
}

impl<S> Default for AppIme<S> {
    fn default() -> Self { Self { hook: None, open: None, ended: None, arrived: 0, reset_at: 0, unplaced: Vec::new() } }
}

impl<S> AppIme<S> {
    /// The app's editor has a composition open: Return and Escape belong to the input method.
    pub(super) fn composing(&self) -> bool {
        self.hook.is_some() && matches!(self.open, Some(Composition { owner: Owner::App { .. }, .. }))
    }

    /// Numbers an input method event as it arrives.
    pub(super) fn arrive(&mut self) -> u64 {
        self.arrived += 1;
        self.arrived
    }

    /// `edit` is a removed field's finished composition, still to commit.
    pub(super) fn awaits(&self, edit: &crate::edit::TextEdit) -> bool {
        self.unplaced.iter().any(|model| edit.is_model(model))
    }
}

/// An `El::ime_cursor` element's drawn rect, device pixels, as the window's IME area in points.
pub(super) fn area(r: [f32; 4], s: f32) -> [f32; 4] {
    [r[0] / s, r[1] / s, (r[2] - r[0]) / s, (r[3] - r[1]) / s]
}

fn commit_preedit(edit: &mut crate::edit::TextEdit, model: &Weak<()>) -> EditOutcome {
    if !edit.is_model(model) { return EditOutcome::default(); }
    match edit.preedit.take() {
        Some((text, _)) => edit.ime(&Ime::Commit(text)),
        None => EditOutcome::default(),
    }
}

impl<S: 'static> Runtime<S> {
    /// An input method event goes to the focused text field, else to `App::on_ime`; turning
    /// on or off is only the app's to hear. `seq` numbers it in arrival order.
    pub(super) fn run_ime(&mut self, ev: Ime, seq: u64, one: &mut Tx) -> bool {
        if self.app_ime.ended.is_some() && matches!(ev, Ime::Preedit(..) | Ime::Commit(_)) {
            // The ended session's answer; a composition ends with one commit at most.
            if matches!(ev, Ime::Commit(_)) { self.app_ime.ended = None; }
            return false;
        }
        let app = matches!(ev, Ime::Enabled | Ime::Disabled) || self.focused_input().is_none();
        let active = self.composing() || matches!(&ev, Ime::Preedit(s, _) | Ime::Commit(s) if !s.is_empty());
        let mut model = None;
        let ran = if app { self.app_ime_event(&ev, one) } else {
            self.with_focused(|e| { let out = e.ime(&ev); model = Some(e.identity()); out }, one)
        };
        match &ev {
            Ime::Preedit(s, _) if !s.is_empty() => {
                let since = self.app_ime.open.as_ref().map_or(seq, |c| c.since);
                let owner = match (model, self.focused.clone()) {
                    (Some(model), Some(path)) => Owner::Field { path, model },
                    _ => Owner::App { shown: s.clone() },
                };
                self.app_ime.open = Some(Composition { since, owner });
            }
            Ime::Preedit(..) | Ime::Commit(_) => self.app_ime.open = None,
            Ime::Enabled | Ime::Disabled => {}
        }
        // The key that confirmed or cancelled a composition comes back up unheard.
        if ran && active { self.ime_eat_key_up = true; }
        ran
    }

    pub(super) fn run_ime_step(&mut self, step: ImeStep) -> bool {
        match step {
            ImeStep::Finish => self.finish_composition(),
            ImeStep::SessionOver(at) => {
                if self.app_ime.ended.is_some_and(|since| since <= at) { self.app_ime.ended = None; }
                false
            }
        }
    }

    /// Ends the composition as the window stops taking keys (it deactivates or hides): its text
    /// commits in the editor it began in, which keeps focus, once the input that arrived before
    /// has run. The platform's input method resets.
    pub fn cancel_composition(&mut self) {
        let composing = self.app_ime.open.is_some() || self.queue.iter().any(|a| matches!(a, Action::Ime(..)));
        if self.queue.is_empty() || !composing {
            self.finish_composition();
        } else if !self.queue.iter().any(|a| matches!(a, Action::ImeStep(ImeStep::Finish))) {
            self.queue.push(Action::ImeStep(ImeStep::Finish));
        }
        self.ime_reset = true;
    }

    /// The shell reset the platform's input method: the sessions open now end, and their events
    /// are what a later [`Self::ime_session_over`] vouches for.
    pub(crate) fn ime_reset_sent(&mut self) {
        self.app_ime.reset_at = self.app_ime.arrived;
    }

    /// The platform ended every session open at the last reset the shell sent: what it sends
    /// from here on belongs to a new one. Whether that's news to the runtime.
    pub(crate) fn ime_session_over(&mut self) -> bool {
        let at = self.app_ime.reset_at;
        let finishing = self.queue.iter().any(|a| matches!(a, Action::ImeStep(ImeStep::Finish)));
        let told = self.queue.iter().any(|a| matches!(a, Action::ImeStep(ImeStep::SessionOver(n)) if *n == at));
        let news = (self.app_ime.ended.is_some_and(|since| since <= at) || finishing) && !told;
        if news { self.queue.push(Action::ImeStep(ImeStep::SessionOver(at))); }
        news
    }

    /// Focus is about to move to `path`: a composition whose editor loses the input method by it
    /// (or already lost it, a new model now under focus) finishes there first. Whether that
    /// changed state.
    pub(super) fn finish_composition_off(&mut self, path: Option<&str>) -> bool {
        let stays = match &self.app_ime.open {
            Some(Composition { owner: Owner::App { .. }, .. }) => self.focused.as_deref() == path && self.focused_input().is_none(),
            Some(Composition { owner: Owner::Field { model, .. }, .. }) => self.focused.as_deref() == path
                && self.focused_input().is_some_and(|i| i.edit.is_model(model)),
            None => true,
        };
        !stays && self.finish_composition()
    }

    /// A removed field's finished composition commits once the view shows its model again.
    pub(super) fn place_unplaced(&mut self) {
        for model in std::mem::take(&mut self.app_ime.unplaced) {
            if !self.commit_in(None, &model) && model.strong_count() > 0 { self.app_ime.unplaced.push(model); }
        }
    }

    /// Commits the open composition's text in the editor it began in; the platform's events
    /// for its session are dropped from now on.
    fn finish_composition(&mut self) -> bool {
        let Some(Composition { since, owner }) = self.app_ime.open.take() else { return false };
        self.app_ime.ended = Some(since);
        self.ime_reset = true;
        match owner {
            Owner::App { shown } => {
                let mut one = self.transaction();
                let ran = self.app_ime_event(&Ime::Commit(shown), &mut one);
                self.outside_frame(one);
                ran
            }
            Owner::Field { path, model } => self.commit_in(Some(&path), &model) || { self.app_ime.unplaced.push(model); false },
        }
    }

    /// Commits the preedit of the field that edits `model`, if the view shows it (at `path`,
    /// else wherever it moved), as that field's own edit: it hears `on_input_change`.
    fn commit_in(&mut self, path: Option<&str>, model: &Weak<()>) -> bool {
        let shows = |i: usize, n: &Node| n.input.as_ref().is_some_and(|f| f.edit.is_model(model))
            && !self.subtree_exiting(i) && !self.subtree_unmounted(i);
        let mounted = self.nodes.iter().enumerate().find(|&(i, n)| path == Some(n.path.as_str()) && shows(i, n))
            .or_else(|| self.nodes.iter().enumerate().find(|&(i, n)| shows(i, n)))
            .map(|(_, n)| n.path.clone())
            .or_else(|| self.virtual_inputs.iter().find(|(_, f)| f.edit.is_model(model)).map(|(p, _)| p.clone()));
        let Some(path) = mounted else { return false };
        let mut one = self.transaction();
        self.with_input_state(path, |state, access, _| commit_preedit(access(state), model), &mut one);
        self.outside_frame(one);
        true
    }

    /// What a handler run outside an action asked of `one` happens as an `update`'s does.
    fn outside_frame(&mut self, mut one: Tx) {
        self.latch_requests(&mut one);
        merge(&mut self.pending, one);
        self.dirty = true;
    }

    fn app_ime_event(&mut self, ev: &Ime, one: &mut Tx) -> bool {
        let Some(f) = self.app_ime.hook else { return false };
        let state = match &mut self.playground {
            Some(playground) if !playground.stage_hears(&self.state) => None,
            Some(playground) => playground.key_state(&mut self.state),
            None => Some(&mut self.state),
        };
        state.is_some_and(|state| Tx::took(app_call!(f, state, ev, one), one))
    }
}
