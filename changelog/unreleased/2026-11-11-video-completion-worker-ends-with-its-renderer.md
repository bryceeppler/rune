- **A dropped `native_surface::SurfaceRenderer` takes its device back from its completion thread.**
  Dropping the renderer, or the `Renderer` that drew a native surface, waits at most for a device poll the thread is already inside and, with frames in flight, for the GPU work submitted so far, so every frame in flight completes and the thread never polls or holds the device afterwards.
  A renderer dropped on a completion thread, from a frame callback or observer, waits for nothing instead, and its completion thread lets the device go once it finishes the poll it is in.
  A device error during that wait no longer panics out of the drop.
  Like `Device::poll`, that wait runs any wgpu callbacks that are ready on the dropping thread.
  Frame observers hear completions on the renderer's completion thread and never inside a device poll, so an observer may wait for the thread that drops the renderer, or drop the renderer's last owner itself.
  A frame whose completion callback runs after its renderer dropped reports its completion without GPU times.
  Before, a thread that polled after its device's `wgpu::Queue` was dropped kept polling and held the device until the process exited, where it could crash the process inside the GPU driver's exit handlers.
