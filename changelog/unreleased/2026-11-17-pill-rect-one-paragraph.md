- `Cx::pill_rect` reads only the paragraph that holds the pill.
  It walked every paragraph of the field for each read, so a view reading ten photo rects in a long note paid for the whole note ten times in every frame.
