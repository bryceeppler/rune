- **A desktop's word on a global shortcut, and a shortcut that reaches another desktop.** On
  Wayland the GlobalShortcuts portal picks the final trigger itself and the person can change or
  remove it later in the system's settings; `AppHandle::shortcut_status(id)` now reads the
  trigger the portal described and whether it still delivers the shortcut, from BindShortcuts,
  ShortcutsChanged and a closed session, and `AppHandle::watch_shortcuts` hears each change.
  On X11 a shortcut's `Tx::activation_token()` now carries the key press's time as a
  `_TIME<ms>` startup id, and `AppHandle::activate` with such a token moves the window to the
  desktop shown now and focuses it as the person's own request, where `focus_window` left a
  window on another Openbox desktop only marked urgent.
