- **A right-click outside an open menu opens the menu of what is under it.** A secondary press
  outside a modal that outside presses dismiss used to land nowhere: hit-testing skips
  everything behind the modal, so a second right-click on another word left the first word's
  context menu open. It now dismisses the modal, as an outside press does, and the frame after,
  which no longer has it, takes the gesture as if it had just begun there, so the element under
  the pointer opens its own menu, as AppKit's, Windows' and GTK's context menus do. A modal that
  outside presses leave open still takes nothing.
