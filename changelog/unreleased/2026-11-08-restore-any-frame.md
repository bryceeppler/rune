- **`App::restore_geometry` takes any saved frame without panicking.** A frame with no area, a
  huge or negative one, or a size that is not a number opens at least the window's minimum size,
  wholly on a present display; a frame with no area centres on the primary display.
