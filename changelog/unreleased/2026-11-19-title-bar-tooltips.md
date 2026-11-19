- On Windows, an element that shows a tooltip inside a `.window_drag()` title bar now answers the system's hit test as content, so resting the pointer on it shows its tooltip; the empty band around it still drags the window.
  Windows claimed it as caption, so the pointer never reached the view. `Runtime::caption_hit` reads the answer at a point, and `CaptionHit` is public.
