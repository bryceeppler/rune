- **A `native_surface::SurfaceRenderer`'s completion thread ends with it.**
  Dropping the renderer, or the `Renderer` that drew a native surface, waits for the thread to finish the frames still in flight and exit, so the thread never outlives it.
  A frame observer may drop the renderer's last owner; on the completion thread, the drop lets the thread finish on its own rather than wait for itself.
  A frame whose GPU work finishes after its renderer is dropped still reports its completion, without GPU times.
  Before, a thread that polled after its device's `wgpu::Queue` was dropped kept polling and held the device until the process exited, where it could crash the process inside the GPU driver's exit handlers.
