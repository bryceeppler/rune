//! Quitting across windows. A quit asks every window's `App::on_close_request` and waits until
//! all of them proceed or one cancels; then the asker hears the answer: AppKit's deferred
//! termination on macOS, the loop itself elsewhere. The system ending the app without asking
//! (`end_session`) runs every window's `App::on_close` instead.

use super::*;
use crate::close_request::{CloseAnswer, all_of};
use crate::platform::quit::QuitFrom;

static PROXY: Mutex<Option<EventLoopProxy<WakeEvent>>> = Mutex::new(None);

/// The loop `request` sends quits to.
pub(super) fn connect(proxy: EventLoopProxy<WakeEvent>) {
    *PROXY.lock().unwrap() = Some(proxy);
}

/// Starts a quit for `Tx::request_quit`: through AppKit on macOS, as Quit from the menu does,
/// so the system's quit and the app's own are one; straight to the loop elsewhere.
pub(crate) fn request() {
    #[cfg(target_os = "macos")]
    crate::platform::quit::terminate();
    #[cfg(not(target_os = "macos"))]
    if let Some(proxy) = PROXY.lock().unwrap().as_ref() { let _ = proxy.send_event(WakeEvent::Quit(QuitFrom::Rune)); }
}

/// A quit waiting on windows' answers, one per host slot.
pub(super) struct Quitting {
    from: QuitFrom,
    answers: Vec<CloseAnswer>,
}

impl Windows {
    /// Asks every window whether the app may quit. A quit already waiting takes this one in.
    pub(super) fn ask_quit(&mut self, event_loop: &ActiveEventLoop, from: QuitFrom) {
        if self.quitting.is_some() { return; }
        let answers = self.hosts.iter_mut().map(|h| h.as_mut().map_or(CloseAnswer::Proceed, |h| h.ask_quit(event_loop))).collect();
        self.quitting = Some(Quitting { from, answers });
        self.settle_quit(event_loop);
    }

    /// Collects the answers that arrived and, once the quit is decided, answers its asker.
    pub(super) fn settle_quit(&mut self, event_loop: &ActiveEventLoop) {
        let Some(quitting) = &mut self.quitting else { return };
        for (answer, host) in quitting.answers.iter_mut().zip(&mut self.hosts) {
            if *answer == CloseAnswer::Pending {
                *answer = host.as_mut().and_then(|h| h.settle_quit()).unwrap_or(CloseAnswer::Proceed);
            }
        }
        let verdict = all_of(quitting.answers.iter().copied());
        if verdict == CloseAnswer::Pending { return; }
        let from = quitting.from;
        self.quitting = None;
        for host in self.hosts.iter_mut().flatten() { host.withdraw_quit(); }
        let proceed = verdict == CloseAnswer::Proceed;
        match from {
            #[cfg(target_os = "macos")]
            QuitFrom::AppKit => crate::platform::quit::reply(proceed),
            _ if proceed => {
                self.end_session();
                event_loop.exit();
            }
            _ => {}
        }
    }

    /// Every window closes as its close button would, asking nothing: the system is ending
    /// the app, or every window agreed to quit.
    pub(super) fn end_session(&mut self) {
        for host in self.hosts.iter_mut().flatten() { host.terminate(); }
    }
}
