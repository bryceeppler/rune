//! A window that draws its own title bar (`Chrome::caption_buttons`). The client area takes in
//! the title bar and the top resize border, and the hit test answers from the view's drag
//! regions and caption buttons (`crate::caption`), so the system still moves the window, snaps
//! it, maximizes it on a double click, opens the system menu and offers Snap Layouts over the
//! maximize button. Over a caption button the view hears the pointer as ordinary client input,
//! so the button lights up and presses it, and its handler acts.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
use windows_sys::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow, GetSystemMetricsForDpi};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT, TrackMouseEvent};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GA_ROOT, GWL_EXSTYLE, GWL_STYLE, GetAncestor, GetCursorPos, GetWindowLongW, HTCAPTION, HTCLIENT, HTCLOSE, HTMAXBUTTON,
    HTMINBUTTON, HTTOP, IsZoomed, MINMAXINFO, NCCALCSIZE_PARAMS, SM_CXPADDEDBORDER, SM_CYFRAME, SWP_FRAMECHANGED,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SendMessageW, SetWindowPos, WM_GETMINMAXINFO, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCCALCSIZE, WM_NCDESTROY, WM_NCHITTEST, WM_NCLBUTTONDBLCLK,
    WM_NCLBUTTONDOWN, WM_NCLBUTTONUP, WM_NCMOUSELEAVE, WM_NCMOUSEMOVE, WindowFromPoint,
};

use crate::caption::{CaptionHit, CaptionMap};
use crate::{Caption, CaptionButton};

const SUBCLASS: usize = 0x5255_4e43;
/// `MK_LBUTTON` and `WM_MOUSELEAVE`, in features of windows-sys this crate leaves off.
const MK_LBUTTON: WPARAM = 0x0001;
const WM_MOUSELEAVE: u32 = 0x02A3;

#[derive(Default)]
struct State {
    map: RefCell<CaptionMap>,
    /// Physical pixels per point.
    scale: Cell<f32>,
    /// The pointer is on a caption button, which the view hears as client input.
    on_button: Cell<bool>,
}

/// The title bar a window draws itself. Dropping it leaves the subclass to the window's
/// `WM_NCDESTROY`.
pub(crate) struct CaptionFrame {
    hwnd: HWND,
    state: Rc<State>,
}

impl CaptionFrame {
    /// Takes the title bar into `window`'s client area, keeping its content size.
    pub(crate) fn install(window: &winit::window::Window) -> Option<Self> {
        let hwnd = super::hwnd(window)?;
        let state = Rc::new(State { scale: Cell::new(window.scale_factor() as f32), ..State::default() });
        let data = Rc::into_raw(state.clone());
        // SAFETY: `data` stays alive until WM_NCDESTROY, where the subclass that reads it is removed.
        if unsafe { SetWindowSubclass(hwnd, Some(subclass), SUBCLASS, data as usize) } == 0 {
            // SAFETY: never handed to Windows.
            drop(unsafe { Rc::from_raw(data) });
            return None;
        }
        let content = window.inner_size();
        // SAFETY: a style-preserving frame refresh on a live window owned by this thread.
        unsafe { SetWindowPos(hwnd, std::ptr::null_mut(), 0, 0, 0, 0, SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE) };
        let _ = window.request_inner_size(winit::dpi::PhysicalSize::new(content.width, content.height.saturating_sub(top_frame(hwnd) as u32)));
        Some(Self { hwnd, state })
    }

    /// The band's metrics and the window's state for `Cx::caption`.
    pub(crate) fn caption(&self, window: &winit::window::Window) -> Caption {
        let scale = window.scale_factor() as f32;
        // SAFETY: reads the state of a live window.
        let maximized = unsafe { IsZoomed(self.hwnd) } != 0;
        Caption { height: top_frame(self.hwnd) as f32 / scale, button_width: Caption::BUTTON_WIDTH, maximized, active: window.has_focus() }
    }

    /// Points winit counts as frame above the content: ask it for this much less height.
    pub(crate) fn inset(&self, window: &winit::window::Window) -> f32 {
        top_frame(self.hwnd) as f32 / window.scale_factor() as f32
    }

    /// The last frame's drag regions and caption buttons.
    pub(crate) fn set_map(&self, map: CaptionMap, scale: f32) {
        self.state.scale.set(scale);
        if *self.state.map.borrow() != map { *self.state.map.borrow_mut() = map; }
    }
}

/// Physical pixels of title bar and top border a titled window has above its client area.
fn top_frame(hwnd: HWND) -> i32 {
    let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    // SAFETY: reads the styles and scale of a live window into a local rect.
    unsafe {
        let (style, ex) = (GetWindowLongW(hwnd, GWL_STYLE) as u32, GetWindowLongW(hwnd, GWL_EXSTYLE) as u32);
        AdjustWindowRectExForDpi(&mut rect, style, 0, ex, GetDpiForWindow(hwnd));
    }
    -rect.top
}

/// The resize border's thickness, which a maximized window hangs off the screen.
fn resize_border(hwnd: HWND) -> i32 {
    // SAFETY: plain metric reads.
    unsafe {
        let dpi = GetDpiForWindow(hwnd);
        GetSystemMetricsForDpi(SM_CYFRAME, dpi) + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi)
    }
}

fn maximized(hwnd: HWND) -> bool {
    // SAFETY: reads the state of a live window.
    unsafe { IsZoomed(hwnd) != 0 }
}

fn point(lparam: LPARAM) -> POINT {
    POINT { x: lparam as i16 as i32, y: (lparam >> 16) as i16 as i32 }
}

fn packed(p: POINT) -> LPARAM {
    ((p.x as u16 as u32) | ((p.y as u16 as u32) << 16)) as LPARAM
}

fn client(hwnd: HWND, screen: POINT) -> POINT {
    let mut p = screen;
    // SAFETY: converts a point for a live window.
    unsafe { ScreenToClient(hwnd, &mut p) };
    p
}

fn button(hit: WPARAM) -> bool {
    matches!(hit as u32, HTMINBUTTON | HTMAXBUTTON | HTCLOSE)
}

/// What the client point `p` is, past the borders `DefWindowProc` already answered for.
fn client_hit(hwnd: HWND, state: &State, p: POINT) -> u32 {
    if !maximized(hwnd) && p.y < resize_border(hwnd) { return HTTOP; }
    let scale = state.scale.get();
    match crate::caption::hit(&state.map.borrow(), p.x as f32 / scale, p.y as f32 / scale) {
        CaptionHit::Client => HTCLIENT,
        CaptionHit::Drag => HTCAPTION,
        CaptionHit::Button(CaptionButton::Minimize) => HTMINBUTTON,
        CaptionHit::Button(CaptionButton::Maximize) => HTMAXBUTTON,
        CaptionHit::Button(CaptionButton::Close) => HTCLOSE,
    }
}

/// The hit test where the pointer is now, if it is over this window.
fn pointer_hit(hwnd: HWND) -> Option<u32> {
    let mut p = POINT { x: 0, y: 0 };
    // SAFETY: reads the pointer and asks a live window owned by this thread.
    unsafe {
        if GetCursorPos(&mut p) == 0 || GetAncestor(WindowFromPoint(p), GA_ROOT) != hwnd { return None; }
        Some(SendMessageW(hwnd, WM_NCHITTEST, 0, packed(p)) as u32)
    }
}

fn track(hwnd: HWND, flags: u32) {
    let mut request = TRACKMOUSEEVENT { cbSize: size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: flags, hwndTrack: hwnd, dwHoverTime: 0 };
    // SAFETY: `request` is a valid TRACKMOUSEEVENT for a live window.
    unsafe { TrackMouseEvent(&mut request) };
}

/// Hands winit a client mouse message at the screen point in `lparam`.
fn forward(hwnd: HWND, message: u32, keys: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: forwards to the next window procedure of a live window.
    unsafe { DefSubclassProc(hwnd, message, keys, packed(client(hwnd, point(lparam)))) }
}

fn leave(hwnd: HWND) {
    // SAFETY: as above.
    unsafe { DefSubclassProc(hwnd, WM_MOUSELEAVE, 0, 0) };
}

unsafe extern "system" fn subclass(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM, _: usize, data: usize) -> LRESULT {
    // SAFETY: `data` is the live `State` until WM_NCDESTROY.
    let state = unsafe { &*(data as *const State) };
    match message {
        WM_NCCALCSIZE if wparam != 0 => {
            // SAFETY: with wparam set, lparam points to the NCCALCSIZE_PARAMS Windows owns.
            let params = unsafe { &mut *(lparam as *mut NCCALCSIZE_PARAMS) };
            let top = params.rgrc[0].top;
            // SAFETY: lets winit and the system lay out the frame first.
            let result = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
            params.rgrc[0].top = top + if maximized(hwnd) { resize_border(hwnd) } else { 0 };
            return result;
        }
        WM_NCHITTEST => {
            // SAFETY: the system's answer for the borders.
            let hit = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
            return if hit == HTCLIENT as LRESULT { client_hit(hwnd, state, client(hwnd, point(lparam))) as LRESULT } else { hit };
        }
        WM_NCMOUSEMOVE if button(wparam) => {
            if !state.on_button.replace(true) { track(hwnd, TME_LEAVE | TME_NONCLIENT); }
            return forward(hwnd, WM_MOUSEMOVE, 0, lparam);
        }
        WM_NCMOUSEMOVE if state.on_button.replace(false) => leave(hwnd),
        WM_NCMOUSELEAVE if state.on_button.get() => match pointer_hit(hwnd) {
            Some(hit) if button(hit as WPARAM) => track(hwnd, TME_LEAVE | TME_NONCLIENT),
            Some(HTCLIENT) => {
                state.on_button.set(false);
                track(hwnd, TME_LEAVE);
            }
            _ => {
                state.on_button.set(false);
                leave(hwnd);
            }
        },
        WM_MOUSELEAVE if pointer_hit(hwnd).is_some_and(|hit| button(hit as WPARAM)) => return 0,
        WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK if button(wparam) => return forward(hwnd, WM_LBUTTONDOWN, MK_LBUTTON, lparam),
        WM_NCLBUTTONUP if button(wparam) => return forward(hwnd, WM_LBUTTONUP, 0, lparam),
        WM_GETMINMAXINFO => {
            // SAFETY: winit fills in the limits for the content size it was given first.
            let result = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
            // SAFETY: lparam points to the MINMAXINFO Windows owns.
            let info = unsafe { &mut *(lparam as *mut MINMAXINFO) };
            info.ptMinTrackSize.y = (info.ptMinTrackSize.y - top_frame(hwnd)).max(0);
            return result;
        }
        WM_NCDESTROY => unsafe {
            RemoveWindowSubclass(hwnd, Some(subclass), SUBCLASS);
            drop(Rc::from_raw(data as *const State));
        },
        _ => {}
    }
    // SAFETY: forwards to the next window procedure.
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}
