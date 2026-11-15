//! Where a window maps on X11. A window manager places a window afresh each time it maps, unless
//! its size hints carry a position, and winit writes that position only when it creates the
//! window, so a window moved while hidden writes it again before it shows.

use x11rb::properties::{WmSizeHints, WmSizeHintsSpecification};
use x11rb::protocol::xproto::ConnectionExt as _;

/// Asks the window manager to map `window` with its frame's top-left at `(x, y)`; None when it
/// isn't an X11 window or the display can't be reached.
pub(crate) fn on_map(window: &winit::window::Window, x: i32, y: i32) -> Option<()> {
    let id = super::window_id(window)?;
    let (connection, _) = x11rb::connect(None).ok()?;
    let mut hints = WmSizeHints::get_normal_hints(&connection, id).ok()?.reply().ok()?.unwrap_or_default();
    hints.position = Some((WmSizeHintsSpecification::UserSpecified, x, y));
    hints.set_normal_hints(&connection, id).ok()?;
    connection.get_input_focus().ok()?.reply().ok().map(drop)
}
