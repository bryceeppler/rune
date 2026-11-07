//! Asking the app before a window closes or the app quits (`App::on_close_request`). Each window
//! asks its own hook and keeps one request in flight per kind, so a second close button click or
//! ⌘Q while a save runs joins the first. The answer travels in the `CloseRequest` itself: the
//! hook answers at once, or keeps the request and answers from any thread when its work ends.
//! A request covers all input the app was given: input queued when Rune asks reaches the app
//! first, and input that reaches it while the request is out makes a proceed ask again.

use std::sync::{Arc, Mutex};

use crate::el::Tx;
use crate::runtime::{Runtime, Waker};

/// What Rune is about to end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CloseKind {
    /// This window: its close button, ⌘W, Alt-F4 or the taskbar's Close, `Tx::close_window`,
    /// `AppHandle::close` or `WindowHandle::close`.
    Window,
    /// The whole app: macOS Quit from the menu, ⌘Q, the Dock or a logout, or `Tx::request_quit`.
    /// Every window with a hook is asked; the app quits once all of them proceed.
    Quit,
}

/// Where a request Rune asked stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CloseAnswer {
    /// The app holds the request and has not answered yet.
    Pending,
    Proceed,
    Cancel,
}

/// A close or quit the app may allow now, refuse, or settle later from any thread. Answering
/// consumes it, so each request is answered once; dropping it unanswered cancels, so work that
/// fails or panics leaves the window open rather than losing what it was saving.
#[must_use = "dropping a close request cancels it"]
pub struct CloseRequest {
    kind: CloseKind,
    slot: Arc<Slot>,
}

impl CloseRequest {
    pub fn kind(&self) -> CloseKind { self.kind }

    /// Go ahead: the window closes, or the app quits once every window has proceeded.
    pub fn proceed(self) { self.slot.answer(CloseAnswer::Proceed); }

    /// Keep the window and the app running.
    pub fn cancel(self) { self.slot.answer(CloseAnswer::Cancel); }
}

impl Drop for CloseRequest {
    fn drop(&mut self) { self.slot.answer(CloseAnswer::Cancel); }
}

impl std::fmt::Debug for CloseRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CloseRequest").field("kind", &self.kind).field("answer", &self.slot.get()).finish()
    }
}

struct Slot {
    answer: Mutex<CloseAnswer>,
    wake: Waker,
}

impl Slot {
    fn get(&self) -> CloseAnswer { *self.answer.lock().unwrap() }

    /// The first answer stands; a later one (a drop after `proceed`) changes nothing.
    fn answer(&self, answer: CloseAnswer) {
        let mut current = self.answer.lock().unwrap();
        if *current != CloseAnswer::Pending { return; }
        *current = answer;
        drop(current);
        (self.wake)();
    }
}

pub(crate) type Hook<S> = fn(&mut S, CloseRequest, &mut Tx);

/// A request of one kind that Rune owes the hook or has given it.
enum Asked {
    /// To ask once the input queued when it was made reaches the app (`Runtime::ask_due_closes`).
    Due,
    /// With the app, which had heard `heard` inputs (`Closing::heard`) when asked.
    Out { slot: Arc<Slot>, heard: u64 },
}

/// A window's hook and the requests it holds, one per kind.
pub(crate) struct Closing<S> {
    hook: Option<Hook<S>>,
    window: Option<Asked>,
    quit: Option<Asked>,
    /// Input the app has been given, counted so an answer given before some of it is stale.
    heard: u64,
    /// Wakes the window's host when the hook was asked or answered, to carry out what it asked.
    wake: Waker,
}

impl<S> Default for Closing<S> {
    fn default() -> Self { Self { hook: None, window: None, quit: None, heard: 0, wake: Arc::new(|| {}) } }
}

impl<S> Closing<S> {
    fn slot(&mut self, kind: CloseKind) -> &mut Option<Asked> {
        match kind { CloseKind::Window => &mut self.window, CloseKind::Quit => &mut self.quit }
    }

    pub(crate) fn set_wake(&mut self, wake: Waker) { self.wake = wake; }

    /// The app was given input a person made, which a save already under way may have missed.
    pub(crate) fn heard_input(&mut self) { self.heard += 1; }
}

impl<S: 'static> crate::App<S> {
    /// Asks the app before its window closes or the app quits, and lets it answer now, never,
    /// or later, after an asynchronous save. The hook gets each request with what it ends
    /// ([`CloseKind`](crate::CloseKind)): proceed or cancel it in the hook, or keep it and
    /// answer from any thread once the work ends; dropping it cancels. A close that proceeds
    /// closes as before (`on_close` runs). A quit asks every window's hook and quits once all of
    /// them proceed: macOS Quit from the menu, ⌘Q, the Dock or a logout (AppKit's reply-later
    /// termination), and `Tx::request_quit` everywhere. `Tx::quit` and `AppHandle::quit` ask
    /// nothing. A request covers all the input the window was given: input queued when Rune
    /// asks reaches the app before the hook, and input that reaches it while the app holds the
    /// request makes a proceed ask the hook again, so a save that started before the last
    /// keystroke never closes the window. Endings the system won't wait for (a Windows session
    /// end, `SIGTERM`, a display server that goes away) run `on_close` alone, so do the last
    /// synchronous save there.
    pub fn on_close_request(mut self, f: fn(&mut S, CloseRequest, &mut Tx)) -> Self {
        self.on_close_request = Some(f);
        self
    }
}

impl<S: 'static> Runtime<S> {
    /// Sets the hook `request_close` asks (`App::on_close_request` sets it for each window).
    pub fn on_close_request(&mut self, hook: fn(&mut S, CloseRequest, &mut Tx)) {
        self.closing.hook = Some(hook);
    }

    /// Asks the hook whether `kind` may go ahead, as the window's host does for a close button
    /// or a quit, and returns its answer so far. Without a hook it proceeds. Input still queued
    /// reaches the app first, so the hook is asked in the next frame and this is `Pending`. A
    /// pending request stays in flight for `settle_close`; while it does, another request of
    /// this kind joins it rather than asking again.
    pub fn request_close(&mut self, kind: CloseKind) -> CloseAnswer {
        // A composition open in the window commits first, so the app saves what it shows.
        self.cancel_composition();
        if self.closing.hook.is_none() { return CloseAnswer::Proceed; }
        if self.closing.slot(kind).is_none() { self.ask_close(kind); }
        self.settle_close(kind).unwrap_or(CloseAnswer::Pending)
    }

    /// The request of `kind` in flight: None when there is none, `Pending` while the app holds
    /// it, and its answer once it has one. A cancel, and a window's proceed, are taken; a quit's
    /// proceed stays until the quit across windows is decided (`withdraw_close`), since another
    /// window may still be saving. A proceed waits for input still queued, and asks the hook
    /// again when the app was given input after it was asked.
    pub fn settle_close(&mut self, kind: CloseKind) -> Option<CloseAnswer> {
        let queued = self.has_pending_input();
        let (answer, heard) = match self.closing.slot(kind).as_ref()? {
            Asked::Due if queued => return Some(CloseAnswer::Pending),
            Asked::Due => { self.hand_close(kind); return self.settle_close(kind); }
            Asked::Out { slot, heard } => (slot.get(), *heard),
        };
        match answer {
            CloseAnswer::Pending => Some(CloseAnswer::Pending),
            CloseAnswer::Proceed if queued => Some(CloseAnswer::Pending),
            CloseAnswer::Proceed if heard != self.closing.heard => { self.ask_close(kind); self.settle_close(kind) }
            CloseAnswer::Proceed if kind == CloseKind::Quit => Some(CloseAnswer::Proceed),
            CloseAnswer::Proceed | CloseAnswer::Cancel => {
                *self.closing.slot(kind) = None;
                Some(answer)
            }
        }
    }

    /// Forgets the request of `kind`: a later answer to it changes nothing.
    pub(crate) fn withdraw_close(&mut self, kind: CloseKind) {
        *self.closing.slot(kind) = None;
    }

    /// Asks the hook requests waited on this frame's input for (`Asked::Due`), now that the
    /// input has reached the app.
    pub(crate) fn ask_due_closes(&mut self) {
        for kind in [CloseKind::Window, CloseKind::Quit] {
            if matches!(self.closing.slot(kind), Some(Asked::Due)) { self.hand_close(kind); }
        }
    }

    /// Asks the hook about `kind` once the input queued now has reached the app: at once when
    /// none is queued, else in the next frame, after that input.
    fn ask_close(&mut self, kind: CloseKind) {
        if self.has_pending_input() { *self.closing.slot(kind) = Some(Asked::Due) } else { self.hand_close(kind) }
    }

    fn hand_close(&mut self, kind: CloseKind) {
        let Some(hook) = self.closing.hook else { return };
        let slot = Arc::new(Slot { answer: Mutex::new(CloseAnswer::Pending), wake: self.closing.wake.clone() });
        *self.closing.slot(kind) = Some(Asked::Out { slot: slot.clone(), heard: self.closing.heard });
        let request = CloseRequest { kind, slot };
        self.update(move |s, tx| hook(s, request, tx));
        (self.closing.wake)();
    }
}

/// Where a quit stands across windows: cancelled once any window cancels, going ahead once all
/// of them proceed, pending otherwise.
pub(crate) fn all_of(answers: impl IntoIterator<Item = CloseAnswer>) -> CloseAnswer {
    answers.into_iter().fold(CloseAnswer::Proceed, |all, one| match (all, one) {
        (CloseAnswer::Cancel, _) | (_, CloseAnswer::Cancel) => CloseAnswer::Cancel,
        (CloseAnswer::Pending, _) | (_, CloseAnswer::Pending) => CloseAnswer::Pending,
        _ => CloseAnswer::Proceed,
    })
}

/// Where a quit stands across windows given each window's answer so far, one per window, and
/// `settle`, which reads window `i`'s request again (None when it holds none, which proceeds).
/// A cancel ends the quit; every other answer is read again each time, since input that reaches
/// a window after it proceeded asks it again (`Runtime::settle_close`).
pub(crate) fn settle_all(answers: &mut [CloseAnswer], mut settle: impl FnMut(usize) -> Option<CloseAnswer>) -> CloseAnswer {
    for (i, answer) in answers.iter_mut().enumerate() {
        if *answer != CloseAnswer::Cancel { *answer = settle(i).unwrap_or(CloseAnswer::Proceed); }
    }
    all_of(answers.iter().copied())
}

#[cfg(test)]
#[path = "close_request_tests.rs"]
mod tests;
