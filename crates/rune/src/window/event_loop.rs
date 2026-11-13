//! Building and running the event loop every native window shares.
#[cfg(not(target_arch = "wasm32"))]
use std::{cell::RefCell, rc::Rc};
#[cfg(not(target_arch = "wasm32"))]
use winit::application::ApplicationHandler;
#[cfg(not(target_arch = "wasm32"))]
use winit::event::{StartCause, WindowEvent};
#[cfg(not(target_arch = "wasm32"))]
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
#[cfg(not(target_arch = "wasm32"))]
use winit::window::WindowId;

use crate::app::App;
use crate::tooling::Args;
use crate::windows::WindowSpec;
#[cfg(not(target_arch = "wasm32"))]
use super::{WakeEvent, Windows};
use super::Factory;

pub fn run<S: 'static>(app: App<S>, args: Args) -> Result<(), String> {
    let accessory = app.accessory;
    run_configured(Box::new(move |_| vec![WindowSpec::from_app("main", app).into()]), args, accessory)
}

pub(crate) fn run_windows(factory: Factory, args: Args, options: crate::RunOptions) -> Result<(), String> {
    run_configured(factory, args, options.accessory)
}

/// Without a display (no `WAYLAND_DISPLAY` or `DISPLAY`, a refused X11 connection) the event
/// loop can't be built.
fn event_loop_error(error: winit::error::EventLoopError) -> String {
    format!("event loop: {error}")
}

/// Whether this process can reach the window server. Over ssh to a Mac no one is logged in to,
/// AppKit starts but never finishes launching, so the app would wait forever.
#[cfg(target_os = "macos")]
fn window_server_session() -> bool {
    use std::ffi::c_void;
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" { fn CGSessionCopyCurrentDictionary() -> *const c_void; }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" { fn CFRelease(cf: *const c_void); }
    // SAFETY: a Copy-rule CF function; a non-null result is released exactly once.
    unsafe {
        let session = CGSessionCopyCurrentDictionary();
        if session.is_null() { return false; }
        CFRelease(session);
    }
    true
}

#[cfg(target_arch = "wasm32")]
fn run_configured(_: Factory, _: Args, _: bool) -> Result<(), String> {
    Err("native window factories are unsupported on web; use App::run for one canvas".into())
}

#[cfg(not(target_arch = "wasm32"))]
fn run_configured(factory: Factory, args: Args, accessory: bool) -> Result<(), String> {
    let _ = accessory;
    crate::tooling::perf_warn::launch::run_starts();
    crate::platform::ready_signal::run_starts();
    #[cfg(target_os = "macos")]
    if !window_server_session() {
        crate::render::discard_prewarm();
        return Err("event loop: no window server session (no one is logged in to this Mac)".into());
    }
    // While AppKit starts up.
    crate::render::prewarm_gpu();
    crate::render::probe_gpu();
    #[allow(unused_mut)]
    let mut builder = EventLoop::<WakeEvent>::with_user_event();
    #[cfg(target_os = "windows")]
    {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        use crate::platform::win32::packets;
        builder.with_msg_hook(packets::hook(packets::arrive));
    }
    #[cfg(target_os = "macos")]
    if accessory {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        builder
            .with_activation_policy(ActivationPolicy::Accessory)
            .with_default_menu(false)
            .with_activate_ignoring_other_apps(false);
    }
    let event_loop = builder.build().inspect_err(|_| crate::render::discard_prewarm()).map_err(event_loop_error)?;
    event_loop.set_control_flow(ControlFlow::Wait);
    #[cfg(target_os = "macos")]
    crate::platform::app_icon::launched();
    let proxy = event_loop.create_proxy();
    super::quitting::connect(proxy.clone());
    #[cfg(all(unix, not(target_os = "ios"), not(target_os = "android")))]
    {
        let proxy = proxy.clone();
        crate::platform::quit::on_sigterm(move || { let _ = proxy.send_event(WakeEvent::SessionEnd); });
    }
    let windows = Rc::new(RefCell::new(Windows { factory: Some(factory), args, proxy, hosts: Vec::new(), gpu: Rc::default(), quitting: None, last_main: Default::default() }));
    #[cfg(target_os = "windows")]
    {
        let windows = Rc::downgrade(&windows);
        crate::platform::quit::on_session(move |step| match windows.upgrade().as_deref().map(RefCell::try_borrow_mut) {
            Some(Ok(mut windows)) => windows.session(step),
            Some(Err(_)) => super::quitting::session_later(step),
            None => crate::platform::quit::SessionReply::Ends,
        });
        crate::platform::quit::keep_event_window();
    }
    crate::open_url::incoming::prepare();
    let result = event_loop.run_app(&mut Handler(windows));
    // The windows' interaction timing queued its last rollups as they closed.
    crate::tooling::interaction::flush();
    crate::render::discard_prewarm();
    crate::clipboard::shutdown(); // Also cover an event-loop error before `exiting`.
    crate::platform::ready_signal::clear();
    result.map_err(|error| format!("run: {error}"))
}

/// The loop's handler, shared so that what Windows sends inside a window procedure
/// (`quit::on_session`) reaches the windows between events.
#[cfg(not(target_arch = "wasm32"))]
struct Handler(Rc<RefCell<Windows>>);

#[cfg(not(target_arch = "wasm32"))]
impl ApplicationHandler<WakeEvent> for Handler {
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) { self.0.borrow_mut().new_events(event_loop, cause); }
    fn resumed(&mut self, event_loop: &ActiveEventLoop) { self.0.borrow_mut().resumed(event_loop); }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: WakeEvent) { self.0.borrow_mut().user_event(event_loop, event); }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) { self.0.borrow_mut().window_event(event_loop, id, event); }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) { self.0.borrow_mut().about_to_wait(event_loop); }
    fn exiting(&mut self, event_loop: &ActiveEventLoop) { self.0.borrow_mut().exiting(event_loop); }
}

#[test]
fn an_event_loop_that_cant_start_is_an_error_naming_the_event_loop() {
    let error = event_loop_error(winit::error::EventLoopError::RecreationAttempt);
    assert!(error.starts_with("event loop: ") && error.contains("recreated"), "{error}");
}
