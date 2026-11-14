- **Windows windows can draw their own title bar.**
  `Chrome::caption_buttons` takes the title bar into the content on Windows, keeping the resize borders, shadow and rounded corners, and a maximized window keeps all of its content on screen.
  `.window_drag()` regions answer Windows' hit test as the title bar, so dragging, Snap, double-click to maximize and the system menu stay the system's.
  `El::caption_button` marks the minimize, maximize and close buttons the app draws, so hovering maximize offers Snap Layouts while the buttons still light up and press as ordinary elements.
  rune-kit's `CaptionButtons` draws all three as Windows 11 does, `Cx::caption` gives the band's height, the buttons' width and whether the window is maximized and active, and `Tx::toggle_maximized` maximizes or restores.
  `Chrome` gains the `caption_buttons` field, so a `Chrome { .. }` literal names it or ends in `..Chrome::DEFAULT`.
