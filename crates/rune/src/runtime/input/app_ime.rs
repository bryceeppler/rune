//! Input method events: the focused text field hears them, else `App::on_ime` (an editor the
//! app draws itself, whose `El::ime_cursor` caret has the input method).
//!
//! A composition ends where it began, as a browser's does on blur. Before focus moves off its
//! editor, or as the window stops taking keys, Rune commits the text the editor shows there
//! (a field hears `on_input_change` while it still has focus) and the shell discards the
//! platform's composition. Whatever the platform still sends for it is dropped, so none of it
//! reaches the new focus.
use super::*;
use std::sync::Weak;

type Model = Weak<crate::edit::History>;

/// The editor a composition is open in: the app's own, showing `shown`, or a text field's model.
enum Owner {
    App { shown: String },
    Field(Model),
}

enum Composition {
    Open(Owner),
    /// Rune committed it. The platform's answer (a clearing preedit, a commit of its own) is
    /// dropped until it commits, another composition begins or a key is pressed.
    Finished,
}

/// `App::on_ime`, and the composition open in any editor.
pub(crate) struct AppIme<S> {
    pub(crate) hook: Option<fn(&mut S, &Ime, &mut Tx) -> bool>,
    composition: Option<Composition>,
    /// Models of removed fields whose finished composition commits once they mount again: only
    /// the view that shows a field can reach its model.
    unplaced: Vec<Model>,
}

impl<S> Default for AppIme<S> {
    fn default() -> Self { Self { hook: None, composition: None, unplaced: Vec::new() } }
}

impl<S> AppIme<S> {
    /// The app's editor has a composition open: Return and Escape belong to the input method.
    pub(super) fn composing(&self) -> bool {
        self.hook.is_some() && matches!(self.composition, Some(Composition::Open(Owner::App { .. })))
    }

    /// `edit` is a removed field's finished composition, still to commit.
    pub(super) fn awaits(&self, edit: &crate::edit::TextEdit) -> bool {
        self.unplaced.iter().any(|model| edit.matches_history(model))
    }

    /// A key pressed after Rune finished a composition: the input method speaks for the new
    /// focus from here on.
    pub(super) fn key_pressed(&mut self) {
        if matches!(self.composition, Some(Composition::Finished)) { self.composition = None; }
    }
}

/// An `El::ime_cursor` element's drawn rect, device pixels, as the window's IME area in points.
pub(super) fn area(r: [f32; 4], s: f32) -> [f32; 4] {
    [r[0] / s, r[1] / s, (r[2] - r[0]) / s, (r[3] - r[1]) / s]
}

fn commit_preedit(edit: &mut crate::edit::TextEdit, model: &Model) -> EditOutcome {
    if !edit.matches_history(model) { return EditOutcome::default(); }
    match edit.preedit.take() {
        Some((text, _)) => edit.ime(&Ime::Commit(text)),
        None => EditOutcome::default(),
    }
}

impl<S: 'static> Runtime<S> {
    /// An input method event goes to the focused text field, else to `App::on_ime`; turning
    /// on or off is only the app's to hear.
    pub(super) fn run_ime(&mut self, ev: Ime, one: &mut Tx) -> bool {
        let finished = matches!(self.app_ime.composition, Some(Composition::Finished));
        match &ev {
            Ime::Preedit(s, _) if finished && s.is_empty() => return false,
            Ime::Commit(_) if finished => { self.app_ime.composition = None; return false; }
            _ => {}
        }
        let app = matches!(ev, Ime::Enabled | Ime::Disabled) || self.focused_input().is_none();
        let active = self.composing() || matches!(&ev, Ime::Preedit(s, _) | Ime::Commit(s) if !s.is_empty());
        let mut model = None;
        let ran = if app { self.app_ime_event(&ev, one) } else {
            self.with_focused(|e| { let out = e.ime(&ev); model = Some(e.weak_history()); out }, one)
        };
        match &ev {
            Ime::Preedit(s, _) if !s.is_empty() => {
                let owner = model.map_or_else(|| Owner::App { shown: s.clone() }, Owner::Field);
                self.app_ime.composition = Some(Composition::Open(owner));
            }
            Ime::Preedit(..) | Ime::Commit(_) => self.app_ime.composition = None,
            Ime::Enabled | Ime::Disabled => {}
        }
        // The key that confirmed or cancelled a composition comes back up unheard.
        if ran && active { self.ime_eat_key_up = true; }
        ran
    }

    /// Ends the composition as the window stops taking keys (it deactivates or hides): its text
    /// commits in the editor it began in, which keeps focus, and the platform's input method
    /// resets.
    pub fn cancel_composition(&mut self) {
        self.finish_composition();
        self.ime_reset = true;
    }

    /// Focus is about to move to `path`: a composition whose editor loses the input method by it
    /// (or already lost it, a new model now under focus) finishes there first. Whether that
    /// changed state.
    pub(super) fn finish_composition_off(&mut self, path: Option<&str>) -> bool {
        let stays = match &self.app_ime.composition {
            Some(Composition::Open(Owner::App { .. })) => self.focused.as_deref() == path && self.focused_input().is_none(),
            Some(Composition::Open(Owner::Field(model))) => self.focused.as_deref() == path
                && self.focused_input().is_some_and(|i| i.edit.matches_history(model)),
            _ => true,
        };
        !stays && self.finish_composition()
    }

    /// A removed field's finished composition commits once the view shows its model again.
    pub(super) fn place_unplaced(&mut self) {
        for model in std::mem::take(&mut self.app_ime.unplaced) {
            if !self.commit_in(&model) && model.strong_count() > 0 { self.app_ime.unplaced.push(model); }
        }
    }

    /// Commits the open composition's text in the editor it began in. Its platform answer is
    /// dropped from now on.
    fn finish_composition(&mut self) -> bool {
        let owner = match self.app_ime.composition.take() {
            Some(Composition::Open(owner)) => owner,
            other => { self.app_ime.composition = other; return false; }
        };
        self.app_ime.composition = Some(Composition::Finished);
        self.ime_reset = true;
        match owner {
            Owner::App { shown } => {
                let mut one = self.transaction();
                let ran = self.app_ime_event(&Ime::Commit(shown), &mut one);
                self.outside_frame(one);
                ran
            }
            Owner::Field(model) => self.commit_in(&model) || { self.app_ime.unplaced.push(model); false },
        }
    }

    /// Commits the preedit of the field that edits `model`, if the view shows one, as that
    /// field's own edit: it hears `on_input_change`.
    fn commit_in(&mut self, model: &Model) -> bool {
        let mounted = self.nodes.iter().enumerate()
            .find(|&(i, n)| n.input.as_ref().is_some_and(|f| f.edit.matches_history(model))
                && !self.subtree_exiting(i) && !self.subtree_unmounted(i))
            .map(|(_, n)| n.path.clone())
            .or_else(|| self.virtual_inputs.iter().find(|(_, f)| f.edit.matches_history(model)).map(|(p, _)| p.clone()));
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
