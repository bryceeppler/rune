- **`App::centered_on` opens a window centred on another app's window.**
  Each time the window comes on screen from hidden, it is centred on the other window as it shows now, maximized included, or where that window comes back to while it is minimized, and moved only as far as it takes to lie on the display holding most of it.
  A window already on screen stays where it is, and Wayland, which places windows itself, is unchanged.
  On X11 the window's size hints carry the position, so the window manager maps it there instead of placing it afresh.
