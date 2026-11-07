//! Wayland's input method (`zwp_text_input_v3`), run by Rune on winit's display connection, in
//! a private queue, for winit's surface. winit's own client drops each `done` event's serial,
//! so a compositor's answer for a session Rune reset reached the next focus. A reset here
//! starts a new session, and a `done` whose serial predates that session's `enable` is dropped.

use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use wayland_backend::client::{Backend, ObjectId};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_registry::WlRegistry, wl_seat::WlSeat, wl_surface::WlSurface};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_manager_v3::ZwpTextInputManagerV3;
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::{ContentHint, ContentPurpose, Event, ZwpTextInputV3};
use winit::window::Window;

use crate::Ime;

/// The protocol state Rune keeps, apart from the wire.
#[derive(Default)]
pub(super) struct Session {
    /// The seat's keyboard focus is on this window's surface.
    entered: bool,
    /// Rune wants the input method on.
    allowed: bool,
    enabled: bool,
    /// `commit` requests sent.
    commits: u32,
    /// `commits` once the current session's `enable` was sent: a `done` with an earlier serial
    /// answers a session that ended.
    since: u32,
    /// The caret's rect, surface points.
    area: Option<[i32; 4]>,
    preedit: Option<(String, Option<(usize, usize)>)>,
    commit: Option<String>,
}

impl Session {
    /// A `done` event: the batch it closes, as the runtime's events, unless it answers a
    /// session that ended.
    pub(super) fn done(&mut self, serial: u32) -> Vec<Ime> {
        let (preedit, commit) = (self.preedit.take(), self.commit.take());
        if !self.enabled || serial < self.since { return Vec::new(); }
        let mut events = Vec::new();
        // An empty batch clears the preedit; a commit replaces it.
        if commit.is_some() || preedit.is_none() { events.push(Ime::Preedit(String::new(), None)); }
        events.extend(commit.map(Ime::Commit));
        events.extend(preedit.map(|(text, cursor)| Ime::Preedit(text, cursor)));
        events
    }

    pub(super) fn preedit(&mut self, text: Option<String>, begin: i32, end: i32) {
        let text = text.unwrap_or_default();
        let at = |i: i32| usize::try_from(i).ok().filter(|&i| text.is_char_boundary(i));
        let cursor = at(begin).map(|b| (b, at(end).unwrap_or(b)));
        self.preedit = Some((text, cursor));
    }

    pub(super) fn commit_string(&mut self, text: Option<String>) {
        self.preedit = None;
        self.commit = text;
    }

    fn sent(&mut self) { self.commits = self.commits.wrapping_add(1); }

    /// Enabled now, a new session from the next commit on.
    pub(super) fn enabling(&mut self) {
        self.enabled = true;
        self.sent();
        self.since = self.commits;
    }

    pub(super) fn disabling(&mut self) {
        self.enabled = false;
        self.sent();
    }
}

struct State {
    surface: ObjectId,
    session: Session,
    events: Vec<Ime>,
}

/// One window's text input, made on first use and kept with the window.
pub(crate) struct TextInput {
    queue: EventQueue<State>,
    state: State,
    manager: ZwpTextInputManagerV3,
    seat: WlSeat,
    text_input: ZwpTextInputV3,
}

impl TextInput {
    /// None off Wayland or when the compositor offers no text input.
    pub(crate) fn new(window: &Window) -> Option<TextInput> {
        let (RawDisplayHandle::Wayland(display), RawWindowHandle::Wayland(handle)) = (
            window.display_handle().ok()?.as_raw(),
            window.window_handle().ok()?.as_raw(),
        ) else {
            return None;
        };
        // SAFETY: winit owns this `wl_display` and keeps it alive for the event loop, which
        // outlives every window; a foreign backend never closes it.
        let backend = unsafe { Backend::from_foreign_display(display.display.as_ptr().cast()) };
        let conn = Connection::from_backend(backend);
        let (globals, queue) = registry_queue_init::<State>(&conn).ok()?;
        let qh = queue.handle();
        let manager = globals.bind::<ZwpTextInputManagerV3, _, _>(&qh, 1..=1, ()).ok()?;
        let seat = globals.bind::<WlSeat, _, _>(&qh, 1..=5, ()).ok()?;
        let text_input = manager.get_text_input(&seat, &qh, ());
        // SAFETY: the pointer is the live `wl_surface` proxy winit created for this window, and
        // the text input is dropped before the window.
        let surface = unsafe { ObjectId::from_ptr(WlSurface::interface(), handle.surface.as_ptr().cast()) }.ok()?;
        let state = State { surface, session: Session::default(), events: Vec::new() };
        let _ = queue.flush();
        Some(TextInput { queue, state, manager, seat, text_input })
    }

    /// Ends the session and, if `allowed`, starts a new one: nothing the compositor sends for
    /// the old one reaches the runtime.
    pub(crate) fn reset(&mut self, allowed: bool) {
        let State { session, events, .. } = &mut self.state;
        session.allowed = allowed;
        disable(&self.text_input, session, events);
        if allowed && session.entered { enable(&self.text_input, session, events); }
        let _ = self.queue.flush();
    }

    /// The caret's rect, window points.
    pub(crate) fn set_area(&mut self, area: [f32; 4]) {
        let area = area.map(|v| v.round() as i32);
        let session = &mut self.state.session;
        if session.area == Some(area) { return; }
        session.area = Some(area);
        if session.enabled {
            self.text_input.set_cursor_rectangle(area[0], area[1], area[2], area[3]);
            self.text_input.commit();
            session.sent();
            let _ = self.queue.flush();
        }
    }

    /// What the compositor sent since the last call, as the runtime's events.
    pub(crate) fn poll(&mut self) -> Vec<Ime> {
        let _ = self.queue.dispatch_pending(&mut self.state);
        let _ = self.queue.flush();
        std::mem::take(&mut self.state.events)
    }
}

impl Drop for TextInput {
    fn drop(&mut self) {
        self.text_input.destroy();
        self.manager.destroy();
        if self.seat.version() >= 5 { self.seat.release(); }
        let _ = self.queue.flush();
    }
}

fn enable(text_input: &ZwpTextInputV3, session: &mut Session, events: &mut Vec<Ime>) {
    text_input.enable();
    text_input.set_content_type(ContentHint::None, ContentPurpose::Normal);
    if let Some([x, y, w, h]) = session.area { text_input.set_cursor_rectangle(x, y, w, h); }
    text_input.commit();
    session.enabling();
    events.push(Ime::Enabled);
}

fn disable(text_input: &ZwpTextInputV3, session: &mut Session, events: &mut Vec<Ime>) {
    if !session.enabled { return; }
    text_input.disable();
    text_input.commit();
    session.disabling();
    events.push(Ime::Disabled);
}

impl Dispatch<ZwpTextInputV3, ()> for State {
    fn event(state: &mut Self, text_input: &ZwpTextInputV3, event: Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        let State { surface, session, events } = state;
        match event {
            Event::Enter { surface: entered } if entered.id() == *surface => {
                session.entered = true;
                if session.allowed { enable(text_input, session, events); }
            }
            Event::Leave { surface: left } if left.id() == *surface => {
                session.entered = false;
                disable(text_input, session, events);
            }
            Event::PreeditString { text, cursor_begin, cursor_end } => session.preedit(text, cursor_begin, cursor_end),
            Event::CommitString { text } => session.commit_string(text),
            Event::Done { serial } => events.extend(session.done(serial)),
            _ => {}
        }
    }
}

impl Dispatch<ZwpTextInputManagerV3, ()> for State {
    fn event(_: &mut Self, _: &ZwpTextInputManagerV3, _: <ZwpTextInputManagerV3 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<WlSeat, ()> for State {
    fn event(_: &mut Self, _: &WlSeat, _: <WlSeat as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(_: &mut Self, _: &WlRegistry, _: <WlRegistry as Proxy>::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}

#[cfg(test)]
#[path = "wayland_tests.rs"]
mod tests;
