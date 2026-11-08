- **Shift or Option with the primary V reaches key handlers before it pastes.** An app can bind
  Paste as Plain Text (Ctrl+Shift+V, ⌥⇧⌘V) in `on_input_key` or `on_key`; when no handler takes
  the chord it still pastes, as before. The plain primary V pastes before any handler hears it,
  as before, on keys with and without a physical code.
