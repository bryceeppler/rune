//! winit applies an X11 input method toggle only once it next handles an X event, and until then
//! the old input context takes the keys typed after Rune reset it. A message from Rune's own
//! connection to the window is that next event, so the reset lands before the user's next key.

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use x11rb::connection::Connection as _;
use x11rb::protocol::xproto::{ClientMessageEvent, ConnectionExt as _, EventMask};
use x11rb::rust_connection::RustConnection;
use winit::window::Window;

/// One window's wake, made on first use and kept with the window.
pub(crate) struct Wake {
    connection: RustConnection,
    window: u32,
    message: u32,
}

impl Wake {
    /// None off X11.
    pub(crate) fn new(window: &Window) -> Option<Wake> {
        let window = match window.window_handle().ok()?.as_raw() {
            RawWindowHandle::Xlib(handle) => u32::try_from(handle.window).ok()?,
            RawWindowHandle::Xcb(handle) => handle.window.get(),
            _ => return None,
        };
        let (connection, _) = x11rb::connect(None).ok()?;
        let message = connection.intern_atom(false, b"_RUNE_IME_RESET").ok()?.reply().ok()?.atom;
        Some(Wake { connection, window, message })
    }

    /// winit handles the message, then the toggles waiting for it.
    pub(crate) fn send(&self) {
        let event = ClientMessageEvent::new(32, self.window, self.message, [0u32; 5]);
        let _ = self.connection.send_event(false, self.window, EventMask::NO_EVENT, event);
        let _ = self.connection.flush();
    }
}
