- On X11, a click right after the pointer entered a window, as when a window over it closes, now lands where the pointer is.
  winit 0.30.13 recorded no position on enter, focus-in or first touch, so a move straight back to the window's last recorded spot was dropped and the press landed where the pointer entered; Rune now builds on `bryceeppler/winit` at that release plus the fix.
