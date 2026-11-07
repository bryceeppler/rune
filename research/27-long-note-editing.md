# 27. Editing a long Markdown note

Reader: someone deciding how a notes app should hold a note in Rune, or changing text layout,
text paint or `TextEdit` history. Re-run the bench below before and after changing any of them.

The question: can one multi-line `text_input` hold a whole ~50 KB Markdown note and stay
smooth at 120 Hz (8.3 ms a frame)? It can scroll, move the caret and follow it at any size
measured. It can't type at 50 KB: every keystroke reshapes the whole note, at 12 ms.

## How it was measured

`crates/rune/examples/long_note.rs`, headless, no window:

```
cargo run --profile perf -p rune --example long_note --features frame-profile -- \
  --sizes 5,50,200 --samples 60 --gpu [--blocks]
```

The note is generated from fictional words: headings, wrapping paragraphs, lists, task lists,
quotes and fenced code, at 8.9 KB (142 lines), 54 KB (861 lines) and 207 KB (3,289 lines). It
sits in a 760 × 900 pt scroll viewport at 2x, in 15 pt type. Each case times an input and the
frame it commits, in thread CPU. `--gpu` adds the renderer's encode and submit time on a
headless GPU, which stayed under 0.5 ms (p95) in every case. `--blocks` splits the note at blank
lines into one keyed field per block (about 1,600 fields at 200 KB). Numbers below are medians
(p95 in brackets) in ms, from an M4 Pro Mac mini.

## Results: one field for the whole note

| Case | 5 KB | 50 KB | 200 KB |
|---|---|---|---|
| Type a character (start, middle or end) | 2.2 (2.5) | 12.0 (12.2) | 45.7 (46.6) |
| Return | 2.0 (2.1) | 12.0 (12.3) | 46.2 (49.2) |
| Backspace | 2.0 (2.1) | 12.0 (12.4) | 46.4 (48.9) |
| Paste 2 KB | 2.4 (2.5) | 12.5 (12.8) | 47.7 (50.0) |
| Cmd+Z right after the paste | 0.12 (0.14) | 0.32 (0.35) | 0.74 (0.81) |
| Scroll frame | 0.10 (0.13) | 0.10 (0.12) | 0.09 (0.12) |
| Down arrow | 0.12 (0.15) | 0.19 (0.23) | 0.41 (0.46) |
| Cmd+Down to the end | 0.17 (0.22) | 0.23 (0.26) | 0.51 (0.56) |
| Click to place the caret | 0.11 (0.18) | 0.16 (0.18) | 0.39 (0.42) |

Caret-follow held at every size: across 120 Down presses the caret never left the viewport,
which moved at most one line (22 pt) per press; Cmd+Down landed at the end with the caret in
view; 30 of 30 clicks placed the caret where clicked without moving the viewport.

Memory (heap growth over the process before the note opened, MB):

| | 5 KB | 50 KB | 200 KB |
|---|---|---|---|
| Opened | 5.5 | 9.8 | 24.8 |
| After 50 typed characters | 10.3 | 18.8 | 45.6 |
| After 200 | 10.5 | 18.8 | 45.6 |
| Undo steps kept of 100 small pastes | 53 | 9 | 2 |

## Why typing costs what it does

`TextSystem::layout_blocks` caches a layout under a hash of the field's whole text, so an edit
anywhere misses and Parley lays out the whole string again: font selection, text analysis,
HarfRust shaping and line breaking, about 0.22 ms per KB. A profile of 200 KB typing puts all
of the frame there. Cmd+Z right after an edit is cheap because the previous text's layout is
still in the cache (stale layouts stay for two commits, and longer within a 64 KB budget).

Memory follows from the same thing: a whole-note layout is roughly 90 bytes of heap per byte
of text, and while typing two are alive, the current one and the previous one.

Undo depth comes from `TextEdit`'s history: up to 128 steps within 1 MB, where each step holds
a full copy of the text before and after it. A long note fits only a few steps: about
1 MB ÷ (2 × note size). Typing coalesces a run of keystrokes into one step, so a step is a burst
of typing or one paste, but a 200 KB note still forgets everything before the last two.

## What was fixed: paint walked every line

Before this change, painting a text walked every glyph of its layout, looking each up in the
atlas, and only then dropped the glyphs outside the clip. A 200 KB note in a scroll view spent
5.4 ms of every frame on that, scrolling included. Paint now binary-searches the layout's lines
for the clip's band (with two ems of slack for ink past a line box) and walks only those. The
per-glyph clip test still decides what draws, so the output is unchanged: the glyph counts
match before and after in every case.

| Case, median ms | 5 KB | 50 KB | 200 KB |
|---|---|---|---|
| Scroll frame | 0.27 → 0.10 | 1.46 → 0.10 | 5.45 → 0.09 |
| Down arrow | 0.30 → 0.12 | 1.58 → 0.19 | 5.81 → 0.41 |
| Click | 0.31 → 0.11 | 1.57 → 0.16 | 5.78 → 0.39 |
| Type a character | 2.44 → 2.19 | 13.68 → 11.96 | 51.46 → 45.72 |

The test `a_long_scrolled_text_paints_only_its_visible_lines` checks that a glyph on a text's
last line isn't rasterized until it's scrolled into view.

## Results: one field per block (`--blocks`)

| Case | 5 KB | 50 KB | 200 KB |
|---|---|---|---|
| Type a character (middle) | 0.31 (0.36) | 0.82 (0.96) | 2.6 (3.2) |
| Type at the start | 0.22 (0.28) | 0.69 (0.87) | 3.0 (4.4) |
| Return | 0.27 (0.34) | 0.77 (0.96) | 2.5 (2.8) |
| Backspace | 0.28 (0.29) | 0.66 (0.82) | 2.4 (3.0) |
| Paste 2 KB | 0.70 (0.75) | 1.38 (1.58) | 3.4 (4.5) |
| Scroll frame | 0.10 (0.12) | 0.12 (0.17) | 0.18 (0.22) |
| Heap after 200 typed characters, MB | 7.2 | 11.7 | 27.1 |
| Undo steps kept of 100 | 100 | 100 | 100 |

An edit reshapes one block (0.03 to 0.6 ms). What's left grows with the number of blocks:
building, diffing and laying out every field each commit, about 2.2 ms at 1,600 fields.

## Done: one layout per paragraph

A text of several hard paragraphs now lays out one Parley layout per paragraph, cached under that paragraph's text, spans, pills and block, and stacks them in a `TextLayout` (`text/paragraphs.rs`).
A stacked paragraph ends at a `\n`, where Parley always ends a paragraph and monospace tab stops restart their columns.
Parley's other hard breaks (`\r`, U+2028, U+2029, and a `\r\n` that a style change or inline box splits in two) stay inside it; it lays out with one block per paragraph Parley makes of it, a count read back from its layout.
Characters with no script of their own take the script of the text before them, which picks their fallback fonts and shaping, so a paragraph that starts with them splits off only where the text before it gives them the script they would take alone; a text whose only script is Latin splits at every `\n`.
A property test edits random text with every hard break, spans that split `\r\n`, pills, monospace tabs, mixed scripts, blocks, widths and scales, and checks each stack against one layout of the same text.
An edit's stack starts from the last one built, so only the paragraphs it touches are laid out.
A stack stays cached only while a node shows it or for two commits; its paragraphs then stay or go under the layout cache's stale budget like any other layout.
Carets, hits, line moves, selection boxes, decorations, pills, glyphs and accessibility lines go through the stack, and its tests check each against one layout of the same text.
A paragraph's first line lands where one layout puts it; with line heights off the pixel grid (a heading, or 1.5x), a wrapped paragraph's later lines can sit a device pixel off one layout's, because each paragraph rounds its own lines.
Right-to-left text, unsorted spans and centred lines without a wrap width still lay out whole.

Measured typing into a notes app's long note (40 KB, 3,091 lines) on Linux with an NVIDIA GPU, five alternating runs a side, the `perf` build pinned to the performance cores: a typed character's frame went from 31.6 ms to 3.1 ms at p95 (2.95 to 3.50), and 30.8 to 2.46 ms at p50.
Shaping fell from 27.7 ms to 0.04 ms of it.
Of the 2.4 ms left (median CPU), the app's own change handler takes about 1.1 ms, and Rune's share is about 1.2 ms, all of it still walking the note: the field's rich spans rebuilt from its whole text once a view (about 0.2 ms), the stack rebuilt over every paragraph (about 0.15 ms), the edit itself in `TextEdit` (about 0.17 ms), and block decorations over every line (about 0.05 ms).

## What's left, and a plan

- **Undo depth.** Store each step as its changed byte range (the removed and inserted text)
  plus the small mark, block and soft-break lists, so a step costs its edit, not the note;
  check an undo still applies by length and a hash rather than comparing full copies.
- **Per-commit work proportional to the note.**
  The whole text is still hashed for the layout cache and copied into the view on each rebuild, a field's rich spans are rebuilt from its whole text, a stack is rebuilt over every paragraph, and block decorations walk every line.
  Each is a fraction of a millisecond at 40 KB; together they are Rune's 1.2 ms above.
- **Many blocks.** `El::virtualize` would make a block-per-field note's commits independent of
  its length, but in a first try a focused field inside a virtualized stack reported its caret
  (`ElementInfo::caret`, which caret-follow also uses) away from its row, and a field revealed
  with `Tx::scroll_into_view` didn't take typing headlessly. That needs its own look before a
  notes app relies on it.

## Advice for a notes app

- Up to about 10 KB, one field per note is fine: 2 ms a keystroke.
- Beyond that, use one keyed field per Markdown block (paragraph, list, quote, code fence). That
  stays under 1 ms a keystroke at 50 KB and under 4 ms at 200 KB, keeps a full undo history per
  block, and holds a few MB instead of two whole-note layouts.
- Moving between blocks becomes the app's job: Up and Down at a block's first or last line move
  focus to the neighbouring block; Backspace at a block's start merges it into the one above;
  Return splits it; a selection across blocks and an undo spanning blocks need an app-level
  model.
- Handle Return. In a multi-line `text_input`, plain Return in a paragraph submits (to
  `on_submit`, or the default button when there is none), while Shift+Return and Option+Return
  type a newline (as does plain Return inside a rich list, quote or code block). An editor
  gives each field an `on_submit` that splits the block or inserts `"\n"`.
