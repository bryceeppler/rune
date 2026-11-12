- **A menu gives focus back to whatever had it when it opened.**
  A `Menu` with `on_close`, a context menu included, returns focus on Escape, an outside click or a choice to what was focused when it opened, and to nothing when nothing was.
  A menu its button's click opened still returns to that button, since the click focused it.
  Before, it always returned to its trigger, so a menu an app's shortcut opened from a list row left the button focused and the list lost the next arrow key.
  A `Menu` without `on_close` still returns focus to its trigger.
