- **`El::pills_after` draws a field's sized pill as a box on a line of its own after bytes that stay text**, such as a photo under its Markdown reference while the caret edits it.
  The box keeps the size, margins and look `El::pill_style` gives it, and `El::on_pill_press`, `Cx::pill_rect` and a selection covering the bytes treat it as they treat a pill.
  A caret at the end of the bytes draws after their text, never beside the box, and the arrow keys step over the box's line.
