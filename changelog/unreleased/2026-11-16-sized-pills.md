- **`PillStyle::sized` draws a field's pill as one box in place of its label, such as a photo.**
  The box is the size given, scaled down to the line's width; the line grows to its height, and the caret beside it and the selection's strip on its line stay text height.
  `PillStyle::image` fills the box with `Pixels`, cropped to its shape, and a decode that lands later wakes the window and draws without laying the text out again; until then the box shows `PillStyle::bg`, its icon and a `PillStyle::caption` centred.
  A selection that covers the pill tints the box, or draws `PillStyle::selection_ring` outside it, with no caret; `PillStyle::border` keeps a hairline on its edge, and `PillStyle::radius` sets the corners of a box or chip.
  Assistive technology reads U+FFFC for the pill, with an image node carrying `PillStyle::description`.
  `PillStyle` is no longer `Copy`, since it can hold `Pixels`: clone it where an app copied it.
