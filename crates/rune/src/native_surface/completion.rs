#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

use super::{FrameObserver, FrameTiming, NativeSurface};
use std::cell::Cell;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

thread_local! {
    static ON_A_COMPLETION_THREAD: Cell<bool> = const { Cell::new(false) };
}

/// A nonblocking poll releases wgpu's snatch lock before this wait. A fence wait would keep that
/// read lock and serialize `queue.present`'s write lock behind GPU work.
const POLL_INTERVAL: Duration = Duration::from_millis(1);

/// Polls the device while the renderer's frames are in flight, since wgpu runs their callbacks
/// only on a thread that polls and the last frame must complete after the window goes idle.
pub(super) struct CompletionThread(Arc<Completions>);

pub(super) struct Completions {
    /// The thread reaches the device only through this, and the renderer's drop empties it: a
    /// thread left holding the device can be the one to tear it down, inside the GPU driver
    /// while the process exits and the driver's exit handlers unload it.
    device: Mutex<Option<wgpu::Device>>,
    state: Mutex<State>,
    changed: Condvar,
}

struct State {
    renderer: Renderer,
    /// Submissions whose completion callbacks have neither run nor been dropped.
    in_flight: usize,
    /// A submission arrived since the thread last began a poll.
    woken: bool,
    /// Completions recorded on any thread, for the completion thread to tell observers outside
    /// a poll, since a renderer's drop waits for a poll in progress.
    reports: Vec<(FrameObserver, FrameTiming)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Renderer {
    /// Frames read their GPU times back, and the thread polls while any is in flight.
    Open,
    /// Dropped on a completion thread, which waits for none: this thread drains the device.
    Dropped,
    /// Nothing polls for the renderer again. Frames still in flight report without GPU times.
    Drained,
}

/// One submission's frames, in flight until their callback reports them or wgpu drops it.
pub(super) struct InFlight(Arc<Completions>);

enum Step {
    Tell(Vec<(FrameObserver, FrameTiming)>),
    Poll,
    Drain,
}

impl CompletionThread {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn spawn(device: &wgpu::Device) -> Self {
        let completions = Arc::new(Completions {
            device: Mutex::new(Some(device.clone())),
            state: Mutex::new(State { renderer: Renderer::Open, in_flight: 0, woken: false, reports: Vec::new() }),
            changed: Condvar::new(),
        });
        let thread = completions.clone();
        std::thread::Builder::new()
            .name("rune-video-completion".into())
            .spawn(move || thread.run())
            .expect("video completion thread");
        Self(completions)
    }

    pub(super) fn completions(&self) -> Arc<Completions> {
        self.0.clone()
    }

    /// Has the thread poll until `queue`'s work so far is done; false when a wake already waits.
    #[cfg(test)]
    pub(super) fn wake(&self, queue: &wgpu::Queue) -> bool {
        let (marker, woke) = self.0.start();
        queue.on_submitted_work_done(move || drop(marker));
        woke
    }
}

impl Drop for CompletionThread {
    fn drop(&mut self) {
        if ON_A_COMPLETION_THREAD.get() {
            // That thread may be the one inside the poll a drain would wait for.
            self.0.set(Renderer::Dropped);
        } else {
            self.0.set(Renderer::Drained);
            self.0.drain();
        }
    }
}

impl Completions {
    /// Counts a submission in flight and wakes the thread to poll for it.
    pub(super) fn in_flight(self: &Arc<Self>) -> InFlight {
        self.start().0
    }

    pub(super) fn renderer_open(&self) -> bool {
        self.state().renderer == Renderer::Open
    }

    /// A renderer dropped while a frame's readback was being submitted polls for it no more,
    /// and wgpu runs the readback's callback only on a thread that polls.
    pub(super) fn wait_if_dropped(&self, device: &wgpu::Device, readback: wgpu::SubmissionIndex) {
        if !self.renderer_open() {
            device.poll(wgpu::PollType::Wait { submission_index: Some(readback), timeout: None }).ok();
        }
    }

    fn start(self: &Arc<Self>) -> (InFlight, bool) {
        let mut state = self.state();
        state.in_flight += 1;
        let woke = !std::mem::replace(&mut state.woken, true);
        drop(state);
        self.changed.notify_one();
        (InFlight(self.clone()), woke)
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn set(&self, renderer: Renderer) {
        self.state().renderer = renderer;
        self.changed.notify_one();
    }

    fn run(self: Arc<Self>) {
        ON_A_COMPLETION_THREAD.set(true);
        loop {
            let step = {
                let mut state = self.state();
                loop {
                    if !state.reports.is_empty() {
                        break Step::Tell(std::mem::take(&mut state.reports));
                    }
                    let renderer = state.renderer;
                    match renderer {
                        Renderer::Dropped => break Step::Drain,
                        Renderer::Drained if state.in_flight == 0 => return,
                        Renderer::Open if std::mem::take(&mut state.woken) => break Step::Poll,
                        Renderer::Open if state.in_flight > 0 => {
                            let (next, waited) =
                                self.changed.wait_timeout(state, POLL_INTERVAL).unwrap_or_else(PoisonError::into_inner);
                            state = next;
                            if waited.timed_out() {
                                break Step::Poll;
                            }
                        }
                        _ => state = self.changed.wait(state).unwrap_or_else(PoisonError::into_inner),
                    }
                }
            };
            match step {
                Step::Tell(reports) => {
                    for (observer, timing) in reports {
                        observer(timing);
                    }
                }
                Step::Poll => self.poll(),
                Step::Drain => {
                    self.set(Renderer::Drained);
                    self.drain();
                }
            }
        }
    }

    fn poll(&self) {
        if let Some(device) = &*self.device.lock().unwrap_or_else(PoisonError::into_inner) {
            device.poll(wgpu::PollType::Poll).ok();
        }
    }

    /// Takes the device back, waiting for a poll in progress, then, with frames in flight, waits
    /// for the work submitted so far: every frame and readback already submitted, and none later.
    fn drain(&self) {
        let device = self.device.lock().unwrap_or_else(PoisonError::into_inner).take();
        if let Some(device) = device.filter(|_| self.state().in_flight > 0) {
            device.poll(wgpu::PollType::wait_indefinitely()).ok();
        }
    }
}

impl InFlight {
    pub(super) fn report(self, frames: impl IntoIterator<Item = (NativeSurface, FrameTiming)>) {
        let reports: Vec<_> =
            frames.into_iter().filter_map(|(surface, timing)| surface.record(&timing).map(|observer| (observer, timing))).collect();
        self.0.state().reports.extend(reports);
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        self.0.state().in_flight -= 1;
        self.0.changed.notify_one();
    }
}
