//! Where a window is, for saving its place and opening there at the next launch
//! (`App::on_geometry`, `WindowHandle::geometry`, `App::restore_geometry`).
//!
//! Frames are in desktop units, the coordinates the platform places windows in: points on
//! macOS, pixels on Windows and X11. A frame saved on one display layout is moved onto one of the
//! displays present when it is restored (`fit`).

use winit::dpi::{LogicalPosition, LogicalSize, PhysicalPosition, Position};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Fullscreen, Window};

use super::{Host, is_wayland};

/// How a window shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WindowState {
    #[default]
    Normal,
    Maximized,
    Minimized,
    Fullscreen,
}

/// Where a window is and how it shows. A maximized, fullscreen or minimized window reports the
/// frame and size it returns to, so saving the latest geometry and restoring it reopens the
/// window where the person left it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WindowGeometry {
    /// The outer frame, title bar and borders included: x, y, width and height in desktop units
    /// (points on macOS, pixels on Windows and X11). None on Wayland, which keeps window
    /// positions from apps.
    pub frame: Option<[i32; 4]>,
    /// The content size in points.
    pub size: (f32, f32),
    pub state: WindowState,
    /// Pixels per point of the display the window is on.
    pub scale: f64,
    /// The name of that display, where the platform gives one.
    pub display: Option<String>,
}

/// A display's usable area in desktop units, its bounds less the menu bar, Dock or taskbar, and
/// how a window's frame meets it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Screen {
    pub area: [i32; 4],
    /// Desktop units per point on it.
    pub units: f64,
    /// How far an edge may pass the area: Windows' invisible resize border, which counts in a
    /// window's frame.
    pub slack: i32,
    pub primary: bool,
}

impl Screen {
    /// A screen that takes frames in points, as macOS and the headless desktop do.
    pub(crate) fn in_points(area: [i32; 4], primary: bool) -> Self { Screen { area, units: 1.0, slack: 0, primary } }
}

impl WindowGeometry {
    /// This geometry placed on `screens`, its frame saved at `units` desktop units per point:
    /// the screen the frame overlaps most, keeping its place there, else centred on the primary
    /// screen. The frame is rescaled to that screen's units, since the window keeps its size in
    /// points, then moved and shrunk to lie wholly on it, and the content shrinks by what the
    /// frame lost, but never below `min`. A frame with no area is placed from the content size.
    /// The content is always finite and at least `min`; the frame is unchanged without screens.
    pub(crate) fn fitted(&self, screens: &[Screen], units: f64, min: (f32, f32)) -> WindowGeometry {
        let at_least = |v: f32, min: f32| if v.is_finite() { v.max(min) } else { min };
        let content = (at_least(self.size.0, min.0), at_least(self.size.1, min.1));
        let frame = self.frame.filter(|f| f[2] > 0 && f[3] > 0);
        let overlap = |s: &&Screen| frame.map_or(0, |f| overlap(f, s.area));
        let best = screens.iter().max_by_key(overlap).filter(|s| overlap(s) > 0);
        let Some(screen) = best.or_else(|| screens.iter().find(|s| s.primary)).or(screens.first()) else {
            return WindowGeometry { size: content, ..self.clone() };
        };
        let per_point = if screen.units.is_finite() && screen.units > 0.0 { screen.units } else { 1.0 };
        let ratio = if units.is_finite() && units > 0.0 { per_point / units } else { 1.0 };
        let want = |saved: Option<i32>, content: f32| saved.map_or(f64::from(content) * per_point, |v| f64::from(v) * ratio);
        let want = (want(frame.map(|f| f[2]), content.0), want(frame.map(|f| f[3]), content.1));
        let [ax, ay, aw, ah] = screen.area.map(i64::from);
        let slack = i64::from(screen.slack.max(0));
        let span = |want: f64, room: i64, min: f32| (want.round() as i64).min(room).max((f64::from(min) * per_point).ceil() as i64).max(1);
        let (w, h) = (span(want.0, aw + 2 * slack, min.0), span(want.1, ah + 2 * slack, min.1));
        let (x, y) = match (best, frame) {
            (Some(_), Some(f)) => (i64::from(f[0]), i64::from(f[1])),
            _ => (ax + (aw - w) / 2, ay + (ah - h) / 2),
        };
        let place = |v: i64, a: i64, room: i64, len: i64| v.min(a + room + slack - len).max(a - slack);
        let placed = [place(x, ax, aw, w), place(y, ay, ah, h), w, h].map(|v| v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32);
        let fit = |content: f32, want: f64, now: i64, min: f32| {
            let lost = ((want - now as f64).max(0.0) / per_point) as f32;
            (content - lost).min((now as f64 / per_point) as f32).max(min)
        };
        let size = (fit(content.0, want.0, w, min.0), fit(content.1, want.1, h, min.1));
        WindowGeometry { frame: self.frame.map(|_| placed), size, ..self.clone() }
    }
}

/// The area two frames share, in square desktop units.
fn overlap(a: [i32; 4], b: [i32; 4]) -> i64 {
    let span = |a: i32, al: i32, b: i32, bl: i32| (i64::from(a) + i64::from(al)).min(i64::from(b) + i64::from(bl)) - i64::from(a.max(b));
    let (w, h) = (span(a[0], a[2], b[0], b[2]), span(a[1], a[3], b[1], b[3]));
    if w > 0 && h > 0 { w * h } else { 0 }
}

/// Desktop units per point at `scale`.
fn units_per_point(scale: f64) -> f64 { if cfg!(target_os = "macos") { 1.0 } else { scale } }

/// How far Windows' invisible resize border reaches outside a window's visible frame.
#[cfg(not(target_os = "macos"))]
fn slack(scale: f64) -> i32 { if cfg!(target_os = "windows") { (8.0 * scale).round() as i32 } else { 0 } }

/// What a window's host remembers of its geometry: the frame and size it had last while normal,
/// what it reported last, and whether a window event may have changed it since.
#[derive(Default)]
pub(super) struct Watch {
    normal: Option<(Option<[i32; 4]>, (f32, f32))>,
    reported: Option<WindowGeometry>,
    pub(super) dirty: bool,
}

fn state(window: &Window) -> WindowState {
    if window.is_minimized() == Some(true) { WindowState::Minimized }
    else if window.fullscreen().is_some() { WindowState::Fullscreen }
    else if window.is_maximized() { WindowState::Maximized }
    else { WindowState::Normal }
}

fn outer_frame(window: &Window, wayland: bool) -> Option<[i32; 4]> {
    if wayland { return None; }
    #[cfg(target_os = "macos")]
    return crate::platform::macos::window_geometry(window).map(|(frame, ..)| frame.map(|v| v.round() as i32));
    #[cfg(not(target_os = "macos"))]
    {
        let (p, s) = (window.outer_position().ok()?, window.outer_size());
        Some([p.x, p.y, s.width as i32, s.height as i32])
    }
}

/// The displays present now, as `fit` takes them.
fn screens(event_loop: &ActiveEventLoop) -> Vec<Screen> {
    #[cfg(target_os = "macos")]
    { let _ = event_loop; mac_screens() }
    #[cfg(not(target_os = "macos"))]
    {
        let primary = event_loop.primary_monitor();
        event_loop.available_monitors().map(|m| {
            let (p, s) = (m.position(), m.size());
            #[allow(unused_mut)]
            let mut area = [p.x, p.y, s.width as i32, s.height as i32];
            #[cfg(target_os = "windows")]
            if let Some(work) = crate::platform::win32::work_area(&m) { area = work; }
            let scale = m.scale_factor();
            Screen { area, units: units_per_point(scale), slack: slack(scale), primary: primary.as_ref() == Some(&m) }
        }).collect()
    }
}

#[cfg(target_os = "macos")]
fn mac_screens() -> Vec<Screen> {
    use objc2_app_kit::NSScreen;
    let Some(mtm) = objc2_foundation::MainThreadMarker::new() else { return Vec::new() };
    let screens = NSScreen::screens(mtm);
    let Some(top) = screens.iter().next().map(|s| s.frame().size.height) else { return Vec::new() };
    screens.iter().enumerate().map(|(i, s)| {
        let r = s.visibleFrame();
        let area = [r.origin.x, top - r.origin.y - r.size.height, r.size.width, r.size.height].map(|v| v.round() as i32);
        Screen::in_points(area, i == 0)
    }).collect()
}

impl<S: 'static> crate::App<S> {
    /// Runs when the window moves, resizes, changes display or scale, or is maximized,
    /// minimized, made fullscreen or restored, at most once an event-loop turn, with its
    /// [`WindowGeometry`]: save it to reopen the window there
    /// (`App::restore_geometry`). Returns whether it changed state the view reads, as
    /// `AppHandle::update_if` does. Also runs once when the window is created.
    pub fn on_geometry(mut self, f: fn(&mut S, &WindowGeometry, &mut crate::Tx) -> bool) -> Self {
        self.on_geometry = Some(f);
        self
    }

    /// Opens the window where `saved` (from `App::on_geometry` or `WindowHandle::geometry`)
    /// was: its frame moved and shrunk to lie wholly on a display present now, else centred on
    /// the primary display, then maximized or fullscreen as it was. A window saved minimized
    /// opens normal. Wayland restores the size and state; the compositor places the window.
    pub fn restore_geometry(mut self, saved: WindowGeometry) -> Self {
        self.restore_geometry = Some(saved);
        self
    }
}

impl<S: 'static> Host<S> {
    /// The window's geometry now, with the frame and size it had last while normal.
    fn read_geometry(&mut self, wayland: bool) -> Option<WindowGeometry> {
        let window = &self.win.as_ref()?.window;
        let state = state(window);
        let scale = window.scale_factor();
        let size: LogicalSize<f32> = window.inner_size().to_logical(scale);
        if state == WindowState::Normal || self.geometry.normal.is_none() {
            self.geometry.normal = Some((outer_frame(window, wayland), (size.width, size.height)));
        }
        let (frame, size) = self.geometry.normal?;
        Some(WindowGeometry { frame, size, state, scale, display: window.current_monitor().and_then(|m| m.name()) })
    }

    /// Reports a geometry that changed since the last report, at the end of a turn so a drag
    /// reports once a turn rather than for each move: `WindowHandle::geometry` reads it and
    /// `App::on_geometry` runs with it.
    pub(super) fn geometry_turn(&mut self, event_loop: &ActiveEventLoop) {
        if !std::mem::take(&mut self.geometry.dirty) { return; }
        let Some(geometry) = self.read_geometry(is_wayland(event_loop)) else { return };
        if self.geometry.reported.as_ref() == Some(&geometry) { return; }
        self.native_handle.set_geometry(geometry.clone());
        self.geometry.reported = Some(geometry.clone());
        let Some(f) = self.on_geometry else { return };
        if self.rt.update_if(|s, tx| f(s, &geometry, tx)) && let Some(w) = &self.win { w.window.request_redraw(); }
        self.shell(event_loop);
    }

    /// Where `App::restore_geometry` opens the window, fitted to the displays present now:
    /// its position, unless the platform places windows itself, and its content size.
    pub(super) fn restored_frame(&mut self, event_loop: &ActiveEventLoop) -> Option<(Option<Position>, (f32, f32))> {
        let saved = self.restore.as_ref()?;
        let wayland = is_wayland(event_loop);
        let min = self.options.size_limits(self.spec.min_size).0;
        let placed = saved.fitted(&screens(event_loop), units_per_point(saved.scale), min);
        let position = placed.frame.filter(|_| !wayland).map(|[x, y, ..]| if cfg!(target_os = "macos") {
            Position::Logical(LogicalPosition::new(f64::from(x), f64::from(y)))
        } else {
            Position::Physical(PhysicalPosition::new(x, y))
        });
        Some((position, placed.size))
    }

    /// Maximizes or fullscreens a window `App::restore_geometry` opened, once, as it was saved.
    /// A window saved minimized opens normal.
    pub(super) fn restore_state(&mut self) {
        let (Some(saved), Some(w)) = (self.restore.take(), &self.win) else { return };
        match saved.state {
            WindowState::Maximized => w.window.set_maximized(true),
            WindowState::Fullscreen => w.window.set_fullscreen(Some(Fullscreen::Borderless(None))),
            WindowState::Normal | WindowState::Minimized => {}
        }
    }
}

#[cfg(test)]
#[path = "geometry_tests.rs"]
mod tests;
