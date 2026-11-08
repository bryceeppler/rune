- **A combining mark straight after a line break in right-to-left text no longer panics debug
  builds.** Such a line ends on the mark, which parley's selection geometry asserts against, so a
  selection or composition underline over it (a Hebrew composition ending in a carriage return
  typed before an accent, say) now takes Rune's own line boxes.
