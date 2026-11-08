- **A secondary press on a text input says which character it is on.** `PointerEvent::text`
  gives `on_secondary_press_local` the byte of the input's text under a right-click, or the
  caret for the Menu key and Shift+F10, so an editor's context menu can act on the word there
  (a spelling menu's suggestions) without moving the caret.
