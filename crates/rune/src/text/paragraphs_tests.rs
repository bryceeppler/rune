use super::*;
use crate::edit::Block;

const STYLE: TextStyle = TextStyle::new(15.5, 400.0).lh(25.0);

/// A Markdown-like note as an editor draws it: a title, headings, bold that runs over a line
/// break, inline and fenced code, list items, quotes, a checklist pill, blank lines, a wrapping
/// paragraph and a hidden syntax mark.
struct Note {
    text: String,
    spans: Vec<Span>,
    blocks: Vec<Block>,
    pills: Vec<PillChip>,
}

fn note(paragraphs: usize, trailing_newline: bool) -> Note {
    let mut text = String::from("A note\n");
    let mut spans = vec![Span::new(0, 6).size(26.0).weight(700.0)];
    let mut blocks = vec![Block::Paragraph];
    let mut pills = Vec::new();
    let mut line = |text: &mut String, s: &str, block: Block| {
        text.push_str(s);
        text.push('\n');
        blocks.push(block);
    };
    for i in 0..paragraphs {
        let at = text.len();
        line(&mut text, &format!("Para {i} with **bold"), Block::Paragraph);
        line(&mut text, "over lines** and `code`", Block::Paragraph);
        spans.push(Span::new(at + 7 + format!("{i}").len(), at + 9 + format!("{i}").len()).size(0.01).color(Color::TRANSPARENT));
        spans.push(Span::new(at + 9 + format!("{i}").len(), text.len() - 15).weight(600.0));
        let code = text.len() - 6;
        spans.push(Span::new(code, code + 4).mono().bg(Color::WHITE, 2.0));
        let at = text.len();
        line(&mut text, &format!("- [ ] task {i}"), Block::Bullet(0));
        pills.push(PillChip { start: at as u32 + 2, end: at as u32 + 5, icon: None, bg: None, fg: None });
        line(&mut text, &format!("> quote {i} lazily"), Block::Quote);
        spans.push(Span::new(text.len() - 7, text.len() - 1).underline(Color::WHITE));
        line(&mut text, "```", Block::Code);
        line(&mut text, &format!("fn code_{i}() {{}}"), Block::Code);
        line(&mut text, "```", Block::Code);
        line(&mut text, "", Block::Paragraph);
        line(&mut text, &format!("Wrapping {i}: the quick brown fox jumps over the lazy dog again and again until the line wraps twice."), Block::Paragraph);
    }
    if !trailing_newline {
        text.pop();
    }
    let spans = flatten_spans(&spans);
    Note { text, spans, blocks, pills }
}

fn lay_out(ts: &mut TextSystem, n: &Note, scale: f32, width: Option<f32>) -> (Rc<Layout<Ink>>, TextLayout) {
    let whole = ts.layout_blocks(&n.text, &n.spans, &n.blocks, &n.pills, STYLE, scale, width, Alignment::Start);
    let stacked = ts.layout_text(None, &n.text, &n.spans, &n.blocks, &n.pills, STYLE, scale, width, Alignment::Start, Wrapping::Words, true);
    assert!(stacked.pieces().count() > 1, "the note stacks");
    (whole, stacked)
}

/// A glyph run's x, baseline, ink, size and glyphs (id, x, advance), by bits.
type Run = (u32, u32, u16, u32, Vec<(u32, u32, u32)>);

/// Each glyph run of a line, and each inline box as a run of no glyphs.
fn runs(line: &parley::Line<'_, Ink>, dy: f32, ink: impl Fn(u16) -> u16) -> Vec<Run> {
    line.items().map(|item| match item {
        PositionedLayoutItem::GlyphRun(gr) => (
            gr.offset().to_bits(),
            (gr.baseline() + dy).to_bits(),
            ink(gr.style().brush.0),
            gr.run().font_size().to_bits(),
            gr.glyphs().map(|g| (g.id, g.x.to_bits(), g.advance.to_bits())).collect(),
        ),
        PositionedLayoutItem::InlineBox(b) => (b.x.to_bits(), (b.y + dy).to_bits(), u16::MAX, b.width.to_bits(), Vec::new()),
    }).collect()
}

/// Each glyph of a line where it draws: id, x, baseline, ink and size, and each inline box.
/// How Parley groups glyphs into runs is its own: a stacked paragraph's script-less characters
/// take its own script where one layout gives them the paragraph's before, which splits or
/// joins a run but draws the same glyphs at the same places, up to the last bits of an x that
/// Parley sums along each run.
type Placed = (u32, f32, u32, u16, u32);

fn placed(line: &parley::Line<'_, Ink>, dy: f32, ink: impl Fn(u16) -> u16) -> Vec<Placed> {
    line.items().flat_map(|item| match item {
        PositionedLayoutItem::GlyphRun(gr) => {
            let (ink, size) = (ink(gr.style().brush.0), gr.run().font_size().to_bits());
            gr.positioned_glyphs().map(|g| (g.id, g.x, (g.y + dy).to_bits(), ink, size)).collect::<Vec<_>>()
        }
        PositionedLayoutItem::InlineBox(b) => vec![(u32::MAX, b.x, (b.y + dy).to_bits(), u16::MAX, b.width.to_bits())],
    }).collect()
}

fn assert_same_glyphs(a: &[Placed], b: &[Placed], what: &str) {
    assert_eq!(a.len(), b.len(), "{what}: {a:?} != {b:?}");
    for (g, h) in a.iter().zip(b) {
        assert!((g.0, g.2, g.3, g.4) == (h.0, h.2, h.3, h.4) && (g.1 - h.1).abs() <= 1e-3, "{what}: {g:?} != {h:?}");
    }
}

/// Boxes as their users read them, in f32: Parley sums a selection's x in f64 along a path that
/// differs by where the selection starts, a few millionths of a pixel apart.
fn f32_boxes(boxes: Vec<(parley::BoundingBox, usize)>) -> Vec<([f32; 4], usize)> {
    boxes.into_iter().map(|(b, i)| ([b.x0 as f32, b.y0 as f32, b.x1 as f32, b.y1 as f32], i)).collect()
}

/// At 1x and 2x, where its line heights are whole pixels, a stacked note's lines are the lines
/// of one layout of it: text, break, metrics, glyphs, inks and pill boxes, bit for bit.
#[test]
fn a_stacked_text_lays_out_line_for_line_as_one_layout() {
    for trailing in [false, true] {
        let n = note(12, trailing);
        for (scale, width) in [(2.0, Some(700.0)), (1.0, Some(350.0)), (2.0, None)] {
            let mut ts = TextSystem::default();
            let (whole, stacked) = lay_out(&mut ts, &n, scale, width);
            assert_line_for_line(&whole, &stacked, &format!("{scale}x"));
        }
    }
}

/// `stacked` is `whole` line for line, glyph for glyph and bit for bit, at its height and width.
fn assert_line_for_line(whole: &Layout<Ink>, stacked: &TextLayout, what: &str) {
    assert_eq!(stacked.len(), whole.len(), "{what}");
    assert_eq!(stacked.height().to_bits(), whole.height().to_bits(), "{what}: height");
    assert_eq!(stacked.full_width(), whole.full_width(), "{what}: width");
    let mut lines = whole.lines().enumerate();
    for p in stacked.pieces() {
        for j in 0..p.lines {
            let (i, w) = lines.next().unwrap();
            let line = stacked.line(i).unwrap();
            assert_eq!(line.text_range(), line_bytes(whole, i), "{what}: line {i}");
            assert_eq!(line.break_reason(), w.break_reason(), "{what}: line {i}");
            assert_eq!(line.metrics(), *w.metrics(), "{what}: line {i}");
            assert_same_glyphs(&placed(&p.layout.get(j).unwrap(), p.y, |k| p.ink(k)), &placed(&w, 0.0, |k| k), &format!("{what}: line {i}"));
        }
    }
    assert!(lines.next().is_none(), "{what}");
    assert_eq!(stacked.lines().count(), whole.len(), "{what}");
}

/// Parley ends a paragraph at `\r`, `\r\n`, U+2028 and U+2029 as at `\n`, and indents each by
/// its own block: a stacked paragraph holding such a break indents the same lines.
#[test]
fn every_hard_break_parley_takes_moves_to_the_next_block() {
    let blocks = vec![Block::Paragraph, Block::Quote, Block::Bullet(1), Block::Code, Block::Quote];
    for sep in ["\r", "\r\n", "\u{2028}", "\u{2029}"] {
        let n = Note { text: format!("first{sep}second\nthird{sep}fourth{sep}"), spans: Vec::new(), blocks: blocks.clone(), pills: Vec::new() };
        let (whole, stacked) = lay_out(&mut TextSystem::default(), &n, 2.0, Some(400.0));
        assert_line_for_line(&whole, &stacked, &format!("{sep:?}"));
        queries_match(&n);
    }
}

/// Spaces, digits, punctuation and emoji at a paragraph's start take the script of its own
/// text, whatever the paragraph before it is written in: every `\n` splits, and the stack draws
/// the glyphs one layout draws.
#[test]
fn every_newline_splits_whatever_script_the_text_before_it_has() {
    let mut ts = TextSystem::default();
    for text in [
        "first\n\n- item\n123 end\n",
        "かな\n👍🏽 ond\nかな\n",
        "かな\n- かな\n\nabc\n",
        "123\nかな\nabc",
        "abc\n\nかな\n(かな)\n",
        "- 牛乳を買う\n- 卵\n- パンを焼く\n## 見出し\n「今日は雨です。」\n",
    ] {
        let whole = ts.layout_blocks(text, &[], &[], &[], STYLE, 2.0, Some(400.0), Alignment::Start);
        let stacked = ts.layout_text(None, text, &[], &[], &[], STYLE, 2.0, Some(400.0), Alignment::Start, Wrapping::Words, true);
        assert_eq!(stacked.pieces().count(), text.trim_end_matches('\n').matches('\n').count() + 1, "{text:?}");
        assert_line_for_line(&whole, &stacked, text);
    }
}

/// Random texts in one script and direction, edited a step at a time with each stack built
/// from the one before, lay out stacked as one layout of the same text, line for line and caret
/// for caret: every hard break Parley takes, `\r\n`s a span boundary or pill splits, monospace
/// tabs, hidden marks, pills, emoji, digits and brackets, blocks, wrap widths and scales.
#[test]
fn random_edits_lay_out_stacked_as_one_layout() {
    const STEPS: usize = 400;
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    let mut next = move |n: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % n.max(1) as u64) as usize
    };
    let tokens = ["word", "ab", " ", "\t", "\t", "e\u{301}", "1", "(", "👍🏽", "\n", "\n", "\r\n", "\r\n", "\r", "\u{2028}", "\u{2029}"];
    let kinds = [Block::Paragraph, Block::Quote, Block::Bullet(1), Block::Code];
    let mut ts = TextSystem::default();
    let (mut text, mut spans, mut blocks) = (String::from("first\tline\nsecond\n"), Vec::<Span>::new(), vec![Block::Paragraph]);
    let mut stacked_steps = 0;
    for step in 0..STEPS {
        let bounds: Vec<usize> = text.char_indices().map(|(i, _)| i).chain([text.len()]).collect();
        let (at, removed, inserted) = if text.len() > 200 || (bounds.len() > 2 && next(4) == 0) {
            let a = next(bounds.len() - 1);
            let b = (a + 1 + next(if text.len() > 200 { 40 } else { 4 })).min(bounds.len() - 1);
            (bounds[a], bounds[b] - bounds[a], String::new())
        } else {
            (bounds[next(bounds.len())], 0, (0..1 + next(3)).map(|_| tokens[next(tokens.len())]).collect::<String>())
        };
        text.replace_range(at..at + removed, &inserted);
        let moved = |v: u32| {
            let v = v as usize;
            (if v >= at + removed { v - removed + inserted.len() } else if v > at { at } else { v }) as u32
        };
        spans = spans.iter().map(|s| Span { start: moved(s.start), end: moved(s.end), ..s.clone() }).filter(|s| s.start < s.end).collect();
        let bounds: Vec<usize> = text.char_indices().map(|(i, _)| i).chain([text.len()]).collect();
        if step % 3 == 0 {
            let mut fresh = Vec::new();
            for _ in 0..next(6) {
                let (a, b) = (bounds[next(bounds.len())], bounds[next(bounds.len())]);
                if a == b {
                    continue;
                }
                let sp = Span::new(a.min(b), a.max(b));
                fresh.push(match next(6) {
                    0 | 1 => sp.mono(),
                    2 => sp.size(0.01).color(Color::TRANSPARENT),
                    3 => sp.weight(700.0).underline(Color::WHITE),
                    4 => sp.bg(Color::WHITE, 2.0),
                    _ => sp.size(31.0),
                });
            }
            spans = flatten_spans(&fresh);
        }
        if step % 5 == 0 {
            blocks = (0..2 + text.len() / 4).map(|_| kinds[next(kinds.len())]).collect();
        }
        let mut pills = Vec::new();
        if step % 4 == 1 {
            let a = next(bounds.len() - 1);
            let b = (a + 1 + next(4)).min(bounds.len() - 1);
            if !text[bounds[a]..bounds[b]].contains('\n') {
                pills.push(PillChip { start: bounds[a] as u32, end: bounds[b] as u32, icon: None, bg: None, fg: None });
            }
        }
        let style = if step / 13 % 3 == 0 { TextStyle { family: Family::Mono, ..STYLE } } else { STYLE };
        let (scale, width, wrapping) = ([1.0, 2.0][step / 11 % 2], [Some(60.0), Some(160.0), Some(400.0), None][step / 7 % 4], [Wrapping::Words, Wrapping::Anywhere, Wrapping::BreakAll][step / 5 % 3]);
        let stacked = ts.layout_text(None, &text, &spans, &blocks, &pills, style, scale, width, Alignment::Start, wrapping, true);
        let whole = ts.layout_blocks_inner(&text, &spans, &blocks, &pills, style, scale, width, Alignment::Start, wrapping, true);
        let what = format!("step {step}: {text:?}, spans {:?}, blocks {blocks:?}, pills {:?}", spans.iter().map(|s| (s.start, s.end)).collect::<Vec<_>>(), pills.iter().map(|p| (p.start, p.end)).collect::<Vec<_>>());
        assert_line_for_line(&whole, &stacked, &what);
        if step % 4 == 0 {
            for &b in &bounds {
                let cur = parley::editing::Cursor::from_byte_index(&whole, b, parley::layout::Affinity::Downstream);
                assert_eq!(stacked.caret_box(b, Affinity::Downstream, 1.0), cursor_geometry(&cur, &whole, 1.0), "caret at {b}, {what}");
                assert_eq!(stacked.move_line(b, Affinity::Downstream, true), move_line(&whole, b, Affinity::Downstream, true), "down from {b}, {what}");
                assert_eq!(stacked.line_edge(b, true), line_edge(&whole, b, true), "line end from {b}, {what}");
            }
        }
        stacked_steps += (stacked.pieces().count() > 1) as usize;
        ts.end_commit();
    }
    assert!(stacked_steps > STEPS * 3 / 4, "{stacked_steps} of {STEPS} steps stacked");
}

/// Each run of a line: whether it reads right to left.
fn directions(line: &parley::Line<'_, Ink>) -> Vec<bool> {
    line.runs().map(|r| r.is_rtl()).collect()
}

/// Random edits of text in several scripts and both directions, with numbers, neutrals, marks,
/// isolates and embeddings at paragraph edges, and deletions at its end. Each stack, built from the one before, is the
/// stack of the same text laid out from nothing, line for line, glyph for glyph and direction
/// for direction. Each stacked paragraph is Parley's layout of that paragraph alone in its own
/// direction, at the whole text's line tops, for its lines, carets, hits, line ends, moves
/// down and selection boxes.
#[test]
fn random_mixed_script_edits_lay_out_each_paragraph_in_its_own_direction() {
    const STEPS: usize = 300;
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    let mut next = move |n: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % n.max(1) as u64) as usize
    };
    let tokens = ["word", "o", " ", "שלום", "مرحبا", "\u{64e}", "١٢", "12", "!", "(", ")", "- ", "\u{200f}", "\u{2067}", "\u{2069}", "\u{202b}", "\u{202c}", "漢字", "かな", "👍🏽", "e\u{301}", "\t", "\r", "\u{2029}", "\n", "\n", "\n"];
    let (mut ts, mut full, mut oracle) = (TextSystem::default(), TextSystem::default(), TextSystem::default());
    let mut text = String::from("שלום world\nword\n");
    let (mut rtl, mut ltr) = (0, 0);
    for step in 0..STEPS {
        let bounds: Vec<usize> = text.char_indices().map(|(i, _)| i).chain([text.len()]).collect();
        if step % 7 == 3 {
            text.pop();
        } else if text.len() > 160 || (bounds.len() > 2 && next(4) == 0) {
            let a = next(bounds.len() - 1);
            let b = (a + 1 + next(if text.len() > 160 { 30 } else { 4 })).min(bounds.len() - 1);
            text.replace_range(bounds[a]..bounds[b], "");
        } else {
            let inserted: String = (0..1 + next(3)).map(|_| tokens[next(tokens.len())]).collect();
            text.insert_str(bounds[next(bounds.len())], &inserted);
        }
        if step % 10 == 0 {
            text.insert_str(0, ["a", "ש", "\u{2067}ש\u{2069}"][step / 10 % 3]);
        }

        let (scale, width, wrapping) = ([1.0, 2.0][step / 11 % 2], [60.0, 160.0, 400.0][step / 7 % 3], [Wrapping::Words, Wrapping::Anywhere][step / 5 % 2]);
        let lay = |ts: &mut TextSystem| ts.layout_text(None, &text, &[], &[], &[], STYLE, scale, Some(width), Alignment::Start, wrapping, true);
        let stacked = lay(&mut ts);
        ts.end_commit();
        full.drop_shaped();
        let fresh = lay(&mut full);
        let what = format!("step {step}: {text:?} at {width} x{scale}");
        assert_eq!((stacked.len(), stacked.height().to_bits(), stacked.pieces().count()), (fresh.len(), fresh.height().to_bits(), fresh.pieces().count()), "{what}");
        for (a, b) in stacked.pieces().zip(fresh.pieces()) {
            assert_eq!((a.start, a.y.to_bits(), a.lines, a.layout.is_rtl()), (b.start, b.y.to_bits(), b.lines, b.layout.is_rtl()), "{what}");
            for j in 0..a.lines {
                let (la, lb) = (a.layout.get(j).unwrap(), b.layout.get(j).unwrap());
                assert_eq!((la.text_range(), la.break_reason(), *la.metrics()), (lb.text_range(), lb.break_reason(), *lb.metrics()), "{what}: paragraph at {}", a.start);
                assert_eq!(runs(&la, a.y, |k| k), runs(&lb, b.y, |k| k), "{what}: paragraph at {}", a.start);
                assert_eq!(directions(&la), directions(&lb), "{what}: paragraph at {}", a.start);
            }
        }
        let (mut line, mut y, mut start) = (0, 0f64, 0);
        let paragraphs: Vec<&str> = if text.is_empty() { vec![""] } else { text.split_inclusive('\n').collect() };
        for (k, (para, piece)) in paragraphs.iter().zip(stacked.pieces()).enumerate() {
            let alone = oracle.layout_keyed(None, para, &[], &[], &[], STYLE, scale, Some(width), Alignment::Start, wrapping, false);
            assert_eq!(piece.layout.is_rtl(), alone.is_rtl(), "{what}: paragraph {k}");
            (rtl, ltr) = (rtl + alone.is_rtl() as usize, ltr + !alone.is_rtl() as usize);
            let shown = if k + 1 == paragraphs.len() { alone.len() } else { alone.len() - 1 };
            let (top, first) = (y.round() as f32, line);
            for j in 0..shown {
                let (a, b) = (stacked.line(line).unwrap(), alone.get(j).unwrap());
                let mut m = *b.metrics();
                for v in [&mut m.baseline, &mut m.block_min_coord, &mut m.block_max_coord, &mut m.content_block_min_coord, &mut m.content_block_max_coord] {
                    *v += top;
                }
                let r = b.text_range();
                assert_eq!(a.text_range(), r.start + start..r.end + start, "{what}: line {line}");
                assert_eq!(a.metrics(), m, "{what}: line {line}");
                assert_eq!(runs(a.parley(), top, |k| k), runs(&b, top, |k| k), "{what}: line {line}");
                assert_eq!(directions(a.parley()), directions(&b), "{what}: line {line}");
                if step % 3 == 0 {
                    let mid = (b.metrics().block_min_coord + b.metrics().block_max_coord) * 0.5;
                    for x in (-1..=(width / 23.0) as i32 + 1).map(|i| i as f32 * 23.0) {
                        let cur = cursor_from_point(&alone, x, mid);
                        assert_eq!(stacked.hit(x, top + mid), (cur.index() + start, cur.affinity()), "hit at ({x}, {}), {what}", top + mid);
                    }
                }
                y += b.metrics().line_height as f64;
                line += 1;
            }
            if step % 3 == 0 {
                let bytes: Vec<usize> = para.char_indices().map(|(i, _)| i).chain([para.len()]).collect();
                for (n, &i) in bytes[..bytes.len() - 1].iter().enumerate() {
                    let cur = parley::editing::Cursor::from_byte_index(&alone, i, parley::layout::Affinity::Downstream);
                    let mut bb = cursor_geometry(&cur, &alone, 1.0);
                    (bb.y0, bb.y1) = (bb.y0 + top as f64, bb.y1 + top as f64);
                    assert_eq!(stacked.caret_box(start + i, Affinity::Downstream, 1.0), bb, "caret at {}, {what}", start + i);
                    assert_eq!(stacked.line_edge(start + i, true), line_edge(&alone, i, true) + start, "line end from {}, {what}", start + i);
                    if let Some((to, side)) = move_line(&alone, i, Affinity::Downstream, true).filter(|&(to, _)| to < para.len()) {
                        assert_eq!(stacked.move_line(start + i, Affinity::Downstream, true), Some((to + start, side)), "down from {}, {what}", start + i);
                    }
                    let end = bytes[(n + 1 + n % 4).min(bytes.len() - 1)];
                    let shift = |boxes: Vec<(parley::BoundingBox, usize)>| boxes.into_iter().filter(|&(_, l)| l < shown)
                        .map(|(mut bb, l)| { (bb.y0, bb.y1) = (bb.y0 + top as f64, bb.y1 + top as f64); (bb, l + first) }).collect();
                    assert_eq!(f32_boxes(stacked.range_boxes(start + i, start + end, false)), f32_boxes(shift(range_boxes(&alone, i, end, false))), "boxes {}..{}, {what}", start + i, start + end);
                }
            }
            start += para.len();
        }
        assert_eq!(stacked.len(), line, "{what}");
        assert_eq!(stacked.pieces().count(), paragraphs.len(), "{what}");
    }
    assert!(rtl > STEPS && ltr > STEPS, "{rtl} right-to-left and {ltr} left-to-right paragraphs");
}

/// Each paragraph reads in its own direction, as a natural writing direction resolves it: a
/// Hebrew list item in an English note, an Arabic paragraph, an English word inside Arabic,
/// and Hebrew with punctuation each draw right to left, aligned right, with the caret at their
/// start on the right. The English lines around them, and a line of digits after Hebrew, which
/// has no strong character, stay left to right.
#[test]
fn each_paragraph_reads_in_its_own_direction() {
    let width = 600.0;
    let cases: [(&str, &str, &[&str]); 5] = [
        ("Notes\nשלום\n- עברית!\nשלום\nend", "- עברית!", &["!", "עברית", "-"]),
        ("Notes\nمرحبا بالعالم!\nend", "مرحبا بالعالم!", &["!", "بالعالم", "مرحبا"]),
        ("Notes\nمرحبا Rune بالعالم!\nend", "مرحبا Rune بالعالم!", &["!", "بالعالم", "Rune", "مرحبا"]),
        ("Notes\nעברית!\nend", "עברית!", &["!", "עברית"]),
        ("Notes\nשלום\n123 456\nend", "123 456", &["123", "456"]),
    ];
    let mut ts = TextSystem::default();
    for (text, line, left_to_right) in cases {
        let stacked = ts.layout_text(None, text, &[], &[], &[], STYLE, 2.0, Some(width), Alignment::Start, Wrapping::Words, true);
        assert_eq!(stacked.pieces().count(), text.matches('\n').count() + 1, "{text:?}");
        let start = text.find(line).unwrap();
        let span = |part: &&str| {
            let a = start + line.find(part).unwrap();
            let boxes = stacked.range_boxes(a, a + part.len(), false);
            assert_eq!(boxes.len(), 1, "{part:?} in {text:?}");
            (boxes[0].0.x0 as f32, boxes[0].0.x1 as f32)
        };
        let order: Vec<(f32, f32)> = left_to_right.iter().map(span).collect();
        assert!(order.windows(2).all(|w| w[0].1 <= w[1].0 + 0.01), "{line:?} draws {left_to_right:?} left to right: {order:?}");
        let caret = stacked.caret_box(start, Affinity::Downstream, 1.0).x0 as f32;
        if line.starts_with('1') {
            assert_eq!((order[0].0, caret), (0.0, 0.0), "{line:?} reads left to right");
        } else {
            assert!((order[order.len() - 1].1 - width).abs() < 1.0, "{line:?} aligns right: {order:?}");
            assert!((caret - width).abs() < 1.5, "the caret starts {line:?} on the right: {caret}");
        }
        for edge in ["Notes", "end"] {
            assert_eq!(stacked.caret_box(text.find(edge).unwrap(), Affinity::Downstream, 1.0).x0, 0.0, "{edge:?} starts on the left in {text:?}");
        }
    }
}

/// A text ending in a hard break ends in an empty paragraph indented by its own block, also
/// once deleting the paragraph after the break has made it the last.
#[test]
fn a_trailing_empty_paragraph_keeps_its_block() {
    let mut ts = TextSystem::default();
    let blocks = vec![Block::Paragraph, Block::Paragraph, Block::Quote];
    for text in ["first\nsecond\nthird", "first\nsecond\n"] {
        let n = Note { text: text.into(), spans: Vec::new(), blocks: blocks.clone(), pills: Vec::new() };
        let (whole, stacked) = lay_out(&mut ts, &n, 2.0, Some(400.0));
        ts.end_commit();
        assert_line_for_line(&whole, &stacked, text);
        queries_match(&n);
    }
}

/// Carets, hits, line edges, Up and Down, clusters and selection boxes come out of a stacked
/// note as they do from one layout of it, including at and across paragraph breaks.
#[test]
fn caret_hit_and_selection_queries_match_one_layout() {
    let styled = note(4, true);
    let plain = Note { spans: Vec::new(), blocks: Vec::new(), pills: Vec::new(), ..note(4, false) };
    for n in [styled, plain] {
        queries_match(&n);
    }
}

fn queries_match(n: &Note) {
    let mut ts = TextSystem::default();
    let (whole, stacked) = lay_out(&mut ts, n, 2.0, Some(700.0));
    let len = n.text.len();
    let bytes: Vec<usize> = (0..=len).filter(|&b| n.text.is_char_boundary(b)).collect();
    for &b in &bytes {
        let cur = parley::editing::Cursor::from_byte_index(&whole, b, parley::layout::Affinity::Downstream);
        assert_eq!(stacked.caret_box(b, Affinity::Downstream, 3.0), cursor_geometry(&cur, &whole, 3.0), "caret at {b}");
        for end in [false, true] {
            assert_eq!(stacked.line_edge(b, end), line_edge(&whole, b, end), "line edge at {b}");
        }
        for down in [false, true] {
            assert_eq!(stacked.move_line(b, Affinity::Downstream, down), move_line(&whole, b, Affinity::Downstream, down), "move {down} from {b}");
        }
        let cluster = parley::Cluster::from_byte_index(&whole, b).map(|c| (c.text_range(), c.is_hard_line_break()));
        assert_eq!(stacked.cluster_at(b), cluster, "cluster at {b}");
    }
    let (w, h) = (stacked.full_width() + 40.0, stacked.height() + 60.0);
    for yi in -2..=(h / 7.0) as i32 {
        for xi in -1..=(w / 37.0) as i32 {
            let (x, y) = (xi as f32 * 37.0, yi as f32 * 7.0);
            let cur = cursor_from_point(&whole, x, y);
            assert_eq!(stacked.hit(x, y), (cur.index(), cur.affinity()), "hit at ({x}, {y})");
        }
    }
    for (k, &a) in bytes.iter().enumerate().step_by(5) {
        for &b in bytes[k..].iter().step_by(11) {
            for pill in [false, true] {
                assert_eq!(f32_boxes(stacked.range_boxes(a, b, pill)), f32_boxes(range_boxes(&whole, a, b, pill)), "boxes {a}..{b}");
            }
        }
    }
}

/// With line heights off the pixel grid (1.5x here), each paragraph's first line still lands
/// on one layout's line, and a wrapped paragraph's later lines land within a pixel of it.
#[test]
fn paragraphs_start_on_one_layouts_lines_off_the_pixel_grid() {
    let n = note(8, false);
    let mut ts = TextSystem::default();
    let (whole, stacked) = lay_out(&mut ts, &n, 1.5, Some(520.0));
    assert_eq!(stacked.len(), whole.len());
    let starts: Vec<usize> = stacked.pieces().map(|p| p.start).collect();
    let mut off = 0;
    for (i, w) in whole.lines().enumerate() {
        let (a, b) = (stacked.line(i).unwrap().metrics(), *w.metrics());
        let dy = (a.block_min_coord - b.block_min_coord).abs();
        if starts.contains(&w.text_range().start) {
            assert_eq!(a, b, "a paragraph's first line, line {i}");
        } else {
            assert!(dy <= 1.0, "line {i} is {dy} px off");
            off += (dy > 0.0) as usize;
        }
    }
    assert!(off > 0, "some wrapped line rounds on its own");
}

/// An edit reshapes the paragraph it touches; Return makes two and joining two makes one.
/// Undoing it reshapes nothing: the old paragraph is still cached.
#[test]
fn an_edit_reshapes_only_the_paragraphs_it_touches() {
    let n = note(20, true);
    let mut ts = TextSystem::default();
    let lay = |ts: &mut TextSystem, text: &str| {
        let before = ts.shaped;
        ts.layout_text(None, text, &[], &n.blocks, &[], STYLE, 2.0, Some(700.0), Alignment::Start, Wrapping::Words, true);
        ts.end_commit();
        ts.shaped - before
    };
    lay(&mut ts, &n.text);
    let at = n.text.find("task 7").unwrap();
    let typed = format!("{}x{}", &n.text[..at], &n.text[at..]);
    assert_eq!(lay(&mut ts, &typed), 1, "typing");
    let split = format!("{}\n{}", &n.text[..at], &n.text[at..]);
    assert_eq!(lay(&mut ts, &split), 2, "Return");
    let joined = n.text.replacen("\n> quote 3", "> quote 3", 1);
    assert_eq!(lay(&mut ts, &joined), 1, "Backspace at a paragraph's start");
    assert_eq!(lay(&mut ts, &n.text), 0, "undo");
}

/// Typing in a note whose lines start with a marker or an indent and then Han after a line
/// ending in kana, with Hebrew and Arabic lines among them, reshapes the one paragraph it
/// touches.
#[test]
fn typing_in_a_japanese_note_with_right_to_left_lines_reshapes_one_paragraph() {
    let text = "　春の雨です。\n- 東京の電車です\n## 図書館です\n> 手紙を書きます\nשלום עולם\nمرحبا بالعالم\n".repeat(30);
    let mut ts = TextSystem::default();
    let lay = |ts: &mut TextSystem, text: &str| {
        let before = ts.shaped;
        let stacked = ts.layout_text(None, text, &[], &[], &[], STYLE, 2.0, Some(700.0), Alignment::Start, Wrapping::Words, true);
        ts.end_commit();
        (ts.shaped - before, stacked.pieces().count())
    };
    assert_eq!(lay(&mut ts, &text).1, 180);
    let at = text.len() / 2 + text[text.len() / 2..].find("電車").unwrap();
    let typed = format!("{}か{}", &text[..at], &text[at..]);
    assert_eq!(lay(&mut ts, &typed), (1, 180));
}

/// Each version of a note built from the one before (typing, Return, a join, a span appearing
/// in another paragraph, a block changing) lays out as the same note built fresh.
#[test]
fn a_stack_built_from_the_last_lays_out_as_one_built_fresh() {
    let n = note(10, true);
    let mut ts = TextSystem::default();
    let at = n.text.find("task 6").unwrap();
    let shift = |spans: &[Span], from: usize, by: isize| -> Vec<Span> {
        spans.iter().map(|s| {
            let m = |v: u32| if v as usize >= from { (v as isize + by) as u32 } else { v };
            Span { start: m(s.start), end: m(s.end), ..s.clone() }
        }).collect()
    };
    let mut versions = vec![(n.text.clone(), n.spans.clone(), n.blocks.clone())];
    versions.push((format!("{}x{}", &n.text[..at], &n.text[at..]), shift(&n.spans, at, 1), n.blocks.clone()));
    let (t, sp, _) = versions.last().unwrap().clone();
    let mut blocks = n.blocks.clone();
    blocks.insert(n.text[..at].matches('\n').count() + 1, Block::Paragraph);
    versions.push((format!("{}\n{}", &t[..at], &t[at..]), shift(&sp, at, 1), blocks.clone()));
    let mut underlined = versions.last().unwrap().1.clone();
    let word = versions.last().unwrap().0.find("quote 2").unwrap();
    underlined.push(Span::new(word, word + 5).underline(Color::WHITE));
    versions.push((versions.last().unwrap().0.clone(), flatten_spans(&underlined), blocks.clone()));
    let mut coded = blocks.clone();
    coded[3] = Block::Code;
    versions.push((versions.last().unwrap().0.clone(), versions.last().unwrap().1.clone(), coded));
    for (text, spans, blocks) in versions {
        let pills: Vec<PillChip> = Vec::new();
        let built = ts.layout_text(None, &text, &spans, &blocks, &pills, STYLE, 2.0, Some(700.0), Alignment::Start, Wrapping::Words, true);
        ts.end_commit();
        let fresh = TextSystem::default().layout_text(None, &text, &spans, &blocks, &pills, STYLE, 2.0, Some(700.0), Alignment::Start, Wrapping::Words, true);
        assert_eq!(built.len(), fresh.len());
        for (a, b) in built.pieces().zip(fresh.pieces()) {
            assert_eq!((a.start, a.y.to_bits(), a.lines), (b.start, b.y.to_bits(), b.lines));
            for j in 0..a.lines {
                let (la, lb) = (a.layout.get(j).unwrap(), b.layout.get(j).unwrap());
                assert_eq!(*la.metrics(), *lb.metrics());
                assert_eq!(runs(&la, a.y, |k| a.ink(k)), runs(&lb, b.y, |k| b.ink(k)), "paragraph at {}", a.start);
            }
        }
    }
}

/// A stack keeps its paragraphs cached across commits that don't use it, however many other
/// texts come and go, so the next keystroke after a pause reshapes one paragraph.
#[test]
fn a_kept_stack_keeps_its_paragraphs_through_idle_commits() {
    let n = note(20, true);
    let mut ts = TextSystem::default();
    let held = ts.layout_text(None, &n.text, &[], &n.blocks, &[], STYLE, 2.0, Some(700.0), Alignment::Start, Wrapping::Words, true);
    ts.end_commit();
    for i in 0..40 {
        ts.layout_rich(&format!("{i} {}", "filler words ".repeat(400)), &[], STYLE, 2.0, Some(700.0));
        ts.end_commit();
    }
    let before = ts.shaped;
    let at = n.text.find("task 9").unwrap();
    let typed = format!("{}x{}", &n.text[..at], &n.text[at..]);
    ts.layout_text(None, &typed, &[], &n.blocks, &[], STYLE, 2.0, Some(700.0), Alignment::Start, Wrapping::Words, true);
    assert_eq!(ts.shaped - before, 1);
    drop(held);
}

/// Notes laid out and then shown no more keep no more shaped bytes than the stale budget:
/// their stacks go, and their paragraphs go as the budget needs.
#[test]
fn stacks_shown_no_more_keep_only_the_stale_budget() {
    let mut ts = TextSystem::default();
    for i in 0..80 {
        let text = format!("{i} {}\n{i} {}", "word ".repeat(400), "other ".repeat(400));
        ts.layout_text(None, &text, &[], &[], &[], STYLE, 2.0, Some(400.0), Alignment::Start, Wrapping::Words, true);
        ts.end_commit();
    }
    for _ in 0..4 {
        ts.end_commit();
    }
    let held: usize = ts.layouts.values().map(|e| e.cost as usize).sum();
    assert!(held <= super::super::layout_cache::STALE_BYTES, "{} layouts hold {held} bytes", ts.layouts.len());
    assert!(ts.stacks.cache.is_empty());
}

/// New families or a new language lay a stacked text out again in them.
#[test]
fn new_fonts_lay_stacks_out_again() {
    let n = note(3, true);
    let mut ts = TextSystem::default();
    let lay = |ts: &mut TextSystem| ts.layout_text(None, &n.text, &n.spans, &n.blocks, &n.pills, STYLE, 2.0, Some(700.0), Alignment::Start, Wrapping::Words, true);
    let before = lay(&mut ts);
    assert!(ts.set_language(parley::fontique::Language::parse("ja").ok()));
    let shaped = ts.shaped;
    let after = lay(&mut ts);
    assert_ne!(before.id(), after.id());
    assert!(ts.shaped > shaped, "its paragraphs shaped again");
}

/// Text with unsorted spans or one paragraph, or unwrapped with right-to-left letters, lays out
/// whole.
#[test]
fn texts_that_cannot_stack_lay_out_whole() {
    let mut ts = TextSystem::default();
    let mut pieces = |text: &str, spans: &[Span]| {
        ts.layout_text(None, text, spans, &[], &[], STYLE, 2.0, None, Alignment::Start, Wrapping::Words, true).pieces().count()
    };
    assert_eq!(pieces("one\ntwo", &[]), 2);
    assert_eq!(pieces("one\n", &[]), 1);
    assert_eq!(pieces("one\nשלום", &[]), 1, "unwrapped right-to-left lines align on the widest");
    assert_eq!(pieces("one\ntwo", &[Span::new(4, 7), Span::new(0, 3)]), 1);
    let centred = ts.layout_text(None, "one\ntwo", &[], &[], &[], STYLE, 2.0, None, Alignment::Center, Wrapping::Words, true);
    assert_eq!(centred.pieces().count(), 1, "unwrapped centred lines centre on the widest");
}



