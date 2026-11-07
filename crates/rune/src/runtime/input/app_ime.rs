//! Input method events: the focused text field hears them, else `App::on_ime` (an editor the
//! app draws itself, whose `El::ime_cursor` caret has the input method).
//!
//! A composition ends where it began, as a browser's does on blur. Before focus moves off its
//! editor, or as the window stops taking keys (after any input that arrived first), Rune
//! commits the text the editor shows there; a field hears `on_input_change` while it still has
//! focus. That ends the input method session the composition was open in, and the shell
//! resets the platform's input method.
//!
//! Every event carries the session it belongs to, stamped by whoever delivers it: the shell
//! knows which of the platform's events answer the reset, and anything else is the current
//! session's. An event from a session Rune ended is dropped; one from the current session
//! always arrives.
use super::*;
use std::sync::Weak;

/// The editor a composition is open in: the app's own, showing `shown`, or the field at `path`
/// that edits `model` (a `TextEdit`'s identity, which its edits and undo history keep).
enum Owner {
    App { shown: String },
    Field { path: String, model: Weak<()> },
}

/// An open composition, in the session its events came from.
struct Composition {
    session: u64,
    owner: Owner,
}

/// `App::on_ime`, the composition open in any editor, and the sessions its events come from.
pub(crate) struct AppIme<S> {
    pub(crate) hook: Option<fn(&mut S, &Ime, &mut Tx) -> bool>,
    open: Option<Composition>,
    /// The current session. Every one before it was ended by finishing a composition.
    session: u64,
    /// The session the last finish ended, until the shell's next reset takes it.
    ending: Option<u64>,
    /// The session the shell's last reset ended, which a late event from the platform is from.
    reset_ended: Option<u64>,
    /// The platform is still answering the shell's last reset (macOS, Windows, X11): its events
    /// are late until its next `Enabled` or `Disabled`, or its second idle wait (the number seen).
    answering: Option<u8>,
    /// Models of removed fields whose finished composition commits once the view shows them
    /// again: only that view can reach them.
    unplaced: Vec<Weak<()>>,
}

impl<S> Default for AppIme<S> {
    fn default() -> Self {
        Self { hook: None, open: None, session: 0, ending: None, reset_ended: None, answering: None, unplaced: Vec::new() }
    }
}

impl<S> AppIme<S> {
    /// The app's editor has a composition open: Return and Escape belong to the input method.
    pub(super) fn composing(&self) -> bool {
        self.hook.is_some() && matches!(self.open, Some(Composition { owner: Owner::App { .. }, .. }))
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
    /// The input method session events belong to now. Finishing a composition (focus left its
    /// editor, or the window stopped taking keys) ends its session and starts the next.
    pub fn ime_session(&self) -> u64 {
        self.app_ime.session
    }

    /// An input method event from `session`, as whoever delivers it knows: the shell, or a test
    /// or embedder replaying one. An event from a session Rune ended is dropped; one from the
    /// current session ([`Self::ime_session`]) always arrives. [`Self::ime`] sends one from the
    /// current session.
    pub fn ime_in(&mut self, session: u64, ev: Ime) {
        self.queue.push(Action::Ime(ev, session));
    }

    /// An input method event goes to the focused text field, else to `App::on_ime`; turning
    /// on or off is only the app's to hear.
    pub(super) fn run_ime(&mut self, ev: Ime, session: u64, one: &mut Tx) -> bool {
        if session < self.app_ime.session && matches!(ev, Ime::Preedit(..) | Ime::Commit(_)) { return false; }
        let app = matches!(ev, Ime::Enabled | Ime::Disabled) || self.focused_input().is_none();
        let active = self.composing() || matches!(&ev, Ime::Preedit(s, _) | Ime::Commit(s) if !s.is_empty());
        let mut model = None;
        let ran = if app { self.app_ime_event(&ev, one) } else {
            self.with_focused(|e| { let out = e.ime(&ev); model = Some(e.identity()); out }, one)
        };
        match &ev {
            Ime::Preedit(s, _) if !s.is_empty() => {
                let owner = match (model, self.focused.clone()) {
                    (Some(model), Some(path)) => Owner::Field { path, model },
                    _ => Owner::App { shown: s.clone() },
                };
                self.app_ime.open = Some(Composition { session: self.app_ime.session, owner });
            }
            Ime::Preedit(..) | Ime::Commit(_) => self.app_ime.open = None,
            Ime::Enabled | Ime::Disabled => {}
        }
        // The key that confirmed or cancelled a composition comes back up unheard.
        if ran && active { self.ime_eat_key_up = true; }
        ran
    }

    /// The shell reset the platform's input method. With `answers`, the platform's answer is
    /// still to come and ends with its next `Enabled` or `Disabled`.
    pub(crate) fn ime_reset_sent(&mut self, answers: bool) {
        let ime = &mut self.app_ime;
        ime.reset_ended = ime.ending.take();
        ime.answering = answers.then_some(0);
    }

    /// An input method event from the platform. `late`: it answers a session the shell's last
    /// reset ended, as Wayland's serial tells; elsewhere so is anything before the end of the
    /// platform's answer to that reset.
    pub(crate) fn ime_from_platform(&mut self, ev: Ime, late: bool) {
        let ime = &mut self.app_ime;
        let late = late || ime.answering.is_some();
        if matches!(ev, Ime::Enabled | Ime::Disabled) { ime.answering = None; }
        let session = late.then_some(ime.reset_ended).flatten().unwrap_or(ime.session);
        self.ime_in(session, ev);
    }

    /// The event loop is about to wait. macOS and Windows answer a reset while it runs, so
    /// their answer has arrived by the second wait after it, `Disabled` or not.
    #[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
    pub(crate) fn ime_platform_idle(&mut self) {
        let ime = &mut self.app_ime;
        if let Some(waits) = &mut ime.answering {
            *waits += 1;
            if *waits >= 2 { ime.answering = None; }
        }
    }

    /// Ends the composition as the window stops taking keys (it deactivates or hides): its text
    /// commits in the editor it began in, which keeps focus, once the input that arrived before
    /// has run. The platform's input method resets.
    pub fn cancel_composition(&mut self) {
        let composing = self.app_ime.open.is_some() || self.queue.iter().any(|a| matches!(a, Action::Ime(..)));
        if self.queue.is_empty() || !composing {
            self.finish_composition();
        } else if !self.queue.iter().any(|a| matches!(a, Action::ImeFinish)) {
            self.queue.push(Action::ImeFinish);
        }
        self.ime_reset = true;
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

    /// Commits the open composition's text in the editor it began in, and ends its session.
    pub(super) fn finish_composition(&mut self) -> bool {
        let Some(Composition { session, owner }) = self.app_ime.open.take() else { return false };
        self.app_ime.ending = Some(session);
        self.app_ime.session = self.app_ime.session.max(session + 1);
        self.ime_reset = true;
        // What the person typed reaches the app: a save under way may have missed it.
        self.closing.heard_input();
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
