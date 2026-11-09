//! Bringing a window forward for the person's own action, with that action's time. winit's
//! `focus_window` asks as an application with no time, which a window manager such as Openbox
//! answers for a window on another desktop by only marking it urgent. A request marked as the
//! person's (source 2 in EWMH) with the key press's time moves the window to the desktop they
//! are on and focuses it there.

use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{AtomEnum, ClientMessageEvent, ConnectionExt as _, EventMask};

const EVERY_DESKTOP: u32 = u32::MAX;
const FROM_THE_PERSON: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ask {
    MoveTo(u32),
    Activate(u32),
}

/// The time a startup id carries in its `_TIME<ms>` suffix, as the startup-notification
/// specification spells it; Rune's X11 shortcuts send one with each key press.
pub(crate) fn time_of(token: &str) -> Option<u32> {
    token.rsplit_once("_TIME").and_then(|(_, time)| time.parse().ok())
}

/// What to ask the window manager for: the window joins the desktop shown now unless it is on
/// every desktop already, then it takes focus at `time`.
pub(crate) fn asks(current: Option<u32>, window: Option<u32>, time: u32) -> Vec<Ask> {
    let move_to = current.filter(|current| window.is_some_and(|window| window != *current && window != EVERY_DESKTOP));
    move_to.map(Ask::MoveTo).into_iter().chain([Ask::Activate(time)]).collect()
}

/// Brings `window` to the current desktop and focuses it; false when it isn't an X11 window or
/// the display can't be reached, so the caller falls back to winit's own focus.
pub(crate) fn bring_here(window: &winit::window::Window, time: u32) -> bool {
    let id = match window.window_handle().ok().map(|handle| handle.as_raw()) {
        Some(RawWindowHandle::Xlib(w)) => match u32::try_from(w.window) { Ok(id) => id, Err(_) => return false },
        Some(RawWindowHandle::Xcb(w)) => w.window.get(),
        _ => return false,
    };
    let sent = send(id, time).is_some();
    eprintln!("DEBUG bring_here {time} sent {sent}");
    sent
}

fn send(id: u32, time: u32) -> Option<()> {
    let (connection, screen) = x11rb::connect(None).ok()?;
    let root = connection.setup().roots.get(screen)?.root;
    let atom = |name: &str| connection.intern_atom(false, name.as_bytes()).ok()?.reply().ok().map(|reply| reply.atom);
    let (current_desktop, wm_desktop, active) = (atom("_NET_CURRENT_DESKTOP")?, atom("_NET_WM_DESKTOP")?, atom("_NET_ACTIVE_WINDOW")?);
    let cardinal = |of: u32, property: u32| connection.get_property(false, of, property, AtomEnum::CARDINAL, 0, 1).ok()?
        .reply().ok()?.value32()?.next();
    let mask = EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY;
    let planned = asks(cardinal(root, current_desktop), cardinal(id, wm_desktop), time);
    eprintln!("DEBUG window {id:#x} asks {planned:?}");
    for ask in planned {
        let (kind, data) = match ask {
            Ask::MoveTo(desktop) => (wm_desktop, [desktop, FROM_THE_PERSON, 0, 0, 0]),
            Ask::Activate(time) => (active, [FROM_THE_PERSON, time, 0, 0, 0]),
        };
        connection.send_event(false, root, mask, ClientMessageEvent::new(32, id, kind, data)).ok()?;
    }
    connection.get_input_focus().ok()?.reply().ok().map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_startup_id_gives_the_time_it_carries() {
        assert_eq!(time_of("_TIME4242"), Some(4242));
        assert_eq!(time_of("xdg-launch/host-123-0-app_TIME98765"), Some(98765));
        assert_eq!(time_of("wayland-token"), None);
        assert_eq!(time_of("_TIMEsoon"), None);
    }

    #[test]
    fn a_window_on_another_desktop_joins_this_one_before_it_takes_focus() {
        assert_eq!(asks(Some(1), Some(0), 9), [Ask::MoveTo(1), Ask::Activate(9)]);
        assert_eq!(asks(Some(1), Some(1), 9), [Ask::Activate(9)]);
        assert_eq!(asks(Some(1), Some(EVERY_DESKTOP), 9), [Ask::Activate(9)]);
        assert_eq!(asks(None, Some(0), 9), [Ask::Activate(9)]);
        assert_eq!(asks(Some(1), None, 9), [Ask::Activate(9)]);
    }
}
