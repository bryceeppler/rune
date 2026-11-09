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

/// `stacked` is `whole` line for line, bit for bit, at its height and width.
fn assert_line_for_line(whole: &Layout<Ink>, stacked: &TextLayout, what: &str) {
    assert_eq!(stacked.len(), whole.len(), "{what}");
    assert_eq!(stacked.height().to_bits(), whole.height().to_bits(), "{what}: height");
    assert_eq!(stacked.full_width(), whole.full_width(), "{what}: width");
    let mut lines = whole.lines().enumerate();
    for p in stacked.pieces() {
        for j in 0..p.lines {
            let (i, w) = lines.next().unwrap();
            let line = stacked.line(i).unwrap();
            assert_eq!(line.text_range(), w.text_range(), "{what}: line {i}");
            assert_eq!(line.break_reason(), w.break_reason(), "{what}: line {i}");
            assert_eq!(line.metrics(), *w.metrics(), "{what}: line {i}");
            assert_eq!(runs(&p.layout.get(j).unwrap(), p.y, |k| p.ink(k)), runs(&w, 0.0, |k| k), "{what}: line {i}");
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

/// Spaces, digits, punctuation and emoji take the script of the text before them, which picks
/// their fallback fonts: a paragraph starting with them stacks apart only where the text before
/// it gives them the script it would give them alone, so it lays out as in one layout. Latin
/// text splits at every `\n`.
#[test]
fn a_paragraph_starting_without_a_script_takes_the_one_before_it() {
    let mut ts = TextSystem::default();
    for (text, pieces) in [
        ("first\n\n- item\n123 end\n", 4),
        ("かな\n👍🏽 ond\nかな\n", 2),
        ("かな\n- かな\n\nabc\n", 3),
        ("123\nかな\nabc", 2),
        ("abc\n\nかな\n(かな)\n", 4),
    ] {
        let whole = ts.layout_blocks(text, &[], &[], &[], STYLE, 2.0, Some(400.0), Alignment::Start);
        let stacked = ts.layout_text(None, text, &[], &[], &[], STYLE, 2.0, Some(400.0), Alignment::Start, Wrapping::Words, true);
        assert_eq!(stacked.pieces().count(), pieces, "{text:?}");
        assert_line_for_line(&whole, &stacked, text);
    }
}

/// Random texts, edited a step at a time with each stack built from the one before, lay out
/// stacked as one layout of the same text, line for line and caret for caret: every hard break
/// Parley takes, `\r\n`s a span boundary or pill splits, monospace tabs, hidden marks, pills,
/// blocks, wrap widths and scales.
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
    let tokens = ["word", "ab", " ", "\t", "\t", "e\u{301}", "漢字", "かな", "1", "(", "👍🏽", "\n", "\n", "\r\n", "\r\n", "\r", "\u{2028}", "\u{2029}"];
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
        let (scale, width, wrapping) = ([1.0, 2.0][step / 11 % 2], [Some(60.0), Some(160.0), Some(400.0), None][step / 7 % 4], [Wrapping::Words, Wrapping::Anywhere, Wrapping::BreakAll][step % 3]);
        let stacked = ts.layout_text(None, &text, &spans, &blocks, &pills, style, scale, width, Alignment::Start, wrapping, true);
        let whole = ts.layout_blocks_inner(&text, &spans, &blocks, &pills, style, scale, width, Alignment::Start, wrapping, true);
        let what = format!("step {step}: {text:?}, spans {:?}, blocks {blocks:?}, pills {:?}", spans.iter().map(|s| (s.start, s.end)).collect::<Vec<_>>(), pills.iter().map(|p| (p.start, p.end)).collect::<Vec<_>>());
        assert_line_for_line(&whole, &stacked, &what);
        if step % 4 == 0 {
            for &b in &bounds {
                let cur = parley::editing::Cursor::from_byte_index(&whole, b, parley::layout::Affinity::Downstream);
                assert_eq!(stacked.caret_box(b, 1.0), cursor_geometry(&cur, &whole, 1.0), "caret at {b}, {what}");
                assert_eq!(stacked.move_line(b, true), move_line(&whole, b, true), "down from {b}, {what}");
                assert_eq!(stacked.line_edge(b, true), line_edge(&whole, b, true), "line end from {b}, {what}");
            }
        }
        stacked_steps += (stacked.pieces().count() > 1) as usize;
        ts.end_commit();
    }
    assert!(stacked_steps > STEPS * 3 / 4, "{stacked_steps} of {STEPS} steps stacked");
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
        assert_eq!(stacked.caret_box(b, 3.0), cursor_geometry(&cur, &whole, 3.0), "caret at {b}");
        for end in [false, true] {
            assert_eq!(stacked.line_edge(b, end), line_edge(&whole, b, end), "line edge at {b}");
        }
        for down in [false, true] {
            assert_eq!(stacked.move_line(b, down), move_line(&whole, b, down), "move {down} from {b}");
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

/// Text with right-to-left characters, unsorted spans or one paragraph lays out whole.
#[test]
fn texts_that_cannot_stack_lay_out_whole() {
    let mut ts = TextSystem::default();
    let mut pieces = |text: &str, spans: &[Span]| {
        ts.layout_text(None, text, spans, &[], &[], STYLE, 2.0, None, Alignment::Start, Wrapping::Words, true).pieces().count()
    };
    assert_eq!(pieces("one\ntwo", &[]), 2);
    assert_eq!(pieces("one\n", &[]), 1);
    assert_eq!(pieces("one\nשלום", &[]), 1);
    assert_eq!(pieces("one\ntwo", &[Span::new(4, 7), Span::new(0, 3)]), 1);
    let centred = ts.layout_text(None, "one\ntwo", &[], &[], &[], STYLE, 2.0, None, Alignment::Center, Wrapping::Words, true);
    assert_eq!(centred.pieces().count(), 1, "unwrapped centred lines centre on the widest");
}
