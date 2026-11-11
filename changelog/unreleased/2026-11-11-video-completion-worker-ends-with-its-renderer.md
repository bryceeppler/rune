- **A `native_surface::SurfaceRenderer`'s completion thread ends with it.**
  Dropping the renderer, or the `Renderer` that drew a native surface, now waits for the GPU work the thread was polling and for the thread itself.
  Before, a thread that polled after its device's `wgpu::Queue` was dropped kept polling and held the device until the process exited, where it could crash the process inside the GPU driver's exit handlers.
