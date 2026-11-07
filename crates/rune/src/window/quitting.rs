//! Quitting across windows. A quit asks every window's `App::on_close_request` and waits until
//! all of them proceed or one cancels; then the asker hears the answer: AppKit's deferred
//! termination on macOS, Windows' session query, the loop itself elsewhere. The system ending
//! the app without asking (`end_session`) runs every window's `App::on_close` instead.

use super::*;
use crate::close_request::{CloseAnswer, settle_all};
use crate::platform::quit::{QuitFrom, SessionReply, SessionStep};

type Hosts = [Option<Box<dyn AnyHost>>];

static PROXY: Mutex<Option<EventLoopProxy<WakeEvent>>> = Mutex::new(None);

/// The loop `request` sends quits to.
pub(super) fn connect(proxy: EventLoopProxy<WakeEvent>) {
    *PROXY.lock().unwrap() = Some(proxy);
}

fn send(event: WakeEvent) {
    if let Some(proxy) = PROXY.lock().unwrap().as_ref() { let _ = proxy.send_event(event); }
}

/// Starts a quit for `Tx::request_quit`: through AppKit on macOS, as Quit from the menu does,
/// so the system's quit and the app's own are one; straight to the loop elsewhere.
pub(crate) fn request() {
    #[cfg(target_os = "macos")]
    crate::platform::quit::terminate();
    #[cfg(not(target_os = "macos"))]
    send(WakeEvent::Quit(QuitFrom::Rune));
}

/// A quit waiting on windows' answers, one per host slot.
pub(super) struct Quitting {
    from: QuitFrom,
    answers: Vec<CloseAnswer>,
}

impl Quitting {
    fn ask(hosts: &mut Hosts, from: QuitFrom) -> Self {
        Quitting { from, answers: hosts.iter_mut().map(|h| h.as_mut().map_or(CloseAnswer::Proceed, |h| h.ask_quit())).collect() }
    }

    /// Reads every window's answer again (`settle_all`). Once the quit is decided, every
    /// window's request is withdrawn and this is the verdict; `Pending` until then.
    fn settle(&mut self, hosts: &mut Hosts) -> CloseAnswer {
        let verdict = settle_all(&mut self.answers, |i| hosts.get_mut(i).and_then(|h| h.as_mut()).and_then(|h| h.settle_quit()));
        if verdict != CloseAnswer::Pending { withdraw(hosts); }
        verdict
    }
}

fn withdraw(hosts: &mut Hosts) {
    for host in hosts.iter_mut().flatten() { host.withdraw_quit(); }
}

/// Every window closes as its close button would, asking nothing.
fn end(hosts: &mut Hosts) {
    for host in hosts.iter_mut().flatten() { host.terminate(); }
}

/// Windows ending the session hears `hosts` from inside a window procedure, without the loop.
/// The query asks every window's hook as a quit: once all proceed every window closes, running
/// `on_close`, before the query answers that the session may end, so the app's last save never
/// waits on `WM_ENDSESSION`. The session waits while a window saves, the loop finishing the quit
/// when it answers (`Windows::settle_quit`), and is refused when one cancels. The end runs every
/// window's `on_close` still due: the synchronous last chance of a session Windows ends without
/// a query, or forces after the app made it wait. A session that goes on after all forgets the
/// quit the query asked.
pub(super) fn session(hosts: &mut Hosts, quitting: &mut Option<Quitting>, step: SessionStep) -> SessionReply {
    match step {
        SessionStep::Query => {
            let quit = quitting.get_or_insert_with(|| Quitting::ask(hosts, QuitFrom::Session));
            quit.from = QuitFrom::Session;
            let verdict = quit.settle(hosts);
            if verdict != CloseAnswer::Pending { *quitting = None; }
            match verdict {
                CloseAnswer::Proceed => { end(hosts); SessionReply::Ends }
                CloseAnswer::Pending => SessionReply::Waits,
                CloseAnswer::Cancel => SessionReply::Refuses,
            }
        }
        SessionStep::Ends => {
            *quitting = None;
            end(hosts);
            SessionReply::Ends
        }
        SessionStep::Continues => {
            if quitting.take_if(|q| q.from == QuitFrom::Session).is_some() { withdraw(hosts); }
            SessionReply::Ends
        }
    }
}

/// A session step that arrived while an event holds the windows, from a message loop that
/// event's handler runs (a modal dialog an app opens), whose handler holds the window's state
/// borrowed, so nothing of the app can run safely until it returns. The query makes the session
/// wait and reaches the loop as a quit once that event ends, so a session Windows asks about
/// never ends on this path. An end that arrives here, which only a forced session end can bring
/// after that wait, can't run `on_close` before it returns: the loop runs it once the event
/// ends, if Windows has not ended the process by then.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(super) fn session_later(step: SessionStep) -> SessionReply {
    match step {
        SessionStep::Query => { send(WakeEvent::Quit(QuitFrom::Session)); SessionReply::Waits }
        SessionStep::Ends => { send(WakeEvent::SessionEnd); SessionReply::Ends }
        SessionStep::Continues => SessionReply::Ends,
    }
}

/// The app ends with its last main window: the windows still open are asked to quit, as Quit
/// from the menu asks them, and asked again each time fewer of them show, so a window that
/// refuses keeps the app running only while it shows.
#[derive(Default)]
pub(crate) struct LastMain {
    asked_showing: Option<usize>,
}

impl LastMain {
    /// Whether to ask the windows to quit now, given whether a main window is open and how many
    /// windows show.
    pub(crate) fn ask(&mut self, main_open: bool, showing: usize) -> bool {
        if main_open { self.asked_showing = None; return false; }
        if self.asked_showing.is_some_and(|asked| showing >= asked) { return false; }
        self.asked_showing = Some(showing);
        true
    }
}

impl Windows {
    /// Asks every window whether the app may quit. A quit already waiting takes this one in.
    pub(super) fn ask_quit(&mut self, event_loop: &ActiveEventLoop, from: QuitFrom) {
        if self.quitting.is_none() { self.quitting = Some(Quitting::ask(&mut self.hosts, from)); }
        self.settle_quit(event_loop);
    }

    /// Collects the answers that arrived and, once the quit is decided, answers its asker.
    pub(super) fn settle_quit(&mut self, event_loop: &ActiveEventLoop) {
        let Some(quitting) = &mut self.quitting else { return };
        let verdict = quitting.settle(&mut self.hosts);
        if verdict == CloseAnswer::Pending { return; }
        let from = quitting.from;
        self.quitting = None;
        let proceed = verdict == CloseAnswer::Proceed;
        match from {
            #[cfg(target_os = "macos")]
            QuitFrom::AppKit => crate::platform::quit::reply(proceed),
            _ if proceed => {
                self.end_session();
                event_loop.exit();
            }
            #[cfg(target_os = "windows")]
            QuitFrom::Session => crate::platform::quit::unblock(),
            _ => {}
        }
    }

    /// Every window closes as its close button would, asking nothing: the system is ending
    /// the app, or every window agreed to quit.
    pub(super) fn end_session(&mut self) { end(&mut self.hosts); }

    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub(super) fn session(&mut self, step: SessionStep) -> SessionReply { session(&mut self.hosts, &mut self.quitting, step) }

    /// Once no main window is open, asks the windows left whether the app may quit
    /// (`LastMain`); an exit already under way asks nothing.
    pub(super) fn end_with_last_main(&mut self, event_loop: &ActiveEventLoop) {
        if event_loop.exiting() || self.quitting.is_some() { return; }
        let main_open = self.hosts.iter().flatten().any(|h| h.keeps_loop());
        let showing = self.hosts.iter().flatten().filter(|h| { let c = h.reopen_candidate(); c.visible || c.minimized }).count();
        if self.last_main.ask(main_open, showing) { self.ask_quit(event_loop, QuitFrom::Rune); }
    }
}

#[cfg(test)]
#[path = "quitting_tests.rs"]
mod tests;
