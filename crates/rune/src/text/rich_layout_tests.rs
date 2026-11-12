use super::*;

const TOKEN: &str = "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
const WORDS: &str = "a plain sentence that wraps at spaces across a few short lines";

fn lines(l: &Layout<Ink>) -> Vec<std::ops::Range<usize>> {
    l.lines().map(|line| line.text_range()).collect()
}

#[test]
fn wrap_anywhere_breaks_an_unbroken_word_inside_the_width() {
    let mut ts = TextSystem::default();
    let style = TextStyle::new(14.0, 400.0);
    let (scale, max_w) = (2.0, 160.0 * 2.0);
    let clipped = ts.layout_rich(TOKEN, &[], style, scale, Some(max_w));
    assert_eq!(clipped.len(), 1, "without wrap-anywhere the token stays on one line");
    assert!(clipped.full_width() > max_w);

    let spans = [Span::new(0, 6).bold()];
    let opts = RichLayoutOptions::new().wrap_anywhere();
    let wrapped = ts.layout_rich_with(TOKEN, &spans, style, scale, Some(max_w), opts);
    assert!(wrapped.len() >= 3, "the token wraps over several lines (got {})", wrapped.len());
    for line in wrapped.lines() {
        assert!(line.metrics().advance <= max_w + 0.5, "line {:?} fits {max_w}", line.text_range());
    }
    let r = lines(&wrapped);
    assert_eq!((r[0].start, r.last().unwrap().end), (0, TOKEN.len()));
    assert!(r.windows(2).all(|w| w[0].end == w[1].start), "lines cover the token without gaps");
}

#[test]
fn wrap_anywhere_keeps_word_breaks_and_defaults_match_layout_rich() {
    let mut ts = TextSystem::default();
    let style = TextStyle::new(14.0, 400.0);
    let plain = ts.layout_rich(WORDS, &[], style, 2.0, Some(240.0));
    let anywhere = ts.layout_rich_with(WORDS, &[], style, 2.0, Some(240.0), RichLayoutOptions::new().wrap_anywhere());
    assert!(plain.len() > 1);
    assert_eq!(lines(&plain), lines(&anywhere), "words that fit break at spaces either way");

    let default = ts.layout_rich_with(WORDS, &[], style, 2.0, Some(240.0), RichLayoutOptions::default());
    assert!(Rc::ptr_eq(&plain, &default), "default options share the layout_rich cache entry");
    let aligned = ts.layout_aligned(WORDS, &[], style, 2.0, Some(240.0), Alignment::Center);
    let with = ts.layout_rich_with(WORDS, &[], style, 2.0, Some(240.0), RichLayoutOptions::new().align(Alignment::Center));
    assert!(Rc::ptr_eq(&aligned, &with), "align matches layout_aligned");
}

#[test]
fn a_hanging_paragraph_wraps_under_the_text_after_its_marker() {
    use crate::edit::Block;
    let mut ts = TextSystem::default();
    let style = TextStyle::new(14.0, 400.0);
    let item = format!("- [ ] {WORDS}");
    let text = format!("Title\n{item}");
    let marker = text.find("a plain").unwrap();
    let blocks = [Block::Paragraph, Block::Hanging { marker: 6, level: 0, quote: false }];
    let plain = ts.layout_blocks(&text, &[], &[Block::Paragraph; 2], &[], style, 2.0, Some(240.0), Alignment::Start);
    let marker_x = parley::Cluster::from_byte_index(&plain, marker).and_then(|c| c.visual_offset()).unwrap();
    let hung = ts.layout_blocks(&text, &[], &blocks, &[], style, 2.0, Some(240.0), Alignment::Start);
    let item_lines: Vec<_> = hung.lines().filter(|l| l.text_range().start > 5).collect();
    assert!(item_lines.len() > 2, "the item wraps");
    assert_eq!(item_lines[0].metrics().inline_min_coord, 0.0, "the marker starts the first line");
    for line in &item_lines[1..] {
        let m = line.metrics();
        assert_eq!(m.inline_min_coord, marker_x, "line {:?} starts under the text", line.text_range());
        assert!(m.inline_min_coord + m.advance - m.hanging_advance <= 240.5, "line {:?} fits", line.text_range());
    }

    let word = format!("- [ ] {}", TOKEN.repeat(2));
    let alone = ts.layout_blocks(&word, &[], &[Block::Hanging { marker: 6, level: 0, quote: false }], &[], style, 2.0, Some(240.0), Alignment::Start);
    assert_eq!(alone.get(0).unwrap().text_range(), 0..6, "a word wider than the line leaves the marker alone on the first line");
    let short = ts.layout_blocks("- [ ] x", &[], &[Block::Paragraph], &[], style, 2.0, Some(240.0), Alignment::Start);
    let x = parley::Cluster::from_byte_index(&short, 6).and_then(|c| c.visual_offset()).unwrap();
    let starts: Vec<f32> = alone.lines().skip(1).map(|l| l.metrics().inline_min_coord).collect();
    assert!(starts.iter().all(|&s| s == x), "the word starts under where text follows the marker, at {x}: {starts:?}");

    let wide = format!("{}x", "-".repeat(80));
    let layout = ts.layout_blocks(&wide, &[], &[Block::Hanging { marker: 81, level: 0, quote: false }], &[], style, 2.0, Some(240.0), Alignment::Start);
    assert!(layout.len() > 1, "{wide:?} wraps");
    assert!(layout.lines().all(|l| l.metrics().inline_min_coord == 0.0), "{wide:?} wraps at the margin");
}

/// Where each line of a right-to-left layout starts on the right, in device px: a wrapped
/// line's trailing space hangs past its left end.
fn right_edges(layout: &Layout<Ink>) -> Vec<f32> {
    layout.lines().map(|l| { let m = l.metrics(); m.inline_min_coord + m.offset + m.advance }).collect()
}

#[test]
fn a_right_to_left_hanging_paragraph_wraps_under_the_text_after_its_marker_from_the_right() {
    use crate::edit::Block;
    let mut ts = TextSystem::default();
    let style = TextStyle::new(14.0, 400.0);
    let item = format!("- [ ] {}", "שלום עולם ".repeat(8));
    let plain = ts.layout_blocks(&item, &[], &[Block::Paragraph], &[], style, 2.0, Some(240.0), Alignment::Start);
    let text = parley::Cluster::from_byte_index(&plain, 6).unwrap();
    let marker = 240.0 - (text.visual_offset().unwrap() + text.advance());
    assert!(marker > 10.0, "the marker takes room on the right: {marker}");
    let hung = ts.layout_blocks(&item, &[], &[Block::Hanging { marker: 6, level: 0, quote: false }], &[], style, 2.0, Some(240.0), Alignment::Start);
    let edges = right_edges(&hung);
    assert!(edges.len() > 2, "the item wraps");
    assert!((edges[0] - 240.0).abs() < 0.5, "the marker starts the first line at the right margin: {edges:?}");
    for (i, edge) in edges.iter().enumerate().skip(1) {
        assert!((edge - (240.0 - marker)).abs() < 0.5, "line {i} starts under the text, {marker} from the right: {edges:?}");
        assert!(hung.get(i).unwrap().metrics().inline_min_coord >= 0.0, "line {i} fits");
    }
}

#[test]
fn a_right_to_left_list_item_indents_from_the_right() {
    use crate::edit::Block;
    let mut ts = TextSystem::default();
    let style = TextStyle::new(14.0, 400.0);
    let indent = (Block::Bullet { level: 0, quote: false }.indent_pt(14.0) * 2.0).round();
    for (text, rtl) in [("שלום עולם ".repeat(8), true), ("hello world ".repeat(8), false)] {
        let layout = ts.layout_blocks(text.trim_end(), &[], &[Block::Bullet { level: 0, quote: false }], &[], style, 2.0, Some(240.0), Alignment::Start);
        assert!(layout.len() > 1, "{text:?} wraps");
        for line in layout.lines() {
            let m = line.metrics();
            let (left, right) = if rtl {
                (m.inline_min_coord + m.offset + m.hanging_advance, m.inline_min_coord + m.offset + m.advance)
            } else {
                (m.inline_min_coord + m.offset, m.inline_min_coord + m.offset + m.advance - m.hanging_advance)
            };
            if rtl {
                assert!((right - (240.0 - indent)).abs() < 0.5 && left >= -0.5, "{text:?} starts {indent} in from the right: {left}..{right}");
            } else {
                assert!((left - indent).abs() < 0.5 && right <= 240.5, "{text:?} starts {indent} in from the left: {left}..{right}");
            }
        }
    }
}

#[test]
fn a_nested_hanging_paragraph_steps_in_like_a_nested_bullet_and_wraps_under_its_text() {
    use crate::edit::Block;
    let mut ts = TextSystem::default();
    let style = TextStyle::new(14.0, 400.0);
    for (words, rtl) in [("hello world ".repeat(8), false), ("שלום עולם ".repeat(8), true)] {
        let words = words.trim_end();
        let from_start = |layout: &Layout<Ink>| -> Vec<f32> {
            layout
                .lines()
                .map(|l| {
                    let m = l.metrics();
                    if rtl { 240.0 - (m.inline_min_coord + m.offset + m.advance) } else { m.inline_min_coord + m.offset }
                })
                .collect()
        };
        let mut bullet = |level| from_start(&ts.layout_blocks(words, &[], &[Block::Bullet { level, quote: false }], &[], style, 2.0, Some(240.0), Alignment::Start))[0];
        let step = bullet(1) - bullet(0);
        assert!(step > 10.0 && (bullet(2) - bullet(1) - step).abs() < 0.5, "{words:?}: bullets step in evenly, by {step}");
        let item = format!("1. {words}");
        let plain = ts.layout_blocks(&item, &[], &[Block::Paragraph], &[], style, 2.0, Some(240.0), Alignment::Start);
        let text = parley::Cluster::from_byte_index(&plain, 3).unwrap();
        let marker = if rtl { 240.0 - (text.visual_offset().unwrap() + text.advance()) } else { text.visual_offset().unwrap() };
        for level in 0..=2u8 {
            let layout = ts.layout_blocks(&item, &[], &[Block::Hanging { marker: 3, level, quote: false }], &[], style, 2.0, Some(240.0), Alignment::Start);
            let starts = from_start(&layout);
            let indent = step * f32::from(level);
            assert!(starts.len() > 2, "{item:?} at level {level} wraps");
            assert!((starts[0] - indent).abs() < 0.5, "{item:?} at level {level}: the marker starts {indent} in, as a nested bullet steps: {starts:?}");
            for (i, s) in starts.iter().enumerate().skip(1) {
                assert!((s - (indent + marker)).abs() < 0.5, "{item:?} at level {level}: line {i} starts under the text, {} in: {starts:?}", indent + marker);
            }
        }
    }
}

#[test]
fn a_pill_before_a_paragraphs_first_letter_leaves_its_direction_to_the_letters_after_it_outside_isolates() {
    let mut ts = TextSystem::default();
    let style = TextStyle::new(15.5, 400.0);
    let pill = [PillChip { start: 0, end: 6, icon: None, bg: None, fg: None }];
    for (text, pills, rtl) in [
        ("- [x] שלום", &pill[..], true),
        ("- [x] hello", &pill[..], false),
        ("x שלום", &[][..], false),
        ("- [ ] שלום", &[][..], true),
        ("- [ ] \u{2066}English\u{2069} שלום עולם", &pill[..], true),
        ("- [x] \u{2067}שלום\u{2069} hello", &pill[..], false),
        ("- [ ] \u{2068}English\u{2069}\u{2069} שלום", &pill[..], true),
        ("- [x] \u{2067}שלום", &pill[..], false),
        ("שלום - [x] \u{2066}English", &[PillChip { start: 9, end: 15, icon: None, bg: None, fg: None }][..], true),
    ] {
        let layout = ts.layout_blocks_inner(text, &[], &[crate::edit::Block::Hanging { marker: 6, level: 0, quote: false }], pills, style, 2.0, Some(600.0), Alignment::Start, Wrapping::Words, false);
        assert_eq!(layout.is_rtl(), rtl, "{text:?} with {} pills", pills.len());
    }
}

#[test]
fn a_pill_starting_a_right_to_left_paragraph_draws_its_chip_and_icon_on_the_right() {
    use crate::edit::Block;
    let mut ts = TextSystem::default();
    let style = TextStyle::new(15.5, 400.0);
    let pm = PillMetrics::new(15.5);
    static BOX: Icon = Icon { name: "box", view: 24.0, body: "<rect x=\"4\" y=\"4\" width=\"16\" height=\"16\"/>" };
    let icon = Some(&BOX);
    for (text, rtl) in [("- [ ] שלום עולם", true), ("- [ ] hello world", false)] {
        let pills = [PillChip { start: 0, end: 6, icon, bg: None, fg: None }];
        let hidden = [Span::new(0, 6).size(0.01)];
        let laid = ts.layout_blocks_inner(text, &hidden, &[Block::Hanging { marker: 6, level: 0, quote: false }], &pills, style, 2.0, Some(600.0), Alignment::Start, Wrapping::Words, false);
        let word = parley::Cluster::from_byte_index(&laid, 6).unwrap();
        let (word_x0, word_x1) = (word.visual_offset().unwrap(), word.visual_offset().unwrap() + word.advance());
        let layout = TextLayout::from(laid.clone());
        let rects = pill_chip_rects(&layout, &pills, 15.5, 2.0, 0.0..100.0);
        let [(0, chip)] = rects[..] else { panic!("{text:?}: one chip, {rects:?}") };
        let mut icons = Vec::new();
        pill_chips(&layout, &pills, 15.5, 2.0, [0.0, 0.0], 0.0..100.0, Color::TRANSPARENT, &mut Vec::new(), &mut icons);
        let [(_, px, [ix, _], _)] = icons[..] else { panic!("{text:?}: one icon") };
        let edge = (pm.margin + pm.pad) * 2.0;
        assert!(ix >= chip[0] && ix + px as f32 <= chip[0] + chip[2] + 0.5, "{text:?}: the icon is inside its chip");
        if rtl {
            assert!((600.0 - (ix + px as f32) - edge).abs() <= 1.0, "{text:?}: the icon stands {edge} in from the right margin, at {ix}");
            assert!(chip[0] >= word_x1 - 0.5, "{text:?}: the chip is right of the text, {chip:?} after {word_x1}");
        } else {
            assert!((ix - edge).abs() <= 1.0, "{text:?}: the icon stands {edge} in from the left margin, at {ix}");
            assert!(chip[0] + chip[2] <= word_x0 + 0.5, "{text:?}: the chip is left of the text");
        }
    }
}

#[test]
fn a_hanging_task_line_measured_at_no_width_lays_out_every_character() {
    use crate::edit::Block;
    let mut ts = TextSystem::default();
    let text = "- [ ] Oat milk\n";
    let pills = [PillChip { start: 0, end: 6, icon: None, bg: None, fg: None }];
    let style = TextStyle::new(15.5, 400.0);
    for scale in [1.0, 2.0] {
        let layout = ts.layout_blocks_inner(text, &[], &[Block::Hanging { marker: 6, level: 0, quote: false }], &pills, style, scale, Some(0.0), Alignment::Start, Wrapping::Anywhere, false);
        let laid: usize = layout.lines().map(|line| line.text_range()).filter(|r| r.start <= r.end).map(|r| r.len()).sum();
        assert_eq!(laid, text.len(), "every character is on a line at scale {scale}");
    }
}


#[test]
fn a_quoted_hanging_paragraph_steps_in_from_the_quotes_indent_and_wraps_under_its_text() {
    use crate::edit::Block;
    let mut ts = TextSystem::default();
    let style = TextStyle::new(14.0, 400.0);
    for (words, rtl) in [("hello world ".repeat(8), false), ("שלום עולם ".repeat(8), true)] {
        let item = format!("1. {}", words.trim_end());
        let mut from_start = |block: Block| -> Vec<f32> {
            let layout = ts.layout_blocks(&item, &[], &[block], &[], style, 2.0, Some(240.0), Alignment::Start);
            layout
                .lines()
                .map(|l| {
                    let m = l.metrics();
                    if rtl { 240.0 - (m.inline_min_coord + m.offset + m.advance) } else { m.inline_min_coord + m.offset }
                })
                .collect()
        };
        let quote = from_start(Block::Quote)[0];
        assert!(quote > 10.0, "{item:?}: a quote starts in from the side it reads from, by {quote}");
        for level in 0..=2u8 {
            let outside = from_start(Block::Hanging { marker: 3, level, quote: false });
            let inside = from_start(Block::Hanging { marker: 3, level, quote: true });
            assert!(outside.len() > 2 && inside.len() >= outside.len(), "{item:?} at level {level} wraps in and out of a quote: {outside:?} and {inside:?}");
            for (i, (o, q)) in outside.iter().zip(&inside).enumerate() {
                assert!((q - o - quote).abs() < 0.5, "{item:?} at level {level}: line {i} in a quote starts the quote's {quote} further in: {inside:?} against {outside:?}");
            }
        }
    }
}

#[test]
fn a_quoted_bullet_steps_in_from_the_quotes_indent_and_wraps_under_its_text() {
    use crate::edit::Block;
    let mut ts = TextSystem::default();
    let style = TextStyle::new(14.0, 400.0);
    for (words, rtl) in [("hello world ".repeat(8), false), ("שלום עולם ".repeat(8), true)] {
        let words = words.trim_end();
        let mut from_start = |block: Block| -> Vec<f32> {
            let layout = ts.layout_blocks(words, &[], &[block], &[], style, 2.0, Some(240.0), Alignment::Start);
            layout
                .lines()
                .map(|l| {
                    let m = l.metrics();
                    if rtl { 240.0 - (m.inline_min_coord + m.offset + m.advance) } else { m.inline_min_coord + m.offset }
                })
                .collect()
        };
        let quote = from_start(Block::Quote)[0];
        for level in 0..=2u8 {
            let outside = from_start(Block::Bullet { level, quote: false });
            let inside = from_start(Block::Bullet { level, quote: true });
            assert!(outside.len() > 2 && inside.len() >= outside.len(), "{words:?} at level {level} wraps in and out of a quote: {outside:?} and {inside:?}");
            for (i, (o, q)) in outside.iter().zip(&inside).enumerate() {
                assert!((q - o - quote).abs() < 0.5, "{words:?} at level {level}: line {i} in a quote starts the quote's {quote} further in: {inside:?} against {outside:?}");
            }
        }
    }
}
