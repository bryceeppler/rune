pub(crate) mod activate;
pub(crate) mod overlay;
pub(crate) mod placed;
pub(crate) mod pointer;
pub(crate) mod xwayland;

/// The X11 id of `window`; None for a window on another backend.
pub(crate) fn window_id(window: &winit::window::Window) -> Option<u32> {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Xlib(w) => u32::try_from(w.window).ok(),
        RawWindowHandle::Xcb(w) => Some(w.window.get()),
        _ => None,
    }
}
