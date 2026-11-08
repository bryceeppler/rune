- **A field's pill chips can act as buttons.** `El::on_pill_press` hears a primary press that
  begins and ends on a `Mark::Pill` chip in a `text_input`, by the pill's id, for checkboxes and
  chips that open something. The press leaves the field's caret, selection and focus as they
  were, and a composition open in that field commits before the handler runs, so an edit the
  handler makes never loses it. The chip shows the pointing hand; a read-only field's chip shows
  none and a press on it changes nothing.
