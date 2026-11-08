- **A right-click outside an open menu opens the menu of what is under it.** A secondary press
  outside a modal used to land nowhere: hit-testing skips everything behind the modal, so a
  second right-click on another word left the first word's context menu open. A modal now has a
  `ModalKind` (`El::modal_kind`), `Dialog` unless set. Outside a `Menu` that outside presses
  dismiss, a right-click dismisses it, and the frame after, which no longer has it, takes the
  gesture as if it had just begun there, so the element under the pointer opens its own menu, as
  AppKit's, Windows' and GTK's context menus do. Outside a `Dialog` (a dialog, sheet or alert) a
  right-click dismisses it when an outside press does and reaches nothing behind it. Rune Kit's
  `Menu`, `context_menu` and `Select` mark their modal a `Menu`; `Dialog` stays a `Dialog`.
