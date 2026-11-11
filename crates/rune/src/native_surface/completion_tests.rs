use super::gpu_tests::{clear_pass, draw};
use super::*;
use crate::render::{Gpu, OFFSCREEN_FORMAT, Offscreen};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

/// GPU work that stays in flight a while: each dispatch is one unit.
struct Workload {
    pipeline: wgpu::ComputePipeline,
    bind: wgpu::BindGroup,
}

impl Workload {
    fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("workload"),
            source: wgpu::ShaderSource::Wgsl(
                r#"
            @group(0) @binding(0) var<storage, read_write> result: array<u32>;
            @compute @workgroup_size(64)
            fn main(@builtin(global_invocation_id) id: vec3<u32>) {
                var x = id.x;
                for (var i = 0u; i < 1048576u; i++) {
                    x = (x ^ (x >> 13u)) * 1664525u + 1013904223u;
                }
                result[id.x] = x;
            }
        "#
                .into(),
            ),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("workload"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 65536 * 4,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() }],
        });
        Self { pipeline, bind }
    }

    fn record(&self, encoder: &mut wgpu::CommandEncoder, dispatches: u32) {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind, &[]);
        for _ in 0..dispatches {
            pass.dispatch_workgroups(1024, 1, 1);
        }
    }

    fn submit(&self, device: &wgpu::Device, queue: &wgpu::Queue, dispatches: u32) -> wgpu::SubmissionIndex {
        let mut encoder = device.create_command_encoder(&Default::default());
        self.record(&mut encoder, dispatches);
        queue.submit([encoder.finish()])
    }
}

/// Buffer destruction and queue.present both acquire wgpu-core's device snatch write lock.
/// Exercise that lock without a window: it must remain available while the completion
/// worker services a submission whose GPU workload is deliberately still running.
///
/// Checked by ordering, not wall-clock time: the lock was taken and released, and a poll
/// after it still found the GPU work pending. A worker that held the snatch read lock through
/// a fence wait would block `destroy` until the work completed, and the poll would then fire
/// its callback. An attempt whose workload finished before the probe (a loaded machine
/// delaying this thread) proves nothing either way, so it retries with more work.
#[test]
fn completion_worker_preserves_cpu_gpu_overlap() {
    use std::sync::mpsc::TryRecvError;
    let gpu = Gpu::headless_wgpu();
    let surface = NativeSurface::new();
    surface.push_bytes(synthetic(16, 16, 0), 1).unwrap();
    let mut renderer = SurfaceRenderer::new(&gpu.device, OFFSCREEN_FORMAT);
    let workload = Workload::new(&gpu.device);
    let target = Offscreen::new(&gpu.device, 16, 16);
    let mut outcomes = Vec::new();
    // Each dispatch is one unit of work (a few hundred ms on a desktop GPU); later attempts
    // queue more of them in the same submission rather than lengthening one dispatch.
    for dispatches in [1, 2, 4, 8, 16] {
        renderer.prepare(&gpu.device, &gpu.queue, &[draw(&surface, [0.0, 0.0, 16.0, 16.0])], (16, 16));
        let probe = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("present lock probe"),
            size: 4,
            usage: wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        workload.record(&mut encoder, dispatches);
        {
            let mut pass = clear_pass(&mut encoder, &target.view, wgpu::Color::BLACK, renderer.timestamp_writes());
            renderer.draw(&mut pass, 0);
        }
        let (done, completed) = std::sync::mpsc::channel();
        encoder.on_submitted_work_done(move || {
            let _ = done.send(());
        });
        renderer.submit(&gpu.queue, encoder);
        // The completion thread has taken submit's wakeup once a new wake is accepted: it is
        // polling, and keeps polling while the frame is in flight.
        while !renderer.wake(&gpu.queue) {
            std::thread::yield_now();
        }
        std::thread::sleep(Duration::from_millis(1));
        let outcome = if completed.try_recv().is_ok() {
            "finished before the probe"
        } else {
            probe.destroy();
            gpu.device.poll(wgpu::PollType::Poll).unwrap();
            match completed.try_recv() {
                Err(TryRecvError::Empty) => {
                    completed.recv_timeout(Duration::from_secs(30)).expect("GPU workload completed");
                    "overlapped"
                }
                _ => "finished by the time the write lock returned",
            }
        };
        eprintln!("overlap probe: {dispatches} dispatches, {outcome}");
        if outcome == "overlapped" {
            return;
        }
        outcomes.push(outcome);
    }
    panic!("the snatch write lock never returned while GPU work was pending: {outcomes:?}");
}

fn eventually(done: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + std::time::Duration::from_secs(10);
    while !done() {
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    true
}

/// Whether every holder of `device` has let it go, a renderer's completion worker included.
fn released(device: &wgpu::Device) -> impl Fn() -> bool + use<> {
    let alive = Arc::new(());
    let watched = Arc::downgrade(&alive);
    device.set_device_lost_callback(move |_, _| drop(alive.clone()));
    move || watched.strong_count() == 0
}

/// A completion worker that outlives its renderer can still be inside the GPU driver when the
/// process exits, while the driver's exit handlers tear it down.
#[test]
fn a_dropped_renderer_leaves_no_worker_on_its_device() {
    let Gpu { device, queue, .. } = Gpu::headless_wgpu();
    let device_released = released(&device);
    let renderer = SurfaceRenderer::new(&device, OFFSCREEN_FORMAT);
    let (entered, worker_polling) = std::sync::mpsc::channel();
    let (resume, resumed) = std::sync::mpsc::channel::<()>();
    queue.on_submitted_work_done(move || {
        entered.send(()).unwrap();
        resumed.recv().unwrap();
    });
    assert!(renderer.wake(&queue), "the first wake is queued");
    worker_polling.recv_timeout(std::time::Duration::from_secs(10)).expect("the woken worker polls the device");
    assert!(renderer.wake(&queue), "the second wake is queued while the worker is held in its poll");
    drop(queue);
    resume.send(()).unwrap();
    drop(renderer);
    drop(device);
    assert!(device_released(), "the completion worker outlived its renderer and still holds the device");
}

/// wgpu runs callbacks on the completion thread while it polls, so the last owner of a renderer
/// can be released there.
fn a_renderer_dropped_inside_a_completion_callback_finishes_dropping_with(wake_waiting: bool) {
    let Gpu { device, queue, .. } = Gpu::headless_wgpu();
    let device_released = released(&device);
    let renderer = SurfaceRenderer::new(&device, OFFSCREEN_FORMAT);
    let (entered, callback_running) = std::sync::mpsc::channel();
    let (hand_over, last_owner) = std::sync::mpsc::channel::<SurfaceRenderer>();
    let (dropped, renderer_dropped) = std::sync::mpsc::channel();
    queue.on_submitted_work_done(move || {
        entered.send(()).unwrap();
        drop(last_owner.recv().unwrap());
        dropped.send(()).unwrap();
    });
    assert!(renderer.wake(&queue), "the first wake is queued");
    callback_running.recv_timeout(std::time::Duration::from_secs(10)).expect("the woken worker runs the callback");
    if wake_waiting {
        assert!(renderer.wake(&queue), "a second wake waits while the callback holds the worker");
    }
    hand_over.send(renderer).unwrap();
    renderer_dropped
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("a renderer dropped inside its own completion callback finishes dropping");
    drop((device, queue));
    assert!(eventually(device_released), "the completion worker ended after its renderer was dropped in a callback");
}

#[test]
fn a_renderer_dropped_inside_a_completion_callback_finishes_dropping() {
    a_renderer_dropped_inside_a_completion_callback_finishes_dropping_with(false);
}

#[test]
fn a_renderer_dropped_inside_a_completion_callback_with_a_wake_waiting_finishes_dropping() {
    a_renderer_dropped_inside_a_completion_callback_finishes_dropping_with(true);
}

fn timed_gpu() -> Gpu {
    let gpu = Gpu::headless_wgpu();
    assert!(
        gpu.device.features().contains(wgpu::Features::TIMESTAMP_QUERY),
        "this test needs timestamp queries, which {:?} lacks",
        gpu.adapter.get_info()
    );
    gpu
}

/// A surface showing one frame, and the completions its observer hears.
fn observed() -> (NativeSurface, Receiver<FrameTiming>) {
    let surface = NativeSurface::new();
    surface.push_bytes(synthetic(16, 16, 0), 1).unwrap();
    let (completed, completions) = channel();
    surface.set_observer(Arc::new(move |t| {
        if t.completed_at.is_some() {
            let _ = completed.send(t);
        }
    }));
    (surface, completions)
}

/// An encoder that draws `surface` with the renderer's own GPU timestamps, for `submit`.
fn frame(gpu: &Gpu, renderer: &mut SurfaceRenderer, surface: &NativeSurface, target: &Offscreen) -> wgpu::CommandEncoder {
    renderer.prepare(&gpu.device, &gpu.queue, &[draw(surface, [0.0, 0.0, 16.0, 16.0])], (16, 16));
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    {
        let mut pass = clear_pass(&mut encoder, &target.view, wgpu::Color::BLACK, renderer.timestamp_writes());
        renderer.draw(&mut pass, 0);
    }
    encoder
}

/// A wgpu callback that keeps whichever thread runs it until the test lets it go.
struct Held {
    entered: Receiver<()>,
    release: Sender<()>,
}

impl Held {
    fn new() -> (Self, impl FnOnce() + Send + 'static) {
        let (entering, entered) = channel();
        let (release, released) = channel();
        let callback = move || {
            let _ = entering.send(());
            let _ = released.recv();
        };
        (Self { entered, release }, callback)
    }

    fn entered(&self, what: &str) {
        self.entered.recv_timeout(Duration::from_secs(10)).expect(what);
    }

    fn release(&self) {
        let _ = self.release.send(());
    }
}

/// Keeps the renderer's completion thread inside a device poll, so another thread runs the
/// callbacks of work that finishes meanwhile.
fn hold_completion_thread(gpu: &Gpu, renderer: &SurfaceRenderer) -> Held {
    let (held, callback) = Held::new();
    gpu.queue.on_submitted_work_done(callback);
    assert!(renderer.wake(&gpu.queue), "the completion thread is idle");
    held.entered("the completion thread polls the device");
    held
}

fn poll_on_another_thread(gpu: &Gpu) -> std::thread::JoinHandle<Result<wgpu::PollStatus, wgpu::PollError>> {
    let device = gpu.device.clone();
    std::thread::spawn(move || device.poll(wgpu::PollType::wait_indefinitely()))
}

/// The frame's completion callback runs only after its renderer's drop has returned, on a thread
/// that polls once, so its timestamp readback has nobody left to poll for it.
#[test]
fn a_frame_in_flight_when_its_renderer_drops_still_reports_its_completion() {
    let gpu = timed_gpu();
    let (surface, completions) = observed();
    let mut renderer = SurfaceRenderer::new(&gpu.device, OFFSCREEN_FORMAT);
    let target = Offscreen::new(&gpu.device, 16, 16);
    let worker = hold_completion_thread(&gpu, &renderer);
    let encoder = frame(&gpu, &mut renderer, &surface, &target);
    let (finishing, callback) = Held::new();
    encoder.on_submitted_work_done(callback);
    renderer.submit(&gpu.queue, encoder);
    let poller = poll_on_another_thread(&gpu);
    finishing.entered("another thread runs the frame's completion callbacks");
    worker.release();
    drop(renderer);
    finishing.release();
    poller.join().unwrap().expect("the frame's GPU work completes");
    completions
        .recv_timeout(Duration::from_secs(10))
        .expect("the frame in flight reports its completion with no other poll of the device");
}

/// A frame's timestamp callback submits the readback of its GPU times, and that submission runs
/// the callbacks of work already done, one of which drops the renderer's last owner.
#[test]
fn a_renderer_dropped_inside_its_own_timestamp_readback_submission_finishes_dropping() {
    let gpu = timed_gpu();
    let (surface, completions) = observed();
    let mut renderer = SurfaceRenderer::new(&gpu.device, OFFSCREEN_FORMAT);
    let target = Offscreen::new(&gpu.device, 16, 16);
    let worker = hold_completion_thread(&gpu, &renderer);
    let encoder = frame(&gpu, &mut renderer, &surface, &target);
    let (hand_over, last_owner) = channel::<SurfaceRenderer>();
    let (dropped, renderer_dropped) = channel();
    let release_worker = worker.release.clone();
    let queue = gpu.queue.clone();
    encoder.on_submitted_work_done(move || {
        queue.on_submitted_work_done(move || {
            let _ = release_worker.send(());
            drop(last_owner.recv());
            let _ = dropped.send(());
        });
    });
    renderer.submit(&gpu.queue, encoder);
    hand_over.send(renderer).unwrap();
    let poller = poll_on_another_thread(&gpu);
    renderer_dropped
        .recv_timeout(Duration::from_secs(10))
        .expect("a renderer dropped inside its own timestamp readback's submission finishes dropping");
    completions.recv_timeout(Duration::from_secs(10)).expect("the frame reports its completion");
    poller.join().unwrap().expect("the frame's GPU work completes");
}

/// Keeps GPU work queued on the device, without a gap, until dropped.
struct Busy {
    running: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Busy {
    fn start(gpu: &Gpu) -> Self {
        let workload = Workload::new(&gpu.device);
        let (device, queue) = (gpu.device.clone(), gpu.queue.clone());
        let mut queued: std::collections::VecDeque<_> = (0..3).map(|_| workload.submit(&device, &queue, 4)).collect();
        let running = Arc::new(AtomicBool::new(true));
        let keep_running = running.clone();
        let thread = std::thread::spawn(move || {
            while keep_running.load(Ordering::SeqCst) {
                queued.push_back(workload.submit(&device, &queue, 4));
                let oldest = queued.pop_front().expect("queued work");
                device
                    .poll(wgpu::PollType::Wait { submission_index: Some(oldest), timeout: None })
                    .expect("queued work completes");
            }
        });
        Self { running, thread: Some(thread) }
    }
}

impl Drop for Busy {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Another renderer on the same device keeps its queue busy, so the queue is never empty.
#[test]
fn a_renderer_drops_while_other_work_keeps_its_queue_busy() {
    let gpu = Gpu::headless_wgpu();
    let (surface, completions) = observed();
    let mut renderer = SurfaceRenderer::new(&gpu.device, OFFSCREEN_FORMAT);
    let target = Offscreen::new(&gpu.device, 16, 16);
    let busy = Busy::start(&gpu);
    let encoder = frame(&gpu, &mut renderer, &surface, &target);
    renderer.submit(&gpu.queue, encoder);
    while !renderer.wake(&gpu.queue) {
        std::thread::yield_now();
    }
    let (dropped, renderer_dropped) = channel();
    let dropping = std::thread::spawn(move || {
        drop(renderer);
        let _ = dropped.send(());
    });
    renderer_dropped
        .recv_timeout(Duration::from_secs(10))
        .expect("a renderer drops while other work keeps its queue busy");
    dropping.join().unwrap();
    completions.recv_timeout(Duration::from_secs(10)).expect("its frame reports its completion");
    drop(busy);
}

/// An app's frame observer can wait for a lock that the app's thread holds while it drops the
/// renderer, as when the renderer lives in state that the observer also updates.
#[test]
fn a_renderer_drops_while_its_frame_observer_waits_for_the_dropping_thread() {
    let gpu = Gpu::headless_wgpu();
    let surface = NativeSurface::new();
    surface.push_bytes(synthetic(16, 16, 0), 1).unwrap();
    let app = Arc::new(Mutex::new(()));
    let (observing, observer_waiting) = channel();
    surface.set_observer({
        let app = app.clone();
        Arc::new(move |t| {
            if t.completed_at.is_some() {
                let _ = observing.send(());
                drop(app.lock());
            }
        })
    });
    let mut renderer = SurfaceRenderer::new(&gpu.device, OFFSCREEN_FORMAT);
    let target = Offscreen::new(&gpu.device, 16, 16);
    let encoder = frame(&gpu, &mut renderer, &surface, &target);
    let queue = gpu.queue.clone();
    let (dropped, renderer_dropped) = channel();
    let app_thread = std::thread::spawn(move || {
        let state = app.lock().unwrap();
        renderer.submit(&queue, encoder);
        observer_waiting.recv_timeout(Duration::from_secs(10)).expect("the frame observer hears the completion");
        drop(renderer);
        let _ = dropped.send(());
        drop(state);
    });
    renderer_dropped
        .recv_timeout(Duration::from_secs(10))
        .expect("a renderer drops while its frame observer waits for the thread dropping it");
    app_thread.join().unwrap();
}

