//! Input method events: the focused text field hears them, else `App::on_ime` (an editor the
//! app draws itself, whose `El::ime_cursor` caret has the input method). A composition belongs
//! to the editor it began in until it ends, so what the input method sends for it after focus
//! moves on still lands there.
use super::*;
use std::sync::Weak;

/// The editor a composition began in: the app's own, or a text field's model.
#[derive(Clone)]
pub(super) enum Owner {
    App,
    Field(Weak<crate::edit::History>),
}

impl PartialEq for Owner {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::App, Self::App) => true,
            (Self::Field(a), Self::Field(b)) => a.ptr_eq(b),
            _ => false,
        }
    }
}

/// An open composition. Once its editor loses the input method it is closing: the shell resets
/// the platform's input method, whose answer (the preedit clearing, then a commit or nothing)
/// is still the owner's until the input method turns off or starts another composition.
struct Composition {
    owner: Owner,
    closing: bool,
}

/// `App::on_ime`, and the composition open in any editor.
pub(crate) struct AppIme<S> {
    pub(crate) hook: Option<fn(&mut S, &Ime, &mut Tx) -> bool>,
    composition: Option<Composition>,
}

impl<S> Default for AppIme<S> {
    fn default() -> Self { Self { hook: None, composition: None } }
}

impl<S> AppIme<S> {
    /// The app's editor has a composition open: Return and Escape belong to the input method.
    pub(super) fn composing(&self) -> bool {
        self.hook.is_some() && self.composition.as_ref().is_some_and(|c| c.owner == Owner::App && !c.closing)
    }
}

/// An `El::ime_cursor` element's drawn rect, device pixels, as the window's IME area in points.
pub(super) fn area(r: [f32; 4], s: f32) -> [f32; 4] {
    [r[0] / s, r[1] / s, (r[2] - r[0]) / s, (r[3] - r[1]) / s]
}

/// Who hears `ev`. The input method turning on or off is only the app's to hear.
fn route(ev: &Ime, open: Option<&Composition>, target: Owner) -> Owner {
    match (ev, open) {
        (Ime::Enabled | Ime::Disabled, _) => Owner::App,
        (Ime::Preedit(s, _), Some(c)) if c.closing && s.is_empty() => c.owner.clone(),
        (Ime::Commit(_), Some(c)) if c.closing => c.owner.clone(),
        _ => target,
    }
}

/// What stays open once `to` heard `ev`. A clearing preedit before a commit doesn't end a closing
/// composition: input methods send the pair when they commit.
fn after(ev: &Ime, open: Option<Composition>, to: Owner) -> Option<Composition> {
    match ev {
        Ime::Preedit(s, _) if !s.is_empty() => Some(Composition { owner: to, closing: false }),
        Ime::Preedit(..) => open.filter(|c| c.closing),
        Ime::Commit(_) => None,
        Ime::Enabled | Ime::Disabled => open.filter(|c| !c.closing),
    }
}

impl<S: 'static> Runtime<S> {
    pub(super) fn run_ime(&mut self, ev: Ime, one: &mut Tx) -> bool {
        let target = self.ime_target();
        let composing = self.composing();
        let open = self.app_ime.composition.take();
        let to = route(&ev, open.as_ref(), target.clone());
        // An answer to a reset came from no key the user pressed in this editor.
        let answer = to != target && !matches!(ev, Ime::Enabled | Ime::Disabled);
        let active = !answer && (composing || matches!(&ev, Ime::Preedit(s, _) | Ime::Commit(s) if !s.is_empty()));
        let ran = match &to {
            Owner::App => self.app_ime_event(&ev, one),
            Owner::Field(_) if to == target => self.with_focused(|e| e.ime(&ev), one),
            Owner::Field(model) => self.owner_ime_event(model, &ev, one),
        };
        self.app_ime.composition = after(&ev, open, to);
        // The key that confirmed or cancelled a composition comes back up unheard.
        if ran && active { self.ime_eat_key_up = true; }
        ran
    }

    /// End native composition on window deactivation without discarding text or field focus.
    /// The input method's answer still goes to the editor that composed, wherever focus is by then.
    pub fn cancel_composition(&mut self) {
        if let Some(c) = &mut self.app_ime.composition { c.closing = true; }
        self.ime(Ime::Preedit(String::new(), None));
        self.ime_reset = true;
    }

    /// Focus moved off the editor a composition is open in: the shell resets the platform's input
    /// method before it serves the new focus.
    pub(super) fn close_composition_off_focus(&mut self) {
        let target = self.ime_target();
        if let Some(c) = &mut self.app_ime.composition && !c.closing && c.owner != target {
            c.closing = true;
            self.ime_reset = true;
        }
    }

    fn ime_target(&self) -> Owner {
        self.focused_input().map_or(Owner::App, |i| Owner::Field(i.edit.weak_history()))
    }

    /// A closing composition's end, for a field that no longer has focus. Gone with its field.
    fn owner_ime_event(&mut self, model: &Weak<crate::edit::History>, ev: &Ime, one: &mut Tx) -> bool {
        let mounted = self.nodes.iter().enumerate()
            .find(|&(i, n)| n.input.as_ref().is_some_and(|f| f.edit.matches_history(model))
                && !self.subtree_exiting(i) && !self.subtree_unmounted(i))
            .map(|(_, n)| n.path.clone())
            .or_else(|| self.virtual_inputs.iter().find(|(_, f)| f.edit.matches_history(model)).map(|(p, _)| p.clone()));
        let Some(path) = mounted else { return false };
        self.with_input_state(path, |state, access, _| {
            let edit = access(state);
            if edit.matches_history(model) { edit.ime(ev) } else { EditOutcome::default() }
        }, one)
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
